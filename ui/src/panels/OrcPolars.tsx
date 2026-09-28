import { useEffect, useRef, useState } from "react";

import { reportFailure } from "../errors";
import type { OrcCatalogueInfo } from "../generated/OrcCatalogueInfo";
import type { OrcHit } from "../generated/OrcHit";
import type { OrcSearchResult } from "../generated/OrcSearchResult";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { onReveal } from "../help/highlight";
import { setHint } from "../hint";
import { useT } from "../i18n";
import { api, IpcError } from "../ipc";
import ConfirmDialog from "../project/ConfirmDialog";
import { THUMB_HEIGHT, THUMB_WIDTH, thumbPaths } from "./orcThumb";

/** How many results a search shows. */
const LIMIT = 50;

/** A year field's value as a filter: a whole number, or none. */
function year(text: string): number | null {
  const value = Number.parseInt(text, 10);
  return Number.isFinite(value) ? value : null;
}

/** The fields a result or an added certificate shows under its name. */
function meta(parts: (string | number | null | undefined)[]): string {
  return parts.filter((part) => part !== null && part !== undefined && part !== "").join(" · ");
}

/** The small polar of a result: light, medium and strong wind. */
function Thumb({ hit }: { hit: OrcHit }) {
  const t = useT();
  const paths = thumbPaths(hit.thumb);
  const speeds = hit.thumb.map((curve) => curve.tws).join(", ");
  return (
    <svg className="orc-thumb" width={THUMB_WIDTH} height={THUMB_HEIGHT} viewBox={`0 0 ${THUMB_WIDTH} ${THUMB_HEIGHT}`}
      role="img" aria-label={t("Polar at {speeds} kn", { speeds })}>
      {paths.map((d, i) => <path key={i} d={d} className={`orc-thumb-${i}`} />)}
    </svg>
  );
}

/**
 * The ORC polars section of the left navigation (spec.md 5): one search box
 * over the embedded catalogue, results updated as you type with a thumbnail
 * each, filters by year built and country, Add (asks first when the project
 * already holds that certificate), and the certificates already added, each
 * with its colour and Remove (undoable).
 */
export default function OrcPolars({ project, onProject }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
}) {
  const t = useT();
  const [info, setInfo] = useState<OrcCatalogueInfo | null>(null);
  const [query, setQuery] = useState("");
  const [yearFrom, setYearFrom] = useState("");
  const [yearTo, setYearTo] = useState("");
  const [country, setCountry] = useState("");
  const [result, setResult] = useState<OrcSearchResult | null>(null);
  const [again, setAgain] = useState<OrcHit | null>(null);
  const latest = useRef(0);
  const added = project.sources.filter((source) => source.orc !== null);
  const listing = useRef(false);
  listing.current = (result?.hits.length ?? 0) > 0;
  const newest = useRef<number | null>(null);
  newest.current = info?.year_max ?? null;

  useEffect(() => {
    api.orcCatalogueInfo().then(setInfo).catch(reportFailure);
  }, []);

  // The search's reveal step for Add (spec.md 3.6): with nothing listed,
  // list the newest boats, so there is an Add button to point at.
  useEffect(() => onReveal("orc:results", () => {
    if (listing.current) return;
    setQuery("");
    setCountry("");
    setYearTo("");
    setYearFrom(String(newest.current ?? new Date().getFullYear()));
  }), []);

  // Every keystroke searches; only the newest answer is shown.
  useEffect(() => {
    const filters = { year_min: year(yearFrom), year_max: year(yearTo), country: country === "" ? null : country };
    if (query.trim() === "" && filters.year_min === null && filters.year_max === null && filters.country === null) {
      latest.current += 1;
      setResult(null);
      return;
    }
    const ticket = ++latest.current;
    api.orcSearch(query, filters, LIMIT)
      .then((found) => { if (ticket === latest.current) setResult(found); })
      .catch(reportFailure);
  }, [query, yearFrom, yearTo, country, project.revision]);

  const add = async (hit: OrcHit, allowDuplicate: boolean) => {
    if (hit.in_project && !allowDuplicate) {
      setAgain(hit);
      return;
    }
    try {
      onProject(await api.orcAdd(hit.id, allowDuplicate));
      setHint(t("Added {name}.", { name: hit.name || hit.sail_no }));
    } catch (error) {
      if (error instanceof IpcError && error.kind === "orc-duplicate") setAgain(hit);
      else reportFailure(error);
    }
  };

  const remove = (id: number) => {
    void api.removeSource(id).then(onProject).catch(reportFailure);
  };

  return (
    <>
      <input type="search" className="orc-search" value={query} data-feature="orc:search"
        placeholder={t("Name, sail number, model, builder, year…")}
        aria-label={t("Search the ORC catalogue")}
        title={t("Every word must match a field: name, sail number, country, model, builder, designer or year")}
        onChange={(event) => setQuery(event.target.value)} />
      <div className="orc-filters">
        <input type="number" inputMode="numeric" value={yearFrom} data-feature="orc:year-from"
          min={info?.year_min ?? undefined} max={info?.year_max ?? undefined}
          placeholder={t("From")} aria-label={t("Built from")} title={t("Earliest year built")}
          onChange={(event) => setYearFrom(event.target.value)} />
        <input type="number" inputMode="numeric" value={yearTo} data-feature="orc:year-to"
          min={info?.year_min ?? undefined} max={info?.year_max ?? undefined}
          placeholder={t("To")} aria-label={t("Built until")} title={t("Latest year built")}
          onChange={(event) => setYearTo(event.target.value)} />
        <select value={country} data-feature="orc:country" aria-label={t("Country")}
          title={t("Only certificates from this country")} onChange={(event) => setCountry(event.target.value)}>
          <option value="">{t("All countries")}</option>
          {(info?.countries ?? []).map((code) => <option key={code} value={code}>{code}</option>)}
        </select>
      </div>
      {result !== null && (
        <>
          <p className="muted orc-count">
            {result.total === 0
              ? t("No certificate matches.")
              : result.total > result.hits.length
                ? t("Best {shown} of {total} certificates", { shown: result.hits.length, total: result.total })
                : result.total === 1 ? t("1 certificate") : t("{total} certificates", { total: result.total })}
          </p>
          <ul className="orc-results" aria-label={t("Search results")}>
            {result.hits.map((hit) => (
              <li key={hit.id}>
                <Thumb hit={hit} />
                <span className="orc-text">
                  <span className="orc-name">{hit.name || hit.sail_no}</span>
                  <span className="muted orc-meta">{meta([hit.sail_no, hit.model, hit.year, hit.builder])}</span>
                  {hit.certificate_year !== null && (
                    <span className="muted orc-meta">{t("Certificate {year}", { year: hit.certificate_year })}</span>
                  )}
                </span>
                <button className="small" data-feature="orc:add" onClick={() => void add(hit, false)}
                  title={hit.in_project
                    ? t("Already in the project; adding it again asks first")
                    : t("Add this certificate to the project as an ORC polar")}>
                  {hit.in_project ? t("Added") : t("Add")}
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
      {added.length === 0
        ? <p className="muted placeholder">{t("No ORC polars in this project yet.")}</p>
        : <ul className="polar-file-list orc-added" aria-label={t("ORC polars in the project")}>
          {added.map((source) => (
            <li key={source.id}>
              <span className="swatch" style={{ backgroundColor: source.colour }} aria-hidden="true" />
              <span className="polar-file-text">
                <span className="polar-file-label">{source.label}</span>
                <span className="muted polar-file-meta">
                  {meta([source.orc!.sail_no, source.orc!.model, source.orc!.year])}
                  {source.orc!.certificate_year !== null
                    && ` · ${t("Certificate {year}", { year: source.orc!.certificate_year })}`}
                </span>
              </span>
              <button className="icon-button" data-feature="orc:remove" onClick={() => remove(source.id)}
                title={t("Remove this ORC polar from the project (undoable)")} aria-label={t("Remove")}>
                ✕
              </button>
            </li>
          ))}
        </ul>}
      {info !== null && (
        <p className="muted orc-provenance" title={info.commit}>
          {t("Catalogue: {records} certificates from jieter/orc-data of {date}", {
            records: info.records, date: info.commit_date,
          })}
        </p>
      )}
      {again !== null && (
        <ConfirmDialog title={t("Add this certificate again?")}
          body={t("{name} is already in the project. Add a second copy?", { name: again.name || again.sail_no })}
          confirmLabel={t("Add again")}
          onCancel={() => setAgain(null)}
          onConfirm={() => {
            const hit = again;
            setAgain(null);
            void add(hit, true);
          }} />
      )}
    </>
  );
}
