// @vitest-environment happy-dom
/**
 * The shell end to end, against a mocked backend: the start screen, the
 * project window's layout, the save guard on every way a project is put
 * down (quitting included), and the two acceptance criteria of plan.md M2 —
 * a language switch relabels everything with no reload, and every control is
 * found by the search in each language and flashed after it is revealed.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AppSettings } from "./generated/AppSettings";
import type { ProjectSummary } from "./generated/ProjectSummary";

const invoke = vi.hoisted(() => vi.fn());
const events = vi.hoisted(() => new Map<string, () => void>());
const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: () => void) => {
    events.set(name, handler);
    return () => events.delete(name);
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

const { default: App } = await import("./App");
const { CATALOGUES, LANGUAGES, language, setLanguage, t } = await import("./i18n");
const { FEATURES } = await import("./help/features");
const { QUIT_REQUESTED } = await import("./ipc");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// happy-dom lays nothing out: every connected element gets a box, so
// "on screen" means "rendered", which is exactly what a reveal step changes.
HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
  return this.isConnected ? new DOMRect(10, 10, 80, 20) : new DOMRect(0, 0, 0, 0);
};
HTMLElement.prototype.scrollIntoView = () => undefined;

function summary(dirty: boolean, path: string | null = null): ProjectSummary {
  return {
    id: 1, name: "Fastnet", path, dirty, revision: 1, boat_name: "", boat_notes: "", sources: [],
    can_undo: false, can_redo: false, undo_label: null, redo_label: null,
  };
}

let settings: AppSettings;
let project: ProjectSummary | null;
let calls: [string, unknown][];

function backend() {
  invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    calls.push([command, args]);
    switch (command) {
      case "app_settings": return settings;
      case "app_info": return { name: "PolarEffects", version: "0.1.0" };
      case "project_summary": return project;
      case "recent_projects": return [
        { path: "/boats/Fastnet.wpsproj", name: "Fastnet", exists: true },
        { path: "/gone/Old.wpsproj", name: "Old", exists: false },
      ];
      case "forget_recent_project": return [{ path: "/boats/Fastnet.wpsproj", name: "Fastnet", exists: true }];
      case "recovered_projects": return [{ id: 9, name: "Lost", original_path: null, saved_unix_s: 0 }];
      case "new_project": project = summary(true); return project;
      case "open_project": project = summary(false, args?.path as string); return project;
      case "close_project":
        if (project?.dirty && !args?.discardUnsaved) throw { kind: "unsaved-changes", message: "unsaved" };
        project = null; return null;
      case "save_project_as": project = summary(false, args?.path as string); return project;
      case "save_project": project = summary(false, project?.path ?? null); return project;
      case "set_language": settings = { ...settings, language: args?.language as string }; return settings;
      case "set_theme": settings = { ...settings, theme: args?.theme as string }; return settings;
      case "chunk_cache_status": return { path: "/cache/chunks", bytes: 0 };
      case "quit_app": return null;
      default: return null;
    }
  });
}

let host: HTMLDivElement;
let root: Root;
const settle = (ms = 30) => act(() => new Promise((resolve) => setTimeout(resolve, ms)));
async function mount() {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<App />));
  await settle();
}
const q = <T extends Element = HTMLElement>(selector: string) => document.querySelector<T>(selector);
const feature = (id: string) => q(`[data-feature="${id}"]`);
const click = async (element: Element | null) => {
  expect(element).not.toBeNull();
  await act(async () => (element as HTMLElement).click());
  await settle();
};
const buttonNamed = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === text) ?? null;
const type = async (input: HTMLInputElement, text: string) => act(async () => {
  input.focus();
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, text);
  input.dispatchEvent(new Event("input", { bubbles: true }));
});

beforeEach(() => {
  localStorage.clear();
  setLanguage("en");
  calls = [];
  project = null;
  settings = {
    recent_projects: [], autosave: "recovery", language: "en", theme: "harbour",
    units: { speed: "kn", wave_height: "m", distance: "nm" },
    chunk_cache: { location: "", size_limit_gb: 20 }, network: { concurrency: 8, timeout_s: 60 },
    projection: "equirectangular",
  };
  dialog.open.mockReset().mockResolvedValue(null);
  dialog.save.mockReset().mockResolvedValue(null);
  backend();
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  document.body.innerHTML = "";
  events.clear();
  setLanguage("en");
});

describe("the start screen", () => {
  it("offers new, open, recent (missing ones greyed) and recovered work", async () => {
    await mount();
    expect(q("h1")?.textContent).toBe("PolarEffects");
    expect(feature("start:new")).not.toBeNull();
    expect(feature("start:browse")).not.toBeNull();
    expect(feature("start:language")).not.toBeNull();
    expect(feature("start:recover")?.textContent).toContain("Lost");
    const missing = q(".recent-item.missing");
    expect(missing?.textContent).toContain("Not found");
    // A missing file is removed only after a confirmation.
    await click(missing);
    expect(q("[role=dialog]")?.textContent).toContain("/gone/Old.wpsproj");
    await click(buttonNamed("Remove"));
    expect(calls).toContainEqual(["forget_recent_project", { path: "/gone/Old.wpsproj" }]);
    expect(q(".recent-item.missing")).toBeNull();
  });

  it("creates a project and opens the project window", async () => {
    await mount();
    await click(feature("new:create"));
    expect(calls.find(([c]) => c === "new_project")?.[1]).toEqual({
      name: "Untitled polar", boat: { name: "", notes: "" }, discardUnsaved: false,
    });
    for (const id of ["shell:project-menu", "shell:rename", "stage:map", "stage:3d", "stage:compare", "shell:search",
      "shell:help", "shell:settings", "nav:orc", "nav:polar-files", "nav:tracks", "panel:sources", "panel:plot",
      "shell:statusbar", "dock:left", "dock:right", "map:projection"]) {
      expect(feature(id), id).not.toBeNull();
    }
    // The three navigation sections, in the spec's order.
    expect([...document.querySelectorAll(".left-nav h2")].map((h) => h.textContent)).toEqual(["ORC polars", "Polar files", "Tracks"]);
    expect(q(".project-name")?.textContent).toContain("•");
  });
});

describe("the project window", () => {
  it("folds the navigation and its sections, and remembers it for the person", async () => {
    project = summary(false, "/p.wpsproj");
    await mount();
    await click(feature("nav:tracks"));
    expect(feature("nav:tracks")?.getAttribute("aria-expanded")).toBe("false");
    await click(feature("dock:left"));
    expect(feature("nav:orc")).toBeNull();
    expect(JSON.parse(localStorage.getItem("pe.panels")!)).toMatchObject({ left: false, tracks: false });
    await click(feature("dock:left"));
    expect(feature("nav:tracks")?.getAttribute("aria-expanded")).toBe("false");
  });

  it("switches the stage", async () => {
    project = summary(false, "/p.wpsproj");
    await mount();
    await click(feature("stage:3d"));
    expect(feature("map:projection")).toBeNull();
    expect(q(".stage-placeholder h2")?.textContent).toBe("3D polar");
    await click(feature("stage:compare"));
    expect(q(".stage-placeholder h2")?.textContent).toBe("Compare");
    await click(feature("stage:map"));
    expect(feature("map:projection")).not.toBeNull();
  });
});

describe("the save guard", () => {
  it("asks before closing unsaved work, and Cancel keeps it", async () => {
    project = summary(true);
    await mount();
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    expect(q("[role=dialog]")?.textContent).toContain("“Fastnet” has changes that have not been saved.");
    await click(buttonNamed("Cancel"));
    expect(calls.some(([c]) => c === "close_project")).toBe(false);
    expect(feature("shell:rename")).not.toBeNull();
  });

  it("Escape is Cancel", async () => {
    project = summary(true);
    await mount();
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    await act(async () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
    await settle();
    expect(q("[role=dialog]")).toBeNull();
    expect(calls.some(([c]) => c === "close_project")).toBe(false);
  });

  it("Don't save closes, telling Rust it was asked", async () => {
    project = summary(true);
    await mount();
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    await click(buttonNamed("Don’t save"));
    expect(calls).toContainEqual(["close_project", { discardUnsaved: true }]);
    expect(feature("start:new")).not.toBeNull();
  });

  it("Save on a never-saved project asks where; cancelling that cancels the close", async () => {
    project = summary(true);
    await mount();
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    await click(buttonNamed("Save"));
    expect(dialog.save).toHaveBeenCalledOnce();
    expect(calls.some(([c]) => c === "close_project")).toBe(false);
    expect(feature("shell:rename")).not.toBeNull();
  });
});

describe("quitting", () => {
  it("runs the guard when Rust reports a quit, and quits only when allowed", async () => {
    project = summary(true);
    await mount();
    const quitRequested = events.get(QUIT_REQUESTED)!;
    expect(quitRequested).toBeDefined();

    await act(async () => quitRequested());
    await settle();
    await click(buttonNamed("Cancel"));
    expect(calls.some(([c]) => c === "quit_app")).toBe(false);

    // Save, then the Save As dialog is cancelled: still no quit.
    await act(async () => quitRequested());
    await settle();
    await click(buttonNamed("Save"));
    expect(calls.some(([c]) => c === "quit_app")).toBe(false);

    await act(async () => quitRequested());
    await settle();
    await click(buttonNamed("Don’t save"));
    expect(calls).toContainEqual(["quit_app", { discardUnsaved: true }]);
  });

  it("saves first when asked to, then quits with nothing to discard", async () => {
    project = summary(true);
    dialog.save.mockResolvedValue("/boats/Fastnet.wpsproj");
    await mount();
    await act(async () => events.get(QUIT_REQUESTED)!());
    await settle();
    await click(buttonNamed("Save"));
    const order = calls.map(([c]) => c).filter((c) => c === "save_project_as" || c === "quit_app");
    expect(order).toEqual(["save_project_as", "quit_app"]);
    expect(calls).toContainEqual(["quit_app", { discardUnsaved: false }]);
  });
});

/** Every text node, tooltip and label on the page, in document order. */
function visibleStrings(): string[] {
  const out: string[] = [];
  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent?.trim();
      if (text) out.push(text);
      return;
    }
    if (node instanceof HTMLElement) {
      for (const attribute of ["title", "aria-label", "placeholder"]) {
        const value = node.getAttribute(attribute);
        if (value) out.push(value);
      }
    }
    node.childNodes.forEach(walk);
  };
  walk(document.body);
  return out;
}

describe("switching language (plan.md M2 acceptance)", () => {
  for (const target of ["fr", "de"] as const) {
    it(`relabels every visible string and tooltip in ${target}, without a reload`, async () => {
      project = summary(true, "/boats/Fastnet.wpsproj");
      await mount();
      await click(feature("shell:project-menu"));
      await click(feature("project:open-recent"));
      await click(feature("shell:settings"));
      const before = visibleStrings();
      const page = document.body.firstElementChild;

      const picker = feature("settings:language") as HTMLSelectElement;
      await act(async () => {
        picker.value = target;
        picker.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await settle();
      const after = visibleStrings();

      expect(language()).toBe(target);
      expect(document.body.firstElementChild, "the page was not reloaded").toBe(page);
      // Rust relabels the native menu as it saves the language.
      expect(calls).toContainEqual(["set_language", { language: target }]);
      expect(after.length).toBe(before.length);
      const catalogue = CATALOGUES[target];
      // Data, not interface: the project's name and path, the version, the
      // languages' own names, and symbols.
      const data = new Set(["Fastnet", "/boats/Fastnet.wpsproj", "v0.1.0", "PolarEffects", "English", "Français",
        "Deutsch", "/cache/chunks", "…"]);
      const untranslated = before.filter((text, index) =>
        text === after[index] && !data.has(text) && catalogue[text] !== text && /\p{L}{2}/u.test(text)
        && !/^(Cmd|Ctrl)\+/.test(text));
      expect(untranslated).toEqual([]);
    });
  }
});

describe("finding every control (plan.md M2 acceptance)", () => {
  const startFeatures = FEATURES.filter((f) => /^(start|new):/.test(f.id));
  const windowFeatures = FEATURES.filter((f) => !/^(start|new):/.test(f.id));

  /** Everything folded away that can be: the search has to bring each control back. */
  function foldEverything() {
    localStorage.setItem("pe.panels", JSON.stringify({
      left: false, right: false, orc: false, polarFiles: false, tracks: false, sources: false, plot: false,
    }));
  }

  async function findAndFlash(id: string) {
    const entry = FEATURES.find((f) => f.id === id)!;
    const search = feature("shell:search") as HTMLInputElement;
    await type(search, t(entry.label));
    await settle();
    const options = [...document.querySelectorAll<HTMLLIElement>(".help-menu-popup [role=option]")];
    const match = options.find((option) =>
      option.querySelector(".help-search-label")?.textContent === t(entry.label)
      && option.querySelector(".help-search-detail")?.textContent === t(entry.description!));
    expect(match, `${id} is offered for “${t(entry.label)}”`).toBeDefined();
    document.querySelectorAll(".feature-flash").forEach((box) => box.remove());
    await act(async () => match!.click());
    await settle(entry.reveal?.length ? 250 : 80);
    const flashed = q(".feature-flash");
    expect(flashed, `${id} is flashed`).not.toBeNull();
    expect(flashed!.style.borderColor).toBe("var(--flash)");
    expect(feature(id), `${id} is on screen`).not.toBeNull();
  }

  for (const { id: lang } of LANGUAGES) {
    it(`finds and flashes every project-window control in ${lang}`, async () => {
      settings = { ...settings, language: lang };
      setLanguage(lang);
      for (const entry of windowFeatures) {
        foldEverything();
        project = summary(false, "/p.wpsproj");
        await mount();
        // The map stage is hidden behind the 3D stage, to be revealed.
        await click(feature("stage:3d"));
        if (entry.reveal?.length) {
          expect(feature(entry.id), `${entry.id} starts hidden`).toBeNull();
        }
        await findAndFlash(entry.id);
        await act(async () => root.unmount());
        host.remove();
        document.body.innerHTML = "";
      }
      await mount();
    }, 60_000);

    it(`finds every start-screen control in ${lang}`, async () => {
      settings = { ...settings, language: lang };
      setLanguage(lang);
      await mount();
      const { searchFeatures } = await import("./help/features");
      const { locateFeature } = await import("./help/highlight");
      for (const entry of startFeatures) {
        expect(searchFeatures(t(entry.label)).map((m) => m.feature.id), entry.id).toContain(entry.id);
        document.querySelectorAll(".feature-flash").forEach((box) => box.remove());
        expect(await locateFeature(entry), entry.id).toBe(true);
        expect(q(".feature-flash"), entry.id).not.toBeNull();
      }
    });
  }
});
