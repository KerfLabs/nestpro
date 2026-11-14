//! Every operation an agent (or a script) can run on a job — the registry the
//! command line and the MCP tools are built from. Numbers an agent sees are
//! 1-based like on screen: part 1, sheet 1, item #1 on a sheet, element 1.
//!
//! Keeping up with the GUI: `GUI_MAP` names, for every ribbon command, the
//! operation that does the same here (or why it has none). The test
//! `every_ribbon_command_is_mapped` fails when a ribbon button is added
//! without an entry, so a new GUI feature can't silently skip the agents.

use super::selection::{Take, Window};
use super::*;
use crate::nest::{CutQuality, JointType, LeadType, TextKind};
use serde_json::{json, Map, Value};

/// What kind of value an argument takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ty {
    Str,
    Num,
    Int,
    Bool,
    /// Parts: "all", a number (1-based), a name, or a list of them.
    Parts,
    /// Points "x,y;x,y;…" in mm (or a JSON list of [x, y]).
    Points,
    /// A file or folder.
    Path,
}

impl Ty {
    pub fn label(self) -> &'static str {
        match self {
            Ty::Str => "text",
            Ty::Num => "number",
            Ty::Int => "integer",
            Ty::Bool => "true/false",
            Ty::Parts => "parts",
            Ty::Points => "points",
            Ty::Path => "path",
        }
    }
}

pub struct Arg {
    pub name: &'static str,
    pub ty: Ty,
    pub required: bool,
    pub help: &'static str,
    /// Allowed values (empty = any).
    pub choices: &'static [&'static str],
}

const fn a(name: &'static str, ty: Ty, help: &'static str) -> Arg {
    Arg { name, ty, required: false, help, choices: &[] }
}
const fn r(name: &'static str, ty: Ty, help: &'static str) -> Arg {
    Arg { name, ty, required: true, help, choices: &[] }
}
const fn c(name: &'static str, choices: &'static [&'static str], help: &'static str) -> Arg {
    Arg { name, ty: Ty::Str, required: false, help, choices }
}

/// How an operation uses the job.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Reads the job, changes nothing.
    Read,
    /// Changes the job (saved afterwards).
    Write,
    /// Makes a new job file.
    Create,
    /// Doesn't need a job.
    Global,
}

pub struct Op {
    pub name: &'static str,
    pub group: &'static str,
    pub kind: Kind,
    pub help: &'static str,
    pub args: &'static [Arg],
}

impl Op {
    /// Changes a file: the job, the sheet stock (`stock`), the Library's job
    /// or part folders (`job-folders`, `part-folders`) or a test Library
    /// folder (`library-check`).
    pub fn writes(&self) -> bool {
        matches!(self.kind, Kind::Write | Kind::Create) || matches!(self.name, "stock" | "library-check" | "job-folders" | "part-folders")
    }

    /// MCP tool name (underscores).
    pub fn tool_name(&self) -> String {
        self.name.replace('-', "_")
    }

    /// JSON schema of the arguments (MCP `inputSchema`).
    pub fn schema(&self) -> Value {
        let mut props = Map::new();
        let mut req = vec![];
        for x in self.args {
            let mut p = Map::new();
            let ty = match x.ty {
                Ty::Num => json!("number"),
                Ty::Int => json!("integer"),
                Ty::Bool => json!("boolean"),
                Ty::Parts => json!(["string", "integer", "array"]),
                Ty::Points => json!(["string", "array"]),
                Ty::Str | Ty::Path => json!("string"),
            };
            p.insert("type".into(), ty);
            p.insert("description".into(), json!(x.help));
            if !x.choices.is_empty() {
                p.insert("enum".into(), json!(x.choices));
            }
            props.insert(x.name.into(), Value::Object(p));
            if x.required {
                req.push(x.name);
            }
        }
        json!({ "type": "object", "properties": props, "required": req })
    }
}

const PARTS: Arg = a("parts", Ty::Parts, "Which parts: \"all\" (default), a part number from 1, a name, or a list (\"1,3\" or [1,\"Plate\"])");
const PART: Arg = r("part", Ty::Parts, "The part: its number (from 1) or its name");
const SHEET: Arg = a("sheet", Ty::Int, "Sheet number, from 1 (default 1)");
const ITEM: Arg = r("item", Ty::Int, "The placed part's number on the sheet (#1, #2… as in info/render)");
const ITEM_ONE: Arg = a("item", Ty::Int, "One placed part's number on the sheet (#1, #2… as in info/render); or use items");
const ITEMS: Arg = a("items", Ty::Str, "Several placed parts that go together, e.g. \"1,3,5\" or \"2-4\" (or use item)");
const SHAPES: &[&str] = &["rectangle", "plate-hole", "strip", "l-bracket", "disc", "flange", "gusset", "slotted-plate"];
const MACHINES: &[&str] = &["laser", "plasma", "oxyfuel", "waterjet", "laserpunch", "punch"];
const LEADS: &[&str] = &["machine", "perpendicular", "line", "arc", "line-arc", "corner"];
const JOINTS: &[&str] = &["gap", "leads", "leads-inclined", "corner"];
const QUALITIES: &[&str] = &["auto", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];
const CUT_QUALITIES: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];
const TEXTS: &[&str] = &["mark", "special", "note"];
const CONTOUR_STATES: &[&str] = &["cut", "mark", "no-cut", "bend-line", "leave-out"];

pub static OPS: &[Op] = &[
    // --- job ---
    Op { name: "new", group: "Job", kind: Kind::Create, help: "Make a new, empty job file with its sheet (size, material, thickness, gas).", args: &[
        a("name", Ty::Str, "Job name (default: the file name)"), a("week", Ty::Str, "Folder/week the job belongs to"),
        a("w", Ty::Num, "Sheet length X, mm (default 2000)"), a("h", Ty::Num, "Sheet width Y, mm (default 1000)"),
        a("material", Ty::Str, "Material name or number (see catalog), default mild steel"), a("thickness", Ty::Num, "Thickness, mm (default 3)"),
        a("gas", Ty::Str, "Gas the job is cut with: its name or number (1 Nitrogen, 2 Air, 3 Oxygen, 4 Argon, 5 Water); default the machine's (Settings → Machines)"),
        a("margin", Ty::Num, "Sheet margin, mm (default 5)"), a("gap", Ty::Num, "Gap between parts, mm (default 5)"),
        a("level", Ty::Int, "Automatic nesting level 1-5 (default 3)"), c("machine", MACHINES, "Machine type (default laser)"),
        a("force", Ty::Bool, "Overwrite an existing file"),
    ] },
    Op { name: "info", group: "Job", kind: Kind::Read, help: "Job report: sheet, what nest will nest on (nest_on: the stock first, with its sheets and remnants for the job, or the job's sheet size, and why), parts (ordered / nested / missing, technology), sheets (usage, cut length, pierces, time, remnant). With sheet=N, also every placed part on that sheet.", args: &[a("sheet", Ty::Int, "Also list the parts placed on this sheet")] },
    Op { name: "rename", group: "Job", kind: Kind::Write, help: "Rename the job or move it to another week/folder.", args: &[a("name", Ty::Str, "New job name"), a("week", Ty::Str, "New week/folder")] },
    Op { name: "undo", group: "Job", kind: Kind::Write, help: "Undo the last change (the MCP session keeps every step; the command line keeps one, in <job>.snjob~).", args: &[] },
    // --- parts ---
    Op { name: "add-shape", group: "Parts", kind: Kind::Write, help: "Add a built-in shape as a part.", args: &[
        Arg { name: "shape", ty: Ty::Str, required: true, help: "Shape", choices: SHAPES },
        a("w", Ty::Num, "Length X, mm"), a("h", Ty::Num, "Width Y, mm"), a("qty", Ty::Int, "Quantity ordered (default 5)"), a("name", Ty::Str, "Reference"),
        a("rotate", Ty::Bool, "May turn 90° when nesting (default true)"), a("group", Ty::Str, "Group (customer, order…): grouped parts nest in their own zone"),
    ] },
    Op { name: "import-xml", group: "Job", kind: Kind::Global, help: "Import a Lantek import XML (DXF Import / Parts Import): one new job file per material and thickness, each product's drawing (Fich) or its library product, with its quantity and rotations.", args: &[
        r("path", Ty::Path, "The XML file"), a("dir", Ty::Path, "Folder for the new job files (default: next to the XML)"), a("force", Ty::Bool, "Overwrite job files that exist"),
    ] },
    Op { name: "import-program", group: "Job", kind: Kind::Create, help: "Open a BLM LC5 machine program set (<NEST>.zip, its .XML, or a Lantek .nxp) as a new editable job: its parts, where they sit, the sheet, material and thickness, and the cut-off as a remnant line.", args: &[
        r("path", Ty::Path, "The program set (.zip / .XML / .nxp)"), a("force", Ty::Bool, "Overwrite the job file if it exists"),
    ] },
    Op { name: "drawing-layers", group: "Parts", kind: Kind::Global, help: "List a part drawing's layers (DXF/DWG layers, IGES levels, DSTV blocks, ESSI cut/mark…) and how many objects each holds, before importing it. Objects the drawing hides (frozen or off layers, invisible) or draws in a centre / hidden / dashed line type get their own row, such as `0 [CENTER]`, that starts as left out. Each part's size, holes and contours (numbered as part-info numbers them, with the state each starts in) are given as the part comes in with `layers`, so import-drawing's `contours` can be chosen before importing. Also answers to dxf-layers.", args: &[
        r("path", Ty::Path, "The drawing file"), c("format", FORMATS, "Read the file as this format (default: by its extension and content)"),
        c("dialect", &["ESSI", "BURNY", "ESAB", "MESSER", "VULCAN"], "ESSI programs: the cutting control's dialect (default: by extension)"),
        c("pdf_layers", &["colour", "pdf", "one"], "PDF drawings: layers by line colour (default), by the PDF's own layers, or all on one layer"),
        a("layers", Ty::Str, "Layer roles for the sizes and contours: \"LAYER=cut|mark|ignore,…\", as import-drawing takes them"),
    ] },
    Op { name: "import-drawing", group: "Parts", kind: Kind::Write, help: "Import part drawings as parts: DXF, DWG, Lantek MEC, IGES, PDF, ESSI, DSTV (NC1) or GEO, picked by extension and content. Every layer is cut unless `layers` says otherwise (a format's marking layers start as mark; rows like `0 [CENTER]` or `HOLES [frozen]` — what the drawing hides or draws as centre / dashed lines — start as left out, `layers=0 [CENTER]=cut` cuts them). An object drawn twice in the same place is cut once. Open lines on the part are kept but never cut: a straight one from edge to edge, a dashed one or one on a BEND layer as a bend line, any other as no-cut (contour-tech changes them; see part-info). `contours` then sets contours as contour-tech does, before the part is made (one drawing of one part; numbers as drawing-layers gives them with the same `layers`), the same as painting them in the import window's preview. Quantity comes from `qty`, else the file's own part data, else the file name (Settings → name format); a file holding several parts gives several parts. Also answers to import-dxf.", args: &[
        r("path", Ty::Path, "Drawing file, or several separated by ';'"), c("format", FORMATS, "Read every file as this format (default: by extension and content)"),
        c("dialect", &["ESSI", "BURNY", "ESAB", "MESSER", "VULCAN"], "ESSI programs: the cutting control's dialect (default: by extension — .MPG ESAB, .LOS MESSER, .DAT VULCAN, else ESSI)"),
        a("scale", Ty::Num, "PDF drawings: the drawing's scale, real size = drawing size × scale (4 for a 1:4 drawing; default 1)"),
        c("pages", &["together", "separate"], "PDF drawings: all pages side by side in one drawing (default), or one part per page"),
        c("pdf_layers", &["colour", "pdf", "one"], "PDF drawings: layers by line colour (default), by the PDF's own layers, or all on one layer"),
        a("layers", Ty::Str, "Layer roles: \"LAYER=cut|mark|ignore,…\" (others: cut, or mark for a format's marking layers)"),
        a("contours", Ty::Str, "Contour choices, in order, as contour-tech makes them: \"CONTOURS=WHAT; …\" with WHAT a state (cut, mark, no-cut, bend-line, leave-out), quality 1-12 or auto, and/or same, e.g. \"7=mark; 1=leave-out; 2=quality 1 same\""),
        a("qty", Ty::Int, "Quantity for every imported part (default: the file's, else from the file name, else 1)"), a("name", Ty::Str, "Reference (one part only)"), a("group", Ty::Str, "Group for the imported parts"),
    ] },
    Op { name: "edit-part", group: "Parts", kind: Kind::Write, help: "Change a part's reference, quantity, size (built-in shapes), rotations or group. Changing the size of a part with technology needs tech=copy (keep it, change a copy) or tech=drop (remove its technology).", args: &[
        PART, a("name", Ty::Str, "New reference"), a("qty", Ty::Int, "Quantity ordered"), a("w", Ty::Num, "Length X, mm (built-in shapes)"), a("h", Ty::Num, "Width Y, mm (built-in shapes)"),
        a("rotate", Ty::Bool, "May turn 90°"), a("mirror", Ty::Bool, "May be mirrored"), a("group", Ty::Str, "Group"), c("tech", &["copy", "drop"], "Size change on a part with technology"),
    ] },
    Op { name: "remove-part", group: "Parts", kind: Kind::Write, help: "Take a part out of the job (its placed copies go too).", args: &[PART] },
    Op { name: "part-info", group: "Parts", kind: Kind::Read, help: "A part's geometry and technology: size, area, cut length, every contour (numbered: holes first, the outline, then open lines; open or closed; its state: cut, mark, no-cut, bend-line or leave-out, set with contour-tech; cut contours with their cut quality by the machine's name, e.g. Slow (1), or Quality n when the machine names none, and where it comes from: by hand (contour-tech quality=), part size range, part, machine size band or default; each cut or marked contour's place in the cut order, cut_order from 1, and cut_order_by_hand when set with part-cut-order), the whole cut order (contours in the order cut, and those set by hand), every element (numbered, for lead-at / joint-at), texts, and Verify Part / Verify Machining results.", args: &[PART] },
    Op { name: "contour-tech", group: "Technology", kind: Kind::Write, help: "Set contours of an imported part (numbers from part-info: holes first, the outline, then open lines) to cut, mark (marked on the surface, open or closed), no-cut (kept on the part and shown, never machined), bend-line (a no-cut line in its own colour) or leave-out (not on the part), and/or give cut contours a cut quality by hand (1-12, which wins over the part's size ranges and quality and the machine's size bands; auto gives the choice back to them). same=true also sets every contour equal to one of them (same shape, moved; with only a quality, the equal contours that are cut). The outline is always cut, open lines can't be cut yet, and a mark always uses the machine's marking quality.", args: &[
        PART, r("contours", Ty::Str, "Contour numbers from part-info, e.g. \"5\", \"1,3\" or \"2-4\""),
        c("state", CONTOUR_STATES, "What the contours become (state, quality or both)"),
        c("quality", QUALITIES, "Cut quality set by hand on cut contours: the machine's quality 1-12, or auto = the part's size ranges and quality and the machine's size bands decide (state, quality or both)"),
        a("same", Ty::Bool, "Also every contour equal to one of them (default false)"),
    ] },
    Op { name: "part-cut-order", group: "Machining", kind: Kind::Write, help: "Set the order an imported part's contours are cut and marked in, by hand (part-info shows each contour's place, and which were set by hand). Each command adds to the end of the order: add (contours by number, in the order given), all-marks, all-cut, or all-quality (every cut contour at one cut quality); undo takes back the last command, clear empties the order. A contour already in the order is skipped, and a bulk command adds only contours not yet ordered, in the automatic order among themselves. Whatever isn't ordered by hand follows in the automatic order (marks, holes, outline). The outline is always cut last: naming a contour after it in one command is refused, and contours added later go in before it. Marks can go anywhere before the outline. A contour that is No cut, Bend line or Leave out has no place in the order; setting an ordered contour to another state with contour-tech takes it out of the order. Built-in shapes follow the automatic order.", args: &[
        PART, Arg { name: "mode", ty: Ty::Str, required: true, help: "add, all-marks, all-cut, all-quality, undo or clear", choices: &["add", "all-marks", "all-cut", "all-quality", "undo", "clear"] },
        a("contours", Ty::Str, "add: contour numbers from part-info in cut order, e.g. \"3,1,2\" or \"4-2\" (4, 3, 2)"),
        c("quality", CUT_QUALITIES, "all-quality: the machine's cut quality 1-12, as part-info shows each contour's"),
    ] },
    Op { name: "export-part-dxf", group: "Parts", kind: Kind::Read, help: "Save a part as DXF (outline and holes on CUT, marking on MARK).", args: &[PART, r("out", Ty::Path, "DXF file to write")] },
    Op { name: "library", group: "Parts", kind: Kind::Write, help: "The product/job library: list it (each record with who last saved it and when: saved_by, e.g. \"sam on PROG-2 (agent)\", and saved_at in this PC's local time), open a library job into this job file, add a library product to the job, or save parts / the job into it (recorded as saved by this PC's user, marked (agent)). A job opened from the library, or saved into it, saves over its own record; a save over a job or library part someone else saved since this job file read it is refused, saying who and when (LB7, LB10).", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "What to do. open: this job file becomes the library job (its own changes are dropped; it leaves the stock alone: its sheets are booked when someone saves it in the window). save-job: over the job's own record, or a new record; a job file booked with stock-book hands its booking to the record and no longer follows the stock (LB10); job names are unique across the Library, whatever their capitals; list marks old jobs that share a name (same_name_as)", choices: &["list", "open", "add", "save-part", "save-job"] },
        a("find", Ty::Str, "list: text to look for"), a("id", Ty::Int, "add: the product's record number; open: the job's"), a("name", Ty::Str, "add: the product's name; open: the job's"),
        a("qty", Ty::Int, "add: quantity"), a("parts", Ty::Parts, "save-part: which parts"), a("folder", Ty::Str, "save-part: the part folder a new library part goes into, its path (\"Acme/2026-11\", made if missing; default: the one named like the job's week); a part already in the Library stays where it is"),
        a("copy", Ty::Bool, "save-job: save as a new job under the first free name (\"DEMO-7 (1)\") instead of over its record; the copy holds no stock of the original (default false)"),
    ] },
    // --- sheet and nesting ---
    Op { name: "set-sheet", group: "Nesting", kind: Kind::Write, help: "Change the sheet: size, margin, gap, material, thickness, gas, nesting level, machine. A new material or thickness with nested sheets needs nestings=keep or nestings=remove. A new gas keeps the nested sheets and picks the machine file's section and the technology record again (leads, speeds, times follow it); the reply says what changed.", args: &[
        a("w", Ty::Num, "Length X, mm"), a("h", Ty::Num, "Width Y, mm"), a("margin", Ty::Num, "Margin, mm"), a("gap", Ty::Num, "Gap between parts, mm"),
        a("material", Ty::Str, "Material name or number"), a("thickness", Ty::Num, "Thickness, mm"), a("gas", Ty::Str, "Gas the job is cut with: its name or number (1 Nitrogen, 2 Air, 3 Oxygen, 4 Argon, 5 Water)"), a("level", Ty::Int, "Nesting level 1-5"), c("machine", MACHINES, "Machine type"),
        c("nestings", &["keep", "remove"], "What happens to nested sheets on a material/thickness change"),
        a("machine_spacing", Ty::Bool, "true = take the part gap and margin from the machine file for the (new) material and thickness (see machine-tech); gap/margin given here still win"),
    ] },
    Op { name: "nest", group: "Nesting", kind: Kind::Write, help: "Nest all parts automatically (Do All; replaces the sheets, cut lines and hand-picked cut order). When the sheet stock has sheets or remnants for the job's material and thickness, they are taken in the stock's order (remnants first, then by size or priority; locked ones never), one sheet at a time, each filled by the best method; stock sheets below the minimum efficiency are dropped; with the window's options on, the best sheet combination and the best sheet format are searched within the maximum time (Sheet stock → Options; see stock mode=rules).", args: &[
        a("group", Ty::Bool, "Nest each part group in its own zone (default: the machine's CT1 setting)"), a("level", Ty::Int, "Nesting level 1-5 for this run (kept)"),
        c("stock", &["auto", "only", "off"], "auto (default): the window's Do All options (use the stock, then the job's own sheet when it runs out if that option is on); only: stock sheets only; off: every sheet the job's size"),
    ] },
    Op { name: "sheet-add", group: "Nesting", kind: Kind::Write, help: "Add an empty sheet to place parts on (Sheets → New): a stock sheet or remnant by reference, a size of its own, or the job's sheet.", args: &[
        a("ref", Ty::Str, "Stock reference (see stock mode=list)"), a("w", Ty::Num, "Or: length X, mm"), a("h", Ty::Num, "Width Y, mm"),
    ] },
    Op { name: "sheet-change", group: "Nesting", kind: Kind::Write, help: "Change which sheet a nested sheet is (Sheets → Change data): another stock sheet or remnant, a size, or back to the job's sheet. Refused when a placed part would end up outside it.", args: &[
        SHEET, a("ref", Ty::Str, "Stock reference"), a("w", Ty::Num, "Or: length X, mm"), a("h", Ty::Num, "Width Y, mm"), a("job_sheet", Ty::Bool, "true = back to the job's own sheet"),
    ] },
    Op { name: "sheet-delete", group: "Nesting", kind: Kind::Write, help: "Delete a nested sheet; its parts are no longer nested.", args: &[r("sheet", Ty::Int, "Sheet number, from 1")] },
    Op { name: "clear-sheets", group: "Nesting", kind: Kind::Write, help: "Take every part off the sheets.", args: &[] },
    Op { name: "compact", group: "Nesting", kind: Kind::Write, help: "Slide every part on a sheet left, then down, along the real contours, to leave one big remnant.", args: &[SHEET] },
    Op { name: "renest-sheet", group: "Nesting", kind: Kind::Write, help: "Nest one sheet again: only the parts already on it, on the same sheet (or remnant), by the job's automatic level; the other sheets stay as they are. Its cut lines and hand-picked cut order go (the automatic remnant line comes back if \"generate remnants after nesting\" is on). Refused, with the sheet unchanged, if they don't all fit back on it. Use nest to re-nest the whole job.", args: &[
        SHEET, a("group", Ty::Bool, "Nest each part group on the sheet in its own zone (default: the machine's CT1 setting, as nest)"),
    ] },
    Op { name: "place", group: "Nesting", kind: Kind::Write, help: "Place one copy of a part on a sheet: at x,y (lower-left of its box), or without x,y at the first free spot (lowest, then leftmost). Beyond the ordered quantity the order grows by one.", args: &[
        PART, a("sheet", Ty::Str, "Sheet number from 1, or \"new\" (default: sheet 1, a new one if there is none)"), a("x", Ty::Num, "X of the box's lower-left corner, mm"), a("y", Ty::Num, "Y, mm"),
        a("angle", Ty::Int, "0, 90, 180 or 270"), a("mirror", Ty::Bool, "Mirrored"),
    ] },
    Op { name: "move", group: "Nesting", kind: Kind::Write, help: "Move a placed part (item) or several together (items) to x,y (the lower-left of their box) or by dx,dy (refused when it would collide or leave the sheet).", args: &[SHEET, ITEM_ONE, ITEMS, a("x", Ty::Num, "New X"), a("y", Ty::Num, "New Y"), a("dx", Ty::Num, "Move by X"), a("dy", Ty::Num, "Move by Y")] },
    Op { name: "slide", group: "Nesting", kind: Kind::Write, help: "Slide placed parts left, right, up or down together: by a distance (stops early at the next part or the margin, never overlapping), or without `by` all the way until they meet the next part at the part gap (along the real contours) or the sheet margin — the original's Ctrl+arrow / Alt+arrow moves.", args: &[
        SHEET, ITEM_ONE, ITEMS,
        Arg { name: "dir", ty: Ty::Str, required: true, help: "Direction", choices: &["left", "right", "up", "down"] },
        a("by", Ty::Num, "Distance in mm (omit = to the end)"),
    ] },
    Op { name: "rotate", group: "Nesting", kind: Kind::Write, help: "Turn a placed part to its next allowed quarter turn (counter-clockwise), or with back=true to its previous one (clockwise), where there is room.", args: &[
        SHEET, ITEM, a("back", Ty::Bool, "Turn to the previous allowed turn (clockwise) instead of the next"),
    ] },
    Op { name: "turn", group: "Nesting", kind: Kind::Write, help: "Turn placed parts together about the centre of their box, by any angle (angle) or so the first of them sits at an angle (to) — the turn bar on the sheet; when the turned group would overlap a part or leave the sheet it moves to the nearest free spot. Angles other than 0/90/180/270° need parts that may turn 90°.", args: &[
        SHEET, ITEM_ONE, ITEMS,
        Arg { name: "angle", ty: Ty::Num, required: false, help: "Turn by this many degrees: positive counter-clockwise, negative clockwise (default 90; any angle, e.g. 15 or -32.5)", choices: &[] },
        a("to", Ty::Num, "Instead of angle: turn so the first of the parts sits at this angle (degrees, counter-clockwise from as drawn); the others turn with it"),
    ] },
    Op { name: "mirror", group: "Nesting", kind: Kind::Write, help: "Mirror placed parts together about the centre of their box: axis x = left↔right (default), y = top↔bottom; moved to the nearest free spot when needed.", args: &[
        SHEET, ITEM_ONE, ITEMS,
        Arg { name: "axis", ty: Ty::Str, required: false, help: "x = left↔right (default), y = top↔bottom", choices: &["x", "y"] },
    ] },
    Op { name: "copy", group: "Nesting", kind: Kind::Write, help: "Copy a placed part (item), or several keeping their layout (items), into the first free spot on its sheet. Copies use pieces still to place first; only a piece beyond the order makes it grow by one.", args: &[SHEET, ITEM_ONE, ITEMS] },
    Op { name: "repeat", group: "Nesting", kind: Kind::Write, help: "Repeat a pattern (Ctrl+C on the sheet): copies of placed parts (items), keeping their layout, into the first free spots on their sheet. Copies use the pieces still to place, so the order doesn't grow. Without times it makes as many whole copies as the pieces left allow. A copy that needs more pieces than are left is refused unless extras says what to do: true places it whole and the order grows by the pieces beyond it; false places only the pieces left.", args: &[
        SHEET, ITEM_ONE, ITEMS,
        a("times", Ty::Int, "How many copies (default: as many whole copies as the pieces left allow, plus one with the rest when extras is given)"),
        a("extras", Ty::Bool, "For a copy short of pieces: true = add extras to the order, false = place only the pieces left (default: refuse, saying how many are short)"),
    ] },
    Op { name: "unplace", group: "Nesting", kind: Kind::Write, help: "Take one placed part off its sheet.", args: &[SHEET, ITEM] },
    // --- technology ---
    Op { name: "leads", group: "Technology", kind: Kind::Write, help: "Lead-ins: auto (placed by contour class — round, rectangular or other part or hole: round ones at the keypad position's angle, rectangles at its corner or nearest side, other shapes at the nearest valid corner, else the middle of the nearest long-enough line), machine (lengths and position from the machine's CT1), or delete.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "auto, machine or delete", choices: &["auto", "machine", "delete"] }, PARTS,
        c("type", LEADS, "auto: lead type (default machine = the machine's lead tables per contour class and quality; line = inclined)"), a("position", Ty::Int, "auto: keypad position 1-9 (1 bottom-left, 9 top-right; default 1)"),
    ] },
    Op { name: "lead-at", group: "Technology", kind: Kind::Write, help: "Start a contour on one element of a part (see part-info for element numbers): by the third of a line nearest `at`, else at its middle.", args: &[PART, r("element", Ty::Int, "Element number from part-info"), a("at", Ty::Points, "Pick point \"x,y\" in part mm (optional)")] },
    Op { name: "lead-edit", group: "Technology", kind: Kind::Write, help: "Move one contour's lead by hand, as dragging its end in the Part view does: the lead-in's pierce and/or the lead-out's end, each as a length (mm, straight from where it meets the contour) and an angle from the contour into the scrap (lead-in: measured from the contour behind its start; lead-out: from the contour ahead; 90 = square off it). The contour keeps its start and lead type, and an arc lead keeps its arc into the contour; what you leave out stays as it is cut now (part-info lists each contour's lead_in / lead_out and the angles it can take). Every piece of the part type gets it. same=true (All the same contours) puts the lead on every hole of the same shape too (moved, not turned or mirrored; part-info's lead_same_as shows which share it), at the same place on each; without lengths or angles it shares the lead the contour has now. Sharing replaces their own moved leads and starts picked on an element; a hole given its own lead or start afterwards keeps that. mode=reset puts the contour's lead back (one it shares goes from all of them; with same=true the leads of every hole like it go too; without contour: every lead of the part).", args: &[
        PART, a("contour", Ty::Int, "Contour number from part-info"),
        a("in_length", Ty::Num, "Lead-in: mm from the start to the pierce"), a("in_angle", Ty::Num, "Lead-in: degrees from the contour behind the start, into the scrap"),
        a("out_length", Ty::Num, "Lead-out: mm from where it leaves the contour to its end"), a("out_angle", Ty::Num, "Lead-out: degrees from the contour ahead, into the scrap"),
        a("same", Ty::Bool, "Every hole of the same shape too (default false)"),
        Arg { name: "mode", ty: Ty::Str, required: false, help: "set (default) or reset", choices: &["set", "reset"] },
    ] },
    Op { name: "joints", group: "Technology", kind: Kind::Write, help: "Micro-joints: auto (one per the machine's maximum distance — 100 mm by default — on every contour, holes too, rounded up to an even number and centred on its line or arc, within the machine's fewest/most for the contour's size class) or delete.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "auto or delete", choices: &["auto", "delete"] }, PARTS,
        a("every", Ty::Num, "auto: one joint per this many mm of each contour (default: the machine's maximum distance for the part's quality)"),
    ] },
    Op { name: "machine-tech", group: "Technology", kind: Kind::Read, help: "Which material × thickness section of the machine's technology file applies to the job (or to another material/thickness/gas), picked by the job's material, thickness and gas, and the values it gives: lead and joint tables, joint gap, pierce and loop radius, kerf, part gap and margins (compare with the sheet's; take them with set-sheet machine_spacing=true), the cutting speeds, pierce times and time settings that time the job (from the machine's technology table and time file), and the machine's 12 cut qualities by name and colour (offered = listed in the window's pickers).", args: &[
        a("material", Ty::Str, "Look at this material instead of the job's (name or number)"), a("thickness", Ty::Num, "Look at this thickness instead of the job's, mm"),
        a("gas", Ty::Str, "Look at this gas instead of the job's (name or number: 1 Nitrogen, 2 Air, 3 Oxygen, 4 Argon, 5 Water)"),
    ] },
    Op { name: "compensation", group: "Machining", kind: Kind::Write, help: "Kerf compensation: how the beam is kept half a kerf on the scrap side of each cut — control (the program writes G41/G42 and the corrector number, the CNC offsets) and/or system (the program carries the already-offset path). Shows the machine's settings, the job's choice and, per contour of a sheet, its side, corrector and radius, plus the contours system compensation can't cut (holes no bigger than the kerf, slots narrower than it). control/system set the job's choice (on, off, or machine = the machine file's).", args: &[
        c("control", &["on", "off", "machine"], "Control compensation (G41/G42 + corrector) for this job"),
        c("system", &["on", "off", "machine"], "System compensation (offset path) for this job"),
        SHEET,
    ] },
    Op { name: "joint-at", group: "Technology", kind: Kind::Write, help: "A micro-joint in the middle of each listed element of a part (see part-info).", args: &[PART, r("elements", Ty::Str, "Element numbers, e.g. \"2,5\"")] },
    Op { name: "loops", group: "Technology", kind: Kind::Write, help: "Loops at sharp outer corners on or off.", args: &[r("on", Ty::Bool, "true = add loops, false = remove"), PARTS, a("size", Ty::Num, "Loop size, mm")] },
    Op { name: "cut-tech", group: "Technology", kind: Kind::Write, help: "Own technology with the defaults (own), back to the sheet's automatic technology (delete), or cut quality by contour size (ranges).", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "own, delete or ranges", choices: &["own", "delete", "ranges"] }, PARTS,
        a("ranges", Ty::Str, "ranges: up to 3 \"from-to:quality\" by equivalent radius in mm, quality 1-12, e.g. \"0-5:1,5-20:2\" (empty = off)"),
    ] },
    Op { name: "tech-set", group: "Technology", kind: Kind::Write, help: "Set technology values on parts directly (the technology settings windows).", args: &[
        PARTS, a("lead_in", Ty::Num, "Lead-in length, mm"), a("lead_out", Ty::Num, "Lead-out length, mm"), c("lead_type", LEADS, "Lead shape"), a("position", Ty::Int, "Keypad position 1-9"),
        a("corner_start", Ty::Bool, "Start at the nearest valid corner"), a("overlap", Ty::Num, "Cut past the start before the lead-out, mm"),
        a("micro_joints", Ty::Int, "Joints per outside contour"), a("joint_width", Ty::Num, "Joint gap, mm"), c("joint_type", JOINTS, "Joint type"),
        a("joint_lead", Ty::Num, "Joint lead length, mm"), a("joint_angle", Ty::Num, "Joint lead angle, degrees"), a("joint_every", Ty::Num, "Automatic joints: one per this many mm of each contour, holes too (0 = use micro_joints)"),
        a("joint_lead_out", Ty::Num, "Joint lead-out distance, mm (default: the same as joint_lead)"),
        a("loops", Ty::Bool, "Loops at sharp corners"), a("loop_size", Ty::Num, "Loop size, mm"), c("quality", QUALITIES, "Cut quality: the machine's quality 1-12 for every contour the part's size ranges don't set, or auto = the machine's size bands decide"),
    ] },
    Op { name: "text", group: "Technology", kind: Kind::Write, help: "Add a text along the bottom of parts (mark = engraved, special = filled in: #REF #QTY #MAT #THK #NUM, note = not machined), or delete all their texts.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "add or delete", choices: &["add", "delete"] }, PARTS,
        a("text", Ty::Str, "add: the text"), a("height", Ty::Num, "add: capital height, mm (default 8)"), c("kind", TEXTS, "add: text kind (default special)"),
    ] },
    // --- machining ---
    Op { name: "cut-order", group: "Machining", kind: Kind::Write, help: "Manual cutting order: pick the contour nearest x,y (cut first, from there); take whole parts (parts: placed parts in the order given, the parts a sweep along points passes in the order it passes them, or the parts wholly inside a box (with touching, every part it touches) in the automatic order; each cut whole in its own cut order, after the picks before it; parts the picks already finish are skipped); undo the last pick or the last parts command; or clear them (all automatic).", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "pick, parts, undo or clear", choices: &["pick", "parts", "undo", "clear"] }, SHEET,
        a("x", Ty::Num, "pick: X on the sheet, mm"), a("y", Ty::Num, "pick: Y, mm"), a("reverse", Ty::Bool, "pick: cut it the other way"), a("tol", Ty::Num, "pick, parts with points: how near, mm (default 5)"),
        a("items", Ty::Str, "parts: placed parts in the order to cut them, e.g. \"3,1,2\" (#n as in info/render; 4-2 is 4, 3, 2)"),
        a("points", Ty::Points, "parts: a sweep \"x,y;x,y;…\" in sheet mm: the parts it passes within tol of their contours, in the order passed"),
        a("box", Ty::Points, "parts: two opposite corners \"x,y;x,y\" in sheet mm: the parts wholly inside, in the sheet's automatic order"),
        a("touching", Ty::Bool, "parts with box: take every part the box touches, not only those wholly inside (as a box dragged to the left on the sheet)"),
    ] },
    Op { name: "remnant", group: "Machining", kind: Kind::Write, help: "Cut off the remnant: auto (the automatic remnant line in the shape set in the Sheet Remnants options), exact (stepped around the parts), or a line at x= or y=. A new remnant line replaces the sheet's old one.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "auto, exact or line", choices: &["auto", "exact", "line"] }, SHEET,
        a("round", Ty::Bool, "exact: round the corners (default true)"), a("x", Ty::Num, "line: vertical line at X"), a("y", Ty::Num, "line: horizontal line at Y"),
    ] },
    Op { name: "cut-line", group: "Machining", kind: Kind::Write, help: "Draw a cut path through the gaps (edge to opposite edge = remnant line); or delete one.", args: &[
        SHEET, a("points", Ty::Points, "Path \"x,y;x,y;…\" in sheet mm"), a("delete", Ty::Int, "Delete cut line number N instead"),
    ] },
    Op { name: "verify", group: "Machining", kind: Kind::Read, help: "Verify Machining: pierces in the scrap, holes before outsides, missing leads, parts outside the sheet, parts closer than the safe distance.", args: &[
        a("sheet", Ty::Str, "Sheet number or \"all\" (default all)"), a("safe", Ty::Num, "Safe distance, mm (default: the part gap; 0 = don't check)"),
    ] },
    Op { name: "program", group: "Machining", kind: Kind::Read, help: "The CNC program of a sheet (the active machine's post), returned and optionally written to a file.", args: &[SHEET, a("out", Ty::Path, "Also write it here")] },
    Op { name: "export", group: "Machining", kind: Kind::Read, help: "Write every sheet's program (G-code, or BLM LC5 sets) and a CSV parts report into <folder>\\<job>. Never sends anything to the machine.", args: &[a("dir", Ty::Path, "Folder (default Settings → export folder, else Documents\\Sheet Nesting)")] },
    // --- stock ---
    Op { name: "stock", group: "Stock", kind: Kind::Global, help: "The sheet stock (the Library folder's Sheets\\stock.json, shared with the window's Sheet stock): list sheets and remnants (each with who changed it last and when: changed_by, changed_at in this PC's local time), create a sheet, add or remove quantity (0 deletes), lock or unlock, edit, delete, or read the Do All / Sheet Remnants options (rules; set in the window). Every change is recorded as this PC's user, marked (agent).", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "What to do", choices: &["list", "create", "add", "remove", "block", "unblock", "edit", "delete", "rules"] },
        a("ref", Ty::Str, "Sheet reference (create: optional, else the next number; block/unblock: optional, else every listed sheet)"),
        a("material", Ty::Str, "list/create/block: material name or number"), a("thickness", Ty::Num, "list/create/block: thickness, mm"),
        a("w", Ty::Num, "create/edit: length X, mm"), a("h", Ty::Num, "create/edit: width Y, mm"), a("qty", Ty::Int, "create/add/remove: how many (default 1)"),
        a("priority", Ty::Int, "create/edit: 1 is used first when ordering by priority"), a("blocked", Ty::Bool, "create/edit: locked — never used by Do All"),
        a("user1", Ty::Str, "create/edit: user data 1"), a("user2", Ty::Str, "create/edit: user data 2"), a("user3", Ty::Str, "create/edit: user data 3"),
        a("min_w", Ty::Num, "list: shortest length"), a("max_w", Ty::Num, "list: longest length"), a("min_h", Ty::Num, "list: narrowest width"), a("max_h", Ty::Num, "list: widest width"),
        a("show_zero", Ty::Bool, "list: also items at quantity 0"), a("remnants", Ty::Str, "list: \"only\", \"none\" or \"all\" (default)"),
    ] },
    Op { name: "stock-book", group: "Stock", kind: Kind::Write, help: "Book the job against the stock, as the window does when it saves a job: take the sheets its nestings use out of stock and store the remnants its remnant lines leave (named <sheet>-<n>, or by refs=; below the minimum area they are scrap). A sheet whose remnant lines changed since gets its remnants made again, unless one of them has been used. Once booked, every later change keeps the stock in step. When a sheet the job is nested on has gone (another PC or job took it), nothing is booked and the reply says who took it and when: nest again to use what's in stock now. release gives the sheets back and takes its unused remnants out again. A job file that is a library job (opened from the Library, or saved into it) isn't booked or released here: its sheets are booked when someone saves it in the window, and a booked job file saved into the Library (library mode=save-job) hands its booking to the record (LB10).", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "book, release or status", choices: &["book", "release", "status"] },
        a("refs", Ty::Str, "book: references for the new remnants, comma-separated, in the order status lists them (default: numbered <sheet>-<n>)"),
    ] },
    // --- the shared Library ---
    Op { name: "library-check", group: "Library", kind: Kind::Global, help: "The two-PC Library check (run it on two or more programming PCs at once before sharing one Library folder): saves made-up DEMO-CHECK jobs and library parts, saves over them again and changes the sheet stock in a loop in a test folder, tagged with this PC's name, then checks that everything this run and every other run saved there is present and whole, and reports PASS or FAIL in plain words. Only an empty folder (made if missing) or one it made itself; a real Library is refused. `nestpro library-check <folder> --minutes 10 --text`.", args: &[
        r("folder", Ty::Path, "The test folder: empty or missing, or one the check made before (e.g. a folder next to the Library on the share). Never the Library itself"),
        a("minutes", Ty::Num, "How long to keep saving, minutes (default 1)"), a("rounds", Ty::Int, "Stop after this many rounds instead (each round: 2 job saves, 2 library part saves, 1 stock change)"),
        a("pc", Ty::Str, "Name to tag this run with (default: this PC's name)"),
    ] },
    Op { name: "job-folders", group: "Library", kind: Kind::Global, help: "The Library's job folders, the same on every PC that shares the Library (the Job Explorer's Library section): list the tree (each folder's path and #number, what it holds, and every saved job's folder), make a folder (new), change a folder's name (rename), delete an empty folder (delete), or move a saved job or a folder into another folder or to the top (move). Folder names are unique within their folder, without case (error 9); a folder never goes into itself or one of its own folders; a folder that still holds jobs or folders isn't deleted.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "What to do", choices: &["list", "new", "rename", "delete", "move"] },
        a("folder", Ty::Str, "The folder: its path from the top, names separated by / (\"Acme/2026-11\", without case), or #<number> from list. new: the folder to make it in (default: the top); rename, delete: the folder; move: the folder to move"),
        a("name", Ty::Str, "new: the new folder's name (default \"New folder\", then \"New folder (1)\"…); rename: its new name"),
        a("saved_job", Ty::Str, "move: the saved job to move: its reference (JOB000012) or record number from list, or its name when no other saved job has it"),
        a("to", Ty::Str, "move: where to: a folder's path or #<number>, or \"/\" for the top of the Library"),
    ] },
    Op { name: "part-folders", group: "Library", kind: Kind::Global, help: "The Library's part folders, apart from its job folders and the same on every PC that shares the Library (the Library window's Products tab): list the tree (each folder's path and #number, what it holds, and every library part's folder), make a folder (new), change a folder's name (rename), delete an empty folder (delete), or move a library part or a folder into another folder or to the top (move). A part saved into the Library for the first time goes into the part folder named like its job's folder (`library save-part`). Folder names are unique within their folder, without case (error 9); a folder never goes into itself or one of its own folders; a folder that still holds parts or folders isn't deleted.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "What to do", choices: &["list", "new", "rename", "delete", "move"] },
        a("folder", Ty::Str, "The folder: its path from the top, names separated by / (\"Acme/2026-11\", without case), or #<number> from list. new: the folder to make it in (default: the top); rename, delete: the folder; move: the folder to move"),
        a("name", Ty::Str, "new: the new folder's name (default \"New folder\", then \"New folder (1)\"…); rename: its new name"),
        a("library_part", Ty::Str, "move: the library part to move: its reference (SHP000012) or record number from list, or its name when no other library part has it"),
        a("to", Ty::Str, "move: where to: a folder's path or #<number>, or \"/\" for the top of the Library"),
    ] },
    // --- looking at it ---
    Op { name: "render", group: "View", kind: Kind::Read, help: "Draw a sheet (parts, numbers, cut path, leads, joints, cut lines) or one part (numbered elements, leads, joints) to PNG or SVG.", args: &[
        a("sheet", Ty::Int, "Sheet number (default 1)"), a("part", Ty::Parts, "Draw this part instead of a sheet"), a("out", Ty::Path, "File to write (.png or .svg; default next to the job)"),
        a("size", Ty::Int, "Image width in pixels (default 1400)"), a("path", Ty::Bool, "Show the cut path, leads and order (default true)"),
    ] },
    // --- reference ---
    Op { name: "catalog", group: "Reference", kind: Kind::Global, help: "Choices the other operations take: shapes, materials, thicknesses, nesting levels, machine types, lead/joint types, qualities, text kinds and fields, layer roles.", args: &[] },
    Op { name: "machines", group: "Reference", kind: Kind::Global, help: "The machines in Settings (read-only here): what each does (laser, plasma, punch…), output, post library, the sheet and hole order changed for it in Settings → Machines (order_changes), and the active machine's technology (CT1) values, with how it orders the parts on a sheet (sheet_order: start corner, columns or rows, zig-zag, zones) and a part's holes nobody ordered by hand (hole_order: none, by proximity or by direction with its start corner, rows or columns and tolerance, and cut small holes first with its size limits); for each, where the value comes from (Settings → Machines, the machine file or the default) and what the machine file alone gives.", args: &[] },
];

pub fn find(name: &str) -> Option<&'static Op> {
    let n = name.replace('_', "-");
    // The DXF-only names from before the other drawing formats.
    let n = match n.as_str() {
        "import-dxf" => "import-drawing".to_string(),
        "dxf-layers" => "drawing-layers".to_string(),
        _ => n,
    };
    OPS.iter().find(|o| o.name == n)
}

// ---------------------------------------------------------------------------
// Reading arguments.
// ---------------------------------------------------------------------------

pub struct Args<'a>(pub &'a Value);

impl Args<'_> {
    fn get(&self, k: &str) -> Option<&Value> {
        self.0.get(k).filter(|v| !v.is_null())
    }
    pub fn num(&self, k: &str) -> Result<Option<f64>, String> {
        match self.get(k) {
            None => Ok(None),
            Some(Value::Number(n)) => Ok(n.as_f64()),
            Some(Value::String(s)) => s.trim().parse().map(Some).map_err(|_| format!("{k}: '{s}' is not a number")),
            Some(v) => Err(format!("{k}: {v} is not a number")),
        }
    }
    pub fn int(&self, k: &str) -> Result<Option<i64>, String> {
        match self.num(k)? {
            None => Ok(None),
            Some(f) if f.fract() == 0.0 => Ok(Some(f as i64)),
            Some(f) => Err(format!("{k}: {f} is not a whole number")),
        }
    }
    pub fn bool(&self, k: &str) -> Result<Option<bool>, String> {
        match self.get(k) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(Value::String(s)) => match s.to_lowercase().as_str() {
                "true" | "yes" | "on" | "1" => Ok(Some(true)),
                "false" | "no" | "off" | "0" => Ok(Some(false)),
                _ => Err(format!("{k}: '{s}' is not true/false")),
            },
            Some(Value::Number(n)) => Ok(Some(n.as_f64() != Some(0.0))),
            Some(v) => Err(format!("{k}: {v} is not true/false")),
        }
    }
    pub fn str(&self, k: &str) -> Option<String> {
        match self.get(k)? {
            Value::String(s) => Some(s.clone()),
            v => Some(v.to_string()),
        }
    }
    fn need_str(&self, k: &str) -> Result<String, String> {
        self.str(k).ok_or_else(|| format!("'{k}' is needed"))
    }
    fn need_int(&self, k: &str) -> Result<i64, String> {
        self.int(k)?.ok_or_else(|| format!("'{k}' is needed"))
    }
    pub fn points(&self, k: &str) -> Result<Option<Vec<P>>, String> {
        let Some(v) = self.get(k) else { return Ok(None) };
        if let Value::Array(list) = v {
            // [x, y] or [[x, y], …]
            if list.len() == 2 && list.iter().all(|x| x.is_number()) {
                return Ok(Some(vec![[list[0].as_f64().unwrap_or(0.0), list[1].as_f64().unwrap_or(0.0)]]));
            }
            return list
                .iter()
                .map(|p| match p.as_array().map(|q| (q.first().and_then(Value::as_f64), q.get(1).and_then(Value::as_f64))) {
                    Some((Some(x), Some(y))) => Ok([x, y]),
                    _ => Err(format!("{k}: {p} is not a point [x, y]")),
                })
                .collect::<Result<Vec<P>, String>>()
                .map(Some);
        }
        let s = self.str(k).unwrap_or_default();
        s.split(';')
            .filter(|t| !t.trim().is_empty())
            .map(|t| {
                let n: Vec<f64> = t.split(',').map(|x| x.trim().parse::<f64>()).collect::<Result<_, _>>().map_err(|_| format!("{k}: '{t}' is not \"x,y\""))?;
                if n.len() == 2 { Ok([n[0], n[1]]) } else { Err(format!("{k}: '{t}' is not \"x,y\"")) }
            })
            .collect::<Result<Vec<P>, String>>()
            .map(Some)
    }
    fn point(&self, k: &str) -> Result<Option<P>, String> {
        Ok(self.points(k)?.and_then(|v| v.first().copied()))
    }
}

/// Look up a choice by its key (case and '-'/'_'/' ' don't matter).
fn pick<T: Copy>(k: &str, what: &str, list: &[(&str, T)]) -> Result<T, String> {
    let norm = |s: &str| s.to_lowercase().replace(['_', ' '], "-");
    let n = norm(k);
    list.iter().find(|(s, _)| norm(s) == n).map(|x| x.1).ok_or_else(|| format!("{what} '{k}' is not one of: {}", list.iter().map(|x| x.0).collect::<Vec<_>>().join(", ")))
}

pub fn shape_from(s: &str) -> Result<Shape, String> {
    pick(s, "shape", &[("rectangle", Shape::Rectangle), ("plate-hole", Shape::PlateHole), ("strip", Shape::Strip), ("l-bracket", Shape::LBracket), ("disc", Shape::Disc), ("flange", Shape::Flange), ("gusset", Shape::Triangle), ("triangle", Shape::Triangle), ("slotted-plate", Shape::SlottedPlate)])
}
pub fn machine_from(s: &str) -> Result<crate::machine::MachineKind, String> {
    use crate::machine::MachineKind::*;
    pick(s, "machine", &[("laser", Laser), ("plasma", Plasma), ("oxyfuel", Oxyfuel), ("waterjet", Waterjet), ("laserpunch", LaserPunch), ("punch", Punch)])
}
fn lead_from(s: &str) -> Result<LeadType, String> {
    pick(
        s,
        "lead type",
        &[("machine", LeadType::Machine), ("perpendicular", LeadType::Perpendicular), ("line", LeadType::Line), ("arc", LeadType::Arc), ("line-arc", LeadType::LineArc), ("corner", LeadType::Corner)],
    )
}
fn lead_key(t: LeadType) -> &'static str {
    match t {
        LeadType::Machine => "machine",
        LeadType::Perpendicular => "perpendicular",
        LeadType::Line => "line",
        LeadType::LineArc => "line-arc",
        LeadType::Arc => "arc",
        LeadType::Corner => "corner",
    }
}
fn joint_from(s: &str) -> Result<JointType, String> {
    pick(s, "joint type", &[("gap", JointType::Gap), ("leads", JointType::Leads), ("leads-inclined", JointType::LeadsInclined), ("corner", JointType::Corner)])
}
/// One of the machine's cut qualities, 1–12.
fn numbered_quality(s: &str) -> Option<CutQuality> {
    s.trim().parse::<usize>().ok().and_then(CutQuality::new)
}
fn old_name(s: &str) -> bool {
    matches!(s.trim().to_lowercase().as_str(), "rough" | "normal" | "fine")
}
/// A part's quality: auto (None, the machine's size bands decide) or 1–12 (CQ1, CQ2).
fn quality_from(s: &str) -> Result<Option<CutQuality>, String> {
    if s.trim().eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    let old = if old_name(s) { "; the old rough / normal / fine are 3 / auto / 1" } else { "" };
    numbered_quality(s).map(Some).ok_or_else(|| format!("quality '{s}': give one of the machine's cut qualities, 1 to 12 (machine-tech lists their names), or auto to let its size bands decide{old}"))
}
/// A size-range row's quality: 1–12 (CQ2).
fn range_quality_from(s: &str) -> Result<CutQuality, String> {
    let old = if old_name(s) { "; the old rough / normal / fine are 3 / 2 / 1" } else { "" };
    numbered_quality(s).ok_or_else(|| format!("ranges: quality '{}': a row takes one of the machine's cut qualities, 1 to 12 (machine-tech lists their names){old}", s.trim()))
}
fn text_kind_from(s: &str) -> Result<TextKind, String> {
    pick(s, "text kind", &[("mark", TextKind::Mark), ("special", TextKind::Special), ("note", TextKind::Note)])
}
fn layer_role_from(s: &str) -> Result<dxf::LayerRole, String> {
    pick(s, "layer role", &[("cut", dxf::LayerRole::Cut), ("mark", dxf::LayerRole::Mark), ("ignore", dxf::LayerRole::Ignore)])
}

/// Layer roles as an agent writes them ("LAYER=cut|mark|ignore,…") over
/// `roles`, the drawing's layers with the roles they start with.
fn set_layer_roles(roles: &mut BTreeMap<String, dxf::LayerRole>, spec: &str) -> Result<(), String> {
    for kv in spec.split(',').filter(|s| !s.trim().is_empty()) {
        let (l, role) = kv.split_once('=').ok_or_else(|| format!("layers: '{kv}' is not LAYER=role"))?;
        let l = l.trim();
        let k = roles.keys().find(|x| x.eq_ignore_ascii_case(l)).cloned().ok_or_else(|| format!("layers: no layer '{l}' in the drawing(s) ({})", roles.keys().cloned().collect::<Vec<_>>().join(", ")))?;
        roles.insert(k, layer_role_from(role.trim())?);
    }
    Ok(())
}

/// Contour choices as an agent writes them (IM12), for a part of `n`
/// contours: "7=mark; 1=leave-out; 2=quality 1 same", each the contours
/// (as contour-tech takes them) and what they become.
fn contour_choices_from(s: &str, n: usize) -> Result<Vec<imports::ContourChoice>, String> {
    let how = "give a state (cut, mark, no-cut, bend-line, leave-out), quality 1-12 or auto, and/or same, e.g. contours=\"7=mark; 1=leave-out; 2=quality 1 same\"";
    let mut out = vec![];
    for entry in s.split([';', '\n']).map(str::trim).filter(|e| !e.is_empty()) {
        let (nums, what) = entry.split_once('=').ok_or_else(|| format!("contours: '{entry}' is not CONTOURS=WHAT: {how}."))?;
        let picks = contours::parse_numbers(nums, n)?;
        let (mut state, mut quality, mut same) = (None, None, false);
        let words: Vec<String> = what.replace('=', " ").split_whitespace().map(str::to_lowercase).collect();
        let mut w = words.iter();
        while let Some(word) = w.next() {
            match word.as_str() {
                "same" => same = true,
                "quality" => {
                    let q = w.next().ok_or_else(|| format!("contours: '{entry}': quality needs 1-12 or auto."))?;
                    quality = Some(contour_quality_from(q)?);
                }
                word => state = Some(contour_state_from(word).map_err(|_| format!("contours: '{word}' in '{entry}': {how}."))?),
            }
        }
        if state.is_none() && quality.is_none() {
            return Err(format!("contours: '{entry}' says nothing to do: {how}."));
        }
        out.push(imports::ContourChoice { contours: picks, paint: contours::Paint { state, quality }, same });
    }
    Ok(out)
}

/// A contour's quality set by hand: 1–12, or auto (None) to give it back to
/// the part's size ranges and quality and the machine's size bands (CQ6).
fn contour_quality_from(s: &str) -> Result<Option<CutQuality>, String> {
    if s.trim().eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    numbered_quality(s).map(Some).ok_or_else(|| {
        format!("quality '{}': the machine has cut qualities 1 to 12 (part-info shows the one each contour gets). Give one of them, or auto to let the part's size ranges and quality and the machine's size bands decide.", s.trim())
    })
}

fn contour_state_from(s: &str) -> Result<dxf::ContourState, String> {
    use dxf::ContourState::*;
    let words: Vec<(&str, dxf::ContourState)> = dxf::ContourState::ALL.iter().map(|st| (st.key(), *st)).chain([("nocut", NoCut), ("bend", BendLine), ("ignore", LeaveOut)]).collect();
    pick(s, "contour state", &words)
}

/// Material by number (from 0 as in catalog), name, or a Settings alias.
pub fn material_from(s: &str, cfg: &AppSettings) -> Result<usize, String> {
    if let Ok(n) = s.trim().parse::<usize>() {
        return if n < cnc::MATERIALS.len() { Ok(n) } else { Err(format!("material {n}: there are {} (0-{})", cnc::MATERIALS.len(), cnc::MATERIALS.len() - 1)) };
    }
    let l = s.trim().to_lowercase();
    cnc::MATERIALS
        .iter()
        .position(|m| m.to_lowercase() == l || m.to_lowercase().starts_with(&l))
        .or_else(|| cfg.material_for(s))
        .ok_or_else(|| format!("material '{s}' is not one of: {}", cnc::MATERIALS.join(", ")))
}

fn key(s: &str) -> String {
    s.to_lowercase()
}

/// Parts chosen by a selector (numbers from 1, names, "all").
pub fn parts_sel(parts: &[Part], v: Option<&Value>, default_all: bool) -> Result<Vec<usize>, String> {
    let all: Vec<usize> = (0..parts.len()).collect();
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return if default_all { Ok(all) } else { Err("say which part ('part')".into()) };
    };
    let one = |t: &Value| -> Result<Vec<usize>, String> {
        match t {
            Value::Number(n) => {
                let k = n.as_u64().unwrap_or(0) as usize;
                if k >= 1 && k <= parts.len() { Ok(vec![k - 1]) } else { Err(format!("there is no part {k} (the job has {})", parts.len())) }
            }
            Value::String(s) => {
                let s = s.trim();
                if s.eq_ignore_ascii_case("all") {
                    return Ok(all.clone());
                }
                if let Ok(k) = s.trim_start_matches('#').parse::<usize>() {
                    return if k >= 1 && k <= parts.len() { Ok(vec![k - 1]) } else { Err(format!("there is no part {k} (the job has {})", parts.len())) };
                }
                if let Some(i) = parts.iter().position(|p| p.name == s).or_else(|| parts.iter().position(|p| key(&p.name) == key(s))) {
                    return Ok(vec![i]);
                }
                Err(format!("no part is called '{s}'"))
            }
            v => Err(format!("{v} doesn't name a part")),
        }
    };
    let mut out = vec![];
    match v {
        Value::Array(list) => {
            for t in list {
                out.extend(one(t)?);
            }
        }
        Value::String(s) if s.contains(',') && !s.eq_ignore_ascii_case("all") && parts.iter().all(|p| p.name != *s) => {
            for t in s.split(',') {
                out.extend(one(&Value::String(t.to_string()))?);
            }
        }
        v => out = one(v)?,
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err("no parts chosen".into());
    }
    Ok(out)
}

fn one_part(job: &JobFile, args: &Args) -> Result<usize, String> {
    let v = parts_sel(&job.parts, args.get("part"), false)?;
    if v.len() != 1 {
        return Err("name one part".into());
    }
    Ok(v[0])
}

fn sheet_idx(job: &JobFile, args: &Args) -> Result<usize, String> {
    if job.sheets.is_empty() {
        return Err("No sheets are nested yet — run nest first.".into());
    }
    let k = args.int("sheet")?.unwrap_or(1);
    if k < 1 || k as usize > job.sheets.len() {
        return Err(format!("there is no sheet {k} (the job has {})", job.sheets.len()));
    }
    Ok(k as usize - 1)
}

/// "#2" or "#1, #3 and #5": placed parts by their numbers on the sheet.
fn names(sel: &[usize]) -> String {
    let v: Vec<String> = sel.iter().map(|i| format!("#{}", i + 1)).collect();
    match v.len() {
        0 => "nothing".into(),
        1 => v[0].clone(),
        n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]),
    }
}

/// The placed parts named by `items` ("1,3,5", "2-4") or `item`.
fn items_idx(job: &JobFile, s: usize, args: &Args) -> Result<Vec<usize>, String> {
    match args.str("items") {
        Some(list) => arrange::parse_items(&list, job.sheets[s].len()),
        None if args.str("item").is_some() => Ok(vec![item_idx(job, s, args)?]),
        None => Err("Name the placed parts: item=N or items=\"1,3,5\".".into()),
    }
}

fn item_idx(job: &JobFile, s: usize, args: &Args) -> Result<usize, String> {
    let k = args.need_int("item")?;
    let n = job.sheets[s].len();
    if k < 1 || k as usize > n {
        return Err(format!("sheet {} has {n} placed part(s); there is no #{k}", s + 1));
    }
    Ok(k as usize - 1)
}

// ---------------------------------------------------------------------------
// Reports.
// ---------------------------------------------------------------------------

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

pub fn tech_json(t: &Option<PartTech>) -> Value {
    match t {
        None => json!("automatic (the sheet's default lead-in, no micro-joints)"),
        Some(t) => json!({
            "lead_in": t.lead_in, "lead_out": t.lead_out, "lead_type": lead_key(t.lead_type), "position": t.position,
            "corner_start": t.corner_start, "overlap": t.overlap, "micro_joints": t.micro_joints, "joint_width": t.joint_width,
            "joint_type": format!("{:?}", t.joint_type).to_lowercase(), "joint_every": t.joint_every, "joint_lead": t.joint_lead, "joint_lead_out": t.joint_lead_out.unwrap_or(t.joint_lead), "joint_angle": t.joint_angle,
            "loops": t.loops, "loop_size": t.loop_size, "quality": t.quality.map_or(json!("auto"), |q| json!(q.number())),
            "leads_on_elements": t.starts.iter().flatten().count(), "leads_moved_by_hand": t.lead_edits.iter().flatten().count(), "joints_on_elements": t.joint_picks.iter().flatten().count(),
            "cut_ranges": t.cut_ranges.iter().filter(|c| c.on).map(|c| format!("{}-{}:{}", c.min_r, c.max_r, c.quality.number())).collect::<Vec<_>>(),
        }),
    }
}

/// The job's sheet settings; its gas (GS11) as the job's own, or the
/// machine's as saved (`base`) for a job without one.
fn spec_json(spec: &SheetSpec, base: &crate::blm::BlmSettings) -> Value {
    let gas = job_gas(spec, base);
    let mut v = json!({
        "w": spec.w, "h": spec.h, "margin": spec.margin, "gap": spec.gap,
        "material": cnc::MATERIALS.get(spec.material).copied().unwrap_or("?"), "material_no": spec.material, "thickness": spec.thickness,
        "gas": gas.name, "gas_no": gas.number, "gas_from": if gas.own { "job" } else { "machine" },
        "level": spec.nest_level().name, "machine": spec.machine.label(),
    });
    if gas.listed().is_none() {
        v["gas_note"] = json!(format!("The machine's gas as saved in Settings → Machines, written as saved: {} named \"{}\", which isn't a pair the LC5 list writes ({}). Pick a gas with set-sheet gas=.", gas.number, gas.program_name, crate::gas::program_words()));
    }
    v
}

/// The gas as a job's machine-tech shows it.
fn gas_json(g: &crate::gas::JobGas) -> Value {
    json!({"number": g.number, "name": g.name, "program_name": g.program_name, "label": g.label(), "from": if g.own { "job" } else { "machine" }})
}

/// What `nest` will nest on, before nesting (the window's "Nest on" line):
/// from the stock (with what it holds for the job) or the job's sheet size, and why.
fn nest_on_json(job: &JobFile, ctx: &Ctx) -> Value {
    let grouped = crate::ct1::active().group_parts && nest::part_groups(&job.parts).len() > 1;
    let on = stockflow::nest_on(&job.spec, &ctx.stock_use(job), grouped);
    let sizes: Vec<Value> = on.sizes.iter().map(|s| json!({"w": r1(s.length), "h": r1(s.width), "qty": s.qty})).collect();
    json!({
        "mode": on.key(), "from_stock": on.from_stock(), "sheets": on.sheets, "remnants": on.remnants, "stock_sizes": sizes,
        "line": on.line(&format!("{:.0} × {:.0} mm", job.spec.w, job.spec.h)), "why": on.explain(),
    })
}

fn placed_json(job: &JobFile, s: usize) -> Value {
    json!(job.sheets[s].iter().enumerate().map(|(i, p)| json!({
        "item": i + 1, "part": p.part + 1, "name": job.parts[p.part].name, "x": r1(p.x), "y": r1(p.y), "w": r1(p.w), "h": r1(p.h), "angle": (p.angle() * 100.0).round() / 100.0, "mirror": p.mirror,
    })).collect::<Vec<_>>())
}

pub fn info(job: &JobFile, ctx: &Ctx, detail_sheet: Option<usize>) -> Value {
    let plans = job.plans();
    let parts: Vec<Value> = job
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let n = placed_count(&job.sheets, i);
            json!({
                "part": i + 1, "name": p.name, "shape": p.shape.label(), "w": r1(p.bbox().0), "h": r1(p.bbox().1), "qty": p.qty, "nested": n,
                "missing": (p.qty as usize).saturating_sub(n), "rotate": p.rotate, "mirror": p.rots.mirrored.iter().any(|m| *m), "group": p.group,
                "texts": p.texts.len(), "technology": tech_json(&p.tech),
            })
        })
        .collect();
    let mut tot = (0.0, 0.0, 0usize, 0.0);
    let sheets: Vec<Value> = job
        .sheets
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let f = job.format(k);
            let (u, ur, cx) = sheet_usage(&job.parts, s, &job.spec, &f);
            let pl = &plans[k];
            tot.0 += pl.cut_len;
            tot.1 += pl.rapid_len;
            tot.2 += pl.pierces;
            tot.3 += pl.seconds;
            let mut v = json!({
                "sheet": k + 1, "parts": s.len(), "usage_pct": r1(u), "usage_to_remnant_pct": r1(ur), "used_up_to_x": r1(cx),
                "cut_len_mm": r1(pl.cut_len), "mark_len_mm": r1(pl.mark_len), "rapid_len_mm": r1(pl.rapid_len), "pierces": pl.pierces, "time_s": r1(pl.seconds),
                "times_s": times_json(&pl.times),
                "manual_cuts": job.manual_cuts.get(k).map_or(0, |v| v.len()), "cut_lines": job.cuts(k).len(),
                "remnant": remnant_info(&job.parts, s, &f, job.cuts(k), crate::kerf::kerf_mm(&job.spec_at(k))).map(|(w, h, a)| json!({"w": r1(w), "h": r1(h), "area_m2": (a / 1e4).round() / 100.0})),
                "size": [r1(f.w), r1(f.h)], "stock_ref": (!f.stock_ref.is_empty()).then(|| f.stock_ref.clone()), "is_remnant": f.remnant, "area_m2": (f.area_mm2() / 1e4).round() / 100.0,
                "remnants_stored": f.made,
                "program": if ctx.blm.on { format!("{}.zip", crate::blm::nest_name(&job.name, k + 1)) } else { ctx.post.file_name(&job.name, k + 1) },
            });
            if detail_sheet == Some(k) {
                v["placed"] = placed_json(job, k);
            }
            v
        })
        .collect();
    let need: u32 = job.parts.iter().map(|p| p.qty).sum();
    let placed: usize = job.sheets.iter().map(|s| s.len()).sum();
    let mut warnings = vec![];
    if job.stale {
        warnings.push("Parts or the sheet changed since the last nest — nest again (or check the sheets).".to_string());
    }
    for (i, p) in job.parts.iter().enumerate() {
        let n = placed_count(&job.sheets, i);
        if (n as u32) < p.qty && !job.sheets.is_empty() {
            let (w, h) = p.bbox();
            let fits = |a: f64, b: f64| a <= job.spec.w - 2.0 * job.spec.margin && b <= job.spec.h - 2.0 * job.spec.margin;
            let why = if !fits(w, h) && !(p.rotate && fits(h, w)) { " — larger than the sheet" } else { "" };
            warnings.push(format!("{}: {} of {} nested{why}", p.name, n, p.qty));
        }
    }
    json!({
        "job": job.name, "week": job.week, "changed_since_nest": job.stale, "sheet": spec_json(&job.spec, &ctx.blm),
        "machine": { "name": ctx.cfg.machine().name, "output": if ctx.blm.on { "BLM LC5 (Siemens 840D set)" } else { "ISO G-code" } },
        "parts": parts, "sheets": sheets,
        "totals": { "ordered": need, "nested": placed, "sheets": job.sheets.len(), "cut_len_mm": r1(tot.0), "rapid_len_mm": r1(tot.1), "pierces": tot.2, "time_s": r1(tot.3), "method": job.method },
        "warnings": warnings,
        "nest_on": nest_on_json(job, ctx),
        "stock": {
            "booked": job.booked,
            "held": job.held,
            "remnants_stored": job.formats.iter().flat_map(|f| f.made.iter().cloned()).collect::<Vec<_>>(),
        },
    })
}

fn part_info(job: &JobFile, i: usize) -> Value {
    let p = &job.parts[i];
    let g = p.geometry();
    let elems = nest::elements_of(&g);
    let (w, h) = p.bbox();
    let plan = cnc::plan_part(&job.parts, i, &job.spec);
    // Every contour with its state (CS1); elements name the contour they're on
    // by the same numbers.
    let list = contours::list(p);
    let numbers = dxf::cut_order(&list);
    let (tech, mt) = (p.tech.unwrap_or_default(), crate::ct1::for_spec(&job.spec));
    // CO3: each contour's place in the cut order, and whether it was set by
    // hand; the holes not set by hand in the machine's hole order where the
    // part is drawn (CO14).
    let order = cutorder::order(p);
    let contours: Vec<Value> = list
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let shape = if c.closed { crate::machine::recognise(&c.pts).map(|s| s.label()) } else { None };
            let mut v = json!({"contour": k + 1, "kind": contours::kind(&list, k), "closed": c.closed, "state": c.state.key(), "length_mm": r1(c.length()), "standard_shape": shape});
            if let Some(place) = order.iter().position(|o| o.0 == k) {
                v["cut_order"] = json!(place + 1);
                v["cut_order_by_hand"] = json!(order[place].1);
            }
            // CQ1, CQ6: the cut quality each cut contour gets on this job's
            // machine, and why ("by hand" when set on the contour).
            if c.closed && c.state == dxf::ContourState::Cut {
                let (q, from) = cnc::contour_quality(&tech, &mt, &c.pts, c.quality);
                v["quality"] = json!(q.number());
                v["quality_name"] = json!(mt.qualities.label(q));
                v["quality_from"] = json!(from.label());
            }
            // Its leads as cut (LE1): what lead-edit changes.
            if let Some(pc) = numbers.iter().position(|&x| x == k).and_then(|ci| plan.contours.iter().find(|pc| pc.contour == ci)) {
                let e = lead_ends(pc);
                let end = |x: Option<(f64, f64)>, ang: (f64, f64)| x.map_or(Value::Null, |(l, a)| json!({"length_mm": r1(l), "angle_deg": r1(a) + 0.0, "angles_deg": [r1(ang.0), r1(ang.1)]}));
                v["lead_in"] = end(e.lead_in, e.in_angles);
                v["lead_out"] = end(e.lead_out, e.out_angles);
                v["lead_by_hand"] = json!(e.hand);
                // LE8: a lead shared by the holes of its shape, by the contour it is set on.
                if let Some((_, from)) = hand_lead(p, pc.contour).filter(|(h, _)| h.same) {
                    v["lead_same_as"] = json!(contour_number(p, from));
                }
            }
            v
        })
        .collect();
    json!({
        "part": i + 1, "name": p.name, "shape": p.shape.label(), "w": r1(w), "h": r1(h), "qty": p.qty, "group": p.group,
        "area_mm2": r1(p.net_area()), "cut_len_mm": r1(p.cut_length()),
        "rotations": { "turn_90": p.rotate, "plain": p.rots.plain, "mirrored": p.rots.mirrored },
        "contours": contours,
        "cut_order": {
            "contours": order.iter().map(|o| o.0 + 1).collect::<Vec<_>>(),
            "by_hand": order.iter().filter(|o| o.1).map(|o| o.0 + 1).collect::<Vec<_>>(),
            "holes": hole_order_json(&mt.hole_order),
            "note": "The holes not set by hand follow the machine's hole order (machines shows it), here as the part is drawn with the head starting at its lower left corner; on a sheet each copy's holes are ordered again where it sits (turned, mirrored) from where the head is (program shows the order the machine gets).",
        },
        "elements": elems.iter().enumerate().map(|(k, e)| json!({
            "element": k + 1, "contour": numbers.get(e.contour).map_or(e.contour, |n| *n) + 1, "kind": e.kind.label().to_lowercase(), "length_mm": r1(e.length), "mid": [r1(e.midpoint()[0]), r1(e.midpoint()[1])],
        })).collect::<Vec<_>>(),
        "texts": p.texts.iter().map(|t| json!({"text": t.text, "kind": format!("{:?}", t.kind).to_lowercase(), "height": r1(t.height), "at": [r1(t.at[0]), r1(t.at[1])]})).collect::<Vec<_>>(),
        "technology": tech_json(&p.tech),
        "verify_part": crate::drawing::verify_part(p),
        "verify_machining": crate::drawing::verify_machining(&plan),
        "machining": { "cut_len_mm": r1(plan.cut_len), "pierces": plan.pierces, "time_s": r1(plan.seconds), "times_s": times_json(&plan.times), "speeds": plan.timing.speeds.label() },
    })
}

/// A sheet's time by kind (CTM15), seconds.
fn times_json(t: &cnc::TimeSplit) -> Value {
    json!({"cut": r1(t.cut), "mark": r1(t.mark), "rapid": r1(t.rapid), "pierce": r1(t.pierce), "head": r1(t.head)})
}

fn catalog() -> Value {
    json!({
        "shapes": SHAPES,
        "materials": cnc::MATERIALS.iter().enumerate().map(|(i, m)| json!({"material": i, "name": m})).collect::<Vec<_>>(),
        "thicknesses_mm": cnc::THICKNESSES,
        "levels": nest::LEVELS.iter().enumerate().map(|(i, l)| json!({"level": i + 1, "name": l.name, "methods": l.methods})).collect::<Vec<_>>(),
        "machines": crate::machine::MachineKind::ALL.iter().map(|m| json!({"key": format!("{m:?}").to_lowercase(), "name": m.label(), "does": m.describe()})).collect::<Vec<_>>(),
        "lead_types": LEADS, "joint_types": JOINTS, "qualities": QUALITIES, "text_kinds": TEXTS,
        "text_fields": nest::TEXT_PARAMS.iter().map(|(k, v)| json!({"field": k, "is": v})).collect::<Vec<_>>(),
        "layer_roles": ["cut", "mark", "ignore"],
        "contour_states": CONTOUR_STATES,
        "keypad_positions": "1 bottom-left, 2 bottom, 3 bottom-right, 4 left, 5 centre, 6 right, 7 top-left, 8 top, 9 top-right",
        "numbering": "parts, sheets, placed items (#) and elements are numbered from 1; materials from 0",
    })
}

/// The machine's values for a sheet's material, thickness and gas (MTL1–MTL9,
/// GS13): which section applies and what it gives, beside the job's own spacing.
fn machine_tech_report(spec: &SheetSpec, job: &SheetSpec, base: &crate::blm::BlmSettings) -> (String, Value) {
    let t = crate::ct1::for_spec(spec);
    let gas = job_gas(spec, base);
    let q1 = t.leads.quality(1);
    let opt = |v: Option<f64>| v.map_or(Value::Null, |x| json!(r1(x * 1000.0) / 1000.0));
    let section = t.applied().map(|s| {
        json!({
            "number": s.no, "material_key": s.material, "material": crate::ct1::material_label(&s.material),
            "thickness_from": s.t_min, "thickness_to": s.t_max, "gas": s.gas, "default": s.default, "hole_size_bands": s.hole_bands.len(),
        })
    });
    let mat = cnc::MATERIALS.get(spec.material).copied().unwrap_or("?");
    let mut msg = format!("{mat} {} mm, {}: {}.", spec.thickness, gas.label(), t.table_label());
    match (t.part_gap, t.sheet_margin()) {
        (None, None) => msg.push_str(" The machine file gives no part gap or margins."),
        (g, m) => msg.push_str(&format!(
            " Machine part gap {}, margin {} (the job: gap {} mm, margin {} mm; set-sheet machine_spacing=true takes the machine's).",
            g.map_or("—".to_string(), |g| format!("{g} mm")),
            m.map_or("—".to_string(), |m| format!("{m} mm")),
            job.gap,
            job.margin
        )),
    }
    let data = json!({
        "material": mat, "material_key": t.picked_for.as_ref().map(|p| p.0.clone()), "thickness": spec.thickness, "gas": gas_json(&gas),
        "table": t.table_label(), "section": section, "sections_in_file": t.file.sections.len(),
        "default_rule": t.default_rule,
        "source": t.source.as_ref().map_or("built-in values".to_string(), |p| p.display().to_string()),
        "leads": {
            "lead_in": t.lead_in, "lead_out": t.lead_out, "lead_angle": t.lead_angle, "hole_lead_in": t.hole_lead_in, "hole_lead_out": t.hole_lead_out,
            "overlap": t.overlap, "position": t.zone, "min_lead": q1.min_lead,
            "by_class": crate::leads::CLASS_NAMES.iter().zip(q1.recs.iter()).map(|(n, r)| json!({"class": n, "type": r.kind, "in": r.d_in, "out": r.d_out})).collect::<Vec<_>>(),
        },
        "joints": { "gap": t.joint_gap, "lead": t.joint_lead, "lead_out": t.joint_lead_out, "angle": t.joint_angle, "every": t.joint_every, "fewest": t.joint_min, "most": t.joint_max },
        "loop_radius": t.loop_radius, "pierce_radius": opt(t.pierce_radius),
        "cutter_radius": opt(t.cutter_radius), "kerf": opt(t.cutter_radius.map(|r| 2.0 * r)), "cut_speed": opt(t.cut_speed),
        "speeds": speeds_json(spec, &t),
        "qualities": qualities_json(&t.qualities),
        "compensation": crate::kerf::status_line(spec),
        "spacing": {
            "machine_gap": opt(t.part_gap), "machine_margins": t.margins.map(|m| json!({"bottom": m[0], "top": m[1], "left": m[2], "right": m[3]})),
            "machine_margin": opt(t.sheet_margin()), "remnant_margin": opt(t.remnant_margin), "y_gap": opt(t.y_gap),
            "job_gap": job.gap, "job_margin": job.margin,
        },
    });
    (msg, data)
}

/// The machine's 12 cut qualities as screens show them: number, name and
/// whether the pickers offer it (CQ4), colour (CQ5).
fn qualities_json(q: &crate::qualities::CutQualities) -> Value {
    json!(nest::CutQuality::all().map(|c| json!({"quality": c.number(), "name": q.label(c), "colour": q.colour_hex(c), "offered": q.offered(c)})).collect::<Vec<_>>())
}

/// Where a sheet's speeds and times come from (TT1–TT10, CTM8–CTM16): the
/// machine's table record, speeds by quality, pierce times, the time file.
fn speeds_json(spec: &crate::nest::SheetSpec, t: &crate::ct1::MachineTech) -> Value {
    let tm = cnc::Timing::for_spec(spec, t);
    let sp = &tm.speeds;
    let f = &t.speed_files;
    let name = |p: &Option<std::path::PathBuf>| p.as_ref().map(|p| p.display().to_string());
    json!({
        "source": sp.label(), "time_settings": tm.settings.label(),
        "table_file": f.table.as_ref().and_then(|t| name(&t.source)), "time_file": name(&f.time.source), "gas": crate::techtable::Keys::for_spec(spec, f.gas).gas,
        "cut_mm_min_by_quality": (1..=12).map(|q| r1(sp.cut_for(q))).collect::<Vec<_>>(),
        "pierce_s_by_quality": (1..=12).map(|q| (sp.pierce_for(q) * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
        "pierce_mode_by_quality": (1..=12).map(|q| sp.pierce_mode(q)).collect::<Vec<_>>(),
        "mark_mm_min": r1(sp.mark), "rapid_mm_min": r1(sp.rapid),
    })
}

/// The job's kerf compensation (KC1–KC6, KC18): the machine's settings, the
/// job's choice, and per contour of one sheet what it gets.
fn compensation_report(job: &JobFile, a: &Args) -> Result<(String, Value), String> {
    let mt = crate::ct1::for_spec(&job.spec);
    let s = &mt.comp;
    let line = crate::kerf::status_line(&job.spec);
    let choice = |v: Option<bool>| v.map_or(json!("machine"), |b| json!(if b { "on" } else { "off" }));
    let mut data = json!({
        "status": line,
        "kerf": 2.0 * crate::kerf::cutter_radius(&mt),
        "cutter_radius_from": if mt.cutter_radius.is_some() { mt.table_label() } else { format!("no cutter radius in the machine file: {} mm kerf assumed", cutoff::KERF_MM) },
        "job": { "control": choice(job.spec.kerf.control), "system": choice(job.spec.kerf.system) },
        "machine": {
            "control": s.control, "system": s.system, "hole_corrector": s.hole_corrector, "outside_corrector": s.outside_corrector,
            "by_quality": s.by_quality, "quality_mode": s.modes[0], "corners": s.corners().label(), "lead_mode": s.lead_mode, "joint_mode": s.joint_mode,
            "common_cut": s.common_cut,
        },
        "notes": [
            "Side = the scrap side from each contour's cutting direction: outside counter-clockwise right (G42), clockwise left (G41); a hole the other way round. Mirrored parts and reversed contours flip it.",
            "Control words: written on the first lead-in move when the needed side or corrector changes, G40 when a contour, marking or cut line needs none and at the end.",
            "The app has no common cut: the machine's common-cut setting is read only.",
        ],
    });
    let mut msg = format!("{line}.");
    if !job.sheets.is_empty() {
        let k = sheet_idx(job, a)?;
        let plans = job.plans();
        let p = &plans[k];
        data["sheet"] = json!(k + 1);
        data["contours"] = json!(p.contours.iter().enumerate().filter(|(_, c)| c.punch.is_none()).map(|(n, c)| json!({
            "cut": n + 1, "part": c.part_no, "hole": c.inner, "side": c.comp.side.label(),
            "word": c.comp.word().map(|(sd, d)| format!("{} D{d}", sd.word())),
            "offset_path": c.comp.system, "radius": (c.comp.radius * 1e4).round() / 1e4,
        })).collect::<Vec<_>>());
        let lost: Vec<Value> = plans.iter().enumerate().flat_map(|(si, p)| p.kerf_issues.iter().map(move |(part, inner, what)| json!({"sheet": si + 1, "part": part, "hole": inner, "what": what}))).collect();
        if !lost.is_empty() {
            msg.push_str(&format!(" {} contour(s) can't be cut fully with system compensation (see not_cut).", lost.len()));
        }
        data["not_cut"] = json!(lost);
    }
    Ok((msg, data))
}

/// The machine's hole order settings (CO20, CO14, CO15), in shop words.
fn hole_order_json(h: &crate::ct1::HoleOrder) -> Value {
    let corner = ["nearest the head", "lower left", "lower right", "upper right", "upper left"].get(h.corner as usize).copied().unwrap_or("nearest the head");
    let how = match h.mode {
        crate::ct1::HoleMode::None => "the part's own contour order, as drawn",
        crate::ct1::HoleMode::Proximity => "the nearest hole next, from where the head is",
        crate::ct1::HoleMode::Direction => "in rows or columns from the start corner, every other one back",
        crate::ct1::HoleMode::MinimumHeat => "the part's own contour order (the sheet's minimum-heat order isn't built yet, FEATURES MAC-53)",
    };
    json!({
        "mode": h.mode.label(), "mode_code": h.mode.code(), "cuts": how,
        "start_corner": corner, "start_corner_code": h.corner, "direction": if h.columns { "columns" } else { "rows" },
        "orthogonality_tolerance_mm": h.ortho_tol,
        "small_holes_first": h.small_first, "big_hole_min_length_mm": h.big_length, "big_hole_min_width_mm": h.big_width, "big_hole_offset_mm": h.big_offset,
    })
}

/// Where an order setting in use comes from (CO16, CO17): a change made in
/// Settings → Machines, the machine file (it sets one of `codes`), or the
/// default. `changed` reads `MachineTech::order_changes`, which holds only
/// the changes used (`OrderChanges::valid`), so a saved value the setting
/// can't take is reported as the file's, as it is cut.
fn order_from(t: &crate::ct1::MachineTech, changed: bool, codes: &[&str]) -> &'static str {
    if changed {
        "Settings → Machines"
    } else if codes.iter().any(|c| t.file.globals.contains_key(*c)) {
        "machine file"
    } else {
        "default"
    }
}

/// The sheet's order in shop words (CZ1–CZ6).
fn sheet_order_words(o: &crate::ct1::SheetOrder) -> String {
    let zones = if o.zones_x <= 1 && o.zones_y <= 1 { "one zone over the whole sheet".to_string() } else { format!("{} × {} zones", o.zones_x.max(1), o.zones_y.max(1)) };
    let (each, back) = if o.zigzag { ("the first", ", every other one back") } else { ("each", "") };
    let way = match (o.horizontal, o.start_left, o.start_bottom) {
        (true, l, b) => format!("rows from the {}, {each} from the {}{back}", if b { "bottom up" } else { "top down" }, if l { "left" } else { "right" }),
        (false, l, b) => format!("columns from the {}, {each} from the {}{back}", if l { "left" } else { "right" }, if b { "bottom up" } else { "top down" }),
    };
    format!("{zones}, {way}; in each zone the nearest part next")
}

/// The sheet's order in use, where each value comes from, and what the
/// machine file alone gives (CO16).
fn sheet_order_json(t: &crate::ct1::MachineTech) -> Value {
    let (o, f, c) = (&t.sheet_order, t.file_order().sheet, &t.order_changes);
    let values = |o: &crate::ct1::SheetOrder| json!({"start_corner": o.corner_label(), "direction": if o.horizontal { "rows" } else { "columns" }, "zigzag": o.zigzag, "zones": [o.zones_x, o.zones_y]});
    let mut v = values(o);
    v["cuts"] = json!(sheet_order_words(o));
    v["from"] = json!({
        "start_corner": order_from(t, c.start_left.is_some() || c.start_bottom.is_some(), &["B110"]),
        "direction": order_from(t, c.rows.is_some(), &["B114"]),
        "zigzag": order_from(t, c.zigzag.is_some(), &["B115"]),
        "zones": order_from(t, c.zones_x.is_some() || c.zones_y.is_some(), &["B122", "B123"]),
    });
    v["machine_file"] = values(&f);
    if t.file_says_top() {
        v["note"] = json!("The machine file's start bottom is off, but the sheet is cut from the bottom row up, as programs from the machine are (MECHANICS CZ6, CO28). A top corner can be set in Settings → Machines.");
    }
    v
}

/// The hole order in use, where each value comes from, and what the
/// machine file alone gives (CO17).
fn hole_order_report(t: &crate::ct1::MachineTech) -> Value {
    let c = &t.order_changes;
    let mut v = hole_order_json(&t.hole_order);
    v["from"] = json!({
        "mode": order_from(t, c.hole_mode.is_some(), &["B565"]),
        "start_corner": order_from(t, c.hole_start_corner.is_some(), &["B566"]),
        "direction": order_from(t, c.hole_columns.is_some(), &["B567"]),
        "orthogonality_tolerance_mm": order_from(t, c.orthogonality_tolerance.is_some(), &["B568"]),
        "small_holes_first": order_from(t, c.small_holes_first.is_some(), &["B587"]),
        "big_hole_min_length_mm": order_from(t, c.big_hole_min_length.is_some(), &["B588"]),
        "big_hole_min_width_mm": order_from(t, c.big_hole_min_width.is_some(), &["B589"]),
        "big_hole_offset_mm": order_from(t, c.big_hole_offset.is_some(), &["B590"]),
    });
    let mut file = hole_order_json(&t.file_order().holes);
    if let Some(m) = file.as_object_mut() {
        m.remove("cuts");
    }
    v["machine_file"] = file;
    v
}

fn machines(ctx: &Ctx) -> Value {
    let t = &ctx.tech;
    json!({
        "active": ctx.cfg.active_machine + 1,
        "machines": ctx.cfg.machines.iter().enumerate().map(|(i, m)| json!({
            "machine": i + 1, "name": m.name, "does": m.kind.key(), "output": m.output.key(), "post_lib": m.post_lib, "blm_ps1": m.blm_ps1, "blm_cf1": m.blm_cf1, "gas": m.gas_name,
            "order_changes": m.order.valid(),
        })).collect::<Vec<_>>(),
        "technology": {
            "source": t.source.as_ref().map_or("built-in values".to_string(), |p| p.display().to_string()),
            "lead_in": t.lead_in, "lead_out": t.lead_out, "overlap": t.overlap, "position": t.zone, "hole_lead_in": t.hole_lead_in,
            "joint_gap": t.joint_gap, "joint_every": t.joint_every, "group_parts": t.group_parts, "zones": [t.sheet_order.zones_x, t.sheet_order.zones_y],
            "sheet_order": sheet_order_json(t),
            "hole_order": hole_order_report(t),
            "material_sections": t.file.sections.len(),
            "qualities": qualities_json(&t.qualities),
            "note": "These are the machine's own values; a job's material and thickness may pick a section of the file instead (machine-tech).",
        },
        "post": { "extension": ctx.post.file_name("JOB", 1) },
        "note": "Settings are changed in the app (Settings window), not by agents. The sheet and hole order are changed per machine in Settings → Machines, and a change wins over the machine file (order_changes; each value's `from` says where it comes from). A job is put on a machine's process with set-sheet machine=<does> (MK1).",
    })
}

// ---------------------------------------------------------------------------
// Running an operation.
// ---------------------------------------------------------------------------

/// What an operation gave back.
pub struct Outcome {
    /// One line in shop terms.
    pub message: String,
    /// Details (reports, lists).
    pub data: Value,
    /// The job changed.
    pub changed: bool,
    /// Files written.
    pub files: Vec<PathBuf>,
}

impl Outcome {
    fn msg(m: impl Into<String>) -> Outcome {
        Outcome { message: m.into(), data: Value::Null, changed: true, files: vec![] }
    }
    fn read(m: impl Into<String>, data: Value) -> Outcome {
        Outcome { message: m.into(), data, changed: false, files: vec![] }
    }
    pub fn to_json(&self) -> Value {
        let mut v = json!({ "ok": true, "message": self.message });
        if !self.data.is_null() {
            v["data"] = self.data.clone();
        }
        if !self.files.is_empty() {
            v["files"] = json!(self.files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>());
        }
        v
    }
}

/// Reject arguments the operation doesn't take (catches typos).
pub fn check_args(op: &Op, args: &Value) -> Result<(), String> {
    if let Value::Object(m) = args {
        for k in m.keys() {
            if !op.args.iter().any(|x| x.name == k) {
                return Err(format!("{} doesn't take '{k}' (it takes: {})", op.name, op.args.iter().map(|x| x.name).collect::<Vec<_>>().join(", ")));
            }
        }
    }
    for x in op.args.iter().filter(|x| x.required) {
        if args.get(x.name).is_none_or(Value::is_null) {
            return Err(format!("{} needs '{}' ({})", op.name, x.name, x.help));
        }
    }
    Ok(())
}

/// A new job from `new`'s arguments.
/// A new job for an operation that makes one (`new`, `import-program`), and the
/// line to say about it.
pub fn create(ctx: &Ctx, op_name: &str, default_name: &str, args: &Value) -> Result<(JobFile, String), String> {
    let op = find(op_name).ok_or("no such operation")?;
    check_args(op, args)?;
    let a = Args(args);
    if op.name == "import-program" {
        let path = PathBuf::from(a.need_str("path")?);
        let pj = imports::program_job(&ctx.cfg, &default_spec(), &path, 0)?;
        let m = format!(
            "Opened {}: {} placed part(s) of {} kind(s) on a {:.0} × {:.0} mm sheet, {} {} mm{}.{}",
            pj.name,
            pj.sheet.len(),
            pj.parts.len(),
            pj.spec.w,
            pj.spec.h,
            cnc::MATERIALS.get(pj.spec.material).copied().unwrap_or("?"),
            pj.spec.thickness,
            if pj.cuts.is_empty() { String::new() } else { format!(", {} cut line(s)", pj.cuts.len()) },
            if pj.notes.is_empty() { String::new() } else { format!(" ({})", pj.notes.join("; ")) }
        );
        return Ok((pj.into_jobfile(), m));
    }
    let mut spec = with_prefs(&default_spec(), &ctx.cfg);
    set_spec_fields(&mut spec, &a, ctx)?;
    let mut job = JobFile::new_job(&a.str("name").unwrap_or_else(|| default_name.to_string()), spec);
    job.week = a.str("week").unwrap_or_default();
    let m = format!("New job '{}' — sheet {} × {} mm, {} {} mm, {}. Add parts next.", job.name, job.spec.w, job.spec.h, cnc::MATERIALS.get(job.spec.material).copied().unwrap_or("?"), job.spec.thickness, job_gas(&job.spec, &ctx.blm).label());
    Ok((job, m))
}

/// `import-xml`: one job file per material and thickness.
fn import_xml(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
    let path = PathBuf::from(a.need_str("path")?);
    let lib = library(ctx);
    let mut im = imports::xml_import(&ctx.cfg, lib.as_ref().ok(), &path, 0)?;
    // Products without a drawing come from the Library: say why it wasn't
    // looked in (LB11).
    if let Err(e) = &lib {
        im.notes.insert(0, format!("Library parts weren't looked up: {e}"));
    }
    let dir = a.str("dir").map(PathBuf::from).unwrap_or_else(|| path.parent().map(Path::to_path_buf).unwrap_or_default());
    let force = a.bool("force")?.unwrap_or(false);
    let files: Vec<PathBuf> = im.jobs.iter().map(|j| dir.join(format!("{}.{}", safe_name(&j.name), jobfile::EXT))).collect();
    if !force {
        let there: Vec<String> = files.iter().filter(|f| f.exists()).map(|f| f.display().to_string()).collect();
        if !there.is_empty() {
            return Err(format!("These job files exist already: {} — use force=true to overwrite, or another dir.", there.join(", ")));
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't make {}: {e}", dir.display()))?;
    let mut notes = im.notes.clone();
    let mut jobs = vec![];
    for (xj, f) in im.jobs.into_iter().zip(&files) {
        let mut spec = with_prefs(&default_spec(), &ctx.cfg);
        match xj.material {
            Some(m) => spec.material = m,
            None if !xj.material_text.is_empty() => notes.push(format!("{}: material '{}' isn't one of the app's — set to {}", xj.name, xj.material_text, cnc::MATERIALS[spec.material])),
            None => {}
        }
        if xj.thickness > 0.0 {
            spec.thickness = xj.thickness;
        }
        let mut j = JobFile::new_job(&xj.name, spec);
        j.week = "XML import".into();
        j.stale = !xj.parts.is_empty();
        jobs.push(json!({"job": f.display().to_string(), "name": xj.name, "material": cnc::MATERIALS.get(j.spec.material).copied().unwrap_or("?"), "thickness": j.spec.thickness, "parts": xj.parts.iter().map(|p| json!({"name": p.name, "qty": p.qty})).collect::<Vec<_>>()}));
        j.parts = xj.parts;
        jobfile::save(f, &j)?;
    }
    let mut o = Outcome::read(
        format!("XML import {}: {} of {} product(s) into {} job file(s). Set the sheet of each, then nest.{}", path.display(), im.added, im.products, files.len(), if notes.is_empty() { String::new() } else { format!(" — {}", notes.join("; ")) }),
        json!({ "jobs": jobs, "notes": notes }),
    );
    o.files = files;
    Ok(o)
}

fn set_spec_fields(spec: &mut SheetSpec, a: &Args, ctx: &Ctx) -> Result<(), String> {
    let pos = |k: &str, v: f64, min: f64| if v >= min { Ok(v) } else { Err(format!("{k} must be at least {min}")) };
    if let Some(v) = a.num("w")? {
        spec.w = pos("w", v, 1.0)?;
    }
    if let Some(v) = a.num("h")? {
        spec.h = pos("h", v, 1.0)?;
    }
    if let Some(v) = a.num("margin")? {
        spec.margin = pos("margin", v, 0.0)?;
    }
    if let Some(v) = a.num("gap")? {
        spec.gap = pos("gap", v, 0.0)?;
    }
    if let Some(v) = a.str("material") {
        spec.material = material_from(&v, &ctx.cfg)?;
    }
    if let Some(v) = a.num("thickness")? {
        spec.thickness = pos("thickness", v, 0.01)?;
    }
    if let Some(v) = a.str("gas") {
        spec.gas = Some(crate::gas::Gas::parse(&v)?);
    }
    if let Some(v) = a.int("level")? {
        if !(1..=5).contains(&v) {
            return Err("level is 1 to 5".into());
        }
        spec.level = v as u8;
    }
    if let Some(v) = a.str("machine") {
        spec.machine = machine_from(&v)?;
    }
    Ok(())
}

/// Run an operation that needs no job.
pub fn run_global(ctx: &Ctx, name: &str, args: &Value) -> Result<Outcome, String> {
    let op = find(name).ok_or_else(|| format!("unknown operation '{name}'"))?;
    check_args(op, args)?;
    let a = Args(args);
    match op.name {
        "catalog" => Ok(Outcome::read("Choices for the operations.", catalog())),
        "stock" => stock_op(ctx, &a),
        "machines" => Ok(Outcome::read(format!("Active machine: {}.", ctx.cfg.machine().name), machines(ctx))),
        "import-xml" => import_xml(ctx, &a),
        "job-folders" => super::folderops::run(ctx, &a, crate::library::Kind::Job),
        "part-folders" => super::folderops::run(ctx, &a, crate::library::Kind::Product),
        "library-check" => {
            let rounds = match a.int("rounds")? {
                Some(n) if n < 1 => return Err(format!("rounds {n}: at least 1")),
                n => n.map(|n| n as u64),
            };
            let minutes = a.num("minutes")?.unwrap_or(1.0);
            if !(minutes > 0.0) {
                return Err(format!("minutes {minutes}: above 0"));
            }
            let opt = super::librarycheck::Options { minutes, rounds, pc: a.str("pc").unwrap_or_default() };
            let rep = super::librarycheck::run(Path::new(&a.need_str("folder")?), &ctx.library_dir, &opt)?;
            // The report is the plain text (`--text` prints just it); FAIL is a refusal (exit code 1).
            if rep.passed { Ok(Outcome::read(rep.text, Value::Null)) } else { Err(rep.text) }
        }
        "drawing-layers" => {
            let path = a.need_str("path")?;
            let parts = read_drawing(ctx, Path::new(&path), &a)?;
            let mut layers: BTreeMap<String, usize> = BTreeMap::new();
            for p in &parts {
                for (l, n) in p.drawing.layers() {
                    *layers.entry(l).or_insert(0) += n;
                }
            }
            let mut roles = crate::formats::default_roles(&parts);
            // DX3: rows that start as "Leave out", with why.
            let hidden: BTreeMap<String, String> = parts.iter().flat_map(|p| p.drawing.hidden.clone()).collect();
            let start = roles.clone();
            if let Some(spec) = a.str("layers") {
                set_layer_roles(&mut roles, &spec)?;
            }
            // What each part comes in as with those roles, as the import window's
            // preview shows it (IM10): its size, holes and contours (IM12), or
            // why it can't be made.
            let sizes: Vec<Value> = parts
                .iter()
                .map(|p| match crate::dxf::build(&p.drawing, &roles) {
                    Ok((g, rep)) => {
                        let list = g.contour_list();
                        let rows: Vec<Value> = list.iter().enumerate().map(|(k, c)| json!({"contour": k + 1, "kind": contours::kind(&list, k), "state": c.state.key(), "length_mm": r1(c.length())})).collect();
                        json!({"w": r1(g.w), "h": r1(g.h), "holes": g.holes.len(), "contours": rows, "notes": crate::engine::import_notes(&path, &rep)})
                    }
                    Err(e) => json!({"error": e}),
                })
                .collect();
            let first = sizes.first().and_then(|v| Some((v["w"].as_f64()?, v["h"].as_f64()?))).map_or(String::new(), |(w, h)| format!(", {w} × {h} mm"));
            let data = json!({
                "layers": layers,
                "default_roles": start.iter().map(|(k, v)| (k.clone(), v.label())).collect::<BTreeMap<_, _>>(),
                "roles": roles.iter().map(|(k, v)| (k.clone(), v.label())).collect::<BTreeMap<_, _>>(),
                "left_out_by_default": hidden,
                "parts": parts.len(),
                "sizes": sizes,
            });
            Ok(Outcome::read(format!("{} layer(s) in {path}{}{first}.", layers.len(), if parts.len() > 1 { format!(" ({} parts)", parts.len()) } else { String::new() }), data))
        }
        n => Err(format!("{n} needs a job")),
    }
}

/// The `format` choices: every drawing format's key.
const FORMATS: &[&str] = &["dxf", "dwg", "mec", "iges", "pdf", "essi", "dstv", "geo"];

fn read_drawing(ctx: &Ctx, path: &Path, a: &Args) -> Result<Vec<crate::formats::ReadPart>, String> {
    let fmt = match a.str("format") {
        Some(f) => Some(crate::formats::Format::from_key(&f).ok_or_else(|| format!("format: '{f}' is not one of {}", FORMATS.join(", ")))?),
        None => None,
    };
    use crate::formats::pdf::{Layers, Pages};
    let opts = crate::formats::ReadOpts {
        essi_dialect: a.str("dialect"),
        pdf_scale: a.num("scale")?,
        pdf_pages: a.str("pages").map(|p| if p == "separate" { Pages::Separate } else { Pages::SideBySide }),
        pdf_layers: a.str("pdf_layers").map(|l| match l.as_str() {
            "pdf" => Layers::PdfLayers,
            "one" => Layers::None,
            _ => Layers::Colour,
        }),
        ..Default::default()
    };
    crate::formats::read_file_with(path, fmt, &ctx.cfg, opts)
}

/// The Library, as the agent's user (LB5).
fn library(ctx: &Ctx) -> Result<crate::library::Library, String> {
    reach_library(ctx, "Nothing was read from or saved to the Library, and the job is as it was")?;
    Ok(crate::library::Library::open(&ctx.library_dir, &ctx.who, &ctx.clock)?.watching(&ctx.watch))
}

/// The Library folder can be used, or the refusal naming it, what was left
/// as it was (`kept`) and what to do instead (LB11, LB12, LB13). Nothing is
/// made either way.
pub(super) fn reach_library(ctx: &Ctx, kept: &str) -> Result<(), String> {
    crate::library::usable(&ctx.library_dir).map_err(|e| format!("{e} {kept}: try again once it can be reached."))
}

fn who(job: &JobFile, t: &[usize]) -> String {
    if t.len() == 1 { format!("'{}'", job.parts[t[0]].name) } else { format!("{} parts", t.len()) }
}

fn edit_tech(job: &mut JobFile, args: &Args, e: TechEdit, what: &str) -> Result<Outcome, String> {
    let t = parts_sel(&job.parts, args.get("parts"), true)?;
    for &i in &t {
        let bbox = job.parts[i].bbox();
        e.apply(&mut job.parts[i].tech, bbox);
    }
    Ok(Outcome::msg(format!("{what} for {}.", who(job, &t))))
}

// ---------------------------------------------------------------------------
// Sheets from the stock, and the stock operation.
// ---------------------------------------------------------------------------

fn describe_format(f: &SheetFormat) -> String {
    match (f.stock_ref.is_empty(), f.remnant) {
        (true, _) => format!("a {:.0} × {:.0} mm sheet", f.w, f.h),
        (false, true) => format!("remnant {} ({:.0} × {:.0} mm)", f.stock_ref, f.w, f.h),
        (false, false) => format!("stock sheet {} ({:.0} × {:.0} mm)", f.stock_ref, f.w, f.h),
    }
}

/// The sheet `ref=` (from stock, matching the job, one still free for it) or
/// `w=`/`h=` names; neither = the job's sheet. `replacing`: the sheet being
/// changed (the stock sheet it is now counts as free).
fn sheet_source(job: &JobFile, ctx: &Ctx, a: &Args, replacing: Option<usize>) -> Result<SheetFormat, String> {
    if let Some(r) = a.str("ref") {
        let st = ctx.stock()?;
        let it = st.get(&r).ok_or_else(|| format!("No sheet '{r}' in stock — see stock mode=list."))?;
        if it.material != job.spec.material || (it.thickness - job.spec.thickness).abs() > crate::stock::TOL {
            return Err(format!("'{r}' is {} {} mm; the job is {} {} mm.", cnc::MATERIALS.get(it.material).copied().unwrap_or("?"), it.thickness, cnc::MATERIALS.get(job.spec.material).copied().unwrap_or("?"), job.spec.thickness));
        }
        if !ctx.cfg.stock.allow_no_stock && !stockflow::one_free(it, &job.held, &job.formats, job.sheets.len(), replacing) {
            let used = job.formats.iter().take(job.sheets.len()).filter(|f| f.stock_ref == r).count();
            return Err(format!("'{r}' has no more in stock ({} on the rack, the job already uses {used}).", it.qty));
        }
        return Ok(SheetFormat::from_stock(it));
    }
    match (a.num("w")?, a.num("h")?) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Ok(SheetFormat::rect(w, h)),
        (None, None) => Ok(SheetFormat::rect(job.spec.w, job.spec.h)),
        _ => Err("give both w and h (mm, above 0), or ref".into()),
    }
}

/// Put these remnant lines on a sheet in place of its old ones (refused when
/// one cuts through a part).
fn set_remnant_lines(parts: &[Part], sheet: &[Placed], list: &mut Vec<CutLine>, lines: Vec<CutLine>) -> Result<(), String> {
    for l in &lines {
        if let Some(i) = cutoff::path_hits_part(parts, sheet, &l.pts) {
            return Err(format!("That remnant line would cut through part #{} — try another shape or a larger offset.", i + 1));
        }
    }
    list.retain(|l| l.kind != CutKind::Remnant);
    list.extend(lines);
    Ok(())
}

/// The automatic remnant line for a sheet that has none yet (None when it
/// has one, there's no remnant, or the line would cut a part).
pub fn auto_remnant_lines(parts: &[Part], sheet: &[Placed], spec: &SheetSpec, ls: &crate::remline::LineSettings) -> Option<Vec<CutLine>> {
    let lines = crate::remline::auto_lines(parts, sheet, spec, ls, crate::kerf::kerf_mm(spec) / 2.0);
    (!lines.is_empty() && lines.iter().all(|l| cutoff::path_hits_part(parts, sheet, &l.pts).is_none())).then_some(lines)
}

fn stock_json(s: &crate::stock::Item, clock: &crate::shared::Clock) -> Value {
    json!({
        "ref": s.reference, "kind": s.kind.label(), "material": cnc::MATERIALS.get(s.material).copied().unwrap_or("?"), "thickness": s.thickness, "w": r1(s.length), "h": r1(s.width), "qty": s.qty,
        "priority": s.priority, "blocked": s.locked, "remnant": s.kind.is_remnant(), "area_m2": (s.area_m2() * 1000.0).round() / 1000.0,
        "origin": s.origin, "job": s.from_job, "irregular": s.outline.is_some(), "user": s.user_data, "created": s.created,
        // Who changed it last, and when (SV9); null for a stock from before.
        "changed_by": s.changed.as_ref().map(|c| c.by.label()), "changed_at": s.changed.as_ref().map(|c| clock.stamp(c.at)),
    })
}

fn rules_json(st: &crate::stock::StockSettings) -> Value {
    json!({
        "use_stock": st.use_stock, "order": st.order.label(), "remnants_first": st.remnants_first,
        "job_sheet_when_out": st.fallback_job_size, "allow_no_stock": st.allow_no_stock,
        "min_efficiency": st.min_efficiency, "efficiency_by": st.eff_mode.label(), "with_references": st.with_references,
        "remnants_on_save": st.remnants_on_save, "keep_completed_sheets": st.keep_completed, "min_remnant_m2": st.min_remnant_m2,
        "remnant_line": st.line.shape.label(), "lines_after_nesting": st.lines_after_nesting,
        "best_sheet_format": st.best_format, "best_sheet_combination": st.best_combination, "combination_min_efficiency": st.combination_min,
        "max_search_seconds": st.max_seconds, "prompt_remnant_refs": st.prompt_remnant_ref,
        "criteria": { "min_w": st.criteria.min_length, "max_w": st.criteria.max_length, "min_h": st.criteria.min_width, "max_h": st.criteria.max_width },
    })
}

fn stock_op(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
    let file = Stock::file(&ctx.library_dir);
    let mat = match a.str("material") {
        Some(m) => Some(material_from(&m, &ctx.cfg)?),
        None => None,
    };
    let thk = a.num("thickness")?;
    let user = || [a.str("user1").unwrap_or_default(), a.str("user2").unwrap_or_default(), a.str("user3").unwrap_or_default()];
    let qty = || -> Result<i32, String> {
        match a.int("qty")? {
            None => Ok(1),
            Some(q) if q >= 1 => Ok(q.min(crate::stock::MAX_QTY as i64) as i32),
            Some(q) => Err(format!("qty {q}: at least 1")),
        }
    };
    let need_ref = || a.str("ref").ok_or_else(|| "give the sheet's ref".to_string());
    let matches = |it: &crate::stock::Item| mat.is_none_or(|m| it.material == m) && thk.is_none_or(|t| (it.thickness - t).abs() <= crate::stock::TOL);
    let describe = |it: &crate::stock::Item| format!("{} {} ({:.0} × {:.0} mm)", it.kind.label().to_lowercase(), it.reference, it.length, it.width);
    let mode = a.need_str("mode")?;
    // The stock is the Library's: none to read or change while it can't be
    // reached (LB11). The options are this PC's settings.
    if mode != "rules" {
        reach_library(ctx, "The sheet stock was not read or changed")?;
    }
    match mode.as_str() {
        "list" => {
            let st = ctx.stock()?;
            let mut crit = crate::stock::Criteria {
                min_length: a.num("min_w")?.unwrap_or(0.0),
                max_length: a.num("max_w")?.unwrap_or(0.0),
                min_width: a.num("min_h")?.unwrap_or(0.0),
                max_width: a.num("max_h")?.unwrap_or(0.0),
                show_zero_qty: a.bool("show_zero")?.unwrap_or(false),
                include_locked: true,
                quoting: true,
                ..Default::default()
            };
            match a.str("remnants").as_deref() {
                Some("none") => {
                    crit.nest_remnants = false;
                    crit.user_remnants = false;
                }
                Some("only") => crit.sheets = false,
                Some("all") | None => {}
                Some(x) => return Err(format!("remnants '{x}': only, none or all")),
            }
            let list: Vec<Value> = st.items.iter().filter(|it| matches(it) && crit.admits(it)).map(|it| stock_json(it, &ctx.clock)).collect();
            let n: i64 = st.items.iter().map(|s| s.qty.max(0) as i64).sum();
            return Ok(Outcome::read(format!("{} stock item(s) listed ({} sheet(s) and remnant(s) in stock in all). Stock file: {}", list.len(), n, file.display()), json!({ "sheets": list, "rules": rules_json(&ctx.cfg.stock) })));
        }
        "rules" => {
            return Ok(Outcome::read(
                "The Do All and Sheet Remnants options nesting and saving follow. They are the window's settings (Sheet stock → Options) and aren't changed from here.",
                rules_json(&ctx.cfg.stock),
            ))
        }
        _ => {}
    }
    // Every change re-reads the stock, changes it and saves it under the
    // Library's write lock, as the window does (LB1, STK-9).
    let m = Stock::update(&ctx.library_at(), |st| match mode.as_str() {
        "create" => {
            let (Some(w), Some(h)) = (a.num("w")?, a.num("h")?) else { return Err("create needs w and h (mm)".into()) };
            if w <= 0.0 || h <= 0.0 {
                return Err("w and h must be above 0".into());
            }
            let m = mat.unwrap_or(0);
            let t = thk.ok_or("create needs the thickness")?;
            let mut it = crate::stock::Item::sheet(&a.str("ref").unwrap_or_default(), m, t, w, h, qty()?);
            if let Some(p) = a.int("priority")? {
                it.priority = p.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            }
            it.locked = a.bool("blocked")?.unwrap_or(false);
            it.user_data = user();
            let r = st.add_sheet(it, ctx.cfg.stock.with_references)?;
            let it = st.get(&r).ok_or("the new sheet went missing")?;
            let msg = format!("Stock: {} {} {} mm — {} on the rack.", describe(it), cnc::MATERIALS.get(m).copied().unwrap_or("?"), t, it.qty);
            Ok(msg)
        }
        "add" | "remove" => {
            let r = need_ref()?;
            let q = qty()?;
            st.change_qty(&r, if a.need_str("mode")? == "add" { q } else { -q })?;
            let msg = match st.get(&r) {
                None => format!("'{r}' is used up and removed from stock."),
                Some(it) => format!("'{r}': {} in stock.", it.qty),
            };
            Ok(msg)
        }
        "block" | "unblock" => {
            let on = a.need_str("mode")? == "block";
            let n = match a.str("ref") {
                Some(r) => {
                    st.get(&r).ok_or_else(|| format!("No sheet '{r}' in stock."))?;
                    st.set_locked(&r, on);
                    1
                }
                None => {
                    let mut n = 0;
                    for it in st.items.iter_mut().filter(|it| matches(it)) {
                        it.locked = on;
                        n += 1;
                    }
                    n
                }
            };
            Ok(format!("{} {n} stock item(s) — {}", if on { "Locked" } else { "Unlocked" }, if on { "Do All won't use them." } else { "Do All may use them." }))
        }
        "edit" => {
            let r = need_ref()?;
            let it = st.items.iter_mut().find(|i| i.reference == r).ok_or_else(|| format!("No sheet '{r}' in stock."))?;
            for (key, slot) in [("w", &mut it.length), ("h", &mut it.width)] {
                if let Some(v) = a.num(key)? {
                    if it.outline.is_some() {
                        return Err("An irregular remnant's size comes from its outline.".into());
                    }
                    *slot = v.max(1.0);
                }
            }
            if let Some(p) = a.int("priority")? {
                it.priority = p.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            }
            if let Some(b) = a.bool("blocked")? {
                it.locked = b;
            }
            for (k, key) in ["user1", "user2", "user3"].iter().enumerate() {
                if let Some(v) = a.str(key) {
                    it.user_data[k] = v;
                }
            }
            let msg = format!("Stock: {} updated.", describe(it));
            Ok(msg)
        }
        "delete" => {
            let r = need_ref()?;
            let it = st.get(&r).cloned().ok_or_else(|| format!("No sheet '{r}' in stock."))?;
            st.delete(&r);
            Ok(format!("{} removed from stock ({} on the rack).", describe(&it), it.qty))
        }
        m => Err(format!("stock mode '{m}'?")),
    })?;
    let mut o = Outcome::read(m, Value::Null);
    o.files.push(file);
    Ok(o)
}


/// Run an operation on a job. The caller saves the job when `changed`.
pub fn run(job: &mut JobFile, ctx: &Ctx, name: &str, args: &Value) -> Result<Outcome, String> {
    let op = find(name).ok_or_else(|| format!("unknown operation '{name}' — see `nestpro ops`"))?;
    if op.kind == Kind::Global {
        return run_global(ctx, name, args);
    }
    check_args(op, args)?;
    let a = Args(args);
    match op.name {
        "info" => {
            let d = match a.int("sheet")? {
                Some(_) => Some(sheet_idx(job, &a)?),
                None => None,
            };
            let v = info(job, ctx, d);
            let m = format!(
                "{}: {} part type(s), {} of {} nested on {} sheet(s).",
                job.name,
                job.parts.len(),
                v["totals"]["nested"],
                v["totals"]["ordered"],
                job.sheets.len()
            );
            Ok(Outcome::read(m, v))
        }
        "rename" => {
            if let Some(n) = a.str("name") {
                job.name = n;
            }
            if let Some(w) = a.str("week") {
                job.week = w;
            }
            Ok(Outcome::msg(format!("Job is now '{}' in '{}'.", job.name, job.week)))
        }
        "undo" => Err("undo is handled by the session".into()),
        "add-shape" => {
            let shape = shape_from(&a.need_str("shape")?)?;
            let mut p = shape_part(shape, job.parts.len() + 1, next_color(&job.parts));
            let (w, h) = (a.num("w")?, a.num("h")?);
            // A disc is as wide as it is high: one size sets both.
            let (w, h) = if shape == Shape::Disc { (w.or(h), h.or(w)) } else { (w, h) };
            if let Some(v) = w {
                p.w = v.max(1.0);
            }
            if let Some(v) = h {
                p.h = v.max(1.0);
            }
            if let Some(v) = a.int("qty")? {
                p.qty = v.max(0) as u32;
            }
            if let Some(v) = a.str("name") {
                p.name = v;
            }
            if let Some(v) = a.bool("rotate")? {
                p.rotate = v;
            }
            if let Some(v) = a.str("group") {
                p.group = v;
            }
            let m = format!("Added part {}: {} {} × {} mm, {} off.", job.parts.len() + 1, p.name, p.bbox().0, p.bbox().1, p.qty);
            job.parts.push(p);
            job.stale = true;
            Ok(Outcome::msg(m))
        }
        "import-drawing" => {
            let paths: Vec<PathBuf> = a.need_str("path")?.split(';').filter(|s| !s.trim().is_empty()).map(|s| PathBuf::from(s.trim())).collect();
            let mut drawings = vec![];
            for p in &paths {
                let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Drawing".into());
                let parts = read_drawing(ctx, p, &a)?;
                drawings.extend(crate::formats::incoming(&stem, parts));
            }
            let mut roles = crate::formats::default_roles(drawings.iter().map(|d| &d.part));
            if let Some(spec) = a.str("layers") {
                set_layer_roles(&mut roles, &spec)?;
            }
            let wanted = a.str("contours").filter(|c| !c.trim().is_empty());
            if wanted.is_some() && drawings.len() != 1 {
                return Err(format!(
                    "contours: contour numbers belong to one part, and this import makes {} parts. Import one drawing of one part with contours=, and the others without; or set their contours afterwards with contour-tech.",
                    drawings.len()
                ));
            }
            let mut notes = vec![];
            let mut added = vec![];
            for inc in &drawings {
                // IM12: the contours chosen, numbered on the part these roles make.
                let choices = match &wanted {
                    Some(c) => {
                        let (bare, _) = part_from_read(&ctx.cfg, &job.spec, &inc.file, &inc.name, &inc.part.drawing, &inc.part.meta, &roles, 0)?;
                        contour_choices_from(c, contours::list(&bare).len())?
                    }
                    None => vec![],
                };
                let (mut p, n) = imports::chosen_part(&ctx.cfg, &job.spec, inc, &roles, &choices, next_color(&job.parts))?;
                notes.extend(n);
                if let Some(q) = a.int("qty")? {
                    p.qty = q.max(0) as u32;
                }
                if let (Some(n), 1) = (a.str("name"), drawings.len()) {
                    p.name = n;
                }
                if let Some(g) = a.str("group") {
                    p.group = g;
                }
                added.push(json!({"part": job.parts.len() + 1, "name": p.name, "w": r1(p.w), "h": r1(p.h), "qty": p.qty, "holes": p.custom.as_ref().map_or(0, |c| c.holes.len())}));
                job.parts.push(p);
            }
            job.stale = true;
            let mut o = Outcome::msg(format!("Imported {} drawing(s).{}", added.len(), if notes.is_empty() { String::new() } else { format!(" {}", notes.join(" · ")) }));
            o.data = json!({ "added": added, "notes": notes, "layers": roles.iter().map(|(k, v)| (k.clone(), v.label())).collect::<BTreeMap<_, _>>() });
            Ok(o)
        }
        "edit-part" => {
            let i = one_part(job, &a)?;
            let mut d = job.parts[i].clone();
            if let Some(v) = a.str("name") {
                d.name = v;
            }
            if let Some(v) = a.int("qty")? {
                d.qty = v.max(0) as u32;
            }
            if let Some(v) = a.bool("rotate")? {
                d.rotate = v;
            }
            if let Some(v) = a.bool("mirror")? {
                d.rots.mirrored = [v; 4];
            }
            if let Some(v) = a.str("group") {
                d.group = v;
            }
            let (nw, nh) = (a.num("w")?, a.num("h")?);
            let resized = nw.is_some_and(|w| (w - d.w).abs() > 1e-9) || nh.is_some_and(|h| (h - d.h).abs() > 1e-9);
            if resized && d.shape == Shape::Custom {
                return Err("An imported drawing keeps its size — import it again to change it.".into());
            }
            // Only the sizes given change (at least 0.5 mm, as in the window);
            // a disc's diameter is both.
            let (nw, nh) = if d.shape == Shape::Disc { (nw.or(nh), nh.or(nw)) } else { (nw, nh) };
            if let Some(w) = nw {
                d.w = w.max(0.5);
            }
            if let Some(h) = nh {
                d.h = h.max(0.5);
            }
            if resized && d.tech.is_some() {
                // The original asks: edit a copy (keeps this part's technology) or
                // edit the part and remove its technology (PT2).
                match a.str("tech").as_deref() {
                    Some("copy") => {
                        let base = job.parts[i].name.clone();
                        d.tech = None;
                        d.name = copy_name(&job.parts, &base);
                        d.color = next_color(&job.parts);
                        let n = d.name.clone();
                        insert_part(&mut job.parts, &mut job.sheets, i + 1, d);
                        job.stale = true;
                        return Ok(Outcome::msg(format!("Kept '{base}' with its technology; the change went to '{n}' (part {}).", i + 2)));
                    }
                    Some("drop") => d.tech = None,
                    _ => return Err(format!("'{}' has its own technology; changing its size can't keep it. Say tech=copy (change a copy) or tech=drop (remove its technology).", job.parts[i].name)),
                }
            }
            let geometry_changed = resized || d.rotate != job.parts[i].rotate || d.rots != job.parts[i].rots;
            job.parts[i] = d;
            if geometry_changed || a.int("qty")?.is_some() || a.str("group").is_some() {
                job.stale = true;
            }
            Ok(Outcome::msg(format!("Part {} '{}' changed.", i + 1, job.parts[i].name)))
        }
        "remove-part" => {
            let i = one_part(job, &a)?;
            let n = job.parts[i].name.clone();
            remove_part(&mut job.parts, &mut job.sheets, i);
            job.manual_cuts.iter_mut().for_each(Vec::clear);
            job.tidy();
            job.stale = true;
            Ok(Outcome::msg(format!("Removed '{n}' and its placed copies.")))
        }
        "part-info" => {
            let i = one_part(job, &a)?;
            Ok(Outcome::read(format!("Part {} '{}'.", i + 1, job.parts[i].name), part_info(job, i)))
        }
        "contour-tech" => {
            let i = one_part(job, &a)?;
            let state = a.str("state").map(|s| contour_state_from(&s)).transpose()?;
            let quality = a.str("quality").as_deref().map(contour_quality_from).transpose()?;
            let picks = contours::parse_numbers(&a.need_str("contours")?, contours::list(&job.parts[i]).len())?;
            let same = a.bool("same")?.unwrap_or(false);
            // The same engine function as the window's cut technology panel (CS6).
            let paint = contours::Paint { state, quality };
            let done = contours::paint(&mut job.parts, &job.sheets, &mut job.manual_cuts, i, &picks, paint, same)?;
            if done.nest_changed {
                job.stale = true;
            }
            let mut data = json!({"contours": done.contours.iter().map(|k| k + 1).collect::<Vec<_>>()});
            if let Some(st) = state {
                data["state"] = json!(st.key());
            }
            if let Some(q) = quality {
                data["quality"] = q.map_or(json!("auto"), |q| json!(q.number()));
                data["skipped"] = json!(done.skipped.iter().map(|(k, _)| k + 1).collect::<Vec<_>>());
            }
            let mut o = Outcome::msg(done.message);
            o.data = data;
            Ok(o)
        }
        "part-cut-order" => {
            let i = one_part(job, &a)?;
            let part = &mut job.parts[i];
            cutorder::check(part)?;
            let cmd = match a.need_str("mode")?.as_str() {
                "add" => cutorder::Command::Contours(contours::parse_in_order(&a.need_str("contours")?, contours::list(part).len())?),
                "all-marks" => cutorder::Command::AllMarks,
                "all-cut" => cutorder::Command::AllCut,
                "undo" => cutorder::Command::Undo,
                "clear" => cutorder::Command::Clear,
                "all-quality" => {
                    let q = a.need_str("quality")?;
                    cutorder::Command::Quality(numbered_quality(&q).ok_or_else(|| format!("quality '{}': give one of the machine's cut qualities, 1 to 12 (part-info shows each contour's).", q.trim()))?)
                }
                m => return Err(format!("part-cut-order mode '{m}': give add, all-marks, all-cut, all-quality, undo or clear.")),
            };
            // One engine function for agents and the part screen's Contours menu (CO3).
            let done = cutorder::apply(part, &crate::ct1::for_spec(&job.spec), &cmd)?;
            let one = |v: &[usize]| v.iter().map(|k| k + 1).collect::<Vec<_>>();
            Ok(Outcome {
                message: format!("Part {} '{}': {}", i + 1, part.name, done.message),
                data: json!({"added": one(&done.added), "skipped": one(&done.skipped)}),
                changed: done.changed,
                files: vec![],
            })
        }
        "export-part-dxf" => {
            let i = one_part(job, &a)?;
            let p = &job.parts[i];
            let g = p.geometry();
            let marks = p.custom.as_ref().map_or(vec![], |c| c.marks.clone());
            let out = PathBuf::from(a.need_str("out")?);
            if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::write(&out, dxf::write(&g.outline, &g.holes, &marks)).map_err(|e| format!("Couldn't save the DXF: {e}"))?;
            let mut o = Outcome::read(format!("Saved '{}' as DXF.", p.name), Value::Null);
            o.files.push(out);
            Ok(o)
        }
        "library" => {
            use crate::library::{Entry, Refused, SaveTo};
            let lib = library(ctx)?;
            match a.need_str("mode")?.as_str() {
                "list" => {
                    let find = a.str("find").unwrap_or_default().to_lowercase();
                    // A job's folder is its place in the Library's job folders
                    // (JX10), a part's in its part folders (JX20).
                    let (jobs, parts) = (crate::folders::load(&lib, crate::library::Kind::Job)?, crate::folders::load(&lib, crate::library::Kind::Product)?);
                    let folder = |e: &crate::library::Entry| {
                        let tree = if e.kind == crate::library::Kind::Job { &jobs } else { &parts };
                        tree.path(tree.folder_of(e.id))
                    };
                    let mut list: Vec<Value> = lib
                        .entries()
                        .into_iter()
                        .filter(|e| find.is_empty() || e.name.to_lowercase().contains(&find) || folder(e).to_lowercase().contains(&find))
                        // Who last saved it and when, in this PC's local time (LB6);
                        // no one for a record saved before saves recorded it.
                        .map(|e| json!({"id": e.id, "kind": format!("{:?}", e.kind).to_lowercase(), "name": e.name, "folder": folder(&e), "material": cnc::MATERIALS.get(e.material).copied().unwrap_or("?"), "thickness": e.thickness, "w": r1(e.w), "h": r1(e.h), "preprogrammed": e.tech, "saved_by": e.saved_by.as_ref().map(|w| w.label()), "saved_at": (e.updated > 0).then(|| ctx.clock.stamp(e.updated))}))
                        .collect();
                    // Jobs that share their name with another job (JX17:
                    // Libraries saved before names were unique): the others'
                    // numbers, so one can be renamed.
                    let all = lib.entries();
                    let twins = crate::library::shared_job_names(&all);
                    // Jobs someone has in use (CU10): who, and since when.
                    let in_use = crate::engine::inuse::look(&lib).unwrap_or_default();
                    for v in list.iter_mut().filter(|v| v["kind"] == "job") {
                        if let Some(others) = v["id"].as_u64().and_then(|id| twins.get(&id)) {
                            v["same_name_as"] = json!(others.iter().map(|e| e.id).collect::<Vec<_>>());
                        }
                        if let Some(u) = v["id"].as_u64().and_then(|id| in_use.get(&id)) {
                            v["in_use"] = json!(u.by_since(&ctx.clock));
                            v["in_use_since"] = json!(u.hold.as_ref().map(|h| ctx.clock.stamp(h.since)));
                        }
                    }
                    Ok(Outcome::read(format!("{} library record(s).", list.len()), json!(list)))
                }
                "open" => {
                    // The file's own stock booking would be left taken with no
                    // job: refused while it holds any.
                    let (sheets, remnants) = (job.held.values().sum::<i32>(), job.formats.iter().map(|f| f.made.len()).sum::<usize>());
                    if job.booked && (sheets > 0 || remnants > 0) {
                        return Err(format!("This job file holds stock it booked ({sheets} sheet(s), {remnants} remnant(s)); opening a library job into it would leave them taken with no job. Open it into a new job file (new, then library mode=open), or give them back first (stock-book mode=release)."));
                    }
                    let id = match (a.int("id")?, a.str("name")) {
                        (Some(id), _) => id as u64,
                        (None, Some(n)) => {
                            let jobs: Vec<Entry> = lib.entries().into_iter().filter(|e| e.kind == crate::library::Kind::Job).collect();
                            match jobs.iter().find(|e| e.name == n).or_else(|| jobs.iter().find(|e| e.name.eq_ignore_ascii_case(&n))) {
                                Some(e) => e.id,
                                None => return Err(format!("no library job is called '{n}' — see library mode=list")),
                            }
                        }
                        _ => return Err("library open needs the job's 'id' or 'name' (library mode=list)".into()),
                    };
                    // The job as saved now, linked to its record and revision
                    // (LB10). Its stock booking stays the record's: this file
                    // leaves the stock alone (stock-book refuses it).
                    let mut j = lib.load_job(id)?;
                    j.booked = false;
                    let saved = j.saved.as_ref().map(|s| format!(", saved by {} at {}", s.by.label(), ctx.clock.stamp(s.at))).unwrap_or_default();
                    // Reading a job in use is allowed; saving over it isn't (CU12).
                    let busy = match crate::engine::inuse::in_use(&lib, id) {
                        Ok(Some(u)) => format!(" {} Saving over it is refused until it's free (library mode=list says in_use); save-job copy=true saves this job file as a new job.", u.said(&ctx.clock)),
                        _ => String::new(),
                    };
                    let m = format!("Opened library job '{}' ({}){saved}. library mode=save-job saves over it.{busy}", j.name, crate::library::auto_ref(crate::library::Kind::Job, id));
                    *job = j;
                    Ok(Outcome::msg(m))
                }
                "add" => {
                    let id = match (a.int("id")?, a.str("name")) {
                        (Some(id), _) => id as u64,
                        (None, Some(n)) => lib.find_product(&n).ok_or_else(|| format!("no library product is called '{n}'"))?,
                        _ => return Err("library add needs 'id' or 'name'".into()),
                    };
                    let pf = lib.load_product(id)?;
                    // Saved back, it goes over that part as read here (LB7). A
                    // part of that name already in the job keeps what it read.
                    if !job.parts.iter().any(|q| q.name == pf.part.name) {
                        job.library_parts.insert(pf.part.name.clone(), crate::library::Record { id, revision: pf.revision });
                    }
                    let mut p = pf.part;
                    p.color = next_color(&job.parts);
                    if let Some(q) = a.int("qty")? {
                        p.qty = q.max(0) as u32;
                    }
                    let mut note = String::new();
                    if pf.material != job.spec.material || (pf.thickness - job.spec.thickness).abs() > 1e-6 {
                        note = format!(" (stored for {} {} mm; the job is {} {} mm)", cnc::MATERIALS.get(pf.material).copied().unwrap_or("?"), pf.thickness, cnc::MATERIALS.get(job.spec.material).copied().unwrap_or("?"), job.spec.thickness);
                    }
                    let m = format!("Added {} from the library as part {}{note}.", p.name, job.parts.len() + 1);
                    job.parts.push(p);
                    job.stale = true;
                    Ok(Outcome::msg(m))
                }
                "save-part" => {
                    let t = parts_sel(&job.parts, a.get("parts"), true)?;
                    // A new part goes into the part folder `folder` names, else
                    // the one named like the job's folder (its place in the
                    // Library now, else its week), made if missing; saving
                    // over a part never moves it (JX19).
                    let into = match a.str("folder") {
                        Some(f) => crate::folders::PartsFolder::Path(f),
                        None => crate::folders::PartsFolder::LikeJob { record: job.library.map(|r| r.id), folder: None, week: job.week.clone() },
                    };
                    let folder = a.str("folder").unwrap_or_else(|| job.week.clone());
                    // Two parts of one reference would save over each other.
                    for (k, &i) in t.iter().enumerate() {
                        if let Some(&j) = t[..k].iter().find(|&&j| job.parts[j].name == job.parts[i].name) {
                            return Err(format!("Parts {} and {} are both '{}'. Nothing was saved. Give one a new reference (edit-part name=…) or take it out (remove-part), then save-part again.", j + 1, i + 1, job.parts[i].name));
                        }
                    }
                    // Over each library part as this job read it, or a new
                    // one (LB7); every part is checked before any is saved.
                    let to: Vec<SaveTo> = t
                        .iter()
                        .map(|&i| {
                            let name = &job.parts[i].name;
                            lib.find_product(name).map_or(SaveTo::New, |id| job.library_parts.get(name).filter(|r| r.id == id).map_or(SaveTo::Unread(id), |r| SaveTo::Read(*r)))
                        })
                        .collect();
                    // To work on theirs, this job's parts of that reference go
                    // first: one still in the job keeps what it read (LB7).
                    let parts = &job.parts;
                    let refused = |c: crate::library::Changed, read: bool, i: usize| {
                        let name = &parts[i].name;
                        let out: Vec<String> = (0..parts.len()).rev().filter(|&k| parts[k].name == *name).map(|k| format!("remove-part part={}", k + 1)).collect();
                        format!(
                            "Library part '{}' ({}) was saved by {}{}. Nothing was saved. To work on theirs, take this job's '{name}' out ({}) and add theirs (library mode=add id={}); to keep yours, give the part a new reference (edit-part name=…) and save-part again.",
                            c.name,
                            crate::library::auto_ref(crate::library::Kind::Product, c.id),
                            c.saved_by_stamp(&ctx.clock),
                            if read { " since this job added it" } else { ", and this job's part of that name didn't come from it" },
                            out.join(", then "),
                            c.id
                        )
                    };
                    for (&i, &to) in t.iter().zip(&to) {
                        if let Err(c) = lib.check(crate::library::Kind::Product, to) {
                            return Err(refused(c, matches!(to, SaveTo::Read(_)), i));
                        }
                    }
                    let mut ids = vec![];
                    for (&i, &to) in t.iter().zip(&to) {
                        match lib.save_product_in(&job.parts[i], &folder, job.spec.material, job.spec.thickness, to, &into) {
                            Ok(rec) => {
                                job.library_parts.insert(job.parts[i].name.clone(), rec);
                                ids.push(rec.id);
                            }
                            Err(Refused::Changed(c)) => return Err(refused(c, matches!(to, SaveTo::Read(_)), i)),
                            Err(r) => return Err(r.words(&ctx.clock)),
                        }
                    }
                    // Where each is now in the part folders. The parts are
                    // saved by now: a part tree that can't be read only
                    // leaves the place out, and the job keeps what it saved.
                    let (places, at) = match crate::folders::load(&lib, crate::library::Kind::Product) {
                        Ok(tree) => {
                            let places: Vec<String> = ids.iter().map(|&id| tree.path(tree.folder_of(id))).collect();
                            let mut shown: Vec<String> = places.iter().map(|p| if p.is_empty() { "the top of the part folders".to_string() } else { format!("part folder '{p}'") }).collect();
                            shown.dedup();
                            (json!(places), format!(", in {}", shown.join(", ")))
                        }
                        Err(e) => (Value::Null, format!(" (its part folder can't be shown: {e})")),
                    };
                    Ok(Outcome { message: format!("Saved {} into the library{at}.", who(job, &t)), data: json!({ "ids": ids, "folders": places }), changed: true, files: vec![] })
                }
                "save-job" => {
                    // Over the job's own record as this file read it (LB10),
                    // or, as a copy, a new job under a free name that holds
                    // none of the original's stock (LB9).
                    let copy = a.bool("copy")?.unwrap_or(false);
                    let mut folder = None;
                    if let Some(from) = job.library.filter(|_| copy) {
                        // Filed in the folder of the job it was copied from
                        // (LB9, JX15); that job gone, by its folder name.
                        if lib.entries().iter().any(|e| e.kind == crate::library::Kind::Job && e.id == from.id) {
                            folder = crate::folders::load(&lib, crate::library::Kind::Job).ok().map(|t| t.folder_of(from.id));
                        }
                        job.name = lib.free_name(crate::library::Kind::Job, &job.name, |_| false);
                        job.library = None;
                        stockflow::forget_booking(&mut job.formats, &mut job.held);
                        job.booked = false;
                        if let Some(e) = job.erp.as_mut() {
                            e.work_order = None;
                            e.sent = None;
                        }
                    }
                    let to = job.library.filter(|_| !copy).map_or(SaveTo::New, SaveTo::Read);
                    // A name another job in the Library holds is refused (JX16);
                    // checked again under the write lock as it's saved.
                    lib.check_job_name(&job.name, to.id()).map_err(|e| format!("{e} (rename name=…), then save-job again. Nothing was saved"))?;
                    match lib.save_job_in(job, to, folder) {
                        Ok(rec) => {
                            job.library = Some(rec);
                            let mut m = if to == SaveTo::New { format!("Saved job '{}' into the library as record {}.", job.name, rec.id) } else { format!("Saved job '{}' over its library record {}.", job.name, rec.id) };
                            // A booked job file hands its booking to the record
                            // and stops following the stock, as one opened from
                            // the Library does (LB10): following it still, a
                            // later save refused because someone saved the job
                            // since would leave its changes taken twice.
                            if job.booked {
                                job.booked = false;
                                m.push_str(" The record keeps the stock this job file booked; from now on its sheets are booked when someone saves it in the window, so this job file no longer changes the stock.");
                            }
                            Ok(Outcome { message: m, data: json!({ "id": rec.id }), changed: true, files: vec![] })
                        }
                        Err(Refused::Changed(c)) => Err(format!(
                            "Library job '{}' ({}) was saved by {} since this job file read it. Nothing was saved. To work on theirs, read it again with library mode=open id={} (this file's changes are dropped); to keep yours, save it as a new job with library mode=save-job copy=true.",
                            c.name,
                            crate::library::auto_ref(crate::library::Kind::Job, c.id),
                            c.saved_by_stamp(&ctx.clock),
                            c.id
                        )),
                        // Someone has the job in use: agents never change it
                        // then, and never hold it themselves (CU12).
                        Err(Refused::InUse(u)) => Err(crate::engine::inuse::agent_refusal(&u, &ctx.clock, "Nothing was saved. To keep this job file's changes now, save it as a new job with library mode=save-job copy=true; library mode=list says which jobs are in use (in_use).")),
                        Err(Refused::Failed(e)) => Err(e),
                    }
                }
                m => Err(format!("library mode '{m}'?")),
            }
        }
        "set-sheet" => {
            let mut spec = job.spec.clone();
            set_spec_fields(&mut spec, &a, ctx)?;
            // "Use machine values" (MTL9): only the fields not given here.
            let spacing = if a.bool("machine_spacing")? == Some(true) {
                Some(use_machine_spacing(&mut spec, a.num("gap")?.is_none(), a.num("margin")?.is_none())?)
            } else {
                None
            };
            let mat_change = spec.material != job.spec.material || (spec.thickness - job.spec.thickness).abs() > 1e-9;
            let nested = !job.sheets.is_empty();
            let mut m = format!("Sheet: {} × {} mm, {} {} mm, {}, {}, gap {} mm, margin {} mm.", spec.w, spec.h, cnc::MATERIALS.get(spec.material).copied().unwrap_or("?"), spec.thickness, job_gas(&spec, &ctx.blm).label(), spec.nest_level().name, spec.gap, spec.margin);
            if let Some(s) = spacing {
                m = format!("{m} {s}");
            }
            if nested && mat_change {
                // The original asks once: keep the nestings, or remove them and free the sheets (PT4).
                match a.str("nestings").as_deref() {
                    Some("remove") => {
                        m.push_str(&format!(" Removed {} nested sheet(s); nest again.", job.sheets.len()));
                        job.clear_nestings();
                    }
                    Some("keep") => m.push_str(" Kept the nested sheets; they're marked changed."),
                    _ => return Err(format!("{} sheet(s) are nested in the old material/thickness. Say nestings=keep or nestings=remove.", job.sheets.len())),
                }
            }
            // GS14: a new gas keeps the nested sheets, unmarked; the section
            // and the technology record are picked again. Say what changed.
            if a.str("gas").is_some() {
                m.push(' ');
                m.push_str(&gas_change_note(&job.parts, &job.sheets, &job.formats, &job.manual_cuts, &job.spec, &spec, &ctx.blm));
            }
            let changed = spec.w != job.spec.w || spec.h != job.spec.h || spec.margin != job.spec.margin || spec.gap != job.spec.gap || mat_change || spec.machine != job.spec.machine;
            // The sheet as it was (the same gas, thickness … again): nothing
            // to save, back up or undo.
            let unchanged = spec == job.spec;
            job.spec = spec;
            if changed && !job.sheets.is_empty() {
                job.stale = true;
            }
            Ok(if unchanged { Outcome::read(m, Value::Null) } else { Outcome::msg(m) })
        }
        "nest" => {
            if let Some(l) = a.int("level")? {
                if !(1..=5).contains(&l) {
                    return Err("level is 1 to 5".into());
                }
                job.spec.level = l as u8;
            }
            if job.parts.is_empty() {
                return Err("The job has no parts — add some first.".into());
            }
            let group = a.bool("group")?.unwrap_or_else(|| crate::ct1::active().group_parts);
            // The stock as the window's Do All options say (stock=only: no
            // fallback to the job's sheet; off: no stock at all).
            let mode = a.str("stock").unwrap_or_else(|| "auto".into());
            let mut st = ctx.cfg.stock.clone();
            if mode == "only" {
                st.use_stock = true;
                st.fallback_job_size = false;
            }
            let stock = (mode != "off").then(|| stockflow::StockUse { st: &st, ..ctx.stock_use(job) });
            let r = nest_all(&job.parts, &job.spec, group, stock.as_ref());
            let grouped = group && nest::part_groups(&job.parts).len() > 1;
            let unreachable = stock.as_ref().filter(|_| !grouped).and_then(stockflow::stock_unreachable_note).unwrap_or_default();
            drop(stock);
            job.sheets = r.sheets;
            job.formats = r.formats;
            job.method = r.method;
            job.cut_lines.clear();
            job.manual_cuts.clear();
            job.stale = false;
            job.tidy();
            if ctx.cfg.stock.lines_after_nesting {
                // "Generate remnants after nesting": the automatic remnant line on every sheet.
                for k in 0..job.sheets.len() {
                    if let Some(lines) = auto_remnant_lines(&job.parts, &job.sheets[k], &job.spec_at(k), &ctx.cfg.stock.line) {
                        job.cuts_mut(k).extend(lines);
                    }
                }
            }
            let need: u32 = job.parts.iter().map(|p| p.qty).sum();
            let placed: usize = job.sheets.iter().map(|s| s.len()).sum();
            let from_stock = job.formats.iter().filter(|f| !f.stock_ref.is_empty()).count();
            let on_job_sheet = job.sheets.len() - from_stock;
            let from = if from_stock > 0 { format!(" ({from_stock} from stock{})", if on_job_sheet > 0 { format!(", {on_job_sheet} the job's sheet size") } else { String::new() }) } else { String::new() };
            let m = if placed as u32 == need {
                format!("Nesting done: {placed} parts on {} sheet(s){from} — level {}, best of {} methods.", job.sheets.len(), job.spec.nest_level().name, r.tried)
            } else if from_stock > 0 && on_job_sheet == 0 {
                format!("{placed} of {need} parts nested on {} sheet(s){from} — the stock ran out or the rest don't fit its sheets; add sheets to stock, or nest with stock=auto and \"nest on the job's sheet size when stock runs out\" on.", job.sheets.len())
            } else {
                format!("{placed} of {need} parts nested on {} sheet(s){from} — some are larger than the sheet.", job.sheets.len())
            };
            let usage: Vec<Value> = job.sheets.iter().enumerate().map(|(k, s)| { let f = job.format(k); json!({"sheet": k + 1, "parts": s.len(), "usage_pct": r1(sheet_usage(&job.parts, s, &job.spec, &f).0), "stock_ref": (!f.stock_ref.is_empty()).then(|| f.stock_ref.clone())}) }).collect();
            let mut o = Outcome::msg(m + &unreachable);
            o.data = json!({ "sheets": usage, "method": job.method });
            Ok(o)
        }
        "clear-sheets" => {
            job.clear_nestings();
            Ok(Outcome::msg("Sheets cleared."))
        }
        "sheet-add" => {
            let f = sheet_source(job, ctx, &a, None)?;
            let what = describe_format(&f);
            let k = job.push_sheet(f);
            Ok(Outcome::msg(format!("Sheet {} added: {what}. Place parts on it with place sheet={}.", k + 1, k + 1)))
        }
        "sheet-change" => {
            let s = sheet_idx(job, &a)?;
            let f = if a.bool("job_sheet")?.unwrap_or(false) { SheetFormat::rect(job.spec.w, job.spec.h) } else { sheet_source(job, ctx, &a, Some(s))? };
            if let Some(i) = job.sheets[s].iter().position(|p| !part_fits_format(&job.parts, &job.spec, &f, p)) {
                return Err(format!("#{} '{}' would be outside that sheet.", i + 1, job.parts[job.sheets[s][i].part].name));
            }
            let what = describe_format(&f);
            let had_cut = job.cuts(s).iter().any(|l| l.kind == CutKind::Remnant);
            if had_cut {
                job.cuts_mut(s).retain(|l| l.kind != CutKind::Remnant);
            }
            job.normalise_formats();
            job.formats[s] = f;
            job.sheet_changed(s);
            Ok(Outcome::msg(format!("Sheet {} is now {what}{}.", s + 1, if had_cut { " — its remnant line was removed, add a new one" } else { "" })))
        }
        "sheet-delete" => {
            let s = sheet_idx(job, &a)?;
            let n = job.sheets[s].len();
            job.remove_sheet(s);
            Ok(Outcome::msg(format!("Sheet {} deleted — {n} part(s) are no longer nested.", s + 1)))
        }
        "stock-book" => {
            reach_library(ctx, "Nothing was booked or given back, and the job is as it was")?;
            let file = Stock::file(&ctx.library_dir);
            let mode = a.need_str("mode")?;
            // A job file that is a library job (opened from the Library or
            // saved into it) leaves the stock to the window, which books the
            // job's sheets when someone saves it there (LB10). Booked here as
            // well, a save refused because someone saved it since would leave
            // them taken twice.
            if let (Some(rec), "book" | "release") = (job.library, mode.as_str()) {
                return Err(format!(
                    "Job file '{}' is library job {} (opened from the Library or saved into it): its sheets are booked when someone saves it in the window, so stock-book mode={mode} is refused here. Nothing was changed in the stock. To take its sheets from stock, open the job in the window and save it there; stock-book mode=status shows what that would take.",
                    job.name,
                    crate::library::auto_ref(crate::library::Kind::Job, rec.id)
                ));
            }
            match mode.as_str() {
                "status" => {
                    let st = ctx.stock()?;
                    job.normalise_formats();
                    let used = stockflow::sheets_used(&job.formats, job.sheets.len());
                    let missing: Vec<&String> = used.keys().filter(|r| st.get(r).is_none()).collect();
                    let rows = stockflow::remnant_rows(&ctx.library_dir, &job.stock_sheets(&ctx.cfg), &job.formats, &ctx.cfg.stock, true);
                    let (_, kept_used) = stockflow::pending_remnants(&st, &job.stock_sheets(&ctx.cfg), &job.formats, &ctx.cfg.stock, true);
                    let v = json!({
                        "booked": job.booked,
                        "held": job.held,
                        "would_take": used,
                        "remnants_stored": job.formats.iter().flat_map(|f| f.made.iter()).map(|r| match st.get(r) { Some(it) => json!({"ref": r, "w": r1(it.length), "h": r1(it.width), "area_m2": (it.area_m2() * 100.0).round() / 100.0, "qty": it.qty}), None => json!({"ref": r, "in_stock": false}) }).collect::<Vec<_>>(),
                        "would_store_remnants": rows.iter().map(|r| json!({"sheet": r.sheet + 1, "piece": r.piece + 1, "ref": r.reference, "w": r1(r.w), "h": r1(r.h), "area_m2": (r.area_m2 * 100.0).round() / 100.0})).collect::<Vec<_>>(),
                        "changed_but_kept_in_use": kept_used.iter().map(|k| k + 1).collect::<Vec<_>>(),
                        "sheets_not_in_stock": missing,
                    });
                    Ok(Outcome::read(if job.booked { "The job is booked against the stock." } else { "The job is not booked against the stock yet (stock-book mode=book)." }, v))
                }
                "book" => {
                    // Own references for the new remnants, in the order `status` lists them.
                    let names: Option<BTreeMap<(usize, usize), String>> = match a.str("refs") {
                        None => None,
                        Some(list) => {
                            let rows = stockflow::remnant_rows(&ctx.library_dir, &job.stock_sheets(&ctx.cfg), &job.formats, &ctx.cfg.stock, true);
                            let refs: Vec<String> = list.split(',').map(|r| r.trim().to_string()).collect();
                            if refs.len() != rows.len() {
                                return Err(format!("refs: {} given, but booking stores {} new remnant(s) — see stock-book mode=status", refs.len(), rows.len()));
                            }
                            let st = ctx.stock()?;
                            for (k, r) in refs.iter().enumerate() {
                                if r.is_empty() || r.chars().count() > crate::stock::MAX_REF {
                                    return Err(format!("refs: '{r}' must be 1 to {} characters", crate::stock::MAX_REF));
                                }
                                if st.get(r).is_some() || refs[..k].contains(r) {
                                    return Err(format!("refs: '{r}' is already taken"));
                                }
                                if st.reserved(r) {
                                    return Err(format!("refs: '{r}' was a remnant that has since been replaced — it is kept for undo; pick another reference"));
                                }
                            }
                            Some(rows.iter().zip(refs).map(|(row, r)| ((row.sheet, row.piece), r)).collect())
                        }
                    };
                    let note = job.book_stock_named(ctx, true, names.as_ref());
                    // Its sheets couldn't be taken (another PC took one, SV10):
                    // said so, not "Booked".
                    let booked = stockflow::sheets_used(&job.formats, job.sheets.len()) == job.held;
                    let mut o = Outcome::msg(match (note.is_empty(), booked) {
                        (true, _) => "Booked: the stock already matches the job.".to_string(),
                        (false, true) => format!("Booked: {note}."),
                        (false, false) => format!("Not booked: {note}."),
                    });
                    o.files.push(file);
                    Ok(o)
                }
                "release" => {
                    job.normalise_formats();
                    let (back, removed, kept) = stockflow::release(&ctx.library_at(), &mut job.formats, &mut job.held)?;
                    job.booked = false;
                    let mut m = format!("Released: {back} sheet(s) given back to stock, {removed} remnant(s) taken out again.");
                    if !kept.is_empty() {
                        m.push_str(&format!(" Left in stock (already used elsewhere): {}.", kept.join(", ")));
                    }
                    m.push_str(" The job is no longer booked.");
                    let mut o = Outcome::msg(m);
                    o.files.push(file);
                    Ok(o)
                }
                m => Err(format!("stock-book mode '{m}'?")),
            }
        }
        "compact" => {
            let s = sheet_idx(job, &a)?;
            let (f, sp) = (job.format(s), job.spec_at(s));
            if f.outline.is_some() {
                return Err(COMPACT_REMNANT.into());
            }
            let before = sheet_usage(&job.parts, &job.sheets[s], &job.spec, &f).2;
            nest::compact_contours(&job.parts, &mut job.sheets[s], &sp);
            job.sheet_changed(s);
            let after = sheet_usage(&job.parts, &job.sheets[s], &job.spec, &f).2;
            Ok(Outcome::msg(format!("Compacted sheet {} — remnant now starts at X {after:.0} mm ({:.0} mm gained).", s + 1, (before - after).max(0.0))))
        }
        "renest-sheet" => {
            let s = sheet_idx(job, &a)?;
            let f = job.format(s);
            let group = a.bool("group")?.unwrap_or_else(|| crate::ct1::active().group_parts);
            let placed = renest_sheet(&job.parts, &job.spec, &job.sheets[s], &f, group)?;
            let before = sheet_usage(&job.parts, &job.sheets[s], &job.spec, &f).2;
            job.sheets[s] = placed;
            job.normalise_formats();
            stockflow::remnants_outdated(&mut job.formats[s]);
            job.cuts_mut(s).clear();
            job.sheet_changed(s);
            if ctx.cfg.stock.lines_after_nesting {
                if let Some(lines) = auto_remnant_lines(&job.parts, &job.sheets[s], &job.spec_at(s), &ctx.cfg.stock.line) {
                    job.cuts_mut(s).extend(lines);
                }
            }
            let after = sheet_usage(&job.parts, &job.sheets[s], &job.spec, &f);
            let mut o = Outcome::msg(format!("Sheet {} nested again: its {} parts, usage {:.1} %, remnant from X {:.0} mm (was {before:.0} mm). The other sheets are unchanged.", s + 1, job.sheets[s].len(), after.0, after.2));
            o.data = json!({ "sheet": s + 1, "parts": job.sheets[s].len(), "usage_pct": r1(after.0), "remnant_from_x": r1(after.2) });
            Ok(o)
        }
        "place" => {
            let pi = one_part(job, &a)?;
            let turns = match a.int("angle")?.unwrap_or(0).rem_euclid(360) {
                0 => 0u8,
                90 => 1,
                180 => 2,
                270 => 3,
                x => return Err(format!("angle {x}: use 0, 90, 180 or 270")),
            };
            let mirror = a.bool("mirror")?.unwrap_or(false);
            if !nest::turn_allowed(&job.parts[pi], turns, mirror) {
                return Err(format!("'{}' may not be placed at {}°{} (its rotation settings).", job.parts[pi].name, turns as u16 * 90, if mirror { " mirrored" } else { "" }));
            }
            let s = match a.str("sheet").as_deref().map(str::trim) {
                Some("new") => job.push_sheet(SheetFormat::rect(job.spec.w, job.spec.h)),
                None if job.sheets.is_empty() => job.push_sheet(SheetFormat::rect(job.spec.w, job.spec.h)),
                None => 0,
                Some(t) => {
                    let k: usize = t.parse().map_err(|_| format!("sheet '{t}': a number or \"new\""))?;
                    if k < 1 || k > job.sheets.len() {
                        return Err(format!("there is no sheet {k} (the job has {}; use sheet=new)", job.sheets.len()));
                    }
                    k - 1
                }
            };
            let c = match (a.num("x")?, a.num("y")?) {
                (Some(x), Some(y)) => {
                    let (w, h) = footprint(&job.parts[pi], turns);
                    let c = Placed { part: pi, x, y, w, h, turns, mirror, spin: Default::default() };
                    if !placement_ok(&job.parts, &job.sheets[s], &job.spec, &job.format(s), None, &c, true) {
                        job.tidy();
                        return Err(format!("There is no room for '{}' at X {x} Y {y} on sheet {} (it would overlap a part or leave the margin).", job.parts[pi].name, s + 1));
                    }
                    c
                }
                (None, None) => match free_spot(&job.parts, &job.sheets[s], &job.spec, &job.format(s), pi, turns, mirror, true) {
                    Some(c) => c,
                    None => {
                        job.tidy();
                        return Err(format!("No free spot for '{}' on sheet {}.", job.parts[pi].name, s + 1));
                    }
                },
                _ => return Err("give both x and y, or neither".into()),
            };
            let extra = placed_count(&job.sheets, pi) as u32 >= job.parts[pi].qty;
            if extra {
                job.parts[pi].qty += 1;
            }
            job.sheets[s].push(c);
            job.sheet_changed(s);
            let n = job.sheets[s].len();
            Ok(Outcome::msg(format!(
                "Placed '{}' as #{n} on sheet {} at X {:.1} Y {:.1}{}.",
                job.parts[pi].name,
                s + 1,
                c.x,
                c.y,
                if extra { format!(" (quantity now {})", job.parts[pi].qty) } else { String::new() }
            )))
        }
        "move" => {
            let s = sheet_idx(job, &a)?;
            let sel = items_idx(job, s, &a)?;
            let b = arrange::group_box(&job.sheets[s], &sel);
            let d = [
                a.num("x")?.map_or(0.0, |x| x - b[0]) + a.num("dx")?.unwrap_or(0.0),
                a.num("y")?.map_or(0.0, |y| y - b[1]) + a.num("dy")?.unwrap_or(0.0),
            ];
            if !arrange::group_ok(&job.parts, &job.sheets[s], &sel, d, &job.spec.for_format(&job.format(s)), &job.format(s), true) {
                return Err(format!("{} can't go to X {:.1} Y {:.1}: it would overlap a part or leave the margin.", names(&sel), b[0] + d[0], b[1] + d[1]));
            }
            for &i in &sel {
                job.sheets[s][i].x += d[0];
                job.sheets[s][i].y += d[1];
            }
            job.sheet_changed(s);
            let c = job.sheets[s][sel[0]];
            Ok(Outcome::msg(if sel.len() == 1 {
                format!("Moved #{} '{}' to X {:.1} Y {:.1}.", sel[0] + 1, job.parts[c.part].name, c.x, c.y)
            } else {
                format!("Moved {} to X {:.1} Y {:.1} (lower-left of their box).", names(&sel), b[0] + d[0], b[1] + d[1])
            }))
        }
        "slide" => {
            let s = sheet_idx(job, &a)?;
            let sel = items_idx(job, s, &a)?;
            let dir = arrange::Dir::parse(&a.need_str("dir")?).ok_or("dir: left, right, up or down")?;
            let by = a.num("by")?;
            if by.is_some_and(|b| b <= 0.0) {
                return Err("by: give a distance above 0 (or leave it out to slide to the end).".into());
            }
            let f = job.format(s);
            let t = arrange::slide(&job.parts, &mut job.sheets[s], &sel, dir, &job.spec, &f, by.unwrap_or(f64::INFINITY), true);
            if t <= arrange::EPS {
                return Err(format!("{} can't move {}: it already sits at the part gap from the next part or at the margin.", names(&sel), dir.name()));
            }
            job.sheet_changed(s);
            let short = by.is_some_and(|b| t + 1e-6 < b);
            Ok(Outcome::msg(format!(
                "Slid {} {} {t:.2} mm{}.",
                names(&sel),
                dir.name(),
                if by.is_none() || short { " — it now meets the next part at the gap, or the margin" } else { "" }
            )))
        }
        "rotate" => {
            let s = sheet_idx(job, &a)?;
            let i = item_idx(job, s, &a)?;
            let old = job.sheets[s][i];
            // QR3: back = the previous allowed turn, as the quick ring's Turn back.
            let c = step_turn(&job.parts[old.part], old, a.bool("back")?.unwrap_or(false))?;
            if !placement_ok(&job.parts, &job.sheets[s], &job.spec, &job.format(s), Some(i), &c, true) {
                return Err("No room to rotate the part here.".into());
            }
            job.sheets[s][i] = c;
            job.sheet_changed(s);
            Ok(Outcome::msg(format!("Turned #{} to {}.", i + 1, nest::angle_text(c.angle()))))
        }
        "turn" | "mirror" => {
            let s = sheet_idx(job, &a)?;
            let sel = items_idx(job, s, &a)?;
            let f = job.format(s);
            let (new, what) = if name == "turn" {
                // HM14: by any angle, or to an angle for the first part.
                let deg = match (a.num("angle")?, a.num("to")?) {
                    (Some(_), Some(_)) => return Err("Give angle (turn by) or to (turn to an angle), not both.".into()),
                    (None, Some(t)) => t - job.sheets[s][sel[0]].angle(),
                    (by, None) => by.unwrap_or(90.0),
                };
                if !deg.is_finite() {
                    return Err("angle: give a number of degrees.".into());
                }
                (arrange::turn_by(&job.parts, &job.sheets[s], &sel, deg, &job.spec, &f, true)?, format!("Turned {} by {}", names(&sel), nest::angle_text(deg)))
            } else {
                let vertical = match a.str("axis").as_deref().map(str::trim) {
                    None | Some("x") | Some("X") => true,
                    Some("y") | Some("Y") => false,
                    Some(v) => return Err(format!("axis: '{v}' — x (left↔right) or y (top↔bottom).")),
                };
                (arrange::mirror(&job.parts, &job.sheets[s], &sel, vertical, &job.spec, &f, true)?, format!("Mirrored {}", names(&sel)))
            };
            for (k, &i) in sel.iter().enumerate() {
                job.sheets[s][i] = new[k];
            }
            job.sheet_changed(s);
            let b = arrange::group_box(&job.sheets[s], &sel);
            Ok(Outcome::msg(format!("{what}; their box now starts at X {:.1} Y {:.1}.", b[0], b[1])))
        }
        "copy" => {
            let s = sheet_idx(job, &a)?;
            let sel = items_idx(job, s, &a)?;
            let f = job.format(s);
            let d = arrange::copy_spot(&job.sheets[s], &sel, &job.spec, &f).ok_or("No free spot on this sheet for a copy.")?;
            let first = job.sheets[s].len() + 1;
            let copies = arrange::copies(&job.sheets[s], &sel, d);
            let extras = pattern::put(&mut job.parts, &mut job.sheets, s, &copies);
            job.sheet_changed(s);
            let c = job.sheets[s][first - 1];
            let grew = if extras > 0 { format!("; {extras} beyond the order, which grew to match") } else { "; from the pieces still to place".to_string() };
            Ok(Outcome::msg(if sel.len() == 1 {
                format!("Copied {} to X {:.0} Y {:.0} as #{first} (quantity {}{grew}).", job.parts[c.part].name, c.x, c.y, job.parts[c.part].qty)
            } else {
                format!("Copied {} as #{first}–#{}{grew}.", names(&sel), job.sheets[s].len())
            }))
        }
        "repeat" => {
            let s = sheet_idx(job, &a)?;
            let sel = items_idx(job, s, &a)?;
            let over = a.bool("extras")?.map(|b| if b { pattern::Over::Extras } else { pattern::Over::OnlyLeft });
            let times = match a.int("times")? {
                Some(t) if t < 1 => return Err(format!("times: {t} — at least 1 copy.")),
                t => t.map(|t| t as u32),
            };
            let pat: Vec<Placed> = sel.iter().map(|&i| job.sheets[s][i]).collect();
            let u = pattern::uses(&job.parts, &job.sheets, &pat);
            let whole = pattern::copies_left(&u);
            // How many copies: as asked, or what the pieces left make — whole
            // copies, or with an answer for a short one, until none is left (HM19, HM20).
            let all = u.iter().map(|x| x.left.div_ceil(x.per)).max().unwrap_or(0);
            let want = times.unwrap_or(if over.is_some() { all } else { whole });
            if over.is_none() && whole == 0 {
                let q = pattern::question(&job.parts, &u).replacen("This copy", &format!("A copy of {}", names(&sel)), 1);
                return Err(format!("{q} Say extras=true to add the missing pieces to the order, or extras=false to place only the pieces left."));
            }
            if over.is_none() && want > whole {
                return Err(format!(
                    "Copy {} of {} would be short: the pieces left make {whole} whole cop{}. Say times={whole}, extras=true to add the missing pieces to the order, or extras=false to place only the pieces left.",
                    whole + 1,
                    names(&sel),
                    if whole == 1 { "y" } else { "ies" }
                ));
            }
            if want == 0 {
                return Err(format!("Nothing of {} is left to place. To add copies beyond the order say times=N with extras=true.", names(&sel)));
            }
            let f = job.format(s);
            let first = job.sheets[s].len() + 1;
            let (mut done, mut placed, mut extras, mut full) = (0u32, 0usize, 0u32, false);
            while done < want {
                let u = pattern::uses(&job.parts, &job.sheets, &pat);
                let pick = pattern::pick(&u, &pat, 0, over).unwrap_or_default();
                if pick.is_empty() {
                    break;
                }
                let sub: Vec<usize> = pick.iter().map(|&k| sel[k]).collect();
                let Some(d) = arrange::copy_spot(&job.sheets[s], &sub, &job.spec, &f) else {
                    full = true;
                    break;
                };
                let pieces = arrange::copies(&job.sheets[s], &sub, d);
                placed += pieces.len();
                extras += pattern::put(&mut job.parts, &mut job.sheets, s, &pieces);
                done += 1;
            }
            if done == 0 {
                return Err(format!("No free spot on sheet {} for a copy of {}.", s + 1, names(&sel)));
            }
            job.sheet_changed(s);
            let u = pattern::uses(&job.parts, &job.sheets, &pat);
            let left: Vec<String> = u.iter().map(|x| format!("{} {}", x.left, job.parts[x.part].name)).collect();
            let mut msg = format!("Placed {done} cop{} of {} as #{first}–#{} ({placed} piece{}", if done == 1 { "y" } else { "ies" }, names(&sel), job.sheets[s].len(), if placed == 1 { "" } else { "s" });
            if extras > 0 {
                msg += &format!(", {extras} beyond the order — it grew");
            }
            msg += &format!("); still to place: {}.", left.join(", "));
            if full {
                msg += &format!(" Sheet {} has no room for more.", s + 1);
            }
            Ok(Outcome::msg(msg))
        }
        "unplace" => {
            let s = sheet_idx(job, &a)?;
            let i = item_idx(job, s, &a)?;
            let p = job.sheets[s].remove(i);
            job.sheet_changed(s);
            job.tidy();
            Ok(Outcome::msg(format!("Took #{} '{}' off sheet {}.", i + 1, job.parts[p.part].name, s + 1)))
        }
        "leads" => match a.need_str("mode")?.as_str() {
            "auto" => {
                let lead_type = lead_from(&a.str("type").unwrap_or_else(|| "machine".into()))?;
                let position = a.int("position")?.unwrap_or(1);
                if !(1..=9).contains(&position) {
                    return Err("position is 1 to 9 (keypad)".into());
                }
                edit_tech(job, &a, TechEdit::AutoLeads { lead_type, position: position as u8 }, "Automatic lead-ins")
            }
            "machine" => edit_tech(job, &a, TechEdit::MachineLeads(crate::ct1::for_spec(&job.spec)), "Machine lead-ins"),
            "delete" => edit_tech(job, &a, TechEdit::DeleteLeads, "Lead-ins removed"),
            m => Err(format!("leads mode '{m}'?")),
        },
        "lead-at" => {
            let i = one_part(job, &a)?;
            let e = a.need_int("element")?;
            if e < 1 {
                return Err("elements are numbered from 1".into());
            }
            let pick = a.point("at")?;
            Ok(Outcome::msg(lead_on_element(&mut job.parts[i], e as usize - 1, pick)?))
        }
        "lead-edit" => {
            let i = one_part(job, &a)?;
            // part-info's contour numbers → the part's cut contours (CS1).
            let order = dxf::cut_order(&contours::list(&job.parts[i]));
            let ci = match a.int("contour")? {
                None => None,
                Some(k) => Some(order.iter().position(|&x| x as i64 == k - 1).ok_or_else(|| format!("Contour {k} isn't one the part cuts (contours {} are; see part-info).", order.iter().map(|x| (x + 1).to_string()).collect::<Vec<_>>().join(", ")))?),
            };
            let name = job.parts[i].name.clone();
            let same = a.bool("same")?.unwrap_or(false);
            match a.str("mode").as_deref().unwrap_or("set") {
                "reset" => {
                    let n = reset_lead_edits(&mut job.parts[i], ci, same);
                    Ok(Outcome::msg(match (ci, n) {
                        (_, 0) => format!("No lead of '{name}' was moved by hand{}.", ci.map_or(String::new(), |c| format!(" on contour {}", order[c] + 1))),
                        (Some(c), 1) => format!("Contour {}'s lead of '{name}' is back to its automatic place.", order[c] + 1),
                        (Some(c), n) => format!("Contour {}'s lead of '{name}' and {} more it shared are back to their automatic place.", order[c] + 1, n - 1),
                        (None, n) => format!("{n} lead(s) of '{name}' back to their automatic place."),
                    }))
                }
                "set" => {
                    let ci = ci.ok_or("Name the contour (see part-info).")?;
                    let (il, ia, ol, oa) = (a.num("in_length")?, a.num("in_angle")?, a.num("out_length")?, a.num("out_angle")?);
                    if il.is_none() && ia.is_none() && ol.is_none() && oa.is_none() {
                        if same {
                            return Ok(Outcome::msg(lead_to_same(&mut job.parts[i], ci)?));
                        }
                        return Err("Give in_length / in_angle for the lead-in, out_length / out_angle for the lead-out, same=true to put the lead it has on every hole of its shape (or mode=reset).".into());
                    }
                    let mut msg = vec![];
                    if il.is_some() || ia.is_some() {
                        msg.push(set_lead_end(&mut job.parts, i, &job.spec, ci, LeadEnd::In, il, ia, same)?);
                    }
                    if ol.is_some() || oa.is_some() {
                        msg.push(set_lead_end(&mut job.parts, i, &job.spec, ci, LeadEnd::Out, ol, oa, same)?);
                    }
                    Ok(Outcome::msg(msg.join(" ")))
                }
                m => Err(format!("lead-edit mode '{m}'? Use set or reset.")),
            }
        }
        "joints" => match a.need_str("mode")?.as_str() {
            "auto" => {
                let every = a.num("every")?;
                if every.is_some_and(|e| e <= 0.0) {
                    return Err("every must be more than 0 mm (use mode=delete to remove joints)".into());
                }
                // The machine's joint table for the job's material and thickness (MTL7).
                let mt = crate::ct1::for_spec(&job.spec);
                let mut o = edit_tech(job, &a, TechEdit::auto_joints(&mt, every), "Automatic micro-joints")?;
                o.message.push_str(" Each contour's count is kept within the machine's fewest/most for its size class.");
                Ok(o)
            }
            "delete" => edit_tech(job, &a, TechEdit::DeleteJoints, "Micro-joints removed"),
            m => Err(format!("joints mode '{m}'?")),
        },
        "machine-tech" => {
            let mut spec = job.spec.clone();
            if let Some(v) = a.str("material") {
                spec.material = material_from(&v, &ctx.cfg)?;
            }
            if let Some(v) = a.num("thickness")? {
                if v <= 0.0 {
                    return Err("thickness must be more than 0 mm".into());
                }
                spec.thickness = v;
            }
            if let Some(v) = a.str("gas") {
                spec.gas = Some(crate::gas::Gas::parse(&v)?);
            }
            let (msg, data) = machine_tech_report(&spec, &job.spec, &ctx.blm);
            Ok(Outcome::read(msg, data))
        }
        "compensation" => {
            let pick = |k: &str| -> Result<Option<Option<bool>>, String> {
                Ok(match a.str(k).as_deref().map(str::trim) {
                    None => None,
                    Some("on") | Some("true") => Some(Some(true)),
                    Some("off") | Some("false") => Some(Some(false)),
                    Some("machine") => Some(None),
                    Some(v) => return Err(format!("{k}: '{v}'? Use on, off or machine.")),
                })
            };
            let (control, system) = (pick("control")?, pick("system")?);
            let changed = control.is_some() || system.is_some();
            if let Some(v) = control {
                job.spec.kerf.control = v;
            }
            if let Some(v) = system {
                job.spec.kerf.system = v;
            }
            let (msg, data) = compensation_report(job, &a)?;
            Ok(if changed { Outcome { message: format!("Compensation set. {msg}"), data, changed: true, files: vec![] } } else { Outcome::read(msg, data) })
        }
        "joint-at" => {
            let i = one_part(job, &a)?;
            let elems = nest::elements_of(&job.parts[i].geometry());
            let mut picks = vec![];
            for t in a.need_str("elements")?.split(',').filter(|s| !s.trim().is_empty()) {
                let k: usize = t.trim().parse().map_err(|_| format!("elements: '{t}' is not a number"))?;
                let e = elems.get(k.wrapping_sub(1)).ok_or_else(|| format!("the part has {} element(s); there is no {k}", elems.len()))?;
                picks.push((e.contour, e.midpoint()));
            }
            let n = joints_on_elements(&mut job.parts[i], &picks);
            Ok(Outcome::msg(format!("{n} micro-joint(s) placed on '{}'.", job.parts[i].name)))
        }
        "loops" => {
            let on = a.bool("on")?.unwrap_or(true);
            let size = a.num("size")?;
            let mut o = edit_tech(job, &a, TechEdit::Loops(on), if on { "Loops added at sharp corners" } else { "Loops removed" })?;
            if let Some(sz) = size {
                for i in parts_sel(&job.parts, a.get("parts"), true)? {
                    if let Some(t) = job.parts[i].tech.as_mut() {
                        t.loop_size = sz.max(0.1);
                    }
                }
                o.message.push_str(&format!(" Loop size {sz} mm."));
            }
            Ok(o)
        }
        "cut-tech" => match a.need_str("mode")?.as_str() {
            "own" => edit_tech(job, &a, TechEdit::Own, "Own cut technology"),
            "delete" => edit_tech(job, &a, TechEdit::Delete, "Technology removed (automatic again)"),
            "ranges" => {
                let mut ranges: [nest::CutRange; 3] = Default::default();
                let spec = a.str("ranges").unwrap_or_default();
                for (k, t) in spec.split(',').filter(|s| !s.trim().is_empty()).enumerate() {
                    if k >= 3 {
                        return Err("ranges: at most 3".into());
                    }
                    let (span, q) = t.split_once(':').ok_or_else(|| format!("ranges: '{t}' is not from-to:quality"))?;
                    let (lo, hi) = span.split_once('-').ok_or_else(|| format!("ranges: '{span}' is not from-to"))?;
                    let num = |s: &str| s.trim().parse::<f64>().map_err(|_| format!("ranges: '{s}' is not a number"));
                    ranges[k] = nest::CutRange { on: true, min_r: num(lo)?, max_r: num(hi)?, quality: range_quality_from(q)? };
                }
                let t = parts_sel(&job.parts, a.get("parts"), true)?;
                for &i in &t {
                    let mut x = job.parts[i].tech.unwrap_or_default();
                    x.cut_ranges = ranges;
                    job.parts[i].tech = Some(x);
                }
                Ok(Outcome::msg(format!("Cut quality by size set for {}.", who(job, &t))))
            }
            m => Err(format!("cut-tech mode '{m}'?")),
        },
        "tech-set" => {
            let t = parts_sel(&job.parts, a.get("parts"), true)?;
            let lead_type = a.str("lead_type").map(|s| lead_from(&s)).transpose()?;
            let joint_type = a.str("joint_type").map(|s| joint_from(&s)).transpose()?;
            let quality = a.str("quality").map(|s| quality_from(&s)).transpose()?;
            let position = a.int("position")?;
            if position.is_some_and(|p| !(1..=9).contains(&p)) {
                return Err("position is 1 to 9 (keypad)".into());
            }
            let nn = |k: &str| -> Result<Option<f64>, String> {
                match a.num(k)? {
                    Some(v) if v < 0.0 => Err(format!("{k} can't be negative")),
                    v => Ok(v),
                }
            };
            let (li, lo, ov, jw, jl, ja, je, ls) = (nn("lead_in")?, nn("lead_out")?, nn("overlap")?, nn("joint_width")?, nn("joint_lead")?, a.num("joint_angle")?, nn("joint_every")?, nn("loop_size")?);
            let jlo = nn("joint_lead_out")?;
            let (mj, cs, lp) = (a.int("micro_joints")?, a.bool("corner_start")?, a.bool("loops")?);
            if mj.is_some_and(|m| m < 0) {
                return Err("micro_joints can't be negative".into());
            }
            for &i in &t {
                let mut x = job.parts[i].tech.unwrap_or_default();
                macro_rules! set {
                    ($f:ident, $v:expr) => {
                        if let Some(v) = $v {
                            x.$f = v;
                        }
                    };
                }
                set!(lead_in, li);
                set!(lead_out, lo);
                set!(lead_type, lead_type);
                set!(position, position.map(|p| p as u8));
                set!(corner_start, cs);
                set!(overlap, ov);
                set!(micro_joints, mj.map(|m| m as u32));
                set!(joint_width, jw);
                set!(joint_type, joint_type);
                set!(joint_lead, jl);
                set!(joint_angle, ja);
                set!(joint_every, je);
                set!(joint_lead_out, jlo.map(Some));
                set!(loops, lp);
                set!(loop_size, ls);
                set!(quality, quality);
                job.parts[i].tech = Some(x);
            }
            Ok(Outcome::msg(format!("Technology set for {}.", who(job, &t))))
        }
        "text" => {
            let t = parts_sel(&job.parts, a.get("parts"), true)?;
            match a.need_str("mode")?.as_str() {
                "add" => {
                    let txt = a.need_str("text")?;
                    if txt.trim().is_empty() {
                        return Err("the text is empty".into());
                    }
                    let h = a.num("height")?.unwrap_or(8.0).max(0.5);
                    let kind = text_kind_from(&a.str("kind").unwrap_or_else(|| "special".into()))?;
                    for &i in &t {
                        add_text(&mut job.parts[i], &txt, h, kind);
                    }
                    Ok(Outcome::msg(format!("Text '{txt}' added to {} as {}.", who(job, &t), kind.label().to_lowercase())))
                }
                "delete" => {
                    for &i in &t {
                        job.parts[i].texts.clear();
                    }
                    Ok(Outcome::msg(format!("Texts removed from {}.", who(job, &t))))
                }
                m => Err(format!("text mode '{m}'?")),
            }
        }
        "cut-order" => {
            let s = sheet_idx(job, &a)?;
            match a.need_str("mode")?.as_str() {
                "pick" => {
                    let (Some(x), Some(y)) = (a.num("x")?, a.num("y")?) else { return Err("pick needs x and y".into()) };
                    let tol = a.num("tol")?.unwrap_or(5.0);
                    let rev = a.bool("reverse")?.unwrap_or(false);
                    let (parts, sheet) = (job.parts.clone(), job.sheets[s].clone());
                    Ok(Outcome::msg(manual_pick(&parts, &sheet, job.manual_mut(s), [x, y], tol, rev)?))
                }
                "parts" => {
                    let (parts, sheet) = (job.parts.clone(), job.sheets[s].clone());
                    let done = sheetpick::picked(&parts, &sheet, job.manual_mut(s));
                    let got = match (a.str("items"), a.points("points")?, a.points("box")?) {
                        (Some(list), None, None) => sheetpick::listed(&done, &arrange::parse_items_in_order(&list, sheet.len())?),
                        (None, Some(path), None) if !path.is_empty() => sheetpick::sweep(&parts, &sheet, &done, &path, a.num("tol")?.unwrap_or(5.0)),
                        (None, None, Some(b)) if b.len() == 2 => {
                            // SW1: a box takes what lies wholly inside it unless told to take what it touches.
                            let take = if a.bool("touching")?.unwrap_or(false) { Take::Touching } else { Take::Inside };
                            sheetpick::boxed(&parts, &sheet, &job.spec_at(s), &done, &Window::new(b[0], b[1], take))
                        }
                        _ => return Err("parts takes whole parts by one of: items=\"3,1,2\" (placed parts in the order to cut them), points=\"x,y;x,y;…\" (a sweep: the parts it passes, in that order) or box=\"x,y;x,y\" (two corners: the parts wholly inside, or with touching=true every part it touches).".into()),
                    };
                    // One engine function for agents and the sheet's sweep and box (MM8, MM9).
                    let message = sheetpick::take(&parts, &sheet, job.manual_mut(s), &got)?;
                    let one = |v: &[usize]| v.iter().map(|k| k + 1).collect::<Vec<_>>();
                    Ok(Outcome { message, data: json!({"added": one(&got.add), "skipped": one(&got.skipped)}), changed: true, files: vec![] })
                }
                "undo" => sheetpick::undo(job.manual_mut(s)).map(Outcome::msg).ok_or_else(|| "No manual picks on this sheet.".to_string()),
                "clear" => {
                    job.manual_mut(s).clear();
                    Ok(Outcome::msg("Manual cuts cleared — the whole sheet is cut automatically."))
                }
                m => Err(format!("cut-order mode '{m}': give pick, parts, undo or clear.")),
            }
        }
        "remnant" => {
            let s = sheet_idx(job, &a)?;
            let sp = job.spec_at(s);
            let (parts, sheet) = (job.parts.clone(), job.sheets[s].clone());
            match a.need_str("mode")?.as_str() {
                "auto" => {
                    let ls = &ctx.cfg.stock.line;
                    let lines = crate::remline::auto_lines(&parts, &sheet, &sp, ls, crate::kerf::kerf_mm(&sp) / 2.0);
                    if lines.is_empty() {
                        return Err(format!("No remnant: less than the minimum width ({:.0} mm) is left past the parts.", ls.min_width));
                    }
                    set_remnant_lines(&parts, &sheet, job.cuts_mut(s), lines)?;
                }
                mode => {
                    let line = match mode {
                        "exact" => cutoff::exact_edge(&sheet, &sp, a.bool("round")?.unwrap_or(true)).ok_or("No remnant worth keeping between the parts and the sheet edge.")?,
                        "line" => match (a.num("x")?, a.num("y")?) {
                            (Some(x), None) => classify_path(vec![[x, 0.0], [x, sp.h]], &sp),
                            (None, Some(y)) => classify_path(vec![[0.0, y], [sp.w, y]], &sp),
                            _ => return Err("line needs x= (vertical) or y= (horizontal)".into()),
                        },
                        m => return Err(format!("remnant mode '{m}'?")),
                    };
                    add_cut_line(&parts, &sheet, job.cuts_mut(s), line)?;
                }
            }
            let m = match remnant_info(&job.parts, &job.sheets[s], &job.format(s), job.cuts(s), crate::kerf::kerf_mm(&job.spec_at(s))) {
                Some((w, h, ar)) => format!("Remnant line added on sheet {} — remnant about {w:.0} × {h:.0} mm ({:.2} m²), cut last.", s + 1, ar / 1e6),
                None => "Cut line added — it is cut after the parts.".into(),
            };
            Ok(Outcome::msg(m))
        }
        "cut-line" => {
            let s = sheet_idx(job, &a)?;
            if let Some(k) = a.int("delete")? {
                let list = job.cuts_mut(s);
                if k < 1 || k as usize > list.len() {
                    return Err(format!("sheet {} has {} cut line(s)", s + 1, list.len()));
                }
                list.remove(k as usize - 1);
                return Ok(Outcome::msg("Cut line deleted."));
            }
            let pts = a.points("points")?.ok_or("give the path's points (or delete=N)")?;
            if pts.len() < 2 {
                return Err("a cut path needs at least 2 points".into());
            }
            let line = classify_path(pts, &job.spec_at(s));
            let kind = line.kind;
            let (parts, sheet) = (job.parts.clone(), job.sheets[s].clone());
            add_cut_line(&parts, &sheet, job.cuts_mut(s), line)?;
            Ok(Outcome::msg(if kind == CutKind::Remnant { "Remnant line added (edge to edge) — cut last." } else { "Cut line added — it is cut after the parts." }))
        }
        "verify" => {
            let plans = job.plans();
            let ks: Vec<usize> = match a.str("sheet").as_deref().map(str::trim) {
                None | Some("all") => (0..job.sheets.len()).collect(),
                Some(_) => vec![sheet_idx(job, &a)?],
            };
            if ks.is_empty() {
                return Err("No sheets are nested yet — run nest first.".into());
            }
            let safe = match a.num("safe")? {
                Some(v) if v <= 0.0 => None,
                Some(v) => Some(v),
                None => Some(job.spec.gap),
            };
            let mut bad = 0;
            let res: Vec<Value> = ks
                .iter()
                .map(|&k| {
                    let issues = verify_sheet(&job.parts, &job.sheets[k], &job.spec_at(k), &plans[k], safe);
                    if !issues.is_empty() {
                        bad += 1;
                    }
                    json!({"sheet": k + 1, "correct": issues.is_empty(), "issues": issues.iter().map(|i| json!({"problem": crate::v44::verify_message(i.code), "detail": i.text})).collect::<Vec<_>>()})
                })
                .collect();
            Ok(Outcome::read(format!("Verify Machining: {} sheet(s) correct, {bad} incorrect.", ks.len() - bad), json!(res)))
        }
        "program" => {
            let s = sheet_idx(job, &a)?;
            let plans = job.plans();
            // JOB-41, MAC-46: the same stop as export.
            let stops = stop_issues(&job.parts, &job.sheets[s], &job.spec_at(s), &plans[s]);
            if !stops.is_empty() {
                return Err(stop_refusal(s + 1, &stops));
            }
            let text = if ctx.blm.on {
                let set = crate::blm::sheet_set(s + 1, &job.parts, &job.sheets[s], &job.spec_at(s), &plans[s], job.cuts(s), &blm_for(&job.spec, &ctx.blm, &job.name, &ctx.cfg));
                // BL39: the same refusal as export.
                let issues = readback::check_set(&set, &job.parts, &job.sheets[s], &plans[s], job.cuts(s));
                if !issues.is_empty() {
                    return Err(readback::refusal(s + 1, &issues));
                }
                let mut t = format!("; BLM LC5 set {} — {} subprogram file(s)\n", set.nest, set.spf.len());
                for (n, body) in &set.spf {
                    t.push_str(&format!("; ==== {n}\n{body}\n"));
                }
                t
            } else {
                let marks = stockflow::remnant_marks(&crate::stock::Stock::load_cached(&ctx.library_dir), &job.stock_sheets(&ctx.cfg), &job.formats, s, &ctx.cfg.stock);
                program(s + 1, &job.parts, &job.sheets[s], &job.spec_at(s), &plans[s], job.cuts(s), &marks, &ctx.post, ctx.cfg.stock.line.edge_offset)
            };
            // QR6: say so when parts on the sheet overlap.
            let warn = sheet_overlap_note(&job.parts, s, &job.sheets[s], &job.spec_at(s)).map_or(String::new(), |w| format!(" Warning: {w} and would be cut through each other."));
            let mut o = Outcome::read(format!("Program for sheet {} ({} lines).{warn}", s + 1, text.lines().count()), json!(text));
            if let Some(out) = a.str("out") {
                let out = PathBuf::from(out);
                std::fs::write(&out, &text).map_err(|e| format!("Couldn't write {}: {e}", out.display()))?;
                o.files.push(out);
            }
            Ok(o)
        }
        "export" => {
            let base = a.str("dir").map(PathBuf::from).unwrap_or_else(|| ctx.export_dir());
            let plans = job.plans();
            let blm = blm_for(&job.spec, &ctx.blm, &job.name, &ctx.cfg);
            let marks = stockflow::all_remnant_marks(&ctx.library_dir, &job.stock_sheets(&ctx.cfg), &job.formats, &ctx.cfg.stock);
            let ex = export(&base, &job.name, &job.parts, &job.sheets, &job.formats, &plans, &job.spec, &job.cut_lines, &marks, &ctx.post, Some(&blm), ctx.cfg.stock.line.edge_offset)?;
            let what = if blm.on { "BLM LC5 program set(s) (.zip + .XML)" } else { "program(s)" };
            let mut o = Outcome::read(
                format!("Exported {} {what} and a parts report to {}{}", job.sheets.len(), ex.dir.display(), ex.warning.as_ref().map_or(String::new(), |w| format!(" — but {w}"))),
                Value::Null,
            );
            o.files = ex.files;
            Ok(o)
        }
        "render" => Err("render is handled by the session".into()),
        n => Err(format!("{n} is not built into the engine")),
    }
}

/// Every ribbon command of the GUI, as "Ribbon group/Button label" (a
/// command in a button's menu as "Ribbon group/Item label"), and what
/// does the same for an agent: an operation name, or `view:` (only changes
/// what the screen shows), `gui:` (a window or picker an agent doesn't need,
/// with what to use instead), or `todo: [ID]` (not yet reachable by an agent;
/// ID = its row in FEATURES.md).
pub const GUI_MAP: &[(&str, &str)] = &[
    // --- Jobs screen ---
    ("Jobs/New\njob", "new"),
    ("Jobs/Copy\njob", "gui: copy the .snjob file, then rename"),
    ("Jobs/Save\njob", "gui: every change is saved to the job file"),
    ("Jobs/Open\njob", "gui: name the job file in each call"),
    ("Status line/Show labor\nticket", "view:"),
    ("Import/Import\ndrawings", "import-drawing"),
    ("Nest/Nest\nall", "nest"),
    ("Nesting/Open CNC\nprogram", "import-program"),
    ("Output/Times and\ncosts", "info"),
    ("Output/Export\nCNC", "export"),
    ("Products/Add\nproduct", "add-shape"),
    ("Products/Parts from\nlibrary", "library"),
    ("Products/Delete\nproduct", "remove-part"),
    ("Library/Library", "library"),
    ("Library/Save to\nlibrary", "library"),
    ("Database/Connections", "gui: settings are changed in the app"),
    ("Database/Browse\ndatabase", "todo: [AGT-4] read-only Lantek SQL database browser (Windows)"),
    ("ERP / MES/Send\nnest", "todo: [AGT-7] send the nesting back to the ERP/MES (Fulcrum); the window does it (erpui.rs)"),
    // --- Nesting screen: Home ---
    ("Job/New job", "new"),
    ("Job/Open job ▾", "gui: name the job file in each call"),
    ("Job/Save job", "gui: every change is saved to the job file"),
    ("Sheets/Previous\nsheet", "view: sheets are named by number"),
    ("Sheets/Next\nsheet", "view: sheets are named by number"),
    ("Sheets/Sheet\nstock", "stock"),
    ("Nest/Sheet\nstock", "stock"),
    ("Nest/Re-nest\nsheet", "renest-sheet"),
    ("Insert/Add\npart", "add-shape"),
    ("Insert/Open\npart", "part-info"),
    ("Insert/Import\ndrawings", "import-drawing"),
    ("Edit/Undo", "undo"),
    ("Edit/Redo", "gui: undo again from the session history instead"),
    ("Modify/Collision", "gui: agents always place with collision checking on"),
    ("Modify/Snap", "gui: snapping only steers a mouse drag; agents give exact places (move, or slide to meet the next part)"),
    ("Modify/Copy part", "copy"),
    ("Modify/Copy pattern", "repeat"),
    ("Modify/Rotate 90°", "rotate"),
    ("Modify/Mirror", "mirror"),
    ("Modify/Remove part", "unplace"),
    ("Modify/Clear sheets", "clear-sheets"),
    ("Remnants/Compact\nto left", "compact"),
    ("Remnants/Cut off\nleftover", "remnant"),
    ("Show/Cutting\npath", "render"),
    ("Show/Part\nnumbers", "render"),
    ("Show/Sheet\ngrid", "view:"),
    ("Panes/CNC\nprogram", "program"),
    ("Panes/Times and\ncosts", "info"),
    ("Panes/Side\npanels", "view:"),
    ("Zoom/Fit\nsheet", "view:"),
    // --- Nesting screen: Technology ---
    ("Lead-ins/Auto\nlead-ins", "leads"),
    ("Lead-ins/Delete\nlead-ins", "leads"),
    ("Lead-ins/Standard\nleads", "leads"),
    ("Lead-ins/On element", "lead-at"),
    ("Micro-joints/Auto\njoints", "joints"),
    ("Micro-joints/Delete\njoints", "joints"),
    ("Micro-joints/On element", "joint-at"),
    ("Micro-joints/Joint\nsettings", "tech-set"),
    ("Loops/Auto\nloops", "loops"),
    ("Loops/Delete\nloops", "loops"),
    ("Cut technology/Own\ntechnology", "cut-tech"),
    ("Cut technology/Remove own\ntechnology", "cut-tech"),
    ("Cut technology/Quality\nby size", "cut-tech"),
    ("Cut technology/Paint\ncontours", "contour-tech"),
    ("Text/Delete\ntexts", "text"),
    // --- Nesting screen: Machining, CNC, Simulation ---
    ("Manual cutting/Pick\ncontours", "cut-order"),
    ("Manual cutting/Reverse\ndirection", "cut-order"),
    ("Manual cutting/Undo\npick", "cut-order"),
    ("Manual cutting/Clear", "cut-order"),
    ("Remnants & cut lines/Cut off\nleftover", "remnant"),
    ("Remnants & cut lines/Exact\nedge", "remnant"),
    ("Remnants & cut lines/Straight\nline", "remnant"),
    ("Remnants & cut lines/Semi-\nautomatic", "todo: [AGT-6] click a free area to make it a remnant (remline::semi_auto_lines) — needs a pick point; use remnant mode=line or cut-line meanwhile"),
    ("Remnants & cut lines/Rotate\nremnant", "todo: [AGT-6] turn or mirror an empty remnant sheet (RE26); sheet-change to another stock remnant meanwhile"),
    ("Remnants & cut lines/Draw\npath", "cut-line"),
    ("Remnants & cut lines/Delete\nline", "cut-line"),
    ("Machining/Machine\nall", "program"),
    ("CNC/CNC\nprogram", "program"),
    ("CNC/Machine\n& output", "machines"),
    ("CNC/Export\nCNC", "export"),
    ("Simulate/Beginning", "view: render shows the cut order, info the times"),
    ("Simulate/Previous", "view: render shows the cut order, info the times"),
    ("Simulate/Next", "view: render shows the cut order, info the times"),
    ("Simulate/End", "view: render shows the cut order, info the times"),
    ("Simulate/Stop", "view: render shows the cut order, info the times"),
    ("Display/Simulation\nwindow", "view:"),
    ("Display/Follow\nhead", "view:"),
    // --- Part drawing (Drawing module) ---
    ("Parts/Back to\nnesting", "view: switches screens"),
    ("Parts/Previous\npart", "view: parts are named by number"),
    ("Parts/Next\npart", "view: parts are named by number"),
    ("Draw/Points", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Draw/Shapes", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Draw/Delete\ngeometry", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Edit geometry/Modify", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Edit geometry/Transform", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Edit geometry/Figures", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Check/Check\ngeometry", "part-info"),
    ("Measure/Orthogonal", "view:"),
    ("Macros/Macros", "todo: [NST-7] macros (greyed out in the app too)"),
    ("Help/Coming\nsoon", "view: list of unbuilt commands"),
    ("Machining/Machine\npart", "part-cut-order"),
    ("Machining/Clear\nmachining", "view: the part's machining is only shown or hidden"),
    ("Machining/Clear cut\norder", "view: the hand-picked order is cleared and the path hidden"),
    ("Standard shapes/Recognise\nshapes", "part-info"),
    ("Contours/Contours", "part-cut-order"),
    ("Contours/Cycles", "todo: [MAC-11] punch/milling cycles (greyed out in the app too)"),
    ("Settings/Options", "todo: [MAC-1] the Feed rates dialog and postprocessor words (greyed out in the app too); the speeds themselves come from the machine's files: machine-tech shows them"),
    ("Compensation/Kerf", "compensation"),
    ("Settings/Extraction", "todo: [MAC-6] component extraction (greyed out in the app too)"),
    ("Check/Current\nstatus", "part-info"),
    ("Check/Verify\nmachining", "verify"),
    ("Display/Parts", "view:"),
    ("Zoom/Zoom\nall", "view:"),
    ("More/Steps", "view:"),
    ("Dimension notes/Auto\ndimensions", "todo: [DRW-2] dimension notes on the part drawing"),
    ("Dimension notes/Delete", "todo: [DRW-2] dimension notes on the part drawing"),
    ("Texts/Texts", "text"),
    ("Texts/Modify", "todo: [DRW-3] move/edit one text (add/delete work through text)"),
    ("View/Texts and\ndimensions", "view:"),
    ("Utilities/Bevels", "todo: [TEC-7] bevels (greyed out in the app too)"),
    ("Utilities/Contours", "todo: [DRW-4] contour utilities (greyed out in the app too)"),
    ("Utilities/Compare", "todo: [DRW-4] compare parts (greyed out in the app too)"),
    ("Utilities/Lifting", "todo: [DRW-4] lifting (greyed out in the app too)"),
    ("Contours/Manual Contours", "part-cut-order"),
    ("Contours/Automatic Contours", "part-info"),
    ("Contours/Parts", "part-info"),
    ("Contours/All Marks", "part-cut-order"),
    ("Contours/All Cut", "part-cut-order"),
    ("Contours/All Contours of one Quality", "part-cut-order"),
];

#[cfg(test)]
mod parity {
    use super::*;

    /// Ribbon commands found in the GUI sources, as "Group/Label".
    pub fn ribbon_commands() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = vec![];
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "rs")).collect();
        files.sort();
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let file = f.file_name().unwrap().to_string_lossy().to_string();
            let mut group = String::new();
            for line in text.lines() {
                if let Some(g) = quoted_after(line, "ribbon_group(ui, ") {
                    group = g;
                }
                for call in ["ribbon_button(ui, Icon::", "ribbon_button_ex(ui, Icon::", "dropdown(ui, Icon::", "ribbon_small(ui, Icon::", "menu_item(ui, Icon::"] {
                    if let Some(rest) = line.split_once(call).map(|x| x.1) {
                        if let Some(label) = rest.split_once(", \"").and_then(|(_, l)| l.split_once('"')).map(|x| x.0.replace("\\n", "\n")) {
                            out.push((format!("{group}/{label}"), file.clone()));
                        }
                    }
                }
            }
        }
        out
    }

    fn quoted_after(line: &str, pat: &str) -> Option<String> {
        let rest = line.split_once(pat)?.1;
        let rest = rest.strip_prefix('"')?;
        Some(rest.split_once('"')?.0.to_string())
    }

    /// Every `todo:` names its row in FEATURES.md, so what agents can't do yet
    /// stays on the feature list; and the list's IDs are unique.
    #[test]
    fn todo_entries_name_a_feature() {
        let list = include_str!("../../../FEATURES.md");
        let ids: Vec<&str> = list.lines().filter_map(|l| l.strip_prefix("| ")).filter_map(|l| l.split(" |").next()).filter(|id| id.len() > 4 && id.as_bytes()[3] == b'-' && id[..3].chars().all(|c| c.is_ascii_uppercase())).collect();
        let mut seen = std::collections::HashSet::new();
        for id in &ids {
            assert!(seen.insert(*id), "FEATURES.md lists {id} twice");
        }
        for (k, v) in GUI_MAP.iter().filter(|(_, v)| v.starts_with("todo:")) {
            let id = v.trim_start_matches("todo:").trim().strip_prefix('[').and_then(|r| r.split(']').next());
            let Some(id) = id else { panic!("GUI_MAP {k:?}: write it as \"todo: [ID] …\" with its FEATURES.md row") };
            assert!(ids.contains(&id), "GUI_MAP {k:?} names {id}, which isn't a row in FEATURES.md");
        }
    }

    #[test]
    fn every_ribbon_command_is_mapped() {
        let found = ribbon_commands();
        let missing: Vec<String> = found.iter().filter(|(k, _)| !GUI_MAP.iter().any(|(m, _)| m == k)).map(|(k, f)| format!("  (\"{}\", \"…\"),   // {f}", k.replace('\n', "\\n"))).collect();
        assert!(missing.is_empty(), "Ribbon commands with no agent operation — add them to engine::ops::GUI_MAP (an op name, or view:/gui:/todo: with a reason):\n{}", missing.join("\n"));
        let gone: Vec<&str> = GUI_MAP.iter().filter(|(m, _)| !found.iter().any(|(k, _)| k == m)).map(|x| x.0).collect();
        assert!(gone.is_empty(), "GUI_MAP names ribbon commands that no longer exist: {gone:?}");
        for (k, v) in GUI_MAP {
            let ok = ["view:", "gui:", "todo:"].iter().any(|p| v.starts_with(p)) || find(v).is_some();
            assert!(ok, "GUI_MAP {k:?} → {v:?} is not an operation (or view:/gui:/todo:)");
        }
    }
}
