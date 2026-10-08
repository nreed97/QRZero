import { useSyncExternalStore } from "react";
import { localGet, localSet } from "./prefs";

/** Everyday display and behaviour options (Settings, General). Stored with the window settings, so pop-out windows follow. */
export interface Display {
  theme: "system" | "dark" | "light";
  fontSize: number;
  dateFormat: "iso" | "dmy" | "mdy";
  localTime: boolean;
  freqUnit: "mhz" | "khz";
  confirmDelete: boolean;
  soundOnLog: boolean;
  /** RST to fill in per mode label, when not the usual one. */
  rst: Record<string, string>;
}

export const DISPLAY_DEFAULTS: Display = {
  theme: "system",
  fontSize: 13,
  dateFormat: "iso",
  localTime: false,
  freqUnit: "mhz",
  confirmDelete: true,
  soundOnLog: false,
  rst: {},
};

const KEY = "qrzero.display";
const subs = new Set<() => void>();
let current: Display = { ...DISPLAY_DEFAULTS, ...localGet<Partial<Display>>(KEY, {}) };

export const getDisplay = () => current;

export function applyDisplay(d: Display = current) {
  const root = document.documentElement;
  if (d.theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", d.theme);
  root.style.setProperty("--fs", `${d.fontSize}px`);
}

export function setDisplay(patch: Partial<Display>) {
  current = { ...current, ...patch };
  localSet(KEY, current);
  applyDisplay();
  subs.forEach((f) => f());
}

/** Another window changed the settings (or the saved copy arrived after start). */
function reload() {
  current = { ...DISPLAY_DEFAULTS, ...localGet<Partial<Display>>(KEY, {}) };
  applyDisplay();
  subs.forEach((f) => f());
}
window.addEventListener("storage", (e) => e.key === KEY && reload());
applyDisplay();

export function useDisplay(): Display {
  return useSyncExternalStore(
    (f) => {
      subs.add(f);
      return () => subs.delete(f);
    },
    getDisplay,
  );
}

/** Ask before deleting, unless that's switched off. */
export function confirmDelete(message: string): boolean {
  return !current.confirmDelete || window.confirm(message);
}

let audio: AudioContext | null = null;
/** A short beep; the context is made on the user's click so browsers allow it. */
export function beep(freq = 880) {
  try {
    audio ??= new AudioContext();
    const osc = audio.createOscillator();
    const gain = audio.createGain();
    osc.type = "sine";
    osc.frequency.value = freq;
    gain.gain.setValueAtTime(0.12, audio.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.001, audio.currentTime + 0.18);
    osc.connect(gain).connect(audio.destination);
    osc.start();
    osc.stop(audio.currentTime + 0.2);
  } catch {
    /* no audio here */
  }
}

/** A frequency in MHz text as shown in the log: MHz as stored, or kHz. */
export function fmtFreq(mhz: string | undefined): string {
  if (!mhz) return "";
  const v = Number(mhz);
  if (current.freqUnit === "mhz" || !Number.isFinite(v)) return mhz;
  return String(Math.round(v * 1e6) / 1e3);
}
