# QSL uploads: LoTW, QRZ and Club Log

Click **QSL** in the top bar.

Each QSO keeps its upload status in the standard ADIF fields (`LOTW_QSL_SENT`, `QRZCOM_QSO_UPLOAD_STATUS`, `CLUBLOG_QSO_UPLOAD_STATUS` and their dates), so exports and other loggers see what's been sent. Add the **LoTW S**, **QRZ** and **Club Log** columns to the log grid to see them.

Every service has a **QSOs from** date. Only QSOs on or after it are uploaded. When you turn a service on it starts from today, so a log you imported (and probably uploaded years ago) isn't sent again. Set an earlier date if you want older QSOs sent.

## LoTW

LoTW uploads use TQSL, already installed and set up with your certificate, and only happen when you click **Sign and upload**.

1. QRZero finds TQSL in its usual place. If it's somewhere else, enter the path to `tqsl.exe`.
2. For each callsign and location, pick the TQSL **station location** that matches it. QSOs from a location with no TQSL station location aren't uploaded.
3. Click **Sign and upload**. QRZero hands the waiting QSOs to TQSL, which signs and uploads them quietly. QSOs TQSL reports as already on LoTW are marked as sent too.

If TQSL asks for your certificate password, it will show its own window.

QSOs imported without a location are not uploaded to LoTW, since QRZero can't tell which station location they belong to. Edit them to give them a location first.

## QRZ Logbook

Paste each callsign's logbook **API key** (on QRZ.com: Logbook, Settings, API); saving a key ticks that callsign for upload. Untick a callsign to pause it. **Test** checks the key. New QSOs go up every few minutes (15 by default; set **Upload every** at the bottom). **Upload now** sends what's waiting straight away.

When you edit a QSO that was already uploaded, QRZero marks it modified and sends the corrected version.

## Club Log

Enter your Club Log email and password, a Club Log **API key** (request one from Club Log's helpdesk), and tick the callsigns to upload. Each callsign goes to its own log on Club Log.

## What gets sent

Uploads send the standard ADIF fields of each QSO. If a service refuses a QSO, the reason is shown under the service and QRZero doesn't keep retrying it until you restart.

Passwords and keys are kept in Windows Credential Manager, not in the log file.
