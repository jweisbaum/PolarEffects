/**
 * One list from the ORC and ORR catalogues' answers to one search (spec.md
 * 5.2): each catalogue's hits in its own rank, alternated, so neither is
 * buried under the other's pages; once one runs out the other goes on
 * alone. Both arrive a page at a time and only grow at the end, so the rows
 * already on screen keep their places when the next pages arrive.
 */
export function interleave<T>(orc: readonly T[], orr: readonly T[]): T[] {
  const out: T[] = [];
  for (let k = 0; k < Math.max(orc.length, orr.length); k++) {
    if (k < orc.length) out.push(orc[k]!);
    if (k < orr.length) out.push(orr[k]!);
  }
  return out;
}
