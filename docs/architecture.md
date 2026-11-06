<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# How svolder is put together

A map for someone who wants to change svolder: where a thing lives, and why it is there. The
words svolder uses in its own sense (unit, profile, function, link, session, writer, cap) are in
the [glossary](glossary.md). The wire format is in [protocol.md](protocol.md), what each command
and setting does for a user is in the [usage guide](usage/README.md), and what is known to be
rough is in [known-issues.md](known-issues.md).

Longer, per-subsystem design notes live in `docs/design/`, one file for each part of the system.

## Design docs

Each says how a part works now and the principles behind it.

- [The daemon](design/daemon.md): sessions, the scanner, the writer and the volume rule, reload.
- [IPC and configuration](design/ipc-and-configuration.md): the socket protocol, the
  configuration file and its writers, the launcher chain, tokens.
- [Profiles](design/profiles.md): the profile format, how profiles load and resolve, the safety
  data they carry.
- [The tone control, presets and AutoEq import](design/tone-control-and-presets.md): the
  parametric EQ model, recall, which preset is on, fitting a headphone correction.
- [MQTT and Home Assistant](design/mqtt-and-home-assistant.md): the bridge, discovery, commands.
- [The MCP server](design/mcp-server.md): the HTTP server, the tools, the volume rules for a model.
- [Host audio](design/host-audio.md): the computer's sink, players and capture, and the computer EQ.
- [The daemon's capture](design/host-capture.md): the holds, the shared capture, the frames on the
  web and the socket.
- [The terminal UI](design/tui.md): the app and its loop, screens and boxes, the key table and the
  three key rules, roles and palettes, the intro, the squirrel and the animation layer.
- [The web UI](design/web-ui.md): the request's way in, sign-in and tokens, view models, the stream,
  the components, writes, meters and the strip.
- [The web page's visualizer](design/web-visualizer.md): the looks, cost, the sink label, timing.
- [Dragging the curve's points](design/web-curve-dragging.md): the Response graph under a mouse and
  a finger: grab reach, the axis hold, Q by wheel, right drag and pinch, the swap hold.
- [Web themes](design/web-themes.md), [the frame](design/frame.md), [Paper](design/web-paper.md), [Signal](design/web-signal.md), [Night drive](design/web-nightdrive.md) and [its checks](design/web-nightdrive-checks.md): the contract.
- [Themes and effects](design/themes-and-effects.md): Ember's effects, the smoke, Arrange mode.
- [The capture file](design/capture.md): what `svolder capture` records, the scrub, and the daemon
  it stops and brings back.
- [Testing](design/testing.md): the gates and when to run which, the fake device, the simulator,
  end to end, the safety list, the docs check and the web checks.
- [How the code is laid out](design/code-layout.md): the layout rules, the families of folders
  and why each name is what it is.

## The three programs

- **`svolder`** (`bins/svolder`) is the command-line tool and the terminal UI. With no verb it
  opens the TUI (or a guided first run). Its verbs read and set a device, edit the configuration,
  run the host checks (`doctor`), manage the daemon, print protocol dumps and record a device
  into one scrubbed file (`capture`). Its root is small:
  `main.rs` dispatches, `cli.rs` is the clap definition, `front_door.rs` is what bare `svolder`
  does, `wizard.rs` is the setup wizard's state machine (the TUI drives it from `tui/wizard.rs`
  and draws a step in `tui/screens/wizard.rs`), and `backend.rs` is the join described below.
  Each verb group has a file in `commands/`, listed in `commands.rs`. The terminal UI is `tui.rs`
  and `tui/`, in families: `app/` (state, with no terminal in it), `run/` (the loop and its jobs),
  `screens/` (what each screen draws), `widgets/` (the row widgets a control screen is made of),
  `boxes/` (the state of the boxes that open over a screen; the toast and the tooltip take no key
  and draw themselves) and `decoration/` (the intro, mascot, effects, transition, title and
  wordmark). Decoration never carries a fact alone, so a reader adapting the controls can skip it.
  `tui.rs` names each child in one line. A name that repeats across these folders (`devices`,
  `presets`, `meters`, `web_box`) is one thing in three layers: its state in `app/` or `boxes/`,
  what the loop does for it in `run/`, and how it is drawn in `screens/`.
- **`svolderd`** (`bins/svolderd`) is the daemon. It keeps one session to every configured device
  and serves everything else from them: a Unix socket for `svolder`, the computer's player and
  signal path for every front end, and optionally the web UI, the MCP server and the MQTT bridge to
  Home Assistant. "The daemon at a glance" at the top of `bins/svolderd/src/main.rs` says what each
  module is: `handler` answers a socket request, `writer` is the write queue, `device` holds the
  actor of each device, `host` follows the player, `http_slot` holds one HTTP server, `mqtt_bridge`
  is the broker side, `web` and `mcp` are the two servers.
- **`svolder-sim`** (`bins/svolder-sim`) serves the fake device as a real Bluetooth peripheral,
  so the other two can be driven over a radio with no hardware. The fake device itself is the
  library crate `svolder-fake`.

A front end reaches a device one of two ways, and the code keeps them behind one type:

```
 svolder (CLI, TUI) ──┬── direct: opens its own Session ───────────────┐
                      └── via the socket ──► svolderd ─ Writer ─ Actor ┼─► Session ─► Link ─► device
 browser ──► web UI ──┐                          ▲                     │
 MCP client ──────────┼──────────────────────────┤  (all of them go through the Writer)
 Home Assistant ─ MQTT┘                          │
                                                 └─ one actor per device, one scanner shared
```

With `--transport auto`, `svolder` walks a chain: the daemon's socket, the socket unit systemd
holds, a daemon it starts itself, and last a session of its own
(`bins/svolder/src/backend/launch.rs`). Both ends hand the front end the same currency, a value
object per reading, so a verb or a screen cannot tell which it got: the `Backend` enum in
`bins/svolder/src/backend.rs` is the join, with `direct.rs` and `remote.rs` as its halves.

Why a daemon at all: a Bluetooth link is one connection that one process should own, and several
front ends (a TUI, a phone, Home Assistant) want to share it. The daemon owns it; everyone else
asks.

## The crates

Dependencies run strictly downwards in this list; nothing depends on a binary.

Each crate's `lib.rs` has a table of its groups. In `svolder-ipc` they are `socket/`, `config/`
and `checks/`, with `clock`, `error`, `passphrase_note` and `token` at the root because several
groups use them. In `svolder-profiles` the tone control's maths is `tone/` (`preset`, `response`,
`fit`, `apo`), the conversions are `convert`, `units`, `db_tables`, `codec` and `meter`, the
histories are `history`, the device rules are `roles` and `safety`, and the profiles themselves are
`model`, `registry`, `builtin` and `schema`.

| Crate | Owns | Depends on |
| --- | --- | --- |
| `svolder-proto` | The wire format: frames, values, the passphrase transform, the advertisement. Pure; no I/O. | nothing |
| `svolder-profiles` | Device profiles (TOML, with `extends`), the conversions between raw steps and dB, the tone control's maths (presets, response curve, fitting, Equalizer APO text), the volume safety rule, the history ring types. | proto |
| `svolder-fake` | `FakeDevice`: answers real protocol frames in memory, plus the `Signal` that plays the fake meters. | proto, profiles |
| `svolder-ble` | The `Transport` and `Link` traits, the per-device `Session` (connect, refresh, set-and-confirm, reconnect), `DeviceState`, the BlueZ transport, the fake transport and a frame tap (a transport decorator that records every frame sent and received). | proto, profiles, fake |
| `svolder-host` | The computer's own audio: the default sink's volume, MPRIS players (track, cover, position), the route to the sink out of `pw-dump`, the sink's monitor as a spectrum, the host EQ's PipeWire config. Each source sits behind a trait with a fake. | profiles |
| `svolder-ipc` | The socket group (`socket/`: the protocol `Request`, `Response`, `Event`, its server and client, socket paths, the daemon version check), the configuration file model and writers, bearer tokens, the host health checks `doctor` runs. | proto, profiles, host |
| `svolder-mqtt` | Home Assistant over MQTT as pure functions (topics, discovery configs, payloads, command parsing) and the one broker client. | proto, profiles |
| `svolder-autoeq` | The AutoEq catalogue: search, cache and download over HTTPS. | nothing |

The binaries sit above: `svolder` uses proto, profiles, ble, ipc, host and autoeq (fake only in
tests); `svolderd` uses proto, profiles, ble, ipc, host and mqtt; `svolder-sim` uses proto, profiles
and fake.

Two splits are worth knowing because they repeat:

- **Pure core, thin I/O shell.** `svolder-mqtt` builds every topic and payload without touching a
  network; `svolderd/src/mqtt_bridge.rs` feeds it and `svolder-mqtt`'s `client` talks to the broker.
  `svolder-proto` never opens a socket. `svolder-host` hides every external tool behind a trait.
  This is why most behaviour is testable without a radio, a broker or a sound server.
- **Data over code.** What a device is lives in `profiles/*.toml`, not in Rust. A control a
  profile declares appears in the TUI, the web page and Home Assistant without further code.

## The life of one volume change

A press of `+` on the TUI's device screen, from the key to the reading that comes back. Every name
below is a real function; `x/tests.rs` beside a file holds its tests.

1. **Key to intent.** `take_key` (`bins/svolder/src/tui/run/input.rs`) hands the event to
   `App::key` (`tui/app/dispatch.rs`), which turns a key press into a list of `Intent`s, and
   `App::on_screen`, which tries the keys every screen shares and then `App::device_key`
   (`tui/app/controls.rs`). `+` and `=` call `nudge("volume", 1)`, which finds the volume control
   in the profile and `step_level` builds `Intent::Set { key: "volume", value: "up", .. }`.
   Nothing here touches a radio or the clock; the `App` is plain state.
2. **Intent to backend.** `send` and `dispatch` (`tui/run/events.rs`) pass each intent through
   `App::paces` (`tui/app/glide.rs`), which drops a raise of the volume above three quarters of its
   range unless the glide's credit, a tenth of the range a second, pays for it. What passes goes to
   `execute`, which calls `Backend::set` (`bins/svolder/src/backend.rs`) without waiting for the
   echo. With a daemon, `Backend::set` builds a `Request::Set` and `RemoteBackend::write`
   (`backend/remote.rs`) sends it with `Client::request`
   (`crates/svolder-ipc/src/socket/client.rs`). With no daemon, `DirectBackend::set`
   (`backend/direct.rs`) builds the message itself (as in step 5) and goes straight to step 6.
3. **The daemon takes the request.** The socket server (`serve` in
   `crates/svolder-ipc/src/socket/server.rs`) calls `Daemon::handle_from`
   (`bins/svolderd/src/handler.rs`), which maps `Request::Set` to a `Command::Set` and calls
   `Daemon::write`. That resolves the device and submits the command to its writer with
   `Writer::submit`.
4. **The writer and the volume rule.** The writer (`bins/svolderd/src/writer.rs`) is the one
   door every front end's write goes through: the socket, the web page, MQTT and MCP, each as its
   own `Origin`. It queues writes per function, drops what a newer slider value replaces, and
   works out a relative step (`up`, `down`) from where the volume is heading, not from a stale
   reading (`Task::resolved`, `roles::next_step` in `svolder-profiles`). A write to an absolute
   level from the web, MQTT or MCP takes `Writer::glide`, whose task asks `safety::judge`
   (`crates/svolder-profiles/src/safety.rs`) whether the level may land at once or must climb.
5. **Command to frame.** The device's actor runs the command: `Actor::run_command`
   (`bins/svolderd/src/device/commands.rs`) calls `build_set`, then `build_relative` for `up`,
   which uses `roles::relative_step`, or `codec::intent` for a literal value. Both produce a
   `Message` for the function the profile names. `Actor::send` refuses the firmware-update family
   and the passphrase function before anything else.
6. **Message to bytes to air.** `Session::set` (`crates/svolder-ble/src/session.rs`; `set_no_wait`
   when nobody waits) calls `encode_command` (`crates/svolder-proto/src/frame.rs`), then the
   session's private `write`, which spaces writes and calls `Link::write`. For a real adapter that
   is `BluezLink` in `crates/svolder-ble/src/bluez/transport.rs`; for the fake, an in-process
   `FakeDevice` that answers real frames.
7. **The reading comes back.** The device answers with a notification. `Session::recv` reads it,
   its private `fold` runs `decode_notification` (`svolder-proto`) and
   `DeviceState::apply_notification` (`crates/svolder-ble/src/state.rs`) records it. The actor's
   loop (`bins/svolderd/src/device/link.rs`) wakes on it, and `Actor::publish_status`
   (`device/publish.rs`) sends an `Event::Status` to every subscriber and records it in the ring
   the web strip and `history` read.
8. **Back to the screen.** `Client::subscribe` feeds `RemoteBackend::next_status`
   (`backend/remote.rs`), which hands the TUI an `Update`. The loop calls `heard`
   (`tui/run/events.rs`), which calls `App::apply` (`tui/app/readings.rs`); the next pass of the
   loop calls `render::draw` (`tui/render.rs`) and the new value is on the screen. Every other
   front end, on the same stream, learns of the change in the same way: the web page over its
   server-sent stream, MQTT as a state message.

A press does not wait for step 7: the screen shows the old value until the device confirms, so
what is drawn is always what the device said, never what a key hoped for.

## Where the volume safety rule lives, and why

A DAC and an amplifier can be driven loud enough to hurt a person or a speaker, and several
front ends write to one volume. So the rule is one piece of pure code that the TUI, the web page,
MQTT and MCP ask. `svolder set` and `svolder raw` do not: an explicit command is taken as meant
and is outside the glide (the "`svolder set` is outside the volume's glide" entry in
[known-issues.md](known-issues.md)).

- **The rule:** `crates/svolder-profiles/src/safety.rs`. `judge` lays three rules over one
  asked-for step, in order: the cap a front end sets (`max_volume_db`, on the volume alone), a
  question for a rise of more than 6 dB over the lowest level of the last minute (MCP asks), and
  the motion: land at once, or, for the volume above three quarters of its range, climb at a tenth
  of the range a second. `Climb` steps that glide. Each front end has its own `Rules`
  (`Rules::tui`, `mqtt`, `web`, `mcp`) because their promises differ: a cap clamps for MQTT and
  the web page, and refuses for MCP.
- **Who applies it:** each front end judges with its own `Rules` and applies the cap (for MCP,
  the question too) before it submits: the web page in `bins/svolderd/src/web/writes.rs`, MQTT in
  `crates/svolder-mqtt/src/command.rs`, MCP in `bins/svolderd/src/mcp/tools.rs`. The daemon's
  writer then judges the glide with `Rules::writer`, which has no cap and asks no question
  (`Writer::glide`). The TUI's `glide` module (`bins/svolder/src/tui/app/glide.rs`) judges with
  `Rules::tui` (no cap, no question) and paces the climb itself, for a dragged bar or a raise past
  three quarters.
  A single `up` step moves one raw step, and the TUI still paces it: `App::paces`
  (`tui/app/glide.rs`, with its `StepPace` credit) lets a `+` or a wheel raise that would land above
  three quarters through only as the glide's credit allows, and drops it whole otherwise. The
  daemon's writer and the direct backend keep the step arithmetic honest: `roles::next_step`
  steps from where the volume is heading, so a lowering sent just before a raise is never undone.
- **The device-side refusals** sit beside it: `svolder-proto` cannot encode the firmware-update
  functions at all, and the daemon turns them away again in `Actor::send`.
- **The tests:** [`scripts/safety-tests.list`](../scripts/safety-tests.list) lists every test that
  guards the rule, the firmware-update refusals and the capture's command line, as
  `path|function`. `make safety-tests` fails if a listed function is gone, so none can be removed
  or renamed unseen. Most live in `bins/svolderd/src/writer/tests.rs`,
  `crates/svolder-profiles/src/safety/tests.rs`, `bins/svolder/src/backend/direct/tests.rs` and
  `bins/svolder/src/tui/app/glide/tests.rs`.

## The web UI's layers

All of it is in `bins/svolderd`, served by the daemon on a port of its own through the HTTP loop
the MCP server shares (`src/http.rs`).

| Layer | Where | What it does |
| --- | --- | --- |
| Routes and checks | `src/web/router.rs`, `auth.rs`, `codes.rs` | The `Host` check, security headers and CSP, the sign-in (token, cookie, one-use codes). |
| API | `src/web/api.rs`, `model.rs` and `model/` | The page's JSON models under `api/v1/` (a device's, the fleet's, the computer's player) and a device's strip; not a stable interface. |
| View models | `src/web/view.rs` and `view/` | What each part says, worked out from the profile, readings and writer; the model carries them. Pure, so tests read them directly. |
| Templates | `bins/svolderd/templates/` (`page.html`, `signin.html`) | askama templates checked when the daemon is built; `src/web/pages.rs` renders them: the page's frame and the sign-in page. A theme draws everything else from the model. |
| Writes | `src/web/writes.rs` | A control, the volume, a climb's end: all through the device's `Writer` as `Origin::Web`; Identify through the daemon's request. |
| Live updates | `src/web/events.rs` and `events/` | One server-sent stream of JSON events per page: the model, then what changed. |
| Static files | `bins/svolderd/web/`: `core/` (`frame.css`, `core.js`, `boot.js`, `store.js`, `stream.js`, `actions.js`, `api.js`, `toast.js`, `visible.js`, `tone-curve.js`, `tone-drag.js`, `login.js`, and the rest of the core's modules; [web themes](design/web-themes.md)), `themes/ember/` (`style.css`, `ember.js`, `live.js`, `press.js`, `strip.js`, `tone-motion.js`, `tone-graph.js`, `tone-editor.js`, `keys.js`, `facts.js`, `playing.js`, `knob.js`, `slider.js`, `meters.js`, `keep.js`, and the rest of the page's scripts), `fonts/`, the icons | Embedded in the binary by `src/web/assets.rs`; `SVOLDER_WEB_ASSETS=<dir>` reads a folder instead, for editing without a rebuild. |
| Effects | `web/themes/ember/effects.js`, `smoke*.js` beside it | A registry of page effects (Smoke, Glow, Off). The smoke is split in three: `smoke-model.js` holds every number (`TUNING`), `smoke-director.js` holds the rules and plans a frame with no GPU, `smoke-engine.js` runs the plan in WebGL, and `smoke.js` is the thin runtime. |

To restyle: colours and sizes are tokens at the top of `themes/ember/style.css`, a part's markup is
its file in `themes/ember/draw/`. A colour also appears in the icon (`src/web/icons.rs`, `icon.svg`)
and in a few scripts that read tokens (the strip, the dial). To retune the smoke, edit `TUNING` in
`smoke-model.js` and the numbers the checks pin (`scripts/web-checks/README.md` says which).

## Where each change goes

| You want to | Start at | Also |
| --- | --- | --- |
| Change what a TUI key does | `bins/svolder/src/tui/app/dispatch.rs` (global keys) or the screen's handler such as `app/controls.rs` | The hint table in `tui/keys.rs` draws the help overlay and each screen's own hint row (`tui/screens/`); keep them in step. Tests: `tui/keys/tests.rs`. |
| Add a device | A new `profiles/<name>.toml` (`extends` a `*-common` file) | One `profile!` line in `crates/svolder-profiles/src/builtin.rs`, by convention after its parents; its name in `product_name` in `crates/svolder-proto/src/advert.rs`; `crates/svolder-profiles/tests/builtin.rs`. A user can add one without Rust in `~/.config/svolder/profiles/`. The steps are in [profiles/README.md](../profiles/README.md); how to build, test and commit is in [CONTRIBUTING.md](../CONTRIBUTING.md). |
| Restyle the web page | `bins/svolderd/web/themes/ember/style.css` and `themes/ember/draw/` | See the layers above. |
| Add a web theme | A folder `bins/svolderd/web/themes/<id>/` and one `THEMES` entry in `bins/svolderd/src/web/themes.rs` | [Web themes](design/web-themes.md) says what it exports and which checks it passes. |
| Change a TUI look | `bins/svolder/src/tui/theme.rs` (roles to styles), `[tui]` settings in `crates/svolder-ipc/src/config/tui.rs` | Decoration (`bins/svolder/src/tui/decoration/`: `intro`, `mascot`, `effects`, `transition`) is separate from the screens, and a screen never names a colour, only a role. |
| Change a smoke look number | `TUNING` in `bins/svolderd/web/themes/ember/smoke-model.js` | The pinned values in `scripts/web-checks/node/`. |
| Add a Home Assistant reading | A profile `readout` control needs no Rust: `exposed_controls` in `crates/svolder-mqtt/src/discovery.rs` publishes it. A reading svolder derives itself touches `topics.rs`, `payload.rs`, `discovery.rs` and `bins/svolderd/src/mqtt_bridge.rs` | The entity table in `contrib/home-assistant/README.md` and the golden files in `crates/svolder-mqtt/tests/goldens`. |
| Add a CLI verb | `bins/svolder/src/cli.rs` (the clap variant, with the verb's flags and help), its arm in `run()` in `bins/svolder/src/main.rs`, and its module in `bins/svolder/src/commands/` with a line in `commands.rs` | The usage guide's `docs/usage/cli.md`. |

## Where the tests live

- **A module's own tests** sit in `x/tests.rs` beside `x.rs` (a module with many tests is a file
  and a folder), or in a `mod tests` at the foot of the file when they are few. A module file is
  always `x.rs` beside its folder `x/`, never `x/mod.rs`; the one exception is
  `tests/common/mod.rs`, because Cargo would build `tests/common.rs` as a test of its own.
  A large test module is an `x/tests.rs` that lists `x/tests/<topic>.rs`, one file per topic.
  Test names are whole sentences, so a behaviour is found by searching for its words.
  A helper that several modules' tests use lives in the narrowest `test_support.rs` that covers
  them all (such as `tui/run/`, `tui/` or the crate root).
- **A crate's integration tests** are in its `tests/` folder: `svolder-proto/tests/vectors.rs`
  (frames), `svolder-ble/tests/session.rs`, `svolder-profiles/tests/builtin.rs` (every built-in
  profile loads and covers its products), `svolder-mqtt/tests/` (discovery goldens, the client),
  `svolder-host/tests/`, `svolder-ipc/tests/adoption.rs`.
- **The programs' tests** run the built binaries against the fake transport:
  `bins/svolder/tests/` (CLI, TUI, daemon verbs, the usage docs' environment table),
  `bins/svolderd/tests/` (daemon, MQTT, MCP, reload).
- **The web page's checks** are scripts under
  [`scripts/web-checks/`](../scripts/web-checks/README.md): node harnesses for the smoke's
  director and model, and browser checks in headless Chromium and Firefox.
- **The gates:** `make check` (format, clippy, safety tests, docs check, licences, headers,
  tests), `make safety-tests`, `make web-checks-quick` after a page change, `make web-checks`
  before one is merged, `make e2e` over two real Bluetooth controllers. `make help` lists the rest.
- **Without hardware:** `SVOLDER_TRANSPORT=fake` or `make demo-safe` runs a fake device and a throwaway
  daemon (`make demo` adds your computer's real audio and the web UI); `make e2e` and `scripts/e2e.sh` drive `svolder-sim` as a peripheral on a second adapter.

## Read these ten files first

1. [`docs/protocol.md`](protocol.md): the frames and the functions, in prose.
2. `profiles/dac-common.toml` and `profiles/bifrost3.toml`: a device as data.
3. `crates/svolder-profiles/src/model.rs`: what a loaded profile is, and what a control can be.
4. `crates/svolder-profiles/src/safety.rs`: the volume rule.
5. `crates/svolder-ble/src/session.rs`: connect, refresh, set-and-confirm, reconnect.
6. `crates/svolder-ipc/src/socket/protocol.rs`: the requests, responses and events front ends
   exchange.
7. `bins/svolderd/src/handler.rs`: how the daemon answers a request.
8. `bins/svolderd/src/writer.rs`: the door every write goes through.
9. `bins/svolder/src/tui/app.rs`: the TUI's state, with no terminal in it.
10. `bins/svolderd/src/web/view.rs`: the web page's view models.

Every Rust file opens with a `//!` paragraph saying what it is for and why; read it before the
code.
