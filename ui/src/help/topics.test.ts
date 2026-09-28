import { expect, it } from "vitest";
import { FEATURES } from "./features";
import { TOPICS, searchTopics, topicsFor } from "./topics";
import { LANGUAGES } from "../i18n";

it("has unique pages whose cross-references exist", () => {
  const ids = new Set(TOPICS.map(topic => topic.id));
  expect(ids.size).toBe(TOPICS.length);
  for (const topic of TOPICS) {
    for (const id of topic.related ?? []) expect(ids.has(id), `${topic.id} links to ${id}`).toBe(true);
  }
});

it("has a page for every area of the interface", () => {
  const pages = new Set(TOPICS.map(topic => topic.id));
  for (const id of ["workspace", "projects", "orc", "polar-files", "tracks", "sources", "map", "polar-3d", "compare", "settings", "search"]) {
    expect(pages.has(id), id).toBe(true);
  }
  // And every page is some feature's page, so none is unreachable from the search.
  const used = new Set(FEATURES.map(f => f.topic));
  expect(TOPICS.filter(topic => !used.has(topic.id)).map(topic => topic.id)).toEqual([]);
});

it("translates the whole reference, page for page", () => {
  const shape = (topics: typeof TOPICS) => topics.map(topic => ({
    id: topic.id, related: topic.related, parameters: topic.parameters?.length, paragraphs: topic.paragraphs.length,
  }));
  for (const { id: language } of LANGUAGES.filter(l => l.id !== "en")) {
    const translated = topicsFor(language);
    expect(translated, language).not.toBe(TOPICS);
    expect(shape(translated), language).toEqual(shape(TOPICS));
    // Not English left in place: every title and paragraph differs.
    translated.forEach((topic, index) => {
      const english = TOPICS[index]!;
      if (topic.title !== english.title) return;
      // Titles that are the same word in both ("Tracks", "Sources") are fine;
      // their paragraphs still have to be translated.
      expect(topic.paragraphs[0], `${language} ${topic.id}`).not.toBe(english.paragraphs[0]);
    });
  }
});

it("searches the reference in the language on screen, without accents", () => {
  expect(searchTopics("projection", topicsFor("en")).map(t => t.id)).toContain("map");
  expect(searchTopics("equirectangulaire", topicsFor("fr")).map(t => t.id)).toContain("map");
  expect(searchTopics("recuperation", topicsFor("fr")).map(t => t.id)).toContain("projects");
  expect(searchTopics("plattkarte", topicsFor("de")).map(t => t.id)).toContain("map");
  expect(searchTopics("wiederherstellen", topicsFor("de")).map(t => t.id)).toContain("projects");
  expect(searchTopics("no-such-thing")).toEqual([]);
});
