/**
 * Seams for the WebDriver tools (D25, `tools/webdriver`), in development
 * builds only.
 *
 * Every function here is gated on `import.meta.env.DEV`, which Vite replaces
 * with `false` in a built bundle, so the bodies are dropped and a shipped
 * application has no way in through them. The driver runs the development
 * build (`npm run dev:webdriver`) for the same reason VectorEffects' does: the
 * dev build under React StrictMode is where a class of bug lives (a8cf1dd).
 *
 * Nothing here adds user-visible text or a control.
 */

type Hooks = {
  /** Answers the next native file dialogs take instead of showing a picker. */
  __peDialogAnswers?: unknown[];
};

const hooks = () => window as unknown as Hooks;

/**
 * The answer the driver queued for the next file dialog, or `undefined` to
 * show the real one.
 *
 * A native picker is outside the webview, where no WebDriver can reach, so a
 * UX test that imports a file queues the path first (`window.__peDialogAnswers
 * = ["/path/Farr 40.txt"]`) and then clicks the application's own Import…
 * button — the whole flow runs, only the picker is skipped.
 */
export function takeDialogAnswer(): unknown {
  if (!import.meta.env.DEV) return undefined;
  const queue = hooks().__peDialogAnswers;
  if (!Array.isArray(queue) || queue.length === 0) return undefined;
  return queue.shift();
}

/** A canvas that can be told to draw now, for a picture of it. */
type Redrawable = HTMLCanvasElement & { __peRedraw?: () => void };

/**
 * Lets the driver's screenshot redraw a WebGL canvas synchronously.
 *
 * The endpoint's own screenshot rasterises the DOM, which leaves every canvas
 * blank, so the driver composites each canvas into the picture itself. A 2D
 * canvas reads back as it is; a WebGL one (the map, the 3D view) has no
 * preserved drawing buffer and reads back empty once the frame is shown, so
 * the driver calls this and reads the canvas in the same task. It also means
 * a window whose frames are throttled (not composited) still photographs.
 * Returns the unregistration, for an effect's cleanup.
 */
export function registerRedraw(canvas: HTMLCanvasElement, redraw: () => void): () => void {
  if (!import.meta.env.DEV) return () => undefined;
  const target = canvas as Redrawable;
  target.__peRedraw = redraw;
  return () => {
    if (target.__peRedraw === redraw) delete target.__peRedraw;
  };
}

/** Read-only scene observations for the isolated desktop comparison test. */
export function registerPolarInspection(canvas: HTMLCanvasElement, read: () => unknown): () => void {
  if (!import.meta.env.DEV) return () => undefined;
  const target = canvas as HTMLCanvasElement & { __peInspectPolar?: () => unknown };
  target.__peInspectPolar = read;
  return () => { delete target.__peInspectPolar; };
}
