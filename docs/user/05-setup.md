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
| Rig, Antenna | `MY_RIG`, `MY_ANTENNA` |

One location is the **default**: it's selected when QRZero starts and offered first when importing. Pick a different location in the top bar before logging from somewhere else.

Removing a location doesn't change QSOs already logged from it; they keep their details.

## Logs

Logs are completely separate, for example one per operator sharing the computer. Create, rename, open or delete logs on the **Logs** tab, and switch between them from the top bar. Deleting a log removes all its QSOs, so QRZero asks you to type the log's name first.

## Callsign lookup

On the **Callsign lookup** tab, turn on QRZ.com lookups and enter your QRZ username and password. Lookups need a QRZ XML subscription. **Save and test login** checks the details straight away.

- On Windows the password is stored in Windows Credential Manager, never in the log file.
- Results are cached for 30 days, so working the same station again is instant and doesn't use your QRZ allowance.
- Portable calls such as `W1AW/P` or `EA8/G4ABC` fall back to the home call if QRZ doesn't know the full call.
