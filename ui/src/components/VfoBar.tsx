import { bandForFreq } from "../modes";
import type { Radio } from "../types";
import { usePopup } from "./usePopup";

/** 14074000 Hz as 14.074.000, the way a radio's display reads. */
export function vfoText(hz: number): string {
  const khz = Math.round(hz / 10) / 100; // to 10 Hz
  const [whole, frac = ""] = khz.toFixed(2).split(".");
  const mhz = whole.slice(0, -3) || "0";
  return `${mhz}.${whole.slice(-3).padStart(3, "0")}.${frac}0`;
}

function Readout({ r }: { r: Radio }) {
  const live = r.connected && r.freq_hz > 0;
  const band = live ? bandForFreq(r.freq_hz / 1e6) : undefined;
  const mode = r.mode || r.rig_mode;
  return (
    <>
      <span className="vfo-name">{r.name}</span>
      <span className="vfo-freq">{live ? vfoText(r.freq_hz) : "--.---.---"}</span>
      {band && <span className="vfo-band">{band}</span>}
      {live && mode && <span className="vfo-mode">{mode}{r.data && !/DATA|FT|PSK|RTTY/.test(mode) ? "-D" : ""}</span>}
      <span className="vfo-state">{!r.connected ? "off" : r.tx ? "TX" : "RX"}</span>
    </>
  );
}

const cls = (r: Radio) => `vfo${r.tx && r.connected ? " tx" : ""}${r.connected ? "" : " offline"}`;

/**
 * The radio the QSO panel follows, in the top bar. With several radios (Flex slices, WSJT-X copies)
 * the others sit in a drop-down, so the bar stays one readout wide.
 */
export default function VfoBar({ radios, radioKey, onRadio }: { radios: Radio[]; radioKey: string; onRadio: (key: string) => void }) {
  const { open, setOpen, box } = usePopup<HTMLDivElement>();
  if (!radios.length) return null;
  const shown = radios.find((r) => r.key === radioKey) ?? radios.find((r) => r.connected) ?? radios[0];
  const others = radios.filter((r) => r !== shown);
  const otherTx = others.some((r) => r.connected && r.tx);
  return (
    <div className="vfo-bar layout-menu" ref={box} aria-label="Radio">
      <div
        className={`${cls(shown)} followed`}
        title={shown.error ? `${shown.name}: ${shown.error}` : `${shown.name}${shown.key === radioKey ? " (the QSO panel follows this radio)" : ""}`}
        data-testid="vfo"
      >
        <Readout r={shown} />
      </div>
      {others.length > 0 && (
        <button
          className={`vfo-more${otherTx ? " tx" : ""}`}
          onClick={() => setOpen(!open)}
          aria-expanded={open}
          aria-haspopup="menu"
          title={`${others.length} more radio${others.length === 1 ? "" : "s"}${otherTx ? ", one transmitting" : ""}: pick one to follow`}
        >
          +{others.length} &#9662;
        </button>
      )}
      {open && (
        <div className="layout-pop vfo-pop" role="menu">
          <div className="head">Follow radio</div>
          {radios.map((r) => (
            <button key={r.key} role="menuitem" className={`item ${cls(r)}${r === shown ? " cur" : ""}`} onClick={() => { setOpen(false); onRadio(r.key); }}>
              <Readout r={r} />
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
