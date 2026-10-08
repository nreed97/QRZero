# FTx monitor and other programs

QRZero listens to WSJT-X, JTDX and N1MM Logger+ over UDP. Set them up in **Settings → Radios and programs**.

## WSJT-X and JTDX

In WSJT-X open **File → Settings → Reporting** and set:

- **UDP Server**: 127.0.0.1, port **2237** (or the address shown in QRZero's settings).

If you run several copies of WSJT-X or JTDX and each sends to its own port, list them all in Settings, **Radios and programs**, **Listen on**, separated by commas: for example `127.0.0.1:2237, 127.0.0.1:2238`. QRZero listens on every one. (Copies that share one port with a multicast group work too: fill in **Multicast group** instead.)
- Tick **Accept UDP requests**, so QRZero can ask WSJT-X to call a station.

QSOs you log in WSJT-X are added to the open log at the selected location, unless the same QSO is already there. If WSJT-X didn't fill in `MY_ANTENNA`, QRZero adds the antenna whose bands include the QSO's band (see **Equipment**). Turn this off with **Log QSOs that WSJT-X / JTDX log** if another logger already does it.

Only one program can listen on a UDP port. If you also use GridTracker or JTAlert, either add their address under **Pass on to**, or set a **multicast group** (for example 224.0.0.1) in WSJT-X and in every program that listens.

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

### Where each decode came from

Every decode has a short tag in the **Src** column and a coloured left edge; each WSJT-X or JTDX gets its own colour, the same one as its box above the list. Hover over the tag for the full story: the program, its instance name, its configuration, the radio it's on and the dial frequency. The boxes above the list show each instance with its source, band, mode and frequency, and **TX** while it transmits (**TX on** when Enable TX is ticked).

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
