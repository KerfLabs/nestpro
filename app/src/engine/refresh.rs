//! Lists and open jobs that update by themselves (JX8, JX9, LB14–LB16).
//! While the auto-refresh setting is on, the window looks every few seconds
//! whether the Library changed (the stamp of the files its lists are read
//! from), and at once when it comes back to the front; the period in whole
//! minutes sets a full reload, as in the original. These are the rules,
//! given the clock; the timer's ticks and the window's focus are the
//! window's (refreshui.rs). Refreshing only reads the Library.

use crate::library::{Changed, Since};
use crate::shared::Clock;

/// How often the quick check looks, seconds (LB14: "every few seconds").
pub const QUICK_S: u64 = 5;
/// The longest period, minutes (JX9: a day).
pub const PERIOD_MAX: u32 = 1440;

/// The setting as kept (JX8, JX9): 0 = off, else whole minutes, 1 to 1440.
/// Below 1 is off; above 1440 is 1440.
pub fn period(minutes: i64) -> u32 {
    if minutes < 1 { 0 } else { minutes.min(PERIOD_MAX as i64) as u32 }
}

/// What a tick of the window's timer asks for (LB14).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Due {
    Nothing,
    /// The quick check: reload only what changed.
    Quick,
    /// The full reload, once per period.
    Full,
}

/// The auto-refresh timer, going by the window's clock (seconds since 1970).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timer {
    quick: u64,
    full: u64,
    /// The period it's timing (0: off).
    minutes: u32,
}

impl Timer {
    /// What is due at `now` with the setting `minutes` (0 = off), and
    /// whether the window just came back to the front.
    pub fn due(&mut self, minutes: u32, now: u64, front_again: bool) -> Due {
        let minutes = period(minutes as i64);
        if minutes == 0 {
            *self = Timer::default();
            return Due::Nothing;
        }
        // Just turned on, a new period, or the clock was put back: start
        // timing from now, with a look at once.
        if self.minutes != minutes || now < self.quick || now < self.full {
            *self = Timer { quick: now, full: now, minutes };
            return Due::Quick;
        }
        if now - self.full >= minutes as u64 * 60 {
            (self.quick, self.full) = (now, now);
            return Due::Full;
        }
        if front_again || now - self.quick >= QUICK_S {
            self.quick = now;
            return Due::Quick;
        }
        Due::Nothing
    }
}

/// When a file the lists are read from last changed, and its size; None
/// when it isn't there.
type FileStamp = Option<(std::time::SystemTime, u64)>;

/// The Library's last change, as cheap to take as a look at a few files'
/// dates (LB14): its list of records and its job and part folders (the Job
/// Explorer, the Library window, Add product), and its sheet stock. Any save
/// or change by any PC gives a different stamp.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    pub lists: [FileStamp; 3],
    pub stock: FileStamp,
}

/// The Library's stamp now, or LB11's words when it can't be reached or
/// holds no Library. Reads only: no lock is taken and nothing is made.
pub fn stamp(dir: &std::path::Path) -> Result<Stamp, String> {
    crate::library::usable(dir)?;
    let file = |p: std::path::PathBuf| std::fs::metadata(p).ok().and_then(|m| Some((m.modified().ok()?, m.len())));
    let folders = |k| dir.join(crate::folders::file_name(k));
    Ok(Stamp {
        lists: [file(dir.join(crate::library::INDEX)), file(folders(crate::library::Kind::Job)), file(folders(crate::library::Kind::Product))],
        stock: file(crate::stock::Stock::file(dir)),
    })
}

/// What a check does about one open Library job (LB15).
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Nothing: as read, or the programmer already kept theirs over this save.
    Nothing,
    /// No unsaved changes here: reload it as saved now, with a note.
    Reload(Changed),
    /// Unsaved changes here: ask (Reload theirs, Keep mine, Save mine as a copy).
    Ask(Changed),
    /// Deleted on another PC: it stays open here with its changes; say so once.
    Gone,
    /// It couldn't be read just now: look again at the next check.
    Later,
}

/// What to do about an open job whose record is `now`, with or without
/// unsaved changes here; `kept`: the revision of a save the programmer
/// already chose to keep their own changes over (Keep mine).
pub fn step(now: Since, unsaved: bool, kept: Option<u64>) -> Step {
    match now {
        Since::Same => Step::Nothing,
        Since::Saved(c) if !unsaved => Step::Reload(c),
        Since::Saved(c) if kept == Some(c.revision) => Step::Nothing,
        Since::Saved(c) => Step::Ask(c),
        Since::Gone => Step::Gone,
        Since::Unreadable => Step::Later,
    }
}

/// The note on a job that reloaded by itself (LB15): "Sam on PROG-2 saved
/// this job at 09:20" (on another day, "on 8 Nov 09:20").
pub fn reload_note(c: &Changed, clock: &Clock) -> String {
    format!("{} saved this job {}", c.by.as_ref().map_or_else(|| "Another PC".to_string(), |w| w.label()), clock.when(c.at))
}

#[cfg(test)]
mod refresh_tests {
    use super::*;

    /// JX8, JX9: off is 0; whole minutes 1 to 1440; a value typed outside
    /// them is off below 1, a day above 1440.
    #[test]
    fn the_period_is_whole_minutes_from_1_to_1440_and_below_1_is_off() {
        assert_eq!([period(0), period(1), period(1440)], [0, 1, 1440]);
        assert_eq!([period(-5), period(1441), period(100_000)], [0, 1440, 1440]);
    }

    /// The times each tick of `t` from `from` to `to` seconds asked for
    /// something.
    fn ticks(t: &mut Timer, minutes: u32, from: u64, to: u64) -> Vec<(u64, Due)> {
        (from..=to).map(|s| (s, t.due(minutes, s, false))).filter(|(_, d)| *d != Due::Nothing).collect()
    }

    /// LB14: off, nothing is ever due; on, a quick check at once, then every
    /// few seconds, and the full reload once per period.
    #[test]
    fn with_the_option_on_a_quick_check_runs_every_few_seconds_and_a_full_reload_once_a_period() {
        let mut t = Timer::default();
        assert!(ticks(&mut t, 0, 1000, 5000).is_empty(), "off: nothing");
        let mut t = Timer::default();
        let got = ticks(&mut t, 1, 1000, 1125);
        let quick: Vec<u64> = got.iter().filter(|(_, d)| *d == Due::Quick).map(|(s, _)| *s).collect();
        let full: Vec<u64> = got.iter().filter(|(_, d)| *d == Due::Full).map(|(s, _)| *s).collect();
        assert_eq!(quick.first(), Some(&1000), "a quick check as soon as it's on");
        assert!(got.windows(2).all(|w| w[1].0 - w[0].0 == QUICK_S), "a look every {QUICK_S} s: {got:?}");
        assert_eq!(full, [1060, 1120], "1 minute: a full reload each minute");
        // 1440 minutes: a full reload once a day, quick checks between.
        let mut t = Timer::default();
        let day = 1440 * 60;
        let got = ticks(&mut t, 1440, 0, 2 * day);
        let full: Vec<u64> = got.iter().filter(|(_, d)| *d == Due::Full).map(|(s, _)| *s).collect();
        assert_eq!(full, [day, 2 * day]);
        assert!(got.iter().filter(|(_, d)| *d == Due::Quick).count() as u64 > 2 * day / QUICK_S - 3);
    }

    fn saved(revision: u64) -> Changed {
        Changed { kind: crate::library::Kind::Job, id: 7, name: "DEMO-7".into(), by: Some(crate::shared::Who::new("Sam", "PROG-2")), at: 1_794_216_000, revision }
    }

    /// LB15: an open job someone else saved reloads by itself when it has no
    /// unsaved changes here, and asks when it has; "Keep mine" isn't asked
    /// again for the same save, but is for a newer one. A deleted record is
    /// said, one that can't be read now is looked at again.
    #[test]
    fn an_open_job_reloads_when_unchanged_and_asks_when_changed() {
        assert_eq!(step(Since::Same, true, None), Step::Nothing);
        assert_eq!(step(Since::Saved(saved(4)), false, None), Step::Reload(saved(4)));
        assert_eq!(step(Since::Saved(saved(4)), true, None), Step::Ask(saved(4)));
        assert_eq!(step(Since::Saved(saved(4)), true, Some(4)), Step::Nothing, "kept mine over this save");
        assert_eq!(step(Since::Saved(saved(5)), true, Some(4)), Step::Ask(saved(5)), "a newer save asks again");
        assert_eq!(step(Since::Saved(saved(4)), false, Some(4)), Step::Reload(saved(4)), "nothing of mine left to keep");
        assert_eq!(step(Since::Gone, true, None), Step::Gone);
        assert_eq!(step(Since::Unreadable, false, None), Step::Later);
    }

    /// LB15: the note names who saved the job and when, by this PC's clock.
    #[test]
    fn the_note_says_who_saved_the_job_and_when() {
        let clock = Clock::at(1_794_216_000 + 60);
        assert_eq!(reload_note(&saved(2), &clock), "Sam on PROG-2 saved this job at 09:20");
        let next_day = Clock::at(1_794_216_000 + 86_400);
        assert_eq!(reload_note(&saved(2), &next_day), "Sam on PROG-2 saved this job on 9 Nov 09:20");
        let nobody = Changed { by: None, ..saved(2) };
        assert_eq!(reload_note(&nobody, &clock), "Another PC saved this job at 09:20");
    }

    /// LB14: the stamp changes with any change to the files the lists are
    /// read from; a Library that can't be reached says so in LB11's words,
    /// and nothing is made there.
    #[test]
    fn the_stamp_follows_the_librarys_list_folders_and_stock() {
        let dir = std::env::temp_dir().join(format!("nestpro_stamp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let gone = stamp(&dir).unwrap_err();
        assert_eq!(gone, crate::library::unreachable(&dir));
        assert!(!dir.exists(), "nothing made");
        crate::library::create(&dir).unwrap();
        let a = stamp(&dir).unwrap();
        assert_eq!(stamp(&dir).unwrap(), a, "nothing changed");
        let clock = Clock::at(1_794_216_000);
        let lib = crate::library::Library::open(&dir, &crate::shared::Who::new("sam", "PROG-1"), &clock).unwrap();
        let b = stamp(&dir).unwrap();
        let j = crate::jobfile::JobFile::new_job("DEMO-7", crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 10.0, level: 3, machine: Default::default(), kerf: Default::default() });
        lib.save_job(&j, crate::library::SaveTo::New).unwrap();
        let c = stamp(&dir).unwrap();
        assert!(c.lists != b.lists && c.stock == b.stock, "a save changes the lists' stamp");
        crate::stock::Stock::update(&lib, |s| s.add_sheet(crate::stock::Item::sheet("S3000", 0, 10.0, 3000.0, 1500.0, 12), true)).unwrap();
        let d = stamp(&dir).unwrap();
        assert!(d.lists == c.lists && d.stock != c.stock, "a stock change changes the stock's stamp");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB14: the window coming back to the front checks at once; turning the
    /// option off stops the timer, and a new period counts from when it's
    /// set.
    #[test]
    fn coming_back_to_the_front_checks_at_once() {
        let mut t = Timer::default();
        assert_eq!(t.due(10, 1000, false), Due::Quick);
        assert_eq!(t.due(10, 1002, false), Due::Nothing);
        assert_eq!(t.due(10, 1002, true), Due::Quick, "back to the front");
        assert_eq!(t.due(10, 1003, false), Due::Nothing);
        assert_eq!(t.due(0, 1004, true), Due::Nothing, "off: not even at the front");
        assert_eq!(t.due(2, 1005, false), Due::Quick, "on again");
        assert_eq!(t.due(2, 1005 + 119, false), Due::Quick);
        assert_eq!(t.due(2, 1005 + 120, false), Due::Full);
        // A clock put back (the PC's time corrected) doesn't stop the checks.
        assert_eq!(t.due(2, 500, false), Due::Quick);
        assert_eq!(t.due(2, 505, false), Due::Quick);
    }
}
