# Mechanics read from the original

The rules below are the behaviour the app follows, each with an ID. **Observed** means seen
in the original program's behaviour or files; **estimate** (or "estimated", "guess") means our
own choice where the original's behaviour couldn't be confirmed. Some tables mark this as
[O] observed, [I] inferred, [U] unknown. The evidence for each ID is kept in the project's
private research notes.

**Reserved:** spec #122 (ByStar Fiber, sheet machines) holds rule IDs BV1–BV72 and MS1–MS8.

**Reserved:** spec #132 (Programming screen) holds rule IDs F24–F33.

**Reserved:** spec #142 (the shop's own materials) holds rule IDs MA8–MA35.

**Jobs (CAM).** In the original, the Jobs manager keeps work orders, products and nested
sheets in a database, and nesting runs in a separate nesting program. Part placement and the
usage percentages come from that nesting side; in the rebuild they remain estimates (see the
end of this section).

| # | Rule / value |
|---|---|
| 1 | Sheet list = one row per nesting record, read from table `DIS_NEST_NEST_00000100`, **sorted by nesting order `NOrder` ascending**. |
| 2 | Sheet-list fields: JobRef, IsQuote, NstRef, Quantity, CDate, WrkRef (machine), MatRef (material), SThickness, **SProfit (usage)**, **SProfitS (usage up to remnant)**, **SXMax / SYMax (furthest X/Y used)**, CNC, NOrder, ShtRef, SLength, SWidth, SPriority, SWeight, ShtRefOrg, MState, SMSQuant, MQ, UData1-3, Descrip, WrkCfg, CopyIndx, NCategory. |
| 3 | Filters: by job, machine, machine config and material when given. **Thickness matches within ± a database tolerance** (`t − tol ≤ SThickness ≤ t + tol`). Only nesting category 1 is queried. |
| 4 | **Identical copies are folded together**: rows with `CopyIndx > 1` are skipped (unless they belong to a special case), so the list shows a nesting once, and its Quantity counts the copies. |
| 5 | Sheet View thumbnail = the nesting's saved bitmap. If none, a generic picture per nesting type is used. **When the nesting is marked modified, the bitmap is altered** to show the change. |
| 6 | Status line for a selected **nesting**: `"%s / %d / %s (%s) / %s / %1.4f"`. That is nesting / quantity / machine (material) / sheet / thickness, with thickness to **4 decimals** and multiplied by the unit factor (mm or inch). The "(material)" part is left out when empty. |
| 7 | Status line for a selected **product**: `"%s / %d / %s / %s / %1.4f"`. That is reference / quantity / machine / material / thickness (4 decimals, unit factor). **The labor ticket (`/ %ld`) is appended only when a global option is on.** |
| 8 | Window title = `"<work order> - [<database>] - <app>"`, falling back to `"[<database>] - <app>"` with no order open. The screen name is prefixed as `"(<screen>) - "`. Marks are appended for a quote or a closed order. |
| 9 | **Duplicate names** (new or copied folder/job): when the name is taken, the program tries `"Name (1)"` … `"Name (9)"`. It gives up with an error after 9 tries, or when the name is longer than 255 characters (250 for jobs). |
| 10 | Copying a job imports it into a new order record with the same rule-9 naming. |
| 11 | Nesting is opened by handing the job to the separate nesting program in its nesting-import mode. Importing nestings writes `IMPORTNEST` to the `CHAPADIB.SYS` hand-off file first. Fast runs can be made hidden. |
| 12 | The sheet-parts helper runs only after a licence check. The **main window is disabled behind a wait dialog** until it finishes, polling every **50 ms**. |
| 13 | Before a session, the hand-off files are deleted: PROCESO, ORDENACT, HANDPRIN, CHGPIEZA, PIEZADIB, GEOAUXDIB, CHAPADIB, CHGSUBJOB, FIGURA, EXPRHELP, DRAWHELP, PROCHELP and PIEZADIBCODES (all `.SYS`). |

## How the rebuild uses them

- **Jobs window title** follows rule 8: `Jobs (CAM) - <job> - [<database>] - Sheet Nesting`. The
  database name is a placeholder (`Workshop`), and the job tree root reads `Jobs 'Workshop'`.
- **Sheet list and Sheet View** are in nesting order (rule 1). **Identical sheets are folded**
  into one row or thumbnail with a Quantity (rule 4). The header reads
  `Sheet View: <rows> / Total Qty: <sheets>`.
- **Usage %, Remnant %, Max X, Max Y** columns mirror SProfit, SProfitS, SXMax and SYMax (rule 2).
- **Modified sheets**: when products or sheet settings change after nesting, the thumbnails are
  dimmed and marked "Modified" (rule 5).
- **Status lines** use the exact formats of rules 6 and 7, with thickness to 4 decimals. The
  labor-ticket suffix is an option ("Show labor ticket in status"), off by default
  (rule 7; the default is estimated).
- **New job / Copy job** use the `Name (1)…(9)` rule, refusing after 9 or past 250 characters
  (rules 9 and 10).
- Rules 11–13 concern starting external programs. The rebuild nests in-process, so they are
  recorded but not imitated.

## Not in this program (still estimated in the rebuild)

- Part placement (the nesting algorithm), part gap, edge margin and the formulas behind
  SProfit/SProfitS belong to the original's nesting side and weren't confirmed. The
  rebuild's MaxRects nesting and its usage formulas (part area ÷ sheet area; part area ÷ area
  left of the remnant cut, at the furthest X + gap) are estimates.
- The thickness tolerance value (rule 3) and the unit factor's value are settings read at
  run time, so they aren't fixed values. The rebuild uses mm (factor 1).

## Confidence

Rules 1, 2, 5, 6, 7, 8, 9 and 11–13 rest on literal formats and names, so they are firm.
Rules 3, 4 and 10 rest on reconstructed conditions and are likely but not certain.

# Part technology

The Jobs manager doesn't compute technology itself; it stores technology records per part:
automatic lead-in/out, automatic micro-joints, leads, bridges, corner loops, chamfers, round
corners, cut quality and marking. Their default **values** weren't confirmed at first, so the
technology values in the rebuild are estimates (later sections refine them). The **rules** for
parts with technology are:

| # | Rule |
|---|---|
| PT1 | When a part is modified, each selected part is checked for how many technology items it has. If it has some **and the "warn" option is on**, a "part with technology" warning (name, count) is shown. |
| PT2 | If the "ask" option is also on, the program asks what to do. Its answer: **0 = put the change on a copy** (the original keeps its technology); **1 = modify the part**; **2 = modify this and all remaining selected parts without asking again**. |
| PT3 | A part without technology goes straight to "modify". |
| PT4 | Modifying a part compares old and new **material, thickness string, thickness value and machine config**. If all are equal, the data just changes. Otherwise the part's nestings are looked up. If there are any, the user is asked **once per batch** what to do with them (the answer is remembered). **3 = cancel**; **2 = remove the nestings**: each sheet is released (quantity = sheets × copies) and the nesting deleted; otherwise the nestings are kept. |
| PT5 | After a change, manufactured quantities are recounted, and the part is flagged "modified in the order" when the nesting window is open. |

How the rebuild uses them:

- Each part can carry **its own technology** (Part Properties → Technology): lead-in, lead-out,
  micro-joints, joint width and cut quality. Off = automatic (5 mm lead-in, no joints).
- The cutting plan and machine program use it. Lead-in and lead-out lengths are applied (holes
  are capped to stay inside). **Micro-joints** are spread evenly along the outer contour, where
  the beam switches off (`M08 ( MICRO-JOINT )`), jumps across with `G0`, and switches on again
  (`M07`). The joint widths are subtracted from cut length. The feed is the machine's speed for the
  contour's quality (see "Machining time and speeds"); only when the machine file gives no
  speed at all is it our typical laser speed × a quality factor (CQ3: estimated).
- **PT1–PT3**: changing the shape or size of a part with technology opens "Part with
  technology" with **Edit a copy** (PT2 answer 0), **Edit this part (remove technology)**
  (answer 1) and **Cancel**, plus "Use this answer for the rest of this session" (answer 2).
  The warning can be turned off in Options (the PT1 "warn" option).
- **PT4**: changing material or thickness while sheets are nested asks **Keep nested sheets**
  (they're marked Modified), **Remove nestings** (sheets are freed) or **Cancel**.
- Parts with technology show a **T** badge in Parts View and a Technology column in the
  product list. Micro-joints are drawn as small gaps with a yellow dot on the sheet.

Still estimated: every technology number (default lead-in 5 mm, lead-out 2 mm, 2 joints of
0.5 mm, quality speed factors), the even spacing of joints, and the exact wording of the
original's dialogs.

# DXF drawings (import, export) and automatic technology defaults

DXF import and export, and the default automatic lead and micro-joint settings.

| # | Rule / value |
|---|---|
| DF1 | The Jobs manager itself never **reads** DXF. It **writes** a part as DXF to the temporary bend folder as `<id>.dxf`, plus a one-line `<id>.txt` beside it. |
| DF2 | "Send to bending" requires the bending software to be installed, and the part's material **and** thickness must equal the product's defaults, or it stops with a message. Then the bending program is run on the DXF and the program waits for it. |
| DF3 | DXF reading has these settings: joining tolerance, ellipse discretisation, spline mode ("As polylines" / "As splines, segments from SPLINESEGS" / "from the user"), blocks, layers, conversion to system units, circle recognition. |
| DF4 | DXF config defaults: **0.01** and **0.1**, then 10 layer slots that are each on with an empty name. Read here as joining tolerance 0.01 mm and arc smoothness (chord error) 0.1 mm; which value is which is inferred. |
| DF5 | Each object read from the DXF is looked up by **layer and line type** in a layer table. The default code **13** means a normal (cut) contour. Objects on layers mapped to another code get that technology applied, e.g. marking, and texts are turned into geometry in **Arial**. |
| A1 | Automatic lead-in/out record defaults: on; two angles of **120°**; **45°**; **3.0**; **10.0**; a value of **100.0** (or −1 when a flag is set); 8 lead records with type codes **13, 1, 11, 10, 12, 10, 9, 5**. |
| A2 | Each lead record's defaults: **10.0**, **7.0**, angles **45° / 45° / 45°**, 10.0 / 10.0 when active, limit **99999.999** (no limit) or **0.5**. Field meanings weren't named. |
| A3 | Automatic micro-joint record defaults: all 10 joint records have type **−1 (none)**, plus one value of **100.0**. **By default no micro-joints are made.** |
| A4 | Lead and joint settings are kept as **12 records per cut type**, and the first type's 12 are copied to the others. With no cut types, a single record holds 99999.999 / −1.0. |

How the rebuild uses them:

- **Joining tolerance 0.01 mm** (`dxf::JOIN_TOL`, DF4). Then, among the chains still open only,
  gaps up to **0.5 mm** are closed with a straight piece (`dxf::CLOSE_TOL`, IM8: the original's
  importers have "close contours" on with 0.5 mm, DXF included) — to the nearest open end, or back
  to the chain's own start when that is nearer — and reported ("N gap(s) up to 0.5 mm closed").
  Contours that close at 0.01 mm are never touched by this, so a loose line near an outline can't
  take a piece of it. Pieces that still don't meet stay open and are reported.
- **What is read** (JOB-20): lines, arcs, circles, polylines (with bulges, old and new style),
  ellipses, splines, TEXT/MTEXT and block inserts, through the same geometry step as DWG, so DW7–DW12
  apply to DXF too: each object's own plane (an extrusion of 0,0,−1 mirrors it), blocks exploded with
  insert point, rotation, scale and grid (objects on layer 0 take the insert's layer), ellipses and
  splines within 0.1 mm, smoothed polylines without their frame points, meshes skipped. Dimensions,
  points, hatches, attributes, viewports, leaders, tolerances, wipeouts and images are annotations
  and left out without a note; other unknown objects are named in the import notes.
- **Model space only (DX1)**: objects marked as paper space (title blocks, sheet borders) are left
  out and counted in the notes ("N paper-space objects"); agents and code can ask for them
  (`dxfread::Options::paper_space`). Estimated, as DW6.
- **Arc smoothness 0.1 mm** (`dxf::ARC_CHORD_ERR`, DF4): arcs, circles and bulges are split
  so no straight piece strays more than 0.1 mm from the true arc.
- **Hidden and drafting objects start as "Leave out" (DX3)**: objects on a layer the drawing has
  **frozen** or **turned off** (a negative colour in its layer table), **invisible** objects, and
  objects drawn in a **dashed or dotted line type** (CENTER, HIDDEN, PHANTOM…; a line type with dash
  elements in the drawing's table, or any type but CONTINUOUS when it has no table) go on a row of
  their own in the drawing's layer list (the import preview's cut technology panel, IM11): `0 [CENTER]`,
  `HOLES [frozen]`, `0 [CENTER/invisible]`. The line type
  is the object's own, else its layer's (BYLAYER), else the block insert's (BYBLOCK). A block inserted
  on a frozen layer, or an invisible insert, hides all it holds. These rows start as Leave out and say
  why on hover; set one to Cut and it is cut like any layer. The import notes count what was left
  out. A dropped drawing whose only rows are such rows opens the Import drawings window too (IM12), so
  nothing vanishes unseen.
  Agents see the rows in `drawing-layers` (`left_out_by_default`) and set them with
  `layers=0 [CENTER]=cut`. A closed centreline (a bolt circle) would otherwise be cut as a hole and drop
  the middle of the plate. Estimated (own design): the original looks objects up by layer **and** line
  type (DF5) but its default table cuts everything (code 13), and it keeps invisible objects (DW13).
  DXF only for layers and line types; the DWG reader reads neither yet (FEATURES JOB-19), only
  invisible objects.
- **An object drawn twice is cut once (DX4)**: cut or marking objects with the same points (to
  0.001 mm, either way round, whatever their layer) are taken once, so a circle drawn twice isn't
  pierced and cut twice; the notes say how many ("N object(s) drawn twice in the same place, cut
  once"). Objects that only overlap in part are not merged. Estimated (own design).
- **At least 16 pieces to the full circle (DX2)**: however small the arc, so small holes stay round,
  are recognised as round and reach the LC5 as arcs. Estimated (own design).
- **Layers (DF5)**: a drawing's layers are listed in the import preview's cut technology panel
  (IM11); a dropped drawing with more than one layer opens the Import drawings window with it (IM12).
  Every layer starts as **Cut** (code 13) and can be set to **Mark** or **Leave out**. Marking lines show
  dashed on the part, are done before the part is cut (`M10 ( MARK ON )` … `M11`), and count
  toward time at the machine's marking speed (CTM9). The sample plate
  has a MARK layer.
- **Export DXF (DF1)**: Part Properties → Export DXF writes `<part>.dxf` (outline and holes on
  layer CUT, marking on MARK, `$INSUNITS` = mm) and a one-line `<part>.txt` beside it. The
  text content is a guess (reference / material / thickness); the original's format
  couldn't be confirmed.
- **Lead angle 45° (A2)**: lead-ins and lead-outs now meet the contour at 45° on the scrap
  side (outside a part, inside a hole), tilted back for the lead-in and forward for the
  lead-out. Before, they were radial.
- **No automatic micro-joints (A3)**: a part's own technology now starts with 0 joints.
  Before, the estimate was 2.

Still estimated or not used: which lead record field is the lead length. 10.0 vs 7.0 vs 3.0
are unnamed, and a 10 mm lead wouldn't fit the 5 mm part gap, so the 5 mm automatic lead-in
stays. Also the meaning of the 120° angles and the 8 lead type codes, the joint value 100.0,
the bending hand-off itself (DF2: no bending software here), and splines and ellipses (not
read yet).

# Importing part drawings: DWG, MEC, IGES, PDF, ESSI, DSTV, GEO

Besides DXF, the original imports part drawings through a family of converters, one per format,
that share one set of options. The rebuild reads each format itself (`src/formats/`) and turns it
into the same drawing the DXF reader makes, so joining, outline and holes, layer roles (Cut / Mark /
Leave out) and part data from the file name follow the DXF rules above. The file's format is
picked by its extension and checked against its content (a DWG named `.dxf` is still read as DWG).
Window: Import drawings (Jobs and Nesting ribbons; a file browser with a preview, IM10) or drop files on the
window; agents: `import-drawing` / `drawing-layers` (`import-dxf` / `dxf-layers` still work; `drawing-layers` also
gives each part's size, holes and numbered contours, as the preview shows them; `import-drawing contours=` sets
contours as the preview's panel does, IM12).

What the file itself says about the part is used: its quantity (before the file name's), and its
material and thickness are compared with the job's and reported when they differ. A file holding
several parts (a Lantek nesting, a VULCAN program) gives one part each, named by the file or
numbered `<file>-1`, `<file>-2`…

Every reader takes untrusted files: a damaged or hostile file gives a plain-word message, never
a crash or a hang, and a file over a reader's limits (millions of points or objects, endless
nesting, huge declared sizes) is refused whole — no part comes in quietly missing a contour.

### Shared converter options

| # | Rule | |
|---|------|---|
| IM1 | Part name: 1 automatic, 2 from the file (ESSI/GEO: the file name; DSTV: from header lines, DS9), 3 ask. Default 2. | observed |
| IM2 | Default material and machine are empty. A material read from a file is kept only if the material table has it. | observed |
| IM3 | Default thickness 0.8 mm and quantity 1, used when the file gives none. | observed |
| IM4 | All rotations allowed by default (plain and mirrored 0/90/180/270, other angles). | observed |
| IM5 | Imported parts go into the active job; the drawing is previewed. | observed |
| IM6 | Catalogue item off (price 0); filler part, hole filler, "unit parts only" and draft off; priority 0. | observed |
| IM7 | Linear tolerance 0.01 mm (ESSI 0.1, which is also its program unit); angular tolerance 0.1°. | observed (values), estimated (ESSI) |
| IM8 | Open contours are closed within 0.5 mm; largest radius 99 999.999; discretisation 10 mm; verify and smoothing off. | observed |
| IM9 | Detect equal parts off; order contours off (when on: outer clockwise, inner counter-clockwise). | observed |
| IM10 | The open window for part drawings shows the picked drawing on its right before it is imported, with a check box that turns the preview off (on by default, IM5) and a Configure… button for the import settings. The rebuild draws the part as the import will make it (same reading, part name and starting layer roles), with its width and height, holes, format, layers, quantity and the notes the import will give; a file no part can be made from shows its lines and the reason; a file of several parts shows them one at a time. The window opens in the folder last imported from and lists the six last ones. The check box only turns the picture on and off; it is not animated, and off leaves the preview blank. | observed (window, check box, Configure), Rebuild (what the preview shows) |
| IM11 | **The cut technology panel in the import preview** (*Cut technology* beside Preview; it opens by itself for a dropped drawing, IM12): the part's panel (CS6: rows, swatches, hover cards, All the same contours, click / box / sweep; Esc ends painting, a second Esc closes the window) on the part the shown drawing is about to make, drawn in the contour colours (CS7; Leave out faint and dashed while the panel is open). Each paint goes through the same engine function and rules as `contour-tech` and shows at once; Ctrl+Z takes back the last paint or layer change on the shown drawing, not a change to the job behind. Above the rows are the drawing's **layer rows**, each with its object count, set to **Cut**, **Mark** or **Leave out** (DF5); rows holding what the drawing hides or draws as centre / dashed lines start as Leave out and say why on hover (DX3). Setting a layer makes the part again with it, so all its lines take it (a Cut layer's contours start as CS2 says, a Mark layer's as Mark, a left-out layer's aren't on the part). A contour painted by hand that the change leaves in the same place, starting as before, keeps its paint; one on that layer takes the layer's, and the panel says how many did. While the panel is open the places column makes room and the preview grows, and the line under the part's size says what the last paint or layer change did. **Import** brings in each picked drawing with what was set for it: the same part as `import-drawing` with the same `layers` and `contours` (IM12). A picked drawing that was never shown comes in with the roles it starts with. What was set is kept, with the drawing as it was read, while the window stays open, however many other drawings are previewed; if the file or the import settings change after that, the preview and the import notes say so and the drawing comes in as it starts (Import reads the file again and compares its contents with what the preview read, so a change shows even when the file's size and time stay the same). | estimated (own design, ADR 0001: the original imports everything as cut and has the programmer fix contours afterwards) |
| IM12 | **Choosing contours with the import**: `import-drawing` takes contour choices next to `layers=`: `contours="7=mark; 1=leave-out; 2=quality 1 same"`, each the contour numbers (as `contour-tech` takes them) and a state, `quality 1-12` or `auto`, and/or `same`, painted in order with `contour-tech`'s rules (CS4, CS5, CQ6) on the part the layer roles make. The numbers are those `drawing-layers` lists for each part (`contours`: number, kind, starting state, length) with the same `layers`. Only for one drawing of one part (contour numbers belong to one part); a choice the part can't take refuses the whole import, saying why. The separate Drawing layers window is retired: a drawing dropped on the window or picked in the system's file picker that has several layer rows, or rows that start as Leave out, opens the Import drawings window with it picked and the cut technology panel open, so its layers are set before it comes in; drawings with one layer come in at once. | estimated (own design, ADR 0001) |

Every format joins at 0.01 mm and then closes gaps up to 0.5 mm (IM8, as for DXF above; how the
original closes a gap — a new straight piece or moving an end — isn't traced). Asking for a name,
equal-part detection and per-import defaults aren't built (FEATURES JOB-18).

### DWG

| # | Rule | |
|---|------|---|
| DW1 | A DWG becomes the same drawing the DXF reader makes. (The original converts it to an R12 DXF and runs its DXF import.) | observed |
| DW2 | AutoCAD R13, R14, 2000, 2004, 2007, 2010, 2013 and 2018 files are read directly; R12 and older are refused with "save it as DXF". | observed |
| DW3 | Objects are found through the file's handle map, so erased objects still in the file are never imported. | observed |
| DW4 | Compressed sections (2004 and 2007 kinds) are unpacked; the 2007 error-correction bytes are dropped. | observed |
| DW5 | A damaged object is skipped and counted; the rest of the drawing imports. | estimated |
| DW6 | Model space only (paper space is an option); block contents appear only where a block is inserted. | estimated |
| DW7 | Each object's own plane is applied: an extrusion of 0,0,−1 mirrors X (circles, arcs, polylines, texts, inserts). | observed |
| DW8 | Block inserts are exploded, up to 16 levels, with insert point, rotation, X/Y/Z scale and extrusion from the block's base point; grid inserts are expanded; objects on layer 0 inside a block take the insert's layer. | observed; layer 0 estimated |
| DW9 | Ellipses are split into straight pieces within 0.1 mm; a full ellipse is a closed piece. | estimated |
| DW10 | Splines with control points are evaluated exactly and split within 0.1 mm; splines with fit points only get a smooth cubic through them; closed when the ends meet or flagged closed. | estimated |
| DW11 | TEXT and MTEXT become texts (position, height, angle) without formatting codes; %%c, %%d, %%p become Ø, °, ±. Attributes aren't imported. | observed (attributes off), estimated |
| DW12 | Smoothed 2D polylines leave out their frame points; 3D polylines are laid flat; mesh and polyface polylines are skipped. | estimated |
| DW13 | Invisible objects are kept (as the original's converter does). The rebuild keeps them too, but on their own layer row, which starts as Leave out (DX3). | observed; row own design |
| DW14 | Numbers are millimetres, no unit conversion. | observed |
| DW15 | Bad input never crashes the program; the message names the AutoCAD version and says to save as DXF. A drawing over the import limits (5 million points, 2 million objects, a block grid of more than 10 000 copies) is refused whole — DXF too — so no contour goes missing from a part that seems imported. | estimated (limits own design) |
| DW16 | When a file can't be read, an installed ODA File Converter or `dwg2dxf` converts it to DXF, which is then read. | estimated |

### Lantek part files (.mec)

Continues MF1–MF3 below ("Lantek's own file format").

| # | Rule | |
|---|------|---|
| MF4 | Version 8: each line after the version line is the hex text of one encrypted line (Blowfish, CBC, 16-byte key, the chain starting again from zero on every line); the plain line is padded with spaces to a multiple of 8 bytes. | observed |
| MF5 | Version 9: everything after the version line is one encrypted block (AES-128, CBC, PKCS#7 padding, fixed key and IV). Copies kept in memory hold the same as base64. | observed |
| MF6 | Decrypted, the file is an ordinary version-7 file read by the same record reader. The newest plain minor version is 7.3. | observed |
| MF7 | The rebuild ships no keys. It reads them from a key file `lantek-keys.txt` in the settings folder, else finds them in the shop's own Lantek install (Settings → Lantek folder, then `LANTEK_DIR` or `C:\Lantek\Expert`): every program library in the folder is searched for 16-byte values built with four 32-bit stores in a row, once per run of the app. The first source whose keys open the file wins; of a source's candidates, the one whose output starts with a numeric record is used. Without keys the message says how to fix it. | observed (how the keys are built), own design |
| MF8 | One record per line, the first number its type: `1` line (two end points), `2` arc (centre, radius, start angle and signed sweep in degrees, negative = clockwise, end points), `3` circle (centre, radius). | observed; sweep sign estimated, checked on samples |
| MF9 | `90` bounding box, `91` counters, `92` a 4×4 placement matrix (identity in every file seen, not applied), `999` end. | observed |
| MF10 | `16 n` applies to the next element only; in nesting files 2 marks outer contours and 1 inner ones. | observed (scope), estimated (meaning) |
| MF11 | `21 n` applies to the next element only; `21 1` marks the lead-in line, which is left out. | observed (scope), estimated (meaning) |
| MF12 | Records 61–63, 71–73 (copies of a contour's first and last element) and 33, 81 (member lists) are bookkeeping and draw nothing. | observed |
| MF13 | "A" in place of a number means no value. Part data: `31 2 1137` material, the third value of `31 2 1090` thickness, `31 2 1145` part name. | estimated |
| MF14 | A file with several outer contours (a nesting) becomes one part per outer contour with what lies inside it; equal shapes become one part with a quantity. | own design |
| MF15 | Coordinates are millimetres. | estimated |

### ESSI cutting programs

| # | Rule | |
|---|------|---|
| ES1 | A line is read by how many +/- signs it has: none = function code, one = code with a value, two = straight move, five = arc; others are ignored. Each number runs from its sign to the next; empty = 0. Moves are relative. | observed |
| ES2 | One program unit is 0.1 mm. | observed |
| ES3 | Control dialects: ESSI (.CNC; comment 3/4, rapid 5/6, oxy 7/8, plasma 53/54, punch mark 9/10, powder 110/111), BURNY (no rapid or plasma codes, powder 13/14), ESAB (.MPG, adds inkjet labels 223–226), MESSER (.LOS, no plasma), VULCAN (.DAT, several parts). Picked by extension, else ESSI; agents: `dialect=`. | observed; by extension own design |
| ES4 | Arc: end and centre relative to the start; a last sign `-` = clockwise, else counter-clockwise; start = end is a full circle. | observed |
| ES5 | With the torch on (oxy or plasma) a move is cut; otherwise it goes on the active marker's layer (torch mark, pen, punch, powder in that order). | observed |
| ES6 | Straight moves are drawn with the torch on outside rapid, or with a marker on (in rapid too). Arcs are always drawn (outside any tool: as cut). Torch on/off without a move is a pierce point, not read. | observed; arcs outside a tool estimated |
| ES7 | Between comment start and end only those two codes count. | observed |
| ES8 | Kerf, tool-table and end codes don't change the shape. | observed |
| ES9 | VULCAN: a part ends at ` 0`, the file at ` NO MORE SHEETS`; ` JOB:`, ` SHEET NO.`, ` GAUGE:`, ` MATRL:` give the name `JOB-n`, thickness and material. | observed |
| ES10 | ESAB inkjet label: the line after code 224 is the text; height from the inkjet size table (40–90 mm), angle in 1/100°. | observed; placement estimated |

### DSTV plates (.nc1)

| # | Rule | |
|---|------|---|
| DS1 | Header lines are read by position; without an outer contour the plate is length × height. | observed |
| DS2 | Only plates (profile code B) import; beams, angles and channels are refused with a message. | observed |
| DS3 | Block codes in column 1, `**` a comment; data from column 3: face letter, numbers each optionally followed by a letter; a blank face or letter repeats the previous one. Unknown blocks are skipped. | observed; skipping own choice |
| DS4 | A contour point with radius r ≠ 0 starts the shorter arc to the next point, counter-clockwise when r > 0 on the front face (reversed on the rear). Points marked `t`/`w` with a radius are notches and left out. | observed |
| DS5 | Holes are circles; `l` after the depth = a slotted hole stretched by length and width and turned by the angle; `m` after y = a marked hole. | observed; angle in degrees estimated |
| DS6 | Holes with radius ≤ 0 are points (not read). | observed |
| DS7 | Only the front and rear faces are read; a contour on both keeps the front's; rear-face marking is left out. | observed |
| DS8 | Material = header line 5, quantity = line 6, thickness = the web thickness (line 13). | observed |
| DS9 | The name can be built from header lines (`4`, `1+'-'+4`); default the file name. | observed |
| DS10 | `SI` numbering becomes marked text (height from the file). | observed; marked estimated |
| DS11 | `PU` powder marking and `KO` marking are drawn as marking; saw cuts, bends and `PR` aren't drawn. | observed |

### GEO drawings

| # | Rule | |
|---|------|---|
| GE1 | TRUMPF GEO (with `#~` sections) and pen-code GEO are told apart by content; plotter-command files (PLT/PLA) are refused. | estimated |
| GE2 | TRUMPF: points are defined once and used by number; `LIN` two points, `ARC` centre/start/end and direction (> 0 counter-clockwise), `CIR` centre and radius; an undefined point fails the import. | observed |
| GE3 | Element kind 1 cut, 2 not cut, 3 marking, 4 bend line, others cut. Not-cut elements and bend lines are left out. | observed |
| GE4 | Header line 8 = 2 means inches (× 25.4). | observed |
| GE5 | Header line 19 = material, line 20 = thickness. | observed |
| GE6 | `TXT` gives height, angle, insertion point and text lines; kind 3 is marked text. | observed; field order and line spacing estimated |
| GE8 | Pen-code GEO: 7/−7 cut, −5 mark, 5 points, 8 pen up, 9/99 end; two numbers = move, five = arc (x y cx cy r, r ≥ 0 counter-clockwise; ends within 0.01 = full circle). | observed |

### IGES drawings

| # | Rule | |
|---|------|---|
| IG1 | IGES is read as fixed 80-column records (Start, Global, Directory, Parameter, Terminate); each directory entry is two records of nine 8-column fields, its status digits giving hidden / dependent / use / hierarchy; parameters are free-format with Hollerith strings. | observed |
| IG2 | Lengths are converted to millimetres by the Global section's units flag (or its unit name when the flag is 3). | estimated |
| IG3 | Read: line (110), circular arc (100; start and end within 0.01 mm = full circle), copious data (106 forms 0–3, 11–13, 63, points joined), parametric spline (112), rational B-spline (126). The rebuild also reads composite curves (102, once), ellipses (104), notes (212, as text) and subfigure instances (408, exploded). Points, dimensions and annotation forms are left out as in DXF; surfaces and solids are left out and reported. | observed (the original's list), own (extras) |
| IG4 | Each entity's transformation matrix (124) is applied, nested ones too; a matrix whose third row is the X or Y axis has its columns turned so the drawing lies in XY. Drawing views aren't used. | observed |
| IG5 | The level number is the layer name. | own design |
| IG6 | Hidden entities, physically dependent ones, parameter-space curves and members of composite curves or subfigure definitions aren't drawn on their own, so nothing is drawn twice. | own design |
| IG7 | Splines become straight pieces no longer than 10 mm and within 0.1 mm of the curve; arcs use the 0.1 mm arc smoothness. | observed (10 mm), own (0.1 mm) |
| IG8 | A drawing flat in XZ or YZ without a matrix is laid into XY. | own design |

### PDF drawings (vector)

The original has no PDF reader of its own: it runs a bundled PDF-to-DXF converter and imports the DXF.

| # | Rule | |
|---|------|---|
| PD1 | PDF units are points: 1 pt = 25.4/72 mm. A page that sets its own unit (UserUnit, in points) scales everything on it; a unit that can't be read refuses the file. | observed (points), own (UserUnit) |
| PD2 | Drawing scale: real size = drawing size × scale, default 1:1. | observed |
| PD3 | All pages go side by side, left to right, in one drawing, each starting where the previous page's media box ends; optionally one part per page (`<file> page 2`). | observed (side by side), estimated (no gap) |
| PD4 | One layer per line colour. The original numbers them 0, 1, 2… in order of first use; the rebuild names them by colour ("Black", "Red", "Colour #FF8000"; within 16/255 of a basic colour gets its name). PDF layers (optional content) or a single layer can be chosen instead. | observed (by colour), own (names, other modes) |
| PD5 | Stroked paths are the geometry; shapes that are only filled are left out unless asked for, white fills always; clipping paths aren't drawn. | estimated |
| PD6 | Curves become straight pieces within 0.1 mm. (The original's converter fits circles and ellipses back.) | own design |
| PD7 | Text stays text (simple fonts only); invisible text is skipped. | observed (text kept), own (font limits) |
| PD8 | Protected, damaged and non-PDF files are refused in plain words; a broken cross-reference table is recovered by scanning the file. | own design |
| PD9 | Form XObjects are drawn through their matrix; images and shadings are skipped and reported. | estimated |

# Part Technology tab (Lead-in/Lead-outs, Micro-Joints, Loops, Cut Technology)

The original drawing module's **Technology** tab has: Lead-in/Lead-outs (Automatic, Delete,
lead shapes), Micro-Joints (Automatic, Delete), Loops (Automatic, Delete), Standard Bevel,
Cut Technology (Automatic, Delete) and Texts.

| # | Rule / value |
|---|---|
| P1 | Automatic lead on an **outer** contour: first the contour is tested for being a standard shape (circle/oblong/rectangle-like), which gets special placement; any other contour goes to the general corner search. |
| P2 | **Corner search**: from a **position code** a reference point is taken on the contour's box (code 5 = a given point; codes over 10 carry a second digit). Corners are skipped if a side is shorter than a minimum, if a loop or micro-joint is already there, or if the corner-validity test (P3) refuses them. Of the rest, the corner **nearest the reference point** (starting from 99999.999) wins. **Corrected in v44 build analysis:** there is **no side-length test**; corners qualify when both elements are longer than the tolerance, are not tangent, carry no loop/bridge and pass the corner-validity test (see AL5). |
| P3 | **Corner validity**: the angle between the two sides is measured 0–360°, using 360° and 180° as limits. The corner must leave room for the lead angle plus a minimum (and stay under a maximum when set). Sides with no technology (e.g. marking) make it invalid. **Corrected in v44 build analysis:** type 10 needs the corner to point into the scrap with an angle ≤ max (B201) + tol; type 11 measures on the scrap side between B203 and B202 (± tol) (AL5). |
| P4 | A lead at a corner is put on the corner's **bisector**. **Corrected in v44 build analysis:** the automatic corner lead (type 10) is **not** on the bisector: it extends the first side backwards (AL7); only the odd-corner type 11 uses the bisector. |
| P5 | Holes have their own **"hole" lead type** (with a translation from an older format). |
| P6 | Lead shapes include segment, arc and **double arc** leads. |
| P7 | **Automatic micro-joints by contour size**: from the contour box w × h, *small* if `small_w ≥ w && small_h ≥ h`, *large* if `w ≥ large_w && h ≥ large_h`, otherwise *medium*. Each class gives two counts (along X, along Y), with separate values for two contour kinds. **Defaults: small 0 × 0, large 99999.999, so every contour is medium, with no joints.** **Corrected in v44 build analysis:** each class gives a **(fewest, most)** joint count, separately for holes and parts (F136–F149; −1 = F121/F122), not counts along X and Y (AM2). |
| P8 | **Automatic loops** run in two passes (two contour classes). Existing leads are recorded, removed, and put back after the loops are inserted. A loop can only go on a corner between segments or arcs (other element types are refused). |

How the rebuild uses them (Technology tab):

- **Lead-in/Lead-outs → Automatic** sets the part's technology to *corner start*.
  - Each contour starts at the valid corner nearest the chosen **Position** (a 1–9 keypad on
    the contour box, 1 = bottom left; P2). A valid corner is convex, under 175°, and has both
    sides at least 1.5 × the lead length (P2/P3, threshold estimated). With no valid corner,
    the vertex nearest the position is used.
  - The lead runs along the corner's **bisector** on the scrap side (P4).
  - **Round holes are pierced at the centre** (P5).
  - Away from corners the chosen **Type** applies: Line (45°), Line + arc, or Arc (a
    quarter-arc arriving tangent to the contour). These are reduced stand-ins for P6's shapes.
  - **Delete** removes leads and corner start.
  - One lead that comes out wrong can then be **moved by hand** in the Part view: drag the
    lead-in's pierce (red dot) or the lead-out's end (blue dot) (LE1–LE7), on one hole or
    on every hole of the same shape (LE8).
- **Micro-Joints → Automatic** sets the part's joint count from its size class (P7).
  **Sizes** edits the limits and counts. "Original defaults" restores P7's off-state, and an
  "Example 0 / 2 / 4" preset is offered. X and Y counts are added together and the joints are
  spread evenly round the contour (simplified).
- **Loops → Automatic** adds a loop outside every convex outer corner of 95° or less (the
  threshold is estimated). Each loop runs past the corner, swings round outside the part and
  comes back along the next side (`( LOOP )` in the program). It is drawn yellow on the cut path.
- **Cut Technology → Automatic** gives the part its own technology with defaults. **Delete**
  returns it to the sheet's automatic technology.
- **Apply to**: the selected part (every piece of it), or all parts. On the sheet (Nesting) it is all part types until "This part type" is picked by hand (then the selected part type, every piece of it), and the choice is kept — picking a part on the sheet does not change it; an opened part works on that part. The right-click ring on a part works on that part type. Each command shows what it will change ("1 type", "all 10"). After Auto lead-ins, Standard leads, Auto joints or Auto loops a card on the drawing says what was changed (part types and pieces); when it changed one part type of several, the card offers "Apply to all N part types" (one undo step each). Undo or redo closes the card.
- The old **Technology** tab (material, sheet, spacing) is now the **Sheet** tab.

Not built: Standard Bevel, Texts (marked/vectorised text needs a font), the exact
lead-length field, the 120° corner limits' role, the position codes above 9, and P1's special
placement for standard shapes. The sample "Square hole" part has automatic leads (Line + arc)
so its centre-pierced hole leads show on the cut path.

# Opening a part to apply technology

How the original opens individual parts from the Jobs manager:

| # | Rule |
|---|---|
| O1 | The **open part** command first checks the licence, hides the tooltip and takes the selected product. Duct products go to the separate duct program instead. |
| O2 | It writes **PIEZADIB.SYS**: the part's reference, machine, material, thickness (`%f`), record number (`%ld`), then the **list of parts opened with it**. |
| O3 | Which parts go in that list depends on a **mode**: 4 = the selected parts; 1 = every part of the job; 2 = parts on the same machine (case-insensitive); otherwise parts with the same machine, material and thickness (thickness within a tolerance). |
| O4 | While writing, it raises a **flag if a listed part has a different machine, material or thickness**. If so, a Yes/No question (naming the part) asks whether to go on. |
| O5 | **PIEZADIBCODES.SYS** repeats the same block for **9 languages**, each line set with its code page, so the drawing module can read names in any language. |
| O6 | Then the drawing module is started (or brought forward) in **launch mode 0 = drawing** (mode 5 = nesting import). Starting it is licence-gated. |

So: you double-click (or "modify") a product. The jobs manager writes the part and its companions
to hand-off files and opens them in the drawing module, where the Technology tab (see the
section above) is applied. You can step through the companion parts there.

How the rebuild does it:

- A third screen, **Part**, sits next to Jobs and Nesting. It opens by **double-clicking a
  product** in the Jobs product list or a tile in Parts View, with **Open part** in
  Nesting → Home or Part Properties, or by choosing "Part" in the top switch.
- **Open together** (Part → Home) picks the list like O3: This part only (mode 4), All
  parts of the job (mode 1), Same machine (mode 2), or Same machine, material & thickness. All
  parts of a job here share one machine, material and thickness, so the last three give the
  same list and O4's warning never triggers.
- The left pane lists the parts in this drawing (with T for own technology), with
  **Previous/Next part** and Part Properties. The window title follows the drawing module's:
  `Drawing - <part> (<machine> / <MATERIAL> / <thickness>) - [<database>]`.
- The canvas shows the part large, with everything its technology does: lead-in shapes,
  lead-outs, micro-joints, loops, pierce points with the cut order, the dimensions and a
  summary. The **Technology** tab opens by default and acts on the shown part.
- The hand-off files (O2/O5) and the separate program (O6) aren't imitated: the rebuild opens
  the part in the same window.

## Contour states: cut, mark, no cut, bend line, leave out

Every contour of a part, closed or open, has a state that says what the machine does with it
(words in `GLOSSARY.md`; ADR 0001: the programmer sets it per contour). The original keeps the same
kinds per element (cut, marking, no cut, bending geometry) and imports a drawing with everything
cut; the rebuild keeps open lines too and gives each contour a starting state. Agents:
`part-info` lists the contours, `contour-tech` sets them. On the part's drawing screen the cut
technology panel paints them (CS6, CS7); the same panel in the import preview paints them before
the part is made, with the drawing's layers (IM11), and `import-drawing contours=` does it for agents (IM12).

| # | Rule | |
|---|------|---|
| CS1 | A part's contours are numbered holes first (every closed contour inside the outline, marked ones too), then the outline, then the open lines. Each has a state: **Cut**, **Mark**, **No cut**, **Bend line** or **Leave out**. A left-out contour keeps its number, so it can be set back. The states are saved with the job and with a library product; a job or product saved before contour states opens exactly as it was, its holes and outline Cut and its marking lines Mark. | observed (the kinds); own design (numbering, open lines kept) |
| CS2 | Starting states, after the layer roles (a layer set to Leave out, including DX3's hidden and dashed rows, puts nothing on the part; a Mark layer's contours start as Mark): a closed contour is **Cut**; an open line is a **Bend line** when it is drawn dashed (a DX3 row set to Cut), lies on a layer whose name has BEND in it, or is straight (within 0.01 mm) and runs from edge to edge of the part (both ends within 0.5 mm of the outline); any other open line is **No cut**. An open line is on the part when the middle of one of its pieces is inside the outline; the others are left out and counted in the import notes, as outlines outside the part are. Dashed and bend-layer pieces are joined only with each other, never with the other cut lines, so a bend line ending on an outline corner can't break the outline, while a hole or outline drawn dashed (its row set to Cut) or on a bend layer still closes and is cut. Where three or more line ends meet at one point (a line across the part ending on the outline's corners, or edges split where a bend line meets them), the lines joined there close along their outside boundary (found by going round the outside, always taking the rightmost turn; gaps up to 0.5 mm in them are closed first, each loose end to the nearest end, as IM8), and the lines inside it stay open lines with the starting states above; drawings where no three ends meet join exactly as before. | estimated (own design, ADR 0002: the original imports everything as cut, and cutting an open line can split the part) |
| CS3 | Only **Cut** contours are cut: they alone are nested, given leads, joints and kerf, pierced, timed and checked by Verify Part. **Mark** contours, open or closed, are marking paths: marked before the part is cut and timed at the marking speed. **No cut** and **Bend line** contours are drawn on the part and on the sheet and never machined: on screen in their colours (CS7); in the agents' pictures (`render`) No cut is a thin grey line and a Bend line dashed purple. Verify Part names holes by these contour numbers. **Leave out** contours aren't drawn or machined. | observed (what each kind does); estimated (colours, our own) |
| CS4 | Setting a state: the outline is always Cut (refused, "without it there is no part to cut out"); an open line can't be set to Cut yet (refused: it could split the part, FEATURES TEC-22); a built-in shape's contours are always cut. A lead-in or joint placed on a contour stays on it, and goes when the contour is no longer cut; so do contours picked by hand for the sheet's cut order. When the cut contours change the job is marked as changed since its nest (a part nested in a hole that is no longer cut would overlap it). | estimated (own design) |
| CS5 | **All the same contours** (`same=true`) also sets every contour equal to a picked one: both open or both closed, the same number of points, and with their boxes' lower-left corners together every point within 0.01 mm, run either way and, closed, from any point. A turned or mirrored copy isn't the same; the outline is never included. | estimated (own design; the original's equal-contour test, DRW-6, isn't studied) |
| CS6 | **The cut technology panel** on the part's drawing screen (Technology → *Paint contours*; an imported part only, a built-in shape's contours are always cut): rows **Cut · quality by size** (cut, the quality from the part's size ranges and the machine's size bands, CQ1), then each quality the machine offers by its name (CQ4; cut at that quality by hand, CQ6), **Mark**, **No cut**, **Bend line** and **Leave out**, each with its colour swatch (CS7) and a hover card: its title, an icon drawn like the row's contours and one plain sentence (Mark's says *etch*); and **All the same contours** (CS5), off by default. With a row chosen, a **click** paints the contour under the pointer (the nearest line within the pick aperture); a drag that starts on a contour **sweeps**: every contour the pointer passes over is painted as it is passed; a drag that starts off the contours draws a **box** and paints the contours wholly inside it when let go (dragged to the left, every contour it touches, SW1). Each paint goes through the same engine function and rules as `contour-tech` (CS4, CS5, CQ6) and shows at once, with no Accept; a click, a box or a whole sweep is one undo step (Ctrl+Z). A box or a sweep paints the contours that can take the row and leaves the rest as they are, naming them with why (the outline is always cut, an open line can't be cut yet); a click on one says why. Leave out contours are drawn faint and dashed while the panel is open, so they can be painted back. **Esc** ends painting: no row is chosen and the pointer picks elements again. | estimated (own design, ADR 0001; the original's drawing-screen tools for this aren't studied) |
| CS7 | **Contour colours**, the same on the part's drawing screen (both looks), on the sheet, in part pictures and on the panel's swatches: a Cut contour in its cut quality's colour (CQ5), for the quality it is cut at (CQ1, CQ6); a **Mark** green (0 255 0, the Drawing module's marking colour, DR3); **No cut** grey (150 150 150), thin; a **Bend line** purple (147 51 234), dashed. **Leave out** contours aren't drawn, except faint and dashed (96 96 104) on the part while the cut technology panel is open. Cut at the size bands' quality has no swatch colour of its own: its row shows the qualities as stripes, and each contour the colour of the quality its band gives. | estimated (own colours, FEATURES DRW-8, DRW-9; the original mixes each element's colour with its technology's colour, DR4/DR5) |

## A part's cut order

The order a part's contours are cut and marked in (words in `GLOSSARY.md`: Cut order, Outline,
Mark). The programmer sets any part of it by hand; whatever isn't set follows the automatic
order, and the outline is always cut last. Agents: `part-cut-order` sets it, `part-info` shows
it, every sheet and machine program follows it (CO8–CO10), and on the part screen Machining →
Contours sets it and the drawing shows it (CO11–CO13). Own design throughout: how the original
keeps and edits a part's order wasn't studied for these rules (ADR 0001, ADR 0002).

| # | Rule | |
|---|------|---|
| CO3 | **A part's cut order set by hand** is a list of its contours by the numbers `part-info` shows (CS1), kept apart from the contours themselves, so setting it never renumbers anything: leads, joints, picked starts and qualities set by hand stay on the same contours. The part cuts the contours in it first, in its order, then the rest in the automatic order (CO5), the outline always last (CO4). `part-info` gives each cut or marked contour its place (`cut_order`, from 1) and whether it was set by hand (`cut_order_by_hand`), and the whole order (`cut_order`: `contours`, `by_hand`). Only an imported part takes one: a built-in shape follows the automatic order, and ordering it is refused, saying to save it as a drawing (`export-part-dxf`) and import that. Saved with the part in the job and in its library product, command by command, so undo works after reopening; a job or product saved before has none and opens as it was, and a part with none writes nothing for it. | estimated (own design) |
| CO4 | **The outline is always cut last**, or the part drops before its holes are cut. A command that names any contour after the outline (`contours=4,1` with 4 the outline) is refused with that reason and changes nothing; the outline named last, or on its own, is taken, and contours added by later commands go in just before it. Marks can go anywhere before the outline. | estimated (own design) |
| CO5 | **Commands add to the end of the order**: contours by number in the order given (a range runs the way it is written: `4-2` is 4, 3, 2), all marks, all cut contours (the outline too), or all contours cut at one cut quality (CQ1, as `part-info` shows it). A contour already in the order is skipped, and so is one named twice in a command. A bulk command adds only contours not yet ordered, in the automatic order among themselves. The **automatic order** is the marks (open or closed) in contour-number order, then the cut holes in the machine's hole order (CO14; in contour-number order under None), then the outline: with no order set by hand, the marks first, as before. Only cut or marked contours have a place: naming a No cut, Bend line or Leave out contour is refused, saying to cut or mark it first. A command that adds nothing changes nothing; all marks on a part with no marks, or a quality none of its contours is cut at, is refused, naming what it has. | estimated (own design) |
| CO6 | **Undo** takes back exactly the last command, wherever its contours went in the order (in front of the outline too); with no order set by hand it is refused. **Clear** empties the order, so every contour follows the automatic order; on an empty order it changes nothing. | estimated (own design) |
| CO7 | **Technology changes after ordering** (`contour-tech`, the cut technology panel, CS4): a contour set to another state leaves the order set by hand. Set to No cut, Bend line or Leave out it has no place; a cut contour set to Mark joins the automatic marks; a contour newly set to Cut (from Mark, or back from No cut) joins the automatic part. A contour set to the state it has, or given only a quality, keeps its place. A command whose contours have all left is gone, so undo takes back the one before it. | estimated (own design) |
| CO8 | **Every sheet cuts each part in its cut order** (CO3): the contours set by hand first, in their order, then the rest in the automatic order (CO5), the outline always last, on every sheet and every copy of the part. The order names contours by number, not by where they are, so it is kept exactly when the part is turned (90°, 180°, 270°) or mirrored on the sheet. The G-code and the LC5 (each part's subprogram) write the contours in that order, and the sheet's cut time and rapid length follow it. A part with no order set by hand is cut as before: its marks first, then its holes in the machine's hole order (CO14; in the drawing's order, exactly as before, under None), the outline last (a part saved before contour states keeps its marks in the order they were saved in). | estimated (own design) |
| CO9 | **Marks are made at their place in the part's cut order**: a mark set by hand between two holes is made between them, and marks not set by hand follow the automatic order after the contours set by hand (CO5). With no order set by hand, all of a part's marks are made before its first cut, as before. A part's marking texts (T1) aren't contours and have no place in the order: they are made with its first cut on the sheet, after any marks due there, as before. The G-code writes each mark just before the contour it comes before, and the LC5 subprogram lists the part's marks and contours in the same order. | estimated (own design) |
| CO10 | **Contours picked in a sheet's Manual cutting win on that sheet** (MM1–MM4): they are cut first there, in the picked order, and the rest of each part follows its own cut order without them, the outline last; other sheets keep the part's order. A mark is made before the part's next cut as soon as every cut ahead of it in the part's order is done, so a mark ordered first is still made before a picked contour. Parts in holes go first whatever the order: on their own by depth (CO1), with picks by being cut whole just before the hole (CO2), and a part cut whole takes its holes in its own order, each after the parts sitting in it. Verify Machining checks the result as before. On the LC5 each part is one call, made where its outline is cut (CO2): a contour picked ahead of another part is cut with the rest of its own part there, after that other part; the G-code keeps the picked order (FEATURES MAC-67). | estimated (own design) |
| CO11 | **Setting the cut order on the part screen** (Machining → Contours; an imported part only, CO3). **Manual Contours** starts picking, which takes the pointer from the other tools until Esc; the status line says *Pick contours in cut order — Esc ends*. A **click** adds the contour under the pointer: a No cut or Bend line is refused with the reason, a Leave out contour isn't drawn and can't be picked, open space does nothing. A drag that starts on a contour is a **sweep**: it adds each cut or marked contour in the order the pointer reaches it, each once, passing over lines with no place and contours already ordered (skipped, and named); it goes in when the button is let go, and while it is held the drawing numbers the contours as they will be. An outline swept before another contour is left out of that sweep, with the reason (CO4); swept last it is taken. Picking sets only the order, never where a cut starts: leads and picked starts stay. **All Marks**, **All Cut** and **All Contours of one Quality** (the machine's qualities, each by its name and colour, CQ4, CQ5, with how many of the part's contours are cut at it) are the bulk commands of CO5. Every click, sweep, box and bulk command is one command, so one Ctrl+Z takes it back, as `part-cut-order undo` does. Esc ends picking; a sweep still held goes in first. **Machine part** (Automatic) clears the order set by hand (CO6), one Ctrl+Z step; Automatic Contours and Parts leave it as it is, the rest following the automatic order. On a built-in shape Manual Contours, All Marks and All Contours of one Quality are greyed out and their hover cards say why; All Cut, Automatic Contours and Parts show its machining. Every Contours item has a hover card: its name, its picture and one plain sentence. | estimated (own design) |
| CO12 | **A box** while picking (a drag from open space) adds the cut and marked contours wholly inside it (dragged to the left, those it touches, SW1), in the automatic order among themselves as the drawing numbers it (CO5: marks, holes, outline; the holes in the machine's hole order, CO14), skipping those already ordered; a box round the whole part puts the outline last. | estimated (own design) |
| CO13 | **Numbers and rapids**: whenever the part's machining is shown (Machine part, not Clear machining), each cut contour and mark shows its place in the part's cut order (CO3, as `part-info` gives it) beside where it starts (a cut contour at its pierce, a mark at its first point), with dashed **rapids** from where each one ends (its lead-out's end, a mark's last point) to where the next starts. The numbers and rapids of contours ordered by hand are solid cyan (34 211 238, our own colour); the automatic ones after them grey (140 144 156). A built-in shape shows its automatic order the same way. | estimated (own design) |

## Selection windows: dragged right takes what is inside, dragged left what it touches

A box drawn by dragging from open space works the same everywhere it is drawn, as in CAD
programs. Own design, asked for by the shop; the original's nesting and drawing screens
aren't studied for it (FEATURES DRW-10).

| # | Rule | |
|---|------|---|
| SW1 | **Which way a box is dragged says what it takes.** Dragged to the **right** (let go to the right of where the button went down) it is a **window**: it takes only what lies **wholly inside** it. Dragged to the **left** it is a **crossing window**: it takes **everything it touches**, even what sticks out of it. Straight up or down counts as a window. The same on the sheet (parts, HM7), in the Part view (lines, arcs, circles and texts, E2, T14), with the cut technology panel (CS6, the import preview's too, IM11), while picking a part's cut order (CO12) and in Manual cutting (MM9). Shift or Ctrl add to the selection where they did before. Zoom Window only frames the view, whichever way it is dragged. | estimated (own design) |
| SW2 | **Lines** (an element, a contour as drawn): a window takes one when every point of it is inside, both ends of each segment and every point along an arc; a crossing window when any stretch of it passes through or touches the box. A box that touches no line takes no line, even inside a hole. | estimated (own design) |
| SW3 | **Parts and texts** are filled: a window takes a part when its whole outline is inside, a text when all four corners of its box are (T14). A crossing window takes either when it touches any of its edges (a part's holes too) or lies on its material, so a small box dragged to the left on a part takes it, while one inside a hole of the part, touching no edge, doesn't. | estimated (own design) |
| SW4 | **How it looks while dragged:** a window has a solid light indigo edge, a crossing window a dashed green edge, each with a faint fill of its colour; bright in both themes, as the drawing canvas is dark in both. | estimated (own design, the usual CAD look) |
| SW5 | **A box that takes nothing says so:** a window "No part lies wholly inside the window — drag to the left to take the parts it touches." (the same for contours), a crossing window "The window touches no part." Agents: `cut-order mode=parts box="x,y;x,y"` takes the parts wholly inside whichever order the corners are given; with `touching=true` it is a crossing window. | estimated (own design) |

## Selecting lines, arcs and circles, and placing leads on them

How the drawing module picks elements and places a lead on a picked element.

| # | Rule |
|---|------|
| E1 | A click picks the element whose piece falls inside a small **capture square** around the cursor. A quick box check rejects elements first; then a segment counts if it crosses a side of the square or has an end inside it. |
| E2 | **Window (box) selection** takes only elements lying **fully inside** the box (both ends of a segment inside). |
| E3 | Of the captured elements the **nearest** wins. When two are equally near (within **1e-6**), the one whose **start point** is nearer wins. Elements that already carry a loop or a micro-joint (bridge) at that spot are skipped. The search starts from a "far" distance of **100000.999**. |
| E4 | A lead-in put **on an element** goes at the element's **middle**: length × 0.5 along a line, the middle angle of an arc. An element **shorter than the lead** is refused. |
| E5 | Arc leads have a "limit length to centre" option, so on small holes the lead never passes the centre. |

How the rebuild uses them:

- The Part view splits each contour into elements: a turn of more than **20°** ends an element
  (estimate). A run with two points is a **Line**, a longer run an **Arc**; a contour with no
  corners is one **Circle**. Holes come first, then the outline, the same order the cut plan uses.
- Hovering highlights the element under the cursor and shows its type and length. Click selects
  it; Shift+click adds or removes it; dragging a box to the right selects the elements fully
  inside (E2), to the left every element it touches (SW1, SW2).
  Capture uses a **6 px** square (estimate; the original's aperture is a setting) with the
  nearest and tie rules of E3.
- **Lead-in/Lead-outs → On element** puts the contour's start in the middle of the one selected
  element (E4). It's refused if the element is shorter than the lead-in. A new pick replaces
  the old one on the same contour. **Micro-Joints → On element** puts a joint in the middle of
  each selected element. Both Delete buttons also remove these picked points.
- In the cut plan a picked start beats the automatic corner start for that contour, and picked
  joints are cut together with the automatic ones.

## Side panes (docking)

The original's side panes come from a third-party docking-pane manager; the original only sets
it up and adjusts it.

| # | Rule |
|---|------|
| D1 | Manager set-up: **translucent docking preview** on, **docking stickers** on, **pane contents shown while dragging**, **default pane options 16** (the title-bar **menu button**), **keyboard navigation 15** (all pane shortcuts). |
| D2 | The ▾ pane menu has four items: **Floating**, **Docking**, **Auto Hide** and **Hide**. Hide is disabled when the pane has option 1 (can't close); Floating is disabled with option 4 (can't float). For an auto-hidden pane, Auto Hide is ticked and Floating/Docking are disabled. Otherwise Docking is ticked unless option 0x20 (can't dock) is set, in which case Floating is ticked. |
| D3 | **Floating**: if the pane may dock, it is floated (if not already) and gets option **0x20**, so it no longer docks. **Docking** only clears 0x20. **Auto Hide** toggles auto-hide. **Hide** closes the pane. |
| D4 | The four main work panes get option **4**, so they **can't float**. Panes made on demand are created at **250 × 140** and tabbed with a fixed neighbouring pane. |
| D5 | When a pane of that group closes and only one is left open, the last one gets option **1** (**can't close**). When a second one opens again, option 1 is cleared. |
| D6 | The layout is saved under the name **"ExpertLayout"** and loaded again at start (after loading, the pane titles are set again). |

How the rebuild uses them (`dock.rs`):

- Every side pane's title bar has **▾** (the D2 menu, plus Collapse/Expand as an extra), a
  **pin** for Auto Hide (floating panes have no pin) and **×** (Hide).
- **Auto Hide** folds the pane into a vertical tab on its edge. Pointing at the tab slides the
  pane out over the work area, and it slides back in 0.6 s (estimate) after the pointer leaves.
- **Dragging** a title turns the pane into a floating window that follows the pointer with its
  contents (D1). Stickers appear: one at the middle of the left and right edges, and up/down
  stickers over the pane under the pointer. Dropping on a sticker docks the pane there, with a
  translucent preview of the place first. Dropping anywhere else leaves it floating. A pane set
  to Floating (option 0x20) never docks until Docking is chosen (D3).
- Double-clicking a title floats or docks the pane (the toolkit's default; the original doesn't
  turn it off).
- D5 is applied per screen: each screen's **last open side pane can't be hidden** (its × and
  Hide are greyed out).
- **Ctrl+Tab / Alt+F6** (Shift = backwards) step through the panes (D1, option 15). The active
  pane's title is tinted, and an auto-hidden pane slides out.
- The layout is saved as **ExpertLayout.txt** in the user's app-data folder whenever it changes,
  and loaded at start (D6). Screenshots and tests skip it.
- A pane shown again from View → Side panels comes back at the original's new-pane height of
  **140** (D4). Docked columns start **250** wide.
- Not imitated: tabbing several panes into one tabbed group, and the toolkit's own drawing theme.

### Docking events (what happens after a pane is moved, closed or right-clicked)

What the original does when the docking manager reports a pane event.

| # | Rule |
|---|------|
| D7 | **Right-click** on a pane (or on the selected tab of a tabbed group) opens the **same pane menu** as ▾ (D2) at the pointer. |
| D8 | When the ▾ menu button is pressed, the menu opens at the button. |
| D9 | When a main work pane **closes**, the next one still open (in a fixed pane order) is **shown and activated** and repainted. Then the last-pane rule D5 is applied. |
| D10 | A pane's contents are created the first time it is shown, then attached to it. |
| D11 | After certain pane actions (moves, docks, closes and similar), the window posts itself a follow-up command. After one other action it hides and shows its client window to repaint it. After another it remembers which work pane is active (1–4). The action names belong to the toolkit, so the meaning of the follow-up command isn't confirmed. |

How the rebuild uses them:

- A right-click on a pane title opens the pane menu (Floating / Docking / Auto Hide / Hide,
  then Collapse) with the Side panels list and Reset under it (D7). ▾ opens the same menu (D8).
- Hiding a pane activates the next open pane on that screen (D9): an auto-hidden one slides out
  and stays out for 2 s (estimate), and a collapsed one opens.
- Contents are drawn only while a pane is shown, so D10 needs nothing extra. Layout changes are
  saved as soon as they happen, which stands in for the follow-up command (D11, inferred).
- A pane is drawn in one place per frame: a pane that floats, docks or hides mid-frame appears
  in its new place on the next frame. This is the rebuild's own rule; it fixes the crash when
  panels were moved.

## Dropping parts by hand (ghost)

While a part is dragged the original draws a copy of the part's outline that follows the
pointer, and only writes the part into the sheet when the drag ends.

| # | Rule |
|---|------|
| G1 | The ghost's outline follows the pointer: every frame the current point moves the whole shape list. |
| G2 | The dragged point is **clamped** to the allowed area (min/max X and Y, each switched on by a bit) instead of the move being refused. |
| G3 | Turning while dragging: the angle from the start point to the pointer is rounded to the rotation step (`Round(angle/step)·step`, step from the settings, 1° when the setting is larger than allowed), normalised to 0–360 and negated when mirrored. |
| G4 | Whether the dragged part may turn at all is a switch on the drag. |
| G5 | Mirror toggles a flag on the drag; turning/mirroring is applied to the ghost before it is dropped. |
| G6 | **Approach snap**: points of the moving part are projected onto the nearest element of the target (line: perpendicular foot; arc/circle: along the radius) and the part is moved so they sit the separation apart. A sliding variant only works between two straight segments. |
| G7 | The Nesting group has **Place** ("Place the selected part on the sheet") for dropping by hand and **Semi-Automatic** ("Nest one part or module only") for letting the program find a spot for one part. (*Unitary* is different: it puts a part on a sheet of the part's own size.) |
| G8 | Next to them, the Transform group offers **Place Near**, **Fit** to the sheet borders and **Expand**. |

How the rebuild uses them (Nesting screen, Parts View):

- **Click a part tile** (or drag it onto the sheet): a see-through ghost of the part follows the
  pointer over the sheet (G1). The tile shows a *Dropping* pill. Click the sheet to drop a copy;
  it keeps dropping copies until all are placed (hold Shift to keep going), Esc / right-click /
  clicking the tile again stops. Dragging a tile drops one copy where you let go.
- The ghost is centred on the pointer and **clamped inside the sheet margin** (G2).
- **R** (or Space) turns it to the next quarter turn the part allows, **M** mirrors it — only
  the turns allowed in the part's Angles chips (G3–G5).
- Within 14 px of a neighbour's side or the sheet margin it **snaps one part gap away** (G6, on
  the parts' boxes; each axis snaps to its nearest target, one axis alone if both would clash).
- Green outline = room to drop; red = it would overlap or leave the sheet (with Collision on).
  The gap halo around it shows the part gap it keeps.
- **Enter** places one copy by itself at the first free spot, lowest then leftmost — our take on
  *Semi-Automatic* (G7; the spot order is estimated).
- Dropping more than the ordered quantity raises the order by one, like *Copy part*.
- Not imitated: turning the ghost to other angles while dropping it (it turns in quarter turns;
  once placed, a part takes any angle, HM14–HM15),
  snapping on the true outline instead of the box, and the sliding snap.

## Moving parts by hand (selection, keys, snap)

How placed parts are picked, nudged, slid, dragged, turned, mirrored and copied on the
Nesting screen. Code: `engine/arrange.rs` (rules), `arrange_ui.rs` (mouse and keys).

| # | Rule | |
|---|------|---|
| HM1 | **Keys on the nesting screen.** Alt + arrow moves the selection by the *movement value*; Ctrl + arrow moves it *to the end* in that direction; Ctrl+R sets the movement value. Ctrl+Q / Ctrl+W turn by the rotation step, + and − (Ctrl+E sets the step); Ctrl+1/5/0/3/4/9 turn 1/5/10/30/45/90°; Ctrl+T turns by a typed angle; Ctrl+D flips the direction of the fixed-angle keys; Ctrl+A / Ctrl+S mirror (X / Y); Ctrl+G turns to the minimum rectangle; Ctrl+M Move, Ctrl+C Copy, Del Delete; F2 orthogonal; F4–F8 zooms. In the original these keys work on the parts picked with Move (Ctrl+M); the rebuild applies them to the selected parts. | observed (the original's key table and its handlers) |
| HM2 | **"To the end" stops one part gap before the next part, or at the sheet margin** — other parts (and in the original also remnant cut-outs and clamps) stop it whether Collision is on or off; the moved parts go as one rigid group. The original slides on its strip profile of the true contours (0.1 mm steps, X on strip edges) and refuses a move that would push the group sideways by a strip or more; the rebuild slides on the exact contours (holes too: a part in another part's hole stops at the hole's edge) and never moves sideways. Parts the selection already sits too close to don't hold it back when it moves away from them. | observed (handlers); exact-contour stop and the "too close" release: rebuild |
| HM3 | **A move by the movement value has no collision test**: the group moves exactly that far and may end up closer than the gap or over another part. The rebuild keeps it inside the sheet and warns on the status line when it ends up closer than the gap. Agents' `slide by=` stops at the gap instead (agents never overlap parts). | no test: observed; staying on the sheet and the warning: rebuild |
| HM4 | The movement value is kept in mm and shown in the user's units. Factory value **10 mm**; on first start the rebuild takes the value of the Lantek install it finds. | observed (factory configuration files) |
| HM5 | A group of parts **turns about the centre of the box around all of them**. Turning keys don't test for collisions at the key press; the original then looks for a nearby spot without collision when the part is put down. The rebuild turns at once and searches rings around the turned group (every half part gap, at least 1 mm, out to the selection's size) for the nearest free spot. Positive angles turn counter-clockwise; when the original's turn-direction setting is off, its fixed-angle keys (Ctrl+1…9) turn clockwise — the rebuild's Ctrl+9 follows the same setting, taken from the install on first start. | centre, no test at the key press, direction: observed; search pattern: estimate |
| HM6 | **Mirror** the selection left↔right (Symmetry X) or top↔bottom (Symmetry Y), about the centre of its box; only parts whose rotations allow mirroring. | commands: observed; about the box centre: estimate |
| HM7 | **Window selection**: pressing on empty sheet and dragging draws a selection window; the parts **wholly inside** it are selected. Dragged to the left it is a crossing window and selects every part it touches (SW1, SW3). | window: observed (manual); wholly inside: estimate; crossing: own design |
| HM8 | With Collision on, a part dragged into another doesn't overlap it: the original moves it back to the correct distance. The rebuild lets a blocked drag **slide**: from the last free position toward the pointer, along X then Y or Y then X (whichever ends nearer), stopping one gap before what is in the way. | keeping the distance: observed (manual); sliding: estimate |
| HM9 | **Snap while dragging** placed parts: within the snap reach (14 px on screen) of a part — measured at the part gap, on the real contours — or of the margin, the selection moves in to sit exactly there, on each axis toward the nearer one. Hold Alt to drag without snapping. | extends G6; contour snap and reach: estimate |
| HM10 | **Copying a selection**: Ctrl+D (ribbon Copy) puts a copy of all selected parts, keeping their layout, at the first free spot by their boxes, left first, then lowest; Ctrl + drag drags a copy instead. In the original each copy adds one to its part's order; the rebuild's copies (these, the quick ring's Copy and a pattern, HM19) use the pieces still to place first, and only a piece beyond the order makes it grow by one, as dropping a part from the parts list does — without asking, and the status line says when a copy was an extra. | Copy command and the original's counting: observed; spot order, Ctrl+drag and counting from the pieces left: rebuild |
| HM11 | Parts being dragged can show the **part gap** as a dashed outline around them (the original's option to draw the moving parts grown by the part gap). Off at the factory; on first start the rebuild takes the install's setting. | observed option and default; look: rebuild |
| HM13 | **Plain arrow keys move the view** (left/right/up/down) and **Page Down / Page Up zoom in / out**; the mouse wheel issues the same commands in the original. The rebuild pans 80 px per press and zooms by 2 (the original's default keyboard zoom factor). | keys: observed; pan step: estimate |
| HM12 | **Settings → Nesting**: the units (mm or inches) every length in the app is shown and typed in (UN1); new jobs start with the part gap and edge margin set there; the movement value, snapping, snap reach and the gap outline are set there too. | rebuild |
| HM14 | **Placed parts keep any angle**, not only quarter turns: a part is mirrored, then turned counter-clockwise by its angle, and its footprint is the box of its turned outline. Its holes, texts, cut path, leads, micro-joints and program all follow. A selection turns about the centre of its box (HM5); one part turns about the middle of its own box. Ctrl+Q / Ctrl+W turn by the rotation step (HM16), Ctrl+1/5/0/3/4/9 turn 1/5/10/30/45/90°, and Ctrl+T types an angle: the part clicked first goes to it and the others turn with it. Angles other than 0/90/180/270° are only for parts that may turn 90°: a part kept to 0° and 180° (grain) keeps to them. Agents: `turn angle=` (turn by) or `to=` (turn to). A job with a part at such an angle is saved as job layout 2, which builds from before free angles refuse to open instead of cutting the part at the wrong angle; other jobs stay at layout 1. | keys and typed angle: observed (HM1); "may turn 90°" deciding other angles: estimate; the job layout: rebuild |
| HM15 | **Turning by dragging.** A round handle sits above the selection when every selected part may take any angle. Dragging it turns the selection about the centre of its box by the angle the pointer goes round that centre, rounded to the rotation step (G3: Round(angle / step) · step), or to 0.1° when the step is Free or Alt is held. The parts show turned while the button is held, outlined red when they would overlap or leave the sheet. Letting go turns them as HM5 does (to the nearest free spot) in one undo step; Esc puts them back. | rounding: observed (G3); the handle: rebuild |
| HM16 | **Rotation step**: Free, 5°, 10°, 15°, 30°, 45°, 90° or a typed step (0.1° to 180°), kept in Settings → Nesting. A new install starts at 15°. With Free, Ctrl+Q / Ctrl+W and the turn bar's buttons turn 1°. | the step for Ctrl+Q / Ctrl+W: observed (HM1); the choices and the 15° start: rebuild |
| HM17 | **The turn bar** sits under the selected parts (above them when there is no room below). It has turn left and turn right by the step, the angle of the part clicked first (type another and press Enter) and the step chips. Its × hides it; Ctrl+T or Settings → Nesting shows it again. | rebuild |
| HM18 | **Copy pattern (Ctrl+C)**: the selected parts go on the pointer as a pattern, spacing kept, held by the part under the pointer (or the first selected); they follow it and snap like a drag (Alt: no snap). A click drops a copy where it fits and the pattern stays on the pointer for the next; the copy just dropped is selected. Each drop is one undo step. Esc, a right-click or the ribbon's Copy pattern again puts it away. In the original Ctrl+C is the Copy command (HM1); the pattern that stays and its counting are the rebuild's. | rebuild |
| HM19 | **A pattern's copies use the pieces still to place**: the order grows only for pieces beyond it. Beside the pointer a label says what one copy uses, how many are left and how many whole copies that makes ("2 × Gusset per copy · 16 left · 8 copies to go"); it turns amber when the next copy is short. | rebuild |
| HM20 | **A copy short of pieces asks before it drops**: *Place just the N left* (only that many go down — the piece held first, then the nearest to it; the order stays; while held, the pieces that won't go down are drawn faint), *Add N extras* (the whole copy; the order grows by the pieces beyond it) or *Cancel* (not dropped, still on the pointer). Enter takes the first choice, Esc cancels. The answer holds until the pattern is put away. Agents' `repeat` refuses a short copy unless `extras` says which. | rebuild |
| HM21 | **A pattern is put away when none of its parts has a piece left** after a drop (Shift + click keeps it on the pointer). Until then it goes along to another sheet or a new one and through undo and other changes to the sheet; another screen, another tool (dropping from the parts list, cut lines, manual cutting) or a job with other parts ends it. While it is held, R, Del and Ctrl+D wait (they would act on the copy dropped last). | rebuild |

How the rebuild uses them:

- **Select**: click a part; Shift or Ctrl + click adds it to the selection or takes it out;
  drag on empty sheet for a window (Shift or Ctrl adds to what is selected); Ctrl+E or Ctrl+A
  selects every part on the sheet; Esc clears. Pan with the middle button or Space + drag.
- **Keys** (HM1): Ctrl + arrows slide to the next part or the margin (HM2); Alt + arrows move
  exactly by the step (HM3). Holding a key repeats the move; moves in a row make one undo step.
  After each move the status line shows how much room is left in each direction. The ribbon's
  **Move** group has the step box and slide buttons. Plain arrows and Page Down / Page Up move
  and zoom the view (HM13). R turns to the next allowed quarter turn; Ctrl+Q / Ctrl+W turn left /
  right by the rotation step and Ctrl+1/5/0/3/4/9 by 1/5/10/30/45/90° in the set direction
  (HM14, HM16); Ctrl+T types an angle; M / Shift+M mirror (Ctrl+A / Ctrl+S in
  the original; Ctrl+S saves here). Del removes the selection, Ctrl+D copies it (in the
  original Ctrl+D flips the turn direction). Ctrl+C puts it on the pointer as a pattern that
  drops a copy per click, using the pieces still to place (HM18–HM21).
- **Drag** (HM8–HM11): the selection moves together; with **Snap** on (ribbon) it snaps to the
  gap near a part or the margin; when blocked it slides along; red outline = can't stay there
  (moved back on release). Ctrl + drag drags a copy.
- **Turn** (HM14–HM17): the turn bar under the selection turns it by the step or to a typed
  angle; the round handle above it turns it by dragging, snapping to the step.
- Not imitated yet: the minimum-rectangle turn (Ctrl+G / Ctrl+F), + / − and Ctrl+E for the step,
  Ctrl+D flipping the fixed-angle keys (FEATURES NST-22), Place Near, Expand, Fit and Advanced
  Copy (NST-4), snap points and Distance & Angle on the sheet (NST-6), orthogonal mode.

## Manual machining (cutting picked contours by hand)

The original keeps a sheet's machining as a list of instructions. Each cut contour carries a
label saying what it is:

| Label | Meaning |
|-------|---------|
| `##EXTCUT` / `##INTCUT` | automatic outside / inside cut |
| `##MANUAL`, `##CNTMAN` | manual outside cut |
| `##MANINT`, `##CNTINT` | manual inside (hole) cut |
| `##MANCOM`, `##CNTCOM` | manual common cut (shared edge) |
| `##CNTISL`, `##ISLAND` | island |
| `##CNTMRK` | marking |
| `##RECCUT` | remnant cut |

| # | Rule |
|---|------|
| MM1 | A contour is **manual** if its label is ##MANUAL, ##MANINT or ##MANCOM. Manual outside cuts are ##CNTMAN / ##CNTISL / ##CNTCOM / ##MANUAL / ##ISLAND / ##MANCOM; manual inside cuts are ##CNTINT / ##MANINT. |
| MM2 | A contour is a **hole** unless it is labelled ##EXTCUT. |
| MM3 | A contour counts as **cut** as soon as any of its elements is machined. |
| MM4 | **Picking a contour on the sheet** finds the part from the clicked element and refuses a contour that is already cut or not valid. It then reads the contour's cut position, cut quality, whether it is a standard shape (circle, rectangle…), whether it is a hole, and copies its geometry for the tool path. |
| MM5 | A manual auxiliary path is **open** when its start and end are further apart than the tolerance. |
| MM6 | After a delete, open manual paths that aren't cut yet are **joined** where two of their ends meet within the tolerance. One is reversed when the ends meet head-to-head or tail-to-tail. |
| MM8 | **A sweep across parts** (in Manual cutting, a drag that starts on a contour) takes whole parts in the order the pointer passes over them: a part is passed when the pointer comes within the pick aperture of one of its contours, and each is taken once. Each part is cut whole at its place among the sheet's picks, in its own cut order (CO8), each hole after the parts sitting in it (CO2), so a part sitting in a hole of a part swept before it is still cut first. A part the picks so far already cut whole (taken by a sweep or box, its outline clicked, or sitting in a picked hole) is skipped and named; a part with only some of its contours clicked is taken, and the rest of it is cut whole at the sweep's place. Its contours start and run as the automatic ones would, so they aren't labelled `( MANUAL CUT )`; the sheet's M1, M2… badges number every pick in the order it is cut, a part taken whole at the first of its contours cut. A click still picks one contour, as before (MM4); a click on a contour of a part already taken whole sets where that contour starts and which way it runs, as on a part whose outline was clicked. On the LC5 a part is one call, made where its outline is cut (CO2), so a contour clicked before the sweep is cut with the rest of its part at the sweep's place (FEATURES MAC-67). Agents: `cut-order mode=parts` with `items` (placed parts in the order to cut them) or `points` (a sweep path). *Estimated (own design).* |
| MM9 | **A box** (in Manual cutting, a drag from open space) takes the whole parts wholly inside it (dragged to the left, every part it touches, SW1, SW3), in the sheet's automatic order among themselves (CO1, CZ4, CZ5); a part partly outside a window is left out, and parts already picked are skipped (MM8). A sweep or box that takes no new part is refused and changes nothing. Every sweep and box is one command: **Undo pick** (and Ctrl+Z) takes back the whole of the last one, as it takes back one click. Each sheet keeps its own picks, saved with the job, and MM7 clears them when parts come off. Agents: `cut-order mode=parts box=…` (with `touching=true`, a crossing window), and `mode=undo`. *Estimated (own design).* |

How the rebuild uses them (Sheet tab → **Manual cutting**):

- **Pick contours**: click contours on the sheet in the order they should be cut. The contour
  under the pointer is highlighted with the start point; where you click is where the cut
  starts (MM4).
- **Reverse direction** (or Shift+click for one pick) cuts it the other way.
- **Sweep or box whole parts**: drag from a contour across parts to take them whole in the
  order you pass them (MM8), or drag from open space to draw a box round whole parts, which
  go in the automatic order (MM9). Each part is cut whole, in its own cut order.
- **Undo pick** removes the last pick (a whole sweep or box at once), and **Clear** goes back to
  fully automatic cutting.
- A contour already picked can't be picked again (MM3). Picking a part's outside first cuts its
  remaining holes before it, so the part doesn't drop (MM2). Picking a hole, or the outside of a
  part with holes, first cuts every part sitting in that hole, whole (CO2).
- Picked contours are cut **first, in the picked order**, labelled `( MANUAL CUT )` in the
  G-code and shown with M1, M2… badges and a direction arrow. Every other contour follows
  automatically.
- While picking, a drag sweeps or boxes (parts don't move); the middle button or Space + drag
  pans the view. Nesting again clears the picks, and so does taking parts off the sheet or
  removing a product from the job (MM7).
- Not imitated: open manual paths and joining them (MM5/MM6), common cuts, islands, and the
  standard-shape macros.

## Micro-joints: types, joint leads and editing

In the original a micro-joint is a group of elements: a **lead-out**, the **uncut gap**, a
**lead-in**, plus auxiliary and "other" lead elements, read back together as one joint.

| # | Rule |
|---|------|
| J1 | The joint settings live in the lead record: a type (**-1 = none**), joint lead-out length, joint lead-in length, gap, and three angles = **45°**. With joints on, both lead lengths start at **10.0**; the gap starts at **0.5** (**1.0** with a flag). **Corrected in v44 build analysis:** the first two values are the lead-in/lead-out **distances** D (perpendicular, F001/F002), then the angles, then the **radii**; the gap is 0.5, or 99999.999 = use B164. |
| J2 | **Types**: 1, 2, 7 and 8 are joints **in a corner**. 3–6 are joints **with a lead-out and a lead-in**. 9–11 are a **plain gap without leads**. The categories are 1 (types 1–2), 2 (types 7–8) and 4 (types 3–6 and 9–11). |
| J3 | A joint with leads is refused on an element that already has a loop or a bridge, on geometry without technology, and where a lead-in sits in the corner. The lead-out is made only if its length is non-zero, and so is the lead-in. |
| J4 | **Both ends of the gap** (the point ± gap/2 along the element) must lie on the **same element** (segment, arc or circle). |
| J5 | A nano-joint's element must be at least **10 ×** its size long. **Corrected in v44 build analysis:** 10 × is the nano-joint gap (10 × tolerance). A joint's element only has to be **longer than the gap**, and a plain gap slides flush to the element's end (AM5). |
| J6 | A gap left at the placeholder **1.0** is replaced by the effective value. With a flag, the gap is recalculated to its real width. |
| J7 | The joint's **lead-out is built from its lead-in**, mirrored across the gap. **Corrected in v44 build analysis:** lead-in and lead-out have their own D, A and form; they lean away from the gap (AM6). |
| J8 | **Editing** a joint's lead-in: the lead keeps its foot on the contour; its end follows the pointer, projected on the lead's line. The angle is limited to **−90…90°** (0 if outside), and the new length is the segment (+ arc) length. The lead-out is edited the same way. |
| J9 | No-cut (joint) elements carry a micro-off type and a nano-joint type as a machining attribute of kind 11. |

How the rebuild uses them:

- **Joint type** (Technology tab → Micro-Joints → Settings), with three choices:
  - **Plain gap** (types 9–11): the beam goes off across the gap. This is the default and
    matches earlier versions.
  - **With lead-out and lead-in** (types 3–6).
  - **At corners, with leads** (types 1/2/7/8): joints start at convex corners, spread over
    them.
- The Settings window sets the gap width, lead length (default **10.0**) and angle (default
  **45°**) from J1.
- **Joint leads**: at a joint the cut runs out into the scrap along the lead-out, then the beam
  goes off. It re-pierces in the scrap and cuts the lead-in back to the gap's end; the lead-out
  is the lead-in mirrored (J7). The G-code says `JOINT LEAD-OUT`, `PIERCE JOINT` and
  `JOINT LEAD-IN`. The leads' length, the extra pierces and the moves count in time and cost.
- On outer contours the leads reach out at most **0.8 ×** the part gap, so they never cut into
  the neighbouring part. This is the rebuild's own safety rule; the original's sheet check isn't
  rebuilt.
- **One element, 10 × long** (J4/J5): a gap that would straddle a corner slides wholly onto the
  element holding its middle. A gap on an element shorter than 10 × its width is dropped.
- **Editing** (J8): in the Part view each joint lead-in has a red handle. Drag it to change the
  leads of all the part's joints. The length is the distance to the pointer, and the angle is
  kept between 0 and 90°; outside −90…90 it falls back to 0, as in the original.
- Not imitated: micro-off and nano-joint attributes (J9) and the real-gap recalculation (J6).

## Automatic nesting (levels, rotations, modules)

The original's automatic nesting is driven by the level files `LevelSys.nst` / `LevelUsr.nst` /
`LevelOld.nst` (lines `A001`–`A022`). The "Optimized nesting" option is a different engine run
by a third party (a cloud service). It isn't rebuilt.

### N1. The five automatic levels (LevelSys.nst)

| Line | Meaning | Level 1 | 2 | 3 | 4 | 5 |
|------|---------|---|---|---|---|---|
| A001 | nesting methods tried | 4 | 7 | 11 | 16 | 18 |
| A002 | main direction (1 vertical, 2 horizontal, 3 both) | 1 | 1 | 1 | 3 | 3 |
| A003 | strip (band) width — only used when A020 = 1 (corrected, see below) | 3.0 | 3.0 | 3.0 | 3.0 | 3.0 |
| A004 / A005 | regular nesting / number of regular tests | off / 99 | on / 99 | on / 99 | on / 99 | on / 99 |
| A006–A009 | modules: parts, semi-complex, complex, staggered — only used when A022 = 1 (corrected, see below) | off | off | on | on | on |
| A011 | minimum efficiency for a module | 0.8 | 0.8 | 0.8 | 0.8 | 0.8 |
| A014 | possible rotations (step, °) | 30 | 30 | 30 | 30 | 30 |
| A015 | level for regular nesting | 0 | 1 | 2 | 2 | 3 |
| A016 | rotate parts to fill holes | 0 | 0 | 0 | 0 | 1 |
| A017 | test each part from different corners | 0 | 0 | 0 | 0 | 1 |
| A018 | special rotations (0 no, 1 yes, 2 automatic) | 2 | 2 | 2 | 2 | 2 |
| A019 | use system modules | 0 | 0 | 1 | 1 | 1 |

Defaults when a line is missing: 0.75, 5.0 and 90.0.

**Corrected**: for a system level, A020/A021/A022 start at 0 and A003, A014/A018
and A006–A010 are copied only when the matching A020/A021/A022 line is 1. All five system levels
have 0 there, so the **strip width, rotation step and module switches come from the machine's CT1**
(B057, B058/B593, B070–B072/B084/B085), not from the level. The level supplies A001, A002, A004,
A005, A011, A012, A015, A016, A017 and A019. The rebuild follows this for the strip width
(`NestLevel::own_band` = A020 = 0 → CT1 B057); it still takes the module switches from the level
(see "True-shape strip nesting").

### N2. Rotations

| # | Rule |
|---|------|
| R1 | Each part keeps **eight rotation permissions**: 0°, 90°, 180°, 270°, each plain or mirrored, plus a flag for "other angles". |
| R2 | A module's angle is the sheet angle ± the module's angle, brought into **[0, 360)** (add or subtract 360). It is allowed if a matching angle with the same mirror flag is in the list within the tolerance; otherwise R1 decides. |
| R3 | **Special rotations**: mode 0 never, mode 1 always, otherwise only for angles that aren't 0/90/180/270. |
| R4 | Extra rotation steps around a main angle: 5°; 10° and 15°; 15°; 45°, chosen by thresholds 5/10/15/30/45. |
| R5 | When the sheet axes are turned 90/180/270°, the nesting corner turns with them. |
| R6 | The main direction comes from the machine's settings (cut machines: two values, main and secondary). |

### How the rebuild nests (`nest.rs`)

- **Level picker** (Sheet tab → Nest → Automatic level, default 3): each level tries exactly
  A001 methods on every sheet and keeps the best sheet. Placement is the true-shape strip engine;
  see "True-shape strip nesting" at the end of this file (it replaced the
  old box engine, which sorted boxes and placed them in a maxrects free list).
- **Modules** (A006/A007, A011):
  - Pair: a part and a copy turned 180° (a mirrored copy for semi-complex), slid together until
    the part gap is reached. The pair is kept only if it fills at least **80 %** of its box and
    beats a single part.
  - Staggered (A009): discs in staggered rows (every other row shifted half a pitch, rows
    pitch·√3/2 apart). Used only when it beats the square grid.
  - "Complex" modules (groups of different parts) aren't imitated.
- **Regular nesting** (A004/A005/A015): tidy arrays of identical parts, at most 99 shapes tried.
  Arrays start from 2 + A015 parts.
- **Holes**: a placed part only takes its material out of the sheet's free strips, so smaller
  parts drop into its holes during normal nesting, at every level (holes over B076). Parts
  inside another part are cut first; with parts in the holes of parts in holes (rings in rings)
  the most deeply nested goes first (CO1).
- **Corners** (A017, level 5): not used by the strip engine (it always nests from the lower left).
- **Rotations** (R1): the Part Properties have the 0/90/180/270° buttons and "may be flipped".
  Placements store quarter turns and a mirror flag; turning a placed part steps to the next
  allowed angle. Free angles (R3/R4, 30° steps) and sheet-axis rotation (R5) are written as
  functions and tested, but not used: the strip engine tries quarter turns and mirrors only.
- Result on the sample job: see "True-shape strip nesting" (before/after table).

### Nesting one sheet again (Home → Nest sheet)

| # | Rule | Source |
|---|------|--------|
| RN1 | **Nest sheet** (also right-click a sheet in the Nesting Explorer → *Nest this sheet again*, and agents' `renest-sheet`) nests only the parts already on the current sheet, back onto that same sheet (its format, or a remnant's real outline), by the job's automatic level, then compacts them against their contours on a rectangular sheet. The other sheets keep their layout; parts never move between sheets. The sheet's cut lines and hand-picked cut order go; the automatic remnant line comes back when "generate remnants after nesting" is on. **Nest all** still re-nests the whole job and may put everything on fewer sheets. | Estimated (the original's "One Sheet (redo)"; its exact behaviour not traced, FEATURES NST-19) |
| RN2 | If the nester can't fit all of the sheet's parts back on it (a hand layout tighter than the automatic level finds), the sheet is left exactly as it was and the status line says how many fit. | Own rule |
| RN3 | With **Group by attribute** on (agents: `group`, default the machine's CT1 setting, as for `nest`) and parts of several groups on the sheet, each group is nested in its own zone with the zone gap, as Nest all does (GZ2, GZ3), and the sheet is not compacted so the zones stay apart. If the groups no longer fit on the one sheet, or the sheet is a remnant (zones work on whole sheets only), it is refused and the sheet is left as it was. | Own rule (follows GZ2/GZ3) |
| RN4 | Remnants already stored in stock from a sheet are rebuilt on the next store when what they were cut from changes: its remnant lines, or — on a sheet with no remnant line, whose stored remnant is the whole sheet less its parts ("generate remnants with the completed sheets") — where its parts are. Re-nesting a sheet always marks them out of date. While unused they are replaced; once used elsewhere they stay (ST12). Files saved before this kept only the lines' fingerprint; for a sheet without a line it is read as unchanged. | Own rule (extends ST12) |
| RN5 | **Undo** brings back each sheet's cut lines and hand-picked cut order together with the layout, so undoing Nest sheet or Nest all restores the lines that went with the old layout. Adding, moving or deleting a cut line by hand, and picking, removing or clearing the manual cut order, are each their own undo step. | Own rule |
| RN6 | Undo and redo don't roll the stock back. Each time storing replaces a sheet's remnants (ST12), the stock file records which remnants took their place (none, when the sheet now has no remnant). A restored sheet whose remnants were replaced that way follows those records to the remnants in stock now, takes them over marked out of date, and the next store rebuilds them for the restored layout (while unused) instead of leaving them in stock untracked; replaced by nothing, its remnants are made again. It never takes a remnant another restored sheet holds, and a remnant missing for any other reason (deleted by hand in the stock window) is left alone, so it isn't remade. A reference in that record that no longer names an item in stock is kept for it: a new sheet can't be created under it (refused, with the reason; automatic sheet numbers skip it), the remnant naming window and agents' `stock-book refs` refuse it with the reason, and an automatic or formatted remnant name that lands on one gets the next automatic reference instead — so undo never takes an unrelated item for one that only shared its name, and a chain of replacements is never broken. Same in the window (on the next save) and for agents (at once, when the job is booked). | Own rule |
| RN7 | **Using a stock sheet or remnant on the shown sheet** (Sheet stock) first says whether the parts already on it would still fit. When some wouldn't, the main choice is **Use it and re-nest sheet N** — all or nothing: the sheet takes the stock item and its parts are nested on it again (RN1), or, if they can't all fit (RN2), nothing changes and the message says so. "Use anyway — parts stay over the edge" keeps the parts where they are (those past the new edge are flagged, QR7). Double-clicking a row does what the main button does (re-nests when parts would no longer fit). What the last use did stays under the buttons. The dialog keeps "Put on sheet N" (using) apart from "Stock" (Add / Reduce / Delete, which don't change the job). | rebuild |

## Round parts in nesting (circle or box?)

The original does **not** treat a round part as its box: it recognises round parts and tests
them with real circle geometry.

| # | Rule |
|---|------|
| C1 | A nesting structure **is a circle** when it holds one part whose outer contour is a single element that is a circle or an arc whose sweep equals 360°. |
| C2 | When two structures are tried as a **pair module**, each is classed first: 2 = circle; 1 = a fill value lying between 0.4× and 0.6× of a reference value (inferred to be how much of its box the part fills); 0 = not paired. |
| C3 | The pair is built on a temporary sheet whose length is the part size × 1.333 / 1.667 / 1.889 depending on the mode, but **× 2.0 when the first part is a circle** — room for two discs side by side before they are pushed together. |
| C4 | Collisions between round shapes use their true geometry: circle–circle, segment–circle and arc–circle tests with a tolerance. |
| C5 | Approach snapping projects onto arcs and circles **along the radius** (see G6). |

How the rebuild uses them:

- A part is round when it is a *Disc*, or an imported outline whose points all lie within
  0.5 % of one radius (at least 16 points) — our version of C1, since our outlines are polygons.
- **Collisions use the circle** (C4) when you drag a part, drop a ghost or check a spot: two
  round parts clash only when their centres are closer than both radii plus the part gap; a
  round part against another part uses the distance from its centre to that part's box. Two
  non-round parts still use boxes.
- **Ring ghosts snap round** (C5): near one ring it slides in to sit one gap from its edge; near
  two it drops into the pocket touching both — the tight, staggered spot. The clearance halo is
  drawn as a circle, with no box.
- Automatic nesting already packs discs in staggered rows (pitch × √3/2) instead of a square grid.
- Not imitated yet: the pair-module classing (C2/C3) in automatic nesting, and true-outline
  collision for non-round parts.

## Opening a part (Drawing module)

Opening a part starts the original's Drawing module (launch mode 0). It is a separate program
with its own ribbon and its own look.

**Tabs and groups**: *Parts* (file menu) · **Home** — Draw (Access to points:
picked / centre / end / near / mid / intersection / tangent / projected / quadrant points;
Delete Geometry; points, line segments, lines, continuous line/arc, arcs, circles, squares,
rectangles, polygons, slots, ovals, ellipses, pulleys), Modify (Break, Fillets, Chamfer,
Offset, Lengthen, Join, Fitting), One-touch Figures (12), Transform (Copy, Move, Stretch,
Rotate, Scale, Place Near, Mirror, Union, Subtraction, Intersection, Change Cutting Direction,
Contour Ordering), Review (Verify Part, Detect part, Time and cost, View Geometry, Distance and
Angle, Angle between three points), Orthogonal, Macros · **Technology** — Lead-in/Lead-outs (16
types), Micro-Joints, Loops, Standard Bevel, 3D Bevel, Cut Technology, Punch Technology, Texts ·
**Machining** — Machining (Automatic, Delete instructions, Rapids, Goto), Contours (Manual,
Automatic, Parts, All Drill Cycles, All Marks, All Cut, All Contours of one Quality, Automatic
Cycles, Flying cut in holes, Manual / Automatic / Semi-automatic hole destruction, Hatch
pattern machining), Cycles, Modify, Options, Component Extraction, View (Current status, Verify
the Machining) · **View** — Display (Part Machining List, Simulation Window, Machining Explorer,
3D view, Change co-ordinates, Axis, Color Display, Auxiliary / Bending geometry, Show Parts,
Restore Panels, Redraw), Zoom (All, Window, Previous, Dynamic, Scale, Base, Define limits, Full
Screen) · **Simulation** — Program, Beginning, Previous, Next, End, Run, Previous/Next Tool,
Reposition, Multihead, Extraction · **Notes** — Dimension Notes (Automatic, Semi-Automatic,
Delete, 7 styles), Texts, View (Text and Dimensions) · **Utilities** — Bevel cuts, Partial cut,
Open Drafting, Compare Geometries, Bitmap, Mosaic, lift positions, arcs in pockets.

| # | Rule |
|---|------|
| DR1 | The **axes** are two lines through the drawing origin spanning the whole view (XOR pen, or a dashed style), drawn only when switched on. |
| DR2 | Drawing colours come from **LANTEKIN.CNI** lines `Cnnn R G B style width` (header "Colores: Rojo Verde Azul Estilo Anchura"); RGB is packed to a Windows colour (R + G·256 + B·65536). |
| DR3 | Default values: C002 255 0 0 style 2 (red, dashed — the axes), C008 255 255 0 (yellow — cut contours), C011 255 255 255 style 2 (white — cursor), C053 0 255 0 (green — marking), C057 0 200 0 style 2, C105 160 160 160 ("colour for filling a part or sheet"). Which number is which item is inferred from how the original looks, except C105 which the file names. |
| DR4 | Each **cut-technology type** has its own colour slot (types 1–5 each their own, anything else a common one). |
| DR5 | An element's colour is **mixed** with its technology colour: a black or white technology colour leaves the element colour; otherwise the two are blended (factors 0.5 and 1.0). |
| DR6 | The view keeps the cursor position in X/Y and shows it in the status bar. |
| DR7 | **Distance and Angle** shows its results in a box of four read-only number fields, each with precision **2** (two decimals); the box's layout is saved as "DISTANG". |
| DR8 | **Angle between three points** is its own Review command ("Calculate the angle between three points"). |

How the rebuild uses them:

- Opening a part now switches the ribbon to the Drawing module's tabs: **Home, Technology,
  Machining, View, Simulation, Notes, Utilities**, and opens on Home like the original. Every
  group of the original is there; the commands this rebuild has work, the drafting commands it
  doesn't have yet (drawing lines/arcs, modify, transform, cycles, extraction, utilities) are
  shown greyed out and say so.
- Ribbon layout: like the original's ribbon, each group mixes big buttons with stacks of three
  small commands (an on/off setting shows a dot, filled when on; a command shows a chevron). A
  group whose commands aren't built yet (Shapes, Modify, Transform, Figures, Cycles, Options,
  Extraction, Macros, the Utilities) is left out (see "How the original builds its ribbon");
  switched on, it is one dropdown that lists them greyed out with a "not yet" tag. All groups are the same height, so their
  titles line up.
- The drawing looks like the original's (View → **Color Display**, on by default): black
  background, each contour in its cut quality's or state's colour (CS7; the original draws cut
  contours yellow, C008), marking texts green (C053), notes grey, the **red
  dashed axes** through the origin (DR1, C002 style 2) and a **full-view cross cursor** (C011).
  The status bar shows **X / Y** of the cursor (DR6).
- Home: **Access to points** (picked / end / mid / centre / near) for the measuring tool,
  **Distance and Angle** (two clicks: distance, dX, dY, angle; **Orthogonal** squares it),
  **Verify Part** (outline closed and not crossing itself, holes inside and apart, texts on the
  part), **Time and cost** (the summary card), **View Geometry** (element points and direction).
- Machining: **Automatic** / **Delete instructions** (shows or hides the cut order and
  pierces), **Contours** menu (Automatic Contours, Parts, All Cut work), **Current status**,
  **Verify the Machining** (every pierce in the scrap, holes before the outside — MM2).
- View: Axis, Color Display, Machining Window, Restore Panels, **Full Screen** (hides the
  panels), **Show Parts** (first / previous / next / last), Zoom All / Window / Previous / In /
  Out.
- Simulation: **Beginning, Previous, Run/Pause, Next, End, Stop** play the part's own program
  the way the sheet's Simulation does (below): the head moves along each beam run — rapid,
  pierce, lead-in, contour, lead-out — so it can be followed from the lead-in into the contour;
  cuts already made turn green (C057), micro-joint gaps stay uncut, and the counter says how
  many contours are cut. Speed is × real time — 0.1×, 0.25×, 0.5×, 1× (default), 5× (our
  choice: slow enough to follow a lead-in). Previous / Next go pierce by pierce and bring the
  head back into view when zoomed in; the head is followed while it runs. The Simulation Window
  (slider, operation, X / Y, cutter radius, heads) sits under the part, which fits above it
  while simulating.
- Fix: the part's simulation used to light a whole contour at once with a fixed dot at its
  start point and paint over its micro-joints, so the head never seemed to go from the lead-in
  into the contour.
- Notes: Dimension notes Automatic / Delete, Texts (new text), Modify (the selected text),
  Text and Dimensions on/off.
- **Angle between three points** (DR8): click a point, the corner, then another point — the
  angle at the corner (0–180°). Distance and Angle and this angle show **2 decimals** (DR7).
- **What's still missing**: Home and Utilities have a *Still missing* button (also Options →
  "What's still missing in the Drawing module…") listing, tab by tab and group by group, every
  command of the original's Drawing module this rebuild doesn't have yet (152), with what each
  tab already has.
- Not imitated: the per-technology colour mixing (DR4/DR5) — each contour is drawn in its cut
  quality's or state's colour instead (CS7, FEATURES DRW-9) — and the drafting commands.

## Database connection (and the BLM LC5 post)

How the original connects to its database, from its settings files.

| # | Rule |
|---|------|
| DB1 | Expert's database settings are lines **R000–R013** of LANTEKIN.CNI ("PARAMETROS DE CONEXION CON LA BASE DE DATOS"): R001 the SQL Server (`host\instance`), R002 the database, R003 user, R004 password, R005 the database folder on the server, R008 its share, R012 database tolerance 0.02. |
| DB2 | The connection string: **no user** -> OLE DB with Windows sign-in, `Provider=sqloledb;Data Source=<server>;Initial Catalog=<db>;Persist Security Info=False;Integrated Security=SSPI`; **with a user** -> the ODBC "SQL Server" driver, `DRIVER=SQL SERVER;SERVER=<server>;DATABASE=<db>;User ID=<user>;Password=<password>`. |
| DB3 | The machines defined in the database are listed in DBConnect.lst (`B000 "<machine>"`); the active machine is LANTEKPR.CFG A000. |
| DB4 | A named instance is found through the SQL Server Browser (UDP 1434), which answers with the instance's TCP port. (Standard SQL Server protocol, SSRP.) |
| DB5 | Each machine's post-processor (with its .CF1/.PS1/.CT1/.SE1 settings in MchFiles) is chosen from the machine's record in the database. The standard post libraries don't name BLM or the LC5 (the CF1 headers only give A001 type, tolerance, block numbering, extension...). |

How the rebuild uses them:

- **Database connection** window (Jobs screen -> Database -> *Connect...*, or Options): reads
  Expert's settings from its folder (default `C:\Lantek\Expert`) and shows the SQL Server, the
  database, how it signs in (Windows or SQL user — the password is never read or shown, only
  whether one is set), the server folder/share, the exact connection string Expert builds (DB2,
  password masked, with *Copy*) and the machines with the active one marked (DB3). Server and
  database can be changed there to point the app at another server.
- **Test connection** asks the SQL Server Browser for the instance's port (DB4) and opens a TCP
  connection to it, in the background, then says whether the server answered. Signing in and reading
  tables is in "Database tables" below.
- **BLM LC5 post** (DB5): the window explains that the post code comes from the LC5's machine
  record in the database or from a program the LC5 already runs; without either, the
  LC5's exact G-code dialect is not imitated yet — our program output stays the generic ISO.
- Simulation fix: the head used to stop at a zero-length step in the path (a contour's start
  repeated right after its lead-in) and sat at the lead-in while the cut kept drawing; it now
  skips such steps and is always at the end of the drawn cut (checked at 41 moments). While it
  runs, the cut being made is drawn bright white with a dot at the tip, and **Follow head** (on
  by default) pans the view to keep the head in the middle when zoomed in.

## Nesting and Machining: machining and simulating the sheet

The original's *Nesting and Machining* module has the tabs
**Subjobs · Home · Technology · Machining · View · Simulation · Applications · Utilities** (plus a
*Selection* tab while something is selected), a dark sheet view, the **Simulation Window** docked
at the bottom (position slider; the operation, e.g. *Mark contour*; X, Y; *Cutter radius*;
*Number of heads*) and a status bar ending in the sheet's **CNC program name**.

| # | Rule |
|---|------|
| SM1 | The Machining tab: Machine All, Automatic, Copy Machining of a Part, Regenerate, Delete instructions, Contours, Cycles, Modify, Micro-Joints, Optimize, Change Cutting Direction, Contour Ordering, Component Extraction, View CNC, View, Sheet Reposition, Move clamps. The Simulation tab: Program, ISN position, Mouse Positioning, Beginning, Previous, Next, End, Run, Previous/Next Tool, Reposition, Multihead, Extraction. |
| SM2 | Each simulated move updates the head's **current point**; a rapid move draws a pierce mark where cutting starts, and the state switches between moving and cutting. |
| SM3 | The head is drawn at its point with a radius (the cutter). |
| SM4 | The window's fields: *Operation*, *Cutter radius*, *Number of heads*, *Mark contour*, *Simulation Speed*, *Start / Pause / Stop simulation*. |

How ours compared, and what the rebuild now does:

- Before: our nesting screen had Home / Sheet / Technology / View only; the cut path could be
  shown, but there was no machining tab, no simulation and no CNC name.
- Now the nesting screen has **Machining** and **Simulation** tabs. *Machining*: **Machine
  All** (every sheet machined automatically, path shown), **Delete instructions** (clears the
  hand-picked order, hides the path), the **Machine** picker, **Manual cutting** (moved here
  from the Sheet tab: pick contours, reverse, undo, clear) and **View CNC** / **Export CNC**.
- *Simulation*: **Beginning, Previous, Run/Pause, Next, End, Stop**, a **Simulation Speed**
  choice (1×, 5×, 20× (default), 50×, 200× real time) and the **Simulation Window**. The program
  plays in order (per part its markings first, then each contour along the beam's own path —
  the one the program and the times use: rapid, pierce, lead-in, contour, loops, overlap,
  lead-out; a micro-joint is jumped and pierced again, a joint with leads leaves along its
  lead-out and comes back along its lead-in; punched contours hit by hit). The head stands
  still at each pierce for the pierce time. Done cuts turn green (C057), markings light green,
  punch hits orange, with a pierce mark where each run started (SM2); the head is a red ring
  with a cross (SM3). Previous/Next jump pierce by pierce (and to each marking and hit); the
  slider moves anywhere. The next rapid starts where the beam stopped (after the lead-out, the
  overlap or the closing joint's lead-out), in the sheet's times too.
- Fix: the head used to run lead-in, the whole contour and a straight line to the lead-out's
  end, so it cut across micro-joints, skipped joint leads, loops and the overlap, and drew arc
  lead-outs straight.
- The Simulation Window shows the operation (e.g. *Cut contour 12 : hole of part 5 (Ring)*) with
  its colour, the head's X / Y, the **cutter radius 0.150** (half the 0.3 mm cut width — the
  value observed in the original) and **1 head**.
- The status bar shows the sheet's CNC name (`CNC: <JOB>X<sheet>`).
- Each step takes the plan's time for it ("Machining time and speeds": rapids, markings, and
  per contour its pierces, cut, head moves); punch hits 0.25 s. The whole run lasts as long as
  the sheet's time on the times card.
- Not imitated: Subjobs, Applications and the contextual Selection tab, multi-head machines,
  tool changes, sheet repositioning and clamps, ISN position / Program stepping.

## Machines and standard shapes (automatic, manual and recognised machining)

Machine types, from the original's machine libraries (`MchFiles\*.CF1`), and how standard
shapes are recognised.

| # | Rule |
|---|------|
| MC1 | Every machine library starts with **A001 machine type: 0 CUT, 1 CUTPUNCH, 2 PUNCH**. The standard library set has 1 063 cutting machines, 100 combined punch + cut and 150 punching machines. |
| MC2 | Cutting machines differ by their technology tables: laser (A035 lens, gas, power), oxyfuel / plasma (A039 power, gas), waterjet (A040–A044 nozzle, pressure, abrasive). Other header values: A002 default technology (1 cut, 2 mark), A004 fixed cut cycle (1 centre-punch, 2 on/off), **A005 machine tolerance 0.005**, A023 entry to circles (0 none, 1 line, 2 arc), A024 circles in one move, A037 speed factor 1.0. |
| MC3 | **Standard-shape recognition** tries, in order: **0 circle** — one circle element, or an arc whose sweep is 360°; **1 rectangle** — more than 3 straight elements and no arcs (4 corners checked square); **2 obround** — 4 elements, 2 straight and 2 arcs alternating, both arcs 180°, the arc centres further apart than the straight sides are long; **3 ring sector** — 2 straight and 2 concentric arcs whose straight sides' lines meet at the arcs' centre. Each returns the centre and size. |
| MC4 | Standard contours are machined as one **standard macro** (start/end markers around it) rather than point by point; a punching machine looks for a matching punch (round, rectangular, obround). |
| MC5 | Machining a part can be **automatic** (Machining → Automatic), **manual** (Contours → Manual Contours: pick contours in order, MM1–MM6) or by contour type (Contours → All Cut / All Marks / All Contours of one Quality / Automatic Cycles / Automatic Punch). |

How the rebuild uses them:

- **Machine** choice (Sheet tab → Machine, Machining tab, the drawing's Machining tab and the
  Export review): the shop's own machines from Settings → Machines & output, by name. Each has
  what it does — Laser, Plasma, Oxyfuel, Waterjet (cutting, MC1 type 0), Laser + punch (type 1)
  or Punch (type 2) — and a program format; picking one sets the job to that process and makes
  it the machine Export CNC writes for (MK1). The machine's name shows in the window title, the
  jobs lists and the status line; the program header keeps the process (`( MACHINE … - TYPE n )`).
- **Recognise shapes** (Machining tab) labels every contour that is a standard shape — circle
  (Ø), rectangle (w × h), obround slot (length × width) — in the original's order (MC3). Our
  outlines are polygons, so the tests work on points: a circle when every point is the same
  distance from the centre, a rectangle when four corners are square (within 0.5°), an obround
  when every point is half the width from the line between the two arc centres. The tolerance is
  the machine tolerance 0.005 mm (A005) plus 0.2 % of the shape's size. Ring sectors (type 3)
  are not recognised.
- **Cutting machines** cut every contour as before (automatic, or manual picks on the sheet).
- **Laser-Punch** punches every *hole* that is a standard shape with one hit at its centre
  (MC4) and cuts everything else; **Punch** punches every standard shape and **nibbles** the
  other contours (a hit every 4 mm). Punched contours have no leads, joints or pierces; the program writes
  `( PUNCH Round Ø20.0 )` / `( NIBBLE n HITS )` with `G0 X Y` + `M12 ( HIT )` per hit; the
  drawing shows the tool outline in orange with a cross at the hit, the sheet shows a cross per
  hit. Each hit counts 0.25 s in the time estimate (estimated).
- Not imitated: punch tool libraries and turrets (tools are assumed to exist in every size),
  different speeds per cutting technology, circles in one move (A024) and ring sectors.

| # | Rule | Source |
|---|---|---|
| MK1 | **One machine choice.** The Sheet tab lists the shop's machines (Settings → Machines & output) by name, each with what it does (laser, plasma, oxyfuel, waterjet, laser + punch, punch) and its program format. Picking one — there, in the Export review or with *Use this machine* in Settings — sets the open job to that process and makes it the machine programs are written for. A job made for another process says so under the list ("job set for punch") until a machine is picked. Changing what the machine in use does, in Settings, changes the open job with it. | rebuild |
| MK2 | Machines brought in from the other program get their process from their name: *plasma*, *oxy* / *flame*, *water*, *punch* (with *laser* or *combi*: laser + punch); anything else is a laser. It can be changed in Settings. | estimate |

## How the original builds its ribbon

The original builds each module's ribbon (Jobs, Nesting, Drawing) from a ribbon layout file
with a third-party ribbon toolkit.

| # | Rule |
|---|------|
| RB1 | Each `<CONTROL>` is read attribute by attribute: CONTROL_ID, ICON_ID, SHOW_FLAG, MENU_ID, CONTROL_TYPE, IMAGE_FLAG, SHOW_SPIN, HIDE_CONTROL, BEGIN_GROUP, FLAG, SIZE, POPUP_SIZE, SUBCONTROL_SIZE, SMALL_IMAGE, LARGE_IMAGE, CAPTION, PROGRAM, CONTROL_GROUPED, BEGIN_CONTROL_GROUP, END_CONTROL_GROUP, BEGIN_NEW_ROW, BUTTON_STYLE, GALLERY_STYLE; child `CONTROLS` are its menu items, `GALLERY_ITEMS` a gallery. |
| RB2 | Defaults: CONTROL_ID and MENU_ID -1, CONTROL_TYPE 1 (a plain button), BUTTON_STYLE 0 (automatic), GALLERY_STYLE 1, IMAGE_FLAG 3. |
| RB3 | CONTROL_TYPE 10 makes a gallery; any other value is passed to the toolkit as its control type (1 button, 2 popup menu, 3 button-popup, 4 split button). Sub-controls are added under it. |
| RB4 | **A command that isn't available is not added at all** (licence check); only a placeholder keeps a grouped row intact. |
| RB5 | **Empty groups are removed, and a tab left with no groups is removed.** |
| RB6 | Saving a customised ribbon writes back CONTROL_ID, ICON_ID, BUTTON_STYLE and BEGIN_GROUP. |
| RB7 | In the Drawing module's ribbon: 24 large buttons (BUTTON_STYLE 4, icon above caption), 23 small icon + caption (3), 149 icon-only (2), almost all in grouped rows (CONTROL_GROUPED 149, BEGIN_NEW_ROW 17); 47 split buttons (type 4), 26 drop-down menus (type 2), 2 galleries. |

How the rebuild uses them:

- Like RB4/RB5, the Drawing module's ribbon **leaves out the commands this rebuild doesn't have**
  instead of showing them greyed: the Shapes, Modify, Transform, Figures, Macros, Cycles, Options,
  Extraction and Steps menus and the greyed snap / contour entries are gone, groups left empty
  disappear, and the **Utilities tab disappears** because none of its commands exist yet.
- *Options -> Show commands not built yet* (or the switch in the *Still missing* window) brings
  them back, greyed with a "not yet" tag, for anyone who wants to see the whole original ribbon.
- Big buttons are the original's style 4, the small captioned rows its style 3; drop-down
  buttons (caret under the label) stand for its popup and split buttons (types 2 and 4).

## Quick ring (right-click tool menu)

The app's own feature, in the style of a CAD program's mouse gestures (the original has
no such ring). Right-click (or S) on a canvas opens a ring of eight tools. Code:
`radial.rs`; the turn-back and overlap rules in `engine/mod.rs`.

- Keep the right button down and flick toward a tool to run it.
- A plain right-click leaves the ring open for a click; the middle, Esc or another
  right-click closes it.

| # | Rule | |
|---|------|---|
| QR1 | **The ring depends on where it opens.** In the Part view: the technology tools (clockwise from the top: lead-ins, lead on element, micro-joints, joint on element, place text, text on element, loops, fit); the element under the pointer is selected first. On the sheet, on a part: the tools for the selection (turn, turn back, copy, mirror top↕bottom, mirror left↔right, undo, delete, fit); the part is selected first unless it already is, so a selection or group stays whole. On empty sheet: the sheet tools (re-nest sheet, redo, next sheet, cut off leftover, sheet stock, undo, previous sheet, fit). The Jobs screen has no ring. | rebuild |
| QR2 | **A tool in several rings keeps its direction**: Fit is always up-left and Undo down-left, so a learned flick does the same thing on every ring. A tool that can't run (nothing selected, last sheet, nothing to undo, a turn or mirror the selected parts' rotations don't allow) is greyed, never hidden or moved. | rebuild |
| QR3 | **Turn** steps the selection to its next allowed quarter turn (counter-clockwise, like R); **Turn back** to the previous allowed one (clockwise). A part allowed only 0° and 180° flips between them either way; several parts turn 90° together. Agents: `rotate back=true`. | rebuild |
| QR4 | **Copy** puts copies of the whole selection, spacing kept, on the pointer: they follow it with no button held and snap like a drag; a click drops them where they fit (a spot that doesn't fit keeps them on the pointer), Esc or a right-click takes them away. Until the drop they are only drawn: the sheet, the parts' order and the undo list change only when they land (one undo step, counted as Ctrl + drag counts, HM10). Any other change to the sheet (a command, undo, nesting, its margin or part gap), another sheet, job or screen, or starting another tool ends the carry. | rebuild |
| QR5 | **Collision** (Home ribbon) toggles collision checking; while it's off a "Collisions OFF" badge sits at the top of the sheet. | rebuild |
| QR6 | **Overlapping parts** (closer than 0.01 mm by their real contours, so touching counts) are outlined in red on the sheet and counted on the badge. Export CNC from the ribbon or quick-access bar asks first (QR8); writing anyway warns, naming each sheet and pair; the agents' `program` and `export` say the same. A job with a part gap of 0 is meant to touch and isn't checked. | rebuild |
| QR7 | **A sheet with overlapping parts or parts past its margin is never "all nested".** The badge counts both ("9 overlaps, 1 part off the sheet"), the parts tray's summary turns red ("⚠ 9 overlaps · 1 off the sheet") and the cards of the parts involved show "!" in red instead of a tick. | rebuild |
| QR8 | **Export CNC checks every sheet first.** With overlapping parts or parts off the sheet on any sheet it lists them and asks: Show sheet (goes to the first one), Export anyway, or Cancel. The review also runs Verify machining on every sheet and says whether all pass (or how many problems, on which sheets, with a link to show them), and the machine and post-processor can be changed in it. The programs follow each sheet's machining as shown (lead-ins, cut order, a hand-picked order). The agents' `export` writes and warns as before (QR6). | rebuild |
| QR9 | **Nest all says what it replaces.** A new nesting clears hand-picked cut orders and drawn cut / remnant lines; when there were any, its message names them and that Ctrl + Z brings them back. | rebuild |
| QR10 | **Import says what came in.** After importing drawings a card on the drawing names how many drawings and new part types (and pieces) came in, lists them with their quantities (up to four), the material they take (the job's), any notes, and offers Nest all, Show new parts (picks the first and scrolls Parts View to it) or the material and sheet; Parts View scrolls to the new parts. It stays until closed, another card replaces it, or undo / redo. | rebuild |
| QR11 | **Open job lists every saved job together**: job files opened or saved lately and the Library's jobs, newest first (file date / Library save date), each marked File or Library, with a search box when there are more than six; Browse for a file and the Library window sit below. The quick-access Open shows the same list. | rebuild |

## Moving and zooming the view, and light/dark

How the original's drawing and nesting views zoom and pan, and how its styles pick colours.

| # | Rule |
|---|------|
| V1 | **Zoom about a point**: the window is scaled by a factor f around the given point P, so P stays where it is: `x0' = Px − (Px − x0)·f`, `y0' = Py − (Py − y0)·f`, `w' = w·f`, `h' = h·f`. Zooming in uses **1.0 / f**, so in and out exactly undo each other. Two factors exist in the settings (one of them for the step zoom commands). |
| V2 | A new window narrower or lower than **1e-6** is ignored — there is no other zoom limit. |
| V3 | Every view change first **stores the current window** in a list, which is what *Zoom Previous* pops and restores. |
| V4 | **Zoom All** fits the drawing's limits: in nesting, the sheet's limits; in the drawing, the extent of all its elements ("good" and "bad", so technology elements too), widened for several parts opened together. An empty drawing gets 0…99999.99999. |
| V5 | The **default style takes the Windows colours** (every background is the system window colour); a named style is read from `Styles\<name>.xml` instead (the install ships Office2013 and Office2016). |

How the rebuild uses them:

- Part view: the wheel zooms about the pointer (V1) by `exp(0.0015 × scroll)`, so scrolling back
  the same amount returns to exactly the same zoom (in = 1/out). The − / + buttons step by 1.25
  and 1/1.25. Zoom stays between 0.3× and 40× of the fitted size (ours; the original only has
  V2's tiny floor).
- **Pan**: middle-drag, or hold Space and drag (left-drag stays the selection window,
  right-click the radial menu).
- **‹ Previous view** (V3): each button zoom, Fit, pan, and each burst of scrolling (a new one
  after 0.4 s without scrolling) stores the view first; the last 20 are kept.
- **Fit** (V4) frames the part *and* its leads, lead-outs, loops and joint leads, with 64 px of
  room for the size labels.
- Light/dark (V5): with no mode chosen the app follows Windows, live; picking a mode with the
  switch stops that, and *Options → Follow Windows light / dark* turns it back on. The sheet view's
  wheel zoom uses the same symmetric factor.

## Remnants and free cut lines (cut-offs without a part)

The nesting Home tab's **Remnants** group has: Create remnants, Delete, Semi-Automatic,
Configure, **Draw the Edge** (draws the remnant's cutting line), **Automatic Edge**, Remnant
reference, **Draw the Exact Edge** (between parts), Delete the Edge, Rotate remnants, **Manual
straight line** and **Modify Edge** (moves the remnant's cutting line). Its settings include
*Round the remnant line*, *Minimum area of remnants*, *Remnant margin*, *Cut the remnant line*
and *Punch the remnant line at the beginning of the sequence*.

| # | Rule |
|---|------|
| RM1 | The automatic remnant line's X is the sheet origin + **cut width × 0.5** + the distance past the parts; mirrored when measured from the other side. A line too close is pulled to the minimum distance, and a remnant narrower than the minimum is **refused** (result code 10). The same in Y. |
| RM2 | The automatic line is only generated when the sheet has no remnant line yet. |
| RM3 | **Round the remnant line** puts a fillet between consecutive elements of the line that are not tangent or aligned — it rounds the steps' corners, not the position. |
| RM4 | Part contours are checked against the remnant and the remnant line. |
| RM5 | A remnant is "rectangular" when it has one contour that is a rectangle. |
| RM6 | The remnant line is cut after the parts unless *Punch the remnant line at the beginning* is set; manual cut paths are their own machining instructions (Machining → Contours → *Manual Contours by Means of Two Points*). |

How the rebuild uses them (Sheet tab → **Remnants & cut lines**; Home tab → *Remnant line*):

- **Automatic edge**: the line in the shape set in the Sheet Remnants options (ST8), the offset
  past the parts (default 10 mm, ST11), or the cutter radius when that option is on. The irregular
  "at the cut" shape always uses the cutter radius (at least 0.03 mm). Refused when less than the
  minimum width (default 100 mm) would be left.
- **Exact edge**: a stepped line that follows the parts band by band, the part gap + half a cut
  (0.3 mm kerf when the machine gives none, estimated) past them (RM1), so the remnant keeps every
  free corner; bands shorter than 50 mm are merged into a neighbour (no teeth or slivers).
  Refused when the remnant is under 100 mm × the sheet side (RM1's minimum; value estimated).
  **Round corners** (on by default) rounds each step with a 10 mm radius (RM3).
- **Straight line**: click two points (ends snap to the sheet edges; Shift squares it). **Draw
  path**: click as many points as you like, double-click or Enter to finish, Backspace undoes a
  point, Esc stops. A line from one sheet edge to the opposite one becomes the remnant line;
  anything else is a free cut path — a cut on the sheet with no part.
- The preview turns red, and the line is refused, if it would cut into a part (it must stay half
  a cut clear of every outline, RM4).
- One remnant line per sheet (a new one replaces it, RM2). The remnant is hatched with its size
  and area. Click a line to select it, **Delete** removes it, and dragging a straight line moves
  it across itself (Modify Edge) — moved back if it would hit a part.
- In the machine program the free paths are cut after the parts as `( CUT LINE )`, then the
  remnant line last as `( REMNANT LINE )` (RM6).
- Not imitated yet: saving remnants into a sheet stock for later jobs, remnant references,
  rotating remnants, the remnant margin setting, and punching the remnant line first.

## True-shape: every part follows its contour

The original never settles for the box of an irregular part: its collision test walks the
contours element by element.

| # | Rule |
|---|------|
| T-S1 | Two contours collide if **any element** of one hits the other: for each element of the first, by its type — 1 segment, 2 arc, 4 circle — the matching exact test is run against the other contour, stopping at the first hit. |
| T-S2 | Against a contour, each element is first tested by its **box** (a fast pre-check) and only then exactly: segment–segment, segment–arc, segment–circle, with a distance tolerance. Arcs stay true arcs. |
| T-S3 | Arcs and circles are kept as arcs end to end (there are arc–arc and arc–circle tests too), so curved features collide by their curve, not a polygon or box. |
| T-S4 | The full true-shape automatic nesting (parts interlocking by outline) lives in the original's separate nesting engine — not confirmed here. |

How the rebuild uses them:

- **Every part collides by its contour** (T-S1/T-S2): the boxes are only the quick first check,
  then each edge of one outline is measured against each edge of the other (edges far apart by
  their own boxes are skipped). Round parts use their exact circle; curves are our outlines'
  fine polygons (we don't keep true arcs — T-S3 is approximated).
- A part may sit **inside another part's hole** when it fits wholly inside it with the gap all
  round; across a hole's edge it clashes.
- **Contour compaction**: after *Nest all*, and on *Compact to left*, every part slides left,
  then down (3 passes), until its contour is one gap from another contour or the margin. Parts
  stop at the real edge, so L-brackets and gussets tuck into each other's notches. Where edges
  are slanted the slide backs off (14 bisection steps) until the true clearance holds.
  Compaction works on whole rectangular sheets only: on a remnant with an outline *Compact* is
  refused (CP1), as it would slide parts toward the box's corner, where there may be no material.
- **Dragging, dropping and the ghost** use the contour test, and the ghost's approach snap slides
  to the nearest contour within reach (then once across, into corners and notches).
- **Picking** a part on the sheet follows its outline, so the right part is picked where boxes
  overlap; clicking in a hole doesn't pick the part.
- The automatic nesting places parts by their outlines too: see "True-shape strip nesting"
  (quarter turns and mirrors only, no free angles).

## Texts on parts (marking)

Texts on parts, including the original's single-line marking fonts (`Mrkfonts\*.FNT`) and its
sheet-export text layers (`NESTTEXT.CFG`).

| # | Rule |
|---|------|
| T1 | A text is a geometry element with a **text type**. Type **1** is a marking text. Types **2–4** are special marking texts. A text counts as marking for 1 or 2–4; any other type is a drawing text that is shown but not machined. |
| T2 | Special texts are templates. Their parameter follows a **#**, and NAME=VALUE parts are split at **=**. The values are filled in from the part and job data when output. |
| T3 | The mark technology is stored as a machining attribute on each element. Assigning it first checks the element doesn't already carry a cut or mark attribute (kinds 1–4 and 8–11). |
| T4 | Cut-or-mark technology codes depend on two flags (4 codes, plus an "auxiliary" set and a "delete" pair). |
| T5 | Text is drawn either from a TrueType font as outlines, or from a **single-line marking font**: one `.FNT` file per character code (e.g. 100 = "d"). Each line is a stroke `1 0 0 0 x1 y1 x2 y2` on a grid about 5 wide and 7 tall, and the file ends with `999`. |
| T6 | DXF TEXT and MTEXT are read as text elements. |
| T7 | Sheet DXF export puts texts on their own layers: `INFO_TEXTS` (colour 4) and `MARK_*` layers for marking. |

Lantek compared with the rebuild:

- **Before:** the rebuild ignored DXF text completely. The only marking was lines on DXF layers
  set to "Mark".
- **Now:** every part has texts with a type, as in T1:
  - **Marking text** and **special text** are marked. Special texts fill in `#REF`, `#QTY`,
    `#MAT`, `#THK` and `#NUM` (T2).
  - **Note** is only drawn (dashed) and never machined.
- The Technology tab has a **Text** group: type the text, pick the type and height, then
  **Place** (in the Part view, click where it starts; a preview follows the pointer) or **Add**
  (along the bottom of the selected parts). **Delete** removes the part's texts.
- Marked texts become marking strokes in the cut plan and the G-code (M10 … M11). They are
  marked before the part is cut, at the marking speed, and their length and time count in the
  costs.
- DXF import (T6): a text on a layer set to Mark becomes a marking text, one on a cut layer
  becomes a note, and one on a left-out layer is dropped.
- The single-line font is the rebuild's own: 4 × 6 grid strokes in the style of T5. Lantek's
  font files are not copied. TrueType text and the exact technology codes (T4) aren't imitated.

### Texts on placed parts (kept readable)

How marking texts behave when their part is turned or mirrored.

| # | Rule |
|---|------|
| T8 | When a part is placed turned or mirrored, its marking text's four box corners are moved with it. For a **mirror** the corners are re-ordered, so the text is **never mirrored**. |
| T9 | The new angle (the moved baseline's angle, plus the text's own angle, brought into 0–360°) is **snapped**: no step → as is; a step of **359.9°** or more → **0°** (always level); otherwise the nearest multiple of the step. |
| T10 | If the snapped angle differs from the real one by more than **0.1°**, the text is turned to it: about its box's middle for special texts, about its insertion point for others. |
| T11 | A text record keeps its type, height, two angles, two alignment flags and a box around the insertion point of ±0.5 × width and ±0.5 × height. Special texts take their box size as given, and type 3 has a square box (a QR code, inferred); other texts are measured from the font. |

How the rebuild uses them:
- On the sheet and in the cut plan, every straight text is drawn fresh from its placed middle
  (T8/T10). It is never mirrored, and its angle is snapped with the T9 rule. The step setting's
  default isn't known, so the rebuild uses 360°: marking texts on the sheet are
  always level and readable, whatever way the part was turned. In the Part view, texts show at
  their own angle.
- Curved texts (a rebuild addition) keep following their arc. On a mirrored part they are
  redrawn readable on the mirrored arc.
- The rebuild's insertion point stays the lower-left corner, because DXF TEXT gives that;
  T11's centred box isn't copied.

### Picking and selecting texts

How texts are picked with a click or a window.

| # | Rule |
|---|------|
| T12 | A click **captures a text** when the capture square crosses one of the **four sides of the text's box**, the turned box from the four corners in its record, or when the square lies inside the box. |
| T13 | The **distance to a text** is the shortest distance from the point to its four box sides. Of several captured texts, the nearest wins. |
| T14 | **Window selection** takes a text only when **all four box corners** are inside the window. |

How the rebuild uses them:
- Right-clicking or double-clicking a text to edit it (the quick text editor) picks with T12/T13:
  the turned box of a straight text, the box around the strokes of a curved one, and the
  nearest box side when texts overlap.
- Dragging a box in the Part view also selects the texts lying wholly inside it (T14), or dragged
  to the left the texts it touches (SW3). They are
  outlined, and **Delete** removes them.

### Rebuild additions (not from the original)

- **Curved text**: a text can follow an arc (`curve` = radius in mm). Positive values put the
  letters outward, reading clockwise over the top of a circle; negative values put them inward,
  reading round the bottom. The Text group's **Arc** sets the radius for a clicked text.
- **Text relative to an element**: select a line, arc or circle in the Part view and the Text
  button becomes **On element**:
  - Along a line, the text runs parallel to it (always reading left to right), offset 0.3 ×
    height into the material.
  - Round an arc or circle, the text is curved and sits outside a hole or inside an outer edge,
    upright at the top or bottom.
- **Radial tool menu**: right-click (or S) on the Part view opens the quick ring's
  technology tools (see "Quick ring", QR1).
- **Quick text editor**: the ring's **Text** and **Text here** open a small editor at the
  pointer with the cursor already in the box, so you just start typing.
  - In the editor: pick Marking, Special or Note; insert #REF, #QTY, #MAT, #THK or #NUM (which
    makes it a special text); set the height and the arc.
  - Enter (or the blue button) places a new text with a click, or along the selected element
    for Text here.
  - Over an existing text the ring shows **Edit text**, and double-clicking a text does the
    same. It opens the editor on that text, which is outlined on the part and changes live as
    you type.
  - **Done** keeps the change, **Delete text** removes the text, and **Esc** puts it back as it
    was.


## Lantek Expert v44 new features (from its new-features notes)

Sized by what each would take to rebuild here. What isn't built is tracked in FEATURES.md.

- **HUGE:** New Import Assistant (4.4); component extraction — stops, traps, lifts, magnetic cups with a 10 s timeout in LANTEKPR.CFG V145, lift strip collisions (3.4.2, mostly punch); nesting parts grouped by attributes into zones with remnant lines (3.1.1).
- **Big:** nano-joints (2.2.1: reduced-power partial cut, point or length, qualities, green colour, automatic at contour ends; needs post support); Verify Machining command (3.1.4) — **built**.
- **Medium:** part properties from the file name (4.1/4.3, format in D018); text/QR/template command with preview (3.1.6, partly built); pre-cutting cycles on lead-ins (3.1.5); multi-head ascending order (3.2.1); punch-only options (3.3.1, 3.3.2, 2.3.1).
- **Small:** select equal / inside / outside contours (2.1.1-2.1.2); Part Viewer reference and size, Ctrl + scroll (3.1.2-3.1.3); active/inactive machines (2.1.3); multi-edit "<Different values>" and shortcut list (1.1-1.2).

| # | Rule |
|---|------|
| VR1 | Verify Machining runs on the current sheet or all sheets of the subjob; with several sheets the panel shows how many are correct and incorrect, and each sheet's result can be opened. |
| VR2 | "Check safe distance": the minimum distance between parts is either the separation of parts set for the machine's material/thickness or a typed value. |
| VR3 | The rebuild measures outline to outline (or to the hole a part sits in) and marks each pair closer than the distance with a red line on the sheet; holes-before-outside is checked per part (`v44::safe_distance_issues`, `drawing::verify_machining`). |


### Verify Machining panel

| # | Rule |
|---|------|
| VM1 | Cut settings group: **"Check safe distance"** checkbox; a list with **"Value"** then **"The separation of parts"**; a **"Minimum distance"** number from 0 to **99999.999** mm; **"Include auxiliary technologies during the test"**. |
| VM2 | The list and the distance are only enabled while the checkbox is on. With "The separation of parts" the distance shows the machine's separation and can't be edited; with "Value" it is typed. The stored setting is choice × checkbox (0 = off). |
| VM3 | Each sheet's result is a list of codes, and each code has its own message; with no error the app says "No problems found — all machining is in place" (the original's own wording reads as if the sheet had been cut), and the panel lists what it checked (parts touching, overlapping or too close, parts outside the sheet, cut contours without lead-ins). Code **20 = "Not enough separation between parts"**; 3 overlaps; 4 out of the work zone or in clamps; 8 cut contours without lead-in/lead-outs; 10 remnants or parts that fall; 14 not enough micro-joints; 16–19 collisions. |
| VM4 | Totals: a sheet whose first code is 0 counts as correct, otherwise incorrect; shown as **"Correct sheets: n"** / **"Incorrect sheets: n"**. |
| VM5 | Per-sheet results show one icon and message per code; the heading is **"Current sheet"** in nesting (the totals of VM4 whenever all sheets were checked, even a single one) and "The selected part" in the Drawing module. |
| VM6 | The machining checks themselves (punch overlaps, clamps, extractions, shearing, repositions) return one counter per error type. |

The rebuild (src/v44.rs) uses the same codes and messages: separation (20) and touching parts (3) from
the outline-to-outline distance, parts outside the sheet (4), cut contours without a lead-in (8) and
holes cut after the outside (10, "parts that fall"); its own pierce check is listed separately.

| # | Rule (the rebuild's own checks) | |
|---|---|---|
| VP1 | **A part drawn so it would be cut wrong** is listed on every sheet it's on ("A part's drawing would be cut wrong"): an outside contour that isn't closed or crosses itself, a hole not wholly inside the outside contour (or touching it), holes that overlap. These are Verify Part's geometry checks; a text sticking out of the part is not one of them. **Export and the agents' `program` stop** on it and say which part and why. | estimated (own design) |
| VP2 | **A lead-in that pierces off the sheet** (its pierce point, or any point of its path, more than 0.01 mm outside the sheet's edges) is listed as code 4, out of the work zone, with where it starts; **export and `program` stop** on it and say to raise the margin above the lead length, shorten the lead or move the part in. A shaped remnant is checked against its box (FEATURES MAC-49). | estimated (own design) |
| VP3 | **The sheet's edge is an obstacle for leads**: a lead whose path would leave the sheet counts as a collision, so an automatic lead moves along its contour and/or shortens as for another part (AL11), and a fixed lead (picked, machine position) is shortened down to the floor (max(2·tol, B295)). What still leaves the sheet is stopped by VP2. | estimated (own design) |

When an export stops (VP1, VP2, or the LC5 read-back BL39) nothing is written and the job's earlier
programs in the folder are removed, so they can't be run by mistake. The
panel follows VM1–VM5 (list order, read-only distance for the separation of parts, 0–99999.999 range,
"Correct sheets:" / "Incorrect sheets:", "Current sheet"). Not built: auxiliary technologies (the
rebuild has none) and the punch-only codes.


## BLM LC5 programs (Siemens 840D) — the post that writes them

The LC5 doesn't read a plain G-code .CNC. Lantek runs the machine's post (see "How Expert makes a
machine's programs"); for the BLM LC5 it is the "ADIGE (Laser Siemens 840D)" post, settings
`MchFiles\PstAdi06.CF1/.PS1/.CT1/.se1`, which writes per sheet a set of Siemens subprograms and an
XML job sheet, and has them zipped. The rules below were checked against an example program set
(`<NEST>.XML` + `.zip`).

| # | Rule |
|---|------|
| BL1 | Files per sheet: `<NEST>.SPF` (sheet), `<NEST>7nn.SPF` (one per part), `<NEST>LIS.SPF` (lists the sheet and part SPFs, not itself), all zipped as `<dir>\<NEST>.zip` (deflate, no folders); the original's post has the zip made by a separate zip helper program, called with the file name, which isn't always installed. Lines end CR LF. |
| BL2 | XML job sheet `<NEST>.XML` (utf-8, tabs): General → NestingName, JobName, Machine, MatName, MatId, MatAlias, Thickness, Length, Width, PartXMax, PartYMax, Quantity, NestingTime (s), GasId, GasName, TechnoTable, UserData1-5, Files/SubPrg(CNCSub, PartRef), Pezzi/Istanza(Name, Length, Width, Quantity, PieceTime, WorkOrder, Order, UserData1-8, Executions/Execution(Quantity, SubProgram/CNCSub)), Utensili/Foratura Id0-6 and Taglio Id0-12 (1 = that pierce type / cut quality is used). Written when CF1 **A049** = 1 or 2 (2 adds times) / PS1 **Z059** = 1 (the standard settings have CF1 A049 2, PS1 Z059 1). |
| BL3 | Subprogram name = nesting name up to the first "." (cut to 5 characters when 6-8 long) + **700 + part number** (701, 702…). |
| BL4 | Sheet program: `;ADILAS`, `;MAT,length,width,thickness`, `;IDMAT=`, `;NOMEMAT=`, `;ALIASMAT=`, `;IDGAS=`, `;NOMEGAS=`, `;Postprocessor version 2.0.0`, `;R220=Z16*1000+2*100+0`, `R12=`thickness, `R201..R203=0.0`, `R21=`length, `R22=`width, `R23` / `R24` / `R523` / `R524` = the free margins (BL17), `R500=0`, `INITNEST`, then per part `ORIG(x,y,angle)` (PS1 Z005 = 1; 0 gives `TRANS X.. Y..` + `AROT Z..`) and `N<k> CALL"/_n_wks_dir/_n_sheet_wpd/_n_<SUB>_spf"`, then `ENDNEST`, `RET`. Warns "Numero sottoprogrammi superiore al massimo consentito - **96**" (Z027). |
| BL5 | Part subprogram: `;M01`, `R16=chk_pret` (Z056), `CICLA:`, `INITPART(1)`, `;<part>`; per path `NEXT_L(x,y,first?0:1,pierce,quality)` (pierce and quality: BL22), lead-in `G01 G41/G42 X Y`, `PROFILE`, moves `G01/G02/G03` with `CORNER` after a corner whose angle is at most **119.9°** (Z043), last move as `LASTLINE(0,0,0,…,x1,y1,x2,y2,0,0)` (10 parameters, Z061; BL27); then `ENDGEO`, the pre-cut retry `IF R16==1 / STOPRE / R16=0 / GOTOB CICLA / ENDIF`, `ENDPART`, `RET`. Coordinates up to 3 decimals, whole numbers as `12.`. |
| BL6 | Material codes from PS1 D-lines: X5CrNi 0100, RST 37-2 0010, Al 1050 A 0200 (Z063 = 1 takes alias and id from the system). |

The rebuild (src/blm.rs): **Machining -> Post settings -> "BLM LC5 output"** (on when the active machine
is an LC5/BLM) makes Export CNC write `<NEST>.XML` and `<NEST>.zip` (sheet SPF, part SPFs, LIS) per sheet
following BL1–BL5, zipped in-app with deflate. Estimated / not built: when `LASTLINE` starts `1,<lead-arc radius>,0` instead of `0,0,0` (BL27),
micro-joints (refused, BL39), the FINAL_CUT / remnant-line subprogram (`HD_DWN_OFF`), part rotation through ORIG's angle
(each rotation gets its own subprogram instead), and gas defaults.


### BLM LC5 — arcs and cut-off lines

| # | Rule |
|---|------|
| BL7 | Curves are written as arcs: `G02`/`G03 X.. Y.. I.. J..` with I/J = centre − arc start (incremental), e.g. `G03 X51.31 Y37.345 I0. J-1.`. The rebuild fits arcs to the contour's points: every point within **0.005 mm** of one circle (the post library's machine tolerance, CF1 A005, which may tighten this but not loosen it), every move between points within **0.105 mm** of it (BL38), turning one way, at most **30°** per step (so a square's corners stay corners), radius at most 5000 mm, at least 3 moves, less than a full turn. The last move stays the LASTLINE straight. |
| BL8 | Cut-off / remnant line = its own subprogram, PartRef and Istanza Name **FINAL_CUT** (no PieceTime/WorkOrder/Order), called after the parts with ORIG at the line's start: `NEXT_L(10u,0,0,3,1,20u,−10u)`, `G01 G40` to 10 mm in, `PROFILE`, cut to 10 mm before the end, **`HD_DWN_OFF`**, run on to 10 mm past the end (u = the line's direction). 10 mm = PS1 Z039/Z040 sheet-trim overruns (Z038 = 1). Lines that aren't one straight edge-to-edge segment are written as an open cut path. |
| BL9 | Repeated points are dropped so no zero-length moves are written (rebuild). |

Still not written for the LC5: micro-joints and corner loops, which stop the export (BL39; PS1 Z051 "LASTLINE su microgiunzioni"), part rotation
through ORIG's angle, part times (PieceTime), and the meaning of NEXT_L's 6th–9th values for the
cut-off line (read from the example; `20u, −10u` is inferred from one vertical line).


### BLM LC5 — the post's arcs and the sheet trim (cut-off line)

| # | Rule |
|---|------|
| BL10 | An arc move is written `G0n X<end x> Y<end y> I<cx − previous x> J<cy − previous y>` — I/J are the centre relative to the point the move starts from (the last written X/Y). |
| BL11 | The sheet trim (Z038) classifies each end by sheet edge: 1 = x below the tolerance, 2 = y below it, 3 = x at/after the sheet length, 4 = y at/after the width; later tests win (4 > 3 > 2 > 1). |
| BL12 | Start: approach point outside by **Z039** on edges 1/2 (x or y − Z039) and by **Z040** on 3/4 (+ Z040); cut start inside by **Z040** on 1/2 (+) and **Z039** on 3/4 (−). With PS1 Z016 = 3 (LT9) the approach isn't a `G0` move but goes into `NEXT_L(start,0,0,3,1,start − approach,approach)`; otherwise `SPYOUT`, `DO $A_OUT[7]=1`, `G0 X.. Y..`. Then `G01 G40 X.. Y..`, `PROFILE`. |
| BL13 | End: cut to **10 mm** inside the end edge, `HD_DWN_OFF` (Z016 = 3 only), then out past the edge by Z040 (edges 3/4) or Z039 (1/2). |

### BLM LC5 — each contour's quality and pierce

| # | Rule | Source |
|---|------|--------|
| BL22 | `NEXT_L(x,y,h,p,q)`: **q** is the contour's own cut quality (CQ1), so small holes go to the LC5 at their size band's slower quality; **p** is the pierce mode the machine's technology table gives that quality (field 26 of the quality's block: 1 smooth, 2 fast, 4 booster) in the record picked for the sheet (TT9). Marks are always **p 0, q 9**. With no table record, or no pierce mode for that quality, p is **2**. Not built: p 3 for a cut starting on a line already cut (common cut, MAC-2), p 1 or 2 for a contour without a lead-in when PS1 Z030 = 0, the post's quality swap (PS1 Z041) and drill marking; real programs also show p 0 at quality 3 on some single outer contours, not traced. | Observed (q, p from the table, marks); real programs cut the smallest holes at 1, then 2, outlines at 3. p 2 without a table is estimated (the example program's code) |
| BL23 | The XML job sheet's tools: `Foratura/Id<p>` is 1 for every pierce code the sheet's NEXT_L lines use and `Taglio/Id<q>` for every quality (marks: Id0 and Id9). A sheet with a cut-off line also flags pierce 2 and quality 3. | Observed; the cut-off line's flags are estimated |

### BLM LC5 — compensation without a lead-in, repeated parts, stale programs

| # | Rule | Source |
|---|------|--------|
| BL24 | A contour **without a lead-in** still gets its G41/G42: with PS1 **Z028** above 0.1 mm (4 mm in the shop's file) the first straight move is split, `G01 G4x` to Z028 along it (at most half of it), then `PROFILE`, then the rest of the move. A contour that starts with an arc gets a zero-length `G01 G4x` at its start first. The rebuild reads Z028 from the PS1 (built-in 4). | Observed |
| BL17 | The sheet program's **free margins**: `R23` and `R24` are the room left to the far X and Y edges, `R523` and `R524` the room left to the near ones, measured from the points of everything written (lead-ins, marking, cut-off lines), so a written arc can reach up to 0.105 mm past them. Never below 0; 0.1 mm or less counts as 0. | observed in real programs (holds on 71–79 of 79 sets; L-shaped cut-offs differ, MAC-41) |
| BL27 | `LASTLINE` starts **`0,0,0`**. Real programs write that on about 92% of contours; the rest have `1,<lead-arc radius or 0>,0`, and what decides isn't known (MAC-31). | observed; the choice is estimated |
| BL38 | A run of moves becomes one arc only if **each move between its points stays within 0.105 mm of that arc** (the 0.1 mm drawing tolerance plus the 0.005 mm machine tolerance). Without this a long edge between two small rounded corners, or a run of straight sides, would be written as a huge arc that leaves the part. | own design |
| BL39 | Before an LC5 set is exported or shown as a program it is **read back**: the text is parsed on its own and each cut is compared with its own planned path, both ways, at 0.125 mm, along with its compensation word and direction. A mismatch, a missing or unplanned part or cut-off line, or a part planned with micro-joints, corner loops or punched holes (which the files can't carry yet) stops the export: nothing is written, and the job's earlier programs are removed, or the message says which couldn't be. A write that fails partway removes them too. Lead-outs and overlap aren't written and don't stop it (MAC-30); a cut-off line is checked for its start, the line it runs on and compensation off, not where it stops (MAC-42). | own design |
| BL37 | Copies of a part share one subprogram only when their **whole machining is the same** relative to their corner (turn, mirror, start point, leads). A copy whose lead was moved, for example clear of a neighbour, gets its own subprogram; otherwise it would be cut with the first copy's lead. | Rebuild |
| KC19 | A post that turns one axis round swaps **G2 and G3** as well as G41 and G42; an arc that kept its word would cut the rest of the circle. | Rebuild |
| CO1 | **Cut order with parts in holes**: the part inside the most other parts' boxes is cut first, so with rings in rings the innermost goes first and no hole drops with an uncut part in it. | Rebuild |
| CO2 | **Manual cuts with parts in holes**: a hole is never cut before a part sitting in it. Picking the hole, or the outside of the part it belongs to, cuts the parts in it first (deepest first; a pick on one of them keeps its start and direction). Verify Machining reports a part cut after a hole it sits in. The LC5 calls each part once, when its outside is cut, so a part in a hole is called before its frame even when one of the frame's other holes was picked first. | Rebuild |
| EXP1 | **Export CNC replaces the job's programs**: in the job's export folder, programs this job wrote earlier and this export didn't (sheets that are gone, or the other format, plain G-code vs. LC5 `.zip` / `.XML`) are removed, so nobody runs a sheet that no longer exists; with no sheets left, exporting refuses and removes them all. Other files are left alone. Remnant marks and cut lines run at their own speed (marking speed, or the normal cutting speed), not the last contour's. | Rebuild |

The rebuild reads Z039, Z040, Z043 from `MchFiles\PstAdi06.PS1` and A005 from `PstAdi06.CF1` at export
(`blm::adi()`; built-in 10 / 10 / 119.9 / 0.005 if the files are missing), classifies trim ends by
edge and writes BL12–BL13 exactly (the example's FINAL_CUT is reproduced line for line by a test).
Estimated: the low-edge tolerance (0.5 mm — the original's value couldn't be confirmed; PS1 B005 "Tolerance out
of the sheet" is 0.5).


## ByStar Fiber laser — the original's post (BV21–BV55)

The original writes ByStar Fiber programs with its own post: for every nested sheet it writes a
`.ccp` cut program (the machine's XML cut program) and a job file, and zips them into a `.bvc`.
Rules BV1–BV20 (spec #122) describe the format as the machine's own CAM (BySoft) writes it; the
rules below are what the original's post writes. Unless a rule names a switch, it describes the
post with its shipped switch values (the table after the rules). Observed = the original's
behaviour is known; estimated = our best guess. None of it has been seen on a machine yet.

### Files and the program frame

| # | Rule | Source |
|---|---|---|
| BV21 | **Files per sheet program:** `<program>.ccp` and `JobInfo.xml` (fixed name, rewritten for each program) in the program folder, and the nesting picture copied to `<program>.png` when the original has one. The three are added in that order to `<program>.bvc` (a zip); each only if the one before was added. A palletising record writes a palletising line instead and no job file or zip. | observed; the picture is the original's own drawing (estimated) |
| BV22 | **B010** on: the loose files are deleted once they are in the `.bvc`. **B017** on: every program of one run goes into the **first** program's `.bvc`; each later program adds its `.ccp` and picture and replaces `JobInfo.xml`, so the zip's job file describes the last sheet only. | observed; the replacement is estimated |
| BV23 | **Cut program frame:** `<?xml version="1.0" encoding="utf-8"?>`, `<CuttingControlProgram MeasurementSystem="Metric" Thickness="t">` (`ToolArrangement="Vertical"` only with **B041**), an empty `<Technology />`, `<ApplicationFlat PlanWidth PlanHeight SheetWidth SheetHeight />`, `<SubPrograms>` … `</SubPrograms>`, then `<ControlCommands>` with `SetClamps Value="Off"` and one `CallSubProgram` per placed part, `</CuttingControlProgram>`. **PlanWidth / PlanHeight = the largest \|x\| and \|y\| of all cut end points** (arc extreme points count, rapids don't). No `PlanLoadingOffsetX/Y`, no `PartRefIds`, no `PartLink`. | observed |
| BV24 | **Ids:** one counter from 1, +1 per command in writing order: every subprogram's commands first (the `SubProgram` element takes an Id too), then `SetClamps` and the calls. Vaporise copies (BV42) are numbered after the rest. Number form: BV52. | observed |
| BV25 | **Units:** **B001** only changes the text to `MeasurementSystem="Imperial"`; the post never scales a coordinate. Inch numbers come from the post library's output unit (CF1 **A006** = 25.4), before the post sees them. The job file always says `Metric`. | observed; the A006 path is estimated |

### Subprograms and calls

| # | Rule | Source |
|---|---|---|
| BV26 | **SubProgram header:** opened at the first move of a subprogram (closing the previous one). `Name` = text **C004** + the running subprogram count (C004 empty: `1`, `2`, …); cut-off subprograms are all named `ResidualSheetCuts` (no number) unless **B016** or **B028**. A part's subprogram adds `PartNumber` (the part's place in the part list; **B016**: the subprogram name), `PartWidth`, `PartHeight` (the part's box). A cut-off subprogram adds the cut lines' extent (+B003 on the edge side); others get `PartHeight` only with **B027** (10 if under 0.5). | observed |
| BV27 | **CallSubProgram:** X/Y = the CAM's subprogram origin, `Rotation` = its angle; cut-off subprogram calls are moved by B003 toward the edge they start from (minus the first overrun in two-line mode). Name as in BV26; vaporise copies `Vaporization_<n>`. For a CAM pattern repeat only the last grid position is kept. | observed; the grid loss is estimated |
| BV28 | **B002** on: every cut, cut-off lines included, is put into subprograms before the post runs. **B032** on: a subprogram that ends inside an open cut-out gets `EndCutOut`. | observed; B002's effect on the CAM side is estimated |

### Contours

| # | Rule | Source |
|---|---|---|
| BV29 | **Order at each contour start:** `SetOperationMode` (only when it changes, BV51) → `SetSensingSensor` (BV37) → `SetToolCorrection Left/Right` (only when it is off; **B011** puts it after the move) → `MoveLineTo` → with a lead-in: `BeginLeadIn`, `SetPiercingMode`, `ExecPiercing`, the lead moves, `EndLeadIn`, `BeginCutOut` → the contour's `LineTo` / `ArcTo` → lead-out `BeginLeadOut` … `EndLeadOut` → `EndCutOut` → `SetToolCorrection None`. Without a lead-in: `BeginCutOut` right before the first move and **no pierce command at all**, unless **B044** (pierce before `BeginCutOut`), engraving with **B036**, or a point cut (**B050**). | observed |
| BV30 | **`BeginCutOut Contour=`:** `Exterior` for a part's outer contour and for cut-off lines; `Interior` for an inner contour (BV50), a point cut, or engraving with **B036**; otherwise `Unknown` (engraving and marks by default). A contour in the marking quality **B026** never gets `Exterior`, and gets `Interior` only through B036. | observed |
| BV31 | **SetToolCorrection side:** when it is off, side left → `Left`, any other side (no side included) → `Right`; a contour already compensated with the "other" side value → `Right` again. So by default **marks get `Right`**; **B031** leaves marks and point marks uncompensated. `None` comes after every `EndCutOut` (if not already off), when entering engraving with **B036**, and when leaving engraving with **B012**. **B040**: no compensation inside a lead-in, so it is written at the cut-out start (the machine CAM's order, BV7). | observed; the "other" side value is estimated |
| BV32 | **Operation mode** (the machine's technology column): q = the contour's laser quality 1–12; **B042** replaces quality 7 and **B043** quality 3 by their own value; in a cut-off line **B045** replaces q. **B015 off:** q2→1, q3→3, q4→4, q5→5, q6→6, q7→7, anything else (q1, q8–q12) → **0**. **B015 on:** the technology table's per-quality operation-mode value (T419). Marking technology, or q = **B026**, → **2**. | observed |
| BV33 | **Pierce mode** (`SetPiercingMode`): **B014 on** → the technology table's per-quality pierce text (T421; marks use the marking block's); empty → no pierce commands at all. **B014 off** → `Fine` for cutting, `Normal` for marking; then q = **B007** → `Normal`, q = **B008** → `Fine`, q = **B009** → `NoTime`; q = **B026** → `Normal`; a pierce on a partly cut common-cut contour → `NoTime` unless **B037**. A cut-off line's re-pierce is always `NoTime`. Engraving with B036: `Fine` (B036 = 2: `Normal`). | observed |
| BV34 | **Leads:** lead elements come from the CAM; `BeginLeadIn` before the first lead-in element, `EndLeadIn` after the last; `BeginLeadOut` before the first lead-out element and `EndLeadOut` after the contour's last element, **both before** `EndCutOut`. **B006**: a contour without a lead-in has its first move split at B006 (at most half its length). **B024**: lead-in bracket and pierce at every contour start, even without a lead-in (and with B040 no compensation at the rapid). | observed; B024's purpose ("no piercing in common cut") is estimated |
| BV35 | **Arcs:** `ArcTo X Y SweepAngle` (end point and signed sweep, no centre); which sweep is written: BV47. **B018** splits an arc into 180° pieces only when it turns **360° or more** (a full circle → two half circles); arcs of 180–360° stay whole. | observed |

### Micro-joints, sensing, cut-offs, marks

| # | Rule | Source |
|---|---|---|
| BV36 | **Micro-joints, overview (B038 off):** the cut before a joint gap is held back; at the gap the post writes that cut, a cut **to the middle of the gap**, and `ExecMicrojoint`, with no rapid and no new pierce: the path runs on through the gap and the machine leaves the joint (BV16). Details: BV46–BV48. | observed |
| BV37 | **Height sensor (B039):** at every contour start, a pierce point inside the sheet by more than the machine tolerance with the sensor off → `SetSensingSensor On`; one at or outside an edge with the sensor on → `Off`. The sensor starts **off** in each program (not in each subprogram), so the first contour gets `On`. **B039 = 1**: no `On` for marks; **B039 = 2**: no switching at contour starts, only the cut-off sequence's own Off/On. | observed |
| BV38 | **Cut-off line (one line edge to edge, B035 off):** `SetOperationMode` (always written), `SetToolCorrection None`, contour start at the pierce point (BV49) with the usual sensor and compensation rules, `BeginCutOut Contour="Exterior"`, pierce (BV33), `ExecPiercing`, cut B005 back toward the start edge, `SetSensingSensor Off`, cut out past the start edge, `SetSensingSensor On`, `EndCutOut`, **`ExecHighPositioning`**; then back at the pierce point: `MoveLineTo`, `BeginCutOut`, `SetPiercingMode NoTime`, `ExecPiercing`, cut B005 on, cut to the sensing-off point before the far edge, `SetSensingSensor Off`, cut out past the far edge, `SetSensingSensor On`, `EndCutOut`, `ExecHighPositioning`, `SetToolCorrection None`. **Where these fall on the sheet:** the cut-off subprogram is written with B003 added toward the far side, and its call is moved B003 outward from the start edge (BV27); together, for a line square to the edges, the pierce is **B004 + B005 inside the start edge**, sensing is off for the last **B004** before each edge, and the cut runs **B003 past each edge**. Inside the subprogram the same points read B003 + B004 + B005 from the start edge (BV49) and B004 + B003 back from the end of the far overrun. B003–B005 are whole units (BV54). A line shorter than B004 at its far end uses half its length there. With the shipped values (all 0) the line is pierced on the edge and has no overrun. | observed; the machine CAM's values are BV14 |
| BV39 | **Cut-off options:** **B020** counts only lines the CAM marks as cut-off lines (BV50). **B025**: a cut-off line that doesn't start on an edge gets its own rapid, pierce and `BeginCutOut Exterior`. **B035**: two-line mode, with the same two pieces as BV38 (pierce, cut back, sensing off, out past the start edge, lift; re-pierce `NoTime`, cut on, sensing off, out past the far edge, lift, compensation off). The differences: the subprogram call is still moved outward from the start edge (BV27), by B003 less the length the subprogram's first cut-off line already sticks out past the sheet, and the line's points are written moved inward by the same amount, so the line keeps its place on the sheet; each end then runs out past the edge it lies on by B003 less the length that line already sticks out; a line too short for B004 + B005 at each end gets B005 = a quarter of its length; a line whose far end is inside the sheet just ends there. **B046**: pierce B004 inside without the B003 shift, state reset after each line. **B048** (with B035 and B046): per edge (the side a line ends on: bottom, left, top, right) inside distance **R000–R003** and outside distance **R004–R007** replace B004/B003, not rounded, and the last `SetSensingSensor On` is put off until later. **B045**: quality for cut-off lines. | observed; B046/B048 geometry estimated |
| BV40 | **Point marks** (a point cycle): **B013** above the machine tolerance (shipped 0.2) → a circle of diameter B013 around the point: `SetOperationMode 3` (**B033**: 2), sensor, `MoveLineTo` centre, compensation, `BeginLeadIn`, pierce, `LineTo` (x + d/2, y), `EndLeadIn`, `BeginCutOut`, `ArcTo` (x − d/2, y) sweep 180, `ArcTo` (x + d/2, y) sweep 180, `EndCutOut`. **B034** above the tolerance → a cross of size B034 in mode 2 instead (each stroke led in from its end to the centre and out). **B013** at or below the tolerance → nothing, except a point cut with **B050** (BV55). | observed |
| BV41 | **Auxiliary codes and texts:** CAM aux 4 → `ExecOptionalStop`, 8 → `ExecOptionalStopWithoutLifting`, 1008 → `ExecDwell`; the scrap-destroy macro texts only change the part bookkeeping (no command). `ExecHighPositioning` is written only in the cut-off sequence. | observed |
| BV42 | **Vaporise all** (technology option 2): every subprogram is copied as `Vaporization_<name>`, each operation mode replaced by the technology's vaporise mode (T006) and each pierce by its vaporise pierce (T008; none if empty); the copies follow the last subprogram and each part call is written twice, the vaporise copy first. | observed; T006/T008 meaning estimated |

### Job file and settings

| # | Rule | Source |
|---|---|---|
| BV43 | **JobInfo.xml:** `JobInfo` with `Guid` (new, no braces), `Name`, `Material`, `Thickness`, `MeasurementSystem="Metric"`, `Parameter` = the technology's parameter-file name (T009) and **`ProcessGas` only when that name is set**. `PartInfos/PartInfo Id Name Quantity [OrderInfo]` — **no sizes, no shapes**. `MaterialInfos/MaterialInfo Id="1" Quantity SizeX SizeY FormatType="Sheet" ArticleInfo [ChargeInfo]` — no margins. `PlanInfos/PlanInfo Multiplicity Name MaterialId="1" EstimatedCuttingTime SizeX SizeY` (used extent) with `PlanPartInfos/PlanPartInfo PartId Quantity SubProgramName`, one per distinct subprogram; **B029** adds an empty `ResidualSheetInfos`, **B030** an empty `BigPartInfos`; then an empty `SortInfos` and `EstimatedCuttingTime` as a child. | observed |
| BV44 | **PlanPartInfo PartId / Quantity** = the part's place in the part list and its quantity, matched by name; **B022**: a PartId under 0.5 becomes the subprogram number (at most the largest part id), a quantity under 0.5 becomes 1. | observed |
| BV45 | **Settings with no effect on the files:** C001–C003 and C005–C013 (material, job and part user info, Guid, parameter, order info) feed only a second job file that is never written; B000 is unused. Only **C004** is live. | observed |

### Finer details (ticket #123)

| # | Rule | Source |
|---|---|---|
| BV46 | **Where a joint's gap lies.** The CAM leaves a joint as an uncut gap between two cuts: the gap starts where the first cut ends and ends where the next one starts. The cut before the gap is held back when the CAM marks it as a joint cut, except: a joint mark of the type the post skips (subtype 2), a cut that carries joint-lead data itself, a cut whose source element (the element it was split from) carries joint-lead data, and the contour's last two elements. At the gap the post writes: if the held cut and the gap lie on one straight line (within 0.01), one `LineTo` from the held cut's start straight to the **gap's middle**; otherwise `LineTo` the held cut's end, then `LineTo` the gap's middle. When the gap lies on an **arc**, its middle is taken **along that arc** (half the gap's arc length) and written as an `ArcTo`; a held arc and an arc gap become one `ArcTo`. **B049** treats every gap as straight (the chord). Then `ExecMicrojoint`. No rapid, no pierce, the cut-out stays open, and the next cut is written from the gap's middle. | observed |
| BV47 | **Which sweep an `ArcTo` gets, and the arc after a joint.** Each `ArcTo` carries the sweep handed to it for that move when its size is above the machine tolerance (CF1 A005); otherwise the sweep is worked out again from the last point the post recorded to the arc's end. Joint half-arcs and B018's 180° pieces hand over their own sweep. For an ordinary arc both ways measure from the recorded point, and after a joint that point is the **gap's far end** (where the CAM resumes), not the gap's middle where the path actually is. So the arc after a joint on an arc gets a sweep short by half the gap's angle, and a reader that rebuilds the arc from the written point, end and sweep gets a slightly different arc. | observed; the machine's reading is estimated |
| BV48 | **Joint switches.** **B019**: the `ExecMicrojoint` that goes before a joint's lead-in is written later, just before the next `MoveLineTo` or `EndCutOut` ("at the end"). **B023**: a joint's lead-in also gets `SetPiercingMode` + `ExecPiercing`. **B047**: a joint cut in the contour's second-to-last place (the very last element never) gets `ExecMicrojoint` before it, unless it carries joint-lead data or its joint mark is subtype 2. **B038** (new treatment): nothing is held; at the gap the cut-out is closed (`EndCutOut`, `SetToolCorrection None`) and the cut after the gap starts like a new contour (BV29: a rapid, and a pierce only with a lead-in or B044). | observed |
| BV49 | **A cut-off line's pierce point (B035 and B046 off).** Let d = B003 + B004 + B005. A cut-off line whose start lies outside the sheet, or inside it but within d minus the machine tolerance of an edge, is pierced where it **crosses the sheet outline shrunk by d on every side** (in the subprogram's own coordinates; on the sheet see BV38). This is not a distance along the line. For example, a line that starts on an edge and meets it at an angle is pierced d / sin(angle) from its start. If it never crosses that outline, its start is moved onto it. If the line is shorter than d, B005 becomes a quarter of its length and the pierce is (that + B003) from the start. A cut-off line that starts further inside gets no pierce of its own (B025 gives it one). | observed; the "moved onto it" case is estimated |
| BV50 | **Contour classes.** The CAM gives each contour a code; its last two digits are the class. **3** = inner contour (`Interior`, BV30). **4** = cut-off (remnant or skeleton) line. A cut that has no contour code at all also counts as a cut-off line, unless **B020** (then only class 4). A code above 99999 counts as neither. `Exterior` comes from the contour being a part's outer contour, not from its class. | observed; the meaning of 3 and 4 is estimated from how they are used |
| BV51 | **When the operation mode is written again.** The post remembers the last mode and writes `SetOperationMode` only on a change, except: it forgets the mode at the start of each program, at each new subprogram, and when a **new part** starts (not right after a point cut); and it always writes it for **every cut-off line** and **every point mark** (3, 2 with B033 or a cross, 0 for a point cut), even when unchanged. | observed; how the post tells a new part from common-cut neighbours is estimated |
| BV52 | **Numbers.** The decimal separator is always `.`; no `+` sign; leading `0` kept (`0.5`). Coordinates, sweep, thickness and sizes have **up to 9 decimals, trailing zeros dropped**, and no point at all when whole (`25`). Rounding is to the nearest, **halves toward zero** (in size). The minus sign comes from the value before rounding, so **−0.0 is written `0` but a tiny negative such as −0.0000000001 is written `-0`**. Ids, rotations and mode values are whole numbers; part call X/Y too (12.5 → `12`, 12.6 → `13`) unless **B021** (3 decimals). | observed |
| BV53 | **File form.** The cut program is written as text, one command per line, two spaces of indent per level, no block numbers and no leading or trailing fill characters; line ends are **CRLF**; no BOM. The text is converted to a code page that depends on the original's screen language: for its default languages (English included), the PC's Windows code page (ANSI); for the others, the code page the original's language list gives that language. The header says `utf-8` either way; for plain ASCII the bytes are the same. The only free text in the cut program is the C004 name prefix. How the job file's bytes are encoded wasn't settled. | observed; the job file's encoding is estimated |
| BV54 | **How the post reads its switches.** A switch missing from the file is 0, except B013 = 10. An on/off switch is on above 0.5 (a negative value is off). **B003–B009, B036, B039 are rounded** to whole numbers, nearest with halves **down** (0.5 → 0, 2.5 → 2, −0.5 → −1), so in an inch program B003–B006 are whole inches. B026, B042, B043 are cut to whole numbers (2.9 → 2). B010, B013 and B034 count only above the machine tolerance (CF1 A005, 0.005). B045 is used when above 0.5, cut to a whole quality. B039: 2 = no switching; any other non-zero value acts as 1. B036: 2 = Normal pierce, any other non-zero value = Fine. When B007, B008 and B009 name the same quality, Normal wins over Fine over NoTime. B042, B043 and B045 are **quality numbers**, not mode numbers. With B015 the table's value is written as it is. | observed |
| BV55 | **Point-cut subtype.** A point cycle is either a punch-mark point (subtype 1) or an on/off point (subtype 2); the CAM gives the type, and the post library's default (CF1 A004) is 2 = on/off for this machine. **B050** (when B013 gives no circle) writes an **on/off point** as a tiny cut: mode 0, sensor rule, `MoveLineTo` the point, `BeginCutOut Contour="Interior"`, `SetPiercingMode Fine`, `ExecPiercing`, `LineTo` (x − 0.05, y), `EndCutOut`. A punch-mark point still gets nothing. | observed; how the CAM sets the type and the A004 default are estimated |

### The post's switches

The original has no screen of its own for this post: its machine settings edit the post's
parameter file line by line (each line's own description is its label). The values below are
the shipped file's; "whole units" means the post rounds the value (BV54). `0` means off unless
said otherwise. Quality numbers are the machine's laser qualities 1–12.

| Switch | What it does | Values | Default |
|---|---|---|---|
| `B001` | Measurement system text in the cut program (BV25) | 0 metric, 1 inch | 0 |
| `B002` | Put every cut, cut-off lines too, into subprograms (BV28) | 0/1 | 0 |
| `B003` | Cut-off lines: how far they run past each sheet edge (BV38) | length, whole units | 0 |
| `B004` | Cut-off lines: height sensing off for this distance before each edge, measured on the sheet (BV38) | length, whole units | 0 |
| `B005` | Cut-off lines: run-up between the pierce and the sensing-off point (BV38, BV49) | length, whole units | 0 |
| `B006` | First move of a contour without a lead-in is split at this distance (BV34) | length, whole units | 0 |
| `B007` | Quality that pierces `Normal` (BV33) | quality number | 0 |
| `B008` | Quality that pierces `Fine` (BV33) | quality number | 0 |
| `B009` | Quality that pierces `NoTime` (BV33) | quality number | 0 |
| `B010` | Keep only the `.bvc`, delete the loose files (BV22) | 0/1 | 0 |
| `B011` | Compensation after the rapid instead of before it (BV29) | 0/1 | 0 |
| `B012` | Compensation off after a mark (BV31) | 0/1 | 0 |
| `B013` | Point marks as circles of this diameter; at or below the machine tolerance, no circle (BV40) | length | 0.2 (10 if the line is missing) |
| `B014` | Pierce mode from the technology table (BV33) | 0/1 | 0 |
| `B015` | Operation mode from the technology table (BV32) | 0/1 | 0 |
| `B016` | Subprogram name also as PartNumber; cut-off subprograms numbered (BV26) | 0/1 | 0 |
| `B017` | All sheets of one run in one `.bvc` (BV22) | 0/1 | 0 |
| `B018` | Split arcs of 360° or more into half circles (BV35) | 0/1 | 0 |
| `B019` | Joint marker "at the end": before the next rapid or cut-out end (BV48) | 0/1 | 0 |
| `B020` | Only lines marked as cut-off lines are treated as cut-offs (BV39, BV50) | 0/1 | 0 |
| `B021` | Part call positions with 3 decimals instead of whole units (BV52) | 0/1 | 0 |
| `B022` | Part id and quantity under 0.5 are fixed (BV44) | 0/1 | 0 |
| `B023` | Pierce at a joint's lead-in (BV48) | 0/1 | 0 |
| `B024` | Lead-in bracket and pierce at every contour start ("no piercing in common cut", BV34) | 0/1 | 0 |
| `B025` | A cut-off line not starting on an edge gets its own rapid and pierce (BV39) | 0/1 | 0 |
| `B026` | Quality used for marking: mode 2, `Normal` pierce (BV30, BV32, BV33) | quality number | 0 |
| `B027` | PartHeight in every subprogram header (BV26) | 0/1 | 0 |
| `B028` | Cut-off subprograms named like part subprograms (BV26) | 0/1 | 0 |
| `B029` | Empty `ResidualSheetInfos` in the job file (BV43) | 0/1 | 0 |
| `B030` | Empty `BigPartInfos` in the job file (BV43) | 0/1 | 0 |
| `B031` | No compensation on marks and point marks (BV31) | 0/1 | 0 |
| `B032` | Close an open cut-out at the end of a subprogram (BV28) | 0/1 | 0 |
| `B033` | Point-mark circles in marking mode 2 (BV40) | 0/1 | 0 |
| `B034` | Point marks as a cross of this size instead of a circle (BV40) | length | 0 |
| `B035` | Two-line cut-off mode: each end runs B003 past its edge, less what the line already sticks out; the call shift stays (BV27, BV39) | 0/1 | 0 |
| `B036` | New marking treatment: compensation off, `Interior`, pierce `Fine` (2: `Normal`) (BV29–BV33) | 0, 1, 2 | 0 |
| `B037` | Never `NoTime` on partly cut common-cut contours (BV33) | 0/1 | 0 |
| `B038` | New micro-joint treatment: stop at the gap and start again (BV48) | 0/1 | 0 |
| `B039` | Height sensor: 1 not switched on for marks, 2 only in the cut-off sequence (BV37) | 0, 1, 2 | 0 |
| `B040` | Compensation after the lead-in (BV31) | 0/1 | 0 |
| `B041` | `ToolArrangement="Vertical"` in the header (BV23) | 0/1 | 0 |
| `B042` | Quality that replaces quality 7 (BV32) | quality number | 0 |
| `B043` | Quality that replaces quality 3 (BV32) | quality number | 0 |
| `B044` | Pierce contours that have no lead-in (BV29) | 0/1 | 0 |
| `B045` | Quality for cut-off lines (BV32, BV33) | quality number | 0 |
| `B046` | New cut-off management (BV39) | 0/1 | 0 |
| `B047` | Joint marker before a joint cut in the contour's second-to-last place, not on cuts with joint-lead data (BV48) | 0/1 | 0 |
| `B048` | Per-edge cut-off distances R000–R007 (needs B035 and B046) (BV39) | 0/1 | 0 |
| `B049` | Joints always straight (BV46) | 0/1 | 0 |
| `B050` | On/off points cut as a tiny cut when no circle is made (BV55) | 0/1 | 0 |
| `C001`–`C003` | Material user info 1–3 (no effect, BV45) | text | empty |
| `C004` | Subprogram name prefix (BV26) | text | empty (names `1`, `2`, …) |
| `C005` | Guid (no effect, BV45) | text | empty |
| `C006` | Job file parameter (no effect, BV45) | text | empty |
| `C007`–`C009` | Job user info 1–3 (no effect, BV45) | text | empty |
| `C010`–`C012` | Part user info 1–3 (no effect, BV45) | text | empty |
| `C013` | Part order info (no effect, BV45) | text | empty |
| `R000`–`R003` | Cut-off distance inside the sheet at the bottom, left, top, right edge (with B048) | length, not rounded | 0 |
| `R004`–`R007` | Cut-off distance outside the sheet at the bottom, left, top, right edge (with B048) | length, not rounded | 0 |

## Database tables (Expert's SQL Server database)

| # | Table | What it holds (columns used by Expert) |
|---|-------|----------------------------------------|
| DT1 | `DIS_MMNN_MMOO_00000200` | Jobs: JobRef, JobName, CDate |
| DT2 | `MMNN_MMOO_00000300` | Parts ordered per job (manufacturing orders): DIS_JobRef, PrdRefDst, IOrder, WrkRef, Quantity, IsOutsourcing ('0' = made in-house) |
| DT3 | `DIS_NEST_NEST_00000100` | Nestings: JobRef, NstRef, WrkRef (machine), MatRef, SThickness, CNC, Quantity, NOrder, CopyIndx |
| DT4 | `DIS_NEST_NEST_00000500` | Parts on each nesting: NstRef, PrdRefDst |
| DT5 | `PPRR_PPRR_00000100` | Products / parts: PrdRef, DIS_PCLASS, DIS_ShtRef, StdCost |
| DT6 | `MMNN_OOPP_00000100` | Work centres (machines): WrkRef |
| DT7 | `DIS_MMTT_MMTT_00000100` | Materials: MatRef |

Expert signs in with LANTEKIN.CNI section R (R003 user, R004 password). When R003 names a SQL login
and R004 is empty, the password isn't in the file, and the rebuild does not try to recover it: the
user types the SQL password or uses Windows sign-in.

How the rebuild uses them (src/dbquery.rs, src/dbbrowse.rs — Jobs ribbon → Database → **Browse
database**, or Database connection → "Open the database (read-only)"):
- Signs in through Windows' .NET SqlClient (Windows PowerShell, script passed as -EncodedCommand; server,
  database, user and the typed password travel in the helper's environment, never on a command line or
  on disk; nothing is saved).
- **Jobs** page: newest 200 jobs (DT1) with counts of in-house parts (DT2, IsOutsourcing = '0') and
  nestings (DT3); for a job: parts ordered (DT2 by DIS_JobRef, by IOrder), nestings with their CNC
  programs (DT3 by JobRef, by NOrder), and the parts on those nestings (DT4 joined on NstRef).
- **All tables** page: every table with its row count (sys.tables + sys.partitions), Expert's own tables
  first with what they hold; the first 200 rows of any table.
- Read-only: only these SELECTs are sent; table names must be letters/digits/underscore and go in
  brackets; job values are quoted N'…' with quotes doubled. Login failures and unreachable servers get a
  plain explanation.


### Job Explorer folders

| # | Rule |
|---|------|
| DT8 | Folders are rows of `DIS_MMNN_BWSR_00000100`: **NodeID**, **NodeName**, **ParentID**, **BwsrType** (which tree — jobs and products share the table). A folder is found by NodeName + ParentID; its parent by NodeID + BwsrType. |
| DT9 | A job's folder: `DIS_MMNN_BWSR_00000200` row with **TblRef** = 'DIS_MMNN_MMOO_00000200', **RecordID** = the job's RecID, **BwsrID** = the folder's NodeID (products the same with TblRef 'PPRR_PPRR_00000100'). |
| DT10 | A nesting's CNC program is stored as a document: `SYST_OWND_00000100` with RecordID = the nesting's RecID, TblRef 'DIS_NEST_NEST_00000100', **DocDsc 'CNC'**. |

The rebuild's **Jobs** page shows the same tree as Expert's Job Explorer: a root
"Jobs '<database>'", folders sorted by name ignoring case, sub-folders, then each folder's jobs (with
part and nesting counts); jobs in no folder sit under the root. Only folders of the BwsrType that holds
jobs are kept. Typing in "Find a job" lists matching jobs flat. Up to 5000 jobs are read, newest first.


## Units and sheet thickness (rebuild)

The app keeps every length in millimetres inside and shows it in the unit picked in
Settings → Nesting → Units (`units.rs`).

| # | Rule | Source |
|---|---|---|
| UN1 | **Every length in the window follows the unit**: sheet size, thickness, part sizes, gap and margin, technology lengths, the cursor X / Y and the unit shown beside it, the hover card, Free X, the jobs and stock lists, remnants and their areas (m² or ft²), the part view's dimensions and measurements, and status messages. Inches are typed as decimals, fractions (`1 1/2`) or steel gauges (`10ga`); either unit can be typed with its suffix (`6mm`, `1/4in`, `1/4"`). Lengths in inches show two more decimals than in mm (at most 4). Agents' operations stay in mm, machine programs in the unit set for the machine, and sheet references (`ST3-2000x1000`) in mm. | rebuild |
| TH1 | **Any thickness can be typed** (0.1–300 mm), in either unit, beside a list of common ones. When the machine's files give no speed, the built-in fallback speed for a thickness between two of its rows is read in a straight line between them (outside them, the nearest row). | rebuild; fallback speeds: estimate |
| TH2 | Thicknesses offered: in mm 0.5–25 mm; in inches the steel gauges 20, 18, 16, 14, 12, 11 and 10 (0.0359, 0.0478, 0.0598, 0.0747, 0.1046, 0.1196, 0.1345 in) and 3/16, 1/4, 5/16, 3/8, 1/2, 5/8, 3/4 and 1 in. A thickness that is a gauge is shown with it ("0.1345 in (10 ga)"). | estimate (manufacturers' standard gauge for steel; other materials' gauges differ) |
| TH3 | Sheet sizes offered on the Sheet tab: in mm 2000 × 1000, 2500 × 1250, 3000 × 1500, 4000 × 2000, 6000 × 2000; in inches 96 × 48, 120 × 48, 120 × 60, 144 × 60, 144 × 72, 240 × 96. Any size can be typed. The sheet grid is 100 mm, or 4 in. | estimate |

## Settings (the rebuild's own — not tied to one install)

Saved in `%APPDATA%\nestpro\settings.ini` (src/settings.rs), edited in **Settings** (Options → Settings…,
Jobs → Database → Connections, Nesting → Machining → Machine & output):
- **Connections**: any number of SQL Server databases — server (`host`, `host\instance`, `host,port`),
  database, Windows account or SQL login with any user name; a password can be remembered encrypted for
  the Windows user (DPAPI via ConvertFrom-SecureString, decrypted only inside the query helper). The
  active one opens in Browse database, which loads the Job Explorer tree right away when it can sign in.
- **Machines & output**: any number of machines, each with what it does (laser, plasma, oxyfuel,
  waterjet, laser + punch, punch; MK1) and ISO G-code (with any .CFF/.CF1 post library or built-in
  values) or BLM / Siemens 840D (with any .PS1/.CF1 post files and gas); the one in use is the job's
  machine and is used for Export CNC.
- **Materials**: the name and code each material gets in programs. **Folders**: export folder; an
  optional Lantek Expert folder.
- Lantek Expert is only an import source ("Import from Lantek…": LANTEKIN.CNI connection, DBConnect.lst
  machines, LANTEKPR.CFF, MchFiles\PstAdi06.*); on first start an install at C:\Lantek\Expert is
  imported once. Nothing reads `C:\Lantek\Expert` after that.


### Connections

| # | Rule |
|---|------|
| CN1 | Connection string: **no user** → `Provider=sqloledb;Data Source=<server>;Initial Catalog=<db>;Persist Security Info=False;Integrated Security=SSPI`; **a user** → `DRIVER=SQL SERVER;SERVER=<server>;DATABASE=<db>;User ID=<user>;Password=<password>` (the choice is "user text length > 0"). |
| CN2 | The built string has doubled single quotes turned back into single ones; a cloud connection uses its own text after `DRIVER=SQL SERVER;` instead. |
| CN3 | Opening: an ADO `Connection` and `Recordset`; **command timeout 600 s**; the connection timeout is not set, so ADO's default 15 s applies; user and password are passed separately to the open call too. |
| CN4 | DBConnect.lst is read from the current folder: an **`A` line names a database as "server,database"** (compared ignoring case), the **`B` lines after it are that database's machines**, a **`C` line ends** them. |
| CN5 | "Encrypted" passwords (XML FLAGS/PASSWORDS attribute "Encrypted", with a salt kept in the database) are Lantek's own application-user passwords, not the SQL Server login. |

The rebuild: Settings → Connections shows the connection string exactly as CN1/CN2 build it (password
masked); the query helper waits 15 s to connect and lets a query run 600 s (CN3); importing from Lantek
takes only the machines DBConnect.lst lists for that server and database (CN4), and a file without `A`
lines still lists every machine.

### Settings area and the Job Explorer's database jobs (rebuild)

- The **Options** menu is short: Settings (Ctrl + ,), Browse database, a Light / Dark / System theme
  switch, and links to General, Machines & output, Keyboard shortcuts and About.
- **Settings** is the one place for everything, grouped APP (General, Appearance, Shortcuts),
  PRODUCTION (Connections, Machines & output, Materials, Folders) and INFO (About). Theme, the
  part-technology warning, showing unbuilt commands and "load database jobs at start-up" are saved in
  settings.ini with the rest.
- **Job Explorer**: under the local jobs it lists the active connection's jobs in Expert's folder tree
  (DT8–DT9). At start-up they load in the background when the connection can sign in by itself (Windows
  account or remembered password; a SQL login without a password is never tried automatically, so the
  account can't be locked by retries); otherwise a Connect row is shown. Clicking a database job opens it
  in the Database window.


### Look and shortcuts

| # | Rule |
|---|------|
| LK1 | Choosing a ribbon style does nothing if it's the current one; otherwise the style is applied, then saved at once to LANTEKIN.CNI. |
| LK2 | Applying a style loads that style's theme from `<install>\Styles`, its INI (OFFICE2013WORD.INI, OFFICE2013WORDGRAYDARK.INI, OFFICE2013WORDGRAYLIGHT.INI, OFFICE2016WORDCOLORFUL.INI, OFFICE2016WORDWHITE.INI) and the back image from `Styles\BackImage\`, then redraws. |
| LK3 | LANTEKIN.CNI **A212** ribbon style: 0 Office 2013 Blue, 1 Office 2013 Gray Dark, 2 Office 2013 Gray Light, 3 Office 2016 Blue, 4 Office 2016 White. Written by rewriting the file through a temporary copy and a rename. |
| LK4 | **A213** ribbon font: 0 system default, 1 normal, 2 large, 3 extra large; a value above 3 reads as 0. |
| LK5 | Keyboard shortcuts are user-defined and kept in OPTIONS.CFG as `K%05d  <key>  <flags>  ;` lines (75 slots; `E%05d` and 69 slots for module 10). v44 adds a list of them with Copy (new features 1.2). |
| LK6 | LANTEKIN.CNI **A206** ON: the Job Explorer is shown (A207–A211 the other Jobs-screen panes, A216 the kit explorer). |

The rebuild: Settings → Appearance has **Interface size** with A213's four sizes (system default and normal
draw at 100 %, large 115 %, extra large 130 % — the zoom amounts are ours); "Import from Lantek…" now also
takes the look from A212 (dark only for Office 2013 Gray Dark) and the size from A213 with LK4's rule;
the theme is saved the moment it changes (LK1); Settings → Shortcuts has **Copy list**, keeping the
list's columns (LK5).

### Opening a nesting from its machine programs (rebuild)

Jobs → Nesting → **Open program** (src/progimport.rs) reads an LC5 program set — `<NEST>.zip` (+ its
`.XML`), the `.XML` (+ its `.zip`), or a sheet `.SPF` with the other `.SPF` files beside it — whether
Lantek's LC5 post wrote it or this app did, and opens it as a new, editable job:
- the sheet program (the file with `INITNEST`) gives the sheet from its `;MAT,length,width,thickness`
  header and one placed part per `ORIG(x,y,angle)` + `CALL"…_n_<SUB>_spf"` (BL4);
- each part subprogram gives the part: `NEXT_L` starts a path, moves before `PROFILE` are the lead-in,
  `G01` lines and `G02/G03 … I J` arcs (rebuilt every 5°, I/J from the move's start, BL10) follow,
  `LASTLINE(…,x1,y1,x2,y2,…)` ends it (BL5). A path closes where it comes back to — its start, or the end
  of Lantek's lead-in arc. The largest closed path is the outline, the others holes; quality-9 paths are
  marking. Parts are named from the XML's `SubPrg` CNCSub → PartRef;
- a subprogram with only an open path (FINAL_CUT, BL8) becomes the remnant line, cut to the sheet edges.
Tested on the example program set: 14 parts of 152.4 × 76.2 on a 1200.15 × 1219.2 sheet,
remnant line at X 172.4. Lantek's nestings inside its database are kept in Lantek's own format and
aren't read; their LC5 programs are.

The database lists (Job Explorer and Database window) search folder names as well as jobs, open found
folders, and draw at most 300 search results and 500 jobs per open folder; Settings → Appearance →
Scrolling sets the mouse-wheel speed (default 2 × 40 px per wheel line).


### Lantek's nesting export (.nxp) and placements

| # | Rule |
|---|------|
| NX1 | Expert imports nestings only from a **`.nxp`** file ("ManufacturingJob"); any other extension is refused. |
| NX2 | Exporting writes a text index EXPORTNEST.TMP — `G001 "<export name>" ;`, `G002 <n> ;` — then blocks for remnants, products and nestings, and moves each nesting's, product's and remnant's own files to the export folder. |
| NX3 | A nesting block: `N001 "<nesting>"`, `N002`–`N004` texts, `N005`–`N012` numbers (`%1.3f`), `N013`–`N018` integers, `N019`–`N025` texts; then per part `N100 "<part>"`, `N101` count, `N102`, `N103`, `N104`, `N199`; per remnant `N200`…`N207` (`N207` 1 or 2), `N299`; `N300 "<…>"`, `N399`; and `N999` ends it. |
| NX4 | The .nxp is that same text, copied to EXPORTNEST.NXP (not zipped); the nesting geometry is in the separate files next to it. |
| NX5 | Importing checks the machines, confirms the data, then writes the job in one transaction (rolled back on error). |
| NX6 | A part's place in the LC5 sheet program is `ORIG(x,y,angle)` (angle as a whole number) with PS1 Z005 = 1, or `TRANS X.. Y..` + `AROT Z..` with Z005 = 0. |

The rebuild: Open program also takes a **.nxp**. It reads the N001 nesting names (NX3) and opens the first
nesting whose LC5 programs (`<name>.zip` / `.XML` / `.SPF`) are next to it, listing the others; geometry
comes from those programs, since the .nxp itself has none (NX4). The program reader accepts both `ORIG`
and `TRANS` + `AROT` placements (NX6), turning the part by the angle.

| # | Rule (rebuild: reading LC5 programs back) | Source |
|---|------|--------|
| NX7 | `LASTLINE`'s 4th value is the move: **1** straight, **2 / 3** an arc as G02 / G03 with its centre at the start + (9th, 10th) values. The original's posts end round holes and slots on such an arc; it's read back as an arc. | Rebuild (from the post's output) |
| NX8 | A contour **without a lead-in** (BL24) starts at the pierce point: its first piece, before `PROFILE`, carries G41/G42. Read back, a path that returns to the pierce point is closed there. | Rebuild |
| NX9 | Opening one sheet's `.SPF` from a folder that holds several sheets' programs opens that sheet. | Rebuild |
| IU1 | **DXF units**: a drawing's `$INSUNITS` header (1 inch, 2 foot, 4 mm, 5 cm, 6 m) is applied when it's read, in every import. Arcs are cut into straight pieces within 0.1 mm of the real curve in mm (not 0.1 of the drawing's unit). A DXF or DWG that doesn't say is read in the app's unit (Settings → Nesting: mm or inch), as the original takes a drawing's numbers in its system units (DXFASYSW / DWGASYSW `A089 0`, observed); drawings from the ERP integration use its own drawing unit. A DWG's own unit isn't read yet, so a DWG is always in the app's unit. | Rebuild |
| PP1 | **Looking never changes**: the number boxes in part and sheet properties keep their limits for typing and dragging only; a value outside them (a quantity of 1000, an 8 mm disc, a sheet over 3 m) is shown as it is. | Rebuild |


## How Lantek stores nestings (and how this app saves jobs)

| # | Rule |
|---|------|
| RD1 | Expert's files for a record live in the **database folder** (LANTEKIN.CNI R005 / share R008) under a folder per kind, **1000 records per sub-folder**: `Products\<n/1000>\<n>` (kinds 1, 3), `Nestings\<n/1000>\<n>` (2, 4), `Machinings\…` (5), `NestQuot\…` (6), and `General\Mlt<n>`, `Turr<n>`, `Punch<n>`, `Hole<n>`, `MchM<n>`, `Para<n>` (11–15, 17). |
| RD2 | A nesting is these files with its number as the name: **`.chp`** (sheet and placed parts), **`.mec`** (machining), **`.ret`** (remnant), `.bmp` and `.wmf` pictures, `.pdf`, `.mcs`, and `<n>Box1.wmf`, `Box2.wmf`. Exporting copies them to `<export>\Nestings\`. |
| RD3 | A part's machining file names come from the database when the part has machinings. |

`.chp` and `.mec` are Lantek's own binary formats (kept on the database share), so
they aren't read; a nesting is opened from its LC5 programs or a .nxp export instead (NX1–NX6).

**This app's jobs** (src/jobfile.rs): Jobs → **Save job** (Ctrl + S; Ctrl + Shift + S asks where) writes one
readable JSON file `<job>.snjob` — products with their technology, texts and imported drawings, sheet
settings, every nested sheet and the cut lines — written to a temporary file first, then renamed (as
Expert rewrites LANTEKIN.CNI, LK3). **Open job** (Ctrl + O) opens it as a job of its own (named with
"(1)" if one with that name is open). Files say `"format": "nestpro-job"` and their layout version; a
newer layout is refused.


### Lantek's own file format (.mec / .chp)

| # | Rule |
|---|------|
| MF1 | A Lantek geometry / machining file starts with **`0 FILEMECLT <major>.<minor>`** (written as "0 FILEMECLT %d.%d"); a file without that word counts as version 7. |
| MF2 | **Encrypted when the major version is above 7.** The rest of the file is then lines of hex, encrypted with Lantek's own keys. |
| MF3 | The sample files shipped with the original are encrypted (sample part machinings are 8.1, tool files 8.00). An older 32-bit component still writes 7.00 (plain). |

Import drawings reads Lantek part files (.mec), plain or encrypted, with keys found in the shop's own
Lantek install (MF4–MF15, "Importing part drawings"). Open program still doesn't open them as a
nesting: it recognises a Lantek file by MF1 (by extension .mec / .chp / .ret, or by its first line
under any name) and says to import its parts with Import drawings, or to open the nesting's LC5
programs.

## The app's own library (product and job database)

Lantek keeps its lists (jobs, products, nestings, folders) in SQL Server tables (DT1–DT10) and each
record's drawings as encrypted files in a shared database folder, `Products\<n/1000>\<n>`,
`Nestings\<n/1000>\<n>` (RD1–RD2, MF2). The rebuild has its own library for shops without Lantek
(src/library.rs, src/libui.rs), laid out the same way but open:
- a folder (Settings → Folders → Library folder; local, or on a shared drive for the whole shop) with
  `Products\<n/1000>\<n>.json` and `Jobs\<n/1000>\<n>.snjob` — **1000 records per sub-folder**, as RD1 —
  and `index.json` listing every record (name, folder, material, thickness, size, counts, last saved and by whom, LB5; each record carries its revision, LB7) for
  quick lists and search; **Rescan** rebuilds the index from the files;
- a new record takes the next number by creating its file exclusively, so two people saving at once on
  a shared folder can't get the same number (Lantek takes it under a database lock, RF1), and every
  change to the Library's files is made under its write lock (LB1–LB3, "Sharing the Library between PCs");
- Jobs → Library → **Library** lists jobs and products (search by name or folder; Open; Add to job;
  delete); **Save to library** saves the open job (over its own record when it came from the library);
  "Add open job's products" stores products, skipping ones already there with the same name, material
  and thickness; the Job Explorer shows "📚 Library" with its jobs by folder.
The Lantek database stays a read-only source next to it (Browse database).


### Record numbers and automatic references

| # | Rule |
|---|------|
| RF1 | A new record's number = the highest RecID + 1, read under an update lock after taking an insert lock; an empty table gives **1**. |
| RF2 | Automatic **job** references (`Jobs_oct`, for DIS_MMNN_MMOO_00000200.JobRef): part 1 fixed text **"JOB"**; part 2 a counter **"000001"…"999999"**, 6 characters (FChars 6), stored in SYST_AARR_00000100/200/300 in one transaction. |
| RF3 | Automatic **product** references (`<kind>_oct`, PPRR_PPRR_00000100.PrdRef): the same counter after a prefix per kind — CommonItem **CMI**, ComplexItem **KIT**, SheetPart **SHP**, DuctFigure **DFG**, RawMaterial **RWM**, Sheet **SHT**, Abrasive **ABR**, Isolant **ISL**, DuctFrame **DFR**, Profile **BEAM**, ProfilePart **BPRT**. |

The rebuild's library numbers records by RF1 (highest + 1, first = 1; the number is taken by creating
its file exclusively, the folder's equivalent of the lock) and shows each record's automatic reference
by RF2/RF3: **JOB000001…** for jobs and **SHP000001…** for products (sheet parts); past 999999 it
refuses a new record, as the counter's end value.

### Sharing the Library between PCs (the rebuild's own design, ADR 0003)

Several programming PCs can point their Library at one folder on the shop's share. The original keeps
its shared lists in SQL Server, which keeps writers apart; the rebuild's Library is a folder of files,
so it brings its own write lock (src/shared.rs), built from what a Windows share keeps between PCs: creating a
file only if it isn't there, and renaming one over another. There is no original rule to follow here beyond RF1.

| # | Rule |
|---|---|
| LB1 | Every change to a shared file in the Library (a job or library part record, the list of records `index.json`, the sheet stock `Sheets\stock.json`) is made under the Library's **write lock**: a file `library.lock` in the Library folder, made with create-new (only one writer can make it, on a local disk and on a Windows share alike), naming the user, PC, process and time. A writer that finds it there waits, trying again every 2 to 40 ms, for at most **120 s**; then it gives up, stores nothing and says who is writing ("the Library is busy: sam on PROG-2…"). The lock covers the whole read, change and write (a save reads the list and writes the record and the list; a stock change re-reads the stock, changes it and writes it), so one PC's change never undoes another's. The window, the command line and the MCP server (the agents' `stock` and `library`) all take it; the window's save waits in place, normally for a moment (JOB-55). Reading never waits, and Rescan reads the record files before it takes the lock, then under it reads again only those written since, so the lock stays short. A change made inside another on the same thread uses the lock it already holds. Own design. |
| LB2 | **A lock left by a PC that crashed** or lost the share is taken over once a waiting PC has seen it unchanged for **90 s**, timed by that PC's own clock (never by comparing clocks between PCs). Only one PC can take over a given abandoned lock, however their steps interleave: nobody but its holder ever deletes `library.lock`; a PC taking over first makes a mark named after that lock (`library.lock.<its token>.taken`) with create-new, then checks the lock is still the one it judged abandoned and renames its own lock over it, and clears the marks. A PC that stops halfway through a takeover leaves its mark, which is taken over the same way once it has stayed unchanged for 90 s. **A writer replaces a file only within 30 s of taking the lock** (a third of the 90 s, by its own clocks), and only after checking just before each rename that the lock is still its own and no takeover of it has begun; after 30 s it replaces nothing ("Nothing was saved — save again") and leaves its lock to be taken over. So what a writer replaces reaches the share before any other PC can take its lock over, unless the share holds that request back for more than the 60 s between, longer than Windows waits on a share request: files on a share can't rule that case out (estimated; the two-PC check, LB4, is how a shop tests its share, and ADR 0003's fallback is the answer if it fails). Own design. |
| LB3 | Every write goes into a temporary file of the writer's own next to the file it replaces, named after its PC, process and a count (`index.json.PROG-1-4312-7.tmp`), and is then renamed over it (tried again for up to a second when Windows says the file is busy, with the lock checked again before each try). A reader sees the old file or the new one, never half of one, and two PCs never share a temporary file. A save to the Library stores the record **and** its line in the list, or nothing. When the record can't be written, nothing was replaced, a new record's number is given back and the save reports an error. When the record was written but its line can't be, the save undoes only its own write, and only while the lock is still its own: the record it saved over is put back, or the new record's number given back, and the error says nothing was saved, so saving again never makes a duplicate. When it can't undo (the lock was taken over, or the share went), its record is stored and the save counts as done, its line following with its next save or Rescan: a save reports success only when its record was written. Record numbers are still taken by creating the record's file exclusively (RF1). Own design. |
| LB4 | **The two-PC Library check** (`nestpro library-check <folder> --minutes 10 --text`; agents: `library-check`). Started on two or more PCs at once against a test folder on the share, each run saves made-up DEMO-CHECK jobs and library parts (new ones, and over its own earlier ones) and changes a stock sheet of its own, round after round, tagged with its PC's name, and logs what it saved in `library-check\<run>.json`. Then it checks that every record any run logged is there, whole, at its latest save and in the list; that each finished run's stock sheet holds exactly what its changes left; and that every record in the folder can be read (one that can't is read again under the write lock, as a save may be under way). It reports PASS, or FAIL with one plain line per problem (exit code 1). It works only in an empty or missing folder (it leaves `library-check.json` there), or in one it made itself that holds nothing else: its list and stock must be readable and hold only DEMO-CHECK records and sheets, every record file, listed or not, must be one of its own (a damaged one only if a run logged it; an empty one is a save cut off), and any other file or folder is refused. It refuses this PC's Library and any folder that may be a real Library, and touches nothing in it. Default length 1 minute. Own design (ADR 0003: the test before more is built on the shared folder). |
| LB5 | **Who saved it, and when.** Every save of a Library job or library part records who saved it and when: in the record (`saved`: the user, the PC, the time) and in its line of the Library's list (`saved_by`, with the line's last-saved time). Who is the Windows user and PC name (`USERNAME`, `COMPUTERNAME`; on Linux the user and host name), and a save made by an agent (the command line or the MCP server) is that PC's user marked as an agent. The time is the saving PC's clock when the save is written, once it holds the write lock (a save that waited for another PC's lock is not signed with when it began waiting). Saving over a record replaces both, so a record shows who saved it last; Rescan takes them from the records. A record saved before this rule has neither: it loads and lists as before, with no name, keeping its last-saved time (after Rescan, when its file was written). The Library check (LB4) signs its saves with the run's user and the PC name it's tagged with. The program is handed who is working and a clock rather than reading them in its rules, so one test can play two PCs and move time on. Own design; estimated. |
| LB6 | **How it's shown.** "Saved by sam on PROG-2, 9 Nov 09:20": on each row of the Library window (jobs and library parts), and in the hover text of the Job Explorer's saved jobs and of Add product's library parts. An agent's save reads "sam on PROG-2 (agent)". The time is this PC's local time, with the year when it isn't this year ("9 Nov 2025 09:20"). A record saved before LB5 reads "Saved 9 Nov 09:20", with no name. Agents: `library` mode=list gives each record `saved_by` (the same words, or null) and `saved_at` ("2026-11-09 09:20", this PC's local time). Own design; estimated. |
| LB7 | **A save never goes over someone else's newer save.** Every Library job and library part carries a **revision**, which every save of it moves on by one (in the record: a job's `library` = its record number and revision; a library part's `revision`; 0 for a record saved before this rule). An open job remembers the revision of its record as it was opened or last saved, and the record and revision of each library part in it as it was added or last saved to the Library (a job file keeps both, `library` and `library_parts`). A library part added to a job that already has a part of that name leaves what that part read as it was, so saving it still asks when someone saved the library part since. A save over a record reads the record as it is now **under the write lock** (LB1) and compares: when the revision differs, nothing is saved and the save is refused, naming who saved it and when (LB5, LB6: "DEMO-7 was saved by sam on PROG-1, 9 Nov 09:30 since you opened it"). A part that wasn't added from the Library (or was renamed since), saved under the name a library part has, counts as changed too: it would replace a part this job never read. A record that isn't there any more is saved again under its number: a deleted record's number is never given to a new record (the delete leaves an empty mark, `7.snjob.deleted`, beside its file's place, and a new record skips a number so marked), so that save never goes over another job. The window looks once before it does anything else, so a refused save never asks for remnant references first. Own design; estimated. |
| LB8 | **A refused job save takes nothing from stock.** The window books the stock when a job is saved (SV7, SS10, SS11): its sheets are taken, the ones it no longer uses given back, and its remnants stored. The revision check (LB7) comes first, then the name check (JX16), and the booking, the job's record and, for a new record, its place in the job folders (JX15) follow under the same hold of the write lock, so no other PC can save the job, or a job under its name, between the checks and the booking. A save refused because someone else saved the job, or because another job holds its name, takes no sheets, gives none back and stores no remnants. A Library job saved to a file (Save job) leaves the stock alone: its sheets are booked when it's saved to the Library, after the check, and the file is a copy holding none of its sheets or remnants (as LB9's copy), which books its own when saved in the window. Own design; estimated. |
| LB9 | **What the window asks.** For a job: **Reload theirs** (the open job becomes the job as saved now; this window's changes are dropped; the stock isn't touched, the job holds what their save holds), **Save mine as a copy** (saved as a new job under the first free name, "DEMO-7 (1)", "DEMO-7 (2)"…, that no job in the Library or open here has, whatever its capitals, and filed in the folder of the job it was copied from (JX15); like Copy job, the copy holds none of the original's sheets or remnants and takes its own when saved, and its work order link is dropped; theirs stays as they saved it), **Overwrite** (this job is saved over theirs as saved now: their sheets go back to stock and their unused remnants come out of it, then this job's sheets are taken and its remnants stored; if someone saves again before it's done, the window asks again), or Cancel (nothing is saved; the next save asks again). For a library part: **Keep theirs** (nothing is saved and the job's part stays as it is; the next save asks again), **Overwrite** (the job's part replaces theirs, asked again if someone saves in between), **Save mine under a new name** (the job's part takes the first free name, "DEMO-P1 (1)"…, and is saved as a new library part; theirs stays). Own design; estimated. |
| LB10 | **Agents.** `library` mode=open (with the job's `id` or `name`) makes the job file that library job, linked to its record and revision; it leaves the stock alone (`booked` off). `stock-book` mode=book and mode=release are refused on a job file that is a library job (opened from the Library, or saved into it): its sheets are booked when someone saves it in the window (SV7, LB8), and booked by the agent as well, a save refused because someone saved the job since would leave them taken twice. The refusal says so and names the job; nothing in the stock changes, and mode=status still says what booking would take. A job file booked with `stock-book` before mode=save-job puts it into the Library hands its booking to the record: the record keeps the sheets and remnants the file holds, and the file stops following the stock (`booked` off), as one opened from the Library does; its later changes are booked when someone saves the job in the window, and save-job says so. mode=save-job saves over the job's own record (one opened from the Library, or saved into it before), so saving twice leaves one record; a job never in the Library becomes a new record. When someone else saved that record since the job file read it, save-job is refused, saying who and when ("kim on PROG-1 at 2026-11-09 09:30") and what to do instead: open it again (the file's changes dropped) or `copy=true` (a new job under the first free name, holding none of the original's stock, as LB9). Agents never overwrite. mode=add links the part to its library record; mode=save-part checks every part as LB7 before saving any and is refused the same way, naming the part, who and when (take the job's parts of that reference out and add theirs, or give the part a new reference and save it again); two parts of one reference picked together are refused before anything is saved. Own design; estimated. |
| LB11 | **The Library can't be reached.** When the Library folder isn't there (the share, the server or the network is down, or the folder was moved or renamed), everything that uses the Library says so and names the folder: "The Library on \\srv\Nesting can't be reached. Check the network or the server." The Library window, the Job Explorer's saved jobs, Add product's library parts, the Open job menu, Settings → Folders and the Sheet stock show this message, with Try again and the Library folder setting, instead of an empty list, and look again only when asked (Try again, or another folder): a share that's down can take seconds to answer. Save to library, saving parts to the library and stock changes are refused with it, and nothing is made in the folder's place: no part of the program makes the Library folder, its sub-folders or its write lock in a folder that isn't there. Do All nests on the job's sheet as when the stock has nothing for the job, and says the stock wasn't used. Agents' `library`, `stock` (all but `rules`) and `stock-book` are refused the same way, saying what was left as it was and to try again once it can be reached; the XML import says Library parts weren't looked up. The two-PC check (LB4) makes its test folder only inside a folder that is there, and otherwise says the test folder can't be reached. This PC's own default folder (`Documents\Sheet Nesting\Library`) when it was never made says "There's no Library on this PC yet" and where to make one. Own design (estimated). |
| LB12 | **A new Library is made only in Settings, after asking.** A folder holds a Library when it has the Library's list (`index.json`) or its `Jobs`, `Products` or `Sheets` folder. A folder that is there but holds none is refused like one that can't be reached ("… holds no Library. Pick the Library's folder in Settings → Folders, or make a new Library there."), and nothing is written in it. Picking a folder in Settings → Folders (typed, Browse…, or ✖ for the default) that holds a Library uses it at once; any other folder (new and empty, holding other files, or not there yet) asks "Make a new Library?", naming it, and the Library stays where it was until the answer is yes: then the folder is made if needed and a new, empty Library set up in it. Cancel makes nothing. A folder typed in the box counts once the typing is done, never while half a name is in it. While the Library can't be found, Settings → Folders shows why, with Try again and "Make a new Library here…", which asks the same question. Own design (estimated). |
| LB13 | **Nothing is lost or moved while the Library can't be reached.** Open jobs stay open, with their unsaved changes and their link to their Library record. A refused Save to library changes nothing in the job, which stays unsaved; Save job still saves it to a file on this PC, leaving the stock alone (LB8); the file is a copy: the window doesn't follow it, so a change an agent makes to it never reaches the Library job, and Save asks where each time; its sheets are booked at its next Save to library. The program never switches to another Library, or to the default one, on its own: only the user changes the Library folder, in Settings (LB12). An agent's refused operation leaves its job file as it was. Own design (estimated). |
| LB14 | **Lists update by themselves.** With the auto-refresh setting on (JX8, JX9: Settings → Folders, "Update the lists and open jobs by themselves", off on a new install, the period in whole minutes 1 to 1440, kept on each PC; importing a Lantek install takes its period), the window looks **every 5 seconds**, and at once when it comes back to the front, whether the Library changed: it looks at the date and size of the Library's list of records, its job and part folder trees and its sheet stock, without reading them and without the write lock. What changed is read again: the Job Explorer's saved jobs, the Library window, Add product's library parts (only the parts whose line in the list changed), each with its folder tree, so a job or part folder made, renamed, moved or deleted, or a job or part moved into another folder, shows too (JX10, JX18), and the Sheet stock. Once per period the **full reload** reads everything again: every library part and every open Library job's record, which also catches a change the dates don't show. A look takes the dates first and reads after, so a save landing during a look shows at the next one; a file being replaced as it's read keeps what was shown and is read again at the next look. A list not shown yet is read when it's shown. Refreshing only reads: it never writes to the Library, takes its write lock or makes it (LB1, LB12). With the option off nothing reloads by itself; Refresh, and this PC's own changes, read the lists again as before. Agents' `library` and `stock` read the Library as it is now at every call. A Windows share can itself hold back another PC's change for a few seconds. Own design (spec #44); estimated. |
| LB15 | **Open jobs someone else saved.** At every look that finds the Library changed, and at every full reload, each Library job in this PC's open jobs is compared with its record (a quick look reads the record only when its line in the list changed). One someone else saved since it was opened or last saved here: with **no unsaved changes**, it reloads by itself as saved now, holding what their save holds with the stock untouched (as Reload theirs, LB9), its remembered revision moved on; the job in front stays in front, on the same sheet. It shows a note, "↻ DEMO-7 was reloaded — Sam on PROG-2 saved this job at 09:20" ("on 8 Nov 09:20" for another day), until it's closed or the job is saved here, and the status line says the same. With **unsaved changes**, the window asks, naming who saved it and when: **Reload theirs** (as LB9), **Keep mine** (the changes stay and nothing is saved; it isn't asked again about that save, but is about a newer one; the job's remembered revision stays as it was, so its next save still asks, LB7–LB9), **Save mine as a copy** (as LB9: the first free name, filed next to theirs, taking its own sheets; theirs stays as they saved it). The answer brings that job to the front. One question at a time: while another waits, it's asked at a later look; a newer save before the answer is the one the question names. Nothing is reloaded while a save waits for its remnants' names or another answer about the job. Own design (spec #44); estimated. |
| LB16 | **A job deleted or moved, or a Library that goes** (JOB-62). A Library job open here that another PC deleted stays open, with its changes and its link to its record; the status line says so once ("DEMO-7 was deleted from the Library on another PC. It stays open here with its changes; Save to library saves it again"), and Save to library saves it again under its number (LB7). A job another PC moved to another folder shows in its new folder; the open job isn't reloaded or asked about, as moving doesn't change the job, and saving it leaves it where it was moved (JX10). When a look finds the Library can't be reached, the lists say so in LB11's words, once, not at every look; open jobs stay open with their changes (LB13) and nothing is made in its place. Until it's back, the look is made in the background, so a share that's down never holds the window up; once it's back the lists are read again in full by themselves. Another Library folder (Settings) starts afresh. Own design; estimated. |

### Preprogrammed parts from the library

Lantek adds stored products with **Parts From Database** / **Kits From Database** (Jobs → Home → Import):
products live in PPRR_PPRR_00000100 (DT5), each with its own machining files
(`Machinings\<n/1000>\<n>`, RD1; RD3) and routings per machine (MMNN_PPOO_00000100/200).

The rebuild: library products are whole parts — shape, imported drawing, **own technology** (leads,
micro-joints, quality, loops, start points) and texts — so a part programmed once is reused as is:
- **Add product** shows "From the library" first (search; up to 8 parts with a **T** badge when they
  carry their own technology; "All … library parts…"), then the blank shapes;
- Jobs → Products → **Parts from library** opens the Library on its products (with a "technology" chip);
- Part Properties → **📚 Save to library** stores the selected part, over the stored one with the same
  name, material and thickness (one product per reference, like a Lantek PrdRef).

Job Explorer: **Open jobs** (what is open now — example jobs can be turned off in Settings → General),
**📚 Library** (one quiet line until it has jobs) and the Lantek database (one line with Connect until it
is loaded). The title bar shows the library folder's name in brackets where the original shows its
database (rule 8), replacing the old fixed "Workshop".


## Library products: key, picker filter, machining choice

| # | Rule |
|---|------|
| PK1 | A product exists when its **reference alone** is found: `PrdRef` is the only field looked up in `PPRR_PPRR_00000100` (returns 3 when the table id is <= 0). Material (`DIS_MatRef`) and thickness (`DIS_Thickness`) are fields of that one product, not part of its key. → `Library::find_product(name)`, `save_part_to_library` updates the product with that name, `add_products_to_library` skips names already stored. |
| PK2 | A product record carries PrdRef, PrdName, StdCost, CDate, RecID, DIS_MatRef, DIS_Thickness, DIS_Length, DIS_Width, DIS_CutPerim, DIS_MrkPerim, DIS_RectArea, DIS_ExtArea, DIS_Area, DIS_Rotations, DIS_IsDraft and DIS_UData1..8_Prt; then one machine assigned to the product is looked up. |
| PK3 | The product picker builds its WHERE from optional filters: `AND (WrkRef = N'%s')`, `AND (DIS_MatRef = N'%s')` when a material is given, `AND (DIS_Thickness = '%f')` only when the thickness is **>= 0.0** (negative = any), then `PrdNameDst LIKE`, `OrdRef LIKE`, `CusName LIKE`, `CusRef LIKE`. → the Add product menu lists library parts of the job's material and exact thickness (to 6 decimals, '%f'); "Every material" passes no material and a negative thickness. `library::product_matches`, THICKNESS_FILTER_MIN, THICKNESS_FILTER_DECIMALS. |
| PK4 | "Has technology" = the product has at least one row in `DIS_MMNN_MACH_00000100` (one row per machining of the product). → `Entry.tech`. |
| PK5 | Of a product's machinings, the first whose machining file name is not empty is used; when none has one, the first in the list. |
| PK6 | A nesting's parts get their machining file by matching the part reference and the machine against the order's machinings, first match wins. |


## Micro-joint placement and cut technology by contour size

| # | Rule |
|---|------|
| MJ1 | Placing one joint on a part: when the picked element belongs to a lead or the contour has a lead-in, the leads on that element are lifted and their points listed; joints already on the element (or a plain-gap type 9–11) are lifted the same way. The joint is then **refused when it sits too near any listed lead point or joint**; afterwards the lifted leads/joints are put back. → `cnc::plan` filters joints in order along the contour. |
| MJ2 | "Too near" = `distance(p, q) <= width + size_of_q` (with a tolerance), checked against every lead point and every joint point. `width` is the joint record's gap; the other point's size comes from its own record. → `nest::joint_too_near`; the rebuild gives the lead-in point the lead-out length as its size and a joint its gap width (estimated). |
| MJ3 | Both ends of a gap must lie on the same element (segment type 1, arc 2; for a circle, type 4, the angle from the centre + 180° wrapped at 360°). → `cnc::fit_joint` (already used). |
| MJ4 | A joint record's gap of **99999.999** (within **1.0**) means "not set": it is replaced with the effective gap in all 10 automatic joint records. |
| MJ5 | A contour with joints is reordered to **start at its first joint's lead-in** (when that isn't already the first element). Built: see "Machine technology file" below. |
| MJ6 | "Joint at the end of the contour" is on only when the end distance is <= **99999.0** and the remaining length is below it. |
| MJ7 | The automatic joint record for a contour comes from the contour's quality; with no contour data the default record is used. |
| CT1 | **Cut technology by size**: the contour's area A (m²) gives `r = sqrt(|A × 1000 × 1000| / π)` (the radius of the circle with the same area, mm). A second value is divided by **1000** and taken off both limits. If the first row of the automatic list starts above r, nothing is assigned; otherwise every row with `min − off <= r < max − off` matches, the matching technology indexes are sorted and the first is used. → `nest::equivalent_radius`, `nest::quality_for_radius`, Cut Technology → **By size** (3 rows: from r, up to r, quality); the contour's feed follows its row's quality. |
| CT2 | Only closed contours with a cut type get it; a flag of −1 forces it. |

Constants used: 0.5, 1.0, 3.0, 5.0, 45.0, 90.0, 100.0, 180.0, 270.0, 360.0, 99999.999; π/180, π, 99999.0, 0.01, 0.1, 1000.0.

Still estimated: the rows' values (they come from the material/thickness table, which couldn't be read; the example row "up to r 5 mm → quality 1" is the rebuild's), the offset (0), the size given to a lead point, and how the X and Y joint counts are split between sides.

### The machine's numbered cut qualities (rebuild)

A cut quality is one of the machine's 12 numbered sets of cutting settings (its technology table
holds speed, pierce and the rest per quality, TT3). The app uses those numbers everywhere: a part's
own quality, the rows of Cut technology by size, a contour's own quality set by hand, the times,
leads, kerf table and the LC5 program.
Screens and agents show each quality by the machine's name and in its colour (CQ4, CQ5).

| # | Rule | Source |
|---|------|--------|
| CQ1 | **Each cut contour's quality**: the quality set on the contour by hand (CQ6); else the part's own size-range row that holds the contour (CT1); else the part's own quality, when it has one; else, for a part with no size ranges of its own, the machine section's size band for the contour (MTL10); else **quality 3**. Part-info lists each contour's quality and where it came from. | Order observed (CT1, MTL10); quality 3 as the last fallback is estimated: real LC5 programs cut outlines at 3, the fastest of the usual laser table's Slow 1 / Medium 2 / Fast 3 |
| CQ2 | **Jobs and products saved with the old three qualities** open with numbers: a part's own *Normal* becomes "no quality of its own" (Normal was what let the size bands decide), *Fine* becomes 1 and *Rough* 3; a size-range row's *Fine* is 1, *Normal* 2 and *Rough* 3. | Estimated (FEATURES MAC-54) |
| CQ3 | When the machine gives no speed at all, a contour is cut at our typical laser speed × 0.7 at quality 1, × 0.85 at quality 2 and × 1.0 at the others. | Estimated |
| CQ4 | **Quality names** come from the machine's technology file: A922 names quality 1 … A933 quality 12, in quotes (at most 40 characters). A named quality shows as its name and number, e.g. **Slow (1)**; an empty one as **Quality 1**. A quality named `<HIDE>` is left out of the pickers (the machine still has it, so size bands and agents can still give it). Most machine files the original ships name none; its BLM laser template names 1 Slow, 3 Fast, 9 Marking, 10 Center. `part-info` gives each contour's `quality_name`; `machine-tech` and `machines` list the 12. | Observed; "Quality 1" where the original shows the bare number "1", and trimming spaces round a name, are ours |
| CQ5 | **Quality colours** are the program's display colours, the same for every machine: line C041 of the colour settings in the Lantek folder (`LANTEKIN.CNI`, `Cnnn R G B style width`) is quality 1's … C052 quality 12's. Without that file or line a quality takes the colour the original ships with: 1 255 128 64, 2 0 128 192, 3 255 0 255, 4 0 128 0, 5 128 0 128, 6 128 64 64, 7 0 128 255, 8 0 128 128, 9 0 64 128, 10 128 128 255, 11 128 255 255, 12 255 128 255. The pickers show a dot in the quality's colour before its name. | Observed (lines, shipped colours); a missing line taking the shipped colour instead of black is ours |
| CQ6 | **A cut quality set by hand on a contour** (agents: `contour-tech quality=1…12`) wins over the rest of CQ1: the part's size-range rows, the part's own quality and the machine's size bands. It is that contour's quality everywhere: times, leads, kerf and the LC5's NEXT_L with the pierce code its quality has (BL22). Only a Cut contour takes one. A Mark is refused (marks are always made at the machine's marking quality, with no pierce); so is a No cut, Bend line or Leave out contour (set it to Cut with the quality instead), and a quality outside the machine's 1–12. `auto` gives the contour back to the rest of CQ1. A contour set to anything but Cut loses its hand-set quality, so when it is cut again the rest of CQ1 decides. With *All the same contours* (CS5) every equal contour that is cut takes it; equal contours that aren't cut are left as they are and named. Part-info shows the quality as set "by hand". Kept with the job and the library product; files saved before have none. | Estimated (own design, ADR 0001; the original's per-contour technology isn't studied) |


## Machine technology file (MchFiles\PstAdi06.CT1, the BLM laser)

The machine's technology values are in plain text beside its post: `Xnnn value ; comment`
(Spanish comments). Values below are from `MchFiles\PstAdi06.CT1` (4605 lines). The
rebuild reads the active machine's CT1 (next to its .CF1/.PS1/post library, else
PstAdi06.CT1); `ct1.rs`.

| # | Code | Value | Meaning (file comment) | Used for |
|---|------|-------|------------------------|----------|
| MT1 | E000 | 1 | Zona donde se mete la ES (1..9) | lead position (keypad) **Corrected in v44 build analysis:** E000… is quality 2; quality 1 is B200 (AL1). |
| MT2 | E001 | 135.0 | Angulo maximo para ES en esquina | read (corner limit) |
| MT3 | E004 | 5.0 | Longitud minima del elemento | read |
| MT4 | E014 / E015 | 7.0 / 5.0 | Distancia entrada / salida agujeros circulares | read (holes) |
| MT5 | E044 / E045 | 7.0 / 5.0 | Distancia entrada / salida contornos 1 | Lead-in/Lead-outs → **Machine** **Corrected in v44 build analysis:** quality 1 reads B244 / B245. |
| MT6 | E046 | 45.0 | Angulo entrada contornos 1 | read |
| MT7 | B156 | 0.5 | Solapamiento de las entradas y salidas | **Overlap**: the cut runs 0.5 mm past its start before the lead-out (`( OVERLAP )` line) |
| MT8 | B157 | 10.0 | Radio del bucle | read |
| MT9 | F001 / F003 | 10.0 / 45.0 | Amarres por calidad: long. entrada / angulo entrada | joint lead length / angle |
| MT10 | F005 | 5.0 | Amarres por calidad: long. no corte | joint gap width |
| MT11 | F120 | 100.0 | Distancia entre amarres | **One joint every 100 mm** (Micro-joints → Settings → One every / Machine values) **Corrected in v44 build analysis:** F120 is **not read** by the original; the distance is **F150** (+4 per quality, default 100.0), and the count is rounded **up** and made even (AM1). |
| MT12 | F121 / F122 | 0 / 32767 | Minimo / maximo numero de amarres por contorno | clamp of the spaced count |
| MT13 | B158–B164 | 8.0, 8.0, 45.0, 45.0, 5.0, 5.0, 5.0 | Auxiliary geometry joint leads in/out, angles, radii, joint width | read only |

The spaced count is `floor(perimeter / F120)` kept within F121–F122 (rounding estimated).

**Start at the first joint (MJ5, now built):** an outer contour whose joints have leads starts
at its first joint's lead-in and ends with that joint's lead-out. There is no separate
lead-in and one pierce less. Not done for picked or manual starts.


## Where a lead goes on a picked element

| # | Rule |
|---|------|
| LP1 | A lead is refused on an element that belongs to a lead (attribute class 5), already carries a bridge or a loop, or has no technology. Near a corner (within the linear and angular tolerances) it goes in the corner instead. |
| LP2 | Straight element (type 1): `L = |end − start|`; refused unless `L > tol × 4.0 + |w|` (`w` = the lead record's overlap; `tol` = the linear tolerance). Arc (type 2): the same test on `radians(|sweep|) × radius`. |
| LP3 | `r` = distance from the element's start to the pick projected on it ÷ L. **r ≤ 0.33333** → the lead goes at `tol + tol + w` from the start; **0.33333 < r < 0.66666** → at `(L − w) × 0.5`; **r ≥ 0.66666** → at `L − (tol + tol) − w`. The point is the start moved that far along the element. On an arc the same thirds apply to the angle. |
| LP4 | Lead type 12 (the laser lead) is placed by its own routine; the others as above. |

→ `nest::Element::lead_point`, `LEAD_PICK_LOW/HIGH`, `LEAD_ELEMENT_FACTOR`, `LEAD_LINEAR_TOL` (0.01, PstAdi06.CF1 A005).
In the Part view the click position is kept (`sel_pick`). **Lead-in/Lead-outs → On element** puts the lead near the start, in the middle or near the end of a line, by the third you clicked, and the status line says which. Arcs, circles and selections without a click keep the middle (E4). Taking the distance to be the linear tolerance is inferred (the same tolerance pair is used for the corner test).


## Moving a lead by hand (dragging its end)

The original can edit a single lead's geometry by hand, next to the joint-lead edit (J8). That
routine wasn't traced, so the rules below copy J8's gesture (the foot stays, the free end follows
the pointer) and are otherwise our own.

| # | Rule | |
|---|---|---|
| LE1 | **A lead moved by hand belongs to one contour of the part type**, so every piece of that part on every sheet has it. It keeps the contour's start and the lead record it was built from. The lead-in's pierce and the lead-out's end are each kept as a **straight distance** from where the lead meets the contour and an **angle from the contour**: the lead-in's angle is measured from the contour behind the start, the lead-out's from the contour ahead, both turning into the scrap. A turned or mirrored piece leans the same way against its cut. Both are measured on the drawn contour, so kerf compensation (KC8) doesn't change the numbers. When the part's cut contours change (CS4), a moved lead stays on its own contour, and goes when that contour is no longer cut. Up to **16** contours of a part can have one. | estimated (own design) |
| LE2 | **Dragging an end**: the foot stays on the contour and the end follows the pointer. The angle stays inside the scrap side's opening: 180° on a straight or round stretch, and 360° less the material's angle at a corner (so at a 90° outside corner it can swing through 270°, round past the first side's line). It keeps **1°** clear of each side, and a pointer past the opening gives the nearer side. The length is at least **1 mm**. Both snap, to **0.1 mm** and **1°**. | J8 (gesture) observed for joint leads; the rest estimated |
| LE3 | **The moved lead's shape**: a straight line from the end to the contour. A lead whose record meets the contour with an arc (laser types 12 and 13, and the tangent-arc forms of types 1–9) keeps that arc's radius and runs straight to it tangentially, so the cut still enters and leaves along the contour. An end dropped closer than the top of the arc gets a smaller arc that starts at the end itself. | estimated (own design) |
| LE4 | **In the cut plan** a contour picked by hand on the sheet (manual machining) still wins. A moved lead wins over a start picked on an element and over the automatic placement, and that contour no longer starts at its first micro-joint (MJ5). Moving a lead on a part without its own technology gives the part the machine's automatic technology as its own, so nothing else changes. | estimated (own design) |
| LE5 | **Collisions**: a moved lead is never moved along its contour. It is shortened, in the machine's shortening steps (AL11), when it would cross its own part or leave the sheet (VP3), and also when it would hit another part if the machine tests leads (B185 > 0). So a piece next to the sheet edge or a neighbour may cut a shorter lead than the Part view shows. | estimated (own design) |
| LE6 | **Putting it back**: double-click the end, or `lead-edit mode=reset`. *Delete lead-ins* removes every moved lead, and *On element* on that contour replaces it. Undo works too. *Auto lead-ins* and *Standard leads* keep moved leads, as they keep starts picked on elements. | estimated (own design) |
| LE7 | **A part on its own** (Part view, `part-info`, Verify Machining of a part, the agents' part picture) has no sheet around it, so the sheet edge (VP3) doesn't move or shorten its leads. Before this, the part sat at the sheet's corner, and a lead at its bottom-left was moved away from where the tables put it. | estimated (own design) |
| LE8 | **All the same contours for leads.** A box at the top left of the Part view (shown when two or more of the part's holes are the same shape; off by default; agents' `lead-edit same=true`): a lead moved on one hole goes on **every hole of the same shape** as CS5 matches them (moved, not turned or mirrored; never the outline), with the same length and angle at the same place on each. It is kept once, on the hole it was moved on, so it takes **one** of LE1's 16 places, and a hole of that shape that is cut later takes it too. Sharing it puts it on all of them: their own moved leads and starts picked on an element go. A hole given its own lead or a start on an element afterwards keeps that instead. With the box off, a lead let go on one hole of such a shape brings up **Apply to them** at the top centre, which puts it on the others; it goes when another lead is moved, the lead is put back or another part is shown. Moving a hole that shares a lead with the box off gives that hole a lead of its own; when it is the hole the shared lead is set on, the others keep it, set on the first of them that has no lead or start of its own (the same when that hole takes a start on an element, or stops being cut, CS4). Double-clicking a shared lead's end puts it back on all of them; with the box on, it puts back the leads of every hole of the shape. | estimated (own design; the original has an *All the same contours* selection option in its technology panels, but how it acts on leads isn't traced) |

In the Part view each lead-in's pierce has a red dot and each lead-out's end a blue one. Hovering
one shows its length and angle and turns the pointer into a hand. Dragging shows the length and
angle beside the pointer, and the status line says what is cut (and if it was shortened).
While the button is down the view keeps its framing, so the part doesn't shift as the lead
grows; after the drag it stays put until *Fit*. Space-drag still pans. Agents use `lead-edit` (`in_length`, `in_angle`, `out_length`,
`out_angle`, or `mode=reset`); `part-info` lists every contour's `lead_in` / `lead_out` (length,
angle and the angles it can take) and `lead_by_hand`.
→ `leads::build_hand`, `leads::hand_polar`, `engine::drag_lead_end`, `engine::set_lead_end`, `cnc::plan_part`.


## Cut order by sheet zones

| # | Rule |
|---|------|
| CZ1 | The sheet is split into a grid of cutting zones: zone width = sheet length ÷ X zones, height = sheet width ÷ Y zones (each only when its count is above 0), drawn from the sheet origin with X+1 by Y+1 lines. Cutting and marking have their own X/Y counts. |
| CZ2 | Default counts: all four are **20**. |
| CZ3 | The BLM machine file sets them: B120/B121 marking 20 × 10, **B122/B123 cutting 20 × 10**; B110 start left ON, B111 start bottom OFF, B114 horizontal OFF, B115 zig-zag OFF. |
| CZ4 | A part is in the active zone when its box overlaps the zone (rejected if `zone.x0 > part.x1`, `part.x0 > zone.x1`, `zone.y0 > part.y1` or `part.y0 > zone.y1`), and then (zone mode ≠ 4) when one of its box corners — (x0,y0), (x1,y1), (x0,y1), (x1,y0) — lies inside the zone. |
| CZ5 | The next zone is the next index in the zone list: a zone found is marked done, and its box and size are set from the zone record. |
| CZ6 | An example BLM program places its 14 strips at ORIG(10,10), (10,93.2) … (10,1091.6): bottom up, although B111 is OFF. So the rebuild starts at the bottom and doesn't follow B111. |

→ `ct1::zones`, `ct1::box_in_zone`, `cnc::plan_manual`: the sheet is cut zone by zone (columns from the left, each bottom up; rows first with B114, every other one reversed with B115). In each zone the parts in it are taken nearest-first, and parts sitting in another part's hole still go first. The counts come from the active machine's CT1 (`ct1::set_active` when settings are applied); without a file, 20 × 10. Settings → Machines shows and changes the start corner, columns or rows, zig-zag and the counts per machine (CO16).

Still missing in nesting, found in the original and the CT1 but not built:
- **Filling holes with small parts**: B134 "test parts inside holes" ON, B076 minimum hole area 0.0.
- **Common-line cutting**: B074 = 0, so off in the BLM machine file.
- **Multi-torch**, and **remnant cut into squares** (B420 = 0, off).
- **Breaking contours that span two zones**: used with work zones and clamps on punching machines.


## Cut order of a part's holes (machine settings)

What the original does when nobody has set a part's cut order by hand. A study (spec #70); the
build ticket for the setting is #76.

| # | Rule | Source |
|---|------|--------|
| CO20 | **Where the hole order settings live.** They are cut settings in the machine file, with these defaults when the file doesn't set them: Order mode of the holes **B565 = 1** (By proximity; 0 None, 2 By direction, 3 Minimum heat), start corner **B566 = 0** (the corner nearest the head), direction **B567 = 1** (rows; 2 columns), orthogonality tolerance **B568 = 1.0 mm**, Cut small holes first **B587 = 0** (off), big-hole minimum length **B588 = 0**, minimum width **B589 = 0**, offset **B590 = 0**. Most of the original's machine files keep these defaults. | observed; the names of modes 0 and 3 estimated |
| CO21 | **By proximity.** After each cut the head is where that cut ended (the first hole: CO27). Each hole is measured from the head to its point: the middle of its box while it is a closed hole without a lead-in, otherwise where its cut (its lead-in) starts. The holes are compared one by one with the best so far, and the first one met starts as the best. A hole replaces the best when its distance is more than the linear tolerance below the best distance. When the two distances are within the tolerance, it replaces the best only when its sideways distance (along X) is more than the tolerance below the best's; the best distance then stays the earlier hole's, and only the sideways distance is updated. Otherwise the hole met first stays. The outline is taken only once every hole is cut. Estimated: the order holes are met in (outward from the head through a grid of zones over the part, then the part's own contour order inside a zone) and which linear tolerance is used (the machine's linear tolerance). | observed; search order and tolerance source estimated |
| CO22 | **By proximity path optimisation** (Normal or Optimized, number of passes): **not found.** The machine file's hole order settings have no key for it, and the original's proximity order has no optimising step: it is the plain nearest-next of CO21. Tried: every key next to the hole order settings, and the proximity order of the part and of the sheet. MAC-63. | not found |
| CO23 | **By direction (and None): picking the next hole.** The start corner is chosen once per part (CO27): start corner 0 takes the corner of the part's outline box nearest the head (on a tie: lower left, lower right, upper right, upper left), 1–4 fix that corner in the same order. Direction 1 cuts rows, 2 columns. The run starts away from the start corner (to the right from a left corner, upward from a bottom corner). Each pick: (1) the row value is the start-side edge nearest the start corner among the holes not yet cut: the lowest bottom edge from a bottom corner, the highest top edge from a top corner (rows), the lowest left edge from a left corner, the highest right edge from a right corner (columns); (2) the candidates are the holes not yet cut whose same edge is within the orthogonality tolerance of the row value; (3) the pick is the candidate nearest the start of the run: running right the smallest left edge, running left the largest right edge, running up the smallest bottom edge, running down the largest top edge. A candidate replaces the best only when strictly better, so among equal edges the hole met first stays. The outline is taken only once every hole is cut. Estimated: the order holes are met in (with None the part's own contour order; with By direction bands across the run from the start side, then the part's contour order inside a band). | observed; corner numbers 1–4 and the met order estimated |
| CO24 | **By direction zig-zags; None and mode 3.** With By direction a fourth step follows each pick: (4) the pick's start-side edge is compared with the same edge of the hole cut just before (for a part's first hole, with the extreme start-side edge over all its holes); when they differ by more than the orthogonality tolerance a new row has begun, so the run is reversed and steps (1)–(3) pick again. The hole cut then becomes the reference for the next test. With mode 0 (None) a part's automatic machining does steps (1)–(3) only: the same rows from the same start corner, every row run the same way. Mode 3 takes the first hole not yet cut in the part's own contour order. | observed; the names of modes 0 and 3 estimated |
| CO25 | **Cut small holes first.** A hole is big when its box along X reaches the minimum length or its box along Y reaches the minimum width (either is enough). Reaching a limit means being at least the limit, or short of it by no more than a tolerance (estimated: the sheet's linear tolerance). The others are small. The big hole's box grown by the offset on every side is its area, and every small hole whose box lies in or overlaps that area is cut before the big hole. With the defaults (0 and 0) every hole is big, so nothing changes. The original applies it when it orders holes on the sheet, not in a part's own automatic machining. The order of the small holes among themselves wasn't traced; estimated: the hole order mode. | observed; the tolerance's source and the order among the small holes estimated |
| CO26 | **Ordering the part's contours** (A310 when the part is saved, A327 when the part comes into the nesting; both off by default) doesn't change the cut order. It builds the part's contours from the drawing when needed and turns every closed cut contour to the machine's cut direction: holes counter-clockwise when A014 is on, the outline counter-clockwise when A015 is on, otherwise clockwise. With either switch on, a mirrored part's contours are always reversed. | A327 observed; A310 estimated |
| CO27 | **Start points and corners.** (a) In a part's own automatic machining the head starts at the part's FROM point when its machining has one, otherwise at the drawing's origin (0, 0), in the part's own drawing frame. By proximity measures the first hole from there, and start corner 0 takes the outline-box corner nearest that point; after each cut the head is where the cut ended. (b) For a part on the sheet, estimated: the head's position after the previous part, in sheet coordinates, with the part's box as it sits on the sheet; which of the original's routines orders a part's holes in the sheet program wasn't settled (MAC-63). (c) The sheet's zones start in the corner set by start left (B110) and start bottom (B111): the left column when B110 is on, else the right; the bottom row (lowest Y) when B111 is on, else the top. The original's defaults are start left on, start bottom on, horizontal (B114) on and zig-zag (B115) off. Another sheet setting, the cut order criterion (B444, default 0), can take the side nearest the head instead; at 0 it doesn't. | (a) and (c) observed; (b) and when B444 applies estimated |
| CO28 | **Why an example program cuts the sheet bottom-up with start bottom off (CZ6): not found.** In the original's zone order, start bottom off starts at the top row (highest Y) of the nesting, and the cut order criterion at its default doesn't move it. The sheet's reference corner setting (A252) may map the nesting's frame onto the program's, but that wasn't traced. MAC-63. | not found |
| CO29 | **Marks in a part's automatic order.** First the centre-punch dots and cycles (when the machine makes them), then the marking texts, then every marked contour, then the cuts: holes in the hole order, the outline last. Marks are taken nearest-first from the head, and an open mark starts at its nearer end. With Manage marks as cuts (**B699**, default off) there is no separate marking pass: marks are ordered among the holes. With order by qualities (B142) the cuts go one quality at a time. | observed |

What the app does with them (#76, #77; estimated or not found: MAC-61, MAC-62, MAC-63):

| # | Rule | |
|---|------|---|
| CO14 | **A part's holes in the machine's hole order.** The holes of a part that nobody has ordered by hand (CO3) are cut in the order the machine file's Order mode of the holes gives, read with the original's defaults (CO20); the agents' `machines` report shows the settings. **None**: the part's own contour order, as every part was cut before the setting; this differs from the original on purpose, whose None runs rows from the start corner (CO24). **By proximity** (the default): the nearest hole next (CO21); two holes as near within the machine's linear tolerance go to the one nearer sideways, else to the one first in the part's own order. **By direction**: rows, or columns, from the start corner, every other one run back (CO23, CO24); a hole whose start-side edge is within the orthogonality tolerance of a row's, the tolerance's edge included, is in that row; start corner 0 takes the corner of the part's box nearest the head, a tie going lower left, lower right, upper right, upper left. **Minimum heat**: the part's own contour order (CO24); the sheet's minimum-heat order isn't built (MAC-53). On a sheet the order is worked out where each copy sits, turned or mirrored, so By direction runs from the start corner along the sheet's rows or columns however the copy is turned: the head starts where the sheet's last cut was (at the sheet's origin before the first), and after each cut (a hole or an outline) it stands at that contour: the middle of its box, or where its cut starts when that is set by hand (a closed cut ends where it starts). The holes set by hand go first and the rest follow from the last of them; a hole picked in Manual cutting goes first on its sheet (CO10) and the rest of the part follows from where the head is after the picks (a hole set by hand that a pick has already cut no longer counts); a part taken whole by a sweep or a box in Manual cutting (MM8, MM9), or picked by its outline, orders its holes the same way, from where the head is when its turn comes; parts sitting in a hole are still cut before it (CO1, CO2). Built-in shapes follow the setting too. `part-info` and the part screen show the order as the part is drawn, the head starting at its lower left corner, so a turned copy on a sheet can differ; a bulk `part-cut-order` command (all cut, all of a quality) and a box on the part screen (CO12) take the holes in that order. | estimated (own design on CO20–CO24, CO27; MAC-61) |
| CO15 | **Cut small holes first** (CO25). With the machine's Cut small holes first on, a hole is big when its box reaches the minimum length along X or the minimum width along Y (within the machine's linear tolerance, so a hole exactly at a limit is big); the others are small. The small holes whose boxes lie in or overlap a big hole's box grown by the offset on every side are cut just before it, in the hole order mode among themselves from where the head is. With the limits at 0 (the defaults) every hole is big and nothing changes. It moves only holes nobody ordered by hand, so a big hole set by hand is cut where the order puts it. It applies on the sheet and in `part-info` alike (the original applies it on the sheet only). | estimated (own design on CO25; MAC-61) |
| CO16 | **The sheet order in Settings → Machines.** Settings → Machines shows each machine's sheet order and lets the shop change it: the start corner (lower left, lower right, upper left or upper right: the corner the first cutting zone is in, CZ3, CO27 (c)), columns or rows, zig-zag (every other column or row runs back) and the number of cutting zones across and up (CZ1; 0 or 1 is one zone, at most 500 each way). Each value starts as the machine file's (B110, B114, B115, B122 / B123); a value the file doesn't set is the BLM file's (CZ3): lower left, columns, no zig-zag, 20 × 10. The start corner is shown as the sheet is cut: the bottom row first, whatever the file's start bottom (B111) says (CZ6); when the file has it off, the screen and `machines` say so, and an upper corner set here starts the sheet at the top. A change is kept for that machine in the app's settings and wins over the file, also when one of the file's material sections applies; a value set back to the file's is not kept, so it follows the file again; **Use the machine file** puts back every sheet and hole order value of the machine. A saved change the setting can't take (a mode or corner number it doesn't have, more than 500 zones, a negative length, a value that can't be read) is left out when the settings are read, and the machine's other changes are kept; that value is then the file's, in the program and in `machines` alike. The sheet's program follows the values in use, on the G-code post and the LC5 alike: zone by zone in that order, in a zone the nearest part next from the end of the last, parts sitting in a hole still before it (CO1, CZ4, CZ5); the head starts at the sheet's origin whatever the corner. `machines` shows each value in use, where it comes from (Settings → Machines, the machine file or the default) and the file's own (`sheet_order`), and each machine's changes (`order_changes`); agents can't change settings. Settings saved before have no changes, so every machine cuts as before. | estimated (own design on CZ1–CZ6, CO27 (c); MAC-62) |
| CO17 | **The hole order in Settings → Machines.** The same section shows and changes the hole order of CO14 and CO15: the order mode (None, By proximity or By direction; Minimum heat is offered only when the file sets it), By direction's start corner (nearest the head, lower left, lower right, upper right, upper left), rows or columns and how close holes' edges must be to share a row (the orthogonality tolerance), and Cut small holes first with the big hole's length along X, its width along Y and how near a small hole must be. Each starts as the machine file's (B565–B568, B587–B590), else the original's default (CO20); By direction's options change only with By direction, the size limits only with Cut small holes first on. Kept per machine, winning over the file and put back by Use the machine file as in CO16. A part's holes nobody ordered by hand are cut in the order the values in use give (CO14, CO15), on both posts; the linear tolerance they compare within (A005) stays the file's. `machines` shows each value in use, where it comes from and the file's own (`hole_order`). | estimated (own design on CO14, CO15, CO20; MAC-62) |


## Part data from the file name (v44 4.1 / 4.3)

| # | Rule |
|---|------|
| FN1 | The DXF import settings have a "Text definition in the file name" dialog (build the format from tags and separators, with a separator check), opened by the "format file name" button of the DXF "Other" page. The import settings also have "Save file name in field". |
| FN2 | A separator may not contain any of `<>:"/\|?*`; the original refuses such a separator with a message listing those characters. |
| FN3 | The format names fields in brackets with separators between them, and `*` marks text that is ignored, e.g. `[Reference]_[Quantity]_[Material]`. The values go to the product (D018 holds the file name). |

→ `filename.rs`: `parse_format` (fields `[Reference]`, `[Quantity]`, `[Material]`, `[Thickness]`; `*` skipped; tags case-insensitive; two fields need a separator; no text before the first field or after the last; the separator check uses FN2's characters), and `read` (each field runs to the next separator; a missing separator or a quantity/thickness that isn't a number means the name doesn't follow the format, and the file name is kept as the reference).
Settings → General → **Part data from the file name** stores the format (`name_format` in settings.ini) and shows a live example or the error.
On DXF import the part takes the reference and quantity from the name. A material or thickness different from the job's is reported in the import notes; one job here has one material, so it isn't changed.
Estimated or not built: `[Thickness]` (not in the original's example), how the original reads the field values (not confirmed), re-reading on format change, and DWG/PDF/DSTV/ESSI.


### Reference of an imported part

| # | Rule |
|---|------|
| FN4 | The imported part's reference starts from the given name (the file name). When automatic references are on and a database is open, it is replaced by the next automatic reference for `PPRR_PPRR_00000100` / "SheetPart" / `PrdRef` (or the stored automatic reference value when that is not empty). |
| FN5 | The reference is `prefix + name + suffix`. When their lengths add up to more than **40**, or the alternative prefix/suffix would, the name is cut at `40 − len(longer prefix)` and then at `40 − len(longer suffix)`. |
| FN6 | "Is the part name the file name?" = the file path's name part (no folder, no extension) compared exactly with the part name. |
| FN7 | The importer also knows reference tags `[MAT] [THK] [LNG] [LNG1] [LNG10] [LNG2] [LNG20] [WDT] [WDT1] [WDT10] [WDT2] [WDT20]`, used for references of generated rectangles (material, thickness, length, width). Not used here. |

→ `filename::part_reference` (REF_MAX_CHARS = 40) applies FN5 on every DXF import, with Settings → General → Part data from the file name → Reference prefix / suffix (`ref_prefix`, `ref_suffix`). `filename::name_is_file_name` is FN6. Automatic references (FN4) come from the app's library numbering (`library::auto_ref`, SHP + 6 digits) only when saving to the library, not at import.


## Parts in the holes of other parts

| # | Rule |
|---|------|
| HF1 | Filling holes is its own pass over the sheets already nested: it sets up a guard, runs the semi-automatic nesting with the same sheet and part lists, then undoes the guard. |
| HF2 | Guard: over the nested items it finds the lowest bottom (starting from **999999**) and the highest top (starting from **−999999**). Every item at that lowest bottom or highest top is switched off for the pass. |
| HF3 | After the pass, every switched-off item is switched on again. |
| HF4 | The BLM machine file turns it on: **B134 ON** "Testear piezas dentro de agujeros" (test parts inside holes), and **B076 0.0** "Area minima de agujeros aprovechables para meter piezas" (smallest usable hole area). |

→ `ct1::MachineTech::parts_in_holes` (B134) and `min_hole_area` (B076). With the strip engine the holes of every placed part stay free space for later parts at every level (corrected: they are simply left open in the part's strips, see "True-shape strip nesting"; B134 isn't needed for that), and holes up to B076 (shoelace `nest::polygon_area`) are skipped. The level's description says so. Not copied: the separate pass itself (HF1–HF3); the items in HF2 are the sheet's free stretches, and the pass only switches off the stretches at the sheet's lowest/highest free Y.


## Nesting parts grouped by attributes (v44 3.1.1)

| # | Rule |
|---|------|
| GZ1 | Settings in the machine file: **A785** group parts by attributes (OFF on the BLM), **A786** list of attributes (""), **A787** put each group in its own nesting zone (ON), **A788** gap between the zones (0.0), **A789** remnant lines between zones (OFF), **A790** their line type (1). |
| GZ2 | "Is any part nested in the current zone?": the zone is the sheet less its edge margins (left, right, bottom, top). A placed item counts when its centre (position + **0.5** × size) is inside, with ≥ / ≤ comparisons. |
| GZ3 | The next group is nested in the zone after what was nested before: the start edge is the left or right margin by the nesting direction, a nesting method is set up and the semi-automatic nesting runs on the same sheet; then the band edges are checked again. |

→ `Part::group` (Part Properties → Group; saved in jobs and library products), Home → **Group by attribute** (its start value from A785), `nest::nest_grouped`:
- The groups are taken in the order they first appear.
- Each group is nested on the last sheet in the zone after the parts already there (when `any_part_in_zone`, GZ2 with ZONE_CENTRE = 0.5), starting at their right edge plus the part gap plus the zone gap (A788 when A787 is on).
- What doesn't fit goes on new sheets.

Grouped sheets skip the contour compaction, so groups keep their zones.
Not built: several attributes at once (A786), start corner and direction, and remnant lines between zones (A789/A790).


## Job Explorer right-click menus

| # | Rule |
|---|------|
| JX1 | Folder menu: **Expand · Collapse · New ▸ Folder / Job · Find · Cut · Copy · Paste · Delete · Change name · Refresh · Organize**. Quotation-job and kit trees use the same menu with "Quotation job" / "Kit" under New. |
| JX2 | Job menu: **Open · Export · Cut · Copy · (Import) · Delete · Change name · Refresh · Properties**. |
| JX3 | Nesting Explorer subjob menu: **New sheet… · Save · Save As · Delete every sheet of the subjob · Paste · Automatic Nesting · Prenesting · Automatic Machining · Verify machining · Create subjob · Modify machine · Expand all · Collapse all**. |
| JX4 | New folder: the folders under the same parent and browser type are listed. If one already has the name, the folder isn't created and error **9** is returned. Otherwise the next NodeID is taken and NodeName / ParentID / BwsrType / NodeID are inserted into `DIS_MMNN_BWSR_00000100`. |
| JX5 | Change name and Move (new parent) run the same sibling check (error 9), then update NodeName or ParentID for that NodeID. |
| JX6 | The sibling check compares names **without case**. |
| JX7 | The default name is "New folder" (next to "New job"). |

→ `jobtree.rs` + `jobs.rs` Job Explorer:
- **Open jobs** right-click: New ▸ Folder / Job.
- **Folder** right-click: New ▸ Folder / Job, Change name, Delete (only when empty). Hovering a folder shows its job count.
- **Job** right-click: Open, **Add nesting (new sheet)**, Copy, Change name, Delete… (asks first). Hovering a job shows a tooltip: folder, products, pieces, nestings, material, thickness, sheet size. Double-click renames.
- Names are typed in place: Enter keeps, Esc cancels.

Folder names use JX4–JX6 (`jobtree::check_folder_name`, FOLDER_NAME_TAKEN = 9). A new folder is "New folder", then "New folder (1)" … "(9)" (the job-name rule, estimated for folders).
Add nesting is the original's "New sheet…": the job opens in Nesting on a new empty sheet that stays while it is shown (`App::empty_sheet_kept`), ready for drag-and-drop or Nest all.
Open jobs folders are this window's own. The Library's job folders are a real tree, the same on every PC (below); Find, Organize, Export and Copy for folders are not built (JOB-7).

### The Library's job folders (the rebuild's tree on the original's rules, spec #44)

| # | Rule | Source |
|---|------|--------|
| JX10 | **One folder tree for the Library's jobs**, kept in the Library folder (`job-folders.json`), so every PC that shares the Library sees the same folders. As in the original's tables (DT8, DT9), each folder has a number of its own (one more than the highest ever given, so a number is never given twice), a name and a parent (0 = the top), and each saved job's place is a line naming its record and its folder's number; a job with no line, or whose folder is gone, sits at the top. Renaming or moving a folder changes only its own line and moving a job only its line: job records are never rewritten, so saving a job opened before someone moved it can't move it back. Every change is made under the Library's write lock (LB1–LB3), the tree read again under it and written back whole, so two PCs' changes never undo each other; a refused change writes nothing. Deleting a job from the Library takes its line out. A job's folder, wherever the program names it (the Library window, the Open job menu, the refusal of a taken job name and its "same name" mark, JX16–JX17, agents' `library list`), is its place in this tree, not the folder name written on the job; moving a job keeps its name, so it never meets the name check. Like a save, a folder change (New folder, Change name, Delete, Paste, agents' `job-folders`) is refused while the Library can't be reached or its folder holds no Library, naming the folder, and nothing is made there (LB11, LB12); agents are also told to try again once it can be reached. | Own design on the original's tables |
| JX11 | **Libraries from before folders.** The first time a Library without a folder tree is opened, each folder name written on its jobs becomes a top-level folder holding those jobs, made under the write lock so two PCs opening it at once make one tree. Names that differ only in case are one folder (JX6), spelt as on the lowest-numbered job; jobs with no folder name stay at the top. Nothing moves and no job record is rewritten. | Estimated (JOB-58) |
| JX12 | **New folder, Change name, Move.** New ▸ Folder makes the folder in the Library at once as "New folder", or "New folder (1)" … "(9)" when that is taken among its siblings (JX7), its name ready to type. A name is trimmed, must not be empty, and must not be used by another folder in the same folder, compared without case; otherwise it is refused (error 9, JX4–JX6). Move runs the same check in the new folder, and a folder never goes into itself or one of its own folders. A change to a folder another PC deleted meanwhile is refused, saying to Refresh. | JX4–JX7 observed; the rest estimated |
| JX13 | **Delete** takes only an empty folder: one holding no saved job and no folder. Otherwise it is refused, saying what it still holds ("'Acme' still holds 2 jobs and 1 folder"). Empty folders are kept until deleted. | Estimated (JOB-58) |
| JX14 | **The Job Explorer's Library section** shows the tree: in each folder its folders by name (without case), then its jobs; a folder shows how many jobs are in it and its folders. Right-click a folder: New ▸ Folder, Cut, Paste, Delete, Change name, Refresh (from JX1; the rest of JX1 is JOB-7); the Library itself: New ▸ Folder, Paste, Refresh; a job: Open, Cut. Cut marks a job or folder (shown with ✂), Paste on a folder or on the Library moves it there, and a refused Paste keeps it cut. Every change, and Refresh, reads the tree again, so what other PCs did shows. A search lists the matching jobs with their folders. The Library window and the agents' `library` list name a job's folder by its path ("Acme/2026-11"). Agents: `job-folders` (list, new, rename, delete, move), with the same rules and lock. | JX1 observed; the layout is the rebuild's |
| JX15 | **Where a job saved into the Library for the first time goes**: into the folder of the library job it was opened or copied from, while that folder is still there; otherwise into the top-level folder named like its Open jobs folder, made if missing (no name: the top). Agents' saves go by the job's week the same way. Saving over its record never moves it. A library job opened from a folder sits in an Open jobs folder of that folder's name. The two-PC Library check (LB4) also makes, renames, nests, moves and deletes job folders each round, files that round's new job in one, and reports any folder or job not where its run left it; a test folder whose job folders aren't all its own (DEMO-CHECK) is refused as a possible real Library. | Estimated (JOB-58) |

### The Library's part folders (the rebuild's tree on the original's rules, spec #44)

| # | Rule | Source |
|---|------|--------|
| JX18 | **One folder tree for the Library's parts, apart from the jobs' tree**, as the original keeps product folders in the same table as job folders, told apart by which tree (DT8, DT9). It is kept in the Library folder (`part-folders.json`), the same on every PC, and runs on the jobs' tree's rules: each folder has a number of its own, a name and a parent, and each library part's place is a line naming its record and its folder's number; a part with no line, or whose folder is gone, sits at the top (JX10). New folder, Change name and Move check the name the same way, and a folder never goes into itself or one of its own folders (JX12); Delete takes only an empty folder, one holding no part and no folder (JX13). Moving a part or a folder changes only its own line: part records are never rewritten. Every change is made under the Library's write lock, the tree read again under it and written back whole, and is refused while the Library can't be reached or its folder holds no Library, naming the folder (LB11, LB12). Deleting a part from the Library takes its line out. **Libraries from before part folders:** the first time one is opened, each folder name written on its parts becomes a top-level part folder holding them, made under the write lock so two PCs opening it at once make one tree; names that differ only in case, or in spaces around them, are one folder, spelt as on the lowest-numbered part; parts with no folder name stay at the top; nothing moves and no record is rewritten (as JX11). | DT8, DT9 observed; the tree is the rebuild's own design on them; the upgrade estimated (JOB-60) |
| JX19 | **Where a library part saved into the Library for the first time goes**: Save to library (a part) and Add open job's products put each new part into the part folder named like its job's folder, made if missing. The job's folder is, in this order: the job's own place in the jobs' tree when the job is in the Library, as it is when the part is saved (another PC may have moved it since); else the Library folder of the library job it was opened or copied from, while that folder is still there (JX15); else its Open jobs folder, as a top-level name. A job folder several levels deep is followed level by level ("Acme/2026-11": part folder "Acme", and "2026-11" inside it), each level found by its name without case (JX6) or made; a job with no folder (at the top of the Library, or in an Open jobs folder with no name) puts its parts at the top. The folder is found or made, and the part filed, under the same hold of the write lock as the save. A part folder another PC renamed or deleted meanwhile is looked for by name again, so one of the job folder's name is made anew. Saving over a part already in the Library (one with the same name, PK1) never moves it, whatever folder it is in, and says where it is; Add open job's products leaves such a part where it is. When the save asks first because someone else saved that part since (LB7, LB9): Keep theirs moves and files nothing, Overwrite goes over the part in whatever folder it is in now, and Save mine under a new name files its copy ("DEMO-P1 (1)") like any new part, in the job's part folder, made if missing; the revision check, the save and the filing are one hold of the write lock. Agents' `library save-part` file a new part in the part folder they name (a path, each level made if missing), else in the one named like the job's folder, found as above (its place in the job folders now when its job file is a library job, else its week). | Estimated (JOB-60) |
| JX20 | **The part folders on screen and for agents.** The Library window's Products tab shows the tree as the Job Explorer's Library section shows the jobs' (JX14): in each folder its folders by name (without case), then its parts; a folder shows how many parts are in it and its folders. Right-click a folder: New ▸ Folder, Cut, Paste, Delete, Change name, Refresh; Library parts itself: New ▸ Folder, Paste, Refresh; a part's name: Add to job, Cut, Delete from the library. In either tree, a Paste where the job, part or folder already is moves nothing and says so. A search lists the matching parts, by name or folder, each with its folder. Add product's library list shows each part's folder (on its tile, and on hover) and finds parts by it; agents' `library list` names a part's folder by its path. Agents: `part-folders` (list, new, rename, delete, move), with the same rules and lock. The two-PC Library check (LB4) files its new parts in the part folder DEMO-CHECK and reports any that isn't there; a test folder holding part folders that aren't the check's own is refused as a possible real Library. | JX1 observed; the layout is the rebuild's |



## Jobs in use and the Job Explorer's auto-refresh

What the original does when several programmers work from one shared database: how it marks a job
as **in use** and how its job explorers catch up with other PCs (CU1–CU4, JX8, JX9). The
shared-Library build (spec #44) follows these settings and defaults (ADR 0002) and notes where it
adds its own: the auto-refresh is LB14–LB16; the holds, Take over and refusals are CU5–CU12.

| # | Rule | Source |
|---|------|--------|
| CU1 | **The concurrency setting.** Configure → System → *Concurrency management* has two switches: **warn** when a job is in use by another user, and **allow** opening a job that is in use. On a fresh install warn is **off** and allow is **on**. Allow is only shown while warn is on. Both are kept in each PC's own configuration file (keys Z083 and Z084), so every PC sets its own. Warn is really the master switch: while it is off, that PC neither marks the jobs it opens nor checks anyone else's (CU3), and turning it off removes that PC's own mark at once. **Rebuild:** warn is **on** in a new install, the rebuild's own default and an exception to ADR 0002, because sharing one Library between PCs is the point of spec #44. It stays a per-PC setting a shop can turn off; allow keeps the original's default, on. | Observed; the rebuild's warn default is its own |
| CU2 | **What makes a job in use.** Opening a job in the Jobs window (with warn on) writes a **hold**: a small file in a `Sysdata` folder inside the shared database folder, so every PC sees it. There is one hold per user and PC, named after both, and it records the job, its kind (job, kit, quotation job), the date and time it was written, and the job's parts. Because it is one per user and PC, opening another job replaces it: a user holds one job per PC. **Closing** the job or **quitting** normally removes it; a crash or a PC switched off leaves it behind (CU4). Only the job open in front is held (the original has one open job; unsaved changes aren't held separately). Deleting a job removes every hold on it (estimated). The original names the user by its own login; the rebuild uses the Windows user and the PC name (spec #44). | Observed; deleting and the user name estimated |
| CU3 | **What another user sees** when opening a job someone else holds (another user, or the same user on another PC): warn **off** (the default): nothing, the job opens as usual and nothing is held. Warn **on**, allow **on**: a warning that the job is in use, listing who has it, on which PC and since when, and asking whether to carry on. *Yes* opens it normally (not read-only) and both now hold it; *No* closes it again. Warn **on**, allow **off**: the same warning with only a button to close, and the job is closed. There is no read-only mode and no way to take the job over or clear the other hold. A run without a window (batch) never asks: it logs each holder and carries on. | Observed |
| CU4 | **Holds left behind.** A hold never expires: its time is shown, never checked. It goes only when, **on the same PC**, the same user opens or closes a job or quits, or warn is turned off there; when the job is deleted (CU2); or when someone deletes the file by hand. Until then others are warned and, with allow on, may carry on. **Rebuild:** a hold not renewed for about 2 minutes counts as gone, and anyone can *Take over* after a warning (spec #44), so one dead PC can't block a job all day. | Observed; the rebuild's expiry and Take over are its own |
| CU5 | **In use, and its hold (rebuild).** A Library job is **in use** on a PC while it is the job in front there, or has changes there that aren't saved; that PC then **holds** it: one small file per job in the Library folder (`Holds\<record>.json`), naming the user and the PC and since when. Taking, renewing, taking over and letting go of a hold are changes to the Library made under its write lock (LB1–LB3), the hold read again under it. Opening a Library job with warn on takes its hold first, before the job opens, so of two PCs opening one job at the same moment one holds it and the other is asked (CU8). The hold goes when the job leaves the front with nothing unsaved, is closed, or the program quits, and every hold of a PC goes when warn is turned off there (CU1). The same user on the same PC (capitals aside) is that PC: after a restart it goes on holding the job, keeping its time; the same user on another PC is someone else (CU3). Only jobs in the Library are held: a job file, or a job never saved to the Library, has nothing to hold. With warn off a PC holds nothing. | Own design (spec #44); estimated |
| CU6 | **Renewing.** The window renews its holds about every **30 s**, in the background so a share that's slow to answer, or a write lock a crashed PC left (LB2), never stops it; so are the holds taken and let go when another job comes to the front, a job is closed or saved, or warn is switched, one piece of work at a time (opening a job from the Library takes its hold in place, as saving waits in place). What that work finds is taken in with the switches as they are by then: a hold it renewed for a job no longer in use here, or after warn was turned off, is let go again; quitting waits a few seconds for work under way, then lets go of every hold. The same renewal takes a job it has open without holding it (opened anyway, CU8) once the other PC lets it go, and reads every hold for the in-use notes (CU10). A renewal that can't reach the Library changes nothing, closes no job and drops no change (LB13): it says so once with the LB11 message, not every 30 s, and once the Library is back the holds are renewed, unless another PC took one meanwhile (CU7, CU9). A renewal that finds a held job deleted from the Library says so; the job stays open here, and Save to library stores it again (LB7). | Own design; estimated |
| CU7 | **A hold left behind is gone after about 2 minutes**, judged by the PC looking at it, on its own clock, never by comparing two PCs' clocks (as LB2 judges a write lock). Every renewal changes the hold, so a hold this PC has seen unchanged for **2 minutes** wasn't renewed and counts as gone. A hold whose last renewal, by its holder's clock, is more than **10 minutes** behind this PC's clock is gone at first sight (a PC that crashed long ago). So clocks a few minutes apart never make a live hold look gone, nor keep a dead one in use for more than 2 minutes of watching. Every look counts: opening a job, the lists, each renewal. A hold that can't be read (half-written or damaged) is in use by a PC it can't name until it has stayed unchanged for 2 minutes. The next PC to take a job whose hold is gone writes over it. | Own design; estimated |
| CU8 | **Opening a job someone else has in use**, with warn on (from the Library window, the Job Explorer, the Open job menu, at start-up, or an open job brought to the front): "DEMO-7 is in use by Sam on PROG-2 since 09:14." With allow on: **Open anyway** (it opens here as usual and saves as usual, through the save check, LB7–LB10, as CU3's "opens normally"; the hold stays theirs, and this PC takes it once they let it go), **Take over…** (CU9) or Cancel. With allow off: Take over… or Cancel, and it isn't opened. Nothing is held while asking. A job that comes to the front by itself (the one next to a job just closed) isn't asked about: it opens without a hold, and the status line says whose it is. With warn off nothing is asked and nothing held (CU1, CU3). | CU3 observed; Take over and the rest are the rebuild's (estimated) |
| CU9 | **Take over.** After a warning naming the holder and since when, and that their PC is told and can then only save their unsaved changes as a copy, the hold moves to this PC whatever it said, under the write lock, and the job opens (read after the hold moved). The PC that lost it is told at its next renewal ("Sam on PROG-2 has taken over DEMO-7"), even when the PC that took it over has let it go again by then: letting go of a hold that was taken over keeps a small note in its place (`Holds\<record>.over.json`: who took it over, and the PCs it was taken from not told yet), until each is told, someone holds the job again (who they then see), or the job is deleted; its job stays open with its changes, and the job is never taken back without asking. There, saving over the job is refused while the other PC has it, offering **Save mine as a copy** (as LB9) or Take over; only the PC it was taken over from is refused this way (a PC that opened it anyway saves as usual, CU8, CU11). A save the holder had already begun when the takeover came finishes first: both need the write lock. | Own design (spec #44); estimated |
| CU10 | **The in-use note.** The Job Explorer's Library section, the Library window and the Open job menu show "In use by Sam on PROG-2 since 09:14" (this PC's local time, with the date when it isn't today) on a job another PC has in use, looked at again, in the background and taking no lock, with each reading of the lists (Refresh, and the auto-refresh's when it reads them again, LB14) and each renewal (CU6). A PC's own jobs carry no note. Agents: `library` mode=list gives each job in use `in_use` ("Sam on PROG-2 since 09:14") and `in_use_since` ("2026-11-09 09:14"). | Own design; estimated |
| CU11 | **No change to a job someone else has in use.** Deleting it from the Library, renaming it, moving it into another folder, and renaming, moving or deleting a folder that holds it at any depth are refused, naming who has it and since when ("DEMO-7 is in use by Sam on PROG-2 since 09:14."); a save that gives it a new name is renaming it. This holds whatever this PC's own warn switch says: the switches decide only whether this PC is warned and may open it. A hold that can't be read now (the share answers with an error) refuses the change too, since whether the job is in use isn't known: "The holds in the Library can't be read now (…): nothing was changed. Try again." Saving over it isn't refused: a job opened anyway, or with warn off, opens normally (CU3) and saves as usual, and the save check asks when the holder saved since (LB7–LB10). Only the PC it was taken over from (CU9) and agents (CU12) are refused a save over it, checked under the write lock before anything is booked (LB8). Jobs this PC holds itself aren't refused. Deleting a job takes any hold left on it (CU2). The save check (LB7–LB10) stays on whatever the holds say. | Own design (spec #44); estimated |
| CU12 | **Agents never hold a job.** The command line and the MCP server take no hold, and are refused a change to any job that is held, their own user's included: `library` mode=save-job over it (a job renamed with `rename` reaches the Library that way), `job-folders` move of it, and rename, move or delete of a folder holding it: "DEMO-7 is in use by Sam on PROG-2 since 09:14. Try again later.", then what to do instead (save-job copy=true). Reading is allowed (`library` list and open). The two-PC Library check (LB4) also takes, renews and lets go of holds each round, on its own jobs and on one job every run shares, and reports a hold taken from under a run without Take over. | Own design (spec #44); estimated |
| JX8 | **The auto-refresh option.** Configure → Draw and Nest has one auto-refresh setting for the job explorers: a tick box with a period in **whole minutes**, kept on each PC as one number (key Z076, 0 = off). On a fresh install it is **off**. When on, the Job Explorer (and the kit and quotation-job explorers) reload from the database once every period, only while that explorer is shown; the manual adds that the open job and its views are brought up to date as well. Apart from this setting, an explorer always reloads after the user's own changes on that PC (deleting a job, for example). **Rebuild:** copies the setting and its off default. While it is on, the rebuild also runs a quick change check every few seconds, so other PCs' saves show within seconds; the period still sets the full reload, as in the original (spec #44). | Observed; what one refresh reloads is the manual's; the rebuild's quick check is its own |
| JX9 | **The timer's limits.** Off by default (0). Ticking the box starts the period at **1 minute**; 1 minute is the shortest, and anything below 1 means off. No upper limit was found. **Rebuild (estimated):** 1 to 1440 minutes (a day): a longer period is no use in a shop, and it keeps the timer far from overflowing. | Observed; upper limit estimated (JOB-54) |


## Job names in the Library

| # | Rule | Source |
|---|------|--------|
| JX16 | **Job names are unique across the Library.** No two jobs in the Library have the same name, whether they are in one folder or in different ones. Saving a new job into the Library, saving a job as a new one (Save as), or renaming a job to a name another job already has is refused, naming that job, its reference and its folder ("the job 'DEMO-001 frames' (JOB000001) in folder Acme already has that name … pick another name"), and nothing is saved: no record is written and no sheet is taken from stock. The window checks the name, takes the job's sheets from stock and stores the job as one change under the Library's write lock (LB1), so when two PCs save a new job under one name at the same moment, one is stored and the other refused. Renaming a Library job in the Job Explorer is checked at once, and again when it's saved. Agents get the same refusal from `library save-job`, telling them to rename the job first. The open jobs keep their own rule: no two open jobs with exactly the same name, and a second copy of a job opens as "… (1)". | The original's rule as spec #44 gives it (not traced here); the lock, the order and the wording are the rebuild's own |
| JX17 | **Names are compared ignoring capitals**, and spaces before or after them, as folder names are (JX6): "DEMO-001 Frames" and "demo-001 FRAMES" are one name. **Libraries saved before JX16** may hold jobs that share a name. Each keeps saving under its own name, exactly as it is, and the Library window marks every such job "same name", naming the other jobs on hover (agents: `same_name_as` in `library list`), so someone can rename one. Renaming one to another job's name, in any capitals, is refused as in JX16. | Estimated: the comparison; the handling of older Libraries is the rebuild's own |


## The active job (ORDENACT.SYS)

| # | Rule |
|---|------|
| AJ1 | Expert has one **active job**, kept in the file **ORDENACT.SYS**. Opening a job writes it line by line: job name, a second name line, the job type (as a decimal number), and, when present, a quotation reference followed by its number. |
| AJ2 | Reading it back: no file → no active job (type 3 and a return of 0). Name and second line are read up to **40** characters. The type line is read the same way: **1** stays 1, **2** stays 2 (quotation job), anything else becomes **3**. Then a 260-character line and a number line (default −1). |
| AJ3 | "Close the current job" removes ORDENACT.SYS (when it exists), so no job is active until another is opened. |
| AJ4 | The database side asks for the active job's name and type when it assigns lots and quotations to it. |

→ The rebuild keeps the active job as the open job's **library record** (`settings.ini` → `active_job`, kept in step each frame):
- At start that saved job is opened again and made current (`jobtree::active_job_id`; an empty or unreadable record means none, as in AJ2, read up to ACTIVE_JOB_LINE_MAX = 40 characters). If it can't be found the record is cleared.
- Job right-click → **Close** takes a saved job out of Open jobs (AJ3); it stays under Saved jobs (Library). An unsaved job can't be closed silently: save it first or Delete it.

| # | Rule (rebuild: keeping work safe) | Source |
|---|------|--------|
| SV1 | Each open job remembers what it looked like when last **saved or opened**. A job that no longer matches has **unsaved changes**; one never saved counts when it has parts or nestings. The example jobs count as saved. | Rebuild |
| SV2 | **Closing the window** with unsaved jobs asks first, naming them: *Go back and save* or *Close without saving*. **Close** on a job with unsaved changes is refused like an unsaved job (save it, or Delete it to drop them). | Rebuild |
| SV3 | A job's file and library record stay with that job: **deleting or closing** the open job shows the next job with its own file, and a **copy** has no file and no record, so saving it asks where. Changing the **Library folder** forgets the open jobs' record numbers (they'd name other jobs in the new folder); they're saved as new records there. | Rebuild |
| SV4 | **Save** checks the job's file first: when another program (an agent) changed it since the window read or wrote it, nothing is written and the window asks which version to keep. A save from the window removes the agents' one-step undo copy (`<job>.snjob~`), and an agent session that finds its job changed by someone else drops its undo steps, so an undo never goes back past someone else's change. | Rebuild |
| SV5 | Job files and the settings file are written to a temporary file and then renamed over the old one, so a crash or a full disk never leaves half a file. | Rebuild |
| SV6 | **Nesting from stock never invents a sheet**: with nothing in stock for the job and "use the job's sheet size when stock runs out" off (agents: `nest stock=only`), nothing is nested. When the minimum efficiency drops every stock sheet, the last nesting is kept. | Rebuild |
| SV7 | **Saving or booking takes the sheets first**, then stores the new remnants. When the sheets can't be taken (another job took the last one), no remnant is stored and the note says so; saving again retries. | Rebuild |
| SV8 | A **copy** of a job holds no stock and owns no remnants: it takes its own sheets when it's saved. | Rebuild |
| SV9 | **Every stock change records who made it and when** (STK-9). A change to the sheet stock (from the window, a save, an agent's `stock` or `stock-book`) records on the stock, and on each sheet or remnant it made or changed, the user and PC (as LB5: the Windows user and PC name, an agent's change marked as an agent) and the time on that PC's clock once the change holds the write lock (LB1), not when it began waiting. A change that takes sheets out of an item (a save or booking taking them, Reduce stock, an agent's `stock remove`) also records on it who took them, how many, and when. Items a change didn't touch keep what they had, and a change that changes nothing records nothing. A stock saved before this rule has none of it and reads as before. Shown in the hover text of a stock row's quantity ("Changed by sam on PROG-1, 9 Nov 09:20", then "Last taken: 1 by sam on PROG-1, 9 Nov 09:20"); agents' `stock list` gives each item `changed_by` and `changed_at` ("2026-11-09 09:20", this PC's local time). Own design; estimated. | Rebuild |
| SV10 | **A save that finds a sheet gone says who took it, and when** (STK-9). When a save, an agent's `stock-book` or a booked job's next change can't take a sheet the job is nested on, because another PC (or another job) took it after the job was nested, the note names who took it and when, instead of "stock not updated": "sam on PROG-1 took the last 3000 × 1500 (S-1) at 09:20, so nothing was taken from stock (remnants not stored): nest again (Do All) to use what's in stock now". With some left but too few: "only 1 of 3000 × 1500 (S-1) left, 2 needed: sam on PROG-1 took 2 at 09:20"; a remnant: "sam on PROG-1 took remnant S-1-1 (1200 × 400) at 09:20". The time reads "at 09:20" for today, "on 8 Nov 09:20" for another day. Nothing is taken and the job is still saved (SV7: saving again retries); an agent's reply begins "Not booked:" instead of "Booked:". A sheet with no record of who took it (a stock from before SV9) keeps the plain note ("Only 0 of sheet S-1 in stock, 1 needed"), and one deleted from stock altogether says it isn't in stock (STK-19). Own design; estimated. | Rebuild |
| SV11 | **Do All plans from the stock as it is now.** Every run reads the stock afresh just before it plans, so it never counts on sheets another PC took after the job was opened or last nested; what this job holds from its own last save still counts as free for it (SS12). Booking stays at save (SV7; booking a sheet the moment it's placed is STK-18), so two PCs can still both plan on the last sheet until one saves: the other's save then says who took it (SV10). Own design; estimated. | Rebuild |
| CP1 | **Compact** is refused on a remnant with an outline (window and agents' `compact`). | Rebuild |
| MM7 | Taking parts off a sheet clears that sheet's **hand-picked cut order**; removing a product clears every sheet's. The placements are renumbered, so the picks would land on other parts. | Rebuild |
| AG1 | An agent operation that **refuses** leaves the job exactly as it was, in the session too (nothing it set before refusing is saved by a later change). | Rebuild |
| AG2 | `edit-part` changes only the sizes it's given (at least 0.5 mm, as in the window); a **disc's** diameter sets both. | Rebuild |
| AG3 | MCP: a `job` that isn't the job file's path (text) is refused, never dropped (the call would otherwise land on the job already open). | Rebuild |
- Open jobs vs Saved jobs: Expert keeps every job in its database and has one active job. The rebuild keeps several jobs open in the window (Open jobs) and saves them to its own library (Saved jobs); the active job is the one shown.


## Removing a nesting from a job

| # | Rule |
|---|------|
| ND1 | Removing a nesting first sets its nested quantities back to 0, so its parts are pending again. Then it updates the job's **CAM last date**, and deletes the nesting in one transaction (retried until it succeeds). If that fails, the quantities are restored. With the propagate flag it removes the `NstRef` rows of `DIS_NEST_NEST_00000100` instead. |
| ND2 | After the database part, every file attached to the nesting is deleted with it. CNC files (kinds `CNCSHT`, `PRVCNC`, `CNCPAL`) go when the "delete CNC" flag is set. The other kinds go unless they are shared kinds (e.g. `CHPAX`, `BGIM`) and the flag for them is off. Nothing of a removed nesting stays behind. |
| ND3 | Copying a part's data into a job reads `DIS_SHPR_PPTT_00000200` rows for the part, work and material with **Thickness ≥ t − tol and ≤ t + tol** (the database tolerance) and ValType 100. For each row the value is DValue when ValType is odd, else CValue. Rows are re-inserted with ValType + 100. |

→ In the rebuild:
- A sheet that becomes empty is a removed nesting. Its cut lines / remnant lines and hand-cut contours go with it (ND2), so later sheets keep their own (`App::refresh`).
- Placed counts come from the sheets, so the parts are pending again at once (ND1).
- The job's **last nesting change** (`Job::cam_changed`, the CAM last date) is set when a nesting is removed, when Nest all makes new sheets, and when Add nesting adds one. It shows in the Job Summary ("Nestings changed") and the job tooltip in the Job Explorer (`jobtree::cam_date_label`: today / yesterday hh:mm UTC, else the date).
- ND3's thickness tolerance isn't used: one job here has one thickness.


## How other programs talk to Expert (integration surface)

What the original offers other programs:

| # | Finding |
|---|---------|
| XI1 | **No network/REST API and no public SDK.** Its interop library exports internal C++ classes (DXF settings, program cost data, technology tables, the cut-progress dialog) with no HTTP, pipe or socket use, and its external-execution service runs Expert's own commands in-process. |
| XI2 | **File-based import is the published interface**: XML schemas in `xsd\Importers` — DXF Import, Parts Import, Parametric Parts / Parametric Geometry Parts Import, ImportKits, Catalogue Import, Duct Import DB, SI Import — and `xsd\Nestings Without Job\lstxNestings.xsd`. |
| XI3 | DXF Import / Parts Import: root `<Import>` with any number of `<Product>`. Required attributes: Reference, Machine, Material, Thickness (decimal), Quantity (decimal), plus `Fich` (the drawing) for DXF Import or `DataBase` for Parts Import. Optional: Name, Priority (bool), Idf1–3, Idfx1–5, IsCat, Price. Children: optional `<Rotations>` (`<Normal>` and `<Symmetrics>`, each Rotation0/90/180/270/Others `value` bool), optional `<Fills value hollows>`, and `<AuxiliarData>` with `<Data DBField value>` rows that go into database fields. |
| XI4 | Other routes: the SQL Server database itself (see the Database tables section), a TRUMPF Oseon connector, Masterlink (ERP), DNC transfer (DNCLINK.CFG), and machine technology tables imported/exported per brand (`TblImpExp\…`, TblImpor.cfg / TblExpor.cfg). |

→ `xmlimport.rs` reads XI3 files:
- **Input:** Jobs → Import drawings accepts .xml, a dropped .xml works, and **`nestpro --import-xml=<file>`** lets an ERP or script hand over an order exactly as it would to Expert.
- **Jobs:** each material + thickness becomes a new job in the folder "XML import". The job is named after the file, plus material and thickness when there are several.
- **Parts:** a product with `Fich` loads that drawing, relative to the XML's folder, with every layer cut. Without `Fich`, the product is taken from the app's library by Reference, the Parts Import case.
- **Quantity and rotations:** Quantity is rounded to whole pieces. `<Rotations>` sets the allowed rotations (Normal → plain, Symmetrics → mirrored), and 90°/270° turn rotation on.
- **Names and material:** the reference uses the prefix/suffix rule (FN5). Material is matched by name, then by Settings → Materials name/code, then by containment (`xmlimport::material_index`). An unknown material keeps the job's and is reported.
- **Read but not used yet:** AuxiliarData (`XmlProduct::data`), Priority, Idf/Idfx and Fills. Machine is ignored (one machine per app).


### How Expert reads an import Product

| # | Rule |
|---|------|
| XI5 | Each attribute name is compared **with case**: Reference, Name, NBono, Machine, Priority, Quantity, Database, Fich, ParRef, Material, Thickness, UserData1…UserData8, IsDraft, File, Price. **Fich, ParRef and File** fill the same drawing field (File and Price only in import modes 2 and 4). |
| XI6 | **Quantity**, **Priority**, IsDraft and Price are read as a whole number, cut at the first non-digit ("40.7" → 40, "true" → 0). **Thickness** = the decimal value ÷ the import's unit factor. Material, Thickness and UserData only apply when the import passes them (mode > 0). UserData1…8 go to the product's user-data fields and mark it changed. |
| XI7 | Child elements are compared **without case**: Rotations → Normal / Symmetrics → Rotation0, Rotation90, Rotation180, Rotation270, Others. A Rotations element marks "rotations given". |
| XI8 | A boolean `value` is true only for **"true" (any case) or exactly "1"**; anything else is false. |
| XI9 | **Fills**: value false → not a filler. value true, hollows false → filler, and **Priority = 99**. value true, hollows true → filler that may also go in holes. |
| XI10 | Kits read Quantity as a whole number and Thickness as a decimal as well, but compare their attribute names without case. |

→ `xmlimport.rs` follows these rules:
- `lantek_atoi` for Quantity and Priority: whole pieces, truncated. A quantity of 0 (or not a number) is skipped with a note. Before, I rounded quantities and defaulted a missing one to 1.
- `lantek_bool` for rotation values. ParRef and File are accepted as Fich. Rotation element names ignore case (`find_tag`).
- UserData1…8 are kept (`XmlProduct::user_data`). Fills gives `filler`, with FILLER_PRIORITY = 99.
- Attribute names keep their case.
- Not used yet: the unit factor (files are taken as mm), priority and filler in nesting order, and user data in the part.


## Materials in Expert

| # | Rule |
|---|------|
| MA1 | The material master is the SQL table **`DIS_MMTT_MMTT_00000100`**: `MatRef` (key), `IntRef` (internal reference: `##MATEXP` + a four-digit material **catalogue number** + `##`, e.g. `##MATEXP0010##` for mild steel; **corrected by MTL2**: it is a number, not the name plus a suffix), `IsMain`, a norm reference (NormRef), costs and scrap price. "Does it exist" = a select on MatRef. |
| MA2 | Thickness ranges per material are **`DIS_MMTT_MMTT_00000200`**: MatRef, MinThickness, MaxThickness, PriceDate, Price, Descrip, RecID, read ordered by MinThickness. A reference that isn't valid or a missing material gives error 100. |
| MA3 | Looking up a material returns the cached material record when the name matches exactly, else 100. |
| MA4 | Products carry their own `DIS_MatRef` and `DIS_Thickness` (see PK2). Nestings get the job's material/thickness, and a nesting's material and thickness can be rewritten. |
| MA5 | Each machine's technology tables (laser / water / oxy-plasma lists, and the material–thickness parameter list) are keyed by material name. Renaming a material is propagated into them (and into the cost tables). |
| MA6 | **Aliases**: a two-name list (main material, alias). A name found as an alias gives its main material. A name that isn't in the list stays as it is. In the laser table the alias `< >` means "the material's default properties". |
| MA7 | Materials can also come from Lantek's cloud (GUID-keyed: list, insert, update, delete) and from XML files (load, save, import from config files). |

→ In the rebuild:
- The three materials of the app keep their name in programs and code (Settings → Materials), plus **Also called**, a comma-separated alias list per material (`AppSettings::material_aliases`, saved as `material_alias<k>`).
- `AppSettings::material_for(name)` applies MA6. The material's own name, its program name, its code or any alias (without case) gives that material; an unknown name gives none.
- It's used for XML imports (before the look-alike fallback), for opened machine programs (the `;MAT` header) and for file-name patterns (the material warning).
- The settings page has a "Try a name" box that shows which material a name means.
- The default aliases are common grades and shop names (estimated, editable).
- Not built: per-material thickness ranges and prices (MA2), norms, cloud and XML material files, and any material beyond the three.


## Window and taskbar icon

| # | Rule |
|---|------|
| IC1 | The window's icons come from a **loose icon file**, `iExpert.ico`, loaded by bare name rather than from the program's own resources. The install has `iExpert.ICO` next to the program. |
| IC2 | Two sizes: **16 × 16** is the small icon (title bar), **32 × 32** the big icon (taskbar, Alt+Tab). The small one is set first, then the big one, and each replaced icon is destroyed. |
| IC3 | If the file can't be loaded, nothing is set: the window keeps the default icon. |
| IC4 | The single-instance window class gets the same file icon at 16 × 16 (loaded from file, default size). |
| IC5 | The program file's own icon (Explorer, shortcuts) is one icon group with 32-bit images at 16, 32, 48, 64, 128 and 256. The manifest marks it DPI-aware. |

→ In the rebuild:
- The app's icon is `assets/icon.png`, a black tile with a white hexagon (eframe/egui's default icon, MIT OR Apache-2.0, credited in `assets/CREDITS.md`). It's not Lantek's picture.
- The Windows program file gets that icon at the original's sizes (`EXE_ICON_SIZES` = 16, 32, 48, 64, 128, 256) through `build.rs` (resized into a generated .ico plus a resource file).
- The running window gets it through `ViewportBuilder::with_icon` (`SMALL_ICON_PX` 16 and `BIG_ICON_PX` 32 are the original's two window sizes; one large image is given and Windows scales it to both).
- Like IC1/IC3, a replacement **`nestpro.ico`** (or `nestpro.png`) is looked for next to the program, then in the working folder (`iconfile.rs`, see SU1). An .ico gives its biggest readable image (PNG, or a 32-/24-bit bitmap with its AND mask). A missing or unreadable file means the built-in icon.


## Start-up

| # | Rule |
|---|------|
| SU1 | The first step makes the program's folder the working folder. That's why bare names like `iExpert.ico`, `AuxiData.cfg` and `Help\FCAM2000.CHM` resolve next to the program. |
| SU2 | Single copy: if another copy is already running, this one quits. |
| SU3 | The first command-line word is read. `/hide` = hide mode (the main window is shown hidden, SW_HIDE 0). A word ending in `.NXP` or `.nxp` (checked letter by letter, upper or lower, not mixed) is kept and opened once the main window is up. |
| SU4 | The main window is shown **maximized (SW_SHOWMAXIMIZED, 3)**, or hidden in hide mode, then updated. |
| SU5 | The window icons (IC1–IC3) are set as the last step, after the window is shown and the start-up file is opened. |
| SU6 | A splash window (font "Segoe UI" unless the style sets one) shows steps while loading. It's skipped in hide mode. |
| SU7 | Help files are `%sHelp\FCAM2000.CHM`, `TUTORIAL.CHM` and `INFOTECN.CHM` under the program folder. The profile name is `EXPERTPR`. |

→ In the rebuild (`startup.rs`):
- The window opens **maximized** (`SW_SHOWMAXIMIZED` 3), except when taking screenshots (a fixed size is needed).
- The first plain command-line word opens a file once the window is up. `.NXP` / `.nxp` opens as a Lantek nesting (`open_nesting_file`), and the app's own `.snjob` opens as a job (so double-clicking a job opens it).
- The replacement icon file is looked for next to the program first (SU1), then in the working folder.
- Not followed: `/hide` (`SW_HIDE` 0; a hidden egui window couldn't be reached), the single-copy check, the splash and the help files.

## Sheet stock and remnants (Linux continuation, 2026-11-04)

Sheet stock, remnants made from nested sheets, and how automatic nesting picks sheets from stock.

**Goal: the same features, our own storage.** Stock lives in the Library folder
(`Sheets/stock.json`), not in Lantek's SQL tables.

| # | Rule |
|---|---|
| ST1 | Sheets, nesting remnants (with an origin sheet) and user remnants are one stock list; automatic nesting skips locked items and items with less stock than it needs. |
| ST2 | Next sheet: largest / smallest real area (strictly better wins) or **lowest priority number** first; remnants first offers only remnants while any is usable. |
| ST3 | Do All fills one sheet at a time in that order, as fully as possible. A stock sheet used less than the minimum efficiency is dropped and the parts nested again (used/area < min; equal kept). |
| ST4 | Stock goes down by the sheets a nesting uses and comes back when the nesting goes; never below 0 unless allowed; capped at 32 500. **Rebuild:** settled when the job is saved. |
| ST5 | Remove down to 0 deletes the item; without sheet references, a new sheet of the same material/thickness/size adds to the existing one (from the manual). |
| ST6 | References: sheets by a base-36 counter; remnants `<origin>-<counter>`, ≤ 40 chars. |
| ST7 | Saving: each sheet's remnant line(s) cut the sheet; part-free pieces of at least the minimum area become remnants with the real outline and area, priority and user data of the parent sheet. Once per sheet. Quote sheets give remnants at 0, locked. |
| ST8 | Automatic line: nine shapes (codes 1, 7, 2, 3, 4, 5, 6, 9, 8 in dialog order — order of the last two inferred); boxes grown by the offset and stretched to the nesting origin for 1–5; Min X/Y on the first six; minimum width; 2 × tol past the edges; rounding except 8; bridges at the edge; refused when a line exists. |
| ST9 | Semi-automatic: the clicked part-free piece (exterior orthogonal: at the sheet edge, square cuts; irregular: gaps and empty holes too). |
| ST10 | Remnants loaded for nesting are turned so the corner with the most straight edge is at the start corner; rotate/mirror by hand only on an empty remnant sheet. |
| ST11 | Dialog order of the nine shapes confirmed (1, 7, 2, 3, 4, 5, 6, 9, 8); exact irregular switches rounding off; Lantek's defaults (vertical strip, offset 10, min width 100, Min X/Y 100, remnants on save **off**). |
| ST12 | Changed remnant lines remake a sheet's remnants on save, unless one is already used (then kept, with a warning). |
| ST13 | Do All: best sheet format (whole job per size; complete → less waste → higher worst efficiency) and best sheet combination (per sheet, other sizes tried under the minimum efficiency), within the time limit. |
| ST14 | Automatic sheet / remnant references from a format (off and empty by default): `[MAT]` material, `[UD1]`–`[UD3]` user data, `[SHT]` origin sheet, `[AUT]` next number, `[LNGx]` / `[WDTx]` / `[THKx]` sizes (x starting with 1 = ÷10, 2 = ÷100; a third character gives the decimals); an unknown `[xxx]` becomes `xxx`; the first 40 characters are kept. Without a format a remnant is `<origin>-<number>`. Counters skip the names Windows keeps for devices. Sheet formats use the same tokens (estimated). Observed (remnants); sheet tokens estimated. |
| ST15 | Automatic mark on remnants (cutting machines, off by default): text with `#300` reference, `#301` length, `#302` width, `#303` area, `#304` weight, `#200` sheet, `#400` material, `#401` thickness, `#104` date (`#N.d` = d decimals; anything unavailable prints `?`); keypad position 1–9 on the remnant's box with X/Y margins, or a search (spots across the remnant, then turned 90°, then smaller text down to the minimum height); engraved before the remnant line is cut. Observed; the default text, size and margins are estimated. |
| ST16 | **Picking in the Sheet stock window**: a click anywhere on a row picks it (the whole row is highlighted), a double-click puts it on the sheet shown. A sheet of another material or thickness than the job can't go on its sheets; the window says what each is and offers *Switch the job to … and use it*, which changes the job's material and thickness to the sheet's and then uses it — with parts already nested the usual question (keep or remove the nestings) comes first and the sheet isn't used yet. Rebuild. |
| ST17 | Using a stock sheet on a job with nothing nested yet makes sheet 1 from it and keeps it while empty (as *Add nesting* does), so parts can be dragged onto it. Rebuild. |
| ST18 | **The sheet you pick for a job is the one Nest all nests on.** Picking a size in the Sheet tab's size list ▾ (a stocked size or a standard one), typing a size in Length X / Width Y, or putting a whole stock sheet (not a remnant) on a sheet from the Sheet stock window makes that size the job's sheet. Every Nest all after that, at any level, nests only on that size: the stock's sheets of that size for this material and thickness first (locked ones never), then more of that size when they run out, if the Do All options allow it (with stock only, parts that don't fit stay unnested). Other sizes and remnants aren't used. *Any size in stock* at the top of the size list gives Nest all the whole stock again in its order, then the job's size; a new job starts that way. A remnant put on a sheet is a one-off for that sheet and doesn't change the job's sheet. The *Nest on* line and the Home tab say which it is. The choice is saved with the job and undone with Ctrl+Z. It is `pick` (`stock` / `size`) in the job's sheet: agents' `set-sheet` with `w` or `h` picks the size, `pick=stock` goes back, `sheet-add` / `sheet-change` with a whole stock sheet pick its size as the window does, and `info` → `nest_on` shows `picked`. Own design; estimated. |

Not yet built: remnant reference points for the post-processor (RF11, meaning estimated),
prenesting, "nesting equals sheet" (UI25).

### The stock through the agent operations (engine/stockflow.rs, engine/ops.rs)

The window and the agent operations (CLI, MCP) nest from stock, settle what a
job holds and store its remnants through the same code (`engine::stockflow`),
so the rules above hold for both. What is the agents' own:

| # | Rule |
|---|---|
| SA1 | Re-nesting never offers a job the remnants it cut itself (the refs in its sheets' `made`); what it holds counts as free again (SS12). Own rule. |
| SA2 | An agent's job takes nothing from stock until it is booked (`stock-book mode=book`, `JobFile.booked`); the window books a job on every save. A booked job is booked again after every change: sheets taken / given back, and new remnant lines' remnants stored even with "generate remnants when you save" off. Own design (the original takes a sheet when a nesting gets it, SS10). |
| SA3 | Release gives back every sheet the job holds and takes the remnants it stored out again while their quantity is still at least 1; one already used elsewhere stays. Remnants of sheets re-nested or deleted after booking stay in stock (as in the window). Own design. |
| SA4 | Sheets → New / Change by an agent: a stock item only while one is free (quantity + what the job holds − the job's sheets already of it), unless "use items with too little stock" is on; Change is refused when a placed part would be outside the new sheet (margin, a remnant's real shape) and drops the sheet's remnant line. From the manual (OK disabled at quantity 0). |
| SA5 | Undo in an agent session doesn't roll the shared stock back: the job keeps what is really held and is booked again against its restored sheets (remnants the undone change stored come out again); undoing a booking releases it. Own design. |
| SA6 | The Do All and Sheet Remnants options are the window's settings: agents read them (`stock mode=rules`) but don't change them. `nest stock=only` turns the job-size fallback off for that run, `stock=off` nests without stock. Own design. |

## ERP / MES integrations: Fulcrum (the rebuild's own design, not Lantek's)

Lantek's ERP links (Masterlink, Oseon) aren't described in the shipped manual, so
there is nothing to copy. These are the rules of the rebuild's own Fulcrum link,
from Fulcrum's public API (swagger v1). **Estimates** are marked.

| # | Rule | Kind |
|---|------|------|
| F2 | One part per (item, drawing); each job operation keeps its own quantity | design |
| F3 | Fractional quantities round up | design |
| F4 | Operation-specific DXF first, else the item's first DXF | Fulcrum API `isOperationSpecific` |
| F6 | DXF unit from `$INSUNITS`, else the setting (default inch) | DXF spec; default is an **estimate** (inch-based shops) |
| F7 | Layer roles from layer names | **estimate** |
| F9 | Placed parts fill orders earliest-due first; extras go to the last | design |
| F10 | Identical sheets → one plate with `count`; runtime = one sheet's estimated time | Fulcrum API `plates[].count`, `runtimeSeconds` |
| F13 | First send makes one work order (needs review by default), later sends replace its nest | Fulcrum API `POST /api/work-orders`, `PUT …/nesting-setup` |
| F14 | Sizes sent in inches by default (÷25.4, areas ÷645.16) | **estimate** (units aren't stated by the API: "as the nesting software reported it") |
| F18 | Work orders are made by a **planner in the ERP** by default: the app lists them, opens a work-order step as a job and stores its nest on that step; it never makes one | design (shop setting) |
| F19 | The other setting: the app makes the work order on the first send (F13) | design |
| F20 | **Work stations**: each ERP operation can be mapped to one of the app's machines. Once any is mapped, only work on mapped operations is listed (work orders and waiting work); until then everything is. The window can show one machine's work only | design |
| F21 | A job opened from the ERP gets the app machine of the equipment its step is scheduled on, else its operation's. The machine is app-wide: it becomes the active machine | design; app-wide machine is an **estimate** (FEATURES JOB-14) |
| F22 | Planner mode, a job imported before its work order existed: Send nest looks at the unfinished work orders on the job's operation and uses the one step holding its job operations. None → nothing is sent ("the planner makes it"); several → nothing is sent, they are named | Fulcrum API `nestable-jobs/list` `workOrderOperations`, `work-orders/{id}/operations/list` |
| F23 | Opening a work-order step takes the job operations the planner put on it (not the material group's); parts already nested on *another* step are left out | Fulcrum API `WorkOrderOperationDto.jobOperations` |



## Automatic lead-ins and micro-joints by contour class

Automatic leads and micro-joints from the machine's technology tables. Code: `leads.rs` (classes,
tables, placement, lead shapes, collision test), `joints.rs` (count, walk, fit, joint leads),
`ct1.rs` (reading the CT1), `cnc.rs` (`plan` puts them together). These rules weren't confirmed by
running the original.

In shop terms: every contour is sorted into a class (round hole, round part, rectangular hole,
rectangular part, slot hole, any other shape). The machine's technology file has one lead
setting per class and cutting quality. The setting gives the lead type and the distance (D),
angle (A) and radius (R) of the lead-in and lead-out. A keypad zone (1 = bottom left of the
contour's box) says where the lead goes. Micro-joints go one per D mm (100 by default) on
**every** contour, holes too. The count is rounded up to an even number, and each joint is
centred on the line or arc it lands on.

### Lead tables

| # | Rule |
|---|---|
| AL1 | **Quality 1 = B block** (B200–B271, B294 pierce radius, B295 min lead). **Quality q ≥ 2 = E block** at (q−2)·71: E000 = quality 2, E071 = quality 3, …, E710 = quality 12; E781/E792 + (q−2). Per class k: type B206+k / E+6+k, D/A/R in/out B214+6k… / E+14+6k…, tolerance B263+k / E+62+k, hole zone B271 / E+70. A quality with no E block copies quality 1. |
| AL2 | Overlap per class and quality: E814 + 8(q−1) + k (quality 1 = E814–E821). 99999.999 (±1.0) = use B156. Each record's tolerance ← B175. Type ≥ 100 → type mod 100. Min element ≥ \|B156\| + 4·tol. Hole zone −1 = outer zone. |
| AL3 | **Classes**: circle → round hole/part (rec 0/1); rectangle → rect hole/part (2/3); slot (2 straights + 2 half arcs) → slot hole (4); a slot-shaped **outer** contour and anything else → "other shape" (records 5, 6, 7 = contour 1, 2, 3, tried in that order; contour 1 at type 0 = no lead). |
| AL4 | **Zone point** on the contour box: 1 (x0,y0), 2 (xm,y0), 3 (x1,y0), 4 (x0,ym), 6 (x1,ym), 7 (x0,y1), 8 (xm,y1), 9 (x1,y1), 5 = a given point. Round contours: the point on the circle at 1 225°, 2 270°, 3 315°, 4 180°, 6 0°, 7 135°, 8 90°, 9 45° (snapped to B175 when B175 ≥ the angular tolerance). A round part with 2πr < E004 or r < E005 gets no lead. |
| AL5 | **Corner leads** (types 10/11): the nearest valid corner to the zone point. Corners count when both elements are longer than tol and are not tangent; there is **no side-length test**. Type 10: the corner points into the scrap and its angle is ≤ B201 + tol. Type 11: the angle on the scrap side is between B203 − tol and B202 + tol (a straight-through point only when B202 ≥ 180 − tol). |
| AL6 | **Element leads** (other types): at the middle of the element whose middle is nearest the zone point. A line must be ≥ E004. An arc must be ≥ E004 with radius ≥ E005 − tol, and longer than 4·tol + \|overlap\|. The point is (L − overlap)/2 from the element's start. Rectangles: type 10 → the zone's corner, type 12 → a long side, others → the middle of the nearest side. |
| AL7 | **Type 10 shape**: the lead-in is a straight of D_in that **extends the first side backwards** past the corner; the lead-out extends the last side forwards by D_out (corrects P4: not the bisector). Type 11: along the scrap-side bisector, turned by A (the bisector itself if A would leave the corner). |
| AL8 | **Types 1–9**: in-form = (t−1) div 3, out-form = (t−1) mod 3 (0 perpendicular, 1 inclined, 2 tangent arc). In the local frame (x = cutting direction, y = scrap side): perpendicular (0, D); inclined (−D/tan A, D) (A ≤ 0.001° → perpendicular); tangent arc with centre (0, R) and start height h = min(D, 2R), arriving along +x. The lead-out is the mirror image, from the overlap point. **No lead-out when D_out ≤ 2·tol** (form 2: also R_out ≤ 2·tol). |
| AL9 | **Types 12/13** (laser): 13 = a straight of D then an arc of R sweeping 90° − A that arrives along the cut; 12 = a straight of D parallel to the contour at height 2R, then a half circle down to the start. |
| AL10 | **Round holes, limit to centre** (on by default). Types 1–9: D ≥ r → D = r and A ≥ 90° (an inclined form turns perpendicular, so the pierce is at the centre); otherwise A ≥ 45°; R ≤ r/2. Type 13: R ≤ r/3, D ≤ min(r, √(r² − 2rR)). Type 12: all ≤ r/2. |
| AL11 | **Collision test B185**: 1 = shorten by B186 per step, never below max(2·tol, B295); 2 = move along the contour in steps of perimeter / B187; 3 = move, then shorten; 5/6 = move (+ mode 4) then move / mode 3; 7 = (mode 4) then shorten; 4 = rebuild from an undecoded table (skipped). |

### Micro-joints

| # | Rule |
|---|---|
| AM1 | **Count**: D = F150 (+4 per quality; default 100). None if \|D\| ≤ tol, or D > P with min 0. n = ceil(P / D), at least min, at most max (reaching max = *clamped*). An odd n > 1 that wasn't clamped becomes n + 1. **F120 is not read.** |
| AM2 | **(min, max) by size**: small when w ≤ F136 and h ≤ F137, large when w ≥ F138 and h ≥ F139, else medium. Pairs: holes F140/41, F144/45, F146/47; parts F142/43, F121/22, F148/49; a value < 0 means F121 / F122. |
| AM3 | **Which contours**: every closed contour, **holes too**. A CT1 fills only the default record. An edge type (3–6, 9–11) reaches every class. A corner type (1, 2, 7, 8) reaches only rectangular/slot/other holes and rectangular/other parts, and only their corners. Round contours get none from a corner type. |
| AM4 | **Where**: spacing s = P/n. The first joint is s/2 after the start (the lead-in point). Each element takes the k positions that fit, **centred on it** (first at (L − (k−1)s)/2). The leftover carries: rem = L − first − (k−1)s, next offset s − rem (or s/2 when rem − tol > D), never below F151. Elements ≤ 2·F151 + tol get none. The step on one element is ≥ F152. A joint exactly on an element end moves 2·S along it. On circles the angle snaps to F153. |
| AM5 | **One joint**: refused when its element is ≤ S long (corrects J5). On a line, within 2·tol + S of an end → the joint goes to that end and becomes a **corner joint**: types 3 → 2 and 4/5/6 → 1. A corner joint straddles the corner: S / (2·sin(θ/2)) either side. A plain gap past an end slides flush to it. Gaps with leads keep their centre ≥ 3·tol from the ends. |
| AM6 | **Joint leads**: lead-in from (+D_in/tan A, D_in) to the gap's end; lead-out from the gap's start to (−D_out/tan A, D_out), only when D_out > 2·tol. Both **lean away from the gap**. Types 2/3 are perpendicular (the angle is not used), 5 has only the lead-out inclined, 6 only the lead-in. An unset gap (99999.999) = B164. |

→ In the rebuild: parts without their own technology now get these machine leads (every sample shape: test `every_sample_shape_gets_its_machine_leads`). The Technology tab's Lead-in/Lead-outs → Automatic uses "Machine tables" by default; the part's own distances apply with the other lead types; picked starts and manual cuts still win. Micro-joints → Automatic sets one per F150 mm.

### Design choices and estimates (ours, not Lantek's)

- **Quality**: the contour's numbered quality (CQ1) picks its lead table: quality 1 the B block, 2–12 the E block (AL1).
- **Built-in tables** (no CT1): the laser template's B block, which PSTBUR01.CT1 and PSTMAZ03.CT1 share. It agrees with every PstAdi06.CT1 value we have: types 13/1/11/10/12/10/11/5, D 7/5, R 7.5/5.5, zone 1, corner 135°, element 5 mm, B156 0.5. B185 = 3, B186 = 0.1 and B187 = 10 come from PSTadi02.CT1. All 12 qualities use the same table. Joints: F150 = 100, F151–F153 = 0, F121/F122 = 0/32767, gap 5, lead 10 at 45°, lead-out 0, type 3 (the laser files have F000 = 1, a corner type).
- **Polylines**: classes come from `machine::recognise` and its tolerances. Elements break where the outline turns more than 20°, or where one segment is more than 2× the length of the next (a straight meeting a tangent arc). Round contours use the true circle tangent for their leads.
- The slot test "arc-centre distance < segment-midpoint distance" is **not applied** [U].
- Slot holes are placed like rectangles (type 12 on a long straight side) [estimate]. The two-digit zone passes are simplified to "on that side first, then anywhere". Zone 5 = the sheet origin.
- **Types 14–16** (special round-hole leads) are not decoded and are built as type 1. Type 11's turn direction (lead-in back against the cut, lead-out forward) is our reading.
- **Overlap**: cut along the contour past the start before the lead-out (MT7). Corner types 10/11 get no overlap (our choice; the overlap's place in the original is [U]).
- A corner type at a point that isn't a corner (picked starts, moved leads) is built perpendicular (type 1). When automatic placement finds nothing, the contour gets no lead (force placement is off: its switch isn't known).
- **Collision test**: against every part on the sheet whose box meets the lead, plus its own part's contours. A lead fails when it crosses a contour or when its free end lies in material. Mode 1 shortens to (1 − B186)ⁿ of the length [I]. Mode 2 rebuilds the lead at each try point. Without B185, or for picked leads, a lead that crosses its own part is still shortened (our safety rule).
- **Joint leads**: with no joint overlap test (F133 ≤ 0, the default and every CT1 we have), leads on outer contours stay within 0.8 × the part gap (`JOINT_LEAD_REACH`, our fallback). With F133 > 0 they shorten by F134 against neighbours instead. Every joint lead is also shortened if it would cross its own part.
- **Corner mode** joints use the evenly chosen convex corners (`corner_joint_spans`), now straddling each corner. Lantek keeps only the walk's points that land on corners. The secondary (corner) records, the V-block material records and the zone modes F129–F132 are not modelled. N8 (one more joint when the leftover > D) is not applied [U].
- A **fixed number** of joints (`micro_joints`, joint distance 0) stays on the outer contour only, spread by the same walk. That is our manual setting.
- The Technology tab's position pad sets the outer zone. Holes use the table's own hole zone when it has one. Size limits edited in Micro-joints → Settings go to the active machine's tables until the machine settings are applied again.

## True-shape strip nesting

Code: `app/src/bands.rs` (strips, drop, placing) and the engine in `app/src/nest.rs`
(`Engine`, `methods_for`, `spot_cost`, `nest`). It replaced the box engine (maxrects free list,
boxes sorted by area/side/perimeter).

In shop terms: every part, at every quarter turn (and mirror) it may take, is cut into thin
strips side by side along X. Each strip keeps the stretches of Y the part covers, from the outline
grown by half the part gap, with its holes left open. The sheet has the same strips, each keeping
the stretches of Y still free. A part is placed by lining its first strip up with a sheet strip and
dropping it down to the lowest height where everything it covers is free, trying the sheet strips
from the left. So parts settle against each other's real outlines, L-brackets and gussets
interlock, and small parts fall into the holes of big ones without any special pass.

| Rule | Status |
|------|--------|
| Strip coordinates are whole 0.1 mm units (`bands::UNITS` = 10). | [O] |
| A part's strips come from its outline grown by **gap ÷ 2** (gap = B056); outer contour outwards, holes inwards. | [O] |
| A hole stays open only when its area is over **B076**; smaller holes count as solid (`min_hole_area`). | [O] |
| Strip grid of width **B057** from the part's left edge; the last strip is cut back to the end (a sliver under 0.5 mm, B091, joins the strip before it). | [O] (sliver handling: estimate) |
| A straight edge running along Y (longer than **1.0 mm**) puts a strip boundary where it lies, when both pieces stay at least 0.5 mm wide. | [O] |
| Each strip holds the Y extent of the material between its two edges: one stretch per separate piece, so notches and holes that cross the strip leave gaps. Overlapping stretches are joined. | [O] |
| Neighbouring strips with the same stretches merge into one wider strip. | [O] |
| No stretch at all → the part's box. | [O] |
| Centre of gravity of a profile = area-weighted mean of its stretches. | [O] |
| Sheet strips: grid of B057 over the usable X range; the leftover width (range mod width) is the **first** strip when ≥ 0.5 mm (B596 = 1). Each strip starts with one free stretch. | [O] |
| **B078 on** (fit to the border): the usable area grows by gap ÷ 2 on every side, so a part's real edge reaches the margin (`MachineTech::fit_border`, default on as in every CT1 we have). | [O] |
| **Drop**: with the part's first strip on sheet strip S, every part stretch must lie inside a free stretch of each sheet strip it overlaps; Y only goes up, to the largest lift any stretch needs, until all fit. Result: the lowest Y at that X, exact to 0.1 mm. The part must not run past the last sheet strip. | [O] |
| Placing: the sheet strips are split at the part's strip edges (so the next part can go flush), then the part's stretches are cut out of the free stretches (a free stretch may split in two; touching ends stay free). Holes stay free, so later parts drop into them. | [O] |
| Spot along X: sheet strips from the cached start onward; the **first fit wins**, except methods **8–11** which try every strip and stop where the rest of the sheet is untouched; there a spot beats the best only if its cost is lower and either by ≥ **2 × strip width** or its second key is lower too. | [O] |
| The search for one orientation resumes where it last fitted (the sheet only fills up). | [O] |
| Cost rules (`spot_cost`): 0 cx; 1 cx then cy; 2 profile's own centre Y then X; 3 (cx + own centre Y)/2; 4 W − X size; 5 W − Y size; 6/8/12 cy; 7/9/13 (cx+cy)/2; 10/14/17 y + cy/W; 11/15/18 (y+x)/2 + ((cx+cy)/2)/L; 16 x + cx/L; 12–15 then cost/L + x. cx/cy = centre of gravity from the usable corner, x/y = the profile's start, L/W = sheet length/width. | [O] |
| Which part next: the size tier T = the largest order value left (B594 = 1: outline area); candidates = every orientation of every item with **0.9 < value/T < 1.1**. Lowest cost wins; costs within **0.2 × strip width** tie → lower second key, else the unmirrored one, else the earlier one. Items that no longer fit drop out; then the next smaller tier. | [O] (loop shape [I]) |
| Each **sheet** is filled once by every method of the level (A001), from what is left, and the best sheet is kept; then the next sheet. | [O] |
| Method k uses cost rule k (÷ 2 at the levels with both directions, so both directions get rules 0, 1, 2 …). | [I] (U3) |
| **Horizontal** = the whole problem turned on its side: the sheet's and parts' X and Y swap, the same engine runs, the result swaps back. | [O] |
| Strip width: CT1 **B057** (`MachineTech::band_width`); the level's A003 only when its A020 = 1, never in the system levels (`NestLevel::own_band` = false). | [O] |

Design choices / estimates (rebuild, not the original):

- **Strip width default 5 mm** (`nest::BAND_DEFAULT`) when the machine's CT1 has no B057. The
  standard laser CT1s say 13.896 mm, which the app uses when it reads such a CT1. The BLM CT1's
  value isn't known. Sample-job utilisation by width: 3 mm 77.6 %, 5 mm 77.0 %,
  13.9 mm 76.1 % (level 3); 3 mm takes about twice as long.
- **Square growth**: the outline is grown as a square around each point (each strip widened by gap ÷ 2
  on both sides, each stretch lengthened by gap ÷ 2 at both ends). This is never less than a round
  offset, so the gap always holds; on slanted edges it leaves up to about 0.4 × the gap of extra air.
  Lantek offsets the arcs exactly. Arcs are our outlines' polygons (no chord tolerance, U1).
- **Guard**: growth = gap ÷ 2 + **0.05 mm** (`BAND_GUARD`), plus, for round parts, the polygon's
  sagitta (the collision test uses the true circle). Stretches round outward (floor/ceil); Lantek
  truncates.
- Stretches are worked out as the projection onto Y of (shape ∩ strip): the Y extents of the edges
  inside the strip plus the cuts along its two side lines. That is the same as "one stretch per
  separate piece", without tracing pieces.
- **Best sheet** (per sheet, as in the original): a sheet taking every part left wins; between such
  sheets the higher efficiency (part area ÷ used length × usable width; U5 unknown in the
  original), then the shorter used length; otherwise the sheet carrying more part area, then more
  parts. The original first counts placed parts per priority set. We have no priority sets, and a
  plain "more parts" first made levels 3/5 worse than level 1 on a 306-part job (67.6 % vs 70.5 %).
- **Methods = cost rule + direction + item set**: at the module levels, the rules take turns using
  pairs/staggered discs and regular arrays as items (an item's profile is the union of its members'
  outlines). In the original those module switches come from the CT1 (B070–B072/B084, N1
  corrected); the rebuild keeps taking them from the level.
- Orientations: the quarter turns and mirrors each part allows (`turn_allowed`); turns that give
  identical strips are tried once (a rectangle is never placed at 180°). **Free angles** (B058 steps,
  45° extras, 360 − A; R3/R4) are out of scope: `Placed` stores quarter turns only.
- Speed-ups that don't change the result: drops already worked out are remembered per orientation
  until a part is placed over that X range, and a full scan stops once the cost at the bottom of the
  next strip can't beat the best (every cost rule only grows with X and Y).
- Not built: nesting from the right or from the top (B050/B051 off), different Y gap (B096/B097),
  methods 19–27 (zones), multi-torch and clamps, the separate holes-only pass (D4: not needed, holes
  stay open in normal nesting), alternate corners (A017). `compact_contours` still runs after
  *Nest all* (main.rs). It only slides parts along their real contours with the exact clash test,
  so it can't make parts overlap.
- **Corrected tests**: `nests_without_overlap` now checks the real contours (`clash_exact`), not
  boxes. Discs sit in each other's pockets, so their boxes come closer than the gap.
  `gussets_pair_up_turned_180_degrees_at_the_module_levels` now expects level 1 to interlock too
  (the strip engine turns gussets into each other without modules) and level 3 to be no longer than
  level 1, instead of strictly shorter.

Sample job (the app's demo parts, 105 on 2000 × 1000, gap 5, margin 5; release build, with the
contour compaction; `cargo test --release nesting_bench -- --ignored --nocapture`):

| Level | Before (box engine) | After (strips, 5 mm) |
|-------|---------------------|----------------------|
| 1 | 2 sheets (1 part on the 2nd), 68.0 % of the used area, 31 ms | **1 sheet**, 76.9 %, 51 ms |
| 3 | 1 sheet, 77.7 %, 202 ms | 1 sheet, 77.0 %, 309 ms |
| 5 | 1 sheet, 77.8 %, 372 ms | 1 sheet, 77.2 %, 440 ms |

A 306-part job (the demo parts × 3 on 3000 × 1500, gap 10): before 64.1 / 70.0 / 70.8 %, after
70.5 / 71.1 / 71.6 % (levels 1 / 3 / 5), 2 sheets each, up to 1.3 s at level 5. "Used area" counts
full sheets plus the last sheet up to its farthest part, in whichever direction leaves the bigger
remnant.

Tests: `bands::tests::{a_rectangle_is_one_strip_grown_by_half_the_gap,
a_hole_across_the_strip_stays_free_and_a_notch_is_open,
placing_splits_the_sheet_strips_and_takes_the_stretches_out}`,
`nest::band_tests::{a_small_l_tucks_into_a_big_ls_notch, a_small_part_drops_into_a_big_parts_hole,
no_two_parts_come_closer_than_the_gap_at_any_level, parts_reach_the_margin_but_never_cross_it,
horizontal_nesting_is_vertical_turned_on_its_side, methods_follow_the_level_and_cost_rules}`.

## Merged: strip nesting on stock sheets and remnants; leads and joints for agents (2026-11-04)

The strip nester and the automatic leads/joints were built on a branch that predated the sheet
stock, remnant outlines and the agent engine. When the two were merged, these rules joined them.
They are the rebuild's own design; the stock rules they follow are in "Sheet
stock and remnants" above.

| # | Rule | Kind |
|---|---|---|
| BN1 | Do All on several formats (the stock's order, then the job's sheet when allowed): sheet by sheet, the next format with sheets left is filled by every method of the level and the best fill kept; a format that takes nothing left is skipped for good without being used up. One unlimited format of the job's size = plain Do All. (`nest::nest_plan`, used by `nest::nest` and `engine::stockflow::nest_from_stock`) | design (follows SS rules: the next sheet is taken in stock order) |
| BN2 | A remnant is filled inside its real shape: its outline is moved in by the margin (less gap ÷ 2 + 0.05 mm when B078 lets parts reach the border, the same as the rectangle's usable area), and every strip keeps only the Y stretches its whole width has inside that shape. Horizontal methods turn the shape with the problem. | design (estimate) |
| BN3 | A part on a remnant fits when its box lies in the remnant's free rectangles **or** its real outline lies inside the remnant (within 0.01 mm² of overlap), at least the margin from its edge — so the window's checks accept what the strip nester places in a notch. Copy still looks for a spot by box. | design |
| BN4 | Automatic micro-joints (window and agents): the part's `joint_every` becomes the machine's maximum distance for its quality (F150 per quality; the part's own quality, else quality 3, CQ1's last fallback, without looking at the size bands: estimate; kept when already set; agents may give `every=`), the fixed count goes to 0, and a part without a joint gap takes the machine's. The size-class fewest/most are the machine's (applied in the cut plan). Delete clears `joint_every` too, so automatic joints really go. | AM1–AM2; delete clearing the spacing is a fix found in the merge |
| BN5 | Automatic leads (window and agents) default to the machine's tables by contour class; Delete sets the type back to inclined (type 5) with no length, so table leads don't come back. (Agents: `leads type=`, default `machine`.) | AL1–AL7 |
| BN6 | "Search for the best sheet combination" (ST13) runs on the strip nester: for each sheet, when the next format in stock order would be used less than the minimum efficiency (part area ÷ sheet area), every other format with sheets left is filled too; the first reaching the minimum is taken, else the one placing the most part area when that is more. "Look for the best sheet format" nests the whole job on each stocked size (plus the job's sheet when stock may run out) and keeps the best by ST13's order; both stop at the maximum time. One function for the window and agents' `nest`. | ST13 |
| BN7 | Remaking changed remnants (ST12) is shared by the window's save and agents' `stock-book` / booked jobs: a fingerprint of the sheet's remnant lines is kept with its remnants (`made_from`); a sheet whose stored remnants carry no fingerprint (jobs saved before it existed) counts as unchanged, so opening and saving an old job doesn't remake its remnants. The note "changed, but already in use — kept" goes into the save's status note. | ST12; the no-fingerprint case is design |
| BN8 | Naming new remnants: the window asks for references when "Prompt for references" is on (suggested `<origin>-<n>`, at most 40 characters, not empty, not in stock, not used twice; Cancel saves without storing remnants). Agents can't be asked: `stock-book mode=book refs=` gives them in the order `status` lists (`would_store_remnants`, with the suggested references); without refs they are numbered. | UI14 (window); agents: design |
| BN9 | Reference formats and remnant marks (ST14, ST15) are shared by the window and the agent operations: suggested and stored references (`stock-book`), the single-sheet `program` and `export` all go through `engine::stockflow` (`next_ref`, `remnant_marks`), so a program has the same marks whichever made it. The material name for `[MAT]` / `#400` comes from the settings in both. |

## Material and thickness sections of the machine file

In shop terms: after the machine's own values, the technology file (CT1) can hold one
section per material and thickness range. When the job's material and thickness fall in a
section, that section's lead table, micro-joint table, joint width, pierce radius, part gap,
kerf, margins and so on take over from the machine's values, so 1 mm sheet gets 2 mm leads
and a 0.3 mm joint gap while 15 mm plate gets 13 mm leads and a 0.7 mm gap. A thickness
that falls between two ranges keeps the machine's own values. `ct1.rs` (`parse_file`,
`pick_section`, `codes_with_section`, `for_spec`).

| ID | Rule (our words) | Observed / estimated |
|---|---|---|
| MTL1 | The file is read top to bottom. Lines outside the sections are the machine's own values. Every Z, Y, X or V line belongs to the section being read, and a `Z999` line closes it; only then is the section tested against the job. A section left open at the end of the file is never used. Sections come after the machine's values, so a fitting one overrides them. | Observed |
| MTL2 | A section names its material by the material's **internal reference**: `##MATEXP` + a four-digit catalogue number + `##` (0010 mild steel, 0100 stainless steel, 0200 pure aluminium, 0210 aluminium-magnesium, 0300 galvanised steel, 0510 copper, 0590 brass), or by a plain material name. A reference with fewer digits (`##MATEXP10##`) is padded to four. A material with no internal reference is looked up by its name. Names compare exactly, upper/lower case included. A section named `*` fits every material. | Observed (padding of the job's reference); padding the file's names too is our estimate |
| MTL3 | Thickness: a section fits when its "from" is negative (any thickness), or when from − 0.005 ≤ t ≤ to + 0.005. Both ends count; there is no nearest-range search, so a thickness in a gap between two ranges (1.65 mm between 0.4–1.6 and 1.7–2.1) fits none. | Observed |
| MTL4 | The other keys: the gas number fits when equal or when the job gives none (0); lens, power, nozzle, pressure, abrasive flow and mixing-tube numbers fit within 0.01 or when the job's is 0.001 or less (not given); the abrasive when the job's is `*`, empty or equal; the configuration alias when the job's is empty or equal. | Observed |
| MTL5 | The first fitting section in file order wins. Exception, when the machine marks default technologies (A093 on) and the job gives only material and thickness: a fitting section not marked default (Z350 off) is only a fallback. The first such one is kept while the search goes on, and a later fitting section marked default wins. | Observed |
| MTL6 | No fitting section: the machine's own values stand, untouched. | Observed |
| MTL7 | A fitting section replaces the **automatic lead tables of every quality** and the **micro-joint table** as a whole. What the section leaves out takes the built-in table values (our laser lead table; joint distance 100 mm), not the machine's. A quality with no lead table of its own copies quality 1. | Observed (replacement); the built-in fallbacks are inferred |
| MTL8 | Code mapping. **Y n = E n** (lead tables of qualities 2–12 and their extras, same numbers). Quality 1's lead table is in Z: **Z n = B (n + 146)** for n = 54–124 (B200–B270), **Z132 = B271** (hole zone), **Z305 = B294** (pierce radius), **Z306 = B295** (shortest lead). **X n = F n** for 0–118 and 129–197 (joint records, size classes, distances by quality), but **X122 = F121** (fewest joints), **X123 = F122** (most) and **X125–X128 = F123–F126**; X119/X120 are not read. | Observed |
| MTL9 | Single values a section replaces: Z003 cutting speed (A058), Z004 **cutter radius** = half the kerf (B002), Z005 sheet margin (A256, and it fills the other three first), Z007/Z008/Z009 top/left/right margins (A257–A259), Z130 remnant margin (A260), Z006 **part gap** (B056), Z335/Z336 a different Y gap (B096/B097), Z026–Z053 = B150–B177 (manual leads, overlap, **loop radius** Z033 = B157, joint leads and **joint width** Z034–Z040 = B158–B164, angle tolerances), Z125 corner rounding (B178), Z126–Z129 kill sharp edges (B179–B182), Z131 default **pierce radius** (B352), Z301 loop limit angle (B292). A value the section doesn't give keeps the machine's. | Observed (mapping); keeping the machine's value for a missing line is our estimate |
| MTL10 | Z010/Z011/Z012 triples (one per size band: from, to, technology index) pick the cut quality of a contour by its size: the contour's equivalent radius (the radius of a circle of the same area) against the bands, the first band with from ≤ r < to, none when the first band starts above r. The index becomes a quality by TT8. The app applies it to contours of parts that leave the quality to the machine (no size ranges and no quality of their own, CQ1); the quality then drives the lead table, the kerf table, the speed and the LC5's NEXT_L (BL22). | Observed (bands, index → quality); radius rather than diameter, outsides included, and which parts it applies to are our estimates |
| MTL11 | Conditional groups (Z308 on → Z309–Z312 and X124; Z313 on → Z314–Z323; Z337 on → Z338; X125–X128 only when ≥ 0) carry values the app doesn't use; they are not carried over. | Observed (rule); not used |
| MTL12 | V lines fill the per-class micro-joint records inside a section. No known file has them; not carried over (FEATURES TEC-14). | Observed (rule); not used |

→ In the rebuild:
- **Material keys** (MTL2): Settings → Materials → *Machine table*, one per material: mild
  steel `0010`, stainless `0100`, aluminium `0210` (the aluminium-magnesium grades its
  aliases name; pure aluminium is `0200`). Our choice, editable. A bare number is our
  shorthand for the catalogue reference; empty = the material's name in programs.
- **Gas, lens, nozzle and the other keys** (MTL4): our jobs carry none, so they count as
  not given and only material and thickness decide. The machine's gas setting is a BLM
  output number, not the file's gas number, so it is not used as a key. Design choice
  (FEATURES TEC-10).
- **Where the section is used**: the cut plan (automatic leads by contour class and
  quality, joint distances, size classes), Technology → Lead-ins → Machine, Micro-joints →
  Automatic and Settings → Machine values, and the agents' `leads mode=machine` /
  `joints mode=auto`. The window's joint size classes (Micro-joints → Settings) still win
  over the file's (TEC-9).
- **Joint gap**: with a fitting section, Automatic micro-joints give every part that
  section's joint gap (the plate's table decides). Without one, a part keeps its own gap as
  before. Our choice.
- **Part gap and margins** (Z006, Z005–Z009) are **never applied by themselves**: the sheet's
  gap and margin stay the user's. The Sheet tab shows the machine's values for the current
  material and thickness with a one-click *Use* button, and agents get them from
  `machine-tech` and take them with `set-sheet machine_spacing=true`. The sheet has one
  margin, so it takes the widest of the machine's four. Design choice (FEATURES TEC-11).
- **Kerf** (Z004 / B002) and the section's corrector numbers (Z024 / Z025 = B124 / B125)
  drive the kerf compensation (see "Kerf compensation" below). The section's **cutting
  speed** (Z003) is the "active" speed of the time calculation ("Machining time and speeds");
  **pierce radius, Y gap** are read and shown only (`machine-tech`).
- The applied section shows on the Sheet tab (under the material) and in the Part view's
  status line: "Machine table 0010 mild steel 2.2–3.2 mm", or "global".

## Kerf compensation

In shop terms: the beam cuts a strip one kerf wide, so to get a part to size its centre must
run half a kerf (the **cutter radius**) on the scrap side: outside a part's outline, inside a
hole. Either the **control** does it (the program keeps the drawn outline and writes G41/G42
plus a **corrector number**, the CNC's tool-table slot that holds the radius) or the program
carries the **offset path** itself (**system** compensation, no G41/G42). A machine may use
either or both. `kerf.rs` (`side_for`, `resolve`, `offset_path`, `compensate`), used by the cut
plan (`cnc::plan_manual`), so the drawing, the simulation, the times and every program follow
the same path.

| ID | Rule (our words) | Observed / estimated |
|---|---|---|
| KC1 | Two independent switches in the machine file: **system** compensation (B116, default off) and **control** compensation (B117, default on). Each cut contour gets a control side + corrector when control is on, and the offset path when system is on; both on = both. | Observed |
| KC2 | **The side is the scrap side, from the contour's real cutting direction on the sheet.** Outside counter-clockwise → right (G42), clockwise → left (G41). Hole counter-clockwise → left (G41), clockwise → right (G42). An open path (marking) → off. Mirroring a part or reversing a contour flips its direction and therefore its side. | Observed |
| KC3 | **Corrector numbers**: holes B124 (default 1), outsides B125 (default 2); a material section's Z024 / Z025 replace them. Marking → no compensation. | Observed |
| KC4 | A compensation word is written only when the needed side or corrector changes; there is no cancel between two contours that need the same. Where the cancel (G40) goes is a post choice. | Observed (CAM side); the post's placement is ours, see below |
| KC5 | **Quality-dependent table** (B748 on): quality q (1–12; any other number uses 1) picks its row B700+4(q−1) … B703+4(q−1): hole corrector, outside corrector, hole radius, outside radius. The mode for normal contours (B749) decides: 0 control with the row's corrector, 1 system with the row's radius, 2 both. A **negative radius flips the side** (the beam runs that far on the part's side). B750 / B751 are the same for common-cut contours. | Observed (mode 2 flipping the control side too is our estimate) |
| KC6 | The **radius** of the offset: the quality row's (when the table is on and the row has one), else the machine's cutter radius **B002**, or the fitting material section's **Z004**. No offset when the radius is zero or the side is off. | Observed (order); our fallback when the file has no B002: 0.15 mm (0.3 mm kerf) |
| KC7 | Common cut can switch the shared contours to system compensation (B191). Read only: the app has no common cut. | Observed (read) |
| KC8 | System offset: every element moves r sideways; a round hole whose radius is not more than r (+ tolerance) has **no path** and is not cut. | Observed |
| KC9 | Corners: an outside corner gets an **arc of radius r round the drawn corner**; an inside corner is **trimmed** where the two offsets cross. | Observed |
| KC10 | **Corner mode A017** (units digit): **0 round** (keep the corner arc), **1 chamfer** (the line touching the arc at its middle, cut against the neighbours: two corners), **2 or more sharp** (the offsets run on until they meet). The tens digit (remove self-crossings) is not built. | Observed |
| KC11 | Small features: an inside arc or element smaller than r is left out and its neighbours joined; the two walls of a **slot narrower than the kerf** are left out too, so the beam passes the slot by. Points closer than the tolerance are merged. | Observed |
| KC12 | Micro-joint gaps keep their ends square to the edge: each end moves sideways with its edge, so a gap on a straight edge keeps its length (what offsetting each piece between gaps on its own gives). | Observed (pieces); doing it on one closed offset loop is ours, the same on straight edges |
| KC13 | **Leads** (B752): **0** the lead-in and lead-out are offset with the contour and joined to it by the corner rules (with a straight lead square to an edge the cut then ends 2r before its start, where the lead-out's offset meets the contour's); **1** leads at a corner, **2** leads on an element, **3** both: the lead is moved unchanged by how far its end moved, and that corner is made sharp. | Observed |
| KC14 | **Joint leads** (B753): 0 offset with their piece, 1 moved unchanged. | Observed |
| KC15 | The **pierce point** moves with the lead: the rapid goes to the first point of the offset lead-in. | Observed |
| KC16 | *Modify compensation* per contour (one contour, all holes, all outsides, all contours; left / right / off) is not built (FEATURES MAC-13). | Observed (not built) |
| KC17 | With control compensation the word goes on the **first lead-in move**, so the CNC ramps the offset in along the lead-in. | Observed |
| KC18 | **Our check** (the original's test for elements left uncut by compensation is not known): contours system compensation can't cut — holes no bigger than the kerf, contours narrower than the kerf everywhere — are left out of the plan (no pierce, no time) and listed; narrow slots and dropped inside arcs are listed as not cut fully. Verify Machining and the agents' `verify` / `compensation` show them. | Estimate (own design) |

→ In the rebuild:
- **The side fix** (KC2): the BLM LC5 post used to write G41 on every hole and G42 on every
  outside. That was right only for contours running counter-clockwise; a mirrored part or a
  clockwise drawing got the kerf on the part's side (parts a kerf small, holes a kerf big). Both
  posts now take the side from the contour's direction on the sheet.
- **Generic G-code post**: `G41 D1` / `G42 D2` on the first lead-in move when the side or
  corrector changes; `G40` on the rapid to whatever needs none (a contour without control
  compensation, marking, a free cut line) and on the way home. A contour without a lead-in gets
  the word on a block of its own. A post that turns one axis round (A016 × A017 < 0) mirrors the
  path, so G41 and G42 swap, and so do G2 and G3 (KC19). Our choices.
- **BLM LC5 post**: its format is kept (G41/G42 on the first lead-in move, no corrector word);
  a contour without a lead-in gets it on a short move before `PROFILE` (BL24); with system
  compensation only, no word and the offset path.
- **The job's choice**: Machining → Compensation → *Kerf* (Drawing and Nesting modules):
  *CN compensation* and *System compensation* toggles for the job, each defaulting to the machine
  file's; *Use the machine's settings* clears them. They win over the machine's switches and the
  quality table's mode (our rule). Stored in the job (`spec.kerf`); agents: `compensation`.
- **Kerf elsewhere**: remnant lines, cut-off lines and remnant shapes use twice the machine's
  cutter radius for the sheet's material and thickness (`kerf::kerf_mm`), else 0.3 mm; the
  simulation shows that radius and draws cuts at least that wide.
- **Where it shows**: the Part view draws the offset path (a thin cyan line beside the part's
  edge) and its info card has "Kerf 0.30 mm · control (G41/G42, D1/D2)" or "… · system offset
  (round corners)"; the sheet's cut path view draws the offset path too; programs carry a
  `( KERF … )` comment.

## Machining time and speeds

How long a sheet takes, and how fast each contour is cut, the way the original works it out.
Every machine has, next to its machine files, a **technology table** (`.TT1`: cutting speed
and pierce time per material, thickness, gas and cut quality, one marking speed) and a
**time file** (`.TM1`: where speeds come from, how pierces are timed, accelerations, head
moves). The time is the machine program replayed move by move. `techtable.rs` (table, record
choice, speeds), `cuttime.rs` (time file, moves, corners), `cnc.rs` (`Timing`, `beam_runs`,
the plan's time by kind), `sheetsim.rs` (the simulation uses the same times). Agents:
`machine-tech` → `speeds`; `info` / `part-info` → `times_s` per sheet.

### Finding the files and the record

| ID | Rule (our words) | Observed / estimated |
|---|---|---|
| TT1 | The machine's post file (CF) names the table (A019) and the time file (A026); `xxxxxxxx.xxx` means none. The names are looked for beside the CF file, then beside the technology file. Without a CF file the files have the technology file's name with `TT` / `TM` and its own number (`PSTADI02.CT1` → `.TT1`, `.TM1`). A missing table leaves no table speeds; a missing time file leaves the defaults of CTM16. | Observed (codes, naming in the shipped files); the folders searched are our estimate |
| TT2 | Table layout: `W000 3` opens the laser's block (other machine types' blocks are skipped). Each `A000` (material reference, as MTL2) starts a record running to the next one: A001/A002 thickness from/to (A001 sets both), A003 lens, A004 power, A005 gas number, A006/A007 bevel angle, A008 = 1 the default record for its material and thickness, A009 configuration alias; then B, C and D lines. | Observed |
| TT3 | Per cut quality q = 1…12 the record's fields are D(50·(q−1) + n): n = 7 **cutting speed** (mm/min), n = 10 **pierce time** (s), n = 11 a second pierce-time term, n = 15 the pierce types used. | Observed (speed, pierce terms); the other fields are unknown |
| TT4 | The CF file's `Tnnn "label" ON/OFF` lines say which fields the machine uses: T412 the second pierce term, T408 the pierce types. Without a CF file every field counts. | Estimated (that these flags gate the values) |
| TT5 | Pierce time from the table for quality q = its pierce time + its second term + for each pierce type k = 1…5 present in the pierce-type field (its decimal digits: ten-thousands = type 1 … units = type 5, non-zero = present) the time B(100·(k−1)+1). | Observed |
| TT6 | Cutting speed = the quality's field; marking speed = C009, the same for every quality. An empty field (≤ 0.001) means no speed. | Observed |
| TT7 | Corner feed reductions (L601–L612: quality, angle range, acceleration, time) apply only when the time file asks for them (CTM8 mode 3). Not built (FEATURES MAC-22). | Observed (format) |
| TT8 | A technology index becomes a cut quality: 1 → 2, 2 → 3, 10–12 → 4–6, 100–102 → 7–9, 110–112 → 10–12, anything else → 1. | Observed |
| TT9 | The record for a job: the first, in file order, whose material, thickness (from − 0.005 ≤ t ≤ to + 0.005, never the nearest range), gas (equal, or the record's is 0 = any), lens, power, angle and alias fit; else the **last** default record (A008) for the material and thickness; else none. Our jobs carry the machine's gas (Settings → Machines) and no lens, power, angle or alias, which fit any record. | Observed (order and fallbacks); the keys our jobs don't carry are our choice (like TEC-10) |
| TT10 | No record, or a quality without a speed: the original would time those cuts as almost instant; the app uses the machine's default cutting speed instead (A058, or the section's Z003), and says so on the times card. A machine speed of 0 counts as none. | Our choice (FEATURES MAC-20) |

### The time file and the calculation

| ID | Rule (our words) | Observed / estimated |
|---|---|---|
| CTM1 | The time is the program replayed: per contour the head goes down, then each beam run (pierce, cut, beam off) with jumps over micro-joints between runs, then the head goes up; markings before their part; rapids between. | Observed (replay, instruction kinds); keeping the head down over micro-joints is our estimate |
| CTM2 | A move's time: each axis ramps up to its share of the feed (capped at the axis's top speed), cruises and ramps down, entering at the previous move's exit speed; a move too short to reach full speed peaks and comes straight down. The slowest axis sets the time. When the exit speed can't be reached, the next move starts slower. The head also starts braking early enough to meet a slower corner, a move the axes' top speeds hold slower, or the end of the run further on. | Observed (ramps, entry/exit); the braking look-ahead is ours |
| CTM3 | Rapids: at the machine's rapid speed (A051, 40 000 mm/min in the shipped template), from rest to rest, with the rapid accelerations. With A001 ON (the default) they run straight; OFF, each axis on its own: a 45° move over the shorter axis, then straight on. | Observed (A051 from the file's own description) |
| CTM4 | Cuts: straight moves use the cut accelerations (P), arcs their own (Q, else P). | Observed; our contours are polylines, so every move uses P (FEATURES MAC-21) |
| CTM5 | Corners: the last move of a run stops; where the path turns by α the head slows to v90 · √((s/(1−s)) / (sin 45°/(1−sin 45°))) with s = sin(90° − α/2), never above the feed (v90 = D168, the speed through a 90° corner). A vertex turning by 20° or less counts as part of a curve and keeps the feed. A negative D168 means the table's value, which isn't decoded: no slow-down then. | Observed (formula, v90); the 20° curve limit and the negative D168 are our estimates |
| CTM6 | Acceleration records `O`/`P`/`Q nnn` = thickness from, to, acceleration X, Y, deceleration X, Y (mm/s²), top speed X, Y (mm/min); the first whose range holds the plate's thickness. None = no limit; a value of 0 or less limits nothing either. | Observed (records, none = no limit); 0 = no limit is our estimate |
| CTM7 | S001 > 0 asks for jerk-limited (S-curve) moves. Timed with plain ramps here (FEATURES MAC-21). | Observed (switch); not built |
| CTM8 | The program writes each contour's and each marking's speed when it differs from the last one. Cutting speed source D018: 1 = the machine's default speed (A058, or the section's Z003) for every contour; 2 = the table's speed for the contour's quality; 3 = the table with corner reductions (TT7, not built: as 2). | Observed |
| CTM9 | Marking speed source D017: 1 = the machine's marking speed (A063, the cutting head's); else the table's (TT6). | Observed (switch); A063 for laser marking is our estimate |
| CTM10 | Pierce time D014: 1 = D016 seconds; 2 = D015 seconds per mm of plate; 3 = the table's (TT5). Every beam-on counts, joint re-pierces included, and the sheet's pierce count is the same count. | Observed (modes); counting every re-pierce is our estimate |
| CTM11 | Beam off: D001 seconds when D169 = 1 (a laser's; the table's value is a waterjet's). | Observed |
| CTM12 | Head: H006 seconds up, H007 down, once each per contour and per marking. | Observed (codes); once per marking is our estimate |
| CTM13 | Technology-change and quality-change times (D002–D013, a 12 × 12 table) and start/end-of-program times (L, M) are not counted (all 0 in the shipped template; FEATURES MAC-23). | Observed (codes); not built |
| CTM14 | Kinds of time kept: cutting, marking, rapids, pierces, head up/down and beam off; the sheet's time is their sum. Factors N001 (rapids), N002 (cutting), N007 (marking), N008 (pierces) scale them. | Observed (factors exist); which factor is which kind is our estimate |
| CTM15 | The times card shows the machine time split into those kinds, where the speeds came from (table record, machine default speed, or estimate) and which time file applies. | Ours |
| CTM16 | No time file: speeds are the machine's default ones (A058/Z003, marking A063), no accelerations (moves take length ÷ speed; an acceleration of "no limit", 99 999.999, changes the speed at once), 1 s per mm of plate per pierce, no head times. No machine speed either: our typical laser speeds (estimate). | Observed (the original's defaults); the last fallback is ours |

## ByStar Fiber laser jobs (.bvc): the format and how one opens

A ByStar Fiber laser runs nests sent to it as `.bvc` files, written by the machine maker's CAM or
by this app. `bvc.rs` reads one as it is, with no code shared with any program writer (it is also
the read-back a written `.bvc` is checked with before it goes out, spec #122), and
`engine::imports::bvc_job` opens it as an editable job: Open CNC program in the window,
`import-program` for agents.

| ID | Rule (our words) | Observed / estimated |
|---|---|---|
| BV1 | A `.bvc` is a zip of a job file (`JobInfo.xml`), one cut program (`<nest>.ccp`) per nest, and usually a preview picture per nest. Entries are found by their name without folders, ignoring case. A file that isn't a zip, or a zip without a job file, isn't a `.bvc` and is refused with that reason. Cut programs the job file doesn't list are named in the note and not opened. | Observed |
| BV2 | The job file's root gives the job's name, its material (free text), thickness, unit (`MeasurementSystem`: `Metric` = mm, `English` = inch) and cutting gas. The material is matched as a machine program header's is (Settings → Materials first, then by its look); the thickness becomes the job's. The job keeps no gas of its own yet, so the gas is only named in the note (JOB-74). | Observed (fields); the gas only in the note is ours |
| BV3 | The job file lists the parts (`PartInfo`: id, name, ordered quantity). A part's shape and which of its contours are cut or engraved are only in the cut programs, so the shape is read from there. A placeholder part with no shape (cut-off subprograms point at it) doesn't become a part. Two parts with one id are refused. | Observed |
| BV4 | The job file lists the sheets used (`MaterialInfo`: id, size). A sheet's size comes from its nest's cut program (`ApplicationFlat`), else from this list. | Observed |
| BV5 | The job file lists the nests (`PlanInfo`): the cut program's name, how many sheets are cut with it (`Multiplicity`), its sheet, and which subprogram is which part (`PlanPartInfo`). A nest naming a sheet or a part the job file doesn't list is refused, as is a nest whose cut program isn't in the zip, and a part the job file says is cut by a subprogram its nest's cut program doesn't have (the message names the part and the subprogram). A nest run less than once opens as one sheet (JOB-74). | Observed; the less-than-once case estimated |
| BV6 | A cut program holds one subprogram per part (and per cut-off) and a main program that places them: each call turns its subprogram about the subprogram's own origin by its rotation (degrees, counter-clockwise, any angle) and then moves it to its X, Y. A call to a subprogram that isn't there is refused (names match ignoring case); two subprograms with one name are refused. A tube program is refused: only flat sheets open. | Observed |
| BV7 | In a subprogram, each contour is the moves between `BeginCutOut` and `EndCutOut`; that is its shape. The moves of a lead-in (`BeginLeadIn`…`EndLeadIn`, or without brackets: after the pierce and before the cut-out) and of a lead-out (`BeginLeadOut`…`EndLeadOut`, after the cut-out as the machine's CAM writes it or inside it before `EndCutOut` as the original's post does, BV34; or without brackets, moves right after the cut-out) are its leads and never part of the shape. Arcs are an end point and a signed sweep in degrees, counter-clockwise positive; the centre lies on the chord's normal at (chord ÷ 2) ÷ tan(sweep ÷ 2) from its middle. An arc of almost no sweep is a straight line; one that ends where it starts has no centre and is skipped and counted in the note. | Observed (commands, arcs); unbracketed leads estimated |
| BV8 | Every length in an inch file (`English`, or `Imperial` as the original's post labels it, BV25) is inches and is opened in mm (× 25.4): moves, placements, sheet sizes and thickness, so an inch job opens at its real size. Paths are the programmed cut (the machine does the kerf compensation), so they are the part's shape as it is. The file carries no cutting values (power, speed, gas pressure). | Observed |
| BV9 | Each contour's operation mode (the machine's technology column, set by `SetOperationMode`) becomes that contour's cut quality, the post's quality-to-mode rule with its default switch (BV32, B015 off) taken the other way: mode 0 (cut) → quality 1 (the lowest of the qualities that give 0), 1 (pulse) → 2, 3–7 (process macros 1–5) → 3–7. Mode 2 is engraving and becomes Mark. A contour with another mode, or none, gets no quality of its own and is cut with the machine's automatic quality, and the note says how many (JOB-74). | Observed (the reverse of BV32); mode 0 → quality 1 rather than 8–12 is ours |
| BV10 | The quality an opened contour gets is set as the contour's own (as if picked by hand), so it stays what the file said when the sheet moves to another machine. What a mode means on the shop's machine (which process macro is set up for what) is the machine's business; the opened job keeps only the column number. | Ours |
| BV11 | A part's outline is its exterior cut-out (the biggest one if there are several), else its biggest closed cut; every other closed cut is a hole; engraved contours are its marks. The part starts at the lower left corner of its outline. A part's open cut paths come in as no-cut lines, since the app cuts closed contours only, and the note counts them (JOB-74). | Observed (exterior/interior); the rest estimated |
| BV12 | Pierce modes (`Normal`, `Fine`, `Drilled`, `NoTime`) and where each pierce is are read and kept with each cut for the read-back; the opened job's pierces are planned by its machine. `NoTime` re-pierces into an existing kerf (a cut-off going on after a lift). | Observed |
| BV13 | Micro-joint markers (`ExecMicrojoint`) mark a joint at that point of the cut; the file gives no width, the machine's parameters do. An opened part keeps them as micro-joints picked on its contours (up to 12 per part), with the app's own joint width (JOB-74). | Observed (markers); the width estimated |
| BV14 | A cut-off (a subprogram that cuts no closed contour, such as a remnant or skeleton cut) is cut to the sheet edge from inside, the head lifted, and the rest cut from the same point after a no-time re-pierce, running past the far edge with height sensing off near the edges. On opening, its straight pieces on one line are joined and the line runs edge to edge as a remnant line on its sheet; a cut-off that isn't straight is a free cut. | Observed |
| BV15 | `ExecHighPositioning` lifts the head after a cut that ran off the sheet; it is kept with that cut. | Observed |
| BV16 | A micro-joint marker leaves no gap in the programmed path: the path closes as if there were no joint, and the machine stops the beam short. A contour that closes within 0.05 mm is closed. | Observed (gap); the 0.05 mm ours |
| BV17 | Kerf compensation is written per cut (`SetToolCorrection` `Left` / `Right` / `None`) and read with each cut for the read-back; the kerf goes into the scrap, so a hole can run either way. The opened job's kerf side and direction are planned by its machine. | Observed |
| BV18 | Leads in these files are straight, or straight then an arc; their lengths vary by contour. An opened part keeps where each contour starts (where its lead meets it, up to 8 per part) as its cut start; the lead's shape and length come from the machine's lead tables (JOB-74). | Observed (lead forms); keeping only the start estimated |
| BV19 | Inside a part, engraving comes first, then the holes, then the outline. The opened part's cut order is planned by its machine (marks before cuts, the outline last), which gives the same. | Observed |
| BV20 | A nest cuts each part completely (its subprogram) before the next. Each nest opens as one sheet, or as many identical sheets as it is run, on the active machine, with every placed part at its position and rotation (a quarter turn, or any other angle); a part's quantity is the job file's ordered one, else how many the nests place. A part whose subprograms differ between nests (shape, a contour's state or cut quality, cut starts or joints, beyond 0.01 mm) opens as one part per version; shapes are compared as their lines and true arcs, both ways, so a side drawn in more pieces or an extra point along it is the same shape. The later ones are named `<part> (<nest>)`, each with the quantity its nests place, and the note says so (JOB-74). Commands this reader doesn't know are skipped and counted by name in the note; known ones that don't change a flat nest's shape are passed over. | Observed (order, placements); counts, the machine and part versions ours (estimated) |
