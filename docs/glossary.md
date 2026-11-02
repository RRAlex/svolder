<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Glossary

The words svolder uses in its own sense, as the code uses them. [The map](architecture.md) shows
where each lives. The bytes behind the protocol terms are in [protocol.md](protocol.md).

## The device

- **Forkbeard**: Schiit's Bluetooth LE control module; a device with one is what svolder talks to.
- **Unit**: one physical device, named by the 8-byte unit id its advertisement carries (`UnitId`).
  The configuration finds a device again by it, so a rename or a moved radio address changes
  nothing.
- **Passphrase** (also **code**): the 4-digit code a device asks for; the frames carry a
  transform of it, which svolder can undo to read the code from an advertisement. A device on
  **auto** that refuses its code has the code read afresh once.
- **Function**: one addressable value on a device, with an id of one to three bytes (`7.1`,
  `210.2.1`). Volume, mute, input and each EQ band are functions. The firmware-update family
  (but the `210.2.x` version query) is **restricted**: it cannot be encoded. The passphrase
  function is **sensitive**: it can only be set by its own verb and no remote view lists it. A
  profile control can name neither.
- **Frame**, **message**, **notification**: a frame is the bytes on the air; a message is one
  function's id and value; a notification is a frame the device sends, carrying messages.
- **Raw** and **display** value: raw is what the wire carries (a step number); display is what a
  person reads (dB, a name), converted through the profile.
- **Capture (file)**: the text file `svolder capture` writes: the host checks, the adverts heard,
  the readings and every frame of a session, scrubbed of unit ids and passphrases, for an owner to
  send. Not the **host capture**, which records the computer's sink monitor for the spectrum.

## Profiles

- **Profile**: a TOML file describing one device model, as data: its controls, their kinds,
  ranges and conversions, and the tone control. Built-in profiles are embedded from `profiles/`;
  files in `~/.config/svolder/profiles/` add or override.
- **Extends**: a profile's inheritance from a shared one (`dac-common`, `mesh-eq-5`); parents
  are loaded first.
- **Family**: the kind of device (dac, amp, preamp) a profile belongs to.
- **Control**: one function as a user sees it, with a **key** (its stable name in a profile), a
  label and a kind: **select**, **toggle**, **level**, **readout**, **meter** or **fault**.
- **Action**: a profile entry that is a command, not a control: a Command with empty data on its id
  plus a slot byte (a Loki Max's recall `7.2.<slot>` and save `7.1.<slot>`). It is kept apart from
  the controls (`Profile::actions`), so it has no reading, is never set to a value and no screen
  lists it.
- **Alias read**: reading a status as another control's: a control's `reads` names further ids
  (`6.1`), and `Profile::read_as` says which control a status on any id belongs to. The session
  applies it once, so a Loki Max band reported on `6.1` or on `6.1.3` is the one band.
- **Readout**: a control the device reports and nobody sets; it appears in the TUI, the web page
  and Home Assistant with no Rust written for it.
- **Key**: a profile control's name (`volume`). Not a keyboard key, and not a TOML key.
- **Reserved key**: one of the eight keys the MQTT bridge's own entities take (`link`, `signal`,
  `firmware`, `dsp_firmware`, `identify`, `eq_mode`, `eq_preset`, `eq_loudness`); a profile
  control keyed like one fails to load.
- **Firmware subsystem**: a part of a device that has a firmware build of its own (1 the main
  controller, 2 the DSP). A profile lists its `firmware_subsystems`, and the session asks them
  one at a time after a connect (`210.2.<n>`, each 300 ms after the one before's answer, as the
  app does); the answers are readings like any other.

## Connecting

- **Transport**: how svolder reaches a radio: `ble` (BlueZ), `fake` (in memory) or `auto` (try
  the daemon, then a session of its own).
- **Fake device**: a Forkbeard answered in memory, the crate `svolder-fake`. `svolder-sim` is the
  program that serves it over Bluetooth as a peripheral.
- **Link**: one open connection to one device, for writing frames and reading notifications.
- **Signal**: how strong a device's radio signal is, as a `Signal`: dBm, a **source** and when it
  was heard. The source is **advert** (the advertisement a listen heard before the connect) or
  **live** (the connected link's own, which an unprivileged service cannot read, so the daemon
  never produces it). It is always worded with its source and age, however old.
- **Identify**: asking a unit to show itself (function 200), so one of several can be told from
  the rest. It goes over the link the daemon holds, and the session lets one request through
  every two seconds, the **gap**: a request inside it sends nothing and says "just sent". A
  profile can offer none (the Mesh Card).
- **Session**: the protocol's life on a link: connect, refresh every value, set and confirm,
  reconnect. It folds every status into a `DeviceState`.
- **Episode**: one refusal of the code and the one retry it earns, so a wrong code never loops.
- **Direct** and **daemon**: the two **backends** a front end can use; the first opens its own
  session, the second asks `svolderd` over its socket.
- **Front end**: anything that asks the daemon for a device: the CLI, the TUI, the web page, an
  MQTT client, an MCP client. The writer calls each one an **origin**.
- **Recovery**: the table that pairs every failure's "what happened" with the one step to take
  (`recovery.rs`), shown as the advice line and in error text.
- **Launcher chain** and **autospawn**: the steps `auto` takes to find a daemon, ending with
  starting one itself.

## The daemon

- **Actor**: the task that owns one device's session, keeps its link up and runs its commands.
- **Writer**: the per-device queue every write passes through; it orders writes, replaces a slider's
  superseded value and applies the volume rule.
- **Fleet**: the trait the daemon's HTTP servers reach its devices through.
- **Slot**: the holder of one HTTP server (web or MCP) that brings it in line with its config.
- **Handler**: the code that answers a socket request.
- **Bridge**: the daemon's MQTT task, which publishes devices for Home Assistant.
- **Recall**: applying a named tone curve to a device as one batch of writes.
- **Ring**: the daemon's recent history of readings, which the web strip and `history` draw.
- **Scanner**: the one shared listening window for devices that are looked for.
- **Reload**: reading the configuration again without restarting (`SIGHUP`, or after an edit).
- **Adoption**: taking over a listening socket a systemd socket unit already holds.

## Volume rules

- **Cap**: a front end's limit on the volume, `max_volume_db`; MQTT and the web page clamp to it,
  MCP refuses.
- **Glide** and **climb**: a raise above three quarters of the range is written to three quarters
  at once, then climbs a tenth of the range a second. A glide is the request; the climb is its
  stepping.
- **Echo**: the device's own report of a value after a write; the next step starts from it, never
  from what was asked.
- **Superseded**: a write that a newer one took the place of. `Mode::Latest` writes (sliders)
  may be; `Mode::Ordered` writes (typed commands, relative steps) never are.
- **Red zone**: the part of the volume where the tone curve can clip.

## Tone control

- **Band**: one peaking filter of the device's EQ, with a frequency, a gain and a **Q**.
- **Mode**: Flat (bypassed), EQ (the bands) or Loudness (the loudness curve, whose strength a
  level sets).
- **Flatten**: set every band's gain to zero.
- **Point**: a band's marker on the Response graph, at the band's own frequency and gain (at 0 dB
  in Loudness); the web page lets it be dragged.
- **Preset**: a named tone curve, built in or saved in the configuration.
- **Device slot**: one of the places a Loki Max itself keeps a set of bands (three, "Preset 1" to
  "Preset 3"). A recall sets the bands from it and a save writes the current bands over it; the
  device never says which slot is current. Not the daemon's **Slot**.
- **Built-in setting**: one of the named band settings a device's profile lists (a Loki Max's
  Flat, Bass Boost, Treble Boost, Smiley and Loudness), written as raw steps, with no conversion.
  Unlike a **Preset** it is data of one device.
- **Fit**: choosing bands so that they match a headphone correction as closely as the device's
  bands allow.
- **APO**: Equalizer APO's parametric text, the format AutoEq writes.

## Host audio

- **MPRIS**: the session-bus interface the host's media players speak; the TUI follows one and
  shows what it plays.
- **Host EQ**: a tone curve applied on the computer, as a PipeWire filter chain before the sink.
- **Sink**: the audio output the computer plays to; its **monitor** is what it plays, captured
  for the spectrum.
- **Host view**: what the daemon says of the computer: whether it watches the host at all, the
  followed player's track (title, artist, album, status, length, progress, cover) and the route.
  Front ends read it with `Host` and hear changes on a `Subscribe` that asked for the host.
- **Route**: where the followed player's sound goes, read from `pw-dump`: the default sink, the
  player's stream, what else mixes into the sink and whether the host EQ is in front of it.
- **Path line**: the route and the DAC's own readings turned into one line from the player to the
  DAC: its hops and a verdict, `untouched` or the hop that changes the signal, or no verdict when
  the computer plays elsewhere. Never worded "bit-perfect".
- **Watcher**: a held claim on the route: while one exists the daemon reads `pw-dump` (at most every
  2 s), and with none it reads nothing. A web page of a DAC and a TUI that asked for the host hold
  one for as long as they are connected.
- **Frame** (an analysis frame): what the **analyser** makes of the sound about 30 times a second:
  per channel 240 one-byte bands at 1/24 octave from 20 Hz, the RMS and peak of each channel, a
  beat flag, a tempo and a beat phase, and the capture's time on the daemon's clock. The byte
  scale is a tuning, not a promise to a front end.
- **Analyser**: the part of `svolder-host` that turns captured samples into frames, however the
  samples are cut up. The **beat** is read from the bass (20-150 Hz) alone, so a passage with no
  bass has no beat and no tempo.
- **Frame capture**: a recorder of the default sink's monitor that hands on frames
  (`FrameCapture`, `PwFrames`), as `Capture` hands on the TUI's blocks. It is passive, so it never
  keeps the sink awake.
- **Frames hold**: a held claim on the daemon's capture (`FramesHold`): the first one starts it,
  and it stops 2 s after the last is let go. A web page's frames stream and a joined TUI's frames
  connection each hold one for as long as they are open.
- **Capture state**: whether the daemon's capture is `Starting`, `Running` (the sink's name and the
  node it records) or `Unavailable` (a plain reason); sent to every holder on change.
- **Frames stream**: the frames of the daemon's capture sent to one front end: `api/v1/host/frames`
  for a web page, `Request::HostFrames` for a TUI.
- **Beat pulse**: the TUI's 200 ms response to a frame's beat flag: the Spectrum row bold and the
  breathing borders lifted.

## The terminal UI

- **Screen**: one of the TUI's pages (Device, Tone, Telemetry, Devices, Settings, Setup). A
  **pane** is a part of one with its own focus.
- **Box**: a window over the screen that takes every key while open (the editor, the preset
  list, the Settings box, an edit line). The toast and the tooltip live beside the boxes and take
  no key.
- **Intent**: what a key press or a click asks for (`Intent::Set`, `Intent::Quit`), carried out
  by the run loop. The `App` holds state and returns intents; it touches no terminal.
- **Hit map**: where the last frame drew each clickable thing, so a click becomes the intent a key
  would.
- **Tier**: one of the two size classes a screen is laid out in above the smallest terminal.
- **Turn**: one pass of the run loop: wait for a key, a click, a reading or a tick, then redraw.
- **Look**: the loop's render-time state: the `[tui]` settings, the theme and the animation layer
  built from them, kept so the Settings box can change them while it runs.
- **Tooltip**: the note a resting pointer brings up for a clickable thing.
- **Accent**: the theme's colour per device family (a DAC and an amp read as two colours);
  `[tui] accent` overrides them all.
- **Role**: what a thing on screen means (accent, fault, dim); a screen never names a colour, a
  **theme** gives each role its style.
- **Toast**: the small note over the bottom of the screen saying what an action came to.
  **Advice** is the one-line hint for a fault or an idle moment.
- **Effects layer**: animation drawn over the finished screen (eased bars, glow, shimmer, the
  spectrum's drift); a frame's text is the same with it on or off. The **intro**, the **mascot**
  (the squirrel) and the **transition** between screens are decoration beside it.
- **Front door**: what bare `svolder` does on a terminal: the setup wizard while no device is
  configured, the TUI after. The setup wizard's state machine is the `wizard` module; **doctor**
  checks the host; **demo** runs the TUI on a fake device and a daemon of its own.

## The web page

- **Theme**: a whole way to draw and use the page, not a colour scheme: a folder under
  `web/themes/` and one entry in the daemon's list. **Ember** is the one the page ships, the test
  theme, with the effects and Arrange mode; **Paper** is the second, a sheet of squared paper in
  light and dark. [Web themes](design/web-themes.md) says what a theme is.
- **Frame** (the page's): what the page draws over every theme and never hands to one: the menu
  button and its menu, the **picker**, the popups, the **reserved key**, the empty root a theme
  draws into. Not the analysis frame above.
- **Core**: the part of the page every theme shares: the stream, the model, the actions, the
  preferences, light and dark, the canvas lifecycle, the tone curve's maths and gestures, the
  registry. It lives in `web/core/`.
- **Model**: the page's JSON picture of the fleet and the shown device: values, words, ranges and
  states by kind of control, no geometry. The first snapshot rides in the page; the stream sends
  the changes.
- **Part**: one piece of a device's model (`link`, `volume`, `controls`, `tone`, …) as JSON, which
  a theme draws and the events stream sends again when it changes.
- **Needs**: the live feeds a theme asks for beyond the model: the meters, the computer's player,
  the music's analysis.
- **Theme menu**: the frame's menu behind the menu button: a Theme row, the theme's own rows, then
  Device and Sign out. The **picker** is the sheet of one card per theme that the Theme row and
  the reserved key open.
- **Reserved key**: `T`, which opens the picker from any theme; no theme may bind it.
- **Paper**: the page's second theme: squared paper, a printed serif, a pencil and blue ink.
  [Paper](design/web-paper.md) describes it. **Dark paper** is its dark scheme, charcoal with chalk
  print, chosen from its menu or with `?theme=paper.dark`.
- **Written control**: Paper's way to draw a control: as words, where an underlined word acts and
  a sliding level is a printed scale. The **needle** is the pencil mark on the volume's ruler (and
  on a meter's scale): a slider you drag or move with the arrows.
- **Note** (Paper's): a short line of blue handwriting about a section of the sheet. The **red
  note** is the one note, at most, that is in pencil red, the most urgent. The **last change** is
  the note that says what changed on the device last, and `from this phone` or `from this browser`
  when this page asked for it.
- **Arrange**: Ember's mode for moving its sections between the band and the two columns, with a
  mouse, a finger or the arrow keys; the order is kept per device kind and per browser.
- **Test driver**: a theme's `driver`, which the contract checks use to reach, operate and read
  every control and fact the way a person would, through the theme's own handlers.
- **Declared hop**: a hop of the signal path after the DAC that the owner names in the
  configuration, with a kind (amp, headphones, speakers or other), because no reading finds it. A
  **linked hop** is one tied to a configured device, such as an amp. `svolder hops` writes both.
- **Facts part**: the part named `facts`, the Device fold-out at the foot of a card: product, unit,
  firmware builds, profile, link, signal, and a table of every reading the daemon holds, named or
  not. Closed by default; its ages are written by the page from stamps, so it is sent only when a
  fact or a reading changes.
- **Strip**: the bar along the bottom, one bar a second of what the device reported.
- **Knob** and **dial**: the volume control, drawn by the theme from the model and moved by the script.
- **Pairing code**, **link code**: one-use codes that sign a browser in; a code to type, and a code
  carried in a link. A **guest** may read without signing in where the server lets it.
- **Smoke**: the page's live background, a fluid simulation. The **haze** is its steady, dim
  base; a **stir** is a disturbance (a pointer, a scroll, a meter) that raises its
  **agitation**; a **settle** is the agitation dying down, and may release a **note**, a music
  glyph.
- **Director**: the smoke's rules, with no GPU: events and time in, a plan of what to draw out.
  The **engine** runs the plan in WebGL and decides nothing.
- **Grab reach**: how far from a band's point a press still takes it, 12 CSS px for a mouse and 24
  for a finger; the nearer point wins, and a right press or a pinch with none in reach takes the
  band nearest in frequency. The wheel reaches 20 px, and a burst of wheel notches keeps its band
  while the pointer stays on the plot.
- **Axis hold**: in a left drag each axis (frequency across, gain up and down) keeps the press's
  value until the pointer has gone 8 px along it, so a sideways sweep never nudges the gain.
- **Swap hold**: the tone part's swap waits, in every browser, while a point is dragged and until
  its writes are answered, so the graph is not replaced under the pointer; the swap that waited
  lands, or is read again, when it goes.
- **Visualizer**: the page's card that draws what the computer plays from the daemon's frames. A tap
  on it, or `v`, cycles its three **looks**: **Spectrogram** (a waterfall, frequency up the card,
  colour from the two channels), **lumiBars** (240 bars, the left channel on a prism gradient and
  the right on a rainbow) and **lumiBars soft** (the same, faded out at the top and bottom; the
  default). The card is translucent. The **graph overlay**, on or off (off at first) over any look,
  draws each channel's curve and its **max hold**: the highest level each frequency reached, held for
  2 seconds, then falling. The effects menu's **Visualizer switch** takes the card out of the page
  altogether (no drawing, no frames asked for), leaving Now playing the whole band at a lower
  height.
- **Visualizer delay**: how far behind the sound the card draws, the median of the last 2 s of each
  frame's time against the page's clock; the Device fold-out shows it while the card runs.
- **Sink label**: the line `Playing to <sink>` on the card when the captured sink is not this
  page's DAC, so the picture is never taken for the DAC's own.
- **Resting page**: a page that has been hidden for 2 s (`GRACE_MS`, `visible.js`). It closes its
  streams so the daemon does no work for it, and opens them when it is shown; its screen waits,
  dimmed and inert, until the first reading of the new stream.
- **Glow**: the page effect that is only CSS, beside Smoke and Off in the effects registry.
- **TUNING**: the one frozen object holding every number of the smoke's look.
- **Gate**: a check set that must pass before a change merges; the **quick gate** is the part
  that leaves the smoke alone. A **mutant** is a deliberate break a check must catch.

## Home Assistant and MCP

- **Discovery**: the configs svolder publishes so Home Assistant makes an entity per control.
- **Base topic**: the prefix every MQTT topic starts with. **Retained** messages keep their last
  value on the broker; the **will** marks the bridge unavailable if it dies.
- **MCP tool**: one action the MCP server offers an agent. The server is **stateless**: each request
  stands alone, with a **bearer token** for sign-in.
