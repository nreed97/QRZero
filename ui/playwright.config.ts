import { defineConfig } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Runs the real server (with the built UI embedded) against a throwaway data folder.
// Build the UI first: `npm run build && npm run e2e`.
const dataDir = mkdtempSync(join(tmpdir(), "qrzero-e2e-"));
const port = 8099;

export default defineConfig({
  testDir: "e2e",
  // The smoke test is one long walk through the app; CI runners are slower than a desktop.
  timeout: 90_000,
  // On CI, failures are also posted as annotations so they show without downloading logs.
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    launchOptions: process.env.PW_CHROMIUM ? { executablePath: process.env.PW_CHROMIUM } : {},
  },
  webServer: {
    command: `cargo run -q -p qrzero-server -- --data-dir "${dataDir}" --bind 127.0.0.1:${port} --token e2e`,
    cwd: "..",
    url: `http://127.0.0.1:${port}/`,
    timeout: 300_000,
    reuseExistingServer: false,
  },
});
