// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const events = vi.hoisted(() => new Map<string, () => void>());
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: () => void) => {
    events.set(name, handler);
    return () => events.delete(name);
  },
}));

const { default: Help } = await import("./Help");
const { openHelp } = await import("./open");
const { setLanguage } = await import("../i18n");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;
const dialog = () => document.querySelector(".help-dialog");
const heading = () => document.querySelector(".help-body article h3")?.textContent;
const key = async (init: KeyboardEventInit) => act(async () => {
  window.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
});

beforeEach(async () => {
  setLanguage("en");
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<Help><button id="behind">behind</button></Help>));
  await act(async () => new Promise((resolve) => setTimeout(resolve, 0)));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); events.clear(); setLanguage("en"); });

it("opens on F1 at the workspace page, and closes on Escape", async () => {
  expect(dialog()).toBeNull();
  await key({ key: "F1" });
  expect(dialog()).not.toBeNull();
  expect(heading()).toBe("The project window");
  // What is behind the window cannot be reached while it is up.
  expect(document.querySelector("#behind")?.closest("[inert]")).not.toBeNull();
  await key({ key: "Escape" });
  expect(dialog()).toBeNull();
});

it("opens from the native menu's Help item", async () => {
  await act(async () => events.get("help://open")!());
  expect(dialog()).not.toBeNull();
});

it("opens at the page the search asked for, and moves between pages", async () => {
  await act(async () => openHelp("settings"));
  expect(heading()).toBe("Settings");
  const topic = [...document.querySelectorAll<HTMLButtonElement>('[data-feature="help:topic"]')]
    .find((button) => button.textContent === "The world map")!;
  await act(async () => topic.click());
  expect(heading()).toBe("The world map");
  expect(topic.getAttribute("aria-current")).toBe("page");
  const related = document.querySelector<HTMLButtonElement>('[data-feature="help:related"]')!;
  const target = related.textContent;
  await act(async () => related.click());
  expect(heading()).toBe(target);
  await act(async () => document.querySelector<HTMLButtonElement>('[data-feature="help:close"]')!.click());
  expect(dialog()).toBeNull();
});

it("filters its pages as you type, and follows the language", async () => {
  await act(async () => openHelp());
  const search = document.querySelector<HTMLInputElement>('[data-feature="help:search"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(search, "orthographic");
    search.dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect([...document.querySelectorAll('[data-feature="help:topic"]')].map((b) => b.textContent)).toEqual(["The world map"]);
  await act(async () => setLanguage("fr"));
  expect(document.querySelector(".help-dialog h2")?.textContent).toBe("Aide de PolarExplorer");
});
