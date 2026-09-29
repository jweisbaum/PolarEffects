import { useEffect, useRef, useState } from "react";

import { reportFailure } from "../errors";
import type { EnvEstimate } from "../generated/EnvEstimate";
import { useT } from "../i18n";
import { api } from "../ipc";
import { formatBytes } from "./trackImport";

type Interval = "hourly" | "three_hourly";

/**
 * The pre-flight of an environment fetch (spec.md 7.5, 13, D19, D27): how
 * many samples it covers, what it will download sampling hourly or every
 * three hours (only the blocks of the archives' fields that hold the
 * track, less what this session already downloaded), and about how much
 * the project grows by. Hourly is offered first unless its download would
 * exceed 1 GB (a long ocean race).
 *
 * It opens only when the user asks for weather: Fetch weather… on a track,
 * or Fetch weather for selected tracks… (never by itself after an import,
 * D24). It shows at once; the estimate (which opens the chunk cache) runs
 * off the UI thread and reads "calculating…" until it arrives. Its two
 * interval choices are tagged and registered, landing on Fetch weather…;
 * its answer buttons (Not now, Fetch) are not, as in every transient
 * dialog (spec.md 3.6).
 */
export default function EnvFetchDialog({ sourceIds, restart, onClose, onStarted }: {
  sourceIds: number[];
  /** Fetch every sample again, not only the missing ones. */
  restart: boolean;
  onClose: () => void;
  /** Once the fetch has started: Fetch weather for selected tracks… clears the ticks (M17a). */
  onStarted?: (() => void) | undefined;
}) {
  const t = useT();
  const [estimate, setEstimate] = useState<EnvEstimate | null>(null);
  const [choice, setChoice] = useState<Interval>("hourly");
  const [busy, setBusy] = useState(false);
  // A choice made while the estimate is calculated is kept when it arrives,
  // and once the user has chosen, the note saying why 3-hourly was
  // preselected no longer applies (M17a).
  const chosen = useRef(false);
  const [userChose, setUserChose] = useState(false);
  const choose = (interval: Interval) => { chosen.current = true; setUserChose(true); setChoice(interval); };
  const close = useRef(onClose);
  close.current = onClose;
  const fetchButton = useRef<HTMLButtonElement>(null);

  // Fetch is the default answer, but it is disabled until the estimate
  // arrives, and a disabled button takes no autofocus: focus it then.
  useEffect(() => {
    if (estimate !== null) fetchButton.current?.focus();
  }, [estimate]);

  // Asked once, when the dialog opens; the dialog is already showing, and
  // the choices stay usable while it is calculated.
  useEffect(() => {
    let live = true;
    api.envEstimate(sourceIds, restart)
      .then((e) => {
        if (!live) return;
        setEstimate(e);
        if (!chosen.current) setChoice(e.recommended === "three_hourly" ? "three_hourly" : "hourly");
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
      onStarted?.();
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
        <p className="modal-summary">
          {estimate === null
            ? t("Reanalysis wind, waves and current for the chosen tracks, from the archives named in Help.")
            : t("Reanalysis wind, waves and current for {count} samples, from the archives named in Help.", { count: estimate.samples })}
        </p>
        <fieldset className="env-fetch-interval">
          <legend>{t("Wind and wave sampling")}</legend>
          <label>
            <input type="radio" name="env-interval" data-feature="env-fetch:hourly" checked={choice === "hourly"}
              onChange={() => choose("hourly")} />
            {estimate === null
              ? t("Hourly: calculating the download…")
              : t("Hourly: about {size} to download", { size: formatBytes(estimate.hourly_bytes) })}
          </label>
          <label>
            <input type="radio" name="env-interval" data-feature="env-fetch:three-hourly" checked={choice === "three_hourly"}
              onChange={() => choose("three_hourly")} />
            {estimate === null
              ? t("Every 3 hours: calculating the download…")
              : t("Every 3 hours: about {size} to download", { size: formatBytes(estimate.three_hourly_bytes) })}
          </label>
        </fieldset>
        {estimate !== null && (
          <p className="muted">{t("Stored in the project: about {size}, the wind, waves and current at each sample.", { size: formatBytes(estimate.stored_bytes) })}</p>
        )}
        {estimate !== null && estimate.cached_bytes > 0 && (
          <p className="muted">{t("{size} of it was already downloaded this session.", { size: formatBytes(estimate.cached_bytes) })}</p>
        )}
        {estimate !== null && estimate.recommended === "three_hourly" && !userChose && (
          <p className="muted">{t("Hourly would download more than {limit}, so every 3 hours is chosen.", { limit: formatBytes(estimate.three_hourly_above_bytes) })}</p>
        )}
        <p className="muted">{t("It runs in the background; the status bar shows its progress and can cancel it. Samples already fetched are kept.")}</p>
        <div className="modal-actions">
          <button onClick={onClose} title={t("Fetch nothing now; Fetch weather… on a track starts it later")}>{t("Not now")}</button>
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
