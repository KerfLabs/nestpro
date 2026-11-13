//! Part shapes and the automatic nesting engine (true-shape strip nesting like
//! Lantek's, see `bands.rs`; quarter turns and mirror, part gap and
//! sheet margin).

pub type P = [f64; 2];

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Shape {
    Rectangle,
    LBracket,
    Disc,
    Flange,
    Triangle,
    SlottedPlate,
    Strip,
    PlateHole,
    /// Shape read from a drawing file (see `Part::custom`).
    Custom,
}

impl Shape {
    pub const ALL: [Shape; 8] = [
        Shape::Rectangle,
        Shape::PlateHole,
        Shape::Strip,
        Shape::LBracket,
        Shape::Disc,
        Shape::Flange,
        Shape::Triangle,
        Shape::SlottedPlate,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Shape::Rectangle => "Rectangle",
            Shape::LBracket => "L-bracket",
            Shape::Disc => "Disc",
            Shape::Flange => "Flange plate",
            Shape::Triangle => "Gusset",
            Shape::SlottedPlate => "Slotted plate",
            Shape::Strip => "Drilled strip",
            Shape::PlateHole => "Plate with hole",
            Shape::Custom => "Imported drawing",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Part {
    pub name: String,
    pub shape: Shape,
    pub w: f64,
    pub h: f64,
    pub qty: u32,
    pub rotate: bool,
    pub color: usize,
    /// The part's own machining technology. `None` = the sheet's automatic
    /// technology (default lead-in, no micro-joints).
    pub tech: Option<PartTech>,
    /// Outline and holes read from a drawing (only for `Shape::Custom`).
    pub custom: Option<std::sync::Arc<crate::dxf::CustomGeo>>,
    /// Which of the original's eight rotations the nester may use.
    pub rots: Rotations,
    /// Texts on the part (marked, filled-in "special" texts, or plain notes).
    pub texts: Vec<PartText>,
    /// Grouping attribute (customer, order, delivery date…): parts of one group
    /// are nested together in their own zone of the sheet (v44 3.1.1).
    #[serde(default)]
    pub group: String,
    /// The ERP/MES orders this part is made for (job operations and how many
    /// each needs), when it was brought in from one (integrations::Demand).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub erp: Vec<crate::integrations::Demand>,
}

/// Allowed rotations: 0/90/180/270°, plain and mirrored — the eight flags the
/// original keeps per part (rule R1).
/// 90°/270° also need `Part::rotate`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Rotations {
    pub plain: [bool; 4],
    pub mirrored: [bool; 4],
}

impl Default for Rotations {
    fn default() -> Self {
        Rotations { plain: [true; 4], mirrored: [false; 4] }
    }
}

/// What a text on a part is. The original's text types (rule T1): type 1 is a
/// marking text, types 2–4 are special marking texts (filled in from the part's
/// data), anything else is a drawing note that isn't machined.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum TextKind {
    Note,
    Mark,
    Special,
}

impl TextKind {
    pub const ALL: [TextKind; 3] = [TextKind::Mark, TextKind::Special, TextKind::Note];
    pub fn label(self) -> &'static str {
        match self {
            TextKind::Note => "Note (not machined)",
            TextKind::Mark => "Marking text",
            TextKind::Special => "Special text (#REF, #QTY…)",
        }
    }
    /// The original's text type code.
    pub fn code(self) -> u8 {
        match self {
            TextKind::Note => 0,
            TextKind::Mark => 1,
            TextKind::Special => 2,
        }
    }
    /// Marked by the machine (rule T1: type 1, or a special type 2–4).
    pub fn machined(self) -> bool {
        let c = self.code();
        c == 1 || (2..=4).contains(&c)
    }
}

/// Marking texts on the sheet are kept level: the angle step the original snaps
/// them to (a step ≥ 359.9° = always 0°). The setting's default isn't in the
/// program; level text is the rebuild's choice.
pub const TEXT_ANGLE_STEP: f64 = 360.0;
/// A text is turned only when it is off by more than this (rule T10).
pub const TEXT_ANGLE_TOL: f64 = 0.1;

/// The original's text angle snap (rule T9): no step → as is;
/// a step of 359.9° or more → 0°; otherwise the nearest multiple of the step.
pub fn snap_text_angle(angle: f64, step: f64) -> f64 {
    if step <= 0.0 {
        angle
    } else if step >= 359.9 {
        0.0
    } else {
        norm_angle((angle / step).round() * step)
    }
}

/// Parameters a special text can use, written `#NAME`.
pub const TEXT_PARAMS: [(&str, &str); 5] = [("#REF", "part reference"), ("#QTY", "quantity"), ("#MAT", "material"), ("#THK", "thickness"), ("#NUM", "part number on the sheet")];

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PartText {
    pub text: String,
    /// Straight text: lower-left corner of the first character. Curved text:
    /// centre of the arc. Part coordinates.
    pub at: P,
    /// Capital height in mm.
    pub height: f64,
    /// Straight text: rotation in degrees (counter-clockwise). Curved text: the
    /// direction from the centre to the middle of the text.
    pub angle: f64,
    /// 0 = straight. Otherwise the baseline follows an arc of this radius (mm):
    /// positive = letters stand outward, reading clockwise (top of a circle);
    /// negative = letters stand inward, reading anticlockwise (bottom of a circle).
    pub curve: f64,
    pub kind: TextKind,
}

impl PartText {
    /// The text as it is marked: special texts fill in their `#` parameters.
    pub fn resolved(&self, part: &Part, material: &str, thickness: f64, number: usize) -> String {
        if self.kind != TextKind::Special {
            return self.text.clone();
        }
        self.text
            .replace("#REF", &part.name)
            .replace("#QTY", &part.qty.to_string())
            .replace("#MAT", material)
            .replace("#THK", &format!("{thickness}"))
            .replace("#NUM", &number.to_string())
    }
    /// Marking strokes on the sheet for a placed part, the way the original keeps
    /// texts readable (rules T8, T10): the text
    /// moves with the part but is never mirrored, and a straight text's angle is
    /// snapped to a multiple of `step` (rule T9; a step of 359.9° or more
    /// means always level) and turned about its middle when it is off by more
    /// than 0.1°. Curved texts keep following their arc.
    pub fn sheet_strokes(&self, text: &str, pl: &Placed, step: f64) -> Vec<Vec<P>> {
        let w = crate::font::width(text, self.height);
        let (s, c) = self.angle.to_radians().sin_cos();
        if self.curve.abs() > 1e-9 {
            let c2 = pl.map(self.at);
            let dir = pl.map([self.at[0] + c * 10.0, self.at[1] + s * 10.0]);
            let mid = (dir[1] - c2[1]).atan2(dir[0] - c2[0]).to_degrees();
            return crate::font::strokes_on_arc(text, c2, self.curve, mid, self.height);
        }
        let local = |x: f64, y: f64| [self.at[0] + x * c - y * s, self.at[1] + x * s + y * c];
        let m = pl.map(local(w / 2.0, self.height / 2.0));
        let a0 = pl.map(local(0.0, 0.0));
        let a1 = pl.map(local(w, 0.0));
        let actual = norm_angle((a1[1] - a0[1]).atan2(a1[0] - a0[0]).to_degrees());
        let snapped = snap_text_angle(actual, step);
        let a = if (snapped - actual).abs() > TEXT_ANGLE_TOL { snapped } else { actual };
        let (s2, c2) = a.to_radians().sin_cos();
        let at = [m[0] - (w / 2.0) * c2 + (self.height / 2.0) * s2, m[1] - (w / 2.0) * s2 - (self.height / 2.0) * c2];
        crate::font::strokes(text, at, self.height, a)
    }

    /// Marking strokes in part coordinates.
    pub fn strokes(&self, text: &str) -> Vec<Vec<P>> {
        if self.curve.abs() > 1e-9 {
            crate::font::strokes_on_arc(text, self.at, self.curve, self.angle, self.height)
        } else {
            crate::font::strokes(text, self.at, self.height, self.angle)
        }
    }
}

/// One of the machine's 12 numbered cut qualities: the sets of cutting
/// settings its technology table holds per quality (TT3). Saved as its number.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CutQuality(u8);

impl CutQuality {
    /// How many qualities a machine has.
    pub const COUNT: usize = 12;
    /// The quality of a cut contour that nothing else sets (CQ1, estimate:
    /// real LC5 programs cut outlines at 3, Fast on the usual laser table).
    pub const FALLBACK: CutQuality = CutQuality(3);

    /// Quality `n`, when the machine has one (1–12).
    pub fn new(n: usize) -> Option<CutQuality> {
        (1..=Self::COUNT).contains(&n).then_some(CutQuality(n as u8))
    }
    pub fn number(self) -> usize {
        self.0 as usize
    }
    /// Every quality, 1 to 12.
    pub fn all() -> impl Iterator<Item = CutQuality> {
        (1..=Self::COUNT as u8).map(CutQuality)
    }
    /// "Quality 3": the quality when the machine gives it no name (screens show
    /// the machine's names through `qualities::CutQualities::label`, CQ4).
    pub fn label(self) -> String {
        format!("Quality {}", self.0)
    }
    /// Cutting speed multiplier when the machine gives no speed at all (CQ3, estimate).
    pub fn speed_factor(self) -> f64 {
        match self.0 {
            1 => 0.7,
            2 => 0.85,
            _ => 1.0,
        }
    }
}

/// A saved cut quality: a number, or one of the app's old three names (CQ2).
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SavedQuality {
    Number(u64),
    Name(String),
}

impl SavedQuality {
    /// A size-range row's quality: Fine 1, Normal 2, Rough 3 (CQ2, estimate).
    fn quality(self) -> Result<CutQuality, String> {
        match self {
            SavedQuality::Number(n) => CutQuality::new(n as usize).ok_or_else(|| format!("cut quality {n} is not 1 to 12")),
            SavedQuality::Name(n) => match n.as_str() {
                "Fine" => Ok(CutQuality(1)),
                "Normal" => Ok(CutQuality(2)),
                "Rough" => Ok(CutQuality(3)),
                _ => Err(format!("unknown cut quality '{n}'")),
            },
        }
    }
}

impl serde::Serialize for CutQuality {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for CutQuality {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        <SavedQuality as serde::Deserialize>::deserialize(d)?.quality().map_err(serde::de::Error::custom)
    }
}

/// A part's own quality as saved: none (the size bands decide), a number, or
/// an old name, where Normal is none (it let the size bands decide), Fine 1
/// and Rough 3 (CQ2, estimate).
fn part_quality<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<CutQuality>, D::Error> {
    match <Option<SavedQuality> as serde::Deserialize>::deserialize(d)? {
        None => Ok(None),
        Some(SavedQuality::Name(n)) if n == "Normal" => Ok(None),
        Some(q) => q.quality().map(Some).map_err(serde::de::Error::custom),
    }
}

/// Technology applied to one part: lead-in/lead-out, micro-joints (small
/// uncut bridges that hold the part in the sheet) and cut quality.
#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct PartTech {
    pub lead_in: f64,
    pub lead_out: f64,
    pub micro_joints: u32,
    pub joint_width: f64,
    /// The part's own cut quality for every cut contour its size ranges don't
    /// set; None = the machine's size bands decide (CQ1).
    #[serde(default, deserialize_with = "part_quality")]
    pub quality: Option<CutQuality>,
    /// Shape of the lead-in (the original's lead-in/lead-out types).
    pub lead_type: LeadType,
    /// Start each contour at the valid corner nearest the chosen position
    /// (rule P2), with the lead on the corner's bisector.
    pub corner_start: bool,
    /// Where on the contour box the lead goes, 1–9 like a keypad (1 = bottom left,
    /// 5 = centre, 9 = top right). Original position code; 5 = a given point.
    pub position: u8,
    /// Small loops at sharp outer corners so the corner stays crisp.
    pub loops: bool,
    pub loop_size: f64,
    /// Lead-ins placed by picking an element: (contour index, point in part
    /// coordinates). Contours are numbered holes first, outer contour last.
    pub starts: [Option<(u8, P)>; 8],
    /// Micro-joints placed by picking an element.
    pub joint_picks: [Option<(u8, P)>; 12],
    /// Micro-joint type (the original's numbering, rule J1).
    pub joint_type: JointType,
    /// Length of each joint lead (lead-out before the gap, lead-in after it), mm.
    pub joint_lead: f64,
    /// Angle of the joint leads from the contour, degrees.
    pub joint_angle: f64,
    /// Cut quality by contour size (the original's automatic cut technology
    /// list, rule CT1): the first range holding a contour's equivalent
    /// radius sets that contour's quality.
    #[serde(default)]
    pub cut_ranges: [CutRange; 3],
    /// Cut past the contour's start by this much before the lead-out
    /// ("Solapamiento de las entradas y salidas", CT1 B156), mm.
    #[serde(default)]
    pub overlap: f64,
    /// Automatic micro-joints: one per this many mm of each contour, holes too
    /// (CT1 F150 "maximum distance between micro-joints"; rule AM1: F120 is not
    /// read); 0 = use `micro_joints` as the count on the outside.
    #[serde(default)]
    pub joint_every: f64,
    /// Joint lead-out distance (CT1 F002); `None` = the same as the lead-in
    /// (files saved before it existed).
    #[serde(default)]
    pub joint_lead_out: Option<f64>,
    /// Leads whose free end was moved by hand, one per contour (LE1–LE7).
    #[serde(default)]
    pub lead_edits: [Option<LeadEdit>; LEAD_EDITS],
}

/// How many contours of one part can have a lead moved by hand.
pub const LEAD_EDITS: usize = 16;

/// A lead moved by hand (LE1–LE3): the contour keeps its start and lead
/// record, and the lead-in's pierce and/or the lead-out's end sit where they
/// were dropped. Both are kept as a length from the contour and an angle
/// measured from the contour, so they follow the part when it is turned or
/// mirrored on a sheet.
#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct LeadEdit {
    /// Contour index, as `starts` counts them (cut holes first, outline last).
    pub contour: u8,
    /// Where the contour starts (the lead's foot), part coordinates.
    pub at: P,
    /// The lead record (0–7, by contour class) the lead was built from.
    pub rec: u8,
    /// Lead-in: (straight distance from the start to the pierce, mm; angle in
    /// degrees from the contour behind the start, turning into the scrap).
    pub lead_in: Option<(f64, f64)>,
    /// Lead-out: (straight distance from where the lead-out leaves the
    /// contour to its end, mm; angle from the contour ahead, into the scrap).
    pub lead_out: Option<(f64, f64)>,
}

/// One row of the automatic cut technology list (rule CT1): the technology
/// index and the lower and upper limit of the radius.
#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct CutRange {
    pub on: bool,
    /// From this equivalent radius (included), mm.
    pub min_r: f64,
    /// Up to this equivalent radius (not included), mm.
    pub max_r: f64,
    pub quality: CutQuality,
}

impl Default for CutRange {
    /// Off: the original's list is empty until the material table fills it.
    fn default() -> Self {
        CutRange { on: false, min_r: 0.0, max_r: SMALL_HOLE_RADIUS_EXAMPLE, quality: CutQuality(1) }
    }
}

/// The original's cut technology by size (rule CT1) multiplies the
/// contour area (m²) by this twice, takes |…| / π and its square root: the
/// radius of the circle with the same area, in mm.
pub const TECAUTO_SCALE: f64 = 1000.0;
/// Example upper limit for the "small holes" row (estimated, not in the code).
pub const SMALL_HOLE_RADIUS_EXAMPLE: f64 = 5.0;

/// Equivalent radius (mm) of a contour of `area_mm2`, as the original computes it.
pub fn equivalent_radius(area_mm2: f64) -> f64 {
    let area_m2 = area_mm2 / (TECAUTO_SCALE * TECAUTO_SCALE);
    ((area_m2 * TECAUTO_SCALE * TECAUTO_SCALE).abs() / std::f64::consts::PI).sqrt()
}

/// The quality for a contour (rule CT1): no row when the
/// first row starts above the radius; otherwise the rows with
/// `min − off ≤ r < max − off` match (the offset is the second argument ÷ 1000;
/// 0 here) and the lowest-numbered match wins (the matches are sorted).
pub fn quality_for_radius(ranges: &[CutRange], r: f64, off: f64) -> Option<CutQuality> {
    let on: Vec<&CutRange> = ranges.iter().filter(|c| c.on).collect();
    if on.first().is_none_or(|c| c.min_r > r) {
        return None;
    }
    on.into_iter().find(|c| c.min_r - off <= r && c.max_r - off > r).map(|c| c.quality)
}

/// A micro-joint may not sit this near a lead point or another joint
/// (rule MJ2): it is refused when
/// the distance between the points is ≤ its gap width + the other's size.
pub fn joint_too_near(p: P, width: f64, others: &[(P, f64)]) -> bool {
    others.iter().any(|&(q, size)| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt() <= width + size + JOINT_NEAR_TOL)
}

/// Tolerance of that comparison (a less-or-equal with tolerance; the caller's
/// value isn't known, estimated).
pub const JOINT_NEAR_TOL: f64 = 1e-6;

/// Lead shape. `Machine` takes the type, distances, angles and radii from the
/// machine's technology tables by contour class and quality (see `leads.rs`);
/// the others are one of the original's lead types with the part's own lead-in
/// and lead-out distances. The first three keep their saved names (old .snjob
/// files load unchanged).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum LeadType {
    /// Inclined lead-in and lead-out at 45° (type 5).
    Line,
    /// Straight then a quarter arc (laser lead, type 13 with angle 0).
    LineArc,
    /// Tangent arc in, inclined out (type 8).
    Arc,
    /// From the machine's tables, per contour class (automatic leads).
    Machine,
    /// Perpendicular in and out (type 1).
    Perpendicular,
    /// On a corner, extending its first side (type 10).
    Corner,
}

impl LeadType {
    pub const ALL: [LeadType; 6] = [LeadType::Machine, LeadType::Perpendicular, LeadType::Line, LeadType::Arc, LeadType::LineArc, LeadType::Corner];
    pub fn label(self) -> &'static str {
        match self {
            LeadType::Line => "Inclined",
            LeadType::LineArc => "Line + arc",
            LeadType::Arc => "Arc",
            LeadType::Machine => "Machine tables",
            LeadType::Perpendicular => "Perpendicular",
            LeadType::Corner => "Corner",
        }
    }
    /// The original's lead type number (0 for `Machine`: it varies by contour class).
    pub fn code(self) -> u8 {
        match self {
            LeadType::Line => 5,
            LeadType::LineArc => 13,
            LeadType::Arc => 8,
            LeadType::Machine => 0,
            LeadType::Perpendicular => 1,
            LeadType::Corner => 10,
        }
    }
}

impl Default for PartTech {
    fn default() -> Self {
        PartTech {
            lead_in: 5.0,
            lead_out: 2.0,
            micro_joints: 0,
            joint_width: 0.5,
            quality: None,
            lead_type: LeadType::Line,
            corner_start: false,
            position: 1,
            loops: false,
            loop_size: 2.0,
            starts: [None; 8],
            joint_picks: [None; 12],
            joint_type: JointType::Gap,
            joint_lead: JOINT_LEAD_DEFAULT,
            joint_angle: JOINT_ANGLE_DEFAULT,
            cut_ranges: Default::default(),
            overlap: 0.0,
            joint_every: 0.0,
            joint_lead_out: None,
            lead_edits: [None; LEAD_EDITS],
        }
    }
}

impl PartTech {
    /// Contour `ci`'s lead moved by hand, if any.
    pub fn lead_edit(&self, ci: usize) -> Option<LeadEdit> {
        self.lead_edits.iter().flatten().find(|e| e.contour as usize == ci).copied()
    }

    /// Put contour `ci`'s lead back to what the plan makes of it (LE6).
    /// Returns whether there was one.
    pub fn clear_lead_edit(&mut self, ci: usize) -> bool {
        let mut found = false;
        for e in self.lead_edits.iter_mut() {
            if e.is_some_and(|x| x.contour as usize == ci) {
                *e = None;
                found = true;
            }
        }
        found
    }
}

/// Joint leads, from the original's defaults (rules A2, J1): lead-in distance
/// D 10.0 off the contour at 45° (the first two values are the distances, the
/// later two the radii); the gap 0.5, or 99999.999 = "use B164".
pub const JOINT_LEAD_DEFAULT: f64 = 10.0;
pub const JOINT_ANGLE_DEFAULT: f64 = 45.0;
/// Joint leads reach out into the scrap at most this share of the part gap, so
/// they never cut into the neighbouring part (rebuild's own safety rule, used
/// only when the machine has no joint overlap test, CT1 F133 ≤ 0).
pub const JOINT_LEAD_REACH: f64 = 0.8;

/// Micro-joint types. The original numbers them (rule J2): 1, 2, 7, 8
/// are joints in a corner; 3–6 are joints with a lead-out and a lead-in;
/// 9–11 are a plain uncut gap without leads.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum JointType {
    /// Beam off across the gap and straight on (types 9–11).
    Gap,
    /// Lead out into the scrap, gap, lead back in, both leads perpendicular
    /// (type 3: the angle is not used).
    Leads,
    /// On a corner, straddling it, with inclined leads (type 1).
    Corner,
    /// Like `Leads` with both leads inclined at the joint angle (type 4).
    LeadsInclined,
}

impl JointType {
    pub const ALL: [JointType; 4] = [JointType::Gap, JointType::Leads, JointType::LeadsInclined, JointType::Corner];
    pub fn label(self) -> &'static str {
        match self {
            JointType::Gap => "Plain gap",
            JointType::Leads => "Leads, perpendicular",
            JointType::LeadsInclined => "Leads, inclined",
            JointType::Corner => "At corners, with leads",
        }
    }
    /// The original's type number this matches.
    pub fn code(self) -> i32 {
        match self {
            JointType::Gap => 9,
            JointType::Leads => 3,
            JointType::Corner => 1,
            JointType::LeadsInclined => 4,
        }
    }
    /// Our type for one of the original's numbers (CT1 F000): 1/2/7/8 corner,
    /// 3 perpendicular leads, 4–6 inclined leads, 9–11 plain gap.
    pub fn from_code(code: i32) -> Option<JointType> {
        match code {
            1 | 2 | 7 | 8 => Some(JointType::Corner),
            3 => Some(JointType::Leads),
            4..=6 => Some(JointType::LeadsInclined),
            9..=11 => Some(JointType::Gap),
            _ => None,
        }
    }
    /// The original's category (rule J2): 1 for types
    /// 1–2, 2 for 7–8, 4 for 3–6 and 9–11, 0 otherwise.
    pub fn category(code: i32) -> i32 {
        match code {
            1 | 2 => 1,
            7 | 8 => 2,
            3..=6 | 9..=11 => 4,
            _ => 0,
        }
    }
    /// Types 1, 2, 7 and 8 are joints in a corner (rule J2).
    pub fn in_corner(code: i32) -> bool {
        matches!(code, 1 | 2 | 7 | 8)
    }
    pub fn has_leads(self) -> bool {
        self != JointType::Gap
    }
}

/// What kind of drawing element a run of the contour is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElemKind {
    Line,
    Arc,
    Circle,
}

impl ElemKind {
    pub fn label(self) -> &'static str {
        match self {
            ElemKind::Line => "Line",
            ElemKind::Arc => "Arc",
            ElemKind::Circle => "Circle",
        }
    }
}

/// One selectable element of a part: a straight line, an arc (a run of short
/// segments without corners) or a whole circle.
#[derive(Clone, Debug)]
pub struct Element {
    pub contour: usize,
    pub kind: ElemKind,
    pub pts: Vec<P>,
    pub length: f64,
}

impl Element {
    /// Point halfway along the element — where the original puts a lead-in
    /// picked on an element (rule E4: length × 0.5).
    pub fn midpoint(&self) -> P {
        let half = self.length / 2.0;
        let mut acc = 0.0;
        for w in self.pts.windows(2) {
            let l = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
            if acc + l >= half && l > 0.0 {
                let t = (half - acc) / l;
                return [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t];
            }
            acc += l;
        }
        self.pts[0]
    }
}

/// Where a lead goes on a picked straight element (rule LP3): the pick's share of the element along it (distance from the
/// element's start to the projected pick ÷ length) at or below this puts the
/// lead near the start …
pub const LEAD_PICK_LOW: f64 = 0.33333;
/// … at or above this near the end, otherwise in the middle (rule LP3).
pub const LEAD_PICK_HIGH: f64 = 0.66666;
/// The element must be longer than this many tolerances plus the lead's
/// overlap (rule LP2).
pub const LEAD_ELEMENT_FACTOR: f64 = 4.0;
/// The linear tolerance passed in (the machine's: CF1 A005).
pub const LEAD_LINEAR_TOL: f64 = 0.01;

impl Element {
    /// The point where a lead picked at `pick` goes (rules LP2, LP3).
    /// Straight lines: refused unless `length > 4·tol + |overlap|`; then at
    /// `2·tol + overlap` from the start (first third), `length − 2·tol − overlap`
    /// (last third) or `(length − overlap) / 2` (middle). Arcs and circles, or
    /// no pick: the middle (rule E4), with the same length check.
    pub fn lead_point(&self, pick: Option<P>, overlap: f64, tol: f64) -> Option<P> {
        let w = overlap.abs();
        let l = self.length;
        if l <= tol * LEAD_ELEMENT_FACTOR + w {
            return None;
        }
        let (Some(p), ElemKind::Line) = (pick, self.kind) else { return Some(self.midpoint()) };
        let (a, b) = (self.pts[0], self.pts[self.pts.len() - 1]);
        let d = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
        let along = ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]).clamp(0.0, l);
        let r = along / l;
        let s = if r <= LEAD_PICK_LOW {
            tol + tol + w
        } else if r < LEAD_PICK_HIGH {
            (l - w) * 0.5
        } else {
            l - (tol + tol) - w
        };
        Some([a[0] + d[0] * s, a[1] + d[1] * s])
    }
}

/// Turn more than this (degrees) at a vertex ends an element.
pub const ELEMENT_CORNER_DEG: f64 = 20.0;

/// Split every contour of a part into elements (lines, arcs, circles).
pub fn elements_of(geo: &Geo) -> Vec<Element> {
    let mut out = vec![];
    let contours: Vec<&Vec<P>> = geo.holes.iter().chain(std::iter::once(&geo.outline)).collect();
    for (ci, pts) in contours.iter().enumerate() {
        let n = pts.len();
        if n < 2 {
            continue;
        }
        let d = |a: P, b: P| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let corner = |i: usize| {
            let (a, v, b) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            let u = [v[0] - a[0], v[1] - a[1]];
            let w = [b[0] - v[0], b[1] - v[1]];
            let lu = (u[0] * u[0] + u[1] * u[1]).sqrt().max(1e-12);
            let lw = (w[0] * w[0] + w[1] * w[1]).sqrt().max(1e-12);
            let dot = ((u[0] * w[0] + u[1] * w[1]) / (lu * lw)).clamp(-1.0, 1.0);
            dot.acos().to_degrees() > ELEMENT_CORNER_DEG
        };
        let corners: Vec<usize> = (0..n).filter(|&i| corner(i)).collect();
        if corners.is_empty() {
            let mut p: Vec<P> = pts.to_vec();
            p.push(pts[0]);
            let length = p.windows(2).map(|w| d(w[0], w[1])).sum();
            out.push(Element { contour: ci, kind: ElemKind::Circle, pts: p, length });
            continue;
        }
        for (k, &c0) in corners.iter().enumerate() {
            let c1 = corners[(k + 1) % corners.len()];
            let mut p = vec![pts[c0]];
            let mut i = c0;
            loop {
                i = (i + 1) % n;
                p.push(pts[i]);
                if i == c1 {
                    break;
                }
            }
            let length = p.windows(2).map(|w| d(w[0], w[1])).sum();
            let kind = if p.len() == 2 { ElemKind::Line } else { ElemKind::Arc };
            out.push(Element { contour: ci, kind, pts: p, length });
        }
    }
    out
}

/// Automatic micro-joints by contour size (rules P7, AM2): a contour is
/// *small* when its box is within `small` in both directions, *large* when it
/// reaches `large` in both, otherwise *medium*. Each class has a (fewest, most)
/// joint count, for parts (outer contours) and for holes; a value below 0 takes
/// `min` / `max` (CT1 F121 / F122). The pairs are (min, max), not counts
/// along X and Y.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AutoJoints {
    /// F136 / F137 and F138 / F139.
    pub small: [f64; 2],
    pub large: [f64; 2],
    /// [small, medium, large] × (min, max): parts F142/F143, F121/F122, F148/F149;
    pub parts: [[i32; 2]; 3],
    /// holes F140/F141, F144/F145, F146/F147.
    pub holes: [[i32; 2]; 3],
    /// F121 / F122: the fallback fewest and most.
    pub min: u32,
    pub max: u32,
}

impl Default for AutoJoints {
    /// The original's defaults (rule P7): small limit 0×0, large
    /// limit 99999.999 — every contour is medium — and 0 / 99999 joints.
    fn default() -> Self {
        AutoJoints { small: [0.0, 0.0], large: [99999.999, 99999.999], parts: [[-1, -1]; 3], holes: [[-1, -1]; 3], min: 0, max: 99999 }
    }
}

impl AutoJoints {
    /// (fewest, most) joints for a contour box of `w` × `h`.
    pub fn limits(&self, w: f64, h: f64, hole: bool) -> (u32, u32) {
        let small = w <= self.small[0] && h <= self.small[1];
        let large = w >= self.large[0] && h >= self.large[1];
        let k = if small { 0 } else if large { 2 } else { 1 };
        let pair = if hole { self.holes[k] } else { self.parts[k] };
        let lo = if pair[0] < 0 { self.min } else { pair[0] as u32 };
        let hi = if pair[1] < 0 { self.max } else { pair[1] as u32 };
        (lo, hi)
    }
    /// Joints on a contour of length `per` and box `w` × `h`, one per `every` mm
    /// within the size class's limits (see `joints::count`).
    pub fn count_for(&self, per: f64, every: f64, w: f64, h: f64, hole: bool) -> u32 {
        let (lo, hi) = self.limits(w, h, hole);
        crate::joints::count(per, every, lo, hi, crate::leads::LIN_TOL).0
    }
}

impl PartTech {
    /// How many technology items the part carries (shown in the warning, like the
    /// count the original passes to its warning dialog).
    pub fn items(&self) -> u32 {
        1 + u32::from(self.lead_out > 0.0)
            + self.micro_joints
            + u32::from(self.quality.is_some())
            + self.starts.iter().flatten().count() as u32
            + self.joint_picks.iter().flatten().count() as u32
            + self.lead_edits.iter().flatten().count() as u32
    }
}

/// Geometry of a part in local coordinates (0..w, 0..h), y up.
pub struct Geo {
    pub outline: Vec<P>,
    /// Convex pieces that together fill the outline (for drawing).
    pub fills: Vec<Vec<P>>,
    /// Inner contours (all convex).
    pub holes: Vec<Vec<P>>,
}

pub fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Vec<P> {
    // At least fine enough for the drawing tolerance on a big circle (BL38).
    let n = n.max(crate::dxf::arc_segments(r, std::f64::consts::TAU));
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<P> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

fn slot(cx: f64, cy: f64, len: f64, r: f64) -> Vec<P> {
    let half = (len / 2.0 - r).max(0.0);
    let mut pts = Vec::new();
    let n = crate::dxf::arc_segments(r, std::f64::consts::PI).max(12);
    for i in 0..=n {
        let a = -std::f64::consts::FRAC_PI_2 + i as f64 / n as f64 * std::f64::consts::PI;
        pts.push([cx + half + r * a.cos(), cy + r * a.sin()]);
    }
    for i in 0..=n {
        let a = std::f64::consts::FRAC_PI_2 + i as f64 / n as f64 * std::f64::consts::PI;
        pts.push([cx - half + r * a.cos(), cy + r * a.sin()]);
    }
    pts
}

impl Part {
    pub fn geometry(&self) -> Geo {
        let (w, h) = (self.w, self.h);
        let m = w.min(h);
        match self.shape {
            Shape::Rectangle => Geo {
                outline: rect(0.0, 0.0, w, h),
                fills: vec![rect(0.0, 0.0, w, h)],
                holes: vec![],
            },
            Shape::LBracket => {
                let t = 0.4 * m;
                let r = (t * 0.18).max(2.0);
                Geo {
                    outline: vec![[0.0, 0.0], [w, 0.0], [w, t], [t, t], [t, h], [0.0, h]],
                    fills: vec![rect(0.0, 0.0, w, t), rect(0.0, t, t, h)],
                    holes: vec![
                        circle(w - t * 0.6, t / 2.0, r, 24),
                        circle(t / 2.0, h - t * 0.6, r, 24),
                    ],
                }
            }
            Shape::Disc => {
                let o = circle(m / 2.0, m / 2.0, m / 2.0, 64);
                Geo {
                    fills: vec![o.clone()],
                    outline: o,
                    holes: vec![circle(m / 2.0, m / 2.0, m * 0.15, 32)],
                }
            }
            Shape::Flange => {
                let inset = m * 0.12;
                let r = m * 0.05;
                Geo {
                    outline: rect(0.0, 0.0, w, h),
                    fills: vec![rect(0.0, 0.0, w, h)],
                    holes: vec![
                        circle(inset, inset, r, 24),
                        circle(w - inset, inset, r, 24),
                        circle(w - inset, h - inset, r, 24),
                        circle(inset, h - inset, r, 24),
                        circle(w / 2.0, h / 2.0, m * 0.25, 48),
                    ],
                }
            }
            Shape::Triangle => {
                let o = vec![[0.0, 0.0], [w, 0.0], [0.0, h]];
                Geo { fills: vec![o.clone()], outline: o, holes: vec![] }
            }
            Shape::Strip => {
                // Holes along the long side, about every 110 mm.
                let long = w.max(h);
                let n = ((long / 110.0).floor() as usize).max(1);
                let step = long / n as f64;
                let r = (m * 0.08).clamp(1.5, 8.0);
                let holes = (0..n)
                    .map(|i| {
                        let t = step * (i as f64 + 0.5);
                        if w >= h { circle(t, h / 2.0, r, 16) } else { circle(w / 2.0, t, r, 16) }
                    })
                    .collect();
                Geo { outline: rect(0.0, 0.0, w, h), fills: vec![rect(0.0, 0.0, w, h)], holes }
            }
            Shape::Custom => match &self.custom {
                Some(c) => Geo { outline: c.outline.clone(), fills: c.fills.clone(), holes: c.holes.clone() },
                None => Geo { outline: rect(0.0, 0.0, w, h), fills: vec![rect(0.0, 0.0, w, h)], holes: vec![] },
            },
            Shape::PlateHole => Geo {
                outline: rect(0.0, 0.0, w, h),
                fills: vec![rect(0.0, 0.0, w, h)],
                holes: vec![circle(w / 2.0, h / 2.0, m * 0.2, 40)],
            },
            Shape::SlottedPlate => Geo {
                outline: rect(0.0, 0.0, w, h),
                fills: vec![rect(0.0, 0.0, w, h)],
                holes: vec![slot(w / 2.0, h / 2.0, w * 0.6, (h * 0.12).max(2.0))],
            },
        }
    }

    /// Bounding box size of the part (the disc is always square).
    pub fn bbox(&self) -> (f64, f64) {
        if self.shape == Shape::Disc {
            let m = self.w.min(self.h);
            (m, m)
        } else {
            (self.w, self.h)
        }
    }

    pub fn net_area(&self) -> f64 {
        let g = self.geometry();
        area(&g.outline) - g.holes.iter().map(|h| area(h)).sum::<f64>()
    }

    pub fn cut_length(&self) -> f64 {
        let g = self.geometry();
        perimeter(&g.outline) + g.holes.iter().map(|h| perimeter(h)).sum::<f64>()
    }

    pub fn pierces(&self) -> usize {
        1 + self.geometry().holes.len()
    }

    /// The quality set by hand on each contour the part cuts, in the order of
    /// `geometry()` (holes, then the outline); empty or `None` where nothing
    /// is set by hand (CQ6). A built-in shape has none.
    pub fn cut_qualities(&self) -> Vec<Option<CutQuality>> {
        match (self.shape, &self.custom) {
            (Shape::Custom, Some(c)) => c.cut_qualities(),
            _ => vec![],
        }
    }
}

pub fn area(poly: &[P]) -> f64 {
    let n = poly.len();
    let mut s = 0.0;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        s += a[0] * b[1] - b[0] * a[1];
    }
    (s / 2.0).abs()
}

pub fn perimeter(poly: &[P]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
        })
        .sum()
}

pub fn centroid(poly: &[P]) -> P {
    let n = poly.len().max(1) as f64;
    let (sx, sy) = poly.iter().fold((0.0, 0.0), |a, p| (a.0 + p[0], a.1 + p[1]));
    [sx / n, sy / n]
}

/// A part placed on a sheet. `w`/`h` are the placed footprint. The part is
/// first mirrored (if `mirror`), then turned `turns` × 90° counter-clockwise —
/// the original allows 0/90/180/270°, plain or mirrored (rule R1).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Placed {
    pub part: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub turns: u8,
    pub mirror: bool,
}

impl Placed {
    /// Turned a quarter (90° or 270°): the footprint is the part's box swapped.
    pub fn rotated(&self) -> bool {
        self.turns % 2 == 1
    }
    pub fn angle(&self) -> u16 {
        (self.turns % 4) as u16 * 90
    }
    /// Map a point in the part's local coordinates onto the sheet.
    pub fn map(&self, p: P) -> P {
        let (bw, bh) = if self.rotated() { (self.h, self.w) } else { (self.w, self.h) };
        let lx = if self.mirror { bw - p[0] } else { p[0] };
        let ly = p[1];
        let (rx, ry) = match self.turns % 4 {
            0 => (lx, ly),
            1 => (bh - ly, lx),
            2 => (bw - lx, bh - ly),
            _ => (ly, bw - lx),
        };
        [self.x + rx, self.y + ry]
    }
    pub fn contains(&self, p: P) -> bool {
        p[0] >= self.x && p[0] <= self.x + self.w && p[1] >= self.y && p[1] <= self.y + self.h
    }
    /// The footprint as (x0, y0, x1, y1).
    pub fn bbox4(&self) -> [f64; 4] {
        [self.x, self.y, self.x + self.w, self.y + self.h]
    }
    /// True when the two boxes are closer than `gap`.
    pub fn clashes(&self, o: &Placed, gap: f64) -> bool {
        self.x < o.x + o.w + gap && o.x < self.x + self.w + gap && self.y < o.y + o.h + gap && o.y < self.y + self.h + gap
    }
}

// ---------------------------------------------------------------------------
// Round parts. The original recognises a part whose outside is one full circle
// (rule C1: the outer contour is a single element, a circle or an arc of 360°)
// and tests round shapes with true circle geometry, not their boxes (rule C4:
// circle–circle, segment–circle and arc–circle tests).
// ---------------------------------------------------------------------------

/// An imported outline counts as a circle when every point lies within this
/// share of the mean radius (our outlines are polygons, not single arcs).
pub const CIRCLE_TOL: f64 = 0.005;
/// …and it has at least this many points.
pub const CIRCLE_MIN_POINTS: usize = 16;

/// Centre (part coordinates) and radius when the part's outside is a full circle.
pub fn part_circle(p: &Part) -> Option<(P, f64)> {
    match p.shape {
        Shape::Disc => {
            let m = p.w.min(p.h);
            Some(([m / 2.0, m / 2.0], m / 2.0))
        }
        Shape::Custom => {
            let o = p.geometry().outline;
            if o.len() < CIRCLE_MIN_POINTS {
                return None;
            }
            let c = centroid(&o);
            let ds: Vec<f64> = o.iter().map(|q| ((q[0] - c[0]).powi(2) + (q[1] - c[1]).powi(2)).sqrt()).collect();
            let r = ds.iter().sum::<f64>() / ds.len() as f64;
            (r > 0.0 && ds.iter().all(|d| (d - r).abs() <= r * CIRCLE_TOL)).then_some((c, r))
        }
        _ => None,
    }
}

/// The circle of a placed round part, on the sheet.
pub fn placed_circle(parts: &[Part], pl: &Placed) -> Option<(P, f64)> {
    let (c, r) = part_circle(parts.get(pl.part)?)?;
    Some((pl.map(c), r))
}

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Is the sheet point `w` on the part itself (inside its outline, not in a hole)?
/// Parts tucked into each other's notches have overlapping boxes, so picking
/// follows the contour.
pub fn placed_contains(parts: &[Part], pl: &Placed, w: P) -> bool {
    if !pl.contains(w) {
        return false;
    }
    let (o, holes) = placed_shape(parts, pl);
    point_in_poly(&o, w) && !holes.iter().any(|h| point_in_poly(h, w))
}

/// A placed part's outline and holes, on the sheet.
pub fn placed_shape(parts: &[Part], pl: &Placed) -> (Vec<P>, Vec<Vec<P>>) {
    let g = parts[pl.part].geometry();
    (g.outline.iter().map(|&q| pl.map(q)).collect(), g.holes.iter().map(|h| h.iter().map(|&q| pl.map(q)).collect()).collect())
}

pub(crate) fn seg_point_dist(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-18 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    dist(p, [a[0] + t * dx, a[1] + t * dy])
}

pub(crate) fn seg_seg_dist(a: P, b: P, c: P, d: P) -> f64 {
    if segs_cross(a, b, c, d) {
        return 0.0;
    }
    seg_point_dist(a, c, d).min(seg_point_dist(b, c, d)).min(seg_point_dist(c, a, b)).min(seg_point_dist(d, a, b))
}

pub(crate) fn bbox_of(poly: &[P]) -> [f64; 4] {
    poly.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
}

/// Are two closed contours closer than `gap` anywhere? Element by element, each
/// pair first tested by its box (like the original does before its exact
/// segment/arc/circle tests, rule T-S2).
pub fn contours_within(a: &[P], b: &[P], gap: f64) -> bool {
    let bb = bbox_of(b);
    for i in 0..a.len() {
        let (p, q) = (a[i], a[(i + 1) % a.len()]);
        if p[0].max(q[0]) < bb[0] - gap || p[0].min(q[0]) > bb[2] + gap || p[1].max(q[1]) < bb[1] - gap || p[1].min(q[1]) > bb[3] + gap {
            continue;
        }
        for j in 0..b.len() {
            let (r, s) = (b[j], b[(j + 1) % b.len()]);
            if p[0].max(q[0]) < r[0].min(s[0]) - gap || p[0].min(q[0]) > r[0].max(s[0]) + gap || p[1].max(q[1]) < r[1].min(s[1]) - gap || p[1].min(q[1]) > r[1].max(s[1]) + gap {
                continue;
            }
            if seg_seg_dist(p, q, r, s) < gap {
                return true;
            }
        }
    }
    false
}

/// `inner` lies inside `outer`'s outline: allowed only when it sits wholly in
/// one of `outer`'s holes with the gap all round (parts nested in holes).
fn inside_clash(inner: &[P], outer: &(Vec<P>, Vec<Vec<P>>), gap: f64) -> bool {
    let Some(&p0) = inner.first() else { return false };
    if !point_in_poly(&outer.0, p0) {
        return false;
    }
    match outer.1.iter().find(|h| point_in_poly(h, p0)) {
        Some(h) => !inner.iter().all(|&q| point_in_poly(h, q)) || contours_within(inner, h, gap),
        None => true,
    }
}

/// True when two placed parts are closer than `gap`, by their real contours
/// (the original tests every element of one contour against the other,
/// rule T-S1). The boxes are only a quick first
/// check; round parts use their exact circle; a part may sit in another's hole.
pub fn clash_exact(parts: &[Part], a: &Placed, b: &Placed, gap: f64) -> bool {
    if !a.clashes(b, gap) {
        return false;
    }
    if let (Some((ca, ra)), Some((cb, rb))) = (placed_circle(parts, a), placed_circle(parts, b)) {
        let d = dist(ca, cb);
        // One circle wholly inside the other may sit in its hole: the contours decide.
        if d + ra.min(rb) > ra.max(rb) {
            return d < ra + rb + gap;
        }
    }
    let (sa, sb) = (placed_shape(parts, a), placed_shape(parts, b));
    contours_within(&sa.0, &sb.0, gap) || inside_clash(&sa.0, &sb, gap) || inside_clash(&sb.0, &sa, gap)
}

/// How far contour `a` can travel along unit direction `u` before it touches
/// contour `b` (rays from each one's corners to the other's edges); None if never.
pub fn contact_along(a: &[P], b: &[P], u: P) -> Option<f64> {
    fn ray(p: P, u: P, c: P, d: P) -> Option<f64> {
        let e = [d[0] - c[0], d[1] - c[1]];
        let den = u[0] * e[1] - u[1] * e[0];
        if den.abs() < 1e-12 {
            return None;
        }
        let w = [c[0] - p[0], c[1] - p[1]];
        let t = (w[0] * e[1] - w[1] * e[0]) / den;
        let s = (w[0] * u[1] - w[1] * u[0]) / den;
        (t >= -1e-9 && (-1e-9..=1.0 + 1e-9).contains(&s)).then_some(t.max(0.0))
    }
    let mut best: Option<f64> = None;
    let mut take = |t: Option<f64>| {
        if let Some(t) = t {
            if best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        }
    };
    for &p in a {
        for j in 0..b.len() {
            take(ray(p, u, b[j], b[(j + 1) % b.len()]));
        }
    }
    let back = [-u[0], -u[1]];
    for &p in b {
        for j in 0..a.len() {
            take(ray(p, back, a[j], a[(j + 1) % a.len()]));
        }
    }
    best
}

/// How far placed part `i` can slide along `u` (an axis direction) and still keep
/// the gap from every other part and the sheet margin, following the real
/// contours. Capped at `limit`.
pub fn free_slide(parts: &[Part], sheet: &[Placed], i: usize, cand: Placed, u: P, spec: &SheetSpec, limit: f64) -> f64 {
    let gap = spec.gap;
    // The sheet margin (the box is the contour's extent).
    let mut t = limit.min(match (u[0] as i32, u[1] as i32) {
        (-1, _) => cand.x - spec.margin,
        (1, _) => spec.w - spec.margin - (cand.x + cand.w),
        (_, -1) => cand.y - spec.margin,
        _ => spec.h - spec.margin - (cand.y + cand.h),
    });
    if t <= 0.0 {
        return 0.0;
    }
    let me = placed_shape(parts, &cand).0;
    let horizontal = u[1] == 0.0;
    for (j, o) in sheet.iter().enumerate() {
        if j == i {
            continue;
        }
        // Only parts in the band swept by the move can be hit.
        let in_band = if horizontal { o.y < cand.y + cand.h + gap && cand.y < o.y + o.h + gap } else { o.x < cand.x + cand.w + gap && cand.x < o.x + o.w + gap };
        let ahead = match (u[0] as i32, u[1] as i32) {
            (-1, _) => o.x < cand.x + 1e-9,
            (1, _) => o.x + o.w > cand.x + cand.w - 1e-9,
            (_, -1) => o.y < cand.y + 1e-9,
            _ => o.y + o.h > cand.y + cand.h - 1e-9,
        };
        if !in_band || !ahead {
            continue;
        }
        if let Some(c) = contact_along(&me, &placed_shape(parts, o).0, u) {
            t = t.min((c - gap).max(0.0));
        }
    }
    // Slanted edges need a little more room than the gap along the move: back
    // off until the real clearance holds.
    let ok = |t: f64| {
        let c = Placed { x: cand.x + u[0] * t, y: cand.y + u[1] * t, ..cand };
        sheet.iter().enumerate().all(|(j, o)| j == i || !clash_exact(parts, &c, o, gap - 1e-6))
    };
    if t <= 0.0 || ok(t) {
        return t.max(0.0);
    }
    let (mut lo, mut hi) = (0.0, t);
    for _ in 0..CONTACT_STEPS {
        let mid = (lo + hi) / 2.0;
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Bisection steps when backing off a slanted contact.
pub const CONTACT_STEPS: usize = 14;
/// Passes of the left-then-down contour compaction.
pub const COMPACT_PASSES: usize = 3;

/// Slide every part left, then down, until its contour meets another part's
/// contour (or the margin) at the part gap — parts tuck into each other's
/// notches instead of stopping at their boxes. Returns how far parts moved.
pub fn compact_contours(parts: &[Part], sheet: &mut [Placed], spec: &SheetSpec) -> f64 {
    let mut moved = 0.0;
    for _ in 0..COMPACT_PASSES {
        let mut pass = 0.0;
        for u in [[-1.0, 0.0], [0.0, -1.0]] {
            let mut order: Vec<usize> = (0..sheet.len()).collect();
            order.sort_by(|&a, &b| if u[0] != 0.0 { sheet[a].x.total_cmp(&sheet[b].x) } else { sheet[a].y.total_cmp(&sheet[b].y) });
            for &i in &order {
                let c = sheet[i];
                let t = free_slide(parts, sheet, i, c, u, spec, f64::MAX);
                if t > 0.01 {
                    sheet[i].x += u[0] * t;
                    sheet[i].y += u[1] * t;
                    pass += t;
                }
            }
        }
        moved += pass;
        if pass < 0.5 {
            break;
        }
    }
    moved
}

/// Where a circle of radius `r` touches two circles (centre, radius) at `gap`:
/// the two crossing points of the circles of radius r1+r+gap and r2+r+gap.
pub fn pocket_points(c1: P, r1: f64, c2: P, r2: f64, r: f64, gap: f64) -> Vec<P> {
    let (a, b) = (r1 + r + gap, r2 + r + gap);
    let d = dist(c1, c2);
    if d < 1e-9 || d > a + b || d < (a - b).abs() {
        return vec![];
    }
    let l = (a * a - b * b + d * d) / (2.0 * d);
    let h = (a * a - l * l).max(0.0).sqrt();
    let (ux, uy) = ((c2[0] - c1[0]) / d, (c2[1] - c1[1]) / d);
    let m = [c1[0] + ux * l, c1[1] + uy * l];
    vec![[m[0] - uy * h, m[1] + ux * h], [m[0] + uy * h, m[1] - ux * h]]
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SheetSpec {
    pub w: f64,
    pub h: f64,
    pub margin: f64,
    pub gap: f64,
    pub material: usize,
    pub thickness: f64,
    /// Automatic nesting level 1–5 (the original's LevelSys.nst configurations).
    pub level: u8,
    /// The machine the sheet is programmed for (cutting, combined or punching).
    pub machine: crate::machine::MachineKind,
    /// The job's kerf compensation choice (Machining → Compensation); unset =
    /// the machine file's (KC1).
    #[serde(default)]
    pub kerf: crate::kerf::KerfChoice,
    /// The job's gas, kept as its number (GS10). None = a job saved before
    /// jobs carried a gas: it is cut with the machine's (GS11).
    #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::gas::number")]
    pub gas: Option<crate::gas::Gas>,
}

impl SheetSpec {
    pub fn fits(&self, p: &Placed) -> bool {
        let e = 1e-6;
        p.x >= self.margin - e && p.y >= self.margin - e && p.x + p.w <= self.w - self.margin + e && p.y + p.h <= self.h - self.margin + e
    }
    pub fn nest_level(&self) -> &'static NestLevel {
        &LEVELS[(self.level.clamp(1, 5) - 1) as usize]
    }
}

/// One sheet's own size and shape. A plain sheet is a rectangle; a remnant
/// can have an irregular outline (stock.rs). `stock_ref` names the stock item
/// the sheet was taken from (empty = not from stock).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SheetFormat {
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub outline: Option<crate::stock::Outline>,
    #[serde(default)]
    pub stock_ref: String,
    #[serde(default)]
    pub remnant: bool,
    /// Remnants already put in stock from this sheet (generated once, RE1).
    #[serde(default)]
    pub made: Vec<String>,
    /// Fingerprint of the remnant lines they were made from.
    #[serde(default)]
    pub made_from: u64,
}

impl SheetFormat {
    pub fn rect(w: f64, h: f64) -> Self {
        SheetFormat { w, h, ..Default::default() }
    }
    /// The sheet's real area in mm² (its outline's, else W × H).
    pub fn area_mm2(&self) -> f64 {
        match &self.outline {
            Some(o) if o.outer.len() >= 3 => o.area_mm2(),
            _ => self.w * self.h,
        }
    }
    /// A stock item as a sheet format.
    pub fn from_stock(it: &crate::stock::Item) -> Self {
        SheetFormat { w: it.length, h: it.width, outline: it.outline.clone(), stock_ref: it.reference.clone(), remnant: it.kind.is_remnant(), made: vec![], made_from: 0 }
    }
}

impl NestResult {
    /// Sheet `i`'s format; the job's sheet size when it has none recorded.
    pub fn format(&self, i: usize, spec: &SheetSpec) -> SheetFormat {
        self.formats.get(i).cloned().unwrap_or_else(|| SheetFormat::rect(spec.w, spec.h))
    }
    /// One format per sheet: missing ones (jobs saved before sheets had their
    /// own formats) are the job's sheet size, extra ones are dropped.
    pub fn normalise(&mut self, spec: &SheetSpec) {
        let n = self.sheets.len();
        self.formats.truncate(n);
        while self.formats.len() < n {
            self.formats.push(SheetFormat::rect(spec.w, spec.h));
        }
    }
    /// Add an empty sheet of this format.
    pub fn push_sheet(&mut self, f: SheetFormat) {
        self.sheets.push(vec![]);
        self.formats.push(f);
    }
}

impl SheetSpec {
    /// The job's settings with the size of one sheet's format.
    pub fn for_format(&self, f: &SheetFormat) -> SheetSpec {
        SheetSpec { w: f.w, h: f.h, ..self.clone() }
    }
}

/// A format nesting may use and how many of it (None = no limit).
#[derive(Clone, Debug, PartialEq)]
pub struct FormatSlot {
    pub format: SheetFormat,
    pub qty: Option<usize>,
}

#[derive(Clone, Default, Debug)]
pub struct NestResult {
    pub sheets: Vec<Vec<Placed>>,
    /// Each sheet's format, in step with `sheets`.
    pub formats: Vec<SheetFormat>,
    /// Name of the method that won (for the status line).
    pub method: String,
    /// How many methods were tried.
    pub tried: usize,
}

// ---------------------------------------------------------------------------
// Nesting levels: the original's automatic nesting configurations, read from
// LevelSys.nst (lines A001-A022; MECHANICS.md "N1. The five automatic levels").
// ---------------------------------------------------------------------------

/// Main nesting direction (A002: 1 vertical, 2 horizontal, 3 both).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NestDir {
    /// Fill columns from the left: the remnant stays on the right.
    Vertical,
    /// Fill rows from the bottom: the remnant stays at the top.
    Horizontal,
    Both,
}

#[derive(Clone, Copy, Debug)]
pub struct NestLevel {
    pub name: &'static str,
    /// A001 number of nesting methods tried; the best result is kept.
    pub methods: usize,
    /// A002 main direction.
    pub dir: NestDir,
    /// A003 strip width (mm). Used only when A020 (`own_band`) is 1; it is 0 in
    /// every system level, so the machine's CT1 B057 applies.
    pub band: f64,
    /// A020: take the strip width from A003 instead of the machine (0 everywhere).
    pub own_band: bool,
    /// A004 regular nesting (tidy arrays of identical parts), A005 tests.
    pub regular: bool,
    pub regular_tests: usize,
    /// A006-A009 modules: part pairs, semi-complex (mirrored pairs),
    /// complex, staggered ("tresbolillo") rows.
    pub modules: bool,
    pub semi_complex: bool,
    pub complex: bool,
    pub staggered: bool,
    /// A011 minimum efficiency for a module to be used.
    pub min_module_eff: f64,
    /// A014 step of the free ("special") rotations, degrees.
    pub rot_step: f64,
    /// A015 level for regular nesting.
    pub regular_level: u8,
    /// A016 rotate parts to fill holes (parts nested inside other parts' holes).
    pub fill_holes: bool,
    /// A017 test each part from different corners.
    pub corners: bool,
    /// A018 special rotations 0 no / 1 yes / 2 automatic.
    pub special_rot: u8,
}

pub const LEVELS: [NestLevel; 5] = [
    NestLevel { name: "1 · Fast", methods: 4, dir: NestDir::Vertical, band: 3.0, own_band: false, regular: false, regular_tests: 99, modules: false, semi_complex: false, complex: false, staggered: false, min_module_eff: 0.8, rot_step: 30.0, regular_level: 0, fill_holes: false, corners: false, special_rot: 2 },
    NestLevel { name: "2 · Regular", methods: 7, dir: NestDir::Vertical, band: 3.0, own_band: false, regular: true, regular_tests: 99, modules: false, semi_complex: false, complex: false, staggered: false, min_module_eff: 0.8, rot_step: 30.0, regular_level: 1, fill_holes: false, corners: false, special_rot: 2 },
    NestLevel { name: "3 · Modules", methods: 11, dir: NestDir::Vertical, band: 3.0, own_band: false, regular: true, regular_tests: 99, modules: true, semi_complex: true, complex: true, staggered: true, min_module_eff: 0.8, rot_step: 30.0, regular_level: 2, fill_holes: false, corners: false, special_rot: 2 },
    NestLevel { name: "4 · Both directions", methods: 16, dir: NestDir::Both, band: 3.0, own_band: false, regular: true, regular_tests: 99, modules: true, semi_complex: true, complex: true, staggered: true, min_module_eff: 0.8, rot_step: 30.0, regular_level: 2, fill_holes: false, corners: false, special_rot: 2 },
    NestLevel { name: "5 · Maximum", methods: 18, dir: NestDir::Both, band: 3.0, own_band: false, regular: true, regular_tests: 99, modules: true, semi_complex: true, complex: true, staggered: true, min_module_eff: 0.8, rot_step: 30.0, regular_level: 3, fill_holes: true, corners: true, special_rot: 2 },
];

/// Is turn `k` (x 90°) allowed for the part, plain or mirrored? 90°/270° also
/// need the part's "may turn 90°" switch.
pub fn turn_allowed(p: &Part, k: u8, mirror: bool) -> bool {
    let k = (k % 4) as usize;
    let set = if mirror { p.rots.mirrored } else { p.rots.plain };
    set[k] && (k % 2 == 0 || p.rotate)
}

/// The original's angle normalisation (rule R2): into [0, 360).
pub fn norm_angle(a: f64) -> f64 {
    let mut a = a;
    if a >= 360.0 {
        a -= 360.0;
    }
    if a < 0.0 {
        a += 360.0;
    }
    a
}

/// Special rotations (rule R3): mode 0 never,
/// 1 always, otherwise only for angles that aren't 0/90/180/270 within `tol`.
#[cfg_attr(not(test), allow(dead_code))] // original rule; the rebuild only turns by quarters
pub fn special_rotation_possible(mode: u8, angle: f64, tol: f64) -> bool {
    match mode {
        0 => false,
        1 => true,
        _ => [0.0, 90.0, 180.0, 270.0].iter().all(|q| (angle - q).abs() > tol),
    }
}

/// Nesting corner after the sheet axes are turned (rule R5): `rot` 1/2/3 = 90/180/270°, corner as (right?, top?).
#[cfg_attr(not(test), allow(dead_code))] // original rule; the rebuild only turns by quarters
pub fn corner_after_axes(rot: u8, corner: (bool, bool)) -> (bool, bool) {
    let (x, y) = corner;
    match rot {
        1 => match (x, y) {
            (false, false) => (true, false),
            (false, true) => (false, false),
            (true, false) => (true, true),
            (true, true) => (false, true),
        },
        2 => (!x, !y),
        3 => match (x, y) {
            (false, false) => (false, true),
            (false, true) => (true, true),
            (true, false) => (false, false),
            (true, true) => (true, false),
        },
        _ => (x, y),
    }
}

/// One part inside an item, relative to the item's corner.
#[derive(Clone, Copy, Debug)]
struct Member {
    part: usize,
    dx: f64,
    dy: f64,
    w: f64,
    h: f64,
    turns: u8,
    mirror: bool,
}

/// What the nester places: one part, or a module of parts (pair, array,
/// staggered rows) that moves as one rectangle.
#[derive(Clone, Debug)]
struct Item {
    w: f64,
    h: f64,
    members: Vec<Member>,
}

impl Item {
    /// The item turned 90° counter-clockwise, if every member may turn.
    fn turned(&self, parts: &[Part]) -> Option<Item> {
        let mut m = Vec::with_capacity(self.members.len());
        for a in &self.members {
            let t = (a.turns + 1) % 4;
            if !turn_allowed(&parts[a.part], t, a.mirror) {
                return None;
            }
            m.push(Member { part: a.part, dx: self.h - (a.dy + a.h), dy: a.dx, w: a.h, h: a.w, turns: t, mirror: a.mirror });
        }
        Some(Item { w: self.h, h: self.w, members: m })
    }
}

/// A single part as an item, in the first allowed orientation.
fn single(parts: &[Part], i: usize) -> Option<Item> {
    let (w, h) = parts[i].bbox();
    for mirror in [false, true] {
        for k in [0u8, 2, 1, 3] {
            if turn_allowed(&parts[i], k, mirror) {
                let (fw, fh) = if k % 2 == 1 { (h, w) } else { (w, h) };
                return Some(Item { w: fw, h: fh, members: vec![Member { part: i, dx: 0.0, dy: 0.0, w: fw, h: fh, turns: k, mirror }] });
            }
        }
    }
    None
}

fn seg_dist(p: P, a: P, b: P) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
    ((p[0] - a[0] - ab[0] * t).powi(2) + (p[1] - a[1] - ab[1] * t).powi(2)).sqrt()
}

fn segs_cross(a: P, b: P, c: P, d: P) -> bool {
    let o = |p: P, q: P, r: P| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let (d1, d2, d3, d4) = (o(a, b, c), o(a, b, d), o(c, d, a), o(c, d, b));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
}

pub fn point_in_poly(poly: &[P], p: P) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n == 0 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Do two outlines come closer than `gap` (or overlap)?
fn too_close(a: &[P], b: &[P], gap: f64) -> bool {
    let (na, nb) = (a.len(), b.len());
    for i in 0..na {
        let (p, q) = (a[i], a[(i + 1) % na]);
        for j in 0..nb {
            let (r, s) = (b[j], b[(j + 1) % nb]);
            if segs_cross(p, q, r, s) || seg_dist(p, r, s) < gap || seg_dist(r, p, q) < gap {
                return true;
            }
        }
    }
    point_in_poly(a, b[0]) || point_in_poly(b, a[0])
}

/// A pair module: the part and a copy turned 180° (or mirrored), slid as close
/// as the gap allows. Kept only if it reaches the level's minimum efficiency.
fn pair_module(parts: &[Part], i: usize, gap: f64, mirrored: bool, min_eff: f64) -> Option<Item> {
    let p = &parts[i];
    if !turn_allowed(p, 0, false) || !turn_allowed(p, 2, mirrored) {
        return None;
    }
    let (w, h) = p.bbox();
    let net = p.net_area();
    let single_eff = net / (w * h).max(1e-9);
    let outline = p.geometry().outline;
    if outline.len() < 3 {
        return None;
    }
    let a_pl = Placed { part: i, x: 0.0, y: 0.0, w, h, turns: 0, mirror: false };
    let a: Vec<P> = outline.iter().map(|&q| a_pl.map(q)).collect();
    let mut best: Option<(f64, f64, f64)> = None; // (efficiency, dx, dy)
    let step = (w.max(h) / 160.0).max(0.5);
    for k in -8i32..=8 {
        let dy = k as f64 * h / 8.0;
        // Slide the copy in from the right until it would come too close.
        let mut dx = w + gap;
        let mut last_ok = None;
        while dx >= -w {
            let b_pl = Placed { part: i, x: dx, y: dy, w, h, turns: 2, mirror: mirrored };
            let b: Vec<P> = outline.iter().map(|&q| b_pl.map(q)).collect();
            if too_close(&a, &b, gap) {
                break;
            }
            last_ok = Some(dx);
            dx -= step;
        }
        if let Some(dx) = last_ok {
            let (x0, x1) = (dx.min(0.0), (dx + w).max(w));
            let (y0, y1) = (dy.min(0.0), (dy + h).max(h));
            let eff = 2.0 * net / ((x1 - x0) * (y1 - y0));
            if best.is_none_or(|b| eff > b.0 + 1e-9) {
                best = Some((eff, dx, dy));
            }
        }
    }
    let (eff, dx, dy) = best?;
    if eff < min_eff || eff <= single_eff + 0.02 {
        return None;
    }
    let (ox, oy) = (dx.min(0.0), dy.min(0.0));
    let (x1, y1) = ((dx + w).max(w), (dy + h).max(h));
    Some(Item {
        w: x1 - ox,
        h: y1 - oy,
        members: vec![
            Member { part: i, dx: -ox, dy: -oy, w, h, turns: 0, mirror: false },
            Member { part: i, dx: dx - ox, dy: dy - oy, w, h, turns: 2, mirror: mirrored },
        ],
    })
}

/// Is the part a disc (round outline)?
fn is_round(p: &Part) -> bool {
    if p.shape == Shape::Disc {
        return true;
    }
    let o = p.geometry().outline;
    if o.len() < 12 {
        return false;
    }
    let c = centroid(&o);
    let rs: Vec<f64> = o.iter().map(|q| ((q[0] - c[0]).powi(2) + (q[1] - c[1]).powi(2)).sqrt()).collect();
    let r = rs.iter().sum::<f64>() / rs.len() as f64;
    rs.iter().all(|x| (x - r).abs() <= r * 0.01)
}

/// Staggered ("tresbolillo") rows of discs: every other row shifted half a
/// pitch and the rows closer together (pitch x √3/2).
fn staggered_module(parts: &[Part], i: usize, count: usize, gap: f64, uw: f64, uh: f64) -> Option<Item> {
    let (d, _) = parts[i].bbox();
    let pitch = d + gap;
    let row = pitch * 3f64.sqrt() / 2.0;
    let per_row = ((uw + gap - pitch / 2.0) / pitch).floor().max(1.0) as usize;
    let max_rows = (((uh - d) / row).floor() + 1.0).max(1.0) as usize;
    let n = count.min(per_row * max_rows);
    if n < 3 || per_row < 2 {
        return None;
    }
    if n.div_ceil(per_row) < 2 {
        return None;
    }
    let mut members = Vec::with_capacity(n);
    for k in 0..n {
        let (r, c) = (k / per_row, k % per_row);
        let shift = if r % 2 == 1 { pitch / 2.0 } else { 0.0 };
        members.push(Member { part: i, dx: c as f64 * pitch + shift, dy: r as f64 * row, w: d, h: d, turns: 0, mirror: false });
    }
    let w = members.iter().map(|m| m.dx + m.w).fold(0.0, f64::max);
    let h = members.iter().map(|m| m.dy + m.h).fold(0.0, f64::max);
    // Only worth it if it beats the square grid for the same discs.
    let rows_sq = n.div_ceil(per_row.min(n));
    let square = (per_row.min(n) as f64 * pitch - gap) * (rows_sq as f64 * pitch - gap);
    (w * h < square - 1e-6).then_some(Item { w, h, members })
}

/// A regular array of one part (cols x rows), trying at most `tests` shapes.
fn regular_block(parts: &[Part], i: usize, count: usize, gap: f64, uw: f64, uh: f64, tests: usize) -> Option<Item> {
    let base = single(parts, i)?;
    let turned = base.turned(parts);
    let mut best: Option<(usize, f64, Item)> = None;
    let mut tried = 0;
    for it in std::iter::once(base.clone()).chain(turned) {
        let max_c = ((uw + gap) / (it.w + gap)).floor().max(1.0) as usize;
        let max_r = ((uh + gap) / (it.h + gap)).floor().max(1.0) as usize;
        for c in 1..=max_c {
            for r in 1..=max_r {
                let n = c * r;
                if n > count || n < 2 {
                    continue;
                }
                if tried >= tests {
                    break;
                }
                tried += 1;
                let w = c as f64 * (it.w + gap) - gap;
                let h = r as f64 * (it.h + gap) - gap;
                // Most parts, then the squarest block.
                let waste = (w - h).abs();
                if best.as_ref().is_none_or(|b| n > b.0 || (n == b.0 && waste < b.1)) {
                    let m0 = it.members[0];
                    let members = (0..n).map(|k| Member { dx: (k % c) as f64 * (it.w + gap), dy: (k / c) as f64 * (it.h + gap), ..m0 }).collect();
                    best = Some((n, waste, Item { w, h, members }));
                }
            }
        }
    }
    best.map(|b| b.2)
}

/// Area of a closed polygon (shoelace), mm².
pub fn polygon_area(p: &[P]) -> f64 {
    let n = p.len();
    ((0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum::<f64>() / 2.0).abs()
}

// ---------------------------------------------------------------------------
// The automatic nesting engine: true-shape strips (bands.rs), the way Lantek
// nests (MECHANICS.md "True-shape strip nesting").
// ---------------------------------------------------------------------------

/// Strip width (mm) when the machine's CT1 has no B057 and the level doesn't set
/// its own. Rebuild's choice: real CT1s use wider strips (about 14 mm); 5 mm gives
/// finer X positions for small parts and is still fast.
pub const BAND_DEFAULT: f64 = 5.0;
/// Added to half the gap when outlines are grown (mm): covers the 0.1 mm rounding
/// of the strips (rebuild's guard, not in the original).
pub const BAND_GUARD: f64 = 0.05;
/// Size tier: the items whose size is within these shares of the largest one left
/// are candidates together (0.9 and 1.1 in the original).
pub const TIER_LO: f64 = 0.9;
pub const TIER_HI: f64 = 1.1;
/// Two candidates whose costs differ by less than this share of the strip width
/// are a tie (0.2 in the original) …
pub const COST_TIE: f64 = 0.2;
/// … and two spots along X in the full-scan methods, this many strip widths.
pub const SPOT_TIE: f64 = 2.0;
/// Methods that try every strip instead of taking the first fit.
pub const FULL_SCAN: std::ops::RangeInclusive<u8> = 8..=11;
pub const MAX_SHEETS: usize = 60;

/// The strip width (mm) the nester uses: the level's A003 only when its A020 is 1
/// (never, in the system levels), else the machine's B057, else `BAND_DEFAULT`.
pub fn band_width(level: &NestLevel) -> f64 {
    if level.own_band {
        return level.band;
    }
    let b = crate::ct1::active().band_width;
    if b > 0.0 { b } else { BAND_DEFAULT }
}

/// One nesting method: a cost rule (the original's method index), a direction
/// (horizontal = the whole problem turned 90°, X and Y swapped and swapped
/// back afterwards) and whether modules and
/// regular arrays are offered as items (rebuild's variety, see MECHANICS).
#[derive(Clone, Copy, Debug)]
struct Method {
    code: u8,
    dir: NestDir,
    modules: bool,
    regular: bool,
}

impl Method {
    fn name(&self) -> String {
        let d = if self.dir == NestDir::Horizontal { "horizontal" } else { "vertical" };
        let mut n = format!("{d}, {}", cost_label(self.code));
        if self.modules {
            n += ", modules";
        }
        if self.regular {
            n += ", regular";
        }
        n
    }
}

fn cost_label(code: u8) -> String {
    let base = match code {
        1 | 19 | 24 => "leftmost then lowest centre",
        2 | 20 | 25 => "lowest shape centre",
        3 => "left and low shape centre",
        4 => "longest along X",
        5 => "tallest",
        6 | 8 | 12 | 21 | 26 => "lowest centre",
        7 | 9 | 13 | 22 | 27 => "lowest-left centre",
        10 | 14 | 17 => "lowest position",
        11 | 15 | 18 => "lowest-left position",
        16 => "leftmost position",
        _ => "leftmost centre",
    };
    let scan = if FULL_SCAN.contains(&code) { ", every strip" } else if (12..=15).contains(&code) { ", first strip" } else { "" };
    format!("method {} ({base}{scan})", code + 1)
}

/// The methods a level tries: A001 of them. Method k uses cost rule k ÷ (number of
/// directions) — the original passes its loop counter on as the cost case (4.6,
/// inferred) — in each direction of the level; at the module levels the rules take
/// turns with and without modules and regular arrays.
fn methods_for(level: &NestLevel) -> Vec<Method> {
    let dirs: &[NestDir] = match level.dir {
        NestDir::Both => &[NestDir::Vertical, NestDir::Horizontal],
        NestDir::Horizontal => &[NestDir::Horizontal],
        NestDir::Vertical => &[NestDir::Vertical],
    };
    let any_modules = level.modules || level.semi_complex || level.staggered;
    let mut groups: Vec<(bool, bool)> = vec![(false, false)];
    if any_modules && level.regular {
        groups.push((true, true));
    }
    if any_modules {
        groups.push((true, false));
    }
    if level.regular {
        groups.push((false, true));
    }
    (0..level.methods)
        .map(|k| {
            let code = k / dirs.len();
            let (modules, regular) = groups[code % groups.len()];
            Method { code: code as u8, dir: dirs[k % dirs.len()], modules, regular }
        })
        .collect()
}

/// Cost of a spot for cost rule `code` (the original's cost rules); lower is
/// better, with an optional second key for ties. `x`/`y`: where the profile starts,
/// from the usable area's corner; `cg`/`size`: the profile's local centre of
/// gravity and size; `l`/`w`: the sheet's length (X) and width (Y), all mm. The
/// original's constant `+L`/`+W` is left out (it doesn't change the order).
fn spot_cost(code: u8, x: f64, y: f64, cg: (f64, f64), size: (f64, f64), l: f64, w: f64) -> (f64, Option<f64>) {
    let (cx, cy) = (x + cg.0, y + cg.1);
    let (c, k2) = match code {
        1 | 19 | 24 => (cx, Some(cy)),
        2 | 20 | 25 => (cg.1, Some(cg.0)),
        3 => ((cx + cg.1) / 2.0, None),
        4 => (w - size.0, None),
        5 => (w - size.1, None),
        6 | 8 | 12 | 21 | 26 => (cy, None),
        7 | 9 | 13 | 22 | 27 => ((cx + cy) / 2.0, None),
        10 | 14 | 17 => (y + cy / w, None),
        11 | 15 | 18 => ((y + x) / 2.0 + ((cx + cy) / 2.0) / l, None),
        16 => (x + cx / l, None),
        _ => (cx, None),
    };
    if (12..=15).contains(&code) { (c / l + x, k2) } else { (c, k2) }
}

type Orients = std::rc::Rc<Vec<(Item, std::rc::Rc<crate::bands::Profile>)>>;

/// One orientation of a kind of item, with its strip profile and where its next
/// search starts (as in the original: the sheet only fills up, so strips that
/// failed before fail again).
struct Orient {
    item: Item,
    prof: std::rc::Rc<crate::bands::Profile>,
    mirror: bool,
    from_x: i32,
    memo: crate::bands::Memo,
}

/// A kind of item still to nest: how many are left and its order value (B594 = 1:
/// the outline's area, holes not taken off).
struct Kind {
    orients: Vec<Orient>,
    qty: usize,
    value: f64,
    done: bool,
}

/// One sheet filled by one method.
struct Fill {
    placed: Vec<Placed>,
    /// Every part left fits on this sheet.
    complete: bool,
    /// Placed part area (mm²), used length along the nesting direction (mm) and
    /// placed area ÷ used area.
    area: f64,
    used: f64,
    eff: f64,
}

impl Fill {
    /// The better sheet. The original first compares the placed count of each
    /// priority set, then the usage: efficiency (by more than 1e-6), then
    /// the used length (by more than 0.1 mm). The rebuild has no priority sets, and
    /// a plain "more parts" first made the higher levels worse on a 306-part test
    /// job (67.6 % against 71.0 %). So (rebuild's choice): a sheet that takes every
    /// part left wins; of two such sheets the higher efficiency, then the shorter
    /// used length; otherwise the sheet carrying more part area, then more parts.
    fn better_than(&self, o: &Fill) -> bool {
        if self.complete != o.complete {
            return self.complete;
        }
        if !self.complete {
            if (self.area - o.area).abs() > 1e-6 {
                return self.area > o.area;
            }
            return self.placed.len() > o.placed.len();
        }
        if (self.eff - o.eff).abs() > 1e-6 {
            return self.eff > o.eff;
        }
        self.used < o.used - 0.1
    }
}

struct Engine<'a> {
    parts: &'a [Part],
    /// The job's settings at the size of the sheet being filled (`set_format`).
    spec: SheetSpec,
    /// A remnant's usable shape (its outline moved in by the margin), sheet
    /// coordinates; None on a plain rectangular sheet.
    region: Option<Vec<Vec<P>>>,
    level: &'a NestLevel,
    band: i32,
    band_mm: f64,
    min_hole: f64,
    fit_border: bool,
    grow: Vec<f64>,
    outline_area: Vec<f64>,
    net: Vec<f64>,
    round: Vec<bool>,
    singles: std::collections::HashMap<(usize, bool), Orients>,
    pairs: std::collections::HashMap<(usize, bool), Option<Orients>>,
}

impl<'a> Engine<'a> {
    fn new(parts: &'a [Part], spec: &'a SheetSpec, level: &'a NestLevel) -> Self {
        let mt = crate::ct1::active();
        let band_mm = band_width(level);
        let band = ((band_mm * crate::bands::UNITS).round() as i32).max(crate::bands::MIN_STRIP);
        // Half the gap, the rounding guard, and for round parts the sagitta of
        // their polygon (the collision test uses the true circle).
        let grow = parts
            .iter()
            .map(|p| {
                let sag = part_circle(p).map_or(0.0, |(_, r)| r * (1.0 - (std::f64::consts::PI / p.geometry().outline.len().max(3) as f64).cos()));
                spec.gap.max(0.0) / 2.0 + BAND_GUARD + sag
            })
            .collect();
        Engine {
            parts,
            spec: spec.clone(),
            region: None,
            level,
            band,
            band_mm: band as f64 / crate::bands::UNITS,
            min_hole: mt.min_hole_area,
            fit_border: mt.fit_border,
            grow,
            outline_area: parts.iter().map(|p| area(&p.geometry().outline)).collect(),
            net: parts.iter().map(|p| p.net_area()).collect(),
            round: parts.iter().map(is_round).collect(),
            singles: Default::default(),
            pairs: Default::default(),
        }
    }

    /// Fill the next sheets at this format's size and, for a remnant, inside its
    /// real shape (RE26). The orientation caches stay: they depend on the gap only.
    fn set_format(&mut self, f: &SheetFormat, base: &SheetSpec) {
        self.spec = base.for_format(f);
        let sp = &self.spec;
        // The strips may come as close to the edge as the usable area does in
        // `fill` (the margin, less the grow when parts may reach the border).
        let a = if self.fit_border { sp.gap.max(0.0) / 2.0 + BAND_GUARD } else { 0.0 };
        self.region = f.outline.as_ref().filter(|o| o.outer.len() >= 3).map(|o| {
            crate::region::offset(o, (sp.margin - a).max(0.0))
                .into_iter()
                .flat_map(|q| std::iter::once(q.outer).chain(q.holes))
                .collect()
        });
    }

    /// The contours of an item, in its footprint coordinates: every member's
    /// outline and the holes bigger than B076 (smaller holes count as solid).
    fn profile(&self, it: &Item, swap: bool) -> crate::bands::Profile {
        let mut polys: Vec<Vec<P>> = vec![];
        for m in &it.members {
            let g = self.parts[m.part].geometry();
            let pl = Placed { part: m.part, x: m.dx, y: m.dy, w: m.w, h: m.h, turns: m.turns, mirror: m.mirror };
            polys.push(g.outline.iter().map(|&q| pl.map(q)).collect());
            for hole in g.holes.iter().filter(|h| polygon_area(h) > self.min_hole) {
                polys.push(hole.iter().map(|&q| pl.map(q)).collect());
            }
        }
        let grow = it.members.iter().map(|m| self.grow[m.part]).fold(0.0, f64::max);
        let (mut w, mut h) = (it.w, it.h);
        if swap {
            // Horizontal: the whole problem turned onto its side (D1).
            for p in &mut polys {
                for q in p.iter_mut() {
                    q.swap(0, 1);
                }
            }
            std::mem::swap(&mut w, &mut h);
        }
        crate::bands::profile(&polys, w, h, grow, self.band)
    }

    /// Every allowed quarter turn and mirror of a part (R1, `turn_allowed`); turns
    /// that give the same strips are tried once.
    fn single_orients(&mut self, i: usize, swap: bool) -> Orients {
        if let Some(v) = self.singles.get(&(i, swap)) {
            return v.clone();
        }
        let (w, h) = self.parts[i].bbox();
        let mut v: Vec<(Item, std::rc::Rc<crate::bands::Profile>)> = vec![];
        for mirror in [false, true] {
            for k in 0..4u8 {
                if !turn_allowed(&self.parts[i], k, mirror) {
                    continue;
                }
                let (fw, fh) = if k % 2 == 1 { (h, w) } else { (w, h) };
                let it = Item { w: fw, h: fh, members: vec![Member { part: i, dx: 0.0, dy: 0.0, w: fw, h: fh, turns: k, mirror }] };
                let pr = self.profile(&it, swap);
                if !v.iter().any(|(_, q)| q.same(&pr)) {
                    v.push((it, std::rc::Rc::new(pr)));
                }
            }
        }
        let v = std::rc::Rc::new(v);
        self.singles.insert((i, swap), v.clone());
        v
    }

    /// A module turned by quarters, as long as every member may turn.
    fn module_orients(&self, it: &Item, swap: bool) -> Orients {
        let mut v: Vec<(Item, std::rc::Rc<crate::bands::Profile>)> = vec![];
        let mut cur = Some(it.clone());
        for _ in 0..4 {
            let Some(c) = cur else { break };
            let pr = self.profile(&c, swap);
            if !v.iter().any(|(_, q)| q.same(&pr)) {
                v.push((c.clone(), std::rc::Rc::new(pr)));
            }
            cur = c.turned(self.parts);
        }
        std::rc::Rc::new(v)
    }

    fn pair_orients(&mut self, i: usize, swap: bool) -> Option<Orients> {
        if let Some(v) = self.pairs.get(&(i, swap)) {
            return v.clone();
        }
        let (gap, eff) = (self.spec.gap, self.level.min_module_eff);
        let mut pair = None;
        if self.level.modules {
            pair = pair_module(self.parts, i, gap, false, eff);
        }
        if pair.is_none() && self.level.semi_complex {
            pair = pair_module(self.parts, i, gap, true, eff);
        }
        let v = pair.map(|p| self.module_orients(&p, swap));
        self.pairs.insert((i, swap), v.clone());
        v
    }

    fn kind(&self, o: &Orients, qty: usize) -> Kind {
        let value = o[0].0.members.iter().map(|m| self.outline_area[m.part]).sum();
        let orients = o.iter().map(|(it, pr)| Orient { item: it.clone(), prof: pr.clone(), mirror: it.members.iter().any(|m| m.mirror), from_x: i32::MIN, memo: Default::default() }).collect();
        Kind { orients, qty, value, done: false }
    }

    /// The items for one method from what is left: staggered discs, pairs and
    /// regular arrays when the method uses them, then single parts.
    fn kinds(&mut self, m: &Method, left: &[u32]) -> Vec<Kind> {
        let swap = m.dir == NestDir::Horizontal;
        let sp = self.spec.clone();
        let (uw, uh) = (sp.w - 2.0 * sp.margin, sp.h - 2.0 * sp.margin);
        let mut out = vec![];
        for i in 0..self.parts.len() {
            let mut n = left[i] as usize;
            if n == 0 {
                continue;
            }
            if m.modules && self.level.staggered && n >= 3 && self.round[i] {
                while n >= 3 {
                    let Some(it) = staggered_module(self.parts, i, n, sp.gap, uw, uh) else { break };
                    n -= it.members.len();
                    let o = self.module_orients(&it, swap);
                    out.push(self.kind(&o, 1));
                }
            }
            if m.modules && n >= 2 && !self.round[i] {
                if let Some(o) = self.pair_orients(i, swap) {
                    out.push(self.kind(&o, n / 2));
                    n %= 2;
                }
            }
            if m.regular && n >= 2 + self.level.regular_level as usize {
                while n >= 2 {
                    let Some(it) = regular_block(self.parts, i, n, sp.gap, uw, uh, self.level.regular_tests) else { break };
                    n -= it.members.len();
                    let o = self.module_orients(&it, swap);
                    out.push(self.kind(&o, 1));
                }
            }
            if n > 0 {
                let o = self.single_orients(i, swap);
                if !o.is_empty() {
                    out.push(self.kind(&o, n));
                }
            }
        }
        out
    }

    /// Fill one sheet with one method: take the largest item left
    /// and every item within ±10 % of its size, find each orientation's
    /// spot, keep the lowest cost (ties within 0.2 × strip
    /// width go to the lower second key, else to the unmirrored one), place it and
    /// repeat. Items that no longer fit drop out; then the next smaller tier.
    fn fill(&mut self, m: &Method, left: &[u32]) -> Fill {
        use crate::bands::UNITS;
        let swap = m.dir == NestDir::Horizontal;
        let sp = self.spec.clone();
        let (l, w) = if swap { (sp.h, sp.w) } else { (sp.w, sp.h) };
        // The usable area grows by the grow when parts may reach the border (B078).
        let a = if self.fit_border { sp.gap.max(0.0) / 2.0 + BAND_GUARD } else { 0.0 };
        let mut sheet = crate::bands::Sheet::new(sp.margin - a, l - sp.margin + a, sp.margin - a, w - sp.margin + a, self.band);
        if let Some(reg) = &self.region {
            // A remnant: only its real shape is free (turned with the problem).
            let reg: Vec<Vec<P>> = if swap { reg.iter().map(|p| p.iter().map(|q| [q[1], q[0]]).collect()).collect() } else { reg.clone() };
            sheet.keep_inside(&reg);
        }
        let (fx0, fy0) = (sheet.x0, sheet.y0);
        let mut kinds = self.kinds(m, left);
        let full = FULL_SCAN.contains(&m.code);
        let (stie, ctie) = (SPOT_TIE * self.band_mm, COST_TIE * self.band_mm);
        let mut placed: Vec<Placed> = vec![];
        loop {
            let t = kinds.iter().filter(|k| k.qty > 0 && !k.done).map(|k| k.value).fold(0.0, f64::max);
            if t <= 0.0 {
                break;
            }
            let mut best: Option<(usize, usize, crate::bands::Spot, bool)> = None;
            for (ki, kd) in kinds.iter_mut().enumerate() {
                let r = kd.value / t;
                if kd.qty == 0 || kd.done || !(r > TIER_LO && r < TIER_HI) {
                    continue;
                }
                let mut any = false;
                for (oi, o) in kd.orients.iter_mut().enumerate() {
                    if o.from_x == i32::MAX {
                        continue;
                    }
                    let (cg, size) = (o.prof.cg, (o.prof.sx as f64 / UNITS, o.prof.sy as f64 / UNITS));
                    let found = sheet.search(&o.prof, o.from_x, full, stie, &mut o.memo, |px, py| spot_cost(m.code, (px - fx0) as f64 / UNITS, (py - fy0) as f64 / UNITS, cg, size, l, w));
                    let Some(s) = found else {
                        o.from_x = i32::MAX;
                        continue;
                    };
                    o.from_x = s.first;
                    any = true;
                    let better = match &best {
                        None => true,
                        Some((_, _, b, bm)) => s.cost < b.cost - ctie || ((s.cost - b.cost).abs() <= ctie && if let (Some(x), Some(y)) = (s.key2, b.key2) { x < y } else { !o.mirror && *bm }),
                    };
                    if better {
                        best = Some((ki, oi, s, o.mirror));
                    }
                }
                if !any {
                    kd.done = true;
                }
            }
            let Some((ki, oi, s, _)) = best else { continue };
            let o = &kinds[ki].orients[oi];
            sheet.place(&o.prof, s.px, s.py);
            let (fu, fv) = (s.px as f64 / UNITS + o.prof.grow, s.py as f64 / UNITS + o.prof.grow);
            let (ox, oy) = if swap { (fv, fu) } else { (fu, fv) };
            for mb in &o.item.members {
                placed.push(Placed { part: mb.part, x: ox + mb.dx, y: oy + mb.dy, w: mb.w, h: mb.h, turns: mb.turns, mirror: mb.mirror });
            }
            kinds[ki].qty -= 1;
        }
        // Efficiency (the original's measure is unknown; rebuild's measure): placed part area ÷ (used length × usable width).
        let area: f64 = placed.iter().map(|p| self.net[p.part]).sum();
        let used = placed.iter().map(|p| if swap { p.y + p.h } else { p.x + p.w }).fold(sp.margin, f64::max) - sp.margin;
        let eff = if used > 0.0 { area / (used * (w - 2.0 * sp.margin).max(1.0)) } else { 0.0 };
        let complete = placed.len() as u64 == left.iter().map(|&q| q as u64).sum::<u64>();
        Fill { placed, complete, area, used, eff }
    }
}

/// Nest every requested part instance onto as few sheets as possible, the way the
/// chosen automatic level says, on sheets of the job's size.
pub fn nest(parts: &[Part], spec: &SheetSpec) -> NestResult {
    nest_plan(parts, spec, &[FormatSlot { format: SheetFormat::rect(spec.w, spec.h), qty: None }])
}

/// Nest sheet by sheet onto the formats given, in that order (the order comes
/// from the stock rules, stock.rs; the original's Do All takes the next sheet
/// the same way). For each sheet every method of the level fills a copy of it
/// from what is left and the best sheet is kept (the choice is made per sheet).
/// A format nothing left fits on is skipped without using it up; sheets are
/// counted against each format's quantity. A remnant is filled inside its real
/// shape.
pub fn nest_plan(parts: &[Part], spec: &SheetSpec, plan: &[FormatSlot]) -> NestResult {
    nest_plan_opt(parts, spec, plan, None)
}

/// One sheet of format `fmt` filled by the best method from what is left:
/// (placed parts, part area placed, method index); None when nothing fits.
fn fill_sheet(eng: &mut Engine, methods: &[Method], fmt: &SheetFormat, base: &SheetSpec, left: &[u32]) -> Option<(Vec<Placed>, f64, usize)> {
    if fmt.w - 2.0 * base.margin <= 0.0 || fmt.h - 2.0 * base.margin <= 0.0 {
        return None;
    }
    eng.set_format(fmt, base);
    let mut best: Option<(Fill, usize)> = None;
    for (mi, m) in methods.iter().enumerate() {
        let f = eng.fill(m, left);
        if best.as_ref().is_none_or(|b| f.better_than(&b.0)) {
            best = Some((f, mi));
        }
    }
    best.filter(|(f, _)| !f.placed.is_empty()).map(|(f, mi)| (f.placed, f.area, mi))
}

/// `nest_plan` with Do All's "Search for the best sheet combination"
/// (`combo_min` = its minimum efficiency, rule ST13): when the next sheet in order
/// would be used less than that (part area ÷ sheet area), the other formats
/// with sheets left are filled too and the first reaching it is taken — or, if
/// none does, the one placing the most part area, when that is more.
pub fn nest_plan_opt(parts: &[Part], spec: &SheetSpec, plan: &[FormatSlot], combo_min: Option<f64>) -> NestResult {
    let level = spec.nest_level();
    let methods = methods_for(level);
    let mut out = NestResult { tried: methods.len(), ..Default::default() };
    let mut eng = Engine::new(parts, spec, level);
    let mut left: Vec<u32> = parts.iter().map(|p| p.qty).collect();
    let mut slots: Vec<Option<usize>> = plan.iter().map(|f| f.qty).collect();
    let mut names: Vec<String> = vec![];
    let mut si = 0;
    while si < plan.len() && left.iter().any(|&q| q > 0) && out.sheets.len() < MAX_SHEETS {
        if slots[si] == Some(0) {
            si += 1;
            continue;
        }
        let mut pick = si;
        let mut best = fill_sheet(&mut eng, &methods, &plan[si].format, spec, &left);
        if let (Some(min), Some(b)) = (combo_min, best.as_ref()) {
            if b.1 / plan[si].format.area_mm2().max(1.0) < min {
                // The other formats with sheets left: the first reaching the
                // minimum wins, else the one placing the most.
                let mut alt: Option<(usize, (Vec<Placed>, f64, usize))> = None;
                for (k, sl) in plan.iter().enumerate() {
                    if k == si || slots[k] == Some(0) || sl.format == plan[si].format {
                        continue;
                    }
                    if let Some(c) = fill_sheet(&mut eng, &methods, &sl.format, spec, &left) {
                        let reaches = c.1 / sl.format.area_mm2().max(1.0) >= min;
                        let more = alt.as_ref().is_none_or(|a| c.1 > a.1 .1 + 1e-6);
                        if reaches {
                            alt = Some((k, c));
                            break;
                        }
                        if more {
                            alt = Some((k, c));
                        }
                    }
                }
                if let Some((k, c)) = alt {
                    if c.1 > b.1 + 1e-6 || c.1 / plan[k].format.area_mm2().max(1.0) >= min {
                        pick = k;
                        best = Some(c);
                    }
                }
            }
        }
        match best {
            Some((placed, _, mi)) => {
                for p in &placed {
                    left[p.part] = left[p.part].saturating_sub(1);
                }
                out.sheets.push(placed);
                out.formats.push(plan[pick].format.clone());
                if let Some(q) = slots[pick].as_mut() {
                    *q -= 1;
                }
                let n = methods[mi].name();
                if !names.contains(&n) {
                    names.push(n);
                }
            }
            // Nothing left fits on this format.
            None => si += 1,
        }
    }
    out.method = names.join("; ");
    out
}

/// Share of a part's size to its centre (rule GZ2).
pub const ZONE_CENTRE: f64 = 0.5;

/// Is any part already nested in the sheet's zone — the sheet less its
/// margins — judged by the part's centre (rule GZ2)?
pub fn any_part_in_zone(sheet: &[Placed], spec: &SheetSpec) -> bool {
    let (x0, y0, x1, y1) = (spec.margin, spec.margin, spec.w - spec.margin, spec.h - spec.margin);
    sheet.iter().any(|p| {
        let (cx, cy) = (p.x + p.w * ZONE_CENTRE, p.y + p.h * ZONE_CENTRE);
        cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1
    })
}

/// The groups of a part list, in order of first appearance.
pub fn part_groups(parts: &[Part]) -> Vec<String> {
    let mut g: Vec<String> = vec![];
    for p in parts {
        if !g.contains(&p.group) {
            g.push(p.group.clone());
        }
    }
    g
}

/// Nest parts grouped by their attribute (v44 3.1.1; rule GZ3): the groups are nested one after another, each
/// group in its own zone. A group starts on the last sheet after the parts
/// already nested there (when any are, rule GZ2) plus
/// the part gap and the zone gap (CT1 A788); what doesn't fit goes on new sheets.
pub fn nest_grouped(parts: &[Part], spec: &SheetSpec, zone_gap: f64) -> NestResult {
    let groups = part_groups(parts);
    if groups.len() < 2 {
        return nest(parts, spec);
    }
    let mut out = NestResult::default();
    let mut methods: Vec<String> = vec![];
    for g in groups {
        let mut sub: Vec<Part> = parts.iter().map(|p| { let mut q = p.clone(); if q.group != g { q.qty = 0; } q }).collect();
        if let Some(last) = out.sheets.last_mut() {
            if any_part_in_zone(last, spec) {
                let used = last.iter().map(|p| p.x + p.w).fold(spec.margin, f64::max);
                let x0 = used + spec.gap + zone_gap.max(0.0);
                if spec.w - spec.margin - x0 > spec.gap {
                    let zspec = SheetSpec { w: spec.w - x0 + spec.margin, ..spec.clone() };
                    let r = nest(&sub, &zspec);
                    out.tried += r.tried;
                    if let Some(first) = r.sheets.into_iter().next() {
                        for mut p in first {
                            p.x += x0 - spec.margin;
                            sub[p.part].qty = sub[p.part].qty.saturating_sub(1);
                            last.push(p);
                        }
                    }
                }
            }
        }
        if sub.iter().any(|p| p.qty > 0) {
            let r = nest(&sub, spec);
            out.tried += r.tried;
            if !r.method.is_empty() {
                methods.push(r.method.clone());
            }
            out.sheets.extend(r.sheets);
        }
    }
    methods.dedup();
    out.method = format!("grouped by attribute ({})", methods.join(", "));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_round_shapes_stay_within_the_drawing_tolerance() {
        // BL38: a big disc or slot split too coarsely would stay straight moves.
        let sag = |p: &[P], r: f64| p.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt() / 2.0).filter(|h| *h < r / 2.0).map(|h| r - (r * r - h * h).sqrt()).fold(0.0, f64::max);
        assert!(sag(&circle(0.0, 0.0, 400.0, 64), 400.0) <= crate::dxf::ARC_CHORD_ERR + 1e-9);
        assert!(sag(&slot(0.0, 0.0, 400.0, 60.0), 60.0) <= crate::dxf::ARC_CHORD_ERR + 1e-9);
    }

    #[test]
    fn nests_without_overlap() {
        let parts = vec![
            Part { name: "A".into(), shape: Shape::Rectangle, w: 400.0, h: 300.0, qty: 12, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] },
            Part { name: "B".into(), shape: Shape::Disc, w: 250.0, h: 250.0, qty: 9, rotate: true, color: 1, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] },
        ];
        let spec = SheetSpec { w: 1500.0, h: 1000.0, margin: 10.0, gap: 8.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None };
        let res = nest(&parts, &spec);
        let total: usize = res.sheets.iter().map(|s| s.len()).sum();
        assert_eq!(total, 21);
        for s in &res.sheets {
            for (i, a) in s.iter().enumerate() {
                assert!(spec.fits(a));
                for b in &s[i + 1..] {
                    // Corrected for true-shape nesting: discs sit in each other's
                    // pockets, so their boxes may come closer than the gap; the
                    // real contours (circles) may not.
                    assert!(!clash_exact(&parts, a, b, spec.gap - 1e-6), "{a:?} and {b:?} closer than the gap");
                }
            }
        }
        assert_eq!(res.formats.len(), res.sheets.len());
    }

    #[test]
    fn a_remnant_is_used_first_and_parts_stay_inside_its_shape() {
        let parts = vec![Part { name: "A".into(), shape: Shape::Rectangle, w: 300.0, h: 200.0, qty: 30, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 10.0, gap: 8.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None };
        // An L-shaped remnant: 1000 × 1000 with the top-left 600 × 600 gone.
        let l = crate::stock::Outline { outer: vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [600.0, 1000.0], [600.0, 400.0], [0.0, 400.0]], holes: vec![] };
        let rem = SheetFormat { w: 1000.0, h: 1000.0, outline: Some(l.clone()), stock_ref: "2-16".into(), remnant: true, made: vec![], made_from: 0 };
        let plan = [FormatSlot { format: rem, qty: Some(1) }, FormatSlot { format: SheetFormat::rect(2000.0, 1000.0), qty: None }];
        let res = nest_plan(&parts, &spec, &plan);
        assert_eq!(res.sheets.iter().map(|s| s.len()).sum::<usize>(), 30);
        assert_eq!(res.formats[0].stock_ref, "2-16");
        assert!(res.formats[1..].iter().all(|f| !f.remnant), "only one remnant was in stock");
        assert!(!res.sheets[0].is_empty());
        // Every part on the remnant lies inside the L, at least the margin from its edge.
        let inner = crate::region::offset(&l, spec.margin);
        let rects = crate::region::free_rects(&inner[0]);
        for p in &res.sheets[0] {
            let inside = rects.iter().any(|r| p.x >= r.x - 1e-6 && p.y >= r.y - 1e-6 && p.x + p.w <= r.x + r.w + 1e-6 && p.y + p.h <= r.y + r.h + 1e-6);
            assert!(inside, "{p:?} is outside the remnant");
        }
    }

    #[test]
    fn best_combination_takes_a_smaller_format_when_the_next_would_be_poorly_used() {
        // Four 450 squares: 18 % of a 3000 × 1500 sheet, 81 % of a 1000 × 1000 one.
        let parts = vec![Part { name: "Q".into(), shape: Shape::Rectangle, w: 450.0, h: 450.0, qty: 4, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 10.0, gap: 8.0, material: 0, thickness: 3.0, level: 1, machine: Default::default(), kerf: Default::default(), gas: None };
        let big = SheetFormat { stock_ref: "BIG".into(), ..SheetFormat::rect(3000.0, 1500.0) };
        let small = SheetFormat { stock_ref: "SMALL".into(), ..SheetFormat::rect(1000.0, 1000.0) };
        let plan = [FormatSlot { format: big, qty: Some(2) }, FormatSlot { format: small, qty: Some(2) }];
        let plain = nest_plan(&parts, &spec, &plan);
        assert_eq!(plain.formats.iter().map(|f| f.stock_ref.as_str()).collect::<Vec<_>>(), ["BIG"]);
        let combo = nest_plan_opt(&parts, &spec, &plan, Some(0.5));
        assert_eq!(combo.formats.iter().map(|f| f.stock_ref.as_str()).collect::<Vec<_>>(), ["SMALL"]);
        assert_eq!(combo.sheets[0].len(), 4);
        // Nothing reaches a 95 % minimum: the format placing more part area wins
        // (here both place all four, so the next in order stays).
        let strict = nest_plan_opt(&parts, &spec, &plan, Some(0.95));
        assert_eq!(strict.formats[0].stock_ref, "BIG");
    }

    #[test]
    fn shaped_parts_hook_into_a_remnant_but_stay_inside_its_outline() {
        // L brackets on the same L-shaped remnant, then a full sheet for the rest:
        // every outline stays inside the remnant's shape, at least the margin from
        // its edge, and the parts keep the gap.
        let parts = vec![Part { name: "L".into(), shape: Shape::LBracket, w: 220.0, h: 180.0, qty: 25, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 10.0, gap: 8.0, material: 0, thickness: 3.0, level: 1, machine: Default::default(), kerf: Default::default(), gas: None };
        let l = crate::stock::Outline { outer: vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [600.0, 1000.0], [600.0, 400.0], [0.0, 400.0]], holes: vec![] };
        let rem = SheetFormat { w: 1000.0, h: 1000.0, outline: Some(l.clone()), stock_ref: "R".into(), remnant: true, made: vec![], made_from: 0 };
        let plan = [FormatSlot { format: rem, qty: Some(1) }, FormatSlot { format: SheetFormat::rect(2000.0, 1000.0), qty: None }];
        let res = nest_plan(&parts, &spec, &plan);
        assert_eq!(res.sheets.iter().map(|s| s.len()).sum::<usize>(), 25);
        assert!(res.formats[0].remnant && res.sheets[0].len() >= 4, "{:?}", res.sheets.iter().map(|s| s.len()).collect::<Vec<_>>());
        for p in &res.sheets[0] {
            assert!(crate::region::poly_inside(&l, spec.margin - 1e-3, &placed_shape(&parts, p).0), "{p:?} leaves the remnant");
        }
        for s in &res.sheets {
            for (i, a) in s.iter().enumerate() {
                for b in &s[i + 1..] {
                    assert!(!clash_exact(&parts, a, b, spec.gap - 1e-6), "{a:?} and {b:?} closer than the gap");
                }
            }
        }
    }
}

#[cfg(test)]
mod level_tests {
    use super::*;

    fn part(name: &str, shape: Shape, w: f64, h: f64, qty: u32) -> Part {
        Part { name: name.into(), shape, w, h, qty, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    #[test]
    fn a_disc_in_a_round_parts_hole_is_no_clash() {
        // A Ø300 disc has a Ø90 hole: a Ø20 disc fits in it, not on the ring.
        let parts = vec![part("Ring", Shape::Disc, 300.0, 300.0, 1), part("Pin", Shape::Disc, 20.0, 20.0, 1)];
        let at = |part, x, y, w| Placed { part, x, y, w, h: w, turns: 0, mirror: false };
        let ring = at(0, 0.0, 0.0, 300.0);
        assert!(!clash_exact(&parts, &at(1, 140.0, 140.0, 20.0), &ring, 5.0), "in the hole");
        assert!(clash_exact(&parts, &at(1, 140.0, 40.0, 20.0), &ring, 5.0), "on the ring");
    }
    fn spec(level: u8) -> SheetSpec {
        SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level, machine: Default::default(), kerf: Default::default(), gas: None }
    }
    fn check_no_overlap(parts: &[Part], res: &NestResult, sp: &SheetSpec) {
        for s in &res.sheets {
            for (i, a) in s.iter().enumerate() {
                assert!(sp.fits(a), "{a:?} off the sheet");
                let oa: Vec<P> = parts[a.part].geometry().outline.iter().map(|&q| a.map(q)).collect();
                for b in &s[i + 1..] {
                    let ob: Vec<P> = parts[b.part].geometry().outline.iter().map(|&q| b.map(q)).collect();
                    // True shapes keep the gap (a part inside another part's hole is fine).
                    let inside = point_in_poly(&oa, ob[0]) || point_in_poly(&ob, oa[0]);
                    if !inside {
                        assert!(!too_close(&oa, &ob, sp.gap - 0.2), "{a:?} and {b:?} closer than the gap");
                    }
                }
            }
        }
    }

    #[test]
    fn levels_match_the_original_configuration_file() {
        let m: Vec<usize> = LEVELS.iter().map(|l| l.methods).collect();
        assert_eq!(m, vec![4, 7, 11, 16, 18]);
        assert!(LEVELS.iter().all(|l| l.band == 3.0 && l.min_module_eff == 0.8 && l.rot_step == 30.0 && l.regular_tests == 99 && l.special_rot == 2));
        assert_eq!(LEVELS[3].dir, NestDir::Both);
        assert!(LEVELS[4].fill_holes && LEVELS[4].corners && !LEVELS[3].fill_holes);
        for l in &LEVELS {
            assert_eq!(methods_for(l).len(), l.methods);
        }
    }

    #[test]
    fn placement_turns_and_mirror_keep_the_part_in_its_footprint() {
        let p = part("T", Shape::Triangle, 100.0, 60.0, 1);
        for turns in 0..4u8 {
            for mirror in [false, true] {
                let (w, h) = if turns % 2 == 1 { (60.0, 100.0) } else { (100.0, 60.0) };
                let pl = Placed { part: 0, x: 10.0, y: 20.0, w, h, turns, mirror };
                for q in p.geometry().outline {
                    let s = pl.map(q);
                    assert!(s[0] >= 10.0 - 1e-9 && s[0] <= 10.0 + w + 1e-9 && s[1] >= 20.0 - 1e-9 && s[1] <= 20.0 + h + 1e-9);
                }
            }
        }
        let pl = Placed { part: 0, x: 0.0, y: 0.0, w: 100.0, h: 60.0, turns: 2, mirror: false };
        assert_eq!(pl.map([0.0, 0.0]), [100.0, 60.0]);
        assert_eq!(pl.angle(), 180);
    }

    #[test]
    fn rotation_permissions_are_respected() {
        let mut p = part("S", Shape::Rectangle, 300.0, 100.0, 12);
        p.rotate = false;
        let parts = vec![p];
        for lv in 1..=5 {
            let r = nest(&parts, &spec(lv));
            assert!(r.sheets.iter().flatten().all(|pl| pl.turns % 2 == 0 && !pl.mirror), "level {lv}");
        }
        let mut q = part("S", Shape::Rectangle, 300.0, 100.0, 4);
        q.rots.plain = [false, true, false, false]; // only 90°
        let r = nest(&[q], &spec(1));
        assert!(r.sheets.iter().flatten().all(|pl| pl.turns == 1));
    }

    #[test]
    fn gussets_pair_up_turned_180_degrees_at_the_module_levels() {
        let parts = vec![part("G", Shape::Triangle, 200.0, 150.0, 20)];
        let one = nest(&parts, &spec(1));
        let three = nest(&parts, &spec(3));
        let turned = three.sheets.iter().flatten().filter(|p| p.turns >= 2).count();
        assert!(turned >= 8, "half of the gussets turned to interlock: {turned}");
        let extent = |r: &NestResult| r.sheets.iter().flatten().map(|p| p.x + p.w).fold(0.0, f64::max);
        // Corrected for true-shape nesting: the strip engine already turns gussets
        // into each other at level 1 (the original needs no module for that), so
        // the module level is no longer strictly shorter — only never longer.
        assert!(one.sheets.iter().flatten().filter(|p| p.turns >= 2).count() >= 8, "level 1 interlocks too");
        assert!(extent(&three) <= extent(&one) + 1e-6, "pairs use no more sheet: {} vs {}", extent(&three), extent(&one));
        check_no_overlap(&parts, &three, &spec(3));
    }

    #[test]
    fn discs_go_in_staggered_rows() {
        // 75 discs: a square grid holds 72 per sheet, staggered rows hold more.
        let parts = vec![part("D", Shape::Disc, 150.0, 150.0, 75)];
        assert_eq!(nest(&parts, &spec(1)).sheets.len(), 2);
        let r = nest(&parts, &spec(3));
        assert_eq!(r.sheets.len(), 1, "{}", r.method);
        let ys: Vec<f64> = r.sheets[0].iter().map(|p| p.y).collect();
        let row = (150.0f64 + 5.0) * 3f64.sqrt() / 2.0;
        assert!(ys.iter().any(|&y| ((y - ys.iter().cloned().fold(f64::MAX, f64::min)) - row).abs() < 0.2), "a row at the staggered pitch: {ys:?}");
        check_no_overlap(&parts, &r, &spec(3));
    }

    #[test]
    fn small_parts_fill_holes_at_the_maximum_level() {
        // A plate with a big hole and small squares that fit inside it.
        let parts = vec![part("Plate", Shape::PlateHole, 900.0, 900.0, 1), part("Tab", Shape::Rectangle, 60.0, 60.0, 40)];
        let r = nest(&parts, &spec(5));
        let plate = r.sheets[0].iter().find(|p| p.part == 0).copied().unwrap();
        let inside = r.sheets[0].iter().filter(|p| p.part == 1 && plate.contains([p.x, p.y]) && plate.contains([p.x + p.w, p.y + p.h])).count();
        assert!(inside > 0, "some tabs sit in the plate's hole");
        check_no_overlap(&parts, &r, &spec(5));
    }

    #[test]
    fn groups_are_nested_in_their_own_zones() {
        let mut a = part("A", Shape::Rectangle, 200.0, 150.0, 6);
        a.group = "Customer 1".into();
        let mut b = part("B", Shape::Rectangle, 120.0, 100.0, 8);
        b.group = "Customer 2".into();
        let parts = vec![a, b];
        let sp = spec(3);
        assert_eq!(part_groups(&parts), vec!["Customer 1".to_string(), "Customer 2".to_string()]);
        let r = nest_grouped(&parts, &sp, 20.0);
        let n: usize = r.sheets.iter().map(|s| s.len()).sum();
        assert_eq!(n, 14);
        check_no_overlap(&parts, &r, &sp);
        // On the shared sheet every B part starts after the A zone plus gap + zone gap.
        let s = &r.sheets[0];
        let a_end = s.iter().filter(|p| p.part == 0).map(|p| p.x + p.w).fold(0.0, f64::max);
        assert!(s.iter().filter(|p| p.part == 1).all(|p| p.x >= a_end + sp.gap + 20.0 - 1e-6));
        assert!(s.iter().any(|p| p.part == 1), "group 2 shares the first sheet");
        // GZ2: "any part in the zone" is judged by the part's centre.
        assert!(any_part_in_zone(s, &sp) && !any_part_in_zone(&[], &sp));
        // One group: the plain nesting.
        assert_eq!(nest_grouped(&parts[..1], &sp, 20.0).sheets.len(), nest(&parts[..1], &sp).sheets.len());
    }

    #[test]
    fn the_machine_fills_holes_at_every_level() {
        // PstAdi06.CT1 B134 ON (test parts inside holes), B076 0.0: holes take
        // parts at level 3 too, not only at the maximum level.
        assert!(crate::ct1::active().parts_in_holes && crate::ct1::active().min_hole_area == 0.0);
        let parts = vec![part("Plate", Shape::PlateHole, 900.0, 900.0, 1), part("Tab", Shape::Rectangle, 60.0, 60.0, 40)];
        let r = nest(&parts, &spec(3));
        let plate = r.sheets[0].iter().find(|p| p.part == 0).copied().unwrap();
        assert!(r.sheets[0].iter().any(|p| p.part == 1 && plate.contains([p.x, p.y]) && plate.contains([p.x + p.w, p.y + p.h])));
        check_no_overlap(&parts, &r, &spec(3));
        assert!((polygon_area(&[[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]]) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn every_level_nests_the_sample_without_overlaps() {
        let parts = vec![
            part("A", Shape::Rectangle, 400.0, 300.0, 6),
            part("B", Shape::Disc, 250.0, 250.0, 5),
            part("C", Shape::Triangle, 200.0, 160.0, 6),
            part("D", Shape::LBracket, 220.0, 180.0, 4),
            part("E", Shape::Flange, 300.0, 300.0, 2),
        ];
        for lv in 1..=5 {
            let sp = spec(lv);
            let r = nest(&parts, &sp);
            let n: usize = r.sheets.iter().map(|s| s.len()).sum();
            assert_eq!(n, 23, "level {lv}");
            assert_eq!(r.tried, LEVELS[lv as usize - 1].methods);
            check_no_overlap(&parts, &r, &sp);
        }
    }

    #[test]
    fn original_angle_rules() {
        assert_eq!(norm_angle(370.0), 10.0);
        assert_eq!(norm_angle(-90.0), 270.0);
        assert!(!special_rotation_possible(0, 30.0, 0.01));
        assert!(special_rotation_possible(1, 90.0, 0.01));
        assert!(special_rotation_possible(2, 30.0, 0.01) && !special_rotation_possible(2, 180.0, 0.01));
        assert_eq!(corner_after_axes(1, (false, false)), (true, false));
        assert_eq!(corner_after_axes(2, (false, false)), (true, true));
        assert_eq!(corner_after_axes(3, (true, true)), (true, false));
    }

    #[test]
    fn special_texts_fill_in_the_part_data() {
        let p = part("BRK-12", Shape::Rectangle, 100.0, 50.0, 7);
        let t = PartText { text: "#REF/#QTY #NUM".into(), at: [0.0, 0.0], height: 5.0, angle: 0.0, curve: 0.0, kind: TextKind::Special };
        assert_eq!(t.resolved(&p, "Mild steel", 3.0, 4), "BRK-12/7 4");
        let m = PartText { kind: TextKind::Mark, ..t.clone() };
        assert_eq!(m.resolved(&p, "Mild steel", 3.0, 4), "#REF/#QTY #NUM");
        assert!(TextKind::Mark.machined() && TextKind::Special.machined() && !TextKind::Note.machined());
    }
}

/// The true-shape strip nester (bands.rs) in shop terms.
#[cfg(test)]
mod band_tests {
    use super::*;

    fn part(name: &str, shape: Shape, w: f64, h: f64, qty: u32) -> Part {
        Part { name: name.into(), shape, w, h, qty, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }
    fn spec(w: f64, h: f64, level: u8) -> SheetSpec {
        SheetSpec { w, h, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level, machine: Default::default(), kerf: Default::default(), gas: None }
    }
    /// Every pair keeps the gap by the exact contour test, and every part stays
    /// inside the margins.
    fn assert_clean(parts: &[Part], r: &NestResult, sp: &SheetSpec) {
        for s in &r.sheets {
            for (i, a) in s.iter().enumerate() {
                assert!(sp.fits(a), "{a:?} crosses the margin");
                for b in &s[i + 1..] {
                    assert!(!clash_exact(parts, a, b, sp.gap - 1e-6), "{a:?} and {b:?} closer than the gap");
                }
            }
        }
    }

    #[test]
    fn a_small_l_tucks_into_a_big_ls_notch() {
        // The big L's notch is 180 × 180; nothing fits above it on this sheet.
        let parts = vec![part("Big L", Shape::LBracket, 300.0, 300.0, 1), part("Small L", Shape::LBracket, 150.0, 150.0, 1)];
        let sp = spec(1000.0, 400.0, 1);
        let r = nest(&parts, &sp);
        assert_eq!(r.sheets.len(), 1);
        let big = r.sheets[0].iter().find(|p| p.part == 0).copied().unwrap();
        let small = r.sheets[0].iter().find(|p| p.part == 1).copied().unwrap();
        // It tucks into the notch (its box overlaps the big L's box, it doesn't go
        // beside it); turned 180° its own notch may even hook round the upright.
        let overlap = small.x < big.x + big.w && big.x < small.x + small.w && small.y < big.y + big.h && big.y < small.y + small.h;
        assert!(overlap && small.x + small.w <= big.x + big.w, "the small L sits in the big one's notch: {big:?} {small:?}");
        assert_clean(&parts, &r, &sp);
    }

    #[test]
    fn a_small_part_drops_into_a_big_parts_hole() {
        // Plate 300 with a 120 hole; a 50 square would also fit right of the plate,
        // but the hole is further left, so the first fit from the left is the hole.
        let parts = vec![part("Plate", Shape::PlateHole, 300.0, 300.0, 1), part("Tab", Shape::Rectangle, 50.0, 50.0, 1)];
        let sp = spec(400.0, 320.0, 1);
        let r = nest(&parts, &sp);
        assert_eq!(r.sheets.len(), 1);
        let plate = r.sheets[0].iter().find(|p| p.part == 0).copied().unwrap();
        let tab = r.sheets[0].iter().find(|p| p.part == 1).copied().unwrap();
        let hole = &placed_shape(&parts, &plate).1[0];
        let corners = [[tab.x, tab.y], [tab.x + tab.w, tab.y], [tab.x, tab.y + tab.h], [tab.x + tab.w, tab.y + tab.h]];
        assert!(corners.iter().all(|&c| point_in_poly(hole, c)), "the tab is in the hole: {tab:?}");
        assert_clean(&parts, &r, &sp);
    }

    #[test]
    fn no_two_parts_come_closer_than_the_gap_at_any_level() {
        let parts = vec![
            part("Plate", Shape::PlateHole, 260.0, 260.0, 2),
            part("L", Shape::LBracket, 180.0, 140.0, 5),
            part("Gusset", Shape::Triangle, 150.0, 110.0, 6),
            part("Disc", Shape::Disc, 120.0, 120.0, 7),
            part("Flange", Shape::Flange, 160.0, 120.0, 3),
            part("Slot", Shape::SlottedPlate, 140.0, 60.0, 4),
            part("Tab", Shape::Rectangle, 40.0, 30.0, 12),
        ];
        let need: usize = parts.iter().map(|p| p.qty as usize).sum();
        for lv in [1u8, 4] {
            let sp = spec(1200.0, 700.0, lv);
            let r = nest(&parts, &sp);
            assert_eq!(r.sheets.iter().map(|s| s.len()).sum::<usize>(), need, "level {lv}");
            assert_clean(&parts, &r, &sp);
        }
    }

    #[test]
    fn parts_reach_the_margin_but_never_cross_it() {
        // B078 on: the usable area grows by half the gap, so the first part's real
        // edge sits on the margin, not half a gap inside it.
        let parts = vec![part("Tab", Shape::Rectangle, 90.0, 70.0, 40)];
        let sp = SheetSpec { margin: 12.0, ..spec(600.0, 400.0, 1) };
        let r = nest(&parts, &sp);
        assert_clean(&parts, &r, &sp);
        let all: Vec<&Placed> = r.sheets.iter().flatten().collect();
        assert!(all.iter().any(|p| (p.x - sp.margin).abs() < 0.2 && (p.y - sp.margin).abs() < 0.2), "a part in the corner at the margin");
        assert!(all.iter().all(|p| p.x >= sp.margin - 1e-6 && p.y >= sp.margin - 1e-6));
    }

    #[test]
    fn horizontal_nesting_is_vertical_turned_on_its_side() {
        let parts = vec![part("A", Shape::Rectangle, 300.0, 100.0, 3), part("B", Shape::Rectangle, 150.0, 120.0, 4), part("C", Shape::Rectangle, 80.0, 80.0, 5)];
        let left: Vec<u32> = parts.iter().map(|p| p.qty).collect();
        let (sv, sh) = (spec(1000.0, 600.0, 1), spec(600.0, 1000.0, 1));
        let v = Engine::new(&parts, &sv, &LEVELS[0]).fill(&Method { code: 0, dir: NestDir::Vertical, modules: false, regular: false }, &left);
        let h = Engine::new(&parts, &sh, &LEVELS[0]).fill(&Method { code: 0, dir: NestDir::Horizontal, modules: false, regular: false }, &left);
        assert_eq!(v.placed.len(), 12);
        let key = |p: &Placed| (p.part, (p.x * 1e3).round() as i64, (p.y * 1e3).round() as i64, (p.w * 1e3).round() as i64);
        let mut a: Vec<_> = v.placed.iter().map(key).collect();
        let mut b: Vec<_> = h.placed.iter().map(|p| key(&Placed { x: p.y, y: p.x, w: p.h, h: p.w, ..*p })).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        // Rows fill from the bottom instead of columns from the left.
        assert!(h.placed.iter().map(|p| p.y + p.h).fold(0.0, f64::max) < 1000.0 - 5.0);
    }

    #[test]
    fn methods_follow_the_level_and_cost_rules() {
        // Level 1 tries cost rules 1–4; level 5 tries rules 1–9 in both directions.
        let m1 = methods_for(&LEVELS[0]);
        assert_eq!(m1.iter().map(|m| m.code).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        let m5 = methods_for(&LEVELS[4]);
        assert_eq!(m5.len(), 18);
        assert!(m5.iter().filter(|m| m.dir == NestDir::Horizontal).count() == 9 && m5.last().unwrap().code == 8);
        // Rule 1: the centre of gravity's X; rule 7: the mean of X and Y.
        assert_eq!(spot_cost(0, 10.0, 20.0, (5.0, 6.0), (10.0, 12.0), 1000.0, 500.0), (15.0, None));
        assert_eq!(spot_cost(1, 10.0, 20.0, (5.0, 6.0), (10.0, 12.0), 1000.0, 500.0), (15.0, Some(26.0)));
        assert_eq!(spot_cost(7, 10.0, 20.0, (5.0, 6.0), (10.0, 12.0), 1000.0, 500.0).0, 20.5);
        // No CT1 strip width: the rebuild's default; A003 is not used (A020 = 0).
        assert!(LEVELS.iter().all(|l| !l.own_band && band_width(l) == BAND_DEFAULT));
    }
}

/// Release-mode measurement of the sample job (the app's demo parts on its
/// default 2000 × 1000 sheet): `cargo test --release nesting_bench -- --ignored --nocapture`.
#[cfg(test)]
mod nesting_bench {
    use super::*;

    fn sample() -> Vec<Part> {
        let p = |name: &str, shape, w, h, qty| Part { name: String::from(name), shape, w, h, qty, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let mut v = vec![
            p("810", Shape::Strip, 980.0, 60.0, 4),
            p("1210", Shape::Strip, 1440.0, 60.0, 4),
            p("Square", Shape::Rectangle, 190.0, 190.0, 2),
            p("Square hole", Shape::PlateHole, 190.0, 190.0, 2),
            p("Ring", Shape::Disc, 200.0, 200.0, 8),
            p("Bracket", Shape::LBracket, 150.0, 120.0, 12),
            p("Gusset", Shape::Triangle, 120.0, 120.0, 14),
            p("Clip", Shape::Flange, 110.0, 70.0, 16),
            p("Angle", Shape::LBracket, 60.0, 45.0, 40),
        ];
        if let Some((g, _)) = crate::dxf::read(crate::dxf::SAMPLE_DXF).ok().and_then(|d| crate::dxf::build(&d, &crate::dxf::sample_roles()).ok()) {
            let mut q = p("Plate-R60 (DXF)", Shape::Custom, g.w, g.h, 3);
            q.custom = Some(std::sync::Arc::new(g));
            v.push(q);
        }
        v
    }

    #[test]
    #[ignore]
    fn sample_job_sheets_and_utilisation() {
        let parts = sample();
        let net: f64 = parts.iter().map(|p| p.net_area() * p.qty as f64).sum();
        let need: u32 = parts.iter().map(|p| p.qty).sum();
        for lv in [1u8, 3, 5] {
            let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: lv, machine: Default::default(), kerf: Default::default(), gas: None };
            let t = std::time::Instant::now();
            let mut r = nest(&parts, &spec);
            let t_nest = t.elapsed();
            for s in r.sheets.iter_mut() {
                compact_contours(&parts, s, &spec);
            }
            let t_all = t.elapsed();
            let placed: usize = r.sheets.iter().map(|s| s.len()).sum();
            // The last sheet counts up to its farthest part along X (columns) or
            // along Y (rows), whichever leaves the bigger remnant.
            let (mx, my) = r.sheets.last().map_or((0.0, 0.0), |s| (s.iter().map(|p| p.x + p.w).fold(0.0, f64::max), s.iter().map(|p| p.y + p.h).fold(0.0, f64::max)));
            let last = (mx * spec.h).min(my * spec.w);
            let used = (r.sheets.len().saturating_sub(1)) as f64 * spec.w * spec.h + last;
            println!(
                "level {lv}: {placed}/{need} parts, {} sheet(s), last sheet used to X {mx:.0} / Y {my:.0} ({:.0}% of it), utilisation {:.1}% of the sheets, {:.1}% of the used area; nest {:.0} ms (+compaction {:.0} ms); method {}",
                r.sheets.len(),
                100.0 * last / (spec.w * spec.h),
                100.0 * net / (r.sheets.len() as f64 * spec.w * spec.h),
                100.0 * net / used.max(1.0),
                t_nest.as_secs_f64() * 1e3,
                (t_all - t_nest).as_secs_f64() * 1e3,
                r.method
            );
        }
    }
}

#[cfg(test)]
mod readable_text_tests {
    use super::*;

    #[test]
    fn text_angle_snap_follows_the_original() {
        assert_eq!(snap_text_angle(47.0, 0.0), 47.0);
        assert_eq!(snap_text_angle(47.0, 90.0), 90.0);
        assert_eq!(snap_text_angle(181.0, 360.0), 0.0);
        assert_eq!(snap_text_angle(181.0, 359.9), 0.0);
        assert_eq!(snap_text_angle(350.0, 45.0), 0.0);
    }

    #[test]
    fn marking_text_stays_level_and_unmirrored_on_turned_parts() {
        let t = PartText { text: "TA".into(), at: [20.0, 10.0], height: 12.0, angle: 0.0, curve: 0.0, kind: TextKind::Mark };
        for (turns, mirror) in [(0u8, false), (1, false), (2, false), (3, true), (0, true)] {
            let (w, h) = if turns % 2 == 1 { (60.0, 100.0) } else { (100.0, 60.0) };
            let pl = Placed { part: 0, x: 500.0, y: 300.0, w, h, turns, mirror };
            let st = t.sheet_strokes("TA", &pl, TEXT_ANGLE_STEP);
            // The T's top bar is level and runs left to right.
            let bar = &st[0];
            assert!((bar[0][1] - bar[1][1]).abs() < 1e-6 && bar[1][0] > bar[0][0], "turns {turns} mirror {mirror}: {bar:?}");
            // The A comes after the T (reads left to right, not mirrored).
            let tx = st[0][0][0];
            let ax = st.last().unwrap()[0][0];
            assert!(ax > tx);
            // Its middle is where the part puts it.
            let local_mid = [20.0 + crate::font::width("TA", 12.0) / 2.0, 16.0];
            let m = pl.map(local_mid);
            let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for s in &st {
                for p in s {
                    x0 = x0.min(p[0]);
                    x1 = x1.max(p[0]);
                    y0 = y0.min(p[1]);
                    y1 = y1.max(p[1]);
                }
            }
            assert!(((x0 + x1) / 2.0 - m[0]).abs() < 1e-6 && ((y0 + y1) / 2.0 - m[1]).abs() < 1e-6);
        }
    }
}
