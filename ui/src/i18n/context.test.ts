import { expect, it } from "vitest";

import { english, setLanguage, t } from "./index";

it("keeps a key's context out of the English and apart in translation", () => {
  expect(english("Compare@@verb")).toBe("Compare");
  expect(english("Max |Δ| {max}")).toBe("Max |Δ| {max}");
  try {
    expect(t("Compare@@verb")).toBe("Compare");
    setLanguage("de");
    expect(t("Compare")).toBe("Vergleich");
    expect(t("Compare@@verb")).toBe("Vergleichen");
  } finally {
    setLanguage("en");
  }
});
