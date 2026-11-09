//! Each contour of a part and its state (Cut, Mark, No cut, Bend line, Leave
//! out; MECHANICS CS1–CS5): the numbered list agents and the window see, and
//! setting contours to a state. The part's outline, holes and marking lines
//! are made from the states (`dxf::CustomGeo::apply_states`), so the nester,
//! leads, kerf, times and posts only ever see what is cut or marked.

use crate::cnc::ManualCut;
use crate::dxf::{self, Contour, ContourState};
use crate::nest::{Part, Placed, Shape, P};

/// A part's contours in number order (CS1): holes first, the outline, then
/// the open lines. A built-in shape's are its holes and outline, all Cut.
pub fn list(part: &Part) -> Vec<Contour> {
    if let (Shape::Custom, Some(c)) = (part.shape, &part.custom) {
        return c.contour_list();
    }
    let g = part.geometry();
    let cut = |pts: &Vec<P>| Contour { pts: pts.clone(), closed: true, state: ContourState::Cut };
    g.holes.iter().map(cut).chain(std::iter::once(cut(&g.outline))).collect()
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

/// Set contours (numbers from 0) of an imported part to a state; with `same`,
/// every contour of the same shape as one of them too (CS4, CS5). The outline
/// is always Cut and an open line can't be cut yet: those are refused, with
/// the reason, and the part is left as it was. Leads and joints placed on a
/// contour stay on it, or go when it is no longer cut.
pub fn set_state(part: &mut Part, picks: &[usize], state: ContourState, same: bool) -> Result<StateChange, String> {
    let Some(geo) = part.custom.as_mut().filter(|_| part.shape == Shape::Custom) else {
        return Err(format!(
            "'{}' is a built-in {} — its contours are always cut. Save it as a drawing (export-part-dxf) and import that to set its contours.",
            part.name,
            part.shape.label().to_lowercase()
        ));
    };
    let mut list = geo.contour_list();
    let n = list.len();
    if let Some(k) = picks.iter().find(|&&k| k >= n) {
        return Err(no_contour(n, k + 1));
    }
    let outline = dxf::outline_index(&list);
    let mut chosen: Vec<usize> = picks.to_vec();
    if same {
        // The outline is never the same shape as a contour inside it.
        chosen.extend((0..n).filter(|&j| Some(j) != outline && picks.iter().any(|&k| same_shape(&list[k], &list[j]))));
    }
    chosen.sort_unstable();
    chosen.dedup();
    for &k in &chosen {
        if Some(k) == outline && state != ContourState::Cut {
            return Err(format!(
                "Contour {} is the part's outline: it is always cut — without it there is no part to cut out, so it can't be {}. Set the contours inside it instead, or take the part out of the job (remove-part).",
                k + 1,
                state.label()
            ));
        }
        if !list[k].closed && state == ContourState::Cut {
            return Err(format!(
                "Contour {} is an open line: cutting open lines isn't built yet (FEATURES TEC-22), and cut through the part it could split it. Set it to mark, no-cut, bend-line or leave-out.",
                k + 1
            ));
        }
    }
    let before = dxf::cut_order(&list);
    for &k in &chosen {
        list[k].state = state;
    }
    let after = dxf::cut_order(&list);
    let geo = std::sync::Arc::make_mut(geo);
    geo.contours = list;
    geo.apply_states();
    let cut_moved: Vec<Option<usize>> = before.iter().map(|c| after.iter().position(|a| a == c)).collect();
    // A lead or joint picked on a contour follows it to its new place in the
    // cut order, or goes with it.
    if let Some(t) = part.tech.as_mut() {
        let follow = |s: &mut Option<(u8, P)>| {
            *s = s.and_then(|(ci, at)| cut_moved.get(ci as usize).copied().flatten().map(|n| (n as u8, at)));
        };
        t.starts.iter_mut().for_each(follow);
        t.joint_picks.iter_mut().for_each(follow);
    }
    Ok(StateChange { contours: chosen, cut_changed: before != after, cut_moved })
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
