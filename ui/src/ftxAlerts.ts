import { useEffect, useState } from "react";
import { localGet, localSet } from "./prefs";
import type { FtxDecode } from "./types";

/** What makes a decode stand out, most important first. */
export type AlertKind = "toMe" | "watched" | "newDxcc" | "newBand" | "newMode" | "newGrid" | "newCall" | "newCallBand" | "cq";

export interface AlertStyle { on: boolean; color: string }

export interface FtxAlertConfig {
  alerts: Record<AlertKind, AlertStyle>;
  /** Hide stations already worked on this band. */
  hideWorked: boolean;
  /** Hide decodes weaker than this, in dB; null shows them all. */
  minSnr: number | null;
  /** Continents to show (AF AN AS EU NA OC SA); empty shows all. */
  continents: string[];
  /** Calls to leave out, comma separated; a trailing * matches a prefix. */
  ignore: string;
}

export const ALERTS: { kind: AlertKind; name: string; tag: string; help: string }[] = [
  { kind: "toMe", name: "Calling you", tag: "ME", help: "The message has your callsign in it" },
  { kind: "watched", name: "Watch list", tag: "WL", help: "The station is on your watch list" },
  { kind: "newDxcc", name: "New DXCC", tag: "DXCC", help: "A country you have never worked" },
  { kind: "newBand", name: "New band", tag: "BAND", help: "A country you have not worked on this band" },
  { kind: "newMode", name: "New mode", tag: "MODE", help: "A country you have not worked in this mode" },
  { kind: "newGrid", name: "New grid", tag: "GRID", help: "A grid square you have not worked on this band (6 m and up)" },
  { kind: "newCall", name: "New call", tag: "NEW", help: "A callsign you have never worked" },
  { kind: "newCallBand", name: "New call on band", tag: "NB", help: "A callsign you have worked, but not on this band" },
  { kind: "cq", name: "Calling CQ", tag: "CQ", help: "The station is calling CQ" },
];

export const CONTINENTS = ["AF", "AN", "AS", "EU", "NA", "OC", "SA"];

export const DEFAULT_ALERTS: FtxAlertConfig = {
  alerts: {
    toMe: { on: true, color: "#e5534b" },
    watched: { on: true, color: "#c88be0" },
    newDxcc: { on: true, color: "#e8a33d" },
    newBand: { on: true, color: "#d8c25a" },
    newMode: { on: true, color: "#4fc4c4" },
    newGrid: { on: true, color: "#8fb0f0" },
    newCall: { on: false, color: "#6cc28a" },
    newCallBand: { on: false, color: "#5fa8e8" },
    cq: { on: true, color: "#6cc28a" },
  },
  hideWorked: false,
  minSnr: null,
  continents: [],
  ignore: "",
};

const KEY = "qrzero.ftx.alerts";

function load(): FtxAlertConfig {
  const saved = localGet<Partial<FtxAlertConfig>>(KEY, {});
  return {
    ...DEFAULT_ALERTS,
    ...saved,
    alerts: Object.fromEntries(
      ALERTS.map(({ kind }) => [kind, { ...DEFAULT_ALERTS.alerts[kind], ...saved.alerts?.[kind] }]),
    ) as FtxAlertConfig["alerts"],
  };
}

const EVENT = "qrzero-ftx-alerts";

/** The FTx alert settings, kept in step across the main window and pop-outs. */
export function useFtxAlerts(): [FtxAlertConfig, (c: FtxAlertConfig) => void] {
  const [cfg, setCfg] = useState(load);
  useEffect(() => {
    const reload = () => setCfg(load());
    const onStorage = (e: StorageEvent) => e.key === KEY && reload();
    window.addEventListener(EVENT, reload);
    window.addEventListener("storage", onStorage);
    return () => {
      window.removeEventListener(EVENT, reload);
      window.removeEventListener("storage", onStorage);
    };
  }, []);
  const save = (c: FtxAlertConfig) => {
    localSet(KEY, c);
    window.dispatchEvent(new Event(EVENT));
  };
  return [cfg, save];
}

/** Every alert that applies to a decode, most important first, whether switched on or not. */
export function alertsOf(d: FtxDecode): AlertKind[] {
  const out: AlertKind[] = [];
  const n = d.needed;
  if (d.to_me) out.push("toMe");
  if (d.watched) out.push("watched");
  if (n && d.call) {
    if (n.new_dxcc) out.push("newDxcc");
    else {
      if (n.new_band) out.push("newBand");
      if (n.new_mode) out.push("newMode");
    }
    if (n.new_grid) out.push("newGrid");
    if (n.new_call) out.push("newCall");
    else if (n.new_call_band) out.push("newCallBand");
  }
  if (d.cq) out.push("cq");
  return out;
}

/** The switched-on alerts for a decode, most important first. */
export function activeAlerts(d: FtxDecode, cfg: FtxAlertConfig): AlertKind[] {
  return alertsOf(d).filter((k) => cfg.alerts[k].on);
}

export const isWorked = (d: FtxDecode) => !!d.needed && !!d.call && !d.needed.new_call_band;

function ignored(call: string, list: string): boolean {
  return list
    .split(/[\s,;]+/)
    .map((s) => s.trim().toUpperCase())
    .filter(Boolean)
    .some((p) => (p.endsWith("*") ? call.startsWith(p.slice(0, -1)) : call === p));
}

/** Whether the alert settings' filters let a decode through. Decodes calling you always show. */
export function passes(d: FtxDecode, cfg: FtxAlertConfig): boolean {
  if (d.to_me) return true;
  if (cfg.hideWorked && isWorked(d)) return false;
  if (cfg.minSnr !== null && d.snr < cfg.minSnr) return false;
  if (cfg.continents.length && d.entity && !cfg.continents.includes(d.entity.cont)) return false;
  if (cfg.ignore.trim() && d.call && ignored(d.call.toUpperCase(), cfg.ignore)) return false;
  return true;
}
