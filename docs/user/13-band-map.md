# Band map

The band map shows one band as a frequency scale running down the left side of the pane, with the DX cluster spots and your FT8 and FT4 decodes laid out beside it. It's the quickest way to see who is on the band and where. Open it from **Layout**, **Band map**. It sits with the Log and Cluster tabs to start with; drag the tab anywhere you like, or pop it out to its own window.

## Following the radio

When the QSO panel follows a radio, the band map shows that radio's band and keeps its frequency in the middle of the scale. A red line with a small arrow on the left marks where the radio is. Tune the radio and the map moves with it.

To look at another band, pick it from the band list, or scroll with the mouse wheel: the map stops following the radio. Tick **Follow radio** to go back.

With no radio, pick the band from the list (20m to start with), then scroll up and down the band with the mouse wheel or the **Lower** and **Higher** buttons.

## Zoom

**+** and **-** zoom in and out, and so does <kbd>Ctrl</kbd> with the mouse wheel. The figure next to them is how much of the band one pixel covers. Each band remembers its own zoom on this computer.

## Spots

Each spot shows the call, how long ago it was spotted, and what it would be new for, in the same colour as the Cluster pane: **DXCC**, **band** or **mode**. Calls you've already worked on the band are shown in grey. Hover over a spot for the exact frequency, mode, country, spotter and comment.

Spots never sit on top of each other. When several are close together they stack in a column, and when a column fills up they move to the next one. A thin line runs from each spot back to its frequency on the scale.

Older spots fade out. The map uses the age setting from the Cluster pane (**Last 30 min** to start with), so spots older than that are left off. If a call is spotted again on another frequency, only the newest spot is shown.

**Click a spot** to fill in the QSO panel. When the panel follows a radio QRZero controls, the radio tunes to the spot and changes mode, the same as clicking a spot in the Cluster pane.

Cluster spots are coloured by mode, the same as in the Cluster pane: blue for CW, green for digital and orange for phone, shown as the call's colour and a bar on its left edge. A worked station's call stays grey. The key is in the toolbar.

**Needed only** hides spots and decodes that aren't new for DXCC, band or mode. When there are more spots above or below what you can see, a small note at the top or bottom of the scale says how many. Click it to jump to the nearest one.

## FT8 and FT4 decodes

WSJT-X and JTDX decodes don't each get their own place on the scale, since they all sit within a few kilohertz of the dial frequency. Instead each WSJT-X or JTDX on the band gets one compact box at its dial frequency, with the calls it has decoded in the last three minutes. Needed calls come first, in colour, then stations calling CQ (in bold), then the strongest. A short blue bar on the scale shows the 3 kHz the program is listening to. Click a call to fill in the QSO panel, as in the FTx monitor.

## Band plan strip

A thin coloured strip down the left edge shows the band plan:

| Colour | Segment |
| --- | --- |
| Blue | CW only |
| Green | CW and data (in the US plan) or narrow digital modes (in the IARU plan) |
| Orange | Phone and all modes |
| Grey | All modes |
| Red | Beacons |

Choose **US plan** (the FCC CW/data and phone sub-bands), **IARU R1 plan** (the Region 1 HF band plan, simplified) or **No plan** from the list on the right. Hover over the strip to see the segment's edges. Parts of the scale outside the band are shaded.
