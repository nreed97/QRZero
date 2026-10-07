# Radios and rig control

QRZero can read the frequency and mode from your radios and tune them. Each radio is set up as equipment at a location (**Settings → Equipment**): edit the radio and pick a **Rig control** method.

| Rig control | For | Settings |
| --- | --- | --- |
| Hamlib rigctld | Almost any radio Hamlib supports. Start `rigctld` first (for example `rigctld -m 2028 -r COM3 -s 38400`). | Host and port (default 127.0.0.1:4532) |
| TCI | ExpertSDR (SunSDR, ANAN with Thetis TCI), and Flex radios through a TCI bridge. Every receiver or **slice** shows up as its own radio. | Host and port (default 127.0.0.1:40001) |
| Kenwood / Elecraft / Flex CAT | Kenwood, Elecraft K3/K4/KX, and the SmartSDR CAT ports of a Flex. | Serial port (COM3) and baud |
| Yaesu CAT | FT-991A, FTDX10, FTDX101, FT-710 and other recent Yaesu radios. | Serial port and baud |
| Icom CI-V | Icom radios over USB or a CI-V interface. | Serial port, baud and the CI-V address (IC-7300 is 94, IC-7610 is 98, IC-705 is A4) |

QRZero connects to the radios at the location you have picked in the top bar. If a connection drops it tries again every few seconds; the QSO panel's status line shows what went wrong.

A serial port can only be used by one program at a time. If WSJT-X or another program also needs the radio, run Hamlib's `rigctld` (or the radio's own TCI server) and point everything at that.

## Following a radio

When radios are connected, the **Radio** box appears in the QSO panel's title bar. Pick a radio and the panel follows it: the frequency, band and mode change as you tune. While it follows a radio you control:

- Typing a frequency and leaving the box tunes the radio.
- Changing the mode sets the radio's mode (FT8, FT4 and other digital modes use the radio's USB data mode).
- Picking a station in the FTx monitor or a spot tunes the radio to it.

Choose **manual** to type frequency and mode yourself.

WSJT-X and N1MM radios are listed too (when those programs are connected), so the panel can follow them, but they can't be tuned from QRZero.

## Several radios (SO2R)

With two or more radios, switch the radio the QSO panel follows:

| Key | Action |
| --- | --- |
| <kbd>Alt</kbd>+<kbd>1</kbd>, <kbd>Alt</kbd>+<kbd>2</kbd> … | Follow radio 1, 2 … |
| <kbd>`</kbd> | Swap between the first two radios |

The rig recorded with each QSO (`MY_RIG`) is the radio being followed.

## Rotator

With PstRotatorAz set up (**Settings → Radios and programs**), the map shows the rotator's heading and **Turn SP** / **LP** buttons that turn the antenna to the short or long path of the station you're working.
