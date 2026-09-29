import { describe, expect, it } from "vitest";

import { setLanguage } from "./i18n";
import { currentHint, later, reportError, setHint, shown } from "./hint";

describe("the hint store", () => {
  it("shows the hint, and an error over it until the hint changes", () => {
    setHint("Drag to pan the map.");
    expect(shown(currentHint())).toEqual({ text: "Drag to pan the map.", kind: "hint", detail: null });

    reportError("the file is locked", "io");
    expect(shown(currentHint())).toEqual({ text: "the file is locked", kind: "error", detail: "io" });

    // The same hint again says nothing new: the error stands.
    setHint("Drag to pan the map.");
    expect(shown(currentHint())?.kind).toBe("error");

    // A different hint is newer than the error.
    setHint("Scroll to zoom.");
    expect(shown(currentHint())).toEqual({ text: "Scroll to zoom.", kind: "hint", detail: null });

    setHint(null);
    expect(shown(currentHint())).toBeNull();
  });

  it("keeps the error's kind out of the line and in the detail", () => {
    setHint(null);
    reportError("Could not save the project to /tmp/x.wpsproj: permission denied", "doing");
    const line = shown(currentHint());
    expect(line?.text).not.toContain("doing");
    expect(line?.detail).toBe("doing");
    reportError(null);
    expect(currentHint().errorDetail).toBeNull();
  });

  it("translates a line when it is shown, so a language switch relabels it (spec.md 3.5)", () => {
    try {
      setLanguage("fr");
      setHint(later("Imported {count} tracks. Fetch their weather from the track list when you want it.", { count: 3 }));
      expect(shown(currentHint())?.text).toBe("3 traces importées. Récupérez leur météo depuis la liste des traces quand vous le souhaitez.");
      setLanguage("de");
      expect(shown(currentHint())?.text).toBe("3 Tracks importiert. Rufen Sie ihr Wetter bei Bedarf in der Trackliste ab.");
      // A parameter that is itself text follows too.
      reportError(later("Undone: {action}", { action: later("Rename source") }));
      expect(shown(currentHint())?.text).toBe("Widerrufen: Quelle umbenennen");
      setLanguage("fr");
      expect(shown(currentHint())?.text).toBe("Annulé\u202f: Renommer une source");
      // An equal deferred hint is the same hint: the error stands.
      setHint(later("Imported {count} tracks. Fetch their weather from the track list when you want it.", { count: 3 }));
      expect(shown(currentHint())?.kind).toBe("error");
    } finally {
      reportError(null);
      setHint(null);
      setLanguage("en");
    }
  });
});
