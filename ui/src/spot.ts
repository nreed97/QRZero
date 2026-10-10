// The live cluster state a spot button needs; the checks themselves are in spotCheck.ts.
import { useEffect, useState } from "react";
import { api } from "./api";
import { onLive } from "./live";
import { DEFAULT_SPOT_COMMENT, type SpotStatus } from "./spotCheck";

/** Whether the cluster is connected and the age limit; `active` refreshes it (e.g. when a menu opens). */
export function useSpotStatus(active = true): SpotStatus {
  const [s, setS] = useState<SpotStatus>({ connected: false, maxMinutes: 10, comment: DEFAULT_SPOT_COMMENT });
  useEffect(() => {
    if (!active) return;
    let live = true;
    const load = () => api.cluster().then((c) => live && setS({ connected: c.connected, maxMinutes: c.config.spot_max_minutes ?? 10, comment: c.config.spot_comment ?? DEFAULT_SPOT_COMMENT })).catch(() => {});
    load();
    const off = onLive((e) => e.type === "cluster_state" && live && setS((p) => ({ ...p, connected: e.connected })));
    return () => {
      live = false;
      off();
    };
  }, [active]);
  return s;
}
