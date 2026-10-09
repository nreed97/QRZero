import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { PopContext } from "../bus";
import { pathInfo } from "../geo";
import { useIntegrations, useRotator } from "../live";
import { localGet, localSet } from "../prefs";
import type { PaneActions } from "./SharedPanes";
import "../rotator.css";

interface Preset { name: string; az: number }
const PRESETS_KEY = "qrzero.rotator";
const DEFAULT_PRESETS: Preset[] = [
  { name: "EU", az: 45 },
  { name: "AF", az: 100 },
  { name: "VK", az: 260 },
  { name: "JA", az: 330 },
];
const TIMEOUT_MS = 90_000;

const norm = (a: number) => ((Math.round(a) % 360) + 360) % 360;
const diff = (a: number, b: number) => Math.abs((((a - b) % 360) + 540) % 360 - 180);
/** Point on the dial for a bearing (0 = up, clockwise) at radius r. */
const at = (az: number, r: number) => {
  const t = (az * Math.PI) / 180;
  return { x: r * Math.sin(t), y: -r * Math.cos(t) };
};

export default function RotatorPane({ ctx, act }: { ctx: PopContext; act: PaneActions }) {
  const integrations = useIntegrations();
  const heading = useRotator();
  const [target, setTarget] = useState<{ az: number; since: number } | null>(null);
  const [err, setErr] = useState("");
  const [typed, setTyped] = useState("");
  const [hover, setHover] = useState<number | null>(null);
  const [presets, setPresets] = useState<Preset[]>(() => localGet(PRESETS_KEY, { presets: DEFAULT_PRESETS }).presets);
  const [editing, setEditing] = useState(false);
  const svgRef = useRef<SVGSVGElement>(null);

  const info = ctx.home && ctx.dx ? pathInfo(ctx.home, ctx.dx) : null;
  // A UDP connection for rotator requests (Settings, UDP connections) also counts as set up.
  const [udpRotator, setUdpRotator] = useState(false);
  useEffect(() => {
    if (integrations && !integrations.rotator_enabled) {
      api.udpConnections().then((r) => setUdpRotator(r.connections.some((c) => c.enabled && c.event === "rotator"))).catch(() => {});
    }
  }, [integrations]);
  const notSetUp = integrations !== null && !integrations.rotator_enabled && !udpRotator;

  // Clear "Turning to" once the heading is within 3 degrees, or after 90 s.
  useEffect(() => {
    if (target && heading !== null && diff(heading, target.az) <= 3) setTarget(null);
  }, [heading, target]);
  useEffect(() => {
    if (!target) return;
    const t = setTimeout(() => setTarget(null), Math.max(0, target.since + TIMEOUT_MS - Date.now()));
    return () => clearTimeout(t);
  }, [target]);

  const turn = (az: number) => {
    const a = norm(az);
    setErr("");
    setTarget({ az: a, since: Date.now() });
    api.rotate(a).catch((e) => {
      setErr((e as Error).message);
      setTarget(null);
    });
  };

  const bearingAt = (e: React.MouseEvent<SVGSVGElement>): number | null => {
    const svg = svgRef.current;
    const m = svg?.getScreenCTM();
    if (!svg || !m) return null;
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    if (Math.hypot(p.x, p.y) > 104) return null;
    return norm((Math.atan2(p.x, -p.y) * 180) / Math.PI);
  };

  const savePresets = (p: Preset[]) => {
    setPresets(p);
    localSet(PRESETS_KEY, { presets: p });
  };

  const go = () => {
    const n = Number(typed);
    if (typed.trim() === "" || !Number.isFinite(n)) return setErr("Type a bearing from 0 to 359.");
    turn(n);
  };

  if (notSetUp) {
    return (
      <div className="rotator">
        <div className="rot-off">
          <p>The rotator isn't set up yet. QRZero turns your antenna directly (rotctld, GS-232) or through PstRotatorAz, or another rotator program through a UDP connection.</p>
          <button onClick={() => act.onSettings("radios")}>Set up the rotator…</button>
        </div>
      </div>
    );
  }

  // Ticks every 5 degrees, longer every 10, numbered every 30.
  const ticks = [];
  for (let a = 0; a < 360; a += 5) {
    const len = a % 30 === 0 ? 9 : a % 10 === 0 ? 6 : 3;
    const p1 = at(a, 100), p2 = at(a, 100 - len);
    ticks.push(<line key={a} className={a % 30 === 0 ? "major" : ""} x1={p1.x} y1={p1.y} x2={p2.x} y2={p2.y} />);
  }
  const numbers = [];
  for (let a = 30; a < 360; a += 30) {
    if (a % 90 === 0) continue;
    const p = at(a, 80);
    numbers.push(<text key={a} x={p.x} y={p.y + 3.5} className="num">{a}</text>);
  }
  const cards = (["N", "E", "S", "W"] as const).map((c, i) => {
    const p = at(i * 90, 80);
    return <text key={c} x={p.x} y={p.y + 5} className="card">{c}</text>;
  });

  const marker = (az: number, cls: string, label: string) => {
    const tip = at(az, 101), l = at(az - 4, 112), r = at(az + 4, 112), t = at(az, 121);
    return (
      <g className={`mark ${cls}`}>
        <line x1={0} y1={0} x2={tip.x} y2={tip.y} />
        <polygon points={`${tip.x},${tip.y} ${l.x},${l.y} ${r.x},${r.y}`} />
        <text x={t.x} y={t.y + 3}>{label}</text>
      </g>
    );
  };

  const needle = heading !== null ? at(heading, 92) : null;
  const tail = heading !== null ? at(heading + 180, 14) : null;
  const statusText = target ? `Turning to ${target.az}°` : heading === null ? "No heading from the rotator" : "";

  return (
    <div className="rotator">
      <div className="rot-head">
        <span className="rot-heading mono">{heading === null ? "---" : `${String(Math.round(heading) % 360).padStart(3, "0")}°`}</span>
        <span className={`rot-status ${target ? "moving" : "muted"}`}>{statusText}</span>
        {hover !== null && <span className="muted small rot-hover">Click to turn to {hover}°</span>}
      </div>
      <div className="rot-body">
        <div className="rot-dial">
          <svg
            ref={svgRef}
            viewBox="-128 -128 256 256"
            preserveAspectRatio="xMidYMid meet"
            onMouseMove={(e) => setHover(bearingAt(e))}
            onMouseLeave={() => setHover(null)}
            onClick={(e) => {
              const b = bearingAt(e);
              if (b !== null) turn(b);
            }}
            role="img"
            aria-label={`Rotator heading ${heading === null ? "unknown" : Math.round(heading) + " degrees"}`}
          >
            <circle className="face" r={104} />
            <circle className="rim" r={100} />
            <g className="ticks">{ticks}</g>
            {numbers}
            {cards}
            {info && marker(info.lp, "lp", "LP")}
            {info && marker(info.sp, "sp", "SP")}
            {target && (() => {
              const p = at(target.az, 100);
              return <line className="target" x1={0} y1={0} x2={p.x} y2={p.y} />;
            })()}
            {hover !== null && (() => {
              const p = at(hover, 100);
              return <line className="hover" x1={0} y1={0} x2={p.x} y2={p.y} />;
            })()}
            {needle && tail && (
              <g className="needle">
                <line x1={tail.x} y1={tail.y} x2={needle.x} y2={needle.y} />
                <circle r={4} />
              </g>
            )}
          </svg>
        </div>
        <div className="rot-controls">
          {info && (
            <div className="rot-paths">
              <div className="small muted rot-dx">{ctx.dxLabel || "Station"}</div>
              <button onClick={() => turn(info.sp)} title="Turn to the short-path bearing">Short path {norm(info.sp)}°</button>
              <button onClick={() => turn(info.lp)} title="Turn to the long-path bearing">Long path {norm(info.lp)}°</button>
            </div>
          )}
          <div className="rot-go">
            <input
              value={typed}
              inputMode="numeric"
              placeholder="Bearing"
              aria-label="Bearing in degrees"
              onChange={(e) => setTyped(e.target.value.replace(/[^\d.]/g, ""))}
              onKeyDown={(e) => e.key === "Enter" && go()}
            />
            <button onClick={go}>Go</button>
          </div>
          <div className="rot-presets">
            {!editing &&
              presets.map((p, i) => (
                <button key={i} onClick={() => turn(p.az)} title={`Turn to ${p.az}°`}>
                  {p.name} {norm(p.az)}°
                </button>
              ))}
            {editing && (
              <div className="rot-edit">
                {presets.map((p, i) => (
                  <div key={i} className="row">
                    <input
                      value={p.name}
                      aria-label="Preset name"
                      onChange={(e) => savePresets(presets.map((q, j) => (j === i ? { ...q, name: e.target.value } : q)))}
                    />
                    <input
                      className="az"
                      value={p.az}
                      inputMode="numeric"
                      aria-label="Preset bearing"
                      onChange={(e) => savePresets(presets.map((q, j) => (j === i ? { ...q, az: norm(Number(e.target.value.replace(/\D/g, "")) || 0) } : q)))}
                    />
                    <button className="tiny" onClick={() => savePresets(presets.filter((_, j) => j !== i))} title="Remove">Remove</button>
                  </div>
                ))}
                <div className="row">
                  <button className="tiny" onClick={() => savePresets([...presets, { name: "New", az: heading !== null ? norm(heading) : 0 }])}>
                    Add{heading !== null ? ` (${norm(heading)}°)` : ""}
                  </button>
                  <button className="tiny" onClick={() => savePresets(DEFAULT_PRESETS)}>Reset</button>
                </div>
              </div>
            )}
            <button className="tiny rot-edit-btn" onClick={() => setEditing(!editing)}>{editing ? "Done" : "Edit presets"}</button>
          </div>
          {err && <div className="small err">{err}</div>}
        </div>
      </div>
    </div>
  );
}
