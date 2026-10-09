import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { onLive } from "../live";
import { MODE_GROUP_NAME, modeGroup } from "../modes";
import { NEED_NAME, needRank, spotKey } from "../needed";
import { setNeededConfig, useNeededConfig } from "../neededPrefs";
import type { Spot } from "../types";
import type { DecodePick } from "./FtxMonitor";
import "../watch.css";

export const spotPick = (s: Spot): DecodePick => ({ call: s.call, grid: null, band: s.band, mode: s.mode, freq_hz: s.freq_hz, tx_freq_hz: s.tx_freq_hz ?? undefined });

/** Cluster spots on the air now that would be new for the log, best first. */
export default function NeededPane({ onPick, onSettings }: { onPick: (p: DecodePick) => void; onSettings: () => void }) {
  const cfg = useNeededConfig();
  const [spots, setSpots] = useState<Map<string, Spot>>(new Map());
  const [connected, setConnected] = useState(false);
  const [now, setNow] = useState(Date.now() / 1000);
  const [err, setErr] = useState("");

  useEffect(() => {
    api
      .cluster()
      .then((c) => {
        setConnected(c.connected);
        setSpots((m) => {
          const next = new Map(m);
          for (const s of c.spots) if (!next.has(spotKey(s)) || next.get(spotKey(s))!.received < s.received) next.set(spotKey(s), s);
          return next;
        });
      })
      .catch((e) => setErr(e.message));
    const t = setInterval(() => setNow(Date.now() / 1000), 15000);
    const off = onLive((e) => {
      if (e.type === "spot") setSpots((m) => new Map(m).set(spotKey(e.spot), e.spot));
      if (e.type === "cluster_state") setConnected(e.connected);
    });
    return () => {
      clearInterval(t);
      off();
    };
  }, []);

  const rows = useMemo(() => {
    const out: { s: Spot; rank: number }[] = [];
    for (const s of spots.values()) {
      const rank = needRank(s);
      if (rank !== null && now - s.received < cfg.listMin * 60) out.push({ s, rank });
    }
    // New entity first, then band, then mode; the newest spot first within each.
    return out.sort((a, b) => a.rank - b.rank || b.s.received - a.s.received);
  }, [spots, now, cfg.listMin]);

  return (
    <div className="ftx cluster needed-pane">
      <div className="grid-tools">
        <span className="muted small">{rows.length} needed on the air</span>
        <label className="small">
          Show spots from the last{" "}
          <select value={cfg.listMin} onChange={(e) => setNeededConfig({ listMin: Number(e.target.value) })} aria-label="Age">
            <option value={10}>10 min</option>
            <option value={30}>30 min</option>
            <option value={60}>hour</option>
            <option value={120}>2 hours</option>
          </select>
        </label>
        <span className="spacer" />
        <button onClick={onSettings} title="Sound and popup alerts for needed spots">Alerts…</button>
      </div>
      {err && <div className="ftx-msg small err">{err}</div>}
      <div className="ftx-table needed-table" role="table" aria-label="Needed now">
        <div className="needed-row head" role="row">
          <span>New</span><span>Call</span><span>Country</span><span>Band</span><span>Freq</span><span>Mode</span><span>UTC</span><span>Spotter</span><span>Comment</span>
        </div>
        {rows.length === 0 && (
          <div className="empty muted">
            {connected ? "Nothing you need is on the air right now." : "Connect to a cluster (Cluster tab) to see what is on the air."}
          </div>
        )}
        {rows.map(({ s, rank }) => {
          const group = modeGroup(s.mode);
          return (
            <div
              key={spotKey(s)}
              className={`needed-row ${group ? `mode-${group}` : ""} ${rank === 0 ? "needed" : ""}`}
              role="row"
              onClick={() => onPick(spotPick(s))}
              title={s.tx_freq_hz ? `Click to tune to ${s.call} and set split to ${(s.tx_freq_hz / 1000).toFixed(2)} kHz` : `Click to tune to ${s.call}`}
            >
              <span><span className={`flag ${rank === 0 ? "new" : "info"}`}>{NEED_NAME[rank]}</span></span>
              <span className={`mono call ${s.watched ? "watched" : ""}`}>{s.call}</span>
              <span>{s.entity?.name ?? ""}</span>
              <span className="mono">{s.band ?? ""}</span>
              <span className="mono num">{(s.freq_hz / 1000).toFixed(1)}</span>
              <span className="mono mode" title={group ? MODE_GROUP_NAME[group] : "Mode not known"}>{s.mode}</span>
              <span className="mono">{s.time ? `${s.time.slice(0, 2)}:${s.time.slice(2, 4)}` : ""}</span>
              <span className="mono">{s.spotter}</span>
              <span className="comment">{s.comment}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
