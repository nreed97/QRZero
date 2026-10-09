# QSL: LoTW, QRZ, Club Log, eQSL and paper cards

Choose **QSL** from the **☰** menu at the right of the top bar. It has four tabs: **Look up** (a call's QSOs and contact details), **Cards to send** (your card queue and label printing), **To reply to**, and **Online services**.

Each QSO keeps its upload status in the standard ADIF fields (`LOTW_QSL_SENT`, `QRZCOM_QSO_UPLOAD_STATUS`, `CLUBLOG_QSO_UPLOAD_STATUS`, `EQSL_QSL_SENT` and their dates), so exports and other loggers see what's been sent. Add the **LoTW S**, **eQSL S**, **QRZ**, **Club Log** and **QSL S** (card) columns to the log grid to see them.

Every service follows the same three steps: **not sent** (N or blank), **sent** (Y, with the date filled in automatically the moment QRZero uploads the QSO, or marks the card sent), then **confirmed** (the matching R field, set when the confirmation arrives).

Every service has a **QSOs from** date. Only QSOs on or after it are uploaded. When you turn a service on it starts from today, so a log you imported (and probably uploaded years ago) isn't sent again. Set an earlier date if you want older QSOs sent.

## Confirmation status and OQRS

The **QSL** section of the QSO editor shows every service in one place: sent and received for LoTW, card and eQSL, upload and received for QRZ, and your **Club Log OQRS** request. A line underneath sums it up, for example "Confirmed by LoTW, Card" or "Not confirmed yet. OQRS requested on Club Log."

QSOs count as confirmed when LoTW R, QSL R (card), eQSL R or QRZ R is Y (or V). QRZ R is the standard ADIF field `QRZCOM_QSO_DOWNLOAD_STATUS`. QRZero doesn't download QRZ confirmations yet, so set it by hand if you want to track one.

Club Log OQRS (online QSL request) has no ADIF field, so QRZero keeps your request in its own fields, `APP_QRZERO_OQRS` and `APP_QRZERO_OQRSDATE`. It doesn't talk to Club Log's OQRS queue; it's a reminder that you've asked. To mark many QSOs, select them in the log and pick **Paper QSL…, Mark OQRS requested**, or right-click.

Log grid columns for all of this: **LoTW R**, **QSL R**, **QRZ R**, **OQRS** and **Confirmed** (letters for each confirming service: L LoTW, C card, E eQSL, Q QRZ).

## Online services

The **Online services** tab has one page per service: **LoTW**, **QRZ Logbook**, **Club Log** and **eQSL**. Pick one across the top. The daily eQSL confirmation download and **Save** stay at the bottom for all of them. QRZ Logbook, Club Log and eQSL each have their own **Upload new QSOs automatically** tick and **Every (min)** interval on their page, so one service can upload every 5 minutes while another is off or runs once an hour. After an upgrade each keeps the interval you had before. LoTW uploads only when you click **Sign and upload**, but it can download confirmations on a timer: tick **Download confirmations automatically** on its page and set **Every (min)** (once a day is 1440; on after an upgrade if you had the daily download on). The bottom tick downloads eQSL confirmations once a day. Rarely used LoTW options (upload by date range, moving from another logger) are under **Upload by date range, moving from another logger**.

## LoTW

LoTW uploads use TQSL, already installed and set up with your certificate, and only happen when you click **Sign and upload**.

1. QRZero finds TQSL in its usual place. If it's somewhere else, enter the path to `tqsl.exe`.
2. For each callsign and location, pick the TQSL **station location** that matches it. QSOs from a location with no TQSL station location aren't uploaded.
3. Click **Sign and upload**. QRZero hands the waiting QSOs to TQSL, which signs and uploads them quietly. QSOs TQSL reports as already on LoTW are marked as sent too.

### Upload needed

To send everything LoTW is missing for a period, use the **Upload needed, from / to** dates under the LoTW button. QRZero counts the QSOs in that range (both days included, UTC) whose LoTW sent status is N or blank, and **Sign and upload** hands them to TQSL in one go, marking each as sent with today's date. The **QSOs from** date doesn't apply to this; the range you pick decides. QSOs from a location with no TQSL station location are skipped, as above.

If TQSL asks for your certificate password, it will show its own window.

QSOs imported without a location are not uploaded to LoTW, since QRZero can't tell which station location they belong to. Edit them to give them a location first.

### Moving from another logger: download first

If your log came from another logger, **download confirmations before your first upload** (LoTW and eQSL have a **Download confirmations** button). Every QSO the service already has is marked sent (with the date filled in if blank) and confirmed, so the upload buttons don't send duplicates. A download never adds QSOs to your log. Records it finds that match nothing are counted, and **Review … not found in the log** under the button lists them, so you can see what the service has that your log doesn't.

### LoTW confirmations

To see which QSOs LoTW has confirmed (for the **Awards** tab), enter your LoTW **website** username and password (the ones you use at lotw.arrl.org, not your certificate password) and click **Download confirmations**. Matching QSOs get **LoTW R** set to Y with the date. LoTW also sends the other station's state, zones, grid and county, and QRZero fills those in where your QSO has them blank. The next download only fetches confirmations newer than the last one.

A confirmation matches a QSO when the call and band are the same, the mode is the same kind (CW, phone or digital) and the times are within 30 minutes. Confirmations that match nothing are counted as **not found in the log**; hover over that text to see them.

## QRZ Logbook

Paste each callsign's logbook **API key** (on QRZ.com: Logbook, Settings, API); saving a key ticks that callsign for upload. Untick a callsign to pause it. **Test** checks the key. With **Upload new QSOs automatically** ticked, new QSOs go up every few minutes (15 by default; change **Every (min)** beside it). **Upload now** sends what's waiting straight away.

When you edit a QSO that was already uploaded, QRZero marks it modified and sends the corrected version.

## Club Log

Enter your Club Log email and password, a Club Log **API key** (request one from Club Log's helpdesk), and tick the callsigns to upload. Each callsign goes to its own log on Club Log.

## eQSL

Enter your eQSL username and password, plus the **QTH nickname** if your eQSL account has more than one, and tick the callsigns to upload. Uploads work like QRZ and Club Log: on the timer, or with **Upload now**. **Download confirmations** fetches eQSLs from your eQSL inbox and sets **eQSL R** on the matching QSOs.

Tick **Download eQSL confirmations once a day** at the bottom to have this done for you.

## Cards to send

Queue a card from the log: select the QSOs and pick **Paper QSL…, Queue a card to send**, or type Q in the **QSL sent** field when you log. QSOs where the other station asked for a card (R) show up too.

The **Cards to send** tab lists everything waiting. Tick the ones to print and pick your label sheet:

- **Avery 5160 / L7160**: 30 small labels, two QSOs per label.
- **Avery 5163 / L7163**: 10 large labels, five QSOs per label.
- **Brother QL-700, DK-22223 50 mm tape, 4 in long** (the default): one label for the fill-in block on the back of a card. It reads "CONFIRMING QSO WITH", the call in large type, then Date, UTC, MHz, Mode and RST for up to four QSOs, newest first; more QSOs with the same station go on another label. **Bottom line** adds an optional line under a rule, such as your power or rig.
- Other **Brother QL-700** rolls: **DK-1201** (29 x 90 mm) and **DK-1209** (29 x 62 mm) with two QSOs per label, or **DK-1202** (62 x 100 mm) with five. Each label is its own page. For any QL-700 size, choose your QL-700 in the print dialog, set the paper to the matching size (for DK-22223, a custom 50 mm x 101.6 mm), and leave scaling at 100%.

**Group by** sorts the queue into piles for a bulk mailing, each with its card and station count: **Bureau (country)** makes one pile per country for your outgoing bureau (cards for a QSL manager get a pile per manager), and **QSL manager** makes one pile per manager, with the cards that have no manager together. Labels print pile by pile in the order shown, and the tick box on a pile's heading picks or unpicks the whole pile. Group by is remembered.

Several QSOs with the same station share a label. **Skip labels** leaves the first few labels empty, for a sheet you've used part of. Click **Print**, and when the labels are on the cards click **Sent via bureau** or **Sent direct** to mark them sent with today's date.

When a card arrives, select its QSOs in the log and pick **Paper QSL…, Card received via bureau** (or direct).

## Cards to reply to

Cards arrive in the mail and don't always get answered right away. The **To reply to** tab is a list of the calls you still owe a reply.

- Right-click QSOs in the log and pick **Add to reply list**, or type a call in **Add a call** and press Enter. A call is listed once, however often you add it.
- Each row shows the call, the date the card arrived (change it if it was earlier), a note you can type in (direct, via bureau, an address), and the newest QSOs you have with that station.
- When you've replied, click **Replied**. Pick **Sent via bureau** or **Sent direct** to also mark those QSOs as card sent with today's date, or **Just remove** to only take the call off the list. The entry is deleted; no history is kept.

## Look up

For answering cards or writing them in bulk: pick **QSL lookup…** from the **☰** menu, or press **Alt+Q**. This opens the QSL window on the **Look up** tab. Type a callsign and press **Enter**.

The top half lists that station's QSOs, newest first: date, UTC, MHz, mode, the RST you sent and received, your own callsign, and the card status (sent and received, with the bureau or direct route and date, and OQRS if you've asked). The bottom half is the contact card: name, address, QTH, country, grid, zones, QSL via (the manager), email, how many cards you've sent and which services have confirmed. Each comes from the newest QSO that has it filled in, and empty ones are left out.

Tick the QSOs for the card (QSOs whose card isn't sent yet start ticked) and use **Mark selected as** to queue a card, or record it as sent or received, by bureau or direct. **Add to reply list** puts the call on the **To reply to** tab. After each lookup the call box is empty and ready for the next call. The call is matched whole, so DL1ABC finds DL1ABC and its portable forms such as DL1ABC/P, but not DL1ABCD.

## What gets sent

Uploads send the standard ADIF fields of each QSO. If a service refuses a QSO, the reason is shown under the service and QRZero doesn't keep retrying it until you restart.

Passwords and keys are kept in Windows Credential Manager (the Keychain on a Mac, the desktop keyring on Linux), not in the log file.
