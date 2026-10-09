# Getting started

QRZero is a logbook for amateur radio. It runs as a desktop app on Windows, macOS (Apple Silicon) and Linux; the same screens can also be opened in a web browser.

## Opening QRZero on a Mac the first time

The Mac version is not signed with an Apple Developer account, so macOS blocks it the first time. Open the `.dmg`, drag QRZero into **Applications**, then either:

- Open **Applications**, Control-click (right-click) **QRZero**, choose **Open**, and click **Open** in the warning; or
- Try to open it once, then go to **System Settings → Privacy & Security**, scroll down and click **Open Anyway** next to the QRZero message; or
- In Terminal, run `xattr -dr com.apple.quarantine /Applications/QRZero.app`.

You only need to do this once per downloaded version. It needs a Mac with an Apple chip (M1 or newer) running macOS 11 or later. This version has had less testing than Windows; please report anything odd.

## First run

The first time QRZero starts, the setup wizard walks you through:

1. **Your callsign**, plus any calls you held before (so old QSOs are labelled correctly).
2. **Callsign lookup**: your QRZ.com login, if you have an XML subscription. Skip it if not. If you turn it on, QRZero looks up your own call to fill in your home details.
3. **Home location**: name, grid square, city, state, zones.
4. **Equipment**: the radios, antennas and amplifiers at home. Optional.
5. **Entry fields**: pick a starting layout (general/DX, CW, CW clubs, parks and summits, satellites, casual contesting). You can change every field later.
6. **Import a log**: bring in an ADIF file from your old logger. Optional.

Every step except the callsign can be skipped. You can run the wizard again from **Settings → General**; it never removes anything.

Your log is stored in one file, `qrzero.db`, in `%APPDATA%\QRZero` on Windows, `~/Library/Application Support/QRZero` on a Mac, or `~/.local/share/QRZero` on Linux. QRZero backs it up once a day when it starts, into the `backups` folder next to it; **Settings → Backups** lists those copies, makes one on demand and restores from one (see **Callsigns, locations and logs**). Copy the backups folder to another drive or a USB stick now and then, so a dead disk doesn't take your log with it.

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

## Coming from Log4OM 2

1. In Log4OM, export your whole log as ADIF (**File → Export → ADIF**).
2. In QRZero, choose **Import**, pick the file and the location those QSOs were made from.
3. Leave **Skip duplicates** on, so importing the same file twice is safe.

Fields Log4OM adds for itself (they start with `APP_L4ONG_`) are kept, and come back out if you export with **Full** fields.
