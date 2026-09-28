import { useEffect, useRef, useState } from "react";

import { reportFailure } from "../errors";
import type { EnvEstimate } from "../generated/EnvEstimate";
import { useT } from "../i18n";
import { api } from "../ipc";
import { formatBytes } from "./trackImport";

type Interval = "hourly" | "three_hourly";

/**
 * The pre-flight of an environment fetch (spec.md 7.5, 13, D19): how many
 * samples it covers and what it will download sampling hourly or every
 * three hours, less what the chunk cache already holds. Hourly is offered
 * first unless its download would exceed half the chunk-cache limit.
 *
 * It opens by itself after an import, since importing is what starts the
 * fetch (spec.md 7.5), and from Refetch environment. Its two interval
 * choices are tagged and registered, landing on Refetch environment; its
 * answer buttons (Not now, Fetch) are not, as in every transient dialog
 * (spec.md 3.6).
 */
export default function EnvFetchDialog({ sourceIds, restart, onClose }: {
  sourceIds: number[];
  /** Fetch every sample again, not only the missing ones. */
  restart: boolean;
  onClose: () => void;
}) {
  const t = useT();
  const [estimate, setEstimate] = useState<EnvEstimate | null>(null);
  const [choice, setChoice] = useState<Interval>("hourly");
  const [busy, setBusy] = useState(false);
  const close = useRef(onClose);
  close.current = onClose;
  const fetchButton = useRef<HTMLButtonElement>(null);

  // Fetch is the default answer, but it is disabled until the estimate
  // arrives, and a disabled button takes no autofocus: focus it then.
  useEffect(() => {
    if (estimate !== null) fetchButton.current?.focus();
  }, [estimate]);

  // Asked once, when the dialog opens.
  useEffect(() => {
    let live = true;
    api.envEstimate(sourceIds, restart)
      .then((e) => {
        if (!live) return;
        setEstimate(e);
        setChoice(e.recommended === "three_hourly" ? "three_hourly" : "hourly");
      })
      .catch((error) => { reportFailure(error); close.current(); });
    return () => { live = false; };
  }, [sourceIds, restart]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); onClose(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const start = async () => {
    setBusy(true);
    try {
      await api.startEnvFetch(sourceIds, choice, restart);
      onClose();
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  const title = t("Fetch wind, waves and current");
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal-narrow env-fetch" role="dialog" aria-label={title} onClick={(e) => e.stopPropagation()}>
        <h2>{title}</h2>
        {estimate === null
          ? <p className="muted">{t("Estimating the download")}</p>
          : <>
            <p className="modal-summary">
              {t("Reanalysis wind, waves and current for {count} samples, from the archives named in Help.", { count: estimate.samples })}
            </p>
            <fieldset className="env-fetch-interval">
              <legend>{t("Wind and wave sampling")}</legend>
              <label>
                <input type="radio" name="env-interval" data-feature="env-fetch:hourly" checked={choice === "hourly"}
                  onChange={() => setChoice("hourly")} />
                {t("Hourly: about {size} to download", { size: formatBytes(estimate.hourly_bytes) })}
              </label>
              <label>
                <input type="radio" name="env-interval" data-feature="env-fetch:three-hourly" checked={choice === "three_hourly"}
                  onChange={() => setChoice("three_hourly")} />
                {t("Every 3 hours: about {size} to download", { size: formatBytes(estimate.three_hourly_bytes) })}
              </label>
            </fieldset>
            {estimate.cached_bytes > 0 && (
              <p className="muted">{t("{size} of it is already in the chunk cache.", { size: formatBytes(estimate.cached_bytes) })}</p>
            )}
            {estimate.recommended === "three_hourly" && (
              <p className="muted">{t("Hourly would fill more than half the chunk cache ({limit}), so every 3 hours is chosen.", { limit: formatBytes(estimate.cache_limit_bytes) })}</p>
            )}
            <p className="muted">{t("It runs in the background; the status bar shows its progress and can cancel it. Samples already fetched are kept.")}</p>
          </>}
        <div className="modal-actions">
          <button onClick={onClose} title={t("Fetch nothing now; Refetch environment starts it later")}>{t("Not now")}</button>
          <span className="spacer" />
          <button className="primary" ref={fetchButton} disabled={busy || estimate === null || estimate.samples === 0}
            title={t("Start the fetch in the background")} onClick={() => void start()}>
            {t("Fetch")}
          </button>
        </div>
      </div>
    </div>
  );
}
