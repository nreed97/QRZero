# Logging QSOs

## Entering a QSO

1. Type the call. The QSO start time is taken from the moment you start typing.
2. Press <kbd>Tab</kbd> or <kbd>Space</kbd> to leave the call field. QRZero looks the call up and fills in name, QTH, state, county, grid, country and DXCC where those boxes are empty.
3. Adjust the reports, add a comment if you like, and press <kbd>Enter</kbd>.

The QSO is saved immediately with the start time as `TIME_ON` and the moment you pressed Enter as `TIME_OFF`. The form clears and the cursor goes back to the call field. Frequency, band, mode and any field marked *keep* stay as they were for the next QSO.

Boxes filled in by the lookup are shown in a different colour, so you can tell them from what you typed. Typing over them is fine; what you type wins.

The fields below the first line can be changed: see **Entry fields**.

| Key | Action |
| --- | --- |
| <kbd>Enter</kbd> | Log the QSO |
| <kbd>Esc</kbd> | Clear the form |
| <kbd>Space</kbd> in the call field | Jump to the sent report |
| <kbd>Tab</kbd> | Next field |
| <kbd>F1</kbd> | This guide (can be changed, see **Keyboard shortcuts** below) |

Two buttons next to **Clear** deal with QRZ.com:

- **Fill from QRZ** looks the call up now and fills in the boxes that are still blank. It does the same as leaving the call field, so use it after you add your QRZ login or when the first lookup failed. It needs your QRZ XML login (Settings, **Callsign lookup**).
- **View on QRZ.com** opens that station's page on the QRZ.com website in your web browser, to read their bio or QSL instructions. It needs no login and doesn't change the form.

If you press <kbd>Enter</kbd> straight from the call field, QRZero waits up to two seconds for the lookup so its details are still saved with the QSO.

## Frequency, band and mode

Typing a frequency in MHz sets the band for you. You can also just pick a band and leave the frequency empty. Picking a mode sets the default reports (599 for CW, 59 for phone, −10 for FT8 and FT4) unless you have already typed your own.

Modes are saved the way the ADIF standard expects, so FT4 is stored as mode `MFSK` with submode `FT4`. The log grid and filters show the familiar name.

## Logging a QSO after the fact

Tick **Enter time** to type the date and time (UTC) yourself, for example when copying a paper log.

## CW club numbers

SKCC, NAQCC, FISTS and CWops numbers can be entered in the entry panel. Choose the **CW clubs** layout under Settings, Entry fields, or add **SKCC**, **NAQCC**, **FISTS** or **CWops** to your own layout. When you type a call, the NAQCC, FISTS and CWops numbers are filled from your last QSO with that call, in grey like other lookup data; type over them if they changed. The **SKCC** box is never filled in for you: type the number the other station sent (with its suffix, which changes as the member advances), and leave it blank if no number was exchanged, because the SKCC awards count only QSOs that have one. The numbers are saved as the ADIF fields SKCC, NAQCC, FISTS and CWOPS, and can be shown as columns in the log. The **SKCC**, **CWops**, **NAQCC** and **FISTS** buttons in the Awards pane count them towards Centurion, Tribune, Senator, the CWops awards, the CWT medals, the NAQCC Friendship Club and the FISTS Century awards.

## The station panel

After a lookup the panel shows:

- **Station details** from QRZ.com (when lookups are on), or from your last QSO with that call. Notes and club numbers (see below) carry over from that QSO; the Comment, serial numbers and reports do not, because they belong to one contact.
- **Note**: your station note for the call, if you have written one (see **Station notes**).
- **Flags**: *First QSO with this call*, or *Already worked on …* when this would be a duplicate on the same band and mode.
- **DXCC slots**: a small grid for the entity of the call, with the bands across and CW, Phone and Digital down. **W** means worked, **C** means confirmed (by the sources set in the Awards pane), and a blank cell is a slot you still need. The band and mode set in the entry panel are marked with a box and bold labels, so you can see at once whether this QSO fills a blank. It follows the DXCC number in the entry form, or the entity from the country file.
- **Awards on (band) (mode)**: one line for each award this QSO would move, for the band and mode set in the entry panel. For example *DXCC* **new country** *Japan*, *WAS OH Ohio:* **new band** *20m*, *WAZ zone 5:* **new mode** *CW*, or *WPX* **new prefix** *K1*. Slots you have worked but not had confirmed are listed after *not confirmed:*. When the QSO adds nothing, the block says so in one grey line. It follows the band and mode as you change them.

The entity and CQ zone come from the QRZ lookup, or from the country file when lookups are off. The state comes from the lookup or the **State** box, so WAS lines only appear when a state is known. What counts as confirmed is set in the Awards pane (see **Awards**). Your full history with the call is in the Worked before pane.

## Worked before

As soon as you type a call, the **Worked before** pane lists every QSO you have had with that station in this log, newest first. The title shows the call and how many QSOs you have with it. QSOs the station made while portable count too: typing DL1ABC also finds DL1ABC/P and DL1ABC/M (marked "as DL1ABC/P" in the Comment column), and typing EA8/DL1ABC finds that and the home call.

Each row shows the date, time (UTC), band, mode, the reports you sent and received, the QSL status for LoTW, paper card and eQSL, the name and your comment. In the QSL columns **C** (in green) means confirmed and **sent** means you sent yours but haven't had one back yet. A red mark at the left of a row means it is on the same band and mode as the entry panel, so logging again would be a duplicate in that slot.

The buttons in the title bar narrow the list. Each one stays on until you click it again, even after a restart:

- **This band**: only QSOs on the band set in the entry panel.
- **This mode**: only QSOs in the mode set in the entry panel.
- **Unconfirmed**: only QSOs not yet confirmed by LoTW, card or eQSL. Handy when deciding whether to send a card.

Working with the rows:

- Click a row to select it. With the list selected, the <kbd>Up</kbd> and <kbd>Down</kbd> arrows move through it.
- Double-click a row, or press <kbd>Enter</kbd>, to open that QSO in the editor.
- Hover over a row for two quick buttons: **Edit** opens the QSO, and **Copy** puts the name, QTH, grid, state, county and QSL manager from that QSO into the entry panel. Useful when the lookup is off or found nothing.
- Right-click a row for more: *Edit QSO*, *Copy name and QTH to entry*, *Show all with CALL in log* (filters the log grid to that call), *Mark card received* (sets the paper card as received today) and *Queue a card* (adds the QSO to the paper QSL queue, see the QSL chapter). Press <kbd>Esc</kbd> or click elsewhere to close the menu.

The line under the list sums up the station: the bands and modes you have worked it on, the date of your first QSO, and which band and mode slots are confirmed.

The pane updates by itself when you log or edit a QSO.

## Station notes

A station note is something you want to remember about a callsign: the operator's name, their dog, the rig they were building, "QSL direct only", or that you promised to meet on 40m. You write it once, and it comes back every time you work that station again.

With a call in the entry panel, the **Notes** pane shows that station's note, ready to edit. Just type. The note saves itself a moment after you stop typing, and again when you click away, and the line under it says when, for example *Saved 14:02*. Line breaks are kept.

When you type the call again later, the **Station** pane shows the note on one line in a highlight colour, right under the station details. Hover over it to read the whole note.

Notes belong to the station, not to a single QSO, and they are kept per log. Portable forms share the home call's note, so DL1ABC/P and EA8/DL1ABC show the note you wrote for DL1ABC. Notes are not written into your QSOs and are not part of an ADIF export.

When the call box is empty, the Notes pane lists all your notes, the most recently changed first, with the call, the first line and the date. Type in the search box to find calls containing what you typed; the arrow keys and <kbd>Enter</kbd> work there too. Click a note to open it. This doesn't touch the entry panel, so it's safe in the middle of a QSO. **All notes** takes you back to the list. To write a note for a station without typing it into the entry panel, search for the call and click **New note for ...**.

To remove a note, click **Delete** and confirm, or simply clear all of its text.

## The map

The map shows your location (from its grid, or latitude/longitude) and the station you are working (from the lookup, or the grid you typed). The solid line is the **short path**, the dashed line the **long path**. Below the map:

- **SP**: the beam heading for the short path, with compass point and distance.
- **LP**: the long-path heading (short path + 180°) and distance.

The shaded area is night, so you can see the grey line; untick **Night** in the pane's title bar to turn the shading off. **Flat** shows the whole world centred on your longitude; **Azimuthal** is a great-circle map centred on you, where straight lines from the centre are true beam headings. When a rotator is connected, a dotted line shows where the beam points: a true great circle leaving your location on the rotator's current heading, so on the Flat map it curves the way the signal really travels, and on the Azimuthal map it is a straight line from the centre. It follows the rotator live.

Choose kilometres or miles in **Settings → General**.

## Rig, antenna and amplifier

If you have set up equipment for the location (see **Equipment**), the **Rig**, **Ant** and **Amp** boxes in the QSO panel's title bar pick what you are using. QRZero remembers your choice for each location. Each QSO records:

| Choice | ADIF field |
| --- | --- |
| Rig | `MY_RIG` |
| Antenna | `MY_ANTENNA` |
| Amplifier | `APP_QRZERO_AMPLIFIER` |

The power box fills from the amplifier's power, or the rig's when no amplifier is picked.

### Antenna by band

Once any antenna at the location has bands ticked (see **Equipment**), the **Ant** box has an **Auto (by band)** choice, and it is the default. In Auto, QRZero picks the antenna whose bands include the band you are on, and changes it whenever the band changes: typed, picked, or followed from the radio. The box shows what it picked, such as *Auto: Hex beam*, and that antenna is saved with the QSO. When no antenna covers the band it shows *Auto: none for 6m* and the QSO is saved without an antenna.

If more than one antenna covers the band, Auto uses the one you last picked by hand on that band, otherwise the first in the equipment list. Picking an antenna by hand uses that antenna on every band until you choose **Auto** again.

## Station callsigns and locations

Pick the callsign you're operating under and where you are from the top bar. The location's details (your grid, POTA or SOTA reference, rig, antenna and so on) are written into each QSO as the ADIF `MY_…` fields. See **Callsigns, locations and logs**.

Under the map, a line shows sunrise and sunset (UTC) at your location and at the other station, and **Grey line both**: the times today when it is within 30 minutes of sunrise or sunset at both ends, which is often when the low bands open between the two. With *Show the computer's local time* on in Settings, your own times also show in local time. It is all worked out on your computer.
