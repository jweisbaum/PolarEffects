// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";

import { activeCount, hasWord, loadFieldsOpen, NO_FIELDS, saveFieldsOpen, toFilters } from "./orcFields";

describe("Search by field (spec.md 5.2)", () => {
  beforeEach(() => localStorage.clear());

  it("sends trimmed field queries and whole years", () => {
    expect(toFilters({ ...NO_FIELDS, name: "  Fröken ", year_from: "1990", year_to: "abc", country: "GBR" }))
      .toEqual({
        year_min: 1990, year_max: null, country: "GBR",
        name: "Fröken", sail_no: "", model: "", builder: "", designer: "", certificate_year: "",
      });
  });

  it("counts year built once and ignores blank boxes", () => {
    expect(activeCount(NO_FIELDS)).toBe(0);
    expect(activeCount({ ...NO_FIELDS, model: "   " })).toBe(0);
    expect(activeCount({ ...NO_FIELDS, year_from: "1990", year_to: "2000" })).toBe(1);
    expect(activeCount({
      name: "a", sail_no: "b", country: "GBR", model: "c", builder: "d", designer: "e",
      year_from: "", year_to: "2000", certificate_year: "202",
    })).toBe(8);
  });

  it("treats a box of only punctuation as empty, as Rust does", () => {
    expect(hasWord(" / - ")).toBe(false);
    expect(hasWord("J/109")).toBe(true);
    expect(hasWord("Ñ")).toBe(true);
    expect(hasWord("٣")).toBe(true);
    const punctuation = { ...NO_FIELDS, name: "'", sail_no: " / ", model: "-", builder: ".", designer: "…", certificate_year: "/" };
    expect(activeCount(punctuation)).toBe(0);
    expect(toFilters(punctuation)).toMatchObject({ name: "", sail_no: "", model: "", builder: "", designer: "", certificate_year: "" });
  });

  it("is folded until opened, then remembered", () => {
    expect(loadFieldsOpen()).toBe(false);
    saveFieldsOpen(true);
    expect(loadFieldsOpen()).toBe(true);
    saveFieldsOpen(false);
    expect(loadFieldsOpen()).toBe(false);
  });
});
