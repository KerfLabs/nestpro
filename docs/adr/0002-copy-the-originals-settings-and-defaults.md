# Copy the original's settings and defaults; a shop's habits are its own settings

nestpro is open source and meant for any cutting shop, not only the one it was first built for. So
where Lantek Expert has a setting for a behaviour (for example the machine's cutting order: by parts
or contours, start corner, direction, zig-zag), the rebuild copies the setting **and the original's
default**, and one shop's habit (say, columns bottom to top, left to right) goes into that shop's
machine settings rather than into the program's defaults. The rebuild picks its own default only
where the original's would cut a part wrong without warning, and says so in `app/MECHANICS.md`
(for example DX3: centre and dashed lines start as Leave out, because a closed centreline would be
cut as a hole).
