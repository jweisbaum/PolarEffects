import { useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { useLanguage, useT } from "../i18n";
import { onReveal } from "./highlight";
import { onOpenHelp } from "./open";
import { searchTopics, topicsFor, type Parameter } from "./topics";

function Parameters({ rows }: { rows: readonly Parameter[] | undefined }) {
  const t = useT();
  if (!rows?.length) return null;
  return <table className="help-parameters"><thead><tr><th>{t("Control or option")}</th><th>{t("What it does")}</th></tr></thead>
    <tbody>{rows.map(([name, description]) => <tr key={name}><th scope="row">{name}</th><td>{description}</td></tr>)}</tbody></table>;
}

/**
 * The help reference (spec.md 3.5): a window of translated topics, one per
 * area, with its own search. Copied from VectorEffects. Opened by F1, the
 * native menu's PolarEffects Help, the "?" button and the feature search.
 */
export default function Help({ children }: { children: ReactNode }) {
  const t = useT();
  const topics = topicsFor(useLanguage());
  const [open, setOpen] = useState(false);
  const [topicId, setTopicId] = useState("workspace");
  const [query, setQuery] = useState("");
  const previousFocus = useRef<HTMLElement | null>(null);
  const show = () => {
    previousFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setOpen(true);
  };
  const close = () => setOpen(false);
  useEffect(() => {
    // Outside Tauri (a test, the plain Vite page) there is no event bus.
    const listening = listen("help://open", show).catch(() => null);
    const opening = onOpenHelp(topic => {
      if (topic) { setTopicId(topic); setQuery(""); }
      show();
    });
    // The search's reveal step for the window's own controls (spec.md 3.6).
    const revealing = onReveal("help:open", () => show());
    return () => { opening(); revealing(); void listening.then(off => off?.()); };
  }, []);
  useEffect(() => {
    if (!open) previousFocus.current?.focus({ preventScroll: true });
    const key = (e: KeyboardEvent) => {
      if (e.key === "F1") { e.preventDefault(); e.stopImmediatePropagation(); if (!open) show(); }
      else if (open) {
        e.stopImmediatePropagation();
        if (e.key === "Escape") { e.preventDefault(); close(); }
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [open]);
  const found = searchTopics(query, topics);
  const topic = found.find(p => p.id === topicId) ?? found[0];
  const choose = (id: string, clearQuery = false) => {
    if (clearQuery) setQuery("");
    setTopicId(id);
  };
  return <>
    <div style={{ display: "contents" }} inert={open || undefined}>{children}</div>
    {open && <div className="modal-backdrop help-backdrop" onClick={close}>
      <section className="help-dialog" role="dialog" aria-modal="true" aria-labelledby="help-title" onClick={e => e.stopPropagation()}>
        <header><h2 id="help-title">{t("PolarEffects Help")}</h2>
          <button data-feature="help:close" onClick={close} aria-label={t("Close help")} title={t("Close help (Esc)")}>{t("Close")}</button></header>
        <div className="help-body"><nav aria-label={t("Help topics")}>
          <input autoFocus type="search" data-feature="help:search" aria-label={t("Search help")} title={t("Search the help pages")}
            placeholder={t("Search the help…")} value={query} onChange={e => setQuery(e.target.value)} />
          <p className="help-count" role="status">{t("{found} of {total} pages", { found: found.length, total: topics.length })}</p>
          {found.map((p, i) => <div key={p.id}>
            {(i === 0 || found[i - 1]?.group !== p.group) && <h3>{p.group}</h3>}
            <button data-feature="help:topic" aria-current={topic?.id === p.id ? "page" : undefined}
              title={p.group} onClick={() => choose(p.id)}>{p.title}</button>
          </div>)}
        </nav><article key={topic?.id} tabIndex={0} aria-label={topic?.title ?? t("Search results")}>
          {topic ? <><h3>{topic.title}</h3>{topic.paragraphs.map(p => <p key={p}>{p}</p>)}
            <Parameters rows={topic.parameters} />
            {topic.related && <footer className="help-related"><h4>{t("Related pages")}</h4>
              {topic.related.map(id => <button key={id} data-feature="help:related" title={t("Go to this page")} onClick={() => choose(id, true)}>{topics.find(page => page.id === id)?.title}</button>)}
            </footer>}
          </> : <p>{t("No page matches “{query}”.", { query })}</p>}
        </article></div>
      </section>
    </div>}
  </>;
}
