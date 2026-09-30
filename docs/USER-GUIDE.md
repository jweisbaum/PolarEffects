# PolarEffects user guide

PolarEffects builds a sailing polar for one boat from ORC certificates,
polar files and race tracks. A polar describes boat speed at different true
wind speeds and angles. The resulting blend can be exported for routing software.

## Install and find help

Choose the package for your computer:

| Computer | Package |
| --- | --- |
| Intel Mac | `_x64.dmg` |
| Apple Silicon Mac | `_aarch64.dmg` |
| Windows x64 | `_x64_en-US.msi` or `_x64-setup.exe` |
| Windows ARM64 | `_arm64-setup.exe` |
| Linux x64 | `.AppImage`, `.deb` or `.rpm` |

On macOS, open the disk image and drag PolarEffects to Applications. On
Windows, run one installer. On Linux, use your distribution's package installer,
or make the AppImage executable and launch it. Windows installers can download
WebView2 if it is missing. Builds made without signing credentials may be
blocked or identified as unverified by the operating system; the release notes
should state their signing status.

Press **F1**, use **?**, or choose **PolarEffects Help** for the offline reference.
Settings changes the interface and help language. The title-bar search finds
controls by name and highlights them when selected. This guide uses English labels;
the built-in reference provides the corresponding French and German instructions.

This guide and the data notices are also installed in `documentation`: on
macOS inside `PolarEffects.app/Contents/Resources`, on Windows beside the
application, and on Linux in the application's resource directory. Release
downloads include the same files in `PolarEffects-documentation.zip`.

## Create and save a project

Choose **New**, enter a project name, optionally name the boat and add notes,
then create it. The left panel imports sources; the centre switches between
Map, 3D and Compare; the right panel holds the source list and 2D polar plot.

Save with **Project → Save** or Cmd+S (Ctrl+S on Windows/Linux). A `.wpsproj`
contains the original sources, your edits and fetched values at each track
position. Keep that file to continue editing; a polar export is only the result.
**Save As** creates another project file. A dot by the project name means
unsaved changes. Closing or replacing an unsaved project offers Save, Don't save
or Cancel. Settings controls autosave and recovery; save explicitly before
sharing a project. After an unclean exit, the start screen offers recovered work.

## Add sources

**ORC polars.** Search the embedded catalogue by name, model, sail number or
other certificate fields. **Search by field** narrows individual fields; all
filled filters must match. Choose **Add** on a result. About shows the catalogue's
source commit and dates. Catalogue searches work offline.

**Polar files.** Import an Expedition `.txt`, Adrena `.pol` or supported CSV
polar. Review its preview and any parser error before continuing. Original
grids stay intact; missing cells remain missing.

**Tracks from files.** Import GeoJSON or CSV from the Tracks section. Check
times and positions in the preview and map. Boat speed and course can be derived
from consecutive positions; implausible fixes can be filtered later.

**Tracks from a race.** Paste a supported YellowBrick, Geovoile or Blue Water
Tracks event URL into the tracker import dialog. Select boats as the list
arrives, then import once their positions are available. This imports tracks
only. Weather is a separate, explicit action.

## Fetch weather for tracks

Use **Fetch weather…** on a track, or select tracks and use **Fetch weather for
selected tracks…**. Choose wind, waves and current as needed, review the estimate
and start the download. Jobs report progress and can be cancelled.

PolarEffects samples the archives in space and time at each position. It saves
those sampled values and their dataset provenance in the project. Downloaded
blocks stay in memory only; other tracks from the same race can reuse them
during that session. **Weather kept in memory** in Settings limits that reuse.

Coverage differs between datasets, dates and locations. A missing value stays
missing, including sea data unavailable on land. A track needs wind to appear
in the polar views and contribute polar samples. Reanalysis is a reconstruction
of past conditions, with finite spatial and temporal resolution; inspect the
track's context before treating a fast sample as representative performance.

## Inspect, filter and edit

The map shows the tracks; **Fit the world** and track framing help find them.
Switch between the flat map and globe. A map box selects the same samples in
the polar views. Escape clears a selection.

In 3D, the polar tower places true wind angle around the axis, boat speed
outward and true wind speed vertically. Cartesian layout uses three straight
axes. Rotate the view, or choose Lasso or Box to select samples and grid nodes.
The 2D plot shows a wind-speed slice; its dot band controls which nearby track
samples appear.

Track filters remove unsuitable samples from the track's polar segment.
Their limits are shown in the units selected in Settings. **Exclude** removes
selected samples or nodes from the blend; **Include** restores them. Filtered
or excluded samples can still be inspected. Neither action deletes raw fixes.

Choose **Edit** on a source to open its table and 3D editing tools. Change a
cell, drag a node, scale selected cells or smooth them. **Reset** clears those
edits; **Reset all edits** restores the source. **Undo** and **Redo** apply to
project edits, including exclusions. A track's grid is derived from its eligible
samples: by default each bin needs five samples and uses their 90th percentile
boat speed. The statistic can be changed in edit mode.

Every change is an overlay beside the original source. Clearing overlays gives
back the imported data. Speed display units can be knots, m/s or km/h; stored
polars and polar export formats use knots.

## Blend, compare and export

Use the source list to control visibility, colour and weight. Visible sources
with positive weight contribute to the blend on the project's output grid.
**Blend settings…** edits that grid. Weights are relative: doubling all weights
does not change the result. The blend uses each source with its edits and
exclusions; it does not extrapolate beyond a source's coverage.

In **Compare**, choose A and B from sources or the blend. The difference is
**A − B**: positive values mean A is faster. Empty coverage is distinguished
from zero difference. Use the heatmap or 3D view to inspect disagreements.

Choose **Export…**, select Expedition, Adrena or CSV, and use the project grid
or a custom grid. Inspect the preview, then choose a destination. The blend is
recomputed for every export; exporting does not save the project or replace
its sources. Given the same project and options, export bytes are reproducible.

The regional GRIB export is separate: it writes requested reanalysis fields for
a chosen area and time interval, with an estimate and cancellable progress.
It does not export a sailing polar.

## When something is missing

- **Track absent in a polar view:** fetch wind and check filters and exclusions.
- **Blank blend cells:** check visibility, positive weights, source coverage and
  the minimum number of samples per track bin. The app does not fill unsupported data.
- **Weather gaps:** check dataset coverage and whether positions fall on land.
- **Slow downloads:** fetch only the tracks and variables needed; reuse the
  session's memory cache for boats in the same race.
- **A moved recent project:** open its new location; removing an entry from
  Recent does not delete the file.

The [data source notices](DATA-SOURCES.md) identify the archives and their terms.
