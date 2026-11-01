# Troubleshooting

Short answers to common problems, with a link to the page that explains the details. If yours isn't here, see [Getting help](23-getting-help).

## Radio and programs

- **The radio doesn't follow, or the serial port is busy.** A serial port can only be used by one program at a time. If WSJT-X also needs the radio, run `rigctld` or the radio's TCI server and point both programs at it. See [Radios and rig control](07-radios).
- **WSJT-X or JTDX decodes don't arrive.** Only one program can listen on a UDP port. If you also use GridTracker or JTAlert, add their address under **Pass on to**, or use a multicast group. See [FTx monitor and other programs](08-ftx-monitor).
- **N1MM contacts or radio data don't arrive.** Check that Broadcast Data in N1MM has **Contacts** and **Radio** ticked and points at `127.0.0.1:12060`. See [Getting started](01-getting-started) and [UDP connections and startup programs](16-udp-and-startup).
- **The rotator doesn't turn.** While it moves you will see **Turning to 123°**; if it never gets there the message clears after a minute and a half. See [Radios and rig control](07-radios).

## Lookups and spots

- **A callsign lookup finds nothing, or the entry panel says the login was refused.** Open **Settings → Callsign lookup** and press **Save and test login** on the service. See [Callsigns, locations and logs](05-setup).
- **No spots appear.** Check the node list and **Connect when QRZero starts** under the Cluster tab's settings; QRZero moves down the list if a node doesn't answer. See [DX cluster](09-cluster).
- **No POTA activators on the band map.** Tick **POTA** in the band map toolbar; the list is only read while it is ticked. See [Band map](13-band-map) and [POTA spots](21-pota).

## QSL and uploads

- **Club Log answers "403 Forbidden".** It has blocked uploads for a while because too many files arrived too quickly. Wait an hour or so and press **Upload now** again. See [QSL](10-qsl).
- **A service refuses a QSO.** The reason is shown under the service, and QRZero doesn't retry that QSO until you restart. See [QSL](10-qsl).
- **A LoTW confirmation was not matched.** A confirmation matches when call and band are the same, the mode is the same kind and the times are within 30 minutes. Unmatched ones are counted as not found in the log. See [QSL](10-qsl).
- **Moving from another logger.** Download confirmations before your first upload, so QSOs the service already has are marked sent. See [QSL](10-qsl).

## Awards

- **Award totals look lower than LoTW's.** Check the callsign box in the Awards pane: it may be set to one callsign instead of **All my callsigns**. See [Awards](11-awards).
- **A spot isn't flagged as needed.** What counts as needed follows **Spot flags** in Settings, Awards. See [Awards](11-awards).

## The log and the window

- **Restoring a backup.** Nothing changes until you close QRZero and start it again. See [Callsigns, locations and logs](05-setup).
- **Where is my log?** In `qrzero.db`, in `%APPDATA%\QRZero` on Windows, `~/Library/Application Support/QRZero` on a Mac or `~/.local/share/QRZero` on Linux. See [Getting started](01-getting-started).
- **A pane has gone missing.** Use the **Layout** menu to show it again, or load a saved layout. See [Arranging the window](06-layout).
- **Using a browser instead of the app.** Close the desktop app first, because both would use the same log and UDP ports. See [Using QRZero in a browser](12-browser).
