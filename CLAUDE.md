# Notes for working on QRZero

- Keep `docs/user/*.md` current with every user-visible change: it is the in-app Help (bundled by `ui/src/components/HelpView.tsx`).
- QSOs store all ADIF fields as JSON (`qsos.fields`), the source of truth. The other `qsos` columns are indexed copies; recompute them via `Columns::from_fields` when fields change. Schema changes go through `Store::migrate` with a new `user_version`.
- Speed matters (the user left Log4OM 2 because it got slow). Run the perf test after touching queries; interactive queries must stay under 100 ms at 200k QSOs.
- Passwords go through `qrzero_core::secrets` (Windows Credential Manager), never plain settings.
- The UI talks to the server only over `/api` with the `x-qrzero-token` header; no Tauri IPC, so the same UI works in a browser.
- Windows is the primary platform. `cargo check --target x86_64-pc-windows-gnu` catches Windows-only compile errors locally.
- Every PR that changes the UI includes screenshots of the change in its description (push them to a throwaway branch such as `pr-screenshots-<n>` and link them with `?raw=true`).
