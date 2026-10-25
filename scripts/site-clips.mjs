// Records the short demo clips on the website (site/clips) from the real app, in dark mode, with made-up data.
//
//   cd ui && npm run build && cargo build -p qrzero-server   (the server embeds the built UI)
//   node scripts/site-clips.mjs [outdir]                     (from the repo root; needs ffmpeg)
//
// It starts a throwaway server on its own data folder, fills it with a demo log, a fake DX cluster and
// fake WSJT-X decodes, drives the UI with Playwright and converts the recordings to webm and mp4.
import { createRequire } from "node:module";
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import net from "node:net";
import dgram from "node:dgram";

const root = resolve(import.meta.dirname, "..");
const require = createRequire(join(root, "ui", "package.json"));
const { chromium } = require("playwright");

const out = resolve(process.argv[2] ?? join(root, "site", "clips"));
const work = mkdtempSync(join(tmpdir(), "qrzero-clips-"));
const PORT = 8199;
const CLUSTER_PORT = 7399;
const base = `http://127.0.0.1:${PORT}`;
const W = 1000, H = 563;
mkdirSync(out, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- a small country file, since the real one is downloaded and the demo should not need the network ------
const CTY = `K,United States,291,NA,5,8,37.53,91.67,5.0,AA AB AC AD AE AF AG AI AJ AK K N W;
G,England,223,EU,14,27,52.77,1.47,0.0,2E G M;
DL,Fed. Rep. of Germany,230,EU,14,28,51.00,-10.00,-1.0,DA DB DC DD DE DF DG DH DI DJ DK DL DM DN DO DP DQ DR Y2 Y3 Y4 Y5 Y6 Y7 Y8 Y9;
JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,7J 7K 7L 7M 7N 8J 8K 8L 8M 8N JA JE JF JG JH JI JJ JK JL JM JN JO JP JQ JR JS;
EA,Spain,281,EU,14,37,40.37,4.88,-1.0,AM AN AO EA EB EC ED EE EF EG EH;
EA8,Canary Islands,29,AF,33,36,28.32,15.85,0.0,AM8 AN8 AO8 EA8 EB8 EC8 ED8 EE8 EF8 EG8 EH8;
VK,Australia,150,OC,30,59,-23.70,-132.40,-10.0,AX VH VI VJ VK VL VM VN VZ;
ZL,New Zealand,170,OC,32,60,-41.28,-174.78,-12.0,ZK ZL ZM;
F,France,227,EU,14,27,46.00,-2.00,-1.0,F HW HX HY TH TM TP TQ TV TW;
I,Italy,248,EU,15,28,42.83,-12.57,-1.0,I IK IZ;
OH,Finland,224,EU,15,18,61.38,-24.80,-2.0,OF OG OH OI OJ;
PY,Brazil,108,SA,11,15,-10.00,53.00,3.0,PP PQ PR PS PT PU PV PW PX PY ZV ZW ZX ZY ZZ;
LU,Argentina,100,SA,13,14,-34.60,64.00,3.0,AY AZ L2 L3 L4 L5 L6 L7 L8 L9 LO LP LQ LR LS LT LU LV LW;
ZS,Republic of South Africa,462,AF,38,57,-27.00,-25.00,-2.0,H5 S4 S8 ZR ZS ZT ZU;
UA,European Russia,54,EU,16,29,55.00,-37.00,-3.0,R U RA RB RC RD RE RF RG RH RI RJ RK RL RM RN RO RP RQ RR RS RT RU RV RW RX RY RZ UA UB UC UD UE UF UG UH UI;
HB,Switzerland,287,EU,14,28,46.90,-7.43,-1.0,HB HE;
SP,Poland,269,EU,15,28,52.00,-20.00,-1.0,3Z HF SN SO SP SQ SR;
OK,Czech Republic,503,EU,15,28,49.80,-15.50,-1.0,OK OL;
9A,Croatia,497,EU,15,28,45.80,-16.00,-1.0,9A;
YO,Romania,275,EU,20,28,45.90,-25.00,-2.0,YO YP YQ YR;
TF,Iceland,242,EU,40,17,64.80,18.00,0.0,TF;
A6,United Arab Emirates,391,AS,21,39,24.00,-54.00,-4.0,A6;
`;

// ---- the demo log -----------------------------------------------------------------------------------------
const DEMO = [
  ["K1ABC", "20m", "CW", "Bob"], ["W1AW", "20m", "CW", "Hiram"], ["DL1ABC", "20m", "FT8", "Klaus"], ["JA1XYZ", "20m", "CW", "Taro"],
  ["EA8AB", "17m", "FT8", "Pedro"], ["VK3ABC", "15m", "CW", "Tom"], ["ZL2XYZ", "15m", "SSB", "Ian"], ["G3XYZ", "40m", "CW", "Dave"],
  ["F5ABC", "40m", "CW", "Luc"], ["I2ABC", "30m", "CW", "Marco"], ["OH2ABC", "20m", "SSB", "Matti"], ["PY2ABC", "20m", "FT8", "Carlos"],
  ["LU1ABC", "15m", "CW", "Diego"], ["ZS6ABC", "20m", "CW", "Johan"], ["UA3ABC", "20m", "CW", "Ivan"], ["HB9ABC", "40m", "SSB", "Hans"],
  ["SP5ABC", "20m", "FT8", "Piotr"], ["OK1ABC", "30m", "CW", "Jan"], ["9A2AB", "20m", "CW", "Ivo"], ["YO3ABC", "40m", "CW", "Dan"],
];
function demoAdif() {
  const now = Date.now();
  return DEMO.map(([call, band, mode, name], i) => {
    const t = new Date(now - (i + 1) * 3_600_000 * 7);
    const d = t.toISOString().slice(0, 10).replace(/-/g, "");
    const hm = t.toISOString().slice(11, 16).replace(":", "");
    const f = (k, v) => `<${k}:${v.length}>${v}`;
    return f("CALL", call) + f("QSO_DATE", d) + f("TIME_ON", hm) + f("BAND", band) + f("MODE", mode) + f("NAME", name) + f("RST_SENT", mode === "CW" ? "599" : "59") + f("RST_RCVD", mode === "CW" ? "599" : "59") + "<EOR>\n";
  }).join("");
}

// ---- a fake DX cluster (telnet) ---------------------------------------------------------------------------
const clusterSockets = new Set();
const cluster = net.createServer((s) => {
  clusterSockets.add(s);
  s.on("close", () => clusterSockets.delete(s));
  s.on("error", () => {});
  s.write("Welcome to the demo cluster\r\nlogin: ");
  let buf = "";
  s.on("data", (d) => {
    buf += d;
    if (buf.includes("\n") && !s.greeted) {
      s.greeted = true;
      s.write("N0CALL de DEMO-1 >\r\n");
    }
  });
});
const spotLine = (spotter, khz, call, comment) => {
  const t = new Date().toISOString().slice(11, 16).replace(":", "") + "Z";
  return `DX de ${spotter}:`.padEnd(15) + khz.toFixed(1).padStart(9) + "  " + call.padEnd(12) + comment.padEnd(30) + t + "\r\n";
};
const spot = (spotter, khz, call, comment) => { for (const s of clusterSockets) s.write(spotLine(spotter, khz, call, comment)); };

// ---- WSJT-X datagrams (as in ui/e2e/smoke.spec.ts) --------------------------------------------------------
const qstr = (s) => { const b = Buffer.from(s, "utf8"); const l = Buffer.alloc(4); l.writeUInt32BE(b.length); return Buffer.concat([l, b]); };
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32BE(n); return b; };
const i32 = (n) => { const b = Buffer.alloc(4); b.writeInt32BE(n); return b; };
const bools = (...v) => Buffer.from(v.map(Number));
function wsjtx(kind, fields) {
  const head = Buffer.alloc(12);
  head.writeUInt32BE(0xadbccbda, 0); head.writeUInt32BE(2, 4); head.writeUInt32BE(kind, 8);
  return Buffer.concat([head, qstr("WSJT-X"), ...fields]);
}
function status() {
  const freq = Buffer.alloc(8); freq.writeBigUInt64BE(14_074_000n);
  return wsjtx(1, [freq, qstr("FT8"), qstr(""), qstr(""), qstr("FT8"), bools(false, false, true), u32(1500), u32(1500), qstr("N0CALL"), qstr("EN34"), qstr("")]);
}
function decode(timeMs, snr, message, hz = 1000) {
  const dt = Buffer.alloc(8); dt.writeDoubleBE(0.1);
  return wsjtx(2, [bools(true), u32(timeMs), i32(snr), dt, u32(hz), qstr("~"), qstr(message), bools(false, false)]);
}
const sock = dgram.createSocket("udp4");
const send = (p) => new Promise((ok, err) => sock.send(p, 2237, "127.0.0.1", (e) => (e ? err(e) : ok())));
const msOfDay = () => { const d = new Date(); return ((d.getUTCHours() * 60 + d.getUTCMinutes()) * 60 + d.getUTCSeconds()) * 1000; };

// ---- server and browser -----------------------------------------------------------------------------------
const server = spawn(join(root, "target", "debug", "qrzero-server"), ["--data-dir", join(work, "data"), "--bind", `127.0.0.1:${PORT}`, "--token", "demo"], { stdio: "ignore" });
const api = async (method, path, body) => {
  const r = await fetch(`${base}/api${path}`, { method, headers: { "x-qrzero-token": "demo", ...(body !== undefined ? { "content-type": "application/json" } : {}) }, body: body !== undefined ? JSON.stringify(body) : undefined });
  if (!r.ok) throw new Error(`${method} ${path}: ${r.status} ${await r.text()}`);
  return r.headers.get("content-type")?.includes("json") ? r.json() : null;
};
async function waitUp() {
  for (let i = 0; i < 100; i++) { try { if ((await fetch(`${base}/`)).ok) return; } catch { /* not yet */ } await sleep(200); }
  throw new Error("server did not start");
}

const dark = { theme: "dark", fontSize: 13, dateFormat: "iso", localTime: false, freqUnit: "mhz", confirmDelete: true, soundOnLog: false, panelButtons: {}, rst: {} };
const tabs = (...panes) => ({ kind: "tabs", panes, active: panes[0] });
const split = (dir, sizes, ...children) => ({ kind: "split", dir, sizes, children });

/** A browser context in dark mode with the given pane layout. */
async function context(browser, layout) {
  const ctx = await browser.newContext({ viewport: { width: W, height: H } });
  await ctx.addInitScript(([d, ws]) => {
    try {
      localStorage.setItem("qrzero.display", JSON.stringify(d));
      localStorage.setItem("qrzero.workspace", JSON.stringify({ ws }));
    } catch { /* storage unavailable */ }
  }, [dark, layout]);
  return ctx;
}

/** Records the page as frames from the browser's screencast (Playwright's own video needs a bundled ffmpeg). */
async function record(ctx, page) {
  const cdp = await ctx.newCDPSession(page);
  const frames = [];
  cdp.on("Page.screencastFrame", (f) => {
    frames.push({ t: f.metadata.timestamp, data: Buffer.from(f.data, "base64") });
    cdp.send("Page.screencastFrameAck", { sessionId: f.sessionId }).catch(() => {});
  });
  await cdp.send("Page.startScreencast", { format: "jpeg", quality: 90, everyNthFrame: 1 });
  return { frames, stop: () => cdp.send("Page.stopScreencast").catch(() => {}) };
}

async function setup(browser) {
  const ctx = await browser.newContext({ viewport: { width: W, height: H } });
  const page = await ctx.newPage();
  await page.goto(`${base}/?token=demo`);
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
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
  await page.getByText("N0CALL · Home").waitFor();
  await ctx.close();
  const logs = await api("GET", "/logs");
  await fetch(`${base}/api/cty`, { method: "POST", headers: { "x-qrzero-token": "demo" }, body: CTY });
  const r = await fetch(`${base}/api/logs/${logs[0].id}/import`, { method: "POST", headers: { "x-qrzero-token": "demo" }, body: new Blob([demoAdif()]) });
  if (!r.ok) throw new Error("import failed: " + (await r.text()));
  return logs[0].id;
}

/** Turns the recorded frames into a small silent webm and mp4, plus a still from `still` seconds in. */
async function finish(rec, name, still, tail = 1.2) {
  await rec.stop();
  const dir = join(work, name);
  mkdirSync(dir, { recursive: true });
  const list = [];
  rec.frames.forEach((f, i) => {
    const file = join(dir, `f${String(i).padStart(5, "0")}.jpg`);
    writeFileSync(file, f.data);
    const next = rec.frames[i + 1]?.t ?? f.t + tail;
    list.push(`file '${file}'`, `duration ${(next - f.t).toFixed(4)}`);
  });
  list.push(`file '${join(dir, `f${String(rec.frames.length - 1).padStart(5, "0")}.jpg`)}'`);
  const txt = join(dir, "frames.txt");
  writeFileSync(txt, list.join("\n"));
  const vf = "fps=24,scale=960:540:flags=lanczos";
  const inp = ["-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", txt, "-an"];
  execFileSync("ffmpeg", [...inp, "-vf", vf, "-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "38", "-row-mt", "1", join(out, `${name}.webm`)]);
  execFileSync("ffmpeg", [...inp, "-vf", vf + ",format=yuv420p", "-c:v", "libx264", "-preset", "slow", "-crf", "30", "-movflags", "+faststart", join(out, `${name}.mp4`)]);
  execFileSync("ffmpeg", [...inp, "-ss", String(still), "-vf", vf, "-frames:v", "1", "-c:v", "libwebp", "-quality", "82", join(out, `${name}.webp`)]);
}

const clips = { lookup, ftx, bandmap };

try {
  cluster.listen(CLUSTER_PORT, "127.0.0.1");
  await waitUp();
  const browser = await chromium.launch({ executablePath: process.env.PW_CHROMIUM });
  const logId = await setup(browser);
  await api("PUT", "/cluster", { nodes: [{ name: "Demo", host: "127.0.0.1", port: CLUSTER_PORT, login: "N0CALL", password: "", commands: [] }], auto_connect: false, spot_max_minutes: 30, spot_comment: "" });
  await api("POST", "/cluster/connect", { connect: true });
  const only = process.argv[3];
  for (const [name, run] of Object.entries(clips)) if (!only || only === name) await run(browser, logId);
  await browser.close();
} finally {
  server.kill();
  cluster.close();
  sock.close();
  rmSync(work, { recursive: true, force: true });
}
console.log("clips written to", out);

// ---- the clips --------------------------------------------------------------------------------------------

async function ftx(browser) {
  const layout = { root: split("col", [0.66, 0.34], tabs("ftx"), split("row", [0.4, 0.6], tabs("entry"), tabs("log"))), popped: [], locked: false };
  const ctx = await context(browser, layout);
  const page = await ctx.newPage();
  await page.goto(`${base}/?token=demo`);
  await page.getByText("N0CALL · Home").waitFor();
  await send(status());
  const period = Math.floor(msOfDay() / 15000) * 15000;
  // The period before this one is already on screen.
  const prev = [["-8", "CQ DL1ABC JO62", 1634], ["-12", "CQ JA1XYZ PM95", 1012], ["-15", "CQ PY2ABC GG66", 1751], ["-4", "K1ABC W1AW RR73", 1882], ["-19", "CQ DX VK3ABC QF22", 2591]];
  const rows = page.locator(".ftx-row:not(.head)");
  // The first packets can come before the page is listening, so send them again until all of them are listed.
  for (let i = 0; i < 6 && (await rows.count()) < prev.length; i++) {
    for (const [snr, m, hz] of prev) await send(decode(period - 15000, +snr, m, hz));
    await sleep(700);
  }
  if ((await rows.count()) < prev.length) throw new Error("the FTx monitor did not list the earlier period");
  await sleep(1500);
  const rec = await record(ctx, page);
  await sleep(800);
  const now = [["-3", "CQ A61AB LL75", 1480], ["-9", "CQ EA8AB IL18", 880], ["-14", "N0CALL ZS6ABC R-10", 1210], ["-6", "CQ SP5ABC KO02", 2210], ["-17", "CQ TF3ABC HP94", 1650], ["-11", "CQ 9A2AB JN85", 1030]];
  for (const [snr, m, hz] of now) { await send(decode(period, +snr, m, hz)); await sleep(1100); }
  await sleep(2500);
  await finish(rec, "ftx-monitor", 7);
  await ctx.close();
}
async function bandmap(browser) {
  const layout = { root: split("row", [0.42, 0.58], split("col", [0.55, 0.45], tabs("entry"), tabs("log")), tabs("bandmap")), popped: [], locked: false };
  const ctx = await context(browser, layout);
  const page = await ctx.newPage();
  await page.goto(`${base}/?token=demo`);
  await page.getByText("N0CALL · Home").waitFor();
  const early = [["DK3WW", 14008.3, "UA3ABC", "CW 24 dB 22 WPM CQ"], ["EA5WU", 14018.0, "ZS6ABC", "CW 18 dB 25 WPM CQ"], ["OH6BG", 14025.2, "A61AB", "CW UP 1"], ["JA1ABC", 14036.8, "JA1XYZ", "CW"],
    ["OZ1AA", 14044.0, "LU1ABC", "CW 9 dB 20 WPM"], ["SM6V", 14052.6, "SP5ABC", "CW 15 dB 24 WPM CQ"], ["G4ABC", 14063.1, "9A2AB", "CW 20 dB 23 WPM CQ"]];
  for (const sp of early) { spot(...sp); await sleep(120); }
  await page.locator(".bm-label", { hasText: "A61AB" }).waitFor();
  // Let the toasts for the first spots go away before recording.
  await page.getByRole("button", { name: "×" }).first().click().catch(() => {});
  await sleep(800);
  const rec = await record(ctx, page);
  await sleep(1200);
  spot("SM6V", 14031.5, "TF3ABC", "CW 12 dB 21 WPM CQ");
  await sleep(2200);
  await page.locator(".bm-label", { hasText: "A61AB" }).click();
  await sleep(1800);
  await page.getByTestId("call").press("Enter");
  await page.locator(".bm-label.worked", { hasText: "A61AB" }).waitFor();
  await sleep(3000);
  await finish(rec, "band-map", 5.8);
  await ctx.close();
}
async function lookup(browser) {
  const layout = { root: split("row", [0.5, 0.5], split("col", [0.62, 0.38], tabs("entry"), tabs("log")), split("col", [0.55, 0.45], tabs("lookup"), tabs("worked"))), popped: [], locked: false };
  const ctx = await context(browser, layout);
  // No lookup account in the demo: answer with what a QRZ.com lookup would return, keeping the server's own country and worked-before parts.
  await ctx.route("**/api/logs/*/lookup/**", async (route) => {
    const res = await route.fetch();
    const body = await res.json();
    body.station = { CALL: body.call, NAME: "Ross", QTH: "Newcastle", STATE: "NSW", GRIDSQUARE: "QF57", CQZ: "30", ITUZ: "59", COUNTRY: "Australia", DXCC: "150", EMAIL: "" };
    body.source = "QRZ.com";
    await route.fulfill({ response: res, json: body });
  });
  const page = await ctx.newPage();
  await page.goto(`${base}/?token=demo`);
  await page.getByText("N0CALL · Home").waitFor();
  const call = page.getByTestId("call");
  await call.focus();
  const rec = await record(ctx, page);
  await sleep(1000);
  await call.pressSequentially("VK2DEF", { delay: 230 });
  await sleep(900);
  await call.press("Tab");
  await sleep(4500);
  await finish(rec, "lookup", 4.2);
  await ctx.close();
}
