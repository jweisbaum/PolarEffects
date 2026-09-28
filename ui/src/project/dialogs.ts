/**
 * Native file dialogs. Copied from VectorEffects.
 *
 * A web `<input type="file">` cannot give a real path, which a desktop app
 * needs in order to save back to the same file, so these go through Tauri's
 * dialog plugin.
 */
import { open, save } from "@tauri-apps/plugin-dialog";

import { whileChoosing } from "../busy";
import { msg, t } from "../i18n";

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

/** Asks for the folder the chunk cache goes in (spec.md 3.4). */
export async function pickCacheFolder(): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing a folder"), () =>
    open({ multiple: false, directory: true, title: t("Choose where to keep the chunk cache") }),
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
