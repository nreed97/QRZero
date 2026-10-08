import { expect, test, type Locator } from "@playwright/test";
import { readFileSync } from "node:fs";
import dgram from "node:dgram";

const ADIF = `Test file<EOH>
<CALL:5>K1ABC<QSO_DATE:8>20240105<TIME_ON:4>1830<BAND:3>40m<MODE:3>SSB<NAME:4>José<EOR>
<CALL:5>K1XYZ<QSO_DATE:8>20240106<TIME_ON:4>0215<FREQ:5>7.074<MODE:4>MFSK<SUBMODE:3>FT4<APP_L4ONG_TEST:1>1<EOR>
<CALL:5>K1XYZ<QSO_DATE:8>20240106<TIME_ON:6>021530<FREQ:5>7.074<MODE:4>MFSK<SUBMODE:3>FT4<EOR>
`;

test("first run, log, import, search, edit and export", async ({ page }) => {
  await page.goto("/?token=e2e");

  // First run opens the setup wizard.
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  await wizard.getByLabel("Callsign", { exact: true }).fill("n0call");
  await wizard.getByLabel(/Previous callsigns/).fill("n0old");
  await wizard.getByRole("button", { name: "Next" }).click();
  await wizard.getByRole("button", { name: "Skip" }).click(); // QRZ lookup
  await expect(wizard.getByRole("heading", { name: "Home location" })).toBeVisible();
  await wizard.getByLabel("Grid").fill("EN34");
  await wizard.getByRole("button", { name: "Next" }).click();
  // The home location step also has a Name box; wait for the equipment step.
  await expect(wizard.getByRole("heading", { name: "Equipment" })).toBeVisible();
  await wizard.getByLabel("Name", { exact: true }).fill("K3");
  await wizard.getByLabel("Power W").fill("100");
  await wizard.getByRole("button", { name: "Add" }).click();
  await expect(wizard.getByRole("cell", { name: "K3" })).toBeVisible();
  await wizard.getByRole("button", { name: "Next" }).click();
  await wizard.getByLabel(/Parks and summits/).check();
  await wizard.getByRole("button", { name: "Next" }).click();
  await wizard.getByRole("button", { name: "Skip" }).click(); // import
  await wizard.getByRole("button", { name: "Start logging" }).click();
  await expect(page.getByText("N0CALL · Home")).toBeVisible();
  await expect(page.getByLabel("Their POTA")).toBeVisible();

  // Log a QSO from the keyboard.
  const call = page.getByTestId("call");
  await call.fill("w1aw");
  await call.press("Space");
  await expect(page.locator("#rst-sent")).toBeFocused();
  await page.getByLabel("Grid").fill("FN31pr");
  await expect(page.locator(".map-info")).toContainText("SP");
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
  await expect(page.locator(".grid > .grid-tools")).toContainText("3 QSOs");

  // Show the rig column; the QSO logged from the keyboard recorded the K3 and its power.
  await page.getByRole("button", { name: "Columns" }).click();
  await page.locator(".column-picker label.chip", { hasText: /^Rig$/ }).locator("input").check();
  await page.getByRole("button", { name: "Done" }).click();
  await expect(page.locator(".grid-row", { hasText: "W1AW" })).toContainText("K3");

  // Drag the Rig heading (added last) in front of Call.
  const heads = page.locator(".grid > .grid-head .col-head");
  const rig = await heads.filter({ hasText: /^Rig$/ }).boundingBox();
  const callHead = await heads.filter({ hasText: /^Call$/ }).boundingBox();
  await page.mouse.move(rig!.x + 10, rig!.y + rig!.height / 2);
  await page.mouse.down();
  await page.mouse.move(callHead!.x + 20, callHead!.y + 5, { steps: 8 });
  await page.mouse.move(callHead!.x + 4, callHead!.y + 5, { steps: 4 });
  await page.mouse.up();
  const order = await heads.allInnerTexts();
  expect(order.indexOf("Rig")).toBe(order.indexOf("Call") - 1);

  // Search by call prefix and by wildcard.
  await page.getByTestId("search").fill("K1");
  await expect(page.locator(".grid > .grid-tools")).toContainText("2 QSOs");
  await page.getByTestId("search").fill("*XY*");
  await expect(page.locator(".grid > .grid-tools")).toContainText("1 QSO");
  await expect(page.locator(".grid-row").first()).toContainText("FT4");
  await page.getByTestId("search").fill("");
  await expect(page.locator(".grid > .grid-tools")).toContainText("3 QSOs");

  // Edit a QSO: the editor opens in its own window.
  const [editWin] = await Promise.all([page.waitForEvent("popup"), page.locator(".grid-row", { hasText: "K1ABC" }).dblclick()]);
  const editor = editWin.locator(".qso-editor");
  const name = editor.getByLabel("Name", { exact: true });
  await expect(name).toHaveValue("José");
  await name.fill("Jose Maria");
  await expect(editor.locator(".qe-changes")).toHaveText("1 change");
  await editor.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".grid-row", { hasText: "K1ABC" })).toContainText("Jose Maria");
  await expect(editor.getByRole("button", { name: "Save" })).toBeDisabled();
  // The QSO above it in the log, from the editor's arrows.
  await editor.getByRole("button", { name: "Previous QSO" }).click();
  await expect(editor.getByLabel("Call", { exact: true })).not.toHaveValue("K1ABC");
  await Promise.all([editWin.waitForEvent("close"), closeWindow(editor.getByRole("button", { name: "Close editor" }))]);

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

  // The FTx monitor waits for WSJT-X, and the integrations are set up in Settings.
  await page.getByRole("tab", { name: "FTx monitor" }).click();
  await expect(page.getByText("Waiting for WSJT-X or JTDX")).toBeVisible();

  // Decodes from a WSJT-X: two periods, shown as lines with period breaks, then as call boxes.
  await sendWsjtx([
    status(),
    decode(45000, -10, "CQ JA1XYZ PM95"),
    decode(45000, -12, "N0CALL EA8AB IL18"),
    decode(60000, -3, "CQ DL1ABC JO62"),
    decode(60000, -14, "N0CALL EA8AB R-10"),
  ]);
  const ftx = page.locator(".ftx").first();
  await expect(ftx.locator(".ftx-row:not(.head)")).toHaveCount(4);
  await ftx.getByLabel("Period breaks").check();
  await expect(ftx.locator(".ftx-break")).toHaveCount(2);
  await expect(ftx.locator(".ftx-break").first()).toContainText("00:01:00 2 decodes, 2 calls");
  await ftx.getByRole("combobox").filter({ hasText: "Call boxes" }).selectOption("calls");
  const first = ftx.locator(".ftx-cycle").first().locator(".ftx-box");
  // With a country file (CI downloads one) DL1ABC is also a new DXCC.
  await expect(first).toHaveText([/^EA8ABME(DXCC)?-14$/, /^DL1ABC(DXCC)?-3$/]);
  await expect(first.first()).toHaveClass(/alerted/);
  await expect(first.nth(1)).toHaveClass(/cq/);
  await first.nth(1).click();
  await expect(page.getByTestId("call")).toHaveValue("DL1ABC");
  // Alerts and filters: ignoring a call hides it; one calling you always shows.
  await ftx.getByRole("button", { name: "Alerts and filters…" }).click();
  const alerts = page.getByRole("dialog", { name: "FTx alerts and filters" });
  await alerts.getByLabel("Calls to ignore").fill("DL1*, EA8AB");
  await expect(first).toHaveText([/^EA8ABME(DXCC)?-14$/]);
  await alerts.getByRole("button", { name: "Reset" }).click();
  await alerts.getByRole("button", { name: "Done" }).click();
  await expect(first).toHaveCount(2);
  await ftx.getByRole("combobox").filter({ hasText: "Call boxes" }).selectOption("lines");
  await ftx.getByLabel("Period breaks").uncheck();
  await page.getByRole("tab", { name: "Log", exact: true }).click();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Radios and programs" }).click();
  await expect(page.getByLabel("Listen to WSJT-X / JTDX")).toBeChecked();

  // A UDP connection from a preset is saved and can send a test message.
  await page.getByRole("button", { name: "UDP connections" }).click();
  await page.getByRole("button", { name: "Rotator program" }).click();
  await expect(page.locator("select[aria-label='Sends when']")).toHaveValue("rotator");
  await expect(page.locator("select[aria-label=Format]")).toHaveValue("pst");
  await page.getByRole("button", { name: "Send test" }).click();
  await expect(page.locator("pre.udp-sent")).toHaveText("<PST><AZIMUTH>45</AZIMUTH></PST>");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Saved.")).toBeVisible();
  await page.getByRole("button", { name: "Startup programs" }).click();
  await page.getByRole("button", { name: "UDP connections" }).click();
  await expect(page.locator("input[aria-label=Name]")).toHaveValue("Rotator");

  // Startup programs: a missing program says why it didn't start.
  await page.getByRole("button", { name: "Startup programs" }).click();
  await page.getByRole("button", { name: "Add a program" }).click();
  await page.getByLabel("Program", { exact: true }).fill("/no/such/program");
  await page.getByRole("button", { name: "Launch now" }).click();
  await expect(page.locator("table.startup-apps")).toContainText("couldn't start");
  await page.getByRole("button", { name: "Close" }).click();

  // With no cluster set up, the Cluster pane sends you to Settings to add one.
  await page.getByRole("tab", { name: "Cluster" }).click();
  await page.getByRole("button", { name: "Add a cluster…" }).click();
  await page.getByRole("button", { name: "VE7CC" }).click();
  await expect(page.locator("input[aria-label=Host]").first()).toHaveValue("dxc.ve7cc.net");
  await page.getByRole("button", { name: "Close" }).click();
  await page.getByRole("tab", { name: "Log", exact: true }).click();

  // QSL uploads: keys are saved, never shown back.
  await page.getByRole("button", { name: "QSL", exact: true }).click();
  await page.getByLabel("QRZ API key for N0OLD").fill("ABCD-1234");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Saved.")).toBeVisible();
  await expect(page.getByLabel("QRZ API key for N0OLD")).toHaveAttribute("placeholder", "API key saved");
  await page.getByRole("button", { name: "Close" }).click();

  // Paper cards: queue one from the log, then find it in QSL, Paper cards.
  await page.locator(".grid-row", { hasText: "K1ABC" }).click();
  await page.getByLabel("Paper QSL").selectOption("queue");
  await page.getByRole("button", { name: "QSL", exact: true }).click();
  await page.getByRole("button", { name: "Paper cards" }).click();
  await expect(page.locator(".paper-queue")).toContainText("K1ABC");
  await expect(page.getByRole("button", { name: "Print 1 label" })).toBeVisible();
  await page.getByRole("button", { name: "Close" }).click();

  // Awards: everything worked so far, nothing confirmed.
  await page.getByRole("tab", { name: "Awards" }).click();
  await page.getByRole("button", { name: "WAZ" }).click();
  await expect(page.locator(".award-table")).toContainText("Zone 40");
  await page.getByRole("tab", { name: "Log", exact: true }).click();

  // Antennas with bands: the QSO panel picks the one for the band.
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Equipment" }).click();
  for (const [antName, bands] of [["Hex beam", ["20m", "15m"]], ["Dipole", ["40m"]]] as const) {
    await page.getByRole("button", { name: "add antenna" }).click();
    await page.getByLabel("Name (shown when logging)").fill(antName);
    for (const b of bands) await page.getByTestId("antenna-bands").getByLabel(b, { exact: true }).check();
    await page.getByRole("button", { name: "Save" }).click();
  }
  await expect(page.locator(".tree .item", { hasText: "Hex beam" })).toContainText("20 15 m");
  await page.getByRole("button", { name: "Close" }).click();
  const ant = page.getByTestId("gear-antenna");
  const bandPick = page.locator(".entry label.f", { has: page.locator("span", { hasText: /^Band$/ }) }).locator("select");
  await bandPick.selectOption("20m");
  await expect(ant.locator("option:checked")).toHaveText("Auto: Hex beam");
  await bandPick.selectOption("6m");
  await expect(ant.locator("option:checked")).toHaveText("Auto: none for 6m");
  await bandPick.selectOption("40m");
  await expect(ant.locator("option:checked")).toHaveText("Auto: Dipole");
  await call.fill("K9ANT");
  await call.press("Enter");
  await expect(page.getByText("Logged K9ANT on 40m")).toBeVisible();
  const headers = { "x-qrzero-token": "e2e" };
  const logId = (await (await page.request.get("/api/logs", { headers })).json())[0].id;
  const found = await (await page.request.post(`/api/logs/${logId}/qsos/search`, { headers, data: { filter: { call: "K9ANT" } } })).json();
  expect(found.rows[0].fields.MY_ANTENNA).toBe("Dipole");

  // Worked before lists earlier QSOs with the call; double-click one to edit it beside the log.
  await call.fill("K1XYZ");
  await call.press("Tab");
  const worked = page.locator(".worked-pane");
  await expect(worked.locator("tbody tr")).toHaveCount(1);
  const [editWin2] = await Promise.all([page.waitForEvent("popup"), worked.locator("tbody tr").first().dblclick()]);
  await expect(editWin2.getByLabel("Call", { exact: true })).toHaveValue("K1XYZ");
  await Promise.all([editWin2.waitForEvent("close"), closeWindow(editWin2.getByRole("button", { name: "Close editor" }))]);

  // A note on a station shows again next time the call is typed.
  await page.getByRole("tab", { name: "Notes" }).click();
  await page.getByLabel("Note for K1XYZ").fill("Runs 5 W\nQSL direct only");
  await expect(page.locator(".notes-pane, .ws-pane").getByText(/Saved \d/).first()).toBeVisible();
  await call.fill("");
  await call.fill("K1XYZ");
  await call.press("Tab");
  await expect(page.locator(".lookup-note")).toContainText("Runs 5 W / QSL direct only");
  // The Station pane says what the QSO would add to awards (K1 is already worked).
  await expect(page.getByLabel("Award hints", { exact: true })).toContainText("WPX");
  await page.getByRole("tab", { name: "Worked before" }).click();

  // Panes: drag the Cluster tab beside the Station pane, it gets its own group.
  const station = page.locator(".ws-group", { has: page.getByRole("tab", { name: "Station" }) }).locator(".ws-body");
  const box = (await station.boundingBox())!;
  await page.getByRole("tab", { name: "Cluster" }).dragTo(station, { targetPosition: { x: box.width - 10, y: box.height / 2 } });
  const clusterGroup = page.locator(".ws-group", { has: page.getByRole("tab", { name: "Cluster" }) });
  await expect(clusterGroup.getByRole("tab")).toHaveCount(1);

  // Layouts are saved by name and come back after a reload.
  page.once("dialog", (d) => d.accept("Test layout"));
  await page.getByRole("button", { name: /^Layout/ }).click();
  await page.getByRole("menuitem", { name: "Save layout as…" }).click();
  await page.keyboard.press("Escape");
  await page.reload();
  await expect(page.getByRole("button", { name: "Layout: Test layout" })).toBeVisible();
  await expect(page.locator(".ws-group", { has: page.getByRole("tab", { name: "Cluster" }) }).getByRole("tab")).toHaveCount(1);

  // A pane pops out into its own window and docks back when that window closes.
  await page.getByRole("tab", { name: "FTx monitor" }).click();
  const [popup] = await Promise.all([page.waitForEvent("popup"), page.getByRole("button", { name: "Pop out FTx monitor" }).click()]);
  // The WSJT-X fed in above may still be listed, so look for the monitor itself.
  await expect(popup.getByLabel("Period breaks")).toBeVisible();
  await expect(page.getByRole("tab", { name: "FTx monitor" })).toHaveCount(0);
  await popup.getByRole("button", { name: "Dock back" }).click();
  await expect(page.getByRole("tab", { name: "FTx monitor" })).toBeVisible();

  // Backups: back up now, download it, then stage a restore and cancel it.
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Backups", exact: true }).click();
  await page.getByRole("button", { name: "Back up now" }).click();
  await expect(page.getByText(/^Saved qrzero-.*-manual\.db\.$/)).toBeVisible();
  const backupRow = page.locator("table.backups tbody tr", { hasText: "Manual" });
  await expect(backupRow).toHaveCount(1);
  const [backupFile] = await Promise.all([page.waitForEvent("download"), backupRow.getByRole("button", { name: "Download" }).click()]);
  expect(backupFile.suggestedFilename()).toMatch(/^qrzero-.*-manual\.db$/);
  expect(readFileSync((await backupFile.path())!).subarray(0, 15).toString()).toBe("SQLite format 3");
  await page.getByTestId("restore-file").setInputFiles({ name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("not a log") });
  await expect(page.getByText(/isn't a QRZero log/)).toBeVisible();
  page.once("dialog", (d) => d.accept());
  await backupRow.getByRole("button", { name: "Restore" }).click();
  await expect(page.getByText("Restart QRZero to finish restoring.")).toBeVisible();
  await page.getByRole("button", { name: "Cancel restore" }).click();
  await expect(page.getByText("Restore cancelled.")).toBeVisible();
  await expect(page.getByText("Restart QRZero to finish restoring.")).toHaveCount(0);
  await page.getByRole("button", { name: "Close" }).click();

  // The user guide opens from the top bar.
  await page.getByRole("button", { name: "Help" }).click();
  await expect(page.getByRole("heading", { name: "Getting started" })).toBeVisible();
  await page.screenshot({ path: "e2e-results/help.png" });
});

/** A WSJT-X UDP datagram: magic, schema 2, message type and instance id, then the fields. */
function wsjtx(kind: number, fields: Buffer[]): Buffer {
  const head = Buffer.alloc(12);
  head.writeUInt32BE(0xadbccbda, 0);
  head.writeUInt32BE(2, 4);
  head.writeUInt32BE(kind, 8);
  return Buffer.concat([head, qstr("WSJT-X"), ...fields]);
}
function qstr(s: string): Buffer {
  const b = Buffer.from(s, "utf8");
  const len = Buffer.alloc(4);
  len.writeUInt32BE(b.length);
  return Buffer.concat([len, b]);
}
const u32 = (n: number) => { const b = Buffer.alloc(4); b.writeUInt32BE(n); return b; };
const i32 = (n: number) => { const b = Buffer.alloc(4); b.writeInt32BE(n); return b; };
const bools = (...v: boolean[]) => Buffer.from(v.map(Number));
function status(): Buffer {
  const freq = Buffer.alloc(8);
  freq.writeBigUInt64BE(14_074_000n);
  return wsjtx(1, [freq, qstr("FT8"), qstr(""), qstr(""), qstr("FT8"), bools(false, false, true), u32(1500), u32(1500), qstr("N0CALL"), qstr("EN34"), qstr("")]);
}
function decode(timeMs: number, snr: number, message: string): Buffer {
  const dt = Buffer.alloc(8);
  dt.writeDoubleBE(0.1);
  return wsjtx(2, [bools(true), u32(timeMs), i32(snr), dt, u32(1000 + snr * -10), qstr("~"), qstr(message), bools(false, false)]);
}
async function sendWsjtx(packets: Buffer[]) {
  const sock = dgram.createSocket("udp4");
  for (const p of packets) await new Promise<void>((ok, err) => sock.send(p, 2237, "127.0.0.1", (e) => (e ? err(e) : ok())));
  sock.close();
}

/** Clicks a button that closes its own window: the click can lose the race with the close and report the page gone. */
async function closeWindow(button: Locator) {
  await button.click({ noWaitAfter: true }).catch((e: Error) => {
    if (!/closed/.test(e.message)) throw e;
  });
}
