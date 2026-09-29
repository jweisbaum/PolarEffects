/**
 * Native file dialogs. Copied from VectorEffects.
 *
 * A web `<input type="file">` cannot give a real path, which a desktop app
 * needs in order to save back to the same file, so these go through Tauri's
 * dialog plugin.
 */
import { open, save } from "@tauri-apps/plugin-dialog";

import { takeDialogAnswer } from "../automation";
import { whileChoosing as choosing } from "../busy";
import { msg, t } from "../i18n";

/**
 * Shows a native dialog, unless the WebDriver tools queued its answer
 * (development builds only, `automation.ts`).
 */
function whileChoosing<T>(busy: string, dialog: () => Promise<T>): Promise<T> {
  const queued = takeDialogAnswer();
  if (queued !== undefined) return Promise.resolve(queued as T);
  return choosing(busy, dialog);
}

/** The project file extension (spec.md 4.3). */
export const EXTENSION = "wpsproj";

/** A function, not a constant: the filter's name is read in the language on screen. */
const filters = () => [{ name: t("PolarEffects project"), extensions: [EXTENSION] }];

/** Asks for a project to open. Returns null if the user cancelled. */
export async function pickProjectToOpen(): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing a project"), () =>
    open({ multiple: false, directory: false, filters: filters() }),
  );
  return typeof chosen === "string" ? chosen : null;
}

/** Asks where to save. Returns null if the user cancelled. */
export async function pickProjectToSave(suggestedName: string): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing where to save"), () =>
    save({ defaultPath: `${suggestedName}.${EXTENSION}`, filters: filters() }),
  );
  return typeof chosen === "string" ? chosen : null;
}

/**
 * Asks for polar files to import (spec.md 6): several at once. The format is
 * read from the content, so "All files" is offered beside the usual
 * extensions. Returns the chosen paths; empty if the user cancelled.
 */
export async function pickPolarFiles(): Promise<string[]> {
  const chosen = await whileChoosing(msg("Choosing polar files"), () =>
    open({
      multiple: true,
      directory: false,
      title: t("Import polar files"),
      filters: [
        { name: t("Polar files"), extensions: ["txt", "pol", "csv"] },
        { name: t("All files"), extensions: ["*"] },
      ],
    }),
  );
  if (Array.isArray(chosen)) return chosen.filter((path): path is string => typeof path === "string");
  return typeof chosen === "string" ? [chosen] : [];
}

/**
 * Asks for track files to import (spec.md 7.3): GeoJSON or CSV, several at
 * once. Returns the chosen paths; empty if the user cancelled.
 */
export async function pickTrackFiles(): Promise<string[]> {
  const chosen = await whileChoosing(msg("Choosing track files"), () =>
    open({
      multiple: true,
      directory: false,
      title: t("Import tracks"),
      filters: [
        { name: t("Track files"), extensions: ["geojson", "json", "csv", "txt", "tsv"] },
        { name: t("All files"), extensions: ["*"] },
      ],
    }),
  );
  if (Array.isArray(chosen)) return chosen.filter((path): path is string => typeof path === "string");
  return typeof chosen === "string" ? [chosen] : [];
}

/** The file extension each export format is written with (spec.md 6). */
export const EXPORT_EXTENSIONS: Readonly<Record<string, string>> = {
  expedition: "txt",
  adrena: "pol",
  csv: "csv",
};

/**
 * Asks where to export the blend (spec.md 12), suggesting `<name>.<ext>`
 * for the format. Returns null if the user cancelled.
 */
export async function pickExportPath(format: string, suggestedName: string): Promise<string | null> {
  const extension = EXPORT_EXTENSIONS[format] ?? "txt";
  const chosen = await whileChoosing(msg("Choosing where to export"), () =>
    save({
      title: t("Export the polar"),
      defaultPath: `${suggestedName}.${extension}`,
      filters: [{ name: t("Polar file"), extensions: [extension] }],
    }),
  );
  return typeof chosen === "string" ? chosen : null;
}
