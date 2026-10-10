import { awardsOff } from "./awardsPref";
import { localGet } from "./prefs";
import type { NewConfirm, QslDownload } from "./types";

const ORDER = ["dxcc", "was", "waz", "wpx", "wac", "itu", "vucc", "iota", "counties"];
const NAME: Record<string, string> = { dxcc: "DXCC", was: "WAS", waz: "WAZ", wpx: "WPX", wac: "WAC", itu: "ITU", vucc: "VUCC", iota: "IOTA", counties: "USA-CA" };
/** What a first confirmation of the whole row (the "mixed" cell) is called for each award. */
const NEW: Record<string, string> = { dxcc: "new entity", was: "new state", waz: "new zone", wpx: "new prefix", wac: "new continent", itu: "new zone", vucc: "new grid", iota: "new island group", counties: "new county" };
const MODES = ["cw", "phone", "digital"];

export interface NewLine { award: string; awardName: string; name: string; what: string }

/**
 * What a download counted toward the awards: cells the service confirmed first, under the
 * confirmation sources the Awards pane counts, leaving out awards switched off in Settings.
 */
export function newConfirmLines(d: QslDownload, service: "lotw" | "eqsl"): NewLine[] {
  const c = localGet<{ lotw: boolean; paper: boolean; eqsl: boolean }>("qrzero.awards", { lotw: true, paper: true, eqsl: false });
  if (!c[service]) return [];
  const off = awardsOff();
  const rows = new Map<string, { n: NewConfirm; cols: string[] }>();
  for (const n of d.new_awards ?? []) {
    if (off.includes(n.award) || (n.before.lotw && c.lotw) || (n.before.paper && c.paper) || (n.before.eqsl && c.eqsl)) continue;
    const k = `${n.award}|${n.key}`;
    const r = rows.get(k) ?? { n, cols: [] };
    r.cols.push(n.column);
    rows.set(k, r);
  }
  const lines = [...rows.values()].map(({ n, cols }) => {
    // Bands first, then the modes.
    const rest = cols.filter((x) => x !== "mixed").sort((a, b) => Number(MODES.includes(a)) - Number(MODES.includes(b)));
    const where = rest.map((x) => (MODES.includes(x) ? x.toUpperCase() : x)).join(", ");
    const what = cols.includes("mixed") ? (where ? `${NEW[n.award] ?? "new"}; ${where}` : (NEW[n.award] ?? "new")) : `new on ${where}`;
    return { award: n.award, awardName: NAME[n.award] ?? n.award, name: n.name, what };
  });
  return lines.sort((a, b) => ORDER.indexOf(a.award) - ORDER.indexOf(b.award) || a.name.localeCompare(b.name));
}
