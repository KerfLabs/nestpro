//! Selection windows (MECHANICS SW1–SW5): a box dragged to the right takes
//! only what lies wholly inside it; a box dragged to the left takes
//! everything it touches, as CAD programs do. Every box in the window goes
//! through `Window`: parts on the sheet (HM7), lines and texts in the Part
//! view (E2, T14), the cut technology panel (CS6), picking the cut order
//! (CO12) and Manual cutting (MM9); so does the agents' `cut-order box=`.

use crate::nest::{self, Part, Placed, P};

/// What a selection window takes (SW1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Take {
    /// Only what lies wholly inside it: a box dragged to the right.
    Inside,
    /// Everything it touches or covers: a box dragged to the left.
    Touching,
}

impl Take {
    /// Which a drag makes, from where it started to where it is now, along X
    /// (SW1). X runs to the right on every view, so screen and drawing
    /// coordinates say the same; a drag straight up or down is a window.
    pub fn of_drag(from_x: f64, to_x: f64) -> Take {
        if to_x < from_x {
            Take::Touching
        } else {
            Take::Inside
        }
    }

    /// "wholly inside" or "touched by": for status lines.
    pub fn words(self) -> &'static str {
        match self {
            Take::Inside => "wholly inside",
            Take::Touching => "touched by",
        }
    }
}

/// A selection window on a drawing or a sheet (mm), and what it takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    lo: P,
    hi: P,
    pub take: Take,
}

/// A point this close to the window's edge counts as inside it, mm.
const EPS: f64 = 1e-6;

impl Window {
    /// The window with opposite corners `a` and `b`, any order, taking `take`.
    pub fn new(a: P, b: P, take: Take) -> Window {
        Window { lo: [a[0].min(b[0]), a[1].min(b[1])], hi: [a[0].max(b[0]), a[1].max(b[1])], take }
    }

    /// The window a drag draws from `from` (where the button went down) to
    /// `to`: dragged to the right it takes what lies wholly inside, to the
    /// left what it touches (SW1).
    pub fn dragged(from: P, to: P) -> Window {
        Window::new(from, to, Take::of_drag(from[0], to[0]))
    }

    /// Point `p` lies in the window or on its edge.
    pub fn holds(&self, p: P) -> bool {
        p[0] >= self.lo[0] - EPS && p[0] <= self.hi[0] + EPS && p[1] >= self.lo[1] - EPS && p[1] <= self.hi[1] + EPS
    }

    /// The box `[x0, y0, x1, y1]` overlaps the window (edges touching count).
    fn meets_box(&self, b: [f64; 4]) -> bool {
        b[0] <= self.hi[0] + EPS && b[2] >= self.lo[0] - EPS && b[1] <= self.hi[1] + EPS && b[3] >= self.lo[1] - EPS
    }

    /// The segment `a`–`b` has a point in the window: it is clipped to the
    /// window's slabs in X and in Y, and something of it is left.
    fn meets_segment(&self, a: P, b: P) -> bool {
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for k in 0..2 {
            let d = b[k] - a[k];
            let (lo, hi) = (self.lo[k] - EPS, self.hi[k] + EPS);
            if d.abs() < 1e-15 {
                if a[k] < lo || a[k] > hi {
                    return false;
                }
                continue;
            }
            let (u, v) = ((lo - a[k]) / d, (hi - a[k]) / d);
            t0 = t0.max(u.min(v));
            t1 = t1.min(u.max(v));
            if t0 > t1 {
                return false;
            }
        }
        true
    }

    /// A line through the points `path` (an element, a contour as drawn;
    /// SW2): a window takes it when every point lies inside; a crossing
    /// window when any stretch of it passes through or touches the box.
    pub fn takes_line(&self, path: &[P]) -> bool {
        match (self.take, path) {
            (_, []) => false,
            (Take::Inside, _) => path.iter().all(|&p| self.holds(p)),
            (Take::Touching, [p]) => self.holds(*p),
            (Take::Touching, _) => path.windows(2).any(|w| self.meets_segment(w[0], w[1])),
        }
    }

    /// A filled shape: the closed ring `outline` with `holes` (SW3): a window
    /// takes it when the whole outline lies inside; a crossing window when
    /// any of its edges, the holes' too, passes through or touches the box,
    /// or the box lies on its material (not wholly in a hole).
    pub fn takes_area(&self, outline: &[P], holes: &[Vec<P>]) -> bool {
        if outline.is_empty() {
            return false;
        }
        match self.take {
            Take::Inside => outline.iter().all(|&p| self.holds(p)),
            Take::Touching => {
                let edges = |r: &[P]| r.iter().zip(r.iter().cycle().skip(1)).any(|(&a, &b)| self.meets_segment(a, b));
                edges(outline) || holes.iter().any(|h| edges(h)) || (nest::point_in_poly(outline, self.lo) && !holes.iter().any(|h| nest::point_in_poly(h, self.lo)))
            }
        }
    }

    /// A placed part on the sheet (SW3): its outline wholly inside, or (a
    /// crossing window) its outline, a hole's edge or its material touched.
    pub fn takes_placed(&self, parts: &[Part], pl: &Placed) -> bool {
        if !self.meets_box(pl.bbox4()) {
            return false;
        }
        let (outline, holes) = nest::placed_shape(parts, pl);
        self.takes_area(&outline, &holes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::Shape;

    /// A 100 × 100 square with a Ø40 hole in its middle, as rings.
    fn plate() -> (Vec<P>, Vec<Vec<P>>) {
        let outline = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let hole: Vec<P> = (0..36).map(|k| (k as f64 * 10.0f64).to_radians()).map(|a| [50.0 + 20.0 * a.cos(), 50.0 + 20.0 * a.sin()]).collect();
        (outline, vec![hole])
    }

    #[test]
    fn the_drag_direction_says_what_the_window_takes() {
        assert_eq!(Window::dragged([0.0, 0.0], [10.0, 10.0]).take, Take::Inside, "to the right");
        assert_eq!(Window::dragged([10.0, 0.0], [10.0, 50.0]).take, Take::Inside, "straight up");
        assert_eq!(Window::dragged([10.0, 10.0], [0.0, 0.0]).take, Take::Touching, "to the left, down");
        assert_eq!(Window::dragged([10.0, 0.0], [0.0, 10.0]).take, Take::Touching, "to the left, up");
        // Either way round it is the same box.
        assert!(Window::dragged([10.0, 10.0], [0.0, 0.0]).holds([5.0, 5.0]));
    }

    #[test]
    fn a_window_takes_a_line_wholly_inside_and_a_crossing_one_any_line_it_touches() {
        let line = [[0.0, 0.0], [100.0, 0.0]];
        let half = |take| Window::new([50.0, -10.0], [150.0, 10.0], take);
        assert!(!half(Take::Inside).takes_line(&line), "half outside");
        assert!(half(Take::Touching).takes_line(&line));
        // A window that holds neither end but crosses the middle.
        let across = |take| Window::new([40.0, -5.0], [60.0, 5.0], take);
        assert!(!across(Take::Inside).takes_line(&line));
        assert!(across(Take::Touching).takes_line(&line));
        // Just touching an end counts; missing it doesn't.
        assert!(Window::new([100.0, -5.0], [120.0, 5.0], Take::Touching).takes_line(&line));
        assert!(!Window::new([100.1, -5.0], [120.0, 5.0], Take::Touching).takes_line(&line));
        assert!(!Window::new([0.0, 1.0], [100.0, 5.0], Take::Touching).takes_line(&line), "beside it");
        // A diagonal passing the corner of the window, but not into it.
        assert!(!Window::new([0.0, 6.0], [4.0, 10.0], Take::Touching).takes_line(&[[0.0, 0.0], [10.0, 5.0]]));
        // Wholly inside: both take it.
        let all = |take| Window::new([-1.0, -1.0], [101.0, 1.0], take);
        assert!(all(Take::Inside).takes_line(&line) && all(Take::Touching).takes_line(&line));
    }

    #[test]
    fn a_crossing_window_takes_a_part_it_touches_or_lies_on_but_not_from_inside_a_hole() {
        let (outline, holes) = plate();
        let w = |a, b| Window::new(a, b, Take::Touching);
        assert!(w([90.0, 40.0], [120.0, 60.0]).takes_area(&outline, &holes), "over the right edge");
        assert!(w([10.0, 10.0], [20.0, 20.0]).takes_area(&outline, &holes), "on the material, touching no line");
        assert!(!w([45.0, 45.0], [55.0, 55.0]).takes_area(&outline, &holes), "inside the hole");
        assert!(w([25.0, 45.0], [35.0, 55.0]).takes_area(&outline, &holes), "over the hole's edge");
        assert!(!w([110.0, 0.0], [120.0, 100.0]).takes_area(&outline, &holes), "beside it");
        // The window needs it all.
        assert!(!Window::new([90.0, 40.0], [120.0, 60.0], Take::Inside).takes_area(&outline, &holes));
        assert!(Window::new([-1.0, -1.0], [101.0, 101.0], Take::Inside).takes_area(&outline, &holes));
    }

    #[test]
    fn a_placed_part_is_taken_by_its_real_shape_not_its_box() {
        // A 300 frame with a Ø120 hole at (10, 200): a crossing window in the
        // hole, though inside the frame's box, takes nothing; on its rim it does.
        let part = Part { name: "DEMO".into(), shape: Shape::PlateHole, w: 300.0, h: 300.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let pl = Placed { part: 0, x: 10.0, y: 200.0, w: 300.0, h: 300.0, turns: 0, mirror: false };
        let parts = [part];
        assert!(!Window::new([150.0, 340.0], [170.0, 360.0], Take::Touching).takes_placed(&parts, &pl), "in the hole");
        assert!(Window::new([20.0, 210.0], [30.0, 220.0], Take::Touching).takes_placed(&parts, &pl), "on the rim");
        assert!(!Window::new([400.0, 210.0], [500.0, 220.0], Take::Touching).takes_placed(&parts, &pl), "off the part");
        assert!(!Window::new([0.0, 190.0], [200.0, 600.0], Take::Inside).takes_placed(&parts, &pl));
        assert!(Window::new([0.0, 190.0], [320.0, 600.0], Take::Inside).takes_placed(&parts, &pl));
    }
}
