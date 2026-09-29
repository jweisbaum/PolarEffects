import { api } from "../ipc";
import { describeError } from "../errors";
import { later, reportError } from "../hint";
import type { AppSettings } from "../generated/AppSettings";
import { LANGUAGES, language, msg, setLanguage, useLanguage, useT } from "./index";

/**
 * The interface language, on the start screen and in Settings (spec.md 3.5).
 * Copied from VectorEffects. The switch is shown at once and saved behind
 * it, and Rust relabels the native menu as it saves; a save that fails puts
 * the previous language back, so what is on screen is what was saved.
 */
export default function LanguagePicker({ feature, onSettings }: {
  /** The `data-feature` id: the picker appears in two places. */
  feature: string;
  onSettings?: ((settings: AppSettings) => void) | undefined;
}) {
  const t = useT();
  const current = useLanguage();
  return <select className="language-picker" aria-label={t("Language")} data-feature={feature}
    title={t("Interface language")} value={current}
    onChange={event => {
      const before = language();
      const next = event.target.value;
      setLanguage(next);
      api.setLanguage(next).then(settings => onSettings?.(settings)).catch(error => {
        setLanguage(before);
        reportError(later(msg("The language could not be saved.")), describeError(error).detail);
      });
    }}>
    {LANGUAGES.map(({ id, name }) => <option key={id} value={id} lang={id}>{name}</option>)}
  </select>;
}
