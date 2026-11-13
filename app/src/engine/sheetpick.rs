//! Whole parts picked into a sheet's cut order in Manual cutting (MECHANICS
//! MM8, MM9): a sweep takes the parts the pointer passes over, in that order;
//! a box takes the parts wholly inside it (dragged to the right) or those it
//! touches (dragged to the left, SW1), in the sheet's automatic order among
//! themselves. Each part is cut whole at its place among the sheet's
//! picks, in its own cut order (`cnc::cut_jobs`). The window turns pointer
//! moves into these calls (with the cut technology panel's gestures,
//! `cutpanel::gestures`), and the agents' `cut-order parts` makes the same
//! calls. A click still picks one contour (`manual_pick`, MM4).

use super::selection::Window;
use crate::cnc::{self, ManualCut};
use crate::cutpanel;
use crate::dxf::{Contour, ContourState};
use crate::nest::{Part, Placed, SheetSpec, P};

/// The placed parts the sheet's picks so far finish (MM8): the picks cut
/// their outline, so they are cut whole already. That is a part taken whole,
/// one whose outline was clicked (its holes go first, MM2), or one sitting
/// in a picked hole (CO2). A part with only some holes clicked isn't.
pub fn picked(parts: &[Part], placed: &[Placed], manual: &[ManualCut]) -> Vec<bool> {
    // Which parts the picks finish doesn't depend on the order of their
    // holes, so the part's own order (CO14's None) is enough here.
    let hand = cnc::hand_holes(parts, placed);
    let outline: Vec<usize> = placed.iter().map(|pl| parts[pl.part].geometry().holes.len()).collect();
    let mut done = vec![false; placed.len()];
    for (i, ci, _) in cnc::cut_jobs(parts, placed, &hand, &crate::ct1::HoleOrder::NONE, &[], manual) {
        if ci == outline[i] {
            done[i] = true;
        }
    }
    done
}

/// What a sweep, a box or an agent's list gathered (MM8, MM9): the placed
/// parts to take whole, in order, and those it met that the picks already
/// finish (skipped, and named).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gathered {
    pub add: Vec<usize>,
    pub skipped: Vec<usize>,
}

impl Gathered {
    /// Placed part `i`, met now: taken once, or skipped when `done` says the
    /// picks already finish it.
    fn meet(&mut self, i: usize, done: &[bool]) {
        if self.add.contains(&i) || self.skipped.contains(&i) {
            return;
        }
        if done.get(i).copied().unwrap_or(false) {
            self.skipped.push(i);
        } else {
            self.add.push(i);
        }
    }
}

/// The contours of placed parts `ids` on the sheet (holes first, then the
/// outline, as `cnc::sheet_contours` gives them), and the placed part each
/// belongs to.
fn contours_of(parts: &[Part], placed: &[Placed], ids: impl Iterator<Item = usize>) -> (Vec<Contour>, Vec<usize>) {
    let (mut list, mut owner) = (vec![], vec![]);
    for i in ids {
        for (pts, _) in cnc::sheet_contours(&parts[placed[i].part], &placed[i]) {
            list.push(Contour::new(pts, true, ContourState::Cut));
            owner.push(i);
        }
    }
    (list, owner)
}

/// Every contour on the sheet as Manual cutting picks them: what the window
/// gives the cut technology panel's gestures, so a drag that starts on a
/// contour sweeps and one from open space draws a box (MM8, MM9).
pub fn contours(parts: &[Part], placed: &[Placed]) -> Vec<Contour> {
    contours_of(parts, placed, 0..placed.len()).0
}

/// A sweep across a sheet's parts in progress (MM8): what it gathered so
/// far, and where the pointer was.
#[derive(Clone, Debug)]
pub struct PartSweep {
    last: P,
    pub got: Gathered,
}

impl PartSweep {
    /// A sweep from `at` (sheet mm): the part whose contour is under the
    /// pointer is taken at once. `done` is `picked`; `half` the pick
    /// aperture, mm.
    pub fn start(parts: &[Part], placed: &[Placed], done: &[bool], at: P, half: f64) -> PartSweep {
        let mut s = PartSweep { last: at, got: Gathered::default() };
        s.to(parts, placed, done, at, half);
        s
    }

    /// The pointer moved on to `at` with the button held: the parts it
    /// passed since, in the order it reached them. A part is passed when
    /// the pointer comes within `half` mm of one of its contours; each is
    /// taken once, and those the picks already finish are skipped.
    pub fn to(&mut self, parts: &[Part], placed: &[Placed], done: &[bool], at: P, half: f64) {
        let a = self.last;
        let (lo, hi) = ([a[0].min(at[0]) - half, a[1].min(at[1]) - half], [a[0].max(at[0]) + half, a[1].max(at[1]) + half]);
        let near = (0..placed.len()).filter(|&i| {
            let q = placed[i].bbox4();
            q[0] <= hi[0] && q[2] >= lo[0] && q[1] <= hi[1] && q[3] >= lo[1]
        });
        let (list, owner) = contours_of(parts, placed, near);
        for k in cutpanel::contours_along_stroke(&list, a, at, half) {
            self.got.meet(owner[k], done);
        }
        self.last = at;
    }
}

/// The parts a sweep along `path` (sheet mm, from its first point) takes
/// (MM8): in the order the pointer passes over them, each once, those the
/// picks already finish (`done`) skipped.
pub fn sweep(parts: &[Part], placed: &[Placed], done: &[bool], path: &[P], half: f64) -> Gathered {
    let Some(&first) = path.first() else { return Gathered::default() };
    let mut s = PartSweep::start(parts, placed, done, first, half);
    for &p in &path[1..] {
        s.to(parts, placed, done, p, half);
    }
    s.got
}

/// The parts a box `w` (sheet mm) takes (MM9, SW1, SW3): those wholly inside
/// it, or with a crossing window those it touches, in the sheet's automatic
/// order among themselves (`cnc::sheet_order`); those the picks already
/// finish (`done`) are skipped.
pub fn boxed(parts: &[Part], placed: &[Placed], spec: &SheetSpec, done: &[bool], w: &Window) -> Gathered {
    let mut g = Gathered::default();
    for i in cnc::sheet_order(placed, spec).into_iter().filter(|&i| w.takes_placed(parts, &placed[i])) {
        g.meet(i, done);
    }
    g
}

/// Placed parts `ids` named by an agent, taken as a sweep would take them
/// (MM8): in the order named, each once, those the picks already finish
/// (`done`) skipped.
pub fn listed(done: &[bool], ids: &[usize]) -> Gathered {
    let mut g = Gathered::default();
    for &i in ids {
        g.meet(i, done);
    }
    g
}

/// "#4, #2 and #1": placed parts by their numbers on the sheet.
fn numbers(ids: &[usize]) -> String {
    let v: Vec<String> = ids.iter().map(|i| format!("#{}", i + 1)).collect();
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]),
    }
}

/// Take the parts `g` gathered whole, as one command at the end of the
/// sheet's picks `list` (MM8, MM9): each is cut whole there, in its own
/// cut order. Returns the status line; refused, changing nothing, when it
/// adds no part.
pub fn take(parts: &[Part], placed: &[Placed], list: &mut Vec<ManualCut>, g: &Gathered) -> Result<String, String> {
    let already = format!("{} {} already picked", numbers(&g.skipped), if g.skipped.len() == 1 { "is" } else { "are" });
    if g.add.is_empty() {
        return Err(if g.skipped.is_empty() {
            "No part there — sweep across parts, draw a box round whole parts, or drag a box to the left over them.".into()
        } else {
            format!("{already} — undo a pick to change it.")
        });
    }
    let n = list.iter().filter_map(|m| m.whole).max().unwrap_or(0) + 1;
    for &i in &g.add {
        let pl = &placed[i];
        let outline = parts[pl.part].geometry().holes.len();
        list.push(ManualCut { placed: i, contour: outline, start: [pl.x, pl.y], reverse: false, whole: Some(n) });
    }
    let what = if g.add.len() == 1 { "its own cut order" } else { "each in its own cut order" };
    let skipped = if g.skipped.is_empty() { String::new() } else { format!(" ({already})") };
    Ok(format!("Manual cut: {} taken whole, {what}{skipped}.", numbers(&g.add)))
}

/// Undo pick (MM9): take back the sheet's last command, one contour picked
/// by a click or every part of the last sweep or box. Returns the status
/// line, or `None` when nothing is picked.
pub fn undo(list: &mut Vec<ManualCut>) -> Option<String> {
    let last = list.pop()?;
    let Some(n) = last.whole else { return Some("Last manual pick removed.".into()) };
    let mut back = vec![last.placed];
    while list.last().is_some_and(|m| m.whole == Some(n)) {
        back.push(list.pop().unwrap().placed);
    }
    back.reverse();
    Some(format!("Last sweep or box taken back: {} back to the automatic order.", numbers(&back)))
}

/// Where the sheet's order badges go, M1, M2… (MM8): the picks in the order
/// the plan cuts them, by place in `plan.contours`. A contour picked by a
/// click is badged at its start, with its direction (`true`); a part taken
/// whole at the first of its contours the plan cuts that no click picked.
pub fn badges(plan: &cnc::SheetPlan, manual: &[ManualCut]) -> Vec<(usize, bool)> {
    let whole: Vec<usize> = manual.iter().filter(|m| m.whole.is_some()).map(|m| m.placed + 1).collect();
    let mut seen = vec![];
    let mut out = vec![];
    for (k, c) in plan.contours.iter().enumerate() {
        if c.manual {
            out.push((k, true));
        } else if whole.contains(&c.part_no) && !seen.contains(&c.part_no) {
            seen.push(c.part_no);
            out.push((k, false));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::Shape;

    fn part(shape: Shape, w: f64, h: f64) -> Part {
        Part { name: "DEMO".into(), shape, w, h, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    fn at(part: usize, x: f64, y: f64, w: f64, h: f64) -> Placed {
        Placed { part, x, y, w, h, turns: 0, mirror: false, spin: Default::default() }
    }

    fn spec() -> SheetSpec {
        SheetSpec { w: 1000.0, h: 800.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() }
    }

    /// Three 100 × 50 plates in a row, #1 at the right, #2 at the left, #3
    /// in the middle; and a 300 frame with a Ø120 hole, #4, with a 50
    /// square in the hole, #5.
    fn sheet() -> (Vec<Part>, Vec<Placed>) {
        let parts = vec![part(Shape::Rectangle, 100.0, 50.0), part(Shape::PlateHole, 300.0, 300.0), part(Shape::Rectangle, 50.0, 50.0)];
        let placed = vec![at(0, 400.0, 10.0, 100.0, 50.0), at(0, 10.0, 10.0, 100.0, 50.0), at(0, 200.0, 10.0, 100.0, 50.0), at(1, 10.0, 200.0, 300.0, 300.0), at(2, 135.0, 325.0, 50.0, 50.0)];
        (parts, placed)
    }

    const NONE: [bool; 5] = [false; 5];

    #[test]
    fn a_sweep_takes_parts_in_the_order_it_passes_them() {
        let (parts, placed) = sheet();
        // Right to left along the plates: #1, #3, #2.
        let g = sweep(&parts, &placed, &NONE, &[[500.0, 35.0], [5.0, 35.0]], 2.0);
        assert_eq!(g, Gathered { add: vec![0, 2, 1], skipped: vec![] });
        // Left to right, back over #2 and on to #1: each once, as first passed.
        let g = sweep(&parts, &placed, &NONE, &[[10.0, 35.0], [300.0, 35.0], [50.0, 35.0], [450.0, 40.0]], 2.0);
        assert_eq!(g.add, vec![1, 2, 0]);
    }

    #[test]
    fn a_sweep_over_open_sheet_takes_nothing() {
        let (parts, placed) = sheet();
        let g = sweep(&parts, &placed, &NONE, &[[600.0, 100.0], [900.0, 700.0]], 2.0);
        assert_eq!(g, Gathered::default());
        let mut list = vec![];
        assert!(take(&parts, &placed, &mut list, &g).unwrap_err().starts_with("No part there"));
        assert!(list.is_empty());
    }

    #[test]
    fn a_sweep_across_a_frame_and_the_part_in_its_hole_takes_both_and_the_inner_is_cut_first() {
        let (parts, placed) = sheet();
        // Up through the frame's left side, its hole and the square in it.
        let g = sweep(&parts, &placed, &NONE, &[[160.0, 190.0], [160.0, 420.0]], 2.0);
        assert_eq!(g.add, vec![3, 4], "as passed: the frame, then the square");
        let mut list = vec![];
        take(&parts, &placed, &mut list, &g).unwrap();
        // CO2: the square sits in the frame's hole, so it is cut before it.
        let plan = cnc::plan_manual(&parts, &placed, &spec(), &list);
        let first: Vec<usize> = plan.contours.iter().map(|c| c.part_no).take(3).collect();
        assert_eq!(first, vec![5, 4, 4], "the square, the frame's hole, its outline");
    }

    #[test]
    fn parts_the_picks_already_finish_are_skipped() {
        let (parts, placed) = sheet();
        // #3's outline clicked: cut whole already. #4's hole clicked: the
        // square in it is cut whole first, the frame's outline isn't cut yet.
        let click = |placed, contour| ManualCut { placed, contour, start: [0.0, 0.0], reverse: false, whole: None };
        let list = vec![click(2, 0), click(3, 0)];
        let done = picked(&parts, &placed, &list);
        assert_eq!(done, vec![false, false, true, false, true]);
        let g = sweep(&parts, &placed, &done, &[[500.0, 35.0], [5.0, 35.0]], 2.0);
        assert_eq!(g, Gathered { add: vec![0, 1], skipped: vec![2] });
        // The frame is taken: what is left of it, its outline.
        let g = sweep(&parts, &placed, &done, &[[160.0, 190.0], [160.0, 420.0]], 2.0);
        assert_eq!(g, Gathered { add: vec![3], skipped: vec![4] });
        // Nothing new: refused, saying which are picked already.
        let mut l = list.clone();
        let e = take(&parts, &placed, &mut l, &listed(&done, &[2, 4])).unwrap_err();
        assert_eq!(e, "#3 and #5 are already picked — undo a pick to change it.");
        assert_eq!(l, list);
    }

    #[test]
    fn a_box_takes_the_whole_parts_inside_in_the_automatic_order() {
        let (parts, placed) = sheet();
        let right = Window::dragged;
        // Round the three plates: number order would be #1, #2, #3; the
        // automatic order goes left to right.
        let g = boxed(&parts, &placed, &spec(), &NONE, &right([0.0, 0.0], [520.0, 100.0]));
        assert_eq!(g.add, vec![1, 2, 0]);
        // Half of #1 inside: only whole parts count.
        let g = boxed(&parts, &placed, &spec(), &NONE, &right([0.0, 0.0], [450.0, 100.0]));
        assert_eq!(g.add, vec![1, 2]);
        // Round the frame: the square in its hole goes first (CO1).
        let g = boxed(&parts, &placed, &spec(), &NONE, &right([0.0, 150.0], [350.0, 550.0]));
        assert_eq!(g.add, vec![4, 3]);
    }

    #[test]
    fn a_box_dragged_to_the_left_takes_the_parts_it_touches() {
        let (parts, placed) = sheet();
        // The box half over #1, dragged the other way: #1 too.
        let g = boxed(&parts, &placed, &spec(), &NONE, &Window::dragged([450.0, 100.0], [0.0, 0.0]));
        assert_eq!(g.add, vec![1, 2, 0]);
        // A small box over the frame's rim and into its hole, short of the
        // square in the hole: the frame alone.
        let g = boxed(&parts, &placed, &spec(), &NONE, &Window::dragged([120.0, 360.0], [60.0, 340.0]));
        assert_eq!(g.add, vec![3]);
    }

    #[test]
    fn an_agents_list_is_taken_as_named_each_once() {
        assert_eq!(listed(&NONE, &[3, 0, 3, 1]).add, vec![3, 0, 1]);
    }

    #[test]
    fn undo_takes_back_a_click_or_a_whole_sweep() {
        let (parts, placed) = sheet();
        let mut list = vec![];
        crate::engine::manual_pick(&parts, &placed, &mut list, [400.0, 30.0], 1.0, false).unwrap();
        take(&parts, &placed, &mut list, &listed(&NONE, &[1, 2])).unwrap();
        take(&parts, &placed, &mut list, &listed(&NONE, &[3])).unwrap();
        crate::engine::manual_pick(&parts, &placed, &mut list, [310.0, 430.0], 1.0, false).unwrap();
        let wholes: Vec<Option<u32>> = list.iter().map(|m| m.whole).collect();
        assert_eq!(wholes, vec![None, Some(1), Some(1), Some(2), None]);
        assert_eq!(undo(&mut list).unwrap(), "Last manual pick removed.");
        assert_eq!(undo(&mut list).unwrap(), "Last sweep or box taken back: #4 back to the automatic order.");
        assert_eq!(undo(&mut list).unwrap(), "Last sweep or box taken back: #2 and #3 back to the automatic order.");
        assert_eq!(list.len(), 1);
        assert_eq!(undo(&mut list).unwrap(), "Last manual pick removed.");
        assert_eq!(undo(&mut list), None);
    }

    #[test]
    fn badges_number_the_picks_in_the_order_they_are_cut() {
        let (parts, placed) = sheet();
        let mut list = vec![];
        // A click on #2's outline, then a sweep over #4 (the frame) and #1.
        crate::engine::manual_pick(&parts, &placed, &mut list, [10.0, 30.0], 1.0, false).unwrap();
        take(&parts, &placed, &mut list, &listed(&NONE, &[3, 0])).unwrap();
        let plan = cnc::plan_manual(&parts, &placed, &spec(), &list);
        let b: Vec<(usize, bool)> = badges(&plan, &list).into_iter().map(|(k, arrow)| (plan.contours[k].part_no, arrow)).collect();
        // #2 with its arrow; the frame at its first cut (after the square in
        // its hole, which wasn't picked); #1.
        assert_eq!(b, vec![(2, true), (4, false), (1, false)]);
        let k = badges(&plan, &list)[1].0;
        assert!(plan.contours[k].inner && plan.contours[..k].iter().any(|c| c.part_no == 5));
    }
}
