import { afterEach, expect, it } from "vitest";

import { describeError, reportFailure } from "./errors";
import { currentHint, reportError } from "./hint";
import { setLanguage } from "./i18n";
import { IpcError } from "./ipc";

afterEach(() => { setLanguage("en"); reportError(null); });

const kinds = ["io", "core", "schema-too-new", "no-project", "never-saved", "unsaved-changes", "bad-option", "doing", "internal"];

it("translates every Rust error kind and keeps the English only as detail", () => {
  for (const language of ["fr", "de"]) {
    setLanguage(language);
    for (const kind of kinds) {
      const english = "Could not open the project at /x.wpsproj: No such file or directory";
      const shown = describeError(new IpcError({ kind, message: english }));
      expect(shown.text, `${language} ${kind}`).not.toContain("Could not");
      expect(shown.text, `${language} ${kind}`).not.toMatch(/^Error/);
      expect(shown.detail).toBe(`${kind}: ${english}`);
    }
  }
});

it("gives each kind its own line", () => {
  const lines = kinds.map((kind) => describeError(new IpcError({ kind, message: "m" })).text);
  expect(new Set(lines).size).toBe(kinds.length);
});

it("falls back to a translated generic line for an unknown kind or a non-Rust failure", () => {
  setLanguage("de");
  expect(describeError(new IpcError({ kind: "unknown", message: "panicked" }))).toEqual({
    text: "Etwas ist schiefgegangen.",
    detail: "unknown: panicked",
  });
  expect(describeError(new Error("dialog plugin missing")).detail).toBe("dialog plugin missing");
  expect(describeError("boom").text).toBe("Etwas ist schiefgegangen.");
});

it("reports on the status line with the English as the tooltip", () => {
  setLanguage("fr");
  reportFailure(new IpcError({ kind: "io", message: "disk full" }));
  expect(currentHint().error).toBe("Un fichier n’a pas pu être lu ou écrit.");
  expect(currentHint().errorDetail).toBe("io: disk full");
});
