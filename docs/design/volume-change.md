<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The life of one volume change

A press of `+` on the TUI's device screen, from the key to the reading that comes back. Every name
below is a real function; the tests of a file sit beside it.

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
