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
  /** Short label of the instance it came from, e.g. "Slice A · WSJT-X". */
  source: string;
  slice: string | null;
  /** 0..7, stable per instance: picks the --src-N colour. */
  color_index: number;
}

export interface FtxInstance {
  id: string;
  dial_freq: number;
  band: string | null;
  mode: string;
  de_call: string;
  dx_call: string;
  transmitting: boolean;
  tx_enabled: boolean;
  program: string;
  configuration_name: string;
  slice: string | null;
  rig_key: string | null;
  rig_name: string | null;
  source: string;
  color_index: number;
}

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

export interface Spot {
  seq: number;
  received: number;
  time: string;
  spotter: string;
  call: string;
  freq_hz: number;
  band: string | null;
  mode: string;
  comment: string;
  entity: Entity | null;
  needed: Needed | null;
}

export interface ClusterNode { name: string; host: string; port: number; login: string; password: string; commands: string[] }
export interface ClusterConfig { nodes: ClusterNode[]; auto_connect: boolean }
export interface ClusterSnapshot { config: ClusterConfig; state: string; connected: boolean; spots: Spot[]; lines: string[] }

export interface QslRun { at: number; running: boolean; uploaded: number; duplicates: number; rejected: string[]; error: string | null }
export interface LotwMapping { callsign: string; location_id: number; station_location: string }
export interface QslConfig {
  interval_min: number;
  qrz_enabled: boolean;
  qrz_since: string;
  qrz_calls: string[];
  clublog_enabled: boolean;
  clublog_since: string;
  clublog_email: string;
  clublog_calls: string[];
  tqsl_path: string;
  lotw_since: string;
  lotw: LotwMapping[];
  lotw_username: string;
  lotw_rcvd_since: string;
  eqsl_enabled: boolean;
  eqsl_since: string;
  eqsl_username: string;
  eqsl_nickname: string;
  eqsl_calls: string[];
  eqsl_rcvd_since: string;
  confirm_daily: boolean;
}
export type QslService = "qrz" | "clublog" | "lotw" | "eqsl";
export interface QslDownload { at: number; running: boolean; received: number; confirmed: number; unmatched: string[]; unmatched_count: number; error: string | null }
export interface QslOverview {
  config: QslConfig;
  secrets: { qrz_calls: string[]; clublog_password: boolean; clublog_app_key: boolean; lotw_password: boolean; eqsl_password: boolean };
  pending: { qrz: number; clublog: number; eqsl: number; lotw: { mapping: LotwMapping; pending: number }[] };
  runs: Partial<Record<QslService, QslRun>>;
  downloads: Partial<Record<"lotw" | "eqsl", QslDownload>>;
  tqsl: { path: string | null; found: boolean; locations: string[] };
}
export interface QslSecrets { qrz_keys?: Record<string, string>; clublog_password?: string; clublog_app_key?: string; lotw_password?: string; eqsl_password?: string }

export type AwardKind = "dxcc" | "was" | "waz" | "wpx";
export type AwardStatus = "worked" | "confirmed";
export interface AwardRow { key: string; name: string; cells: Record<string, AwardStatus> }
export interface AwardColumn { key: string; worked: number; confirmed: number }
export interface AwardTable { award: AwardKind; columns: AwardColumn[]; rows: AwardRow[]; total: number }
