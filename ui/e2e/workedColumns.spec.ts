import { expect, test, type Page } from "@playwright/test";

const qso = (id: number, date: string, band: string, mode: string, name: string) => ({
  id, log_id: 1, location_id: null,
  fields: { CALL: "K1ABC", QSO_DATE: date, TIME_ON: "1200", BAND: band, MODE: mode, NAME: name },
});
const usb = qso(903, "20240401", "15m", "USB", "Ann");
usb.fields = { ...usb.fields, LOTW_QSL_RCVD: "Y" } as typeof usb.fields;
const rows = [qso(901, "20240303", "40m", "SSB", "Ann"), qso(902, "20240106", "20m", "FT8", "Ann"), usb];

// The entry panel needs a callsign, so finish the setup wizard when the data folder is still fresh.
async function setup(page: Page) {
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  if (!(await wizard.waitFor({ timeout: 3000 }).then(() => true, () => false))) return;
  await wizard.getByLabel("Callsign", { exact: true }).fill("n0call");
  await wizard.getByRole("button", { name: "Next" }).click();
  await wizard.getByRole("button", { name: "Skip" }).click();
  await wizard.getByLabel("Grid").fill("EN34");
  await wizard.getByRole("button", { name: "Next" }).click();
  await wizard.getByRole("button", { name: "Next" }).click();
  await wizard.getByRole("button", { name: "Next" }).click();
  await wizard.getByRole("button", { name: "Skip" }).click();
  await wizard.getByRole("button", { name: "Skip" }).click();
  await wizard.getByRole("button", { name: "Start logging" }).click();
}

test("Worked before columns can be moved, resized and hidden, and are remembered", async ({ page }) => {
  await page.route("**/api/logs/*/qsos/search", (r) => r.fulfill({ json: { total: rows.length, rows } }));
  await page.goto("/?token=e2e");
  await setup(page);
  await page.getByRole("tab", { name: "Log", exact: true }).click();
  await page.getByTestId("call").fill("K1ABC");
  const pane = page.locator(".worked-pane");
  const heads = pane.locator("th.col-head");
  await expect(heads.nth(0)).toHaveText("Date", { timeout: 5000 });

  // Drag Band onto the left half of Date.
  const from = (await heads.nth(2).boundingBox())!;
  const to = (await heads.nth(0).boundingBox())!;
  await page.mouse.move(from.x + 12, from.y + 8);
  await page.mouse.down();
  await page.mouse.move(to.x + 4, to.y + 8, { steps: 8 });
  await page.mouse.up();
  await expect(heads.nth(0)).toHaveText("Band");
  await expect(pane.locator("tbody tr").first().locator("td").first()).toHaveText("40m");

  // Resize Date, then hide Name.
  const date = heads.nth(1);
  await expect(date).toHaveText("Date");
  const before = (await date.boundingBox())!.width;
  const e = (await date.locator(".col-resize").boundingBox())!;
  await page.mouse.move(e.x + 3, e.y + 6);
  await page.mouse.down();
  await page.mouse.move(e.x + 43, e.y + 6, { steps: 6 });
  await page.mouse.up();
  expect((await date.boundingBox())!.width).toBeGreaterThan(before + 25);
  await pane.getByRole("button", { name: "Columns", exact: true }).click();
  await pane.getByLabel("Name", { exact: true }).uncheck();
  await expect(pane.locator("th.col-head", { hasText: "Name" })).toHaveCount(0);

  // Still so after a reload; Reset columns puts everything back.
  await page.reload();
  await setup(page);
  await page.getByRole("tab", { name: "Log", exact: true }).click();
  await page.getByTestId("call").fill("K1ABC");
  await expect(heads.nth(0)).toHaveText("Band");
  await expect(pane.locator("th.col-head", { hasText: "Name" })).toHaveCount(0);
  await pane.getByRole("button", { name: "Columns", exact: true }).click();
  await pane.getByRole("button", { name: "Reset columns" }).click();
  await expect(heads.nth(0)).toHaveText("Date");
  await expect(pane.locator("th.col-head", { hasText: "Name" })).toHaveCount(1);

  // The Full summary: USB counts as SSB, and Confirmed lists bands only.
  await pane.getByRole("button", { name: "Full", exact: true }).click();
  const summary = pane.locator(".wb-summary");
  await expect(summary).toContainText("Modes FT8 SSB");
  await expect(summary).toContainText("Confirmed 15m");
  await expect(summary).not.toContainText("USB");
  await pane.getByRole("button", { name: "Full", exact: true }).click();
});
