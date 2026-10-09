import type { Spot } from "./types";

/** What a spot would add to the log: 0 a new entity, 1 a new band for the entity, 2 a new mode. null when nothing. */
export type NeedRank = 0 | 1 | 2;

export function needRank(s: Pick<Spot, "needed">): NeedRank | null {
  const n = s.needed;
  if (!n) return null;
  if (n.new_dxcc) return 0;
  if (n.new_band) return 1;
  if (n.new_mode) return 2;
  return null;
}

export const NEED_NAME = ["New entity", "New band", "New mode"] as const;

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
