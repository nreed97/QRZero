import { useEffect, useRef, useState } from "react";
import { beep } from "../display";
import { onLive } from "../live";
import { AlertGate, NEED_NAME, needRank } from "../needed";
import { useNeededConfig } from "../neededPrefs";
import type { Spot } from "../types";
import type { DecodePick } from "./FtxMonitor";
import { spotPick } from "./NeededPane";

const SHOW_SEC = 25;
const MAX = 4;
interface Toast { id: number; spot: Spot }

/** Sound and popup for a needed spot the moment it appears on the cluster. Lives in the main window only. */
export default function NeededAlerts({ onPick }: { onPick: (p: DecodePick) => void }) {
  const cfg = useNeededConfig();
  const cfgRef = useRef(cfg);
  cfgRef.current = cfg;
  const gate = useRef(new AlertGate());
  const next = useRef(1);
  const [toasts, setToasts] = useState<Toast[]>([]);

  useEffect(
    () =>
      onLive((e) => {
        if (e.type !== "spot") return;
        const c = cfgRef.current;
        if (!c.sound && !c.popup) return;
        // A spot the cluster replays when it connects is old news.
        if (Date.now() / 1000 - e.spot.received > 120) return;
        if (!gate.current.check(e.spot, c, Date.now() / 1000)) return;
        if (c.sound) {
          // A higher, double beep for a new entity.
          beep(needRank(e.spot) === 0 ? 1100 : 780);
          if (needRank(e.spot) === 0) setTimeout(() => beep(1100), 220);
        }
        if (c.popup) {
          const id = next.current++;
          setToasts((t) => [{ id, spot: e.spot }, ...t].slice(0, MAX));
          setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), SHOW_SEC * 1000);
        }
      }),
    [],
  );

  if (!toasts.length) return null;
  return (
    <div className="needed-toasts" role="alert" aria-live="polite">
      {toasts.map(({ id, spot: s }) => (
        <div key={id} className="needed-toast">
          <button
            className="needed-toast-body"
            title={`Click to tune to ${s.call}`}
            onClick={() => {
              onPick(spotPick(s));
              setToasts((t) => t.filter((x) => x.id !== id));
            }}
          >
            <b>{NEED_NAME[needRank(s) ?? 2]}</b> <span className="mono call">{s.call}</span> {s.entity?.name ?? ""}
            <br />
            <span className="mono">{s.band ?? ""} {(s.freq_hz / 1000).toFixed(1)} {s.mode}</span>
          </button>
          <button className="tiny" aria-label="Dismiss" onClick={() => setToasts((t) => t.filter((x) => x.id !== id))}>×</button>
        </div>
      ))}
    </div>
  );
}
