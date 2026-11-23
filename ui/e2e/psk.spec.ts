import { expect, test, type Page } from "@playwright/test";

const now = Math.floor(Date.now() / 1000);
const entity = { name: "Falkland Islands", prefix: "VP8", cont: "SA", dxcc: 141, lat: 0, lon: 0 };
const needed = { new_call: true, new_call_band: true, new_dxcc: false, new_band: true, new_mode: false };
const spot = (call: string, khz: number, tuneKhz: number, band: string, mode: string, age: number, receivers: number, best: number) => ({
  call, band, mode, freq_hz: khz * 1000, tune_hz: tuneKhz * 1000, first: now - age - 600, last: now - age, reports: receivers + 2, receivers,
  best_snr: best, last_snr: best - 4, last_receiver: "K1ABC", last_receiver_grid: "FN42aa", grid: "GD18", entity, needed,
});
const config = { enabled: false, calls: [] as string[], minutes: 30, host: "mqtt.pskreporter.info", port: 1883 };

async function closeWizard(page: Page) {
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  if (await wizard.waitFor({ timeout: 3000 }).then(() => true, () => false)) await wizard.getByRole("button", { name: "Close", exact: true }).click();
}

async function openPane(page: Page) {
  await page.goto("/?token=e2e");
  await closeWizard(page);
  if (!(await page.getByRole("tab", { name: "PSK Reporter" }).isVisible())) {
    await page.getByRole("button", { name: /^Layout/ }).click();
    await page.getByRole("menu").getByLabel("PSK Reporter").check();
    await page.getByRole("button", { name: /^Layout/ }).click();
  }
  await page.getByRole("tab", { name: "PSK Reporter" }).click();
  return page.locator(".cluster", { has: page.getByLabel("PSK Reporter spots", { exact: true }) });
}

test("PSK Reporter feed is off by default and shows the calls looked for once on", async ({ page }) => {
  let view = { config, state: "off", connected: false, spots: [] as ReturnType<typeof spot>[] };
  const saved: (typeof config)[] = [];
  await page.route("**/api/psk", async (r) => {
    if (r.request().method() === "PUT") {
      const c = r.request().postDataJSON();
      saved.push(c);
      view = {
        config: c,
        state: c.enabled ? "connected to mqtt.pskreporter.info" : "off",
        connected: c.enabled,
        spots: c.enabled ? [spot("VP8PJ", 14075.6, 14074, "20m", "FT8", 30, 14, -3), spot("VP8PJ", 10106.0, 10106.0, "30m", "CW", 1200, 3, 18)] : [],
      };
    }
    return r.fulfill({ json: view });
  });
  const pane = await openPane(page);
  await expect(pane.getByLabel("Feed on")).not.toBeChecked();
  await expect(pane).toContainText("Enter the calls to look for");

  await pane.getByLabel("Calls to look for").fill("vp8pj");
  await pane.getByLabel("Calls to look for").press("Enter");
  await expect.poll(() => saved.at(-1)?.calls).toEqual(["VP8PJ"]);
  expect(saved.at(-1)?.enabled).toBe(false);
  await pane.getByLabel("Feed on").check();
  await expect.poll(() => saved.at(-1)?.enabled).toBe(true);

  const rows = pane.locator(".spot-row:not(.head)");
  await expect(rows).toHaveCount(2);
  // Heard in the last few minutes: highlighted. Not for 20 minutes: greyed.
  await expect(rows.first()).toHaveClass(/psk-live/);
  await expect(rows.first()).toContainText("14074.0");
  await expect(rows.first()).toContainText("New band");
  await expect(rows.nth(1)).toHaveClass(/psk-stale/);
  await expect(pane).toContainText("1 on now, 2 in all");
  await pane.screenshot({ path: "e2e-results/psk-reporter.png" });

  // A click tunes: with Tune all radios and none connected, the pane says so.
  await pane.getByLabel("Tune all radios").check();
  await rows.first().click();
  await expect(pane).toContainText("No radio that QRZero can tune is connected.");
});
