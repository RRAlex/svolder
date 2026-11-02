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
make web-checks-quick   # about 5 minutes: the page changed, the smoke did not
make web-checks         # about 41 minutes: everything, before merging any change to the web page
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
- `node/core-prefs.mjs`, `core-registry.mjs`, `core-climb.mjs`, `core-loop.mjs`: the core's
  preferences and light or dark (storage that refuses, the choice and its cookie, the system's
  setting followed until overridden), the theme registry (a theme that throws or cannot run falls
  back to Ember with a toast), the climb's popup (and a climb a touch ended) and the tone mode's
  note, and the canvas loop and
  the analysis frames shared by their holders.
- `node/viz.mjs`: the visualizer's pure rules (`viz-colour.js`, `viz-model.js`): the base colour
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
  levels, the picker, the key `T` and the button's fade, a theme that throws, a second theme and
  back, the Device and Sign in rows; the Glow layer's place and edges; a device switch with the
  layer.
- `contract.py`: the theme contract, one short pass for each theme the page lists, driven only by
  the theme's test driver (`theme.driver`): volume up and down, mute, a band change, a preset, a
  device out of reach and back (`Daemon.drop()` then `restart()`), a write the daemon refuses and
  the words shown, a page that rests and wakes, the menu button and `T` also after a fade, `?theme=`
  kept by its cookie, and the same as a guest (no request, "Sign in to change this"). It pins the
  members of `core` and holds the menu button shown, at least 24 px, on top and inside the window
  on every kind of device at 1280 and 390 px. `WEB_CHECK_THEMES` (a comma list, as
  `WEB_CHECK_THEMES=paper`) limits it and `completeness.py` to the named themes while one is being
  worked on; unset, both run every listed theme, and a name the page does not list fails the pass.
- `completeness.py`: every target and fact of `core/contract.js` on a Gungnir 2 with a player, an
  amp with a signal, a five-meter amp, a mono amp and a Loki Max (one daemon,
  `Daemon(fleet=[(profile, name, udid)])`), in Chromium, for each theme the page lists: reached,
  operated, and the change read back from the model route; every fact shown with the model's words.
- `core_rows.py`: the core's rows Ember does not use, from a test page: a second device followed
  over one stream, a hop added, the menu's fade, a band moved by `core.drag` from plain
  coordinates, light and dark following the system (Chromium), a canvas loop that pauses while the
  page is hidden, a theme unmounted and mounted again, and Ember mounted while the stream is lost.
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
- `paper_shots.py` (`make paper-shots`, Chromium): Paper's stills, a DAC and an amp at desktop and
  phone size in light and dark paper, into `target/web-shots/paper`; no check, only pictures.
- `arrange.py`: Ember's Arrange mode in one browser (a fake DAC with a player, a DAC and an amp
  in one daemon, a DAC that goes away): the sections moved by the arrow keys and by a drag of the
  handle, Done, Reset and Escape, the order per layout kept across a reload and a screen swap, the
  page's own drags inert meanwhile, a swapped part landing in its moved section, `T`, a theme that
  unmounts mid-arrange, and a phone's order. `ARRANGE_SHOTS=<dir>` keeps four pictures.
- `tone_note.py`: the Tone pane's Flat and Loudness notes, and that no mode moves the layout.
- `knob_click.py`: a click on the knob's ring jumps there; a drag, the centre and the gaps do not.
- `device_presets.py`: a Loki Max's presets part (`Daemon(profile='lokimax')`): three slots and five
  built-in settings, a built-in's click and its lamp (also lit by another client through the
  stream), a slot's recall, the save question opening in place, Keep closing it with no request and
  Save posting `confirm: true`, and what was saved coming back on a recall.
- `playing.py`: the Now playing card on the fake host (`Daemon(host='fake')`): the desktop band and
  the phone's line that a tap opens, the track, cover, path and counting time, the transport posting
  `api/v1/host/press`, a guest and a read-only page without buttons, the card's box unchanged when
  the cover goes (`SVOLDER_FAKE_COVER_GONE_AFTER_MS`) and when the player goes
  (`SVOLDER_FAKE_PLAYER_GONE_AFTER_MS`), a cover address the daemon no longer holds (a real 404: the
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
  `mutate_sources.sh` and `mutate_paper.sh` (thirteen mutants of Paper's ruler, notes, changes,
  log and words, run against a copy of the web dir by `node/paper-*.mjs`);
- browser: `python3 scripts/web-checks/mutate_smoke.py [name ...]`, and the same for
  `mutate_notes.py`, `mutate_haze.py`, `mutate_scroll.py`, `mutate_sources.py`,
  `mutate_effects.py`, `mutate_webui.py`, `mutate_registry.py` and `mutate_paper.py` (Paper's page:
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
