import { useBoatApi } from "../boats/context";
import { listen } from "@tauri-apps/api/event";
import type { OrrHit } from "../generated/OrrHit";
import type { OrrCatalogueInfo } from "../generated/OrrCatalogueInfo";
import { MEASUREMENTS, measurementBounds } from "./measurements";
import { useEffect, useRef, useState } from "react";

import { reportFailure } from "../errors";
import type { OrcCatalogueInfo } from "../generated/OrcCatalogueInfo";
import type { OrcHit } from "../generated/OrcHit";
import type { OrcFilters } from "../generated/OrcFilters";
type SearchHit = OrcHit | OrrHit;
type SearchResult = { total: number; hits: SearchHit[] };
/** Both catalogues' answers to one search, page by page. */
interface Results {
  /** Each catalogue's total, and its hits in the pages loaded so far. */
  orc: SearchResult;
  orr: SearchResult;
  /** Pages loaded (each one page of either catalogue that still had some). */
  pages: number;
}
import type { ProjectSummary } from "../generated/ProjectSummary";
import { useBoatReveal } from "../boats/context";
import { later, setHint } from "../hint";
import { msg, useT } from "../i18n";
import { IpcError, ORC_UPDATED, ORR_UPDATED } from "../ipc";
import ConfirmDialog from "../project/ConfirmDialog";
import { activeCount, type FieldQueries, loadFieldsOpen, NO_FIELDS, saveFieldsOpen, toFilters } from "./orcFields";
import { THUMB_HEIGHT, THUMB_WIDTH, thumbPaths } from "./orcThumb";
import { interleave } from "./catalogueSearch";

/** How many results a page of either catalogue holds. */
const LIMIT = 50;

/** The fields a result or an added certificate shows under its name. */
function meta(parts: (string | number | null | undefined)[]): string {
  return parts.filter((part) => part !== null && part !== undefined && part !== "").join(" · ");
}

/** An ORR hit is told from an ORC one by its string id. */
function isOrr(hit: SearchHit): hit is OrrHit {
  return typeof hit.id === "string";
}

/** Which catalogue a certificate came from, as a small badge. */
function Badge({ orr }: { orr: boolean }) {
  return <span className={orr ? "catalogue-badge orr" : "catalogue-badge orc"}>{orr ? "ORR" : "ORC"}</span>;
}

/**
 * Watches the end of a scrolling list: `onMore` when it comes into view,
 * so the next page loads as the person scrolls. Where there is no
 * IntersectionObserver (tests), the sentinel stays a button.
 */
function useLoadMore(onMore: () => void, armed: boolean) {
  const ref = useRef<HTMLLIElement | null>(null);
  const callback = useRef(onMore);
  callback.current = onMore;
  useEffect(() => {
    const element = ref.current;
    if (!armed || !element || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) callback.current();
    }, { root: element.parentElement, rootMargin: "80px" });
    observer.observe(element);
    return () => observer.disconnect();
  }, [armed]);
  return ref;
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
 * The ORC / ORR polars section of the left navigation (spec.md 5): one
 * search box over both catalogues at once, results updated as you type with
 * a thumbnail and a catalogue badge each, the next page loading as the list
 * is scrolled; under it "Search by field", one box per field (year built
 * and country among them) and the measurements, folded away until wanted;
 * Add (asks first when the project already holds that certificate), and the
 * certificates already added, each with its colour and Remove (undoable).
 */
export default function OrcPolars({ project, onProject }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
}) {
  const api = useBoatApi();
  const onReveal = useBoatReveal();
  const t = useT();
  const [info, setInfo] = useState<OrcCatalogueInfo | null>(null);
  const [orrInfo, setOrrInfo] = useState<OrrCatalogueInfo | null>(null);
  const [catalogueRevision, setCatalogueRevision] = useState(0);
  const [query, setQuery] = useState("");
  const [measuresOpen, setMeasuresOpen] = useState(false);
  const [measureMin, setMeasureMin] = useState<string[]>(Array(MEASUREMENTS.length).fill(""));
  const [measureMax, setMeasureMax] = useState<string[]>(Array(MEASUREMENTS.length).fill(""));
  const sizeMin = measurementBounds(measureMin), sizeMax = measurementBounds(measureMax);
  const measuresValid = sizeMin !== null && sizeMax !== null && sizeMin.every((min, k) => min === null || sizeMax[k] === null || min <= sizeMax[k]!);
  const measurementCount = measureMin.filter((min, k) => min !== "" || measureMax[k] !== "").length;
  const changeMeasure = (index: number, value: string, max: boolean) => {
    (max ? setMeasureMax : setMeasureMin)((old) => old.map((v, k) => k === index ? value : v));
  };
  const [fields, setFields] = useState<FieldQueries>(NO_FIELDS);
  const [fieldsOpen, setFieldsOpen] = useState(loadFieldsOpen);
  const [result, setResult] = useState<Results | null>(null);
  const [loading, setLoading] = useState(false);
  const [again, setAgain] = useState<SearchHit | null>(null);
  const latest = useRef(0);
  const added = project.sources.filter((source) => source.orc !== null || source.orr !== null);
  const listing = useRef(false);
  listing.current = result !== null && result.orc.hits.length + result.orr.hits.length > 0;
  const newest = useRef<number | null>(null);
  newest.current = info?.year_max ?? null;

  useEffect(() => {
    let active = true;
    setInfo(null);
    setOrrInfo(null);
    void api.orcCatalogueInfo().then((info) => { if (active) setInfo(info); }).catch(reportFailure);
    void api.orrCatalogueInfo().then((info) => { if (active) setOrrInfo(info); }).catch(reportFailure);
    return () => { active = false; };
  }, [catalogueRevision]);
  useEffect(() => {
    // Either catalogue changed under the list: a scrape finished.
    const refresh = () => setCatalogueRevision((r) => r + 1);
    const unlisten = [listen(ORR_UPDATED, refresh), listen(ORC_UPDATED, refresh)];
    return () => { for (const pending of unlisten) void pending.then((stop) => stop()); };
  }, []);

  // The search's reveal step for Add (spec.md 3.6): with nothing listed,
  // list the newest boats, so there is an Add button to point at.
  useEffect(() => onReveal("orc:results", () => {
    if (listing.current) return;
    setMeasureMin(Array(MEASUREMENTS.length).fill("")); setMeasureMax(Array(MEASUREMENTS.length).fill(""));
    setQuery("");
    setFields({ ...NO_FIELDS, year_from: String(newest.current ?? new Date().getFullYear()) });
  }), []);

  // The reveal step of every per-field box: unfold "Search by field".
  useEffect(() => onReveal("orc:fields", () => setFieldsOpen(true)), []);
  useEffect(() => onReveal("orc:measurements", () => { setFieldsOpen(true); setMeasuresOpen(true); }), []);
  useEffect(() => saveFieldsOpen(fieldsOpen), [fieldsOpen]);

  /** One page of each catalogue that still has some, appended to what is shown. */
  const load = (page: number, ticket: number, before: Results | null) => {
    const filters = { ...toFilters(fields), size_min: measurementBounds(measureMin)!, size_max: measurementBounds(measureMax)! };
    const offset = page * LIMIT;
    const ask = (more: boolean, search: (query: string, filters: OrcFilters, limit: number, offset: number) => Promise<SearchResult>) =>
      more ? search(query, filters, LIMIT, offset) : Promise.resolve<SearchResult>({ total: 0, hits: [] });
    const wanted = (held: SearchResult | undefined) => page === 0 || (held !== undefined && held.hits.length < held.total);
    setLoading(true);
    Promise.all([ask(wanted(before?.orc), api.orcSearch), ask(wanted(before?.orr), api.orrSearch)])
      .then(([orc, orr]) => {
        if (ticket !== latest.current) return;
        setResult(page === 0 ? { orc, orr, pages: 1 } : {
          orc: { total: before?.orc.total ?? 0, hits: [...(before?.orc.hits ?? []), ...orc.hits] },
          orr: { total: before?.orr.total ?? 0, hits: [...(before?.orr.hits ?? []), ...orr.hits] },
          pages: page + 1,
        });
        setLoading(false);
      })
      .catch((error) => { if (ticket === latest.current) setLoading(false); reportFailure(error); });
  };

  // Every keystroke searches both catalogues; only the newest answer is shown.
  const active = activeCount(fields) + measurementCount;
  useEffect(() => {
    if (!measuresValid || (query.trim() === "" && active === 0)) {
      latest.current += 1;
      setResult(null);
      setLoading(false);
      return;
    }
    const ticket = ++latest.current;
    setResult(null);
    load(0, ticket, null);
  }, [query, fields, active, project.revision, measureMin, measureMax, measuresValid, catalogueRevision]);
  const shown = result === null ? 0 : result.orc.hits.length + result.orr.hits.length;
  const total = result === null ? 0 : result.orc.total + result.orr.total;
  const more = result !== null && shown < total;
  const loadMore = () => { if (result !== null && more && !loading) load(result.pages, latest.current, result); };
  const sentinel = useLoadMore(loadMore, more && !loading);

  const field = (key: keyof FieldQueries) => ({
    value: fields[key],
    onChange: (event: { target: { value: string } }) => {
      const value = event.target.value;
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
      <input type="search" className="orc-search" value={query} data-feature="orc:search"
        placeholder={t("Name, sail number, model, year…")}
        aria-label={t("Search the ORC and ORR catalogues")}
        title={t("Every word must match a field: name, sail number, country, model, builder, designer or year; both catalogues are searched")}
        onChange={(event) => setQuery(event.target.value)} />
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
          <div className="orc-measurements">
            {/* Folded until wanted (asked 2026-10-02): nine rows of bounds are a lot to scroll past. */}
            <button type="button" className="orc-fields-toggle" data-feature="orc:measurements" aria-expanded={measuresOpen}
              aria-controls="orc-measurements" title={t("Bound the search by the boat's measurements")}
              onClick={() => setMeasuresOpen((open) => !open)}>
              <span className="disclose" aria-hidden="true">{measuresOpen ? "▾" : "▸"}</span>
              {measurementCount === 0 ? t("Measurements") : t("Measurements ({count})", { count: measurementCount })}
            </button>
            {measuresOpen && (
              <div id="orc-measurements" role="group" aria-label={t("Measurements")}>
                <div className="orc-measure orc-measure-head" aria-hidden="true">
                  <span /><span>{t("Min")}</span><span>{t("Max")}</span>
                </div>
                {MEASUREMENTS.map((measurement, k) => <div className="orc-measure" key={measurement.id}>
                  <span className="orc-measure-name">{t(measurement.label)}</span>
                  <input type="number" min={0} step="any" value={measureMin[k]} data-feature={`orc-measure:${measurement.minId}`}
                    aria-label={t("Minimum {measurement}", { measurement: t(measurement.label) })} placeholder={t("Min")}
                    onChange={(event) => changeMeasure(k, event.target.value, false)} />
                  <input type="number" min={0} step="any" value={measureMax[k]} data-feature={`orc-measure:${measurement.maxId}`}
                    aria-label={t("Maximum {measurement}", { measurement: t(measurement.label) })} placeholder={t("Max")}
                    onChange={(event) => changeMeasure(k, event.target.value, true)} />
                </div>)}
                {!measuresValid && <p role="alert">{t("Measurement bounds must be nonnegative and increasing.")}</p>}
              </div>
            )}
          </div>
          <button type="button" className="small" data-feature="orc:fields-clear" disabled={active === 0}
            title={t("Empty every field box; the search box above is kept")}
            onClick={() => { setFields(NO_FIELDS); setMeasureMin(Array(MEASUREMENTS.length).fill("")); setMeasureMax(Array(MEASUREMENTS.length).fill("")); }}>
            {t("Clear")}
          </button>
        </div>
      )}
      {result !== null && (
        <>
          <p className="muted orc-count">
            {total === 0
              ? t("No certificate matches.")
              : total > shown
                ? t("Certificates 1–{last} of {total}", { last: shown, total })
                : total === 1 ? t("1 certificate") : t("{total} certificates", { total })}
          </p>
          <ul className="orc-results" aria-label={t("Search results")}>
            {interleave<SearchHit>(result.orc.hits, result.orr.hits).map((hit) => (
              <li key={hit.id}>
                <Thumb hit={hit} />
                <span className="orc-text">
                  <span className="orc-name"><Badge orr={isOrr(hit)} />{hit.name || hit.sail_no}</span>
                  {"variant" in hit && <span className="muted orc-meta">{hit.variant === "short_course" ? t("Short course") : t("Offshore")}</span>}
                  <span className="muted orc-meta">{meta([hit.sail_no, hit.model, hit.year, hit.builder])}</span>
                  {hit.certificate_year !== null && (
                    <span className="muted orc-meta">
                      {t("Certificate {year}", { year: hit.certificate_year })}
                      {/* A downloaded certificate's own number: two valid certificates of one boat differ by it. */}
                      {"ref_no" in hit && hit.ref_no !== null && ` · ${hit.ref_no}`}
                    </span>
                  )}
                </span>
                <button className="small" data-feature="orc:add" disabled={isOrr(hit) && hit.in_project} onClick={() => void add(hit, false)}
                  title={isOrr(hit) ? t("Add this ORR certificate once to the project") : hit.in_project
                    ? t("Already in the project; adding it again asks first")
                    : t("Add this certificate to the project as an ORC polar")}>
                  {hit.in_project ? t("Added") : t("Add")}
                </button>
              </li>
            ))}
            {more && (
              <li className="orc-more" ref={sentinel}>
                {/* Reached by scrolling; a button for a window without an IntersectionObserver. */}
                <button type="button" className="small" data-feature="orc:more" disabled={loading} onClick={loadMore}>
                  {loading ? t("Loading more…") : t("More")}
                </button>
              </li>
            )}
          </ul>
        </>
      )}
      {added.length === 0
        ? <p className="muted placeholder">{t("No ORC or ORR polars in this project yet.")}</p>
        : <ul className="polar-file-list orc-added" aria-label={t("ORC and ORR polars in the project")}>
          {added.map((source) => (
            <li key={source.id}>
              <span className="swatch" style={{ backgroundColor: source.colour }} aria-hidden="true" />
              <span className="polar-file-text">
                <span className="polar-file-label"><Badge orr={source.orr !== null} />{source.label}</span>
                <span className="muted polar-file-meta">
                  {meta([(source.orc ?? source.orr)!.sail_no, (source.orc ?? source.orr)!.model, (source.orc ?? source.orr)!.year])}
                  {(source.orc ?? source.orr)!.certificate_year !== null
                    && ` · ${t("Certificate {year}", { year: (source.orc ?? source.orr)!.certificate_year! })}`}
                </span>
              </span>
              <button className="icon-button" data-feature="orc:remove" onClick={() => remove(source.id)}
                title={t("Remove this polar from the project (undoable)")} aria-label={t("Remove")}>
                ✕
              </button>
            </li>
          ))}
        </ul>}
      {info !== null && (
        <p className="muted orc-provenance" title={info.commit}>
          {info.scraped > 0
            // Some of it was downloaded since the build: say so, rather than credit it all to the bundle.
            ? t("Catalogue: {records} certificates: jieter/orc-data of {date}, and {scraped} downloaded from ORC", {
              records: info.records, date: info.commit_date, scraped: info.scraped,
            })
            : t("Catalogue: {records} certificates from jieter/orc-data of {date}", {
              records: info.records, date: info.commit_date,
            })}
        </p>
      )}
      {orrInfo !== null && (
        <p className="muted orc-provenance">
          {t("ORR catalogue: {records} polar variants. Refresh it in Settings.", { records: orrInfo.records })}
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
