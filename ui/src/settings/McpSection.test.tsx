// @vitest-environment happy-dom
/**
 * The MCP section shows the URL and a ready client configuration only while
 * the service is on, and every change goes through the command (spec.md 3.7).
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { McpStatus } from "../generated/McpStatus";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const held = vi.hoisted(() => {
  const off: McpStatus = { enabled: false, port: 47392, token: "", bound_port: null, bind_error: null, sessions: 0, last_tool: null, clients: ["claude_code", "codex", "claude_desktop"] };
  const on: McpStatus = { ...off, enabled: true, token: "tok_abc", bound_port: 47392 };
  // What the command answers: where it wrote the client's skill, if it has one.
  const registered = (client: string): { skill: string | null } => ({
    skill:
      client === "claude_code"
        ? "/home/someone/.claude/skills/polarexplorer/SKILL.md"
        : client === "claude_desktop"
          ? "/home/someone/settings/PolarExplorer-skill.zip"
          : null,
  });
  return {
    status: off,
    on,
    registered,
    mcpStatus: vi.fn(async () => held.status),
    setMcp: vi.fn(async (enabled: boolean, port: number) => {
      held.status = enabled ? { ...held.on, port } : { ...held.on, enabled: false, token: "", bound_port: null, port };
      return held.status;
    }),
    rotateMcpToken: vi.fn(async () => {
      held.status = { ...held.status, token: "tok_new" };
      return held.status;
    }),
    registerMcpClient: vi.fn(async (client: string): Promise<{ skill: string | null }> => held.registered(client)),
  };
});

vi.mock("../ipc", () => ({
  api: {
    mcpStatus: held.mcpStatus,
    setMcp: held.setMcp,
    rotateMcpToken: held.rotateMcpToken,
    registerMcpClient: held.registerMcpClient,
  },
  IpcError: class extends Error {},
}));

import McpSection from "./McpSection";

let root: Root;
let host: HTMLDivElement;

beforeEach(() => {
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  held.mcpStatus.mockClear();
  held.setMcp.mockClear();
  held.rotateMcpToken.mockClear();
  held.registerMcpClient.mockReset();
  held.registerMcpClient.mockImplementation(async (client: string) => held.registered(client));
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  held.status = { enabled: false, port: 47392, token: "", bound_port: null, bind_error: null, sessions: 0, last_tool: null, clients: ["claude_code", "codex", "claude_desktop"] };
});

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe("McpSection", () => {
  it("shows no URL while off and one with the token once on", async () => {
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    expect(host.textContent).not.toContain("http://127.0.0.1");
    const toggle = host.querySelector<HTMLInputElement>('[data-feature="settings:mcp-enable"]');
    await act(async () => {
      toggle?.click();
    });
    await flush();
    expect(held.setMcp).toHaveBeenCalledWith(true, 47392);
    expect(host.textContent).toContain("http://127.0.0.1:47392/mcp");
    const snippet = host.querySelector("pre")?.textContent ?? "";
    expect(snippet).toContain("Bearer tok_abc");
    expect(snippet).toContain("claude mcp add");
  });

  it("fetches its status once, regardless of a new onError identity on re-render", async () => {
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    expect(held.mcpStatus).toHaveBeenCalledTimes(1);
    // A parent re-render (SettingsDialog holds several unrelated useStates)
    // passes a *new* `onError` function each time; that alone must not
    // re-trigger the fetch.
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    expect(held.mcpStatus).toHaveBeenCalledTimes(1);
  });

  it("rotates the token through the command", async () => {
    held.status = held.on;
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    const rotate = Array.from(host.querySelectorAll("button")).find((b) => b.textContent?.includes("Rotate"));
    await act(async () => {
      rotate?.click();
    });
    await flush();
    expect(held.rotateMcpToken).toHaveBeenCalled();
    expect(host.textContent).toContain("tok_new");
  });

  const button = (words: string) =>
    Array.from(host.querySelectorAll("button")).find((b) => b.textContent === words);

  it("offers no client button while the service is off", async () => {
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    expect(button("Add to Claude Code")).toBeUndefined();
    expect(button("Add to Codex")).toBeUndefined();
  });

  it("adds itself to a client through the command, and asks for an update once the token has moved on", async () => {
    held.status = held.on;
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    await act(async () => {
      button("Add to Claude Code")?.click();
    });
    await flush();
    expect(held.registerMcpClient).toHaveBeenCalledWith("claude_code");
    expect(button("Added to Claude Code")).toBeDefined();
    // The other client was not touched and still says so.
    expect(button("Add to Codex")).toBeDefined();
    expect(host.textContent).toContain("restarted");
    // It was given the skill too, and says where; Codex has none to be given.
    expect(host.textContent).toContain("/home/someone/.claude/skills/polarexplorer/SKILL.md");

    // Claude Code now holds a token the listener no longer answers.
    const rotate = Array.from(host.querySelectorAll("button")).find((b) => b.textContent?.includes("Rotate"));
    await act(async () => {
      rotate?.click();
    });
    await flush();
    expect(button("Update in Claude Code")).toBeDefined();
    expect(host.textContent).not.toContain("restarted");
  });

  it("opens the extension in Claude Desktop, never claims it was installed, and never asks for an update", async () => {
    held.status = held.on;
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    await act(async () => {
      button("Add to Claude Desktop")?.click();
    });
    await flush();
    expect(held.registerMcpClient).toHaveBeenCalledWith("claude_desktop");
    expect(button("Opened in Claude Desktop")).toBeDefined();
    expect(host.textContent).toContain("confirm it there");
    expect(host.textContent).not.toContain("picks it up when it is restarted");
    // The skill cannot ride in the extension: the person is told where it is.
    expect(host.textContent).toContain("PolarExplorer-skill.zip");
    expect(host.textContent).toContain("upload");

    // The extension reads the token as it goes: a rotation stales nothing.
    const rotate = Array.from(host.querySelectorAll("button")).find((b) => b.textContent?.includes("Rotate"));
    await act(async () => {
      rotate?.click();
    });
    await flush();
    expect(button("Opened in Claude Desktop")).toBeDefined();
    expect(button("Update in Claude Desktop")).toBeUndefined();
  });

  it("offers only the clients this platform has", async () => {
    held.status = { ...held.on, clients: ["claude_code", "codex"] };
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    expect(button("Add to Codex")).toBeDefined();
    expect(button("Add to Claude Desktop")).toBeUndefined();
  });

  it("reports a client that could not be written and does not claim it was", async () => {
    held.status = held.on;
    const refusal = new Error("the `claude` command was not found");
    held.registerMcpClient.mockImplementation(async () => {
      throw refusal;
    });
    const onError = vi.fn();
    act(() => root.render(<McpSection onError={onError} />));
    await flush();
    await act(async () => {
      button("Add to Codex")?.click();
    });
    await flush();
    expect(held.registerMcpClient).toHaveBeenCalledWith("codex");
    expect(onError).toHaveBeenCalledWith(refusal);
    expect(button("Add to Codex")).toBeDefined();
    expect(button("Added to Codex")).toBeUndefined();
  });

  it("says why ChatGPT is not offered, whether the service is on or off", async () => {
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    expect(host.querySelector(".mcp-chatgpt")?.textContent).toContain("ChatGPT");
    held.status = held.on;
    act(() => root.unmount());
    root = createRoot(host);
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    expect(host.querySelector(".mcp-chatgpt")?.textContent).toContain("only on this computer");
    expect(button("Add to ChatGPT")).toBeUndefined();
  });

  it("changes the port through the command, and shows why the listener is not up", async () => {
    held.status = { ...held.on, bound_port: null, bind_error: "could not listen on 127.0.0.1:47392: address in use" };
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    // The line is the interface's own, in its language; the system's
    // English reason is kept for the tooltip, as every error's is.
    const error = host.querySelector<HTMLElement>(".modal-error")!;
    expect(error.textContent).toBe("The service could not start listening on port 47392. Another application may be using that port: choose another.");
    expect(error.title).toContain("address in use");
    const port = host.querySelector<HTMLInputElement>('[data-feature="settings:mcp-port"]')!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(port, "50001");
      port.dispatchEvent(new Event("input", { bubbles: true }));
      port.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
    await flush();
    expect(held.setMcp).toHaveBeenCalledWith(true, 50001);
  });

  it("says who is connected and what they last did", async () => {
    held.status = { ...held.on, sessions: 2, last_tool: "blend_cell" };
    act(() => root.render(<McpSection onError={() => {}} />));
    await flush();
    expect(host.querySelector(".mcp-connected")?.textContent).toBe("2 clients connected, last: blend_cell");
  });
});
