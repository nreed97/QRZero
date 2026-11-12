import { useEffect, useRef, useState } from "react";
import { beep } from "../display";
import { onLive } from "../live";
import { AlertGate, needLabel, needRank } from "../needed";
import { useNeededConfig } from "../neededPrefs";
import { getOrigin, loadHome, originActive, passesOrigin } from "../spotOrigin";
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
        const spot = e.spot;
        void (async () => {
          // Spots the Cluster pane's origin filters hide don't alert either.
          const origin = getOrigin();
          if (originActive(origin) && !passesOrigin(spot, origin, await loadHome())) return;
          if (!gate.current.check(spot, c, Date.now() / 1000)) return;
          if (c.sound) {
            // A higher, double beep for a new entity.
            beep(needRank(spot) === 0 ? 1100 : 780);
            if (needRank(spot) === 0) setTimeout(() => beep(1100), 220);
          }
          if (c.popup) {
            const id = next.current++;
            setToasts((t) => [{ id, spot }, ...t].slice(0, MAX));
            setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), SHOW_SEC * 1000);
          }
        })();
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
            title={`Click to fill in ${s.call}; double-click to tune too`}
            onClick={() => onPick(spotPick(s))}
            onDoubleClick={() => {
              onPick(spotPick(s, true));
              setToasts((t) => t.filter((x) => x.id !== id));
            }}
          >
            <b>{needLabel(s)}</b> <span className="mono call">{s.call}</span> {s.entity?.name ?? ""}
            <br />
            <span className="mono">{s.band ?? ""} {(s.freq_hz / 1000).toFixed(1)} {s.mode}</span>
          </button>
          <button className="tiny" aria-label="Dismiss" onClick={() => setToasts((t) => t.filter((x) => x.id !== id))}>×</button>
        </div>
      ))}
    </div>
  );
}
