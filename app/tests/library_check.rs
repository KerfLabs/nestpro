//! The two-PC Library check (`nestpro library-check`) started as two separate
//! programs at once on one folder, as two programming PCs would run it on the
//! shop's share: the Library's write lock and each writer's own temporary
//! files have to hold between processes, not only between threads (LB1–LB4).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

/// A folder of the test's own under the build folder (not the RAM disk).
fn scratch(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `nestpro library-check <folder> --rounds n --pc <pc> --text`, with a home
/// folder of its own so it never sees this PC's settings or Library.
fn start(home: &Path, folder: &Path, pc: &str, rounds: u32) -> Child {
    Command::new(env!("CARGO_BIN_EXE_nestpro"))
        .arg("library-check")
        .arg(folder)
        .args(["--rounds", &rounds.to_string(), "--pc", pc, "--text"])
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("APPDATA", home)
        .env("LANTEK_DIR", home.join("no-lantek"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start nestpro")
}

fn report(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn two_pcs_running_the_library_check_at_once_both_pass_and_a_half_written_record_fails_it() {
    let base = scratch("library_check");
    let folder = base.join("check");
    // Two "PCs" started at the same moment on one empty folder.
    let pcs = [start(&base, &folder, "PROG-1", 60), start(&base, &folder, "PROG-2", 60)];
    let outs: Vec<Output> = pcs.into_iter().map(|c| c.wait_with_output().unwrap()).collect();
    for o in &outs {
        let r = report(o);
        eprintln!("{r}\n");
        assert!(o.status.success() && r.contains("PASS"), "a run failed:\n{r}");
    }
    // The run that finished last checked the other one's records too.
    assert!(outs.iter().any(|o| report(o).contains("from 2 run(s)")), "neither run saw the other");

    // A job record cut off halfway, as a dropped connection would leave it:
    // the next run fails and says which record and what's wrong, plainly.
    let rec = folder.join("Jobs").join("0").join("5.snjob");
    let text = std::fs::read_to_string(&rec).unwrap();
    std::fs::write(&rec, &text[..text.len() / 2]).unwrap();
    let o = start(&base, &folder, "PROG-1", 2).wait_with_output().unwrap();
    let r = report(&o);
    eprintln!("{r}\n");
    assert!(!o.status.success(), "a half-written record passed:\n{r}");
    assert!(r.contains("FAIL") && r.contains("Job record 5 (Jobs/0/5.snjob) is half-written or damaged"), "{r}");

    // A real Library is refused and left as it was.
    let real = base.join("Library");
    std::fs::create_dir_all(real.join("Jobs").join("0")).unwrap();
    std::fs::write(real.join("index.json"), r#"{"format":"nestpro-library","version":1,"entries":[]}"#).unwrap();
    let o = start(&base, &real, "PROG-1", 2).wait_with_output().unwrap();
    let r = report(&o);
    assert!(!o.status.success() && r.contains("real Library"), "{r}");
    assert_eq!(std::fs::read_dir(&real).unwrap().count(), 2, "the check wrote into a real Library");
    let _ = std::fs::remove_dir_all(&base);
}
