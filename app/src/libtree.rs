//! The Library's folder trees on screen (folders.rs), the same on every PC
//! that shares the Library: the jobs' in the Job Explorer's Library section
//! (JX10–JX15), the library parts' in the Library window's Products tab
//! (JX18–JX20). Both have the original's folder menu (JX1): New ▸ Folder,
//! Cut, Paste, Delete, Change name, Refresh.

use crate::folders::{self, Refused, Tree, TOP};
use crate::library::{auto_ref, Entry, Kind, Library};
use crate::libui::LibItem;
use crate::theme::th;
use crate::App;
use eframe::egui;

/// What a tree's menus asked for, done after drawing.
#[derive(Clone)]
pub(crate) enum TreeAct {
    /// A job: open it; a library part: add it to the open job.
    Open(u64),
    /// A library part: delete it from the Library.
    Remove(u64),
    NewFolder(u64),
    Cut(LibItem),
    Paste(u64),
    Delete(u64),
    Rename(u64),
    Typing(u64, String),
    CommitName(u64, String),
    CancelName,
    Refresh,
}

/// Draws one record's row (with its folder's path when given, as a search
/// lists it) and sets what its menu asked for.
pub(crate) type Row<'a> = dyn Fn(&mut egui::Ui, &Entry, Option<&str>, &mut Option<TreeAct>) + 'a;

/// How a tree is titled: its root line and the one quiet line while it
/// holds nothing, each with its hover.
pub(crate) struct TreeLook {
    pub title: String,
    pub tip: String,
    pub empty: String,
    pub empty_tip: String,
    /// Keeps each tree's open folders apart.
    pub salt: &'static str,
}

/// A tree as it is drawn: a copy of the view, the folders to open out this
/// time, and every folder opened out (screenshots).
struct Shown {
    kind: Kind,
    tree: Tree,
    cut: Option<LibItem>,
    edit: Option<(u64, String)>,
    reveal: Vec<u64>,
    reveal_all: bool,
}

/// What drawing one folder needs from the tree around it.
struct FolderCtx<'a> {
    shown: &'a Shown,
    act: Option<TreeAct>,
}

impl App {
    /// Read both folder trees again (with the list already read).
    pub(crate) fn lib_reload_tree(&mut self, lib: &Library) {
        for kind in [Kind::Job, Kind::Product] {
            match folders::load(lib, kind) {
                Ok(t) => *self.libui.view(kind).tree = t,
                Err(e) => {
                    *self.libui.view(kind).tree = Tree::default();
                    self.libui.status = e;
                }
            }
        }
    }

    /// The Library's saved jobs.
    pub fn lib_jobs(&self) -> Vec<Entry> {
        self.lib_records(Kind::Job)
    }

    /// The Library's saved jobs or library parts.
    pub fn lib_records(&self, kind: Kind) -> Vec<Entry> {
        self.libui.entries.iter().filter(|e| e.kind == kind).cloned().collect()
    }

    /// Change the Library's job or part folders under its write lock (JX10,
    /// JX18), then show the tree as it is now, other PCs' changes included.
    /// A refusal goes to the status line.
    fn lib_change<T>(&mut self, kind: Kind, f: impl FnOnce(&mut Tree, &[Entry]) -> Result<T, Refused>) -> Option<T> {
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => {
                self.status = e;
                return None;
            }
        };
        let r = folders::change(&lib, kind, f);
        self.libui.entries = lib.entries();
        self.lib_reload_tree(&lib);
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.status = e.say(kind, false);
                None
            }
        }
    }

    /// New ▸ Folder in `parent` (TOP: at the top) of the `kind` tree: made
    /// in the Library at once as "New folder" (JX7), its name ready to type.
    pub fn lib_new_folder(&mut self, kind: Kind, parent: u64) -> Option<u64> {
        let (id, name) = self.lib_change(kind, |t, _| {
            let n = t.new_name(parent)?;
            t.make(parent, &n).map(|id| (id, n))
        })?;
        *self.libui.view(kind).edit = Some((id, name));
        self.status = "New folder — type its name and press Enter.".into();
        Some(id)
    }

    /// Change name (JX5, JX6).
    pub fn lib_rename_folder(&mut self, kind: Kind, id: u64, name: &str) -> bool {
        let done = self.lib_change(kind, |t, _| t.rename(id, name)).is_some();
        if done {
            *self.libui.view(kind).edit = None;
            self.status = format!("Folder renamed to {}.", name.trim());
        }
        done
    }

    /// Delete a folder: only an empty one (JX13).
    pub fn lib_delete_folder(&mut self, kind: Kind, id: u64) -> bool {
        let name = self.libui.tree_of(kind).get(id).map_or_else(String::new, |f| f.name.clone());
        let done = self.lib_change(kind, |t, es| t.delete(id, es)).is_some();
        if done {
            let v = self.libui.view(kind);
            if *v.cut == Some(LibItem::Folder(id)) {
                *v.cut = None;
            }
            self.status = format!("Folder {name} deleted.");
        }
        done
    }

    /// The name of a job, part or folder in the `kind` tree, for messages.
    fn lib_item_name(&self, kind: Kind, item: LibItem) -> String {
        let record = |k: Kind, r: u64, what: &str| self.libui.entries.iter().find(|e| e.kind == k && e.id == r).map_or_else(|| format!("{what} {}", auto_ref(k, r)), |e| format!("{what} '{}'", e.name));
        match item {
            LibItem::Job(r) => record(Kind::Job, r, "Job"),
            LibItem::Part(r) => record(Kind::Product, r, "Part"),
            LibItem::Folder(f) => self.libui.tree_of(kind).get(f).map_or_else(|| "That folder".into(), |f| format!("Folder '{}'", f.name)),
        }
    }

    /// Cut: the job, part or folder waits for Paste (JX14).
    pub fn lib_cut(&mut self, kind: Kind, item: LibItem) {
        *self.libui.view(kind).cut = Some(item);
        self.status = format!("{} cut — right-click a folder (or the Library) and Paste to move it there.", self.lib_item_name(kind, item));
    }

    /// Paste what was cut in the `kind` tree into `to` (TOP: the top), JX12,
    /// JX14. A refused paste keeps it cut. Pasting it where it already is
    /// changes nothing and says so.
    pub fn lib_paste(&mut self, kind: Kind, to: u64) -> bool {
        let Some(item) = *self.libui.view(kind).cut else {
            self.status = format!("Nothing is cut — right-click a {} or folder and Cut it first.", if kind == Kind::Job { "job" } else { "part" });
            return false;
        };
        let what = self.lib_item_name(kind, item);
        // Where it was before, read under the lock with the change.
        let was = self.lib_change(kind, |t, es| match item {
            LibItem::Folder(f) => {
                let was = t.get(f).map(|f| f.parent);
                t.move_folder(f, to).map(|_| was)
            }
            LibItem::Job(r) | LibItem::Part(r) if es.iter().any(|e| e.id == r) => {
                let was = t.folder_of(r);
                t.put(r, to).map(|_| Some(was))
            }
            LibItem::Job(_) | LibItem::Part(_) => Err(Refused::Gone(what.clone())),
        });
        let Some(was) = was else { return false };
        *self.libui.view(kind).cut = None;
        if let LibItem::Job(r) = item {
            // An open copy of it now belongs in that folder (JX15).
            for j in self.jobs.iter_mut().filter(|j| j.lib_id == Some(r)) {
                j.lib_folder = Some(to);
            }
        }
        let place = self.libui.tree_of(kind).place(to);
        self.status = if was == Some(to) { format!("{what} is already in {place}: nothing moved.") } else { format!("{what} moved to {place}.") };
        true
    }

    /// Draw the `kind` tree with `rows` as its records, then do what its
    /// folder menus asked for. Returns what a record's row asked for (Open,
    /// Remove), for the caller to do.
    pub(crate) fn lib_tree(&mut self, ui: &mut egui::Ui, kind: Kind, records: &[Entry], needle: &str, look: &TreeLook, row: &Row) -> Option<TreeAct> {
        let shown = {
            let reveal_all = self.libui.reveal_all;
            let v = self.libui.view(kind);
            Shown { kind, tree: v.tree.clone(), cut: *v.cut, edit: v.edit.clone(), reveal: std::mem::take(v.reveal), reveal_all }
        };
        let act = draw_tree(ui, &shown, records, needle, look, row);
        self.lib_tree_act(kind, act)
    }

    /// Do a folder menu's choice in the `kind` tree; a record's (Open,
    /// Remove) is handed back.
    fn lib_tree_act(&mut self, kind: Kind, act: Option<TreeAct>) -> Option<TreeAct> {
        match act {
            Some(TreeAct::NewFolder(parent)) => {
                if self.lib_new_folder(kind, parent).is_some() {
                    self.libui.view(kind).reveal.push(parent);
                }
            }
            Some(TreeAct::Cut(item)) => self.lib_cut(kind, item),
            Some(TreeAct::Paste(to)) => {
                if self.lib_paste(kind, to) {
                    self.libui.view(kind).reveal.push(to);
                }
            }
            Some(TreeAct::Delete(id)) => {
                self.lib_delete_folder(kind, id);
            }
            Some(TreeAct::Rename(id)) => {
                let name = self.libui.tree_of(kind).get(id).map(|f| (id, f.name.clone()));
                *self.libui.view(kind).edit = name;
            }
            Some(TreeAct::Typing(id, t)) => *self.libui.view(kind).edit = Some((id, t)),
            Some(TreeAct::CommitName(id, t)) => {
                // Unchanged: nothing to write.
                if self.libui.tree_of(kind).get(id).is_some_and(|f| f.name == t.trim()) {
                    *self.libui.view(kind).edit = None;
                } else {
                    self.lib_rename_folder(kind, id, &t);
                }
            }
            Some(TreeAct::CancelName) => *self.libui.view(kind).edit = None,
            Some(TreeAct::Refresh) => {
                self.lib_refresh();
                self.status = "Library read again.".into();
            }
            record @ Some(TreeAct::Open(_) | TreeAct::Remove(_)) => return record,
            None => {}
        }
        None
    }

    /// The Library section of the Job Explorer: the job folder tree with the
    /// original's folder menu (JX1: New ▸ Folder, Cut, Paste, Delete, Change
    /// name, Refresh), each folder's sub-folders first, then its jobs. With a
    /// search, the matching jobs with their folders. Returns a job to open.
    pub(crate) fn library_folder_tree(&mut self, ui: &mut egui::Ui, jobs: &[Entry], needle: &str) -> Option<u64> {
        let lib_tip = format!("Saved jobs: kept in the Library ({}), the same on every PC that shares it. Click one to open it (it joins Open jobs).", self.library_dir().display());
        let look = TreeLook {
            title: format!("📚 Saved jobs (Library) ({})", jobs.len()),
            tip: format!("{lib_tip}\nRight-click: New ▸ Folder, Paste, Refresh"),
            empty: "📚 Saved jobs (Library) — none yet".into(),
            empty_tip: format!("Jobs saved in {}.\nRight-click an open job → Save to library (or Jobs → Save to library) to keep it here; right-click here for New ▸ Folder.", self.library_dir().display()),
            salt: "lib_jobs_root",
        };
        let open_ids: Vec<u64> = self.jobs.iter().filter_map(|j| j.lib_id).chain(self.job_lib_id).collect();
        let cut = self.libui.cut;
        let clock = self.clock.clone();
        let current = self.job_lib_id;
        // The jobs other PCs have in use (CU10).
        let notes: std::collections::BTreeMap<u64, String> = self.inuse.others.keys().filter_map(|&id| self.in_use_note(id).map(|n| (id, n))).collect();
        let job_row = move |ui: &mut egui::Ui, e: &Entry, path: Option<&str>, act: &mut Option<TreeAct>| {
            let open_now = open_ids.contains(&e.id);
            let mut label = format!("{}  ·  {} sh{}", e.name, e.count_b, if open_now { "  · open" } else { "" });
            if let Some(p) = path.filter(|p| !p.is_empty()) {
                label = format!("{label}  ·  {p}");
            }
            let mut text = egui::RichText::new(label);
            if cut == Some(LibItem::Job(e.id)) {
                text = egui::RichText::new(format!("✂ {}", text.text())).color(th().text_dim);
            }
            // Who last saved it, and when (LB6).
            let saved = e.saved_note(&clock);
            let saved = if saved.is_empty() { String::new() } else { format!("\n{saved}") };
            let r = ui.selectable_label(current == Some(e.id), text).on_hover_text(format!("{} — {lib_tip}{saved}\nRight-click: Open, Cut", auto_ref(Kind::Job, e.id)));
            // Another PC has it in use (CU10): said under it.
            if let Some(n) = notes.get(&e.id) {
                ui.label(egui::RichText::new(format!("🔒 {n}")).size(11.0).color(th().warn_fg));
            }
            if r.clicked() {
                *act = Some(TreeAct::Open(e.id));
            }
            r.context_menu(|ui| {
                if ui.button("Open").clicked() {
                    *act = Some(TreeAct::Open(e.id));
                    ui.close();
                }
                if ui.button("Cut").on_hover_text("Then right-click a folder (or the Library) and Paste to move the job there").clicked() {
                    *act = Some(TreeAct::Cut(LibItem::Job(e.id)));
                    ui.close();
                }
            });
        };
        match self.lib_tree(ui, Kind::Job, jobs, needle, &look, &job_row) {
            Some(TreeAct::Open(id)) => Some(id),
            _ => None,
        }
    }

    /// The Library section opened out, as paths: "Acme/", "Acme/2026-11/",
    /// "Acme/2026-11/DEMO-001" (folders first in each folder, by name).
    pub fn library_outline(&self) -> Vec<String> {
        self.libui.tree.outline(&self.lib_jobs())
    }

    /// The Library's part folders opened out, as paths: "Acme/",
    /// "Acme/DEMO-P1" (JX18).
    pub fn library_parts_outline(&self) -> Vec<String> {
        self.libui.parts.tree.outline(&self.lib_records(Kind::Product))
    }
}

/// Draw a tree: its root (right-click: New ▸ Folder, Paste, Refresh), in
/// each folder its folders by name, then its records; with a search, the
/// matching records with their folders. Returns what was asked for.
fn draw_tree(ui: &mut egui::Ui, shown: &Shown, records: &[Entry], needle: &str, look: &TreeLook, row: &Row) -> Option<TreeAct> {
    let (tree, cut, kind) = (&shown.tree, shown.cut, shown.kind);
    let mut act: Option<TreeAct> = None;
    // A folder's menu, and the tree's own (`id` TOP): JX1.
    let menu = |ui: &mut egui::Ui, id: u64, act: &mut Option<TreeAct>| {
        ui.menu_button("New", |ui| {
            if ui.button("📁 Folder").clicked() {
                *act = Some(TreeAct::NewFolder(id));
                ui.close();
            }
        });
        if id != TOP && ui.button("Cut").on_hover_text("Then right-click another folder (or the Library) and Paste to move it there").clicked() {
            *act = Some(TreeAct::Cut(LibItem::Folder(id)));
            ui.close();
        }
        let what = if kind == Kind::Job { "Cut a job or folder first" } else { "Cut a part or folder first" };
        if ui.add_enabled(cut.is_some(), egui::Button::new("Paste")).on_disabled_hover_text(what).clicked() {
            *act = Some(TreeAct::Paste(id));
            ui.close();
        }
        if id != TOP {
            if ui.button("Delete").on_hover_text("Only an empty folder can be deleted").clicked() {
                *act = Some(TreeAct::Delete(id));
                ui.close();
            }
            if ui.button("Change name").clicked() {
                *act = Some(TreeAct::Rename(id));
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Refresh").on_hover_text("Read the Library again: what other PCs saved, made or moved").clicked() {
            *act = Some(TreeAct::Refresh);
            ui.close();
        }
    };
    if records.is_empty() && tree.folders.is_empty() {
        // One quiet line until the tree has records or folders.
        ui.label(egui::RichText::new(&look.empty).color(th().text_dim)).on_hover_text(&look.empty_tip).context_menu(|ui| menu(ui, TOP, &mut act));
        return act;
    }
    let root = egui::CollapsingHeader::new(egui::RichText::new(&look.title).strong()).id_salt(look.salt).default_open(true).show(ui, |ui| {
        if !needle.is_empty() {
            // Searching: the matching records, each with its folder.
            let found: Vec<&Entry> = records.iter().filter(|e| e.name.to_lowercase().contains(needle) || tree.path(tree.folder_of(e.id)).to_lowercase().contains(needle)).collect();
            if found.is_empty() {
                let what = if kind == Kind::Job { "No saved job matches." } else { "No library part matches." };
                ui.label(egui::RichText::new(what).size(11.0).color(th().text_dim));
            }
            for e in found {
                row(ui, e, Some(&tree.path(tree.folder_of(e.id))), &mut act);
            }
            return;
        }
        fn folder(ui: &mut egui::Ui, id: u64, records: &[Entry], ctx: &mut FolderCtx, row: &Row, menu: &dyn Fn(&mut egui::Ui, u64, &mut Option<TreeAct>), salt: &str) {
            let t = &ctx.shown.tree;
            for f in t.children(id) {
                // Its name being typed (New folder / Change name).
                if let Some((eid, text)) = ctx.shown.edit.as_ref().filter(|(e, _)| *e == f.id) {
                    let mut txt = text.clone();
                    let r = ui.add(egui::TextEdit::singleline(&mut txt).desired_width(160.0));
                    r.request_focus();
                    if r.changed() {
                        ctx.act = Some(TreeAct::Typing(*eid, txt.clone()));
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        ctx.act = Some(TreeAct::CommitName(*eid, txt));
                    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        ctx.act = Some(TreeAct::CancelName);
                    }
                    continue;
                }
                let inside = t.records_within(f.id, records);
                let mut title = egui::RichText::new(format!("📁 {}  ({inside})", f.name));
                if ctx.shown.cut == Some(LibItem::Folder(f.id)) {
                    title = egui::RichText::new(format!("✂ 📁 {}  ({inside})", f.name)).color(th().text_dim);
                }
                let mut h = egui::CollapsingHeader::new(title).id_salt((salt, f.id)).default_open(false);
                if ctx.shown.reveal_all || ctx.shown.reveal.contains(&f.id) {
                    h = h.open(Some(true));
                }
                let mut a = None;
                let resp = h.show(ui, |ui| {
                    folder(ui, f.id, records, ctx, row, menu, salt);
                    for e in t.records_in(f.id, records) {
                        row(ui, e, None, &mut a);
                    }
                });
                if a.is_some() {
                    ctx.act = a;
                }
                let mut a = None;
                let held = folders::noun(ctx.shown.kind, inside);
                resp.header_response.on_hover_text(format!("{held} in it and its folders; {} folder(s) in it — right-click for New folder, Cut, Paste, Delete, Change name, Refresh", t.children(f.id).len())).context_menu(|ui| menu(ui, f.id, &mut a));
                if a.is_some() {
                    ctx.act = a;
                }
            }
        }
        let mut ctx = FolderCtx { shown, act: None };
        folder(ui, TOP, records, &mut ctx, row, &menu, look.salt);
        for e in tree.records_in(TOP, records) {
            row(ui, e, None, &mut ctx.act);
        }
        if ctx.act.is_some() {
            act = ctx.act;
        }
    });
    root.header_response.on_hover_text(&look.tip).context_menu(|ui| menu(ui, TOP, &mut act));
    act
}

#[cfg(test)]
mod libtree_tests {
    use crate::folders::TOP;
    use crate::library::Kind;
    use crate::libui::LibItem;
    use crate::shared::{Clock, Who};
    use crate::App;
    use std::path::{Path, PathBuf};

    /// 2026-11-09 09:20 UTC: a test clock shows UTC.
    const NOV_9_0920: u64 = 1_794_216_000;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestpro_libtree_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A new, empty Library in a scratch folder: made once, as in Settings
    /// (LB12).
    fn library(tag: &str) -> PathBuf {
        let dir = scratch(tag);
        crate::library::create(&dir).unwrap();
        dir
    }

    /// One programming PC: `user` at a window on PC `name`, working on the
    /// Library in `dir`, going by `clock` (LB5). Its warn switch is off
    /// (CU1): it holds no job, so another PC may move the jobs it has open;
    /// folders with jobs in use are tested in `inuseui.rs`.
    fn pc(dir: &Path, user: &str, name: &str, clock: &Clock) -> App {
        let mut a = App::new(None);
        a.cfg.in_use_warn = false;
        a.cfg.library_dir = dir.display().to_string();
        a.who = Who::new(user, name);
        a.clock = clock.clone();
        a.lib_refresh();
        a
    }

    /// A new folder in `parent`, named `name`.
    fn folder(a: &mut App, parent: u64, name: &str) -> u64 {
        let id = a.lib_new_folder(Kind::Job, parent).unwrap_or_else(|| panic!("{}", a.status));
        assert!(a.lib_rename_folder(Kind::Job, id, name), "{}", a.status);
        id
    }

    #[test]
    fn a_folder_made_renamed_nested_moved_or_deleted_on_one_pc_shows_on_another_after_a_refresh() {
        let dir = library("two_pcs");
        let clock = Clock::at(NOV_9_0920);
        let (mut pc1, mut pc2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "kim", "PROG-2", &clock));
        let acme = folder(&mut pc1, TOP, "Acme");
        let nov = folder(&mut pc1, acme, "2026-11");
        pc2.lib_refresh();
        assert_eq!(pc2.library_outline(), ["Acme/", "Acme/2026-11/"]);
        // Renamed on PC 1.
        assert!(pc1.lib_rename_folder(Kind::Job, acme, "Acme Ltd"), "{}", pc1.status);
        pc2.lib_refresh();
        assert_eq!(pc2.library_outline(), ["Acme Ltd/", "Acme Ltd/2026-11/"]);
        // Moved into another folder with Cut and Paste.
        let beta = folder(&mut pc1, TOP, "Beta");
        pc1.lib_cut(Kind::Job, LibItem::Folder(nov));
        assert!(pc1.lib_paste(Kind::Job, beta), "{}", pc1.status);
        pc2.lib_refresh();
        assert_eq!(pc2.library_outline(), ["Acme Ltd/", "Beta/", "Beta/2026-11/"]);
        // Deleted.
        assert!(pc1.lib_delete_folder(Kind::Job, acme), "{}", pc1.status);
        pc2.lib_refresh();
        assert_eq!(pc2.library_outline(), ["Beta/", "Beta/2026-11/"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_pcs_changing_folders_without_refreshing_never_lose_each_others_changes() {
        let dir = library("stale");
        let clock = Clock::at(NOV_9_0920);
        let (mut pc1, mut pc2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "kim", "PROG-2", &clock));
        // Both make a folder at the top at once, neither having seen the other's.
        let a = pc1.lib_new_folder(Kind::Job, TOP).unwrap();
        let b = pc2.lib_new_folder(Kind::Job, TOP).unwrap();
        assert_ne!(a, b);
        assert_eq!(pc2.library_outline(), ["New folder/", "New folder (1)/"]);
        assert_eq!(pc2.libui.edit.as_ref().map(|e| e.1.as_str()), Some("New folder (1)"), "the name being typed is the one it got");
        // PC 1 names its folder; PC 2, still showing the old name, makes one in it.
        assert!(pc1.lib_rename_folder(Kind::Job, a, "Acme"));
        let inside = folder(&mut pc2, a, "2026-11");
        pc1.lib_refresh();
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "New folder (1)/"]);
        // PC 1 deletes PC 2's empty folder; PC 2 hasn't refreshed and tries to
        // use it: refused, saying why and what to do.
        assert!(pc1.lib_delete_folder(Kind::Job, inside));
        assert!(!pc2.lib_rename_folder(Kind::Job, inside, "2026-12"));
        assert!(pc2.status.contains("no longer in the Library") && pc2.status.contains("Refresh"), "{}", pc2.status);
        // A refused change still shows PC 2 the folders as they are now.
        assert_eq!(pc2.library_outline(), ["Acme/", "New folder (1)/"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Save the open job `i` into the Library under `name` (in the Open jobs
    /// folder `week`); its record number.
    fn save(a: &mut App, i: usize, name: &str, week: &str) -> u64 {
        a.open_job(i);
        a.job_name = name.into();
        a.jobs[i].week = week.into();
        a.save_job_to_library();
        a.job_lib_id.unwrap_or_else(|| panic!("{}", a.status))
    }

    #[test]
    fn deleting_a_folder_that_holds_a_job_is_refused_and_an_empty_folder_is_kept() {
        let dir = library("delete");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        let acme = folder(&mut pc1, TOP, "Acme");
        folder(&mut pc1, TOP, "Empty");
        let job = save(&mut pc1, 0, "DEMO-001", "W12");
        pc1.lib_cut(Kind::Job, LibItem::Job(job));
        assert!(pc1.lib_paste(Kind::Job, acme), "{}", pc1.status);
        assert!(!pc1.lib_delete_folder(Kind::Job, acme), "a folder holding a job was deleted");
        assert!(pc1.status.contains("still holds 1 job"), "{}", pc1.status);
        // Another PC, a stale view of it: refused there too.
        let mut pc2 = pc(&dir, "kim", "PROG-2", &clock);
        assert!(!pc2.lib_delete_folder(Kind::Job, acme));
        // A restart: both folders are still there, the empty one too.
        let mut after = pc(&dir, "sam", "PROG-1", &clock);
        assert_eq!(after.library_outline(), ["Acme/", "Acme/DEMO-001", "Empty/", "W12/"]);
        // Once the job is deleted from the Library, the folder can go.
        after.library().unwrap().delete(crate::library::Kind::Job, job).unwrap();
        after.lib_refresh();
        assert!(after.lib_delete_folder(Kind::Job, acme), "{}", after.status);
        assert_eq!(after.library_outline(), ["Empty/", "W12/"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cut_and_paste_move_a_job_and_a_folder_never_goes_into_its_own_sub_folder() {
        let dir = library("cut_paste");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        let acme = folder(&mut pc1, TOP, "Acme");
        let nov = folder(&mut pc1, acme, "2026-11");
        let week = folder(&mut pc1, nov, "Week 46");
        let job = save(&mut pc1, 0, "DEMO-001", "");
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/Week 46/", "DEMO-001"]);
        // The job into a folder three levels down.
        pc1.lib_cut(Kind::Job, LibItem::Job(job));
        assert!(pc1.lib_paste(Kind::Job, week), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/Week 46/", "Acme/2026-11/Week 46/DEMO-001"]);
        // A folder into itself, or into one of its own folders: refused, still cut.
        pc1.lib_cut(Kind::Job, LibItem::Folder(acme));
        assert!(!pc1.lib_paste(Kind::Job, acme));
        assert!(!pc1.lib_paste(Kind::Job, week));
        assert!(pc1.status.contains("can't go into itself or one of its own folders"), "{}", pc1.status);
        assert_eq!(pc1.libui.cut, Some(LibItem::Folder(acme)));
        // Out to the top instead: it goes with everything in it.
        pc1.lib_cut(Kind::Job, LibItem::Folder(week));
        assert!(pc1.lib_paste(Kind::Job, TOP), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "Week 46/", "Week 46/DEMO-001"]);
        // A name taken where it goes (without case, JX5, JX6): refused.
        folder(&mut pc1, nov, "week 46");
        pc1.lib_cut(Kind::Job, LibItem::Folder(week));
        assert!(!pc1.lib_paste(Kind::Job, nov));
        assert!(pc1.status.contains("error 9"), "{}", pc1.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_job_saved_for_the_first_time_goes_into_its_folder_and_saving_again_never_moves_it() {
        let dir = library("first_save");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        let acme = folder(&mut pc1, TOP, "Acme");
        let nov = folder(&mut pc1, acme, "2026-11");
        // A new job: into the top-level folder named like its Open jobs folder, made if missing.
        let job = save(&mut pc1, 0, "DEMO-001", "W12");
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "W12/", "W12/DEMO-001"]);
        // Filed elsewhere on PC 2; saving it again on PC 1 doesn't move it back.
        let mut pc2 = pc(&dir, "kim", "PROG-2", &clock);
        pc2.lib_cut(Kind::Job, LibItem::Job(job));
        assert!(pc2.lib_paste(Kind::Job, nov), "{}", pc2.status);
        pc1.parts[0].qty += 1;
        pc1.save_job_to_library();
        assert_eq!(pc1.job_lib_id, Some(job), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-001", "W12/"]);
        // Opened from there on PC 2, it sits in an Open jobs folder of that
        // name, and a copy of it is saved next to it.
        pc2.open_library_job(job);
        assert_eq!(pc2.jobs[pc2.cur_job].week, "2026-11");
        pc2.copy_job();
        pc2.save_job_to_library();
        assert_eq!(pc2.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-001", "Acme/2026-11/DEMO-001 (1)", "W12/"]);
        // The folder it came from deleted meanwhile: a copy goes by its Open
        // jobs folder's name instead.
        pc2.copy_job();
        let copy = pc2.cur_job;
        pc2.jobs[copy].week = "W13".into();
        let w12 = pc2.libui.tree.find("W12").unwrap();
        pc2.jobs[copy].lib_folder = Some(w12);
        assert!(pc2.lib_delete_folder(Kind::Job, w12), "{}", pc2.status);
        pc2.save_job_to_library();
        assert_eq!(pc2.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-001", "Acme/2026-11/DEMO-001 (1)", "W13/", "W13/DEMO-001 (1) (1)"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_saved_before_folders_opens_with_its_folder_names_as_top_level_folders() {
        // Record files and a list as the program wrote them before folders:
        // each job carries its folder's name, and there is no folder tree.
        let dir = scratch("old_format");
        let clock = Clock::at(NOV_9_0920);
        let job = |name: &str, week: &str| {
            let spec = crate::nest::SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None };
            let mut j = crate::jobfile::JobFile::new_job(name, spec);
            j.week = week.into();
            j
        };
        let jobs = [job("DEMO-001", "W12"), job("DEMO-002", "Acme"), job("DEMO-003", "w12"), job("DEMO-004", "")];
        let mut entries = vec![];
        for (k, j) in jobs.iter().enumerate() {
            let id = k as u64 + 1;
            let p = dir.join("Jobs").join("0").join(format!("{id}.snjob"));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, serde_json::to_string_pretty(j).unwrap()).unwrap();
            entries.push(serde_json::json!({"kind": "Job", "id": id, "name": j.name, "folder": j.week, "material": 0, "thickness": 3.0, "w": 2000.0, "h": 1000.0, "count_a": 0, "count_b": 0, "updated": 1_790_000_000u64}));
        }
        std::fs::write(dir.join(crate::library::INDEX), serde_json::json!({"format": "nestpro-library", "version": 1, "entries": entries}).to_string()).unwrap();
        let before: Vec<Vec<u8>> = (1..=4).map(|id| std::fs::read(dir.join("Jobs").join("0").join(format!("{id}.snjob"))).unwrap()).collect();
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        // W12 and w12 are one folder (JX6), spelt as on the first job.
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/DEMO-002", "W12/", "W12/DEMO-001", "W12/DEMO-003", "DEMO-004"]);
        // Nothing moved: the job records are as they were.
        let after: Vec<Vec<u8>> = (1..=4).map(|id| std::fs::read(dir.join("Jobs").join("0").join(format!("{id}.snjob"))).unwrap()).collect();
        assert_eq!(after, before);
        // The folders are real ones, the same on another PC: renaming one
        // keeps its jobs.
        let w12 = pc1.libui.tree.find("W12").unwrap();
        assert!(pc1.lib_rename_folder(Kind::Job, w12, "Week 12"));
        let pc2 = pc(&dir, "kim", "PROG-2", &clock);
        assert_eq!(pc2.library_outline(), ["Acme/", "Acme/DEMO-002", "Week 12/", "Week 12/DEMO-001", "Week 12/DEMO-003", "DEMO-004"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// JX16 with the tree (JX10): a name another job holds is refused
    /// naming that job's folder as the tree shows it, after it was moved
    /// there. Cut and Paste keep a job's name, so moving never trips the
    /// check, even for jobs from before names were unique (JX17).
    #[test]
    fn a_taken_name_is_refused_naming_its_folder_in_the_tree_and_moving_never_trips_it() {
        let dir = library("taken_name");
        let clock = Clock::at(NOV_9_0920);
        let (mut pc1, mut pc2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "kim", "PROG-2", &clock));
        let job = save(&mut pc1, 0, "DEMO-001 frames", "Acme");
        let beta = folder(&mut pc1, TOP, "Beta");
        let nov = folder(&mut pc1, beta, "2026-11");
        pc1.lib_cut(Kind::Job, LibItem::Job(job));
        assert!(pc1.lib_paste(Kind::Job, nov), "{}", pc1.status);
        // PC 2 saves another job under that name, in other capitals: refused,
        // naming where the job is now, not the folder name written on it.
        pc2.lib_refresh();
        pc2.open_job(1);
        pc2.job_name = "demo-001 FRAMES".into();
        pc2.save_job_to_library();
        assert!(pc2.job_lib_id.is_none(), "saved under a taken name: {}", pc2.status);
        assert!(pc2.status.contains("'DEMO-001 frames'") && pc2.status.contains("folder Beta/2026-11") && !pc2.status.contains("Acme"), "{}", pc2.status);
        // A Library from before names were unique: another job shares the
        // name. Both move freely.
        let lib = pc1.library().unwrap();
        let mut twin = lib.load_job(job).unwrap();
        twin.name = "demo-001 FRAMES".into();
        std::fs::write(lib.path(crate::library::Kind::Job, 9), serde_json::to_string(&twin).unwrap()).unwrap();
        lib.rebuild_index().unwrap();
        pc1.lib_refresh();
        let acme = pc1.libui.tree.find("Acme").unwrap();
        pc1.lib_cut(Kind::Job, LibItem::Job(9));
        assert!(pc1.lib_paste(Kind::Job, acme), "{}", pc1.status);
        pc1.lib_cut(Kind::Job, LibItem::Job(job));
        assert!(pc1.lib_paste(Kind::Job, beta), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/demo-001 FRAMES", "Beta/", "Beta/2026-11/", "Beta/DEMO-001 frames"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB11 for folders: with the Library's folder gone (the share down),
    /// New folder, Change name, Delete and Paste are refused naming the
    /// Library's folder, as a save is, and nothing is made. Once it's back,
    /// they work.
    #[test]
    fn folder_changes_refuse_a_library_that_cant_be_reached_and_make_nothing() {
        let dir = library("unreachable");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        let acme = folder(&mut pc1, TOP, "Acme");
        let job = save(&mut pc1, 0, "DEMO-001", "");
        let away = dir.with_extension("away");
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&dir, &away).unwrap();
        let said = format!("The Library on {} can't be reached", dir.display());
        assert!(pc1.lib_new_folder(Kind::Job, TOP).is_none());
        assert!(pc1.status.contains(&said), "New folder: {}", pc1.status);
        assert!(!pc1.lib_rename_folder(Kind::Job, acme, "Acme Ltd"));
        assert!(pc1.status.contains(&said), "Change name: {}", pc1.status);
        assert!(!pc1.lib_delete_folder(Kind::Job, acme));
        assert!(pc1.status.contains(&said), "Delete: {}", pc1.status);
        pc1.lib_cut(Kind::Job, LibItem::Job(job));
        assert!(!pc1.lib_paste(Kind::Job, acme));
        assert!(pc1.status.contains(&said), "Paste: {}", pc1.status);
        assert!(!dir.exists(), "a folder change made the Library folder");
        // The share is back: the job is still cut, and Paste moves it.
        std::fs::rename(&away, &dir).unwrap();
        pc1.lib_retry();
        assert!(pc1.lib_paste(Kind::Job, acme), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/DEMO-001"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // The Library's part folders (JX18–JX20).
    // -----------------------------------------------------------------------

    /// Save part `k` of the open job to the library as `name`; its record.
    fn save_part(a: &mut App, k: usize, name: &str) -> u64 {
        a.parts[k].name = name.into();
        a.save_part_to_library(k);
        a.library().unwrap().find_product(name).unwrap_or_else(|| panic!("{}", a.status))
    }

    /// Open job `i`, in the Open jobs folder `week`.
    fn open_in(a: &mut App, i: usize, week: &str) {
        a.open_job(i);
        a.jobs[i].week = week.into();
    }

    #[test]
    fn a_part_saved_from_a_job_in_folder_acme_goes_into_the_parts_folder_acme_made_if_missing() {
        let dir = library("part_first_save");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        // A job in the Open jobs folder "Acme", not in the Library yet.
        open_in(&mut pc1, 1, "Acme");
        save_part(&mut pc1, 0, "DEMO-P1");
        assert_eq!(pc1.library_parts_outline(), ["Acme/", "Acme/DEMO-P1"]);
        assert!(pc1.status.contains("in part folder Acme"), "{}", pc1.status);
        // The folder is there now: the next part joins it, found without case.
        pc1.jobs[1].week = "ACME".into();
        save_part(&mut pc1, 1, "DEMO-P2");
        assert_eq!(pc1.library_parts_outline(), ["Acme/", "Acme/DEMO-P1", "Acme/DEMO-P2"]);
        // A job in no folder: its part goes to the top of the parts tree.
        pc1.jobs[1].week = String::new();
        save_part(&mut pc1, 2, "DEMO-P3");
        assert_eq!(pc1.library_parts_outline(), ["Acme/", "Acme/DEMO-P1", "Acme/DEMO-P2", "DEMO-P3"]);
        // Another PC sees the same tree.
        let pc2 = pc(&dir, "kim", "PROG-2", &clock);
        assert_eq!(pc2.library_parts_outline(), ["Acme/", "Acme/DEMO-P1", "Acme/DEMO-P2", "DEMO-P3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A new part folder in `parent`, named `name`.
    fn part_folder(a: &mut App, parent: u64, name: &str) -> u64 {
        let id = a.lib_new_folder(Kind::Product, parent).unwrap_or_else(|| panic!("{}", a.status));
        assert!(a.lib_rename_folder(Kind::Product, id, name), "{}", a.status);
        id
    }

    #[test]
    fn a_part_moved_with_cut_and_paste_shows_on_another_pc_after_a_refresh() {
        let dir = library("part_cut_paste");
        let clock = Clock::at(NOV_9_0920);
        let (mut pc1, mut pc2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "kim", "PROG-2", &clock));
        open_in(&mut pc1, 1, "Acme");
        let p1 = save_part(&mut pc1, 0, "DEMO-P1");
        let beta = part_folder(&mut pc1, TOP, "Beta");
        pc2.lib_refresh();
        assert_eq!(pc2.library_parts_outline(), ["Acme/", "Acme/DEMO-P1", "Beta/"]);
        let record = pc1.library().unwrap().path(Kind::Product, p1);
        let before = std::fs::read(&record).unwrap();
        // PC 1 cuts the part and pastes it into Beta.
        pc1.lib_cut(Kind::Product, LibItem::Part(p1));
        assert!(pc1.lib_paste(Kind::Product, beta), "{}", pc1.status);
        assert!(pc1.status.contains("Part 'DEMO-P1' moved to 'Beta'"), "{}", pc1.status);
        assert_eq!(pc1.library_parts_outline(), ["Acme/", "Beta/", "Beta/DEMO-P1"]);
        // PC 2 still shows what it read, until it refreshes.
        assert_eq!(pc2.library_parts_outline(), ["Acme/", "Acme/DEMO-P1", "Beta/"]);
        pc2.lib_refresh();
        assert_eq!(pc2.library_parts_outline(), ["Acme/", "Beta/", "Beta/DEMO-P1"]);
        // Only its place changed: the part's record is as it was.
        assert_eq!(std::fs::read(&record).unwrap(), before, "moving a part rewrote its record");
        // Pasted into the folder it's already in: nothing moves, and it says so.
        pc2.lib_cut(Kind::Product, LibItem::Part(p1));
        assert!(pc2.lib_paste(Kind::Product, beta), "{}", pc2.status);
        assert!(pc2.status.contains("already in 'Beta'"), "{}", pc2.status);
        assert_eq!(pc2.library_parts_outline(), ["Acme/", "Beta/", "Beta/DEMO-P1"]);
        // The parts' tree is apart from the jobs': no job folder was made.
        assert!(pc2.library_outline().is_empty(), "{:?}", pc2.library_outline());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A part folder another PC renamed, deleted or moved things out of
    /// meanwhile: what this PC does next goes by the tree as it is now.
    #[test]
    fn part_folders_another_pc_changed_meanwhile_are_refused_or_made_again() {
        let dir = library("part_stale");
        let clock = Clock::at(NOV_9_0920);
        let (mut pc1, mut pc2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "kim", "PROG-2", &clock));
        open_in(&mut pc1, 1, "Acme");
        let p1 = save_part(&mut pc1, 0, "DEMO-P1");
        let beta = part_folder(&mut pc1, TOP, "Beta");
        // PC 2 renames Acme and deletes the empty Beta; PC 1 hasn't refreshed.
        pc2.lib_refresh();
        let acme = pc2.libui.parts.tree.find("Acme").unwrap();
        assert!(pc2.lib_rename_folder(Kind::Product, acme, "Acme Ltd"), "{}", pc2.status);
        assert!(pc2.lib_delete_folder(Kind::Product, beta), "{}", pc2.status);
        // PC 1 pastes into the deleted folder: refused, saying what to do; still cut.
        pc1.lib_cut(Kind::Product, LibItem::Part(p1));
        assert!(!pc1.lib_paste(Kind::Product, beta));
        assert!(pc1.status.contains("no longer in the Library") && pc1.status.contains("Refresh"), "{}", pc1.status);
        assert_eq!(pc1.libui.parts.cut, Some(LibItem::Part(p1)));
        // A refused change still shows the tree as it is now.
        assert_eq!(pc1.library_parts_outline(), ["Acme Ltd/", "Acme Ltd/DEMO-P1"]);
        // PC 1 saves another part from its job in Acme: there is no part
        // folder Acme any more, so it is made again (the renamed one keeps
        // its name and its part).
        save_part(&mut pc1, 1, "DEMO-P2");
        assert_eq!(pc1.library_parts_outline(), ["Acme/", "Acme/DEMO-P2", "Acme Ltd/", "Acme Ltd/DEMO-P1"]);
        // A part another PC deleted from the Library meanwhile: Paste is refused.
        let p2 = pc1.library().unwrap().find_product("DEMO-P2").unwrap();
        pc1.lib_cut(Kind::Product, LibItem::Part(p2));
        pc2.library().unwrap().delete(Kind::Product, p2).unwrap();
        assert!(!pc1.lib_paste(Kind::Product, TOP));
        assert!(pc1.status.contains("Part 'DEMO-P2' is no longer in the Library"), "{}", pc1.status);
        // Its place went with it: Acme is empty and can be deleted.
        let acme = pc1.libui.parts.tree.find("Acme").unwrap();
        assert!(pc1.lib_delete_folder(Kind::Product, acme), "{}", pc1.status);
        assert_eq!(pc1.library_parts_outline(), ["Acme Ltd/", "Acme Ltd/DEMO-P1"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_a_part_folder_that_still_holds_a_part_is_refused() {
        let dir = library("part_delete");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        open_in(&mut pc1, 1, "Acme");
        let p1 = save_part(&mut pc1, 0, "DEMO-P1");
        let acme = pc1.libui.parts.tree.find("Acme").unwrap();
        let inner = part_folder(&mut pc1, acme, "Brackets");
        assert!(!pc1.lib_delete_folder(Kind::Product, acme), "a folder holding a part was deleted");
        assert!(pc1.status.contains("'Acme' still holds 1 part and 1 folder") && pc1.status.contains("Move or delete what's in it first"), "{}", pc1.status);
        // Emptied: the part cut into the sub-folder, then the sub-folder out to the top.
        pc1.lib_cut(Kind::Product, LibItem::Part(p1));
        assert!(pc1.lib_paste(Kind::Product, inner));
        assert!(!pc1.lib_delete_folder(Kind::Product, inner));
        assert!(pc1.status.contains("still holds 1 part:"), "{}", pc1.status);
        pc1.lib_cut(Kind::Product, LibItem::Folder(inner));
        assert!(!pc1.lib_paste(Kind::Product, inner), "a folder went into itself");
        assert!(pc1.lib_paste(Kind::Product, TOP), "{}", pc1.status);
        assert!(pc1.lib_delete_folder(Kind::Product, acme), "{}", pc1.status);
        assert_eq!(pc1.library_parts_outline(), ["Brackets/", "Brackets/DEMO-P1"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// JX19 with the jobs' tree: a job in the Library goes by its place in
    /// the jobs' tree, nested folders and all, as it is when the part is
    /// saved; a library job at the top has no folder.
    #[test]
    fn a_part_from_a_job_filed_several_folders_deep_goes_into_the_same_path_of_part_folders() {
        let dir = library("part_nested");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        let acme = folder(&mut pc1, TOP, "Acme");
        let nov = folder(&mut pc1, acme, "2026-11");
        let job = save(&mut pc1, 1, "DEMO-001", "W12");
        // Another PC files the job in Acme/2026-11 after this PC saved it.
        let mut pc2 = pc(&dir, "kim", "PROG-2", &clock);
        pc2.lib_cut(Kind::Job, LibItem::Job(job));
        assert!(pc2.lib_paste(Kind::Job, nov), "{}", pc2.status);
        // This PC's job still sits in Open jobs folder W12, but its part goes
        // where the job is now: Acme/2026-11, each level made.
        save_part(&mut pc1, 0, "DEMO-P1");
        assert_eq!(pc1.library_parts_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-P1"]);
        // A part folder Acme that's there already is used, whatever its capitals.
        let acme_parts = pc1.libui.parts.tree.find("Acme").unwrap();
        assert!(pc1.lib_rename_folder(Kind::Product, acme_parts, "ACME"));
        save_part(&mut pc1, 1, "DEMO-P2");
        assert_eq!(pc1.library_parts_outline(), ["ACME/", "ACME/2026-11/", "ACME/2026-11/DEMO-P1", "ACME/2026-11/DEMO-P2"]);
        // The job moved to the top of the Library: no folder, so the top.
        pc1.lib_cut(Kind::Job, LibItem::Job(job));
        assert!(pc1.lib_paste(Kind::Job, TOP));
        save_part(&mut pc1, 2, "DEMO-P3");
        assert_eq!(pc1.library_parts_outline(), ["ACME/", "ACME/2026-11/", "ACME/2026-11/DEMO-P1", "ACME/2026-11/DEMO-P2", "DEMO-P3"]);
        // A copy not yet in the Library goes by the folder of the job it was
        // copied from (JX15): opened from Acme/2026-11, copied, its part saved.
        pc1.lib_cut(Kind::Job, LibItem::Job(job));
        assert!(pc1.lib_paste(Kind::Job, nov));
        let mut pc3 = pc(&dir, "lee", "PROG-3", &clock);
        pc3.open_library_job(job);
        pc3.copy_job();
        let k = pc3.parts.len() - 1;
        save_part(&mut pc3, k, "DEMO-P4");
        assert!(pc3.library_parts_outline().contains(&"ACME/2026-11/DEMO-P4".to_string()), "{:?}", pc3.library_parts_outline());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// PK1: a part is found by its name, so saving a part whose name another
    /// library part already has would go over that one: it asks first (LB7,
    /// LB9), and Overwrite saves over it in its own folder; it never moves.
    /// Add open job's products leaves it where it is too.
    #[test]
    fn a_part_saved_under_the_name_of_a_part_in_another_folder_stays_in_that_folder() {
        let dir = library("part_same_name");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        open_in(&mut pc1, 1, "Beta");
        let p1 = save_part(&mut pc1, 0, "DEMO-P1");
        // The same name from a job in Acme, not added from the Library: asked.
        open_in(&mut pc1, 2, "Acme");
        assert_eq!(save_part(&mut pc1, 0, "DEMO-P1"), p1);
        assert!(matches!(pc1.libui.question, Some(crate::libui::SaveQuestion::Part { added: false, .. })), "{}", pc1.status);
        pc1.overwrite_part();
        assert!(pc1.status.contains("Updated DEMO-P1") && pc1.status.contains("where it was: in part folder Beta"), "{}", pc1.status);
        assert_eq!(pc1.library_parts_outline(), ["Beta/", "Beta/DEMO-P1"]);
        // Add open job's products: the one already there stays, the others
        // are filed in Acme.
        pc1.parts[1].name = "DEMO-P2".into();
        pc1.parts[2].name = "DEMO-P3".into();
        pc1.add_products_to_library();
        assert!(pc1.status.contains("2 product(s) added in part folder Acme, 1 already there"), "{}", pc1.status);
        assert_eq!(pc1.library_parts_outline(), ["Acme/", "Acme/DEMO-P2", "Acme/DEMO-P3", "Beta/", "Beta/DEMO-P1"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_saved_before_part_folders_opens_with_its_parts_folder_names_as_top_level_folders() {
        // Part records and a list as the program wrote them before part
        // folders: each part carries its folder's name; the job folders'
        // tree is there (as the job folders made it), the parts' isn't.
        let dir = scratch("old_parts");
        let clock = Clock::at(NOV_9_0920);
        let part = |name: &str| crate::nest::Part { name: name.into(), shape: crate::nest::Shape::Rectangle, w: 100.0, h: 50.0, qty: 2, rotate: true, color: 1, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let old = [("DEMO-P1", "Acme"), ("DEMO-P2", "W12"), ("DEMO-P3", "acme"), ("DEMO-P4", ""), ("DEMO-P5", "ACME ")];
        let mut entries = vec![];
        let file = |id: u64| dir.join("Products").join("0").join(format!("{id}.json"));
        for (k, (name, folder)) in old.iter().enumerate() {
            let id = k as u64 + 1;
            std::fs::create_dir_all(file(id).parent().unwrap()).unwrap();
            std::fs::write(file(id), serde_json::json!({"format": "nestpro-library", "version": 1, "part": part(name), "folder": folder, "material": 0, "thickness": 3.0}).to_string()).unwrap();
            entries.push(serde_json::json!({"kind": "Product", "id": id, "name": name, "folder": folder, "material": 0, "thickness": 3.0, "w": 100.0, "h": 50.0, "count_a": 2, "count_b": 0, "updated": 1_790_000_000u64}));
        }
        std::fs::write(dir.join(crate::library::INDEX), serde_json::json!({"format": "nestpro-library", "version": 1, "entries": entries}).to_string()).unwrap();
        std::fs::write(dir.join(crate::folders::file_name(Kind::Job)), serde_json::to_string(&crate::folders::Tree::default()).unwrap()).unwrap();
        let before: Vec<Vec<u8>> = (1..=5).map(|id| std::fs::read(file(id)).unwrap()).collect();
        let index = std::fs::read(dir.join(crate::library::INDEX)).unwrap();
        // Two PCs open it at the same moment: one tree is made.
        let start = std::sync::Barrier::new(2);
        std::thread::scope(|s| {
            for (user, name) in [("sam", "PROG-1"), ("kim", "PROG-2")] {
                let (dir, clock, start) = (&dir, &clock, &start);
                s.spawn(move || {
                    start.wait();
                    crate::library::Library::open(dir, &Who::new(user, name), clock).unwrap();
                });
            }
        });
        let (mut pc1, mut pc2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "kim", "PROG-2", &clock));
        // Acme, acme and "ACME " are one folder, spelt as on the first part.
        let want = ["Acme/", "Acme/DEMO-P1", "Acme/DEMO-P3", "Acme/DEMO-P5", "W12/", "W12/DEMO-P2", "DEMO-P4"];
        assert_eq!(pc1.library_parts_outline(), want);
        assert_eq!(pc2.library_parts_outline(), want);
        assert_eq!(pc1.libui.parts.tree.folders.len(), 2, "two PCs opening at once made two trees' worth of folders");
        assert!(dir.join(crate::folders::file_name(Kind::Product)).is_file(), "the part folders weren't made on first open");
        // Nothing moved: the part records and the list are as they were.
        assert_eq!((1..=5).map(|id| std::fs::read(file(id)).unwrap()).collect::<Vec<_>>(), before);
        assert_eq!(std::fs::read(dir.join(crate::library::INDEX)).unwrap(), index);
        // They are real folders, the same on another PC: renaming one keeps its parts.
        let w12 = pc1.libui.parts.tree.find("W12").unwrap();
        assert!(pc1.lib_rename_folder(Kind::Product, w12, "Week 12"));
        pc2.lib_refresh();
        assert_eq!(pc2.library_parts_outline(), ["Acme/", "Acme/DEMO-P1", "Acme/DEMO-P3", "Acme/DEMO-P5", "Week 12/", "Week 12/DEMO-P2", "DEMO-P4"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB11 for part folders: with the Library's folder gone, New folder,
    /// Change name, Delete and Paste in the parts' tree are refused naming
    /// the folder, and nothing is made; Save to library of a part too.
    #[test]
    fn part_folder_changes_refuse_a_library_that_cant_be_reached_and_make_nothing() {
        let dir = library("part_unreachable");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        open_in(&mut pc1, 1, "Acme");
        let p1 = save_part(&mut pc1, 0, "DEMO-P1");
        let acme = pc1.libui.parts.tree.find("Acme").unwrap();
        let away = dir.with_extension("away");
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&dir, &away).unwrap();
        let said = format!("The Library on {} can't be reached", dir.display());
        assert!(pc1.lib_new_folder(Kind::Product, TOP).is_none());
        assert!(pc1.status.contains(&said), "New folder: {}", pc1.status);
        assert!(!pc1.lib_rename_folder(Kind::Product, acme, "Acme Ltd"));
        assert!(pc1.status.contains(&said), "Change name: {}", pc1.status);
        assert!(!pc1.lib_delete_folder(Kind::Product, acme));
        assert!(pc1.status.contains(&said), "Delete: {}", pc1.status);
        pc1.lib_cut(Kind::Product, LibItem::Part(p1));
        assert!(!pc1.lib_paste(Kind::Product, TOP));
        assert!(pc1.status.contains(&said), "Paste: {}", pc1.status);
        pc1.parts[1].name = "DEMO-P2".into();
        pc1.save_part_to_library(1);
        assert!(pc1.status.contains(&said), "Save to library: {}", pc1.status);
        assert!(!dir.exists(), "a part folder change made the Library folder");
        // Back: the part is still cut, and Paste moves it.
        std::fs::rename(&away, &dir).unwrap();
        pc1.lib_retry();
        assert!(pc1.lib_paste(Kind::Product, TOP), "{}", pc1.status);
        assert_eq!(pc1.library_parts_outline(), ["Acme/", "DEMO-P1"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The text a few frames of `f` leave on screen.
    fn shown(ctx: &eframe::egui::Context, mut f: impl FnMut(&eframe::egui::Context)) -> String {
        let mut text = String::new();
        for _ in 0..3 {
            let mut out = ctx.run_pass(eframe::egui::RawInput::default(), |c| f(c));
            out.textures_delta.clear();
            text = crate::ui_parts::shown_text(&out);
        }
        text
    }

    /// JX20: the Library window's Products tab shows the part folders, a
    /// search there lists the matching parts with their folders, and Add
    /// product's library list shows each part's folder and finds parts by it.
    #[test]
    fn the_library_window_shows_the_part_folders_and_add_product_finds_parts_by_their_folder() {
        let dir = library("part_window");
        let clock = Clock::at(NOV_9_0920);
        let mut a = pc(&dir, "sam", "PROG-1", &clock);
        open_in(&mut a, 1, "Acme");
        save_part(&mut a, 0, "DEMO-P1");
        a.jobs[1].week = "Beta".into();
        save_part(&mut a, 1, "DEMO-P2");
        let beta = a.libui.parts.tree.find("Beta").unwrap();
        part_folder(&mut a, beta, "2026-11");
        a.libui.open = true;
        a.libui.tab = 1;
        a.libui.reveal_all = true;
        let ctx = eframe::egui::Context::default();
        let text = shown(&ctx, |c| a.library_window(c));
        for want in ["📦 Library parts (2)", "📁 Acme  (1)", "📁 Beta  (1)", "📁 2026-11  (0)", "DEMO-P1", "DEMO-P2", "Saved by sam on PROG-1, 9 Nov 09:20"] {
            assert!(text.contains(want), "{want:?} isn't shown:\n{text}");
        }
        // A search: the matching parts, each with its folder.
        a.libui.search = "beta".into();
        let text = shown(&ctx, |c| a.library_window(c));
        assert!(text.contains("DEMO-P2") && !text.contains("DEMO-P1") && !text.contains("📁"), "{text}");
        // Add product's library list: each part with its folder, found by it.
        a.libui.part_all = true;
        a.libui.part_search = "acme".into();
        let mut text = String::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |ui| crate::ui_parts::library_parts_section(&mut a, ui));
            out.textures_delta.clear();
            text = crate::ui_parts::shown_text(&out);
        }
        assert!(text.contains("DEMO-P1") && text.contains("Acme") && !text.contains("DEMO-P2"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB9 with JX19: PROG-1 saved DEMO-P1 from its job in folder Acme;
    /// PROG-2 added it to its own job in Acme; PROG-1 then moved it into
    /// part folder Beta and saved it again. PROG-2's save of its copy asks.
    /// Returns (PROG-1, PROG-2, PROG-2's part, DEMO-P1's record).
    fn part_asked(dir: &Path, clock: &Clock) -> (App, App, usize, u64) {
        let mut prog1 = pc(dir, "sam", "PROG-1", clock);
        open_in(&mut prog1, 1, "Acme");
        let id = save_part(&mut prog1, 0, "DEMO-P1");
        let mut prog2 = pc(dir, "kim", "PROG-2", clock);
        open_in(&mut prog2, 2, "Acme");
        prog2.add_library_product(id);
        let mine = prog2.parts.len() - 1;
        let beta = part_folder(&mut prog1, TOP, "Beta");
        prog1.lib_cut(Kind::Product, LibItem::Part(id));
        assert!(prog1.lib_paste(Kind::Product, beta), "{}", prog1.status);
        clock.advance(std::time::Duration::from_secs(10 * 60));
        prog1.parts[0].qty += 1;
        prog1.save_part_to_library(0);
        assert!(prog1.libui.question.is_none(), "{}", prog1.status);
        clock.advance(std::time::Duration::from_secs(5 * 60));
        prog2.parts[mine].qty += 5;
        prog2.lib_refresh();
        prog2.save_part_to_library(mine);
        assert!(matches!(prog2.libui.question, Some(crate::libui::SaveQuestion::Part { added: true, .. })), "{}", prog2.status);
        assert_eq!(prog2.library_parts_outline(), ["Acme/", "Beta/", "Beta/DEMO-P1"]);
        (prog1, prog2, mine, id)
    }

    /// Who last saved library part `id`, and its quantity.
    fn part_saved(a: &App, id: u64) -> (String, u32) {
        let pf = a.library().unwrap().load_product(id).unwrap();
        (pf.saved.map(|s| s.by.label()).unwrap_or_default(), pf.part.qty)
    }

    #[test]
    fn keep_theirs_moves_no_part_and_files_nothing() {
        let dir = library("answer_keep");
        let clock = Clock::at(NOV_9_0920);
        let (_prog1, mut prog2, _, id) = part_asked(&dir, &clock);
        let theirs = part_saved(&prog2, id);
        prog2.keep_theirs();
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        prog2.lib_refresh();
        assert_eq!(prog2.library_parts_outline(), ["Acme/", "Beta/", "Beta/DEMO-P1"]);
        assert_eq!(part_saved(&prog2, id), theirs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_mine_under_a_new_name_files_the_copy_in_the_jobs_part_folder() {
        let dir = library("answer_new_name");
        let clock = Clock::at(NOV_9_0920);
        let (_prog1, mut prog2, mine, id) = part_asked(&dir, &clock);
        let theirs = part_saved(&prog2, id);
        // Part folder Acme went meanwhile: the copy makes it again.
        let acme = prog2.libui.parts.tree.find("Acme").unwrap();
        assert!(prog2.lib_delete_folder(Kind::Product, acme), "{}", prog2.status);
        prog2.save_part_under_new_name();
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert_eq!(prog2.parts[mine].name, "DEMO-P1 (1)");
        assert!(prog2.status.contains("Saved DEMO-P1 (1)") && prog2.status.contains("in part folder Acme"), "{}", prog2.status);
        assert_eq!(prog2.library_parts_outline(), ["Acme/", "Acme/DEMO-P1 (1)", "Beta/", "Beta/DEMO-P1"]);
        assert_eq!(part_saved(&prog2, id), theirs, "theirs changed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrite_saves_over_the_part_where_another_pc_put_it() {
        let dir = library("answer_overwrite");
        let clock = Clock::at(NOV_9_0920);
        let (_prog1, mut prog2, mine, id) = part_asked(&dir, &clock);
        let record = prog2.library().unwrap().path(Kind::Product, id);
        prog2.overwrite_part();
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert!(prog2.status.contains("Updated DEMO-P1") && prog2.status.contains("where it was: in part folder Beta"), "{}", prog2.status);
        assert_eq!(prog2.library_parts_outline(), ["Acme/", "Beta/", "Beta/DEMO-P1"]);
        assert_eq!(part_saved(&prog2, id), ("kim on PROG-2".to_string(), prog2.parts[mine].qty));
        assert!(record.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
