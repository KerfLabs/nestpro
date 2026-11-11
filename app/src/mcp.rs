//! MCP server (Model Context Protocol, JSON-RPC 2.0, one message per line on
//! stdin/stdout) so an AI agent can program jobs like a CAM programmer: every
//! operation in `engine::ops` is a tool, `render` also returns the picture.
//! Nothing but protocol messages goes to stdout; notes go to stderr.

use crate::engine::ops::{self, Kind};
use crate::engine::session::Session;
use crate::engine::Ctx;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// How an agent should work with the tools (sent at initialize).
pub const INSTRUCTIONS: &str = "Sheet Nesting (nestpro) — 2D cutting CAM for laser/plasma/oxyfuel/waterjet, a rebuild of Lantek Expert's cutting workflow.\n\
Work like a CAM programmer: 1) `new` (job file path, sheet size, material, thickness) or pass an existing job path; an order from another system comes in with `import_xml` (Lantek import XML, one job file per material and thickness), and a machine program set back into a job with `import_program`; \
2) add parts: `import_drawing` (DXF, DWG, MEC, IGES, PDF, ESSI, DSTV, GEO; check `drawing_layers` first for multi-layer drawings) and/or `add_shape`, set quantities with `edit_part`; \
3) sheets: `stock` mode=list shows the sheets and remnants on the rack (create/add them there); `nest` takes them from stock in the stock's order (remnants first) and uses the job's own sheet when none are stocked; `sheet_add`/`sheet_change` pick a sheet by hand. Then look: `info` (usage, missing parts, which stock sheet each is) and `render` (a picture of the sheet); fix by hand with `place`/`move`/`rotate`/`unplace`/`compact` if needed; \
4) technology: `leads` (machine or auto), `joints`, `loops`, `cut_tech`, `tech_set`, `text`, or per element with `part_info` + `lead_at`/`joint_at`; one contour's lead that came out wrong: `lead_edit` (its length and angle, from `part_info`); \
5) machining: `cut_order` (optional manual order), `remnant` to keep the leftover, `verify` until every sheet is correct; \
6) `program` to read a sheet's CNC code, `export` to write all programs; 7) `stock_book` mode=book when the job is released: its sheets come out of stock and its remnants go in (later changes keep the stock in step). \
Every tool takes `job` (the .snjob path); after the first call it may be left out. Every change is saved to the job file; `undo` steps back. \
Numbers are from 1 (parts, sheets, placed items #, elements). Lengths are mm, X along the sheet length, Y up. \
Nothing is ever sent to a machine and settings are not changed from here.";

pub fn tools() -> Vec<Value> {
    ops::OPS
        .iter()
        .map(|o| {
            let mut schema = o.schema();
            if o.kind != Kind::Global {
                schema["properties"]["job"] = json!({"type": "string", "description": "The job file (.snjob). Needed for `new` and the first call; later calls use the open job."});
                if o.kind == Kind::Create {
                    schema["required"].as_array_mut().map(|r| r.push(json!("job")));
                }
            }
            let ro = !o.writes();
            json!({
                "name": o.tool_name(),
                "title": format!("{} · {}", o.group, o.name),
                "description": o.help,
                "inputSchema": schema,
                "annotations": { "readOnlyHint": ro, "destructiveHint": false, "idempotentHint": ro, "openWorldHint": false },
            })
        })
        .collect()
}

pub fn serve() -> i32 {
    let mut session = Session::new(Ctx::load());
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    eprintln!("nestpro MCP server {} ready", env!("CARGO_PKG_VERSION"));
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let r: Vec<Value> = batch.iter().filter_map(|m| handle(&mut session, m)).collect();
                if r.is_empty() { None } else { Some(Value::Array(r)) }
            }
            Ok(m) => handle(&mut session, &m),
            Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}})),
        };
        if let Some(r) = reply {
            if writeln!(out, "{}", r).and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
    0
}

/// Answer one message (None for notifications).
pub fn handle(s: &mut Session, m: &Value) -> Option<Value> {
    let id = m.get("id").cloned();
    let method = m.get("method").and_then(Value::as_str).unwrap_or("");
    let params = m.get("params").cloned().unwrap_or(Value::Null);
    let id = id?; // notifications get no answer
    let ok = |result: Value| Some(json!({"jsonrpc": "2.0", "id": id.clone(), "result": result}));
    match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let v = if PROTOCOLS.contains(&asked) { asked } else { PROTOCOLS[0] };
            ok(json!({
                "protocolVersion": v,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "nestpro", "title": "Sheet Nesting", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let mut args = params.get("arguments").cloned().unwrap_or(json!({}));
            // A job given as something other than its file's path is refused,
            // not dropped (or the call would land on the job already open).
            let job = match args.as_object_mut().and_then(|o| o.remove("job")) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(p)) => Ok(Some(p)),
                Some(_) => Err("job must be the job file's path, as text".to_string()),
            };
            ok(match job.and_then(|job| s.call(name, job.as_deref(), &args)) {
                Ok(r) => {
                    let mut content = vec![json!({"type": "text", "text": serde_json::to_string_pretty(&r.json).unwrap_or_default()})];
                    if let Some(png) = r.image {
                        content.push(json!({"type": "image", "data": base64(&png), "mimeType": "image/png"}));
                    }
                    json!({ "content": content, "isError": false })
                }
                Err(e) => json!({ "content": [{"type": "text", "text": e}], "isError": true }),
            })
        }
        _ => Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("method not found: {method}")}})),
    }
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}
