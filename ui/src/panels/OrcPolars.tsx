import { useBoatApi } from "../boats/context";
import { listen } from "@tauri-apps/api/event";
import type { OrrHit } from "../generated/OrrHit";
import type { OrrCatalogueInfo } from "../generated/OrrCatalogueInfo";
import { MEASUREMENTS, measurementBounds } from "./measurements";
import { useEffect, useRef, useState } from "react";

import { reportFailure } from "../errors";
import type { OrcCatalogueInfo } from "../generated/OrcCatalogueInfo";
import type { OrcHit } from "../generated/OrcHit";
type SearchHit = OrcHit | OrrHit;
type SearchResult = { total: number; hits: SearchHit[] };
import type { ProjectSummary } from "../generated/ProjectSummary";
import { useBoatReveal } from "../boats/context";
import { later, setHint } from "../hint";
import { msg, useT } from "../i18n";
import { IpcError, ORR_UPDATED } from "../ipc";
import ConfirmDialog from "../project/ConfirmDialog";
import { activeCount, type FieldQueries, loadFieldsOpen, NO_FIELDS, saveFieldsOpen, toFilters } from "./orcFields";
import { THUMB_HEIGHT, THUMB_WIDTH, thumbPaths } from "./orcThumb";

/** How many results a search shows. */
const LIMIT = 50;

/** The fields a result or an added certificate shows under its name. */
function meta(parts: (string | number | null | undefined)[]): string {
  return parts.filter((part) => part !== null && part !== undefined && part !== "").join(" · ");
}

/** The small polar of a result: light, medium and strong wind. */
function Thumb({ hit }: { hit: SearchHit }) {
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
 * each; under it "Search by field", one box per field (year built and
 * country among them), folded away until wanted; Add (asks first when the project
 * already holds that certificate), and the certificates already added, each
 * with its colour and Remove (undoable).
 */
export default function OrcPolars({ project, onProject }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
}) {
  const api = useBoatApi();
  const onReveal = useBoatReveal();
  const t = useT();
  const [info, setInfo] = useState<OrcCatalogueInfo | OrrCatalogueInfo | null>(null);
  const [catalogue, setCatalogue] = useState<"orc" | "orr">("orc");
  const [catalogueRevision, setCatalogueRevision] = useState(0);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const [measureMin, setMeasureMin] = useState<string[]>(Array(9).fill(""));
  const [measureMax, setMeasureMax] = useState<string[]>(Array(9).fill(""));
  const sizeMin = measurementBounds(measureMin), sizeMax = measurementBounds(measureMax);
  const measuresValid = sizeMin !== null && sizeMax !== null && sizeMin.every((min, k) => min === null || sizeMax[k] === null || min <= sizeMax[k]!);
  const measurementCount = measureMin.filter((min, k) => min !== "" || measureMax[k] !== "").length;
  const changeMeasure = (index: number, value: string, max: boolean) => {
    setPage(0);
    (max ? setMeasureMax : setMeasureMin)((old) => old.map((v, k) => k === index ? value : v));
  };
  const [fields, setFields] = useState<FieldQueries>(NO_FIELDS);
  const [fieldsOpen, setFieldsOpen] = useState(loadFieldsOpen);
  const [result, setResult] = useState<SearchResult | null>(null);
  const [again, setAgain] = useState<SearchHit | null>(null);
  const latest = useRef(0);
  const added = project.sources.filter((source) => catalogue === "orc" ? source.orc !== null : source.orr !== null);
  const listing = useRef(false);
  listing.current = (result?.hits.length ?? 0) > 0;
  const newest = useRef<number | null>(null);
  newest.current = info && "year_max" in info ? info.year_max : null;

  useEffect(() => {
    let active = true;
    setInfo(null);
    const request = catalogue === "orc" ? api.orcCatalogueInfo() : api.orrCatalogueInfo();
    void request.then((info) => { if (active) setInfo(info); }).catch(reportFailure);
    return () => { active = false; };
  }, [catalogue, catalogueRevision]);
  useEffect(() => {
    const unlisten = listen(ORR_UPDATED, () => { setPage(0); setCatalogueRevision((r) => r + 1); });
    return () => { void unlisten.then((stop) => stop()); };
  }, []);

  // The search's reveal step for Add (spec.md 3.6): with nothing listed,
  // list the newest boats, so there is an Add button to point at.
  useEffect(() => onReveal("orc:results", () => {
    if (listing.current) return;
    setCatalogue("orc");
    setPage(0);
    setMeasureMin(Array(9).fill("")); setMeasureMax(Array(9).fill(""));
    setQuery("");
    setFields({ ...NO_FIELDS, year_from: String(newest.current ?? new Date().getFullYear()) });
  }), []);

  // The reveal step of every per-field box: unfold "Search by field".
  useEffect(() => onReveal("orc:fields", () => setFieldsOpen(true)), []);
  useEffect(() => saveFieldsOpen(fieldsOpen), [fieldsOpen]);

  // Every keystroke searches; only the newest answer is shown.
  const active = activeCount(fields) + measurementCount;
  useEffect(() => {
    if (!measuresValid || (catalogue === "orc" && query.trim() === "" && active === 0)) {
      latest.current += 1;
      setResult(null);
      return;
    }
    const ticket = ++latest.current;
    setResult(null);
    const search = catalogue === "orc" ? api.orcSearch : api.orrSearch;
    search(query, { ...toFilters(fields), size_min: measurementBounds(measureMin)!, size_max: measurementBounds(measureMax)! }, LIMIT, page * LIMIT)
      .then((found) => { if (ticket === latest.current) setResult(found); })
      .catch(reportFailure);
  }, [query, fields, active, project.revision, page, measureMin, measureMax, measuresValid, catalogue, catalogueRevision]);

  const field = (key: keyof FieldQueries) => ({
    value: fields[key],
    onChange: (event: { target: { value: string } }) => {
      const value = event.target.value;
      setPage(0);
      setFields((before) => ({ ...before, [key]: value }));
    },
  });

  const add = async (hit: SearchHit, allowDuplicate: boolean) => {
    if (typeof hit.id === "string" && hit.in_project) return;
    if (hit.in_project && !allowDuplicate) {
      setAgain(hit);
      return;
    }
    try {
      onProject(await (typeof hit.id === "string" ? api.orrAdd(hit.id) : api.orcAdd(hit.id, allowDuplicate)));
      setHint(later(msg("Added {name}."), { name: hit.name || hit.sail_no }));
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
      <label className="catalogue-picker">
        {t("Polar catalogue")}
        <select data-feature="orc:catalogue" value={catalogue} onChange={(event) => { setCatalogue(event.target.value as "orc" | "orr"); setPage(0); setResult(null); }}>
          <option value="orc">ORC</option><option value="orr">ORR</option>
        </select>
      </label>
      <input type="search" className="orc-search" value={query} data-feature="orc:search"
        placeholder={t("Name, sail number, model, year…")}
        aria-label={catalogue === "orc" ? t("Search the ORC catalogue") : t("Search the ORR catalogue")}
        title={t("Every word must match a field: name, sail number, country, model, builder, designer or year")}
        onChange={(event) => { setQuery(event.target.value); setPage(0); }} />
      <button type="button" className="orc-fields-toggle" data-feature="orc:fields" aria-expanded={fieldsOpen}
        aria-controls="orc-fields" title={t("Search each field on its own")}
        onClick={() => setFieldsOpen((open) => !open)}>
        <span className="disclose" aria-hidden="true">{fieldsOpen ? "▾" : "▸"}</span>
        {active === 0 ? t("Search by field") : t("Search by field ({count})", { count: active })}
      </button>
      {fieldsOpen && (
        <div className="orc-fields" id="orc-fields" role="group" aria-label={t("Search by field")}>
          {/* Pairs in reading order; each label stays in sight once its box is filled. */}
          <label className="orc-field">
            <span>{t("Boat name")}</span>
            <input type="text" {...field("name")} data-feature="orc:field-name"
              title={t("Every word must start a word of the boat name")} />
          </label>
          <label className="orc-field">
            <span>{t("Sail number")}</span>
            <input type="text" {...field("sail_no")} data-feature="orc:field-sail"
              title={t("With or without the country: GBR1124, GBR 1124 and GBR/1124 are the same")} />
          </label>
          <label className="orc-field">
            <span>{t("Model / type")}</span>
            <input type="text" {...field("model")} data-feature="orc:field-model"
              title={t("Every word must start a word of the model or type")} />
          </label>
          <label className="orc-field">
            <span>{t("Builder")}</span>
            <input type="text" {...field("builder")} data-feature="orc:field-builder"
              title={t("Every word must start a word of the builder")} />
          </label>
          <label className="orc-field">
            <span>{t("Designer")}</span>
            <input type="text" {...field("designer")} data-feature="orc:field-designer"
              title={t("Every word must start a word of the designer")} />
          </label>
          <label className="orc-field">
            <span>{t("Country")}</span>
            <select {...field("country")} data-feature="orc:country" title={t("Only certificates from this country")}>
              <option value="">{t("All countries")}</option>
              {(info?.countries ?? []).map((code) => <option key={code} value={code}>{code}</option>)}
            </select>
          </label>
          <label className="orc-field">
            <span>{t("Built from")}</span>
            <input type="number" inputMode="numeric" {...field("year_from")} data-feature="orc:year-from"
              min={info && "year_min" in info ? info.year_min ?? undefined : undefined} max={info && "year_max" in info ? info.year_max ?? undefined : undefined} title={t("Earliest year built")} />
          </label>
          <label className="orc-field">
            <span>{t("Built until")}</span>
            <input type="number" inputMode="numeric" {...field("year_to")} data-feature="orc:year-to"
              min={info && "year_min" in info ? info.year_min ?? undefined : undefined} max={info && "year_max" in info ? info.year_max ?? undefined : undefined} title={t("Latest year built")} />
          </label>
          <label className="orc-field">
            <span>{t("Certificate year")}</span>
            <input type="text" inputMode="numeric" maxLength={4} {...field("certificate_year")}
              data-feature="orc:field-certificate-year"
              title={t("The certificate year, or its start: 202 finds 2020 to 2029")} />
          </label>
          <fieldset className="orc-measurements">
            <legend>{t("Measurements")}</legend>
            {MEASUREMENTS.map((measurement, k) => <div className="track-range" key={measurement.id}>
              <label className="orc-field">{t(measurement.label)}
                <input type="number" min={0} step="any" value={measureMin[k]} data-feature={`orc-measure:${measurement.minId}`}
                  aria-label={t("Minimum {measurement}", { measurement: t(measurement.label) })}
                  onChange={(event) => changeMeasure(k, event.target.value, false)} />
              </label>
              <label className="orc-field">{t("Maximum")}
                <input type="number" min={0} step="any" value={measureMax[k]} data-feature={`orc-measure:${measurement.maxId}`}
                  aria-label={t("Maximum {measurement}", { measurement: t(measurement.label) })}
                  onChange={(event) => changeMeasure(k, event.target.value, true)} />
              </label>
            </div>)}
            {!measuresValid && <p role="alert">{t("Measurement bounds must be nonnegative and increasing.")}</p>}
          </fieldset>
          <button type="button" className="small" data-feature="orc:fields-clear" disabled={active === 0}
            title={t("Empty every field box; the search box above is kept")}
            onClick={() => { setFields(NO_FIELDS); setMeasureMin(Array(9).fill("")); setMeasureMax(Array(9).fill("")); setPage(0); }}>
            {t("Clear")}
          </button>
        </div>
      )}
      {result !== null && (
        <>
          <p className="muted orc-count">
            {result.total === 0
              ? t("No certificate matches.")
              : result.total > result.hits.length
                ? t("Certificates {first}–{last} of {total}", { first: page * LIMIT + 1, last: page * LIMIT + result.hits.length, total: result.total })
                : result.total === 1 ? t("1 certificate") : t("{total} certificates", { total: result.total })}
          </p>
          {result.total > LIMIT && <nav className="orc-pages" aria-label={t("Result pages")}>
            <button data-feature="orc:previous-page" disabled={page === 0} onClick={() => setPage((p) => p - 1)}>{t("Previous")}</button>
            <span>{t("Page {page} of {pages}", { page: page + 1, pages: Math.ceil(result.total / LIMIT) })}</span>
            <button data-feature="orc:next-page" disabled={(page + 1) * LIMIT >= result.total} onClick={() => setPage((p) => p + 1)}>{t("Next")}</button>
          </nav>}
          <ul className="orc-results" aria-label={t("Search results")}>
            {result.hits.map((hit) => (
              <li key={hit.id}>
                <Thumb hit={hit} />
                <span className="orc-text">
                  <span className="orc-name">{hit.name || hit.sail_no}</span>
                  {"variant" in hit && <span className="muted orc-meta">{hit.variant === "short_course" ? t("Short course") : t("Offshore")}</span>}
                  <span className="muted orc-meta">{meta([hit.sail_no, hit.model, hit.year, hit.builder])}</span>
                  {hit.certificate_year !== null && (
                    <span className="muted orc-meta">{t("Certificate {year}", { year: hit.certificate_year })}</span>
                  )}
                </span>
                <button className="small" data-feature="orc:add" disabled={catalogue === "orr" && hit.in_project} onClick={() => void add(hit, false)}
                  title={catalogue === "orr" ? t("Add this ORR certificate once to the project") : hit.in_project
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
        ? <p className="muted placeholder">{catalogue === "orc" ? t("No ORC polars in this project yet.") : t("No ORR polars in this project yet.")}</p>
        : <ul className="polar-file-list orc-added" aria-label={catalogue === "orc" ? t("ORC polars in the project") : t("ORR polars in the project")}>
          {added.map((source) => (
            <li key={source.id}>
              <span className="swatch" style={{ backgroundColor: source.colour }} aria-hidden="true" />
              <span className="polar-file-text">
                <span className="polar-file-label">{source.label}</span>
                <span className="muted polar-file-meta">
                  {meta([(source.orc ?? source.orr)!.sail_no, (source.orc ?? source.orr)!.model, (source.orc ?? source.orr)!.year])}
                  {(source.orc ?? source.orr)!.certificate_year !== null
                    && ` · ${t("Certificate {year}", { year: (source.orc ?? source.orr)!.certificate_year! })}`}
                </span>
              </span>
              <button className="icon-button" data-feature="orc:remove" onClick={() => remove(source.id)}
                title={catalogue === "orc" ? t("Remove this ORC polar from the project (undoable)") : t("Remove this ORR polar from the project (undoable)")} aria-label={t("Remove")}>
                ✕
              </button>
            </li>
          ))}
        </ul>}
      {info !== null && (
        <p className="muted orc-provenance" title={"commit" in info ? info.commit : "RegattaMan"}>
          {"commit_date" in info ? t("Catalogue: {records} certificates from jieter/orc-data of {date}", {
            records: info.records, date: info.commit_date,
          }) : t("ORR catalogue: {records} polar variants. Refresh it in Settings.", { records: info.records })}
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
