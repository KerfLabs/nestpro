# nestpro — Sheet Nesting

A source-available sheet-nesting CAM program for cutting machines (laser, plasma, oxyfuel,
waterjet), written in Rust + egui. It covers a shop's daily cutting workflow: jobs, parts,
sheet stock and remnants, nesting, cutting technology, machining and CNC programs.

It is Windows-first and also builds and runs on Linux.

It reads files from Lantek Expert (`.nxp` nestings, `.mec` parts, machine files) so a shop
can move its work over, and aims to match that program's features and behaviour.
`app/ROADMAP.md` lists what's done and what's missing.

> nestpro is an independent project. It is not affiliated with, endorsed by or
> connected to Lantek. Lantek and Lantek Expert are trademarks of their owner and are
> used here only to describe file compatibility.

| Folder | What's in it |
|---|---|
| `app/` | The app. `ROADMAP.md` = done and missing features, `MECHANICS.md` / `mechanics.json` = the behaviour rules the app follows |
| `tools/` | `check.sh` (public check + Linux tests + Windows type-check), `shot.sh out.png` (build + screenshot on a virtual display), `public-check.sh`, `install-hooks.sh` |
| `FEATURES.md` | The list of what isn't built yet (IDs, status, priority) |
| `examples/drawings/` | Made-up part drawings (DXF) to try Import drawings and nesting; `DEMO-OPEN-SKETCH` can't make a part, to show what the import window says then; `DEMO-BEND-PLATE` has a bend line, a stray open line and four equal holes, to try the cut technology panel |
| `research/` | Not in this repo: a private checkout of the research notes (see `AGENTS.md`) |

## Build

```bash
cd app && cargo run --release          # Windows or Linux
tools/check.sh                         # public check + Linux tests + Windows compile check
```

Linux needs GTK 3 for file dialogs (`libgtk-3-dev` on Debian/Ubuntu). To read an existing
Lantek install, set `LANTEK_DIR` to its folder (default `C:\Lantek\Expert`).

## For agents and scripts: command line and MCP

The same program runs without its window. `nestpro.exe` with no arguments (or a
job file) opens the app as before; a known operation name runs it headless:

```bash
nestpro new  jobs/frames.snjob --w=3000 --h=1500 --material=steel --thickness=5
nestpro import-dxf jobs/frames.snjob --path="C:\Drawings\BR-100.dxf" --qty=20
nestpro stock --mode=create --w=3000 --h=1500 --thickness=5 --material=steel --qty=10
nestpro contour-tech jobs/frames.snjob --part=1 --contours=5 --state=mark   # numbers from part-info
nestpro contour-tech jobs/frames.snjob --part=1 --contours=2 --quality=1 --same=true   # every equal hole cut at quality 1
nestpro part-cut-order jobs/frames.snjob --part=1 --mode=add --contours=3,1,2   # these holes first, in this order; the outline last
nestpro nest jobs/frames.snjob                      # takes sheets from stock (remnants first)
nestpro cut-order jobs/frames.snjob --sheet=1 --mode=parts --items=4,2,1   # these parts first, each cut whole in its own order
nestpro leads jobs/frames.snjob --mode=machine
nestpro machine-tech jobs/frames.snjob               # which material/thickness table of the machine file applies
nestpro compensation jobs/frames.snjob --system=on   # kerf: offset path instead of G41/G42 (on/off/machine)
nestpro remnant jobs/frames.snjob --mode=auto --sheet=1
nestpro verify jobs/frames.snjob
nestpro render jobs/frames.snjob --sheet=1          # PNG next to the job
nestpro export jobs/frames.snjob                    # programs + parts.csv
nestpro stock-book jobs/frames.snjob --mode=book    # sheets out of stock, remnants in
nestpro ops            # every operation;  nestpro help <operation>  for its options
nestpro ops --gui      # which GUI ribbon command each operation stands for
```

- Answers are JSON on stdout (`--text` for plain lines). Exit code 0 = done,
  1 = refused/failed (the JSON says why), 2 = bad command.
- **The job file is the truth.** Every change is saved straight into the
  `.snjob`; the previous version is kept as `<job>.snjob~` and `undo` goes back.
  The app opens the same file (Open job), so a person can check or finish what
  an agent programmed. While a job is open in the window, the window looks at
  its file every second: an agent's change is reloaded, and when both sides
  changed it asks whether to take the file's version or keep its own.
- Work from other systems: `import-xml` turns a Lantek import XML into one job
  file per material and thickness; `import-program` turns a BLM LC5 program set
  (`.zip` / `.XML` / `.nxp`) back into an editable job.
- Numbers are from 1, as on screen: part 1, sheet 1, placed part #1, element 1.
- **Sheet stock** is the window's (Library folder, `Sheets\stock.json`; Jobs →
  Sheet stock). `nest` takes sheets from it by the window's Do All options, in
  the same code as the window. An agent's job leaves the stock alone until
  `stock-book --mode=book` (saving the job in the window books it too); a booked
  job keeps the stock in step with every later change, and `--mode=release`
  gives it all back.
- **One Library for several PCs.** Every change to the Library's records, its
  list and the sheet stock is made under one write lock in the Library folder,
  so PCs and agents saving at the same moment never lose each other's work.
  Before sharing a folder, start `nestpro library-check <test folder>
  --minutes 10 --text` on two PCs at once: it saves made-up jobs, parts and
  stock changes there and reports PASS or FAIL in plain words. It refuses a
  real Library.
- **Job folders** are the Library's, the same on every PC: `job-folders
  --mode=list` shows the tree, and `new`, `rename`, `delete` (empty folders
  only) and `move` (a saved job or a folder) change it, as the Job Explorer's
  Library section does. **Part folders** are a tree of their own:
  `part-folders` does the same for library parts (`move --library_part=…`),
  as the Library window's Products tab does.
- Agents may create and change jobs, use the Library, and write programs to the
  export folder. They never send anything to a machine and don't change
  settings (machines, posts, materials are set in the app).

**MCP server** (`nestpro mcp`, stdio): every operation is a tool, `render`
also returns the picture, and the server tells the agent how a CAM programmer
works through a job. Register it with the agent, for example:

```bash
claude mcp add nestpro -- "C:\Program Files\Sheet Nesting\nestpro.exe" mcp     # Claude Code, Windows
claude mcp add nestpro -- /path/to/app/target/release/nestpro mcp             # Claude Code, Linux
```

Claude Desktop (`claude_desktop_config.json`):
`{"mcpServers": {"nestpro": {"command": "C:\\...\\nestpro.exe", "args": ["mcp"]}}}`

How it is built: `app/src/engine/` holds the job rules (shared with the window),
`engine/ops.rs` the list of operations the CLI and the MCP tools are made from,
`engine/render.rs` the pictures, `engine/session.rs` opening/saving/undo.

## Contributing

Bug reports, test files and code are all welcome. See [CONTRIBUTING.md](CONTRIBUTING.md):
it explains how to keep customer data out of public issues, the checks to run, and the
one-time [contributor agreement](CLA.md).

## License

[PolyForm Small Business License 1.0.0](https://polyformproject.org/licenses/small-business/1.0.0)
— see `LICENSE`.

- **Free** for any business, sole traders included, with fewer than 100 people
  (employees plus contractors) **and** under about US$1 million revenue last year
  (the 2019 figure, adjusted for inflation). That covers use, changes, and sharing.
- **Bigger companies** need a commercial license from Kurf Labs: write to
  hello@kurflabs.com ([kurflabs.com](https://kurflabs.com)). Shops that contribute
  code may get one free or cheap.

[LICENSING.md](LICENSING.md) answers the common questions in plain words: how to count
people and revenue, groups of companies, evaluation, and home use.

Releases before this change were published under MIT and stay MIT for anyone
who already has them.
