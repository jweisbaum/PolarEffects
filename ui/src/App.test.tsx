// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import App from "./App";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

async function renderApp(): Promise<HTMLElement> {
  const container = document.createElement("div");
  document.body.append(container);
  await act(async () => {
    createRoot(container).render(<App />);
  });
  return container;
}

describe("App", () => {
  afterEach(() => {
    document.body.innerHTML = "";
    invoke.mockReset();
  });

  it("shows the name and version the backend reports", async () => {
    invoke.mockResolvedValue({ name: "PolarEffects", version: "9.8.7" });
    const root = await renderApp();
    expect(root.querySelector("h1")?.textContent).toBe("PolarEffects");
    expect(root.querySelector(".version")?.textContent).toBe("9.8.7");
  });

  it("shows the backend's message when the command fails", async () => {
    invoke.mockRejectedValue({ kind: "internal", message: "boom" });
    const root = await renderApp();
    expect(root.querySelector("[role=alert]")?.textContent).toBe("boom");
  });
});
