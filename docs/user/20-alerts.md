# Push alerts (ntfy)

QRZero can send its alerts to your phone, tablet or another computer through **ntfy**, a free push service. Alerts then reach you when you are away from the radio: a needed station on the cluster, a station on your watch list, or a DXpedition you still need.

## Setting it up

1. Install the ntfy app on your phone (Android or iPhone), or open ntfy.sh in a browser.
2. In QRZero, open **Settings…**, **Alerts** and tick **Send alerts through ntfy**. QRZero makes up a topic name like `qrzero-k3m9xq2tpa7w`.
3. In the ntfy app, subscribe to that topic (on the same server, ntfy.sh unless you changed it).
4. Click **Send test**. A "QRZero test" message should arrive within a few seconds. **Save** keeps the settings.

ntfy.sh needs no account. Anyone who knows a topic there can read it, so keep the made-up name rather than something like your callsign. If you run your own ntfy server, or reserve a topic on ntfy.sh, enter its address under **Server** and an **Access token**; the token is kept in the system's password store, not in a settings file.

**Priority** sets how insistently your phone announces the alerts (ntfy's levels, from Min to Urgent).

## What is sent

- **Needed cluster spots**, chosen the same way as the sound and popup alerts: something new only, also new bands, also new modes, or also unconfirmed slots. What counts as needed follows **Settings…**, **Awards**, **Spot flags** (the awards, bands and modes you chase, and the DX Marathon). The same call, band and mode is sent at most once in the time you pick (30 minutes to start with). Spots the cluster replays when QRZero connects aren't sent.
- **Watch list and DXpedition alerts**: the same ones that show in the Watch list pane.

Each alert has the call and what it is needed for as its title (for example "New zone 25: UA9XX"), and the band, frequency, mode, country, spotter and comment below.

QRZero sends only while it is running and connected to the cluster. The bottom of the page shows when the last alert went and the last error, if any (a wrong server address, say).
