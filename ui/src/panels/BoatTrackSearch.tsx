import { useBoatApi } from "../boats/context";
import { useEffect, useState } from "react";
import type { BoatTrackSearch as SearchResult } from "../generated/BoatTrackSearch";
import type { TrackImportResult } from "../generated/TrackImportResult";

import { useT } from "../i18n";
import { describeError } from "../errors";

export default function BoatTrackSearch({ onImport }: { onImport: (result: TrackImportResult) => void }) {
  const api = useBoatApi();
  const t = useT();
  const [query, setQuery] = useState("");
  const [offset, setOffset] = useState(0);
  const [result, setResult] = useState<SearchResult | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [importing, setImporting] = useState<string | null>(null);
  // Keep catalogue offsets intact while removing imports from each search.
  const [added, setAdded] = useState(() => new Map<string, ReadonlySet<string>>());
  const removed = added.get(query);
  const remaining = Math.max(0, (result?.total ?? 0) - (removed?.size ?? 0));
  useEffect(() => {
    let active = true;
    setResult(null); setError(null);
    if (!query.trim()) { setLoading(false); return; }
    setLoading(true);
    const timer = setTimeout(() => {
      void api.searchDatabaseBoats(query, offset).then(r => { if (active) setResult(r); }).catch(e => { if (active) setError(e); }).finally(() => { if (active) setLoading(false); });
    }, 200);
    return () => { active = false; clearTimeout(timer); };
  }, [query, offset]);
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
  return <div className="boat-track-search">
    <label>{t("Search tracks by vessel details")}<input type="search" data-feature="tracks:boat-search" title={t("Search all vessel fields, including name, model, class, make and builder")} placeholder={t("Name, model, class, make, builder…")} value={query} onChange={e => { setQuery(e.target.value); setOffset(0); }} /></label>
    {loading && <p role="status">{t("Searching boat tracks…")}</p>}
    {result && !result.downloaded && <p>{t("Download boat metadata in Settings to search local tracks.")}</p>}
    {result?.downloaded && <>
      <p role="status">{remaining === 1 ? t("1 matching track") : t("{count} matching tracks", { count: remaining })}</p>
      <ul className="boat-track-results">{result.hits.filter(hit => !removed?.has(hit.id)).map(hit => <li key={hit.id}>
        <strong>{hit.boat_name}</strong><span>{hit.event_name}</span><small>{[hit.source, hit.model, hit.sail_number, hit.start?.slice(0, 10)].filter(Boolean).join(" · ")}</small>
        {hit.original_url && <small title={hit.original_url}>{hit.original_url}</small>}
        {!hit.file_available && <small className="modal-error">{t("GeoJSON file missing from the configured directory")}</small>}
        <button data-feature="tracks:boat-import" title={t("Import this boat track; weather can be fetched afterwards")} disabled={!hit.file_available || importing !== null} onClick={() => void add(hit.id)}>{importing === hit.id ? t("Importing track…") : t("Import boat track")}</button>
      </li>)}</ul>
      {result.total > 100 && <div className="section-actions"><button data-feature="tracks:boat-previous" title={t("Previous boat tracks")} disabled={offset === 0 || loading} onClick={() => setOffset(n => Math.max(0, n - 100))}>{t("Previous boat tracks")}</button><button data-feature="tracks:boat-next" title={t("More boat tracks")} disabled={offset + 100 >= result.total || loading} onClick={() => setOffset(n => n + 100)}>{t("More boat tracks")}</button></div>}
    </>}
    {error !== null && <p className="modal-error" role="alert">{describeError(error).text}<br />{describeError(error).detail}</p>}
  </div>;
}
