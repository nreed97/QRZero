import { distanceKm } from "./geo";
import type { Entity, Spot } from "./types";

/** The Cluster pane's "Origin and country" choices. Needed now, the band map and the alerts follow them. */
export type Origin = "" | "continent" | "near";
export interface OriginFilters {
  /** Where the spotter must be: anywhere, on my continent, or within `km` of me. */
  origin: Origin;
  km: number;
  /** Hide spots where the spotter and the station are in the same country. */
  hideSelf: boolean;
  /** Hide spots of stations in my own country. */
  hideHome: boolean;
  /** Hide spots of stations in these continents. */
  hideCont: string[];
  /** Hide spots with no usable country (bad calls, beacons). */
  hideUnknown: boolean;
}
export const ORIGIN_DEFAULTS: OriginFilters = { origin: "", km: 3000, hideSelf: false, hideHome: false, hideCont: [], hideUnknown: false };

export const originActive = (f: OriginFilters) => !!f.origin || f.hideSelf || f.hideHome || f.hideUnknown || f.hideCont.length > 0;

function sameCountry(a?: Entity | null, b?: Entity | null): boolean {
  return !!a && !!b && (a.dxcc != null && b.dxcc != null ? a.dxcc === b.dxcc : a.prefix === b.prefix);
}

export function passesOrigin(s: Spot, f: OriginFilters, home: Entity | null): boolean {
  if (f.hideSelf && sameCountry(s.entity, s.spotter_entity)) return false;
  if (f.hideHome && sameCountry(s.entity, home)) return false;
  if (f.hideUnknown && !s.entity) return false;
  if (s.entity && f.hideCont.includes(s.entity.cont)) return false;
  // Origin filters only apply when both ends are known; an unknown spotter is kept.
  if (home && s.spotter_entity) {
    if (f.origin === "continent" && s.spotter_entity.cont !== home.cont) return false;
    if (f.origin === "near" && distanceKm(home, s.spotter_entity) > f.km) return false;
  }
  return true;
}
