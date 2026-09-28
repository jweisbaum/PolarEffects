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
  dot, the stage switcher, the help search and settings.
- **Project menu**: New…, Open…, Open Recent ▸, Save, Save As…, Close, each
  with `data-feature="project:*"`. Standard shortcuts (Cmd/Ctrl-N, O, S,
  Shift-S, W).
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
- A running job (scrape, reanalysis fetch, GRIB export) belongs to the
  project. Replacing the project asks to cancel the job first.

### 3.4 Settings (global)

Stored in `settings.json` in the platform config directory
(`directories::ProjectDirs::from("com", "PolarEffects", "PolarEffects")`),
beside the recent list. Every field has a default; a file that fails to parse
falls back to defaults. Settings are not stored in projects.

- **Language** (§3.5). Also shown on the start screen.
- **Theme**.
- **Units**: boat and wind speed (kn default, m/s, km/h), wave height (m,
  ft), distance (nm, km).
- **Autosave**: recovery (default), save, off.
- **Chunk cache**: location, size limit (default 20 GB), Clear cache button,
  current size.
- **Network**: request concurrency (default 8), timeout.

### 3.5 Language, help and tooltips

The VectorEffects i18n system is copied (D10):

- English text is the key: `t("Import {count} tracks", {count})` via
  `useT()`, `msg("…")` for tables built at module load.
- Catalogues in `ui/src/i18n/locales/<lang>/<area>.ts`. **v1 ships English,
  French and German.** The language picker is in Settings and on the start
  screen; changing it relabels everything immediately, including the native
  menu.
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
  tag.

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
  next_id
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
  track's entry) rather than dropping it silently on the next save.
- Floats go through the canonical helpers.
- Saves are atomic: write `<name>.wpsproj.tmp`, then rename.
- Migrations: `MIGRATIONS: &[(u32, fn(&mut Value) -> Result<()>)]`, keyed by
  the version each step migrates from. Newer files are refused with a message
  naming both versions.

### 4.4 Recent projects

The ten most recent, stored in `settings.json`, updated on open and save.

### 4.5 Autosave and recovery

Copied from VectorEffects: in recovery mode the dirty project is written to
`<data_dir>/autosave/<project id>.wpsproj` every 60 s or every 50 history
entries. A clean save or close deletes it. After a crash the start screen
offers it under Recovered work; it opens dirty at its original path.

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
  MIT licence). `ALL2025.json` is not used: it is Python `repr`, not JSON, and
  holds 34 boats.
- The builder keeps: sail number, country, name, type/model, builder,
  designer, year, the certificate year, the size fields (LOA, beam, draft,
  displacement, sail areas, crew), GPH/OSN, and the VPP (`angles`, `speeds`,
  BSP per angle per TWS, beat and run angles and VMGs). Speed axes differ by
  year (6–24 kn up to 2024, 4–24 kn from 2025); both are kept as given.
- Output: a compact binary (`catalogue.bin`, bincode + zstd-free lz4) in
  `crates/pe-orc/data/`, loaded lazily on first use. Its build date and the
  orc-data commit it came from appear in About.
- Refreshing the catalogue is a developer task and a new release, never a
  run-time fetch (invariant 4).

### 5.2 Search

- One search box that matches **across all fields**: boat name, sail number,
  country, model/type, builder, designer, year. Tokens are ANDed, so
  `farr 40 2023` finds Farr 40s from 2023.
- Results update **as the user types**, within 30 ms of each keystroke, best
  first. Ranking: exact sail number, then name prefix, then model prefix, then
  other token matches, then newer certificates first.
- Case- and accent-insensitive; accepts `GBR1124`, `GBR 1124`, `GBR/1124`.
- Each result shows name, sail number, model, year, builder and a small
  polar thumbnail. Filters: year range, country.

### 5.3 Adding and removing

- **Add** puts a copy of the record in the project as an ORC source with the
  next palette colour. Any number can be added; adding the same certificate
  twice asks first.
- The added list sits under the search. Each entry has **Remove**
  (undoable) and shows its colour.
- The ORC VPP converts to a polar on the ORC angles (52°–150°) plus the
  beat and run angles per TWS. Nothing is invented outside the angles ORC
  gives; the blend (§12) handles gaps.

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
  file. Other files in the same batch still import.
- Speeds above 60 kn or negative values are refused as a format error.
- Imported files are listed with colour, format and axes. **Remove** is
  undoable.

---

## 7. Tracks

### 7.1 Track list

The Tracks section lists every track source: colour, boat name, event title,
date range, sample count, and an environment status (not fetched, fetching
n %, ready, partial, failed). Each has Show on map, Remove, Refetch
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
  format string. Speed unit selectable (kn default).
- Several files can be chosen at once. A file with several boats offers the
  same boat picker as a tracker event.

### 7.4 Deriving heading and speed

Every track is reduced to timestamped fixes. Then, per fix:

- **Heading.** If the track supplies heading or COG, use it. Otherwise use
  the initial great-circle bearing from the previous fix to the next fix
  (central difference). The first and last fixes use the one neighbour they
  have.
- **Speed.** If the track supplies SOG or boat speed, use it. Otherwise
  (distance(prev, this) + distance(this, next)) / (t_next − t_prev).
- Duplicate timestamps are merged, and out-of-order fixes are sorted, before
  derivation. Both are reported in the import summary.
- The track records per fix whether heading and speed were **given** or
  **derived**, so the user can filter on it.

Derivation settings (per track, editable later, undoable): maximum gap
between neighbours used for a central difference (default 3 h; beyond it the
fix gets no derived values), and whether to prefer given or derived values.

### 7.5 Environment for each sample

After import, every track is matched against reanalysis automatically, as a
background job (§7.7):

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
  current speed and direction (toward), and dataset ids.
- **Current correction** (D13). Wind and current are both ground-relative;
  a polar is water-relative. When a current is available:
  - boat velocity through the water = ground velocity − current;
    BSP = its magnitude and heading = its direction (leeway ignored);
  - wind over the water = wind − current; TWS and TWA use it.
  Both raw (ground) and corrected values are stored. A project-level toggle
  chooses which feed the polar (default: corrected where current exists).

#### 7.5.1 Current source

No single anonymous global dataset has total current including tides for
every year since 2015. The app takes each sample from the first source in
this chain that covers its time and place (D20):

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
   reads), 1993 onward, **without tides**. Samples from this tier are marked
   "no tide" and can be filtered.

All are Copernicus Marine ARCO zarr v2 on `s3.waw3-1.cloudferro.com`, read
anonymously with the same blosc/LZ4 codec as ERA5. For sampling along a
track the **geoChunked** stores are used (one chunk covers months at one
place). Their fill value is about 9.97e36, and a missing chunk comes back as
HTTP 403 or 404; both mean "no data".

Adding tides to tier 3 from a harmonic atlas (FES2014) is possible in pure
Rust but blocked on licensing: see `plan.md` §6, Q1.

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

### 7.7 Jobs

Scraping, reanalysis fetches and GRIB exports are **jobs**: they run on a
worker pool, show progress in the status bar and the track list, and can be
cancelled. A cancelled or failed fetch keeps whatever samples completed
(status "partial") and can be resumed with Refetch. Jobs for tracks from the
same event share the chunk cache, so the second boat of a race costs almost
nothing.

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
equirectangular and orthographic.

- Every visible track is drawn in its source colour. Filtered-out fixes are
  drawn dimmed.
- Hovering a fix shows time, BSP, heading, TWS, TWA, Hs and current.
- Selecting samples in a polar view highlights them on the map, and a box
  selection on the map selects those samples in the polar views.
- Wind barbs at the hovered time are a stretch goal (§14).

### 9.2 Polar plot

A classic 2D polar diagram in the right panel (and full-size as a Map stage
overlay on demand):

- A TWS slider (or "all") chooses the slice. Curves for every visible polar
  source and the blend at that TWS; dots for every sample whose TWS is within
  ±1 kn (configurable) of the slice.
- Everything uses source colours. The blend is drawn thicker.
- Hover shows the source, TWA, TWS and BSP.

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
- Orbit, pan, zoom; preset cameras (top, side per TWS, isometric); an axis
  legend with the display units.
- Must hold 60 fps with 200,000 dots and 20 surfaces on the reference
  machines (§13), using instanced points.

### 10.2 Showing all known points

"All dots" means every known (TWA, TWS, BSP) triple: every sample from every
visible track, and the grid nodes of every visible polar source. Toggles: show
samples, show polar nodes, show surfaces, show filtered samples (dimmed),
colour dots by source / by Hs / by current speed / by time.

### 10.3 Excluding dots

- Click selects a dot; Shift-click adds; a lasso or box (in screen space)
  selects many.
- **Exclude** removes the selection from the blend. Excluded sample dots are
  drawn hollow; excluded ORC or file nodes are drawn as crosses. **Include**
  restores them. Both are undoable.
- For polar sources, excluding a node stores an exclusion in the overlay; the
  node's cell is then empty for that source in the blend.
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
  extrapolated beyond the source's axes).

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
global chunk of about 2–3.5 MB. A 5-day race needs about 120 hours × 4
variables ≈ 1.3 GB. Currents from the geoChunked stores are cheap by
comparison (about 1 MB per 1.3° × 0.7° box per six months). The job shows the expected download
size before it starts and lets the user pick hourly or 3-hourly sampling
(D19).

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
