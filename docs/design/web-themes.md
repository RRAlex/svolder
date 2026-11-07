<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Web themes

The web page is a frame, a core and a theme. A theme is a whole experience of the same device, not
a colour scheme: it draws every control and every fact from one JSON model, in its own way. This
page is the map of that arrangement: what each of the three owns, what a theme must export, what
the core gives it, how it is loaded and chosen, how the checks hold every theme to the same
contract, and how to add one. The details sit in the pages it points to: [the frame](frame.md),
[the page and its stream](web-ui.md), [Ember's effects and Arrange mode](themes-and-effects.md),
[the checks](testing.md). It sits below [the map](../architecture.md).

The page ships four themes. **Ember** is the test theme: it is complete and it looks and behaves as
the page does, so it proves the core against every kind of device. **Paper** is a sheet of squared
paper in light and dark, with written controls and handwritten notes ([web-paper.md](web-paper.md));
building it on the core alone found seven gaps in the core, which that page lists. **Signal** is the
sound's way from the player to the ears drawn as one line, dark only, with the chosen hop opened
below it ([web-signal.md](web-signal.md)); it found three gaps: two in the page model's computer
facts and the order in which the daemon shuts the web server and the host service down. **Night
drive** is the music drawn as an outrun road, dark only, with the volume as a large neon number and
the rest of the device on a sheet ([web-nightdrive.md](web-nightdrive.md)); it found gaps in the
canvas loop, the analysis rules, the menu and the picker, and in the reserved keys, which that page
lists. Its computer side is two keys by the menu button (the panel's top row on a phone) that turn the volume
figure, the dash's keys and the curve on the mountains into the computer's
([host-control.md](host-control.md#night-drive)).

## Who owns what

| Layer | Folder | It owns |
| --- | --- | --- |
| The frame | `web/core/` (`frame.js`, `menu.js`, `picker.js`, `switch.js`, `frame.css`) | the sign-in page, the menu button and its menu, the picker of themes, the popups, the reserved keys `T` and Space, the empty theme root `#theme` |
| The core | `web/core/` | the stream, the model, the actions, the sender and its refusal words, the climb's popup, the per-theme preferences, light and dark, the lifecycle of a canvas, the analysis stream, the tone curve's maths and gestures, the registry |
| A theme | `web/themes/<id>/` | everything drawn in `#theme`: its markup or canvas, its style sheet, its effects, its menu rows, the sections it lets a person arrange, its test driver |

The frame sits above the theme by `z-index`, in a cascade layer that no theme's sheet can outrank,
so no theme can hide the way back to the picker ([frame.md](frame.md)). A theme draws the header,
footer, device list and sign-out its own way, and the menu carries a Device row and a Sign out row
as well, so a theme that leaves one out never strands anyone.

## What a theme exports

A theme's module calls `registerTheme(theme)` (`core/registry.js`) when it is evaluated. The
object has:

- `id`, `label`: the folder's name and the name a person reads.
- `needs`: the live feeds it wants, from `meters` and `host` (sent on the event stream) and
  `analysis` (the music's frames, which the core opens and closes). The model is always sent.
- `schemes`: `dark`, `light` or both. A theme with one scheme gets no switch.
- `supported()` (optional): `{ ok, why }`; a theme that says no is replaced by Ember.
- `mount(root, core, { signal })`: draws into `root` and returns its `unmount` (a function or an
  object with one). The signal is aborted when the theme unmounts or throws, so every listener the
  theme made with it goes too. Unmounting leaves nothing behind.
- `menuButton`, `menuRows(core)`, `menuOpening()` (optional), `preview(box)`: what it gives the
  frame ([frame.md](frame.md#what-a-theme-gives-the-frame)).
- `sections` (optional): the sections Arrange mode may move, if the theme arranges.
- `driver`: the test driver of the contract checks (below).

A theme may draw with DOM, SVG, canvas or WebGL. It owns its style sheet, which must not name the
frame's elements (`make theme-css-check` fails when it does).

## The model and its events

The daemon builds the model from the same view models the TUI's words come from, so a value reads
the same everywhere and a guest sees what a guest may. The page carries the first snapshot in a
JSON block, so a theme draws at once with no blank screen; the stream then sends `model`, `part`,
`volume`, `level`, `meters`, `strip`, `fleet` and `host` events ([web-ui.md](web-ui.md#live-updates)).
`core.model` is the store that keeps them:

- `page()` is the whole model: `viewer` (`signed_in`, `guest`, `writes`), `fleet` (every device,
  its link and the declared hops), `shown`, `device`, `followed`, `card`, `host`, `strip`.
- `page().host` is the computer's player (`track`, `path`) and `route`, the route's facts read-only:
  `sink` (`words`, `rate_hz`, `format`, `volume_pct`, `muted`), `player_rate_hz`, `host_eq`,
  `eq_preset` (the applied preset's name, from the saved computer EQ state, only while the filter
  is in the player's path, on, and in front of this sink), `eq_autoeq`, `others_mixing`; `null`
  while the route is not read.
- `shown()`, `device(url?)` and `part(name, url?)` read a device and one of its parts: `link`,
  `card`, `volume`, `controls`, `meters`, `power`, `tone`, `presets`, `device_presets`, `facts`,
  `playing`.
- `on(event, listener)` hears `model`, `part`, `volume`, `level`, `meters`, `strip`, `fleet`,
  `host` and `state` (`open`, `lost`, `resting`, `waiting`).

The model describes a control by its kind: a level, a choice, a toggle, a slider, an action, a band
set, a meter, a power state. Each carries its words, range and state. A theme draws the kinds, not
the devices, so a control a profile adds later appears in every theme without a change to it. The
model holds no geometry. The stream follows the device shown and, when a theme asks, others
(`core.stream.follow(urls)`), so a theme may show several devices at once.

## What the core gives a theme

`core` is one object, and `contract.py` pins its members.

| Member | What it is |
| --- | --- |
| `model` | the store above |
| `actions` | every write, as one call: `setVolume`, `stopClimb`, `setControl`, `setMode`, `setBand`, `setLoudness`, `recallPreset`, `savePreset`, `renamePreset`, `deletePreset`, `recallSlot`, `saveSlot`, `applySetting`, `connect`, `identify`, `press`, `signOut`, `signInPrompt`, `showDevice` and `hops` (`add`, `edit`, `remove`, `move`) |
| `prefs` | the mounted theme's own preferences, kept in `localStorage` under `svolder.theme.<id>.<key>`, every access guarded |
| `scheme` | light or dark for the theme: follows the system, with the person's word kept |
| `lifecycle` | `watchVisible` and `canvasLoop` (a loop that pauses when hidden, follows the size and the pixel density, and draws one still frame under reduced motion; `reduced: 'theme'` keeps it running under reduced motion for a theme that draws its own, and `pace()` gives the least milliseconds between draws, waited on a timer from 100 ms) |
| `analysis` | the host's analysis frames, shared by every holder, and `viz-model.js` that decodes and smooths them and schedules the beat |
| `curve`, `drag` | the tone control's maths and the drag gesture as a model fed plain coordinates, so any drawing can edit a band |
| `words`, `clock` | the daemon's words for ages, the signal and the Identify answer; the Now playing card's arithmetic against the daemon's clock |
| `frame` | `anchor(el, { fixed })`, `realign()` and `fade(on)` for the menu button |
| `keys` | the reserved keys (`T`, Space) |
| `title` | the browser tab's title for every theme: `<device> · connected — svolder` (`reconnecting`, `reading again`, `resting`, the lowercased title of a card for a device out of reach, `fault`); `add(fn)` puts a theme's words after the status and returns the function that takes them away; `statusOf(page, state)` is the status word for a theme's own trouble line |
| `hold` | `repeat(event, { root, step, … })`: a pressed `−` or `+` steps once and, held, repeats after `HOLD_AFTER_MS` (380) every `HOLD_EVERY_MS` (70), up to a cap of 10 s; it follows the button of the pressed one's id through a redraw and ends at the pointer's release anywhere, the window's blur, the page hidden and the other ends the node harness lists |
| `registry`, `climb`, `toneNote`, `stream` | the registry of themes, the raised-volume popup, the tone mode's note, the stream |

Every action toasts a refusal in the daemon's words and reads the device again, so what the
device holds is shown, not what was chosen. A guest's call sends nothing and says why. The volume
goes one at a time with the newest waiting and is held to the cap.

## Safety stays in the daemon

A theme gets no way round a safety rule, because the rules are not in the page. The volume rule
and its three-quarters glide, the confirmations (a slot's save needs `"confirm": true`), write
pacing, gone-device handling and the sign-in and guest limits are the daemon's ([daemon.md](daemon.md),
[web-ui.md](web-ui.md#writes)). A theme reaches a device only through the core's actions, which
wrap the JSON writes. What a widget does (Ember's ring click on the knob, the sticky 0 dB on a
curve drag) is that theme's; a rule that protects the listener is the core's.

## Loading and the choice

The choice is per browser: `localStorage` and the cookie `svolder_theme`, which lets the server
paint the right frame on the first request. `?theme=<id>` sets it and is kept. The server lists the
themes in `THEMES` (`bins/svolderd/src/web/themes.rs`), which carries each theme's id, label,
blurb, module, style sheets, head scripts and metas, schemes, `color-scheme`, `theme-color` and the
files it serves. `switch.js` loads a chosen theme's files and mounts it; a theme that fails to
load, cannot run or throws is replaced by Ember with a popup that names it. The whole account is in
[frame.md](frame.md#the-choice-loading-and-the-fallback).

## The menu, the picker and `T`

The menu button opens one Theme row, then the theme's own rows, then Device and Sign out. The Theme
row, or `T`, opens the picker: a card for each listed theme, drawn by the theme's `preview(box)`,
the one in use marked, a close button at its top right and a foot that says how to get back (`T`
and `?theme=ember`). Escape, a click outside the sheet and the close button close it. No theme binds
`T`. The menu and the picker are as tall as the visible window (the visual viewport, `dvh`) and
scroll inside themselves, so on a phone every theme and every row can be reached. The menu never
grows with the number of themes ([frame.md](frame.md)).

## Arrange mode

Arrange mode is Ember's and not the core's: Paper's order, for example, is part of its look. Ember
lists its sections, keeps an order per layout and per browser, and offers the mode from its menu
and the key `A` ([themes-and-effects.md](themes-and-effects.md#arrange-mode)). A theme that
wants the same exports `sections` and builds its own mode.

## Every theme is complete

A theme presents every control and every fact of every kind of device (DAC, amp, preamp) and never
sends a person to another theme for a missing piece. What stays a theme's choice is decoration:
effects, a visualizer, how things are worded and laid out.

`core/contract.js` makes that checkable. `targetsOf(device)` lists everything to operate (the
volume and mute, each control, the power switch, each band's frequency, gain and Q, the mode, the
loudness, a preset, a slot, a built-in setting, a transport button when there is a track) and
`factsOf(device)` lists every fact to show. A theme's `driver` lets a check do what a person does:

- `reach(target)`: show it (scroll, open a fold) and say whether it is there;
- `operate(target, change)`: press it, key it or pick its value;
- `shown(target)`: the words the theme shows for it;
- `state()`: `{ lost, waiting, resting, toasts }`;
- `linkWords()`: the words beside the shown device's name that tell its link state, or nothing.

A driver acts on the theme's own DOM and never calls an action itself, so what the check sees at
the daemon went through the theme's handlers.

## What every theme does the same way

Completeness says what a theme shows; these lines say how the same thing is done wherever a person
learns it once. Each line names its check, or says which theme does not keep it yet.

- **The frame's menu button is the same in every theme**: 44 by 44 px, 8 px from the window's top
  edge and 8 px from its right edge, at every width, pinned to the window (it stays there when the
  page scrolls) on an opaque backdrop in the theme's own colour (a plain background, no blur), so it
  reads cleanly over scrolling content. Each theme's header keeps that corner clear: nothing the theme draws stands under
  it, for a guest too (Ember's Sign in stands left of it). The menu scrolls inside a short window.
  Checked by `frame_button.py` (every listed theme, signed in, guest and with a long device name, at
  1280x800, 768x1024, 412x915 and 360x780: the exact rectangle, nothing under it at the top of the
  page, the same rectangle and an opaque backdrop after a scroll) and, for the menu's scrolling,
  `effects_menu.py`. Kept by all four.
- **Space plays and pauses the computer's player in every theme.** The frame's second reserved key:
  Space alone, not a repeat, with the focus neither in a field it would type into nor on a button, a
  link, a summary, a select or an element with a role Space activates (button, slider, switch,
  checkbox, tab, option, menu items), and no menu, picker or modal dialog open, posts one press of
  `pause` (while the player plays) or `play` through `core.actions.press`, and the page does not
  scroll. Night drive's sheet counts as a modal dialog. Nothing where the page has no player or the player has no buttons. Checked by
  `space_key.py` (all four themes) and `node/frame-keys.mjs`.
- **The device's connection state is a mark beside its name, and every state but connected is also
  in words**, never by colour alone. The words are checked in every theme by
  `contract.py` (`the_link_state_is_in_words_beside_the_name`, through the driver's `linkWords()`):
  a device taken off the air has words beside its name that it did not have while connected. Ember
  keeps the whole line, a dot after the shown device's name (named for a screen reader, not a tab
  stop, following the device after a switch), the word drawn unless connected (`ember_link.py`).
  Paper says the state in the words under its title (always, `, connected`), with no mark yet.
  Signal says it under the device's name on the line (the card's title) with the hop's dot, not the
  link's own word. Night drive says it in its top line, beside the device's name, while the link is
  not on (`not connected`).
- **The volume's `−` `Mute` `+` in that order, held repeats (the core's hold: after 380 ms, every
  70 ms, a 10 s cap), muted reads `−∞ dB`.** Signal keeps all of it (`signal_theme.py`:
  `the_volume_buttons_are_in_embers_order`, `holding_plus_repeats`, the muted words). Ember keeps
  all of it (`knob_click.py` for the order and the hold; `ember_link.py`
  `the_muted_volume_reads_minus_infinity`). Paper's exception: it writes `− quieter`, `mute`,
  `louder +` as words, says `muted` when muted and does not use the core's hold; this is a decision
  of its style, not a gap to close. Night drive's dash keeps all of it (`nightdrive_theme.py`,
  `the_dash_runs_minus_mute_plus_input_track_transport_device` and
  `holding_plus_repeats_and_the_release_ends_it`).
- **Now playing: the title, the artist, the transport as inline SVG, and the cover when there is
  one**, each theme styling it (Paper draws it in monochrome). The cover is a fact in
  `factsOf(device)`, so `completeness.py` fails a theme that does not show it. That pass runs at
  1280 by 900 in Chromium with Ember's sections in their usual order; Ember drops its cover in a
  Now playing cell under 400 px wide (an arranged band of three sections), where the fact is not
  shown and the check does not look. With no cover Paper and Signal draw no box and leave no gap; Ember keeps the
  box empty so its card does not move. Each theme's own check covers the cover going and a new
  track (`signal_theme.py`, `paper.py`, `playing.py`). Transport as inline SVG: Ember, Signal and Night drive.
  Night drive shows the cover in its dash, between the input and the title, and draws no box when
  there is none (`nightdrive_theme.py`, `the_dash_shows_the_cover_when_there_is_one`).
  Not yet: Paper's transport is written words, by its design.
- **The tone curve: drag a point for frequency and gain, the wheel or a pinch for Q.** Ember
  (`tone_drag.py`, `tone_touch.py`) and Signal (`signal_theme.py`: `a_point_drag_sets_a_band`,
  `a_pinch_sets_the_q`) keep it. Night drive keeps the drag, the wheel and the pinch on its mountains,
  and a diamond for Q (`nightdrive_theme.py`: `a_bubble_is_grabbed_at_dpr_2`,
  `dragging_a_diamond_sets_q`, `a_pinch_sets_the_q_of_the_band_nearest_the_fingers`). Paper has the drag and the wheel (`paper.py`) and no pinch yet.
- **The tab title comes from the core** (`core.title`: `<device> · <state> — svolder`): no theme
  writes its own. Checked by `node/core-title.mjs` and `core_rows.py`
  (`the_tab_title_follows_on_every_theme`: Ember, Paper, Signal and Night drive). Kept by all.

## The checks

One set of checks runs against every listed theme ([testing.md](testing.md)):

- `contract.mjs` (node): the lists above and the actions' bodies.
- `contract.py`: a short pass per theme in each browser, under five minutes: volume up and down,
  mute, a band, a preset, a device lost and back, a refused write, resting and waking, the menu
  button present and `T` opening the picker, also after a fade, `?theme=` switching, and the same as
  a guest. It checks what the fake daemon received and what the theme showed, never pixels.
- `completeness.py`: every target and fact on a DAC, an amp, a five-meter amp, a mono amp and the
  Loki Max (the track's cover is a fact, shown by every theme).
- `frame_button.py`: the menu button is the same rectangle in every theme, with nothing under it.
- `ember_link.py`: Ember's connection dot, its words for every state, its place after a switch, and
  the muted volume's `−∞ dB`.
- `core_rows.py`: what the core holds for themes that need it: a second device followed, a declared
  hop, the menu's fade, a band moved from plain coordinates, light and dark, a canvas loop that
  pauses, a theme that unmounts clean.
- Ember keeps every check; they prove the browser-drawn page is the page. A theme's own
  browser pass stays short. Paper's own are in [web-paper.md](web-paper.md#the-checks-and-the-mutants), Signal's in
  [web-signal.md](web-signal.md#the-checks-and-the-mutants) and Night drive's in
  [web-nightdrive-checks.md](web-nightdrive-checks.md#the-checks-and-the-mutants).
- `contract.py` and `completeness.py` run every listed theme; `WEB_CHECK_THEMES` (a comma list)
  limits them while one is being worked on, and an unlisted name fails the pass.

## How to add a theme

1. Make `bins/svolderd/web/themes/<id>/` with a module `<id>.js` that calls `registerTheme` with
   the object above, its style sheet, and a `driver`.
2. Add one entry to `THEMES` in `bins/svolderd/src/web/themes.rs`, with every file of the folder
   in `files` (a name not listed there is never served).
3. Keep the style sheet off the frame's elements, use only the page's own origin (the policy is
   `default-src 'none'` with `'self'`), write no inline script or `style=` attribute and add no
   build step.
4. Draw every control by its kind from `core.model` and write through `core.actions`.
5. Put the theme's look in tokens, set the `--frame-*` tokens so the button matches, and give
   `preview(box)` a small picture.
6. Run `make check` and the contract checks: they pick the theme up from the page's list. The
   completeness check lists the themes it expects (`listed` in `completeness.py`); add the new id.
   A light and a dark theme sets its `--frame-*` tokens for both and gives `theme_color_dark`.
7. A gap in the core is fixed in the core, not worked round in the theme. A theme that cannot fix
   it in the core says so in its page's "found missing" section (each theme's page has one).

[Back to the map](../architecture.md).
