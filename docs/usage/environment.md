<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Environment variables

The environment variables svolder and its daemon read, as one table that `make docs-env` keeps in
step with the build. Back to the [usage index](README.md).

## Environment variables

Every `SVOLDER_*` variable read anywhere in the workspace, one row each for what reads it and what
it does: `svolder help env` prints this same table for a terminal, and `svolder help env --markdown`
prints the table below. `make docs-env` rewrites it here from the built binary, and `make check`
fails naming that target when the two have drifted.

<!-- env-table:begin -->
| Variable | Read by | What it does |
|----------|---------|---------------|
| `SVOLDER_TRANSPORT` | svolder, svolderd | Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon`, `ble`, or `fake`. Default `auto` for `svolder`, `ble` for `svolderd`. |
| `SVOLDER_ADAPTER` | svolder, svolderd | Bluetooth adapter for a direct BlueZ session, e.g. `hci0`. Defaults to the configured `adapter`, else the first adapter BlueZ reports. |
| `SVOLDER_SOCKET` | svolder, svolderd | Unix socket path to serve or connect to. Defaults to `$XDG_RUNTIME_DIR/svolder.sock`, then `/run/user/<uid>/svolder.sock`, then `/tmp/svolder-<uid>/svolder.sock`. |
| `SVOLDER_FAKE_PROFILE` | svolder, svolderd | Profile the in-process fake device (`--transport fake`) uses. Default `gungnir2-v2`. |
| `SVOLDER_FAKE_UNIT` | svolder, svolderd | Unit id (8 bytes, hex) the fake device reports. Default `0102030405060708`. |
| `SVOLDER_FAKE_FIRMWARE` | svolder, svolderd | Firmware version the fake device reports. Default `3`. |
| `SVOLDER_FAKE_PASSPHRASE` | svolder, svolderd | Passphrase the fake device requires: four digits, or `none` for no passphrase. Default `1234`. |
| `SVOLDER_FAKE_FAULTS` | svolder, svolderd | `1` seeds every `Fault` control the chosen profile has as active. Unset by default. |
| `SVOLDER_FAKE_SIGNAL` | svolder, svolderd | `1` plays a signal on the meters; needs a profile whose meters have a scale, such as `fake-amp`. Unset by default. |
| `SVOLDER_FAKE_LOKI_REPORTS` | svolder, svolderd | Which id the fake Loki Max reports a band on: `full` (the default) is its three-id form, `6.<band>.3`, and `short` the two-id form, `6.<band>`, which a front end reads as the same band. Any other value is refused. |
| `SVOLDER_FAKE_SILENT_RECALL` | svolder, svolderd | `1` makes the fake Loki Max set its bands on a device-slot recall without reporting them, as a unit that pushes nothing after a recall would. Unset by default: a recall reports the bands it changed. |
| `SVOLDER_FAKE_QUIET_SAME` | svolder, svolderd | `1` makes the fake device report a change only: a Command equal to the value a control holds gets no Status, on every control. Unset by default: only the tone-control mode is silent on a repeat, and every other control is reported. |
| `SVOLDER_BLE_TRACE` | svolder, svolderd | Traces a real BlueZ connect's steps to stderr, one line per step with its elapsed time. Set to anything but empty or `0`; unset by default. |
| `SVOLDER_HOST` | svolder, svolderd | `off` keeps `svolder tui` away from the host's own audio for that process, whatever `[tui] host_audio` says: no `wpctl`, no `pw-record`, no `pw-play`, no session bus. `svolder host eq apply` and `remove` refuse with it set. For `svolderd`, `off` turns the host service off (no player watcher, no `pw-dump`, no cover reads), whatever `[host] enabled` says, and `fake` runs a fake player, a fake route and a bundled cover in its place, for demos and checks. Unset by default. |
| `SVOLDER_MQTT_PASSWORD` | svolderd | The MQTT broker's password, in place of `[mqtt] password`, so the password need not sit in the config file. Unset by default; an empty value counts as unset. |
| `SVOLDER_MCP_TOKEN` | svolder, svolderd | The MCP server's bearer token, outranking `[mcp] token_file` and the generated token file; with it set svolder generates nothing and `svolder mcp token --rotate` refuses. Unset by default; an empty value counts as unset. |
| `SVOLDER_WEB_TOKEN` | svolder, svolderd | The web UI's token, outranking `[web] token_file` and the generated token file; with it set svolder generates nothing and `svolder web token --rotate` refuses. Unset by default; an empty value counts as unset. |

**For tests and development:**

| Variable | Read by | What it does |
|----------|---------|---------------|
| `SVOLDER_FAKE_MORE` | svolderd | More live fake devices beside the first, for `svolderd --transport fake`: `<profile>@<UNIT>`, comma-separated, each answering its own unit id (16 hex digits) with the code `1234` and firmware `3`. The other `SVOLDER_FAKE_*` knobs describe the first device alone, except `SVOLDER_FAKE_SIGNAL`, which plays on every device. The daemon lists a device only when a `[[device]]` block names its unit. Unset by default. |
| `SVOLDER_FAKE_SEVER_AFTER_MS` | svolder, svolderd | Cuts the in-process fake device's first link this many milliseconds after it comes up, the way a device dropping the connection would, so a test can watch a reconnect. Unset by default. |
| `SVOLDER_FAKE_GONE_AFTER_MS` | svolder, svolderd | Takes the in-process fake device off the air this many milliseconds after it starts: its link is cut and it is neither heard nor reached again, so a test can watch a device out of reach. Unset by default. |
| `SVOLDER_FAKE_GONE_FILE` | svolder, svolderd | A path: the in-process fake device goes off the air, as with `SVOLDER_FAKE_GONE_AFTER_MS`, the moment a file appears there, so a test drops it on its own cue. Unset by default. |
| `SVOLDER_FAKE_COVER_GONE_AFTER_MS` | svolderd | With `SVOLDER_HOST=fake`, drops the fake player's cover art this many milliseconds after the host service starts, the track staying, so a check can watch a card lose its picture. Unset by default. |
| `SVOLDER_FAKE_PLAYER_GONE_AFTER_MS` | svolderd | With `SVOLDER_HOST=fake`, makes the fake player quit this many milliseconds after the host service starts, so a check can watch a card go to no player. Unset by default. |
| `SVOLDER_FAKE_COVER_GONE_FILE` | svolderd | A path: with `SVOLDER_HOST=fake`, the fake player's cover art is dropped, as with `SVOLDER_FAKE_COVER_GONE_AFTER_MS`, the moment a file appears there, so a test drops it on its own cue. Unset by default. |
| `SVOLDER_FAKE_PLAYER_GONE_FILE` | svolderd | A path: with `SVOLDER_HOST=fake`, the fake player quits, as with `SVOLDER_FAKE_PLAYER_GONE_AFTER_MS`, the moment a file appears there, so a test drops it on its own cue. Unset by default. |
| `SVOLDER_FAKE_ROUTE` | svolderd | With `SVOLDER_HOST=fake`, `changed` makes the fake route alter the signal in every way a route can (the player at 91 % and 44.1 kHz into a sink at 48 kHz and 54 %, the host EQ on, two other sounds mixing), so a check can see a changed path line. Unset by default: the route is untouched. |
| `SVOLDER_FAKE_CAPTURE` | svolderd | With `SVOLDER_HOST=fake`, `off` gives the fake host no capture: the visualizer's stream reports it unavailable, so a check can see the card say so. Unset by default: the fake host captures synthetic music while a page or a TUI watches. |
| `SVOLDER_FAKE_TITLE` | svolderd | With `SVOLDER_HOST=fake`, names the fake track in place of `Test Pressing`, so a check (or a look at the phone's folded line) can see a title too long for its room. Unset by default. |
| `SVOLDER_WEB_ASSETS` | svolderd | A directory laid out as `bins/svolderd/web/` that the web UI reads its stylesheet, scripts and fonts from on every request, in place of the copies built in: for working on the page without a rebuild (templates still need one). Unset by default. |
| `SVOLDER_SYSTEMCTL` | svolder, svolderd, make | The `systemctl` binary to run, for `daemon install`/`uninstall`, `doctor`'s Daemon check, and the daemon's own startup checks alike. Default `systemctl`. |
| `SVOLDER_AUTOEQ_URL` | svolder | The `results/` directory `eq search` and `eq import --autoeq` read AutoEq's index and files from, in place of AutoEq's repository on raw.githubusercontent.com: a mirror, or a local server in tests. Default is AutoEq's own. |
| `SVOLDER_SCREENSHOTS_AUTOEQ_URL` | make | The `results/` directory `make screenshots` downloads its one AutoEq file from, in place of AutoEq's repository on raw.githubusercontent.com: a mirror. Not `SVOLDER_AUTOEQ_URL`, which every test binary points at a dead address. Default is AutoEq's own. |
| `SVOLDER_WPCTL` | svolder, svolderd | The `wpctl` binary `svolder tui` reads and sets the host's default sink through (its volume, its mute and its name), `svolder host eq` reads the sinks through, and the daemon reads the default sink through to follow it with its capture (it never sets it). Default `wpctl` on `PATH`. |
| `SVOLDER_WIREPLUMBER` | svolder | The `wireplumber` binary whose `--version` tells `svolder host eq` and `svolder doctor` which WirePlumber this machine has. Default `wireplumber` on `PATH`. |
| `SVOLDER_PIPEWIRE` | svolder | The `pipewire` binary `svolder host eq` runs the computer EQ's filter chain with. Default `pipewire` on `PATH`. |
| `SVOLDER_HOST_EQ_MANAGER` | svolder | What keeps the computer EQ running: `systemd` (a user unit) or `process` (a detached `pipewire` and an autostart entry). Default `systemd` when the user manager answers, else `process`. |
| `SVOLDER_PW_RECORD` | svolder, svolderd | The `pw-record` binary `svolder tui` and the daemon capture the host's default sink's monitor through, for the spectrum and the host level and for the web visualizer; it only ever runs with that sink's node as its target. Default `pw-record` on `PATH`. |
| `SVOLDER_PW_DUMP` | svolderd | The `pw-dump` binary the daemon reads the PipeWire graph through, to find the default sink, the followed player's stream and what else plays into the sink; it is run read-only, only while a front end that shows the path is connected (a `subscribe` that asked for the host, the event stream of a DAC's web page), once at once and then every two seconds. Default `pw-dump` on `PATH`. |
| `SVOLDER_UNITDIR` | svolder, make | Where `daemon install` writes and `daemon uninstall` removes the systemd user units. Default `$XDG_CONFIG_HOME/systemd/user`, else `~/.config/systemd/user`; as root, `/etc/systemd/user`. |
| `SVOLDER_TEST_PREBUILT` | svolder test suite, make | `1` tells the daemon-spawning test helper that `make test`/`make check` already built every workspace binary, so it runs the one on disk. Unset, the helper builds the daemon into `target/test-daemon`, apart from `target/debug`. |
| `SVOLDER_WRITE_FIXTURES` | svolderd test suite | Set to anything, makes the web view's fixture tests rewrite their files in `scripts/web-checks/node/fixtures/` instead of comparing with them: `tone-curve.json` from the core's numbers (run `SVOLDER_WRITE_FIXTURES=1 cargo test -p svolderd curve_fixture` after the response maths changes). Unset, a stale file fails the test. |
| `SVOLDER_E2E_BIN_DIR` | e2e.sh | Directory holding prebuilt `svolder` and `svolderd` for `make e2e` to drive instead of its own cargo build, such as `target/dist/aarch64`. Unset, the run builds and uses `target/debug`. |
| `SVOLDER_E2E_ADV_ADAPTER` | e2e.sh | Controller `make e2e` advertises the peripheral from; set together with `SVOLDER_E2E_SCAN_ADAPTER`, or leave both unset to auto-detect two qualifying controllers. |
| `SVOLDER_E2E_SCAN_ADAPTER` | e2e.sh, svolder-ble tests | Controller `make e2e` scans and connects from; set together with `SVOLDER_E2E_ADV_ADAPTER`, or leave both unset to auto-detect two qualifying controllers. |
| `SVOLDER_E2E_UNIT` | e2e.sh, svolder-ble tests | Unit id `make e2e` starts the peripheral with. Default `0102030405060708`. |
| `SVOLDER_E2E_SIGNAL_UNIT` | e2e.sh | Unit id `make e2e` starts the signal section's own peripheral with, distinct from `SVOLDER_E2E_UNIT` so a stale BlueZ device object from the peripheral that section stops is never mistaken for the new one. Default `0102030405060709`. |
| `SVOLDER_E2E_LEGACY` | e2e.sh | `1` adds a step to `make e2e` that runs the simulator in its legacy layout and checks it advertises again after a link closes; needs bluetoothd started with `--experimental`. Unset by default. |
| `SVOLDER_E2E_LEGACY_UNIT` | e2e.sh | Unit id that step's peripheral advertises, distinct from the others for the reason `SVOLDER_E2E_SIGNAL_UNIT` gives. Default `010203040506070b`. |
| `SVOLDER_E2E_KEEP_SIM` | e2e.sh | `1` leaves `svolder-sim` running after a failing `make e2e` run instead of stopping it, and prints its pid to kill before the next run. |
| `SVOLDER_E2E_SIM_PID` | svolder-ble tests | The running `svolder-sim`'s pid, exported by `make e2e` so a live test can tell the peripheral apart from other devices on the air. |
<!-- env-table:end -->
