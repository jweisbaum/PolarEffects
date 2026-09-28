// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { IS_MAC } from "../chords";
import { setLanguage } from "../i18n";
import HelpMenu, { isSearchChord } from "./HelpMenu";
import { onReveal } from "./highlight";
import { onOpenHelp } from "./open";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;
const input = () => host.querySelector<HTMLInputElement>('input[type="search"]')!;
const options = () => [...host.querySelectorAll<HTMLLIElement>('[role="option"]')];
const labels = () => options().map(o => o.querySelector(".help-search-label")?.textContent);
const type = async (text: string) => act(async () => {
  const field = input();
  field.focus();
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, text);
  field.dispatchEvent(new Event("input", { bubbles: true }));
});
const key = async (target: EventTarget, init: KeyboardEventInit) => act(async () => {
  target.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
});
const settle = () => act(() => new Promise(resolve => setTimeout(resolve, 150)));

beforeEach(async () => {
  setLanguage("en");
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<HelpMenu />));
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove();
  document.querySelectorAll(".feature-flash, [data-feature]").forEach(e => e.remove());
  setLanguage("en");
  vi.restoreAllMocks();
});

it("reads Cmd-F on a Mac and Ctrl-F elsewhere, and nothing else", () => {
  const chord = (init: Partial<KeyboardEvent>) => ({ key: "f", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...init });
  expect(isSearchChord(chord({ metaKey: true }), true)).toBe(true);
  expect(isSearchChord(chord({ ctrlKey: true }), true)).toBe(false);
  expect(isSearchChord(chord({ ctrlKey: true }), false)).toBe(true);
  expect(isSearchChord(chord({ metaKey: true, shiftKey: true }), true)).toBe(false);
  expect(isSearchChord(chord({}), true)).toBe(false);
});

it("focuses the search on the chord", async () => {
  await key(window, { key: "f", metaKey: IS_MAC, ctrlKey: !IS_MAC });
  expect(document.activeElement).toBe(input());
});

it("lists features as the person types, in the language on screen", async () => {
  const picker = document.createElement("select");
  picker.dataset.feature = "settings:language";
  document.body.append(picker);
  await type("langu");
  expect(labels()[0]).toBe("Language");
  await act(async () => setLanguage("de"));
  await type("sprache");
  expect(labels()[0]).toBe("Sprache");
  await type("zzzz-nothing");
  expect(host.querySelector(".help-search-empty")?.textContent).toContain("zzzz-nothing");
  // The reference is always offered, last.
  expect(labels().at(-1)).toBe("Hilfe öffnen");
});

it("reveals the chosen feature, then flashes it in orange", async () => {
  const target = document.createElement("select");
  target.dataset.feature = "settings:language";
  target.hidden = true;
  document.body.append(target);
  vi.spyOn(target, "getBoundingClientRect").mockImplementation(() =>
    (target.hidden ? new DOMRect(0, 0, 0, 0) : new DOMRect(40, 10, 90, 24)));
  const revealed: string[] = [];
  const off = onReveal("settings:", step => { revealed.push(step); target.hidden = false; });
  try {
    await type("interface language");
    expect(labels()[0]).toBe("Language");
    await key(input(), { key: "Enter" });
    await settle();
    expect(revealed).toEqual(["settings:", "settings:appearance"]);
    const box = document.querySelector<HTMLElement>(".feature-flash");
    expect(box).not.toBeNull();
    expect(box!.style.borderColor).toBe("var(--flash)");
    expect([box!.style.left, box!.style.top, box!.style.width, box!.style.height]).toEqual(["36px", "6px", "98px", "32px"]);
    expect(host.querySelector(".help-menu-popup")).toBeNull();
  } finally {
    off();
  }
});

it("opens a help page, and the reference, from the results", async () => {
  const opened: (string | undefined)[] = [];
  const off = onOpenHelp(topic => opened.push(topic));
  await type("orthographic");
  const page = options().findIndex(o => o.querySelector(".help-search-label")?.textContent === "The world map");
  expect(page).toBeGreaterThanOrEqual(0);
  await act(async () => options()[page]!.click());
  await type("orthographic");
  await act(async () => options().at(-1)!.click());
  await act(async () => host.querySelector<HTMLButtonElement>(".help-button")!.click());
  expect(opened).toEqual(["map", undefined, undefined]);
  off();
});
