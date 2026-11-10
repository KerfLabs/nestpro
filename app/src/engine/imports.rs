//! Bringing work in from outside, shared by the window and the agents:
//! a Lantek import XML (products grouped into one job per material and
//! thickness) and a BLM LC5 machine program set turned back into a job.

use super::*;
use crate::xmlimport::{material_index, parse, XmlProduct};

/// One job the XML import makes: products of one material and thickness.
pub struct XmlJob {
    /// The file name, or "<file> · <material> <thickness>" when the file holds several groups.
    pub name: String,
    /// The material as written in the file, and the app's material for it (None = not recognised).
    pub material_text: String,
    pub material: Option<usize>,
    /// 0 = not given.
    pub thickness: f64,
    pub parts: Vec<Part>,
}

pub struct XmlImport {
    pub jobs: Vec<XmlJob>,
    /// Products in the file, and how many became parts.
    pub products: usize,
    pub added: usize,
    pub notes: Vec<String>,
}

fn key(p: &XmlProduct) -> (String, f64) {
    (p.material.to_lowercase(), (p.thickness * 1000.0).round() / 1000.0)
}

/// Read a Lantek import XML (DXF Import / Parts Import). Each product's drawing
/// (Fich, relative to the XML) is read with every layer cut; without one, the
/// library product of the same reference is used. Quantity 0 is skipped.
/// Part colours start at `color0`.
pub fn xml_import(cfg: &AppSettings, lib: Option<&crate::library::Library>, path: &Path, color0: usize) -> Result<XmlImport, String> {
    let text = std::fs::read(path).map(|b| String::from_utf8_lossy(&b).to_string()).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let products = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if products.is_empty() {
        return Err(format!("{} has no products.", path.display()));
    }
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "XML".into());
    // Groups of material and thickness, in file order.
    let mut groups: Vec<(String, f64)> = vec![];
    for p in &products {
        if !groups.contains(&key(p)) {
            groups.push(key(p));
        }
    }
    let several = groups.len() > 1;
    let mut out = XmlImport { jobs: vec![], products: products.len(), added: 0, notes: vec![] };
    let mut color = color0;
    for (mat, thk) in groups {
        let first = products.iter().find(|p| p.material.to_lowercase() == mat).map(|p| p.material.clone()).unwrap_or_default();
        // Names, codes and aliases first (an alias gives its main material, MA6), then by look.
        let material = cfg.material_for(&first).or_else(|| material_index(&first, &cnc::MATERIALS, &cfg.materials));
        let mut job = XmlJob { name: if several { format!("{stem} · {first} {thk}") } else { stem.clone() }, material_text: first, material, thickness: thk, parts: vec![] };
        for p in products.iter().filter(|p| key(p) == (mat.clone(), thk)) {
            // Quantity 0 (or not a number, which reads as 0, XI6): nothing to make.
            if p.quantity == 0 {
                out.notes.push(format!("{}: quantity 0 — skipped", p.reference));
                continue;
            }
            let name = crate::filename::part_reference(&cfg.ref_prefix, &p.reference, &cfg.ref_suffix);
            let part = match &p.fich {
                Some(f) => {
                    let fp = PathBuf::from(f.trim());
                    let fp = if fp.is_absolute() { fp } else { dir.join(fp) };
                    // Any drawing format, one part per product: a file holding several
                    // parts (a Lantek nesting, a VULCAN program) can't stand for one.
                    let one = crate::formats::read_file(&fp, None, cfg).and_then(|mut v| {
                        if v.len() == 1 {
                            Ok(v.swap_remove(0))
                        } else {
                            Err(format!("{} holds {} parts, and a product takes one drawing — import the file with Import drawings instead", fp.display(), v.len()))
                        }
                    });
                    match one {
                        // Every layer cut, except a format's marking layers and what the drawing hides (DX3).
                        Ok(rp) => match dxf::build(&rp.drawing, &crate::formats::default_roles([&rp])) {
                            Ok((g, rep)) => {
                                // What the drawing's import left out, named per product.
                                out.notes.extend(super::import_notes(&p.reference, &rep));
                                Some(Part {
                                name: name.clone(),
                                shape: Shape::Custom,
                                w: g.w,
                                h: g.h,
                                qty: p.quantity,
                                rotate: true,
                                color,
                                tech: None,
                                texts: g.texts.clone(),
                                custom: Some(std::sync::Arc::new(g)),
                                rots: Default::default(),
                                group: String::new(),
                                erp: vec![],
                            })
                            }
                            Err(e) => {
                                out.notes.push(format!("{}: {e}", p.reference));
                                None
                            }
                        },
                        Err(e) => {
                            // The message already names the drawing's file.
                            out.notes.push(format!("{}: {e}", p.reference));
                            None
                        }
                    }
                }
                // Parts Import: the part from the database (here: the library).
                None => match lib.and_then(|l| l.find_product(&p.reference).and_then(|id| l.load_product(id).ok())) {
                    Some(pf) => {
                        let mut part = pf.part;
                        part.name = name.clone();
                        part.qty = p.quantity;
                        part.color = color;
                        Some(part)
                    }
                    None => {
                        out.notes.push(format!("{}: no drawing (Fich) and not in the library", p.reference));
                        None
                    }
                },
            };
            if let Some(mut part) = part {
                if let Some(r) = p.rotations {
                    part.rots = r;
                    part.rotate = r.plain[1] || r.plain[3] || r.mirrored[1] || r.mirrored[3];
                }
                job.parts.push(part);
                color += 1;
                out.added += 1;
            }
        }
        out.jobs.push(job);
    }
    Ok(out)
}

/// The app's material for a material name in a machine program header:
/// Settings → Materials first, then by its look.
pub fn program_material(cfg: &AppSettings, name: &str) -> usize {
    let mu = name.to_uppercase();
    cfg.material_for(name).unwrap_or(if mu.contains("STAIN") || mu.contains("INOX") || mu.contains("X5CR") || mu.contains("304") || mu.contains("316") {
        1
    } else if mu.starts_with("AL") {
        2
    } else {
        0
    })
}

/// The part of the line a→b inside the sheet 0..w × 0..h (Liang–Barsky).
pub fn clip_to_sheet(a: P, b: P, w: f64, h: f64) -> Option<(P, P)> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    for (p, q) in [(-d[0], a[0]), (d[0], w - a[0]), (-d[1], a[1]), (d[1], h - a[1])] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    if !t0.is_finite() || !t1.is_finite() || t1 - t0 < 1e-9 {
        return None;
    }
    Some(([a[0] + d[0] * t0, a[1] + d[1] * t0], [a[0] + d[0] * t1, a[1] + d[1] * t1]))
}

/// A nesting read back from a machine program set.
pub struct ProgramJob {
    pub name: String,
    pub spec: SheetSpec,
    pub parts: Vec<Part>,
    pub sheet: Vec<Placed>,
    pub cuts: Vec<CutLine>,
    pub notes: Vec<String>,
}

/// Open a BLM LC5 program set (`<NEST>.zip`, its `.XML`, or a Lantek `.nxp`
/// list) as an editable nesting: each part subprogram a part, each call a
/// placed part, the header's sheet, material and thickness over `base`, and
/// cut-offs back as remnant lines (edge to edge) or free cuts.
pub fn program_job(cfg: &AppSettings, base: &SheetSpec, path: &Path, color0: usize) -> Result<ProgramJob, String> {
    let im = crate::progimport::open(path).map_err(|e| format!("Couldn't open {}: {e}", path.display()))?;
    let mut spec = base.clone();
    if im.sheet_w > 0.0 && im.sheet_h > 0.0 {
        spec.w = im.sheet_w;
        spec.h = im.sheet_h;
    }
    if im.thickness > 0.0 {
        spec.thickness = im.thickness;
    }
    spec.material = program_material(cfg, &im.material);
    let parts: Vec<Part> = im
        .parts
        .iter()
        .enumerate()
        .map(|(k, (pname, _sub, g))| Part {
            name: pname.clone(),
            shape: Shape::Custom,
            w: g.w,
            h: g.h,
            qty: im.placed.iter().filter(|p| p.0 == k).count() as u32,
            rotate: true,
            color: color0 + k,
            tech: None,
            texts: vec![],
            group: String::new(),
            erp: vec![],
            custom: Some(std::sync::Arc::new(g.clone())),
            rots: Default::default(),
        })
        .collect();
    let sheet: Vec<Placed> = im.placed.iter().map(|&(k, x, y)| Placed { part: k, x, y, w: parts[k].w, h: parts[k].h, turns: 0, mirror: false }).collect();
    let cuts: Vec<CutLine> = im
        .cuts
        .iter()
        .filter_map(|c| {
            let (a, b) = (*c.first()?, *c.last()?);
            Some(match clip_to_sheet(a, b, spec.w, spec.h) {
                Some((p, q)) => CutLine { pts: vec![p, q], kind: CutKind::Remnant },
                None => CutLine { pts: c.clone(), kind: CutKind::Free },
            })
        })
        .collect();
    Ok(ProgramJob { name: im.name, spec, parts, sheet, cuts, notes: im.notes })
}

impl ProgramJob {
    /// As a job file (one sheet, the cut-offs on it).
    pub fn into_jobfile(self) -> JobFile {
        let mut j = JobFile::new_job(&self.name, self.spec);
        j.week = "Opened programs".into();
        j.parts = self.parts;
        j.sheets = vec![self.sheet];
        j.cut_lines = vec![self.cuts];
        j.method = "opened from machine program".into();
        j
    }
}

/// A signature of a job's content, to tell whether a job file on disk is the
/// one the window last read or wrote. The stock booking is left out (saving
/// books the stock, which doesn't make the job different).
pub fn job_sig(j: &JobFile) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut c = j.clone();
    c.held = Default::default();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&c).unwrap_or_default().hash(&mut h);
    h.finish()
}

/// One part of a drawing file as the import window shows it before it's
/// imported (IM10).
pub struct PreviewPart {
    /// The part's name: the file's, or numbered for a file of several parts.
    pub name: String,
    /// Everything the file draws for this part, each piece with the role its
    /// layer starts with (every layer cut, a format's marking layers marked).
    pub pieces: Vec<(dxf::LayerRole, Vec<P>)>,
    /// Lower-left and upper-right corners of everything drawn.
    pub min: P,
    pub max: P,
    pub layers: usize,
    /// The part those roles make with its notes (what's left out, part data
    /// that doesn't match the job), or why no part can be made.
    pub part: Result<(Part, Vec<String>), String>,
}

/// A drawing file read for the preview: its format and each part in it.
pub struct DrawingPreview {
    pub format: crate::formats::Format,
    pub parts: Vec<PreviewPart>,
    /// Each part as read, to make it again with other layer roles and
    /// contour choices (IM11).
    pub reads: Vec<crate::formats::Incoming>,
    /// A fingerprint of the bytes read (`fingerprint`): Import compares it
    /// with the file as it is then (IM11).
    pub fingerprint: u64,
}

/// A fingerprint of a file's bytes, to tell whether it changed since it was
/// read, whatever its size and time say (IM11).
pub fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Read a drawing file for the import window's preview (IM10): the same
/// reading, part names and starting layer roles as Import drawings and
/// `import-drawing`, without adding anything to the job.
pub fn drawing_preview(cfg: &AppSettings, spec: &SheetSpec, path: &Path) -> Result<DrawingPreview, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: couldn't read ({e})", path.display()))?;
    let (format, parts) = crate::formats::read_path_bytes(path, &bytes, None, cfg, Default::default())?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Drawing".into());
    let reads = crate::formats::incoming(&stem, parts);
    let roles = DrawingChoices::default().roles(&reads);
    let parts = reads
        .iter()
        .map(|inc| {
            let (mut min, mut max) = ([f64::MAX; 2], [f64::MIN; 2]);
            let pieces = inc
                .part
                .drawing
                .pieces
                .iter()
                .map(|p| {
                    for q in &p.pts {
                        min = [min[0].min(q[0]), min[1].min(q[1])];
                        max = [max[0].max(q[0]), max[1].max(q[1])];
                    }
                    (roles.get(&p.layer).copied().unwrap_or(dxf::LayerRole::Cut), p.pts.clone())
                })
                .collect();
            if min[0] > max[0] {
                (min, max) = ([0.0; 2], [0.0; 2]);
            }
            let part = part_from_read(cfg, spec, &inc.file, &inc.name, &inc.part.drawing, &inc.part.meta, &roles, 0);
            PreviewPart { name: inc.name.clone(), pieces, min, max, layers: inc.part.drawing.layers().len(), part }
        })
        .collect();
    Ok(DrawingPreview { format, parts, reads, fingerprint: fingerprint(&bytes) })
}

/// One contour choice made before a drawing is imported (IM11, IM12):
/// contours by number on the part its layer roles make, painted as
/// `contour-tech` paints them.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourChoice {
    /// Contour numbers, from 0.
    pub contours: Vec<usize>,
    pub paint: contours::Paint,
    /// All the same contours (CS5).
    pub same: bool,
}

/// The part one part of a drawing makes with layer roles `roles`, then each
/// of `choices` painted in turn with `contours::paint` (IM11, IM12): the import
/// preview's panel and `import-drawing contours=` both make parts this way. A
/// choice the part can't take is refused as `contour-tech` refuses it.
pub fn chosen_part(cfg: &AppSettings, spec: &SheetSpec, read: &crate::formats::Incoming, roles: &BTreeMap<String, dxf::LayerRole>, choices: &[ContourChoice], color: usize) -> Result<(Part, Vec<String>), String> {
    let (part, notes) = part_from_read(cfg, spec, &read.file, &read.name, &read.part.drawing, &read.part.meta, roles, color)?;
    let mut parts = vec![part];
    for c in choices {
        contours::paint(&mut parts, &[], &mut [], 0, &c.contours, c.paint, c.same).map_err(|e| format!("{}: {e}", read.name))?;
    }
    Ok((parts.swap_remove(0), notes))
}

/// What the programmer set in the import preview for one drawing file
/// (IM11): layer rows set in the cut technology panel, and each part's
/// contour choices in the order they were painted.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct DrawingChoices {
    /// Layer rows set by hand; the others keep the role they start with.
    pub roles: BTreeMap<String, dxf::LayerRole>,
    /// Each part's contour choices, by its place in the file.
    pub contours: BTreeMap<usize, Vec<ContourChoice>>,
}

impl DrawingChoices {
    /// The layer roles for the file's parts `reads`: the ones they start
    /// with, as `import-drawing` gives them (D5, DX3), with the rows set here.
    pub fn roles(&self, reads: &[crate::formats::Incoming]) -> BTreeMap<String, dxf::LayerRole> {
        let mut roles = crate::formats::default_roles(reads.iter().map(|r| &r.part));
        for (l, r) in &self.roles {
            if let Some(role) = roles.get_mut(l) {
                *role = *r;
            }
        }
        roles
    }

    /// Part `ix`'s contour choices.
    pub fn of(&self, ix: usize) -> &[ContourChoice] {
        self.contours.get(&ix).map_or(&[], Vec::as_slice)
    }
}

/// Whether two contours are the same shape in the same place.
fn same_place(a: &dxf::Contour, b: &dxf::Contour) -> bool {
    let (ba, bb) = (crate::nest::bbox_of(&a.pts), crate::nest::bbox_of(&b.pts));
    contours::same_shape(a, b) && (ba[0] - bb[0]).abs() <= dxf::JOIN_TOL && (ba[1] - bb[1]).abs() <= dxf::JOIN_TOL
}

/// The contours painted by hand on `painted` (made from `start`), as choices
/// for `now`, the part the drawing makes after a layer row changed: each one
/// `now` still has in the same place, starting as it did, keeps its paint;
/// one the change left out, or made start as something else (it is on that
/// layer), takes the layer's. Also says how many gave way.
fn carry(start: &Part, painted: &Part, now: &Part) -> (Vec<ContourChoice>, usize) {
    let (s, p, n) = (contours::list(start), contours::list(painted), contours::list(now));
    let mut part = now.clone();
    let (mut out, mut gave_way) = (vec![], 0);
    for (k, (sk, pk)) in s.iter().zip(&p).enumerate() {
        if (sk.state, sk.quality) == (pk.state, pk.quality) {
            continue;
        }
        let paint = if pk.state == dxf::ContourState::Cut { contours::Paint::cut_at(pk.quality) } else { contours::Paint::state(pk.state) };
        let Some(j) = n.iter().position(|c| c.state == s[k].state && same_place(c, pk)) else {
            gave_way += 1;
            continue;
        };
        let mut parts = vec![part.clone()];
        match contours::paint(&mut parts, &[], &mut [], 0, &[j], paint, false) {
            Ok(_) => {
                part = parts.swap_remove(0);
                out.push(ContourChoice { contours: vec![j], paint, same: false });
            }
            Err(_) => gave_way += 1,
        }
    }
    (out, gave_way)
}

/// A layer row set to `role` in the import preview's panel (IM11): the
/// file's parts are made again with it, so every contour on that layer takes
/// it (D5, CS2). A contour painted by hand that the change leaves in place,
/// starting as before, keeps its paint; one on that layer takes the layer's.
/// The new choices, and how many contours painted by hand gave way.
pub fn set_layer(cfg: &AppSettings, spec: &SheetSpec, reads: &[crate::formats::Incoming], choices: &DrawingChoices, layer: &str, role: dxf::LayerRole) -> (DrawingChoices, usize) {
    let before = choices.roles(reads);
    let mut next = choices.clone();
    next.roles.insert(layer.to_string(), role);
    let after = next.roles(reads);
    let mut gave_way = 0;
    for (&ix, list) in &choices.contours {
        let Some(read) = reads.get(ix) else { continue };
        let made = |roles: &BTreeMap<String, dxf::LayerRole>, ch: &[ContourChoice]| chosen_part(cfg, spec, read, roles, ch, 0).ok().map(|r| r.0);
        let (kept, lost) = match (made(&before, &[]), made(&before, list), made(&after, &[])) {
            (Some(start), Some(painted), Some(now)) => carry(&start, &painted, &now),
            _ => (vec![], list.iter().map(|c| c.contours.len()).sum()),
        };
        gave_way += lost;
        if kept.is_empty() {
            next.contours.remove(&ix);
        } else {
            next.contours.insert(ix, kept);
        }
    }
    (next, gave_way)
}
