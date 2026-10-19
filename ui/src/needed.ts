import type { GoalHit, Spot } from "./types";

/**
 * What a spot would add to the log: 0 something new (an entity, zone, prefix..., or a Marathon
 * country or zone this year), 1 a new band, 2 a new mode, 3 a confirmation still missing. null when nothing.
 */
export type NeedRank = 0 | 1 | 2 | 3;

const MODE_COLS = ["cw", "phone", "digital"];

export function hitRank(h: GoalHit): NeedRank {
  if (h.status === "worked") return 3;
  if (h.column === "mixed") return 0;
  return MODE_COLS.includes(h.column) ? 2 : 1;
}

/** The award goals decide when the spot has them; before the log's awards are counted, the plain DXCC flags do. */
export function needRank(s: Pick<Spot, "needed" | "goals" | "marathon">): NeedRank | null {
  if (s.marathon) return 0;
  if (s.goals) return s.goals.length ? hitRank(s.goals[0]) : null;
  const n = s.needed;
  if (!n) return null;
  if (n.new_dxcc) return 0;
  if (n.new_band) return 1;
  if (n.new_mode) return 2;
  return null;
}

export const NEED_NAME = ["New", "New band", "New mode", "Unconfirmed"] as const;

const AWARD_NEW: Record<string, (key: string) => string> = {
  dxcc: () => "New DXCC",
  waz: (k) => `New zone ${k}`,
  wpx: (k) => `New prefix ${k}`,
  wac: (k) => `New continent ${k}`,
  itu: (k) => `New ITU zone ${k}`,
  iota: (k) => `New IOTA ${k}`,
};
const AWARD_NAME: Record<string, string> = { dxcc: "DXCC", waz: "WAZ", wpx: "WPX", wac: "WAC", itu: "ITU", iota: "IOTA" };

/** A short label for one goal hit, like "New zone 25", "New band (WAZ)" or "Unconfirmed DXCC 20m". */
export function hitLabel(h: GoalHit): string {
  const award = AWARD_NAME[h.award] ?? h.award.toUpperCase();
  const what = h.award === "dxcc" ? "" : ` (${award})`;
  switch (hitRank(h)) {
    case 0: return (AWARD_NEW[h.award] ?? (() => `New ${award}`))(h.key);
    case 1: return `New band${what}`;
    case 2: return `New mode${what}`;
    default: return `Unconfirmed ${award}${h.column === "mixed" ? "" : ` ${h.column}`}`;
  }
}

/** Labels for everything a spot is needed for, most important first (at most one per award and kind). */
export function needLabels(s: Pick<Spot, "needed" | "goals" | "marathon">): string[] {
  const out: string[] = [];
  if (s.marathon) out.push(s.marathon.country && s.marathon.zone ? "Marathon country, zone" : s.marathon.country ? "Marathon country" : "Marathon zone");
  if (s.goals) {
    for (const h of s.goals) {
      const l = hitLabel(h);
      if (!out.includes(l)) out.push(l);
    }
    return out;
  }
  const n = s.needed;
  if (n?.new_dxcc) out.push("New DXCC");
  if (n?.new_band) out.push("New band");
  if (n?.new_mode) out.push("New mode");
  return out;
}

/** The label for the most important thing a spot is needed for. */
export const needLabel = (s: Pick<Spot, "needed" | "goals" | "marathon">) => needLabels(s)[0] ?? "";

/** The same call on the same band and mode counts as one spot, however often it is spotted. */
export const spotKey = (s: Pick<Spot, "call" | "band" | "mode">) => `${s.call}|${s.band ?? ""}|${s.mode}`;

export interface NeededAlertConfig {
  sound: boolean;
  popup: boolean;
  /** How much to alert on: only new entities, or also new bands, or also new modes. */
  level: NeedRank;
  /** Minutes before the same call, band and mode may alert again. */
  repeatMin: number;
  /** Minutes a spot stays on the Needed now list. */
  listMin: number;
}

/**
 * Remembers which spots have already alerted. Pure, so it can be tested: returns true when the
 * spot should alert now (and records it), false when it is not needed enough or alerted recently.
 */
export class AlertGate {
  private seen = new Map<string, number>();
  check(s: Spot, cfg: NeededAlertConfig, nowSec: number): boolean {
    const rank = needRank(s);
    if (rank === null || rank > cfg.level) return false;
    const key = spotKey(s);
    const last = this.seen.get(key);
    if (last !== undefined && nowSec - last < cfg.repeatMin * 60) return false;
    this.seen.set(key, nowSec);
    if (this.seen.size > 2000) for (const [k, t] of this.seen) if (nowSec - t > cfg.repeatMin * 60) this.seen.delete(k);
    return true;
  }
}

export interface NeededRow { rank: NeedRank; /** Club Log Most Wanted rank, Infinity when unknown. */ wanted: number; received: number }

/** New entity, band, mode; then the most wanted entity (unranked last); then the newest spot. */
export const compareNeeded = (a: NeededRow, b: NeededRow) =>
  a.rank - b.rank || (a.wanted === b.wanted ? 0 : a.wanted < b.wanted ? -1 : 1) || b.received - a.received;
