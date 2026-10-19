import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { onLive } from "../live";
import { MODE_GROUP_NAME, modeGroup } from "../modes";
import { compareNeeded, type NeedRank, needLabel, needRank, spotKey } from "../needed";
import { setNeededConfig, useNeededConfig } from "../neededPrefs";
import type { MostWanted, Spot } from "../types";
import type { DecodePick } from "./FtxMonitor";
import "../watch.css";

export const spotPick = (s: Spot, qsy = false): DecodePick => ({ call: s.call, grid: null, band: s.band, mode: s.mode, freq_hz: s.freq_hz, tx_freq_hz: s.tx_freq_hz ?? undefined, qsy });

/** Cluster spots on the air now that would be new for the log, best first. */
export default function NeededPane({ onPick, onSettings }: { onPick: (p: DecodePick) => void; onSettings: () => void }) {
  const cfg = useNeededConfig();
  const [spots, setSpots] = useState<Map<string, Spot>>(new Map());
  const [connected, setConnected] = useState(false);
  const [now, setNow] = useState(Date.now() / 1000);
  const [err, setErr] = useState("");
  const [wanted, setWanted] = useState<MostWanted | null>(null);

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
    api.mostWanted().then(setWanted).catch(() => {});
    const t = setInterval(() => setNow(Date.now() / 1000), 15000);
    const off = onLive((e) => {
      if (e.type === "spot") setSpots((m) => new Map(m).set(spotKey(e.spot), e.spot));
      if (e.type === "cluster_state") setConnected(e.connected);
      if (e.type === "spots_reflagged") api.cluster().then((c) => setSpots(new Map(c.spots.map((s) => [spotKey(s), s])))).catch(() => {});
    });
    return () => {
      clearInterval(t);
      off();
    };
  }, []);

  const rows = useMemo(() => {
    const out: { s: Spot; rank: NeedRank; wanted: number }[] = [];
    for (const s of spots.values()) {
      const rank = needRank(s);
      if (rank !== null && now - s.received < cfg.listMin * 60) {
        const code = s.entity?.dxcc;
        out.push({ s, rank, wanted: (code != null && wanted?.ranks[code]) || Infinity });
      }
    }
    // New entity first, then band, then mode; within each the most wanted entity, then the newest spot.
    return out.sort((a, b) => compareNeeded({ ...a, received: a.s.received }, { ...b, received: b.s.received }));
  }, [spots, now, cfg.listMin, wanted]);

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
        {wanted && Object.keys(wanted.ranks).length === 0 && <span className="muted small" title={wanted.error ?? ""}>Most Wanted ranks not loaded</span>}
        <span className="spacer" />
        <button onClick={onSettings} title="Sound and popup alerts for needed spots">Alerts…</button>
      </div>
      {err && <div className="ftx-msg small err">{err}</div>}
      <div className="ftx-table needed-table" role="table" aria-label="Needed now">
        <div className="needed-row head" role="row">
          <span>New</span><span title="Club Log Most Wanted rank of the entity">Wanted</span><span>Call</span><span>Country</span><span>Band</span><span>Freq</span><span>Mode</span><span>UTC</span><span>Spotter</span><span>Comment</span>
        </div>
        {rows.length === 0 && (
          <div className="empty muted">
            {connected ? "Nothing you need is on the air right now." : "Connect to a cluster (Cluster tab) to see what is on the air."}
          </div>
        )}
        {rows.map(({ s, rank, wanted: wr }) => {
          const group = modeGroup(s.mode);
          return (
            <div
              key={spotKey(s)}
              className={`needed-row ${group ? `mode-${group}` : ""} ${rank === 0 ? "needed" : ""}`}
              role="row"
              onClick={() => onPick(spotPick(s))}
              onDoubleClick={() => onPick(spotPick(s, true))}
              title={s.tx_freq_hz ? `Click to fill in ${s.call}; double-click to tune too and set split to ${(s.tx_freq_hz / 1000).toFixed(2)} kHz` : `Click to fill in ${s.call}; double-click to tune too`}
            >
              <span><span className={`flag ${rank === 0 ? "new" : "info"}`}>{needLabel(s)}</span></span>
              <span className="mono num" title={wr === Infinity ? "" : `Club Log Most Wanted: number ${wr}`}>{wr === Infinity ? "" : wr}</span>
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
