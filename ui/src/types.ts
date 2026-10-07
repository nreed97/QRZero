export type Fields = Record<string, string>;

export interface Log { id: number; name: string; qso_count: number }
export interface StationCallsign { id: number; log_id: number; callsign: string; is_default: boolean }
export interface Location { id: number; log_id: number; name: string; is_default: boolean; fields: Fields }
export interface Qso { id: number; log_id: number; location_id: number | null; fields: Fields }

export interface QsoFilter {
  call?: string;
  bands?: string[];
  modes?: string[];
  from?: number;
  to?: number;
  station_callsigns?: string[];
  location_ids?: number[];
  dxcc?: number;
  fields?: Fields;
  ids?: number[];
}

export interface WorkedBefore {
  call_count: number;
  recent: Qso[];
  call_slots: [string, string][];
  dxcc: number | null;
  dxcc_count: number;
  dxcc_bands: string[];
  dxcc_modes: string[];
}

export interface Entity { name: string; prefix: string; dxcc: number | null; cont: string; cq: number; itu: number; lat: number; lon: number }

export interface LookupResult {
  worked: WorkedBefore;
  entity: Entity | null;
  station: Fields | null;
  source: string | null;
  error: string | null;
}

export interface ImportReport {
  imported: number;
  duplicates: number;
  rejected: number;
  added_callsigns: string[];
  messages: string[];
}

export interface Settings { qrz_enabled: boolean; qrz_username: string; qrz_password_set: boolean }

export type EquipmentKind = "rig" | "antenna" | "amplifier" | "rotator" | "other";
export interface Equipment { id: number; location_id: number; kind: EquipmentKind; name: string; fields: Fields; sort: number }

/** A frequency source the entry panel can follow: a controlled rig, a WSJT-X instance or an N1MM radio. */
export interface Radio {
  key: string;
  name: string;
  source: "rig" | "wsjtx" | "n1mm";
  can_tune: boolean;
  connected: boolean;
  freq_hz: number;
  mode: string;
  rig_mode: string;
  data: boolean;
  tx: boolean;
  error: string | null;
}

export interface Needed { new_call: boolean; new_call_band: boolean; new_dxcc: boolean; new_band: boolean; new_mode: boolean }

export interface FtxDecode {
  seq: number;
  instance: string;
  time: string;
  snr: number;
  dt: number;
  df: number;
  mode: string;
  message: string;
  call: string | null;
  to: string | null;
  grid: string | null;
  cq: boolean;
  cq_target: string | null;
  to_me: boolean;
  band: string | null;
  freq_hz: number;
  entity: Entity | null;
  needed: Needed | null;
  low_confidence: boolean;
}

export interface FtxInstance { id: string; dial_freq: number; band: string | null; mode: string; de_call: string; dx_call: string; transmitting: boolean; tx_enabled: boolean }

export interface Integrations {
  wsjtx_enabled: boolean;
  wsjtx_listen: string;
  wsjtx_multicast: string;
  wsjtx_forward: string[];
  wsjtx_auto_log: boolean;
  n1mm_enabled: boolean;
  n1mm_listen: string;
  n1mm_auto_log: boolean;
  rotator_enabled: boolean;
  rotator_addr: string;
}

export interface IntegrationStatus { wsjtx: string | null; n1mm: string | null; rotator: string | null }
export interface CtyStatus { entities: number; age_days: number | null; file: string | null }
