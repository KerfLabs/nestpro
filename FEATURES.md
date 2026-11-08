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
| STK-16 | The stock file's record of replaced remnants (used by undo, RN6) is never pruned; it grows by one entry per replaced remnant. A clean-up (e.g. drop entries no open job can still undo to) is not built | Missing | ★ | MECHANICS RN6, `Stock::replaced` |
| STK-17 | Changing the Library folder while open jobs hold stock: their holdings (and remnant links) are from the old folder's stock, so later saves give back to or skip taking from the new folder's stock. Record numbers are already dropped (MECHANICS SV3) | Missing | ★ | audit 2026-11-05; rare (switching libraries with booked jobs open) |
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
| REM-13 | Remnant marks preview their references by simulating the next store for every sheet up to the one shown, on each redraw while marks are on; cache it (per job state) if it shows on jobs with many sheets | Missing | ★ | MECHANICS RN6, `stockflow::remnant_refs` |
| REM-14 | Automatic line's distance past the parts: the app uses the offset, or the cutter radius when that option is on or for the irregular "at the cut" shape; RM1 observed half a cut width added to the distance. About 0.15 mm on a 0.3 mm kerf. Check which the original does in each mode | Estimate | ★ | MECHANICS RM1, ST8 |

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
| NST-19 | "One Sheet (redo)" as the original does it: the rebuild's **Nest sheet** re-nests only the parts already on the sheet onto the same sheet (RN1); whether the original also pulls in parts not yet nested, and "Do All on selected sheets", weren't traced | Estimate | ★★ | MECHANICS RN1, RN2; ROADMAP Home tab |

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
| MAC-9 | BLM LC5 output: part rotation through ORIG's angle (each turn gets its own subprogram), part times (PieceTime 0), the NEXT_L values of non-straight cut-off lines. Arcs (BL7, BL10) and the FINAL_CUT trim (BL8, BL11–BL13) are built; see MAC-30–MAC-37 for the rest | Partial | ★★ | MECHANICS BL1–BL13 |
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
| MAC-25 | Speeds and times checked only against the installer's template laser (PSTadi02). The BLM's CF names `PstAdi06.TT1` / `.TM1`, but neither is in the copy of the BLM's machine files we have, so its speeds fall back to the machine default (4000 mm/min, 3 s pierce) and its CT1 has no material × thickness sections. On the one example sheet both programs timed, ours says 714 s (cut 338, pierce 184, rapid 193) and the original's XML 522 s. Find out whether those files exist elsewhere on the shop PC, then match the times card | Estimate | ★★★ | MECHANICS TT1, CTM8 |
| MAC-27 | Oxyfuel, plasma and waterjet technology tables (their own record layouts) | Missing | ★ | MECHANICS TT6 |
| MAC-28 | Overlap check when the part gap is 0: such a job is meant to have touching parts, so parts that cross each other aren't reported either | Estimate | ★ | MECHANICS QR6, `engine::overlapping` |
| MAC-29 | A plain micro-joint across a corner is timed as one straight jump between beam runs; the program goes round the corner in two moves | Estimate | ★ | MECHANICS CTM1; `cnc::beam_runs` |
| MAC-30 | **BLM LC5 output drops part of the plan**: micro-joints, corner loops and punched holes aren't written, so a part planned with them now stops the export (BL39). Lead-outs, overlap and remnant marks aren't written either and don't stop it, while the sheet view, Simulation and times show them | Partial | ★★ | `blm::write_path` reads only the contour points and lead-in; PS1 Z051 "LASTLINE su microgiunzioni" not traced |
| MAC-31 | LC5 values still on a guess: when `LASTLINE` starts `1,<radius>,0` instead of `0,0,0` (we always write `0,0,0`, BL27; real programs have the other on about 8% of contours), and R16 (we always write `chk_pret`, real programs write `1` for some technologies) | Estimate | ★★ | MECHANICS BL5, BL27 |
| MAC-33 | LC5 output shape vs the post: the last move is always a straight LASTLINE (the post writes type 2/3 with I/J when a contour ends on an arc), and lead-in arcs are written as G01 runs before PROFILE (the post writes the straight part, PROFILE, then the arc as G02/G03) | Estimate | ★★ | MECHANICS BL5, BL7 |
| MAC-34 | LC5 NEXT_L pierce and quality codes are fixed (cut 2 / 3, marking 0 / 9): the quality a contour gets from the machine's size bands (TEC-12) or the part's own quality isn't passed to the LC5, so small holes planned slower are cut at the normal quality | Estimate | ★★ | MECHANICS BL5, MTL10; the code ↔ quality mapping of the XML's Foratura / Taglio ids not traced |
| MAC-35 | LC5 subprogram names: the nest name is job + `X` + sheet; when that's 6–8 characters BL3 cuts it to 5, so every sheet of the job writes the same subprogram names (`<5 chars>701`…) with different contents, and two jobs sharing their first 5 characters collide. Real programs don't always cut a 6–8 character name (some 8-character nests keep it whole), so the rule itself is in doubt. Whether the LC5 clears its sheet folder between nests isn't known | Estimate | ★★ | MECHANICS BL3; `blm::nest_name`, `blm::sub_name` |
| MAC-36 | LC5 material and gas: the IDMAT / name written comes from Settings → Materials (defaults STEEL 0010, STAINLESS 0100, ALUMINIUM 0200; an example aluminium-alloy program uses its alloy name and 0210); the gas is one per machine, not per job or material. The LC5 picks its cutting parameters by these | Estimate | ★★★ | MECHANICS BL6, MTL2; TEC-15 |
| MAC-37 | Checking LC5 output: Verify Machining and Simulation run on the cut plan, not on the files written; there's no compare of our program set against the original's for the same nest (a golden set of the shop's own programs) | Missing | ★★★ | `progimport.rs` can re-read a set |
| MAC-42 | LC5 read-back (BL39) checks a cut-off line's start and the line it runs on, not where it stops; it covers LC5 output only | Partial | ★★ | `engine/readback.rs` |
| MAC-44 | LC5 cut-off lines the trim writer can't place are refused by the read-back and stop the whole export: slanted lines, and lines along or within 0.5 mm of a sheet edge. The refusal is right; the writer should cut them | Missing | ★★ | `blm::final_cut_sub`, `engine/readback.rs` |
| MAC-45 | LC5 with **system compensation** on (off by default; use control compensation for now): the lead-in can stop short of the offset contour, and arcs are written from the contour's start, not where the head stands, so they shift. The read-back refuses most of these, with a technical message (a Ø1.5 hole at 0.5 mm cutter radius, or R20 corners at 0.75 mm), but about 0.12 mm can pass within its tolerance | Partial | ★★ | `leads.rs`, `blm::write_path`, `engine/readback.rs` |
| MAC-46 | A lead-in can pierce off the sheet: a 7 mm machine lead on a part at the default 5 mm margin starts 2 mm outside it, and neither Verify Machining nor the read-back says so. Set the margin above the lead length for now | Missing | ★★★ | `leads.rs`, `engine::ops` `verify` |
| MAC-39 | LC5 gas is one per machine in Settings, while real programs choose it per job (mild steel is cut with either gas, even at one thickness). Real programs code oxygen as 3 (BL16); our unused fallback in `for_spec` calls it 2 | Partial | ★★★ | `blm::BlmSettings::for_spec`, `engine::Ctx`, MECHANICS BL6 |
| MAC-40 | LC5 `CORNER`: our limit is 119.9°, but real programs never write it above about 95° (BL28), so we mark corners the original leaves alone | Estimate | ★★ | `blm::CORNER_MAX_ANGLE_DEG`, MECHANICS BL5 |
| MAC-41 | LC5 cut-off lines: a cut-off that should stop where it meets another (an L-shaped remnant) is written across the whole sheet, which splits the remnant | Partial | ★★ | `blm` trim writer, `progimport.rs` |
| OUT-7 | Open program (LC5 set): open cuts without a lead-in are lost, with no warning, so a re-exported program misses them; a stepped cut-off comes back as one slanted line, which the read-back then refuses (MAC-44) | Missing | ★★ | `progimport::parse_sub`, `progimport::parse_set` |

## Jobs, parts and data (JOB)

| ID | What | Status | Pri | Notes / source |
|---|---|---|---|---|
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
| JOB-18 | Shared converter-import options: "ask for the name" mode, equal-part detection, contour ordering, per-import default material/thickness/quantity and rotations (the rebuild takes the job's material; gaps up to 0.5 mm are closed, IM8) | Missing | ★★ | MECHANICS IM1–IM9 |
| JOB-19 | DWG and DXF: model space only (DW6, DX1; the original's DWG setting points to all layouts — check); frozen/off layers aren't left out (DXF neither); password-protected, R12 and older, proxy and 3D objects, attributes, part data from texts aren't read; the converter fallback is untested | Estimate | ★★ | MECHANICS DW6, DW16 |
| JOB-21 | Lantek .mec: marking attribute unknown (nothing imports as marking); engraved texts exploded into loops come in as holes; lead-ins left out; records 4–7, 101–107, 120–124, texts (25) and continuation lines (900) not read; placement matrix (92) not applied; rotated copies in a nesting don't merge; names only for one-part files; the key search depends on how the install's library was compiled (key file otherwise) | Estimate | ★★ | MECHANICS MF7–MF15 |
| JOB-22 | ESSI: bevel codes, the "remove leads and loops" option, ESAB symmetry (code 19), pierce points; where the original sets the 0.1 mm unit isn't confirmed (samples agree) | Partial | ★ | MECHANICS ES1–ES10 |
| JOB-23 | DSTV: unfolding U and L profiles (off by default in the original), bend lines (KA), hole machining (drill, thread, countersink), bevel / weld preparation, point holes; never tried on a real shop file | Partial | ★★ | MECHANICS DS1–DS11 |
| JOB-24 | GEO: plotter-command PLT/PLA files, PKT points, bend data (#~37), ATT attribute groups; TXT field order estimated; never tried on a real file | Partial | ★ | MECHANICS GE1–GE8 |
| JOB-25 | Per-format import options in the window (DWG/DXF paper space and invisible objects, DSTV name expression and rear marking, GEO keep not-cut, MEC split nesting / merge equal, Lantek key file, ESSI dialect, PDF scale / pages / layers): the window uses the defaults; agents have `format`, the ESSI `dialect` and the PDF `scale`, `pages`, `pdf_layers` | Partial | ★ | `formats/` Options |
| JOB-26 | Other importers of the original: HPGL (.PLT), ISO CUT and ISO PUNCH programs, ME10, EPS, CESI OM, ALMA, CAM2000, ASCII punch, parametric parts | Missing | ★ | manual, Drawing module import list |
| JOB-27 | PDF: curves aren't fitted back to arcs and circles (the original's converter does); protected PDFs are refused even with an empty password; page rotation is ignored; text in CID fonts is skipped; the gap between pages and the handling of filled shapes aren't confirmed | Partial | ★ | MECHANICS PD3–PD7 |
| JOB-28 | IGES: hyperbolas and parabolas (104) come in as straight lines; model-space scale is ignored; compressed IGES is refused; level names (406) aren't used; points (116) are left out (the original imports them) | Partial | ★ | MECHANICS IG3, IG5 |
| JOB-33 | A DWG's own unit (its header's `INSUNITS`) isn't read: every DWG is taken in the app's unit (Settings → Nesting), so a DWG drawn in mm opens 25.4× too large on an inch-set PC. The original's import setting itself (DXFASYSW / DWGASYSW `A089`, "transform to system units") isn't read either | Missing | ★★ | `formats/dwg`, MECHANICS IU1 |
| JOB-32 | ERP send: renaming or deleting a job while its nest is being sent drops the work order the ERP returns, so sending again (with app-made work orders) makes a second one | Missing | ★ | `erpui::erp_sent` matches the job by index and name; audit 2026-11-05 |
| JOB-34 | DXF arcs, circles and bulges come in as straight pieces (0.1 mm sag, D4), not as true arcs. The LC5 output fits them back; the default post writes G1 only (`full circle as one G2/G3` covers whole circles, off by default), and sizes read a little short (Ø20 reads 19.91) | Estimate | ★★ | `dxf::arc_segments`, `formats/dwg/geom.rs`, `cnc.rs` |
| JOB-36 | DXF/DWG splines stored as fit points only: a closed one has no smooth seam (a 28–36° corner, and the closing span is straight when the last point doesn't repeat the first), so a Ø100 shape comes in up to 1.6 mm off. Splines with control points are unaffected | Estimate | ★ | `formats/dwg/geom.rs` `fit_cubic`, MECHANICS DW10 |
| JOB-37 | The original's DXF reading settings for ellipse discretisation and spline mode (D3) aren't offered; ellipses and splines are always split within 0.1 mm | Missing | ★ | MECHANICS D3, DW9, DW10 |
| JOB-38 | DXF line types aren't read (D5 looks objects up by layer **and** line type): a closed centreline, such as a bolt circle on layer 0 in CENTER line type, is cut as a hole and drops the middle of the plate. Export doesn't stop it | Missing | ★★★ | `formats/dxfread.rs`, MECHANICS D5 |
| JOB-39 | Objects marked invisible in a DXF are cut by default (`skip_invisible` is off and the window can't change it) | Estimate | ★★ | `dxfread::Options`, MECHANICS DW13 |
| JOB-40 | Exact duplicate objects aren't removed: the same circle drawn twice becomes two holes, cut and pierced twice. Verify Part reports an overlap, the import notes say nothing | Missing | ★★ | `dxf::build` |
| JOB-41 | Export doesn't stop or warn on Verify Part problems (holes that overlap or cross the outside contour, an outside contour that crosses itself), and Verify Machining passes them | Missing | ★★★ | `engine::export`, `verify` operation |

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
| APP-2 | Quick ring set up per screen in Settings: choose the tool in each of the 8 slots of the Part view, on-a-part and empty-sheet rings (any ribbon command), keeping the shared tools anchored | Missing | ★★ | MECHANICS QR1, QR2; the built-in rings come first, so operators find out what they miss |

## Done

| ID | What | Built |
|---|---|---|
| MAC-38 | **LC5 straight edges stay straight**: a run of moves becomes an arc only if every move stays within 0.105 mm of it; big discs, slots and reopened programs are split finely enough to still fit; every set is read back before export and refused if it wouldn't cut the plan | 2026-11-06 (`blm::arc_fit`, `engine/readback.rs`, MECHANICS BL38, BL39) |
| JOB-35 | **Small holes are round**: at least 16 pieces to a circle, so holes of any size are recognised as round and reach the LC5 as arcs | 2026-11-06 (`dxf::arc_segments`, MECHANICS DX2) |
| MAC-43 | **LC5 sheet header**: free margins come from what's written, not fixed values | 2026-11-06 (`blm::sheet_margins`, MECHANICS BL17) |
| JOB-30 | **Drawings without a unit** (DXF without `$INSUNITS`, every DWG) are read in the app's unit, Settings → Nesting (mm or inch), as the original reads them in its system units | 2026-11-05 (`formats::read_bytes`, MECHANICS IU1) |
| — | Audit fixes (agents): a refused operation keeps nothing it changed; `edit-part` changes only the sizes given, a disc's diameter both; MCP refuses a `job` that isn't a path | 2026-11-05 (MECHANICS AG1–AG3) |
| JOB-29 | **DXF units**: the drawing's `$INSUNITS` (inch, foot, cm, m) is applied in every import (window, drag and drop, agents' `import-dxf`, drawings named in an import XML); the ERP import uses its drawing unit only when the file doesn't say | 2026-11-05 (`formats/dxfread.rs`, MECHANICS IU1) |
| OUT-6 | Open program (LC5 set): a contour that ends on an arc (LASTLINE type 2 / 3) comes back with the arc; a contour without a lead-in (BL24) comes back closed; an `.SPF` picked from a folder of several sheets opens that sheet | 2026-11-05 (`progimport.rs`, MECHANICS NX7–NX9) |
| — | Audit fix (parts): looking at a part's properties no longer changes it (quantities over 999, sizes under 10 mm, sheets over 3000 mm, technology values were clamped by the boxes on display) | 2026-11-05 (MECHANICS PP1) |
| — | Audit fixes (nesting): Compact refused on shaped remnants; undo or removing a product drops a part held on the pointer (was a crash); taking parts off a sheet clears its hand-picked cut order | 2026-11-05 (MECHANICS CP1, MM7) |
| — | Audit fixes (stock): stock-only nesting never invents a sheet; a booking that can't take its sheet stores no remnants; a copied job holds no stock | 2026-11-05 (MECHANICS SV6–SV8) |
| — | Audit fixes (saving): closing the window asks about unsaved jobs; Close refuses a job changed since its save; delete / close / copy never leave a job pointing at another job's file; Save won't write over a file an agent changed; agents' undo stops at another writer's change; settings written atomically; Library folder change forgets old record numbers | 2026-11-05 (MECHANICS SV1–SV5) |
| MAC-32 | LC5 output: a contour without a lead-in now gets its G41 / G42 on a short move before PROFILE, as the original's post does | 2026-11-05 (`blm::write_path`, MECHANICS BL24) |
| — | Audit fixes (CNC): a post that turns one axis round swaps G2/G3 too; LC5 copies of a part with different leads get their own subprogram; rings in rings are cut innermost first; remnant marks and cut lines run at their own speed; Export CNC removes the job's stale programs | 2026-11-05 (MECHANICS KC19, BL37, CO1, EXP1) |
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
| — | **Nest sheet**: re-nest only the current sheet's parts onto that sheet, other sheets untouched (ribbon, Nesting Explorer right-click, agents' `renest-sheet`) | 2026-11-04 (`engine::renest_sheet`, MECHANICS RN1–RN6: group zones, stock remnants rebuilt, undo keeps cut lines and follows the stock) |
| NST-13 | **Moving parts by hand**: several parts selected (Shift/Ctrl + click, window, Ctrl+E/Ctrl+A), moved together; Alt + arrows by the movement value (Ctrl+R), Ctrl + arrows slide to the next part at the part gap or the margin; drags snap to the gap and slide along parts; Ctrl + drag copies; turn (R, Ctrl+Q/W/9) and mirror (M, Shift+M) about the selection's centre; copy, remove a selection; Settings → Nesting with mm/inch (fractions), new jobs' part gap and edge margin, step, snap; agents' `slide`, `turn`, `mirror`, group `move` / `copy` | 2026-11-04 (`engine/arrange.rs`, `arrange_ui.rs`, `units.rs`, MECHANICS HM1–HM12) |
| NST-1 | **True-shape nesting**: strip (band) profiles — parts hook into notches and drop into holes, every method fills each sheet and the best is kept; on the job's sheet, stock sheets and inside a remnant's real outline; agents' `nest` runs the same code | 2026-11-04 (`bands.rs`, `nest::nest_plan`, MECHANICS "True-shape strip nesting") |
| REM-12 | Nesting on an irregular remnant follows its true shape (strips outside the outline are taken), no longer its free rectangles | 2026-11-04 (`bands::Sheet::keep_inside`) |
| — | Automatic lead-ins by contour class and automatic micro-joints from the machine's CT1; agents' `leads` / `joints` set the same part technology the window does | 2026-11-04 (`leads.rs`, `joints.rs`, `engine::TechEdit`, MECHANICS AL1–AL11, AM1–AM6) |
| — | **Material × thickness sections of the machine file**: the first fitting section (material key, thickness range, default technology) replaces the lead and micro-joint tables, joint gap, pierce and loop radius; the applied section shows on the Sheet tab and Part view; the machine's part gap and margins offered with *Use*; agents' `machine-tech` and `set-sheet machine_spacing=true` | 2026-11-04 (`ct1.rs`, MECHANICS MTL1–MTL12) |
| MAC-19 | **Speeds and times from the machine's files**: the technology table (.TT1, named by the CF file) gives cutting speed and pierce time per material, thickness, gas and quality and the marking speed (record choice as the original); the time file (.TM1) says where speeds come from, how pierces are timed, and gives accelerations, the 90° corner speed and head times; each move is timed with ramps per axis and corner slow-downs; rapids at A051; the sheet's time split into cutting, marking, rapids, pierces and head on the times card and for agents (`info` `times_s`, `machine-tech` `speeds`); the simulation takes the same times | 2026-11-04 (`techtable.rs`, `cuttime.rs`, `cnc::Timing`, MECHANICS TT1–TT10, CTM1–CTM16) |
| MAC-26 | The program's feed: every contour (with or without a lead-in) and every marking writes its own speed when it differs from the last one | 2026-11-04 (`cnc::program_mm`) |
| TEC-12 | **Size bands** of a machine section (Z010–Z012) pick a contour's cut quality (small holes slower), which drives its leads, kerf table and speed | 2026-11-04 (`ct1::MachineTech::band_quality`, MECHANICS MTL10, TT8) |
| MAC-12 | **Kerf compensation**: side from each contour's real cutting direction (fixes the BLM post's fixed G41-on-holes / G42-on-outsides for mirrored parts and clockwise drawings); control compensation (G41/G42 + corrector, section Z024/Z025) in the generic post; system compensation (offset path with round / chamfer / sharp corners, trimmed inside corners, tiny holes and narrow slots reported, joint gaps, leads moved or offset, pierce moved) used by the drawing, simulation, times and programs; the quality-dependent table; the job's CN / System toggles (Machining → Compensation); remnant and cut-off lines use the machine's kerf; agents' `compensation` | 2026-11-04 (`kerf.rs`, MECHANICS KC1–KC18) |
| — | **Quick ring per screen**: right-click on a part on the sheet (turn, turn back, copy on the pointer, mirror ↔ / ↕, delete, undo, fit) or on empty sheet (select all, next / previous sheet, collisions on/off, compact, undo, redo, fit); Fit and Undo keep their direction in every ring; overlapping parts outlined in red with a "Collisions OFF" badge, and the CNC export (and agents' `program` / `export`) warns about them | 2026-11-04 (`radial.rs`, `arrange_ui.rs`, `engine::overlapping`, MECHANICS QR1–QR6) |
| JOB-20 | **DXF reader on the DWG geometry step**: object planes (mirrored arcs), ellipses, splines, block inserts (nested, scaled, rotated, grids; layer 0 takes the insert's layer), smoothed and 3D polylines, model space only; the 0.5 mm gap closing only touches chains still open | 2026-11-04 (`formats/dxfread.rs`, MECHANICS DXF, DX1) |
| JOB-1 | **Import DWG, Lantek MEC, IGES, PDF, ESSI, DSTV and GEO** besides DXF and the Lantek import XML: own readers for each (DWG R13–2018, encrypted MEC with keys from the shop's Lantek install, ESSI dialects, DSTV plates, TRUMPF and pen-code GEO, IGES curves, vector PDF), picked by extension and content; part data in the file (quantity, material, thickness); several parts per file; gaps up to 0.5 mm closed for every format, DXF too; window Import drawings and drop; agents' `import-drawing` / `drawing-layers` | 2026-11-04 (`formats/`, MECHANICS "Importing part drawings", IM1–IM9, DW, MF4–MF15, ES, DS, GE, IG, PD) |
| — | Saving jobs (`.snjob`) and the Library | before 2026-11 |
