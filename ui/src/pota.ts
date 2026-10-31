import { useEffect, useState } from "react";
import { api } from "./api";
import { onLive } from "./live";
import type { PotaList } from "./types";

export const POTA_EVERY_MS = 60000;

/**
 * The POTA spot list, read now and then once a minute while `active`. Nothing runs when it is
 * off. A logged QSO reloads it too, so the worked flags follow the log.
 */
export function usePota(active: boolean): { list: PotaList | null; err: string; refresh: () => void } {
  const [list, setList] = useState<PotaList | null>(null);
  const [err, setErr] = useState("");
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    if (!active) return;
    let live = true;
    const load = (force: boolean) =>
      api
        .pota(force)
        .then((l) => { if (live) { setList(l); setErr(""); } })
        .catch((e) => { if (live) setErr((e as Error).message); });
    void load(nonce > 0);
    const timer = setInterval(() => void load(false), POTA_EVERY_MS);
    const off = onLive((e) => {
      if ((e as { type: string }).type === "qso_noted") setTimeout(() => void load(false), 1500);
    });
    return () => {
      live = false;
      clearInterval(timer);
      off();
    };
  }, [active, nonce]);

  return { list, err, refresh: () => setNonce((n) => n + 1) };
}

/** What a POTA spot would be new for, as short labels. */
export function potaNeeds(n: import("./types").PotaSpot["needed"]): string[] {
  if (!n) return [];
  const out: string[] = [];
  if (n.new_dxcc) out.push("New DXCC");
  if (n.new_band) out.push("New band");
  if (n.new_mode) out.push("New mode");
  return out;
}
