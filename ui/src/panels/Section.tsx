import type { ReactNode } from "react";

import { useT } from "../i18n";

/**
 * A foldable section of a side panel (spec.md 3.2): a heading that folds and
 * unfolds it, and its body. The heading is the control, so it carries the
 * `data-feature` the search flashes.
 */
export default function Section({ feature, title, tooltip, open, onToggle, children }: {
  feature: string;
  /** The heading, already translated. */
  title: string;
  /** What the section holds, already translated. */
  tooltip: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  const t = useT();
  return (
    <section className="panel-section">
      <header>
        <button className="panel-heading" data-feature={feature} aria-expanded={open} onClick={onToggle}
          title={open ? t("{section}: click to fold", { section: tooltip }) : t("{section}: click to unfold", { section: tooltip })}>
          <span className="disclose" aria-hidden="true">{open ? "▾" : "▸"}</span>
          <h2>{title}</h2>
        </button>
      </header>
      {open && <div className="panel-body">{children}</div>}
    </section>
  );
}
