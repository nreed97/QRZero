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

export interface LookupResult {
  worked: WorkedBefore;
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
