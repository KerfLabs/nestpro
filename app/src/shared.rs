//! Writing to a Library that several PCs share (ADR 0003: the Library is a
//! folder on the shop's share). Every change to a shared file in the Library
//! (job and library part records, the list of records, the sheet stock) is
//! made under the Library's write lock (LB1), a lock left by a PC that
//! crashed is taken over after a short time (LB2), and every file is written
//! through a temporary file of this writer's own and renamed into place
//! (LB3). Reading never waits for the lock: a file is always whole.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The write lock's file, in the Library folder (LB1).
pub const LOCK_FILE: &str = "library.lock";
/// A write lock that stays the same this long was left by a PC that crashed
/// or lost the share, and the next writer takes it over (LB2).
pub const ABANDONED_AFTER: Duration = Duration::from_secs(10);
/// The longest a change waits for the write lock before it gives up,
/// storing nothing (LB1).
pub const LONGEST_WAIT: Duration = Duration::from_secs(30);

/// Who is writing: the Windows user and PC name (user and host name on Linux).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Who {
    pub user: String,
    pub pc: String,
}

impl Who {
    /// This PC and the user logged on to it.
    pub fn this_pc() -> Who {
        Who { user: user_name(), pc: pc_name() }
    }

    /// "sam on PROG-1".
    pub fn label(&self) -> String {
        format!("{} on {}", self.user, self.pc)
    }
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
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// What the lock file says: who holds the lock, since when, and the token
/// that tells this holder's lock from a later one.
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

thread_local! {
    /// Locks this thread holds: (lock file, token, how many times taken).
    static HELD: std::cell::RefCell<Vec<(PathBuf, String, usize)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The Library's write lock, held until dropped (LB1). Taking it again on
/// the same thread while it's held just counts, so a change made inside
/// another never waits for itself.
pub struct WriteLock {
    file: PathBuf,
    token: String,
}

impl WriteLock {
    /// Take the write lock of the Library in `dir`, waiting while another
    /// writer has it (LB1) and taking over one that was abandoned (LB2).
    pub fn take(dir: &Path) -> Result<WriteLock, String> {
        Self::take_with(dir, ABANDONED_AFTER, LONGEST_WAIT)
    }

    /// `take` with other times (tests).
    pub fn take_with(dir: &Path, abandoned_after: Duration, longest_wait: Duration) -> Result<WriteLock, String> {
        let file = dir.join(LOCK_FILE);
        let again = HELD.with(|h| {
            h.borrow_mut().iter_mut().find(|(f, _, _)| *f == file).map(|(_, t, n)| {
                *n += 1;
                t.clone()
            })
        });
        if let Some(token) = again {
            return Ok(WriteLock { file, token });
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("can't use the Library folder {}: {e}", dir.display()))?;
        let who = Who::this_pc();
        let token = format!("{}-{}", writer_tag(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()));
        let me = Holder { user: who.user, pc: who.pc, process: std::process::id(), since: now_secs(), token: token.clone() };
        let text = serde_json::to_string(&me).map_err(|e| e.to_string())?;
        let started = Instant::now();
        // The lock seen last and since when this PC has seen it unchanged:
        // judged by this PC's own watch, never by another PC's clock (LB2).
        let mut seen: Option<(String, Instant)> = None;
        let mut pause = Duration::from_millis(2);
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&file) {
                Ok(mut f) => {
                    use std::io::Write as _;
                    if let Err(e) = f.write_all(text.as_bytes()).and_then(|_| f.sync_all()) {
                        drop(f);
                        let _ = std::fs::remove_file(&file);
                        return Err(format!("can't lock the Library at {}: {e}", dir.display()));
                    }
                    HELD.with(|h| h.borrow_mut().push((file.clone(), token.clone(), 1)));
                    return Ok(WriteLock { file, token });
                }
                // Windows reports a lock file being deleted by its holder as
                // access denied: busy, like one that's there.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists || (cfg!(windows) && e.kind() == std::io::ErrorKind::PermissionDenied) => {
                    let current = std::fs::read_to_string(&file).unwrap_or_default();
                    match &seen {
                        Some((c, since)) if *c == current => {
                            if since.elapsed() >= abandoned_after {
                                // Abandoned: remove it, unless it changed hands
                                // just now.
                                if std::fs::read_to_string(&file).unwrap_or_default() == current {
                                    let _ = std::fs::remove_file(&file);
                                }
                                seen = None;
                                continue;
                            }
                        }
                        _ => seen = Some((current.clone(), Instant::now())),
                    }
                    if started.elapsed() >= longest_wait {
                        let by = serde_json::from_str::<Holder>(&current).map_or_else(|_| "another PC".to_string(), |h| format!("{} on {}", h.user, h.pc));
                        return Err(format!("the Library at {} is busy: {by} has been writing to it for more than {} s. Nothing was saved — try again.", dir.display(), longest_wait.as_secs()));
                    }
                    // A little jitter so two waiting PCs don't keep colliding.
                    let jitter = Duration::from_micros(u64::from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos())) % 3000);
                    std::thread::sleep(pause + jitter);
                    pause = (pause * 2).min(Duration::from_millis(40));
                }
                Err(e) => return Err(format!("can't lock the Library at {}: {e}", dir.display())),
            }
        }
    }

    /// Still this writer's: no other PC took it over as abandoned (LB2).
    /// Checked just before a file is replaced.
    pub fn still_held(&self) -> Result<(), String> {
        let ours = std::fs::read_to_string(&self.file).ok().and_then(|t| serde_json::from_str::<Holder>(&t).ok()).is_some_and(|h| h.token == self.token);
        if ours {
            Ok(())
        } else {
            Err("another PC took over the Library's write lock while this PC was writing (it took longer than 10 s). Nothing was saved — save again".into())
        }
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let last = HELD.with(|h| {
            let mut h = h.borrow_mut();
            match h.iter().position(|(f, _, _)| *f == self.file) {
                Some(i) => {
                    h[i].2 -= 1;
                    if h[i].2 == 0 {
                        h.remove(i);
                        true
                    } else {
                        false
                    }
                }
                None => true,
            }
        });
        if last && self.still_held().is_ok() {
            let _ = std::fs::remove_file(&self.file);
        }
    }
}

/// Write a whole file: into a temporary file of this writer's own next to it
/// (`index.json.PROG-1-4312-7.tmp`), then renamed over it, so a reader never
/// sees half a file and two writers never share a temporary file (LB3). With
/// `lock`, the write lock must still be this writer's just before the rename
/// (LB2); if it isn't, nothing is replaced.
pub fn write_file(path: &Path, bytes: &[u8], lock: Option<&WriteLock>) -> Result<(), String> {
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
    let done = written.map_err(|e| e.to_string()).and_then(|_| match lock {
        Some(l) => l.still_held(),
        None => Ok(()),
    });
    let done = done.and_then(|_| rename(&tmp, path).map_err(|e| e.to_string()));
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done
}

/// Rename, trying again for a moment when Windows says the file is busy (a
/// virus scanner or the search indexer reading it).
fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut tries = 0;
    loop {
        match std::fs::rename(from, to) {
            Err(e) if cfg!(windows) && tries < 40 && !matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::AlreadyExists) => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            r => return r,
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

    #[test]
    fn a_lock_left_by_a_pc_that_crashed_is_taken_over_but_one_in_use_is_waited_for() {
        let dir = scratch("abandoned");
        let short = Duration::from_millis(300);
        // PROG-9 crashed while saving: its lock stays behind.
        std::fs::write(dir.join(LOCK_FILE), r#"{"user":"sam","pc":"PROG-9","process":7,"since":0,"token":"PROG-9-7-0"}"#).unwrap();
        let t = Instant::now();
        let lock = WriteLock::take_with(&dir, short, Duration::from_secs(5)).unwrap();
        assert!(t.elapsed() >= short, "taken over before it counted as abandoned");
        // A PC writing now holds it: another waits for it, however long that
        // takes, and never takes it over while it's in use.
        let other = {
            let dir = dir.clone();
            std::thread::spawn(move || {
                let t = Instant::now();
                WriteLock::take_with(&dir, Duration::from_secs(2), Duration::from_secs(5)).map(|_| t.elapsed())
            })
        };
        std::thread::sleep(Duration::from_millis(500));
        lock.still_held().unwrap();
        drop(lock);
        let waited = other.join().unwrap().unwrap();
        assert!(waited >= Duration::from_millis(450) && waited < Duration::from_secs(2), "waited {waited:?}");
        // Released: no lock file is left.
        assert!(!dir.join(LOCK_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_writer_whose_lock_was_taken_over_replaces_nothing() {
        let dir = scratch("taken_over");
        let file = dir.join("index.json");
        std::fs::write(&file, "before").unwrap();
        let lock = WriteLock::take(&dir).unwrap();
        // This PC stalled; another took the lock over as abandoned.
        std::fs::write(dir.join(LOCK_FILE), r#"{"user":"kim","pc":"PROG-2","process":9,"since":0,"token":"PROG-2-9-1"}"#).unwrap();
        let e = write_file(&file, b"after", Some(&lock)).unwrap_err();
        assert!(e.contains("Nothing was saved"), "{e}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
        drop(lock);
        // The other PC's lock is left alone, and no temporary file stays.
        assert!(std::fs::read_to_string(dir.join(LOCK_FILE)).unwrap().contains("PROG-2"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
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
}
