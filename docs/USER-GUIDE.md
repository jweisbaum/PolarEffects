# PolarExplorer user guide

PolarExplorer builds sailing polars in independent boat tabs from ORC and ORR certificates,
polar files and race tracks. A polar describes boat speed at different true
wind speeds and angles. The resulting blend can be exported for routing software.

## Boat tabs and race projects

A new project starts with one boat. **Add boat**, to the right of the project
name in the top bar, opens another tab with its own sources, filters, edits,
undo history and views. It is also available in projects opened from a tracker.
Double-click a tab or press F2 to edit its name in place; Enter keeps it and
Escape cancels. **Delete boat…** removes the selected boat and its sources;
**Undo delete boat** restores it while the project stays open. Keep at least one
boat. Saving includes every remaining boat. Each boat starts with 0° at the top.
With multiple boats, **Export all…** above the tabs writes one polar per boat
to a folder, reporting boats with no exportable data and avoiding overwrites.

**Split view** compares two boats and **Four-way view** compares four. Choose
a boat in each pane's dropdown; the tab row is hidden. These views use 3D only. Rotation, pan, zoom and camera
presets are linked. Hovering a dot shows the nearest visible point in the
other panes within 5° TWA and 1 knot TWS; a pane without a corresponding point
says so. Each pane retains its own sources and filters. Use **Filters and
display** to open the comparison pane’s filter controls; they start collapsed
to leave room for the polar. Returning to Single view restores its panel layout.

Choose **Open project from tracker…** on the start screen or Project menu and
paste a YellowBrick or Blue Water race link. The app downloads the event natively,
creates one tab per boat, imports its race track, and searches the embedded ORC
catalogue, downloaded ORR catalogue and local boat metadata for identical models.
Historical tracks come from the GeoJSON directory configured in Settings.
Choose **Exact boat only** to limit both polars and historical tracks to that
individual boat. This requires matching MMSI or a sail number corroborated by
builder and length; the name or model alone is insufficient. Conflicting
identifiers or specifications reject a match. The boat's track from the supplied
race is included with either option.
The report shows available original boat details (MMSI, model, class, type,
builder, length, sail number and other supplied fields), the matched model,
source counts and unavailable tracks.
Choose **Open project** to open these boats, or **Cancel** (also Escape) to
close the list and keep your current project and unsaved changes.
Imported tracks offer the usual weather download and polar extraction controls.

Names and team names alone never prove identity or a model match. Model, class,
type, design, make and builder fields are checked for specific model evidence.
Broad classes such as IRC, IMOCA 60 or Class 40 and a builder alone are insufficient.
Conflicting builders or known lengths reject matches. A corroborated vessel
identifier can supply an otherwise missing, unambiguous model; a name cannot.
Conflicting MMSIs prevent that identity recovery even when sail numbers match.
No fuzzy model-number matching is used. Boats without verified models still
receive their own race track, with no guessed historical sources.

## Install and find help

Choose the package for your computer:

| Computer | Package |
| --- | --- |
| Intel Mac | `_x64.dmg` |
| Apple Silicon Mac | `_aarch64.dmg` |
| Windows x64 | `_x64_en-US.msi` or `_x64-setup.exe` |
| Windows ARM64 | `_arm64-setup.exe` |
| Linux x64 | `.AppImage`, `.deb` or `.rpm` |

On macOS, open the disk image and drag PolarExplorer to Applications. On
Windows, run one installer. On Linux, use your distribution's package installer,
or make the AppImage executable and launch it. Windows installers can download
WebView2 if it is missing. Builds made without signing credentials may be
blocked or identified as unverified by the operating system; the release notes
should state their signing status.

Press **F1**, use **?**, or choose **PolarExplorer Help** for the offline reference.
Settings changes the interface and help language. The title-bar search finds
controls by name and highlights them when selected. This guide uses English labels;
the built-in reference provides the corresponding French and German instructions.

This guide and the data notices are also installed in `documentation`: on
macOS inside `PolarExplorer.app/Contents/Resources`, on Windows beside the
application, and on Linux in the application's resource directory. Release
downloads include the same files in `PolarExplorer-documentation.zip`.

## Create and save a project

Choose **New**, enter a project name, optionally name the boat and add notes,
then create it. The left panel imports sources; the centre switches between
3D (the default), Map and Compare; Map appears only when tracks are imported.
The right panel holds the source list and 2D polar plot. The Project menu sits
between the search bar and Settings.

The left and right panels overlay the view. Use their edge arrows to hide or
show them without resizing or reframing the map or polar. View controls remain
accessible beside the open panels.

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

**ORR polars.** Choose ORR in **Polar catalogue** to search the bundled
RegattaMan certificates. Offshore and short-course tables are separate variants.
Settings → **ORR polars** downloads a chosen certificate year, with progress
and cancellation. Repeated scrapes update existing catalogue entries; repeated
imports do not duplicate a source or change copies already in your projects.
Scrapes include the complete public certificate fields and ratings: hull, rig,
sails, stability, trim, performance metrics, custom/general and wind-range
ratings, PCS ratings and both time-allowance tables. Their original values are
kept in the catalogue and copied into the project when you add a polar. Builder
and build-year filters use the certificate data. Older polar-only project
sources stay as imported; remove and add a source again to use a refreshed copy.

Both catalogues support measurement bounds for length, beam, draft,
displacement, sail areas and crew weight. Bounds use metric units and omit
boats without the required measurement. **Previous** and **Next** browse
pages of 50 results.

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

**Tracks from the SYRF database.** In Settings → **PostgreSQL track library**,
enter the connection details and use **Test connection**. Success appears in
green; a failed test shows an error. Choose the GeoJSON root and a boat metadata
directory, then **Download boat metadata**. The local snapshot contains vessels
and related records from YellowBrick, Geovoile, Blue Water, old Geovoile,
Regadata and America's Cup. Once downloaded, searching and importing work
without a database connection. On this machine the matching GeoJSON root is
`/Volumes/Disk_Three/s3/syrf-tracks-individual-production`.

Use **Search tracks by vessel details** in the Tracks panel. Search by any
vessel field: name, model, class, make, builder, sail number, measurements or
other saved values. Combine terms from different fields to narrow results;
all terms must match the same vessel. Case and accents are ignored. Existing
metadata downloads already contain these fields and need no refresh.
Results show the race,
date, source and whether its GeoJSON file is available. Import a result, then
use its ordinary **Fetch weather…** button to prepare polar samples.
Your query, remaining results and current page stay open after each import,
so you can add several tracks from the same search. Successfully imported
tracks disappear from that search's results; failed imports stay available
to retry.

**Maintain the library.** Settings → **Scrape tracks now** runs the native
YellowBrick, Geovoile and Blue Water scrapers. Choose **Only on demand**,
**On startup** or **On shutdown** for scheduling. Shutdown waits for the job;
open Settings to see progress or cancel it. All modes, including explicit race
URLs, scrape only races with confirmed terminal results for every participant.
Ongoing, future and unverified races are counted as skipped. An old last position
does not prove completion. YellowBrick and Geovoile are checked before downloading
track files. Blue Water returns metadata and tracks together, so its response is
read to check completion and discarded if unfinished. Scraping saves database relations,
individual GeoJSON tracks and a refreshed metadata snapshot. Existing race and
participant identities are reused. Completed races with available files are
skipped unless explicitly requested using **Race URLs**. Entering URLs limits
the run to those events and their Geovoile legs. Enter a **YellowBrick user key**
and **YellowBrick device ID (UDID)** in Settings to resolve codes from the
version 3 race catalogue. The scraper associates only races listed as free,
then reads version 4 MyRaces for their URLs, including child races. With both
fields empty, it uses known database mappings; explicit race URLs also work.
The metadata directory's `yellowbrick-races.json` records the complete catalogue
and resolved URLs. Unresolved races are listed there and counted in job details.
Scrapers preserve existing mark-crossing events; these providers
do not supply new SYRF mark-crossing records.

**Back up the database.** **Export entire database…** writes SQL for every
database table and its schema. PostgreSQL's `pg_dump` client must be installed;
set its executable path if it is not detected. The export excludes source
ownership and access grants, so restoring does not require the source password.
The destination PostgreSQL server still controls its own authentication and
must have the required extensions, including PostGIS, installed. GeoJSON files
are separate and should be backed up with their directory structure intact.

## Fetch weather for tracks

Track details offer **Wind source**: use supplied true wind where speed and
direction are both present, or downloaded weather only. CSV import can map
TWS/TWD columns and a separate wind-speed unit. GeoJSON and SYRF files also
retain supplied wind. Changing this choice preserves both data sources and
needs no fresh download.

Use **Fetch weather…** on a track, or select tracks and use **Fetch weather for
selected tracks…**. **Select all**, above the imported tracks list, ticks every
track at once; then use the weather button to download their weather together.
Choose wind, waves and current as needed, review the estimate
and start the download. Jobs report progress and can be cancelled.

PolarExplorer samples the archives in space and time at each position. It saves
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
In asymmetric mode, angle labels run 0–180° on both halves of the 2D and 3D
polars. Port and starboard retain their independent speeds.
The 2D plot shows a wind-speed slice; its dot band controls which nearby track
samples appear.

Hover over a 3D dot to see its source, TWA, TWS and BSP. Track dots also show UTC
time, waves and current.

Use **Wave range filters** at the bottom centre of the 3D screen to set lower
and upper limits for wave height, angle off the bow, and period. Each row has one
slider with two round handles: drag the start or end handle, or focus it and use
the arrow keys. Drag the highlighted middle section to move both limits together
without changing their spacing; it stops at either end of the scale. You can also
focus the middle section and use the arrow keys, or Home/End to move to either
end. The values beside the slider show both limits. The dots update while you
drag, and the blend and both plots follow. These saved, undoable ranges apply
in addition to the other filters. Missing measurements fail an active range.
**Reset ranges** clears these sliders without clearing other filters.

Track filters remove unsuitable samples from the track's polar segment.
Their limits are shown in the units selected in Settings. **Exclude** removes
selected samples or nodes from the blend; **Include** restores them. Filtered
or excluded samples can still be inspected. Neither action deletes raw fixes.

Filter edits update while you type. **Direction change**, **AWA change**, and
wind speed/direction change thresholds compare each point only with the
immediately previous and next points. A change over the threshold on either side
excludes the point. Missing neighbours and gaps are not skipped or bridged.
AWA is calculated from boat motion and the selected true wind.
These controls are available for individual tracks, global filters and priority
groups. Empty thresholds disable their filter; Undo restores an edit.

Track filters also offer unknown wave/current removal, tack/gybe windows,
and stops defined by a ground-speed threshold with time padding before and
after each event. Wave direction can be compared with COG. Timestamp filters
accept minutes or seconds and keep exact multiples from UTC midnight; they
never round times. Track start/end times accept seconds.

With tracks present, the 3D view offers **Global point filters**. These layer
over individual filters and affect the blend and every view; they have no
start/end time. **Priority filter groups** select the first group with enough
samples in each TWA/TWS cell, pooled across visible tracks with positive weight.
Put stricter criteria first and a looser fallback last. If none qualifies, the
cell has no track evidence. This pooled minimum replaces the per-track minimum
while priorities are enabled. Other sources can still supply the cell.

Colour dots by wave period, wave angle to the bow or wave angle to the wind.
Time colouring displays the UTC date and time at both ends of the scale.

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

Hover the blend in the 3D view or on the polar plot to see, for the cell under
the pointer, each source behind its value with that source's own speed and its
share of the weight. On the polar plot, **Measure** lists every curve's boat
speed at the pointer's wind angle with the differences between them; click to
pin a point and measure from it. **Colour** can show the time of day each track
position was sailed at (night, morning, afternoon, evening by local solar time)
instead of its track, in both views.

Use the source list to control visibility, colour and weight. Visible sources
with positive weight contribute to the blend on the project's output grid.
**Blend settings…** edits that grid. Weights run from 0 to 1 and are relative: halving
every weight does not change the result. A project saved with a weight above
1 by an earlier version opens with that weight at 1. The blend uses each source with its edits and
exclusions; it does not extrapolate beyond a source's coverage.

In the top bar, **Asymmetric polar** keeps independent starboard
(0–180°) and port (180–360°) values in both plots, blending, corrections and
export. Half-circle inputs seed both sides. Symmetric mode averages opposite
original nodes without rewriting imported data. The toggle applies immediately
and supports Undo/Redo; leave it unchecked for symmetric polars. In Blend
settings, choose linear or monotone spline interpolation and TWA steps of
1/2/5/10° or TWS steps of 1/2/5/10 kn.
Neither interpolation mode extrapolates beyond supported values.

**Correct blend** opens a cell table for manual changes after blending.
Corrections are saved as overlays, support Undo/Redo, and can be reset to the
current calculated blend. Head-to-wind rows at 0° and 360° remain zero.

In **Compare**, choose A and B from sources or the blend. The difference is
**A − B**: positive values mean A is faster. Empty coverage is distinguished
from zero difference. Use the heatmap or 3D view to inspect disagreements.

Choose **Export…**, select Expedition, Adrena or CSV, and use the project grid
or a custom grid. Inspect the preview, then choose a destination. The blend is
recomputed for every export; exporting does not save the project or replace
its sources. Given the same project and options, export bytes are reproducible.

## Let an AI client drive PolarExplorer

**Settings → MCP service** lets an AI client on the same computer work in
PolarExplorer for you: open a project, add certificates, polar files and
tracks, fetch weather, set weights and filters, read what stands behind a
blend cell, compare and export. It uses the same commands the interface
does, so every change it makes is one you can undo, and you watch the views
follow it. While a client is connected the status bar shows **MCP** and the
last tool it called. It does not replace a file that is already there (a
project saved under a new name, an exported polar) unless it was told to.

The service is off until you turn it on. Turning it on opens a port on this
computer only (47392 unless you change it) and issues a token a client must
present; turning it off closes the port and forgets the token. **Rotate
token** issues a new one, after which a client added before must be added
again.

- **Add to Claude Code** and **Add to Codex** write the service into that
  client's own configuration. Restart a session that is already running.
- **Add to Claude Desktop** (macOS and Windows) opens PolarExplorer's
  extension in Claude Desktop, where you confirm the install. It needs
  installing once.
- **Configuration for other clients** gives the address and token as text.
- **ChatGPT** is not offered: it reaches MCP servers only over the public
  internet, and this service answers only on this computer.

A client cannot read or change Settings, use the track database or quit the
application. It can start the downloads you can start (a tracker's race,
weather for a track, a year of the ORR catalogue); ask it to say so first
if that matters to you. It can also read the files you could open here: a
track or polar file it is pointed at is read wherever it is.

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
