//! Readers for part drawings in formats other than DXF (JOB-1). Each reader turns a
//! file into a `dxf::Drawing`, so joining, outline/holes, layer roles and part data
//! from the file name work exactly as for DXF.

pub mod dstv;
pub mod dwg;
pub mod dxfread;
pub mod essi;
pub mod geo;
pub mod iges;
pub mod mec;
pub mod pdf;

use std::collections::BTreeMap;
use std::path::Path;

/// Part data a file carries besides its geometry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub name: Option<String>,
    pub material: Option<String>,
    pub thickness: Option<f64>,
    pub qty: Option<u32>,
}

/// One part's drawing read from a file (some formats hold several parts).
#[derive(Debug, Clone)]
pub struct ReadPart {
    pub drawing: crate::dxf::Drawing,
    pub meta: Meta,
    /// Layers that are marking (not cut through) by default.
    pub mark_layers: Vec<String>,
}

/// The drawing formats the app imports.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Dxf,
    Dwg,
    Mec,
    Iges,
    Pdf,
    Essi,
    Dstv,
    Geo,
}

impl Format {
    pub const ALL: [Format; 8] = [Format::Dxf, Format::Dwg, Format::Mec, Format::Iges, Format::Pdf, Format::Essi, Format::Dstv, Format::Geo];

    /// What the file picker and messages call it.
    pub fn label(self) -> &'static str {
        match self {
            Format::Dxf => "DXF drawing",
            Format::Dwg => "DWG drawing (AutoCAD)",
            Format::Mec => "Lantek part (MEC)",
            Format::Iges => "IGES drawing",
            Format::Pdf => "PDF drawing (vector)",
            Format::Essi => "ESSI cutting program",
            Format::Dstv => "DSTV plate (NC1)",
            Format::Geo => "GEO drawing (Trumpf)",
        }
    }

    /// File extensions, lower case.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Format::Dxf => &["dxf"],
            Format::Dwg => &["dwg"],
            Format::Mec => &["mec"],
            Format::Iges => &["igs", "iges"],
            Format::Pdf => &["pdf"],
            Format::Essi => &["cnc", "esi", "ess", "essi", "mpg", "los", "dat"],
            Format::Dstv => &["nc1", "nc"],
            Format::Geo => &["geo"],
        }
    }

    /// The operation-argument name (`format=` for agents).
    pub fn key(self) -> &'static str {
        match self {
            Format::Dxf => "dxf",
            Format::Dwg => "dwg",
            Format::Mec => "mec",
            Format::Iges => "iges",
            Format::Pdf => "pdf",
            Format::Essi => "essi",
            Format::Dstv => "dstv",
            Format::Geo => "geo",
        }
    }

    pub fn from_key(k: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.key().eq_ignore_ascii_case(k) || f.extensions().iter().any(|e| e.eq_ignore_ascii_case(k)))
    }

    fn from_ext(ext: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)))
    }

    /// Recognise a file by its first bytes, whatever its extension.
    pub fn sniff(bytes: &[u8]) -> Option<Format> {
        let head: String = bytes.iter().take(400).map(|&c| c as char).collect();
        let first = head.lines().next().unwrap_or("").trim();
        if bytes.starts_with(b"%PDF") {
            return Some(Format::Pdf);
        }
        if bytes.len() > 6 && &bytes[..4] == b"AC10" && bytes[4..6].iter().all(u8::is_ascii_digit) {
            return Some(Format::Dwg);
        }
        if first.starts_with("0 FILEMECLT") {
            return Some(Format::Mec);
        }
        // IGES: 80-column lines, the first in the Start section (column 73 = 'S').
        if head.lines().next().is_some_and(|l| l.trim_end_matches('\r').len() == 80 && l.as_bytes().get(72) == Some(&b'S')) {
            return Some(Format::Iges);
        }
        if first.starts_with("#~") {
            return Some(Format::Geo);
        }
        if first == "ST" || first.starts_with("ST ") || first.starts_with("** ") && head.lines().any(|l| l.trim() == "ST") {
            return Some(Format::Dstv);
        }
        let lines: Vec<&str> = head.lines().map(str::trim).take(4).collect();
        if lines.first() == Some(&"0") && lines.get(1) == Some(&"SECTION") {
            return Some(Format::Dxf);
        }
        None
    }
}

/// Which format a file is: by its extension, checked against its content.
pub fn detect(path: &Path, bytes: &[u8]) -> Option<Format> {
    let by_ext = path.extension().and_then(|e| e.to_str()).and_then(Format::from_ext);
    match (by_ext, Format::sniff(bytes)) {
        // The content wins when it's unmistakable (a DWG saved as .dxf, a DXF as .cnc…).
        (_, Some(s)) => Some(s),
        (e, None) => e,
    }
}

/// Every extension the import picker offers, in both cases.
pub fn picker_extensions() -> Vec<String> {
    Format::ALL.iter().flat_map(|f| f.extensions().iter()).flat_map(|e| [e.to_string(), e.to_uppercase()]).collect()
}

/// True when the path looks like a drawing the app imports (by extension).
pub fn is_drawing(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).and_then(Format::from_ext).is_some()
}

/// Choices for reading a file beyond its format.
#[derive(Debug, Clone, Default)]
pub struct ReadOpts {
    /// The file's extension (picks the ESSI control dialect, ES3).
    pub ext: String,
    /// ESSI control dialect by name (ESSI, BURNY, ESAB, MESSER, VULCAN); default by extension.
    pub essi_dialect: Option<String>,
    /// PDF drawing scale: real size = drawing size × scale (PD2; default 1).
    pub pdf_scale: Option<f64>,
    /// PDF pages side by side in one drawing, or one part each (PD3).
    pub pdf_pages: Option<pdf::Pages>,
    /// PDF layers by line colour, by the PDF's own layers, or one layer (PD4).
    pub pdf_layers: Option<pdf::Layers>,
}

/// The ESSI dialect a file is written in: the one asked for, else by its extension
/// (.MPG ESAB, .LOS MESSER, .DAT VULCAN), else plain ESSI (ES3).
fn essi_options(o: &ReadOpts) -> Result<essi::Options, String> {
    let name = match &o.essi_dialect {
        Some(d) => d.clone(),
        None => match o.ext.to_ascii_uppercase().as_str() {
            "MPG" => "ESAB".into(),
            "LOS" => "MESSER".into(),
            "DAT" => "VULCAN".into(),
            _ => "ESSI".into(),
        },
    };
    let dialect = essi::Dialect::named(&name).ok_or_else(|| format!("'{name}' is not an ESSI dialect ({})", essi::Dialect::NAMES.join(", ")))?;
    Ok(essi::Options { dialect, ..Default::default() })
}

/// Read a drawing in a known format. `cfg` gives the Lantek folder (where the keys
/// of encrypted Lantek files are found, MF7).
pub fn read_bytes(fmt: Format, bytes: &[u8], cfg: &crate::settings::AppSettings, opts: &ReadOpts) -> Result<Vec<ReadPart>, String> {
    // A reader that trips over a damaged file gives a message instead of closing
    // the program (as DW15 for every format).
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_unguarded(fmt, bytes, cfg, opts)))
        .unwrap_or_else(|_| Err(format!("This {} could not be read (it looks damaged). Open it in the program that made it and save it again, or export it as DXF.", fmt.label())))
}

fn read_unguarded(fmt: Format, bytes: &[u8], cfg: &crate::settings::AppSettings, opts: &ReadOpts) -> Result<Vec<ReadPart>, String> {
    // A DXF or DWG that doesn't say its unit is in the app's unit (Settings →
    // Nesting), as the original reads it in its system units (A089 = 0; IU1).
    let unit_mm = cfg.nesting.units.factor();
    match fmt {
        Format::Dxf => crate::dxf::read_in(&String::from_utf8_lossy(bytes), unit_mm).map(|drawing| vec![ReadPart { drawing, meta: Meta::default(), mark_layers: vec![] }]),
        Format::Dwg => dwg::read(bytes, &dwg::Options { unit_mm, ..Default::default() }),
        Format::Mec => {
            let mut o = mec::Options::default();
            // The Lantek folder from Settings is searched before the default one
            // (after the key file); the same folder twice is searched once (MF7).
            if !cfg.lantek_dir.trim().is_empty() {
                o.keys.insert(1, mec::KeySource::Install(std::path::PathBuf::from(cfg.lantek_dir.trim())));
            }
            mec::read(bytes, &o)
        }
        Format::Iges => iges::read(bytes, &Default::default()),
        Format::Pdf => {
            let mut o = pdf::Options::default();
            if let Some(s) = opts.pdf_scale {
                if !(s.is_finite() && s > 0.0) {
                    return Err(format!("the PDF scale must be a number above 0 (it was {s})"));
                }
                o.scale = s;
            }
            o.pages = opts.pdf_pages.unwrap_or(o.pages);
            o.layers = opts.pdf_layers.unwrap_or(o.layers);
            pdf::read(bytes, &o)
        }
        Format::Essi => essi::read(bytes, &essi_options(opts)?),
        Format::Dstv => dstv::read(bytes, &Default::default()),
        Format::Geo => geo::read(bytes, &Default::default()),
    }
}

/// Read a drawing file in any format the app knows (`format` forces one).
pub fn read_file(path: &Path, format: Option<Format>, cfg: &crate::settings::AppSettings) -> Result<Vec<ReadPart>, String> {
    read_file_with(path, format, cfg, ReadOpts::default())
}

/// `read_file` with reading choices (the extension is filled in from the path).
pub fn read_file_with(path: &Path, format: Option<Format>, cfg: &crate::settings::AppSettings, opts: ReadOpts) -> Result<Vec<ReadPart>, String> {
    read_path(path, format, cfg, opts).map(|(_, parts)| parts)
}

/// `read_file_with`, also saying which format the file was read as.
pub fn read_path(path: &Path, format: Option<Format>, cfg: &crate::settings::AppSettings, mut opts: ReadOpts) -> Result<(Format, Vec<ReadPart>), String> {
    opts.ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
    let bytes = std::fs::read(path).map_err(|e| format!("{}: couldn't read ({e})", path.display()))?;
    let fmt = format.or_else(|| detect(path, &bytes)).ok_or_else(|| {
        let kinds: Vec<&str> = Format::ALL.iter().map(|f| f.label()).collect();
        format!("{}: not a drawing this app can read ({})", path.display(), kinds.join(", "))
    })?;
    let parts = read_bytes(fmt, &bytes, cfg, &opts).map_err(|e| format!("{}: {e}", path.display()))?;
    if parts.is_empty() {
        return Err(format!("{}: no part drawing in this {}", path.display(), fmt.label()));
    }
    Ok((fmt, parts))
}

/// One part of an imported file, with the file's name and the part's own name.
#[derive(Debug, Clone)]
pub struct Incoming {
    /// The file's name without extension (part data from the file name is read from this).
    pub file: String,
    /// The part's name: the file's name for a one-part file, else numbered or
    /// as the file names its parts.
    pub name: String,
    pub part: ReadPart,
}

/// The name each part of a file gets: the file's name, numbered when the file
/// holds several parts, unless the file names its parts.
pub fn part_names(stem: &str, parts: &[ReadPart]) -> Vec<String> {
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| match &p.meta.name {
            // A page of a PDF keeps the file's name: "<file> page 2".
            Some(n) if n.starts_with("page ") && parts.len() > 1 => format!("{stem} {n}"),
            Some(n) if !n.trim().is_empty() && parts.len() > 1 => n.trim().to_string(),
            _ if parts.len() > 1 => format!("{stem}-{}", i + 1),
            _ => stem.to_string(),
        })
        .collect()
}

/// The parts of a file, each with the file's name and its own name.
pub fn incoming(stem: &str, parts: Vec<ReadPart>) -> Vec<Incoming> {
    part_names(stem, &parts).into_iter().zip(parts).map(|(name, part)| Incoming { file: stem.to_string(), name, part }).collect()
}

/// Starting layer roles: cut, except the format's marking layers.
pub fn default_roles<'a>(parts: impl IntoIterator<Item = &'a ReadPart>) -> BTreeMap<String, crate::dxf::LayerRole> {
    let mut roles = BTreeMap::new();
    for p in parts {
        for l in p.drawing.layers().keys() {
            let r = if p.mark_layers.iter().any(|m| m == l) { crate::dxf::LayerRole::Mark } else { crate::dxf::LayerRole::Cut };
            roles.entry(l.clone()).or_insert(r);
        }
    }
    roles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_recognised_by_content() {
        assert_eq!(Format::sniff(b"%PDF-1.4\n"), Some(Format::Pdf));
        assert_eq!(Format::sniff(b"AC1032\0\0\0"), Some(Format::Dwg));
        assert_eq!(Format::sniff(b"0 FILEMECLT 7.00\n"), Some(Format::Mec));
        assert_eq!(Format::sniff(b"0\nSECTION\n2\nENTITIES\n"), Some(Format::Dxf));
        assert_eq!(Format::sniff(b"#~1\n"), Some(Format::Geo));
        assert_eq!(Format::sniff(b"ST\n** DEMO-001\n"), Some(Format::Dstv));
        let iges = format!("{:<72}S{:>7}\n", "Demo part", 1);
        assert_eq!(Format::sniff(iges.as_bytes()), Some(Format::Iges));
        // A DWG named .dxf is still read as DWG.
        assert_eq!(detect(Path::new("a.dxf"), b"AC1015\0\0"), Some(Format::Dwg));
        assert_eq!(detect(Path::new("a.CNC"), b"%\n39+1\n"), Some(Format::Essi));
        // ES3: a VULCAN program (.DAT) is an ESSI program in the VULCAN dialect.
        assert_eq!(detect(Path::new("SHEETS.DAT"), b" JOB: DEMO\n"), Some(Format::Essi));
        assert!(is_drawing(Path::new("SHEETS.DAT")));
        let o = essi_options(&ReadOpts { ext: "DAT".into(), ..Default::default() }).unwrap();
        assert_eq!(o.dialect.name, "VULCAN");
        assert_eq!(Format::from_key("igs"), Some(Format::Iges));
    }

    #[test]
    fn a_drawing_without_a_unit_is_read_in_the_apps_unit() {
        let mut cfg = crate::settings::AppSettings::default();
        let size = |cfg: &crate::settings::AppSettings, text: &str| {
            let p = read_bytes(Format::Dxf, text.as_bytes(), cfg, &ReadOpts::default()).unwrap();
            crate::dxf::build(&p[0].drawing, &Default::default()).unwrap().0.w
        };
        let mm = size(&cfg, crate::dxf::SAMPLE_DXF);
        cfg.nesting.units = crate::units::LenUnit::Inch;
        assert!((size(&cfg, crate::dxf::SAMPLE_DXF) - mm * 25.4).abs() < 1e-6, "no unit: the app's (inch)");
        // A drawing that says mm stays mm whatever the app's unit.
        let said = format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n{}", crate::dxf::SAMPLE_DXF);
        assert!((size(&cfg, &said) - mm).abs() < 1e-6);
    }

    #[test]
    fn several_parts_in_one_file_are_numbered() {
        let p = |n: Option<&str>| ReadPart { drawing: Default::default(), meta: Meta { name: n.map(Into::into), ..Default::default() }, mark_layers: vec![] };
        assert_eq!(part_names("DEMO", &[p(Some("X"))]), vec!["DEMO"]);
        assert_eq!(part_names("DEMO", &[p(None), p(Some("B-2"))]), vec!["DEMO-1", "B-2"]);
        assert_eq!(part_names("DEMO", &[p(Some("page 1")), p(Some("page 2"))]), vec!["DEMO page 1", "DEMO page 2"]);
    }
}
