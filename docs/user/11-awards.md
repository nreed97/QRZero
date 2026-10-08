# Awards

The **Awards** pane, a tab next to the log to start with, shows how far you are towards these awards:

- **DXCC**: DXCC entities (countries).
- **WAS**: the 50 US states.
- **WAZ**: the 40 CQ zones.
- **WPX**: callsign prefixes, counted by CQ's WPX rules (K1ABC counts as K1, DL/K1ABC as DL0, K1ABC/4 as K4).
- **WAC**: the six continents.
- **ITU**: the 90 ITU zones.
- **VUCC**: 4-character grid squares (like FN31) worked on 6 m and up. HF contacts don't count. Each band column shows that band's grid count, as the award is issued per band.
- **IOTA**: Islands on the Air references (like EU-005), from the QSO's IOTA field.
- **Counties**: US counties (USA-CA, 3077 of them), from the County field (like `OH,Franklin`). Downloading LoTW confirmations or a QRZ lookup can fill it in.

On the **DXCC** tab a line under the toolbar also shows the **DXCC Challenge** (confirmed and worked entity-band slots on 160 to 6 m, 1000 needed) and **5BDXCC** (entities confirmed on 80, 40, 20, 15 and 10 m, 100 needed).

Each row is an entity, state, zone, prefix and so on. The columns are **Mixed**, then **CW**, **Phone** and **Digital**, then each band from 160 m to 6 m, plus 2 m and 70 cm. A green **C** means confirmed and an amber **W** means worked but not yet confirmed. The second header row shows confirmed and worked totals for each column, and the top right shows the overall total.

## What counts as confirmed

Tick the sources that count under **Confirmed by**:

- **LoTW**: QSOs with **LoTW R** = Y. Download them in QSL, LoTW, **Download confirmations**.
- **Cards**: QSOs with **QSL rcvd** = Y, from paper cards you've marked as received.
- **eQSL**: QSOs with **eQSL R** = Y. ARRL's DXCC and WAS don't accept eQSL, so it's off by default.

## Finding what you need

- **Not yet confirmed** hides rows that are already confirmed.
- Pick one of your callsigns to count only QSOs made with it. Most awards are issued to one callsign.
- Click a row to see its QSOs in the log. Click the button above the log (for example **DXCC 291 ×**) to show everything again.

## While you log

You don't need to open this pane to see what a QSO would be worth. After a lookup, the **Station** pane has a short **Awards on 20m CW** block (with the band and mode from the entry panel). It has one line per award that the QSO would move:

- **new country**, **new state**, **new zone**, **new prefix**, **new continent**, **new grid**, **new island group** or **new county** when you have never worked it. Grid, island and county lines appear once the lookup or entry has a grid square, IOTA reference or county.
- **new band** or **new mode** when you have worked it, but not on this band (160 m to 6 m) or in this mode group (CW, Phone or Digital).
- **not confirmed: …** when you have worked that slot but it isn't confirmed yet, so this QSO is another chance at a confirmation.

When none of that applies, it says *Nothing new for awards, all confirmed*. The block counts confirmations from the sources ticked under **Confirmed by** in this pane, and it updates when you change the band or mode. It always counts all your callsigns.

## Where the data comes from

DXCC uses each QSO's DXCC field, or the country file if it has none. WAS uses the STATE field. QSOs with Alaska or Hawaii count for AK and HI even without a state. WAZ uses the CQZ field. WAC and ITU use the CONT and ITUZ fields, or the country file when they are blank. VUCC uses GRIDSQUARE, IOTA uses IOTA and Counties uses CNTY. Downloading LoTW confirmations fills in a blank state and zones from LoTW's records, which helps WAS and WAZ.

The list of DXCC entities comes from the country file, so deleted entities don't appear.
