use super::*;

/// Writes DWG bit codes, for building small test drawings in code.
#[derive(Default)]
struct W {
    bits: Vec<u8>,
}

impl W {
    fn b(&mut self, v: u8) -> &mut Self {
        self.bits.push(v & 1);
        self
    }
    fn n(&mut self, v: u64, n: u32) -> &mut Self {
        for i in (0..n).rev() {
            self.b(((v >> i) & 1) as u8);
        }
        self
    }
    fn rc(&mut self, v: u8) -> &mut Self {
        self.n(v as u64, 8)
    }
    fn raw(&mut self, b: &[u8]) -> &mut Self {
        for &x in b {
            self.rc(x);
        }
        self
    }
    fn rl(&mut self, v: u32) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }
    fn rd(&mut self, v: f64) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }
    fn bs(&mut self, v: u16) -> &mut Self {
        match v {
            0 => self.n(2, 2),
            256 => self.n(3, 2),
            1..=255 => self.n(1, 2).rc(v as u8),
            _ => self.n(0, 2).raw(&v.to_le_bytes()),
        }
    }
    fn bl(&mut self, v: u32) -> &mut Self {
        match v {
            0 => self.n(2, 2),
            1..=255 => self.n(1, 2).rc(v as u8),
            _ => self.n(0, 2).rl(v),
        }
    }
    fn bd(&mut self, v: f64) -> &mut Self {
        if v == 1.0 {
            self.n(1, 2)
        } else if v == 0.0 {
            self.n(2, 2)
        } else {
            self.n(0, 2).rd(v)
        }
    }
    fn h(&mut self, code: u8, v: u8) -> &mut Self {
        if v == 0 {
            self.rc(code << 4)
        } else {
            self.rc(code << 4 | 1).rc(v)
        }
    }
    fn t(&mut self, s: &str) -> &mut Self {
        self.bs(s.len() as u16).raw(s.as_bytes())
    }
    fn bytes(&self) -> Vec<u8> {
        self.bits.chunks(8).map(|c| c.iter().enumerate().fold(0u8, |a, (i, &b)| a | (b << (7 - i)))).collect()
    }
}

/// One R2000 object: `body` writes everything after the handle; `handles` the handle stream.
fn object(typ: u16, handle: u8, body: impl Fn(&mut W), handles: impl Fn(&mut W)) -> Vec<u8> {
    let mut pre = W::default();
    pre.h(0, handle);
    body(&mut pre);
    let mut hs = W::default();
    handles(&mut hs);
    // type + RL bitsize + data before the handles
    let mut head = W::default();
    head.bs(typ);
    let bitsize = head.bits.len() + 32 + pre.bits.len();
    let mut all = W::default();
    all.bs(typ).rl(bitsize as u32);
    all.bits.extend(&pre.bits);
    all.bits.extend(&hs.bits);
    let data = all.bytes();
    let mut out = vec![];
    let mut ms = W::default();
    ms.raw(&(data.len() as u16).to_le_bytes());
    out.extend(ms.bytes());
    out.extend(&data);
    out.extend([0, 0]); // CRC (not checked)
    out
}

/// Common entity data for a model-space (or block-owned) entity, R2000.
fn ent_common(w: &mut W, entmode: u8) {
    w.bs(0).b(0); // no EED, no graphic
    w.n(entmode as u64, 2).bl(0).b(1).bs(256).bd(1.0).n(0, 2).n(0, 2).bs(0).rc(29);
}

fn ent_handles(w: &mut W, owner: Option<u8>, layer: u8) {
    if let Some(o) = owner {
        w.h(4, o);
    }
    w.h(3, 0); // xdictionary
    w.h(5, layer);
}

fn line(handle: u8, a: [f64; 2], b: [f64; 2], layer: u8, owner: Option<u8>) -> Vec<u8> {
    object(
        19,
        handle,
        |w| {
            ent_common(w, if owner.is_some() { 0 } else { 2 });
            w.b(1).rd(a[0]).n(3, 2).rd(b[0]).rd(a[1]).n(3, 2).rd(b[1]).b(1).b(1);
        },
        |w| ent_handles(w, owner, layer),
    )
}

/// A small R2000 drawing: a 100×50 plate of lines on layer CUT, a Ø20 hole made of a
/// circle with a −Z extrusion (so its centre is mirrored), and a block insert holding a
/// second hole, placed at 70,25.
fn sample_r2000() -> Vec<u8> {
    let mut objs: Vec<(u8, Vec<u8>)> = vec![];
    let layer = |h: u8, name: &str| {
        let name = name.to_string();
        object(51, h, move |w| {
            w.bs(0).bl(0).t(&name).b(0).bs(0).b(0).bs(0).bs(7);
        }, |w| {
            w.h(4, 1);
        })
    };
    objs.push((0x10, layer(0x10, "0")));
    objs.push((0x11, layer(0x11, "CUT")));
    let pts = [[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]];
    for i in 0..4 {
        objs.push((0x20 + i as u8, line(0x20 + i as u8, pts[i], pts[(i + 1) % 4], 0x11, None)));
    }
    // Circle at OCS (-25, 25) with extrusion 0,0,-1 → world (25, 25).
    objs.push((
        0x30,
        object(
            18,
            0x30,
            |w| {
                ent_common(w, 2);
                w.bd(-25.0).bd(25.0).bd(0.0).bd(10.0).b(1).b(0).bd(0.0).bd(0.0).bd(-1.0);
            },
            |w| ent_handles(w, None, 0x11),
        ),
    ));
    // Block "HOLE" (header 0x40, base 5,5) with a circle centred on its base point,
    // on layer 0 (so it takes the insert's layer).
    objs.push((
        0x40,
        object(
            49,
            0x40,
            |w| {
                w.bs(0).bl(0).t("HOLE").b(0).bs(0).b(0).b(0).b(0).b(0).b(0).b(0).bd(5.0).bd(5.0).bd(0.0);
            },
            |w| {
                w.h(4, 1);
            },
        ),
    ));
    objs.push((
        0x41,
        object(
            18,
            0x41,
            |w| {
                ent_common(w, 0);
                w.bd(5.0).bd(5.0).bd(0.0).bd(8.0).b(1).b(1);
            },
            |w| ent_handles(w, Some(0x40), 0x10),
        ),
    ));
    // Insert at 70,25, scale 1, no rotation.
    objs.push((
        0x50,
        object(
            7,
            0x50,
            |w| {
                ent_common(w, 2);
                w.bd(70.0).bd(25.0).bd(0.0).n(3, 2).bd(0.0).bd(0.0).bd(0.0).bd(1.0).b(0);
            },
            |w| {
                ent_handles(w, None, 0x11);
                w.h(5, 0x40);
            },
        ),
    ));

    // File: header with three locators, classes, objects, object map.
    let mut f = vec![0u8; 0x19 + 3 * 9 + 2 + 16];
    f[..6].copy_from_slice(b"AC1015");
    f[0x15..0x19].copy_from_slice(&3u32.to_le_bytes());
    let cls_at = f.len();
    f.extend([0x8D, 0xA1, 0xC4, 0xB8, 0xC4, 0xA9, 0xF8, 0xC5, 0xC0, 0xDC, 0xF4, 0x5F, 0xE7, 0xCF, 0xB6, 0x8A]);
    f.extend(0u32.to_le_bytes());
    f.extend([0, 0]);
    let mut locs = vec![];
    for (h, o) in &objs {
        locs.push((*h, f.len()));
        f.extend(o);
    }
    let map_at = f.len();
    let mut body = W::default();
    let (mut ph, mut pl) = (0u64, 0i64);
    for (h, l) in &locs {
        let dh = *h as u64 - ph;
        let dl = *l as i64 - pl;
        body.rc(dh as u8);
        // signed modular char, positive
        let mut v = dl as u64;
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 && byte & 0x40 == 0 {
                body.rc(byte);
                break;
            }
            body.rc(byte | 0x80);
        }
        ph = *h as u64;
        pl = *l as i64;
    }
    let b = body.bytes();
    f.extend(((b.len() + 2) as u16).to_be_bytes());
    f.extend(&b);
    f.extend([0, 0, 0, 2, 0, 0]);
    let map_len = f.len() - map_at;
    for (i, (id, at, len)) in [(0u8, 0usize, 0usize), (1, cls_at, 22), (2, map_at, map_len)].iter().enumerate() {
        let p = 0x19 + i * 9;
        f[p] = *id;
        f[p + 1..p + 5].copy_from_slice(&(*at as u32).to_le_bytes());
        f[p + 5..p + 9].copy_from_slice(&(*len as u32).to_le_bytes());
    }
    f
}

#[test]
fn reads_a_small_r2000_drawing_into_a_plate_with_two_holes() {
    let bytes = sample_r2000();
    let parts = read(&bytes, &Options { use_converter: false, ..Default::default() }).unwrap();
    assert_eq!(parts.len(), 1);
    let d = &parts[0].drawing;
    assert_eq!(d.report.entities, 6, "4 lines, a circle, a block circle");
    // The block's circle is on layer 0, so it takes the insert's layer (DW8).
    assert!(d.pieces.iter().all(|p| p.layer == "CUT"));
    let (g, _) = crate::dxf::build(d, &Default::default()).unwrap();
    assert_eq!(g.holes.len(), 2);
    assert!((g.w - 100.0).abs() < 1e-6 && (g.h - 50.0).abs() < 1e-6);
    // Read in inches (the app's unit; IU1): at size, the holes as finely split.
    let inch = read(&bytes, &Options { use_converter: false, unit_mm: 25.4, ..Default::default() }).unwrap();
    let (gi, _) = crate::dxf::build(&inch[0].drawing, &Default::default()).unwrap();
    assert!((gi.w - 2540.0).abs() < 1e-6 && (gi.h - 1270.0).abs() < 1e-6, "{} × {}", gi.w, gi.h);
    assert!(gi.holes[0].len() > g.holes[0].len(), "{} vs {} pieces", gi.holes[0].len(), g.holes[0].len());
    // −Z extrusion: the circle stored at x = −25 lands at x = +25 (DW7).
    let centre = |h: &Vec<crate::nest::P>| {
        let n = h.len() as f64;
        [h.iter().map(|p| p[0]).sum::<f64>() / n, h.iter().map(|p| p[1]).sum::<f64>() / n]
    };
    let mut cs: Vec<[f64; 2]> = g.holes.iter().map(centre).collect();
    cs.sort_by(|a, b| a[0].total_cmp(&b[0]));
    assert!((cs[0][0] - 25.0).abs() < 0.05 && (cs[0][1] - 25.0).abs() < 0.05, "{cs:?}");
    // Block circle at its base point lands on the insert point.
    assert!((cs[1][0] - 70.0).abs() < 0.05 && (cs[1][1] - 25.0).abs() < 0.05, "{cs:?}");
}

#[test]
fn blocks_can_be_left_unexploded() {
    let bytes = sample_r2000();
    let d = read_native(&bytes, &Options { explode_blocks: false, ..Default::default() }).unwrap();
    assert_eq!(d.pieces.len(), 5);
}

#[test]
fn damaged_files_give_plain_errors_never_a_panic() {
    let good = sample_r2000();
    let opts = Options { use_converter: false, ..Default::default() };
    // Cut short at every length, and flip bytes all over: never a panic.
    for n in (0..good.len()).step_by(7) {
        let _ = read(&good[..n], &opts);
    }
    for i in 0..good.len() {
        let mut b = good.clone();
        b[i] ^= 0xa5;
        let _ = read(&b, &opts);
    }
    assert!(read(b"not a drawing", &opts).unwrap_err().contains("not a DWG"));
    let e = read(b"AC1009\0\0\0\0", &opts).unwrap_err();
    assert!(e.contains("R12") && e.contains("DXF"), "{e}");
    let mut r2004 = b"AC1018".to_vec();
    r2004.resize(0x200, 0);
    let e = read(&r2004, &opts).unwrap_err();
    assert!(e.contains("2004") && e.contains("save it as DXF"), "{e}");
}

/// Samples shipped with the original program: LANTEK_DIR\Samples\DWG (skipped when absent).
#[test]
fn original_samples_read_when_present() {
    let base = std::env::var("LANTEK_DIR").unwrap_or_else(|_| r"C:\Lantek\Expert".into());
    let dir = std::path::Path::new(&base).join("Samples").join("DWG");
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dwg")) {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let d = read_native(&bytes, &Options::default()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let (g, _) = crate::dxf::build(&d, &Default::default()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(g.outline.len() >= 3 && g.w > 0.0 && g.h > 0.0, "{}", p.display());
        n += 1;
    }
    eprintln!("read {n} sample DWGs");
}

/// Private check over any folder of drawings:
/// NESTPRO_DWG_DIR=<folder> cargo test dwg_dump -- --ignored --nocapture
#[test]
#[ignore]
fn dwg_dump() {
    let dir = std::env::var("NESTPRO_DWG_DIR").unwrap_or_default();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    files.sort();
    for p in files {
        let bytes = std::fs::read(&p).unwrap();
        let t0 = std::time::Instant::now();
        match read_native(&bytes, &Options::default()) {
            Ok(d) => {
                let b = crate::dxf::build(&d, &Default::default());
                println!(
                    "{}: {} pieces, {} texts, {} entities, layers {:?}, skipped {:?} -> {} ({:?})",
                    p.display(),
                    d.pieces.len(),
                    d.texts.len(),
                    d.report.entities,
                    d.layers(),
                    d.report.skipped,
                    match b {
                        Ok((g, r)) => format!("outline {} pts, {} holes, {:.1}x{:.1}, open {}, outside {}", g.outline.len(), g.holes.len(), g.w, g.h, r.open_pieces, r.outside_loops),
                        Err(e) => e,
                    },
                    t0.elapsed()
                );
            }
            Err(e) => println!("{}: ERROR {e}", p.display()),
        }
        if std::env::var("NESTPRO_DWG_PIECES").is_ok() {
            if let Ok(d) = read_native(&bytes, &Options::default()) {
                for pc in &d.pieces {
                    let (a, b) = (pc.pts[0], pc.pts[pc.pts.len() - 1]);
                    println!("  {} n={} closed={} ({:.3},{:.3})..({:.3},{:.3})", pc.layer, pc.pts.len(), pc.closed, a[0], a[1], b[0], b[1]);
                }
                for t in &d.texts {
                    println!("  TEXT {:?}", t);
                }
            }
        }
        // Same drawing as DXF next to it (NESTPRO_DXF_DIR), for comparison.
        if let Ok(dd) = std::env::var("NESTPRO_DXF_DIR") {
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            for ext in ["DXF", "dxf"] {
                if let Ok(t) = std::fs::read(std::path::Path::new(&dd).join(format!("{stem}.{ext}"))) {
                    let r = crate::dxf::read(&String::from_utf8_lossy(&t)).and_then(|d| crate::dxf::build(&d, &Default::default()).map(|x| (d.pieces.len(), x)));
                    match r {
                        Ok((n, (g, _))) => println!("   DXF: {n} pieces -> outline {} pts, {} holes, {:.1}x{:.1}", g.outline.len(), g.holes.len(), g.w, g.h),
                        Err(e) => println!("   DXF: {e}"),
                    }
                }
            }
        }
    }
}

/// Private: which objects fail. NESTPRO_DWG_FILE=<file> cargo test dwg_fail -- --ignored --nocapture
#[test]
#[ignore]
fn dwg_fail() {
    let bytes = std::fs::read(std::env::var("NESTPRO_DWG_FILE").unwrap()).unwrap();
    let (ver, _) = container::version(&bytes).unwrap();
    let layout = match ver {
        Ver::R13 | Ver::R14 | Ver::R2000 => container::layout_r13(&bytes, ver),
        Ver::R2007 => r2007::sections(&bytes).and_then(|s| container::layout_r2004(&bytes, ver, s)),
        _ => container::sections_r2004(&bytes).and_then(|s| container::layout_r2004(&bytes, ver, s)),
    }
    .unwrap();
    println!("classes: {:?}", layout.classes.iter().map(|c| (c.num, c.dxf_name.clone(), c.is_entity)).collect::<Vec<_>>());
    let reader = objects::Reader { buf: &layout.objects, ver, classes: layout.classes.iter().map(|c| (c.num, c.clone())).collect() };
    let mut fails: std::collections::BTreeMap<u16, usize> = Default::default();
    let mut ok: std::collections::BTreeMap<String, usize> = Default::default();
    for &(h, off) in &layout.map {
        let mut r = bits::Bits::at(&layout.objects, off, ver);
        let size = r.ms().unwrap_or(0);
        if ver >= Ver::R2010 {
            let _ = r.mcu();
        }
        let typ = r.ot().unwrap_or(9999);
        match reader.read(off) {
            Ok(o) if o.handle == h => {
                let k = format!("{:?}", o.kind);
                *ok.entry(k.split([' ', '(', '{']).next().unwrap().to_string()).or_default() += 1;
            }
            Ok(o) => {
                println!("handle mismatch {h:x} vs {:x} type {typ} size {size}", o.handle);
                *fails.entry(typ).or_default() += 1
            }
            Err(_) => *fails.entry(typ).or_default() += 1,
        }
    }
    println!("ok {ok:?}\nfails by type {fails:?}");
}

/// Private: flip and cut bytes of real drawings; must never panic.
/// NESTPRO_DWG_FILE=<file> cargo test dwg_fuzz -- --ignored --nocapture
#[test]
#[ignore]
fn dwg_fuzz() {
    let good = std::fs::read(std::env::var("NESTPRO_DWG_FILE").unwrap()).unwrap();
    let n: usize = std::env::var("NESTPRO_FUZZ_N").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    let mut seed: u64 = 0x1234_5678;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let (mut ok, mut err) = (0, 0);
    for i in 0..n {
        let mut b = good.clone();
        if i % 5 == 4 {
            b.truncate((rnd() as usize) % good.len());
        } else {
            for _ in 0..(1 + rnd() % 8) {
                let at = (rnd() as usize) % b.len();
                b[at] ^= (rnd() as u8) | 1;
            }
        }
        match std::panic::catch_unwind(|| read_native(&b, &Options::default())) {
            Ok(Ok(_)) => ok += 1,
            Ok(Err(_)) => err += 1,
            Err(_) => panic!("panic on mutation {i}"),
        }
    }
    println!("fuzz: {ok} read, {err} refused, 0 panics");
}
