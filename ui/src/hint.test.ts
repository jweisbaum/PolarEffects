import { describe, expect, it } from "vitest";

import { currentHint, reportError, setHint, shown } from "./hint";

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
    expect(currentHint().errorKind).toBeNull();
  });
});
