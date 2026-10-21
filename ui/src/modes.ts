import { useSyncExternalStore } from "react";
import { api } from "./api";
import { getDisplay } from "./display";
import { localSet } from "./prefs";
import { adifGroup } from "./adifModes";

export type ModeGroup = "cw" | "phone" | "digital";

/** A mode in the user's list: the label shown in the pickers, how it is stored (ADIF MODE/SUBMODE) and where awards count it. */
export interface ModeChoice { label: string; mode: string; submode?: string; rst: string; group: ModeGroup }

/** The short list offered until the user changes it (Settings, Modes). */
export const DEFAULT_MODES: ModeChoice[] = [
  { label: "CW", mode: "CW", rst: "599", group: "cw" },
  { label: "SSB", mode: "SSB", rst: "59", group: "phone" },
  { label: "AM", mode: "AM", rst: "59", group: "phone" },
  { label: "FM", mode: "FM", rst: "59", group: "phone" },
  { label: "FT8", mode: "FT8", rst: "-10", group: "digital" },
  { label: "FT4", mode: "MFSK", submode: "FT4", rst: "-10", group: "digital" },
  { label: "RTTY", mode: "RTTY", rst: "599", group: "digital" },
  { label: "PSK31", mode: "PSK", submode: "PSK31", rst: "599", group: "digital" },
];

/** The modes offered for entering and editing QSOs. Changed in place by `setModes`, so a screen that shows it calls `useModes()` to redraw. */
export const MODES: ModeChoice[] = [...DEFAULT_MODES];

const PREF = "modes";
const REV_KEY = "qrzero.modes.rev";
const subs = new Set<() => void>();
let rev = 0;

function apply(list: ModeChoice[] | null) {
  MODES.splice(0, MODES.length, ...(list && list.length ? list : DEFAULT_MODES));
  rev++;
  subs.forEach((f) => f());
}

/** Saves the list for every window (null goes back to the default list) and tells the other windows. */
export async function setModes(list: ModeChoice[] | null): Promise<void> {
  await api.setPref(PREF, list);
  apply(list);
  localSet(REV_KEY, Date.now());
}

function load() {
  api.getPref<ModeChoice[]>(PREF).then((l) => Array.isArray(l) && apply(l)).catch(() => {});
}
load();
window.addEventListener("storage", (e) => e.key === REV_KEY && load());

/** Redraws the caller when the mode list changes. */
export function useModes(): ModeChoice[] {
  useSyncExternalStore((f) => { subs.add(f); return () => subs.delete(f); }, () => rev);
  return MODES;
}

export function modeLabel(f: Record<string, string>): string {
  return f.SUBMODE || f.MODE || "";
}

export function choiceFor(label: string): ModeChoice {
  const c = MODES.find((m) => m.label === label) ?? { label, mode: label, rst: "59", group: adifGroup(label) };
  const own = getDisplay().rst[label]?.trim();
  return own ? { ...c, rst: own } : c;
}

export const BANDS: [string, number, number][] = [
  ["160m", 1.8, 2.0], ["80m", 3.5, 4.0], ["60m", 5.06, 5.45], ["40m", 7.0, 7.3], ["30m", 10.1, 10.15],
  ["20m", 14.0, 14.35], ["17m", 18.068, 18.168], ["15m", 21.0, 21.45], ["12m", 24.89, 24.99],
  ["10m", 28.0, 29.7], ["6m", 50.0, 54.0], ["4m", 70.0, 71.0], ["2m", 144.0, 148.0],
  ["1.25m", 222.0, 225.0], ["70cm", 420.0, 450.0], ["33cm", 902.0, 928.0], ["23cm", 1240.0, 1300.0],
  ["2190m", 0.1357, 0.1378], ["630m", 0.472, 0.479],
];

export function bandForFreq(mhz: number): string | undefined {
  return BANDS.find(([, lo, hi]) => mhz >= lo && mhz <= hi)?.[0];
}

const DIGITAL = ["FT8", "FT4", "RTTY", "PSK31", "PSK", "JS8", "Q65", "MSK144", "SSTV", "JT65", "JT9", "FST4", "MFSK", "OLIVIA", "WSPR", "DATA", "DIGI"];
const PHONE = ["SSB", "USB", "LSB", "AM", "FM", "PHONE", "DV"];

/** CW, phone or digital, for filters and the colour of a spot; "" when the mode isn't known. */
export function modeGroup(mode: string): ModeGroup | "" {
  const m = mode.toUpperCase();
  const own = MODES.find((c) => c.label.toUpperCase() === m || (c.submode ?? c.mode).toUpperCase() === m);
  if (own) return own.group;
  if (m === "CW") return "cw";
  if (PHONE.includes(m)) return "phone";
  if (DIGITAL.includes(m)) return "digital";
  return "";
}

export const MODE_GROUP_NAME: Record<ModeGroup, string> = { cw: "CW", phone: "Phone", digital: "Digital" };

/** The rows of an award slot grid, in order. */
export const SLOT_GROUPS = ["CW", "Phone", "Digital"] as const;
export type SlotGroup = (typeof SLOT_GROUPS)[number];
const SLOT_PHONE = ["SSB", "USB", "LSB", "AM", "FM", "DIGITALVOICE", "C4FM", "DSTAR", "DMR", "FREEDV", "M17"];

/** The award slot row a mode counts in, as the server groups it (every other mode is Digital); "" when there is no mode. */
export function slotGroup(mode: string): SlotGroup | "" {
  const m = mode.trim().toUpperCase();
  if (!m) return "";
  const own = MODES.find((c) => c.label.toUpperCase() === m || (c.submode ?? c.mode).toUpperCase() === m);
  if (own) return own.group === "cw" ? "CW" : own.group === "phone" ? "Phone" : "Digital";
  return m === "CW" ? "CW" : SLOT_PHONE.includes(m) ? "Phone" : "Digital";
}
