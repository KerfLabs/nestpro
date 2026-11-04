//! Drawing a sheet or a part without a window, to PNG (tiny-skia) or SVG, so
//! an agent can look at its work: parts with their numbers, cut path, leads,
//! micro-joints, cut order, cut/remnant lines; for a part, its numbered
//! elements (the numbers `lead-at` and `joint-at` take).

use super::*;
use std::fmt::Write as _;

type Rgba = [u8; 4];

enum Prim {
    /// Filled area; several rings with the even-odd rule (outline + holes).
    Fill { rings: Vec<Vec<P>>, color: Rgba },
    /// A line in mm; `px` wide on the image.
    Line { pts: Vec<P>, closed: bool, color: Rgba, px: f32, dash: bool },
    Dot { at: P, px: f32, color: Rgba },
}

/// A drawing in mm, Y up.
pub struct Scene {
    min: P,
    max: P,
    prims: Vec<Prim>,
    /// Pixels per mm, fixed when the image size is chosen (texts need it).
    scale: f64,
    pad: f64,
}

const BG: Rgba = [255, 255, 255, 255];
const SHEET: Rgba = [226, 232, 240, 255];
const EDGE: Rgba = [71, 85, 105, 255];
const INK: Rgba = [15, 23, 42, 255];
const LEAD: Rgba = [22, 163, 74, 255];
const CUT: Rgba = [220, 38, 38, 255];
const RAPID: Rgba = [37, 99, 235, 160];
const JOINT: Rgba = [234, 179, 8, 255];
const CUTLINE: Rgba = [192, 38, 211, 255];
const REMNANT: Rgba = [134, 239, 172, 120];
const MARK: Rgba = [8, 145, 178, 255];
const ELEM: Rgba = [29, 78, 216, 255];

fn part_rgba(color: usize, a: u8) -> Rgba {
    let (r, g, b) = crate::ui_parts::PALETTE[color % crate::ui_parts::PALETTE.len()];
    [r, g, b, a]
}

impl Scene {
    fn new(min: P, max: P, width_px: u32) -> Scene {
        let pad = 24.0;
        let w = (max[0] - min[0]).max(1.0);
        let scale = (width_px as f64 - 2.0 * pad).max(50.0) / w;
        Scene { min, max, prims: vec![], scale, pad }
    }

    fn size_px(&self) -> (u32, u32) {
        let w = (self.max[0] - self.min[0]) * self.scale + 2.0 * self.pad;
        let h = (self.max[1] - self.min[1]) * self.scale + 2.0 * self.pad;
        (w.ceil().max(16.0) as u32, h.ceil().max(16.0) as u32)
    }

    fn to_px(&self, p: P) -> (f32, f32) {
        (((p[0] - self.min[0]) * self.scale + self.pad) as f32, ((self.max[1] - p[1]) * self.scale + self.pad) as f32)
    }

    fn fill(&mut self, rings: Vec<Vec<P>>, color: Rgba) {
        self.prims.push(Prim::Fill { rings, color });
    }
    fn line(&mut self, pts: Vec<P>, closed: bool, color: Rgba, px: f32) {
        self.prims.push(Prim::Line { pts, closed, color, px, dash: false });
    }
    fn dashed(&mut self, pts: Vec<P>, color: Rgba, px: f32) {
        self.prims.push(Prim::Line { pts, closed: false, color, px, dash: true });
    }
    fn dot(&mut self, at: P, px: f32, color: Rgba) {
        self.prims.push(Prim::Dot { at, px, color });
    }

    /// Text centred on `at`, `px` pixels high (the app's stroke font).
    fn label(&mut self, text: &str, at: P, px: f64, color: Rgba) {
        let h = px / self.scale;
        let w = crate::font::width(text, h);
        let start = [at[0] - w / 2.0, at[1] - h / 2.0];
        let lw = (px / 7.0).clamp(1.0, 3.0) as f32;
        // A white halo first so numbers read on top of the parts.
        for s in crate::font::strokes(text, start, h, 0.0) {
            self.line(s, false, [255, 255, 255, 220], lw + 2.5);
        }
        for s in crate::font::strokes(text, start, h, 0.0) {
            self.line(s, false, color, lw);
        }
    }

    pub fn png(&self) -> Result<Vec<u8>, String> {
        use tiny_skia::*;
        let (w, h) = self.size_px();
        let mut pm = Pixmap::new(w, h).ok_or("image too large")?;
        pm.fill(Color::from_rgba8(BG[0], BG[1], BG[2], BG[3]));
        let paint = |c: Rgba| {
            let mut p = Paint::default();
            p.set_color_rgba8(c[0], c[1], c[2], c[3]);
            p.anti_alias = true;
            p
        };
        let path_of = |pts: &[P], closed: bool| -> Option<tiny_skia::Path> {
            let mut pb = PathBuilder::new();
            for (i, &q) in pts.iter().enumerate() {
                let (x, y) = self.to_px(q);
                if i == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
            }
            if closed {
                pb.close();
            }
            pb.finish()
        };
        for pr in &self.prims {
            match pr {
                Prim::Fill { rings, color } => {
                    let mut pb = PathBuilder::new();
                    for r in rings {
                        for (i, &q) in r.iter().enumerate() {
                            let (x, y) = self.to_px(q);
                            if i == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
                        }
                        pb.close();
                    }
                    if let Some(p) = pb.finish() {
                        pm.fill_path(&p, &paint(*color), FillRule::EvenOdd, Transform::identity(), None);
                    }
                }
                Prim::Line { pts, closed, color, px, dash } => {
                    if pts.len() < 2 {
                        continue;
                    }
                    if let Some(p) = path_of(pts, *closed) {
                        let mut st = Stroke { width: *px, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
                        if *dash {
                            st.dash = StrokeDash::new(vec![6.0, 5.0], 0.0);
                        }
                        pm.stroke_path(&p, &paint(*color), &st, Transform::identity(), None);
                    }
                }
                Prim::Dot { at, px, color } => {
                    let (x, y) = self.to_px(*at);
                    if let Some(p) = PathBuilder::from_circle(x, y, *px) {
                        pm.fill_path(&p, &paint(*color), FillRule::Winding, Transform::identity(), None);
                    }
                }
            }
        }
        pm.encode_png().map_err(|e| e.to_string())
    }

    pub fn svg(&self) -> String {
        let (w, h) = self.size_px();
        // `attr="rgb(…)" attr-opacity="…"`
        let col = |attr: &str, c: Rgba| format!("{attr}=\"rgb({},{},{})\" {attr}-opacity=\"{:.3}\"", c[0], c[1], c[2], c[3] as f64 / 255.0);
        let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n");
        let d = |pts: &[P], closed: bool| {
            let mut t = String::new();
            for (i, &q) in pts.iter().enumerate() {
                let (x, y) = self.to_px(q);
                let _ = write!(t, "{}{x:.1},{y:.1} ", if i == 0 { "M" } else { "L" });
            }
            if closed {
                t.push('Z');
            }
            t
        };
        for pr in &self.prims {
            match pr {
                Prim::Fill { rings, color } => {
                    let path: String = rings.iter().map(|r| d(r, true)).collect();
                    let _ = writeln!(s, "<path d=\"{path}\" fill-rule=\"evenodd\" {}/>", col("fill", *color));
                }
                Prim::Line { pts, closed, color, px, dash } => {
                    let _ = writeln!(
                        s,
                        "<path d=\"{}\" fill=\"none\" {} stroke-width=\"{px}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{}/>",
                        d(pts, *closed),
                        col("stroke", *color),
                        if *dash { " stroke-dasharray=\"6 5\"" } else { "" }
                    );
                }
                Prim::Dot { at, px, color } => {
                    let (x, y) = self.to_px(*at);
                    let _ = writeln!(s, "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{px}\" {}/>", col("fill", *color));
                }
            }
        }
        s.push_str("</svg>\n");
        s
    }
}

/// Cut path of a plan: lead-ins, contours, joints, rapids between them, and
/// (when there aren't too many) the cut order next to each pierce.
fn draw_plan(sc: &mut Scene, plan: &SheetPlan, numbers: bool) {
    let mut at: Option<P> = None;
    let order = numbers && plan.contours.len() <= 400;
    for (k, c) in plan.contours.iter().enumerate() {
        if let Some(prev) = at {
            sc.dashed(vec![prev, c.lead_in], RAPID, 1.0);
        }
        if c.punch.is_none() {
            sc.line(c.points.clone(), true, CUT, 1.2);
        }
        if c.lead_path.len() >= 2 {
            sc.line(c.lead_path.clone(), false, LEAD, 2.0);
        }
        sc.dot(c.lead_in, 2.5, LEAD);
        for (a, b) in &c.joints {
            let m = cnc::point_at(&c.points, (a + b) / 2.0);
            sc.dot(m, 3.5, JOINT);
        }
        for (o, i) in &c.joint_leads {
            sc.line(o.clone(), false, LEAD, 1.2);
            sc.line(i.clone(), false, LEAD, 1.2);
        }
        if order {
            let p = c.lead_in;
            sc.label(&(k + 1).to_string(), [p[0], p[1] + 9.0 / sc.scale], 9.0, LEAD);
        }
        at = Some(c.lead_out.unwrap_or(c.points[0]));
    }
    for (_, m) in &plan.marks {
        sc.line(m.clone(), false, MARK, 1.2);
    }
}

/// One sheet: parts filled in their colours with #numbers, cut lines, the
/// remnant shaded, and (with `path`) the cut path.
pub fn sheet_scene(job: &JobFile, k: usize, width_px: u32, path: bool) -> Result<Scene, String> {
    let sheet = job.sheets.get(k).ok_or_else(|| format!("there is no sheet {} (the job has {})", k + 1, job.sheets.len()))?;
    let f = job.format(k);
    let spec = &job.spec_at(k);
    let mut sc = Scene::new([0.0, 0.0], [spec.w, spec.h], width_px);
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    // A remnant sheet shows its own shape; the rest of its box stays blank.
    let outline = crate::remnant::sheet_outline(&f);
    let mut rings = vec![outline.outer.clone()];
    rings.extend(outline.holes.iter().cloned());
    sc.fill(rings, SHEET);
    for r in super::remnant_pieces(&job.parts, sheet, &f, job.cuts(k)) {
        let mut rings = vec![r.outer];
        rings.extend(r.holes);
        sc.fill(rings, REMNANT);
    }
    for ring in std::iter::once(&outline.outer).chain(outline.holes.iter()) {
        sc.line(ring.clone(), true, EDGE, 1.5);
    }
    if spec.margin > 0.0 && f.outline.is_none() {
        let m = spec.margin;
        let mut r = rect(m, m, spec.w - m, spec.h - m);
        r.push([m, m]);
        sc.dashed(r, [148, 163, 184, 255], 0.8);
    }
    for pl in sheet {
        let (o, holes) = nest::placed_shape(&job.parts, pl);
        let mut rings = vec![o.clone()];
        rings.extend(holes.iter().cloned());
        sc.fill(rings, part_rgba(job.parts[pl.part].color, 200));
        sc.line(o, true, INK, 1.0);
        for h in holes {
            sc.line(h, true, INK, 0.8);
        }
    }
    if path {
        let plans = job.plans();
        draw_plan(&mut sc, &plans[k], true);
    }
    for (i, pl) in sheet.iter().enumerate() {
        let px = (pl.w.min(pl.h) * sc.scale * 0.35).clamp(10.0, 22.0);
        sc.label(&format!("#{}", i + 1), [pl.x + pl.w / 2.0, pl.y + pl.h / 2.0], px, INK);
    }
    for l in job.cuts(k) {
        sc.line(l.pts.clone(), false, CUTLINE, 2.5);
    }
    if !f.stock_ref.is_empty() {
        let what = if f.remnant { "remnant" } else { "stock sheet" };
        sc.label(&format!("{what} {}", f.stock_ref), [spec.w / 2.0, spec.h - 14.0 / sc.scale], 14.0, EDGE);
    }
    Ok(sc)
}

/// One part at its own origin: outline, holes, numbered elements, texts and
/// (with `path`) its leads, joints and cut order.
pub fn part_scene(job: &JobFile, i: usize, width_px: u32, path: bool) -> Scene {
    let p = &job.parts[i];
    let (w, h) = p.bbox();
    let m = (w.max(h) * 0.06).max(4.0);
    let mut sc = Scene::new([-m, -m], [w + m, h + m], width_px);
    let g = p.geometry();
    let mut rings = vec![g.outline.clone()];
    rings.extend(g.holes.iter().cloned());
    sc.fill(rings, part_rgba(p.color, 90));
    sc.line(g.outline.clone(), true, INK, 1.5);
    for hh in &g.holes {
        sc.line(hh.clone(), true, INK, 1.2);
    }
    for t in &p.texts {
        let col = if t.kind.machined() { MARK } else { [100, 116, 139, 255] };
        for s in t.strokes(&t.text) {
            sc.line(s, false, col, 1.0);
        }
    }
    if path {
        let pl = Placed { part: i, x: 0.0, y: 0.0, w, h, turns: 0, mirror: false };
        let plan = cnc::plan(&job.parts, &[pl], &job.spec);
        draw_plan(&mut sc, &plan, false);
    }
    for (k, e) in nest::elements_of(&g).iter().enumerate() {
        let mid = e.midpoint();
        sc.dot(mid, 2.5, ELEM);
        sc.label(&(k + 1).to_string(), mid, 12.0, ELEM);
    }
    sc
}

/// Write a scene as .svg or .png (by the extension); returns the PNG bytes
/// when it was a PNG.
pub fn write(sc: &Scene, out: &Path) -> Result<Option<Vec<u8>>, String> {
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    if out.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
        std::fs::write(out, sc.svg()).map_err(|e| format!("Couldn't write {}: {e}", out.display()))?;
        Ok(None)
    } else {
        let png = sc.png()?;
        std::fs::write(out, &png).map_err(|e| format!("Couldn't write {}: {e}", out.display()))?;
        Ok(Some(png))
    }
}
