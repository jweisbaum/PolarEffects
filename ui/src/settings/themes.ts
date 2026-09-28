/**
 * The bundled themes (spec.md 3.1), applied as CSS variables on the document.
 * Copied from VectorEffects; Harbour is the default. The Custom theme editor
 * is deferred (spec.md 14).
 *
 * `--flash`, the feature-search highlight, is the same in every theme and
 * lives in the stylesheet, not here.
 */
import catalogue from "./themes.json";
import { msg, t } from "../i18n";

export const THEMES = catalogue;
export const DEFAULT_THEME = "harbour";
export type Theme = typeof THEMES[number];
export type MapColour = keyof Theme["map"];

/**
 * The bundled themes' names, marked for the catalogues: `themes.json` holds
 * them in English, and they are translated where they are shown.
 */
export const THEME_NAMES: readonly string[] = [
  msg("Harbour"), msg("Midnight"), msg("Ocean"), msg("Plum"), msg("Ember"), msg("Paper"),
];

/** A theme's name in the language on screen. */
export function themeName(theme: Theme): string {
  return t(theme.name);
}

export function themeOf(id: string | undefined): Theme {
  return THEMES.find(theme => theme.id === id) ?? THEMES.find(theme => theme.id === DEFAULT_THEME)!;
}

let current = themeOf(DEFAULT_THEME);
const listeners = new Set<() => void>();

/** Applied to the document, including portals and the start screen. */
export function applyTheme(id: string | undefined): void {
  current = themeOf(id);
  if (typeof document !== "undefined") {
    const root = document.documentElement;
    root.dataset.theme = current.id;
    root.dataset.themeScheme = current.scheme;
    root.style.colorScheme = current.scheme;
    for (const [role, colour] of Object.entries(current.roles)) root.style.setProperty(`--${role}`, colour);
  }
  for (const listener of listeners) listener();
}

/** Called when the theme changes, so a canvas can redraw. */
export function onThemeChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** `#rrggbb` as four 0–1 channels. */
export function rgba(hex: string, alpha = 1): [number, number, number, number] {
  return [parseInt(hex.slice(1, 3), 16) / 255, parseInt(hex.slice(3, 5), 16) / 255, parseInt(hex.slice(5, 7), 16) / 255, alpha];
}

/** A map colour of the active theme: the canvas follows the theme like the DOM. */
export function mapColour(role: MapColour): string {
  return current.map[role];
}
