import { describe, expect, it, vi } from "vitest";

import { quitThroughGuard } from "./quitGuard";

const never = () => {
  throw new Error("should not have been called");
};

describe("quitting through the save guard", () => {
  it("quits without asking when nothing would be lost", async () => {
    const quit = vi.fn(async () => undefined);
    expect(await quitThroughGuard({ current: async () => null, ask: never, save: never, quit })).toBe(true);
    expect(await quitThroughGuard({ current: async () => ({ dirty: false }), ask: never, save: never, quit })).toBe(true);
    expect(quit).toHaveBeenCalledWith(false);
  });

  it("stays open when the user cancels", async () => {
    const quit = vi.fn();
    const save = vi.fn();
    expect(await quitThroughGuard({ current: async () => ({ dirty: true }), ask: async () => "cancel", save, quit })).toBe(false);
    expect(quit).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
  });

  it("carries Don't save to Rust, which checks it", async () => {
    const quit = vi.fn(async () => undefined);
    await quitThroughGuard({ current: async () => ({ dirty: true }), ask: async () => "discard", save: never, quit });
    expect(quit).toHaveBeenCalledWith(true);
  });

  it("saves first, then quits with nothing to discard", async () => {
    const order: string[] = [];
    await quitThroughGuard({
      current: async () => ({ dirty: true }),
      ask: async () => "save",
      save: async () => { order.push("save"); return true; },
      quit: async (discard) => { order.push(`quit:${discard}`); },
    });
    expect(order).toEqual(["save", "quit:false"]);
  });

  // The failure the guard exists for: Save chosen, the Save As dialog
  // cancelled, and the work thrown away regardless.
  it("stays open when the save does not complete", async () => {
    const quit = vi.fn();
    expect(await quitThroughGuard({
      current: async () => ({ dirty: true }), ask: async () => "save", save: async () => false, quit,
    })).toBe(false);
    expect(quit).not.toHaveBeenCalled();
  });

  it("asks about what Rust has now, not what the view last saw", async () => {
    const ask = vi.fn(async () => "cancel" as const);
    await quitThroughGuard({ current: async () => ({ dirty: true }), ask, save: never, quit: never });
    expect(ask).toHaveBeenCalledOnce();
  });
});
