# DX cluster

Click the **Cluster** tab (next to the Log tab to start with) to see spots from a DX cluster. Drag the tab beside the log to keep both in view, or pop it out to its own window.

## Setting up nodes

Open **Settings → DX cluster** and add the nodes you like, or use one of the suggestions (VE7CC, NC7J, the Reverse Beacon Network). For each node:

| Setting | |
| --- | --- |
| Host and port | From the node's listing, e.g. `dxc.ve7cc.net` port 23 |
| Login | Leave empty to log in as the station callsign in the top bar |
| Password | Only for nodes that ask for one |
| Commands after login | Sent once connected, separated by `;`, e.g. `set/skimmer; set/ft8` |

QRZero connects to the first node and moves down the list if a node doesn't answer or drops the connection. Tick **Connect when QRZero starts** to connect automatically.

## Spots

Each spot shows the time, frequency, call, country, a best guess at the mode (from the comment, or the band plan), the spotter and the comment. Spots are flagged against the open log the same way as the FTx monitor: **New DXCC**, **New band**, **New mode**, **New call**, **Worked**. A new spot of a station on the same frequency replaces the older one.

- **Click a spot** to fill in the QSO panel. When the panel follows a radio QRZero controls, the radio tunes to the spot and changes mode.
- The filters narrow the list by band, mode group (CW, phone, digital), age, **Needed only** and **Hide worked**.
- Stations on your watch list (see **Watch list**) are marked **Watched**, with the call in colour.

## Console

**Console** shows what the node sends and lets you type commands, for example `sh/dx 20` for the last 20 spots or `dx 14025 K1ABC tnx qso` to spot a station.
