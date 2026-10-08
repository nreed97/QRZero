import { bandForFreq } from "../modes";
import type { Radio } from "../types";

/** 14074000 Hz as 14.074.000, the way a radio's display reads. */
export function vfoText(hz: number): string {
  const khz = Math.round(hz / 10) / 100; // to 10 Hz
  const [whole, frac = ""] = khz.toFixed(2).split(".");
  const mhz = whole.slice(0, -3) || "0";
  return `${mhz}.${whole.slice(-3).padStart(3, "0")}.${frac}0`;
}

/** Each radio's frequency, band, mode and transmit state, in the top bar. Click one to have the QSO panel follow it. */
export default function VfoBar({ radios, radioKey, onRadio }: { radios: Radio[]; radioKey: string; onRadio: (key: string) => void }) {
  if (!radios.length) return null;
  return (
    <div className="vfo-bar" aria-label="Radios">
      {radios.map((r) => {
        const followed = r.key === radioKey;
        const live = r.connected && r.freq_hz > 0;
        const band = live ? bandForFreq(r.freq_hz / 1e6) : undefined;
        const mode = r.mode || r.rig_mode;
        const state = !r.connected ? "off" : r.tx ? "TX" : "RX";
        return (
          <button
            key={r.key}
            className={`vfo${followed ? " followed" : ""}${r.tx && r.connected ? " tx" : ""}${r.connected ? "" : " offline"}`}
            onClick={() => onRadio(r.key)}
            title={r.error ? `${r.name}: ${r.error}` : `${r.name}${followed ? " (the QSO panel follows this radio)" : ": click to follow this radio"}`}
            aria-pressed={followed}
            data-testid="vfo"
          >
            <span className="vfo-name">{r.name}</span>
            <span className="vfo-freq">{live ? vfoText(r.freq_hz) : "--.---.---"}</span>
            {band && <span className="vfo-band">{band}</span>}
            {live && mode && <span className="vfo-mode">{mode}{r.data && !/DATA|FT|PSK|RTTY/.test(mode) ? "-D" : ""}</span>}
            <span className="vfo-state">{state}</span>
          </button>
        );
      })}
    </div>
  );
}
