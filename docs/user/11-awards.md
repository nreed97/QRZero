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

- **SKCC** and **CWops**: the CW club awards, described under **CW club awards** below. They have their own buttons next to the others.

On the **DXCC** tab a line under the toolbar also shows the **DXCC Challenge** (confirmed and worked entity-band slots on 160 to 6 m, 1000 needed) and **5BDXCC** (entities confirmed on 80, 40, 20, 15 and 10 m, 100 needed).

Each row is an entity, state, zone, prefix and so on. The columns are **Mixed**, then **CW**, **Phone** and **Digital**, then each band from 160 m to 6 m, plus 2 m and 70 cm. A green **C** means confirmed and an amber **W** means worked but not yet confirmed. The second header row shows confirmed and worked totals for each column, and the top right shows the overall total.

## Choosing which awards to show

Open **Settings…**, **Awards** and tick the awards you chase. An award you untick disappears from the Awards pane, from the award lines in the QSO panel, and (for DXCC) from the slot grid there. If you don't operate CW, untick **SKCC**, **CWops**, **NAQCC** and **FISTS**. **Clear all** and **Select all** switch everything off or on at once.

Hiding an award only hides it. Your log is not touched, and ticking the award again brings it back as it was. Every award is on to begin with, and so is any award added in a later version.

## What counts as confirmed

Tick the sources that count under **Confirmed by**:

- **LoTW**: QSOs with **LoTW R** = Y. Download them in QSL, LoTW, **Download confirmations**.
- **Cards**: QSOs with **QSL rcvd** = Y, from paper cards you've marked as received.
- **eQSL**: QSOs with **eQSL R** = Y. ARRL's DXCC and WAS don't accept eQSL, so it's off by default.

## Finding what you need

- **Not yet confirmed** hides rows that are already confirmed.
- Pick one of your callsigns to count only QSOs made with it. Most awards are issued to one callsign.
- Click a row to see its QSOs in the log. Click the button above the log (for example **DXCC 291 ×**) to show everything again.

## CW club awards

The **SKCC**, **CWops**, **NAQCC** and **FISTS** buttons count from the club numbers you log on CW QSOs (the SKCC, CWops, NAQCC and FISTS boxes in the entry panel, see **Logging**). Only CW QSOs count. Members are counted by their number, not their call, because calls change hands. Confirmations don't apply: both clubs run on your log. You can pick one of your callsigns as with the other awards.

QRZero uses the numbers and SKCC suffixes exactly as logged. It has no member list, so it cannot check that a number is real, that a member was active when you worked them, or that both of you used a straight key, bug or sideswiper (SKCC requires that). Check those before you apply; the clubs' own award managers are the authority.

**SKCC**

- **Centurion**: 100 different SKCC members, QSOs from 2002-03-01. Every further 100 is another level (x2, x3, up to x10 shown here). *Since* is the date of the QSO that made 100.
- **Tribune**: 50 different Centurions, Tribunes or Senators, taken from the suffix (C, T or S) in the QSO. Only QSOs from the day you became a Centurion count, and from 2007-03-01. Every further 50 is another level.
- **Senator**: 200 different Tribunes or Senators, from the day you reached Tribune x8 (400 Tribune members) and from 2013-08-01. Members used for Tribune can count again. Every further 200 is another level.

The first table shows each award's members, level, how many more members the next level needs, and the date it was first reached. The second shows the members counted on each band, for the single-band endorsements. Rag Chew, Triple Key, the WAS awards and the other SKCC awards are not tracked yet.

**CWops**

- **ACA** (Annual Competition Award): different CWops members contacted in each calendar year. One contact per member per year counts.
- **ACMA** (Annual Cumulative Membership Award, from 2024): members contacted on each band in the year, added up.
- **CMA** (Cumulative Membership Award): different members contacted on each band since 2010-01-03, with the total of all bands.
- **DXCC** and **US states**: different entities and states where you have worked a member, for all bands and per band (100 entities, or all 50 states, for the awards).
- **CWT medals**: points for the weekly CWops tests, which run for an hour on Wednesday 1300 and 1900 UTC and Thursday 0300 and 0700 UTC. An hour with 10 or more contacts is a point; working the same call again on a different band counts as another contact, but on the same band it doesn't (5 or more contacts if you operate from outside North America and Europe; pick where you operate from). A year's points earn bronze (50, or 24), silver (80, or 40) and gold (120, or 60). If you worked some CWTs that are not in this log (from another logger, or portably), type the year and how many points they earned into **CWTs worked outside this log** and click **Set**. The points are added to that year's total and medal, and shown as *incl. n added*. Set 0 to remove them. They are kept with this log. Under the medal table, one line per year opens a list of every CWT hour you have a QSO in, with the QSOs logged, the contacts that count, and any QSOs logged within 5 minutes outside the hour (those do not count). Use it to see why an hour did not earn a point. Unless you tick **Only QSOs logged as a CWops test**, every CW QSO in those hours counts, including ones you typed in by hand; ticking it counts only QSOs whose contest ID (from N1MM or an import) names a CWT or CWops test.

WAE and the other CWops awards are not tracked yet.

**NAQCC**

- **Friendship Club**: 200 different NAQCC members, one point each, taken by club number from QSOs since 2005-01-01. Every further 200 is another level. *Since* is the date of the QSO that made 200. The same table also shows the members counted on each band.

NAQCC awards are for NAQCC members only, the other station must have been a member when you worked them, and all QSOs must be two-way CW. QRZero has no member list and cannot check that. The club also gives points for sprint QSOs and has awards that depend on your power and antenna (DXCC, WAC, WAS, 2XQRP and so on) and on when members joined (New Member Ambassador). Those are not tracked; use this table for the plain member count and the club's own rules for the rest.

**FISTS**

- **Century, Silver Century, Gold Century and Diamond Century**: 100, 250, 500 and 1000 points. A FISTS member in your own country is worth one point and a member in another DXCC entity is worth two. A member's number counts once, with its best contact. The table shows how many more points each award needs and the date of the QSO that reached it. QRZero takes your country from the MY_DXCC field of the QSO, or from your station callsign when that is blank. Members it cannot compare (no DXCC on the QSO or for you) count one point and are noted under the table.
- **WAS**: different states where you have worked a FISTS member, all bands and per band (50 for the award). It uses the STATE field.

The member must have had a FISTS number when you worked them; QRZero cannot check that. FISTS club stations (worth extra points), the Platinum awards (which count Century holders you work) and the other endorsements are not tracked yet.

## While you log

The **Station** pane also shows a **DXCC slots** grid for the entity of the call you are working: bands across, CW, Phone and Digital down, **W** for worked, **C** for confirmed and blank for slots still needed. It uses the same confirmation sources as this pane. The band-by-mode grid covers 160 m to 6 m.

You don't need to open this pane to see what a QSO would be worth. After a lookup, the **Station** pane has a short **Awards on 20m CW** block (with the band and mode from the entry panel). It has one line per award that the QSO would move:

- **new country**, **new state**, **new zone**, **new prefix**, **new continent**, **new grid**, **new island group** or **new county** when you have never worked it. Grid, island and county lines appear once the lookup or entry has a grid square, IOTA reference or county.
- **new band** or **new mode** when you have worked it, but not on this band (160 m to 6 m) or in this mode group (CW, Phone or Digital).
- **not confirmed: …** when you have worked that slot but it isn't confirmed yet, so this QSO is another chance at a confirmation.

When none of that applies, it says *Nothing new for awards, all confirmed*. The block counts confirmations from the sources ticked under **Confirmed by** in this pane, and it updates when you change the band or mode. It always counts all your callsigns.

## Where the data comes from

DXCC uses each QSO's DXCC field, or the country file if it has none. WAS uses the STATE field. QSOs with Alaska or Hawaii count for AK and HI even without a state. WAZ uses the CQZ field. WAC and ITU use the CONT and ITUZ fields, or the country file when they are blank. VUCC uses GRIDSQUARE, IOTA uses IOTA and Counties uses CNTY. Downloading LoTW confirmations fills in a blank state and zones from LoTW's records, which helps WAS and WAZ.

The list of DXCC entities comes from the country file, so deleted entities don't appear.
