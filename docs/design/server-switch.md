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

The TUI has the same two switches in Settings, Connect (the Web UI box and the MCP server row).
It writes the file with the same writers and sends `reload`.

## Where the code and tests live

- The defaults: `crates/svolder-ipc/src/config/parse.rs` (`ListenDefaults`, `enabled_switch`),
  the writers in `config/connections.rs`; tests in `config/tests/web.rs`.
- The request: `crates/svolder-ipc/src/socket/protocol/server_switch.rs`, tests in
  `protocol/tests/server_switch.rs`; the handler `bins/svolderd/src/handler/server_switch.rs`,
  tests in `handler/server_switch/tests.rs`; a first `web on` with no config file, against a
  spawned daemon: `bins/svolderd/tests/server_switch.rs`.
- The verbs: `bins/svolder/src/commands/server_switch.rs`, tests in
  `commands/server_switch/tests.rs`.
- The compose file's switches and log cap: `bins/svolder/tests/docker_compose.rs`.
