<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Void Garden: the theme

Void Garden is the page's fifth theme (`?theme=voidgarden`): a night garden drawn on the GPU that
grows with the music, dark only. It is the one theme that draws a 3D scene, so it is the one with a
frame budget, a choice of renderer and a library of its own (three.js). This page says how it is
built; [its checks](web-voidgarden-checks.md) say how it is held. What a listener sees and does is
in [the web UI guide](../usage/web-ui.md#web-ui); the contract every theme keeps is in
[web-themes.md](web-themes.md). Words such as the layers, the rig, the director, the dash, the HUD
and the panel are this page's.

The code is `bins/svolderd/web/themes/voidgarden/`. The files that matter, by what they own:

| Files | They own |
| --- | --- |
| `voidgarden.js`, `states.js`, `key-table.js` | the mount, the menu's rows and their kept choices, how the page stands, the keys |
| `scene-wire.js`, `gpu.js`, `tier-probe.js`, `budget.js` | the scene's loop, the renderer, the choice of tier, the frame budget |
| `features.js`, `cues.js`, `director.js`, `history.js`, `palette.js`, `wave.js` | what the scene reads: the features, the cues, the conductor, the 18 s history, the cover's colours |
| `motion.js`, `layers.js`, `layers/*.js`, `tsl/*.js` | the camera's flight and the rig, the layers in order, the shader nodes |
| `dash-wire.js`, `hud-wire.js`, `panel-wire.js`, `hud.js`, `draw/*.js` | the controls, the HUD and the audio panel: rules (pure) and drawing (DOM and 2D canvas) |
| `driver.js`, `debug.js`, `debug-wire.js` | the test driver and the readout with its switches |
| `vendor/` | three.js, unmodified |

## Vendored three.js

Void Garden loads three.js r187 by a dynamic import, only when it is chosen: three build files
(`three.core.js`, `three.webgpu.js`, `three.tsl.js`) under `themes/voidgarden/vendor/`, unmodified
(MIT). They are listed in `themes.rs` like any script, and their licence text (`three.LICENSE`) is
kept beside them and not served, as htmx's is. `make vendor-three` fetches the release from the npm
registry, checks its pinned sha256 and writes the three files; a second run writes nothing. The
page loads nothing of it for another theme.

## The scene's loop and the tiers

`createGpu` (`gpu.js`) makes one renderer on one of two **tiers**: WebGPU where the browser gives an
adapter, else WebGL2, the floor. A choice (below) can force either. When neither gives a context the
theme draws a still of the valley (an SVG) and says `This theme needs WebGPU or WebGL2`; the keys
still write and the feed steps once a second. When there is no adapter the theme sets the WebGL2
floor itself, so three.js's own console warning never fires.

The renderer's tier, the reason for it (`chosen`, `not a secure context`, `no adapter`), the vendor
and the capability bag (compute, `maxParticles`, whether the neighbour pass exists, a GPU timer) are
what `?vg-debug` words as `WebGPU · amd`, `WebGL2 · no adapter` and so on. WebGPU's particle ceiling
is four times the floor's. Before its first frame the scene compiles every material with
`compileAsync`, so the page stays live while it loads.

The scene holds the shared analysis stream only while it is shown (let go after the core's grace);
the dash's `no music analysis: <why>` reads a second holder of the same stream, open only while the
page shows. The clock for the cues is the analysis hello's offset to the daemon's.

**Choosing the tier** (`tier-probe.js`). The Renderer row is Auto, WebGPU or WebGL2. Auto measures
both on a browser's first visit (a moment each, behind a black frame) and keeps the faster, as the
`renderer-pick` preference `{ v: 2, picks: [{ key, tier, fps }] }` (the last 4 keys); `key` names
the browser and its version, the GPU and a class of the screen's size, so an update, another GPU or
another size measures again. A change of the row applies on the next load. Why measure: on the same GPU Firefox's WebGPU can draw
the top step at about half the rate of its WebGL2 (the time goes outside the passes, in its GPU
process), where Chromium's is the other way about. `?vg-gpu=webgpu|webgl2` forces a tier for the
visit; `?vg-probe` makes a check's browser measure (it otherwise keeps WebGPU) and
`?vg-probe=slow-<tier>` slows one tier. On Firefox's WebGPU the loop keeps the frame polled while it
draws (Firefox noticed a finished frame only on a 100 ms timer of its own).

## The feed, the cues and the director

Everything the scene reads is pure and node-tested, and none of it touches the DOM.

- **Features** (`features.js`) are ten numbers from the analysis frames: bass, mids, highs, loud,
  novelty, centroid, flatness and the left, right and width of the stereo image. Flatness is a map in
  dB, and the panel's words say it as `0.31 tonal` or `0.62 noisy`; the centroid is on a 30 Hz to
  14 kHz log axis, in Hz or kHz. `history.js` keeps 18 s of every feature.
- **Cues** (`cues.js`) are read from the page model: `mute`, `pause`, `changed` (the path changes the
  sound), `cross`, `weak`, `linkDrop`, `volume` and the rates. A weak link is a fresh reading
  (under 120 s, as the daemon's `SIGNAL_OLD_SECS`) below -80 dBm; the model carries the signal as
  words, so `facts.signal_age.head` is parsed. A stream silent for 2 s drifts as if paused; a
  player's paused or stopped status counts only after half a second of silence, and sound heard
  outranks the status, so a stale player under a playing sink does not stop the scene. The scene
  listens to the whole sink: a notification counts as heard.
- **The conductor** (`director.js`) turns the features into events: the beat (tempo and phase from the
  core's beat clock), hits of the kick, snare and hats (onsets against a slow mean, with a least gap
  each), peaks, morphs, the **section** (intro, build, drop, break, with hysteresis and a 3.5 s
  pending rule) and a reseed at a track change. Its clock is the sum of the `dt` it is handed, so the
  same input gives the same events at any step rate.
- **Flash limiter.** Every accent that changes the whole frame's light (the effects below, the
  director's flashes) asks `FlashLimiter` first: at most 3 grants in any second, as the page's rule.
  Accents are drawn only on Full motion, never in Calm or while muted.
- **Explore** (`director.explore(kind, { flash })`). `seed` draws a new set of worlds for the track,
  `scene` goes to the next world of the set, `effect` fires one of the scene's two effects through the
  limiter (with no flash unless Motion is Full). A track change reseeds from the title and the artist;
  a player that announces the same track again resets the explored world to the track's first.
- **Determinism.** A seed and a time give the same frame: the noise reads a 32-bit integer lattice
  hash (the same in JS and in the shader, bit for bit), so every GPU draws the same land. A float hash
  would give each GPU other last digits, and a CPU mirror could never sit on the drawn land.

## The rig and the layers

`motion.js` holds the rig: the clock, the uniforms as time constants, the camera's flight along the
river, and the accents. The flight is not rebased, so in 32-bit floats the vertex function drifts from
the CPU mirror with the distance flown (0.02 units after 30000, about 0.07 after 3.5 hours).

A layer is `(ctx) => { name, group, object, build(seed), update(frame), dispose() }`; `layers.js` has
the order, and the order matters: **sky** builds the light, **land** the terrain, **river** and
**flora** read both, then **beacons**, **hero**, **particles** and the **post** chain. `ctx` carries
what layers share: `ctx.terrain = { height(x, z, oct), riverX(z) }` (the seam, set in the land's
factory and again at each build, so a holder survives a reseed), `ctx.land` (the mirror and the shader
nodes of the terrain), `ctx.env` (the light's uniforms) and `ctx.shading` (the fog). Palette colours
are `new Color(hex)`, never hex divided by 255. Every read of one element's buffer by another goes
through `lib.neighbours`, and sprites are drawn by `tsl/sprite.js`, never as a Points object.

**Sky.** Six moods (dusk, golden hour, night, storm, dawn, overcast) with a sun that follows the
river's heading, clouds and stars, coloured from the track's two cover colours through one of six
palette programs. The cover is the same-origin `api/v1/host/cover?v=<hash>` drawn at 32 by 32; a
missing, grey or failed cover gives the seeded pair. It is decoded only while a GPU scene runs.

**Land and river.** Terrain is one function with an exact JS mirror (`createTerrain(seed)`). A seed
gives the river's phases and swing, the noise scale, the relief, the valley size and the snow line; a
chain of ten kinds of world (valley, canyon, braided delta, archipelago, mesas, coast, gorge country,
spire forest, highland woods, fjord) in a shuffled bag, never the same twice in a row, flips every
1800 units over a blend of 100. The river ripples, mirrors the sky, foams and carries the bass's light;
waterfalls fall off steep banks. A mute frosts the land and a changed path thickens the fog.

**The terrain cache** (`layers/land-cache.js`). The land is shaded per pixel from textures, not from
noise: five nested levels of tiles ahead of the camera are drawn into render targets, a few tiles a
frame, holding the height, the normal and the rock's noise (RGBA16F, or a packed RGBA8 where float
targets are absent). A pixel reads a texture, so the land costs about a fifth of what direct shading
did, at the same look; far past about 150 units the cache's texels filter noise that would alias. A new
track or a seek draws what is in view before the first frame shows. The cache's layout and texel
mirror are pure; `ctx.land.direct` shades from the terrain per fragment and is the checks' reference;
whoever adds a context-lost restore calls `ctx.land.cache.reset()`. The river reads its bed's depth
from the cache and the land's vertices take their height from it. The land takes its octave count by
distance, fewer far away, so there is no seam at a ring's edge, and is drawn near rows first.

**Flora.** Seven kinds of tree (pine, cypress, broadleaf, bare, palm, crystal, willow), rock spires
and far trees as sprites, placed on the CPU through the mirror in three grids (near 72 by 72 cells of 2
units to 84, far sprites 96 by 96 of 3.6 from 70 to 250, spires 26 by 26 of 24 to 330) in a fixed
shuffled cell order, so a share of them thins evenly; they are rebuilt when the camera's cell, heading
sector or the share changes. A trunk is sunk 0.25 into the mirror's height at 4 octaves.

**Beacons.** Four of six kinds in each world (pillars, masts, a lighthouse, obelisks, a runway, a ring
of stones; the first world has the sculpted four), as faceted bodies lit by the sun and sky with beams
and reflections, drawn from compute. A body the camera is about to pass shrinks inside 7 units.

**Hero.** The lattice planet by default, large and close, wearing an instrument: a ring of 128
spectrum bars, three band curves, a faint disc and an oscilloscope trace; sections switch the parts on
and off, eased. Nine forms (lattice planet, noise icosphere, EQ sphere, torus, cube, helix and spiral,
glyph swarm and the sculpture) and the sculpture's ten shapes (knot, gyroid, galaxy, lotus, lissajous,
superformula, helix, moebius, nova, swarm). A `morph` goes to the next form of a shuffled bag; a
`section` always changes form and scatters it into particles that reform. Its size follows the section
(intro 1.7, build 2.7, drop 4, break 2.1). The kick squashes it, a twist sways it over two bars, each
bar bends it the other way, the snare folds spikes and the hats shimmer.

**Particles and compute.** The EQ forms and the sculpture are stateful: a compute pass keeps each
particle's offset from its form and its velocity (a damped spring), a burst is an impulse into that
state, and the form is recomputed from the keys each frame. Counts: EQ forms 22000 (WebGL2) or 44000
(WebGPU), the sculpture 40000 or 100000, the swarm 6000 or 24000; all are scaled by the budget's particle
share and held to the cap. The swarm's motes carry trails; on WebGPU a neighbour pass reads six other
motes (separation, cohesion, alignment), on WebGL2 there is none and they fly on steering alone.
**A compute pass on WebGL2 may name at most four storage buffers** (each is a transform-feedback
output); a fifth makes the pass write nothing and say nothing, so every pass is split to stay within it.

**Post.** `tsl/post.js` is a chain: bloom with a second octave, anamorphic streaks, a chromatic split,
grain, VHS bleed, datamosh, pixel sort, barcode strips, a 1-bit grade, barrel and heat haze. The
accents (barcode, datamosh, pixel sort, VHS, flare) are Full only and pass the flash limiter. The chain
draws the scene once per frame, skips the effects that are at rest, and `nopost` bypasses it.

## The draw order: backdrop and front

At full share one pass draws everything. Below it the frame is two passes: the **backdrop** (camera
layer 1: sky, land, river, flora) is drawn into its own target at the budget's share of the canvas and
the **front** (layer 0: hero, beacons, particles) at the canvas's own size over a quad (layer 2) that
writes the backdrop's colour and depth (`depthNode`, always-pass), then the post chain. So only the far
picture gets coarse and the central sphere stays crisp. The backdrop's depth is read nearest and its
colour bilinear, so a cut where the hero meets a land edge is the backdrop's texel. The sky is drawn
after the opaque land, at the far plane, so it shades only the pixels it shows. The hero and the
beacons' lights draw after the water, so the hero never sinks behind the river.

## The frame budget

`budget.js` is pure: it is told each frame's cost and says which step to hold. Its aim is a smooth 60
draws a second where the screen and the GPU allow it (a desktop) and an even 30 otherwise (a phone, or a
screen that cannot show 60). At 30 the frames are paced evenly (`capped`).

**The ladder** (`ladderOf(base, coarse)`) is a hierarchy of small steps in the order a viewer notices
least (`GROUPS`):

1. the backdrop, in two rounds: the sky's detail (cloud octaves 4, 3, 2), the water's (the two finest
   ripples), the land's detail normals, its rings (.75, .55, .4), the trees (.7, .5), then the backdrop's
   own resolution (.9, .82, .75, then .67, .6, .55, .5);
2. the particles (.7, .5, .3);
3. the post chain's cost (its bloom drawn at half its size);
4. the effects (the bloom's second octave, then the grit);
5. the hero's particles (.6, .35);
6. the frame's whole resolution (0.82 down to 0.3; a phone 0.75), as the last resort.

The land's rings are not dropped by default beyond those rounds: at half the vertices a mesa's terraces
turn jagged. A density above one pixel per CSS pixel on a hi-DPI screen goes with the backdrop's first
round, long before the effects or the hero; the backdrop is never drawn under 0.3 pixel per CSS pixel.

**Reading the cost.** The GPU's own time is read where there is a timer: the WebGL2 tier through
`createQueryTimer(gl)` (the renderer's own tracking is off there and only one query may be open; Chromium
gives it, Firefox does not) and the WebGPU tier through the timestamps, which sum the compute passes too
and are divided by the renders since the last read (one frame's time, not an eighth). Without a timer the
draws a second are the evidence. The limits follow the display's own refresh rate, read from the offered
frames (24, 25, 48, 50 where steady, else 60 or more), at 0.92 of it, so a 50 Hz screen is not walked down
for pacing the 30 cap at 25. A step down is quick (a second a step, two when far over); a step up waits for
a calm spell (12 s) and is on trial (5 s), and the way up shuts for 60 s, doubling to 480 s, after a trial
that was too much. While chasing 60 only the backdrop's steps are spent; after `chaseSteps` (5) of them
it judges whether the rate rose by `chaseGain` (4 a second), and if not it settles on 30 at the least step
that holds it, found from the costs read on the way down. So a GPU that can do 30 at full detail gets full
detail at 30. 60 is tried again on trial whenever its wait is over (`retry60Tries`: after two failed
trials in a row not until a reload; 60 held for 30 s resets the count), and goes back to 30 at once when the
GPU cannot hold it; it is not tried while the bottom step read over twice the 60 limit lately.

**Kept.** The budget keeps where it settled per browser as `{ v: 4, key, step, fps }` (`key` is the
tier, the GPU and a class of the screen's size), so another GPU or a window of another size starts at the
top and reads again; a version that is not 4 is read again from the top. A resize maps the current levels
(scale, particles, rings, look) onto the new ladder by meaning. A phone starts one step down with a render
scale never under 1.0 (the last resort 0.75). The Detail row: Auto is all of this and is the only mode
that writes the kept tier, High pins the top (it also lifts a phone's cap), Low pins the lowest step; an
explicit choice does not step back up. `driver.forceDrawMs(ms)` turns the draws-a-second rule off for
a check, and `driver.hold(step, fps)` holds a step.

Known edges are in [the known issues](../known-issues.md#void-garden). `make voidgarden-measure` prints
the cost of a step, what the budget settles on and a still on the machine's GPU
([the checks](web-voidgarden-checks.md#measuring-on-a-real-gpu)).

## The dash, the HUD and the panel

**The dash** (`dash-wire.js`, `draw/dash.js`). On a desktop two dark-glass capsules in the bottom
corners: left, the device and `This computer`, then the volume (`−`, the figure, `+`, mute) and the input
as one chip that opens a list; right, the cover and the track (the theme draws the track, because the
theme fact `Cover` is the theme's to show), previous, play or pause, next and the explore keys. On a phone
the features chip and one capsule (volume, play, `⋯`), the sheet behind `⋯` holding the rest, every
target 44 px (desktop keys are 34 px with a 44 px hit area through `::after`). `Device controls…` is the
first row of the theme's menu and opens the core's popup; every target the dash does not draw (power,
modes, bands, presets, slots, controls by kind, the computer's points and bypass) is reached through
`core.driver.controls`. A held key repeats by the core's rule (380 ms, then 70 ms, ending at 10 s);
`↑` `↓` step the volume the same way. There is no wheel over the volume figure.

**The fade.** The class `vg-dim` on the page fades the chrome by opacity alone, after `fade-after`
seconds (3 by default, 4 on a phone; a hand-set preference). Trouble, a held key, an open list, the
keyboard focus in the corners and a mouse over them hold it up; the input that wakes it only wakes it
(on a touch screen; a mouse's move wakes it before the click). The HUD dims with it.

**States** (`states.js`). `data-mood` on the page is `ok`, `unreachable`, `lost`, `wait` or `nocap`;
the scene dulls and is redrawn 500 ms apart while reading again or resting, the dash is inert while out
of reach or lost, and a quiet line or a card (with `Try now`) says so in words. The press that wakes a
faded card only wakes it.

**The HUD** (`hud.js`, `hud-wire.js`) has three modules of real readings. *Host*: output, format, the
computer's EQ with its curve as a 64-value sparkline over -12 to +12 dB, crossfeed, virtual speakers,
loudness, VU L and R in dBFS (the RMS from the levels' bytes with a held peak and the over lamp; the
levels stream is held only while Host shows and the page is visible) and the signal path (amber when it
changes the sound, with the PipeWire config chip, drawn whenever the daemon sent a check and opening
`setup`). *DAC*: volume with its bar, input and rate, the profile's own controls by kind and the link
with the signal bars, the model's own words (`−58 dBm live`, `advert`) and 8 s of history. *Amp*: power,
tone, loudness and one bar per meter; none for a declared, unlinked hop, which has no model to read. A
row the model does not carry is left out. Words are DOM text; bars, sparklines and the history are one
aria-hidden 2D canvas over the rows' empty slots. The **director's moments** bring a module in: at the
start every module shows 5 s; a section change shows Host 4 s (a drop the Amp too); a new track the DAC
4 s; the path starting or ceasing to change the sound 5 s and a new rate 4 s, both Host. One module per
slot; one at a time on a narrow window; while the panel is Spread every module is in. A module's strength
is its opacity alone (0.25 s in, 0.4 s out). A top line (device, link words, the verdict or the trouble)
shows on a desktop, fading with the chrome; a phone has none.

**The audio features panel** (`panel-wire.js`, `draw/panel.js`). Panel, Spread or Off (the `A` key and
the Audio panel row cycle it; the driver's `operate('Audio panel', { mode })` writes the preference like
a menu change). The ten features each have a 24-segment bar with a peak hold and ticks and a three-digit
figure; the traces are 18 s with marks at -15, -10 and -5 s, then the lamps, the rate, the seed and Q, in
the scene's own palette. It fades with the chrome (3 s, 4 s on a phone; `vg-dim`) and only input
wakes it; `Pin features panel` keeps it from fading (the preference `panel-fades` is its reverse).
Spread lays the features around the screen (bass, mids and highs as upright meters with a trail at the
bottom left, loud and novelty strips above, centroid and flatness dials, left, right and width rows). On
a phone the chip opens a compact box at the top with two columns and 9 s of traces; a phone turned
sideways takes the phone layout. The traces are the 2D canvas's on both tiers.

All these run on the scene's time (the sum of the steps' `dt`), except the dash's hands (held keys, the
idle fade, the card's seconds), which read the real clock. The HUD's and the panel's classes all start
`vg-h-` and are under `#vg-hud` and `#vg-panel`; the bottom of the screen is the dash's (`--vg-dash-h`).

## Menu rows and preferences

The preferences are under `svolder.theme.voidgarden.<name>`, kept per browser; where the browser keeps
nothing a choice stands for the session (the mount puts them behind a memory layer for the scene's use
and gives the core's back at unmount).

| Row | Preference | Values |
| --- | --- | --- |
| Motion | `motion` | `full`, `calm`, `still` (reduced motion starts on Calm) |
| Audio panel | `panel` | `panel`, `spread`, `off` |
| Pin features panel | `panel-fades` | on by default |
| Detail | `detail` | `auto`, `high`, `low` |
| Renderer | `renderer` | `auto`, `webgpu`, `webgl2` (read at load) |
| (hand-set) | `fade-after` | seconds |

## The readout and its switches

`?vg-debug` shows a readout whose first line names the tier; `?vg-debug=nosky,noland,noriver,noflora,
nobeacons,nohero,noparticles,nopost,nodash,nohud,nopanel` also turns the named layer or part off from
the first frame, to find on a phone what costs the most. The query rides through the core's routing; a
fragment does not. `?vg-gpu=webgpu|webgl2` is above.

[Back to the web themes](web-themes.md).
