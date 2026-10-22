# The log grid

The grid shows every QSO in the open log, newest first. It stays fast with hundreds of thousands of QSOs because it only loads the rows on screen.

## Searching

- Type in **Search call** to show calls that start with what you type, for example `W1` or `DL1ABC`.
- Use `*` as a wildcard to search anywhere in the call: `*/P` finds portable calls, `*ABC*` finds any call containing ABC.
- The **band** and **mode** boxes narrow the list further.

## Selecting

- Click a row to select it. <kbd>Ctrl</kbd>+click adds or removes rows, <kbd>Shift</kbd>+click selects a range.
- Or tick the box at the start of each row.

With rows selected, the toolbar shows **Clear selection**, **Paper QSL…**, **Export selected**, **Bulk edit** and **Delete**. Deleting asks first and can't be undone.

**Paper QSL…** sets the card fields on the selected QSOs: queue a card to send, card sent (bureau or direct), card received (bureau or direct; a card received also queues a card back to send, unless one was already sent), or not sending (takes it out of the queue), plus **Mark OQRS requested** and **Clear OQRS request** for Club Log OQRS. Sent and received also record today's date. See [QSL](10-qsl.md) for printing labels.

Right-click a row for a menu. It acts on the selected rows when you right-click one of them, otherwise on that row alone:

- **Edit** opens the QSO editor.
- **Mark OQRS requested**, **Card sent via bureau**, **Card sent direct**, **Queue a card to send** and **Card received, queue a reply** set the QSL fields on the QSOs the menu acts on, the same as **Paper QSL…**.
- **Fill from lookup** looks each call up and fills in name, QTH, state, grid and the rest wherever the QSO has nothing. Nothing you already have is changed. It needs a QRZ or HamQTH login (Settings, **Callsign lookup**), asks them in the order set there, and goes on to the next service for anything the first one left blank.
- **View CALL on QRZ.com** (one row only) opens that station's page on QRZ.com in your web browser. It needs no login.
- **Send through UDP connections** sends the QSOs again to every **QSO logged** connection under Settings, **UDP connections**, just as if they had been logged now. Use it to push older contacts into another program. See [UDP connections](16-udp-and-startup.md).
- **Upload to QRZ Logbook / Club Log / eQSL / LoTW (TQSL)** appears for each service that is set up under QSL. It uploads the chosen QSOs right away, even ones already sent, and updates their sent status. QSOs only go to a service for the station callsigns ticked there (and, for LoTW, the locations with a TQSL station location). See [QSL](10-qsl.md).
- **Export**, **Bulk edit** and **Delete** do the same as the buttons above the grid.

When you click a row in the **Awards** tab, the log shows only those QSOs and a button such as **DXCC 291 ×** appears above the grid. Click it to show all QSOs again.

## Looking at a station from the log

Click a row (or move with the arrow keys) and the **Station**, **Worked before** and **Map** panes switch to that call, so you can see your earlier QSOs with it and its details without opening the editor. The New QSO fields are not touched. The panes go back to following New QSO as soon as you type or pick another call there.

## Editing a QSO

Double-click a row, or select it and press <kbd>Enter</kbd>, to open the QSO editor in its own window. Put the window wherever you like; QRZero remembers where it was. There is only ever one editor window: double-click another QSO, in the log or in Worked before, and the editor switches to it (asking first if you have unsaved changes).

The editor groups the fields the way you think about a contact:

- **Contact**: call, date, on and off time, frequency, band, mode and submode, power, reports, name, QTH, grid, comment and notes. Type the date as `2026-10-08` and times as `1423` or `142305`, all in UTC. Change the frequency to one in another band and the band follows.
- **Their location**: country, DXCC, CQ and ITU zones, continent, state, county and IOTA. POTA and SOTA references appear when the QSO has one; **Add POTA** or **Add SOTA** puts the box there when you need it.
- **My station**: your callsign, operator, location, rig, antenna and grid. Rig and antenna suggest the equipment you set up for the chosen location, but you can type anything.
- **QSL**: a small table with one line each for LoTW, paper card, eQSL, QRZ and Club Log. Set what was sent and received and when. Below it are **Card sent via** (bureau, direct, electronic or manager) and **QSL via**.
- **Other fields**: your own fields from the entry panel and anything else the QSO carries, such as fields from an import.

**All ADIF fields** at the bottom opens the full list of everything stored with the QSO, the way the ADIF file sees it. It edits the same QSO as the boxes above, so a change in one shows in the other. Clear a value to remove that field, or type a name and value at the end and click **Add field**.

Labels of fields you have changed turn amber, and the title bar counts your unsaved changes. A box outlined in red has a value that won't save, for example a date that doesn't exist; fix it and save again.

Keys while the editor is open:

- <kbd>Ctrl</kbd>+<kbd>S</kbd> saves.
- <kbd>Esc</kbd> closes the editor window. If you have unsaved changes it asks before throwing them away.
- <kbd>Alt</kbd>+<kbd>Up</kbd> and <kbd>Alt</kbd>+<kbd>Down</kbd> step to the QSO above or below in the log grid, as do the arrow buttons in the title bar.

**Fill from lookup** in the editor's bottom bar looks the call up and fills in only the fields that are blank, then lists what it filled and which service each came from. Nothing is saved until you press **Save**. **View on QRZ.com** next to it opens the station's page on QRZ.com in your web browser.

**Revert** puts back what was saved, and **Delete QSO** removes the contact after asking. Editing a QSO that was already uploaded to QRZ or Club Log marks it for upload again.

## Choosing columns

Click **Columns** above the grid to choose which columns are shown. Tick the ones you want; new ones are added at the right. **Reset** goes back to the standard set, order and widths.

To move a column, drag its heading left or right; a line shows where it will land. To make a column wider or narrower, drag the right edge of its heading. Double-click that edge to give the column its normal width back. Besides the usual QSO fields you can show your rig, antenna, location, POTA/SOTA references, QSL status and more. Your choice is saved with your settings.
