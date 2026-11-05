//! Fallback for DWGs our reader cannot open (DW16): an installed DWG→DXF converter
//! writes a temporary DXF, which the DXF reader then reads.
//!
//! Looked for, in order: the ODA File Converter (its usual install folders on
//! Windows, or on PATH), then LibreDWG's `dwg2dxf` on PATH. Nothing is downloaded or
//! bundled; without a converter the user gets the plain "save it as DXF" message.

use crate::dxf::Drawing;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub enum Converter {
    Oda(PathBuf),
    LibreDwg(PathBuf),
}

impl Converter {
    pub fn name(&self) -> &'static str {
        match self {
            Converter::Oda(_) => "the ODA File Converter",
            Converter::LibreDwg(_) => "dwg2dxf",
        }
    }
}

fn on_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(exe)).find(|p| p.is_file())
}

/// ODA install folders: `<Program Files>\ODA\ODAFileConverter <version>\ODAFileConverter.exe`.
fn oda_installed() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = vec![];
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(p) = std::env::var_os(var) {
                roots.push(PathBuf::from(p).join("ODA"));
            }
        }
        roots.push(PathBuf::from(r"C:\Program Files\ODA"));
    } else {
        roots.push(PathBuf::from("/opt"));
        roots.push(PathBuf::from("/usr/lib"));
    }
    let exe = if cfg!(windows) { "ODAFileConverter.exe" } else { "ODAFileConverter" };
    let mut found: Vec<PathBuf> = vec![];
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("odafileconverter") {
                let p = e.path().join(exe);
                if p.is_file() {
                    found.push(p);
                }
            }
        }
    }
    // Highest version folder name last → take it.
    found.sort();
    found.pop()
}

pub fn find() -> Option<Converter> {
    let oda_exe = if cfg!(windows) { "ODAFileConverter.exe" } else { "ODAFileConverter" };
    if let Some(p) = oda_installed().or_else(|| on_path(oda_exe)) {
        return Some(Converter::Oda(p));
    }
    let lib_exe = if cfg!(windows) { "dwg2dxf.exe" } else { "dwg2dxf" };
    on_path(lib_exe).map(Converter::LibreDwg)
}

fn run_with_timeout(mut cmd: Command, limit: Duration) -> Result<(), String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("it could not be started ({e})"))?;
    let t0 = Instant::now();
    loop {
        match child.try_wait() {
            // A converter that reports a failure may have left a DXF cut short:
            // its output isn't read (DW16).
            Ok(Some(st)) if st.success() => return Ok(()),
            Ok(Some(st)) => return Err(format!("it stopped with an error ({st})")),
            Ok(None) if t0.elapsed() < limit => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("it did not finish within a minute".into());
            }
        }
    }
}

fn find_dxf(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dxf")))
}

/// Convert through `conv` and read the DXF it writes.
pub fn run(conv: &Converter, bytes: &[u8], unit_mm: f64) -> Result<Drawing, String> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("nestpro-dwg-{}-{stamp}", std::process::id()));
    let (inp, out) = (tmp.join("in"), tmp.join("out"));
    let result = (|| {
        std::fs::create_dir_all(&inp).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let src = inp.join("part.dwg");
        std::fs::write(&src, bytes).map_err(|e| e.to_string())?;
        let mut cmd;
        match conv {
            Converter::Oda(exe) => {
                // Folder in, folder out, R12 DXF like the original's converter (DW1).
                cmd = Command::new(exe);
                cmd.arg(&inp).arg(&out).arg("ACAD12").arg("DXF").arg("0").arg("1").arg("*.dwg");
            }
            Converter::LibreDwg(exe) => {
                cmd = Command::new(exe);
                cmd.arg("-y").arg("-o").arg(out.join("part.dxf")).arg(&src);
            }
        }
        run_with_timeout(cmd, Duration::from_secs(60))?;
        let dxf = find_dxf(&out).ok_or("it wrote no DXF")?;
        let text = std::fs::read(&dxf).map_err(|e| e.to_string())?;
        crate::dxf::read_in(&String::from_utf8_lossy(&text), unit_mm)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}
