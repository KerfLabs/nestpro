//! ERP / MES on screen: the settings page (Settings → ERP / MES: token, who
//! makes work orders, which operations run on which machine), the window
//! (the planner's work orders and the work still waiting for one), opening
//! work as a job, and sending the nest back. The talking is in integrations/.

use crate::integrations::{self as erp, Connector, ErpJob, Group, GroupFilter, ImportPart, Integration, Provider, SendResult, StationInfo, Task, Units, WorkOrderRef, WorkOrders};
use crate::nest::{Part, Shape};
use crate::settings_ui::{card, row, tile};
use crate::theme::th;
use crate::ui_parts::*;
use crate::{App, Job};
use eframe::egui;
use egui::RichText;
use std::collections::BTreeSet;

/// A work package with its drawings, ready to become a job.
pub struct Imported {
    pub material: erp::Ref,
    pub operation: erp::Ref,
    /// The work-order step it was opened from (planner's work orders).
    pub work_order: Option<WorkOrderRef>,
    /// The machine that step is scheduled on, if any.
    pub equipment: erp::Ref,
    pub info: Option<erp::MaterialInfo>,
    /// Each part to make and its drawing's text (or why it couldn't be read).
    pub parts: Vec<(ImportPart, Result<String, String>)>,
    pub notes: Vec<String>,
}

/// Where a send's answer goes back to.
struct SendFor {
    job: usize,
    name: String,
}

/// The window's two lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErpTab {
    /// Work orders (planner's) on this shop's operations.
    WorkOrders,
    /// Job operations not on a work order yet.
    Waiting,
}

pub struct ErpUi {
    pub open: bool,
    pub provider: Provider,
    pub tab: ErpTab,
    /// Only work for this machine of the app's (empty = all of ours).
    pub machine_filter: String,
    /// Also list finished and cancelled work orders.
    pub show_finished: bool,
    /// Settings → ERP / MES: the system's operations, as last loaded.
    pub station_info: Vec<StationInfo>,
    pub stations_task: Option<Task<Vec<StationInfo>>>,
    pub stations_msg: String,
    /// A token typed for this session only (never saved as text).
    pub session_token: String,
    /// The settings page's token box.
    pub token_input: String,
    pub check: Option<Task<String>>,
    pub check_msg: String,
    pub filter: GroupFilter,
    pub groups: Vec<Group>,
    pub loaded: bool,
    pub load: Option<Task<Vec<Group>>>,
    pub sel_group: usize,
    /// Job operations ticked in the selected group: (job id, job operation id).
    pub picked: BTreeSet<(String, String)>,
    pub import: Option<Task<Imported>>,
    send: Option<(Task<SendResult>, SendFor)>,
    pub msg: String,
    pub msg_ok: bool,
    /// Details of the last import or send.
    pub notes: Vec<String>,
}

impl Default for ErpUi {
    fn default() -> Self {
        ErpUi {
            open: false,
            provider: Provider::Fulcrum,
            tab: ErpTab::WorkOrders,
            machine_filter: String::new(),
            show_finished: false,
            station_info: vec![],
            stations_task: None,
            stations_msg: String::new(),
            session_token: String::new(),
            token_input: String::new(),
            check: None,
            check_msg: String::new(),
            // Everything on sheet and plate: each tab picks what it lists.
            filter: GroupFilter { need_nesting: false, flat_only: true, search: String::new() },
            groups: vec![],
            loaded: false,
            load: None,
            sel_group: 0,
            picked: BTreeSet::new(),
            import: None,
            send: None,
            msg: String::new(),
            msg_ok: true,
            notes: vec![],
        }
    }
}

/// A layer's role in an ERP drawing, from its name: marking layers are
/// marked, drafting layers (dimensions, notes, title block) left out,
/// everything else cut. (Estimate: common CAD layer names.)
pub fn guess_role(layer: &str) -> crate::dxf::LayerRole {
    use crate::dxf::LayerRole::*;
    let l = layer.to_ascii_uppercase();
    let has = |ws: &[&str]| ws.iter().any(|w| l.contains(w));
    if has(&["MARK", "ETCH", "SCRIBE", "ENGRAV", "BEND"]) {
        Mark
    } else if has(&["DIM", "TEXT", "NOTE", "TITLE", "BORDER", "FRAME", "CENTER", "CENTRE", "HIDDEN", "DEFPOINTS", "ANNO", "HATCH", "VIEWPORT"]) {
        Ignore
    } else {
        Cut
    }
}

/// Fetch the cut list and every drawing (runs on the background thread).
fn fetch(c: Box<dyn Connector>, group: Group, ops: Vec<(String, String, String)>) -> Result<Imported, String> {
    let wp = c.work_package(&ops)?;
    let info = wp.parts.iter().find_map(|p| p.material.clone());
    let (plan, notes) = erp::plan_import(&wp, None);
    let mut cache: std::collections::HashMap<String, Result<String, String>> = Default::default();
    let mut parts = vec![];
    for p in plan {
        let text = cache.entry(p.drawing.id.clone()).or_insert_with(|| c.download(&p.drawing).map(|b| String::from_utf8_lossy(&b).into_owned())).clone();
        parts.push((p, text));
    }
    Ok(Imported { material: group.material, operation: group.operation, work_order: None, equipment: Default::default(), info, parts, notes })
}

/// Fetch a work-order step's cut list and drawings (F23): the job
/// operations the step runs, as the planner grouped them.
fn fetch_step(c: Box<dyn Connector>, group: Group, wo: erp::Ref, step_id: String) -> Result<Imported, String> {
    let steps = c.work_order_steps(&wo.id)?;
    let step = steps.into_iter().find(|s| s.id == step_id).ok_or_else(|| format!("Work order {} no longer has that step — refresh the list.", wo.name))?;
    if step.job_ops.is_empty() {
        return Err(format!("Work order {} runs no job operations on {} — nothing to nest.", wo.name, group.operation.name));
    }
    let ops: Vec<(String, String, String)> = step.job_ops.iter().map(|(j, _, o, itm)| (j.clone(), o.clone(), itm.clone())).collect();
    let wp = c.work_package(&ops)?;
    let info = wp.parts.iter().find_map(|p| p.material.clone());
    let (plan, notes) = erp::plan_import(&wp, Some(&step.id));
    let mut cache: std::collections::HashMap<String, Result<String, String>> = Default::default();
    let mut parts = vec![];
    for p in plan {
        let text = cache.entry(p.drawing.id.clone()).or_insert_with(|| c.download(&p.drawing).map(|b| String::from_utf8_lossy(&b).into_owned())).clone();
        parts.push((p, text));
    }
    let operation = if step.operation.id.is_empty() { group.operation } else { step.operation.clone() };
    Ok(Imported { material: group.material, operation, work_order: Some(WorkOrderRef { id: wo.id, op_id: step.id, name: wo.name }), equipment: step.equipment, info, parts, notes })
}

/// Turn a drawing's text into a part for `ip`, in mm.
pub fn part_from(ip: &ImportPart, text: &str, drawing_units: Units, color: usize) -> Result<(Part, Vec<String>), String> {
    // In mm: the file's own unit, else the integration's drawing unit.
    let d = crate::dxf::read_in(text, drawing_units.mm())?;
    let roles: std::collections::BTreeMap<String, crate::dxf::LayerRole> = d.layers().keys().map(|l| (l.clone(), guess_role(l))).collect();
    let (g, rep) = crate::dxf::build(&d, &roles)?;
    let mut notes = vec![];
    let ignored: Vec<&String> = roles.iter().filter(|(_, r)| **r == crate::dxf::LayerRole::Ignore).map(|(l, _)| l).collect();
    if !ignored.is_empty() {
        notes.push(format!("{}: layer(s) {} left out", ip.name, ignored.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    }
    if rep.open_pieces > 0 {
        notes.push(format!("{}: {} open piece(s) left out", ip.name, rep.open_pieces));
    }
    if rep.outside_loops > 0 {
        notes.push(format!("{}: {} outline(s) outside the part left out", ip.name, rep.outside_loops));
    }
    let part = Part {
        name: crate::filename::part_reference("", &ip.name, ""),
        shape: Shape::Custom,
        w: g.w,
        h: g.h,
        qty: ip.qty(),
        rotate: true,
        color,
        tech: None,
        texts: g.texts.clone(),
        group: String::new(),
        custom: Some(std::sync::Arc::new(g)),
        rots: Default::default(),
        erp: ip.demands.clone(),
    };
    Ok((part, notes))
}

impl App {
    fn integ(&self) -> Integration {
        self.cfg.integration(self.erp.provider)
    }

    /// The connector, signed in (token read on the calling thread: call it
    /// inside a task, DPAPI may take a moment).
    fn connector_for(integ: &Integration, session: &str) -> Result<Box<dyn Connector>, String> {
        erp::check_url(&integ.url())?;
        let tok = erp::token(integ, session)?;
        Ok(erp::connector(integ, tok))
    }

    pub fn open_erp(&mut self, p: Provider) {
        if self.erp.provider != p || !self.erp.loaded {
            self.erp.tab = if self.cfg.integration(p).work_orders == WorkOrders::Planner { ErpTab::WorkOrders } else { ErpTab::Waiting };
        }
        self.erp.provider = p;
        self.erp.open = true;
        if !self.erp.loaded && self.erp.load.is_none() {
            self.erp_refresh();
        }
    }

    pub fn erp_refresh(&mut self) {
        let (integ, session, f) = (self.integ(), self.erp.session_token.clone(), self.erp.filter.clone());
        let name = integ.provider.name();
        self.erp.load = Some(Task::spawn(format!("Asking {name} for work to nest…"), move || Self::connector_for(&integ, &session)?.groups(&f)));
    }

    /// Settings → ERP / MES: load the system's operations to map.
    fn erp_load_stations(&mut self) {
        let (integ, session) = (self.integ(), self.erp.session_token.clone());
        self.erp.stations_msg = "Loading…".into();
        self.erp.stations_task = Some(Task::spawn("stations", move || Self::connector_for(&integ, &session)?.stations()));
    }

    /// The job already open for a work-order step, if any.
    pub fn erp_job_for_step(&self, step_id: &str) -> Option<usize> {
        let on = |e: &Option<ErpJob>| e.as_ref().and_then(|l| l.work_order.as_ref()).is_some_and(|w| w.op_id == step_id);
        if on(&self.erp_job) {
            return Some(self.cur_job);
        }
        self.jobs.iter().position(|j| on(&j.erp))
    }

    /// Open a planner's work-order step as a job (or switch to it when it is
    /// open already).
    pub fn erp_open_step(&mut self, group: usize, nest: usize) {
        let Some(g) = self.erp.groups.get(group).cloned() else { return };
        let Some(n) = g.nests.get(nest).cloned() else { return };
        if let Some(i) = self.erp_job_for_step(&n.op_id) {
            self.open_job(i);
            self.erp.msg = format!("Work order {} is open already as {} — switched to it.", n.work_order.name, self.job_name);
            self.erp.msg_ok = true;
            return;
        }
        let (integ, session) = (self.integ(), self.erp.session_token.clone());
        let what = format!("Fetching work order {}: its parts and drawings…", n.work_order.name);
        self.erp.import = Some(Task::spawn(what, move || fetch_step(Self::connector_for(&integ, &session)?, g, n.work_order, n.op_id)));
    }

    fn erp_test(&mut self) {
        let (integ, session) = (self.integ(), self.erp.session_token.clone());
        self.erp.check_msg = "Testing…".into();
        self.erp.check = Some(Task::spawn("test", move || Self::connector_for(&integ, &session)?.check()));
    }

    fn erp_import(&mut self) {
        let Some(g) = self.erp.groups.get(self.erp.sel_group).cloned() else { return };
        let ops: Vec<(String, String, String)> = g.open.iter().filter(|o| self.erp.picked.contains(&(o.job.id.clone(), o.op_id.clone()))).map(|o| (o.job.id.clone(), o.op_id.clone(), o.item_to_make_id.clone())).collect();
        if ops.is_empty() {
            return;
        }
        let (integ, session) = (self.integ(), self.erp.session_token.clone());
        let n = ops.len();
        self.erp.import = Some(Task::spawn(format!("Fetching the cut list and drawings for {n} job operation(s)…"), move || fetch(Self::connector_for(&integ, &session)?, g, ops)));
    }

    /// Make a job from an import and open it.
    fn erp_make_job(&mut self, im: Imported) {
        let integ = self.integ();
        let provider = integ.provider;
        let mut spec = crate::engine::with_prefs(&self.spec, &self.cfg);
        let mut notes = im.notes;
        if let Some(m) = &im.info {
            match erp::pick_material(&self.cfg, m) {
                Some(k) => spec.material = k,
                None => notes.push(format!("Material \"{}\" isn't one of the app's — kept {}; add it as an alias in Settings → Materials", if m.material_name.is_empty() { &m.name } else { &m.material_name }, crate::cnc::MATERIALS.get(spec.material).copied().unwrap_or("?"))),
            }
            if let Some(t) = m.thickness_mm {
                spec.thickness = (t * 1000.0).round() / 1000.0;
            }
        }
        let mut parts = vec![];
        for (ip, text) in &im.parts {
            let made = text.clone().and_then(|t| part_from(ip, &t, integ.drawing_units, self.color_seq + parts.len()));
            match made {
                Ok((p, n)) => {
                    notes.extend(n);
                    parts.push(p);
                }
                Err(e) => notes.push(format!("{} ({}): {e}", ip.name, ip.drawing.file_name)),
            }
        }
        if parts.is_empty() {
            self.erp.msg = "Nothing imported: no drawing could be read.".into();
            self.erp.msg_ok = false;
            self.erp.notes = notes;
            return;
        }
        self.color_seq += parts.len();
        let base = match (&im.work_order, im.material.name.is_empty()) {
            (Some(w), false) => format!("{} {}", w.name, im.material.name),
            (Some(w), true) => w.name.clone(),
            (None, false) => format!("{} {}", im.material.name, im.operation.name),
            (None, true) => format!("{} job", provider.name()),
        };
        let name = self.unique_job_name(base.trim()).unwrap_or(base);
        let link = ErpJob { provider, material_id: im.material.id.clone(), material_name: im.material.name.clone(), operation_id: im.operation.id.clone(), operation_name: im.operation.name.clone(), work_order: im.work_order.clone(), sent: None, sheet_item: String::new() };
        // The machine the work runs on (F21): the scheduled equipment's, else the operation's.
        let mut machine_note = String::new();
        if let Some(m) = integ.machine_for(&im.operation.id, &im.equipment.id) {
            match self.cfg.machines.iter().position(|x| x.name == m) {
                Some(k) => {
                    // Applied (post, technology) with the next frame's settings pass.
                    self.cfg.active_machine = k;
                    machine_note = format!(" Machine: {m}.");
                }
                None => notes.push(format!("{} maps to machine \"{m}\", which isn't in Settings → Machines any more", if im.equipment.name.is_empty() { &im.operation.name } else { &im.equipment.name })),
            }
        }
        let (n, qty): (usize, u32) = (parts.len(), parts.iter().map(|p| p.qty).sum());
        self.store_job();
        self.jobs.push(Job {
            week: provider.name().into(),
            name,
            parts,
            spec,
            result: Default::default(),
            stale: true,
            lib_id: None,
            cut_lines: vec![],
            manual_cuts: vec![],
            cam_changed: None,
            held: Default::default(),
            erp: Some(link),
            file: None,
            saved: None,
        });
        let i = self.jobs.len() - 1;
        self.open_job(i);
        self.job_path = None;
        self.erp.picked.clear();
        let from = im.work_order.as_ref().map_or(String::new(), |w| format!(" from work order {}", w.name));
        self.erp.msg = format!("Made job {}{from}: {n} part kind(s), {qty} parts, {} {} mm.{machine_note} Set the sheet size, then Nest all.", self.job_name, crate::cnc::MATERIALS.get(self.spec.material).copied().unwrap_or(""), self.spec.thickness);
        self.erp.msg_ok = true;
        self.erp.notes = notes;
        self.status = self.erp.msg.clone();
    }

    /// The open job has parts for an ERP order and sheets to report.
    pub fn erp_can_send(&self) -> bool {
        !self.result.sheets.is_empty() && self.parts.iter().any(|p| !p.erp.is_empty())
    }

    pub fn erp_send(&mut self) {
        if !self.erp_can_send() {
            return;
        }
        self.store_job();
        let provider = self.parts.iter().find_map(|p| p.erp.first()).map_or(self.erp.provider, |d| d.provider);
        let link = self.erp_job.clone().unwrap_or(ErpJob { provider, material_id: String::new(), material_name: String::new(), operation_id: String::new(), operation_name: String::new(), work_order: None, sent: None, sheet_item: String::new() });
        let seconds: Vec<f64> = self.plans.iter().map(|p| p.seconds).collect();
        let report = erp::nest_report(provider, &self.job_name, &self.cfg.machine().name, &self.parts, &self.result.sheets, &self.result.formats, &self.sheet_groups(), &seconds, &self.spec);
        let ops = erp::job_ops(&self.parts, provider);
        let integ = self.cfg.integration(provider);
        let session = self.erp.session_token.clone();
        let name = self.job_name.clone();
        self.erp.notes = report.notes.clone();
        if self.stale {
            self.erp.notes.insert(0, "The parts changed since the last nesting — the sheets as they are now were sent.".into());
        }
        let what = format!("Sending {} sheet(s) to {}…", report.sheets.iter().map(|s| s.count).sum::<u32>(), provider.name());
        let planner = integ.work_orders == WorkOrders::Planner;
        let task = Task::spawn(what, move || {
            let c = Self::connector_for(&integ, &session)?;
            let mut link = link;
            let mut found = vec![];
            // F22: with planner-made work orders the app never makes one; it
            // finds the planner's work order holding these job operations.
            if planner && link.work_order.is_none() {
                let a = erp::find_work_order(c.as_ref(), &link.operation_id, &ops)?;
                match a.found {
                    Some(w) => {
                        found.push(format!("Found the planner's work order {} for this job.", w.name));
                        if a.missing > 0 {
                            found.push(format!("{} of the job's {} job operation(s) aren't on it; their parts are still in the nest.", a.missing, ops.len()));
                        }
                        link.work_order = Some(w);
                    }
                    None if a.several.is_empty() => return Err(format!("No work order holds this job's work yet. The planner makes it in {}; send again once it exists.", integ.provider.name())),
                    None => return Err(format!("This job's work is on several work orders ({}). Open each from the Work orders list and nest it on its own.", a.several.join(", "))),
                }
            }
            let mut r = c.send_nest(&name, &link, &ops, &report)?;
            r.issues.splice(0..0, found);
            Ok(r)
        });
        self.erp.send = Some((task, SendFor { job: self.cur_job, name: self.job_name.clone() }));
    }

    fn erp_sent(&mut self, to: SendFor, r: Result<SendResult, String>) {
        match r {
            Ok(r) => {
                let link = if to.job == self.cur_job && to.name == self.job_name { self.erp_job.as_mut() } else { self.jobs.get_mut(to.job).filter(|j| j.name == to.name).and_then(|j| j.erp.as_mut()) };
                let provider = self.erp.provider;
                if let Some(l) = link {
                    if r.work_order.is_some() {
                        l.work_order = r.work_order.clone();
                    }
                    if r.error.is_none() {
                        l.sent = Some(erp::now());
                    }
                } else if to.job == self.cur_job && to.name == self.job_name {
                    self.erp_job = Some(ErpJob { provider, material_id: String::new(), material_name: String::new(), operation_id: String::new(), operation_name: String::new(), work_order: r.work_order.clone(), sent: r.error.is_none().then(erp::now), sheet_item: String::new() });
                }
                self.store_job();
                let wo = r.work_order.as_ref().map_or(String::new(), |w| if w.name.is_empty() { w.id.clone() } else { w.name.clone() });
                match &r.error {
                    None => {
                        self.erp.msg = format!("{} {to_name}'s nest on work order {wo}{}.", if r.created { "Made a work order and stored" } else { "Stored" }, if r.issues.is_empty() { String::new() } else { format!(" — {} remark(s)", r.issues.len()) }, to_name = to.name);
                        self.erp.msg_ok = true;
                    }
                    Some(e) => {
                        self.erp.msg = e.clone();
                        self.erp.msg_ok = false;
                    }
                }
                self.erp.notes.extend(r.issues);
                self.status = self.erp.msg.clone();
                // The group list changes once work is nested.
                self.erp.loaded = false;
            }
            Err(e) => {
                self.erp.msg = e;
                self.erp.msg_ok = false;
                self.status = self.erp.msg.clone();
            }
        }
    }

    /// Collect finished background requests (every frame).
    fn erp_poll(&mut self, ctx: &egui::Context) {
        let mut busy = false;
        if let Some(t) = &self.erp.check {
            busy = true;
            if let Some(r) = t.poll() {
                self.erp.check_msg = match r {
                    Ok(s) => format!("✔ {s}"),
                    Err(e) => format!("✖ {e}"),
                };
                self.erp.check = None;
            }
        }
        if let Some(t) = &self.erp.stations_task {
            busy = true;
            if let Some(r) = t.poll() {
                self.erp.stations_task = None;
                match r {
                    Ok(v) => {
                        self.erp.stations_msg = format!("✔ {} operation(s), {} marked nestable.", v.len(), v.iter().filter(|s| s.nestable).count());
                        self.erp.station_info = v;
                    }
                    Err(e) => self.erp.stations_msg = format!("✖ {e}"),
                }
            }
        }
        if let Some(t) = &self.erp.load {
            busy = true;
            if let Some(r) = t.poll() {
                self.erp.load = None;
                match r {
                    Ok(g) => {
                        self.erp.groups = g;
                        self.erp.loaded = true;
                        self.erp.sel_group = self.erp.sel_group.min(self.erp.groups.len().saturating_sub(1));
                        let keep: BTreeSet<(String, String)> = self.erp.groups.iter().flat_map(|g| g.open.iter().map(|o| (o.job.id.clone(), o.op_id.clone()))).collect();
                        self.erp.picked.retain(|k| keep.contains(k));
                    }
                    Err(e) => {
                        self.erp.msg = e;
                        self.erp.msg_ok = false;
                        self.erp.loaded = true;
                    }
                }
            }
        }
        if let Some(t) = &self.erp.import {
            busy = true;
            if let Some(r) = t.poll() {
                self.erp.import = None;
                match r {
                    Ok(im) => self.erp_make_job(im),
                    Err(e) => {
                        self.erp.msg = e;
                        self.erp.msg_ok = false;
                    }
                }
            }
        }
        if let Some((t, _)) = &self.erp.send {
            busy = true;
            if let Some(r) = t.poll() {
                let (_, to) = self.erp.send.take().expect("checked");
                self.erp_sent(to, r);
            }
        }
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    // ---------------------------------------------------------------------
    // Ribbon, window, settings page.
    // ---------------------------------------------------------------------

    /// Jobs ribbon group: one button per system in use, and Send nest.
    pub fn ribbon_erp(&mut self, ui: &mut egui::Ui) {
        let on: Vec<Provider> = self.cfg.integrations.iter().filter(|i| i.enabled).map(|i| i.provider).collect();
        if on.is_empty() {
            return;
        }
        ribbon_group(ui, "ERP / MES", |ui| {
            for p in on {
                if ribbon_button(ui, Icon::WorkOrders, &format!("{}\njobs", p.name()), self.erp.open && self.erp.provider == p, true).on_hover_text(format!("Work waiting to be nested in {} — import it as a job", p.name())).clicked() {
                    if self.erp.open && self.erp.provider == p {
                        self.erp.open = false;
                    } else {
                        self.open_erp(p);
                    }
                }
            }
            let tip = if self.erp_can_send() { "Store the open job's nest on its work order (made the first time)" } else { "Import a job from the ERP and nest it first" };
            if ribbon_button(ui, Icon::Export, "Send\nnest", false, self.erp_can_send() && self.erp.send.is_none()).on_hover_text(tip).clicked() {
                self.erp_send();
                self.erp.open = true;
            }
        });
    }

    pub fn erp_window(&mut self, ctx: &egui::Context) {
        self.erp_poll(ctx);
        if !self.erp.open {
            return;
        }
        let mut open = true;
        let name = self.erp.provider.name();
        egui::Window::new(RichText::new(format!("{name} — work to nest")).size(16.0).strong())
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(crate::ui_parts::dialog_pos(ctx))
            .id(egui::Id::unique("erp_window"))
            .open(&mut open)
            .default_size([1080.0, 640.0])
            .min_size([760.0, 420.0])
            .collapsible(false)
            .show(ctx, |ui| self.erp_window_body(ui));
        if !open {
            self.erp.open = false;
        }
    }

    /// Work on this operation is listed: one of ours (F20), and the
    /// machine chosen at the top of the window, if any.
    fn erp_shows(&self, integ: &Integration, op_id: &str) -> bool {
        integ.ours(op_id) && (self.erp.machine_filter.is_empty() || integ.station_machine(op_id) == Some(self.erp.machine_filter.as_str()))
    }

    /// Work-order steps listed: (group, nest) indexes.
    fn erp_wo_rows(&self, integ: &Integration) -> Vec<(usize, usize)> {
        let mut v = vec![];
        for (gi, g) in self.erp.groups.iter().enumerate() {
            if !self.erp_shows(integ, &g.operation.id) {
                continue;
            }
            for (ni, n) in g.nests.iter().enumerate() {
                if self.erp.show_finished || !n.finished() {
                    v.push((gi, ni));
                }
            }
        }
        v
    }

    /// Groups with work waiting for a work order.
    fn erp_waiting_groups(&self, integ: &Integration) -> Vec<usize> {
        (0..self.erp.groups.len()).filter(|&k| !self.erp.groups[k].open.is_empty() && self.erp_shows(integ, &self.erp.groups[k].operation.id)).collect()
    }

    fn erp_window_body(&mut self, ui: &mut egui::Ui) {
        let t = th();
        let name = self.erp.provider.name();
        let integ = self.integ();
        let rows = self.erp_wo_rows(&integ);
        let waiting = self.erp_waiting_groups(&integ);
        let n_wait: usize = waiting.iter().map(|&k| self.erp.groups[k].open.len()).sum();
        // Tabs, machine, filters.
        let mut refresh = false;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.erp.tab, ErpTab::WorkOrders, RichText::new(format!("Work orders ({})", rows.len())).strong());
            ui.selectable_value(&mut self.erp.tab, ErpTab::Waiting, RichText::new(format!("Waiting for a work order ({n_wait})")).strong());
            ui.separator();
            let mut machines: Vec<String> = integ.stations.iter().chain(integ.equipment.iter()).map(|s| s.machine.clone()).collect();
            machines.sort();
            machines.dedup();
            if !machines.is_empty() {
                let shown = if self.erp.machine_filter.is_empty() { "All my machines".to_string() } else { self.erp.machine_filter.clone() };
                egui::ComboBox::from_id_salt("erp_machine").selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.erp.machine_filter, String::new(), "All my machines");
                    for m in &machines {
                        ui.selectable_value(&mut self.erp.machine_filter, m.clone(), m);
                    }
                });
            }
            let r = ui.add(egui::TextEdit::singleline(&mut self.erp.filter.search).desired_width(220.0).hint_text("Search job, customer, item, work order…"));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                refresh = true;
            }
            refresh |= ui.checkbox(&mut self.erp.filter.flat_only, "Sheet & plate").changed();
            if self.erp.tab == ErpTab::WorkOrders {
                ui.checkbox(&mut self.erp.show_finished, "Finished too");
            }
            if ui.add_enabled(self.erp.load.is_none(), egui::Button::new("⟳ Refresh")).clicked() {
                refresh = true;
            }
            if ui.button("Settings…").on_hover_text("Settings → ERP / MES: token, work stations, units").clicked() {
                self.open_settings(crate::settings_ui::Section::Integrations);
            }
        });
        let tasks: Vec<&String> = [self.erp.load.as_ref().map(|t| &t.what), self.erp.import.as_ref().map(|t| &t.what), self.erp.send.as_ref().map(|t| &t.0.what)].into_iter().flatten().collect();
        if !tasks.is_empty() {
            ui.horizontal(|ui| {
                for task in tasks {
                    ui.add(egui::Spinner::new());
                    ui.label(RichText::new(task).color(t.text_dim));
                }
            });
        }
        if refresh && self.erp.load.is_none() {
            self.erp_refresh();
        }
        if !integ.is_mapped() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Showing every operation in {name}. Map the operations your machines run (lasers…) to see only their work.")).size(11.5).color(t.text_dim));
                if ui.small_button("Map work stations…").clicked() {
                    self.open_settings(crate::settings_ui::Section::Integrations);
                }
            });
        }
        if !self.erp.msg.is_empty() {
            ui.add_space(4.0);
            let (bg, fg) = if self.erp.msg_ok { (t.ok_bg, t.ok_fg) } else { (t.warn_bg, t.warn_fg) };
            egui::Frame::new().fill(bg).corner_radius(6).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&self.erp.msg).color(fg));
                if !self.erp.notes.is_empty() {
                    egui::CollapsingHeader::new(RichText::new(format!("{} detail(s)", self.erp.notes.len())).size(11.5).color(fg)).id_salt("erp_notes").show(ui, |ui| {
                        for n in &self.erp.notes {
                            ui.label(RichText::new(format!("• {n}")).size(11.5).color(fg));
                        }
                    });
                }
            });
        }
        ui.add_space(6.0);
        self.erp_job_card(ui);
        ui.add_space(6.0);
        if self.erp.groups.is_empty() && (self.erp.load.is_some() || !self.erp.loaded) {
            ui.label(RichText::new(format!("Loading from {name}…")).color(t.text_dim));
            return;
        }
        match self.erp.tab {
            ErpTab::WorkOrders => self.erp_work_orders(ui, &integ, &rows),
            ErpTab::Waiting => self.erp_waiting(ui, &integ, &waiting),
        }
    }

    /// The planner's work orders on this shop's operations.
    fn erp_work_orders(&mut self, ui: &mut egui::Ui, integ: &Integration, rows: &[(usize, usize)]) {
        let t = th();
        if rows.is_empty() {
            let who = if integ.work_orders == WorkOrders::Planner { format!(" The planner makes them in {}.", integ.provider.name()) } else { String::new() };
            ui.label(RichText::new(format!("No work orders on these operations yet.{who}")).color(t.text_dim));
            return;
        }
        let mut open: Option<(usize, usize)> = None;
        egui::ScrollArea::vertical().id_salt("erp_wos").auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("erp_wo_grid").striped(true).spacing([16.0, 7.0]).show(ui, |ui| {
                for c in ["Work order", "Status", "Step", "Material", "Operation", "Machine", "Nest", ""] {
                    ui.label(RichText::new(c).size(11.5).strong().color(t.text_dim));
                }
                ui.end_row();
                for &(gi, ni) in rows {
                    let g = &self.erp.groups[gi];
                    let n = &g.nests[ni];
                    ui.label(RichText::new(&n.work_order.name).strong());
                    ui.label(status_label(&n.status));
                    ui.label(&n.op_status);
                    ui.label(&g.material.name);
                    ui.label(&g.operation.name);
                    ui.label(integ.station_machine(&g.operation.id).unwrap_or("—"));
                    if n.nested {
                        ui.label(format!("{} sheet(s), {} parts, {}:{:02}", n.sheets, n.parts, (n.machine_s / 60.0) as u64, (n.machine_s % 60.0) as u64));
                    } else {
                        chip(ui, "to nest", t.accent_soft, t.accent_fg);
                    }
                    let is_open = self.erp_job_for_step(&n.op_id).is_some();
                    let label = if is_open { "Switch to it" } else if n.nested { "Open (renest)" } else { "Open as job" };
                    let tip = if n.nested && !is_open { "It has a nest already: sending again replaces it" } else { "Fetch its parts and drawings and make a job of them" };
                    if ui.add_enabled(self.erp.import.is_none(), egui::Button::new(label)).on_hover_text(tip).clicked() {
                        open = Some((gi, ni));
                    }
                    ui.end_row();
                }
            });
        });
        if let Some((gi, ni)) = open {
            self.erp_open_step(gi, ni);
        }
    }

    /// Job operations not on a work order yet, by material and operation.
    fn erp_waiting(&mut self, ui: &mut egui::Ui, integ: &Integration, waiting: &[usize]) {
        let t = th();
        if waiting.is_empty() {
            ui.label(RichText::new("Nothing waiting for a work order on these operations.").color(t.text_dim));
            return;
        }
        if !waiting.contains(&self.erp.sel_group) {
            self.erp.sel_group = waiting[0];
            self.erp.picked.clear();
        }
        if integ.work_orders == WorkOrders::Planner {
            ui.label(RichText::new(format!("Not on a work order yet. You can import it to plan a nest; Send nest stores it on the work order once the planner makes it in {}.", integ.provider.name())).size(11.5).color(t.text_dim));
        }
        let h = ui.available_height();
        ui.horizontal_top(|ui| {
            // Groups: one material on one operation.
            ui.vertical(|ui| {
                ui.set_width(290.0);
                ui.label(RichText::new(format!("{} group(s) · material + operation", waiting.len())).size(11.0).color(t.text_dim));
                egui::ScrollArea::vertical().id_salt("erp_groups").max_height(h - 20.0).show(ui, |ui| {
                    for &k in waiting {
                        let g = &self.erp.groups[k];
                        let sel = k == self.erp.sel_group;
                        let sub = format!("{} · {} waiting{}", g.operation.name, g.open.len(), if g.earliest_start.is_empty() { String::new() } else { format!(" · from {}", erp::day(&g.earliest_start)) });
                        if group_tile(ui, sel, &g.material.name, &sub) && !sel {
                            self.erp.sel_group = k;
                            self.erp.picked.clear();
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| self.erp_group_detail(ui, h));
        });
    }

    fn erp_group_detail(&mut self, ui: &mut egui::Ui, h: f32) {
        let t = th();
        let Some(g) = self.erp.groups.get(self.erp.sel_group).cloned() else { return };
        ui.horizontal(|ui| {
            ui.label(RichText::new(&g.material.name).size(15.0).strong().color(t.text_strong));
            ui.label(RichText::new(format!("on {}", g.operation.name)).color(t.text_dim));
        });
        let keys: Vec<(String, String)> = g.open.iter().map(|o| (o.job.id.clone(), o.op_id.clone())).collect();
        let n = keys.iter().filter(|k| self.erp.picked.contains(*k)).count();
        ui.horizontal(|ui| {
            if ui.small_button("Select all").clicked() {
                self.erp.picked.extend(keys.iter().cloned());
            }
            if ui.small_button("Clear").clicked() {
                self.erp.picked.clear();
            }
            let can = n > 0 && self.erp.import.is_none();
            if ui.add_enabled(can, egui::Button::new(RichText::new(format!("Import {n} into a new job")).strong())).on_hover_text("Fetches the cut list and DXF drawings and makes a job of them; each part remembers the orders it is for").clicked() {
                self.erp_import();
            }
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt("erp_ops").max_height((h - 150.0).max(120.0)).show(ui, |ui| {
            egui::Grid::new("erp_ops_grid").striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
                for c in ["", "Job", "Item", "Qty", "Customer", "Sales order", "Due", "Start", ""] {
                    ui.label(RichText::new(c).size(11.5).strong().color(t.text_dim));
                }
                ui.end_row();
                for o in &g.open {
                    let key = (o.job.id.clone(), o.op_id.clone());
                    let mut on = self.erp.picked.contains(&key);
                    if ui.checkbox(&mut on, "").changed() {
                        if on {
                            self.erp.picked.insert(key);
                        } else {
                            self.erp.picked.remove(&key);
                        }
                    }
                    ui.label(&o.job.name);
                    ui.label(&o.item.name);
                    ui.label(if o.qty.fract() == 0.0 { format!("{}", o.qty) } else { format!("{:.2}", o.qty) });
                    ui.label(&o.customer);
                    ui.label(&o.sales_order);
                    ui.label(erp::day(&o.due));
                    ui.label(erp::day(&o.start));
                    ui.horizontal(|ui| {
                        if o.rework {
                            chip(ui, "rework", t.warn_bg, t.warn_fg);
                        }
                        if o.multi_material {
                            chip(ui, "several materials", t.warn_bg, t.warn_fg);
                        }
                    });
                    ui.end_row();
                }
            });
        });
    }

    /// The open job's link: where it came from, its work order, Send nest.
    fn erp_job_card(&mut self, ui: &mut egui::Ui) {
        let t = th();
        let from_erp = self.parts.iter().filter(|p| !p.erp.is_empty()).count();
        let planner = self.integ().work_orders == WorkOrders::Planner;
        let mut send = false;
        let mut forget = false;
        egui::Frame::new().fill(t.card).stroke(egui::Stroke::new(1.0, t.border)).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("Open job: {}", self.job_name)).strong().color(t.text_strong));
                if from_erp == 0 {
                    ui.label(RichText::new(if planner { "— not from the ERP. Open a work order below to make one." } else { "— not from the ERP. Tick job operations below and import them to make one." }).color(t.text_dim));
                    return;
                }
                let placed: usize = self.result.sheets.iter().map(|s| s.len()).sum();
                ui.label(RichText::new(format!("· {from_erp} ERP part kind(s) · {} sheet(s), {placed} placed", self.result.sheets.len())).color(t.text_dim));
                if let Some(l) = &self.erp_job {
                    if !l.material_name.is_empty() {
                        ui.label(RichText::new(format!("· {} on {}", l.material_name, l.operation_name)).color(t.text_dim));
                    }
                    match &l.work_order {
                        Some(w) => chip(ui, &format!("work order {}", if w.name.is_empty() { &w.id } else { &w.name }), t.ok_bg, t.ok_fg),
                        None if planner => chip(ui, "waiting for the planner's work order", t.accent_soft, t.accent_fg),
                        None => chip(ui, "no work order yet", t.accent_soft, t.accent_fg),
                    };
                }
            });
            if from_erp == 0 {
                return;
            }
            ui.horizontal(|ui| {
                ui.label("Sheet stock item");
                let mut item = self.erp_job.as_ref().map_or(String::new(), |l| l.sheet_item.clone());
                if ui.add(egui::TextEdit::singleline(&mut item).desired_width(160.0).hint_text("item number (optional)")).on_hover_text("The ERP's item number for the sheets cut, so it knows what stock the nest uses").changed() {
                    if let Some(l) = self.erp_job.as_mut() {
                        l.sheet_item = item;
                    }
                }
                let busy = self.erp.send.is_some();
                let has_wo = self.erp_job.as_ref().is_some_and(|l| l.work_order.is_some());
                let label = if has_wo { "Send nest to its work order" } else if planner { "Send nest — find the planner's work order" } else { "Send nest — make work order" };
                if ui.add_enabled(self.erp_can_send() && !busy, egui::Button::new(RichText::new(label).strong())).on_hover_text(if self.result.sheets.is_empty() { "Nest the job first" } else { "Stores the sheets, the parts on each and how many go to each job operation" }).clicked() {
                    send = true;
                }
                let forget_tip = if planner { "The next send looks for the planner's work order again (use if it was replaced in the ERP)" } else { "The next send makes a new work order (use if it was deleted in the ERP)" };
                if has_wo && ui.small_button("Forget work order").on_hover_text(forget_tip).clicked() {
                    forget = true;
                }
                if let Some(s) = self.erp_job.as_ref().and_then(|l| l.sent) {
                    let ago = erp::now().saturating_sub(s);
                    ui.label(RichText::new(format!("last sent {} ago", if ago < 3600 { format!("{} min", ago / 60) } else if ago < 86400 { format!("{} h", ago / 3600) } else { format!("{} days", ago / 86400) })).size(11.5).color(t.text_dim));
                }
            });
        });
        if send {
            self.erp_send();
        }
        if forget {
            if let Some(l) = self.erp_job.as_mut() {
                l.work_order = None;
                l.sent = None;
            }
            self.store_job();
        }
    }

    /// Settings → ERP / MES.
    pub fn settings_integrations(&mut self, ui: &mut egui::Ui) {
        let t = th();
        ui.label(RichText::new("ERP / MES").size(18.0).strong().color(t.text_strong));
        ui.label(RichText::new("Systems the shop runs its orders in. Pull the work waiting to be nested, nest it here, send the sheets back.").size(11.5).color(t.text_dim));
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for p in Provider::ALL {
                let i = self.cfg.integration(p);
                let sub = if !i.enabled { "off".to_string() } else if !i.token_enc.is_empty() || !self.erp.session_token.is_empty() || std::env::var(p.token_env()).is_ok_and(|v| !v.trim().is_empty()) { "token set".into() } else { "needs a token".into() };
                if tile(ui, self.erp.provider == p, i.enabled, p.name(), &sub) {
                    self.erp.provider = p;
                    self.erp.check_msg.clear();
                    self.erp.token_input.clear();
                }
            }
            ui.add_enabled_ui(false, |ui| {
                tile(ui, false, false, "More systems", "added as connectors");
            });
        });
        ui.add_space(10.0);
        let p = self.erp.provider;
        let env = p.token_env();
        let env_set = std::env::var(env).is_ok_and(|v| !v.trim().is_empty());
        let mut test = false;
        card(ui, p.name(), p.about(), |ui| {
            let i = self.cfg.integration_mut(p);
            row(ui, "Use it", |ui| {
                ui.checkbox(&mut i.enabled, "Show in the Jobs ribbon");
            });
            row(ui, "API address", |ui| {
                ui.add(egui::TextEdit::singleline(&mut i.base_url).desired_width(320.0).hint_text(p.default_url()));
            });
            row(ui, "API token", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.erp.token_input).password(true).desired_width(240.0).hint_text(if i.token_enc.is_empty() { "paste the token" } else { "remembered — paste to change" }));
                let has = !self.erp.token_input.trim().is_empty();
                if cfg!(windows) {
                    if ui.add_enabled(has, egui::Button::new("Remember")).on_hover_text("Saved encrypted for your Windows account on this PC").clicked() {
                        match crate::settings::protect(self.erp.token_input.trim()) {
                            Ok(enc) => {
                                i.token_enc = enc;
                                self.erp.session_token = self.erp.token_input.trim().to_string();
                                self.erp.token_input.clear();
                                self.erp.check_msg = "Token remembered (encrypted for your Windows account).".into();
                            }
                            Err(e) => self.erp.check_msg = format!("✖ {e}"),
                        }
                    }
                }
                if ui.add_enabled(has, egui::Button::new("Use this session")).on_hover_text("Kept in memory until the app closes").clicked() {
                    self.erp.session_token = self.erp.token_input.trim().to_string();
                    self.erp.token_input.clear();
                    self.erp.check_msg = "Token in use until the app closes.".into();
                }
            });
            row(ui, "", |ui| {
                if !i.token_enc.is_empty() {
                    chip(ui, "remembered", t.ok_bg, t.ok_fg);
                }
                if !self.erp.session_token.is_empty() {
                    chip(ui, "this session", t.ok_bg, t.ok_fg);
                }
                if env_set {
                    chip(ui, &format!("from {env}"), t.ok_bg, t.ok_fg);
                }
                if (!i.token_enc.is_empty() || !self.erp.session_token.is_empty()) && ui.small_button("Forget").clicked() {
                    i.token_enc.clear();
                    self.erp.session_token.clear();
                }
            });
            ui.label(RichText::new(format!("{} Or set {env} in the environment{}.", p.token_hint(), if cfg!(windows) { "" } else { " (tokens are only remembered on Windows)" })).size(11.0).color(t.text_dim));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(self.erp.check.is_none(), egui::Button::new("Test connection")).clicked() {
                    test = true;
                }
                if !self.erp.check_msg.is_empty() {
                    let col = if self.erp.check_msg.starts_with('✖') { t.warn_fg } else { t.ok_fg };
                    ui.label(RichText::new(&self.erp.check_msg).color(col));
                }
            });
        });
        ui.add_space(10.0);
        card(ui, "Work orders", "Who groups the work and makes the work orders the nests go on.", |ui| {
            let i = self.cfg.integration_mut(p);
            row(ui, "Made by", |ui| {
                ui.radio_value(&mut i.work_orders, WorkOrders::Planner, format!("The planner, in {}", p.name()));
                ui.radio_value(&mut i.work_orders, WorkOrders::App, "This app, when a nest is sent");
            });
            if i.work_orders == WorkOrders::Planner {
                ui.label(RichText::new("Open a work order from the list and nest it; Send nest stores the nest on that work order. Work imported before the planner made one goes to it once it exists. The app never makes work orders.").size(11.0).color(t.text_dim));
            } else {
                row(ui, "New work orders", |ui| {
                    ui.radio_value(&mut i.approve, false, "Need review");
                    ui.radio_value(&mut i.approve, true, "Approved");
                });
            }
        });
        ui.add_space(10.0);
        self.settings_stations(ui, p);
        ui.add_space(10.0);
        card(ui, "What is sent back", "With each nest: the sheets, the parts on each, how many go to each job operation, and what didn't fit.", |ui| {
            let i = self.cfg.integration_mut(p);
            row(ui, "Sizes in", |ui| {
                ui.radio_value(&mut i.units, Units::Inch, "inches");
                ui.radio_value(&mut i.units, Units::Millimeter, "millimetres");
            });
            if p == Provider::Fulcrum {
                row(ui, "Report the nest as", |ui| {
                    egui::ComboBox::from_id_salt("erp_software").selected_text(i.software.clone()).show_ui(ui, |ui| {
                        for s in erp::fulcrum::SOFTWARE {
                            ui.selectable_value(&mut i.software, s.to_string(), s);
                        }
                    });
                    ui.label(RichText::new("the nesting program Fulcrum shows").size(11.0).color(th().text_dim));
                });
            }
        });
        ui.add_space(10.0);
        card(ui, "Drawings", "Units of a DXF that doesn't say ($INSUNITS). Marking layers (MARK, ETCH…) are marked and drafting layers (DIM, TEXT, TITLE…) left out.", |ui| {
            let i = self.cfg.integration_mut(p);
            row(ui, "Unitless drawings", |ui| {
                ui.radio_value(&mut i.drawing_units, Units::Inch, "inches");
                ui.radio_value(&mut i.drawing_units, Units::Millimeter, "millimetres");
            });
        });
        if test {
            self.erp_test();
        }
    }

    /// Settings → ERP / MES → Work stations: the system's operations (and the
    /// machines they may run on) mapped to the app's machines.
    fn settings_stations(&mut self, ui: &mut egui::Ui, p: Provider) {
        let t = th();
        let machines: Vec<String> = self.cfg.machines.iter().map(|m| m.name.clone()).collect();
        let mut load = false;
        // What to list: the loaded operations (nestable ones, or all when none
        // is marked), plus mapped ones not in that list.
        let integ = self.cfg.integration(p);
        let any_nestable = self.erp.station_info.iter().any(|s| s.nestable);
        let mut ops: Vec<StationInfo> = self.erp.station_info.iter().filter(|s| s.nestable || !any_nestable).cloned().collect();
        for st in &integ.stations {
            if !ops.iter().any(|o| o.op.id == st.id) {
                ops.push(StationInfo { op: erp::Ref { id: st.id.clone(), name: st.name.clone() }, ..Default::default() });
            }
        }
        let mut changes: Vec<(bool, erp::Ref, String)> = vec![];
        card(ui, "Work stations", "Which of the shop's operations run on which of this app's machines. Only work on mapped operations is listed, and a job opened from it gets that machine.", |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(self.erp.stations_task.is_none(), egui::Button::new(format!("Load operations from {}", p.name()))).clicked() {
                    load = true;
                }
                if !self.erp.stations_msg.is_empty() {
                    let col = if self.erp.stations_msg.starts_with('✖') { t.warn_fg } else { t.text_dim };
                    ui.label(RichText::new(&self.erp.stations_msg).size(11.5).color(col));
                }
            });
            if ops.is_empty() {
                ui.label(RichText::new("Nothing mapped yet: every operation's work is listed.").size(11.5).color(t.text_dim));
                return;
            }
            ui.add_space(4.0);
            let pick = |ui: &mut egui::Ui, salt: String, cur: &str, none: &str| -> Option<String> {
                let mut v = cur.to_string();
                let shown = if v.is_empty() { none.to_string() } else { v.clone() };
                egui::ComboBox::from_id_salt(salt).width(170.0).selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut v, String::new(), none);
                    for m in &machines {
                        ui.selectable_value(&mut v, m.clone(), m);
                    }
                });
                (v != cur).then_some(v)
            };
            egui::Grid::new("erp_stations").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                for c in [format!("{} operation", p.name()), "Runs on (its machines)".into(), "This app's machine".into()] {
                    ui.label(RichText::new(c).size(11.5).strong().color(t.text_dim));
                }
                ui.end_row();
                for o in &ops {
                    ui.label(RichText::new(&o.op.name).strong());
                    let wh: Vec<&str> = o.equipment.iter().chain(o.work_centers.iter()).map(|r| r.name.as_str()).collect();
                    ui.label(RichText::new(if wh.is_empty() { "—".to_string() } else { wh.join(", ") }).size(11.5).color(t.text_dim));
                    if let Some(m) = pick(ui, format!("st_{}", o.op.id), integ.station_machine(&o.op.id).unwrap_or(""), "— not ours —") {
                        changes.push((false, o.op.clone(), m));
                    }
                    ui.end_row();
                    // Its machines, when a step is scheduled on one of them.
                    if integ.station_machine(&o.op.id).is_some() {
                        for e in &o.equipment {
                            ui.label(RichText::new(format!("    ↳ {}", e.name)).size(12.0));
                            ui.label(RichText::new("when scheduled on this machine").size(11.0).color(t.text_dim));
                            let cur = integ.equipment.iter().find(|x| x.id == e.id).map_or("", |x| x.machine.as_str());
                            if let Some(m) = pick(ui, format!("eq_{}_{}", o.op.id, e.id), cur, "same as the operation") {
                                changes.push((true, e.clone(), m));
                            }
                            ui.end_row();
                        }
                    }
                }
            });
        });
        let i = self.cfg.integration_mut(p);
        for (eq, r, m) in changes {
            i.set_station(eq, &r, &m);
        }
        if load {
            self.erp_load_stations();
        }
    }

    /// Screenshots: fill the window with made-up work (no network): two laser
    /// operations mapped to the first machines, a saw that isn't ours.
    pub fn erp_demo(&mut self) {
        use erp::{NestRef, OpenOp, Ref};
        let r = |id: &str, n: &str| Ref { id: id.into(), name: n.into() };
        let op = |job: &str, item: &str, qty: f64, cust: &str, so: &str, due: &str| OpenOp { job: r(job, job), op_id: format!("{job}-{item}"), item_to_make_id: String::new(), item: r(item, item), qty, customer: cust.into(), sales_order: so.into(), due: due.into(), start: "2026-11-05T07:00:00Z".into(), rework: false, multi_material: false };
        let wo = |n: &str, status: &str, step: &str, sheets: u32, parts: u32, s: f64| NestRef { work_order: r(&format!("wo-{n}"), n), op_id: format!("step-{n}"), status: status.into(), op_status: step.into(), sheets, parts, machine_s: s, nested: sheets > 0 };
        let laser = r("op-laser", "Laser Cut");
        let thin = r("op-thin", "Fiber Laser Thin");
        self.erp.groups = vec![
            Group {
                material: r("SS-304-Sheet-0.25", "304 Stainless Sheet 1/4\""),
                operation: laser.clone(),
                earliest_start: "2026-11-05T07:00:00Z".into(),
                open: vec![op("J-1050", "CVR-9 Cover", 8.0, "Globex", "SO-1031", "2026-11-19")],
                nests: vec![wo("WO-318", "scheduled", "Ready", 0, 0, 0.0), wo("WO-311", "inProgress", "Running", 3, 46, 2710.0)],
            },
            Group { material: r("A36-Plate-0.375", "A36 Plate 3/8\""), operation: laser.clone(), earliest_start: "2026-11-06T07:00:00Z".into(), open: vec![], nests: vec![wo("WO-320", "approved", "Pending", 0, 0, 0.0)] },
            Group { material: r("AL-5052-Sheet-0.125", "5052 Aluminum 1/8\""), operation: thin.clone(), earliest_start: String::new(), open: vec![op("J-1052", "PNL-7 Panel", 30.0, "Globex", "SO-1031", "2026-11-21")], nests: vec![wo("WO-322", "needsReview", "Pending", 0, 0, 0.0)] },
            Group { material: r("A36-Bar-2x0.25", "A36 Flat Bar 2 x 1/4\""), operation: r("op-saw", "Saw"), earliest_start: String::new(), open: vec![op("J-1047", "BASE-4 Base plate", 12.0, "Initech", "SO-1019", "2026-11-14")], nests: vec![wo("WO-319", "approved", "Pending", 0, 0, 0.0)] },
        ];
        let names: Vec<String> = self.cfg.machines.iter().map(|m| m.name.clone()).collect();
        let first = names.first().cloned().unwrap_or_else(|| "Laser".into());
        let second = names.get(1).cloned().unwrap_or_else(|| first.clone());
        let i = self.cfg.integration_mut(Provider::Fulcrum);
        i.set_station(false, &laser, &first);
        i.set_station(false, &thin, &second);
        self.erp.station_info = vec![
            StationInfo { op: laser, nestable: true, work_centers: vec![r("wc1", "Laser cell")], equipment: vec![r("eq1", "Fiber 6 kW"), r("eq2", "Fiber 3 kW")] },
            StationInfo { op: thin, nestable: true, work_centers: vec![r("wc1", "Laser cell")], equipment: vec![r("eq2", "Fiber 3 kW")] },
        ];
        self.erp.stations_msg = "✔ 6 operation(s), 2 marked nestable.".into();
        self.erp.loaded = true;
        self.erp.open = true;
        self.erp.tab = ErpTab::WorkOrders;
    }
}

/// A work order's status in shop words.
fn status_label(s: &str) -> &str {
    match s {
        "needsReview" => "needs review",
        "approved" => "approved",
        "scheduled" => "scheduled",
        "inProgress" => "in progress",
        "complete" => "complete",
        "cancelled" => "cancelled",
        x => x,
    }
}

/// A group in the list: material (title) and what waits (sub).
fn group_tile(ui: &mut egui::Ui, selected: bool, title: &str, sub: &str) -> bool {
    let t = th();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(282.0, 50.0), egui::Sense::click());
    let fill = if selected { t.accent_soft } else if resp.hovered() { t.hover } else { t.card };
    ui.painter().rect(r, 8, fill, egui::Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { t.accent } else { t.border }), egui::StrokeKind::Inside);
    let clip = |s: &str, n: usize| if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s.to_string() };
    ui.painter().text(r.left_top() + egui::vec2(10.0, 8.0), egui::Align2::LEFT_TOP, clip(title, 34), egui::FontId::proportional(13.0), if selected { t.accent_fg } else { t.text_strong });
    ui.painter().text(r.left_top() + egui::vec2(10.0, 28.0), egui::Align2::LEFT_TOP, clip(sub, 44), egui::FontId::proportional(11.0), t.text_dim);
    ui.add_space(3.0);
    resp.on_hover_text(format!("{title}\n{sub}")).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dxf::LayerRole;

    #[test]
    fn layer_names_pick_their_role() {
        assert_eq!(guess_role("0"), LayerRole::Cut);
        assert_eq!(guess_role("Outline"), LayerRole::Cut);
        assert_eq!(guess_role("ETCH"), LayerRole::Mark);
        assert_eq!(guess_role("Bend Lines"), LayerRole::Mark);
        assert_eq!(guess_role("DIMENSIONS"), LayerRole::Ignore);
        assert_eq!(guess_role("Title Block"), LayerRole::Ignore);
    }

    /// A 4 × 2 inch plate with its unit in the header, and a dimension line.
    const PLATE_IN: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
0\nLINE\n8\n0\n10\n0\n20\n0\n11\n4\n21\n0\n0\nLINE\n8\n0\n10\n4\n20\n0\n11\n4\n21\n2\n\
0\nLINE\n8\n0\n10\n4\n20\n2\n11\n0\n21\n2\n0\nLINE\n8\n0\n10\n0\n20\n2\n11\n0\n21\n0\n\
0\nLINE\n8\nDIM\n10\n0\n20\n-1\n11\n4\n21\n-1\n0\nENDSEC\n0\nEOF\n";

    fn wait(a: &mut App, ctx: &egui::Context, done: impl Fn(&App) -> bool) {
        for _ in 0..400 {
            a.erp_poll(ctx);
            if done(a) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("the request never finished");
    }

    /// A laser operation with job operation jo1 waiting and the planner's
    /// work order WO-318 holding it (ids are Fulcrum-sized, 24 characters).
    const WO_ID: &str = "6650dddddddddddddddddddd";
    const STEP_ID: &str = "6650eeeeeeeeeeeeeeeeeeee";
    fn groups_with_wo(with_wo: bool) -> String {
        let nests = if with_wo { format!(r#"[{{"workOrderOperationId":"{STEP_ID}","workOrder":{{"id":"{WO_ID}","name":"WO-318"}},"status":"scheduled","operationStatus":"Ready","sheetCount":0,"partCount":0,"machineTimeSeconds":0,"isNested":false}}]"#) } else { "[]".into() };
        format!(
            r#"{{"data":[{{"material":{{"id":"SS-304-Sheet-0.25","name":"304 SS 1/4\""}},"operation":{{"id":"op-laser","name":"Laser Cut"}},
            "jobOperations":[{{"jobOperationId":"jo1","job":{{"id":"6650aaaaaaaaaaaaaaaaaaaa","name":"J-1001"}},"item":{{"id":"it1","name":"Bracket"}},"quantityToMake":12}}],
            "workOrderOperations":{nests}}},
            {{"material":{{"id":"A36-Bar","name":"A36 Flat Bar"}},"operation":{{"id":"op-saw","name":"Saw"}},"jobOperations":[],"workOrderOperations":[]}}],"hasNextPage":false}}"#
        )
    }
    fn steps() -> String {
        format!(r#"[{{"id":"{STEP_ID}","order":1,"status":"ready","operation":{{"id":"op-laser","name":"Laser Cut"}},"scheduledEquipment":{{"id":"eq-6k","name":"Fiber 6 kW"}},"isNested":false,"jobOperations":[{{"jobId":"6650aaaaaaaaaaaaaaaaaaaa","jobNumber":1001,"jobName":"J-1001","itemToMakeId":"itm1","jobOperationId":"jo1"}}]}}]"#)
    }

    /// A shop whose planner makes work orders, with the laser operation
    /// mapped to "Fiber 1" and the 6 kW machine to "Fiber 2".
    fn planner_app(url: String) -> App {
        let mut a = App::new(None);
        let mut i = Integration { base_url: url, units: Units::Inch, work_orders: WorkOrders::Planner, ..Integration::new(Provider::Fulcrum) };
        i.set_station(false, &erp::Ref { id: "op-laser".into(), name: "Laser Cut".into() }, "Fiber 1");
        i.set_station(true, &erp::Ref { id: "eq-6k".into(), name: "Fiber 6 kW".into() }, "Fiber 2");
        *a.cfg.integration_mut(Provider::Fulcrum) = i;
        a.cfg.machines = vec![crate::settings::Machine { name: "Fiber 1".into(), ..Default::default() }, crate::settings::Machine { name: "Fiber 2".into(), ..Default::default() }];
        a.cfg.active_machine = 0;
        a.erp.session_token = "tok".into();
        a
    }

    fn mock_planner(with_wo: bool) -> (String, std::sync::Arc<std::sync::Mutex<Vec<(String, String, String)>>>) {
        use crate::integrations::fulcrum::tests::MANIFEST;
        crate::integrations::mock::mock(vec![
            ("/api/nesting/nestable-jobs/list", 200, groups_with_wo(with_wo)),
            ("/api/nesting/work-package-manifest", 200, MANIFEST.into()),
            ("/api/attachments/6650bbbbbbbbbbbbbbbbbbbb/download", 200, PLATE_IN.into()),
            ("/operations/list", 200, steps()),
            ("/nesting-setup", 200, r#"{"validationIssues":[]}"#.into()),
            ("/api/work-orders", 500, String::new()),
        ])
    }

    #[test]
    fn the_planners_work_order_opens_as_a_job_and_takes_its_nest() {
        let (url, seen) = mock_planner(true);
        let mut a = planner_app(url);
        let ctx = egui::Context::default();
        a.erp_refresh();
        wait(&mut a, &ctx, |a| a.erp.load.is_none());
        let integ = a.integ();
        // F20: only the mapped laser operation's work is listed (not the saw).
        assert_eq!(a.erp_wo_rows(&integ), vec![(0, 0)]);
        assert_eq!(a.erp_waiting_groups(&integ), vec![0]);
        a.erp_open_step(0, 0);
        wait(&mut a, &ctx, |a| a.erp.import.is_none());
        assert!(a.erp.msg_ok, "{} {:?}", a.erp.msg, a.erp.notes);
        assert_eq!(a.job_name, "WO-318 304 SS 1/4\"");
        let wo = a.erp_job.as_ref().unwrap().work_order.clone().unwrap();
        assert_eq!((wo.id.as_str(), wo.op_id.as_str(), wo.name.as_str()), (WO_ID, STEP_ID, "WO-318"));
        assert_eq!(a.cfg.machine().name, "Fiber 2", "F21: scheduled on the 6 kW machine");
        assert_eq!((a.parts.len(), a.parts[0].qty), (1, 12));
        // Opening it again switches to the job instead of making another.
        let n = a.jobs.len();
        a.erp_open_step(0, 0);
        assert!(a.erp.import.is_none() && a.jobs.len() == n);

        a.run_nesting();
        a.erp_send();
        wait(&mut a, &ctx, |a| a.erp.send.is_none());
        assert!(a.erp.msg_ok, "{} {:?}", a.erp.msg, a.erp.notes);
        let seen = seen.lock().unwrap();
        assert!(!seen.iter().any(|s| s.0 == "POST /api/work-orders"), "the app never makes a work order");
        assert!(seen.iter().any(|s| s.0 == format!("PUT /api/work-orders/{WO_ID}/operations/{STEP_ID}/nesting-setup")));
    }

    #[test]
    fn work_imported_early_goes_to_the_planners_work_order_once_it_exists() {
        // F22: imported from Waiting (no work order), sent once the planner made one.
        let (url, seen) = mock_planner(true);
        let mut a = planner_app(url);
        let ctx = egui::Context::default();
        a.erp_refresh();
        wait(&mut a, &ctx, |a| a.erp.load.is_none());
        a.erp.sel_group = 0;
        a.erp.picked.insert(("6650aaaaaaaaaaaaaaaaaaaa".into(), "jo1".into()));
        a.erp_import();
        wait(&mut a, &ctx, |a| a.erp.import.is_none());
        assert!(a.erp_job.as_ref().unwrap().work_order.is_none());
        assert_eq!(a.cfg.machine().name, "Fiber 1", "the operation's machine");
        a.run_nesting();
        a.erp_send();
        wait(&mut a, &ctx, |a| a.erp.send.is_none());
        assert!(a.erp.msg_ok, "{} {:?}", a.erp.msg, a.erp.notes);
        assert_eq!(a.erp_job.as_ref().unwrap().work_order.as_ref().unwrap().op_id, STEP_ID);
        assert!(a.erp.notes.iter().any(|n| n.contains("WO-318")), "{:?}", a.erp.notes);
        let seen = seen.lock().unwrap();
        assert!(!seen.iter().any(|s| s.0 == "POST /api/work-orders"));
        assert!(seen.iter().any(|s| s.0.starts_with("PUT ")));
    }

    #[test]
    fn without_the_planners_work_order_nothing_is_sent() {
        let (url, seen) = mock_planner(false);
        let mut a = planner_app(url);
        let ctx = egui::Context::default();
        a.erp_refresh();
        wait(&mut a, &ctx, |a| a.erp.load.is_none());
        a.erp.picked.insert(("6650aaaaaaaaaaaaaaaaaaaa".into(), "jo1".into()));
        a.erp_import();
        wait(&mut a, &ctx, |a| a.erp.import.is_none());
        a.run_nesting();
        a.erp_send();
        wait(&mut a, &ctx, |a| a.erp.send.is_none());
        assert!(!a.erp.msg_ok && a.erp.msg.contains("planner makes it"), "{}", a.erp.msg);
        assert!(a.erp_job.as_ref().unwrap().work_order.is_none());
        let seen = seen.lock().unwrap();
        assert!(!seen.iter().any(|s| s.0 == "POST /api/work-orders" || s.0.starts_with("PUT ")));
    }

    #[test]
    fn import_nest_and_send_through_the_app() {
        use crate::integrations::fulcrum::tests::{GROUPS, MANIFEST};
        let (url, seen) = crate::integrations::mock::mock(vec![
            ("/api/nesting/nestable-jobs/list", 200, GROUPS.into()),
            ("/api/nesting/work-package-manifest", 200, MANIFEST.into()),
            ("/api/attachments/6650bbbbbbbbbbbbbbbbbbbb/download", 200, PLATE_IN.into()),
            ("/api/work-orders", 200, r#"{"id":"6650cccccccccccccccccccc"}"#.into()),
            ("/operations/list", 200, r#"[{"id":"woop9","order":1,"jobOperations":[{"jobId":"6650aaaaaaaaaaaaaaaaaaaa","jobOperationId":"jo1"}]}]"#.into()),
            ("/nesting-setup", 200, r#"{"validationIssues":[]}"#.into()),
        ]);
        let mut a = App::new(None);
        let ctx = egui::Context::default();
        // This shop setting: the app makes the work order on the first send.
        *a.cfg.integration_mut(Provider::Fulcrum) = Integration { base_url: url, units: Units::Inch, work_orders: WorkOrders::App, ..Integration::new(Provider::Fulcrum) };
        a.erp.session_token = "tok".into();
        let jobs_before = a.jobs.len();

        a.erp_refresh();
        wait(&mut a, &ctx, |a| a.erp.load.is_none());
        assert_eq!(a.erp.groups.len(), 1, "{}", a.erp.msg);
        a.erp.picked.insert(("6650aaaaaaaaaaaaaaaaaaaa".into(), "jo1".into()));
        a.erp_import();
        wait(&mut a, &ctx, |a| a.erp.import.is_none());
        assert!(a.erp.msg_ok, "{} {:?}", a.erp.msg, a.erp.notes);
        assert_eq!(a.jobs.len(), jobs_before + 1);
        assert_eq!(a.job_name, "304 SS 1/4\" Laser");
        assert_eq!((a.parts.len(), a.parts[0].name.as_str(), a.parts[0].qty), (1, "BRK-1 rev B", 12));
        assert!((a.parts[0].w - 101.6).abs() < 1e-6, "inches became mm");
        assert_eq!((a.spec.material, a.spec.thickness), (1, 6.35), "304 stainless, 1/4 inch");
        assert_eq!(a.erp_job.as_ref().unwrap().material_id, "SS-304-Sheet-0.25");
        assert!(!a.erp_can_send(), "nothing nested yet");

        a.run_nesting();
        assert_eq!(a.placed_of(0), 12);
        a.erp_send();
        wait(&mut a, &ctx, |a| a.erp.send.is_none());
        assert!(a.erp.msg_ok, "{} {:?}", a.erp.msg, a.erp.notes);
        let l = a.erp_job.clone().unwrap();
        assert_eq!(l.work_order.unwrap().op_id, "woop9");
        assert!(l.sent.is_some());
        // The job list keeps the link (it is saved with the job).
        a.store_job();
        assert!(a.jobs[a.cur_job].erp.as_ref().unwrap().work_order.is_some());

        let seen = seen.lock().unwrap();
        let put = seen.iter().find(|s| s.0.starts_with("PUT ")).unwrap();
        let b: serde_json::Value = serde_json::from_str(&put.2).unwrap();
        assert_eq!(b["parts"][0]["quantity"], 12);
        assert_eq!(b["parts"][0]["jobOperationQuantities"][0]["jobOperationId"], "jo1");
        assert_eq!(b["parts"][0]["itemId"], "it1");
        assert!((b["parts"][0]["surfaceArea"].as_f64().unwrap() - 8.0).abs() < 1e-3, "4 × 2 in²");
        let sheets: u64 = b["plates"].as_array().unwrap().iter().map(|p| p["count"].as_u64().unwrap()).sum();
        assert_eq!(sheets as usize, a.result.sheets.len());
        assert!((b["plates"][0]["dimensionX"].as_f64().unwrap() - a.spec.w / 25.4).abs() < 1e-3);
    }

    #[test]
    fn an_inch_drawing_becomes_a_part_in_mm_with_its_orders() {
        // A 4 × 2 inch plate, units stated in the header.
        let dxf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
0\nLINE\n8\n0\n10\n0\n20\n0\n11\n4\n21\n0\n0\nLINE\n8\n0\n10\n4\n20\n0\n11\n4\n21\n2\n\
0\nLINE\n8\n0\n10\n4\n20\n2\n11\n0\n21\n2\n0\nLINE\n8\n0\n10\n0\n20\n2\n11\n0\n21\n0\n\
0\nLINE\n8\nDIM\n10\n0\n20\n-1\n11\n4\n21\n-1\n0\nENDSEC\n0\nEOF\n";
        let d = |job: &str, qty: u32| erp::Demand { provider: Provider::Fulcrum, job_id: job.into(), job_name: job.into(), op_id: "o".into(), item_to_make_id: String::new(), item_id: "i".into(), qty, customer: String::new(), due: String::new(), drawing_id: "a".into(), drawing_name: "p.dxf".into() };
        let ip = ImportPart { name: "PLT-1".into(), drawing: erp::DrawingRef { id: "a".into(), file_name: "p.dxf".into(), op_specific: false }, demands: vec![d("j1", 3), d("j2", 4)] };
        let (p, notes) = part_from(&ip, dxf, Units::Millimeter, 0).unwrap();
        assert!((p.w - 101.6).abs() < 1e-6 && (p.h - 50.8).abs() < 1e-6, "{} × {}", p.w, p.h);
        assert_eq!((p.qty, p.erp.len(), p.name.as_str()), (7, 2, "PLT-1"));
        assert!(notes.iter().any(|n| n.contains("DIM")), "{notes:?}");
        // Without a unit in the file, the setting decides.
        let bare = dxf.replace("9\n$INSUNITS\n70\n1\n", "");
        let (q, _) = part_from(&ip, &bare, Units::Inch, 0).unwrap();
        assert!((q.w - 101.6).abs() < 1e-6);
        let (q, _) = part_from(&ip, &bare, Units::Millimeter, 0).unwrap();
        assert!((q.w - 4.0).abs() < 1e-6);
        // A 1 inch hole in a file without a unit stays round at the inch setting.
        let holed = bare.replace("0\nENDSEC\n0\nEOF", "0\nCIRCLE\n8\n0\n10\n2\n20\n1\n40\n0.5\n0\nENDSEC\n0\nEOF");
        let (q, _) = part_from(&ip, &holed, Units::Inch, 0).unwrap();
        let h = &q.custom.as_ref().unwrap().holes[0];
        assert!(h.len() > 20, "a 1 inch hole in {} pieces", h.len());
    }
}
