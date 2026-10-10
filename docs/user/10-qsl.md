# QSL: LoTW, QRZ, Club Log, eQSL and paper cards

Choose **QSL** from the **☰** menu at the right of the top bar. It has two tabs: **Queue** (the cards you mean to send, with label printing) and **Online services**.

Each QSO keeps its upload status in the standard ADIF fields (`LOTW_QSL_SENT`, `QRZCOM_QSO_UPLOAD_STATUS`, `CLUBLOG_QSO_UPLOAD_STATUS`, `EQSL_QSL_SENT` and their dates), so exports and other loggers see what's been sent. Add the **LoTW S**, **eQSL S**, **QRZ**, **Club Log** and **QSL S** (card) columns to the log grid to see them.

Every service follows the same three steps: **not sent** (N or blank), **sent** (Y, with the date filled in automatically the moment QRZero uploads the QSO, or marks the card sent), then **confirmed** (the matching R field, set when the confirmation arrives).

Every service has a **QSOs from** date. Only QSOs on or after it are uploaded. When you turn a service on it starts from today, so a log you imported (and probably uploaded years ago) isn't sent again. Set an earlier date if you want older QSOs sent.

## Confirmation status and OQRS

The **QSL** section of the QSO editor shows every service in one place: sent and received for LoTW, card and eQSL, upload and received for QRZ, and your **Club Log OQRS** request. A line underneath sums it up, for example "Confirmed by LoTW, Card" or "Not confirmed yet. OQRS requested on Club Log."

QSOs count as confirmed when LoTW R, QSL R (card), eQSL R, QRZ R or Club Log R is Y (or V). QRZ R is the standard ADIF field `QRZCOM_QSO_DOWNLOAD_STATUS`; **Download confirmations** on the QRZ Logbook page fills it. Club Log R (`APP_QRZERO_CLUBLOG_RCVD`, since ADIF has no field for it) is filled by **Download matches** on the Club Log page. Club Log and QRZ confirmations show in the log and the QSO editor but don't count toward the Awards tab, which uses LoTW, paper cards and eQSL.

Club Log OQRS (online QSL request) has no ADIF field, so QRZero keeps your request in its own fields, `APP_QRZERO_OQRS` and `APP_QRZERO_OQRSDATE`. It doesn't talk to Club Log's OQRS queue; it's a reminder that you've asked. To mark many QSOs, select them in the log and pick **Paper QSL…, Mark OQRS requested**, or right-click.

Log grid columns for all of this: **LoTW R**, **QSL R**, **QRZ R**, **Club Log R**, **OQRS** and **Confirmed** (letters for each confirming service: L LoTW, C card, E eQSL, Q QRZ, G Club Log).

## Online services

The **Online services** tab has one page per service: **LoTW**, **QRZ Logbook**, **Club Log** and **eQSL**. Pick one across the top. **Save** stays at the bottom for all of them. QRZ Logbook, Club Log and eQSL each have one **Upload new QSOs automatically** tick on their page, with a **Wait (min)** box beside it (2 to start with, 1 to 60). A QSO you log or edit goes up that many minutes later; every change restarts the wait, so quick corrections go up in one piece. Every 15 minutes QRZero also sweeps for anything still waiting (an import, or an upload that failed). Each service is switched on or off on its own. After an upgrade, a service that had either the timer or the "shortly after" tick on keeps uploading automatically. To send particular QSOs right away, right-click them in the log and pick **Upload to** a service. LoTW uploads only when you click **Sign and upload**, but it can download confirmations on a timer: tick **Download confirmations automatically** on its page and set **Every (min)** (once a day is 1440; on after an upgrade if you had the daily download on).  Rarely used LoTW options (upload by date range, moving from another logger) are under **Upload by date range, moving from another logger**.

## LoTW

LoTW uploads use TQSL, already installed and set up with your certificate, and only happen when you click **Sign and upload**.

1. QRZero finds TQSL in its usual place. If it's somewhere else, enter the path to `tqsl.exe`.
2. For each callsign and location, pick the TQSL **station location** that matches it. QSOs from a location with no TQSL station location aren't uploaded.
3. Click **Sign and upload**. QRZero hands the waiting QSOs to TQSL, which signs and uploads them quietly. QSOs TQSL reports as already on LoTW are marked as sent too.

### Upload by date range

See **Uploading a date range by hand** below; LoTW has the same panel under **Upload by date range, moving from another logger**. TQSL signs and uploads whatever is listed.

**Portable operation in another state.** Two things help:

- **Use QTH details from the log** (on by default) tells TQSL to take each QSO's own My location details (state, county, grid, zones) instead of the station location's, so QSOs made away from home are signed for where they were made. QRZero only passes the details along; it never changes the My location on your QSOs. This needs a TQSL that supports it (2.5 or newer). For best results the QSO should carry all of those details, which a QRZero location does for the QSOs logged from it.
- **Upload under** picks a different TQSL station location for just this upload, in place of the ones set per callsign and location. Use it for a trip you've set up in TQSL. It applies to the whole upload (the button and the date range) and isn't remembered.

For activations like POTA where you want everything in one place, give your portable location one TQSL station location and leave **Upload under** alone. You don't need a TQSL location per activation.

If TQSL asks for your certificate password, it will show its own window.

QSOs imported without a location are not uploaded to LoTW, since QRZero can't tell which station location they belong to. Edit them to give them a location first.

### Moving from another logger: download first

If your log came from another logger, **download confirmations before your first upload** (LoTW and eQSL have a **Download confirmations** button). Every QSO the service already has is marked sent (with the date filled in if blank) and confirmed, so the upload buttons don't send duplicates. A download never adds QSOs to your log. Records it finds that match nothing are counted, and **Review … not found in the log** under the button lists them, so you can see what the service has that your log doesn't.

### LoTW confirmations

To see which QSOs LoTW has confirmed (for the **Awards** tab), enter your LoTW **website** username and password (the ones you use at lotw.arrl.org, not your certificate password) and click **Download confirmations**. Matching QSOs get **LoTW R** set to Y with the date. LoTW also sends the other station's state, zones, grid and county, and QRZero fills those in where your QSO has them blank. The next download only fetches confirmations newer than the last one.

### New toward your awards

After a LoTW or eQSL download that confirms QSOs, a **New toward your awards** list appears under the button. It shows what those confirmations counted toward for the first time: a new entity, state, zone or prefix, and the new bands and modes (like `20m, CW`) it filled. It follows your settings: awards switched off in **Settings, Awards** are left out, and only the confirmation sources ticked in the Awards pane (LoTW, Paper, eQSL) count, so an eQSL confirmation of an entity LoTW already confirmed isn't new. The list stays until the next download (or you quit QRZero). When a download started by a timer confirms QSOs, a one-line notice also appears at the top of the window.

A confirmation matches a QSO when the call and band are the same, the mode is the same kind (CW, phone or digital) and the times are within 30 minutes. Confirmations that match nothing are counted as **not found in the log**; hover over that text to see them.

## Uploading a date range by hand

Every service page (LoTW, QRZ Logbook, Club Log, eQSL) has an **Upload by date range** panel for sending a batch yourself, for example after a trip or when you first set a service up.

1. Pick **From** and **To** (both days are included, UTC). A blank date means no limit on that side. The service's **QSOs from** date doesn't apply here; the range you pick decides.
2. Click **Show QSOs**. The list is every QSO in that range the service hasn't been sent (its status is N or blank), oldest first, for the callsigns you ticked on that page (for LoTW, the locations you matched to a TQSL station location). **Everything not yet sent** does the same with both dates blank, so it lists every QSO that service is missing.
3. To leave some out, tick them and click **Remove … from queue**. They are marked Ignore for that service (status I), so this upload and the automatic uploads skip them from now on. To send one later, set its status back to N in the QSO editor.
4. Click **Upload N QSOs**. What was listed is what goes, and each QSO sent is marked sent with today's date. The result line says how many went up and how many the service refused.

The list shows up to 2000 QSOs; **Upload** sends all of them, not only the ones shown.

## QRZ Logbook

Paste each callsign's logbook **API key** (on QRZ.com: Logbook, Settings, API); saving a key ticks that callsign for upload. Untick a callsign to pause it. **Test** checks the key. With **Upload new QSOs automatically** ticked, new and edited QSOs go up a couple of minutes after you log them (change **Wait (min)** beside it). **Upload now** sends what's waiting straight away.

When you edit a QSO that was already uploaded, QRZero marks it modified and sends the corrected version.

**Download confirmations** asks QRZ for the QSOs in your logbook it shows as confirmed, and sets **QRZ R** (with the date) on the matching QSOs. QRZ also counts the LoTW confirmations it knows of, so a QSO QRZ confirmed from LoTW is marked as a QRZ confirmation too; it does not set LoTW R. It checks every callsign you ticked and asks QRZ for all its confirmed QSOs each time, a page at a time, so a big logbook takes a moment. A download never adds QSOs to your log.

## Club Log

Enter your Club Log email and password (an application password works if you use two-step login) and tick the callsigns to upload. There is no API key to enter; QRZero carries its own. Each callsign goes to its own log on Club Log.

**Download matches** gets the QSOs Club Log has matched against the other station's log (both of you uploaded the QSO) and sets **Club Log R** on the matching QSOs, as the editor's Club Log line shows. Matches work like LoTW's, within 15 minutes. Club Log asks for an **application password** here (Club Log, Settings, App Passwords), not your login password; if the download says the login was refused, that is the usual reason. The next download only asks for matches made since the last one. Matches with no mode, or for a QSO that isn't in your log, are counted but change nothing.

## eQSL

Enter your eQSL username and password, plus the **QTH nickname** if your eQSL account has more than one, and tick the callsigns to upload. Uploads work like QRZ and Club Log: automatically after the wait, or with **Upload now**. **Download confirmations** fetches eQSLs from your eQSL inbox and sets **eQSL R** on the matching QSOs.

Tick **Download eQSL confirmations once a day** on the eQSL page to have this done for you.

## The card queue

Paper cards go through one **Queue**. A QSO gets into it two ways, both from the **Log** tab (select QSOs, then use **Paper QSL…** above the log, or right-click):

- **Queue a card to send**: you want to send a card to get theirs back. You can also type Q in the **QSL sent** field when you log.
- **Card received, queue a reply**: their card arrived (with a PSE QSL). The QSO is marked received with today's date and goes into the queue so you answer it. If a card has already been sent for that QSO, it only records the receipt. The **Paper QSL…** menu has both a bureau and a direct version.

Use the Log tab's search to find the QSOs. Only QSOs marked Q (queued) are in the queue; nothing else is.

Choose **QSL Queue…** from the **☰** menu, or press **Alt+Q**, to open the **Queue** tab:

1. **Review.** Click a row to see the contact information for that station (name, address, QTH, country, grid, zones, QSL via, email, cards sent so far, confirmations) and the QSO as it goes on the card: Date, UTC, MHz, Mode, RST, and your Rig, Pwr, Ant and Contest ID. Nothing is ticked at first. Tick the cards you want to print or mark; with nothing ticked, the buttons act on the one you're looking at. **Group by, Bureau (country)** sorts the queue into piles for a bulk mailing, one per country for your outgoing bureau (cards for a QSL manager get a pile per manager); the tick box on a pile's heading picks the whole pile.
2. **Print the label.** Pick the labels: **Brother QL-700, straight to the printer** (the default; no print dialog, paper size or scaling to set; see **Label printer** below), or **Avery 5160 / L7160** (30 small labels, two QSOs each) or **Avery 5163 / L7163** (10 large labels, five QSOs each), which print through the normal print dialog. Several QSOs with one station share a label; **Skip labels** (Avery) leaves the first few empty on a part-used sheet.
3. **Mark it sent.** Click **Sent via bureau** or **Sent direct** (today's date is filled in) and the QSOs leave the queue. After a successful QL-700 print a line asks whether to mark those QSOs sent, so it's one click; a failed print marks nothing. **Remove from queue** takes QSOs out without sending a card.

### Label

The label is shown as it will print: "CONFIRMING QSO WITH", the call in large type, then Date, UTC, MHz, Mode and RST for up to four QSOs, newest first, and an optional line at the bottom. **Save label image** saves each label as a PNG file, the way it reads on the card, for printing some other way.

### Label printer

Open **Label printer** under the preview and pick your printer (QRZero picks a Brother QL printer on its own the first time), the **tape** and the label **length**. DK-22223, 50 mm continuous tape cut at 101.6 mm is the fill-in block on the back of a card. Other continuous DK rolls (29, 38, 54 and 62 mm) work too; narrower tape fits fewer QSOs per label. **Bottom line** is printed under a rule at the foot of every label, for example `RIG K3S · ANT hex beam · PWR 100W`. These settings are kept in your log.

The Brother driver must be installed so Windows knows the printer, and its tape must match the **Tape** setting. Printing straight to the printer is Windows only; elsewhere use **Save label image**.

## What gets sent

Uploads send the standard ADIF fields of each QSO. If a service refuses a QSO, the reason is shown under the service and QRZero doesn't keep retrying it until you restart.

Passwords and keys are kept in Windows Credential Manager (the Keychain on a Mac, the desktop keyring on Linux), not in the log file.
