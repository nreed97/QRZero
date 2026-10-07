# Logging QSOs

## Entering a QSO

1. Type the call. The QSO start time is taken from the moment you start typing.
2. Press <kbd>Tab</kbd> or <kbd>Space</kbd> to leave the call field. QRZero looks the call up and fills in name, QTH, state, county, grid, country and DXCC where those boxes are empty.
3. Adjust the reports, add a comment if you like, and press <kbd>Enter</kbd>.

The QSO is saved immediately with the start time as `TIME_ON` and the moment you pressed Enter as `TIME_OFF`. The form clears and the cursor goes back to the call field. Frequency, band, mode and any field marked *keep* stay as they were for the next QSO.

Boxes filled in by the lookup are shown in a different colour, so you can tell them from what you typed. Typing over them is fine; what you type wins.

The fields below the first line can be changed: see **Entry fields**.

| Key | Action |
| --- | --- |
| <kbd>Enter</kbd> | Log the QSO |
| <kbd>Esc</kbd> | Clear the form |
| <kbd>Space</kbd> in the call field | Jump to the sent report |
| <kbd>Tab</kbd> | Next field |
| <kbd>F1</kbd> | This guide |

If you press <kbd>Enter</kbd> straight from the call field, QRZero waits up to two seconds for the lookup so its details are still saved with the QSO.

## Frequency, band and mode

Typing a frequency in MHz sets the band for you. You can also just pick a band and leave the frequency empty. Picking a mode sets the default reports (599 for CW, 59 for phone, −10 for FT8 and FT4) unless you have already typed your own.

Modes are saved the way the ADIF standard expects, so FT4 is stored as mode `MFSK` with submode `FT4`. The log grid and filters show the familiar name.

## Logging a QSO after the fact

Tick **Enter time** to type the date and time (UTC) yourself, for example when copying a paper log.

## The station panel

After a lookup the panel shows:

- **Station details** from QRZ.com (when lookups are on), or from your last QSO with that call.
- **Flags**: *New DXCC* when you have never worked that entity, *New band* or *New mode* when you haven't worked the entity on the band or mode set in the entry panel, and *Already worked on …* when this would be a duplicate on the same band and mode.
- **History**: your most recent QSOs with the call.

DXCC flags need the DXCC entity number, which comes from the QRZ lookup. Without lookups, only the per-call history is shown.

## The map

The map shows your location (from its grid, or latitude/longitude) and the station you are working (from the lookup, or the grid you typed). The solid line is the **short path**, the dashed line the **long path**. Below the map:

- **SP**: the beam heading for the short path, with compass point and distance.
- **LP**: the long-path heading (short path + 180°) and distance.

The shaded area is night, so you can see the grey line. **Flat** shows the whole world centred on your longitude; **Azimuthal** is a great-circle map centred on you, where straight lines from the centre are true beam headings. Choose kilometres or miles in **Settings → General**.

## Rig, antenna and amplifier

If you have set up equipment for the location (see **Equipment**), the **Rig**, **Ant** and **Amp** boxes in the QSO panel's title bar pick what you are using. QRZero remembers your choice for each location. Each QSO records:

| Choice | ADIF field |
| --- | --- |
| Rig | `MY_RIG` |
| Antenna | `MY_ANTENNA` |
| Amplifier | `APP_QRZERO_AMPLIFIER` |

The power box fills from the amplifier's power, or the rig's when no amplifier is picked.

## Station callsigns and locations

Pick the callsign you're operating under and where you are from the top bar. The location's details (your grid, POTA or SOTA reference, rig, antenna and so on) are written into each QSO as the ADIF `MY_…` fields. See **Callsigns, locations and logs**.
