// Start-up rules from the original (MECHANICS.md "Start-up", SU1–SU5):
// - it changes to the program's own folder first (SU1),
//   so loose files such as the window icon are found next to the program;
// - it reads the first command-line word: "/hide" starts without showing
//   the main window, a name ending in ".NXP" or ".nxp" is opened once the
//   window is up (SU3);
// - the main window is shown maximized (show state 3, SW_SHOWMAXIMIZED),
//   or hidden (0) in /hide mode; the icons are set last (SU4–SU5).

use std::path::PathBuf;

/// ShowWindow states the original uses for its main window.
pub const SW_HIDE: i32 = 0;
pub const SW_SHOWMAXIMIZED: i32 = 3;
/// The first-word switch for starting hidden.
pub const HIDE_SWITCH: &str = "/hide";
/// The extension compared at the end of the first word, in the two spellings
/// the original accepts (it checks 'N','X','P' or 'n','x','p' letter by letter).
pub const OPEN_EXT_UPPER: &str = ".NXP";
pub const OPEN_EXT_LOWER: &str = ".nxp";

/// What a plain first argument asks for.
#[derive(Debug, PartialEq)]
pub enum StartFile {
    /// A Lantek nesting export (the original's rule).
    Nesting(PathBuf),
    /// One of this app's own job files (so double-clicking one opens it).
    Job(PathBuf),
}

/// The first command-line word when it isn't one of the app's `--` options.
pub fn first_word(args: &[String]) -> Option<&str> {
    args.get(1).map(String::as_str).filter(|a| !a.starts_with("--"))
}

/// The main window's show state for a first word (SW_HIDE for "/hide").
pub fn show_state(first: Option<&str>) -> i32 {
    if first == Some(HIDE_SWITCH) { SW_HIDE } else { SW_SHOWMAXIMIZED }
}

/// The file to open after start-up, if the first word names one.
pub fn start_file(first: Option<&str>, job_ext: &str) -> Option<StartFile> {
    let w = first?;
    if w.ends_with(OPEN_EXT_UPPER) || w.ends_with(OPEN_EXT_LOWER) {
        return Some(StartFile::Nesting(PathBuf::from(w)));
    }
    let p = PathBuf::from(w);
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case(job_ext)) {
        return Some(StartFile::Job(p));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn nxp_in_either_spelling() {
        let f = |w: &str| start_file(Some(w), "snjob");
        assert_eq!(f("C:\\x\\job.NXP"), Some(StartFile::Nesting("C:\\x\\job.NXP".into())));
        assert_eq!(f("job.nxp"), Some(StartFile::Nesting("job.nxp".into())));
        // Mixed case isn't one of the original's two spellings.
        assert_eq!(f("job.Nxp"), None);
        assert_eq!(f("job.dxf"), None);
        assert_eq!(f("plate.SNJOB"), Some(StartFile::Job("plate.SNJOB".into())));
    }

    #[test]
    fn options_are_not_files() {
        assert_eq!(first_word(&a(&["app", "--screenshot", "x.png"])), None);
        assert_eq!(first_word(&a(&["app", "a.nxp"])), Some("a.nxp"));
        assert_eq!(first_word(&a(&["app"])), None);
    }

    #[test]
    fn maximized_unless_hidden() {
        assert_eq!(show_state(None), SW_SHOWMAXIMIZED);
        assert_eq!(show_state(Some("a.nxp")), SW_SHOWMAXIMIZED);
        assert_eq!(show_state(Some("/hide")), SW_HIDE);
    }
}
