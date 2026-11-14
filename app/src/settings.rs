//! The app's own settings, saved in %APPDATA%\nestpro\settings.ini (~/.config/nestpro/settings.ini on Linux): database
//! connections, machine profiles (how each machine's programs are written),
//! material codes and folders. Nothing here is tied to one install — Lantek
//! Expert's settings can be imported, but every value can be changed.

use std::path::{Path, PathBuf};

/// How a machine's programs are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// ISO G-code (.CNC etc.), shaped by a post library (.CFF/.CF1) or the
    /// built-in defaults.
    Iso,
    /// BLM / Siemens 840D: sheet .SPF + one .SPF per part + list, zipped,
    /// with the .XML job sheet (see blm.rs).
    BlmSiemens,
}

impl Output {
    pub fn key(self) -> &'static str {
        match self {
            Output::Iso => "iso",
            Output::BlmSiemens => "blm",
        }
    }
    pub fn from_key(s: &str) -> Self {
        if s.eq_ignore_ascii_case("blm") { Output::BlmSiemens } else { Output::Iso }
    }
}

/// A SQL Server database the app can open.
#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    pub name: String,
    /// `host`, `host\instance` or `host,port`.
    pub server: String,
    pub database: String,
    /// Sign in with the Windows account (otherwise `user` + password).
    pub windows: bool,
    pub user: String,
    /// The password encrypted for this Windows account (DPAPI), or empty.
    pub password_enc: String,
}

impl Default for Connection {
    fn default() -> Self {
        Connection { name: "New connection".into(), server: String::new(), database: String::new(), windows: true, user: String::new(), password_enc: String::new() }
    }
}

/// A machine and how its programs are written.
#[derive(Clone, Debug, PartialEq)]
pub struct Machine {
    pub name: String,
    pub output: Output,
    /// Post library (.CFF/.CF1) for ISO output; empty = built-in values.
    pub post_lib: String,
    /// For BLM output: the post's .PS1 and .CF1 (trim overruns, corner angle,
    /// arc tolerance); empty = built-in values.
    pub blm_ps1: String,
    pub blm_cf1: String,
    pub gas_id: u32,
    pub gas_name: String,
    /// What the machine does (laser, plasma, punch …): picking the machine
    /// on the Sheet tab sets the job to it (MK1).
    pub kind: crate::machine::MachineKind,
    /// The sheet and hole order changed in Settings → Machines: they win over
    /// the machine file's (CO16, CO17). Empty = the file's (settings saved
    /// before them have none).
    pub order: crate::ct1::OrderChanges,
}

impl Default for Machine {
    fn default() -> Self {
        Machine { name: "Laser 01".into(), output: Output::Iso, post_lib: String::new(), blm_ps1: String::new(), blm_cf1: String::new(), gas_id: 1, gas_name: "Nitrogen".into(), kind: Default::default(), order: Default::default() }
    }
}

/// Everything saved.
#[derive(Clone, Debug, PartialEq)]
pub struct AppSettings {
    /// A Lantek Expert install to import from (optional).
    pub lantek_dir: String,
    /// Where programs are exported; empty = Documents\Sheet Nesting.
    pub export_dir: String,
    /// The app's own product and job library (a local or shared folder);
    /// empty = Documents\Sheet Nesting\Library.
    pub library_dir: String,
    /// Start with the example jobs (W09_10 … W12).
    pub examples: bool,
    /// The active job: its library record, reopened at start (the original's
    /// ORDENACT.SYS, AJ1–AJ2). Empty = none.
    pub active_job: String,
    /// Job files opened or saved lately, newest first (the Open job menu).
    pub recent_jobs: Vec<String>,
    /// Folders part drawings were imported from lately, newest first (the
    /// import window opens in the first, IM10).
    pub import_dirs: Vec<String>,
    /// Show the selected drawing in the import window (IM5, IM10; default on).
    pub import_preview: bool,
    /// Part data in drawing file names, e.g. [Reference]_[Quantity]_[Material]; empty = off.
    pub name_format: String,
    /// Text put before / after every imported part's reference (FN5).
    pub ref_prefix: String,
    pub ref_suffix: String,
    pub connections: Vec<Connection>,
    pub active_conn: usize,
    pub machines: Vec<Machine>,
    pub active_machine: usize,
    /// Per material of the app (mild steel, stainless, aluminium): the name
    /// and code written into machine programs.
    pub materials: Vec<(String, String)>,
    /// Other names each material goes by (comma-separated): grades, norms,
    /// shop names. A name that is an alias stands for its main material, as
    /// in the original (MA6).
    pub material_aliases: Vec<String>,
    /// Per material: its key in the machine file's material × thickness
    /// sections (a catalogue number such as 0010, or a plain name; empty = the
    /// material's name in programs). MTL2.
    pub material_refs: Vec<String>,
    /// Appearance: "system" (follow Windows), "light" or "dark".
    pub theme: String,
    /// Ask before changing a part that has its own technology.
    pub warn_tech: bool,
    /// Concurrency management (CU1), kept on this PC only: warn when a job
    /// is in use by someone else (on in a new install: the rebuild's own
    /// default, spec #44), and allow opening it anyway (the original's
    /// default, on). With warn off this PC neither holds the jobs it opens
    /// nor checks anyone else's on opening.
    pub in_use_warn: bool,
    pub in_use_allow: bool,
    /// Show the original's commands that aren't built yet (greyed).
    pub show_unbuilt: bool,
    /// At start-up, load the active database's jobs into the Job Explorer
    /// when the connection can sign in by itself.
    pub auto_db: bool,
    /// Interface size, as Expert's "Ribbon font" (LANTEKIN.CNI A213):
    /// 0 system default, 1 normal, 2 large, 3 extra large.
    pub ui_size: u8,
    /// Mouse-wheel scroll speed, × egui's 40 px per wheel line.
    pub scroll_speed: f32,
    /// Sheet stock and remnant options.
    pub stock: crate::stock::StockSettings,
    /// Moving parts by hand, units, and the part gap and edge margin new jobs start with.
    pub nesting: NestPrefs,
    /// ERP / MES systems the app talks to (one per provider).
    pub integrations: Vec<crate::integrations::Integration>,
    /// Auto-refresh (the original's Z076, JX8, JX9): 0 = off, as on a fresh
    /// install; else the full reload's period in whole minutes, 1 to 1440.
    /// While on, a quick check every few seconds shows other PCs' saves
    /// (LB14). Kept on each PC.
    pub refresh_minutes: u32,
}

/// Moving parts by hand on the sheet, and the units lengths are shown in
/// (Settings → Nesting; MECHANICS "Moving parts by hand", HM1–HM12).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct NestPrefs {
    /// Units for the part gap, edge margin and moves (the app keeps mm inside).
    pub units: crate::units::LenUnit,
    /// Part gap a new job starts with, mm.
    pub part_gap: f64,
    /// Edge margin (distance from the sheet edge) a new job starts with, mm.
    pub edge_margin: f64,
    /// The movement value: how far Alt+arrow moves the selected parts, mm (HM3, HM4).
    pub step: f64,
    /// Ctrl+9 turns clockwise: the original's "drag rotation direction" switched
    /// off, which negates its fixed-angle turn keys (HM5).
    pub keys_clockwise: bool,
    /// Snap dragged parts to one part gap from a neighbour or the margin (HM9).
    pub snap: bool,
    /// How near (screen pixels) a dragged part must come before it snaps.
    pub snap_px: f32,
    /// Show the part-gap halo around parts being moved (HM11).
    pub halo: bool,
    /// The rotation step, degrees: what the turn bar's buttons and Ctrl+Q /
    /// Ctrl+W turn by and what a turn by the handle snaps to; 0 = free (HM16).
    pub turn_step: f64,
    /// Show the turn bar under the selected parts (HM17).
    pub turn_bar: bool,
}

/// Rotation step the app starts with, degrees (HM16).
pub const TURN_STEP_DEFAULT: f64 = 15.0;
/// The largest rotation step, degrees.
pub const TURN_STEP_MAX: f64 = 180.0;

/// Movement value the app starts with, mm: the original's factory value (HM4).
pub const STEP_DEFAULT: f64 = 10.0;

impl Default for NestPrefs {
    fn default() -> Self {
        // The gap outline starts off, as in the original (HM11).
        NestPrefs { units: Default::default(), part_gap: 5.0, edge_margin: 5.0, step: STEP_DEFAULT, keys_clockwise: false, snap: true, snap_px: crate::ghost::SNAP_PX, halo: false, turn_step: TURN_STEP_DEFAULT, turn_bar: true }
    }
}

impl NestPrefs {
    /// Take the original's own values for moving parts from its LANTEKPR.CFG
    /// text: A222 movement value (mm), A074 turn direction (OFF = the
    /// fixed-angle keys turn clockwise), V025 gap outline.
    pub fn import_cfg(&mut self, cfg: &str) {
        let val = |k: &str| cfg.lines().find(|l| l.split_whitespace().next() == Some(k)).and_then(|l| l.split_whitespace().nth(1)).map(str::to_string);
        if let Some(v) = val("A222").and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0 && *v <= 1000.0) {
            self.step = v;
        }
        if let Some(v) = val("A074") {
            self.keys_clockwise = v.eq_ignore_ascii_case("OFF");
        }
        if let Some(v) = val("V025") {
            self.halo = v.eq_ignore_ascii_case("ON");
        }
    }

    /// Values kept in their ranges (a hand-edited settings file).
    pub fn sane(mut self) -> Self {
        let d = NestPrefs::default();
        let fix = |v: f64, lo: f64, hi: f64, def: f64| if v.is_finite() && (lo..=hi).contains(&v) { v } else { def };
        self.part_gap = fix(self.part_gap, 0.0, 50.0, d.part_gap);
        self.edge_margin = fix(self.edge_margin, 0.0, 100.0, d.edge_margin);
        self.step = fix(self.step, 0.001, 1000.0, d.step);
        self.turn_step = fix(self.turn_step, 0.0, TURN_STEP_MAX, d.turn_step);
        self.snap_px = if self.snap_px.is_finite() && (2.0..=60.0).contains(&self.snap_px) { self.snap_px } else { d.snap_px };
        self
    }
}

/// The mouse-wheel scroll speed the app starts with (× 40 px per line).
pub const SCROLL_SPEED_DEFAULT: f32 = 2.0;

/// Interface sizes (LANTEKIN.CNI A213; LK4).
pub const UI_SIZES: [&str; 4] = ["System default", "Normal", "Large", "Extra large"];

/// A213 as Expert reads it: anything above 3 is 0 (system default).
pub fn ui_size_from(v: i64) -> u8 {
    if (0..=3).contains(&v) { v as u8 } else { 0 }
}

/// How much bigger each interface size draws the app.
pub fn ui_zoom(size: u8) -> f32 {
    match size {
        2 => UI_ZOOM_LARGE,
        3 => UI_ZOOM_XLARGE,
        _ => 1.0,
    }
}
pub const UI_ZOOM_LARGE: f32 = 1.15;
pub const UI_ZOOM_XLARGE: f32 = 1.3;

/// Expert's ribbon styles (LANTEKIN.CNI A212; each has its own theme file,
/// LK2). Only
/// "Office 2013 Gray Dark" is a dark style.
pub const LANTEK_STYLES: [(&str, bool); 5] = [
    ("Office 2013 Blue", false),
    ("Office 2013 Gray Dark", true),
    ("Office 2013 Gray Light", false),
    ("Office 2016 Blue", false),
    ("Office 2016 White", false),
];

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            lantek_dir: String::new(),
            export_dir: String::new(),
            library_dir: String::new(),
            examples: true,
            active_job: String::new(),
            recent_jobs: vec![],
            import_dirs: vec![],
            import_preview: true,
            name_format: String::new(),
            ref_prefix: String::new(),
            ref_suffix: String::new(),
            connections: vec![],
            active_conn: 0,
            machines: vec![Machine::default()],
            active_machine: 0,
            materials: crate::engine::material::default_pc_programs(),
            material_aliases: crate::engine::material::default_pc_other_names(),
            material_refs: crate::engine::material::default_pc_machine_refs(),
            theme: "system".into(),
            warn_tech: true,
            in_use_warn: true,
            in_use_allow: true,
            show_unbuilt: false,
            auto_db: true,
            ui_size: 0,
            scroll_speed: SCROLL_SPEED_DEFAULT,
            stock: Default::default(),
            nesting: Default::default(),
            integrations: default_integrations(),
            refresh_minutes: 0,
        }
    }
}

/// One entry per known system, ready to fill in.
pub fn default_integrations() -> Vec<crate::integrations::Integration> {
    crate::integrations::Provider::ALL.into_iter().map(crate::integrations::Integration::new).collect()
}

/// Where a Lantek Expert install usually is on Windows.
pub const LANTEK_DEFAULT_DIR: &str = r"C:\Lantek\Expert";

/// The Lantek install folder to look in when none is set: `LANTEK_DIR` from
/// the environment, else the usual Windows folder (on other systems, a copy
/// of the install has to be pointed at with `LANTEK_DIR`).
pub fn lantek_default_dir() -> String {
    match std::env::var("LANTEK_DIR") {
        Ok(d) if !d.trim().is_empty() => d,
        _ => LANTEK_DEFAULT_DIR.to_string(),
    }
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unesc(s: &str) -> String {
    let mut o = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => o.push('\n'),
                Some(x) => o.push(x),
                None => {}
            }
        } else {
            o.push(c);
        }
    }
    o
}

impl AppSettings {
    pub fn file() -> PathBuf {
        // Tests never touch this PC's own settings: what they save goes to a
        // folder of the test run's own.
        if cfg!(test) {
            return std::env::temp_dir().join(format!("nestpro_test_settings_{}", std::process::id())).join("settings.ini");
        }
        // %APPDATA% on Windows; $XDG_CONFIG_HOME or ~/.config elsewhere.
        let base = std::env::var_os("APPDATA")
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("nestpro").join("settings.ini")
    }

    /// The app's material for a name from outside (an import, a program, a
    /// file name): the material's own name, its name in programs, its code or
    /// one of its aliases, without case. As in the original (MA6), an
    /// alias gives its main material; an unknown name gives None.
    pub fn material_for(&self, name: &str) -> Option<usize> {
        crate::engine::material::find(self, name)
    }

    pub fn conn(&self) -> Option<&Connection> {
        self.connections.get(self.active_conn)
    }

    /// A system's settings (always present after load).
    pub fn integration(&self, p: crate::integrations::Provider) -> crate::integrations::Integration {
        self.integrations.iter().find(|i| i.provider == p).cloned().unwrap_or_else(|| crate::integrations::Integration::new(p))
    }

    pub fn integration_mut(&mut self, p: crate::integrations::Provider) -> &mut crate::integrations::Integration {
        if !self.integrations.iter().any(|i| i.provider == p) {
            self.integrations.push(crate::integrations::Integration::new(p));
        }
        self.integrations.iter_mut().find(|i| i.provider == p).expect("just added")
    }

    pub fn machine(&self) -> Machine {
        self.machines.get(self.active_machine).cloned().unwrap_or_default()
    }

    /// The settings as text (`[section]` + `key = value` lines).
    pub fn to_text(&self) -> String {
        let mut s = String::from("# nestpro settings\n[general]\n");
        s += &format!("lantek_dir = {}\nexport_dir = {}\nactive_conn = {}\nactive_machine = {}\n", esc(&self.lantek_dir), esc(&self.export_dir), self.active_conn, self.active_machine);
        s += &format!("library_dir = {}\nexamples = {}\nname_format = {}\n", esc(&self.library_dir), self.examples, esc(&self.name_format));
        s += &format!("ref_prefix = {}\nref_suffix = {}\nactive_job = {}\n", esc(&self.ref_prefix), esc(&self.ref_suffix), esc(&self.active_job));
        for r in &self.recent_jobs {
            s += &format!("recent_job = {}\n", esc(r));
        }
        for d in &self.import_dirs {
            s += &format!("import_dir = {}\n", esc(d));
        }
        s += &format!("import_preview = {}\n", self.import_preview);
        s += &format!("theme = {}\nwarn_tech = {}\nshow_unbuilt = {}\nauto_db = {}\n", self.theme, self.warn_tech, self.show_unbuilt, self.auto_db);
        s += &format!("in_use_warn = {}\nin_use_allow = {}\n", self.in_use_warn, self.in_use_allow);
        s += &format!("ui_size = {}\nscroll_speed = {}\n", self.ui_size, self.scroll_speed);
        s += &format!("refresh_minutes = {}\n", self.refresh_minutes);
        s += &format!("stock = {}\n", serde_json::to_string(&self.stock).unwrap_or_default());
        s += &format!("nesting = {}\n", serde_json::to_string(&self.nesting).unwrap_or_default());
        crate::engine::material::write_pc(self, &mut s, |x| esc(x));
        for c in &self.connections {
            s += &format!(
                "[connection]\nname = {}\nserver = {}\ndatabase = {}\nwindows = {}\nuser = {}\npassword_enc = {}\n",
                esc(&c.name),
                esc(&c.server),
                esc(&c.database),
                c.windows,
                esc(&c.user),
                esc(&c.password_enc)
            );
        }
        for m in &self.machines {
            s += &format!(
                "[machine]\nname = {}\noutput = {}\npost_lib = {}\nblm_ps1 = {}\nblm_cf1 = {}\ngas_id = {}\ngas_name = {}\nkind = {}\n",
                esc(&m.name),
                m.output.key(),
                esc(&m.post_lib),
                esc(&m.blm_ps1),
                esc(&m.blm_cf1),
                m.gas_id,
                esc(&m.gas_name),
                m.kind.key()
            );
            // Only a machine whose order was changed in Settings → Machines
            // writes it (CO16, CO17): an unchanged one saves as before.
            if !m.order.is_empty() {
                s += &format!("order = {}\n", serde_json::to_string(&m.order).unwrap_or_default());
            }
        }
        for i in &self.integrations {
            s += &format!(
                "[integration]\nprovider = {}\nenabled = {}\nbase_url = {}\ntoken_enc = {}\nunits = {}\ndrawing_units = {}\napprove = {}\nsoftware = {}\n",
                i.provider.key(),
                i.enabled,
                esc(&i.base_url),
                esc(&i.token_enc),
                i.units.key(),
                i.drawing_units.key(),
                i.approve,
                esc(&i.software)
            );
            s += &format!("work_orders = {}\n", i.work_orders.key());
            // id|name|machine (the id never holds a bar; the machine is last).
            for st in &i.stations {
                s += &format!("station = {}|{}|{}\n", esc(&st.id), esc(&st.name), esc(&st.machine));
            }
            for st in &i.equipment {
                s += &format!("equipment = {}|{}|{}\n", esc(&st.id), esc(&st.name), esc(&st.machine));
            }
        }
        s
    }

    pub fn parse(text: &str) -> Self {
        let mut st = AppSettings { machines: vec![], integrations: vec![], ..Default::default() };
        let mut sec = String::new();
        // An [integration] for a provider this build doesn't know is skipped.
        let mut skip_integration = false;
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            if l.starts_with('[') && l.ends_with(']') {
                sec = l[1..l.len() - 1].to_string();
                match sec.as_str() {
                    "connection" => st.connections.push(Connection::default()),
                    "machine" => st.machines.push(Machine::default()),
                    "integration" => skip_integration = true,
                    _ => {}
                }
                continue;
            }
            let Some((k, v)) = l.split_once('=') else { continue };
            let (k, v) = (k.trim(), unesc(v.trim()));
            match sec.as_str() {
                "general" => match k {
                    "lantek_dir" => st.lantek_dir = v,
                    "export_dir" => st.export_dir = v,
                    "library_dir" => st.library_dir = v,
                    "examples" => st.examples = v != "false",
                    "name_format" => st.name_format = v,
                    "ref_prefix" => st.ref_prefix = v,
                    "ref_suffix" => st.ref_suffix = v,
                    "active_job" => st.active_job = v,
                    "recent_job" => st.recent_jobs.push(v),
                    "import_dir" => st.import_dirs.push(v),
                    "import_preview" => st.import_preview = v != "false",
                    "active_conn" => st.active_conn = v.parse().unwrap_or(0),
                    "active_machine" => st.active_machine = v.parse().unwrap_or(0),
                    "theme" => st.theme = if ["light", "dark", "system"].contains(&v.as_str()) { v } else { "system".into() },
                    "warn_tech" => st.warn_tech = v != "false",
                    "in_use_warn" => st.in_use_warn = v != "false",
                    "in_use_allow" => st.in_use_allow = v != "false",
                    "show_unbuilt" => st.show_unbuilt = v == "true",
                    "auto_db" => st.auto_db = v != "false",
                    "ui_size" => st.ui_size = ui_size_from(v.parse().unwrap_or(0)),
                    "stock" => st.stock = serde_json::from_str(&v).unwrap_or_default(),
                    "nesting" => st.nesting = serde_json::from_str::<NestPrefs>(&v).unwrap_or_default().sane(),
                    "refresh_minutes" => st.refresh_minutes = crate::engine::refresh::period(v.parse::<i64>().unwrap_or(0)),
                    "scroll_speed" => st.scroll_speed = v.parse::<f32>().ok().filter(|x| (0.5..=6.0).contains(x)).unwrap_or(SCROLL_SPEED_DEFAULT),
                    _ if k.starts_with("material") => {
                        crate::engine::material::read_pc(&mut st, k, v);
                    }
                    _ => {}
                },
                "connection" => {
                    if let Some(c) = st.connections.last_mut() {
                        match k {
                            "name" => c.name = v,
                            "server" => c.server = v,
                            "database" => c.database = v,
                            "windows" => c.windows = v == "true",
                            "user" => c.user = v,
                            "password_enc" => c.password_enc = v,
                            _ => {}
                        }
                    }
                }
                "integration" => {
                    use crate::integrations::{Integration, Provider, Units};
                    if k == "provider" {
                        skip_integration = true;
                        if let Some(p) = Provider::from_key(&v) {
                            if !st.integrations.iter().any(|i| i.provider == p) {
                                st.integrations.push(Integration::new(p));
                                skip_integration = false;
                            }
                        }
                        continue;
                    }
                    if skip_integration {
                        continue;
                    }
                    if let Some(i) = st.integrations.last_mut() {
                        match k {
                            "enabled" => i.enabled = v != "false",
                            "base_url" => i.base_url = v,
                            "token_enc" => i.token_enc = v,
                            "units" => i.units = Units::from_key(&v),
                            "drawing_units" => i.drawing_units = Units::from_key(&v),
                            "approve" => i.approve = v == "true",
                            "software" => i.software = v,
                            "work_orders" => i.work_orders = crate::integrations::WorkOrders::from_key(&v),
                            "station" | "equipment" => {
                                if let Some((id, rest)) = v.split_once('|') {
                                    if let Some((name, machine)) = rest.rsplit_once('|') {
                                        let st = crate::integrations::Station { id: id.into(), name: name.into(), machine: machine.into() };
                                        if !st.id.is_empty() && !st.machine.is_empty() {
                                            if k == "station" { i.stations.push(st) } else { i.equipment.push(st) }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "machine" => {
                    if let Some(m) = st.machines.last_mut() {
                        match k {
                            // Settings saved before machines said what they do: guessed
                            // from the name (MK2); a `kind` line after it wins.
                            "name" => {
                                m.kind = crate::machine::MachineKind::guess_from_name(&v);
                                m.name = v;
                            }
                            "output" => m.output = Output::from_key(&v),
                            "post_lib" => m.post_lib = v,
                            "blm_ps1" => m.blm_ps1 = v,
                            "blm_cf1" => m.blm_cf1 = v,
                            "gas_id" => m.gas_id = v.parse().unwrap_or(1),
                            "gas_name" => m.gas_name = v,
                            "kind" => m.kind = crate::machine::MachineKind::from_key(&v).unwrap_or_default(),
                            // Each value on its own: one that can't be read or
                            // taken is left out, the others kept (CO16).
                            "order" => m.order = crate::ct1::OrderChanges::read(&v),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if st.machines.is_empty() {
            st.machines.push(Machine::default());
        }
        for p in crate::integrations::Provider::ALL {
            st.integration_mut(p);
        }
        st.active_conn = st.active_conn.min(st.connections.len().saturating_sub(1));
        st.active_machine = st.active_machine.min(st.machines.len() - 1);
        st
    }

    /// Saved settings, or — the first time — an import of a Lantek install
    /// if there is one, else empty defaults.
    pub fn load() -> Self {
        // A test starts from the defaults: never from what another test
        // saved, nor from a Lantek install on the PC running it.
        if cfg!(test) {
            return AppSettings::default();
        }
        match Self::read_from(&Self::file()) {
            Some(s) => s,
            None => {
                let mut s = AppSettings::default();
                let dir = lantek_default_dir();
                if Path::new(&dir).join("LANTEKIN.CNI").exists() {
                    s.import_lantek(&dir);
                }
                s
            }
        }
    }

    /// The settings saved in `f`, None when it can't be read.
    pub fn read_from(f: &Path) -> Option<Self> {
        std::fs::read_to_string(f).ok().map(|t| Self::parse(&t))
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&Self::file())
    }

    /// Save the settings in `f`.
    pub fn save_to(&self, f: &Path) -> std::io::Result<()> {
        if let Some(d) = f.parent() {
            std::fs::create_dir_all(d)?;
        }
        // Write next to it, then rename, so a crash or a full disk never
        // leaves half a settings file (machines, posts and materials lost).
        let tmp = f.with_extension("tmp");
        std::fs::write(&tmp, self.to_text())?;
        std::fs::rename(&tmp, f)
    }

    /// Add Lantek Expert's database connection and machines from an install
    /// folder (LANTEKIN.CNI, DBConnect.lst, LANTEKPR.CFG, MchFiles). Returns
    /// what was added.
    pub fn import_lantek(&mut self, dir: &str) -> String {
        let p = Path::new(dir);
        let Some(db) = crate::dbconn::read(p) else {
            return format!("No Lantek Expert settings (LANTEKIN.CNI) in {dir}.");
        };
        self.lantek_dir = dir.to_string();
        let name = format!("Lantek — {}", if db.database.is_empty() { "database" } else { &db.database });
        let conn = Connection { name: name.clone(), server: db.server.clone(), database: db.database.clone(), windows: db.user.is_empty(), user: db.user.clone(), password_enc: String::new() };
        match self.connections.iter().position(|c| c.server == conn.server && c.database == conn.database) {
            Some(k) => self.active_conn = k,
            None => {
                self.connections.push(conn);
                self.active_conn = self.connections.len() - 1;
            }
        }
        let mch = p.join("MchFiles");
        let file_if = |f: &Path| if f.exists() { f.display().to_string() } else { String::new() };
        let cff = file_if(&p.join(crate::post::EXPERT_CFF));
        let mut added = 0;
        for m in &db.machines {
            if self.machines.iter().any(|x| &x.name == m) {
                continue;
            }
            let u = m.to_uppercase();
            let blm = u.contains("LC5") || u.contains("BLM");
            self.machines.push(Machine {
                name: m.clone(),
                output: if blm { Output::BlmSiemens } else { Output::Iso },
                post_lib: cff.clone(),
                blm_ps1: if blm { file_if(&mch.join("PstAdi06.PS1")) } else { String::new() },
                blm_cf1: if blm { file_if(&mch.join("PstAdi06.CF1")) } else { String::new() },
                kind: crate::machine::MachineKind::guess_from_name(m),
                ..Default::default()
            });
            added += 1;
        }
        // Drop the untouched starter profile once real machines exist.
        if added > 0 && self.machines.len() > added && self.machines[0] == Machine::default() {
            self.machines.remove(0);
        }
        if let Some(k) = self.machines.iter().position(|x| x.name == db.active_machine) {
            self.active_machine = k;
        }
        // Look: A212 ribbon style (dark only for Office 2013 Gray Dark) and
        // A213 ribbon font size.
        if let Ok(b) = std::fs::read(p.join("LANTEKIN.CNI")) {
            let cni: String = b.iter().map(|&c| c as char).collect();
            let val = |k: &str| cni.lines().find(|l| l.split_whitespace().next() == Some(k)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<i64>().ok());
            if let Some(st) = val("A212").and_then(|v| LANTEK_STYLES.get(v as usize)) {
                self.theme = if st.1 { "dark" } else { "light" }.into();
            }
            if let Some(f) = val("A213") {
                self.ui_size = ui_size_from(f);
            }
            // Z076: the Job Explorer's auto-refresh, minutes (0 = off; JX8, JX9).
            if let Some(m) = val("Z076") {
                self.refresh_minutes = crate::engine::refresh::period(m);
            }
        }
        // Moving parts by hand (LANTEKPR.CFG): the movement value, the turn
        // direction of the fixed-angle keys and the gap outline (HM4, HM5, HM11).
        let mut moves = String::new();
        if let Ok(b) = std::fs::read(p.join("LANTEKPR.CFG")) {
            self.nesting.import_cfg(&b.iter().map(|&c| c as char).collect::<String>());
            moves = format!(" Moving parts: step {}.", self.nesting.units.show_long(self.nesting.step));
        }
        format!("Imported {name} and {added} machine(s) from {dir}.{moves}")
    }
}

/// Encrypt a password for this Windows account (DPAPI, through PowerShell's
/// ConvertFrom-SecureString); only this user on this PC can read it back.
pub fn protect(password: &str) -> Result<String, String> {
    if !cfg!(windows) {
        return Err("remembering passwords needs Windows (DPAPI)".into());
    }
    let script = "$ProgressPreference='SilentlyContinue'; $s = ConvertTo-SecureString $env:NESTPRO_PW -AsPlainText -Force; [Console]::Out.Write(($s | ConvertFrom-SecureString))";
    let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &crate::dbquery::base64(&utf16)])
        .env("NESTPRO_PW", password)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(s)
    } else {
        Err("couldn't encrypt the password".into())
    }
}

/// Read back a secret saved with [`protect`] (Windows only, same user and PC).
pub fn unprotect(enc: &str) -> Result<String, String> {
    if !cfg!(windows) {
        return Err("reading a remembered token needs Windows (DPAPI) — set it for this session or in the environment instead".into());
    }
    if !enc.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("the remembered token is damaged — enter it again".into());
    }
    let script = "$ProgressPreference='SilentlyContinue'; $ss = ConvertTo-SecureString $env:NESTPRO_PWENC; [Console]::Out.Write([Runtime.InteropServices.Marshal]::PtrToStringBSTR([Runtime.InteropServices.Marshal]::SecureStringToBSTR($ss)))";
    let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &crate::dbquery::base64(&utf16)])
        .env("NESTPRO_PWENC", enc)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !s.is_empty() {
        Ok(s)
    } else {
        Err("couldn't read the remembered token (saved by another Windows user or PC?) — enter it again".into())
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn material_names_and_aliases_give_the_main_material() {
        // MA6: an alias gives its
        // main material; an unknown name gives none.
        let s = AppSettings::default();
        assert_eq!(s.material_for("Mild steel"), Some(0));
        assert_eq!(s.material_for("s355"), Some(0));
        assert_eq!(s.material_for(" AISI316 "), Some(1));
        assert_eq!(s.material_for("0100"), Some(1), "the program code");
        assert_eq!(s.material_for("STAINLESS"), Some(1), "the name in programs");
        assert_eq!(s.material_for("5754"), Some(2));
        assert_eq!(s.material_for("Copper"), None);
        assert_eq!(s.material_for(""), None);
    }

    #[test]
    fn moving_part_values_come_from_the_originals_cfg() {
        // HM4, HM5, HM11: a factory file and a shop that works in inches.
        let mut n = NestPrefs::default();
        assert_eq!((n.step, n.keys_clockwise, n.halo), (10.0, false, false));
        n.import_cfg("A074 OFF                   ; x\r\nA222 1.270000              ; y\r\nV025 ON ; z\r\n");
        assert_eq!((n.step, n.keys_clockwise, n.halo), (1.27, true, true));
        n.import_cfg("A222 0.0 ; ignored\nA074 ON\n");
        assert_eq!((n.step, n.keys_clockwise), (1.27, false));
    }

    /// JX8, JX9: importing a Lantek install takes its auto-refresh period
    /// (Z076, whole minutes, 0 = off), kept in 1–1440.
    #[test]
    fn importing_a_lantek_install_takes_its_auto_refresh_period() {
        let dir = std::env::temp_dir().join(format!("nestpro_z076_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (line, want) in [("Z076 5                     ; auto-refresh", 5), ("Z076 0 ; off", 0), ("Z076 9000", 1440)] {
            std::fs::write(dir.join("LANTEKIN.CNI"), format!("R001 \"srv\\SQL\" ; server\r\nR002 \"DEMO\" ; db\r\n{line}\r\n")).unwrap();
            let mut s = AppSettings { refresh_minutes: 7, ..Default::default() };
            s.import_lantek(&dir.display().to_string());
            assert_eq!(s.refresh_minutes, want, "{line}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Tests start from the defaults even on a PC with a Lantek install:
    /// `load` never imports one there. Checked in a process of its own, so
    /// LANTEK_DIR is set for it alone.
    #[test]
    fn tests_start_from_the_defaults_even_with_a_lantek_install() {
        if std::env::var_os("NESTPRO_LOAD_CHILD").is_some() {
            assert_eq!(AppSettings::load().to_text(), AppSettings::default().to_text(), "the Lantek install in LANTEK_DIR was imported");
            return;
        }
        let dir = std::env::temp_dir().join(format!("nestpro_load_install_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("LANTEKIN.CNI"), "R001 \"srv\\SQL\" ; server\r\nR002 \"DEMO\" ; db\r\nZ076 5 ; auto-refresh\r\n").unwrap();
        // Imported on purpose, the install does change the settings.
        let mut imported = AppSettings::default();
        imported.import_lantek(&dir.display().to_string());
        assert_ne!(imported.to_text(), AppSettings::default().to_text());
        let name = "settings::settings_tests::tests_start_from_the_defaults_even_with_a_lantek_install";
        let out = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", name, "--test-threads=1"]).env("LANTEK_DIR", &dir).env("NESTPRO_LOAD_CHILD", "1").output().unwrap();
        let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && said.contains("1 passed"), "{said}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_round_trip_through_text() {
        let s = AppSettings {
            lantek_dir: r"D:\Lantek\Expert".into(),
            export_dir: r"\\server\cnc\out".into(),
            library_dir: r"\\server\shop\library".into(),
            examples: false,
            name_format: "[Reference]_[Quantity]_*".into(),
            ref_prefix: "J88-".into(),
            active_job: "12".into(),
            recent_jobs: vec![r"C:\Jobs\DEMO-001.snjob".into(), r"\\server\jobs\Acme.snjob".into()],
            import_dirs: vec![r"\\server\dxf\Acme".into(), r"C:\Lantek\Expert\Samples\DXF".into()],
            import_preview: false,
            ref_suffix: String::new(),
            connections: vec![
                Connection { name: "Shop".into(), server: r"srv\SQL".into(), database: "SHOP".into(), windows: false, user: "lantek".into(), password_enc: "01000000d08c".into() },
                Connection { name: "Test = 2".into(), ..Default::default() },
            ],
            active_conn: 1,
            machines: vec![Machine { name: "LC5".into(), output: Output::BlmSiemens, blm_ps1: r"C:\x\PstAdi06.PS1".into(), gas_id: 2, gas_name: "Oxygen".into(), ..Default::default() }, Machine { name: "Plasma table".into(), kind: crate::machine::MachineKind::Plasma, ..Default::default() }],
            active_machine: 0,
            materials: vec![("S235".into(), "0010".into()), ("304".into(), "0100".into()), ("5052".into(), "0200".into())],
            material_aliases: vec!["A36, S275".into(), String::new(), "AL".into()],
            material_refs: vec!["10".into(), "AISI304".into(), String::new()],
            theme: "dark".into(),
            warn_tech: false,
            in_use_warn: false,
            in_use_allow: false,
            show_unbuilt: true,
            auto_db: false,
            ui_size: 2,
            scroll_speed: 3.0,
            refresh_minutes: 15,
            stock: crate::stock::StockSettings { order: crate::stock::Order::ByPriority, min_efficiency: 0.6, ..Default::default() },
            nesting: NestPrefs { units: crate::units::LenUnit::Inch, part_gap: 6.35, edge_margin: 6.35, step: 25.4 / 16.0, keys_clockwise: true, snap: false, snap_px: 20.0, halo: true, turn_step: 7.5, turn_bar: false },
            integrations: vec![crate::integrations::Integration {
                provider: crate::integrations::Provider::Fulcrum,
                enabled: false,
                base_url: "https://api.example.com".into(),
                token_enc: "01000000d08c".into(),
                units: crate::integrations::Units::Millimeter,
                drawing_units: crate::integrations::Units::Inch,
                approve: true,
                software: "lantek".into(),
                work_orders: crate::integrations::WorkOrders::App,
                stations: vec![crate::integrations::Station { id: "op1".into(), name: "Laser | fiber".into(), machine: "Fiber 1".into() }],
                equipment: vec![crate::integrations::Station { id: "eq1".into(), name: "FL 6kW".into(), machine: "Fiber 2".into() }],
            }],
        };
        assert_eq!(AppSettings::parse(&s.to_text()), s);
        // MK2: settings saved before machines said what they do guess it from the name.
        let old = AppSettings::parse("[machine]\nname = Plasma 2\noutput = iso\n");
        assert_eq!(old.machines.iter().find(|m| m.name == "Plasma 2").map(|m| m.kind), Some(crate::machine::MachineKind::Plasma));
        // A system this build doesn't know is skipped; every known one is there.
        let odd = AppSettings::parse("[integration]\nprovider = sap\nbase_url = x\n");
        assert_eq!(odd.integrations, default_integrations());
        assert_eq!((ui_size_from(3), ui_size_from(7), ui_size_from(-1)), (3, 0, 0));
        // JX8, JX9: auto-refresh is off unless set; a period outside 1–1440
        // minutes written by hand is off below 1, a day above.
        assert_eq!(AppSettings::default().refresh_minutes, 0);
        assert_eq!(AppSettings::parse("[general]\nrefresh_minutes = 0\n").refresh_minutes, 0);
        assert_eq!(AppSettings::parse("[general]\nrefresh_minutes = -3\n").refresh_minutes, 0);
        assert_eq!(AppSettings::parse("[general]\nrefresh_minutes = 99999\n").refresh_minutes, 1440);
        assert_eq!(AppSettings::parse("[general]\nrefresh_minutes = x\n").refresh_minutes, 0);
        assert!(LANTEK_STYLES[1].1 && !LANTEK_STYLES[3].1);
        // Empty or missing text gives one starter machine and no connection.
        let e = AppSettings::parse("");
        assert_eq!(e.machines.len(), 1);
        assert!(e.connections.is_empty() && e.conn().is_none());
        // CU1: a new install warns when a job is in use (the rebuild's own
        // default) and allows opening it anyway (the original's); settings
        // saved before the switches get the same.
        assert!(AppSettings::default().in_use_warn && AppSettings::default().in_use_allow);
        assert!(e.in_use_warn && e.in_use_allow);
        let off = AppSettings::parse("[general]\nin_use_warn = false\nin_use_allow = false\n");
        assert!(!off.in_use_warn && !off.in_use_allow);
    }

    #[test]
    fn a_machines_order_changes_survive_a_restart_and_use_the_machine_file_puts_the_files_values_back() {
        use crate::ct1::{HoleMode, MachineTech, OrderChanges, OrderSettings};
        let d = std::env::temp_dir().join(format!("nestpro_settings_order_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("MchFiles")).unwrap();
        // A made-up machine file: start bottom off (not followed, CZ6), 20 × 10
        // zones, By proximity.
        std::fs::write(d.join("MchFiles").join("PstAdi06.CT1"), "# Demo laser\nB110 ON\nB111 OFF\nB114 OFF\nB115 OFF\nB122 20\nB123 10\nB565 1\n").unwrap();
        let lantek = d.to_str().unwrap();
        let file_order = |m: &Machine| MachineTech::for_machine(m, lantek).file_order();
        let in_use = |m: &Machine| MachineTech::for_machine(m, lantek).order();
        let file = file_order(&Machine::default());
        assert_eq!((file.sheet.corner_label(), file.sheet.horizontal, file.sheet.zones_x, file.holes.mode), ("lower left", false, 20, HoleMode::Proximity));

        // Settings saved before this: no order line, so every value is the file's.
        let path = d.join("settings.ini");
        std::fs::write(&path, "# nestpro settings\n[general]\nlantek_dir = x\nactive_machine = 0\n[machine]\nname = Laser 01\noutput = iso\n[machine]\nname = Laser 02\noutput = blm\n").unwrap();
        let mut s = AppSettings::read_from(&path).unwrap();
        assert!(s.machines.iter().all(|m| m.order.is_empty()));
        assert_eq!(in_use(&s.machines[0]), file);
        assert!(!s.to_text().contains("\norder = "), "nothing new is written for an unchanged machine");

        // Two machines, each changed its own way, as Settings → Machines does:
        // the values wanted, kept where they differ from the file's.
        let mut a: OrderSettings = file;
        (a.sheet.start_left, a.sheet.start_bottom, a.sheet.horizontal, a.sheet.zigzag, a.sheet.zones_x, a.sheet.zones_y) = (false, false, true, true, 4, 0);
        s.machines[0].order = OrderChanges::between(&file, &a);
        let mut b = file;
        (b.holes.mode, b.holes.corner, b.holes.columns, b.holes.ortho_tol) = (HoleMode::Direction, 3, true, 0.5);
        (b.holes.small_first, b.holes.big_length, b.holes.big_width, b.holes.big_offset) = (true, 40.0, 30.0, 2.5);
        // A value set to what the file says isn't a change: it follows the file.
        b.sheet.zones_x = 20;
        s.machines[1].order = OrderChanges::between(&file, &b);
        assert!(s.machines[1].order.zones_x.is_none() && !s.machines[1].order.sheet_changed());
        s.save_to(&path).unwrap();

        // A restart: each machine cuts with its own changes.
        let mut s = AppSettings::read_from(&path).unwrap();
        assert_eq!((in_use(&s.machines[0]), in_use(&s.machines[1])), (a, b));
        assert_eq!(s.machines[0].order, OrderChanges { start_left: Some(false), start_bottom: Some(false), rows: Some(true), zigzag: Some(true), zones_x: Some(4), zones_y: Some(0), ..Default::default() });
        // Switching the active machine switches the order it cuts with.
        s.active_machine = 1;
        assert_eq!(in_use(&s.machine()), b);

        // "Use the machine file" on the first: the file's values come back,
        // after a restart too; the second keeps its changes.
        s.machines[0].order = OrderChanges::default();
        s.save_to(&path).unwrap();
        let s = AppSettings::read_from(&path).unwrap();
        assert_eq!((in_use(&s.machines[0]), in_use(&s.machines[1])), (file, b));
        assert_eq!(file_order(&s.machines[1]), file, "the file's own values stay the file's");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_saved_order_value_that_cant_be_taken_is_left_out_and_the_machines_other_changes_are_kept() {
        use crate::ct1::OrderChanges;
        let d = std::env::temp_dir().join(format!("nestpro_settings_bad_order_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("settings.ini");
        // A hand-edited line: a negative zone count, a mode and a corner the
        // settings don't have, a negative tolerance, too many zones, a word
        // for a switch; and two changes that are fine.
        let line = r#"{"start_bottom":false,"zones_x":-1,"zones_y":100000,"hole_mode":0,"hole_start_corner":9,"orthogonality_tolerance":-1.0,"zigzag":"yes","big_hole_offset":2.5}"#;
        std::fs::write(&path, format!("[machine]\nname = Demo laser\noutput = iso\norder = {line}\n[machine]\nname = Demo plasma\norder = not a list\n")).unwrap();
        let s = AppSettings::read_from(&path).unwrap();
        let kept = OrderChanges { start_bottom: Some(false), hole_mode: Some(0), big_hole_offset: Some(2.5), ..Default::default() };
        assert_eq!(s.machines[0].order, kept);
        // A line that can't be read at all leaves every value the file's.
        assert!(s.machines[1].order.is_empty());
        // Saved and read again, the good changes are still there, and only them.
        s.save_to(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(r#"order = {"start_bottom":false,"hole_mode":0,"big_hole_offset":2.5}"#), "{text}");
        assert_eq!(AppSettings::read_from(&path).unwrap().machines[0].order, kept);
        // Settings with no changes save back exactly as they were read.
        let old = AppSettings::default().to_text();
        assert_eq!(AppSettings::parse(&old).to_text(), old);
        let _ = std::fs::remove_dir_all(&d);
    }
}
