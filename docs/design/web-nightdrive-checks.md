<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Night drive: motion, cost and checks

The second page of [Night drive](web-nightdrive.md), the page's fourth theme: how often the scene is
drawn (Motion), what it costs and how it is held to its budget, the readout that shows it, the checks
and the mutants that hold the theme, its known limits and what it found missing in the core. Words
such as the scene, the mountains, the floor, the dash and the sheet are the first page's.

## Motion

Motion has three levels, in the menu's `Motion` rows and kept as the preference `motion`. With none
kept, reduced motion (`prefers-reduced-motion`) starts on Calm and everything else on Full
(`motionOf`).

| Level | Draws | Road and events | Mountains |
| --- | --- | --- | --- |
| Full | at the screen's rate (`BASE_PACE.full` 0) | rolling; the sky's events fire | follow the music |
| Calm | at most 10 a second (100 ms) | stopped, no events, half-rate mood | follow with a time constant of `CALM_RIDGE` (0.6 s) |
| Still | about 1 a second (1000 ms) | stopped, no events, no flare, no embers | follow at that rate |

Whatever the level, a hidden page draws nothing (the core's `canvasLoop` stops), a dull scene is drawn
no oftener than every 500 ms, and the mountains being tuned are drawn at the screen's rate while
held. The loop is `canvasLoop({ …, reduced: 'theme', pace })`: the core keeps it running under
reduced motion and the theme reads `loop.reduced`.

**The idle pace, on a coarse pointer only.** While the music is silent (every column under 0.02 for
`SILENT_FOR`, 2 s) or the player is not playing, a phone (a coarse pointer) draws no oftener than
every `IDLE_PACE` (1000 / 30 ms, 30 a second), to save some of the battery. The pace is that high because Full goes on
moving while the music is silent or paused (the road rolls at its cruise speed, the embers and the sky go on), and a moving
scene needs 30 draws a second to look smooth (the road rolls 4.2 rows a second, so under 30 draws a second the steps show).
Calm and Still are slower than that already and nothing in them rolls. A desktop keeps its level's own pace.

## The frame budget

A phone, or a slow browser, must stay able to open the menu. The budget (`budget.js`, `BUDGET`) is
time-based, from each draw's own time and the clock handed in; no DOM.

**The base density.** A canvas is drawn at most at `dpr` (2) backing pixels per CSS pixel and at most
`megapixels` (2.5) backing pixels in all (`baseScale`), whatever the screen's density; the CSS size
is not touched. A phone at density 3.75 whose canvas is 682 by 1274 backing pixels is drawn at 2.00.

**What steps it down.** A mean draw time over `slowMs` (12 ms) across the last `window` (60) draws,
or fewer than `minFps` (24) draws a second for `lowForS` (3 s) while the scene asks for every frame
(a page slow in compositing or input handling draws fast and seldom; at the cap the bar is 0.7 of
the cap's rate). Each step takes one of, in order:

1. the wet road, held off for `wetHeldS` (180 s);
2. the density, by `step` (0.75) each time, down to 1 backing pixel per CSS pixel;
3. the cap: at most `fps` (30) draws a second, whatever Motion asks.

**The way back.** A mean under `calmMs` (6 ms) for `upAfterS` (10 s) lifts one step, in the reverse
order (the cap, then the density, then the wet road once its 180 s are over). A pause between two
draws longer than `gapS` (3 s), a hidden tab for example, restarts the window and the clock of the
way back, which are not evidence of anything. After the low-rate trigger the way back stays shut
for `heldS` (60 s), since drawing was not what was slow.

**Finding the limits fast.** Choosing Full from Calm or Still (`probe()`) lifts the cap, unless a step-down on the rate is
holding it, and measures for `probeS` (2 s, and again after each step it takes) in short spans: after `probeLookS` (0.5 s) of
draws, a mean over `slowMs` across the last `probeDraws` (8) draws, or fewer draws in that span than the bar above, steps down at
once, so a phone settles in about a second a step (a 5x CPU slowdown settles in about 2 s). The limits are the ones above; only
the time to find them out is short. A transient jank in the first 2 s of a probe can keep a lower tier; the ladder walks back up. The tier that held (`{ capped, step }`)
is kept per browser (the theme's `tier` preference) and is where the next visit starts; the measuring corrects it.

**A phone starts at the cap.** A coarse pointer starts with the cap on (30 a second) and the wet
road off, and the way back lifts the cap after 10 s under budget to `phoneFps` (60) rather than to the
screen's rate: a 120 Hz phone draws at most 60 a second, for the battery. `keepFrame`
(`motion.js`) holds the cap itself, because the core waits on a timer only for a pace of 100 ms or
more (`MIN_PACE`): the frames in between are left undrawn and their time carried to the next draw.
The cap takes the frame whose time is nearest the pace's, so a 30 cap on a 72 Hz screen is every
second frame (36 a second) and on 75 Hz 37.5: "about 30". The 60 ceiling is held as a ceiling
(`AT_MOST`, a draw once 90 % of the pace has passed), never the nearest frame, which on a 144 Hz
screen would be 72 a second.

**What it costs, measured** (`driver.perf()`, the readout's `draw` line; the draw time is the work of
one draw, not the browser's rasterising after it).

| Where | Draw time |
| --- | --- |
| Chromium, 1280 by 800, density 1, fake music | p95 3.85 ms of the mean of the last 60 draws; 12.0 ms at 4x CPU throttle |
| Chromium, 1280 by 800, every cell lit | median of 5 readings 3.5 ms (the checks hold it to 4.5 ms) |
| Chromium, 1280 by 800, quiet with the wet road and the haze | p95 3.5 ms |
| Chromium, 390 by 844 at density 3, CPU 6x slower | 4.0 ms at scale 1.5 |
| Headless Firefox, 1280 by 800, density 1, fake music | p95 9.1 ms |
| Headless Firefox, 1280 by 800, every cell lit | median of 5 readings 8.0 ms |

These were measured with the checks' own harness (`nightdrive_timing.py`, `what_a_draw_costs`) on a
shared machine, so they are upper bounds. Headless Firefox rasterises in software, so its numbers
are two to three times a real browser's: the 8 to 9 ms it reads sits between `calmMs` (6) and
`slowMs` (12), so the budget may keep the wet road off and nothing steps down. A check that reads a
Firefox's drawing time accepts the wet road staying off when it is 6 ms or more
(`nightdrive_theme.py`), and forces the budget's measured time to 2 ms with `driver.forceDrawMs` to
run the return path.

## The readout and the bisect switches

Add `nd-debug` to the address (`?theme=nightdrive&nd-debug`) and a small box under the device line
reads, twice a second (`EVERY_MS`, 500 ms), the draws a second (measured) and the pace asked, the mean
and 95th percentile of the last 60 draw times, the canvas's size in backing pixels and the density
(`3.75 -> 2.00`: the screen's, then the one in use), the scale, whether the cap is on, how many steps
down it has taken and whether the wet road is allowed, the Motion level, the long tasks of the last
10 s (`n/a` where the browser has no `longtask` entry type), what the music reaches in the sky (bass,
highs, the highs' jump) and what the ear hears of it (`med`, `long`, balance, embers), and which
switches are in force. The box is `aria-hidden`.

Name layers after it, separated by commas, to turn them off one at a time and find the cost:
`?theme=nightdrive&nd-debug=noglow,nofloor`. The names (`SWITCHES` in `debug.js`):

| Switch | What it turns off |
| --- | --- |
| `noglow` | the sun's, the mountains' and the bubbles' glow, and the HUD's and dash's CSS shadows |
| `nofloor` | the floor's heat grid |
| `nosky` | the sky, the sun, the stars and the events |
| `noridge` | the mountains |
| `nodash` | the dash along the bottom |
| `nohud` | the top line, the volume and the EQ line |
| `nomood` | the mood: the night, the sun, the heat and the road's speed stay as they were, with no flare and no wave |
| `nowet` | the wet road |
| `noembers` | the embers |
| `nohaze` | the haze over the far floor |
| `dpr1` | the density: 1 backing pixel per CSS pixel |

A word that is not a switch is listed in the box as unknown and does nothing. The parameter goes in
the query: a fragment is dropped by the core at once. A step to another device drops the query from
the address, and the box stays until the page is reloaded (`ASKED` is read once). Signing in with a
link also drops the query, so open the link once, then the address with the query.

## The checks and the mutants

- **Node** (`scripts/web-checks/node/`): `nightdrive-scene.mjs` (the camera, the axis, the music's
  levels, the floor), `nightdrive-ridge.mjs` (the heights, and that a steady spectrum's ripple between bands leaves a tooth of under 0.5 px, the second difference of the smoothed bands in px of height), `nightdrive-floor.mjs` (the heat, the near edge's
  continuity), `nightdrive-mood.mjs` (the ear, the look, the embers), `nightdrive-sky.mjs` (the
  events at any frame rate, Motion's paces), `nightdrive-budget.mjs` (the density, the step-down and
  the way back), `nightdrive-tune.mjs` (the map, the bubbles, the diamonds against the core's
  response, the morph), `nightdrive-hud.mjs` (the HUD's words, the fade, the dash's markup),
  `nightdrive-states.mjs`, `nightdrive-controls.mjs` (a scale's writes by device), `nightdrive-draw.mjs` (the sections and the thumbnail),
  `nightdrive-glow.mjs` (no blur) and `nightdrive-debug.mjs` (the readout's words and the switches).
- **Browser** (Chromium and Firefox unless noted): `nightdrive_theme.py` (the scene, Motion, the HUD
  and its fade, the dash, tuning, the budget's step-down on a slowed phone and the wet road's return, the cursor, the picker on a phone; some cases are
  Chromium only), `nightdrive_sheet.py` (the sheet and every control's write on a DAC and an amp),
  `nightdrive_timing.py` (a draw's cost, the wet road allowed at the start, the draws and floor rows a second, the budget's rate rule, a phone's start at the 30 a second cap and the readout's rate: the gate's serial lane), `nightdrive_phone.py` (the phone's two orientations) and `nightdrive_states.py` (out of reach, lost,
  no analysis, a card while faded), with the helpers in `nightdrive_base.py`.
- **The contract and completeness passes** run Night drive through its driver. The driver shuts or
  opens the sheet as the target needs it (the volume, the mute, the input, the power key, the
  transport and the cover live on the dash, everything else on the sheet), unfolds the facts,
  scrolls to the word, scale or chip, operates it as a person does and reads the words the page shows.
  It never calls a core action itself. The completeness pass asserts that every theme lists as many
  targets of each kind, and as many facts, as Ember, on every kind of device.
- **Mutation runners**: `node/mutate_nightdrive.sh` (twenty-two mutants of the sources: the camera's
  perspective, a band's column, the ridge's range and its smoothing, a row rate that ignores the
  tempo, the rays', the stars' and the showers' gaps, a per-frame average, a per-frame easing,
  diamonds on the closed form, a grace of 0, an EQ line without its preset, a muted `−0.0`, four
  inputs as chips, a moment every second, embers in Calm, a fade that waits a minute, a wet
  road back with no window, wet slices that sway each by their own phase, a linear heat curve, a near
  edge that does not fade)
  and `mutate_nightdrive.py` (twenty-nine mutants of the page, Chromium: a fade that trouble does
  not hold off, a focus that does not wake the HUD, the analysis kept while hidden, reduced motion
  starting on Full, Still at the screen's rate, a dash hold that never repeats, a diamond drag that
  writes nothing, a pinch on band 1, hit tests in canvas pixels, a device switch mid-drag that
  writes, a first tap on a faded dash that is let through, a long press that woke the dash and
  presses on its release, a wet road copied a tenth of a second apart, an amp's meters heard as a
  part, a sky drawn twelve times in every draw, a loop each mount starts and nothing ends, and others). The page runner counts a kill only when a check the mutant names fails, so a flake in another check
  cannot pass for a kill; the node runner counts a failure of any of its harnesses.
- **Stills**: `make nightdrive-shots` draws a DAC on the fake capture at rest, a held bubble with
  its bell and diamonds, the sheet, a phone upright and sideways (each on the device's side and on the computer's), a device out of reach and no capture,
  into PNGs under `target/web-shots/nightdrive`.

## Known limits

- The headless Firefox the checks use draws two to three times slower than a real one (the budget,
  above); its drawing-time cases accept the wet road staying off.
- Firefox touch input is not reachable headless: the touch cases (the picker on a phone, a drag of
  the menu) run in Chromium only.
- An amp's temperature gauge is checked on fixtures only.
- A bubble is not focusable; the sheet's band table is the keyboard path.
- Canvas 2D only. A coloured mesh of mountains in WebGL is not built.

## What Night drive found missing in the core

Night drive was built on the core alone. It found these gaps; each was fixed in the core (or, where
it was the theme's own, in the theme) in its own commit, before the theme depended on it:

1. **The core's analysis handed out the frames as text, and the decoding, smoothing and beat schedule
   lived in Ember's `viz-model.js`, which no other theme may import.** `viz-model.js` is now
   `core/viz-model.js`; Ember's visualizer imports it from the core, and its behaviour is unchanged.
2. **`canvasLoop` ran no loop under reduced motion and drew at every frame**, so a theme could
   not offer Full under reduced motion nor a slow level without waking at the screen's rate. It takes
   `reduced: 'theme'` (the loop runs and the theme reads `loop.reduced`) and `pace()` (the least
   milliseconds between draws; 100 or more is waited on a timer, then one frame; `dt` is held to
   twice the pace and never under a tenth of a second). The defaults leave every other theme as it was.
3. **`canvasLoop` waits on a timer only for a pace of 100 ms or more**, so a pace of 33 ms, the
   30 a second cap, drew at every frame. Night drive keeps its cap itself (`keepFrame`, `motion.js`),
   leaving the frames in between undrawn and carrying their time. No core change was made; a later
   theme that wants a short pace needs the same or a core timer for 16 to 99 ms.
4. **The frame's menu capped its height by `innerHeight` and the picker's sheet by an auto grid row**,
   so on a phone the last rows were out of reach and a pinched page's menu ran past the screen.
   `menu.js` caps by the visual viewport, and the picker is one `minmax(0, 1fr)` row in a `100dvh`
   border-box sheet with `touch-action: pan-y` and contained overscroll on the scrolling lists.
5. **The picker had no close button.** On a phone with no keyboard and no gesture for a tap
   outside the sheet, the picker could only be left by choosing a theme. It has a close button at its
   top right (`.pclose`), reached by the arrows and Tab after the cards.
6. **Space was no key of any theme.** A page with a music player had no key for play and pause. Space
   is the frame's second reserved key, in every theme: alone, not a repeat, with the focus not in a
   field or on a control Space activates, and no menu, picker or modal dialog open (the sheet counts),
   it posts one press of `pause` or `play` through `core.actions.press`. The contract's key checks
   cover all four themes.
7. **The tab's title, the held `−` and `+`, the menu button's place and the connection's words were
   each a theme's own.** `core.title`, `core.hold` and the frame's one button rectangle, opaque
   backdrop and 44 px size are the core's, and the conventions every theme keeps are listed in
   [web-themes.md](web-themes.md). The theme's hold, its `T` and its button follow them from the first
   commit.
8. **The link word beside the device's name (a theme gap, found by the contract's
   `the_link_state_is_in_words_beside_the_name`).** Night drive's top line showed only the card's
   title at the right. It now puts the device's link word beside the name while the link is not on
   (`Gungnir 2 · not connected · USB 1 · 48 kHz`), and the contract's words check covers Night drive
   too.
9. **A check gap, not a core gap.** The contract's menu-button check asked for `scrollY > 0`, which
   a page that never scrolls cannot give. It accepts a page that does not scroll where the button
   is pinned.

[Back to Night drive](web-nightdrive.md), [back to web themes](web-themes.md).
