import { expect, test, type Locator, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import dgram from "node:dgram";

const ADIF = `Test file<EOH>
<CALL:5>K1ABC<QSO_DATE:8>20240105<TIME_ON:4>1830<BAND:3>40m<MODE:3>SSB<NAME:4>José<EOR>
<CALL:5>K1XYZ<QSO_DATE:8>20240106<TIME_ON:4>0215<FREQ:5>7.074<MODE:4>MFSK<SUBMODE:3>FT4<APP_L4ONG_TEST:1>1<EOR>
<CALL:5>K1XYZ<QSO_DATE:8>20240106<TIME_ON:6>021530<FREQ:5>7.074<MODE:4>MFSK<SUBMODE:3>FT4<EOR>
`;

/** Double-clicks a log row and returns the editor window, trying again when a slow runner doesn't open it. */
async function openEditorWindow(page: Page, call: string): Promise<Page> {
  for (let i = 0; i < 3; i++) {
    const popup = page.waitForEvent("popup", { timeout: 10_000 }).catch(() => undefined);
    await page.locator(".grid-row", { hasText: call }).dblclick();
    const win = await popup;
    if (win) return win;
  }
  throw new Error(`the editor window for ${call} didn't open`);
}

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
  await expect(wizard.getByRole("heading", { name: "Confirmations" })).toBeVisible();
  await wizard.getByRole("button", { name: "Skip" }).click(); // confirmations
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
  await fromMenu(page, "Import ADIF…");
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
  const editWin = await openEditorWindow(page, "K1ABC");
  const editor = editWin.locator(".qso-editor");
  const name = editor.getByLabel("Name", { exact: true });
  // On a slow runner the window can come up empty; opening the QSO again sends it again.
  await expect(async () => {
    await expect(name).toHaveValue("José", { timeout: 4000 });
  }).toPass({ timeout: 30_000, intervals: [100] }).catch(async () => {
    await page.locator(".grid-row", { hasText: "K1ABC" }).dblclick();
    await expect(name).toHaveValue("José");
  });
  await name.fill("Jose Maria");
  await expect(editor.locator(".qe-changes")).toHaveText("1 change");
  await editor.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".grid-row", { hasText: "K1ABC" })).toContainText("Jose Maria");
  await expect(editor.getByRole("button", { name: "Save" })).toBeDisabled();
  // The QSO above it in the log, from the editor's arrows.
  await editor.getByRole("button", { name: "Previous QSO" }).click();
  await expect(editor.getByLabel("Call", { exact: true })).not.toHaveValue("K1ABC");
  // Fill from QRZ with no QRZ login says how to turn it on.
  await editor.getByRole("button", { name: "Fill from QRZ" }).click();
  await expect(editor.getByRole("alert")).toContainText("QRZ login");
  await Promise.all([editWin.waitForEvent("close"), closeWindow(editor.getByRole("button", { name: "Close editor" }))]);

  // Right-click a logged QSO for its menu; sending needs a UDP connection first.
  await page.locator(".grid-row", { hasText: "K1ABC" }).click({ button: "right" });
  const menu = page.getByRole("menu");
  await expect(menu.getByRole("menuitem", { name: /Fill .* from QRZ/ })).toBeVisible();
  await menu.getByRole("menuitem", { name: /Send .* through UDP connections/ }).click();
  await expect(menu).toBeHidden();
  await expect(page.locator(".grid > .grid-tools")).toContainText("No UDP connection is set up");

  // Export 40m only, full fields.
  await fromMenu(page, "Export ADIF…");
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
  // The radio's VFO shows in the top bar.
  await expect(page.locator(".topbar").getByTestId("vfo")).toContainText(/14\.074\.000\s*20m\s*FT8\s*RX/);
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

  // The control strip: WSJT-X transmitting its R+report, sent from a socket that then hears
  // what QRZero sends back (requests go to the address the status came from).
  const radio = dgram.createSocket("udp4");
  const heard: Buffer[] = [];
  radio.on("message", (m) => heard.push(m));
  await new Promise<void>((ok) => radio.bind(0, "127.0.0.1", ok));
  const fromRadio = (p: Buffer) => new Promise<void>((ok, err) => radio.send(p, 2237, "127.0.0.1", (e) => (e ? err(e) : ok())));
  const sentKind = (kind: number) => heard.filter((m) => m.readUInt32BE(8) === kind);
  try {
    await fromRadio(fullStatus({ transmitting: true, txMessage: "EA8AB N0CALL R-14", dx: "EA8AB", report: "-14" }));
    const strip = ftx.getByRole("group", { name: /^Control / });
    await expect(strip.getByTestId("ftx-state")).toHaveText("TX");
    await expect(strip.getByTestId("ftx-txmsg")).toHaveText("Tx: EA8AB N0CALL R-14");
    await expect(strip.getByTestId("ftx-stage")).toHaveText("Sending R+report");
    // The rest of WSJT-X's state is under More controls.
    await expect(strip).not.toContainText("Rx 1500 Tx 1210");
    await strip.getByRole("button", { name: "More controls" }).click();
    await expect(strip).toContainText("DX EA8AB IL18");
    await expect(strip).toContainText("Rx 1500 Tx 1210");
    await strip.getByRole("button", { name: "Fewer controls" }).click();
    // Our transmission is a line of its own in the list, and a TX line in the call boxes.
    const txRow = ftx.locator(".ftx-row.tx");
    await expect(txRow).toHaveCount(1);
    await expect(txRow).toContainText("EA8AB N0CALL R-14");
    await expect(txRow).toContainText("1210");
    await ftx.getByRole("combobox").filter({ hasText: "Call boxes" }).selectOption("calls");
    await expect(ftx.locator(".ftx-txline")).toHaveText(/TX: EA8AB N0CALL R-14/);
    await ftx.getByRole("combobox").filter({ hasText: "Call boxes" }).selectOption("lines");

    await strip.getByRole("button", { name: "Halt TX" }).click();
    await expect.poll(() => sentKind(8).length).toBe(1);
    expect(sentKind(8)[0].subarray(-1)[0]).toBe(0); // halt now, not just auto Tx
    await strip.getByRole("button", { name: "Stop after this" }).click();
    await expect.poll(() => sentKind(8).length).toBe(2);
    expect(sentKind(8)[1].subarray(-1)[0]).toBe(1);
    await strip.getByRole("button", { name: "Call CQ" }).click();
    await expect.poll(() => sentKind(9).length).toBe(1);
    expect(sentKind(9)[0].subarray(-15).toString("latin1")).toBe("CQ N0CALL EN34\x01");
    await strip.getByRole("button", { name: "More controls" }).click();
    await strip.getByLabel("DX call").fill("dl1abc");
    await strip.getByRole("button", { name: "Set DX" }).click();
    await expect.poll(() => sentKind(15).length).toBe(1);
    expect(sentKind(15)[0].includes(Buffer.from("DL1ABC"))).toBe(true);

    // WSJT-X stops and waits with Enable Tx still on.
    await fromRadio(fullStatus({ transmitting: false, txMessage: "EA8AB N0CALL R-14", dx: "EA8AB", report: "-14" }));
    await expect(strip.getByTestId("ftx-state")).toHaveText("Tx on");
    await expect(txRow).toHaveCount(1);
  } finally {
    radio.close();
  }
  await page.getByRole("tab", { name: "Log", exact: true }).click();
  // About names the installed version and links to the project.
  await fromMenu(page, "About QRZero");
  const about = page.getByRole("dialog", { name: "About QRZero" });
  await expect(about).toContainText(/Version \d+\.\d+\.\d+/);
  await expect(about.getByRole("link", { name: "Releases and downloads" })).toHaveAttribute("href", "https://github.com/nreed97/QRZero/releases");
  await expect(about.getByRole("link", { name: "Report a problem or suggest a change" })).toHaveAttribute("href", "https://github.com/nreed97/QRZero/issues");
  await about.getByRole("button", { name: "Close" }).click();
  await fromMenu(page, "Settings…");
  // General is the first section and opens by default; the search box jumps to a setting.
  await expect(page.getByRole("heading", { name: "General" })).toBeVisible();
  await page.getByLabel("Search settings").fill("rotor");
  await page.getByRole("option", { name: /Control a rotator/ }).click();
  await expect(page.getByRole("heading", { name: "Radios and programs" })).toBeVisible();
  await expect(page.locator(".setting-hit")).toContainText("Control a rotator");
  await page.getByRole("button", { name: "Radios and programs" }).click();
  await expect(page.getByLabel("Listen to WSJT-X / JTDX")).toBeChecked();

  // A UDP connection from a preset is saved and can send a test message.
  await page.getByRole("button", { name: "UDP connections" }).click();
  await page.getByRole("button", { name: "Rotator program" }).click();
  await expect(page.locator("select[aria-label='Sends when']")).toHaveValue("rotator");
  await expect(page.locator("select[aria-label=Format]")).toHaveValue("pst");
  await page.getByRole("button", { name: "Send test" }).click();
  await expect(page.locator("pre.udp-sent")).toHaveText("<PST><AZIMUTH>45</AZIMUTH></PST>");
  await page.getByRole("button", { name: "Save", exact: true }).click();
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

  // General: colours and text size apply at once; a changed report is used for the mode.
  await page.getByRole("button", { name: "General", exact: true }).click();
  await page.getByLabel("Colours").selectOption("light");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.getByLabel("Text size").selectOption("16");
  await expect(page.locator("body")).toHaveCSS("font-size", "16px");
  await page.getByLabel("Colours").selectOption("system");
  await page.getByLabel("Text size").selectOption("13");
  await page.getByRole("textbox", { name: "CW" }).fill("579");
  await page.getByRole("button", { name: "Reset these options" }).click();
  await expect(page.getByRole("textbox", { name: "CW" })).toHaveValue("");
  await page.getByRole("button", { name: "Close", exact: true }).click();

  // With no cluster set up, the Cluster pane sends you to Settings to add one.
  await page.getByRole("tab", { name: "Cluster" }).click();
  await page.getByRole("button", { name: "Add a cluster…" }).click();
  await page.getByRole("button", { name: "VE7CC" }).click();
  await expect(page.locator("input[aria-label=Host]").first()).toHaveValue("dxc.ve7cc.net");
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("tab", { name: "Log", exact: true }).click();

  // QSL uploads: keys are saved, never shown back.
  await fromMenu(page, /^QSL:/);
  await page.getByRole("button", { name: "QRZ Logbook" }).click();
  await page.getByLabel("QRZ API key for N0OLD").fill("ABCD-1234");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByText("Saved.")).toBeVisible();
  await expect(page.getByLabel("QRZ API key for N0OLD")).toHaveAttribute("placeholder", "API key saved");
  // Upload by date range: the preview lists what would be sent (the logged QSOs are N0CALL's, so none for N0OLD).
  await page.getByText("Upload by date range").click();
  await page.getByRole("button", { name: "Everything not yet sent" }).click();
  await expect(page.getByText(/Nothing waiting for QRZ Logbook/)).toBeVisible();
  // The note fades away, so a second save shows it again; Save and close saves and closes.
  await expect(page.getByText("Saved.")).toBeHidden({ timeout: 6000 });
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByText("Saved.")).toBeVisible();
  await page.getByRole("button", { name: "Save and close" }).click();
  await expect(page.getByRole("button", { name: "Save and close" })).toHaveCount(0);

  // Queue: queue a card from the log, review it in QSL, Queue (details and contact info), print its label, mark it sent.
  await page.locator(".grid-row", { hasText: "K1ABC" }).click();
  await page.getByLabel("Paper QSL").selectOption("queue");
  await expect(page.getByText("1 QSO queued to send")).toBeVisible();
  await fromMenu(page, /^QSL Queue/);
  const queue = page.getByRole("dialog", { name: "QSL" });
  await expect(queue.locator(".paper-queue")).toContainText("K1ABC");
  await expect(queue.locator(".queue-detail")).toContainText("K1ABC");
  await expect(queue.locator(".queue-detail")).toContainText("QSO_DATE");
  await expect(queue.getByRole("button", { name: "Print 1 label" })).toBeVisible();
  await queue.getByLabel("Group by").selectOption("bureau");
  await expect(queue.locator(".paper-queue .pile")).toContainText("1 card");
  await queue.getByLabel("Group by").selectOption("none");
  await expect(queue.locator("tbody input[type=checkbox]:checked")).toHaveCount(0);
  // The label can be saved as an image.
  const [png] = await Promise.all([page.waitForEvent("download"), queue.getByRole("button", { name: "Save label image" }).click()]);
  expect(png.suggestedFilename()).toBe("K1ABC.png");
  // Printing needs a printer: with an unknown one the error says so and nothing is offered as sent.
  await queue.getByText("Label printer").click();
  await queue.getByRole("textbox", { name: "Printer" }).fill("No such printer");
  await queue.getByRole("button", { name: "Print 1 label" }).click();
  await expect(queue.locator(".ql-msg.err")).toContainText(/only set up for Windows|Can't open the printer/);
  await expect(queue.locator(".ql-after")).toHaveCount(0);
  // When the printer takes it, ask whether to mark the QSO sent; the answer takes it out of the queue.
  await page.route("**/api/label/print?*", (r) => r.fulfill({ json: 1 }));
  await queue.getByRole("button", { name: "Print 1 label" }).click();
  await expect(queue.locator(".ql-after")).toContainText("Mark 1 QSO sent?");
  await queue.getByRole("button", { name: "Via bureau", exact: true }).click();
  await expect(queue.getByText("Nothing in the queue.")).toBeVisible();
  await page.getByRole("button", { name: "Close", exact: true }).click();

  // Queueing with one QSO selected queues just that QSO of a station that has two.
  const apiHeaders = { "x-qrzero-token": "e2e" };
  const lid = (await (await page.request.get("/api/logs", { headers: apiHeaders })).json())[0].id;
  for (const time of ["1000", "1100"]) {
    await page.request.post(`/api/logs/${lid}/qsos`, { headers: apiHeaders, data: { location_id: null, fields: { CALL: "K8TWO", QSO_DATE: "20240301", TIME_ON: time, BAND: "20m", MODE: "CW" } } });
  }
  await page.getByTestId("search").fill("K8TWO");
  await expect(page.locator(".grid-row", { hasText: "K8TWO" })).toHaveCount(2);
  await page.locator(".grid-row", { hasText: "K8TWO" }).first().click();
  await page.getByLabel("Paper QSL").selectOption("queue");
  await expect(page.getByText("1 QSO queued to send")).toBeVisible();
  // A card that arrives puts a card back in the queue for you to send.
  await page.getByTestId("search").fill("W1AW");
  await page.locator(".grid-row", { hasText: "W1AW" }).click({ button: "right" });
  await page.getByRole("menuitem", { name: /Card received, queue a reply/ }).click();
  await expect(page.getByText("1 QSO queued to send")).toBeVisible();
  await fromMenu(page, /^QSL Queue/);
  await expect(queue.locator(".paper-queue tbody tr")).toHaveCount(2); // one K8TWO and W1AW, nothing else
  await expect(queue.locator(".paper-queue")).toContainText("W1AW");
  await expect(queue.locator(".paper-queue")).toContainText("K8TWO");
  await expect(queue.locator(".paper-queue")).not.toContainText("K1ABC");
  await queue.locator(".paper-queue tbody tr", { hasText: "W1AW" }).getByText("received").waitFor();
  // Mark the one being looked at sent: the other stays.
  await queue.locator(".paper-queue tbody tr", { hasText: "W1AW" }).click();
  await queue.getByRole("button", { name: "Sent direct" }).click();
  await expect(queue.locator(".paper-queue tbody tr")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await page.getByTestId("search").fill("");

  // OQRS: mark from the right-click menu, then see it in the editor.
  await page.locator(".grid-row", { hasText: "K1ABC" }).click({ button: "right" });
  await page.getByRole("menuitem", { name: /Mark OQRS requested/ }).click();
  const oqrsWin = await openEditorWindow(page, "K1ABC");
  const oqrs = oqrsWin.locator(".qso-editor");
  await expect(oqrs.getByLabel("Club Log OQRS")).toHaveValue("Y");
  await expect(oqrs.getByText("Not confirmed yet. OQRS requested on Club Log.")).toBeVisible();
  await Promise.all([oqrsWin.waitForEvent("close"), closeWindow(oqrs.getByRole("button", { name: "Close editor" }))]);

  // Awards: everything worked so far, nothing confirmed.
  await page.getByRole("tab", { name: "Awards" }).click();
  await page.getByRole("button", { name: "WAZ" }).click();
  await expect(page.locator(".award-table")).toContainText("Zone 40");
  await page.getByRole("tab", { name: "Log", exact: true }).click();

  // Antennas with bands: the QSO panel picks the one for the band.
  await fromMenu(page, "Settings…");
  await page.getByRole("button", { name: "Equipment" }).click();
  for (const [antName, bands] of [["Hex beam", ["20m", "15m"]], ["Dipole", ["40m"]]] as const) {
    await page.getByRole("button", { name: "add antenna" }).click();
    await page.getByLabel("Name (shown when logging)").fill(antName);
    for (const b of bands) await page.getByTestId("antenna-bands").getByLabel(b, { exact: true }).check();
    await page.getByRole("button", { name: "Save" }).click();
  }
  await expect(page.locator(".tree .item", { hasText: "Hex beam" })).toContainText("20 15 m");
  await page.getByRole("button", { name: "Close", exact: true }).click();
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
  // The DXCC slot grid shows when the call resolves to an entity (needs the country file).
  if (await page.getByLabel("DXCC slots", { exact: true }).count()) await expect(page.getByLabel("DXCC slots", { exact: true })).toContainText("CW");
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
  // ...and even with the browser's storage wiped, as happens when the desktop app restarts on a new address.
  await page.waitForTimeout(600); // the copy to the database is saved shortly after a change
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(page.getByRole("button", { name: "Layout: Test layout" })).toBeVisible();
  await expect(page.locator(".ws-group", { has: page.getByRole("tab", { name: "Cluster" }) }).getByRole("tab")).toHaveCount(1);

  // A pane pops out into its own window and docks back when that window closes.
  await page.getByRole("tab", { name: "FTx monitor" }).click();
  const [popup] = await Promise.all([page.waitForEvent("popup"), page.getByRole("button", { name: "Pop out FTx monitor" }).click()]);
  // The WSJT-X fed in above may still be listed, so look for the monitor itself.
  await expect(popup.getByLabel("Period breaks")).toBeVisible();
  await expect(page.getByRole("tab", { name: "FTx monitor" })).toHaveCount(0);
  await closeWindow(popup.getByRole("button", { name: "Dock back" }));
  await expect(page.getByRole("tab", { name: "FTx monitor" })).toBeVisible();

  // Entry fields can differ by mode group: CW gets its own layout with an SKCC box.
  await fromMenu(page, "Settings…");
  await page.getByRole("button", { name: "Entry fields", exact: true }).click();
  await page.getByRole("button", { name: "CW", exact: true }).click();
  await page.getByRole("button", { name: "Set up fields for CW" }).click();
  await page.getByLabel("Add a field to line 2").selectOption("SKCC");
  await page.getByRole("button", { name: "Add", exact: true }).first().click();
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await page.locator("label.f", { has: page.locator("span", { hasText: "Mode" }) }).locator("select").selectOption("CW");
  await expect(page.getByLabel("SKCC")).toBeVisible();
  await page.locator("label.f", { has: page.locator("span", { hasText: "Mode" }) }).locator("select").selectOption("SSB");
  await expect(page.getByLabel("SKCC")).toHaveCount(0);
  await page.locator("label.f", { has: page.locator("span", { hasText: "Mode" }) }).locator("select").selectOption("CW");
  await expect(page.getByLabel("SKCC")).toBeVisible();

  // Backups: back up now, download it, then stage a restore and cancel it.
  await fromMenu(page, "Settings…");
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
  await page.getByRole("button", { name: "Close", exact: true }).click();

  // CW club awards: 100 SKCC members make a Centurion; ten calls in a CWT hour make a CWops test point.
  const adifField = (name: string, v: string) => `<${name}:${v.length}>${v}`;
  const cwQso = (call: string, date: string, time: string, extra: Record<string, string>) =>
    [adifField("CALL", call), adifField("QSO_DATE", date), adifField("TIME_ON", time), adifField("BAND", "20m"), adifField("MODE", "CW"), ...Object.entries(extra).map(([k, v]) => adifField(k, v)), "<EOR>"].join("");
  const clubAdif = ["<EOH>"];
  for (let n = 1; n <= 100; n++) clubAdif.push(cwQso(`W${n}SKC`, "20240201", "120000", { SKCC: `${n}${n % 2 ? "T" : ""}` }));
  for (let n = 1; n <= 40; n++) clubAdif.push(cwQso(`K${n}FST`, "20240301", "120000", { FISTS: String(n), MY_DXCC: "291", DXCC: n <= 30 ? "291" : "223", ...(n === 1 ? { STATE: "OH" } : {}) }));
  for (let n = 1; n <= 30; n++) clubAdif.push(cwQso(`K${n}NAQ`, "20240302", "120000", { NAQCC: String(n) }));
  for (let n = 1; n <= 10; n++) clubAdif.push(cwQso(`K${n}CWT`, "20240103", "130500", { CWOPS: String(1000 + n) }));
  await page.getByRole("tab", { name: "Awards" }).click();
  await page.request.post(`/api/logs/${logId}/import`, { headers, data: Buffer.from(clubAdif.join("\n")) });
  await page.getByRole("button", { name: "SKCC" }).click();
  const centurion = page.locator(".club-table tbody tr", { hasText: "Centurion" }).first();
  await expect(centurion).toContainText("100");
  await expect(centurion).toContainText("x1");
  await expect(centurion).toContainText("2024-02-01");
  await page.getByRole("button", { name: "NAQCC" }).click();
  const friendship = page.locator(".club-table tbody tr", { hasText: "Friendship Club" }).first();
  await expect(friendship).toContainText("30");
  await expect(friendship).toContainText("170 more");
  await page.screenshot({ path: "e2e-results/awards-naqcc.png" });
  await page.getByRole("button", { name: "FISTS" }).click();
  // 30 members at home (1 point) and 10 abroad (2 points) make 50 points.
  await expect(page.getByText("40 different members, 50 points")).toBeVisible();
  await expect(page.locator(".club-table", { hasText: "Silver Century" })).toContainText("50 more");
  await page.screenshot({ path: "e2e-results/awards-fists.png" });
  await page.getByRole("button", { name: "CWops" }).click();
  await expect(page.locator(".club-table", { hasText: "ACA" })).toContainText("2024");
  const cwtYear = page.locator(".club-table", { hasText: "Medal" }).locator("tbody tr", { hasText: "2024" });
  await expect(cwtYear).toContainText("none yet");
  // Points from CWTs worked outside the log are added by hand.
  await page.getByLabel("Year for added CWT points").fill("2024");
  await page.getByLabel("Added CWT points", { exact: true }).fill("5");
  await page.getByRole("button", { name: "Set", exact: true }).click();
  await expect(cwtYear).toContainText("incl. 5 added");
  await page.getByRole("tab", { name: "Log", exact: true }).click();

  // Awards you don't chase can be switched off in Settings; they leave the Awards pane.
  await fromMenu(page, "Settings…");
  await page.getByRole("button", { name: "Awards", exact: true }).click();
  await page.screenshot({ path: "e2e-results/settings-awards-on.png" });
  await page.getByRole("checkbox", { name: /^SKCC/ }).uncheck();
  await page.getByRole("checkbox", { name: /^CWops/ }).uncheck();
  await page.getByRole("checkbox", { name: /^NAQCC/ }).uncheck();
  await page.getByRole("checkbox", { name: /^FISTS/ }).uncheck();
  await page.screenshot({ path: "e2e-results/settings-awards.png" });
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("tab", { name: "Awards" }).click();
  await expect(page.getByRole("button", { name: "WAZ" })).toBeVisible();
  await expect(page.getByRole("button", { name: "SKCC" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "CWops" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "NAQCC" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "FISTS" })).toHaveCount(0);
  await page.screenshot({ path: "e2e-results/awards-no-cw.png" });
  // The choice survives a restart: the desktop app's browser storage is new each launch, so only the database copy counts.
  await expect.poll(async () => JSON.stringify(await (await page.request.get("/api/prefs/ui.local", { headers })).json())).toContain("qrzero.awards_off");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await page.getByRole("tab", { name: "Awards" }).click();
  await expect(page.getByRole("button", { name: "WAZ" })).toBeVisible();
  await expect(page.getByRole("button", { name: "SKCC" })).toHaveCount(0);
  await fromMenu(page, "Settings…");
  await page.getByRole("button", { name: "Awards", exact: true }).click();
  await page.getByRole("button", { name: "Select all" }).click();
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await expect(page.getByRole("button", { name: "SKCC" })).toBeVisible();
  await expect(page.getByRole("button", { name: "NAQCC" })).toBeVisible();
  await expect(page.getByRole("button", { name: "FISTS" })).toBeVisible();
  await page.getByRole("tab", { name: "Log", exact: true }).click();

  // The user guide opens from the top bar.
  await fromMenu(page, /^Help/);
  await expect(page.getByRole("heading", { name: "Getting started" })).toBeVisible();
  await page.screenshot({ path: "e2e-results/help.png" });
  // Links between pages work, and the search box finds text on any page and jumps to it.
  await page.locator("article.doc").getByRole("link", { name: "Radios and rig control" }).click();
  await expect(page.getByRole("heading", { name: "Radios and rig control", level: 1 })).toBeVisible();
  await page.getByLabel("Search the guide").fill("ci-v address");
  await page.screenshot({ path: "e2e-results/help-search.png" });
  await page.getByLabel("Search results").getByRole("button").first().click();
  await expect(page.locator("article.doc mark").first()).toBeVisible();
  await page.screenshot({ path: "e2e-results/help-search-page.png" });
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
/** A full WSJT-X 2.6 status: Enable Tx on, Rx 1500 Hz, Tx 1210 Hz, FT8 15 s periods. */
function fullStatus(o: { transmitting: boolean; txMessage: string; dx: string; report: string }): Buffer {
  const freq = Buffer.alloc(8);
  freq.writeBigUInt64BE(14_074_000n);
  return wsjtx(1, [
    freq, qstr("FT8"), qstr(o.dx), qstr(o.report), qstr("FT8"), bools(true, o.transmitting, false), u32(1500), u32(1210),
    qstr("N0CALL"), qstr("EN34"), qstr("IL18"), bools(false), qstr(""), bools(false), Buffer.from([0]), u32(10), u32(15),
    qstr("Default"), qstr(o.txMessage),
  ]);
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

/** Picks an item from the ☰ menu in the top bar. */
async function fromMenu(page: Page, item: string | RegExp) {
  await page.locator(".topbar").getByRole("button", { name: "Menu" }).click();
  await page.locator(".topbar").getByRole("menuitem", { name: item }).click();
}
