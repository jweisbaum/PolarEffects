import { useT } from "../i18n";

/** A stage whose view arrives in a later milestone: 3D (M7) and Compare (M15). */
export default function Placeholder({ title, body }: { title: string; body: string }) {
  const t = useT();
  return (
    <div className="stage-placeholder">
      <h2>{t(title)}</h2>
      <p className="muted">{t(body)}</p>
    </div>
  );
}
