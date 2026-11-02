<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Themes and effects

The web page's theme Ember owns an effects layer behind everything it shows, with three levels:
Smoke, Glow and Off. Other themes bring their own effects or none; the layer, its menu rows and
Arrange mode are Ember's, and the menu that shows the rows is the frame's. This page describes the
layer as the parts a theme is made of: the look constants, the effect that registers itself, the
rules that decide what it does, the engine that draws it, the runtime that wires it to the page,
the menu rows that offer it, and the checks that prove it. It says how a theme hands the smoke its
own rules, so a different look or a different effect is a change to a named part and not a hunt
through the page. The files are in `bins/svolderd/web/themes/ember/`. What a theme is, and what
the frame and the core give it, is in [web-themes.md](web-themes.md). This page belongs to the
[web UI](web-ui.md) and sits below [the map](../architecture.md). The terminal UI has its own
decoration (a palette, a glow along its spectrum, an animation layer) in [tui.md](tui.md); the two
share a word and a principle, not code.

## A theme's parts

- **Tokens**: colours, type and sizes as custom properties, and Glow's three tints; `:root` in
  `style.css`. The `--frame-*` tokens dress the frame's menu button.
- **Look constants**: every number of the smoke's look, with its unit; `TUNING` in
  `smoke-model.js`.
- **The effect**: an id, a label, a support check and a `start`; `register(...)` in `effects.js`.
- **The rules**: what the smoke does for each event and each frame, with no GPU; `Director` in
  `smoke-director.js`.
- **The engine**: the GPU's work, a plan in and pixels out, no decisions; `createEngine` in
  `smoke-engine.js`.
- **The runtime**: the page's listeners and the animation loop, wired to rules and engine; `start`
  in `smoke.js`.
- **The menu rows**: a radio per level and the Visualizer switch, given to the frame's menu by
  `menuRows` in `effects.js`; the frame (`core/menu.js`) draws them.
- **The checks**: rules proved in node, the rest in a browser; `scripts/web-checks/`.

## The layer and its menu

`effects.js` owns the layer, a fixed element behind the whole page (`.fx`), so no column clips what
it draws. It keeps one choice per browser:

- `LEVELS` is `smoke`, `glow`, `off`, in the menu's order. The choice is stored in `localStorage`
  under `KEY` (`svolder.effects`), read and written inside `try`/`catch` (`stored`, `keep`); a
  refused store leaves the menu working for the page's life.
- `pick` chooses what shows: the address's `?effects=` level, else the stored one, else Smoke (Glow
  under `prefers-reduced-motion: reduce`, the one thing that setting still changes). Smoke on a
  browser that cannot run it shows Glow and the menu says why. The page never changes the choice
  itself, with two exceptions that fall back to Glow for the page's life and tell the person: an
  effect that fails to start, and a lost WebGL context (`contextLost`).
- `supported()` of each effect is probed once, and only when Smoke is to show or the menu first
  opens, because Smoke's probe makes a throwaway WebGL context.
- The menu is the frame's ([frame.md](frame.md); `core/menu.js`, opened by the frame's button over a slot beside the
  connection pill): its Theme row, then Ember's rows, then the Device and Sign out rows. Ember's
  rows are a heading, a `menuitemradio` per level, and a switch row; the frame gives them the
  keyboard and ARIA behaviour of a menu. `menuRows` draws the levels from the registry, so the menu
  and the registry cannot disagree (`scripts/web-checks/effects_cycle.py` checks it); a theme's
  `menuOpening()` runs when the menu opens, which is where Ember asks whether each effect can run.

The page tells the layer what changed through a few calls, all no-ops for Glow and Off:
`level(x)` (the volume's fraction, 0.06 when muted or unread, or the louder meter), `step`
(a volume step, for a puff at the dial), `meters` (an amp's fractions), `away` (the device is off
or its link is lost), `lost` (the page's own stream), `layout` (the dial's or meters' place and
the content column's box, after a resize, a scroll or a screen swap) and a click on a control
(`CLICKABLE`). A few calls (`stats`, `haze`, `seed`, `roll`, `readback`) exist for the browser
checks and are inert unless called.

## Arrange mode

Ember draws its sections in the order this browser kept (`order.js`): a layout has three zones, the
band across the top and two columns, and the person moves the sections between them. The order is
`{ band, left, right }` of section ids, one per layout (a DAC's is not an amp's), kept under the
theme's own preference `arrange` (`svolder.theme.ember.arrange`), by Done alone. A section the
screen does not draw (a DAC has no power button) keeps its place in the order; one a later version
adds goes to the end of its default zone. `screen.js` draws in that order and marks the view
`.arranged` when it differs from the default in what is drawn; the style sheet's phone `order`
rules and the visualizer's `place` leave an arranged view alone, so on a phone it reads band, left,
right.

`arrange.js` is the mode (the menu's "Arrange sections…" row, or `A`): a bar, a handle on each
section, the sections' own children inert, and a drag by the handle or the arrow keys on it. The
sections move in the page (`sections.js`, with `moveBefore` where the browser has it), never drawn
again, so a part the stream swaps meanwhile lands in its section by id and is given its handle by
a `MutationObserver`. Escape puts back the order from before; a theme that unmounts, or a screen
that changes shape, ends the mode without storing anything. The keys that write leave the page
alone while it lasts. Checked by `node/ember-order.mjs` and `arrange.py`.

## The contract and the test driver

Every theme presents everything a device offers, on every kind of device, and shows every fact the
Device section holds (the design says no theme points to another for a missing piece).
`core/contract.js` lists what that is, as data from a device's model: `targetsOf(device)` is every
control and action (`{control: key}` for the volume, its mute, each control, the power switch and
what is under it; `{band, param}`; `{mode}`; `{loudness: true}`; `{preset: name}`; `{slot: n}`;
`{setting: name}`; `{transport: button}`, only when the viewer has the buttons and there is a
track: no track, nothing to operate) and `factsOf(device)` the labels of the facts (Product,
Unit, Firmware, DSP, Profile, Link, Signal, Passphrase when there is a note, and the id of every
reading row).

A theme exports a `driver` (Ember's is `driver.js`) that the checks use to prove it: `reach(target)`
shows a control or a fact the way a person would (scrolling, opening a fold) and says whether it is
there; `operate(target, change)` does what a person does to it (`{step: ±1}`, `{raw}`, `{value}`,
`{on}`, `{apply: true}`, `{recall: true}`, `{save: true}` or nothing); `shown(target)` is the words
the theme shows for it; `state()` says `{lost, waiting, resting, toasts}`. A driver acts on the
theme's own DOM and never calls a core action itself, so what the check sees at the daemon went
through the theme's own handlers. Checked by `contract.mjs` (the lists), `contract.py` (a short
pass per theme: volume, mute, a band, a preset, a device that goes away and comes back, a refused
write, a page that rests and wakes, the menu button and `T`, `?theme=`, and the same as a guest),
`completeness.py` (every target and fact on a DAC, an amp, a five-meter amp, a mono amp and the Loki
Max) and `core_rows.py` (what the core holds for the themes to come: a second device followed, a
declared hop, the menu's fade, a band moved from plain coordinates, light and dark, a canvas loop
that pauses, a theme that unmounts clean).

## Glow

Glow is the page's own CSS and needs no script beyond setting the level (`.glow`,
`style.css`). Three radial gradients tinted by `--glow-1`, `--glow-2` and `--glow-3` sit behind
the dial of a DAC or between the meters of an amp (`--fx-x`, `--fx-y`, set by `layout`), with an
opacity of `.22 + .78 × --lvl`, so a louder volume is a larger, brighter glow. It turns slowly
whatever the system's motion setting says (the Off level stills the page), and it greys when the
device is away.
It is registered with `canvas: false`: the layer makes no canvas for it.

## The smoke

The smoke is one WebGL2 stable-fluids simulation, written from the published method, that the
pointer, a finger, a scroll, a volume step, an amp's meters and a click stir, that rests as a haze
behind the whole page, and that now and then carries a faint music note off in its swirls. It is
split three ways so each side can be changed and proved on its own.

### The director: the rules

`Director` (`smoke-director.js`) holds every rule that needs no GPU. It takes the page's events
and the time from its caller, and a `random` function (never a clock, the DOM or WebGL), and
answers frame by frame with a `FramePlan` (documented at the head of the file): whether to
simulate, wait or stop, the step's length, the splats to apply, a grid shift, the content band's
fade, a note to stamp, how to draw. The same events, times and `random` give the same plans, so
node proves the rules with a fake clock. What it decides:

- **The rhythm** (`rhythm` in `smoke-model.js`): stopped while the tab is hidden or the away fade
  is over, a frame for every animation frame for `stirredSecs` after a stir, otherwise the calm
  rate (`calmFps`). The page at rest never runs faster than calm.
- **What each input becomes:** the pointer and touch (a push along the motion, a little dye when
  pressed, never prevented), the scroll (broad splats whose strength follows the scroll's speed,
  capped), a volume step (a puff at the dial's dot along the arc's tangent, a burst of steps
  counted by `VolumeBurst`, a big rise by `bigRise`), a meter (continuous emission, a kick in its
  red zone), a click (a small puff).
- **The resting source** at the anchor (under the dial's centre, or between an amp's meters),
  its strength following the level, with no upward push so the smoke spreads and wanders.
- **The haze** (`Haze`, `HazeClock`, `hazeBands`): steady sources spread over the viewport and
  denser in the bands beside the content column (or the window's own edges where there are no
  margins), each wandering slowly and pushing weakly in a direction that turns. It is fed per
  simulated frame in proportion to the step, the same capped step the dye's dissipation uses, so
  its density is steady whatever the frame rate. Behind the column it stays faint so the content
  reads as well as under Glow, and the dye in the column's place fades when the margins change.
- **The notes** (`smoke-notes.js` draws three glyphs, a quaver, a beamed pair and a crotchet, into
  one strip: red the glyph, green a halo). An `Agitation` measure rises with stirs and decays; a
  `SettleWatch` notes a stir that passes `settleFrom` and falls under `settleBelow` and rolls the
  episode's odds once (`settleOdds`, `clickOdds`, `bigRiseOdds`); a `NoteGate` allows one note per
  `noteCooldown` and at most `noteCap` visible; `noteSpot` finds the nearest place that is dense
  enough and open (the page's `open` test says the box shows smoke, not a control). No note while
  away.
- **The shade** (`drawShade`): drawn once per page load from a cool range, a middle hue and
  two side hues of one muted saturation, a greyer tone and a lighter tint, kept across Smoke,
  Glow and Smoke again. The smoke only ever lightens a dark page.
- **Away, lost, hidden:** away greys and fades to a stop; a lost link or context stops it; a
  hidden tab runs nothing and time spent hidden is never simulated.

### The engine: the GPU

`createEngine(canvas, tuning)` (`smoke-engine.js`) owns the WebGL2 context, the programs, the
velocity and dye grids (half-float, the dye's resolution set by `dyeRes`), their resizing, the
passes (advection, curl and vorticity confinement, divergence, pressure by Jacobi iteration, the
pressure gradient, splat, the grid shift that holds half a viewport of dye each way and gives it
back on the reverse move, the content band's fade, the note's stamp, and the draw), the notes'
atlas, a float readback for the browser checks and the context's loss. It returns `null` where
there is no WebGL2 or no half-float target. Its interface (`fit`, `run`, `draw`, `shift`,
`clear`, `readback`, `density`, `stop`, `onLost`) is documented at the head of the file. It
executes a plan and decides nothing.

### The runtime

`start(canvas, theme, opts)` in `smoke.js` is the thin layer: the page's listeners (passive
`pointermove` and touch handlers, `visibilitychange`, a resize observer) and the animation loop,
which hand each event and each frame to the director and each plan to the engine. It returns the
handle the layer drives (`level`, `step`, `click`, `meters`, `away`, `anchor`, `scroll`, `stop`
and the checks' hooks) or `null` when the smoke cannot run. `supported()` says whether a context
and a half-float target are available, with a one-line reason when they are not.

## How a theme hands over its own rules

The registry is the first hand-over point. `register({ id, label, supported, start, canvas })`
adds an effect, or replaces the one with the same `id`; the layer asks `supported()`, makes a
canvas when `canvas` is true, and calls `start(canvas, theme, hooks)` for a handle (or `null`).
Smoke and Glow register this way, in the menu's order, and Off is the absence of one. To add a
level: register it, add its id to `LEVELS` (its row follows from the registry), and add its CSS
for `.fx[data-level=...]` if it needs any.

The second is inside the smoke. `effects.js` registers Smoke with
```js
start: (canvas, theme) => smoke.start(canvas, theme, {
  director: smoke.makeDirector, tuning: TUNING,
})
```
`opts.director` is a factory `({ random, now, shade, roll, notes, open, density, tuning }) ->
Director`; `opts.tuning` is a `TUNING`-shaped object the engine reads (grid sizes, pass constants).
The rules read `TUNING` inside the director, so a theme that wants different rules passes its own
director (given `tuning` to use if it wishes) and its own numbers. The runtime needs only the
methods it calls on a director: `visible`, `frame`, `wake`, `pointer`, `touchStart`, `leave`,
`resized`, `level`, `step`, `click`, `meters`, `away`, `lost`, `anchor`, `scroll`, `roll`, `haze`
and `stats` (and `beat`, which a director may leave out), with `frame(now, size, resized)`
answering a `FramePlan`. The GPU code does not change for a new director or new numbers.

A theme's tokens are the third part: colours and sizes are `:root` custom properties in
`style.css`; Glow reads `--glow-1` to `--glow-3`, and the smoke's shade is drawn from its own
`shade*` numbers in `TUNING`. A theme that changes the page's colours changes the tokens; one that
changes the smoke's mood changes the shade numbers or hands over a director.

### The same parts in the TUI

The terminal's equivalents are separate and not a registry: `Preset` (colours, spectrum, ramp)
and `Theme` for the look, `Effects` and `EffectsConfig` for motion. See [tui.md](tui.md).

## Principles

**Effects are decoration, never the only carrier of a fact.** Every value they decorate is also
text. A level, a step or a meter only changes how the smoke looks; the page reads the same with
the layer off.

**Rules live apart from the GPU.** The director has no DOM, no WebGL and no clock; the engine has
no rules. Rules are then proved in node in seconds with a fake clock, and the browser checks keep
what needs a GPU or a real page. Put a decision in the engine and only minutes of headless Chrome
and Firefox can check it.

**Time and chance are arguments.** The director takes `random` and the time from its caller (the
runtime passes `seeded(n)` when the checks fix a seed, else `Math.random`). Read `Date.now` or
`Math.random` inside a rule and a check cannot reproduce a frame.

**Every look number is in `TUNING`, with its unit, and only there.** The checks pin those numbers;
a number hidden in a rule or a shader is retuned in one place and breaks in another.

**The page rests.** At rest the smoke runs at the calm rate and no faster; the haze's feeding
counts as calm; a hidden tab or a stopped away page asks for no frames. A haze that keeps the loop
at the stirred rate would keep the GPU busy while nobody touches the page.

**Content reads first.** The haze behind the column stays at or under Glow's densest pixel, and
the smoke never captures a pointer, never calls `preventDefault` and listens passively, so the
controls receive what they always did.

**The choice is the person's.** It is kept per browser, defaults to Smoke (Glow under reduced
motion: the only place that setting is read), and the page changes it only to fall back to Glow when
Smoke cannot start or the browser resets its graphics, and says so.

**A fallback is visible.** A browser that cannot run Smoke shows Glow and the menu names the
reason, so a missing effect is never a silent one.

**The registry and the menu agree.** The menu's level rows are drawn from the registry, so it
cannot offer what the script cannot start; a check compares them all the same.

**One simulation, no outside code.** The page's CSP is `script-src 'self'`; the smoke is written
here from the published method and loads nothing.

**Pictures are for looking, numbers are the proof.** The checks read the smoke with `readPixels`
and float readbacks, never a screenshot; screenshots of the smoke are for tuning by eye.

## Where the code and tests live

- Layer, registry, menu rows and Glow's level (the volume's, plus the music's share while the
  visualizer's frames flow, and the beat's puff to the smoke): `bins/svolderd/web/themes/ember/effects.js`;
  Glow's CSS in `bins/svolderd/web/themes/ember/style.css`.
- Rules and numbers: `smoke-director.js`, `smoke-model.js` (`TUNING`, `rhythm`, `Haze`,
  `Agitation`, `SettleWatch`, `NoteGate`, `drawShade`), `smoke-notes.js`.
- Engine and runtime: `smoke-engine.js`, `smoke.js`.
- The menu: `bins/svolderd/web/core/menu.js` (rows, keys), `frame.js` (the button, the fade, the key
  `T`), `picker.js` (the picker of themes) and `frame.css`; Ember's rows are `menuRows` in `ember.js`
  (the effects' from `effects.js`, then the Arrange row).
- Arrange mode: `order.js`, `sections.js`, `arrange.js` in the same folder.
- The contract: `bins/svolderd/web/core/contract.js`; Ember's test driver: `driver.js` in its folder.
- Checks: node harnesses `scripts/web-checks/node/director.mjs`, `model.mjs`, `notes.mjs`,
  `sources.mjs`, `effects.mjs`; browser checks beside them (`effects_cycle.py`, `haze.py`,
  `notes.py`, `scroll.py`, `sources.py`, `touch.py`, `glow_level.py` and others); the coverage map
  `scripts/web-checks/COVERAGE.md`. A retuned `TUNING` number is changed in
  `scripts/web-checks/node/look-values.mjs` too. A change to a smoke file needs the full gate; the
  quick gate refuses. Which gate and when is in [testing.md](testing.md) and
  [scripts/web-checks/README.md](../../scripts/web-checks/README.md).

[Back to the map](../architecture.md).
