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
      "The ORC polars section of the left navigation will search the built-in catalogue of ORC certificates by boat name, sail number or type, and add the chosen ones to the project as sources. The catalogue is part of the application and is never downloaded.",
    ],
    parameters: [
      ["ORC polars (section)", "Click the heading to fold or unfold the section."],
    ],
    related: ["sources", "polar-files", "tracks"] },
  { id: "polar-files", group: "Sources", title: "Polar files",
    paragraphs: [
      "The Polar files section will import existing polars in Expedition or Adrena format. An imported polar is kept exactly as read; your edits are stored beside it.",
    ],
    parameters: [
      ["Polar files (section)", "Click the heading to fold or unfold the section."],
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
      "Below the list, the 2D polar plot draws boat speed against true wind angle for one true wind speed, with the samples of the tracks as dots.",
    ],
    parameters: [
      ["Sources (section)", "Click the heading to fold or unfold the list."],
      ["Polar plot (section)", "Click the heading to fold or unfold the plot."],
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
      "The 3D stage will show a polar as a surface of boat speed over true wind angle and true wind speed, with the track samples as dots, and will let you exclude dots and edit one source at a time.",
    ],
    related: ["compare", "sources"] },
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
