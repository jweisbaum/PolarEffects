// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const setLanguageCall = vi.fn();
vi.mock("../ipc", () => ({ api: { setLanguage: (language: string) => setLanguageCall(language) } }));

const { default: LanguagePicker } = await import("./LanguagePicker");
const { language, setLanguage, t, useT } = await import("./index");
const { currentHint, shown } = await import("../hint");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;
const saved = vi.fn();

function Label() {
  const t = useT();
  return <span id="label">{t("Help")}</span>;
}
const choose = async (value: string) => act(async () => {
  const select = host.querySelector("select")!;
  select.value = value;
  select.dispatchEvent(new Event("change", { bubbles: true }));
});

beforeEach(async () => {
  setLanguage("en");
  setLanguageCall.mockReset();
  saved.mockReset();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<><LanguagePicker feature="start:language" onSettings={saved} /><Label /></>));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); setLanguage("en"); });

it("offers exactly English, French and German, each named in itself", () => {
  const options = [...host.querySelectorAll("option")].map(o => [o.value, o.textContent]);
  expect(options).toEqual([["en", "English"], ["fr", "Français"], ["de", "Deutsch"]]);
});

it("switches the interface at once, saves, and remembers it for the next launch", async () => {
  setLanguageCall.mockResolvedValue({ language: "fr" });
  await choose("fr");
  expect(language()).toBe("fr");
  expect(host.querySelector("#label")?.textContent).toBe("Aide");
  expect(document.documentElement.lang).toBe("fr");
  expect(setLanguageCall).toHaveBeenCalledWith("fr");
  expect(saved).toHaveBeenCalledWith({ language: "fr" });
  expect(localStorage.getItem("pe.language")).toBe("fr");
});

it("puts the previous language back when the save fails", async () => {
  setLanguageCall.mockRejectedValue(new Error("disk full"));
  await choose("de");
  expect(language()).toBe("en");
  expect(host.querySelector("#label")?.textContent).toBe("Help");
  expect(shown(currentHint())?.text).toBe("The language could not be saved.");
  expect(currentHint().errorDetail).toContain("disk full");
});

it("falls back to English for a string no catalogue holds, and fills placeholders", () => {
  setLanguage("de");
  expect(t("No such string {x}", { x: 3 })).toBe("No such string 3");
  expect(t("Hilfe")).toBe("Hilfe");
  setLanguage("klingon");
  expect(language()).toBe("de");
});
