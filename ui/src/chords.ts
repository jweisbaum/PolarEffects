import { language } from "./i18n";

/**
 * Keyboard chords that differ by platform: Cmd on a Mac, Ctrl elsewhere.
 */

/** Whether this is a Mac, whose command key is Cmd rather than Ctrl. */
export const IS_MAC = /Mac|iPhone|iPad/.test(globalThis.navigator?.platform ?? "");

/** How the accelerator key is written in a tooltip, in English. */
export const ACCEL = IS_MAC ? "Cmd" : "Ctrl";

/**
 * A chord as the language on screen writes it (glossary, M17b review):
 * English "Cmd+Shift+S" / "Ctrl+Shift+S", French "⌘+Maj+S" / "Ctrl+Maj+S",
 * German "⌘+Umschalt+S" / "Strg+Umschalt+S". `"accel"` and `"shift"` stand
 * for the modifiers; anything else is the key as printed on it. Called at
 * render, so a language switch rewrites every tooltip that shows one.
 */
export function chordText(keys: readonly string[], lang: string = language(), mac: boolean = IS_MAC): string {
  const names = MODIFIERS[lang] ?? MODIFIERS.en!;
  return keys.map((key) => key === "accel" ? (mac ? names.cmd : names.ctrl) : key === "shift" ? names.shift : key).join("+");
}

const MODIFIERS: Record<string, { cmd: string; ctrl: string; shift: string }> = {
  en: { cmd: "Cmd", ctrl: "Ctrl", shift: "Shift" },
  fr: { cmd: "⌘", ctrl: "Ctrl", shift: "Maj" },
  de: { cmd: "⌘", ctrl: "Strg", shift: "Umschalt" },
};

/** Whether an event holds the platform's accelerator and nothing it should not. */
export function isAccel(event: Pick<KeyboardEvent, "metaKey" | "ctrlKey">, mac: boolean = IS_MAC): boolean {
  return mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}
