//! Every operation an agent (or a script) can run on a job — the registry the
//! command line and the MCP tools are built from. Numbers an agent sees are
//! 1-based like on screen: part 1, sheet 1, item #1 on a sheet, element 1.
//!
//! Keeping up with the GUI: `GUI_MAP` names, for every ribbon command, the
//! operation that does the same here (or why it has none). The test
//! `every_ribbon_command_is_mapped` fails when a ribbon button is added
//! without an entry, so a new GUI feature can't silently skip the agents.

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
    /// Changes a file: the job, or (the stock operation) the sheet stock.
    pub fn writes(&self) -> bool {
        matches!(self.kind, Kind::Write | Kind::Create) || self.name == "stock"
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
const QUALITIES: &[&str] = &["rough", "normal", "fine"];
const TEXTS: &[&str] = &["mark", "special", "note"];

pub static OPS: &[Op] = &[
    // --- job ---
    Op { name: "new", group: "Job", kind: Kind::Create, help: "Make a new, empty job file with its sheet (size, material, thickness).", args: &[
        a("name", Ty::Str, "Job name (default: the file name)"), a("week", Ty::Str, "Folder/week the job belongs to"),
        a("w", Ty::Num, "Sheet length X, mm (default 2000)"), a("h", Ty::Num, "Sheet width Y, mm (default 1000)"),
        a("material", Ty::Str, "Material name or number (see catalog), default mild steel"), a("thickness", Ty::Num, "Thickness, mm (default 3)"),
        a("margin", Ty::Num, "Sheet margin, mm (default 5)"), a("gap", Ty::Num, "Gap between parts, mm (default 5)"),
        a("level", Ty::Int, "Automatic nesting level 1-5 (default 3)"), c("machine", MACHINES, "Machine type (default laser)"),
        a("force", Ty::Bool, "Overwrite an existing file"),
    ] },
    Op { name: "info", group: "Job", kind: Kind::Read, help: "Job report: sheet, parts (ordered / nested / missing, technology), sheets (usage, cut length, pierces, time, remnant). With sheet=N, also every placed part on that sheet.", args: &[a("sheet", Ty::Int, "Also list the parts placed on this sheet")] },
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
    Op { name: "dxf-layers", group: "Parts", kind: Kind::Global, help: "List a DXF drawing's layers and how many objects each holds, before importing it.", args: &[r("path", Ty::Path, "The DXF file")] },
    Op { name: "import-dxf", group: "Parts", kind: Kind::Write, help: "Import DXF drawings as parts. Every layer is cut unless `layers` says otherwise; part data in the file name follows Settings → name format.", args: &[
        r("path", Ty::Path, "DXF file, or several separated by ';'"), a("layers", Ty::Str, "Layer roles: \"LAYER=cut|mark|ignore,…\" (others: cut)"),
        a("qty", Ty::Int, "Quantity for every imported part (default: from the file name, else 1)"), a("name", Ty::Str, "Reference (one file only)"), a("group", Ty::Str, "Group for the imported parts"),
    ] },
    Op { name: "edit-part", group: "Parts", kind: Kind::Write, help: "Change a part's reference, quantity, size (built-in shapes), rotations or group. Changing the size of a part with technology needs tech=copy (keep it, change a copy) or tech=drop (remove its technology).", args: &[
        PART, a("name", Ty::Str, "New reference"), a("qty", Ty::Int, "Quantity ordered"), a("w", Ty::Num, "Length X, mm (built-in shapes)"), a("h", Ty::Num, "Width Y, mm (built-in shapes)"),
        a("rotate", Ty::Bool, "May turn 90°"), a("mirror", Ty::Bool, "May be mirrored"), a("group", Ty::Str, "Group"), c("tech", &["copy", "drop"], "Size change on a part with technology"),
    ] },
    Op { name: "remove-part", group: "Parts", kind: Kind::Write, help: "Take a part out of the job (its placed copies go too).", args: &[PART] },
    Op { name: "part-info", group: "Parts", kind: Kind::Read, help: "A part's geometry and technology: size, area, cut length, contours, every element (numbered, for lead-at / joint-at), texts, and Verify Part / Verify Machining results.", args: &[PART] },
    Op { name: "export-part-dxf", group: "Parts", kind: Kind::Read, help: "Save a part as DXF (outline and holes on CUT, marking on MARK).", args: &[PART, r("out", Ty::Path, "DXF file to write")] },
    Op { name: "library", group: "Parts", kind: Kind::Write, help: "The product/job library: list it, add a library product to the job, or save parts / the job into it.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "What to do", choices: &["list", "add", "save-part", "save-job"] },
        a("find", Ty::Str, "list: text to look for"), a("id", Ty::Int, "add: the product's record number"), a("name", Ty::Str, "add: the product's name"),
        a("qty", Ty::Int, "add: quantity"), a("parts", Ty::Parts, "save-part: which parts"), a("folder", Ty::Str, "save-part: product folder/group"),
    ] },
    // --- sheet and nesting ---
    Op { name: "set-sheet", group: "Nesting", kind: Kind::Write, help: "Change the sheet: size, margin, gap, material, thickness, nesting level, machine. A new material or thickness with nested sheets needs nestings=keep or nestings=remove.", args: &[
        a("w", Ty::Num, "Length X, mm"), a("h", Ty::Num, "Width Y, mm"), a("margin", Ty::Num, "Margin, mm"), a("gap", Ty::Num, "Gap between parts, mm"),
        a("material", Ty::Str, "Material name or number"), a("thickness", Ty::Num, "Thickness, mm"), a("level", Ty::Int, "Nesting level 1-5"), c("machine", MACHINES, "Machine type"),
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
    Op { name: "rotate", group: "Nesting", kind: Kind::Write, help: "Turn a placed part to its next allowed quarter turn, where there is room.", args: &[SHEET, ITEM] },
    Op { name: "turn", group: "Nesting", kind: Kind::Write, help: "Turn placed parts together about the centre of their box by 90° (counter-clockwise), -90° or 180°; when the turned group would overlap a part or leave the sheet it moves to the nearest free spot.", args: &[
        SHEET, ITEM_ONE, ITEMS,
        Arg { name: "angle", ty: Ty::Int, required: false, help: "90 (default, counter-clockwise), -90 or 180", choices: &[] },
    ] },
    Op { name: "mirror", group: "Nesting", kind: Kind::Write, help: "Mirror placed parts together about the centre of their box: axis x = left↔right (default), y = top↔bottom; moved to the nearest free spot when needed.", args: &[
        SHEET, ITEM_ONE, ITEMS,
        Arg { name: "axis", ty: Ty::Str, required: false, help: "x = left↔right (default), y = top↔bottom", choices: &["x", "y"] },
    ] },
    Op { name: "copy", group: "Nesting", kind: Kind::Write, help: "Copy a placed part (item), or several keeping their layout (items), into the first free spot on its sheet (each part's order grows by one per copy).", args: &[SHEET, ITEM_ONE, ITEMS] },
    Op { name: "unplace", group: "Nesting", kind: Kind::Write, help: "Take one placed part off its sheet.", args: &[SHEET, ITEM] },
    // --- technology ---
    Op { name: "leads", group: "Technology", kind: Kind::Write, help: "Lead-ins: auto (placed by contour class — round, rectangular or other part or hole: round ones at the keypad position's angle, rectangles at its corner or nearest side, other shapes at the nearest valid corner, else the middle of the nearest long-enough line), machine (lengths and position from the machine's CT1), or delete.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "auto, machine or delete", choices: &["auto", "machine", "delete"] }, PARTS,
        c("type", LEADS, "auto: lead type (default machine = the machine's lead tables per contour class and quality; line = inclined)"), a("position", Ty::Int, "auto: keypad position 1-9 (1 bottom-left, 9 top-right; default 1)"),
    ] },
    Op { name: "lead-at", group: "Technology", kind: Kind::Write, help: "Start a contour on one element of a part (see part-info for element numbers): by the third of a line nearest `at`, else at its middle.", args: &[PART, r("element", Ty::Int, "Element number from part-info"), a("at", Ty::Points, "Pick point \"x,y\" in part mm (optional)")] },
    Op { name: "joints", group: "Technology", kind: Kind::Write, help: "Micro-joints: auto (one per the machine's maximum distance — 100 mm by default — on every contour, holes too, rounded up to an even number and centred on its line or arc, within the machine's fewest/most for the contour's size class) or delete.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "auto or delete", choices: &["auto", "delete"] }, PARTS,
        a("every", Ty::Num, "auto: one joint per this many mm of each contour (default: the machine's maximum distance for the part's quality)"),
    ] },
    Op { name: "machine-tech", group: "Technology", kind: Kind::Read, help: "Which material × thickness section of the machine's technology file applies to the job (or to another material/thickness), and the values it gives: lead and joint tables, joint gap, pierce and loop radius, kerf, part gap and margins (compare with the sheet's; take them with set-sheet machine_spacing=true).", args: &[
        a("material", Ty::Str, "Look at this material instead of the job's (name or number)"), a("thickness", Ty::Num, "Look at this thickness instead of the job's, mm"),
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
        a("ranges", Ty::Str, "ranges: up to 3 \"from-to:quality\" by equivalent radius in mm, e.g. \"0-5:fine,5-20:normal\" (empty = off)"),
    ] },
    Op { name: "tech-set", group: "Technology", kind: Kind::Write, help: "Set technology values on parts directly (the technology settings windows).", args: &[
        PARTS, a("lead_in", Ty::Num, "Lead-in length, mm"), a("lead_out", Ty::Num, "Lead-out length, mm"), c("lead_type", LEADS, "Lead shape"), a("position", Ty::Int, "Keypad position 1-9"),
        a("corner_start", Ty::Bool, "Start at the nearest valid corner"), a("overlap", Ty::Num, "Cut past the start before the lead-out, mm"),
        a("micro_joints", Ty::Int, "Joints per outside contour"), a("joint_width", Ty::Num, "Joint gap, mm"), c("joint_type", JOINTS, "Joint type"),
        a("joint_lead", Ty::Num, "Joint lead length, mm"), a("joint_angle", Ty::Num, "Joint lead angle, degrees"), a("joint_every", Ty::Num, "Automatic joints: one per this many mm of each contour, holes too (0 = use micro_joints)"),
        a("joint_lead_out", Ty::Num, "Joint lead-out distance, mm (default: the same as joint_lead)"),
        a("loops", Ty::Bool, "Loops at sharp corners"), a("loop_size", Ty::Num, "Loop size, mm"), c("quality", QUALITIES, "Cut quality"),
    ] },
    Op { name: "text", group: "Technology", kind: Kind::Write, help: "Add a text along the bottom of parts (mark = engraved, special = filled in: #REF #QTY #MAT #THK #NUM, note = not machined), or delete all their texts.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "add or delete", choices: &["add", "delete"] }, PARTS,
        a("text", Ty::Str, "add: the text"), a("height", Ty::Num, "add: capital height, mm (default 8)"), c("kind", TEXTS, "add: text kind (default special)"),
    ] },
    // --- machining ---
    Op { name: "cut-order", group: "Machining", kind: Kind::Write, help: "Manual cutting order: pick the contour nearest x,y (cut first, from there), undo the last pick, or clear them (all automatic).", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "pick, undo or clear", choices: &["pick", "undo", "clear"] }, SHEET,
        a("x", Ty::Num, "pick: X on the sheet, mm"), a("y", Ty::Num, "pick: Y, mm"), a("reverse", Ty::Bool, "pick: cut it the other way"), a("tol", Ty::Num, "pick: how near, mm (default 5)"),
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
    Op { name: "stock", group: "Stock", kind: Kind::Global, help: "The sheet stock (the Library folder's Sheets\\stock.json, shared with the window's Sheet stock): list sheets and remnants, create a sheet, add or remove quantity (0 deletes), lock or unlock, edit, delete, or read the Do All / Sheet Remnants options (rules; set in the window).", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "What to do", choices: &["list", "create", "add", "remove", "block", "unblock", "edit", "delete", "rules"] },
        a("ref", Ty::Str, "Sheet reference (create: optional, else the next number; block/unblock: optional, else every listed sheet)"),
        a("material", Ty::Str, "list/create/block: material name or number"), a("thickness", Ty::Num, "list/create/block: thickness, mm"),
        a("w", Ty::Num, "create/edit: length X, mm"), a("h", Ty::Num, "create/edit: width Y, mm"), a("qty", Ty::Int, "create/add/remove: how many (default 1)"),
        a("priority", Ty::Int, "create/edit: 1 is used first when ordering by priority"), a("blocked", Ty::Bool, "create/edit: locked — never used by Do All"),
        a("user1", Ty::Str, "create/edit: user data 1"), a("user2", Ty::Str, "create/edit: user data 2"), a("user3", Ty::Str, "create/edit: user data 3"),
        a("min_w", Ty::Num, "list: shortest length"), a("max_w", Ty::Num, "list: longest length"), a("min_h", Ty::Num, "list: narrowest width"), a("max_h", Ty::Num, "list: widest width"),
        a("show_zero", Ty::Bool, "list: also items at quantity 0"), a("remnants", Ty::Str, "list: \"only\", \"none\" or \"all\" (default)"),
    ] },
    Op { name: "stock-book", group: "Stock", kind: Kind::Write, help: "Book the job against the stock, as the window does when it saves a job: take the sheets its nestings use out of stock and store the remnants its remnant lines leave (named <sheet>-<n>, or by refs=; below the minimum area they are scrap). A sheet whose remnant lines changed since gets its remnants made again, unless one of them has been used. Once booked, every later change keeps the stock in step. release gives the sheets back and takes its unused remnants out again.", args: &[
        Arg { name: "mode", ty: Ty::Str, required: true, help: "book, release or status", choices: &["book", "release", "status"] },
        a("refs", Ty::Str, "book: references for the new remnants, comma-separated, in the order status lists them (default: numbered <sheet>-<n>)"),
    ] },
    // --- looking at it ---
    Op { name: "render", group: "View", kind: Kind::Read, help: "Draw a sheet (parts, numbers, cut path, leads, joints, cut lines) or one part (numbered elements, leads, joints) to PNG or SVG.", args: &[
        a("sheet", Ty::Int, "Sheet number (default 1)"), a("part", Ty::Parts, "Draw this part instead of a sheet"), a("out", Ty::Path, "File to write (.png or .svg; default next to the job)"),
        a("size", Ty::Int, "Image width in pixels (default 1400)"), a("path", Ty::Bool, "Show the cut path, leads and order (default true)"),
    ] },
    // --- reference ---
    Op { name: "catalog", group: "Reference", kind: Kind::Global, help: "Choices the other operations take: shapes, materials, thicknesses, nesting levels, machine types, lead/joint types, qualities, text kinds and fields, layer roles.", args: &[] },
    Op { name: "machines", group: "Reference", kind: Kind::Global, help: "The machines in Settings (read-only here): output, post library, and the active machine's technology (CT1) values.", args: &[] },
];

pub fn find(name: &str) -> Option<&'static Op> {
    let n = name.replace('_', "-");
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
fn quality_from(s: &str) -> Result<CutQuality, String> {
    pick(s, "quality", &[("rough", CutQuality::Rough), ("normal", CutQuality::Normal), ("fine", CutQuality::Fine)])
}
fn text_kind_from(s: &str) -> Result<TextKind, String> {
    pick(s, "text kind", &[("mark", TextKind::Mark), ("special", TextKind::Special), ("note", TextKind::Note)])
}
fn layer_role_from(s: &str) -> Result<dxf::LayerRole, String> {
    pick(s, "layer role", &[("cut", dxf::LayerRole::Cut), ("mark", dxf::LayerRole::Mark), ("ignore", dxf::LayerRole::Ignore)])
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
            "loops": t.loops, "loop_size": t.loop_size, "quality": t.quality.label().to_lowercase(),
            "leads_on_elements": t.starts.iter().flatten().count(), "joints_on_elements": t.joint_picks.iter().flatten().count(),
            "cut_ranges": t.cut_ranges.iter().filter(|c| c.on).map(|c| format!("{}-{}:{}", c.min_r, c.max_r, c.quality.label().to_lowercase())).collect::<Vec<_>>(),
        }),
    }
}

fn spec_json(spec: &SheetSpec) -> Value {
    json!({
        "w": spec.w, "h": spec.h, "margin": spec.margin, "gap": spec.gap,
        "material": cnc::MATERIALS.get(spec.material).copied().unwrap_or("?"), "material_no": spec.material, "thickness": spec.thickness,
        "level": spec.nest_level().name, "machine": spec.machine.label(),
    })
}

fn placed_json(job: &JobFile, s: usize) -> Value {
    json!(job.sheets[s].iter().enumerate().map(|(i, p)| json!({
        "item": i + 1, "part": p.part + 1, "name": job.parts[p.part].name, "x": r1(p.x), "y": r1(p.y), "w": r1(p.w), "h": r1(p.h), "angle": p.angle(), "mirror": p.mirror,
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
        "job": job.name, "week": job.week, "changed_since_nest": job.stale, "sheet": spec_json(&job.spec),
        "machine": { "name": ctx.cfg.machine().name, "output": if ctx.blm.on { "BLM LC5 (Siemens 840D set)" } else { "ISO G-code" } },
        "parts": parts, "sheets": sheets,
        "totals": { "ordered": need, "nested": placed, "sheets": job.sheets.len(), "cut_len_mm": r1(tot.0), "rapid_len_mm": r1(tot.1), "pierces": tot.2, "time_s": r1(tot.3), "method": job.method },
        "warnings": warnings,
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
    let plan = cnc::plan(&job.parts, &[Placed { part: i, x: 0.0, y: 0.0, w, h, turns: 0, mirror: false }], &job.spec);
    let contours: Vec<Value> = g
        .holes
        .iter()
        .enumerate()
        .map(|(k, hh)| json!({"contour": k + 1, "kind": "hole", "length_mm": r1(nest::perimeter(hh)), "standard_shape": crate::machine::recognise(hh).map(|s| s.label())}))
        .chain(std::iter::once(json!({"contour": g.holes.len() + 1, "kind": "outside", "length_mm": r1(nest::perimeter(&g.outline)), "standard_shape": crate::machine::recognise(&g.outline).map(|s| s.label())})))
        .collect();
    json!({
        "part": i + 1, "name": p.name, "shape": p.shape.label(), "w": r1(w), "h": r1(h), "qty": p.qty, "group": p.group,
        "area_mm2": r1(p.net_area()), "cut_len_mm": r1(p.cut_length()),
        "rotations": { "turn_90": p.rotate, "plain": p.rots.plain, "mirrored": p.rots.mirrored },
        "contours": contours,
        "elements": elems.iter().enumerate().map(|(k, e)| json!({
            "element": k + 1, "contour": e.contour + 1, "kind": e.kind.label().to_lowercase(), "length_mm": r1(e.length), "mid": [r1(e.midpoint()[0]), r1(e.midpoint()[1])],
        })).collect::<Vec<_>>(),
        "texts": p.texts.iter().map(|t| json!({"text": t.text, "kind": format!("{:?}", t.kind).to_lowercase(), "height": r1(t.height), "at": [r1(t.at[0]), r1(t.at[1])]})).collect::<Vec<_>>(),
        "technology": tech_json(&p.tech),
        "verify_part": crate::drawing::verify_part(p),
        "verify_machining": crate::drawing::verify_machining(&plan),
        "machining": { "cut_len_mm": r1(plan.cut_len), "pierces": plan.pierces, "time_s": r1(plan.seconds) },
    })
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
        "keypad_positions": "1 bottom-left, 2 bottom, 3 bottom-right, 4 left, 5 centre, 6 right, 7 top-left, 8 top, 9 top-right",
        "numbering": "parts, sheets, placed items (#) and elements are numbered from 1; materials from 0",
    })
}

/// The machine's values for a sheet's material and thickness (MTL1–MTL9):
/// which section applies and what it gives, beside the job's own spacing.
fn machine_tech_report(spec: &SheetSpec, job: &SheetSpec) -> (String, Value) {
    let t = crate::ct1::for_spec(spec);
    let q1 = t.leads.quality(1);
    let opt = |v: Option<f64>| v.map_or(Value::Null, |x| json!(r1(x * 1000.0) / 1000.0));
    let section = t.applied().map(|s| {
        json!({
            "number": s.no, "material_key": s.material, "material": crate::ct1::material_label(&s.material),
            "thickness_from": s.t_min, "thickness_to": s.t_max, "gas": s.gas, "default": s.default, "hole_size_bands": s.hole_bands.len(),
        })
    });
    let mat = cnc::MATERIALS.get(spec.material).copied().unwrap_or("?");
    let mut msg = format!("{mat} {} mm: {}.", spec.thickness, t.table_label());
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
        "material": mat, "material_key": t.picked_for.as_ref().map(|p| p.0.clone()), "thickness": spec.thickness,
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
        "compensation": crate::kerf::status_line(spec),
        "spacing": {
            "machine_gap": opt(t.part_gap), "machine_margins": t.margins.map(|m| json!({"bottom": m[0], "top": m[1], "left": m[2], "right": m[3]})),
            "machine_margin": opt(t.sheet_margin()), "remnant_margin": opt(t.remnant_margin), "y_gap": opt(t.y_gap),
            "job_gap": job.gap, "job_margin": job.margin,
        },
    });
    (msg, data)
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

fn machines(ctx: &Ctx) -> Value {
    let t = &ctx.tech;
    json!({
        "active": ctx.cfg.active_machine + 1,
        "machines": ctx.cfg.machines.iter().enumerate().map(|(i, m)| json!({
            "machine": i + 1, "name": m.name, "output": m.output.key(), "post_lib": m.post_lib, "blm_ps1": m.blm_ps1, "blm_cf1": m.blm_cf1, "gas": m.gas_name,
        })).collect::<Vec<_>>(),
        "technology": {
            "source": t.source.as_ref().map_or("built-in values".to_string(), |p| p.display().to_string()),
            "lead_in": t.lead_in, "lead_out": t.lead_out, "overlap": t.overlap, "position": t.zone, "hole_lead_in": t.hole_lead_in,
            "joint_gap": t.joint_gap, "joint_every": t.joint_every, "group_parts": t.group_parts, "zones": [t.zones_x, t.zones_y],
            "material_sections": t.file.sections.len(),
            "note": "These are the machine's own values; a job's material and thickness may pick a section of the file instead (machine-tech).",
        },
        "post": { "extension": ctx.post.file_name("JOB", 1) },
        "note": "Settings are changed in the app (Settings window), not by agents.",
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
    let m = format!("New job '{}' — sheet {} × {} mm, {} {} mm. Add parts next.", job.name, job.spec.w, job.spec.h, cnc::MATERIALS.get(job.spec.material).copied().unwrap_or("?"), job.spec.thickness);
    Ok((job, m))
}

/// `import-xml`: one job file per material and thickness.
fn import_xml(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
    let path = PathBuf::from(a.need_str("path")?);
    let lib = library(ctx).ok();
    let im = imports::xml_import(&ctx.cfg, lib.as_ref(), &path, 0)?;
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
        "dxf-layers" => {
            let path = a.need_str("path")?;
            let d = read_dxf(Path::new(&path))?;
            let layers = d.layers();
            Ok(Outcome::read(format!("{} layer(s) in {path}.", layers.len()), json!(layers)))
        }
        n => Err(format!("{n} needs a job")),
    }
}

fn read_dxf(path: &Path) -> Result<dxf::Drawing, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: couldn't read ({e})", path.display()))?;
    dxf::read(&String::from_utf8_lossy(&bytes)).map_err(|e| format!("{}: {e}", path.display()))
}

fn library(ctx: &Ctx) -> Result<crate::library::Library, String> {
    let dir = if ctx.cfg.library_dir.is_empty() { crate::library::default_dir() } else { PathBuf::from(&ctx.cfg.library_dir) };
    crate::library::Library::open(&dir)
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

fn stock_json(s: &crate::stock::Item) -> Value {
    json!({
        "ref": s.reference, "kind": s.kind.label(), "material": cnc::MATERIALS.get(s.material).copied().unwrap_or("?"), "thickness": s.thickness, "w": r1(s.length), "h": r1(s.width), "qty": s.qty,
        "priority": s.priority, "blocked": s.locked, "remnant": s.kind.is_remnant(), "area_m2": (s.area_m2() * 1000.0).round() / 1000.0,
        "origin": s.origin, "job": s.from_job, "irregular": s.outline.is_some(), "user": s.user_data, "created": s.created,
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
    let dir = ctx.library_dir.clone();
    let file = Stock::file(&dir);
    let mut st = ctx.stock()?;
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
    let out = |st: &Stock, m: String| -> Result<Outcome, String> {
        st.save(&dir)?;
        let mut o = Outcome::read(m, Value::Null);
        o.files.push(file.clone());
        Ok(o)
    };
    match a.need_str("mode")?.as_str() {
        "list" => {
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
            let list: Vec<Value> = st.items.iter().filter(|it| matches(it) && crit.admits(it)).map(stock_json).collect();
            let n: i64 = st.items.iter().map(|s| s.qty.max(0) as i64).sum();
            Ok(Outcome::read(format!("{} stock item(s) listed ({} sheet(s) and remnant(s) in stock in all). Stock file: {}", list.len(), n, file.display()), json!({ "sheets": list, "rules": rules_json(&ctx.cfg.stock) })))
        }
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
            out(&st, msg)
        }
        "add" | "remove" => {
            let r = need_ref()?;
            let q = qty()?;
            st.change_qty(&r, if a.need_str("mode")? == "add" { q } else { -q })?;
            let msg = match st.get(&r) {
                None => format!("'{r}' is used up and removed from stock."),
                Some(it) => format!("'{r}': {} in stock.", it.qty),
            };
            out(&st, msg)
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
            out(&st, format!("{} {n} stock item(s) — {}", if on { "Locked" } else { "Unlocked" }, if on { "Do All won't use them." } else { "Do All may use them." }))
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
            out(&st, msg)
        }
        "delete" => {
            let r = need_ref()?;
            let it = st.get(&r).cloned().ok_or_else(|| format!("No sheet '{r}' in stock."))?;
            st.delete(&r);
            out(&st, format!("{} removed from stock ({} on the rack).", describe(&it), it.qty))
        }
        "rules" => Ok(Outcome::read(
            "The Do All and Sheet Remnants options nesting and saving follow. They are the window's settings (Sheet stock → Options) and aren't changed from here.",
            rules_json(&ctx.cfg.stock),
        )),
        m => Err(format!("stock mode '{m}'?")),
    }
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
        "import-dxf" => {
            let paths: Vec<PathBuf> = a.need_str("path")?.split(';').filter(|s| !s.trim().is_empty()).map(|s| PathBuf::from(s.trim())).collect();
            let mut drawings = vec![];
            for p in &paths {
                let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Drawing".into());
                drawings.push((stem, read_dxf(p)?));
            }
            let mut roles = default_roles(drawings.iter().map(|d| &d.1));
            if let Some(spec) = a.str("layers") {
                for kv in spec.split(',').filter(|s| !s.trim().is_empty()) {
                    let (l, role) = kv.split_once('=').ok_or_else(|| format!("layers: '{kv}' is not LAYER=role"))?;
                    let l = l.trim();
                    let k = roles.keys().find(|x| x.eq_ignore_ascii_case(l)).cloned().ok_or_else(|| format!("layers: no layer '{l}' in the drawing(s) ({})", roles.keys().cloned().collect::<Vec<_>>().join(", ")))?;
                    roles.insert(k, layer_role_from(role.trim())?);
                }
            }
            let mut notes = vec![];
            let mut added = vec![];
            for (stem, d) in &drawings {
                let (mut p, n) = part_from_drawing(&ctx.cfg, &job.spec, stem, d, &roles, next_color(&job.parts))?;
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
            d.w = nw.unwrap_or(d.w).max(1.0);
            d.h = nh.unwrap_or(d.h).max(1.0);
            if resized && d.tech.is_some() {
                // The original asks: edit a copy (keeps this part's technology) or
                // edit the part and remove its technology (T2).
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
            let lib = library(ctx)?;
            match a.need_str("mode")?.as_str() {
                "list" => {
                    let find = a.str("find").unwrap_or_default().to_lowercase();
                    let list: Vec<Value> = lib
                        .entries()
                        .into_iter()
                        .filter(|e| find.is_empty() || e.name.to_lowercase().contains(&find) || e.folder.to_lowercase().contains(&find))
                        .map(|e| json!({"id": e.id, "kind": format!("{:?}", e.kind).to_lowercase(), "name": e.name, "folder": e.folder, "material": cnc::MATERIALS.get(e.material).copied().unwrap_or("?"), "thickness": e.thickness, "w": r1(e.w), "h": r1(e.h), "preprogrammed": e.tech}))
                        .collect();
                    Ok(Outcome::read(format!("{} library record(s).", list.len()), json!(list)))
                }
                "add" => {
                    let id = match (a.int("id")?, a.str("name")) {
                        (Some(id), _) => id as u64,
                        (None, Some(n)) => lib.find_product(&n).ok_or_else(|| format!("no library product is called '{n}'"))?,
                        _ => return Err("library add needs 'id' or 'name'".into()),
                    };
                    let pf = lib.load_product(id)?;
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
                    let folder = a.str("folder").unwrap_or_default();
                    let mut ids = vec![];
                    for &i in &t {
                        let existing = lib.find_product(&job.parts[i].name);
                        ids.push(lib.save_product(&job.parts[i], &folder, job.spec.material, job.spec.thickness, existing)?);
                    }
                    Ok(Outcome::read(format!("Saved {} into the library.", who(job, &t)), json!({ "ids": ids })))
                }
                "save-job" => {
                    let id = lib.save_job(job, None)?;
                    Ok(Outcome::read(format!("Saved job '{}' into the library as record {id}.", job.name), json!({ "id": id })))
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
            let mut m = format!("Sheet: {} × {} mm, {} {} mm, {}, gap {} mm, margin {} mm.", spec.w, spec.h, cnc::MATERIALS.get(spec.material).copied().unwrap_or("?"), spec.thickness, spec.nest_level().name, spec.gap, spec.margin);
            if let Some(s) = spacing {
                m = format!("{m} {s}");
            }
            if nested && mat_change {
                // The original asks once: keep the nestings, or remove them and free the sheets (T4).
                match a.str("nestings").as_deref() {
                    Some("remove") => {
                        m.push_str(&format!(" Removed {} nested sheet(s); nest again.", job.sheets.len()));
                        job.clear_nestings();
                    }
                    Some("keep") => m.push_str(" Kept the nested sheets; they're marked changed."),
                    _ => return Err(format!("{} sheet(s) are nested in the old material/thickness. Say nestings=keep or nestings=remove.", job.sheets.len())),
                }
            }
            let changed = spec.w != job.spec.w || spec.h != job.spec.h || spec.margin != job.spec.margin || spec.gap != job.spec.gap || mat_change || spec.machine != job.spec.machine;
            job.spec = spec;
            if changed && !job.sheets.is_empty() {
                job.stale = true;
            }
            Ok(Outcome::msg(m))
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
            let mut o = Outcome::msg(m);
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
            let file = Stock::file(&ctx.library_dir);
            match a.need_str("mode")?.as_str() {
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
                            }
                            Some(rows.iter().zip(refs).map(|(row, r)| ((row.sheet, row.piece), r)).collect())
                        }
                    };
                    let note = job.book_stock_named(ctx, true, names.as_ref());
                    let mut o = Outcome::msg(if note.is_empty() { "Booked: the stock already matches the job.".to_string() } else { format!("Booked: {note}.") });
                    o.files.push(file);
                    Ok(o)
                }
                "release" => {
                    job.normalise_formats();
                    let (back, removed, kept) = stockflow::release(&ctx.library_dir, &mut job.formats, &mut job.held)?;
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
            let before = sheet_usage(&job.parts, &job.sheets[s], &job.spec, &f).2;
            nest::compact_contours(&job.parts, &mut job.sheets[s], &sp);
            job.sheet_changed(s);
            let after = sheet_usage(&job.parts, &job.sheets[s], &job.spec, &f).2;
            Ok(Outcome::msg(format!("Compacted sheet {} — remnant now starts at X {after:.0} mm ({:.0} mm gained).", s + 1, (before - after).max(0.0))))
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
                    let c = Placed { part: pi, x, y, w, h, turns, mirror };
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
            let c = next_turn(&job.parts[old.part], old)?;
            if !placement_ok(&job.parts, &job.sheets[s], &job.spec, &job.format(s), Some(i), &c, true) {
                return Err("No room to rotate the part here.".into());
            }
            job.sheets[s][i] = c;
            job.sheet_changed(s);
            Ok(Outcome::msg(format!("Turned #{} to {}°.", i + 1, c.angle())))
        }
        "turn" | "mirror" => {
            let s = sheet_idx(job, &a)?;
            let sel = items_idx(job, s, &a)?;
            let f = job.format(s);
            let (new, what) = if name == "turn" {
                let q = match a.int("angle")?.unwrap_or(90) {
                    90 | -270 => 1,
                    180 | -180 => 2,
                    -90 | 270 => 3,
                    v => return Err(format!("angle: {v}° — parts turn by 90, -90 or 180.")),
                };
                (arrange::turn(&job.parts, &job.sheets[s], &sel, q, &job.spec, &f, true)?, format!("Turned {}", names(&sel)))
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
            for c in arrange::copies(&job.sheets[s], &sel, d) {
                job.parts[c.part].qty += 1;
                job.sheets[s].push(c);
            }
            job.sheet_changed(s);
            let c = job.sheets[s][first - 1];
            Ok(Outcome::msg(if sel.len() == 1 {
                format!("Copied {} to X {:.0} Y {:.0} as #{first} (quantity now {}).", job.parts[c.part].name, c.x, c.y, job.parts[c.part].qty)
            } else {
                format!("Copied {} as #{first}–#{} (each part's quantity grew by one per copy).", names(&sel), job.sheets[s].len())
            }))
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
            let (msg, data) = machine_tech_report(&spec, &job.spec);
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
                    ranges[k] = nest::CutRange { on: true, min_r: num(lo)?, max_r: num(hi)?, quality: quality_from(q.trim())? };
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
                "undo" => match job.manual_mut(s).pop() {
                    Some(_) => Ok(Outcome::msg("Last manual pick removed.")),
                    None => Err("No manual picks on this sheet.".into()),
                },
                "clear" => {
                    job.manual_mut(s).clear();
                    Ok(Outcome::msg("Manual cuts cleared — the whole sheet is cut automatically."))
                }
                m => Err(format!("cut-order mode '{m}'?")),
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
            let text = if ctx.blm.on {
                let set = crate::blm::sheet_set(s + 1, &job.parts, &job.sheets[s], &job.spec_at(s), &plans[s], job.cuts(s), &blm_for(&job.spec, &ctx.blm, &job.name, &ctx.cfg));
                let mut t = format!("; BLM LC5 set {} — {} subprogram file(s)\n", set.nest, set.spf.len());
                for (n, body) in &set.spf {
                    t.push_str(&format!("; ==== {n}\n{body}\n"));
                }
                t
            } else {
                let marks = stockflow::remnant_marks(&crate::stock::Stock::load_cached(&ctx.library_dir), &job.stock_sheets(&ctx.cfg), &job.formats, s, &ctx.cfg.stock);
                program(s + 1, &job.parts, &job.sheets[s], &job.spec_at(s), &plans[s], job.cuts(s), &marks, &ctx.post, ctx.cfg.stock.line.edge_offset)
            };
            let mut o = Outcome::read(format!("Program for sheet {} ({} lines).", s + 1, text.lines().count()), json!(text));
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

/// Every ribbon command of the GUI, as "Ribbon group/Button label", and what
/// does the same for an agent: an operation name, or `view:` (only changes
/// what the screen shows), `gui:` (a window or picker an agent doesn't need,
/// with what to use instead), or `todo: [ID]` (not yet reachable by an agent;
/// ID = its row in FEATURES.md).
pub const GUI_MAP: &[(&str, &str)] = &[
    // --- Jobs screen ---
    ("Modules/Nesting and\nmachining", "view: switches screens"),
    ("Jobs/New\njob", "new"),
    ("Jobs/Copy\njob", "gui: copy the .snjob file, then rename"),
    ("Jobs/Save\njob", "gui: every change is saved to the job file"),
    ("Jobs/Open\njob", "gui: name the job file in each call"),
    ("Status line/Show labor\nticket", "view:"),
    ("Import/DXF", "import-dxf"),
    ("Nest/Nest\nall", "nest"),
    ("Nesting/Open\nprogram", "import-program"),
    ("Review/Times and\ncosts", "info"),
    ("Review/Export\ndata", "export"),
    ("Products/Add\nproduct", "add-shape"),
    ("Products/Parts from\nlibrary", "library"),
    ("Products/Delete\nproduct", "remove-part"),
    ("Library/Library", "library"),
    ("Library/Save to\nlibrary", "library"),
    ("Database/Connections", "gui: settings are changed in the app"),
    ("Database/Browse\ndatabase", "todo: [AGT-4] read-only Lantek SQL database browser (Windows)"),
    ("ERP / MES/Send\nnest", "todo: [AGT-7] send the nesting back to the ERP/MES (Fulcrum); the window does it (erpui.rs)"),
    // --- Nesting screen: Home ---
    ("Sheets/New\njob", "new"),
    ("Sheets/Previous\nsheet", "view: sheets are named by number"),
    ("Sheets/Next\nsheet", "view: sheets are named by number"),
    ("Sheets/Sheet\nstock", "stock"),
    ("Insert/Nest\nall", "nest"),
    ("Insert/Add\npart", "add-shape"),
    ("Insert/Open\npart", "part-info"),
    ("Insert/Import\nDXF", "import-dxf"),
    ("Edit/Undo", "undo"),
    ("Edit/Redo", "gui: undo again from the session history instead"),
    ("Modify/Collision", "gui: agents always place with collision checking on"),
    ("Modify/Snap", "gui: snapping only steers a mouse drag; agents give exact places (move, or slide to meet the next part)"),
    ("Modify/Copy\npart", "copy"),
    ("Modify/Rotate\n90°", "rotate"),
    ("Modify/Mirror", "mirror"),
    ("Modify/Remove\npart", "unplace"),
    ("Modify/Clear\nsheets", "clear-sheets"),
    ("Remnants/Compact\nto left", "compact"),
    ("Review/Cutting\npath", "render"),
    ("Review/Machine\nprogram", "program"),
    ("Appearance/Light\nmode", "view:"),
    ("Appearance/Dark\nmode", "view:"),
    ("Show/Cutting\npath", "render"),
    ("Show/Part\nnumbers", "render"),
    ("Show/Grid\n100 mm", "view:"),
    ("Panes/Machine\nprogram", "program"),
    ("Panes/Times and\ncosts", "info"),
    ("Panes/Side\npanels", "view:"),
    ("Zoom/Fit\nsheet", "view:"),
    // --- Nesting screen: Technology ---
    ("Lead-in/Lead-outs/Automatic", "leads"),
    ("Lead-in/Lead-outs/Delete", "leads"),
    ("Lead-in/Lead-outs/Machine", "leads"),
    ("Lead-in/Lead-outs/On element", "lead-at"),
    ("Micro-Joints/Automatic", "joints"),
    ("Micro-Joints/Delete", "joints"),
    ("Micro-Joints/On element", "joint-at"),
    ("Micro-Joints/Settings", "tech-set"),
    ("Loops/Automatic", "loops"),
    ("Loops/Delete", "loops"),
    ("Cut Technology/Automatic", "cut-tech"),
    ("Cut Technology/Delete", "cut-tech"),
    ("Cut Technology/By size", "cut-tech"),
    ("Text/Delete", "text"),
    // --- Nesting screen: Machining, CNC, Simulation ---
    ("Manual cutting/Pick\ncontours", "cut-order"),
    ("Manual cutting/Reverse\ndirection", "cut-order"),
    ("Manual cutting/Undo\npick", "cut-order"),
    ("Manual cutting/Clear", "cut-order"),
    ("Remnants & cut lines/Automatic\nedge", "remnant"),
    ("Remnants & cut lines/Exact\nedge", "remnant"),
    ("Remnants & cut lines/Straight\nline", "remnant"),
    ("Remnants & cut lines/Semi-\nautomatic", "todo: [AGT-6] click a free area to make it a remnant (remline::semi_auto_lines) — needs a pick point; use remnant mode=line or cut-line meanwhile"),
    ("Remnants & cut lines/Rotate\nremnant", "todo: [AGT-6] turn or mirror an empty remnant sheet (RE26); sheet-change to another stock remnant meanwhile"),
    ("Remnants & cut lines/Draw\npath", "cut-line"),
    ("Remnants & cut lines/Delete\nline", "cut-line"),
    ("Verify/Verify\nMachining", "verify"),
    ("Machining/Machine\nAll", "program"),
    ("CNC/View\nCNC", "program"),
    ("CNC/Machine\n& output", "machines"),
    ("CNC/Export\nCNC", "export"),
    ("Simulate/Beginning", "view: render shows the cut order, info the times"),
    ("Simulate/Previous", "view: render shows the cut order, info the times"),
    ("Simulate/Next", "view: render shows the cut order, info the times"),
    ("Simulate/End", "view: render shows the cut order, info the times"),
    ("Simulate/Stop", "view: render shows the cut order, info the times"),
    ("Display/Simulation\nWindow", "view:"),
    ("Display/Follow\nhead", "view:"),
    // --- Part drawing (Drawing module) ---
    ("Parts/Back to\nnesting", "view: switches screens"),
    ("Parts/Previous\npart", "view: parts are named by number"),
    ("Parts/Next\npart", "view: parts are named by number"),
    ("Draw/Points", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Draw/Shapes", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Draw/Delete\nGeometry", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Edit geometry/Modify", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Edit geometry/Transform", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Edit geometry/Figures", "todo: [DRW-1] drafting (greyed out in the app too)"),
    ("Review/Verify\nPart", "part-info"),
    ("Review/Ortho-\ngonal", "view:"),
    ("Macros/Macros", "todo: [NST-7] macros (greyed out in the app too)"),
    ("Rebuild/Still\nmissing", "view: list of unbuilt commands"),
    ("Machining/Automatic", "part-info"),
    ("Machining/Delete\ninstructions", "view: the part's machining is only shown or hidden"),
    ("Standard shapes/Recognise\nshapes", "part-info"),
    ("Contours/Contours", "part-info"),
    ("Contours/Cycles", "todo: [MAC-11] punch/milling cycles (greyed out in the app too)"),
    ("Settings/Options", "todo: [MAC-1] speed rates, postprocessor words (greyed out in the app too)"),
    ("Compensation/Kerf", "compensation"),
    ("Settings/Extraction", "todo: [MAC-6] component extraction (greyed out in the app too)"),
    ("View/Current\nstatus", "part-info"),
    ("View/Verify\nMachining", "part-info"),
    ("Display/Parts", "view:"),
    ("Zoom/Zoom\nAll", "view:"),
    ("Appearance/Light", "view:"),
    ("Appearance/Dark", "view:"),
    ("More/Steps", "view:"),
    ("Dimension Notes/Automatic", "todo: [DRW-2] dimension notes on the part drawing"),
    ("Dimension Notes/Delete", "todo: [DRW-2] dimension notes on the part drawing"),
    ("Texts/Texts", "text"),
    ("Texts/Modify", "todo: [DRW-3] move/edit one text (add/delete work through text)"),
    ("View/Text and\nDimensions", "view:"),
    ("Utilities/Bevels", "todo: [TEC-7] bevels (greyed out in the app too)"),
    ("Utilities/Contours", "todo: [DRW-4] contour utilities (greyed out in the app too)"),
    ("Utilities/Compare", "todo: [DRW-4] compare parts (greyed out in the app too)"),
    ("Utilities/Lifting", "todo: [DRW-4] lifting (greyed out in the app too)"),
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
                for call in ["ribbon_button(ui, Icon::", "ribbon_button_ex(ui, Icon::", "dropdown(ui, Icon::"] {
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
