# Getting started

QRZero is a logbook for amateur radio. It runs as a Windows desktop app; the same screens can also be opened in a web browser.

## First run

The first time QRZero starts it asks for **your callsign** and a **home location** (a name and, optionally, your grid square). That's all it needs to start logging. Everything else is in **Settings**.

Your log is stored in one file, `qrzero.db`, in `%APPDATA%\QRZero`. Back that folder up the way you back up anything else, or export an ADIF file now and then.

## The main window

| Area | What it's for |
| --- | --- |
| Top bar | Pick the log, the station callsign and the location you are logging from. The UTC clock is on the right. |
| Entry panel | Type the QSO and press <kbd>Enter</kbd> to log it. |
| Station panel | Details about the station you are working and your history with it. |
| Log grid | Every QSO in the open log, newest first. |

## Coming from Log4OM 2

1. In Log4OM, export your whole log as ADIF (**File → Export → ADIF**).
2. In QRZero, choose **Import**, pick the file and the location those QSOs were made from.
3. Leave **Skip duplicates** on, so importing the same file twice is safe.

Fields Log4OM adds for itself (they start with `APP_L4ONG_`) are kept, and come back out if you export with **Full** fields.
