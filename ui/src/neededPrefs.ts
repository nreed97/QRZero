import { useEffect, useState } from "react";
import { localGet, localSet } from "./prefs";
import type { NeededAlertConfig } from "./needed";

export const DEFAULT_NEEDED: NeededAlertConfig = { sound: true, popup: true, level: 2, repeatMin: 30, listMin: 30 };
const KEY = "qrzero.needed";
const EVENT = "qrzero-needed";

export const getNeededConfig = (): NeededAlertConfig => ({ ...DEFAULT_NEEDED, ...localGet<Partial<NeededAlertConfig>>(KEY, {}) });

export function setNeededConfig(patch: Partial<NeededAlertConfig>) {
  localSet(KEY, { ...getNeededConfig(), ...patch });
  window.dispatchEvent(new Event(EVENT));
}

export function useNeededConfig(): NeededAlertConfig {
  const [cfg, setCfg] = useState(getNeededConfig);
  useEffect(() => {
    const reload = () => setCfg(getNeededConfig());
    const onStorage = (e: StorageEvent) => e.key === KEY && reload();
    window.addEventListener(EVENT, reload);
    window.addEventListener("storage", onStorage);
    return () => {
      window.removeEventListener(EVENT, reload);
      window.removeEventListener("storage", onStorage);
    };
  }, []);
  return cfg;
}

