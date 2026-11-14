//! The Import drawings window: a file browser inside the app that shows the
//! picked drawing before it's imported (IM10), as the original's Open dialog
//! does. The system file picker can't draw that preview, so this takes its
//! place; "System dialog…" still opens the system one. Its cut technology
//! panel sets the drawing's layers and paints its contours before the part
//! is made (IM11), through the same engine functions as `import-drawing`
//! (`engine::imports::chosen_part`, `set_layer`); this module only draws and
//! passes clicks along.

use crate::cutpanel::{self, CutPanel, PaintTarget, Row};
use crate::dxf::LayerRole;
use crate::engine::contours;
use crate::engine::imports::{self, drawing_preview, ContourChoice, DrawingChoices, DrawingPreview, PreviewPart};
use crate::nest::{Part, SheetSpec, P};
use crate::qualities::CutQualities;
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

/// The fingerprint of file `path` as it is now (`imports::fingerprint`):
/// Import compares it with the bytes the preview read, so a drawing changed
/// since is never brought in as it was set, whatever its size and time say
/// (IM11).
fn file_fingerprint(path: &Path) -> Option<u64> {
    std::fs::read(path).ok().map(|b| imports::fingerprint(&b))
}

/// A file's last change and the settings it was read with (`preview_key`):
/// choices made on one reading don't carry to another.
type Stamp = (Option<u64>, String);

/// One layer row of the shown drawing in the cut technology panel (IM11).
#[derive(Clone, Debug, PartialEq)]
pub struct LayerRow {
    pub name: String,
    /// Objects on it.
    pub objects: usize,
    pub role: LayerRole,
    /// Why it starts as Leave out (DX3), for its hover text.
    pub why: Option<String>,
}

/// A drawing whose layers or contours were set in the preview (IM11): the
/// reading they were set on, kept apart from the preview's cache (which
/// forgets old previews), and what was set.
#[derive(Clone)]
struct Edited {
    stamp: Stamp,
    read: Arc<DrawingPreview>,
    choices: DrawingChoices,
}

/// A picked drawing at Import (IM11).
pub enum Chosen {
    /// As the preview read it, with what was set for it there.
    Read(Arc<DrawingPreview>, DrawingChoices),
    /// Not read yet: read it with the roles it starts with.
    Unread,
    /// Set in the preview, but the file (or the import settings) changed
    /// since: read it again as it starts, and say so.
    Changed,
}

/// The shown drawing's layer rows with the roles `choices` give them.
fn layer_rows(d: &DrawingPreview, choices: &DrawingChoices) -> Vec<LayerRow> {
    let roles = choices.roles(&d.reads);
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    let mut why: std::collections::BTreeMap<String, String> = Default::default();
    for r in &d.reads {
        for (l, n) in r.part.drawing.layers() {
            *counts.entry(l).or_insert(0) += n;
        }
        for (l, w) in &r.part.drawing.hidden {
            why.entry(l.clone()).or_insert_with(|| w.clone());
        }
    }
    counts.into_iter().map(|(name, objects)| LayerRow { role: roles.get(&name).copied().unwrap_or(LayerRole::Cut), why: why.get(&name).cloned(), name, objects }).collect()
}

/// The focused drawing with what was set for it (IM11).
struct Current {
    path: PathBuf,
    stamp: Stamp,
    /// The part of the file shown.
    ix: usize,
    d: Arc<DrawingPreview>,
    choices: DrawingChoices,
    /// The part it makes with them, or why none can be made.
    made: Result<(Part, Vec<String>), String>,
}

/// The part about to be imported, painted in the preview's cut technology
/// panel (IM11): each paint is the engine's `contours::paint` and is kept as
/// a contour choice, so Import makes the same part (`imports::chosen_part`).
struct PreviewPaint<'a> {
    cur: &'a mut Current,
    names: &'a CutQualities,
    undo: &'a mut Vec<(PathBuf, Edited)>,
    note: &'a mut String,
}

impl PaintTarget for PreviewPaint<'_> {
    type Before = DrawingChoices;

    fn part(&self) -> Option<&Part> {
        self.cur.made.as_ref().ok().map(|m| &m.0)
    }

    fn paint_now(&mut self, picks: &[usize], row: Row, same: bool) -> Result<String, String> {
        let part = self.part().cloned().filter(|_| !picks.is_empty()).ok_or("No contour picked.")?;
        let mut parts = vec![part];
        let done = contours::paint(&mut parts, &[], &mut [], 0, picks, row.paint(), same)?;
        self.cur.choices.contours.entry(self.cur.ix).or_default().push(ContourChoice { contours: picks.to_vec(), paint: row.paint(), same });
        if let Ok(m) = self.cur.made.as_mut() {
            m.0 = parts.swap_remove(0);
        }
        let n: Vec<String> = done.contours.iter().map(|k| (k + 1).to_string()).collect();
        Ok(format!("Contour{} {}: {}.", if n.len() == 1 { "" } else { "s" }, n.join(", "), row.label(self.names)))
    }

    fn before(&self) -> DrawingChoices {
        self.cur.choices.clone()
    }

    fn push_undo(&mut self, before: DrawingChoices) {
        self.undo.push((self.cur.path.clone(), Edited { stamp: self.cur.stamp.clone(), read: self.cur.d.clone(), choices: before }));
    }

    fn say(&mut self, s: String) {
        *self.note = s;
    }
}

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
    /// What was set in the preview for each drawing (IM11), with the reading
    /// it was set on.
    edited: HashMap<PathBuf, Edited>,
    /// The cut technology panel in the preview (IM11).
    pub panel: CutPanel<DrawingChoices>,
    /// Ctrl+Z in the preview: a drawing's choices before each paint or layer change.
    undo: Vec<(PathBuf, Edited)>,
    /// The part the shown drawing makes with its choices, for (file, reading,
    /// part, choices).
    shown: Option<(PathBuf, Stamp, usize, DrawingChoices, Result<(Part, Vec<String>), String>)>,
    /// What the last paint or layer change did, or why not.
    pub note: String,
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
            edited: HashMap::new(),
            panel: CutPanel::default(),
            undo: vec![],
            shown: None,
            note: String::new(),
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
        self.edited.clear();
        self.undo.clear();
        self.shown = None;
        self.note.clear();
        self.panel.row = None;
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
            self.file_preview(p, m, k, s);
        }
    }

    /// Keep a preview, forgetting the older ones when there are too many
    /// (what was set for a drawing is kept apart, in `edited`).
    fn file_preview(&mut self, p: PathBuf, m: Option<u64>, k: String, s: Shown) {
        if self.cache.len() >= MAX_CACHED {
            self.cache.clear();
        }
        self.cache.insert(p, (m, k, s));
    }

    /// Start reading the focused file when it has no preview yet (one at a
    /// time: moving quickly down the list reads only where it stops).
    pub(crate) fn want_preview(&mut self, cfg: &AppSettings, spec: &crate::nest::SheetSpec, ctx: &egui::Context) {
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
            self.file_preview(path, m, key, s);
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

    /// The stamp file `path` has now: its last change and these settings.
    fn stamp_now(&self, path: &Path, cfg: &AppSettings, spec: &SheetSpec) -> Option<Stamp> {
        let modified = match self.entries.iter().find(|e| e.path == path) {
            Some(e) => e.modified,
            None => std::fs::metadata(path).ok()?.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
        };
        Some((modified, preview_key(cfg, spec)))
    }

    /// File `path` as the preview read it, when it has been read with these
    /// settings since its last change, and the stamp of that reading: the
    /// reading something was set on, else the preview's.
    fn read_of(&self, path: &Path, cfg: &AppSettings, spec: &SheetSpec) -> Option<(Stamp, Arc<DrawingPreview>)> {
        let stamp = self.stamp_now(path, cfg, spec)?;
        if let Some(e) = self.edited.get(path).filter(|e| e.stamp == stamp) {
            return Some((stamp, e.read.clone()));
        }
        match self.cache.get(path) {
            Some((m, k, Shown::Drawing(d))) if (m, k) == (&stamp.0, &stamp.1) => Some((stamp, d.clone())),
            _ => None,
        }
    }

    /// What was set for `path` on the reading `stamp` (IM11).
    fn choices_for(&self, path: &Path, stamp: &Stamp) -> DrawingChoices {
        self.edited.get(path).filter(|e| &e.stamp == stamp).map(|e| e.choices.clone()).unwrap_or_default()
    }

    /// Each picked file at Import (IM11): as the preview read it with what
    /// was set for it, not read yet, or set on a reading the file no longer
    /// has.
    pub fn chosen(&self, files: &[PathBuf], cfg: &AppSettings, spec: &SheetSpec) -> Vec<(PathBuf, Chosen)> {
        files
            .iter()
            .map(|f| {
                // The file's bytes now, not the folder listing or the file's
                // size and time, which may not show a change.
                let same_file = |e: &Edited| file_fingerprint(f) == Some(e.read.fingerprint) && Some(&e.stamp) == self.stamp_now(f, cfg, spec).as_ref();
                let chosen = match self.edited.get(f) {
                    Some(e) if same_file(e) => Chosen::Read(e.read.clone(), e.choices.clone()),
                    Some(_) => Chosen::Changed,
                    // Nothing set: the preview's reading only while the bytes are the
                    // same, else read it again as it is now (IM11).
                    None => self.read_of(f, cfg, spec).filter(|(_, d)| file_fingerprint(f) == Some(d.fingerprint)).map_or(Chosen::Unread, |(_, d)| Chosen::Read(d, DrawingChoices::default())),
                };
                (f.clone(), chosen)
            })
            .collect()
    }

    /// The focused drawing, its part shown and the part it makes with what
    /// was set for it (made again only when that changed).
    fn current(&mut self, cfg: &AppSettings, spec: &SheetSpec) -> Option<Current> {
        let path = self.focus.clone()?;
        let (stamp, d) = self.read_of(&path, cfg, spec)?;
        let ix = self.part_ix.min(d.parts.len().checked_sub(1)?);
        let choices = self.choices_for(&path, &stamp);
        if self.edited.get(&path).is_some_and(|e| e.stamp != stamp) {
            self.note = "The file or the import settings changed after its layers and contours were set here: they are shown as they start. Set them again.".into();
        }
        let fresh = matches!(&self.shown, Some((p, st, i, c, _)) if *p == path && *st == stamp && *i == ix && *c == choices);
        if !fresh {
            let made = if choices == DrawingChoices::default() { d.parts[ix].part.clone() } else { imports::chosen_part(cfg, spec, &d.reads[ix], &choices.roles(&d.reads), choices.of(ix), 0) };
            self.shown = Some((path.clone(), stamp.clone(), ix, choices.clone(), made));
        }
        let made = self.shown.as_ref().map(|s| s.4.clone())?;
        Some(Current { path, stamp, ix, d, choices, made })
    }

    /// Keep the part the drawing `cur` shows, and what was set for it when
    /// `edits` changed it. Otherwise what was set stays as it was, so a
    /// drawing set on an earlier reading of its file is still reported at
    /// Import.
    fn keep(&mut self, cur: Current, edits: bool) {
        if edits && cur.choices == DrawingChoices::default() {
            self.edited.remove(&cur.path);
        } else if edits {
            self.edited.insert(cur.path.clone(), Edited { stamp: cur.stamp.clone(), read: cur.d.clone(), choices: cur.choices.clone() });
        }
        self.shown = Some((cur.path, cur.stamp, cur.ix, cur.choices, cur.made));
    }

    /// Run `f` with the shown part as the panel's paint target (IM11); `None`
    /// when no drawing is shown.
    fn with_paint<R>(&mut self, cfg: &AppSettings, spec: &SheetSpec, names: &CutQualities, f: impl FnOnce(&mut PreviewPaint, &mut CutPanel<DrawingChoices>) -> R) -> Option<R> {
        let mut cur = self.current(cfg, spec)?;
        let mut panel = std::mem::take(&mut self.panel);
        let before = cur.choices.clone();
        let r = f(&mut PreviewPaint { cur: &mut cur, names, undo: &mut self.undo, note: &mut self.note }, &mut panel);
        self.panel = panel;
        let edits = cur.choices != before;
        self.keep(cur, edits);
        Some(r)
    }

    /// Set layer row `layer` of the shown drawing to `role` (IM11): its parts
    /// are made again with it (`imports::set_layer`), one undo step.
    fn set_layer(&mut self, cfg: &AppSettings, spec: &SheetSpec, layer: &str, role: LayerRole) {
        let Some(mut cur) = self.current(cfg, spec) else { return };
        if cur.choices.roles(&cur.d.reads).get(layer) == Some(&role) {
            return;
        }
        let (next, gave_way) = imports::set_layer(cfg, spec, &cur.d.reads, &cur.choices, layer, role);
        let before = std::mem::replace(&mut cur.choices, next);
        self.undo.push((cur.path.clone(), Edited { stamp: cur.stamp.clone(), read: cur.d.clone(), choices: before }));
        let what = match role {
            LayerRole::Cut => "its lines are cut (open ones stay Bend line or No cut)",
            LayerRole::Mark => "its lines are marked",
            LayerRole::Ignore => "its lines are left out",
        };
        let gave = match gave_way {
            0 => String::new(),
            1 => " 1 contour painted by hand took the layer's choice.".into(),
            n => format!(" {n} contours painted by hand took the layer's choice."),
        };
        self.note = format!("Layer {layer}: {}, {what}.{gave}", role.label());
        cur.made = Err(String::new());
        self.keep(cur, true);
        // Made again with the new roles when next shown.
        self.shown = None;
    }

    /// Ctrl+Z in the preview: take back the last paint or layer change on
    /// the shown drawing. False when there is none.
    fn undo_last(&mut self) -> bool {
        let Some(path) = self.focus.clone() else { return false };
        let Some(at) = self.undo.iter().rposition(|u| u.0 == path) else { return false };
        let (path, before) = self.undo.remove(at);
        if before.choices == DrawingChoices::default() {
            self.edited.remove(&path);
        } else {
            self.edited.insert(path, before);
        }
        self.shown = None;
        self.note = "Undone.".into();
        true
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
            if let Some(d) = files.first().and_then(|f| f.parent()) {
                remember_dir(&mut self.cfg.import_dirs, &d.display().to_string());
            }
            // As if dropped: a drawing with several layer rows comes back to
            // the window to set them (IM12).
            self.import_drawings(files);
            self.apply_settings(false);
        }
    }

    /// Import the files picked in the window, remembering their folder: each
    /// drawing with the layers and contours set for it in the preview (IM11),
    /// the same part `import-drawing` makes with them.
    fn import_from_window(&mut self, files: Vec<PathBuf>) {
        if let Some(d) = files.first().and_then(|f| f.parent()) {
            remember_dir(&mut self.cfg.import_dirs, &d.display().to_string());
        }
        self.importui.open = false;
        self.importui.panel.row = None;
        let chosen = self.importui.chosen(&files, &self.cfg, &self.spec);
        self.import_chosen(chosen);
        self.apply_settings(false);
    }

    /// The shown drawing's layer rows (IM11).
    pub fn preview_layers(&mut self) -> Vec<LayerRow> {
        self.importui.current(&self.cfg, &self.spec).map_or(vec![], |c| layer_rows(&c.d, &c.choices))
    }

    /// The part the shown drawing will come in as, with what was set for it.
    pub fn preview_part(&mut self) -> Option<Part> {
        self.importui.current(&self.cfg, &self.spec).and_then(|c| c.made.ok()).map(|m| m.0)
    }

    /// Paint contours `picks` (numbers from 0) of the shown part with `row`, as
    /// a click in the preview does (IM11, CS6): one undo step; the panel says
    /// what it did or why not. True when something changed.
    pub fn preview_paint(&mut self, picks: &[usize], row: Row, same: bool) -> bool {
        let names = crate::ct1::for_spec(&self.spec).qualities;
        self.importui.with_paint(&self.cfg, &self.spec, &names, |t, _| cutpanel::paint_contours(t, picks, row, same)).unwrap_or(false)
    }

    /// Set a layer row of the shown drawing (IM11).
    pub fn preview_set_layer(&mut self, layer: &str, role: LayerRole) {
        self.importui.set_layer(&self.cfg, &self.spec, layer, role);
    }

    /// Ctrl+Z in the preview.
    pub fn preview_undo(&mut self) -> bool {
        self.importui.undo_last()
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
            let (mut down, mut up, mut home, mut end, mut enter, mut back, mut all, mut space, mut esc, mut undo) = (false, false, false, false, false, false, false, false, false, false);
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
                // The job behind isn't undone while the window is open.
                undo = i.consume_key(Modifiers::COMMAND, Key::Z);
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
            if esc && self.importui.panel.row.is_some() {
                // Esc ends painting first (CS6), then closes the window.
                let names = crate::ct1::for_spec(&self.spec).qualities;
                let ended = self.importui.with_paint(&self.cfg, &self.spec, &names, |t, p| cutpanel::end_painting(t, p, &names));
                if ended.is_none() {
                    self.importui.panel.row = None;
                }
            } else if esc {
                act = Act::Close;
            }
            if undo && !self.preview_undo() {
                self.importui.note = "Nothing to undo in the preview.".into();
            }
        }
        self.importui.sync_name();

        let rows = self.importui.visible();
        let mut open = true;
        let unit = self.cfg.nesting.units;
        let panel = self.importui.panel.open;
        egui::Window::new(RichText::new("Import part drawings").size(16.0).strong())
            .id(egui::Id::unique("import_browser"))
            .pivot(Align2::CENTER_TOP)
            .default_pos(dialog_pos(ctx))
            .default_size([if panel { 1240.0 } else { 1040.0 }, if panel { 680.0 } else { 620.0 }])
            .min_size([if panel { 980.0 } else { 760.0 }, 440.0])
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                self.import_toolbar(ui, &mut act);
                ui.add_space(4.0);
                let bottom_h = 82.0;
                let body_h = (ui.available_height() - bottom_h).max(200.0);
                let full_w = ui.available_width();
                ui.allocate_ui_with_layout(Vec2::new(full_w, body_h), egui::Layout::left_to_right(Align::Min), |ui| {
                    let gap = ui.spacing().item_spacing.x;
                    if panel {
                        // IM11: the preview grows and the cut technology panel sits on
                        // its right; the places column makes room for them.
                        let (list_w, panel_w) = (330.0, 270.0);
                        let prev_w = (full_w - list_w - panel_w - 2.0 * gap).max(300.0);
                        ui.allocate_ui_with_layout(Vec2::new(list_w, body_h), egui::Layout::top_down(Align::Min), |ui| self.import_list(ui, &rows, list_w, body_h, &mut act));
                        ui.allocate_ui_with_layout(Vec2::new(prev_w, body_h), egui::Layout::top_down(Align::Min), |ui| self.import_preview_pane(ui, prev_w, unit, &mut act));
                        ui.allocate_ui_with_layout(Vec2::new(panel_w, body_h), egui::Layout::top_down(Align::Min), |ui| self.import_cut_panel(ui, panel_w, body_h));
                    } else {
                        let side_w = 170.0;
                        let prev_w = 300.0;
                        let list_w = (full_w - side_w - prev_w - 2.0 * gap).max(240.0);
                        ui.allocate_ui_with_layout(Vec2::new(side_w, body_h), egui::Layout::top_down(Align::Min), |ui| self.import_places(ui));
                        ui.allocate_ui_with_layout(Vec2::new(list_w, body_h), egui::Layout::top_down(Align::Min), |ui| self.import_list(ui, &rows, list_w, body_h, &mut act));
                        ui.allocate_ui_with_layout(Vec2::new(prev_w, body_h), egui::Layout::top_down(Align::Min), |ui| self.import_preview_pane(ui, prev_w, unit, &mut act));
                    }
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
    /// what the import will say about it; Preview on/off, the cut technology
    /// panel (IM11) and the import settings.
    fn import_preview_pane(&mut self, ui: &mut egui::Ui, w: f32, unit: LenUnit, act: &mut Act) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Preview").strong().color(th().text_strong));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let r = ui.checkbox(&mut self.cfg.import_preview, "Show").on_hover_text("Read and draw the picked drawing (turn off to browse large folders on a slow network faster)");
                if r.changed() {
                    self.importui.loading = None;
                }
                let open = self.importui.panel.open;
                let tip = "Set the drawing's layers and paint its contours (cut, mark, no cut, bend line, leave out) before it is imported";
                if ui.selectable_label(open, "Cut technology").on_hover_text(tip).clicked() {
                    self.importui.panel.open = !open;
                    self.importui.panel.row = None;
                }
            });
        });
        let panel = self.importui.panel.open;
        let side = if panel { (ui.available_height() - 130.0).max(160.0) } else { w.min(ui.available_height() - 150.0).max(160.0) };
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, side), if panel { Sense::click_and_drag() } else { Sense::hover() });
        ui.painter().rect_filled(rect, 6.0, th().card);
        ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0, th().border), StrokeKind::Inside);
        // The drawing as it will come in, with what was set for it (IM11).
        let cur = self.importui.current(&self.cfg, &self.spec);
        let mut drawing: Option<Arc<DrawingPreview>> = None;
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
                    drawing = Some(d.clone());
                    if d.parts.len() > 1 {
                        parts_nav = Some(d.parts.len());
                    }
                }
            },
        }
        if let Some(d) = drawing {
            let ix = self.importui.part_ix.min(d.parts.len().saturating_sub(1));
            if let Some(pp) = d.parts.get(ix) {
                let made = cur.filter(|c| c.ix == ix).map_or_else(|| pp.part.clone(), |c| c.made);
                self.paint_preview(ui, &resp, pp, &made, ix, unit, rect);
                info.extend(preview_lines(pp, &made, d.format, unit, panel));
                // What the last paint or layer change did, or why not (IM11).
                if panel && !self.importui.note.is_empty() {
                    info.insert(info.len().min(2), (self.importui.note.clone(), th().accent_fg));
                }
            }
        }
        let iu = &mut self.importui;
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

/// Name, size, holes, format, layers, quantity and the import's notes for one
/// part, as `made` with what was set for it (IM11). `panel`: the cut
/// technology panel is open.
fn preview_lines(pp: &PreviewPart, made: &Result<(Part, Vec<String>), String>, format: Format, unit: LenUnit, panel: bool) -> Vec<(String, Color32)> {
    let mut v = vec![];
    match made {
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
            if pp.layers > 1 && !panel {
                v.push(("Several layers: Cut technology sets which are cut, marked or left out before Import.".into(), th().text_dim));
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

/// Where a part `w` × `h` mm sits in `inner`, as large as fits: the screen
/// point of its lower-left corner and pixels per mm.
fn fit(inner: Rect, w: f64, h: f64) -> (Pos2, f32) {
    let (w, h) = (w.max(1e-6), h.max(1e-6));
    let s = (inner.width() / w as f32).min(inner.height() / h as f32);
    (inner.center() + Vec2::new(-(w as f32) * s / 2.0, h as f32 * s / 2.0), s)
}

impl App {
    /// Draw one part of the drawing in `rect`: the part as it will come in
    /// (`made`), every contour in its state's or quality's colour (CS7), with
    /// its width and height; or, when no part can be made, the drawing's
    /// lines. With a row of the cut technology panel chosen, clicks, boxes
    /// and sweeps on it paint the part (IM11, CS6); `ix` is its place in the file.
    #[allow(clippy::too_many_arguments)]
    fn paint_preview(&mut self, ui: &egui::Ui, resp: &egui::Response, pp: &PreviewPart, made: &Result<(Part, Vec<String>), String>, ix: usize, unit: LenUnit, rect: Rect) {
        let p = ui.painter_at(rect);
        match made {
            Ok((part, _)) => {
                let (w, h) = (part.w.max(1e-6), part.h.max(1e-6));
                let (o, s) = fit(rect.shrink(30.0), w, h);
                let to_s = |q: P| Pos2::new(o.x + q[0] as f32 * s, o.y - q[1] as f32 * s);
                let to_w = |q: Pos2| [((q.x - o.x) / s) as f64, ((o.y - q.y) / s) as f64];
                let col = part_color(self.color_seq);
                paint_part(&p, part, to_s, col, Stroke::new(1.2, darker(col, 0.5)), th().card);
                let mt = crate::ct1::for_spec(&self.spec);
                cutpanel::paint_contour_colours(&p, part, &mt, &to_s, 1.4, self.importui.panel.open);
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
                // Painting with the panel (IM11): the same clicks, boxes and
                // sweeps as on the part's drawing screen (CS6).
                if self.importui.panel.painting() {
                    let half = (crate::partview::APERTURE_PX / s) as f64;
                    self.importui.with_paint(&self.cfg, &self.spec, &mt.qualities, |t, panel| cutpanel::canvas(t, panel, ui, resp, &p, ix, &to_s, &to_w, half, &mt.qualities));
                }
            }
            Err(_) => {
                let (w, h) = ((pp.max[0] - pp.min[0]).max(1e-6), (pp.max[1] - pp.min[1]).max(1e-6));
                let (o, s) = fit(rect.shrink(16.0), w, h);
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

    /// The preview's cut technology panel (IM11): the drawing's layer rows,
    /// each set to Cut, Mark or Leave out (rows in [ ] start as Leave out,
    /// DX3), then the rows that paint contours (CS6) and what the last change
    /// did.
    fn import_cut_panel(&mut self, ui: &mut egui::Ui, w: f32, h: f32) {
        let names = crate::ct1::for_spec(&self.spec).qualities;
        let cur = self.importui.current(&self.cfg, &self.spec);
        let frame = egui::Frame::new().fill(th().panel).stroke(Stroke::new(1.0, th().border)).corner_radius(4.0);
        let mut set: Option<(String, LayerRole)> = None;
        frame.show(ui, |ui| {
            ui.set_min_size(Vec2::new(w - 2.0, h - 2.0));
            ui.set_max_width(w - 2.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            crate::ui_parts::pane_header(ui, "Cut technology");
            let head = Rect::from_min_size(ui.min_rect().left_top(), Vec2::new(ui.available_width(), 24.0));
            let close = ui.put(Rect::from_center_size(head.right_center() - Vec2::new(14.0, 0.0), Vec2::splat(20.0)), egui::Button::new("✖").small().frame(false));
            if close.on_hover_text("Close the cut technology panel").clicked() {
                self.importui.panel.open = false;
                self.importui.panel.row = None;
            }
            egui::ScrollArea::vertical().id_salt("import_cut_panel").auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(6.0);
                let section = |ui: &mut egui::Ui, t: &str| {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(t).size(10.5).strong().color(th().text_dim));
                    });
                };
                section(ui, "LAYERS");
                match &cur {
                    None => {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.label(RichText::new("Pick a drawing to set its layers and contours.").size(11.5).color(th().text_dim));
                        });
                    }
                    Some(c) => {
                        for row in layer_rows(&c.d, &c.choices) {
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                let pick_w = 96.0;
                                let name_w = (ui.available_width() - pick_w - 14.0).max(40.0);
                                let (r, resp) = ui.allocate_exact_size(Vec2::new(name_w, 22.0), Sense::hover());
                                let p = ui.painter_at(r);
                                p.text(r.left_center(), Align2::LEFT_CENTER, &row.name, FontId::proportional(12.5), th().text_strong);
                                p.text(r.right_center() - Vec2::new(4.0, 0.0), Align2::RIGHT_CENTER, row.objects.to_string(), FontId::proportional(11.0), th().text_dim);
                                let why = row.why.as_ref().map_or(String::new(), |w| format!("\nStarts as Leave out: {w}."));
                                resp.on_hover_text(format!("{}: {} object(s){why}", row.name, row.objects));
                                egui::ComboBox::from_id_salt(("import_layer_role", &row.name)).width(pick_w).selected_text(row.role.label()).show_ui(ui, |ui| {
                                    for role in LayerRole::ALL {
                                        if ui.selectable_label(row.role == role, role.label()).clicked() && row.role != role {
                                            set = Some((row.name.clone(), role));
                                        }
                                    }
                                });
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.add(egui::Label::new(RichText::new("Setting a layer sets all its lines. Rows in [ ] hold what the drawing hides or draws as centre / dashed lines; they start as Leave out.").size(11.0).color(th().text_dim)).wrap());
                        });
                    }
                }
                ui.add_space(6.0);
                ui.separator();
                section(ui, "CONTOURS");
                let locked = match &cur {
                    None => Some("Pick a drawing first."),
                    Some(c) if c.made.is_err() => Some("No part can be made from this drawing with these layers: set the layers above."),
                    _ => None,
                };
                cutpanel::rows_ui(ui, &mut self.importui.panel, &names, locked);
            });
        });
        if let Some((layer, role)) = set {
            self.importui.set_layer(&self.cfg, &self.spec, &layer, role);
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
        let spec = crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 10.0, gap: 10.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() };
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

    /// The window open on a folder holding the made-up layered plate, the
    /// plate picked and its preview read, with a job of no parts behind it.
    fn window_on_layered_plate(name: &str) -> (App, PathBuf, PathBuf) {
        let d = std::env::temp_dir().join(format!("nestpro_importui_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("DEMO-LAYERED-PLATE.dxf");
        std::fs::write(&f, include_str!("../../examples/drawings/DEMO-LAYERED-PLATE.dxf")).unwrap();
        let mut a = App::new(None);
        a.parts.clear();
        a.result = Default::default();
        a.manual_cuts.clear();
        a.refresh();
        a.undo.clear();
        a.importui.sync = true;
        a.importui.show(std::slice::from_ref(&d));
        a.importui.focus = Some(f.clone());
        a.importui.picked.insert(f.clone());
        a.importui.want_preview(&a.cfg, &a.spec, &egui::Context::default());
        (a, d, f)
    }

    fn states(part: &crate::nest::Part) -> Vec<&'static str> {
        crate::engine::contours::list(part).iter().map(|c| c.state.key()).collect()
    }

    /// The part `import-drawing` makes from `file` with `args`, through the agents' session.
    fn agents_part(d: &Path, file: &Path, args: serde_json::Value) -> crate::nest::Part {
        let mut s = crate::engine::session::Session::new(crate::engine::Ctx::builtin());
        let job = d.join("agents.snjob");
        let new = serde_json::json!({"w": 3000, "h": 1500, "material": "steel", "thickness": 3, "force": true});
        s.call("new", Some(job.to_str().unwrap()), &new).unwrap();
        let mut args = args;
        args["path"] = serde_json::json!(file.to_str().unwrap());
        s.call("import-drawing", None, &args).unwrap();
        s.job.as_ref().unwrap().parts[0].clone()
    }

    #[test]
    fn contours_painted_in_the_preview_come_in_as_import_drawing_makes_them() {
        let (mut a, d, f) = window_on_layered_plate("paint");
        use crate::cutpanel::Row;
        // IM11: the drawing's layer rows, the hidden and centre-line ones
        // starting as Leave out (DX3), with why.
        let rows = a.preview_layers();
        let names: Vec<(&str, LayerRole)> = rows.iter().map(|r| (r.name.as_str(), r.role)).collect();
        assert_eq!(names, [("0", LayerRole::Cut), ("0 [CENTER]", LayerRole::Ignore), ("ETCH", LayerRole::Cut), ("OLD-REV [frozen]", LayerRole::Ignore)]);
        assert!(rows[3].why.as_deref().is_some_and(|w| w.contains("frozen")), "{:?}", rows[3].why);
        assert_eq!(states(&a.preview_part().unwrap()), ["cut", "cut", "cut", "cut", "cut", "bend-line", "no-cut"]);

        // A click on the stray open line with Mark, one on the big hole with
        // Leave out: each paints at once.
        assert!(a.preview_paint(&[6], Row::Mark, false));
        assert!(a.preview_paint(&[0], Row::LeaveOut, false));
        assert_eq!(states(&a.preview_part().unwrap()), ["leave-out", "cut", "cut", "cut", "cut", "bend-line", "mark"]);
        // The outline can't be marked: refused, saying why, nothing changes.
        assert!(!a.preview_paint(&[4], Row::Mark, false));
        assert!(a.importui.note.contains("outline"), "{}", a.importui.note);

        // Import brings in the part as painted ...
        a.import_from_window(vec![f.clone()]);
        let got = a.parts.last().expect("a part was imported").clone();
        assert_eq!(states(&got), ["leave-out", "cut", "cut", "cut", "cut", "bend-line", "mark"]);
        // ... the same part import-drawing makes with those choices.
        let agents = agents_part(&d, &f, serde_json::json!({"contours": "7=mark; 1=leave-out"}));
        assert_eq!(crate::engine::contours::list(&got), crate::engine::contours::list(&agents));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_layer_set_in_the_panel_paints_its_contours_and_single_contours_keep_their_paint() {
        let (mut a, d, f) = window_on_layered_plate("layers");
        use crate::cutpanel::Row;
        let shown = |a: &mut App| states(&a.preview_part().unwrap()).join(" ");
        // The stray line marked and the etched frame (contour 4) No cut by hand.
        a.preview_paint(&[6], Row::Mark, false);
        a.preview_paint(&[3], Row::NoCut, false);
        // ETCH set to Mark: its frame takes the layer's Mark; the stray line,
        // on another layer, keeps its paint.
        a.preview_set_layer("ETCH", LayerRole::Mark);
        assert_eq!(shown(&mut a), "cut cut cut mark cut bend-line mark");
        assert!(a.importui.note.contains("1 contour painted by hand"), "{}", a.importui.note);
        // The frozen row set to Cut: its old hole comes in among the holes
        // (contour 4), and the stray line, now contour 8, is still marked.
        a.preview_set_layer("OLD-REV [frozen]", LayerRole::Cut);
        assert_eq!(shown(&mut a), "cut cut cut cut mark cut bend-line mark");
        // ETCH left out, then cut: its frame is cut, the layer's choice.
        a.preview_set_layer("ETCH", LayerRole::Ignore);
        assert_eq!(shown(&mut a), "cut cut cut cut cut bend-line mark");
        a.preview_set_layer("ETCH", LayerRole::Cut);
        assert_eq!(shown(&mut a), "cut cut cut cut cut cut bend-line mark");
        // Ctrl+Z takes each change back, one at a time.
        assert!(a.preview_undo());
        assert_eq!(shown(&mut a), "cut cut cut cut cut bend-line mark");
        assert!(a.preview_undo());
        assert_eq!(shown(&mut a), "cut cut cut cut mark cut bend-line mark");
        let roles: Vec<(String, LayerRole)> = a.preview_layers().into_iter().map(|r| (r.name, r.role)).collect();
        assert_eq!(roles[2], ("ETCH".to_string(), LayerRole::Mark));
        assert_eq!(roles[3], ("OLD-REV [frozen]".to_string(), LayerRole::Cut));

        // Import: the part import-drawing makes with those layers and the
        // stray line marked.
        a.import_from_window(vec![f.clone()]);
        let got = a.parts.last().unwrap().clone();
        let agents = agents_part(&d, &f, serde_json::json!({"layers": "ETCH=mark,OLD-REV [frozen]=cut", "contours": "8=mark"}));
        assert_eq!(crate::engine::contours::list(&got), crate::engine::contours::list(&agents));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn what_was_set_in_the_preview_survives_the_preview_forgetting_the_drawing() {
        let (mut a, d, f) = window_on_layered_plate("evict");
        use crate::cutpanel::Row;
        // The big hole left out and the etched frame marked by its layer.
        a.preview_paint(&[0], Row::LeaveOut, false);
        a.preview_set_layer("ETCH", LayerRole::Mark);
        let edited = ["leave-out", "cut", "cut", "mark", "cut", "bend-line", "no-cut"];
        assert_eq!(states(&a.preview_part().unwrap()), edited);
        // Read the folder again (⟳) and preview more drawings than the window
        // keeps: the plate's preview is forgotten, the plate stays picked.
        let square = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n50\n20\n0\n10\n50\n20\n50\n10\n0\n20\n50\n0\nENDSEC\n0\nEOF\n";
        for k in 0..=MAX_CACHED {
            std::fs::write(d.join(format!("DEMO-SQUARE-{k}.dxf")), square).unwrap();
        }
        a.importui.refresh();
        for k in 0..=MAX_CACHED {
            a.importui.focus = Some(d.join(format!("DEMO-SQUARE-{k}.dxf")));
            a.importui.want_preview(&a.cfg, &a.spec, &egui::Context::default());
        }
        assert!(!a.importui.cache.contains_key(&f), "the plate's preview should be forgotten");
        assert!(a.importui.picked.contains(&f));
        // Import still brings in the plate as it was set.
        a.import_from_window(vec![f.clone()]);
        assert_eq!(states(a.parts.last().unwrap()), edited);

        // Set again, then the file changes on disk: it comes in as it starts,
        // and the import says why, never quietly.
        a.importui.open = true;
        a.importui.focus = Some(f.clone());
        a.importui.want_preview(&a.cfg, &a.spec, &egui::Context::default());
        assert!(a.preview_paint(&[0], Row::LeaveOut, false));
        let older = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&f).unwrap().set_modified(older).unwrap();
        a.importui.refresh();
        // Even previewed again as it is now.
        a.importui.want_preview(&a.cfg, &a.spec, &egui::Context::default());
        assert_eq!(states(&a.preview_part().unwrap())[0], "cut");
        assert!(a.importui.note.contains("changed"), "{}", a.importui.note);
        a.import_from_window(vec![f.clone()]);
        assert_eq!(states(a.parts.last().unwrap())[0], "cut");
        let card = a.result_card.as_ref().expect("a result card").lines.join(" ");
        assert!(card.contains("changed") && card.contains("set them again"), "{card}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_drawing_replaced_on_disk_before_import_is_reported_not_imported_as_set() {
        let (mut a, d, f) = window_on_layered_plate("replaced");
        use crate::cutpanel::Row;
        assert!(a.preview_paint(&[0], Row::LeaveOut, false));
        // Replaced outside the app (another drawing), the folder not read again:
        // its listing still has the old time.
        std::fs::write(&f, include_str!("../../examples/drawings/DEMO-BEND-PLATE.dxf")).unwrap();
        a.import_from_window(vec![f.clone()]);
        // The drawing as it is now comes in as it starts, and the import says so.
        let got = a.parts.last().unwrap();
        assert_eq!(states(got), ["cut", "cut", "cut", "cut", "cut", "cut", "bend-line", "no-cut"], "the bend plate, as it starts");
        let card = a.result_card.as_ref().expect("a result card").lines.join(" ");
        assert!(card.contains("changed") && card.contains("set them again"), "{card}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_drawing_changed_with_the_same_size_and_time_is_reported_not_imported_as_set() {
        let (mut a, d, f) = window_on_layered_plate("same_size");
        use crate::cutpanel::Row;
        assert!(a.preview_paint(&[0], Row::LeaveOut, false));
        // The plate made 400 long instead of 300: the same number of bytes,
        // and its last-change time put back, as a coarse clock or a network
        // share's cached details would show it.
        let when = std::fs::metadata(&f).unwrap().modified().unwrap();
        let text = std::fs::read_to_string(&f).unwrap();
        assert_eq!(text.matches("300.0000").count(), 2);
        std::fs::write(&f, text.replace("300.0000", "400.0000")).unwrap();
        std::fs::File::options().write(true).open(&f).unwrap().set_modified(when).unwrap();
        a.import_from_window(vec![f.clone()]);
        // The drawing as it is now comes in as it starts, and the import says so.
        let got = a.parts.last().unwrap();
        assert!((got.w - 400.0).abs() < 1e-6, "{}", got.w);
        assert_eq!(states(got)[0], "cut");
        let card = a.result_card.as_ref().expect("a result card").lines.join(" ");
        assert!(card.contains("changed") && card.contains("set them again"), "{card}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn showing_a_changed_drawing_while_a_row_is_chosen_keeps_the_change_warning() {
        let (mut a, d, f) = window_on_layered_plate("passive");
        use crate::cutpanel::Row;
        a.importui.panel.open = true;
        a.importui.panel.row = Some(Row::Mark);
        assert!(a.preview_paint(&[0], Row::LeaveOut, false));
        // The file changes, the folder is read again and the new preview shown.
        let older = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&f).unwrap().set_modified(older).unwrap();
        a.importui.refresh();
        a.importui.want_preview(&a.cfg, &a.spec, &egui::Context::default());
        // Drawing the preview with the row chosen passes through the panel
        // without painting.
        let names = crate::ct1::for_spec(&a.spec).qualities;
        for _ in 0..3 {
            a.importui.with_paint(&a.cfg, &a.spec, &names, |_, _| ());
        }
        a.import_from_window(vec![f.clone()]);
        assert_eq!(states(a.parts.last().unwrap())[0], "cut");
        let card = a.result_card.as_ref().expect("a result card").lines.join(" ");
        assert!(card.contains("changed") && card.contains("set them again"), "{card}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_dropped_drawing_with_layers_opens_the_window_with_its_panel() {
        let (mut a, d, f) = window_on_layered_plate("drop");
        a.importui.open = false;
        let bend = d.join("DEMO-BEND-PLATE.dxf");
        std::fs::write(&bend, include_str!("../../examples/drawings/DEMO-BEND-PLATE.dxf")).unwrap();
        // IM12: the one-layer drawing comes in; the layered one waits in the
        // window, picked, its panel open.
        a.import_drawings(vec![bend, f.clone()]);
        assert_eq!(a.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["DEMO-BEND-PLATE"]);
        assert!(a.importui.open && a.importui.panel.open);
        assert_eq!(a.importui.picked.iter().collect::<Vec<_>>(), [&f]);
        assert_eq!(a.importui.focus.as_ref(), Some(&f));
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
        let spec = crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 10.0, gap: 10.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() };
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
