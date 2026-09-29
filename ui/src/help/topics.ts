import { fold, language, type Language } from "../i18n";

/**
 * The help reference (spec.md 3.5): one topic per area of the interface, in
 * every language. Copied in shape from VectorEffects.
 *
 * The English pages are here; each translation is a file in `locales/`
 * with the same pages, in the same order, with the same ids and
 * cross-references — only the words differ. `topics.test.ts` holds them to
 * that shape. Pages are prose, not keys: they are translated as whole pages,
 * which reads better than a sentence at a time.
 */

export type Parameter = readonly [name: string, description: string];
export interface HelpTopic {
  id: string; group: string; title: string; paragraphs: string[];
  parameters?: readonly Parameter[]; related?: string[];
}

export const TOPICS: HelpTopic[] = [
  { id: "workspace", group: "Workspace", title: "The project window",
    paragraphs: [
      "PolarEffects builds a sailing polar for one boat from ORC certificates, polar files and race tracks, and exports it for routing software.",
      "The title bar holds the Project menu, the project’s name, the stage switcher, the search and Settings. The left navigation gathers the sources, the centre stage shows the map, the 3D polar or the comparison, and the right panel lists the sources above a 2D polar plot. The status bar at the bottom shows hints, errors and work in progress.",
    ],
    parameters: [
      ["Hide or show the navigation (◀)", "Folds the whole left navigation away to give the stage more room. Each of its sections also folds on its own. What is folded is remembered for you, not stored in the project."],
      ["Stage: Map / 3D / Compare", "Chooses what the centre shows. The map is the default."],
      ["Hide or show the right panel (▶)", "Folds the source list and the polar plot away."],
      ["Status bar", "Shows the last hint or error and a line for each job that is running."],
      ["Undo / Redo", "Cmd+Z and Cmd+Shift+Z (Ctrl on Windows and Linux) reverse or reapply the last change to the project."],
    ],
    related: ["projects", "search", "map", "sources"] },
  { id: "projects", group: "Workspace", title: "Projects, saving and recovery",
    paragraphs: [
      "A project is one attempt to build one polar for one boat. It is saved as a .wpsproj file that holds every source exactly as imported, with your changes stored beside it.",
      "When an action would close a project with unsaved changes — New, Open, Open Recent, Close or quitting — PolarEffects asks first: Save, Don’t save or Cancel. Cancel, Escape and a click outside the question all keep the project open as it is. Saving a project that has never been saved asks where to put it; cancelling that also cancels the action.",
    ],
    parameters: [
      ["New… (Cmd+N)", "Creates a project. The name is required; the boat’s name and notes are optional."],
      ["Open… (Cmd+O)", "Opens a .wpsproj file."],
      ["Open Recent", "Lists the ten most recent projects, newest first."],
      ["Save (Cmd+S) / Save As… (Cmd+Shift+S)", "Writes the project to its file, or to a new one."],
      ["Close (Cmd+W)", "Closes the project and returns to the start screen."],
      ["Project name", "Click the name in the title bar to rename the project. A dot after the name means there are unsaved changes."],
      ["Recent projects", "On the start screen. A file that has moved or been deleted is shown greyed as Not found; click it to remove it from the list. Clear forgets the whole list without deleting any project."],
      ["Recovered work", "If PolarEffects did not close cleanly, the start screen offers the unsaved work it had kept. Recovering opens it as the project it came from, still unsaved."],
    ],
    related: ["settings", "workspace"] },
  { id: "orc", group: "Sources", title: "ORC polars",
    paragraphs: [
      "The ORC polars section searches the catalogue of ORC certificates built into PolarEffects (about 18,000 boats from jieter/orc-data) and adds the chosen ones to the project as sources. The catalogue is part of the application and is never downloaded; About names the orc-data version it was built from.",
      "Type in the search box and the results update as you type. Every word must match the start of a word in some field: name, sail number, country, model, builder, designer, year built or certificate year, so farr 40 2023 finds Farr 40s from 2023. Case and accents do not matter, and GBR1124, GBR 1124 and GBR/1124 are the same sail number. An exact sail number comes first, then names and models that start with what you typed, then the other matches, newer certificates first.",
      "Search by field, under the box, unfolds one box per field: boat name, sail number, country, model or type, builder, designer, year built (from and to) and certificate year. Every box you fill must match, and so must the search box: the words of a box must start words of that field only, so a designer box holding farr finds boats designed by Farr but not the Farr 40s of other designers. Case, accents and the sail number's separators do not matter here either, and a certificate year box holding 202 finds the certificates of 2020 to 2029. A boat whose whole field equals what you typed in a box comes first. Collapsed, the heading shows how many boxes are in use; Clear empties them all and keeps the search box.",
      "Add copies the certificate into the project as an ORC polar with the next free colour. Its polar has the ORC angles (52° to 150°) at the certificate's own wind speeds, plus the beat and run angle at each wind speed, where boat speed is the VMG divided by the cosine of the angle. Nothing is filled in between or beyond those angles. Adding a certificate the project already holds asks first.",
    ],
    parameters: [
      ["ORC polars (section)", "Click the heading to fold or unfold the section."],
      ["Search", "Words to find, in any order. Each result shows the name, sail number, model, year built, builder, certificate year and a small polar at light, medium and strong wind."],
      ["Search by field", "Unfolds or folds the per-field boxes; PolarEffects remembers which. The number in brackets is how many are in use."],
      ["Boat name, Sail number, Model / type, Builder, Designer", "Words that must start words of that field, in any order."],
      ["Country", "Only certificates from one country."],
      ["Built from / Built until", "Only boats built in those years, inclusive. A boat with no year is left out while either is set."],
      ["Certificate year", "The certificate's year, or its first digits."],
      ["Clear", "Empties every per-field box. The search box is kept."],
      ["Add", "Adds that certificate as a source. Added marks one the project already holds; adding it again asks first."],
      ["Added list", "Every ORC polar in the project with its colour, sail number, model and certificate year."],
      ["Remove (✕)", "Removes that ORC polar from the project. Undo puts it back."],
    ],
    related: ["sources", "polar-files", "tracks"] },
  { id: "polar-files", group: "Sources", title: "Polar files",
    paragraphs: [
      "Import… opens a file picker where several polar files can be chosen at once. Each file becomes one source, named after the file and given the next free colour. The format is read from the content, not the extension: Expedition (.txt: comment lines starting with !, then one row per wind speed, TWS followed by TWA and BSP pairs), or a TWA × TWS table as Adrena writes it (.pol, tab-separated) or a spreadsheet saves it (.csv, semicolon- or comma-separated), with TWA\\TWS, TWA/TWS or TWA in the top-left cell.",
      "A file that cannot be read is listed below the button with its line, its column and what is wrong there, and nothing of it is imported; the other files of the same import still are. Speeds above 60 kn and negative values are refused. Angles past 180° are the port side and are folded onto starboard. An imported polar is kept exactly as read; your edits are stored beside it.",
    ],
    parameters: [
      ["Polar files (section)", "Click the heading to fold or unfold the section."],
      ["Import…", "Chooses one or more polar files to import. One undo takes the whole import back out."],
      ["File list", "Every imported file with its colour, its format and its axes: the TWA and TWS each covers, and how many values it has."],
      ["Remove (✕)", "Removes that polar file from the project. Undo puts it back."],
    ],
    related: ["sources", "orc", "tracks"] },
  { id: "tracks", group: "Sources", title: "Tracks",
    paragraphs: [
      "The Tracks section imports race tracks of the boat. File… reads GeoJSON and CSV files, several at once; YellowBrick…, Geovoile… and Blue Water… import boats from a race tracker (see Race trackers). Each boat becomes one track source with the next free colour, and one undo takes a whole import back out. Positions are kept exactly as imported; filters and exclusions are stored beside them.",
      "GeoJSON files hold Point features, one position each, or LineString features with a time per position in properties.times or properties.coordTimes. Point properties are read whatever their case: time, timestamp or date (ISO 8601, or epoch seconds or milliseconds); cog, heading, hdg or course; sog, speed, bsp or stw (knots); and boat or name, which groups the points into one track per boat. A CSV needs a header row. The import dialog guesses the time, latitude, longitude, heading, speed and boat columns from the header and the first rows, shows a preview, and lets you correct each one, the time format (ISO 8601, epoch seconds or milliseconds, or your own pattern such as %d/%m/%Y %H:%M) and the speed unit. Times without a zone are UTC. When a file holds several boats, tick the ones to import.",
      "Every position gets a heading and a speed. The track's own heading (COG) and speed (SOG or boat speed) are used where it gives them; elsewhere they are derived from the neighbouring positions: the heading is the great-circle bearing from the previous position to the next, and the speed is the distance through the position over the time between its neighbours. The first and last positions, and those next to a gap longer than the maximum gap (3 hours by default), use the one neighbour they have; a position with none has no derived values. Positions at the same time are merged and positions out of order are sorted; the summary after an import says how many. Whether each value was given or derived is kept, so the filters can use it.",
      "Each sample becomes a dot in the polar plots once it has its wind, which Fetch weather… finds for it (see Wind, waves and current); importing never fetches weather, so until then a track is drawn on the map but has no place in the polar. Filtered samples stay in the project; they are drawn dimmed on the map, and in the plots when Show filtered is on.",
    ],
    parameters: [
      ["Tracks (section)", "Click the heading to fold or unfold the section."],
      ["File…", "Chooses GeoJSON or CSV files to import. The import dialog shows each file's boats, and for a CSV the column mapping."],
      ["YellowBrick…", "Imports boats from a YellowBrick race (see Race trackers)."],
      ["Geovoile…", "Imports boats from a Geovoile race (see Race trackers)."],
      ["Blue Water…", "Imports boats from a Blue Water Tracks race (see Race trackers)."],
      ["Track list", "Every track with its colour, boat, event or file, dates, the samples the blend uses out of all it has, and the weather status: not fetched, queued, fetching with how far it is, ready, partial or failed."],
      ["Show on map (⌖)", "Switches to the map and frames the track."],
      ["Filters (▸)", "Unfolds the track's sample filters and its heading and speed derivation. Every change is one undo."],
      ["From / To (UTC)", "The time window: samples outside it are filtered out, such as motoring before the start or after the finish."],
      ["Minimum / Maximum BSP", "Samples slower or faster than these are filtered out. The minimum is 1 kn by default; empty means no bound."],
      ["Manoeuvre threshold", "Samples whose heading changes more than this from a neighbour are filtered out (30° by default): tacks and gybes are not polar sailing. Empty keeps them."],
      ["Heading / Speed: given or derived", "Keep only samples whose value the track gave, or only derived ones, or either."],
      ["Wind, waves and current", "Ranges of TWS, TWA, wave height and current speed; the wave direction by sector or angle off the bow, or by compass direction; and leaving out currents without tide. A sample without the value a filter reads is left out."],
      ["Maximum gap", "Neighbours further apart in time than this are not used to derive heading and speed."],
      ["Prefer: given / derived values", "Use what the track gives where it gives it, or always derive heading and speed from the positions."],
      ["Select (tick box)", "Ticks the track for Fetch weather for selected tracks…."],
      ["Fetch weather… / Cancel fetch", "On the track's row: fetches the wind, waves and current the samples do not have yet, or all of them again once the track is ready (see Wind, waves and current); while it runs, Cancel fetch stops it."],
      ["Fetch weather for selected tracks…", "Fetches the weather of every ticked track, one after another, after one download estimate."],
      ["Export reanalysis GRIB…", "Arrives in a later version."],
      ["Remove (✕)", "Removes the track from the project. Undo puts it back."],
    ],
    related: ["trackers", "environment", "map", "sources", "polar-3d"] },
  { id: "trackers", group: "Sources", title: "Race trackers",
    paragraphs: [
      "YellowBrick… imports boats from a race followed on YellowBrick. Paste the race’s address — a yb.tl link, a cf.yb.tl or app.yb.tl viewer link — or its race key, such as fastnet2025, and choose Open. PolarEffects downloads the whole event, without any weather: its title and dates and every boat’s full track, with its progress shown and a Cancel. The boats are listed as soon as the tracker names them (YellowBrick and Geovoile name them first), so you can search and tick boats while the positions still download. Only the public race data is read; no account, purchase or device key is ever used.",
      "The dialog then lists every boat with its sail number, model, division, number of positions and status, above a small map of the fleet’s tracks. Type in the search box to find boats by name, sail number, model or division; tick the boats to import, and they are drawn highlighted on the map. Import tracks adds one track per ticked boat with the next free colours, and one undo takes them all back out. No weather is fetched: use Fetch weather… on a track, or Fetch weather for selected tracks…, when you want it.",
      "YellowBrick gives positions only, so each boat’s heading and speed are derived from its positions. A boat’s time window starts at its start and ends at its finish when the tracker gives them (otherwise the race’s start and end), so motoring before the start and after the finish is filtered out; change it in the track’s filters. The track keeps the event’s address and title and the boat’s tracker id, sail number, model and division.",
      "Geovoile… reads the races Geovoile follows from about 2016 on. Paste the race’s viewer address, such as vendeeglobe.geovoile.com/2016/tracker/ (or …/viewer/). A race sailed in legs is imported one leg at a time: add ?leg=2 to the address for the second leg, or choose the leg in the dialog once one is open. Geovoile’s official heading and speed are used for the positions its reports cover, and the rest are derived; the division is the boat’s class, and a boat’s time window ends at its official arrival. Older Geovoile trackers (Flash, or from 2012–2015) are not supported, and the dialog says so.",
      "Blue Water… reads a race’s public data from its own API. Paste the race’s page address, such as race.bluewatertracks.com/2025-melbourne-hobart-westcoaster, or its race key alone. Course and speed over ground are given for every position and used as they are, so nothing is derived; the division is the boat’s handicap class, and a boat’s time window ends at its own official finish, or otherwise the race’s tracked end.",
      "Downloaded events stay in memory until PolarEffects closes (once they hold two million positions in all, the oldest goes first), so opening the same race again, to import another boat, downloads nothing; Download again fetches newer positions. When YellowBrick’s compact position file cannot be read, its KML export is read instead, which is slower. Some races answer with a server error: the dialog says so and offers Retry.",
    ],
    parameters: [
      ["Event address", "A yb.tl, cf.yb.tl or app.yb.tl link, or the race key alone; for Geovoile, the race’s viewer address; for Blue Water Tracks, its race.bluewatertracks.com link or race key."],
      ["Open", "Downloads every boat’s track, or opens the event kept from earlier in this session."],
      ["Cancel", "Stops any download under way and closes the dialog; nothing is imported."],
      ["Retry", "After a failure, asks the tracker again."],
      ["Download again", "For an event kept from earlier in this session: downloads it again."],
      ["Search", "Shows only the boats whose name, sail number, model or division contains every word typed; case and accents do not matter."],
      ["Boats", "Tick the boats to import. A boat the tracker has no positions for cannot be ticked. The box in the heading ticks or unticks every boat shown."],
      ["Leg", "For a Geovoile race sailed in legs: the leg shown. Choosing another downloads it."],
      ["Import tracks", "Adds one track per ticked boat, as one undo. It waits for the positions, and fetches no weather."],
    ],
    related: ["tracks", "environment"] },
  { id: "environment", group: "Sources", title: "Wind, waves and current",
    paragraphs: [
      "Every track sample is matched with reanalysis wind, waves and current at its time and place. Importing never fetches it: Fetch weather… on a track, or Fetch weather for selected tracks… for the ticked ones, starts it. A dialog opens at once and shows, when it has worked them out (calculating… until then), how many samples it covers, what it will download sampling hourly or every three hours (less what this session has already downloaded), and about how much the project grows by. Only the parts of each archive field that hold the track's positions are downloaded — about a tenth of the whole field — and the project keeps only the wind, waves and current at each sample: a few kilobytes per track. Hourly is chosen unless it would download more than 1 GB (a long ocean race). The fetch runs in the background, one track after another, and what it downloads is kept in memory for the session (Settings), so the second boat of a race downloads almost nothing. Nothing downloaded is written to disk. The status bar and the track list show its progress, and Cancel fetch stops it: the samples already fetched are kept and saved with the project (status partial), and Fetch weather… resumes with the rest.",
      "Wind (10 m, u and v) comes from WeatherBench2's ERA5 until 10 January 2023 and from ARCO-ERA5 after it; waves (significant height and mean direction) from ARCO-ERA5. Values are interpolated bilinearly between the four surrounding grid points and linearly between hours; wind interpolates its components and the wave direction is interpolated as a unit vector. Grid points on land are left out, and a place with land all round has no waves or current. The current comes from the first source that has it: the NW European Shelf or Iberia–Biscay–Ireland reanalyses (tidal, from 1993), the global merged current (circulation plus tide, from November 2020, with Stokes drift if chosen), or GlobCurrent (geostrophic, Ekman and tide from FES2022, from 1993).",
      "Wind and current are both over the ground; a polar is through the water. Where a current was found, the boat's velocity through the water is its ground velocity minus the current (leeway ignored), and the wind over the water is the wind minus the current. Both are stored, and Correct for current chooses which feed the polar. The wave angle is measured off the bow: 0° head seas, 180° following. Each sample records the dataset and version that supplied it. Wind and current speeds are kept to 0.01 kn, directions to 0.1° and wave heights to a centimetre; the angles and corrected values are worked out from them again whenever the project is opened. Changing a track's heading and speed derivation recomputes the angles from the stored environment without fetching again.",
    ],
    parameters: [
      ["Fetch dialog", "The samples to fetch and the download hourly and every 3 hours. Fetch starts it; Not now leaves the track not fetched until Fetch weather…."],
      ["Hourly / Every 3 hours", "How finely wind and waves are read in time. Every 3 hours reads a third as much and interpolates between; currents are always hourly."],
      ["Status bar", "The track being fetched, how far it is and how many wait, with Cancel fetch, which stops every fetch."],
      ["Fetch weather… / Cancel fetch", "On a track's row: fetches what is missing, or every sample again once the track is ready or the interval changes; while it runs, Cancel fetch stops that track's fetch."],
      ["Correct for current", "Feeds the polar from boat speed, TWS and TWA through the water where a current was found (on by default). One undo."],
      ["Include Stokes drift", "Adds the waves' Stokes drift to the global merged current, from the next fetch (off by default). One undo."],
      ["Leave out currents without tide", "A track filter: leaves out the samples whose current came from a source marked without tide. Every current source read today includes the tide, so it leaves nothing out unless such a source is added."],
      ["Replacing the project", "New, Open or Close while a fetch runs asks to cancel it first."],
    ],
    related: ["tracks", "settings", "map"] },
  { id: "sources", group: "Sources", title: "Source list and polar plot",
    paragraphs: [
      "The right panel lists every source of the project — ORC polars, polar files and tracks — each with its colour, a show or hide switch and a blend weight. A hidden source is left out of the blend and of every plot.",
      "Below the list, the 2D polar plot draws boat speed against true wind angle: a curve per visible polar source at one true wind speed, or one curve per wind speed a source has when the slider is set to All, each in its source's colour, with the blend drawn thicker in its own colour. Track samples with their wind are dots in their track's colour when their wind speed is within a knot of the slice (the band is a setting); excluded samples are hollow, and samples selected on the map or in the 3D view are ringed. Hovering a curve or a dot shows its source, TWA, TWS and BSP. Full size opens the same plot as a large overlay on the map, closed with its own button, Escape or switching stage.",
    ],
    parameters: [
      ["Sources (section)", "Click the heading to fold or unfold the list."],
      ["Polar plot (section)", "Click the heading to fold or unfold the plot."],
      ["Blend", "The top entry stands for the blended polar: its colour, a show or hide switch, how many cells have direct evidence and how many were filled, Blend settings and Export (see The blend and export)."],
      ["Colour", "Click the swatch for the palette of sixteen colours or a custom colour. A new source takes the first palette colour no source uses."],
      ["Show or hide", "The checkbox. A hidden source is left out of the blend and of every plot."],
      ["Name", "Click a source’s name to rename it; Enter keeps the new name, Esc cancels."],
      ["Kind and count", "The symbol says whether the source is an ORC polar, a polar file or a track. The count is the cells a polar holds, or the samples a track uses out of all it has."],
      ["Weight", "How much the source counts in the blend, from 0 to 2 (1 by default). One drag of the slider is one undo."],
      ["Edit / Compare", "Edit opens the source in the 3D view to edit its polar (see The 3D polar); Compare arrives in a later version. A ✎ after Edit means the source holds edits."],
      ["Remove", "Removes the source from the project. Undo puts it back."],
      ["Reorder (⠿)", "Drag a source by its handle, or focus the handle and press the up and down arrow keys. The order is only how the list is shown."],
      ["All / wind speed slider", "All draws one curve per wind speed each visible source has; the slider picks one true wind speed instead."],
      ["Filtered", "Also draws the samples the track filters take out, dimmed. Offered when the project has a visible track."],
      ["Full size", "Opens the polar plot as a large overlay on the map stage. Close it, press Escape, or switch stage to return."],
    ],
    related: ["workspace", "blend", "polar-3d", "compare"] },
  { id: "blend", group: "Sources", title: "The blend and export",
    paragraphs: [
      "The blend is the one polar PolarEffects makes from the visible sources, and the polar it exports. It is made on the project’s output grid: each polar source is read onto that grid between its own points, never beyond them, with its edits in and every cell read from an excluded point left out; each track counts through its polar segment, binned on the same grid.",
      "Each cell is the weighted mean of the sources with a value there. A polar source counts by its weight; a track cell by its weight times its samples over the samples for full confidence (30 by default), at most fully, and a cell you edited counts fully. Cells no source reaches are then filled between known values, first along the true wind angle at the same wind speed, then along the wind speed. Nothing is extrapolated, so a cell beyond every source stays empty. The 0° row is 0 kn. Smoothing, off by default, evens the filled grid.",
      "The Blend entry at the top of the source list shows how many cells have direct evidence and how many were filled. The blend is drawn thicker in the polar plot and opaque in the 3D view, in its own colour. Hiding it hides it from the plots only; export always writes it.",
      "Export writes the blend as an Expedition (.txt), Adrena (.pol) or CSV (.csv) polar, recomputed from the sources at that moment. Axis values are written with at most two decimals and boat speeds with two, so the same project always gives the same bytes on every computer. The dialog previews the grid first; custom axes write the blend read onto them, without extrapolating.",
    ],
    parameters: [
      ["Blend colour", "Click the swatch to change the colour the blend is drawn in."],
      ["Show or hide the blend", "Draws the blend in the plots or not. Export is not affected."],
      ["Coverage", "Cells with direct evidence from a source, and cells filled between them. The tooltip also counts the cells left empty."],
      ["Blend settings", "The output grid, the statistic new tracks start with, the samples a track cell needs, the samples for full confidence, current correction, Stokes drift and smoothing. Apply makes them one change, undone with Undo."],
      ["Output grid", "True wind angles (0 to 180) and wind speeds (0 to 70 kn), in increasing order, with at most two decimals and so at least 0.01 apart: every export then reads back. Default grid puts back the grid a new project starts with."],
      ["Statistic for new tracks", "The statistic of a cell’s boat speeds a newly imported track starts with: the 90th percentile by default. Each track keeps its own, changed in its edit panel."],
      ["Samples a cell needs", "A track cell with fewer samples has no value (5 by default)."],
      ["Samples for full confidence", "A track cell counts in the blend by its samples over this number, at most fully (30 by default)."],
      ["Export…", "Opens the export dialog: the format, the grid and a preview, then where to save."],
      ["Format", "Expedition: one row per wind speed holding only the angles that have a value. Adrena: a tab-separated table. CSV: the same table with semicolons."],
      ["Grid", "The project’s output grid, or custom axes the blend is read onto. A grid two of whose values would be written the same with two decimals is refused, and the message names both values."],
    ],
    related: ["sources", "polar-3d", "tracks"] },
  { id: "map", group: "Views", title: "The world map",
    paragraphs: [
      "The map is the default stage. It draws the land and coastlines of the world from data built into the application; no map tiles are ever downloaded.",
      "Every visible track is drawn in its source colour; positions the filters take out are drawn faint, and excluded ones paler. A track that crosses the 180° meridian is drawn as one line across it. Hovering a position shows its time, boat speed and heading (given or derived), and its TWS, TWA, wave height and current once the environment has been fetched.",
      "Shift-drag a box to select the positions inside it. The selection is shared with the polar views: the same samples are selected in the 3D view and ringed in the polar plot, and Show on map in the 3D view highlights its selected samples here and frames them.",
    ],
    parameters: [
      ["Projection: Equirectangular / Orthographic", "Equirectangular draws longitude and latitude as a flat grid. Orthographic draws a globe as seen from space. The choice is remembered for you."],
      ["Drag", "Pans the flat map, or turns the globe."],
      ["Scroll or pinch", "Zooms in and out around the pointer."],
      ["Fit the world", "Shows the whole world again."],
      ["Fit the tracks", "Frames every visible track."],
      ["Shift-drag", "Selects the track positions inside the box, here and in the polar views. Escape or Clear selects none."],
    ],
    related: ["workspace", "tracks"] },
  { id: "polar-3d", group: "Views", title: "The 3D polar",
    paragraphs: [
      "The 3D stage shows every visible polar source as a translucent surface over its own grid, with its grid points as dots, and every track sample that has its wind as a dot in its track's colour. A sample without wind (before the environment is fetched) has no place in the polar and is not drawn. Nothing is resampled or extrapolated: an empty cell of a source is a hole in its surface.",
      "In the polar tower (the default) the angle round the vertical axis is TWA, the distance from it is BSP and the height is TWS, so each wind speed is a classic polar curve and the stack is a surface. The Cartesian layout puts TWA, TWS and BSP on three straight axes. The axes are labelled in your speed unit.",
      "Select dots by clicking one, Shift-clicking to add, or drawing a lasso or a box round many. Only dots that are drawn are counted and acted on. The selection panel shows how many are selected, their mean TWA, TWS and BSP, and how many come from each source. Exclude removes the selection from the blend: a grid point is then drawn as a cross, and that cell is empty for that source when the blend is made; a sample is drawn as a ring and left out of its track's polar segment. Include puts them back. Both are ordinary changes, undone with Undo. Nothing in the source itself changes. The samples selected here are selected on the map too, and a box on the map selects them here.",
      "Edit on a source in the source list opens it here in edit mode. The source being edited is drawn opaque and the others fade (Hide other sources hides them). What is edited is the source’s own polar: the imported grid of a polar file, the VPP grid of an ORC certificate, or, for a track, its polar segment — the samples that pass the filters and are not excluded, binned onto the project’s output grid with port and starboard folded together; a cell holds the track’s statistic of their boat speeds (the 90th percentile by default) once it has at least five samples, and keeps its sample count and spread for the table’s tooltips. Every edit is stored beside the source, never in it: an edited node is drawn as a square and highlighted in the table, every view updates at once, and Reset all edits gives the source back exactly as imported. Surfaces, nodes and the 2D curves show each source as edited; the 2D curves also leave out excluded nodes, as the blend does.",
    ],
    parameters: [
      ["Layout: Polar tower / Cartesian", "How TWA, TWS and BSP are placed in the scene."],
      ["Top / Side / Isometric", "Preset cameras: straight down the wind-speed axis (the classic polar diagram), across it, or the three-quarter view."],
      ["Rotate / Lasso / Box", "With Rotate, drag to turn the view, right-drag to pan and scroll to zoom. With Lasso or Box, dragging selects instead. A click selects the nearest dot in every tool; Shift adds to the selection; Escape clears it."],
      ["Show: Samples / Polar nodes / Surfaces / Filtered samples", "What is drawn. Filtered samples are those the track filters remove, drawn dimmed."],
      ["Colour", "Colours the dots by source, or by wave height, current speed or time. The last three need track samples with their environment and are offered once those exist."],
      ["Exclude / Include", "Remove the selection from the blend, or put it back. Excluded grid points are drawn as crosses, excluded samples as rings."],
      ["Show on map", "Switches to the map, highlights the selected samples there and frames them."],
      ["Edit (source list)", "Opens the source in the 3D view in edit mode, with its table. Done leaves edit mode; the edits stay."],
      ["Drag", "In edit mode, drag a node of the source being edited to change its boat speed. Shift snaps to 0.05 kn. One drag is one undo."],
      ["Table", "The source’s polar in knots, TWA down and TWS across. Type a value and press Enter to edit a cell; empty the cell to reset it. Each typed value is one undo. Clicking a cell selects it, Shift-click adds, and edited cells are highlighted."],
      ["Scale / Smooth / Reset", "Act on the selected cells: scale them by the percentage, smooth each over its neighbours on the grid (a 3 × 3 kernel), or put them back to the source’s values. Each is one undo."],
      ["Reset all edits", "Clears every edit of the source being edited; undo puts them back."],
      ["Statistic", "For a track: how a cell of its polar segment sums up its samples’ boat speeds — the 90th or 75th percentile, the median or the mean."],
      ["Hide other sources", "While a source is edited, hides the other sources instead of fading them."],
    ],
    related: ["sources", "compare"] },
  { id: "compare", group: "Views", title: "Compare",
    paragraphs: [
      "The Compare stage will show the difference between two polars — two sources, or a source and the blend — cell by cell.",
    ],
    related: ["polar-3d", "sources"] },
  { id: "settings", group: "Settings and help", title: "Settings",
    paragraphs: [
      "Settings apply to the whole application and every project, and are never stored in a project. Open them with the gear in the title bar, the Settings button on the start screen, or Cmd+, (Ctrl+, on Windows and Linux).",
    ],
    parameters: [
      ["Language", "English, French or German. Everything changes at once, including the menu bar. Also on the start screen."],
      ["Theme", "The colours of the application. Harbour is the default."],
      ["Speed / Wave height / Distance", "The units values are shown in. Stored values do not change."],
      ["Polar plot dot band", "How far from the polar plot's wind speed a track sample may be and still be drawn as a dot, from ±0.25 to ±5 kn (±1 kn by default)."],
      ["Autosave", "Keep a recovery copy of unsaved work (the default), save into the project file itself, or leave everything until you save."],
      ["Keep downloaded weather in memory", "How much downloaded wind, wave and current data is kept in memory for the session (256 MB by default, 16–4096 MB), so other boats of the same race reuse it. It is forgotten when PolarEffects quits; nothing downloaded is kept on disk, and every value a project uses is saved in the project. An earlier version's chunk cache folder is removed the first time, as the status line says."],
      ["Concurrent requests / Request timeout", "How many downloads run at once (8 by default) and how long one may take before it is abandoned."],
    ],
    related: ["projects", "search"] },
  { id: "search", group: "Settings and help", title: "Search and help",
    paragraphs: [
      "The search box in the title bar finds any control by its name or by words it is known by, in the language on screen, with or without accents. Results appear as you type. Choose one and PolarEffects opens whatever hides it — a panel, a section, a stage, a menu or Settings — and outlines it in orange for a moment.",
      "Help pages are listed below the controls in the results. This reference opens with F1, the ? button or the Help menu, and has its own search.",
    ],
    parameters: [
      ["Search (Cmd+F / Ctrl+F)", "Moves to the search box from anywhere."],
      ["? (F1)", "Opens this reference."],
      ["Arrow keys and Enter", "Choose a result without the mouse. Escape closes the list."],
    ],
    related: ["workspace", "settings"] },
];

/**
 * The reference in each language. Every translation has the same pages, in
 * the same order, with the same ids and cross-references — only the words
 * differ; `topics.test.ts` holds them to that shape.
 */
const translations = import.meta.glob<{ default: HelpTopic[] }>("./locales/*.ts", { eager: true });

export function topicsFor(language: Language): HelpTopic[] {
  if (language === "en") return TOPICS;
  return translations[`./locales/${language}.ts`]?.default ?? TOPICS;
}

/** Topics whose text holds every word of the query, in the given language. */
export function searchTopics(query: string, topics: HelpTopic[] = topicsFor(language())): HelpTopic[] {
  const words = fold(query).trim().split(/\s+/).filter(Boolean);
  return topics.filter((topic) => {
    const text = fold(JSON.stringify(topic));
    return words.every((word) => text.includes(word));
  });
}
