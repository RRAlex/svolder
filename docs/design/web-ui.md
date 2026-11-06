<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The web UI

`svolderd` serves a page for a browser on this computer or a phone on the network. This page is
how it is built: the request's way in, the sign-in and its tokens, the view models and the JSON
model, the live stream, the page's script, the writes, and the meters and the strip. It sits one
level below [the map](../architecture.md), whose table of layers says where each part lives; what
the page shows and how a user reaches it are in the [web UI guide](../usage/web-ui.md), and the
daemon it is a slot of is in [daemon.md](daemon.md). The page is a frame, a core and a theme:
[web-themes.md](web-themes.md) says what each owns and how a theme is added, [frame.md](frame.md)
the frame, Ember's effects and Arrange mode in [themes-and-effects.md](themes-and-effects.md)
and the theme Paper in [web-paper.md](web-paper.md).

## Where it runs

The web UI is a `[web]` section of the configuration (`WebConfig`, off by default) and a slot of
the daemon, brought in line by `Daemon::web_to` on start, reload and shutdown exactly as MCP's and
MQTT's are ([daemon.md](daemon.md)). It listens on a port of its own, served through the hardened
accept loop the MCP server shares (`bins/svolderd/src/http.rs`: a bound on connections, a timeout
on the request head, a cap on its size). A port that cannot be bound or a token that cannot be had
fails the web UI, never the daemon: `WebStatus` (`crates/svolder-ipc/src/socket/protocol.rs`) says
`Off`, `Listening` or `Failed`, `svolder daemon status` and `doctor` print it, and a reload tries
again. The daemon has no TLS; a reverse proxy terminates it, which is why every URL the page uses
is relative (a proxy can mount it under a path that ends in `/`).

## A request's way in

`router` (`src/web/router.rs`) puts one middleware, `guard`, in front of every route, so nothing
reaches a handler unchecked:

1. **`Host`** must be one the server answers to (`host_allowed`: the bound address, the loopback
   names on a loopback or wildcard bind, and `[web] hosts`); a wildcard bind with no `hosts` lets
   any through and the sign-in is the check. Otherwise 403 with no body.
2. **A POST** must carry a single `Origin` whose host and port equal the request's (or one
   `[web] origins` names) (`auth::origin_allowed`), `Content-Type: application/json` (415) and at
   most `MAX_BODY` (16 KiB, 413) that arrives in time. GET and POST are the only methods, and a
   GET never changes anything. There are no CORS headers.
3. **Every answer** gets `secure`'s headers (`nosniff`, no referrer, `DENY` framing,
   same-origin policies, `no-store` on pages and the API), and HTML gets the `CSP`:
   `default-src 'none'`, everything else `'self'` (images also `data:`), no framing.

The routes are the page (`/`, `?device=NAME`), the sign-in (`/login`, `/pair`, `/logout`,
`/api/v1/session`), the assets and the manifest, then under `/api/v1/devices/{device}/`: `screen`,
`parts/{part}`, `model`, `strip`, `events` (the stream) and the writes, `/api/v1/fleet` and
`/api/v1/host` (JSON models, the first in the page), and under `/api/v1/host/` the computer's side:
`cover`, `press` and `frames` ([Now playing](#now-playing)). `docs/usage` tells scripts to use the
CLI, the socket, MQTT or MCP: this API is shaped for the page and promised to nobody.

## Sign-in and tokens

- **The token.** `Store::WEB` (`crates/svolder-ipc/src/token.rs`) resolves the web token from
  `SVOLDER_WEB_TOKEN`, `[web] token_file` or a generated file (32 random bytes, mode 0600), the
  way `Store::MCP` does for MCP ([ipc-and-configuration.md](ipc-and-configuration.md)). It is
  never logged, printed or put in a status; `token_matches` compares in constant time.
- **The cookie** (`auth.rs`). `svolder_web_<port>=v1.<login seconds>.<mac>`, the MAC an
  HMAC-SHA256 keyed by the token over the login time, `HttpOnly; SameSite=Strict`, no `Path`, no
  `Domain`, `Secure` when `[web] url` is `https` or a proxy says so. Nothing is stored: a rotated
  token changes the key and signs every browser out, a daemon restart signs nobody out. A signed-in
  page load renews the cookie with the same login time and a fresh `COOKIE_MAX_AGE`; a login older
  than `LOGIN_LIFE` (three years) has ended and the sign-in page says so. The port is in the name
  because cookies ignore ports, so two daemons on one host do not overwrite each other.
- **One-use codes** (`codes.rs`, `Codes`). A pairing code (six digits drawn without bias, one open
  at a time, `PAIR_TRIES` wrong tries end it) is typed on the sign-in page; link codes (16 random
  bytes, at most `MAX_LINKS` open) travel in a link's fragment, which never reaches a server or its
  log. Each lives `CODE_LIFE` (five minutes) and signs one browser in. They live in the daemon,
  not the server, so a restart of the server leaves them open, and a changed token ends them
  (`Codes::bind`). A socket client may ask for either (`Request::WebPair`, `WebLink`): the socket
  gives full control already, so a code grants nothing it does not. `svolder web` and the TUI's Web
  box ask. `Request::WebLinkOpen` asks whether a link code is still open (`Codes::link_open`)
  without spending it: the box asks once a second while its link shows, and replaces a link a
  browser used (a phone camera's QR preview can) with a fresh one.
- **Arrival from another site.** A Strict cookie is not sent on a top-level navigation from another
  site, so the sign-in page's script (`login.js`) asks `api/v1/session` first and goes straight to
  the page when it is signed in; it also takes a link's code out of the address bar before
  anything else.
- **Guest and read-only.** `[web] guest = "read"` lets a visitor without a cookie see the screens
  (no unit id, passphrase, config path or token) with every control drawn inert; every write
  answers 401 to a guest and 403 when `[web] writes` is off. `auth = "none"` is allowed only on a
  loopback address.

## The view models

`view::screen` (`src/web/view.rs`) turns a `Snapshot` (the device's `DeviceInfo` row, its profile,
the confirmed readings, the writer's climb, the cap, the viewer's `Access`, the presets that fit,
whether the configuration is writable) into a `Screen`: a device's whole page as plain data. The
sub-models are `view/controls.rs` (a `ControlView` per control, by its kind), `view/dial.rs` (the
knob's range, words and marks), `view/facts.rs` (the Device fold-out), `view/host.rs` (the Now playing card),
`view/meters.rs` (an amp's meters and its power, with no faults), `view/strip.rs` and
`view/tone.rs`. They are pure: no clock, no network, no daemon. `model.rs` turns a `Screen` into the
page's JSON model (`DeviceModel`: values, words, ranges and states, no geometry), which the page
carries, `api/v1/devices/{device}/model` answers and the events stream sends the changes of; the
templates (`bins/svolderd/templates/`: `page.html`, `signin.html`) are the frame and the sign-in
page only. The tests read the view models and the model directly. Words come from the core's text
(`svolder_profiles::units`), so a value reads the same here, in the TUI and in Home Assistant.

A control appears by its kind, from the profile and no further code: the volume (`roles::volume`)
is the knob, a select with up to five values a segmented track and with more a list, a toggle a
switch, another level a slider, a readout a label, a fault a line in the card above the screen while
it is active (every active fault, one line each, for every family, and in no other place on the
page). A part (`Part` in `model.rs`: `link`, `card`, `volume`, `controls`, `meters`, `power`, `tone`,
`presets`, `device_presets`, `facts`, `playing`) is one view model and one key of the model.
`shape` is a hash of what makes a device a different device (the card, the profile, the layout,
which parts exist): when it changes the stream sends the whole model rather than parts that no
longer fit.

The model gives the page what a drag on the graph needs, so the page never converts a raw step or
formats a number itself. `tone_steps` (`view::tone::steps_json`) holds every step of every band: per
band `{"f": [[hz, "words"], …]}` and, for all bands, `"gain"` and `"q"` as `[[db, "words"], …]` and
`[[q, "words"], …]`, 128 entries each, the index being the raw step and the words exactly
`units::band_text` through `minus`. `tone.readings` is `{"mode": "eq"|"flat"|"loudness"|null,
"bands": [[f, g, q], …]}` with each band's raw frequency, gain and Q, and `null` for a value not
read. `inert` is set for a guest or with writes off, and the page then lets no point be grabbed. A
band's marker (`tone.markers`) sits at its own frequency and gain, as in the TUI, and not on the
summed curve: in Loudness and with no mode read at 0 dB; a band not read in full has none.

## Live updates

`GET api/v1/events?device=<url>&needs=<meters,host>&follow=<url,url>` (`src/web/events.rs`) is one
server-sent stream of JSON events per page: `device` is the device shown (`model.device.url`),
`follow` the others it keeps live, `needs` what else it draws from. It opens with `retry: 2000` and
the page's `model`, then the shown device's `volume`, each slider's `level`, its `meters` (with
`needs=meters`) and its `strip` of the last five minutes, and the same components of each followed
device, so a page that reconnects catches up. Then it sends only what changed, comparing each
part's JSON with what it last sent (`Sent`, in `events/device.rs`; one task per device, one for the
fleet and the host in `events/page.rs`). Every event but `model`, `fleet` and `host` names its
`device`:

- `model`: the shown device's shape changed, so the whole model again; `device` the same for a
  followed device (`{device, data}`);
- `part`: a part's JSON changed (`{device, name, data}`), coalesced over `PARTS_AFTER` (100 ms),
  the link and the card also on the second's turn, since a row changes without a feed;
- `volume`, `level`, `meters`: what each component reads, at most twenty a second
  (`COMPONENTS_AFTER`), the `meters` only to a page that needs them;
- the `playing` part only when what it says changes, never for the clock it carries ([Now
  playing](#now-playing)), and the `facts` part only when `Facts::key` changes (see the Device part
  below), at most once a second (`FACTS_AFTER`);
- `strip`: the shown device's bars of each second not sent yet; a keep-alive every 15 s;
- `fleet`: the devices' links or families, the list, or the hops changed; `host` (with
  `needs=host`): the computer's player changed, with or without a device.

A write answers with the parts it changed (`parts`) and the words of its toast (`toast`).

A page hidden for 2 s (`GRACE_MS`) closes the stream and opens it when it is shown, so a background
tab costs the daemon nothing and its next snapshot catches the page up. A rest is not a lost link,
but the woken screen is dimmed and inert until that reading (a volume step is absolute). A page
loaded hidden opens it when shown. While a DAC page's stream lives it holds the host's route reads
on (a `HostWatch`), so the daemon reads PipeWire for a page that is open and shows a path, and for
nobody else; an amp's or a preamp's stream holds nothing. A rotated token ends the stream. The page's first answer is a frame carrying the first snapshot of the model (the
strip's bars included) in a `<script type="application/json" id="svolder-page">` block, which the
CSP does not execute; the theme draws the page from it before the stream opens. A lost stream dims
the controls and the link's dot beside the device's name says so until it returns.

## Components and the page script

Three web components, plain ES modules with no build step, each own their state and write through
the core's actions, reading their first state from `data-` attributes and then from their stream
event: `sv-knob` (`knob.js`, the volume: the theme draws the dial, the script repaints the same
geometry as readings arrive), `sv-slider` (`slider.js`, every other level) and `sv-meters`
(`meters.js`). They are kept across swaps (`hx-preserve`); `keep.js` holds a swap back while a
slider is dragged or a stepper held, in a browser that cannot move a kept element in place, and
the part is drawn again from the model's newest copy. `api.js` holds the one-at-a-time sender and
a refusal's words, `toast.js` the popups, `strip.js` the strip along the bottom (drawn on a
canvas) and `tone-motion.js` the tone curve's easing and flashes. `tone-curve.js` is the tone
control's curve maths in plain numbers (the response, the peak that can clip and its words, the
graph's x and y for a frequency and a gain, and back): it touches no page and imports nothing, and a
fixture of the core's own numbers holds it equal to `tone/response.rs` (see [the tone
control](tone-control-and-presets.md#the-curve-in-the-page)). `tone-drag.js` is the curve's gestures
as a model with no page in it, and `tone-graph.js` the page around it for the mouse and the finger;
[Dragging the curve's points](web-curve-dragging.md) says what they do and why. `core/stream.js`
opens the event stream and the store (`core/store.js`) keeps what it says (the model of the page and
its readings; the stream tells the page with a `svolder-lost` event when it drops or returns),
`playing.js` the Now playing card's counting and its buttons, `visualizer.js` the visualizer card
and `core/visible.js` whether something is on show ([web-visualizer.md](web-visualizer.md)),
`tone-editor.js` the steppers that repeat while held and the band editor, and `keys.js` the desktop
keys (arrows for the volume, `m`, `0`, `f`, `p`, `v`, `?`; none while a field has focus or with a
modifier) and the key list. The page is a frame ([frame.md](frame.md); `templates/page.html`: the theme's root,
the page's model and the list of themes as data) that starts the core (`core/boot.js`) and mounts
the theme Ember (`themes/ember/ember.js`), which draws every part from the model (`draw/`) and
follows the store (`live.js`); htmx is its swap engine only (one htmx file is vendored). Every
write is a core action (`core/actions.js`), which toasts a refusal and reads the device again. The
modules register nothing when imported: each exports an `install…` call that `ember.js` makes,
its listeners ended by the mount's signal.

The Device part lists every reading the daemon holds, named or not, and an amp changes some ten
times a second, so it is built to stay quiet. Its ages are not made on the server (that would
change the part every second for every open page): the part carries each reading's stamp and the
daemon's clock, and the page words the ages. Only a change of a fact or a reading's words, or the
newest stamp reaching a new minute, changes what the stream sends (a meter's reading does not: the
meters show it already), so a row that is not the newest can show an age up to about two minutes
old until the next send. The table never holds a function of the `Sensitive` class, the
passphrase's, for the owner and for a guest alike: the encoded code is reversible, so no remote
view lists it. A device whose profile offers no identify (the Mesh Card) has no button.

`facts.js` owns the Device fold-out's page side. The part is a native
`<details class="device-facts">`, closed as drawn; the script opens it from `localStorage`
(`svolder.facts`, every access in try/catch, the page whole without it), keeps it open across a
swap of its part, and writes the ages in place once a second from the `data-heard-at` stamps
against the part's `data-now`, the daemon's clock when the part was made, so a clock skew between
browser and daemon never shows. Opening the fold-out reads the model afresh, so the table is
current when someone looks. The signal's words come from the server
(`Signal::describe`), and the script re-words their age with the same thresholds (`node/facts.mjs`
holds the two to the daemon's edges). The Identify button is the core's `identify`, which says the
daemon's gap (`data-gap`) in a toast.

The CSP shapes the code. There is no inline script and no `style=` attribute in any template
(positions are SVG attributes, components set custom properties through the CSSOM, which the CSP
allows), htmx is configured by a `<meta name="htmx-config">` with `allowEval` and `allowScriptTags`
off, and there is no `hx-on`. Assets are `include_bytes!`ed in `assets.rs`'s `FILES` and in each
theme's `files` (`themes.rs`; a name absent from both is never served); `SVOLDER_WEB_ASSETS=<dir>`
reads a directory on every request instead, so styles and scripts can be edited with no rebuild
(a template, or a file a list does not name, still needs one).

## Now playing

The card is the page's one view of the computer. `svolderd` follows the player and reads the route
once for every front end (`host.rs`, [host-audio.md](host-audio.md)); the page asks the host through
three methods of `Fleet` (`host_view`, `host_watch`, `host_cover`; a press goes as the socket's
`HostPress`), so the web code never holds the host service itself. `view::host` turns the host's
view and the device's own readings into a `Playing`: the track (title, artist, `album · player`,
status, the cover's hash, the length and the position with its rate and read time) and, for a DAC
with a route, the path (`svolder_ipc::host::path_line` against the device's name, its `input`
reading and its `sample_rate` reading in hertz). It exists only while the host is available, so a
daemon with `SVOLDER_HOST=off` draws nothing, and an amp or a preamp gets the card without a path.
`shape` holds whether it exists, so a host that comes or goes redraws the screen.

- **The stream sends it by what it says.** `Playing::key` is everything but the daemon's clock;
  the `playing` part carries that clock as `now_ms` and the position as `progress`, and the page
  counts from them. A host change wakes the stream like a feed's; a card that says the same is not
  sent again.
- **The page counts the time.** `playing.js` writes the elapsed time and the bar once a second
  (the facts' own timer; none while the tab is hidden): the position plus the time since the read
  times the rate while it plays, clamped to the length, against the daemon's clock as `data-now`
  gave it (`clockOffset`), so a browser clock that is wrong shows the right time. The arithmetic is
  pure and held by `node/playing.mjs`.
- **The cover.** `GET api/v1/host/cover?v=<hash>` (reader-gated) answers only the current cover's
  hash, with its type, `ETag: "<hash>"` (a 304 to a matching `If-None-Match`) and a cache the
  browser may keep, since the address changes with the picture; any other hash is a 404
  (`web/host.rs`). A cover that goes after the page asked for it hides its `<img>`.
- **The frames.** `GET api/v1/host/frames` (`web/host/frames.rs`) is a stream of its own, for the
  visualizer, reader-gated as the device stream is (a guest may watch; a rotated token ends it at
  once; with the host off it is a 404). It opens with `retry: 2000` and a `hello` event `{"now_ms",
  "capture": "starting|running|unavailable", "sink": name|null, "why": text|null}`, where `now_ms`
  is the daemon's clock (the page's offset to it comes from here, taken again on each stream; each
  frame's `t` against it is the delay), then a `capture` event of the same shape without the clock
  on each change, and while the capture runs a `frame` event about 30 times a second: `{"t": capture
  unix ms, "l": base64 of 240 bytes, "r": base64 of 240 bytes, "lv": [rmsL, rmsR, peakL, peakR],
  "beat": 0|1, "bpm": n|null, "phase": 0-255}`. A byte maps dB onto 0-255 over -72 to 0 dBFS, a
  tuning and not a promise; the page reads the base64 with `atob`. The encoder is a dozen lines in
  `frames.rs` (the base64 crate is only a transitive dependency). A keep-alive comment goes
  every 3 s. The stream holds the daemon's capture ([host-capture.md](host-capture.md)) for as long
  as it is open: the capture starts with the first stream and stops 2 s after the last closes, so a
  page that is hidden costs the daemon nothing once it lets its stream go. A page that reads slowly
  is sent the frames it has room for and loses the rest; the capture's changes are never dropped.
- **A press.** `POST api/v1/host/press` with `{"button": "previous|play|pause|next"}` goes through
  `writes::writer` like every write (guest 401, `[web] writes` off 403), then the daemon's
  `HostPress`; the card follows the player's answer through the stream. The buttons are not drawn
  for a viewer who may not write.
- **No reflow.** The cover's box is always there and empty without a cover (a cover that 404s is
  hidden, its box stays); the band's height is fixed (`--band-h`, 240 px: a cover, the bar and a
  path of three chip rows, which holds a long changed path at 820, 1100 and 1280 px) and the
  visualizer in the band's right half has the same box, so starting it, or having no capture, moves
  nothing. The band is Now playing and the visualizer in halves from 1100 px (the cover 84 px);
  below that Now playing takes three fifths, with a 64 px cover and a smaller title. The path line
  puts the verdict chip first and the hops after it, so a path too long for its box can only lose
  hop detail, never the answer; `playing.py` reads every chip inside the band at 820, 1100 and
  1280 px on the fake changed route (`SVOLDER_FAKE_ROUTE=changed`). With no player the card is
  the one line `No player` in the same box. On a phone the folded line is 72 px whatever it says
  ([what it holds](web-visualizer.md#the-phones-folded-line)). A tap on the line (or its chevron)
  opens the full card in place; `data-expanded` is kept in the module and put on each incoming card,
  so the stream's swaps do not fold it. A card with no player has nothing to open: an open card
  whose player quits folds to the quiet line and stays folded when a player returns.
- **Where it sits.** On a desktop it is a band under the header, across both columns, with the
  volume and the tone control in their two columns below it; the visualizer
  ([web-visualizer.md](web-visualizer.md)) joins it on its right. On a phone it is a line above the
  volume (a thumbnail, the title and artist, play and next, a thin bar) that a tap opens into
  the full card and a second tap folds; the visualizer is a 180 px card under the volume (under the
  meters on an amp's page), for which the band and the columns dissolve into the screen's own
  (`display: contents`) and the parts are ordered, which leaves the card before the volume in the
  tab order (a known issue). An amp's or a preamp's page, which has no tone control, takes the same
  band and line in its own layout, with the transport and no path, since the path ends at a DAC. The
  card is drawn only while the host is available, and its buttons only for a viewer who may write; a
  guest who was let in sees the card and presses nothing.

## Writes

Every write goes through the device's writer as `Origin::Web` (`src/web/writes.rs`), the way every
front end's does ([daemon.md](daemon.md)). A write answers once the writer has, so the part it
returns shows the device's echo; a write a newer one replaced answers 204 and the page draws nothing.

- **The volume** is judged first by `Rules::web` with the page's cap (`[web] max_volume_db`, which
  clamps and, below every step, still lets the page lower), then sent as a glide: a raise above
  three quarters of the range climbs a tenth of the range a second, and a second page, another
  front end or a touch of the mute steers or ends the climb. `volume/stop` ends one.
- **Other controls** are newest-wins: a select by name, a toggle as a bool, a level in its units.
- **The tone control** (`writes/tone.rs`): a band's frequency, gain or Q, the mode, the loudness
  strength, a preset's recall. A band write takes exactly one of `step` (one step either way),
  `value` (in the parameter's unit) and `raw` (the step itself, 0 to 127, as a drag writes it: it
  converts through the same value-to-step path as every write, and a step outside 0 to 127 is
  refused with 422). A band written while the mode reads Flat asks the writer to follow it with the
  mode EQ (`flat_to_eq`) and the answer says so for the page's toast.
- **Identify** (`POST .../identify`) goes through `Request::Identify` and the daemon's one send per
  gap (`IDENTIFY_GAP`, kept by the BLE session: the page adds no limit of its own); the page has
  no button for a profile whose `identify` is false (`Facts::identify_offered`). It answers
  `{"sent": bool}`, `false` inside the gap, and 409 with the daemon's words (and the wait to the
  next attempt) for a device that is down. It is a write like any other: 401 to a guest, 403 with
  `[web] writes` off.
- **Presets** (`writes/presets.rs`) save, rename and delete through the configuration writers
  under their lock, then ask the daemon to reload, as `svolder eq save` does, so MQTT's list
  follows. A built-in is never touched, a taken name never overwritten. While the file is not
  writable the controls are hidden with a note, and recall still works.

## Meters and the strip

An amp's meters are an arch of `SEGMENTS` (32) per meter on its own scale. The scale is the
profile's (`svolder-profiles`, `meter.rs`: a logarithmic scale with a zero that may follow a floor
reading, a full-scale reading and a class-A block, or a straight line over a range), and every
front end reads a meter through the `Placed` that settles it, so the web, the TUI and Home
Assistant agree. The server draws the arches; `meters.js` lights segments up to each reading, keeps
a peak segment for a moment and writes the percentage and the state line (it says why there are no
readings: no link, standby, warming up, power meters off). A fraction from `RED_FROM` (0.93) is red.

The strip along the bottom is one bar a second from the daemon's ring of the last half hour
(`view/strip.rs`, `device/ring.rs`), with marks for what happened (link changes, recalls). A DAC's
bar is its volume's place in its range, tinted by the sample rate, red where that second's curve
could clip and dim while muted; an amp's is its louder meter on its scale; a second with no link
has no bar. It is JSON drawn on a canvas, so it needs no component.

## Restyling

Colours, sizes and type are tokens in `:root` at the top of `bins/svolderd/web/themes/ember/style.css`; a part's
markup is its file in `themes/ember/draw/`. A colour also appears in the icon (`src/web/icons.rs`,
`icon.svg`, `make web-icons`) and in the scripts that read tokens (the strip, the dial). The page's
small motions ignore `prefers-reduced-motion`: only the first visit's default effects level reads
it (Glow, not Smoke), and the effects menu's Off stills the page (`reduced_motion.py`).

## Principles

**Every check is in front of every route.** `guard` runs the Host, Origin, type and size checks
before a handler is chosen. A route added later is covered without its author remembering; a check
placed inside handlers would be missed by the one that forgets.

**The page holds no state the device did not give.** Components show the device's reading, show a
step asked for only until the device echoes it (and a climb the writer is performing, which the
device's readings confirm step by step), and go back to the last reading when a write is
refused. A page that shows its own guess shows a value the device may have refused.

**Writes are the writer's.** The volume's cap and glide, a climb's cancellation and the flat-to-EQ
rule are the same as for every front end because the page submits as an origin and adds nothing.

**View models are pure, and the model is data.** A part's content is a function of its snapshot, so
the stream compares each part's JSON and sends a part only when it changed, and the tests read the
models without a browser.

**Strict CSP, no inline anything.** It keeps a stored name or a device string from becoming script.
Add an inline handler and the page stops working under its own policy; the test
`no_page_carries_an_inline_script_or_style` fails first.

**The server says; the theme draws.** The model holds values, words and states and no geometry; the
theme's one drawing code makes the first paint and every live update, so they draw the same picture.

**The card's box does not depend on what a player publishes.** A cover that comes or goes, a path
that grows to three rows, a player that quits and a visualizer that is added all change what is
drawn inside the box and never the box; a volume dial that moved under a pointer because a track
changed would be a bug.

**The page asks the daemon for a cover and nobody else.** It has no address of a player's to
follow, the policy lets it load images from itself only, and the daemon reads the picture locally.

**A sign-in stores nothing.** The cookie is derived from the token, so there is no session file
to leak, rotation signs everyone out at once, and a restart signs no one out.

**Secrets travel only as far as they must.** A link code rides the fragment, a pairing code is
short-lived and attempt-limited, and neither is in a log.

**The stream catches up.** Each connection starts with a snapshot, so a phone that slept, a proxy
that cut the stream or a daemon that restarted needs no state of its own to recover.

**Relative URLs everywhere.** A page behind a reverse proxy at a sub-path must keep working, and
the cookie, set only by root-level URLs with no `Path`, is scoped to that mount.

## Where the code and tests live

- The server: `bins/svolderd/src/web.rs`, `src/web/router.rs`, `auth.rs`, `codes.rs`, `api.rs`,
  `model.rs` and `model/`, `events.rs` and `events/`, `writes.rs` and `writes/`, `host.rs`,
  `pages.rs`, `themes.rs`, `assets.rs`, `icons.rs`.
- View models: `src/web/view.rs` and `view/`; tests in `src/web/view/tests.rs`.
- Templates (the frame, the sign-in page): `bins/svolderd/templates/`. Static files: `bins/svolderd/web/`.
- Tests in Rust sit beside each module (`src/web/router/tests.rs`, `auth/tests.rs`,
  `codes/tests.rs`, `events/tests.rs`, `writes/tests.rs`); they run a router against a fake
  daemon (`src/web/test_support.rs`). `make e2e` drives a volume write through the page over a real
  radio. The scripts and the effects are checked in a real browser: [testing.md](testing.md).
- Configuration: `crates/svolder-ipc/src/config.rs` (`WebConfig`) and
  `crates/svolder-ipc/src/token.rs`; the TUI's Web box is
  `bins/svolder/src/tui/boxes/web_box.rs`, and `svolder web` is
  `bins/svolder/src/commands/web.rs`.

[Back to the map](../architecture.md).
