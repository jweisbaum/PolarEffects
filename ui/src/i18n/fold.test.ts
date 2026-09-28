// @vitest-environment happy-dom
import { afterEach, expect, it } from "vitest";
import { fold, setLanguage } from "./index";

afterEach(() => setLanguage("en"));

it("ignores case and the accents a person leaves off", () => {
  expect(fold("Échelle Größe")).toBe("echelle große");
  expect(fold("Abattée")).toBe("abattee");
  expect(fold("Übersicht")).toBe("ubersicht");
  expect(fold("ＡＢＣ１２")).toBe("abc12");
});

it("marks the page's language", () => {
  setLanguage("de");
  expect(document.documentElement.lang).toBe("de");
  setLanguage("fr");
  expect(document.documentElement.lang).toBe("fr");
});
