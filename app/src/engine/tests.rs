//! Engine tests: a whole job programmed through the agent operations, the way
//! the command line and the MCP server run them (job file on disk).

use super::session::{backup_path, Session};
use super::*;
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
    let layers = call(&mut s, "dxf-layers", None, json!({"path": dxf.to_str().unwrap()}));
    assert!(layers["data"].as_object().is_some_and(|m| m.contains_key("MARK")), "{layers}");
    let r = call(&mut s, "import-dxf", None, json!({"path": dxf.to_str().unwrap(), "layers": "MARK=mark", "qty": 3}));
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
    crate::stock::Stock::update(&lib, |s| s.take(&again[0], 1, false)).unwrap();
    let r = call(&mut s, "remnant", None, json!({"mode": "line", "x": mx + 220.0}));
    assert!(r["message"].as_str().unwrap().contains("already in use"), "{r}");
    assert_eq!(jobfile::load(Path::new(jp)).unwrap().formats[0].made, again);
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
    // The sheet's time by kind adds up to its time; parts cut at quality 1 (no size bands): 1500 mm/min.
    let info = call(&mut s, "info", None, json!({}));
    let sh = &info["data"]["sheets"][0];
    let t = &sh["times_s"];
    let sum: f64 = ["cut", "mark", "rapid", "pierce", "head"].iter().map(|k| t[k].as_f64().unwrap()).sum();
    assert!((sum - sh["time_s"].as_f64().unwrap()).abs() < 0.3, "{sh}");
    // Two 600 mm outsides at 1500 mm/min ≈ 48 s of cutting; two head-ups of 0.7 s.
    assert!((t["cut"].as_f64().unwrap() - 48.0).abs() < 2.0, "{t}");
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
