# Where things live

A module-level map for finding the right code fast. It lists folders and modules, not functions or files within them, so it stays true as code moves inside a module. If a PR adds, renames, moves or removes a module (a Rust file under `crates/`, a UI area, a workflow), update this map in the same PR.

Search tip: each Rust module starts with a `//!` doc comment saying what it is, so `rg '^//!' crates` gives a one-line summary of every module.

## Layout

| Path | What it is |
| --- | --- |
| `crates/core` | Log and pure logic: no network servers, no UI. |
| `crates/radio` | Protocol clients for radios, UDP programs and the cluster. |
| `crates/server` | Local HTTP API (`/api`), the live station state, and the embedded UI. |
| `src-tauri` | Thin desktop shell (window, installer) around the server. |
| `ui` | React + Vite front end. The same UI runs in the app and in a browser. |
| `docs/user` | In-app Help, bundled by `ui/src/components/HelpView.tsx`. |
| `.github/workflows` | `ci.yml` (tests and builds), `release.yml` (installers on tag) and `pages.yml` (publishes `site/`). |
| `site` | The public website (plain HTML/CSS, no build step), deployed to GitHub Pages. Screenshots in `site/img` are copies of `docs/screenshots`. |

## Core (`crates/core/src`)

- Log database, schema and migrations: `store.rs`. ADIF import/export: `adif.rs`. Backups and restore: `backup.rs`. Passwords: `secrets.rs`.
- QSO fields, bands and modes: `model.rs`, `band.rs`.
- DXCC resolution from the country file: `cty.rs`. "Have I worked it?" sets: `worked.rs`.
- Awards (DXCC, WAS, WAZ, WPX and the rest): `awards.rs`. CW club awards (SKCC, CWops, NAQCC, FISTS): `cwclubs.rs`.
- Log statistics (the Statistics window's text summary): `stats.rs`. CQ DX Marathon (countries and zones per year): `marathon.rs`.
- QSL services: uploads in `qsl.rs`; confirmation downloads, eQSL and matching in `confirm.rs`.
- Callsign lookup: `qrz.rs`, `hamqth.rs`.
- Tests: `crates/core/tests` (`perf.rs` is the ignored large-log speed test).

## Radio and UDP integrations (`crates/radio/src`)

- Rig control backends (Hamlib, TCI, Icom, Kenwood, Yaesu, serial): `rig/` (start at `rig/mod.rs`).
- WSJT-X / JTDX: `wsjtx.rs`. N1MM Logger+: `n1mm.rs`. PstRotatorAz: `pst.rs`. Direct rotator control: `rotor.rs`.
- DX cluster client: `cluster.rs`.

## Server (`crates/server/src`)

- API routes, app state and settings: `lib.rs`. Binary entry point: `main.rs`.
- The live station (rig connections, WSJT-X/N1MM/Pst over UDP, event stream): `station.rs`.
- Cluster connection and spot flagging: `cluster.rs`. Watch list: `watch.rs`.
- Online uploads and QSL queues: `qsl.rs`. Label printing: `label.rs`.
- Custom UDP connections and relay: `udp_out.rs`. Programs started with QRZero: `startup.rs`.
- Backups API: `backups.rs`. Update check: `updates.rs`.
- Feeds: `propagation.rs`, `contests.rs`, `dxped.rs`, `mostwanted.rs` (Club Log Most Wanted ranks).
- Tests: `crates/server/tests` (each file is named for its area; `api.rs` is the general one).

## UI (`ui/src`)

- App shell, docked and pop-out layout: `App.tsx`, `components/Workspace.tsx`, `components/Popout.tsx`, `workspace.ts`.
- Frameless windows (QRZero's own title bar, and the title commands it sends the desktop shell): `frame.ts`, `components/WindowControls.tsx`.
- Panes and dialogs: `components/*.tsx`, named for what they show (for example `LogGrid`, `ClusterPane`, `BandMapPane`, `AwardsPane`, `QslDialog`, `StatsDialog`, `SettingsDialog`). Header and radio readout: `MainMenu.tsx`, `VfoBar.tsx`.
- Server calls: `api.ts` (everything goes over `/api`). Live events: `live.ts`. Shared types: `types.ts`.
- Per-feature helpers and saved preferences sit beside the components as `*.ts` and `*.css`. Settings search index: `settingsIndex.ts`. Keyboard shortcuts: `shortcuts.ts`, `shortcutKeys.ts`.
- End-to-end tests: `ui/e2e/*.spec.ts` (Playwright; `smoke.spec.ts` is the broad one, config in `ui/playwright.config.ts`).

## Desktop shell and release

- `src-tauri`: windows (frameless by default, driven by `qrzero:` page-title commands), icons, installer config (`tauri.conf.json`).
- CI jobs (Linux tests plus e2e, Windows installer, macOS build) are in `.github/workflows/ci.yml`; tag-triggered release builds are in `.github/workflows/release.yml`; the website deploy is `.github/workflows/pages.yml`.
