<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Void Garden: the checks

The second page of [Void Garden](web-voidgarden.md): how the theme is held. It runs the contract
every theme keeps ([web-themes.md](web-themes.md), `contract.py`, `completeness.py`, `keys.py`, the
contrast and phone sweeps) and adds its own checks, which are in `scripts/web-checks/` and described
at length in its `README.md` and `COVERAGE.md`.

## What the checks may read

A check reads **numbers, not pictures**: a count of draws, a float field, a statistic of a small
readback, a rule such as "inside the window", "clear of the menu button", "a thumb's size". None reads
a colour, a place or a density of the scene, and none asserts a frame rate or a GPU time: the look is
a person's to change, and it lives in the contrast and phone baselines and in the stills of
`make voidgarden-shots`. The scene's time is moved by the check (the fade runs on a fake clock), so
nothing depends on the machine's pace.

## Cells

A cell is one engine (Chromium or Firefox) at one tier:

| Cell | What it is |
| --- | --- |
| `none` | both 3D APIs off: the scene is the still, so the page is what is under test |
| `webgl2` | the stock browser, the WebGL2 tier (the live scene, on software rendering in a gate) |
| `webgpu` | Chromium with the four SwiftShader switches, Firefox with `dom.webgpu.enabled` |

Every check that is not about the scene loads the theme in the `none` cell: on software rendering a
live scene blocks the page's main thread for seconds at each draw, even in Still, and a sweep loads the
page hundreds of times. The live scene is the `webgl2` cell of `voidgarden_theme.py`, the tier checks
and the stills.

## The checks

| Script | Holds |
| --- | --- |
| `voidgarden_theme.py` | what mounts (the layers in order, the slot last), the menu's rows and their kept choices (across a reload, with storage refused), the keys `A` `S` `N` `E` and the volume keys, the dash, the driver, reduced motion, the fade by opacity alone |
| `voidgarden_states.py` | the card and `Try now`, a lost stream, reading again and resting, no capture, a guest, a refused write, a card arriving while faded |
| `voidgarden_phone.py` | at 360, 390, 768 and 844 wide: no sideways scroll, the dash and keys in the window and a thumb's size, the menu button clear, the modules above the dash; Chromium's finger |
| `voidgarden_timing.py` | the serial lane: no draw while hidden, each level's pace as a ceiling, a held key's pace |
| `voidgarden_gpu.py` | the tiers: the forced tier, the same frame for the same seed and time, the live pixels, the budget stepping down and up under a forced draw time, the analysis held only while shown, the still and quiet line, the palette's colours, a neighbour read, the sprites |
| `voidgarden_overlay.py` | the HUD's rows and the panel's three modes, the fade, the words at 4.5:1 over the worst backdrop (the no-GPU cell only) |
| `voidgarden_land.py` | sky, land, river and flora: materials compile, the switches, the uniforms follow the cues, the vertex function against the JS mirror, the lattice hash |
| `voidgarden_hero.py` | the nine forms and the sculpture's ten, a morph, the particle counts against the budget and the cap, the swarm's neighbour pass |
| `voidgarden_fx.py` | the beacons, the post chain's effects, the grit and bloom steps, eruptions landing on the terrain |
| `voidgarden_perf.py` | the chain draws the scene once per frame and the `?vg-debug` switches do what they say |
| `voidgarden_shots.py` | stills for looking (`make voidgarden-shots`), not a check |

The layer checks run in the `webgl2` and `webgpu` cells of both engines; `contrast`, the phone sweep,
`contract`, `completeness` and `keys` run all five themes. The node harnesses (`node/voidgarden-*.mjs`:
budget, cues, dash, director, feed, fx, hero, hud, mount, palette, rig, scene, terrain) hold the pure
rules: the terrain's mirror against the reference land's numbers, the worlds, the trees, the waterfalls,
the cache's texel mirror, the budget's ladder and its trials, the HUD's rows and moments, the dash's
first-tap rule and the director's determinism.

## Measuring on a real GPU

`make voidgarden-measure ARGS='1920x1080 full step:0,4 settle:60 still:out.png'`
(`scripts/web-checks/voidgarden_measure.py`) drives headless Chromium on the machine's own GPU (the
WebGL2 tier; the renderer string is printed and must not name SwiftShader) against a throwaway fake
daemon and prints one JSON line a case. It is never a gate and asserts nothing; its numbers are for
tuning. The cases: `full[:switches]` (the cost of a frame at the top, with named layers off),
`step:N[,M]` (the GPU time, rate and scale of a held ladder step, at 60 or `@30`), `settle[:secs]` (what
the budget settles on and how long it took, and any flapping after), `still:PATH`, `pair:PREFIX` (the
top step and the settled step drawn at the same seed and time, to judge the loss and nothing else),
`shot:PATH[:seed[:t[:step]]]` (a frame at a seed and time for comparing two builds) and `load[:cpu[:mbit]]`
(a phone's first load under a slowed CPU and network: the bytes, the module parse, the renderer, the
layers' build and the first draws). The checks drive the ladder with `driver.forceDrawMs(ms)` and
`driver.hold(step, fps)`, and draw one frame with `driver.freeze()` and `driver.still(seed, t)`.

## Limits of the checks

The software renderer is not a GPU: the pixel checks are statistics, and a frame there takes seconds.
The WebGPU tier is not measured on the test machine's GPU (the browser gives no adapter there), so every real-GPU
number is the WebGL2 tier's. Contrast over the live scene is `voidgarden_overlay.py`'s worst backdrop
alone, and the no-GPU line's contrast is held in the baseline as a known failure.

[Back to the web themes](web-themes.md).
