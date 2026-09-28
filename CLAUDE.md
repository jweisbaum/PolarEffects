# CLAUDE.md

Guidance for Claude Code working in this repository.

**Read `spec.md` before changing behaviour. Read `plan.md` before starting new
work.** This file is the operational layer: invariants, layout, commands,
conventions, and recipes. It follows the sibling application VectorEffects
(`/Users/jon/VectorEffects`), whose architecture this project copies. When a
question below has no answer, look at how VectorEffects solved it first.

---

## What this is

PolarEffects is a Tauri desktop app that builds a sailing polar for one boat.
It blends ORC polars, imported Expedition/Adrena polars, and polar segments
derived from historical race tracks. Each track position is matched with
reanalysis wind, waves and current, and the result is exported as an
Expedition or Adrena polar. Rust owns the entire domain: document model,
imports, scraping, reanalysis sampling, polar math, blending and export.
React/TypeScript is a view layer only.

---

## Hard invariants

Violating any of these is a design regression. If a task seems to require it,
stop and raise it rather than working around it.

1. **Sources are immutable; edits are overlays.** An imported polar, an ORC
   record, and a track's raw fixes are stored exactly as imported. Every user
   change is stored beside the source as an overlay: an excluded sample, a cell
   override, a filter, a colour or a weight. Removing every overlay must give
   back the source exactly. Nothing silently rewrites imported data.
2. **The blend is derived, never stored as truth.** The blended polar, the
   per-track polar segments, and the comparison surfaces are recomputed from
   sources plus overlays. They may be cached in memory, never persisted as
   project data. Export always recomputes.
3. **Fetched environment data is project data, rendered views are not.**
   Wind, wave and current samples at track positions are expensive to fetch,
   so they are saved in the project with the dataset name and version they
   came from. Deleting the chunk cache directory must always be lossless.
4. **Nothing is fetched that the user did not ask for, and nothing reaches
   in.** No CDN fonts, no map tiles, no telemetry, no remote schema fetches.
   The webview's CSP stays `'self'`-only. Only two crates may use the network:
   `pe-env` (reanalysis archives) and `pe-trackers` (YellowBrick, Geovoile,
   Blue Water Tracks), each for its allow-listed hosts only.
   `npm run check:offline` enforces this. The ORC catalogue is bundled at
   build time and never fetched at run time.
5. **Export is deterministic and byte-reproducible** across machines and
   platforms: the same project gives the same `.txt`, `.pol`, `.csv` and
   `.grib2` bytes.
6. **Interaction stays fast; imports and fetches may be slow.** Long work
   (scraping, reanalysis sampling, GRIB writing) runs off the UI thread,
   reports progress, and can be cancelled. Editing a polar updates every open
   view within the budget in spec §13.
7. **Every string is translatable and every control is findable.** No
   user-visible text bypasses `t()`/`msg()`, and every interactive control has
   a `data-feature` id registered in the help search.

---

## Repository layout

```
crates/
  pe-core/      Document model (Project, sources, overlays), undo/redo,
                IDs, geodesy, .wpsproj I/O, migrations, canonical floats
  pe-polar/     Polar grid type, interpolation, Expedition/Adrena readers
                and writers, track-sample binning, blending, comparison
  pe-orc/       The embedded ORC catalogue (generated at build time) and
                its search index
  pe-tracks/    Track model, GeoJSON/CSV import, heading/speed derivation,
                sample filters. No network
  pe-trackers/  Network: YellowBrick, Geovoile (hwx decoder), Blue Water
                Tracks clients and decoders
  pe-env/       Network: zarr readers for WeatherBench2 wind, ARCO-ERA5
                waves, and ocean currents; pure-Rust blosc/LZ4; chunk cache;
                space-time sampling
  pe-grib/      GRIB2 writer (fixed-layout message template, as in
                VectorEffects' ve-grib), regional grids
  pe-app/       Tauri shell: IPC commands, state, jobs, autosave, settings
ui/             React + TypeScript + Vite front end
  src/project/  Start screen, project menu, save guard, dialogs
  src/map/      WebGL2 world map with tracks
  src/polar/    2D polar plot, 3D polar view (three.js), compare view
  src/panels/   Left navigation (ORC / Polar files / Tracks), source list
  src/settings/ Settings dialog, themes, language picker
  src/help/     Feature registry, search, highlight, help topics
  src/i18n/     Catalogues (en, fr, de), glossary, coverage tests
  src/generated/ ts-rs bindings. Never edit by hand
assets/         Natural Earth basemap, ORC catalogue build input, samples
tools/          orc-catalogue-builder, basemap-builder, check-offline.sh
```

**Dependency direction:** `pe-app` → everything. `pe-polar`, `pe-tracks`,
`pe-orc` and `pe-grib` → `pe-core`. `pe-trackers` → `pe-tracks`. `pe-env` →
`pe-core`. Never the reverse, and `pe-core` depends on no sibling.

---

## Commands

Everything runs from the **repository root**. The Tauri CLI only resolves the
app crate from here.

```bash
npm install                 # root npm workspace; installs ui/ too
npm run dev                 # tauri dev
npm run ui:dev              # frontend only, no Rust backend

# Checks: run all of these before declaring work done
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run ui:typecheck
npm run ui:test
npm run check:offline       # invariant 4

# Regenerate TS bindings after changing any IPC-facing Rust type
npm run bindings            # cargo run -p pe-app --example export_bindings

# Rebuild the embedded ORC catalogue from a jieter/orc-data git checkout
# (read from its HEAD commit; the build date is that commit's date unless
# SOURCE_DATE_EPOCH is set, so rebuilding the same commit is byte-identical)
cargo run -p orc-catalogue-builder -- ../orc-data/site/data \
    crates/pe-orc/data/catalogue.bin

# Tests that use the network on purpose (never in the default suite)
PE_TEST_LIVE=1 cargo test -p pe-env --test live -- --ignored --nocapture
PE_TEST_LIVE=1 cargo test -p pe-trackers --test live -- --ignored --nocapture

npm run build               # tauri build for the host target
```

---

## Conventions

| Topic | Rule |
|---|---|
| Speeds | Boat speed and wind speed are stored in **knots** (the unit of every polar format and of ORC data). Reanalysis m/s is converted once, on ingest, in `pe-env`. |
| Angles | Degrees. TWA in [0, 180], symmetric (port and starboard folded). Headings and directions in [0, 360). |
| Direction sense | Wind and waves use the meteorological **"from"** direction. Currents use the oceanographic **"toward"** direction. Name the field accordingly (`twd_from`, `current_toward`). |
| Components | `u` east, `v` north, as stored by ERA5 and CMEMS. `speed = hypot(u, v)`. |
| Longitude | [−180, 180) everywhere except inside `pe-env` and `pe-grib`, which use the archive's 0–360 grid. Every geodesy function has antimeridian tests. |
| Time | UTC only, `i64` epoch seconds. Local race time zones are display only. |
| Earth | Spherical, radius 6,371,229 m (the ERA5 and GRIB shape of earth). |
| Floats | Every `f64` in project JSON goes through the `canonical` serde helpers, as in VectorEffects. |
| Determinism | No `HashMap` iteration anywhere that feeds blending, export or the saved JSON. Use `BTreeMap` or sorted `Vec`. |
| IDs | `SourceId`, `TrackId`, `SampleId` are newtype u64s allocated by the project. They are never reused. |
| Colours | Stored as `#rrggbb` on the source, never derived from list order. |

---

## Recipes

**Adding interface text.** Write English in `t("…")` or `msg("…")`. Add the
key to `ui/src/i18n/locales/{fr,de}/<area>.ts` with a real translation (use
the glossary in `ui/src/i18n/GLOSSARY*.md`). `npm run ui:test` fails on a
missing or unused key.

**Adding a control.** Give it `data-feature="<area>:<name>"` and a registry
entry in `ui/src/help/features/<area>.ts` (label, description, keywords,
reveal steps). `features.test.ts` checks tags and registry match both ways.

**Adding a field to the document.** Add it in `pe-core` with a serde default.
Bump `SCHEMA_VERSION` and add a migration if old files need a value. Add a
save → load → save byte-identity test and, if it is user-editable, a `Command`
with an undo inverse test. Run `npm run bindings`.

**Adding a tracker.** A new module in `pe-trackers` implementing
`TrackerClient` (resolve a URL to an event, list boats, fetch one boat's
fixes). Add its hosts to the `pe-trackers` allow-list in
`tools/check-offline.sh`. Decoders get fixture tests from a recorded response
in `crates/pe-trackers/tests/fixtures/`. No live request in the default suite.

**Adding a reanalysis variable.** Add it to `pe-env`'s variable table with
its dataset, units, direction sense and fill behaviour (land is NaN for waves
and currents). Add a fixture chunk test and record the dataset version on
each sample.

**Changing export output.** Formats are pinned by golden files in
`crates/pe-polar/tests/golden/`. Change the golden file in the same commit and
say why in the commit message. GRIB output is checked with the test reader
and, in CI, ecCodes.

---

## Environment gotchas

- The whole back end is Rust. No Python, Node or browser sidecars, and no
  headless browser for scraping. Tracker formats are decoded in Rust.
- Use `rustls` with the `ring` provider and `reqwest` blocking with
  `default-features = false` and the `rustls-no-provider` feature (reqwest's
  plain `rustls` feature pulls in `aws-lc-rs`, a C library). No OpenSSL, no
  `zarrs_http`. This keeps the Windows ARM64 and cross-compiled builds
  working.
- `zarrs` builds `libz-sys` as a build-time dependency of its build script
  only; it runs on the build host and is not linked into the app. That is
  the one accepted C build dependency. Anything linked into the app stays
  pure Rust.
- Blosc is decoded by our own pure-Rust decoder over `lz4_flex`; never pull
  `blosc-src` or any other C library.
- Geovoile hwx seeds differ per site. Parse them from the viewer HTML; never
  hard-code one set.
- The ARCO-ERA5 time axis is preallocated past today. Coverage comes from
  the root `.zattrs` (`valid_time_start`, `valid_time_stop`,
  `valid_time_stop_era5t`), and a 404 chunk means missing, not an error.
- The YellowBrick `RaceSetup` JSON is ISO-8859-1, not UTF-8.
- Never reuse credentials, device ids or cookies found in the
  `tracker-index` reference repository.

---

## Testing rules

- Assert against an independent reference, never "what the code currently
  does": hand-computed values, a published polar, a recorded vendor
  response, ecCodes for GRIB.
- Geodesy and heading derivation need antimeridian, polar and
  stationary-boat cases.
- Every document change needs a round-trip test and an undo inverse test.
- Parsers get malformed-input tests: they return an error naming the line and
  column, never panic.
- Performance changes need before and after numbers against spec §13.

## Style

- `thiserror` in library crates, `anyhow` only at the app boundary, a
  serialisable `AppError { kind, message }` over IPC.
- No `unwrap` or `expect` outside tests (clippy denies them). `unsafe_code` is
  forbidden workspace-wide.
- Doc comments explain why, not what.
- Frontend types that cross IPC are always generated by ts-rs.
- `ui/src/ipc.ts` is the only place `invoke` is called.

## Definition of done

fmt, clippy, all tests and `check:offline` pass. The relevant recipe is
followed. `spec.md` is updated in the same commit as any behaviour change.
Every new string is translated into French and German. Every new control is
in the help search. `plan.md` milestone status is updated.

## Ask, don't guess

Ask before: adding a network host, adding a C or C++ dependency, changing
the `.wpsproj` layout, changing an export format, or changing how the blend
weights sources.
