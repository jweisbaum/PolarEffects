# PolarEffects — Plan

**Status:** Draft v0.1 · **Date:** 2026-09-27

How PolarEffects gets built, in order. `spec.md` says what it does;
`CLAUDE.md` says how to work in the code. Update a milestone's status in the
same commit that finishes it.

**2026-09-27: First draft.** Spec, plan and CLAUDE.md drafted from the
product description, a survey of VectorEffects, verified vendor formats
(YellowBrick, Geovoile, Blue Water Tracks), the `tracker-index` reference
scrapers, and the ERA5 and Copernicus Marine archives. Open questions in §6.

---

## 1. Sequencing strategy

1. **Scaffold the VectorEffects skeleton first** (M0–M2) so every later
   feature lands in a shell that already saves, translates and is searchable.
   Retrofitting i18n and the feature registry is far more expensive than
   starting with them.
2. **Retire the three technical risks early**, as spikes before their
   features (M3): the reanalysis fetch cost, the hwx and AllPositions3
   decoders, and 3D performance with 200k dots.
3. **Polars before tracks.** The polar core, file import and ORC give a
   working, exportable app (M4–M7) before any network code.
4. **Tracks by source, file first.** File import proves the sample pipeline
   without scraping (M8); then environment (M9); then each tracker (M10–M12).
5. **Editing, blend, compare, export** (M13–M16) once every source exists.
6. **Release on all five targets** from M0 on in CI; signing and packaging in
   M18.

---

## 2. Milestones

### M0 — Workspace, CI on five targets, offline check · **complete**

**Goal:** an empty Tauri app that builds, tests and bundles on every target.

**Deliverables**

- Cargo workspace (edition 2024, resolver 3, pinned toolchain), crates from
  `CLAUDE.md` as stubs, workspace lints (`unsafe_code = "forbid"`, clippy
  `unwrap_used`/`expect_used` deny), `rustfmt.toml`, `clippy.toml` — copied
  from VectorEffects.
- Root npm workspace owning the Tauri CLI and scripts; `ui/` with React 19,
  TypeScript, Vite, Vitest.
- `pe-app` with the `AppError { kind, message }` pattern, one command, ts-rs
  export example and `npm run bindings`.
- `tools/check-offline.sh` adapted: network allowed only in `pe-env` (hosts
  `storage.googleapis.com/weatherbench2`,
  `storage.googleapis.com/gcp-public-data-arco-era5`,
  `s3.waw3-1.cloudferro.com/mdl-arco-`) and `pe-trackers` (`yb.tl`,
  `cf.yb.tl`, `*.geovoile.com`, `api.bluewatertracks.com`); CSP `'self'`.
- GitHub Actions: CI (fmt, clippy, tests, UI typecheck/test, bindings drift,
  offline) on macos-15, macos-15-intel, ubuntu-22.04, windows-2022, and a
  Windows ARM64 build job (`windows-11-arm` runner or cross-build from x64).
  Release workflow with tauri-action for all five targets.

**Acceptance**

- A bundled app starts on all five targets (manual check on the Windows ARM64
  reference laptop).
- `cargo tree` shows no `openssl-sys`, `blosc-src` or other `*-sys` crate
  except those Tauri itself needs.

**Risks:** Windows ARM64 is new relative to VectorEffects. `ring` needs clang
for `aarch64-pc-windows-msvc`; confirm the runner has it.

---

### M1 — Document model, `.wpsproj`, history, project lifecycle · **complete**

**Goal:** the full data model from spec §4 exists, round-trips and is
undoable, before any of it is visible.

**Deliverables**

- `pe-core`: `Project`, `Source`, `Overlay`, `Track` (fixes and samples as
  plain data), IDs, canonical floats, `Command` + `History` (200 entries,
  coalescing).
- `.wpsproj` ZIP I/O with `META-INF/version`, `project.json`,
  `tracks/<id>.json`, fixed entry timestamps, atomic save, migration table.
- Session state in `pe-app`: dirty flag and revision, New / Open / Save / Save
  As / Close with `discard_unsaved` guards, recent list (10), autosave and
  recovery.

**Acceptance**

- Property test: any generated project survives save → load → save with
  byte-identical output.
- Every command has an undo inverse test.
- Opening a file with a newer schema is refused with a message naming both
  versions.

---

### M2 — Shell: start screen, layout, settings, i18n, help search

**Goal:** the app looks and behaves like VectorEffects with the PolarEffects
palette, in three languages, with a working feature search.

**Deliverables**

- Start screen (spec §3.1) with the Harbour theme and ported themes.
- Project window layout (spec §3.2): title bar, project menu, collapsible left
  navigation with three empty sections, stage switcher, right panel, status
  bar.
- Save guard dialog and flow copied from VectorEffects (`saveGuard.ts`,
  `UnsavedChangesDialog.tsx`).
- Settings dialog and `settings.json` (spec §3.4).
- i18n: English, French, German catalogues, glossary files, coverage tests,
  language picker in Settings and on the start screen.
- Feature registry, search-as-you-type, reveal steps, orange flash
  (`--flash`), `features.test.ts`.
- Help window with one topic per area in all three languages.
- World map stage: WebGL2 basemap from VectorEffects' `basemap.bin`,
  equirectangular and orthographic.

**Acceptance**

- Switching language relabels every visible string, tooltip and the native
  menu with no reload.
- Every control in the shell is found by the search in each language and is
  flashed orange after its panel is revealed.

---

### M3 — Risk spikes

**Goal:** measure the three unknowns before building on them. Spike code may
be thrown away; the numbers and fixtures are kept.

**Deliverables**

1. **Reanalysis cost.** Pure-Rust zarr v2 + blosc/LZ4 reader (port
   VectorEffects' `ve-zarr` HTTP store and `blosc.rs`). Fetch u10/v10 from
   WeatherBench2 and swh/mwd from ARCO-ERA5 for a 24 h window; fetch
   `utotal` from the CMEMS geoChunked store at one point. Record bytes,
   seconds and cache hit behaviour. Decide hourly vs 3-hourly default (D19).
2. **Decoders.** Rust decoders for YellowBrick AllPositions3 (Appendix B) and
   Geovoile hwx (Appendix A) passing against recorded fixtures (the Fastnet
   2025 binary; one 2024 and one 2016 Geovoile site).
3. **3D.** A three.js scene with 200k instanced dots and 20 surfaces on the
   three reference machines; lasso selection over 200k dots.

**Acceptance:** a short report appended to this plan's changelog with the
numbers, and a go/no-go on each budget in spec §13.

---

### M4 — Polar core and polar file import

**Deliverables**

- `pe-polar`: `Polar` (TWA × TWS grid, optional cells), bilinear resampling
  without extrapolation, port/starboard folding.
- Expedition and Adrena/grid readers (format sniffing, all header variants,
  errors with line and column) and writers.
- Polar files section: multi-file import, list, remove (spec §6).
- Source list with colours, palette allocation, visibility, weight, rename,
  reorder (spec §8).

**Acceptance**

- Golden-file round trip for each format.
- Malformed-input tests never panic.

---

### M5 — ORC catalogue and search

**Deliverables**

- `tools/orc-catalogue-builder`: reads a jieter/orc-data checkout's
  `site/data/**.json`, normalises fields, writes `crates/pe-orc/data/
  catalogue.bin` with the source commit hash.
- `pe-orc`: lazy load, tokenised search index over all fields, ranking from
  spec §5.2.
- ORC section: search-as-you-type with thumbnails, filters, add, remove.
- ORC VPP → `Polar` conversion (both speed axes; beat and run angles).

**Acceptance**

- Search p99 < 30 ms per keystroke over the full catalogue on the slowest
  reference machine.
- A known certificate's VPP matches the ORC PDF values to 0.01 kn.

**Risks:** the per-boat files' schema changed across years; the builder
must report records it drops and why.

---

### M6 — 2D polar plot

**Deliverables:** the right-panel polar plot (spec §9.2) with TWS slider,
source curves, blend placeholder, hover.

---

### M7 — 3D polar view

**Deliverables**

- `ui/src/polar/` three.js scene (spec §10.1–10.2): polar-tower and
  Cartesian layouts, surfaces per source, instanced dots, legends, cameras,
  colour-by modes.
- Selection (click, shift, lasso, box), exclude/include as undoable commands
  (spec §10.3).
- Rust → UI transport for large arrays: a binary IPC response
  (`tauri::ipc::Response`) of packed `f32`, not JSON.

**Acceptance:** spec §13 3D budgets met on the reference machines.

---

### M8 — File tracks and the sample pipeline

**Deliverables**

- `pe-tracks`: fixes, heading and speed derivation (spec §7.4), GeoJSON
  and CSV import with the column-mapping dialog (spec §7.3), boat picker for
  multi-boat files.
- Track list and map drawing (spec §7.1, §9.1), hover details.
- Samples without environment: dots appear once environment arrives (M9).

**Acceptance**

- Heading and speed derivation tests against hand-computed values, including
  the antimeridian, a stationary boat and a single-fix track.

---

### M9 — Environment: wind, waves, currents

**Deliverables**

- `pe-env`: zarr v2 reader, blosc/LZ4, HTTP store (blocking reqwest +
  rustls/ring), on-disk chunk cache with size limit and LRU eviction, dataset
  coverage from metadata.
- Datasets: WeatherBench2 hourly u10/v10; ARCO-ERA5 u10/v10 (after
  WeatherBench2's end), swh, mwd; current tiers from spec §7.5.1 with
  `scale_factor` handling.
- Sampling: bilinear space, linear time, vector interpolation for
  directions, land-NaN stencil rule.
- Current correction (spec §7.5, D13).
- Job system: worker pool, progress events, cancel, partial results, resume;
  pre-flight download size estimate.
- Sample filters UI (spec §7.6).

**Acceptance**

- Sampled u10 at 2020-07-27 12Z, 50N 5W matches a value decoded
  independently from the same chunk (record the reference in the test).
- A cancelled job leaves a consistent project that resumes to the same
  result as an uninterrupted one.

---

### M10 — YellowBrick

**Deliverables:** `pe-trackers::yellowbrick` (URL → key, RaceSetup with
ISO-8859-1, AllPositions3 decoder, KML fallback), the shared tracker dialog
(spec §7.2), boat picker with map preview, per-session event cache.

**Acceptance:** fixture tests from recorded responses; one live test behind
`PE_TEST_LIVE`.

---

### M11 — Geovoile

**Deliverables:** `pe-trackers::geovoile`: viewer HTML parsing (rooturl,
resourcesurl, seeds), versions file (JS object literal parser), hwx decoder,
config XML, tracks and reports; multi-leg support; clear refusal of Flash and
pre-2016 trackers.

**Acceptance:** fixtures from at least three sites of different years decode
to the boat count and first/last fix recorded by hand from the live viewer.

**Risks:** Geovoile changes its format between editions. The decoder must
fail with "unsupported Geovoile version" rather than produce garbage: check
that the output parses and the first fix is a plausible time and position.

---

### M12 — Blue Water Tracks

**Deliverables:** `pe-trackers::bluewater` (slug from URL, race JSON,
GeoJSON positions with SOG/COG).

---

### M13 — Polar segments and editing

**Deliverables**

- Track → polar segment binning with statistic, minimum count, per-cell
  count and spread (spec §12.1).
- Edit mode per source (spec §10.4): node drag, table editor, scale, smooth,
  reset; overrides as overlay commands.
- Recompute pipeline: an edit invalidates only the affected source and the
  blend; every open view refreshes.

**Acceptance:** spec §13 edit-to-view budget; removing all overrides restores
the source byte-for-byte (invariant 1).

---

### M14 — Blend and export

**Deliverables**

- `pe-polar::blend` (spec §12.3) with coverage output; Blend settings
  (grid, statistic defaults, `n_full`, smoothing, current-correction toggle).
- Export: Expedition `.txt`, Adrena `.pol` and `.csv`, with a preview and
  the choice of which grid to export.

**Acceptance:** golden exports; byte-identical on all five targets (CI
compares hashes).

---

### M15 — Compare

**Deliverables:** Compare stage (spec §11): operand pickers, difference
surface, overlap rules, summary, 2D Δ heat map.

---

### M16 — Reanalysis GRIB export

**Deliverables:** `pe-grib` ported from `ve-grib::writer` and `packing`,
regional template 3.0 grids across the antimeridian, optional waves
(discipline 10, category 0, parameters 3 and 4 for Hs and direction) and
currents; the per-track export dialog (spec §7.8).

**Acceptance:** ecCodes `grib_dump` and wgrib2 read every message with
correct times, grid and values in CI; output hashes identical across
targets.

---

### M17 — Translation and help completion

**Deliverables:** French and German reviewed by a sailor who speaks each
language; help topics complete; every control in the search.

---

### M18 — Release

**Deliverables:** signing and notarisation (macOS), Windows signing, bundles
for all five targets, About with ORC catalogue provenance and data
attributions (ECMWF/Copernicus ERA5, WeatherBench2, Copernicus Marine,
jieter/orc-data MIT), user guide.

---

## 3. Testing strategy

- **Unit**: geodesy, derivation, interpolation, binning, blending, parsers,
  decoders, GRIB packing.
- **Fixtures**: recorded vendor responses and zarr chunks in
  `tests/fixtures/`, small enough to commit (crop chunks in a test helper
  where needed; document how each was recorded).
- **Golden files**: polar exports, GRIB bytes, `.wpsproj` bytes.
- **Property tests**: document round-trip, undo/redo inverses, blend
  invariants (a single visible source blends to itself; weights scale-free).
- **Live tests**: behind `PE_TEST_LIVE=1` and `--ignored`, never in default
  CI; a weekly scheduled workflow runs them to catch vendor changes.
- **UI**: Vitest for logic, i18n coverage, feature registry.

## 4. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| Reanalysis download volume (global chunk per hour per variable) | Slow first import; large disk use | Shared chunk cache, size estimate before start, 3-hourly option, M3 measurements |
| Vendors change formats (Geovoile especially) | Imports break | Plausibility checks, clear "unsupported" errors, weekly live tests |
| Scraping terms of use | Legal or blocking | Only user-initiated single-event fetches, polite concurrency, no credentials (D5); ask before adding trackers |
| WeatherBench2 frozen at 2023-01-10 | No wind for recent races from WB2 | ARCO-ERA5 fallback (D12, Q2) |
| No global tides before 2020-11 | Current correction weaker for older races | Regional tidal reanalyses; "no tide" flag and filter; Q1 |
| Windows ARM64 toolchain | Build failures | CI job from M0; no C deps (D6) |
| 3D performance with large tracks | Janky editing | Instanced points, binary IPC, M3 spike |
| ORC schema drift across years | Missing boats | Builder reports dropped records |

## 5. Decisions log

| # | Decision | Rationale |
|---|---|---|
| D1 | Copy VectorEffects' architecture: Tauri 2, Rust domain, React view, ts-rs bindings, one stylesheet with theme tokens | Requested; proven in the sibling app. Settled with the user 2026-09-27 |
| D2 | Back end is Rust only; no headless browser. Tracker formats decoded in Rust | Requested; the `tracker-index` scrapers use puppeteer, which cannot ship |
| D3 | ORC catalogue built from jieter/orc-data per-boat files and embedded | `ALL2025.json` is Python repr with 34 boats; per-boat files hold ~18k. Settled with the user 2026-09-27 |
| D4 | One tracker dialog flow: URL → download all boats → pick → import | Requested behaviour, identical across trackers |
| D5 | Never use the app.yb.tl purchase flow or any stored device key or cookie | Credentials in the reference repo are not ours to reuse; public JSON/BIN endpoints suffice |
| D6 | No C/C++ dependencies beyond Tauri's own; rustls with ring | Windows ARM64 target and "Rust only" requirement |
| D7 | Speeds stored in knots, directions in degrees | Every polar format and ORC is knots; avoids conversion noise in exports |
| D8 | Default theme "Harbour" (blue); same structure as VectorEffects | Requested "same styling, slightly different colours" |
| D9 | Save guard copied: Save / Don't save / Cancel; back end refuses without `discard_unsaved` | Requested; no path can drop work silently |
| D10 | i18n copied (English as key, coverage tests); v1 = en, fr, de | Requested |
| D11 | `.wpsproj` = ZIP of canonical JSON + per-track entries, fixed timestamps | Diffable, deterministic, recoverable |
| D12 | Wind from WeatherBench2; waves from ARCO-ERA5; wind after 2023-01-10 from ARCO-ERA5 | WB2 wind requested by the user 2026-09-27; WB2 has no waves and ends 2023-01-10. The post-2023 fallback is proposed, see Q2 |
| D13 | Correct boat speed and wind for current; store raw and corrected | Polars are water-relative; tracks are ground-relative |
| D14 | GRIB2 written natively from a fixed in-code message template, ported from `ve-grib` | Requested "same approach as VectorEffects"; VectorEffects keeps the template in code, not in a file |
| D15 | Visibility is the include/exclude switch for the blend | One concept instead of two; requested "hidden or visible to include or exclude" |
| D16 | three.js, bundled locally, for 3D | VectorEffects has no 3D; three.js is mature and works in every Tauri webview |
| D17 | Edits stored as overlays (cell overrides, exclusions) | Invariant 1; reversible, auditable |
| D18 | Track segment cell statistic defaults to the 90th percentile, minimum 5 samples | Polars describe good sailing; the mean undershoots |
| D19 | Reanalysis sampling hourly by default, 3-hourly option | To be confirmed by M3 numbers |
| D20 | Current tiers: regional tidal reanalysis → global merged (uo + utide, 2020-11+) → GlobCurrent without tides | Only anonymous sources; tides everywhere from 2020-11 and in NW Europe/IBI since 1993 |

## 6. Settled before coding started

The user accepted the proposals below on 2026-09-27 ("all good").

- **Q1 — Tides before November 2020 outside NW Europe/IBI:** accept "no
  tide" for those samples, flagged and filterable. FES2014 stays deferred.
- **Q2 — Wind after 2023-01-10:** ARCO-ERA5 supplies it (D12).
- **Q3 — Stokes drift:** excluded by default (`uo + utide`), with a setting
  to include it.
- **Q4 — Blend rule:** weighted mean per cell with sample-count confidence
  (spec §12.3).
- **Q5 — Linux ARM64:** deferred.
- **Q6 — Terms of use:** the user accepts the risk; imports stay
  user-initiated, one event at a time, with polite concurrency.

---

## Appendix A — Geovoile hwx decoding

Verified on 24hultim 2025, Vendée Globe 2024 and Vendée Globe 2016
(2026-09-27).

- The seeds are four 24-bit constants in the first base64 `/C/…` segment of a
  `data:image/png` source in the viewer HTML. They differ per site (2022–2025
  sites: `0x7BC495, 0x4557FA, 0xD56AAF, 0xFF8040`; VG2016:
  `0x88FE88, 0xFE88AA, 0xEECC80, 0xA0A0F0`), so always parse them.
- Keystream (all arithmetic masked to 24 bits):

```
step(): t = x; t ^= (t << 11) & 0xFFFFFF; t ^= (t >> 8) & 0xFFFFFF
        x = y; y = z; z = w; w ^= (w >> 19) & 0xFFFFFF; w ^= t
dec(b): r = b ^ (x & 0xFF); step(); return r
```

- Container:

```
skip buf[0] steps
out_len = dec(buf[1]) << 16 | dec(buf[2]) << 8 | dec(buf[3]); i = 4
loop until out.len() == out_len:
  flags = buf[i] ^ (i & 0xFF) ^ 0xA3; i += 1      // flags are NOT keystream-decoded
  for bit in 7..=0:
    0 → out.push(dec(buf[i])); i += 1
    1 → b = dec(buf[i]); len = (b >> 4) + 3
        off = ((b & 0xF) << 8 | dec(buf[i + 1])) + 1; i += 2
        copy len bytes from out[out.len() - off], byte by byte
```

- Output is UTF-8: XML for `config`, JSON-like JS literals for the rest.
- A working Python reference was written during research; port its test
  vectors into `crates/pe-trackers/tests/fixtures/geovoile/`.

## Appendix B — YellowBrick AllPositions3

Verified against `fastnet2025` (5,726,173 bytes consumed exactly, 444 teams,
positions at Cowes and the Cherbourg finish).

```
u8  flags   bit0 alt, bit1 dtf, bit2 lap, bit3 pc
u32 refTime epoch seconds
repeat until EOF:
  u16 teamId (= RaceSetup teams[].id), u16 count
  count moments, newest first:
    (peek & 0x80) == 0 → absolute:
        u32 t (at = refTime + t), i32 lat, i32 lon
        [i16 alt] [i32 dtf [u8 lap]] [i32 pc]
    else → delta from the previous (newer) moment:
        u16 w (dt = w & 0x7FFF; at = prev.at − dt), i16 dLat, i16 dLon
        [i16 alt] [i16 dDtf [u8 lap]] [i16 pc]
lat, lon = value / 1e5 degrees; dtf in metres
```

The alt, lap and pc layouts are unverified (the Fastnet data had those flags
off); cover them with a fixture from a race that sets them before relying
on them.
