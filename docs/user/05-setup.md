# Callsigns, locations and logs

Open **Settings…** from the **☰** menu at the right of the top bar. Its sections are listed down the left, in four groups:

- **Program**: General (colours, text size, units, formats, reports, setup wizard), Backups, Keyboard
- **Station**: Callsigns, Locations, Equipment, Logs
- **Logging**: Entry fields, Callsign lookup, Awards
- **Connections**: Radios and programs (WSJT-X, JTDX, N1MM, PstRotatorAz, country file), UDP connections, DX cluster, Startup programs

Click a section to show it on the right. Settings opens on **General**.

### Searching settings

Type in the **Search settings** box at the top left to find a setting by its name or a word about it, for example *rotor*, *backup*, *colours* or *QRZ*. Section names match too. The list shows each match with the section it is in. Click one (or press **Enter** for the first) and Settings jumps to that section, scrolls to the setting and outlines it for a moment. **Esc** or the **×** clears the search.

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
- **Fields for** at the top picks which modes the layout applies to. **All modes** is the main layout. For **CW**, **Phone** or **Digital**, click **Set up fields for…** to give that group a layout of its own (it starts as a copy of the main one). Whenever the entry panel's mode belongs to the group, those fields replace the main ones, so CW can show club numbers while SSB shows something else. A group marked * has its own layout. **Use the main layout** takes it back. Values in fields that the new mode doesn't show are dropped, so nothing is logged unseen.

## General

The options people change most. They apply at once and are kept with your other window settings; pop-out windows follow them.

- **Colours**: follow Windows, or always dark or light.
- **Text size**: 11 to 18 px.
- **Distances**: kilometres or miles.
- **Dates in the log**: 2026-10-08, 08/10/2026 or 10/08/2026. Frequencies in the log can read in MHz (14.074) or kHz (14074). Stored QSOs are not changed, and exports stay ADIF.
- **Local time**: show the computer's time next to the UTC clock in the top bar.
- **Reports filled in for each mode**: change the report the QSO panel starts with for CW, SSB, FM, AM, RTTY, PSK31, FT8 or FT4 (for example 59 for CW). An empty box means the usual report.
- **Ask before deleting** QSOs, notes, equipment and layouts. Turn it off if the prompts slow you down; deleting a whole log, restoring a backup and discarding edits always ask.
- **Beep when a QSO is logged**.
- **Tell me when a newer QRZero is released**: once a day (and shortly after start) QRZero asks GitHub for the latest release. When it is newer than yours, a link such as *Version 0.14 is available* appears in the top bar and opens the download page in your web browser. The × hides it until the next release. Nothing is downloaded or installed for you, and the check never delays starting. Turn it off here if you don't want QRZero to contact GitHub.
- **Run the setup wizard** again (it never removes anything), and **Reset these options**.

## Logs

Logs are completely separate, for example one per operator sharing the computer. Create, rename, open or delete logs on the **Logs** tab, and switch between them from the top bar. Deleting a log removes all its QSOs, so QRZero asks you to type the log's name first.

## Callsign lookup

On the **Callsign lookup** tab, turn on QRZ.com lookups and enter your QRZ username and password. Lookups need a QRZ XML subscription. **Save and test login** checks the details straight away.

Settings pages with a **Save** button (and the QSL window's **Online services** page) also have **Save and close**, which saves and then closes the window. The *Saved.* note fades out after a couple of seconds, so if you save again it shows again.

- On Windows the password is stored in Windows Credential Manager, never in the log file. On a Mac it goes to the Keychain. On Linux it goes to the desktop keyring (GNOME Keyring or KWallet); only when no keyring is running, as on a headless server, is it kept in the log file's settings.
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

## Keyboard shortcuts

The **Keyboard** tab lists the keys QRZero responds to and lets you change them.

| Action | Where | Default |
| --- | --- | --- |
| QSL Detail Lookup | Anywhere | <kbd>Alt</kbd>+<kbd>Q</kbd> |
| User guide | Anywhere | <kbd>F1</kbd> |
| Settings | Anywhere | none |
| QSL: LoTW, QRZ, Club Log | Anywhere | none |
| Swap between radio 1 and 2 | QSO entry | <kbd>`</kbd> |
| Save the QSO | Edit QSO window | <kbd>Ctrl</kbd>+<kbd>S</kbd> |
| Previous / next QSO in the log grid | Edit QSO window | <kbd>Alt</kbd>+<kbd>Up</kbd> / <kbd>Down</kbd> |

- Click the key button next to an action and press the new combination. <kbd>Esc</kbd> cancels.
- A key that is already used for another action is refused, and the message says which one. Clear that one first if you want to move the key.
- A letter or digit on its own is refused, because it would trigger while you type a callsign. Use <kbd>Ctrl</kbd> or <kbd>Alt</kbd> with it, or an F key. A few keys belong to Windows or the program (<kbd>Ctrl</kbd>+<kbd>C</kbd>, <kbd>F5</kbd>, <kbd>Alt</kbd>+<kbd>1</kbd> to <kbd>9</kbd> for following a radio, and so on) and can't be used.
- **Clear** leaves an action without a key, **Default** puts back its original key, and **Reset all to defaults** does that for every action.
- **Print the list…** prints every key, yours and the fixed ones (<kbd>Enter</kbd> logs, <kbd>Esc</kbd> clears, and so on), as a one-page sheet to keep by the radio.

Your choices are saved in the log database, so they survive restarts and come back with a restored backup. The ☰ menu shows the current key next to each action.
