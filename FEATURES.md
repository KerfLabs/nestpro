# Feature list: what isn't built yet

The one list of everything the rebuild doesn't do yet, or does only partly, or does
on a guess. Work from it, add to it, and tick things off here when they're built.
`app/ROADMAP.md` compares the rebuild with the original screen by screen; this file
is the to-do list behind it.

**Status:** **Missing** = not there · **Partial** = a simpler version exists ·
**Estimate** = built, but a value or rule is guessed and needs checking against the
original (or a real job).
**Priority:** ★★★ needed for daily laser work · ★★ valuable · ★ niche or other machine types.

**Rules for keeping it up to date** (also in `AGENTS.md`):
- When you leave something out, guess a rule, or find a gap, add a row: next free ID in
  its area, status, priority, and where it's written down (manual page, MECHANICS rule).
- When you build one, move its row to **Done** with the date and what you built. Never reuse an ID.
- Ribbon commands an agent can't reach yet are `todo: [ID] …` in `GUI_MAP`
  (`app/src/engine/ops.rs`); the test `todo_entries_name_a_feature` fails if that ID isn't here.

## Sheets and stock (STK)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| STK-4 | Prenesting: temporary sheet formats, trimmed to the nested size (coils), optionally added to stock | Missing | ★ | manual pp. 209–213 |
| STK-5 | Reference formats for sheets and remnants (`[MAT]` `[LNG]` `[WDT]` `[THK]` `[UD1-3]` `[AUT]` `[SHT]`) | Missing | ★★ | manual p. 204; counters (`1`…, `<sheet>-<n>`) and asking for remnant references on save (MECHANICS ST6, BN8; agents: `stock-book refs=`) are built |
| STK-7 | Warehouses, locations, stock movements, minimum stock / reorder | Missing | ★ | the original's `WWHH_*` database tables |
| STK-8 | Sheets: delete all of a subjob / job, Go to, change quantity, change data (length, width, remnant margin, user data) for the sheet / subjob / job, background image | Partial | ★ | a stock item on the current sheet (Sheet stock window); agents: `sheet-add` / `sheet-change` / `sheet-delete` |
| STK-9 | Two PCs changing `Sheets\stock.json` at the same moment: the later write wins (no locking) | Missing | ★★ | own risk; the stock is re-read before every change (`Stock::update`) |
| STK-10 | When the stock runs out, the rest go on the job's own sheet size (the original leaves them unnested) | Estimate | ★★ | Do All option `fallback_job_size`, on by default (`stock.rs`); agents: `nest stock=only` turns it off |
| STK-11 | Do All with part groups in zones doesn't take sheets from stock (the groups nest on the job's sheet size) | Missing | ★ | `engine::nest_all` |
| STK-12 | Agent booking (`stock-book`): an agent's job takes nothing from stock until booked; undo re-books rather than rolling the stock back | Estimate | ★ | own design, MECHANICS SA2, SA5 |
| STK-14 | Best sheet format / combination: the search stops at the time limit between whole nestings (a single long nesting isn't cut short); which sheet counts as "better" follows ST13 as traced, the tie margins (1 mm² of waste) are the rebuild's | Estimate | ★ | MECHANICS ST13, BN6; `engine::stockflow::best_format` |
| STK-15 | Remnants stored before remnant-line fingerprints existed are never remade when an old job's lines change (treated as unchanged) | Estimate | ★ | MECHANICS BN7 |
| STK-13 | Remnants stored from a sheet stay in stock when that sheet is re-nested or deleted (only `stock-book release` takes them out) | Estimate | ★ | MECHANICS SA3; the original's behaviour not traced |

## Remnants (REM)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| REM-3 | Line options: minimum X/Y, offset or cutter radius, micro-joints where the line meets the edge, minimum width, round radius, delete area around parts, auxiliary geometry | Partial | ★★ | offset, min X/Y, minimum width, rounding and edge bridges are built (MECHANICS ST8) |
| REM-4 | Remnant reference points (for placing it on the machine) | Missing | ★ | |
| REM-6 | Automatic marking of remnants (text, position, size) | Missing | ★ | |
| REM-7 | Modify edge: drag an orthogonal segment of a stepped line | Partial | ★ | straight lines can be dragged |
| REM-9 | Remnant cut into squares (CT1 B420) | Missing | ★ | MECHANICS "Cut order by sheet zones" |
| REM-10 | Remnant lines between group zones (A789/A790) | Missing | ★ | MECHANICS "Nesting parts grouped by attributes" |

## Nesting (NST)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| NST-2 | One Window: nest only inside a window you draw | Missing | ★★ | |
| NST-3 | Unitary / Grid: a part onto a sheet of matching size | Missing | ★ | |
| NST-4 | Advanced Copy (arrays: window, multiple, grid), Place Near (parallel, perpendicular, sides, points, tangent, corner to a point), Expand, Fit to borders | Partial | ★★ | built: Compact to left, snap while dropping or dragging, Ctrl+arrow slide to the next part, copy of a selection (MECHANICS HM2, HM9, HM10) |
| NST-5 | Nesting for common cut (shared edges) | Missing | ★★ | |
| NST-6 | Snap points (end, mid, centre, intersection, tangent…) for Move's reference and insertion points, Distance & Angle and Angle between three points on the sheet, orthogonal mode (F2) | Missing | ★★ | the gap snap while dragging is built (HM9) |
| NST-7 | Macros (record and replay) | Missing | ★ | greyed in the app |
| NST-8 | Grouping by several attributes at once (A786), start corner and direction | Missing | ★ | MECHANICS "Nesting parts grouped by attributes" |
| NST-9 | Multi-torch | Missing | ★ | a CT1 setting |
| NST-10 | Free rotation angles in automatic nesting (A014 / B058 angle steps); the strip nester tries quarter turns and mirror only | Missing | ★★ | ROADMAP, MECHANICS "True-shape strip nesting" |
| NST-11 | Strip nester guesses: which sheet is "better" (no priority sets; complete sheet, then efficiency, then part area), the efficiency measure, 5 mm strips when the CT1 has no B057, the 0.05 mm rounding guard | Estimate | ★★ | MECHANICS "True-shape strip nesting", `nest::Fill::better_than`, `BAND_DEFAULT`, `BAND_GUARD` |
| NST-12 | Remnants: a part's real outline must stay inside the remnant (the window's checks and the nester agree); copying parts on a remnant (`copy_spot`) still looks for a spot by box | Partial | ★ | `engine::part_fits_format`, `engine::arrange::copy_spot` |
| NST-14 | Turning placed parts by hand at any angle: the rotation step (Ctrl+E) with Ctrl+Q / Ctrl+W, 1/5/10/30/45° keys, a typed angle (Ctrl+T), turn to the minimum rectangle (Ctrl+G / Ctrl+F); turning while dragging. Placed parts only hold quarter turns (`nest::Placed::turns`), so this needs a free angle on every placed part (outline, CNC, texts) | Missing | ★★ | MECHANICS HM1, G3 |
| NST-15 | Moving parts by hand — guesses to check against the original: window selection = parts wholly inside, the blocked drag sliding along, the 14 px snap reach on contours (the original's drag snap and its "closer point without collision" on drop weren't traced), the ring search for a free spot after a turn (the original searches when the part is put down), mirror about the selection's box centre, Alt+arrow kept on the sheet, the 80 px arrow-key pan and which of Page Up / Page Down zooms in | Estimate | ★★ | MECHANICS HM5–HM9, HM13 |
| NST-18 | Remnant cut-outs and machine clamps as obstacles for Ctrl+arrow slides (the original stops at them too); "fit parts to the sheet border" (B078) off should keep half a gap more from the margin in manual moves | Missing | ★ | MECHANICS HM2 |
| NST-16 | Inches everywhere: only the part gap, edge margin, moves and Settings → Nesting follow the units setting; sheet sizes, part sizes, coordinates in hover cards, the status bar and reports are still in mm | Partial | ★★ | MECHANICS HM12, `units.rs` |
| NST-17 | Selection extras: Trap (polyline) selection, invert selection, select by part / all copies of a part, multihead groups (Alt+M) | Missing | ★ | MECHANICS HM7 |

## Technology (TEC)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| TEC-1 | Lead-ins: types 14–16, force placement, dynamic and custom leads, move point, lead-out like lead-in | Partial | ★★ | automatic leads by contour class from the machine's tables (types 1–13, zones, corner rules, round-hole limits, collision test) are built: MECHANICS AL1–AL11, `leads.rs` |
| TEC-2 | Micro-joints: per-class joint records from the material file (V lines), zone modes, nano-joints, move point, to geometry, semi-automatic | Partial | ★ | automatic joints (count per F150, even, size-class fewest/most, holes too, centred, corner joints, joint leads) are built: MECHANICS AM1–AM6, `joints.rs`; the joint table per material × thickness section too (MTL7) |
| TEC-3 | **Bridges**: join parts so they cut as one | Missing | ★★ | |
| TEC-4 | Cut technologies: no cut, clean cut, dynamic, hatch, bending lines | Partial | ★★ | cut quality per part and marking texts are built |
| TEC-5 | Texts: QR codes, templates, vectorize, written text | Partial | ★★ | marked / note / special, curved |
| TEC-6 | Loops type 2, round angles | Partial | ★ | |
| TEC-7 | Bevel (standard, 3D) | Missing | ★ | bevel heads only |
| TEC-8 | Exact lead-length field, the 120° corner limits, position codes above 9, P1's placement for standard shapes | Missing | ★ | MECHANICS (Technology tab) |
| TEC-9 | Micro-joint size classes (fewest / most per small, medium, large part and hole) are edited in the window only (Micro-joints → Settings) and change the active machine's values until the machine is loaded again; they aren't saved to the machine file, and agents use the machine file's | Partial | ★ | `techui.rs`, `ct1::MachineTech::joint_sizes`; agents: `joints mode=auto every=` |
| TEC-10 | Machine-file sections chosen by gas, lens, power, nozzle, pressure, abrasive or configuration alias: jobs carry none of these, so only material and thickness pick the section (and the default-technology rule always applies when the machine has it on) | Estimate | ★★ | MECHANICS MTL4, MTL5; `ct1::JobKeys` |
| TEC-11 | The machine's part gap, Y gap and four margins per material/thickness are only offered (Sheet tab *Use*, `set-sheet machine_spacing=true`), never applied by themselves; the sheet has one margin (the widest is taken) and no separate Y gap | Estimate | ★★ | MECHANICS MTL9; own design |
| TEC-16 | Size bands (MTL10): measured as the contour's equivalent radius, applied to outsides too, and only to parts that leave the quality to the machine (no size ranges of their own, quality Normal) | Estimate | ★★ | MECHANICS MTL10 |
| TEC-13 | A second technology file read after the machine's (its sections win over the first's) | Missing | ★ | MECHANICS MTL6 |
| TEC-14 | Per-class micro-joint records of a section (V lines) and the conditional value groups (Z308/Z313/Z337, X124–X128 switches) | Missing | ★ | MECHANICS MTL11, MTL12 |
| TEC-15 | Material keys for the machine file: one per app material (aluminium defaults to 0210 aluminium-magnesium, a guess); galvanised, copper and brass sections can't be reached until the app has those materials (JOB-2) | Estimate | ★★ | MECHANICS MTL2; Settings → Materials → Machine table |

## Machining and output (MAC)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| MAC-1 | **Postprocessor words** (OPSTOP, STOP, AUXFUN… placed in the program) and the Feed rates dialog for editing the machine's speeds | Partial | ★★ | the post library (.CFF/.CF1) shapes the G-code; BLM LC5 output; CN / system compensation built (MAC-12); speeds and times from the machine's files built (MAC-19) |
| MAC-2 | Common-cut / continuous cut, flying cut in holes, hole destruction | Missing | ★★ | |
| MAC-3 | Avoid holes already cut (head rapids) | Missing | ★★ | |
| MAC-4 | Manual machining: two points, between micro-joints, continuous contours | Partial | ★★ | manual contour picking is built |
| MAC-5 | Change cutting direction / contour ordering tools; delete instructions (by selection, from–to) | Partial | ★ | reverse, undo pick, clear |
| MAC-6 | Component extraction (stop, trap, lift; boxes) | Missing | ★ | greyed in the app |
| MAC-7 | Clamps and work zone, sheet reposition, multihead | Missing | ★ | |
| MAC-8 | Verify: unstable contours, auxiliary technologies, punch-only codes | Partial | ★ | Verify Machining + safe distance are built |
| MAC-9 | BLM LC5 output: arcs (written as G01), micro-joints, FINAL_CUT remnant subprogram, 2nd LASTLINE value | Partial | ★★ | MECHANICS (BLM LC5 output) |
| MAC-10 | Breaking contours that span two zones | Missing | ★ | punching machines |
| MAC-11 | Cycles, pockets, nibbling, punch macros | Missing | ★ | punch / milling |
| MAC-13 | **Modify compensation** per contour (one contour, all holes, all outsides, all contours; side left / right / off), and the per-contour CN / system compensation commands (side + corrector / side + radius) | Missing | ★★ | MECHANICS KC16 |
| MAC-14 | Compensation of common-cut contours (B191, quality modes B750/B751) — needs common cut (MAC-2) | Missing | ★ | MECHANICS KC7 |
| MAC-15 | Compensation of bevel contours (system on bevels, angle compensation) | Missing | ★ | MECHANICS KC3; TEC-7 |
| MAC-16 | System offset: removing self-crossing loops (corner mode tens digit); keeping true arcs in the offset path (ours is a polyline, arcs as runs of short moves) | Missing | ★ | MECHANICS KC10 |
| MAC-17 | Kerf guesses: control words stay on between contours that need the same, G40 on the next rapid that needs none (post choice); a mirroring post swaps G41/G42; the job's toggles win over the machine's switches and quality mode; a vertex turning ≤ 20° counts as part of a curve; sharp corners further than 10 cutter radii are chamfered; 0.15 mm cutter radius when the file has none; in quality mode 2 a negative radius flips the control side too | Estimate | ★★ | MECHANICS KC4–KC6, KC10; own design |
| MAC-18 | The original's test for elements left uncut by compensation (what it compares): ours lists holes no bigger than the kerf, contours narrower than it, narrow slots and dropped inside arcs | Estimate | ★ | MECHANICS KC18 |
| MAC-20 | A job with no table record (or a quality without a table speed) is timed at the machine's default speed (A058 / Z003); the original would time those cuts as almost instant | Estimate | ★★ | MECHANICS TT10; own design |
| MAC-21 | Time guesses: our contours are polylines, so arcs use the straight-cut accelerations (P, not Q) and a vertex turning ≤ 20° counts as part of a curve; jerk-limited moves (S001) are timed as plain ramps; the head stays down over micro-joints and every re-pierce counts a full pierce; laser marking at A063 when the time file says "active"; the N-factor ↔ kind mapping; braking look-ahead before slow corners; a negative D168 (the table's corner speed, not decoded) = no slow-down; zero accelerations or top speeds = no limit | Estimate | ★★ | MECHANICS CTM2, CTM4–CTM7, CTM9, CTM10, CTM14 |
| MAC-22 | Corner feed reductions of the technology table (L601–L612, time file D018 = 3) | Missing | ★ | MECHANICS TT7 |
| MAC-23 | Technology-change and quality-change times, start/end-of-program times, the pre-cut pierce percentage (D167), the radial-acceleration limit on arcs, head Z travel (H014–H017) | Missing | ★ | MECHANICS CTM13; all 0 / off in the shipped template |
| MAC-24 | The CF file's speed and acceleration factors (A037, A038, A046, A047): read by the original, where they apply wasn't traced; not applied | Missing | ★ | MECHANICS CTM3 |
| MAC-25 | Speeds and times checked only against the installer's template laser (PSTadi02); the shop's own BLM files (PstAdi06 .CF1 / .TT1 / .TM1) not seen yet: check which table and time file its CF names and that the times card matches the original's | Estimate | ★★★ | MECHANICS TT1, CTM8 |
| MAC-26 | The program's feed: the header takes the first contour's speed and each lead-in writes its contour's; a contour without a lead-in keeps the previous feed | Partial | ★★ | `cnc::program_mm` |
| MAC-27 | Oxyfuel, plasma and waterjet technology tables (their own record layouts) | Missing | ★ | MECHANICS TT6 |

## Jobs, parts and data (JOB)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| JOB-1 | **Import DWG, MEC, IGES, GEO, DSTV, ESSI, PDF** (DXF and Lantek import XML are built) | Partial | ★★★ | ROADMAP |
| JOB-2 | Materials: thickness ranges and prices, norms, more than 3 materials, XML/cloud material files; consumables (gas, nozzles) | Partial | ★★ | MECHANICS "Materials in Expert" |
| JOB-3 | Kits from database, Kit Explorer | Missing | ★★ | |
| JOB-4 | Quotation (save quote, quotation jobs) | Missing | ★★ | |
| JOB-5 | Element search (type, name, date, status) | Partial | ★ | simple search |
| JOB-6 | Export / import nestings, database backup | Partial | ★★ | programs + CSV |
| JOB-7 | Job folders: more than one level, cut / paste / find / organize / export | Partial | ★ | |
| JOB-8 | Part data from the file name: `[Thickness]`, re-reading on a format change | Estimate | ★ | MECHANICS FN |
| JOB-9 | Parametric editor, duct parts, processes | Missing | ★ | 8 built-in shapes |
| JOB-10 | Flex3D unfolding / bending, 3D import | Missing | ★ | |
| JOB-11 | ERP connectors (Masterlink, Oseon) | Missing | ★ | needs those systems |
| JOB-12 | Fulcrum link not yet tried against the live API: every answer is read as the published API description states it | Estimate | ★★★ | MECHANICS F18–F23; needs a read-only token |
| JOB-13 | Fulcrum: sheet stock item matched automatically from material + sheet size (now typed per job) | Partial | ★★ | MECHANICS F13 |
| JOB-14 | Fulcrum: a job opened from a work order switches the app-wide machine; jobs don't keep their own machine | Estimate | ★★ | MECHANICS F21 |
| JOB-15 | Fulcrum: finding the planner's work order asks for every unfinished work order on the operation, one request each (slow with many open work orders) | Estimate | ★ | MECHANICS F22 |
| JOB-16 | Fulcrum: re-sending replaces the step's nest without checking whether someone else stored a newer one meanwhile | Missing | ★ | MECHANICS F18 |
| JOB-17 | Fulcrum: report sheets cut and scrap back (completion tracking); remnants as Fulcrum inventory | Missing | ★★ | ROADMAP |

## Part drawing (DRW)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| DRW-1 | Drafting: points, shapes, delete geometry, modify, transform, figures | Missing | ★★ | full list in the app: Drawing → "What's still missing" (`MISSING` in `drawing.rs`) |
| DRW-2 | Dimension notes on the part drawing | Missing | ★ | |
| DRW-3 | Move / edit one text | Missing | ★ | add / delete work |
| DRW-4 | Utilities: contours, compare parts, lifting | Missing | ★ | |
| DRW-5 | Zoom window / previous / scale, full screen | Partial | ★ | zoom, fit |

## Simulation, reports, machine link (OUT)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| OUT-1 | **Reports and labels** (sheet lists, part labels, cost lists) | Partial | ★★ | CSV report |
| OUT-2 | **DNC**: sending programs to the machine | Missing | ★★ | agents never send (by design) |
| OUT-3 | Importing other CAMs' programs (Amada, Trumpf, Messer…) and technology tables | Partial | ★ | Open program reads LC5 .zip / .XML / .SPF |
| OUT-4 | Pallet / suction-cup unloading | Missing | ★ | |
| OUT-5 | Simulation: next tool, reposition, extraction steps | Partial | ★ | the Simulation tab plays the program |

## Agents: command line and MCP (AGT)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| AGT-4 | Agent operation for the Lantek database browser (Windows only) | Missing | ★ | `dbbrowse.rs` |
| AGT-5 | Command-line answers in a Windows console from the windowed `nestpro.exe`: type-checked, never run on a Windows PC | Estimate | ★★ | `cli.rs` |
| AGT-6 | Agent operations for the semi-automatic remnant (click an area) and rotating / mirroring an empty remnant sheet | Missing | ★ | built in the window (MECHANICS ST9, ST10) |
| AGT-7 | Agent operations for the ERP/MES link: bring work in as a job, send the nest back | Missing | ★★ | built in the window (`erpui.rs`, `integrations/`, MECHANICS "ERP / MES integrations") |

## App (APP)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
| APP-1 | Start-up: `/hide`, single-copy check, splash, help files | Missing | ★ | MECHANICS SU1–SU7 |

## Done

| ID | What | Built |
|---|---|---|
| — | Sheet stock: list, quantities, lock, priority, criteria, Do All and Sheet Remnants options; Do All from stock (remnants first, order, minimum efficiency); remnants stored on save with outline and area, nested inside their outline | 2026-11-04 (MECHANICS "Sheet stock and remnants" ST1–ST10) |
| STK-1 | **Best sheet format**: nest everything on each stocked size, keep the best (complete, less waste, higher worst-sheet efficiency) | 2026-11-04 (MECHANICS ST13, BN6; window and agents' `nest`) |
| STK-2 | **Best sheet combination**: per sheet, other sizes tried under the minimum efficiency, with a maximum search time | 2026-11-04 (MECHANICS ST13, BN6; `nest::nest_plan_opt`) |
| — | Remnants remade when a sheet's remnant lines change (kept if already used); prompt for remnant references on save; "This job" column in the stock window | 2026-11-04 (MECHANICS ST12, BN7, BN8; window and agents' `stock-book`) |
| STK-3 | Minimum efficiency to drop a sheet after nesting and use another | 2026-11-04 (MECHANICS ST3, `engine::stockflow::nest_from_stock`) |
| STK-6 | Completed sheets: their part-free area stored as remnants ("generate remnants with the completed sheets") | 2026-11-04 (`stockflow::make_remnants`) |
| REM-1 | Semi-automatic remnant (3 modes) in the window | 2026-11-04 (MECHANICS ST9; agents: AGT-6) |
| REM-2 | Automatic line: the original's 9 shapes | 2026-11-04 (`remline.rs`, MECHANICS ST8) |
| REM-5 | Rotate / mirror an empty remnant sheet; remnants oriented to the start corner when loaded | 2026-11-04 (MECHANICS ST10) |
| REM-8 | One region becoming several remnants (part-free pieces); several remnant lines per sheet | 2026-11-04 (`remnant.rs`, MECHANICS ST7) |
| REM-11 | (withdrawn: the remnant is now the sheet less the cut strip and parts, `remnant.rs`) | 2026-11-04 |
| AGT-1 | The window notices when an agent changed the open job file and reloads it (asks when both changed) | 2026-11-04 (`filewatch.rs`) |
| AGT-2 | Agent operation for the Lantek import XML (`import-xml`) | 2026-11-04 (`engine/imports.rs`) |
| AGT-3 | Agent operation for opening a BLM LC5 program set (`import-program`) | 2026-11-04 (`engine/imports.rs`) |
| — | Agent operations on the window's stock: `stock`, `stock-book`, `sheet-add`, `sheet-change`, `sheet-delete`, `nest stock=` | 2026-11-04 (`engine/stockflow.rs`, MECHANICS SA1–SA6) |
| — | Command line and MCP server on a shared engine; every ribbon command mapped | 2026-11-04 |
| NST-13 | **Moving parts by hand**: several parts selected (Shift/Ctrl + click, window, Ctrl+E/Ctrl+A), moved together; Alt + arrows by the movement value (Ctrl+R), Ctrl + arrows slide to the next part at the part gap or the margin; drags snap to the gap and slide along parts; Ctrl + drag copies; turn (R, Ctrl+Q/W/9) and mirror (M, Shift+M) about the selection's centre; copy, remove a selection; Settings → Nesting with mm/inch (fractions), new jobs' part gap and edge margin, step, snap; agents' `slide`, `turn`, `mirror`, group `move` / `copy` | 2026-11-04 (`engine/arrange.rs`, `arrange_ui.rs`, `units.rs`, MECHANICS HM1–HM12) |
| NST-1 | **True-shape nesting**: strip (band) profiles — parts hook into notches and drop into holes, every method fills each sheet and the best is kept; on the job's sheet, stock sheets and inside a remnant's real outline; agents' `nest` runs the same code | 2026-11-04 (`bands.rs`, `nest::nest_plan`, MECHANICS "True-shape strip nesting") |
| REM-12 | Nesting on an irregular remnant follows its true shape (strips outside the outline are taken), no longer its free rectangles | 2026-11-04 (`bands::Sheet::keep_inside`) |
| — | Automatic lead-ins by contour class and automatic micro-joints from the machine's CT1; agents' `leads` / `joints` set the same part technology the window does | 2026-11-04 (`leads.rs`, `joints.rs`, `engine::TechEdit`, MECHANICS AL1–AL11, AM1–AM6) |
| — | **Material × thickness sections of the machine file**: the first fitting section (material key, thickness range, default technology) replaces the lead and micro-joint tables, joint gap, pierce and loop radius; the applied section shows on the Sheet tab and Part view; the machine's part gap and margins offered with *Use*; agents' `machine-tech` and `set-sheet machine_spacing=true` | 2026-11-04 (`ct1.rs`, MECHANICS MTL1–MTL12) |
| MAC-19 | **Speeds and times from the machine's files**: the technology table (.TT1, named by the CF file) gives cutting speed and pierce time per material, thickness, gas and quality and the marking speed (record choice as the original); the time file (.TM1) says where speeds come from, how pierces are timed, and gives accelerations, the 90° corner speed and head times; each move is timed with ramps per axis and corner slow-downs; rapids at A051; the sheet's time split into cutting, marking, rapids, pierces and head on the times card and for agents (`info` `times_s`, `machine-tech` `speeds`); the simulation takes the same times | 2026-11-04 (`techtable.rs`, `cuttime.rs`, `cnc::Timing`, MECHANICS TT1–TT10, CTM1–CTM16) |
| TEC-12 | **Size bands** of a machine section (Z010–Z012) pick a contour's cut quality (small holes slower), which drives its leads, kerf table and speed | 2026-11-04 (`ct1::MachineTech::band_quality`, MECHANICS MTL10, TT8) |
| MAC-12 | **Kerf compensation**: side from each contour's real cutting direction (fixes the BLM post's fixed G41-on-holes / G42-on-outsides for mirrored parts and clockwise drawings); control compensation (G41/G42 + corrector, section Z024/Z025) in the generic post; system compensation (offset path with round / chamfer / sharp corners, trimmed inside corners, tiny holes and narrow slots reported, joint gaps, leads moved or offset, pierce moved) used by the drawing, simulation, times and programs; the quality-dependent table; the job's CN / System toggles (Machining → Compensation); remnant and cut-off lines use the machine's kerf; agents' `compensation` | 2026-11-04 (`kerf.rs`, MECHANICS KC1–KC18) |
| — | Saving jobs (`.snjob`) and the Library | before 2026-11 |
