# Radios and rig control

QRZero can read the frequency and mode from your radios and tune them. Each radio is set up as equipment at a location (**Settings → Equipment**): edit the radio and pick a **Rig control** method.

| Rig control | For | Settings |
| --- | --- | --- |
| Hamlib rigctld | Almost any radio Hamlib supports. Start `rigctld` first (for example `rigctld -m 2028 -r COM3 -s 38400`). | Host and port (default 127.0.0.1:4532) |
| TCI | ExpertSDR (SunSDR, ANAN with Thetis TCI), and Flex radios through a TCI bridge. Every receiver or **slice** shows up as its own radio, and a slice you close on the radio disappears from the list. | Host and port (default 127.0.0.1:40001) |
| Kenwood / Elecraft / Flex CAT | Kenwood, Elecraft K3/K4/KX, and the SmartSDR CAT ports of a Flex. | Serial port (COM3 on Windows, `/dev/cu.usbserial-…` on a Mac, `/dev/ttyUSB0` on Linux) and baud |
| Yaesu CAT | FT-991A, FTDX10, FTDX101, FT-710 and other recent Yaesu radios. | Serial port and baud |
| Icom CI-V | Icom radios over USB or a CI-V interface. | Serial port, baud and the CI-V address (IC-7300 is 94, IC-7610 is 98, IC-705 is A4) |

QRZero connects to the radios at the location you have picked in the top bar. If a connection drops it tries again every few seconds; the QSO panel's status line shows what went wrong.

A serial port can only be used by one program at a time. If WSJT-X or another program also needs the radio, run Hamlib's `rigctld` (or the radio's own TCI server) and point everything at that.

## Following a radio

When radios are connected, the **Radio** box appears in the QSO panel's title bar. Pick a radio and the panel follows it: the frequency, band and mode change as you tune. While it follows a radio you control:

- Typing a frequency and leaving the box tunes the radio.
- Changing the mode sets the radio's mode (FT8, FT4 and other digital modes use the radio's USB data mode).
- Double-clicking a spot (Cluster, Band map, Needed now, Watch list, DXpeditions) fills it in and tunes the radio to it. A single click only fills in the QSO panel. Picking a station in the FTx monitor never retunes the radio.

Choose **manual** to type frequency and mode yourself.

## Split

When a radio is in split, the VFO readout in the header shows both frequencies, **RX** for the one you listen on and **TX** for the one you transmit on. The QSO panel shows the transmit frequency in the first box (labelled **TX MHz**) and the receive frequency in a second box (**RX MHz**), and both are logged: the QSO's frequency is where you transmitted and its RX frequency is where you listened. Typing a frequency in the **TX MHz** box and leaving it sets the radio's transmit frequency. Out of split, the panel goes back to a single **Freq MHz** box.

Split works over every rig control method, with these limits:

- **TCI** and **Hamlib (rigctld)**: split is read and set. Hamlib needs a radio whose driver supports split (`rigctl` commands `s`, `S` and `I`).
- **Kenwood, Elecraft and Flex CAT**, and **Yaesu CAT**: split is read and set. Setting it receives on VFO A and transmits on VFO B.
- **Icom CI-V**: split is read and set on radios that can read the other VFO's frequency (IC-7300, IC-7610, IC-705, IC-9700 and similar). Older Icoms show the single frequency, and QRZero turns split off but can't set the transmit frequency.
- **WSJT-X** radios show the single frequency they report.

Radios that don't report split show and log one frequency, as before.

### Split from spots

When you double-click a cluster or band map spot whose comment says where the DX is listening, QRZero tunes the radio to the spot and sets split with the transmit frequency to match:

- `UP 5`, `UP5`, `UP 5K`: 5 kHz above the spot. A bare `UP` means 1 kHz. `UP 5-10` uses the lower figure, 5.
- `DN 2` or `DOWN 2`: 2 kHz below.
- `QSX 14.205` (MHz), `QSX 14205` (kHz) or `QSX 205` (the last three digits of the spot's kHz). `LISTENING` and `LSN` work the same way.

Anything more than 100 kHz from the spot is ignored. Spots that set split show it in the row's tooltip. Double-clicking a spot that doesn't say turns split off, so the radio never keeps the last station's transmit frequency.

WSJT-X radios are listed too (when it is connected), so the panel can follow them, but they can't be tuned from QRZero.

## VFO readout

The top bar shows the radio the QSO panel follows: its name, frequency (for example `14.074.000`), band, mode and **RX** or **TX**. The readout turns red while the radio transmits, and shows `off` when the radio isn't connected (hover over it to see why).

With more than one radio (two Flex slices, a second WSJT-X), a small **+1** (or **+2** …) button sits next to it. It turns red when one of those radios transmits. Click it for a list of every radio with its frequency and state, and pick one to follow it.

## Several radios (SO2R)

With two or more radios, switch the radio the QSO panel follows:

| Key | Action |
| --- | --- |
| <kbd>Alt</kbd>+<kbd>1</kbd>, <kbd>Alt</kbd>+<kbd>2</kbd> … | Follow radio 1, 2 … |
| <kbd>`</kbd> | Swap between the first two radios |

The rig recorded with each QSO (`MY_RIG`) is the radio being followed.

## Rotator

QRZero can talk to a rotator directly or through PstRotatorAz. In **Settings → Radios and programs → Rotator**, tick **Control a rotator** and pick how to connect:

- **PstRotatorAz (UDP)**: the existing link; in PstRotatorAz turn on UDP Control (port 12000).
- **Hamlib rotctld (TCP)**: give the address of a running `rotctld`, normally `127.0.0.1:4533`. This covers any rotator Hamlib supports.
- **Yaesu GS-232 (serial or TCP)**: for controllers that emulate the GS-232, which most do. Type the serial port (for example `COM5`, or `/dev/cu.usbserial-1` on a Mac) and baud rate, or leave the port blank and give a network address like `192.168.1.50:23`. Use this or rotctld for a network controller such as the AF6SA WRC, whichever protocol its manual lists (the WRC also has a PstRotator-compatible telnet mode, which is not the same as these).

GS-232 replies don't need a line break at the end, which is how the WRC answers. If the controller answers with something QRZero can't read as a heading, the status line shows what it said. The settings box shows whether the connection is up. QRZero reconnects by itself if the controller goes away.

### Turning the antenna from N1MM

Tick **Let N1MM (or another program) turn the rotator through QRZero** under the rotator settings. QRZero then listens for PstRotatorAz-style UDP commands (default `127.0.0.1:12040`) and passes every bearing, and stop, to whichever rotator connection you set up above, and answers when the program asks for the heading. N1MM's own rotor messages (what **Alt+J** sends) work as well as PstRotatorAz-style ones. In N1MM, under **Config → Configure Ports**, set the rotor's UDP address to the one above. Use this when N1MM is the program you want to drive the antenna and QRZero holds the connection to the controller. Don't point it at the same port PstRotatorAz itself uses.

With a rotator set up, the map shows the rotator's heading and **Turn SP** / **LP** buttons that turn the antenna to the short or long path of the station you're working.

For another rotator program, or to send the heading somewhere else as well, add a UDP connection that sends on **Rotator turn request** (see *UDP connections and startup programs*). The Rotator pane works with that alone too, though it only shows the current heading when the rotator above is on.

## Rotator pane

The **Rotator** pane gives the rotator a proper control of its own. Open it from **Layout**, **Rotator**. It shares a tab with the map to start with; drag it anywhere you like, or pop it out to its own window.

The compass dial shows where the antenna points now, with the heading in degrees at the top. When you have a station in the QSO panel whose location QRZero knows, an orange **SP** arrow marks the short-path bearing and a grey **LP** arrow the long path.

Tick **Map** next to the heading to draw an azimuthal map behind the dial. It is centred on your location with north at the top, so the dial's bearings line up with the map: the other station sits at its true bearing and distance, and a dotted line from the centre follows the rotator's heading as a great circle (on this projection every great circle through your location is a straight line). It needs your grid under **Settings**; the choice is remembered.

There are several ways to turn the antenna:

- **Click anywhere on the dial** to turn to that bearing. As you move the mouse over the dial, QRZero shows the bearing you'd turn to.
- **Short path** and **Long path** turn to the station in the QSO panel.
- Type a bearing in the box and press **Go** (or <kbd>Enter</kbd>).
- The preset buttons, for example **EU 45°** or **JA 330°**, turn to places you call often. Click **Edit presets** to rename them, change their bearings, remove them or add your own. **Add** starts a new preset at the current heading. Presets are kept on this computer.

While the antenna is moving you'll see **Turning to 123°**. It clears once the heading is within 3° of where you asked for, or after a minute and a half if the rotator doesn't get there.

If no rotator is set up yet, the pane says so and offers a button that opens **Settings → Radios and programs**, where you turn it on. If the heading shows `---`, QRZero isn't hearing back from the rotator: check the status line in that settings box (for PstRotatorAz, that its UDP control is on).
