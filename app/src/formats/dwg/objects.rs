//! Reading single objects (entities, layers, block definitions) from the object data
//! (DW5). Each object is parsed on its own; a damaged object is counted and left
//! out, it never stops the rest of the drawing.

use super::bits::{tv, Bits, Href, Ver, BR};
use super::container::Class;
use std::collections::HashMap;

pub type V3 = [f64; 3];

#[derive(Debug, Clone)]
pub enum Kind {
    Line { a: V3, b: V3 },
    Circle { c: V3, r: f64, ext: V3 },
    Arc { c: V3, r: f64, a0: f64, a1: f64, ext: V3 },
    LwPoly { closed: bool, elev: f64, ext: V3, verts: Vec<([f64; 2], f64)> },
    /// 2D (`three_d` false, OCS) or 3D polyline (WCS); its vertices are separate objects.
    Poly { three_d: bool, closed: bool, spline_fit: bool, elev: f64, ext: V3, owned: Vec<u64>, first: Option<u64>, last: Option<u64> },
    Vertex { flags: u8, pt: V3, bulge: f64 },
    Ellipse { c: V3, major: V3, ext: V3, ratio: f64, t0: f64, t1: f64 },
    Spline(Spline),
    Text { pt: V3, ext: V3, rot: f64, h: f64, s: String },
    MText { pt: V3, xdir: V3, h: f64, s: String },
    Insert { block: u64, pt: V3, scale: V3, rot: f64, ext: V3, cols: u16, rows: u16, dc: f64, dr: f64 },
    Layer { name: String },
    BlockHeader { name: String, base: V3 },
    /// An entity type that is read past but not turned into geometry.
    Skip { name: String, annotation: bool },
    /// Non-graphic object that is not needed.
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct Spline {
    pub degree: usize,
    pub closed: bool,
    pub knots: Vec<f64>,
    pub ctrl: Vec<V3>,
    pub weights: Vec<f64>,
    pub fit: Vec<V3>,
    pub beg_tan: V3,
    pub end_tan: V3,
}

#[derive(Debug, Clone)]
pub struct Obj {
    pub handle: u64,
    pub kind: Kind,
    /// Entity mode: 0 = owned by a block / polyline, 1 = paper space, 2 = model space.
    pub entmode: u8,
    pub owner: Option<u64>,
    pub layer: Option<u64>,
    pub invisible: bool,
    pub next: Option<u64>,
}

/// Find the string stream of an R2007+ object whose data ends just after bit `last`.
/// The number of vertices a polyline says it owns. Each takes at least a byte
/// of the object, so a count the object can't hold is a damaged object (DW5);
/// a real but huge polyline goes on to the point limit (refused whole, DW15).
fn owned_count(n: usize, r: &Bits) -> BR<usize> {
    if n > r.left() / 8 { Err(super::bits::Eof) } else { Ok(n) }
}

pub fn string_stream(data: &[u8], last: usize, ver: Ver) -> Option<Bits<'_>> {
    let mut r = Bits::new(data, ver);
    r.pos = last;
    if r.b().ok()? == 0 {
        return None;
    }
    let mut p = last.checked_sub(16)?;
    r.pos = p;
    let mut size = r.rs().ok()? as usize;
    if size & 0x8000 != 0 {
        p = p.checked_sub(16)?;
        r.pos = p;
        let hi = r.rs().ok()? as usize;
        size = (size & 0x7fff) | (hi << 15);
    }
    let start = p.checked_sub(size)?;
    let mut s = Bits::new(data, ver);
    s.pos = start;
    s.end = p;
    Some(s)
}

fn skip_eed(r: &mut Bits) -> BR<()> {
    loop {
        let n = r.bs()? as usize;
        if n == 0 {
            return Ok(());
        }
        r.h()?;
        r.skip(n * 8)?;
    }
}

/// Names of fixed entity types that are not geometry for a part.
fn fixed_name(t: u16) -> Option<(&'static str, bool)> {
    Some(match t {
        2 => ("ATTRIB", true),
        3 => ("ATTDEF", true),
        4 => ("BLOCK", true),
        5 => ("ENDBLK", true),
        6 => ("SEQEND", true),
        12..=14 => ("VERTEX", true),
        20..=26 => ("DIMENSION", true),
        27 => ("POINT", true),
        28 => ("3DFACE", false),
        29 | 30 => ("POLYLINE", false),
        31 => ("SOLID", false),
        32 => ("TRACE", false),
        33 => ("SHAPE", false),
        34 => ("VIEWPORT", true),
        37 => ("REGION", false),
        38 => ("3DSOLID", false),
        39 => ("BODY", false),
        40 => ("RAY", false),
        41 => ("XLINE", false),
        45 => ("LEADER", true),
        46 => ("TOLERANCE", true),
        47 => ("MLINE", false),
        78 => ("HATCH", true),
        0x1f2 => ("ACAD_PROXY_ENTITY", false),
        _ => return None,
    })
}

/// Entity classes that are annotations (left out without a note, like DIMENSION in DXF).
fn annotation_class(name: &str) -> bool {
    matches!(
        name,
        "HATCH" | "MLEADER" | "MULTILEADER" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" | "WIPEOUT" | "IMAGE" | "ACAD_TABLE" | "OLE2FRAME"
    )
}

pub struct Reader<'a> {
    pub buf: &'a [u8],
    pub ver: Ver,
    pub classes: HashMap<u16, Class>,
}

struct Common {
    entmode: u8,
    reactors: u32,
    xdic: bool,
    bylayer_lt: bool,
    nolinks: bool,
    color_book: bool,
    lt_flags: u8,
    ps_flags: u8,
    mat_flags: u8,
    vis: [bool; 3],
    invisible: bool,
}

impl<'a> Reader<'a> {
    pub fn read(&self, off: usize) -> BR<Obj> {
        let ver = self.ver;
        let mut r = Bits::at(self.buf, off, ver);
        let size = r.ms()? as usize;
        // From R2010 the size of the handle data follows; the object size counts from
        // after it.
        let hsize = if ver >= Ver::R2010 { r.mcu()? as usize } else { 0 };
        let start = r.pos;
        let end = start.checked_add(size.checked_mul(8).ok_or(super::bits::Eof)?).ok_or(super::bits::Eof)?;
        if end > self.buf.len() * 8 {
            return Err(super::bits::Eof);
        }
        r.end = end;
        let mut hstart = 0usize;
        if ver >= Ver::R2010 {
            hstart = end.checked_sub(hsize).ok_or(super::bits::Eof)?;
        }
        let typ = r.ot()?;
        if (Ver::R2000..Ver::R2010).contains(&ver) {
            hstart = start + r.rl()? as usize;
        }
        let handle = r.h()?.val;
        skip_eed(&mut r)?;

        // What the type is.
        let class = if typ >= 500 { self.classes.get(&typ) } else { None };
        let is_entity = match typ {
            1..=41 | 43..=47 | 77 | 78 | 0x1f2 => true,
            t if t >= 500 => class.is_some_and(|c| c.is_entity),
            _ => false,
        };
        let lw = class.is_some_and(|c| c.dxf_name == "LWPOLYLINE") || typ == 77;
        if !is_entity && !lw && typ != 49 && typ != 51 {
            return Ok(Obj { handle, kind: Kind::Other, entmode: 0, owner: None, layer: None, invisible: false, next: None });
        }

        if !is_entity && !lw {
            // Table entries: layer and block definition.
            if ver <= Ver::R14 {
                hstart = start + r.rl()? as usize;
            }
            r.bl()?;
            if ver >= Ver::R2004 {
                r.b()?;
            }
            if ver >= Ver::R2013 {
                r.b()?;
            }
            let mut strs = if ver >= Ver::R2007 { string_stream(self.buf, hstart.checked_sub(1).ok_or(super::bits::Eof)?, ver) } else { None };
            let name = tv(&mut r, &mut strs)?;
            let kind = if typ == 51 {
                Kind::Layer { name }
            } else {
                r.b()?;
                // The xref index is gone from R2007 on.
                if ver < Ver::R2007 {
                    r.bs()?;
                }
                for _ in 0..5 {
                    r.b()?;
                }
                if ver >= Ver::R2000 {
                    r.b()?;
                }
                if ver >= Ver::R2004 {
                    r.bl()?;
                }
                let base = r.bd3()?;
                Kind::BlockHeader { name, base }
            };
            return Ok(Obj { handle, kind, entmode: 0, owner: None, layer: None, invisible: false, next: None });
        }

        // Entities: graphic image, then the common entity data.
        if r.b()? == 1 {
            let n = if ver >= Ver::R2010 { r.bll()? } else { r.rl()? as u64 };
            r.skip(usize::try_from(n).ok().and_then(|n| n.checked_mul(8)).ok_or(super::bits::Eof)?)?;
        }
        if ver <= Ver::R14 {
            hstart = start + r.rl()? as usize;
        }
        let c = self.common(&mut r)?;
        let mut strs = if ver >= Ver::R2007 { string_stream(self.buf, hstart.checked_sub(1).ok_or(super::bits::Eof)?, ver) } else { None };

        let kind = if lw { Some(self.lwpoly(&mut r)?) } else { self.entity(typ, &mut r, &mut strs)? };
        let kind = match kind {
            Some(k) => k,
            None => {
                let (name, annotation) = match class {
                    Some(c) => (c.dxf_name.clone(), annotation_class(&c.dxf_name)),
                    None => fixed_name(typ).map_or((format!("type {typ}"), false), |(n, a)| (n.to_string(), a)),
                };
                Kind::Skip { name, annotation }
            }
        };

        // Handle stream: owner, reactors, dictionary, layer, links.
        let mut h = Bits::new(self.buf, ver);
        h.pos = hstart;
        h.end = end;
        let hr = |h: &mut Bits| -> BR<Href> { h.h() };
        let owner = if c.entmode == 0 { Some(hr(&mut h)?.abs(handle)) } else { None };
        for _ in 0..c.reactors.min(10_000) {
            hr(&mut h)?;
        }
        if c.xdic {
            hr(&mut h)?;
        }
        let mut layer = None;
        let mut next = None;
        if ver <= Ver::R14 {
            layer = Some(hr(&mut h)?.abs(handle));
            if !c.bylayer_lt {
                hr(&mut h)?;
            }
        }
        if ver <= Ver::R2000 && !c.nolinks {
            hr(&mut h)?;
            next = Some(hr(&mut h)?.abs(handle));
        } else if ver <= Ver::R2000 {
            next = Some(handle + 1);
        }
        if ver >= Ver::R2004 && c.color_book {
            hr(&mut h)?;
        }
        if ver >= Ver::R2000 {
            layer = Some(hr(&mut h)?.abs(handle));
            if c.lt_flags == 3 {
                hr(&mut h)?;
            }
        }
        if ver >= Ver::R2007 && c.mat_flags == 3 {
            hr(&mut h)?;
        }
        if ver >= Ver::R2000 && c.ps_flags == 3 {
            hr(&mut h)?;
        }
        if ver >= Ver::R2010 {
            for v in c.vis {
                if v {
                    hr(&mut h)?;
                }
            }
        }
        // Entity-specific handles.
        let kind = match kind {
            Kind::Insert { pt, scale, rot, ext, cols, rows, dc, dr, .. } => {
                let block = hr(&mut h)?.abs(handle);
                Kind::Insert { block, pt, scale, rot, ext, cols, rows, dc, dr }
            }
            Kind::Poly { three_d, closed, spline_fit, elev, ext, owned, .. } => {
                let mut first = None;
                let mut last = None;
                let mut list = vec![];
                // DW5: a vertex list cut short is a damaged object, not a polyline
                // with fewer corners.
                if ver <= Ver::R2000 {
                    first = Some(h.h()?.abs(handle));
                    last = Some(h.h()?.abs(handle));
                } else {
                    for _ in 0..owned.len() {
                        list.push(h.h()?.abs(handle));
                    }
                }
                Kind::Poly { three_d, closed, spline_fit, elev, ext, owned: list, first, last }
            }
            k => k,
        };
        Ok(Obj { handle, kind, entmode: c.entmode, owner, layer, invisible: c.invisible, next })
    }

    fn common(&self, r: &mut Bits) -> BR<Common> {
        let ver = self.ver;
        let entmode = r.bb()?;
        let reactors = r.bl()?;
        let mut xdic = true;
        if ver >= Ver::R2004 {
            xdic = r.b()? == 0;
        }
        if ver >= Ver::R2013 {
            r.b()?;
        }
        let mut bylayer_lt = true;
        if ver <= Ver::R14 {
            bylayer_lt = r.b()? == 1;
        }
        let mut nolinks = true;
        if ver <= Ver::R2000 {
            nolinks = r.b()? == 1;
        }
        let mut color_book = false;
        if ver >= Ver::R2004 {
            color_book = r.cmc()? & 0x40 != 0;
        } else {
            r.bs()?;
        }
        r.bd()?;
        let (mut lt_flags, mut ps_flags, mut mat_flags) = (0, 0, 0);
        if ver >= Ver::R2000 {
            lt_flags = r.bb()?;
            ps_flags = r.bb()?;
        }
        if ver >= Ver::R2007 {
            mat_flags = r.bb()?;
            r.rc()?;
        }
        let mut vis = [false; 3];
        if ver >= Ver::R2010 {
            for v in &mut vis {
                *v = r.b()? == 1;
            }
        }
        let invisible = r.bs()? & 1 == 1;
        if ver >= Ver::R2000 {
            r.rc()?;
        }
        Ok(Common { entmode, reactors, xdic, bylayer_lt, nolinks, color_book, lt_flags, ps_flags, mat_flags, vis, invisible })
    }

    fn lwpoly(&self, r: &mut Bits) -> BR<Kind> {
        let flag = r.bs()?;
        if flag & 4 != 0 {
            r.bd()?;
        }
        let elev = if flag & 8 != 0 { r.bd()? } else { 0.0 };
        if flag & 2 != 0 {
            r.bd()?;
        }
        let ext = if flag & 1 != 0 { r.bd3()? } else { [0.0, 0.0, 1.0] };
        let n = r.bl()? as usize;
        let nb = if flag & 16 != 0 { r.bl()? as usize } else { 0 };
        let nid = if self.ver >= Ver::R2010 && flag & 1024 != 0 { r.bl()? as usize } else { 0 };
        let nw = if flag & 32 != 0 { r.bl()? as usize } else { 0 };
        if n > r.left() || nb > r.left() || nid > r.left() || nw > r.left() {
            return Err(super::bits::Eof);
        }
        let mut pts: Vec<[f64; 2]> = Vec::with_capacity(n);
        for i in 0..n {
            if self.ver <= Ver::R14 || i == 0 {
                pts.push(r.rd2()?);
            } else {
                let p = pts[i - 1];
                pts.push([r.dd(p[0])?, r.dd(p[1])?]);
            }
        }
        let mut bulges = vec![0.0; n];
        for i in 0..nb {
            let b = r.bd()?;
            if i < n {
                bulges[i] = b;
            }
        }
        let verts = pts.into_iter().zip(bulges).collect();
        Ok(Kind::LwPoly { closed: flag & 512 != 0, elev, ext, verts })
    }

    fn entity(&self, typ: u16, r: &mut Bits<'a>, strs: &mut Option<Bits<'a>>) -> BR<Option<Kind>> {
        let ver = self.ver;
        Ok(Some(match typ {
            1 => {
                // TEXT
                let (pt, ext, rot, h, s);
                if ver <= Ver::R14 {
                    let elev = r.bd()?;
                    let p = r.rd2()?;
                    r.rd2()?;
                    ext = r.bd3()?;
                    r.bd()?;
                    r.bd()?;
                    rot = r.bd()?;
                    h = r.bd()?;
                    r.bd()?;
                    s = tv(r, strs)?;
                    pt = [p[0], p[1], elev];
                } else {
                    let df = r.rc()?;
                    let elev = if df & 1 == 0 { r.rd()? } else { 0.0 };
                    let p = r.rd2()?;
                    if df & 2 == 0 {
                        r.dd(p[0])?;
                        r.dd(p[1])?;
                    }
                    ext = r.be()?;
                    r.bt()?;
                    if df & 4 == 0 {
                        r.rd()?;
                    }
                    rot = if df & 8 == 0 { r.rd()? } else { 0.0 };
                    h = r.rd()?;
                    if df & 0x10 == 0 {
                        r.rd()?;
                    }
                    s = tv(r, strs)?;
                    pt = [p[0], p[1], elev];
                }
                Kind::Text { pt, ext, rot, h, s }
            }
            7 | 8 => {
                let pt = r.bd3()?;
                let scale = if ver <= Ver::R14 {
                    r.bd3()?
                } else {
                    match r.bb()? {
                        3 => [1.0, 1.0, 1.0],
                        1 => [1.0, r.dd(1.0)?, r.dd(1.0)?],
                        2 => {
                            let x = r.rd()?;
                            [x, x, x]
                        }
                        _ => {
                            let x = r.rd()?;
                            [x, r.dd(x)?, r.dd(x)?]
                        }
                    }
                };
                let rot = r.bd()?;
                let ext = r.bd3()?;
                r.b()?;
                if ver >= Ver::R2004 {
                    r.bl()?;
                }
                let (mut cols, mut rows, mut dc, mut dr) = (1, 1, 0.0, 0.0);
                if typ == 8 {
                    cols = r.bs()?;
                    rows = r.bs()?;
                    dc = r.bd()?;
                    dr = r.bd()?;
                }
                Kind::Insert { block: 0, pt, scale, rot, ext, cols, rows, dc, dr }
            }
            10 => {
                let flags = r.rc()?;
                let pt = r.bd3()?;
                let sw = r.bd()?;
                if sw >= 0.0 {
                    r.bd()?;
                }
                let bulge = r.bd()?;
                Kind::Vertex { flags, pt, bulge }
            }
            11 => {
                let flags = r.rc()?;
                let pt = r.bd3()?;
                Kind::Vertex { flags, pt, bulge: 0.0 }
            }
            15 => {
                let flags = r.bs()?;
                r.bs()?;
                r.bd()?;
                r.bd()?;
                r.bt()?;
                let elev = r.bd()?;
                let ext = r.be()?;
                let n = if ver >= Ver::R2004 { r.bl()? as usize } else { 0 };
                Kind::Poly { three_d: false, closed: flags & 1 != 0, spline_fit: flags & 4 != 0, elev, ext, owned: vec![0; owned_count(n, r)?], first: None, last: None }
            }
            16 => {
                let f1 = r.rc()?;
                let f2 = r.rc()?;
                let n = if ver >= Ver::R2004 { r.bl()? as usize } else { 0 };
                Kind::Poly { three_d: true, closed: f2 & 1 != 0, spline_fit: f1 & 3 != 0, elev: 0.0, ext: [0.0, 0.0, 1.0], owned: vec![0; owned_count(n, r)?], first: None, last: None }
            }
            17 => {
                let c = r.bd3()?;
                let rad = r.bd()?;
                r.bt()?;
                let ext = r.be()?;
                let a0 = r.bd()?;
                let a1 = r.bd()?;
                Kind::Arc { c, r: rad, a0, a1, ext }
            }
            18 => {
                let c = r.bd3()?;
                let rad = r.bd()?;
                r.bt()?;
                let ext = r.be()?;
                Kind::Circle { c, r: rad, ext }
            }
            19 => {
                let (a, b);
                if ver <= Ver::R14 {
                    a = r.bd3()?;
                    b = r.bd3()?;
                } else {
                    let zz = r.b()? == 1;
                    let x0 = r.rd()?;
                    let x1 = r.dd(x0)?;
                    let y0 = r.rd()?;
                    let y1 = r.dd(y0)?;
                    let (mut z0, mut z1) = (0.0, 0.0);
                    if !zz {
                        z0 = r.rd()?;
                        z1 = r.dd(z0)?;
                    }
                    a = [x0, y0, z0];
                    b = [x1, y1, z1];
                }
                Kind::Line { a, b }
            }
            35 => Kind::Ellipse { c: r.bd3()?, major: r.bd3()?, ext: r.bd3()?, ratio: r.bd()?, t0: r.bd()?, t1: r.bd()? },
            36 => Kind::Spline(self.spline(r)?),
            44 => {
                let pt = r.bd3()?;
                r.bd3()?;
                let xdir = r.bd3()?;
                r.bd()?;
                if ver >= Ver::R2007 {
                    r.bd()?;
                }
                let h = r.bd()?;
                r.bs()?;
                r.bs()?;
                r.bd()?;
                r.bd()?;
                let s = tv(r, strs)?;
                Kind::MText { pt, xdir, h, s }
            }
            _ => return Ok(None),
        }))
    }

    fn spline(&self, r: &mut Bits) -> BR<Spline> {
        let mut scenario = r.bl()?;
        if self.ver >= Ver::R2013 {
            let flags = r.bl()?;
            let knot_param = r.bl()?;
            if flags & 1 != 0 {
                scenario = 2;
            }
            if knot_param == 15 {
                scenario = 1;
            }
        }
        let degree = r.bl()? as usize;
        let mut s = Spline { degree: degree.clamp(1, 25), ..Default::default() };
        let left = r.left();
        let mut nfit = 0;
        let (mut nknots, mut nctrl, mut weighted) = (0, 0, false);
        if scenario == 2 {
            r.bd()?;
            s.beg_tan = r.bd3()?;
            s.end_tan = r.bd3()?;
            nfit = r.bl()? as usize;
        } else if scenario == 1 {
            r.b()?;
            s.closed = r.b()? == 1;
            let periodic = r.b()? == 1;
            s.closed |= periodic;
            r.bd()?;
            r.bd()?;
            nknots = r.bl()? as usize;
            nctrl = r.bl()? as usize;
            weighted = r.b()? == 1;
        } else {
            return Err(super::bits::Eof);
        }
        if nknots > left || nctrl > left || nfit > left {
            return Err(super::bits::Eof);
        }
        for _ in 0..nknots {
            s.knots.push(r.bd()?);
        }
        for _ in 0..nctrl {
            s.ctrl.push(r.bd3()?);
            s.weights.push(if weighted { r.bd()? } else { 1.0 });
        }
        for _ in 0..nfit {
            s.fit.push(r.bd3()?);
        }
        Ok(s)
    }
}
