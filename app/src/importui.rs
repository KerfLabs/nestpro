//! The Import drawings window: a file browser inside the app that shows the
//! picked drawing before it's imported (IM10), as the original's Open dialog
//! does. The system file picker can't draw that preview, so this takes its
//! place; "System dialog…" still opens the system one.

use crate::engine::imports::{drawing_preview, DrawingPreview, PreviewPart};
use crate::formats::Format;
use crate::settings::AppSettings;
use crate::theme::th;
use crate::ui_parts::{darker, dialog_pos, paint_part, part_color};
use crate::units::LenUnit;
use crate::App;
use eframe::egui;
use egui::{Align, Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

/// Folders kept in the window's Recent list.
pub const RECENT_DIRS: usize = 6;
const ROW_H: f32 = 24.0;
/// Previews kept in memory before the oldest are dropped.
const MAX_CACHED: usize = 48;

/// Which files the list shows ("Files of type").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    /// Every drawing format and the Lantek import XML.
    Drawings,
    Only(Format),
    Xml,
    Everything,
}

impl Filter {
    fn list() -> Vec<Filter> {
        let mut v = vec![Filter::Drawings];
        v.extend(Format::ALL.map(Filter::Only));
        v.extend([Filter::Xml, Filter::Everything]);
        v
    }

    fn label(self) -> String {
        match self {
            Filter::Drawings => "Part drawings and import XML".into(),
            Filter::Only(f) => format!("{} ({})", f.label(), f.extensions().iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(", ")),
            Filter::Xml => "Lantek import XML (*.xml)".into(),
            Filter::Everything => "All files (*.*)".into(),
        }
    }

    pub fn accepts(self, path: &Path) -> bool {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let xml = ext.eq_ignore_ascii_case("xml");
        match self {
            Filter::Drawings => xml || crate::formats::is_drawing(path),
            Filter::Only(f) => f.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)),
            Filter::Xml => xml,
            Filter::Everything => true,
        }
    }
}

/// One file or folder in the open folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub dir: bool,
    /// Last change, seconds since 1970.
    pub modified: Option<u64>,
    pub size: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortBy {
    Name,
    Date,
    Size,
}

/// Names in the order Windows Explorer gives them: case left out, and runs of
/// digits by their value (PART2 before PART10, DXFPAR10 before DXFPAR0100).
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let digits = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.next_if(char::is_ascii_digit) {
                        s.push(c);
                    }
                    s
                };
                let (n, m) = (digits(&mut x), digits(&mut y));
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let o = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// What the list shows: the folders first, then the files the filter lets
/// through, each kept only when its name holds `search`, each group sorted.
pub fn rows<'a>(entries: &'a [Entry], filter: Filter, search: &str, sort: SortBy, desc: bool) -> Vec<&'a Entry> {
    let needle = search.trim().to_lowercase();
    let mut v: Vec<&Entry> = entries.iter().filter(|e| (e.dir || filter.accepts(&e.path)) && (needle.is_empty() || e.name.to_lowercase().contains(&needle))).collect();
    v.sort_by(|a, b| {
        b.dir.cmp(&a.dir).then_with(|| {
            let by_name = natural(&a.name, &b.name);
            let o = match sort {
                SortBy::Name => by_name,
                SortBy::Date => a.modified.cmp(&b.modified).then(by_name),
                SortBy::Size => a.size.cmp(&b.size).then(by_name),
            };
            if desc {
                o.reverse()
            } else {
                o
            }
        })
    });
    v
}

/// The files and folders in `dir` (hidden ones left out).
pub fn list_dir(dir: &Path) -> Result<Vec<Entry>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("Can't open {}: {e}", dir.display()))?;
    let mut out = vec![];
    for de in rd.flatten() {
        let path = de.path();
        let name = de.file_name().to_string_lossy().to_string();
        // A link counts as what it points to.
        let md = match de.metadata() {
            Ok(m) if m.file_type().is_symlink() => std::fs::metadata(&path),
            r => r,
        };
        let Ok(md) = md else { continue };
        if hidden(&name, &md) {
            continue;
        }
        let modified = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
        out.push(Entry { name, dir: md.is_dir(), modified, size: if md.is_dir() { 0 } else { md.len() }, path });
    }
    Ok(out)
}

#[cfg(windows)]
fn hidden(_name: &str, md: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN_OR_SYSTEM: u32 = 0x2 | 0x4;
    md.file_attributes() & HIDDEN_OR_SYSTEM != 0
}

#[cfg(not(windows))]
fn hidden(name: &str, _md: &std::fs::Metadata) -> bool {
    name.starts_with('.')
}

/// "12 KB" for a file size.
pub fn size_text(b: u64) -> String {
    match b {
        0..1024 => format!("{b} B"),
        1024..1_048_576 => format!("{} KB", b.div_ceil(1024)),
        _ => format!("{:.1} MB", b as f64 / 1_048_576.0),
    }
}

/// Put `dir` first in the Recent folders list.
pub fn remember_dir(list: &mut Vec<String>, dir: &str) {
    list.retain(|d| d != dir);
    list.insert(0, dir.to_string());
    list.truncate(RECENT_DIRS);
}

/// A shortcut in the left column.
#[derive(Clone, Debug)]
struct Place {
    /// A symbol, or empty for a drawn folder.
    icon: &'static str,
    label: String,
    path: PathBuf,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

/// The usual folders: the app's documents, home, desktop, documents, downloads
/// (each only when it's there; a desktop moved to OneDrive is found too).
fn places() -> Vec<Place> {
    let mut v = vec![];
    let docs = crate::library::documents_dir();
    if docs.is_dir() {
        v.push(Place { icon: "", label: "Sheet Nesting".into(), path: docs });
    }
    if let Some(h) = home() {
        v.push(Place { icon: "🏠", label: "Home".into(), path: h.clone() });
        for (icon, name) in [("🖥", "Desktop"), ("", "Documents"), ("", "Downloads")] {
            if let Some(p) = [h.join(name), h.join("OneDrive").join(name)].into_iter().find(|p| p.is_dir()) {
                v.push(Place { icon, label: name.into(), path: p });
            }
        }
    }
    v
}

/// The drives to start from (Windows: every drive letter in use; elsewhere the root).
#[cfg(windows)]
fn drives() -> Vec<Place> {
    unsafe extern "system" {
        fn GetLogicalDrives() -> u32;
    }
    // SAFETY: a plain Win32 call without arguments.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8).filter(|i| mask & (1 << i) != 0).map(|i| {
        let l = (b'A' + i) as char;
        Place { icon: "🖴", label: format!("{l}:"), path: PathBuf::from(format!("{l}:\\")) }
    }).collect()
}

#[cfg(not(windows))]
fn drives() -> Vec<Place> {
    vec![Place { icon: "🖴", label: "Computer".into(), path: PathBuf::from("/") }]
}

/// What the preview shows for a file.
enum Shown {
    Drawing(Arc<DrawingPreview>),
    /// A Lantek import XML: how many products, and a line for each of the first ones.
    Xml { products: usize, lines: Vec<String> },
    Failed(String),
}

type Loaded = (PathBuf, Option<u64>, String, Shown);

/// The settings a preview depends on: a preview read with other ones is read again.
fn preview_key(cfg: &AppSettings, spec: &crate::nest::SheetSpec) -> String {
    // Material names and aliases decide whether the file's material matches the job's.
    format!("{:?}|{}|{}|{}|{}|{:?}|{:?}|{}|{}", cfg.nesting.units, cfg.name_format, cfg.ref_prefix, cfg.ref_suffix, cfg.lantek_dir, cfg.materials, cfg.material_aliases, spec.material, spec.thickness)
}

/// Read a file for the preview (runs off the window's thread).
fn read_preview(cfg: &AppSettings, spec: &crate::nest::SheetSpec, path: &Path) -> Shown {
    let read = || {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")) {
            let text = std::fs::read(path).map(|b| String::from_utf8_lossy(&b).to_string()).map_err(|e| e.to_string())?;
            let ps = crate::xmlimport::parse(&text)?;
            let lines = ps.iter().take(12).map(|p| format!("{} — qty {} · {} {} mm", p.reference, p.quantity, p.material, p.thickness)).collect();
            return Ok(Shown::Xml { products: ps.len(), lines });
        }
        drawing_preview(cfg, spec, path).map(|d| Shown::Drawing(Arc::new(d)))
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(read)).unwrap_or_else(|_| Err("This file could not be read (it looks damaged).".into())).unwrap_or_else(Shown::Failed)
}

pub struct ImportUi {
    pub open: bool,
    pub dir: PathBuf,
    path_text: String,
    entries: Vec<Entry>,
    list_error: Option<String>,
    pub filter: Filter,
    search: String,
    /// The files that Import brings in.
    pub picked: BTreeSet<PathBuf>,
    /// The row the keyboard is on; its file is previewed.
    pub focus: Option<PathBuf>,
    /// Where a Shift-click range starts.
    anchor: Option<PathBuf>,
    sort: SortBy,
    desc: bool,
    /// Previews by file: its last change and the settings it was read with (`preview_key`).
    cache: HashMap<PathBuf, (Option<u64>, String, Shown)>,
    loading: Option<PathBuf>,
    tx: mpsc::Sender<Loaded>,
    rx: mpsc::Receiver<Loaded>,
    /// Which part of a file of several parts is shown.
    part_ix: usize,
    scroll_to_focus: bool,
    /// The file name box, and what the window last wrote in it (anything else was typed).
    name_text: String,
    name_auto: String,
    places: Vec<Place>,
    drives: Vec<Place>,
    /// Read previews on the window's thread (screenshots, so they're ready in time).
    pub sync: bool,
}

impl Default for ImportUi {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        ImportUi {
            open: false,
            dir: PathBuf::new(),
            path_text: String::new(),
            entries: vec![],
            list_error: None,
            filter: Filter::Drawings,
            search: String::new(),
            picked: BTreeSet::new(),
            focus: None,
            anchor: None,
            sort: SortBy::Name,
            desc: false,
            cache: HashMap::new(),
            loading: None,
            tx,
            rx,
            part_ix: 0,
            scroll_to_focus: false,
            name_text: String::new(),
            name_auto: String::new(),
            places: vec![],
            drives: vec![],
            sync: false,
        }
    }
}

impl ImportUi {
    /// Open the window in the first of `dirs` that can be listed.
    pub fn show(&mut self, dirs: &[PathBuf]) {
        self.open = true;
        self.places = places();
        self.drives = drives();
        self.cache.clear();
        self.name_text.clear();
        self.name_auto.clear();
        for d in dirs {
            if self.go(d.clone()) {
                return;
            }
        }
    }

    /// Open folder `dir`; false (and the folder kept) when it can't be listed.
    pub fn go(&mut self, dir: PathBuf) -> bool {
        match list_dir(&dir) {
            Ok(e) => {
                self.entries = e;
                self.path_text = dir.display().to_string();
                self.dir = dir;
                self.list_error = None;
                self.picked.clear();
                self.focus = None;
                self.anchor = None;
                self.search.clear();
                self.scroll_to_focus = true;
                true
            }
            Err(e) => {
                self.list_error = Some(e);
                self.path_text = self.dir.display().to_string();
                false
            }
        }
    }

    fn up(&mut self) {
        if let Some(p) = self.dir.parent().map(Path::to_path_buf) {
            let from = self.dir.clone();
            if self.go(p) {
                // Back in the parent, the folder just left is the one picked.
                self.focus = Some(from.clone());
                self.anchor = Some(from);
            }
        }
    }

    fn refresh(&mut self) {
        let (picked, focus) = (self.picked.clone(), self.focus.clone());
        if self.go(self.dir.clone()) {
            self.picked = picked.into_iter().filter(|p| self.entries.iter().any(|e| &e.path == p)).collect();
            self.focus = focus.filter(|f| self.entries.iter().any(|e| &e.path == f));
        }
        self.cache.clear();
    }

    fn visible(&self) -> Vec<Entry> {
        rows(&self.entries, self.filter, &self.search, self.sort, self.desc).into_iter().cloned().collect()
    }

    /// Take in previews read in the background.
    fn poll(&mut self) {
        while let Ok((p, m, k, s)) = self.rx.try_recv() {
            if self.loading.as_ref() == Some(&p) {
                self.loading = None;
            }
            if self.cache.len() >= MAX_CACHED {
                self.cache.clear();
            }
            self.cache.insert(p, (m, k, s));
        }
    }

    /// Start reading the focused file when it has no preview yet (one at a
    /// time: moving quickly down the list reads only where it stops).
    fn want_preview(&mut self, cfg: &AppSettings, spec: &crate::nest::SheetSpec, ctx: &egui::Context) {
        if !cfg.import_preview || self.loading.is_some() {
            return;
        }
        let Some(e) = self.focus.as_ref().and_then(|f| self.entries.iter().find(|e| &e.path == f)) else { return };
        let key = preview_key(cfg, spec);
        if e.dir || self.cache.get(&e.path).is_some_and(|(m, k, _)| *m == e.modified && *k == key) {
            return;
        }
        let (path, m) = (e.path.clone(), e.modified);
        if self.sync {
            let s = read_preview(cfg, spec, &path);
            self.cache.insert(path, (m, key, s));
            return;
        }
        self.loading = Some(path.clone());
        let (tx, cfg, spec, ctx) = (self.tx.clone(), cfg.clone(), spec.clone(), ctx.clone());
        std::thread::spawn(move || {
            let s = read_preview(&cfg, &spec, &path);
            let _ = tx.send((path, m, key, s));
            ctx.request_repaint();
        });
    }

    /// Focus row `i` of `rows` from the keyboard; Shift picks the range from the anchor.
    fn focus_row(&mut self, rows: &[Entry], i: usize, shift: bool) {
        let Some(e) = rows.get(i) else { return };
        self.focus = Some(e.path.clone());
        if shift {
            self.pick_range(rows, &e.path);
        } else {
            self.anchor = Some(e.path.clone());
            self.picked.clear();
            if !e.dir {
                self.picked.insert(e.path.clone());
            }
        }
        self.part_ix = 0;
        self.scroll_to_focus = true;
    }

    /// Pick the files from the anchor row to `to`.
    fn pick_range(&mut self, rows: &[Entry], to: &Path) {
        let at = |p: &Path| rows.iter().position(|e| e.path == p);
        let (Some(a), Some(b)) = (self.anchor.as_deref().and_then(at).or(Some(0)), at(to)) else { return };
        let (a, b) = (a.min(b), a.max(b));
        self.picked = rows[a..=b].iter().filter(|e| !e.dir).map(|e| e.path.clone()).collect();
    }

    /// The file name box shows the picked files; a name typed in it counts until the picks change.
    fn sync_name(&mut self) {
        let names = self.picked_names();
        if names != self.name_auto {
            self.name_text = names.clone();
            self.name_auto = names;
        }
    }

    /// The file name box text for the picked files, as Windows writes it.
    fn picked_names(&self) -> String {
        let names: Vec<String> = self.picked.iter().filter_map(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).collect();
        match names.len() {
            1 => names[0].clone(),
            _ => names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(" "),
        }
    }
}

/// What the window asks the app to do once it's drawn.
enum Act {
    None,
    Import(Vec<PathBuf>),
    System,
    Close,
    Settings,
}

impl App {
    /// Ask for part drawings: the app's import window, with a preview (IM10).
    pub fn import_dialog(&mut self) {
        let mut dirs: Vec<PathBuf> = self.cfg.import_dirs.iter().map(PathBuf::from).collect();
        dirs.push(crate::library::documents_dir());
        dirs.extend(home());
        dirs.push(PathBuf::from("."));
        self.importui.show(&dirs);
    }

    /// The system's own file picker (no preview), starting in `start`.
    pub fn import_system_dialog(&mut self, start: Option<&Path>) {
        let mut all = crate::formats::picker_extensions();
        all.extend(["xml".to_string(), "XML".to_string()]);
        let mut dlg = rfd::FileDialog::new().set_title("Import part drawings").add_filter("Part drawings or Lantek import XML", &all);
        for f in Format::ALL {
            let ext: Vec<String> = f.extensions().iter().flat_map(|e| [e.to_string(), e.to_uppercase()]).collect();
            dlg = dlg.add_filter(f.label(), &ext);
        }
        if let Some(d) = start {
            dlg = dlg.set_directory(d);
        }
        if let Some(files) = dlg.add_filter("Lantek import XML (DXF Import / Parts Import)", &["xml", "XML"]).pick_files() {
            self.import_from_window(files);
        }
    }

    /// Import the files picked in either window, remembering their folder.
    fn import_from_window(&mut self, files: Vec<PathBuf>) {
        if let Some(d) = files.first().and_then(|f| f.parent()) {
            remember_dir(&mut self.cfg.import_dirs, &d.display().to_string());
        }
        self.importui.open = false;
        self.import_drawings(files);
        self.apply_settings(false);
    }

    pub fn import_window(&mut self, ctx: &egui::Context) {
        if !self.importui.open {
            return;
        }
        self.importui.poll();
        self.importui.want_preview(&self.cfg, &self.spec, ctx);
        let rows = self.importui.visible();
        let mut act = Act::None;

        // Keys move in the list unless a text box has them (they don't reach the sheet behind).
        if !ctx.egui_wants_keyboard_input() {
            let at = self.importui.focus.as_ref().and_then(|f| rows.iter().position(|e| &e.path == f));
            let (mut down, mut up, mut home, mut end, mut enter, mut back, mut all, mut space, mut esc) = (false, false, false, false, false, false, false, false, false);
            let shift = ctx.input(|i| i.modifiers.shift);
            ctx.input_mut(|i| {
                for m in [Modifiers::NONE, Modifiers::SHIFT] {
                    down |= i.consume_key(m, Key::ArrowDown);
                    up |= i.consume_key(m, Key::ArrowUp);
                    home |= i.consume_key(m, Key::Home);
                    end |= i.consume_key(m, Key::End);
                }
                enter = i.consume_key(Modifiers::NONE, Key::Enter);
                back = i.consume_key(Modifiers::NONE, Key::Backspace) || i.consume_key(Modifiers::ALT, Key::ArrowUp);
                all = i.consume_key(Modifiers::COMMAND, Key::A);
                space = i.consume_key(Modifiers::NONE, Key::Space);
                esc = i.consume_key(Modifiers::NONE, Key::Escape);
            });
            let last = rows.len().saturating_sub(1);
            if down && !rows.is_empty() {
                self.importui.focus_row(&rows, at.map_or(0, |i| (i + 1).min(last)), shift);
            }
            if up && !rows.is_empty() {
                self.importui.focus_row(&rows, at.map_or(0, |i| i.saturating_sub(1)), shift);
            }
            if home && !rows.is_empty() {
                self.importui.focus_row(&rows, 0, shift);
            }
            if end && !rows.is_empty() {
                self.importui.focus_row(&rows, last, shift);
            }
            if all {
                self.importui.picked = rows.iter().filter(|e| !e.dir).map(|e| e.path.clone()).collect();
            }
            if space
                && let Some(e) = at.and_then(|i| rows.get(i)).filter(|e| !e.dir)
                && !self.importui.picked.remove(&e.path)
            {
                self.importui.picked.insert(e.path.clone());
            }
            if enter {
                match at.and_then(|i| rows.get(i)) {
                    Some(e) if e.dir => {
                        self.importui.go(e.path.clone());
                    }
                    _ if !self.importui.picked.is_empty() => act = Act::Import(self.importui.picked.iter().cloned().collect()),
                    _ => {}
                }
            }
            if back {
                self.importui.up();
            }
            if esc {
                act = Act::Close;
            }
        }
        self.importui.sync_name();

        let rows = self.importui.visible();
        let mut open = true;
        let unit = self.cfg.nesting.units;
        egui::Window::new(RichText::new("Import part drawings").size(16.0).strong())
            .id(egui::Id::unique("import_browser"))
            .pivot(Align2::CENTER_TOP)
            .default_pos(dialog_pos(ctx))
            .default_size([1040.0, 620.0])
            .min_size([760.0, 440.0])
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                self.import_toolbar(ui, &mut act);
                ui.add_space(4.0);
                let bottom_h = 82.0;
                let body_h = (ui.available_height() - bottom_h).max(200.0);
                let full_w = ui.available_width();
                ui.allocate_ui_with_layout(Vec2::new(full_w, body_h), egui::Layout::left_to_right(Align::Min), |ui| {
                    let side_w = 170.0;
                    let prev_w = 300.0;
                    let list_w = (full_w - side_w - prev_w - 2.0 * ui.spacing().item_spacing.x).max(240.0);
                    ui.allocate_ui_with_layout(Vec2::new(side_w, body_h), egui::Layout::top_down(Align::Min), |ui| self.import_places(ui));
                    ui.allocate_ui_with_layout(Vec2::new(list_w, body_h), egui::Layout::top_down(Align::Min), |ui| self.import_list(ui, &rows, list_w, body_h, &mut act));
                    ui.allocate_ui_with_layout(Vec2::new(prev_w, body_h), egui::Layout::top_down(Align::Min), |ui| self.import_preview_pane(ui, prev_w, unit, &mut act));
                });
                ui.separator();
                self.import_bottom(ui, &mut act);
            });
        if !open {
            act = Act::Close;
        }
        match act {
            Act::None => {}
            Act::Import(files) => self.import_from_window(files),
            Act::System => {
                let dir = self.importui.dir.clone();
                self.importui.open = false;
                self.import_system_dialog(Some(&dir));
            }
            Act::Close => {
                self.importui.open = false;
                self.apply_settings(false);
            }
            Act::Settings => self.open_settings(crate::settings_ui::Section::General),
        }
    }

    /// Up, refresh, the folder's path (type one and press Enter), search.
    fn import_toolbar(&mut self, ui: &mut egui::Ui, act: &mut Act) {
        let iu = &mut self.importui;
        ui.horizontal(|ui| {
            if ui.add_enabled(iu.dir.parent().is_some(), egui::Button::new("⬆")).on_hover_text("Up one folder (Backspace)").clicked() {
                iu.up();
            }
            if ui.button("⟳").on_hover_text("Read the folder again").clicked() {
                iu.refresh();
            }
            let search_w = 190.0;
            let sys_w = 120.0;
            let w = (ui.available_width() - search_w - sys_w - 3.0 * ui.spacing().item_spacing.x).max(120.0);
            let r = ui.add(egui::TextEdit::singleline(&mut iu.path_text).desired_width(w).hint_text("Folder"));
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let p = PathBuf::from(iu.path_text.trim());
                if p.is_file() {
                    *act = Act::Import(vec![p]);
                } else {
                    iu.go(p);
                }
            }
            ui.add(egui::TextEdit::singleline(&mut iu.search).desired_width(search_w).hint_text("🔍 Search this folder"));
            if ui.add_sized([sys_w, 20.0], egui::Button::new("System dialog…")).on_hover_text("Pick the files with the system's own window instead (network places, its search) — without the preview").clicked() {
                *act = Act::System;
            }
        });
        if let Some(e) = &iu.list_error {
            ui.label(RichText::new(format!("⚠ {e}")).color(th().warn_fg));
        }
    }

    /// The left column: usual folders, folders imported from lately, drives.
    fn import_places(&mut self, ui: &mut egui::Ui) {
        let mut go: Option<PathBuf> = None;
        let iu = &self.importui;
        let recent: Vec<Place> = self
            .cfg
            .import_dirs
            .iter()
            .map(|d| {
                let p = PathBuf::from(d);
                let label = p.file_name().map_or_else(|| d.clone(), |n| n.to_string_lossy().to_string());
                Place { icon: "", label, path: p }
            })
            .collect();
        egui::ScrollArea::vertical().id_salt("import_places").auto_shrink([false, false]).show(ui, |ui| {
            for (title, list) in [("PLACES", &iu.places), ("RECENT FOLDERS", &recent), (if cfg!(windows) { "DRIVES" } else { "COMPUTER" }, &iu.drives)] {
                if list.is_empty() {
                    continue;
                }
                ui.add_space(4.0);
                ui.label(RichText::new(title).size(10.5).strong().color(th().text_dim));
                for pl in list {
                    let w = ui.available_width();
                    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 24.0), Sense::click());
                    let here = pl.path == iu.dir;
                    let bg = if here { th().accent_soft } else if r.hovered() { th().hover } else { Color32::TRANSPARENT };
                    ui.painter().rect_filled(rect, 4.0, bg);
                    let p = ui.painter_at(rect);
                    if pl.icon.is_empty() {
                        folder_icon(&p, Rect::from_center_size(rect.left_center() + Vec2::new(14.0, 0.0), Vec2::new(16.0, 12.0)));
                    } else {
                        p.text(rect.left_center() + Vec2::new(14.0, 0.0), Align2::CENTER_CENTER, pl.icon, FontId::proportional(13.0), th().text_dim);
                    }
                    p.text(rect.left_center() + Vec2::new(28.0, 0.0), Align2::LEFT_CENTER, &pl.label, FontId::proportional(13.0), if here { th().accent_fg } else { th().text });
                    if r.on_hover_text(pl.path.display().to_string()).clicked() {
                        go = Some(pl.path.clone());
                    }
                }
            }
        });
        if let Some(p) = go {
            self.importui.go(p);
        }
    }

    /// The folder's contents: check box, name, date, size; click picks, double-click opens.
    fn import_list(&mut self, ui: &mut egui::Ui, rows: &[Entry], w: f32, h: f32, act: &mut Act) {
        let iu = &mut self.importui;
        let mut go: Option<PathBuf> = None;
        let frame = egui::Frame::new().fill(th().field).stroke(Stroke::new(1.0, th().border)).corner_radius(4.0).inner_margin(1.0);
        frame.show(ui, |ui| {
            // The frame adds its margin and line around this (2 px a side).
            ui.set_min_size(Vec2::new(w - 4.0, h - 4.0));
            ui.set_max_width(w - 4.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let cw = ui.available_width();
            let (date_w, size_w, box_w) = (96.0, 70.0, 26.0);
            // Header: pick all, and the columns that sort the list.
            let (hr, _) = ui.allocate_exact_size(Vec2::new(cw, 24.0), Sense::hover());
            ui.painter().rect_filled(hr, 0.0, th().pane_header);
            // Columns line up with the rows, which leave room for the scroll bar.
            let rw_id = egui::Id::unique("import_row_w");
            let rw = ui.data(|d| d.get_temp::<f32>(rw_id)).unwrap_or(cw).min(cw);
            let hr = Rect::from_min_size(hr.min, Vec2::new(rw, hr.height()));
            let files: Vec<&Entry> = rows.iter().filter(|e| !e.dir).collect();
            let all_picked = !files.is_empty() && files.iter().all(|e| iu.picked.contains(&e.path));
            let box_rect = Rect::from_center_size(Pos2::new(hr.left() + box_w / 2.0, hr.center().y), Vec2::splat(14.0));
            if paint_check(ui, box_rect, all_picked, egui::Id::unique("import_all")).clicked() {
                iu.picked = if all_picked { BTreeSet::new() } else { files.iter().map(|e| e.path.clone()).collect() };
            }
            let cols = [(SortBy::Name, "Name", Rect::from_min_max(Pos2::new(hr.left() + box_w, hr.top()), Pos2::new(hr.right() - date_w - size_w, hr.bottom()))), (SortBy::Date, "Date modified", Rect::from_min_max(Pos2::new(hr.right() - date_w - size_w, hr.top()), Pos2::new(hr.right() - size_w, hr.bottom()))), (SortBy::Size, "Size", Rect::from_min_max(Pos2::new(hr.right() - size_w, hr.top()), hr.max))];
            for (s, label, r) in cols {
                let resp = ui.interact(r, egui::Id::unique(("import_sort", label)), Sense::click());
                let arrow = if iu.sort == s { if iu.desc { " ▼" } else { " ▲" } } else { "" };
                let (anchor, pos) = if s == SortBy::Size { (Align2::RIGHT_CENTER, r.right_center() - Vec2::new(10.0, 0.0)) } else { (Align2::LEFT_CENTER, r.left_center() + Vec2::new(6.0, 0.0)) };
                ui.painter().text(pos, anchor, format!("{label}{arrow}"), FontId::proportional(12.0), if resp.hovered() { th().text_strong } else { th().text_dim });
                if resp.clicked() {
                    if iu.sort == s {
                        iu.desc = !iu.desc;
                    } else {
                        (iu.sort, iu.desc) = (s, s != SortBy::Name);
                    }
                }
            }
            if rows.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    let msg = if iu.search.trim().is_empty() { "No drawings in this folder." } else { "Nothing here matches the search." };
                    ui.label(RichText::new(msg).color(th().text_dim));
                    if iu.filter != Filter::Everything {
                        ui.label(RichText::new("Files of type (below) chooses which files are listed.").size(11.5).color(th().text_dim));
                    }
                });
                return;
            }
            // Keep the focused row in view when the keyboard moved it.
            let mut sa = egui::ScrollArea::vertical().id_salt(("import_rows", iu.dir.clone())).auto_shrink([false, false]);
            if iu.scroll_to_focus {
                iu.scroll_to_focus = false;
                if let Some(i) = iu.focus.as_ref().and_then(|f| rows.iter().position(|e| &e.path == f)) {
                    let view_h = ui.available_height();
                    let y = i as f32 * ROW_H;
                    let cur = ui.data(|d| d.get_temp::<f32>(egui::Id::unique("import_rows_off"))).unwrap_or(0.0);
                    if y < cur {
                        sa = sa.vertical_scroll_offset(y);
                    } else if y + ROW_H > cur + view_h {
                        sa = sa.vertical_scroll_offset(y + ROW_H - view_h);
                    }
                } else {
                    sa = sa.vertical_scroll_offset(0.0);
                }
            }
            let out = sa.show_rows(ui, ROW_H, rows.len(), |ui, range| {
                let rw = ui.available_width();
                ui.data_mut(|d| d.insert_temp(rw_id, rw));
                for e in &rows[range] {
                    let (rect, r) = ui.allocate_exact_size(Vec2::new(rw, ROW_H), Sense::click());
                    let picked = iu.picked.contains(&e.path);
                    let focused = iu.focus.as_ref() == Some(&e.path);
                    let bg = if picked || (focused && e.dir) { th().accent_soft } else if r.hovered() { th().hover } else { Color32::TRANSPARENT };
                    ui.painter().rect_filled(rect, 0.0, bg);
                    if focused {
                        ui.painter().rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0, th().accent_border), StrokeKind::Inside);
                    }
                    let box_rect = Rect::from_center_size(Pos2::new(rect.left() + box_w / 2.0, rect.center().y), Vec2::splat(14.0));
                    let mut box_hit = false;
                    if !e.dir {
                        box_hit = paint_check(ui, box_rect, picked, egui::Id::unique(("import_pick", &e.path))).clicked();
                    }
                    let p = ui.painter_at(rect);
                    let x = rect.left() + box_w;
                    if e.dir {
                        folder_icon(&p, Rect::from_center_size(Pos2::new(x + 15.0, rect.center().y), Vec2::new(18.0, 14.0)));
                    } else {
                        ext_tag(&p, Rect::from_center_size(Pos2::new(x + 15.0, rect.center().y), Vec2::new(30.0, 15.0)), &e.path);
                    }
                    let name_clip = Rect::from_min_max(Pos2::new(x + 34.0, rect.top()), Pos2::new(rect.right() - date_w - size_w - 4.0, rect.bottom()));
                    ui.painter_at(name_clip).text(name_clip.left_center(), Align2::LEFT_CENTER, &e.name, FontId::proportional(13.0), th().text_strong);
                    let date = e.modified.map(crate::jobtree::civil_date).unwrap_or_default();
                    p.text(Pos2::new(rect.right() - date_w - size_w + 6.0, rect.center().y), Align2::LEFT_CENTER, date, FontId::proportional(12.0), th().text_dim);
                    if !e.dir {
                        p.text(Pos2::new(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, size_text(e.size), FontId::proportional(12.0), th().text_dim);
                    }
                    if box_hit {
                        if !iu.picked.remove(&e.path) {
                            iu.picked.insert(e.path.clone());
                        }
                        iu.focus = Some(e.path.clone());
                        iu.anchor = Some(e.path.clone());
                        iu.part_ix = 0;
                    } else if r.clicked() {
                        let m = ui.input(|i| i.modifiers);
                        iu.focus = Some(e.path.clone());
                        iu.part_ix = 0;
                        if m.shift {
                            iu.pick_range(rows, &e.path);
                        } else if m.command {
                            if !e.dir && !iu.picked.remove(&e.path) {
                                iu.picked.insert(e.path.clone());
                            }
                            iu.anchor = Some(e.path.clone());
                        } else {
                            iu.picked.clear();
                            if !e.dir {
                                iu.picked.insert(e.path.clone());
                            }
                            iu.anchor = Some(e.path.clone());
                        }
                    }
                    if r.double_clicked() {
                        if e.dir {
                            go = Some(e.path.clone());
                        } else if iu.picked.contains(&e.path) {
                            *act = Act::Import(iu.picked.iter().cloned().collect());
                        } else {
                            *act = Act::Import(vec![e.path.clone()]);
                        }
                    }
                }
            });
            ui.data_mut(|d| d.insert_temp(egui::Id::unique("import_rows_off"), out.state.clamped_offset().y));
        });
        if let Some(p) = go {
            self.importui.go(p);
        }
    }

    /// The right column: the focused drawing as it will come in, its size and
    /// what the import will say about it; Preview on/off and the import settings.
    fn import_preview_pane(&mut self, ui: &mut egui::Ui, w: f32, unit: LenUnit, act: &mut Act) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Preview").strong().color(th().text_strong));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let r = ui.checkbox(&mut self.cfg.import_preview, "Show").on_hover_text("Read and draw the picked drawing (turn off to browse large folders on a slow network faster)");
                if r.changed() {
                    self.importui.loading = None;
                }
            });
        });
        let side = w.min(ui.available_height() - 150.0).max(160.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, side), Sense::hover());
        ui.painter().rect_filled(rect, 6.0, th().card);
        ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0, th().border), StrokeKind::Inside);
        let key = preview_key(&self.cfg, &self.spec);
        let iu = &mut self.importui;
        let focus = iu.focus.as_ref().and_then(|f| iu.entries.iter().find(|e| &e.path == f)).cloned();
        let center = |ui: &mut egui::Ui, text: &str, color: Color32| {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(13.0), color);
        };
        let mut info: Vec<(String, Color32)> = vec![];
        let mut parts_nav: Option<usize> = None;
        match &focus {
            None => center(ui, "Pick a drawing to see it here", th().text_dim),
            Some(e) if e.dir => {
                folder_icon(ui.painter(), Rect::from_center_size(rect.center(), Vec2::new(80.0, 62.0)));
                info.push((e.name.clone(), th().text_strong));
                info.push(("Folder — double-click or Enter to open it".into(), th().text_dim));
            }
            Some(e) if !self.cfg.import_preview => {
                center(ui, "Preview is off", th().text_dim);
                info.push((e.name.clone(), th().text_strong));
                info.push((size_text(e.size), th().text_dim));
            }
            Some(e) => match iu.cache.get(&e.path).filter(|(m, k, _)| *m == e.modified && *k == key).map(|(_, _, s)| s) {
                None => {
                    ui.put(Rect::from_center_size(rect.center(), Vec2::splat(24.0)), egui::Spinner::new());
                    info.push((e.name.clone(), th().text_strong));
                    info.push(("Reading…".into(), th().text_dim));
                }
                Some(Shown::Failed(msg)) => {
                    center(ui, "Can't show this file", th().warn_fg);
                    info.push((e.name.clone(), th().text_strong));
                    info.push((format!("⚠ {msg}"), th().warn_fg));
                }
                Some(Shown::Xml { products, lines }) => {
                    center(ui, "Lantek import XML", th().text_dim);
                    info.push((e.name.clone(), th().text_strong));
                    info.push((format!("Import XML · {products} product(s), one job per material and thickness"), th().text_dim));
                    for l in lines {
                        info.push((l.clone(), th().text));
                    }
                    if *products > lines.len() {
                        info.push((format!("… and {} more", products - lines.len()), th().text_dim));
                    }
                }
                Some(Shown::Drawing(d)) => {
                    let n = d.parts.len();
                    let ix = iu.part_ix.min(n.saturating_sub(1));
                    if let Some(pp) = d.parts.get(ix) {
                        paint_preview(ui, pp, unit, rect, self.color_seq);
                        info.extend(preview_lines(pp, d.format, unit));
                    }
                    if n > 1 {
                        parts_nav = Some(n);
                    }
                }
            },
        }
        if let Some(n) = parts_nav {
            ui.horizontal(|ui| {
                if ui.add_enabled(iu.part_ix > 0, egui::Button::new("◀").small()).clicked() {
                    iu.part_ix -= 1;
                }
                ui.label(RichText::new(format!("Part {} of {n}", iu.part_ix.min(n - 1) + 1)).color(th().text));
                if ui.add_enabled(iu.part_ix + 1 < n, egui::Button::new("▶").small()).clicked() {
                    iu.part_ix += 1;
                }
            });
        } else {
            ui.add_space(4.0);
        }
        egui::ScrollArea::vertical().id_salt("import_info").max_height((ui.available_height() - 34.0).max(40.0)).auto_shrink([false, true]).show(ui, |ui| {
            for (i, (t, c)) in info.iter().enumerate() {
                let rt = RichText::new(t).color(*c);
                ui.add(egui::Label::new(if i == 0 { rt.strong().size(14.0) } else { rt.size(12.0) }).wrap());
            }
        });
        ui.with_layout(egui::Layout::bottom_up(Align::Min), |ui| {
            let r = ui.button("Configure…").on_hover_text("How drawings are read: part data from the file name, reference prefix and suffix");
            egui::Popup::menu(&r).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                ui.set_min_width(300.0);
                ui.label(RichText::new("Part data from the file name").strong());
                ui.label(RichText::new("Fields: [Reference] [Quantity] [Material] [Thickness]; * skips text.").size(11.5).color(th().text_dim));
                ui.add(egui::TextEdit::singleline(&mut self.cfg.name_format).hint_text("[Reference]_[Quantity]_[Material]").desired_width(280.0));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Reference prefix");
                    ui.add(egui::TextEdit::singleline(&mut self.cfg.ref_prefix).desired_width(60.0));
                    ui.label("suffix");
                    ui.add(egui::TextEdit::singleline(&mut self.cfg.ref_suffix).desired_width(60.0));
                });
                ui.add_space(6.0);
                ui.label(RichText::new(format!("Drawings that don't say their unit are read in {} (Settings → Nesting → Units).", unit.label())).size(11.5).color(th().text_dim));
                ui.add_space(4.0);
                if ui.button("All settings…").clicked() {
                    *act = Act::Settings;
                }
            });
        });
    }

    /// File name, files of type, Import and Cancel (as the system's Open window lays them out).
    fn import_bottom(&mut self, ui: &mut egui::Ui, act: &mut Act) {
        let iu = &mut self.importui;
        let (label_w, btn) = (96.0, Vec2::new(110.0, 26.0));
        let field_w = (ui.available_width() - label_w - btn.x - 2.0 * ui.spacing().item_spacing.x - 4.0).max(200.0);
        let n = iu.picked.len();
        let typed = iu.name_text != iu.name_auto && !iu.name_text.trim().is_empty();
        let mut go = false;
        ui.horizontal(|ui| {
            ui.add_sized([label_w, btn.y], egui::Label::new("File name:"));
            let r = ui.add_sized([field_w, btn.y - 4.0], egui::TextEdit::singleline(&mut iu.name_text).hint_text("Pick drawings above, or type a name"));
            go |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let label = if n > 1 { format!("Import {n}") } else { "Import".into() };
            let primary = egui::Button::new(RichText::new(label).color(Color32::WHITE).strong()).fill(th().accent).min_size(btn);
            go |= ui.add_enabled(n > 0 || typed, primary).on_hover_text("Add the picked drawings to the job (Enter)").clicked();
        });
        ui.horizontal(|ui| {
            ui.add_sized([label_w, btn.y], egui::Label::new("Files of type:"));
            let before = iu.filter;
            egui::ComboBox::from_id_salt("import_filter").width(field_w).selected_text(iu.filter.label()).show_ui(ui, |ui| {
                for f in Filter::list() {
                    ui.selectable_value(&mut iu.filter, f, f.label());
                }
            });
            // Files the new type hides aren't imported.
            if iu.filter != before {
                let filter = iu.filter;
                iu.picked.retain(|p| filter.accepts(p));
            }
            if ui.add(egui::Button::new("Cancel").min_size(btn)).clicked() {
                *act = Act::Close;
            }
        });
        if !go {
            return;
        }
        if typed {
            // Typed: a folder opens, a file is imported.
            let t = iu.name_text.trim().trim_matches('"').to_string();
            let p = if Path::new(&t).is_absolute() { PathBuf::from(&t) } else { iu.dir.join(&t) };
            if p.is_dir() {
                iu.go(p);
                iu.name_text.clear();
            } else if p.is_file() {
                *act = Act::Import(vec![p]);
            } else {
                iu.list_error = Some(format!("No file or folder named {t} here."));
            }
        } else if n > 0 {
            *act = Act::Import(iu.picked.iter().cloned().collect());
        }
    }
}

/// A check box drawn at `r` that answers clicks.
fn paint_check(ui: &mut egui::Ui, r: Rect, on: bool, id: egui::Id) -> egui::Response {
    let resp = ui.interact(r.expand(4.0), id, Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(r, 3.0, th().accent);
        let pts = [r.left_top() + Vec2::new(3.0, 7.5), r.left_top() + Vec2::new(6.0, 10.5), r.left_top() + Vec2::new(11.0, 4.0)];
        p.line_segment([pts[0], pts[1]], Stroke::new(1.8, Color32::WHITE));
        p.line_segment([pts[1], pts[2]], Stroke::new(1.8, Color32::WHITE));
    } else {
        p.rect_filled(r, 3.0, th().field);
        p.rect_stroke(r, 3.0, Stroke::new(1.0, if resp.hovered() { th().accent_border } else { th().control_edge }), StrokeKind::Inside);
    }
    resp
}

/// A folder: a tab and a body in folder yellow.
fn folder_icon(p: &egui::Painter, r: Rect) {
    let back = Color32::from_rgb(222, 170, 52);
    let front = Color32::from_rgb(246, 196, 72);
    let tab = Rect::from_min_size(r.min, Vec2::new(r.width() * 0.42, r.height() * 0.3));
    p.rect_filled(tab, r.height() * 0.08, back);
    p.rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.top() + r.height() * 0.16), r.max), r.height() * 0.1, back);
    p.rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.top() + r.height() * 0.3), r.max), r.height() * 0.1, front);
}

/// A small tag with the file's extension (DXF, DWG, XML…).
fn ext_tag(p: &egui::Painter, r: Rect, path: &Path) {
    let ext = path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
    let (bg, fg) = if ext == "XML" {
        (th().ok_bg, th().ok_fg)
    } else if crate::formats::is_drawing(path) {
        (th().accent_soft, th().accent_fg)
    } else {
        (th().pane_header, th().text_dim)
    };
    p.rect_filled(r, 3.0, bg);
    let ext: String = ext.chars().take(4).collect();
    p.text(r.center(), Align2::CENTER_CENTER, ext, FontId::proportional(9.5), fg);
}

/// Name, size, holes, format, layers, quantity and the import's notes for one part.
fn preview_lines(pp: &PreviewPart, format: Format, unit: LenUnit) -> Vec<(String, Color32)> {
    let mut v = vec![];
    match &pp.part {
        Ok((part, notes)) => {
            v.push((part.name.clone(), th().text_strong));
            let holes = part.geometry().holes.len();
            let holes = match holes {
                0 => String::new(),
                1 => " · 1 hole".into(),
                n => format!(" · {n} holes"),
            };
            v.push((format!("{} × {}{holes}", unit.show(part.w), unit.show(part.h)), th().text));
            let mut kind = format!("{} · {} layer{}", format.label(), pp.layers, if pp.layers == 1 { "" } else { "s" });
            if part.qty > 1 {
                kind += &format!(" · quantity {}", part.qty);
            }
            v.push((kind, th().text_dim));
            if pp.layers > 1 {
                v.push(("Several layers: after Import you choose which are cut, marked or left out.".into(), th().text_dim));
            }
            for n in notes {
                v.push((format!("⚠ {n}"), th().warn_fg));
            }
        }
        Err(e) => {
            v.push((pp.name.clone(), th().text_strong));
            v.push((format!("{} · {} layer{}", format.label(), pp.layers, if pp.layers == 1 { "" } else { "s" }), th().text_dim));
            v.push((format!("⚠ No part can be made from it: {e}"), th().warn_fg));
        }
    }
    v
}

/// Draw one part of the drawing in `rect`: the part as it will come in, with
/// its width and height; or, when no part can be made, the drawing's lines.
fn paint_preview(ui: &egui::Ui, pp: &PreviewPart, unit: LenUnit, rect: Rect, color: usize) {
    let p = ui.painter_at(rect);
    match &pp.part {
        Ok((part, _)) => {
            let (w, h) = (part.w.max(1e-6), part.h.max(1e-6));
            let inner = rect.shrink(30.0);
            let s = (inner.width() / w as f32).min(inner.height() / h as f32);
            let o = inner.center() + Vec2::new(-(w as f32) * s / 2.0, h as f32 * s / 2.0);
            let map = |q: crate::nest::P| Pos2::new(o.x + q[0] as f32 * s, o.y - q[1] as f32 * s);
            let col = part_color(color);
            paint_part(&p, part, map, col, Stroke::new(1.2, darker(col, 0.5)), th().card);
            // Width under the part, height on its right.
            let dim = Stroke::new(1.0, th().text_dim);
            let (x0, x1, y0, y1) = (o.x, o.x + w as f32 * s, o.y - h as f32 * s, o.y);
            let yd = y1 + 12.0;
            p.line_segment([Pos2::new(x0, yd), Pos2::new(x1, yd)], dim);
            for x in [x0, x1] {
                p.line_segment([Pos2::new(x, yd - 4.0), Pos2::new(x, yd + 4.0)], dim);
            }
            let font = FontId::proportional(11.0);
            let wt = p.layout_no_wrap(unit.show(part.w), font.clone(), th().text);
            let wr = Rect::from_center_size(Pos2::new((x0 + x1) / 2.0, yd), wt.size() + Vec2::new(6.0, 0.0));
            p.rect_filled(wr, 2.0, th().card);
            p.galley(wr.left_top() + Vec2::new(3.0, 0.0), wt, th().text);
            let xd = x1 + 12.0;
            p.line_segment([Pos2::new(xd, y0), Pos2::new(xd, y1)], dim);
            for y in [y0, y1] {
                p.line_segment([Pos2::new(xd - 4.0, y), Pos2::new(xd + 4.0, y)], dim);
            }
            let ht = p.layout_no_wrap(unit.show(part.h), font, th().text);
            let (gw, gh) = (ht.size().x, ht.size().y);
            let ym = (y0 + y1) / 2.0;
            p.rect_filled(Rect::from_center_size(Pos2::new(xd, ym), Vec2::new(gh, gw + 6.0)), 2.0, th().card);
            p.add(egui::epaint::TextShape::new(Pos2::new(xd - gh / 2.0, ym + gw / 2.0), ht, th().text).with_angle(-std::f32::consts::FRAC_PI_2));
        }
        Err(_) => {
            let (w, h) = ((pp.max[0] - pp.min[0]).max(1e-6), (pp.max[1] - pp.min[1]).max(1e-6));
            let inner = rect.shrink(16.0);
            let s = (inner.width() / w as f32).min(inner.height() / h as f32);
            let o = inner.center() + Vec2::new(-(w as f32) * s / 2.0, h as f32 * s / 2.0);
            let map = |q: &crate::nest::P| Pos2::new(o.x + (q[0] - pp.min[0]) as f32 * s, o.y - (q[1] - pp.min[1]) as f32 * s);
            for (role, pts) in &pp.pieces {
                let c = match role {
                    crate::dxf::LayerRole::Mark => th().accent_fg,
                    _ => th().text_strong,
                };
                // Points closer than half a pixel add nothing on screen.
                let mut line: Vec<Pos2> = Vec::with_capacity(pts.len());
                for q in pts {
                    let s = map(q);
                    if line.last().is_none_or(|l| l.distance(s) > 0.5) {
                        line.push(s);
                    }
                }
                p.add(egui::Shape::line(line, Stroke::new(1.0, c)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, dir: bool, modified: u64, size: u64) -> Entry {
        Entry { name: name.into(), path: PathBuf::from(name), dir, modified: Some(modified), size }
    }

    #[test]
    fn names_sort_as_explorer_does() {
        let mut v = vec!["DXFPAR0100.dxf", "DXFPAR11.DXF", "dxfpar10.dxf", "DXFPAR0101.dxf", "Part2.dxf", "part10.dxf"];
        v.sort_by(|a, b| natural(a, b));
        assert_eq!(v, ["dxfpar10.dxf", "DXFPAR11.DXF", "DXFPAR0100.dxf", "DXFPAR0101.dxf", "Part2.dxf", "part10.dxf"]);
    }

    #[test]
    fn folders_come_first_and_the_filter_hides_other_files() {
        let entries = vec![e("B.dxf", false, 3, 10), e("notes.txt", false, 1, 5), e("INCH", true, 9, 0), e("A.DWG", false, 2, 30), e("job.xml", false, 4, 1)];
        let names = |v: Vec<&Entry>| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(rows(&entries, Filter::Drawings, "", SortBy::Name, false)), ["INCH", "A.DWG", "B.dxf", "job.xml"]);
        assert_eq!(names(rows(&entries, Filter::Only(Format::Dxf), "", SortBy::Name, false)), ["INCH", "B.dxf"]);
        assert_eq!(names(rows(&entries, Filter::Everything, "", SortBy::Size, true)), ["INCH", "A.DWG", "B.dxf", "notes.txt", "job.xml"]);
        assert_eq!(names(rows(&entries, Filter::Drawings, "b.D", SortBy::Name, false)), ["B.dxf"]);
        assert_eq!(names(rows(&entries, Filter::Drawings, "", SortBy::Date, true)), ["INCH", "job.xml", "B.dxf", "A.DWG"]);
    }

    #[test]
    fn recent_folders_keep_the_newest_first_without_repeats() {
        let mut l = vec![];
        for d in ["a", "b", "a", "c", "d", "e", "f", "g"] {
            remember_dir(&mut l, d);
        }
        assert_eq!(l, ["g", "f", "e", "d", "c", "a"]);
    }

    #[test]
    fn a_new_pick_replaces_a_typed_file_name() {
        let mut iu = ImportUi::default();
        iu.picked.insert(PathBuf::from("A.dxf"));
        iu.sync_name();
        iu.name_text = "typed.dxf".into();
        iu.sync_name();
        assert_eq!(iu.name_text, "typed.dxf");
        iu.picked = [PathBuf::from("B.dxf")].into();
        iu.sync_name();
        assert_eq!(iu.name_text, "B.dxf");
    }

    #[test]
    fn a_unit_change_reads_the_preview_again() {
        let d = std::env::temp_dir().join(format!("nestpro_importui_units_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        // No $INSUNITS: read in the app's unit (IU1).
        let f = d.join("DEMO-RECT.dxf");
        std::fs::write(&f, "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n100\n20\n0\n10\n100\n20\n50\n10\n0\n20\n50\n0\nENDSEC\n0\nEOF\n").unwrap();
        let mut iu = ImportUi { sync: true, ..Default::default() };
        assert!(iu.go(d.clone()));
        iu.focus = Some(f.clone());
        let spec = crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 10.0, gap: 10.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        let width = |iu: &ImportUi| match iu.cache.get(&f) {
            Some((_, _, Shown::Drawing(p))) => p.parts[0].part.as_ref().unwrap().0.w,
            _ => panic!("no preview"),
        };
        let mut cfg = AppSettings::default();
        let ctx = egui::Context::default();
        iu.want_preview(&cfg, &spec, &ctx);
        assert!((width(&iu) - 100.0).abs() < 1e-6);
        cfg.nesting.units = LenUnit::Inch;
        iu.want_preview(&cfg, &spec, &ctx);
        assert!((width(&iu) - 2540.0).abs() < 1e-6);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_window_lists_a_folder_and_previews_a_drawing() {
        let d = std::env::temp_dir().join(format!("nestpro_importui_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("INCH")).unwrap();
        std::fs::write(d.join("DEMO-PLATE.dxf"), crate::dxf::SAMPLE_DXF).unwrap();
        std::fs::write(d.join("readme.txt"), "x").unwrap();
        let mut iu = ImportUi::default();
        assert!(iu.go(d.clone()));
        let v = iu.visible();
        assert_eq!(v.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["INCH", "DEMO-PLATE.dxf"]);
        // Down twice: past the folder to the drawing, which is picked.
        iu.focus_row(&v, 0, false);
        iu.focus_row(&v, 1, false);
        assert_eq!(iu.picked.iter().collect::<Vec<_>>(), [&d.join("DEMO-PLATE.dxf")]);
        assert_eq!(iu.picked_names(), "DEMO-PLATE.dxf");
        let cfg = AppSettings::default();
        let spec = crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 10.0, gap: 10.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        match read_preview(&cfg, &spec, &d.join("DEMO-PLATE.dxf")) {
            Shown::Drawing(p) => {
                let (part, _) = p.parts[0].part.as_ref().unwrap();
                assert!(part.w > 0.0 && part.h > 0.0 && p.parts[0].layers >= 1);
            }
            Shown::Failed(e) => panic!("{e}"),
            Shown::Xml { .. } => panic!("not XML"),
        }
        assert!(matches!(read_preview(&cfg, &spec, &d.join("readme.txt")), Shown::Failed(_)));
        // Up from the folder lands on it in its parent.
        assert!(iu.go(d.join("INCH")));
        iu.up();
        assert_eq!(iu.dir, d);
        assert_eq!(iu.focus.as_deref(), Some(d.join("INCH").as_path()));
        let _ = std::fs::remove_dir_all(&d);
    }
}
