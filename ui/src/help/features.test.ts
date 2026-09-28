/**
 * The search's registry agrees with the interface (spec.md 3.6): every
 * control tagged `data-feature` is findable, every registered feature has an
 * element to flash, every reveal step has a handler, and every feature is
 * found in every language by its own translated label.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { FEATURES, searchFeatures } from "./features";
import { TOPICS } from "./topics";
import { LANGUAGES, setLanguage, t } from "../i18n";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

function source(dir: string, out: string[] = [], tsxOnly = false): string[] {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) { if (name !== "generated" && name !== "locales") source(path, out, tsxOnly); }
    else if ((tsxOnly ? /\.tsx$/ : /\.tsx?$/).test(name) && !/\.test\.tsx?$/.test(name)) out.push(readFileSync(path, "utf8"));
  }
  return out;
}
const text = source(ROOT).join("\n");
const jsx = source(ROOT, [], true).join("\n");
/** `data-feature="x"`, and `feature="x"` on the components that pass one through. */
const tagged = new Set([...jsx.matchAll(/(?:data-feature|\bfeature)=(?:"([^"$]+)"|\{"([^"$]+)"\})/g)].map(m => m[1] ?? m[2]!));
/** `data-feature={`stage:${…}`}`: a family of ids built from a prefix. */
const families = [...jsx.matchAll(/data-feature=\{`([^`$]*)\$\{/g)].map(m => m[1]!);
const reveals = [...text.matchAll(/onReveal\(\s*"([^"]+)"/g)].map(m => m[1]!);

describe("feature registry", () => {
  it("has unique ids", () => {
    const ids = FEATURES.map(f => f.id);
    expect(ids.filter((id, i) => ids.indexOf(id) !== i)).toEqual([]);
  });
  it("registers every tagged control", () => {
    const ids = new Set(FEATURES.map(f => f.id));
    expect([...tagged].filter(id => !ids.has(id))).toEqual([]);
  });
  it("has an element for every feature", () => {
    expect(FEATURES.map(f => f.id).filter(id => !tagged.has(id) && !families.some(p => id.startsWith(p)))).toEqual([]);
  });
  it("names help pages that exist", () => {
    const topics = new Set(TOPICS.map(p => p.id));
    expect(FEATURES.filter(f => !f.topic || !topics.has(f.topic)).map(f => `${f.id} → ${f.topic}`)).toEqual([]);
  });
  it("has a handler for every reveal step", () => {
    const steps = FEATURES.flatMap(f => f.reveal ?? []);
    expect(steps.filter(step => !reveals.some(r => r === step || (r.endsWith(":") && step.startsWith(r))))).toEqual([]);
  });
  it("describes every feature", () => {
    expect(FEATURES.filter(f => !f.description || !f.keywords?.length).map(f => f.id)).toEqual([]);
  });

  for (const { id: language } of LANGUAGES) {
    it(`finds every feature by its label in ${language}`, () => {
      setLanguage(language);
      try {
        const missed = FEATURES.filter(f => !searchFeatures(t(f.label)).some(m => m.feature.id === f.id)).map(f => f.id);
        expect(missed).toEqual([]);
      } finally {
        setLanguage("en");
      }
    });
  }

  it("searches in the language on screen, ignoring case and accents", () => {
    setLanguage("en");
    expect(searchFeatures("langu").map(m => m.feature.id)).toContain("settings:language");
    setLanguage("fr");
    expect(searchFeatures("langue").map(m => m.feature.id)).toContain("settings:language");
    // « Délai d'attente », typed without its accent.
    expect(searchFeatures("delai").map(m => m.feature.id)).toContain("settings:timeout");
    setLanguage("de");
    expect(searchFeatures("SPRACHE").map(m => m.feature.id)).toContain("settings:language");
    // „Größenbegrenzung“ and „Öffnen…“, typed without umlauts.
    expect(searchFeatures("offnen").map(m => m.feature.id)).toContain("project:open");
    setLanguage("en");
  });

  it("does not match English words once the interface is in another language", () => {
    setLanguage("fr");
    expect(searchFeatures("Projection").map(m => m.feature.id)).toContain("map:projection");
    expect(searchFeatures("Wave height").map(m => m.feature.id)).not.toContain("settings:wave-unit");
    setLanguage("en");
  });

  it("ranks a label match above a description match", () => {
    setLanguage("en");
    expect(searchFeatures("help")[0]?.feature.id).toMatch(/:help$/);
  });

  it("uses a help page's title, in the language on screen, to find its controls", () => {
    setLanguage("de");
    // Only the page title, „Das Projektfenster“, says this.
    expect(searchFeatures("projektfenster").map(m => m.feature.id)).toContain("shell:statusbar");
    setLanguage("en");
  });
});
