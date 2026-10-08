# Propagation

The **Propagation** pane shows how the sun and the Earth's magnetic field are treating the bands right now. It sits as a tab next to the Map to start with; show or hide it from **Layout**, drag it anywhere, or pop it out to its own window.

## Where the numbers come from

Everything except sunrise and sunset comes from N0NBH's solar data feed at hamqsl.com, the same numbers you see on the familiar solar banners on many ham websites. N0NBH gathers them from NOAA's Space Weather Prediction Center and other observatories, and works out the band conditions from them.

QRZero only fetches the feed while the pane is open, and at most once every 30 minutes, which is plenty since these numbers change slowly. **Refresh** asks for fresh numbers straight away, unless they were fetched in the last minute. The line at the top, for example "Updated 14:00 UTC by N0NBH", is the time N0NBH last updated the feed, not when QRZero fetched it.

If the feed can't be reached, the pane says so in red and keeps showing the last numbers it had, with the time they were fetched.

## The numbers

Each box shows a reading, and a word or two on what it means for you. Green is good news, amber is worth watching, red means trouble. Hover over a box for a short reminder.

| Reading | What it is | What you want |
| --- | --- | --- |
| **SFI** | Solar flux index, the sun's radio noise at 10.7 cm. It tracks how strongly the sun ionises the upper atmosphere. | Higher is better. Under 70 the high bands are mostly dead; above about 120, 15m and 10m open up well; 150 and up is great. |
| **SSN** | Sunspot number. More spots, more ionisation. | Higher is better, much like SFI. |
| **A** | A index, the day's average disturbance of the Earth's magnetic field. | Low. Under 8 is quiet; 16 to 29 is active; 30 and up is a storm with noise, fading and poor polar paths. |
| **K** | K index, the same thing over the last three hours, on a scale of 0 to 9. | 0 to 2 is quiet. 3 is unsettled, 4 is active, 5 and above is a geomagnetic storm. |
| **X-ray** | The sun's X-ray output, by flare class: A, B, C, M, X. | A or B. M and X flares cause short-wave fadeouts on the sunlit side of the Earth, from a few minutes to an hour or more. |
| **Solar wind** | How fast the stream of particles from the sun is moving, in km/s. | Below about 400 is calm. Above 500 it tends to stir up the magnetic field. |
| **Bz** | The north-south part of the magnetic field carried by the solar wind, in nanotesla. | Zero or positive (northward). A strong negative (southward) Bz lets the solar wind couple into the Earth's field, and storms often follow. |
| **Geomag** | N0NBH's one-word summary of the magnetic field: Quiet, Unsettled, Active, Storm. | Quiet. |
| **Noise** | The extra noise level you can expect on HF from geomagnetic activity, in S units. | S0 to S2. |
| **MUF** | Maximum usable frequency on a 3000 km path, measured by an ionosonde. The word under it is the highest band that the MUF reaches. | Higher than the band you want to use. |

Below the boxes are a few more readings for those who like them: foF2 (the highest frequency the F2 layer reflects straight up), the aurora level and how far south the aurora reaches, proton and electron flux (high values bring polar absorption), and the 304 Angstrom helium line, another measure of solar activity.

## Band conditions

The **HF band** table is N0NBH's estimate for the four band pairs (80m-40m, 30m-20m, 17m-15m, 12m-10m), for daytime and night time: **Good** in green, **Fair** in amber, **Poor** in red. The column that matches your location right now (day or night) is underlined.

The **VHF** table shows aurora in the northern hemisphere and sporadic E for Europe on 2m, 4m and 6m and for North America on 2m. Anything other than "Band Closed" is shown in blue, because it's worth a look.

These are broad estimates for the whole world. Your own path, antenna and noise will always have the last word, so spin the dial too.

## Sunrise and sunset

At the bottom, QRZero works out sunrise and sunset at your location (from the grid of the location in the top bar), in UTC, and says whether it's daylight or dark there now. When the other station's position is known (from their grid or a lookup), it shows theirs too. Paths along the grey line, where it's dawn or dusk at one or both ends, are often the best for long-distance contacts on the low bands.

The Map pane shades the night side of the Earth with the same calculation, moving once a minute, so you can see the grey line.
