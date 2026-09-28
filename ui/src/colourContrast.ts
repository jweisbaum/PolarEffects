/**
 * Whether a stored colour can be seen on the theme's background (plan.md
 * M14 review). The blend's colour is the person's choice and saved in the
 * project; a white blend saved before M14 vanishes on the light Paper
 * theme. Rather than change what is stored, the views draw a contrasting
 * outline around a colour too close to the background.
 */

/** The WCAG relative luminance of `#rrggbb`, `#rgb` or `rgb(a)(r, g, b…)`; null if unreadable. */
export function luminance(colour: string): number | null {
  const rgb = parse(colour.trim());
  if (!rgb) return null;
  const [r, g, b] = rgb.map((c) => {
    const x = c / 255;
    return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function parse(colour: string): [number, number, number] | null {
  let hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(colour)?.[1];
  if (hex) {
    if (hex.length === 3) hex = [...hex].map((c) => c + c).join("");
    return [0, 2, 4].map((k) => parseInt(hex!.slice(k, k + 2), 16)) as [number, number, number];
  }
  const rgb = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/i.exec(colour);
  return rgb ? [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])] : null;
}

/** The WCAG contrast ratio of two colours, 1 to 21; 21 when either is unreadable (nothing to fix). */
export function contrastRatio(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  if (la === null || lb === null) return 21;
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** Below this a line of the colour is hard to see on the background. */
export const MIN_CONTRAST = 2;

/** Whether `colour` needs an outline to be seen on `background`. */
export function needsOutline(colour: string, background: string): boolean {
  return contrastRatio(colour, background) < MIN_CONTRAST;
}

/** A theme token's value now, or `fallback` outside a document. */
export function themeColour(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}
