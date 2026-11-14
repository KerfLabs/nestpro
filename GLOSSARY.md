# nestpro

The words the shop and this program use for getting a drawing from a customer to a cut part.
Behaviour rules live in `app/MECHANICS.md` by rule ID; this file only names things.

## People

**Programmer**:
The person at the office PC who turns drawings into parts, nests them and makes the CNC program.
_Avoid_: user, engineer

**Operator**:
The person at the cutting machine who loads sheets, runs programs and unloads parts.
_Avoid_: user

**Planner**:
The person who groups orders into work orders in the ERP and schedules them.
_Avoid_: scheduler, nester

## Parts and drawings

**Drawing**:
A file from the customer or engineering (DXF, DWG…) that a part's geometry is read from. It arrives with no reliable cut or mark information; the programmer adds that.
_Avoid_: print, CAD file

**Part**:
One shape to be cut, with its quantity, inside a job.
_Avoid_: piece

**Product**:
A part saved in the library for reuse, with its material and thickness and, when preprogrammed, its own technology.
_Avoid_: part type, database part

**Preprogrammed**:
Said of a product whose programmer has already set its technology, so a nest uses that instead of the automatic one.

## What happens to each line

**Contour**:
One joined-up run of lines on a part, open or closed. The closed kinds are the outline and the holes.
_Avoid_: entity, element, profile

**Outline**:
The closed contour around the outside of a part.

**Hole**:
A closed contour inside the outline.

**Cut**:
A line the machine cuts through the sheet.

**Mark**:
A line or text the machine draws on the sheet's surface without cutting through. The shop floor calls these etchings.
_Avoid_: etch, scribe, engrave

**No cut**:
A contour kept on the part and shown on screen, but never cut or marked.
_Avoid_: show only, reference line

**Leave out**:
A line from the drawing that is not made into anything on the part.
_Avoid_: ignore, delete

**Bend line**:
A no-cut line showing where the part is bent later, shown in its own colour. The programmer may mark it instead.

## How a part is cut

**Machine**:
A cutting machine set-up the program writes programs for: its post, technology tables and settings. One machine can stand for several physical cutters of the same kind, e.g. two lasers of different power. Each sheet is cut on one machine; the sheets of one job can go to different machines.
_Avoid_: laser, cutter (for the set-up)

**Cut quality**:
One of the machine's 12 numbered sets of cutting settings (speed, pierce, gas pressure, power), named and coloured by its technology table, e.g. Slow, Medium, Fast, Marking. Every cut contour has one.
_Avoid_: technology, quality level, colour, operation mode (a ByStar Fiber's words for its cut qualities)

**Gas**:
The gas blown through the nozzle with the beam (nitrogen, air, oxygen, argon or water), numbered the way the machine knows it. A job is cut with one gas, like its one material and thickness; the machine's gas is only the starting choice for a new job.
_Avoid_: assist gas, cutting gas, gas type

**Cut order**:
The order the machine cuts things in: a part's cut order is the order of its contours, a sheet's cut order the order of its parts. The programmer can set any part of it by hand; the rest follows the machine's automatic order. The outline is always cut last.
_Avoid_: machining order, cut sequence, cut pattern

**Rapid**:
The head moving from the end of one cut to the start of the next without cutting.
_Avoid_: travel, jump, idle move

**Sweep**:
Holding the mouse button down and dragging across contours or parts as if the pointer were the head; each is taken in the order the pointer passes over it.
_Avoid_: drag-select, paint stroke

**Window**:
A box dragged to the right from open space; it takes only what lies wholly inside it.
_Avoid_: marquee, rubber band, box select

**Crossing window**:
A box dragged to the left from open space; it takes everything it touches, even what sticks out of it.
_Avoid_: capture, lasso, touch select

## Sharing the Library

**Library**:
The shop's one store of saved jobs, job folders, products and sheet stock, which every programming PC opens.
_Avoid_: database (that word is kept for the original program's database)

**Folder**:
A named place in the Library's tree that holds jobs, or products, and other folders. Every PC sees the same tree.
_Avoid_: week, group, directory

**In use**:
Said of a job a programmer has open on their PC; others are warned or kept out, as the shop's setting says.
_Avoid_: locked, blocked (a locked sheet is a different thing: one nesting won't use)

**Hold**:
The Library's note that a job is in use: who has it, on which PC, and since when. One per job; it lapses when its PC stops renewing it.
_Avoid_: lock (the Library's write lock is a different thing: a moment's turn to write)

**Take over**:
Moving a job's hold to this PC from whoever has it, after a warning; the PC that had it is told and keeps its changes only as a copy.
_Avoid_: steal, force open

## Work from the ERP

**ERP**:
The shop's order system (Fulcrum), where orders, work orders and time are kept.
_Avoid_: MES, Fulcrum (in rules that should hold for any system)

**Order job**:
One order line in the ERP: one item to make, in a quantity, through its routing of operations. Not the same as a job in this program.
_Avoid_: job (that word is this program's nesting job)

**Work order**:
The planner's group of order jobs that run through the shop together, one step after another.
_Avoid_: WO, nest, batch

**Step**:
One operation of a work order (programming, laser cutting, bending…), covering every order job on it.
_Avoid_: work-order operation, op

**Programming step**:
The step where a programmer nests and programs the work order's parts; finishing it readies the laser step after it.
_Avoid_: programming job, engineering

**Laser step**:
The step after the programming step where the work order's sheets are cut, and where its nest is stored.
_Avoid_: cutting op, nestable op

**Programming queue**:
The work a programmer can pick up: every laser step on a planner's work order, from the operations the shop chose to show, that is waiting for a nest. One entry per work order and material.
_Avoid_: backlog, work list

**Keep separate**:
Said of an ERP operation whose work must never share a nesting job with work from any other operation, such as aerospace work kept apart from commercial work. It limits only which parts go together, not which sheets or remnants are used.
_Avoid_: AS-only, segregated
