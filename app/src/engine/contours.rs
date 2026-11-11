//! Each contour of a part and its state (Cut, Mark, No cut, Bend line, Leave
//! out; MECHANICS CS1–CS5): the numbered list agents and the window see, and
//! setting contours to a state. The part's outline, holes and marking lines
//! are made from the states (`dxf::CustomGeo::apply_states`), so the nester,
//! leads, kerf, times and posts only ever see what is cut or marked.

use crate::cnc::ManualCut;
use crate::dxf::{self, Contour, ContourState};
use crate::nest::{CutQuality, Part, Placed, Shape, P};

/// A part's contours in number order (CS1): holes first, the outline, then
/// the open lines. A built-in shape's are its holes and outline, all Cut.
pub fn list(part: &Part) -> Vec<Contour> {
    if let (Shape::Custom, Some(c)) = (part.shape, &part.custom) {
        return c.contour_list();
    }
    let g = part.geometry();
    let cut = |pts: &Vec<P>| Contour::new(pts.clone(), true, ContourState::Cut);
    g.holes.iter().map(cut).chain(std::iter::once(cut(&g.outline))).collect()
}

/// How a contour shows on screen (CS7): a cut contour by its cut quality,
/// the others by their state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Cut(CutQuality),
    Mark,
    NoCut,
    BendLine,
    LeaveOut,
}

/// How each contour of `part` shows, in number order (CS7): a Cut contour
/// at the quality it is cut at on the machine `mt` (CQ1, CQ6), the others
/// by their state.
pub fn looks(part: &Part, mt: &crate::ct1::MachineTech) -> Vec<Look> {
    let tech = part.tech.unwrap_or_default();
    list(part)
        .iter()
        .map(|c| match c.state {
            ContourState::Cut => Look::Cut(crate::cnc::contour_quality(&tech, mt, &c.pts, c.quality).0),
            ContourState::Mark => Look::Mark,
            ContourState::NoCut => Look::NoCut,
            ContourState::BendLine => Look::BendLine,
            ContourState::LeaveOut => Look::LeaveOut,
        })
        .collect()
}

/// What a contour is on the part, for agents: "hole", "outside" (the
/// outline) or "open".
pub fn kind(list: &[Contour], k: usize) -> &'static str {
    if !list[k].closed {
        "open"
    } else if dxf::outline_index(list) == Some(k) {
        "outside"
    } else {
        "hole"
    }
}

/// The refusal for a contour number the part doesn't have (`k` from 1).
fn no_contour(n: usize, k: usize) -> String {
    format!("The part has {n} contours (1–{n}, see part-info); there is no contour {k}.")
}

/// Contour numbers as an agent writes them ("5", "1,3", "2-4", [1, 3]),
/// from 0, checked against the part's `n` contours.
pub fn parse_numbers(s: &str, n: usize) -> Result<Vec<usize>, String> {
    let mut v = vec![];
    for tok in s.split([',', ' ', ';', '[', ']']).map(str::trim).filter(|t| !t.is_empty()) {
        let (a, b) = match tok.split_once('-') {
            Some((a, b)) => (a.trim().parse::<usize>(), b.trim().parse::<usize>()),
            None => (tok.parse::<usize>(), tok.parse::<usize>()),
        };
        let (Ok(a), Ok(b)) = (a, b) else { return Err(format!("contours: '{tok}' isn't a contour number (e.g. 5, 1,3 or 2-4; see part-info).")) };
        if let Some(k) = [a, b].into_iter().find(|&k| k == 0 || k > n) {
            return Err(no_contour(n, k));
        }
        v.extend((a.min(b)..=a.max(b)).map(|k| k - 1));
    }
    if v.is_empty() {
        return Err("Name the contours, e.g. contours=5 or contours=1,3 (numbers from part-info).".into());
    }
    v.sort_unstable();
    v.dedup();
    Ok(v)
}

/// How near the points of two contours of the same shape must be, mm.
const SAME_TOL: f64 = dxf::JOIN_TOL;

/// Whether two contours are the same shape in another place (CS5): both open
/// or both closed, the same number of points, and moved onto each other
/// (lower-left corners of their boxes together) every point matches within
/// 0.01 mm, run either way and, closed, from any point. A turned or mirrored
/// copy is not the same.
pub fn same_shape(a: &Contour, b: &Contour) -> bool {
    let n = a.pts.len();
    if a.closed != b.closed || n != b.pts.len() || n == 0 {
        return false;
    }
    let (ba, bb) = (crate::nest::bbox_of(&a.pts), crate::nest::bbox_of(&b.pts));
    if ((ba[2] - ba[0]) - (bb[2] - bb[0])).abs() > SAME_TOL || ((ba[3] - ba[1]) - (bb[3] - bb[1])).abs() > SAME_TOL {
        return false;
    }
    let pa: Vec<P> = a.pts.iter().map(|p| [p[0] - ba[0], p[1] - ba[1]]).collect();
    let pb: Vec<P> = b.pts.iter().map(|p| [p[0] - bb[0], p[1] - bb[1]]).collect();
    let near = |p: P, q: P| (p[0] - q[0]).abs() <= SAME_TOL && (p[1] - q[1]).abs() <= SAME_TOL;
    // Each way round, from each point of `b` that lies on `a`'s first point.
    let starts: Vec<usize> = if a.closed { (0..n).filter(|&j| near(pa[0], pb[j])).collect() } else { vec![0, n - 1] };
    starts.into_iter().any(|j| {
        let fwd = |i: usize| if a.closed { pb[(j + i) % n] } else if j == 0 { pb[i] } else { pb[n - 1 - i] };
        let back = |i: usize| pb[(j + n - i) % n];
        (0..n).all(|i| near(pa[i], fwd(i))) || (a.closed && (0..n).all(|i| near(pa[i], back(i))))
    })
}

/// What setting contours changed: the contours (numbers from 0) and, for each
/// contour the part cut before (in `dxf::cut_order`), where it is in the new
/// cut order, or `None` when it is no longer cut.
pub struct StateChange {
    pub contours: Vec<usize>,
    pub cut_moved: Vec<Option<usize>>,
    /// The part cuts other contours than before.
    pub cut_changed: bool,
}

impl StateChange {
    /// Some contour that was cut no longer is.
    pub fn cuts_fewer(&self) -> bool {
        self.cut_moved.contains(&None)
    }
}

/// An imported part's contours, or the refusal for a built-in shape, whose
/// contours are always cut at the part's qualities.
fn custom_contours(part: &Part) -> Result<Vec<Contour>, String> {
    match (&part.custom, part.shape) {
        (Some(geo), Shape::Custom) => Ok(geo.contour_list()),
        _ => Err(format!(
            "'{}' is a built-in {} — its contours are always cut. Save it as a drawing (export-part-dxf) and import that to set its contours.",
            part.name,
            part.shape.label().to_lowercase()
        )),
    }
}

/// The picked contours (numbers from 0) and, with `same`, every contour of
/// the same shape as one of them (CS5; never the outline), in number order.
fn with_same(list: &[Contour], picks: &[usize], same: bool) -> Result<Vec<usize>, String> {
    let n = list.len();
    if let Some(k) = picks.iter().find(|&&k| k >= n) {
        return Err(no_contour(n, k + 1));
    }
    let outline = dxf::outline_index(list);
    let mut chosen: Vec<usize> = picks.to_vec();
    if same {
        // The outline is never the same shape as a contour inside it.
        chosen.extend((0..n).filter(|&j| Some(j) != outline && picks.iter().any(|&k| same_shape(&list[k], &list[j]))));
    }
    chosen.sort_unstable();
    chosen.dedup();
    Ok(chosen)
}

/// Save a changed contour list on the part and remake its outline, holes and
/// marks from it.
fn store(part: &mut Part, list: Vec<Contour>) {
    if let Some(geo) = part.custom.as_mut() {
        let geo = std::sync::Arc::make_mut(geo);
        geo.contours = list;
        geo.apply_states();
    }
}

/// Why contour `k` (from 0) of `list` can't be set to `state` (CS4): the
/// outline is always cut, and an open line can't be cut yet.
fn state_refusal(list: &[Contour], k: usize, state: ContourState) -> Option<String> {
    if Some(k) == dxf::outline_index(list) && state != ContourState::Cut {
        return Some(format!(
            "Contour {} is the part's outline: it is always cut — without it there is no part to cut out, so it can't be {}. Set the contours inside it instead, or take the part out of the job (remove-part).",
            k + 1,
            state.label()
        ));
    }
    if !list[k].closed && state == ContourState::Cut {
        return Some(format!(
            "Contour {} is an open line: cutting open lines isn't built yet (FEATURES TEC-22), and cut through the part it could split it. Set it to mark, no-cut, bend-line or leave-out.",
            k + 1
        ));
    }
    None
}

/// Set contours (numbers from 0) of an imported part to a state; with `same`,
/// every contour of the same shape as one of them too (CS4, CS5). The outline
/// is always Cut and an open line can't be cut yet: those are refused, with
/// the reason, and the part is left as it was. Leads and joints placed on a
/// contour stay on it, or go when it is no longer cut. A contour no longer
/// cut loses a quality set on it by hand (CQ6).
pub fn set_state(part: &mut Part, picks: &[usize], state: ContourState, same: bool) -> Result<StateChange, String> {
    let mut list = custom_contours(part)?;
    let chosen = with_same(&list, picks, same)?;
    if let Some(e) = chosen.iter().find_map(|&k| state_refusal(&list, k, state)) {
        return Err(e);
    }
    let before = dxf::cut_order(&list);
    for &k in &chosen {
        list[k].state = state;
        if state != ContourState::Cut {
            list[k].quality = None;
        }
    }
    let after = dxf::cut_order(&list);
    store(part, list);
    let cut_moved: Vec<Option<usize>> = before.iter().map(|c| after.iter().position(|a| a == c)).collect();
    // A lead or joint picked on a contour follows it to its new place in the
    // cut order, or goes with it.
    if let Some(t) = part.tech.as_mut() {
        let follow = |s: &mut Option<(u8, P)>| {
            *s = s.and_then(|(ci, at)| cut_moved.get(ci as usize).copied().flatten().map(|n| (n as u8, at)));
        };
        t.starts.iter_mut().for_each(follow);
        t.joint_picks.iter_mut().for_each(follow);
        // So does a lead moved by hand (LE1).
        for e in t.lead_edits.iter_mut() {
            *e = e.and_then(|x| cut_moved.get(x.contour as usize).copied().flatten().map(|n| crate::nest::LeadEdit { contour: n as u8, ..x }));
        }
    }
    Ok(StateChange { contours: chosen, cut_changed: before != after, cut_moved })
}

/// What setting a quality by hand changed (CQ6): the contours set (numbers
/// from 0), and the contours equal to a picked one that were left as they
/// are because they aren't cut, with their state.
pub struct QualityChange {
    pub contours: Vec<usize>,
    pub skipped: Vec<(usize, ContourState)>,
}

/// Why contour `k` (from 0) can't take a cut quality: it is a mark, or isn't
/// cut at all (CQ6). `q` is the quality asked for, for what to do instead.
fn not_cut(c: &Contour, k: usize, q: &str) -> String {
    let open = "it is an open line, and open lines can't be cut yet (FEATURES TEC-22)";
    match (c.state, c.closed) {
        (ContourState::Mark, true) => format!("Contour {} is a mark: marks are always made with the machine's marking quality and no pierce, so it can't be given a cutting quality. Leave it a mark, or to cut it instead set state=cut quality={q}.", k + 1),
        (ContourState::Mark, false) => format!("Contour {} is a mark: marks are always made with the machine's marking quality and no pierce, so it can't be given a cutting quality. Leave it a mark: {open}.", k + 1),
        (st, true) => format!("Contour {} is set to {}: it isn't cut, so it takes no cut quality. To cut it at that quality, set state=cut quality={q}.", k + 1, st.label()),
        (st, false) => format!("Contour {} is set to {}: it isn't cut, so it takes no cut quality, and {open}.", k + 1, st.label()),
    }
}

/// Set a cut quality by hand on contours (numbers from 0) of an imported
/// part, or with `None` give them back to the part's size ranges and quality
/// and the machine's size bands (CQ6); with `same`, on every contour of the
/// same shape as one of them too (CS5). A picked contour that isn't cut (a
/// mark always uses the marking quality) is refused, with what to do instead,
/// and the part is left as it was; an equal contour that isn't cut is left
/// as it is. `asked` is the quality as the agent wrote it, for the refusal.
pub fn set_quality(part: &mut Part, picks: &[usize], quality: Option<CutQuality>, same: bool, asked: &str) -> Result<QualityChange, String> {
    let mut list = custom_contours(part)?;
    let chosen = with_same(&list, picks, same)?;
    if let Some(&k) = picks.iter().find(|&&k| list[k].state != ContourState::Cut) {
        return Err(not_cut(&list[k], k, asked));
    }
    let (set, skipped): (Vec<usize>, Vec<usize>) = chosen.into_iter().partition(|&k| list[k].state == ContourState::Cut);
    for &k in &set {
        list[k].quality = quality;
    }
    let skipped = skipped.into_iter().map(|k| (k, list[k].state)).collect();
    store(part, list);
    Ok(QualityChange { contours: set, skipped })
}

/// What painting contours sets: a state, a cut quality by hand, or both
/// (CS4, CQ6). The agents' `contour-tech` and the window's cut technology
/// panel (CS6) both paint with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Paint {
    pub state: Option<ContourState>,
    /// `Some(Some(q))` sets quality `q` by hand; `Some(None)` gives the
    /// contours back to the part's size ranges and quality and the machine's
    /// size bands (CQ1).
    pub quality: Option<Option<CutQuality>>,
}

impl Paint {
    /// Set the contours to `state`, nothing else.
    pub fn state(state: ContourState) -> Paint {
        Paint { state: Some(state), quality: None }
    }

    /// Cut the contours, at quality `q` set by hand, or with `None` at the
    /// quality the size ranges and bands give.
    pub fn cut_at(q: Option<CutQuality>) -> Paint {
        Paint { state: Some(ContourState::Cut), quality: Some(q) }
    }

    /// Why this paint can't go on any contour: it sets nothing, or gives a
    /// quality to contours it makes something other than Cut (CQ6: a mark
    /// always uses the machine's marking quality).
    fn refusal(self) -> Option<String> {
        match (self.state, self.quality) {
            (None, None) => Some("Give a state (cut, mark, no-cut, bend-line or leave-out), a quality (1-12 or auto) for cut contours, or both.".into()),
            (Some(st), Some(_)) if st != ContourState::Cut => {
                let why = if st == ContourState::Mark {
                    "marks are always made with the machine's marking quality and no pierce, so a mark can't be given a cutting quality"
                } else {
                    "only cut contours take a cut quality"
                };
                Some(format!("state={} with a quality: {why}. Leave out the quality, or use state=cut with it.", st.key()))
            }
            _ => None,
        }
    }
}

/// A quality as an agent writes it, for refusals: its number, or auto.
fn asked(q: Option<CutQuality>) -> String {
    q.map_or("auto".to_string(), |q| q.number().to_string())
}

/// Why contour `k` (from 0) of `part` can't take `paint` on its own (CS4,
/// CQ6), or `None` when `paint` would paint it: the refusal `paint` gives
/// for it. The cut technology panel paints a box or a sweep with the
/// contours that can take its row and names the rest (CS6).
pub fn refusal(part: &Part, k: usize, paint: Paint) -> Option<String> {
    if let Some(e) = paint.refusal() {
        return Some(e);
    }
    let list = match custom_contours(part) {
        Ok(list) => list,
        Err(e) => return Some(e),
    };
    if k >= list.len() {
        return Some(no_contour(list.len(), k + 1));
    }
    if let Some(e) = paint.state.and_then(|st| state_refusal(&list, k, st)) {
        return Some(e);
    }
    // A quality goes on the contour once the paint has made it Cut.
    match (paint.quality, paint.state.unwrap_or(list[k].state)) {
        (Some(q), st) if st != ContourState::Cut => Some(not_cut(&list[k], k, &asked(q))),
        _ => None,
    }
}

/// What a paint did.
pub struct Painted {
    /// The contours set (numbers from 0).
    pub contours: Vec<usize>,
    /// Contours equal to a picked one left as they are because only cut
    /// contours take a quality (CQ6), with their state.
    pub skipped: Vec<(usize, ContourState)>,
    /// The part cuts other contours than before, so its nest is out of date
    /// (CS4).
    pub nest_changed: bool,
    /// What was done, in the shop's words.
    pub message: String,
}

/// Paint contours `picks` (numbers from 0) of part `i` with `paint`; with
/// `same`, every contour equal to one of them too (CS5). The one function the
/// agents' `contour-tech` and the window's cut technology panel both call. A
/// refusal (CS4, CQ6) leaves the part, its sheets and their hand-picked cut
/// order as they were. Hand picks on the sheets follow the part's new cut
/// contours (CS4); `nest_changed` says the caller's nest is out of date.
pub fn paint(parts: &mut [Part], sheets: &[Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, picks: &[usize], paint: Paint, same: bool) -> Result<Painted, String> {
    let Some(part) = parts.get(i) else { return Err(format!("There is no part {} (the job has {}).", i + 1, parts.len())) };
    if let Some(e) = paint.refusal() {
        return Err(e);
    }
    let numbers = |v: &[usize]| v.iter().map(|k| (k + 1).to_string()).collect::<Vec<_>>().join(", ");
    let plural = |v: &[usize]| if v.len() == 1 { "" } else { "s" };
    let mut p = part.clone();
    let mut message = format!("Part {} '{}':", i + 1, p.name);
    let (mut contours, mut skipped) = (vec![], vec![]);
    let ch = match paint.state {
        Some(st) => {
            let ch = set_state(&mut p, picks, st, same)?;
            message.push_str(&format!(" contour{} {} set to {}.", plural(&ch.contours), numbers(&ch.contours), st.label()));
            contours = ch.contours.clone();
            Some(ch)
        }
        None => None,
    };
    if let Some(q) = paint.quality {
        let qc = set_quality(&mut p, picks, q, same, &asked(q))?;
        let what = q.map_or("the part's size ranges and quality and the machine's size bands decide".to_string(), |q| format!("cut at quality {} (by hand)", q.number()));
        message.push_str(&format!(" Contour{} {}: {what}.", plural(&qc.contours), numbers(&qc.contours)));
        if !qc.skipped.is_empty() {
            let left: Vec<String> = qc.skipped.iter().map(|(k, st)| format!("{} ({})", k + 1, st.key())).collect();
            message.push_str(&format!(" Equal contour{} {} left as {}: only cut contours take a quality.", if left.len() == 1 { "" } else { "s" }, left.join(", "), if left.len() == 1 { "it is" } else { "they are" }));
        }
        contours = qc.contours;
        skipped = qc.skipped;
    }
    parts[i] = p;
    let ch = ch.filter(|ch| ch.cut_changed);
    if let Some(ch) = &ch {
        follow_on_sheets(ch, i, sheets, manual);
        message.push_str(if ch.cuts_fewer() {
            " It cuts fewer contours now: check its sheets (a part nested in a hole that is no longer cut overlaps it)."
        } else {
            " It cuts more contours now."
        });
    }
    Ok(Painted { contours, skipped, nest_changed: ch.is_some(), message })
}

/// After part `part`'s cut contours changed, the contours picked by hand for
/// the sheets' cut order (CS4) follow them, and picks on contours no longer
/// cut go.
pub fn follow_on_sheets(ch: &StateChange, part: usize, sheets: &[Vec<Placed>], manual: &mut [Vec<ManualCut>]) {
    for (sheet, picks) in sheets.iter().zip(manual.iter_mut()) {
        picks.retain_mut(|m| {
            if !sheet.get(m.placed).is_some_and(|pl| pl.part == part) {
                return true;
            }
            ch.cut_moved.get(m.contour).copied().flatten().map(|c| m.contour = c).is_some()
        });
    }
}
