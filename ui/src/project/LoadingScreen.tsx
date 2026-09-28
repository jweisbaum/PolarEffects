import { useBusy } from "../busy";
import { msg, useT } from "../i18n";

/** The busy labels that mean a project is being opened (see `ipc.ts`). */
const OPENING: readonly string[] = [msg("Opening project"), msg("Recovering project")];

/**
 * The page shown while a project opens. Copied in shape from VectorEffects
 * (spec.md 3.1): it covers everything, and fades in after a moment's delay
 * (in the stylesheet), so a project that opens within a frame or two never
 * flashes a page at anyone.
 *
 * PolarEffects reads a project in one step, so there is no bar: the page
 * says what is happening until the command answers.
 */
export default function LoadingScreen() {
  const t = useT();
  const busy = useBusy();
  const label = busy.labels.find((l) => OPENING.includes(l));
  if (label === undefined) return null;
  return (
    <div className="loading" role="dialog" aria-modal="true" aria-label={t(label)}>
      <div className="loading-panel">
        <p className="muted loading-verb">{t(label)}</p>
        <div className="progress-bar loading-bar indeterminate" role="progressbar">
          <div className="progress-fill" />
        </div>
      </div>
    </div>
  );
}
