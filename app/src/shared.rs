//! Writing to a Library that several PCs share (ADR 0003: the Library is a
//! folder on the shop's share). Every change to a shared file in the Library
//! (job and library part records, the list of records, the sheet stock) is
//! made under the Library's write lock (LB1); a lock left by a PC that
//! crashed is taken over after a while, by exactly one PC (LB2); and every
//! file is written through a temporary file of this writer's own and renamed
//! into place (LB3). Reading never waits for the lock: a file is always whole.
//!
//! How the lock holds with nothing but files on a share (LB2):
//! - `library.lock` is made with create-new, so only one writer gets it, and
//!   only its holder ever removes it.
//! - A PC taking over an abandoned lock never deletes it. It first makes a
//!   mark named after that lock's token (`library.lock.<token>.taken`) with
//!   create-new, so only one PC can take over each abandoned lock; then it
//!   checks the lock is still the abandoned one and renames its own lock
//!   over it. A PC that stops halfway leaves its mark, which is taken over in
//!   turn the same way.
//! - A writer replaces a file only while it has held the lock for less than
//!   a third of the abandon time (30 s of 90 s), by its own clocks, and only
//!   after checking the lock is still its own and nobody has begun to take it
//!   over. A takeover needs the lock seen unchanged for the whole abandon
//!   time, so what a writer replaces lands first unless the share sits on
//!   the request for more than the 60 s between.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Test seam: tests stop a writer at a named step (`take over`, `record
/// written`, `lock checked`…) to play out how two PCs' steps interleave.
/// Does nothing outside tests.
pub(crate) fn pause(step: &str) {
    #[cfg(test)]
    hooks::run(step);
    let _ = step;
}

#[cfg(test)]
pub(crate) mod hooks {
    use std::cell::RefCell;
    thread_local! {
        static HOOK: RefCell<Option<Box<dyn FnMut(&str)>>> = const { RefCell::new(None) };
    }
    /// Call `f` at every step this thread reaches, until `clear`.
    pub fn set(f: impl FnMut(&str) + 'static) {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }
    pub fn clear() {
        HOOK.with(|h| *h.borrow_mut() = None);
    }
    pub(super) fn run(step: &str) {
        // Taken out while it runs, so a step reached inside it doesn't re-enter it.
        let f = HOOK.with(|h| h.borrow_mut().take());
        if let Some(mut f) = f {
            f(step);
            HOOK.with(|h| {
                let mut h = h.borrow_mut();
                if h.is_none() {
                    *h = Some(f);
                }
            });
        }
    }
}

/// The write lock's file, in the Library folder (LB1).
pub const LOCK_FILE: &str = "library.lock";
/// A write lock (or a takeover's mark) that a waiting PC has seen unchanged
/// this long was left by a PC that crashed or lost the share, and is taken
/// over (LB2).
pub const ABANDONED_AFTER: Duration = Duration::from_secs(90);
/// A writer replaces files only this long after it took the lock, by its own
/// clocks: a third of ABANDONED_AFTER, leaving 60 s for a request already
/// sent to reach the share before anyone may take the lock over (LB2).
pub const HOLD_FOR: Duration = Duration::from_secs(ABANDONED_AFTER.as_secs() / 3);
/// The longest a change waits for the write lock before it gives up,
/// storing nothing (LB1). Longer than ABANDONED_AFTER, so a lock left by a
/// crash is taken over rather than given up on.
pub const LONGEST_WAIT: Duration = Duration::from_secs(120);
/// How many takeovers in a row (PCs that stopped halfway through taking a
/// lock over) are followed before the rest are ignored.
const MAX_MARKS: usize = 16;

/// Who is working: the Windows user and PC name (user and host name on
/// Linux), and whether it's an agent (the command line or the MCP server)
/// working for that user (LB5). The window and the agents give the engine
/// their own; it never reads them itself, so a test can be two PCs at once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Who {
    pub user: String,
    pub pc: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agent: bool,
}

impl Who {
    /// This PC and the user logged on to it.
    pub fn this_pc() -> Who {
        Who::new(&user_name(), &pc_name())
    }

    pub fn new(user: &str, pc: &str) -> Who {
        Who { user: user.into(), pc: pc.into(), agent: false }
    }

    /// The same user and PC, working through an agent.
    pub fn as_agent(mut self) -> Who {
        self.agent = true;
        self
    }

    /// "sam on PROG-1", or "sam on PROG-1 (agent)".
    pub fn label(&self) -> String {
        format!("{} on {}{}", self.user, self.pc, if self.agent { " (agent)" } else { "" })
    }
}

/// Who saved a Library record, and when (seconds since 1970) (LB5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub by: Who,
    pub at: u64,
}

/// The clock the engine goes by: seconds since 1970, shown in local time.
/// The window and the agents give the engine this PC's clock; a test gives
/// it one that stands still until the test moves it (shown as UTC), shared
/// by every PC the test plays.
#[derive(Clone, Debug, Default)]
pub struct Clock(Option<std::sync::Arc<AtomicU64>>);

impl Clock {
    /// This PC's clock and time zone.
    pub fn pc() -> Clock {
        Clock(None)
    }

    /// A clock set to `secs` since 1970 (tests).
    pub fn at(secs: u64) -> Clock {
        Clock(Some(std::sync::Arc::new(AtomicU64::new(secs))))
    }

    /// Move a set clock forward (this PC's clock moves by itself).
    pub fn advance(&self, d: Duration) {
        if let Some(t) = &self.0 {
            t.fetch_add(d.as_secs(), Ordering::SeqCst);
        }
    }

    pub fn now(&self) -> u64 {
        self.0.as_ref().map_or_else(now_secs, |t| t.load(Ordering::SeqCst))
    }

    /// `t` in local time: year, month, day, hour, minute.
    fn local(&self, t: u64) -> (i64, u32, u32, u32, u32) {
        let zone = if self.0.is_some() { 0 } else { local_offset(t) };
        civil(t as i64 + zone)
    }

    /// "9 Nov 09:20" in local time, with the year when it isn't this year
    /// ("9 Nov 2025 09:20") (LB6).
    pub fn shown(&self, t: u64) -> String {
        let (y, m, d, hh, mm) = self.local(t);
        let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(m as usize - 1).min(11)];
        if y == self.local(self.now()).0 { format!("{d} {month} {hh:02}:{mm:02}") } else { format!("{d} {month} {y} {hh:02}:{mm:02}") }
    }

    /// "at 09:20" for a time today, else "on 9 Nov 09:20" (with the year
    /// when it isn't this year), in local time (SV10).
    pub fn when(&self, t: u64) -> String {
        let (y, m, d, hh, mm) = self.local(t);
        let now = self.local(self.now());
        if (y, m, d) == (now.0, now.1, now.2) { format!("at {hh:02}:{mm:02}") } else { format!("on {}", self.shown(t)) }
    }

    /// "2026-11-09 09:20" in local time (agents) (LB6).
    pub fn stamp(&self, t: u64) -> String {
        let (y, m, d, hh, mm) = self.local(t);
        format!("{y}-{m:02}-{d:02} {hh:02}:{mm:02}")
    }
}

/// Year, month, day, hour and minute of `t` seconds since 1970 (Howard
/// Hinnant's civil-from-days).
fn civil(t: i64) -> (i64, u32, u32, u32, u32) {
    let (day, s) = (t.div_euclid(86400), t.rem_euclid(86400));
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d, (s / 3600) as u32, (s % 3600 / 60) as u32)
}

/// Seconds since 1970 of a civil date (Howard Hinnant's days-from-civil).
#[cfg(windows)]
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// What to add to `t` (UTC) for this PC's local time at that moment.
#[cfg(windows)]
fn local_offset(t: u64) -> i64 {
    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        ms: u16,
    }
    unsafe extern "system" {
        fn SystemTimeToTzSpecificLocalTime(zone: *const std::ffi::c_void, utc: *const SystemTime, local: *mut SystemTime) -> i32;
    }
    let t = t - t % 60;
    let (y, m, d, hh, mm) = civil(t as i64);
    let utc = SystemTime { year: y as u16, month: m as u16, day: d as u16, hour: hh as u16, minute: mm as u16, ..Default::default() };
    let mut local = SystemTime::default();
    // SAFETY: both point to SYSTEMTIMEs that live through the call; no zone
    // (null) means this PC's own time zone.
    if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) } == 0 {
        return 0;
    }
    let back = days_from_civil(i64::from(local.year), u32::from(local.month), u32::from(local.day)) * 86400 + i64::from(local.hour) * 3600 + i64::from(local.minute) * 60;
    back - t as i64
}

/// What to add to `t` (UTC) for this PC's local time at that moment.
#[cfg(all(unix, target_pointer_width = "64"))]
fn local_offset(t: u64) -> i64 {
    use std::os::raw::{c_char, c_int, c_long};
    #[repr(C)]
    #[allow(dead_code)]
    struct Tm {
        sec: c_int,
        min: c_int,
        hour: c_int,
        mday: c_int,
        mon: c_int,
        year: c_int,
        wday: c_int,
        yday: c_int,
        isdst: c_int,
        gmtoff: c_long,
        zone: *const c_char,
    }
    unsafe extern "C" {
        fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let secs = t as i64;
    let mut tm = std::mem::MaybeUninit::<Tm>::zeroed();
    // SAFETY: `secs` and `tm` live through the call; localtime_r fills `tm`
    // and returns null when it can't.
    let ok = unsafe { localtime_r(&secs, tm.as_mut_ptr()) };
    if ok.is_null() {
        return 0;
    }
    // SAFETY: filled by localtime_r above.
    i64::from(unsafe { tm.assume_init() }.gmtoff)
}

/// UTC, where the time zone can't be asked.
#[cfg(not(any(windows, all(unix, target_pointer_width = "64"))))]
fn local_offset(_t: u64) -> i64 {
    0
}

/// This PC's name: `COMPUTERNAME` on Windows, the host name elsewhere.
pub fn pc_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        let env = |k: &str| std::env::var(k).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let name = if cfg!(windows) {
            env("COMPUTERNAME")
        } else {
            env("HOSTNAME").or_else(|| ["/proc/sys/kernel/hostname", "/etc/hostname"].iter().find_map(|f| std::fs::read_to_string(f).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())))
        };
        name.unwrap_or_else(|| "this PC".into())
    })
    .clone()
}

/// The user logged on: `USERNAME` on Windows, `USER` elsewhere.
pub fn user_name() -> String {
    ["USERNAME", "USER", "LOGNAME"].iter().find_map(|k| std::env::var(k).ok().filter(|s| !s.trim().is_empty())).unwrap_or_else(|| "someone".into())
}

/// A name made safe for a file name (letters, digits, `-` and `_`).
pub fn file_safe(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(40).collect();
    if t.is_empty() { "PC".into() } else { t }
}

/// A tag no other writer has: this PC's name, the process and a counter
/// (`PROG-1-4312-7`). Names temporary files and write locks (LB3).
pub fn writer_tag() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!("{}-{}-{}", file_safe(&pc_name()), std::process::id(), N.fetch_add(1, Ordering::Relaxed))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// "30 s", or "200 ms" for the short times tests use.
fn shown(d: Duration) -> String {
    if d >= Duration::from_secs(1) { format!("{} s", d.as_secs()) } else { format!("{} ms", d.as_millis()) }
}

/// What the lock file (or a takeover's mark) says: who holds the lock, since
/// when, and the token that tells this holder's lock from every other.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Holder {
    user: String,
    pc: String,
    process: u32,
    /// Seconds since 1970 on the holder's clock (shown only; never compared
    /// with this PC's clock).
    since: u64,
    token: String,
}

/// The name a lock file (or a mark) goes by in the name of the mark that
/// takes it over: its token, or a fingerprint of what's in it when it can't
/// be read (a PC stopped while making it).
fn key(text: &[u8]) -> String {
    match serde_json::from_slice::<Holder>(text) {
        Ok(h) if !h.token.is_empty() && h.token.len() <= 100 && h.token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') => h.token,
        _ => {
            // FNV-1a: the same everywhere, unlike the standard hasher.
            let h = text.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3));
            format!("x{h:016x}")
        }
    }
}

/// The mark a PC makes to take over the lock (or mark) holding `text` (LB2).
fn mark_path(lock: &Path, text: &[u8]) -> PathBuf {
    lock.with_file_name(format!("{LOCK_FILE}.{}.taken", key(text)))
}

/// The lock file, then the mark of each PC taking it over in turn: the last
/// one is whoever is writing (or taking over) now. Empty when the Library
/// isn't locked.
fn read_chain(lock: &Path) -> Vec<Vec<u8>> {
    let mut chain = vec![];
    let Ok(t) = std::fs::read(lock) else { return chain };
    chain.push(t);
    while chain.len() < MAX_MARKS {
        match chain.last().map(|t| std::fs::read(mark_path(lock, t))) {
            Some(Ok(t)) => chain.push(t),
            _ => break,
        }
    }
    chain
}

/// "sam on PROG-2", from a lock file's text.
fn holder_label(text: Option<&Vec<u8>>) -> String {
    text.and_then(|t| serde_json::from_slice::<Holder>(t).ok()).map_or_else(|| "another PC".to_string(), |h| format!("{} on {}", h.user, h.pc))
}

/// Still inside this writer's time: held for less than `hold_for` since
/// `since`, by this PC's steady clock and its wall clock both (the steady
/// one doesn't count time asleep on every system) (LB2).
fn within(since: Instant, wall: SystemTime, hold_for: Duration) -> Result<(), String> {
    if since.elapsed() < hold_for && wall.elapsed().map_or(true, |d| d < hold_for) {
        Ok(())
    } else {
        Err(format!("this PC took longer than {} to write to the Library, so another PC may take its write lock over. Nothing was saved — save again", shown(hold_for)))
    }
}

/// A claim on the lock: a fresh token, and the text of a lock file (or
/// mark) naming this PC with it. Every claim has its own token, so no two
/// lock files or marks ever say the same.
fn claim() -> (String, String) {
    let who = Who::this_pc();
    let token = format!("{}-{}", writer_tag(), SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()));
    let me = Holder { user: who.user, pc: who.pc, process: std::process::id(), since: now_secs(), token: token.clone() };
    let text = serde_json::to_string(&me).unwrap_or_default();
    (token, text)
}

/// A lock this writer holds: which, its token, and its time.
#[derive(Clone)]
struct Lease {
    file: PathBuf,
    token: String,
    /// When it was taken (just before asking for it), by both clocks.
    since: Instant,
    wall: SystemTime,
    /// How long after `since` it may still replace files (LB2).
    hold_for: Duration,
}

thread_local! {
    /// Locks this thread holds, and how many times each is taken.
    static HELD: std::cell::RefCell<Vec<(Lease, usize)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The Library's write lock, held until dropped (LB1). Taking it again on
/// the same thread while it's held just counts, so a change made inside
/// another never waits for itself.
pub struct WriteLock(Lease);

impl WriteLock {
    /// Take the write lock of the Library in `dir`, waiting while another
    /// writer has it (LB1) and taking over one that was abandoned (LB2).
    pub fn take(dir: &Path) -> Result<WriteLock, String> {
        Self::take_for(dir, ABANDONED_AFTER, HOLD_FOR, LONGEST_WAIT)
    }

    /// `take` with other times (tests). The lock replaces files for a third
    /// of `abandoned_after`, as HOLD_FOR is of ABANDONED_AFTER.
    pub fn take_with(dir: &Path, abandoned_after: Duration, longest_wait: Duration) -> Result<WriteLock, String> {
        Self::take_for(dir, abandoned_after, abandoned_after / 3, longest_wait)
    }

    fn take_for(dir: &Path, abandoned_after: Duration, hold_for: Duration, longest_wait: Duration) -> Result<WriteLock, String> {
        let file = dir.join(LOCK_FILE);
        let again = HELD.with(|h| {
            h.borrow_mut().iter_mut().find(|(l, _)| l.file == file).map(|(l, n)| {
                *n += 1;
                l.clone()
            })
        });
        if let Some(lease) = again {
            return Ok(WriteLock(lease));
        }
        // A folder that isn't there is never made here: the share may be
        // down (LB11).
        if !dir.is_dir() {
            return Err(crate::library::unreachable(dir));
        }
        let (token, text) = claim();
        let started = Instant::now();
        // The lock (and marks) seen last, and since when this PC has seen
        // them unchanged: judged by this PC's own watch, never by another
        // PC's clock (LB2).
        let mut seen: Option<(Vec<Vec<u8>>, Instant)> = None;
        let mut wait = Duration::from_millis(2);
        loop {
            let (since, wall) = (Instant::now(), SystemTime::now());
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&file) {
                Ok(mut f) => {
                    use std::io::Write as _;
                    if let Err(e) = f.write_all(text.as_bytes()).and_then(|_| f.sync_all()) {
                        drop(f);
                        let _ = std::fs::remove_file(&file);
                        return Err(format!("can't lock the Library at {}: {e}", dir.display()));
                    }
                    return Ok(Lease { file, token, since, wall, hold_for }.held());
                }
                // Windows reports a lock file being deleted by its holder as
                // access denied: busy, like one that's there.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists || (cfg!(windows) && e.kind() == std::io::ErrorKind::PermissionDenied) => {
                    let chain = read_chain(&file);
                    match &seen {
                        Some((c, t)) if !chain.is_empty() && *c == chain => {
                            if t.elapsed() >= abandoned_after {
                                pause("take over");
                                if let Some(lease) = take_over(&file, &chain, hold_for) {
                                    return Ok(lease.held());
                                }
                                seen = None;
                                continue;
                            }
                        }
                        _ => seen = (!chain.is_empty()).then(|| (chain.clone(), Instant::now())),
                    }
                    if started.elapsed() >= longest_wait {
                        return Err(format!("the Library at {} is busy: {} has been writing to it for more than {}. Nothing was saved — try again.", dir.display(), holder_label(chain.first()), shown(longest_wait)));
                    }
                    pause("lock busy");
                    // A little jitter so two waiting PCs don't keep colliding.
                    let jitter = Duration::from_micros(u64::from(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos())) % 3000);
                    std::thread::sleep(wait + jitter);
                    wait = (wait * 2).min(Duration::from_millis(40));
                }
                Err(e) => return Err(format!("can't lock the Library at {}: {e}", dir.display())),
            }
        }
    }

    /// Still this writer's to write with (LB2): inside its time, still its
    /// own lock, and nobody has begun to take it over. Checked just before
    /// every file is replaced.
    pub fn still_held(&self) -> Result<(), String> {
        let l = &self.0;
        within(l.since, l.wall, l.hold_for)?;
        let now = std::fs::read(&l.file).ok();
        let ours = now.as_deref().and_then(|t| serde_json::from_slice::<Holder>(t).ok()).is_some_and(|h| h.token == l.token);
        let taking_over = now.as_deref().is_some_and(|t| mark_path(&l.file, t).exists());
        pause("lock read");
        if !ours || taking_over {
            return Err("another PC took over the Library's write lock while this PC was writing. Nothing was saved — save again".into());
        }
        // Reading the lock can be slow on a share: still inside the time
        // after it, so only the change itself is left to send (LB2).
        within(l.since, l.wall, l.hold_for)
    }
}

impl Lease {
    /// Note this lock as held by this thread.
    fn held(self) -> WriteLock {
        HELD.with(|h| h.borrow_mut().push((self.clone(), 1)));
        WriteLock(self)
    }
}

/// Take over the abandoned lock (or mark) at the end of `chain` (LB2). Only
/// one PC can: it makes the mark named after it with create-new, checks that
/// nothing has changed, and renames its own lock over `library.lock`; then
/// the old marks are cleared. None when another PC is taking it over, or
/// it changed or went.
fn take_over(file: &Path, chain: &[Vec<u8>], hold_for: Duration) -> Option<Lease> {
    use std::io::Write as _;
    let (token, me) = claim();
    // This PC's time runs from making the mark: from then on others see it
    // and could judge it abandoned in turn.
    let (since, wall) = (Instant::now(), SystemTime::now());
    let mark = mark_path(file, chain.last()?);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&mark).ok()?;
    let written = f.write_all(me.as_bytes()).and_then(|_| f.sync_all());
    drop(f);
    // Half a mark is taken over in turn once it has stayed that way.
    written.ok()?;
    pause("take over: marked");
    // Gone or replaced since: there's nothing to take over, and this mark is
    // of no use to anyone (no lock holding what it names comes back).
    let root_moved_on = || read_chain(file).first() != chain.first();
    let mut expect = chain.to_vec();
    expect.push(me.as_bytes().to_vec());
    if read_chain(file) != expect {
        if root_moved_on() {
            let _ = std::fs::remove_file(&mark);
        }
        return None;
    }
    pause("take over: checked");
    if replace(file, me.as_bytes(), || within(since, wall, hold_for)).is_err() {
        if root_moved_on() {
            let _ = std::fs::remove_file(&mark);
        }
        return None;
    }
    // The old lock is gone: its marks, this PC's too, are history.
    for t in chain {
        let _ = std::fs::remove_file(mark_path(file, t));
    }
    Some(Lease { file: file.to_path_buf(), token, since, wall, hold_for })
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let last = HELD.with(|h| {
            let mut h = h.borrow_mut();
            match h.iter().position(|(l, _)| l.file == self.0.file && l.token == self.0.token) {
                Some(i) => {
                    h[i].1 -= 1;
                    if h[i].1 == 0 {
                        h.remove(i);
                        true
                    } else {
                        false
                    }
                }
                None => false,
            }
        });
        // Removed only by its holder, and only inside its time (LB2): later,
        // it is left for another PC to take over.
        if last && self.still_held().is_ok() {
            let _ = std::fs::remove_file(&self.0.file);
        }
    }
}

/// Write a whole file: into a temporary file of this writer's own next to it
/// (`index.json.PROG-1-4312-7.tmp`), then renamed over it, so a reader never
/// sees half a file and two writers never share a temporary file (LB3). With
/// `lock`, the write lock must still be this writer's just before the rename
/// (LB2); if it isn't, nothing is replaced.
pub fn write_file(path: &Path, bytes: &[u8], lock: Option<&WriteLock>) -> Result<(), String> {
    replace(path, bytes, || lock.map_or(Ok(()), WriteLock::still_held))
}

/// `write_file`, checking `fence` just before every try of the rename.
fn replace(path: &Path, bytes: &[u8], fence: impl Fn() -> Result<(), String>) -> Result<(), String> {
    if let Some(d) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let name = path.file_name().map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!("{name}.{}.tmp", writer_tag()));
    let written = (|| {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    let done = written.map_err(|e| e.to_string()).and_then(|_| rename(&tmp, path, &fence));
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done
}

/// Rename after `fence` says it may, trying again for a moment when Windows
/// says the file is busy (a virus scanner or the search indexer reading it),
/// with the fence checked again before each try.
fn rename(from: &Path, to: &Path, fence: &impl Fn() -> Result<(), String>) -> Result<(), String> {
    let mut tries = 0;
    loop {
        fence()?;
        pause("lock checked");
        match std::fs::rename(from, to) {
            Err(e) if cfg!(windows) && tries < 40 && !matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::AlreadyExists) => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            r => return r.map_err(|e| e.to_string()),
        }
    }
}

#[cfg(test)]
mod shared_tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestpro_lock_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut n: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|f| f.file_name().to_string_lossy().into_owned()).collect();
        n.sort();
        n
    }

    #[test]
    fn a_lock_left_by_a_pc_that_crashed_is_taken_over_but_one_in_use_is_waited_for() {
        use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
        use std::sync::Arc;
        let dir = scratch("abandoned");
        // Abandoned after a short time, but with the full 30 s to write once
        // taken over, so a busy PC never runs out of time while it takes the
        // lock over or lets go (the time to write has tests of its own).
        let short = Duration::from_millis(100);
        let take_dead_lock = || WriteLock::take_for(&dir, short, HOLD_FOR, Duration::from_secs(10));
        // PROG-9 crashed while saving: its lock stays behind.
        std::fs::write(dir.join(LOCK_FILE), r#"{"user":"sam","pc":"PROG-9","process":7,"since":0,"token":"PROG-9-7-0"}"#).unwrap();
        let t = Instant::now();
        let lock = take_dead_lock().unwrap();
        assert!(t.elapsed() >= short, "taken over before it counted as abandoned");
        assert!(std::fs::read_to_string(dir.join(LOCK_FILE)).unwrap().contains(&pc_name()));
        drop(lock);
        // A PC that crashed just as it made the lock, before writing in it.
        std::fs::write(dir.join(LOCK_FILE), "").unwrap();
        let t = Instant::now();
        drop(take_dead_lock().unwrap());
        assert!(t.elapsed() >= short);
        // A PC writing now holds it: another waits for it and never takes it
        // over while it's in use. The holder lets go once the other has found
        // it busy a few times; the other gets it then, and only then.
        let lock = WriteLock::take(&dir).unwrap();
        let let_go = Arc::new(AtomicBool::new(false));
        let (busy, busy_rx) = std::sync::mpsc::channel::<()>();
        let other = {
            let (dir, let_go) = (dir.clone(), let_go.clone());
            std::thread::spawn(move || {
                let (mut times, mut busy) = (0, Some(busy));
                let taken_over = Arc::new(AtomicBool::new(false));
                let saw = taken_over.clone();
                hooks::set(move |s| match s {
                    "lock busy" => {
                        times += 1;
                        if times >= 3 && let Some(b) = busy.take() {
                            let _ = b.send(());
                        }
                    }
                    "take over" => saw.store(true, SeqCst),
                    _ => {}
                });
                let got = WriteLock::take(&dir).map(|l| {
                    let after = let_go.load(SeqCst);
                    drop(l);
                    after
                });
                hooks::clear();
                (got, taken_over.load(SeqCst))
            })
        };
        busy_rx.recv_timeout(Duration::from_secs(60)).expect("the other PC never found the lock busy");
        lock.still_held().unwrap();
        let_go.store(true, SeqCst);
        drop(lock);
        let (got, taken_over) = other.join().unwrap();
        assert!(!taken_over, "a lock in use was taken over");
        assert!(got.unwrap(), "the other PC got the lock before it was let go");
        // Released: no lock file, mark or temporary file is left.
        assert!(names(&dir).is_empty(), "{:?}", names(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_writer_whose_lock_was_taken_over_replaces_nothing() {
        let dir = scratch("taken_over");
        let file = dir.join("index.json");
        std::fs::write(&file, "before").unwrap();
        let lock = WriteLock::take(&dir).unwrap();
        // Another PC has begun to take it over (its mark is there): nothing
        // is replaced either.
        let mine = std::fs::read(dir.join(LOCK_FILE)).unwrap();
        std::fs::write(mark_path(&dir.join(LOCK_FILE), &mine), r#"{"user":"kim","pc":"PROG-2","process":9,"since":0,"token":"PROG-2-9-1"}"#).unwrap();
        let e = write_file(&file, b"after", Some(&lock)).unwrap_err();
        assert!(e.contains("Nothing was saved"), "{e}");
        // This PC stalled; another took the lock over as abandoned.
        std::fs::write(dir.join(LOCK_FILE), r#"{"user":"kim","pc":"PROG-2","process":9,"since":0,"token":"PROG-2-9-1"}"#).unwrap();
        let e = write_file(&file, b"after", Some(&lock)).unwrap_err();
        assert!(e.contains("Nothing was saved"), "{e}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
        drop(lock);
        // The other PC's lock is left alone, and no temporary file stays.
        assert!(std::fs::read_to_string(dir.join(LOCK_FILE)).unwrap().contains("PROG-2"));
        assert_eq!(names(&dir).len(), 3, "{:?}", names(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_change_made_inside_another_on_the_same_thread_does_not_wait_for_itself() {
        let dir = scratch("reentrant");
        let outer = WriteLock::take(&dir).unwrap();
        let t = Instant::now();
        let inner = WriteLock::take_with(&dir, Duration::from_secs(5), Duration::from_secs(1)).unwrap();
        assert!(t.elapsed() < Duration::from_millis(100));
        drop(inner);
        outer.still_held().unwrap();
        drop(outer);
        assert!(!dir.join(LOCK_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB2: a PC that stopped halfway through taking over a crashed PC's
    /// lock leaves its mark; once that has stayed unchanged for the abandon
    /// time, the next PC takes over in its turn, and every mark is cleared.
    #[test]
    fn a_pc_that_stopped_halfway_through_a_takeover_is_taken_over_in_turn() {
        let dir = scratch("halfway");
        let lock = dir.join(LOCK_FILE);
        let dead = r#"{"user":"sam","pc":"PROG-9","process":7,"since":0,"token":"PROG-9-7-0"}"#;
        std::fs::write(&lock, dead).unwrap();
        // PROG-8 made its mark on PROG-9's lock, then crashed.
        std::fs::write(mark_path(&lock, dead.as_bytes()), r#"{"user":"kim","pc":"PROG-8","process":8,"since":0,"token":"PROG-8-8-0"}"#).unwrap();
        // Abandoned after a short time, with the full 30 s to write once
        // taken over, so a busy PC never runs out of it here.
        let short = Duration::from_millis(100);
        let t = Instant::now();
        let held = WriteLock::take_for(&dir, short, HOLD_FOR, Duration::from_secs(5)).unwrap();
        assert!(t.elapsed() >= short);
        let text = std::fs::read_to_string(&lock).unwrap();
        assert!(text.contains(&pc_name()) && !text.contains("PROG-9"), "{text}");
        held.still_held().unwrap();
        assert_eq!(names(&dir), vec![LOCK_FILE.to_string()], "marks left behind");
        drop(held);
        assert!(names(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB2's bound. A writer replaces files only for a third of the abandon
    /// time after it took the lock, and another PC takes a lock over only
    /// after seeing it unchanged for the whole abandon time. So a rename this
    /// writer sends after its last check lands before any takeover unless it
    /// is held up for longer than the two thirds between.
    #[test]
    fn a_writer_replaces_files_only_inside_its_time_and_a_takeover_waits_for_the_rest() {
        // Abandoned after 1.8 s: a writer replaces files for 0.6 s.
        let abandoned = Duration::from_millis(1800);
        let dir = scratch("bound");
        let file = dir.join("index.json");
        std::fs::write(&file, "before").unwrap();
        // PROG-1 took the lock, then stalled before it wrote: past its time,
        // it replaces nothing and leaves its lock to be taken over.
        let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(10)).unwrap();
        std::thread::sleep(Duration::from_millis(700));
        let e = write_file(&file, b"PROG-1, late", Some(&lock)).unwrap_err();
        assert!(e.contains("took longer than 600 ms") && e.contains("Nothing was saved"), "{e}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
        drop(lock);
        assert!(dir.join(LOCK_FILE).exists(), "a lock past its time was removed");
        std::fs::remove_file(dir.join(LOCK_FILE)).unwrap();

        // PROG-1 takes the lock and writes in time, but its rename is held up
        // after its last check for 1.0 s, under the 1.2 s between. PROG-2
        // waits meanwhile and takes the lock over (PROG-1 is past its time
        // when it lets go), then saves.
        let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(10)).unwrap();
        let t0 = Instant::now();
        let prog2 = {
            let (dir, file) = (dir.clone(), file.clone());
            std::thread::spawn(move || {
                let l = WriteLock::take_with(&dir, abandoned, Duration::from_secs(10)).unwrap();
                let got = t0.elapsed();
                write_file(&file, b"PROG-2", Some(&l)).unwrap();
                got
            })
        };
        hooks::set(|s| {
            if s == "lock checked" {
                std::thread::sleep(Duration::from_millis(1000));
            }
        });
        let r = write_file(&file, b"PROG-1", Some(&lock));
        hooks::clear();
        r.unwrap();
        drop(lock);
        let got = prog2.join().unwrap();
        assert!(got >= abandoned, "PROG-2 took the lock over after {got:?}, before it had stayed unchanged for {abandoned:?}");
        // PROG-1's held-up rename landed first; PROG-2's save is the one kept.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "PROG-2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB2: the time is checked again after reading the lock, not only
    /// before. A writer inside its time whose read of the lock is held up
    /// past it replaces nothing, so a slow read can't push its rename past
    /// another PC's takeover.
    #[test]
    fn a_writer_held_up_while_checking_its_lock_replaces_nothing() {
        let abandoned = Duration::from_millis(1800);
        let dir = scratch("slow_check");
        let file = dir.join("index.json");
        std::fs::write(&file, "before").unwrap();
        let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(10)).unwrap();
        hooks::set(|s| {
            if s == "lock read" {
                std::thread::sleep(Duration::from_millis(700));
            }
        });
        let r = write_file(&file, b"PROG-1, late", Some(&lock));
        hooks::clear();
        let e = r.unwrap_err();
        assert!(e.contains("took longer than 600 ms") && e.contains("Nothing was saved"), "{e}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
        drop(lock);
        let _ = std::fs::remove_file(dir.join(LOCK_FILE));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB2: two waiting PCs judge one crashed PC's lock abandoned at the
    /// same moment and both take it over, the second one stopped at each step
    /// of its takeover in turn while the first one goes ahead. However their
    /// steps interleave, only one of them holds the lock at any moment, and
    /// both get it in the end.
    #[test]
    fn two_pcs_taking_over_one_abandoned_lock_never_both_hold_it() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        use std::sync::mpsc::channel;
        use std::sync::Arc;
        for (n, step) in ["take over", "take over: marked", "take over: checked"].into_iter().enumerate() {
            let dir = scratch(&format!("double_takeover_{n}"));
            // PROG-9 crashed while saving: its lock stays behind.
            std::fs::write(dir.join(LOCK_FILE), r#"{"user":"sam","pc":"PROG-9","process":7,"since":0,"token":"PROG-9-7-0"}"#).unwrap();
            let abandoned = Duration::from_millis(600);
            let (holding, both) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
            let (pc2_there, pc2_there_rx) = channel::<()>();
            let (pc1_holds, pc1_holds_rx) = channel::<()>();
            // Hold the lock a moment, counting how often two PCs held it at once.
            let hold = |lock: WriteLock, holding: &AtomicUsize, both: &AtomicUsize, told: Option<std::sync::mpsc::Sender<()>>| {
                if holding.fetch_add(1, SeqCst) > 0 {
                    both.fetch_add(1, SeqCst);
                }
                if let Some(t) = told {
                    let _ = t.send(());
                }
                std::thread::sleep(Duration::from_millis(150));
                holding.fetch_sub(1, SeqCst);
                drop(lock);
            };
            // PROG-1 takes it over once PROG-2 has reached its step.
            let pc1 = {
                let (dir, holding, both) = (dir.clone(), holding.clone(), both.clone());
                std::thread::spawn(move || {
                    let mut first = true;
                    hooks::set(move |s| {
                        if s == "take over" && std::mem::take(&mut first) {
                            let _ = pc2_there_rx.recv_timeout(Duration::from_secs(3));
                        }
                    });
                    let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(20));
                    hooks::clear();
                    hold(lock.unwrap(), &holding, &both, Some(pc1_holds));
                })
            };
            // PROG-2 stops at `step` until PROG-1 holds the lock (or a while).
            let pc2 = {
                let (dir, holding, both) = (dir.clone(), holding.clone(), both.clone());
                std::thread::spawn(move || {
                    let mut first = true;
                    hooks::set(move |s| {
                        if s == step && std::mem::take(&mut first) {
                            let _ = pc2_there.send(());
                            let _ = pc1_holds_rx.recv_timeout(Duration::from_secs(3));
                        }
                    });
                    let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(20));
                    hooks::clear();
                    hold(lock.unwrap(), &holding, &both, None);
                })
            };
            pc1.join().unwrap();
            pc2.join().unwrap();
            assert_eq!(both.load(SeqCst), 0, "with PROG-2 stopped at '{step}', PROG-1 and PROG-2 both held the write lock at once");
            let left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|f| f.file_name().to_string_lossy().into_owned()).filter(|f| f.ends_with(".taken")).collect();
            assert!(left.is_empty(), "takeover marks left behind: {left:?}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
