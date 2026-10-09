import { useSyncExternalStore } from "react";
import { localGet, localSet } from "./prefs";

/** Every award QRZero tracks, in the order the Awards pane lists them. */
export const AWARD_LIST: { key: string; name: string; what: string; group: "main" | "cw" }[] = [
  { key: "dxcc", name: "DXCC", what: "Entities, plus the Challenge and 5BDXCC", group: "main" },
  { key: "was", name: "WAS", what: "The 50 US states", group: "main" },
  { key: "waz", name: "WAZ", what: "The 40 CQ zones", group: "main" },
  { key: "wpx", name: "WPX", what: "Callsign prefixes", group: "main" },
  { key: "wac", name: "WAC", what: "The six continents", group: "main" },
  { key: "itu", name: "ITU", what: "The 90 ITU zones", group: "main" },
  { key: "vucc", name: "VUCC", what: "Grid squares on 6 m and up", group: "main" },
  { key: "iota", name: "IOTA", what: "Island groups", group: "main" },
  { key: "counties", name: "Counties", what: "US counties (USA-CA)", group: "main" },
  { key: "skcc", name: "SKCC", what: "Straight Key Century Club levels (CW)", group: "cw" },
  { key: "cwops", name: "CWops", what: "ACA, CMA, ACMA and CWT medals (CW)", group: "cw" },
  { key: "naqcc", name: "NAQCC", what: "Friendship Club, 200 members (CW)", group: "cw" },
  { key: "fists", name: "FISTS", what: "Century, Silver, Gold and Diamond, and WAS (CW)", group: "cw" },
];

const KEY = "qrzero.awards_off";
const listeners = new Set<() => void>();
let off: string[] = localGet<{ off: string[] }>(KEY, { off: [] }).off ?? [];

/** Awards the operator has hidden. Everything is on until switched off, so awards added later show up too. */
export function awardsOff(): readonly string[] {
  return off;
}

export function setAwardEnabled(key: string, on: boolean) {
  const next = on ? off.filter((k) => k !== key) : off.includes(key) ? off : [...off, key];
  off = next;
  localSet(KEY, { off });
  listeners.forEach((l) => l());
}

export function setAwardsOff(keys: string[]) {
  off = keys;
  localSet(KEY, { off });
  listeners.forEach((l) => l());
}

export function useAwardsOff(): readonly string[] {
  return useSyncExternalStore(
    (cb) => (listeners.add(cb), () => void listeners.delete(cb)),
    awardsOff,
  );
}
