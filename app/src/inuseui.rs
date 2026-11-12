//! Jobs in use, in the window (CU5–CU11; the rules are in
//! `engine/inuse.rs`): the holds this window takes, renews and lets go of as
//! jobs come to the front, get unsaved changes, are closed or the program
//! quits; the in-use notes in the lists; what opening a job someone else has
//! in use asks (CU8); Take over and its warning (CU9); and being told a job
//! was taken over.

use crate::engine::inuse::{self, InUse, NotTaken, Renewed, Want, Watch};
use crate::library::Library;
use crate::theme::th;
use crate::App;
use eframe::egui;
use egui::RichText;
use std::collections::{BTreeMap, BTreeSet};

/// What a renewal brings back: what it found for each job, and every job
/// someone else has in use now.
type Tick = (Result<Vec<(u64, Renewed)>, String>, Result<BTreeMap<u64, InUse>, String>);

#[derive(Default)]
pub struct InUseUi {
    /// What this window has seen of the holds (CU7).
    pub watch: Watch,
    /// Library jobs this window holds, by record.
    pub held: BTreeSet<u64>,
    /// Jobs open here that someone else holds: opened anyway (CU8), or
    /// brought to the front while theirs. Taken at a renewal once free.
    pub busy: BTreeMap<u64, InUse>,
    /// Jobs this window held that someone else has now (CU9): kept open with
    /// their changes, never taken back without asking.
    pub lost: BTreeMap<u64, InUse>,
    /// Every job someone else has in use, as last looked at (CU10).
    pub others: BTreeMap<u64, InUse>,
    /// Opening (or saving) a job someone else has in use: what's asked.
    pub ask: Option<Ask>,
    /// A job taken over from this window, being told (CU9).
    pub told: Option<InUse>,
    /// When the next renewal is due, by this window's clock (CU6).
    pub next: u64,
    /// A renewal running in the background (a share that's slow to answer
    /// never stops the window).
    pub running: Option<std::sync::mpsc::Receiver<Tick>>,
    /// The last renewal couldn't reach the Library: said once (LB11).
    pub out_of_reach: bool,
    /// The job in front, how many jobs were open and whether warn was on, at
    /// the last sync.
    pub synced: Option<(Option<u64>, usize, bool)>,
    /// The Library folder the holds are in (let go of there, even after
    /// the Library folder setting changed).
    pub dir: Option<std::path::PathBuf>,
}

/// What a job someone else has in use was being opened for.
#[derive(Clone, Debug, PartialEq)]
pub enum Opening {
    /// A library job not open here yet.
    Library(u64),
    /// The open job at this index, brought to the front.
    Open(usize),
    /// Saving the open job over its record.
    Save,
}

/// The question about a job someone else has in use (CU8), and, once Take
/// over was pressed, its warning (CU9).
#[derive(Clone, Debug)]
pub struct Ask {
    pub what: Opening,
    pub u: InUse,
    pub warning: bool,
}

impl App {
    /// The Library jobs in use here (CU5): the job in front and every job
    /// with unsaved changes, by record and name; none taken over from here.
    pub fn in_use_jobs(&mut self) -> Vec<(u64, String)> {
        self.store_job();
        let mut out = vec![];
        for i in 0..self.jobs.len() {
            let Some(id) = self.jobs[i].lib_id else { continue };
            if self.inuse.lost.contains_key(&id) || out.iter().any(|(j, _)| *j == id) {
                continue;
            }
            if i == self.cur_job || self.job_unsaved(i) {
                out.push((id, self.jobs[i].name.clone()));
            }
        }
        out
    }

    /// Take the holds this window should have and let go of the rest (CU5),
    /// now: after a job came to the front, was opened, closed or saved. With
    /// warn off it holds nothing (CU1). While the Library can't be reached
    /// nothing is tried here: the renewal looks again (LB11).
    pub fn sync_holds(&mut self) {
        let open: BTreeSet<u64> = self.jobs.iter().filter_map(|j| j.lib_id).chain(self.job_lib_id).collect();
        // A job taken over from here that is closed now: done with (CU9).
        let watch = self.inuse.watch.clone();
        self.inuse.lost.retain(|id, _| open.contains(id) || {
            watch.done_with(*id);
            false
        });
        self.inuse.synced = Some((self.job_lib_id, self.jobs.len(), self.cfg.in_use_warn));
        let wants = if self.cfg.in_use_warn { self.in_use_jobs() } else { vec![] };
        if !self.cfg.in_use_warn || (wants.is_empty() && self.inuse.held.is_empty()) {
            // Warn off: nothing held (CU1). Nothing in use: the Library isn't
            // touched at all.
            self.release_holds();
            self.inuse.busy.clear();
            return;
        }
        if self.libui.error.is_some() {
            return;
        }
        let Ok(lib) = self.library() else { return };
        self.inuse.dir = Some(lib.dir.clone());
        let drop: Vec<u64> = self.inuse.held.iter().copied().filter(|h| !wants.iter().any(|(w, _)| w == h)).collect();
        if inuse::release(&lib, &drop).is_ok() {
            for h in &drop {
                self.inuse.held.remove(h);
            }
        }
        self.inuse.busy.retain(|id, _| wants.iter().any(|(w, _)| w == id));
        for (id, name) in wants {
            if self.inuse.held.contains(&id) {
                continue;
            }
            match inuse::take(&lib, id, &name) {
                Ok(_) => {
                    self.inuse.held.insert(id);
                    self.inuse.busy.remove(&id);
                }
                Err(NotTaken::InUse(u)) => {
                    // Came to the front by itself (CU8): said, not asked.
                    if !self.inuse.busy.contains_key(&id) && self.job_lib_id == Some(id) {
                        self.status = format!("{} It's open here as usual; saving it asks first if they've saved it since.", u.said(&self.clock));
                    }
                    self.inuse.busy.insert(id, u);
                }
                // The share: said by the renewal and the lists (LB11).
                Err(NotTaken::Failed(_)) => {}
            }
        }
    }

    /// Let go of every hold this window has (warn turned off, CU1; quitting).
    pub fn release_holds(&mut self) {
        if self.inuse.held.is_empty() {
            return;
        }
        let held: Vec<u64> = self.inuse.held.iter().copied().collect();
        let dir = self.inuse.dir.clone().unwrap_or_else(|| self.library_dir());
        let lib = Library::at(&dir, &self.who, &self.clock).watching(&self.inuse.watch);
        if inuse::release(&lib, &held).is_ok() {
            self.inuse.held.clear();
        }
    }

    /// Quitting: let go of this window's holds, waiting a few seconds at most
    /// for a share that's slow to answer. One not let go is gone 2 minutes
    /// later anyway (CU7).
    pub fn release_holds_on_quit(&mut self) {
        if self.inuse.held.is_empty() {
            return;
        }
        let held: Vec<u64> = std::mem::take(&mut self.inuse.held).into_iter().collect();
        let dir = self.inuse.dir.clone().unwrap_or_else(|| self.library_dir());
        let lib = Library::at(&dir, &self.who, &self.clock).watching(&self.inuse.watch);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(inuse::release(&lib, &held));
        });
        let _ = rx.recv_timeout(std::time::Duration::from_secs(3));
    }

    /// What a renewal asks for: every job in use here (CU5), whether held.
    fn renewal_wants(&mut self) -> Vec<Want> {
        if !self.cfg.in_use_warn {
            return vec![];
        }
        let held = self.inuse.held.clone();
        self.in_use_jobs().into_iter().map(|(job, name)| Want { held: held.contains(&job), job, name }).collect()
    }

    /// The Library as the renewal uses it: as it is, nothing set up (LB11).
    fn renewal_library(&self) -> Library {
        Library::at(&self.library_dir(), &self.who, &self.clock).watching(&self.inuse.watch)
    }

    /// Renew now, in this thread (what the timer does in the background).
    pub fn renew_holds(&mut self) {
        let wants = self.renewal_wants();
        let lib = self.renewal_library();
        let r = inuse::renew(&lib, &wants);
        let l = inuse::look(&lib);
        self.apply_renewal((r, l));
    }

    /// Every frame: take or let go of holds when another job came to the
    /// front or a job was opened or closed; renew about every 30 s in the
    /// background (CU6), and take in what the last renewal found.
    pub fn hold_frame(&mut self, ctx: &egui::Context) {
        if self.inuse.synced != Some((self.job_lib_id, self.jobs.len(), self.cfg.in_use_warn)) {
            self.sync_holds();
        }
        if let Some(rx) = &self.inuse.running {
            match rx.try_recv() {
                Ok(t) => {
                    self.inuse.running = None;
                    self.apply_renewal(t);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.inuse.running = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        } else if self.clock.now() >= self.inuse.next {
            self.inuse.next = self.clock.now() + inuse::RENEW_EVERY.as_secs();
            let wants = self.renewal_wants();
            let lib = self.renewal_library();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let r = inuse::renew(&lib, &wants);
                let l = inuse::look(&lib);
                let _ = tx.send((r, l));
            });
            self.inuse.running = Some(rx);
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }

    /// Take in what a renewal found (CU6, CU9).
    fn apply_renewal(&mut self, (r, l): Tick) {
        match r {
            Ok(found) => {
                if std::mem::take(&mut self.inuse.out_of_reach) {
                    self.status = "The Library can be reached again; the jobs in use here are held again.".into();
                }
                let wanted: Vec<u64> = self.in_use_jobs().into_iter().map(|(j, _)| j).collect();
                let mut stale = vec![];
                for (id, res) in found {
                    match res {
                        // Let go meanwhile (another job came to the front):
                        // not held after all.
                        Renewed::Held(_) if !wanted.contains(&id) => stale.push(id),
                        Renewed::Held(_) => {
                            self.inuse.held.insert(id);
                            self.inuse.busy.remove(&id);
                        }
                        Renewed::Lost(u) => {
                            self.inuse.held.remove(&id);
                            self.status = format!("{} has taken over {}: your changes stay here — save them as a copy, or Save job to a file.", u.by().map_or_else(|| "Another PC".to_string(), |w| w.label()), u.name);
                            self.inuse.lost.insert(id, u.clone());
                            self.inuse.told = Some(u);
                        }
                        Renewed::Busy(u) => {
                            self.inuse.busy.insert(id, u);
                        }
                        Renewed::JobGone => {
                            self.inuse.held.remove(&id);
                            self.inuse.busy.remove(&id);
                            let name = self.jobs.iter().find(|j| j.lib_id == Some(id)).map_or_else(String::new, |j| j.name.clone());
                            self.status = format!("{name} is no longer in the Library: another PC deleted it. It stays open here; Save to library stores it again.");
                        }
                    }
                }
                if !stale.is_empty() {
                    let _ = inuse::release(&self.renewal_library(), &stale);
                }
            }
            // Said once, not every 30 s (LB11); the jobs stay open with their
            // changes (LB13), and the next renewal looks again.
            Err(e) => {
                if !std::mem::replace(&mut self.inuse.out_of_reach, true) {
                    self.status = format!("{e} The jobs open here stay open with their changes; their holds are renewed once it can be reached again.");
                }
            }
        }
        if let Ok(o) = l {
            self.inuse.others = o;
        }
    }

    /// Look at the holds again (with the lists): every job someone else has
    /// in use (CU10).
    pub fn look_at_holds(&mut self, lib: &Library) {
        if let Ok(o) = inuse::look(lib) {
            self.inuse.others = o;
        }
    }

    /// "In use by Sam on PROG-2 since 09:14" for library job `id`, when
    /// someone else has it (CU10).
    pub fn in_use_note(&self, id: u64) -> Option<String> {
        self.inuse.others.get(&id).map(|u| u.note(&self.clock))
    }

    /// Someone else has library job `id` in use, as this window sees it now
    /// (read again; CU7).
    fn someone_else_has(&mut self, id: u64) -> Option<InUse> {
        let lib = self.library().ok()?;
        let u = inuse::in_use(&lib, id).ok()??;
        self.inuse.others.insert(id, u.clone());
        Some(u)
    }

    /// Open library job `id`. When someone else has it in use and warn is on,
    /// ask first (CU3, CU8); with warn off it opens as before (CU1).
    pub fn open_library_job(&mut self, id: u64) {
        // Not asked again about a job held here, taken over from here (told
        // already), or opened here anyway.
        let known = self.inuse.held.contains(&id) || self.inuse.lost.contains_key(&id) || self.inuse.busy.contains_key(&id);
        if self.cfg.in_use_warn && !known {
            if let Some(u) = self.someone_else_has(id) {
                self.status = format!("{} Open it anyway, take it over, or cancel.", u.said(&self.clock));
                self.inuse.ask = Some(Ask { what: Opening::Library(id), u, warning: false });
                return;
            }
        }
        self.open_library_job_now(id);
    }

    /// Bring open job `i` to the front (the Job Explorer): a library job
    /// someone else has in use now asks first, as opening it does (CU8).
    pub fn switch_to_job(&mut self, i: usize) {
        let id = self.jobs.get(i).and_then(|j| j.lib_id);
        let known = |id: &u64| self.inuse.held.contains(id) || self.inuse.lost.contains_key(id) || self.inuse.busy.contains_key(id);
        if let Some(id) = id.filter(|id| i != self.cur_job && self.cfg.in_use_warn && !known(id)) {
            if let Some(u) = self.someone_else_has(id) {
                self.status = format!("{} Open it anyway, take it over, or cancel.", u.said(&self.clock));
                self.inuse.ask = Some(Ask { what: Opening::Open(i), u, warning: false });
                return;
            }
        }
        self.open_job(i);
    }

    /// Saving the open job over its record while someone else has it in use
    /// was refused, nothing saved (CU11): ask.
    pub fn ask_in_use(&mut self, u: InUse) {
        self.libui.overwrite = None;
        self.status = format!("{} Nothing was saved — save yours as a copy, or take it over.", u.said(&self.clock));
        self.inuse.ask = Some(Ask { what: Opening::Save, u, warning: false });
    }

    /// Open anyway (CU8, allow on): the job opens here as usual, without a
    /// hold, and saves as usual through the save check (CU3, LB7); the hold
    /// stays theirs, and this PC takes it once they let it go.
    pub fn open_anyway(&mut self) {
        let Some(a) = self.inuse.ask.take() else { return };
        if !self.cfg.in_use_allow {
            return;
        }
        self.inuse.busy.insert(a.u.job, a.u.clone());
        match a.what {
            Opening::Library(id) => self.open_library_job_now(id),
            Opening::Open(i) => self.open_job(i),
            Opening::Save => {}
        }
        self.status = format!("{} Opened here too, as usual: it stays theirs, and saving it asks first if they've saved it since.", a.u.said(&self.clock));
    }

    /// Take over, after its warning (CU9): the hold moves to this PC, then
    /// the job opens (read after the hold moved, so a save they finished
    /// first is in it) or the save goes ahead.
    pub fn take_over_job(&mut self) {
        let Some(a) = self.inuse.ask.take() else { return };
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        match inuse::take_over(&lib, a.u.job, &a.u.name) {
            Ok(was) => {
                self.inuse.held.insert(a.u.job);
                self.inuse.busy.remove(&a.u.job);
                self.inuse.lost.remove(&a.u.job);
                self.inuse.others.remove(&a.u.job);
                let from = was.map_or_else(|| "nobody (it was free)".to_string(), |h| h.by.label());
                match a.what {
                    Opening::Library(id) => self.open_library_job_now(id),
                    Opening::Open(i) => self.open_job(i),
                    Opening::Save => self.save_job_to_library(),
                }
                self.status = format!("Took over {} from {from}: their PC is told the next time it checks, within about 30 seconds. {}", a.u.name, self.status);
            }
            Err(e) => self.status = format!("Couldn't take over {}: {e}", a.u.name),
        }
    }

    /// Save mine as a copy, for a job someone else has in use (CU9; LB9's
    /// copy): a new job under the first free name; theirs stays theirs.
    pub fn save_in_use_copy(&mut self, u: &InUse) {
        if self.job_lib_id != Some(u.job) {
            if let Some(i) = self.jobs.iter().position(|j| j.lib_id == Some(u.job)) {
                self.open_job(i);
            }
        }
        if self.job_lib_id != Some(u.job) {
            return;
        }
        let theirs = self.job_name.clone();
        if self.save_copy() {
            self.inuse.lost.remove(&u.job);
            self.inuse.busy.remove(&u.job);
            self.inuse.watch.done_with(u.job);
            self.status = format!("{} {theirs} stays with {}.", self.status, u.by().map_or_else(|| "the PC that has it".to_string(), |w| w.label()));
        }
    }

    /// What opening (or saving) a job someone else has in use asks (CU8),
    /// and the Take over warning (CU9).
    pub fn in_use_window(&mut self, ctx: &egui::Context) {
        let Some(a) = self.inuse.ask.clone() else { return };
        let allow = self.cfg.in_use_allow;
        let by = a.u.by().map_or_else(|| "a PC its hold doesn't name".to_string(), |w| w.label());
        let mut pick: Option<&str> = None;
        let title = if a.warning { "Take over this job?" } else { "This job is in use" };
        egui::Window::new(RichText::new(title).size(15.0).strong()).id(egui::Id::unique("in_use_question")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_width(500.0);
            let text = if a.warning {
                format!(
                    "{by} has had {} in use {}. Taking it over moves it to this PC: their PC is told the next time it checks, within about 30 seconds, and their changes not yet saved can then only be saved as a copy. Take it over only when you know they've finished, or after asking them.",
                    a.u.name,
                    a.u.hold.as_ref().map_or_else(String::new, |h| inuse::since(&self.clock, h.since))
                )
            } else {
                match a.what {
                    Opening::Save => format!("{} Nothing was saved.", a.u.said(&self.clock)),
                    _ if allow => format!("{} You can open it here too, or take it over.", a.u.said(&self.clock)),
                    _ => format!("{} This PC's settings don't let a job someone else has in use be opened; you can take it over.", a.u.said(&self.clock)),
                }
            };
            egui::Frame::new().fill(th().warn_bg).corner_radius(4.0).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).color(th().warn_fg));
            });
            ui.add_space(8.0);
            let mut row = |ui: &mut egui::Ui, key: &'static str, label: &str, what: &str| {
                ui.horizontal(|ui| {
                    let b = egui::Button::new(RichText::new(label).strong()).min_size(egui::vec2(170.0, 28.0));
                    if ui.add(b).clicked() {
                        pick = Some(key);
                    }
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new(what).size(12.0).color(th().text_dim)).wrap());
                });
                ui.add_space(2.0);
            };
            if a.warning {
                row(ui, "confirm", "Take over", &format!("{} moves to this PC now.", a.u.name));
                row(ui, "back", "Back", "Don't take it over.");
            } else {
                match a.what {
                    Opening::Save => row(ui, "copy", "Save mine as a copy", "Save yours as a new job under a free name. Theirs stays theirs."),
                    _ if allow => row(ui, "anyway", "Open anyway", "Open it here too. It stays theirs; if they save first, saving asks you."),
                    _ => {}
                }
                row(ui, "take", "Take over…", &format!("Move it to this PC; {by} is told."));
                row(ui, "cancel", "Cancel", if a.what == Opening::Save { "Keep working; nothing is saved." } else { "Don't open it." });
            }
        });
        match pick {
            Some("anyway") => self.open_anyway(),
            Some("take") => {
                if let Some(q) = self.inuse.ask.as_mut() {
                    q.warning = true;
                }
            }
            Some("confirm") => self.take_over_job(),
            Some("back") => {
                if let Some(q) = self.inuse.ask.as_mut() {
                    q.warning = false;
                }
            }
            Some("copy") => {
                self.inuse.ask = None;
                self.save_in_use_copy(&a.u);
            }
            Some("cancel") => {
                self.inuse.ask = None;
                self.status = if a.what == Opening::Save { format!("Not saved: {}", a.u.said(&self.clock)) } else { format!("Not opened: {}", a.u.said(&self.clock)) };
            }
            _ => {}
        }
    }

    /// Being told a job was taken over from this PC (CU9): its changes stay;
    /// save them as a copy now, or later (saving over it asks).
    pub fn taken_over_window(&mut self, ctx: &egui::Context) {
        let Some(u) = self.inuse.told.clone() else { return };
        let mut copy = false;
        let mut ok = false;
        let by = u.by().map_or_else(|| "Another PC".to_string(), |w| w.label());
        egui::Window::new(RichText::new("Someone else has taken over this job").size(15.0).strong()).id(egui::Id::unique("taken_over")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_width(500.0);
            egui::Frame::new().fill(th().warn_bg).corner_radius(4.0).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("{by} has taken over {} (in use by them {}). Your changes are still here: save them as a copy, or Save job to keep them in a file on this PC. Saving over the job asks while they have it.", u.name, u.hold.as_ref().map_or_else(String::new, |h| inuse::since(&self.clock, h.since)))).color(th().warn_fg));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                copy = ui.add(egui::Button::new(RichText::new("Save mine as a copy").strong()).min_size(egui::vec2(170.0, 28.0))).clicked();
                ok = ui.add(egui::Button::new("OK").min_size(egui::vec2(80.0, 28.0))).clicked();
            });
        });
        if copy || ok {
            self.inuse.told = None;
        }
        if copy {
            self.save_in_use_copy(&u);
        }
    }
}

impl App {
    /// Screenshots (`--in-use=note|ask|warning|taken` with `--library-dir=` a
    /// new, empty folder): made-up library jobs DEMO-7, DEMO-8 and DEMO-9 in
    /// folder Acme; Sam on PROG-2 has had DEMO-7 open since 09:14, and this
    /// window is sam on PROG-1 at 09:20. note: the Library window; tree: the
    /// Job Explorer; menu: the Open job menu; ask: opening DEMO-7; warning:
    /// Take over's warning; taken: this window had DEMO-7 and Sam took it
    /// over.
    pub fn demo_in_use(&mut self, what: &str) {
        let dir = self.library_dir();
        if self.cfg.library_dir.is_empty() || dir == crate::library::default_dir() || crate::library::holds_library(&dir) {
            self.status = "--in-use needs --library-dir= set to a new, empty folder".into();
            return;
        }
        let at_0914 = 1_794_215_640;
        let clock = crate::shared::Clock::at(at_0914);
        self.clock = clock.clone();
        self.who = crate::shared::Who::new("sam", "PROG-1");
        let sam = crate::shared::Who::new("Sam", "PROG-2");
        let r = (|| -> Result<(), String> {
            crate::library::create(&dir)?;
            let theirs = Library::open(&dir, &sam, &clock)?;
            let mut ids = vec![];
            for name in ["DEMO-7", "DEMO-8", "DEMO-9"] {
                let mut j = self.current_jobfile();
                j.name = name.into();
                j.week = "Acme".into();
                j.library = None;
                j.held.clear();
                ids.push(theirs.save_job(&j, crate::library::SaveTo::New)?.id);
            }
            let demo7 = ids[0];
            if what == "taken" {
                // This window had DEMO-7 open; Sam took it over at 09:20.
                self.open_library_job(demo7);
                if let Some(p) = self.parts.first_mut() {
                    p.qty += 2;
                }
                clock.advance(std::time::Duration::from_secs(6 * 60));
                inuse::take_over(&theirs, demo7, "DEMO-7")?;
                clock.advance(std::time::Duration::from_secs(30));
                self.renew_holds();
                return Ok(());
            }
            inuse::take(&theirs, demo7, "DEMO-7").map_err(|e| format!("{e:?}"))?;
            clock.advance(std::time::Duration::from_secs(6 * 60));
            self.lib_refresh();
            match what {
                "ask" | "warning" => {
                    self.open_library_job(demo7);
                    if let Some(a) = self.inuse.ask.as_mut() {
                        a.warning = what == "warning";
                    }
                }
                "menu" => self.demo_open_menu = true,
                "tree" => {}
                _ => {
                    self.libui.open = true;
                    self.libui.tab = 0;
                }
            }
            Ok(())
        })();
        if let Err(e) = r {
            self.status = format!("--in-use: {e}");
        }
    }
}

/// The in-use note on a list row (CU10): "In use by Sam on PROG-2 since
/// 09:14", in the warning colours.
pub fn in_use_chip(ui: &mut egui::Ui, note: &str) -> egui::Response {
    crate::ui_parts::chip(ui, &format!("🔒 {note}"), th().warn_bg, th().warn_fg).on_hover_text("Another PC has this job open, or unsaved changes in it. Opening it asks first; it can't be deleted, renamed or moved until they close it (or it's taken over).")
}

#[cfg(test)]
mod inuseui_tests {
    use crate::engine::inuse;
    use crate::library::{self, Kind, Library, SaveTo};
    use crate::shared::{Clock, Who};
    use crate::App;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    /// 2026-11-09 09:14 UTC: a test clock shows UTC.
    const NOV_9_0914: u64 = 1_794_215_640;

    fn new_library(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestpro_inuseui_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        library::create(&dir).unwrap();
        dir
    }

    /// `user`'s window on PC `pc` on the Library in `dir`, with the concurrency
    /// switches `warn` and `allow` (CU1), and no example jobs open but one
    /// empty job.
    fn window(dir: &Path, user: &str, pc: &str, clock: &Clock, warn: bool, allow: bool) -> App {
        let mut a = App::new(None);
        a.cfg.library_dir = dir.display().to_string();
        a.cfg.in_use_warn = warn;
        a.cfg.in_use_allow = allow;
        a.who = Who::new(user, pc);
        a.clock = clock.clone();
        a.jobs.truncate(1);
        a.cur_job = 0;
        a.job_lib_id = None;
        a.jobs[0].lib_id = None;
        a
    }

    /// Library job `name` (one part), saved by kim on PROG-3.
    fn saved(dir: &Path, name: &str) -> u64 {
        let spec = crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 10.0, level: 3, machine: Default::default(), kerf: Default::default() };
        let mut j = crate::jobfile::JobFile::new_job(name, spec);
        j.week = "Acme".into();
        j.parts.push(crate::engine::shape_part(crate::nest::Shape::Rectangle, 1, 1));
        Library::open(dir, &Who::new("kim", "PROG-3"), &Clock::at(NOV_9_0914)).unwrap().save_job(&j, SaveTo::New).unwrap().id
    }

    fn holder(dir: &Path, id: u64) -> Option<String> {
        std::fs::read(inuse::path(dir, id)).ok().and_then(|b| serde_json::from_slice::<inuse::Hold>(&b).ok()).map(|h| h.by.label())
    }

    fn open_names(a: &App) -> Vec<String> {
        a.jobs.iter().map(|j| j.name.clone()).collect()
    }

    /// Every 30 s for `minutes`, each window renews (its timer).
    fn renewing(clock: &Clock, minutes: u64, wins: &mut [&mut App]) {
        for _ in 0..minutes * 2 {
            clock.advance(inuse::RENEW_EVERY);
            for w in wins.iter_mut() {
                w.renew_holds();
            }
        }
    }

    /// CU3, CU8, CU10: PC 1 opens DEMO-7 and holds it; PC 2 sees it in use
    /// with PC 1's user and PC and since when, and each combination of PC 2's
    /// two switches does as CU3 says.
    #[test]
    fn a_job_open_on_one_pc_is_in_use_on_the_other_as_its_switches_say() {
        let dir = new_library("switches");
        let id = saved(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let mut pc1 = window(&dir, "Sam", "PROG-2", &clock, true, true);
        pc1.open_library_job(id);
        assert_eq!(pc1.job_name, "DEMO-7");
        assert_eq!(holder(&dir, id).as_deref(), Some("Sam on PROG-2"));
        clock.advance(Duration::from_secs(60));
        // Warn on, allow on: asked; Open anyway opens it, the hold stays PC 1's.
        let mut pc2 = window(&dir, "sam", "PROG-1", &clock, true, true);
        pc2.lib_refresh();
        assert_eq!(pc2.in_use_note(id).as_deref(), Some("In use by Sam on PROG-2 since 09:14"));
        pc2.open_library_job(id);
        let ask = pc2.inuse.ask.clone().expect("asked");
        assert_eq!(ask.u.said(&clock), "DEMO-7 is in use by Sam on PROG-2 since 09:14.");
        assert!(!open_names(&pc2).contains(&"DEMO-7".to_string()), "opened before the answer");
        pc2.open_anyway();
        assert_eq!(pc2.job_name, "DEMO-7");
        assert_eq!(holder(&dir, id).as_deref(), Some("Sam on PROG-2"));
        assert!(pc2.inuse.busy.contains_key(&id) && !pc2.inuse.held.contains(&id));
        // Warn on, allow off: asked with no Open anyway; it isn't opened.
        let mut pc3 = window(&dir, "lee", "PROG-3", &clock, true, false);
        pc3.open_library_job(id);
        assert!(pc3.inuse.ask.is_some());
        pc3.open_anyway();
        assert!(!open_names(&pc3).contains(&"DEMO-7".to_string()), "opened with allow off");
        // Warn off: nothing asked, it opens, and nothing is held (CU1).
        let mut pc4 = window(&dir, "kim", "PROG-4", &clock, false, true);
        pc4.open_library_job(id);
        assert!(pc4.inuse.ask.is_none());
        assert_eq!(pc4.job_name, "DEMO-7");
        assert!(pc4.inuse.held.is_empty());
        assert_eq!(holder(&dir, id).as_deref(), Some("Sam on PROG-2"));
        // PC 1 with warn off: it doesn't hold what it opens, so others open
        // it freely.
        let other = saved(&dir, "DEMO-8");
        pc4.open_library_job(other);
        assert_eq!(holder(&dir, other), None);
        pc3.open_library_job(other);
        assert!(pc3.inuse.ask.is_none());
        assert_eq!(holder(&dir, other).as_deref(), Some("lee on PROG-3"));
        // Turning warn off lets go at once (CU1).
        pc1.cfg.in_use_warn = false;
        pc1.sync_holds();
        assert_eq!(holder(&dir, id), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU7: PC 1 stops renewing (a crash); about 2 minutes later PC 2 opens
    /// the job freely and holds it. While PC 1 renews, it never does.
    #[test]
    fn a_pc_that_stops_renewing_lets_go_after_about_two_minutes() {
        let dir = new_library("expiry");
        let id = saved(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let mut pc1 = window(&dir, "Sam", "PROG-2", &clock, true, true);
        let mut pc2 = window(&dir, "sam", "PROG-1", &clock, true, true);
        pc1.open_library_job(id);
        pc2.lib_refresh();
        assert!(pc2.in_use_note(id).is_some());
        renewing(&clock, 5, &mut [&mut pc1, &mut pc2]);
        pc2.open_library_job(id);
        assert!(pc2.inuse.ask.take().is_some(), "a renewed hold looked gone");
        // PC 1 crashes: only PC 2's window goes on.
        renewing(&clock, 2, &mut [&mut pc2]);
        assert_eq!(pc2.in_use_note(id), None, "still in use 2 minutes after the last renewal");
        pc2.open_library_job(id);
        assert!(pc2.inuse.ask.is_none());
        assert_eq!(pc2.job_name, "DEMO-7");
        assert_eq!(holder(&dir, id).as_deref(), Some("sam on PROG-1"));
        // PC 1 back: told at its next renewal; its job stays open.
        pc1.renew_holds();
        assert!(pc1.inuse.told.is_some() && pc1.inuse.lost.contains_key(&id));
        assert_eq!(pc1.job_name, "DEMO-7");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU9: Take over (after its warning) moves the hold; PC 1 is told at its
    /// next renewal, keeps its unsaved changes, is asked rather than saving
    /// over the job, and saves them as a copy. The job stays PC 2's.
    #[test]
    fn take_over_moves_the_hold_and_the_loser_saves_its_changes_as_a_copy() {
        let dir = new_library("take_over");
        let id = saved(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let mut pc1 = window(&dir, "Sam", "PROG-2", &clock, true, true);
        pc1.open_library_job(id);
        pc1.parts[0].qty = 40;
        clock.advance(Duration::from_secs(6 * 60));
        // PC 2 (allow off): asked; Take over shows the warning first.
        let mut pc2 = window(&dir, "sam", "PROG-1", &clock, true, false);
        pc2.open_library_job(id);
        pc2.inuse.ask.as_mut().unwrap().warning = true;
        pc2.take_over_job();
        assert_eq!(pc2.job_name, "DEMO-7");
        assert_eq!(holder(&dir, id).as_deref(), Some("sam on PROG-1"));
        assert!(pc2.status.contains("Took over DEMO-7 from Sam on PROG-2"), "{}", pc2.status);
        // PC 1 is told at its next renewal; its changes stay.
        clock.advance(inuse::RENEW_EVERY);
        pc1.renew_holds();
        let told = pc1.inuse.told.clone().expect("told");
        assert_eq!(told.note(&clock), "In use by sam on PROG-1 since 09:20");
        assert_eq!((pc1.job_name.as_str(), pc1.parts[0].qty), ("DEMO-7", 40));
        // Saving over it is refused and asks; nothing was saved.
        let record = std::fs::read(Library::at(&dir, &pc1.who, &clock).path(Kind::Job, id)).unwrap();
        pc1.save_job_to_library();
        assert!(matches!(&pc1.inuse.ask, Some(a) if a.what == super::Opening::Save), "{}", pc1.status);
        assert!(pc1.status.contains("in use by sam on PROG-1 since 09:20"), "{}", pc1.status);
        assert_eq!(std::fs::read(Library::at(&dir, &pc1.who, &clock).path(Kind::Job, id)).unwrap(), record);
        // Saved as a copy: a new job with PC 1's changes; DEMO-7 stays PC 2's.
        pc1.inuse.ask = None;
        pc1.save_in_use_copy(&told);
        assert_eq!((pc1.job_name.as_str(), pc1.parts[0].qty), ("DEMO-7 (1)", 40));
        assert_ne!(pc1.job_lib_id, Some(id));
        assert!(pc1.inuse.lost.is_empty());
        assert_eq!(holder(&dir, id).as_deref(), Some("sam on PROG-1"));
        assert_eq!(holder(&dir, pc1.job_lib_id.unwrap()).as_deref(), Some("Sam on PROG-2"));
        assert_eq!(std::fs::read(Library::at(&dir, &pc1.who, &clock).path(Kind::Job, id)).unwrap(), record);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU3, CU8: PC 2 opens DEMO-7 while PC 1 holds it, with Open anyway,
    /// then again with its warn switch off, and saves: the save goes through
    /// and PC 1 keeps its hold. PC 1 then saves its own changes and the save
    /// check asks, naming PC 2 (LB7, LB9); nothing is lost: the record keeps
    /// PC 2's save, and PC 1 still has its changes, which it saves as a copy.
    #[test]
    fn a_job_opened_anyway_or_with_warn_off_saves_as_usual_and_the_holder_is_asked() {
        use crate::libui::SaveQuestion;
        for warn_off in [false, true] {
            let dir = new_library(if warn_off { "save_warn_off" } else { "save_anyway" });
            let id = saved(&dir, "DEMO-7");
            let clock = Clock::at(NOV_9_0914);
            let lib = Library::at(&dir, &Who::new("kim", "PROG-3"), &clock);
            let mut pc1 = window(&dir, "Sam", "PROG-2", &clock, true, true);
            pc1.open_library_job(id);
            pc1.parts[0].qty = 40;
            clock.advance(Duration::from_secs(60));
            let mut pc2 = window(&dir, "sam", "PROG-1", &clock, !warn_off, true);
            pc2.open_library_job(id);
            if warn_off {
                assert!(pc2.inuse.ask.is_none(), "warn off asked");
            } else {
                assert!(pc2.inuse.ask.is_some(), "not asked");
                pc2.open_anyway();
            }
            assert_eq!(pc2.job_name, "DEMO-7");
            pc2.parts[0].qty = 25;
            pc2.save_job_to_library();
            assert!(pc2.inuse.ask.is_none() && pc2.libui.question.is_none(), "PC 2's save asked: {}", pc2.status);
            assert_eq!(pc2.job_lib_id, Some(id));
            assert_eq!(lib.load_job(id).unwrap().parts[0].qty, 25, "PC 2's save didn't land");
            assert_eq!(holder(&dir, id).as_deref(), Some("Sam on PROG-2"), "PC 1 lost its hold");
            // PC 1 saves: the save check asks, naming PC 2; nothing written.
            clock.advance(Duration::from_secs(60));
            pc1.save_job_to_library();
            let Some(SaveQuestion::Job(c)) = pc1.libui.question.clone() else { panic!("PC 1 wasn't asked: {}", pc1.status) };
            assert_eq!(c.saved_by(&clock), "sam on PROG-1, 9 Nov 09:15");
            assert!(pc1.inuse.ask.is_none());
            assert_eq!((lib.load_job(id).unwrap().parts[0].qty, pc1.parts[0].qty), (25, 40));
            // PC 1 keeps its changes as a copy; PC 2's save stays.
            pc1.save_mine_as_copy();
            assert_eq!((pc1.job_name.as_str(), pc1.parts[0].qty), ("DEMO-7 (1)", 40), "{}", pc1.status);
            assert_eq!(lib.load_job(pc1.job_lib_id.unwrap()).unwrap().parts[0].qty, 40);
            assert_eq!(lib.load_job(id).unwrap().parts[0].qty, 25);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// CU5: the job in front is held; switching to another lets go of it
    /// unless it has unsaved changes; closing it, and quitting, let go too.
    #[test]
    fn a_job_with_unsaved_changes_stays_in_use_after_switching_away_and_one_without_doesnt() {
        let dir = new_library("switching");
        let (a, b) = (saved(&dir, "DEMO-7"), saved(&dir, "DEMO-8"));
        let clock = Clock::at(NOV_9_0914);
        let mut pc = window(&dir, "Sam", "PROG-2", &clock, true, true);
        pc.open_library_job(a);
        pc.open_library_job(b);
        // DEMO-7, unchanged and no longer in front, is let go.
        assert_eq!((holder(&dir, a), holder(&dir, b).as_deref()), (None, Some("Sam on PROG-2")));
        // Changed, then left: still held.
        pc.parts[0].qty += 3;
        let i = pc.jobs.iter().position(|j| j.lib_id == Some(a)).unwrap();
        pc.switch_to_job(i);
        assert_eq!((holder(&dir, a).as_deref(), holder(&dir, b).as_deref()), (Some("Sam on PROG-2"), Some("Sam on PROG-2")));
        let k = pc.jobs.iter().position(|j| j.lib_id == Some(b)).unwrap();
        pc.switch_to_job(k);
        assert_eq!(holder(&dir, a), None);
        assert_eq!(holder(&dir, b).as_deref(), Some("Sam on PROG-2"), "unsaved changes keep it in use");
        // Renewals keep both.
        renewing(&clock, 3, &mut [&mut pc]);
        assert_eq!(holder(&dir, b).as_deref(), Some("Sam on PROG-2"));
        // Saved, then left: let go.
        pc.save_job_to_library();
        pc.switch_to_job(pc.jobs.iter().position(|j| j.lib_id == Some(a)).unwrap());
        assert_eq!((holder(&dir, a).as_deref(), holder(&dir, b)), (Some("Sam on PROG-2"), None));
        // Closing the job in front lets go of it; quitting lets go of all.
        let k = pc.jobs.iter().position(|j| j.lib_id == Some(a)).unwrap();
        pc.close_job(k);
        assert_eq!(holder(&dir, a), None, "{}", pc.status);
        pc.release_holds_on_quit();
        assert!(std::fs::read_dir(dir.join(inuse::FOLDER)).map_or(0, |d| d.count()) == 0, "a hold was left on quitting");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU11: PC 1 has DEMO-7 open, filed three folders down. On PC 2,
    /// deleting it from the Library, renaming it, moving it, and renaming,
    /// moving or deleting a folder above it are refused with who and since
    /// when, its warn switch on or off, and nothing changes. PC 1 moves its
    /// own job freely. Once PC 1 closes it, PC 2 deletes it.
    #[test]
    fn deleting_renaming_or_moving_a_job_in_use_or_its_folders_is_refused_with_who_and_since_when() {
        use crate::folders::TOP;
        use crate::libui::LibItem;
        let dir = new_library("refusals");
        let id = saved(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let mut pc2 = window(&dir, "sam", "PROG-1", &clock, true, true);
        pc2.lib_refresh();
        let folder = |a: &mut App, parent: u64, name: &str| {
            let f = a.lib_new_folder(Kind::Job, parent).unwrap();
            assert!(a.lib_rename_folder(Kind::Job, f, name), "{}", a.status);
            f
        };
        // Saved from its Open jobs folder Acme, DEMO-7 is filed in Acme (JX15).
        let acme = pc2.libui.tree.find("Acme").unwrap();
        let nov = folder(&mut pc2, acme, "2026-11");
        let week = folder(&mut pc2, nov, "Week 46");
        pc2.lib_cut(Kind::Job, LibItem::Job(id));
        assert!(pc2.lib_paste(Kind::Job, week), "{}", pc2.status);
        let outline = ["Acme/", "Acme/2026-11/", "Acme/2026-11/Week 46/", "Acme/2026-11/Week 46/DEMO-7"];
        assert_eq!(pc2.library_outline(), outline);
        let mut pc1 = window(&dir, "Sam", "PROG-2", &clock, true, true);
        pc1.open_library_job(id);
        clock.advance(Duration::from_secs(60));
        let said = "DEMO-7 is in use by Sam on PROG-2 since 09:14.";
        for warn in [true, false] {
            pc2.cfg.in_use_warn = warn;
            assert!(!pc2.delete_from_library(Kind::Job, id));
            assert!(pc2.status.contains(said) && pc2.status.contains("Nothing was deleted"), "{}", pc2.status);
            pc2.lib_cut(Kind::Job, LibItem::Job(id));
            assert!(!pc2.lib_paste(Kind::Job, TOP));
            assert!(pc2.status.contains(said) && pc2.status.contains("can't be moved"), "{}", pc2.status);
            assert!(!pc2.lib_rename_folder(Kind::Job, acme, "Acme Ltd"));
            assert_eq!(pc2.status, "'Acme' holds DEMO-7, which is in use by Sam on PROG-2 since 09:14. Try again when they've closed it.");
            pc2.lib_cut(Kind::Job, LibItem::Folder(nov));
            assert!(!pc2.lib_paste(Kind::Job, TOP));
            assert!(pc2.status.starts_with("'Acme/2026-11' holds DEMO-7, which is in use by Sam on PROG-2 since 09:14."), "{}", pc2.status);
            assert!(!pc2.lib_delete_folder(Kind::Job, week));
            assert!(pc2.status.starts_with("'Acme/2026-11/Week 46' holds DEMO-7, which is in use by Sam on PROG-2 since 09:14.") && pc2.status.contains("only an empty folder"), "{}", pc2.status);
            assert_eq!(pc2.library_outline(), outline);
        }
        // Renamed in PC 2's window (opened anyway): refused at once.
        pc2.cfg.in_use_warn = true;
        pc2.open_library_job(id);
        pc2.open_anyway();
        let cur = pc2.cur_job;
        assert!(!pc2.rename_job(cur, "DEMO-7 frames"));
        assert!(pc2.status.contains(said), "{}", pc2.status);
        // PC 1 moves its own job in use freely.
        pc1.lib_refresh();
        pc1.lib_cut(Kind::Job, LibItem::Job(id));
        assert!(pc1.lib_paste(Kind::Job, nov), "{}", pc1.status);
        // Closed on PC 1: PC 2 may delete it, and no hold is left.
        let k = pc1.jobs.iter().position(|j| j.lib_id == Some(id)).unwrap();
        pc1.close_job(k);
        let k = pc2.jobs.iter().position(|j| j.lib_id == Some(id)).unwrap();
        pc2.close_job(k);
        assert!(pc2.delete_from_library(Kind::Job, id), "{}", pc2.status);
        assert_eq!(holder(&dir, id), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB11, LB13, CU6: the Library can't be reached while a job is held:
    /// the renewal says so once, not every 30 s; the job stays open with its
    /// changes and nothing is made in the Library's place. Back again, the
    /// hold is renewed.
    #[test]
    fn out_of_reach_while_holding_is_said_once_and_the_job_stays_open() {
        let d = std::env::temp_dir().join(format!("nestpro_inuseui_reach_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let dir = d.join("Nesting");
        library::create(&dir).unwrap();
        let id = saved(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let mut pc = window(&dir, "Sam", "PROG-2", &clock, true, true);
        pc.open_library_job(id);
        pc.parts[0].qty = 9;
        std::fs::rename(&dir, d.join("away")).unwrap();
        clock.advance(inuse::RENEW_EVERY);
        pc.renew_holds();
        let said = pc.status.clone();
        assert!(said.contains("can't be reached") && said.contains("stay open"), "{said}");
        pc.status.clear();
        renewing(&clock, 2, &mut [&mut pc]);
        assert_eq!(pc.status, "", "said again");
        assert!(!dir.exists(), "made in the Library's place");
        assert_eq!((pc.job_name.as_str(), pc.parts[0].qty), ("DEMO-7", 9));
        std::fs::rename(d.join("away"), &dir).unwrap();
        clock.advance(inuse::RENEW_EVERY);
        pc.renew_holds();
        assert!(pc.status.contains("reached again"), "{}", pc.status);
        assert!(pc.inuse.held.contains(&id) && pc.inuse.lost.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
