# Notes for working on QRZero

- `docs/code-map.md` says where things live (module and folder level). Read it before searching, and update it in any PR that adds, renames, moves or removes a module.
- Keep `docs/user/*.md` current with every user-visible change: it is the in-app Help (bundled by `ui/src/components/HelpView.tsx`).
- QSOs store all ADIF fields as JSON (`qsos.fields`), the source of truth. The other `qsos` columns are indexed copies; recompute them via `Columns::from_fields` when fields change. Schema changes go through `Store::migrate` with a new `user_version`.
- Speed matters (the user left Log4OM 2 because it got slow). Run the perf test after touching queries; interactive queries must stay under 100 ms at 200k QSOs.
- Passwords go through `qrzero_core::secrets` (Windows Credential Manager), never plain settings.
- The UI talks to the server only over `/api` with the `x-qrzero-token` header; no Tauri IPC, so the same UI works in a browser.
- Windows is the primary platform. `cargo check --target x86_64-pc-windows-gnu` catches Windows-only compile errors locally.
- Every PR that changes the UI includes screenshots of the change in its description (push them to a throwaway branch such as `pr-screenshots-<n>` and link them with `?raw=true`).
- Keep the app lightweight: weigh the startup, memory and CPU cost of every feature, and prefer cheap defaults (no polling or timers that nothing needs).
- The UI is plain and classic, in the style of a desktop logger: dense panels, small radii, system fonts, no gradients or decorative emoji. It must not look generated.
- Do not mention Log4OM 2 or other loggers in `README.md` or `docs/user/*.md`, and do not copy another logger's features directly.

## Checks before opening a PR

- `cargo test`, `cargo clippy --all-targets -- -D warnings` and `cargo check --target x86_64-pc-windows-gnu`.
- The perf test is `#[ignore]`d, so `cargo test` skips it: `cargo test --release -p qrzero-core --test perf -- --ignored --nocapture` (CI runs it too).
- In `ui/`: `npx tsc --noEmit -p .`, `npm run build` (the e2e server embeds the built UI), then `npm run e2e`. In a cloud sandbox use `PW_CHROMIUM=/opt/pw-browsers/chromium npm run e2e` and never run `playwright install`.
- The e2e server listens on UDP 2237; stop any stray `qrzero-server` first or tests fail falsely.
- The smoke test "editor window opens after double-click" is flaky on CI; it has a retry, so a failure there alone is worth one look, not a rewrite.
- Tests that start a server must give it their own `secret_service`, because Windows CI tests share one Credential Manager. Background tasks hold `Weak` references to app state, or the SQLite file stays open after shutdown and backup restore fails on Windows.
- CI has network, so the country file loads there and decodes get DXCC tags that local runs do not. Write e2e expectations that allow both.

## Releases

Pushing a tag builds the installers, and the app version comes from the tag's number (`QRZero-v1.0` becomes 1.0.0; the update check needs the `v` before the number), so every release needs a new number. Cloud sessions cannot push tags; Nick creates them.
