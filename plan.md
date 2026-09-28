# PolarEffects — Plan

**Status:** Draft v0.1 · **Date:** 2026-09-27

How PolarEffects gets built, in order. `spec.md` says what it does;
`CLAUDE.md` says how to work in the code. Update a milestone's status in the
same commit that finishes it.

**2026-09-27: First draft.** Spec, plan and CLAUDE.md drafted from the
product description, a survey of VectorEffects, verified vendor formats
(YellowBrick, Geovoile, Blue Water Tracks), the `tracker-index` reference
scrapers, and the ERA5 and Copernicus Marine archives. Open questions in §6.

**2026-09-28: M3 risk spikes.** Measured on the development machine only
(2019 MacBook Pro, x86_64 Intel i9, Intel UHD 630, home broadband); the three
reference machines of spec §13 are still to be measured. Spike code is kept
as the foundation of `pe-env`, `pe-trackers` and `ui/src/polar/`.

- *Reanalysis* (`PE_TEST_LIVE=1 cargo test -p pe-env --test live`), 24 h at
  50N 5W. WeatherBench2 u10/v10: 3.32 MB per hourly global chunk; cold
  0.287 s/chunk one at a time, 0.097–0.111 s/chunk with 8 in flight
  (≈ 32 MB/s); warm (disk cache) 3 ms/chunk read + decode; open 1.2 s
  (12 requests). ARCO-ERA5 swh 1.77 MB, mwd 1.67 MB, u10 3.32 MB per chunk;
  0.07–0.17 s/chunk with 8 in flight; open 1.05 s (14 requests, coverage
  from `.zattrs` to 2026-09-21). CMEMS merged geoChunked utotal/utide/uo:
  0.88/0.81/0.75 MB per chunk (178 days × 16 × 8 cells), 0.78 s each
  (latency-bound); a missing (all-land) chunk is HTTP 403 and reads as
  missing. **5-day race, hourly:** 120 h × 10.1 MB ≈ 1.21 GB, ≈ 40 s cold
  at this bandwidth, currents ≈ 3 MB per box crossed; 3-hourly ≈ 0.40 GB,
  ≈ 13 s. Second boat of the same event (reopen + 24 cached hours): 0.76 s.
- *Decoders*: YellowBrick AllPositions3 for Fastnet 2025 (5,726,173 bytes,
  444 teams, 714,380 fixes) decodes in 68 ms (debug build); live fetch of
  both responses 2.8 s. Geovoile hwx decodes 24 Heures Ultim 2025 and
  Vendée Globe 2016 (29 boats, 107,459 fixes) with seeds parsed per site;
  Route du Rhum 2022's page gives the same seeds as 2025. **No 2024 site
  fixture**: recording one (New York–Vendée 2024) was blocked in this
  session; to do before M11. The Vendée Globe 2016 viewer page now answers
  HTTP 500, so its seeds come from Appendix A.
- *3D* (`ui/bench3d.html` in headless Chrome, ANGLE Metal on the Intel UHD
  630, 1280 × 800): 200k dots + 20 surfaces hold 60 fps (frame p50/p95/p99
  16.7/16.8/16.8 ms over 600 frames; one start-up hitch of 0.1–0.6 s for
  shader compilation). At 400k dots + 40 surfaces p95 is 33 ms, so the
  headroom is under 2×. Building 200k dots 28 ms in the page (8 ms in Node),
  20 surfaces 8 ms in the page including the three.js objects (1 ms for
  the meshes alone in Node); one lasso over 200k dots 8 ms in the page
  (project 4.5 ms + select 12 ms in Node with a 64-vertex lasso). Chrome is
  not WKWebView or WebView2: the Tauri webviews must be checked in M7.
- *Budgets* (spec §13): 3D 60 fps — **go** on this machine, reference
  machines unmeasured. Second boat < 5 s — **go** (0.76 s for one variable;
  four variables need their opened arrays kept per session in M9, since
  each open costs ≈ 1 s of metadata requests). Reanalysis cold — reported
  as above, with per-chunk completions every ≈ 0.1 s, so progress every
  second is easy. Edit to views < 100 ms — **go** for the 3D side (rebuild
  28 + 8 ms, lasso 8 ms); IPC transfer of 200k samples unmeasured (M7).
  Start screen < 1.5 s, ORC search < 30 ms, blend < 50 ms and opening 50
  tracks < 2 s are not exercised by these spikes: no evidence against,
  measured in M5, M8 and M14. ORC search, measured in M5: p99 2.4 ms per
  keystroke over the full catalogue (debug build, Intel i9) — **go**.
  Blend, measured in M14: 1.0 ms after an edit at 20 sources and 200k
  samples (debug build) — **go**.
- *D19 decided*: hourly stays the default (see §5).

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

### M2 — Shell: start screen, layout, settings, i18n, help search · **complete**

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

### M3 — Risk spikes · **complete**

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

### M4 — Polar core and polar file import · **complete**

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

*M9b, user request, 2026-09-28*: the user's `polar_examples/` (688
real-world files, committed as test data) exposed two importer gaps, both
narrow tolerances rather than a loosened check — an Expedition label row
(`twa0 bsp0 TwaUp bspUp …`, or space-separated `pol Twa0 Bsp0 UpTwa UpBsp
…`) is now skipped instead of refused, and the TWS axis accepts up to 70 kn
(`MAX_TWS_KN`) while boat speed stays capped at 60 (`MAX_SPEED_KN`); a TWS 0
column and TWA 0 row of zeros were already accepted. 687 of 688 files parse;
`polars/J46 heel.txt` is a heel-angle table, not a boat-speed polar, and is
excluded by name with its reason recorded in
`pe-polar/tests/polar_examples.rs`, which walks the whole directory and
spot-checks four files by hand. `pe-app/tests/polar_import.rs` imports a
handful of the same files through `import_polar_files`.

---

### M5 — ORC catalogue and search · **complete**

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

### M6 — 2D polar plot · **complete**

**Deliverables:** the right-panel polar plot (spec §9.2) with TWS slider,
source curves, blend placeholder, hover.

Built as specified, with three points settled that the brief left open (D21):
"All" draws one curve per visible polar source per wind speed that source's
own grid has (the classic multi-curve diagram), rather than one shared slice;
curve points are read at each source's own TWA axis (equivalent to a finer
sweep, since interpolation is piecewise-linear in TWA, but cheaper and exact
at every known angle); and the Map-stage overlay is a toggle owned by the
shell (`App.tsx`), opened by the panel's "Full size" button or the
`overlay:plot` reveal step, closed by its own button, Escape, or switching
stage. `polar_plot(tws: number | null)` returns curves, the TWS domain,
`dots` (always empty; the shape is final for M8/M9) and `blend` (always
`None`; the hook for M14). Tracks are not polar sources, so they never
contribute a curve, only future dots.

---

### M7 — 3D polar view · **complete**

**Deliverables**

- `ui/src/polar/` three.js scene (spec §10.1–10.2): polar-tower and
  Cartesian layouts, surfaces per source, instanced dots, legends, cameras,
  colour-by modes.
- Selection (click, shift, lasso, box), exclude/include as undoable commands
  (spec §10.3).
- Rust → UI transport for large arrays: a binary IPC response
  (`tauri::ipc::Response`) of packed `f32`, not JSON.

**Acceptance:** spec §13 3D budgets met on the reference machines.

Built as specified. The generic selection and exclusion machinery is
complete; per the pre-flight ruling, Exclude and Include act on **polar
nodes** of ORC and file sources now (`Command::ExcludeCells` /
`IncludeCells`, one history entry per action, a `Batch` across sources),
and `set_excluded` already takes sample ids, ignored until tracks have
samples (M8). `polar_scene` returns the scene as a packed binary
`tauri::ipc::Response` (layout in `pe-app/src/polar3d.rs` and
`ui/src/polar/scenePacket.ts`, pinned by `ui/src/polar/fixtures/scene-v1.bin`
from both sides). Samples, the blend surface (M14), colour by Hs / current /
time and "show filtered" have the wire shape and the controls but nothing to
show yet: the controls are offered disabled with a tooltip. "Show on map" is
disabled until M8. `pe_polar::blend_input` is the source grid the blend
will read, with excluded cells empty.

*Measured 2026-09-28 on the development machine* (2019 MacBook Pro, Intel
UHD 630, `ui/bench3d.html` in headless Chrome with ANGLE Metal, 1280 × 800,
now drawing M7's shaped dots): 200k dots + 20 surfaces hold 60 fps, frame
p50/p95 16.7/16.8 ms over 300 frames in two runs (p99 16.8 and 33.2 ms; one
shader-compile hitch of 0.15–0.33 s at start). 400k + 40: p95 33 ms, so
the headroom is still under 2×. Per edit at 200k samples: Rust packs the
scene in 29 ms (debug build) into 8.0 MB; the page unpacks it in 2.7 ms
(views, no copy) and rebuilds the drawn dots in 15 ms; a lasso takes 6–7
ms. Budget: **go** on this machine; the reference machines and the Tauri
webviews (WKWebView, WebView2), including the IPC transfer of the 8 MB
buffer, are still unmeasured — to do before release (M18).

---

### M8 — File tracks and the sample pipeline · **complete**

**Deliverables**

- `pe-tracks`: fixes, heading and speed derivation (spec §7.4), GeoJSON
  and CSV import with the column-mapping dialog (spec §7.3), boat picker for
  multi-boat files.
- Track list and map drawing (spec §7.1, §9.1), hover details.
- Samples without environment: dots appear once environment arrives (M9).

**Acceptance**

- Heading and speed derivation tests against hand-computed values, including
  the antimeridian, a stationary boat and a single-fix track.

Built as specified (acceptance: `pe-tracks/src/derive.rs` tests, values
hand-computed on the 6,371,229 m sphere: equator, antimeridian both ways,
north at 50°N, over the pole, off-line central difference, stationary,
single fix, gaps, prefer). `pe-tracks` holds geodesy, derivation, filters,
time parsing and the GeoJSON and CSV readers (never-panics proptests);
`pe-app/src/tracks.rs` inspects and imports outside the lock, one
`Batch` per import; `map_tracks.rs` sends visible tracks as a packed binary
buffer (unwrapped longitudes). New undoable commands: `ExcludeSamples`,
`IncludeSamples`, `SetSampleFilters`, `SetDerivation` (carrying every
sample's motion both ways, since `pe-core` cannot derive). Ids are capped at
2^53 − 1 (`pe_core::MAX_ID`, `Project::reserve_ids`, validation) so they
cross IPC as exact JavaScript numbers. Per the controller rulings: a sample
without wind has no place in the polar and is not drawn (the 2D and 3D dots
appear once M9 fills TWS/TWA — `samples_become_dots_once_they_have_wind`
and `samples_appear_in_the_3d_scene_once_they_have_wind` inject values to
prove it); sample exclusion and "show on map" are wired; the M7 minors are
fixed (only drawn dots are counted or acted on, numeric selection keys —
keying and re-finding all 200k samples selected measured 48 ms in Node
against 188 ms for the string keys it replaces, camera refit on project change, `buildDots` uses the
shown mode, "?" translated); the ±1 kn dot band is a Settings preference.
The wind, wave and current filters are modelled and evaluated but offered
disabled until M9. The import dialog's configuration controls (column roles,
time format and pattern, speed unit, boat picker) are tagged and registered;
since the dialog exists only once files are chosen, their search entries
land on File… (`Feature.landing`) and say a file must be chosen first. Only
its Cancel and Import answer buttons are untagged (spec.md 3.6).

*Measured 2026-09-28* on the development machine, debug build (`pe-app` at
opt-level 0), 50 tracks × 10,000 fixes: project summary 49 ms, map packet
141 ms (10 MB), 3D scene 173 ms, 2D plot 51 ms, excluding 10,000 samples
87 ms (summary included). Release numbers and the webview's draw rate at
this size are still to measure (M18).

---

### M9 — Environment: wind, waves, currents · **complete**

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

Built as specified (acceptance: `pe-env/tests/sampler.rs`
`acceptance_u10_at_50n_5w_matches_numcodecs` — u10 = 9.382978439331055 at
2020-07-27T12Z, 50N 5W through the whole provider, the reference decoded
from the same WeatherBench2 chunk with numcodecs 0.12.1 in M3 and recorded
in the test; `pe-app/tests/env_jobs.rs`
`a_cancelled_fetch_resumes_to_the_uninterrupted_result` and
`…_saves_as_partial_and_resumes_after_reopening`, against a fake provider,
no network). `pe-env` gained the `Provider` trait and `Reanalysis`
(WeatherBench2 then ARCO wind, ARCO waves as unit vectors, the four current
tiers with int16 `scale_factor` unpacking and the surface level), whole-chunk
reads grouped per batch, the pre-flight estimate, HTTP retry/backoff and a
body cap, and the cache carries from M3. Current fixtures
(`currents-crop/`, 340 KB) are the NW Shelf, merged and GlobCurrent chunks
around 49.86N 5.13W cut to 48 hours, with numcodecs references
(`currents-crop/record.py`). `Sample::relate` (in `pe-core`, plain vector
arithmetic on stored values) recomputes TWA, tack, the D13 correction and
the wave angle, and `SetDerivation` runs it both ways (M8 carry). Jobs live
in `pe-app/src/env.rs`: one runner, per-batch writes, cancel, resume,
Refetch, cancel on project replacement; the import opens the pre-flight
(spec §7.5, §13). New undoable commands `SetUseCorrected`,
`SetStokesDrift`. The autosave snapshot is renamed into place under the
lock (M3 carry).

*Measured 2026-09-28*, live (`provider_end_to_end`, 5 positions through
every tier): cold 29.9 s, of which about 15 s is opening seven stores'
metadata; the same provider again 0.15 s (the second boat of a session);
a new provider over a warm cache 15.6 s (metadata only). A real race opens
only the stores its positions need. 5-day cold numbers stand as in M3.

---

### M10 — YellowBrick · **complete**

**Deliverables:** `pe-trackers::yellowbrick` (URL → key, RaceSetup with
ISO-8859-1, AllPositions3 decoder, KML fallback), the shared tracker dialog
(spec §7.2), boat picker with map preview, per-session event cache.

**Acceptance:** fixture tests from recorded responses; one live test behind
`PE_TEST_LIVE`.

*Done 2026-09-28.* `pe-trackers` gained the `TrackerClient` trait and the
shared event model (`event.rs`: resolve without network, one fetch
returns the whole event), `http.rs` (`Fetcher`: 256 MB body cap, the
§7.7 transient/permanent rules with three retries, progress, cancel; a
small copy of `pe-env`'s classification, since the two network crates do
not depend on each other) and `kml.rs` (a bounded reader for YellowBrick's
per-team `gx:Track` placemarks). The YellowBrick client reads RaceSetup
tags into a division and each team's own start and `finishedAt`; a binary
that does not decode, is refused, or is a web page falls back to
`https://yb.tl/<key>.kml` (the CDN answers 504 for it) with a 10-minute
timeout. `TrackOrigin::Tracker` gained optional `model` and `division`
(left out of the file when absent, so no schema bump). `pe-app/trackers.rs`
downloads on a worker with progress events and an immediate Cancel, keeps
four events per session, and imports through the file import's commit
(one Batch, next palette colours), setting each boat's start–finish as its
time window; the environment pre-flight opens after it. The UI adds
`TrackerImportDialog` (address, progress, Retry, table with search and
tick-all, SVG map preview over the basemap coastline), the Race trackers
help topic, and the registry entries (landing on YellowBrick…).

*Fixtures* (2026-09-28): Rolex Middle Sea Race 2024 — RaceSetup (97 KB),
the first three teams of `AllPositions3` (45 KB of 1.37 MB) and of the KML
(844 KB of 23 MB). The KML is checked against the binary: every KML fix is
a binary fix at the same time and 1e-5° position; the binary also holds
reports at repeated times (1689 against 1670 for the first team). A local
server replays the recordings through the client (primary, fallback, 5xx).
*Live* (`PE_TEST_LIVE=1`): Fastnet 2025 through the client 444 boats,
714,380 fixes in 0.4–1.6 s; Middle Sea 2024 KML 23 MB in 17 s; an unknown
key answers 500 four times and is reported "not answering". *Unverified:*
the AllPositions3 alt/lap/pc layouts — every race probed (about 50 keys:
Fastnet 2023/2025, ARC 2025, RMSR 2024 and others) has flags `0x02`, so
Appendix B's note stands.

---

### M11 — Geovoile · **complete**

**Deliverables:** `pe-trackers::geovoile`: viewer HTML parsing (rooturl,
resourcesurl, seeds), versions file (JS object literal parser), hwx decoder,
config XML, tracks and reports; multi-leg support; clear refusal of Flash and
pre-2016 trackers.

**Acceptance:** fixtures from at least three sites of different years decode
to the boat count and first/last fix recorded by hand from the live viewer.

**Risks:** Geovoile changes its format between editions. The decoder must
fail with "unsupported Geovoile version" rather than produce garbage: check
that the output parses and the first fix is a plausible time and position.

*Done 2026-09-28.* The `Geovoile` client resolves a viewer address with no
network (exact `*.geovoile.com` host, no user name or port, root segments,
`?leg=<n>`), then reads the page, versions, config, tracks and reports
(five progress steps). Every resource address from the page
(`resourcesurl`, `versionsurl`) is resolved against it and must be HTTPS
on a Geovoile host before it is requested. Reports are parsed by column
name (the 2016 and 2025 orders differ); each gives its heading and speed
(when non-zero) to the fix nearest it within 60 s, the latest status, and
the arrival or hidden time as the boat's finish. The config gives each
boat's class (the division) and its run's start. Legacy pages (no
`rooturl`: 2012–2015 HTML, Flash) are a new `TrackerError::Legacy`, shown
"older tracker" with no Retry; a format change stays "unsupported Geovoile
version" (its own error kind now, also without Retry). Both network crates
build their clients with a redirect policy: same host, or HTTPS to an
allow-listed host (tests end to end through local servers). M10 carries:
the tracker dialog ignores backdrop clicks while downloading or importing;
YellowBrick counts its KML step from the start, so progress never goes
back (asserted in the fixture and pe-app tests); the session keeps events
up to 2,000,000 positions rather than four events; spec §7.7 says the
tracker download's progress is in its dialog. The dialog offers a leg
picker for a race in legs.

*Fixtures and acceptance* (2026-09-28): five sites of four editions
decode to the boat count, total fixes and the first configured boat's
first and last fix — Vendée Globe 2016 (29 boats, 107,459 fixes; Appendix
A's seeds, its page answers 500), Route du Rhum 2018 (26, 28,038; seeds
split over two images, empty versions file), New York Vendée 2024 (28,
64,453), Solitaire du Figaro 2024 leg 1 of 3 (45, 21,990) and 24 Heures
Ultim 2025 (14, 8013). The references are the independent Python decoder's,
checked against each race's facts (start ports and times, official
arrivals in the reports: every arrived boat's last fix is after its
arrival, the tracks running on into port). They were not read off the
live viewers by eye: no browser in this environment. The Route du Rhum
2014 page is recorded as the refused generation. *Live*
(`PE_TEST_LIVE=1`): New York Vendée 2024 28 boats, 64,453 fixes (10,223
with official speed) in 0.7 s; Solitaire 2024 leg 1 45 boats, 21,990 fixes
in 0.6 s. The Vendée Globe 2024 tracker answers "Not available" at
`vendeeglobe.geovoile.com/2024/tracker/` (refused as no public event).

---

### M12 — Blue Water Tracks · **complete**

**Deliverables:** `pe-trackers::bluewater` (slug from URL, race JSON,
GeoJSON positions with SOG/COG).

*Done 2026-09-28.* The `BlueWaterTracks` client resolves
`race.bluewatertracks.com/<slug>`, an `api.bluewatertracks.com/api/race/
<slug>` link, or a bare slug (no network), then makes one request:
`GET /api/race/<slug>`. Live probing during research found the API answers
an unknown slug with HTTP 200 and `race` an empty array rather than an
object (`{"positions":[],"race":[]}`), not a 404 — checked before the
object is parsed, and a plain 404 is read the same way, both as
`NoSuchEvent`. A boat's division is the distinct `division` values across
its `handicaps` (each rating system usually agrees); SOG and COG are given
per position and used exactly as given (0 is a real value here, unlike
Geovoile's official reports). The event's start is `raceStartTime`; a
boat's finish is its own `finishTime` else the race's `trackTimeFinish`.
Positions are not guaranteed sorted or deduplicated per boat (unlike
YellowBrick's and Geovoile's own formats), so each boat's fixes go through
the same `pe_tracks::normalise` a file import uses before the client
returns, keeping `TrackerBoat`'s "oldest first" invariant for the dialog's
table and map preview.

M11 review carries, done alongside: (1) a new `TrackerError::Http { status,
why }` replaces matching "answered 404" in the message text — Geovoile's
same-generation 404 check now matches the status; (2) Geovoile's
`parse_viewer` clamps `nblegs` to 1–99 and refuses a `numleg` outside
`1..=nblegs` as unsupported, rather than trusting the page; (3) the tracker
session cache in `pe-app` now also keys by the *requested* key (an alias to
the actual one a race in legs was downloaded under), so re-pasting a
multi-leg address that resolves without a leg still hits the cache instead
of downloading again.

*Fixtures and acceptance* (2026-09-28):
`bluewater/melbournehobartwestcoaster2025-race.json` — the 2025 Melbourne
Hobart Westcoaster (5 boats, 863 positions) as served, crew, bios, images
and sponsor details cropped out. The whole event decodes through the
client (local server) to the recorded boat count, fixes and first/last fix
of Alien, every fix sorted, given and never derived. *Live*
(`PE_TEST_LIVE=1`): the same race, 5 boats, 863 fixes, 2.2 s, matching the
fixture exactly; an unknown slug reads as no public event.

---

### M13 — Polar segments and editing · **complete**

**Deliverables**

- Track → polar segment binning with statistic, minimum count, per-cell
  count and spread (spec §12.1).
- Edit mode per source (spec §10.4): node drag, table editor, scale, smooth,
  reset; overrides as overlay commands.
- Recompute pipeline: an edit invalidates only the affected source and the
  blend; every open view refreshes.

**Acceptance:** spec §13 edit-to-view budget; removing all overrides restores
the source byte-for-byte (invariant 1).

Built as specified (spec §10.4, §12.1, D22). `pe-polar::segment` bins a
track's used samples onto the output grid (nearest node, half-step bins,
beyond the outer half-steps left out, ties up; linear-rank percentiles;
spread = sample standard deviation; count and spread kept for every cell);
`pe-polar::edit` writes overrides onto a grid and computes scale and smooth
(3×3 binomial kernel over present neighbours). `pe-core` gains
`Command::EditCells { source, action, cells: [CellEdit { before, after }] }`
— one command for every tool, `action` naming the history entry, drags
coalescing via `Command::merge` — and `Command::SetSegmentStatistic`;
`cell_overrides` are validated sorted, one per cell, 0–60 kn. Acceptance:
`removing_every_edit_restores_the_source_byte_for_byte` (pe-core) and
`every_tool_is_one_entry_and_reset_all_restores_the_source_byte_for_byte`
(pe-app, through IPC) compare `.wpsproj` bytes. `pe-app/src/derived.rs`
holds the derived cache (per-source revisions moved only by the commands
that reach a source, never saved; untargeted changes invalidate all, the
environment fetch names its track); the 3D scene, the 2D plot, the table and
the project summary all read it. New IPC: `polar_edit_surface`,
`edit_polar`, `set_segment_statistic`, `polar_plot_dots`; `polar_scene`
takes `focus` and `samples_key`. Carried items: M6 — 2D curves read each
source through its overlay (edits in, excluded nodes empty); M7 — scene
layout v2 (48-byte header, samples key, flags-only mode; fixtures
`scene-v2.bin`, `scene-v2-flags.bin`), and the frontend keeps the samples'
dots when their flags are unchanged, rebuilding the nodes alone; M8 — 2D
dots travel as the packed "PE2D" buffer (`dots-v1.bin`) and draw as squares
beyond 20,000.

*Measured 2026-09-28 on the development machine* (2019 MacBook Pro, Intel
i9; `cargo test --release -p pe-app --test perf_edit -- --ignored`), 20
polar sources and 20 tracks × 10,000 samples, Rust side of edit → every
view (command + summary, flags-only 3D scene, 2D curves, 2D dots at a
slice, table): a table edit on a polar source **25.8 ms** (scene 16.9 ms,
0.91 MB); a drag step on a track segment **23.4 ms** (no re-binning); excluding
1,000 samples of one track **36.1 ms** (that track re-binned). Cold full
scene 64.4 ms, warm 34.3 ms, 8.1 MB. Debug build (pe-app at opt-level 0):
61, 67 and 97 ms. Frontend (Node, 200k samples, warm): unpacking a
flags-only scene 1.1 ms and rebuilding the dots 5 ms when the flags are
unchanged (36 ms when every sample's dot is rebuilt). **Go** on this
machine at under 100 ms; the IPC transfer, `setData` in WKWebView/WebView2
and the reference machines are measured before release (M18). M8 carry, 50
tracks × 10,000 fixes in "all" (release): gathering 500,000 dots 25 ms;
JSON objects (the M8 shape) 197 ms and 66.6 MB against the packed buffer
33 ms and 14.0 MB. The minimum samples per cell is used (5) but edited in
Blend settings, which arrive in M14.

---

### M14 — Blend and export · **complete**

**Deliverables**

- `pe-polar::blend` (spec §12.3) with coverage output; Blend settings
  (grid, statistic defaults, `n_full`, smoothing, current-correction toggle).
- Export: Expedition `.txt`, Adrena `.pol` and `.csv`, with a preview and
  the choice of which grid to export.

**Acceptance:** golden exports; byte-identical on all five targets (CI
compares hashes).

Built as specified (spec.md §8, §9.2, §10.1, §12.2–12.4, D23).
`pe-polar::blend::blend` is the one function holding the rule (weighted
mean, `w = weight × confidence`, `min(1, n / n_full)` for track cells;
fill along TWA, then TWS, only between known values; 0° row 0 kn; optional
3×3 binomial smoothing; per-cell origin for coverage); `blend::on_grid`
reads a polar source onto the output grid with every cell read from an
excluded node empty. `pe-polar::export` checks a grid writes back (axis
collisions named, out-of-range axes, > 60 kn, nothing to write) before the
writers run. `pe-core` gains `BlendSettings::default_statistic` (serde
default, no schema bump; new tracks start with it), strict
`OutputGrid::validate` (1–512 values, two decimals, ≥ 0.01 apart, TWA
0–180, TWS 0–70) and `Command::SetBlendSettings` / `SetOutputGrid` (the
dialog's Apply is one entry; the Blend entry's switch and colour name their
own). `pe-app/src/blend.rs` assembles the blend from the derived cache for
the views (cached by the visible sources' `Arc`s, weights, grid, `n_full`
and smoothing) and **from scratch for export** (`blend::fresh`); new IPC:
`set_blend_visible`, `set_blend_colour`, `set_blend_settings`,
`export_preview`, `export_polar`; `ProjectSummary.blend`; the 2D plot's
`blend` is now a list of curves; the 3D scene sends the blend surface
(`BLEND_SOURCE`). Frontend: the Blend row (colour, show/hide, coverage,
Blend settings, Export…), `BlendSettingsDialog`, `ExportDialog` (format,
project or custom grid, preview table, native save dialog), `axes.ts` (the
grid editor's rules, the M4 carry). Carried items: M4 — export refuses
colliding axes naming both values, the grid editor refuses values closer
than 0.01, and a property test re-imports every export as the grid written
(`pe-polar/tests/blend.rs`); M6 — the plot's hover layout includes the blend
(`plotMaxBsp`); M7 — the blend surface takes the Blend entry's colour and
the 3D bounds include it. Acceptance: golden files
`pe-polar/tests/golden/blend.{txt,pol,csv}` (a fixed blend, two cells worked
by hand) and the SHA-256 of a fixed project's three exports pinned in
`pe-app/tests/export.rs`, run by every CI target that runs tests (four;
Windows ARM64 builds its tests but cannot run them on the hosted runner).

*Measured 2026-09-28 on the development machine* (Intel i9, debug build,
`perf_edit`): the blend after an edit at 20 polar sources and 20 tracks ×
10,000 samples 1.0 ms (budget 50 ms); an export from scratch (every segment
binned again) 70.5 ms; edit → every view 73, 73 and 92 ms (M13: 61, 67,
97), the summary now carrying the blend's coverage.

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
| Coarse tides before 2020-11 outside NW Europe/IBI | Current correction weaker for older races (GlobCurrent's 0.25° FES2022 tide) | Regional tidal reanalyses first; Q1, Q7 (settled: GlobCurrent includes the tide) |
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
| D19 | Reanalysis sampling hourly by default, 3-hourly option; the pre-flight dialog preselects 3-hourly when the hourly download would exceed half the chunk-cache limit | Confirmed by M3: a 5-day race hourly is ≈ 1.2 GB and ≈ 40 s cold at 8 requests in flight, and a warm chunk is 3 ms. Hourly resolves wind shifts and tidal streams that 3-hourly smooths. A long race is different: the Vendée Globe hourly would be ≈ 19 GB, about the whole default cache, which is when 3-hourly is the better default |
| D20 | Current tiers: regional tidal reanalysis → global merged (uo + utide, 2020-11+) → GlobCurrent (geostrophic + Ekman + FES2022 tide, 1993+; its 202411 metadata, checked 2026-09-28, Q7) | Only anonymous sources; GlobCurrent (FES2022) gives tides globally from 1993, not only NW Europe/IBI |
| D21 | 2D polar plot (M6): "All" draws one curve per visible source per wind speed that source's grid has; curves are read at each source's own TWA points; the full-size view is a Map-stage overlay toggled by the shell, closed by its own button, Escape or a stage switch | Spec §9.2 named the slider's "all" state and the full-size overlay without saying what either draws or how the overlay opens and closes |
| D23 | Blend and export (M14): an output cell read from an excluded node of a polar source is empty for that source (not read across it); the fill steps interpolate only between known values and the 0° row takes no part in them (set to 0 kn last, "filled" unless a source had it); sources are summed in id order and cells rounded to 1e-6 kn; the Blend settings dialog applies as one undo entry, the Blend entry's switch and colour as their own; the grid editor takes two decimals at most (≥ 0.01 apart); a custom export grid is the project-grid blend resampled; export refuses axis collisions, > 60 kn and an empty blend, naming the values | Spec §12.3 named the rule and the fill order without saying how exclusions reach a resampled cell, whether the 0° row anchors the fill, the summation order or how settings are undone; §12.2 and the M4 carry left the grid editor's precision open |
| D22 | Polar edits and segments (M13): one `EditCells` command for every edit tool (overrides before/after per cell, the tool naming the undo entry, drags coalescing); segment bins are half-steps around each output-grid node with nothing beyond the outer half-steps, and nothing is binned into a 0° TWA node (samples nearest 0° are dropped, not moved: the 0° row is 0 kn, spec §12.3; controller ruling); spread is the sample standard deviation; smooth is the 3×3 binomial kernel over neighbours with a value as the blend reads them (excluded nodes take no part; controller ruling); the 3D samples key mixes a per-opening nonce, so two openings never share one; views show sources as edited, and the 2D curves also leave excluded nodes out | Spec §10.4 and §12.1 named the tools, the statistic and "count and spread" without the binning edges, the spread measure, the kernel or how undo groups them |

## 6. Settled before coding started

The user accepted the proposals below on 2026-09-27 ("all good").

- **Q1 — Tides before November 2020 outside NW Europe/IBI:** accept "no
  tide" for those samples, flagged and filterable. FES2014 stays deferred.
  *Superseded by Q7 (M9):* GlobCurrent 202411 already includes a FES2022
  tide, so those samples have tides and none is flagged.
- **Q2 — Wind after 2023-01-10:** ARCO-ERA5 supplies it (D12).
- **Q3 — Stokes drift:** excluded by default (`uo + utide`), with a setting
  to include it.
- **Q4 — Blend rule:** weighted mean per cell with sample-count confidence
  (spec §12.3).
- **Q5 — Linux ARM64:** deferred.
- **Q6 — Terms of use:** the user accepts the risk; imports stay
  user-initiated, one event at a time, with polite concurrency.

### Settled in M9

- **Q7 — GlobCurrent and tides:** settled by the stores' metadata on
  2026-09-28 (controller ruling): GlobCurrent 202411 `uo`/`vo` are "absolute
  geostrophic velocity + depth Ekman + tide velocity" (FES2022) in both the
  multi-year and near-real-time stores, so the tier is recorded
  `has_tide: true` and nothing is labelled "no tide". The tier order is
  unchanged.

---

## Appendix A — Geovoile hwx decoding

Verified on 24hultim 2025 and Vendée Globe 2016 (2026-09-27), whose
resources are committed fixtures, and in M11 (2026-09-28) on Route du Rhum
2018, New York Vendée 2024 and Solitaire du Figaro 2024 leg 1, also
committed.

- The seeds are four 24-bit constants in the first base64 `/C/…` segment of a
  `data:image/png` source in the viewer HTML (the segments may be spread
  over several such sources: Route du Rhum 2018 has the constants in one
  and the keystream in the next). They differ per site (2022–2025
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
on them. The M3 decoder follows the table literally: in a delta moment,
`alt` and `pc` are read as values, not deltas, and only `dDtf` accumulates.
