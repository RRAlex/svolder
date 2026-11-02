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
- A click on the volume knob's ring jumps to that step, and the rest of the knob does not:
  `knob_click.py`. Mutants: `mutate_webui.py`.
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
  holder told what was said, a doubling wait): `node/core-loop.mjs`.

## Ember's drawing

Ember (`bins/svolderd/web/themes/ember/draw/`) draws the page's parts from the JSON model in the
browser, and what it draws is the markup the templates give today: the page's styles and scripts
read the same DOM.

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
