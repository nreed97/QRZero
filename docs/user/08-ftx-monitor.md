# FTx monitor and other programs

QRZero listens to WSJT-X, JTDX and N1MM Logger+ over UDP. Set them up in **Settings → Radios and programs**.

## WSJT-X and JTDX

In WSJT-X open **File → Settings → Reporting** and set:

- **UDP Server**: 127.0.0.1, port **2237** (or the address shown in QRZero's settings).
- Tick **Accept UDP requests**, so QRZero can ask WSJT-X to call a station.

QSOs you log in WSJT-X are added to the open log at the selected location, unless the same QSO is already there. Turn this off with **Log QSOs that WSJT-X / JTDX log** if another logger already does it.

Only one program can listen on a UDP port. If you also use GridTracker or JTAlert, either add their address under **Pass on to**, or set a **multicast group** (for example 224.0.0.1) in WSJT-X and in every program that listens.

## The FTx monitor

Click **FTx monitor** above the log (or **Both** to see it next to the log). It collects the decodes from every running WSJT-X and JTDX, so several radios and bands appear in one list, newest at the top.

| Flag | Meaning |
| --- | --- |
| **New DXCC** | You have never worked this entity |
| **New band** / **New mode** | You've worked the entity, but not on this band or mode |
| **New call** | A station you've never worked |
| **New on band** | You've worked the station, but not on this band |
| **Worked** | Already worked on this band |

Rows calling you are highlighted, and needed stations are shaded.

- **Click** a decode to put the call, grid, band and mode into the QSO panel and look it up.
- **Double-click** to have that WSJT-X call the station, as if you'd double-clicked it in WSJT-X.
- **CQ only** and **Needed only** narrow the list; the instance box shows one WSJT-X at a time.

The DXCC flags need the country file (see below) and compare with QSOs in the open log.

## N1MM Logger+

In N1MM open **Config → Configure Ports, Mode Control, Winkey, etc → Broadcast Data**, tick **Contacts** and **Radio**, and set the address to `127.0.0.1:12060`. QSOs logged in N1MM are copied into the open log; when you edit or delete one in N1MM, QRZero follows. N1MM's radios can be followed by the QSO panel.

## Country file

DXCC entities for decodes, spots and the map come from AD1C's country files (country-files.com). QRZero downloads `cty.csv` when it first starts and every two weeks after. If your PC can't reach the site, download the file elsewhere and use **Load cty.csv or cty.dat…**.

When a QSO is logged without a DXCC entity (for example with QRZ lookups off), QRZero fills in `DXCC`, `COUNTRY`, `CONT`, `CQZ` and `ITUZ` from the country file.
