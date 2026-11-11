//! Lantek's XML import files (C:\Lantek\Expert\xsd\Importers\DXF Import.xsd and
//! Parts Import.xsd) — the way other programs (an ERP, a script) hand parts to
//! Expert. Expert has no web/REST API; it reads these files:
//!
//! ```xml
//! <Import>
//!   <Product Reference="BRK-12" Machine="Laser 01" Material="Mild steel"
//!            Thickness="3" Quantity="40" Fich="C:\dxf\BRK-12.dxf">
//!     <Rotations>
//!       <Normal><Rotation0 value="true"/><Rotation90 value="true"/>…</Normal>
//!       <Symmetrics><Rotation0 value="false"/>…</Symmetrics>
//!     </Rotations>
//!     <AuxiliarData><Data DBField="DIS_UData1_Prt" value="…"/></AuxiliarData>
//!   </Product>
//! </Import>
//! ```
//!
//! `Fich` (DXF Import) is the drawing; without it (Parts Import) the part is
//! taken from the database — here, from the app's library by its reference.

use crate::nest::Rotations;

#[derive(Clone, Debug, PartialEq)]
pub struct XmlProduct {
    pub reference: String,
    pub name: String,
    pub machine: String,
    pub material: String,
    pub thickness: f64,
    pub quantity: u32,
    /// The drawing file (DXF Import's `Fich`).
    pub fich: Option<String>,
    /// Priority — a whole number in the original (XI6), not a boolean.
    pub priority: i32,
    /// UserData1…UserData8 attributes (the product's user data fields).
    pub user_data: [String; 8],
    /// `<Fills value hollows>`: a filler part (Some), and whether it may also go in holes.
    pub filler: Option<bool>,
    /// `<Rotations>`: Normal 0/90/180/270 and Symmetrics (mirrored) 0/90/180/270.
    pub rotations: Option<Rotations>,
    /// `<AuxiliarData><Data DBField value/>`.
    pub data: Vec<(String, String)>,
}

/// Priority given to a filler part without holes (XI9).
pub const FILLER_PRIORITY: i32 = 99;

/// Is this text a Lantek import file (root `<Import>` with `<Product>`s)?
pub fn is_import_xml(text: &str) -> bool {
    text.contains("<Import") && (text.contains("<Product") || text.contains("</Import>") || text.contains("<Import/>"))
}

/// The value of attribute `name` in a start tag's text (`"…"` or `'…'`), XML
/// entities decoded.
pub fn attr(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let k = rest.find(name)?;
        let before = rest[..k].chars().last();
        let after = &rest[k + name.len()..];
        let after_t = after.trim_start();
        if before.is_none_or(|c| c.is_whitespace()) && after_t.starts_with('=') {
            let v = after_t[1..].trim_start();
            let q = v.chars().next()?;
            if q == '"' || q == '\'' {
                let end = v[1..].find(q)?;
                return Some(unescape(&v[1..1 + end]));
            }
            return None;
        }
        rest = &rest[k + name.len()..];
    }
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// The original's boolean (XI8): "true" in any case,
/// or exactly "1"; anything else is false.
pub fn lantek_bool(v: &str) -> bool {
    v.eq_ignore_ascii_case("true") || v == "1"
}

/// C atoi as the original uses it for Quantity, Priority, IsDraft (XI6):
/// leading blanks, an optional sign, then digits up to the first non-digit;
/// no number = 0 ("40.7" → 40, "abc" → 0).
pub fn lantek_atoi(v: &str) -> i64 {
    let t = v.trim_start();
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let n: i64 = digits.bytes().take_while(u8::is_ascii_digit).fold(0i64, |a, d| a.saturating_mul(10).saturating_add(i64::from(d - b'0')));
    if neg { -n } else { n }
}

/// Element names are compared without case (XI7).
fn find_tag(block: &str, tag: &str) -> Option<usize> {
    block.to_ascii_lowercase().find(&tag.to_ascii_lowercase())
}

fn flag(block: &str, tag: &str) -> Option<bool> {
    let k = find_tag(block, &format!("<{tag}"))?;
    let end = block[k..].find('>')? + k;
    attr(&block[k..end], "value").map(|v| lantek_bool(&v))
}

fn section<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let a = find_tag(block, &format!("<{tag}"))?;
    let b = find_tag(&block[a..], &format!("</{tag}>")).map_or(block.len(), |e| a + e);
    Some(&block[a..b])
}

/// Read all `<Product>`s.
pub fn parse(text: &str) -> Result<Vec<XmlProduct>, String> {
    if !is_import_xml(text) {
        return Err("not a Lantek import file (no <Import> with <Product> entries)".into());
    }
    let mut out = vec![];
    let mut rest = text;
    while let Some(k) = rest.find("<Product") {
        let from = &rest[k..];
        // `<Products…` or similar isn't a Product tag.
        if !from[8..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/') {
            rest = &from[8..];
            continue;
        }
        let tag_end = from.find('>').ok_or("unclosed <Product> tag")?;
        let tag = &from[..tag_end];
        let self_closed = tag.ends_with('/');
        let body_end = if self_closed { tag_end + 1 } else { from.find("</Product>").map_or(from.len(), |e| e + "</Product>".len()) };
        let body = &from[..body_end];
        let reference = attr(tag, "Reference").unwrap_or_default();
        if reference.trim().is_empty() {
            return Err(format!("a <Product> has no Reference (product {})", out.len() + 1));
        }
        let num = |n: &str| attr(tag, n).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok());
        let rotations = section(body, "Rotations").map(|r| {
            let read = |sec: &str| {
                let s = section(r, sec).unwrap_or("");
                [flag(s, "Rotation0"), flag(s, "Rotation90"), flag(s, "Rotation180"), flag(s, "Rotation270")]
            };
            let (n, m) = (read("Normal"), read("Symmetrics"));
            Rotations { plain: n.map(|f| f.unwrap_or(true)), mirrored: m.map(|f| f.unwrap_or(false)) }
        });
        let mut data = vec![];
        if let Some(aux) = section(body, "AuxiliarData") {
            let mut r = aux;
            while let Some(d) = r.find("<Data") {
                let e = r[d..].find('>').map_or(r.len(), |e| d + e);
                let t = &r[d..e];
                if let (Some(f), Some(v)) = (attr(t, "DBField"), attr(t, "value")) {
                    data.push((f, v));
                }
                r = &r[e..];
            }
        }
        // Fills: value=false → not a filler; value=true, hollows=false → filler
        // with Priority 99; hollows=true → filler that may also go in holes
        // (XI9).
        let filler = find_tag(body, "<Fills").and_then(|k| {
            let e = body[k..].find('>').map_or(body.len(), |e| k + e);
            let t = &body[k..e];
            attr(t, "value").filter(|v| lantek_bool(v)).map(|_| attr(t, "hollows").is_some_and(|h| lantek_bool(&h)))
        });
        let mut priority = attr(tag, "Priority").map_or(0, |v| lantek_atoi(&v) as i32);
        if filler == Some(false) {
            priority = FILLER_PRIORITY;
        }
        let user_data: [String; 8] = std::array::from_fn(|k| attr(tag, &format!("UserData{}", k + 1)).unwrap_or_default());
        out.push(XmlProduct {
            reference: reference.trim().to_string(),
            name: attr(tag, "Name").unwrap_or_default(),
            machine: attr(tag, "Machine").unwrap_or_default(),
            material: attr(tag, "Material").unwrap_or_default().trim().to_string(),
            thickness: num("Thickness").unwrap_or(0.0),
            // Quantity: read as a whole number (XI6) — whole pieces, truncated.
            quantity: attr(tag, "Quantity").map_or(0, |q| lantek_atoi(&q).clamp(0, u32::MAX as i64) as u32),
            // The drawing: Fich, or its aliases ParRef and File (same field, XI5).
            fich: ["Fich", "ParRef", "File"].iter().find_map(|n| attr(tag, n)).filter(|f| !f.trim().is_empty()),
            priority,
            user_data,
            filler,
            rotations,
            data,
        });
        rest = &from[body_end..];
    }
    Ok(out)
}

/// The app's material for a name in the file: an exact name or code first
/// (Settings → Materials), then a name that contains it or the other way round.
pub fn material_index(name: &str, materials: &[&str], codes: &[(String, String)]) -> Option<usize> {
    let n = name.trim().to_lowercase();
    if n.is_empty() {
        return None;
    }
    if let Some(i) = materials.iter().position(|m| m.to_lowercase() == n) {
        return Some(i);
    }
    if let Some(i) = codes.iter().position(|(a, b)| a.to_lowercase() == n || b.to_lowercase() == n) {
        return Some(i);
    }
    materials.iter().position(|m| {
        let m = m.to_lowercase();
        m.contains(&n) || n.contains(&m)
    })
}

impl crate::App {
    /// Import a Lantek XML import file: one new job per material and thickness,
    /// in the folder "XML import", with each product's drawing (Fich) or the
    /// library product of the same reference.
    pub fn import_xml(&mut self, path: &std::path::Path) {
        let lib = self.library();
        let mut im = match crate::engine::imports::xml_import(&self.cfg, lib.as_ref().ok(), path, self.color_seq) {
            Ok(im) => im,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        // Products without a drawing come from the Library: say why it
        // wasn't looked in (LB11).
        if let Err(e) = &lib {
            im.notes.insert(0, format!("Library parts weren't looked up: {e}"));
        }
        let mut notes = im.notes;
        self.color_seq += im.added;
        let jobs = im.jobs.len();
        for xj in im.jobs {
            self.new_job_in_week("XML import".into());
            if let Some(n) = self.unique_job_name(&xj.name) {
                let i = self.cur_job;
                self.rename_job(i, &n);
            }
            match xj.material {
                Some(m) => self.spec.material = m,
                None if !xj.material_text.is_empty() => notes.push(format!("material '{}' isn't one of the app's — kept {}", xj.material_text, crate::cnc::MATERIALS[self.spec.material.min(2)])),
                None => {}
            }
            if xj.thickness > 0.0 {
                self.spec.thickness = xj.thickness;
            }
            self.parts.extend(xj.parts);
            self.sel_part = if self.parts.is_empty() { None } else { Some(0) };
            self.stale = true;
            self.refresh();
            self.store_job();
        }
        self.status = format!(
            "XML import {}: {} of {} product(s) into {jobs} new job(s) under 'XML import'{}",
            path.display(),
            im.added,
            im.products,
            if notes.is_empty() { ".".to_string() } else { format!(" — {}", notes.join("; ")) }
        );
    }
}

#[cfg(test)]
mod xmlimport_tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Import>
  <Product Reference="BRK-12" Name="Bracket" Machine="Laser 01" Material="Mild steel" Thickness="3" Quantity="40" Priority="true" Fich="brk.dxf">
    <Rotations>
      <Normal><Rotation0 value="true"/><Rotation90 value="false"/><Rotation180 value="true"/><Rotation270 value="false"/><Others value="false"/></Normal>
      <Symmetrics><Rotation0 value="true"/><Rotation90 value="false"/><Rotation180 value="false"/><Rotation270 value="false"/><Others value="false"/></Symmetrics>
    </Rotations>
    <AuxiliarData><Data DBField="DIS_UData1_Prt" value="Order 7 &amp; 8"/></AuxiliarData>
  </Product>
  <Product Reference="PL-5" Machine="Laser 01" Material="AISI304" Thickness="1,5" Quantity="2">
    <AuxiliarData><Data DBField="X" value="y"/></AuxiliarData>
  </Product>
</Import>"#;

    #[test]
    fn reads_lantek_import_products() {
        assert!(is_import_xml(SAMPLE));
        assert!(!is_import_xml("<NESTING><PART/></NESTING>"));
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.len(), 2);
        // Priority="true" is 0: the original reads it as a whole number (XI6).
        assert_eq!((p[0].reference.as_str(), p[0].quantity, p[0].thickness, p[0].fich.as_deref(), p[0].priority), ("BRK-12", 40, 3.0, Some("brk.dxf"), 0));
        let r = p[0].rotations.unwrap();
        assert_eq!((r.plain, r.mirrored), ([true, false, true, false], [true, false, false, false]));
        assert_eq!(p[0].data, vec![("DIS_UData1_Prt".to_string(), "Order 7 & 8".to_string())]);
        assert_eq!((p[1].thickness, p[1].fich.clone(), p[1].rotations), (1.5, None, None));
        assert!(parse("<Import><Product Machine=\"x\"/></Import>").is_err(), "Reference is required");
    }

    #[test]
    fn values_are_read_the_way_the_original_reads_them() {
        assert_eq!((lantek_atoi("40.7"), lantek_atoi(" 12pcs"), lantek_atoi("abc"), lantek_atoi("-3")), (40, 12, 0, -3));
        assert!(lantek_bool("TRUE") && lantek_bool("1") && !lantek_bool(" true") && !lantek_bool("yes") && !lantek_bool("01"));
        let x = r#"<Import>
  <Product Reference="F1" Material="Mild steel" Thickness="2" Quantity="7.9" Priority="3" ParRef="f1.dxf" UserData2="Line 4">
    <ROTATIONS><normal><rotation90 value="1"/></normal></ROTATIONS>
    <Fills value="true" hollows="false"/>
    <AuxiliarData/>
  </Product>
  <Product Reference="F2" Quantity="2" File="f2.dxf"><Fills value="TRUE" hollows="1"/><AuxiliarData/></Product>
</Import>"#;
        let p = parse(x).unwrap();
        assert_eq!((p[0].quantity, p[0].fich.as_deref(), p[0].user_data[1].as_str()), (7, Some("f1.dxf"), "Line 4"));
        assert!(p[0].rotations.unwrap().plain[1], "element names without case");
        assert_eq!((p[0].filler, p[0].priority), (Some(false), FILLER_PRIORITY));
        assert_eq!((p[1].filler, p[1].priority, p[1].fich.as_deref()), (Some(true), 0, Some("f2.dxf")));
    }

    #[test]
    fn matches_materials_by_name_or_code() {
        let mats = crate::cnc::MATERIALS;
        let codes = vec![("STEEL".to_string(), "S235".to_string()), ("INOX".to_string(), "AISI304".to_string()), ("ALU".to_string(), "AL5754".to_string())];
        assert_eq!(material_index("mild steel", &mats, &codes), Some(0));
        assert_eq!(material_index("AISI304", &mats, &codes), Some(1));
        assert_eq!(material_index("aluminium 5754", &mats, &codes), Some(2));
        assert_eq!(material_index("Copper", &mats, &codes), None);
    }

    #[test]
    fn an_import_file_makes_one_job_per_material_and_thickness() {
        let dir = std::env::temp_dir().join(format!("nestpro_xml_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n200\n20\n0\n10\n200\n20\n100\n10\n0\n20\n100\n0\nENDSEC\n0\nEOF\n";
        std::fs::write(dir.join("brk.dxf"), dxf).unwrap();
        std::fs::write(dir.join("order.xml"), SAMPLE).unwrap();
        let mut a = crate::App::new(None);
        a.cfg.library_dir = dir.join("lib").display().to_string();
        let n = a.jobs.len();
        a.import_xml(&dir.join("order.xml"));
        assert_eq!(a.jobs.len(), n + 2, "{}", a.status);
        let j = a.jobs.iter().find(|j| j.parts.iter().any(|p| p.name == "BRK-12")).expect(&a.status);
        assert_eq!(j.week, "XML import");
        let p = j.parts.iter().find(|p| p.name == "BRK-12").unwrap();
        assert_eq!((p.qty, p.w, p.h, p.rotate), (40, 200.0, 100.0, false));
        assert!(a.status.contains("not in the library"), "{}", a.status);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
