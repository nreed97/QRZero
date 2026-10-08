// Antenna bands: an antenna's BANDS field is a comma-separated band list ("40m,20m,15m").
import { BANDS } from "./modes";
import type { Equipment, Fields } from "./types";

const ORDER = BANDS.map(([b]) => b);

/** Bands offered in the antenna form: 160m to 70cm. */
export const ANTENNA_BANDS = ORDER.slice(0, ORDER.indexOf("70cm") + 1);

/** The bands in a BANDS field, in band order. Also reads "40 20 15" or "40M/20M". */
export function parseBands(text: string | undefined): string[] {
  const found = new Set<string>();
  for (const t of (text ?? "").toLowerCase().split(/[\s,;/]+/)) {
    const b = ORDER.find((x) => x === t) ?? ORDER.find((x) => x === `${t}m`);
    if (b) found.add(b);
  }
  return ORDER.filter((b) => found.has(b));
}

export function formatBands(bands: string[]): string {
  return ORDER.filter((b) => bands.includes(b)).join(",");
}

/** Short form for the equipment tree: "40 20 15 m", "2 m 70 cm". */
export function shortBands(bands: string[]): string {
  const parts: string[] = [];
  let unit = "";
  for (const b of bands) {
    const u = b.endsWith("cm") ? "cm" : "m";
    if (unit && u !== unit) parts.push(unit);
    parts.push(b.slice(0, -u.length));
    unit = u;
  }
  if (unit) parts.push(unit);
  return parts.join(" ");
}

export const hasBands = (f: Fields) => parseBands(f.BANDS).length > 0;

/**
 * The antenna for a band: one whose bands include it, preferring the one last
 * used on that band, else the first in tree order.
 */
export function antennaForBand(antennas: Equipment[], band: string, recentId?: number): Equipment | undefined {
  const matches = antennas.filter((a) => parseBands(a.fields.BANDS).includes(band));
  return matches.find((a) => a.id === recentId) ?? matches[0];
}
