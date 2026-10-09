import { useEffect, useMemo, useRef, useState } from "react";
import { geoAzimuthalEquidistant, geoCircle, geoEquirectangular, geoGraticule10, geoInterpolate, geoPath } from "d3-geo";
import { api } from "../api";
import { compass, destination, fmtDistance, pathInfo, subsolarPoint, type LatLon } from "../geo";

import { land, borders } from "../mapData";
import { fmtLocal, fmtUtc, greylineOverlap, sunTimes } from "../sun";
import { useDisplay } from "../display";

export type MapView = "flat" | "azimuthal";

interface Props {
  home: LatLon | null;
  homeLabel: string;
  dx: LatLon | null;
  dxLabel: string;
  units: "km" | "mi";
  view: MapView;
  onView: (v: MapView) => void;
  /** Whether the night side is shaded. */
  shade: boolean;
  onShade: (on: boolean) => void;
  /** Rotator heading, when PstRotatorAz is connected; undefined when it isn't set up. */
  rotator?: number | null;
}

export default function MapPanel({ home, homeLabel, dx, dxLabel, units, view, onView, shade, onShade, rotator }: Props) {
  const display = useDisplay();
  const [turnErr, setTurnErr] = useState("");
  const turn = (az: number) => {
    setTurnErr("");
    api.rotate(az).catch((e) => setTurnErr((e as Error).message));
  };
  const box = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 380, h: 210 });
  const [now, setNow] = useState(new Date());

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setSize({ w: Math.max(200, e.contentRect.width), h: Math.max(140, e.contentRect.height) }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // The grey line moves slowly; once a minute is plenty.
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(t);
  }, []);

  const centre = home ?? { lat: 0, lon: 0 };
  const { path, project } = useMemo(() => {
    const p =
      view === "azimuthal"
        ? geoAzimuthalEquidistant().rotate([-centre.lon, -centre.lat]).clipAngle(179.9)
        : geoEquirectangular().rotate([-centre.lon, 0]);
    p.fitExtent([[2, 2], [size.w - 2, size.h - 2]], { type: "Sphere" });
    return { path: geoPath(p), project: (ll: LatLon) => p([ll.lon, ll.lat]) };
  }, [view, centre.lat, centre.lon, size.w, size.h]);

  const night = useMemo(() => {
    const sun = subsolarPoint(now);
    return geoCircle().center([sun.lon + 180, -sun.lat]).radius(90)();
  }, [now]);

  const info = home && dx ? pathInfo(home, dx) : null;
  let shortPath = "", longPath = "";
  if (home && dx) {
    shortPath = path({ type: "LineString", coordinates: [[home.lon, home.lat], [dx.lon, dx.lat]] }) ?? "";
    const mid = geoInterpolate([home.lon, home.lat], [dx.lon, dx.lat])(0.5);
    const anti: [number, number] = [mid[0] + 180, -mid[1]];
    longPath = path({ type: "LineString", coordinates: [[home.lon, home.lat], anti, [dx.lon, dx.lat]] }) ?? "";
  }

  // The beam: a great circle leaving the QTH on the rotator heading, drawn out to the far side of the globe.
  let beamPath = "";
  if (home && typeof rotator === "number") {
    const coords: [number, number][] = [];
    for (let d = 0; d <= 180; d += 2) {
      const p = destination(home, rotator, d);
      coords.push([p.lon, p.lat]);
    }
    beamPath = path({ type: "LineString", coordinates: coords }) ?? "";
  }

  const marker = (ll: LatLon | null, label: string, cls: string) => {
    const pt = ll && project(ll);
    if (!pt) return null;
    return (
      <g className={cls} transform={`translate(${pt[0]},${pt[1]})`}>
        <circle r={3.5} />
        <text x={6} y={4}>{label}</text>
      </g>
    );
  };

  return (
    <section className="panel map-panel">
      <div className="panel-title">
        <span>Map</span>
        <span className="spacer" />
        <label className="check" title="Shade the night side of the Earth (the grey line)"><input type="checkbox" checked={shade} onChange={(e) => onShade(e.target.checked)} /> Night</label>
        <button className={`tiny ${view === "flat" ? "on" : ""}`} onClick={() => onView("flat")} title="Flat world map">Flat</button>
        <button className={`tiny ${view === "azimuthal" ? "on" : ""}`} onClick={() => onView("azimuthal")} title="Azimuthal equidistant, centred on your location">Azimuthal</button>
      </div>
      <div className="map-box" ref={box}>
        <svg width={size.w} height={size.h} role="img" aria-label="Map of the path between you and the other station">
          <path className="sea" d={path({ type: "Sphere" }) ?? ""} />
          <path className="graticule" d={path(geoGraticule10()) ?? ""} />
          <path className="land" d={path(land) ?? ""} />
          <path className="borders" d={path(borders) ?? ""} />
          {shade && <path className="night" d={path(night) ?? ""} />}
          {longPath && <path className="long-path" d={longPath} />}
          {shortPath && <path className="short-path" d={shortPath} />}
          {beamPath && <path className="beam-path" d={beamPath} />}
          {marker(home, homeLabel, "home")}
          {marker(dx, dxLabel, "dx")}
        </svg>
      </div>
      <div className="map-info">
        {!home ? (
          <span className="muted">Set a grid for your location to see paths.</span>
        ) : !info ? (
          <span className="muted">No position for the other station yet.</span>
        ) : (
          <>
            <span title="Short path">SP <b>{Math.round(info.sp)}°</b> {compass(info.sp)} · {fmtDistance(info.spKm, units)}</span>
            <span title="Long path">LP <b>{Math.round(info.lp)}°</b> · {fmtDistance(info.lpKm, units)}</span>
            {rotator !== undefined && (
              <span className="turn">
                <button className="tiny" onClick={() => turn(info.sp)} title="Turn the rotator to the short path">Turn SP</button>
                <button className="tiny" onClick={() => turn(info.lp)} title="Turn the rotator to the long path">LP</button>
              </span>
            )}
          </>
        )}
        {rotator !== undefined && <span className="muted rot" title="Rotator heading (dotted line on the map)">Rot {rotator === null ? "?" : `${Math.round(rotator)}°`}</span>}
        {turnErr && <span className="err">{turnErr}</span>}
      </div>
      <SunInfo home={home} homeLabel={homeLabel} dx={dx} dxLabel={dxLabel} now={now} local={display.localTime} />
    </section>
  );
}

function sunText(t: ReturnType<typeof sunTimes>, local: boolean) {
  if (!t.rise || !t.set) return t.polar === "day" ? "sun up all day" : "no sunrise today";
  const base = `${fmtUtc(t.rise)} / ${fmtUtc(t.set)}`;
  return local ? `${base} (${fmtLocal(t.rise)} / ${fmtLocal(t.set)} local)` : base;
}

/** Sunrise and sunset at your QTH and the other station, and when the grey line is over both. */
function SunInfo({ home, homeLabel, dx, dxLabel, now, local }: { home: LatLon | null; homeLabel: string; dx: LatLon | null; dxLabel: string; now: Date; local: boolean }) {
  if (!home) return null;
  const mine = sunTimes(now, home);
  const theirs = dx ? sunTimes(now, dx) : null;
  const both = dx ? greylineOverlap(now, home, dx) : [];
  return (
    <div className="map-sun" title="Sunrise / sunset, UTC. The grey line window is within 30 minutes of sunrise or sunset at both ends.">
      <span>Sun {homeLabel || "QTH"} <b>{sunText(mine, local)}</b></span>
      {theirs && <span>{dxLabel || "DX"} <b>{sunText(theirs, false)}</b></span>}
      {dx && (
        <span>
          Grey line both:{" "}
          <b>{both.length ? both.map((w) => `${fmtUtc(w.from)}-${fmtUtc(w.to)}`).join(", ") : "none today"}</b>
        </span>
      )}
    </div>
  );
}
