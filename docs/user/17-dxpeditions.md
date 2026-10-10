# DXpeditions

The DXpeditions pane lists the DXpeditions that are on the air now or starting soon, and marks the ones that would be new for you. Open it from **Layout**, **DXpeditions**. It sits next to the Watch list to start with; drag the tab anywhere you like, or pop it out to its own window.

## Where the list comes from

QRZero reads NG3K's DXpedition calendar (the ADXO page) when it starts and every few hours after that. The last copy is kept in your log database, so the list is still there offline and after a restart. Press **Refresh** to read the page now (it ignores a second press within a minute). If the page can't be read, a message says so and the last list stays on screen.

The pane shows what is on the air now and what starts in the next two months. DXpeditions that have finished are dropped. The calendar is a volunteer's page, so dates and calls can change; check the operators' own announcements before you plan around one.

## What each row says

| Column | Meaning |
| --- | --- |
| **Call** | The DXpedition's call. |
| **When** | **Now** while it is on the air, **Soon** before it starts, with the dates (UTC). |
| **New for you** | **New DXCC** if you have never worked the entity. If you have, the bands you haven't worked it on (**Bands**) and the modes you haven't (**Modes**: CW, Phone or Digital). **?** when QRZero can't tell, for example before the country file is loaded or before your log is read. |
| **Last spot** | The most recent cluster spot of that call: frequency, mode and time (UTC). |

Ones that would be new are in bold, and ones that are on the air now come first. Tick **Needed only** to hide the rest.

Bands and modes are judged on your active log, per entity, the way DXCC counts them: an entity worked on 20m CW still needs 17m, and still needs Phone and Digital.

## Click to fill in, double-click to tune

Click a row to fill in the call in the QSO panel. Double-click it to also tune: if the call has been spotted, your radio goes to its frequency and mode, just like double-clicking a spot in the Cluster pane.

## Alerts

When the DX cluster spots a listed call that would be new (a new DXCC, or a new band or mode for an entity you have worked), QRZero raises an alert exactly like a Watch list hit: a notice in the top bar, a line in the Watch list's **Recent hits** (marked "DXpedition: New DXCC", "New band 17m" or "New mode CW"), and the beep if you ticked **Sound** there. The same call and band alert once every 10 minutes. Spots of a listed call that wouldn't be new still show in **Last spot**, without an alert.

Alerts work with the pane closed, as long as the cluster is connected. Portable forms of a call count, for example `VP8LP/P`.

## Adding your own

Click **Add call…** for a DXpedition the calendar doesn't have, or one you've heard about elsewhere. Type the call, optionally from and to dates (UTC) and a note, and press **Add**. Without dates the call stays listed and alerting until you **Delete** it. Your own entries are kept in the log database.
