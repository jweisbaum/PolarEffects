/**
 * The picture the MCP service's `screenshot` tool asks for (spec.md 3.7).
 *
 * Every stage is a canvas, so only the frontend can say what is on it. Each
 * stage registers its canvas with the redraw that paints it now: a WebGL
 * canvas has no preserved drawing buffer and reads back empty once its
 * frame has been shown, so the redraw and the read happen in one task. The
 * picture is of the stage's canvas alone; the panels around it are not in it.
 *
 * Unlike `automation.ts`'s seams this is in every build: the service is a
 * shipped feature, switched on in Settings.
 */

const registered = new Map<HTMLCanvasElement, () => void>();

/**
 * Offers a stage's canvas for the service's screenshot. `redraw` paints the
 * canvas synchronously. Returns the unregistration, for an effect's cleanup.
 */
export function registerCapture(canvas: HTMLCanvasElement, redraw: () => void): () => void {
  registered.set(canvas, redraw);
  return () => {
    if (registered.get(canvas) === redraw) registered.delete(canvas);
  };
}

/**
 * The stage on show as base64 PNG data: the largest registered canvas that
 * is in the document and laid out (the full-size plot covers the stage
 * under it, and is the larger). Throws, with a reason for the client, when
 * there is none or it gives no picture. `read` is for tests.
 */
export function captureStage(
  read: (canvas: HTMLCanvasElement) => string = (canvas) => canvas.toDataURL("image/png"),
): string {
  let best: HTMLCanvasElement | null = null;
  let bestArea = 0;
  for (const canvas of registered.keys()) {
    if (!canvas.isConnected) continue;
    const box = canvas.getBoundingClientRect();
    const area = box.width * box.height;
    if (area > bestArea) { best = canvas; bestArea = area; }
  }
  if (best === null) throw new Error("there is nothing on show to photograph");
  registered.get(best)?.();
  const url = read(best);
  const comma = url.indexOf(",");
  const data = comma >= 0 ? url.slice(comma + 1) : "";
  if (!url.startsWith("data:image/png") || data.length === 0) throw new Error("the stage gave no picture");
  return data;
}
