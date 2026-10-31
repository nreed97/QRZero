# POTA spots

The **POTA spots** pane lists the activators on the air in Parks on the Air right now, taken straight from the POTA spot list (api.pota.app). It is off to start with; turn it on from **Layout**. You don't need a login or a cluster connection.

## What it shows

The newest spot is on top. Each row has the time (UTC), the activator, the frequency, the mode, the park reference and name, the location, the activator's country, who spotted them, the comment and flags. A green **New DXCC**, **New band** or **New mode** flag marks an activator that would be new for you, and **Worked** (the row is grey) marks one you have already worked on that band and mode. The flags use your open log, the same as in the Cluster and Band map panes, and they update when you log a QSO.

Choose a band, a mode group, **Needed only** or **Hide worked** to narrow the list; the choices are remembered. Drag a column heading to move it or its right edge to resize it, and double-click the edge to put the width back.

## Filling in a QSO

**Click** a spot to fill in the QSO panel with the call, band, mode and the park reference in the **Their POTA** field. **Double-click** it to fill in and tune the radio, the same as in the Cluster pane. The park comes from the spot you click, never from an earlier contact with that station. If **Their POTA** isn't in your QSO panel, add it under **Settings**, **Entry fields** (the *Parks and summits* preset has it); the reference is logged either way.

## How often it reads the list

QRZero reads the POTA list when the pane is open and then once a minute, and keeps it for half a minute so that several panes share one request. **Refresh** reads it at once. Nothing is read while the pane is closed. If the list can't be reached, the last one stays on screen with the reason above it.

## On the band map

The **Band map** has a **POTA** tick box in its toolbar. With it on, the activators on the shown band appear on the map as spots with a dashed edge and their park reference, and **Needed only** applies to them. The band map reads the list once a minute only while the box is ticked.
