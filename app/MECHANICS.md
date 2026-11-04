# Mechanics read from the original

The rules below are the behaviour the app follows, each with an ID. **Observed** means seen
in the original program's behaviour or files; **estimate** (or "estimated", "guess") means our
own choice where the original's behaviour couldn't be confirmed. Some tables mark this as
[O] observed, [I] inferred, [U] unknown. The evidence for each ID is kept in the project's
private research notes.

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
| T1 | When a part is modified, each selected part is checked for how many technology items it has. If it has some **and the "warn" option is on**, a "part with technology" warning (name, count) is shown. |
| T2 | If the "ask" option is also on, the program asks what to do. Its answer: **0 = put the change on a copy** (the original keeps its technology); **1 = modify the part**; **2 = modify this and all remaining selected parts without asking again**. |
| T3 | A part without technology goes straight to "modify". |
| T4 | Modifying a part compares old and new **material, thickness string, thickness value and machine config**. If all are equal, the data just changes. Otherwise the part's nestings are looked up. If there are any, the user is asked **once per batch** what to do with them (the answer is remembered). **3 = cancel**; **2 = remove the nestings**: each sheet is released (quantity = sheets × copies) and the nesting deleted; otherwise the nestings are kept. |
| T5 | After a change, manufactured quantities are recounted, and the part is flagged "modified in the order" when the nesting window is open. |

How the rebuild uses them:

- Each part can carry **its own technology** (Part Properties → Technology): lead-in, lead-out,
  micro-joints, joint width and cut quality. Off = automatic (5 mm lead-in, no joints).
- The cutting plan and machine program use it. Lead-in and lead-out lengths are applied (holes
  are capped to stay inside). **Micro-joints** are spread evenly along the outer contour, where
  the beam switches off (`M08 ( MICRO-JOINT )`), jumps across with `G0`, and switches on again
  (`M07`). The joint widths are subtracted from cut length. The feed is the material speed ×
  quality factor (Rough 1.2, Normal 1.0, Fine 0.7: estimated).
- **T1–T3**: changing the shape or size of a part with technology opens "Part with
  technology" with **Edit a copy** (T2 answer 0), **Edit this part (remove technology)**
  (answer 1) and **Cancel**, plus "Use this answer for the rest of this session" (answer 2).
  The warning can be turned off in Options (the T1 "warn" option).
- **T4**: changing material or thickness while sheets are nested asks **Keep nested sheets**
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
| D1 | The Jobs manager itself never **reads** DXF. It **writes** a part as DXF to the temporary bend folder as `<id>.dxf`, plus a one-line `<id>.txt` beside it. |
| D2 | "Send to bending" requires the bending software to be installed, and the part's material **and** thickness must equal the product's defaults, or it stops with a message. Then the bending program is run on the DXF and the program waits for it. |
| D3 | DXF reading has these settings: joining tolerance, ellipse discretisation, spline mode ("As polylines" / "As splines, segments from SPLINESEGS" / "from the user"), blocks, layers, conversion to system units, circle recognition. |
| D4 | DXF config defaults: **0.01** and **0.1**, then 10 layer slots that are each on with an empty name. Read here as joining tolerance 0.01 mm and arc smoothness (chord error) 0.1 mm; which value is which is inferred. |
| D5 | Each object read from the DXF is looked up by **layer and line type** in a layer table. The default code **13** means a normal (cut) contour. Objects on layers mapped to another code get that technology applied, e.g. marking, and texts are turned into geometry in **Arial**. |
| A1 | Automatic lead-in/out record defaults: on; two angles of **120°**; **45°**; **3.0**; **10.0**; a value of **100.0** (or −1 when a flag is set); 8 lead records with type codes **13, 1, 11, 10, 12, 10, 9, 5**. |
| A2 | Each lead record's defaults: **10.0**, **7.0**, angles **45° / 45° / 45°**, 10.0 / 10.0 when active, limit **99999.999** (no limit) or **0.5**. Field meanings weren't named. |
| A3 | Automatic micro-joint record defaults: all 10 joint records have type **−1 (none)**, plus one value of **100.0**. **By default no micro-joints are made.** |
| A4 | Lead and joint settings are kept as **12 records per cut type**, and the first type's 12 are copied to the others. With no cut types, a single record holds 99999.999 / −1.0. |

How the rebuild uses them:

- **Joining tolerance 0.01 mm** (`dxf::JOIN_TOL`, D4). Open pieces whose ends are further
  apart than that stay open and are reported.
- **Arc smoothness 0.1 mm** (`dxf::ARC_CHORD_ERR`, D4): arcs, circles and bulges are split
  so no straight piece strays more than 0.1 mm from the true arc.
- **Layers (D5)**: a drawing with more than one layer opens "Drawing layers". Every layer
  starts as **Cut** (code 13) and can be set to **Mark** or **Leave out**. Marking lines show
  dashed on the part, are done before the part is cut (`M10 ( MARK ON )` … `M11`), and count
  toward time at twice the cutting speed (the speed factor is estimated). The sample plate
  has a MARK layer.
- **Export DXF (D1)**: Part Properties → Export DXF writes `<part>.dxf` (outline and holes on
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
the bending hand-off itself (D2: no bending software here), and splines and ellipses (not
read yet).

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
- **Micro-Joints → Automatic** sets the part's joint count from its size class (P7).
  **Sizes** edits the limits and counts. "Original defaults" restores P7's off-state, and an
  "Example 0 / 2 / 4" preset is offered. X and Y counts are added together and the joints are
  spread evenly round the contour (simplified).
- **Loops → Automatic** adds a loop outside every convex outer corner of 95° or less (the
  threshold is estimated). Each loop runs past the corner, swings round outside the part and
  comes back along the next side (`( LOOP )` in the program). It is drawn yellow on the cut path.
- **Cut Technology → Automatic** gives the part its own technology with defaults. **Delete**
  returns it to the sheet's automatic technology.
- **Apply to**: the selected part, or all parts.
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
  it; Shift+click adds or removes it; dragging a box selects the elements fully inside (E2).
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
- Not imitated: free-angle turning by dragging around a point (our parts turn in 90° steps),
  snapping on the true outline instead of the box, and the sliding snap.

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

How the rebuild uses them (Sheet tab → **Manual cutting**):

- **Pick contours**: click contours on the sheet in the order they should be cut. The contour
  under the pointer is highlighted with the start point; where you click is where the cut
  starts (MM4).
- **Reverse direction** (or Shift+click for one pick) cuts it the other way.
- **Undo pick** removes the last pick, and **Clear** goes back to fully automatic cutting.
- A contour already picked can't be picked again (MM3). Picking a part's outside first cuts its
  remaining holes before it, so the part doesn't drop (MM2).
- Picked contours are cut **first, in the picked order**, labelled `( MANUAL CUT )` in the
  G-code and shown with M1, M2… badges and a direction arrow. Every other contour follows
  automatically.
- While picking, dragging only pans the view (parts don't move). Nesting again clears the
  picks.
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
  inside another part are cut first.
- **Corners** (A017, level 5): not used by the strip engine (it always nests from the lower left).
- **Rotations** (R1): the Part Properties have the 0/90/180/270° buttons and "may be flipped".
  Placements store quarter turns and a mirror flag; turning a placed part steps to the next
  allowed angle. Free angles (R3/R4, 30° steps) and sheet-axis rotation (R5) are written as
  functions and tested, but not used: the strip engine tries quarter turns and mirrors only.
- Result on the sample job: see "True-shape strip nesting" (before/after table).

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
  background, cut contours yellow (C008), marking texts green (C053), notes grey, the **red
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
- Simulation: **Beginning, Previous, Run/Pause, Next, End, Stop** — contours already cut turn
  green (C057), the one being cut is white with the head marked; Run steps every 0.7 s.
- Notes: Dimension notes Automatic / Delete, Texts (new text), Modify (the selected text),
  Text and Dimensions on/off.
- **Angle between three points** (DR8): click a point, the corner, then another point — the
  angle at the corner (0–180°). Distance and Angle and this angle show **2 decimals** (DR7).
- **What's still missing**: Home and Utilities have a *Still missing* button (also Options →
  "What's still missing in the Drawing module…") listing, tab by tab and group by group, every
  command of the original's Drawing module this rebuild doesn't have yet (152), with what each
  tab already has.
- Not imitated: the per-technology colours and mixing (DR4/DR5) — every contour uses the cut
  colour — and the drafting commands.

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
  plays in order (per part its markings first, then each contour: rapid, pierce, lead-in,
  contour, lead-out; punched contours hit by hit). Done cuts turn green (C057), markings light
  green, punch hits orange; the head is a red ring with a cross (SM3). Previous/Next jump
  between cutting operations; the slider moves anywhere.
- The Simulation Window shows the operation (e.g. *Cut contour 12 : hole of part 5 (Ring)*) with
  its colour, the head's X / Y, the **cutter radius 0.150** (half the 0.3 mm cut width — the
  value observed in the original) and **1 head**.
- The status bar shows the sheet's CNC name (`CNC: <JOB>X<sheet>`).
- Times come from the plan: cuts at the part's feed plus a pierce time, rapids at 60 000 mm/min,
  markings at twice the cut speed, punch hits 0.25 s.
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

- **Machine** choice (Sheet tab → Machine, and the drawing's Machining tab): Laser, Plasma,
  Oxyfuel, Waterjet (cutting, MC1 type 0), Laser-Punch (type 1) and Punch (type 2). The machine
  shows in the window title, the status line and the program header (`( MACHINE … - TYPE n )`).
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

- **Automatic edge**: a straight line from sheet edge to sheet edge, the part gap + half a cut
  (0.3 mm kerf, estimated) past the parts (RM1), in X or Y — whichever leaves the larger remnant.
  Refused when less than 100 mm would be left (RM1's minimum; value estimated).
- **Exact edge**: a stepped line that follows the parts band by band, so the remnant keeps every
  free corner; bands shorter than 50 mm are merged into a neighbour (no teeth or slivers).
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
- Dragging a box in the Part view also selects the texts lying wholly inside it (T14). They are
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
- **Radial tool menu**: right-click (or S) on the Part or sheet canvas opens a ring of eight
  tools, in the style of a CAD program's mouse gestures. Clockwise from the top: lead-ins, lead
  on element, micro-joints, joint on element, place text, text on element, loops, fit view.
  - Keep the right button down and flick toward a tool to run it.
  - A plain right-click leaves the ring open for a click; the middle, Esc or another
    right-click closes it.
  - On the sheet, the part under the pointer is selected first; in the Part view, the element
    under it is.
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

Sized by what each would take to rebuild here (list in src/v44.rs, shown under Options -> "Lantek v44 new features").

- **HUGE:** New Import Assistant (4.4); component extraction — stops, traps, lifts, magnetic cups with a 10 s timeout in LANTEKPR.CFG V145, lift strip collisions (3.4.2, mostly punch); nesting parts grouped by attributes into zones with remnant lines (3.1.1).
- **Big:** nano-joints (2.2.1: reduced-power partial cut, point or length, qualities, green colour, automatic at contour ends; needs post support); Verify Machining command (3.1.4) — **built**.
- **Medium:** part properties from the file name (4.1/4.3, format in D018); text/QR/template command with preview (3.1.6, partly built); pre-cutting cycles on lead-ins (3.1.5); multi-head ascending order (3.2.1); punch-only options (3.3.1, 3.3.2, 2.3.1).
- **Small:** select equal / inside / outside contours (2.1.1-2.1.2); Part Viewer reference and size, Ctrl + scroll (3.1.2-3.1.3); active/inactive machines (2.1.3); multi-edit "<Different values>" and shortcut list (1.1-1.2).

| # | Rule |
|---|------|
| V1 | Verify Machining runs on the current sheet or all sheets of the subjob; with several sheets the panel shows how many are correct and incorrect, and each sheet's result can be opened. |
| V2 | "Check safe distance": the minimum distance between parts is either the separation of parts set for the machine's material/thickness or a typed value. |
| V3 | The rebuild measures outline to outline (or to the hole a part sits in) and marks each pair closer than the distance with a red line on the sheet; holes-before-outside is checked per part (`v44::safe_distance_issues`, `drawing::verify_machining`). |


### Verify Machining panel

| # | Rule |
|---|------|
| VM1 | Cut settings group: **"Check safe distance"** checkbox; a list with **"Value"** then **"The separation of parts"**; a **"Minimum distance"** number from 0 to **99999.999** mm; **"Include auxiliary technologies during the test"**. |
| VM2 | The list and the distance are only enabled while the checkbox is on. With "The separation of parts" the distance shows the machine's separation and can't be edited; with "Value" it is typed. The stored setting is choice × checkbox (0 = off). |
| VM3 | Each sheet's result is a list of codes, and each code has its own message; with no error the message is "All machining on the sheet has been executed". Code **20 = "Not enough separation between parts"**; 3 overlaps; 4 out of the work zone or in clamps; 8 cut contours without lead-in/lead-outs; 10 remnants or parts that fall; 14 not enough micro-joints; 16–19 collisions. |
| VM4 | Totals: a sheet whose first code is 0 counts as correct, otherwise incorrect; shown as **"Correct sheets: n"** / **"Incorrect sheets: n"**. |
| VM5 | Per-sheet results show one icon and message per code; the heading is **"Current sheet"** in nesting and "The selected part" in the Drawing module. |
| VM6 | The machining checks themselves (punch overlaps, clamps, extractions, shearing, repositions) return one counter per error type. |

The rebuild (src/v44.rs) uses the same codes and messages: separation (20) and touching parts (3) from
the outline-to-outline distance, parts outside the sheet (4), cut contours without a lead-in (8) and
holes cut after the outside (10, "parts that fall"); its own pierce check is listed separately. The
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
| BL4 | Sheet program: `;ADILAS`, `;MAT,length,width,thickness`, `;IDMAT=`, `;NOMEMAT=`, `;ALIASMAT=`, `;IDGAS=`, `;NOMEGAS=`, `;Postprocessor version 2.0.0`, `;R220=Z16*1000+2*100+0`, `R12=`thickness, `R201..R203=0.0`, `R21=`length, `R22=`width, `R23=`length − PartXMax, `R24=0`, `R523=10`, `R524=0`, `R500=0`, `INITNEST`, then per part `ORIG(x,y,angle)` (PS1 Z005 = 1; 0 gives `TRANS X.. Y..` + `AROT Z..`) and `N<k> CALL"/_n_wks_dir/_n_sheet_wpd/_n_<SUB>_spf"`, then `ENDNEST`, `RET`. Warns "Numero sottoprogrammi superiore al massimo consentito - **96**" (Z027). |
| BL5 | Part subprogram: `;M01`, `R16=chk_pret` (Z056), `CICLA:`, `INITPART(1)`, `;<part>`; per path `NEXT_L(x,y,first?0:1,pierce,quality)` (marking: pierce 0, quality 9; cutting: pierce 2, quality 3 in the example), lead-in `G01 G41/G42 X Y`, `PROFILE`, moves `G01/G02/G03` with `CORNER` after a corner whose angle is at most **119.9°** (Z043), last move as `LASTLINE(…,x1,y1,x2,y2,0,0)` (10 parameters, Z061); then `ENDGEO`, the pre-cut retry `IF R16==1 / STOPRE / R16=0 / GOTOB CICLA / ENDIF`, `ENDPART`, `RET`. Coordinates up to 3 decimals, whole numbers as `12.`. |
| BL6 | Material codes from PS1 D-lines: X5CrNi 0100, RST 37-2 0010, Al 1050 A 0200 (Z063 = 1 takes alias and id from the system). |

The rebuild (src/blm.rs): **Machining -> Post settings -> "BLM LC5 output"** (on when the active machine
is an LC5/BLM) makes Export CNC write `<NEST>.XML` and `<NEST>.zip` (sheet SPF, part SPFs, LIS) per sheet
following BL1–BL5, zipped in-app with deflate. Estimated / not built: the 2nd LASTLINE value (written
`1.` for cut paths, `0` for marking — the example has 1. and 1.004), arcs (written as G01 moves),
micro-joints, the FINAL_CUT / remnant-line subprogram (`HD_DWN_OFF`), part rotation through ORIG's angle
(each rotation gets its own subprogram instead), and gas defaults.


### BLM LC5 — arcs and cut-off lines

| # | Rule |
|---|------|
| BL7 | Curves are written as arcs: `G02`/`G03 X.. Y.. I.. J..` with I/J = centre − arc start (incremental), e.g. `G03 X51.31 Y37.345 I0. J-1.`. The rebuild fits arcs to the contour's points: every point within **0.005 mm** of one circle (the post library's machine tolerance, CF1 A005), turning one way, at most **30°** per step (so a square's corners stay corners), radius at most 5000 mm, at least 3 moves, less than a full turn. The last move stays the LASTLINE straight. |
| BL8 | Cut-off / remnant line = its own subprogram, PartRef and Istanza Name **FINAL_CUT** (no PieceTime/WorkOrder/Order), called after the parts with ORIG at the line's start: `NEXT_L(10u,0,0,3,1,20u,−10u)`, `G01 G40` to 10 mm in, `PROFILE`, cut to 10 mm before the end, **`HD_DWN_OFF`**, run on to 10 mm past the end (u = the line's direction). 10 mm = PS1 Z039/Z040 sheet-trim overruns (Z038 = 1). Lines that aren't one straight edge-to-edge segment are written as an open cut path. |
| BL9 | Repeated points are dropped so no zero-length moves are written (rebuild). |

Still not written for the LC5: micro-joints (PS1 Z051 "LASTLINE su microgiunzioni"), part rotation
through ORIG's angle, part times (PieceTime), and the meaning of NEXT_L's 6th–9th values for the
cut-off line (read from the example; `20u, −10u` is inferred from one vertical line).


### BLM LC5 — the post's arcs and the sheet trim (cut-off line)

| # | Rule |
|---|------|
| BL10 | An arc move is written `G0n X<end x> Y<end y> I<cx − previous x> J<cy − previous y>` — I/J are the centre relative to the point the move starts from (the last written X/Y). |
| BL11 | The sheet trim (Z038) classifies each end by sheet edge: 1 = x below the tolerance, 2 = y below it, 3 = x at/after the sheet length, 4 = y at/after the width; later tests win (4 > 3 > 2 > 1). |
| BL12 | Start: approach point outside by **Z039** on edges 1/2 (x or y − Z039) and by **Z040** on 3/4 (+ Z040); cut start inside by **Z040** on 1/2 (+) and **Z039** on 3/4 (−). With PS1 Z016 = 3 (LT9) the approach isn't a `G0` move but goes into `NEXT_L(start,0,0,3,1,start − approach,approach)`; otherwise `SPYOUT`, `DO $A_OUT[7]=1`, `G0 X.. Y..`. Then `G01 G40 X.. Y..`, `PROFILE`. |
| BL13 | End: cut to **10 mm** inside the end edge, `HD_DWN_OFF` (Z016 = 3 only), then out past the edge by Z040 (edges 3/4) or Z039 (1/2). |

The rebuild reads Z039, Z040, Z043 from `MchFiles\PstAdi06.PS1` and A005 from `PstAdi06.CF1` at export
(`blm::adi()`; built-in 10 / 10 / 119.9 / 0.005 if the files are missing), classifies trim ends by
edge and writes BL12–BL13 exactly (the example's FINAL_CUT is reproduced line for line by a test).
Estimated: the low-edge tolerance (0.5 mm — the original's value couldn't be confirmed; PS1 B005 "Tolerance out
of the sheet" is 0.5).


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


## Settings (the rebuild's own — not tied to one install)

Saved in `%APPDATA%\nestpro\settings.ini` (src/settings.rs), edited in **Settings** (Options → Settings…,
Jobs → Database → Connections, Nesting → Machining → Machine & output):
- **Connections**: any number of SQL Server databases — server (`host`, `host\instance`, `host,port`),
  database, Windows account or SQL login with any user name; a password can be remembered encrypted for
  the Windows user (DPAPI via ConvertFrom-SecureString, decrypted only inside the query helper). The
  active one opens in Browse database, which loads the Job Explorer tree right away when it can sign in.
- **Machines & output**: any number of machines, each ISO G-code (with any .CFF/.CF1 post library or
  built-in values) or BLM / Siemens 840D (with any .PS1/.CF1 post files and gas); the active one is used
  for Export CNC.
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


## How Lantek stores nestings (and how this app saves jobs)

| # | Rule |
|---|------|
| ST1 | Expert's files for a record live in the **database folder** (LANTEKIN.CNI R005 / share R008) under a folder per kind, **1000 records per sub-folder**: `Products\<n/1000>\<n>` (kinds 1, 3), `Nestings\<n/1000>\<n>` (2, 4), `Machinings\…` (5), `NestQuot\…` (6), and `General\Mlt<n>`, `Turr<n>`, `Punch<n>`, `Hole<n>`, `MchM<n>`, `Para<n>` (11–15, 17). |
| ST2 | A nesting is these files with its number as the name: **`.chp`** (sheet and placed parts), **`.mec`** (machining), **`.ret`** (remnant), `.bmp` and `.wmf` pictures, `.pdf`, `.mcs`, and `<n>Box1.wmf`, `Box2.wmf`. Exporting copies them to `<export>\Nestings\`. |
| ST3 | A part's machining file names come from the database when the part has machinings. |

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

The rebuild does **not** decrypt these: that would mean taking Lantek's encryption keys out of its program.
Open program recognises a Lantek file by MF1 (by extension .mec / .chp / .ret, or by its first line
under any name) and explains, with the file's version, that Lantek encrypts it and that the nesting's
LC5 programs or a DXF export open here instead (MF2).

## The app's own library (product and job database)

Lantek keeps its lists (jobs, products, nestings, folders) in SQL Server tables (DT1–DT10) and each
record's drawings as encrypted files in a shared database folder, `Products\<n/1000>\<n>`,
`Nestings\<n/1000>\<n>` (ST1–ST2, MF2). The rebuild has its own library for shops without Lantek
(src/library.rs, src/libui.rs), laid out the same way but open:
- a folder (Settings → Folders → Library folder; local, or on a shared drive for the whole shop) with
  `Products\<n/1000>\<n>.json` and `Jobs\<n/1000>\<n>.snjob` — **1000 records per sub-folder**, as ST1 —
  and `index.json` listing every record (name, folder, material, thickness, size, counts, last saved) for
  quick lists and search; **Rescan** rebuilds the index from the files;
- a new record takes the next number by creating its file exclusively, so two people saving at once on
  a shared folder can't get the same number (Lantek takes it under a database lock, RF1);
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

### Preprogrammed parts from the library

Lantek adds stored products with **Parts From Database** / **Kits From Database** (Jobs → Home → Import):
products live in PPRR_PPRR_00000100 (DT5), each with its own machining files
(`Machinings\<n/1000>\<n>`, ST1; ST3) and routings per machine (MMNN_PPOO_00000100/200).

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

Still estimated: the rows' values (they come from the material/thickness table, which couldn't be read; the example row "up to r 5 mm → Fine" is the rebuild's), the offset (0), the size given to a lead point, and how the X and Y joint counts are split between sides.


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


## Cut order by sheet zones

| # | Rule |
|---|------|
| CZ1 | The sheet is split into a grid of cutting zones: zone width = sheet length ÷ X zones, height = sheet width ÷ Y zones (each only when its count is above 0), drawn from the sheet origin with X+1 by Y+1 lines. Cutting and marking have their own X/Y counts. |
| CZ2 | Default counts: all four are **20**. |
| CZ3 | The BLM machine file sets them: B120/B121 marking 20 × 10, **B122/B123 cutting 20 × 10**; B110 start left ON, B111 start bottom OFF, B114 horizontal OFF, B115 zig-zag OFF. |
| CZ4 | A part is in the active zone when its box overlaps the zone (rejected if `zone.x0 > part.x1`, `part.x0 > zone.x1`, `zone.y0 > part.y1` or `part.y0 > zone.y1`), and then (zone mode ≠ 4) when one of its box corners — (x0,y0), (x1,y1), (x0,y1), (x1,y0) — lies inside the zone. |
| CZ5 | The next zone is the next index in the zone list: a zone found is marked done, and its box and size are set from the zone record. |
| CZ6 | An example BLM program places its 14 strips at ORIG(10,10), (10,93.2) … (10,1091.6): bottom up, although B111 is OFF. So the rebuild starts at the bottom and doesn't follow B111. |

→ `ct1::zones`, `ct1::box_in_zone`, `cnc::plan_manual`: the sheet is cut zone by zone (columns from the left, each bottom up; rows first with B114, every other one reversed with B115). In each zone the parts in it are taken nearest-first, and parts sitting in another part's hole still go first. The counts come from the active machine's CT1 (`ct1::set_active` when settings are applied); without a file, 20 × 10.

Still missing in nesting, found in the original and the CT1 but not built:
- **Filling holes with small parts**: B134 "test parts inside holes" ON, B076 minimum hole area 0.0.
- **Common-line cutting**: B074 = 0, so off in the BLM machine file.
- **Multi-torch**, and **remnant cut into squares** (B420 = 0, off).
- **Breaking contours that span two zones**: used with work zones and clamps on punching machines.


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
Folders here are one level deep; Cut/Paste/Find/Organize/Export are not built.


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
- **Input:** Jobs → Import DXF accepts .xml, a dropped .xml works, and **`nestpro --import-xml=<file>`** lets an ERP or script hand over an order exactly as it would to Expert.
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
| MA1 | The material master is the SQL table **`DIS_MMTT_MMTT_00000100`**: `MatRef` (key), `IntRef` (internal reference `##MATEXP` + name + suffix), `IsMain`, a norm reference (NormRef), costs and scrap price. "Does it exist" = a select on MatRef. |
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

- **Quality mapping**: Normal = quality 1 (B block), Fine = 2, Rough = 3. A contour's quality comes from the cut ranges by size, or else from the part's quality.
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
| BN4 | Automatic micro-joints (window and agents): the part's `joint_every` becomes the machine's maximum distance for its quality (F150 per quality; kept when already set; agents may give `every=`), the fixed count goes to 0, and a part without a joint gap takes the machine's. The size-class fewest/most are the machine's (applied in the cut plan). Delete clears `joint_every` too, so automatic joints really go. | AM1–AM2; delete clearing the spacing is a fix found in the merge |
| BN5 | Automatic leads (window and agents) default to the machine's tables by contour class; Delete sets the type back to inclined (type 5) with no length, so table leads don't come back. (Agents: `leads type=`, default `machine`.) | AL1–AL7 |
| BN6 | "Search for the best sheet combination" (ST13) runs on the strip nester: for each sheet, when the next format in stock order would be used less than the minimum efficiency (part area ÷ sheet area), every other format with sheets left is filled too; the first reaching the minimum is taken, else the one placing the most part area when that is more. "Look for the best sheet format" nests the whole job on each stocked size (plus the job's sheet when stock may run out) and keeps the best by ST13's order; both stop at the maximum time. One function for the window and agents' `nest`. | ST13 |
| BN7 | Remaking changed remnants (ST12) is shared by the window's save and agents' `stock-book` / booked jobs: a fingerprint of the sheet's remnant lines is kept with its remnants (`made_from`); a sheet whose stored remnants carry no fingerprint (jobs saved before it existed) counts as unchanged, so opening and saving an old job doesn't remake its remnants. The note "changed, but already in use — kept" goes into the save's status note. | ST12; the no-fingerprint case is design |
| BN8 | Naming new remnants: the window asks for references when "Prompt for references" is on (suggested `<origin>-<n>`, at most 40 characters, not empty, not in stock, not used twice; Cancel saves without storing remnants). Agents can't be asked: `stock-book mode=book refs=` gives them in the order `status` lists (`would_store_remnants`, with the suggested references); without refs they are numbered. | UI14 (window); agents: design |
| BN9 | Reference formats and remnant marks (ST14, ST15) are shared by the window and the agent operations: suggested and stored references (`stock-book`), the single-sheet `program` and `export` all go through `engine::stockflow` (`next_ref`, `remnant_marks`), so a program has the same marks whichever made it. The material name for `[MAT]` / `#400` comes from the settings in both. |
