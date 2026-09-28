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
      "Add copies the certificate into the project as an ORC polar with the next free colour. Its polar has the ORC angles (52° to 150°) at the certificate's own wind speeds, plus the beat and run angle at each wind speed, where boat speed is the VMG divided by the cosine of the angle. Nothing is filled in between or beyond those angles. Adding a certificate the project already holds asks first.",
    ],
    parameters: [
      ["ORC polars (section)", "Click the heading to fold or unfold the section."],
      ["Search", "Words to find, in any order. Each result shows the name, sail number, model, year built, builder, certificate year and a small polar at light, medium and strong wind."],
      ["From / To", "Only boats built in those years, inclusive. A boat with no year is left out while either is set."],
      ["Country", "Only certificates from one country."],
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
      "The Tracks section will import race tracks of the boat from YellowBrick, Geovoile, Blue Water Tracks or GeoJSON and CSV files. Each position is matched with the wind, waves and current of the time, and the track becomes a polar segment that can be filtered and blended.",
    ],
    parameters: [
      ["Tracks (section)", "Click the heading to fold or unfold the section."],
    ],
    related: ["sources", "map", "orc"] },
  { id: "sources", group: "Sources", title: "Source list and polar plot",
    paragraphs: [
      "The right panel lists every source of the project — ORC polars, polar files and tracks — each with its colour, a show or hide switch and a blend weight. A hidden source is left out of the blend and of every plot.",
      "Below the list, the 2D polar plot draws boat speed against true wind angle: a curve per visible polar source at one true wind speed, or one curve per wind speed a source has when the slider is set to All, each in its source's colour, with the blend drawn thicker once it exists. Track samples within a knot of the slice will appear as dots. Hovering a curve or a dot shows its source, TWA, TWS and BSP. Full size opens the same plot as a large overlay on the map, closed with its own button, Escape or switching stage.",
    ],
    parameters: [
      ["Sources (section)", "Click the heading to fold or unfold the list."],
      ["Polar plot (section)", "Click the heading to fold or unfold the plot."],
      ["Blend", "The top entry stands for the blended polar. Its show or hide switch and Blend settings arrive with the blend."],
      ["Colour", "Click the swatch for the palette of sixteen colours or a custom colour. A new source takes the first palette colour no source uses."],
      ["Show or hide", "The checkbox. A hidden source is left out of the blend and of every plot."],
      ["Name", "Click a source’s name to rename it; Enter keeps the new name, Esc cancels."],
      ["Kind and count", "The symbol says whether the source is an ORC polar, a polar file or a track. The count is the cells a polar holds, or the samples a track uses out of all it has."],
      ["Weight", "How much the source counts in the blend, from 0 to 2 (1 by default). One drag of the slider is one undo."],
      ["Edit / Compare", "Open the source in the 3D view, or compare it with another. Both arrive in later versions."],
      ["Remove", "Removes the source from the project. Undo puts it back."],
      ["Reorder (⠿)", "Drag a source by its handle, or focus the handle and press the up and down arrow keys. The order is only how the list is shown."],
      ["All / wind speed slider", "All draws one curve per wind speed each visible source has; the slider picks one true wind speed instead."],
      ["Full size", "Opens the polar plot as a large overlay on the map stage. Close it, press Escape, or switch stage to return."],
    ],
    related: ["workspace", "polar-3d", "compare"] },
  { id: "map", group: "Views", title: "The world map",
    paragraphs: [
      "The map is the default stage. It draws the land and coastlines of the world from data built into the application; no map tiles are ever downloaded. Tracks will be drawn on it in their source colours.",
    ],
    parameters: [
      ["Projection: Equirectangular / Orthographic", "Equirectangular draws longitude and latitude as a flat grid. Orthographic draws a globe as seen from space. The choice is remembered for you."],
      ["Drag", "Pans the flat map, or turns the globe."],
      ["Scroll or pinch", "Zooms in and out around the pointer."],
      ["Fit the world", "Shows the whole world again."],
    ],
    related: ["workspace", "tracks"] },
  { id: "polar-3d", group: "Views", title: "The 3D polar",
    paragraphs: [
      "The 3D stage shows every visible polar source as a translucent surface over its own grid, with its grid points as dots; track samples join them as dots once tracks arrive. Nothing is resampled or extrapolated: an empty cell of a source is a hole in its surface.",
      "In the polar tower (the default) the angle round the vertical axis is TWA, the distance from it is BSP and the height is TWS, so each wind speed is a classic polar curve and the stack is a surface. The Cartesian layout puts TWA, TWS and BSP on three straight axes. The axes are labelled in your speed unit.",
      "Select dots by clicking one, Shift-clicking to add, or drawing a lasso or a box round many. The selection panel shows how many are selected, their mean TWA, TWS and BSP, and how many come from each source. Exclude removes the selected grid points from the blend: the point is then drawn as a cross, and that cell is empty for that source when the blend is made. Include puts them back. Both are ordinary changes, undone with Undo. Nothing in the source itself changes.",
    ],
    parameters: [
      ["Layout: Polar tower / Cartesian", "How TWA, TWS and BSP are placed in the scene."],
      ["Top / Side / Isometric", "Preset cameras: straight down the wind-speed axis (the classic polar diagram), across it, or the three-quarter view."],
      ["Rotate / Lasso / Box", "With Rotate, drag to turn the view, right-drag to pan and scroll to zoom. With Lasso or Box, dragging selects instead. A click selects the nearest dot in every tool; Shift adds to the selection; Escape clears it."],
      ["Show: Samples / Polar nodes / Surfaces / Filtered samples", "What is drawn. Filtered samples are those the track filters remove, drawn dimmed; they arrive with tracks."],
      ["Colour", "Colours the dots by source, or by wave height, current speed or time. The last three need track samples with their environment and are offered once those exist."],
      ["Exclude / Include", "Remove the selection from the blend, or put it back. Excluded grid points are drawn as crosses, excluded samples as rings."],
      ["Show on map", "Highlights the selected samples on the map. It arrives with tracks."],
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
      ["Autosave", "Keep a recovery copy of unsaved work (the default), save into the project file itself, or leave everything until you save."],
      ["Chunk cache", "Where downloaded wind, wave and current data is kept, and how large that folder may grow (20 GB by default). Clear cache empties it; nothing is lost, because every value a project uses is saved in the project."],
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
