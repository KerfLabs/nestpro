//! A part's cut order (CO3–CO7): the order its contours are cut and marked
//! in. The programmer sets any part of it by hand; the rest follows the
//! automatic order, and the outline is always cut last. The order set by hand
//! is a list of contour numbers (`dxf::CustomGeo::hand_order`), so contour
//! numbers never change: leads, joints, picked starts and hand-set qualities
//! keep pointing at the same contours.

use super::contours::Look;
use crate::ct1::MachineTech;
use crate::dxf::{self, Contour, ContourState};
use crate::nest::{CutQuality, Part, Shape};

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
fn hand(part: &Part) -> Vec<usize> {
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

/// An imported part's geometry; a built-in shape has none to order.
fn custom(part: &Part) -> Option<&dxf::CustomGeo> {
    match (&part.custom, part.shape) {
        (Some(geo), Shape::Custom) => Some(geo),
        _ => None,
    }
}

/// A part's cut order (CO3, CO5): every contour cut or marked (numbers from
/// 0), in the order the part cuts it, and whether it was set by hand. The
/// order set by hand comes first, then the rest in the automatic order, and
/// the outline always last.
pub fn order(part: &Part) -> Vec<(usize, bool)> {
    let list = super::contours::list(part);
    let hand = hand(part);
    let o = dxf::outline_index(&list);
    let by_hand = hand.iter().filter(|&&k| Some(k) != o).map(|&k| (k, true));
    let auto = automatic(&list).into_iter().filter(|k| !hand.contains(k) && Some(*k) != o).map(|k| (k, false));
    let outline = o.filter(|&k| in_order(&list[k])).map(|k| (k, hand.contains(&k)));
    by_hand.chain(auto).chain(outline).collect()
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
    let auto = automatic(&list);
    // A bulk command takes its contours in the automatic order.
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
