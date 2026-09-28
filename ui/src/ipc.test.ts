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
});
