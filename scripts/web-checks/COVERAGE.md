<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Coverage map: which check proves which rule

Each entry is a rule of the web UI's effects layer and the check that proves it. The rules of the
smoke's model and director are proved in node, on a fake clock and a seeded random, with no GPU and
no browser; the browser checks hold what needs a real page or a GPU. A test is named by its title in
the file (`ok('...')`, shortened where the title is long); a browser check by the function or
section that holds it. [README.md](README.md) lists the check files and the gates.

What the browser checks hold: finite and bounded fields, the calm rate as measured, the haze's
density, side bars and legibility against Glow, away and lost grey, real touch panning, the wheel,
resize and screen swaps, the Content Security Policy and the fallbacks, the step puff on the page,
and one note formed end to end.

"Mutants" names the runner and the mutant that fails the check when the rule is broken in a copy of
the code (see "Mutation tests" in the README). A rule with no mutant listed is proved by its test
alone.

## The approved look

- Every number of the smoke's look that a node scenario depends on equals the approved value in
  `node/look-values.mjs`. Proved by `node/model.mjs` "the approved look: TUNING holds the numbers
  of node/look-values.mjs", the one test that fails when a number is retuned in `TUNING` alone;
  the tests written for those numbers skip themselves in the other harnesses under `run-all.sh`
  (`LOOK_RETUNED_SKIPS=1`) and run in full once the two agree. The node mutation runners do not set
  the skip, so a mutant that changes a number is still killed by the scenarios written for it. The
  browser checks that use a look number (`notes.py`, `smoke_quick.py`, `effects_cycle.py`,
  `haze.py`) read it from the page's `TUNING`.

## Notes: when a note forms

- A fast back and forth never settles into a note, and a moderate sweep stamps none. Proved by
  `node/notes.mjs` "the pointer: a 600 px/s wave, a slow drift ..." (never reach the threshold) and
  "the pointer: an ordinary move ... reaches the threshold". Mutants: `node/mutate_notes.sh`
  (`agitPointer` far too high; `agitSpeedMax` clamp removed).
- The pointer's speed is counted in short sides a second, so a phone and a desktop agree. Proved by
  `node/director.mjs` "phone width: a flick of 255 px ... or 312 px ..." (the note's place lies
  within a cell of the flick) and `notes.py` `phone_section` (ordinary moves and clicks on Mute
  form notes at 390 x 844, at least one in fifteen tries each).
- Two notes are at least `noteCooldown` seconds apart, at most `noteCap` are alive, and a note's
  life ends. Proved by `node/notes.mjs` "NoteGate: the cooldown holds for 4 s", "NoteGate: the cap
  holds ... and the life expires" and "NoteGate: twenty settles inside one second ..."; at the
  director's level by `node/director.mjs` "gate: 4 s gap and a cap of 2 across settle and click
  notes" and "a glide of 40 small steps is one stir; twenty clicks one stir; one click every 1.3 s
  for 16 s ...".
- A settle waits for the stir to die down: `SettleWatch` holds the settle `settleSecs` after the
  rise, whatever the size. Proved by `node/notes.mjs` "SettleWatch: the settle waits settleSecs
  after the rise" and "SettleWatch: no settle while the agitation is above settleBelow"; in the
  browser, `notes.py` waits on the page's own settle counter instead of a fixed sleep.
- A glide of many small steps, and twenty quick clicks, are one stir: one settle, one roll, one
  note at most. Proved by `node/director.mjs` "a glide of 40 small steps is one stir; ..." and
  `node/notes.mjs` "SettleWatch: one place per episode ...".
- Each source settles at its own odds: a mouse stir and a one-step volume change at 1/3, a click
  at 1/2, a big volume rise (over `bigRiseDb`, over an eighth of a dial without dB, or into the red
  zone) and a fast scroll at the odds of their kind. Proved by `node/director.mjs` "odds: each
  source settles once, at its own odds" (the roll that wins tells the sources apart) and "step: a
  burst over 10 dB is big ...", `node/notes.mjs` "SettleWatch: the episode's odds are the highest
  of its contributions", "SettleWatch: seeded rates over 10 000 episodes are within 3 % of 1/3, 1/2
  and 2/3" and the `bigRise` and `VolumeBurst` tests. In the browser the win at a roll just under
  the odds and the loss just over them are `notes.py` checks (the page reports "big" for a rise of
  20 dB; a one-step change wins at 0.30 and loses at 0.40).
- A note is reachable from real input: ordinary mouse moves and real clicks on Mute each settle
  on every try and form a note within fifteen seeded tries, from a stir in thin smoke too and with
  Mute under the dial's cloud; the effects button rolls none. Proved by `notes.py`
  `visible_section` (it does not bound the rate: the odds are the node tests' job).
- Where a note forms: at the stir's place when that is visible and dense, else at the nearest
  visible, dense place on the whole canvas, else nowhere and no gap is spent; the dye is read once
  per note. Proved by `node/director.mjs` "note places: ...". A note is off its control, in smoke,
  and carved out of it: `notes.py` `visible_section`.
- The meters: a music peak into the red forms a note at one of the meters, whatever their number
  (one, two or five meters); a reading held at 0.9 forms none, a reading just into red for a moment
  forms none, a reading at 0.95 held for five seconds forms one. Proved by `node/director.mjs`
  "meters: a music peak into red forms a note at a meter on one, two and five meters ..." and
  `node/notes.mjs` "meters: ..." (three tests) and "a meter: the excess over red per second ..."; in
  the browser the page gives the smoke one anchor place per meter, at each meter's centre
  (`notes.py`, a load each), and the mono amp's peak forms a note end to end. Mutants:
  `node/mutate_director.sh` ("meters feed it below red", "meters fed from the eased reading, not the
  peak").
- Nothing stirs a note by itself: 600 s of an untouched page, the haze feeding, rolls no settle and
  forms no note. Proved by `node/director.mjs` "idle: 600 s of an untouched page ..."; 60 s of an
  untouched page in `notes.py`. Mutant: `node/mutate_director.sh` ("the haze stirs the notes").
- A scroll is a stir at the settle odds. Proved by `node/director.mjs` "odds" (scroll, both
  rolls); `notes.py` settles a fast scroll (3000 px in half a second) once and wins a roll of 0.30.
  Mutant: `node/mutate_director.sh` ("a scroll does not stir the notes").
- Away and lost drop stirs and forget the pending one: nothing that happens while the device is
  away or the link is lost rolls a settle or forms a note, a click while away makes no puff, no
  stir and no note, and a puff queued just before the spell is not drawn when it ends. Proved by
  `node/director.mjs` "away and lost: nothing that happens then rolls a settle or forms a note"
  and "settle: ... reset on away and lost; kept on hide"; in the browser, `notes.py` (a flick, then
  the link lost or the device away before it settles: no note, no settle after) and `smoke_page.py`
  `lost_link` ("the stir pending at the loss settles into no note"). Mutants:
  `node/mutate_director.sh` (the away reset, the away queue drop and the step guard, each killed
  by "away and lost"; "away leaves the pending stir standing"; "a click works while away").
- Reduced motion does not change the notes (the page's small motions ignore it altogether, only the
  first default level reads it: `reduced_motion.py`). Proved by `smoke_core.py` "reduced motion with
  a stored smoke: the canvas exists and draws" and `notes.py`'s end-to-end flick and gate; the
  director knows nothing of reduced motion.
- A flick forms a note after the gap ends: `notes.py` (a flick after the cooldown forms the next
  note) and `node/notes.mjs` "NoteGate: the cooldown holds for 4 s". A click every 1.3 s for 16 s
  keeps at least 4 s between notes and never more than two alive: `node/director.mjs` "gate" and
  the glide test above.
- Rates in a real browser: a mouse stir, a click and a 12 dB rise each settle every try once and
  win within the 99 % binomial interval of their odds; a note forms (it is reachable) from a real
  flick, a volume step and an amp peak. Proved by `node/notes.mjs` "SettleWatch: seeded rates ..."
  for the rates and `notes.py` for the reachability.

## Notes: how a note looks

- The atlas's three cells hold the red mask and the green halo, upright, at the glyph height the
  model says; each glyph is drawn inside its cell. Proved by `node/notes.mjs` "drawAtlas: ..."
  (seven tests) and `notes.py` (the atlas texture read back).
- A stamped note is carved out of the smoke, not added to it: the stamping step thins the smoke
  over its gap and leaves the smoke ringing it. Proved by `notes.py` look section with three calm
  notes (`LOOK_NOTES`; the glyph is carved at -1.2 to -0.35 against the smoke, `CARVE_LO` and
  `CARVE_HI`, and the halo's fit is -0.6 to -0.02, `HALO_LO` and `HALO_HI`).
- The tilt, size and place of a note stay in range: `node/notes.mjs` "notePlacement: ..." (two
  tests) and "notePosition: kept `reach` inside the canvas ...".

## The haze

- The haze is fed every simulated frame, as the dial is: over 10 s at rest the side bands'
  frame-to-frame change is at most 10 times the dial cloud's and under 0.002 in absolute terms (a
  pulsed feed gives 0.028). Proved by `haze.py` `steady` (both browsers) and `node/model.mjs`
  "HazeClock: the haze is fed every simulated frame ...", `node/director.mjs` "haze: fed every
  simulated frame in proportion to dt ...".
- The page has one palette of three related tones: two side tones and a dial tone at least 20
  degrees apart, inside the saturation band the look allows. Proved by `haze.py` `colour` (reads
  what the page was given) and `node/model.mjs` "drawShade: ..." (10 000 draws: the ranges, the
  20 degree spread, the saturation band), "Haze: every emission takes one of the two side tones
  ...", `node/director.mjs` "palette: ...".
- The side bars are denser than the column and steady: their density at 20 s and at 40 s agrees
  within 30 %. Proved by `haze.py`; long-run steadiness is held by `soak.py`, which runs before a
  merge that touches a smoke file and compares five-minute samples with `soak-baseline.json`.
- The haze does not follow the volume; the dial's cloud is brighter at a higher level, because the
  level feeds the dial only. Proved by `haze.py` (the level is 0.06 from the page's start, so the
  low reading is at 30 s).
- A stroke's pace: the dye along its path at 4 s beats the recorded baseline, and its centroid
  travels further in 2 s (the page runs 5 s before the stroke; the margin is threefold, 47 against
  14.9). Proved by `haze.py` (`pace_check`, against `smoke-baseline.json`). Mutant:
  `mutate_haze.py` "the old pace".
- A column change starts the band fade for `hazeFadeSecs`: `node/director.mjs` "bands: ...".

## The calm rate and the hidden tab

- A meter below red leaves the calm rate alone, and a red one runs every frame. Proved by
  `node/director.mjs` "rhythm: ..." (a quiet meter stays calm, a red one runs every frame) and
  `node/model.mjs` "rhythm" (four tests). In the browser the calm rate is measured in
  `smoke_core.py`, `haze.py`, `effects_cycle.py` and `smoke_quick.py`, and `smoke_page.py`
  `calm_rhythm` runs 6 s proving that the page's readings reach the smoke (meter splats are
  counted) and the own-handle red and below-red rates.
- A hidden tab simulates nothing and owes nothing on return: the first frame after is one frame
  with dt under 0.049. Proved by `smoke_core.py` (hidden for `HIDDEN_SECS`, 8 s by default, with its
  frame callback held) and `node/director.mjs` "haze: ... a 3600 s hidden gap owes nothing on
  return" for the hour.
- Twenty Smoke and Glow rounds leave one canvas and never more than one live context: no leak
  across switches. Proved by `effects_cycle.py` (ten rounds by the menu: one live context and one
  canvas, the loop's pace, the calm rate, the shade); the device switch is in `smoke_core.py`.

## What stirs the smoke

- The pointer's force follows the motion and the dye comes only when pressed: an unpressed stroke is
  counted and pushes the fluid along the motion (rightward velocity over 4 cells a second; a
  downward stroke pushes it down), adds no dye, and a far corner is untouched; a pressed stroke adds
  dye along its path. Proved by `node/sources.mjs` "pointerSplat: the force follows the motion and
  scales with speed ...", "pointerSplat: dye only when pressed ..." and `node/director.mjs` "splats:
  a pointer move pushes along the motion, y up", "clamped to maxPush, direction kept"; `sources.py`
  shows the page passes the button and the canvas holds the dye (readPixels). Mutant:
  `node/mutate_director.sh` ("pointer: y not flipped").
- A push adds, never sets: a slow pointer crossing a flow does not brake it. Proved by
  `node/sources.mjs` "pointerSplat" (the push is `add`, not `pull`) and `sources.py`. Mutants:
  `node/mutate_sources.sh`.
- A volume step puffs at the dial's dot: dye over 5 times that at the mirror point for a handle of
  its own, and a volume write on the page puffs at the dot (peak over 1.3 times the resting one).
  Proved by `node/sources.mjs` "dialDot" and "stepPuff: at the dot, along the tangent times the
  direction", `node/director.mjs` "splats: ... a step puffs at the dial's dot", and `sources.py`.
  Mutant: `node/mutate_director.sh` ("puff: not at the dot").
- The stir is bounded: repeated hard strokes over one place for 6 s leave every field finite, the
  velocity under 75 and the dye under 1. Proved by `sources.py`. Mutant: `mutate_sources.py`
  "pointer: no maxPush cap".
- No stray puff on a load, a device switch or a reconnect, and the pointer's listeners never
  interfere with the dial's drag: `sources.py`. Mutant: `mutate_sources.py` ("pointer: listener
  not removed at stop").

## Scrolling

- At rest the resting source has no upward push: `node/model.mjs` "the resting source has no
  upward mean velocity over 600 s (40 seeded runs)" and `scroll.py` (a seeded run of 12 s, the
  cloud's centre not above its source). Mutants: `node/mutate_model.sh` (the two `restDrift`
  mutants).
- The dye a scroll moves out of view, half a viewport each way, comes back on the reverse scroll:
  after a scroll down that cuts the dial's cloud and back up, no step across the former cut line
  beyond 2 times between rows, the rows above it hold 0.4 of their earlier density and no row is
  denser than the cloud was. Beyond the hold, after a scroll of 600 px against 350, the strip is
  empty (under 0.01 of dye). Proved by `scroll.py` `smear` (both browsers). Mutants:
  `mutate_scroll.py` "scrolled-out dye is dropped", "the strip beyond the hold copies the edge".
- A scroll burst stirs and nothing stirs once it falls under `scrollFloor`: `node/director.mjs`
  "scroll: ..." and `node/model.mjs` "scrollStir: ...".

## Levels, loads and the layout

- Every load settles before it is read, with no error, no warning and no horizontal scroll, on the
  DAC page, an amp page, a Loki Max page and a lost-link page at three sizes and every level:
  `sweep.py` (each load settles 1.2 s).
- A plan without `velocityLoss` or `vorticity` runs: finite fields, and the splat moves the fluid.
  Proved by `effects_cycle.py`. Mutant: `mutate_registry.py` "the engine needs a plan's
  velocityLoss" (a missing `vorticity` is not proved: an omitted one runs confinement).
- The EQ graph's `color-mix()` background counts as covering the smoke (alpha 0.35 or more), so a
  note is not carved behind a graph or a card. Proved by `node/effects.mjs` "backgroundAlpha ..."
  and `tone_note.py`. Mutant: `mutate_webui.py` "a colour-mix background counts as transparent".
- In Loudness the strength's words (`strength N`) show on the Loudness button, the two round
  buttons are described by them, they change after a + and a -, follow a held + and - while it is
  held, and the label and words fit the button on one line each and clear of the round buttons, at
  1280 x 1100 and 390 x 844; the band table's page position stays within 1 px across Flat, EQ,
  Loudness and Flat at 390 x 844. Proved by `tone_note.py`. Mutants: `mutate_webui.py` "stepOnce
  leaves the Loudness words as they were" and "a held Loudness button is lost when its part is
  swapped"; the Rust view test `loudness_steps_on_its_own_button_and_no_row_follows` proves the
  words appear in Loudness only.
- A click on the volume knob's ring jumps to that step, and the rest of the knob does not; holding its `+` or `-`
  repeats the step (after about 380 ms, then every 70 ms) and the release ends it: `knob_click.py`. Mutants: `mutate_webui.py`.
- A Loki Max's page draws a Presets part of three slots and five built-in settings, and no tone
  part; a built-in's click posts its name to `device-presets/apply` and its lamp lights, as it does
  when another client applies one (the stream redraws the part, and the page makes no request);
  a slot's click posts its number to `device-presets/recall` and a toast says what was sent; the
  save icon opens the question in place, Keep closes it and no request is made, the question stays
  open while the stream sends parts, Save posts `{"slot": 2, "confirm": true}` to
  `device-presets/save`, closes the question, and a recall of that slot brings the saved bands
  back. Proved by `device_presets.py` (both browsers). The server's side (no save without
  `confirm: true`, a guest and a read-only page refused, `controls/recall` a 404) is proved by the
  Rust tests `a_slot_save_without_confirm_true_is_refused_and_sends_nothing`,
  `a_slot_save_needs_writes` and `the_control_route_refuses_an_action_key`.

## The tone curve's maths

- The page's curve maths equals the core's: the 181 frequencies, the summed response of every case
  (flat, boosts, cuts, overlaps, each band's window edges, gains -20 and +6, Q 0.5 and 8, Flat and
  Loudness) within 1e-6, the clipping peak (null in Flat, Loudness and below `PEAK_SHOWN_DB`), the
  peak's words with U+2212, a band's marker point, the viewBox maps and their inverses; the raw
  peak of every case (also those not shown, such as "tiny peak") within 1e-6, including a case
  whose peak lies away from every band's centre ("peak away from every band centre": a low-Q
  boost and the cut above it), which the refinement round the highest sample alone finds; and each
  case's line path number for number within 0.051 (Rust rounds ties to even, the script up). Proved
  by `node/tone-curve.mjs` against `node/fixtures/tone-curve.json`, which the Rust test
  `the_curve_fixture_is_current` keeps current (a stale file fails it, naming
  `SVOLDER_WRITE_FIXTURES=1`). Hand-run mutants, each killed: another sample rate, a refinement
  without the band windows (case "wide boost and a narrow one between samples"), one without the
  window round the highest sample (case "peak away from every band centre"), Flat markers at
  0 dB, a peak shown from 0 dB, a hyphen for the minus, too few golden-section steps.

## The curve's gestures

- A press grabs the nearer band's point within 12 px (a mouse) or 24 px (a finger), the picked band
  on an exact tie and the lower otherwise; a right press or a pinch with none in reach takes the
  band nearest in x, and a left press nothing. A left drag keeps each axis on the press's value
  until the pointer has gone 8 px along it (and then for good); every value is the table's nearest
  step (frequency by log distance), the pointer's map carrying on past the plot's edges (40 px below
  the plot is the lowest gain). In a left drag whose y axis is free the gain is the table's 0 dB
  step while the pointer is within 10 px of the y the plot draws 0 dB at (edges included, 10.5 px
  out is the nearest step), the zone at most the px of 1 dB (6.6 px on a 172 px plot, 2.3 on a 60
  px one), the step wherever the table has it, no zone for a table without one, none for a right
  drag or a pinch (a Q gesture asks for no gain), none while the y axis is held (7 px from the
  press), and moving about inside it writes the one step once, the step written again after a
  detour outside. A right drag sets Q to `q0 x 2^(-dx / 120)`, a pinch to
  `q0 x d0 / d`; the wheel makes one step a notch (a pixel delta of 40 or more, 53 to 120 on a
  mouse, is one step whatever its size; smaller ones, a trackpad's, add up at 40 px a step with the
  rest carried; lines and pages are a step an event). A coordinate that is not a number keeps the
  held step and writes nothing. One write is in flight per parameter and the newest waits; the
  release and a lost release write where the pointer last was, once; Esc and a device switch keep
  what was asked, a lost stream drops the waiting write; a refusal (or a write that never answers,
  or one that arrives after the release) toasts once, ends the drag and writes nothing more; "Mode
  set to EQ" toasts once per drag; `settled` waits for every answer, those sent in turn included,
  also for a write sent in the microtasks after a loop ended (a counter of sends, not the loops); a
  waiting value equal to the one last sent for its parameter is not written again (A in flight, B
  waiting, the pointer back at A); a press let go where it landed writes nothing; the label sits
  above its point and inside the plot at
  every edge. Proved by `node/tone-drag.mjs`. Hand-run mutants, each killed: the lost stream's drop,
  the 8 px edge (`>` for `>=`), the mode toast's once, the tie's picked band, the refusal's once,
  the pinch's ratio inverted, the wheel's direction, an unchanged value written again, a second loop
  started per write, the guard on a step that is not a number; and for the zero zone: the zone
  removed, its half width doubled, its cap of 1 dB removed, the zone applied to a Q gesture, the
  zone ignoring the axis hold, the 0 dB step hardcoded to 98 (the table's zero being elsewhere),
  the edge exclusive, the guard for a table with no 0 dB step taken out.

## The swap hold

- While a point is dragged, and until its writes are answered, the tone part's swap waits in a
  browser with or without `Element.moveBefore`, the release reads it again once, and a drag the
  device did not take puts the part among the swaps that wait (so it is read back from the server);
  a screen change clears what waited; two overlapping holds (a drag begun while the one before waits
  for its answers) keep the part waiting until both have let go, and a refused first drag still
  reads the part back after the second. A part that waited is drawn once, from the newest copy the
  model holds, however many swaps came while it was held, and a use of the page's own (a form open,
  a request unanswered) holds it the same way. Proved by `node/keep.mjs` (it defines
  `Element.moveBefore` for the second case).

- The frame's menu button, menu and picker (`core/frame.js`, `menu.js`, `picker.js`): the button is
  one node over the theme root, on top and not covered, kept displayed and clickable whatever a
  theme's sheet says, at the top right with 16 px insets without an anchor; the menu's rows reach
  the arrows in order (the Theme row, the theme's rows, then Device and Sign out when they apply),
  Escape and `q` close it, and with two devices the Device row opens in place into a radio per
  device; a guest's menu has Sign in. `T` opens the picker (also while the button is faded, which
  brings it back), types in a text field, and the page under the picker gets no key; a pick of
  another theme unmounts the old one and mounts the new one, a theme that throws leaves Ember with
  its popup, and a mute still being written when the theme unmounts leaves no error. Proved by
  `effects_menu.py` ("the_button_is_on_top_and_not_covered", "the_theme_row_opens_the_picker_with_ember_marked",
  "t_opens_the_picker_when_faded", "t_in_the_preset_name_field_types_a_t",
  "a_theme_that_throws_falls_back_to_ember_with_a_toast", "a second theme is mounted ...",
  "and back to Ember ...", "a mute awaiting its write ...", the Device and Sign in rows) and
  `node/frame-keys.mjs` ("t_in_a_text_field_types_and_opens_nothing"). Hand-run mutants, each
  killed (`mutate_effects.py`): the Theme row not opening the picker, Escape not closing it, the
  keys reaching the page under it, T typing into a field, a faded button staying faded, a theme able
  to hide the button, and the old mute's `ember.core` after the unmount.
- A theme's files (sheets, metas, scripts) are in place before it mounts, the fallback included,
  and the other themes' go after: `node/switch.mjs` ("a_first_paint_that_falls_back_loads_embers_...",
  "a_pick_of_ember_from_a_paper_first_paint_...", "a_sheet_that_never_answers_times_out_...", "the_theme_and_the_fallback_both_failing_...")
  and `core-registry.mjs` (`prepare`); in a browser `effects_menu.py` ("a pick of Ember from a page
  without its sheet, meta and htmx loads them ...", "a theme that throws, picked from a page
  without Ember's files, falls back to a styled Ember with htmx ..."). The button's guard holds
  against a later, more specific `!important` rule (`effects_menu.py`), and no theme sheet names
  the frame's parts (`make theme-css-check`). Hand-run mutants, each killed (`mutate_effects.py`):
  the guard not layered or not guarded, the frame after the theme in the page, the scripts or the
  metas not loaded, the fallback not prepared, the focus left where it was.
- A slider's write that waits behind the one in flight goes to the device that was shown when the
  value was asked for, and is dropped when another device is shown or the slider has left the page
  (the held repeat of a stepper of the band editor stops the same way): `node/level-write.mjs`.
  The same words toasted again are one popup with its time restarted: `node/toast.mjs`. In a browser
  without `Element.moveBefore` the Loudness strength's words follow a held round button while the
  Tone part's swap waits: `tone_note.py` (the run named "without moveBefore"), which kills the
  mutant that leaves the words to the swap.

## The curve under the mouse

- A left drag on a band's point (in Flat, which the drag switches to EQ with one toast) changes that
  band's frequency and gain, the point's `cx`/`cy` sit under the release within 1.5 px, the readings
  hold the steps whose words the label showed, the cells show the label's words, the other bands
  stay, the writes carry `raw`, the label goes with the drag and the band is picked in the table; a
  drag straight across (under 8 px of drift) leaves the gain cell and posts no gain write; a right
  drag 120 px right halves the Q (within 0.05 octave), moves nothing else and writes only Q; two
  wheel notches of 40 px up over a point raise its Q raw by 2 and one event of -100 px by exactly 1,
  the page does not scroll there, the wheel away from every point is the page's own and scrolls it;
  a wheel burst whose pointer drifts 18 to 29 px off the point between notches keeps stepping the
  same band's Q (four notches, four steps, every one prevented, no other band stepped, the page
  where it was), a notch 18 px from a point starts a burst and one 30 px away is the page's own,
  the first notch off the point after 700 ms of quiet scrolls, and so does one right after the
  pointer left the plot (a synthetic `pointermove` from outside it) or after a press, each of those
  scrolling the page (and the drift check asserting the page is scrollable); a notch at another
  band's point during band 2's burst steps band 2; a stream of 0.5 px deltas (synthetic events,
  fired at the plot found anew each time, as the part is swapped) held the page for under 700 ms
  after the step, and a Q at 127 does not hold it (run against the page before the change the first
  three fail; against one that renewed its hold on every event the last two do; with the pointer-
  leave and press resets taken out of the page two fail; the graph's reach is also held by
  `node/tone-drag.mjs`);
  a guest page grabs nothing and posts nothing. In Chromium only (a press held across calls): a
  write posted from outside during a held drag does not replace the graph (the same figure and
  marker nodes), its value is in the cell after the release, the drag lands under the release, and a
  write of the dragged band's own Q from outside shows in its cells and readings after the release;
  a device that goes away mid-drag toasts once, posts nothing after, leaves no label, and the tone
  part then shows what a fresh read of it from the server shows. In Chromium with the page's writes
  held (a wrapper of `fetch` that parks the band writes until the check lets them go): a wheel
  notch over a band just let go of leaves the drag's readings with the wheel's Q (every write of
  `data-readings` carries it) and the device ends on it; a press right after a release starts a
  drag (its label shows) whose point ends under its release, the page and the device agreeing on
  the band; a notch inside a burst steps from a Q the readings carry that the burst does not know
  (put in by the page, the stream's echo being slower than the burst's 700 ms), and one after a
  right drag inside the window steps from the drag's Q, the notch's write following the drag's
  last; a second drag's single write on a parameter the first still had waiting goes after it (the
  last frequency write is the second drag's and the device has it); a press that moves nothing
  after a drag leaves the drag's values in the readings; a wheel notch the device refuses (the page
  answers a 409) leaves the part read again with the device's Q. Run against the page before the
  change these fail 8 checks. A press within 100 ms of a tone
  swap, while its curve is still easing, stands: the drawn line and its `data-y` agree after the
  tween's own timer has run (both browsers; synthetic pointer events fired from the swap's own
  event). Proved by `tone_drag.py` (both browsers, but for the Chromium-only parts). The band 3 drag
  is 40 px, not 60: that band's window ends 53 px from where it starts.

## The curve under a finger

- In Chromium under mobile and touch emulation (390 x 844, CDP touch events; headless Firefox does
  not pan on touch): a finger put down on a point and dragged moves that band (frequency and gain
  change, the point sits under the finger's last place within 1.5 px, the others stay, the band is
  picked, only frequency and gain are written, every touch start and move was prevented) and the
  page does not scroll; a finger put down on the graph away from every point scrolls the page,
  writes nothing and has no touch prevented; with two points between 12 and 40 px apart, a touch
  within 24 px of both grabs the nearer, on either side; two fingers moving to twice their distance
  halve the picked band's Q (within 0.1 octave), writing only Q, with the page's
  `visualViewport.scale` still 1 and `scrollY` unchanged; with no band picked the pinch acts on the
  band nearest the fingers' midpoint (the midpoint put well away from every point), picks it and
  leaves the others; a second finger put down while the first holds a point pinches that point's
  band and leaves its frequency and gain; right after a drag's release, with its writes still
  unanswered (the check holds them 3 s), a finger put down away from every point scrolls the page
  and has no touch prevented; a `touchcancel` mid-drag ends the drag (no label), the band keeps
  where the finger got to, the swap that waited (an outside write) lands once the writes are
  answered, and the next touch on a point drags again; a third finger put down during a pinch
  leaves the drag going, lifting it ends and writes nothing, and the pinch carries on to halve the
  Q with the others unchanged and the page neither zoomed nor scrolled. Proved by `tone_touch.py`;
  the same script run against the page before the touch code fails 14 of its first 29 checks (the
  pinch zooms the page and nothing drags). Hand-run mutants, each killed: no `touchcancel`
  listener (3 checks), a lifted finger that is not one of the drag's ends the drag (2 checks). A
  touch on a point right after a touch drag's release, its writes held 2 s, starts a drag whose
  point ends under the finger (the same script on the page before the writes waited fails it).

## The visualizer's rules

- The base scale runs through the design's five stops and the tint leans cyan where the left
  channel is louder and orange where the right is, by the gap times the loudness; equal channels
  take no tint; silence is the base colour of level 0 and transparent. Proved by `node/viz.mjs`
  "base_scale_matches_the_design_stops", "tint_is_cyan_when_left_leads_and_orange_when_right_leads",
  "equal_channels_get_no_tint", "silence_draws_base_zero" and
  "a_spectrogram_point_is_transparent_in_silence_and_opaque_by_a_quarter_scale".
- The visualizer card and Now playing take the tone panel's translucent ground from one token and
  the canvas is never opaque. Proved by `node/viz.mjs`
  "the_cards_are_translucent_alike_and_the_canvas_paints_no_ground" and `visualizer.py` "Now
  playing and the visualizer are translucent ...".
- A browser with nothing kept starts on lumiBars soft with the graph off, a tap goes from it to the
  spectrogram, and the Device fold-out is folded. Proved by `visualizer.py` "a browser with nothing
  kept starts ...", "a tap goes from soft ..." and "the Device fold-out is folded ...".
- A frame decodes from the wire shape, and what is not a whole frame decodes to nothing. Proved by
  `node/viz.mjs` "a_frame_decodes_from_the_wire_shape".
- The curves keep 0.4 of the last frame per frame interval; a peak is held 2 s and then falls; with
  no frame for a second the picture falls as in silence, at the pace of a draw (not in one step);
  each band holds and falls on its own. Proved by `node/viz.mjs` "smoothing_...",
  "max_hold_keeps_a_peak_2_s_then_falls", "each_band_holds_and_falls_on_its_own",
  "with_no_frame_for_a_second_..." and "a_stalled_stream_falls_at_the_normal_rate_...".
- Of the frames waiting for a draw only the newest is drawn, a late one alone is drawn, one older
  than the newest seen is refused. Proved by `node/viz.mjs`
  "a_late_frame_is_dropped_only_when_a_newer_one_arrived".
- The clock offset comes from `hello` and no frame moves it; the delay is the median of the last
  2 s. Proved by `node/viz.mjs` "the_clock_offset_comes_from_hello_..." and
  "the_delay_is_the_median_of_the_last_2_s".
- A beat's pulse is drawn on the beat, scheduled ahead of the frames, behind a delay of 0, 60, 150
  or 400 ms; a beat frame that arrives late is not pulsed twice; without a tempo a flag pulses once;
  a stream that stops or a tempo that clears ends the pulses. Proved by `node/viz.mjs`
  "beats_land_on_time_with_a_steady_150_ms_delay",
  "a_beat_frame_that_arrives_late_is_not_pulsed_twice",
  "without_a_tempo_a_beat_flag_pulses_once_...", "the_grid_stops_when_...",
  "after_a_long_gap_the_next_beat_is_found_at_once" (a gap of 60 s or a million beats costs one
  call) and "a_flag_without_a_tempo_is_dropped_when_late_...". Hand-run mutants, each killed: the
  grid ignoring the beat last pulsed, a pulse at the frame's arrival, no skip of a beat already
  late, the catch-up bounded to 8 beats, flags that a frame without one clears or that are not
  checked for lateness.
- The bands map to pixels on a log axis from 20 Hz to 20 kHz and back. Proved by `node/viz.mjs`
  "bands_map_to_pixels_on_a_log_axis_from_20_hz_to_20_khz".

## Now playing

- The elapsed time is the player's position plus the time since it was read times its rate while it
  plays, the position as it was while it does not, never below 0 or past the length (the fake
  player runs on past it). Proved by `node/playing.mjs`
  "progress_ticks_from_position_rate_and_read_time",
  "progress_does_not_move_while_paused_or_stopped" and
  "progress_is_clamped_to_zero_and_the_length"; in the page by `playing.py` "the time counts
  forward while it plays", "the time stops at the length, the bar full" and "a paused track does
  not count".
- The time is worked out against the daemon's clock, whatever the browser's. Proved by
  `node/playing.mjs` "progress_uses_the_daemon_clock_offset" (a browser behind and one ahead give
  the same time).
- The time reads `m:ss`, `h:mm:ss` from an hour, seconds rounded down; the bar is empty without a
  length. Proved by `node/playing.mjs` "times_read_m_ss_and_h_mm_ss" and "the_bar_fills_by_...".
- A hidden tab counts nothing and counts again when shown. Proved by `playing.py` "a hidden tab does
  not count; it counts again when shown".
- The card shows the fake track with its cover (asked for by hash) and the path `untouched` for the
  fake route on the fake DAC; on a desktop the band stands above the two columns and shows the
  whole card. Proved by `playing.py` "the card shows the fake title ...", "the cover is the
  daemon's ...", "the band stands above the two columns ...", "on a desktop the whole card shows"
  and "the path says untouched ...".
- A press posts `api/v1/host/press` with the button named and the card follows the answer. Proved
  by `playing.py` "pause posts api/v1/host/press ..." and "the card follows the answer ..."; the
  route's gates (guest 401, writes off 403), the cover's ETag, 304 and 404 by `web/host/tests.rs`.
- A guest and a page with writes off see the card without buttons, and post nothing. Proved by
  `playing.py` "a guest sees the card without buttons" and "a page with writes off ...".
- The phone's line is 72 px, above the volume; a tap opens the whole card, a second tap folds it, a
  swap of the card keeps it open. Proved by `playing.py` "on a phone the compact line sits above
  the volume", "a tap opens it into the whole card", "a swap of the card keeps it open" and "a
  second tap folds it again, to the same box".
- The folded line at the user's measured widths (341, 426 and 784 px): 72 px, a round 40 px play and
  a bare next with a 32 × 44 px tap area and no previous, the title clear of the buttons, the
  chevron a button in the bottom-right corner with the bar stopping short of it, a tap on it or on
  the line opening the card, every box back where it was after a fold. Proved by `playing.py`
  "the folded line is 72 px at the page width ...", "a round 40 px play and a bare next ...", "the
  title stays clear of the buttons ...", "the fold arrow is a button in the card's bottom-right
  corner ...", "the thin bar stops short of the arrow ...", "a tap on the arrow opens the card" and
  "a second tap on the arrow folds it, every box back where it was". Mutants (`mutate_webui.py`):
  "the fold arrow stays on the right of the head", "the bar runs under the arrow", "the folded line
  has no next", "the folded next is a round button", "the folded next draws a ring".
- A cut folded title is faded at its right edge in place of "...", and loops for as long as the line
  is folded, the page shows and the title is cut: it rests 2 s at its start, then scrolls left at 30
  px a second, linearly, a copy of the text (`aria-hidden`, not selectable) 40 px behind it, so the
  lap wraps with no jump; it is whole and still, with no copy, when the card is open, on a desktop,
  with no player or when the title fits, and the system's reduced-motion setting does not stop it.
  Proved by `node/playing.mjs` "the_title_loops_at_30_px_a_second_linearly_after_a_two_second_rest"
  and "a_long_title_is_not_sped_up_to_fit_a_time", and in the page by `playing.py` "a long title is
  cut ... and it loops" and "its copy is aria-hidden ..." (at each width), "a new track whose title
  is cut loops ...", "forever (iterations Infinity), linearly ...", "the rest lasts about 2 s ...",
  "at 30 px a second ..." (twice: no easing), "the wrap does not jump ...", "the gap between the
  text and its copy stands in every frame", "at the lap's end the copy stands where the text
  started", "still looping after two laps", "opened, the title is not cut ...", "on a desktop the
  band shows the title whole ..." and "with the system asking for less motion the cut title still
  loops" (also `reduced_motion.py` "the phone's cut title still loops under reduced motion").
  Mutants: "a cut title does not loop", "the loop runs one iteration only", "the loop goes at
  another pace", "the loop is eased instead of linear", "the lap does not rest at its start", "the
  loop has no copy", "the copy does not move with the text", "the copy is not aria-hidden", "the
  copy is a different gap from the lap (the wrap jumps)", "the copy stays when the title fits", "the
  loop stops under reduced motion", "a cut title has no fade".
- The loop's edges: it starts from the first paint (a real daemon's own long title,
  `SVOLDER_FAKE_TITLE`); the page hiding stops it and a track that comes while it is hidden has none
  until the page shows; opening the card stops it and folding starts it again; a track that comes
  while the card is open gets it when the line folds; a player that quits and returns gets it; a tap
  that opens or folds the card acts in the same task; a card that grows till the title fits (no
  redraw of the screen, only the observer) stops it, and back starts it; a resize to a width where
  the title fits stops it, and back starts it; a font that changes the text's width (`loadingdone`)
  fits it again; a same-track swap (also a real press) carries it on where it was and a new track
  starts at its rest; the fade and the clip stand from the style alone, with no "..." on first
  paint. Proved by the `playing.py` checks of those names ("the page hiding stops the loop at once
  ...", "the page showing again starts the loop ...", "the line folding starts the loop ...", "the
  player returning with the same track gets its loop", "a font coming in that makes the text fit
  ...", "the same track swapped in again carries the loop on ...", "the next track ... starts its
  loop from its start", "the fade and the clip come from the style alone ...", "syncing again while
  nothing changed keeps the very same animation going", "a tap that opens the card stops the loop
  ... at once", "a card that grows till the title fits ...", "a real press swaps the part ...").
  Mutants: "the loop goes on while the card is open", "the loop goes on while the page is hidden",
  "a page that shows again does not start the loop", "the page hiding does not stop the loop", "a
  same-track swap restarts the loop at its rest", "a new track takes the old track's loop on", "a
  sync with nothing to change restarts the loop", "folding the line does not start the loop", "a
  resize does not fit the title again", "no re-fit after a font loads", "the fade waits for the
  script to mark the title cut".
- The small motions ignore reduced motion; only the first default level reads it. Proved by
  `reduced_motion.py` (the knob eases, the glow turns, fades at its own speeds and is not pinned,
  the dot pulses, a held peak falls by steps, the tone curve eases and a changed value flashes, Off
  stills the page) and `web/assets/tests.rs` (no `prefers-reduced-motion` in the stylesheet or the
  scripts but `effects.js`); the first default is Glow under reduced motion and a picked level wins
  by `smoke_core.py` and `node/effects.mjs`. Mutants (`mutate_webui.py`): "the knob does not ease
  under reduced motion", "a held peak jumps under reduced motion", "the tone curve does not ease
  under reduced motion", "a changed band value does not flash under reduced motion", "the glow
  stands still under reduced motion", "the glow keeps its slow fade under reduced motion", "a
  catch-all stills the page under reduced motion".
- No reflow: with the cover gone the card's box and the cover's box are where they were and the
  cover's space is the background; with the player gone the card is one quiet line in the same box
  (72 px on a phone), with no buttons and no stale title or art. Proved by `playing.py` "the card's
  box and the cover's box are exactly where they were", "the space is given back to the
  background" and "with the player gone the card is one quiet line ...".
- A changed path stays inside Now playing's half of the band: the verdict chip is first, every chip
  sits inside its box at 820, 1100 and 1280 px, and the band is the same height as for an untouched
  path.
  Proved by `playing.py` "the changed route reads changed ...", "the verdict is the first chip ..."
  and "every chip of the path sits inside the band's box" (the fake route
  `SVOLDER_FAKE_ROUTE=changed`: the player at 91 % and 44.1 kHz, the sink at 48 kHz and 54 %, the
  host EQ, two other sounds).
- A cover that answers 404 is hidden and its box stays; an open phone card whose player quits folds
  to the 72 px quiet line. Proved by `playing.py` "a cover that answers 404 is hidden ..." and "an
  open card whose player quits folds ...".
- An amp page has the same band, and no path. Proved by `playing.py` "an amp page has the same
  band above its columns"; a daemon with no host draws no card by "with no host the page has no
  card at all".
- With the visualizer off a changed path's every chip stands inside the band at its lower fixed
  height (168 px below 1100 px, 188 from there), the verdict first, at 820, 1100 and 1280 px; a path
  that does not fit its card ends in a chip of three dots, the verdict first, the hops cut kept in
  the page and named by the dots, and comes back whole when the card grows, and a card too low even
  for the verdict drops the dots too. The `?` list leaves out its `v` row on a page with no card.
  `node/viz.mjs` ties the head script's storage key to the card's. Proved by `playing.py` "the band
  is 168 px tall" / "188 px tall", "every chip of the path sits inside the band's box (...
  visualizer off)", "a path that does not fit ends in a chip of three dots ...", "the hops put out
  of sight stay in the page ...", "a lower card cuts more ..." and "the row comes back whole when
  the card does". A guest and a page with writes off have a `?` list of `v` and `?`: "and ? opens a
  list of the keys ...". Hand-run mutant, killed: `fitPath` not called.

## The visualizer card

- The card draws the fake host's synthetic music, and a fresh waterfall's oldest columns are the
  transparent (no white first frames). Proved by `visualizer.py` "the card runs once
  shown ...", "a fresh spectrogram's oldest columns ..." and "the card draws: non-blank pixels".
- A tap cycles spectrogram, lumiBars, lumiBars soft and back, and a chip names each for about a
  second; the first run says `tap to change the look` once; `v` cycles, for a guest too. Proved by
  `visualizer.py` "a tap cycles ...", "the chip goes after about a second", "the hint reads ...",
  "v cycles the look" and "a guest's v cycles the look too".
- The corner toggle shows and hides the graph (the channels' curves in their own colours) and
  leaves the look alone. Proved by `visualizer.py` "the graph starts as it was kept", "the corner
  toggle shows the graph ..." and "and hides it again" (the curve colours are counted in the
  canvas's pixels).
- The choice (look, graph, hint seen) is kept in `svolder.viz.2` and is there after a reload; a
  look or graph is kept only once chosen, and the hint keeps only that it was shown; the key an
  earlier card used (`svolder.viz`) is never read. Proved by `visualizer.py` "the choice is kept in
  svolder.viz.2", "after a reload the look and the graph are as left ...", and, on a browser that
  holds an earlier card's `svolder.viz`, "an earlier card's svolder.viz ... is ignored", "the hint
  keeps only that it was shown" and "a tap keeps the look it chose and still no graph".
- A hidden page draws nothing at once, keeps its frames stream for the 2 s grace, closes it after,
  opens one again when shown, and a page shown inside the grace keeps its stream; a card below the
  fold opens no stream until it is scrolled to, and one scrolled away closes it. Proved by
  `visualizer.py` "a hidden page draws nothing at once", "its frames stream is still open inside the
  2 s grace", "and closed after it", "a shown page opens the stream again and draws", "a page shown
  again inside the grace keeps its stream", "a page that starts hidden opens no frames stream", "the
  card is below the fold ..." and "scrolled out of view again ...". Hand-run mutants, each killed:
  a grace of 0 (2 checks), a card that keeps animating when hidden (1 check).
- A page hidden for more than 2 s rests: its device stream and its frames stream are both closed
  after the grace (the daemon then stops reading the route for it: `the_stream_carries_now_playing_
  and_holds_the_route_reads` holds that the stream's end lets go), a rest does not read as a fault
  (no `lost`, the screen not inert, no notice), nothing opens again while it stays hidden, and a
  page shown again opens one stream whose snapshot puts a change made while hidden on screen within
  2 s; a hide and show inside the grace opens and closes nothing; a page loaded hidden opens no
  device stream until it is shown. Proved by `visualizer.py` "a hide and show inside the grace opens
  and closes nothing", "a page hidden for more than 2 s closes its device stream and its frames
  stream", "a rest does not read as a fault ...", "nothing opens again while it stays hidden",
  "a change made while hidden is not on the page yet", "shown again, the change made while hidden is
  on screen within 2 s", "one new device stream was opened ...", "a second rest ...", "a page that
  starts hidden opens no device stream, and is not lost"; a device switch and a history step (back
  and forward) while it rests open no stream, and the page shown after them has one stream, on the
  device it shows, with that device's screen: "a device switch while it rests ...", "shown after a
  device switch ...", "a history step while it rests opens no stream", "shown after a history step
  ...". Hand-run mutants, each killed: a rest that also marks the link lost (1 check), a hide that
  does not mark the page resting (3 checks), a stream opened at a device switch while it rests (5
  checks).
- With no capture possible (`SVOLDER_FAKE_CAPTURE=off`) the card says why in one quiet line, keeps
  every box exactly where it was with the capture on, and draws nothing.
  Proved by `visualizer.py` "with no capture the card says so quietly ..." and "and keeps its box
  exactly where it was ...".
- The card is the band's right half beside Now playing at 820 px (Now playing the larger share) and
  1280 px (halves), 240 px tall, and a 180 px card under the volume (above the controls, below the
  compact Now playing line) on a phone, and under the meters on an amp page; no box moves when it
  starts. Proved by `visualizer.py` "the band holds Now playing on the left ...", "at 1280 px the
  two are halves ...", "nothing moves when the visualizer starts", "at 820 px the card sits ...",
  "on a phone the card sits under the volume ..." and "nothing moves when it starts".
- The Device fold-out shows `Visualizer delay N ms` while the card runs and not after it stops.
  Proved by `visualizer.py` "the Device fold-out shows ..." and "the delay row goes with it"; the
  median itself by `node/viz.mjs`.
- The sink is named (`Playing to <sink>`) on an amp page and not on the DAC's own page; the label
  and the Now playing part's mark are one decision of `view/host.rs`
  (`the_visualizer_names_the_sink_unless_it_is_this_dac`, `the_key_holds_the_sink_label`). Proved
  in the page by `visualizer.py` "a page that shows its own DAC names no sink" and "with the sink
  named ...".
- A device change, a swap of the whole screen and a daemon restart (the device stream sends the
  screen again) while the card runs leave one card, one open frames stream and one animation loop
  (the check counts the page's `EventSource`s and the loop's pending animation frames), and the new
  card draws. Proved by `visualizer.py` "a device change leaves one card ...", "swaps of the screen
  and a daemon restart ...". Hand-run mutant, killed: a new watch made at every swap.
- A frames stream the browser gave up on is asked for again after 3 s, then 6 s (a stand-in stream
  that fails at once). Proved by `visualizer.py` "a stream the browser gave up on is asked for
  again ...".
- A guest sees the card run; with reduced motion it still draws and its chip fades as for anyone.
  Proved by `visualizer.py` "a guest sees the card run and draw" and "with reduced motion it still
  draws".
- On a phone the card is after the volume (or the meters) in the page's order, outside the band,
  and back in the band at a desktop width. Proved by `visualizer.py` "and the card is after the
  volume in the page's order too ...", "back on a desktop width the card is in the band again",
  "and on a phone width again ..." and "and follows them in the page's order too" (amp).
- The effects menu's Visualizer row is a checkbox item shown only where the screen has the band,
  after the levels for the arrows, flipped by Enter or Space, and a level pick is untouched by it.
  Proved by `effects_menu.py` "a page with no Now playing band hides the Visualizer row ...", "a
  page with the band shows it ...", "the arrows reach the switch after the levels ...", "Space on
  the switch turns it off ..." and "Enter on the switch turns it on again". Hand-run mutant,
  killed: the row left out of the arrows' list (1 check).
- Off takes the card out of the page with its loop and its frames stream, is kept as `on`, gives
  Now playing the whole band at 188 px (1280) with the page moved once, survives a reload with the
  page saying so before any script ran and no frames stream ever opened, comes back on, works
  without storage, shows no card on a phone, and leaves `v` doing nothing. Proved by
  `visualizer.py` "by default the visualizer is on ...", "off from the keyboard ...", "the
  choice is kept in svolder.viz.2, the menu closed ...", "Now playing has the whole band, 188 px
  tall at 1280 px", "the page moved once ...", "then nothing moves", "v does nothing with the card
  off", "a reload keeps it off ...", "on again by click ...", "with storage refused the switch
  still turns the card off ...", "with the visualizer off a phone has no card ..." and "an amp page
  has the switch, and off leaves Now playing the band". Hand-run mutants, each killed: the removal
  left out (cards, loops, stream), `viz-pref.js` not read (the first-paint check).
- A woken page's wait leaves the glow in colour (`waking`, not the lost link's grey) while a lost
  link still greys it, and a screen with nothing to read is ready at its first event instead of
  after 5 s. Proved by `visualizer.py` "and the glow stays in colour ...", "a lost link greys the
  glow, as before" and "a woken page on a screen with nothing to read is ready at the first event
  ...".

## The effects hear the frames

- A beat asks the smoke for a puff, from the dial's track at the glow's level and outward, with
  no stir (a beat arms no note); none while away, with the context lost, without a dial or for a
  strength that is not a number; at most `maxPuffs` wait; a stronger beat is a bigger puff. Proved
  by `node/director.mjs` "a_beat_asks_the_smoke_for_a_puff".
- The glow's level is the volume's plus 0.3 of the music's loudness, never over 1, the volume's
  alone without music; a volume change while music plays keeps its share on top; levels that are
  not numbers change nothing; while music plays only a change of the two decimals the glow can
  show writes the level. Proved by `node/effects.mjs` "loudness: ...", "glowLevel: ...",
  "the_glow_follows_the_level" and "music: a loudness that moves less than the glow can show ...".
- A beat's puff leaves the dial at the position the caller gives (the volume's fraction), not at
  the music-lifted glow. Proved by `node/director.mjs` "a_beat_asks_the_smoke_for_a_puff".
- In both browsers, with the card running the glow's `--lvl` is over the volume's and moves, and
  the layer says `hearing`; a hidden page takes it off at once; with no capture the glow is the
  volume's; the smoke makes at least three beat puffs in 14 s of the fake music and none with the
  page hidden or no frames. Proved by `visualizer.py` "with the glow on and the card running ...",
  "a hidden page takes the music off at once ...", "with no capture the glow is the volume's
  level ...", "the smoke puffs on the music's beats ...", "a hidden page makes no more beat puffs"
  and "with no frames the smoke makes no beat puffs".

## The core

The core (`bins/svolderd/web/core/`) holds the page rules that protect a listener whatever theme is
drawn. Each is proved in node, on fakes for the stream, the clock, storage and the system's settings.

- A model replaces the whole page and is heard once; a part event, a reading and a write's parts
  change one device only, a part event carrying the copy it replaced; the stream's model replaces
  a stale first snapshot; a read of the model after a refusal republishes the parts a refused
  write leaves unsettled. Proved by `node/core-store.mjs`.
- The stream: its address (the device, the needs, the followed devices), an error closing it and
  reopening it 2 s later, never by the browser's own retry, and asking the session (a sign-in that
  ended loads the sign-in page, a signed-in page now a guest's reloads); a page hidden past the
  grace rests, wakes waiting (dimmed, inert, the body's `lost` and `waking` classes) until the
  shown device's first reading or 5 s, a screen with nothing to read ready at its model; a device
  switched while the page rests is the one opened on waking, one switched on a live stream replaces
  the stream and no event of the old one reaches the page; a followed device the fleet no longer
  lists is dropped and its events change nothing. Proved by `node/core-stream.mjs`.
- The actions: each posts today's request; a guest's call sends nothing and offers Sign in, a
  read-only page's says why; a refusal is toasted with the daemon's words and the model read again;
  the answer's toast is said and its parts applied; the volume is written one at a time with the
  newest waiting, held to the cap, a raise dropped while the stream is lost or waking and a
  lowering always sent; a device that cannot be shown stays and says so; the address is pushed.
  Proved by `node/core-actions.mjs`.
- Preferences under a theme's own key, storage that refuses read as nothing, the choice written to
  storage and cookie together, light or dark following the system until a person overrides it and
  ignoring it for a theme with one scheme: `node/core-prefs.mjs`. A theme that throws on mount or
  cannot run falls back to Ember with a toast: `node/core-registry.mjs`. The climb's popup once a
  wording for 6 s, a climb a touch ended that stays ended until a reading says another target, and the
  tone mode's note on a change into Flat or Loudness or the person's own click within 8 s:
  `node/core-climb.mjs`. The canvas loop (on show only, a long gap held to 0.1 s,
  one still frame for reduced motion) and the analysis frames (one stream for every holder, a late
  holder told what was said, a doubling wait): `node/core-loop.mjs`. A loop may keep running under
  reduced motion (`reduced: 'theme'`, the theme reads `loop.reduced`; the default `'still'` is as
  before) and draw at a pace (a timer for 100 ms or more, then one frame; every frame below; `dt`
  held to twice the pace, never under 0.1 s; nothing drawn while hidden; a pace that changes
  between draws leaves one timer or frame outstanding and draws at the new rate):
  `node/core-loop.mjs`. The analysis rules (`core/viz-model.js`) are served from the core
  for any theme: `the_analysis_rules_are_served_from_the_core`, tested by `node/viz.mjs`.
- The tab's title, for every theme: `<device> · connected — svolder`, `reconnecting` while the
  stream is lost, `reading again`, `resting`, a card's title in lower case for a device out of reach
  and `fault` for a fault (the stream's state first, then the card), plain `svolder` with no
  device; a part another piece adds follows the status and goes when removed, one that throws is left
  out (`node/core-title.mjs`). Proved on Ember, Paper, Signal and Night drive, connected, with the daemon gone and
  back, and after a device switch, by `core_rows.py` (both browsers).
- The held `−` and `+`, for every theme: one step at the press, after 380 ms a step every 70 ms; it
  follows the button of the pressed one's id through a redraw, and ends at the release or cancel of
  that pointer anywhere, a move of it with no button down, the window's blur, the page hidden, the
  hold no longer allowed, no button left, 10 s of repeats, the signal's abort, `end()` (with `leave`
  also the pointer leaving the button or the button losing the capture); a viewer who may not write
  gets one step, no repeat; a context menu over the held button is prevented; `onStart`/`onEnd`
  bracket it once (`node/core-hold.mjs`).

- The frame's Space key plays and pauses the computer's player, in every theme (`space_key.py`, both browsers):
  `space_on_the_body_posts_one_press_and_does_not_scroll`, Space on a focused button activates it and posts no press,
  an open menu owns it, a text field types it, `space_with_no_player_posts_nothing`; the key's rule and the button
  chosen in `node/frame-keys.mjs` (`space_plays_and_pauses_unless_the_focus_owns_it`,
  `space_presses_the_button_the_players_state_calls_for_and_nothing_without_a_player`). Hand-run mutant, killed: the
  key not prevented (the page scrolls).
- The theme picker's close button (`effects_menu.py`, both browsers): `the_picker_close_button_is_44_px_inside_the_window`
  at 1280x800, 412x915 and 360x780 in all four themes, and a click on it closes the picker with the focus back on the
  menu button.

## Ember's drawing

Ember (`bins/svolderd/web/themes/ember/draw/`) draws the page's parts from the JSON model in the
browser, and what it draws is the markup the templates give today: the page's styles and scripts
read the same DOM.

- The link is a dot after the shown device's name (the part is `#part-link`, drawn in the switcher by
  the shell and again by the `link` part): `node/ember-parts.mjs` "the_link_dot_follows_the_shown_devices_name_for_everyone"
  and "a_guests_sign_in_shares_the_side_with_the_slot_and_the_link_is_beside_the_name" against the frozen
  fixture; in the browser `ember_link.py` ("the_link_is_a_green_dot_beside_the_name",
  "the_dot_is_named_and_not_focusable", "trouble_shows_its_word_and_changes_the_dot", both browsers,
  a desktop and a phone), and `effects_menu.py` "the button stays where it is when the link changes".
- The link, card, controls (every kind of control: segments, list, switch, slider, readout; inert
  for a guest and for a read-only page), power (the amp's with its switches and readouts, a
  preamp's alone, the hidden one), meters (32 segments an arch, the class-A and red shares, lit by
  the reading, the state line, the hidden one), volume (the dial's 49 ticks, labels, arcs and
  handle, the readout with its minus set apart, the notes, the hidden one), tone (the mode track
  with the loudness strength's round - and +, the curve's line, area, ghost, peak and a point per
  band, the band table faint in Flat), bands, presets (the status lines, the user's own presets
  with rename and delete, save, a configuration that cannot be written) and device presets (the
  slots with save, the built-in settings, a guest's inert ones), facts (the product known or not,
  the unit, the DSP build, the passphrase note, an advertised and a live signal, readings with and
  without a stamp, Identify offered and not) and Now playing (a track with its cover, length and
  position, a bare one, a paused one, the transport for a writer and none for a guest, the path
  with its hops, with none and not read yet, the sink named or not) parts, the screen in the
  default order and in a kept one, and the page's shell (one device, two with the switcher, a name
  with a space and a slash, a guest, a read-only page, a token that signs out, no device) of each
  of 45 cases (a DAC
  as a signed-in viewer, a guest and a read-only one, in Flat, in Loudness with and without a
  strength and seen by a guest, climbing under a cap and a risk, above the risk step, muted, at
  silence and unread, with a band not read, a preset of the user's under a name that needs
  escaping, recalled, a preset edited after it was recalled, a configuration that cannot be
  written; a lost link, a device not found under a name that needs escaping, a refused passphrase,
  two faults; an amp on, in standby, warming up and seen by a guest, five meters, one; a Loki Max,
  also seen by a guest and with a built-in setting lit; a Saga 2 without a mute control; a phono
  stage, also seen by a guest; Now playing with an untouched route, a changed one, no player, a
  bare track on an amp, a guest's track on another sink and a route not read; a DAC with its
  fold-out full and one with a live signal, no Identify and a token; an amp whose passphrase was
  refused; two devices, one named with a space and a slash; no device; a device that cannot be
  read) are held to the golden copy of the askama templates' DOM for the same models, compared tag
  by tag: a changed attribute or text, a missing space between two tags, a number off by more than
  0.011 where the drawing computes geometry, or a missing or extra attribute is a difference. A
  page with no device draws its card as the screen holds it. Proved by `node/ember-parts.mjs`
  against `node/fixtures/ember-parts.json`, that golden copy (no test writes it; it is not
  rewritten); the harness first checks its comparison
  (`the_comparison_catches_a_changed_attribute`, `..._a_missing_space_between_tags`,
  `..._a_number_off_by_0_02`, `..._passes_reordered_attributes_and_hx_post`) and the geometry: the
  arch (`the_arch_has_its_segments_in_their_classes`), the dial
  (`the_dial_maps_a_step_to_its_angle_and_place`, `a_label_too_close_to_another_is_dropped`,
  `the_gungnir_dial_reads_its_labels_at_their_true_places`,
  `the_risk_the_cap_and_a_climb_shape_the_knob`) and the curve
  (`the_points_sit_at_each_bands_own_gain`, `a_band_not_read_in_full_draws_no_point`,
  `the_curve_draws_the_line_the_area_and_the_ghost_for_each_mode`), the data blocks
  (`the_comparison_reads_a_data_block_as_json`,
  `the_strip_and_the_tone_steps_are_data_blocks_that_cannot_close_their_script`), the visualizer
  (`the_visualizer_card_is_the_same_on_every_screen_that_has_one`), the ids
  (`a_device_name_with_a_space_and_a_slash_makes_css_identifiers`) and the order
  (`the_default_order_is_the_layout_the_page_has_today`,
  `a_kept_order_goes_first_and_what_it_misses_goes_back_to_its_zone`,
  `a_screen_drawn_in_a_kept_order_holds_the_same_parts_in_it`,
  `every_screen_draws_the_sections_of_its_default_order_in_order`,
  `an_order_that_names_what_ember_lacks_or_is_not_a_list_cannot_break_the_screen`). Hand-run mutants, each killed:
  a changed heading, the space before Power's word removed, an arch's centre moved by 1, the space
  before a preset's button closes removed, the strength's span renamed, the guest's round - and +
  left enabled, the mute's empty fallback dropped, the notes' room always on.

## Arrange mode (Ember)

- The order of Ember's sections (`themes/ember/order.js`): the default is today's layout; a kept
  order drops ids the layout lacks and puts a new one at the end of its default zone; the arrows
  move a section within its zone and to the zone before or after it at the same place, stop at an
  edge and step over a section the screen does not draw; a section is placed by index; an order kept
  for a DAC is not an amp's. Proved by `node/ember-order.mjs` ("the_default_order_is_todays_layout",
  "a_kept_order_drops_unknown_ids_and_appends_new_ones_to_their_default_zone",
  "arrows_move_within_and_between_zones", "move_to_inserts_at_the_index",
  "an_order_kept_for_a_dac_is_not_an_amps"); the screen drawn in a kept order is marked `.arranged`
  only when what shows is not the default, and a screen with no player draws no band unless another
  section is in it (`node/ember-parts.mjs`
  "a_screen_in_an_arranged_order_is_marked_and_the_default_is_not").
- The mode itself (`arrange.js`, `sections.js`), in both browsers by `arrange.py`
  ("the_default_order_is_todays_dom": no `.arranged`, today's DOM, the screen the default order
  draws; "every_movable_section_in_every_zone_keeps_the_page_working": each section in each zone,
  applied and kept, and a control write and a volume write show within 2 s;
  "the_keyboard_moves_a_section": `A` starts it with a handle focused and every section's children
  inert, the arrows move a section and the live region says where it stands, none of it writes to
  the device; "a_drag_by_the_handle_moves_tone_into_the_band": a mouse drag (a finger drag too, in
  Chromium) lifts the section, leaves its ghost, shows the drop line and lands between the two
  sections the line was between, and Done keeps the order for the layout and nothing else;
  "reset_puts_the_default_back" (kept by Done) and "escape_restores_the_order_from_before" (nothing
  stored); "the_order_survives_a_reload_and_a_screen_swap": a reload, a switch to the amp and back,
  and a screen that changes shape (the device goes away and comes back), which also ends a mode that
  was on, with a popup; "a_dac_order_does_not_leak_to_the_amp";
  "while_arranging_the_knob_and_the_curve_are_inert": a drag on the dial, on the curve and a click
  on a switch post nothing, the arrow keys write nothing, and they write again after Done;
  "on_a_phone_the_order_reads_band_left_right": 390 px, also while arranging, and the default is as
  it was after a cancel; "t_opens_the_picker_while_arranging": `T` with a handle focused opens the
  picker and leaves the order alone). Also: a part swapped into a moved section lands there by id
  and gets its handle (and the focus, if its handle had it); a theme that unmounts mid-arrange
  leaves no mark, no `inert` and the stored order as it was; the frame's row and the key `A` (not
  typed in a field: `effects_menu.py`); the Visualizer switch puts the card back where the
  arrangement in progress has it (not the kept order), and Done keeps it there; the glow follows a
  moved dial, also by a move that leaves the page as tall as it was and so resizes nothing (the
  layer is told by the mode); a device whose link is lost cannot be arranged (`A` does nothing, the
  row is not offered); a drag whose pointer capture fails still ends where the button is let go;
  three sections in the band share one row at 1280 px and Now playing's title is not cut and its
  previous-track button stays shown, clickable and sending its request in a narrow cell (1280 px,
  900 px) while a wide cell has the whole card; a browser that refuses storage keeps the page whole
  and says the order is not kept; the theme lists its `sections`. Hand-run mutants, each killed
  (`mutate_effects.py`, the entries "arrange: ..."): the keys that write reaching a page being
  arranged, the children not made inert, Escape or an unmount keeping the order, a swapped section
  or handle not given its handle or the focus again, Reset not moving the sections, the default kept
  as an order, one order for every layout, a drag landing nowhere, the visualizer placed on an
  arranged screen, the phone's `order` rules reaching an arranged screen, the menu row offered while
  arranging, the layer not told after a move, a drag that ends only over the screen, the switch
  seating the card by the kept order, a locked device offered by the row or arranged by the mode, a narrow Now playing cell losing its
  previous button.

## The theme contract and the core's own rows

- The conventions every theme keeps (`docs/design/web-themes.md`, "What every theme does the same way"):
  the menu button in the top-right corner of every theme and nothing right of it in Ember's header
  (`frame_button.py`, both browsers: "the_menu_button_is_in_the_top_right_corner_in_every_theme",
  "ember_keeps_nothing_right_of_the_button"); the cover of the track is a fact of `factsOf(device)`
  that Ember, Paper and Signal show (`node/contract.mjs` "facts_name_what_the_device_section_shows",
  `completeness.py` "the cover is a fact" and "all N facts are reached and shown with the model's
  words"; Night drive's driver reads them from the dash and the sheet).

Every theme presents everything a device offers, on every kind of device, and shows every fact of
the Device section (design 4.6). `core/contract.js` lists what that is from a device's model, and a
theme's test driver (`theme.driver`: `reach`, `operate`, `shown`, `state`) is what the checks use
to prove it. They judge what the fake daemon holds and what the theme shows, never pixels.

- The model's controls are of kinds the core knows (`choice`, `toggle`, `slider`, `readout` and the
  volume, power, tone, presets and device presets objects), `targetsOf` and `factsOf` list every
  control, action and fact of every device kind in the fixture, each target maps to one action
  request with today's path and body (a readout none, a slot's save and recall one each), a followed
  device's targets carry its name, a guest's send nothing and say why. Proved by `node/contract.mjs`
  ("every_control_has_a_kind_the_core_knows", "every_target_maps_to_one_action_request",
  "a_followed_devices_targets_carry_its_name", and the lists' own tests). Mutants
  (`mutate_effects.py`, the entries "contract: ..."): a band without its Q, the Signal fact or the
  unit not listed, the transport's pause and play swapped, the slots not listed.
- One short pass for each theme the page lists, through its driver: "volume_up_and_down",
  "mute", "a_band_change" (band 2's gain), "a_preset_applied" (Warm), "the_device_lost_and_back"
  (the device dropped on the check's cue, the daemon restarted), "a_refused_write" (a step sent to a
  device that went out of reach while the page's stream was closed: the daemon's words are shown and
  nothing changes), "resting_and_waking" (a change made while the page rests shows within 2 s of its
  waking), "the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade",
  "theme_query_switches_and_is_kept" (`?theme=` and its cookie) and the guest's pass (no request,
  "Sign in to change this"). The core's members are pinned (`core` has these seventeen and no
  others), and the menu button is displayed, visible, opaque, at least 24 px, on top at its centre
  and inside the window for every theme, every device kind and at 1280 x 900 and 390 x 844 (the page
  scrolled to its top: Ember's button sits in its header). Proved by `contract.py` in both browsers.
  Mutants: the core losing a member, a step down that raises, a band step down that steps up, the
  lost card not the lost state, the toasts never shown, the page never resting, the volume shown
  as its number only, the button hidden or too small.
- Completeness: every `targetsOf` target of a Gungnir 2 with a player, a fake amp with a signal, a
  fake five-meter amp, a fake mono amp and a Loki Max is reached and operated, and the model route
  reports the change (a readout is shown with the model's words; a slot's save is proved by what a
  recall brings back; each operation is one write request); every `factsOf` fact is reached and
  shown with the model's words (a reading that moves is held to its shape). Proved by
  `completeness.py` (Chromium). Mutants: the Device fold-out not opened, a slot save that does not
  answer its question, the loudness steppers swapped.
- The rows Ember does not use: a second device followed over one stream (`stream.follow`), a hop
  added from the page (in the model within 1.5 s and in the config file), the menu's fade and its
  return on a pointer move, a press, a touch or a key, a band moved from a 400 x 100 map by
  `drag.createDrag` (the steps at the daemon are those of the target Hz and dB), light and dark
  following the system with the person's word held and kept as `probe.light` (Chromium), a canvas
  loop that stops while the page is hidden and starts again, and Ember unmounted and mounted again
  (no canvas, no layer left, one write for one key press, no error), and Ember mounted while the
  stream is lost (the screen inert, the smoke grey, both back with the stream). A fleet that no
  longer lists the device shown moves the page to the first device listed or to the empty page
  (`node/core-stream.mjs`). Proved by `core_rows.py`
  (both browsers). Mutants: the button never fading, a followed device not asked for, a hop added
  by the edit route, the scheme not on the root or not kept in the cookie, the loop ignoring a
  hidden page, the drag's gain read from the x, Ember's listeners outliving its mount.
- `tone_drag.py` and `effects_menu.py` drop their device by `Daemon.drop()` (the fake watches a
  file, `SVOLDER_FAKE_GONE_FILE`) and assert it was connected before; `tone_note.py` brings the Tone
  pane into view before each click and waits for what it measures.

## Paper

- The cover in Now playing, in monochrome (`paper.py`, both browsers): "the_cover_shows_in_monochrome" (the
  host's picture, 56 px, left of the words, a same-origin address with the version, no style
  attribute; the computed `filter` of the picture holds `grayscale(1)` in light paper and in dark
  paper), "the_cover_follows_the_track" (another track's own address; a real 404 hides the box and the
  words move to the section's left; a track with no cover has no box; the first picture comes back),
  "no_cover_leaves_no_gap" (the daemon stops holding it, `SVOLDER_FAKE_COVER_GONE_AFTER_MS`),
  "the_cover_shows_on_a_phone" (inside the window, no sideways scroll). Each killed by a mutant made by
  hand once: no cover drawn, no filter, no dark filter, no hidden handler.

The theme Paper (design section 7), built on the core alone. Its tokens, its words and its sheet are
proved by node harnesses; its light and dark and its first frame by `paper.py` in both browsers.

- Registration: Paper is the second theme, light and dark paper, with its browser colour for each;
  a cookie that says dark paper paints dark paper with Paper's sheet and one theme colour in the
  first byte. Proved by the Rust tests `paper_is_listed_in_light_and_dark`,
  `the_list_is_the_pickers_and_the_registrys_data` and `a_paper_dark_cookie_paints_dark_paper_first`.
  The picker lists Ember and Paper and Paper's card has its name, its blurb and a picture
  (`effects_menu.py`, `paper.py` "paper_mounts_from_the_address_and_from_the_picker").
- Tokens: every text colour reads at 4.5 or more on its sheet in both papers (the frame's ink, dim
  and accent also on the menu's surface and hover, its on-accent colour on its accent), the system's
  dark block and the person's set the same values, both papers dress every colour token the frame
  declares, and each value is the planned one. Proved by `node/paper-tokens.mjs`
  ("every_text_token_reads_at_aa_on_its_sheet", "the_two_dark_blocks_agree",
  "both_papers_set_every_frame_colour_token", "the_tokens_hold_the_planned_values"), and in the
  browser by "contrast_in_light_and_dark" (the computed colour of the subtitle, the title, a label,
  the footer, a note, a red note and the volume's words on the body's background).
- Words: the title says the kind, link, input and rate (the rate from the readout, else from the
  signal path's DAC hop), a guest's and a read-only page's title say so, the footer dates by day and
  month, and every fixed word is written in characters the faces carry (no arrow glyph: a note's arrow is a
  drawn stroke and the word `to`). Proved by `node/paper-words.mjs` ("the_title_says_kind_link_input_and_rate",
  "a_guest_and_a_read_only_page_say_so_in_the_title", "the_date_is_day_and_month",
  "paper_writes_only_the_characters_its_fonts_carry").
- The sheet: the skeleton holds every section's element, the title, card and footer draw from every
  fixture page with no `style` attribute and every word escaped; the state rewrites the title's line
  (`Reconnecting to svolder...`, `reading the device again...`) and fades or stills the sheet. Proved
  by `node/paper-live.mjs` ("the_skeleton_holds_every_section_in_its_place",
  "the_title_names_the_device_its_line_and_the_slot_for_the_button",
  "the_state_rewrites_the_titles_line", "no_markup_carries_a_style_attribute_and_every_word_is_escaped",
  "the_state_fades_or_stills_the_sheet_and_rewrites_the_title").
- Keeping the sheet current: a part draws only the sections that show it, a model or the shown
  device replaced draws all, another device's part nothing, a held section waits and is drawn once
  from the newest model when released (Review Focus 2), readings are heard until the mount ends. Proved
  by `node/paper-live.mjs` ("a_part_draws_only_the_sections_that_show_it",
  "a_held_section_waits_and_is_drawn_from_the_newest_model_when_released",
  "readings_are_heard_until_the_signal_aborts", "a_stopped_sheet_listens_to_nothing"). Mutant (run
  by hand): the stale mark of a held section not kept, killed by the held-section test.
- The log of what the page asked keeps the last fifty with the time asked and answered and whether
  the answer was good; marks are put in place through the style object. Proved by
  `node/paper-live.mjs` ("a_change_this_page_asked_for_is_logged_with_its_answer",
  "the_log_keeps_the_last_fifty", "place_sets_marks_through_the_style_object").
- The menu button is the ink circle over the title's slot, at 1280 and at 390 px; the menu offers
  Auto, Light and Dark as radios (Auto checked), a choice paints at once, is kept in the cookie and
  moves the browser's colours; Auto follows the system (Chromium's emulated setting, a Firefox profile
  that says dark). Proved by `paper.py` ("the_button_is_the_ink_circle_over_the_slot",
  "the_menu_offers_auto_light_and_dark_and_keeps_the_choice", "auto_follows_the_system").
- Dark paper from the first byte (Review Focus 1): dark chosen, a switch to Ember and back by the
  picker, a reload: the served page carries `data-scheme="dark"` and its theme colour, and a
  `MutationObserver` set before the module ran never sees the scheme become light. Proved by
  `paper.py` "dark_paper_is_painted_dark_from_the_first_byte".
- No sideways scroll at 390 px in light and dark paper (the sheet so far). Proved by `paper.py`
  "no_sideways_scroll_at_390".
- Now playing: the track in italics with its artist, `back`, `pause` or `play` and `next` as written
  words only for a viewer who has the buttons (a guest on another sink has none), `nothing playing`
  with no player; a tap posts the host's press and the host's status is what the words follow. Proved
  by `node/paper-draw.mjs` ("the_transport_is_drawn_for_a_writer_only") and `paper.py`
  ("the_transport_words_press"; the fake player answers play and pause only, so `next` is proved
  by its post).
- The amp: `Power: on` and its word while it warms, the power and its switches written, and one
  small printed scale per meter (class-A share hatched, a pencil needle at the meter's fraction, its
  reading and the state line beside), the needles moved in place by a reading of the meters; the
  readouts of the power are not printed as lines. Proved by `node/paper-draw.mjs`
  ("the_amp_draws_power_and_a_needle_per_meter": amp, five, mono) and `paper.py`
  ("an_amp_shows_power_meters_and_its_readouts": needle within 0.05 of the model's fraction in the same
  tick, power off and on written).
- The device's facts: the spec table (product, unit when shown, firmware, DSP, profile, link, signal,
  passphrase when noted), every reading with the age it was heard (kept current while the page is on
  show) and `identify` with its words. Proved by `node/paper-draw.mjs` ("every_fact_is_on_the_sheet",
  every fixture) and `paper.py` ("the_spec_table_lists_every_fact").
- The card and the states: a lost link or a fault is the red note under the title (`p.pn.red.card`),
  its words in print, the faults listed with their advice, `try now` and the seconds to the next try;
  a page with no device draws only the card; a stopped daemon reads `Reconnecting to svolder...` and
  stills the sheet, a hidden page shown again reads `reading the device again...`. Proved by
  `node/paper-draw.mjs` ("a_lost_link_draws_the_card_as_the_red_note_with_try_now",
  "faults_are_listed_under_the_red_note", "no_device_draws_only_the_card") and `paper.py`
  ("the_states_in_papers_words": a device taken off the air, `try now` posts connect, the daemon
  stopped and restarted, hidden 3 s then shown; the red note reads at AA in both papers).
- Another device is one tap away: `also here: Amp` shows the amp at `?device=Amp`. Proved by `paper.py`
  "a_second_device_is_one_tap_away".
- Review Focus 4, long words at 390 px: a DAC with nine presets (one 40 characters unbroken), a
  120-character track title, a fleet device named `Desk / Left` (shown too), an amp and a Loki Max, in
  light and dark paper: no sideways scroll and every written word inside the width. Proved by
  `paper.py` "no_sideways_scroll_at_390_with_long_words".
- The notes: at most one red note, the most urgent (the card, a red note of the volume, the EQ's
  peak, a path that changes the sound, the passphrase note); the EQ said in words for each mode;
  the path's rates (`rate matches all the way` with an ink tick, or the rates with hand-drawn
  arrows and the word `to` for the ear, never U+2192); the track's time and album (no time when the
  part has no clock); the facts, power (`label: text`), presets and passphrase notes, each about one
  section; and the last change: from elsewhere it gives its time, `from this browser` or `from this
  phone` only for what this page asked within 3 s, a drag is one change, a first reading or a
  replaced model is none, and of several fields changed together a fixed one is told. No `style`
  attribute, every word escaped, only characters the faces carry. Proved by `node/paper-notes.mjs`
  ("at_most_one_note_is_red_and_it_is_the_most_important",
  "the_red_order_is_card_volume_peak_path_passphrase", "the_eq_is_said_in_words_for_each_mode",
  "the_path_note_names_the_one_hop_that_changes", "the_rate_note_says_when_the_rate_matches",
  "the_track_note_counts_its_time_and_names_the_album", "a_part_with_no_clock_shows_no_time",
  "a_change_from_elsewhere_gives_its_time_only", "a_change_this_page_sent_says_from_this_browser",
  "a_request_never_answered_stops_counting", "changes_together_tell_one_in_a_fixed_order",
  "a_drag_of_many_steps_is_one_change", "the_first_reading_and_a_new_model_are_not_changes",
  "each_note_names_what_it_is_about", "a_note_draws_its_arrow_as_an_ink_stroke_and_says_to",
  "drawn_notes_are_ink_with_a_tilt_and_no_style_attribute") and `paper.py`
  ("notes_appear_and_update", "from_this_browser_only_for_this_pages_change",
  "a_control_change_reaches_the_last_change_note", "each_note_sits_near_what_it_is_about",
  "one_red_note_is_circled": the ellipse holds the red note, the card's included; on a phone a
  `span.ring` and no ellipse; the real `.pn` and `.pn.red` read at AA in both papers in
  "contrast_in_light_and_dark").
- The test driver (`themes/paper/driver.js`): `reach` scrolls to the written word, the needle or the
  scale of a target (`#p-needle`, `[data-act=...]`, `[data-key]`, `td[data-fact]`), `operate` taps the
  word, presses the arrow keys on the needle or scale, picks an option of the paper list a word opens
  (mode, preset, a longer choice, the slot to save over and the question's `save`), and `shown` reads
  the words on the sheet; `state` reads the core's state, the card and the toasts as Ember's does.
  Proved by `contract.py` in Chromium and Firefox for Paper (volume up and down, mute, a band change,
  a preset, the device lost and back, a refused write, resting and waking, the menu button and `T`
  after a fade, `?theme=` kept, and the guest pass: every `operate` posts nothing and toasts
  `Sign in to change this`, Review Focus 3) and by `completeness.py` for Paper on the Gungnir 2 with
  its player, the amp with a signal, the five-meter and mono amps and the Loki Max: every target
  reached, operated and read back from the model route (one write each), every fact shown with the
  model's words. Each pass takes under 5 minutes per theme. Both passes now run over every listed
  theme (`NO_DRIVER_YET` is gone); `WEB_CHECK_THEMES=paper` limits a run to one, and a name the page
  does not list fails the pass (checked by running each pass with `WEB_CHECK_THEMES=bogus`: both
  report `WEB_CHECK_THEMES names only listed themes` failed). The driver finds its needle or scale
  again on every arrow press, so a section drawn again in the middle of a run does not leave it
  stale.
- Mutants of Paper's sources, run by hand with `node/mutate_paper.sh` (each killed by the harness
  named): the ruler not linear in the steps (`paper-ruler.mjs` "the_needle_fraction_follows_the_steps"),
  a place rounded down ("raw_at_a_place_rounds_and_clamps_to_the_range"), a place not held to the range
  (the same), labels allowed to touch (`LABEL_GAP = 0`, "a_label_keeps_its_gap_from_the_one_before_on_a_wide_ruler"),
  two red volume notes both red (`paper-notes.mjs` "two_red_volume_notes_make_one_red_note_the_first"),
  the peak redder than the volume ("the_red_order_is_card_volume_peak_path_passphrase"), a run of
  changes no run (`RUN_MS = 0`, "a_drag_of_many_steps_is_one_change"), a request this page's for a
  minute (`MINE_MS = 60000`, "a_request_never_answered_stops_counting"), the first reading a change
  ("the_first_reading_and_a_new_model_are_not_changes"), two changing hops told as the only hop
  ("the_path_note_names_the_one_hop_that_changes"), the EQ's words smallest first
  ("the_eq_is_said_in_words_for_each_mode"), a rate losing its decimals and a retry at nought seconds
  waiting (`paper-words.mjs` "a_rate_keeps_its_decimals_and_a_retry_at_nought_says_now").
- Mutants of Paper's page, run by hand with `mutate_paper.py` (one worker, `paper.py` in Chromium):
  `place` setting nothing (killed by "the_needle_sits_at_its_value"), the system's dark overriding a
  person's Light (the dark block's `:not([data-scheme="light"])` dropped; killed by "auto_follows_the_system:
  a person's Light beats a dark system", Chromium's emulated dark system), a held section drawn anyway
  (Review Focus 2; killed by the held-needle and held-point checks), the log empty (never `from this
  browser`; killed by "from_this_browser_only_for_this_pages_change" and
  "a_control_change_reaches_the_last_change_note"), a meter's needle at its class-A
  share (killed by "a meter's first draw puts the needle at its fraction", which draws the section
  from a reading with a fraction of 0.7 and a class-A share of 0.25; the node harness's amp test
  kills it too). Each must make a check of `paper.py` fail.

## Signal

- The cover in the Now playing bar and the player's own area (`signal_theme.py`, both browsers):
  "the_bar_shows_the_cover_when_there_is_one" (36 px, before the transport, a same-origin address
  with the version, no style attribute; the player's area shows it before the title),
  "the_cover_follows_the_track" (another track's address; a real 404 hides it with no gap; no cover,
  no box; it comes back), "no_cover_leaves_no_gap" (`SVOLDER_FAKE_COVER_GONE_AFTER_MS`),
  "the_bar_shows_the_cover_on_a_phone". Each killed by a mutant made by hand once: no cover drawn,
  an address with no version, no hidden handler.

The theme Signal (design sections 3, 8 and 9), built on the core alone. Its frame: the registration,
the line of hops, its dots and the choice of a hop. The open area below the line is filled by the
later tasks through `openers`.

- Registration: Signal is the third theme, dark only, on the ground `#0f0d0b`, with no rows of its own
  in the frame's menu. Proved by the Rust tests `signal_is_listed_dark_only` and
  `the_list_is_the_pickers_and_the_registrys_data`; the picker lists Ember, Paper and Signal and
  Signal's card has its name, its blurb and a picture (`signal_theme.py`
  "signal_is_listed_and_mounts_from_the_picker", `paper.py`, `effects_menu.py`, `completeness.py`).
- The hops (`node/signal-line.mjs`): `the_line_runs_player_computer_dac_then_its_hops_in_file_order`,
  `a_hop_after_another_dac_is_not_on_this_line`, `a_linked_hop_is_live_only_when_listed`,
  `no_host_means_the_line_starts_at_the_dac`,
  `the_computer_hop_stays_while_the_route_is_not_read` (the words `route not read`), `an_amp_no_hop_links_stands_alone_after_a_stand_in_ring`,
  `the_host_eq_marks_the_computers_stretch_as_changing` (the path's `computer` hop wins, the route's
  host EQ stands in when there is no path),
  `the_autoeq_headphones_ring_ends_the_line_when_no_headphones_are_declared`,
  `a_band_preset_draws_no_headphones_ring`, `a_declared_headphones_hop_replaces_the_autoeq_ring`,
  `the_dac_says_neither_while_its_rate_is_not_read` (a DAC hop marked unknown draws neither
  `arrives as sent` nor `changes the sound`; the Rust test
  `a_dac_with_no_read_rate_is_unknown_and_not_changing` holds the mark) and
  `a_choice_that_disappears_goes_back_to_the_shown_device`.
- The line in a browser (`signal_theme.py`, both browsers): `the_line_shows_every_hop_in_order` (the
  ids in order, spread between 10 % and 90 %, the band 140 px, the box 200 by 104 at y 30, the dots on
  y 60, one horizontal listbox, no `style` attribute in the markup),
  `declared_hops_are_rings_and_live_ones_dots` (the colours of the dots, the ring's italic words),
  `a_click_moves_the_box` (a click on a label or a dot), `left_right_move_the_choice` (arrows, Home,
  End, the focus and the one tab stop), `a_hop_that_leaves_gives_the_box_back` (Review Focus 1: a hop
  removed from the model, a device switch), `no_host_starts_the_line_at_the_device`,
  `an_amp_no_hop_links_stands_alone`, `a_draw_between_press_and_release` (the line is drawn again
  between the press and the release of a click on a hop, on the computer and on a declared hop: the
  one click still chooses it, and the hop element is the same one before and after, so the draw keeps
  the elements of the line; fails on the page that replaced them, 6 of 6), `a_draw_in_a_drag` (a draw
  while a finger drags the strip's track keeps the drag class and the track where the finger has it).
- The dots (Review Focus 4): `the_dots_move_while_playing_and_stop_on_pause` (the dashes' offset
  changes only while the track plays), `the_dots_stand_still_under_reduced_motion` (the player plays,
  the offset never changes, the dashes are drawn once), `no_frame_while_hidden` (no change of the
  offset while the page is hidden, movement again once it shows).
- The DAC's first screen (Task 3). `node/signal-draw.mjs`: `the_volume_reads_the_devices_db_with_its_unit`
  (the number and the unit, the track's slider values, the three buttons, `muted` after a mute),
  `the_volume_shows_the_step_asked_for_until_it_is_read_back`, `the_cap_is_marked_on_the_track`,
  `the_curve_has_one_point_per_band` (a point at each band's marker, the curve's 181 points, Flat
  dashed, Loudness without points, an unread band without its point), `the_choices_are_chips_with_the_one_on_marked` (the input chips; the mode chips come from `modeRow`; no Preset chip),
  `a_guests_controls_are_drawn_inert` (every guest and read-only fixture: every button and slider
  `aria-disabled` and quiet, and a writer's not), `no_markup_carries_a_style_attribute` (also the opener's whole screen; in the browser `#signal [style]` is empty but for the elements Signal places through the CSSOM, named in `CSSOM`),
  `words_from_the_model_are_escaped`, `the_bar_names_the_track_and_the_transport` (the progress
  against the daemon's clock, a paused track standing still) and the opener (a DAC's screen for the
  shown device only, an amp none).
- The first screen in a browser (`signal_theme.py`, both browsers): `the_first_screen_has_h6s_columns`
  (a DAC's: the volume at 310 px and the curve, 40 px apart, two columns from 1020 px; 96 px weight 200; no input or mode in it), `minus_and_plus_step_the_volume` (the
  buttons from the step asked for, Mute and Unmute, the focus kept through the redraw, the keys on the
  track, a drag of the knob), `dragging_past_the_cap_writes_the_cap` (no write above the cap, the knob
  and the cap mark meet), `a_point_drag_sets_a_band`, `a_tone_update_waits_for_the_point_drag` (Chromium only: the button
  stays down; the
  open area held through the drag, drawn from the newest model after it), `an_input_chip_sets_the_input`
  (also the mode chips and the preset list: open, Escape, kept through a change from elsewhere, a
  press on another chip while it waits is not lost to the redraw, a recall), `a_guests_tap_posts_nothing` (a real guest page, `guest = "read"`: every control inert, a tap on volume, mute, input, mode, preset and a point and the keys post nothing; a guest gets no transport at all) and
  `the_bottom_bar_plays_and_pauses` (the buttons press the player, the dots stop with the player paused
  through the bar and start when it plays, the time moves).
- The rest of a device, the shelf, and the slim bar (Task 4). `node/signal-draw.mjs`:
  `the_shelf_lists_every_control_the_model_has` (every fake fixture: each control but the input, which
  the first screen draws, once, whatever its kind; every band's three values with their steppers,
  the loudness, every preset, slot and setting), `the_band_table_has_typed_values_and_steppers` (a
  guest's fields read-only), `the_facts_start_folded`, `the_fold_is_kept_as_open_or_closed_and_works_without_storage`,
  `a_loki_max_shows_its_slots_and_settings` (in the middle column, no band table, a guest cannot save),
  `the_presets_have_save_as_rename_and_delete_for_a_writer_only`, `a_shelf_without_a_device_is_nothing`,
  `the_slim_bar_names_the_hops_and_rings_the_chosen_one` (escaped, the volume beside the shown device),
  `a_hold_given_back_before_a_reset_does_not_count_after_it` (the carried minor: a deferred give-back
  planned before a device switch does not drop a hold taken after it); `no_markup_carries_a_style_attribute`
  and the guest check now cover the shelf and the slim bar.
- The shelf in a browser (`signal_theme.py`, both browsers): `the_band_table_steps_each_value` (each
  of the three values steps up and down, a typed value is written with its unit and the field shows
  what the device took, Escape writes nothing, a value being typed stays through a change from
  elsewhere), `a_preset_saves_renames_and_deletes` (Save as, Rename and Delete in place; Delete asks
  first and Keep sends nothing), `the_shelf_controls` lines (a switch, a key on the balance scale, the
  centre, a drag, the Loudness strength: see `the_loudness_buttons_step_the_strength`), `a_slot_save_asks_first` (a Loki Max: slots, settings, the
  question before the write, `confirm: true` only after `Save`, recall and a built-in setting),
  `an_open_list_survives_a_part_update` (a list and a question both stay through a change from
  elsewhere), `the_facts_unfold_and_the_choice_survives_a_reload` (`svolder.theme.signal.facts`,
  folded by default, still folds with storage refused, Identify posts),
  `the_slim_bar_pins_when_the_line_scrolls_away` (64 px at the top, under the frame's layer, the same
  hops as the line, the chosen one with the DAC's volume, gone at the top) and
  `a_slim_bar_tap_chooses_and_scrolls_up` (and no bar on a phone). The guest check also taps the shelf.
- Fix round 1 of Task 4: the shelf draws the device's power (the switch, its switches, its readouts) for
  every family; the completeness test also walks every `targetsOf` target of every fixture;
  `the_power_switch_writes` (a Hati's shelf, a tap writes the opposite); the host and fleet events
  arriving while a question is open leave it open; `the_slim_bar_sits_at_the_safe_area_and_the_focus_keeps_clear_of_both_bars`
  (node: the stylesheet's safe-area top and the scroll margins) and `focus_clear` in the browser (the
  last shelf control and a band field focused on a 600 px window are fully visible between the bars).
- A live amp (Task 5): `node/signal-draw.mjs` `an_amp_opens_power_meters_and_faults` (power first with its
  switch, switches and readouts, a scale per channel with the class A share shaded and a needle at the
  reading, the amp's name, Identify, Unlink with the hop's id, an amp not read, in standby and warming up,
  a followed amp not read yet), `the_amp_has_its_power_once_per_page_and_the_shelf_leaves_it_out`,
  `an_amps_controls_are_inert_for_a_viewer_who_may_not_write`,
  `the_meters_are_put_in_place_without_drawing_the_area_again`,
  `the_dac_keeps_its_volume_written_while_the_amp_is_chosen` (muted, no volume read),
  `an_amps_shelf_is_its_facts_and_not_its_power`; `node/signal-line.mjs`
  `a_chosen_amp_that_leaves_the_fleet_gives_the_box_back_to_the_dac` (unplugged, unlinked, the list of
  followed devices). The completeness test now asserts the drawn hook of every preset, slot, setting, mode,
  loudness and transport target (it asserted nothing for them) and walks the amp's area; the no-style and
  escaping tests cover a linked amp. In the browser (`signal_theme.py`, both browsers):
  `a_linked_amp_is_a_live_dot` (solid amber, the thin stretch, followed by the stream, its area),
  `an_amp_control_writes_to_the_amps_url` (power, a switch, Identify; the needles move in place; no request
  to the DAC), `the_dac_keeps_its_volume_written_while_the_amp_is_chosen` (follows a volume change, the slim
  bar says the same), `the_dacs_name_opens_the_device_list`, `a_chosen_amp_that_leaves_the_fleet_gives_the_box_back_to_the_dac`
  (a question open for it closes, nothing written to the gone URL), `unlink_removes_the_hop_and_the_amp_stands_alone`
  (asks once, Keep sends nothing, one removal by the hop's id, the amp alone on its line when shown) and
  `a_guest_cannot_unlink_or_switch_an_amp`.
- Task 5 fix round 1: the amp's area draws its faults (`an_amp_opens_power_meters_and_faults`:
  the `fault` fixture's Over-temperature / Over-current rows and the card's advice, none for none; browser: a `fake-amp` fleet draws each fault row with the device's words), the chosen DAC's name
  carries a caret and `aria-haspopup` only with another device to list
  (`the_chosen_dacs_name_shows_that_it_opens_the_device_list`), the device list closes on a fleet change and on
  another choice, and the completeness test checks each preset's and setting's own button.
- `contrast_on_the_ground`: every kind of text (names, the chosen name, ring names and words, the
  green and yellow words, the quiet word) reads at 4.5 or more on both ends of the ground.
- The test driver (`themes/signal/driver.js`, Task 9): `reach` chooses the hop that holds the target as a person taps it on
  the line (the shown device's own, or a linked amp's when the control is the amp's, so its writes carry `{ device: url }`),
  unfolds the facts, scrolls to the word, scale, chip or field (`#sg-vtrack`, `[data-act=...]`, `[data-key]`, `.sg-scale`,
  `#sg-facts-body td[data-fact]`), `operate` taps it, presses the arrow keys on a scale, types nothing but steps a band
  with its `-` and `+`, picks an option of the list a chip opens (a longer choice, the slot to save over, then the
  question's `save`), and `shown` reads the words on the page (the volume's `aria-valuetext`, a band's field, a scale's
  value); `state` reads the core's state, the card and the toasts. `contract.py` and `completeness.py` run Signal like every
  listed theme (`NO_DRIVER_YET` is gone from both). Proved by `contract.py` in both browsers (66 checks) and by
  `completeness.py` (every target reached, operated and read back from the model route, one write each; every fact shown
  with the model's words) on the Gungnir 2 (36 targets, 36 facts), the amp (3, 14), the five-meter amp (3, 17), the mono
  amp (3, 14) and the Loki Max (19, 15): the counts equal Ember's and Paper's on every kind.
- Every theme lists what the first theme lists (`completeness.py`, `same_counts`): per device kind, the count of targets of
  each kind and of facts must equal the first theme's, failing with the kind and the two counts, so Signal's counts equal Ember's.
- The four hands-on passes of 0.31.4 (checks written after the code; each new check was run against the page of
  `1219166c` and fails there, by the browser run of `the_mode_has_its_place`, `the_loudness_buttons_step_the_strength`,
  `holding_a_band_stepper_repeats`, `the_menu_row_adds_a_device`, `the_tab_title_names_the_device_and_the_connection` and
  `the_menu_button_is_fixed_on_a_phone`; `other_themes_pin_the_button_too` holds on both pages, as it should):
  - The menu button on a phone (`signal_theme.py`, both browsers): `the_menu_button_is_fixed_on_a_phone` (the frame is
    `fixed`, position fixed, and the button keeps its place against the pinned strip across scroll steps 0, 150, 400, 700
    and back; at 1280 px it is pinned too, 8 px from the top after a scroll) and
    `other_themes_pin_the_button_too` (Paper and Ember: `fixed` too, the button 8 px from the top after a scroll).
  - A DAC's choices (`node/signal-draw.mjs`): `the_choices_are_chips_with_the_one_on_marked` (input chips only, no Preset
    chip), `the_mode_sits_where_the_screen_puts_it` (on a desktop the mode heads `#sg-mid` and the input is under the
    band table, neither in the first screen; on a phone the mode is under the curve and the shelf is `sg-up` (input,
    presets), the band table, the rest). Browser, `the_mode_has_its_place`: 1280 px and 390 px, a breakpoint change
    redrawing both ways, the mode drawn once, no sideways scroll; `the_first_screen_has_h6s_columns` (two columns from
    1020 px) and `the_curve_has_a_row_of_its_own_in_the_middle_layout`.
  - One write per tap on the input chips, now in the shelf (`an_input_chip_sets_the_input` counts the posts;
    `completeness.py` found the double write the shelf's own click handler added, fixed in `controls-wire.js`).
  - Loudness (`node/signal-draw.mjs` `the_loudness_chip_carries_round_steppers_while_loudness_is_on`; browser
    `the_loudness_buttons_step_the_strength`): the wrap `#sg-loud.sg-loudw` with round `−`/`+` over the chip's ends and the
    strength under its name, no separate box for a DAC, a click is one step, the strength follows, held it repeats and a
    release stops it, a key is one step, and the wrap is under the curve at 390 px.
  - Review round: an amp's shelf keeps its Loudness box in the middle column after the controls and a DAC without an
    input draws no empty choices section (`node/signal-draw.mjs`); the Loudness round buttons answer a touch anywhere in
    a 44 px square at 390 px (a pseudo-element grows the 38 px disc; `the_loudness_buttons_step_the_strength`); the Add
    device row is offered only when the sheet can open (node); a held stepper also ends on a move reporting no button
    down and after 10 s, and a second, non-primary pointer starts no hold and cannot end the first one's.
  - Holding (`holding_a_band_stepper_repeats`): a held band `+` steps at once, repeats after the pause across the
    redraws of the shelf (the button is replaced, the hold follows it by id), and stops on release, cancel, blur, a hidden
    page, the button leaving and another device shown; no context menu and no selection or panning on a stepper; a key is one
    step.
  - Add device (`node/signal-draw.mjs` `the_menu_row_adds_a_device_while_a_device_is_shown`; browser `the_menu_row_adds_a_device`
    and every `the_plus_sheet_*` check, which now open the sheet from the menu): the row is there with a device shown,
    absent without one, opens the sheet, and the focus goes back to the menu button.
  - The tab's title (`the_tab_title_names_the_device_and_the_connection`, `a_lost_stream_*`, `a_lost_device_*`):
    `Gungnir 2 · connected — svolder`, `reconnecting` while the stream is lost, the trouble card's words lowercased for a
    lost device, and plain `svolder` after another theme is picked.
- Mutants of Signal's sources, run by hand with `node/mutate_signal.sh` (each killed by the harness named): the line
  dropping the hops after the DAC (`signal-draw.mjs` `a_declared_hop_says_where_it_sits_and_moves_by_the_whole_lists_places`,
  and `signal-line.mjs` `the_line_runs_player_computer_dac_then_its_hops_in_file_order`), a declared hop drawn live
  (`signal-line.mjs` `a_linked_hop_is_live_only_when_listed`), a chosen hop that disappears staying chosen (`signal-line.mjs`
  `a_choice_that_disappears_goes_back_to_the_shown_device`), the facts starting open (`signal-draw.mjs`
  `the_fold_is_kept_as_open_or_closed_and_works_without_storage`), the computer's hop gaining a write
  (`signal-draw.mjs` `the_computer_opens_the_routes_facts_read_only`), the strip not centring the chosen hop
  (`signal-draw.mjs` `the_strip_is_the_same_hops_a_step_apart_with_the_wire_under_them`).
- Mutants of Signal's page, run by hand with `mutate_signal.py` (one worker, `signal_theme.py` in Chromium; each killed by
  the check named): an amp control written without `{ device: url }` (`an_amp_control_writes_to_the_amps_url`), the dots
  ignoring reduced motion (`the_dots_stand_still_under_reduced_motion`), the slim bar never pinning (`the_slim_bar` "scrolled
  past the line, it is shown" and `a_slim_bar_tap_chooses_and_scrolls_up`), the Add device row opening the sheet for a guest
  (the core refuses a guest's post, so the sheet's opening is what is mutated; killed by `a_guest_cannot_add_or_edit_a_hop`), the strip not centring (`the_strip_centres_the_chosen_hop`).
- Declared hops (Task 6): `node/signal-draw.mjs` `the_sheet_offers_unlinked_amps_first` (the amps no hop links as
  `<name> (connected)` before Amp / Headphones / Speakers / Other, the first chosen with its own name; none left: an
  Amp, declared), `a_linked_hop_edits_only_its_note` (the Edit form of a hop that links a device has the note field
  and no name or kind; a declared hop's has all three; a live linked amp's tools have no Remove),
  `a_declared_hop_says_where_it_sits_and_moves_by_the_whole_lists_places` (Move earlier / later name the place in the
  whole `[[hop]]` list of the previous / next hop of the same line, disabled at the ends, the place in words, a note
  escaped, a hop the list lost draws nothing), `the_sheet_and_the_declared_area_are_inert_for_a_viewer_who_may_not_write`,
  `no_style_attribute_in_the_sheet_or_the_declared_area`; `node/signal-line.mjs`
  `a_chosen_hop_removed_elsewhere_gives_the_box_back` (removed, renamed, moved to another line). In the browser
  (`signal_theme.py`, both browsers): `the_plus_sheet_adds_a_declared_hop` (opened from the menu's Add device… row; the sheet, Escape and Cancel send nothing,
  the typed fields survive a host / fleet / part reading, Add posts name / kind / note / the DAC's name, the new hop is
  chosen with its dashed box, then the repo-built `svolder hops list` on the same temp homes lists it; a refused name
  keeps the sheet open; a connected amp is linked and followed), `edit_changes_the_name_and_note` (in place, Escape,
  the box follows a renamed hop, the CLI lists the new name) and `a_linked_hop_edits_only_its_note`,
  `move_earlier_reorders_the_line` (the whole list's place posted, the CLI order, the ends disabled; a linked amp
  moves too), `remove_asks_then_removes` (Keep sends nothing, one removal by id, the box back on the DAC),
  `a_chosen_hop_removed_elsewhere_gives_the_box_back` (with the Edit form open, with a Remove question open; nothing
  posted) and `a_guest_cannot_add_or_edit_a_hop` (a real guest: the menu's Add device… row refuses, every button inert, the core's words, nothing
  posted).
- The player, the computer and the states (Task 7). `node/signal-draw.mjs`: `the_player_opens_the_track_and_the_verdict`
  (the title, the byline, the progress that moves, the transport, the path's hops and its verdict in the colour of its
  class, nothing playing, paused offers Play), `the_player_goes_inert_for_a_viewer_who_may_not_write`,
  `the_computer_opens_the_routes_facts_read_only` (the sink's words, rate and format, PipeWire's volume, muted, the host
  EQ off / on / on with its escaped preset, the other streams, the path verdict; no button, input, link, tabindex, data-act
  or role in it), `the_computer_says_route_not_read_before_the_route_arrives`, `a_lost_link_breaks_the_line_at_the_dac`
  (a hollow red ring, the stretch after it still and the dashes before it moving, what follows dimmed, amber for waiting and
  resting, no ring for a lost stream), `the_lost_card_offers_try_now_and_the_seconds` (Try now, the seconds, none for a
  refusal, the fault advice, inert for a viewer who may not write, the waiting and resting cards),
  `a_device_in_trouble_keeps_what_was_last_heard_dimmed_and_inert` and `player_and_computer_hops_open_their_areas` (no
  `style` attribute); `node/signal-line.mjs` `the_player_quitting_while_chosen_gives_the_box_back_to_the_dac` and
  `a_device_card_or_a_waiting_page_marks_the_shown_hop_in_trouble`. In the browser (`signal_theme.py`, both browsers):
  `the_player_transport_works_when_chosen` (the bottom bar hides and returns, previous / pause / next post the host's press,
  the dots move and stop with the player, the time in the area moves), `the_computer_hop_has_no_control` (the facts from
  the route, nothing focusable, a click or Enter sends nothing), `a_lost_device_shows_try_now_and_the_seconds` (the card in
  the open area, the seconds counting down, the red ring, the stillness and dimming after it, the player's dots still
  moving, the last heard volume and curve dimmed and inert, Try now posts connect, the device back clears it),
  `a_lost_stream_greys_the_line_and_makes_the_page_inert` (`Reconnecting to svolder…`, faded, inert, the dots still, back
  after a restart) and `a_refused_write_toasts` (a rename onto a taken name: the core toasts, nothing renamed, the page
  usable). Fix round 1: a card means a down link only for lost / looking / refused / waiting / resting (the ring, the still
  stretch, the dimmed inert columns); `a_fault_on_a_connected_device_keeps_its_controls_live` (a red ring, the line goes on,
  the controls live; the browser harness cannot fault a fake device, so node only); the lost-stream check waits for the dash
  and its movement first; a lost card under reduced motion and hidden.
- The phone (Task 8, under 720 px). `node/signal-draw.mjs`: `the_strip_is_the_same_hops_a_step_apart_with_the_wire_under_them`
  (the hops of the line and no other, a step of 110 px, the wire's dots on the same step, the dash loops and the yellow
  stretch, one line of words, the box, a ring chosen boxed dashed, an escaped name, no `style` attribute),
  `a_lost_link_on_the_strip_draws_the_same_ring_and_dims_what_follows`,
  `each_value_of_the_band_table_names_its_column_for_the_phone_rows`. In the browser (`signal_theme.py`, both browsers; 390 by
  844, Chromium under mobile and touch emulation, Firefox with its touch events on): `the_strip_centres_the_chosen_hop` (92 px
  high, the hops of the desktop line, the chosen at the middle with the neighbours 110 px either side, the box 112 by 74, one
  listbox, no `+` after the last hop, the wire not cut at the strip's width, the dots moving, the arrow, End and
  Home keys sliding it, no `style` attribute but where the CSSOM places), `a_swipe_chooses_the_neighbour` (a drag past a short
  way chooses one hop, a short one nothing, a tap on a neighbour, a swipe is not also a tap, nothing past the ends, a vertical
  stroke scrolls the page (Chromium) and chooses nothing, nothing posted), `the_strip_pins_when_scrolled` (sticky at the top
  under the frame's layer, no slim bar, a tap while pinned chooses, keyboard focus lands under the strip and above the Now
  playing bar), `touch_targets_are_44_px` (every button, hop, field and slider of a DAC page and the Add device sheet),
  `no_sideways_scroll_with_long_hop_names` (a DAC, an amp and a Loki Max with six hops after the device, a 40-character name and
  `Left/Right Cans`, at 390 and 1100 px: the page does not scroll sideways, also with the long hop chosen; at 1100 px the labels
  stay inside the page and clear of each other) and `the_said_and_stub_do_not_overlap` (host off and the stream lost, 1280, 720
  and 390 px). The floating `+` is gone: Add device… is a row of the frame's menu.
  Fix round 1: the frame's menu button is 44 px on a phone, sized by Signal's `--frame-btn-size` token (the frame is not
  edited), and keeps the page's right 60 px to itself (its hit area reaches 7 px past it); the strip is centred in the
  rest. `the_menu_button_clears_the_strip` (360, 390 and 719 px, with the first, the DAC and the last hop chosen: the
  button's rect and hit area against the box, dots, wire and every point of every hop), `the_strips_dots_stop` (reduced motion; hidden, then shown),
  `focus_clears_the_pinned_block_with_both_lines` (the scroll-margin follows the pinned block's real height), the 44 px
  measure also on a declared hop's area, an amp's, the lost card (Try now), and on the frame's button (its size comes from
  Signal's token, so Signal's check owns it).
- Final pass (0.31.4, after the whole-branch review). A page open at shutdown never hears the computer go: the daemon stops
  the web server before the host service (Rust test `a_page_open_at_shutdown_never_hears_the_host_go` in
  `web/tests.rs`, which fails on the old order with the `host: null` event in the stream), and `a_lost_stream_greys_the_line_and_makes_the_page_inert`
  now names `host === null` and the flow stretches in the detail of its `dots stand still` check. A linked amp shown on its own page
  (`node/signal-line.mjs` `a_linked_amp_shown_alone_has_the_dac_its_row_names_before_it`, also the amp whose DAC is not listed and the
  computer hop without words on a page that holds no route; `node/signal-draw.mjs`
  `a_linked_amp_shown_alone_draws_a_grey_way_in_then_a_thin_stretch_from_the_dac`; browser, both browsers:
  `a_linked_amp_shown_alone_has_the_dac_before_it`: the hops, one grey stretch, two greyed dots, one thin flowing stretch
  from the DAC, no ring, no words on the computer; an ArrowLeft onto the DAC boxes it in place, the page stays on the amp, focus and box agree and the area offers `Show <DAC>`; Enter or a tap on the DAC shows that device). The middle layout
  (`the_curve_has_a_row_of_its_own_in_the_middle_layout`, both browsers, 768 and 900 px: the curve 300 px or wider under the
  volume and beside nothing, a point of band 2 dragged and written, an amp's meters 300 px or wider in a row under its power;
  at 1020 px H6's three columns). An amp's faults are the facts' rows of the fault family by id (`node/signal-draw.mjs`
  `an_amp_opens_power_meters_and_faults`: a renamed fault is still drawn, a row outside the family is not). The `host-eq.toml`
  stamp that finds a preset applied to the same sink is `a_state_file_written_again_has_another_stamp` (Rust). The
  chosen box is narrower than a step (the clearance sweep of `no_sideways_scroll_with_long_hop_names` and
  `the_menu_button_clears_the_strip` still pass). Mutants: `node/mutate_signal.sh` has two more (a linked amp losing its DAC, an
  amp's faults picked by their words); `mutate_signal.py` counts a kill only when the mutant's own check fails.
- The user's pass (0.31.4), each proved by a check that failed on the page before the change.
  `node/signal-draw.mjs` and `signal_theme.py`: `the_volume_buttons_are_in_embers_order` (`−`, Mute,
  `+`; a device with no mute key keeps `−` and `+`) and `muted_shows_minus_infinity` (the number
  `−∞` with the unit `dB`, Unmute pressed, the slider's value text `muted`; the slim bar's and the
  line's volume words `−∞ dB`, `volumeWords`). `holding_plus_repeats` (`signal_theme.py`, both
  browsers; a press steps once, 380 ms later the steps come every 70 ms; a release, a cancelled
  pointer, a blur of the window, a hidden page and the button leaving the page each stop them; Enter
  on the focused button stays one step; in Chromium a real held mouse on `−` too).
  `a_pinch_sets_the_q` (`signal_theme.py`, both browsers, a phone: synthetic touches on the curve;
  twice as far apart halves the Q of the band nearest the midpoint, half the distance doubles it,
  frequency and gain untouched, the touch events prevented, a lifted finger ends it).
  `the_transport_is_drawn_as_icons_no_font_can_swap` (`node/signal-draw.mjs`: no glyph text, one
  svg per button, `currentColor`, `aria-hidden`, no `style`, the labels kept) and
  `transport_icons_are_drawn_shapes` (`signal_theme.py`: 14 px, the button's colour, no text).
  `the_page_fills_a_window_up_to_1100` and `a_tall_window_keeps_the_block_together` (1280 by 1000 the
  line at the top and the bar at the bottom as always; 1280 by 2000 the gap between the open area
  and the shelf no larger than at 1100 and the block centred within 3 px; a 600 px wide window
  keeps the page as it is) and `the_shelf_has_two_columns` (900, 1100, 1280 and 1700 px: the
  band table beside the middle column, the facts below across the width; one column at 600, 720,
  800 and 899 px).
  The hold also stops when another device is shown, survives a redraw of the open area (the button
  stays, the steps go on), and a context menu on a held button is prevented; `a_pinch_handoff` (a
  finger on band 2's point, a second one centred near band 3: band 2's Q changes, band 3's does not).
  Mutants (`mutate_signal.py`, `node/mutate_signal.sh`): a hold that does not hold the open area, a held plus that never repeats, a held band stepper that never repeats, a blur
  that does not stop a hold, the menu button following the page on a phone, the mode missing under the curve on a phone, a tab that keeps its plain title, a pinch the wrong way, a tall window that stretches the open area, a
  shelf of three columns, a muted volume that says the word, a transport icon not hidden, the mode kept on a desktop's first screen, a Loudness chip without its round buttons, the menu row offered with no device (`node/mutate_signal.sh`).

## Night drive

The theme Night drive (design sections 3, 8 and 10), built on the core
alone. Its frame: the registration, the scene (sky, sun, stars, the plain side grid, the floor of lit
cells rolling with the music and the mountains' ridge), its motion levels, the sky's rare events and
what it costs, and the HUD over it (the top line, the outrun volume, the EQ line and the fade). The
dash and the sheet come with the later tasks.

- Registration: Night drive is the fourth theme, dark only, on the ground `#0a0018`, with the Motion rows (below) in
  the frame's menu. Proved by the Rust tests `nightdrive_is_listed_dark_only` and
  `the_list_is_the_pickers_and_the_registrys_data`; the picker lists Ember, Paper, Signal and Night
  drive and Night drive's card has its name, its blurb and a picture (`nightdrive_theme.py`
  "nightdrive_is_listed_and_mounts_from_the_picker", `paper.py`, `signal_theme.py`,
  `effects_menu.py`). `contract.py` and `completeness.py` run it like every listed theme (below, "The driver").
- The camera and the axis (`node/nightdrive-scene.mjs`): `the_nearest_row_is_at_the_bottom_and_the_far_row_near_the_horizon`,
  `the_far_rows_are_wider_than_f6s` (x = 6 at the horizon is 0.16 of the width from the middle, F6's
  0.069), `the_axis_puts_20_hz_left_and_20_khz_right_and_round_trips`,
  `every_band_lands_in_one_column_and_every_column_has_a_band`.
- The music's levels and the floor (`node/nightdrive-scene.mjs`): `a_lone_band_at_1_khz_lights_its_column_only`,
  `the_ridge_reads_minus_60_to_0_db`, `the_bass_and_the_highs_are_the_means_of_their_columns`,
  `rows_come_twice_a_beat_or_steadily` (`rowRate(120) = 4`, no tempo 3.5, 300 is 6, 30 is 2),
  `the_floor_keeps_22_rows_newest_at_the_horizon`, `a_still_floor_does_not_roll`.
- The scene in a browser (`nightdrive_theme.py`, both browsers, the host's synthetic capture):
  `the_scene_draws_sky_sun_mountains_and_floor` (sampled pixels: the top of the sky within 12 of
  `#0a0018`, the sun's centre warm, a floor cell lit after 2 s, the ridge's cyan edge),
  `the_floor_rolls_with_the_music` (the draws go on and the newest row is a new one, with the music's
  columns, within 1 s), `the_menu_button_sits_in_its_slot` (a 44 px slot at the top right, the frame's
  button over it), `contrast_on_the_scene` (each text token on `#0a0018` and on the panel over the
  dusk at the horizon at 4.5 or more).
- The sky's rare events (`node/nightdrive-sky.mjs`), by the clock and never by the frame:
  `rays_need_the_bass_held_high_for_3_s` (0.7 for 2.9 s none, 3.0 s one, a dip starts the hold again),
  `rays_come_at_most_once_a_minute` (10 minutes of high bass, 9 or 10 rays, 60 s apart),
  `the_rays_fade_in_2_s_hold_6_s_fade_out_4_s` (0.5 at 1 s, 1 at 5 s, 0.5 at 10 s, 0 at 12 s, peak
  alpha 0.55), `a_star_needs_a_strong_transient` (steady highs no star, a jump of 0.3 one, 0.15 and a
  faint rise under the floor of the range none), `a_star_has_its_place_and_lasts_0_8_s`, `stars_come_at_most_one_every_5_s` (a
  transient every 100 ms for 5 minutes, 55 to 60 stars, 5 s apart),
  `a_shower_comes_on_intense_highs_at_most_every_40_s` (highs at 0.7: none before 2 s, then 3 to 6 stars over
  1.5 s ignoring the 5 s gap, 40 s apart, the same at 30 and 144 steps a second; a dip restarts the hold),
  `the_levels_readout_says_what_the_music_reaches` (`driver.sky().levels`: now, how long the bass has held, the
  highs' jump, the maxima of the last 60 s, and each against the loudest of the last minute),
  `music_with_dynamics_fires_rays_a_shower_and_stars_at_the_users_levels` (kick-shaped bass 0.16 to 0.54 at 2 a second, hat-shaped
  highs 0.09 to 0.22 at 4 a second, a 40 s verse at 60 % and a 30 s chorus: 300 s give 1 to 4 rays, 1 to 4 showers and 3 or more
  stars), `a_flat_drone_and_near_silence_fire_nothing`, `a_smooth_bass_swell_from_0_30_to_0_54_over_20_s_fires_rays`,
  `quiet_listening_fires_what_loud_does` (rays and showers read by their place in the minute's range, a leaky count of how long they
  are high),
  `the_dice_is_rolled_for_a_transient_not_for_a_frame`, `the_limits_hold_at_any_frame_rate`
  (200 s at 30, 60 and 144 steps a second, and with a 2 s gap, the same firings
  within one), `nothing_fires_when_off`, `the_log_keeps_the_last_20`.
- Motion (`node/nightdrive-sky.mjs`): `reduced_motion_starts_on_calm_unless_a_level_is_kept`,
  `the_pace_of_each_level_and_of_silence_and_the_idle_pace_is_the_phones_alone` (Full 0, Calm 100, Still 1000 ms; on a
  coarse pointer only, silent or paused at least 125; fine and coarse across the three levels and playing, paused and silent). In a browser (`nightdrive_theme.py`, both): `the_menu_has_motion_full_calm_still` (three
  radios, Full checked), `motion_is_kept_per_browser` (Calm under `svolder.theme.nightdrive.motion`,
  still Calm after a load), `calm_stops_the_road` (no new floor row in 2 s, draws go on, Full rolls
  it again), `still_draws_about_one_frame_a_second` (3 to 7 draws in 5 s),
  The timing cases, in `nightdrive_timing.py` (both browsers, the gate's serial lane),
  are `a_paused_player_on_a_desktop_keeps_its_levels_own_pace` (the draws go on at the rate they had, the asked pace under 125 ms), `a desktop starts with the wet road allowed by the budget` (the budget, fed the real draws, allows the wet road at the start), `reduced_motion_starts_on_calm_and_full_can_be_picked` (the system asks for less: pace 100, Full
  picked: draws keep coming) and `what_a_draw_costs` (below).
- What it costs (`nightdrive_theme.py`): `the_analysis_is_held_while_shown_and_let_go_when_hidden_or_unmounted`
  (one holder of the core's analysis while shown, still held inside the core's grace, none after it,
  held again once shown, none after a switch to Paper) and `a_switch_away_and_back_leaves_one_loop` (the page's frame requests for each frame of the browser
  after a round trip to Paper are what they were: a second loop adds one a frame; killed by `a mount starts a loop nothing ends`); in `nightdrive_timing.py` `no_frame_while_hidden` (no draw while the
  page is hidden, draws again once shown).
- The HUD's words and fade (`node/nightdrive-hud.mjs`): `the_top_line_names_device_input_and_rate`
  (`Bifrost 3 · USB · 96 kHz`, each left out when absent), `the_verdict_shows_when_the_computer_changes_the_sound`
  (`host EQ on · changes the sound` in amber; `host EQ on` alone when the path says it changes
  nothing; nothing with the host EQ off), `trouble_replaces_the_verdict` (lost, an unreachable card,
  a fault, reading again, resting: the core title's words), `the_eq_line_reads_mode_or_preset_and_gains`
  (`EQ WARM +3.0 +1.5 0.0 −1.0 +2.0`, `EQ`, `LOUDNESS 3`, `FLAT`, no tone: no line),
  `the_volume_reads_its_db_and_muted_reads_minus_infinity`, `an_amp_without_a_volume_says_who_sets_it`,
  `the_outrun_volume_has_no_style_attribute_and_prefixed_ids` (no `style`, no script, the four `nd-`
  ids, the minus and the infinity are paths and never glyphs), `the_fade_waits_its_time_and_any_input_wakes_it`
  (a fake clock: six kinds of input, the idle time counted from the last), `trouble_holds_the_fade_off`
  (and trouble arriving while faded wakes it), `wire_hud_holds_the_fade_for_every_trouble_word` (waiting,
  resting, lost, an unreachable card and a fault card through `wireHud.draw`; back to connected the
  fade resumes after the full idle time), `a_volume_from_elsewhere_shows_the_volume_for_5_s_while_faded`.
- The HUD in a browser (`nightdrive_theme.py`, both browsers, the fade preference set to 2 s):
  `the_hud_says_what_the_model_says` (the top line, the volume's aria and the EQ line against the
  model), `the_top_fades_after_the_idle_time_and_any_input_wakes_it` (opacity 0 within 5 s, 1 at
  once after a pointer move), `the_faded_hud_stays_in_the_tab_order_and_focus_wakes_it` (not
  `display: none`, not hidden, in the tree; a Tab wakes it), `a_volume_change_from_elsewhere_shows_the_volume_for_5_s`
  (only the volume shows; faded again by 6 s), `trouble_while_faded_shows_the_top_line`
  (the device dropped while faded brings the top line back with the tab title's status word and keeps
  it up 5 s past the idle time), `the_menu_button_is_not_in_the_fade`, `the_figures_are_russo_one_and_the_unit_mr_dafoe`
  (`document.fonts`), `the_hud_words_clear_the_sun_or_pass_contrast_there` (1280x800 and 1280x1600:
  each word's box against the sun's drawn box, no overlap or 4.5 on the sun's darkest stop). The
  dash's half of the fade checks is below.
- The dash's markup (`node/nightdrive-hud.mjs`): `the_dash_runs_minus_mute_plus_input_track_transport_device`
  (`−` `MUTE` `+`, the input, the track, the transport, `DEVICE ▴`, in that order; ids for the held keys; no
  `style`; MUTE pressed while muted; no volume keys on a device with none),
  `three_inputs_are_chips_and_more_are_one_chip_with_a_list` (3 chips; 4 or more one chip naming the
  current input, `aria-haspopup="listbox"`), `the_transport_is_svg_not_glyphs` (one svg and a path per
  key, no glyph; pause while playing, play while paused), `a_guests_dash_is_drawn_inert` (the keys
  `quiet` and `aria-disabled`, the sheet's key not; a guest has no transport), `the_track_reads_title_artist_album_and_time`
  (`Massive Attack · Mezzanine · 1:42 / 5:29`, the fill's share, the time moving two seconds on, a paused
  track standing still, `Nothing playing`), `an_amp_has_power_and_readouts_where_the_input_would_be`.
- The dash in a browser (`nightdrive_theme.py`, both browsers): `the_dash_runs_minus_mute_plus_input_track_transport_device`
  and `the_track_reads_title_artist_album_and_time` and `the_transport_is_svg_not_glyphs` against the
  model (40 px keys along the bottom), `minus_and_plus_step_the_volume_and_mute_toggles` (a tap steps one,
  two quick taps two from the step asked, a key on the focused `+` keeps the focus through the redraw),
  `muted_reads_minus_infinity` (the HUD's figures gone, the infinity drawn, `muted, minus infinity dB`,
  MUTE pressed), `holding_plus_repeats_and_the_release_ends_it` (a step at the press, the next about 380 ms
  on, then every 70 ms; a release, a cancel, a blur, a hidden page and a lost stream end it; a redraw
  keeps the button; a long press's context menu is prevented; Enter stays one step),
  `dragging_past_the_cap_writes_the_cap` (`[web] max_volume_db`, a held `+` writes the cap and nothing above
  it), `an_input_chip_sets_the_input` (the Loki Max's two chips), `a_long_input_list_opens_and_picks`
  (five inputs: one chip, the list over it, Escape closes it with no write, a pick writes it),
  `the_transport_plays_and_pauses` (pause posts the host's press and the key turns to play; the time and
  the fill move on), `a_guests_tap_posts_nothing` (a real guest: only the sheet's key is live; a tap on
  `+`, `−`, MUTE and the input posts nothing and the core says to sign in),
  `the_dash_fades_to_a_tenth_with_the_hud` (1280x900 and 915x412: the dash at 0.1 with the HUD after the idle time),
  `a_move_wakes_the_dash_at_once`, `a_tap_on_a_faded_key_only_wakes_the_dash` (nothing posted, the next tap writes, the
  click of the waking tap presses no transport key), `the_dash_stays_while_its_input_list_is_open` (the HUD faded, the
  dash at 1; it fades again once the list is closed), `the_portrait_panel_never_fades` (412x915) and
  `a_long_press_that_woke_a_faded_dash_presses_nothing_when_let_go` (held 1.5 s, then released and clicked: nothing posted; the next press does) and
  `a_press_on_the_dash_while_faded_acts_and_wakes` (a mouse arriving wakes it and its press on `+` posts the volume,
  the top shows again); in `node/nightdrive-hud.mjs`: `the_fade_waits_15_seconds_unless_the_viewer_kept_another_time`
  and `the_fade_tells_its_watchers_when_it_fades_and_what_woke_it`.
- The sun at the horizon (`nightdrive_theme.py`, dpr 1 and 2 in Chromium): `the_sun_reaches_the_horizon` (a band at
  its lowest gain brings the curve down to the horizon; the sky canvas is sun-coloured in every pixel from 0.05h
  above the horizon to 2 CSS px above it, no stripe or dark gap).
- The ridge as heights (`node/nightdrive-ridge.mjs`): `the_ridge_has_no_sawtooth_in_silence`,
  `a_bass_band_makes_a_rounded_hill`, `the_swell_drifts_slowly_and_freezes_in_still`,
  `the_swell_gives_way_to_the_music`, `the_ridge_is_drawn_as_curves_with_no_run_of_straight_segments`.
- The floor as a heat grid (`node/nightdrive-floor.mjs`): `a_quiet_row_is_flat`, `a_loud_cell_is_lifted_and_hotter_than_a_quiet_one`,
  `a_crossing_of_level_1_at_the_nearest_row_is_lifted_by_the_floors_height`, `strips_are_drawn_far_first`,
  `the_rows_roll_toward_the_viewer_and_a_floor_not_rolling_is_drawn_the_same`,
  `the_heat_follows_the_ramp_and_leans_warm_for_bass_and_cool_for_highs`, `the_wet_road_thins_the_ground_so_the_reflection_shows`,
  `the_mood_dims_the_lines_and_the_heat`, `a_wave_runs_down_the_floor_from_the_horizon_and_fades`,
  `cells_alike_in_colour_and_alpha_are_filled_together_and_unlike_ones_apart`,
  `a_lifted_near_edge_still_has_floor_under_it_to_the_bottom_of_the_scene`; in both browsers
  (`nightdrive_theme.py`) `the_loud_floor_is_hotter_than_the_quiet_one` and `the_loud_floor_is_lifted` (pixels, the mood's layers
  off); in `nightdrive_timing.py` `what_a_draw_costs` prints drawMs with every cell lit and in the quiet with the wet road, and in Chromium holds the median of five readings to 4.5 ms for
  every cell lit and asserts the draws went on between the readings (a stalled loop would keep its last mean; slow machines are covered by the budget's step-down).
- The ear and the mood (`node/nightdrive-mood.mjs`): `a_level_is_read_against_the_loudest_of_the_last_minute`,
  `the_medium_view_eases_over_1_2_s_and_the_long_over_8_s`, `quiet_music_at_a_low_volume_still_reaches_long_near_1_after_a_minute`,
  `a_drop_after_a_breakdown_fires_one_moment_and_a_steady_loud_song_fires_none_after_the_first`,
  `a_moment_comes_at_most_once_every_15_s`, `the_balance_leans_to_the_highs_or_the_bass_and_silence_holds_it`,
  `a_peak_of_6_s_brings_the_embers_which_ease_in_and_out`, `the_ear_is_the_same_at_30_60_and_144_steps_a_second_and_after_a_gap`;
  the look (`the_mood_is_0_18_plus_0_82_of_the_long_view_and_0_7_looks_as_it_did`,
  `the_road_is_wet_and_the_air_hazy_in_the_quiet_and_dry_and_clear_when_loud`, `the_heat_leans_warm_for_bass_and_cool_for_bright_music`,
  `the_cruise_speed_is_0_6_to_1_4_times_the_tempo_s_and_eases_over_seconds`,
  `a_moment_flares_the_sun_for_2_5_s_and_sends_a_wave_in_full_only`, `calm_halves_the_ears_rates_of_change_and_still_takes_the_colours_as_they_come`,
  `still_finds_no_moment_so_spends_none_and_a_flare_stops_on_a_switch_to_it`,
  `embers_only_at_a_peak_and_only_in_full`) and the sparks (`embers_rise_from_hot_cells_only_while_the_peak_lasts_and_fade_and_die`,
  `sparks_rise_only_from_the_near_half_of_the_floor`, `the_number_alive_is_capped_by_the_canvas_s_area`,
  `the_births_follow_the_strength_and_the_clock_not_the_frame`). In both browsers (`nightdrive_theme.py`, the bands held and the ear moved; the cruise case is in `nightdrive_timing.py`):
  `the_night_follows_the_song` (mood over 0.9, a dry road and no haze when loud; under 0.3, wet and thick haze in the silence; the sky
  deeper in the quiet), `the_wet_road_shows_the_sun_in_the_quiet_and_not_when_loud` and `the_haze_lies_over_the_far_floor_in_the_quiet_only`
  (pixels), `nowet_takes_its_layer_away_and_leaves_the_others`, `nohaze_takes_its_layer_away_and_leaves_the_others`,
  `nomood_keeps_the_scene_as_it_was`, `a_drop_brings_a_flare_and_a_wave` (flare over 0.6 within 4 s of loud music after the silence, with the wave; the pixel
  just outside the sun's rim is sun-coloured during the flare and the night before and after, the green channel at 1.4 radii
  rises 8 or more over the sky at the same height; all gone 2.5 s later), `the_embers_rise_after_a_peak_of_6_s`, `noembers_takes_the_sparks_away` and `the_road_cruises_with_the_song_and_keeps_the_tempo` (rows a second about 5.6 when loud, 2.4 in
  the silence and 4 with the mood off: the cruise factor multiplies the tempo's rate).
- A phone stays usable (`node/nightdrive-budget.mjs`): `the_budget_caps_the_density_at_2_and_the_canvas_at_2_5_megapixels`,
  `a_slow_mean_steps_the_scale_down_by_0_75_to_one_backing_px_and_then_caps_the_rate`,
  `one_slow_draw_among_fast_ones_changes_nothing`, `the_scale_comes_back_only_after_10_s_comfortably_under_budget`,
  `a_middling_mean_neither_steps_down_nor_counts_as_comfortable`, `a_phone_that_is_never_slow_stays_at_its_capped_density`,
  and the wet road (`the_wet_road_is_the_first_thing_a_slow_mean_drops_before_the_scale_and_then_the_rate`,
  `too_few_draws_a_second_drops_the_wet_road_first_too`, `a_phone_starts_without_the_wet_road_and_it_comes_last_after_the_cap_and_the_scale`,
  `the_wet_road_stays_off_for_3_minutes_after_a_step_down_dropped_it`; in the browser `a_phone_starts_without_the_wet_road`);
  in Chromium `a_phone_stays_usable` (density 3 held to 2; with the CPU 30x slower the scale steps down with the CSS size
  and the Motion choice untouched; with it 6x slower a tap on the frame's button opens the menu within a second);
  the rate as a second trigger (`node/nightdrive-budget.mjs`:
  `too_few_draws_a_second_for_3_s_steps_down_while_every_frame_is_asked_for_though_each_draw_is_fast`,
  `a_rate_under_24_means_nothing_when_the_scene_asks_for_less_than_every_frame_or_the_draws_are_comfortable`,
  `a_brief_dip_under_3_s_is_not_a_step_down`, `after_a_step_down_for_the_rate_the_way_back_stays_shut_for_a_minute`,
  `a_pause_between_draws_restarts_the_rate_too`, `the_30_a_second_cap_keeps_its_cadence_at_any_screen_rate_and_never_trips_the_rate_trigger`
  (50 to 144 Hz: the frame nearest the pace is drawn, 28 or more a second from 60 Hz up, no step down); both browsers, in `nightdrive_timing.py`, `a_slow_page_steps_down_on_the_rate`: the main thread busy
  45 ms in 50 gives 20 draws a second of 3 ms, and the scale steps down or the rate is capped within 14 s while the same
  page left alone does not) and a phone's start at the cap (`a_phone_starts_at_the_rate_s_cap_and_the_cap_lifts_after_10_s_under_budget`;
  Chromium, in `nightdrive_timing.py`, `a_phone_starts_at_30_a_second`: a coarse pointer begins capped and the draws keep to 21 to 34 a second).
- The readout and the bisect switches (`node/nightdrive-debug.mjs`: `an_address_without_the_parameter_asks_for_nothing`,
  `the_bare_parameter_shows_the_readout_with_every_layer_on`, `the_switches_are_comma_separated_and_a_wrong_word_is_named_not_obeyed`,
  `the_rate_is_the_draws_of_the_last_second_not_the_one_asked_for`, `the_mean_and_the_95th_percentile_are_of_the_last_60_draws`,
  `the_box_says_each_number_and_n_a_where_the_browser_has_none`, `the_new_switches_are_named_and_obeyed`, `the_box_without_an_ear_says_so`;
  `nightdrive_theme.py`, both browsers):
  `the_readout_is_not_made_without_the_parameter`, `the_readout_shows_the_numbers_and_hides_from_the_tree_and_the_query_stays`
  (aria-hidden, the query kept, the draws a second within 10 of the loop's own count, drawn twice a second and no more),
  `nosky_nodash_nohud_and_noglow_turn_off_what_they_name`, `nofloor_noridge_nohaze_and_noembers_leave_the_road_canvas_empty` and, in Chromium,
  `dpr1_draws_at_one_backing_pixel_per_css_pixel`. Firefox's checks now set the density they are asked for.
- The glow is drawn, never blurred (`node/nightdrive-glow.mjs`): `nothing_in_the_scene_is_blurred` (no `shadowBlur` or
  `shadowColor` set by the sky, the ridge, the floor, the bubbles or the diamonds, glow on or off, lit or dull),
  `the_suns_glow_is_a_gradient_round_it_and_is_left_out_when_the_glow_is_off`,
  `the_ridges_edge_and_the_bubbles_glow_with_wider_fainter_strokes_under_the_line`, `the_wet_roads_slices_sway_as_one_wave`
  (adjacent slices differ by under 1 px at 1280 wide at any time, every slice leans the same way, the near ones further). In a
  browser (`nightdrive_theme.py`, both): `the_wet_roads_mirror_is_never_older_than_one_draw` (a copy of the sky at every draw).
- The phone's top (`nightdrive_theme.py`, both browsers, density 3): `the_phone_top_has_room` (at 412x915 and 360x780, in EQ and in
  Flat: the device line, the dB readout, the EQ readout, the menu button and the verdict meet nowhere; the EQ readout holds the bands' gains; with every band at the device's highest gain the curve's top and the highest bubble are below the lowest of the readouts' boxes, which fails when the stack grows past them; and a 55-character device name wraps without meeting the dB or the button).
- The ridge keeps its detail (`node/nightdrive-ridge.mjs`): `a_notch_a_quarter_octave_wide_is_still_there_after_smoothing`
  (a notch of 6 bands and one of 2 show 40 % or more of their depth), the ripple case and `a_bass_band_makes_a_rounded_hill`
  bracketed from both sides round sigma 0.87, radius 2, step 3 px (a kernel twice as wide or half as wide fails: ratio 0.5 to 1.2 per 5 px of axis, peak 45 to 85 px); the ripple case measures the tooth as the second difference of the smoothed bands in px (it does not shrink with a finer step) and holds it under 0.5 px: 0.87 is the smallest sigma that does so at radius 2 (0.84 leaves 0.61, 0.6 leaves 3.2).
- The cursor rests (`node/nightdrive-hud.mjs`: `the_cursor_rests_after_2_s_still_over_the_scene_and_a_move_brings_it_back`,
  `a_move_restarts_the_2_s`, `the_cursor_stays_over_the_dash_and_the_menu`,
  `the_cursor_stays_over_the_huds_words_though_they_are_click_through`,
  `the_cursor_stays_while_a_button_is_held_and_rests_2_s_after_the_release`, `a_finger_has_no_cursor_to_hide`,
  `stop_brings_the_cursor_back_and_lets_the_document_go`; `nightdrive_theme.py`, both browsers):
  `the_cursor_hides_when_it_rests` (computed cursor over the scene is not none after 0.5 s, none after 2.2 s, back on the next move,
  never none over the dash or over a word of the HUD (click-through, so the scene is under it), and not none while a bubble is held
  still, in both browsers; `the_cursor_stays_over_the_huds_words_though_they_are_click_through` on a fake clock).
- The cover is as tall as the track (`nightdrive_theme.py`, both browsers): `the_cover_spans_the_title_to_the_bar` (at 1280x800
  and 412x915: top within 2 px of the title's text box, bottom within 2 px of the bar's bottom, square).
- The picker on a phone (`effects_menu.py`, both browsers, density 3; a finger in Chromium, a wheel in Firefox, which has no
  touch path in these runs): `the_picker_reaches_every_theme_on_a_phone` (915x412, 412x915 and 360x780, Night drive and Ember:
  the sheet ends inside the window; where the grid scrolls a drag brings the last card fully into view, where it fits the last
  card is inside it; red at 360x780 against the old CSS). The frame menu under a finger (Chromium):
  `the_menu_ends_inside_what_is_seen` (412x915 pinched to 3x, so the visual viewport is a third of the window: the menu's bottom
  is inside it) and a touch drag of the open menu in a 390x300 window brings its last row into view, Night drive and Ember.
- The theme's thumbnail (`node/nightdrive-draw.mjs`: `the_thumbnails_sun_stripes_show_at_the_menus_size_in_the_palettes_colour`):
  each stripe is 1.5 px or more at the menu's 32 px, in the palette's stripe colour.
- The pinned button's scrolled case is tried against a scroll (`frame_button.py`): a theme whose page has room to scroll must have
  moved, and at the phone sizes at least one theme's page scrolled (`the_scrolled_case_scrolls`); Ember's glow-edge check
  (`effects_layout.py`) skips only the button's own rows and columns.
- The cover in the dash (`node/nightdrive-hud.mjs`: `the_cover_sits_between_the_input_and_the_title_and_a_track_with_none_has_no_box`;
  `nightdrive_theme.py`, both browsers): `the_dash_shows_the_cover_when_there_is_one` (the fake host's picture drawn, 30 px, rounded
  4 px with a cyan edge, between the input and the title, a same-origin address with its version, no style attribute; on a phone
  held upright it stays on the track's own row), `no_cover_leaves_no_gap` (`SVOLDER_FAKE_COVER_GONE_AFTER_MS`: no box, the title
  moves left to the track's own start) and `the_cover_follows_the_track` (another track's address; a real 404 is hidden and leaves
  no gap; a track with none has no box; the first one comes back).
- The tuning on the mountains (`node/nightdrive-tune.mjs`): `the_map_puts_0_db_at_the_baseline_and_the_range_inside_the_mountains`
  (0 dB at `H - 0.08h`, the largest gain `0.075h` away, the whole range inside the band that shows the curve),
  `the_map_round_trips_hz_and_db`, `the_morph_rises_over_the_mountains_and_falls_1_5_s_after_the_last_touch`
  (fake clock: 12 % of the gap a frame, the grace measured from the release, a touch and a lifted finger the
  same grace, the bubbles shown before they are grabbable), `a_guest_or_no_writes_never_morphs`,
  `no_bands_never_morphs` (an amp, a device in Flat with no bands), `loudness_shows_the_curve_without_bubbles`,
  `bubbles_sit_at_each_bands_own_gain` (also in Flat), the held band's curve and label from the raw steps,
  `the_pace_returns_to_the_levels_once_the_morph_has_settled` (the morph is moving only on its way, not at rest).
  In a browser (`nightdrive_theme.py`, both unless noted; the page's requests read from its own request log,
  relative paths): `hovering_the_mountains_shows_the_curve_and_leaving_lets_it_go` (`data-tune`, a bubble per
  band), `a_bubble_drag_sets_frequency_and_gain` (the last write of each parameter is the step nearest the drop,
  worked out here from the scene's size and the device's tables, to that band of that device only),
  `a_flat_device_switches_to_eq_on_the_first_drag` (one toast), `the_curve_stays_1_5_s_after_the_drag` (on at
  1.2 s, off by 2.5 s with the pointer gone from the mountains), `a_guest_tap_on_the_mountains_says_why` (a real
  guest: no morph, nothing posted, the core's sign-in words). Chromium (a button held across calls):
  `the_value_shows_above_the_bubble` (`250 Hz · +1.5 dB · Q 1.00` from the steps' words, above the bubble, gone
  with the release), `a_tone_update_waits_for_the_drag` (a write from elsewhere moves no bubble under the hand,
  and is drawn after the release), `a_device_switch_mid_drag_writes_nothing_to_the_new_device` (a second DAC with
  its own gain range; nothing posted to it, nothing posted by the drag after the switch, its bands unchanged,
  the curve drawn from it), `a_device_switch_without_a_drag_shows_no_curve` (both browsers: a switch with no drag on leaves `data-tune` off and
  the morph at 0; mutant, the grace given on every switch, killed), `a_bubble_is_grabbed_at_dpr_2` (the ring drawn at its place on a canvas of twice
  the size, a press 6 px away grabs, 16 px away does not). Mutants (run by hand, each killed): a guest tap that
  posts straight to the daemon; no word to the core; the switch handler off (the drag goes on writing, to the
  old device) and with the writer taking the shown device (writes land on the new one); the model read live
  under the hand; the gain map upside down; the pointer in canvas px at dpr 2.
- Q on the mountains (`node/nightdrive-tune.mjs`): `the_diamonds_sit_where_the_bell_is_at_half_its_gain` (the core's
  `responseDb` of the band alone at `lo` and `hi`, within 0.05 dB of half the gain; an axis end where it never comes
  down), `a_diamond_at_a_steps_half_point_picks_that_step`, `the_wider_the_diamonds_the_lower_the_q`,
  `a_flat_band_has_its_diamonds_at_the_analog_width`, `the_upper_diamond_stops_at_20_khz`,
  `a_diamond_drag_lands_on_the_step_it_names` (the core's `createDrag` pinch with `spread(q0 / qs[i])`), the
  diamonds' places on the curve (`the_diamonds_sit_on_the_curve_at_the_half_gain_points`,
  `a_flat_band_s_highlight_spans_the_analog_width_around_its_bubble`) and the morph's idle rule
  (`a_resting_pointer_lets_the_curve_go`). In a browser
  (`nightdrive_theme.py`, request log read by relative paths): `hovering_a_bubble_lights_its_bell_and_diamonds` (both:
  the diamonds sit ON the curve and are the band alone at half its gain by the core's own response; the diamonds
  are on the canvas; `the_highlight_is_the_curve_in_pink_between_the_diamonds`: pink on the curve inside
  lo..hi, its own colour well outside), `a_flat_band_highlights_the_analog_width_around_its_bubble`,
  `the_tuning_range_fits_the_scene` (the bubble at the largest gain inside the scene and clear of the top line at
  1280x800 and, in Chromium, 390x844; the smallest below the horizon on the floor, inside the scene),
  `a_resting_pointer_lets_the_curve_go` (on at once, off by 2.5 s, a 1 px move brings it back), `dragging_a_diamond_sets_q` (both: the drop at a step's half point writes that step's
  Q and only Q, to that band), `the_wheel_steps_the_nearest_bands_q` (both: one notch, one step, up narrows; the band
  nearest in x when no bubble is near; a wheel in the sky writes nothing),
  `the_wheel_and_the_pinch_keep_the_curve_1_5_s` (both for the wheel and the diamond; Chromium for the lifted
  fingers). Chromium: `a_diamond_is_grabbed_at_dpr_2` (dark centre under a pink edge on a canvas of twice the size;
  a press 6 px away grabs, 16 px does not), `a_pinch_sets_the_q_of_the_band_nearest_the_fingers` (real touch events:
  twice the distance halves the Q, of band 3 only, nothing else written), `a_first_touch_shows_the_curve_and_a_second_grabs`,
  a finger held on a lit bell's diamond keeping the curve past the grace and writing the step it is dragged to, a
  held diamond keeping the curve and the full rate with the pointer out of the band (mouse),
  `a_press_on_the_bubble_takes_the_bubble_beside_a_diamond` (narrowest Q, a diamond within 12 px: the press moves
  the band and never writes the Q; killed by the diamonds-first grab order). Every tuning case starts by putting the
  bands at known values (`reset_bands`), so the group passes in any order. Mutants (run by hand, each killed): the wheel's step sign
  flipped, the wheel without the grace, the wheel not limited to the mountains' band (it wrote from the sky), the
  diamond press in device px at dpr 2, the diamond's Q read from the wrong side, a diamond drag not counted as held,
  the pinch always on band 1.
- The neon sheet's markup (`node/nightdrive-draw.mjs`): `the_sheet_runs_mode_input_then_bands_beside_presets` (the
  sections in order, the presets in the side column, a modal dialog with a close key, nothing for no device),
  `a_device_with_no_tone_draws_no_mode_bands_or_presets`, `loudness_carries_its_round_minus_and_plus_and_strength`
  (round keys either side of the chip, the strength under the name, none while Loudness is off),
  `every_band_value_has_a_field_and_steppers` (15 fields, 30 steppers, each named), `a_guests_sheet_is_drawn_inert`
  (a guest, a read-only viewer, a page whose writes are off: every key `quiet` and `aria-disabled`, every field
  read only, nothing to save with), `nothing_in_the_sheet_has_a_style_attribute` (no style attribute, inline
  script or style in any model of the fixtures; a preset name is escaped). Mutants (run by hand, each killed): the
  viewer's inertness ignored, a style attribute on a heading, a band value with no `+`.
- The neon sheet in a browser (`nightdrive_sheet.py`, both browsers; the helpers it shares with `nightdrive_theme.py` are in `nightdrive_base.py`; the page's requests read from its own request
  log by relative paths; every case starts by putting the bands, the mode and the presets where it needs them, so
  the group passes in any order): `device_opens_the_sheet_and_focus_goes_in` (a modal dialog, the focus inside, the
  scene and the dash `inert`, two columns on a desktop and a tall window kept whole), `the_menu_device_row_opens_it`
  (Escape returns the focus to the menu button), `close_by_x_escape_or_a_tap_on_the_scene_and_focus_returns`,
  `the_scene_runs_dimmed_under_it` (brightness .45, saturate .8, draws keep coming), `the_band_table_steps_and_types_each_value`
  (a `+` on all fifteen values, a `−`, a typed value written as typed, Escape writing nothing),
  `holding_a_band_stepper_repeats` (the first repeat at about 380 ms, then about every 70 ms; a release, a cancel, a
  blur, a hidden page and a lost stream end it; a redraw keeps the key; let go at 10 s), `the_mode_and_loudness_strength_set`,
  `a_preset_saves_renames_and_deletes` (nothing deleted before the answer, Keep deletes nothing, a list of the
  person's own when the bands are on another preset), `the_sheet_holds_while_typing_or_a_list_is_open` (the list on a
  six-input device), `a_device_switch_with_the_sheet_open_redraws_it_for_the_new_device` (a held stepper and an
  open form for the old device write nothing, nothing at all reaches the new one), a real guest's sheet inert with
  every tap asked of the core, and the phone's sheet (30 % from the top, one column in the settled order, 44 px
  targets; from the right, `min(420px, 60vw)`, in landscape). Mutants (run by hand, each killed): the hold without
  its 10 s cap (and with a 9 s one: steps must still come past 10.2 s), a hold that outlives a switch and writes to the device then shown, the focus not returned, typing not
  holding the sheet, a guest's mode tap not asked of the core, the scene not dimmed, a delete without its question,
  Escape in a field writing the value, a list's Escape choosing an option.
- The rest of a device on the sheet, amps and other kinds (`node/nightdrive-draw.mjs`):
  `the_sheet_lists_every_control_the_model_has` (every control of every fixture once: a toggle a pill, a slider a
  scale with a knob and its centre, a readout its words, the power key and its switches), `the_facts_start_folded`,
  `a_loki_max_shows_its_slots_and_settings`, `an_amps_dash_has_power_and_temperature_gauges` (arcs filled by the
  state and the number, the model's minus read), `an_amp_never_morphs` (no bands, nothing tunable),
  `faults_are_listed_with_their_advice_and_turn_the_top_red` (the top line's tone is `bad`). In a browser
  (`nightdrive_sheet.py`, both): `a_slot_save_asks_first` (nothing posted before `Save`; then the slot with
  `confirm`), `the_facts_unfold_and_the_choice_survives_a_reload` (the fold kept under `svolder.theme.nightdrive.facts`),
  `an_amp_shown_alone_keeps_the_music_and_its_gauges`, `an_amps_meters_move_live_on_the_sheet` (the fake amp with its signal
  on: a needle changes at least twice in 3 s with the sheet left open, the same element), `every_control_writes` (a toggle, a scale by key and by the
  centre tap and a press along it, the amp's power key and switch), `identify_says_the_daemons_words`.

- The phone (`nightdrive_phone.py`, both browsers): `portrait_puts_the_scene_above_the_dash_panel` (412x915 and 360x780: the panel is its own rows, under 30 % of the window, to the window's bottom and edges; the scene's bottom is the panel's top within 2 px and the canvas fills it; the volume is in the scene, centred, under the device line and the EQ line and above the sun at rest; `−` `MUTE` `+` in that order on one row, the track, the transport and `DEVICE ▴` on one row in the panel; the EQ line holds the gains; no scroll), `the_portrait_volume_fades_with_the_hud` (upright, the volume, the device line and the EQ line reach opacity 0 and the panel stays at 1), `landscape_shows_the_scene_full_with_a_slim_dash` (844x390 and 667x375: the scene and
  its canvas are the window, the dash under a quarter of the height with every key inside, the volume above it),
  `the_menu_button_is_fixed_on_a_phone` (44 px, 8 px from the top and the right, the frame fixed, in both turns),
  `the_sheet_slides_up_in_portrait_and_in_from_the_right_in_landscape` (30 % from the top and the full width with the `nd-sheet-up`
  keyframes; `min(420px, 60vw)` against the right edge with `nd-sheet-side`), `turning_the_phone_keeps_an_open_sheet_open` (the sheet
  open with a control focused: still open, the same control focused and the other layout after a turn, and back),
  `touch_targets_are_44_px` (the dash, the sheet in Loudness and the menu button, in both turns: each is 44 px or a 44 px box about its middle
  lands on it; the round Loudness keys are 32 px to the eye and 44 px to the hand), `touching_the_mountains_tunes_on_a_phone` (the mountains
  lie in the window's top half; Chromium: a touch in the panel shows no curve, a first touch on the mountains shows it and grabs nothing, a second
  grabs the bubble and a move writes that band of this device; Firefox: the pointer over the mountains shows the curve, over the panel it does
  not), `no_sideways_scroll_with_long_names` (a 40-character device name with no break, a long track and a long preset: the page no wider than the
  window and no part of the HUD, the dash or the sheet past its edge at 390, 844 and 1100 px) and `a_tall_window_keeps_the_scene_whole`
  (1280x1600 and 2560x1440: the sun whole, the dash on screen along the bottom, the volume above the sun, no scroll). Each is red against a
  broken stylesheet (the scene's share, the volume's width, a 40 px key, a 4 px hit area, the sheet at 40 %, a window-wide volume, a name that does not
  wrap), the menu button against a header that does not pin it, the rotation against a sheet that closes on a resize, and the request log's
  band write against a wrong band.
- The controls' writes (`node/nightdrive-controls.mjs`, and `node/signal-controls.mjs` for Signal's): `a_scale_step_asked_after_a_switch_is_written_to_the_new_device_only` (a write to device A in flight, the
  device B shown, a step on B: B has its own run and nothing more reaches A) and `a_scale_keeps_one_write_in_flight_and_the_latest_waiting`.
- The states (`node/nightdrive-states.mjs`): `the_mood_of_each_state` (every state and the order they win in: a lost stream, then a page
  reading again, then a card, then no capture; a fault on a connected device and a guest are `ok`), `an_unreachable_card_has_try_now_and_the_seconds`
  (the title, Try now, `next try in 8 s`, `trying again now` at 0, no key and no seconds for a card that has none, a command in mono, the key quiet for a
  viewer who may not write, the words escaped, no `style`), `a_fault_keeps_the_controls_live` (the mood is `ok`, the top line says fault in red, no key of the
  dash is quiet, the faults with their advice are the sheet's) and `no_capture_says_why` (the line says the reason, a default when there is none). In a browser
  (`nightdrive_states.py`, both): `out_of_reach_stops_the_road_and_shows_the_card` (the device dropped: no floor row in 2.5 s, the sun's pixel grey, the mountains' edge
  within 6 % of the horizon, the card centred with its words, Try now and the seconds counting down, the dash inert and dimmed, Try now posting `connect` to this
  device), `a_lost_stream_freezes_and_fades_with_one_line` (no row, the pace slowed to 400 ms or more, the sun darker under the veil, one quiet line, the volume, the EQ
  line and the dash faded, the dash inert; the stream back brings it all again), `reading_again_and_resting_say_their_words`,
  `no_capture_breathes_the_curve_and_says_why` (`SVOLDER_FAKE_CAPTURE=off`: the ridge's edge within the breath of the tone curve at six columns for 8 s and moving by 0.6 to 2 times
  the breath, the floor's rows all 0.05 and rolling, the line's reason, a tap on `−` still lowers the volume), `a_guest_sees_the_scene_with_quiet_controls`,
  `a_refused_write_toasts` (the daemon refuses a rename onto a taken name, asked of the core from Night drive's page: the core toasts its words, nothing is renamed) and `a_card_while_faded_wakes_the_top` (the top faded, the device dropped: the top
  line's words and the card, the fade held off). A fault card on a live device cannot be made by the fake and stays node-only. A lost stream makes the page inert but its words (the scene, the dash, the sheet and the card are `inert`; the top line and the quiet line are not), asserted in `a_lost_stream_freezes_and_fades_with_one_line`. With no capture a guest's mountains rest at the baseline instead of taking the curve (the same page, a signed-in viewer then a guest: within the breath of the curve, then of the baseline), and with the breath amplitude at 0 the "it breathes" assertion goes red. `themes.rs`'s test `every_script_and_stylesheet_in_a_themes_folder_is_listed` fails for a file on disk that the list does not name.

- The driver (`themes/nightdrive/driver.js`, `contract.py` and `completeness.py`, which no longer leave Night drive out): `reach` shuts or
  opens the sheet as the target needs it (the volume, the mute, the input, an amp's power key, the transport and the cover are on the dash; every
  other control, the bands, the modes, the presets, the slots and the facts are on the sheet, whose folded facts it unfolds), scrolls to the word,
  scale or chip and says whether it is drawn and in view; `operate` taps a key (a step of the volume is a tap with no pointer, which is one step),
  presses the arrow keys on a scale, steps a band with its `-` and `+`, picks an option of the list a chip opens (a long input, the slot to save over,
  then the question's `save`); `shown` reads the words on the page (the outrun volume's reading, a band's field, a scale's value, the strength under
  Loudness, a fact's cell, the cover's address); `state` reads the core's state, the card and the toasts; `linkWords` reads the top line's left, where the
  device's link word stands beside its name while the link is not on (`the_link_state_is_in_words_beside_the_name`, found missing by the contract and
  fixed in `hud.js`). Proved by `contract.py` in both browsers (67 checks each) and by `completeness.py` in both (every target reached, operated and
  read back from the model route, one write each; every fact shown with the model's words) on the Gungnir 2 (36 targets, 36 facts), the amp (3, 14), the
  five-meter amp (3, 17), the mono amp (3, 14) and the Loki Max (19, 15): run with `WEB_CHECK_THEMES=ember,nightdrive`, the per-kind counts of targets and facts
  equal Ember's (the 36 `count equals` checks). The menu-button check accepts a page that does not scroll (Night drive's page never does: its root is the
  window), where it still wants the button pinned to the window.
- The top line with the link words (`nightdrive_theme.py`, both browsers, density 3): `the_top_line_with_the_link_words_has_room` (the device dropped, so the left reads
  `Gungnir 2 · not connected · USB 1 · 48 kHz` and the right the card's words: at 1280x800, 412x915 and 360x780 the two parts, the dB readout, the EQ readout and the
  menu button meet nowhere and the line stays inside the window; red against a left that does not wrap, `mutate_nightdrive.py`).
- A fade woken by focus alone (`nightdrive_theme.py`, `the_faded_hud_stays_in_the_tab_order_and_focus_wakes_it`, "a focus moved with no key or pointer wakes
  the faded HUD": a Tab already wakes it through the key, so the check moves the focus from the page with neither).
- A quick rise of the highs is one transient at any frame rate (`node/nightdrive-sky.mjs`, `a_quick_rise_is_a_transient_at_any_frame_rate`: the highs' average is a
  time constant, so a rise over 0.5 s gives one star at 30, 60 and 144 steps a second; the average taken per frame fails it).
- Mutants of Night drive's sources, run by hand with `node/mutate_nightdrive.sh` (nineteen, each killed by the harness named): the camera's K back to 12
  (`nightdrive-floor.mjs`), a band's column off by one, the ridge over -72 dB and the rows ignoring the tempo (`nightdrive-scene.mjs`), the rays every 6 s, a star
  every half second, a shower every 4 s, the highs' average taken per frame and the level's easing taken per frame (`nightdrive-sky.mjs`), the diamonds on the closed
  form and a grace of 0 (`nightdrive-tune.mjs`), the EQ line without the preset's name, a muted volume reading `-0.0` and four inputs drawn as chips
  (`nightdrive-hud.mjs`), the ear's moment every second and embers in Calm (`nightdrive-mood.mjs`), the wet road coming back with no window after an idle drop
  (`nightdrive-budget.mjs`), the heat's curve linear and the near edge's lift not fading (`nightdrive-floor.mjs`).
- Mutants of Night drive's page, run by hand with `mutate_nightdrive.py` (one worker, Chromium; each run names the cases it needs and runs them unmutated first,
  and a kill is a failure of a check the mutant names): focus not waking the faded HUD (`the_faded_hud_stays_in_the_tab_order_and_focus_wakes_it`), trouble letting the fade
  go on (`trouble_while_faded_shows_the_top_line`, `a_card_while_faded_wakes_the_top`), the analysis kept while hidden
  (`the_analysis_is_held_while_shown_and_let_go_when_hidden_or_unmounted`), reduced motion starting on Full
  (`reduced_motion_starts_on_calm_and_full_can_be_picked`, in `nightdrive_timing.py`), Still at the screen's rate (`still_draws_about_one_frame_a_second`), a dash hold that never
  repeats (`holding_plus_repeats_and_the_release_ends_it`), a diamond drag that writes nothing (`dragging_a_diamond_sets_q`), a pinch on band 1
  (`a_pinch_sets_the_q_of_the_band_nearest_the_fingers`), hit tests in canvas pixels (`a_bubble_is_grabbed_at_dpr_2`, `a_diamond_is_grabbed_at_dpr_2`), a device
  switch that lets a drag go on and write to the new device (`a_device_switch_mid_drag_writes_nothing_to_the_new_device`), a guest's tap that morphs the mountains
  (`a_guest_tap_on_the_mountains_says_why`), a first touch that grabs (`a_first_touch_shows_the_curve_and_a_second_grabs`), the sheet's focus not given back
  (`close_by_x_escape_or_a_tap_on_the_scene_and_focus_returns`), its columns swapped (`device_opens_the_sheet_and_focus_goes_in`), the menu button following the page
  on a phone (`the_menu_button_is_fixed_on_a_phone`), Space playing under the open menu (`space_key.py`, "with the menu open ... space presses nothing"), a 30 px picker
  close key (`effects_menu.py`, `the_picker_close_button_is_44_px_inside_the_window (nightdrive ...)`). Every "nothing posted" assertion has a mutant that does post, so its
  request filter is known to match the core's relative paths (`api/v1/...`): a question that sends on Keep or before the answer (`Keep deletes nothing`,
  `Keep writes nothing`, `a_slot_save_asks_first`), a band field that writes on Escape, a second tap on the input chip that writes, the core letting a guest's and a
  read-only page's call go out (`a_guests_tap_posts_nothing`, `a_guests_sheet_is_inert_in_a_browser`, `a_guest_tap_on_the_mountains_says_why`: a request brings the sign-in, so those
  checks read the page going away as a request that went out), a switch that writes and a first touch that grabs. Twenty-two mutants, 17 minutes 40 seconds in all.

## The quick gate

- The quick gate refuses after a change to `notes.py`, a smoke file, `effects.js` or `node/`, and
  lets a change to `tone_note.py`, `effects_menu.py`, `ember.js` or a new check outside the list
  through: it is for web changes that leave the smoke alone. Proved by `gate_refusal.sh` (both
  gates, once). Mutant: the whole directory in the list, killed.

## Mutants the node harnesses cover

A rule a node mutant covers needs no browser run, which makes it fast:

- Stamping while away (five edits), the pending stir standing while away, a click working while
  away: `node/mutate_director.sh`. The away reset, the away queue drop and the step guard are
  mutants of their own, killed by "away and lost"; the guards of `noteAt` and `pointer` are
  equivalent (away resets the watch, and what is ignored while away is forgotten on return).
- A scroll not stirring the notes: `node/mutate_director.sh`, killed by "odds" (scroll). The haze
  stirring the notes: killed by "idle". Meters feeding below red, or from the peak rather than the
  eased reading: killed by "meters".
- The pointer feeding far too much, or from the uncapped speed: `node/mutate_notes.sh`
  (`agitPointer: 35`, the `agitSpeedMax` clamp to 1.2), killed by the pointer tests.
- A zero size unguarded: the director's `usable(size)` makes the three guards redundant, and
  `node/director.mjs` "bad input" proves the size bounds (a zero or huge size gives a finite
  plan, an unusable canvas never simulates).

## Known survivors

Three browser mutants survive: "an earlier GL error turns the notes off" and "stamp has no halo"
(`mutate_notes.py`), and "meters iterates the readings" (`mutate_sources.py`). A check that kills
any of them would be new. "stamp has no halo" (`noteHalo: 0.25` to 0) survives on the tree before
the themes work too: the halo check in `notes.py` is a statistical fit of the drawn opacity per
unit of halo weight inside a wide band (-0.6 to -0.02), which a fit near zero can pass.
- The volume: a printed ruler linear in the device's steps (a step's place, the step at a place,
  held to the range), labelled where the dB crosses each multiple of 10 (20 under 600 px) read off
  the device's table with the real bottom mark kept and a label within 3 % of a kept one dropped,
  from its words when it has no dB; ticks every 2 dB; the risk zone and the cap at their steps.
  Proved by `node/paper-ruler.mjs` ("the_needle_fraction_follows_the_steps",
  "raw_at_a_place_rounds_and_clamps_to_the_range",
  "a_dac_ruler_is_labelled_every_10_db_from_its_real_bottom", "the_gungnir_ruler_reads_its_true_places",
  "a_narrow_ruler_is_labelled_every_20_db", "a_level_without_db_is_labelled_from_its_words",
  "the_risk_and_the_cap_stand_at_their_steps"), which also draws the section from every fixture (no
  `style`, no arrow) with the mute word following the state and a guest's words quiet.
- The volume in the browser: the needle sits at the reading's fraction of the ruler within 1.5 px
  and the level is written beside it; a drag along the ruler writes the steps it passes and settles
  on the one it ends on; a press elsewhere on the ruler moves the needle there; `louder +`,
  `- quieter`, `mute` and `unmute` write; the arrows, Page Up and Page Down move the needle, Home
  goes to the bottom and End posts nothing; a drag past a cap held in the config posts no step above
  it, settles at it and stops the needle at the cap's mark (Review Focus 5); a reading from elsewhere
  during a drag leaves the needle under the pointer and is shown within 3 s of the release (Review
  Focus 2, Chromium); Paper to Ember and back leaves one listener, one tap one write. Proved by
  `paper.py` ("the_needle_sits_at_its_value", "dragging_the_needle_writes_its_steps",
  "the_written_volume_words_write", "the_needle_takes_the_arrow_keys",
  "dragging_past_the_cap_writes_the_cap",
  "a_reading_from_elsewhere_during_a_needle_drag_lands_after_it",
  "switching_to_ember_and_back_leaves_one_listener"). Mutant (run by hand): the drag not holding the
  section, killed by the held-needle check.
- A state change while a section is held: the sheet does not turn inert under a gesture (it does
  when the hold ends), and the section is drawn from the newest reading after the release; a reading
  that fits the drawn ruler is put in place, so the focus stays. Proved by `node/paper-live.mjs`
  ("a_state_change_during_a_hold_neither_stills_the_sheet_nor_leaves_the_section_stale",
  "a_reading_of_the_volume_is_drawn_and_one_that_fits_the_ruler_is_put_in_place").
- The tone drawn: one point per band the model marks, at the marker's place (a band not read in full
  has none, Loudness none), the curve dashed and dim in Flat, the heading naming the mode and the
  preset (escaped; `no preset` when none is on) as words that open lists, the loudness strength
  with its two steppers only in Loudness with a reading, the band table with a `-` and a `+`
  round every frequency, gain and Q; no `style` attribute in any section of any fixture; and on a
  guest's or a read-only sheet every acting word is `quiet` and `aria-disabled` (Review Focus 3).
  Proved by `node/paper-draw.mjs` ("the_points_sit_at_each_bands_marker",
  "a_band_not_read_in_full_draws_no_point", "the_curve_is_dashed_in_flat",
  "the_curve_path_is_the_responses_line_in_the_plot", "the_heading_names_the_mode_and_the_preset",
  "the_loudness_strength_has_its_two_words", "every_band_parameter_has_its_two_steppers",
  "a_held_graph_is_drawn_from_the_held_band", "no_markup_carries_a_style_attribute",
  "a_guests_sheet_has_no_underlined_word").
- The tone in the browser: a point dragged by (+80, -30) px writes the frequency and gain steps
  nearest the release point and stands where they put it; the table's `+` and `-` write one raw
  step of the gain, Q and frequency; the arrows on a focused point step its frequency and gain and
  it keeps the focus; the mode word opens a listbox of the modes with the current one selected,
  ArrowDown and Enter write the next, Escape closes with no write and the focus back on the word, a
  press outside closes it and the word toggles it; the preset word lists the presets and a choice
  applies it; the strength's steppers write one step; a change from elsewhere during a point drag
  leaves the dragged point and the others where they were and is drawn within 3 s of the release
  (Review Focus 2, Chromium). Proved by `paper.py` ("a_band_point_drag_writes_the_steps",
  "the_band_table_steps_write", "the_mode_list_chooses_and_closes",
  "the_preset_list_applies_a_preset", "the_loudness_strength_steps",
  "a_tone_update_waits_for_the_point_drag"). Mutant (run by hand): the drag not taking the hold,
  killed by the held-point check.
- The controls, the presets and the device's own presets drawn: every control of every fixture
  (a DAC, a Loki Max, a Saga, a phono stage, the amps) is written by its kind and key (a choice of a
  few values as its label and values with the current one `on`, a longer one as a word that opens a
  list, a switch as `on`, `off` or a dash, a level as a `role=slider` scale with its range, its
  reading, a pencil mark at the reading's fraction and the value that writes the centre, a readout
  with `faint` when the device says so); the tone presets say which is on, name each preset (one's
  own in italics) and offer `save as...` and `rename or delete...` only when the configuration may be
  written (the second only when a preset of one's own exists), with the note when it may not; the
  device's presets offer the slots, `save the bands to...` only where the device allows it (never to
  a guest) and the built-in settings with the matching one lit. Proved by `node/paper-draw.mjs`
  ("every_control_of_every_fixture_has_its_written_word",
  "the_presets_offer_save_rename_and_delete_only_when_editable",
  "the_device_presets_offer_slots_save_and_built_ins"; the guest check
  "a_guests_sheet_has_no_underlined_word" runs over these sections too).
- The controls, presets and questions in the browser: a choice word writes its option and the pencil
  underline moves to it while the others stay dotted; a switch word flips and reads the new state;
  the balance scale's mark dragged to 25 % writes raw steps ending on the one there, the arrows step
  it and a tap on the value writes the centre; a preset is applied by its word; `save as...` opens a
  ruled input (40 characters, required, named `Preset name`) with `save` and `cancel` and writes the
  name; `rename or delete...` lists one's own presets, rename asks for the new name and writes it,
  delete asks in Paper's words with the safe answer `keep` focused, keep posts nothing and delete
  removes the preset; a refused form (a name already taken) toasts, stays open and enabled with the
  typed text; a Loki Max's slot save asks `Save the bands to Preset 2? What the device holds there is
  replaced.` and posts `{"slot": 2, "confirm": true}` only after `save` (keep and cancel post
  nothing), a built-in setting applies and lights; an open paper list keeps its place and focus while
  another section and its own section's reading change, and Escape then shows the newest (Review
  Focus 2, Chromium); a control's list holds the controls the same way (Chromium); on a phone a
  touch beside a point is the page's scroll and writes nothing, a touch on the point's own area drags
  it (Chromium). Proved by `paper.py` ("a_choice_word_writes_its_option", "a_toggle_word_flips",
  "the_balance_scale_drags_and_centres",
  "presets_apply_save_rename_and_delete_with_papers_questions",
  "a_slot_save_asks_then_writes_with_confirm", "a_built_in_setting_applies",
  "a_refused_form_stays_open_with_its_text", "an_open_paper_list_survives_a_part_update",
  "a_controls_list_holds_its_section", "a_touch_beside_a_point_scrolls_and_on_it_drags",
  "a_device_switch_closes_what_is_open").
- Holds: a scale's hold is not released by a slow answer after the next gesture took it, a hold
  taken around a call is given back if the call throws, and another device shown tells the
  switchers once (a form, a question or a list open for the old device is closed and its holds
  reset). Proved by `node/paper-live.mjs` ("a_slow_answer_does_not_release_a_hold_the_next_gesture_took",
  "a_hold_taken_around_a_call_is_given_back_if_it_throws",
  "another_device_shown_tells_the_switchers_once_and_lets_a_reset_release_the_holds"). Mutants (run
  by hand): the generation check removed, and the switchers not called, each killed.
