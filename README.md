# QRZero

A fast, reliable amateur radio logger for Windows, built to replace Log4OM 2.

QRZero is a desktop app with a web interface: a small local server holds the log (SQLite) and the window shows its UI. The same server can run on its own and be used from a browser.

**User guide:** [docs/user](docs/user/01-getting-started.md). The same pages are built into the app under **Help**.

## Status

Step 1 of the plan (core log and ADIF) is in place:

- Fast QSO entry with callsign lookup when you leave the call field (QRZ.com XML, cached 30 days)
- Separate logs, several station callsigns per log, and operating locations with a default
- ADIF import (duplicate detection, location assignment, UTF-8 or Windows-1252) and filtered export (standard or full fields)
- A virtualised log grid that stays fast at hundreds of thousands of QSOs

Next: rig control (CAT, Hamlib, TCI, SO2R, Flex slices) and UDP integrations (WSJT-X/JTDX, N1MM, PstRotatorAz), then QSL services and the DX cluster.

## Layout

| Path | What |
| --- | --- |
| `crates/core` | Log database, ADIF reader/writer, QRZ client. No UI or HTTP. |
| `crates/server` | Local HTTP API (axum) with the UI embedded; `qrzero-server` binary. |
| `src-tauri` | Desktop app: starts the server and opens a window on it. |
| `ui` | React + TypeScript UI (Vite). |
| `docs/user` | User guide, shown in the app's Help. |

## Building

Needs Rust (stable) and Node 22.

```sh
cd ui && npm ci && npm run build && cd ..   # UI first: the server embeds ui/dist
cargo test                                   # core and server tests
cargo run -p qrzero-server                   # browser version on http://127.0.0.1:8073
npx @tauri-apps/cli@2 build                  # Windows installer (MSI and setup .exe)
# On Linux first: apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev
# then: npx @tauri-apps/cli@2 build --bundles deb,rpm,appimage
```

UI development with live reload: run `cargo run -p qrzero-server -- --token dev`, then `npm run dev` in `ui/` and open `http://localhost:5173/?token=dev`.

Other checks:

```sh
cargo test --release -p qrzero-core --test perf -- --ignored --nocapture   # 200k-QSO speed check
cd ui && npm run e2e                                                       # browser end-to-end test
```

CI runs all of these and attaches the Windows installer to each run.

## Releases

Pushing a tag builds the Windows and Linux packages and attaches them to that tag's GitHub Release (creating a pre-release if none exists). For a tag that already exists, run **Actions → Release → Run workflow** and enter the tag name. The version comes from the tag: `QRZero-Alpha-v0.12` builds version 0.12.0, and every file name carries it (use plain numbers, up to three parts; a tag with no number builds 0.1.0). Each release gets the MSI, the setup `.exe`, and `QRZero_<version>_portable.exe` (no install; needs the WebView2 runtime that ships with Windows 10 and 11). For Linux it adds a `.deb`, an `.rpm`, an `.AppImage`, and `QRZero-server_<version>_linux-x86_64.tar.gz` (the browser version without the desktop window, for a shack PC or Raspberry Pi-class server reached from a browser).
