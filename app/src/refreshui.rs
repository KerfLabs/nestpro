//! Lists and open jobs that update by themselves (JX8, LB14–LB16): the
//! window's side. While the auto-refresh setting is on, a quick check every
//! few seconds, and at once when the window comes back to the front, looks
//! whether the Library changed; what changed is read again: the Job
//! Explorer's saved jobs, the Library window, Add product's library parts
//! and the sheet stock. Once per period everything is read again. An open
//! job someone else saved reloads by itself when it has no unsaved changes
//! here, with a note saying who saved it and when; with unsaved changes the
//! programmer is asked. The rules are engine::refresh's; refreshing only
//! reads the Library, never takes its write lock and never writes to it.

use crate::engine::refresh::{self, Due, Stamp, Step, Timer};
use crate::library::{Changed, Kind, Library, Record};
use crate::libui::SaveQuestion;
use crate::theme::th;
use crate::App;
use eframe::egui;
use egui::RichText;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// The window's auto-refresh state.
#[derive(Default)]
pub struct AutoRefresh {
    pub timer: Timer,
    /// The Library's stamp at the last look that read everything that had
    /// changed (None: not looked yet, or a list couldn't be read: read again).
    pub seen: Option<Stamp>,
    /// Look at the open jobs at the next check even if nothing changed: one
    /// couldn't be read, or its question had to wait.
    pub recheck: bool,
    /// Each open Library job's line in the Library's list at the last look
    /// that read its record: a quick check reads the record again only when
    /// its line changed (the full reload reads them all).
    pub lines: HashMap<u64, crate::library::Entry>,
    /// Per Library job record: the revision of the save the programmer
    /// chose to keep their own changes over (Keep mine), so it isn't asked
    /// about again. The job's own remembered revision stays as it was, so
    /// its next save still asks (LB7).
    pub kept: HashMap<u64, u64>,
    /// Library jobs open here that were deleted on another PC, said once.
    pub gone: HashSet<u64>,
    /// The note on a job that reloaded by itself, by record: the revision it
    /// reloaded and "sam on PROG-2 saved this job at 09:20". Shown while that
    /// job is in front at that revision, until closed.
    pub notes: HashMap<u64, (u64, String)>,
    /// The window had the focus at the last frame.
    pub focused: bool,
    /// The Library couldn't be reached at the last look (LB11).
    pub down: bool,
    /// While it can't be reached: a look in the background whether it's
    /// back, so a share that's down never holds the window up.
    pub probe: Option<std::thread::JoinHandle<bool>>,
}

impl AutoRefresh {
    /// Another Library folder: what was seen of the old one is forgotten.
    pub fn forget(&mut self) {
        *self = AutoRefresh { timer: std::mem::take(&mut self.timer), focused: self.focused, ..Default::default() };
    }
}

impl App {
    /// Every frame (JX8): the timer, and the window coming back to the front.
    pub fn auto_refresh(&mut self, ctx: &egui::Context) {
        if self.shot.is_some() {
            return;
        }
        let focused = ctx.input(|i| i.focused);
        let front_again = focused && !self.autorefresh.focused;
        self.autorefresh.focused = focused;
        if self.cfg.refresh_minutes > 0 {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        self.refresh_tick(front_again);
    }

    /// One tick of the timer, by this window's clock: a quick check every
    /// few seconds, at once when the window came back to the front, and the
    /// full reload once per period (LB14). Nothing while the option is off.
    pub fn refresh_tick(&mut self, front_again: bool) {
        match self.autorefresh.timer.due(self.cfg.refresh_minutes, self.clock.now(), front_again) {
            Due::Nothing => {}
            due => self.refresh_check(due == Due::Full),
        }
    }

    /// One look at the Library (LB14): the stamp of the files its lists are
    /// read from is taken first, then whatever changed since the last look
    /// is read again (everything, for the full reload), then the open jobs
    /// someone else saved are reloaded or asked about (LB15). A save landing
    /// during the look changes the stamp after it was taken, so the next
    /// look reads again. Reads only.
    pub fn refresh_check(&mut self, full: bool) {
        let dir = self.library_dir();
        let mut full = full;
        if self.autorefresh.down {
            if !self.refresh_back(&dir) {
                return;
            }
            full = true;
        }
        let stamp = match refresh::stamp(&dir) {
            Ok(s) => s,
            Err(e) => return self.refresh_unreachable(e),
        };
        crate::shared::pause("refresh stamp taken");
        let lib = Library::at(&dir, &self.who, &self.clock);
        let seen = self.autorefresh.seen.clone();
        let changed = seen.as_ref() != Some(&stamp);
        let lists = full || seen.as_ref().is_none_or(|s| s.lists != stamp.lists);
        let jobs = full || changed || self.autorefresh.recheck;
        // The Library's list, read once for the lists and the open jobs.
        let list = if lists || jobs { lib.read_index().ok() } else { None };
        let mut ok = true;
        if self.libui.loaded && lists {
            ok &= self.refresh_lists(&lib, list.clone(), full);
        }
        if self.stockui.loaded && (full || seen.as_ref().is_none_or(|s| s.stock != stamp.stock)) {
            ok &= self.refresh_stock(&dir);
        }
        if jobs {
            self.autorefresh.recheck = !self.refresh_open_jobs(&lib, list.as_deref(), full);
        }
        self.autorefresh.seen = if ok { Some(stamp) } else { None };
    }

    /// The Library can't be reached (LB11): the lists say so in its words,
    /// once, not at every look. Open jobs stay open with their changes and
    /// their link to their record (LB13). Nothing is made in its place.
    fn refresh_unreachable(&mut self, e: String) {
        self.autorefresh.seen = None;
        if self.autorefresh.down {
            return;
        }
        self.autorefresh.down = true;
        if self.libui.loaded {
            self.libui.entries.clear();
            self.libui.products.clear();
            self.libui.status = e.clone();
            self.libui.error = Some(e.clone());
        }
        if self.stockui.loaded {
            self.stockui.stock = Default::default();
            self.stockui.unreachable = Some(e.clone());
        }
        self.status = e;
    }

    /// While the Library can't be reached: is it back? Looked at in the
    /// background, one look at a time, so a share that's down never holds
    /// the window up (LB11). True once it's back.
    fn refresh_back(&mut self, dir: &Path) -> bool {
        match self.autorefresh.probe.take() {
            Some(p) if p.is_finished() => {
                if !p.join().unwrap_or(false) {
                    return false;
                }
                self.autorefresh.down = false;
                self.status = format!("The Library on {} can be reached again.", dir.display());
                true
            }
            Some(p) => {
                self.autorefresh.probe = Some(p);
                false
            }
            None => {
                let d = dir.to_path_buf();
                self.autorefresh.probe = Some(std::thread::spawn(move || crate::library::usable(&d).is_ok()));
                false
            }
        }
    }

    /// Read the Library's list and its job and part folder trees again for
    /// the Job Explorer, the Library window (Jobs and Products) and Add
    /// product (LB14, JX10, JX18). A quick check reads only the library
    /// parts whose line changed; the full reload reads them all. Something
    /// that can't be read now (being replaced) keeps what was shown, and
    /// false says to read it again at the next look.
    fn refresh_lists(&mut self, lib: &Library, list: Option<Vec<crate::library::Entry>>, full: bool) -> bool {
        let (Some(entries), Ok(tree), Ok(part_tree)) = (list, crate::folders::load(lib, Kind::Job), crate::folders::load(lib, Kind::Product)) else { return false };
        let old: HashMap<u64, (crate::library::Entry, crate::nest::Part)> = std::mem::take(&mut self.libui.products).into_iter().map(|p| (p.0.id, p)).collect();
        let mut ok = true;
        let mut products = Vec::new();
        for e in entries.iter().filter(|e| e.kind == Kind::Product) {
            match old.get(&e.id) {
                Some(p) if !full && p.0 == *e => products.push(p.clone()),
                was => match lib.load_product(e.id) {
                    Ok(pf) => products.push((e.clone(), pf.part)),
                    Err(_) => {
                        ok = false;
                        if let Some(p) = was {
                            products.push(p.clone());
                        }
                    }
                },
            }
        }
        products.sort_by(|a, b| b.0.updated.cmp(&a.0.updated));
        self.libui.entries = entries;
        *self.libui.view(Kind::Job).tree = tree;
        *self.libui.view(Kind::Product).tree = part_tree;
        self.libui.products = products;
        if self.libui.error.take().is_some() {
            self.libui.status.clear();
        }
        // The jobs other PCs have in use, for the same lists (CU10): looked
        // at in the background, read-only.
        self.look_at_holds();
        ok
    }

    /// Read the sheet stock again for the Sheet stock window (LB14). A stock
    /// that can't be read now keeps what was shown, read again next look.
    fn refresh_stock(&mut self, dir: &Path) -> bool {
        match crate::stock::Stock::load(dir) {
            Ok(s) => {
                self.stockui.stock = s;
                self.stockui.unreachable = None;
                true
            }
            Err(_) => false,
        }
    }

    /// Look at every open Library job (LB15). One someone else saved since
    /// it was opened or last saved here reloads by itself when it has no
    /// unsaved changes, with a note; with unsaved changes the programmer is
    /// asked. A quick check reads a job's record only when its line in the
    /// Library's list (`list`, as read in this look) changed since the last
    /// look; the full reload reads every one. False: one must be looked at
    /// again at the next check.
    fn refresh_open_jobs(&mut self, lib: &Library, list: Option<&[crate::library::Entry]>, full: bool) -> bool {
        // Not while a save waits for remnant names or for an answer about a
        // job or one of its parts (LB15): the next look does it.
        if self.naming.is_some() || self.pending.is_some() || matches!(self.libui.question, Some(SaveQuestion::Job(_) | SaveQuestion::Part { .. })) {
            return false;
        }
        self.store_job();
        let mut ok = true;
        for i in 0..self.jobs.len() {
            let Some(id) = self.jobs[i].lib_id else { continue };
            let line = list.and_then(|l| l.iter().find(|e| e.kind == Kind::Job && e.id == id)).cloned();
            if !full && line.is_some() && self.autorefresh.lines.get(&id) == line.as_ref() {
                continue;
            }
            let since = lib.since(Kind::Job, Record { id, revision: self.jobs[i].lib_read.revision });
            if !matches!(since, crate::library::Since::Gone) {
                self.autorefresh.gone.remove(&id);
            }
            let unsaved = matches!(since, crate::library::Since::Saved(_)) && self.job_unsaved(i);
            let done = match refresh::step(since, unsaved, self.autorefresh.kept.get(&id).copied()) {
                Step::Nothing => true,
                Step::Later => false,
                Step::Reload(c) => self.reload_open_job(lib, i, &c),
                Step::Ask(c) => self.ask_theirs(i, c),
                Step::Gone => {
                    if self.autorefresh.gone.insert(id) {
                        self.status = format!("{} was deleted from the Library on another PC. It stays open here with its changes; Save to library saves it again.", self.jobs[i].name);
                    }
                    true
                }
            };
            match line {
                Some(l) if done => {
                    self.autorefresh.lines.insert(id, l);
                }
                _ => {
                    self.autorefresh.lines.remove(&id);
                }
            }
            ok &= done;
        }
        ok
    }

    /// Reload open job `i` as saved now, by itself (LB15): it had no unsaved
    /// changes. It holds what their save holds; the stock isn't touched
    /// (LB9). The job in front stays in front, on the same sheet.
    fn reload_open_job(&mut self, lib: &Library, i: usize, c: &Changed) -> bool {
        let Ok(j) = lib.load_job(c.id) else { return false };
        let revision = j.library.map_or(0, |r| r.revision);
        let name = if self.jobs.iter().enumerate().any(|(k, o)| k != i && o.name == j.name) { self.jobs[i].name.clone() } else { j.name.clone() };
        let place = crate::folders::load(lib, Kind::Job).map(|t| crate::libui::place_in(&t, c.id)).unwrap_or_else(|_| (self.jobs[i].lib_folder.unwrap_or(crate::folders::TOP), self.jobs[i].week.clone()));
        let file = self.jobs[i].file.clone();
        let mut job = crate::libui::job_from_library(c.id, name, j, place);
        job.file = file;
        self.jobs[i] = job;
        if i == self.cur_job {
            let (sheet, part) = (self.cur_sheet, self.sel_part);
            self.cur_job = usize::MAX;
            self.open_job_forced(i);
            // Through `select_sheet`, so the program and overlaps shown are
            // that sheet's.
            self.select_sheet(sheet);
            if part.is_some_and(|p| p < self.parts.len()) {
                self.sel_part = part;
            }
        }
        self.mark_job_saved(i);
        self.autorefresh.kept.remove(&c.id);
        let note = refresh::reload_note(c, &self.clock);
        self.status = format!("Reloaded {}: {note}.", self.jobs[i].name);
        self.autorefresh.notes.insert(c.id, (revision, note));
        true
    }

    /// Ask about open job `i`, which has unsaved changes here and which
    /// someone else saved since (LB15). One question at a time: while
    /// another is waiting, this one waits for the next look (false). The
    /// question about this job already up names their latest save.
    fn ask_theirs(&mut self, i: usize, c: Changed) -> bool {
        match &mut self.libui.question {
            None => {
                self.status = format!("{} was saved by {} since you opened it, and it has changes here that aren't saved. Reload theirs, keep yours, or save yours as a copy.", self.jobs[i].name, c.saved_by(&self.clock));
                self.libui.question = Some(SaveQuestion::Theirs(c));
                true
            }
            Some(SaveQuestion::Theirs(q)) if q.id == c.id => {
                *q = c;
                true
            }
            Some(_) => false,
        }
    }

    /// The open job a refresh question is about, brought to the front; None
    /// when it was closed or saved under another record since.
    fn theirs_job(&mut self) -> Option<Changed> {
        let Some(SaveQuestion::Theirs(c)) = self.libui.question.clone() else { return None };
        self.libui.question = None;
        self.store_job();
        let i = self.jobs.iter().position(|j| j.lib_id == Some(c.id))?;
        self.open_job(i);
        Some(c)
    }

    /// Reload theirs (LB15, as LB9): the job becomes the job as saved now;
    /// this window's changes are dropped; the stock isn't touched.
    pub fn refresh_reload_theirs(&mut self) {
        let Some(c) = self.theirs_job() else { return };
        self.autorefresh.kept.remove(&c.id);
        self.libui.question = Some(SaveQuestion::Job(c));
        self.reload_theirs();
    }

    /// Save mine as a copy (LB15, as LB9): saved as a new job under the
    /// first free name, filed next to theirs, taking its own sheets; theirs
    /// stays as they saved it.
    pub fn refresh_copy_mine(&mut self) {
        let Some(c) = self.theirs_job() else { return };
        self.libui.question = Some(SaveQuestion::Job(c));
        self.save_mine_as_copy();
    }

    /// Keep mine (LB15): the job stays as it is here, and isn't asked about
    /// again for this save of theirs. Its remembered revision stays as it
    /// was, so saving it still asks first (LB7, LB9).
    pub fn keep_mine(&mut self) {
        let Some(SaveQuestion::Theirs(c)) = self.libui.question.clone() else { return };
        self.libui.question = None;
        self.autorefresh.kept.insert(c.id, c.revision);
        let name = self.jobs.iter().find(|j| j.lib_id == Some(c.id)).map_or_else(|| c.name.clone(), |j| j.name.clone());
        self.status = format!("Kept your changes to {name}. {} saved it since you opened it, so saving it asks first.", c.saved_by(&self.clock));
    }

    /// The note on the job in front when it reloaded by itself (LB15), until
    /// it's closed or the job is saved again.
    pub fn reload_note_card(&mut self, ctx: &egui::Context) {
        let Some(id) = self.job_lib_id else { return };
        let Some((revision, note)) = self.autorefresh.notes.get(&id).cloned() else { return };
        if revision != self.lib_read.revision {
            self.autorefresh.notes.remove(&id);
            return;
        }
        let mut close = false;
        let top = 70.0 + crate::ui_parts::RIBBON_H;
        egui::Area::new(egui::Id::unique("reload_note")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, [0.0, top]).show(ctx, |ui| {
            egui::Frame::new().fill(th().accent_soft).stroke(egui::Stroke::new(1.0, th().accent_border)).corner_radius(6).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("↻ {} was reloaded", self.job_name)).strong().color(th().text_strong));
                    ui.label(RichText::new(format!("{note}.")).color(th().text));
                    ui.add_space(6.0);
                    close = ui.small_button("✖").on_hover_text("Close this note").clicked();
                });
            });
        });
        if close {
            self.autorefresh.notes.remove(&id);
        }
    }

    /// Screenshots (`--reload-note` with `--library-dir=` a new, empty
    /// folder): library job DEMO-7 (Acme) is open here, unchanged; Sam on
    /// PROG-2 saves it at 09:20, and the quick check reloads it with the
    /// note. `ask`: DEMO-7 has a change here that isn't saved, so the check
    /// asks instead. Made-up users and PCs on a clock set to 9 Nov 09:20.
    pub fn demo_reload_note(&mut self, ask: bool) {
        let dir = self.library_dir();
        if self.cfg.library_dir.is_empty() || dir == crate::library::default_dir() || crate::library::holds_library(&dir) {
            self.status = "--reload-note needs --library-dir= set to a new, empty folder".into();
            return;
        }
        let clock = crate::shared::Clock::at(1_794_216_000 - 10 * 60);
        self.clock = clock.clone();
        self.who = crate::shared::Who::new("kim", "PROG-1");
        self.cfg.refresh_minutes = 1;
        let r = (|| -> Result<(), String> {
            crate::library::create(&dir)?;
            let sam = Library::open(&dir, &crate::shared::Who::new("Sam", "PROG-2"), &clock)?;
            let mut j = self.current_jobfile();
            j.name = "DEMO-7".into();
            j.week = "Acme".into();
            j.library = None;
            j.held.clear();
            // Filed in folder Acme, named like its Open jobs folder (JX15).
            let rec = sam.save_job(&j, crate::library::SaveTo::New)?;
            self.open_library_job(rec.id);
            self.refresh_tick(false);
            clock.advance(std::time::Duration::from_secs(10 * 60));
            let mut theirs = sam.load_job(rec.id)?;
            if let Some(p) = theirs.parts.first_mut() {
                p.qty += 5;
            }
            sam.save_job(&theirs, crate::library::SaveTo::Read(theirs.library.ok_or("no record")?))?;
            if ask {
                if let Some(p) = self.parts.first_mut() {
                    p.qty += 2;
                }
            }
            self.refresh_tick(true);
            Ok(())
        })();
        if let Err(e) = r {
            self.status = format!("--reload-note: {e}");
        }
    }
}

#[cfg(test)]
mod refreshui_tests {
    //! Two PCs on one Library folder, each a window with its own who, on one
    //! clock the test moves (LB14–LB16).
    use crate::engine::refresh::QUICK_S;
    use crate::libui::libui_tests::{demo7, holds, in_stock, listed, made, new_library, remnant_line, stock_file, stock_window, window, NOV_9_0920};
    use crate::libui::SaveQuestion;
    use crate::library::{Kind, Library, SaveTo};
    use crate::shared::{Clock, Who};
    use crate::App;
    use std::path::Path;
    use std::time::Duration;

    fn later(clock: &Clock, secs: u64) {
        clock.advance(Duration::from_secs(secs));
    }

    /// The names of the jobs in the Job Explorer's Library section.
    fn saved_jobs(a: &App) -> Vec<String> {
        let mut v: Vec<String> = a.lib_jobs().into_iter().map(|e| e.name).collect();
        v.sort();
        v
    }

    /// The library parts in Add product's list.
    fn add_product_list(a: &App) -> Vec<String> {
        let mut v: Vec<String> = a.libui.products.iter().map(|(e, p)| format!("{} ×{}", e.name, p.qty)).collect();
        v.sort();
        v
    }

    /// A window on PC `pc` with auto-refresh on (period `minutes`) and its
    /// lists read, as when the Job Explorer, the Library window and the Sheet
    /// stock are shown.
    fn watching(dir: &Path, user: &str, pc: &str, clock: &Clock, minutes: u32) -> App {
        let mut a = window(dir, user, pc, clock);
        a.cfg.refresh_minutes = minutes;
        a.lib_refresh();
        a.stockui.loaded = false;
        a.stock_reload();
        a.refresh_tick(false);
        a
    }

    /// PC 1 (sam on PROG-1) saves job `name` into the Library as a new job
    /// with one part.
    fn save_new_job(dir: &Path, clock: &Clock, name: &str) -> u64 {
        let mut j = crate::jobfile::JobFile::new_job(name, crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 10.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() });
        j.parts = vec![crate::nest::Part { name: "DEMO-P1".into(), shape: crate::nest::Shape::Rectangle, w: 600.0, h: 400.0, qty: 2, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        Library::open(dir, &Who::new("sam", "PROG-1"), clock).unwrap().save_job(&j, SaveTo::New).unwrap().id
    }

    /// LB14, acceptance 1 and 4: a job PC 1 saves shows in PC 2's Job
    /// Explorer and Library window at PC 2's next quick check, a few seconds
    /// later, without Refresh; so do a library part (Add product) and a stock
    /// change (Sheet stock). Between two checks nothing moves.
    #[test]
    fn a_job_saved_on_one_pc_shows_on_the_other_within_seconds_without_refresh() {
        let dir = new_library("refresh_lists");
        let clock = Clock::at(NOV_9_0920);
        let mut pc2 = watching(&dir, "kim", "PROG-2", &clock, 10);
        assert!(saved_jobs(&pc2).is_empty());
        later(&clock, 1);
        save_new_job(&dir, &clock, "DEMO-7");
        let sam = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        let p = crate::nest::Part { name: "DEMO-P1".into(), shape: crate::nest::Shape::Rectangle, w: 600.0, h: 400.0, qty: 3, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        sam.save_product(&p, "Acme", 0, 10.0, SaveTo::New).unwrap();
        crate::stock::Stock::update(&sam, |s| s.add_sheet(crate::stock::Item::sheet("S3000", 0, 10.0, 3000.0, 1500.0, 12), true)).unwrap();
        later(&clock, QUICK_S - 2);
        pc2.refresh_tick(false);
        assert!(saved_jobs(&pc2).is_empty(), "between two checks nothing moves");
        later(&clock, 1);
        pc2.refresh_tick(false);
        assert_eq!(saved_jobs(&pc2), ["DEMO-7"]);
        assert_eq!(listed(&pc2, Kind::Job), [("DEMO-7".to_string(), "Saved by sam on PROG-1, 9 Nov 09:20".to_string())]);
        assert_eq!(add_product_list(&pc2), ["DEMO-P1 ×3"]);
        assert_eq!(pc2.stockui.stock.get("S3000").map(|i| i.qty), Some(12));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Acceptance 3: with the option off (the default), nothing reloads by
    /// itself, however long it's left; Refresh still works.
    #[test]
    fn with_the_option_off_nothing_moves_until_refresh() {
        let dir = new_library("refresh_off");
        let clock = Clock::at(NOV_9_0920);
        let mut pc2 = watching(&dir, "kim", "PROG-2", &clock, 0);
        assert_eq!(pc2.cfg.refresh_minutes, crate::settings::AppSettings::default().refresh_minutes, "off by default");
        let id = save_new_job(&dir, &clock, "DEMO-7");
        pc2.open_library_job(id);
        let sam = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        save_new_job(&dir, &clock, "DEMO-8");
        let j = sam.load_job(id).unwrap();
        sam.save_job(&crate::jobfile::JobFile { name: "DEMO-7".into(), parts: vec![], ..j.clone() }, SaveTo::Read(j.library.unwrap())).unwrap();
        for _ in 0..30 {
            later(&clock, 120);
            pc2.refresh_tick(true);
        }
        assert!(saved_jobs(&pc2).is_empty(), "the list is as it was read");
        assert!(!pc2.parts.is_empty(), "the open job is as it was opened");
        pc2.lib_refresh();
        assert_eq!(saved_jobs(&pc2), ["DEMO-7", "DEMO-8"], "Refresh reads it again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB14: a save landing during one of PC 2's checks, after it took the
    /// Library's stamp and read the list (here: while it reads its open
    /// job's record), shows at the next check; one landing between two
    /// checks shows at the next one.
    #[test]
    fn a_save_landing_during_a_check_shows_at_the_next() {
        let dir = new_library("refresh_during");
        let clock = Clock::at(NOV_9_0920);
        let id = save_new_job(&dir, &clock, "DEMO-7");
        let mut pc2 = watching(&dir, "kim", "PROG-2", &clock, 10);
        pc2.open_library_job(id);
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        save_new_job(&dir, &clock, "DEMO-9");
        let (d, c) = (dir.clone(), clock.clone());
        let mut once = true;
        crate::shared::hooks::set(move |step| {
            if step == "refresh reads a record" && std::mem::take(&mut once) {
                save_new_job(&d, &c, "DEMO-8");
            }
        });
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        crate::shared::hooks::clear();
        assert_eq!(saved_jobs(&pc2), ["DEMO-7", "DEMO-9"], "read before DEMO-8 was saved");
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert_eq!(saved_jobs(&pc2), ["DEMO-7", "DEMO-8", "DEMO-9"], "the next check reads it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB14: the window coming back to the front checks at once, without
    /// waiting for the next quick check. Periods of 1 and 1440 minutes both
    /// check every few seconds.
    #[test]
    fn coming_back_to_the_front_checks_at_once() {
        for minutes in [1, 1440] {
            let dir = new_library(&format!("refresh_front_{minutes}"));
            let clock = Clock::at(NOV_9_0920);
            let mut pc2 = watching(&dir, "kim", "PROG-2", &clock, minutes);
            save_new_job(&dir, &clock, "DEMO-7");
            pc2.refresh_tick(false);
            assert!(saved_jobs(&pc2).is_empty(), "the same second: no check due");
            pc2.refresh_tick(true);
            assert_eq!(saved_jobs(&pc2), ["DEMO-7"], "back to the front: checked at once");
            save_new_job(&dir, &clock, "DEMO-8");
            later(&clock, QUICK_S);
            pc2.refresh_tick(false);
            assert_eq!(saved_jobs(&pc2), ["DEMO-7", "DEMO-8"], "{minutes} min: a few seconds later");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// LB14, acceptance 4: the full reload once per period reads everything
    /// again, even what a quick check can't see: a library part whose record
    /// was stored while its line in the list wasn't (LB3) shows as stored at
    /// the next full reload, not before.
    #[test]
    fn the_full_reload_runs_once_per_period() {
        let dir = new_library("refresh_full");
        let clock = Clock::at(NOV_9_0920);
        let mut pc2 = watching(&dir, "kim", "PROG-2", &clock, 1);
        let sam = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        let p = crate::nest::Part { name: "DEMO-P1".into(), shape: crate::nest::Shape::Rectangle, w: 600.0, h: 400.0, qty: 3, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let rec = sam.save_product(&p, "Acme", 0, 10.0, SaveTo::New).unwrap();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert_eq!(add_product_list(&pc2), ["DEMO-P1 ×3"]);
        let mut pf = sam.load_product(rec.id).unwrap();
        pf.part.qty = 9;
        std::fs::write(sam.path(Kind::Product, rec.id), serde_json::to_string_pretty(&pf).unwrap()).unwrap();
        for _ in 0..(60 / QUICK_S - 2) {
            later(&clock, QUICK_S);
            pc2.refresh_tick(false);
            assert_eq!(add_product_list(&pc2), ["DEMO-P1 ×3"], "quick checks: the list didn't change");
        }
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert_eq!(add_product_list(&pc2), ["DEMO-P1 ×9"], "the full reload, a minute after the last");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB15, acceptance 2: PC 2 has library job DEMO-7 open and unchanged,
    /// and another job in front. PC 1 saves DEMO-7: at PC 2's next check it
    /// reloads by itself as PC 1 saved it, with nothing unsaved, holding what
    /// PC 1's save holds; the stock isn't touched. Brought to the front it
    /// shows the note; its remembered revision moved on, so PC 2's next save
    /// goes over it without asking.
    #[test]
    fn an_unchanged_open_job_reloads_by_itself_with_a_note() {
        let dir = new_library("refresh_reload");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = demo7(&dir, &clock);
        let id = pc1.job_lib_id.unwrap();
        let mut pc2 = stock_window(&dir, "kim", "PROG-2", &clock);
        pc2.cfg.refresh_minutes = 5;
        pc2.open_library_job(id);
        let demo7_at = pc2.cur_job;
        pc2.open_job(0);
        assert_ne!(pc2.cur_job, demo7_at);
        let front = pc2.job_name.clone();
        pc2.refresh_tick(false);
        later(&clock, 60);
        pc1.parts[0].qty = 25;
        pc1.run_nesting();
        remnant_line(&mut pc1);
        pc1.save_job_to_library();
        let stock = stock_file(&dir);
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert!(pc2.libui.question.is_none(), "nothing asked: {}", pc2.status);
        assert_eq!(pc2.job_name, front, "the job in front stays in front");
        assert_eq!(pc2.jobs[demo7_at].parts[0].qty, 25, "{}", pc2.status);
        assert!(pc2.status.contains("sam on PROG-1 saved this job at 09:21"), "{}", pc2.status);
        assert!(!pc2.job_unsaved(demo7_at));
        assert_eq!(stock_file(&dir), stock, "the stock isn't touched");
        pc2.open_job(demo7_at);
        assert_eq!((&pc2.held, made(&pc2)), (&pc1.held, made(&pc1)), "it holds what PC 1's save holds");
        let note = pc2.autorefresh.notes.get(&id).map(|n| n.1.clone());
        assert_eq!(note.as_deref(), Some("sam on PROG-1 saved this job at 09:21"));
        // The job in front reloads by itself too, staying in front.
        later(&clock, 60);
        pc1.parts[0].qty = 26;
        pc1.run_nesting();
        pc1.save_job_to_library();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert_eq!((pc2.cur_job, pc2.parts[0].qty), (demo7_at, 26), "{}", pc2.status);
        // Its remembered revision moved on: PC 2's save goes over it.
        later(&clock, 60);
        pc2.parts[0].qty = 30;
        pc2.run_nesting();
        pc2.save_job_to_library();
        assert!(pc2.libui.question.is_none(), "{}", pc2.status);
        assert_eq!(pc2.library().unwrap().load_job(id).unwrap().parts[0].qty, 30);
        assert!(!pc2.autorefresh.notes.contains_key(&id) || pc2.autorefresh.notes[&id].0 != pc2.lib_read.revision, "the note goes with PC 2's own save");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// PC 1 (sam on PROG-1) and PC 2 (kim on PROG-2) have library job DEMO-7
    /// (DEMO-P1 ×20) open; PC 2 nests 40 and doesn't save; PC 1 saves 25
    /// with a remnant line at 09:30. PC 2's next check asks, and drops
    /// nothing.
    struct Asked {
        dir: std::path::PathBuf,
        clock: Clock,
        pc1: App,
        pc2: App,
        id: u64,
        stock: Vec<u8>,
    }

    fn asked(tag: &str) -> Asked {
        let dir = new_library(tag);
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = demo7(&dir, &clock);
        let id = pc1.job_lib_id.unwrap();
        let mut pc2 = stock_window(&dir, "kim", "PROG-2", &clock);
        pc2.cfg.refresh_minutes = 5;
        pc2.open_library_job(id);
        pc2.refresh_tick(false);
        later(&clock, 10 * 60);
        pc1.parts[0].qty = 25;
        pc1.run_nesting();
        remnant_line(&mut pc1);
        pc1.save_job_to_library();
        pc2.parts[0].qty = 40;
        pc2.run_nesting();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        let q = pc2.libui.question.clone().expect("PC 2 is asked");
        assert!(matches!(&q, SaveQuestion::Theirs(c) if c.saved_by(&clock) == "sam on PROG-1, 9 Nov 09:30"), "{q:?}");
        assert_eq!(pc2.parts[0].qty, 40, "nothing dropped");
        assert!(pc2.job_unsaved(pc2.cur_job));
        let stock = stock_file(&dir);
        Asked { dir, clock, pc1, pc2, id, stock }
    }

    /// LB15 Reload theirs: the job becomes the job as PC 1 saved it, PC 2's
    /// changes dropped, holding what PC 1's save holds; the stock isn't
    /// touched. Asked once: further checks don't ask again.
    #[test]
    fn a_changed_open_job_asks_and_reload_theirs_takes_their_save() {
        let Asked { dir, clock, pc1, mut pc2, id, stock } = asked("refresh_ask_reload");
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert!(matches!(pc2.libui.question, Some(SaveQuestion::Theirs(_))), "still the one question");
        pc2.refresh_reload_theirs();
        assert!(pc2.libui.question.is_none(), "{}", pc2.status);
        assert_eq!((pc2.parts[0].qty, pc2.job_lib_id), (25, Some(id)));
        assert_eq!((&pc2.held, made(&pc2)), (&pc1.held, made(&pc1)));
        assert!(!pc2.job_unsaved(pc2.cur_job));
        assert_eq!(stock_file(&dir), stock);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB15 Keep mine: PC 2's changes stay, nothing is saved and it isn't
    /// asked again for that save; its remembered revision stays as it was,
    /// so its save still asks first (LB7) and takes nothing from stock
    /// (LB8). A newer save by PC 1 asks again.
    #[test]
    fn keep_mine_keeps_my_changes_and_saving_still_asks() {
        let Asked { dir, clock, mut pc1, mut pc2, id, stock } = asked("refresh_keep");
        pc2.keep_mine();
        assert!(pc2.libui.question.is_none());
        assert_eq!(pc2.parts[0].qty, 40);
        for _ in 0..3 {
            later(&clock, QUICK_S);
            pc2.refresh_tick(false);
        }
        assert!(pc2.libui.question.is_none(), "not asked again for the same save");
        pc2.save_job_to_library();
        assert!(matches!(&pc2.libui.question, Some(SaveQuestion::Job(c)) if c.id == id), "the save check asks: {}", pc2.status);
        assert_eq!(stock_file(&dir), stock, "the refused save took nothing");
        assert_eq!(pc2.library().unwrap().load_job(id).unwrap().parts[0].qty, 25);
        pc2.libui.question = None;
        later(&clock, 60);
        pc1.parts[0].qty = 26;
        pc1.run_nesting();
        pc1.save_job_to_library();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert!(matches!(&pc2.libui.question, Some(SaveQuestion::Theirs(c)) if c.saved_by(&clock) == "sam on PROG-1, 9 Nov 09:31"), "a newer save asks again: {:?}", pc2.libui.question);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB15: PC 1 saves DEMO-7 again before PC 2 answers. The question names
    /// PC 1's latest save, and Reload theirs takes it.
    #[test]
    fn a_second_save_before_the_answer_is_the_one_asked_about() {
        let Asked { dir, clock, mut pc1, mut pc2, .. } = asked("refresh_twice");
        later(&clock, 60);
        pc1.parts[0].qty = 26;
        pc1.run_nesting();
        pc1.save_job_to_library();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert!(matches!(&pc2.libui.question, Some(SaveQuestion::Theirs(c)) if c.saved_by(&clock) == "sam on PROG-1, 9 Nov 09:31"), "{:?}", pc2.libui.question);
        pc2.refresh_reload_theirs();
        assert_eq!(pc2.parts[0].qty, 26, "{}", pc2.status);
        assert!(!pc2.job_unsaved(pc2.cur_job));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB15 Save mine as a copy, as LB9: PC 2's job is saved as a new job
    /// under the first free name, filed in DEMO-7's folder, taking its own
    /// sheets from stock and storing its own remnant; PC 1's DEMO-7 stays as
    /// PC 1 saved it. Asked about a job that isn't in front, the answer
    /// brings it to the front first.
    #[test]
    fn save_mine_as_a_copy_from_the_question_takes_its_own_sheets() {
        let Asked { dir, mut pc2, id, .. } = asked("refresh_copy");
        let record = std::fs::read(pc2.library().unwrap().path(Kind::Job, id)).unwrap();
        let demo7_at = pc2.cur_job;
        pc2.open_job(0);
        remnant_line_at(&mut pc2, demo7_at);
        let (sheets, remnants) = in_stock(&dir);
        pc2.refresh_copy_mine();
        assert!(pc2.libui.question.is_none(), "{}", pc2.status);
        assert_eq!(pc2.cur_job, demo7_at, "brought to the front");
        assert_eq!((pc2.job_name.as_str(), pc2.parts[0].qty), ("DEMO-7 (1)", 40), "{}", pc2.status);
        let copy = pc2.job_lib_id.filter(|c| *c != id).unwrap_or_else(|| panic!("{}", pc2.status));
        assert_eq!(std::fs::read(pc2.library().unwrap().path(Kind::Job, id)).unwrap(), record, "theirs stays as they saved it");
        pc2.lib_refresh();
        assert_eq!(pc2.libui.tree_of(Kind::Job).folder_of(copy), pc2.libui.tree_of(Kind::Job).folder_of(id), "filed next to theirs");
        let (left, now) = in_stock(&dir);
        assert!(holds(&pc2) > 0);
        assert_eq!(left, sheets - holds(&pc2), "the copy took its own sheets");
        assert_eq!(made(&pc2).len(), 1, "{}", pc2.status);
        assert!(remnants.iter().all(|r| now.contains(r)) && now.contains(&made(&pc2)[0]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A remnant line across the last sheet of open job `i` (not in front).
    fn remnant_line_at(a: &mut App, i: usize) {
        let cur = a.cur_job;
        a.open_job(i);
        remnant_line(a);
        a.open_job(cur);
    }

    /// LB15: the job in front, reloaded by itself while PC 2 looks at its
    /// second sheet, stays on that sheet, and the program shown is that
    /// sheet's, not the first one's.
    #[test]
    fn a_job_reloaded_in_front_shows_the_program_of_the_sheet_it_was_on() {
        let dir = new_library("refresh_sheet");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = demo7(&dir, &clock);
        let id = pc1.job_lib_id.unwrap();
        pc1.parts[0].qty = 60;
        pc1.run_nesting();
        pc1.save_job_to_library();
        let mut pc2 = stock_window(&dir, "kim", "PROG-2", &clock);
        pc2.cfg.refresh_minutes = 5;
        pc2.open_library_job(id);
        assert!(pc2.result.sheets.len() >= 2, "{}", pc2.status);
        pc2.select_sheet(1);
        pc2.refresh_tick(false);
        later(&clock, 60);
        pc1.parts[0].qty = 61;
        pc1.run_nesting();
        pc1.save_job_to_library();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert_eq!((pc2.parts[0].qty, pc2.cur_sheet), (61, 1), "{}", pc2.status);
        let shown = pc2.program.clone();
        pc2.refresh();
        assert_eq!(pc2.program, shown, "the program shown is the second sheet's");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB15: no open job reloads by itself while a save waits for an answer;
    /// here PC 2's "save this part to the library" question about DEMO-7's
    /// part. The job stays as it is under the question; the first look after
    /// the answer reloads it.
    #[test]
    fn nothing_reloads_while_a_save_question_waits() {
        let dir = new_library("refresh_part_question");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = demo7(&dir, &clock);
        let id = pc1.job_lib_id.unwrap();
        pc1.save_part_to_library(0);
        let mut pc2 = stock_window(&dir, "kim", "PROG-2", &clock);
        pc2.cfg.refresh_minutes = 5;
        pc2.open_library_job(id);
        pc2.refresh_tick(false);
        pc2.save_part_to_library(0);
        assert!(matches!(pc2.libui.question, Some(SaveQuestion::Part { .. })), "{}", pc2.status);
        assert!(!pc2.job_unsaved(pc2.cur_job));
        later(&clock, 60);
        pc1.parts[0].qty = 25;
        pc1.run_nesting();
        pc1.save_job_to_library();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert_eq!(pc2.parts[0].qty, 20, "not reloaded under the question");
        assert!(matches!(pc2.libui.question, Some(SaveQuestion::Part { .. })));
        pc2.keep_theirs();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert_eq!(pc2.parts[0].qty, 25, "{}", pc2.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB16 (JOB-62): PC 1 deletes DEMO-7 while PC 2 has it open with
    /// changes. It stays open on PC 2 with its changes and its link; PC 2 is
    /// told once; Save to library saves it again. A job PC 1 moves to another
    /// folder shows there on PC 2, PC 2's open job isn't reloaded or asked
    /// about, and PC 2's save leaves it where PC 1 put it (JX10).
    #[test]
    fn a_job_deleted_or_moved_on_another_pc_stays_open_here() {
        let dir = new_library("refresh_gone");
        let clock = Clock::at(NOV_9_0920);
        let gone = save_new_job(&dir, &clock, "DEMO-7");
        let moved = save_new_job(&dir, &clock, "DEMO-8");
        let mut pc2 = watching(&dir, "kim", "PROG-2", &clock, 10);
        pc2.open_library_job(moved);
        pc2.open_library_job(gone);
        pc2.parts[0].qty += 1;
        let sam = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        sam.delete(Kind::Job, gone).unwrap();
        let elsewhere = crate::folders::change(&sam, Kind::Job, |t, _| {
            let f = t.make(crate::folders::TOP, "Elsewhere")?;
            t.put(moved, f).map(|_| f)
        })
        .unwrap();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert_eq!(saved_jobs(&pc2), ["DEMO-8"]);
        assert!(pc2.status.contains("DEMO-7 was deleted from the Library on another PC"), "{}", pc2.status);
        assert_eq!((pc2.job_name.as_str(), pc2.job_lib_id), ("DEMO-7", Some(gone)), "still open, still linked");
        assert!(pc2.job_unsaved(pc2.cur_job), "its changes kept");
        assert!(pc2.libui.question.is_none());
        pc2.status.clear();
        save_new_job(&dir, &clock, "DEMO-9");
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        assert_eq!(saved_jobs(&pc2), ["DEMO-8", "DEMO-9"]);
        assert_eq!(pc2.status, "", "told once");
        assert_eq!(pc2.libui.tree_of(Kind::Job).folder_of(moved), elsewhere, "the moved job shows in its new folder");
        let at = pc2.jobs.iter().position(|j| j.lib_id == Some(moved)).unwrap();
        assert!(!pc2.job_unsaved(at));
        pc2.save_job_to_library();
        assert!(pc2.libui.question.is_none(), "{}", pc2.status);
        assert!(pc2.library().unwrap().load_job(gone).is_ok(), "saved again: {}", pc2.status);
        pc2.open_job(at);
        pc2.parts[0].qty += 2;
        pc2.save_job_to_library();
        pc2.lib_refresh();
        assert_eq!(pc2.libui.tree_of(Kind::Job).folder_of(moved), elsewhere, "its save leaves it where PC 1 put it: {}", pc2.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// JX18 with LB14: on PC 1, a part folder is made, library part DEMO-P1
    /// is moved into it with Cut and Paste, the folder is renamed, and an
    /// empty one is made and deleted. PC 2's Library window (Products tab)
    /// and Add product's folder labels show each change at PC 2's next look,
    /// without Refresh. With the option off, nothing moves until Refresh.
    #[test]
    fn a_library_part_moved_on_one_pc_shows_in_its_new_folder_on_the_other() {
        use crate::folders::TOP;
        use crate::libui::LibItem;
        for minutes in [10, 0] {
            let dir = new_library(&format!("refresh_part_folders_{minutes}"));
            let clock = Clock::at(NOV_9_0920);
            let sam = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
            let p = crate::nest::Part { name: "DEMO-P1".into(), shape: crate::nest::Shape::Rectangle, w: 600.0, h: 400.0, qty: 3, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
            let part = sam.save_product(&p, "", 0, 10.0, SaveTo::New).unwrap().id;
            let mut pc1 = window(&dir, "sam", "PROG-1", &clock);
            pc1.lib_refresh();
            let mut pc2 = watching(&dir, "kim", "PROG-2", &clock, minutes);
            assert_eq!(pc2.library_parts_outline(), ["DEMO-P1"]);
            // A look a few seconds after each change on PC 1.
            let look = |pc2: &mut App| {
                later(&clock, QUICK_S);
                pc2.refresh_tick(false);
                if minutes == 0 {
                    pc2.lib_refresh();
                }
            };
            let brackets = pc1.lib_new_folder(Kind::Product, TOP).unwrap();
            assert!(pc1.lib_rename_folder(Kind::Product, brackets, "Brackets"), "{}", pc1.status);
            pc1.lib_cut(Kind::Product, LibItem::Part(part));
            assert!(pc1.lib_paste(Kind::Product, brackets), "{}", pc1.status);
            if minutes == 0 {
                later(&clock, QUICK_S);
                pc2.refresh_tick(true);
                assert_eq!(pc2.library_parts_outline(), ["DEMO-P1"], "option off: nothing moves by itself");
            }
            look(&mut pc2);
            assert_eq!(pc2.library_parts_outline(), ["Brackets/", "Brackets/DEMO-P1"], "{minutes} min");
            assert_eq!(pc2.libui.part_folder(part), "Brackets", "Add product's label");
            assert!(pc1.lib_rename_folder(Kind::Product, brackets, "Brackets 2026"), "{}", pc1.status);
            let old = pc1.lib_new_folder(Kind::Product, TOP).unwrap();
            look(&mut pc2);
            assert_eq!(pc2.library_parts_outline(), ["Brackets 2026/", "Brackets 2026/DEMO-P1", "New folder/"], "{minutes} min");
            assert!(pc1.lib_delete_folder(Kind::Product, old), "{}", pc1.status);
            look(&mut pc2);
            assert_eq!(pc2.library_parts_outline(), ["Brackets 2026/", "Brackets 2026/DEMO-P1"], "{minutes} min");
            assert_eq!(pc2.libui.part_folder(part), "Brackets 2026");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Let a background look for the Library finish.
    fn settle(a: &App) {
        if let Some(p) = &a.autorefresh.probe {
            while !p.is_finished() {
                std::thread::yield_now();
            }
        }
    }

    /// LB11, LB13 with LB14: the Library's folder goes (the share is down)
    /// while the option is on. PC 2's lists say so in LB11's words, once;
    /// its open job stays open with its changes and its link; nothing is made
    /// in the folder's place. When it's back, the lists come back by
    /// themselves with what PC 1 saved meanwhile.
    #[test]
    fn the_library_going_and_coming_back_while_the_option_is_on() {
        let dir = new_library("refresh_down");
        let clock = Clock::at(NOV_9_0920);
        let id = save_new_job(&dir, &clock, "DEMO-7");
        let mut pc2 = watching(&dir, "kim", "PROG-2", &clock, 1);
        pc2.open_library_job(id);
        pc2.parts[0].qty += 1;
        let away = dir.with_extension("away");
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&dir, &away).unwrap();
        later(&clock, QUICK_S);
        pc2.refresh_tick(false);
        let said = crate::library::unreachable(&dir);
        assert_eq!((pc2.libui.error.as_deref(), pc2.stockui.unreachable.as_deref(), pc2.status.as_str()), (Some(said.as_str()), Some(said.as_str()), said.as_str()));
        pc2.status.clear();
        for _ in 0..30 {
            later(&clock, QUICK_S);
            pc2.refresh_tick(false);
            settle(&pc2);
        }
        assert_eq!(pc2.status, "", "said once, not at every look");
        assert!(!dir.exists(), "nothing made in its place");
        assert_eq!((pc2.job_name.as_str(), pc2.job_lib_id), ("DEMO-7", Some(id)));
        assert!(pc2.job_unsaved(pc2.cur_job), "its changes kept");
        std::fs::rename(&away, &dir).unwrap();
        save_new_job(&dir, &clock, "DEMO-8");
        for _ in 0..3 {
            later(&clock, QUICK_S);
            pc2.refresh_tick(false);
            settle(&pc2);
        }
        assert_eq!(pc2.libui.error, None);
        assert_eq!(pc2.stockui.unreachable, None);
        assert_eq!(saved_jobs(&pc2), ["DEMO-7", "DEMO-8"]);
        assert!(pc2.job_unsaved(pc2.cur_job), "its changes still kept");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
