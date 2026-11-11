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
A cutting machine set-up the program writes programs for: its post, technology tables and settings. One machine can stand for several physical cutters of the same kind, e.g. two lasers of different power.
_Avoid_: laser, cutter (for the set-up)

**Cut quality**:
One of the machine's 12 numbered sets of cutting settings (speed, pierce, gas pressure, power), named and coloured by its technology table, e.g. Slow, Medium, Fast, Marking. Every cut contour has one.
_Avoid_: technology, quality level

**Cut order**:
The order the machine cuts things in: a part's cut order is the order of its contours, a sheet's cut order the order of its parts. The programmer can set any part of it by hand; the rest follows the machine's automatic order. The outline is always cut last.
_Avoid_: machining order, cut sequence, cut pattern

**Rapid**:
The head moving from the end of one cut to the start of the next without cutting.
_Avoid_: travel, jump, idle move

**Sweep**:
Holding the mouse button down and dragging across contours or parts as if the pointer were the head; each is taken in the order the pointer passes over it.
_Avoid_: drag-select, paint stroke
