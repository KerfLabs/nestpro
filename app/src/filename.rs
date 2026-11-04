//! Part data from the drawing's file name (Lantek Expert v44, NewFeatures 4.1 /
//! 4.3; dialog "Text definition in the file name", FN1–FN3).
//! A format such as `[Reference]_[Quantity]_[Material]` names the fields in
//! the file name; the text between fields is the separator; `*` is text that
//! is skipped. A separator may not contain `<>:"/\|?*` (the original refuses
//! such a separator with a message listing those characters, FN2).

/// Characters a separator may not contain (the original's message).
pub const BAD_SEPARATOR_CHARS: &str = "<>:\"/\\|?*";

/// The fields a file name can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Reference,
    Quantity,
    Material,
    /// Not in the original's example; added for sheet-metal shops (estimated).
    Thickness,
    /// `*`: text that is skipped.
    Ignore,
}

impl Field {
    pub const ALL: [Field; 5] = [Field::Reference, Field::Quantity, Field::Material, Field::Thickness, Field::Ignore];
    pub fn tag(self) -> &'static str {
        match self {
            Field::Reference => "[Reference]",
            Field::Quantity => "[Quantity]",
            Field::Material => "[Material]",
            Field::Thickness => "[Thickness]",
            Field::Ignore => "*",
        }
    }
}

/// A parsed format: fields with the separator that follows each (empty for
/// the last one).
#[derive(Clone, Debug, PartialEq)]
pub struct Format {
    pub items: Vec<(Field, String)>,
}

/// What a file name gave.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct NameData {
    pub reference: Option<String>,
    pub quantity: Option<u32>,
    pub material: Option<String>,
    pub thickness: Option<f64>,
}

pub fn valid_separator(s: &str) -> bool {
    !s.chars().any(|c| BAD_SEPARATOR_CHARS.contains(c))
}

/// Parse `[Reference]_[Quantity]_[Material]` (case-insensitive tags, `*`
/// for skipped text). Two fields must be split by a separator.
pub fn parse_format(f: &str) -> Result<Format, String> {
    let mut items: Vec<(Field, String)> = vec![];
    let mut rest = f.trim();
    let mut lead = String::new();
    while !rest.is_empty() {
        let lower = rest.to_lowercase();
        let found = Field::ALL.iter().find(|fl| lower.starts_with(&fl.tag().to_lowercase()));
        if let Some(&fl) = found {
            if !lead.is_empty() {
                if items.is_empty() {
                    return Err(format!("text \"{lead}\" before the first field"));
                }
                if !valid_separator(&lead) {
                    return Err(format!("the separator \"{lead}\" cannot contain any of {BAD_SEPARATOR_CHARS}"));
                }
                items.last_mut().unwrap().1 = std::mem::take(&mut lead);
            } else if !items.is_empty() {
                return Err("two fields need a separator between them".into());
            }
            items.push((fl, String::new()));
            rest = &rest[fl.tag().len()..];
        } else {
            let c = rest.chars().next().unwrap();
            lead.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    if !lead.is_empty() {
        return Err(format!("text \"{lead}\" after the last field"));
    }
    if items.is_empty() {
        return Err("no field — use [Reference], [Quantity], [Material], [Thickness] or *".into());
    }
    Ok(Format { items })
}

/// Read a file name (without its extension) with a format. `None` when the
/// name doesn't follow it (a separator is missing or a number isn't one).
pub fn read(format: &Format, stem: &str) -> Option<NameData> {
    let mut d = NameData::default();
    let mut rest = stem;
    for (k, (field, sep)) in format.items.iter().enumerate() {
        let last = k + 1 == format.items.len();
        let value = if last || sep.is_empty() {
            let v = rest;
            rest = "";
            v
        } else {
            let at = rest.find(sep.as_str())?;
            let v = &rest[..at];
            rest = &rest[at + sep.len()..];
            v
        };
        let value = value.trim();
        match field {
            Field::Reference => d.reference = Some(value.to_string()).filter(|s| !s.is_empty()),
            Field::Quantity => d.quantity = Some(value.parse::<u32>().ok()?),
            Field::Material => d.material = Some(value.to_string()).filter(|s| !s.is_empty()),
            Field::Thickness => d.thickness = Some(value.replace(',', ".").trim_end_matches("mm").parse::<f64>().ok()?),
            Field::Ignore => {}
        }
    }
    Some(d)
}

#[cfg(test)]
mod filename_tests {
    use super::*;

    #[test]
    fn reads_reference_quantity_and_material_from_the_name() {
        let f = parse_format("[Reference]_[Quantity]_[Material]").unwrap();
        let d = read(&f, "BRACKET-12_40_S235").unwrap();
        assert_eq!((d.reference.as_deref(), d.quantity, d.material.as_deref()), (Some("BRACKET-12"), Some(40), Some("S235")));
        // Skipped text and other separators.
        let f = parse_format("*-[Reference] x[Quantity]").unwrap();
        let d = read(&f, "JOB881-PLATE A x3").unwrap();
        assert_eq!((d.reference.as_deref(), d.quantity), (Some("PLATE A"), Some(3)));
        let f = parse_format("[Reference]_[Thickness]mm").unwrap_err();
        assert!(f.contains("after the last field"));
        let d = read(&parse_format("[Reference]_[Thickness]").unwrap(), "P1_6,35").unwrap();
        assert_eq!(d.thickness, Some(6.35));
        // Names that don't follow the format are left alone.
        assert_eq!(read(&parse_format("[Reference]_[Quantity]").unwrap(), "PLATE"), None);
        assert_eq!(read(&parse_format("[Reference]_[Quantity]").unwrap(), "PLATE_many"), None);
    }

    #[test]
    fn separators_follow_the_originals_rule() {
        assert!(parse_format("[Reference]/[Quantity]").unwrap_err().contains("cannot contain"));
        assert!(parse_format("[Reference][Quantity]").is_err());
        assert!(valid_separator(" - ") && !valid_separator("a?b"));
    }
}

/// Longest product reference the import builds (FN5: 40 characters).
pub const REF_MAX_CHARS: usize = 40;

/// The reference an imported part gets (FN5):
/// `prefix + name + suffix`. When the three together are
/// longer than 40 characters, the name is cut to 40 − the prefix's length,
/// then to 40 − the suffix's length (the original keeps both cuts, so the
/// longer of the two wins).
pub fn part_reference(prefix: &str, name: &str, suffix: &str) -> String {
    let (p, n, s) = (prefix.chars().count(), name.chars().count(), suffix.chars().count());
    let mut base: String = name.to_string();
    if p + n + s > REF_MAX_CHARS {
        let keep = REF_MAX_CHARS.saturating_sub(p).min(REF_MAX_CHARS.saturating_sub(s));
        base = name.chars().take(keep).collect();
    }
    format!("{prefix}{base}{suffix}")
}

/// Is the part still named after its file? (FN6: the file's name without
/// folder and extension, compared exactly.)
#[cfg_attr(not(test), allow(dead_code))]
pub fn name_is_file_name(part_name: &str, path: &std::path::Path) -> bool {
    // Split on both separators: paths from Lantek files are Windows-style
    // even when the rebuild runs on Linux.
    let p = path.to_string_lossy();
    let file = p.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = match file.rfind('.') {
        Some(i) if i > 0 => &file[..i],
        _ => file,
    };
    stem == part_name
}

#[cfg(test)]
mod reference_tests {
    use super::*;

    #[test]
    fn references_are_prefix_name_suffix_cut_to_forty() {
        assert_eq!(part_reference("", "PLATE-7", ""), "PLATE-7");
        assert_eq!(part_reference("J88-", "PLATE-7", "-A"), "J88-PLATE-7-A");
        let long = "X".repeat(50);
        // Cut to 40 − 4 (prefix) then 40 − 2 (suffix): 36 characters of the name.
        assert_eq!(part_reference("J88-", &long, "-A"), format!("J88-{}-A", "X".repeat(36)));
        assert_eq!(part_reference("", &long, "").chars().count(), 40);
        assert!(name_is_file_name("PLATE-7", std::path::Path::new(r"C:\dxf\PLATE-7.dxf")));
        assert!(!name_is_file_name("plate-7", std::path::Path::new(r"C:\dxf\PLATE-7.dxf")));
    }
}
