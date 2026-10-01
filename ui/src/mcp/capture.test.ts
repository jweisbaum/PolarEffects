// @vitest-environment happy-dom
/**
 * The picture the MCP service's `screenshot` asks for (spec.md 3.7): the
 * stage on show, redrawn and read in one task.
 */
import { afterEach, describe, expect, it } from "vitest";

import { captureStage, registerCapture } from "./capture";

function canvas(width: number, height: number, attached = true): HTMLCanvasElement {
  const element = document.createElement("canvas");
  element.getBoundingClientRect = () => ({ left: 0, top: 0, right: width, bottom: height, width, height, x: 0, y: 0, toJSON: () => ({}) });
  if (attached) document.body.append(element);
  return element;
}
const read = (element: HTMLCanvasElement) => `data:image/png;base64,PNG${element.getBoundingClientRect().width}`;

afterEach(() => { document.body.replaceChildren(); });

describe("captureStage", () => {
  it("redraws the largest canvas on show, then reads it", () => {
    const order: string[] = [];
    const small = canvas(200, 200);
    const large = canvas(900, 600);
    const offSmall = registerCapture(small, () => order.push("small"));
    const offLarge = registerCapture(large, () => order.push("large"));
    expect(captureStage((element) => { order.push("read"); return read(element); })).toBe("PNG900");
    expect(order).toEqual(["large", "read"]);
    offLarge();
    expect(captureStage(read)).toBe("PNG200");
    offSmall();
  });

  it("passes over a canvas that is not on screen", () => {
    const hidden = canvas(0, 0);
    const detached = canvas(800, 600, false);
    const offs = [registerCapture(hidden, () => undefined), registerCapture(detached, () => undefined)];
    expect(() => captureStage(read)).toThrow(/nothing on show/);
    for (const off of offs) off();
  });

  it("says so when nothing can be photographed or the canvas reads back empty", () => {
    expect(() => captureStage(read)).toThrow(/nothing on show/);
    const blank = canvas(300, 300);
    const off = registerCapture(blank, () => undefined);
    expect(() => captureStage(() => "data:,")).toThrow(/gave no picture/);
    off();
  });
});
