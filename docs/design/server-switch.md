<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Turning the web UI and the MCP server on

The two servers of the daemon are off until `[web] enabled` or `[mcp] enabled` is true. Three
things set that without a hand-written file: a variable in the daemon's environment, a request on
the socket, and the CLI verbs and TUI settings in front of it. The socket and the file are in
[ipc-and-configuration.md](ipc-and-configuration.md); the servers in [web-ui.md](web-ui.md) and
[mcp-server.md](mcp-server.md).

## The environment's default

`SVOLDER_WEB_ENABLED` and `SVOLDER_MCP_ENABLED` are the default of their table's `enabled`, read
where `SVOLDER_WEB_LISTEN`, `SVOLDER_MCP_LISTEN` and `SVOLDER_WEB_URL` are: `ListenDefaults`
(`config/parse.rs`), which `Config::parse` reads from the environment and a test hands in
(`parse_with_listen_defaults`). The raw tables keep `enabled` as an option, so a key in the file
is told from none: the file's value wins, true or false, and only a table with no `enabled` takes
the variable. `1`, `true`, `yes` and `on` read as on, `0`, `false`, `no` and `off` as off, in any
case; a blank value is unset; anything else is a `ConfigError::Parse` naming the variable, raised
only where the variable decides. A server the variable turns on is judged as one the file turns
on: `auth = "none"` on an address that is not loopback is refused.

A configuration file that is not there reads as an empty one (`Config::load_from`), so the
defaults of the environment reach a daemon that has written nothing yet. That is the container's
first start: the image sets the two listen addresses, the compose file may set the two switches
(it carries them commented out), and no `config.toml` exists.

The writers of the two tables (`Config::update_web`, `Config::update_mcp`) compare the table as
the environment made it with the table after the edit, and write the keys that differ. Turning off
a server the variable turned on therefore writes `enabled = false`, which wins from then on;
turning on a server the variable already turned on writes nothing.

## The request

`Request::ServerSwitch { server, enabled }` (`server` is `web` or `mcp`; feature `server_switch`)
turns one server on or off. The handler (`handler/server_switch.rs`) makes the write through
`edit_then_reload` with the table's writer, under the file's lock and against the file as it is
then, and the reload that follows starts or stops the server (`servers_to`): a listener that was
never bound is bound at that reload, so nothing a switch changes waits for a restart. The answer
is `Response::ServerSwitch { web, mcp }`, where both servers stand, in the shapes `version`
reports (`WebStatus`, `McpStatus`).

A refusal goes through `ConfigWriteError::answer` and reads `invalid` (the file does not parse,
or would not with the key written), `read_only`, `file_busy` or `stopping` (after the stop mark
nothing is written). A port that cannot be bound, or a token that cannot be had, is not a refusal:
the key is written and the server's state in the answer is `failed`. The write waits five seconds
for the reload (`RELOAD_WAIT`); an answer given before a slower reload has run shows the servers
as they were.

`Request::GuestSwitch { read }` (feature `guest_switch`) is the same switch for `[web] guest`: `read`
writes `guest = "read"`, otherwise `"none"`, through `edit_then_reload` with `Config::update_web`
(`handler/server_switch.rs`), answered with `Response::ServerSwitch` whose web state carries the
guest mode. It is a request of its own rather than a field of `ServerSwitch`: a daemon that lacks it
answers an unknown kind, where an extra field on `server_switch` would be ignored by an older
daemon while it read the rest as an `enabled` switch. The reload sees a `[web]` section that differs
in `guest`, so the running web server is stopped and started again on the same address and answers
the new mode at once (open event streams end with the old server). A web UI that is off just has the
key written.

## The front ends

`svolder web on|off` and `svolder mcp on|off` (`commands/server_switch.rs`) follow the rule for a
front end that changes the file: with a daemon, the request and no write of their own; with a
daemon that lacks the feature, the sentence naming `svolder daemon restart`; with no daemon, the
table's writer on their own file. No daemon is what `Client::connect_if_any` calls none: no
socket, or one nothing listens on. Any other failure to connect (a socket this user may not open,
a path too long) may hide a daemon, so the verb fails naming the socket and writes nothing. They
print the server's status line (`WebStatus::line`, `McpStatus::line`) on stdout and exit 0 only
when the server stands as asked: one that cannot listen, and one the answer still shows as it
was, exit 1.

`svolder web guest on|off` (`guest_switch` in the same file) follows the same rule through
`Request::GuestSwitch`, and with no daemon writes `guest` with `Config::update_web`. It succeeds
when the web UI listens with the mode asked for, or is off (the key is written, and a note says the
page is off).

The TUI has the same two switches in Settings, Connect (the Web UI box and the MCP server row), and
`ConnChange::WebGuest` for the guest view (`tui/run/connect.rs`, the key itself is the Web UI box's).
It sends the switch to a daemon that has it and otherwise writes the file with the same writers and
sends `reload`.

## The MQTT client's keys

`svolder mqtt on|off`, `mqtt set host|port|username VALUE`, `mqtt set commands on|off` and `mqtt
password` follow the same rule through `Request::MqttSet { op }` (feature `mqtt_set`; `MqttOp` is
`enabled`, `commands`, `host`, `port`, `username` or `password`, the last two taking `null` to
clear), answered with `Response::MqttSet` carrying the bridge's `MqttStatus`. The handler
(`handler/mqtt_set.rs`) writes the one key with
`Config::update_mqtt` through `edit_then_reload`, so the reload restarts the bridge on the new
section. The verb then asks `version` for up to three seconds while the status is still a plain
`connecting`, so the line it prints can name a refused login. A refused login is a note, not a
failure: the key is written and the bridge runs; the verb fails only when the daemon did not act
(`on` with the bridge still off, `off` with it still running, `commands` with a `Connected` that
reports the other mode). With the client off, any key but `enabled` prints the status with what it
saved, `mqtt: off (saved: KEY = VALUE)`.

`MqttOp`'s `Debug` prints a password as `***`, no answer carries it, and the verb reads it from a
terminal without echo or from the first line of standard input, never from an argument.
`SVOLDER_MQTT_ENABLED`, `_HOST`, `_PORT` and `_USERNAME` are read by `ListenDefaults` as the
web and MCP switches are, and the file wins; `SVOLDER_MQTT_PASSWORD` is read where the daemon
connects and wins over the file. The TUI's Connect rows write the file with the same
`Config::update_mqtt` and send `reload`, as its other `[mqtt]` rows do.

## Where the code and tests live

- The defaults: `crates/svolder-ipc/src/config/parse/net.rs` (`ListenDefaults`, `enabled_switch`),
  the writers in `config/connections.rs`; tests in `config/tests/web.rs`.
- The request: `crates/svolder-ipc/src/socket/protocol/server_switch.rs`, tests in
  `protocol/tests/server_switch.rs`; the handler `bins/svolderd/src/handler/server_switch.rs`,
  tests in `handler/server_switch/tests.rs` (the guest switch, the running page following it, and each
refusal included); a first `web on` with no config file, against a
  spawned daemon: `bins/svolderd/tests/server_switch.rs`.
- The verbs: `bins/svolder/src/commands/server_switch.rs`, tests in
  `commands/server_switch/tests.rs`.
- The MQTT keys: `crates/svolder-ipc/src/socket/protocol/mqtt_set.rs`, the handler
  `bins/svolderd/src/handler/mqtt_set.rs` (tests beside it), the verbs
  `bins/svolder/src/commands/mqtt.rs` (tests beside it, and `bins/svolder/tests/mqtt_cli.rs`), the
  defaults and the writer in `config/parse/net.rs` and `config/connections.rs`.
- The compose file's switches and log cap: `bins/svolder/tests/docker_compose.rs`.
