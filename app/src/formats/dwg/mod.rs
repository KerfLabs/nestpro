//! AutoCAD DWG part drawings (JOB-1, rules DW1–DW16 in MECHANICS.md).
//!
//! The original program turns a DWG into an R12 DXF with a bundled converter and then
//! runs its DXF import (DW1). We read the DWG ourselves, natively, and hand the result
//! to the same place the DXF reader feeds: a `dxf::Drawing` with one piece per drawn
//! object, its layer name, and texts. `dxf::build` then joins pieces, picks the outline
//! and holes and applies layer roles exactly as for DXF.
//!
//! Versions read natively: R13, R14, 2000, 2004, 2007, 2010, 2013 and 2018 (all DWG
//! versions AutoCAD has written since 1994). Older files, and files that cannot be read
//! for another reason, go through a DWG→DXF converter if one is installed (ODA File
//! Converter or LibreDWG's `dwg2dxf`), otherwise the user is told to save as DXF (DW16).

mod bits;
mod container;
mod convert;
pub(crate) mod geom;
pub(crate) mod objects;
mod r2007;

use crate::dxf::Drawing;
use bits::Ver;
use objects::Kind;

/// How a DWG is read. Defaults follow the original's converter settings where they
/// matter for geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Also read paper-space layouts (title blocks, sheet borders). Off: model space only.
    pub paper_space: bool,
    /// Explode block inserts into their objects (nested blocks too).
    pub explode_blocks: bool,
    /// Leave out objects marked invisible. The original's converter keeps them (DW13).
    pub skip_invisible: bool,
    /// When the file cannot be read natively, try an installed DWG→DXF converter.
    pub use_converter: bool,
    /// mm per drawing unit (the drawing's own unit isn't read yet): every length
    /// is placed in mm, so arcs are split within ARC_CHORD_ERR mm (IU1).
    pub unit_mm: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options { paper_space: false, explode_blocks: true, skip_invisible: false, use_converter: true, unit_mm: 1.0 }
    }
}

/// Read a DWG file into one part drawing.
pub fn read(bytes: &[u8], opts: &Options) -> Result<Vec<super::ReadPart>, String> {
    // DW15: the reader checks every read; should a case still slip through, the
    // import fails with a message instead of closing the program.
    let native = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_native(bytes, opts)))
        .unwrap_or_else(|_| Err("This DWG could not be read (it looks damaged). Open it in a CAD program and save it as DXF.".into()));
    let drawing = match native {
        Ok(d) => d,
        Err(e) => {
            // A converter cannot help with a file that is not a DWG, is locked, or simply
            // has nothing in model space.
            let is_dwg = bytes.starts_with(b"AC") || bytes.starts_with(b"MC0.0");
            let hopeless = e.starts_with("No lines") || e.contains("password") || e.contains("encrypted") || e.contains("bigger than the import");
            if !is_dwg || hopeless || !opts.use_converter {
                return Err(e);
            }
            match convert::find() {
                Some(conv) => convert::run(&conv, bytes, opts.unit_mm).map_err(|ce| format!("{e} Converting it with {} failed too: {ce}", conv.name()))?,
                None => return Err(e),
            }
        }
    };
    Ok(vec![super::ReadPart { drawing, meta: super::Meta::default(), mark_layers: vec![] }])
}

/// Read a DWG with our own reader only (no external converter).
pub fn read_native(bytes: &[u8], opts: &Options) -> Result<Drawing, String> {
    let (ver, name) = container::version(bytes)?;
    let unreadable = |why: String| format!("This AutoCAD {name} DWG could not be read ({why}). Open it in a CAD program and save it as DXF.");
    let layout = match ver {
        Ver::R13 | Ver::R14 | Ver::R2000 => container::layout_r13(bytes, ver),
        Ver::R2007 => r2007::sections(bytes).and_then(|s| container::layout_r2004(bytes, ver, s)),
        _ => container::sections_r2004(bytes).and_then(|s| container::layout_r2004(bytes, ver, s)),
    }
    .map_err(|e| if e.contains("password") || e.contains("encrypted") { e } else { unreadable(e) })?;

    let reader = objects::Reader { buf: &layout.objects, ver, classes: layout.classes.iter().map(|c| (c.num, c.clone())).collect() };
    let mut db = geom::Db::default();
    let mut damaged = 0usize;
    for &(handle, off) in &layout.map {
        match reader.read(off) {
            Ok(o) if o.handle == handle => {
                match &o.kind {
                    Kind::Layer { name } => {
                        db.layers.insert(handle, name.clone());
                    }
                    Kind::BlockHeader { name, base } => {
                        db.blocks.insert(handle, (name.clone(), *base));
                    }
                    Kind::Other => continue,
                    _ => {}
                }
                if let Some(ow) = o.owner {
                    db.owned.entry(ow).or_default().push(handle);
                }
                db.order.push(handle);
                db.objs.insert(handle, o);
            }
            _ => damaged += 1,
        }
    }
    if db.objs.is_empty() {
        return Err(unreadable("no objects could be read".into()));
    }

    // DW6: model space (and paper space when asked); blocks only through inserts.
    let mut ctx = geom::Ctx::new(&db, opts.explode_blocks, opts.skip_invisible);
    let u = opts.unit_mm;
    let unit = geom::Mat { x: [u, 0.0, 0.0], y: [0.0, u, 0.0], z: [0.0, 0.0, u], o: [0.0; 3] };
    for h in &db.order {
        if ctx.over {
            break;
        }
        let o = &db.objs[h];
        let top = o.entmode == 2 || (opts.paper_space && o.entmode == 1);
        if top {
            ctx.entity(o, &unit, None, 0);
        }
    }
    // Over a limit, part of the drawing is missing: refuse it whole.
    if let Some(e) = ctx.refusal() {
        return Err(e);
    }
    let geom::Ctx { mut d, depth_hit, .. } = ctx;
    if damaged > 0 {
        d.report.skipped.push(format!("{damaged} objects that could not be read"));
    }
    if depth_hit {
        d.report.skipped.push("blocks nested more than 16 deep".into());
    }
    if damaged > 0 && d.pieces.is_empty() {
        return Err(unreadable(format!("{damaged} damaged objects")));
    }
    if d.pieces.is_empty() {
        return Err("No lines, arcs, circles or polylines were found in the drawing's model space.".into());
    }
    Ok(d)
}

#[cfg(test)]
mod tests;
