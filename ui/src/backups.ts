// Backups of the log database: /api/backups.
import { request } from "./api";

export type BackupKind = "auto" | "manual" | "before_restore";
export interface Backup { name: string; size: number; created: number; kind: BackupKind }
export interface DbSummary { schema: number; logs: number; qsos: number }
export interface PendingRestore { source: string; staged_at: number; summary: DbSummary }
export interface BackupSettings { auto: boolean; keep: number }
export interface BackupOverview {
  folder: string;
  backups: Backup[];
  settings: BackupSettings;
  pending: PendingRestore | null;
  last_restore: { ok: boolean; message: string } | null;
}

const json = async <T>(method: string, path: string, body?: unknown): Promise<T> => (await request(method, path, body)).json();
const file = (name: string) => `/backups/files/${encodeURIComponent(name)}`;

export const backups = {
  overview: () => json<BackupOverview>("GET", "/backups"),
  saveSettings: (s: BackupSettings) => json<BackupOverview>("PUT", "/backups/settings", s),
  create: () => json<Backup>("POST", "/backups"),
  remove: (name: string) => json<null>("DELETE", file(name)),
  /** The backup file itself, fetched with the session token. */
  download: async (name: string) => (await request("GET", file(name))).blob(),
  restore: (name: string) => json<PendingRestore>("POST", `${file(name)}/restore`),
  restoreUpload: async (f: File) =>
    (await request("POST", `/backups/restore?${new URLSearchParams({ name: f.name })}`, undefined, f)).json() as Promise<PendingRestore>,
  cancelRestore: () => json<null>("DELETE", "/backups/pending"),
};
