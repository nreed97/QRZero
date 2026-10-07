// The server's live event stream (/api/events): radios, FTx decodes, QSOs logged
// by other programs. Read with fetch so the session token can go in a header.
import { useEffect, useState } from "react";
import { api } from "./api";
import type { FtxDecode, FtxInstance, Integrations, Radio } from "./types";

export type LiveEvent =
  | { type: "radios"; radios: Radio[] }
  | { type: "ftx_instances"; instances: FtxInstance[] }
  | { type: "decode"; decode: FtxDecode }
  | { type: "ftx_clear"; instance: string }
  | { type: "qso_logged"; log_id: number; call: string; source: string; added: boolean }
  | { type: "rotator"; azimuth: number | null }
  | { type: "cty"; entities: number }
  | { type: "integrations"; config: Integrations }
  | { type: "error"; message: string };

type Listener = (e: LiveEvent) => void;
const listeners = new Set<Listener>();
const latest: { radios: Radio[]; instances: FtxInstance[]; rotator: number | null; connected: boolean; integrations: Integrations | null } = {
  integrations: null,
  radios: [],
  instances: [],
  rotator: null,
  connected: false,
};
let started = false;

function dispatch(e: LiveEvent) {
  if (e.type === "radios") latest.radios = e.radios;
  if (e.type === "ftx_instances") latest.instances = e.instances;
  if (e.type === "rotator") latest.rotator = e.azimuth;
  if (e.type === "integrations") latest.integrations = e.config;
  listeners.forEach((l) => l(e));
}

async function run() {
  for (;;) {
    try {
      const resp = await fetch("/api/events", { headers: { "x-qrzero-token": api.token() } });
      if (!resp.ok || !resp.body) throw new Error(String(resp.status));
      latest.connected = true;
      const reader = resp.body.pipeThrough(new TextDecoderStream()).getReader();
      let buf = "";
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buf += value;
        let end;
        while ((end = buf.indexOf("\n\n")) >= 0) {
          const chunk = buf.slice(0, end);
          buf = buf.slice(end + 2);
          const data = chunk.split("\n").filter((l) => l.startsWith("data:")).map((l) => l.slice(5).trimStart()).join("\n");
          if (data) {
            try { dispatch(JSON.parse(data)); } catch { /* ignore a bad event */ }
          }
        }
      }
    } catch { /* reconnect below */ }
    latest.connected = false;
    dispatch({ type: "radios", radios: [] });
    await new Promise((r) => setTimeout(r, 2000));
  }
}

export function onLive(l: Listener): () => void {
  if (!started && api.hasToken()) {
    started = true;
    void run();
  }
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useRadios(): Radio[] {
  const [radios, setRadios] = useState(latest.radios);
  useEffect(() => onLive((e) => e.type === "radios" && setRadios(e.radios)), []);
  return radios;
}

export function useRotator(): number | null {
  const [az, setAz] = useState(latest.rotator);
  useEffect(() => onLive((e) => e.type === "rotator" && setAz(e.azimuth)), []);
  return az;
}

export function useInstances(): FtxInstance[] {
  const [list, setList] = useState(latest.instances);
  useEffect(() => onLive((e) => e.type === "ftx_instances" && setList(e.instances)), []);
  return list;
}

export function useIntegrations(): Integrations | null {
  const [c, setC] = useState(latest.integrations);
  useEffect(() => onLive((e) => e.type === "integrations" && setC(e.config)), []);
  return c;
}
