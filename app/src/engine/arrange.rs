//! Moving parts by hand on a nested sheet: nudging by a step, sliding to the
//! next part, dragging with snap and slide, turning and mirroring a selection
//! about its common centre, copying a selection and window selection. Rules
//! HM1–HM12 in MECHANICS.md "Moving parts by hand".
//!
//! Every function takes the sheet as it is and the indices of the parts that
//! move together (the selection); the other parts and the sheet edge are what
//! stops them. The GUI and the agent operations both call these.

use super::part_fits_format;
use super::selection::Window;
use crate::nest::{self, Part, Placed, SheetFormat, SheetSpec, P};

/// A direction on the sheet (X right, Y up).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Down,
    Up,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Left, Dir::Right, Dir::Down, Dir::Up];
    /// The unit vector.
    pub fn u(self) -> P {
        match self {
            Dir::Left => [-1.0, 0.0],
            Dir::Right => [1.0, 0.0],
            Dir::Down => [0.0, -1.0],
            Dir::Up => [0.0, 1.0],
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Dir::Left => "left",
            Dir::Right => "right",
            Dir::Down => "down",
            Dir::Up => "up",
        }
    }
    pub fn arrow(self) -> &'static str {
        match self {
            Dir::Left => "←",
            Dir::Right => "→",
            Dir::Down => "↓",
            Dir::Up => "↑",
        }
    }
    pub fn parse(s: &str) -> Option<Dir> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" | "l" | "-x" => Some(Dir::Left),
            "right" | "r" | "+x" | "x" => Some(Dir::Right),
            "down" | "d" | "-y" => Some(Dir::Down),
            "up" | "u" | "+y" | "y" => Some(Dir::Up),
            _ => None,
        }
    }
}

/// Bisection steps when backing off to the real clearance.
pub const BACKOFF_STEPS: usize = 20;
/// Moves shorter than this (mm) count as no move.
pub const EPS: f64 = 1e-4;

fn shift(p: Placed, d: P) -> Placed {
    Placed { x: p.x + d[0], y: p.y + d[1], ..p }
}

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(u: P, t: f64) -> P {
    [u[0] * t, u[1] * t]
}

fn len(a: P) -> f64 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}

/// The selection as sorted, unique indices that exist on the sheet.
pub fn clean(sheet: &[Placed], sel: &[usize]) -> Vec<usize> {
    let mut v: Vec<usize> = sel.iter().copied().filter(|&i| i < sheet.len()).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The box around the selected parts as [x0, y0, x1, y1].
pub fn group_box(sheet: &[Placed], sel: &[usize]) -> [f64; 4] {
    sel.iter().filter_map(|&i| sheet.get(i)).fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| {
        [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x + p.w), b[3].max(p.y + p.h)]
    })
}

/// Window selection (HM7, SW1, SW3): the parts lying wholly inside a window
/// dragged to the right, or every part a window dragged to the left touches.
pub fn window_select(parts: &[Part], sheet: &[Placed], w: &Window) -> Vec<usize> {
    sheet.iter().enumerate().filter(|(_, p)| w.takes_placed(parts, p)).map(|(i, _)| i).collect()
}

/// Can the selection sit moved by `d`: every moved part inside the sheet (its
/// margin, a remnant's real shape) and, with collision on, at least the part
/// gap from every part that isn't moving. The moving parts keep their places
/// among themselves, so they aren't tested against each other.
#[allow(clippy::too_many_arguments)]
pub fn group_ok(parts: &[Part], sheet: &[Placed], sel: &[usize], d: P, spec: &SheetSpec, f: &SheetFormat, collision: bool) -> bool {
    ok_except(parts, sheet, sel, d, spec, f, collision, &[])
}

/// `group_ok`, leaving the parts in `skip` out of the collision test.
#[allow(clippy::too_many_arguments)]
fn ok_except(parts: &[Part], sheet: &[Placed], sel: &[usize], d: P, spec: &SheetSpec, f: &SheetFormat, collision: bool, skip: &[usize]) -> bool {
    let gap = spec.gap - 1e-6;
    sel.iter().all(|&i| {
        let c = shift(sheet[i], d);
        part_fits_format(parts, spec, f, &c) && (!collision || sheet.iter().enumerate().all(|(j, o)| sel.contains(&j) || skip.contains(&j) || !nest::clash_exact(parts, &c, o, gap)))
    })
}

/// Outline and holes of a placed part, as closed rings.
fn rings(parts: &[Part], p: &Placed) -> Vec<Vec<P>> {
    let (o, holes) = nest::placed_shape(parts, p);
    std::iter::once(o).chain(holes).filter(|r| r.len() >= 2).collect()
}

/// How far the selection, moved by `d0`, can go further along `dir` (HM2): up
/// to `limit`, stopping at the sheet margin and, with collision on, one part
/// gap before the next part's contour (holes count both ways, so a part in
/// another part's hole stops at the hole's edge). Slanted edges need a little
/// more room than the gap along the move: the move backs off until the real
/// clearance holds. 0 when the selection can't sit at `d0` at all.
#[allow(clippy::too_many_arguments)]
pub fn room(parts: &[Part], sheet: &[Placed], sel: &[usize], d0: P, dir: Dir, spec: &SheetSpec, f: &SheetFormat, limit: f64, collision: bool) -> f64 {
    let spec = spec.for_format(f);
    let u = dir.u();
    let gap = spec.gap;
    let movers: Vec<Placed> = sel.iter().map(|&i| shift(sheet[i], d0)).collect();
    // Parts the selection already sits too close to (after a forced move)
    // don't hold it back when it moves away from them.
    let mut behind: Vec<usize> = vec![];
    if collision && !group_ok(parts, sheet, sel, d0, &spec, f, collision) {
        for (j, o) in sheet.iter().enumerate() {
            if sel.contains(&j) || !movers.iter().any(|c| nest::clash_exact(parts, c, o, gap - 1e-6)) {
                continue;
            }
            let theirs = rings(parts, o);
            let ahead = movers.iter().any(|c| rings(parts, c).iter().any(|a| theirs.iter().any(|b| nest::contact_along(a, b, u).is_some())));
            if ahead {
                return 0.0;
            }
            behind.push(j);
        }
    }
    if sel.is_empty() || !ok_except(parts, sheet, sel, d0, &spec, f, collision, &behind) {
        return 0.0;
    }
    // The sheet margin (a part's box is its contour's extent).
    let mut t = limit;
    for c in &movers {
        t = t.min(match dir {
            Dir::Left => c.x - spec.margin,
            Dir::Right => spec.w - spec.margin - (c.x + c.w),
            Dir::Down => c.y - spec.margin,
            Dir::Up => spec.h - spec.margin - (c.y + c.h),
        });
    }
    if t <= EPS {
        return 0.0;
    }
    if collision {
        let horizontal = u[1] == 0.0;
        for c in &movers {
            let mine = rings(parts, c);
            for (j, o) in sheet.iter().enumerate() {
                if sel.contains(&j) || behind.contains(&j) {
                    continue;
                }
                // Only parts in the band the move sweeps, and not wholly behind it.
                let in_band = if horizontal { o.y < c.y + c.h + gap && c.y < o.y + o.h + gap } else { o.x < c.x + c.w + gap && c.x < o.x + o.w + gap };
                let behind = match dir {
                    Dir::Left => o.x > c.x + c.w + gap,
                    Dir::Right => o.x + o.w < c.x - gap,
                    Dir::Down => o.y > c.y + c.h + gap,
                    Dir::Up => o.y + o.h < c.y - gap,
                };
                if !in_band || behind {
                    continue;
                }
                let theirs = rings(parts, o);
                for a in &mine {
                    for b in &theirs {
                        if let Some(k) = nest::contact_along(a, b, u) {
                            t = t.min((k - gap).max(0.0));
                        }
                    }
                }
            }
        }
    }
    let ok = |t: f64| ok_except(parts, sheet, sel, add(d0, scale(u, t)), &spec, f, collision, &behind);
    if t <= EPS {
        return 0.0;
    }
    if ok(t) {
        return t;
    }
    let (mut lo, mut hi) = (0.0, t);
    for _ in 0..BACKOFF_STEPS {
        let mid = (lo + hi) / 2.0;
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Move the selection along `dir` by at most `limit` (`f64::INFINITY` = to the
/// end, HM2; the movement value = a nudge, HM3). Returns how far it went.
#[allow(clippy::too_many_arguments)]
pub fn slide(parts: &[Part], sheet: &mut [Placed], sel: &[usize], dir: Dir, spec: &SheetSpec, f: &SheetFormat, limit: f64, collision: bool) -> f64 {
    let sel = clean(sheet, sel);
    let t = room(parts, sheet, &sel, [0.0, 0.0], dir, spec, f, limit, collision);
    if t > EPS {
        let d = scale(dir.u(), t);
        for &i in &sel {
            sheet[i] = shift(sheet[i], d);
        }
        t
    } else {
        0.0
    }
}

/// The parts (not selected) that the selection now sits closer to than the
/// part gap — after a forced move by the movement value (HM3).
pub fn too_close(parts: &[Part], sheet: &[Placed], sel: &[usize], spec: &SheetSpec) -> Vec<usize> {
    let gap = spec.gap - 1e-6;
    (0..sheet.len()).filter(|j| !sel.contains(j) && sel.iter().any(|&i| nest::clash_exact(parts, &sheet[i], &sheet[*j], gap))).collect()
}

/// How much further the selection could move in each direction before it
/// meets the next part at the gap or the margin (shown after a nudge), capped at `cap`.
pub fn rooms(parts: &[Part], sheet: &[Placed], sel: &[usize], spec: &SheetSpec, f: &SheetFormat, cap: f64, collision: bool) -> [f64; 4] {
    Dir::ALL.map(|d| room(parts, sheet, sel, [0.0, 0.0], d, spec, f, cap, collision))
}

/// Snap while dragging (HM9): from a valid offset `d`, when the selection is
/// within `reach` of a part (at the gap, on the real contours) or the sheet
/// margin, it moves in to sit exactly there — on each axis toward the nearer
/// one. Returns the snapped offset.
#[allow(clippy::too_many_arguments)]
pub fn snap(parts: &[Part], sheet: &[Placed], sel: &[usize], d: P, spec: &SheetSpec, f: &SheetFormat, reach: f64, collision: bool) -> P {
    let mut d = d;
    for (neg, pos) in [(Dir::Left, Dir::Right), (Dir::Down, Dir::Up)] {
        let a = room(parts, sheet, sel, d, neg, spec, f, reach, collision);
        let b = room(parts, sheet, sel, d, pos, spec, f, reach, collision);
        let pick = [(a, neg), (b, pos)].into_iter().filter(|(t, _)| *t > EPS && *t < reach - EPS).min_by(|x, y| x.0.total_cmp(&y.0));
        if let Some((t, dir)) = pick {
            d = add(d, scale(dir.u(), t));
        }
    }
    d
}

/// Where a dragged selection goes (HM8, HM9). `sheet` holds the parts at
/// their places when the drag started; `want` is the offset the pointer asks
/// for and `last` the last offset that was valid (None when the selection
/// started where it can't stay, as a copy over its original does). When `want`
/// is blocked, the selection slides from `last` toward it — along X then Y or
/// Y then X, whichever ends nearer — and stops one gap before what is in the
/// way. Then it snaps (when `reach` > 0). Returns the offset and whether it is valid.
#[allow(clippy::too_many_arguments)]
pub fn drag(parts: &[Part], sheet: &[Placed], sel: &[usize], last: Option<P>, want: P, spec: &SheetSpec, f: &SheetFormat, collision: bool, reach: f64) -> (P, bool) {
    let spec = spec.for_format(f);
    let mut d = want;
    if !group_ok(parts, sheet, sel, want, &spec, f, collision) {
        let Some(last) = last else { return (want, false) };
        let step = |from: P, axis: usize| -> P {
            let delta = want[axis] - from[axis];
            if delta.abs() <= EPS {
                return from;
            }
            let dir = match (axis, delta > 0.0) {
                (0, false) => Dir::Left,
                (0, true) => Dir::Right,
                (_, false) => Dir::Down,
                _ => Dir::Up,
            };
            add(from, scale(dir.u(), room(parts, sheet, sel, from, dir, &spec, f, delta.abs(), collision)))
        };
        let xy = step(step(last, 0), 1);
        let yx = step(step(last, 1), 0);
        let dist = |p: P| len([p[0] - want[0], p[1] - want[1]]);
        d = if dist(xy) <= dist(yx) { xy } else { yx };
    }
    if reach > 0.0 {
        d = snap(parts, sheet, sel, d, &spec, f, reach, collision);
    }
    (d, true)
}

/// The nearest offset (within `max_r`) where the selection fits, searching
/// rings around where it is (HM5). Rings every `step`, 16 points per ring
/// plus the four axis points.
#[allow(clippy::too_many_arguments)]
pub fn nearest_free(parts: &[Part], sheet: &[Placed], sel: &[usize], spec: &SheetSpec, f: &SheetFormat, collision: bool, step: f64, max_r: f64) -> Option<P> {
    if group_ok(parts, sheet, sel, [0.0, 0.0], spec, f, collision) {
        return Some([0.0, 0.0]);
    }
    let step = step.max(0.5);
    let mut r = step;
    while r <= max_r + 1e-9 {
        let n = 16;
        let mut best: Option<(f64, P)> = None;
        for k in 0..n {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            let d = [r * a.cos(), r * a.sin()];
            if group_ok(parts, sheet, sel, d, spec, f, collision) && best.is_none_or(|b| len(d) < b.0) {
                best = Some((len(d), d));
            }
        }
        if let Some((_, d)) = best {
            return Some(d);
        }
        r += step;
    }
    None
}

/// Turn the selection a number of quarter turns (1 = 90° counter-clockwise,
/// 3 = 90° clockwise, 2 = 180°) about the centre of its box (HM5). See `turn_by`.
pub fn turn(parts: &[Part], sheet: &[Placed], sel: &[usize], quarters: u8, spec: &SheetSpec, f: &SheetFormat, collision: bool) -> Result<Vec<Placed>, String> {
    turn_by(parts, sheet, sel, (quarters % 4) as f64 * 90.0, spec, f, collision)
}

/// Turn the selection `deg` degrees (positive = counter-clockwise) about the
/// centre of its box (HM5, HM14). Every part must allow its new angle. With
/// collision on, a turned selection that lands on another part or off the
/// sheet moves to the nearest free spot. Returns the new places of the
/// selected parts (in `sel` order).
pub fn turn_by(parts: &[Part], sheet: &[Placed], sel: &[usize], deg: f64, spec: &SheetSpec, f: &SheetFormat, collision: bool) -> Result<Vec<Placed>, String> {
    let sel = clean(sheet, sel);
    if sel.is_empty() {
        return Err("Select the parts to turn first.".into());
    }
    if nest::split_angle(deg) == (0, 0.0) {
        return Ok(sel.iter().map(|&i| sheet[i]).collect());
    }
    if let Some(p) = refuses_turn(parts, sheet, &sel, deg) {
        let why = if nest::split_angle(deg).1 == 0.0 { "its rotations, Part Properties" } else { "only parts that may turn 90° take other angles — Part Properties" };
        return Err(format!("'{}' isn't allowed to turn {} ({why}).", p.name, nest::angle_text(signed(deg))));
    }
    let turned = turned_about(parts, sheet, &sel, deg);
    place_changed(parts, sheet, &sel, turned, spec, f, collision, "turn")
}

/// The selected parts turned `deg` about the centre of their box, where they
/// land before any search for room (`sel` clean). A drag of the turn handle
/// shows this while the button is held (HM15).
pub fn turned_about(parts: &[Part], sheet: &[Placed], sel: &[usize], deg: f64) -> Vec<Placed> {
    let b = group_box(sheet, sel);
    let c = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
    sel.iter().map(|&i| turn_about(&parts[sheet[i].part], sheet[i], c, deg)).collect()
}

/// An angle in (-180°, 180°], for messages.
fn signed(deg: f64) -> f64 {
    let a = deg.rem_euclid(360.0);
    if a > 180.0 { a - 360.0 } else { a }
}

/// One placed part turned `deg` counter-clockwise about the sheet point `c`:
/// its angle grows by `deg` and the middle of its own box goes round `c`.
pub fn turn_about(part: &Part, p: Placed, c: P, deg: f64) -> Placed {
    let (q, rest) = nest::split_angle(deg);
    if rest == 0.0 && p.spin.is_none() {
        // Quarter turns of a part on a quarter turn: its box turns exactly.
        let m = [p.x + p.w / 2.0 - c[0], p.y + p.h / 2.0 - c[1]];
        let r = match q {
            0 => m,
            1 => [-m[1], m[0]],
            2 => [-m[0], -m[1]],
            _ => [m[1], -m[0]],
        };
        let (w, h) = if q % 2 == 1 { (p.h, p.w) } else { (p.w, p.h) };
        return Placed { x: c[0] + r[0] - w / 2.0, y: c[1] + r[1] - h / 2.0, w, h, turns: (p.turns + q) % 4, ..p };
    }
    let pv = nest::placed_pivot(part, &p);
    let (s, co) = deg.to_radians().sin_cos();
    let m = [pv[0] - c[0], pv[1] - c[1]];
    let to = [c[0] + m[0] * co - m[1] * s, c[1] + m[0] * s + m[1] * co];
    let n = nest::oriented(part, p.part, p.angle() + deg, p.mirror);
    let at = nest::placed_pivot(part, &n);
    Placed { x: to[0] - at[0], y: to[1] - at[1], ..n }
}

/// Mirror the selection about the centre of its box (HM6): `vertical` = about
/// the vertical centre line (left↔right, the original's Symmetry X), otherwise
/// about the horizontal one (top↔bottom, Symmetry Y). Every part must allow
/// mirroring in its new turn.
pub fn mirror(parts: &[Part], sheet: &[Placed], sel: &[usize], vertical: bool, spec: &SheetSpec, f: &SheetFormat, collision: bool) -> Result<Vec<Placed>, String> {
    let sel = clean(sheet, sel);
    if sel.is_empty() {
        return Err("Select the parts to mirror first.".into());
    }
    let b = group_box(sheet, &sel);
    let mut out = vec![];
    for &i in &sel {
        let p = sheet[i];
        let part = &parts[p.part];
        let angle = mirror_angle(p, vertical);
        if !nest::angle_allowed(part, angle, !p.mirror) {
            return Err(format!("'{}' isn't allowed to be mirrored (its rotations, Part Properties).", part.name));
        }
        // The mirror image of the footprint is the footprint of the mirror image.
        let (x, y) = if vertical { (b[0] + b[2] - (p.x + p.w), p.y) } else { (p.x, b[1] + b[3] - (p.y + p.h)) };
        let n = if p.spin.is_none() { Placed { turns: nest::split_angle(angle).0, mirror: !p.mirror, ..p } } else { nest::oriented(part, p.part, angle, !p.mirror) };
        out.push(Placed { x, y, ..n });
    }
    place_changed(parts, sheet, &sel, out, spec, f, collision, "mirror")
}

/// The angle a part takes when mirrored: left↔right (`vertical`) is its own
/// X mirror with the angle reversed; top↔bottom adds a half turn.
fn mirror_angle(p: Placed, vertical: bool) -> f64 {
    let a = if vertical { -p.angle() } else { 180.0 - p.angle() };
    a.rem_euclid(360.0)
}

/// The first selected part whose rotations don't allow turning it by `deg`
/// (None = all may turn).
fn refuses_turn<'a>(parts: &'a [Part], sheet: &[Placed], sel: &[usize], deg: f64) -> Option<&'a Part> {
    clean(sheet, sel).into_iter().map(|i| (&parts[sheet[i].part], sheet[i])).find(|(p, s)| !nest::angle_allowed(p, s.angle() + deg, s.mirror)).map(|(p, _)| p)
}

/// The selection may turn by `quarters` (each part's allowed rotations; room
/// is found when it turns). The quick ring greys Turn when not (QR2).
pub fn can_turn(parts: &[Part], sheet: &[Placed], sel: &[usize], quarters: u8) -> bool {
    can_turn_by(parts, sheet, sel, (quarters % 4) as f64 * 90.0)
}

/// The selection may turn by `deg` (HM14).
pub fn can_turn_by(parts: &[Part], sheet: &[Placed], sel: &[usize], deg: f64) -> bool {
    !clean(sheet, sel).is_empty() && refuses_turn(parts, sheet, sel, deg).is_none()
}

/// The selection may be mirrored left↔right (`vertical`) or top↔bottom.
pub fn can_mirror(parts: &[Part], sheet: &[Placed], sel: &[usize], vertical: bool) -> bool {
    let sel = clean(sheet, sel);
    !sel.is_empty() && sel.iter().all(|&i| nest::angle_allowed(&parts[sheet[i].part], mirror_angle(sheet[i], vertical), !sheet[i].mirror))
}

/// Put the changed selection in, moving it to the nearest free spot when it
/// doesn't fit where it is.
#[allow(clippy::too_many_arguments)]
fn place_changed(parts: &[Part], sheet: &[Placed], sel: &[usize], new: Vec<Placed>, spec: &SheetSpec, f: &SheetFormat, collision: bool, what: &str) -> Result<Vec<Placed>, String> {
    let spec = spec.for_format(f);
    let mut trial = sheet.to_vec();
    for (k, &i) in sel.iter().enumerate() {
        trial[i] = new[k];
    }
    let b = group_box(&trial, sel);
    let reach = (b[2] - b[0]).max(b[3] - b[1]).max(spec.gap * 4.0);
    let d = nearest_free(parts, &trial, sel, &spec, f, collision, (spec.gap / 2.0).max(1.0), reach).ok_or_else(|| format!("No room to {what} the selection here."))?;
    Ok(new.into_iter().map(|p| shift(p, d)).collect())
}

/// Where a copy of the selection goes (HM10): the first free spot, left
/// first then lowest, by the parts' boxes, keeping the selection's layout.
/// Returns the offset from the originals.
pub fn copy_spot(sheet: &[Placed], sel: &[usize], spec: &SheetSpec, f: &SheetFormat) -> Option<P> {
    let sel = clean(sheet, sel);
    if sel.is_empty() {
        return None;
    }
    let spec = spec.for_format(f);
    let (gap, m) = (spec.gap, spec.margin);
    let b = group_box(sheet, &sel);
    let mut xs = vec![m];
    let mut ys = vec![m];
    for o in sheet {
        xs.push(o.x + o.w + gap);
        ys.push(o.y + o.h + gap);
    }
    let mut cands: Vec<P> = xs.iter().flat_map(|&x| ys.iter().map(move |&y| [x - b[0], y - b[1]])).collect();
    cands.sort_by(|a, c| (a[0], a[1]).partial_cmp(&(c[0], c[1])).unwrap_or(std::cmp::Ordering::Equal));
    cands.into_iter().find(|&d| {
        sel.iter().all(|&i| {
            let c = shift(sheet[i], d);
            super::fits_format(&spec, f, &c) && !sheet.iter().any(|o| c.clashes(o, gap - 1e-6))
        })
    })
}

/// Copies of the selected parts at offset `d` (they are appended to the sheet by the caller).
pub fn copies(sheet: &[Placed], sel: &[usize], d: P) -> Vec<Placed> {
    clean(sheet, sel).iter().map(|&i| shift(sheet[i], d)).collect()
}

/// The ring of points around a part at the part gap (the halo drawn around
/// moving parts, HM11): the outline grown by the gap, round at the corners.
pub fn gap_halo(parts: &[Part], p: &Placed, gap: f64) -> Vec<Vec<P>> {
    if gap <= 0.0 {
        return vec![];
    }
    let (o, _) = nest::placed_shape(parts, p);
    if o.len() < 3 {
        return vec![];
    }
    let ol = crate::stock::Outline { outer: o, holes: vec![] };
    crate::region::offset_round(&ol, -gap).into_iter().map(|g| g.outer).collect()
}

/// Parse a list of placed-part numbers ("1,3,5" or "2-4", from 1) into indices,
/// in number order, each once.
pub fn parse_items(s: &str, n: usize) -> Result<Vec<usize>, String> {
    let mut v = parse_items_in_order(s, n)?;
    v.sort_unstable();
    v.dedup();
    Ok(v)
}

/// Placed parts ("3,1,2", "4-2", numbers from 1) in the order written, a
/// range the way it runs (`4-2` is 4, 3, 2); numbers from 0.
pub fn parse_items_in_order(s: &str, n: usize) -> Result<Vec<usize>, String> {
    let mut v = vec![];
    for tok in s.split([',', ' ', ';']).map(str::trim).filter(|t| !t.is_empty()) {
        let tok = tok.trim_start_matches('#');
        let (a, b) = match tok.split_once('-') {
            Some((a, b)) => (a.trim().parse::<usize>(), b.trim().parse::<usize>()),
            None => (tok.parse::<usize>(), tok.parse::<usize>()),
        };
        let (Ok(a), Ok(b)) = (a, b) else { return Err(format!("'{tok}' isn't a part number (use e.g. 1,3,5 or 2-4).")) };
        if a == 0 || b == 0 || a > n || b > n {
            return Err(format!("There is no part #{} on this sheet (it has {n}).", if a == 0 || a > n { a } else { b }));
        }
        if a <= b {
            v.extend((a..=b).map(|k| k - 1));
        } else {
            v.extend((b..=a).rev().map(|k| k - 1));
        }
    }
    if v.is_empty() {
        return Err("Name the parts, e.g. items=1,3,5.".into());
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::Shape;

    const GAP: f64 = 6.35;

    fn spec() -> SheetSpec {
        SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: GAP, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default() }
    }
    fn fmt() -> SheetFormat {
        SheetFormat::rect(1000.0, 500.0)
    }
    fn part(shape: Shape, w: f64, h: f64) -> Part {
        Part { name: "P".into(), shape, w, h, qty: 9, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }
    /// Part 0 = 100 × 40 rectangle, part 1 = 200 × 200 gusset (triangle),
    /// part 2 = 400 × 400 plate with a hole (r 80), part 3 = 20 × 20 square.
    fn parts() -> Vec<Part> {
        vec![part(Shape::Rectangle, 100.0, 40.0), part(Shape::Triangle, 200.0, 200.0), part(Shape::PlateHole, 400.0, 400.0), part(Shape::Rectangle, 20.0, 20.0)]
    }
    fn at(part: usize, x: f64, y: f64, parts: &[Part]) -> Placed {
        let (w, h) = parts[part].bbox();
        Placed { part, x, y, w, h, turns: 0, mirror: false, spin: Default::default() }
    }
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn ctrl_arrow_slides_to_one_part_gap_from_the_next_part() {
        let ps = parts();
        let mut sheet = vec![at(0, 50.0, 100.0, &ps), at(0, 600.0, 110.0, &ps)];
        let t = slide(&ps, &mut sheet, &[1], Dir::Left, &spec(), &fmt(), f64::INFINITY, true);
        assert!(close(sheet[1].x, 150.0 + GAP), "{:?}", sheet[1]);
        assert!(close(t, 600.0 - 150.0 - GAP));
        // Again: it already sits at the gap.
        assert_eq!(slide(&ps, &mut sheet, &[1], Dir::Left, &spec(), &fmt(), f64::INFINITY, true), 0.0);
        // Nothing above: up to the top margin.
        slide(&ps, &mut sheet, &[1], Dir::Up, &spec(), &fmt(), f64::INFINITY, true);
        assert!(close(sheet[1].y + sheet[1].h, 490.0));
    }

    #[test]
    fn alt_arrow_moves_by_the_step_and_never_into_a_part() {
        let ps = parts();
        let mut sheet = vec![at(0, 50.0, 100.0, &ps), at(0, 160.0, 100.0, &ps)];
        // 10 mm apart, gap 6.35: a 1 mm step goes, a 5 mm step stops at the gap.
        assert!(close(slide(&ps, &mut sheet, &[1], Dir::Left, &spec(), &fmt(), 1.0, true), 1.0));
        assert!(close(sheet[1].x, 159.0));
        let t = slide(&ps, &mut sheet, &[1], Dir::Left, &spec(), &fmt(), 5.0, true);
        assert!(close(t, 9.0 - GAP) && close(sheet[1].x, 150.0 + GAP), "{t} {:?}", sheet[1]);
        // Right: free.
        assert!(close(slide(&ps, &mut sheet, &[1], Dir::Right, &spec(), &fmt(), 25.4, true), 25.4));
    }

    #[test]
    fn with_collision_off_only_the_margin_stops_a_slide() {
        let ps = parts();
        let mut sheet = vec![at(0, 50.0, 100.0, &ps), at(0, 600.0, 100.0, &ps)];
        slide(&ps, &mut sheet, &[1], Dir::Left, &spec(), &fmt(), f64::INFINITY, false);
        assert!(close(sheet[1].x, 10.0));
    }

    #[test]
    fn a_selection_slides_together_until_its_leading_part_meets_one() {
        let ps = parts();
        // Two selected parts stacked; the lower one meets the fixed part first.
        let mut sheet = vec![at(0, 50.0, 100.0, &ps), at(0, 500.0, 100.0, &ps), at(0, 400.0, 200.0, &ps)];
        slide(&ps, &mut sheet, &[1, 2], Dir::Left, &spec(), &fmt(), f64::INFINITY, true);
        assert!(close(sheet[1].x, 150.0 + GAP), "{:?}", sheet[1]);
        assert!(close(sheet[2].x, 50.0 + GAP), "the group keeps its layout: {:?}", sheet[2]);
    }

    #[test]
    fn a_slide_follows_the_real_contour_into_a_slanted_side() {
        let ps = parts();
        // Gusset with its slanted side facing up-right; a square slides left
        // over the gusset's box and stops at the gap from the slant.
        let mut sheet = vec![at(1, 10.0, 10.0, &ps), at(3, 600.0, 150.0, &ps)];
        slide(&ps, &mut sheet, &[1], Dir::Left, &spec(), &fmt(), f64::INFINITY, true);
        let sq = sheet[1];
        assert!(sq.x < 210.0, "tucked inside the gusset's box: {sq:?}");
        assert!(!nest::clash_exact(&ps, &sheet[0], &sq, GAP - 1e-6), "keeps the gap");
        assert!(nest::clash_exact(&ps, &sheet[0], &Placed { x: sq.x - 0.1, ..sq }, GAP - 1e-6), "and is at it");
    }

    #[test]
    fn a_part_in_another_parts_hole_stops_at_the_hole_edge() {
        let ps = parts();
        // Square in the middle of the plate's hole (centre 210, 210, r 80).
        let mut sheet = vec![at(2, 10.0, 10.0, &ps), at(3, 200.0, 200.0, &ps)];
        assert!(group_ok(&ps, &sheet, &[1], [0.0, 0.0], &spec(), &fmt(), true));
        let t = slide(&ps, &mut sheet, &[1], Dir::Right, &spec(), &fmt(), f64::INFINITY, true);
        assert!(t > 40.0 && t < 80.0, "{t}");
        assert!(!nest::clash_exact(&ps, &sheet[0], &sheet[1], GAP - 1e-6));
    }

    #[test]
    fn a_dragged_part_slides_along_instead_of_jumping_back() {
        let ps = parts();
        let sheet = vec![at(0, 300.0, 100.0, &ps), at(0, 600.0, 100.0, &ps)];
        // Pull part 1 onto part 0 (and a bit down): it stops at the gap on the
        // right of part 0, keeping the vertical part of the move.
        let (d, ok) = drag(&ps, &sheet, &[1], Some([0.0, 0.0]), [-280.0, -20.0], &spec(), &fmt(), true, 0.0);
        assert!(ok);
        assert!(close(600.0 + d[0], 400.0 + GAP), "{d:?}");
        assert!(close(d[1], -20.0));
        // A copy that starts on its original is invalid until it is clear.
        let (_, ok) = drag(&ps, &sheet, &[1], None, [-280.0, 0.0], &spec(), &fmt(), true, 0.0);
        assert!(!ok);
    }

    #[test]
    fn a_dragged_part_snaps_to_the_gap_when_near() {
        let ps = parts();
        let sheet = vec![at(0, 300.0, 100.0, &ps), at(0, 600.0, 300.0, &ps)];
        // Dropped 10 mm right of the gap position, level with part 0: snaps in.
        let want = [400.0 + GAP + 10.0 - 600.0, -200.0];
        let (d, ok) = drag(&ps, &sheet, &[1], Some([0.0, 0.0]), want, &spec(), &fmt(), true, 14.0);
        assert!(ok && close(600.0 + d[0], 400.0 + GAP), "{d:?}");
        // Out of reach: stays where it is put.
        let (d, _) = drag(&ps, &sheet, &[1], Some([0.0, 0.0]), [want[0] + 30.0, want[1]], &spec(), &fmt(), true, 14.0);
        assert!(close(d[0], want[0] + 30.0));
    }

    #[test]
    fn a_selection_turns_about_its_centre() {
        let ps = parts();
        // Two 100 × 40 parts side by side; turned 90° left they stack.
        let sheet = vec![at(0, 300.0, 200.0, &ps), at(0, 410.0, 200.0, &ps)];
        let new = turn(&ps, &sheet, &[0, 1], 1, &spec(), &fmt(), true).unwrap();
        assert!(new.iter().all(|p| p.turns == 1 && close(p.w, 40.0) && close(p.h, 100.0)));
        let b0 = group_box(&sheet, &[0, 1]);
        let b1 = group_box(&new, &[0, 1]);
        let c = |b: [f64; 4]| [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        assert!(close(c(b0)[0], c(b1)[0]) && close(c(b0)[1], c(b1)[1]), "same centre: {b0:?} {b1:?}");
        assert!(new[0].y < new[1].y, "the left part ends up below");
        // A part that may not turn refuses.
        let mut stiff = ps.clone();
        stiff[0].rotate = false;
        assert!(turn(&stiff, &sheet, &[0], 1, &spec(), &fmt(), true).unwrap_err().contains("isn't allowed"));
    }

    #[test]
    fn a_part_turns_to_any_angle_and_back() {
        // HM14: a 100 × 40 part turned 30° about its middle; its footprint is
        // the box of the turned outline and every corner lies on that box.
        let ps = parts();
        let sheet = vec![at(0, 300.0, 200.0, &ps)];
        let new = turn_by(&ps, &sheet, &[0], 30.0, &spec(), &fmt(), true).unwrap();
        let p = new[0];
        assert!(close(p.angle(), 30.0) && p.turns == 0 && close(p.spin.deg, 30.0), "{p:?}");
        let (s, c) = 30f64.to_radians().sin_cos();
        assert!(close(p.w, 100.0 * c + 40.0 * s) && close(p.h, 100.0 * s + 40.0 * c), "{p:?}");
        assert!(close(p.x + p.w / 2.0, 350.0) && close(p.y + p.h / 2.0, 220.0), "same middle: {p:?}");
        let (o, _) = nest::placed_shape(&ps, &p);
        let b = nest::bbox_of(&o);
        assert!(close(b[0], p.x) && close(b[1], p.y) && close(b[2], p.x + p.w) && close(b[3], p.y + p.h), "{b:?} {p:?}");
        // The first corner went round the middle by 30°.
        let m = [350.0, 220.0];
        let q = [m[0] + (-50.0) * c - (-20.0) * s, m[1] + (-50.0) * s + (-20.0) * c];
        assert!(o.iter().any(|v| close(v[0], q[0]) && close(v[1], q[1])), "{o:?}");
        // On by 60° more it is a plain quarter turn again, in the same middle.
        let mut after = sheet.clone();
        after[0] = p;
        let q = turn_by(&ps, &after, &[0], 60.0, &spec(), &fmt(), true).unwrap()[0];
        assert!(q.turns == 1 && q.spin.is_none() && close(q.w, 40.0) && close(q.h, 100.0), "{q:?}");
        assert!(close(q.x, 330.0) && close(q.y, 170.0), "{q:?}");
        // Back by -30°: where it started.
        let back = turn_by(&ps, &after, &[0], -30.0, &spec(), &fmt(), true).unwrap()[0];
        assert!(back.spin.is_none() && close(back.x, 300.0) && close(back.y, 200.0) && back.turns == 0, "{back:?}");
    }

    #[test]
    fn a_group_turns_to_any_angle_about_its_centre() {
        // HM5, HM14: two parts turned 45° together keep their spacing; their
        // middles go round the middle of the group's box.
        let ps = parts();
        let sheet = vec![at(0, 300.0, 200.0, &ps), at(0, 410.0, 200.0, &ps)];
        let new = turn_by(&ps, &sheet, &[0, 1], 45.0, &spec(), &fmt(), true).unwrap();
        let mid = |p: &Placed| nest::placed_pivot(&ps[0], p);
        let (a, b) = (mid(&new[0]), mid(&new[1]));
        assert!(close((b[0] - a[0]).hypot(b[1] - a[1]), 110.0), "spacing kept: {a:?} {b:?}");
        assert!(close((b[1] - a[1]).atan2(b[0] - a[0]).to_degrees(), 45.0));
        assert!(close((a[0] + b[0]) / 2.0, 405.0) && close((a[1] + b[1]) / 2.0, 220.0));
        assert!(new.iter().all(|p| close(p.angle(), 45.0)));
        // A part that may not turn 90° takes no other angle either.
        let mut stiff = ps.clone();
        stiff[0].rotate = false;
        assert!(turn_by(&stiff, &sheet, &[0], 15.0, &spec(), &fmt(), true).unwrap_err().contains("may turn 90°"));
        assert!(!can_turn_by(&stiff, &sheet, &[0], 15.0) && can_turn_by(&ps, &sheet, &[0], 15.0));
    }

    #[test]
    fn a_part_at_any_angle_mirrors_and_turns_by_quarters() {
        // HM6 with HM14: mirroring left↔right reverses the angle, top↔bottom
        // adds a half turn; the footprint is mirrored in the group's box.
        let mut ps = parts();
        ps[1].rots.mirrored = [true; 4];
        let sheet = vec![at(1, 300.0, 100.0, &ps)];
        let p = turn_by(&ps, &sheet, &[0], 20.0, &spec(), &fmt(), true).unwrap()[0];
        let lr = mirror(&ps, &[p], &[0], true, &spec(), &fmt(), true).unwrap()[0];
        assert!(lr.mirror && close(lr.angle(), 340.0) && close(lr.w, p.w) && close(lr.h, p.h) && close(lr.x, p.x) && close(lr.y, p.y), "{lr:?}");
        // Its outline is the mirror image about the box's middle line.
        let (o1, _) = nest::placed_shape(&ps, &p);
        let (o2, _) = nest::placed_shape(&ps, &lr);
        let axis = p.x + p.w / 2.0;
        assert!(o1.iter().all(|q| o2.iter().any(|v| close(v[0], 2.0 * axis - q[0]) && close(v[1], q[1]))), "{o1:?} {o2:?}");
        let tb = mirror(&ps, &[p], &[0], false, &spec(), &fmt(), true).unwrap()[0];
        assert!(tb.mirror && close(tb.angle(), 160.0), "{tb:?}");
        // A quarter turn keeps the free part of the angle.
        let q = turn(&ps, &[p], &[0], 1, &spec(), &fmt(), true).unwrap()[0];
        assert!(q.turns == 1 && close(q.spin.deg, 20.0), "{q:?}");
    }

    #[test]
    fn a_turned_part_that_would_overlap_moves_to_the_nearest_free_spot() {
        let ps = parts();
        // A 100 × 40 part between two others 50 mm apart can't turn in place.
        let sheet = vec![at(0, 300.0, 200.0, &ps), at(3, 330.0, 250.0, &ps)];
        let new = turn(&ps, &sheet, &[0], 1, &spec(), &fmt(), true).unwrap();
        let mut after = sheet.clone();
        after[0] = new[0];
        assert!(group_ok(&ps, &after, &[0], [0.0, 0.0], &spec(), &fmt(), true));
    }

    #[test]
    fn mirroring_flips_the_layout_and_the_parts() {
        let mut ps = parts();
        let sheet = vec![at(1, 100.0, 100.0, &ps), at(0, 320.0, 100.0, &ps)];
        assert!(mirror(&ps, &sheet, &[0, 1], true, &spec(), &fmt(), true).unwrap_err().contains("mirrored"), "parts mirror only when allowed");
        for p in &mut ps {
            p.rots.mirrored = [true; 4];
        }
        let sheet = vec![at(1, 100.0, 100.0, &ps), at(0, 320.0, 100.0, &ps)];
        let new = mirror(&ps, &sheet, &[0, 1], true, &spec(), &fmt(), true).unwrap();
        assert!(new[0].mirror && new[1].mirror);
        assert!(new[0].x > new[1].x, "left and right swap");
        let g = group_box(&sheet, &[0, 1]);
        assert_eq!(group_box(&new, &[0, 1]), g, "same box");
        // The gusset's right angle now sits bottom-right: its outline's lowest-right corner.
        let (o, _) = nest::placed_shape(&ps, &new[0]);
        assert!(o.iter().any(|q| close(q[0], new[0].x + new[0].w) && close(q[1], new[0].y + new[0].h)), "{o:?}");
        let top = mirror(&ps, &sheet, &[0], false, &spec(), &fmt(), true).unwrap();
        assert!(top[0].mirror && top[0].turns == 2);
    }

    #[test]
    fn a_copied_selection_lands_clear_of_every_part() {
        let ps = parts();
        let sheet = vec![at(0, 10.0, 10.0, &ps), at(0, 10.0, 60.0, &ps)];
        let d = copy_spot(&sheet, &[0, 1], &spec(), &fmt()).unwrap();
        let mut all = sheet.clone();
        all.extend(copies(&sheet, &[0, 1], d));
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(!a.clashes(b, GAP - 1e-6), "{a:?} {b:?}");
            }
        }
        // Left first, then lowest: on top of the pair.
        assert!(close(all[2].x, 10.0) && close(all[2].y, 100.0 + GAP), "left first: {:?}", all[2]);
    }

    #[test]
    fn a_window_selects_the_parts_wholly_inside_and_a_crossing_one_those_it_touches() {
        use crate::engine::selection::Take;
        let ps = parts();
        let sheet = vec![at(0, 10.0, 10.0, &ps), at(0, 200.0, 10.0, &ps), at(3, 120.0, 20.0, &ps)];
        // Dragged to the right: #2 sticks out past X 250, so only #1 and #3.
        assert_eq!(window_select(&ps, &sheet, &Window::dragged([0.0, 0.0], [250.0, 100.0])), vec![0, 2]);
        // The same box dragged to the left takes #2 too, as it touches it.
        assert_eq!(window_select(&ps, &sheet, &Window::dragged([250.0, 100.0], [0.0, 0.0])), vec![0, 1, 2]);
        // A small crossing box on #2's edge takes #2 alone.
        let edge = Window::new([195.0, 15.0], [205.0, 25.0], Take::Touching);
        assert_eq!(window_select(&ps, &sheet, &edge), vec![1]);
    }

    #[test]
    fn part_numbers_parse_as_lists_and_ranges() {
        assert_eq!(parse_items("1,3, 5", 5).unwrap(), vec![0, 2, 4]);
        assert_eq!(parse_items("#2-4", 5).unwrap(), vec![1, 2, 3]);
        assert!(parse_items("6", 5).unwrap_err().contains("no part #6"));
        assert!(parse_items("x", 5).is_err());
    }
}
