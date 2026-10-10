# Watch list

The watch list keeps an eye out for the stations you're chasing. Tell it what you want, and QRZero lets you know as soon as one turns up on the DX cluster or in your WSJT-X and JTDX decodes. It works in the background, so you get the alert even when the Watch list pane is hidden.

Open it from **Layout**, **Watch list**. It sits next to the Lookup pane to start with; drag the tab anywhere you like, or pop it out to its own window.

## What you can watch

Click **Add…** and choose what to watch for:

| Kind | Matches |
| --- | --- |
| **Call** | One station, in all its portable forms. `DL1ABC` also matches `DL1ABC/P`, `DL1ABC/QRP` and `EA8/DL1ABC`. |
| **Prefix** | Any call starting with it, for example `VP8` or `3Y0`. For portable calls QRZero looks at where the station is: `VP8/G4ABC` and `G4ABC/VP8` both match `VP8`, and a trailing `/P` is never taken as a prefix. |
| **Entity** | A DXCC entity from the country file, such as *Bouvet* or *South Sandwich Islands*, whatever the call looks like. |

For each entry you can also:

- pick **Bands** to only hear about it there (none picked means any band),
- pick **Modes**: **CW**, **Phone**, **Digital**, or just **FT8** or **FT4** (none picked means any mode),
- add a **Note**, for example "need it on 30m" or "pileup splits up 5".

Press **Add** (or Enter) to keep it. **Edit** or a double-click changes an entry, **Delete** removes it, and the tick box in the **On** column turns an entry off for a while without losing it. The list is saved in the log database, so it is still there next time.

A spot with no clear mode, or a decode from a WSJT-X whose band QRZero doesn't know yet, only matches entries that allow any mode or any band.

## Hits

When a watched station shows up, three things happen:

- it appears at the top of **Recent hits** with the time, call, frequency, mode, band, where it came from (**Cluster** or **FTx**), the entry it matched, the country and the spot comment or decoded message,
- the main window shows a short notice in the top bar, even if the Watch list pane is hidden,
- the spot or decode is marked **Watched** in the Cluster and FTx monitor panes.

To keep things calm, the same entry, call and band alert once every 10 minutes, however many times the station is spotted or decoded. The spots and decodes themselves are still marked **Watched** each time.

**Click a hit** to fill in the QSO panel, or **double-click** it to fill in and tune your radio, just like a cluster spot.

New hits are shown in bold and counted next to **Recent hits** (and in the window title when the pane is popped out). Clicking a hit or **Mark seen** clears the count.

Tick **Sound** for a short beep with every new hit. It is off to start with, and only beeps while the Watch list pane is open somewhere.

The hits list lives in memory and starts empty each time QRZero starts.

DXpeditions that would be new for you also show up here as hits; see **DXpeditions**.
