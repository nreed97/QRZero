# QSL: LoTW, QRZ, Club Log, eQSL and paper cards

Click **QSL** in the top bar. The first tab has the online services; **Paper cards** has your card queue and label printing.

Each QSO keeps its upload status in the standard ADIF fields (`LOTW_QSL_SENT`, `QRZCOM_QSO_UPLOAD_STATUS`, `CLUBLOG_QSO_UPLOAD_STATUS`, `EQSL_QSL_SENT` and their dates), so exports and other loggers see what's been sent. Add the **LoTW S**, **QRZ** and **Club Log** columns to the log grid to see them.

Every service has a **QSOs from** date. Only QSOs on or after it are uploaded. When you turn a service on it starts from today, so a log you imported (and probably uploaded years ago) isn't sent again. Set an earlier date if you want older QSOs sent.

## LoTW

LoTW uploads use TQSL, already installed and set up with your certificate, and only happen when you click **Sign and upload**.

1. QRZero finds TQSL in its usual place. If it's somewhere else, enter the path to `tqsl.exe`.
2. For each callsign and location, pick the TQSL **station location** that matches it. QSOs from a location with no TQSL station location aren't uploaded.
3. Click **Sign and upload**. QRZero hands the waiting QSOs to TQSL, which signs and uploads them quietly. QSOs TQSL reports as already on LoTW are marked as sent too.

If TQSL asks for your certificate password, it will show its own window.

QSOs imported without a location are not uploaded to LoTW, since QRZero can't tell which station location they belong to. Edit them to give them a location first.

### LoTW confirmations

To see which QSOs LoTW has confirmed (for the **Awards** tab), enter your LoTW **website** username and password (the ones you use at lotw.arrl.org, not your certificate password) and click **Download confirmations**. Matching QSOs get **LoTW R** set to Y with the date. LoTW also sends the other station's state, zones, grid and county, and QRZero fills those in where your QSO has them blank. The next download only fetches confirmations newer than the last one.

A confirmation matches a QSO when the call and band are the same, the mode is the same kind (CW, phone or digital) and the times are within 30 minutes. Confirmations that match nothing are counted as **not found in the log**; hover over that text to see them.

## QRZ Logbook

Paste each callsign's logbook **API key** (on QRZ.com: Logbook, Settings, API); saving a key ticks that callsign for upload. Untick a callsign to pause it. **Test** checks the key. New QSOs go up every few minutes (15 by default; set **Upload every** at the bottom). **Upload now** sends what's waiting straight away.

When you edit a QSO that was already uploaded, QRZero marks it modified and sends the corrected version.

## Club Log

Enter your Club Log email and password, a Club Log **API key** (request one from Club Log's helpdesk), and tick the callsigns to upload. Each callsign goes to its own log on Club Log.

## eQSL

Enter your eQSL username and password, plus the **QTH nickname** if your eQSL account has more than one, and tick the callsigns to upload. Uploads work like QRZ and Club Log: on the timer, or with **Upload now**. **Download confirmations** fetches eQSLs from your eQSL inbox and sets **eQSL R** on the matching QSOs.

Tick **Download LoTW and eQSL confirmations once a day** at the bottom to have this done for you.

## Paper cards

Queue a card from the log: select the QSOs and pick **Paper QSL…, Queue a card to send**, or type Q in the **QSL sent** field when you log. QSOs where the other station asked for a card (R) show up too.

The **Paper cards** tab lists everything waiting. Tick the ones to print and pick your label sheet:

- **Avery 5160 / L7160**: 30 small labels, two QSOs per label.
- **Avery 5163 / L7163**: 10 large labels, five QSOs per label.

Several QSOs with the same station share a label. **Skip labels** leaves the first few labels empty, for a sheet you've used part of. Click **Print**, and when the labels are on the cards click **Sent via bureau** or **Sent direct** to mark them sent with today's date.

When a card arrives, select its QSOs in the log and pick **Paper QSL…, Card received via bureau** (or direct).

## What gets sent

Uploads send the standard ADIF fields of each QSO. If a service refuses a QSO, the reason is shown under the service and QRZero doesn't keep retrying it until you restart.

Passwords and keys are kept in Windows Credential Manager, not in the log file.
