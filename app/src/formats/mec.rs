//! Lantek Expert's own part and nesting files (.mec, also .mcs) — JOB-1.
//!
//! Rules (MECHANICS.md, "Lantek's own file format"):
//! - MF1/MF2: the first line `0 FILEMECLT <major>.<minor>` gives the version;
//!   above 7 the rest of the file is encrypted.
//! - MF4: version 8 — every following line is hex text of one Blowfish-CBC
//!   encrypted line (16-byte key, the chain starts at zero for each line,
//!   padded with spaces to 8 bytes).
//! - MF5: version 9 — everything after the first line is one AES-128-CBC
//!   block of binary data with PKCS#7 padding (a fixed key and IV); copies kept
//!   in memory hold the same as base64.
//! - MF6: once decrypted, the text is an ordinary version-7 file.
//! - MF7: this app holds no keys. They are found in the shop's own Lantek
//!   install, or read from a key file the user points to.
//! - MF8–MF14: the plain records (see `parse_plain`); MF15: millimetres.
//!
//! Geometry comes out as `dxf::Drawing` pieces, so joining, outline/holes and
//! layer roles work as for DXF.


use super::{Meta, ReadPart};
use crate::dxf::{Drawing, Piece};
use crate::nest::P;
use std::path::{Path, PathBuf};

mod cipher;

/// Name of the key file looked for next to the app's settings (MF7).
pub const KEY_FILE_NAME: &str = "lantek-keys.txt";

/// Keys for encrypted Lantek files. Each list may hold several candidates;
/// the one that turns the file into readable text is used (MF7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Keys {
    /// Keys for version-8 files (Blowfish, per line).
    pub line: Vec<Vec<u8>>,
    /// Keys for version-9 files (AES-128).
    pub file: Vec<[u8; 16]>,
    /// Starting chain values (IV) for version-9 files.
    pub iv: Vec<[u8; 16]>,
}

impl Keys {
    fn is_empty(&self) -> bool {
        self.line.is_empty() && self.file.is_empty() && self.iv.is_empty()
    }
}

/// Where the keys for encrypted files come from (tried in order, MF7).
#[derive(Debug, Clone, PartialEq)]
pub enum KeySource {
    /// The Lantek Expert install folder: the keys are found in its program files.
    Install(PathBuf),
    /// A text file with `line-key = <hex>`, `file-key = <hex>`, `file-iv = <hex>`.
    KeyFile(PathBuf),
    /// Keys given directly (tests).
    #[cfg_attr(not(test), allow(dead_code))]
    Keys(Keys),
}

/// Import options. The original has no settings for this import: it reads
/// the geometry as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Where to look for the keys of encrypted files.
    pub keys: Vec<KeySource>,
    /// A nesting file (several outer contours) gives one part per outer
    /// contour, with the holes inside it (MF14). Off: the whole file is one drawing.
    pub split_nest: bool,
    /// Parts of a nesting with the same shape (same position-free geometry)
    /// become one part with a quantity (MF14).
    pub merge_same: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            // The key file is quick to read; the install is searched only when it
            // has none that open the file.
            keys: vec![KeySource::KeyFile(default_key_file()), KeySource::Install(PathBuf::from(crate::settings::lantek_default_dir()))],
            split_nest: true,
            merge_same: true,
        }
    }
}

/// `%APPDATA%\nestpro\lantek-keys.txt` (`~/.config/nestpro/lantek-keys.txt`).
pub fn default_key_file() -> PathBuf {
    crate::settings::AppSettings::file().with_file_name(KEY_FILE_NAME)
}

// ------------------------------------------------------------------ keys

/// The program libraries of a Lantek install (every `.dll` in its folder, any
/// case), in name order: one of them holds the keys (MF7).
fn install_key_holders(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("dll")))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// 16-byte values a program builds with four 32-bit stores in a row
/// (`mov dword [reg], imm32` … `[reg+0xC]`) — how the install fills in its
/// key and IV. Every such value is a candidate; the file decides (MF7).
fn four_store_values(bin: &[u8]) -> Vec<[u8; 16]> {
    let mut out: Vec<[u8; 16]> = vec![];
    let n = bin.len();
    let mut i = 0;
    while i + 26 <= n {
        // optional REX.B prefix (r8–r15), then C7 /0
        let rex = bin[i] == 0x41;
        let s = i + rex as usize;
        if s + 6 > n || bin[s] != 0xC7 {
            i += 1;
            continue;
        }
        let m0 = bin[s + 1];
        // mod=00, reg=0, rm=base (not SIB 4, not RIP 5)
        if m0 > 7 || m0 == 4 || m0 == 5 {
            i += 1;
            continue;
        }
        let mut val = [0u8; 16];
        let mut p = s + 2;
        val[0..4].copy_from_slice(&bin[p..p + 4]);
        p += 4;
        let mut ok = true;
        for k in 1..4 {
            if rex {
                if p >= n || bin[p] != 0x41 {
                    ok = false;
                    break;
                }
                p += 1;
            }
            if p + 7 > n || bin[p] != 0xC7 || bin[p + 1] != 0x40 | m0 || bin[p + 2] != (4 * k) as u8 {
                ok = false;
                break;
            }
            val[4 * k..4 * k + 4].copy_from_slice(&bin[p + 3..p + 7]);
            p += 7;
        }
        if ok {
            if !out.contains(&val) {
                out.push(val);
            }
            i = p;
        } else {
            i += 1;
        }
    }
    out
}

/// Candidate keys from a Lantek install folder (MF7). The folder's libraries
/// are searched once per run of the app; later files use what was found.
pub fn keys_from_install(dir: &Path) -> Result<Keys, String> {
    use std::sync::{Mutex, OnceLock};
    static FOUND: OnceLock<Mutex<std::collections::HashMap<PathBuf, Result<Keys, String>>>> = OnceLock::new();
    let key = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let cache = FOUND.get_or_init(Default::default);
    if let Some(r) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return r;
    }
    let r = search_install(dir);
    if let Ok(mut c) = cache.lock() {
        c.insert(key, r.clone());
    }
    r
}

fn search_install(dir: &Path) -> Result<Keys, String> {
    let files = install_key_holders(dir);
    if files.is_empty() {
        return Err(format!("no Lantek install in {}", dir.display()));
    }
    let mut vals: Vec<[u8; 16]> = vec![];
    for f in &files {
        // A library that can't be read (locked, no access) is passed over.
        let Ok(bin) = std::fs::read(f) else { continue };
        for v in four_store_values(&bin) {
            if !vals.contains(&v) {
                vals.push(v);
            }
        }
    }
    if vals.is_empty() {
        return Err(format!("the keys were not found in the Lantek install in {} (a Lantek version this app doesn't know)", dir.display()));
    }
    Ok(Keys { line: vals.iter().map(|v| v.to_vec()).collect(), file: vals.clone(), iv: vals })
}

/// Keys from a key file: lines `line-key = <hex>`, `file-key = <hex>`,
/// `file-iv = <hex>`; `#` starts a comment.
pub fn keys_from_text(text: &str) -> Result<Keys, String> {
    let mut k = Keys::default();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (name, val) = line.split_once('=').ok_or_else(|| format!("line {}: expected name = hex value", n + 1))?;
        let bytes = cipher::from_hex(&val.trim().replace(' ', "")).ok_or_else(|| format!("line {}: the value is not hex", n + 1))?;
        let sixteen = || -> Result<[u8; 16], String> { bytes.as_slice().try_into().map_err(|_| format!("line {}: needs 16 bytes (32 hex digits)", n + 1)) };
        match name.trim().to_ascii_lowercase().as_str() {
            "line-key" => {
                if bytes.is_empty() || bytes.len() > 56 {
                    return Err(format!("line {}: the line key needs 1 to 56 bytes", n + 1));
                }
                k.line.push(bytes)
            }
            "file-key" => k.file.push(sixteen()?),
            "file-iv" => k.iv.push(sixteen()?),
            other => return Err(format!("line {}: unknown name '{other}' (use line-key, file-key or file-iv)", n + 1)),
        }
    }
    Ok(k)
}

pub fn keys_from_file(path: &Path) -> Result<Keys, String> {
    let text = std::fs::read_to_string(path).map_err(|_| format!("no key file at {}", path.display()))?;
    keys_from_text(&text).map_err(|e| format!("key file {}: {e}", path.display()))
}

/// The keys one source gives.
fn source_keys(s: &KeySource) -> Result<Keys, String> {
    match s {
        KeySource::Install(d) => keys_from_install(d),
        KeySource::KeyFile(f) => keys_from_file(f),
        KeySource::Keys(k) => Ok(k.clone()),
    }
}

// ------------------------------------------------------------------ decryption

/// Looks like the start of a plain Lantek file: a record number followed by
/// numbers only (the first record is always numeric, MF8–MF9).
fn plausible(b: &[u8]) -> bool {
    let t: &[u8] = {
        let mut e = b.len();
        while e > 0 && (b[e - 1] == b' ' || b[e - 1] == 0) {
            e -= 1;
        }
        &b[..e]
    };
    !t.is_empty() && t[0].is_ascii_digit() && t.iter().all(|&c| c.is_ascii_digit() || b" .-+eEA\t\r\n".contains(&c))
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

fn trim_pad(b: &[u8]) -> &[u8] {
    let mut e = b.len();
    while e > 0 && (b[e - 1] == b' ' || b[e - 1] == 0) {
        e -= 1;
    }
    &b[..e]
}

/// MF4: the encrypted lines of a version-8 file as bytes; a line that isn't
/// whole blocks of hex means the file is damaged (nothing is left out quietly).
fn v8_lines(lines: &[&[u8]]) -> Result<Vec<Vec<u8>>, usize> {
    lines
        .iter()
        .map(|l| latin1(l).trim().to_string())
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(n, l)| cipher::from_hex(&l).filter(|b| !b.is_empty() && b.len() % 8 == 0).ok_or(n + 2))
        .collect()
}

/// MF4: version 8, one Blowfish-encrypted hex line per plain line.
fn decrypt_v8(data: &[Vec<u8>], keys: &Keys) -> Option<String> {
    let first = data.first()?;
    if first.is_empty() || first.len() % 8 != 0 {
        return None;
    }
    for k in &keys.line {
        let Some(bf) = cipher::Blowfish::new(k) else { continue };
        if !plausible(&bf.decrypt_cbc([0; 8], first)) {
            continue;
        }
        let mut out = String::new();
        for l in data {
            if l.len() % 8 != 0 {
                return None;
            }
            out.push_str(&latin1(trim_pad(&bf.decrypt_cbc([0; 8], l))));
            out.push('\n');
        }
        return Some(out);
    }
    None
}

/// MF5: version 9, one AES-128-CBC block of binary data (or base64 of it).
fn decrypt_v9(body: &[u8], keys: &Keys) -> Option<String> {
    let try_one = |data: &[u8]| -> Option<String> {
        if data.len() < 16 || data.len() % 16 != 0 {
            return None;
        }
        let nb = data.len() / 16;
        let last: [u8; 16] = data[data.len() - 16..].try_into().ok()?;
        let first: [u8; 16] = data[..16].try_into().ok()?;
        for k in &keys.file {
            let aes = cipher::Aes128::new(k);
            // With two blocks or more the last one checks the key without the IV.
            if nb >= 2 {
                let prev: [u8; 16] = data[data.len() - 32..data.len() - 16].try_into().ok()?;
                let tail = aes.decrypt_cbc(&prev, &last);
                let Some(tail) = cipher::unpad(tail) else { continue };
                if !tail.iter().all(|&c| (0x20..0x7f).contains(&c) || c == b'\r' || c == b'\n' || c == b'\t' || c >= 0xa0) {
                    continue;
                }
            }
            let head = aes.decrypt_block(&first);
            for iv in &keys.iv {
                let p: Vec<u8> = head.iter().zip(iv).map(|(a, b)| a ^ b).collect();
                if !plausible(&p) {
                    continue;
                }
                if let Some(plain) = cipher::unpad(aes.decrypt_cbc(iv, data)) {
                    return Some(latin1(&plain));
                }
            }
        }
        None
    };
    try_one(body).or_else(|| {
        let b = cipher::from_base64(body)?;
        try_one(&b)
    })
}

/// The file's version and its plain text (records after the version line).
/// Encrypted files are decrypted with the keys from `opts` (MF4–MF7).
pub fn decrypt(bytes: &[u8], opts: &Options) -> Result<((u32, u32), String), String> {
    let nl = bytes.iter().position(|&c| c == b'\n').unwrap_or(bytes.len());
    let first = latin1(&bytes[..nl]);
    let Some((ma, mi)) = crate::progimport::lantek_file_version(first.trim()) else {
        // MF1: no version line — a version-7 file from its first line on.
        return Ok(((7, 0), latin1(bytes)));
    };
    let rest = if nl < bytes.len() { &bytes[nl + 1..] } else { &[][..] };
    if !crate::progimport::lantek_encrypted(ma) {
        return Ok(((ma, mi), latin1(rest)));
    }
    if ma > 9 {
        return Err(format!("This Lantek file is version {ma}.{mi}, newer than this app can read (8 and 9). Export the part as DXF from Lantek and open that."));
    }
    let fix = format!(
        "Lantek encrypts its files from version 8 on, and this app has no keys of its own. Point the Lantek folder (Settings, or LANTEK_DIR) at the Lantek Expert install on this PC (default {}), or put a key file at {} — or export the part as DXF from Lantek and open that.",
        crate::settings::LANTEK_DEFAULT_DIR,
        default_key_file().display()
    );
    // Each source in turn (a folder named twice is searched once); the first
    // whose keys open the file wins.
    let lines: Vec<&[u8]> = rest.split(|&c| c == b'\n').collect();
    let v8 = if ma == 8 {
        v8_lines(&lines).map_err(|n| format!("This Lantek file (version {ma}.{mi}) is damaged: line {n} is not encrypted data. Export the part again from Lantek, or as DXF."))?
    } else {
        vec![]
    };
    let (mut why, mut any) = (vec![], false);
    let mut seen: Vec<&KeySource> = vec![];
    for src in &opts.keys {
        if seen.contains(&src) {
            continue;
        }
        seen.push(src);
        let keys = match source_keys(src) {
            Ok(k) if !k.is_empty() => k,
            Ok(_) => continue,
            Err(e) => {
                why.push(e);
                continue;
            }
        };
        any = true;
        let plain = if ma == 8 { decrypt_v8(&v8, &keys) } else { decrypt_v9(rest, &keys) };
        if let Some(p) = plain {
            return Ok(((ma, mi), p));
        }
    }
    if !any {
        return Err(format!("This Lantek file (version {ma}.{mi}) is encrypted and no keys were found ({}). {fix}", why.join("; ")));
    }
    Err(format!("This Lantek file (version {ma}.{mi}) is encrypted and none of the keys found open it — the file may be damaged, or the keys belong to another Lantek version. {fix}"))
}

// ------------------------------------------------------------------ plain records

/// One drawn element as read (before joining).
#[derive(Debug, Clone)]
struct Elem {
    side: i64,
    pts: Vec<P>,
    closed: bool,
}

/// What a plain file holds.
#[derive(Debug, Default)]
struct Plain {
    elems: Vec<Elem>,
    leads: usize,
    stripped: usize,
    material: Option<String>,
    thickness: Option<f64>,
    names: Vec<String>,
    skipped: Vec<String>,
    any_side: bool,
    /// Arcs and circles with no radius (damaged): left out and named.
    no_radius: usize,
    /// Record types not read, for the notes.
    unknown: std::collections::BTreeSet<i64>,
    /// Points made so far; past the limit the file is refused whole.
    points: usize,
    over: bool,
}

/// Most elements and points one Lantek file may give (refused beyond). Real
/// nestings of curved parts hold over a million short lines.
const MAX_ELEMS: usize = 2_000_000;
const MAX_POINTS: usize = 5_000_000;

fn quoted(line: &str) -> Option<String> {
    let a = line.find('"')?;
    let b = line.rfind('"')?;
    (b > a).then(|| line[a + 1..b].to_string())
}

/// Numbers after the record's first `skip` fields; "A" (no value, MF13) or
/// anything else that is not a number stops the list.
fn numbers(t: &[&str], skip: usize, want: usize) -> Option<Vec<f64>> {
    // "NaN" and "inf" parse as numbers but are no coordinates: they end the list too.
    let v: Vec<f64> = t.iter().skip(skip).take(want).map_while(|s| s.parse::<f64>().ok().filter(|v| v.is_finite())).collect();
    (v.len() == want).then_some(v)
}

/// MF8–MF13: read the records of a plain (version 7, or decrypted) file.
fn parse_plain(text: &str) -> Plain {
    let mut pl = Plain::default();
    let mut side: i64 = 0;
    let mut state: i64 = 0;
    for line in text.lines() {
        if pl.over {
            break;
        }
        // Vertical tabs count as spaces, as does any other whitespace.
        let t: Vec<&str> = line.split_whitespace().collect();
        let Some(code) = t.first().and_then(|c| c.parse::<i64>().ok()) else { continue };
        match code {
            // MF8: 1 line, 2 arc, 3 circle: `<type> <id> <a> <b> <values…>`.
            1..=3 => {
                // MF10/MF11: attributes belong to this element only.
                let (side, state) = (std::mem::take(&mut side), std::mem::take(&mut state));
                let want = [4, 9, 3][(code - 1) as usize];
                let Some(v) = numbers(&t, 4, want) else {
                    pl.stripped += 1;
                    continue;
                };
                // MF11: an element marked 1 is a lead-in.
                if state == 1 {
                    pl.leads += 1;
                    continue;
                }
                let (pts, closed) = match code {
                    1 => (vec![[v[0], v[1]], [v[2], v[3]]], false),
                    2 => {
                        // centre, radius, start angle and signed sweep in
                        // degrees (negative = clockwise), then both end points.
                        let (c, r, a0, sw) = ([v[0], v[1]], v[2], v[3].to_radians(), v[4].to_radians());
                        if r <= 0.0 {
                            pl.no_radius += 1;
                            continue;
                        }
                        let full = sw.abs() >= std::f64::consts::TAU - 1e-9;
                        let mut pts = crate::dxf::arc_points(c, r, a0, if full { std::f64::consts::TAU.copysign(sw) } else { sw });
                        if full {
                            pts.pop();
                            (pts, true)
                        } else {
                            let n = pts.len() - 1;
                            pts[0] = [v[5], v[6]];
                            pts[n] = [v[7], v[8]];
                            (pts, false)
                        }
                    }
                    _ => {
                        if v[2] <= 0.0 {
                            pl.no_radius += 1;
                            continue;
                        }
                        let mut pts = crate::dxf::arc_points([v[0], v[1]], v[2], 0.0, std::f64::consts::TAU);
                        pts.pop();
                        (pts, true)
                    }
                };
                pl.points += pts.len();
                if pl.points > MAX_POINTS || pl.elems.len() >= MAX_ELEMS {
                    pl.over = true;
                    break;
                }
                pl.elems.push(Elem { side, pts, closed });
            }
            // MF10: contour side of the next element.
            16 => {
                side = t.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                pl.any_side = true;
            }
            // MF11: machining role of the next element.
            21 => state = t.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
            // MF9: bounding box, counters, end.
            0 | 90 | 91 | 999 => {}
            // MF9: placement matrix; identity in every file seen.
            92 => {
                if let Some(m) = numbers(&t, 1, 16) {
                    let id = (0..16).all(|i| (m[i] - if i % 5 == 0 { 1.0 } else { 0.0 }).abs() < 1e-9);
                    if !id && !pl.skipped.iter().any(|s| s.starts_with("placement")) {
                        pl.skipped.push("placement matrix (not applied)".into());
                    }
                }
            }
            // MF12: contour bookkeeping (copies of the first and last element,
            // member lists) — nothing new to draw.
            33 | 81 => {}
            61..=63 | 71..=73 => {
                side = 0;
                state = 0;
            }
            // MF13: machining and job data; a few give part data.
            31 => {
                if t.get(1) == Some(&"2") {
                    match t.get(2).copied() {
                        Some("1137") => pl.material = quoted(line).filter(|s| !s.is_empty()),
                        Some("1090") => pl.thickness = quoted(line).and_then(|s| s.split(',').nth(2).and_then(|x| x.trim().parse().ok())).filter(|&x: &f64| x > 0.0),
                        Some("1145") => {
                            if let Some(n) = quoted(line).filter(|s| !s.is_empty()) {
                                pl.names.push(n);
                            }
                        }
                        _ => {}
                    }
                }
            }
            32 | 38..=60 => {}
            other => {
                pl.unknown.insert(other);
            }
        }
    }
    // The record types not read, in the notes: the first 20 by number, then a
    // count of the rest.
    let n = pl.unknown.len();
    let listed: Vec<String> = pl.unknown.iter().take(20).map(|r| format!("record {r}")).collect();
    pl.skipped.extend(listed);
    if n > 20 {
        pl.skipped.push(format!("{} other record types", n - 20));
    }
    pl
}

/// Layer name for a contour side (MF10).
fn layer_for(side: i64, any_side: bool) -> String {
    match side {
        _ if !any_side => "Cut".into(),
        2 => "Cut – outer".into(),
        1 => "Cut – inner".into(),
        0 => "Cut".into(),
        s => format!("Cut – side {s}"),
    }
}

fn close_pt(a: P, b: P) -> bool {
    (a[0] - b[0]).abs() <= crate::dxf::JOIN_TOL && (a[1] - b[1]).abs() <= crate::dxf::JOIN_TOL
}

/// Joins elements end to end into closed loops: (points, element indices).
fn closed_loops(el: &[Elem]) -> Result<Vec<(Vec<P>, Vec<usize>)>, String> {
    let mut out = vec![];
    let mut open: Vec<usize> = vec![];
    for (i, e) in el.iter().enumerate() {
        if e.closed {
            out.push((e.pts.clone(), vec![i]));
        } else if e.pts.len() >= 2 {
            open.push(i);
        }
    }
    // End points filed by grid cell (one join tolerance wide): finding the next
    // piece looks in the 3 × 3 cells around an end, so joining stays in step
    // with the number of pieces. The lowest-numbered match is taken, as before.
    let mut ends = crate::dxf::EndGrid::new(crate::dxf::JOIN_TOL);
    for &j in &open {
        ends.add(el[j].pts[0], j);
        ends.add(*el[j].pts.last().unwrap(), j);
    }
    let mut used = vec![false; el.len()];
    for &s in &open {
        if used[s] {
            continue;
        }
        used[s] = true;
        let mut chain = el[s].pts.clone();
        let mut members = vec![s];
        loop {
            let end = *chain.last().unwrap();
            if chain.len() > 2 && close_pt(end, chain[0]) {
                chain.pop();
                out.push((chain, members));
                break;
            }
            let next = ends.first(end, |j| !used[j], |j| close_pt(el[j].pts[0], end) || close_pt(*el[j].pts.last().unwrap(), end));
            if ends.work > crate::dxf::JOIN_WORK {
                return Err(crate::dxf::join_too_complex());
            }
            let Some(j) = next else { break };
            used[j] = true;
            members.push(j);
            let mut p = el[j].pts.clone();
            if !close_pt(p[0], end) {
                p.reverse();
            }
            chain.extend(p.into_iter().skip(1));
        }
    }
    Ok(out)
}

/// Position-free shape key of a group of elements (for merging equal parts):
/// each element's own points in order (either direction), the elements sorted.
/// Keeping each element whole keeps which points are joined to which, so two
/// shapes with the same corners but different edges never count as equal.
fn shape_key(el: &[&Elem]) -> Vec<Vec<(i64, i64)>> {
    let (mut ox, mut oy) = (f64::MAX, f64::MAX);
    for e in el {
        for p in &e.pts {
            ox = ox.min(p[0]);
            oy = oy.min(p[1]);
        }
    }
    let mut k: Vec<Vec<(i64, i64)>> = el
        .iter()
        .map(|e| {
            let fwd: Vec<(i64, i64)> = e.pts.iter().map(|p| (((p[0] - ox) * 20.0).round() as i64, ((p[1] - oy) * 20.0).round() as i64)).collect();
            let rev: Vec<(i64, i64)> = fwd.iter().rev().copied().collect();
            fwd.min(rev)
        })
        .collect();
    k.sort_unstable();
    k
}

/// MF14: one group of element indices per part. A file with one outer
/// contour (or with splitting off) is one group.
/// Loops' boxes filed by grid cell, for finding the loops that may hold a point.
/// A loop over many cells goes on a short list that is always looked at.
struct BoxGrid {
    lo: [f64; 2],
    cell: f64,
    map: std::collections::HashMap<(i64, i64), Vec<usize>>,
    big: Vec<usize>,
    boxes: Vec<[f64; 4]>,
}

impl BoxGrid {
    fn new(loops: &[&[P]]) -> BoxGrid {
        // (With no loops there is nothing to file.)
        let boxes: Vec<[f64; 4]> = loops
            .iter()
            .map(|l| l.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]))
            .collect();
        let all = boxes.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
        let side = ((all[2] - all[0]).max(all[3] - all[1])).max(1e-6);
        let cell = side / (loops.len() as f64).sqrt().clamp(1.0, 1024.0);
        let mut g = BoxGrid { lo: [all[0], all[1]], cell, map: Default::default(), big: vec![], boxes };
        for (k, b) in g.boxes.iter().enumerate() {
            let (x0, y0, x1, y1) = (g.kx(b[0]), g.ky(b[1]), g.kx(b[2]), g.ky(b[3]));
            if (x1 - x0 + 1) * (y1 - y0 + 1) > 64 {
                g.big.push(k);
            } else {
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        g.map.entry((x, y)).or_default().push(k);
                    }
                }
            }
        }
        g
    }
    fn kx(&self, v: f64) -> i64 {
        ((v - self.lo[0]) / self.cell).floor() as i64
    }
    fn ky(&self, v: f64) -> i64 {
        ((v - self.lo[1]) / self.cell).floor() as i64
    }
    /// Loops whose box holds `p`.
    fn at(&self, p: P) -> impl Iterator<Item = usize> + '_ {
        let cell = (self.kx(p[0]), self.ky(p[1]));
        self.map.get(&cell).into_iter().flatten().chain(self.big.iter()).copied().filter(move |&k| {
            let b = self.boxes[k];
            p[0] >= b[0] && p[0] <= b[2] && p[1] >= b[1] && p[1] <= b[3]
        })
    }
}

/// Most candidate checks splitting one nesting may take; past that the file is
/// refused whole rather than holding up the window.
const SPLIT_WORK: usize = 200_000_000;

fn split_parts(pl: &Plain, opts: &Options) -> Result<Vec<Vec<usize>>, String> {
    let all: Vec<usize> = (0..pl.elems.len()).collect();
    if !opts.split_nest {
        return Ok(vec![all]);
    }
    let loops = closed_loops(&pl.elems)?;
    let areas: Vec<f64> = loops.iter().map(|l| crate::dxf::signed_area(&l.0).abs()).collect();
    let index: Vec<crate::dxf::PolyIndex> = loops.iter().map(|l| crate::dxf::PolyIndex::new(&l.0)).collect();
    let grid = BoxGrid::new(&loops.iter().map(|l| l.0.as_slice()).collect::<Vec<_>>());
    let work = std::cell::Cell::new(0usize);
    let count = |n: usize| work.set(work.get() + n);
    // Point-in tests count the edges they look at too.
    let contains = |l: usize, p: P| {
        let mut w = 0;
        let r = index[l].contains_counted(p, &mut w);
        count(w);
        r
    };
    // Outer contours: marked outer (MF10), or else every loop that sits inside
    // no other loop (closed shapes inside a part, such as engraved digits,
    // stay with that part).
    // A loop with no area (a line drawn twice over itself) is never a part.
    let outers: Vec<usize> = (0..loops.len())
        // Once over the work budget nothing more is searched (refused below).
        .filter(|&i| work.get() <= SPLIT_WORK && areas[i] > crate::dxf::JOIN_TOL * crate::dxf::JOIN_TOL)
        .filter(|&i| {
            if pl.any_side {
                let outer = loops[i].1.iter().filter(|&&e| pl.elems[e].side == 2).count();
                outer * 2 > loops[i].1.len()
            } else {
                !grid.at(loops[i].0[0]).any(|j| {
                    count(1);
                    work.get() > SPLIT_WORK || (j != i && areas[j] > areas[i] && contains(j, loops[i].0[0]))
                })
            }
        })
        .collect();
    if work.get() > SPLIT_WORK {
        return Err(too_complex());
    }
    if outers.len() <= 1 {
        return Ok(vec![all]);
    }
    let mut groups: Vec<Vec<usize>> = vec![vec![]; outers.len()];
    let mut owner = vec![usize::MAX; pl.elems.len()];
    for (g, &o) in outers.iter().enumerate() {
        for &e in &loops[o].1 {
            owner[e] = g;
        }
    }
    let largest = (0..outers.len()).max_by(|&a, &b| areas[outers[a]].total_cmp(&areas[outers[b]])).unwrap_or(0);
    // Which outer contour each loop is, to find the smallest one holding a point.
    let mut group_of = vec![usize::MAX; loops.len()];
    for (g, &o) in outers.iter().enumerate() {
        group_of[o] = g;
    }
    for (e, el) in pl.elems.iter().enumerate() {
        if owner[e] == usize::MAX {
            let p = el.pts[el.pts.len() / 2];
            owner[e] = grid
                .at(p)
                .take_while(|_| work.get() <= SPLIT_WORK)
                .filter(|&l| {
                    count(1);
                    group_of[l] != usize::MAX && contains(l, p)
                })
                .map(|l| group_of[l])
                .min_by(|&a, &b| areas[outers[a]].total_cmp(&areas[outers[b]]))
                .unwrap_or(largest);
            if work.get() > SPLIT_WORK {
                return Err(too_complex());
            }
        }
        groups[owner[e]].push(e);
    }
    Ok(groups)
}

fn too_complex() -> String {
    "This Lantek nesting is too complex to split into parts in reasonable time (very many contours on top of each other). Nothing was imported. Export the parts as DXF from Lantek.".into()
}

/// Reads a Lantek .mec / .mcs file: one part, or one per outer contour of a
/// nesting (MF14). Encrypted files need keys (MF7).
pub fn read(bytes: &[u8], opts: &Options) -> Result<Vec<ReadPart>, String> {
    let ((ma, mi), text) = decrypt(bytes, opts)?;
    let pl = parse_plain(&text);
    if pl.over {
        return Err(format!(
            "This Lantek file is bigger than the import can take whole (more than {} elements or {} million points). Nothing was imported, so no contour goes missing. Export the parts as DXF from Lantek.",
            MAX_ELEMS,
            MAX_POINTS / 1_000_000
        ));
    }
    if pl.elems.is_empty() {
        if pl.stripped > 0 {
            return Err(format!(
                "This Lantek file (version {ma}.{mi}) lists {} drawing elements but their coordinates have been taken out (Lantek leaves copies like this in its log and temporary folders). Open the part's own file from the Lantek database instead, or export it as DXF.",
                pl.stripped
            ));
        }
        return Err(if pl.leads > 0 { "This Lantek file holds only lead-ins, no part contour.".into() } else { "This Lantek file holds no lines, arcs or circles.".into() });
    }
    let groups = split_parts(&pl, opts)?;
    let nest = groups.len() > 1;
    let mut parts: Vec<(Vec<usize>, Vec<Vec<(i64, i64)>>, u32)> = vec![];
    // Equal shapes found through a map of their keys (not by comparing each
    // with every part so far).
    let mut seen: std::collections::HashMap<Vec<Vec<(i64, i64)>>, usize> = std::collections::HashMap::new();
    for g in groups {
        let key = if nest && opts.merge_same { shape_key(&g.iter().map(|&e| &pl.elems[e]).collect::<Vec<_>>()) } else { vec![] };
        match seen.get(&key).filter(|_| !key.is_empty()) {
            Some(&i) => parts[i].2 += 1,
            None => {
                if !key.is_empty() {
                    seen.insert(key.clone(), parts.len());
                }
                parts.push((g, key, 1));
            }
        }
    }
    let mut skipped = pl.skipped.clone();
    if pl.leads > 0 {
        skipped.push(format!("lead-ins ({}, left out)", pl.leads));
    }
    if pl.stripped > 0 {
        skipped.push(format!("elements without coordinates ({})", pl.stripped));
    }
    if pl.no_radius > 0 {
        skipped.push(format!("arcs or circles without a radius ({}, damaged)", pl.no_radius));
    }
    let name = (pl.names.len() == 1 || (!pl.names.is_empty() && pl.names.iter().all(|n| n == &pl.names[0]))).then(|| pl.names[0].clone());
    Ok(parts
        .into_iter()
        .map(|(g, _, qty)| {
            let mut d = Drawing::default();
            for &e in &g {
                let el = &pl.elems[e];
                d.pieces.push(Piece { layer: layer_for(el.side, pl.any_side), pts: el.pts.clone(), closed: el.closed });
            }
            d.report.entities = d.pieces.len();
            d.report.skipped = skipped.clone();
            ReadPart {
                drawing: d,
                meta: Meta { name: if nest { None } else { name.clone() }, material: pl.material.clone(), thickness: pl.thickness, qty: nest.then_some(qty) },
                mark_layers: vec![],
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const PLAIN: &str = "0 FILEMECLT 7.00\n\
90 0 0 100 50\n\
91 0 3 1 1 0 0 0 0 0\n\
92 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1\n\
1 0 0 0 0 0 100 0\n\
1 1 0 0 100 0 100 50\n\
2 0 0 0 50 50 50 0 180 100 50 0 50\n\
1 2 0 0 0 50 0 0\n\
3 0 0 0 30 20 5\n\
999\n";

    fn opts_with(keys: Keys) -> Options {
        Options { keys: vec![KeySource::Keys(keys)], ..Options::default() }
    }

    #[test]
    fn plain_file_reads_lines_arcs_and_circles() {
        let parts = read(PLAIN.as_bytes(), &Options { keys: vec![], ..Options::default() }).unwrap();
        assert_eq!(parts.len(), 1);
        let d = &parts[0].drawing;
        assert_eq!(d.pieces.len(), 5);
        // MF8: the arc goes counter-clockwise from 0° over 180° (a bulge on top).
        let top = d.pieces[2].pts.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
        assert!((top - 100.0).abs() < 0.2, "{top}");
        let (g, _) = crate::dxf::build(d, &BTreeMap::new()).unwrap();
        assert_eq!(g.holes.len(), 1);
        assert!((g.h - 100.0).abs() < 0.2);
    }

    #[test]
    fn clockwise_arcs_have_a_negative_sweep() {
        let t = "0 FILEMECLT 7.00\n2 0 0 0 0 0 10 90 -90 0 10 10 0\n999\n";
        let p = &read(t.as_bytes(), &Options::default()).unwrap()[0].drawing.pieces[0].pts;
        let mid = p[p.len() / 2];
        assert!(mid[0] > 6.0 && mid[1] > 6.0, "{mid:?}");
    }

    #[test]
    fn lead_ins_and_bookkeeping_are_left_out_and_part_data_read() {
        let t = "0 FILEMECLT 7.3\n21 1\n16 2 0\n1 9 0 0 -2 0 0 0\n21 3\n16 2 0\n1 0 0 0 0 0 10 0\n1 1 0 0 10 0 10 10\n1 2 0 0 10 10 0 10\n21 2\n1 3 0 0 0 10 0 0\n\
16 2 0\n61 4 0 0 0 0 10 0\n16 2 0\n71 4 0 0 0 0 10 0\n33 1 0\n81\n\
31 2 1137 100 A A A \"Demo steel\"\n31 2 1090 4 1 2 3 \"3000,1500,6.35,1,1,0\"\n999\n";
        let parts = read(t.as_bytes(), &Options::default()).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].drawing.pieces.len(), 4);
        assert!(parts[0].drawing.report.skipped.iter().any(|s| s.starts_with("lead-ins (1")));
        assert_eq!(parts[0].meta.material.as_deref(), Some("Demo steel"));
        assert_eq!(parts[0].meta.thickness, Some(6.35));
        assert!(parts[0].drawing.layers().contains_key("Cut – outer"));
    }

    #[test]
    fn attributes_belong_to_the_next_element_only() {
        let t = "21 1\n1 0 0 0 -2 0 0 0\n1 1 0 0 0 0 10 0\n999\n";
        let p = read(t.as_bytes(), &Options::default()).unwrap();
        assert_eq!(p[0].drawing.pieces.len(), 1);
        assert_eq!(p[0].drawing.pieces[0].pts[1], [10.0, 0.0]);
    }

    #[test]
    fn same_corners_with_other_edges_are_not_the_same_shape() {
        // Five corners, the inner one joined to different outer edges.
        let el = |pts: &[[f64; 2]]| -> Vec<Elem> { pts.windows(2).map(|w| Elem { side: 0, pts: vec![w[0], w[1]], closed: false }).chain([Elem { side: 0, pts: vec![*pts.last().unwrap(), pts[0]], closed: false }]).collect() };
        let a = el(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [5.0, 5.0], [0.0, 10.0]]);
        let b = el(&[[0.0, 0.0], [5.0, 5.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
        let ka = shape_key(&a.iter().collect::<Vec<_>>());
        let kb = shape_key(&b.iter().collect::<Vec<_>>());
        assert_ne!(ka, kb);
        // The same shape elsewhere, drawn the other way round, is equal.
        let c: Vec<Elem> = a.iter().rev().map(|e| Elem { side: 0, pts: e.pts.iter().rev().map(|p| [p[0] + 100.0, p[1] + 50.0]).collect(), closed: false }).collect();
        assert_eq!(ka, shape_key(&c.iter().collect::<Vec<_>>()));
    }

    #[test]
    fn a_nesting_splits_into_parts_and_merges_equal_ones() {
        let sq = |x: f64, y: f64| format!("16 2 0\n1 0 0 0 {x} {y} {} {y}\n16 2 0\n1 0 0 0 {} {y} {} {}\n16 2 0\n1 0 0 0 {} {} {x} {}\n16 2 0\n1 0 0 0 {x} {} {x} {y}\n", x + 10.0, x + 10.0, x + 10.0, y + 10.0, x + 10.0, y + 10.0, y + 10.0, y + 10.0);
        let mut t = String::from("0 FILEMECLT 7.3\n");
        t += &sq(0.0, 0.0);
        t += "16 1 0\n3 0 0 0 5 5 2\n";
        t += &sq(50.0, 0.0);
        t += "16 1 0\n3 0 0 0 55 5 2\n";
        t += "16 2 0\n3 0 0 0 100 100 20\n999\n";
        let parts = read(t.as_bytes(), &Options::default()).unwrap();
        assert_eq!(parts.len(), 2);
        let two = parts.iter().find(|p| p.meta.qty == Some(2)).unwrap();
        assert_eq!(two.drawing.pieces.len(), 5);
        assert!(parts.iter().any(|p| p.meta.qty == Some(1) && p.drawing.pieces.len() == 1));
        let whole = read(t.as_bytes(), &Options { split_nest: false, ..Options::default() }).unwrap();
        assert_eq!(whole.len(), 1);
    }

    #[test]
    fn stripped_copies_say_so() {
        let t = "0 FILEMECLT 7.1 \n90 \x0b\x0b\x0b\x0b\n1 0 0 0 \x0b\x0b\x0b\x0b\n2 0 0 0 \x0b\x0b\x0b\x0b\x0b\x0b\x0b\x0b\x0b\n999\n";
        let e = read(t.as_bytes(), &Options::default()).unwrap_err();
        assert!(e.contains("coordinates have been taken out"), "{e}");
    }

    // Made-up keys: these are not Lantek's.
    const LINE_KEY: &[u8; 16] = b"DEMO-KEY-0123456";
    const FILE_KEY: [u8; 16] = *b"made-up-aes-key!";
    const FILE_IV: [u8; 16] = *b"made-up-chain-iv";

    fn encrypt_v8(plain: &str, key: &[u8]) -> Vec<u8> {
        let bf = cipher::Blowfish::new(key).unwrap();
        let mut out = String::from("0 FILEMECLT 8.1\r\n");
        for l in plain.lines().skip(1) {
            let mut b = l.as_bytes().to_vec();
            while b.len() % 8 != 0 {
                b.push(b' ');
            }
            for c in bf.encrypt_cbc([0; 8], &b) {
                out.push_str(&format!("{c:02X}"));
            }
            out.push_str("\r\n");
        }
        out.into_bytes()
    }

    fn encrypt_v9(plain: &str) -> Vec<u8> {
        let body: String = plain.lines().skip(1).map(|l| format!("{l}\n")).collect();
        let mut out = b"0 FILEMECLT 9.3\n".to_vec();
        out.extend(cipher::Aes128::new(&FILE_KEY).encrypt_cbc_padded(&FILE_IV, body.as_bytes()));
        out
    }

    #[test]
    fn version_8_round_trip_with_a_made_up_key() {
        let enc = encrypt_v8(PLAIN, LINE_KEY);
        // A wrong candidate first: the file picks the key that reads.
        let keys = Keys { line: vec![b"wrong-key-000000".to_vec(), LINE_KEY.to_vec()], ..Keys::default() };
        let ((ma, mi), text) = decrypt(&enc, &opts_with(keys.clone())).unwrap();
        assert_eq!((ma, mi), (8, 1));
        assert_eq!(text.lines().collect::<Vec<_>>(), PLAIN.lines().skip(1).collect::<Vec<_>>());
        assert_eq!(read(&enc, &opts_with(keys)).unwrap()[0].drawing.pieces.len(), 5);
    }

    #[test]
    fn a_damaged_encrypted_line_is_reported_not_dropped() {
        // MF4: one line's hex spoilt: the file is refused as damaged, so no hole
        // can go missing without a word.
        let enc = String::from_utf8(encrypt_v8(PLAIN, LINE_KEY)).unwrap();
        let mut lines: Vec<String> = enc.lines().map(String::from).collect();
        lines[3] = "GG".into();
        let bad = lines.join("\r\n");
        let keys = Keys { line: vec![LINE_KEY.to_vec()], ..Keys::default() };
        let e = read(bad.as_bytes(), &opts_with(keys)).unwrap_err();
        assert!(e.contains("damaged") && e.contains("line 4"), "{e}");
    }

    #[test]
    fn rectangles_all_on_top_of_each_other_are_refused_quickly() {
        // 4 000 identical rectangles in one place: the split compares each with
        // every other, within its work budget (past it the file is refused).
        let mut t = String::from("0 FILEMECLT 7.00\n");
        for k in 0..4_000 {
            let o = k as f64 * 1e-9;
            for (a, b) in [([o, 0.0], [10.0 + o, 0.0]), ([10.0 + o, 0.0], [10.0 + o, 10.0]), ([10.0 + o, 10.0], [o, 10.0]), ([o, 10.0], [o, 0.0])] {
                t.push_str(&format!("1 0 0 0 {} {} {} {}\n", a[0], a[1], b[0], b[1]));
            }
        }
        t.push_str("999\n");
        let t0 = std::time::Instant::now();
        let r = read(t.as_bytes(), &Options::default());
        assert!(t0.elapsed().as_secs_f64() < 30.0, "{:?}", t0.elapsed());
        // Refused, or the equal shapes merged: never thousands of parts.
        assert!(r.as_ref().map_or(true, |p| p.len() < 100), "{:?}", r.map(|p| p.len()));
    }

    #[test]
    fn a_crowded_nesting_splits_in_step_with_its_size() {
        // 3 000 equal separate squares plus 3 000 copies of one line on top of
        // each other: joining, splitting and merging stay quick.
        let mut t = String::from("0 FILEMECLT 7.00\n");
        for k in 0..3_000 {
            let (x, y) = ((k % 60) as f64 * 20.0, (k / 60) as f64 * 20.0);
            for (a, b) in [([x, y], [x + 10.0, y]), ([x + 10.0, y], [x + 10.0, y + 10.0]), ([x + 10.0, y + 10.0], [x, y + 10.0]), ([x, y + 10.0], [x, y])] {
                t.push_str(&format!("1 0 0 0 {} {} {} {}\n", a[0], a[1], b[0], b[1]));
            }
        }
        for _ in 0..3_000 {
            t.push_str("1 0 0 0 5000 0 5010 0\n");
        }
        t.push_str("999\n");
        let t0 = std::time::Instant::now();
        let parts = read(t.as_bytes(), &Options::default()).unwrap();
        // The loose lines go with one square, so that one is a part of its own.
        assert_eq!(parts.len(), 2);
        assert_eq!(parts.iter().map(|p| p.meta.qty.unwrap()).sum::<u32>(), 3_000);
        assert!(t0.elapsed().as_secs_f64() < 20.0, "{:?}", t0.elapsed());
    }

    #[test]
    fn a_hole_without_a_radius_is_named() {
        let t = "0 FILEMECLT 7.00\n3 0 0 0 50 50 40\n3 1 0 0 50 50 0\n999\n";
        let parts = read(t.as_bytes(), &Options::default()).unwrap();
        assert!(parts[0].drawing.report.skipped.iter().any(|s| s.contains("without a radius (1")), "{:?}", parts[0].drawing.report.skipped);
    }

    #[test]
    fn values_that_are_not_numbers_are_not_coordinates() {
        // A circle with radius NaN next to a real one: the NaN one is left out
        // and the part still builds.
        let t = "0 FILEMECLT 7.00\n16 2\n3 0 0 0 50 50 40\n16 1\n3 1 0 0 50 50 NaN\n999\n";
        let parts = read(t.as_bytes(), &Options::default()).unwrap();
        for p in &parts {
            assert!(p.drawing.pieces.iter().all(|pc| pc.pts.iter().all(|q| q[0].is_finite() && q[1].is_finite())));
            crate::dxf::build(&p.drawing, &BTreeMap::new()).unwrap();
        }
    }

    #[test]
    fn version_9_round_trip_with_a_made_up_key() {
        let enc = encrypt_v9(PLAIN);
        let keys = Keys { file: vec![*b"another-aes-key!", FILE_KEY], iv: vec![FILE_KEY, FILE_IV], ..Keys::default() };
        let (_, text) = decrypt(&enc, &opts_with(keys.clone())).unwrap();
        assert!(text.starts_with("90 0 0 100 50\n"), "{text:?}");
        assert_eq!(read(&enc, &opts_with(keys.clone())).unwrap()[0].drawing.pieces.len(), 5);
        // The same data as base64 (how in-memory copies keep it).
        let b64 = {
            let raw = &enc[16..];
            const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for ch in raw.chunks(3) {
                let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
                for k in 0..4 {
                    s.push(if k <= ch.len() { A[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
                }
            }
            s
        };
        let enc64 = [b"0 FILEMECLT 9.3\n".to_vec(), b64.into_bytes()].concat();
        assert!(decrypt(&enc64, &opts_with(keys)).is_ok());
    }

    #[test]
    fn missing_keys_explain_how_to_fix_it() {
        let enc = encrypt_v8(PLAIN, LINE_KEY);
        let o = Options { keys: vec![KeySource::Install(PathBuf::from("/no/such/lantek")), KeySource::KeyFile(PathBuf::from("/no/such/keys.txt"))], ..Options::default() };
        let e = decrypt(&enc, &o).unwrap_err();
        assert!(e.contains("encrypted") && e.contains("key file") && e.contains("DXF"), "{e}");
        let wrong = Keys { line: vec![b"nope".to_vec()], ..Keys::default() };
        assert!(decrypt(&enc, &opts_with(wrong)).unwrap_err().contains("none of the keys"));
    }

    #[test]
    fn key_file_and_install_search() {
        let k = keys_from_text("# demo\nline-key = 44454D4F\nfile-key = 000102030405060708090a0b0c0d0e0f\nfile-iv = 0F0E0D0C0B0A09080706050403020100\n").unwrap();
        assert_eq!(k.line, vec![b"DEMO".to_vec()]);
        assert_eq!(k.file[0][15], 15);
        assert!(keys_from_text("file-key = 0011").is_err());
        assert!(keys_from_text("colour = 00").is_err());
        // Four 32-bit stores through one register give a candidate (made-up bytes).
        let mut bin = vec![0x90u8; 8];
        bin.extend([0xC7, 0x01, 1, 2, 3, 4, 0xC7, 0x41, 4, 5, 6, 7, 8, 0xC7, 0x41, 8, 9, 10, 11, 12, 0xC7, 0x41, 12, 13, 14, 15, 16, 0xC3]);
        bin.extend([0x41, 0xC7, 0x00, 1, 1, 1, 1, 0x41, 0xC7, 0x40, 4, 2, 2, 2, 2, 0x41, 0xC7, 0x40, 8, 3, 3, 3, 3, 0x41, 0xC7, 0x40, 12, 4, 4, 4, 4]);
        let v = four_store_values(&bin);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], std::array::from_fn(|i| i as u8 + 1));
        assert_eq!(v[1][12], 4);
    }

    /// The sample parts shipped with Lantek, read with the keys of the
    /// install in LANTEK_DIR (skipped when there is none).
    #[test]
    fn lantek_sample_parts_read_when_installed() {
        let dir = PathBuf::from(crate::settings::lantek_default_dir());
        let samples = dir.join("Samples").join("MEC");
        let Ok(list) = std::fs::read_dir(&samples) else { return };
        let o = Options { keys: vec![KeySource::Install(dir.clone())], ..Options::default() };
        let mut n = 0;
        for f in list.flatten().map(|e| e.path()) {
            if !f.extension().is_some_and(|e| e.eq_ignore_ascii_case("mec")) {
                continue;
            }
            let parts = read(&std::fs::read(&f).unwrap(), &o).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
            for p in &parts {
                let (g, rep) = crate::dxf::build(&p.drawing, &BTreeMap::new()).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
                assert!(g.outline.len() >= 3 && rep.open_pieces == 0, "{}", f.display());
            }
            n += 1;
        }
        assert!(n > 0);
    }
}
