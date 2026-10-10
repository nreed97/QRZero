# Getting started

QRZero is a logbook for amateur radio. It runs as a desktop app on Windows, macOS (Apple Silicon) and Linux; the same screens can also be opened in a web browser (see [Using QRZero in a browser](12-browser)).

This page takes you from a fresh install to your first QSO, a radio connected, your logging programs talking to QRZero and your QSLs going out. Each step links to the page with the details. To find anything quickly, use the search box at the top of this window.

## 1. Install

Download the installer for your computer from the QRZero releases page on GitHub.

- **Windows**: run the setup program (or unzip the portable version anywhere and run it).
- **Mac**: open the `.dmg` and drag QRZero into **Applications**. The first start needs one extra step, below.
- **Linux**: install the `.deb` or `.rpm`, or make the AppImage executable and run it.

### Opening QRZero on a Mac the first time

The Mac version is not signed with an Apple Developer account, so macOS blocks it the first time. Open the `.dmg`, drag QRZero into **Applications**, then either:

- Open **Applications**, Control-click (right-click) **QRZero**, choose **Open**, and click **Open** in the warning; or
- Try to open it once, then go to **System Settings → Privacy & Security**, scroll down and click **Open Anyway** next to the QRZero message; or
- In Terminal, run `xattr -dr com.apple.quarantine /Applications/QRZero.app`.

You only need to do this once per downloaded version. It needs a Mac with an Apple chip (M1 or newer) running macOS 11 or later. This version has had less testing than Windows; please report anything odd.

## 2. The setup wizard

The first time QRZero starts, the setup wizard walks you through:

1. **Your callsign**, plus any calls you held before (so old QSOs are labelled correctly).
2. **Callsign lookup**: your QRZ.com login, if you have an XML subscription. Skip it if not. If you turn it on, QRZero looks up your own call to fill in your home details.
3. **Home location**: name, grid square, city, state, zones.
4. **Equipment**: the radios, antennas and amplifiers at home. Optional.
5. **Entry fields**: pick a starting layout (general/DX, CW, CW clubs, parks and summits, satellites, casual contesting). You can change every field later.
6. **Import a log**: bring in an ADIF file from your old logger. Optional.

Every step except the callsign can be skipped. You can run the wizard again from **Settings → General**; it never removes anything. More in [Callsigns, locations and logs](05-setup).

## 3. Bring in your old log

If you used another logger, export your whole log from it as an ADIF file, then in QRZero choose **Import** from the **☰** menu, pick the file and the location those QSOs were made from. Leave **Skip duplicates** on, so importing the same file twice is safe. Fields your old logger added for itself are kept. See [Importing and exporting ADIF](04-import-export).

## 4. Log your first QSO

1. Click the **New QSO** pane and type the callsign. The **Worked before** pane shows your earlier QSOs with that call, and the **Station** and **Map** panes fill in details and the beam heading.
2. Fill in the band, mode, frequency and reports, or tab through them. A QSO is stamped with the current UTC time unless you change it.
3. Press <kbd>Enter</kbd>. The QSO appears at the top of the **Log** pane.

Double-click any QSO in the log to edit it. See [Logging QSOs](02-logging) and [The log grid](03-log-grid).

## 5. Connect a radio

Open **Settings → Equipment**, edit your radio and pick a **Rig control** method: Hamlib `rigctld`, TCI, Kenwood/Elecraft/Flex CAT, Yaesu CAT or Icom CI-V. Pick the radio in the **Radio** box in the New QSO pane's title bar, and the frequency, band and mode follow your tuning. You can also tune the radio from the pane, from FTx decodes and from DX spots. A serial port can only be used by one program at a time, so if WSJT-X also needs the radio, run `rigctld` or the radio's TCI server and point both programs at it. See [Radios and rig control](07-radios), which also covers split, several radios and rotators.

## 6. WSJT-X, JTDX and N1MM Logger+

- **WSJT-X / JTDX**: in WSJT-X open **File → Settings → Reporting**, set the **UDP Server** to 127.0.0.1 port 2237, and tick **Accept UDP requests**. QSOs you log there are added to your log, and decodes show up in the **FTx monitor** tab with flags for new countries, bands and calls.
- **N1MM Logger+**: in N1MM open **Config → Configure Ports, Mode Control, Winkey, etc → Broadcast Data**, tick **Contacts** and **Radio**, and use the address `127.0.0.1:12060`.

See [FTx monitor and other programs](08-ftx-monitor) for the full steps, and [UDP connections and startup programs](16-udp-and-startup) for other programs and for starting your radio software with QRZero.

## 7. QSL services

Choose **QSL** from the **☰** menu, then the **Online services** tab. Enter your login for each service you use (LoTW, QRZ Logbook, Club Log, eQSL), set the **QSOs from** date, and tick the automatic upload if you want new QSOs sent in the background. The **Queue** tab keeps your paper card queue and prints labels. Passwords are kept in the system's password store, not in a settings file. See [QSL: LoTW, QRZ, Club Log, eQSL and paper cards](10-qsl).

## 8. Where to find things

| I want to... | Go to |
| --- | --- |
| Change my callsign, location, radios or antennas | **Settings** (☰ menu) and [Callsigns, locations and logs](05-setup) |
| Choose which fields the QSO panel shows | **Settings → Entry fields** |
| Import or export a log | **Import** and **Export** in the ☰ menu; [Importing and exporting ADIF](04-import-export) |
| Move, hide or pop out panes | The **Layout** menu; [Arranging the window](06-layout) |
| See DX spots | The **Cluster** and **Band map** tabs; [DX cluster](09-cluster), [Band map](13-band-map) |
| Check what I still need for DXCC, WAS and other awards | The **Awards** tab; [Awards](11-awards) |
| Send or answer paper QSL cards | Queue them in the Log tab, then **QSL Queue…** in the ☰ menu, or Alt+Q |
| Send confirmations | **QSL** in the ☰ menu |
| Back up or restore my log | **Settings → Backups** |
| Search this guide | The search box at the top of the Help window |
| See which version I have, or find the project page, releases and issue tracker | **About QRZero** in the ☰ menu |

The log is stored in one file, `qrzero.db`, in `%APPDATA%\QRZero` on Windows, `~/Library/Application Support/QRZero` on a Mac, or `~/.local/share/QRZero` on Linux. QRZero backs it up once a day when it starts, into the `backups` folder next to it; **Settings → Backups** lists those copies, makes one on demand and restores from one. Copy the backups folder to another drive or a USB stick now and then, so a dead disk doesn't take your log with it.

## The main window

The window is made of panes. Each pane has a tab at its top, and panes can share a spot as tabs.

| Pane | What it's for |
| --- | --- |
| Top bar | Pick the log, the station callsign and the location you are logging from. The UTC clock and the **Layout** menu are on the right. |
| New QSO | Type the QSO and press <kbd>Enter</kbd> to log it. |
| Worked before | Every earlier QSO with the call you are typing. |
| Station | Details about the station you are working. |
| Map | Your QTH, the other station, and the path between them with headings and distance. |
| Log | Every QSO in the open log, newest first. |
| FTx monitor, Cluster, Awards | Tabs next to the log to start with. |

You can move, resize, hide and pop out any of them. See **Arranging the window**. Double-click a QSO to edit it in its own window.
