/**
 * Which panels and sections are open (spec.md 3.2). Copied from
 * VectorEffects.
 *
 * The left navigation folds away as a whole and per section; the right panel
 * folds away as a whole and per section. A viewer's convenience, kept in
 * `localStorage` — "remembered per user, not per project" — never a
 * project's fact: two people opening one file may want different panels put
 * away.
 */

export interface PanelState {
  /** The left navigation. */
  left: boolean;
  /** The right panel. */
  right: boolean;
  /** Left navigation sections, in order (spec.md 3.2). */
  orc: boolean;
  polarFiles: boolean;
  tracks: boolean;
  /** Right panel sections. */
  sources: boolean;
  plot: boolean;
}

const KEY = "pe.panels";

export const OPEN: PanelState = {
  left: true,
  right: true,
  orc: true,
  polarFiles: true,
  tracks: true,
  sources: true,
  plot: true,
};

/** Which dock a section sits in, so revealing it opens the dock too. */
export const SECTION_DOCK: Readonly<Partial<Record<keyof PanelState, "left" | "right">>> = {
  orc: "left",
  polarFiles: "left",
  tracks: "left",
  sources: "right",
  plot: "right",
};

/** The remembered layout, or everything open. Storage may be absent or refuse. */
export function loadPanels(): PanelState {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (raw === null) return OPEN;
    return normalise(JSON.parse(raw) as Partial<PanelState>);
  } catch {
    return OPEN;
  }
}

/** Remembers a layout, if storage allows. */
export function savePanels(state: PanelState): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(state));
  } catch {
    // A private window, or storage refused: the layout lasts the session.
  }
}

/** The layout with one panel or section flipped. */
export function togglePanel(state: PanelState, panel: keyof PanelState): PanelState {
  return { ...state, [panel]: !state[panel] };
}

/**
 * The layout with `panel` open, and the dock it sits in: what the feature
 * search's `panel:` and `section:` reveal steps need. Unchanged (the same
 * object) when it is already showing.
 */
export function reveal(state: PanelState, panel: keyof PanelState): PanelState {
  const dock = SECTION_DOCK[panel];
  if (state[panel] && (dock === undefined || state[dock])) return state;
  return { ...state, [panel]: true, ...(dock ? { [dock]: true } : {}) };
}

/** A stored layout with anything missing or malformed replaced by open. */
export function normalise(partial: Partial<PanelState> | null | undefined): PanelState {
  const pick = (key: keyof PanelState) =>
    typeof partial?.[key] === "boolean" ? (partial[key] as boolean) : true;
  return {
    left: pick("left"),
    right: pick("right"),
    orc: pick("orc"),
    polarFiles: pick("polarFiles"),
    tracks: pick("tracks"),
    sources: pick("sources"),
    plot: pick("plot"),
  };
}
