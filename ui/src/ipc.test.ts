import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { api, IpcError, isErrorPayload } from "./ipc";

describe("isErrorPayload", () => {
  it("accepts the shape Rust actually sends", () => {
    expect(isErrorPayload({ kind: "io", message: "disk full" })).toBe(true);
  });

  it("rejects a bare string, which is how a panic arrives", () => {
    expect(isErrorPayload("something exploded")).toBe(false);
    expect(isErrorPayload(null)).toBe(false);
    expect(isErrorPayload({ kind: "io" })).toBe(false);
  });
});

describe("api", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("calls the Rust command by its snake_case name", async () => {
    invoke.mockResolvedValue({ name: "PolarEffects", version: "0.1.0" });
    await expect(api.appInfo()).resolves.toEqual({ name: "PolarEffects", version: "0.1.0" });
    expect(invoke).toHaveBeenCalledWith("app_info", undefined);
  });

  it("turns an AppError payload into an IpcError with its kind", async () => {
    invoke.mockRejectedValue({ kind: "doing", message: "Could not read x: gone" });
    const err = await api.appInfo().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(IpcError);
    expect((err as IpcError).kind).toBe("doing");
    expect((err as IpcError).message).toBe("Could not read x: gone");
  });

  it("turns a bare string into an IpcError of unknown kind", async () => {
    invoke.mockRejectedValue("panicked");
    const err = await api.appInfo().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(IpcError);
    expect((err as IpcError).kind).toBe("unknown");
    expect((err as IpcError).message).toBe("panicked");
  });

  it("passes the unsaved-changes answer to every call that drops a project", async () => {
    invoke.mockResolvedValue(null);
    await api.newProject("Fastnet", { name: "Boat", notes: "" }, true);
    expect(invoke).toHaveBeenLastCalledWith("new_project", {
      name: "Fastnet",
      boat: { name: "Boat", notes: "" },
      discardUnsaved: true,
    });
    await api.openProject("/a.wpsproj");
    expect(invoke).toHaveBeenLastCalledWith("open_project", {
      path: "/a.wpsproj",
      discardUnsaved: false,
    });
    await api.closeProject(true);
    expect(invoke).toHaveBeenLastCalledWith("close_project", { discardUnsaved: true });
    await api.openRecovered(7);
    expect(invoke).toHaveBeenLastCalledWith("open_recovered", { id: 7, discardUnsaved: false });
  });

  it("names the history and recent-list commands as Rust does", async () => {
    invoke.mockResolvedValue([]);
    await api.undo();
    expect(invoke).toHaveBeenLastCalledWith("undo", undefined);
    await api.renameProject("New");
    expect(invoke).toHaveBeenLastCalledWith("rename_project", { name: "New" });
    await api.clearRecent();
    expect(invoke).toHaveBeenLastCalledWith("clear_recent", undefined);
    await api.saveProjectAs("/b");
    expect(invoke).toHaveBeenLastCalledWith("save_project_as", { path: "/b" });
  });
});
