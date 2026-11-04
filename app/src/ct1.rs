//! The machine's technology file (Lantek `MchFiles\<post>.CT1`): plain text
//! lines `Xnnn value ; comment`. The BLM laser's file is PstAdi06.CT1; its
//! lead-in, overlap and micro-joint values are the defaults here (see
//! MECHANICS.md "Machine technology file"). The automatic lead tables by
//! contour class and quality are in `leads::LeadTable` (B200–B295, E000–E909).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Values the part technology uses, by their CT1 codes.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineTech {
    /// Quality 1's values (corrected in v44 build analysis: quality 1 is the B
    /// block; E000… is quality 2). B200: zone (1–9 keypad) where the lead goes.
    pub zone: u8,
    /// B201: largest corner angle for a lead in a corner, degrees.
    pub corner_max: f64,
    /// B204: shortest element that takes a lead, mm.
    pub min_element: f64,
    /// B244 / B245: lead-in and lead-out distance on other-shaped contours (contour 1), mm.
    pub lead_in: f64,
    pub lead_out: f64,
    /// B246: lead-in angle on contours, degrees.
    pub lead_angle: f64,
    /// B214 / B215: lead-in and lead-out distance in round holes, mm.
    pub hole_lead_in: f64,
    pub hole_lead_out: f64,
    /// Automatic lead tables, every class and quality (B150–B295, E000–E909).
    pub leads: crate::leads::LeadTable,
    /// B156: overlap of the cut past its start before the lead-out, mm.
    pub overlap: f64,
    /// B157: loop radius, mm.
    pub loop_radius: f64,
    /// F000: joint type of quality 1's default record (3 = leads, perpendicular).
    pub joint_type: i32,
    /// F001 / F003: joint lead-in distance and angle (joints by quality 1).
    pub joint_lead: f64,
    pub joint_angle: f64,
    /// F002: joint lead-out distance (0 = no lead-out).
    pub joint_lead_out: f64,
    /// F005: joint uncut length (the gap), mm; 99999.999 there = B164.
    pub joint_gap: f64,
    /// F150: maximum distance between joints, mm (quality 1; corrected in v44
    /// build analysis: F120 is not read).
    pub joint_every: f64,
    /// Per quality (F150 + 4q … F153 + 4q): max distance, min distance to a
    /// corner, min distance between joints on one element, step angle on circles.
    pub joint_dist: [[f64; 4]; 12],
    /// F121 / F122: fewest and most joints per contour (fallbacks of the size classes).
    pub joint_min: u32,
    pub joint_max: u32,
    /// F136–F149 (+ F121/F122): fewest/most joints by contour size.
    pub joint_sizes: crate::nest::AutoJoints,
    /// F133–F135: joint overlap test (> 0 on), shortening per step, tries.
    pub joint_test: i32,
    pub joint_shorten: f64,
    pub joint_tries: i32,
    /// B122 / B123: cutting zones along X and Y (the cut order goes zone by zone).
    pub zones_x: u32,
    pub zones_y: u32,
    /// B110: start at the left (cutting); start at the bottom, as the real
    /// BLM program does (B111 not followed, see `from_text`).
    pub start_left: bool,
    pub start_bottom: bool,
    /// B114 / B115: run along X first (else along Y), and zig-zag.
    pub horizontal: bool,
    pub zigzag: bool,
    /// B134: test parts inside holes (nest small parts in the holes of big ones).
    pub parts_in_holes: bool,
    /// B076: smallest hole area offered to other parts, mm² (0 = any).
    pub min_hole_area: f64,
    /// A785: group parts by attributes; A787: each group in its own nesting
    /// zone; A788: gap between the zones, mm.
    pub group_parts: bool,
    pub group_zones: bool,
    pub zone_gap: f64,
    /// Nesting: B057 strip width of the true-shape nester, mm (0 = not in the
    /// file; the nester then uses `nest::BAND_DEFAULT`). The shop CT1s say 13.896.
    pub band_width: f64,
    /// Nesting: B078 "fit the parts to the sheet border": the usable area grows by
    /// half the gap so a part's real edge can reach the margin (shop CT1s: ON).
    pub fit_border: bool,
    /// The file the values came from (None = built-in values).
    pub source: Option<PathBuf>,
}

impl Default for MachineTech {
    /// The values in C:\Lantek\Expert\MchFiles\PstAdi06.CT1 (the BLM laser).
    fn default() -> Self {
        MachineTech {
            zone: 1,
            corner_max: 135.0,
            min_element: 5.0,
            lead_in: 7.0,
            lead_out: 5.0,
            lead_angle: 45.0,
            hole_lead_in: 7.0,
            hole_lead_out: 5.0,
            leads: crate::leads::LeadTable::default(),
            overlap: 0.5,
            loop_radius: 10.0,
            // F000–F005 of the laser files (PSTBUR01/PSTMAZ03.CT1, matching PstAdi06):
            // type 1 there is a corner type; ours defaults to 3 (perpendicular leads,
            // PSTadi02.CT1) so every contour can take joints.
            joint_type: 3,
            joint_lead: 10.0,
            joint_angle: 45.0,
            joint_lead_out: 0.0,
            joint_gap: 5.0,
            joint_every: 100.0,
            joint_dist: [[100.0, 0.0, 0.0, 0.0]; 12],
            joint_min: 0,
            joint_max: 32767,
            joint_sizes: crate::nest::AutoJoints { max: 32767, ..Default::default() },
            joint_test: -1,
            joint_shorten: -1.0,
            joint_tries: -1,
            zones_x: 20,
            zones_y: 10,
            start_left: true,
            start_bottom: true,
            horizontal: false,
            zigzag: false,
            parts_in_holes: true,
            min_hole_area: 0.0,
            group_parts: false,
            group_zones: true,
            zone_gap: 0.0,
            // Nesting (B057 isn't known for PstAdi06; B078 ON as in every shop CT1).
            band_width: 0.0,
            fit_border: true,
            source: None,
        }
    }
}

static ACTIVE: std::sync::RwLock<Option<MachineTech>> = std::sync::RwLock::new(None);

/// The active machine's technology (set when settings are applied).
pub fn active() -> MachineTech {
    ACTIVE.read().ok().and_then(|g| g.clone()).unwrap_or_default()
}

pub fn set_active(t: MachineTech) {
    if let Ok(mut g) = ACTIVE.write() {
        *g = Some(t);
    }
}

/// `code → value` for every `Xnnn value ; comment` line (quotes kept).
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        let mut it = line.splitn(2, char::is_whitespace);
        let (Some(code), Some(val)) = (it.next(), it.next()) else { continue };
        let b = code.as_bytes();
        if b.len() == 4 && b[0].is_ascii_uppercase() && b[1..].iter().all(u8::is_ascii_digit) {
            m.insert(code.to_string(), val.trim().to_string());
        }
    }
    m
}

impl MachineTech {
    pub fn from_text(text: &str) -> MachineTech {
        let m = parse(text);
        let d = MachineTech::default();
        let f = |k: &str, def: f64| m.get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(def);
        let u = |k: &str, def: u32| m.get(k).and_then(|v| v.parse::<i64>().ok()).map_or(def, |v| v.clamp(0, u32::MAX as i64) as u32);
        let i = |k: &str, def: i32| m.get(k).and_then(|v| v.parse::<f64>().ok()).map_or(def, |v| v as i32);
        let leads = crate::leads::LeadTable::from_codes(&m);
        let q1 = leads.quality(1);
        // Joints (AM1, MT11): F0q0–F0q8 = quality q's
        // default record; F150+4q… = its distances; an unset gap (99999.999) = B164.
        let b164 = f("B164", d.joint_gap);
        let gap = f("F005", d.joint_gap);
        let gap = if (gap - crate::leads::UNSET).abs() <= crate::leads::UNSET_TOL { b164 } else { gap };
        let mut joint_dist = d.joint_dist;
        for (q, row) in joint_dist.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = f(&format!("F{}", 150 + 4 * q + j), *v);
            }
        }
        let (jmin, jmax) = (u("F121", d.joint_min), u("F122", d.joint_max));
        let pair = |a: &str, b: &str| [i(a, -1), i(b, -1)];
        let joint_sizes = crate::nest::AutoJoints {
            small: [f("F136", 0.0), f("F137", 0.0)],
            large: [f("F138", 99999.999), f("F139", 99999.999)],
            parts: [pair("F142", "F143"), [jmin as i32, jmax.min(i32::MAX as u32) as i32], pair("F148", "F149")],
            holes: [pair("F140", "F141"), pair("F144", "F145"), pair("F146", "F147")],
            min: jmin,
            max: jmax,
        };
        MachineTech {
            zone: q1.zone.clamp(1, 9) as u8,
            corner_max: q1.corner_max,
            min_element: f("B204", d.min_element),
            lead_in: q1.recs[5].d_in,
            lead_out: q1.recs[5].d_out,
            lead_angle: q1.recs[5].a_in,
            hole_lead_in: q1.recs[0].d_in,
            hole_lead_out: q1.recs[0].d_out,
            overlap: leads.overlap,
            leads,
            loop_radius: f("B157", d.loop_radius),
            joint_type: i("F000", d.joint_type),
            joint_lead: f("F001", d.joint_lead),
            joint_angle: f("F003", d.joint_angle),
            joint_lead_out: f("F002", d.joint_lead_out),
            joint_gap: gap,
            joint_every: joint_dist[0][0],
            joint_dist,
            joint_min: jmin,
            joint_max: jmax,
            joint_sizes,
            joint_test: i("F133", d.joint_test),
            joint_shorten: f("F134", d.joint_shorten),
            joint_tries: i("F135", d.joint_tries),
            zones_x: u("B122", d.zones_x),
            zones_y: u("B123", d.zones_y),
            start_left: m.get("B110").map_or(d.start_left, |v| v == "ON"),
            // B111 is OFF in PstAdi06.CT1, yet the real program 1234X38THXSAMPLE
            // cuts from the bottom up (ORIG Y 10 → 1091.6): the bottom start is kept.
            start_bottom: d.start_bottom,
            horizontal: m.get("B114").map_or(d.horizontal, |v| v == "ON"),
            zigzag: m.get("B115").map_or(d.zigzag, |v| v == "ON"),
            parts_in_holes: m.get("B134").map_or(d.parts_in_holes, |v| v == "ON"),
            min_hole_area: f("B076", d.min_hole_area),
            group_parts: m.get("A785").map_or(d.group_parts, |v| v == "ON"),
            group_zones: m.get("A787").map_or(d.group_zones, |v| v == "ON"),
            zone_gap: f("A788", d.zone_gap),
            // Nesting: strip width and fit to the border.
            band_width: f("B057", d.band_width).max(0.0),
            fit_border: m.get("B078").map_or(d.fit_border, |v| v == "ON"),
            source: None,
        }
    }

    /// Read a CT1 file (Latin-1 text).
    pub fn load(path: &Path) -> Option<MachineTech> {
        let bytes = std::fs::read(path).ok()?;
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let mut t = MachineTech::from_text(&text);
        t.source = Some(path.to_path_buf());
        Some(t)
    }

    /// The machine's CT1: beside its post library or BLM .CF1/.PS1 (same
    /// name, .CT1), else the BLM post's file in the Lantek folder.
    pub fn for_machine(m: &crate::settings::Machine, lantek_dir: &str) -> MachineTech {
        let mut tries: Vec<PathBuf> = vec![];
        for p in [&m.blm_cf1, &m.blm_ps1, &m.post_lib] {
            if !p.is_empty() {
                let pb = PathBuf::from(p);
                tries.push(pb.with_extension("CT1"));
                tries.push(pb.with_extension("ct1"));
            }
        }
        let dir = if lantek_dir.is_empty() { crate::settings::lantek_default_dir() } else { lantek_dir.to_string() };
        tries.push(Path::new(&dir).join("MchFiles").join("PstAdi06.CT1"));
        tries.iter().find(|p| p.is_file()).and_then(|p| MachineTech::load(p)).unwrap_or_default()
    }

    /// Joints for a medium-sized part contour of length `per` at one per
    /// `joint_every` mm (F150), within F121–F122: ceil, then made even
    /// (corrected in v44 build analysis: not rounded down, F120 not read).
    pub fn joints_for(&self, per: f64) -> u32 {
        crate::joints::count(per, self.joint_every, self.joint_min, self.joint_max, self.leads.lin_tol).0
    }
}

#[cfg(test)]
mod ct1_tests {
    use super::*;

    #[test]
    fn reads_the_blm_technology_lines() {
        // Corrected in v44 build analysis: quality 1's contour lead is B244 (E044 is
        // quality 2), the joint distance is F150 (F120 is not read), and the count
        // is rounded up and made even.
        let t = "# DATOS\nB156 0.5                   ; Solapamiento de las entradas y salidas.\nB244 8.0 ; Distancia entrada contornos 1.\nE000 1\nE044 9.0\nF005 2.5 ; Long. no corte\nF120 150.0 ; Distancia entre amarres.\nF150 120.0\nF122 32767\nA009 \"CAJPIE.CAJ\" ; Cajetin\n";
        let m = parse(t);
        assert_eq!(m.get("A009").map(String::as_str), Some("\"CAJPIE.CAJ\""));
        let mt = MachineTech::from_text(t);
        assert_eq!((mt.overlap, mt.lead_in, mt.joint_gap, mt.joint_every, mt.joint_max), (0.5, 8.0, 2.5, 120.0, 32767));
        assert_eq!(mt.leads.quality(2).recs[5].d_in, 9.0);
        // Missing codes keep the PstAdi06.CT1 values.
        assert_eq!((mt.lead_out, mt.joint_lead, mt.zone), (5.0, 10.0, 1));
        // 620 mm at one per 120 mm: ceil 5.2 = 6.
        assert_eq!(mt.joints_for(620.0), 6);
        // An unset gap takes B164.
        assert_eq!(MachineTech::from_text("F005 99999.999\nB164 0.35\n").joint_gap, 0.35);
    }

    #[test]
    fn zones_split_the_sheet_and_hold_parts_by_their_corners() {
        let t = MachineTech::default();
        let z = zones(2000.0, 1000.0, &t);
        assert_eq!(z.len(), 200);
        // Left column first, bottom up (as the real BLM program cuts).
        assert_eq!(z[0], [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(z[1], [0.0, 100.0, 100.0, 200.0]);
        assert_eq!(z[10], [100.0, 0.0, 200.0, 100.0]);
        // CZ4: overlap plus a corner inside.
        assert!(box_in_zone([90.0, 10.0, 150.0, 60.0], [0.0, 0.0, 100.0, 100.0]));
        assert!(!box_in_zone([110.0, 10.0, 150.0, 60.0], [0.0, 0.0, 100.0, 100.0]));
        // A long strip crossing the zone with no corner in it: not in that zone.
        assert!(!box_in_zone([-10.0, 40.0, 300.0, 60.0], [100.0, 0.0, 200.0, 100.0]));
        let zz = zones(200.0, 200.0, &MachineTech { zones_x: 2, zones_y: 2, zigzag: true, ..MachineTech::default() });
        assert_eq!(zz, vec![[0.0, 0.0, 100.0, 100.0], [0.0, 100.0, 100.0, 200.0], [100.0, 100.0, 200.0, 200.0], [100.0, 0.0, 200.0, 100.0]]);
    }

    #[test]
    fn a_column_of_parts_is_cut_bottom_up_like_the_real_program() {
        use crate::nest::{Part, Placed, Shape, SheetSpec};
        // 1234X38THXSAMPLE: ORIG(10,10) (10,93.2) … (10,1091.6), in that order.
        let parts = vec![Part { name: "S".into(), shape: Shape::Rectangle, w: 1180.0, h: 73.2, qty: 14, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let mut placed: Vec<Placed> = (0..14).map(|k| Placed { part: 0, x: 10.0, y: 10.0 + 83.2 * k as f64, w: 1180.0, h: 73.2, turns: 0, mirror: false }).collect();
        placed.reverse();
        let spec = SheetSpec { w: 1200.15, h: 1219.2, margin: 10.0, gap: 10.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() };
        let plan = crate::cnc::plan(&parts, &placed, &spec);
        let ys: Vec<f64> = plan.contours.iter().map(|c| placed[c.part_no - 1].y).collect();
        assert!(ys.windows(2).all(|w| w[0] <= w[1]), "{ys:?}");
    }

    #[test]
    fn the_installed_blm_file_matches_the_built_in_values() {
        let p = Path::new(r"C:\Lantek\Expert\MchFiles\PstAdi06.CT1");
        if let Some(t) = MachineTech::load(p) {
            let d = MachineTech::default();
            assert_eq!((t.lead_in, t.lead_out, t.overlap, t.joint_gap, t.joint_every, t.joint_lead), (d.lead_in, d.lead_out, d.overlap, d.joint_gap, d.joint_every, d.joint_lead));
        }
    }
}

/// Cutting zones of a sheet, in the order they are cut (CZ1: zone width = sheet length ÷ X zones, height =
/// sheet width ÷ Y zones, only when the count is above 0). Each zone is
/// (x0, y0, x1, y1). Columns (or rows when `horizontal`) start on the
/// configured side; with `zigzag` every other one runs back.
pub fn zones(w: f64, h: f64, t: &MachineTech) -> Vec<[f64; 4]> {
    let nx = t.zones_x.max(1) as usize;
    let ny = t.zones_y.max(1) as usize;
    let (zw, zh) = (if t.zones_x > 0 { w / t.zones_x as f64 } else { w }, if t.zones_y > 0 { h / t.zones_y as f64 } else { h });
    let xs: Vec<usize> = if t.start_left { (0..nx).collect() } else { (0..nx).rev().collect() };
    let ys: Vec<usize> = if t.start_bottom { (0..ny).collect() } else { (0..ny).rev().collect() };
    let mut out = vec![];
    let cell = |i: usize, j: usize| [i as f64 * zw, j as f64 * zh, (i + 1) as f64 * zw, (j + 1) as f64 * zh];
    if t.horizontal {
        for (k, &j) in ys.iter().enumerate() {
            let row: Vec<usize> = if t.zigzag && k % 2 == 1 { xs.iter().rev().copied().collect() } else { xs.clone() };
            out.extend(row.into_iter().map(|i| cell(i, j)));
        }
    } else {
        for (k, &i) in xs.iter().enumerate() {
            let col: Vec<usize> = if t.zigzag && k % 2 == 1 { ys.iter().rev().copied().collect() } else { ys.clone() };
            out.extend(col.into_iter().map(|j| cell(i, j)));
        }
    }
    out
}

/// Is a part's box in a zone? (CZ4):
/// its box must overlap the zone, and one of its box corners must lie in it.
pub fn box_in_zone(b: [f64; 4], z: [f64; 4]) -> bool {
    if z[0] > b[2] || b[0] > z[2] || z[1] > b[3] || b[1] > z[3] {
        return false;
    }
    let inside = |x: f64, y: f64| x >= z[0] && x <= z[2] && y >= z[1] && y <= z[3];
    inside(b[0], b[1]) || inside(b[2], b[3]) || inside(b[0], b[3]) || inside(b[2], b[1])
}
