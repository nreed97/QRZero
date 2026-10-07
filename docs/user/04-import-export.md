# Importing and exporting ADIF

QRZero reads and writes ADIF `.adi` files, the format every logger and QSL service understands.

## Importing

Choose **Import** in the top bar.

| Option | What it does |
| --- | --- |
| **Location for these QSOs** | Links the imported QSOs to one of your locations. Your default location is picked to start with. |
| **Location details** | *Fill in* adds the location's `MY_…` fields only where the file has none. *Replace* overwrites the file's `MY_…` fields. *Don't change* links the QSOs without touching their fields. |
| **Skip duplicates** | Skips a record when the log already has the same call on the same band and mode within one minute. Safe to leave on. |
| **Add new station callsigns** | Adds any `STATION_CALLSIGN` the log doesn't know yet to your callsign list, which is handy if you have held more than one call. |

Every field in the file is kept, including fields other programs add for themselves. Records without a call, date or time can't be imported and are listed in the summary.

Files written by older Windows programs in Windows-1252 rather than UTF-8 are read correctly, so accented names come through intact.

## Exporting

Choose **Export** in the top bar, or select rows in the grid and choose **Export selected**.

**What to export**

- **Selected QSOs**: the rows you selected in the grid.
- **What the log grid shows now**: whatever your current search and band/mode filter shows.
- **Choose filters**: any combination of date range (UTC), bands, modes, station callsigns, locations, and one ADIF field value. For example, field `LOTW_QSL_SENT` with an empty value exports QSOs that have no LoTW sent status.

The dialog shows how many QSOs will be exported before you start.

**Fields**

- **Standard** writes the ADIF fields other loggers and QSL services use: call, date and time, band, frequency, mode, reports, station and location details, QSL statuses and so on.
- **Full** writes every field stored with each QSO, including fields from other programs (`APP_…`). Use it for backups or moving to another QRZero install.

The file is saved to your Downloads folder, named after the log and the date.
