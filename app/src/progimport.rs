//! Open a nesting from the machine programs written for the BLM LC5 (the
//! PstAdi06 / Siemens 840D set: `<NEST>.zip` with the sheet .SPF, one .SPF
//! per part and the list, plus `<NEST>.XML`) — Lantek's or this app's — and
//! turn it back into an editable nesting: each part subprogram becomes a part
//! (its cut contours, arcs included, and marking lines), each `ORIG` + `CALL`
//! a placed part, the header gives sheet, material and thickness, the XML
//! gives the part names, and a cut-off (FINAL_CUT) becomes a remnant line.

use crate::dxf::CustomGeo;
use crate::nest::P;

/// Arcs are rebuilt with a point at least every this many degrees.
pub const ARC_STEP_DEG: f64 = 5.0;
/// Path ends closer than this (mm) close the contour.
pub const CLOSE_TOL: f64 = 0.05;

/// A nesting read from machine programs.
#[derive(Debug, Default)]
pub struct Imported {
    pub name: String,
    pub job: String,
    pub sheet_w: f64,
    pub sheet_h: f64,
    pub thickness: f64,
    /// Material name in the header (e.g. "STAINLESS").
    pub material: String,
    /// The gas the sheet program's header names, as written: its `;IDGAS=`
    /// number and `;NOMEGAS=` name (GS2), both empty for no gas. None when
    /// the header has no gas lines.
    pub gas: Option<(String, String)>,
    /// The gas the XML job sheet names, as written: `GasId` and `GasName`.
    /// None without an XML or without those tags.
    pub xml_gas: Option<(String, String)>,
    /// (part name, subprogram, geometry)
    pub parts: Vec<(String, String, CustomGeo)>,
    /// (part index, x, y) of the part's lower-left corner on the sheet.
    pub placed: Vec<(usize, f64, f64)>,
    /// Open cut paths in sheet coordinates (cut-offs, free cut lines).
    pub cuts: Vec<Vec<P>>,
    pub notes: Vec<String>,
}

/// Read the files in a zip (stored or deflated), via the central directory.
pub fn read_zip(b: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let u16_at = |o: usize| b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize);
    let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize);
    let eocd = (0..b.len().saturating_sub(21)).rev().find(|&o| u32_at(o) == Some(0x0605_4b50)).ok_or("not a zip file")?;
    let n = u16_at(eocd + 10).ok_or("bad zip")?;
    let mut at = u32_at(eocd + 16).ok_or("bad zip")?;
    let mut out = vec![];
    for _ in 0..n {
        if u32_at(at) != Some(0x0201_4b50) {
            return Err("bad zip directory".into());
        }
        let method = u16_at(at + 10).ok_or("bad zip")?;
        let csize = u32_at(at + 20).ok_or("bad zip")?;
        let (nl, xl, cl) = (u16_at(at + 28).ok_or("bad zip")?, u16_at(at + 30).ok_or("bad zip")?, u16_at(at + 32).ok_or("bad zip")?);
        let local = u32_at(at + 42).ok_or("bad zip")?;
        let name = String::from_utf8_lossy(b.get(at + 46..at + 46 + nl).ok_or("bad zip")?).to_string();
        at += 46 + nl + xl + cl;
        let (lnl, lxl) = (u16_at(local + 26).ok_or("bad zip")?, u16_at(local + 28).ok_or("bad zip")?);
        let start = local + 30 + lnl + lxl;
        let data = b.get(start..start + csize).ok_or("bad zip entry")?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(data).map_err(|_| format!("can't unpack {name}"))?,
            m => return Err(format!("{name}: packing method {m} isn't supported")),
        };
        out.push((name, bytes));
    }
    Ok(out)
}

fn nums(args: &str) -> Vec<f64> {
    args.split(',').map(|s| s.trim().parse::<f64>().unwrap_or(0.0)).collect()
}

fn inside(s: &str, open: &str) -> Option<String> {
    let a = s.find(open)? + open.len();
    let b = s[a..].find(')')? + a;
    Some(s[a..b].to_string())
}

/// The X/Y/I/J words of a move line.
fn words(l: &str) -> (Option<f64>, Option<f64>, f64, f64) {
    let (mut x, mut y, mut i, mut j) = (None, None, 0.0, 0.0);
    for w in l.split_whitespace() {
        let (k, v) = w.split_at(1);
        let v = v.trim_end_matches('.').parse::<f64>().ok().or_else(|| v.parse::<f64>().ok());
        match (k, v) {
            ("X", Some(v)) => x = Some(v),
            ("Y", Some(v)) => y = Some(v),
            ("I", Some(v)) => i = v,
            ("J", Some(v)) => j = v,
            _ => {}
        }
    }
    (x, y, i, j)
}

/// Points along an arc from `a` to `b` around `c` (not including `a`).
pub fn arc_points(a: P, b: P, c: P, ccw: bool) -> Vec<P> {
    let r = ((a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2)).sqrt();
    let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
    let mut a1 = (b[1] - c[1]).atan2(b[0] - c[0]);
    let tau = std::f64::consts::TAU;
    if ccw {
        while a1 <= a0 + 1e-9 {
            a1 += tau;
        }
    } else {
        while a1 >= a0 - 1e-9 {
            a1 -= tau;
        }
    }
    let sweep = a1 - a0;
    // Big radii need finer steps than the angle alone, to stay within the
    // drawing tolerance the LC5 output's arc fit allows (BL38).
    let n = ((sweep.abs().to_degrees() / ARC_STEP_DEG).ceil() as usize).max(crate::dxf::arc_segments(r, sweep.abs()));
    let mut v: Vec<P> = (1..n).map(|k| {
        let t = a0 + sweep * k as f64 / n as f64;
        [c[0] + r * t.cos(), c[1] + r * t.sin()]
    }).collect();
    v.push(b);
    v
}

/// One part subprogram: closed cut contours, open cut paths, marking paths.
pub fn parse_sub(text: &str) -> (Vec<Vec<P>>, Vec<Vec<P>>, Vec<Vec<P>>) {
    let (mut loops, mut open, mut marks) = (vec![], vec![], vec![]);
    let mut cur: P = [0.0, 0.0];
    let mut path: Vec<P> = vec![];
    let mut profile = false;
    let mut mark = false;
    let mut started = false;
    // Where the head pierced (NEXT_L): a contour without a lead-in starts
    // there, its first piece (carrying G41/G42) before PROFILE (BL24).
    let mut pierce: P = [0.0, 0.0];
    let finish = |path: &mut Vec<P>, mark: bool, pierce: P, loops: &mut Vec<Vec<P>>, open: &mut Vec<Vec<P>>, marks: &mut Vec<Vec<P>>| {
        if path.len() >= 2 {
            let l = *path.last().unwrap();
            let near = |q: &P| ((q[0] - l[0]).powi(2) + (q[1] - l[1]).powi(2)).sqrt() < CLOSE_TOL;
            // The contour closes where the path comes back to: its start, or
            // — after Lantek's lead-in arc (a move right after PROFILE) — the
            // end of that arc, or — without a lead-in — the pierce point.
            let mut start = (0..path.len().saturating_sub(3)).find(|&i| near(&path[i]));
            if start.is_none() && !mark && path.len() >= 3 && near(&pierce) {
                path.insert(0, pierce);
                start = Some(0);
            }
            let mut p = std::mem::take(path);
            if mark {
                marks.push(p);
            } else if let Some(i) = start {
                p.pop();
                loops.push(p.split_off(i));
            } else {
                open.push(p);
            }
        }
        path.clear();
    };
    for raw in text.lines() {
        let l = raw.trim();
        if let Some(a) = inside(l, "NEXT_L(") {
            finish(&mut path, mark, pierce, &mut loops, &mut open, &mut marks);
            let v = nums(&a);
            cur = [v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0)];
            pierce = cur;
            // NEXT_L(x, y, next, pierce, quality …): quality 9 = marking.
            mark = v.get(4).copied() == Some(9.0) && v.len() < 6;
            profile = false;
            started = true;
        } else if l == "PROFILE" {
            profile = true;
            path.push(cur);
        } else if l.starts_with("G01") || l.starts_with("G02") || l.starts_with("G03") || l.starts_with("G1 ") || l.starts_with("G2 ") || l.starts_with("G3 ") {
            if !started {
                continue;
            }
            let (x, y, i, j) = words(l);
            let to = [x.unwrap_or(cur[0]), y.unwrap_or(cur[1])];
            if !profile {
                cur = to; // lead-in: the contour starts where it ends
                continue;
            }
            let g = l.split_whitespace().next().unwrap_or("");
            if g == "G02" || g == "G2" || g == "G03" || g == "G3" {
                path.extend(arc_points(cur, to, [cur[0] + i, cur[1] + j], g.ends_with('3')));
            } else {
                path.push(to);
            }
            cur = to;
        } else if let Some(a) = inside(l, "LASTLINE(") {
            let v = nums(&a);
            if v.len() >= 8 {
                let (from, to) = ([v[4], v[5]], [v[6], v[7]]);
                if path.is_empty() {
                    path.push(from);
                }
                // The 4th value is the move: 2 / 3 an arc (as G02 / G03) with
                // the centre at from + (9th, 10th) — the original's posts end
                // round holes on one; 1 a straight line.
                match v.get(3) {
                    Some(&t) if (t == 2.0 || t == 3.0) && v.len() >= 10 => path.extend(arc_points(from, to, [from[0] + v[8], from[1] + v[9]], t == 3.0)),
                    _ => path.push(to),
                }
                cur = to;
            }
            finish(&mut path, mark, pierce, &mut loops, &mut open, &mut marks);
            profile = false;
        } else if l == "ENDGEO" {
            finish(&mut path, mark, pierce, &mut loops, &mut open, &mut marks);
            profile = false;
        }
    }
    finish(&mut path, mark, pierce, &mut loops, &mut open, &mut marks);
    (loops, open, marks)
}

fn signed_area(p: &[P]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum::<f64>() * 0.5
}

fn rotate(p: P, deg: f64) -> P {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// A part from its contours; returns the geometry and the shift that moves
/// it to start at (0, 0).
fn part_geo(loops: &[Vec<P>], marks: &[Vec<P>]) -> Option<(CustomGeo, P)> {
    let oi = (0..loops.len()).max_by(|&a, &b| signed_area(&loops[a]).abs().total_cmp(&signed_area(&loops[b]).abs()))?;
    let o = &loops[oi];
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for q in o {
        lo = [lo[0].min(q[0]), lo[1].min(q[1])];
        hi = [hi[0].max(q[0]), hi[1].max(q[1])];
    }
    let sh = |l: &Vec<P>| l.iter().map(|q| [q[0] - lo[0], q[1] - lo[1]]).collect::<Vec<P>>();
    let outline = sh(o);
    let holes: Vec<Vec<P>> = loops.iter().enumerate().filter(|(k, _)| *k != oi).map(|(_, l)| sh(l)).collect();
    let fills = crate::dxf::triangulate(&outline);
    let hole_fills = holes.iter().flat_map(|h| crate::dxf::triangulate(h)).collect();
    let marks = marks.iter().map(sh).collect();
    Some((CustomGeo { outline, holes, fills, hole_fills, marks, texts: vec![], w: hi[0] - lo[0], h: hi[1] - lo[1], contours: vec![], hand_order: vec![] }, lo))
}

/// Part names from the job sheet: CNCSub → PartRef.
pub fn xml_names(xml: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut rest = xml;
    while let Some(a) = rest.find("<SubPrg>") {
        let Some(b) = rest[a..].find("</SubPrg>") else { break };
        let blk = &rest[a..a + b];
        let tag = |t: &str| blk.find(&format!("<{t}>")).and_then(|s| blk[s + t.len() + 2..].find(&format!("</{t}>")).map(|e| blk[s + t.len() + 2..s + t.len() + 2 + e].trim().to_string()));
        if let (Some(sub), Some(r)) = (tag("CNCSub"), tag("PartRef")) {
            out.push((sub.to_uppercase(), r));
        }
        rest = &rest[a + b..];
    }
    out
}

fn xml_tag(xml: &str, t: &str) -> Option<String> {
    let s = xml.find(&format!("<{t}>"))? + t.len() + 2;
    let e = xml[s..].find(&format!("</{t}>"))? + s;
    Some(xml[s..e].trim().to_string())
}

/// Turn a program set (file name, text) and its optional XML into a nesting.
pub fn parse_set(files: &[(String, String)], xml: Option<&str>) -> Result<Imported, String> {
    let main = files.iter().find(|f| f.1.contains("INITNEST")).ok_or("no sheet program (INITNEST) in these files")?;
    let mut im = Imported { name: main.0.trim_end_matches(".SPF").trim_end_matches(".spf").to_string(), ..Default::default() };
    for l in main.1.lines().take(12) {
        let l = l.trim();
        if let Some(h) = l.strip_prefix(';') {
            let f: Vec<&str> = h.split(',').collect();
            if f.len() == 4 && f[1].trim().parse::<f64>().is_ok() {
                im.material = f[0].trim().to_string();
                im.sheet_w = f[1].trim().parse().unwrap_or(0.0);
                im.sheet_h = f[2].trim().parse().unwrap_or(0.0);
                im.thickness = f[3].trim().parse().unwrap_or(0.0);
            }
        }
    }
    // The gas lines of the header (GS2), before the nesting starts.
    for l in main.1.lines().map(str::trim).take_while(|l| !l.starts_with("INITNEST")) {
        if let Some(v) = l.strip_prefix(";IDGAS=") {
            im.gas.get_or_insert_with(Default::default).0 = v.trim().to_string();
        }
        if let Some(v) = l.strip_prefix(";NOMEGAS=") {
            im.gas.get_or_insert_with(Default::default).1 = v.trim().to_string();
        }
    }
    let names = xml.map(xml_names).unwrap_or_default();
    if let Some(x) = xml {
        let (id, name) = (xml_tag(x, "GasId"), xml_tag(x, "GasName"));
        if id.is_some() || name.is_some() {
            im.xml_gas = Some((id.unwrap_or_default(), name.unwrap_or_default()));
        }
        im.job = xml_tag(x, "JobName").unwrap_or_default();
        if let Some(n) = xml_tag(x, "NestingName") {
            im.name = n;
        }
    }
    let find = |sub: &str| files.iter().find(|f| f.0.to_uppercase().trim_end_matches(".SPF") == sub);
    // (subprogram, angle) → part index, and the part's shift.
    let mut known: Vec<(String, i64, usize, P)> = vec![];
    let mut orig: (P, f64) = ([0.0, 0.0], 0.0);
    for l in main.1.lines() {
        let l = l.trim();
        if let Some(a) = inside(l, "ORIG(") {
            let v = nums(&a);
            orig = ([v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0)], v.get(2).copied().unwrap_or(0.0));
        } else if let Some(t) = l.strip_prefix("TRANS") {
            // With PS1 Z005 = 0 the post writes TRANS X.. Y.. then AROT Z..
            // instead of ORIG(x, y, angle) (NX6).
            let (x, y, _, _) = words(t);
            orig = ([x.unwrap_or(0.0), y.unwrap_or(0.0)], 0.0);
        } else if let Some(t) = l.strip_prefix("AROT") {
            if let Some(z) = t.split_whitespace().find_map(|w| w.strip_prefix('Z')).and_then(|v| v.trim_end_matches('.').parse::<f64>().ok()) {
                orig.1 = z;
            }
        } else if l.contains("CALL") {
            let Some(q) = l.find('"') else { continue };
            let call = l[q + 1..].trim_end_matches('"').to_string();
            let sub = call.rsplit("/_n_").next().unwrap_or("").trim_end_matches("_spf").trim_end_matches("_SPF").to_uppercase();
            let Some(f) = find(&sub) else {
                im.notes.push(format!("{sub}: subprogram not in the files"));
                continue;
            };
            let akey = (orig.1 * 1000.0).round() as i64;
            let (loops, open, marks) = parse_sub(&f.1);
            if loops.is_empty() {
                // A cut-off / free cut: an open path on the sheet.
                for p in open {
                    im.cuts.push(p.iter().map(|q| { let r = rotate(*q, orig.1); [r[0] + orig.0[0], r[1] + orig.0[1]] }).collect());
                }
                continue;
            }
            let (pi, shift) = match known.iter().find(|k| k.0 == sub && k.1 == akey) {
                Some(k) => (k.2, k.3),
                None => {
                    let rl: Vec<Vec<P>> = loops.iter().map(|l| l.iter().map(|q| rotate(*q, orig.1)).collect()).collect();
                    let rm: Vec<Vec<P>> = marks.iter().map(|l| l.iter().map(|q| rotate(*q, orig.1)).collect()).collect();
                    let Some((g, sh)) = part_geo(&rl, &rm) else { continue };
                    let base = names.iter().find(|n| n.0 == sub).map(|n| n.1.clone()).unwrap_or_else(|| sub.clone());
                    let name = if akey != 0 { format!("{base} ({}°)", orig.1) } else { base };
                    im.parts.push((name, sub.clone(), g));
                    known.push((sub.clone(), akey, im.parts.len() - 1, sh));
                    (im.parts.len() - 1, sh)
                }
            };
            im.placed.push((pi, orig.0[0] + shift[0], orig.0[1] + shift[1]));
        }
    }
    if im.placed.is_empty() {
        return Err("no parts found in the sheet program".into());
    }
    Ok(im)
}

/// The nesting names in a Lantek nesting export (.nxp). Expert's nesting
/// export (NX2–NX4) writes a text index — `G001 "<export>"`, `G002 <n>`, then per nesting a block
/// `N001 "<name>"` … `N025`, its parts `N100`–`N199`, remnants `N200`–`N299`,
/// `N300`, ending `N999` — and copies it as EXPORTNEST.NXP; the nestings'
/// own files are moved next to it.
pub fn nxp_nestings(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.trim_start().starts_with("N001"))
        .filter_map(|l| {
            let a = l.find('"')? + 1;
            let b = l[a..].find('"')? + a;
            Some(l[a..b].to_string())
        })
        .collect()
}

/// Lantek's own geometry / machining files start with `0 FILEMECLT <major>.<minor>`
/// (MF1; a file without that line counts
/// as version 7). Returns the version when the file has that line.
pub fn lantek_file_version(first_line: &str) -> Option<(u32, u32)> {
    let mut w = first_line.split_whitespace();
    if w.next()? != "0" || w.next()? != "FILEMECLT" {
        return None;
    }
    let mut v = w.next()?.split('.');
    let major = v.next()?.parse().ok()?;
    let minor = v.next().and_then(|m| m.parse().ok()).unwrap_or(0);
    Some((major, minor))
}

/// Versions above 7 are encrypted (MF2) with Lantek's own keys — this app
/// doesn't read those.
pub const LANTEK_PLAIN_MAX_MAJOR: u32 = 7;

pub fn lantek_encrypted(major: u32) -> bool {
    major > LANTEK_PLAIN_MAX_MAJOR
}

/// The message for a Lantek .mec / .chp file.
fn lantek_file_message(path: &std::path::Path, ver: Option<(u32, u32)>) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    // MF1, MF2: the parts of a Lantek .mec come in through Import drawings (MF4–MF15).
    let parts = "import its parts with Import drawings (Lantek .mec files, encrypted too)";
    match ver {
        Some((ma, mi)) if lantek_encrypted(ma) => format!(
            "{name} is a Lantek drawing/machining file (version {ma}.{mi}), which Lantek encrypts. It doesn't open as a nesting here: {parts}, or open the nesting's LC5 programs."
        ),
        Some((ma, mi)) => format!("{name} is an older Lantek file (version {ma}.{mi}). It doesn't open as a nesting here: {parts}, or open the nesting's LC5 programs."),
        None => format!("{name} is one of Lantek's own nesting files. It doesn't open as a nesting here: {parts}, or open the nesting's LC5 programs."),
    }
}

/// Read a nesting from a picked file: a .zip (with the .XML next to it), an
/// .XML (with its .zip), or a sheet .SPF (with the other .SPF files beside it).
pub fn open(path: &std::path::Path) -> Result<Imported, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let stem = path.with_extension("");
    let latin = |b: &[u8]| b.iter().map(|&c| c as char).collect::<String>();
    let sibling = |e: &[&str]| e.iter().map(|x| stem.with_extension(x)).find(|p| p.exists());
    if ext == "mec" || ext == "chp" || ext == "ret" {
        let first = std::fs::read(path).ok().map(|b| b.iter().take(200).map(|&c| c as char).collect::<String>()).unwrap_or_default();
        return Err(lantek_file_message(path, lantek_file_version(first.lines().next().unwrap_or(""))));
    }
    if ext == "nxp" {
        // Open the first nesting of the export whose LC5 programs are beside it.
        let text = std::fs::read(path).map(|b| latin(&b)).map_err(|e| e.to_string())?;
        let names = nxp_nestings(&text);
        let dir = path.parent().ok_or("no folder")?;
        for n in &names {
            for e in ["zip", "ZIP", "XML", "xml", "SPF", "spf"] {
                let f = dir.join(format!("{n}.{e}"));
                if f.exists() {
                    let mut im = open(&f)?;
                    if names.len() > 1 {
                        im.notes.push(format!("the export lists {} nestings: {}", names.len(), names.join(", ")));
                    }
                    return Ok(im);
                }
            }
        }
        return Err(if names.is_empty() {
            "no nestings (N001 lines) in this .nxp".into()
        } else {
            format!("this export lists {} — open their LC5 programs (.zip / .XML), which aren't next to it", names.join(", "))
        });
    }
    let (files, xml): (Vec<(String, String)>, Option<String>) = match ext.as_str() {
        "zip" | "xml" => {
            let zip = if ext == "zip" { Some(path.to_path_buf()) } else { sibling(&["zip", "ZIP"]) };
            let zip = zip.ok_or("no .zip with the programs next to this .XML")?;
            let b = std::fs::read(&zip).map_err(|e| e.to_string())?;
            let files = read_zip(&b)?.into_iter().map(|(n, d)| (n, latin(&d))).collect();
            let xml = if ext == "xml" { std::fs::read(path).ok() } else { sibling(&["XML", "xml"]).and_then(|p| std::fs::read(p).ok()) };
            (files, xml.map(|b| String::from_utf8_lossy(&b).to_string()))
        }
        "spf" => {
            let dir = path.parent().ok_or("no folder")?;
            let files = std::fs::read_dir(dir)
                .map_err(|e| e.to_string())?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("spf")))
                .filter_map(|e| std::fs::read(e.path()).ok().map(|b| (e.file_name().to_string_lossy().to_string(), latin(&b))))
                .collect::<Vec<(String, String)>>();
            // The sheet program picked comes first, so it's the one opened
            // when the folder holds several sheets' programs.
            let picked = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let mut files = files;
            files.sort_by_key(|f| f.0 != picked);
            (files, sibling(&["XML", "xml"]).and_then(|p| std::fs::read(p).ok()).map(|b| String::from_utf8_lossy(&b).to_string()))
        }
        _ => {
            // A Lantek file under another name still says what it is.
            let first = std::fs::read(path).ok().map(|b| b.iter().take(200).map(|&c| c as char).collect::<String>()).unwrap_or_default();
            if let Some(v) = lantek_file_version(first.lines().next().unwrap_or("")) {
                return Err(lantek_file_message(path, Some(v)));
            }
            return Err("pick a .zip, .XML or .SPF machine program, or a Lantek .nxp export".into());
        }
    };
    parse_set(&files, xml.as_deref())
}

#[cfg(test)]
mod progimport_tests {
    use super::*;

    #[test]
    fn big_arcs_are_rebuilt_within_the_drawing_tolerance() {
        // BL38: 5 degree steps would sit 0.95 mm off an R1000 arc.
        let v = arc_points([1000.0, 0.0], [0.0, 1000.0], [0.0, 0.0], true);
        for w in v.windows(2) {
            let half = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt() / 2.0;
            assert!(1000.0 - (1000.0f64.powi(2) - half * half).sqrt() <= crate::dxf::ARC_CHORD_ERR + 1e-9);
        }
    }

    #[test]
    fn reads_back_a_program_set_this_app_writes() {
        use crate::nest::{Part, Placed, Shape, SheetSpec};
        let parts = vec![Part { name: "BLANK".into(), shape: Shape::PlateHole, w: 100.0, h: 60.0, qty: 2, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let sheet = vec![Placed { part: 0, x: 10.0, y: 10.0, w: 100.0, h: 60.0, turns: 0, mirror: false, spin: Default::default() }, Placed { part: 0, x: 10.0, y: 80.0, w: 100.0, h: 60.0, turns: 0, mirror: false, spin: Default::default() }];
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 10.0, material: 1, thickness: 6.35, level: 3, machine: Default::default(), kerf: Default::default(), gas: None };
        let plan = crate::cnc::plan(&parts, &sheet, &spec);
        let cut = crate::cutoff::CutLine { pts: vec![[200.0, 0.0], [200.0, 500.0]], kind: crate::cutoff::CutKind::Remnant };
        let set = crate::blm::sheet_set(1, &parts, &sheet, &spec, &plan, &[cut], &crate::blm::BlmSettings::for_spec(&spec, "LC5", "J-1"));
        // Through the zip, like the real files.
        let files: Vec<(String, String)> = read_zip(&crate::blm::zip(&set.spf)).unwrap().into_iter().map(|(n, d)| (n, String::from_utf8(d).unwrap())).collect();
        let im = parse_set(&files, Some(&set.xml)).unwrap();
        assert_eq!((im.sheet_w, im.sheet_h, im.thickness, im.material.as_str()), (1000.0, 500.0, 6.35, "STAINLESS"));
        assert_eq!(im.parts.len(), 1);
        assert_eq!(im.parts[0].0, "BLANK");
        let g = &im.parts[0].2;
        assert!((g.w - 100.0).abs() < 0.01 && (g.h - 60.0).abs() < 0.01, "{} x {}", g.w, g.h);
        assert_eq!(g.holes.len(), 1, "the plate's hole");
        assert_eq!(im.placed.len(), 2);
        assert!((im.placed[0].1 - 10.0).abs() < 0.01 && (im.placed[1].2 - 80.0).abs() < 0.01);
        assert_eq!(im.cuts.len(), 1);
        assert!(im.cuts[0].iter().all(|q| (q[0] - 200.0).abs() < 0.01));
    }

    #[test]
    fn reads_nxp_indexes_and_trans_arot_placements() {
        let nxp = "G001 \"EXPORT\" ;\nG002 1 ;\nN001 \"1234X38THXSAMPLE\" ;\nN002 \"J-1234-01\" ;\nN100 \"BLANK\" ;\nN199  ;\nN999  ;\nN001 \"SECOND\" ;\n";
        assert_eq!(nxp_nestings(nxp), vec!["1234X38THXSAMPLE", "SECOND"]);
        let main = ";ADILAS\n;STEEL,1000,500,3\nINITNEST\nTRANS X20 Y30\nAROT Z90\nCALL\"/_n_wks_dir/_n_sheet_wpd/_n_A701_spf\"\nENDNEST\n";
        let sub = "INITPART(1)\nNEXT_L(0,0,0,0,3)\nPROFILE\nG01 X10. Y0.\nG01 X10. Y5.\nG01 X0. Y5.\nLASTLINE(0,0,0,1,0.,5.,0.,0.,0,0)\nENDGEO\n";
        let im = parse_set(&[("A.SPF".into(), main.into()), ("A701.SPF".into(), sub.into())], None).unwrap();
        // A 10 x 5 part turned 90°: 5 x 10, placed from TRANS.
        let g = &im.parts[0].2;
        assert!((g.w - 5.0).abs() < 1e-6 && (g.h - 10.0).abs() < 1e-6, "{} x {}", g.w, g.h);
        assert!((im.placed[0].1 - 15.0).abs() < 1e-6 && (im.placed[0].2 - 30.0).abs() < 1e-6, "{:?}", im.placed[0]);
    }

    #[test]
    fn recognises_lantek_files_and_their_encryption() {
        assert_eq!(lantek_file_version("0 FILEMECLT 8.1"), Some((8, 1)));
        assert_eq!(lantek_file_version("0 FILEMECLT 8.00\r"), Some((8, 0)));
        assert_eq!(lantek_file_version("0 FILEMECLT 7.00"), Some((7, 0)));
        assert_eq!(lantek_file_version(";ADILAS"), None);
        assert!(lantek_encrypted(8) && !lantek_encrypted(7));
        // Lantek's own sample, when the install is on this PC.
        let f = std::path::Path::new(r"C:\Lantek\Expert\Samples\MEC\MECPART0.MEC");
        if f.exists() {
            let e = open(f).unwrap_err();
            assert!(e.contains("version 8.1") && e.contains("encrypts"), "{e}");
        }
    }

    #[test]
    fn arcs_come_back_round() {
        // Half circle radius 5 from (10,0) to (0,0) counter-clockwise around (5,0).
        let pts = arc_points([10.0, 0.0], [0.0, 0.0], [5.0, 0.0], true);
        assert!(pts.iter().all(|q| ((q[0] - 5.0).powi(2) + q[1].powi(2)).sqrt() - 5.0 < 1e-9));
        assert!(pts.iter().all(|q| q[1] >= -1e-9));
        assert_eq!(*pts.last().unwrap(), [0.0, 0.0]);
    }

    #[test]
    fn a_contour_ending_on_an_arc_or_without_a_lead_in_comes_back_whole() {
        // A slot whose round end is the LASTLINE (type 2, centre from + (0,-50)).
        let slot = "NEXT_L(100,0,0,2,3)\nPROFILE\nG01 X0 Y0\nG01 X0 Y100\nG01 X100 Y100\nLASTLINE(0,0,0,2,100,100,100,0,0,-50)\nENDGEO\n";
        let (loops, open, _) = parse_sub(slot);
        assert!(open.is_empty() && loops.len() == 1, "{loops:?} {open:?}");
        let w = loops[0].iter().map(|q| q[0]).fold(f64::MIN, f64::max);
        assert!((w - 150.0).abs() < 0.5, "round end reaches x = 150, got {w}");
        // No lead-in (BL24): G41 on a short move before PROFILE, back to the pierce point.
        let bare = "NEXT_L(0,0,0,2,3)\nG01 G41 X4 Y0\nPROFILE\nG01 X100 Y0\nG01 X100 Y50\nG01 X0 Y50\nLASTLINE(0,0,0,1,0,50,0,0,0,0)\nNEXT_L(40,20,1,2,3)\nG01 G42 X44 Y20\nPROFILE\nG01 X50 Y20\nG01 X50 Y30\nG01 X40 Y30\nLASTLINE(0,0,0,1,40,30,40,20,0,0)\nENDGEO\n";
        let (loops, open, _) = parse_sub(bare);
        assert!(open.is_empty() && loops.len() == 2, "{loops:?} {open:?}");
        let (g, _) = part_geo(&loops, &[]).unwrap();
        assert_eq!((g.w.round(), g.h.round(), g.holes.len()), (100.0, 50.0, 1));
    }

    #[test]
    fn reads_the_gas_the_sheet_program_and_its_xml_name() {
        // GS2: the header's ;IDGAS= / ;NOMEGAS= and the XML's GasId / GasName,
        // as written (empty for no gas); None without them.
        let sub = ("A701.SPF".to_string(), "NEXT_L(0,0,0,1,3)\nPROFILE\nG01 X10. Y0.\nG01 X10. Y5.\nG01 X0. Y5.\nLASTLINE(0,0,0,1,0.,5.,0.,0.,0,0)\nENDGEO\n".to_string());
        let set = |gas: &str| vec![("A.SPF".to_string(), format!(";ADILAS\r\n;STEEL,1000,500,12.7\r\n;IDMAT=0010\r\n;NOMEMAT=STEEL\r\n{gas};Postprocessor version 2.0.0\r\nINITNEST\r\nORIG(10,10,0)\r\nN1 CALL\"/_n_wks_dir/_n_sheet_wpd/_n_A701_spf\"\r\nENDNEST\r\n")), sub.clone()];
        let xml = "<General>\r\n\t<GasId>5</GasId>\r\n\t<GasName>H2O</GasName>\r\n</General>\r\n";
        let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
        let im = parse_set(&set(";IDGAS=3\r\n;NOMEGAS=Oxygen\r\n"), Some(xml)).unwrap();
        assert_eq!((im.gas, im.xml_gas, im.thickness), (pair("3", "Oxygen"), pair("5", "H2O"), 12.7));
        let im = parse_set(&set(";IDGAS=\r\n;NOMEGAS=\r\n"), None).unwrap();
        assert_eq!((im.gas, im.xml_gas), (pair("", ""), None));
        let im = parse_set(&set(""), Some("<General></General>")).unwrap();
        assert_eq!((im.gas, im.xml_gas), (None, None));
    }

    #[test]
    fn an_spf_picked_from_a_folder_opens_that_sheet() {
        let d = std::env::temp_dir().join(format!("nestpro_spf_pick_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for (n, w) in [("A", 1000), ("B", 2000)] {
            std::fs::write(d.join(format!("{n}.SPF")), format!(";ADILAS\n;STEEL,{w},500,3\nINITNEST\nORIG(10,10,0)\nN1 CALL\"/_n_wks_dir/_n_sheet_wpd/_n_{n}701_spf\"\nENDNEST\n")).unwrap();
            std::fs::write(d.join(format!("{n}701.SPF")), "NEXT_L(0,0,0,2,3)\nPROFILE\nG01 X50 Y0\nG01 X50 Y50\nG01 X0 Y50\nLASTLINE(0,0,0,1,0,50,0,0,0,0)\nENDGEO\n").unwrap();
        }
        for (n, w) in [("A", 1000.0), ("B", 2000.0)] {
            let im = open(&d.join(format!("{n}.SPF"))).unwrap();
            assert_eq!(im.sheet_w, w, "{n}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
