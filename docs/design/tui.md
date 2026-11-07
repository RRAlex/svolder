<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The terminal UI

`svolder tui` (and bare `svolder`) is the terminal front end. This page is how it is built: the
state it keeps, the screens and boxes it draws, the key table and the three rules every key
follows, the look (roles, palettes, size tiers), and the decoration laid over the screens (the
intro, the squirrel, the animation layer with its glow). It sits one level below
[the map](../architecture.md), whose life of a volume change follows one key press through this
code; what each screen shows and each key does for a user is in the [TUI guide](../usage/tui.md),
and the words (role, tier, intent) are in the [glossary](../glossary.md). The web page's
decoration and the effects it shares a name with are in
[themes-and-effects.md](themes-and-effects.md).

## How it is built

Three layers, from the plain to the dirty:

- **`App`** (`bins/svolder/src/tui/app.rs`) is the state: the readings heard, the screen, the pane,
  the row focused, the boxes open over the screen. It touches no terminal, no radio and no clock of
  its own. `App::apply` takes an update from the backend, `App::key` and the mouse helpers turn a
  press into a list of `Intent`s (`app/intent.rs`: a write, a refresh, a device switch, a change
  to the configuration), and the read-only views (`faults`, `peak`, `level`, `placed`) are what a
  screen draws.
- **`render::draw`** (`render.rs`, with `screens/`) draws one frame from an `App`, a `Theme` and a
  `HitMap`. It decides which screen shows, or the one message a terminal under 80 by 24 gets
  (`MIN_WIDTH`, `MIN_HEIGHT`). Every screen records the rectangle of each row and widget it draws,
  and `mouse::resolve` turns a click back into the same helpers a key uses.
- **The loop** (`run.rs`, with `run/events.rs`, `run/input.rs`, `run/look.rs`) owns the
  terminal and the backend. It reads a key, a mouse event, a device update or a tick, hands it to
  the `App`, carries out the intents (`execute`), draws a frame, then lays the animation layer
  over it. A write is sent without waiting for its echo; the echo arrives on the update feed like
  any reading, so a held key is never held up by the write before it.

A device is opened through the same `Backend` every verb uses (the map says how), and a device
switch closes it and opens another through `Opener::reopen`, the path a start takes, so starting
on a device and switching to one cannot drift apart. The loop never prints: the alternate screen is
up, so anything an open has to say becomes a toast. A joined TUI reads the player, the path and the
computer's controls from the daemon, else its own: [host-audio.md](host-audio.md#a-joined-tui), [host-control.md](host-control.md#the-tui).

### The TUI always starts

Opening needs neither a reachable daemon nor a device. A start ends only on a quit key, a terminal
that cannot be claimed, or a configuration file that does not parse (opening on defaults would let
a Settings save overwrite the file the user must fix). Otherwise `Holding` says why there is no
device: `Waiting` (the daemon holds it and has not found it; the device screen shows a waiting
panel that fills in by itself) or `Nothing` (the Devices screen shows the failure and the command
that fixes it). The menu, Devices, Settings, help and the device editor work in both. With nothing
configured, the setup wizard (`wizard.rs`, `screens/wizard.rs`) runs first and can be left for an
empty Devices screen.

## Screens and boxes

`Screen` (`app.rs`) names four screens the user moves between, and two names that are never shown:

| Screen | Tab | Key | What it is |
| --- | --- | --- | --- |
| `Device` | Controls | `c`, `Esc` | Control rows, tone summary, Status (`screens/device.rs`). |
| `Tone` | EQ | `e` | Response curve and band table (`screens/tone.rs`); needs a tone control. |
| `Telemetry` | Telemetry | `t` | Every reading with its age and rate (`screens/telemetry.rs`). |
| `Devices` | Devices | `d` | The configured devices, to switch, add, edit (`screens/devices.rs`). |
| `Settings`, `Setup` | none | `s` | Names for the key table only: a box and the wizard. |

A box is modal: it is drawn over the screen, which is dimmed round it, and it takes every key until
it closes, so a letter typed into a name or a number is that character and never the key it is
elsewhere. The boxes are the quit question (`screens/quit.rs`), the menu `Esc` opens on the device
screen (`screens/menu.rs`), the help overlay (`screens/help.rs`), the Settings box
(`screens/settings.rs`), the preset list (`screens/presets.rs`, `boxes/presets.rs`), the device
editor (`boxes/editor.rs`) and a band's edit line (`boxes/edit_line.rs`). The toast
(`boxes/toast.rs`) and the tooltip (`boxes/tooltip.rs`) are not modal. `Overlays`
(`app/overlays.rs`) holds which are open.

The Settings box has five categories, in tabs on its top border: Look, Effects, Behaviour, Host
audio and Connect (`CATEGORIES` in `screens/settings.rs`). A row of the first four is one key of
`[tui]`; each row asks for one `TuiChange` (`app/intent.rs`), which the loop applies to the running
theme and animation layer and writes back to the file at once (`Config::update_tui`, called from
`run/look.rs`). Connect holds the daemon's `[mcp]` and `[mqtt]` sections and the Web UI box
(`boxes/web_box.rs`) instead: each row asks for a `ConnChange`, is written key by key and followed
by a `reload`, and the token row is masked.

### What the tone and graph screens add

The EQ screen draws the summed response of the bands' peaking filters (the maths is in
`svolder-profiles`, see [tone-control-and-presets.md](tone-control-and-presets.md)) in braille,
coloured by frequency, with each band's point marked. The curve draws where a band is going (a step
a key or a drag has asked for) and, while the edit line is open, at the steps its fields hold; the
band table shows only what the device confirmed. Dragging a point is `curve_drag.rs`: a left drag
sets frequency and gain, each axis on its own until the pointer leaves the press's column or row;
a right drag sets Q. A level bar takes the mouse too (`glide.rs`). One-row history graphs
(`widgets/graph.rs`, over the history types in `svolder-profiles`) show readings and the host
capture; the daemon keeps the connection strip's history so every front end reads the same one.

### What the Status pane and the Devices list say about the device

The facts about a device that no reading carries (its product and product id, the profile it is read
through, how strong its signal is, whether its readings are stale or its passphrase refused) reach
the TUI as one `DeviceRow` (`backend.rs`), filled from the daemon's `DeviceInfo`, or, in a direct
session, from the advertisement the session opened on (`DirectBackend::own_row`; the strength is an
advert reading stamped at open). The Devices list holds a row per device; the open device's own row
is `Session::facts`, taken when a device opens, whenever the link changes (one question to the
daemon that also judges the passphrase note, `Backend::own_facts`) and with each relist of the
Devices screen, which is the only poll the TUI has, so the Status pane's `Device` row and its `Link`
row's flags are as fresh as those events: they are not polled while the device screen shows. With no
facts the pane has no `Device` row at all, so a frame drawn without a backend is as it was.

Nothing here formats a number or a sentence of its own. The signal is `svolder_ipc::Signal`'s
`describe` on the Status pane (`−62 dBm advert, 3 min ago`) and `describe_short` in the list column
(`−62 dBm 3 min`), always with the proper minus (`svolder_ipc::SIGNAL_MINUS`); the two link flags
are `format::link_flags`, whose words (`passphrase refused`, `readings stale`, after `connected · `
or `disconnected · `) the web page repeats. A false boolean readout is its label with the web's
false word (`not connected`, `unlocked`, `no`: `Control::flag_words` in `svolder-profiles`, which
the web page takes its words from too), dim: the one way a readout can leave the line is not having
been heard yet. On a pane too narrow for the whole first line the dim false readouts go first, the
last one first, so the readouts that read true, the firmware builds and the faults keep the room
they had without them. When even dropping every dim readout cannot fit the line, the faults are cut
last.

`b` asks the unit to show itself: it acts on the device, so it is a lowercase key, live on the
device screens and on the Devices list, where it names the highlighted row. It is dimmed, not
removed, for a profile with no identify, and pressing the dimmed key still goes to the daemon (or
the direct session), which is the authority and answers with the refusal as a toast; the Devices
list reads other rows' profiles from the built-in ones only, so a user profile's own
`identify = false` can leave the key lit there until it is pressed.

Rows drop their facts from the right, whole, and the list drops its `signal` column before it
squeezes a name, a product or a state: the signal is the fact that moves, and the one a narrow
terminal can best do without.

## The keys

### One table

`keys.rs` holds one table, `HINTS`, of every key `App::key` reads: its code, its label, what it
does, which screens list it, when it is worth pressing (`When`), its group, and, for an uppercase
key, the setting it toggles. The help overlay, the setup wizard's key row, the idle advice (`a`
with no fault) and the documentation are all drawn from it (`keys::all`, `hints_for`, `shared`,
`only_on`), so none can name a key that does not exist or miss one that does. A key that is not
worth pressing now (an EQ key on a device with no tone control, `b` on a device whose profile
offers no identify) is dimmed with its reason, never left out.

### Dispatch order

`App::pressed` (`app/dispatch.rs`) tries, in order: Ctrl-C (quit at once, past every question),
Ctrl-R (fetch the AutoEq index again, only while the preset search box is open, whose letters are
all the query's; any other chord with control or alt asks for nothing), the quit question, the
menu, the help overlay, the Settings box, the device editor and its removal question, the preset
list, a band's edit line, `Esc` ending a curve drag, and only then `App::on_screen`. That tries
`Tab`, then the keys every screen shares (`global_key`), then the screen's own (`device_key`,
`tone_key`, `telemetry_key`, `devices_key`). Before any of it, `App::key` stops a volume climb and
lets go of a held bar, so a key never lands on top of a motion the user did not mean to continue.

### The three rules

Every key in the table follows one of three rules, which the help overlay opens with
(`RULES` in `screens/help.rs`):

1. **Lowercase letters act on the device, or open a screen or the Settings box.** `m` mutes, `i`
   flips phase, `o` flips NOS, `r` refreshes; `e`, `t`, `d`, `c` change screen; `s` opens Settings.
2. **Uppercase letters toggle a TUI mode and save it to the config.** `E` effects, `M` mouse, `H`
   hue drift, `R` rainbow scroll, `F` runner, `V` title meter. Each turns the row the Settings box
   shows for the same switch (`keys::setting_turned_by`), so the key and the box never give two
   answers. One exception: on the Settings box's token row (`When::TokenRow` in `keys.rs`) `R`
   makes a new token, `c` copies it and `v` shows it. They are bound to that row, not to a mode of
   the TUI or to the device, so `every_screens_keys_follow_the_three_rules` skips such keys.
3. **Symbols and navigation keys are the same on every screen.** The arrows, `Tab`, `Enter`, `Esc`
   and the symbols (`+ = -`, `[ ] , .`) mean the same wherever they act.

The tests pin them: `every_screens_keys_follow_the_three_rules` walks the whole table
(`app/tests/key_presses.rs`), `uppercase_letters_toggle_tui_modes_and_lowercase_never_do` and
`m_mutes_on_every_screen_and_nothing_lowercase_toggles_a_mode` pin the first two, and
`every_hint_on_a_screen_does_something_when_pressed` (`keys/tests.rs`) fails for a listed key that
does nothing.

`Esc` goes up one level: it closes whatever box is open, or ends a curve drag; on EQ, Telemetry or
Devices it returns to Controls; on Controls it opens the menu.

### Adding a key

Add a `KeyHint` to `HINTS` with its screens and group, handle the code in the screen's key function
(or in `global_key` if every screen shares it), and run the tests above: a lowercase key that claims
to toggle, an uppercase key that does not, or a key the table lists that does nothing, each fails
one. The documentation's key list and the help overlay follow the table without further edits. A
plain `[tui]` setting follows a different path, written at the head of
`crates/svolder-ipc/src/config/tui_settings.rs`: its field, its parse line, its `TuiSetting` entry
and its row in `CATEGORIES`; the table tests there fail for a key that skips a step.

## The look

### Roles, not colours

A screen never names a colour, only a `Role` (`theme.rs`: `Text`, `Dim`, `Faint`, `Good`, `Warn`,
`Alert`, `Rule`, `Selected`, `Bar`, `MeterOk`, `Fault`, `CurveSum`, and the rest).
`Theme::from_config` resolves roles to styles once per run from the `[tui]` table, `--no-color` /
`NO_COLOR` and the terminal's colour depth. Restyling is therefore a change to a preset or to the
role table, never a hunt through screens.

A `Preset` is a table: an accent per device family (`dac`, `amp`, `preamp`, so a DAC and an amp
read as two colours), the three semantic colours, the text greys, the stops the glow travels
(`spectrum`, with `rest` the stop the accent sits on), the ANSI slots the accents take at 16
colours (`slots`), the three background greys (`greys`, which `Surface::{Ground, Pane, Raised}`
select), and optionally an `emphasis` colour and a tone-curve ramp (`curve`). Four ship, `amber`
(the default), `ice`, `red` and `neon`; `PRESETS` lists them and `[tui] palette` names one. With
an `emphasis` colour (`red`) bars, the selected row and pane titles take it and the accent is kept
for the rule, the key hints, the `▶` marker and the frame the keys are in, and every other frame
is drawn faint. `[tui] accent` overrides all three accents, `[tui.accents]` one family at a time,
`[tui.spectrum] stops` the glow's stops.

Depth is a fallback chain, not a second design: at 256 colours the same colours go through the
cube (`palette.rs`), at 16 each role takes the ANSI slot it means (a nearest-RGB reduction would
land a muted colour on a grey), and monochrome marks the selected row by reverse video alone.

### Layout and size tiers

A screen is not stretched with the terminal. `Tier::of` (`layout_tier.rs`) answers once per frame:
wide from `WIDE_FROM` (120) columns, tall from `TALL_FROM` (40) rows. Wide widens the panes, adds
air inside each border and caps the body at `BODY_MAX` (256) columns, centred; tall adds blank rows
and taller meters; below both, up to 119 by 39, a screen draws exactly as at 80 by 24. Every screen
asks for the tier of the area it draws, and the mouse hits, the curve's cells and the frames the
animation traces are read back from that same layout, so none can disagree with the screen.

## Decoration

Everything in this section is laid over screens that already say everything in plain text.

### The intro and the wordmark

The TUI claims the terminal before it opens anything, so the seconds a connect takes show the
wordmark rather than a blank screen (`decoration/intro.rs`). The mark is a text asset
(`bins/svolder/assets/wordmark/large.txt`, `small.txt`), one row per line, every glyph one cell,
drawn as a two-tone gradient of the family accent (`decoration/wordmark.rs`; `Size::Large` for the
intro, `Size::Small` for the header). At true colour it fades in and holds; below that, or with the
layer off, it is drawn once and held. `[tui] intro_effect` picks the build: `fade`, or the demo
effect `particles` (`decoration/intro/particles.rs`, sparks thrown in along a front and landing on
the letters), whose exit breaks the mark back into sparks that fly into the header's logo
(`decoration/intro/reform.rs`) while the panes arrive. `random` picks among the demo effects. Any
key ends the intro early; Ctrl-C and `q` give up on the device instead of getting past the mark. A
demo effect is a function of the time, the frame size and the seed alone
(`decoration/intro/rng.rs`), paced at sixty frames a second and thirty where the terminal cannot
keep up (`decoration/intro/pace.rs`). To add an effect: a variant of `Demo` and `Scene`
(`decoration/intro/demo.rs`), a module beside `particles`, and a word for `intro_effect`.
`decoration/intro_sound.rs` plays a short sound through PipeWire with it when `intro_sound` is on.

### The squirrel

Ratatoskr is half-block pixel art (`decoration/mascot/`): the sprite sheet is a text asset
(`bins/svolder/assets/mascot/sprites.txt`) parsed once into named layers (`sheet.rs`), the
costume a palette picks and the season the local date falls in dress it (`looks.rs`), its tricks
are poses over time (`tricks.rs`) and `draw.rs` paints it where its box fits. The brain
(`brain.rs`, `Mascot`) watches what the app shows through a `View` snapshot and says what the
squirrel is doing: it never draws and never reads a clock (every method takes the instant it is
told about). It stays out of sight until a typed code, read only in the help overlay and the menu
(`codes.rs`, kept out of the user guide on purpose), calls it; hidden, it still visits for a fault,
a lost or returned link, power and an input switch. It runs where the animation layer does and is
off at 16 colours and in monochrome. `[tui.effects] mascot` switches it off.

### The animation layer

`Effects` (`decoration/effects.rs`, split by concern into `effects/keyed.rs`, `shimmer.rs`,
`drift.rs`, `passage.rs`, `reform.rs`) is drawn over a frame after the widgets finish. It is built
on `tachyonfx`, constructed with `Effects::new(capable, config)` and re-tuned live by
`Effects::configure`. What it runs:

- the arrival of a pane (its frame traces itself round while the content sweeps in behind it) and,
  by `[tui] transition`, the screen-change passages `noise` and `melt` (`decoration/transition.rs`);
- a red flash then a slow pulse on the header while a fault stands, and a fade-in of readings
  after a reconnect;
- an accent glow on a row whose value just changed, and a grey trail on the row the cursor left;
- the shimmer, a bright band across the row the user just changed or moved to, repeated at the
  `REMINDERS` (2 s, 10 s, 30 s, 2 min) after the last key and then left alone;

Three more are drawn by the screens themselves but answer to the layer's switches, derived in one
place (`App::set_look`, from `Effects::capable`): the glide of an eased bar, the breathing of the
borders with the level, and the peak marker a meter holds. Each has its own switch in
`EffectsConfig` (`crates/svolder-ipc/src/config.rs`). The layer runs only at true colour with
colour on; `noise` and `melt` also draw at 16 colours and in monochrome.
The spinner (`widgets/spinner.rs`) is information, not an effect, and runs whatever the layer does.

### The TUI's glow

The decoration answers to two things and nothing else: where the user's attention is, and how loud
the music is. Attention is the instant of the last key (the shimmer's schedule) and a focus change;
level is `LevelSource` (`audio_level.rs`), one number from the device's own scaled meters
while they stream, else the capture of the host's sink, never a mix.

- **The runner** is a three-cell band of light that runs once round the frame of the pane that has
  the focus, on a focus change and on the frame the keys are in when a screen is arrived on
  (`Effects::on_lap`, `lap_ms`). Its brightness and pace follow the level: the spectrum's hot end
  and about 0.4 s a lap at the loudest, else a lift of the frame's own colour and about 1.2 s. It
  never runs on a modal or the wizard. Switch: `effects.frames` (key `F`).
- **Hue drift** (`effects.hue_drift`, key `H`, off by default) moves the decoration (borders,
  titles, the rule, the key hints) along the preset's spectrum: at rest the accent, with no level a
  slow wander in the cool half, with a level toward the hot end as it rises
  (`Effects::glow_position`, `level_position`, `Theme::set_glow`).
- **Rainbow scroll** (`effects.spectrum_drift`, key `R`) carries the tone curve's own colours round
  the hue circle, or along the preset's ramp on `red` and `neon`.

What carries a value never takes the glow: a bar, the cursor, the selected row and a curve's points
keep the family's resting accent (`Theme::accent`), meters never glide, and the semantic colours do
not drift. The curve's colouring by frequency is the curve's own (`effects::spectrum`) and applies
with the layer off; the layer only carries it round.

### The loop's pace

`tick_period` (`run/look.rs`) answers how long the loop waits: `FAST_TICK` (33 ms) only while
something moves (an effect, a passage, a gliding value, a volume climb, a falling peak marker, a
drifting curve, a called squirrel, a beat's pulse), a hundred milliseconds while a spinner turns,
the meters' own pace while the host capture drives the decoration, and the app's own 250 ms
otherwise, which is what an idle run with effects off keeps. A tooltip waiting to show shortens
whichever period applies to `tooltip::PENDING_TICK` (100 ms), so the pointer's rest is noticed in
time.

## Principles

**The `App` is plain state.** It takes updates and keys and returns intents; the loop owns every
terminal, radio and clock. Put a terminal call or a clock read in the `App` and its tests need a
terminal and cannot say what a key does.

**What is drawn is what the device said.** A press writes and waits for the echo; the screen shows
the old value until the device confirms. A glide, a glow or a fade decorates a change the device
has already confirmed, and a rejected write shows none.

**A screen names a role, never a colour.** The palette, the depth fallbacks, monochrome and `red`'s
emphasis all live in the theme. A hard-coded colour in a screen is wrong at 16 colours, under
`NO_COLOR` and in every preset but one.

**One table names the keys.** The help overlay, the wizard's key row, the idle advice and the user
guide read `HINTS`. A key handled but not listed is invisible; a key listed but not handled
lies, and a test fails for it.

**The three key rules hold for every key.** Someone who knows them can guess a key. A lowercase
key that toggles a mode, or an uppercase one that acts on the device, breaks that and the tests
say so.

**A box takes every key until it closes.** Otherwise a letter typed into a preset name would also
fire the key it is elsewhere.

**Decoration is never the only carrier of a fact.** A frame's text is identical with effects on or
off (`a_frame_says_the_same_with_effects_on_as_with_them_off`, `decoration/effects/tests.rs`), and
the layer leaves the backgrounds the theme painted
(`the_effects_leave_the_backgrounds_as_the_theme_painted_them`). Make a fact appear only in an
animation and a 16-colour terminal never sees it.

**Input first, and rest when nothing moves.** Effects are applied to a frame after input has been
handled, a key redraws at once whatever is mid-animation, and no effect queues input. The fast
period ends with the last effect
(`the_tick_period_is_a_quarter_second_and_falls_to_a_frame_only_for_an_effect`,
`run/look/tests/ticks.rs`), so an idle TUI costs a tick a quarter second.

**Meters never glide, and a value never follows the music.** A meter is a fact reported moment to
moment; only decoration follows the level.

**The screen and the file never part company.** A Settings row is a key of `[tui]`, applied to the
running theme and written at once; the box's bottom border names the file. A failed write is a
toast and the in-memory setting stays for the session.

**Secrets stay out of the TUI's state.** `Connections` (`boxes/connections.rs`) holds the token's
masked tail and the MQTT password only as set or unset; the token's text is held for ten seconds
while revealed, and prints as `***` in a `Debug` dump.

**The tier is computed once and read back.** Layout, hit rectangles, the curve's cells and the
traced frames all come from `Tier::of` on the same area. A second computation somewhere is a click
that lands on the wrong row at one size.

**Pure brains, no clocks.** The squirrel's `Mascot`, the intro's scenes and the transitions take the
time as an argument, and their chance is a seed (`decoration/intro/rng.rs`) or a hash of a cell's
place (`decoration/transition.rs`), so a test can say what a frame draws.

## Where the code and tests live

- State and keys: `bins/svolder/src/tui/app.rs` and `app/` (`dispatch.rs`, `controls.rs`, `tone.rs`,
  `overlays.rs`, `intent.rs`); the key table `keys.rs`; tests in `app/tests/`, `keys/tests.rs` and
  `screens/help/tests.rs`.
- Drawing: `render.rs`, `screens/`, `widgets.rs` and `widgets/`, `layout_tier.rs`, `theme.rs`,
  `palette.rs`; tests beside each (`screens/*/tests.rs`, `theme/tests.rs`). The Settings box's rows
  are pinned by `screens/settings/rows.golden`; the user guide's mockups are read back by tests
  (`test_support.rs`, `screens/menu/tests.rs`, `screens/settings/tests.rs`), so a screen that
  changes shape fails until the guide follows. The configuration template is read back the same way:
  `crates/svolder-ipc/src/config/tests/tui.rs` compares it with `docs/usage/config.md`.
- The loop: `run.rs`, `run/events.rs`, `run/input.rs`, `run/look.rs`; tests in `run/tests/` and
  `run/look/tests/`.
- The start and the terminal: `start.rs` (claim the terminal, play the intro beside the open),
  `waiting.rs` (the keys that give up, and the spinner's clock), `terminal.rs` (raw mode, undone on
  every exit); tests in `start/tests.rs`, `waiting/tests.rs` and `terminal/tests.rs`.
- The setup wizard on the terminal: `wizard.rs` (the keys and the walk) and `screens/wizard.rs`
  (one step drawn); tests in `wizard/tests.rs` and `screens/wizard/tests.rs`.
- Clicks and hovers: `mouse.rs`, the hit map each screen fills as it draws; tests in
  `mouse/tests.rs`.
- Boxes: `boxes/` (`editor.rs`, `edit_line.rs`, `presets.rs` and `presets/`, `connections.rs`,
  `web_box.rs`, `toast.rs`, `tooltip.rs`), each with its `tests.rs`.
- Decoration: `decoration/` (`effects.rs` and `effects/`, `transition.rs`, `intro.rs` and `intro/`,
  `mascot.rs` and `mascot/`, `title.rs`); tests in `decoration/effects/tests.rs`,
  `decoration/transition/tests.rs`, `decoration/mascot/tests.rs`.
- The `[tui]` table: `crates/svolder-ipc/src/config.rs` (`TuiConfig`, `EffectsConfig`),
  `config/tui_settings.rs`; tests in `config/tui_settings/tests.rs`.
- Screenshots for the README are drawn from the same renderer by `tui/screenshots/` (`make
  screenshots`).

[Back to the map](../architecture.md).
