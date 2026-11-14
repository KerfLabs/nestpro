//! Saving and reopening a job: its products (with their own technology,
//! texts and imported drawings), the sheet settings, every nested sheet and
//! the cut lines, in one readable JSON file (`.snjob`). Lantek keeps a job in
//! its database plus per-record files on the server (Nestings\<n/1000>\<n>
//! .chp/.mec/.ret…, MECHANICS.md "How Lantek stores nestings"); this app keeps
//! each job in one file the person chooses.

use crate::cutoff::CutLine;
use crate::nest::{Part, Placed, SheetSpec};
use serde::{Deserialize, Serialize};

/// What a job file says it is, and the newest layout version this build
/// reads. Layout 2: a part on a sheet may sit at an angle that isn't a
/// quarter turn (HM14), which older builds would show and cut at the wrong
/// angle, so they refuse it ("made by a newer version").
pub const FORMAT: &str = "nestpro-job";
pub const VERSION: u32 = 2;
/// A job with every part on a quarter turn is written as layout 1, which
/// older builds still open.
pub const VERSION_QUARTER_TURNS: u32 = 1;
/// The job file extension.
pub const EXT: &str = "snjob";

#[derive(Clone, Serialize, Deserialize)]
pub struct JobFile {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub week: String,
    pub parts: Vec<Part>,
    pub spec: SheetSpec,
    pub sheets: Vec<Vec<Placed>>,
    /// Each sheet's format (size, remnant outline, stock reference). Older
    /// files have none: every sheet is then the job's sheet size.
    #[serde(default)]
    pub formats: Vec<crate::nest::SheetFormat>,
    /// Sheets taken from stock, per reference, when the job was saved.
    #[serde(default)]
    pub held: std::collections::BTreeMap<String, i32>,
    #[serde(default)]
    pub cut_lines: Vec<Vec<CutLine>>,
    #[serde(default)]
    pub method: String,
    /// Contours picked by hand, in cutting order, per sheet.
    #[serde(default)]
    pub manual_cuts: Vec<Vec<crate::cnc::ManualCut>>,
    /// Parts or sheet changed since the last nest.
    #[serde(default)]
    pub stale: bool,
    /// The sheet stock follows this job: its sheets are taken (`held`) and its
    /// remnants stored. The window books a job on every save; an agent books
    /// it with `stock-book` (until then its nestings leave the stock alone),
    /// except a job file that is a library job, which the window books (LB10).
    #[serde(default)]
    pub booked: bool,
    /// The ERP/MES work the job came from (integrations::ErpJob), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub erp: Option<crate::integrations::ErpJob>,
    /// A Library job: who saved it, and when (LB5). Set by the Library when
    /// it stores the job; None for a job file, and for a Library job saved
    /// before saves recorded it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<crate::shared::Saved>,
    /// The Library record this job is, at the revision it was read or last
    /// saved at (LB7). Set by the Library; a job file an agent opened from the
    /// Library keeps it, so its next save goes over that record while nobody
    /// else has saved it since. None for a job never in the Library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<crate::library::Record>,
    /// Library parts in the job, by name: the record each was added from or
    /// last saved to, at its revision then (LB7).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub library_parts: std::collections::BTreeMap<String, crate::library::Record>,
}

/// The layout a job needs: 2 only when a part sits at a free angle.
pub fn layout_for(job: &JobFile) -> u32 {
    if job.sheets.iter().flatten().any(|p| !p.spin.is_none()) { VERSION } else { VERSION_QUARTER_TURNS }
}

/// The job as written to its file, marked with the layout it needs.
pub fn to_text(job: &JobFile) -> Result<String, String> {
    let v = layout_for(job);
    if job.version == v {
        return serde_json::to_string_pretty(job).map_err(|e| e.to_string());
    }
    serde_json::to_string_pretty(&JobFile { version: v, ..job.clone() }).map_err(|e| e.to_string())
}

pub fn save(path: &std::path::Path, job: &JobFile) -> Result<(), String> {
    let text = to_text(job)?;
    // Write next to it, then rename, so a crash never leaves half a file
    // (as Expert rewrites LANTEKIN.CNI through a temporary file, LK3); the
    // temporary file is this writer's own (LB3).
    crate::shared::write_file(path, text.as_bytes(), None)
}

pub fn load(path: &std::path::Path) -> Result<JobFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let j: JobFile = serde_json::from_str(&text).map_err(|e| format!("not a job file ({e})"))?;
    if j.format != FORMAT {
        return Err("not a sheet-nesting job file".into());
    }
    if j.version > VERSION {
        return Err(format!("made by a newer version (layout {} > {VERSION})", j.version));
    }
    Ok(j)
}

#[cfg(test)]
mod jobfile_tests {
    use super::*;
    use crate::nest::Shape;

    #[test]
    fn a_job_survives_saving_and_reopening() {
        let geo = crate::dxf::parse(crate::dxf::SAMPLE_DXF).unwrap().0;
        let mut tech = crate::nest::PartTech::default();
        tech.micro_joints = 3;
        let parts = vec![
            Part { name: "Plate".into(), shape: Shape::PlateHole, w: 120.0, h: 80.0, qty: 2, rotate: true, color: 3, tech: Some(tech), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] },
            Part { name: "Drawn".into(), shape: Shape::Custom, w: geo.w, h: geo.h, qty: 1, rotate: false, color: 4, tech: None, custom: Some(std::sync::Arc::new(geo)), rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] },
        ];
        let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 1, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() };
        let sheets = vec![vec![Placed { part: 0, x: 5.0, y: 5.0, w: 120.0, h: 80.0, turns: 1, mirror: true, spin: Default::default() }]];
        let cuts = vec![vec![CutLine { pts: vec![[300.0, 0.0], [300.0, 1000.0]], kind: crate::cutoff::CutKind::Remnant }]];
        // The sheet is an L-shaped remnant from stock.
        let outline = crate::stock::Outline { outer: vec![[0.0, 0.0], [2000.0, 0.0], [2000.0, 1000.0], [800.0, 1000.0], [800.0, 400.0], [0.0, 400.0]], holes: vec![] };
        let formats = vec![crate::nest::SheetFormat { w: 2000.0, h: 1000.0, outline: Some(outline), stock_ref: "2-16".into(), remnant: true, made: vec![], made_from: 0 }];
        let j = JobFile { format: FORMAT.into(), version: VERSION, name: "Frames".into(), week: "W12".into(), parts, spec, sheets, formats, held: Default::default(), cut_lines: cuts, method: "Modules".into(), manual_cuts: vec![vec![crate::cnc::ManualCut { placed: 0, contour: 0, start: [5.0, 5.0], reverse: true, whole: None }]], stale: true, booked: false, erp: None, saved: None, library: None, library_parts: Default::default() };
        let path = std::env::temp_dir().join("nestpro_test_job.snjob");
        save(&path, &j).unwrap();
        let k = load(&path).unwrap();
        assert_eq!((k.name.as_str(), k.week.as_str(), k.parts.len()), ("Frames", "W12", 2));
        assert_eq!(k.parts[0].tech.unwrap().micro_joints, 3);
        let (a, b) = (j.parts[1].custom.as_ref().unwrap(), k.parts[1].custom.as_ref().unwrap());
        assert_eq!(a.outline, b.outline);
        assert_eq!(a.holes.len(), b.holes.len());
        assert_eq!(k.sheets, j.sheets);
        assert_eq!(k.formats, j.formats);
        assert_eq!(k.cut_lines, j.cut_lines);
        assert_eq!((k.manual_cuts.clone(), k.stale), (j.manual_cuts.clone(), true));
        assert_eq!((k.spec.w, k.spec.material), (2000.0, 1));
        // A job saved before sheets had formats opens with the job's size.
        let mut old: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        old.as_object_mut().unwrap().remove("formats");
        std::fs::write(&path, old.to_string()).unwrap();
        let k = load(&path).unwrap();
        let mut r = crate::nest::NestResult { sheets: k.sheets, formats: k.formats, method: String::new(), tried: 0 };
        r.normalise(&k.spec);
        assert_eq!(r.formats, vec![crate::nest::SheetFormat::rect(2000.0, 1000.0)]);
        // Something else is refused.
        std::fs::write(&path, "{\"format\":\"other\"}").unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn only_a_job_with_a_part_at_a_free_angle_needs_the_new_layout() {
        // HM14: a part at 30° is written as layout 2, which older builds
        // refuse rather than cut at the wrong angle; quarter turns stay at 1.
        let part = Part { name: "P".into(), shape: Shape::Rectangle, w: 100.0, h: 40.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() };
        let quarter = Placed { x: 10.0, y: 10.0, ..crate::nest::oriented(&part, 0, 90.0, false) };
        let mut j = JobFile { format: FORMAT.into(), version: VERSION, name: "DEMO-001".into(), week: String::new(), parts: vec![part.clone()], spec, sheets: vec![vec![quarter]], formats: vec![], held: Default::default(), cut_lines: vec![], method: String::new(), manual_cuts: vec![], stale: false, booked: false, erp: None, saved: None, library: None, library_parts: Default::default() };
        let version = |j: &JobFile| serde_json::from_str::<serde_json::Value>(&to_text(j).unwrap()).unwrap()["version"].as_u64();
        assert_eq!(version(&j), Some(1));
        j.sheets[0].push(Placed { x: 300.0, y: 10.0, ..crate::nest::oriented(&part, 0, 30.0, false) });
        assert_eq!(version(&j), Some(2));
        let d = std::env::temp_dir().join(format!("nestpro_layout_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("DEMO-001.snjob");
        save(&path, &j).unwrap();
        assert_eq!(load(&path).unwrap().sheets, j.sheets);
        let _ = std::fs::remove_dir_all(&d);
    }
}
