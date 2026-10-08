# Callsigns, locations and logs

Open **Settings** from the top bar.

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

- On Windows the password is stored in Windows Credential Manager, never in the log file.
- Results are cached for 30 days, so working the same station again is instant and doesn't use your QRZ allowance.
- Portable calls such as `W1AW/P` or `EA8/G4ABC` fall back to the home call if QRZ doesn't know the full call.
