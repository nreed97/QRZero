import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";

const ADIF = `Test file<EOH>
<CALL:5>K1ABC<QSO_DATE:8>20240105<TIME_ON:4>1830<BAND:3>40m<MODE:3>SSB<NAME:4>José<EOR>
<CALL:5>K1XYZ<QSO_DATE:8>20240106<TIME_ON:4>0215<FREQ:5>7.074<MODE:4>MFSK<SUBMODE:3>FT4<APP_L4ONG_TEST:1>1<EOR>
<CALL:5>K1XYZ<QSO_DATE:8>20240106<TIME_ON:6>021530<FREQ:5>7.074<MODE:4>MFSK<SUBMODE:3>FT4<EOR>
`;

test("first run, log, import, search, edit and export", async ({ page }) => {
  await page.goto("/?token=e2e");

  // First run asks for a callsign and location.
  await page.getByLabel("Your callsign").fill("n0call");
  await page.getByLabel("Grid").fill("EN34");
  await page.getByRole("button", { name: "Start logging" }).click();
  await expect(page.getByText("Logging as N0CALL from Home")).toBeVisible();

  // Log a QSO from the keyboard.
  const call = page.getByTestId("call");
  await call.fill("w1aw");
  await call.press("Space");
  await expect(page.locator("#rst-sent")).toBeFocused();
  await page.locator("#rst-sent").press("Enter");
  await expect(page.getByText("Logged W1AW on 20m CW")).toBeVisible();
  await expect(call).toBeFocused();
  await expect(page.locator(".grid-row").first()).toContainText("W1AW");

  // Working the same call again shows the history and a duplicate warning.
  await call.fill("W1AW");
  await call.press("Tab");
  await expect(page.getByText("Already worked on 20m CW")).toBeVisible();
  await call.press("Escape");

  // Import an ADIF file into the default location.
  await page.getByRole("button", { name: "Import" }).click();
  await page.getByTestId("import-file").setInputFiles({ name: "test.adi", mimeType: "text/plain", buffer: Buffer.from(ADIF) });
  await page.getByRole("dialog").getByRole("button", { name: "Import" }).click();
  await expect(page.getByText("QSOs imported")).toContainText("2");
  await expect(page.getByText("1 duplicates skipped")).toBeVisible();
  await page.getByRole("button", { name: "Done" }).click();
  await expect(page.locator(".grid-tools")).toContainText("3 QSOs");

  // Search by call prefix and by wildcard.
  await page.getByTestId("search").fill("K1");
  await expect(page.locator(".grid-tools")).toContainText("2 QSOs");
  await page.getByTestId("search").fill("*XY*");
  await expect(page.locator(".grid-tools")).toContainText("1 QSO");
  await expect(page.locator(".grid-row").first()).toContainText("FT4");
  await page.getByTestId("search").fill("");
  await expect(page.locator(".grid-tools")).toContainText("3 QSOs");

  // Edit a QSO.
  await page.locator(".grid-row", { hasText: "K1ABC" }).dblclick();
  const name = page.getByRole("dialog").locator(".kv-row", { has: page.locator('input[value="NAME"]') }).locator("input").nth(1);
  await expect(name).toHaveValue("José");
  await name.fill("Jose Maria");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".grid-row", { hasText: "K1ABC" })).toContainText("Jose Maria");

  // Export 40m only, full fields.
  await page.getByRole("button", { name: "Export", exact: true }).click();
  await page.getByLabel("Choose filters").check();
  await page.locator("label.chip", { hasText: /^40m$/ }).locator("input").check();
  await page.getByLabel(/Full: every stored field/).check();
  await expect(page.getByText("2 QSOs will be exported")).toBeVisible();
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("dialog").getByRole("button", { name: "Export" }).click(),
  ]);
  const text = readFileSync((await download.path())!, "utf8");
  expect(text).toContain("<CALL:5>K1ABC");
  expect(text).toContain("<APP_L4ONG_TEST:1>1");
  expect(text).toContain("<MY_GRIDSQUARE:4>EN34");
  expect(text).not.toContain("W1AW");

  // The user guide opens from the top bar.
  await page.getByRole("button", { name: "Help" }).click();
  await expect(page.getByRole("heading", { name: "Getting started" })).toBeVisible();
  await page.screenshot({ path: "e2e-results/help.png" });
});
