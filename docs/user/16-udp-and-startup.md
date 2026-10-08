# UDP connections and startup programs

## UDP connections

QRZero talks to WSJT-X, JTDX, N1MM Logger+ and PstRotatorAz by name (see **Settings → Radios and programs**). For everything else, such as an antenna switch, a band decoder, another rotator program or a script of your own, set up a **UDP connection** under **Settings → UDP connections**. It works much like the UDP outbound connections and the UDP relay in Log4OM.

Each connection has:

- **On**: untick it to keep the connection without using it.
- **Name**: anything that reminds you what it's for.
- **Sends when**: the event that makes it send (below).
- **Format**: what the message looks like (below).
- **Host** and **Port**: where it goes. `127.0.0.1` is this computer. A broadcast address such as `192.168.1.255` reaches every computer on your network.

**Send test** sends a made-up example (W1AW on 14.074 MHz, a heading of 45°, and so on) straight away, without saving, and shows you exactly what was sent. The **Status** column shows when each connection last sent, or why it couldn't.

The **or add** buttons start a connection with sensible settings for common jobs: an antenna switch or band decoder, a rotator program, a WSJT-X relay, and logged QSOs as ADIF. Change the port to whatever the other program listens on, then **Save**.

### Events

| Sends when | What happens |
| --- | --- |
| **QSO logged** | A QSO is logged, from the QSO panel or from WSJT-X, JTDX or N1MM. Duplicates that QRZero skips aren't sent. To send QSOs already in the log, select them, right-click and choose **Send through UDP connections**. |
| **Radio frequency / mode change** | A radio QRZero follows (a rig under Equipment, WSJT-X or N1MM) changes frequency, mode or transmit state. It sends only when something changed, and at most four times a second per radio; while you spin the VFO the last frequency always follows. Give a radio name under **Only this radio** (as the QSO panel shows it, for example `FLEX-6600 A`) to send for that radio only. |
| **Rotator turn request** | You ask the rotator to turn, from the Rotator pane (or the map's **Turn SP** / **LP**, which show when PstRotatorAz is on). This works whether or not PstRotatorAz is turned on under Radios and programs, so you can drive another rotator program, or both. |
| **Call entered** | A callsign is entered in the QSO panel and looked up. The message carries the call, the grid from the lookup, and the short-path heading from your location's grid (or the country's centre when there's no grid). |
| **Relay WSJT-X / JTDX packets** | Every packet WSJT-X or JTDX sends to QRZero is passed on unchanged. See *Relay* below. |
| **Relay N1MM packets** | The same for N1MM Logger+ broadcasts. |

### Formats

| Format | What's sent | Good for |
| --- | --- | --- |
| **N1MM RadioInfo XML** | N1MM Logger+'s `<RadioInfo>` message: `RadioNr`, `Freq` and `TXFreq` (in tens of Hz, as N1MM sends them, so 14.074 MHz is `1407400`), `Mode`, `IsTransmitting` and the rest of N1MM's fields. `RadioNr` is 1 for a single radio and counts up for the slices or receivers of one rig. | Antenna switches and band decoders that follow N1MM (many listen on port 12060). |
| **N1MM contactinfo XML** | N1MM's `<contactinfo>` message for a QSO: `call`, `band` (in MHz, as N1MM labels it, e.g. `14`), `rxfreq` / `txfreq` in tens of Hz, `mode`, `timestamp`, `snt` / `rcv`, `gridsquare`, `name`, `comment` and so on. | Programs that read N1MM's contact broadcasts. |
| **ADIF record** | One ADIF record ending in `<EOR>`, with every field of the QSO. For other events, the call, frequency, band, mode, grid and heading as ADIF fields. | Loggers and scripts that take ADIF over UDP. |
| **JSON** | One JSON object: `app`, `event`, then what the event has, such as `call`, `freq_hz`, `band`, `mode`, `radio`, `radio_nr`, `tx`, `azimuth`, `grid`, and for QSOs `fields` with every ADIF field. | Your own scripts, Node-RED and the like. |
| **PstRotatorAz azimuth** | `<PST><AZIMUTH>123</AZIMUTH></PST>`, the heading rounded to whole degrees. Only for **Rotator turn request** and **Call entered**. | PstRotatorAz on another port or computer, and other rotator programs that accept PstRotatorAz's UDP commands. |
| **Custom text** | Your own message, with placeholders filled in. | Anything else. |

The N1MM formats follow N1MM Logger+'s published UDP broadcast messages. Most antenna switches and band decoders only look at the radio number and frequency; if yours wants something more, check its manual, and try **Send test** with the device watching.

### Custom text

Write the message under **Message**. These placeholders are filled in:

| Placeholder | Value |
| --- | --- |
| `{CALL}` | The station's callsign |
| `{FREQ_HZ}` | Frequency in Hz, e.g. `14074000` |
| `{FREQ_KHZ}` | Frequency in kHz, e.g. `14074.000` |
| `{FREQ_MHZ}` | Frequency in MHz, e.g. `14.074000` |
| `{BAND}` | Band, e.g. `20m` |
| `{MODE}` | Mode (the submode such as `FT4` when there is one) |
| `{RIG_MODE}` | The radio's own mode name, e.g. `USB` or `DIGU` |
| `{TX}` | `1` while transmitting, otherwise `0` |
| `{AZ}` | Heading in whole degrees |
| `{GRID}` | The station's grid |
| `{RADIO}`, `{RADIO_NR}` | The radio's name and number |
| `{EVENT}` | `qso_logged`, `radio`, `rotator` or `lookup` |
| `{DATE}`, `{TIME}` | Now, in UTC: `20240315`, `140512` |

For a logged QSO, any ADIF field works too, for example `{RST_SENT}`, `{NAME}` or `{STATE}`. A placeholder with nothing to fill in becomes empty. Write `\r`, `\n` or `\t` for a carriage return, line feed or tab, for programs that want each message to end with one. Braces around anything other than a placeholder name are left as they are, so `{"freq": {FREQ_HZ}}` sends `{"freq": 14074000}`.

### Relay

A UDP port can only be listened to by one program. When WSJT-X sends to QRZero, a **Relay WSJT-X / JTDX packets** connection passes each packet on, untouched, to another program such as GridTracker or JTAlert: point that program at the relay's host and port (for example 127.0.0.1:2238). Add one relay connection per program. The relayed packets come from QRZero, so anything the other program sends back (for example a double-click in GridTracker asking WSJT-X to call a station) reaches QRZero, not WSJT-X, and isn't passed back. If you need that, use a multicast group instead (see the FTx monitor page).

**Relay N1MM packets** does the same for N1MM Logger+'s broadcasts. The status shows how many packets have been passed on.

The older **Pass on to** box under Radios and programs does the same as a WSJT-X relay and still works.

## Startup programs

Under **Settings → Startup programs**, list the programs you always run with QRZero, such as PstRotatorAz, a rig control server or a band decoder program. QRZero starts the ticked ones each time it starts.

- **Program**: the full path, for example `C:\Program Files (x86)\PstRotatorAz\PstRotatorAz.exe`. Quotes around it are fine.
- **Arguments**: anything that goes after the program name, typed as you would in a shortcut.
- **Not if running**: leave the program alone when one with the same file name is already running, so restarting QRZero doesn't start a second copy. This check works on Windows only; elsewhere the program is always started.
- **Launch now** starts it straight away, to try it out.

Programs start one after another in the background, each in its own folder (many Windows programs look for their settings there), without a console window, and they keep running when QRZero closes. QRZero doesn't wait for them, so a slow or missing program never holds it up. If one can't be started, **Last start** says why, for example a mistyped path.

Startup programs run with your Windows account, like anything you start yourself, and can only be set up from QRZero's own settings.
