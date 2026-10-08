# FTx monitor and other programs

QRZero listens to WSJT-X, JTDX and N1MM Logger+ over UDP. Set them up in **Settings → Radios and programs**.

## WSJT-X and JTDX

In WSJT-X open **File → Settings → Reporting** and set:

- **UDP Server**: 127.0.0.1, port **2237** (or the address shown in QRZero's settings).

If you run several copies of WSJT-X or JTDX and each sends to its own port, list them all in Settings, **Radios and programs**, **Listen on**, separated by commas: for example `127.0.0.1:2237, 127.0.0.1:2238`. QRZero listens on every one. (Copies that share one port with a multicast group work too: fill in **Multicast group** instead.)
- Tick **Accept UDP requests**, so QRZero can ask WSJT-X to call a station and use the controls in the FTx monitor. Without it WSJT-X ignores every request.

QSOs you log in WSJT-X are added to the open log at the selected location, unless the same QSO is already there. If WSJT-X didn't fill in `MY_ANTENNA`, QRZero adds the antenna whose bands include the QSO's band (see **Equipment**). Turn this off with **Log QSOs that WSJT-X / JTDX log** if another logger already does it.

Only one program can listen on a UDP port. If you also use GridTracker or JTAlert, either add their address under **Pass on to** (or as a relay under **Settings → UDP connections**), or set a **multicast group** (for example 224.0.0.1) in WSJT-X and in every program that listens.

## The FTx monitor

Click the **FTx monitor** tab (next to the Log tab to start with). Drag the tab beside the log to see both, or pop it out to its own window. It collects the decodes from every running WSJT-X and JTDX, so several radios and bands appear in one list, newest at the top.

| Flag | Meaning |
| --- | --- |
| **New DXCC** | You have never worked this entity |
| **New band** / **New mode** | You've worked the entity, but not on this band or mode |
| **New call** | A station you've never worked |
| **New on band** | You've worked the station, but not on this band |
| **Worked** | Already worked on this band |

Rows calling you are highlighted, and needed stations are shaded. Stations on your watch list (see **Watch list**) are marked **Watched**, with their call in colour in the message.

- **Click** a decode to put the call, grid, band and mode into the QSO panel and look it up.
- **Double-click** to have that WSJT-X call the station, as if you'd double-clicked it in WSJT-X.
- **CQ only** and **Needed only** narrow the list; the source box (**All sources**) shows one WSJT-X or JTDX at a time.
- **Period breaks** draws a line between decode periods (every 15 seconds for FT8), with the period's time and how many decodes and calls it had.

### Running WSJT-X from the monitor

Above the decodes there is one line for each WSJT-X or JTDX, in its colour. It shows what that program is doing right now:

- Its source, band, mode and dial frequency.
- **TX** in red while it is on the air (the whole line turns red too), **Tx on** when Enable Tx is ticked and it is waiting for its period, **Tx off** otherwise. **Decoding** shows while it decodes.
- **Tx:** the message it is sending, or will send next.
- Where the QSO has got to (see below).
- **DX** call and grid, **Rpt** the report it will send, **Rx** and **Tx** audio offsets in Hz, and the T/R period.
- **Tx watchdog** in red when WSJT-X's watchdog has stopped transmitting. Tick Enable Tx in WSJT-X, or answer a station, to carry on.

With the source box set to one program, only its line shows.

#### Where the QSO is

QRZero works this out from the message being sent, compared with your call:

| Shown | Message being sent |
| --- | --- |
| **Calling CQ** | CQ N0CALL EN34 |
| **Calling K1ABC** | K1ABC N0CALL EN34 |
| **Sending report** | K1ABC N0CALL -12 |
| **Sending R+report** | K1ABC N0CALL R-08 (**Sending R+grid** for the contest R EN34) |
| **Sending RRR** / **Sending RR73** | K1ABC N0CALL RR73 |
| **Sending 73** | K1ABC N0CALL 73 |
| **Sending free text** | anything else |
| **Idle** | Enable Tx is off and nothing is being sent |

WSJT-X doesn't report its QSO state, only the message, so a free-text QSO or a message from someone else's call shows as free text.

#### Controls

- **Halt TX** stops transmitting at once, as WSJT-X's Halt Tx button does.
- **Stop after this** unticks Enable Tx, so the current transmission finishes and no more follow.
- **Call CQ** puts "CQ *your call* *grid*" (from WSJT-X's own settings) in Tx5 and sends it next.
- **More controls** opens a second line:
  - **Message**, **Send**: puts any text in Tx5 and sends it next; **Set Tx5** only puts it there. Free text is up to 13 characters.
  - **DX call**, **Grid**, **Set DX**: fills in WSJT-X's DX call and grid and generates the standard messages, ready to call that station.
  - **Rx offset**, **Set Rx**: moves WSJT-X's Rx frequency.
  - **Mode**: switches between FT8 and FT4.
  - **Configuration**, **Switch**: switches WSJT-X to another of its configurations, by its exact name.
  - **Replay**: WSJT-X sends everything in its Band Activity window again, for example after QRZero was restarted. Decodes already listed aren't repeated.
  - **Clear windows**: clears WSJT-X's Band Activity and Rx Frequency windows, and that program's decodes here.
- **Double-click** a decode to answer it, as before.

Send and Call CQ only go out while Enable Tx is on in WSJT-X. Answering a decode by double-clicking it ticks Enable Tx too, if WSJT-X's **Double-click on call sets Tx enable** setting is on.

#### What can't be done from here

WSJT-X's UDP interface only accepts a short list of requests, so some things still need WSJT-X itself:

- **Ticking Enable Tx.** There is no request for it. Answering a decode (double-click) can tick it, as above; Halt TX and Stop after this can untick it.
- **Choosing Tx1 to Tx6.** WSJT-X moves through them by itself; QRZero can only set and send Tx5 (free text).
- **The Tx offset.** Only the Rx offset can be set. Set the Tx offset in WSJT-X (Shift+click the waterfall, or the Rx to Tx button).
- **Clearing the DX call.** An empty DX call means "leave it as it is", so it can be changed but not emptied.
- **The dial frequency, band and power**, Hold Tx Freq, auto sequencing and the other settings.
- **JTDX** accepts fewer requests than WSJT-X: it ignores switching configuration, and may ignore other requests depending on its version.

### Your transmissions in the list

Each time a program transmits, the message is added to the list once for that period, marked **TX** in red with the Tx offset in the Freq column and **Sent** in the Flags column. In **Call boxes** it is a red **TX:** line at the top of that period. These lines always show, whatever the filters; click and double-click do nothing on them. They need WSJT-X 2.1 or later, which sends the Tx message.

### Call boxes

Set **View** to **Call boxes** to see the stations heard rather than every line, much like JTAlert's callsign list. Each period gets its own row of boxes, one per station, with its signal report. The newest period is at the top.

- A box is coloured by its most important alert, with a short tag beside the call: **ME** calling you, **WL** on your watch list, **DXCC** new country, **BAND** country new on this band, **MODE** country new in this mode, **NEW** new call, **NB** call new on this band. The colours and which alerts are on are yours to choose (see below).
- A line under the box means the station is calling CQ. A grey call is already worked on this band.
- The boxes in a row are in order of importance, following the alert list, with CQs ahead of the rest and worked stations last.
- Hover over a box to see the full message, country and alerts. Click and double-click work as in the line view, and **CQ only**, **Needed only** and the source box apply here too.

Both choices are remembered.

### Alerts and filters

**Alerts and filters…** in the toolbar chooses which stations stand out and which are hidden. Changes apply straight away, in the main window and any popped-out monitor.

- **Alerts**: tick the ones you want and pick a colour for each. A station gets the colour of the first ticked alert that fits it, from the top of the list down. The same colours shade the rows in the line view and name the alerts in its Flags column. **Needed only** shows just the stations with a ticked alert. New call and New call on band are off to start with.
- **Hide stations already worked on this band.**
- **Weakest signal to show**: hides decodes below that many dB. Leave it empty to show all.
- **Continents to show**: tick the ones you want; with none ticked, all are shown.
- **Calls to ignore**: a list such as `K1ABC, W1AW, VE*`, where a `*` at the end matches every call starting with what comes before it.

Stations calling you always show, whatever the filters. **Reset** puts everything back as it started.

### Where each decode came from

Every decode has a short tag in the **Src** column and a coloured left edge; each WSJT-X or JTDX gets its own colour, the same one as its line in the control strip above the list. Hover over the tag for the full story: the program, its instance name, its configuration, the radio it's on and the dial frequency.

QRZero works the source out from what the program sends, in this order:

1. **A slice or receiver in the instance name or configuration name.** "Slice A", "slice-b", "VFO B" or "RX2" anywhere in either gives the tag **A**, **B**, ... and a label such as *Slice A · WSJT-X*.
2. **A radio QRZero follows on the same frequency.** If one of your rigs (Hamlib, TCI or a serial rig, not WSJT-X or N1MM) is within 3 kHz of the instance's dial frequency, the decode is labelled with that radio, for example *FT-991A · JTDX*. With TCI each Flex slice is its own radio (*FLEX-6600 A*, *FLEX-6600 B*), so the slice letter comes from there. If two radios sit on the same frequency and nothing in the names says which, QRZero doesn't guess.
3. **The configuration name** (other than "Default"), then **the instance name**.

To make this reliable with several copies, name them after the slice:

- **WSJT-X**: start each copy with `--rig-name`, for example a shortcut to `wsjtx.exe --rig-name="Slice A"`. Its instance name becomes *WSJT-X - Slice A*. Without a slice in the name, the tag is the first few characters of the rig name (`--rig-name=IC7300` shows **IC7**).
- **WSJT-X configurations** (**File → Settings**, **Configurations** menu): a configuration called "Flex Slice B" labels that copy *Slice B* too. Recent WSJT-X versions send the configuration name; JTDX doesn't.
- **JTDX**: its instance name starts with JTDX. Most versions accept the same `--rig-name` option; otherwise QRZero relies on the radio's frequency (step 2).

The DXCC flags need the country file (see below) and compare with QSOs in the open log.

## N1MM Logger+

In N1MM open **Config → Configure Ports, Mode Control, Winkey, etc → Broadcast Data**, tick **Contacts** and **Radio**, and set the address to `127.0.0.1:12060`. QSOs logged in N1MM are copied into the open log; when you edit or delete one in N1MM, QRZero follows. N1MM's radios can be followed by the QSO panel.

## Country file

DXCC entities for decodes, spots and the map come from AD1C's country files (country-files.com). QRZero downloads `cty.csv` when it first starts and every two weeks after. If your PC can't reach the site, download the file elsewhere and use **Load cty.csv or cty.dat…**.

When a QSO is logged without a DXCC entity (for example with QRZ lookups off), QRZero fills in `DXCC`, `COUNTRY`, `CONT`, `CQZ` and `ITUZ` from the country file.
