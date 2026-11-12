//! A part's cut order (CO3–CO7): the order its contours are cut and marked
//! in. The programmer sets any part of it by hand; the rest follows the
//! automatic order, and the outline is always cut last. The order set by hand
//! is a list of contour numbers (`dxf::CustomGeo::hand_order`), so contour
//! numbers never change: leads, joints, picked starts and hand-set qualities
//! keep pointing at the same contours.
//!
//! The holes nobody has ordered by hand follow the machine's hole order
//! setting (CO14, CO15, `hole_order`): on the part screen and in `part-info`
//! where the part is drawn, and on a sheet where each copy sits.

use super::contours::Look;
use crate::ct1::{HoleMode, HoleOrder, MachineTech};
use crate::dxf::{self, Contour, ContourState};
use crate::nest::{CutQuality, Part, Shape, P};

/// Whether a contour has a place in the cut order: it is cut or marked.
fn in_order(c: &Contour) -> bool {
    matches!(c.state, ContourState::Cut | ContourState::Mark)
}

/// The automatic order of a part's contours (CO5): the marks first, while
/// the part is still held by the sheet, then the cut holes, then the
/// outline, each in number order. No Cut, Bend line and Leave out contours
/// have no place.
pub fn automatic(list: &[Contour]) -> Vec<usize> {
    let marks = (0..list.len()).filter(|&k| list[k].state == ContourState::Mark);
    marks.chain(dxf::cut_order(list)).collect()
}

/// The order set by hand, as one list (CO3, CO4): each command's contours
/// in the order added, the outline moved last. Only contours the part has
/// and cuts or marks, each once, so a file edited elsewhere can't point the
/// order at anything else. Empty for a built-in shape.
pub fn hand(part: &Part) -> Vec<usize> {
    let Some(geo) = custom(part) else { return vec![] };
    let list = geo.contour_list();
    let o = dxf::outline_index(&list);
    let mut all: Vec<usize> = vec![];
    for &k in geo.hand_order.iter().flatten() {
        if k < list.len() && in_order(&list[k]) && !all.contains(&k) {
            all.push(k);
        }
    }
    all.sort_by_key(|&k| Some(k) == o);
    all
}

/// Contours `picks` (numbers from 0) gathered by one sweep on the part
/// screen, in the order passed (CO11): the outline, passed before another
/// contour, can't take that place (CO4), so it is left out of the pick,
/// with the reason; the rest go in as one command. The outline passed last
/// stays.
pub fn hold_outline(list: &[Contour], picks: &[usize]) -> (Vec<usize>, Option<(usize, String)>) {
    let Some(o) = dxf::outline_index(list) else { return (picks.to_vec(), None) };
    match picks.iter().position(|&k| k == o) {
        Some(at) if at + 1 < picks.len() => {
            let rest = picks.iter().copied().filter(|&k| k != o).collect();
            let why = format!("Contour {}, the part's outline, is left out: it is always cut last, or the part drops before its holes are cut.", o + 1);
            (rest, Some((o, why)))
        }
        _ => (picks.to_vec(), None),
    }
}

/// An imported part's geometry; a built-in shape has none to order.
fn custom(part: &Part) -> Option<&dxf::CustomGeo> {
    match (&part.custom, part.shape) {
        (Some(geo), Shape::Custom) => Some(geo),
        _ => None,
    }
}

/// A part's cut order (CO3, CO5) on the active machine: every contour cut
/// or marked (numbers from 0), in the order the part cuts it, and whether it
/// was set by hand. See `order_with`.
pub fn order(part: &Part) -> Vec<(usize, bool)> {
    order_with(part, &crate::ct1::active().hole_order)
}

/// A part's cut order (CO3, CO5, CO14) where it is drawn, with the hole
/// order setting `holes`: every contour cut or marked (numbers from 0), in
/// the order the part cuts it, and whether it was set by hand. The order set
/// by hand comes first, then the rest in the automatic order: the marks, the
/// holes as `holes` orders them from where the head is (the drawing's origin,
/// or the last hole set by hand, CO27), and the outline always last.
pub fn order_with(part: &Part, holes: &HoleOrder) -> Vec<(usize, bool)> {
    let list = super::contours::list(part);
    let hand = hand(part);
    let o = dxf::outline_index(&list);
    let by_hand = hand.iter().filter(|&&k| Some(k) != o).map(|&k| (k, true));
    let (marks, cuts): (Vec<usize>, Vec<usize>) = automatic(&list).into_iter().filter(|k| !hand.contains(k) && Some(*k) != o).partition(|&k| list[k].state == ContourState::Mark);
    // Each hole where the part's drawing has it, from where its cut starts
    // when that is set by hand (CO21).
    let cut = dxf::cut_order(&list);
    let at = |k: usize| HoleAt::new(&list[k].pts, cut.iter().position(|&c| c == k).and_then(|ci| hand_start(part, ci)));
    let from = hand.iter().rev().find(|&&k| Some(k) != o && list[k].state == ContourState::Cut).map_or([0.0, 0.0], |&k| at(k).at);
    let (w, h) = part.bbox();
    let outline = o.map_or([0.0, 0.0, w, h], |k| bbox(&list[k].pts));
    let found: Vec<HoleAt> = cuts.iter().map(|&k| at(k)).collect();
    let cuts = hole_order(&found, outline, from, holes).into_iter().map(|j| cuts[j]);
    let auto = marks.into_iter().chain(cuts).map(|k| (k, false));
    let outline = o.filter(|&k| in_order(&list[k])).map(|k| (k, hand.contains(&k)));
    by_hand.chain(auto).chain(outline).collect()
}

/// Where cut contour `ci` (holes first, the outline last, as the sheet
/// numbers them) starts when that is set on the part by hand: a lead moved
/// by hand, else a start picked on an element. Part coordinates.
pub fn hand_start(part: &Part, ci: usize) -> Option<P> {
    let tech = part.tech.as_ref()?;
    tech.lead_edit(ci).map(|e| e.at).or_else(|| tech.starts.iter().flatten().find(|s| s.0 as usize == ci).map(|s| s.1))
}

/// The box of a contour: [x0, y0, x1, y1].
fn bbox(pts: &[P]) -> [f64; 4] {
    pts.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
}

/// A hole to order, in the frame it is ordered in (the part's drawing, or
/// the sheet where a copy sits): its box, and its point, which the head is
/// measured to and stands at once the hole is cut (CO21): where its cut
/// starts when that is set by hand, else the middle of its box (MAC-61).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoleAt {
    pub lo: P,
    pub hi: P,
    pub at: P,
}

impl HoleAt {
    pub fn new(pts: &[P], start: Option<P>) -> HoleAt {
        let b = bbox(pts);
        HoleAt { lo: [b[0], b[1]], hi: [b[2], b[3]], at: start.unwrap_or([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]) }
    }

    /// Big for Cut small holes first (CO25): its box reaches the minimum
    /// length along X or the minimum width along Y, within the tolerance.
    fn big(&self, s: &HoleOrder) -> bool {
        self.hi[0] - self.lo[0] >= s.big_length - s.tol - SLACK || self.hi[1] - self.lo[1] >= s.big_width - s.tol - SLACK
    }

    /// Whether this hole's box lies in or overlaps `big`'s box grown by the
    /// offset on every side (CO25).
    fn next_to(&self, big: &HoleAt, s: &HoleOrder) -> bool {
        let d = s.big_offset;
        self.lo[0] <= big.hi[0] + d && self.hi[0] >= big.lo[0] - d && self.lo[1] <= big.hi[1] + d && self.hi[1] >= big.lo[1] - d
    }
}

/// The order the machine's hole order setting `s` cuts `holes` in (CO14,
/// CO15): `holes` in the part's own contour order, the head at `from`, the
/// part's outline box `outline` ([x0, y0, x1, y1]), all in one frame. The
/// holes' places in `holes`, in cut order.
///
/// None and Minimum heat keep the part's own order. By proximity takes the
/// nearest hole next (CO21); By direction cuts rows or columns from a start
/// corner, every other one back (CO23, CO24). With Cut small holes first,
/// the small holes next to a big hole go just before it, in the same mode
/// among themselves (CO25).
pub fn hole_order(holes: &[HoleAt], outline: [f64; 4], from: P, s: &HoleOrder) -> Vec<usize> {
    let mut left: Vec<usize> = (0..holes.len()).collect();
    let mut out = Vec::with_capacity(holes.len());
    let mut run = Run::new(holes, outline, from, s);
    let mut head = from;
    while !left.is_empty() {
        let k = run.pick(holes, &left, head);
        if s.small_first && holes[k].big(s) {
            let small: Vec<usize> = left.iter().copied().filter(|&j| j != k && !holes[j].big(s) && holes[j].next_to(&holes[k], s)).collect();
            if !small.is_empty() {
                let found: Vec<HoleAt> = small.iter().map(|&j| holes[j]).collect();
                for j in hole_order(&found, outline, head, &HoleOrder { small_first: false, ..*s }).into_iter().map(|j| small[j]) {
                    left.retain(|&x| x != j);
                    out.push(j);
                }
            }
        }
        left.retain(|&x| x != k);
        head = holes[k].at;
        out.push(k);
    }
    out
}

/// Slack for comparing lengths: a hole exactly on a tolerance's edge is
/// within it, whatever the rounding of its points.
const SLACK: f64 = 1e-9;

/// One part's run through its holes in a hole order mode: By direction's
/// start corner, which way the run goes and the edge of the last hole cut.
struct Run {
    mode: HoleMode,
    tol: f64,
    ortho: f64,
    columns: bool,
    /// The start corner is on the left (rows) or at the bottom (columns).
    from_low: bool,
    /// The start side's edge is the low one (bottom for rows, left for
    /// columns): the start corner is at the bottom (rows) or on the left
    /// (columns).
    side_low: bool,
    /// The run goes the way from the start corner (right, or up), not back.
    forward: bool,
    /// The start-side edge of the hole cut last (CO24).
    last: f64,
}

impl Run {
    fn new(holes: &[HoleAt], outline: [f64; 4], from: P, s: &HoleOrder) -> Run {
        // CO23: the start corner, chosen once per part; corner 0 is the one
        // nearest the head, on a tie lower left, lower right, upper right,
        // upper left.
        let corners = [[outline[0], outline[1]], [outline[2], outline[1]], [outline[2], outline[3]], [outline[0], outline[3]]];
        let d = |c: P| (c[0] - from[0]).hypot(c[1] - from[1]);
        let corner = match s.corner {
            1..=4 => s.corner as usize - 1,
            _ => (1..4).fold(0, |best, c| if d(corners[c]) < d(corners[best]) { c } else { best }),
        };
        let (left, bottom) = (corner == 0 || corner == 3, corner <= 1);
        let (from_low, side_low) = if s.columns { (bottom, left) } else { (left, bottom) };
        let mut run = Run { mode: s.mode, tol: s.tol, ortho: s.ortho_tol, columns: s.columns, from_low, side_low, forward: true, last: 0.0 };
        // For the first hole, the extreme start-side edge over all the holes (CO24).
        let edges = holes.iter().map(|h| run.side(h));
        run.last = if side_low { edges.fold(f64::MAX, f64::min) } else { edges.fold(f64::MIN, f64::max) };
        run
    }

    /// A hole's start-side edge: its bottom or top edge for rows, its left
    /// or right edge for columns.
    fn side(&self, h: &HoleAt) -> f64 {
        let a = if self.columns { 0 } else { 1 };
        if self.side_low { h.lo[a] } else { h.hi[a] }
    }

    /// The next hole to cut among `left` (places in `holes`, in the part's
    /// own order), the head at `head`.
    fn pick(&mut self, holes: &[HoleAt], left: &[usize], head: P) -> usize {
        match self.mode {
            HoleMode::None | HoleMode::MinimumHeat => left[0],
            HoleMode::Proximity => self.nearest(holes, left, head),
            HoleMode::Direction => {
                let mut k = self.in_row(holes, left);
                // CO24: a new row or column has begun, so the run turns back.
                if (self.side(&holes[k]) - self.last).abs() > self.ortho + SLACK {
                    self.forward = !self.forward;
                    k = self.in_row(holes, left);
                }
                self.last = self.side(&holes[k]);
                k
            }
        }
    }

    /// CO21: the nearest hole to the head. A hole replaces the best so far
    /// when it is nearer by more than the tolerance; when the two are within
    /// it, when it is nearer sideways (along X) by more than the tolerance.
    fn nearest(&self, holes: &[HoleAt], left: &[usize], head: P) -> usize {
        let dist = |k: usize| (holes[k].at[0] - head[0]).hypot(holes[k].at[1] - head[1]);
        let side = |k: usize| (holes[k].at[0] - head[0]).abs();
        let (mut best, mut bd, mut bs) = (left[0], dist(left[0]), side(left[0]));
        for &k in &left[1..] {
            let (d, sd) = (dist(k), side(k));
            if d < bd - self.tol {
                (best, bd, bs) = (k, d, sd);
            } else if (d - bd).abs() <= self.tol && sd < bs - self.tol {
                (best, bs) = (k, sd);
            }
        }
        best
    }

    /// CO23 steps 1–3: the row (or column) nearest the start corner, then
    /// the hole in it nearest the start of the run. Among equals the hole
    /// met first stays.
    fn in_row(&self, holes: &[HoleAt], left: &[usize]) -> usize {
        let edges = left.iter().map(|&k| self.side(&holes[k]));
        let row = if self.side_low { edges.fold(f64::MAX, f64::min) } else { edges.fold(f64::MIN, f64::max) };
        let a = if self.columns { 1 } else { 0 };
        // Running away from the start corner: the smallest low edge first;
        // running back: the largest high edge first.
        let ahead = self.forward == self.from_low;
        let key = |k: usize| if ahead { holes[k].lo[a] } else { -holes[k].hi[a] };
        let mut best: Option<usize> = None;
        for &k in left.iter().filter(|&&k| (self.side(&holes[k]) - row).abs() <= self.ortho + SLACK) {
            if best.is_none_or(|b| key(k) < key(b)) {
                best = Some(k);
            }
        }
        best.unwrap_or(left[0])
    }
}

/// One step of a part's machining on a sheet, numbered as the sheet plan
/// numbers it: a cut contour of the part's geometry (its cut holes, then the
/// outline, as `cnc::sheet_contours` lists them) or a marking line of its
/// drawing (`dxf::CustomGeo::marks`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Cut(usize),
    Mark(usize),
}

/// The order a sheet cuts and marks part `part` in (CO8, CO9): its cut
/// order (`order`), the order set by hand first and the outline last, each
/// contour as the sheet plan numbers it. With no order set by hand, the
/// marks first, then the holes and the outline, all as the part's geometry
/// lists them: what every sheet cut before an order set by hand reached it.
/// That is the automatic order (CO5), and for a part saved before contour
/// states it keeps its marks in the order they were saved in. The holes not
/// set by hand are listed in the part's own order: each copy on a sheet
/// orders them by the machine's setting where it sits (CO14, `hole_order`),
/// which doesn't move the marks, all made before them.
pub fn sheet_steps(part: &Part) -> Vec<Step> {
    let marks = part.custom.as_ref().map_or(0, |g| g.marks.len());
    let geo = match custom(part) {
        Some(geo) if !hand(part).is_empty() => geo,
        _ => {
            let cuts = part.geometry().holes.len() + 1;
            return (0..marks).map(Step::Mark).chain((0..cuts).map(Step::Cut)).collect();
        }
    };
    let list = geo.contour_list();
    let cut = dxf::cut_order(&list);
    let mark = geo.mark_numbers();
    let step = |k: usize| cut.iter().position(|&c| c == k).map(Step::Cut).or_else(|| mark.iter().position(|&m| m == k).map(Step::Mark));
    order_with(part, &HoleOrder::NONE).into_iter().filter_map(|(k, _)| step(k)).collect()
}

/// The holes of part `part` set by hand in its cut order (CO3), in that
/// order, as the sheet plan numbers its cut contours (holes first). The
/// rest of its holes follow the machine's hole order on each sheet (CO14).
pub fn sheet_hand_holes(part: &Part) -> Vec<usize> {
    let Some(geo) = custom(part) else { return vec![] };
    let list = geo.contour_list();
    let cut = dxf::cut_order(&list);
    let o = dxf::outline_index(&list);
    hand(part).into_iter().filter(|&k| Some(k) != o).filter_map(|k| cut.iter().position(|&c| c == k)).collect()
}

/// A command on a part's cut order (CO3, CO5, CO6). Each one adds to the
/// end of the order, or takes back the last one, or empties it.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// These contours (numbers from 0), in the order given.
    Contours(Vec<usize>),
    /// Every mark not yet ordered.
    AllMarks,
    /// Every cut contour not yet ordered.
    AllCut,
    /// Every cut contour at this cut quality not yet ordered.
    Quality(CutQuality),
    /// Take back the last command.
    Undo,
    /// Empty the order: every contour in the automatic order.
    Clear,
}

/// What a cut-order command did.
pub struct Ordered {
    /// The contours it added (numbers from 0), in the order added.
    pub added: Vec<usize>,
    /// The contours it named that were already in the order (CO5).
    pub skipped: Vec<usize>,
    /// The order changed.
    pub changed: bool,
    /// What was done, in the shop's words.
    pub message: String,
}

/// Why a command that adds `picks` (numbers from 0, in order) can't be done
/// (CO4): it names a contour after the outline, which is always cut last.
fn outline_refusal(list: &[Contour], picks: &[usize]) -> Option<String> {
    let o = dxf::outline_index(list)?;
    let at = picks.iter().position(|&k| k == o)?;
    let after = picks[at + 1..].first()?;
    Some(format!(
        "Contour {} is the part's outline, and this names contour {} after it: the outline is always cut last, or the part drops before its holes are cut (and its marks are made). Name the outline last, or leave it out — it is cut last anyway.",
        o + 1,
        after + 1
    ))
}

/// Contour numbers from 0 as the shop reads them: "1, 3 and 5".
fn numbers(v: &[usize]) -> String {
    let n: Vec<String> = v.iter().map(|k| (k + 1).to_string()).collect();
    match n.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => n.join(""),
    }
}

/// "contour" or "contours", for `n` of them.
fn contours_word(n: usize) -> &'static str {
    if n == 1 { "contour" } else { "contours" }
}

/// The refusal for a built-in shape, whose contours follow the automatic order.
fn built_in(part: &Part) -> String {
    format!(
        "'{}' is a built-in {}: its contours are cut in the automatic order (holes, then the outline). To set its cut order, save it as a drawing (export-part-dxf) and import that.",
        part.name,
        part.shape.label().to_lowercase()
    )
}

/// Whether `part` can take a cut order set by hand (CO3): an imported part
/// can; a built-in shape is refused, with what to do instead.
pub fn check(part: &Part) -> Result<(), String> {
    custom(part).map(|_| ()).ok_or_else(|| built_in(part))
}

/// The order set by hand, command by command, to change.
fn steps_mut(part: &mut Part) -> &mut Vec<Vec<usize>> {
    let geo = part.custom.as_mut().expect("an imported part");
    &mut std::sync::Arc::make_mut(geo).hand_order
}

/// Run a cut-order command on part `part`, whose contours are cut at the
/// qualities `mt` gives (CQ1, CQ6). The one function the agents'
/// `part-cut-order` and the part screen's Contours menu call. A refusal
/// leaves the part as it was.
pub fn apply(part: &mut Part, mt: &MachineTech, cmd: &Command) -> Result<Ordered, String> {
    let Some(geo) = custom(part) else { return Err(built_in(part)) };
    let none = format!("'{}' has no cut order set by hand: its contours are cut in the automatic order.", part.name);
    let done = |changed: bool, message: String| Ok(Ordered { added: vec![], skipped: vec![], changed, message });
    let empty = geo.hand_order.is_empty();
    match cmd {
        // CO6: undo takes back the last command, clear empties the order.
        Command::Undo if empty => Err(format!("Nothing to undo: {none}")),
        Command::Undo => {
            let last = steps_mut(part).pop().unwrap_or_default();
            done(true, format!("took back the last command: {} {} out of the cut order.", contours_word(last.len()), numbers(&last)))
        }
        Command::Clear if empty => done(false, none),
        Command::Clear => {
            steps_mut(part).clear();
            done(true, "cut order set by hand cleared: every contour is cut in the automatic order.".into())
        }
        _ => {
            let (picks, what) = picks(part, mt, cmd)?;
            append(part, picks, what)
        }
    }
}

/// The contours (numbers from 0) a command that adds names, in its order,
/// and how it names them in shop words when it is a bulk command (CO5).
fn picks(part: &Part, mt: &MachineTech, cmd: &Command) -> Result<(Vec<usize>, Option<String>), String> {
    let list = super::contours::list(part);
    // A bulk command takes the contours not yet ordered in the automatic
    // order, the holes in the machine's hole order as the part is drawn
    // (CO14); those already ordered come first, in number order, and are
    // skipped.
    let shown = order_with(part, &mt.hole_order);
    let mut auto: Vec<usize> = shown.iter().filter(|o| o.1).map(|o| o.0).collect();
    auto.sort_unstable();
    auto.extend(shown.iter().filter(|o| !o.1).map(|o| o.0));
    let kind = |f: &dyn Fn(usize) -> bool| auto.iter().copied().filter(|&k| f(k)).collect::<Vec<usize>>();
    match cmd {
        Command::Contours(v) => match v.iter().find(|&&k| !in_order(&list[k])) {
            Some(&k) => Err(format!(
                "Contour {} is set to {}: it is neither cut nor marked, so it has no place in the cut order. Cut or mark it first (contour-tech), or leave it out of the order.",
                k + 1,
                list[k].state.label()
            )),
            None => Ok((v.clone(), None)),
        },
        Command::AllMarks => {
            let v = kind(&|k| list[k].state == ContourState::Mark);
            if v.is_empty() {
                return Err(format!("'{}' has no marks. Mark contours first (contour-tech state=mark), or add contours by number.", part.name));
            }
            Ok((v, Some("every mark".into())))
        }
        Command::AllCut => Ok((kind(&|k| list[k].state == ContourState::Cut), Some("every cut contour".into()))),
        Command::Quality(q) => {
            let looks = super::contours::looks(part, mt);
            let v = kind(&|k| looks[k] == Look::Cut(*q));
            if v.is_empty() {
                let mut have: Vec<usize> = looks.iter().filter_map(|l| if let Look::Cut(c) = l { Some(c.number() - 1) } else { None }).collect();
                have.sort_unstable();
                have.dedup();
                return Err(format!(
                    "No contour of '{}' is cut at quality {}: its cut contours are at quality {} (part-info shows each contour's). Give one of those.",
                    part.name,
                    q.number(),
                    numbers(&have)
                ));
            }
            Ok((v, Some(format!("every contour cut at quality {}", q.number()))))
        }
        Command::Undo | Command::Clear => Ok((vec![], None)),
    }
}

/// Add `picks` (numbers from 0, in order) to the end of the part's cut
/// order as one command (CO3–CO5): contours already in it are skipped, and
/// the outline stays last. `what` names a bulk command's contours.
fn append(part: &mut Part, picks: Vec<usize>, what: Option<String>) -> Result<Ordered, String> {
    let ordered = hand(part);
    let (mut added, mut skipped) = (vec![], vec![]);
    for k in picks {
        if ordered.contains(&k) || added.contains(&k) {
            skipped.push(k);
        } else {
            added.push(k);
        }
    }
    if let Some(e) = outline_refusal(&super::contours::list(part), &added) {
        return Err(e);
    }
    if added.is_empty() {
        let message = match what {
            Some(w) => format!("nothing added: {w} is already in the cut order."),
            None if skipped.len() == 1 => format!("nothing added: contour {} is already in the cut order.", numbers(&skipped)),
            None => format!("nothing added: contours {} are already in the cut order.", numbers(&skipped)),
        };
        return Ok(Ordered { added, skipped, changed: false, message });
    }
    let mut message = format!("{} {} added to the cut order.", contours_word(added.len()), numbers(&added));
    if !skipped.is_empty() {
        message.push_str(&format!(" Skipped {}: already in it.", numbers(&skipped)));
    }
    steps_mut(part).push(added.clone());
    Ok(Ordered { added, skipped, changed: true, message })
}

/// Take contours (numbers from 0) out of the order set by hand (CO7): a
/// contour set to another state leaves it, and the automatic order places
/// it again if it is still cut or marked. A command left with no contours
/// goes, so undo takes back the one before it.
pub fn forget(part: &mut Part, ks: &[usize]) {
    let Some(geo) = part.custom.as_mut().filter(|g| g.hand_order.iter().flatten().any(|k| ks.contains(k))) else { return };
    let geo = std::sync::Arc::make_mut(geo);
    geo.hand_order.iter_mut().for_each(|step| step.retain(|k| !ks.contains(k)));
    geo.hand_order.retain(|step| !step.is_empty());
}
