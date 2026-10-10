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

- **Click a spot** to fill in the QSO panel. When the panel follows a radio QRZero controls, the radio tunes to the spot and changes mode. If the comment says where the DX listens (`UP 5`, `QSX 14.205`), the radio is also put in split with that transmit frequency; see the Radios chapter.
- The filters narrow the list by band, mode group (CW, phone, digital), age, **Needed only** and **Hide worked**.
- **Origin and country…** hides spots you could never work. **Spotted from** keeps only spots whose spotter is on your continent, or within a distance of you (distance from the middle of the spotter's country to yours). **Hide same-country spots** drops a station spotted by someone in its own country, such as a Japanese station spotted from Japan. You can also hide your own country, unknown countries, and whole continents. Spots whose spotter's country can't be worked out are kept. The button shows how many of these are on; **Reset** clears them. These need your callsign set so QRZero knows where you are.
- Each spot is coloured by its mode group: a bar down its left edge and the mode in colour, blue for CW, green for digital and orange for phone (the key is in the toolbar). These are the same colours as the band plan strip on the band map. When the mode isn't known from the spot's frequency or comment, the spot is left uncoloured.
- Stations on your watch list (see **Watch list**) are marked **Watched**, with the call in colour.

## Needed now

The **Needed now** pane (Layout menu; it starts as a tab beside Cluster) lists only the spots on the air that would be new for your log, best first: a **new entity**, then a **new band** for an entity you have worked, then a **new mode**. Within each group the newest spot is on top. It uses the same needed flags as the Cluster pane, so a spot shows here exactly when it would be flagged **New DXCC**, **New band** or **New mode** there. Click a spot to fill in the QSO panel and tune, just like clicking it in the Cluster pane.

A call on the same band and mode is listed once, however many people spot it. Spots drop off after the time chosen in the pane (30 minutes to start with). It works for every mode: CW, phone and digital spots are ranked together, with the mode coloured as in the Cluster pane.

### Alerts

When a needed station is spotted, QRZero can play a sound and show a popup in the corner of the main window. Click the popup to tune to the station; it goes away by itself after a short while. A new entity gets a double beep, a new band or mode a single one.

In **Settings**, **DX cluster**, under **Alerts for needed spots** (the **Alerts…** button in the pane takes you there):

- **Play a sound** and **Show a popup** are switched on and off separately.
- **Alert for** chooses how much alerts: new entities only, new entities and bands, or also new modes.
- **Not again for the same call, band and mode within** sets how long before the same station can alert again (30 minutes to start with). A different band or mode is a different slot and alerts on its own. Spots the cluster sends when you first connect aren't alerted.

The popups and the sound come from the main window only, so pop-out windows don't repeat them.

## Spotting a station you worked

To tell the cluster you worked someone, click **Spot…** in the QSO panel, or right-click the QSO in the log and choose **Spot … to the cluster…**. A box shows the frequency and a comment that you can change, and the exact line that will be sent. Nothing goes out until you click **Send spot**.

Spotting is switched off, with the reason shown, when the cluster isn't connected or the QSO is too old. To keep an old contact from being spotted by mistake, only QSOs from the last 10 minutes can be spotted; change the limit in **Settings**, **DX cluster**, **Only spot a QSO made within the last … minutes**. The comment from the right-click menu starts as the one set in **Settings**, **DX cluster**, **Comment for spots sent from the log's right-click menu** (**spotted with QRZero** to start with). Besides plain text it can hold `{call}`, `{mode}`, `{band}`, `{freq}`, `{rst_sent}`, `{rst_rcvd}`, `{name}` and `{my_grid}`, which are filled in from the QSO, so `{mode} {rst_rcvd} via QRZero` becomes `CW 579 via QRZero`. Clusters keep only the first 30 characters, so longer comments are cut. The QSO panel's **Spot…** starts with the mode. For a QSO logged in split, the spot is at the frequency you listened on, where the DX transmits.

## Console

**Console** shows what the node sends and lets you type commands, for example `sh/dx 20` for the last 20 spots or `dx 14025 K1ABC tnx qso` to spot a station.
