import { useBoatApi } from "../boats/context";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import type { BoatTrackSearch as SearchResult } from "../generated/BoatTrackSearch";
import type { TrackImportResult } from "../generated/TrackImportResult";

import { useT } from "../i18n";
import { describeError } from "../errors";
import { LIBRARY_METADATA, LIBRARY_SCRAPE } from "../ipc";


/**
 * Searches the track library (spec.md 3.8) as you type, the next hundred
 * tracks loading as the list is scrolled to its end (asked 2026-10-02).
 */
export default function BoatTrackSearch({ onImport, metadataDirectory = "" }: {
  onImport: (result: TrackImportResult) => void;
  metadataDirectory?: string;
}) {
  const api = useBoatApi();
  const t = useT();
  const [query, setQuery] = useState("");
  const [available, setAvailable] = useState(false);
  useEffect(() => {
    let live = true;
    let generation = 0;
    setAvailable(false);
    const check = () => {
      const ticket = ++generation;
      void api.trackMetadataAvailable().then(value => {
        if (live && ticket === generation) setAvailable(value);
      }).catch(() => { if (live && ticket === generation) setAvailable(false); });
    };
    check();
    window.addEventListener("focus", check);
    const stops = [LIBRARY_METADATA, LIBRARY_SCRAPE].map(name =>
      listen<{ running: boolean }>(name, event => { if (!event.payload.running) check(); }).catch(() => null));
    return () => {
      live = false;
      window.removeEventListener("focus", check);
      for (const stop of stops) void stop.then(off => off?.());
    };
  }, [api, metadataDirectory]);
  const [result, setResult] = useState<SearchResult | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [importing, setImporting] = useState<string | null>(null);
  // Keep the pages shown intact while removing imports from each search.
  const [added, setAdded] = useState(() => new Map<string, ReadonlySet<string>>());
  const removed = added.get(query);
  const remaining = Math.max(0, (result?.total ?? 0) - (removed?.size ?? 0));
  const latest = useRef(0);
  useEffect(() => {
    const ticket = ++latest.current;
    setResult(null); setError(null);
    if (!available || !query.trim()) { setLoading(false); return; }
    setLoading(true);
    const timer = setTimeout(() => {
      void api.searchDatabaseBoats(query, 0).then(r => {
        if (ticket === latest.current) { setResult(r); if (!r.downloaded) setAvailable(false); }
      }).catch(e => { if (ticket === latest.current) setError(e); }).finally(() => { if (ticket === latest.current) setLoading(false); });
    }, 50);
    return () => { clearTimeout(timer); ++latest.current; };
  }, [api, available, metadataDirectory, query]);
  const more = result !== null && result.downloaded && result.hits.length < result.total;
  const loadMore = () => {
    if (!result || !more || loading) return;
    const ticket = latest.current;
    setLoading(true);
    void api.searchDatabaseBoats(query, result.hits.length)
      .then(r => { if (ticket === latest.current) { setResult({ ...r, hits: [...result.hits, ...r.hits] }); if (!r.downloaded) setAvailable(false); } })
      .catch(e => { if (ticket === latest.current) setError(e); })
      .finally(() => { if (ticket === latest.current) setLoading(false); });
  };
  const sentinel = useRef<HTMLLIElement | null>(null);
  const onMore = useRef(loadMore);
  onMore.current = loadMore;
  useEffect(() => {
    const element = sentinel.current;
    if (!more || loading || !element || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) onMore.current();
    }, { root: element.parentElement, rootMargin: "80px" });
    observer.observe(element);
    return () => observer.disconnect();
  }, [more, loading]);
  const add = async (id: string) => {
    setImporting(id); setError(null);
    try {
      const imported = await api.importDatabaseTrack(id);
      onImport(imported);
      if (imported.imported.length > 0) {
        setAdded(previous => new Map(previous).set(query, new Set([...(previous.get(query) ?? []), id])));
      }
    } catch (e) { setError(e); } finally { setImporting(null); }
  };
  if (!available) return null;
  return <div className="boat-track-search">
    <label>{t("Search tracks by vessel details")}<input type="search" data-feature="tracks:boat-search" title={t("Search all vessel fields, including name, model, class, make and builder")} placeholder={t("Name, model, class, make, builder…")} value={query} onChange={e => setQuery(e.target.value)} /></label>
    {loading && <p role="status">{t("Searching boat tracks…")}</p>}
    {result?.downloaded && <>
      <p role="status">{remaining === 1 ? t("1 matching track") : t("{count} matching tracks", { count: remaining })}</p>
      <ul className="boat-track-results">{result.hits.filter(hit => !removed?.has(hit.id)).map(hit => <li key={hit.id}>
        <strong>{hit.boat_name}</strong><span>{hit.event_name}</span><small>{[hit.source, hit.model, hit.sail_number, hit.start?.slice(0, 10)].filter(Boolean).join(" · ")}</small>
        {hit.original_url && <small title={hit.original_url}>{hit.original_url}</small>}
        {!hit.file_available && <small className="modal-error">{t("GeoJSON file missing from the configured directory")}</small>}
        <button data-feature="tracks:boat-import" title={t("Import this boat track; weather can be fetched afterwards")} disabled={!hit.file_available || importing !== null} onClick={() => void add(hit.id)}>{importing === hit.id ? t("Importing track…") : t("Import boat track")}</button>
      </li>)}
      {more && <li className="orc-more" ref={sentinel}>
        {/* Reached by scrolling; a button for a window without an IntersectionObserver. */}
        <button type="button" className="small" data-feature="tracks:boat-more" title={t("More boat tracks")} disabled={loading} onClick={loadMore}>{loading ? t("Loading more…") : t("More")}</button>
      </li>}
      </ul>
    </>}
    {error !== null && <p className="modal-error" role="alert">{describeError(error).text}<br />{describeError(error).detail}</p>}
  </div>;
}
