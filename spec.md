# PolarEffects — Specification

**Status:** Draft v0.1 · **Date:** 2026-09-27

This document says what PolarEffects does and the rules it keeps. `plan.md`
says in what order it gets built. `CLAUDE.md` is the operational guide for
working in the code. Decisions are cited as D## and milestones as M## (both
defined in `plan.md`).

---

## 1. Purpose and scope

### 1.1 Purpose

PolarEffects builds a sailing polar for one specific boat. The user gathers
evidence about how the boat sails:

- ORC velocity predictions for the boat and its sister ships,
- existing polars in Expedition or Adrena format,
- historical race tracks of the boat, from YellowBrick, Geovoile, Blue Water
  Tracks, or GeoJSON/CSV files.

The app puts every track position against reanalysis wind, waves and current.
It turns each track into a polar segment, lets the user inspect, filter and
edit every source in 3D, blends the enabled sources into one polar, and
exports it for routing software.

### 1.2 Users

Navigators, routers and performance analysts who already know what a polar
is and use Expedition, Adrena or a similar router. The app does not teach
sailing. It does explain every control through tooltips, help topics and the
feature search.

### 1.3 Platforms

One code base, built for:

| Platform | Rust target |
|---|---|
| macOS Apple silicon | `aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` |
| Linux x64 | `x86_64-unknown-linux-gnu` |
| Windows x64 | `x86_64-pc-windows-msvc` |
| Windows ARM64 | `aarch64-pc-windows-msvc` |

Windows ARM64 is a target that VectorEffects does not ship. It rules out any
C dependency that does not cross-compile cleanly to it (D6).

### 1.4 Architecture

The app copies VectorEffects' architecture (D1):

- Tauri 2 shell. Rust owns the whole domain. React 19 + TypeScript + Vite is
  a view layer only.
- **The back end is Rust only.** No Python, Node or browser sidecars, and no
  headless browser for scraping (D2).
- IPC types are generated with ts-rs into `ui/src/generated/`. All `invoke`
  calls go through `ui/src/ipc.ts`.
- Errors cross IPC as `{ kind, message }`, with messages built as "could not
  <action> <subject>: <cause>".
- One plain stylesheet with theme role colours applied as CSS variables. The
  system font stack; no web fonts.
- Undo/redo is a command history in `pe-core`.

The crate layout is in `CLAUDE.md`.

### 1.5 Non-negotiable invariants

These are repeated from `CLAUDE.md`, which is authoritative:

1. Sources are immutable; edits are overlays.
2. The blend is derived, never stored as truth.
3. Fetched environment samples are project data; rendered views are not.
4. Nothing is fetched that the user did not ask for, and nothing reaches in.
5. Export is deterministic and byte-reproducible.
6. Interaction stays fast; imports and fetches may be slow.
7. Every string is translatable and every control is findable.

### 1.6 Out of scope for v1

Live tracking, routing, VPP computation from hull measurements, sail
crossover charts, polars per sail, instrument log import (NMEA/Expedition
logs: see §14), multi-boat projects.

---

## 2. Vocabulary

| Term | Meaning |
|---|---|
| **Project** | One attempt to build one polar for one boat. Saved as `.wpsproj`. |
| **Source** | Anything that contributes to the polar: an ORC polar, an imported polar file, or a track. |
| **Polar** | Boat speed (BSP) as a function of true wind angle (TWA) and true wind speed (TWS), on a TWA × TWS grid. |
| **Sample** | One track position with its derived heading and speed and the wind, wave and current found for it. A dot in the plots. |
| **Polar segment** | The partial polar derived from one track's samples. It covers only the TWA/TWS cells the track visited. |
| **Overlay** | A user change stored beside a source: exclusions, cell overrides, filters, colour, weight, visibility. |
| **Blend** | The polar computed from all enabled sources with their overlays. The thing that is exported. |
| **Event** | A race on a tracker (one YellowBrick race key, one Geovoile race, one Blue Water race). One event can supply several tracks. |

---

## 3. Application shell

### 3.1 Start screen

Shown when no project is open. Same layout and component structure as
VectorEffects' start screen (`StartScreen.tsx`): a centred panel with

- **New project**: opens the new-project form (§4.2) inline.
- **Open…**: native file picker filtered to `*.wpsproj`.
- **Recent projects**: the ten most recent, name and path, newest first,
  with **Clear**. A missing file is shown greyed with "Not found" and removed
  on click after a confirmation.
- **Recovered work**: autosaves left by a crash (§4.5), if any.
- Header controls: language picker, Help, Settings.

**Styling** is VectorEffects' structure with a different default palette
(D8). The default theme is **Harbour**:

| Role | Value |
|---|---|
| `--bg` | `#2e3f55` |
| `--surface` | `#253447` |
| `--panel` | `rgba(37, 52, 71, 0.96)` |
| `--inset` | `#1f2c3c` |
| `--raised` | `#2e3f55` |
| `--hover`, `--active` | `#36597a` |
| `--border` | `#5b7fa3` |
| `--border-subtle` | `#3a506a` |
| `--text` | `#d6e6f5` |
| `--muted` | `#b3c9de` |
| `--accent` | `#8fb8de` |
| `--highlight` | `#e8f1fa` |
| `--warning` | `#edbd8d` |
| `--error` | `#e9a7a3` |
| `--selected-bg` | `#8fb8de` |
| `--selected-ink` | `#1f2c3c` |
| `--flash` | `#ff8a1f` (feature-search highlight, same in every theme) |

The other bundled themes are ported from VectorEffects (Midnight, Ocean,
Plum, Ember, Paper) with the same token set. A Custom theme editor is
deferred (§14).

### 3.2 Project window

```
┌────────────────────────────────────────────────────────────────────────┐
│ [Project ▾]  Fastnet polar •        [Map | 3D | Compare]   [? search] ⚙ │
├──────────────┬───────────────────────────────────────────┬─────────────┤
│ ◀ ORC polars │                                           │ Sources     │
│   search…    │                                           │ ■ ORC Bxx   │
│   added list │         centre stage:                     │ ■ Exp file  │
│ ▸ Polar files│         world map (default),              │ ■ Track 1   │
│   import…    │         3D polar, or compare              │ □ Track 2   │
│ ▸ Tracks     │                                           │─────────────│
│   + YB / GV  │                                           │ Polar plot  │
│   + BWT / file                                           │ (2D, dots)  │
│   track list │                                           │ TWS [12] kn │
├──────────────┴───────────────────────────────────────────┴─────────────┤
│ status / hints / job progress                                          │
└────────────────────────────────────────────────────────────────────────┘
```

- **Title bar**: the project menu, the project name (click to rename), a dirty
  dot, the stage switcher, the help search (a search box and a "?" button
  that opens the help window) and settings (Cmd/Ctrl-,).
- **Project menu**: New…, Open…, Open Recent ▸, Save, Save As…, Close, each
  with `data-feature="project:*"`. Standard shortcuts (Cmd/Ctrl-N, O, S,
  Shift-S, W). Cmd/Ctrl-Z and Shift-Z undo and redo outside text fields. The
  native menu has no Close Window item, so Cmd/Ctrl-W always means Close
  project.
- **Left navigation**: collapsible as a whole (◀) and per section. Three
  sections, in this order: ORC polars (§5), Polar files (§6), Tracks (§7).
  Collapse state is remembered per user, not per project.
- **Centre stage**: one of Map (default, §9.1), 3D (§10) or Compare (§11).
- **Right panel**: the source list (§8) and the 2D polar plot (§9.2).
  Collapsible.
- **Status bar**: hints, errors, and a progress line for running jobs, with
  Cancel.

### 3.3 Unsaved changes

The VectorEffects save guard is copied unchanged (D9):

- Dirty state lives in Rust. Every mutation bumps a revision and sets
  `dirty`; save clears it. A new project starts dirty.
- New, Open, Open Recent, Close and quitting the app go through
  `mayReplaceProject`. It asks **Save / Don't save / Cancel**. Cancel is the
  default and is taken by Escape and by clicking outside.
- Choosing Save on a never-saved project opens Save As. Cancelling that file
  dialog aborts the whole operation.
- The back end refuses to drop a dirty project unless the call passes
  `discard_unsaved = true`, so no caller can lose work by forgetting to ask.
- Quitting (the Quit item, Cmd/Ctrl-Q, the platform's own quit) and the
  window's close button are stopped in Rust while the project is dirty; Rust
  emits `app://quit-requested`, the frontend runs the same guard, and exits
  through `quit_app(discard_unsaved)`, which Rust checks like any other
  discard. "Don't save" also drops the recovery snapshot.
- A running job (scrape, reanalysis fetch, GRIB export) belongs to the
  project. Replacing the project asks to cancel the job first.

### 3.4 Settings (global)

Stored in `settings.json` in the platform config directory
(`directories::ProjectDirs::from("com", "PolarEffects", "PolarEffects")`),
beside the recent list. Every field has a default; a file that fails to parse
falls back to defaults, and it is read field by field, so one unreadable or
out-of-range value costs only that preference. Settings are not stored in
projects. Each change is validated and saved by Rust at once; a refused value
or a failed write leaves the previous setting in place.

- **Language** (§3.5). Also shown on the start screen.
- **Theme**.
- **Units**: boat and wind speed (kn default, m/s, km/h), wave height (m,
  ft), distance (nm, km).
- **Autosave**: recovery (default), save, off.
- **Chunk cache**: location, size limit (default 20 GB, 1–2000), Clear cache
  button, current size. The chunks live in a `chunks` folder inside the chosen
  location (the platform cache directory by default), and Clear removes only
  that folder, never the rest of a folder the user pointed at. The least
  recently used chunks are removed to stay under the limit. A chunk being
  written is a `.partial` file beside its name; opening the cache removes
  only those older than an hour, since a younger one may be another running
  PolarEffects writing it.
- **Network**: request concurrency (default 8, 1–32), timeout (default 60 s,
  5–600 s).
- **Map projection** (§9.1), remembered here rather than in the project.
- **Polar plot dot band** (§9.2): how far from the plot's wind speed a
  sample may be and still be drawn, ±0.25 to ±5 kn (default ±1 kn). A display
  preference; it never changes the blend.

### 3.5 Language, help and tooltips

The VectorEffects i18n system is copied (D10):

- English text is the key: `t("Import {count} tracks", {count})` via
  `useT()`, `msg("…")` for tables built at module load.
- Catalogues in `ui/src/i18n/locales/<lang>/<area>.ts`. **v1 ships English,
  French and German.** The language picker is in Settings and on the start
  screen; changing it relabels everything immediately, including the native
  menu, which Rust rebuilds from a translated table (`menu.rs`) on every
  language change.
- `coverage.test.ts` fails on a missing, unused or untranslated key, and on
  JSX text, `title` or `aria-label` that skips `t`.
- A glossary per language fixes sailing terms (TWA, TWS, BSP, VMG, polar,
  "abattée", "Wende"…) so the same concept is always the same word.
- **Tooltips** on every control, translated. Each tooltip shows the shortcut
  if there is one.
- **Help** is a reference window of translated topics, one per area. Every
  topic exists in every language (`topics.test.ts`).
- Strings that come from Rust (error kinds, job names, history labels) are
  keyed and translated in the UI, as in VectorEffects.

### 3.6 Feature search

Copied from VectorEffects (spec §5.8 there), with the highlight in orange:

- Cmd/Ctrl-F or the search box in the title bar. Results appear **as the user
  types**, ranked by: translated label, keywords, description, help topics.
  Matching is case- and accent-insensitive in the **current language**.
- Every control has `data-feature="<area>:<name>"` and a registry entry
  (`ui/src/help/features/<area>.ts`) with label, description, keywords, help
  topic and reveal steps (`panel:left`, `section:tracks`, `stage:3d`,
  `dialog:settings` …).
- Choosing a result runs the reveal steps (opening the panel, section, stage
  or dialog that hides the control), then highlights the control with a
  flashing **orange** outline (`--flash`, `#ff8a1f`). The outline follows the
  control for 2.4 s or until the next pointer down. Motion is reduced under
  `prefers-reduced-motion`.
- `features.test.ts` fails if a tag has no registry entry or an entry has no
  tag, and if any feature is not found by its own translated label in every
  language.
- The start screen has no search box (as in VectorEffects); its Help button
  opens the help window. The help window's own controls (search, topic list,
  related pages, Close) are registered, revealed by `help:open`.
- Exempt from the registry, and only these: dynamic per-item list rows — one
  recent or recovered project, its Discard button, one Open Recent entry —
  whose containers carry the id instead; and the answer buttons of transient
  dialogs (Save / Don't save / Cancel, confirmations), since nothing can
  reveal a question that has not been asked.
- Errors are shown translated by their `kind` (§1.4); Rust's English message
  is kept only as the tooltip, and an unknown kind shows a translated generic
  line.

---

## 4. Project model

### 4.1 Contents

```
Project
  id, name, created, schema_version
  boat: { name, notes }                        // free text
  grid: { twa: [deg], tws: [kn] }               // output grid, §12.2
  blend: BlendSettings                          // §12
  sources: [Source]                             // ordered as the user sees them
  next_id                                       // ids never exceed 2^53 − 1
Source
  id, kind, label, colour "#rrggbb", visible, weight (0..=2, default 1)
  kind = Orc { record: OrcRecord }                     // copied from the catalogue
       | PolarFile { format, file_name, polar: Polar } // parsed at import
       | Track { track: Track }
  overlay: Overlay
Overlay
  cell_overrides: [(twa, tws, bsp)]             // polar edits, §10.4
  excluded_cells: [(twa, tws)]                  // polar nodes removed, §10.3
  excluded_samples: [SampleId]                  // track dots removed, §10.3
  filters: SampleFilters                        // track sources only, §7.6
Track
  origin: { tracker, event_url, event_title, boat_id, boat_name, sail_no } | File { name }
  fixes: [Fix { t, lat, lon, cog?, sog? }]      // as imported
  derivation: DerivationSettings                // §7.4
  statistic: median | mean | p75 | p90          // §12.1
  samples: [Sample]                             // §7.5, derived + fetched env
  env_meta: { datasets: [(name, version, fetched_at)] }
```

### 4.2 New project

The form has one required field, **Project name**. Optional: boat name and
notes. The grid and blend settings start from defaults (§12.2) and are
changed later from the Blend section of the source list. A new project opens
straight into the project window with the Map stage.

### 4.3 File format: `.wpsproj`

Same container rules as VectorEffects' `.veproj` (D11):

- A ZIP (deflate) with:
  - `META-INF/version`: the schema version in plain text, so a newer file can
    be refused before parsing.
  - `project.json`: canonical JSON, stable key order, everything except bulk
    track data.
  - `tracks/<track id>.json`: one entry per track, holding its fixes and
    samples. Kept separate so `project.json` stays small and diffable.
- Every entry has a fixed timestamp, so saving an unchanged project twice
  gives identical bytes.
- No rendered images and no blend results (invariant 2). A test fails on any
  unexpected entry, and opening refuses an archive that holds one (or lacks a
  track's entry) rather than dropping it silently on the next save. The one
  exception is operating-system litter from re-zipping or browsing the file
  (`__MACOSX/…`, `.DS_Store`, `Thumbs.db`), which is ignored on open and
  never written back.
- Floats go through the canonical helpers.
- Saves are atomic: write `<name>.wpsproj.tmp`, then rename. The same
  helper writes `settings.json` and autosave manifests.
- The document is validated as it will be read back (after canonical
  rounding) before anything is written, so a project that could not be
  reopened is refused at save time.
- Save As appends `.wpsproj` unless the name already ends in exactly that
  (`Race.v2` → `Race.v2.wpsproj`).
- Migrations: `MIGRATIONS: &[(u32, fn(&mut Value) -> Result<()>)]`, keyed by
  the version each step migrates from. Newer files are refused with a message
  naming both versions.

### 4.4 Recent projects

The ten most recent, stored in `settings.json`, updated on open and save.

### 4.5 Autosave and recovery

Copied from VectorEffects: in recovery mode the dirty project is written to
`<data_dir>/autosave/<project id>.wpsproj` every 60 s or every 50 history
entries. A clean save or close deletes it. After a crash the start screen
offers it under Recovered work; it opens dirty at its original path. The
snapshot is written beside its final name and renamed into place under the
session lock, after checking that the project is still open, unsaved and
not on its way out, so a Save, Close or "Don't save" during the write never
leaves stale work to be offered back. A running environment fetch dirties
the project as it writes, so its results are snapshotted too.

### 4.6 Undo and redo

Every document change is a `Command` with `apply` and `undo`, in a history of
200 entries. Drags (3D node edits, weight sliders) coalesce into one entry.
Selection, camera and stage changes are not recorded. Imports are undoable
(undo removes the source). Fetched environment data survives undo of a
filter change because it lives on the track, not in the command.

---

## 5. ORC polars

### 5.1 Catalogue

The ORC catalogue is **embedded in the app** (D3):

- Built by `tools/orc-catalogue-builder` from jieter/orc-data's per-boat files
  (`site/data/<country>/<sail>.json`, about 18,000 boats across all years,
  MIT licence). `ALL2025.json` is not used as a source of records: it is
  Python `repr`, not JSON, and holds 34 boats.
- The builder reads the files **from the checkout's `HEAD` commit** through
  `git`, not from the working tree: orc-data has file names that differ only
  in case (`FIN/FIN71.json`, `FIN/Fin71.json`), which a macOS or Windows
  checkout collapses into one.
- The builder keeps: sail number, country, name, type/model, builder,
  designer, year, the certificate year, the size fields (LOA, beam, draft,
  displacement, sail areas, crew), GPH/OSN, and the VPP (`angles`, `speeds`,
  BSP per angle per TWS, beat and run angles and VMGs). Speed axes differ by
  year (6–20 kn up to 2023, 6–24 kn in 2024, 4–24 kn from 2025); all are kept
  as given. Sizes of 0 (orc-data's "no spinnaker") are kept as absent.
- The sail number is shown once with its country: orc-data's `GBR/GBR1124`
  is `GBR 1124`; its stand-in for a missing number (`GBR/_3`) is empty.
- **Certificate year.** The per-boat files do not state it. The builder reads
  it from the VPP's wind-speed axis (4–24 kn: the latest yearly list from
  2025 holding the boat, else the year the checkout's Makefile fetches; 6–24
  kn: 2024; 6–20 kn: the latest of the yearly lists `ALL2019`–`ALL2023`
  holding the boat's sail number, of which only the sail numbers are read).
  When none of this identifies a year it is left empty rather than guessed
  (about 700 boats, all certificates older than 2019).
- **Dropped records.** The builder reports every file it leaves out and why
  (not JSON, VPP shape or axis wrong, a speed that is negative or above
  60 kn, values finer than 0.01, a file identical to another). At orc-data
  `c2ca870c` it read 18,141 files, kept 18,135 and dropped 6: four with
  negative boat speeds and two exact duplicates.
- Output: a compact binary (`catalogue.bin`, postcard records in one LZ4
  block, speeds and angles as whole hundredths, which is exact for
  orc-data's two decimals) in `crates/pe-orc/data/`, about 5.7 MB. It is
  embedded in the binary and decoded and indexed on first use (about 0.2 s).
  Its small uncompressed header holds the orc-data commit, the commit's date
  and the build date (the commit's date unless `SOURCE_DATE_EPOCH` is set,
  so rebuilding one commit is byte-identical); About shows them (the native About panel), and the
  ORC polars section's footer shows the certificate count and commit date.
- Refreshing the catalogue is a developer task and a new release, never a
  run-time fetch (invariant 4).

### 5.2 Search

- One search box that matches **across all fields**: boat name, sail number,
  country, model/type, builder, designer, year built and certificate year.
  Tokens are ANDed, and each must match the **start of a word** of some field,
  so `farr 40 2023` finds Farr 40s from 2023 and `arr` finds nothing. The name
  is also indexed without its punctuation, so `oneil` finds O'Neil.
- Results update **as the user types**, within 30 ms of each keystroke, best
  first. Ranking: exact sail number (with or without its country), then name
  prefix, then model prefix, then other token matches; within each, newer
  certificates first, then by name.
- Case- and accent-insensitive (decomposed accents and `İ` included); accepts `GBR1124`, `GBR 1124`, `GBR/1124`.
- Each result shows name, sail number, model, year, builder, the certificate
  year and a small polar thumbnail (light, medium and strong wind: the
  certificate's wind speeds nearest 6, 12 and 20 kn). Filters: year built
  (from, to; a boat without a year is left out while either is set) and
  country. An empty query with no filter lists nothing; with a filter it
  lists what the filter admits. At most 50 results are listed, with the
  total count.
- Measured in M5 on the development machine (Intel i9, debug build): 188
  keystrokes over the full catalogue, median 1.2 ms, p99 2.4 ms.

### 5.3 Adding and removing

- **Add** puts a copy of the record in the project as an ORC source with the
  next palette colour, labelled with the boat's name (else its model, else
  its sail number); one undo takes it back out. Any number can be added;
  adding the same certificate twice asks first. orc-data gives no
  certificate number, so "the same certificate" is the same country, sail
  number, name, model, year built and certificate year. A result the project
  already holds is marked **Added**.
- The added list sits under the search. Each entry has **Remove**
  (undoable) and shows its colour, sail number, model and certificate year.
- The ORC VPP converts to a polar on the ORC angles (52°–150°) plus the
  beat and run angles per TWS, on the certificate's own wind speeds. At a
  beat or run angle only its own wind speed has a value, the VMG divided by
  the cosine of the angle (by the cosine of 180° less the angle for a run);
  where it falls on a table angle, the table's value stays. Nothing is
  invented outside the angles ORC gives; the blend (§12) handles gaps.

---

## 6. Polar files

- **Import…** opens a native picker allowing **multiple files**. Each file
  becomes one source, labelled with its file name.
- Formats, detected from content (not only the extension):
  - **Expedition** (`.txt`): `!` comment lines; each row is `TWS` followed by
    `TWA BSP` pairs, whitespace-separated; rows may have different lengths.
  - **Adrena / grid** (`.pol` tab-separated, `.csv` semicolon- or
    comma-separated): top-left cell `TWA\TWS`, `TWA/TWS` or `TWA`; first row
    TWS values; first column TWA values; cells BSP in knots.
- A parse error names the file, line and column and imports nothing from that
  file. Other files in the same batch still import. Everything a batch
  imports is one undo entry.
- Speeds above 60 kn or negative values are refused as a format error.
- Reading details:
  - Text is UTF-8 (with or without a BOM), UTF-16 with a BOM, or else
    Latin-1; CRLF, CR and LF line endings are all accepted. Blank lines and
    `!` comment lines are skipped in both layouts.
  - Expedition rows keep their own angles: the polar's TWA axis is the union
    of every row's angles, and a row that has no point at an angle leaves that
    cell empty. Rows and pairs may come in any order.
  - Table cells may be quoted, trailing separators are ignored, an empty cell
    or a short row is an empty cell, and a row longer than the header is an
    error. In tab- and semicolon-separated files (and Expedition), a comma
    inside a number is its decimal point (`5,25`).
  - Angles in (180°, 360°] are the port side and are folded (`360 − TWA`);
    where a file gives both sides of one angle, the two speeds are averaged.
    Angles below 0° or above 360° are refused. The same TWS twice, or the
    same written TWA twice in a row or table, is refused.
  - Files over 4 MB, or with more than 512 distinct angles or wind speeds,
    are refused as not being polars.
- Writing (used by export, §12, and pinned by golden files): `\n` line
  endings, axis values with at most two decimals and no trailing zeros,
  boat speeds with exactly two decimals. Expedition: a `!` comment line, then
  one tab-separated row per TWS holding only the cells that have a value.
  Adrena: `TWA\TWS` top-left, tab-separated, empty cells left blank. CSV: the
  same with semicolons.
- Imported files are listed with colour, format and axes. **Remove** is
  undoable.

---

## 7. Tracks

### 7.1 Track list

The Tracks section lists every track source: colour, boat name, event title,
date range, sample count, and an environment status (not fetched, queued,
fetching n %, ready, partial, failed). Each has Show on map, Remove, Refetch
environment, and Export reanalysis GRIB (§7.8).

Buttons above the list: **YellowBrick…**, **Geovoile…**, **Blue Water…**,
**File…**. Several events and several files can be imported into one
project, and several boats from one event.

### 7.2 Tracker import

All three trackers share one dialog flow (D4):

1. The user pastes the event URL. The app resolves the event and shows its
   title and dates.
2. **The full tracks of all boats are downloaded**, then shown as a table:
   boat name, sail number, model/class, division, fix count, status. A map
   preview shows the tracks.
3. The user selects one or more boats (search box over name, sail number,
   model). **Import** creates one track source per selected boat.
4. The downloaded event is kept in memory for the session, so a second import
   from the same event does not download again.

**YellowBrick** (host `yb.tl`, CDN `cf.yb.tl`):

- Race key = the first path segment of `https://yb.tl/<key>` (also accepts
  `cf.yb.tl`, `app.yb.tl` viewer links, and a bare key).
- `GET /JSON/<key>/RaceSetup` (ISO-8859-1): title, start, stop, `teams[]`
  (id, name, sail, model, type, tags).
- `GET /BIN/<key>/AllPositions3`: the full history, decoded in Rust. Big-endian.
  `u8 flags` (bit0 altitude, bit1 DTF, bit2 lap, bit3 percent), `u32 refTime`,
  then per team `u16 id`, `u16 count` and `count` moments newest first. A
  moment whose first byte has the high bit clear is absolute
  (`u32 t, i32 lat, i32 lon` in 1e-5 degrees, then the optional fields);
  otherwise it is a delta from the previous (newer) moment
  (`u16 dt & 0x7FFF, i16 dlat, i16 dlon`, then optional deltas).
- YellowBrick gives no speed or course; both are derived (§7.4).
- Fallback if the binary fails to decode: `GET /<key>.kml`.
- Some keys return 5xx; the dialog says so and offers Retry.
- The app.yb.tl purchase flow and any device credential are never used (D5).

**Geovoile** (hosts `*.geovoile.com`):

- The user pastes the viewer URL (`https://<sub>.geovoile.com/<root>/tracker/`
  or `/viewer/`, with an optional leg). The app reads the viewer HTML for
  `rooturl`, `resourcesurl` and the four 24-bit hwx seeds.
- `GET <root>tracker/resources/versions/v` (a JS object literal), then
  `resources/[leg<N>/]<type>/v<ver>` for `config` (XML: boats, names, sail
  numbers, colours, legs) and `tracks` (JSON). If `resourcesurl` is set, the
  path is `<resourcesurl>[leg<N>_]tracker_<type>.hwx?v=<ver>`.
- The hwx decoder (xorshift keystream from the seeds + LZSS) is written in
  Rust from the verified algorithm in `plan.md` Appendix A. Seeds are always
  parsed per site.
- Tracks: `loc` holds `[t, lat·1e5, lon·1e5]` then `[dt, dlat, dlon]`
  deltas. `reports` supplies official heading and speed where present; they
  are used as `cog`/`sog` when non-zero.
- Only the modern `tracker/` generation (about 2016 on) is supported. Flash
  (`.hwz`) and 2012–2015 HTML trackers are refused with a clear message.

**Blue Water Tracks** (host `api.bluewatertracks.com`):

- The user pastes `https://race.bluewatertracks.com/<slug>`; the slug is the
  last path component.
- `GET https://api.bluewatertracks.com/api/race/<slug>`: `race.boats[]`
  (boat_id, boatName, sailNo, design, handicaps) and `positions[]` as GeoJSON
  Features with `properties.{boat_id, date, sog, cog}`.
- SOG and COG are provided and used.

### 7.3 File import

- **GeoJSON**: a FeatureCollection of Point features, or LineString features
  with per-vertex times in `properties.times` / `properties.coordTimes`.
  Point properties recognised (case-insensitive): `time`/`timestamp`/`date`
  (ISO 8601 or epoch seconds/ms), `cog`/`heading`/`hdg`/`course`,
  `sog`/`speed`/`bsp`/`stw`, `boat`/`name`. Features are grouped into one
  track per boat name.
- **CSV**: header row required. A column-mapping step guesses time, lat, lon,
  heading, speed and boat columns from the header and a preview, and lets the
  user correct them. Time formats: ISO 8601, epoch seconds or ms, or a user
  format string (`%Y %y %m %d %H %M %S %f %b %z`). Speed unit selectable
  (kn default, m/s, km/h, mph). The separator (`,` `;` or tab) is taken from
  the header, quoted fields follow RFC 4180, and a decimal comma is read when
  the separator is not a comma. The file is re-read as the mapping changes,
  so the dialog shows the boats (or the error) the mapping gives.
- Times without a zone are UTC. A zone offset must be within ±14:00;
  24:00:00 is read as the next day's midnight, and no later time of day is.
  Longitudes written 0–360 are accepted and
  folded to [−180, 180); in-range values are stored bit for bit as read.
  GeoJSON MultiLineString features with one list of times per line (as GPX
  converters write) are read too. A file that is not UTF-8 is read as
  Latin-1.
- A file that cannot be read is reported with its line and column (CSV and
  JSON syntax) or its feature number (a GeoJSON feature that is not a usable
  position), and nothing of it is imported; the other files still import.
- Several files can be chosen at once. A file with several boats offers the
  same boat picker as a tracker event. All the tracks of one import are one
  undo entry. The import summary gives, per track, the positions kept, how
  many were out of order and sorted, how many duplicate times were merged,
  and how many headings and speeds were given or derived.

### 7.4 Deriving heading and speed

Every track is reduced to timestamped fixes. Then, per fix:

- **Heading.** If the track supplies heading or COG, use it. Otherwise use
  the heading *at* the fix (central difference): the circular mean of the
  great-circle bearing arriving from the previous fix (its final bearing)
  and the one leaving for the next (its initial bearing). The first fix
  uses the initial bearing to its one neighbour, the last the final bearing
  from it, so on a long leg each end has its own heading (along 60°N from
  0° to 10°E: 85.7° leaving, 94.3° arriving). Where the positions used are
  the same place (a stationary boat), or the two bearings point opposite
  ways, there is no heading.
- **Speed.** If the track supplies SOG or boat speed, use it. Otherwise
  (distance(prev, this) + distance(this, next)) / (t_next − t_prev).
- Duplicate timestamps are merged, and out-of-order fixes are sorted, before
  derivation. Both are reported in the import summary. Of several fixes at
  one time the first in the file is kept; a heading or speed only a later
  one gives is kept with it.
- The track records per fix whether heading and speed were **given** or
  **derived**, so the user can filter on it.

Derivation settings (per track, editable later, undoable): maximum gap
between neighbours used for a central difference (default 3 h, 1 s–24 h). A
neighbour further away in time than the gap is not used: a fix with one
usable neighbour uses that one (as the first and last fixes do), and a fix
with none gets no derived values. And whether to prefer given or derived
values; with "derived", a fix with nothing to derive from keeps its given
value. Changing either re-derives every sample's heading and speed, as one
undo entry that restores the previous values exactly.

### 7.5 Environment for each sample

After import, every track is matched against reanalysis, as a background
job (§7.7). The import opens the fetch's pre-flight (§13) for the tracks it
added, with Fetch as its default answer; Not now leaves them "not fetched"
until Refetch environment.

| Quantity | Dataset | Variable |
|---|---|---|
| 10 m wind u, v | **WeatherBench2** ERA5, hourly 0.25° (D12) | `10m_u_component_of_wind`, `10m_v_component_of_wind` |
| Significant wave height | **ARCO-ERA5**, hourly 0.25° | `significant_height_of_combined_wind_waves_and_swell` |
| Mean wave direction (from) | ARCO-ERA5 | `mean_wave_direction` |
| Surface current, total incl. tides | see §7.5.1 | u, v |

- Values are interpolated bilinearly in space and linearly in time. Wind and
  current interpolate their u and v; wave direction interpolates as a unit
  vector. Land cells (NaN) are ignored in the stencil; if all four are NaN the
  value is missing.
- **WeatherBench2 ends on 2023-01-10.** For times after its last hour, wind
  comes from ARCO-ERA5 (same variables, same grid, same model). Each sample
  records which dataset supplied it. See D12.
- From these, each sample stores: TWS (kn), TWD (from), TWA (0–180),
  tack side, Hs (m), wave direction (from), wave angle relative to the bow,
  current speed and direction (toward), and dataset ids. Units are
  converted once, on ingest (1 m/s = 3600/1852 kn). The track records each
  dataset's name, version (the store's dated name) and fetch time, and
  whether its current has tides; each sample records which supplied its
  wind, waves and current, and whether the fetch has answered for it
  (what Refetch resumes from).
- TWA is the angle between the heading and where the wind comes from; the
  wind over the starboard side is starboard tack, and head to wind or dead
  downwind is neither. The wave angle is measured off the bow (0° head
  seas, 180° following), the bow pointing along the heading through the
  water where there is a current and the ground heading otherwise.
- The environment is stored as found; everything that relates it to the
  boat's motion (TWA, tack, the corrected values, the wave angle) is
  recomputed from the stored values whenever the motion changes, so a
  change of derivation settings (§7.4) needs no fetch and undoes exactly.
- **Current correction** (D13). Wind and current are both ground-relative;
  a polar is water-relative. When a current is available:
  - boat velocity through the water = ground velocity − current;
    BSP = its magnitude and heading = its direction (leeway ignored);
  - wind over the water = wind − current; TWS and TWA use it.
  Both raw (ground) and corrected values are stored. A project-level toggle,
  **Correct for current** above the track list, chooses which feed the
  polar (default: corrected where current exists); it is one undo, as is
  **Include Stokes drift** (§7.5.1), which applies to the next fetch.

#### 7.5.1 Current source

Every tier below includes the tide (settled from the stores' own metadata
on 2026-09-28, `plan.md` §6, Q7); they differ in resolution and in how the
tide is modelled. The app takes each sample from the first source in this
chain that covers its time and place (D20):

1. **Regional tidal reanalyses**, 1993 to a few months ago. They are tidally
   forced models, so `uo`/`vo` is already the total current:
   - NW European Shelf, `NWSHELF_MULTIYEAR_PHY_004_009`,
     `cmems_mod_nws_phy-uv_my_7km-2D_PT1H-i` (7 km, hourly, 40–65N,
     20W–13E; int16 with `scale_factor`).
   - Iberia–Biscay–Ireland, `IBI_MULTIYEAR_PHY_005_002`,
     `cmems_mod_ibi_phy-cur_my_0.027deg_PT1H-m` (1/36°, hourly means,
     26–56N, 19W–5E).
2. **Global merged surface current**, 2020-11-01 onward:
   `GLOBAL_ANALYSISFORECAST_PHY_001_024`,
   `cmems_mod_glo_phy_anfc_merged-uv_PT1H-i` (1/12°, hourly). It carries
   `uo` (circulation), `utide` (FES2014 tide), `vsdx` (Stokes drift) and
   `utotal = uo + utide + vsdx`. The app uses `uo + utide` by default, with a
   setting to include Stokes drift.
3. **GlobCurrent** (`MULTIOBS_GLO_PHY_MYNRT_015_003`, the store VectorEffects
   reads), 1993 onward, 0.25°, **with tide (FES2022)**: in version 202411
   both the multi-year and near-real-time stores describe `uo` as "absolute
   geostrophic velocity + depth Ekman + tide velocity". The multi-year
   series (`cmems_obs-mob_glo_phy-cur_my_0.25deg_PT1H-i`) is read first and
   the near-real-time one (`…_nrt_…`) after it ends; `uo`/`vo` are read at
   the level nearest the surface (0 m of 0 and −15 m).

Each dataset record says whether its current includes the tide; every
tier above does, so no sample is marked "no tide" today. The "leave out
currents without tide" filter stays for a tier without one should it ever
be added.

A position is looked for in a tier only if it lies inside that tier's grid
and time axis (the regional tiers only inside their boxes, so a race
elsewhere never opens those stores). A tier whose value is missing there —
land, fill, or a chunk the archive does not have — passes the position to
the next tier. A tier whose store will not open (the archive down, a
version withdrawn) is left out for ten minutes, its positions going on to
the next tier; the fetch goes on and reports "left out a current source
that would not open" on the status line with the reason. The stores read (versions as recorded on each sample):
`cmems_mod_nws_phy-uv_my_7km-2D_PT1H-i_202112`,
`cmems_mod_ibi_phy-cur_my_0.027deg_PT1H-m_202511`,
`cmems_mod_glo_phy_anfc_merged-uv_PT1H-i_202211`,
`cmems_obs-mob_glo_phy-cur_{my,nrt}_0.25deg_PT1H-i_202411`. Integer-packed
arrays (the NW Shelf and GlobCurrent `int16`) are unpacked with their own
`scale_factor`/`add_offset`, and their fill value is missing.

All are Copernicus Marine ARCO zarr v2 on `s3.waw3-1.cloudferro.com`, read
anonymously with the same blosc/LZ4 codec as ERA5. For sampling along a
track the **geoChunked** stores are used (one chunk covers months at one
place). Their fill value is about 9.97e36, and a missing chunk comes back as
HTTP 403 or 404; both mean "no data".


### 7.6 Sample filters

Per track source, editable any time, undoable:

- **Wave height** range (m) and **wave direction**: relative to the bow
  (head, bow, beam, quarter, following sectors, or a custom range) or
  absolute (from-direction range).
- Current speed range.
- TWS range, TWA range.
- Time window (defaults to the race start and finish when the tracker gives
  them, so pre-start and post-finish motoring is out).
- Minimum BSP (default 1 kn) and maximum BSP.
- Manoeuvres: exclude fixes where heading changes more than N° (default 30°)
  between neighbours.
- Given versus derived heading/speed.

Filtered-out samples stay in the project and appear dimmed in the plots when
"Show filtered" is on.

- A sample lacking the value an active filter reads (no wind under a TWS
  range, no speed under the minimum BSP) is filtered out: nothing shows it
  passes. With no filter on a quantity, a missing value does not matter.
- The manoeuvre filter compares a sample's heading with each neighbour's and
  takes the larger change; a neighbour without a heading, or further away
  in time than the track's maximum gap, is ignored.
- Wave sectors, off the bow (0° head seas): head below 30°, bow 30–60°, beam
  60–120°, quarter 120–150°, following from 150°.
- BSP, TWS and TWA are the water-relative (current-corrected) values where
  they exist and the project uses them (§7.5), the ground values otherwise.
- Every filter is edited in a track's details in the Tracks section: time
  window, BSP range, manoeuvres and given versus derived (M8), and TWS,
  TWA, wave height and current speed ranges, the wave direction (sectors,
  an angle off the bow, or a compass range from–to clockwise) and "leave
  out currents without tide" (M9). Each change is one undo.

### 7.7 Jobs

Scraping, reanalysis fetches and GRIB exports are **jobs**: they run on a
worker pool, show progress in the status bar and the track list, and can be
cancelled. A cancelled or failed fetch keeps whatever samples completed
(status "partial") and can be resumed with Refetch. Jobs for tracks from the
same event share the chunk cache, so the second boat of a race costs almost
nothing.

The environment fetch in detail (M9):

- One runner takes the queued tracks one after another, so a second boat
  reads the chunks the first one just cached instead of downloading them at
  the same moment. Within a track, samples go in batches of at most three
  sampling intervals of track time (3 h hourly, 9 h 3-hourly) and 400
  samples; each batch's chunk reads run on a pool of the network
  concurrency setting (§3.4), each chunk fetched and decoded once.
- Each finished batch is written into the project at once. Cancel stops at
  the next chunk read, keeping every finished batch. Each sample's values
  depend only on its own time and place, so a resumed fetch ends exactly
  where an uninterrupted one would.
- Refetch fetches the samples still missing; on a ready track, or at a
  different interval or Stokes-drift choice than the last fetch (both
  recorded on the track), it starts over. Starting over first clears every
  sample's wind, waves and current (raw, corrected and which datasets) and
  the track's dataset records, so a restart that is cancelled leaves a
  track partly fetched at the new settings, never a mix of two fetches.
- The status bar shows the running track, its progress and how many wait,
  with Cancel fetch (all); a track's details have Cancel fetch for it. A
  failure is reported on the status line and leaves "partial" (or "failed"
  when nothing finished).
- A job belongs to the project it was started for: New, Open, Close and
  opening recovered work first ask to cancel a running fetch, and a result
  for a track that was removed or changed meanwhile is dropped.
- Reads: a transient failure (a 5xx or 429 answer, a timeout, a dropped
  connection) is retried three times, pausing 0.5, 1 and 2 s; anything else
  fails at once. A retry pause ends at once on Cancel. A body over 64 MB is
  refused. A cached chunk that arrived but fails to decode is removed from
  the cache and fetched once more; a failed read is not. Opened archives
  are kept for the session, so only the first track pays their metadata
  requests.

### 7.8 Reanalysis GRIB export

Per track: **Export reanalysis GRIB…** writes the 10 m wind (u, v) along the
track to a `.grib2` file:

- Area: the track's bounding box plus a 2° margin, on the native 0.25° grid,
  handling the antimeridian.
- Times: every hour from the first fix to the last.
- Options: include wave height and direction, include current.
- **Writer** (D14): the VectorEffects approach, native Rust with a fixed
  message template. Sections 0–8 are laid out once with constant values
  (shape of earth 6, template 3.0 lat/lon grid, template 4.0, template 5.0
  simple packing at 16 bits, no bitmap unless NaN is present). Per message
  only the reference time, forecast hour, grid corners, parameter and data
  array change. Ported from `ve-grib::writer` with regional grids added.
- The data comes from the chunk cache; missing hours are fetched first.
- Output is byte-reproducible (invariant 5) and validated by ecCodes in CI.

---

## 8. Source list

The right panel lists every source (ORC, file, track) in one list:

- Colour swatch: click to change (a palette of 16 plus a custom picker). New
  sources take the next unused palette colour.
- Visibility toggle: **hidden sources are excluded from the blend and from
  every plot** (D15). This is the one "include/exclude" switch.
- Weight slider (0–2, default 1).
- Label (rename in place), kind icon, and a count (cells for polars,
  samples/used samples for tracks).
- Actions: Edit (opens the 3D stage focused on this source, §10.4), Compare
  (§11), Remove.
- Drag to reorder (display order only).
- A **Blend** entry at the top represents the current blend: colour, show or
  hide, and a Blend settings button (§12).

---

## 9. 2D views

### 9.1 Map

The default centre stage. A WebGL2 world map with the VectorEffects basemap
(Natural Earth land and coastlines, embedded; no tiles). Projections:
equirectangular and orthographic, chosen on the map and remembered in the
settings. Drag pans the flat map or turns the globe; the wheel zooms, about the
pointer on the flat map and about the centre on the globe; "Fit the world"
shows everything again. The globe's land is drawn through an equirectangular
mask texture, so no land triangle folds across the horizon.

- Every visible track is drawn in its source colour. Filtered-out fixes are
  drawn dimmed (excluded ones less so). A track crossing the antimeridian is
  one continuous line: longitudes are unwrapped along each track.
- Hovering a fix shows time (UTC), BSP and heading (each marked given or
  derived), TWS, TWA, Hs and current, in the display units; a value not yet
  known shows as a dash.
- Selecting samples in a polar view highlights them on the map, and a box
  selection on the map (Shift-drag) selects those samples in the polar views.
  "Show on map" in the 3D view and in the track list switches to the map and
  frames what was asked for. "Fit the tracks" frames every visible track.
- Tracks travel from Rust as one packed binary buffer (layout in
  `pe-app/src/map_tracks.rs` and `ui/src/map/trackPacket.ts`, pinned by a
  shared fixture), drawn in one WebGL2 draw call.
- Wind barbs at the hovered time are a stretch goal (§14).

### 9.2 Polar plot

A classic 2D polar diagram in the right panel (and full-size as a Map stage
overlay on demand):

- A TWS slider (or "all") chooses the slice (D21). One value draws one curve
  per visible polar source (tracks are not polar sources) and the blend, all
  at that TWS; "all" draws one curve per visible source per wind speed that
  source's own grid has, rather than a shared slice — the classic diagram of
  several TWS curves at once. A curve is read at bilinear interpolation
  (`pe-polar`, no extrapolation) across the source's own TWA axis; dots are
  for every sample whose TWS is within ±1 kn (configurable in Settings,
  §3.4) of the slice, in their track's colour, excluded ones hollow and
  selected ones ringed. A sample without wind has no place in the polar and
  is not drawn. A "Filtered" toggle adds the filtered-out samples, dimmed.
- Everything uses source colours. The blend is drawn thicker.
- Hover shows the source, TWA, TWS and BSP.
- Full size opens the same plot as a large overlay owned by the Map stage
  (D21): the panel's "Full size" button switches to the Map stage and opens
  it; its own button, Escape, or switching stage again closes it.

---

## 10. 3D polar view

### 10.1 Scene

- Built with **three.js**, bundled by Vite from npm (no CDN; invariant 4)
  (D16).
- Axes: TWA (angle), TWS, BSP. Two layouts, toggled:
  - **Polar tower** (default): x = BSP · sin(TWA), y = BSP · cos(TWA),
    z = TWS. Each TWS is a classic polar curve; stacked they make a surface.
  - **Cartesian**: x = TWA, y = TWS, z = BSP.
- Every visible polar source is a translucent surface in its colour, with the
  grid lines drawn. Every sample is a dot in its track's colour. The blend is
  an opaque surface.
- Orbit, pan, zoom; preset cameras (top, side, isometric); an axis
  legend with the display units. Top looks down the vertical axis (in the
  tower, the classic polar diagram with every TWS stacked); side looks
  across it, so each TWS is a level; the axes carry tick labels in the
  display speed unit.
- Every surface is the source's own grid over its own axes: nothing is
  resampled or extrapolated, and an empty cell is a hole.
- Must hold 60 fps with 200,000 dots and 20 surfaces on the reference
  machines (§13), using instanced points.
- Rust assembles the scene and sends it as one packed little-endian binary
  buffer (`f32` coordinates, `u32` ids and flags), not JSON; the layout is
  documented once on each side (`pe-app/src/polar3d.rs`,
  `ui/src/polar/scenePacket.ts`) and pinned by a shared fixture.

### 10.2 Showing all known points

"All dots" means every known (TWA, TWS, BSP) triple: every sample from every
visible track, and the grid nodes of every visible polar source. Toggles: show
samples, show polar nodes, show surfaces, show filtered samples (dimmed),
colour dots by source / by Hs / by current speed / by time.
Polar nodes have no environment, so they keep their source colour in every
colour mode. A mode, or "show filtered", with nothing to show (no sample has
that value, no sample is filtered) is offered disabled with a tooltip saying
why.

### 10.3 Excluding dots

- Click selects a dot; Shift-click adds (or removes a dot already
  selected); a lasso or box (in screen space) selects many, Shift adding.
  The Rotate, Lasso and Box tools choose what a drag does; a click selects
  in each, and Escape clears the selection. The selection survives a
  refetch of the scene (dots are matched by source and grid cell, or by
  sample id).
- Only dots that are drawn are counted in the selection info and acted on
  by Exclude, Include and "show on map": a selected dot that a toggle hides
  (samples off, or a filtered sample while "show filtered" is off) takes no
  part.
- **Exclude** removes the selection from the blend. Excluded sample dots are
  drawn hollow; excluded ORC or file nodes are drawn as crosses. **Include**
  restores them. Both are undoable.
- For polar sources, excluding a node stores an exclusion in the overlay; the
  node's cell is then empty for that source in the blend. The surface still
  shows the source as imported, with the excluded node drawn as a cross.
  One Exclude or Include over nodes of several sources is one undo entry;
  nodes already in the asked state are left alone.
- Selection info: count, mean TWS/TWA/BSP, source breakdown, "show on map".

### 10.4 Editing one source

Every source can be edited on its own (D17):

- **Edit** on a source focuses it: its surface is fully opaque, others fade
  (kept visible for context, toggle to hide).
- The editable surface is the source's polar: the imported grid for files,
  the VPP grid for ORC, and the **polar segment** for tracks (§12.1).
- Edit tools:
  - drag a node vertically (BSP) with the mouse; Shift snaps to 0.05 kn;
  - a **table editor** (TWA rows × TWS columns) beside the 3D view, with the
    same cells; typing a value is an edit;
  - scale a selection by a percentage;
  - smooth a selection (3×3 kernel on the grid);
  - reset a selection to the source value.
- Edits are stored as `cell_overrides` on the overlay (invariant 1) and shown
  with a marker in both the 3D view and the table. "Reset all edits" clears
  them.
- **Every edit updates the blend and every open view** (3D, compare, 2D polar
  plot) within the §13 budget. Edits drive a recompute in Rust; the UI never
  computes a blend itself.

---

## 11. Compare

- The Compare stage takes two operands, A and B. Each is any source's polar,
  any track's polar segment, or the current blend.
- 3D: A and B as surfaces, plus a **difference surface** coloured by
  ΔBSP = A − B with a diverging scale (A faster / B faster), centred on zero,
  with its range shown in the legend. A toggle shows Δ as percent of B.
- **Overlap**: only cells where both A and B have a value are coloured.
  Cells covered by only one are drawn in neutral grey with a pattern and
  counted in the summary.
- Summary: overlap cell count, mean and max |Δ|, the TWS/TWA regions where A
  is faster and where B is faster, and a 2D Δ heat map (TWA × TWS).
- A swap button, and each operand's picker lists sources by colour and name.

---

## 12. Polar segments and the blend

### 12.1 Polar segment from a track

- Samples that pass the filters and are not excluded are binned onto the
  project output grid (§12.2), folding port and starboard.
- Per cell: statistic of BSP, selectable per track — median, mean, 75th or
  **90th percentile (default)**. A polar describes good sailing, not average
  sailing, so an upper percentile is the default (D18).
- A cell needs at least **5 samples** (setting) to have a value. Cells below
  that are empty.
- The segment keeps, per cell, its sample count and spread; these drive the
  blend weight and appear in tooltips.

### 12.2 Output grid

Project setting, editable in Blend settings:

- TWS default: 4, 6, 8, 10, 12, 14, 16, 20, 25, 30 kn.
- TWA default: 0, 30, 35, 40, 45, 52, 60, 70, 75, 80, 90, 100, 110, 120,
  135, 150, 160, 170, 180.
- Polar sources are resampled onto this grid (bilinear in TWA × TWS, never
  extrapolated beyond the source's axes). The two linear steps run along TWA
  within each bracketing TWS column first, using that column's own known
  angles, then along TWS; so a ragged Expedition polar reads each wind speed
  between its own points, and a query below a column's first or above its
  last angle has no value.

### 12.3 Blend

For each cell (TWA, TWS) of the output grid, over the **visible** sources
that have a value in that cell:

    blend = Σ wᵢ · bspᵢ / Σ wᵢ

- `wᵢ = source weight × confidence`. Confidence is 1 for polar sources and
  `min(1, n / n_full)` for track segments, with n the cell's sample count and
  `n_full` default 30.
- Cell overrides apply before blending; exclusions remove the cell.
- Cells with no source stay empty, then are filled in order by: interpolation
  along TWA within the same TWS; then along TWS; the 0° row is 0 kn.
- Optional smoothing (off by default) over the filled grid.
- The blend shows its own coverage: cells with direct evidence versus filled.
- The blending rule is isolated in `pe-polar::blend` behind one function so it
  can be changed later without touching views (asked before changing, see
  `CLAUDE.md`).

---

## 13. Performance budgets

On the reference machines (M1 MacBook Air; a 2020 Intel i5 Windows laptop;
a Snapdragon X Windows ARM64 laptop):

| Operation | Budget |
|---|---|
| Start screen visible after launch | < 1.5 s |
| ORC search result update per keystroke | < 30 ms |
| Blend recompute after an edit (20 sources, 200k samples) | < 50 ms |
| Edit to every open view updated | < 100 ms |
| 3D view with 200k dots, 20 surfaces | 60 fps |
| Open a project with 50 tracks | < 2 s |
| Reanalysis for a 5-day race, cold cache | reported, not budgeted; progress every second |
| Second boat from the same event | < 5 s (cache hit) |

The chunk-cache cost of reanalysis is real: one hour of one variable is a
global chunk of about 1.7–3.3 MB (measured in M3: wind 3.3 MB, wave height
1.8 MB, wave direction 1.7 MB). A 5-day race needs about 120 hours × 4
variables ≈ 1.2 GB. Currents from the geoChunked stores are cheap by
comparison (about 0.8 MB per variable per 1.3° × 0.7° box per six months).
The job shows the expected download size before it starts and lets the user
pick hourly or 3-hourly sampling (D19). Hourly is the default; when the
hourly download would exceed half the chunk-cache size limit (a long ocean
race), 3-hourly is preselected instead. The estimate counts the ERA5 hours
the samples need at each interval (wind 3.3 MB × 2, wave height 1.8 MB,
wave direction 1.7 MB per hour), leaves out the chunks already in the
cache, and adds about 0.8 MB per current variable per geoChunk box and half
year crossed. 3-hourly reads 00, 03, … 21 UTC and interpolates linearly
between them; currents are always hourly.

---

## 14. Deferred (post-v1)

- Other trackers found in `tracker-index` (Kwindoo, RaceQs, TracTrac,
  GeoRacing, Estela, TackTracker, Metasail, iSail, YachtBot, Kattack).
- Instrument log import (NMEA, Expedition logs) as a track source with
  measured BSP and wind.
- Polars per sail and crossover charts.
- Custom theme editor.
- Wind barbs and wave fields on the map.
- Linux ARM64 build.
- Additional languages (VectorEffects ships nine).

## 15. Resolved questions

See the decisions log in `plan.md` §5.
