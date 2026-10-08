# Callsigns, locations and logs

Open **Settings…** from the **☰** menu at the right of the top bar.

## Station callsigns

Each log can hold several callsigns, for example your current call and the calls you held before. The default callsign is selected when QRZero starts. Every QSO you log records the station callsign in `STATION_CALLSIGN` and `OPERATOR`.

## Locations

A location is somewhere you operate from: home, a holiday QTH, a park. Each has a name and the details that describe it:

| Setting | ADIF field |
| --- | --- |
| Grid | `MY_GRIDSQUARE` |
| City, State, County, Country | `MY_CITY`, `MY_STATE`, `MY_CNTY`, `MY_COUNTRY` |
| DXCC, CQ zone, ITU zone | `MY_DXCC`, `MY_CQ_ZONE`, `MY_ITU_ZONE` |
| IOTA, SOTA, POTA, WWFF references | `MY_IOTA`, `MY_SOTA_REF`, `MY_POTA_REF`, `MY_WWFF_REF` |
| Rig, Antenna | `MY_RIG`, `MY_ANTENNA` (only used when no equipment is picked) |

One location is the **default**: it's selected when QRZero starts and offered first when importing. Pick a different location in the top bar before logging from somewhere else.

Removing a location doesn't change QSOs already logged from it; they keep their details.

## Equipment

The **Equipment** tab lists what you have at each location as a tree: radios, antennas, amplifiers, rotators and other gear. A location can have as many of each as you like.

- **add radio**, **add antenna** and so on add an item under that location.
- **Edit** changes its name and details: model, power in watts, bands, height (antennas) and notes. You can also move it to another location.
- The arrows change the order, which is the order shown in the QSO panel.
- An antenna's **Bands** row has a tick box for each band from 160m to 70cm. Tick the bands it is used on; the tree then shows them after the antenna's name (for example *40 20 15 m*). The QSO panel uses them to pick the antenna for the band you are on, and QSOs logged by WSJT-X, JTDX or N1MM get the first antenna in the list that covers their band.

Equipment is chosen per QSO from the QSO panel (see **Logging QSOs**).

## Entry fields

The **Entry fields** tab chooses what you type for each QSO. Call, reports, frequency, band and mode are always on the first line; up to three more lines are yours.

- **Start from** loads a ready-made layout, then you can adjust it.
- **Add a field** adds any common ADIF field, or **Your own field…** for something QRZero doesn't know (it's saved as `APP_QRZERO_…`).
- Give a field your own **label**, a **width**, and a **default** that fills every new QSO.
- Tick **keep** to hold the value from one QSO to the next (handy for a park reference, or your power).
- The arrows move a field left or right, or up and down a line.

## General

Distances in kilometres or miles, and **Run the setup wizard**.

## Logs

Logs are completely separate, for example one per operator sharing the computer. Create, rename, open or delete logs on the **Logs** tab, and switch between them from the top bar. Deleting a log removes all its QSOs, so QRZero asks you to type the log's name first.

## Callsign lookup

On the **Callsign lookup** tab, turn on QRZ.com lookups and enter your QRZ username and password. Lookups need a QRZ XML subscription. **Save and test login** checks the details straight away.

- On Windows the password is stored in Windows Credential Manager, never in the log file. On Linux it goes to the desktop keyring (GNOME Keyring or KWallet); only when no keyring is running, as on a headless server, is it kept in the log file's settings.
- Results are cached for 30 days, so working the same station again is instant and doesn't use your QRZ allowance.
- Portable calls such as `W1AW/P` or `EA8/G4ABC` fall back to the home call if QRZ doesn't know the full call.

## Backups

The **Backups** tab keeps copies of the log file, `qrzero.db`. A backup holds everything in the log: every log, QSO, location, piece of equipment and setting. Passwords are not in it; they stay in Windows Credential Manager.

- **Back up the log when QRZero starts** makes an automatic backup the first time QRZero starts each day. It runs in the background, so starting isn't held up. It's on unless you turn it off.
- **Keep the newest … automatic backups** (10 to start with) deletes older automatic backups. Manual and *before restore* backups stay until you delete them.
- **Back up now** makes a manual backup straight away. It's safe while you're logging.
- The list shows when each backup was made (your local time), its size and its type. **Download** saves a copy wherever you like, **Delete** removes it.
- **Backups folder** shows where they are kept (`%APPDATA%\QRZero\backups`), with **Copy** to put the path on the clipboard. Copy that folder to another drive now and then.

### Restoring

Restoring replaces the **whole** log file, all logs and QSOs in it, with the backup.

1. Click **Restore** next to a backup, or **Restore from a file…** to pick a `.db` file, for example one you downloaded or copied from another computer.
2. QRZero checks the file first. It refuses anything that isn't a QRZero log, a damaged file, or a log from a newer version of QRZero (update QRZero first).
3. The tab then says **Restart QRZero to finish restoring**. Close QRZero and start it again. Until you do, nothing has changed, and **Cancel restore** drops it.
4. When QRZero starts, it first saves the log as it is then as a *before restore* backup, then puts the backup in its place. Backups from older versions of QRZero are brought up to date as they open.

QSOs logged between choosing **Restore** and restarting are not in the restored log, but they are in the *before restore* backup. If you restored the wrong file, restore that *before restore* backup to undo it.
