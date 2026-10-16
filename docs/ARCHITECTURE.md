# Architecture

Poof is a Cargo workspace with a strict dependency direction:

```
┌──────────────────────────── crates/poof-app (binary `poof`) ─────────────────────────────┐
│  main.rs ── app.rs (root: routing, sidebar, toasts, dialogs)                            │
│               │                                                                         │
│               ├── screen/overview  screen/explore  screen/cleanup  screen/changes  …    │
│               │        (each: State · Message · update → Action · view)                 │
│               └── widget/ (cards, bars, modal, treemap canvas, chart canvas) · style.rs │
└───────────────────────────────────────────┬─────────────────────────────────────────────┘
                                            │ depends on (never the reverse)
┌───────────────────────────────────── crates/poof-core ──────────────────────────────────┐
│  scan      parallel walker → arena Tree           treemap   pure squarified layout      │
│  cleanup   declarative rule CATALOG → find()      remove    guard + Trash/delete + log  │
│  history   scan Summary, persistence, changes()   monitor   samples, alerts, LaunchAgent│
│  suspects  known "System Data" hogs               volume    capacity via Foundation     │
│  system    tmutil, Full Disk Access, Finder, notifications    settings · paths · time   │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

## Principles

1. **Functional core, imperative shell.** `poof-core` holds all domain logic
   and has no UI dependency. Most of it is pure functions over plain data
   (`treemap::squarify`, `history::changes`, `monitor::evaluate`,
   `remove::guard`), which keeps it exhaustively unit-tested. I/O sits at the
   edges in small, named functions.
2. **The Elm architecture for the UI.** iced enforces *State → view → Message
   → update*. Each screen is a self-contained component that talks to the rest
   of the app only through `screen::Event`s returned in an `Action`. Screens
   never reach into each other. The root `App` routes events, for example
   `Removed` → refresh disk figures and re-run cleanup analysis.
3. **Data over code.** Cleanup targets (`cleanup::CATALOG`) and System Data
   suspects (`suspects::SUSPECTS`) are static tables. Adding a target is a data
   change, not new logic. Overlaps resolve automatically: a broad rule skips
   any path a more specific rule claims.
4. **One guarded path to deletion.** Explore and Clean Up both call
   `remove::remove`, which applies the protected-path guard and writes the
   operations log. There is no second way to delete.
5. **Design tokens, not ad-hoc styling.** `style.rs` defines the palette,
   spacing, radii and type scale for light and dark themes. Views ask for a
   role (`style::card`, `Tone::Warning`), never a colour.

## Key flows

**Scanning.** `explore::State::start` spawns `scan::scan` on a worker thread
(`task::blocking`), sharing an `Arc<Progress>` of atomic counters. A 100 ms
subscription re-renders the progress view while the scan runs; there is no
per-file message traffic. The walker uses rayon to fan out over
subdirectories, `lstat`s every entry, counts allocated blocks, deduplicates
hard links and never crosses file systems. Small files are folded into one
"N smaller files" node per folder, so the tree stays at roughly 100k nodes
even for millions of files. The nested result is flattened once into an
arena (`Vec<Node>` + `NodeId`), with children sorted by size.

**History.** After a scan, `history::Summary::from_tree` records every item of
at least 32 MB, down to eight levels, as JSON in
`~/Library/Application Support/<app id>/history/`. `history::changes` diffs
two summaries deepest-first and subtracts what descendants already explain,
so the reported changes add up to the true total.

**Monitoring.** `poof --check` (run by the optional LaunchAgent every 30 min,
and by the app while it is open; at most one sample per 10 minutes) appends a free-space sample.
`monitor::evaluate` is a pure function that decides whether to alert (a sharp
drop in 24 h, or space below a floor), with a cooldown between alerts.

**Cleanup.** `cleanup::find` evaluates every rule in parallel, measures each
candidate with the same walker that powers scans, and returns `Finding`s. The
UI preselects Safe rules only. Removal goes through `remove::remove`.

## Disk accounting rules

- Sizes are **allocated** bytes (`st_blocks × 512`), not logical length. A
  sparse `Docker.raw` or an evicted iCloud file is counted for what it actually
  occupies.
- Hard links are counted once; symlinks are never followed.
- "Macintosh HD" scans `/System/Volumes/Data`, the writable APFS volume, so
  firmlinked paths such as `/Users` and `/Applications` are counted exactly
  once. Paths are displayed in their familiar form (`/Users/…`).
- Capacity numbers come from `NSURL` resource keys. "Available" includes
  purgeable space, exactly as Finder reports it.

## Adding things

- **A cleanup target:** add a `Rule` to `crates/poof-core/src/cleanup/catalog.rs`
  and pick `Safety::Safe` only if the data is regenerated automatically
  without user impact. The catalog tests check for unique ids and paths that
  stay inside the home folder.
- **A "System Data" suspect:** add a `Suspect` to
  `crates/poof-core/src/suspects.rs`, with advice on how to reclaim the space
  properly.
- **A screen:** create `screen/<name>.rs` with `State`, `Message`, `update` and
  `view`, add a `Screen` variant, and route it in `app.rs`.

Decision records explaining the main choices live in [`docs/adr`](adr).
