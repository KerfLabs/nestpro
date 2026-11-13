//! Engine tests: a whole job programmed through the agent operations, the way
//! the command line and the MCP server run them (job file on disk).

use super::session::{backup_path, Session};
use super::*;
use crate::shared::{Clock, Who};
use serde_json::{json, Value};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nestpro_engine_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn call(s: &mut Session, op: &str, job: Option<&str>, args: Value) -> Value {
    match s.call(op, job, &args) {
        Ok(r) => r.json,
        Err(e) => panic!("{op} {args}: {e}"),
    }
}

fn err(s: &mut Session, op: &str, args: Value) -> String {
    match s.call(op, None, &args) {
        Ok(r) => panic!("{op} {args} should be refused, gave {}", r.json),
        Err(e) => e,
    }
}

#[test]
fn an_lc5_export_writes_straight_edges_and_refuses_what_it_cant_carry() {
    let d = dir("lc5_gate");
    let job = d.join("plate.snjob");
    let jp = job.to_str().unwrap();
    // A plate with R1 corners, each object as a CAD program writes it.
    let (w, h, r) = (300.0, 150.0, 1.0);
    let line = |a: [f64; 2], b: [f64; 2]| format!("0\nLINE\n8\n0\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]);
    let arc = |c: [f64; 2], a0: f64, a1: f64| format!("0\nARC\n8\n0\n10\n{}\n20\n{}\n40\n{r}\n50\n{a0}\n51\n{a1}\n", c[0], c[1]);
    let body = [
        line([r, 0.0], [w - r, 0.0]), arc([w - r, r], 270.0, 360.0),
        line([w, r], [w, h - r]), arc([w - r, h - r], 0.0, 90.0),
        line([w - r, h], [r, h]), arc([r, h - r], 90.0, 180.0),
        line([0.0, h - r], [0.0, r]), arc([r, r], 180.0, 270.0),
    ].concat();
    let dxf = d.join("PLATE.dxf");
    std::fs::write(&dxf, format!("0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n")).unwrap();
    let mut ctx = Ctx::builtin();
    ctx.blm.on = true;
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(jp), json!({"w": 1500, "h": 1000, "material": "steel", "thickness": 5}));
    call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "qty": 2}));
    call(&mut s, "nest", None, json!({"stock": "off"}));
    call(&mut s, "leads", None, json!({"mode": "machine"}));
    let out = d.join("cnc");
    let e = call(&mut s, "export", None, json!({"dir": out.to_str().unwrap()}));
    let zip = e["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).find(|f| f.ends_with(".zip")).unwrap().to_string();

    // BL38: no arc in the program is bigger than the part's corners.
    let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    let radii: Vec<f64> = text.lines().filter(|l| l.starts_with("G02") || l.starts_with("G03")).map(|l| {
        let v = |c: char| l.split_whitespace().find_map(|w| w.strip_prefix(c)).and_then(|n| n.parse::<f64>().ok()).unwrap_or(0.0);
        v('I').hypot(v('J'))
    }).collect();
    assert!(!radii.is_empty() && radii.iter().all(|r| *r < 1.5), "{radii:?}\n{text}");

    // BL39: planned micro-joints stop the export and the program, and the set
    // written before is gone, so it can't be run by mistake.
    call(&mut s, "joints", None, json!({"mode": "auto"}));
    let refused = err(&mut s, "export", json!({"dir": out.to_str().unwrap()}));
    assert!(refused.contains("micro-joints") && refused.contains("nothing was exported"), "{refused}");
    assert!(!Path::new(&zip).exists());
    assert!(err(&mut s, "program", json!({"sheet": 1})).contains("micro-joints"));
}

#[test]
fn an_agent_programs_a_job_from_parts_to_cnc() {
    let d = dir("flow");
    let job = d.join("frames.snjob");
    let jp = job.to_str().unwrap();
    let dxf = d.join("PLATE.dxf");
    std::fs::write(&dxf, crate::dxf::SAMPLE_DXF).unwrap();
    let mut s = Session::new(Ctx::builtin());

    let r = call(&mut s, "new", Some(jp), json!({"w": 1500, "h": 1000, "material": "stainless", "thickness": 2}));
    assert!(job.exists(), "{r}");
    // The file is the truth: the next call opens it from disk.
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "add-shape", Some(jp), json!({"shape": "disc", "w": 200, "qty": 6, "name": "Disc-200"}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 120, "qty": 4}));
    let layers = call(&mut s, "drawing-layers", None, json!({"path": dxf.to_str().unwrap()}));
    assert!(layers["data"]["layers"].as_object().is_some_and(|m| m.contains_key("MARK")), "{layers}");
    // IM10: the part's size before importing, as the import window previews it.
    let size = &layers["data"]["sizes"][0];
    assert!(size["w"].as_f64().is_some_and(|w| w > 0.0) && size["h"].as_f64().is_some_and(|h| h > 0.0), "{layers}");
    let r = call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "layers": "MARK=mark", "qty": 3}));
    assert_eq!(r["data"]["added"][0]["qty"], 3, "{r}");
    let saved = jobfile::load(&job).unwrap();
    assert_eq!(saved.parts.len(), 3);
    assert!(saved.stale && saved.spec.material == 1 && saved.spec.thickness == 2.0);

    // Nest, then look at it.
    let r = call(&mut s, "nest", None, json!({}));
    assert!(r["message"].as_str().unwrap().contains("13 parts"), "{r}");
    let info = call(&mut s, "info", None, json!({"sheet": 1}));
    assert_eq!(info["data"]["totals"]["nested"], 13);
    assert_eq!(info["data"]["changed_since_nest"], false);
    assert!(info["data"]["sheets"][0]["placed"].as_array().unwrap().len() > 0);

    // Technology: machine leads on everything, automatic joints on the rectangles
    // (one per the machine's 100 mm, spaced by distance rather than a count).
    call(&mut s, "leads", None, json!({"mode": "machine"}));
    call(&mut s, "joints", None, json!({"mode": "auto", "parts": "2"}));
    let j = jobfile::load(&job).unwrap();
    let t = j.parts[1].tech.unwrap();
    assert_eq!((t.micro_joints, t.joint_every), (0, 100.0));
    assert!(j.parts.iter().all(|p| p.tech.is_some()));
    // An element lead on the imported part.
    let pi = call(&mut s, "part-info", None, json!({"part": 3}));
    let n = pi["data"]["elements"].as_array().unwrap().len();
    assert!(n >= 4, "{pi}");
    let long = pi["data"]["elements"].as_array().unwrap().iter().find(|e| e["length_mm"].as_f64().unwrap() > 30.0).unwrap()["element"].as_u64().unwrap();
    call(&mut s, "lead-at", None, json!({"part": 3, "element": long}));
    assert_eq!(jobfile::load(&job).unwrap().parts[2].tech.unwrap().starts.iter().flatten().count(), 1);
    call(&mut s, "text", None, json!({"mode": "add", "parts": "Disc-200", "text": "#REF", "kind": "mark"}));

    // Machining: keep the remnant, verify, read the program, export.
    let r = call(&mut s, "remnant", None, json!({"mode": "auto", "sheet": 1}));
    assert!(r["message"].as_str().unwrap().contains("Remnant"), "{r}");
    let v = call(&mut s, "verify", None, json!({}));
    assert!(v["data"].as_array().unwrap().len() >= 1);
    let p = call(&mut s, "program", None, json!({"sheet": 1}));
    let code = p["data"].as_str().unwrap();
    assert!(code.contains("REMNANT LINE"), "the remnant is cut last");
    let out = d.join("out");
    let e = call(&mut s, "export", None, json!({"dir": out.to_str().unwrap()}));
    let files = e["files"].as_array().unwrap();
    assert!(files.iter().any(|f| f.as_str().unwrap().ends_with("parts.csv")));
    assert!(files.iter().all(|f| Path::new(f.as_str().unwrap()).exists()));

    // A picture of the sheet and of a part.
    let r = s.call("render", None, &json!({"sheet": 1})).unwrap();
    let png = r.image.unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let svg = d.join("part.svg");
    s.call("render", None, &json!({"part": 3, "out": svg.to_str().unwrap()})).unwrap();
    assert!(std::fs::read_to_string(&svg).unwrap().starts_with("<svg"));

    // Undo steps back one change at a time, and across runs via <job>~.
    assert!(backup_path(&job).exists());
    call(&mut s, "undo", None, json!({}));
    assert!(jobfile::load(&job).unwrap().cut_lines.iter().all(|c| c.is_empty()), "remnant line undone");
    let mut fresh = Session::new(Ctx::builtin());
    call(&mut fresh, "undo", Some(jp), json!({}));
    assert!(jobfile::load(&job).unwrap().parts[0].texts.is_empty(), "text undone from the file copy");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn refusals_say_what_to_do() {
    let d = dir("refuse");
    let job = d.join("j.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({}));
    assert!(s.call("new", Some(job.to_str().unwrap()), &json!({})).is_err(), "no silent overwrite");
    assert!(err(&mut s, "nest", json!({"levl": 3})).contains("doesn't take 'levl'"));
    assert!(err(&mut s, "nest", json!({})).contains("no parts"));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 300, "qty": 2}));
    call(&mut s, "cut-tech", None, json!({"mode": "own"}));
    assert!(err(&mut s, "edit-part", json!({"part": 1, "w": 500})).contains("tech=copy"));
    let r = call(&mut s, "edit-part", None, json!({"part": 1, "w": 500, "tech": "copy"}));
    assert!(r["message"].as_str().unwrap().contains("(1)"), "{r}");
    call(&mut s, "nest", None, json!({}));
    assert!(err(&mut s, "set-sheet", json!({"thickness": 8})).contains("nestings=keep"));
    assert!(err(&mut s, "place", json!({"part": 1, "x": 0, "y": 0})).contains("no room"));
    assert!(err(&mut s, "move", json!({"item": 9})).contains("no #9"));
    assert!(err(&mut s, "add-shape", json!({"shape": "hexagon"})).contains("not one of"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn placing_by_hand_follows_the_sheet_rules() {
    let d = dir("place");
    let job = d.join("p.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 200, "h": 100, "qty": 1}));
    call(&mut s, "place", None, json!({"part": 1}));
    let r = call(&mut s, "place", None, json!({"part": 1, "angle": 90}));
    assert!(r["message"].as_str().unwrap().contains("quantity now 2"), "{r}");
    let j = jobfile::load(&job).unwrap();
    assert_eq!(j.sheets[0][0].x, 5.0);
    assert_eq!((j.sheets[0][1].w, j.sheets[0][1].h), (100.0, 200.0));
    call(&mut s, "move", None, json!({"item": 1, "x": 600, "y": 300}));
    call(&mut s, "compact", None, json!({}));
    call(&mut s, "copy", None, json!({"item": 1}));
    call(&mut s, "unplace", None, json!({"item": 3}));
    assert_eq!(jobfile::load(&job).unwrap().sheets[0].len(), 2);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn renesting_a_sheet_keeps_its_parts_and_leaves_the_others_alone() {
    // RN1, RN2
    let d = dir("renest");
    let job = d.join("r.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 200, "h": 100, "qty": 3}));
    // Two parts spread out on sheet 1, one on a second sheet: all three would
    // fit on one sheet, so a whole-job nest would empty sheet 2.
    call(&mut s, "place", None, json!({"part": 1, "x": 300, "y": 300}));
    call(&mut s, "place", None, json!({"part": 1, "x": 700, "y": 50}));
    call(&mut s, "place", None, json!({"part": 1, "sheet": "new", "x": 400, "y": 200}));
    let r = call(&mut s, "renest-sheet", None, json!({"sheet": 1}));
    assert!(r["message"].as_str().unwrap().contains("Sheet 1 nested again"), "{r}");
    let j = jobfile::load(&job).unwrap();
    assert_eq!(j.sheets.len(), 2, "the second sheet stays");
    assert_eq!(j.sheets[0].len(), 2);
    assert!(j.sheets[0].iter().all(|p| p.x < 300.0), "packed to the left: {:?}", j.sheets[0]);
    assert_eq!((j.sheets[1].len(), j.sheets[1][0].x, j.sheets[1][0].y), (1, 400.0, 200.0), "sheet 2 untouched");
    assert!(err(&mut s, "renest-sheet", json!({"sheet": 3})).contains("no sheet 3"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn renesting_a_sheet_keeps_each_group_in_its_zone() {
    // RN3
    let d = dir("renest-groups");
    let job = d.join("g.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 150, "h": 100, "qty": 2, "group": "Acme"}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 150, "h": 100, "qty": 2, "group": "Demo"}));
    // Mixed up by hand: Demo parts left of Acme parts.
    call(&mut s, "place", None, json!({"part": 2, "x": 10, "y": 10}));
    call(&mut s, "place", None, json!({"part": 1, "x": 200, "y": 10}));
    call(&mut s, "place", None, json!({"part": 2, "x": 400, "y": 10}));
    call(&mut s, "place", None, json!({"part": 1, "x": 600, "y": 10}));
    call(&mut s, "renest-sheet", None, json!({"sheet": 1, "group": true}));
    let j = jobfile::load(&job).unwrap();
    assert_eq!(j.sheets[0].len(), 4);
    let acme_end = j.sheets[0].iter().filter(|p| p.part == 0).map(|p| p.x + p.w).fold(0.0, f64::max);
    let demo_start = j.sheets[0].iter().filter(|p| p.part == 1).map(|p| p.x).fold(f64::MAX, f64::min);
    assert!(demo_start > acme_end, "Demo's zone after Acme's: {:?}", j.sheets[0]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn every_op_has_a_schema_and_mcp_lists_them() {
    let mut s = Session::new(Ctx::builtin());
    let init = crate::mcp::handle(&mut s, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26"}})).unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert!(crate::mcp::handle(&mut s, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
    let list = crate::mcp::handle(&mut s, &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), ops::OPS.len());
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
        assert!(t["name"].as_str().unwrap().chars().all(|c| c.is_ascii_lowercase() || c == '_'));
    }
    let bad = crate::mcp::handle(&mut s, &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "nest", "arguments": {}}})).unwrap();
    assert_eq!(bad["result"]["isError"], true, "no job open yet");
    assert_eq!(crate::mcp::base64(b"Man"), "TWFu");
    assert_eq!(crate::mcp::base64(b"Ma"), "TWE=");
}

#[test]
fn an_order_comes_in_as_xml_and_a_machine_program_comes_back_as_a_job() {
    let d = dir("imports");
    std::fs::write(d.join("brk.dxf"), crate::dxf::SAMPLE_DXF).unwrap();
    let xml = d.join("order-77.xml");
    std::fs::write(
        &xml,
        r#"<?xml version="1.0"?><Import>
  <Product Reference="BRK-12" Material="Mild steel" Thickness="3" Quantity="40" Fich="brk.dxf"/>
  <Product Reference="BRK-13" Material="Mild steel" Thickness="3" Quantity="0" Fich="brk.dxf"/>
  <Product Reference="PL-5" Material="AISI304" Thickness="1,5" Quantity="2" Fich="brk.dxf"/>
</Import>"#,
    )
    .unwrap();
    let out = d.join("jobs");
    let mut s = Session::new(Ctx::builtin());
    let r = call(&mut s, "import-xml", None, json!({"path": xml.to_str().unwrap(), "dir": out.to_str().unwrap()}));
    let files = r["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "one job per material and thickness: {r}");
    assert!(r["message"].as_str().unwrap().contains("2 of 3 product(s)"), "{r}");
    let steel = jobfile::load(Path::new(files[0].as_str().unwrap())).unwrap();
    assert_eq!((steel.spec.material, steel.spec.thickness, steel.parts.len(), steel.parts[0].qty), (0, 3.0, 1, 40));
    let stainless = jobfile::load(Path::new(files[1].as_str().unwrap())).unwrap();
    assert_eq!((stainless.spec.material, stainless.spec.thickness), (1, 1.5));
    // No silent overwrite; the jobs are ordinary job files an agent carries on with.
    assert!(s.call("import-xml", None, &json!({"path": xml.to_str().unwrap(), "dir": out.to_str().unwrap()})).err().unwrap_or_default().contains("exist"));
    call(&mut s, "nest", Some(files[0].as_str().unwrap()), json!({"stock": "off"}));

    // A BLM LC5 set written by the engine reads back as a job.
    let mut ctx = Ctx::builtin();
    ctx.blm.on = true;
    let mut b = Session::new(ctx);
    let e = call(&mut b, "export", Some(files[0].as_str().unwrap()), json!({"dir": d.join("cnc").to_str().unwrap()}));
    let zip = e["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).find(|f| f.ends_with(".zip")).unwrap().to_string();
    let back = d.join("back.snjob");
    let r = call(&mut s, "import-program", Some(back.to_str().unwrap()), json!({"path": zip}));
    assert!(r["message"].as_str().unwrap().contains("placed part"), "{r}");
    let j = jobfile::load(&back).unwrap();
    assert_eq!(j.sheets.len(), 1);
    assert_eq!(j.sheets[0].len(), steel.parts[0].qty.min(j.sheets[0].len() as u32) as usize);
    assert!(!j.parts.is_empty() && j.spec.thickness == 3.0);
    // Exported again as plain G-code: the LC5 set it replaces is gone, so no
    // stale program is left in the folder to run.
    let e2 = call(&mut s, "export", Some(files[0].as_str().unwrap()), json!({"dir": d.join("cnc").to_str().unwrap()}));
    assert!(!Path::new(&zip).exists() && !Path::new(&zip.replace(".zip", ".XML")).exists());
    assert!(e2["files"].as_array().unwrap().iter().all(|f| Path::new(f.as_str().unwrap()).exists()));
    // Every sheet gone: exporting refuses, and takes the old programs away.
    call(&mut s, "clear-sheets", Some(files[0].as_str().unwrap()), json!({}));
    assert!(err(&mut s, "export", json!({"dir": d.join("cnc").to_str().unwrap()})).contains("Nothing to export"));
    let prog = e2["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).find(|f| !f.ends_with(".csv")).unwrap().to_string();
    assert!(!Path::new(&prog).exists(), "{prog}");
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// The sheet stock through the agent operations (the window's stock model:
// stock.rs items, per-sheet formats, `held` reconciled like a window save).
// ---------------------------------------------------------------------------

/// A context whose library folder (and so its stock) is the test's own.
fn stock_ctx(d: &Path) -> Ctx {
    let mut c = Ctx::builtin();
    c.library_dir = d.join("library");
    crate::library::create(&c.library_dir).unwrap();
    c.cfg.stock = crate::stock::StockSettings::default();
    c
}

/// Every part inside its sheet (a remnant's outline too) and clear of the others.
fn check_sheets(j: &JobFile) {
    for (k, s) in j.sheets.iter().enumerate() {
        let sp = j.spec_at(k);
        for (a, p) in s.iter().enumerate() {
            assert!(j.fits(k, p), "sheet {} #{} leaves the sheet {:?}", k + 1, a + 1, p);
            for q in &s[a + 1..] {
                assert!(!nest::clash_exact(&j.parts, p, q, sp.gap - 1e-6), "sheet {} parts overlap", k + 1);
            }
        }
    }
}

#[test]
fn do_all_takes_sheets_from_stock_and_booking_stores_the_remnants() {
    let d = dir("stock_flow");
    let jp = d.join("brackets.snjob");
    let jp = jp.to_str().unwrap();
    let ctx = stock_ctx(&d);
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(jp), json!({"w": 2000, "h": 1000, "thickness": 3}));
    // Stock: 2 small sheets, 1 large one, a locked one, and one for another thickness.
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "S1", "w": 1500, "h": 750, "thickness": 3, "qty": 2}));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "BIG", "w": 3000, "h": 1500, "thickness": 3}));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "NO", "w": 4000, "h": 2000, "thickness": 3, "blocked": true}));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "T5", "w": 4000, "h": 2000, "thickness": 5}));
    let l = call(&mut s, "stock", None, json!({"mode": "list", "thickness": 3}));
    assert_eq!(l["data"]["sheets"].as_array().unwrap().len(), 3, "{l}");
    assert_eq!(l["data"]["rules"]["order"], crate::stock::Order::LargestFirst.label(), "{l}");
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 300, "qty": 40}));

    // Largest first: the 3000 × 1500 sheet, then the small ones.
    let r = call(&mut s, "nest", None, json!({}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert_eq!(j.format(0).stock_ref, "BIG", "{r}");
    assert!((0..j.sheets.len()).all(|k| { let f = j.format(k); f.stock_ref != "NO" && f.stock_ref != "T5" }), "locked / other thickness never used");
    let total: usize = j.sheets.iter().map(|s| s.len()).sum();
    assert_eq!(total, 40, "{r}");
    assert_eq!(j.spec_at(0).w, 3000.0);
    check_sheets(&j);
    // Not booked yet: the stock is untouched.
    let st = crate::stock::Stock::load(&lib).unwrap();
    assert_eq!(st.get("BIG").unwrap().qty, 1);

    // Remnant on the last sheet, then book: sheets out, remnant in.
    let last = j.sheets.len();
    let last_ref = j.format(last - 1).stock_ref.clone();
    call(&mut s, "remnant", None, json!({"mode": "auto", "sheet": last}));
    let b = call(&mut s, "stock-book", None, json!({"mode": "book"}));
    let st = crate::stock::Stock::load(&lib).unwrap();
    assert_eq!(st.get("BIG").unwrap().qty, 0, "{b}");
    let rem: Vec<_> = st.items.iter().filter(|x| x.kind.is_remnant()).collect();
    // The automatic line can leave more than one piece (each its own remnant).
    assert!(!rem.is_empty(), "{b}");
    for r in &rem {
        assert!(r.reference.starts_with(&format!("{last_ref}-")), "{}", r.reference);
        assert_eq!((r.from_job.as_str(), r.qty), ("brackets", 1));
    }
    let status = call(&mut s, "stock-book", None, json!({"mode": "status"}));
    assert_eq!(status["data"]["booked"], true, "{status}");

    // Booked: deleting the last sheet gives its stock sheet back.
    let before = st.get(&last_ref).unwrap().qty;
    call(&mut s, "sheet-delete", None, json!({"sheet": last}));
    let st2 = crate::stock::Stock::load(&lib).unwrap();
    assert_eq!(st2.get(&last_ref).unwrap().qty, before + 1);

    // Undo puts the sheet back and takes it from stock again.
    call(&mut s, "undo", None, json!({}));
    let st3 = crate::stock::Stock::load(&lib).unwrap();
    assert_eq!(st3.get(&last_ref).unwrap().qty, before);

    // Release: everything back as it was.
    call(&mut s, "stock-book", None, json!({"mode": "release"}));
    let st4 = crate::stock::Stock::load(&lib).unwrap();
    assert_eq!(st4.get("BIG").unwrap().qty, 1);
    assert_eq!(st4.get("S1").unwrap().qty, 2);
    assert!(!st4.items.iter().any(|x| x.kind.is_remnant()), "the unused remnant is taken out again");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn undoing_a_booked_sheet_renest_brings_its_remnant_back() {
    // RN4, RN6: book → re-nest (its remnant is replaced) → undo.
    let d = dir("renest_book");
    let jp = d.join("demo.snjob");
    let jp = jp.to_str().unwrap();
    let ctx = stock_ctx(&d);
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(jp), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "S1", "w": 2000, "h": 1000, "thickness": 3, "qty": 2}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 200, "qty": 6}));
    call(&mut s, "nest", None, json!({}));
    call(&mut s, "remnant", None, json!({"mode": "auto", "sheet": 1}));
    call(&mut s, "stock-book", None, json!({"mode": "book"}));
    let remnants = || crate::stock::Stock::load(&lib).unwrap().items.into_iter().filter(|x| x.kind.is_remnant()).map(|x| x.reference).collect::<Vec<_>>();
    let made = jobfile::load(Path::new(jp)).unwrap().formats[0].made.clone();
    assert!(!made.is_empty());
    assert_eq!(remnants(), made);
    // Re-nesting drops the remnant line, so the stored remnant goes.
    call(&mut s, "renest-sheet", None, json!({"sheet": 1}));
    assert!(remnants().is_empty(), "replaced by nothing: no line now");
    call(&mut s, "undo", None, json!({}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert!(!j.formats[0].made.is_empty(), "the restored sheet has its remnant again");
    assert_eq!(remnants(), j.formats[0].made, "in stock, and nothing left behind");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_remnant_is_nested_inside_its_outline_and_used_first() {
    let d = dir("stock_remnant");
    let jp = d.join("r.snjob");
    let jp = jp.to_str().unwrap();
    let mut ctx = stock_ctx(&d);
    // Keep the remnant as stored (no turning for the machine) so the test knows its shape.
    ctx.cfg.stock.remnant_rotation = crate::stock::RemnantRotation::NotAllowed;
    let mut st = crate::stock::Stock::default();
    st.add_sheet(crate::stock::Item::sheet("A", 0, 3.0, 3000.0, 1500.0, 5), true).unwrap();
    // An L-shaped remnant: 1200 wide at the bottom, 400 wide above y = 500.
    let l = crate::stock::Outline { outer: vec![[0.0, 0.0], [1200.0, 0.0], [1200.0, 500.0], [400.0, 500.0], [400.0, 1500.0], [0.0, 1500.0]], holes: vec![] };
    let parent = st.get("A").unwrap().clone();
    st.add_remnant(l, &parent, 1, Some("A-1".into()), "old");
    st.save(&ctx.library_dir).unwrap();
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(jp), json!({"thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 200, "qty": 12, "rotate": false}));
    call(&mut s, "nest", None, json!({}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert_eq!(j.format(0).stock_ref, "A-1", "remnants first");
    assert!(j.format(0).remnant);
    assert!(!j.sheets[0].is_empty(), "parts go on the remnant");
    check_sheets(&j);
    // Nothing in the cut-away corner.
    assert!(j.sheets[0].iter().all(|p| !(p.x + p.w > 400.0 && p.y + p.h > 500.0)), "{:?}", j.sheets[0]);
    // A part can't be moved into the missing corner.
    let e = err(&mut s, "move", json!({"sheet": 1, "item": 1, "x": 800, "y": 900}));
    assert!(e.contains("overlap") || e.contains("margin") || e.contains("sheet"), "{e}");
    // The picture draws its outline.
    let png = d.join("r.png");
    call(&mut s, "render", None, json!({"sheet": 1, "out": png.to_str().unwrap()}));
    assert!(png.exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn stock_runs_out_and_sheets_are_picked_by_hand() {
    let d = dir("stock_out");
    let jp = d.join("o.snjob");
    let jp = jp.to_str().unwrap();
    let mut s = Session::new(stock_ctx(&d));
    call(&mut s, "new", Some(jp), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "ONE", "w": 1000, "h": 1000, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 450, "h": 450, "qty": 10}));
    // Only stock: 4 fit on the one sheet, the rest wait.
    let r = call(&mut s, "nest", None, json!({"stock": "only"}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert_eq!((j.sheets.len(), j.sheets[0].len()), (1, 4), "{r}");
    assert!(r["message"].as_str().unwrap().contains("4 of 10"), "{r}");
    // Default options: the rest go on the job's sheet size.
    call(&mut s, "nest", None, json!({}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert_eq!(j.sheets.iter().map(|s| s.len()).sum::<usize>(), 10);
    assert!(j.format(1).stock_ref.is_empty(), "the job's own sheet");
    // Off: the job's sheet only.
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert!(j.formats.iter().all(|f| f.stock_ref.is_empty()));
    // Sheets by hand: from stock, a size, refused when the stock has none left.
    call(&mut s, "clear-sheets", None, json!({}));
    call(&mut s, "sheet-add", None, json!({"ref": "ONE"}));
    let e = err(&mut s, "sheet-add", json!({"ref": "ONE"}));
    assert!(e.contains("no more in stock"), "{e}");
    call(&mut s, "sheet-add", None, json!({"w": 1200, "h": 600}));
    call(&mut s, "place", None, json!({"part": 1, "sheet": "2"}));
    let e = err(&mut s, "sheet-change", json!({"sheet": 2, "w": 300, "h": 300}));
    assert!(e.contains("outside"), "{e}");
    call(&mut s, "sheet-delete", None, json!({"sheet": 1}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert_eq!(j.sheets.len(), 1);
    assert_eq!(j.spec_at(0).w, 1200.0);
    // Stock edits: quantity to 0 deletes; the rules are read-only here.
    call(&mut s, "stock", None, json!({"mode": "remove", "ref": "ONE"}));
    assert!(s.ctx.stock().unwrap().get("ONE").is_none());
    let rules = call(&mut s, "stock", None, json!({"mode": "rules"}));
    assert_eq!(rules["data"]["use_stock"], true, "{rules}");
    let _ = std::fs::remove_dir_all(&d);
}

/// The automatic leads, micro-joints and true-shape nesting the window uses are
/// what the agent operations give too (one cut plan, `cnc::plan`).
#[test]
fn agent_leads_and_joints_follow_the_machine_rules() {
    let d = dir("leads_joints");
    let jp = d.join("lj.snjob");
    let jp = jp.to_str().unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(jp), json!({"w": 1500, "h": 1000, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 120, "qty": 2}));
    call(&mut s, "add-shape", None, json!({"shape": "l-bracket", "w": 200, "h": 160, "qty": 4}));
    call(&mut s, "nest", None, json!({}));
    // Leads from the machine's tables (the default type), joints automatic.
    let r = call(&mut s, "leads", None, json!({"mode": "auto", "position": 9}));
    assert!(r["message"].as_str().unwrap().contains("Automatic lead-ins"), "{r}");
    call(&mut s, "joints", None, json!({"mode": "auto"}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert!(j.parts.iter().all(|p| p.tech.is_some_and(|t| t.lead_type == nest::LeadType::Machine && t.corner_start && t.joint_every == 100.0)));
    let info = call(&mut s, "part-info", None, json!({"part": 1}));
    assert_eq!(info["data"]["technology"]["lead_type"], "machine", "{info}");
    // The cut plan: a 300 × 120 rectangle (840 mm round) gets ceil(840 / 100) = 9
    // joints, made even = 10; every contour has a lead.
    let plan = crate::cnc::plan(&j.parts, &j.sheets[0], &j.spec_at(0));
    let rect: Vec<_> = plan.contours.iter().filter(|c| !c.inner && j.sheets[0][c.part_no - 1].part == 0).collect();
    assert!(!rect.is_empty());
    assert!(rect.iter().all(|c| c.joints.len() == 10), "{:?}", rect.iter().map(|c| c.joints.len()).collect::<Vec<_>>());
    assert!(plan.contours.iter().all(|c| c.lead_path.len() >= 2));
    // A given spacing; delete takes the automatic joints away too.
    call(&mut s, "joints", None, json!({"mode": "auto", "parts": "1", "every": 50}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert_eq!(j.parts[0].tech.unwrap().joint_every, 50.0);
    assert!(err(&mut s, "joints", json!({"mode": "auto", "every": 0})).contains("more than 0"));
    call(&mut s, "joints", None, json!({"mode": "delete"}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    let plan = crate::cnc::plan(&j.parts, &j.sheets[0], &j.spec_at(0));
    assert!(plan.contours.iter().all(|c| c.joints.is_empty()));
    // Leads deleted: back to a plain type with no length.
    call(&mut s, "leads", None, json!({"mode": "delete"}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert!(j.parts.iter().all(|p| p.tech.is_some_and(|t| t.lead_type == nest::LeadType::Line && t.lead_in == 0.0)));
    let _ = std::fs::remove_dir_all(&d);
}

/// Nesting by outline through the agent: small L brackets turn into each
/// other's notches, so they need less sheet than their boxes would.
#[test]
fn agent_nest_interlocks_parts_by_their_outline() {
    let d = dir("true_shape");
    let jp = d.join("ts.snjob");
    let jp = jp.to_str().unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(jp), json!({"w": 1000, "h": 500, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "l-bracket", "w": 200, "h": 200, "qty": 12}));
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    check_sheets(&j);
    // Twelve 200 × 200 boxes need more than one 1000 × 500 sheet (at most 4 × 2 = 8
    // fit); by their outline they all go on one.
    assert_eq!(j.sheets.len(), 1, "{:?}", j.sheets.iter().map(|s| s.len()).collect::<Vec<_>>());
    assert_eq!(j.sheets[0].len(), 12);
    let _ = std::fs::remove_dir_all(&d);
}

/// The window's Do All searches reach agents' `nest`: best sheet combination
/// and best sheet format both take the small sheet that 4 parts fill well.
#[test]
fn agent_nest_follows_best_combination_and_best_format() {
    let d = dir("best_sheets");
    let jp = d.join("b.snjob");
    let jp = jp.to_str().unwrap();
    let mut ctx = stock_ctx(&d);
    ctx.cfg.stock.fallback_job_size = false;
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(jp), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "BIG", "w": 3000, "h": 1500, "thickness": 3, "qty": 5}));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "SMALL", "w": 1000, "h": 1000, "thickness": 3, "qty": 5}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 450, "h": 450, "qty": 4}));
    let refs = |jp: &str| jobfile::load(Path::new(jp)).unwrap().formats.iter().map(|f| f.stock_ref.clone()).collect::<Vec<_>>();
    call(&mut s, "nest", None, json!({}));
    assert_eq!(refs(jp), ["BIG"], "largest first without the searches");
    for (combo, format) in [(true, false), (false, true)] {
        s.ctx.cfg.stock.best_combination = combo;
        s.ctx.cfg.stock.combination_min = 0.2;
        s.ctx.cfg.stock.best_format = format;
        call(&mut s, "nest", None, json!({}));
        assert_eq!(refs(jp), ["SMALL"], "combination {combo}, format {format}");
        assert_eq!(jobfile::load(Path::new(jp)).unwrap().sheets[0].len(), 4);
    }
    let rules = call(&mut s, "stock", None, json!({"mode": "rules"}));
    assert_eq!(rules["data"]["best_sheet_format"], true, "{rules}");
    let _ = lib;
    let _ = std::fs::remove_dir_all(&d);
}

/// Booking names the new remnants as told, and a moved remnant line remakes
/// them — until one is used, then they stay.
#[test]
fn agent_booking_names_and_remakes_remnants() {
    let d = dir("remake");
    let jp = d.join("r.snjob");
    let jp = jp.to_str().unwrap();
    let ctx = stock_ctx(&d);
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(jp), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 400, "qty": 4}));
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    let mx = j.sheets[0].iter().map(|p| p.x + p.w).fold(0.0, f64::max);
    call(&mut s, "remnant", None, json!({"mode": "line", "x": mx + 20.0}));
    let st = call(&mut s, "stock-book", None, json!({"mode": "status"}));
    let rows = st["data"]["would_store_remnants"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 1, "{st}");
    assert!(!rows[0]["ref"].as_str().unwrap().is_empty());
    assert!(err(&mut s, "stock-book", json!({"mode": "book", "refs": "A,B"})).contains("1 new remnant"));
    call(&mut s, "stock-book", None, json!({"mode": "book", "refs": "RACK-C-7"}));
    assert!(crate::stock::Stock::load(&lib).unwrap().get("RACK-C-7").is_some());
    // Booked: moving the line remakes the (unused) remnant.
    call(&mut s, "remnant", None, json!({"mode": "line", "x": mx + 120.0}));
    let j = jobfile::load(Path::new(jp)).unwrap();
    let again = j.formats[0].made.clone();
    assert_eq!(again.len(), 1);
    let st = crate::stock::Stock::load(&lib).unwrap();
    assert!(st.get("RACK-C-7").is_none() && st.get(&again[0]).is_some(), "replaced");
    assert_eq!(st.items.iter().filter(|x| x.kind.is_remnant()).count(), 1);
    // Once it's used, moving the line keeps it, and says so.
    crate::stock::Stock::update(&crate::library::Library::at(&lib, &Who::new("kim", "PROG-2"), &Clock::pc()), |s| s.take(&again[0], 1, false)).unwrap();
    let r = call(&mut s, "remnant", None, json!({"mode": "line", "x": mx + 220.0}));
    assert!(r["message"].as_str().unwrap().contains("already in use"), "{r}");
    assert_eq!(jobfile::load(Path::new(jp)).unwrap().formats[0].made, again);
    let _ = std::fs::remove_dir_all(&d);
}

/// A refused operation changes nothing, not even in the session; an edited
/// disc changes its whole diameter and a quantity edit leaves sizes alone; an
/// MCP call with a job that isn't a path is refused, not run on the open job.
#[test]
fn refusals_change_nothing_and_edits_change_only_what_was_asked() {
    let d = dir("refusals");
    let jp = d.join("r.snjob");
    let jp = jp.to_str().unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(jp), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "disc", "w": 100, "qty": 1}));
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let kerf = s.job.as_ref().unwrap().spec.kerf;
    err(&mut s, "compensation", json!({"system": "on", "sheet": 999}));
    assert_eq!(s.job.as_ref().unwrap().spec.kerf, kerf, "refused: nothing kept");
    call(&mut s, "edit-part", None, json!({"part": 1, "w": 200}));
    let p = &jobfile::load(Path::new(jp)).unwrap().parts[0];
    assert_eq!((p.w, p.h), (200.0, 200.0), "a disc's diameter is both sizes");
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 100, "h": 100, "qty": 1}));
    s.job.as_mut().unwrap().parts[1].w = 0.5;
    call(&mut s, "edit-part", None, json!({"part": 2, "qty": 10}));
    assert_eq!(jobfile::load(Path::new(jp)).unwrap().parts[1].w, 0.5, "a quantity edit leaves the size");
    let r = crate::mcp::handle(&mut s, &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "clear-sheets", "arguments": {"job": 123}}})).unwrap();
    assert_eq!(r["result"]["isError"], true, "{r}");
    assert!(!jobfile::load(Path::new(jp)).unwrap().sheets.is_empty(), "the open job wasn't touched");
    let _ = std::fs::remove_dir_all(&d);
}

/// Stock only, nothing in stock: no sheet is made up. Two jobs nested on the
/// last sheet: the second booking takes nothing and stores no remnants of a
/// sheet it couldn't take.
#[test]
fn stock_only_never_invents_a_sheet_and_a_failed_booking_stores_no_remnants() {
    let d = dir("stock_last");
    let ctx = stock_ctx(&d);
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    let (ja, jb) = (d.join("a.snjob"), d.join("b.snjob"));
    let (ja, jb) = (ja.to_str().unwrap(), jb.to_str().unwrap());
    call(&mut s, "new", Some(ja), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 200, "qty": 2}));
    call(&mut s, "nest", None, json!({"stock": "only"}));
    assert!(jobfile::load(Path::new(ja)).unwrap().sheets.iter().all(|s| s.is_empty()));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "LAST", "w": 2000, "h": 1000, "thickness": 3, "qty": 1}));
    for jp in [ja, jb] {
        if jp == jb {
            call(&mut s, "new", Some(jb), json!({"w": 2000, "h": 1000, "thickness": 3}));
            call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 200, "qty": 2}));
        }
        call(&mut s, "nest", Some(jp), json!({"stock": "only"}));
        assert_eq!(jobfile::load(Path::new(jp)).unwrap().formats[0].stock_ref, "LAST");
        call(&mut s, "remnant", None, json!({"mode": "line", "x": 800.0}));
    }
    call(&mut s, "stock-book", Some(ja), json!({"mode": "book"}));
    let remnants = |lib: &Path| crate::stock::Stock::load(lib).unwrap().items.iter().filter(|x| x.kind.is_remnant()).count();
    assert_eq!(remnants(&lib), 1);
    let r = call(&mut s, "stock-book", Some(jb), json!({"mode": "book"}));
    assert!(r["message"].as_str().unwrap().contains("remnants not stored"), "{r}");
    assert_eq!(remnants(&lib), 1, "no remnant of a sheet that wasn't taken");
    let _ = std::fs::remove_dir_all(&d);
}

/// SV10 for agents: two agents on two PCs nest on the last 2000 × 1000
/// sheet. Sam's agent on PROG-1 books first; Kim's agent on PROG-2 books ten
/// minutes later and is told who took the sheet and when. Nothing is taken
/// for it, and nesting again books what's in stock now (SV11).
#[test]
fn an_agents_stock_book_names_who_took_the_last_sheet() {
    let d = dir("stock_gone");
    let clock = Clock::at(1_794_216_000); // 2026-11-09 09:20 UTC
    let agent = |user: &str, pc: &str| {
        let mut ctx = stock_ctx(&d);
        ctx.who = Who::new(user, pc).as_agent();
        ctx.clock = clock.clone();
        Session::new(ctx)
    };
    let (mut prog1, mut prog2) = (agent("sam", "PROG-1"), agent("kim", "PROG-2"));
    let lib = d.join("library");
    let (ja, jb) = (d.join("DEMO-001 frames.snjob"), d.join("DEMO-002 brackets.snjob"));
    let (ja, jb) = (ja.to_str().unwrap(), jb.to_str().unwrap());
    call(&mut prog1, "stock", None, json!({"mode": "create", "ref": "LAST", "w": 2000, "h": 1000, "thickness": 3, "qty": 1}));
    call(&mut prog2, "stock", None, json!({"mode": "create", "ref": "SMALL", "w": 1500, "h": 750, "thickness": 3, "qty": 2}));
    for (s, jp) in [(&mut prog1, ja), (&mut prog2, jb)] {
        call(s, "new", Some(jp), json!({"w": 2000, "h": 1000, "thickness": 3}));
        call(s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 200, "qty": 4}));
        call(s, "nest", None, json!({"stock": "only"}));
        assert_eq!(jobfile::load(Path::new(jp)).unwrap().formats[0].stock_ref, "LAST");
    }
    call(&mut prog1, "stock-book", None, json!({"mode": "book"}));
    clock.advance(std::time::Duration::from_secs(10 * 60));
    let r = call(&mut prog2, "stock-book", None, json!({"mode": "book"}));
    let m = r["message"].as_str().unwrap();
    assert!(m.starts_with("Not booked: sam on PROG-1 (agent) took the last 2000 × 1000 (LAST) at 09:20"), "{r}");
    assert!(m.contains("nest again"), "says what to do: {r}");
    let st = crate::stock::Stock::load(&lib).unwrap();
    assert_eq!((st.get("LAST").unwrap().qty, st.get("SMALL").unwrap().qty), (0, 2), "nothing taken for PROG-2");
    // The stock says who changed it, and when (SV9).
    let l = call(&mut prog2, "stock", None, json!({"mode": "list", "show_zero": true}));
    let row = |r: &str| l["data"]["sheets"].as_array().unwrap().iter().find(|x| x["ref"] == r).cloned().unwrap();
    assert_eq!((row("LAST")["changed_by"].as_str(), row("LAST")["changed_at"].as_str()), (Some("sam on PROG-1 (agent)"), Some("2026-11-09 09:20")), "{l}");
    assert_eq!(row("SMALL")["changed_by"].as_str(), Some("kim on PROG-2 (agent)"), "{l}");
    // Nesting again plans from the stock as it is now, and books it.
    let r = call(&mut prog2, "nest", None, json!({"stock": "only"}));
    assert!(jobfile::load(Path::new(jb)).unwrap().formats.iter().all(|f| f.stock_ref == "SMALL"), "{r}");
    assert_eq!(crate::stock::Stock::load(&lib).unwrap().get("SMALL").unwrap().qty, 1, "{r}");
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// The machine file's material × thickness sections (MTL1–MTL9), through the
// agent operations. Each test puts a made-up machine file on its own thread.
// ---------------------------------------------------------------------------

fn demo_machine() {
    crate::ct1::set_thread_active(Some(crate::ct1::MachineTech::from_text(&crate::ct1::demo_sections_file())));
}

#[test]
fn an_agent_sees_which_machine_table_the_job_uses() {
    demo_machine();
    let d = dir("mtl_table");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"material": "mild steel", "thickness": 3, "gap": 5, "margin": 5}));
    let r = call(&mut s, "machine-tech", None, json!({}));
    assert_eq!(r["data"]["section"]["thickness_from"], 2.2, "{r}");
    assert_eq!(r["data"]["table"], "Machine table: 0010 mild steel 2.2–3.2 mm");
    assert_eq!((r["data"]["spacing"]["machine_gap"].as_f64(), r["data"]["spacing"]["machine_margin"].as_f64()), (Some(6.0), Some(12.0)));
    assert_eq!(r["data"]["joints"]["gap"], 0.35);
    assert!(r["message"].as_str().unwrap().contains("Machine part gap 6 mm"), "{r}");
    // Between two ranges: the machine's own values.
    let r = call(&mut s, "machine-tech", None, json!({"thickness": 1.65}));
    assert!(r["data"]["section"].is_null(), "{r}");
    assert!(r["data"]["table"].as_str().unwrap().contains("global"));
    // Another material without touching the job.
    let r = call(&mut s, "machine-tech", None, json!({"material": "stainless", "thickness": 8}));
    assert_eq!(r["data"]["spacing"]["machine_gap"], 9.0, "{r}");
    assert_eq!(jobfile::load(&job).unwrap().spec.material, 0);
}

#[test]
fn use_machine_values_takes_the_plates_gap_and_margin() {
    demo_machine();
    let d = dir("mtl_spacing");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"material": "mild steel", "thickness": 3, "gap": 5, "margin": 5}));
    // Changing the thickness alone never changes the sheet's spacing.
    call(&mut s, "set-sheet", None, json!({"thickness": 15}));
    let j = jobfile::load(&job).unwrap();
    assert_eq!((j.spec.gap, j.spec.margin), (5.0, 5.0));
    // The one-click action: the plate's gap and widest margin.
    let r = call(&mut s, "set-sheet", None, json!({"machine_spacing": true}));
    assert!(r["message"].as_str().unwrap().contains("Machine spacing"), "{r}");
    let j = jobfile::load(&job).unwrap();
    assert_eq!((j.spec.gap, j.spec.margin), (15.0, 10.0));
    // A gap given in the same call wins over the machine's.
    call(&mut s, "set-sheet", None, json!({"thickness": 3, "machine_spacing": true, "gap": 8}));
    let j = jobfile::load(&job).unwrap();
    assert_eq!((j.spec.gap, j.spec.margin), (8.0, 12.0));
    // A machine with no spacing values refuses and says why.
    crate::ct1::set_thread_active(Some(crate::ct1::MachineTech::default()));
    let e = err(&mut s, "set-sheet", json!({"machine_spacing": true}));
    assert!(e.contains("no part gap or margins"), "{e}");
}

#[test]
fn leads_on_thick_plate_are_longer_than_on_thin_sheet() {
    demo_machine();
    let d = dir("mtl_leads");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 1000, "material": "mild steel", "thickness": 1}));
    call(&mut s, "add-shape", None, json!({"shape": "plate-hole", "w": 300, "h": 200, "qty": 1}));
    call(&mut s, "nest", None, json!({}));
    let j = jobfile::load(&job).unwrap();
    let longest = |thk: f64| {
        let spec = SheetSpec { thickness: thk, ..j.spec_at(0) };
        let plan = crate::cnc::plan(&j.parts, &j.sheets[0], &spec);
        plan.contours.iter().map(|c| crate::leads::path_len(&c.lead_path)).fold(0.0, f64::max)
    };
    let (thin, thick, between) = (longest(1.0), longest(15.0), longest(1.65));
    assert!(thin > 0.0 && thick > 3.0 * thin, "plate leads {thick} mm vs sheet leads {thin} mm");
    // No section for 1.65 mm: the machine's own 7 mm tables, between the two.
    assert!(between > thin && between < thick, "{thin} < {between} < {thick}");
}

#[test]
fn automatic_joints_take_the_gap_of_the_plates_table() {
    demo_machine();
    let d = dir("mtl_joints");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"material": "mild steel", "thickness": 15}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 200, "qty": 1}));
    call(&mut s, "joints", None, json!({"mode": "auto"}));
    call(&mut s, "leads", None, json!({"mode": "machine"}));
    let t = jobfile::load(&job).unwrap().parts[0].tech.unwrap();
    assert_eq!((t.joint_width, t.lead_in), (0.70, 13.0));
    // Thinner sheet, joints again: that range's gap.
    call(&mut s, "set-sheet", None, json!({"thickness": 3}));
    call(&mut s, "joints", None, json!({"mode": "auto"}));
    assert_eq!(jobfile::load(&job).unwrap().parts[0].tech.unwrap().joint_width, 0.35);
    // Between two ranges the part keeps its gap (the machine's own values, AM rules).
    call(&mut s, "set-sheet", None, json!({"thickness": 1.65}));
    call(&mut s, "joints", None, json!({"mode": "auto"}));
    assert_eq!(jobfile::load(&job).unwrap().parts[0].tech.unwrap().joint_width, 0.35);
}

// ---------------------------------------------------------------------------
// Kerf compensation through the agent operations (KC1–KC18).
// ---------------------------------------------------------------------------

#[test]
fn an_agent_reads_and_sets_the_kerf_compensation() {
    crate::ct1::set_thread_active(None);
    let d = dir("kerf_comp");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "mild steel", "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "plate-hole", "w": 100, "h": 60, "qty": 1}));
    call(&mut s, "nest", None, json!({}));
    // The machine's defaults: control compensation, hole D1, outside D2.
    let r = call(&mut s, "compensation", None, json!({}));
    assert!(r["data"]["status"].as_str().unwrap().contains("control (G41/G42, D1/D2)"), "{r}");
    assert_eq!(r["data"]["job"]["control"], "machine");
    let words: Vec<&str> = r["data"]["contours"].as_array().unwrap().iter().map(|c| c["word"].as_str().unwrap()).collect();
    assert_eq!(words, vec!["G41 D1", "G42 D2"], "{r}");
    // Switch the job to system compensation: offset path, no words.
    let r = call(&mut s, "compensation", None, json!({"control": "off", "system": "on"}));
    assert!(r["message"].as_str().unwrap().starts_with("Compensation set."), "{r}");
    assert!(r["data"]["contours"].as_array().unwrap().iter().all(|c| c["word"].is_null() && c["offset_path"] == true), "{r}");
    let saved = jobfile::load(&job).unwrap();
    assert_eq!(saved.spec.kerf, crate::kerf::KerfChoice { control: Some(false), system: Some(true) });
    let prog = call(&mut s, "program", None, json!({}));
    let text = prog["data"]["program"].as_str().or(prog["message"].as_str()).unwrap_or_default().to_string();
    assert!(!format!("{prog}").contains("G42") && format!("{prog}").contains("OFFSET PATH"), "{text}");
    // Back to the machine's own settings.
    call(&mut s, "compensation", None, json!({"control": "machine", "system": "machine"}));
    assert_eq!(jobfile::load(&job).unwrap().spec.kerf, crate::kerf::KerfChoice::default());
    let e = err(&mut s, "compensation", json!({"system": "maybe"}));
    assert!(e.contains("on, off or machine") || e.contains("maybe"), "{e}");
}

#[test]
fn verify_lists_a_hole_too_small_for_the_kerf() {
    // A plasma-like kerf of 25 mm: the 24 mm hole can't be cut with an offset path.
    crate::ct1::set_thread_active(Some(crate::ct1::MachineTech::from_text("B116 ON\nB117 OFF\nB002 12.5\n")));
    let d = dir("kerf_small_hole");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "mild steel", "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "plate-hole", "w": 100, "h": 60, "qty": 1}));
    call(&mut s, "nest", None, json!({}));
    let r = call(&mut s, "compensation", None, json!({}));
    assert_eq!(r["data"]["not_cut"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(r["data"]["not_cut"][0]["hole"], true);
    assert_eq!(r["data"]["contours"].as_array().unwrap().len(), 1, "only the outside is cut: {r}");
    let v = call(&mut s, "verify", None, json!({}));
    assert!(format!("{v}").contains("no bigger than the 25.00 mm kerf"), "{v}");
    crate::ct1::set_thread_active(None);
}

#[test]
fn an_agent_arranges_parts_by_hand() {
    // HM1–HM10 through the operations: place, slide to the next part, nudge,
    // move and copy a group, turn and mirror it.
    let d = dir("arrange");
    let jp = d.join("a.snjob");
    let jp = jp.to_str().unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(jp), json!({"w": 1000, "h": 500, "margin": 10, "gap": 6.35}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 100, "h": 40, "qty": 3}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 100}));
    call(&mut s, "place", None, json!({"part": 1, "x": 600, "y": 110}));
    call(&mut s, "place", None, json!({"part": 1, "x": 600, "y": 300}));
    let at = |n: usize| {
        let j = jobfile::load(Path::new(jp)).unwrap();
        j.sheets[0][n]
    };
    // Ctrl+arrow: #2 slides left to one part gap from #1.
    let r = call(&mut s, "slide", None, json!({"item": 2, "dir": "left"}));
    assert!(r["message"].as_str().unwrap().contains("meets the next part"), "{r}");
    assert!((at(1).x - 156.35).abs() < 0.01, "{:?}", at(1));
    assert!(err(&mut s, "slide", json!({"item": 2, "dir": "left"})).contains("part gap"));
    // Alt+arrow: by a distance.
    call(&mut s, "slide", None, json!({"items": "2,3", "dir": "up", "by": 6.35}));
    assert!((at(1).y - 116.35).abs() < 0.01 && (at(2).y - 306.35).abs() < 0.01);
    // A group move keeps the layout; refused onto another part.
    call(&mut s, "move", None, json!({"items": "2-3", "dx": 100}));
    assert!((at(1).x - 256.35).abs() < 0.01 && (at(2).x - 700.0).abs() < 0.01);
    assert!(err(&mut s, "move", json!({"items": "2,3", "x": 60, "y": 100})).contains("overlap"));
    // Turn and mirror a group about its centre.
    call(&mut s, "turn", None, json!({"items": "1,2", "angle": -90}));
    assert!(at(0).turns == 3 && at(1).turns == 3);
    assert!(err(&mut s, "turn", json!({"item": 1, "angle": 45})).contains("90"));
    assert!(err(&mut s, "mirror", json!({"items": "1,2"})).contains("mirrored"));
    call(&mut s, "edit-part", None, json!({"part": 1, "mirror": true}));
    call(&mut s, "mirror", None, json!({"items": "1,2", "axis": "y"}));
    assert!(at(0).mirror && at(1).mirror);
    // Copy a group: three new parts, every part clear of the others.
    let r = call(&mut s, "copy", None, json!({"items": "1-3"}));
    assert!(r["message"].as_str().unwrap().contains("#4–#6"), "{r}");
    let j = jobfile::load(Path::new(jp)).unwrap();
    assert_eq!(j.sheets[0].len(), 6);
    assert_eq!(j.parts[0].qty, 6, "each copy adds one to the order");
    for (i, a) in j.sheets[0].iter().enumerate() {
        for b in &j.sheets[0][i + 1..] {
            assert!(!crate::nest::clash_exact(&j.parts, a, b, 6.35 - 1e-6), "{a:?} {b:?}");
        }
    }
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// Machining time and speeds (CTM1–CTM16, TT1–TT10) through the agent operations.
// ---------------------------------------------------------------------------

#[test]
fn an_agent_sees_the_table_speeds_and_the_time_split() {
    let mut mt = crate::ct1::MachineTech::from_text("A051 40000\nA058 3000\n");
    let time = crate::cuttime::TimeSettings::from_text("D014 3\nD017 2\nD018 2\nH006 0.7\nP001 0 999999 10000 10000 10000 10000 114000 140000\n");
    mt.speed_files = std::sync::Arc::new(crate::techtable::SpeedFiles { time, table: Some(crate::techtable::Table::from_text(crate::techtable::demo_table())), fields: Default::default(), gas: 1 });
    crate::ct1::set_thread_active(Some(mt));
    let d = dir("speeds");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"material": "stainless", "thickness": 3, "gap": 5, "margin": 5}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 200, "h": 100, "qty": 2}));
    call(&mut s, "nest", None, json!({}));
    // The machine's table record for stainless 2.7–3.2 mm with gas 1.
    let r = call(&mut s, "machine-tech", None, json!({}));
    let sp = &r["data"]["speeds"];
    assert!(sp["source"].as_str().unwrap().contains("technology table"), "{r}");
    assert_eq!((sp["cut_mm_min_by_quality"][0].as_f64(), sp["cut_mm_min_by_quality"][2].as_f64()), (Some(1500.0), Some(9500.0)));
    assert_eq!((sp["mark_mm_min"].as_f64(), sp["rapid_mm_min"].as_f64(), sp["gas"].as_u64()), (Some(7500.0), Some(40000.0), Some(1)));
    // The sheet's time by kind adds up to its time; with no quality of their
    // own and no size bands, contours are cut at quality 3 (CQ1): 9500 mm/min.
    let info = call(&mut s, "info", None, json!({}));
    let sh = &info["data"]["sheets"][0];
    let t = &sh["times_s"];
    let sum: f64 = ["cut", "mark", "rapid", "pierce", "head"].iter().map(|k| t[k].as_f64().unwrap()).sum();
    assert!((sum - sh["time_s"].as_f64().unwrap()).abs() < 0.3, "{sh}");
    // Two 600 mm outsides at 9500 mm/min ≈ 7.6 s of cutting; two head-ups of 0.7 s.
    assert!((t["cut"].as_f64().unwrap() - 7.6).abs() < 0.5, "{t}");
    assert!((t["head"].as_f64().unwrap() - 1.4).abs() < 0.05, "{t}");
    crate::ct1::set_thread_active(None);
}

#[test]
fn turning_back_steps_to_the_previous_allowed_turn() {
    // QR3: back from 0° is 270° (a quarter clockwise); with only 0° and 180°
    // allowed, either way goes to 180°.
    let mut p = Part { name: "P".into(), shape: Shape::Rectangle, w: 100.0, h: 40.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
    let at = Placed { part: 0, x: 0.0, y: 0.0, w: 100.0, h: 40.0, turns: 0, mirror: false };
    let b = step_turn(&p, at, true).unwrap();
    assert_eq!((b.turns, b.w, b.h), (3, 40.0, 100.0));
    assert_eq!(step_turn(&p, at, false).unwrap().turns, 1);
    p.rots.plain = [true, false, true, false];
    assert_eq!(step_turn(&p, at, true).unwrap().turns, 2);
    p.rotate = false;
    assert!(step_turn(&p, at, true).is_err());
}

#[test]
fn overlapping_parts_are_reported_and_the_export_warns() {
    // QR6: parts cut through each other are named per sheet; touching counts,
    // a part gap of 0 is meant to touch and isn't reported.
    let d = dir("overlap");
    let p = Part { name: "Plate".into(), shape: Shape::Rectangle, w: 100.0, h: 100.0, qty: 3, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
    let at = |x: f64| Placed { part: 0, x, y: 10.0, w: 100.0, h: 100.0, turns: 0, mirror: false };
    let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
    let parts = vec![p];
    let clear = vec![at(10.0), at(115.0)];
    assert!(overlapping(&parts, &clear, &spec).is_empty());
    let bad = vec![at(10.0), at(115.0), at(150.0)];
    assert_eq!(overlapping(&parts, &bad, &spec), vec![(1, 2)]);
    assert_eq!(overlapping(&parts, &[at(10.0), at(110.0)], &spec), vec![(0, 1)], "touching");
    assert!(overlapping(&parts, &[at(10.0), at(110.0)], &SheetSpec { gap: 0.0, ..spec.clone() }).is_empty());
    let sheets = vec![clear, bad];
    let w = overlap_warning(&parts, &sheets, |_| spec.clone()).unwrap();
    assert!(w.contains("sheet 2: #2 Plate and #3 Plate overlap") && !w.contains("sheet 1"), "{w}");
    let formats = vec![SheetFormat::rect(1000.0, 500.0); 2];
    let plans = plans(&parts, &sheets, &formats, &spec, &[]);
    let ex = export(&d, "DEMO-001", &parts, &sheets, &formats, &plans, &spec, &[], &[], &crate::post::PostConfig::default(), None, 0.0).unwrap();
    assert!(ex.warning.as_deref().is_some_and(|w| w.contains("overlap")), "{:?}", ex.warning);
    assert!(ex.files.len() >= 3, "the programs are still written");
}

#[test]
fn an_agent_turns_a_part_back_to_its_previous_turn() {
    // QR3 through the operations: rotate back=true is the quick ring's Turn back.
    let d = dir("turnback");
    let job = d.join("turn.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1500, "h": 1000}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 100, "h": 40, "qty": 1}));
    call(&mut s, "nest", None, json!({}));
    let angle = |s: &mut Session| call(s, "info", None, json!({"sheet": 1}))["data"]["sheets"][0]["placed"][0]["angle"].as_f64().unwrap();
    let a0 = angle(&mut s);
    call(&mut s, "rotate", None, json!({"sheet": 1, "item": 1, "back": true}));
    assert_eq!(angle(&mut s), (a0 + 270.0) % 360.0, "a quarter clockwise");
    call(&mut s, "rotate", None, json!({"sheet": 1, "item": 1}));
    assert_eq!(angle(&mut s), a0, "and the next turn brings it back");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_agent_imports_dstv_and_essi_drawings() {
    let d = dir("formats");
    let job = d.join("plates.snjob");
    let jp = job.to_str().unwrap();
    // A made-up DSTV plate: 300 × 200, 10 mm S355, 3 off, one round hole.
    let nc1 = "ST\n** demo\n  DEMO-001\n  D-10\n  1\n  P7\n  S355J2\n  3\n  PL10*200\n  B\n    300.00\n    200.00\n     10.00\n     10.00\n     10.00\n\
      0.00\n     15.70\n      0.000\n      0.000\n      0.000\n      0.000\n      0.000\n  \n  \n  \n  \n\
AK\n  v       0.00u      0.00       0.00\n        300.00       0.00       0.00\n        300.00     200.00       0.00\n          0.00     200.00       0.00\n          0.00       0.00       0.00\n\
BO\n  v     150.00s    100.00      22.00       0.00\nKO\n  v     100.00u    150.00       0.00\n        150.00     150.00       0.00\nEN\n";
    std::fs::write(d.join("DEMO-PLATE.nc1"), nc1).unwrap();
    // A made-up ESSI program (0.1 mm units): a 100 × 50 rectangle cut with the oxy torch.
    std::fs::write(d.join("DEMO-RECT.CNC"), "%\n7\n+1000+0\n+0+500\n-1000+0\n+0-500\n8\n0\n").unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(jp), json!({"w": 1500, "h": 1000, "material": "steel", "thickness": 10}));

    // The DSTV marking block starts as mark.
    let layers = call(&mut s, "drawing-layers", None, json!({"path": d.join("DEMO-PLATE.nc1").to_str().unwrap()}));
    assert_eq!(layers["data"]["default_roles"][crate::formats::dstv::KO], "Mark", "{layers}");

    let paths = format!("{};{}", d.join("DEMO-PLATE.nc1").display(), d.join("DEMO-RECT.CNC").display());
    let r = call(&mut s, "import-drawing", None, json!({"path": paths}));
    let added = r["data"]["added"].as_array().unwrap();
    assert_eq!(added.len(), 2, "{r}");
    // DS8: the quantity comes from the file's header.
    assert_eq!((added[0]["w"].as_f64(), added[0]["h"].as_f64(), added[0]["qty"].as_u64(), added[0]["holes"].as_u64()), (Some(300.0), Some(200.0), Some(3), Some(1)), "{r}");
    // ES2: 0.1 mm per program unit.
    assert_eq!((added[1]["w"].as_f64(), added[1]["h"].as_f64(), added[1]["qty"].as_u64()), (Some(100.0), Some(50.0), Some(1)), "{r}");
    // S355J2 is a mild-steel grade and the plate is 10 mm: nothing to report.
    assert!(!r["data"]["notes"].to_string().contains("the file says"), "{r}");
    let saved = jobfile::load(&job).unwrap();
    assert_eq!(saved.parts.len(), 2);
    assert!(saved.parts[0].custom.as_ref().is_some_and(|c| !c.marks.is_empty()), "the KO marking comes in as marking");

    // A file that is no drawing is refused in plain words.
    std::fs::write(d.join("notes.txt"), "hello").unwrap();
    let e = err(&mut s, "import-drawing", json!({"path": d.join("notes.txt").to_str().unwrap()}));
    assert!(e.contains("not a drawing"), "{e}");
}

#[test]
fn parts_of_one_file_keep_their_own_names() {
    // With name format [Reference]-[Quantity], "DEMO-2" (the second part of DEMO)
    // must not read as reference DEMO, quantity 2: part data comes from the file name.
    let mut cfg = AppSettings::default();
    cfg.name_format = "[Reference]-[Quantity]".into();
    let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
    let square = |x: f64| crate::dxf::Piece { layer: "0".into(), pts: vec![[x, 0.0], [x + 10.0, 0.0], [x + 10.0, 10.0], [x, 10.0]], closed: true };
    let part = |x: f64| crate::formats::ReadPart { drawing: crate::dxf::Drawing { pieces: vec![square(x)], ..Default::default() }, meta: Default::default(), mark_layers: vec![] };
    let inc = crate::formats::incoming("DEMO", vec![part(0.0), part(50.0)]);
    let names: Vec<(String, u32)> = inc
        .iter()
        .map(|i| {
            let (p, _) = part_from_read(&cfg, &spec, &i.file, &i.name, &i.part.drawing, &i.part.meta, &Default::default(), 0).unwrap();
            (p.name, p.qty)
        })
        .collect();
    assert_eq!(names, vec![("DEMO-1".to_string(), 1), ("DEMO-2".to_string(), 1)]);
    // A one-part file still reads its name: BRACKET-40 is BRACKET, 40 off.
    let one = crate::formats::incoming("BRACKET-40", vec![part(0.0)]);
    let (p, _) = part_from_read(&cfg, &spec, &one[0].file, &one[0].name, &one[0].part.drawing, &one[0].part.meta, &Default::default(), 0).unwrap();
    assert_eq!((p.name.as_str(), p.qty), ("BRACKET", 40));
    // Steel grades count as the material of their alias.
    assert_eq!(material_like(&cfg, "S355J2"), Some(0));
    assert_eq!(material_like(&cfg, "Unobtainium"), None);
}

#[test]
fn the_xml_import_names_what_a_drawing_lost_and_refuses_multi_part_files() {
    let d = dir("xmlnotes");
    // A plate whose second object is damaged, and a Lantek file with two different parts.
    std::fs::write(d.join("plate.dxf"), "0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n50\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\nbad\n21\n5\n0\nENDSEC\n0\nEOF\n").unwrap();
    std::fs::write(d.join("nest.mec"), "0 FILEMECLT 7.00\n3 0 0 0 50 50 40\n3 1 0 0 200 50 30\n999\n").unwrap();
    let xml = d.join("order.xml");
    std::fs::write(
        &xml,
        r#"<?xml version="1.0"?><Import>
  <Product Reference="DEMO-1" Material="Mild steel" Thickness="3" Quantity="2" Fich="plate.dxf"/>
  <Product Reference="DEMO-2" Material="Mild steel" Thickness="3" Quantity="2" Fich="nest.mec"/>
</Import>"#,
    )
    .unwrap();
    let im = imports::xml_import(&AppSettings::default(), None, &xml, 0).unwrap();
    let notes = im.notes.join(" | ");
    assert!(notes.contains("DEMO-1: ignored damaged LINE"), "{notes}");
    assert!(notes.contains("DEMO-2") && notes.contains("holds 2 parts"), "{notes}");
    assert_eq!(im.added, 1);
}

#[test]
fn export_stops_on_a_part_drawn_wrong_or_a_pierce_off_the_sheet() {
    let d = dir("stops");
    let job = d.join("stops.snjob");
    let jp = job.to_str().unwrap();
    let out = d.join("cnc");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(jp), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 5}));

    // MAC-46, VP2: a plate flush in the sheet's corner with no margin; its
    // machine lead sits on that corner and can only shorten (not move), so it
    // still starts off the sheet.
    call(&mut s, "set-sheet", None, json!({"margin": 0}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 100, "h": 100, "qty": 1, "name": "Edge"}));
    call(&mut s, "place", None, json!({"part": 1, "x": 0, "y": 0}));
    call(&mut s, "leads", None, json!({"mode": "machine"}));
    let v = call(&mut s, "verify", None, json!({}));
    assert!(v.to_string().contains("pierces off the sheet"), "{v}");
    let e = err(&mut s, "export", json!({"dir": out.to_str().unwrap()}));
    assert!(e.contains("Export stopped: sheet 1") && e.contains("pierces off the sheet") && e.contains("margin"), "{e}");
    assert!(err(&mut s, "program", json!({"sheet": 1})).contains("pierces off the sheet"));
    // Moved in, the same part exports.
    call(&mut s, "move", None, json!({"sheet": 1, "item": 1, "x": 50, "y": 50}));
    call(&mut s, "export", None, json!({"dir": out.to_str().unwrap()}));

    // JOB-41, VP1: a plate whose two holes overlap is refused, and so are the
    // job's earlier programs (they'd be run by mistake).
    let sq = "0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n200\n20\n0\n10\n200\n20\n100\n10\n0\n20\n100\n";
    let c = |x: f64| format!("0\nCIRCLE\n8\n0\n10\n{x}\n20\n50\n30\n0\n40\n20\n");
    let dxf = d.join("HOLES.dxf");
    std::fs::write(&dxf, format!("0\nSECTION\n2\nENTITIES\n{sq}{}{}0\nENDSEC\n0\nEOF\n", c(80.0), c(110.0))).unwrap();
    call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "qty": 1}));
    call(&mut s, "place", None, json!({"part": 2, "x": 300, "y": 50}));
    call(&mut s, "leads", None, json!({"mode": "machine"}));
    let v = call(&mut s, "verify", None, json!({}));
    assert!(v.to_string().contains("holes 1 and 2 overlap"), "{v}");
    let e = err(&mut s, "export", json!({"dir": out.to_str().unwrap()}));
    assert!(e.contains("part 'HOLES': holes 1 and 2 overlap") && e.contains("No program of this job is left"), "{e}");
}

#[test]
fn an_agents_stock_change_waits_for_another_pcs_change_and_both_count() {
    let d = dir("stock_locked");
    let ctx = stock_ctx(&d);
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "S1", "w": 2000, "h": 1000, "thickness": 3, "qty": 1}));
    // The window on another PC is in the middle of a stock change: it has
    // read the stock (1 on the rack) and is about to store 6.
    let (read, go) = std::sync::mpsc::channel();
    let (waiting, agent_waits) = std::sync::mpsc::channel::<()>();
    let other = std::thread::spawn(move || {
        crate::stock::Stock::update(&crate::library::Library::at(&lib, &Who::new("kim", "PROG-2"), &Clock::pc()), |st| {
            read.send(()).unwrap();
            agent_waits.recv_timeout(std::time::Duration::from_secs(60)).expect("the agent waits for the lock");
            st.change_qty("S1", 5)
        })
    });
    go.recv().unwrap();
    crate::shared::hooks::set(move |s| {
        if s == "lock busy" {
            let _ = waiting.send(());
        }
    });
    let r = call(&mut s, "stock", None, json!({"mode": "add", "ref": "S1", "qty": 1}));
    crate::shared::hooks::clear();
    other.join().unwrap().unwrap();
    // The agent's change waited for the other one and counted on top of it.
    assert_eq!(r["message"], "'S1': 7 in stock.", "{r}");
    let l = call(&mut s, "stock", None, json!({"mode": "list"}));
    assert_eq!(l["data"]["sheets"][0]["qty"], 7, "{l}");
}

/// Every file under `dir` with its size, to see that nothing was touched.
fn listing(dir: &Path) -> Vec<(String, u64)> {
    let mut out = vec![];
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for f in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let m = f.metadata().unwrap();
            if m.is_dir() {
                todo.push(f.path());
            } else {
                out.push((f.path().strip_prefix(dir).unwrap().to_string_lossy().into_owned(), m.len()));
            }
        }
    }
    out.sort();
    out
}

/// LB11, LB13: with the Library folder missing, every library and stock
/// operation is refused, naming the folder and what to do instead; the job
/// file is as it was, and no folder is made.
#[test]
fn agents_are_refused_naming_the_folder_while_the_library_cant_be_reached() {
    let d = dir("unreachable");
    let mut ctx = Ctx::builtin();
    ctx.library_dir = d.join("Nesting");
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    let jp = d.join("DEMO-001.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 200, "h": 100, "qty": 2, "name": "DEMO-PLATE"}));
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let job_before = std::fs::read_to_string(&jp).unwrap();
    let said = format!("The Library on {} can't be reached", lib.display());
    for (op, args) in [
        ("library", json!({"mode": "list"})),
        ("library", json!({"mode": "add", "name": "DEMO-PLATE"})),
        ("library", json!({"mode": "save-part", "parts": "1"})),
        ("library", json!({"mode": "save-job"})),
        ("stock", json!({"mode": "list"})),
        ("stock", json!({"mode": "create", "w": 2000, "h": 1000, "thickness": 3})),
        ("stock", json!({"mode": "add", "ref": "S1"})),
        ("stock-book", json!({"mode": "status"})),
        ("stock-book", json!({"mode": "book"})),
    ] {
        let e = err(&mut s, op, args.clone());
        assert!(e.contains(&said), "{op} {args}: {e}");
        assert!(e.contains("try again once it can be reached"), "{op} {args} doesn't say what to do: {e}");
        assert!(!lib.exists(), "{op} {args} made the Library folder");
    }
    assert_eq!(std::fs::read_to_string(&jp).unwrap(), job_before, "a refused operation changed the job file");
    // Do All still nests on the job's sheet, and says the stock wasn't used.
    let r = call(&mut s, "nest", None, json!({}));
    let m = r["message"].as_str().unwrap();
    assert!(m.contains("The sheet stock wasn't used") && m.contains(&said), "{m}");
    assert!(!lib.exists(), "nesting made the Library folder");
    // The two-PC check on a test folder whose share is down: said so, and
    // nothing is made in its place.
    let gone = d.join("Share").join("check");
    let e = err(&mut s, "library-check", json!({"folder": gone.to_str().unwrap(), "rounds": 1}));
    assert!(e.contains(&gone.display().to_string()) && e.contains("can't be reached"), "{e}");
    assert!(!d.join("Share").exists(), "the check made the folders of a share that's down");
}

/// JX16 for agents: saving a job into the Library under a name another job
/// holds (in another folder, in other capitals) is refused, naming that
/// job's folder and saying what to do instead, and nothing is stored.
/// Renamed, it saves.
#[test]
fn an_agent_saving_into_the_library_under_a_name_another_job_holds_is_refused() {
    let d = dir("unique_job_names");
    let mut s = Session::new(stock_ctx(&d));
    // A programmer saved DEMO-001 frames in folder Acme.
    let spec = SheetSpec { w: 3000.0, h: 1500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
    let mut theirs = JobFile::new_job("DEMO-001 frames", spec);
    theirs.week = "Acme".into();
    crate::library::Library::open(&s.ctx.library_dir, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap().save_job(&theirs, crate::library::SaveTo::New).unwrap();
    let job = d.join("frames.snjob");
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 3000, "h": 1500, "material": "steel", "thickness": 3}));
    call(&mut s, "rename", None, json!({"name": "demo-001 FRAMES", "week": "W12"}));
    let e = err(&mut s, "library", json!({"mode": "save-job"}));
    assert!(e.contains("'DEMO-001 frames'") && e.contains("folder Acme") && e.contains("Nothing was saved") && e.contains("rename"), "{e}");
    let names = |s: &mut Session| call(s, "library", None, json!({"mode": "list"}))["data"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(names(&mut s), ["DEMO-001 frames"]);
    call(&mut s, "rename", None, json!({"name": "DEMO-002 frames"}));
    call(&mut s, "library", None, json!({"mode": "save-job"}));
    assert_eq!(names(&mut s), ["DEMO-001 frames", "DEMO-002 frames"]);
    // A Library saved before names were unique: jobs 1 and 3 share a name.
    // The list marks each with the other's number, as the window does
    // (JX17); the others carry no mark.
    let lib = crate::library::Library::open(&s.ctx.library_dir, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
    let mut twin = theirs.clone();
    twin.name = "demo-001 FRAMES".into();
    std::fs::write(lib.path(crate::library::Kind::Job, 3), serde_json::to_string(&twin).unwrap()).unwrap();
    lib.rebuild_index().unwrap();
    let list = call(&mut s, "library", None, json!({"mode": "list"}))["data"].clone();
    let marks: Vec<(u64, Value)> = list.as_array().unwrap().iter().map(|e| (e["id"].as_u64().unwrap(), e["same_name_as"].clone())).collect();
    assert_eq!(marks, [(1, json!([3])), (2, Value::Null), (3, json!([1]))]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_library_check_refuses_a_real_library() {
    let d = dir("check_refuses");
    let mut s = Session::new(Ctx::builtin());
    // A Library with a real job in it.
    let real = d.join("Library");
    let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
    crate::library::create(&real).unwrap();
    crate::library::Library::open(&real, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap().save_job(&JobFile::new_job("Frames", spec), crate::library::SaveTo::New).unwrap();
    let before = listing(&real);
    let e = err(&mut s, "library-check", json!({"folder": real.to_str().unwrap(), "rounds": 2}));
    assert!(e.contains("real Library") && e.contains("empty"), "{e}");
    assert_eq!(listing(&real), before, "the check touched a real Library");
    // A folder with other files in it is refused the same way.
    let other = d.join("Drawings");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("bracket.dxf"), "0\nEOF\n").unwrap();
    let e = err(&mut s, "library-check", json!({"folder": other.to_str().unwrap(), "rounds": 2}));
    assert!(e.contains("real Library"), "{e}");
    assert_eq!(listing(&other).len(), 1);
    // So is this PC's own Library, even while it's still empty.
    let own = s.ctx.library_dir.clone();
    let _ = std::fs::remove_dir_all(&own);
    std::fs::create_dir_all(&own).unwrap();
    let e = err(&mut s, "library-check", json!({"folder": own.to_str().unwrap(), "rounds": 2}));
    assert!(e.contains("this PC's Library"), "{e}");
    assert!(listing(&own).is_empty());
    let _ = std::fs::remove_dir_all(&own);
}

#[test]
fn the_library_check_passes_on_a_test_folder_and_fails_on_a_half_written_record() {
    let d = dir("check_runs");
    let mut s = Session::new(Ctx::builtin());
    let folder = d.join("check");
    let r = call(&mut s, "library-check", None, json!({"folder": folder.to_str().unwrap(), "rounds": 6, "pc": "PROG-1"}));
    let report = r["message"].as_str().unwrap();
    assert!(report.contains("PASS") && report.contains("PROG-1") && report.contains("Checked: 6 job(s), 6 library part(s)"), "{report}");
    // Run again on the folder it made: it checks the first run's records too.
    let r = call(&mut s, "library-check", None, json!({"folder": folder.to_str().unwrap(), "rounds": 3, "pc": "PROG-2"}));
    let report = r["message"].as_str().unwrap();
    // Each run's own folder and one folder a round (the spare ones are deleted again).
    assert!(report.contains("PASS") && report.contains("Checked: 9 job(s), 9 library part(s), 2 stock sheet(s) and 11 job folder(s) from 2 run(s) (PROG-1, PROG-2)"), "{report}");
    // Round 1: its own folder, then each round a folder made, renamed and a job
    // moved in, a spare made and deleted; every second round a folder nested.
    assert!(report.contains("3 stock change(s), 17 job folder change(s)"), "{report}");
    // Holds (CU12): taken on each round's job, renewed, and all let go.
    assert!(report.contains("3 hold(s) taken"), "{report}");
    assert_eq!(std::fs::read_dir(folder.join(crate::engine::inuse::FOLDER)).map_or(0, |d| d.count()), 0, "a finished run left a hold");
    // A job record cut off halfway through, as a dropped connection would leave it.
    let lib = crate::library::Library::open(&folder, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
    let rec = lib.path(crate::library::Kind::Job, 2);
    let text = std::fs::read_to_string(&rec).unwrap();
    std::fs::write(&rec, &text[..text.len() / 2]).unwrap();
    // Folder changes lost, as a PC writing the folders without the lock
    // would lose them: a rename undone and a job's place dropped.
    let file = folder.join(crate::folders::file_name(crate::library::Kind::Job));
    let mut t = crate::folders::load(&lib, crate::library::Kind::Job).unwrap();
    let renamed = t.folders.iter().position(|f| f.name.ends_with(" round 1 renamed")).unwrap();
    t.folders[renamed].name = "DEMO-CHECK round 1".into();
    t.placed.remove(&3);
    std::fs::write(&file, serde_json::to_string(&t).unwrap()).unwrap();
    // A library part's place in the part folders lost the same way.
    let parts = folder.join(crate::folders::file_name(crate::library::Kind::Product));
    let mut t = crate::folders::load(&lib, crate::library::Kind::Product).unwrap();
    t.placed.remove(&2);
    std::fs::write(&parts, serde_json::to_string(&t).unwrap()).unwrap();
    // A hold PROG-1's finished run never let go of.
    let left = crate::engine::inuse::Hold { format: crate::engine::inuse::FORMAT.into(), job: 4, name: "DEMO-CHECK left".into(), by: Who::new(&crate::shared::user_name(), "PROG-1"), since: 0, renewed: 0, beat: 1, taken_from: vec![] };
    std::fs::write(crate::engine::inuse::path(&folder, 4), serde_json::to_string(&left).unwrap()).unwrap();
    let e = err(&mut s, "library-check", json!({"folder": folder.to_str().unwrap(), "rounds": 1, "pc": "PROG-1"}));
    assert!(e.contains("FAIL") && e.contains("Job record 2") && e.contains("half-written"), "{e}");
    assert!(e.contains("is 'DEMO-CHECK round 1' in") && e.contains("a folder change was lost"), "{e}");
    assert!(e.contains("(record 3) is in the top of the Library where PROG-1 left it in 'DEMO-CHECK PROG-1"), "{e}");
    assert!(e.contains("(record 2) is in the top of the Library where PROG-1's save filed it in 'DEMO-CHECK': its place in the part folders was lost"), "{e}");
    assert!(e.contains("A hold on 'DEMO-CHECK left' (Holds/4.json) was left by") && e.contains("whose run has finished"), "{e}");
}

/// A test folder the Library check made itself (one round, so it holds the
/// check's own records, list, stock and log).
fn check_folder(s: &mut Session, d: &Path, name: &str) -> PathBuf {
    let f = d.join(name);
    call(s, "library-check", None, json!({"folder": f.to_str().unwrap(), "rounds": 1}));
    f
}

#[test]
fn the_library_check_refuses_a_folder_it_cant_read_or_recognise() {
    let d = dir("check_unsure");
    let mut s = Session::new(Ctx::builtin());
    let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
    let refused = |s: &mut Session, f: &Path, why: &str| {
        let before = listing(f);
        let e = err(s, "library-check", json!({"folder": f.to_str().unwrap(), "rounds": 1}));
        assert!(e.contains(why), "{e}");
        assert_eq!(listing(f), before, "the check touched a folder it refused");
    };
    // A real job was saved into a folder the check made, and the list was
    // lost since: the record itself gives it away.
    let f = check_folder(&mut s, &d, "lost list");
    crate::library::Library::open(&f, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap().save_job(&JobFile::new_job("Frames", spec.clone()), crate::library::SaveTo::New).unwrap();
    std::fs::remove_file(f.join(crate::library::INDEX)).unwrap();
    refused(&mut s, &f, "'Frames'");
    // A real job's record is there but missing from the list.
    let f = check_folder(&mut s, &d, "unlisted");
    let lib = crate::library::Library::open(&f, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
    let list = std::fs::read(f.join(crate::library::INDEX)).unwrap();
    lib.save_job(&JobFile::new_job("Frames", spec.clone()), crate::library::SaveTo::New).unwrap();
    std::fs::write(f.join(crate::library::INDEX), list).unwrap();
    refused(&mut s, &f, "'Frames'");
    // The list can't be read.
    let f = check_folder(&mut s, &d, "damaged list");
    std::fs::write(f.join(crate::library::INDEX), "{ half a list").unwrap();
    refused(&mut s, &f, "can't be read");
    // The stock can't be read.
    let f = check_folder(&mut s, &d, "damaged stock");
    std::fs::write(crate::stock::Stock::file(&f), "{ half a stock").unwrap();
    refused(&mut s, &f, "can't be read");
    // A record no run of the check saved, and that can't be read.
    let f = check_folder(&mut s, &d, "unknown record");
    std::fs::write(f.join("Jobs").join("0").join("99.snjob"), "{ \"name\": \"Fra").unwrap();
    refused(&mut s, &f, "record 99");
    // Something else the check doesn't make.
    let f = check_folder(&mut s, &d, "other files");
    std::fs::create_dir_all(f.join("Drawings")).unwrap();
    refused(&mut s, &f, "'Drawings'");
    // A job folder the check didn't make, or folders that can't be read.
    let f = check_folder(&mut s, &d, "real folder");
    let lib = crate::library::Library::open(&f, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
    crate::folders::change(&lib, crate::library::Kind::Job, |t, _| t.make(crate::folders::TOP, "Acme")).unwrap();
    refused(&mut s, &f, "the job folder 'Acme'");
    let f = check_folder(&mut s, &d, "damaged folders");
    std::fs::write(f.join(crate::folders::file_name(crate::library::Kind::Job)), "{ half a tree").unwrap();
    refused(&mut s, &f, "folder tree");
    // The same for the part folders (JX18).
    let f = check_folder(&mut s, &d, "real part folder");
    let lib = crate::library::Library::open(&f, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
    crate::folders::change(&lib, crate::library::Kind::Product, |t, _| t.make(crate::folders::TOP, "Acme")).unwrap();
    refused(&mut s, &f, "the part folder 'Acme'");
    let f = check_folder(&mut s, &d, "damaged part folders");
    std::fs::write(f.join(crate::folders::file_name(crate::library::Kind::Product)), "{ half a tree").unwrap();
    refused(&mut s, &f, "part-folders.json");
}

/// The 200 × 100 mm outline of the contour-state tests' made-up plates.
const DEMO_SQUARE: &str = "0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n200\n20\n0\n10\n200\n20\n100\n10\n0\n20\n100\n";

/// A made-up plate for the contour-state tests: a 200 × 100 mm outline, a
/// 30 mm hole and two equal 10 mm holes, a straight line across the plate from
/// edge to edge, and a short stray line on it.
fn contour_plate(d: &Path) -> PathBuf {
    let sq = DEMO_SQUARE.to_string();
    let circle = |x: f64, y: f64, r: f64| format!("0\nCIRCLE\n8\n0\n10\n{x}\n20\n{y}\n30\n0\n40\n{r}\n");
    let line = |a: [f64; 2], b: [f64; 2]| format!("0\nLINE\n8\n0\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]);
    let body = [sq, circle(40.0, 50.0, 15.0), circle(150.0, 25.0, 5.0), circle(150.0, 75.0, 5.0), line([100.0, 0.0], [100.0, 100.0]), line([20.0, 85.0], [70.0, 85.0])].concat();
    let p = d.join("DEMO-PLATE.dxf");
    std::fs::write(&p, format!("0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n")).unwrap();
    p
}

/// A part's cut contours as part-info lists them: the closed contours set to
/// Cut, the only ones with a cut quality. A mark line, a No cut or a Bend line
/// is a contour too (CS1), so tests about qualities read these, not all.
fn cut_contours(pi: &Value) -> Vec<Value> {
    pi["data"]["contours"].as_array().unwrap_or_else(|| panic!("{pi}")).iter().filter(|c| c["state"] == "cut" && c["closed"] == true).cloned().collect()
}

/// A part's contours as part-info lists them: (number, kind, state, length mm).
fn contour_rows(s: &mut Session, part: usize) -> Vec<(u64, String, String, f64)> {
    let pi = call(s, "part-info", None, json!({"part": part}));
    pi["data"]["contours"]
        .as_array()
        .unwrap_or_else(|| panic!("{pi}"))
        .iter()
        .map(|c| (c["contour"].as_u64().unwrap(), c["kind"].as_str().unwrap().to_string(), c["state"].as_str().unwrap_or_else(|| panic!("no state: {c}")).to_string(), c["length_mm"].as_f64().unwrap()))
        .collect()
}

#[test]
fn open_lines_come_in_as_bend_lines_or_no_cut_and_are_never_cut() {
    let d = dir("contour_states");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1}));

    // CS2: closed contours start as Cut; the straight line from edge to edge is
    // a Bend line, the stray line No cut. Holes first, the outline, then open lines.
    let rows = contour_rows(&mut s, 1);
    let kinds: Vec<(&str, &str)> = rows.iter().map(|r| (r.1.as_str(), r.2.as_str())).collect();
    assert_eq!(kinds, [("hole", "cut"), ("hole", "cut"), ("hole", "cut"), ("outside", "cut"), ("open", "bend-line"), ("open", "no-cut")], "{rows:?}");
    assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6]);
    assert!((rows[4].3 - 100.0).abs() < 0.1 && (rows[5].3 - 50.0).abs() < 0.1, "{rows:?}");

    // CS3: neither is cut, pierced, timed or checked: four pierces (outline and
    // three holes), 600 mm of outline plus the holes, and Verify Part is clean.
    let pi = call(&mut s, "part-info", None, json!({"part": 1}));
    assert_eq!(pi["data"]["machining"]["pierces"], 4, "{pi}");
    let cut = pi["data"]["cut_len_mm"].as_f64().unwrap();
    assert!((750.0..758.0).contains(&cut), "cut {cut} mm");
    assert_eq!(pi["data"]["machining"]["times_s"]["mark"], 0.0, "{pi}");
    assert_eq!(pi["data"]["verify_part"], json!([]), "{pi}");
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    let code = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    assert!(!code.contains("MARK ON"), "{code}");
    assert_eq!(code.matches("M07").count(), 4, "{code}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn contour_tech_marks_a_bend_line_leaves_out_a_hole_and_sets_every_equal_hole() {
    let d = dir("contour_tech");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    let states = |s: &mut Session| contour_rows(s, 1).into_iter().map(|r| r.2).collect::<Vec<_>>();
    let program = |s: &mut Session| call(s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();

    // CS4: the Bend line (contour 5) made a Mark is marked before the part is
    // cut, along x = 100 of the part (150 on the sheet).
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "5", "state": "mark"}));
    assert_eq!(states(&mut s), ["cut", "cut", "cut", "cut", "mark", "no-cut"]);
    let code = program(&mut s);
    assert_eq!(code.matches("MARK ON").count(), 1, "{code}");
    let mark = &code[code.find("MARK ON").unwrap()..code.find("MARK OFF").unwrap()];
    assert!(mark.contains("X150.00"), "{mark}");
    let pi = call(&mut s, "part-info", None, json!({"part": 1}));
    assert!(pi["data"]["machining"]["times_s"]["mark"].as_f64().unwrap() > 0.0, "{pi}");

    // The 30 mm hole left out is gone from the part and the program; it keeps
    // its number, so it can be set back.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": 1, "state": "leave-out"}));
    assert_eq!(states(&mut s), ["leave-out", "cut", "cut", "cut", "mark", "no-cut"]);
    assert_eq!(program(&mut s).matches("M07").count(), 3);
    let pi = call(&mut s, "part-info", None, json!({"part": 1}));
    // The plate less two 10 mm holes, each 16 straight pieces (DX2).
    let small = 8.0 * 25.0 * (std::f64::consts::PI / 8.0).sin();
    assert!((pi["data"]["area_mm2"].as_f64().unwrap() - (20000.0 - 2.0 * small)).abs() < 0.5, "{pi}");
    // Elements name their contour by the same numbers (the holes are 2 and 3 now).
    let on: std::collections::BTreeSet<u64> = pi["data"]["elements"].as_array().unwrap().iter().map(|e| e["contour"].as_u64().unwrap()).collect();
    assert_eq!(on, [2, 3, 4].into());

    // CS5: same=true marks every hole equal to hole 2; only the outline is cut.
    let r = call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "2", "state": "mark", "same": true}));
    assert!(r["message"].as_str().unwrap().contains("2, 3"), "{r}");
    assert_eq!(states(&mut s), ["leave-out", "mark", "mark", "cut", "mark", "no-cut"]);
    let code = program(&mut s);
    assert_eq!((code.matches("M07").count(), code.matches("MARK ON").count()), (1, 3), "{code}");
    // ... and back to Cut.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1-3", "state": "cut"}));
    assert_eq!(states(&mut s), ["cut", "cut", "cut", "cut", "mark", "no-cut"]);
    assert_eq!(program(&mut s).matches("M07").count(), 4);

    // The outline is always cut, and an open line can't be cut yet: refused,
    // saying why, and nothing changes.
    let e = err(&mut s, "contour-tech", json!({"part": 1, "contours": "4", "state": "mark"}));
    assert!(e.contains("outline") && e.contains("no part"), "{e}");
    let e = err(&mut s, "contour-tech", json!({"part": 1, "contours": "6", "state": "cut"}));
    assert!(e.contains("open line") && e.contains("split"), "{e}");
    let e = err(&mut s, "contour-tech", json!({"part": 1, "contours": "7", "state": "mark"}));
    assert!(e.contains("6 contours") && e.contains("part-info"), "{e}");
    assert_eq!(states(&mut s), ["cut", "cut", "cut", "cut", "mark", "no-cut"]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn contour_states_are_kept_in_jobs_and_products_and_old_jobs_open_as_before() {
    let d = dir("contour_keep");
    let job = d.join("plate.snjob");
    let jp = job.to_str().unwrap();
    let ctx = || {
        let mut c = Ctx::builtin();
        c.cfg.library_dir = d.join("library").to_string_lossy().into_owned();
        c.library_dir = d.join("library");
        crate::library::create(&c.library_dir).unwrap();
        c
    };
    let mut s = Session::new(ctx());
    call(&mut s, "new", Some(jp), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1, "name": "DEMO-001"}));
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "5", "state": "mark"}));
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "2", "state": "no-cut"}));
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "state": "leave-out"}));
    let set = ["leave-out", "no-cut", "cut", "cut", "mark", "no-cut"];
    let states = |s: &mut Session, part: usize| contour_rows(s, part).into_iter().map(|r| r.2).collect::<Vec<_>>();

    // The job reopened from its file keeps every state.
    let mut s = Session::new(ctx());
    call(&mut s, "info", Some(jp), json!({}));
    assert_eq!(states(&mut s, 1), set);
    // So does a product saved to the library and added to a job again.
    call(&mut s, "library", None, json!({"mode": "save-part", "parts": "1"}));
    call(&mut s, "library", None, json!({"mode": "add", "name": "DEMO-001", "qty": 2}));
    assert_eq!(states(&mut s, 2), set);
    call(&mut s, "place", None, json!({"part": 2, "x": 50, "y": 50}));
    let code = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    assert_eq!((code.matches("M07").count(), code.matches("MARK ON").count()), (2, 1), "{code}");

    // A job saved before contour states (no contour list) opens as it was: its
    // holes and outline Cut, its marking lines Mark, the same program.
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&job).unwrap()).unwrap();
    for p in v["parts"].as_array_mut().unwrap() {
        p["custom"].as_object_mut().unwrap().remove("contours");
    }
    std::fs::write(&job, serde_json::to_string(&v).unwrap()).unwrap();
    let mut s = Session::new(ctx());
    call(&mut s, "info", Some(jp), json!({}));
    assert_eq!(states(&mut s, 2), ["cut", "cut", "mark"], "the cut hole, the outline and the mark");
    assert_eq!(call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap(), code);
    // ... and its contours can be set from there.
    call(&mut s, "contour-tech", None, json!({"part": 2, "contours": "1", "state": "mark"}));
    assert_eq!(states(&mut s, 2), ["mark", "cut", "mark"]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn open_lines_start_by_layer_line_type_and_shape() {
    let d = dir("contour_start");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    let sq = DEMO_SQUARE.to_string();
    let line = |layer: &str, ltype: &str, a: [f64; 2], b: [f64; 2]| format!("0\nLINE\n8\n{layer}\n{ltype}10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]);
    // From the left edge and back to it, but not straight.
    let crooked = "0\nLWPOLYLINE\n8\n0\n90\n3\n70\n0\n10\n0\n20\n50\n10\n50\n20\n70\n10\n0\n20\n90\n".to_string();
    let body = [
        sq,
        crooked,
        line("BEND-UP", "", [20.0, 20.0], [60.0, 20.0]),
        line("0", "6\nDASHED\n", [20.0, 40.0], [50.0, 40.0]),
        // A 40 mm square hole drawn dashed, line by line.
        line("0", "6\nDASHED\n", [120.0, 20.0], [160.0, 20.0]),
        line("0", "6\nDASHED\n", [160.0, 20.0], [160.0, 60.0]),
        line("0", "6\nDASHED\n", [160.0, 60.0], [120.0, 60.0]),
        line("0", "6\nDASHED\n", [120.0, 60.0], [120.0, 20.0]),
        line("MARK", "", [20.0, 60.0], [40.0, 60.0]),
        line("0", "", [300.0, 0.0], [400.0, 0.0]),
    ]
    .concat();
    let dxf = d.join("DEMO-LINES.dxf");
    std::fs::write(&dxf, format!("0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n")).unwrap();
    let by_length = |s: &mut Session, part: usize| -> std::collections::BTreeMap<i64, String> { contour_rows(s, part).into_iter().map(|r| (r.3.round() as i64, r.2)).collect() };

    // CS2: layer roles first — the dashed line's row starts as left out (DX3),
    // the marking layer's line is a Mark; then a bend layer's line is a Bend
    // line, a line that isn't straight is No cut, and one off the part is left out.
    let r = call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "layers": "MARK=mark"}));
    let notes = r["data"]["notes"].to_string();
    assert!(notes.contains("1 open piece(s) off the part left out") && notes.contains("2 open line(s) kept"), "{notes}");
    let want: std::collections::BTreeMap<i64, String> = [(600, "cut"), (108, "no-cut"), (40, "bend-line"), (20, "mark")].into_iter().map(|(l, st)| (l, st.to_string())).collect();
    assert_eq!(by_length(&mut s, 1), want);
    // The dashed row set to Cut (DX3): its square closes and is a cut hole,
    // its open line a Bend line, never cut.
    call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "layers": "MARK=mark,0 [DASHED]=cut"}));
    let got = by_length(&mut s, 2);
    assert_eq!((got.get(&160).map(String::as_str), got.get(&30).map(String::as_str)), (Some("cut"), Some("bend-line")), "{got:?}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_line_across_corners_stays_a_bend_line_and_the_outline_stays_whole() {
    // S1: a 200 × 100 plate drawn as four lines, with a straight line on the
    // same layer ending on its corners (or one corner and the far edge). The
    // outline is the whole rectangle and the line a Bend line, whatever order
    // the drawing lists them in.
    let d = dir("contour_branch");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    let line = |a: [f64; 2], b: [f64; 2]| format!("0\nLINE\n8\n0\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]);
    let edges = [line([0.0, 0.0], [200.0, 0.0]), line([200.0, 0.0], [200.0, 100.0]), line([200.0, 100.0], [0.0, 100.0]), line([0.0, 100.0], [0.0, 0.0])];
    let corner_to_corner = line([0.0, 0.0], [200.0, 100.0]);
    let corner_to_edge = line([0.0, 0.0], [200.0, 50.0]);
    let cases = [(&corner_to_corner, 0, 224), (&corner_to_corner, 2, 224), (&corner_to_corner, 4, 224), (&corner_to_edge, 0, 206), (&corner_to_edge, 4, 206)];
    for (n, (across, at, len)) in cases.into_iter().enumerate() {
        let mut body = edges.to_vec();
        body.insert(at, across.clone());
        let dxf = d.join(format!("DEMO-{n}.dxf"));
        std::fs::write(&dxf, format!("0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n", body.concat())).unwrap();
        call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "qty": 1}));
        let rows = contour_rows(&mut s, n + 1);
        let got: Vec<(&str, &str, i64)> = rows.iter().map(|r| (r.1.as_str(), r.2.as_str(), r.3.round() as i64)).collect();
        assert_eq!(got, [("outside", "cut", 600), ("open", "bend-line", len)], "case {n}: line at {at}");
        let pi = call(&mut s, "part-info", None, json!({"part": n + 1}));
        assert_eq!((pi["data"]["w"].as_f64(), pi["data"]["h"].as_f64(), pi["data"]["machining"]["pierces"].as_u64()), (Some(200.0), Some(100.0), Some(1)), "case {n}: {pi}");
    }
    // As a flat pattern is often drawn: the edges split where the bend line meets them.
    let split = [
        line([0.0, 0.0], [100.0, 0.0]), line([100.0, 100.0], [100.0, 0.0]), line([100.0, 0.0], [200.0, 0.0]), line([200.0, 0.0], [200.0, 100.0]),
        line([200.0, 100.0], [100.0, 100.0]), line([100.0, 100.0], [0.0, 100.0]), line([0.0, 100.0], [0.0, 0.0]),
    ];
    let dxf = d.join("DEMO-SPLIT.dxf");
    std::fs::write(&dxf, format!("0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n", split.concat())).unwrap();
    call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "qty": 1}));
    let mut part = cases.len() + 1;
    let got: Vec<(String, String, i64)> = contour_rows(&mut s, part).into_iter().map(|r| (r.1, r.2, r.3.round() as i64)).collect();
    assert_eq!(got, [("outside".to_string(), "cut".to_string(), 600), ("open".to_string(), "bend-line".to_string(), 100)]);

    // With a 0.2 mm gap at a corner (closed by IM8): where the cross line
    // doesn't meet it, and where it does; the cross line first and last.
    let gap_far = [line([0.0, 0.0], [100.0, 0.0]), line([100.0, 0.0], [100.0, 100.0]), line([100.0, 100.0], [0.0, 100.0]), line([0.0, 99.8], [0.0, 0.0])];
    let gap_near = [line([0.0, 0.0], [100.0, 0.0]), line([100.0, 0.0], [100.0, 100.0]), line([100.0, 100.0], [0.0, 100.0]), line([0.0, 100.0], [0.0, 0.2])];
    let across = line([0.0, 0.0], [100.0, 100.0]);
    for (edges, at) in [(&gap_far, 0), (&gap_far, 4), (&gap_near, 0), (&gap_near, 4)] {
        let mut body = edges.to_vec();
        body.insert(at, across.clone());
        part += 1;
        let dxf = d.join(format!("DEMO-GAP-{part}.dxf"));
        std::fs::write(&dxf, format!("0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n", body.concat())).unwrap();
        let r = call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "qty": 1}));
        assert!(r["data"]["notes"].to_string().contains("1 gap(s) up to 0.5 mm closed"), "part {part}: {r}");
        let got: Vec<(String, String, i64)> = contour_rows(&mut s, part).into_iter().map(|r| (r.1, r.2, r.3.round() as i64)).collect();
        assert_eq!(got, [("outside".to_string(), "cut".to_string(), 400), ("open".to_string(), "bend-line".to_string(), 141)], "part {part}");
        let pi = call(&mut s, "part-info", None, json!({"part": part}));
        assert!((pi["data"]["area_mm2"].as_f64().unwrap() - 10000.0).abs() < 1.0, "part {part}: {pi}");
        assert_eq!(pi["data"]["verify_part"], json!([]), "part {part}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// The machine's numbered cut qualities on the LC5 (MAC-34, MTL10, BL22, BL23).
// ---------------------------------------------------------------------------

/// A made-up laser for 12.9–20.1 mm mild steel: size bands send equivalent
/// radii below 5 mm to quality 1, 5–12 mm to quality 2 and the rest to 3; its
/// technology table pierces quality 1 with code 1, 2 with 4 and 3 with 2.
fn thick_plate_laser() {
    let mut mt = crate::ct1::MachineTech::from_text("A051 40000\nA058 3000\nZ000 \"##MATEXP0010##\"\nZ001 12.9\nZ002 20.1\nZ010 0\nZ011 5\nZ012 0\nZ010 5\nZ011 12\nZ012 1\nZ010 12\nZ011 99999\nZ012 2\nZ999 0\n");
    let table = "#- LASER -#\nW000 3\nA000 \"##MATEXP0010##\"\nA001 12.9\nA002 20.1\nA005 3\nA008 1\nC009 3000\nD007 600\nD026 1\nD057 900\nD076 4\nD107 1200\nD126 2\n";
    mt.speed_files = std::sync::Arc::new(crate::techtable::SpeedFiles { time: Default::default(), table: Some(crate::techtable::Table::from_text(table)), fields: Default::default(), gas: 3 });
    crate::ct1::set_thread_active(Some(mt));
}

/// A 300 × 200 plate with a Ø6 hole, a Ø16 hole and a line on layer MARK.
fn plate_with_two_holes(d: &Path) -> PathBuf {
    let sq = "0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n300\n20\n0\n10\n300\n20\n200\n10\n0\n20\n200\n";
    let hole = |x: f64, r: f64| format!("0\nCIRCLE\n8\n0\n10\n{x}\n20\n100\n30\n0\n40\n{r}\n");
    let mark = "0\nLWPOLYLINE\n8\nMARK\n90\n2\n70\n0\n10\n200\n20\n40\n10\n260\n20\n40\n";
    let dxf = d.join("DEMO-001.dxf");
    std::fs::write(&dxf, format!("0\nSECTION\n2\nENTITIES\n{sq}{}{}{mark}0\nENDSEC\n0\nEOF\n", hole(60.0, 3.0), hole(130.0, 8.0))).unwrap();
    dxf
}

/// (pierce, quality) of every NEXT_L in a program, sorted.
fn next_l_codes(text: &str) -> Vec<(u32, u32)> {
    let mut v: Vec<(u32, u32)> = text
        .lines()
        .filter_map(|l| l.strip_prefix("NEXT_L(")?.strip_suffix(')').map(|a| a.split(',').map(str::trim).collect::<Vec<_>>()))
        .filter(|a| a.len() == 5)
        .map(|a| (a[3].parse().unwrap(), a[4].parse().unwrap()))
        .collect();
    v.sort();
    v
}

#[test]
fn an_lc5_program_cuts_each_contour_at_its_own_quality_and_pierce() {
    thick_plate_laser();
    let d = dir("lc5_qualities");
    let job = d.join("plate.snjob");
    let mut ctx = Ctx::builtin();
    ctx.blm.on = true;
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 15}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_two_holes(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));

    // part-info: each contour's quality from the machine's size bands.
    let info = call(&mut s, "part-info", None, json!({"part": 1}));
    let mut q: Vec<(f64, u64)> = cut_contours(&info).iter().map(|c| (c["length_mm"].as_f64().unwrap(), c["quality"].as_u64().unwrap_or(0))).collect();
    q.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(q.iter().map(|c| c.1).collect::<Vec<_>>(), vec![1, 2, 3], "{info}");

    // The program: the Ø6 hole at quality 1 (pierce 1), the Ø16 hole at 2
    // (pierce 4), the outline at 3 (pierce 2), the mark at pierce 0, quality 9.
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    assert_eq!(next_l_codes(&text), vec![(0, 9), (1, 1), (2, 3), (4, 2)], "{text}");

    // BL23: the job sheet flags exactly the pierce codes and qualities used.
    let out = d.join("cnc");
    let e = call(&mut s, "export", None, json!({"dir": out.to_str().unwrap()}));
    let xml = e["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).find(|f| f.to_uppercase().ends_with(".XML")).unwrap().to_string();
    let xml = std::fs::read_to_string(xml).unwrap();
    let flagged = |block: &str| -> Vec<u32> {
        let b = xml.split(&format!("<{block}>")).nth(1).unwrap().split(&format!("</{block}>")).next().unwrap();
        b.lines().filter(|l| l.contains(">1<")).map(|l| l.trim().trim_start_matches("<Id").split('>').next().unwrap().parse().unwrap()).collect()
    };
    assert_eq!((flagged("Foratura"), flagged("Taglio")), (vec![0, 1, 2, 4], vec![1, 2, 3, 9]), "{xml}");
    crate::ct1::set_thread_active(None);
}

#[test]
fn a_job_saved_with_rough_normal_fine_opens_and_exports_with_numbered_qualities() {
    thick_plate_laser();
    let d = dir("old_qualities");
    let job = d.join("old.snjob");
    let jp = job.to_str().unwrap();
    let mut ctx = Ctx::builtin();
    ctx.blm.on = true;
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(jp), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 15}));
    let dxf = plate_with_two_holes(&d);
    for _ in 0..3 {
        call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    }
    call(&mut s, "cut-tech", None, json!({"mode": "own"}));
    call(&mut s, "nest", None, json!({"stock": "off"}));
    // The file as the app saved it before the machine's numbered qualities:
    // part 1 Normal, part 2 Rough with a Fine row up to r 5 mm and a Normal
    // row from 5 to 12 mm, part 3 Fine.
    let mut old: Value = serde_json::from_str(&std::fs::read_to_string(&job).unwrap()).unwrap();
    for (k, q) in ["Normal", "Rough", "Fine"].into_iter().enumerate() {
        old["parts"][k]["tech"]["quality"] = json!(q);
    }
    old["parts"][1]["tech"]["cut_ranges"][0] = json!({"on": true, "min_r": 0.0, "max_r": 5.0, "quality": "Fine"});
    old["parts"][1]["tech"]["cut_ranges"][1] = json!({"on": true, "min_r": 5.0, "max_r": 12.0, "quality": "Normal"});
    std::fs::write(&job, serde_json::to_string_pretty(&old).unwrap()).unwrap();

    // Normal leaves the quality to the machine's size bands; Rough is quality
    // 3 where the part's rows (Fine 1, Normal 2) don't hold a contour; Fine is
    // quality 1 on every contour (CQ2).
    let mut s = Session::new({
        let mut c = Ctx::builtin();
        c.blm.on = true;
        c
    });
    let quality = |s: &mut Session, part: usize| {
        let info = call(s, "part-info", Some(jp), json!({"part": part}));
        let mut q: Vec<(f64, u64, String)> = cut_contours(&info).iter().map(|c| (c["length_mm"].as_f64().unwrap(), c["quality"].as_u64().unwrap_or(0), c["quality_from"].as_str().unwrap_or("").to_string())).collect();
        q.sort_by(|a, b| a.0.total_cmp(&b.0));
        (info["data"]["technology"]["quality"].clone(), q.into_iter().map(|c| (c.1, c.2)).collect::<Vec<_>>())
    };
    let from = |v: &[(u64, &str)]| v.iter().map(|&(q, f)| (q, f.to_string())).collect::<Vec<_>>();
    assert_eq!(quality(&mut s, 1), (json!("auto"), from(&[(1, "machine size band"), (2, "machine size band"), (3, "machine size band")])));
    assert_eq!(quality(&mut s, 2), (json!(3), from(&[(1, "part size range"), (2, "part size range"), (3, "part")])));
    assert_eq!(quality(&mut s, 3), (json!(1), from(&[(1, "part"), (1, "part"), (1, "part")])));

    // And it still exports: each part's subprogram at its own codes.
    let text = call(&mut s, "program", Some(jp), json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    let mut subs: Vec<Vec<(u32, u32)>> = text.split("; ==== ").map(next_l_codes).filter(|c| !c.is_empty()).collect();
    subs.sort();
    assert_eq!(subs, vec![vec![(0, 9), (1, 1), (1, 1), (1, 1)], vec![(0, 9), (1, 1), (2, 3), (4, 2)], vec![(0, 9), (1, 1), (2, 3), (4, 2)]], "{text}");
    let out = d.join("cnc");
    call(&mut s, "export", Some(jp), json!({"dir": out.to_str().unwrap()}));

    // Agents give a number or auto; an old name is refused with what it is now.
    let e = err(&mut s, "tech-set", json!({"parts": "3", "quality": "fine"}));
    assert!(e.contains("1 to 12") && e.contains("fine are 3 / auto / 1"), "{e}");
    let e = err(&mut s, "cut-tech", json!({"mode": "ranges", "parts": "3", "ranges": "0-5:normal"}));
    assert!(e.contains("1 to 12") && e.contains("fine are 3 / 2 / 1"), "{e}");
    call(&mut s, "tech-set", None, json!({"parts": "3", "quality": 2}));
    assert_eq!(quality(&mut s, 3), (json!(2), from(&[(2, "part"), (2, "part"), (2, "part")])));
    crate::ct1::set_thread_active(None);
}

// ---------------------------------------------------------------------------
// A cut quality set by hand on one contour (TEC-4, CQ6).
// ---------------------------------------------------------------------------

/// A made-up 300 × 200 plate with two equal Ø40 holes (the thick-plate
/// laser's size bands cut them at quality 3) and a line on layer MARK.
/// Contours: 1 and 2 the holes, 3 the outline, 4 the mark.
fn plate_with_equal_holes(d: &Path) -> PathBuf {
    let sq = "0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n300\n20\n0\n10\n300\n20\n200\n10\n0\n20\n200\n";
    let hole = |x: f64| format!("0\nCIRCLE\n8\n0\n10\n{x}\n20\n100\n30\n0\n40\n20\n");
    let mark = "0\nLWPOLYLINE\n8\nMARK\n90\n2\n70\n0\n10\n200\n20\n40\n10\n260\n20\n40\n";
    let dxf = d.join("DEMO-002.dxf");
    std::fs::write(&dxf, format!("0\nSECTION\n2\nENTITIES\n{sq}{}{}{mark}0\nENDSEC\n0\nEOF\n", hole(80.0), hole(200.0))).unwrap();
    dxf
}

/// Each contour's (number, quality, where it came from) as part-info lists
/// them; contours without a quality show 0 and "".
fn contour_qualities(s: &mut Session, part: usize) -> Vec<(u64, u64, String)> {
    let pi = call(s, "part-info", None, json!({"part": part}));
    pi["data"]["contours"].as_array().unwrap_or_else(|| panic!("{pi}")).iter().map(|c| (c["contour"].as_u64().unwrap(), c["quality"].as_u64().unwrap_or(0), c["quality_from"].as_str().unwrap_or("").to_string())).collect()
}

#[test]
fn a_hole_given_a_quality_by_hand_goes_to_the_lc5_at_it_and_same_sets_its_twin() {
    thick_plate_laser();
    let d = dir("hand_quality");
    let job = d.join("plate.snjob");
    let mut ctx = Ctx::builtin();
    ctx.blm.on = true;
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 15}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_equal_holes(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let program = |s: &mut Session| next_l_codes(call(s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap());
    let band = |n: u64| (n, 3, "machine size band".to_string());
    // Both Ø40 holes and the outline at their size band's quality 3 (pierce 2).
    assert_eq!(program(&mut s), vec![(0, 9), (2, 3), (2, 3), (2, 3)]);

    // CQ6: hole 1 set to quality 1 by hand is written at quality 1 with its
    // pierce code (1); its twin, left alone, keeps its band's quality.
    let r = call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "quality": 1}));
    assert!(r["message"].as_str().unwrap().contains("quality 1"), "{r}");
    assert_eq!(contour_qualities(&mut s, 1), vec![(1, 1, "by hand".to_string()), band(2), band(3), (4, 0, String::new())]);
    assert_eq!(program(&mut s), vec![(0, 9), (1, 1), (2, 3), (2, 3)]);

    // same=true sets every equal hole: both holes now at quality 1.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "2", "quality": 1, "same": true}));
    assert_eq!(contour_qualities(&mut s, 1), vec![(1, 1, "by hand".to_string()), (2, 1, "by hand".to_string()), band(3), (4, 0, String::new())]);
    assert_eq!(program(&mut s), vec![(0, 9), (1, 1), (1, 1), (2, 3)]);
    // CQ4: part-info names a quality set by hand as the machine file does.
    crate::ct1::set_thread_active(Some(laser_install(&d.join("Named"), "A922 \"Slow\"\n")));
    let pi = call(&mut s, "part-info", None, json!({"part": 1}));
    let first = &pi["data"]["contours"][0];
    assert_eq!((first["quality_name"].as_str(), first["quality_from"].as_str()), (Some("Slow (1)"), Some("by hand")), "{pi}");
    crate::ct1::set_thread_active(None);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_mark_or_a_quality_the_machine_doesnt_have_is_refused_with_what_to_do_instead() {
    thick_plate_laser();
    let d = dir("hand_quality_refused");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 15}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_equal_holes(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "state": "mark"}));
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "2", "state": "no-cut"}));
    let before = contour_qualities(&mut s, 1);

    // A mark always uses the machine's marking quality: refused, closed or open.
    let e = err(&mut s, "contour-tech", json!({"part": 1, "contours": "1", "quality": 2}));
    assert!(e.contains("Contour 1 is a mark") && e.contains("marking quality") && e.contains("state=cut quality=2"), "{e}");
    let e = err(&mut s, "contour-tech", json!({"part": 1, "contours": "4", "quality": 2}));
    assert!(e.contains("Contour 4 is a mark") && e.contains("marking quality") && e.contains("open lines can't be cut yet"), "{e}");
    let e = err(&mut s, "contour-tech", json!({"part": 1, "contours": "3", "state": "mark", "quality": 1}));
    assert!(e.contains("marking quality") && e.contains("state=cut"), "{e}");
    // A contour that isn't cut takes no quality; cutting it with one is the way.
    let e = err(&mut s, "contour-tech", json!({"part": 1, "contours": "2", "quality": 2}));
    assert!(e.contains("No cut") && e.contains("state=cut quality=2"), "{e}");
    // The machine has qualities 1 to 12.
    for q in [json!(0), json!(13), json!("slow")] {
        let e = err(&mut s, "contour-tech", json!({"part": 1, "contours": "3", "quality": q}));
        assert!(e.contains("1 to 12") && e.contains("auto"), "{q}: {e}");
    }
    let e = err(&mut s, "contour-tech", json!({"part": 1, "contours": "3"}));
    assert!(e.contains("state") && e.contains("quality"), "{e}");
    // A built-in shape's contours are always cut at the part's qualities.
    call(&mut s, "add-shape", None, json!({"shape": "disc", "w": 200, "qty": 1}));
    let e = err(&mut s, "contour-tech", json!({"part": 2, "contours": "1", "quality": 1}));
    assert!(e.contains("built-in") && e.contains("import"), "{e}");
    assert_eq!(contour_qualities(&mut s, 1), before, "nothing changed");

    // state=cut with a quality is what to do instead: the mark is cut at 2.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "state": "cut", "quality": 2}));
    assert_eq!(contour_qualities(&mut s, 1)[0], (1, 2, "by hand".to_string()));
    // With same=true and only a quality, the equal hole that isn't cut (No
    // cut) is left as it is, and named.
    let r = call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "quality": 3, "same": true}));
    assert!(r["message"].as_str().unwrap().contains("2 (no-cut) left as it is"), "{r}");
    assert_eq!((r["data"]["contours"].clone(), r["data"]["skipped"].clone()), (json!([1]), json!([2])));
    assert_eq!(contour_rows(&mut s, 1)[1].2, "no-cut");
    assert_eq!(contour_qualities(&mut s, 1)[..2], [(1, 3, "by hand".to_string()), (2, 0, String::new())]);
    crate::ct1::set_thread_active(None);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn hand_set_qualities_beat_the_parts_own_and_are_kept_in_jobs_and_products() {
    thick_plate_laser();
    let d = dir("hand_quality_keep");
    let job = d.join("plate.snjob");
    let jp = job.to_str().unwrap();
    let ctx = || {
        let mut c = Ctx::builtin();
        c.blm.on = true;
        c.cfg.library_dir = d.join("library").to_string_lossy().into_owned();
        c.library_dir = d.join("library");
        crate::library::create(&c.library_dir).unwrap();
        c
    };
    let mut s = Session::new(ctx());
    call(&mut s, "new", Some(jp), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 15}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_equal_holes(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark", "name": "DEMO-002"}));
    // The part's own size range (up to r 50 mm at quality 2) and quality 4.
    call(&mut s, "cut-tech", None, json!({"mode": "ranges", "parts": "1", "ranges": "0-50:2"}));
    call(&mut s, "tech-set", None, json!({"parts": "1", "quality": 4}));
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "quality": 1}));
    let row = |n: u64, q: u64, from: &str| (n, q, from.to_string());
    let set = vec![row(1, 1, "by hand"), row(2, 2, "part size range"), row(3, 4, "part"), row(4, 0, "")];
    assert_eq!(contour_qualities(&mut s, 1), set, "CQ6: by hand first");
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let program = |s: &mut Session| call(s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    let code = program(&mut s);
    assert_eq!(next_l_codes(&code), vec![(0, 9), (1, 1), (2, 4), (4, 2)], "{code}");

    // The job reopened from its file keeps the quality set by hand, and its program.
    let mut s = Session::new(ctx());
    call(&mut s, "info", Some(jp), json!({}));
    assert_eq!(contour_qualities(&mut s, 1), set);
    assert_eq!(program(&mut s), code);
    // So does a product saved to the library and added to a job again.
    call(&mut s, "library", None, json!({"mode": "save-part", "parts": "1"}));
    call(&mut s, "library", None, json!({"mode": "add", "name": "DEMO-002", "qty": 1}));
    assert_eq!(contour_qualities(&mut s, 2), set);

    // auto gives the hole back to the part's size range.
    call(&mut s, "contour-tech", None, json!({"part": 2, "contours": "1", "quality": "auto"}));
    assert_eq!(contour_qualities(&mut s, 2)[0], row(1, 2, "part size range"));
    // A hole that stops being cut loses its quality set by hand: cut again,
    // the part's size range decides.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "state": "mark"}));
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "state": "cut"}));
    assert_eq!(contour_qualities(&mut s, 1)[0], row(1, 2, "part size range"));
    crate::ct1::set_thread_active(None);
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// The cut qualities by the machine's names and colours (CQ4, CQ5).
// ---------------------------------------------------------------------------

/// A made-up Lantek folder whose machine file starts with `names` (A922–A933
/// lines, Latin-1 like the original's files) and gives the thick-plate size
/// bands of `thick_plate_laser`; the machine as the app loads it from there.
fn laser_install(root: &Path, names: &str) -> crate::ct1::MachineTech {
    std::fs::create_dir_all(root.join("MchFiles")).unwrap();
    let ct1 = format!("{names}A051 40000\nA058 3000\nZ000 \"##MATEXP0010##\"\nZ001 12.9\nZ002 20.1\nZ010 0\nZ011 5\nZ012 0\nZ010 5\nZ011 12\nZ012 1\nZ010 12\nZ011 99999\nZ012 2\nZ999 0\n");
    let latin1: Vec<u8> = ct1.chars().map(|c| c as u32 as u8).collect();
    std::fs::write(root.join("MchFiles").join("PstAdi06.CT1"), latin1).unwrap();
    crate::ct1::MachineTech::for_machine(&crate::settings::Machine::default(), root.to_str().unwrap())
}

/// Each contour's quality as part-info shows it, smallest contour first.
fn contour_quality_names(s: &mut Session) -> Vec<String> {
    let info = call(s, "part-info", None, json!({"part": 1}));
    let mut q: Vec<(f64, String)> = cut_contours(&info).iter().map(|c| (c["length_mm"].as_f64().unwrap(), c["quality_name"].as_str().unwrap_or("").to_string())).collect();
    q.sort_by(|a, b| a.0.total_cmp(&b.0));
    q.into_iter().map(|c| c.1).collect()
}

#[test]
fn part_info_names_each_contours_quality_as_the_machine_file_does() {
    let d = dir("quality_names");
    let names = "A922 \"Slow\"                ; Nombre para la calidad de corte  1.\nA923 \"Medium\"              ; Nombre para la calidad de corte  2.\nA924 \"R\u{e1}pido  \"           ; Nombre para la calidad de corte  3.\nA930 \"Marking\"             ; Nombre para la calidad de corte  9.\n";
    crate::ct1::set_thread_active(Some(laser_install(&d.join("Named"), names)));
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(d.join("plate.snjob").to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 15}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_two_holes(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    // The Ø6 hole Slow (1), the Ø16 hole Medium (2), the outline Rápido (3).
    assert_eq!(contour_quality_names(&mut s), ["Slow (1)", "Medium (2)", "R\u{e1}pido (3)"]);

    // A machine file that leaves the names empty (as the shop's BLM file does).
    crate::ct1::set_thread_active(Some(laser_install(&d.join("Plain"), "A922 \"\"                    ; Nombre para la calidad de corte  1.\n")));
    assert_eq!(contour_quality_names(&mut s), ["Quality 1", "Quality 2", "Quality 3"]);
    crate::ct1::set_thread_active(None);
}

#[test]
fn machine_tech_lists_the_qualities_by_name_and_leaves_out_hidden_ones() {
    let d = dir("quality_list");
    let names = "A922 \"Slow\"\nA923 \"Medium\"\nA924 \"Fast\"\nA925 \"<HIDE>\"\nA930 \"Marking\"\nA931 \"Center\"\n";
    let mt = laser_install(&d.join("Expert"), names);
    crate::ct1::set_thread_active(Some(mt.clone()));
    let mut ctx = Ctx::builtin();
    ctx.tech = mt;
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(d.join("plate.snjob").to_str().unwrap()), json!({"material": "mild steel", "thickness": 15}));
    let r = call(&mut s, "machine-tech", None, json!({}));
    let q = r["data"]["qualities"].as_array().unwrap_or_else(|| panic!("{r}"));
    let labels: Vec<&str> = q.iter().map(|q| q["name"].as_str().unwrap()).collect();
    assert_eq!(labels, ["Slow (1)", "Medium (2)", "Fast (3)", "Quality 4", "Quality 5", "Quality 6", "Quality 7", "Quality 8", "Marking (9)", "Center (10)", "Quality 11", "Quality 12"]);
    // A quality the file names <HIDE> isn't offered in the pickers (the machine still has it).
    let offered: Vec<u64> = q.iter().filter(|q| q["offered"] == json!(true)).map(|q| q["quality"].as_u64().unwrap()).collect();
    assert_eq!(offered, [1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12]);
    // The machines list shows the active machine's names too.
    let m = call(&mut s, "machines", None, json!({}));
    assert_eq!(m["data"]["technology"]["qualities"][8]["name"], json!("Marking (9)"), "{m}");
    // MK1: each machine says what it does, in set-sheet's words.
    let does = m["data"]["machines"][0]["does"].as_str().unwrap_or_else(|| panic!("{m}"));
    assert!(crate::machine::MachineKind::from_key(does).is_some(), "{does}");
    crate::ct1::set_thread_active(None);
}

#[test]
fn each_quality_takes_the_colour_of_the_originals_colour_settings_else_its_shipped_one() {
    let d = dir("quality_colours");
    let colours = |s: &mut Session| -> Vec<String> {
        let r = call(s, "machine-tech", None, json!({}));
        r["data"]["qualities"].as_array().unwrap().iter().map(|q| q["colour"].as_str().unwrap_or("").to_string()).collect()
    };
    // A Lantek folder whose colour settings give qualities 1 and 3 their own colours.
    let root = d.join("Expert");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("LANTEKIN.CNI"), "#- Colores: Rojo Verde Azul Estilo Anchura -#\nC008 255 255 0 0 1 ;\nC041 200 10 20 0 1 ;\nC043   0 255   0 0 1       ; x\nC053 0 255 0 0 1 ;\n").unwrap();
    crate::ct1::set_thread_active(Some(laser_install(&root, "")));
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(d.join("plate.snjob").to_str().unwrap()), json!({"material": "mild steel", "thickness": 15}));
    assert_eq!(colours(&mut s), ["#c80a14", "#0080c0", "#00ff00", "#008000", "#800080", "#804040", "#0080ff", "#008080", "#004080", "#8080ff", "#80ffff", "#ff80ff"]);

    // No colour settings: the colours the original ships with.
    crate::ct1::set_thread_active(Some(laser_install(&d.join("Bare"), "")));
    assert_eq!(colours(&mut s)[..3], ["#ff8040", "#0080c0", "#ff00ff"]);
    crate::ct1::set_thread_active(None);
}

/// The made-up layered plate of `examples/drawings`, copied into `d`: a
/// 300 × 160 plate on layer 0 with a 40 mm hole (contour 1), two equal 12 mm
/// holes (2, 3), an etched frame on layer ETCH (4), the outline (5), a bend
/// line from edge to edge (6) and a stray open line (7); centre lines drawn
/// dashed (row `0 [CENTER]`) and an old hole on the frozen layer OLD-REV.
fn layered_plate(d: &Path) -> PathBuf {
    let p = d.join("DEMO-LAYERED-PLATE.dxf");
    std::fs::write(&p, include_str!("../../../examples/drawings/DEMO-LAYERED-PLATE.dxf")).unwrap();
    p
}

/// Each contour's state, and its quality where it is set by hand.
fn painted(s: &mut Session, part: usize) -> Vec<String> {
    let pi = call(s, "part-info", None, json!({"part": part}));
    pi["data"]["contours"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| match (c["state"].as_str().unwrap(), c["quality_from"].as_str()) {
            ("cut", Some("by hand")) => format!("cut {}", c["quality"]),
            (st, _) => st.to_string(),
        })
        .collect()
}

#[test]
fn import_drawing_sets_layers_and_contours_in_one_call() {
    let d = dir("import_choices");
    let job = d.join("plate.snjob");
    let plate = layered_plate(&d);
    let p = plate.to_str().unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));

    // IM12: drawing-layers numbers the contours the part will have, with the
    // layer roles given, so an agent can choose them before importing.
    let r = call(&mut s, "drawing-layers", None, json!({"path": p}));
    assert_eq!(r["data"]["default_roles"]["0 [CENTER]"], "Leave out", "{r}");
    assert_eq!(r["data"]["default_roles"]["OLD-REV [frozen]"], "Leave out", "{r}");
    let states = |r: &Value| r["data"]["sizes"][0]["contours"].as_array().unwrap_or_else(|| panic!("{r}")).iter().map(|c| c["state"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(states(&r), ["cut", "cut", "cut", "cut", "cut", "bend-line", "no-cut"]);
    let r = call(&mut s, "drawing-layers", None, json!({"path": p, "layers": "ETCH=mark"}));
    assert_eq!(states(&r), ["cut", "cut", "cut", "mark", "cut", "bend-line", "no-cut"]);

    // One call: the etch layer marked, then the stray line marked, the big
    // hole left out and every hole equal to the first small one cut Slow (1).
    let r = call(&mut s, "import-drawing", None, json!({"path": p, "qty": 1, "layers": "ETCH=mark", "contours": "7=mark; 1=leave-out; 2=quality 1 same"}));
    assert_eq!(painted(&mut s, 1), ["leave-out", "cut 1", "cut 1", "mark", "cut", "bend-line", "mark"], "{r}");

    // A choice the part can't take is refused as contour-tech refuses it, and
    // nothing is imported.
    let e = err(&mut s, "import-drawing", json!({"path": p, "contours": "5=mark"}));
    assert!(e.contains("outline") && e.contains("always cut"), "{e}");
    let e = err(&mut s, "import-drawing", json!({"path": p, "contours": "8=mark"}));
    assert!(e.contains("7 contours"), "{e}");
    let e = err(&mut s, "import-drawing", json!({"path": p, "contours": "1=purple"}));
    assert!(e.contains("cut, mark, no-cut, bend-line"), "{e}");
    // Contour numbers belong to one part: two drawings at once are refused.
    let e = err(&mut s, "import-drawing", json!({"path": format!("{p};{p}"), "contours": "1=mark"}));
    assert!(e.contains("one drawing"), "{e}");
    let info = call(&mut s, "info", None, json!({}));
    assert!(info["message"].as_str().unwrap().contains("1 part type(s)"), "{info}");
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// A part's cut order set by hand (MAC-5, CO3–CO7).
// ---------------------------------------------------------------------------

/// While it lives, the test's thread works on a machine whose file holds
/// `codes` (made up); then on the built-in machine again.
struct OnMachine;

fn on_machine(codes: &str) -> OnMachine {
    crate::ct1::set_thread_active(Some(crate::ct1::MachineTech::from_text(codes)));
    OnMachine
}

impl Drop for OnMachine {
    fn drop(&mut self) {
        crate::ct1::set_thread_active(None);
    }
}

/// A machine that cuts the holes nobody ordered by hand in the part's own
/// order (Order mode of the holes None, CO14), as every part was cut before
/// the setting: for the tests of the order set by hand (CO3–CO10), which
/// name the automatic holes in number order.
fn holes_as_drawn() -> OnMachine {
    on_machine("B565 0\n")
}

/// A part's cut order as part-info gives it: the contours in the order they
/// are cut, and the ones set by hand.
fn part_cut_order(s: &mut Session, part: usize) -> (Vec<u64>, Vec<u64>) {
    let pi = call(s, "part-info", None, json!({"part": part}));
    let nums = |v: &Value| v.as_array().unwrap_or_else(|| panic!("{pi}")).iter().map(|n| n.as_u64().unwrap()).collect::<Vec<_>>();
    (nums(&pi["data"]["cut_order"]["contours"]), nums(&pi["data"]["cut_order"]["by_hand"]))
}

/// Each contour's (number, place in the cut order, set by hand) as part-info
/// lists them; a contour that is neither cut nor marked has no place (0).
fn cut_places(s: &mut Session, part: usize) -> Vec<(u64, u64, bool)> {
    let pi = call(s, "part-info", None, json!({"part": part}));
    pi["data"]["contours"]
        .as_array()
        .unwrap_or_else(|| panic!("{pi}"))
        .iter()
        .map(|c| (c["contour"].as_u64().unwrap(), c["cut_order"].as_u64().unwrap_or(0), c["cut_order_by_hand"].as_bool().unwrap_or(false)))
        .collect()
}

#[test]
fn contours_ordered_by_hand_are_cut_first_and_the_outline_last() {
    let d = dir("part_order");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1}));
    // No order set by hand: the holes in number order, then the outline.
    assert_eq!(part_cut_order(&mut s, 1), (vec![1, 2, 3, 4], vec![]));
    // A start picked on hole 1 (element 1) and a quality set by hand on hole 3.
    call(&mut s, "lead-at", None, json!({"part": 1, "element": 1}));
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "3", "quality": 1}));
    let unchanged = |s: &mut Session| {
        let pi = call(s, "part-info", None, json!({"part": 1}));
        (pi["data"]["elements"].clone(), pi["data"]["technology"].clone(), contour_qualities(s, 1))
    };
    let before = unchanged(&mut s);

    // CO3: holes 3, 1, 2 by hand take places 1–3; the outline follows in the
    // automatic part. The bend line and the no-cut line have no place.
    let r = call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "3, 1, 2"}));
    assert_eq!(r["data"]["added"], json!([3, 1, 2]), "{r}");
    assert_eq!(cut_places(&mut s, 1), [(1, 2, true), (2, 3, true), (3, 1, true), (4, 4, false), (5, 0, false), (6, 0, false)]);
    assert_eq!(part_cut_order(&mut s, 1), (vec![3, 1, 2, 4], vec![3, 1, 2]));
    // Contour numbers don't change: elements, the picked start and the
    // quality set by hand stay on the same contours.
    assert_eq!(unchanged(&mut s), before);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_outline_is_always_cut_last_and_later_picks_go_in_before_it() {
    let _drawn = holes_as_drawn();
    let d = dir("part_order_outline");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1}));
    // The line across the plate (contour 5) marked: an etching.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "5", "state": "mark"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![5, 1, 2, 3, 4], vec![]), "marks first, as today");

    // CO4: a hole named after the outline in one command is refused with the
    // reason, and nothing changes; so is a mark (marks go before the outline).
    let e = err(&mut s, "part-cut-order", json!({"part": 1, "mode": "add", "contours": "4, 1"}));
    assert!(e.contains("Contour 4 is the part's outline") && e.contains("always cut last") && e.contains("drops before its holes are cut"), "{e}");
    let e = err(&mut s, "part-cut-order", json!({"part": 1, "mode": "add", "contours": "2, 4, 5"}));
    assert!(e.contains("contour 5 after it") && e.contains("always cut last"), "{e}");
    assert_eq!(part_cut_order(&mut s, 1), (vec![5, 1, 2, 3, 4], vec![]));

    // The outline named last is taken; it stays last, and what is added
    // later goes in just before it: a hole, then the mark.
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "2, 4"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![2, 5, 1, 3, 4], vec![2, 4]));
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "3"}));
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "5"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![2, 3, 5, 1, 4], vec![2, 3, 5, 4]));
    let _ = std::fs::remove_dir_all(&d);
}

/// A made-up 300 × 100 plate: contours 1–5 holes (Ø20, Ø10, Ø20, Ø10, Ø30
/// from left to right), 6 the outline, 7 a line on layer MARK.
fn plate_with_five_holes(d: &Path) -> PathBuf {
    let sq = "0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n300\n20\n0\n10\n300\n20\n100\n10\n0\n20\n100\n";
    let circle = |layer: &str, x: f64, r: f64| format!("0\nCIRCLE\n8\n{layer}\n10\n{x}\n20\n50\n30\n0\n40\n{r}\n");
    let line = "0\nLINE\n8\nMARK\n10\n20\n20\n85\n11\n80\n21\n85\n";
    let body = [sq.to_string(), circle("0", 40.0, 10.0), circle("0", 90.0, 5.0), circle("0", 140.0, 10.0), circle("0", 190.0, 5.0), circle("0", 250.0, 15.0), line.to_string()].concat();
    let p = d.join("DEMO-003.dxf");
    std::fs::write(&p, format!("0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n")).unwrap();
    p
}

#[test]
fn bulk_commands_add_only_contours_not_yet_ordered_in_the_automatic_order() {
    let _drawn = holes_as_drawn();
    let d = dir("part_order_bulk");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_five_holes(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    // Marks between the holes: the small holes 2 and 4 marked.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "2,4", "state": "mark"}));
    let rows: Vec<(String, String, u64)> = contour_rows(&mut s, 1).into_iter().map(|r| (r.1, r.2, r.3.round() as u64)).collect();
    let row = |k: &str, st: &str, l: u64| (k.to_string(), st.to_string(), l);
    assert_eq!(rows, [row("hole", "cut", 63), row("hole", "mark", 31), row("hole", "cut", 63), row("hole", "mark", 31), row("hole", "cut", 94), row("outside", "cut", 800), row("open", "mark", 60)]);
    // Holes 1 and 5 cut at quality 1 by hand; the rest at the default 3.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1,5", "quality": 1}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![2, 4, 7, 1, 3, 5, 6], vec![]), "marks first, then holes, then the outline");
    let order = |s: &mut Session, args: Value| {
        let mut a = args;
        a["part"] = json!(1);
        let r = call(s, "part-cut-order", None, a);
        (r["data"]["added"].clone(), r["data"]["skipped"].clone())
    };

    // CO5: All marks adds the marks not yet ordered, in the automatic order
    // (number order), after mark 4 set by hand.
    order(&mut s, json!({"mode": "add", "contours": "4"}));
    assert_eq!(order(&mut s, json!({"mode": "all-marks"})), (json!([2, 7]), json!([4])));
    assert_eq!(part_cut_order(&mut s, 1), (vec![4, 2, 7, 1, 3, 5, 6], vec![4, 2, 7]));
    // All of quality 1: holes 1 and 5.
    assert_eq!(order(&mut s, json!({"mode": "all-quality", "quality": 1})), (json!([1, 5]), json!([])));
    assert_eq!(part_cut_order(&mut s, 1), (vec![4, 2, 7, 1, 5, 3, 6], vec![4, 2, 7, 1, 5]));
    // A contour already in the order is skipped, and so is one named twice.
    assert_eq!(order(&mut s, json!({"mode": "add", "contours": "3, 1, 3"})), (json!([3]), json!([1, 3])));
    // All cut: only the outline is left.
    assert_eq!(order(&mut s, json!({"mode": "all-cut"})), (json!([6]), json!([1, 3, 5])));
    assert_eq!(part_cut_order(&mut s, 1), (vec![4, 2, 7, 1, 5, 3, 6], vec![4, 2, 7, 1, 5, 3, 6]));

    // Nothing left to add: nothing changes, and the job isn't saved again.
    let saved = std::fs::read_to_string(&job).unwrap();
    let r = call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "all-marks"}));
    assert!(r["message"].as_str().unwrap().contains("already in the cut order"), "{r}");
    assert_eq!(std::fs::read_to_string(&job).unwrap(), saved);
    // A quality no contour is cut at, or a part with no marks: refused, saying
    // what the part has.
    let e = err(&mut s, "part-cut-order", json!({"part": 1, "mode": "all-quality", "quality": 7}));
    assert!(e.contains("quality 7") && e.contains("1 and 3") && e.contains("part-info"), "{e}");
    for q in [json!(0), json!(13), json!("auto")] {
        let e = err(&mut s, "part-cut-order", json!({"part": 1, "mode": "all-quality", "quality": q}));
        assert!(e.contains("1 to 12"), "{q}: {e}");
    }
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1}));
    let e = err(&mut s, "part-cut-order", json!({"part": 2, "mode": "all-marks"}));
    assert!(e.contains("no marks") && e.contains("contour-tech"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn undo_takes_back_the_last_command_and_clear_empties_the_order() {
    let _drawn = holes_as_drawn();
    let d = dir("part_order_undo");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    let program = |s: &mut Session| call(s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    let before = program(&mut s);
    let add = |s: &mut Session, c: &str| call(s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": c}));

    // Nothing to undo yet: refused, saying so.
    let e = err(&mut s, "part-cut-order", json!({"part": 1, "mode": "undo"}));
    assert!(e.contains("no cut order set by hand") && e.contains("automatic"), "{e}");

    // CO6: undo takes back exactly the last command, the contours it added
    // in front of the outline too.
    add(&mut s, "2, 4");
    add(&mut s, "3, 1");
    assert_eq!(part_cut_order(&mut s, 1), (vec![2, 3, 1, 4], vec![2, 3, 1, 4]));
    let r = call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "undo"}));
    assert!(r["message"].as_str().unwrap().contains("3 and 1"), "{r}");
    assert_eq!(part_cut_order(&mut s, 1), (vec![2, 1, 3, 4], vec![2, 4]));
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "undo"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![1, 2, 3, 4], vec![]));
    assert!(err(&mut s, "part-cut-order", json!({"part": 1, "mode": "undo"})).contains("no cut order set by hand"));

    // Clear empties the whole order at once; on an empty order it changes nothing.
    add(&mut s, "3");
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "all-cut"}));
    let r = call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "clear"}));
    assert!(r["message"].as_str().unwrap().contains("automatic"), "{r}");
    assert_eq!(part_cut_order(&mut s, 1), (vec![1, 2, 3, 4], vec![]));
    let saved = std::fs::read_to_string(&job).unwrap();
    let r = call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "clear"}));
    assert!(r["message"].as_str().unwrap().contains("no cut order set by hand"), "{r}");
    assert_eq!(std::fs::read_to_string(&job).unwrap(), saved);
    // With no order set by hand the program is the one from before.
    assert_eq!(program(&mut s), before);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn contours_set_to_another_state_leave_the_order_and_new_cuts_join_the_automatic_part() {
    let _drawn = holes_as_drawn();
    let d = dir("part_order_tech");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_five_holes(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    let tech = |s: &mut Session, c: &str, st: &str| call(s, "contour-tech", None, json!({"part": 1, "contours": c, "state": st}));
    let add = |s: &mut Session, c: &str| call(s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": c}));

    // A contour left out (or No cut, or a Bend line) before ordering has no
    // place in the order: naming it is refused, saying what to do instead.
    tech(&mut s, "3", "leave-out");
    let e = err(&mut s, "part-cut-order", json!({"part": 1, "mode": "add", "contours": "1, 3"}));
    assert!(e.contains("Contour 3 is set to Leave out") && e.contains("contour-tech"), "{e}");
    assert_eq!(part_cut_order(&mut s, 1), (vec![7, 1, 2, 4, 5, 6], vec![]));
    // The open marked line is an etching: it can go first.
    add(&mut s, "7, 1, 2, 4, 5");
    assert_eq!(part_cut_order(&mut s, 1), (vec![7, 1, 2, 4, 5, 6], vec![7, 1, 2, 4, 5]));

    // CO7: set to No cut or Bend line, a contour leaves the order.
    tech(&mut s, "2", "no-cut");
    tech(&mut s, "4", "bend-line");
    assert_eq!(part_cut_order(&mut s, 1), (vec![7, 1, 5, 6], vec![7, 1, 5]));
    assert_eq!(cut_places(&mut s, 1)[1], (2, 0, false), "no place");
    // A cut hole set to Mark leaves the order and joins the automatic marks;
    // a contour newly set to Cut joins the automatic part (after the marks).
    tech(&mut s, "1", "mark");
    tech(&mut s, "3", "cut");
    assert_eq!(part_cut_order(&mut s, 1), (vec![7, 5, 1, 3, 6], vec![7, 5]));
    // An ordered mark set to Cut leaves the order too.
    add(&mut s, "1");
    tech(&mut s, "1", "cut");
    assert_eq!(part_cut_order(&mut s, 1), (vec![7, 5, 1, 3, 6], vec![7, 5]));
    // Setting a contour to the state it has, or only its quality, keeps it.
    tech(&mut s, "7", "mark");
    tech(&mut s, "5", "cut");
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "5", "quality": 2}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![7, 5, 1, 3, 6], vec![7, 5]));
    // The open mark set to No cut leaves; naming it again is refused.
    tech(&mut s, "7", "no-cut");
    assert_eq!(part_cut_order(&mut s, 1), (vec![5, 1, 3, 6], vec![5]));
    let e = err(&mut s, "part-cut-order", json!({"part": 1, "mode": "add", "contours": "7"}));
    assert!(e.contains("Contour 7 is set to No cut"), "{e}");
    // Undo takes back the command the order's contour 5 came from: the
    // contours that left with a technology change are gone from it already.
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "undo"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![1, 3, 5, 6], vec![]));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_built_in_shape_follows_the_automatic_order_and_ordering_it_is_refused() {
    let d = dir("part_order_shape");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "plate-hole", "w": 200, "h": 100, "qty": 1}));
    // Its hole, then its outline, in the automatic order.
    assert_eq!(part_cut_order(&mut s, 1), (vec![1, 2], vec![]));
    for args in [json!({"mode": "add", "contours": "1"}), json!({"mode": "add", "contours": "5"}), json!({"mode": "all-cut"}), json!({"mode": "undo"}), json!({"mode": "clear"})] {
        let mut a = args.clone();
        a["part"] = json!(1);
        let e = err(&mut s, "part-cut-order", a);
        assert!(e.contains("built-in") && e.contains("automatic order") && e.contains("export-part-dxf") && e.contains("import"), "{args}: {e}");
    }
    assert_eq!(part_cut_order(&mut s, 1), (vec![1, 2], vec![]));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_hand_set_order_is_kept_in_jobs_and_products_and_old_files_open_with_none() {
    let d = dir("part_order_keep");
    let job = d.join("plate.snjob");
    let jp = job.to_str().unwrap();
    let lib = d.join("library");
    crate::library::create(&lib).unwrap();
    let ctx = || {
        let mut c = Ctx::builtin();
        c.library_dir = lib.clone();
        c
    };
    let mut s = Session::new(ctx());
    call(&mut s, "new", Some(jp), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1, "name": "DEMO-001"}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    let program = |s: &mut Session| call(s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    let code = program(&mut s);
    assert!(!std::fs::read_to_string(&job).unwrap().contains("hand_order"), "no order set by hand, nothing written for it");
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "3, 1"}));
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "2"}));
    let set = (vec![3, 1, 2, 4], vec![3, 1, 2]);

    // The job reopened from its file keeps the order, command by command:
    // undo in a new run takes back the last one.
    let mut s = Session::new(ctx());
    call(&mut s, "info", Some(jp), json!({}));
    assert_eq!(part_cut_order(&mut s, 1), set);
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "undo"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![3, 1, 2, 4], vec![3, 1]));
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "2"}));
    // A product saved to the library and used in a new job keeps it.
    call(&mut s, "library", None, json!({"mode": "save-part", "parts": "1"}));
    let job2 = d.join("other.snjob");
    call(&mut s, "new", Some(job2.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "library", None, json!({"mode": "add", "name": "DEMO-001", "qty": 2}));
    assert_eq!(part_cut_order(&mut s, 1), set);

    // A job saved before cut orders (no order in its file) opens with none,
    // and writes the same program.
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&job).unwrap()).unwrap();
    v["parts"][0]["custom"].as_object_mut().unwrap().remove("hand_order").expect("the order was saved with the part");
    std::fs::write(&job, serde_json::to_string(&v).unwrap()).unwrap();
    let mut s = Session::new(ctx());
    call(&mut s, "info", Some(jp), json!({}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![1, 2, 3, 4], vec![]));
    assert_eq!(program(&mut s), code);
    // So does a product saved before.
    let products = d.join("library").join("Products");
    let file = walk(&products).into_iter().find(|p| p.extension().is_some_and(|e| e == "json")).expect("the product's file");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    v["part"]["custom"].as_object_mut().unwrap().remove("hand_order").expect("the order was saved with the product");
    std::fs::write(&file, serde_json::to_string(&v).unwrap()).unwrap();
    call(&mut s, "library", None, json!({"mode": "add", "name": "DEMO-001", "qty": 1}));
    assert_eq!(part_cut_order(&mut s, 2), (vec![1, 2, 3, 4], vec![]));
    let _ = std::fs::remove_dir_all(&d);
}

/// Every file under `dir`.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() { out.extend(walk(&p)) } else { out.push(p) }
    }
    out
}

#[test]
fn odd_cut_order_commands_are_refused_or_change_nothing() {
    let d = dir("part_order_odd");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1}));
    let order = |args: Value| {
        let mut a = args;
        a["part"] = json!(1);
        a
    };
    // Numbers the part doesn't have, none at all, or no mode it knows.
    for c in ["7", "0", "2, 9", "1-7"] {
        let e = err(&mut s, "part-cut-order", order(json!({"mode": "add", "contours": c})));
        assert!(e.contains("6 contours") && e.contains("part-info"), "{c}: {e}");
    }
    for c in ["", " , ", "two"] {
        let e = err(&mut s, "part-cut-order", order(json!({"mode": "add", "contours": c})));
        assert!(e.contains("contour"), "{c:?}: {e}");
    }
    assert!(err(&mut s, "part-cut-order", order(json!({"mode": "add"}))).contains("contours"));
    assert!(err(&mut s, "part-cut-order", order(json!({"mode": "all-quality"}))).contains("quality"));
    let e = err(&mut s, "part-cut-order", order(json!({"mode": "reverse"})));
    assert!(e.contains("add, all-marks, all-cut, all-quality, undo or clear"), "{e}");
    // The open lines (a bend line and a no-cut line) have no place.
    let e = err(&mut s, "part-cut-order", order(json!({"mode": "add", "contours": "5"})));
    assert!(e.contains("Contour 5 is set to Bend line"), "{e}");
    assert_eq!(part_cut_order(&mut s, 1), (vec![1, 2, 3, 4], vec![]), "nothing changed");
    // A range runs the way it is written.
    call(&mut s, "part-cut-order", None, order(json!({"mode": "add", "contours": "3-1"})));
    assert_eq!(part_cut_order(&mut s, 1), (vec![3, 2, 1, 4], vec![3, 2, 1]));

    // A part with only an outline: the outline is its whole order.
    let sq = d.join("DEMO-SQUARE.dxf");
    std::fs::write(&sq, format!("0\nSECTION\n2\nENTITIES\n{DEMO_SQUARE}0\nENDSEC\n0\nEOF\n")).unwrap();
    call(&mut s, "import-drawing", None, json!({"path": sq.to_str().unwrap(), "qty": 1}));
    assert_eq!(part_cut_order(&mut s, 2), (vec![1], vec![]));
    assert!(err(&mut s, "part-cut-order", json!({"part": 2, "mode": "all-marks"})).contains("no marks"));
    call(&mut s, "part-cut-order", None, json!({"part": 2, "mode": "all-cut"}));
    assert_eq!(part_cut_order(&mut s, 2), (vec![1], vec![1]));
    call(&mut s, "part-cut-order", None, json!({"part": 2, "mode": "undo"}));
    assert_eq!(part_cut_order(&mut s, 2), (vec![1], vec![]));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_part_saved_before_contour_states_takes_an_order_and_keeps_its_program() {
    let _drawn = holes_as_drawn();
    let d = dir("part_order_old");
    let job = d.join("plate.snjob");
    let jp = job.to_str().unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(jp), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_five_holes(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    // Saved before contour states: no contour list, only its outline, holes and marks.
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&job).unwrap()).unwrap();
    v["parts"][0]["custom"].as_object_mut().unwrap().remove("contours");
    std::fs::write(&job, serde_json::to_string(&v).unwrap()).unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "info", Some(jp), json!({}));
    let program = |s: &mut Session| call(s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    let code = program(&mut s);
    // Its holes, the outline, its marking line: the mark first, as today.
    assert_eq!(part_cut_order(&mut s, 1), (vec![7, 1, 2, 3, 4, 5, 6], vec![]));
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "5, 3"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![5, 3, 7, 1, 2, 4, 6], vec![5, 3]));
    // Ordering doesn't remake its outline, holes or marks: cleared again, the
    // program is the one it had.
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "clear"}));
    assert_eq!(program(&mut s), code);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&job).unwrap()).unwrap();
    assert!(v["parts"][0]["custom"].get("contours").is_none(), "its contours weren't written");
    let _ = std::fs::remove_dir_all(&d);
}

/// LB5, LB6: an agent's Library saves record its user marked as an agent,
/// and `library list` says who saved each record and when, the window's
/// saves and records from before included.
#[test]
fn an_agents_library_saves_say_agent_and_the_list_says_who_and_when() {
    let d = dir("saved_by_agent");
    let clock = Clock::at(1_794_216_000); // 2026-11-09 09:20 UTC
    let mut ctx = stock_ctx(&d);
    ctx.who = Who::new("sam", "PROG-2").as_agent();
    ctx.clock = clock.clone();
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    let jp = d.join("DEMO-001 frames.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 300, "qty": 2, "name": "DEMO-P1"}));
    call(&mut s, "library", None, json!({"mode": "save-job"}));
    clock.advance(std::time::Duration::from_secs(5 * 60));
    call(&mut s, "library", None, json!({"mode": "save-part", "folder": "Acme"}));
    // Kim saved a job in the window on PROG-1.
    clock.advance(std::time::Duration::from_secs(60));
    let spec = jobfile::load(&jp).unwrap().spec;
    crate::library::Library::open(&lib, &Who::new("kim", "PROG-1"), &clock).unwrap().save_job(&JobFile::new_job("DEMO-002 brackets", spec), crate::library::SaveTo::New).unwrap();
    let l = call(&mut s, "library", None, json!({"mode": "list"}));
    let row = |name: &str| l["data"].as_array().unwrap().iter().find(|r| r["name"] == name).cloned().unwrap_or_else(|| panic!("{name} isn't listed: {l}"));
    let (job, part, window) = (row("DEMO-001 frames"), row("DEMO-P1"), row("DEMO-002 brackets"));
    assert_eq!((job["saved_by"].as_str(), job["saved_at"].as_str()), (Some("sam on PROG-2 (agent)"), Some("2026-11-09 09:20")), "{job}");
    assert_eq!((part["saved_by"].as_str(), part["saved_at"].as_str()), (Some("sam on PROG-2 (agent)"), Some("2026-11-09 09:25")), "{part}");
    assert_eq!((window["saved_by"].as_str(), window["saved_at"].as_str()), (Some("kim on PROG-1"), Some("2026-11-09 09:26")), "{window}");
    // The record itself says so too.
    let rec = crate::library::Library::open(&lib, &Who::new("kim", "PROG-1"), &clock).unwrap().load_job(job["id"].as_u64().unwrap()).unwrap();
    assert_eq!(rec.saved.map(|s| s.by.label()).as_deref(), Some("sam on PROG-2 (agent)"));
    let _ = std::fs::remove_dir_all(&d);
}

/// LB7, LB10: an agent opens library job DEMO-7 into its job file and saves
/// it twice: one record. Once the window saved it since the agent read it,
/// the agent's save is refused with who, when and what to do instead; save
/// it as a copy, or open it again to take theirs.
#[test]
fn an_agent_saves_its_library_job_over_its_record_unless_someone_saved_it_since() {
    use crate::library::{Library, SaveTo};
    let d = dir("agent_changed_since");
    let clock = Clock::at(1_794_216_000); // 2026-11-09 09:20 UTC
    let mut ctx = stock_ctx(&d);
    ctx.who = Who::new("sam", "PROG-2").as_agent();
    ctx.clock = clock.clone();
    let window = Library::open(&ctx.library_dir, &Who::new("kim", "PROG-1"), &clock).unwrap();
    let mut s = Session::new(ctx);
    let jp = d.join("DEMO-7.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"w": 2000, "h": 1000, "thickness": 3}));
    let spec = jobfile::load(&jp).unwrap().spec;
    let id = window.save_job(&JobFile::new_job("DEMO-7", spec), SaveTo::New).unwrap().id;
    // The agent opens it, adds a part and saves, twice: one record.
    let r = call(&mut s, "library", None, json!({"mode": "open", "name": "DEMO-7"}));
    assert!(r["message"].as_str().unwrap().contains("DEMO-7"), "{r}");
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 300, "qty": 2, "name": "DEMO-P1"}));
    let r = call(&mut s, "library", None, json!({"mode": "save-job"}));
    assert_eq!(r["data"]["id"].as_u64(), Some(id), "{r}");
    clock.advance(std::time::Duration::from_secs(60));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 200, "h": 100, "qty": 1, "name": "DEMO-P2"}));
    let r = call(&mut s, "library", None, json!({"mode": "save-job"}));
    assert_eq!(r["data"]["id"].as_u64(), Some(id), "{r}");
    let jobs = |s: &mut Session| {
        let l = call(s, "library", None, json!({"mode": "list"}));
        let mut v: Vec<String> = l["data"].as_array().unwrap().iter().filter(|r| r["kind"] == "job").map(|r| format!("{} ({})", r["name"].as_str().unwrap(), r["saved_by"].as_str().unwrap_or(""))).collect();
        v.sort();
        v
    };
    assert_eq!(jobs(&mut s), ["DEMO-7 (sam on PROG-2 (agent))"]);
    // Kim saves it in the window; the agent changes its copy and saves.
    clock.advance(std::time::Duration::from_secs(10 * 60));
    let mut theirs = window.load_job(id).unwrap();
    theirs.week = "W12".into();
    window.save_job(&theirs, SaveTo::Read(theirs.library.unwrap())).unwrap();
    let record = std::fs::read(window.path(crate::library::Kind::Job, id)).unwrap();
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 100, "h": 100, "qty": 1, "name": "DEMO-P3"}));
    let e = err(&mut s, "library", json!({"mode": "save-job"}));
    assert!(e.contains("kim on PROG-1 at 2026-11-09 09:31") && e.contains("Nothing was saved"), "{e}");
    assert!(e.contains(&format!("mode=open id={id}")) && e.contains("copy=true"), "{e}");
    assert_eq!(std::fs::read(window.path(crate::library::Kind::Job, id)).unwrap(), record, "a refused save wrote the record");
    // Saved as a copy: a new job under a free name; theirs stays.
    let r = call(&mut s, "library", None, json!({"mode": "save-job", "copy": true}));
    let copy = r["data"]["id"].as_u64().unwrap();
    assert_ne!(copy, id);
    assert_eq!(jobfile::load(&jp).unwrap().name, "DEMO-7 (1)");
    assert_eq!(jobs(&mut s), ["DEMO-7 (1) (sam on PROG-2 (agent))", "DEMO-7 (kim on PROG-1)"]);
    assert_eq!(std::fs::read(window.path(crate::library::Kind::Job, id)).unwrap(), record);
    // A job file holding stock it booked isn't replaced: that stock would be
    // left taken with no job.
    let mut booked = jobfile::load(&jp).unwrap();
    booked.booked = true;
    booked.held.insert("S1".into(), 1);
    jobfile::save(&jp, &booked).unwrap();
    let e = err(&mut s, "library", json!({"mode": "open", "id": id}));
    assert!(e.contains("1 sheet(s)") && e.contains("stock-book mode=release"), "{e}");
    booked.held.clear();
    jobfile::save(&jp, &booked).unwrap();
    // Opened again, the job file is theirs, and saves over their record.
    call(&mut s, "library", None, json!({"mode": "open", "id": id}));
    let j = jobfile::load(&jp).unwrap();
    assert_eq!((j.name.as_str(), j.week.as_str(), j.parts.len()), ("DEMO-7", "W12", 2));
    assert_eq!(call(&mut s, "library", None, json!({"mode": "save-job"}))["data"]["id"].as_u64(), Some(id));
    let _ = std::fs::remove_dir_all(&d);
}

/// LB10: an agent's job file that is a library job doesn't book stock: its
/// sheets are booked when someone saves it in the window. Booked here too, a
/// refused save would leave them taken twice. status still answers.
#[test]
fn an_agent_cant_book_stock_on_a_job_file_opened_from_the_library() {
    let d = dir("agent_library_no_booking");
    let mut ctx = stock_ctx(&d);
    ctx.who = Who::new("sam", "PROG-2").as_agent();
    let stock = crate::stock::Stock::file(&ctx.library_dir);
    let mut s = Session::new(ctx);
    let first = d.join("DEMO-7.snjob");
    call(&mut s, "new", Some(first.to_str().unwrap()), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "S1", "w": 2000, "h": 1000, "thickness": 3, "qty": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 300, "qty": 12, "name": "DEMO-P1"}));
    call(&mut s, "nest", None, json!({}));
    let id = call(&mut s, "library", None, json!({"mode": "save-job"}))["data"]["id"].as_u64().unwrap();
    // Another job file opens DEMO-7 from the Library.
    let opened = d.join("DEMO-7-agent.snjob");
    call(&mut s, "new", Some(opened.to_str().unwrap()), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "library", None, json!({"mode": "open", "id": id}));
    let before = std::fs::read(&stock).unwrap();
    let e = err(&mut s, "stock-book", json!({"mode": "book"}));
    assert!(e.contains("DEMO-7") && e.contains("booked when someone saves it in the window"), "{e}");
    assert_eq!(std::fs::read(&stock).unwrap(), before, "a refused booking changed the stock");
    let e = err(&mut s, "stock-book", json!({"mode": "release"}));
    assert!(e.contains("booked when someone saves it in the window"), "{e}");
    assert_eq!(std::fs::read(&stock).unwrap(), before);
    assert!(!jobfile::load(&opened).unwrap().booked);
    // status still says what booking would take.
    let st = call(&mut s, "stock-book", None, json!({"mode": "status"}));
    assert_eq!(st["data"]["booked"], false, "{st}");
    assert!(st["data"]["would_take"]["S1"].as_u64().unwrap() > 0, "{st}");
    // The job file that saved it into the Library is that library job too.
    let e = match s.call("stock-book", Some(first.to_str().unwrap()), &json!({"mode": "book"})) {
        Ok(r) => panic!("booking the saved job file should be refused, gave {}", r.json),
        Err(e) => e,
    };
    assert!(e.contains("booked when someone saves it in the window"), "{e}");
    assert_eq!(std::fs::read(&stock).unwrap(), before);
    let _ = std::fs::remove_dir_all(&d);
}

/// LB10: an agent books its job file (1 sheet), then saves it into the
/// Library: the record keeps that booking and the job file stops following
/// the stock. Someone saves the job in the window, the agent's change nests
/// it on 3 sheets and its save is refused: the stock still holds out only
/// the 1 sheet the record holds, and the agent can take theirs.
#[test]
fn a_job_file_booked_before_its_save_into_the_library_leaves_the_stock_to_the_record() {
    use crate::library::{Library, SaveTo};
    let d = dir("agent_booked_then_saved");
    let clock = Clock::at(1_794_216_000); // 2026-11-09 09:20 UTC
    let mut ctx = stock_ctx(&d);
    ctx.who = Who::new("sam", "PROG-2").as_agent();
    ctx.clock = clock.clone();
    let lib = ctx.library_dir.clone();
    let window = Library::open(&lib, &Who::new("kim", "PROG-1"), &clock).unwrap();
    let mut s = Session::new(ctx);
    let jp = d.join("DEMO-7.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "stock", None, json!({"mode": "create", "ref": "S1", "w": 2000, "h": 1000, "thickness": 3, "qty": 5}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 300, "qty": 4, "name": "DEMO-P1"}));
    call(&mut s, "nest", None, json!({}));
    call(&mut s, "stock-book", None, json!({"mode": "book"}));
    let in_stock = || crate::stock::Stock::load(&lib).unwrap().get("S1").unwrap().qty;
    assert_eq!(in_stock(), 4, "booking takes the 1 sheet");
    // Saved into the Library: the record keeps the booking, the job file
    // no longer follows the stock.
    let r = call(&mut s, "library", None, json!({"mode": "save-job"}));
    let id = r["data"]["id"].as_u64().unwrap();
    assert!(r["message"].as_str().unwrap().contains("booked when someone saves it in the window"), "{r}");
    assert_eq!(window.load_job(id).unwrap().held.get("S1"), Some(&1));
    assert!(!jobfile::load(&jp).unwrap().booked);
    // Kim saves DEMO-7 in the window; the agent's change nests it on 3
    // sheets, and its save is refused.
    clock.advance(std::time::Duration::from_secs(10 * 60));
    let mut theirs = window.load_job(id).unwrap();
    theirs.week = "W12".into();
    window.save_job(&theirs, SaveTo::Read(theirs.library.unwrap())).unwrap();
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 1900, "h": 900, "qty": 2, "name": "DEMO-P2"}));
    call(&mut s, "nest", None, json!({}));
    assert_eq!(jobfile::load(&jp).unwrap().sheets.len(), 3);
    let e = err(&mut s, "library", json!({"mode": "save-job"}));
    assert!(e.contains("kim on PROG-1") && e.contains("Nothing was saved"), "{e}");
    // The stock matches the record: 1 sheet out, not 3.
    let held = window.load_job(id).unwrap().held;
    assert_eq!(held.get("S1"), Some(&1));
    assert_eq!(in_stock(), 4, "the stock holds out more sheets than the record");
    // The agent takes theirs; the stock still matches.
    call(&mut s, "library", None, json!({"mode": "open", "id": id}));
    assert_eq!(jobfile::load(&jp).unwrap().week, "W12");
    assert_eq!(in_stock(), 4);
    let _ = std::fs::remove_dir_all(&d);
}

/// LB7, LB10: an agent's save-part goes through the same check: a library
/// part the window saved since the agent added it is refused with who and
/// when, and nothing is saved.
#[test]
fn an_agents_save_part_is_refused_when_the_library_part_was_saved_since() {
    use crate::library::{Library, SaveTo};
    let d = dir("agent_part_changed_since");
    let clock = Clock::at(1_794_216_000); // 2026-11-09 09:20 UTC
    let mut ctx = stock_ctx(&d);
    ctx.who = Who::new("sam", "PROG-2").as_agent();
    ctx.clock = clock.clone();
    let window = Library::open(&ctx.library_dir, &Who::new("kim", "PROG-1"), &clock).unwrap();
    let mut s = Session::new(ctx);
    let jp = d.join("DEMO-8.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 300, "qty": 2, "name": "DEMO-P1"}));
    // Saved by the agent, then saved again: over its own record.
    let first = call(&mut s, "library", None, json!({"mode": "save-part", "folder": "Acme"}))["data"]["ids"][0].as_u64().unwrap();
    let again = call(&mut s, "library", None, json!({"mode": "save-part", "folder": "Acme"}))["data"]["ids"][0].as_u64().unwrap();
    assert_eq!(first, again);
    // Kim saves DEMO-P1 in the window.
    clock.advance(std::time::Duration::from_secs(10 * 60));
    let pf = window.load_product(first).unwrap();
    let mut p = pf.part.clone();
    p.w = 450.0;
    window.save_product(&p, "Acme", pf.material, pf.thickness, SaveTo::Read(crate::library::Record { id: first, revision: pf.revision })).unwrap();
    let e = err(&mut s, "library", json!({"mode": "save-part", "folder": "Acme"}));
    assert!(e.contains("DEMO-P1") && e.contains("kim on PROG-1 at 2026-11-09 09:30") && e.contains("Nothing was saved"), "{e}");
    assert_eq!(window.load_product(first).unwrap().part.w, 450.0);
    // Theirs added next to the job's own part: two parts of one reference
    // are refused before anything is saved.
    call(&mut s, "library", None, json!({"mode": "add", "id": first}));
    let two = err(&mut s, "library", json!({"mode": "save-part", "folder": "Acme"}));
    assert!(two.contains("Parts 1 and 2 are both 'DEMO-P1'") && two.contains("Nothing was saved"), "{two}");
    assert_eq!(window.load_product(first).unwrap().part.w, 450.0);
    // As the refusal says: the job's part out, theirs in, then it saves.
    assert!(e.contains("remove-part part=1") && e.contains(&format!("library mode=add id={first}")), "{e}");
    call(&mut s, "remove-part", None, json!({"part": 2}));
    call(&mut s, "remove-part", None, json!({"part": 1}));
    call(&mut s, "library", None, json!({"mode": "add", "id": first}));
    call(&mut s, "library", None, json!({"mode": "save-part", "folder": "Acme"}));
    assert_eq!(window.load_product(first).unwrap().part.w, 450.0);
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// The Library's job folders through the agents' `job-folders` (JX10–JX15).
// ---------------------------------------------------------------------------

/// LB11 for agents' folders: `job-folders` on a Library that can't be
/// reached is refused naming its folder and saying what to do, as
/// `library save-job` is, and nothing is made.
#[test]
fn agents_job_folders_are_refused_naming_the_folder_while_the_library_cant_be_reached() {
    let d = dir("job_folders_unreachable");
    let mut ctx = Ctx::builtin();
    ctx.library_dir = d.join("Nesting");
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    let said = format!("The Library on {} can't be reached", lib.display());
    for args in [
        json!({"mode": "list"}),
        json!({"mode": "new", "name": "Acme"}),
        json!({"mode": "rename", "folder": "Acme", "name": "Acme Ltd"}),
        json!({"mode": "delete", "folder": "Acme"}),
        json!({"mode": "move", "saved_job": "JOB000001", "to": "Acme"}),
    ] {
        let e = err(&mut s, "job-folders", args.clone());
        assert!(e.contains(&said), "{args}: {e}");
        assert!(e.contains("try again once it can be reached"), "{args} doesn't say what to do: {e}");
        assert!(!lib.exists(), "{args} made the Library folder");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_agents_copy_of_a_library_job_goes_into_its_folder() {
    // LB9, LB10, JX15: save-job copy=true files the copy next to the job it
    // was copied from, not by the job's folder name.
    let d = dir("agent_copy_folder");
    let mut s = Session::new(stock_ctx(&d));
    let jp = d.join("frames.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"name": "DEMO-001", "week": "W12"}));
    call(&mut s, "library", None, json!({"mode": "save-job"}));
    call(&mut s, "job-folders", None, json!({"mode": "new", "name": "Acme"}));
    call(&mut s, "job-folders", None, json!({"mode": "new", "folder": "Acme", "name": "2026-11"}));
    call(&mut s, "job-folders", None, json!({"mode": "move", "saved_job": "DEMO-001", "to": "Acme/2026-11"}));
    call(&mut s, "library", None, json!({"mode": "save-job", "copy": true}));
    let l = call(&mut s, "job-folders", None, json!({"mode": "list"}));
    assert_eq!(l["data"]["outline"], json!(["Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-001", "Acme/2026-11/DEMO-001 (1)", "W12/"]), "{l}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_agents_new_library_part_goes_where_its_library_job_is_now() {
    // JX19: a library job moved to another folder since its file was made:
    // its new parts go into the part folder named like where it is now, not
    // by the folder name the file was made with.
    let d = dir("agent_part_follows_job");
    let mut s = Session::new(stock_ctx(&d));
    let jp = d.join("frames.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"name": "DEMO-001", "week": "W12"}));
    call(&mut s, "library", None, json!({"mode": "save-job"}));
    call(&mut s, "job-folders", None, json!({"mode": "new", "name": "Acme"}));
    call(&mut s, "job-folders", None, json!({"mode": "new", "folder": "Acme", "name": "2026-11"}));
    call(&mut s, "job-folders", None, json!({"mode": "move", "saved_job": "DEMO-001", "to": "Acme/2026-11"}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 120, "qty": 4, "name": "DEMO-P1"}));
    let saved = call(&mut s, "library", None, json!({"mode": "save-part", "parts": [1]}));
    assert_eq!(saved["data"]["folders"], json!(["Acme/2026-11"]), "{saved}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_agents_copy_of_a_library_job_someone_deleted_goes_by_its_folder_name() {
    // LB9, JX15: the job it was copied from is gone from the Library, so its
    // folder isn't known: the copy goes into the folder its name says.
    let d = dir("agent_copy_deleted");
    let ctx = stock_ctx(&d);
    let lib = crate::library::Library::open(&ctx.library_dir, &Who::new("kim", "PROG-1"), &ctx.clock).unwrap();
    let mut s = Session::new(ctx);
    let jp = d.join("frames.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"name": "DEMO-001", "week": "W12"}));
    let id = call(&mut s, "library", None, json!({"mode": "save-job"}))["data"]["id"].as_u64().unwrap();
    call(&mut s, "job-folders", None, json!({"mode": "new", "name": "Acme"}));
    call(&mut s, "job-folders", None, json!({"mode": "move", "saved_job": "DEMO-001", "to": "Acme"}));
    lib.delete(crate::library::Kind::Job, id).unwrap();
    call(&mut s, "library", None, json!({"mode": "save-job", "copy": true}));
    let l = call(&mut s, "job-folders", None, json!({"mode": "list"}));
    let outline: Vec<String> = serde_json::from_value(l["data"]["outline"].clone()).unwrap();
    assert!(outline.iter().any(|r| r.starts_with("W12/DEMO-001")), "{l}");
    assert!(!outline.iter().any(|r| r.starts_with("Acme/DEMO")) && !outline.iter().any(|r| r.starts_with("DEMO")), "{l}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn agents_make_rename_move_and_delete_job_folders_and_are_told_what_to_do_instead() {
    let d = dir("job_folders");
    let ctx = stock_ctx(&d);
    let mut s = Session::new(ctx);
    let jp = d.join("frames.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"name": "DEMO-001", "week": "W12"}));
    // Saved into the Library, a job goes into the folder named like its week.
    call(&mut s, "library", None, json!({"mode": "save-job"}));
    call(&mut s, "job-folders", None, json!({"mode": "new", "name": "Acme"}));
    let nov = call(&mut s, "job-folders", None, json!({"mode": "new", "folder": "acme", "name": "2026-11"}));
    assert!(nov["message"].as_str().unwrap().contains("Acme/2026-11"), "{nov}");
    // A name taken in the same folder, without case: the original's error 9.
    let e = err(&mut s, "job-folders", json!({"mode": "new", "name": "ACME"}));
    assert!(e.contains("already in the top of the Library") && e.contains("error 9"), "{e}");
    // A saved job, by name, into a folder by its path.
    let moved = call(&mut s, "job-folders", None, json!({"mode": "move", "saved_job": "demo-001", "to": "Acme/2026-11"}));
    assert_eq!(moved["data"]["outline"], json!(["Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-001", "W12/"]), "{moved}");
    // Refusals say what to do instead.
    let e = err(&mut s, "job-folders", json!({"mode": "delete", "folder": "Acme"}));
    assert!(e.contains("still holds 1 folder") && e.contains("Move or delete what's in it first"), "{e}");
    let e = err(&mut s, "job-folders", json!({"mode": "move", "folder": "Acme", "to": "Acme/2026-11"}));
    assert!(e.contains("can't go into itself or one of its own folders"), "{e}");
    let e = err(&mut s, "job-folders", json!({"mode": "move", "folder": "Beta", "to": "/"}));
    assert!(e.contains("no folder 'Beta'") && e.contains("job-folders mode=list"), "{e}");
    let e = err(&mut s, "job-folders", json!({"mode": "move", "saved_job": "DEMO-404", "to": "/"}));
    assert!(e.contains("JOB000012"), "{e}");
    // Renaming a folder keeps what's in it; an empty one can be deleted.
    call(&mut s, "job-folders", None, json!({"mode": "rename", "folder": "Acme", "name": "Acme Ltd"}));
    call(&mut s, "job-folders", None, json!({"mode": "delete", "folder": "W12"}));
    let l = call(&mut s, "job-folders", None, json!({"mode": "list"}));
    assert_eq!(l["data"]["outline"], json!(["Acme Ltd/", "Acme Ltd/2026-11/", "Acme Ltd/2026-11/DEMO-001"]), "{l}");
    assert_eq!(l["data"]["jobs"][0]["folder"], "Acme Ltd/2026-11", "{l}");
    // By number too, and the job by its reference: to the top.
    let id = l["data"]["folders"][1]["folder"].as_str().unwrap().to_string();
    call(&mut s, "job-folders", None, json!({"mode": "move", "saved_job": "JOB000001", "to": "/"}));
    call(&mut s, "job-folders", None, json!({"mode": "move", "folder": id, "to": "/"}));
    let l = call(&mut s, "job-folders", None, json!({"mode": "list"}));
    assert_eq!(l["data"]["outline"], json!(["2026-11/", "Acme Ltd/", "DEMO-001"]), "{l}");
    // The library listing names a job's folder by its place in the tree.
    call(&mut s, "job-folders", None, json!({"mode": "move", "saved_job": "DEMO-001", "to": "2026-11"}));
    let l = call(&mut s, "library", None, json!({"mode": "list", "find": "2026"}));
    assert_eq!((l["data"][0]["name"].as_str(), l["data"][0]["folder"].as_str()), (Some("DEMO-001"), Some("2026-11")), "{l}");
}

// ---------------------------------------------------------------------------
// The Library's part folders through the agents' `part-folders` (JX18–JX20).
// ---------------------------------------------------------------------------

/// JX19, JX20: an agent's new library parts go into the part folder named
/// like the job's week (or the folder it names), made if missing; `library
/// list` names each part's folder by its path; `part-folders` lists the tree
/// and moves a part, with refusals that say what to do instead.
#[test]
fn agents_list_the_part_folders_and_move_a_library_part() {
    let d = dir("part_folders");
    let mut s = Session::new(stock_ctx(&d));
    let jp = d.join("frames.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"name": "DEMO-001", "week": "Acme"}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 120, "qty": 4, "name": "DEMO-P1"}));
    call(&mut s, "add-shape", None, json!({"shape": "disc", "w": 200, "qty": 2, "name": "DEMO-P2"}));
    // Saved into the Library: filed in the part folder named like the job's week.
    let saved = call(&mut s, "library", None, json!({"mode": "save-part", "parts": [1]}));
    assert!(saved["message"].as_str().unwrap().contains("in part folder 'Acme'"), "{saved}");
    // Or in the folder it names, several levels made.
    let saved = call(&mut s, "library", None, json!({"mode": "save-part", "parts": [2], "folder": "Beta/2026-11"}));
    assert_eq!(saved["data"]["folders"], json!(["Beta/2026-11"]), "{saved}");
    let l = call(&mut s, "part-folders", None, json!({"mode": "list"}));
    assert_eq!(l["data"]["outline"], json!(["Acme/", "Acme/DEMO-P1", "Beta/", "Beta/2026-11/", "Beta/2026-11/DEMO-P2"]), "{l}");
    assert_eq!(l["data"]["parts"][0]["ref"], "SHP000001", "{l}");
    assert_eq!(l["data"]["folders"][0]["parts"], 1, "{l}");
    // The job folders are a tree of their own: none was made.
    let jobs = call(&mut s, "job-folders", None, json!({"mode": "list"}));
    assert_eq!(jobs["data"]["outline"], json!([]), "{jobs}");
    // A part by its name into a folder by its path, then by its reference to the top.
    let moved = call(&mut s, "part-folders", None, json!({"mode": "move", "library_part": "demo-p1", "to": "beta"}));
    assert_eq!(moved["data"]["outline"], json!(["Acme/", "Beta/", "Beta/2026-11/", "Beta/2026-11/DEMO-P2", "Beta/DEMO-P1"]), "{moved}");
    let again = call(&mut s, "part-folders", None, json!({"mode": "move", "library_part": "SHP000001", "to": "Beta"}));
    assert!(again["message"].as_str().unwrap().contains("already in 'Beta': nothing moved"), "{again}");
    // Saving a part over again never moves it.
    call(&mut s, "edit-part", None, json!({"part": 1, "qty": 9}));
    call(&mut s, "library", None, json!({"mode": "save-part", "parts": [1]}));
    // `library list` names a part's folder by its path, and finds by it.
    let found = call(&mut s, "library", None, json!({"mode": "list", "find": "2026-11"}));
    assert_eq!(found["data"].as_array().unwrap().iter().map(|v| (v["name"].as_str().unwrap(), v["folder"].as_str().unwrap())).collect::<Vec<_>>(), [("DEMO-P2", "Beta/2026-11")], "{found}");
    // Refusals say what to do instead.
    let e = err(&mut s, "part-folders", json!({"mode": "delete", "folder": "Beta"}));
    assert!(e.contains("'Beta' still holds 1 part and 1 folder") && e.contains("Move or delete what's in it first"), "{e}");
    let e = err(&mut s, "part-folders", json!({"mode": "move", "library_part": "DEMO-404", "to": "/"}));
    assert!(e.contains("No library part") && e.contains("part-folders mode=list") && e.contains("SHP000012"), "{e}");
    let e = err(&mut s, "part-folders", json!({"mode": "move", "folder": "Gamma", "to": "/"}));
    assert!(e.contains("no folder 'Gamma'") && e.contains("part-folders mode=list"), "{e}");
    let e = err(&mut s, "part-folders", json!({"mode": "new", "name": "ACME"}));
    assert!(e.contains("already in the top of the Library") && e.contains("error 9"), "{e}");
    call(&mut s, "part-folders", None, json!({"mode": "move", "library_part": "SHP000001", "to": "Acme"}));
    let l = call(&mut s, "part-folders", None, json!({"mode": "list"}));
    assert_eq!(l["data"]["outline"], json!(["Acme/", "Acme/DEMO-P1", "Beta/", "Beta/2026-11/", "Beta/2026-11/DEMO-P2"]), "{l}");
    let _ = std::fs::remove_dir_all(&d);
}

/// JX19, LB7: a part saved into the Library stays saved when the part folder
/// tree can't be read afterwards: the agent is told its folder can't be
/// shown, and saving again goes over its own record instead of being refused
/// as someone else's newer save.
#[test]
fn an_agents_saved_part_stays_saved_when_the_part_folders_cant_be_read() {
    let d = dir("part_folders_damaged");
    let ctx = stock_ctx(&d);
    let tree = ctx.library_dir.join(crate::folders::file_name(crate::library::Kind::Product));
    let mut s = Session::new(ctx);
    let jp = d.join("frames.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"name": "DEMO-001", "week": "Acme"}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 300, "h": 120, "qty": 4, "name": "DEMO-P1"}));
    let first = call(&mut s, "library", None, json!({"mode": "save-part", "parts": [1]}))["data"]["ids"][0].as_u64().unwrap();
    std::fs::write(&tree, "{ damaged").unwrap();
    call(&mut s, "edit-part", None, json!({"part": 1, "qty": 9}));
    let saved = call(&mut s, "library", None, json!({"mode": "save-part", "parts": [1]}));
    assert!(saved["message"].as_str().unwrap().contains("part folder can't be shown"), "{saved}");
    assert_eq!(saved["data"]["ids"], json!([first]), "{saved}");
    let again = call(&mut s, "library", None, json!({"mode": "save-part", "parts": [1]}));
    assert_eq!(again["data"]["ids"], json!([first]), "{again}");
    let _ = std::fs::remove_dir_all(&d);
}

/// LB11 for agents' part folders: `part-folders` on a Library that can't be
/// reached is refused naming its folder and saying what to do, and nothing
/// is made.
#[test]
fn agents_part_folders_are_refused_naming_the_folder_while_the_library_cant_be_reached() {
    let d = dir("part_folders_unreachable");
    let mut ctx = Ctx::builtin();
    ctx.library_dir = d.join("Nesting");
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    let said = format!("The Library on {} can't be reached", lib.display());
    for args in [
        json!({"mode": "list"}),
        json!({"mode": "new", "name": "Acme"}),
        json!({"mode": "rename", "folder": "Acme", "name": "Acme Ltd"}),
        json!({"mode": "delete", "folder": "Acme"}),
        json!({"mode": "move", "library_part": "SHP000001", "to": "Acme"}),
    ] {
        let e = err(&mut s, "part-folders", args.clone());
        assert!(e.contains(&said) && e.contains("no part folder was made"), "{args}: {e}");
        assert!(e.contains("try again once it can be reached"), "{args} doesn't say what to do: {e}");
        assert!(!lib.exists(), "{args} made the Library folder");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_agent_moves_one_contours_lead_by_hand_and_puts_it_back() {
    let d = dir("lead_edit");
    let job = d.join("plate.snjob");
    let jp = job.to_str().unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(jp), json!({"w": 1500, "h": 1000, "material": "steel", "thickness": 5}));
    call(&mut s, "add-shape", None, json!({"shape": "plate-hole", "w": 200, "h": 150, "qty": 3}));
    call(&mut s, "leads", None, json!({"mode": "auto"}));
    // part-info shows each contour's lead as cut (LE1): 1 = the hole, 2 = the outline.
    let info = |s: &mut Session| call(s, "part-info", None, json!({"part": 1}))["data"]["contours"].clone();
    let c = info(&mut s);
    let outline = &c[1]["lead_in"];
    assert!(outline["length_mm"].as_f64().unwrap() > 0.0, "{c}");
    // Its start is a convex corner: the scrap opens 270°, so it can swing round it.
    assert_eq!(outline["angles_deg"], json!([-89.0, 179.0]), "{c}");
    assert_eq!(c[1]["lead_by_hand"], false);

    // Swing the outline's lead-in round the corner and stretch it; shorten the hole's.
    let r = call(&mut s, "lead-edit", None, json!({"part": 1, "contour": 2, "in_length": 10, "in_angle": -45}));
    assert!(r["message"].as_str().unwrap().contains("10 mm at -45°"), "{r}");
    call(&mut s, "lead-edit", None, json!({"part": 1, "contour": 1, "in_length": 4}));
    let c = info(&mut s);
    assert_eq!((c[1]["lead_in"]["length_mm"].as_f64(), c[1]["lead_in"]["angle_deg"].as_f64(), &c[1]["lead_by_hand"]), (Some(10.0), Some(-45.0), &json!(true)), "{c}");
    assert_eq!(c[0]["lead_in"]["length_mm"].as_f64(), Some(4.0), "{c}");
    let saved = jobfile::load(&job).unwrap().parts[0].tech.unwrap();
    assert_eq!(saved.lead_edits.iter().flatten().count(), 2);

    // Every piece on the sheet takes it, however the nester turned it; one
    // that would reach the sheet's edge or a neighbour is only shortened (LE5).
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let j = jobfile::load(&job).unwrap();
    let plans = j.plans();
    let outlines: Vec<&cnc::Contour> = plans[0].contours.iter().filter(|c| c.contour == 1).collect();
    assert_eq!(outlines.len(), 3);
    for c in outlines {
        let e = lead_ends(c);
        assert!(c.lead_hand, "{e:?}");
        let (l, a) = e.lead_in.unwrap();
        assert!(l > 0.0 && l < 10.0 + 1e-6 && (a + 45.0).abs() < 1e-6, "{e:?}");
    }

    // Refusals say what to do instead.
    assert!(err(&mut s, "lead-edit", json!({"part": 1, "contour": 2, "in_angle": 200})).contains("can lean"));
    assert!(err(&mut s, "lead-edit", json!({"part": 1, "contour": 2, "in_length": 0.4})).contains("at least 1 mm"));
    assert!(err(&mut s, "lead-edit", json!({"part": 1, "contour": 7, "in_length": 5})).contains("isn't one the part cuts"));
    assert!(err(&mut s, "lead-edit", json!({"part": 1, "contour": 2})).contains("in_length"));

    // Back to automatic: one contour, then the rest.
    let r = call(&mut s, "lead-edit", None, json!({"part": 1, "contour": 2, "mode": "reset"}));
    assert!(r["message"].as_str().unwrap().contains("back"), "{r}");
    let c = info(&mut s);
    assert_eq!((&c[1]["lead_by_hand"], &c[0]["lead_by_hand"]), (&json!(false), &json!(true)));
    call(&mut s, "leads", None, json!({"mode": "delete"}));
    assert_eq!(jobfile::load(&job).unwrap().parts[0].tech.unwrap().lead_edits.iter().flatten().count(), 0);
}

#[test]
fn dragging_a_lead_end_in_the_part_view_moves_that_lead_only() {
    // A plate with a hole and no technology of its own: the machine's leads.
    let mut parts = vec![Part { name: "P".into(), shape: Shape::PlateHole, w: 200.0, h: 150.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
    let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
    let before = cnc::plan_part(&parts, 0, &spec);
    let hole0 = before.contours.iter().find(|c| c.contour == 0).unwrap().lead_path.clone();
    // Drag the outline's pierce (it starts at the bottom-left corner) out to the left.
    let to = [-6.0, 4.0];
    let m = drag_lead_end(&mut parts, 0, &spec, 1, LeadEnd::In, to, crate::units::LenUnit::Mm).unwrap();
    assert!(m.contains("Contour 2's lead-in moved by hand"), "{m}");
    let t = parts[0].tech.unwrap();
    // The part keeps the machine's leads everywhere else (LE4).
    assert_eq!((t.lead_type, t.corner_start, t.lead_edits.iter().flatten().count()), (nest::LeadType::Machine, true, 1));
    let after = cnc::plan_part(&parts, 0, &spec);
    let c = after.contours.iter().find(|c| c.contour == 1).unwrap();
    // The pierce lands under the pointer, within the 0.1 mm / 1° snap.
    assert!((c.lead_in[0] - to[0]).hypot(c.lead_in[1] - to[1]) < 0.15, "{:?}", c.lead_in);
    assert_eq!(after.contours.iter().find(|c| c.contour == 0).unwrap().lead_path, hole0);
    // Dragging the same point again changes nothing; dragging into the part keeps it in the scrap.
    drag_lead_end(&mut parts, 0, &spec, 1, LeadEnd::In, to, crate::units::LenUnit::Mm).unwrap();
    assert_eq!(parts[0].tech.unwrap(), t);
    let m = drag_lead_end(&mut parts, 0, &spec, 1, LeadEnd::In, [5.0, 8.0], crate::units::LenUnit::Inch).unwrap();
    // The window's message follows its unit (NST-16).
    assert!(m.contains(" in at "), "{m}");
    let (o, _) = nest::placed_shape(&parts, &Placed { part: 0, x: 0.0, y: 0.0, w: 200.0, h: 150.0, turns: 0, mirror: false });
    let c = cnc::plan_part(&parts, 0, &spec).contours.into_iter().find(|c| c.contour == 1).unwrap();
    assert!(!nest::point_in_poly(&o, c.lead_in), "{:?}", c.lead_in);
    // The lead-out's end too; then double-click (reset) puts the contour back.
    let out = *c.lead_out_path.last().unwrap();
    drag_lead_end(&mut parts, 0, &spec, 1, LeadEnd::Out, [out[0] - 2.0, out[1] - 3.0], crate::units::LenUnit::Mm).unwrap();
    assert!(parts[0].tech.unwrap().lead_edit(1).unwrap().lead_out.is_some());
    assert_eq!(reset_lead_edits(&mut parts[0], Some(1)), 1);
    assert_eq!(cnc::plan_part(&parts, 0, &spec).contours.iter().find(|c| c.contour == 1).unwrap().lead_path, before.contours.iter().find(|c| c.contour == 1).unwrap().lead_path);
}

/// Each contour part-info gives a lead: (number, moved by hand).
fn leads_by_hand(s: &mut Session, part: usize) -> Vec<(u64, bool)> {
    let pi = call(s, "part-info", None, json!({"part": part}));
    pi["data"]["contours"].as_array().unwrap().iter().filter_map(|c| c["lead_by_hand"].as_bool().map(|b| (c["contour"].as_u64().unwrap(), b))).collect()
}

#[test]
fn a_moved_lead_stays_with_its_contour_when_the_cut_contours_change() {
    let d = dir("lead_edit_states");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": contour_plate(&d).to_str().unwrap(), "qty": 1}));
    call(&mut s, "leads", None, json!({"mode": "auto"}));
    let holes: Vec<u64> = contour_rows(&mut s, 1).into_iter().filter(|r| r.1 == "hole" && r.2 == "cut").map(|r| r.0).collect();
    assert!(holes.len() >= 2, "{holes:?}");
    let (a, b) = (holes[0], holes[1]);
    call(&mut s, "lead-edit", None, json!({"part": 1, "contour": b, "in_length": 4}));
    // An earlier hole stops being cut: the moved lead stays on its own hole (LE1, CS4).
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": a.to_string(), "state": "mark"}));
    let moved: Vec<u64> = leads_by_hand(&mut s, 1).into_iter().filter(|r| r.1).map(|r| r.0).collect();
    assert_eq!(moved, vec![b]);
    // Its own hole stops being cut: the moved lead goes with it, to no other contour.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": b.to_string(), "state": "mark"}));
    assert!(leads_by_hand(&mut s, 1).iter().all(|r| !r.1));
    assert_eq!(jobfile::load(&job).unwrap().parts[0].tech.unwrap().lead_edits.iter().flatten().count(), 0);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_moved_lead_reads_the_same_with_system_compensation() {
    let d = dir("lead_edit_kerf");
    let job = d.join("plate.snjob");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 200, "h": 100, "qty": 1}));
    call(&mut s, "leads", None, json!({"mode": "auto"}));
    call(&mut s, "compensation", None, json!({"control": "off", "system": "on"}));
    // The lead is measured on the drawn contour, not the offset path (KC8, LE1).
    let lead = |s: &mut Session| {
        let c = call(s, "part-info", None, json!({"part": 1}))["data"]["contours"][0]["lead_in"].clone();
        (c["length_mm"].as_f64().unwrap(), c["angle_deg"].as_f64().unwrap())
    };
    call(&mut s, "lead-edit", None, json!({"part": 1, "contour": 1, "in_length": 10, "in_angle": 90}));
    assert_eq!(lead(&mut s), (10.0, 90.0));
    // Only the length given: the angle stays.
    call(&mut s, "lead-edit", None, json!({"part": 1, "contour": 1, "in_length": 12}));
    assert_eq!(lead(&mut s), (12.0, 90.0));
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// Sheets and machine programs follow a part's cut order (MAC-5, CO8–CO10).
// ---------------------------------------------------------------------------

/// The program written for `name` before a part's hand-set cut order reached
/// the sheet (spec #70): a job with no hand-set order must write it
/// unchanged. `NESTPRO_RECORD=1` writes it anew.
fn same_as_recorded(name: &str, text: &str) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/engine/testdata").join(name);
    if std::env::var_os("NESTPRO_RECORD").is_some() {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{name}: {e} (NESTPRO_RECORD=1 records it)"));
    if want != text {
        let (a, b): (Vec<&str>, Vec<&str>) = (want.split('\n').collect(), text.split('\n').collect());
        let k = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
        panic!("{name}: line {} was {:?}, now {:?} ({} lines before, {} now)", k + 1, a.get(k), b.get(k), a.len(), b.len());
    }
}

/// A made-up 300 × 200 frame: a 200 × 120 window (contour 1), a Ø16 hole
/// (contour 2), the outline (3) and a line on layer MARK (4).
fn frame_with_window(d: &Path) -> PathBuf {
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| format!("0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n{x0}\n20\n{y0}\n10\n{x1}\n20\n{y0}\n10\n{x1}\n20\n{y1}\n10\n{x0}\n20\n{y1}\n");
    let hole = "0\nCIRCLE\n8\n0\n10\n270\n20\n100\n30\n0\n40\n8\n";
    let mark = "0\nLINE\n8\nMARK\n10\n20\n20\n185\n11\n120\n21\n185\n";
    let body = [rect(0.0, 0.0, 300.0, 200.0), rect(40.0, 40.0, 240.0, 160.0), hole.to_string(), mark.to_string()].concat();
    let p = d.join("DEMO-FRAME.dxf");
    std::fs::write(&p, format!("0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n")).unwrap();
    p
}

/// Every sheet's program of the job open in `s`, on the post `ctx` sets.
fn programs(s: &mut Session) -> Vec<String> {
    let n = call(s, "info", None, json!({}))["data"]["sheets"].as_array().map_or(0, Vec::len);
    (1..=n).map(|k| call(s, "program", None, json!({"sheet": k}))["data"].as_str().unwrap().to_string()).collect()
}

/// Made-up jobs with no hand-set cut order, each written on the G-code post
/// and the LC5: a plate with five holes and a marking line turned and
/// mirrored, with a text and a built-in flange; a frame with a part sitting
/// in its window, and a sheet where the window is picked by hand; a part
/// saved before contour states whose marks are listed open line first.
fn no_order_jobs(d: &Path, blm: bool) -> Vec<(String, Vec<String>)> {
    // Order mode of the holes None: the part's own order, as before the
    // setting (CO14).
    let drawn = holes_as_drawn();
    let mut out = vec![];
    let ctx = || {
        let mut c = Ctx::builtin();
        c.blm.on = blm;
        c
    };
    // Five holes, turned and mirrored, with a mark, a text and a flange.
    let job = d.join("demo.snjob");
    let mut s = Session::new(ctx());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 3, "force": true}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_five_holes(d).to_str().unwrap(), "qty": 4, "layers": "MARK=mark"}));
    call(&mut s, "add-shape", None, json!({"shape": "flange", "w": 200, "h": 200, "qty": 1, "name": "DEMO-FLANGE"}));
    call(&mut s, "text", None, json!({"mode": "add", "parts": "1", "text": "DEMO", "kind": "mark"}));
    call(&mut s, "edit-part", None, json!({"part": 1, "mirror": true}));
    for (x, y, angle, mirror) in [(20, 20, 0, false), (400, 20, 90, false), (20, 400, 180, true), (600, 20, 270, true)] {
        call(&mut s, "place", None, json!({"part": 1, "x": x, "y": y, "angle": angle, "mirror": mirror}));
    }
    call(&mut s, "place", None, json!({"part": 2, "x": 800, "y": 400}));
    out.push(("holes".to_string(), programs(&mut s)));

    // A frame with a part in its window; on sheet 2 the window picked by hand.
    let job = d.join("rings.snjob");
    let mut s = Session::new(ctx());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 3, "force": true}));
    call(&mut s, "import-drawing", None, json!({"path": frame_with_window(d).to_str().unwrap(), "qty": 3, "layers": "MARK=mark"}));
    call(&mut s, "add-shape", None, json!({"shape": "plate-hole", "w": 150, "h": 80, "qty": 2, "name": "DEMO-INNER"}));
    call(&mut s, "edit-part", None, json!({"part": 1, "mirror": true}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    call(&mut s, "place", None, json!({"part": 2, "x": 110, "y": 110}));
    call(&mut s, "place", None, json!({"part": 1, "x": 500, "y": 50, "angle": 90, "mirror": true}));
    call(&mut s, "place", None, json!({"part": 1, "sheet": "new", "x": 50, "y": 50}));
    call(&mut s, "place", None, json!({"part": 2, "sheet": "2", "x": 110, "y": 110}));
    call(&mut s, "cut-order", None, json!({"mode": "pick", "sheet": 2, "x": 90, "y": 150}));
    out.push(("rings".to_string(), programs(&mut s)));

    // A part saved before contour states: its marks listed open line first.
    let job = d.join("old.snjob");
    let jp = job.to_str().unwrap();
    let mut s = Session::new(ctx());
    call(&mut s, "new", Some(jp), json!({"w": 1000, "h": 500, "material": "mild steel", "thickness": 3, "force": true}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_five_holes(d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&job).unwrap()).unwrap();
    let geo = v["parts"][0]["custom"].as_object_mut().unwrap();
    geo.remove("contours");
    let square = json!([[200.0, 60.0], [220.0, 60.0], [220.0, 80.0], [200.0, 80.0], [200.0, 60.0]]);
    geo["marks"].as_array_mut().unwrap().push(square);
    std::fs::write(&job, serde_json::to_string(&v).unwrap()).unwrap();
    let mut s = Session::new(ctx());
    call(&mut s, "info", Some(jp), json!({}));
    out.push(("old".to_string(), programs(&mut s)));

    // A 25 mm kerf with system compensation: both holes of a marked plate
    // can't be cut, so the outline is its first cut (KC18).
    crate::ct1::set_thread_active(Some(crate::ct1::MachineTech::from_text("B565 0\nB116 ON\nB117 OFF\nB002 12.5\n")));
    let job = d.join("kerf.snjob");
    let mut s = Session::new(ctx());
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "mild steel", "thickness": 3, "force": true}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_two_holes(d).to_str().unwrap(), "qty": 2, "layers": "MARK=mark"}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    call(&mut s, "place", None, json!({"part": 1, "x": 400, "y": 50, "angle": 180}));
    out.push(("kerf".to_string(), programs(&mut s)));
    drop(drawn);
    out
}

#[test]
fn a_job_with_no_hand_set_order_writes_the_programs_it_wrote_before() {
    for (blm, post) in [(false, "iso"), (true, "lc5")] {
        let d = dir(&format!("no_order_{post}"));
        for (job, texts) in no_order_jobs(&d, blm) {
            for (k, text) in texts.iter().enumerate() {
                same_as_recorded(&format!("no-order-{job}-{post}-sheet{}.txt", k + 1), text);
            }
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// One path a program cuts or marks, as read back from its text: a mark, or
/// a cut with the box of its points and the centres of its arcs.
struct ReadPath {
    mark: bool,
    lo: [f64; 2],
    hi: [f64; 2],
    arcs: Vec<[f64; 2]>,
}

impl ReadPath {
    fn new(mark: bool) -> ReadPath {
        ReadPath { mark, lo: [f64::MAX; 2], hi: [f64::MIN; 2], arcs: vec![] }
    }
    fn add(&mut self, p: [f64; 2]) {
        self.lo = [self.lo[0].min(p[0]), self.lo[1].min(p[1])];
        self.hi = [self.hi[0].max(p[0]), self.hi[1].max(p[1])];
    }
    /// Where it is: the centre of its arcs, else of its box.
    fn centre(&self) -> [f64; 2] {
        match self.arcs.len() {
            0 => [(self.lo[0] + self.hi[0]) / 2.0, (self.lo[1] + self.hi[1]) / 2.0],
            n => [self.arcs.iter().map(|c| c[0]).sum::<f64>() / n as f64, self.arcs.iter().map(|c| c[1]).sum::<f64>() / n as f64],
        }
    }
    fn size(&self) -> f64 {
        (self.hi[0] - self.lo[0]) * (self.hi[1] - self.lo[1])
    }
}

/// The value after `key` in a program word list ("X12.5 Y3" → 12.5 for 'X').
fn word(line: &str, key: char) -> Option<f64> {
    line.split_whitespace().find_map(|w| w.strip_prefix(key)?.parse().ok())
}

/// The paths of a G-code program, part by part in the order the parts
/// first appear: each pierce (M07) or mark (M10) to its end.
fn iso_paths(text: &str) -> Vec<(usize, Vec<ReadPath>)> {
    let mut out: Vec<(usize, Vec<ReadPath>)> = vec![];
    let mut part = 0;
    let mut cur: Option<ReadPath> = None;
    for l in text.lines() {
        if let Some(r) = l.strip_prefix("( PART ") {
            part = r.split(' ').next().unwrap().parse().unwrap();
            continue;
        }
        let body = l.split_once(' ').map_or(l, |x| x.1);
        if body.starts_with("M07 ( PIERCE") || body.starts_with("M10") {
            cur = Some(ReadPath::new(body.starts_with("M10")));
        } else if body == "M08" || body.starts_with("M11") {
            if let Some(p) = cur.take() {
                match out.iter_mut().find(|o| o.0 == part) {
                    Some(o) => o.1.push(p),
                    None => out.push((part, vec![p])),
                }
            }
        } else if let (Some(p), true) = (cur.as_mut(), body.starts_with("G1") || body.starts_with("G2") || body.starts_with("G3")) {
            if let (Some(x), Some(y)) = (word(body, 'X'), word(body, 'Y')) {
                p.add([x, y]);
            }
        }
    }
    out
}

/// The paths of each part's subprogram in an LC5 set as `program` returns
/// it, in the order the subprograms are written: each NEXT_L to its
/// LASTLINE, a mark when its pierce and quality codes are 0 and 9.
fn lc5_paths(text: &str) -> Vec<Vec<ReadPath>> {
    let mut out = vec![];
    for file in text.split("; ==== ").skip(1).filter(|f| f.contains("INITPART")) {
        let mut paths: Vec<ReadPath> = vec![];
        let mut at = [0.0, 0.0];
        let mut profile = false;
        for l in file.lines().map(str::trim) {
            if let Some(a) = l.strip_prefix("NEXT_L(").and_then(|a| a.strip_suffix(')')) {
                let a: Vec<f64> = a.split(',').map(|v| v.parse().unwrap()).collect();
                paths.push(ReadPath::new(a[3] == 0.0 && a[4] == 9.0));
                at = [a[0], a[1]];
                profile = false;
            } else if l == "PROFILE" {
                profile = true;
            } else if let Some(a) = l.strip_prefix("LASTLINE(").and_then(|a| a.strip_suffix(')')) {
                let a: Vec<f64> = a.split(',').map(|v| v.parse().unwrap()).collect();
                let p = paths.last_mut().unwrap();
                p.add([a[4], a[5]]);
                p.add([a[6], a[7]]);
            } else if let (Some(x), Some(y)) = (word(l, 'X'), word(l, 'Y')) {
                let p = paths.last_mut().unwrap();
                if profile {
                    p.add([x, y]);
                    if let (Some(i), Some(j)) = (word(l, 'I'), word(l, 'J')) {
                        p.arcs.push([at[0] + i, at[1] + j]);
                    }
                }
                at = [x, y];
            }
        }
        out.push(paths);
    }
    out
}

/// The contours of DEMO-003 (`plate_with_five_holes`) a part's paths cut, by
/// part-info's numbers: the biggest cut is the outline (6), each hole is
/// known by how far it is from the outline's centre (1–5: 110, 60, 10, 40
/// and 100 mm, however the part is turned or mirrored), a mark is 7.
fn five_holes_numbers(paths: &[ReadPath]) -> Vec<u64> {
    let outline = paths.iter().filter(|p| !p.mark).max_by(|a, b| a.size().total_cmp(&b.size())).unwrap();
    let c = outline.centre();
    paths
        .iter()
        .map(|p| {
            if p.mark {
                return 7;
            }
            if std::ptr::eq(p, outline) {
                return 6;
            }
            let q = p.centre();
            let d = ((q[0] - c[0]).powi(2) + (q[1] - c[1]).powi(2)).sqrt();
            [(1, 110.0), (2, 60.0), (3, 10.0), (4, 40.0), (5, 100.0)].into_iter().min_by(|a, b| (a.1 - d).abs().total_cmp(&(b.1 - d).abs())).unwrap().0
        })
        .collect()
}

/// A job with DEMO-003 (`plate_with_five_holes`, its line on layer MARK)
/// ordered by hand as `order`, `copies` placed at (x, y, angle, mirrored).
fn ordered_plate_job(d: &Path, blm: bool, order: &str, copies: &[(i64, i64, i64, bool)]) -> Session {
    let mut ctx = Ctx::builtin();
    ctx.blm.on = blm;
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(d.join("ordered.snjob").to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 3, "force": true}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_five_holes(d).to_str().unwrap(), "qty": copies.len(), "layers": "MARK=mark"}));
    call(&mut s, "edit-part", None, json!({"part": 1, "mirror": true}));
    if !order.is_empty() {
        call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": order}));
    }
    for &(x, y, angle, mirror) in copies {
        call(&mut s, "place", None, json!({"part": 1, "x": x, "y": y, "angle": angle, "mirror": mirror}));
    }
    s
}

/// The sheet's G-code program, and each placed part's contours in the order
/// it cuts and marks them (part-info's numbers).
fn iso_order(s: &mut Session, sheet: usize) -> Vec<(usize, Vec<u64>)> {
    let text = call(s, "program", None, json!({"sheet": sheet}))["data"].as_str().unwrap().to_string();
    iso_paths(&text).iter().map(|(p, paths)| (*p, five_holes_numbers(paths))).collect()
}

/// Verify Machining finds nothing wrong on any sheet.
fn verify_clean(s: &mut Session) {
    let v = call(s, "verify", None, json!({}));
    assert!(v["data"].as_array().unwrap().iter().all(|r| r["correct"] == true), "{v}");
}

/// Copies turned 90°, 180° and 270°, and mirrored.
const TURNED: [(i64, i64, i64, bool); 4] = [(50, 50, 90, false), (400, 50, 180, false), (50, 500, 270, true), (400, 500, 0, true)];

#[test]
fn every_copy_of_a_part_is_cut_in_its_hand_set_order_turned_or_mirrored() {
    let d = dir("sheet_order_turned");
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &TURNED);
    // CO8: holes 3, 1, 2 as set by hand, then the rest in the automatic order
    // (the mark, holes 4 and 5), the outline last: what part-info says.
    let want = vec![3, 1, 2, 7, 4, 5, 6];
    assert_eq!(part_cut_order(&mut s, 1).0, want);
    let got = iso_order(&mut s, 1);
    assert_eq!(got.len(), 4);
    for (part, order) in got {
        assert_eq!(order, want, "copy #{part}");
    }
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_lc5_set_writes_each_copy_in_its_hand_set_order() {
    let d = dir("sheet_order_lc5");
    let mut s = ordered_plate_job(&d, true, "3, 1, 2", &TURNED);
    let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    // One subprogram per copy (each turned or mirrored its own way, BL37),
    // each writing the contours in the part's cut order (CO8, CO9).
    let subs = lc5_paths(&text);
    assert_eq!(subs.len(), 4, "{text}");
    for paths in &subs {
        assert_eq!(five_holes_numbers(paths), vec![3, 1, 2, 7, 4, 5, 6], "{text}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// A point on the edge of DEMO-003's hole `k` (1–5) where `pl` puts it.
fn five_holes_edge(pl: Placed, k: usize) -> [f64; 2] {
    let (x, r) = [(40.0, 10.0), (90.0, 5.0), (140.0, 10.0), (190.0, 5.0), (250.0, 15.0)][k - 1];
    pl.map([x + r, 50.0])
}

#[test]
fn a_sheet_pick_wins_on_its_sheet_and_the_rest_follows_the_parts_order() {
    let d = dir("sheet_order_picks");
    // Sheet 1: two copies turned 90°; sheet 2: one mirrored.
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &[(50, 50, 90, false), (400, 50, 90, false)]);
    call(&mut s, "place", None, json!({"part": 1, "sheet": "new", "x": 50, "y": 50, "mirror": true}));
    let turned = |x: f64| Placed { part: 0, x, y: 50.0, w: 100.0, h: 300.0, turns: 1, mirror: false };
    let mirrored = Placed { part: 0, x: 50.0, y: 50.0, w: 300.0, h: 100.0, turns: 0, mirror: true };
    // CO10: hole 2, which the part's order names, picked on sheet 2: cut
    // first there, then the rest in the part's order (3, 1, the mark, 4, 5),
    // the outline last.
    let at = five_holes_edge(mirrored, 2);
    call(&mut s, "cut-order", None, json!({"mode": "pick", "sheet": 2, "x": at[0], "y": at[1], "tol": 1}));
    assert_eq!(iso_order(&mut s, 2), vec![(1, vec![2, 3, 1, 7, 4, 5, 6])]);
    // Sheet 1 keeps the part's order on the copy nobody picked; on the other
    // copy hole 4, which the order doesn't name, is picked and goes first.
    assert_eq!(iso_order(&mut s, 1), vec![(1, vec![3, 1, 2, 7, 4, 5, 6]), (2, vec![3, 1, 2, 7, 4, 5, 6])]);
    let at = five_holes_edge(turned(400.0), 4);
    call(&mut s, "cut-order", None, json!({"mode": "pick", "sheet": 1, "x": at[0], "y": at[1], "tol": 1}));
    let got = iso_order(&mut s, 1);
    assert_eq!(got, vec![(2, vec![4, 3, 1, 2, 7, 5, 6]), (1, vec![3, 1, 2, 7, 4, 5, 6])]);
    assert_eq!(iso_order(&mut s, 2), vec![(1, vec![2, 3, 1, 7, 4, 5, 6])], "the other sheet keeps its own pick");
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_part_in_a_hole_of_an_ordered_part_is_cut_before_that_hole() {
    let d = dir("sheet_order_rings");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(d.join("rings.snjob").to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": frame_with_window(&d).to_str().unwrap(), "qty": 3, "layers": "MARK=mark"}));
    call(&mut s, "add-shape", None, json!({"shape": "plate-hole", "w": 150, "h": 80, "qty": 3, "name": "DEMO-INNER"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![4, 1, 2, 3], vec![]), "the mark, the window, the Ø16 hole, the outline");
    // The Ø16 hole by hand, then the window: the part in the window still
    // goes before the window (CO1, CO2).
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "2, 1"}));
    for sheet in 1..=3 {
        let new = if sheet == 1 { json!({"part": 1, "x": 50, "y": 50}) } else { json!({"part": 1, "sheet": "new", "x": 50, "y": 50}) };
        call(&mut s, "place", None, new);
        call(&mut s, "place", None, json!({"part": 2, "sheet": sheet.to_string(), "x": 110, "y": 110}));
    }
    // Sheet 2: the frame's outline picked; sheet 3: its window.
    call(&mut s, "cut-order", None, json!({"mode": "pick", "sheet": 2, "x": 50, "y": 150, "tol": 1}));
    call(&mut s, "cut-order", None, json!({"mode": "pick", "sheet": 3, "x": 90, "y": 150, "tol": 1}));
    // The frame's contours by size: outline 3, window 1, hole 2; its mark 4.
    // The inner part: its hole, then its outline.
    let frame = |p: &[ReadPath]| -> Vec<u64> {
        p.iter().map(|q| if q.mark { 4 } else if q.size() > 50_000.0 { 3 } else if q.size() > 10_000.0 { 1 } else { 2 }).collect()
    };
    let inner = |p: &[ReadPath]| -> Vec<&str> { p.iter().map(|q| if q.size() > 10_000.0 { "outline" } else { "hole" }).collect() };
    let runs = |sheet: usize, s: &mut Session| -> Vec<(usize, usize)> {
        // (placed part, paths in a row) as the program cuts them.
        let text = call(s, "program", None, json!({"sheet": sheet}))["data"].as_str().unwrap().to_string();
        let mut out: Vec<(usize, usize)> = vec![];
        let mut part = 0;
        for l in text.lines() {
            if let Some(r) = l.strip_prefix("( PART ") {
                part = r.split(' ').next().unwrap().parse().unwrap();
            }
            if l.contains("M07 ( PIERCE") || l.contains("M10 ( MARK ON )") {
                match out.last_mut() {
                    Some(o) if o.0 == part => o.1 += 1,
                    _ => out.push((part, 1)),
                }
            }
        }
        out
    };
    let paths = |sheet: usize, s: &mut Session| iso_paths(call(s, "program", None, json!({"sheet": sheet}))["data"].as_str().unwrap());
    for sheet in 1..=3 {
        let p = paths(sheet, &mut s);
        let f = p.iter().find(|x| x.0 == 1).unwrap();
        let i = p.iter().find(|x| x.0 == 2).unwrap();
        assert_eq!(inner(&i.1), vec!["hole", "outline"], "sheet {sheet}");
        let (want, order) = match sheet {
            // CO1: the part in the window first, then the frame in its order.
            1 => (vec![2, 1, 4, 3], vec![(2, 2), (1, 4)]),
            // CO10, CO2: the picked outline cuts the frame whole in its order,
            // the part in the window just before the window.
            2 => (vec![2, 1, 4, 3], vec![(1, 1), (2, 2), (1, 3)]),
            // The picked window first, after the part in it; then the rest
            // of the frame in its order.
            _ => (vec![1, 2, 4, 3], vec![(2, 2), (1, 4)]),
        };
        assert_eq!(frame(&f.1), want, "sheet {sheet}");
        assert_eq!(runs(sheet, &mut s), order, "sheet {sheet}");
    }
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_mark_set_by_hand_between_two_holes_is_made_between_them() {
    for blm in [false, true] {
        let d = dir(&format!("sheet_order_mark_{blm}"));
        let mut s = ordered_plate_job(&d, blm, "3, 7, 1", &[(50, 50, 90, false), (400, 50, 0, true)]);
        // A text marked on the part: not a contour, so it is made with the
        // part's first cut, as before (CO9).
        call(&mut s, "text", None, json!({"mode": "add", "parts": "1", "text": "DEMO", "kind": "mark"}));
        let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
        let copies: Vec<Vec<u64>> = if blm { lc5_paths(&text).iter().map(|p| five_holes_numbers(p)).collect() } else { iso_paths(&text).iter().map(|p| five_holes_numbers(&p.1)).collect() };
        assert_eq!(copies.len(), 2, "{text}");
        for order in copies {
            // The text's strokes, then hole 3, the mark, hole 1 and the rest.
            let strokes = order.iter().take_while(|&&k| k == 7).count();
            assert!(strokes > 1, "{order:?}");
            assert_eq!(order[strokes..], [3, 7, 1, 2, 4, 5, 6], "lc5 {blm}: {order:?}");
        }
        // With no order set by hand, the marks first, as before.
        call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "clear"}));
        let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
        let first = if blm { five_holes_numbers(&lc5_paths(&text)[0]) } else { five_holes_numbers(&iso_paths(&text)[0].1) };
        let strokes = first.iter().take_while(|&&k| k == 7).count();
        assert_eq!(first[strokes..], [1, 2, 3, 4, 5, 6], "lc5 {blm}: {first:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn a_technology_change_after_ordering_reaches_the_program() {
    let _drawn = holes_as_drawn();
    let d = dir("sheet_order_tech");
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &[(50, 50, 270, true)]);
    // CO7 on the sheet: hole 1 set to No cut leaves the order and isn't cut.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "state": "no-cut"}));
    assert_eq!(iso_order(&mut s, 1), vec![(1, vec![3, 2, 7, 4, 5, 6])]);
    // Cut again, it joins the automatic part after the mark.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "state": "cut"}));
    assert_eq!(iso_order(&mut s, 1), vec![(1, vec![3, 2, 7, 1, 4, 5, 6])]);
    // Hole 2 marked: it joins the automatic marks, made after hole 3.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "2", "state": "mark"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![3, 2, 7, 1, 4, 5, 6], vec![3]));
    assert_eq!(iso_order(&mut s, 1), vec![(1, vec![3, 7, 7, 1, 4, 5, 6])]);
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cut_time_and_rapid_length_follow_the_order() {
    let d = dir("sheet_order_time");
    let mut s = ordered_plate_job(&d, false, "", &[(50, 50, 0, false)]);
    let sheet = |s: &mut Session| {
        let i = call(s, "info", None, json!({}));
        let v = &i["data"]["sheets"][0];
        (v["rapid_len_mm"].as_f64().unwrap(), v["time_s"].as_f64().unwrap(), v["cut_len_mm"].as_f64().unwrap())
    };
    let part = |s: &mut Session| call(s, "part-info", None, json!({"part": 1}))["data"]["machining"]["times_s"]["rapid"].as_f64().unwrap();
    let (rapid, time, cut) = sheet(&mut s);
    let own = part(&mut s);
    // Back and forth across the plate: holes 1, 5, 2, 4, 3, about 300 mm
    // more from centre to centre than left to right.
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "1, 5, 2, 4, 3"}));
    let (rapid2, time2, cut2) = sheet(&mut s);
    assert!(rapid2 > rapid + 200.0, "rapids {rapid} → {rapid2}");
    assert!(time2 > time, "time {time} → {time2}");
    assert_eq!(cut2, cut, "the same contours are cut");
    assert!(part(&mut s) > own, "the part's own rapid time follows it too");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_part_saved_before_contour_states_is_cut_and_marked_in_its_hand_set_order() {
    let d = dir("sheet_order_old_part");
    let job = d.join("old.snjob");
    let jp = job.to_str().unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(jp), json!({"w": 1000, "h": 500, "material": "mild steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": plate_with_five_holes(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    // Saved before contour states, with a closed marking square after its
    // marking line: numbered 1–5 holes, 6 the square, 7 the outline, 8 the line.
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&job).unwrap()).unwrap();
    let geo = v["parts"][0]["custom"].as_object_mut().unwrap();
    geo.remove("contours");
    geo["marks"].as_array_mut().unwrap().push(json!([[200.0, 60.0], [220.0, 60.0], [220.0, 80.0], [200.0, 80.0], [200.0, 60.0]]));
    std::fs::write(&job, serde_json::to_string(&v).unwrap()).unwrap();
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "info", Some(jp), json!({}));
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "3, 6, 1"}));
    assert_eq!(part_cut_order(&mut s, 1).0, vec![3, 6, 1, 8, 2, 4, 5, 7]);
    // On the sheet: hole 3, the square, hole 1, the line, the rest.
    let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    let paths = &iso_paths(&text)[0].1;
    assert_eq!(five_holes_numbers(paths), vec![3, 7, 1, 7, 2, 4, 5, 6], "{text}");
    let marks: Vec<bool> = paths.iter().filter(|p| p.mark).map(|p| p.size() > 100.0).collect();
    assert_eq!(marks, vec![true, false], "the square, then the line");
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// Whole parts swept or boxed into a sheet's cut order (MAC-5, MM8, MM9).
// ---------------------------------------------------------------------------

/// The placed parts in the order a G-code program first cuts them.
fn iso_parts(s: &mut Session, sheet: usize) -> Vec<usize> {
    iso_order(s, sheet).iter().map(|p| p.0).collect()
}

#[test]
fn parts_taken_whole_are_cut_first_in_that_order_each_in_its_own_order() {
    let d = dir("sheet_parts_items");
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &TURNED);
    let want = vec![3, 1, 2, 7, 4, 5, 6];
    assert_eq!(iso_parts(&mut s, 1), vec![1, 3, 2, 4], "the automatic order");
    // MM8: #4, #2 and #1, as a sweep would take them; #3 follows on its own.
    let r = call(&mut s, "cut-order", None, json!({"mode": "parts", "items": "4, 2, 1"}));
    assert_eq!(r["data"]["added"], json!([4, 2, 1]), "{r}");
    let got = iso_order(&mut s, 1);
    assert_eq!(got.iter().map(|p| p.0).collect::<Vec<_>>(), vec![4, 2, 1, 3]);
    for (part, order) in got {
        assert_eq!(order, want, "copy #{part}, turned or mirrored, in its own order");
    }
    // Their contours start where the automatic ones would: none is a
    // contour picked by hand.
    let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    assert!(!text.contains("MANUAL CUT"), "{text}");
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

/// Where `TURNED` puts each copy (#1–#4).
fn turned_copy(k: usize) -> Placed {
    let (x, y, w, h, turns, mirror) = [(50.0, 50.0, 100.0, 300.0, 1, false), (400.0, 50.0, 300.0, 100.0, 2, false), (50.0, 500.0, 100.0, 300.0, 3, true), (400.0, 500.0, 300.0, 100.0, 0, true)][k - 1];
    Placed { part: 0, x, y, w, h, turns, mirror }
}

/// (placed part, paths in a row) in the order a G-code program cuts and
/// marks them.
fn part_runs(s: &mut Session, sheet: usize) -> Vec<(usize, usize)> {
    let text = call(s, "program", None, json!({"sheet": sheet}))["data"].as_str().unwrap().to_string();
    let mut out: Vec<(usize, usize)> = vec![];
    let mut part = 0;
    for l in text.lines() {
        if let Some(r) = l.strip_prefix("( PART ") {
            part = r.split(' ').next().unwrap().parse().unwrap();
        }
        if l.contains("M07 ( PIERCE") || l.contains("M10 ( MARK ON )") {
            match out.last_mut() {
                Some(o) if o.0 == part => o.1 += 1,
                _ => out.push((part, 1)),
            }
        }
    }
    out
}

#[test]
fn a_sweep_and_a_box_take_parts_and_undo_takes_back_one_command() {
    let d = dir("sheet_parts_sweep_box");
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &TURNED);
    let want = vec![3, 1, 2, 7, 4, 5, 6];
    // A sweep from #2's far edge to the left, over #1, then up over #3.
    let r = call(&mut s, "cut-order", None, json!({"mode": "parts", "points": "700,100; 100,100; 100,650"}));
    assert_eq!(r["data"]["added"], json!([2, 1, 3]), "{r}");
    // Then a click on hole 4 of #4: one contour, cut after the sweep.
    let at = five_holes_edge(turned_copy(4), 4);
    call(&mut s, "cut-order", None, json!({"mode": "pick", "x": at[0], "y": at[1], "tol": 1}));
    assert_eq!(iso_order(&mut s, 1), vec![(2, want.clone()), (1, want.clone()), (3, want.clone()), (4, vec![4, 3, 1, 2, 7, 5, 6])]);
    let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    assert_eq!(text.matches("MANUAL CUT").count(), 1, "only the clicked hole is a contour picked by hand");
    verify_clean(&mut s);
    // MM9: undo takes back the click, then the whole sweep.
    assert_eq!(call(&mut s, "cut-order", None, json!({"mode": "undo"}))["message"], "Last manual pick removed.");
    assert_eq!(iso_parts(&mut s, 1), vec![2, 1, 3, 4]);
    let r = call(&mut s, "cut-order", None, json!({"mode": "undo"}));
    assert_eq!(r["message"], "Last sweep or box taken back: #2, #1 and #3 back to the automatic order.");
    assert_eq!(iso_parts(&mut s, 1), vec![1, 3, 2, 4]);
    assert!(err(&mut s, "cut-order", json!({"mode": "undo"})).contains("No manual picks"));
    // A box round #1 and #2, and half of #4: #1 and #2, in the automatic order.
    let r = call(&mut s, "cut-order", None, json!({"mode": "parts", "box": "720,0; 0,420"}));
    assert_eq!(r["data"]["added"], json!([1, 2]), "{r}");
    assert_eq!(iso_parts(&mut s, 1), vec![1, 2, 3, 4]);
    for (part, order) in iso_order(&mut s, 1) {
        assert_eq!(order, want, "copy #{part}");
    }
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_box_with_touching_takes_every_part_it_touches() {
    // SW1: an agent's box takes the parts wholly inside it, or with
    // touching=true every part it touches, as a box dragged to the left on
    // the sheet does; both in the automatic order (MM9).
    let d = dir("sheet_parts_box_touching");
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &TURNED);
    // Up to Y 550: round #1 and #2, and over the lower ends of #3 and #4.
    let r = call(&mut s, "cut-order", None, json!({"mode": "parts", "box": "0,0; 720,550"}));
    assert_eq!(r["data"]["added"], json!([1, 2]), "{r}");
    call(&mut s, "cut-order", None, json!({"mode": "undo"}));
    let r = call(&mut s, "cut-order", None, json!({"mode": "parts", "box": "0,0; 720,550", "touching": true}));
    assert_eq!(r["data"]["added"], json!([1, 3, 2, 4]), "{r}");
    assert_eq!(iso_parts(&mut s, 1), vec![1, 3, 2, 4]);
    verify_clean(&mut s);
    // Over open sheet it touches nothing: refused, nothing changes.
    call(&mut s, "cut-order", None, json!({"mode": "clear"}));
    let e = err(&mut s, "cut-order", json!({"mode": "parts", "box": "180,400; 350,450", "touching": true}));
    assert!(e.starts_with("No part there"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_sweep_over_parts_already_picked_skips_them_and_takes_the_rest_of_a_part_with_a_hole_picked() {
    let d = dir("sheet_parts_picked");
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &TURNED);
    // Hole 4 of #2 clicked; #3's outline clicked, which cuts it whole.
    let at = five_holes_edge(turned_copy(2), 4);
    call(&mut s, "cut-order", None, json!({"mode": "pick", "x": at[0], "y": at[1], "tol": 1}));
    call(&mut s, "cut-order", None, json!({"mode": "pick", "x": 50, "y": 650, "tol": 1}));
    // An agent names #3, #2 and #1: #3 is already cut whole by the picks
    // and skipped; the rest of #2 is taken whole here, then #1.
    let r = call(&mut s, "cut-order", None, json!({"mode": "parts", "items": "3, 2, 1"}));
    assert_eq!((r["data"]["added"].clone(), r["data"]["skipped"].clone()), (json!([2, 1]), json!([3])), "{r}");
    assert!(r["message"].as_str().unwrap().contains("#3 is already picked"), "{r}");
    assert_eq!(part_runs(&mut s, 1), vec![(2, 1), (3, 7), (2, 6), (1, 7), (4, 7)]);
    assert_eq!(iso_order(&mut s, 1)[0], (2, vec![4, 3, 1, 2, 7, 5, 6]));
    // Only parts already picked: refused, and nothing changes.
    let e = err(&mut s, "cut-order", json!({"mode": "parts", "items": "3, 1"}));
    assert!(e.contains("#3 and #1 are already picked"), "{e}");
    assert_eq!(part_runs(&mut s, 1), vec![(2, 1), (3, 7), (2, 6), (1, 7), (4, 7)]);
    // A sweep over open sheet takes nothing.
    let e = err(&mut s, "cut-order", json!({"mode": "parts", "points": "800,300; 1400,900"}));
    assert!(e.starts_with("No part there"), "{e}");
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_sweep_across_a_frame_and_the_part_in_its_window_cuts_that_part_before_the_window() {
    let d = dir("sheet_parts_frame");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(d.join("frames.snjob").to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": frame_with_window(&d).to_str().unwrap(), "qty": 2, "layers": "MARK=mark"}));
    call(&mut s, "add-shape", None, json!({"shape": "plate-hole", "w": 150, "h": 80, "qty": 1, "name": "DEMO-INNER"}));
    // The frame: the Ø16 hole by hand, then the window (the mark, the outline after).
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "2, 1"}));
    call(&mut s, "place", None, json!({"part": 1, "x": 500, "y": 50}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    call(&mut s, "place", None, json!({"part": 2, "x": 110, "y": 110}));
    // Across #2 (the frame at the left), its window and #3 sitting in it,
    // then on to #1: as passed, #2, #3, #1.
    let r = call(&mut s, "cut-order", None, json!({"mode": "parts", "points": "50,150; 200,150; 200,300; 600,300; 600,200"}));
    assert_eq!(r["data"]["added"], json!([2, 3, 1]), "{r}");
    // CO2: #2's Ø16 hole, then #3 whole before the window, then the
    // window, the mark and the outline; then #1.
    assert_eq!(part_runs(&mut s, 1), vec![(2, 1), (3, 2), (2, 3), (1, 4)]);
    verify_clean(&mut s);
    // A box round #3 alone takes it; round #2 and #3, #3 goes first (CO1).
    call(&mut s, "cut-order", None, json!({"mode": "clear"}));
    let r = call(&mut s, "cut-order", None, json!({"mode": "parts", "box": "40,40; 360,260"}));
    assert_eq!(r["data"]["added"], json!([3, 2]), "{r}");
    assert_eq!(part_runs(&mut s, 1), vec![(3, 2), (2, 4), (1, 4)]);
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

/// The order an LC5 sheet program calls its parts: each call's ORIG.
fn lc5_calls(text: &str) -> Vec<[f64; 2]> {
    let main = text.split("; ==== ").nth(1).unwrap();
    main.lines()
        .filter_map(|l| l.trim().strip_prefix("ORIG(")?.strip_suffix(')').map(|a| a.split(',').map(|v| v.parse::<f64>().unwrap()).collect::<Vec<_>>()))
        .map(|v| [v[0], v[1]])
        .collect()
}

#[test]
fn the_lc5_set_calls_parts_taken_whole_in_that_order_each_in_its_own_order() {
    let d = dir("sheet_parts_lc5");
    let mut s = ordered_plate_job(&d, true, "3, 1, 2", &TURNED);
    let corners = |s: &mut Session| -> Vec<usize> {
        let text = call(s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
        lc5_calls(&text).iter().map(|o| (1..=4).find(|&k| (turned_copy(k).x - o[0]).abs() < 1.0 && (turned_copy(k).y - o[1]).abs() < 1.0).unwrap_or(0)).collect()
    };
    assert_eq!(corners(&mut s), vec![1, 3, 2, 4], "the automatic order");
    call(&mut s, "cut-order", None, json!({"mode": "parts", "items": "4, 2, 1"}));
    // `program` reads the set back before showing it (BL39).
    let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    assert_eq!(corners(&mut s), vec![4, 2, 1, 3], "{text}");
    for paths in lc5_paths(&text) {
        assert_eq!(five_holes_numbers(&paths), vec![3, 1, 2, 7, 4, 5, 6], "{text}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn each_sheet_keeps_its_own_parts_picks_and_taking_parts_off_clears_them() {
    let d = dir("sheet_parts_sheets");
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &TURNED);
    for (x, y) in [(50, 50), (400, 50), (800, 50)] {
        let sheet = if x == 50 { "new" } else { "2" };
        call(&mut s, "place", None, json!({"part": 1, "sheet": sheet, "x": x, "y": y}));
    }
    call(&mut s, "cut-order", None, json!({"mode": "parts", "sheet": 1, "items": "4, 3"}));
    call(&mut s, "cut-order", None, json!({"mode": "parts", "sheet": 2, "points": "1100,100; 100,100"}));
    assert_eq!(iso_parts(&mut s, 1), vec![4, 3, 1, 2]);
    assert_eq!(iso_parts(&mut s, 2), vec![3, 2, 1]);
    // MM7: taking a part off sheet 1 clears its picks; sheet 2 keeps its own.
    call(&mut s, "unplace", None, json!({"sheet": 1, "item": 1}));
    assert_eq!(iso_parts(&mut s, 1).len(), 3);
    let info = call(&mut s, "info", None, json!({}));
    assert_eq!(info["data"]["sheets"][0]["manual_cuts"], 0, "{info}");
    assert_eq!(info["data"]["sheets"][1]["manual_cuts"], 3, "{info}");
    assert_eq!(iso_parts(&mut s, 2), vec![3, 2, 1]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn picks_save_with_the_job_and_a_click_is_saved_as_before() {
    let d = dir("sheet_parts_saved");
    let job = d.join("ordered.snjob");
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &TURNED);
    let at = five_holes_edge(turned_copy(1), 2);
    call(&mut s, "cut-order", None, json!({"mode": "pick", "x": at[0], "y": at[1], "tol": 1}));
    call(&mut s, "cut-order", None, json!({"mode": "parts", "items": "4, 2"}));
    let before = programs(&mut s);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&job).unwrap()).unwrap();
    let picks = v["manual_cuts"][0].as_array().unwrap();
    // A click is saved with the fields it always had; a part taken whole
    // also says by which sweep or box.
    let keys = |p: &Value| p.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&picks[0]), vec!["contour", "placed", "reverse", "start"]);
    assert_eq!((picks[1]["placed"].clone(), picks[1]["whole"].clone(), picks[2]["placed"].clone(), picks[2]["whole"].clone()), (json!(3), json!(1), json!(1), json!(1)));
    // Opened again, it writes the same programs.
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "info", Some(job.to_str().unwrap()), json!({}));
    assert_eq!(programs(&mut s), before);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_technology_change_after_a_sweep_keeps_the_parts_taken() {
    let d = dir("sheet_parts_tech");
    let mut s = ordered_plate_job(&d, false, "3, 1, 2", &TURNED);
    call(&mut s, "cut-order", None, json!({"mode": "parts", "items": "4, 2, 1"}));
    // CO7 with the sweep: hole 1 set to No cut leaves every copy's order;
    // the parts taken whole stay where the sweep put them.
    call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "1", "state": "no-cut"}));
    let got = iso_order(&mut s, 1);
    assert_eq!(got.iter().map(|p| p.0).collect::<Vec<_>>(), vec![4, 2, 1, 3]);
    for (part, order) in got {
        assert_eq!(order.len(), 6, "copy #{part}: {order:?}");
        assert_eq!(order.last(), Some(&6), "copy #{part}: the outline last");
    }
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------
// A part's holes ordered by the machine's settings (MAC-53, CO14, CO15).
// ---------------------------------------------------------------------------

/// Where DEMO-HOLES has its six Ø10 holes (contours 1–6); its 300 × 200
/// outline is contour 7. Holes 3 and 6 sit 0.5 mm off their rows.
const SCATTERED: [[f64; 2]; 6] = [[250.0, 150.0], [50.0, 50.0], [150.0, 150.5], [250.0, 50.0], [50.0, 150.0], [150.0, 49.5]];

/// A made-up drawing: a 300 × 200 plate with the holes of `SCATTERED`.
fn scattered_holes(d: &Path) -> PathBuf {
    let sq = "0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n300\n20\n0\n10\n300\n20\n200\n10\n0\n20\n200\n";
    let holes: String = SCATTERED.iter().map(|c| format!("0\nCIRCLE\n8\n0\n10\n{}\n20\n{}\n30\n0\n40\n5\n", c[0], c[1])).collect();
    let p = d.join("DEMO-HOLES.dxf");
    std::fs::write(&p, format!("0\nSECTION\n2\nENTITIES\n{sq}{holes}0\nENDSEC\n0\nEOF\n")).unwrap();
    p
}

/// Where `place` puts a copy of a 300 × 200 part at (x, y), turned `angle`
/// and mirrored or not.
fn copy_at(x: f64, y: f64, angle: i64, mirror: bool) -> Placed {
    let turns = (angle / 90).rem_euclid(4) as u8;
    let (w, h) = if turns % 2 == 1 { (200.0, 300.0) } else { (300.0, 200.0) };
    Placed { part: 0, x, y, w, h, turns, mirror }
}

/// A job with DEMO-HOLES on the machine the thread works on, written on the
/// G-code post or the LC5, with a copy placed for each of `copies` (x, y,
/// angle, mirrored, and "new" to start a new sheet or "" for the last one).
fn scattered_job(d: &Path, blm: bool, copies: &[(f64, f64, i64, bool, &str)]) -> Session {
    let mut ctx = Ctx::builtin();
    ctx.blm.on = blm;
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(d.join("holes.snjob").to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 3, "force": true}));
    call(&mut s, "import-drawing", None, json!({"path": scattered_holes(d).to_str().unwrap(), "qty": copies.len()}));
    call(&mut s, "edit-part", None, json!({"part": 1, "mirror": true}));
    for &(x, y, angle, mirror, sheet) in copies {
        let mut a = json!({"part": 1, "x": x, "y": y, "angle": angle, "mirror": mirror});
        if !sheet.is_empty() {
            a["sheet"] = json!(sheet);
        }
        call(&mut s, "place", None, a);
    }
    s
}

/// The contours of DEMO-HOLES that `paths` cut, by part-info's numbers: the
/// biggest is the outline (7), each hole is known by where `pl` puts it.
/// `local`: the paths are read from the copy's corner, as an LC5
/// subprogram writes them, else from the sheet's origin.
fn scattered_numbers(paths: &[ReadPath], pl: Placed, local: bool) -> Vec<u64> {
    let outline = paths.iter().max_by(|a, b| a.size().total_cmp(&b.size())).unwrap();
    let off = if local { [pl.x, pl.y] } else { [0.0, 0.0] };
    paths
        .iter()
        .map(|p| {
            if std::ptr::eq(p, outline) {
                return 7;
            }
            let q = p.centre();
            let d = |k: usize| {
                let c = pl.map(SCATTERED[k]);
                (c[0] - off[0] - q[0]).hypot(c[1] - off[1] - q[1])
            };
            let k = (0..6).min_by(|&a, &b| d(a).total_cmp(&d(b))).unwrap();
            assert!(d(k) < 2.0, "a path at {q:?} isn't one of the holes");
            k as u64 + 1
        })
        .collect()
}

/// Each copy's contours in the order sheet `sheet`'s program cuts them, copy
/// by copy as the program reaches them: (placed part, part-info's numbers).
/// `copies` are where the sheet's copies sit, in the order placed.
fn scattered_program(s: &mut Session, sheet: usize, copies: &[Placed]) -> Vec<(usize, Vec<u64>)> {
    let text = call(s, "program", None, json!({"sheet": sheet}))["data"].as_str().unwrap().to_string();
    if text.contains("INITPART") {
        // One subprogram per copy (BL37), here one copy per sheet.
        let subs = lc5_paths(&text);
        assert_eq!(subs.len(), copies.len(), "{text}");
        return subs.iter().zip(copies).enumerate().map(|(k, (p, pl))| (k + 1, scattered_numbers(p, *pl, true))).collect();
    }
    iso_paths(&text).iter().map(|(part, paths)| (*part, scattered_numbers(paths, copies[*part - 1], false))).collect()
}

/// A copy turned 0°, 90°, 180°, 270° and mirrored, each on its own sheet at
/// (20, 20), so the head comes to each from the sheet's origin.
const TURNS: [(i64, bool); 5] = [(0, false), (90, false), (180, false), (270, false), (0, true)];

/// The order every copy of `TURNS` is cut in, sheet by sheet.
fn turned_orders(d: &Path, blm: bool) -> (Session, Vec<Vec<u64>>) {
    let copies: Vec<(f64, f64, i64, bool, &str)> = TURNS.iter().enumerate().map(|(k, &(a, m))| (20.0, 20.0, a, m, if k == 0 { "" } else { "new" })).collect();
    let mut s = scattered_job(d, blm, &copies);
    let got = TURNS
        .iter()
        .enumerate()
        .map(|(k, &(a, m))| {
            let order = scattered_program(&mut s, k + 1, &[copy_at(20.0, 20.0, a, m)]);
            assert_eq!(order.len(), 1);
            order[0].1.clone()
        })
        .collect();
    (s, got)
}

#[test]
fn with_order_mode_none_a_parts_holes_keep_its_own_order_turned_or_not() {
    let _m = on_machine("B565 0\n");
    for blm in [false, true] {
        let d = dir(&format!("holes_none_{blm}"));
        let (mut s, got) = turned_orders(&d, blm);
        // CO14: the part's own contour order, as before the setting.
        assert_eq!(part_cut_order(&mut s, 1), (vec![1, 2, 3, 4, 5, 6, 7], vec![]));
        for (order, turn) in got.iter().zip(TURNS) {
            assert_eq!(order, &vec![1, 2, 3, 4, 5, 6, 7], "lc5 {blm}, {turn:?}");
        }
        verify_clean(&mut s);
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn by_proximity_cuts_the_nearest_hole_next_where_each_copy_sits() {
    // No machine file: the original's default, By proximity (CO20).
    for blm in [false, true] {
        let d = dir(&format!("holes_proximity_{blm}"));
        let (mut s, got) = turned_orders(&d, blm);
        // CO21 as drawn, the head at the drawing's origin (CO27): hole 2 at
        // (50, 50), then 5 straight above it (hole 6 is as near, within the
        // linear tolerance, but no nearer sideways, so the first met stays),
        // 3, 1, 4 and 6.
        assert_eq!(part_cut_order(&mut s, 1), (vec![2, 5, 3, 1, 4, 6, 7], vec![]));
        // On the sheet, where each copy sits (CO14): turned 90° hole 5 is
        // nearest the origin; from it holes 2 and 3 are 100 mm away within
        // the tolerance, and 3 is nearer sideways, so it goes first.
        let want: [Vec<u64>; 5] = [vec![2, 5, 3, 1, 4, 6, 7], vec![5, 3, 1, 4, 6, 2, 7], vec![1, 4, 6, 2, 5, 3, 7], vec![4, 6, 2, 5, 3, 1, 7], vec![4, 1, 3, 5, 2, 6, 7]];
        for ((order, want), turn) in got.iter().zip(&want).zip(TURNS) {
            assert_eq!(order, want, "lc5 {blm}, {turn:?}");
        }
        // Both posts write it, and the LC5 reads back as planned (BL39): the
        // program call above would be refused otherwise.
        verify_clean(&mut s);
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn the_next_copy_on_a_sheet_starts_from_where_the_head_is_and_ties_keep_the_hole_met_first() {
    let d = dir("holes_proximity_copies");
    // Two copies side by side: the second starts from the first's middle,
    // (170, 120), where its holes 2 and 5 are exactly as far and as far
    // sideways: hole 2, met first, stays.
    let mut s = scattered_job(&d, false, &[(20.0, 20.0, 0, false, ""), (400.0, 20.0, 0, false, "")]);
    let copies = [copy_at(20.0, 20.0, 0, false), copy_at(400.0, 20.0, 0, false)];
    assert_eq!(scattered_program(&mut s, 1, &copies), vec![(1, vec![2, 5, 3, 1, 4, 6, 7]), (2, vec![2, 5, 3, 1, 4, 6, 7])]);
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn by_direction_cuts_rows_or_columns_from_the_start_corner_every_other_one_back() {
    let d = dir("holes_direction");
    // CO23, CO24 as drawn, from each start corner, in rows and in columns.
    for (codes, want) in [
        // Corner nearest the head (the drawing's origin): lower left. Rows
        // bottom up, every other one back; hole 6 is 0.5 mm below its row
        // and still in it.
        ("B565 2\n", vec![2, 6, 4, 1, 3, 5, 7]),
        ("B565 2\nB567 2\n", vec![2, 5, 3, 6, 4, 1, 7]),
        ("B565 2\nB566 2\nB567 2\n", vec![4, 1, 3, 6, 2, 5, 7]),
        ("B565 2\nB566 3\n", vec![1, 3, 5, 2, 6, 4, 7]),
        ("B565 2\nB566 4\n", vec![5, 3, 1, 4, 6, 2, 7]),
    ] {
        let _m = on_machine(codes);
        let mut s = scattered_job(&d, false, &[(20.0, 20.0, 0, false, "")]);
        assert_eq!(part_cut_order(&mut s, 1).0, want, "{codes:?}");
    }
    // On the sheet each copy goes bottom up from its lower left corner,
    // whichever way it is turned (CO14).
    let _m = on_machine("B565 2\n");
    for blm in [false, true] {
        let (mut s, got) = turned_orders(&d, blm);
        let want: [Vec<u64>; 5] = [vec![2, 6, 4, 1, 3, 5, 7], vec![5, 2, 6, 3, 1, 4, 7], vec![1, 3, 5, 2, 6, 4, 7], vec![4, 1, 3, 6, 2, 5, 7], vec![4, 6, 2, 5, 3, 1, 7]];
        for ((order, want), turn) in got.iter().zip(&want).zip(TURNS) {
            assert_eq!(order, want, "lc5 {blm}, {turn:?}");
        }
        verify_clean(&mut s);
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_start_corner_nearest_the_head_is_chosen_per_copy_and_a_tie_takes_the_lower_left() {
    let d = dir("holes_direction_corners");
    let _m = on_machine("B565 2\n");
    // The copy high up on the left is cut first, from the origin: lower
    // left. The head then stands at its middle (170, 700), nearest the
    // other copy's upper left corner: rows from the top down.
    let placed = [(20.0, 600.0, 0, false, ""), (400.0, 20.0, 0, false, "")];
    let copies = [copy_at(20.0, 600.0, 0, false), copy_at(400.0, 20.0, 0, false)];
    let mut s = scattered_job(&d, false, &placed);
    assert_eq!(scattered_program(&mut s, 1, &copies), vec![(1, vec![2, 6, 4, 1, 3, 5, 7]), (2, vec![5, 3, 1, 4, 6, 2, 7])]);
    // Start corner 1 fixes the lower left for both.
    drop(_m);
    let _m = on_machine("B565 2\nB566 1\n");
    assert_eq!(scattered_program(&mut s, 1, &copies), vec![(1, vec![2, 6, 4, 1, 3, 5, 7]), (2, vec![2, 6, 4, 1, 3, 5, 7])]);
    // A copy above the first: from (170, 120) its lower left and lower
    // right corners are as near, and the lower left wins (lower right
    // would cut 4, 6, 2 first).
    drop(_m);
    let _m = on_machine("B565 2\n");
    let mut s = scattered_job(&d, false, &[(20.0, 20.0, 0, false, ""), (20.0, 300.0, 0, false, "")]);
    let copies = [copy_at(20.0, 20.0, 0, false), copy_at(20.0, 300.0, 0, false)];
    assert_eq!(scattered_program(&mut s, 1, &copies), vec![(1, vec![2, 6, 4, 1, 3, 5, 7]), (2, vec![2, 6, 4, 1, 3, 5, 7])]);
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_hole_on_the_orthogonality_tolerances_edge_is_in_the_row() {
    let d = dir("holes_direction_tolerance");
    // Holes 3 and 6 are 0.5 mm off their rows: at 0.5 mm they are in them,
    // at 0.49 mm each is a row of its own.
    for (tol, want) in [("1", vec![2, 6, 4, 1, 3, 5, 7]), ("0.5", vec![2, 6, 4, 1, 3, 5, 7]), ("0.49", vec![6, 4, 2, 5, 1, 3, 7])] {
        let _m = on_machine(&format!("B565 2\nB568 {tol}\n"));
        let mut s = scattered_job(&d, false, &[(20.0, 20.0, 0, false, "")]);
        assert_eq!(part_cut_order(&mut s, 1).0, want, "tolerance {tol}");
        assert_eq!(scattered_program(&mut s, 1, &[copy_at(20.0, 20.0, 0, false)]), vec![(1, want)], "tolerance {tol}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_hand_set_order_wins_for_the_holes_it_names_and_the_rest_follow_the_setting() {
    let d = dir("holes_hand_set");
    // By proximity (the default): hole 4 by hand, then the nearest from it.
    let mut s = scattered_job(&d, false, &[(20.0, 20.0, 0, false, ""), (20.0, 20.0, 90, false, "new")]);
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "4"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![4, 1, 3, 5, 2, 6, 7], vec![4]));
    assert_eq!(scattered_program(&mut s, 1, &[copy_at(20.0, 20.0, 0, false)]), vec![(1, vec![4, 1, 3, 5, 2, 6, 7])]);
    // Turned 90°: hole 4 still first, the rest by proximity where it sits.
    assert_eq!(scattered_program(&mut s, 2, &[copy_at(20.0, 20.0, 90, false)]), vec![(1, vec![4, 6, 2, 5, 3, 1, 7])]);
    // A bulk command adds the holes in the order part-info shows (CO5).
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "all-cut"}));
    assert_eq!(part_cut_order(&mut s, 1), (vec![4, 1, 3, 5, 2, 6, 7], vec![4, 1, 3, 5, 2, 6, 7]));
    // Now set by hand, every copy keeps it, turned or not.
    assert_eq!(scattered_program(&mut s, 2, &[copy_at(20.0, 20.0, 90, false)]), vec![(1, vec![4, 1, 3, 5, 2, 6, 7])]);
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_hole_picked_in_manual_cutting_goes_first_and_the_rest_go_nearest_from_it() {
    let d = dir("holes_manual_pick");
    let mut s = scattered_job(&d, false, &[(20.0, 20.0, 0, false, "")]);
    let copies = [copy_at(20.0, 20.0, 0, false)];
    // Hole 3 picked on its right edge (175, 170.5): the head is there after
    // it, nearest hole 1.
    call(&mut s, "cut-order", None, json!({"mode": "pick", "sheet": 1, "x": 175, "y": 170.5, "tol": 1}));
    assert_eq!(scattered_program(&mut s, 1, &copies), vec![(1, vec![3, 1, 4, 6, 2, 5, 7])]);
    let _m = on_machine("B565 0\n");
    assert_eq!(scattered_program(&mut s, 1, &copies), vec![(1, vec![3, 1, 2, 4, 5, 6, 7])]);
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

/// DEMO-HOLES as drawn at (20, 20) with hole 2 (its lower left) set by
/// hand, and on the sheet hole 2 picked, then hole 4 (its lower right): the
/// head stands at hole 4 when the rest of the part comes (CO10, CO14).
fn hand_set_hole_picked_then_another(d: &Path) -> Session {
    let mut s = scattered_job(d, false, &[(20.0, 20.0, 0, false, "")]);
    call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "add", "contours": "2"}));
    call(&mut s, "cut-order", None, json!({"mode": "pick", "sheet": 1, "x": 65, "y": 70, "tol": 1}));
    call(&mut s, "cut-order", None, json!({"mode": "pick", "sheet": 1, "x": 265, "y": 70, "tol": 1}));
    s
}

/// By proximity from the pick on hole 4's left edge (265, 70): hole 6 at
/// (170, 69.5), 3 above it, then 1 and 5 exactly as far (1 met first). Taken
/// from hole 2, where the order set by hand put it before the picks, the
/// rest would be 5, 3, 1, 6.
const AFTER_THE_PICKS: [u64; 7] = [2, 4, 6, 3, 1, 5, 7];

#[test]
fn after_picks_the_rest_of_a_part_follows_from_the_last_pick_not_from_a_hand_set_hole_already_cut() {
    let d = dir("holes_after_picks");
    let mut s = hand_set_hole_picked_then_another(&d);
    let copies = [copy_at(20.0, 20.0, 0, false)];
    assert_eq!(scattered_program(&mut s, 1, &copies), vec![(1, AFTER_THE_PICKS.to_vec())]);
    verify_clean(&mut s);
    // None: the rest in the part's own order, as before.
    let _m = on_machine("B565 0\n");
    assert_eq!(scattered_program(&mut s, 1, &copies), vec![(1, vec![2, 4, 1, 3, 5, 6, 7])]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_rest_of_a_part_taken_whole_after_picks_follows_from_the_last_pick() {
    let d = dir("holes_whole_after_picks");
    let copies = [copy_at(20.0, 20.0, 0, false)];
    // A box round the part, then a sweep across it, each after the picks
    // (MM8, MM9): the part is cut whole there, from the last pick.
    for parts in [json!({"mode": "parts", "sheet": 1, "box": "10,10; 330,230"}), json!({"mode": "parts", "sheet": 1, "points": "10,120; 330,120"})] {
        let mut s = hand_set_hole_picked_then_another(&d);
        call(&mut s, "cut-order", None, parts.clone());
        let info = call(&mut s, "info", None, json!({}));
        assert_eq!(info["data"]["sheets"][0]["manual_cuts"], 3, "{parts}: {info}");
        assert_eq!(scattered_program(&mut s, 1, &copies), vec![(1, AFTER_THE_PICKS.to_vec())], "{parts}");
        verify_clean(&mut s);
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_frames_holes_follow_the_setting_and_the_part_in_its_window_still_goes_before_the_window() {
    let d = dir("holes_frame");
    let mut s = Session::new(Ctx::builtin());
    call(&mut s, "new", Some(d.join("frame.snjob").to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 3}));
    call(&mut s, "import-drawing", None, json!({"path": frame_with_window(&d).to_str().unwrap(), "qty": 1, "layers": "MARK=mark"}));
    call(&mut s, "add-shape", None, json!({"shape": "plate-hole", "w": 150, "h": 80, "qty": 1, "name": "DEMO-INNER"}));
    call(&mut s, "edit-part", None, json!({"part": 1, "mirror": true}));
    // The frame mirrored: its Ø16 hole on the left (80, 150), the window
    // (110–310, 90–210) holding the inner part. Its outline picked: it is
    // cut whole, its holes nearest first from the origin.
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50, "mirror": true}));
    call(&mut s, "place", None, json!({"part": 2, "x": 130, "y": 110}));
    call(&mut s, "cut-order", None, json!({"mode": "pick", "sheet": 1, "x": 50, "y": 150, "tol": 1}));
    // The frame's paths by size: outline 3, window 1, hole 2; its mark 4.
    let frame = |p: &[ReadPath]| -> Vec<u64> { p.iter().map(|q| if q.mark { 4 } else if q.size() > 50_000.0 { 3 } else if q.size() > 10_000.0 { 1 } else { 2 }).collect() };
    let runs = |s: &mut Session| -> (Vec<u64>, Vec<(usize, usize)>) {
        let text = call(s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
        let paths = iso_paths(&text);
        let order = frame(&paths.iter().find(|x| x.0 == 1).unwrap().1);
        let mut out: Vec<(usize, usize)> = vec![];
        let mut part = 0;
        for l in text.lines() {
            if let Some(r) = l.strip_prefix("( PART ") {
                part = r.split(' ').next().unwrap().parse().unwrap();
            }
            if l.contains("M07 ( PIERCE") || l.contains("M10 ( MARK ON )") {
                match out.last_mut() {
                    Some(o) if o.0 == part => o.1 += 1,
                    _ => out.push((part, 1)),
                }
            }
        }
        (order, out)
    };
    // By proximity: the mark, the Ø16 hole, then the inner part whole
    // (hole, outline) before the window, then the outline (CO2).
    assert_eq!(runs(&mut s), (vec![4, 2, 1, 3], vec![(1, 2), (2, 2), (1, 2)]));
    verify_clean(&mut s);
    // None: the window first, after the part in it.
    let _m = on_machine("B565 0\n");
    assert_eq!(runs(&mut s), (vec![4, 1, 2, 3], vec![(2, 2), (1, 4)]));
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

/// A made-up 500 × 200 plate: a 60 × 60 hole (contour 1) with Ø6 holes
/// above (2) and below (7) it, a 60 × 40 hole (3) with a Ø6 hole beside it
/// (4) and a 59.9 × 15 slot above it (6), a Ø6 hole between them (5); the
/// outline is 8.
fn plate_with_big_and_small_holes(d: &Path) -> PathBuf {
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| format!("0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n{x0}\n20\n{y0}\n10\n{x1}\n20\n{y0}\n10\n{x1}\n20\n{y1}\n10\n{x0}\n20\n{y1}\n");
    let small = |x: f64, y: f64| format!("0\nCIRCLE\n8\n0\n10\n{x}\n20\n{y}\n30\n0\n40\n3\n");
    let body = [rect(0.0, 0.0, 500.0, 200.0), rect(40.0, 70.0, 100.0, 130.0), small(70.0, 150.0), rect(300.0, 80.0, 360.0, 120.0), small(380.0, 100.0), small(200.0, 100.0), rect(280.0, 130.0, 339.9, 145.0), small(70.0, 45.0)].concat();
    let p = d.join("DEMO-SMALL.dxf");
    std::fs::write(&p, format!("0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n")).unwrap();
    p
}

#[test]
fn with_cut_small_holes_first_the_small_holes_next_to_a_big_hole_go_before_it() {
    let d = dir("holes_small_first");
    // Big: 60 mm along X or Y, so the 60 × 60 and 60 × 40 holes are big
    // (exactly at the limit) and the 59.9 mm slot is small; a small hole
    // within 25 mm of a big hole's box is next to it (CO25).
    let limits = "B587 1\nB588 60\nB589 60\nB590 25\n";
    let order = |codes: &str| {
        let _m = on_machine(codes);
        let mut s = Session::new(Ctx::builtin());
        call(&mut s, "new", Some(d.join("small.snjob").to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 3, "force": true}));
        call(&mut s, "import-drawing", None, json!({"path": plate_with_big_and_small_holes(&d).to_str().unwrap(), "qty": 1}));
        call(&mut s, "place", None, json!({"part": 1, "x": 20, "y": 20}));
        let text = call(&mut s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
        verify_clean(&mut s);
        // The program's holes by size and place, as part-info numbers them.
        let at = [[70.0, 100.0], [70.0, 150.0], [330.0, 100.0], [380.0, 100.0], [200.0, 100.0], [309.95, 137.5], [70.0, 45.0]];
        let cut: Vec<u64> = iso_paths(&text)[0]
            .1
            .iter()
            .map(|p| {
                if p.size() > 50_000.0 {
                    return 8;
                }
                let q = p.centre();
                (0..7).min_by(|&a, &b| (at[a][0] + 20.0 - q[0]).hypot(at[a][1] + 20.0 - q[1]).total_cmp(&(at[b][0] + 20.0 - q[0]).hypot(at[b][1] + 20.0 - q[1]))).unwrap() as u64 + 1
            })
            .collect();
        (part_cut_order(&mut s, 1), cut)
    };
    // Off (the default): the part's own order under None.
    let none = vec![1, 2, 3, 4, 5, 6, 7, 8];
    assert_eq!(order("B565 0\n"), ((none.clone(), vec![]), none.clone()));
    // On, None: holes 2 and 7 before the big hole 1, hole 4 and the slot
    // before the big hole 3; hole 5 is next to neither.
    let want = vec![2, 7, 1, 4, 6, 3, 5, 8];
    assert_eq!(order(&format!("B565 0\n{limits}")), ((want.clone(), vec![]), want));
    // On, By proximity: the nearest hole each time, but a big hole only
    // after the small ones next to it.
    let want = vec![7, 2, 1, 5, 6, 4, 3, 8];
    assert_eq!(order(&format!("B565 1\n{limits}")), ((want.clone(), vec![]), want));
    // Limits at 0 (the original's defaults) make every hole big: no change.
    assert_eq!(order("B565 0\nB587 1\n"), ((none.clone(), vec![]), none));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_part_taken_whole_by_a_sweep_or_a_box_cuts_its_holes_in_the_machines_order_where_it_sits() {
    let d = dir("holes_taken_whole");
    // One copy as drawn at (20, 20) and one turned 90° at (400, 20); the
    // sheet would cut the left one first.
    let placed = [(20.0, 20.0, 0, false, ""), (400.0, 20.0, 90, false, "")];
    let copies = [copy_at(20.0, 20.0, 0, false), copy_at(400.0, 20.0, 90, false)];
    let mut s = scattered_job(&d, false, &placed);
    // A sweep from the turned copy to the other takes both whole, in that
    // order (MM8). The turned copy is cut first, its holes nearest-first
    // from the sheet's origin where it sits (CO14): as on a sheet of its
    // own. The other follows from the turned copy's middle (500, 170): its
    // right-hand hole 1 first.
    call(&mut s, "cut-order", None, json!({"mode": "parts", "sheet": 1, "points": "500,170; 170,120"}));
    let swept = vec![(2, vec![5, 3, 1, 4, 6, 2, 7]), (1, vec![1, 4, 6, 2, 5, 3, 7])];
    assert_eq!(scattered_program(&mut s, 1, &copies), swept);
    verify_clean(&mut s);
    // Order mode None: each part taken whole cuts its holes in its own order.
    {
        let _m = on_machine("B565 0\n");
        assert_eq!(scattered_program(&mut s, 1, &copies), vec![(2, vec![1, 2, 3, 4, 5, 6, 7]), (1, vec![1, 2, 3, 4, 5, 6, 7])]);
        verify_clean(&mut s);
    }
    // A box round the turned copy takes it whole (MM9): the same order.
    call(&mut s, "cut-order", None, json!({"mode": "clear", "sheet": 1}));
    call(&mut s, "cut-order", None, json!({"mode": "parts", "sheet": 1, "box": "390,10; 610,330"}));
    assert_eq!(scattered_program(&mut s, 1, &copies), swept);
    // On the LC5, a box round a turned copy on a sheet of its own; the
    // read-back passes, or the program would be refused (BL39).
    let mut s = scattered_job(&d, true, &[(400.0, 20.0, 90, false, "")]);
    call(&mut s, "cut-order", None, json!({"mode": "parts", "sheet": 1, "box": "390,10; 610,330"}));
    assert_eq!(scattered_program(&mut s, 1, &[copy_at(400.0, 20.0, 90, false)]), vec![(1, vec![5, 3, 1, 4, 6, 2, 7])]);
    verify_clean(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_machines_report_shows_how_the_machine_orders_a_parts_holes() {
    let hole_order = |tech: Option<&str>| {
        let mut ctx = Ctx::builtin();
        if let Some(t) = tech {
            ctx.tech = crate::ct1::MachineTech::from_text(t);
        }
        let mut s = Session::new(ctx);
        call(&mut s, "machines", None, json!({}))["data"]["technology"]["hole_order"].clone()
    };
    // No machine file: the original's defaults (CO20).
    let h = hole_order(None);
    assert_eq!(h["mode"], json!("by proximity"), "{h}");
    assert_eq!((h["mode_code"].clone(), h["start_corner"].clone(), h["direction"].clone()), (json!(1), json!("nearest the head"), json!("rows")), "{h}");
    assert_eq!((h["orthogonality_tolerance_mm"].clone(), h["small_holes_first"].clone(), h["big_hole_min_length_mm"].clone()), (json!(1.0), json!(false), json!(0.0)), "{h}");
    // A made-up machine file that sets them.
    let h = hole_order(Some("# Demo laser\nB565 2\nB566 3\nB567 2\nB568 0.5\nB587 1\nB588 40\nB589 30\nB590 2.5\n"));
    assert_eq!((h["mode"].clone(), h["start_corner"].clone(), h["direction"].clone(), h["orthogonality_tolerance_mm"].clone()), (json!("by direction"), json!("upper right"), json!("columns"), json!(0.5)), "{h}");
    assert_eq!((h["small_holes_first"].clone(), h["big_hole_min_length_mm"].clone(), h["big_hole_min_width_mm"].clone(), h["big_hole_offset_mm"].clone()), (json!(true), json!(40.0), json!(30.0), json!(2.5)), "{h}");
    // Minimum heat says what the app does with it.
    let h = hole_order(Some("B565 3\n"));
    assert_eq!(h["mode"], json!("minimum heat"));
    assert!(h["cuts"].as_str().unwrap().contains("own contour order"), "{h}");
}

// ---------------------------------------------------------------------------
// The sheet and hole order changed in Settings → Machines (MAC-53, CO16, CO17).
// ---------------------------------------------------------------------------

/// The app's settings for a made-up Lantek folder in `root` whose machine
/// file holds `codes`, with `machines` (the first in use).
fn shop_settings(root: &Path, codes: &str, machines: Vec<crate::settings::Machine>) -> crate::settings::AppSettings {
    std::fs::create_dir_all(root.join("MchFiles")).unwrap();
    std::fs::write(root.join("MchFiles").join("PstAdi06.CT1"), codes).unwrap();
    crate::settings::AppSettings { lantek_dir: root.to_string_lossy().into_owned(), machines, active_machine: 0, ..Default::default() }
}

/// A made-up machine on the G-code post or the LC5, its order changed by `order`.
fn demo_machine_with(name: &str, blm: bool, order: crate::ct1::OrderChanges) -> crate::settings::Machine {
    let output = if blm { crate::settings::Output::BlmSiemens } else { crate::settings::Output::Iso };
    crate::settings::Machine { name: name.into(), output, order, ..Default::default() }
}

/// Put the session on its settings' active machine as Settings → Apply
/// does (`install_machine`: the machine file with the changes made in
/// Settings → Machines), on this test's thread only.
fn apply_machine(s: &mut Session) -> OnMachine {
    let m = s.ctx.cfg.machine();
    let tech = crate::ct1::MachineTech::for_machine(&m, &s.ctx.cfg.lantek_dir);
    s.ctx.blm = crate::engine::blm_base(&m);
    s.ctx.tech = tech.clone();
    crate::ct1::set_thread_active(Some(tech));
    OnMachine
}

/// Four 100 × 100 squares near the corners of a 1500 × 1000 sheet: #1 lower
/// left, #2 upper left, #3 lower right, #4 upper right.
const SQUARES: [[f64; 2]; 4] = [[50.0, 50.0], [50.0, 850.0], [1350.0, 50.0], [1350.0, 850.0]];

fn squares_job(d: &Path, cfg: crate::settings::AppSettings) -> Session {
    let mut s = Session::new(Ctx::builtin());
    s.ctx.cfg = cfg;
    call(&mut s, "new", Some(d.join("squares.snjob").to_str().unwrap()), json!({"w": 1500, "h": 1000, "material": "mild steel", "thickness": 3, "force": true}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 100, "h": 100, "qty": 4, "name": "DEMO-SQUARE"}));
    for at in SQUARES {
        call(&mut s, "place", None, json!({"part": 1, "x": at[0], "y": at[1]}));
    }
    s
}

/// The squares in the order sheet 1's program cuts them: the G-code
/// program's parts, or the LC5 set's calls by where each is placed. The LC5
/// set is read back before `program` shows it (BL39).
fn squares_cut(s: &mut Session) -> Vec<usize> {
    let text = call(s, "program", None, json!({"sheet": 1}))["data"].as_str().unwrap().to_string();
    if !text.contains("INITPART") {
        return iso_parts(s, 1);
    }
    lc5_calls(&text).iter().map(|o| 1 + (0..4).find(|&k| (SQUARES[k][0] - o[0]).abs() < 1.0 && (SQUARES[k][1] - o[1]).abs() < 1.0).unwrap_or_else(|| panic!("{o:?}: {text}"))).collect()
}

#[test]
fn a_sheet_order_changed_in_settings_changes_the_order_the_program_cuts_the_parts_in() {
    use crate::ct1::OrderChanges;
    for blm in [false, true] {
        let d = dir(&format!("settings_sheet_order_{blm}"));
        // A made-up machine file with start bottom off (not followed, CZ6).
        let cfg = shop_settings(&d.join("Lantek"), "# Demo laser\nB110 ON\nB111 OFF\nB114 OFF\nB115 OFF\nB122 20\nB123 10\nB565 0\n", vec![demo_machine_with("Demo laser", blm, OrderChanges::default())]);
        let mut s = squares_job(&d, cfg);
        let cut = |s: &mut Session, c: OrderChanges| {
            s.ctx.cfg.machines[0].order = c;
            let _m = apply_machine(s);
            squares_cut(s)
        };
        let corner = |left: bool, bottom: bool| OrderChanges { start_left: Some(left), start_bottom: Some(bottom), ..Default::default() };
        // The machine file's: columns from the left, each bottom up.
        assert_eq!(cut(&mut s, OrderChanges::default()), vec![1, 2, 3, 4], "lc5 {blm}");
        // Each start corner (CZ3, CO27 (c)).
        assert_eq!(cut(&mut s, corner(false, true)), vec![3, 4, 1, 2], "lc5 {blm}");
        assert_eq!(cut(&mut s, corner(true, false)), vec![2, 1, 4, 3], "lc5 {blm}");
        assert_eq!(cut(&mut s, corner(false, false)), vec![4, 3, 2, 1], "lc5 {blm}");
        // Rows, then rows and columns zig-zag in 2 × 2 zones: every other one back.
        assert_eq!(cut(&mut s, OrderChanges { rows: Some(true), ..Default::default() }), vec![1, 3, 2, 4], "lc5 {blm}");
        let two = OrderChanges { zones_x: Some(2), zones_y: Some(2), zigzag: Some(true), ..Default::default() };
        assert_eq!(cut(&mut s, OrderChanges { rows: Some(true), ..two }), vec![1, 3, 4, 2], "lc5 {blm}");
        assert_eq!(cut(&mut s, OrderChanges { zigzag: Some(false), ..two }), vec![1, 2, 3, 4], "lc5 {blm}");
        assert_eq!(cut(&mut s, two), vec![1, 2, 4, 3], "lc5 {blm}");
        // No zones (0 or 1 each way): one zone, the nearest part next from
        // the end of the last (CZ1).
        for n in [0, 1] {
            assert_eq!(cut(&mut s, OrderChanges { zones_x: Some(n), zones_y: Some(n), rows: Some(true), ..Default::default() }), vec![1, 2, 4, 3], "lc5 {blm}, {n} zones");
        }
        // Many zones: still columns from the left, each bottom up.
        assert_eq!(cut(&mut s, OrderChanges { zones_x: Some(500), zones_y: Some(500), ..Default::default() }), vec![1, 2, 3, 4], "lc5 {blm}");
        // A change equal to the file's value cuts as the file does.
        assert_eq!(cut(&mut s, OrderChanges { zones_x: Some(20), start_left: Some(true), ..Default::default() }), vec![1, 2, 3, 4], "lc5 {blm}");
        verify_clean(&mut s);
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn a_hole_order_changed_in_settings_changes_the_order_the_program_cuts_a_parts_holes_in() {
    use crate::ct1::OrderChanges;
    for blm in [false, true] {
        let d = dir(&format!("settings_hole_order_{blm}"));
        // The machine file: By proximity (CO20).
        let cfg = shop_settings(&d.join("Lantek"), "# Demo laser\nB565 1\n", vec![demo_machine_with("Demo laser", blm, OrderChanges::default())]);
        let mut s = scattered_job(&d, blm, &[(20.0, 20.0, 0, false, "")]);
        s.ctx.cfg = cfg;
        let copies = [copy_at(20.0, 20.0, 0, false)];
        let cut = |s: &mut Session, c: OrderChanges| {
            s.ctx.cfg.machines[0].order = c;
            let _m = apply_machine(s);
            let got = scattered_program(s, 1, &copies);
            verify_clean(s);
            got[0].1.clone()
        };
        let direction = OrderChanges { hole_mode: Some(2), ..Default::default() };
        // The same orders the machine file's settings give (CO14).
        assert_eq!(cut(&mut s, OrderChanges::default()), vec![2, 5, 3, 1, 4, 6, 7], "lc5 {blm}: by proximity");
        assert_eq!(cut(&mut s, OrderChanges { hole_mode: Some(0), ..Default::default() }), vec![1, 2, 3, 4, 5, 6, 7], "lc5 {blm}: none");
        assert_eq!(cut(&mut s, direction), vec![2, 6, 4, 1, 3, 5, 7], "lc5 {blm}: by direction");
        assert_eq!(cut(&mut s, OrderChanges { hole_columns: Some(true), ..direction }), vec![2, 5, 3, 6, 4, 1, 7], "lc5 {blm}: columns");
        assert_eq!(cut(&mut s, OrderChanges { hole_start_corner: Some(3), ..direction }), vec![1, 3, 5, 2, 6, 4, 7], "lc5 {blm}: from the upper right");
        // Holes 3 and 6 are 0.5 mm off their rows: below that each is a row of its own.
        assert_eq!(cut(&mut s, OrderChanges { orthogonality_tolerance: Some(0.49), ..direction }), vec![6, 4, 2, 5, 1, 3, 7], "lc5 {blm}: tolerance");
        // "Use the machine file": by proximity again.
        assert_eq!(cut(&mut s, OrderChanges::default()), vec![2, 5, 3, 1, 4, 6, 7], "lc5 {blm}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn the_machines_report_shows_each_order_setting_in_use_and_where_it_comes_from() {
    use crate::ct1::OrderChanges;
    let d = dir("settings_order_report");
    let changed = OrderChanges { start_bottom: Some(false), zones_x: Some(4), zones_y: Some(0), hole_mode: Some(2), hole_columns: Some(true), small_holes_first: Some(true), big_hole_min_length: Some(40.0), ..Default::default() };
    let cfg = shop_settings(&d.join("Lantek"), "# Demo laser\nB110 OFF\nB111 OFF\nB114 ON\nB565 1\n", vec![demo_machine_with("Demo laser", false, changed), demo_machine_with("Demo plasma", false, OrderChanges::default())]);
    let mut s = Session::new(Ctx::builtin());
    s.ctx.cfg = cfg;
    let report = |s: &mut Session| {
        let _m = apply_machine(s);
        call(s, "machines", None, json!({}))["data"].clone()
    };
    let r = report(&mut s);
    let (sheet, holes) = (&r["technology"]["sheet_order"], &r["technology"]["hole_order"]);
    // In use: the file's right-hand start and rows, the top row and zones changed.
    assert_eq!((sheet["start_corner"].clone(), sheet["direction"].clone(), sheet["zigzag"].clone(), sheet["zones"].clone()), (json!("upper right"), json!("rows"), json!(false), json!([4, 0])), "{sheet}");
    assert_eq!(sheet["from"], json!({"start_corner": "Settings → Machines", "direction": "machine file", "zigzag": "default", "zones": "Settings → Machines"}), "{sheet}");
    assert_eq!(sheet["machine_file"], json!({"start_corner": "lower right", "direction": "rows", "zigzag": false, "zones": [20, 10]}), "{sheet}");
    // The file's start bottom is off: shown as what the sheet does (CZ6).
    assert!(sheet["note"].as_str().unwrap_or_else(|| panic!("{sheet}")).contains("bottom"), "{sheet}");
    assert_eq!((holes["mode"].clone(), holes["direction"].clone(), holes["small_holes_first"].clone(), holes["big_hole_min_length_mm"].clone()), (json!("by direction"), json!("columns"), json!(true), json!(40.0)), "{holes}");
    assert_eq!(holes["from"]["mode"], json!("Settings → Machines"), "{holes}");
    assert_eq!((holes["from"]["start_corner"].clone(), holes["from"]["orthogonality_tolerance_mm"].clone()), (json!("default"), json!("default")), "{holes}");
    assert_eq!(holes["machine_file"]["mode"], json!("by proximity"), "{holes}");
    // Each machine's own changes; the second has none.
    assert_eq!(r["machines"][0]["order_changes"]["zones_x"], json!(4), "{r}");
    assert_eq!(r["machines"][1]["order_changes"], json!({}), "{r}");
    // Switching the active machine: the second one cuts as its file says.
    s.ctx.cfg.active_machine = 1;
    let r = report(&mut s);
    let sheet = &r["technology"]["sheet_order"];
    assert_eq!((sheet["start_corner"].clone(), sheet["zones"].clone(), r["technology"]["hole_order"]["mode"].clone()), (json!("lower right"), json!([20, 10]), json!("by proximity")), "{r}");
    assert_eq!((sheet["from"]["start_corner"].clone(), sheet["from"]["zones"].clone()), (json!("machine file"), json!("default")), "{sheet}");
    // A machine file that sets nothing: the defaults, said so.
    let mut s = Session::new(Ctx::builtin());
    s.ctx.cfg = shop_settings(&d.join("Empty"), "", vec![demo_machine_with("Demo laser", false, OrderChanges { rows: Some(true), ..Default::default() })]);
    let r = report(&mut s);
    let sheet = &r["technology"]["sheet_order"];
    assert_eq!((sheet["start_corner"].clone(), sheet["direction"].clone(), sheet["zones"].clone()), (json!("lower left"), json!("rows"), json!([20, 10])), "{sheet}");
    assert_eq!((sheet["from"]["direction"].clone(), sheet["from"]["zones"].clone()), (json!("Settings → Machines"), json!("default")), "{sheet}");
    assert!(sheet["note"].is_null(), "{sheet}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_saved_order_value_the_machine_cant_take_is_reported_from_the_file_and_cut_as_the_file_says() {
    use crate::ct1::OrderChanges;
    let d = dir("settings_order_rejected");
    // The machine file: By direction from the upper right, 0.5 mm.
    let codes = "# Demo laser\nB565 2\nB566 3\nB568 0.5\n";
    // Saved changes the settings can't take (a mode and a corner they don't
    // have, a negative tolerance, too many zones) beside one they can.
    let bad = OrderChanges { hole_mode: Some(9), hole_start_corner: Some(9), orthogonality_tolerance: Some(-1.0), zones_x: Some(100_000), hole_columns: Some(true), ..Default::default() };
    let columns = OrderChanges { hole_columns: Some(true), ..Default::default() };
    let copies = [copy_at(20.0, 20.0, 0, false)];
    let mut s = scattered_job(&d, false, &[(20.0, 20.0, 0, false, "")]);
    let on = |s: &mut Session, c: OrderChanges| {
        s.ctx.cfg = shop_settings(&d.join("Lantek"), codes, vec![demo_machine_with("Demo laser", false, c)]);
        let _m = apply_machine(s);
        let r = call(s, "machines", None, json!({}))["data"].clone();
        (r, scattered_program(s, 1, &copies))
    };
    let (r, cut) = on(&mut s, bad);
    // The program cuts as the file says, with the one change it can take.
    assert_eq!(cut, on(&mut s, columns).1);
    assert_ne!(cut, on(&mut s, OrderChanges::default()).1, "the columns change is used");
    // The report says the same: the file's values, from the file.
    let (h, sheet) = (&r["technology"]["hole_order"], &r["technology"]["sheet_order"]);
    assert_eq!((h["mode"].clone(), h["start_corner"].clone(), h["orthogonality_tolerance_mm"].clone(), h["direction"].clone()), (json!("by direction"), json!("upper right"), json!(0.5), json!("columns")), "{h}");
    assert_eq!(
        (h["from"]["mode"].clone(), h["from"]["start_corner"].clone(), h["from"]["orthogonality_tolerance_mm"].clone(), h["from"]["direction"].clone()),
        (json!("machine file"), json!("machine file"), json!("machine file"), json!("Settings → Machines")),
        "{h}"
    );
    assert_eq!((sheet["zones"].clone(), sheet["from"]["zones"].clone()), (json!([20, 10]), json!("default")), "{sheet}");
    assert_eq!(r["machines"][0]["order_changes"], json!({"hole_columns": true}), "{r}");
    let _ = std::fs::remove_dir_all(&d);
}


// Jobs in use (CU5–CU12) through the agents' operations.

/// CU12: an agent never takes a hold. It reads a job someone has in use
/// (`library list` says who and since when, `library open` works and says
/// so), and is refused saving over it (also after renaming it), moving it,
/// and renaming, moving or deleting a folder that holds it several folders
/// down, each refusal naming who and since when and what to do instead. Its
/// own user's window holding the job refuses it too. A copy is allowed, and
/// once the job is free its save goes through.
#[test]
fn agents_are_refused_a_job_in_use_and_never_hold_one() {
    use crate::engine::inuse;
    use crate::library::{Kind, Library};
    let d = dir("agents_in_use");
    let clock = Clock::at(1_794_215_640); // 2026-11-09 09:14 UTC
    let mut ctx = stock_ctx(&d);
    ctx.who = Who::new("sam", "PROG-1").as_agent();
    ctx.clock = clock.clone();
    let lib = ctx.library_dir.clone();
    let mut s = Session::new(ctx);
    let jp = d.join("DEMO-7.snjob");
    call(&mut s, "new", Some(jp.to_str().unwrap()), json!({"w": 2000, "h": 1000, "thickness": 3}));
    call(&mut s, "rename", None, json!({"name": "DEMO-7"}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 300, "qty": 2, "name": "DEMO-P1"}));
    let id = call(&mut s, "library", None, json!({"mode": "save-job"}))["data"]["id"].as_u64().unwrap();
    for (folder, name) in [("", "Acme"), ("Acme", "2026-11"), ("Acme/2026-11", "Week 46")] {
        call(&mut s, "job-folders", None, json!({"mode": "new", "folder": folder, "name": name}));
    }
    call(&mut s, "job-folders", None, json!({"mode": "move", "saved_job": "DEMO-7", "to": "Acme/2026-11/Week 46"}));
    assert!(!inuse::path(&lib, id).exists(), "an agent took a hold");
    // Sam opens it in the window on PROG-2.
    let window = Library::open(&lib, &Who::new("Sam", "PROG-2"), &clock).unwrap();
    inuse::take(&window, id, "DEMO-7").unwrap();
    clock.advance(std::time::Duration::from_secs(60));
    // Reading is allowed.
    let l = call(&mut s, "library", None, json!({"mode": "list"}));
    let row = l["data"].as_array().unwrap().iter().find(|r| r["name"] == "DEMO-7").cloned().unwrap();
    assert_eq!((row["in_use"].as_str(), row["in_use_since"].as_str()), (Some("Sam on PROG-2 since 09:14"), Some("2026-11-09 09:14")), "{row}");
    let r = call(&mut s, "library", None, json!({"mode": "open", "id": id}));
    assert!(r["message"].as_str().unwrap().contains("DEMO-7 is in use by Sam on PROG-2 since 09:14."), "{r}");
    // Changing it is refused, naming who and since when.
    let said = "DEMO-7 is in use by Sam on PROG-2 since 09:14. Try again later.";
    let record = std::fs::read(window.path(Kind::Job, id)).unwrap();
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 200, "h": 100, "qty": 1, "name": "DEMO-P2"}));
    let e = err(&mut s, "library", json!({"mode": "save-job"}));
    assert!(e.contains(said) && e.contains("Nothing was saved") && e.contains("copy=true"), "{e}");
    call(&mut s, "rename", None, json!({"name": "DEMO-7 frames"}));
    let e = err(&mut s, "library", json!({"mode": "save-job"}));
    assert!(e.contains("is in use by Sam on PROG-2 since 09:14. Try again later."), "{e}");
    assert_eq!(std::fs::read(window.path(Kind::Job, id)).unwrap(), record, "a refused save wrote the record");
    let e = err(&mut s, "job-folders", json!({"mode": "move", "saved_job": "DEMO-7", "to": "/"}));
    assert!(e.starts_with("DEMO-7 is in use by Sam on PROG-2 since 09:14. It can't be moved while it's in use. Try again later. Nothing was changed."), "{e}");
    let deep = "'Acme/2026-11/Week 46' holds DEMO-7, which is in use by Sam on PROG-2 since 09:14.";
    let e = err(&mut s, "job-folders", json!({"mode": "rename", "folder": "Acme", "name": "Acme Ltd"}));
    assert!(e.contains("'Acme' holds DEMO-7, which is in use by Sam on PROG-2 since 09:14.") && e.contains("Try again later"), "{e}");
    let e = err(&mut s, "job-folders", json!({"mode": "move", "folder": "Acme/2026-11", "to": "/"}));
    assert!(e.contains("'Acme/2026-11' holds DEMO-7, which is in use by Sam on PROG-2 since 09:14."), "{e}");
    let e = err(&mut s, "job-folders", json!({"mode": "delete", "folder": "Acme/2026-11/Week 46"}));
    assert!(e.contains(deep) && e.contains("only an empty folder can be deleted"), "{e}");
    let l = call(&mut s, "job-folders", None, json!({"mode": "list"}));
    assert!(l["data"]["outline"].as_array().unwrap().iter().any(|p| p == "Acme/2026-11/Week 46/DEMO-7"), "{l}");
    // Its own user's window holding it refuses it as well.
    inuse::release(&window, &[id]).unwrap();
    inuse::take(&Library::open(&lib, &Who::new("sam", "PROG-1"), &clock).unwrap(), id, "DEMO-7").unwrap();
    let e = err(&mut s, "job-folders", json!({"mode": "move", "saved_job": "DEMO-7", "to": "/"}));
    assert!(e.contains("in use by sam on PROG-1 since 09:15"), "{e}");
    // A copy is no change to it.
    let copy = call(&mut s, "library", None, json!({"mode": "save-job", "copy": true}))["data"]["id"].as_u64().unwrap();
    assert_ne!(copy, id);
    // Free again: the agent's change goes through, and it holds nothing.
    inuse::release(&Library::open(&lib, &Who::new("sam", "PROG-1"), &clock).unwrap(), &[id]).unwrap();
    call(&mut s, "job-folders", None, json!({"mode": "move", "saved_job": "DEMO-7", "to": "/"}));
    call(&mut s, "job-folders", None, json!({"mode": "rename", "folder": "Acme", "name": "Acme Ltd"}));
    assert!(std::fs::read_dir(lib.join(inuse::FOLDER)).map_or(0, |d| d.count()) == 0, "a hold is left");
    let _ = std::fs::remove_dir_all(&d);
}

/// LB16: agents never wait for a refresh. Every `library` read reads the
/// Library as it is now, so a job another PC saved a moment ago is listed,
/// and opened as saved now, at once.
#[test]
fn an_agent_sees_another_pcs_save_at_once() {
    let d = dir("refresh_agent");
    let clock = Clock::at(1_794_216_000);
    let mut ctx = stock_ctx(&d);
    ctx.who = Who::new("kim", "PROG-2").as_agent();
    ctx.clock = clock.clone();
    let lib = crate::library::Library::open(&ctx.library_dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
    let mut s = Session::new(ctx);
    let jp = d.join("DEMO-7.snjob");
    let jp = jp.to_str().unwrap();
    call(&mut s, "new", Some(jp), json!({"w": 3000, "h": 1500, "thickness": 10}));
    let names = |s: &mut Session| call(s, "library", None, json!({"mode": "list"}))["data"].as_array().unwrap().iter().map(|e| format!("{} ({})", e["name"].as_str().unwrap(), e["saved_by"].as_str().unwrap_or("-"))).collect::<Vec<_>>();
    assert!(names(&mut s).is_empty());
    let mut j = JobFile::new_job("DEMO-7", crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 10.0, level: 3, machine: Default::default(), kerf: Default::default() });
    let rec = lib.save_job(&j, crate::library::SaveTo::New).unwrap();
    assert_eq!(names(&mut s), ["DEMO-7 (sam on PROG-1)"]);
    j.parts = vec![crate::nest::Part { name: "DEMO-P1".into(), shape: crate::nest::Shape::Rectangle, w: 600.0, h: 400.0, qty: 4, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
    lib.save_job(&j, crate::library::SaveTo::Read(rec)).unwrap();
    call(&mut s, "library", None, json!({"mode": "open", "name": "DEMO-7"}));
    assert_eq!(jobfile::load(Path::new(jp)).unwrap().parts.iter().map(|p| p.qty).collect::<Vec<_>>(), [4], "opened as saved now");
    let _ = std::fs::remove_dir_all(&d);
}
