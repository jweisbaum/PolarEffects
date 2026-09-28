// @vitest-environment happy-dom
import { afterEach, expect, it } from "vitest";
import { applyTheme, DEFAULT_THEME, mapColour, rgba, THEME_NAMES, THEMES, themeName, themeOf } from "./themes";
import { setLanguage } from "../i18n";

afterEach(() => applyTheme(DEFAULT_THEME));

function contrast(a: string, b: string): number {
  const luminance = (hex: string) => {
    const channels = rgba(hex).slice(0, 3).map(c => c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    return channels[0]! * 0.2126 + channels[1]! * 0.7152 + channels[2]! * 0.0722;
  };
  const values = [luminance(a), luminance(b)].sort((x, y) => x - y);
  return (values[1]! + 0.05) / (values[0]! + 0.05);
}

it.each(THEMES)("$name keeps text and selected controls readable", theme => {
  const r = theme.roles;
  for (const [ink, background] of [[r.text, r.bg], [r.text, r.surface], [r.text, r.hover],
    [r.muted, r.surface], [r["selected-ink"], r["selected-bg"]], [r["selected-ink"], r["selected-hover"]]]) {
    expect(contrast(ink!, background!), `${theme.id}: ${ink} on ${background}`).toBeGreaterThanOrEqual(4.5);
  }
});

it("defaults to Harbour with the palette spec.md 3.1 names", () => {
  const harbour = themeOf(undefined);
  expect(harbour.id).toBe("harbour");
  expect(harbour.roles).toMatchObject({
    bg: "#2e3f55", surface: "#253447", panel: "rgba(37, 52, 71, 0.96)", inset: "#1f2c3c", raised: "#2e3f55",
    hover: "#36597a", active: "#36597a", border: "#5b7fa3", "border-subtle": "#3a506a", text: "#d6e6f5",
    muted: "#b3c9de", accent: "#8fb8de", highlight: "#e8f1fa", warning: "#edbd8d", error: "#e9a7a3",
    "selected-bg": "#8fb8de", "selected-ink": "#1f2c3c",
  });
  expect(THEMES.map(theme => theme.id)).toEqual(["harbour", "midnight", "ocean", "plum", "ember", "paper"]);
});

it("gives every theme the same roles", () => {
  const roles = Object.keys(THEMES[0]!.roles).sort();
  const map = Object.keys(THEMES[0]!.map).sort();
  for (const theme of THEMES) {
    expect(Object.keys(theme.roles).sort(), theme.id).toEqual(roles);
    expect(Object.keys(theme.map).sort(), theme.id).toEqual(map);
  }
});

it("switches document and canvas colours together, including from light back to dark", () => {
  for (const id of ["paper", "midnight", "harbour"]) {
    applyTheme(id);
    const theme = themeOf(id);
    expect(document.documentElement.dataset.theme).toBe(id);
    expect(document.documentElement.style.colorScheme).toBe(theme.scheme);
    expect(document.documentElement.style.getPropertyValue("--text")).toBe(theme.roles.text);
    expect(mapColour("land")).toBe(theme.map.land);
  }
  applyTheme("unknown");
  expect(document.documentElement.dataset.theme).toBe(DEFAULT_THEME);
});

it("marks every bundled theme's name for translation", () => {
  expect(THEMES.map(theme => theme.name).filter(name => !THEME_NAMES.includes(name))).toEqual([]);
  setLanguage("fr");
  try {
    expect(themeName(themeOf("harbour"))).toBe("Port");
  } finally {
    setLanguage("en");
  }
});
