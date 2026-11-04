//! Lengths shown and typed in millimetres or inches. The app keeps every
//! length in millimetres inside; this is only how they are shown and read
//! back (Settings → Nesting → Units). Inches may be typed as decimals or
//! fractions ("1/4", "1 1/2", "0.25"), and either unit can be typed with its
//! suffix ("6 mm", "1/4in", "1/4\"").

use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LenUnit {
    #[default]
    Mm,
    Inch,
}

pub const MM_PER_INCH: f64 = 25.4;

impl LenUnit {
    pub fn factor(self) -> f64 {
        match self {
            LenUnit::Mm => 1.0,
            LenUnit::Inch => MM_PER_INCH,
        }
    }
    pub fn suffix(self) -> &'static str {
        match self {
            LenUnit::Mm => "mm",
            LenUnit::Inch => "in",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            LenUnit::Mm => "Millimetres",
            LenUnit::Inch => "Inches",
        }
    }
    /// The number alone: mm to 2 decimals, inches to 4, trailing zeros dropped.
    pub fn num(self, mm: f64) -> String {
        let v = mm / self.factor();
        let d = match self {
            LenUnit::Mm => 2,
            LenUnit::Inch => 4,
        };
        let s = format!("{v:.d$}");
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        if s == "-0" { "0".into() } else { s }
    }
    /// "6.35 mm" / "0.25 in".
    pub fn show(self, mm: f64) -> String {
        format!("{} {}", self.num(mm), self.suffix())
    }
    /// Like `show`, with the fraction for inch values on a 1/64" step ("0.25 in (1/4\")").
    pub fn show_long(self, mm: f64) -> String {
        match (self, fraction(mm / MM_PER_INCH)) {
            (LenUnit::Inch, Some(f)) if !f.contains('/') => self.show(mm),
            (LenUnit::Inch, Some(f)) => format!("{} ({f}\")", self.show(mm)),
            _ => self.show(mm),
        }
    }
    /// The same length in the other unit, as a hint next to a box: "= 6.35 mm · 1/4\"" or "= 0.25 in".
    pub fn other(self, mm: f64) -> String {
        match self {
            LenUnit::Inch => match fraction(mm / MM_PER_INCH).filter(|f| f.contains('/')) {
                Some(f) => format!("= {} · {f}\"", LenUnit::Mm.show(mm)),
                None => format!("= {}", LenUnit::Mm.show(mm)),
            },
            LenUnit::Mm => format!("= {}", LenUnit::Inch.show(mm)),
        }
    }
    /// A typed length, in mm. Bare numbers are in this unit; a suffix
    /// (mm, in, ") picks the unit; inches may be fractions ("1 1/2").
    pub fn parse(self, s: &str) -> Option<f64> {
        let t = s.trim().to_ascii_lowercase();
        let (body, unit) = if let Some(b) = t.strip_suffix("mm") {
            (b, LenUnit::Mm)
        } else if let Some(b) = t.strip_suffix("in").or_else(|| t.strip_suffix('"')) {
            (b, LenUnit::Inch)
        } else {
            (t.as_str(), self)
        };
        let v = parse_mixed(body.trim())?;
        v.is_finite().then_some(v * unit.factor())
    }
}

/// "1 1/2", "-3/8", "0.25", "2" → the number.
fn parse_mixed(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r.trim()),
        None => (false, s),
    };
    let frac = |f: &str| -> Option<f64> {
        let (a, b) = f.split_once('/')?;
        let (a, b): (f64, f64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
        (b != 0.0).then_some(a / b)
    };
    let v = match s.split_once(char::is_whitespace) {
        Some((whole, f)) if f.contains('/') => whole.trim().parse::<f64>().ok()? + frac(f)?,
        _ if s.contains('/') => frac(s)?,
        _ => s.parse().ok()?,
    };
    Some(if neg { -v } else { v })
}

/// The inch value as a fraction on a 1/64" step ("1/4", "1 3/8", "2"), when it is one.
pub fn fraction(inches: f64) -> Option<String> {
    let n = (inches * 64.0).round();
    if (inches * 64.0 - n).abs() > 1e-6 || n < 0.0 {
        return None;
    }
    let n = n as u64;
    let (whole, mut num, mut den) = (n / 64, n % 64, 64u64);
    while num > 0 && num % 2 == 0 {
        num /= 2;
        den /= 2;
    }
    Some(match (whole, num) {
        (w, 0) => format!("{w}"),
        (0, _) => format!("{num}/{den}"),
        (w, _) => format!("{w} {num}/{den}"),
    })
}

/// A drag-or-type box for a length kept in mm, shown and typed in `unit`.
pub fn drag_len(ui: &mut egui::Ui, mm: &mut f64, unit: LenUnit, max_mm: f64) -> egui::Response {
    let speed = match unit {
        LenUnit::Mm => 0.2,
        LenUnit::Inch => MM_PER_INCH / 64.0,
    };
    ui.add(
        egui::DragValue::new(mm)
            .range(0.0..=max_mm)
            .speed(speed)
            .custom_formatter(move |v, _| unit.show(v))
            .custom_parser(move |s| unit.parse(s)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inches_are_typed_as_fractions_or_decimals() {
        let i = LenUnit::Inch;
        assert_eq!(i.parse("1/4"), Some(6.35));
        assert_eq!(i.parse("0.25"), Some(6.35));
        assert!((i.parse("1 1/2").unwrap() - 38.1).abs() < 1e-9);
        assert_eq!(i.parse("1/4\""), Some(6.35));
        assert_eq!(i.parse("5mm"), Some(5.0));
        assert_eq!(LenUnit::Mm.parse("1/4in"), Some(6.35));
        assert_eq!(LenUnit::Mm.parse("6.35"), Some(6.35));
        assert_eq!(i.parse("1/0"), None);
        assert_eq!(i.parse("abc"), None);
    }

    #[test]
    fn lengths_show_in_the_chosen_unit() {
        assert_eq!(LenUnit::Inch.show(6.35), "0.25 in");
        assert_eq!(LenUnit::Inch.show_long(6.35), "0.25 in (1/4\")");
        assert_eq!(LenUnit::Inch.show_long(25.4), "1 in");
        assert_eq!(LenUnit::Mm.show(5.0), "5 mm");
        assert_eq!(LenUnit::Mm.show(6.35), "6.35 mm");
        assert_eq!(fraction(1.375).as_deref(), Some("1 3/8"));
        assert_eq!(LenUnit::Inch.other(6.35), "= 6.35 mm · 1/4\"");
        assert_eq!(LenUnit::Mm.other(6.35), "= 0.25 in");
        assert_eq!(fraction(0.3), None);
    }
}
