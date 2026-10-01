import { useEffect, useState } from "react";

/**
 * A whole number typed in and committed on Enter or on leaving the field,
 * so a half-typed value is never saved.
 */
export default function IntegerField({ value, min, max, onCommit, ...rest }: {
  value: number; min: number; max: number; onCommit: (value: number) => void;
  "data-feature": string; title: string; "aria-label": string;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const parsed = Number(text);
    if (Number.isInteger(parsed) && parsed >= min && parsed <= max) {
      if (parsed !== value) onCommit(parsed);
    } else {
      setText(String(value));
    }
  };
  return <input type="number" inputMode="numeric" min={min} max={max} step={1} value={text} {...rest}
    onChange={(event) => setText(event.target.value)} onBlur={commit}
    onKeyDown={(event) => { if (event.key === "Enter") commit(); }} />;
}
