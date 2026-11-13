//! Repeating a pattern of placed parts (HM14–HM17): copies of a group of
//! parts, keeping their layout, that use the pieces still to place instead of
//! growing the order. When a copy needs more pieces than are left, the user
//! (or the agent) chooses: add extras, or place only the pieces left. The GUI
//! drops one copy per click (Ctrl+C, `arrange_ui.rs`); agents use `repeat`.

use crate::nest::{Part, Placed};

/// One part type in a pattern: how many pieces one copy uses, and how many
/// of it are still to place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Use {
    pub part: usize,
    pub per: u32,
    pub left: u32,
}

/// What to do with a copy that needs more pieces than are left (HM16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Over {
    /// Place the whole copy; the order grows by the pieces beyond it.
    Extras,
    /// Place only the pieces still to place.
    OnlyLeft,
}

/// What one copy of `pattern` uses, per part type, in the pattern's order.
pub fn uses(parts: &[Part], sheets: &[Vec<Placed>], pattern: &[Placed]) -> Vec<Use> {
    let mut v: Vec<Use> = vec![];
    for p in pattern {
        match v.iter_mut().find(|u| u.part == p.part) {
            Some(u) => u.per += 1,
            None => {
                let left = parts.get(p.part).map_or(0, |q| q.qty.saturating_sub(super::placed_count(sheets, p.part) as u32));
                v.push(Use { part: p.part, per: 1, left });
            }
        }
    }
    v
}

/// Whole copies the pieces left still make: the fewest over the part types.
pub fn copies_left(uses: &[Use]) -> u32 {
    uses.iter().map(|u| u.left / u.per).min().unwrap_or(0)
}

/// Pieces one copy needs beyond those left (0: the copy fits the order).
pub fn short(uses: &[Use]) -> u32 {
    uses.iter().map(|u| u.per.saturating_sub(u.left)).sum()
}

/// Pieces one copy can take from those left (what "place only those left" places).
pub fn placeable(uses: &[Use]) -> u32 {
    uses.iter().map(|u| u.per.min(u.left)).sum()
}

/// Which of the pattern's pieces go down for one copy (indices into
/// `pattern`, in order). With pieces enough, or `Extras`: all of them.
/// `OnlyLeft`: per part type only as many as are left — the held piece
/// first, then the nearest to it (HM16). A short copy with no answer yet:
/// `None`, ask first.
pub fn pick(uses: &[Use], pattern: &[Placed], held: usize, over: Option<Over>) -> Option<Vec<usize>> {
    if short(uses) == 0 || over == Some(Over::Extras) {
        return Some((0..pattern.len()).collect());
    }
    over?;
    let centre = |p: &Placed| [p.x + p.w / 2.0, p.y + p.h / 2.0];
    let h = pattern.get(held).map_or([0.0, 0.0], centre);
    let mut order: Vec<usize> = (0..pattern.len()).collect();
    order.sort_by(|&a, &b| {
        let d = |i: usize| {
            let c = centre(&pattern[i]);
            (c[0] - h[0]).hypot(c[1] - h[1])
        };
        (a != held).cmp(&(b != held)).then(d(a).total_cmp(&d(b))).then(a.cmp(&b))
    });
    let mut taken = vec![0u32; uses.len()];
    let mut out = vec![];
    for i in order {
        if let Some(k) = uses.iter().position(|u| u.part == pattern[i].part) {
            if taken[k] < uses[k].left {
                taken[k] += 1;
                out.push(i);
            }
        }
    }
    out.sort_unstable();
    Some(out)
}

/// Put copy pieces on sheet `s`: each takes a piece still to place, and only
/// a piece beyond the order makes it grow by one (HM15). Returns how many
/// pieces were extras.
pub fn put(parts: &mut [Part], sheets: &mut [Vec<Placed>], s: usize, pieces: &[Placed]) -> u32 {
    let mut extras = 0;
    for p in pieces {
        if super::placed_count(sheets, p.part) as u32 >= parts[p.part].qty {
            parts[p.part].qty += 1;
            extras += 1;
        }
        sheets[s].push(*p);
    }
    extras
}

fn name(parts: &[Part], i: usize) -> &str {
    parts.get(i).map_or("?", |p| p.name.as_str())
}

fn pieces(n: u32) -> String {
    if n == 1 { "1 piece".into() } else { format!("{n} pieces") }
}

/// The line beside a pattern on the pointer (HM15): what one copy uses, what
/// is left and how many whole copies that makes — or, for a copy that is
/// short, what the drop will do.
pub fn label(parts: &[Part], uses: &[Use], over: Option<Over>) -> String {
    let what = match uses {
        [u] => format!("{} × {} per copy · {} left", u.per, name(parts, u.part), u.left),
        _ => format!("Per copy: {}", uses.iter().map(|u| format!("{} × {} ({} left)", u.per, name(parts, u.part), u.left)).collect::<Vec<_>>().join(", ")),
    };
    let sh = short(uses);
    if sh == 0 {
        let n = copies_left(uses);
        return format!("{what} · {n} cop{} to go", if n == 1 { "y" } else { "ies" });
    }
    let can = placeable(uses);
    match over {
        Some(Over::Extras) => format!("{what} · this copy adds {} to the order", pieces(sh)),
        Some(Over::OnlyLeft) => format!("{what} · this copy places only the {} left", pieces(can)),
        None if can == 0 => format!("{what} · a drop asks before adding extras"),
        None => format!("{what} · {} short — a drop asks", pieces(sh)),
    }
}

/// The question for a copy that is short (HM16): which part types it needs
/// more of than are left.
pub fn question(parts: &[Part], uses: &[Use]) -> String {
    let short: Vec<String> = uses
        .iter()
        .filter(|u| u.per > u.left)
        .map(|u| {
            let left = if u.left == 0 { "none".to_string() } else { u.left.to_string() };
            format!("{} × {} ({left} left)", u.per, name(parts, u.part))
        })
        .collect();
    format!("This copy needs {}.", short.join(" and "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::Shape;

    fn part(name: &str, qty: u32) -> Part {
        Part { name: name.into(), shape: Shape::Rectangle, w: 100.0, h: 100.0, qty, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }
    fn at(part: usize, x: f64) -> Placed {
        Placed { part, x, y: 0.0, w: 100.0, h: 100.0, turns: 0, mirror: false }
    }

    #[test]
    fn a_copy_uses_the_pieces_left_and_counts_the_copies_to_go() {
        // HM15: two of 18 placed as a pattern of two: 16 left, 8 copies.
        let parts = vec![part("Gusset", 18)];
        let sheets = vec![vec![at(0, 0.0), at(0, 0.0)]];
        let u = uses(&parts, &sheets, &sheets[0]);
        assert_eq!(u, vec![Use { part: 0, per: 2, left: 16 }]);
        assert_eq!((copies_left(&u), short(&u)), (8, 0));
        assert_eq!(label(&parts, &u, None), "2 × Gusset per copy · 16 left · 8 copies to go");
        assert_eq!(pick(&u, &sheets[0], 0, None), Some(vec![0, 1]));
        // Put down: the order stays at 18.
        let (mut parts, mut sheets) = (parts, sheets);
        let pat = sheets[0].clone();
        assert_eq!(put(&mut parts, &mut sheets, 0, &pat), 0);
        assert_eq!((parts[0].qty, sheets[0].len()), (18, 4));
    }

    #[test]
    fn a_short_copy_asks_then_adds_extras_or_places_only_those_left() {
        // HM16: one Gusset left, the pattern holds two.
        let parts = vec![part("Gusset", 3), part("Bracket", 5)];
        let pat = vec![at(0, 0.0), at(1, 150.0), at(0, 300.0)];
        let sheets = vec![vec![at(0, 0.0), at(0, 300.0)]];
        let u = uses(&parts, &sheets, &pat);
        assert_eq!(u, vec![Use { part: 0, per: 2, left: 1 }, Use { part: 1, per: 1, left: 5 }]);
        assert_eq!((copies_left(&u), short(&u), placeable(&u)), (0, 1, 2));
        assert_eq!(pick(&u, &pat, 2, None), None, "asks first");
        assert_eq!(question(&parts, &u), "This copy needs 2 × Gusset (1 left).");
        assert!(label(&parts, &u, None).ends_with("1 piece short — a drop asks"), "{}", label(&parts, &u, None));
        assert_eq!(pick(&u, &pat, 2, Some(Over::Extras)), Some(vec![0, 1, 2]));
        // Only those left: the held Gusset (#3), not the far one, and the Bracket.
        assert_eq!(pick(&u, &pat, 2, Some(Over::OnlyLeft)), Some(vec![1, 2]));
        assert_eq!(pick(&u, &pat, 0, Some(Over::OnlyLeft)), Some(vec![0, 1]));
        // Extras grow the order by the pieces beyond it only.
        let (mut parts, mut sheets) = (parts, sheets);
        assert_eq!(put(&mut parts, &mut sheets, 0, &pat), 1);
        assert_eq!((parts[0].qty, parts[1].qty), (4, 5));
    }

    #[test]
    fn nothing_left_says_a_drop_asks_before_adding_extras() {
        let parts = vec![part("Clip", 1)];
        let sheets = vec![vec![at(0, 0.0)]];
        let u = uses(&parts, &sheets, &sheets[0]);
        assert_eq!((copies_left(&u), placeable(&u)), (0, 0));
        assert!(label(&parts, &u, None).ends_with("0 left · a drop asks before adding extras"), "{}", label(&parts, &u, None));
        assert_eq!(question(&parts, &u), "This copy needs 1 × Clip (none left).");
        assert_eq!(pick(&u, &sheets[0], 0, Some(Over::OnlyLeft)), Some(vec![]));
    }
}
