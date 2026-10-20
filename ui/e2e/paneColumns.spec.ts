import { expect, test, type Locator, type Page } from "@playwright/test";

const now = Math.floor(Date.now() / 1000);
const entity = { name: "Germany", prefix: "DL", cont: "EU", dxcc: 230, lat: 0, lon: 0 };
const neededSpot = {
  seq: 1, received: now - 20, time: "0015", spotter: "K1TTT", call: "DL1ABC", freq_hz: 14025000, band: "20m", mode: "CW", comment: "up 1",
  entity, needed: { new_dxcc: true, new_band: false, new_mode: false }, goals: null, marathon: null,
};
const cluster = {
  config: { nodes: [], auto_connect: false, spot_max_minutes: 10, spot_comment: "" },
  state: "", connected: true, lines: [], spots: [neededSpot],
};
const dxped = {
  items: [{
    id: 0, manual: false, call: "3Y0K", start: "2026-10-01", end: "2026-11-01", note: "Bouvet", active: true, entity: "Bouvet", prefix: "3Y", dxcc: 24,
    need: { new_dxcc: true, bands: [], modes: [], unknown: false }, needed: true, spot: null,
  }],
  fetched_at: now, error: null, url: "",
};
const contests = {
  items: [{ title: "CQ WW DX", start: now - 3600, end: now + 3600, active: true, modes: ["CW"], link: "", when: "" }],
  fetched_at: now, error: null, url: "",
};

async function closeWizard(page: Page) {
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  if (await wizard.waitFor({ timeout: 3000 }).then(() => true, () => false)) await wizard.getByRole("button", { name: "Close", exact: true }).click();
}

async function drag(page: Page, head: Locator, by: number) {
  await head.scrollIntoViewIfNeeded();
  const before = (await head.boundingBox())!.width;
  const e = (await head.locator(".col-resize").boundingBox())!;
  await page.mouse.move(e.x + 4, e.y + 6);
  await page.mouse.down();
  await page.mouse.move(e.x + 4 + by, e.y + 6, { steps: 6 });
  await page.mouse.up();
  return { before, after: (await head.boundingBox())!.width };
}

test("tables in the other panes have resizable columns", async ({ page }) => {
  await page.route("**/api/cluster", (r) => (r.request().method() === "GET" ? r.fulfill({ json: cluster }) : r.continue()));
  await page.route("**/api/dxpeditions", (r) => (r.request().method() === "GET" ? r.fulfill({ json: dxped }) : r.continue()));
  await page.route("**/api/contests", (r) => (r.request().method() === "GET" ? r.fulfill({ json: contests }) : r.continue()));
  await page.goto("/?token=e2e");
  await closeWizard(page);

  const cases: [string, string, string][] = [
    ["Needed now", ".needed-row.head > span", "Country"],
    ["FTx monitor", ".ftx-row.head > span", "Message"],
    ["DXpeditions", ".dxped-pane .watch-table th", "Last spot"],
    ["Contests", ".watch-table th", "Contest"],
  ];
  for (const [tab, sel, label] of cases) {
    if (!(await page.getByRole("tab", { name: tab }).isVisible())) {
      await page.getByRole("button", { name: /^Layout/ }).click();
      await page.getByRole("menu").getByLabel(tab).check();
      await page.getByRole("button", { name: /^Layout/ }).click();
    }
    await page.getByRole("tab", { name: tab }).click();
    const head = page.locator(sel).filter({ hasText: label }).first();
    await expect(head).toBeVisible();
    const { before, after } = await drag(page, head, -30);
    expect(after, tab).toBeLessThan(before - 15);
  }
});
