//! The app's own library ("database"): products and jobs kept as one open,
//! readable file per record in a folder that can be local or shared on the
//! network, so a shop without Lantek has its own part and job database.
//! Laid out like Lantek's record folders (rule ST1:
//! `Products\<n/1000>\<n>`, `Nestings\<n/1000>\<n>` — 1000
//! records per folder), with an index for quick lists and search. Lantek
//! keeps the lists in SQL Server and the files encrypted; here both are
//! plain files.

use crate::jobfile::JobFile;
use crate::nest::Part;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "nestpro-library";
pub const VERSION: u32 = 1;
/// Records per sub-folder, as Lantek (n / 1000).
pub const PER_FOLDER: u64 = 1000;
pub const INDEX: &str = "index.json";

/// Automatic references, as Lantek makes them (rules RF2, RF3): a fixed text then
/// a 6-character counter from "000001" to "999999" — "JOB" for jobs,
/// "SHP" for sheet parts (also KIT, CMI, RWM, SHT, BEAM, BPRT…).
pub const JOB_PREFIX: &str = "JOB";
pub const PART_PREFIX: &str = "SHP";
pub const REF_DIGITS: usize = 6;
pub const REF_MAX: u64 = 999_999;

/// The automatic reference of a library record.
pub fn auto_ref(kind: Kind, id: u64) -> String {
    let p = match kind {
        Kind::Job => JOB_PREFIX,
        Kind::Product => PART_PREFIX,
    };
    format!("{p}{id:0w$}", w = REF_DIGITS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Product,
    Job,
}

impl Kind {
    fn folder(self) -> &'static str {
        match self {
            Kind::Product => "Products",
            Kind::Job => "Jobs",
        }
    }
    fn ext(self) -> &'static str {
        match self {
            Kind::Product => "json",
            Kind::Job => crate::jobfile::EXT,
        }
    }
}

/// One line of the index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub kind: Kind,
    pub id: u64,
    pub name: String,
    /// Jobs: their folder (week); products: a group, e.g. the customer.
    pub folder: String,
    pub material: usize,
    pub thickness: f64,
    pub w: f64,
    pub h: f64,
    /// Jobs: products and sheets; products: default quantity and 0.
    pub count_a: usize,
    pub count_b: usize,
    /// Seconds since 1970 of the last save.
    pub updated: u64,
    /// Products: set up with their own technology (leads, joints, quality)
    /// or texts — "preprogrammed".
    #[serde(default)]
    pub tech: bool,
}

#[derive(Serialize, Deserialize)]
struct Index {
    format: String,
    version: u32,
    entries: Vec<Entry>,
}

/// A product as stored: the part plus its material and thickness.
#[derive(Serialize, Deserialize)]
pub struct ProductFile {
    pub format: String,
    pub version: u32,
    pub part: Part,
    pub folder: String,
    pub material: usize,
    pub thickness: f64,
}

pub struct Library {
    pub dir: PathBuf,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

impl Library {
    /// Open (and set up, the first time) a library folder.
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("can't use {}: {e}", dir.display()))?;
        let lib = Library { dir: dir.to_path_buf() };
        if !dir.join(INDEX).exists() {
            lib.write_index(&lib.rescan())?;
        }
        Ok(lib)
    }

    /// Where a record's file is: <dir>\<Kind>\<id / 1000>\<id>.<ext>.
    pub fn path(&self, kind: Kind, id: u64) -> PathBuf {
        self.dir.join(kind.folder()).join((id / PER_FOLDER).to_string()).join(format!("{id}.{}", kind.ext()))
    }

    pub fn entries(&self) -> Vec<Entry> {
        std::fs::read_to_string(self.dir.join(INDEX))
            .ok()
            .and_then(|t| serde_json::from_str::<Index>(&t).ok())
            .map(|i| i.entries)
            .unwrap_or_else(|| self.rescan())
    }

    fn write_index(&self, entries: &[Entry]) -> Result<(), String> {
        let i = Index { format: FORMAT.into(), version: VERSION, entries: entries.to_vec() };
        write_atomic(&self.dir.join(INDEX), &serde_json::to_string_pretty(&i).map_err(|e| e.to_string())?)
    }

    /// Rebuild the index from the record files (after copying folders in,
    /// or if the index is lost).
    pub fn rescan(&self) -> Vec<Entry> {
        let mut out = vec![];
        for kind in [Kind::Product, Kind::Job] {
            let root = self.dir.join(kind.folder());
            let Ok(subs) = std::fs::read_dir(&root) else { continue };
            for sub in subs.flatten() {
                let Ok(files) = std::fs::read_dir(sub.path()) else { continue };
                for f in files.flatten() {
                    let p = f.path();
                    let Some(id) = p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u64>().ok()) else { continue };
                    let updated = f.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
                    let e = match kind {
                        Kind::Job => crate::jobfile::load(&p).ok().map(|j| job_entry(id, &j, updated)),
                        Kind::Product => std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<ProductFile>(&t).ok()).map(|pf| product_entry(id, &pf, updated)),
                    };
                    out.extend(e);
                }
            }
        }
        out.sort_by_key(|e| (e.kind as u8, e.id));
        out
    }

    /// Re-read the index from the record files and save it.
    pub fn rebuild_index(&self) -> Result<usize, String> {
        let e = self.rescan();
        self.write_index(&e)?;
        Ok(e.len())
    }

    /// A new record number: one more than the highest, taken by creating
    /// its file so two people saving at once on a shared folder can't get
    /// the same number (Lantek takes the next number under a lock:
    /// "SELECT TOP 1 RecID … WITH (UPDLOCK) ORDER BY RecID DESC").
    fn new_id(&self, kind: Kind) -> Result<u64, String> {
        // Highest number + 1, or 1 for the first (RF1).
        let mut id = self.entries().iter().filter(|e| e.kind == kind).map(|e| e.id).max().unwrap_or(0) + 1;
        loop {
            if id > REF_MAX {
                return Err(format!("the reference counter is full ({}{REF_MAX})", if kind == Kind::Job { JOB_PREFIX } else { PART_PREFIX }));
            }
            let p = self.path(kind, id);
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                Ok(_) => return Ok(id),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => id += 1,
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    fn upsert(&self, e: Entry) -> Result<(), String> {
        let mut all = self.entries();
        all.retain(|x| !(x.kind == e.kind && x.id == e.id));
        all.push(e);
        all.sort_by_key(|e| (e.kind as u8, e.id));
        self.write_index(&all)
    }

    /// Save a job (a new record, or over `id`). Returns its number.
    pub fn save_job(&self, job: &JobFile, id: Option<u64>) -> Result<u64, String> {
        let id = match id {
            Some(i) => i,
            None => self.new_id(Kind::Job)?,
        };
        crate::jobfile::save(&self.path(Kind::Job, id), job)?;
        self.upsert(job_entry(id, job, now()))?;
        Ok(id)
    }

    pub fn load_job(&self, id: u64) -> Result<JobFile, String> {
        crate::jobfile::load(&self.path(Kind::Job, id))
    }

    /// Save a product. Returns its number.
    pub fn save_product(&self, part: &Part, folder: &str, material: usize, thickness: f64, id: Option<u64>) -> Result<u64, String> {
        let id = match id {
            Some(i) => i,
            None => self.new_id(Kind::Product)?,
        };
        // A library product is reusable stock, not one order's part: its ERP orders stay with the job.
        let mut part = part.clone();
        part.erp.clear();
        let pf = ProductFile { format: FORMAT.into(), version: VERSION, part, folder: folder.into(), material, thickness };
        write_atomic(&self.path(Kind::Product, id), &serde_json::to_string_pretty(&pf).map_err(|e| e.to_string())?)?;
        self.upsert(product_entry(id, &pf, now()))?;
        Ok(id)
    }

    pub fn load_product(&self, id: u64) -> Result<ProductFile, String> {
        let t = std::fs::read_to_string(self.path(Kind::Product, id)).map_err(|e| e.to_string())?;
        serde_json::from_str(&t).map_err(|e| format!("not a library product ({e})"))
    }

    /// The product stored under this reference. As in Lantek
    /// (rule PK1: only `PrdRef` is checked in PPRR_PPRR_00000100) the reference alone is the key;
    /// material and thickness are properties of that one product.
    pub fn find_product(&self, name: &str) -> Option<u64> {
        self.entries().into_iter().find(|e| e.kind == Kind::Product && e.name == name).map(|e| e.id)
    }

    pub fn delete(&self, kind: Kind, id: u64) -> Result<(), String> {
        let _ = std::fs::remove_file(self.path(kind, id));
        let mut all = self.entries();
        all.retain(|x| !(x.kind == kind && x.id == id));
        self.write_index(&all)
    }
}

fn job_entry(id: u64, j: &JobFile, updated: u64) -> Entry {
    Entry { kind: Kind::Job, id, name: j.name.clone(), folder: j.week.clone(), material: j.spec.material, thickness: j.spec.thickness, w: j.spec.w, h: j.spec.h, count_a: j.parts.len(), count_b: j.sheets.len(), updated, tech: false }
}

fn product_entry(id: u64, p: &ProductFile, updated: u64) -> Entry {
    let (w, h) = p.part.bbox();
    let tech = p.part.tech.is_some() || !p.part.texts.is_empty();
    Entry { kind: Kind::Product, id, name: p.part.name.clone(), folder: p.folder.clone(), material: p.material, thickness: p.thickness, w, h, count_a: p.part.qty as usize, count_b: 0, updated, tech }
}

/// Thickness filters below this mean "any thickness" (rule PK3: Lantek adds
/// `AND (DIS_Thickness = '%f')` only when the value is >= 0.0).
pub const THICKNESS_FILTER_MIN: f64 = 0.0;
/// Decimals the thickness is written with in that filter ('%f').
pub const THICKNESS_FILTER_DECIMALS: i32 = 6;

/// Lantek's product filter (rule PK3): the same material when one is
/// given, the same thickness when it is not negative, and the name.
pub fn product_matches(e: &Entry, material: Option<usize>, thickness: f64, needle: &str) -> bool {
    let k = 10f64.powi(THICKNESS_FILTER_DECIMALS);
    material.is_none_or(|m| e.material == m)
        && (thickness < THICKNESS_FILTER_MIN || (e.thickness * k).round() == (thickness * k).round())
        && (needle.is_empty() || e.name.to_lowercase().contains(needle) || e.folder.to_lowercase().contains(needle))
}

/// The app's folder in the user's documents: Documents\Sheet Nesting
/// (%USERPROFILE% on Windows, $HOME elsewhere).
pub fn documents_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Documents")
        .join("Sheet Nesting")
}

/// The default library folder: Documents\Sheet Nesting\Library.
pub fn default_dir() -> PathBuf {
    documents_dir().join("Library")
}

#[cfg(test)]
mod library_tests {
    use super::*;
    use crate::nest::{Placed, Shape, SheetSpec};

    #[test]
    fn stores_products_and_jobs_like_record_folders() {
        let dir = std::env::temp_dir().join(format!("nestpro_lib_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let lib = Library::open(&dir).unwrap();
        let part = Part { name: "Bracket".into(), shape: Shape::LBracket, w: 120.0, h: 80.0, qty: 5, rotate: true, color: 1, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        assert_eq!((auto_ref(Kind::Job, 1), auto_ref(Kind::Product, 1234)), ("JOB000001".to_string(), "SHP001234".to_string()));
        let p1 = lib.save_product(&part, "ACME", 0, 3.0, None).unwrap();
        let p2 = lib.save_product(&part, "ACME", 1, 2.0, None).unwrap();
        // The reference is the key: the first "Bracket" is found whatever its material.
        assert_eq!(lib.find_product("Bracket"), Some(p1));
        assert_eq!(lib.find_product("Plate"), None);
        let es = lib.entries();
        let e2 = es.iter().find(|e| e.id == p2 && e.kind == Kind::Product).unwrap();
        assert!(product_matches(e2, Some(1), 2.0, ""));
        assert!(!product_matches(e2, Some(1), 5.0, ""));
        assert!(product_matches(e2, Some(1), -1.0, "brack"));
        assert!(product_matches(e2, None, -1.0, "acme"));
        assert!(!product_matches(e2, Some(0), 2.0, ""));
        let mut tp = part.clone();
        tp.tech = Some(Default::default());
        let p3 = lib.save_product(&tp, "ACME", 2, 1.0, None).unwrap();
        assert!(lib.entries().iter().find(|e| e.kind == Kind::Product && e.id == p3).unwrap().tech);
        lib.delete(Kind::Product, p3).unwrap();
        assert_eq!((p1, p2), (1, 2));
        assert_eq!(lib.path(Kind::Product, 1234), dir.join("Products").join("1").join("1234.json"));
        let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        let job = JobFile { format: crate::jobfile::FORMAT.into(), version: crate::jobfile::VERSION, name: "Frames".into(), week: "W12".into(), parts: vec![part.clone()], spec, sheets: vec![vec![Placed { part: 0, x: 5.0, y: 5.0, w: 120.0, h: 80.0, turns: 0, mirror: false }]], formats: vec![], held: Default::default(), cut_lines: vec![], method: String::new(), manual_cuts: vec![], stale: false, booked: false, erp: None };
        let j = lib.save_job(&job, None).unwrap();
        // Saving again over the same number keeps one record.
        lib.save_job(&job, Some(j)).unwrap();
        let e = lib.entries();
        assert_eq!(e.iter().filter(|x| x.kind == Kind::Job).count(), 1);
        assert_eq!(e.iter().filter(|x| x.kind == Kind::Product).count(), 2);
        assert_eq!(lib.load_job(j).unwrap().sheets.len(), 1);
        assert_eq!(lib.load_product(p2).unwrap().thickness, 2.0);
        // The index can be rebuilt from the files alone.
        std::fs::remove_file(dir.join(INDEX)).unwrap();
        assert_eq!(lib.rebuild_index().unwrap(), 3);
        lib.delete(Kind::Product, p1).unwrap();
        assert_eq!(lib.entries().iter().filter(|x| x.kind == Kind::Product).count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
