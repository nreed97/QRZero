export type Fields = Record<string, string>;

export interface Log { id: number; name: string; qso_count: number }
export interface StationCallsign { id: number; log_id: number; callsign: string; is_default: boolean }
export interface Location { id: number; log_id: number; name: string; is_default: boolean; fields: Fields }
export interface Qso { id: number; log_id: number; location_id: number | null; fields: Fields }

export interface QsoFilter {
  call?: string;
  /** Exact call, plus portable forms of its base call (DL1ABC finds DL1ABC/P). */
  exact_call?: string;
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
  /** The call looked up, uppercase. */
  call: string;
  worked: WorkedBefore;
  entity: Entity | null;
  station: Fields | null;
  source: string | null;
  error: string | null;
  /** The operator's station note for the base call. */
  note: string | null;
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
  /** In split: receiving on freq_hz, transmitting on tx_freq_hz. */
  split: boolean;
  tx_freq_hz: number;
  error: string | null;
}

export interface Needed { new_call: boolean; new_call_band: boolean; new_dxcc: boolean; new_band: boolean; new_mode: boolean; new_grid?: boolean }

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
  /** Id of the watch list entry this station matches. */
  watched?: number | null;
  /** Our own transmission that period, not a decode: message is what was sent, df the Tx offset. */
  tx?: boolean;
}

/** A Configure request to WSJT-X; anything left out stays as it is. */
export interface FtxConfigure {
  mode?: string;
  tr_period?: number;
  rx_df?: number;
  dx_call?: string;
  dx_grid?: string;
  /** Regenerate Tx1-Tx6, as Generate Std Msgs does. */
  generate_messages?: boolean;
}

export interface FtxInstance {
  id: string;
  dial_freq: number;
  band: string | null;
  mode: string;
  de_call: string;
  de_grid: string;
  dx_call: string;
  dx_grid: string;
  /** The report WSJT-X will send. */
  report: string;
  transmitting: boolean;
  tx_enabled: boolean;
  decoding: boolean;
  /** Being sent now, or next (empty from programs older than WSJT-X 2.1). */
  tx_message: string;
  rx_df: number;
  tx_df: number;
  /** The Tx watchdog has stopped transmitting. */
  tx_watchdog: boolean;
  /** Seconds, 0 when not sent. */
  tr_period: number;
  sub_mode: string;
  fast_mode: boolean;
  special_op_mode: number;
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
  /** Look up auto-logged QSOs on QRZ and fill in what they left blank. */
  auto_log_lookup: boolean;
  rotator_enabled: boolean;
  /** "pst" (PstRotatorAz), "rotctld" (Hamlib) or "gs232" (Yaesu GS-232). */
  rotator_kind: string;
  rotator_addr: string;
  rotator_tcp: string;
  rotator_serial: string;
  rotator_baud: number;
  rotator_serve: boolean;
  rotator_serve_addr: string;
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
  /** Where the DX listens when the comment says ("UP 5", "QSX 14.205"). */
  tx_freq_hz?: number | null;
  entity: Entity | null;
  /** Where the spotter is. */
  spotter_entity?: Entity | null;
  needed: Needed | null;
  /** Id of the watch list entry this station matches. */
  watched?: number | null;
}

export interface ClusterNode { name: string; host: string; port: number; login: string; password: string; commands: string[] }
export interface ClusterConfig { nodes: ClusterNode[]; auto_connect: boolean }
export interface ClusterSnapshot { home?: Entity | null; config: ClusterConfig; state: string; connected: boolean; spots: Spot[]; lines: string[] }

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

export type AwardKind = "dxcc" | "was" | "waz" | "wpx" | "wac" | "itu" | "vucc" | "iota" | "counties";
export type AwardStatus = "worked" | "confirmed";
export interface AwardRow { key: string; name: string; cells: Record<string, AwardStatus> }
export interface AwardColumn { key: string; worked: number; confirmed: number }
export interface AwardTable { award: AwardKind; columns: AwardColumn[]; rows: AwardRow[]; total: number }
/** What a QSO would add to one award: its row and the cells it falls in (mixed, mode group, band). */
export interface AwardHint { award: AwardKind; key: string; name: string; cells: { column: string; status: "new" | AwardStatus }[] }

/** DXCC slots of one entity: per band (row), the status of CW, Phone and Digital. */
export interface SlotGrid { bands: string[]; cells: ("new" | AwardStatus)[][] }

/** A station note, stored under the base call. Times are Unix seconds. */
export interface Note {
  call: string;
  text: string;
  created_at: number;
  updated_at: number;
}

export type WatchKind = "call" | "prefix" | "entity";
export interface WatchEntry { id: number; kind: WatchKind; value: string; name: string; bands: string[]; modes: string[]; note: string; enabled: boolean }
export interface WatchHit {
  seq: number;
  entry_id: number;
  label: string;
  note: string;
  call: string;
  freq_hz: number;
  band: string | null;
  mode: string;
  grid: string | null;
  country: string | null;
  source: "cluster" | "ftx";
  detail: string;
  time: number;
}
export interface DxpedNeed { unknown: boolean; new_dxcc: boolean; bands: string[]; modes: string[] }
export interface DxpedSpot { call: string; freq_hz: number; band: string | null; mode: string; comment: string; grid: string | null; time: number }
export interface DxpedItem {
  /** Only hand-added ones have an id. */
  id: number;
  manual: boolean;
  call: string;
  /** YYYY-MM-DD, empty for no date. */
  start: string;
  end: string;
  note: string;
  active: boolean;
  entity: string | null;
  prefix: string | null;
  dxcc: number | null;
  need: DxpedNeed;
  needed: boolean;
  spot: DxpedSpot | null;
}
export interface DxpedList { items: DxpedItem[]; fetched_at: number | null; error: string | null; url: string }
export interface DxpedPlanned { id: number; call: string; start: string; end: string; note: string }
export interface CtyEntityInfo { prefix: string; name: string; dxcc: number | null; cont: string }

/** N0NBH solar data as /api/propagation returns it. */
export interface SolarData {
  source: string;
  updated: string;
  updated_utc: string | null;
  /** Simple readings by the feed's element name ("solarflux", "kindex", ...). */
  values: Record<string, string>;
  bands: { name: string; time: string; condition: string }[];
  vhf: { name: string; location: string; condition: string }[];
}
export interface PropagationReport { data: SolarData | null; fetched_at: string | null; error: string | null }

export type UdpEvent = "qso_logged" | "radio" | "rotator" | "lookup" | "relay_wsjtx" | "relay_n1mm";
export type UdpFormat = "n1mm_radio" | "n1mm_contact" | "adif" | "json" | "pst" | "template";
export interface UdpConnection {
  id: number;
  name: string;
  enabled: boolean;
  host: string;
  port: number;
  event: UdpEvent;
  format: UdpFormat;
  template: string;
  /** Radio events: only this radio (by name); empty for all. */
  radio: string;
}
export interface RunStatus { ok: boolean; text: string }
export interface StartupApp { id: number; enabled: boolean; path: string; args: string; skip_if_running: boolean }
