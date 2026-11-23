# PSK Reporter

The **PSK Reporter** pane shows, live, who is hearing the stations you are looking for: a DXpedition you want to catch, a friend's portable station, or your own call. It reads the reception reports that PSK Reporter receives from thousands of FT8, FT4, CW skimmer and other receiving stations, through PSK Reporter's live feed (mqtt.pskreporter.info). You don't need a login. Turn the pane on from **Layout**.

## Turning it on

The feed is **off** until you turn it on, and QRZero connects to it only then.

1. Type the calls to look for in the box, separated by commas or spaces (for example `VP8PJ, 3Y0J`), and press **Enter** or **Apply**. Up to 20 calls.
2. Tick **Feed on**.

The state next to the toolbar says whether QRZero is connected. Only the reports of your calls are sent to QRZero, so the feed uses very little network. It stays on (and connects again when QRZero starts) until you untick **Feed on**. Changing the calls connects again with the new list. A call with a `/` (such as `VP2M/K1ABC`) is looked for exactly as typed.

**Last 15 min** to **Last 2 hours** chooses how long a report stays in the list.

## What it shows

Each row is one station on one band and mode, made from all its reports: the time it was last heard (UTC) and how long ago (**Ago**), the call, the frequency, band and mode, the best signal report, how many different stations heard it, the last one that did (with their grid square and report), the station's country and flags. A green **New DXCC**, **New band** or **New mode** flag marks a station that would be new for you, from your open log.

- A row heard in the **last 5 minutes** is highlighted and bold: the station is on the air there now.
- A row not heard for **15 minutes** or more is grey.
- The newest is on top. The toolbar counts how many are on now.

For FT8, FT4, JS8 and the other digital modes the frequency shown is the dial frequency of that watering hole (14074.0 for FT8 on 20 m), as the reports carry the dial plus the audio offset. Point at the frequency to see the exact one heard.

## Tuning to a station

**Click** a row to tune the radio to it and fill in the QSO panel with the call, band and mode. Unlike the other spot panes, a single click tunes, as you open this pane to go after a station.

To tune every connected radio at once (for example a second receiver, or an SDR next to your transceiver), tick **Tune all radios**. Without it, the radio selected in the radio bar is tuned. Only radios QRZero controls can be tuned; with none connected, a click only fills in the QSO panel.

## Notes

- Reports reach PSK Reporter a few minutes after they are heard, as receiving programs send them in batches. A station that just started may take a few minutes to show.
- A station that nobody with a reporting program can hear won't show, even when it is on the air.
- The list is kept in memory only and starts empty each time QRZero starts.
