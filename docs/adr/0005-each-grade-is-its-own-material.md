# Each grade is its own material, kept in the Library

A shop that stocks 304 and 316, or 5052 and 6061, has to keep them apart in stock, on jobs and on parts. We considered keeping three fixed materials (mild steel, stainless, aluminium) and adding a grade under each. We chose what the original does instead: the shop names its own materials, each grade is one material, and each is based on a **standard** from the material catalogue. The standard gives the material its kind, its density and, unless the shop sets its own, its machine-file key, so two grades still share cutting technology until the shop splits them.

The material list lives in the Library (ADR 0003), not in each PC's settings, because stock, jobs and parts all point at materials. With a list per PC, one PC's "316" could be another PC's "304".

## Consequences

- Jobs, parts and stock sheets store the material's id and its name. Files saved before this change store 0, 1 and 2, which keep meaning mild steel, stainless steel and aluminium. Those three can be renamed but never deleted.
- A material that is in use can't be deleted. Renaming changes nothing else, because files point at the id, and the name stored beside it is only used to find the material again in a file from another Library.
- A PC with no Library keeps its own list. When it joins a Library, the Library's list wins, and the PC's own materials are added to it unless one of their names already matches a Library material.
