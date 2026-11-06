<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Night drive

Night drive is the page's fourth theme. It draws the music as an outrun road at dusk: a striped sun
on the horizon, mountains along it that are the live spectrum, and a floor of lit cells that rolls
toward you, one row of the music at each step. The volume is a large neon number, the controls are a
strip along the bottom, and everything else is on a sheet that slides over the scene. It is dark only.
It is plain ES modules with no build step, in `bins/svolderd/web/themes/nightdrive/`, and one entry
in `bins/svolderd/src/web/themes.rs`. It is built on the core alone: no file of Ember's, Paper's or
Signal's belongs to it, and the gaps it found in the core were fixed in the core ([the second page](web-nightdrive-checks.md#what-night-drive-found-missing-in-the-core)).
[Web themes](web-themes.md) says what a theme is; [the frame](frame.md) says what the page keeps
for itself.

In this theme's words, **the scene** is the canvases and what the HUD lays over them, **the
mountains** are the spectrum's ridge, **the floor** is the grid of cells in front of it, **the dash**
is the strip along the bottom and **the sheet** is the panel of everything else about the device.
**Motion** is the theme's one setting: Full, Calm or Still.

## The scene

Two canvases fill the window under the HUD. The sky canvas carries the gradient, the stars, the
sun (striped, with a halo) and the wet road's reflection. The road canvas carries the mountains, the
floor, the haze, the embers and the tuning bubbles. The camera is one projection
(`camera.js`, `CAMERA`): the floor is seen from high, `z` is 1 at the bottom row and 0 at the horizon,
`K` is 8 and `spread` 0.24. The horizon stands at 0.56 of the scene's height, and at 0.62 on a phone
held upright (`horizonOf`). The plain grid beside the spectrum's columns runs out to 30 and is
drawn faint.

**One frequency axis.** The screen's `x` is `w * ln(hz / 20) / ln(1000)`, 20 Hz at the left edge and
20 kHz at the right (`axis.js`). The mountains, the tone curve, the bubbles and the floor's 24
columns all use it, so a peak of the music stands above the band that EQ would move. A band of the
core's analysis (240 at 1/24 octave from 20 Hz, `core/viz-model.js`) goes to the column its centre
falls in.

**The music's levels** (`music.js`). A band's level is the larger of the left and right of the
core's smoothed bands (0 to 1 over -72 to 0 dBFS). The ridge's unit is `(72 v - 12) / 60` clamped to
0..1, so -60 dB is nothing and 0 dB is full. A column's level is the mean of its bands' units.
The bass is the mean of columns 0 to 5 (20 to 112 Hz) and the highs the mean of columns 19 to 23
(4.7 to 20 kHz). With no analysis the columns hold 0.05 and the ridge is flat.

**The mountains** (`ridge.js`, `draw/road.js`). The band units are smoothed across frequency
(`RIDGE.sigma` 0.87, `radius` 2, so a notch of two bands still stands) and read every `RIDGE.step` (3)
CSS pixels at up to `RIDGE.scale` (0.2) of the scene's height
over the horizon. Under them lies a slow swell, `RIDGE.swell`, that is all the ridge does in silence;
it drifts one wavelength in `period` (10 s) in Full, at half that rate in Calm and not at all in
Still, and the music takes the ridge over entirely at level `gives` (0.15).

**The floor** (`floor.js`, `draw/road.js`). A row is the 24 column levels at the moment it is born
at the horizon; rows are born `rowRate(bpm)` times a second, `clamp(2 * bpm / 60, 2, 6)` and 3.5
with no tempo (the daemon's beat period, read while it is under a second old), times the mood's
cruise factor (below). The scene keeps `CAMERA.rows` (22) rows plus `FLOOR.extra` (2) beyond the
bottom edge. A cell is lifted toward you in proportion to its level (`FLOOR.height`, 0.25 of the
scene's height at full level), and the near edge moves continuously as the rows roll, with no jump at
a birth. A cell's colour and alpha are read from its level against the floor's own recent loudest
cell (`FLOOR.heatFloor` 0.15, relaxing over `PEAK_TAU`, 60 s), raised to `FLOOR.heatCurve` (0.6), so a
quiet record and a loud one use the whole range of colours. The colour is a ramp from dark violet
through pink and orange to pale yellow (`draw/heat.js`, `RAMP`), cut into `STEPS` (64) colours once,
and it leans to a warm ramp for bass-heavy music and a cool one for bright music. The road stops (rows frozen) in Calm and Still, while the device
is out of reach and while the stream is lost.

**Draws that do not allocate.** The floor and the ridge reuse their arrays; `nightdrive-glow.mjs`
fails on `shadowBlur`: the glow is drawn as stacked strokes and gradients, never blurred, because a
blur costs a phone far more than anything else in the scene.

## The mood

The mood is what the scene makes of the music's loudness and balance, not of any one frame. It is
three small modules, `mood.js` (the ear), `look.js` and `embers.js`, all pure, all counted in seconds
and never in frames, so a screen at 30 or at 144 draws a second gives the same night.

**The ear** (`mood.js`, `EAR`). A level is read against the loudest of the last minute: the peak
falls away with a time constant of `peakFall` (60 s) and never under `peakFloor` (0.05), and `rel` is
the level over it. `med` and `long` are `rel` eased over 1.2 s and 8 s; Calm doubles both rates.
The balance is the highs (the last 80 bands) against the bass (the first 48), eased over 6 s, 0.5 for
even; it is held while the level is under `silent` (0.02). The ear starts at `start` (0.7), so a page
that has just opened looks as the scene always has until the music says otherwise. A **moment** is
`med` rising `momentRise` (0.2) over `long` while over `momentMin` (0.6), and not again for
`momentGap` (15 s); Still finds none. **Embers** come when `med` has stayed over `loud` (0.78) for
`emberAfter` (6 s).

**The look** (`look.js`, `LOOK`). From the ear's state:

- the night's `mood`, 0 for deep night to 1 for warm and loud (`mood0` 0.18 and `moodSpan` 0.82
  over `med`; the sky, the sun's colours and the heat lean to it);
- the road's speed, the tempo's row rate times `cruiseBase + cruiseSpan * long` (0.6 and 0.8),
  eased over `cruiseTau` (2 s);
- `wet`, a line from `wetFrom` (0.75) falling with `wetSlope` (1.1) as `med` rises, at most
  `wetMax` (0.6): the road is wet in the quiet and in the gaps and dry at a sustained peak;
- `haze`, the same from `hazeFrom` (0.85), `hazeSlope` (1.2), at most `hazeMax` (0.75): a haze over
  the far floor and the foot of the mountains in the quiet;
- where the heat leans, from the balance (`balanceMid` 0.45, `balanceGain` 4): warm for bass, cool
  for highs;
- a moment's **flare** at the sun and one **wave** that runs down the floor (`flareRise` 0.15 s,
  `flareTau` 0.8 s, gone after `flareFor` 2.5 s); Calm shows the flare at `calmFlare` (half) with no
  wave, and Still none.

**The wet road** (`draw/mood.js`, `WET`) mirrors the sky, the sun and the mountains in the floor as
`slices` (32) thin horizontal slices, reaching `reach` (0.85) of the way down. The slices sway
together as one wave, the more the nearer. The mirror is the sky just drawn with the mountains over
it, copied fresh at every draw (`reflect` in `scene-wire.js`, at the scene's own density), so the
reflection never trails the sky and the ridge is in it. It is the costliest layer, so [the budget](web-nightdrive-checks.md#the-frame-budget) owns it: it is the first thing a slow draw gives up, and it
stays off for `BUDGET.wetHeldS` (180 s) before the scene tries it again. While the music is silent
or the player is not playing the hold does not apply: the wet road returns as soon as the draws
measure under `calmMs`, and a step-down drops it again if it pushes them over `slowMs`.

**Embers** (`embers.js`, `EMBERS`) rise from the floor's hot near cells while `embers` is on: at most
`max` (90) alive at once, fewer on a small canvas (`capOf`, never under `min` 24), each living `life`
(1.83 s), born by the clock.

**The sky's rare events** (`sky.js`, `SKY`) are scheduled by the clock, only in Full, with the
analysis running and the device reachable. A band is read as its level eased over `envTau` (0.4 s)
and placed in the last minute's range: `(env - trough) / max(range, peak - trough)`, the peak and
trough relaxing toward the envelope over `rangeTau` (60 s) with `range` (0.01) the least range that
counts, so a flat drone or near silence fires nothing and a quiet record fires like a loud one. A
place of 0.8 or more fills a leaky count at `fill` (1 a second) and anything under drains it at
`drain` (0.5 a second).

- **Rays** from the sun when the bass place has held `bassHold` (3 s), at least `raysGap` (60 s)
  after the last rays began; the envelope is `raysIn` 2 s in, `raysHold` 6 s, `raysOut` 4 s out at
  most `raysAlpha` (0.55), turning clockwise at `raysSpin` (6 degrees a second).
- **A shooting star** when the highs jump `starJump` (0.45 of the minute's loudest) while over
  `starMin` (0.5), at least `starGap` (5 s) after the last, with a chance of `starChance` (0.5); it
  lasts `starLife` (0.8 s). The highs' average is `starAvg` (1 s), a time constant.
- **A shower** when the highs place has held `showerHold` (2 s): `showerStars` (3) and up to
  `showerMore` (3) more over `showerSpan` (1.5 s), at most one every `showerGap` (40 s).

The scheduler keeps a log of its last firings with their inputs, which `driver.sky()` returns. The
checks run the same input at 30, 60 and 144 steps a second and with a 2 s gap and expect the same
firings (`nightdrive-sky.mjs`, `the_limits_hold_at_any_frame_rate`).

## The HUD

Over the scene there are three lines of words and the fade.

- **The top line**, at the left: `<device> · <link word> · <input> · <rate>`, each left out when the
  model has none. The link word (`not connected` and its kind) shows only while the link is not on.
  At the right is the path's verdict: `host EQ on · changes the sound` in amber when the shown
  device's path has a `computer` hop that changes the sound, `host EQ on` alone when the route has it
  on and the path says nothing changes, else nothing. Trouble replaces the verdict with the core
  title's status words (`reconnecting`, a card's title, `fault`, `reading again`, `resting`), in red
  for a lost link, a card or a fault and amber for a page reading again or resting.
- **The volume**, a large neon number in Russo One with the unit in Mr Dafoe. Muted, it reads `−∞`
  with the `dB`. Russo One has neither the minus (U+2212) nor the infinity, so those two are drawn as
  paths (`draw/volume.js`). An amp with no volume of its own says, in Plex Sans in its place, that
  the DAC's volume sets the level.
- **The EQ line**: `EQ`, then the applied preset's name when there is one, then the gains with a
  sign and one decimal (`−` U+2212, `0.0` bare). In Loudness it reads `LOUDNESS <strength>`, in the
  flat mode `FLAT`; a device with no tone control has no line. The model writes a negative gain
  with U+2212, which `parseFloat` does not read, so `gainWords` replaces it first.

**The fade** (`hud-wire.js`). After `fade-after` seconds with no `pointermove`, `pointerdown`,
`keydown`, `wheel`, `touchstart` or `focusin` (the preference of that name, default 15, 1 to 600,
read when the theme mounts), the root carries `nd-faded` and the top line, the volume and the EQ line
go to opacity 0 over 2.5 s; waking removes it with no transition. It is opacity only: the faded HUD
stays in the tab order and the accessibility tree. A volume that changed and is not this page's own
ask (`OWN_MS`, 3 s, covers its own) shows the volume alone for `SHOW_VOLUME_MS` (5 s). **Trouble holds
the fade off**: a status word but `connected`, a card, the quiet line. Trouble that arrives while the
HUD is faded brings the top line back with its words and keeps it for as long as the trouble lasts.
On a phone held upright the volume is in the scene with the rest of the HUD, and fades with it.

**The dash fades with it** (`dash-fade.js`). When the HUD has faded, the dash goes to 10 % opacity
(the root carries `nd-dash-dim`; opacity only, over 2.5 s, none under reduced motion) on a desktop and
on a phone held sideways. The panel of a phone held upright never fades. The dash stays up while a
key of it is held or a list is open, while it has the keyboard focus (`:focus-visible`) and while a
mouse rests over it. Whatever wakes the HUD wakes the dash. The input that wakes a faded dash only
wakes it: a press that wakes it does not reach the key under the finger, and is deaf from its start
until `CLICK_MS` (100 ms) after its release, so a long press that woke the dash presses nothing when
it is let go. A keydown, or any other press, clears that state, so a keyboard press is never
swallowed and no lost release leaves it stuck.

**The cursor rests** (`rest.js`): after `REST_MS` (2 s) with no move over the scene, not over the
dash, the HUD or a menu, and not while a button is held, the root carries `nd-rest` and the cursor is
hidden over the scene. A fine pointer only.

## The dash

The strip along the bottom holds, in this order: `−` `MUTE` `+` (the core's hold: a step at the press
and, after 380 ms, every 70 ms, up to its 10 s cap; muted reads `−∞`), the input, the cover and the
track (title; artist, album and elapsed over length; a progress bar), previous, play or pause and
next as inline SVG, and `DEVICE ▴`, which opens the sheet. The input is chips when the device has
`CHIPS_MAX` (3, `draw/dash.js`) options or fewer, else one chip showing the current one that opens a
list (`USB ▾`). An amp has its power key and its temperature and power gauges where the input would
be. The keys are 40 px high on a desktop and 44 px on a phone, and the hit area is 44 px.

**Space plays and pauses.** The frame's reserved key does it ([web-themes.md](web-themes.md)); Night
drive binds nothing for it. The dash's play button posts the same press.

## Tuning on the mountains

The pointer over the band of the scene around the horizon (`BAND`: 0.32 of the height above it and
0.10 below, `tune.js`) turns the mountains into the tone curve with a bubble for each band; they
turn back after `GRACE_MS` (1.5 s) with no drag, wheel notch or lifted finger. The morph eases at
`KEEP` (0.88) of the gap each frame at 60 a second. Bubbles are drawn above a morph of 0.02 and are
grabbed from 0.5, so a touch on the mountains that has not yet turned them only shows the curve. The
scene carries `data-tune="on"` while it is on. Never for a guest, a page whose writes are off or a
device with no bands; in Loudness the curve shows with no bubbles.

The map (`mountainMap`): `hzAt` is the axis above, the gain is a line from the baseline at
`H - 0.08 h` with `0.14 h` of span for the largest `|gain|` of the device's `tone_steps.gain`
(`GAIN_FALLBACK`, 12 dB, when it lists none). The curve drawn is the model's `tone.response`; each
bubble sits at its band's own gain, so it stays under the finger. A press takes the nearest bubble or
diamond within `HOVER_PX` (24 CSS px), the bubble on a tie. A drag goes through the core's drag
(`core/tone-drag.js`), which writes frequency and gain with the pacing and the volume rule of the
core; every coordinate here is in CSS pixels of the scene, so a bubble is grabbed where it is drawn
at any density. While a drag is on, and until its writes are answered, the curve and the bubbles are
drawn from the tone as it stood at the press with the values the drag asks for, so a reading never
moves a bubble under the hand.

**Q from a diamond.** The lit band's bell shows two diamonds where its curve crosses half its gain.
`halfPoints` finds them by bisection on the core's `responseDb` of that band alone (an axis end when
the curve never crosses; for a gain under `FLAT_DB` 0.05 the analog width). A press on a diamond
drags the Q: `qAtDiamond` picks the Q step whose half point is nearest the pointer in log distance,
and the core's pinch writes it (`spread(q0 / qs[i])`). The wheel over the mountains steps the Q of the
band nearest in `x`, and two fingers pinch the band nearest their middle. The closed form is not
used: the bilinear curve's half-gain points drift from it by up to 6 dB above 4 kHz.

A device switch while a drag is held ends the drag with `cancel('switch')`: nothing is written to
the device now shown. `onSwitch` does it (`live.js`).

## The sheet

`DEVICE ▴` or the menu's `Device…` row opens the sheet, a dialog over the scene. While it stands the
scene and the dash under it are `inert` and dimmed (the scene goes on drawing), the focus goes in,
and the ✕ button, Escape or a tap on the scene closes it and gives the focus back to what opened it.

- **On a desktop** (720 px wide and more and 500 px high and more) it is two columns, `1.15fr 1fr`,
  at most 1100 px or 94 % of the width, with `--nd-sheet-top` and `--nd-sheet-bottom` margins above and below it. Row one: **Mode** (a chip for each mode the device has; while Loudness is on its chip
  carries a round `−` and `+` and the strength under its name) and **Input**. Row two: the **band
  table** (typed values with `−` and `+`, one device step a tap, a held one repeats) and the
  **presets** (chips; Save as, Rename and Delete ask their question in place), the device's slots and
  its controls (balance and sliders as scales, toggles as pills), an amp's meters with their zones.
  The device's **facts** fold across both columns; the fold is kept as the preference `facts`
  (`open` or `closed`), works when storage is refused, and a guest may open it.
- **On a phone held upright** the sheet slides up from the dash to 30 % from the top, one column:
  Mode, Input, presets, bands, slots, controls, facts, scrolling inside.
- **On a phone held sideways** (under 500 px high) it slides in from the right, `min(420px, 60vw)`
  wide, the same column.

The dash on a phone held upright is a panel under the scene that holds only its own rows, and the
scene takes the rest of the height, about 75 % of it. The volume figure is in the scene,
`min(60vw, 20dvh)` wide, and the EQ line shows its values there.

The presets are what the model offers: a preset made by AutoEq is one of the chips. There is no
AutoEq picker in this theme; adding one is a new feature for every front end.

Every write goes through `core.actions`; a question that asks before it writes (delete a preset,
replace what a slot holds) has the safe answer focused, and an answer the daemon refuses leaves the
question open with what was typed. A slot save posts `"confirm": true` only after the question is
answered. The sheet is drawn again from every answer, so a section in use is held (`live.js`
`hold`/`release`): a value being typed, a list open, a question asked, a stepper held, and what
came meanwhile is drawn from the newest model when it is let go.

The bubbles are not focusable. The band table is the keyboard path to every band value.

## Kinds of device

- **A DAC** has the volume, the tone control on the mountains and the sheet above.
- **An amp** (Forkbeard amp; the fake amp, the five-meter amp and the mono amp in the checks) has no
  tone control, so the mountains stay on the music and no bubble appears. Its dash has the power key
  and the temperature and power gauges, its sheet has the controls and the meters (needles and the
  class A share), and the HUD says the amp has no volume of its own. The theme listens to the stream's
  `meters` event and puts each reading into the needles in place, without drawing the sheet again
  (`controls-wire.js`). The temperature gauge is
  unproven on a real amp: no profile reports a temperature as a reading, only over-temperature
  faults, so it is checked on node fixtures only.
- **A Loki Max** has its slots and settings on the sheet like Signal's shelf.
- **A device that is not the shown one** is reached with `{ device: url }` on every action, so
  choosing never switches the device shown.

## The states

- **Stream lost.** The scene is drawn dull under a veil, the quiet line says `Reconnecting to
  svolder…`, the dash and the HUD are inert and the mountains lie flat.
- **A device out of reach** (a lost link, waiting, resting). The scene is dull (greys from
  `PALETTE`, the road stopped, the mountains flat), a card in the middle says why with `Try now` and
  `next try in 12 s` from the daemon's `retry_in`, and the dash is inert. The last heard volume stays
  in view, dimmed.
- **Reading again or resting.** The scene is dull, the quiet line says `reading the device again…`
  or that the page is paused while hidden. A dull scene is drawn `STILL_PACE` (500 ms) apart.
- **A fault on a connected device.** The top line goes red and the controls stay live; the daemon's
  own refusals apply.
- **No music analysis** (the capture is unavailable). The scene stays lit; the mountains take the
  shape of the tone curve and breathe (not in Still), and the quiet line says
  `no music analysis: <why>`.
- **A refused write.** The core's toast, and the device is read again.
- **A guest or a read-only viewer.** Every control is drawn, inert, and a tap toasts the core's
  words; the mountains never turn into the curve.

The page does not scroll: the theme root is `100vw` by `100dvh`, the scene has `touch-action: none`
and the sheet scrolls inside itself. The HUD sizes with `clamp()` against the smaller side, so a tall
window (1280 by 1600) keeps the sun, the volume and the dash whole.

## Look

The tokens are in `style.css` on `:root[data-theme="nightdrive"]`; the canvases' colours are
literals in one object, `PALETTE` in `draw/palette.js`.

| Token | Value |
| --- | --- |
| night, dusk, deep | `#0a0018`, `#5b0f5e`, `#05030a` |
| ink, mid, dim | `#f3e9ff`, `#b9a9cf`, `#8a7aa3` |
| pink, pink ink | `#ff3cc8`, `#ffb3ec` |
| track, track sub | `#ff9ae6`, `#b388c9` |
| cyan, cyan ink, cyan key | `#22d3ee`, `#7ff3ff`, `#b5f7ff` |
| warn, bad | `#ffd166`, `#ff5a6e` |
| hairline, panel, sheet | `rgba(243,233,255,.10)`, `rgba(10,0,24,.62)`, `rgba(12,6,24,.9)` |

The sky is a gradient from `#0a0018` to `#5b0f5e` at the horizon; the sun runs `#ffd54a`, `#ff8a3d`,
`#ff2d78` with a pink halo and stripes in `#5b0f5e`; the mountains are filled `#14002a` and edged
`#22d3ee`. At the mood's low end the sky and the sun lean to `skyDim` and `sunDim`, at its high end to
`skyWarm` and `sunWarm`. A dull scene (out of reach, lost, reading again) uses the greys (`#0c0a12`,
`#2c2834`, the sun `#8a8592` to `#4a4652`). Text over the panel and the sheet reads at
4.5 to 1 or better (the contrast check Paper and Signal use).

Text is IBM Plex Mono and IBM Plex Sans. The two other fonts are Russo One (the volume's digits) and
Mr Dafoe (the volume's unit), both under the SIL Open Font License (`OFL-RussoOne.txt` and
`OFL-MrDafoe.txt` beside them) and served by the daemon from the page's own origin, so the page makes
no new request to anyone else. There is no scheme switch and no effects layer of Ember's.

The menu button is the frame's waves icon over a 44 px slot at the scene's top right, which the top
line and the EQ line keep clear of; it is anchored to the window (`fixed`) at every width. The menu
carries the three `Motion` rows (Full, Calm, Still), a rule and the `Device…` row, offered while a
device is shown. The preview in the picker and the menu is an SVG of the dusk gradient, the striped
sun, a cyan ridge and four lines of the road, with no font. Night drive needs `meters`, `host` and
`analysis`, and its only scheme is `dark`; `theme_color` is `#0a0018`.

Nothing Night drive draws carries a `style=` attribute or an inline script or style: positions are set
through the style object or as SVG attributes, and the canvases are drawn from script. Browser storage
is only through `core.prefs`, under `svolder.theme.nightdrive.<name>`: `motion`, `facts` and
`fade-after`.

**The analysis is held only while the scene is on show.** The theme opens `core.analysis` when the
canvas shows, lets it go once the scene has been hidden for the core's grace and on unmount, so the
daemon captures the computer's audio only for a page that is looked at.

## The files

| File | What it holds |
| --- | --- |
| `nightdrive.js` | the registration, the mount, the menu rows |
| `camera.js`, `axis.js`, `music.js`, `ridge.js`, `floor.js` | the camera, the one axis, the music's levels, the ridge's heights, the floor's rows (pure) |
| `mood.js`, `look.js`, `embers.js`, `sky.js`, `motion.js`, `budget.js` | the ear, the look, the embers, the sky's events, the Motion levels, the frame budget (pure) |
| `scene-wire.js` | the loop: reads the analysis, rolls the floor, makes the mood and draws the frame |
| `draw/` | the canvases' drawing (`sky.js`, `road.js`, `heat.js`, `mood.js`, `events.js`, `tune.js`, `esc.js`, `palette.js`) and the markup of the HUD, the volume, the dash, the sheet's sections, the card (`hud.js`, `volume.js`, `dash.js`, `sheet.js`, `modes.js`, `bands.js`, `presets.js`, `slots.js`, `controls.js`, `facts.js`, `meters.js`, `gauges.js`, `card.js`) |
| `tune.js`, `tune-wire.js` | the tone control on the mountains as numbers, and the hands on it |
| `hud.js`, `hud-wire.js`, `dash-fade.js`, `rest.js` | the HUD's words, its fade, the dash's fade, the resting cursor |
| `dash-wire.js`, `sheet-wire.js`, `bands-wire.js`, `presets-wire.js`, `controls-wire.js`, `facts-wire.js`, `card-wire.js`, `gestures.js`, `lists.js`, `ask.js`, `fold.js`, `act.js`, `inert.js`, `place.js` | the hands: what a tap, a hold, a drag, a key, a list or a question does, and the log of what this page asked |
| `states.js`, `words.js` | how the page stands and the quiet line's words; the fixed words |
| `live.js` | the store's events on the page: which sections are drawn again, which are held |
| `debug.js`, `debug-wire.js`, `stats.js` | the readout and the switches, the draws' times |
| `driver.js`, `preview.js`, `style.css` | the test driver, the picker's picture, the tokens and every section |

Pure modules take numbers and return numbers; drawing modules return markup or draw on a context;
wiring modules are the only ones that listen. Night drive reaches a device only through
`core.actions`, and every action for a device that is not the shown one carries that device's URL.
Safety stays where it was: the volume rule and the cap, confirmations, write pacing, gone-device
handling, sign-in and guest limits are the daemon's and the core's.

Motion, the frame budget, the readout, the checks, the known limits and what Night drive found
missing in the core are on [the second page](web-nightdrive-checks.md).

[Back to web themes](web-themes.md).
