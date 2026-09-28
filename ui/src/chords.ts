/**
 * Keyboard chords that differ by platform: Cmd on a Mac, Ctrl elsewhere.
 */

/** Whether this is a Mac, whose command key is Cmd rather than Ctrl. */
export const IS_MAC = /Mac|iPhone|iPad/.test(globalThis.navigator?.platform ?? "");

/** How the accelerator key is written in a tooltip. */
export const ACCEL = IS_MAC ? "Cmd" : "Ctrl";

/** Whether an event holds the platform's accelerator and nothing it should not. */
export function isAccel(event: Pick<KeyboardEvent, "metaKey" | "ctrlKey">, mac: boolean = IS_MAC): boolean {
  return mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}
