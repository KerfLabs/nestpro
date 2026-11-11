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
    /// The number alone, as precise as a mm value with `mm_dec` decimals:
    /// inches get two decimals more (at most 4); trailing zeros dropped.
    /// For sizes, positions and lengths in lists ("2000" / "78.74").
    pub fn num_dp(self, mm: f64, mm_dec: usize) -> String {
        let d = match self {
            LenUnit::Mm => mm_dec,
            LenUnit::Inch => (mm_dec + 2).min(4),
        };
        let s = format!("{:.d$}", mm / self.factor());
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        if s == "-0" { "0".into() } else { s }
    }
    /// A length with its unit, as precise as `num_dp`: "1814 mm" / "71.42 in".
    pub fn len(self, mm: f64, mm_dec: usize) -> String {
        format!("{} {}", self.num_dp(mm, mm_dec), self.suffix())
    }
    /// Two lengths without the unit: "2000 × 1000" / "48 × 96".
    pub fn dims(self, w: f64, h: f64, mm_dec: usize) -> String {
        format!("{} × {}", self.num_dp(w, mm_dec), self.num_dp(h, mm_dec))
    }
    /// Two lengths with the unit: "2000 × 1000 mm" / "48 × 96 in".
    pub fn size(self, w: f64, h: f64, mm_dec: usize) -> String {
        format!("{} {}", self.dims(w, h, mm_dec), self.suffix())
    }
    /// An area given in mm²: "1.076 m²" / "11.58 ft²".
    pub fn area(self, mm2: f64) -> String {
        format!("{} {}", self.area_num(mm2), self.area_suffix())
    }
    /// The area's number alone: "1.076" (m²) / "11.58" (ft²).
    pub fn area_num(self, mm2: f64) -> String {
        match self {
            LenUnit::Mm => format!("{:.3}", mm2 / 1e6),
            LenUnit::Inch => format!("{:.2}", mm2 / (MM_PER_INCH * 12.0).powi(2)),
        }
    }
    /// The unit of areas in column headings: "m²" / "ft²".
    pub fn area_suffix(self) -> &'static str {
        match self {
            LenUnit::Mm => "m²",
            LenUnit::Inch => "ft²",
        }
    }
    /// A sheet thickness: "3 mm" / "0.1875 in (3/16\")"; a steel gauge is
    /// named when the value is one (TH2).
    pub fn thickness(self, mm: f64) -> String {
        match self {
            LenUnit::Mm => self.show(mm),
            LenUnit::Inch => match gauge_of(mm) {
                Some(g) => format!("{} ({g} ga)", self.show(mm)),
                None => self.show_long(mm),
            },
        }
    }
    /// Common sheet thicknesses in this unit, mm, with their names (TH2):
    /// metric plate in mm; steel gauges and fractions in inches.
    pub fn thickness_presets(self) -> Vec<(f64, String)> {
        match self {
            LenUnit::Mm => [0.5, 0.8, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0].into_iter().map(|t| (t, self.show(t))).collect(),
            LenUnit::Inch => {
                let mut v: Vec<(f64, String)> = STEEL_GAUGES.iter().map(|&(g, inch)| (inch * MM_PER_INCH, format!("{g} ga  ·  {inch} in"))).collect();
                for (n, d) in [(3u32, 16u32), (1, 4), (5, 16), (3, 8), (1, 2), (5, 8), (3, 4), (1, 1)] {
                    let inch = n as f64 / d as f64;
                    let name = if d == 1 { format!("{n} in") } else { format!("{n}/{d}\"  ·  {} in", LenUnit::Inch.num(inch * MM_PER_INCH)) };
                    v.push((inch * MM_PER_INCH, name));
                }
                v
            }
        }
    }
    /// The sheet grid's step, mm: 100 mm, or 4 inches.
    pub fn grid_step(self) -> f64 {
        match self {
            LenUnit::Mm => 100.0,
            LenUnit::Inch => 4.0 * MM_PER_INCH,
        }
    }
    /// Sheet sizes offered on the Sheet tab, mm (TH3): metric sheets, or the
    /// common inch plate sizes (4 × 8 ft, 5 × 10 ft …).
    pub fn sheet_presets(self) -> &'static [(f64, f64)] {
        match self {
            LenUnit::Mm => &[(2000.0, 1000.0), (2500.0, 1250.0), (3000.0, 1500.0), (4000.0, 2000.0), (6000.0, 2000.0)],
            LenUnit::Inch => &[(2438.4, 1219.2), (3048.0, 1219.2), (3048.0, 1524.0), (3657.6, 1524.0), (3657.6, 1828.8), (6096.0, 2438.4)],
        }
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
        // A steel gauge: "10ga", "10 ga", "10 gauge" (TH2).
        if let Some(g) = t.strip_suffix("gauge").or_else(|| t.strip_suffix("ga")) {
            let g: u32 = g.trim().parse().ok()?;
            return STEEL_GAUGES.iter().find(|x| x.0 == g).map(|x| x.1 * MM_PER_INCH);
        }
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

/// Steel sheet gauges (manufacturers' standard gauge), inches (TH2).
pub const STEEL_GAUGES: [(u32, f64); 7] = [(20, 0.0359), (18, 0.0478), (16, 0.0598), (14, 0.0747), (12, 0.1046), (11, 0.1196), (10, 0.1345)];

/// The steel gauge a thickness in mm is, when it is one (to 0.0001").
pub fn gauge_of(mm: f64) -> Option<u32> {
    STEEL_GAUGES.iter().find(|(_, inch)| (inch * MM_PER_INCH - mm).abs() < 0.0001 * MM_PER_INCH).map(|g| g.0)
}

/// A drag-or-type box for a length kept in mm, shown and typed in `unit`.
pub fn drag_len(ui: &mut egui::Ui, mm: &mut f64, unit: LenUnit, max_mm: f64) -> egui::Response {
    drag_len_in(ui, mm, unit, 0.0, max_mm)
}

/// `drag_len` between `min_mm` and `max_mm`. A value already outside stays
/// until it is changed (a job may carry one).
pub fn drag_len_in(ui: &mut egui::Ui, mm: &mut f64, unit: LenUnit, min_mm: f64, max_mm: f64) -> egui::Response {
    let speed = match unit {
        LenUnit::Mm => 0.2,
        LenUnit::Inch => MM_PER_INCH / 64.0,
    };
    ui.add(
        egui::DragValue::new(mm)
            .range(min_mm..=max_mm)
            .clamp_existing_to_range(false)
            .speed(speed)
            .custom_formatter(move |v, _| unit.show(v))
            .custom_parser(move |s| unit.parse(s)),
    )
}

/// A typed sheet thickness (decimal or fraction, either unit) with a ▾ list
/// of common ones beside it (TH1, TH2). True when it changed.
pub fn thickness_box(ui: &mut egui::Ui, id: &str, mm: &mut f64, unit: LenUnit) -> bool {
    let before = *mm;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        drag_len_in(ui, mm, unit, MIN_THICKNESS, MAX_THICKNESS)
            .on_hover_text("Type the thickness: 0.25, 1/4, 3/16 or 10ga in inches, or 6mm / 0.25in in either unit");
        egui::ComboBox::from_id_salt(id).width(18.0).selected_text("").show_ui(ui, |ui| {
            ui.set_min_width(150.0);
            for (t, name) in unit.thickness_presets() {
                if ui.selectable_label((t - *mm).abs() < 1e-6, name).clicked() {
                    *mm = t;
                }
            }
        });
    });
    (*mm - before).abs() > 1e-9
}

/// The same as `thickness_box` with the common thicknesses as buttons below
/// the box instead of a ▾ list — for places that are a pop-up themselves.
pub fn thickness_inline(ui: &mut egui::Ui, mm: &mut f64, unit: LenUnit) -> bool {
    let before = *mm;
    drag_len_in(ui, mm, unit, MIN_THICKNESS, MAX_THICKNESS).on_hover_text("Type the thickness: 0.25, 1/4, 3/16 or 10ga in inches, or 6mm / 0.25in in either unit");
    ui.horizontal_wrapped(|ui| {
        for (t, name) in unit.thickness_presets() {
            let short = name.split("  ·").next().unwrap_or(&name).to_string();
            if ui.selectable_label((t - *mm).abs() < 1e-6, short).on_hover_text(name).clicked() {
                *mm = t;
            }
        }
    });
    (*mm - before).abs() > 1e-9
}

/// Thinnest and thickest sheet the app takes, mm.
pub const MIN_THICKNESS: f64 = 0.1;
pub const MAX_THICKNESS: f64 = 300.0;

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

    #[test]
    fn sizes_and_thicknesses_follow_the_unit() {
        // NST-16: sizes, positions and areas in the chosen unit.
        assert_eq!(LenUnit::Mm.size(2000.0, 1000.0, 0), "2000 × 1000 mm");
        assert_eq!(LenUnit::Inch.size(2438.4, 1219.2, 0), "96 × 48 in");
        assert_eq!(LenUnit::Inch.len(1814.0, 0), "71.42 in");
        assert_eq!(LenUnit::Mm.len(1814.0, 0), "1814 mm");
        assert_eq!(LenUnit::Mm.area(1_076_000.0), "1.076 m²");
        assert_eq!(LenUnit::Inch.area(304.8 * 304.8 * 2.0), "2.00 ft²");
        // TH1: thickness typed as a fraction, decimal or gauge.
        let i = LenUnit::Inch;
        assert!((i.parse("3/16").unwrap() - 4.7625).abs() < 1e-9);
        assert!((i.parse("10ga").unwrap() - 0.1345 * 25.4).abs() < 1e-9);
        assert!((LenUnit::Mm.parse("10 gauge").unwrap() - 0.1345 * 25.4).abs() < 1e-9);
        assert_eq!(i.parse("13ga"), None);
        assert_eq!(i.thickness(4.7625), "0.1875 in (3/16\")");
        assert_eq!(i.thickness(0.1345 * 25.4), "0.1345 in (10 ga)");
        assert_eq!(LenUnit::Mm.thickness(3.0), "3 mm");
        // TH2/TH3: the offered lists are in the unit.
        assert!(i.thickness_presets().iter().any(|(t, n)| (t - 6.35).abs() < 1e-9 && n.starts_with("1/4\"")));
        assert_eq!(i.sheet_presets()[0], (2438.4, 1219.2));
    }
}
