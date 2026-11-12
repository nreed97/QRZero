import { useEffect, useState } from "react";
import { api } from "./api";
import { localGet, localSet } from "./prefs";
import { type OriginFilters, ORIGIN_DEFAULTS, originActive } from "./spotOriginFilter";
import type { Entity } from "./types";

export { ORIGIN_DEFAULTS, originActive, passesOrigin } from "./spotOriginFilter";
export type { Origin, OriginFilters } from "./spotOriginFilter";

/** Same key as the Cluster pane's other filters, which share the saved object. */
export const CLUSTER_KEY = "qrzero.cluster";
const EVENT = "qrzero-cluster-filters";

export const getOrigin = (): OriginFilters => ({ ...ORIGIN_DEFAULTS, ...localGet<Partial<OriginFilters>>(CLUSTER_KEY, {}) });

/** Saves a change to the Cluster filters (any of them) and tells the other panes. */
export function saveClusterFilters(next: object) {
  localSet(CLUSTER_KEY, next);
  window.dispatchEvent(new Event(EVENT));
}

let homeCache: Entity | null | undefined;
/** The operator's own entity, read from the server once and only when a filter needs it. */
export async function loadHome(): Promise<Entity | null> {
  if (homeCache === undefined) {
    try {
      homeCache = (await api.cluster()).home ?? null;
    } catch {
      return null;
    }
  }
  return homeCache;
}
export const cachedHome = () => homeCache ?? null;

/** The current origin filters, kept up to date as the Cluster pane changes them, and the home entity once one is in use. */
export function useOrigin(): { filters: OriginFilters; home: Entity | null } {
  const [filters, setFilters] = useState(getOrigin);
  const [home, setHome] = useState<Entity | null>(cachedHome);
  useEffect(() => {
    const reload = () => setFilters(getOrigin());
    const onStorage = (e: StorageEvent) => e.key === CLUSTER_KEY && reload();
    window.addEventListener(EVENT, reload);
    window.addEventListener("storage", onStorage);
    return () => {
      window.removeEventListener(EVENT, reload);
      window.removeEventListener("storage", onStorage);
    };
  }, []);
  const active = originActive(filters);
  useEffect(() => {
    if (active && !home) void loadHome().then(setHome);
  }, [active, home]);
  return { filters, home };
}
