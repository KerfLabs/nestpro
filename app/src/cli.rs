//! The command line: `nestpro <operation> [job.snjob] [--arg=value …]`, and
//! `nestpro mcp` for the MCP server. Plain `nestpro` (or a file to open) still
//! starts the window. Answers are JSON on stdout (`--text` for plain lines);
//! exit code 0 = done, 1 = the operation refused or failed, 2 = bad command.

use crate::engine::ops::{self, Kind};
use crate::engine::session::Session;
use crate::engine::Ctx;
use serde_json::{json, Map, Value};

/// Print a line; a reader that stopped listening (`… | head`) is not an error.
macro_rules! say {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($t)*);
    }};
}

/// Words that start the command line instead of the window.
pub fn is_cli(args: &[String]) -> bool {
    match args.get(1).map(String::as_str) {
        Some("help" | "--help" | "-h" | "/?" | "ops" | "mcp" | "version" | "--version") => true,
        Some(w) => !w.starts_with('-') && !w.contains('.') && !w.contains(['/', '\\']) && ops::find(w).is_some(),
        None => false,
    }
}

/// The window app is a Windows GUI program (no console of its own); when run
/// from a console, write to that console. Pipes (MCP clients, scripts) are
/// left as they are.
#[cfg(windows)]
fn attach_console() {
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
        fn GetStdHandle(n: u32) -> isize;
    }
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // SAFETY: plain Win32 calls with constant arguments.
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        if h == 0 || h == -1 {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

pub fn main(args: Vec<String>) -> i32 {
    #[cfg(windows)]
    attach_console();
    let rest = &args[2..];
    match args[1].as_str() {
        "help" | "--help" | "-h" | "/?" => {
            match rest.first().and_then(|n| ops::find(n)) {
                Some(op) => say!("{}", op_help(op)),
                None => say!("{}", usage()),
            }
            0
        }
        "version" | "--version" => {
            say!("nestpro {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "ops" => {
            if rest.iter().any(|a| a == "--gui") {
                // Which GUI ribbon command each operation stands for.
                for (k, v) in ops::GUI_MAP {
                    say!("{:<44} {v}", k.replace('\n', " "));
                }
            } else if rest.iter().any(|a| a == "--json") {
                let list: Vec<Value> = ops::OPS.iter().map(|o| json!({"name": o.name, "group": o.group, "kind": format!("{:?}", o.kind).to_lowercase(), "help": o.help, "args": o.schema()})).collect();
                say!("{}", serde_json::to_string_pretty(&list).unwrap_or_default());
            } else {
                say!("{}", op_list());
            }
            0
        }
        "mcp" => crate::mcp::serve(),
        name => run_op(name, rest),
    }
}

fn run_op(name: &str, rest: &[String]) -> i32 {
    let Some(op) = ops::find(name) else {
        eprintln!("unknown command '{name}'\n\n{}", usage());
        return 2;
    };
    let (job, args, text) = match parse(op, rest) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n\n{}", op_help(op));
            return 2;
        }
    };
    let mut s = Session::new(Ctx::load());
    match s.call(name, job.as_deref(), &args) {
        Ok(r) => {
            if text {
                say!("{}", r.json["message"].as_str().unwrap_or(""));
                match &r.json["data"] {
                    Value::Null => {}
                    Value::String(t) => say!("{t}"),
                    d => say!("{}", serde_json::to_string_pretty(d).unwrap_or_default()),
                }
                if let Some(f) = r.json["files"].as_array() {
                    for f in f {
                        say!("  {}", f.as_str().unwrap_or(""));
                    }
                }
            } else {
                say!("{}", serde_json::to_string_pretty(&r.json).unwrap_or_default());
            }
            0
        }
        Err(e) => {
            if text {
                eprintln!("{e}");
            } else {
                say!("{}", serde_json::to_string_pretty(&json!({"ok": false, "error": e})).unwrap_or_default());
            }
            1
        }
    }
}

/// `[job] --a=1 --b 2 --flag --args '{json}' --text` → (job, args, text).
fn parse(op: &ops::Op, rest: &[String]) -> Result<(Option<String>, Value, bool), String> {
    let mut job = None;
    let mut m = Map::new();
    let mut text = false;
    let mut i = 0;
    while i < rest.len() {
        let w = &rest[i];
        i += 1;
        if let Some(k) = w.strip_prefix("--") {
            let (k, v) = match k.split_once('=') {
                Some((k, v)) => (k.to_string(), Some(v.to_string())),
                None => (k.to_string(), None),
            };
            let k = k.replace('-', "_");
            if k == "text" {
                text = true;
                continue;
            }
            let takes = |name: &str| op.args.iter().find(|a| a.name == name);
            let v = match v {
                Some(v) => v,
                // A true/false option alone means true.
                None if takes(&k).is_some_and(|a| a.ty == ops::Ty::Bool) => "true".into(),
                None if i < rest.len() => {
                    i += 1;
                    rest[i - 1].clone()
                }
                None => return Err(format!("--{k} needs a value")),
            };
            match k.as_str() {
                "job" => job = Some(v),
                "args" => {
                    let extra: Value = serde_json::from_str(&v).map_err(|e| format!("--args is not JSON: {e}"))?;
                    let Value::Object(o) = extra else { return Err("--args must be a JSON object".into()) };
                    m.extend(o);
                }
                _ => {
                    m.insert(k, Value::String(v));
                }
            }
        } else if job.is_none() && op.kind != Kind::Global {
            job = Some(w.clone());
        } else {
            return Err(format!("unexpected '{w}'"));
        }
    }
    if job.is_none() && op.kind != Kind::Global {
        job = std::env::var("NESTPRO_JOB").ok().filter(|s| !s.is_empty());
    }
    if job.is_none() && op.kind != Kind::Global {
        return Err("give the job file first, e.g. `nestpro info jobs/frames.snjob` (or set NESTPRO_JOB)".into());
    }
    Ok((job, Value::Object(m), text))
}

fn usage() -> String {
    format!(
        "Sheet Nesting (nestpro) — window app, command line and MCP server.\n\n\
         USAGE\n  nestpro                         open the window\n  nestpro <file.snjob>            open a job in the window\n  \
         nestpro <operation> <job.snjob> [--arg=value …] [--text]\n  nestpro mcp                     MCP server on stdin/stdout (for AI agents)\n  \
         nestpro ops [--json|--gui]      list every operation (--gui: the GUI command each stands for)\n  nestpro help <operation>        one operation's arguments\n\n\
         A typical job:\n  nestpro new jobs/frames.snjob --w=3000 --h=1500 --material=steel --thickness=5\n  \
         nestpro import-dxf jobs/frames.snjob --path=\"bracket.dxf\" --qty=20\n  nestpro add-shape jobs/frames.snjob --shape=disc --w=300 --qty=8\n  \
         nestpro nest jobs/frames.snjob\n  nestpro leads jobs/frames.snjob --mode=machine\n  nestpro remnant jobs/frames.snjob --mode=auto --sheet=1\n  \
         nestpro verify jobs/frames.snjob\n  nestpro render jobs/frames.snjob --sheet=1\n  nestpro export jobs/frames.snjob\n\n\
         Answers are JSON on stdout; --text prints plain lines. Every change is saved to the job file (the previous version as <job>.snjob~; `undo` goes back).\n\n{}",
        op_list()
    )
}

fn op_list() -> String {
    let mut s = String::from("OPERATIONS\n");
    let mut group = "";
    for o in ops::OPS {
        if o.group != group {
            group = o.group;
            s.push_str(&format!("  {group}\n"));
        }
        let first = o.help.split(". ").next().unwrap_or(o.help).trim_end_matches('.');
        s.push_str(&format!("    {:<16} {first}\n", o.name));
    }
    s
}

fn op_help(op: &ops::Op) -> String {
    let mut s = format!("nestpro {}{} [options]\n\n{}\n", op.name, if op.kind == Kind::Global { "" } else { " <job.snjob>" }, op.help);
    if !op.args.is_empty() {
        s.push_str("\nOPTIONS\n");
        for a in op.args {
            let choices = if a.choices.is_empty() { String::new() } else { format!(" [{}]", a.choices.join("|")) };
            s.push_str(&format!("  --{:<14} {}{}{} — {}\n", a.name, a.ty.label(), choices, if a.required { ", required" } else { "" }, a.help));
        }
    }
    s
}
