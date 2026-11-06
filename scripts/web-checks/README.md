<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The web UI's checks

Node harnesses (`node/*.mjs`) and browser checks (`*.py`, in headless Chromium and Firefox) for the
web UI's effects layer: the effects menu and its levels, the Glow level, the smoke behind the page
and the notes that form in it, the volume knob, the Tone pane and the Now playing card. The smoke
is read with `readPixels` and float readbacks, as numbers; no check takes a picture of it. Each
check runs against a throwaway fake daemon (never the installed binaries, never ports 8737 or 8738)
and keeps its whole output in `target/web-checks/out/`. The drivers' ports (geckodriver's
WebDriver and BiDi ports, Chromium's debugging port) are picked free, so two runs can share a
machine.

[COVERAGE.md](COVERAGE.md) maps each rule to the check that proves it.
[docs/design/testing.md](../../docs/design/testing.md) says where these checks sit among svolder's
other tests, and [docs/design/themes-and-effects.md](../../docs/design/themes-and-effects.md)
describes the layer they cover.

## Run it

```sh
make web-checks-quick   # about 38 minutes at the default 6 jobs (much longer with WEB_CHECKS_JOBS=1): the page changed, the smoke did not
make web-checks         # over an hour at 6 jobs: everything, before merging any change to the web page
```

One check on its own, from the repository root (build the daemon first with `cargo build -p
svolderd`; the browser checks start `target/debug/svolderd`):

```sh
node scripts/web-checks/node/director.mjs     # a node harness: no browser, a few seconds
python3 scripts/web-checks/haze.py chromium   # a browser check: chromium or firefox
python3 scripts/web-checks/notes.py firefox look   # some take a section name; see the file's header
```

Every check prints one line per assertion, `ok` or `FAIL` with the numbers it read, and exits 1 on
any failure. `run-all.sh` is the entry point of both make targets: it runs every check, prints one
line per check with its time and last output line, then a total.

## Requirements

`node`, `python3` with `python3-websockets`, `python3-numpy` and `python3-pil` (apt packages),
`chromium`, `firefox` and `geckodriver` (the browsers may be snaps), and a Rust toolchain to build
`svolderd`. `run-all.sh` names anything missing and stops before it runs a check.

## Which gate runs what

- `make web-checks`: every node harness, every browser check in both browsers, the gate refusal;
  before a merge, and whenever a smoke file or one of the smoke's own checks changed.
- `make web-checks-quick`: the node harnesses, the checks that leave the smoke alone (effects menu
  and layout, the Tone message, the ring click, the curve's drag, Now playing, the visualizer,
  devices, a Loki Max's presets, the Firefox preload) and `smoke_quick.py` per browser; after a
  change to the page, the styles or the templates that does not touch the smoke.

Both gates run their browser checks `WEB_CHECKS_JOBS` at a time (default 6; `1` is one by one, the
serial lane's checks after the others), longest first (the `weight` table in `run-all.sh`; refresh it from a run's times when the
checks change). Every check has its own daemon home, free ports, browser profile and output files, so
any two can run side by side. A check that fails beside others is run once more alone: passing
alone is `flaky` (counted as passed, named in the total line, both outputs kept in
`target/web-checks/out/NAME.out` and `NAME.again.out`), failing again is `FAIL`. On both gates
`smoke_core.py`, `smoke_page.py`, `haze.py`, `effects_cycle.py`, `scroll.py` (the smoke's velocity
after wheel steps a set time apart), `smoke_quick.py` and
`nightdrive_timing.py` (Night drive's draw cost and its draws and floor rows a second) run one at a
time after the parallel part: they read the page's own frame pace and calm rate and want the machine
to themselves. A TERM or Ctrl-C stops
every running check, and their browsers and daemons with them.

The quick gate refuses, with `web-checks: a smoke file changed (NAME): run make web-checks` and
exit 1, when a file of the smoke or one of its own checks changed against `main`
(`WEB_CHECKS_BASE` names another base) in the commits, the working tree or the untracked files. The
list is `smoke_files` in `run-all.sh`: the smoke's scripts under `bins/svolderd/web/themes/ember/`,
`effects.js` (it starts and stops the smoke, and the quick gate does not cycle effects), the
smoke's checks and tools here, `COVERAGE.md` and `node/`. `gate_refusal.sh` proves the refusal on a throwaway
repository. Outside a git checkout the gate runs and says it could not tell.

## What is here

- `snippets.py`: loads every `mutate_*.py` table and fails when an entry's `old` text does not occur
  exactly once in the file it names; needs only python3 (`make web-snippets`, part of `make check`).
- `node/model.mjs`: the smoke's model (`smoke-model.js`): the rhythm, the sources' strength, the
  palette, the haze, the scroll push, bad input.
- `node/notes.mjs`: the notes' numbers: the agitation measure, the gate, the settle watch, volume
  bursts, placement, the three glyphs.
- `node/sources.mjs`: what stirs the smoke: the pointer's force and dye, the volume step's puff, the
  meters' emission.
- `node/director.mjs`: the director (`smoke-director.js`) on a fake clock: every rule above working
  together, away and lost, bad input.
- `node/effects.mjs`: the effects module: stored level, which level the page picks, the background
  alpha.
- `node/tone-curve.mjs`: the page's copy of the tone control's curve maths (`tone-curve.js`) against
  `node/fixtures/tone-curve.json`, the core's own numbers that a Rust test writes and keeps current.
- `node/tone-drag.mjs`: the curve's gestures (`tone-drag.js`) over fake writes that answer when the
  test says: grab reach and ties, the axis hold, the snap past the plot's edges, the right drag's
  and the pinch's Q, the wheel's steps (a mouse's notch is one whatever its pixels), one write in
  flight per parameter (a value already sent is not written again), `settled`, the release, Esc, a
  lost stream, a refusal, the mode toast, the label's place, a coordinate map of the drawing's own
  that writes the same steps, and the sticky 0 dB turned off by a hold of 0.
- `node/ember-parts.mjs`: the theme Ember's drawing (`draw/parts.js`, `draw/meter-arch.js`,
  `draw/dial.js`, `draw/curve.js`, `draw/screen.js`, `draw/shell.js`, `order.js`) of the link, card,
  controls, power, meters, volume, tone, bands, presets, device presets, facts and playing parts,
  the whole screen and the page's shell from the page's JSON model, against
  `node/fixtures/ember-parts.json`, the golden copy of the DOM the server's templates drew for 45
  cases (frozen: no test writes it). The same harness holds the claims the Rust tests
  keep of the model to the markup Ember draws (the cases named like those tests). `node/html-same.mjs` is the comparison (tags read in order; the attributes htmx
  reads dropped, attribute order and entities ignored, whitespace runs one space, JSON attributes
  and JSON data blocks as JSON, drawn numbers within 0.011); the harness checks that comparison
  first.
- `node/keep.mjs`: the swap hold of the tone part while a point is dragged (`keep.js`), with and
  without `Element.moveBefore`, two overlapping holds, and the part drawn again once they go, once,
  from the newest copy the model holds (not from the swap that was held).
- `node/level-write.mjs`: a slider's writes (`level-write.js`): one in flight, the newest waiting,
  each for the device shown when it was asked, so a value that waits behind the write in flight is
  dropped when another device is shown or the slider has left, and never goes to the other device.
- `node/toast.mjs`: the popups (`core/toast.js`): the same words again leave the one popup standing
  and start its time over; a button, other words or a popup on its way out make a popup of their own.
- `node/frame-keys.mjs`: the frame's reserved key (`core/frame.js`): `T` is the frame's unless a
  field it would type into has the focus or Ctrl, Alt or Meta is held; and the two helpers Ember's keys
  ask the menu (`core/menu.js`: whose a key is, whether a node is inside it).
- `node/ember-order.mjs`: the order of Ember's sections (`order.js`): the default, a kept order that
  drops what the layout lacks and gives a new section its default zone, the arrow keys' moves, a
  section placed by index, and one order per layout.
- `node/contract.mjs`: what every theme must present (`core/contract.js`), read from the fixture's
  pages (every device kind, a guest's too): every control of the model is of a kind the core knows
  (`choice`, `toggle`, `slider`, `readout` and the volume, power, tone, presets and device presets
  objects), `targetsOf` and `factsOf` list what the model offers, each target maps to one action
  request with today's path and body (a readout none, a slot's save and recall one each), a
  followed device's targets carry its name, and a guest's send nothing and say to sign in.
- `node/switch.mjs`: the theme on show and the switch (`core/switch.js`), over a fake head: whichever
  theme ends up mounted, the fallback included, has its sheet, metas and scripts in place first
  (metas first, scripts in order, none added twice); a sheet or script that fails or never answers
  leaves the theme on show and releases the switch; a theme and a fallback that both fail leave a
  popup that stays; a pick before the first mount and an empty `?theme=` do nothing.
- `node/facts.mjs`: the Device fold-out's page rules (`words.js`, `facts.js`): the ages as the daemon words them
  (90 s, 90 min and 36 h edges), the signal's words, the Identify toast with the daemon's gap, and
  when a read of the part is overtaken by a newer swap.
- `node/playing.mjs`: the Now playing card's arithmetic (`clock.js`, `playing.js`): the elapsed time from the
  player's position, rate and read time (none while paused, clamped to the length), the daemon-clock
  offset it is worked out against, the bar's fraction and the `m:ss` and `h:mm:ss` words.
- `node/core-store.mjs`, `core-stream.mjs`, `core-actions.mjs`: the core's model store (a model
  replaces everything and says so once, a part event changes one device, a reading folds in, a
  write's answer applies its parts), its one event stream (the address, the reopen after 2 s, the
  session question, the rest and the wait on waking, a device switched while it rests, a device no
  longer followed) and its actions (each posts today's request; a guest's or a read-only page's
  send nothing; a refusal is toasted and the model read again; the volume's cap and the raise that
  waits for a live stream; a device switch and its address), over fake streams, clocks and posts.
- `node/core-title.mjs`: the tab's title the core keeps for every theme (the device, the status words,
  the parts a theme adds).
- `node/core-hold.mjs`: the held `−` and `+` the core gives every theme (a step at the press, the
  repeat after 380 ms every 70, the ways it ends, a redraw that replaces the button, the 10 s cap),
  over a fake clock and fake events.
- `node/core-prefs.mjs`, `core-registry.mjs`, `core-climb.mjs`, `core-loop.mjs`: the core's
  preferences and light or dark (storage that refuses, the choice and its cookie, the system's
  setting followed until overridden), the theme registry (a theme that throws or cannot run falls
  back to Ember with a toast), the climb's popup (and a climb a touch ended) and the tone mode's
  note, and the canvas loop (on show only, `reduced: 'still'` or `'theme'`, a pace waited on a
  timer with `dt` held to twice the pace) and the analysis frames shared by their holders.
- `node/viz.mjs`: the visualizer's pure rules (`viz-colour.js`, `core/viz-model.js`): the base colour
  scale and the stereo tint at the design's values, a frame decoded from the wire shape, the
  smoothing, the 2 s max hold, which frame is drawn (the newest waiting, a late one alone still
  drawn), the clock offset from `hello`, the delay's median over 2 s, the beats scheduled ahead
  from the tempo and phase (on time behind a steady delay, never twice, none in silence) and the
  band and pixel maps.
- `smoke_core.py`: the level shown, the cloud and the haze, loss and refusal, switches, hidden and
  away, sizing.
- `smoke_page.py`: how the page drives the smoke: a lost stream, when WebGL is probed, the menu, the
  meters.
- `smoke_quick.py`: the quick gate's one short check: it starts, stays finite, rests at the calm
  rate.
- `sources.py`: what stirs the smoke on a real page: the pointer, the dial's drag, the volume puff,
  the meters.
- `notes.py`: the notes in a real browser: the atlas, one note end to end, the look, the page's
  settle wiring.
- `haze.py`: the haze across the page: its density, the calm rate, steadiness, the palette, the pace
  of a stroke.
- `scroll.py`: scrolling stirs the smoke, the dye a scroll moves out of view returns, the resting
  source rises not.
- `sweep.py`: four pages (DAC, amp, Loki Max, lost link) at three sizes and every level load with no
  error, no warning, no horizontal scroll.
- `effects_cycle.py`: the registry and the engine across level switches and a lost context.
- `effects_menu.py`, `effects_layout.py`, `effects_devices.py`: the frame's menu with Ember's
  levels, the picker (its sheet inside a window held sideways, its grid scrolling to the last theme under a
  finger), the key `T` and the button's fade, a theme that throws, a second theme and
  back, the Device and Sign in rows; the Glow layer's place and edges; a device switch with the
  layer.
- `contract.py`: the theme contract, one short pass for each theme the page lists, driven only by
  the theme's test driver (`theme.driver`): volume up and down, mute, a band change, a preset, a
  device out of reach and back (`Daemon.drop()` then `restart()`), a write the daemon refuses and
  the words shown, a page that rests and wakes, the link state in words beside the device's name (the driver's
  `linkWords()`), the menu button and `T` also after a fade, `?theme=`
  kept by its cookie, and the same as a guest (no request, "Sign in to change this"). It pins the
  members of `core` and holds the menu button shown, at least 24 px, on top and inside the window
  on every kind of device at 1280 and 390 px. `WEB_CHECK_THEMES` (a comma list, as
  `WEB_CHECK_THEMES=paper`) limits it and `completeness.py` to the named themes while one is being
  worked on; unset, both run every listed theme, and a name the page does not list fails the pass.
- `frame_button.py`: the frame's menu button is the same in every theme the registry lists (Night drive
  too): 44 by 44 px, 8 px from the top and the right, at 1280x800, 768x1024, 412x915 and 360x780, for a
  signed-in viewer and a guest, and nothing the theme draws stands under it, with the scrolled case really
  scrolling where a page can.
- `space_key.py`: the frame's Space key in all four themes: on the page body one press of the player's button
  is posted (relative `api/v1/host/press`) and the page does not scroll; on a focused button that button is
  activated and nothing is posted; an open menu owns the key; in a text field it types; with no player on the page
  nothing is posted.
- `ember_link.py`: Ember's connection state as a dot after the shown device's name (an image named by
  the state's words, not a tab stop): every state's word (`not connected`, `connecting…`, connected),
  its place after a switch of two devices, a long name, and the muted volume reading `−∞ dB`.
- `completeness.py`: every target and fact of `core/contract.js` on a Gungnir 2 with a player, an
  amp with a signal, a five-meter amp, a mono amp and a Loki Max (one daemon,
  `Daemon(fleet=[(profile, name, udid)])`), in Chromium, for each theme the page lists: reached,
  operated, and the change read back from the model route; every fact shown with the model's words (the
  track's cover is one: shown by every theme as the picture asked for by its version).
- `core_rows.py`: the core's rows Ember does not use, from a test page: a second device followed
  over one stream, a hop added, the menu's fade, a band moved by `core.drag` from plain
  coordinates, light and dark following the system (Chromium), a canvas loop that pauses while the
  page is hidden, a theme unmounted and mounted again, the tab's title on Ember, Paper, Signal and Night drive (connected,
  reconnecting, a device switch), and Ember mounted while the stream is lost.
- `node/paper-tokens.mjs`, `node/paper-words.mjs`, `node/paper-live.mjs`, `node/paper-ruler.mjs`, `node/paper-draw.mjs`, `node/paper-notes.mjs`: the theme Paper's tokens
  (AA on its sheet in light and dark paper, the two dark blocks alike, every frame colour token set,
  the planned values), its words (the title's line, the footer's date, only characters its fonts
  carry) and its sheet and what keeps it current (`draw/sheet.js`, `live.js`, `act.js`, `place.js`:
  which sections a change draws, a held section waiting for the newest model, the log of what the
  page asked, marks placed through the style object) and its volume's ruler (the steps' places,
  the labels at each 10 dB read off the device's table, the ticks, the zones, the drawn section) and
  its tone (the points at the bands' markers, the dashed Flat curve, the heading's words, the
  steppers, no `style`, no underlined word on a guest's sheet) and its controls and presets (every
  control written by its kind, the presets' save, rename and delete words only when editable, the
  device's slots, save and built-in settings) and its handwritten notes (which facts, their words,
  the one red note and its order, the last change and whose it is, the arrow drawn as a stroke).
- `paper.py`: the theme Paper in a browser: mounted from the address and the picker, its title,
  footer and menu button over the title's slot, Auto following the system, the menu's Auto, Light
  and Dark with the choice kept, dark paper painted dark from the first byte after a switch to Ember
  and back and a reload, contrast in both papers, no sideways scroll at 390 px, the volume (the needle's place, a drag,
  the written words, the keys, the cap, a reading from elsewhere during a drag) and the tone (a
  point dragged, the band table's steppers, the arrows on a point, the mode and preset lists, the
  loudness strength, a change from elsewhere during a point drag) and the controls (a choice, a
  switch, the balance scale), the presets with their forms and questions (a slot save posts
  `confirm: true` only after `save`), the device's built-in settings, a refused form, an open list
  kept through a change from elsewhere and a touch on the phone graph, Now playing's transport,
  an amp's power and needles, the device's spec table, the card for a lost device and the page's
  states in Paper's words, a second device one tap away, and no sideways scroll at 390 px with long
  names, titles and presets (a DAC, an amp, a Loki Max), and the notes (their words, the EQ's note
  and the track's time moving, `last change` and `from this browser`, each note in the band's third
  or after its section on a phone, one red note circled, ringed on a phone).
- `node/signal-line.mjs`: the theme Signal's line (`themes/signal/line.js`): the hops in order
  (player, computer, the shown device, the hops declared after it in file order), a hop after another
  DAC left off, a linked hop live only when listed, the line starting at the device with no host, an
  amp no hop links standing alone after a stand-in ring, the computer's stretch changing the sound
  from the path's flag, the AutoEq headphones ring and what replaces it, and a choice that falls
  back to the shown device (also the player that quits while chosen) and the shown hop's trouble,
  and a linked amp shown on its own page with the DAC its row names before it.
- `node/signal-draw.mjs`: what Signal draws of a DAC's first screen (`themes/signal/draw/`): the
  volume with its unit, the curve with a point on each band, the input chips and the mode row (the
  Loudness chip's round `−` and `+`; the mode under the curve on a phone, heading the shelf's middle
  column on a desktop, the input and presets first in a phone's shelf), the Add device row of the
  menu, the Now playing bar with its progress, the shelf, a live amp's area (power, meters, Unlink), the player's and the computer's
  open areas (the computer read only), the card of a device in trouble and the broken line, what a guest's or a
  read-only viewer's controls owe (drawn inert), no `style` attribute and escaped words.
- `signal_theme.py`: the theme Signal in a browser: mounted from the address and the picker, the
  line's hops in order and evenly placed, its sizes (band 140 px, box 200 by 104, dots on y 60),
  rings for declared hops and dots for live ones, a click (even with the line drawn again between its
  press and its release) and the arrow, Home and End keys moving the choice and the box, the dots moving while the player plays and holding when it pauses, when
  the page is hidden and under reduced motion (the volume buttons in Ember's order, muted as `−∞ dB`, a held
  `−` or `+` repeating until it ends, a two-finger pinch on the curve setting a band's Q, the
  transport as svg icons, the page filling a window up to 1100 px and one centred block above it,
  the shelf in two columns, a held band or loudness stepper repeating across the shelf's redraws and
  stopping on a release, a blur or a hidden page, the mode's place at 1280 px and at 390 px, the
  tab's title), a hop that leaves and a device switch giving the
  choice back to the device, the line with no host and an amp standing alone, contrast on the
  ground; the DAC's first screen and the Now playing bar; the shelf below it (the band table with
  typed and stepped values, presets saved, renamed and deleted in place, the switches, scales and
  loudness, a Loki Max's slots asked before they are written, the facts folded and the fold kept
  across a reload, Identify) and the slim bar that pins when the line scrolls away; a linked amp as a
  live dot with its power, switches and meters, its writes going to its own URL, the DAC's volume
  written under its name while the amp is chosen, the device list opened by the DAC's name, a chosen
  amp that leaves the fleet, Unlink asked once and the amp left standing alone, and a guest's taps; the
  menu's Add device… row and the sheet that declares a hop (checked against `svolder hops list` on the same temp homes), a declared hop's
  Edit in place, Move earlier / later and Remove asked once, a hop removed elsewhere, and a guest's taps on them; the player
  chosen (its transport, the bottom bar hidden), the computer's read-only facts, a lost device's card with Try now and the
  seconds, a lost stream (`Reconnecting to svolder…`, faded, inert) and a refused write's toast; the
  phone (390 by 844, touch emulation): the strip with the chosen hop centred and boxed, a swipe or a tap
  choosing, the strip pinned when scrolled, 44 px targets, no sideways scroll with long hop names, the
  quiet lines not meeting, the menu button fixed against the window while the strip is pinned (and
  Paper and Ember placing it on the page, as before); a linked amp shown on its own page (the greyed way in, the DAC as a live
  hop, the thin stretch to the amp, an arrow onto the DAC boxing it in place with a Show button, a tap or Enter showing it); the middle layout (768 and 900 px:
  the curve and an amp's meters 300 px or wider, a point dragged; H6 whole from 1020 px).
  `contract.py` and `completeness.py` run Signal through its test driver (`themes/signal/driver.js`)
  like every listed theme.
- `signal_shots.py` (`make signal-shots`, Chromium): Signal's stills, a DAC at desktop width, with the
  linked amp chosen, with a declared hop chosen, with its link lost, the Add device sheet open, the linked amp shown on its own page, a tablet (768 px), and a phone
  at rest and scrolled with the strip pinned, into `target/web-shots/signal`; no check, only pictures.
- `nightdrive_shots.py` (`make nightdrive-shots`, Chromium): Night drive's stills on the fake capture's
  music, a desktop at rest, with a bubble held, with the sheet open, with the device out of reach and with no
  capture, and a phone upright and sideways, into `target/web-shots/nightdrive`; no check, only pictures.
- `node/nightdrive-scene.mjs`: the theme Night drive's scene maths (`themes/nightdrive/`): the camera
  (the nearest row at the bottom, the far rows wider than F6's), the one frequency axis and the 24
  columns the 240 bands go to, the music's levels (the ridge's unit, a column's mean, the bass and the
  highs, the row rate) and the floor's 22 rows, newest at the horizon, still when it does not roll.
- `node/nightdrive-sky.mjs`: the theme Night drive's sky events and motion: the rays after a held
  bass, the shooting star on a transient and the shower on intense highs (each band's smoothed level read by its place in the
  last minute's range with a leaky count: kick- and hat-shaped music with dynamics fires, a flat drone and near silence do not), their limits (rays once a minute,
  a star in 5 s, a shower in 40 s) at 30, 60 and 144 steps a second, nothing when off, the log of 20, the
  levels readout of the driver, and the Full, Calm and Still levels with their pace.
- `node/nightdrive-hud.mjs`: the theme Night drive's HUD: the top line (device, input and rate, the
  path's verdict, the core title's trouble words in its place), the EQ line, the volume's words, the
  outrun volume's SVG (no `style`, prefixed ids, the minus and infinity as paths) and the fade on a
  fake clock (any input wakes it, trouble holds it off, a volume from elsewhere shows for 5 s), and the
  dash's markup (the keys in order, chips or one list chip, the transport as SVG, inert for a guest,
  the track's words and progress, the cover between the input and the title and none when the track has none),
  and the cursor's rest over the scene on a fake clock (2 s, any move, not over the dash, not while a button is held).
- `node/nightdrive-tune.mjs`: the theme Night drive's tuning on the mountains: the map between pixels and
  frequency and gain, the bubbles at each band's own gain, the held curve and label, the morph on a fake clock
  (the 1.5 s grace) and when a page may be tuned at all (a guest, writes off, no bands); and Q from a diamond
  (the half-gain points of a band's bell against the core's response, the Q step a diamond names, the diamonds on the curve, the resting pointer,
  a diamond drag landing on the step it names).
- `node/nightdrive-draw.mjs`: the theme Night drive's neon sheet as markup: its sections in order (Mode, Input, the
  band table, the presets beside it), the Loudness chip with its round `−` and `+` and the strength under its name,
  a field and two steppers for every band value, every key drawn inert for a guest and for a page whose writes are
  off, and no `style` attribute, inline script or unescaped name anywhere; and the thumbnail's sun stripes thick
  enough to show at the menu's size.
- `node/nightdrive-ridge.mjs`: the theme Night drive's ridge as heights: no sawtooth in silence, a notch a quarter
  octave and one a twelfth wide still there after smoothing, a ripple between bands gone, one bass band a
  rounded hill, the slow swell's drift (halved in Calm, frozen in Still) and the swell giving way to the music, and
  the ridge drawn as curves.
- `node/nightdrive-floor.mjs`: the theme Night drive's floor drawn as a heat grid on a recording canvas: a quiet row
  flat, a loud cell lifted and hotter than a quiet one, a near crossing lifted more than the same far one, the strips
  drawn far first (ground, heat, lines), the rows rolling toward the viewer and a floor that does not roll drawn the same
  twice, a lifted near edge with floor still under it to the bottom of the scene, the heat ramp leaning warm for bass and cool for highs, the wet road thinning the ground, the mood dimming the
  lines and the heat, and the wave of a moment running down the floor.
- `node/nightdrive-mood.mjs`: the theme Night drive's mood: the ear's level against the last minute's loudest, the
  medium and long views, quiet music at a low volume reaching long near 1, a drop after a breakdown firing one moment
  (and a steady song none after the first, at most one in 15 s), the balance and silence holding it, the embers
  of a peak, the same at 30, 60 and 144 steps a second and after a gap; the look made of it at each Motion level
  (mood, wet road, haze, heat lean, cruise speed, flare and wave); and the sparks (hot near cells, the cap by the
  canvas's area, births by the clock).
- `node/nightdrive-budget.mjs`: the theme Night drive's pixel budget on a fake clock and fake draw times: the density and
  2.5 MP caps, the steps down of 0.75 on a slow mean to one backing px per CSS px, the 30 a second cap after it, and
  the way back after 10 s comfortably under budget (a pause between draws restarts the clock), the step down on too few
  draws a second while every frame is asked for (and the way back shut for a minute after it), a phone starting at the cap,
  and the wet road: dropped first, before the scale and the rate, absent from a phone's start, back last and not for 3 minutes.
- `node/nightdrive-debug.mjs`: the theme Night drive's readout and bisect switches (`nomood`, `nowet`, `noembers` and `nohaze`
  among them): what an address asks for, the draws' rate and 95th percentile from the draws' own record, and the box's words
  (the ear's values in its sky line).
- `node/nightdrive-glow.mjs`: the theme Night drive's glow on a recording canvas: nothing in the scene is blurred (no
  `shadowBlur`), the sun's glow is a gradient round it, the ridge's edge and the bubbles glow with wider, fainter strokes
  under the line, and none of it is drawn when the glow is off; the wet road's slices sway as one wave (adjacent slices under 1 px apart at 1280 wide, all leaning the same way).
- `nightdrive_timing.py`: the theme Night drive's timing cases in a browser, which read the CPU or the wall clock and so run in the
  serial lane: no frame while the page is hidden, reduced motion starting on Calm with Full pickable, a paused player drawing at its
  level's own pace, a desktop starting with the wet road allowed by the budget, what a draw costs (printed; with every cell lit the median of five readings held to 4.5 ms in Chromium, the draws
  checked to have gone on between the readings), the floor's rows a second (loud, silent, mood off) and the budget's rate rule (a page
  that draws fast and seldom steps down, the same page left alone keeps its density and rate), a phone's start at the 30 a second cap
  and the readout's draws a second against the loop's own count.
- `nightdrive_theme.py`: the theme Night drive in a browser: mounted from the address and the picker,
  the scene's sampled pixels (the sky's top, the sun, a lit floor cell after 2 s, the ridge), the floor
  rolling with the host's synthetic capture, the analysis held only while the scene is on show (and
  let go on a hidden page and on a switch to another theme), the menu button in
  its 44 px slot and contrast on the scene, the Motion rows (kept per browser, Calm stopping the road,
  Still at about a frame a second) and the HUD: its words against the model, the fade after
  2 s and the wake on a move, a Tab, the volume shown for 5 s, trouble while faded, the menu button
  outside the fade, the two fonts and the words' places against the sun at 1280x800 and 1280x1600; and
the dash: the keys against the model, `+` `−` and MUTE, a held `+` (timed: the first repeat about 380 ms on,
then every 70 ms, ended by a release, a cancel, a blur, a hidden page and a lost stream), the cap held by
`[web] max_volume_db`, the input's chips and list, the transport, a guest's taps posting nothing, and
the dash outside the fade; and the tuning on the mountains: hovering turns them into the curve and
leaving lets it go, a bubble drag writes the steps nearest the drop (Flat becomes EQ), the curve stays 1.5 s
after the drag, a guest's tap says why, the value above a held bubble, a reading from elsewhere waiting for
the drag, a switch of device in the middle of a drag writing nothing to the new one, and a bubble grabbed
where it is drawn at a density of 2; the phone's top at 412x915 and 360x780 (no two of the device line, the dB, the EQ
readout and the menu button meet, the curve's top below the dB), the cursor hiding at rest and the cover as tall as the
track; and Q: a hovered bubble's pink stretch of the curve with its diamonds on it, the range of the map, a resting pointer letting the curve go, a diamond
dragged to a step's half point writing that step (mouse and touch), the wheel and the two-finger pinch on the
nearest band, a first touch only showing the curve, the curve staying 1.5 s after a notch or lifted fingers, and
a diamond grabbed where it is drawn at a density of 2; the neon sheet is `nightdrive_sheet.py`: opened by `DEVICE ▴` and by the menu's
Device… row with the focus inside, closed by the cross, Escape and a tap on the scene with the focus back, the scene
and the dash dimmed and out of reach while the scene goes on drawing, the band table stepped and typed (a held
stepper timed like the volume's and ended by a release, a cancel, a blur, a hidden page and a lost stream, let go at
10 s), the mode and the Loudness steppers, the presets saved, renamed and deleted in place, the sheet held while a
value is typed or a list is open, the phone's two layouts, a switch of device with the sheet open writing nothing,
and a guest's taps asked of the core and posting nothing, the device's slots (a save asks first), settings, controls (a toggle, a scale, the amp's power), facts (folded, the fold kept) and Identify, and an amp shown alone with its gauges. Both share `nightdrive_base.py` (the page, the request log, the bands reset); it is not run by itself.
- `nightdrive_phone.py`: the theme Night drive on a phone, in both browsers: held upright the scene is the top 52 % and the dash a
  panel under it (the volume large, `−` `MUTE` `+` in that order, the track, the transport and `DEVICE ▴`), held sideways the
  scene is the window with a slim dash, the sheet from the bottom or from the right and kept across a turn (the same control
  focused), every button and field 44 px (a 44 px box about the middle lands on it; the Loudness round keys by their hit area), the menu
  button fixed at 8 px, the mountains tuned from the scene's top half (touch in Chromium, the pointer in Firefox), a 40-character
  device name, a long track and a long preset with no sideways scroll at 390, 844 and 1100 px, and a tall window (1280x1600, 2560x1440) whole.
- `node/signal-controls.mjs`: the same on Signal's controls (`controls-wire.js`): one write in flight per scale of a device, a step after a device switch goes to the new device only.
- `node/nightdrive-controls.mjs`: the theme Night drive's controls on a stand-in page: a scale keeps one write in flight with the newest waiting, and a step asked after a device switch is written to the new device, never to the one shown before.
- `node/nightdrive-states.mjs` and `nightdrive_states.py`: the theme Night drive's states: the page's mood in each (`ok`,
  `unreachable`, `lost`, `wait`, `nocap`), the card of a device out of reach with Try now and the seconds, a fault that leaves the controls
  live, the quiet line's words; in a browser a dropped device (the road stops, the sun greys, the mountains flatten, the card, the dash out of
  reach, Try now posting `connect`), a lost stream, a page reading again and resting, a host with no capture (`SVOLDER_FAKE_CAPTURE=off`),
  a guest, a refused write and a card arriving while the top is faded.
- `paper_shots.py` (`make paper-shots`, Chromium): Paper's stills, a DAC and an amp at desktop and
  phone size in light and dark paper, into `target/web-shots/paper`; no check, only pictures.
- `arrange.py`: Ember's Arrange mode in one browser (a fake DAC with a player, a DAC and an amp
  in one daemon, a DAC that goes away): the sections moved by the arrow keys and by a drag of the
  handle, Done, Reset and Escape, the order per layout kept across a reload and a screen swap, the
  page's own drags inert meanwhile, a swapped part landing in its moved section, `T`, a theme that
  unmounts mid-arrange, and a phone's order. `ARRANGE_SHOTS=<dir>` keeps four pictures.
- `tone_note.py`: the Tone pane's Flat and Loudness notes, and that no mode moves the layout.
- `knob_click.py`: a click on the knob's ring jumps there; a drag, the centre and the gaps do not; holding the `+` or `-` button repeats the step and a release ends it.
- `device_presets.py`: a Loki Max's presets part (`Daemon(profile='lokimax')`): three slots and five
  built-in settings, a built-in's click and its lamp (also lit by another client through the
  stream), a slot's recall, the save question opening in place, Keep closing it with no request and
  Save posting `confirm: true`, and what was saved coming back on a recall.
- `playing.py`: the Now playing card on the fake host (`Daemon(host='fake')`): the desktop band and
  the phone's line that a tap opens, the track, cover, path and counting time, the transport posting
  `api/v1/host/press`, a guest and a read-only page without buttons, the card's box unchanged when
  the cover goes (`SVOLDER_FAKE_COVER_GONE_FILE`, written by the check once the card shows the cover) and when the player goes
  (`SVOLDER_FAKE_PLAYER_GONE_FILE`), a cover address the daemon no longer holds (a real 404: the
  picture is hidden and its box stays), an open phone card whose player quits (it folds to the quiet
  line), the folded line at 341, 426 and 784 px (a round play, a bare next, the arrow in the corner,
  a cut title looping slowly, its copy a gap behind it), a changed path on the fake changed route
  (`SVOLDER_FAKE_ROUTE=changed`: every chip sits inside the band's fixed box at 820, 1100 and 1280
  px, the verdict first), an amp page, a daemon with no host.
- `reduced_motion.py`: with reduced motion emulated, the small motions all run (the glow turns and
  follows the music, the dot pulses, the knob eases, a held peak falls by steps, the tone curve
  eases and a changed value flashes) and only the first default level (Glow) follows the setting;
  Off stills the page.
- `visualizer.py`: the visualizer card on the fake host: the canvas draws the synthetic music (a
  fresh waterfall starts as silence), a tap and `v` cycle the three looks with a chip, the graph
  toggle, the choice kept across a reload, a hidden page and a card out of view letting their frames
  stream go after 2 s, and a hidden page its device stream too, with no sign of a fault and a change
  made meanwhile on screen within 2 s of its being shown (the page's visibility is a switch the
  check holds, every EventSource noted), no capture (`SVOLDER_FAKE_CAPTURE=off`) keeping the box,
  the placement at 820, 1280 and 390 px with no box moving when it starts, the Device fold-out's
  delay row, an amp page's sink label, a guest and reduced motion, and the effects hearing the
  frames (the glow over the volume's level, the smoke's beat puffs). `VIZ_SHOTS=<dir>` keeps stills
  for looking.
- `tone_drag.py`: the Response graph under the mouse: a left drag, a drag straight across, a right
  drag for Q, the wheel (a burst that keeps its band as the pointer drifts), a guest; in Chromium
  also the swap hold, a device that goes away, and a drag let go of with its writes held (a new
  press, the wheel, a burst's base).
- `tone_touch.py`: the Response graph under a finger and a pinch, in Chromium under touch emulation
  (headless Firefox does not pan on touch): a drag from a point, a scroll away from every point, a
  touch between two points, a pinch for Q with a band picked and without, a second finger on a held
  point, a touchcancel mid-drag, a third finger in a pinch.
- `touch.py`: a finger stirs the smoke under touch emulation, and the page still scrolls.
- `firefox_preload.py`: Firefox's error collector sees an error thrown by a page's first script.
- `gate_refusal.sh`: the quick gate's refusal, on a throwaway repository.
- `common.py`, `smoke_probe.py`: the shared driver (fake daemon, Chromium over CDP, Firefox over
  geckodriver) and the in-page probe; not checks. A `Daemon` takes `fleet=[(profile, name, udid)]`
  (more fake devices, each with its `[[device]]` block), reads its model (`model(name)`), writes
  through the web API (`post`) and can take its first device off the air on the check's own cue
  (`drop()`, through `SVOLDER_FAKE_GONE_FILE`: no clock, so a loaded machine cannot reorder it).

## The approved look

The smoke's numbers live in `TUNING` (`bins/svolderd/web/themes/ember/smoke-model.js`). The numbers
the node scenarios are written for are copied once, in `node/look-values.mjs`. To retune on purpose, change
a number in `TUNING` and in that file together. While they differ, `node/model.mjs` fails its test
"the approved look" and names the numbers, and no other node check fails for it: `run-all.sh`
makes the tests written for the old numbers skip themselves (`LOOK_RETUNED_SKIPS=1`). Once the two
agree, those tests run against the new numbers and say which rule the new look breaks. The browser
checks that depend on a look number (`notes.py`: the note cooldown and the rolls around each odds;
`smoke_quick.py`, `effects_cycle.py` and `haze.py`: the calm rate) read it from the page's `TUNING`
and derive their brackets from it, so a retune never fails them for the number alone.

## Mutation tests

A check that cannot fail proves nothing. The mutation runners break the code in a scratch copy and
the checks must fail. Each runner holds a list of small edits (a guard removed, a number changed, a
sign flipped); for each it copies the page's files, applies the edit to the copy, points the check
at the copy and expects a failure. A mutant that passes is reported as `SURVIVED`: the check does
not cover that edit. An edit whose anchor text no longer exists in the source stops the runner with
`ANCHOR MISSING`: the list needs updating to the code.

They are slow, so they run by hand, one at a time (a second one makes the timing bounds flake),
and are optional: the gates do not run them.

- node: `bash scripts/web-checks/node/mutate_director.sh`, `mutate_model.sh`, `mutate_notes.sh`,
  `mutate_sources.sh`, `mutate_paper.sh` (thirteen mutants of Paper's ruler, notes, changes,
  log and words, run against a copy of the web dir by `node/paper-*.mjs`) and `mutate_signal.sh`
  (thirteen mutants of Signal's line, fold, computer hop, strip, linked amp and faults, volume and transport icons, the mode's place, the Loudness chip and the menu row, run by `node/signal-*.mjs`)
  and `mutate_nightdrive.sh` (twenty-two mutants (a minute's fade and a ridge smoothed over 0.6 of a band among them, the wet road's slices swaying each by their own phase among them) of Night drive's camera, axis, levels, tempo, sky, ear, look, budget, floor drawing, tuning, words and dash, run by `node/nightdrive-*.mjs`);
- browser: `python3 scripts/web-checks/mutate_smoke.py [name ...]`, and the same for
  `mutate_notes.py`, `mutate_haze.py`, `mutate_scroll.py`, `mutate_sources.py`,
  `mutate_effects.py`, `mutate_webui.py`, `mutate_registry.py`, `mutate_signal.py` (Signal's page:
  the amp's write without the amp's url, the dots ignoring reduced motion, the slim bar never
  pinning, a held plus that never repeats, a blur that does not stop a hold, a pinch the wrong way, a tall window that stretches, a shelf of three columns, the Add device row opening the sheet for a guest (the core refuses a guest's post, so the sheet
  is what can be mutated), the strip not centring, each killed only by the check it names; it drives `signal_theme.py`, fifteen full runs, so a
  complete `mutate_signal.py` run takes about 75 minutes)
  `mutate_nightdrive.py` (Night drive's page, Chromium, twenty-nine mutants: a sky drawn twelve times in every draw, a loop each mount starts and nothing ends, a long press that presses on its release, a first tap on a faded dash let through, a wet-road copy made a tenth of a second apart, an amp's meters heard as a part, the fade not woken by focus, trouble fading, the analysis kept while hidden, reduced motion starting on Full, Still at the screen's rate, a dash hold that never repeats, a diamond drag that writes nothing, a pinch on band 1, hit tests in canvas pixels, a switch that lets a drag write, a guest's morph, a first touch that grabs, a question that sends early or on Keep, a band field or a list chip that writes when it should not, the core letting a guest's call go out, the sheet's focus not given back, its columns swapped, the menu button following the page on a phone, Space playing under the menu, a 30 px picker close key; each run names the cases of `nightdrive_theme.py`, `nightdrive_sheet.py`, `nightdrive_phone.py`, `nightdrive_states.py`, `space_key.py` or `effects_menu.py` it needs, runs them unmutated first and counts a kill only when a check the mutant names fails)
  and `mutate_paper.py` (Paper's page:
  `place` setting nothing, Auto ignoring the system, a held section drawn anyway, the log empty, a
  meter's needle at its class-A share; it drives `paper.py`) (with no name they run every
  mutant; each file's header says which check it drives).

A worked example from `node/mutate_model.sh`: the line `Math.exp((-4 * dt) / secs)|Math.exp((-2 *
dt) / secs)` halves the speed of the easing in the copy; `model.mjs` must fail its test "ease:
converges, never overshoots, monotone". A line that survives means a rule without a test.

## Tuning aids

Not checks; run by hand when retuning the look.

- `tune.py key=val ...` serves a copy of the page with `TUNING` values changed and prints the
  resting cloud's numbers at several levels.
- `notes_tune.py [chromium|firefox]` prints the stamp's look in the canvas's own pixels over
  several volume steps.
- `glow_level.py` prints the Glow level's densest pixel as an equivalent alpha, to tune the smoke's
  densest cloud against.
- `soak.py chromium [minutes] [out.json]` runs the smoke untouched for 30 minutes by default and
  compares each five-minute sample with the same minute of `soak-baseline.json`: the frame rate
  within 1 frame a second and the mean alpha within 15 % must hold (exit 1 on a miss or a page
  error); the largest velocity within 25 % is only reported, as a noisy maximum. It is in neither
  gate; run it before a merge that touches a smoke file. It holds the long-run behaviour (haze
  density, frame rate, fluid size) that the gate's 20 s and 40 s reads do not.
- `smoke-baseline.json` is the recorded pace of a stroke that `haze.py` compares against: a stroke
  must last longer and travel further than in that recording. It holds one entry per browser.
  `SMOKE_WEB=<copy of the web dir whose pace is the reference> python3 haze.py chromium --baseline`
  (and `firefox`) records it again; do that only to move the reference on purpose.
  `soak-baseline.json` is the output file of a `soak.py` run (its fourth argument) kept as the
  reference.
