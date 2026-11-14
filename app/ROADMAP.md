# What the original has that the rebuild doesn't (yet)

Full survey of the original's features: its three ribbons (jobs manager, nesting &
machining, part drawing), the helper programs it ships (DNC links, program importers,
technology tables…) and the behaviour recorded in MECHANICS.md.

**Have** = working in the rebuild · **Part** = a simpler version exists · **Missing**.
Priority: ★★★ needed for real daily laser work · ★★ valuable · ★ niche / other machine types.

The item-by-item to-do list (with IDs, estimates to check, and what agents can't reach yet)
is **`FEATURES.md`** at the top of the repo; keep both in step.

## Jobs manager (Home / Manage / Utilities / View)

| Original | Rebuild | Pri |
|---|---|---|
| Job Explorer, Product List, Product view, Sheet list, Sheet View | Have (Jobs screen) | |
| Save jobs to disk / database (New, Connect, Information) | Have — `.snjob` files and the Library (jobs and products on disk, a folder several PCs can share: every change under one write lock, MECHANICS LB1–LB3; `library-check` tests a shared folder from several PCs at once, LB4; a save over someone else's newer save asks first and takes nothing from stock, LB7–LB10; one job folder tree for every PC, with sub-folders, Cut and Paste, JX10–JX15, and one for library parts, each new part filed like its job, JX18–JX20; a Library that can't be reached is said so everywhere, naming its folder, and is never made by itself: a new one is made in Settings after asking, LB11–LB13; with auto-refresh on, the lists and open jobs show other PCs' saves within seconds, LB14–LB16; a job open on another PC is **in use**: held, shown, warned about, Take over, and not deleted, renamed or moved from under it, CU5–CU12); Lantek SQL database is read-only (Windows) | |
| Parts from database, Import parts (DXF/DWG/MEC/IGES/GEO…) | Have — DXF, DWG, Lantek MEC (encrypted too), IGES, PDF, ESSI, DSTV, GEO (`formats/`), picked in an import window with a preview of the drawing (IM10); still missing HPGL, ISO programs, ME10, EPS and others (JOB-26) | |
| Rectangles (quick rectangular parts) | Part — built-in shapes | |
| Sheets stock manager (sizes, quantities, remnants) | Have — Sheet stock window: list/criteria, quantities, priority, locking, create, what the open job uses; nesting takes sheets from stock (best sheet format / combination searches, time limit) | |
| **Materials** and **Consumables** (gas, nozzles) | Part — fixed material list (any thickness typed, in mm or inches: MECHANICS TH1), each with its machine-file key (Settings → Materials → Machine table); the machine file's material × thickness sections pick leads, joints, joint gap and offer gap/margins (MECHANICS MTL1–MTL12); each job is cut with its own gas (1 Nitrogen … 5 Water), picked on the Sheet tab and shown in the job list, the job summary and the message after programs are written, and the machine's gas in Settings → Machines is a pick from the list (MECHANICS GS10–GS17); no nozzle keys | ★★ |
| Machine setup (several machines) | Part — several machines in Settings, each with what it does (laser, plasma, punch…) and its post (library, BLM, CT1); the Sheet tab picks the job's machine from them (MECHANICS MK1) | ★★ |
| Kits From Database, Kit Explorer | Missing | ★★ |
| Quotation (save quote, Quotation Jobs) | Missing | ★★ |
| Element Search (search jobs by type, name, date, status) | Part — simple search | ★ |
| Export / Import nestings, database backup | Part — program + CSV export | ★★ |
| Punches, turrets, multitools, dies, drills | Missing (punch machines) | ★ |
| Flex3D unfolding / bending, 3D importer (SolidWorks, Inventor…) | Missing | ★ |
| Duct parts module, Processes, Parametric editor | Missing (Part — 8 shapes with width/height) | ★ |
| Masterlink / Oseon ERP connectors | Different systems — the rebuild has its own ERP / MES link with a **Fulcrum** connector: Settings → ERP / MES + Jobs → ERP / MES (see below) | |

## Nesting screen — Home tab

| Original | Rebuild | Pri |
|---|---|---|
| Sheets: New, Delete (current / subjob / job), Previous/Next/Go to, Change quantity, Change data, Unblock | Part — a stock sheet or remnant on the current sheet (Sheet stock window), previous/next, clear sheets; agents also have New / Change / Delete sheet (`sheet-add`, `sheet-change`, `sheet-delete`) | ★★ |
| Sheet set-up: material, thickness, machine, sheet size, margin and gap | **Have** — the Sheet tab, one grid of named fields; its *Nest on* line says before nesting whether Nest all takes the stock first (with its sheets and remnants for the job) or this size, and why; the size list shows the stocked sizes (agents: `info` → `nest_on`). Stock from an ERP's inventory missing (STK-20) | ★★ |
| Nesting: Do All (on selected sheets / filling sheets), Active subjob, One Sheet (redo), Delete | Part — Nest all with 5 levels; **Nest sheet** re-nests the current sheet with only its own parts (RN1, also right-click the sheet in the Nesting Explorer); Do All on selected sheets missing | |
| **One Window** (nest only inside a window you draw) | Missing | ★★ |
| **Place** / **Semi-Automatic** (one part at a time) | **Have** — Parts View ghost drop, Enter = auto-place one | |
| Unitary / Grid (part onto a sheet of matching size) | Missing | ★ |
| Collisions, Copy, Move | Have — several parts selected (click, Shift/Ctrl + click, a window dragged right for the parts wholly inside or left for every part it touches, SW1), moved, turned, mirrored and copied together; Alt + arrows move by a step, Ctrl + arrows slide to the next part at the part gap; drags snap and slide (Ctrl + drag copies); Ctrl+C keeps a pattern on the pointer that drops a copy per click from the pieces still to place, asking before extras (HM18–HM21); the quick ring (right-click) has its own tools on a part and on empty sheet (our own addition, MECHANICS QR1–QR6); overlapping parts outlined and warned about at export; turned to any angle by the turn bar, its handle or Ctrl+Q / Ctrl+W by the step (MECHANICS HM14–HM17) | ★★ |
| **Advanced Copy** (arrays of a part), **Place Near**, **Expand**, **Fit** to borders | Part — Compact to left, snap while dropping or dragging, slide to the next part | ★★ |
| **Nesting for common cut** (shared edges) | Missing | ★★ |
| Margins toggle | Have (sheet margin) | |
| Remnants: create (auto / semi-auto), draw edge, exact edge, rotate, reference | Have — nine automatic line shapes, semi-automatic (3 types), saved into stock on save, rotate/flip; reference points and marks missing | ★ |
| Time and cost, Export data | Have | |
| Snap points (end, mid, centre, intersection, tangent…), Distance & Angle, Orthogonal | Missing (the part-gap snap while dragging is in) | ★★ |
| Macros (record) | Missing | ★ |
| **True-shape nesting** (parts interlock by outline) | Have — strip (band) profiles like the original's: parts hook into notches and drop into holes at every level, best method per sheet, on stock sheets and inside a remnant's real outline too (`bands.rs`, `nest::nest_plan`, MECHANICS "True-shape strip nesting"). Quarter turns only; free angles (A014/B058 steps) still missing | ★★ |

## Technology tab (nesting and part drawing)

| Original | Rebuild | Pri |
|---|---|---|
| Lead-in/outs: Automatic, Semi-automatic, Delete, 16 lead types, dynamic, custom, move point, lead-out like lead-in, bisector | Part — automatic leads from the machine's CT1 tables by contour class and quality (types 1–13, zones, corner rules, round-hole clipping, collision test; `leads.rs`, MECHANICS AL1–AL11); one lead's free end dragged by hand in the Part view, on one hole or every hole of the same shape (MECHANICS LE1–LE8); moving a lead's start by drag (TEC-27), types 14–16, force placement, dynamic/custom leads missing | ★★ |
| Micro-joints: Automatic, Semi-auto, 6 types, corner lead-in, nano-joints, move point, to geometry | Part — Lantek's automatic count (ceil P/F150, size-class min/max, even), holes too, centred on elements, corner joints straddle corners, joint leads (`joints.rs`, MECHANICS AM1–AM6); lead and joint tables per material × thickness section of the machine file (MTL7); per-class V records, zone modes, nano-joints missing | ★ |
| Loops (2 types), Round angles | Part — loops | ★ |
| **Bridges** (join parts so they cut as one) | Missing | ★★ |
| Standard bevel, 3D bevel | Missing (bevel heads) | ★ |
| Cut technology: Marking, No cut, Clean cut, Cutting, Micro-joint, Dynamic, Hatch, Bending lines | Part — the machine's numbered cut qualities per part and by size, shown by the machine's names and colours, each contour's own quality to the LC5, marking texts; each contour Cut / Mark / No cut / Bend line / Leave out, open lines kept at import (agents: `contour-tech`, CS1–CS5); a cut quality per contour set by hand, winning over the size ranges and bands (agents: `contour-tech quality=`, CQ6); the cut technology panel on the part's drawing screen paints contours by click, box or sweep, each shown in its quality's or state's colour on the part and the sheet (CS6, CS7); the same panel in the import preview sets the drawing's layers and contours before the part is made (agents: `import-drawing contours=`, IM11, IM12) | ★★ |
| Texts: Marked, Written, **QR codes**, Templates, Vectorize | Part — marked/note/special texts, curved | ★★ |

## Machining tab

| Original | Rebuild | Pri |
|---|---|---|
| Machine All / Automatic / Regenerate | Have (automatic cut order + program) | |
| Manual contours, two points, between micro-joints, continuous contours | Part — manual contour picking | ★★ |
| **Common-cut / continuous cut**, flying cut in holes, hole destruction | Missing | ★★ |
| **Order machining**, change cutting direction, contour ordering | Part — manual order, reverse; a part's own cut order set by hand, kept with the part and its library product (agents: `part-cut-order`, CO3–CO7; every sheet and machine program follows it, CO8–CO10; set and shown on the part screen, Machining → Contours: click, sweep or box, numbers and rapids, CO11–CO13; on the sheet, Manual cutting sweeps or boxes whole parts, MM8, MM9; MAC-5); the holes nobody ordered by hand follow the machine's hole order (None, By proximity, By direction, Cut small holes first) where each copy sits on the sheet (CO14, CO15); the sheet order (start corner, columns or rows, zig-zag, zones) and the hole order are seen and changed per machine in Settings → Machines (CO16, CO17, MAC-62) | ★★ |
| Delete instructions (current, by selection, from-to) | Part — undo pick / clear | ★ |
| **Component extraction** (stop, trap, lift; assign boxes) | Missing | ★ |
| Avoid holes already cut (head rapids) | Missing | ★★ |
| Speed rates, CN/system compensation, postprocessor words | Part — the post library (.CFF/.CF1) shapes the G-code, BLM LC5 output; **kerf compensation built** (side from the cutting direction, G41/G42 + corrector, system offset path, job toggles; MECHANICS KC1–KC18); **speeds and times from the machine's technology table and time file** (speed and pierce time per material, thickness, gas and quality; accelerations, corners, head times; MECHANICS TT1–TT10, CTM1–CTM16); postprocessor words and the Feed rates dialog missing (FEATURES MAC-1, MAC-13–MAC-27) | ★★ |
| Cycles, pockets, nibbling, punch macros | Missing (punch / milling) | ★ |
| Clamps and work zone, sheet reposition, multihead | Missing | ★ |
| Verify machining, verify unstable contours | Part — Verify Machining with safe distance; unstable contours not yet | ★ |

## Simulation, View and helpers

| Original | Rebuild | Pri |
|---|---|---|
| **Simulation** (run / step / next tool / reposition / extraction) | Part — the Simulation tab plays the program in the machine's own times (accelerations, corners, pierces, head); next tool / reposition / extraction not yet | ★ |
| Panels: explorer, viewers, lists, machining window, restore panels | Have (dockable panes) | |
| Zoom all / window / previous / scale, full screen | Part — zoom, fit | ★ |
| Part drawing (Drafting): lines, arcs, circles, slots, polygons, fillets, offset, trim, union/subtract, dimension notes | Part — the Drawing module's tabs, look, Verify Part, measuring, machining check and simulation are in; drafting commands are shown greyed out | ★★ |
| **Reports / labels** (`Reports\*.rpt`: sheet lists, part labels, cost lists) | Part — CSV report | ★★ |
| **DNC — sending programs to the machine** (one link per control maker) | Missing | ★★ |
| Importing other CAM programs (Amada, Trumpf, Messer…) and other makers' tech tables | Missing | ★ |
| Pallet / suction-cup unloading | Missing | ★ |

## Agents and scripts (command line + MCP)

| Feature | Rebuild | Pri |
|---|---|---|
| Headless engine shared by the window, CLI and MCP (`src/engine/`) | Have | |
| Operations: job, parts (shapes, DXF + layer roles, library), sheet, nest, place/move/rotate/copy, technology (leads, element leads, leads moved by hand, joints, loops, cut quality, texts), cut order, remnants/cut lines, verify, program, export, sheet stock | Have — 56 operations (`nestpro ops`; `machine-tech` reports the machine-file section for the job; `library-check` is the two-PC test of a shared Library folder; `job-folders` and `part-folders` file Library jobs and parts in folders) | |
| Pictures for the agent (sheet/part PNG or SVG, numbered elements) | Have | |
| Every ribbon command mapped to an operation, test-enforced (`nestpro ops --gui`) | Have | |
| Window notices when an agent changed the open job file: reloads it, or asks when both changed | Have (`filewatch.rs`, once a second) | |
| Each open job keeps its own file (Save no longer writes into the previous job's file) | Have | |
| Lantek import XML (`import-xml`, one job file per material and thickness) and BLM program set back into a job (`import-program`) | Have — same rules as the window (`engine/imports.rs`) | |
| Lantek database browser for agents | Missing (`todo:` in GUI_MAP; needs a Lantek SQL Server connection, Windows) | ★ |
| Sheet stock / remnant stock operations | Have — `stock`, `stock-book` (with `refs=` for new remnants' names; changed remnant lines remake unused remnants), `sheet-add`, `sheet-change`, `sheet-delete`; `nest` takes from stock through the window's code, with the best sheet format / combination searches when the window's options are on (`engine/stockflow.rs`, MECHANICS SA1–SA6, BN6–BN8) | |
| Semi-automatic remnant and rotate/flip remnant sheet for agents | Missing (`todo:` in GUI_MAP) | ★ |
| True-shape nesting, automatic leads and micro-joints for agents | Have — `nest`, `leads` (default type: the machine's tables), `joints` (`every=`) run the window's code (`nest::nest_plan`, `engine::TechEdit`, `cnc::plan`); the joint size classes are the machine's (FEATURES TEC-9) | |

## What this means for the next steps

For day-to-day laser work the most valuable gaps are, in order:

1. ~~Saving jobs~~ — done (`.snjob` + Library).
2. ~~Sheet stock and remnants~~ — done (see MECHANICS.md "Sheet stock and remnants").
3. ~~True-shape nesting~~ — done (strip profiles, on stock sheets and inside remnant outlines too); free rotation angles still to do.
4. ~~Speeds per material~~ — done (the machine's technology table and time file; check them against
   the shop's own BLM files, FEATURES MAC-25). Still to do for the post-processor: postprocessor words.
5. **Common cut and bridges**, then **Simulation** and **reports/labels**.

Punching, bevel, duct and 3D unfolding are other products' work and can wait.

## ERP / MES integrations (the rebuild's own; MECHANICS "ERP / MES integrations")

| Feature | Rebuild | Pri |
|---|---|---|
| Integration layer: one `Connector` per system, shared settings page, window and job links | Have (`src/integrations/`, `src/erpui.rs`) | |
| **Fulcrum**: token (DPAPI on Windows), Test connection | Have | |
| Fulcrum: list work waiting to be nested (material + operation groups, search, filters) | Have | |
| Fulcrum: **the planner's work orders** — list them, open a step as a job, store the nest on it (the app makes none) | Have (MECHANICS F18, F22, F23) | |
| Fulcrum: **work stations** — operations and scheduled machines mapped to the app's machines; only their work listed | Have (MECHANICS F20, F21) | |
| Fulcrum: import job operations as a job (cut list, DXF download, units, material, thickness, quantities per order) | Have | |
| Fulcrum: send the nest back (work order, sheets × count, parts per sheet, quantity per job operation, unplaced parts) | Have — **not yet tried against the live API** | ★★★ |
| Fulcrum: sheet stock item matched automatically (material shape + sheet size) | Part — item number typed per job | ★★ |
| Fulcrum: report sheets cut / scrap back (completion tracking) | Missing | ★★ |
| Fulcrum: remnants as inventory | Missing (after the sheet stock manager) | ★ |
| Other systems (Masterlink, Oseon, others) | Missing — add a connector | ★ |
