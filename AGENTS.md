# Agent guide — nestpro (Sheet Nesting)

A source-available sheet-nesting CAM program for cutting machines (laser, plasma, oxyfuel,
waterjet) in Rust + egui. It covers the daily cutting workflow (jobs → parts → sheets →
nesting → technology → machining → CNC program) and reads the files of Lantek Expert, the
commercial program it is built to replace in a working shop. Read `README.md` for the
folder layout.

**Goal = the same features and behaviour as the program it replaces, not database
compatibility.** The app keeps its own storage (the Library: `.snjob` jobs, products,
sheet stock). Importing the other program's files (`.nxp`, `.mec`, DXF, machine files)
and reading its database for migration are in scope.

## Two repos: public app, private research

| Repo | Holds | Where it lives locally |
|---|---|---|
| `KurfLabs/nestpro` (public) | The app, tests, `app/MECHANICS.md` (behaviour rules in our own words), tools, docs | this folder |
| `KurfLabs/nestpro-research` (private, never public) | How the rules were found: feature dossiers, code addresses and evidence, decompiled notes, reference screenshots, notes about the shop's own setup, and the project's history before the split | `research/` inside this folder (git-ignored here, its own git repo) |

If `research/` is missing, clone it: `git clone https://github.com/KurfLabs/nestpro-research research`.
Its own `AGENTS.md` explains how features are studied. Read it before studying a feature.

A rule crosses from research to the public repo **only as behaviour**: what the app does,
in our words, with its rule ID (e.g. `SS11`) and whether it was observed or estimated.
The evidence for that ID (addresses, internal names, screenshots) stays in
`research/evidence/`.

## Keeping the public repo clean

Everything committed here will be published. `tools/public-check.sh` enforces this; it runs
in `tools/check.sh`, in the pre-commit hook (`tools/install-hooks.sh` once per clone) and in
CI. Never bypass it with `--no-verify`. GitHub text (PR and issue bodies, comments) is public too
and no hook sees it: write it to a file and run `tools/public-check.sh --text <file>` before posting.

Never commit, anywhere in the public repo (code comments and test names included):
- code addresses, decompiler names (`FUN_…`, `local_…`), analysis evidence IDs, or the
  names of the other program's internal functions, classes and binaries as sources.
  Cite the rule ID instead: `// SS11: stock is capped at 32 500.`
- the other program's manual or help text, screenshots, icons or artwork. Describe
  behaviour in our own words and draw icons ourselves.
- shop data: customer, job, part, drawing or PO numbers, people's names, machine set-ups,
  server, database or share names, Windows user paths, e-mail addresses. Tests and
  examples use made-up data (`DEMO-001`, `Acme`, `C:\Lantek\Expert`, `srv\SQL`).
- images that haven't been reviewed. Take screenshots with `tools/shot.sh` on demo data,
  look at every pixel for the items above, then add the image's sha256 to
  `tools/public-images.txt`. Day-to-day check screenshots go in `app/shots/`
  (git-ignored).
- `vendor/` (the other program's install, which contains real customer jobs in
  `Expert/LogFile$`), `research/`, `history/`.

Names that must never appear publicly but are too specific to list here (the shop's server,
database, customers) are in `research/public-denylist.txt`; the check reads it when
`research/` is present. Add to it when you meet a new one.

## Platform

- **Windows first.** The app is used on Windows. Never break the Windows build to make
  something work on Linux.
- It must also build and run on Linux. Gate Windows-only features with `cfg!(windows)` /
  `#[cfg(windows)]` and give a clear message elsewhere. Don't stub out the Windows path.
- The other program's data uses Windows paths (`C:\...`, `\`-separated) even on Linux.
  Split on both separators when parsing names from its files.
- Its install folder defaults to `C:\Lantek\Expert`, or comes from the `LANTEK_DIR`
  environment variable. Settings go in `%APPDATA%\nestpro` (Windows) or `~/.config/nestpro`.
  Documents go in `Documents\Sheet Nesting` under `USERPROFILE`/`HOME` (`library::documents_dir()`).

## Commands

```bash
tools/check.sh            # after every change: public check + Linux tests + Windows type-check
tools/shot.sh out.png     # build + screenshot (its own headless Xvfb display); add app flags e.g. --jobs, --part-tab=machining
tools/install-hooks.sh    # once per clone: pre-commit public check
cd app && cargo run       # run it (needs a display)
```

`cargo` is in `~/.cargo/bin`, which may not be on PATH. The scripts add it. The Windows
`.exe` (linking, icon resource) can only be built on Windows.

## Building a feature

1. Study it in the research repo (its `AGENTS.md`) and write the dossier there.
2. Build it here. Code comments explain behaviour and cite rule IDs only.
3. Add each rule the app now follows to `app/MECHANICS.md` (rule ID, the rule in our
   words, observed or estimated) and `app/mechanics.json`
   (`{"name","group","value","source"}`, where `source` is the rule ID). Put the evidence
   for the same ID in `research/evidence/`.
4. Verify: `tools/check.sh`, tests for the rules, a screenshot with `tools/shot.sh`.
   Update `app/ROADMAP.md` and **`FEATURES.md`** (see Rules).
5. Commit the research repo and this repo separately.

## Agent interface (CLI + MCP) — keep it level with the GUI

The app is also driven headless by agents and scripts (`nestpro <operation> job.snjob`,
`nestpro mcp`; see README). When you add or change a feature:

- Put the rule in `app/src/engine/` (a plain function over parts/sheets/spec), and have
  the GUI's `App` method call it — never keep the logic only in a UI method.
- Add or extend an operation in `engine/ops.rs` (`OPS` + its arm in `run`), with
  arguments in shop terms and refusals that say what to do instead.
- Every new ribbon button needs a line in `engine::ops::GUI_MAP`: the operation that does
  the same, or `view:` / `gui:` / `todo:` with a reason. The test
  `every_ribbon_command_is_mapped` fails until it has one.
- Test the operation in `engine/tests.rs` through `Session::call`, like an agent would.
- A new field in a job must go into `JobFile` (with `#[serde(default)]`) so agents see it.

## Rules

- **`FEATURES.md` is the list of what isn't built yet.** Whenever you leave something out,
  guess a rule or value, or find a gap, add a row (next free ID in its area, status, priority,
  source). When you build one, move it to *Done* with the date. A ribbon command agents can't
  reach yet is `todo: [ID] …` in `GUI_MAP`; a test checks the ID is in the list.

- The app's icon is egui's default (credited in `app/assets/CREDITS.md`). Keep it.
- Commit or push only when the user asks.
- Explain results in shop terms (sheets, remnants, parts, cuts) first, code second.

## Agent skills

### Issue tracker

GitHub Issues on `KurfLabs/nestpro`, which are public; `FEATURES.md` stays the list of what isn't built yet. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

### Agent loop

Settings for running a spec's tickets with agents (`babysit-tickets`):

- **Check:** `tools/check.sh`. Pass = exit 0 and a last line `== All checks passed`; any failed
  step exits 1 and names it on a `== FAILED:` line. A second check in the same checkout waits
  for the first.
- **Max implementers:** 2, with `CARGO_BUILD_JOBS=2` each (the build machine has 4 cores).
- **Idle limit:** 45 minutes.
- **Risky paths:** anything that changes a machine program (`app/src/cnc.rs`, `app/src/post.rs`,
  `app/src/blm.rs`, `app/src/kerf.rs`, `app/src/leads.rs`, `app/src/joints.rs`), and the public
  check (`tools/public-check.sh`, `tools/public-images.txt`). The final PR stops for the user.
- **Hotspot files:** `FEATURES.md`, `app/MECHANICS.md`, `app/mechanics.json`, `app/ROADMAP.md`,
  `GUI_MAP` and `OPS` in `app/src/engine/ops.rs`, `app/src/engine/tests.rs`. Add only the IDs a
  ticket assigns; keep both sides' rows on a merge.

Reference lines (this project rebuilds another program):

- **Original:** Lantek Expert v44, its install in `vendor/Expert` (local only, never committed).
- **Study guide:** `research/AGENTS.md`; research skill `reverse-engineer-anything`.
- **Private evidence:** `research/evidence/`. Studies commit to `research/` and don't push it.
- **Public rules:** `app/MECHANICS.md` and `app/mechanics.json`.
- **Published:** behaviour in our words with its rule ID, marked observed or estimated. Never
  evidence (see "Keeping the public repo clean").
