# The Library is a folder on the shop's share, not a database server

Several programming PCs share one Library (jobs, job folders, products, sheet stock), as they share
Lantek Expert's SQL Server database. We keep the Library as a folder of files on the shop's Windows
share instead: for a handful of PCs there's nothing to install, license or look after, a backup is a
copy of the folder, and the files can be opened by hand when something goes wrong. Keeping several
PCs safe is therefore the program's job: each change to a shared file happens under a lock, and a
save never writes over someone else's newer work without asking.

## Considered options

- **SQL Server, like the original.** Rejected: a server to install and administer for a few PCs, and
  the rebuild doesn't need to share the original's database (feature parity, not database
  compatibility).
- **A small nestpro service with its own database on one always-on PC.** Kept as the fallback: we
  switch to it only if a two-PC test on the real share shows the folder can't be trusted (lost or
  half-written files, locks that don't hold).

## Consequences

- A two-PC test on the shop's own share comes before more is built on the folder.
- The command line and MCP server write to the same folder, so they follow the same locks and checks
  as the window.
