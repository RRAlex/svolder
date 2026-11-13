<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Changelog

Each version below becomes the top of its release notes. 0.25.4 is the first version with
downloads; the versions before it were source only.

## 0.33.0

- **A Devices popup on the web page**: the menu's *Devices…* row lists every configured device with
  a dot (connected or not) and the tag *Shown* on the one the page shows. **Overview** graphs every
  device's key readings over the last 30 minutes at once (signal strength first, then what the
  profile marks). A device's own page has its settings (name, profile, passphrase: Automatic, None
  or a 4-digit code, Make default), its facts, *Identify*, **All readings** and *Remove…*, each
  saved by its own control; *Show on the page* switches the shown device while the popup stays open.
  The four themes' Device fold-outs keep one line, *Device settings…* (*Devices…* for a reader),
  that opens it. A guest, or a viewer of a page with `[web] writes` off, gets the same popup
  read-only.
- **Add a device from the web**: **+ Add a device** at the end of the Devices popup runs the walk of
  `svolder setup` and the terminal's `n`, step by step: a warning first when a device is connected,
  the devices heard as they are heard, the name, the 4-digit passphrase when the device needs one,
  then the device's page. Reloading the page mid-walk takes it up where it stands; a walk started
  from the terminal shows read-only. The menu's `Device: <name> ›` row is gone.
- **AutoEq through the daemon**: `svolderd` is the only process that reaches AutoEq, and `svolder`
  links no HTTP client or TLS. The web page has an **EQ library…** popup (for a viewer who may
  write): search, group the headphones with their measurements, preview a correction for the device
  on show or the computer (target, fit, where it misses, how far to lower the volume), compare two
  measurements, save it as a preset, import a parametric EQ file, and write a device's curve over a
  preset of your own. With an imported preset on, each theme's tone graph shows its target, the
  places the bands miss it by more than 2 dB and the fit (`fit 0.9 dB`), and the fit follows a band
  you edit (the TUI's tone header too). `svolder eq search`, `eq import --autoeq` and the TUI's `a`
  ask the daemon; with none running, or under `--transport ble` or `fake`, they say AutoEq needs
  svolderd. Headphone names that read the same as letters and digits (`Koss UR 20`, `Koss UR20`) are
  one headphone.
- **A Help popup**: the menu's *Help…* row, or `?` in every theme (a guest too), lists the keys of
  what has the focus, the keys that work everywhere, and the daemon's version (once signed in). It
  replaces Ember's key card. Night drive's `↑` and `↓` do nothing for a guest.
- **The TUI and the CLI on the daemon**: with `svolderd` running, `svolder setup`, `svolder add` and
  the TUI's `n` run the device walk on the daemon (no Adapter step, a warning naming a device that
  holds a link, a walk running elsewhere is refused with where it started). The TUI's device editor,
  removal, default and the Web UI and MCP rows, and the CLI's `add UNIT` and `remove`, are the
  daemon's writes; a daemon too old for them is told to `svolder daemon restart` (exit 7). Ctrl-C,
  SIGTERM or a dropped ssh session cancel the walk that run started and leave with 130. The TUI's
  add-a-device scan lists devices as they are heard, and its AutoEq search box never waits for the
  daemon. Pairing codes are masked (`Tab` shows them) and never read back.
- **`config.toml` changed or missing**: `svolder daemon reload` makes the running daemon read the
  file and the profiles again. The daemon says when the file changed since its last reload (`svolder
  daemon status`, `svolder doctor`, the TUI, the Overview) and a hand edit takes effect at `svolder
  daemon reload` or SIGHUP, never on its own. A deleted file is told as missing, and every write the
  daemon is asked for is refused in those words instead of making a fresh file that would drop every
  device; `daemon reload` refuses and keeps the running configuration.
- **First run in a container**: `docker exec -it svolder svolder setup` and `docker exec -it svolder
  svolder` are the way in, with `docker logs -f svolder`. `svolder web on|off` and `svolder mcp
  on|off` switch the servers (a running daemon applies it at once; with none the verb writes
  `config.toml`); `SVOLDER_WEB_ENABLED=1` and `SVOLDER_MCP_ENABLED=1` do it from the environment,
  and a missing `config.toml` takes the environment's defaults. The compose file caps the log at
  three files of 10 MB, `svolder daemon logs` points to `docker logs` or `podman logs`, and the
  packages and image install `ca-certificates`.
- **Smaller changes**: a popup closes from outside only when the press and the release are both
  beside its box, and the page does not scroll under it; the browser tab's title names the open
  popup; an inline question is read to a screen reader with its answer. Signal's menu row *Add
  device…* reads *Add to the chain…* (devices are added from the Devices popup). Popup headings and
  Signal's lesser words read at a higher contrast. The web page is built for Firefox 140 and
  Chromium 142 or later. A page that showed "No devices" takes up the first device added. A device's
  name is checked when written (not blank, at most 40 characters). The daemon keeps a device's
  signal strengths for half an hour. Profiles can mark key readings, which `svolder devices --json`
  carries as `key_readings`. A daemon stopping during a reload closes every device it started, and
  `svolderd.service` allows 60 s to stop.
- **Security and community files**: `SECURITY.md` (private reports through GitHub's advisory form),
  `CODE_OF_CONDUCT.md`, issue forms and a pull request template.
- For contributors: the socket's `version` answer lists `features`, errors carry a typed `reason`,
  an unknown request answers code 7, and jobs (walk, AutoEq) have a registry with a stream and
  replay for late subscribers; while it runs the daemon is the one writer of the configured devices
  (`[[device]]`, `default_device`). The new `svolder-walk` crate holds the walk. The web core gained
  the modal stack, the history router, the key table, the popup frame and its shared parts; `make
  routes-doc-check` holds the routes to `docs/usage/web-ui.md`, `make network-check` joins `make
  check`, `make tui-hangup-check` (a pty run) is new, and new browser checks cover contrast, phone
  widths, popups and keys (the web checks need the apt package `node-jsdom`).

## 0.32.7

- **Doctor on a laptop's Bluetooth switch**: when the platform's own Bluetooth switch
  (`thinkpad_bluetooth`, `dell-bluetooth` and the like) is blocked, `doctor` says
  `sudo rfkill unblock bluetooth` for a soft block, or that a hardware switch or the firmware blocks
  it, in place of `bluetoothctl power on`.
- **Rootless Docker and Podman**: `svolder web --qr` and the pair link treat the address of
  slirp4netns or pasta (10.0.2.x behind 10.0.2.2) like a bridge address: they refuse it and ask for
  `[web] url`. The daemon docs say to use `ports:` and `[web] url` under rootless Docker.
- **A computer side in the demo**: `make demo-computer`, and `svolder tui --demo` without `--host`,
  now show a simulated computer in the TUI's Target row (volume, mute, EQ points and presets), which
  the web page shares, so a change in one shows in the other. Your real audio stays untouched.
- **A sandboxed `svolderd.service`**: the unit now refuses new privileges, set-id files, personality
  changes, real-time scheduling and new namespaces, allows one native system-call architecture, and
  limits sockets to Unix, IPv4, IPv6 and netlink. An opt-in drop-in,
  `systemd/svolderd-filesystem.conf`, also makes the file system read-only except the config, state
  and runtime directories; it needs systemd 254 and a host that allows user namespaces, and it is
  not proven end to end yet. See "The unit's sandbox" in `docs/usage/daemon.md`.
- **Toolchain and audit**: for contributors, release builds and the `check` workflow use Rust
  1.99.1, `rust-version` stays 1.93, and a `minimum` CI job builds and tests on 1.93.1. `make audit`
  runs `cargo audit` and allows only the `paste` and `ttf-parser` unmaintained warnings, by id; it
  needs the network and cargo-audit and is not part of `make check`. The release image has
  cargo-auditable, and `make static` builds with it, so the shipped binaries carry their dependency
  list.
- **Test broker and unit-check**: for contributors, the MQTT test broker sits behind a
  `broker-tests` feature in svolder-mqtt, svolderd and svolder; `make check` and `make test` enable
  it, and a plain `cargo test` no longer builds about 175 extra crates for svolder-mqtt.
  `make unit-check` verifies the shipped units offline with `systemd-analyze --user verify` where
  `systemd-analyze` is installed (the target prints SKIPPED for that step otherwise) and is part of
  `make check`. A flaky unit test that swept the scratch directory is fixed.

## 0.32.6

- **Faster CI**: the release run skips its test job when `check.yml` already passed on the tagged
  commit (push main, wait for `check.yml` to pass, then push the tag), and `make check` skips its
  private-bus test run where the system bus already has no BlueZ, as on GitHub's runners.
- **Home Assistant dashboard: Computer EQ**: `dashboard.yaml` has a small on/off badge for the
  computer's EQ in the Tone control heading, shown only while the `This computer` entities exist.

## 0.32.5

Fixes from a first user's Raspberry Pi 4 and Docker set-up.

- **Docker first start**: when `./config` or `./run` did not exist, Docker creates them owned by
  root and the daemon could not write its configuration. The image's entrypoint now hands a
  root-owned folder to your user (`PUID` and `PGID`, which compose takes from `HOST_UID` and
  `HOST_GID` in `.env`), says on stderr when a folder belongs to someone else, and runs the daemon
  without root's group; the published-image steps no longer need `mkdir` and `chown`.
  `docker exec svolder svolder ...` runs as the daemon's user.
- **Doctor on a Raspberry Pi**: an adapter blocked by rfkill gets `sudo rfkill unblock bluetooth`
  (a hardware block says so; each adapter is judged by its own switch), and a Bluetooth service
  that is not on the system bus gets `sudo systemctl enable --now bluetooth` (install bluez if it
  is missing). The Bluetooth guide has a Raspberry Pi section.
- **Web and MCP in Docker**: the compose files now give the container the host's network and the
  image listens on every interface, so the steps are `enabled = true` under `[web]` or `[mcp]` in
  `config.toml`, then `docker exec svolder svolder web token` to sign in; the QR code and pair link
  carry the host's LAN address. A native `svolderd` on the same machine must not serve the same
  ports. `svolder web --qr` in a container on a bridge network says to set `[web] url`. New
  `SVOLDER_WEB_LISTEN`, `SVOLDER_MCP_LISTEN` and `SVOLDER_WEB_URL` set only the defaults; the file
  wins. The web-address picker also skips Podman and CNI interfaces.
- For contributors: `make sim-app` starts the simulator for the Schiit phone app, after checking
  BlueZ's flags and printing the drop-in when they are missing; the release workflow check now
  requires every Dockerfile `COPY` source to be staged into the image's build context.
- For contributors: `make check` also runs the tests on a system bus with no BlueZ, as GitHub's
  runners have (`make check-no-bluez`), and a check workflow runs `make check` on every push to
  main, so a release's own test step holds no surprise.

## 0.32.4

- Editing a device so that it no longer shares a unit with another device is accepted; the edit is judged by the file it would leave, not the file it starts from.
- For contributors: `make tests-check` (was `make mutants`) checks that the web tests catch a deliberate break. It is slow, by hand only and not part of any gate.
- A test race in the events stream's host-hold test is fixed: it reads until the pause it asserts has arrived.

## 0.32.3

A fix release: the TUI's Target row is reached and drawn like the rows around it, a Yes/No question
on the web page owns the keyboard, a unit is configured once, and the daemon's reads of
`config.toml` leave its async threads. It also carries 0.31.3 to 0.32.2, which were never
released on their own; their notes follow.

- **Target row reached going down**: `↓` from the row above Volume (Mute) lands on the Target row
  first instead of skipping it, a further `↓` going on to Volume.
- **Target row chaser**: the Target row has a cursor of its own on the device and tone-control
  screens, so the trail and the shimmer play when the cursor moves onto or off it.
- **Target row look**: with the Target row selected, the side shown is in the highlight box, bold,
  and the other side plain dim, in colour and in monochrome; unselected, the side shown stays bold.
  A device with no volume gets the same look, keys and chaser on its Host Volume switch.
- **One unit, one device**: a unit id is configured once. `svolder add`, the setup wizard and the
  TUI's device editor refuse a second device on a unit another device carries, naming both, and
  the setup wizard's scan list leaves configured units out with a line saying which devices they
  are. A file that already has two `[[device]]` blocks for one unit (the wizard used to allow it)
  still starts the daemon and loads in every command: the first block is used, the later one is
  left out, and a warning says so (also in `svolder doctor`), for example `devices "dac" and
  "amp" are both configured for unit 1A2B3C4D5E6F7A8B; "amp" is left out and "dac" is used. Run
  `svolder remove amp` or edit the file to take one of the two blocks out`. `svolder remove` works
  on such a file; other device writes refuse until it is fixed.
- **Hop edits**: `svolder hops edit` and the page's hop edit read the hop they change under the
  configuration file's lock, so an edit that races another write keeps the other change; the page's
  answer to a hop write lists the file just written.
- **Web page, a question owns the keyboard**: while Ember's Yes/No question is open, the arrow
  keys, `m`, `0` and `f` no longer change the volume, the mute or the tone behind it; only the
  question's own keys act, and `T` no longer opens the theme picker behind it.
- **Web page, Paper's lost or waiting sheet is out of reach**: a list or a form open on the sheet
  no longer keeps it clickable when the stream is lost or the page waits for the device again; the
  form stays, with what was typed, and is usable again when the page wakes.
- **Web page, no theme can start**: when no theme starts, the menu and the picker no longer draw
  the failed theme's rows, and a card picked in the picker tries that theme.
- **Web page, waking after a lost link**: a page that was waking when its link dropped keeps its
  dial and sliders inert until the first reading of the reopened stream, as a page that woke
  without a drop does.
- **Web page, a device that leaves the fleet**: when the daemon stops listing a device the page
  follows, the page's stream is opened again without it, once, instead of going on asking for it.
- **Web page, a refusal is kept**: a volume raise the page drops after a refused write no longer
  replaces the refusal as the answer to that write.
- **The web page's config reads leave the async threads**: the live stream, page loads, tone and
  preset saves, the preset and hop writes and the computer's presets no longer read `config.toml`
  on the daemon's async threads.

Internal:

- `make mutants` runs every node web mutant, and `make mutants FULL=1` the browser ones too, with each runner's kills and wall time and a non-zero exit when a mutant survives. The browser runners share one pool (`MUT_WORKERS`, default from the cores and the CPU pressure), and Ember's and Paper's mutants run only the cases they name.
- **Ember's drawing**: its dial, meters and tone curve, on the first draw and on live updates
  alike, round a value that falls exactly halfway to the even digit, as the daemon's own text does
  (0.125 is `0.12`).
- **Container runs**: the two `backend` tests that cannot pass inside a container skip themselves
  there, with the reason on stderr.
- **Dependencies**: `fixedbitset` 0.5.9, `itoa` 1.0.19 and `uuid` 1.29.2, patch bumps only.
- **The web checks**: browser checks hold `/tmp/svolder-web-gate.lock` shared and the timing checks
  exclusive, the job count follows the cores and the CPU pressure, `make web-checks-task` runs the
  logic-only checks in Chromium alone, each run writes its per-check times to
  `target/web-checks/timing.tsv`, and the README in `scripts/web-checks/` describes the ladder. The
  quick and task gates reuse a check's pass while the files it reads are unchanged and report it as
  `cached` (`WEB_CHECKS_NO_CACHE=1` runs everything), and Night drive's Still and readout rates are
  checked against a fake clock.
  New cases: a stream's `close()` and its wait while lost, a fleet that prunes, a kept refusal,
  and Night drive's mouse hover at the tone band's edges.

## 0.32.2

Groundwork for the Bifrost 3: the simulator can play the unit's USB, S/PDIF and sample-rate
readings, the TUI picks between the DAC and the computer, and `make help` reads as a short menu.

- **The simulator's knobs**: `svolder-sim --profile bifrost3` starts with `--usb on|off`,
  `--spdif-locked on|off` and `--sample-rate HZ` (`44100` or `44.1k`; USB off, S/PDIF unlocked and
  48 kHz by default) and, while it runs, takes the lines `usb 1|0`, `spdif 1|0` and `rate 96000`
  on its standard input. Each change goes to the connected central at once as a Status, so the
  Status pane, `status --json` and the web page's readings follow. A bad value is one line on
  standard error and the sim keeps going. The S/PDIF lock goes out as a UInt8, as a Bifrost 3
  sends it.
- **Firmware labels**: a unit's readings 210.2.1 and 210.2.2 read `Firmware` and `DSP firmware`
  in the CLI, the TUI and the web page.
- **Target in the TUI**: a `Target` row on the Controls pane and at the top of the tone screen
  picks the DAC or the computer with `←` and `→`, the same switch as the web page's. The Volume and
  Mode rows keep their own values; the row says why when the computer cannot be used.
- **`make help`** is in sections with one short line per target; `make help T=<target>` gives the
  details, with each setting and the values it takes. On a terminal the sections and targets are
  bold, the settings dim and the warning about your real audio yellow; a pipe, `NO_COLOR` and
  `TERM=dumb` get plain text. `make help-check` is part of `make check`.
- **`third-party-licenses` is now `3rdpartylicenses`**, the make target that writes the notices.
- **Docs**: when you read BlueZ state by hand with several adapters, `bluetoothctl list` then
  `select <adapter address>` first, since a bare `bluetoothctl info` answers for its default
  controller only; the simulator's standard input when it runs as a background job.

Internal:

- **Dependencies**: `cargo update` within the current versions (rmcp 3.9, ureq-proto 0.7, toml 1.2,
  unicode-width 0.2.3 and 32 more); builds and the whole test suite pass, and `cargo audit` (0.22.2)
  finds no known vulnerability, only two unmaintained-crate warnings (`paste` and `ttf-parser`),
  which are in the known issues.

## 0.32.1

A burn-down after the Computer flip: the computer's EQ is steadier in the four web themes and in
the daemon, and a machine that was missing its chain now finds it.

- **Ember**: the computer's points table has `−` and `+` steppers for the point you picked, in the
  look of the DAC's table (hold one to keep stepping; the last value is written when you let go).
  The tone part's DAC and Computer swap now waits for wheel and stepper bursts on Ember's computer
  curve, as it already did for drags and steppers, so a flip no longer replaces the panel under
  your hand.
- **Paper**: a number typed in a cell reads `2.9k` and a comma (`+3,2`), and a word that is no
  number is said and the cell put back. The wheel and the arrow keys on a point are one burst, so
  ten notches make ten live updates and one saved write instead of ten saved writes. A tap that
  adds a point needs under 6 px and 600 ms (it was 8 px with no time limit), and a second finger
  anywhere on the page drops a waiting tap, so it adds no point.
- **Night drive**: the computer's points table has `−` and `+` steppers on each value (a phone
  shows a point as a block), and a held stepper ends when its point is removed by another front
  end instead of going on to the point that took its number. The lit bell, its label and a drag
  follow the points when they are put in order. Two fingers pinch the point in hand; the question
  for a preset a device plays too is asked in the sheet with Apply and Keep.
- **Signal**: an arrow key on a point is a live update and one saved write (two posts for one key),
  a typed value equal to the point's own value writes nothing, and the player hop reads
  `PipeWire volume 54 %`.
- **In every theme**: a typed value is rounded to its field (frequency to 0.01 Hz, gain to 0.1 dB,
  Q to 0.01) and Enter on a cell that has not changed writes nothing, even for a +14 dB point.
  Removing the point in hand puts none in hand. A tap on a full table says `16 is the most`. The
  tone curve, the marker and the label follow a point that moved when the table was put in order.
  A comma in a typed value is read both ways, as a decimal mark and as a thousands separator, and
  the field's range picks the one that fits (`1,000` Hz is 1000, `20,500` Hz is 20.5); when the
  range cannot tell (`20,000` Hz) the cell keeps its value and says `Write 20 or 20k.`
- **Where the computer's chain is found**: a machine whose wireplumber says it is older than 0.5
  while its `wpctl` is 0.5 lists the filter-chain sink under `Filters:`; svolder now reads both
  headings, so the computer EQ finds its chain there.
- **The daemon**: a refused turn-on no longer keeps the sink's rate in mind, so the same load
  plays once the sink is back at a rate it can; retries while a turn-off is held write nothing to
  the state file, and a late success keeps the preamp it measured at the rate that played. A save
  over a state file written by the command line meanwhile sends the screen's points at the next look, to the
  command line's chain. The terminal of `pw-cli` reads raw, so a 16-point update is not cut at
  4095 bytes.
- **The TUI**: the write queue's bound of 64 and the move back to the DAC (the computer's EQ goes
  off first, and a refusal leaves both sides as they were) have tests.
- **Docs**: what applies after a live softening, what a reload does to the node bound, and the
  quieter-until-an-apply price of the node cap, in `docs/design/host-control-preamp.md`.

Internal:

- **The computer side's shared web code** (the typed-number words, the point hand with its drag,
  tap, touch, pinch, wheel and key bursts, the table, the computer view, the presets and the
  volume steps) moved into the web core and the four themes call it; their requests are the same
  but for the lines above. Each helper has a node case written first and mutants that fail it
  (`mutate_computer.sh`), and `contract.py` pins the core's members.
- **The preamp lab** has a liveness rule (a quiet node plays the view's curve under its preamp), a
  reload-writes-nothing rule, a quiet tail after every sequence and a targeted short run beside the
  ordinary one; the controller's checked request and the session's deliver, settle and node run
  once, over a small link trait, for the daemon and the lab.
- **Checks**: Night drive's wet-road case pins the measured draw time (it raced the draw budget
  under Firefox load), a second case pins the budget's hysteresis, and `ND_ONLY=wet_road` runs
  them alone. The 16-point drag and the chain's node id before and after a restart are in
  `make host-e2e`. `computer_paper` is in both lists of `run-all.sh`.

## 0.32.0

The web page, the terminal and Home Assistant can now change the computer's own sound: one switch
flips the volume and the tone control from your device to the computer, whose EQ you edit point by
point while the music plays, with no gap, in every theme.

- **The switch**: `DAC │ This computer` flips the volume control and the tone control together, on a
  DAC's page and an amp's. Flipping writes nothing and the side is kept per viewer; a guest can
  flip to look but every control stays quiet. Where the computer cannot be used, a sentence says why.
- **The computer's volume**: the default sink's volume and mute, 0 to 100 % in steps of 1 %, never
  a boost. The first paint reads `—` rather than a live 0 %.
- **The live computer EQ**: up to 16 points (peak, low shelf, high shelf), the first always there.
  Drag one, turn the wheel or pinch for its Q, tap an empty spot or `+ Add point` to add one, type
  a value, change a type or remove a point; the sound changes with no gap. The table is put in
  order by frequency when an edit ends, never under a finger, and removing the last point sets it
  to 0 dB. The preamp follows the curve's peak so the signal never passes unity, and a line says
  by how much. A sample-rate change that leaves the curve unplayable turns the EQ off and says why,
  and an EQ chain left by an older svolder is named in the note until the EQ is turned off and on
  or edited. Off takes the EQ out of the path and keeps the points. The path line says
  `host EQ on` for a chain in the path, flat or not. On a PipeWire that does not take live
  changes it applies when the drag ends and says so once.
- **Shared presets**: the same list on both sides; built-ins of the shown device and saved ones
  play on the computer as written, and `Save as…` keeps the computer's points. A preset that a
  device plays too would play twice, so the web page, the TUI and MCP ask first (Yes flattens the
  device, No changes nothing).
- **Ember**: a pill beside the device's name; the curve, the points table and the presets are the
  DAC's own parts.
- **Paper**: `showing: the DAC · this computer` under the title, the chosen one underlined.
- **Signal**: choosing the computer's hop is the switch, and its facts (format, other sounds, the
  path verdict) stay below the controls.
- **Night drive**: two keys by the menu button, and on a phone one small `DAC │ PC` pill in the top
  left; the dots sit on the mountains and a drag works only inside the curve's band, so a tap
  outside it is a tap on the scene. The mouse wheel over the volume number and `↑` `↓` set the
  volume, a faded dash fades to a fifth rather than a tenth, the phone's panel is two translucent
  rows over the scene (about nine tenths), and choosing Full measures the phone's speed at once
  instead of waiting, so the scene settles in about two seconds.
- **The TUI**: `C` flips the EQ tab (`EQ for ‹ NAME | Computer ›`, mode `Off | EQ`, a Points
  pane with `a` add, `x` remove, `y` type, Enter or a double-click to type values) and the
  Controls tab's Volume row. The separate host keys are gone; the computer-side preset list
  applies on the computer and asks the both-sides question.
- **MCP**: `get_host`, `set_host_volume`, `host_mute` and `set_host_eq`, with the volume rule's
  cap and its question for a rise of more than 6 dB.
- **MQTT and Home Assistant**: one `This computer` device with a computer-EQ switch (ON puts the
  kept points back, OFF removes the EQ) and a sensor naming the applied preset.
- **Packages and Docker**: the deb and rpm recommend the PipeWire tools, the AUR lists them with
  what each is for, and the Nix package puts them on its PATH. The image carries them, and the
  opt-in `docker-compose.host-audio.yml` mounts the PipeWire socket; `make docker` sets it up.
- **`make demo-computer`** (and `make demo-computer-amp`) shows the page with a simulated
  computer, your own audio untouched.
- **`make host-e2e`** plays a tone through the real daemon and chain on a private PipeWire and
  WirePlumber with no sound card, drags a point, changes a type and the sample rate, and checks
  the level at each step. It is not part of `make check`.

## 0.31.5

The web page gets its fourth theme, Night drive: a synthwave scene that plays your music. 0.31.4
and 0.31.5 ship as one release.

- **Night drive**: pick it from the menu's Theme row, with `T`, or with `?theme=nightdrive`. It is dark only. A sun sits on the horizon above a road and a
  night sky of stars; the music drives all of it.
  - **The mountains are the analyzer and the tone curve.** The ridge is the live spectrum, a
    smooth line through the levels with a slow swell at silence. Drag a point on it to tune the
    EQ; the wheel or a two-finger pinch sets a band's Q. The curve rests when nothing tunes, and a
    lit pink highlight on the curve shows where a dot acts.
  - **The floor is heat.** Its cells rise into short neon bars with the music and glow on loud
    ones; the heat's low end is spread over more colours.
  - **The mood** follows the song: the night and the sun follow the music's own range, and the
    stars, the sun's rays and the wet road answer the band's envelope.
  - **Sky events** are rare: shooting stars and showers, rays turning slowly while they show.
  - **The dash** holds the volume, the input, the mode, the EQ line and Now playing, with a very
    small cover when there is one. Like the HUD's words it fades after 15 s, to a faint 10 % on
    a desktop and on a phone held sideways, and wakes on a move or a key; the first tap or click
    on a faded dash only wakes it, however long it is held.
  - **The sheet** under the scene carries every control and fact the device has, an amp's
    meters and faults included, so nothing needs the scene to be reached.
  - **On a phone** the scene keeps the top, the dash shrinks to its rows at the bottom in
    portrait, and the sheet follows, with more room for the tuning.
  - **Motion levels** (Full, Calm, Still) set how much moves. A frame
    budget watches the draws: a slow screen steps down, a phone draws at most 60 a second in Full
    and blurs nothing, and the scene returns once the draws measure under budget. A readout names
    the level in use on a slow phone.
  - Its fonts, Russo One and Mr Dafoe, are shipped with svolder.

Night drive and Signal's second pass gave every theme these, from the shared core:

- **The tab's title** is the core's and names the device and its state.
- **Held − and +**: pressing a volume, band or strength − or + keeps stepping, in every theme.
- **Space plays and pauses** the computer's player.
- **The theme picker** has a close button, scrolls on a phone, and, with the menu, follows the
  visible window and scrolls inside a short one.
- **The menu button** is the same 44 px button in the corner in every theme, on an opaque backdrop.
- **The device's link** is said in words beside its name in every theme (a green dot while
  connected, the state's word otherwise). Ember's connected pill is gone from beside the menu.
- Now playing shows the album cover in Signal and Night drive, and in monochrome in Paper.
- A muted volume reads −∞ dB in Ember, as in the other themes.
- Signal's guest check reads the real requests, so a guest is verified to write nothing.
- A canvas loop may run under reduced motion and at a pace, and the analysis rules (the held
  bass, the minute's range) are the core's.

Also:

- **The CLI demo**: `svolder tui --demo --host --web` opts the demo into the computer's real
  audio and the web page; the default demo stays safe. `make demo` uses them, and
  `make demo-dac` and `make demo-amp` start a DAC or an amp demo.
- **The TUI's Spectrum row** spans the width of the Controls pane, and its colours are softer.
- **Fixes in every theme with a volume:** pressing − or + to a step you had asked for a moment
  before, after another hand set the device back, is written again (Signal, Paper and Night
  drive dropped it). A change made right after switching to another device can no longer reach
  the previous one. Space is named in the key lists. On a phone, choosing a hop in Signal scrolls
  all the way back to the top.
- **The demo** can no longer hang at exit while it removes the computer EQ it applied.
- **The web gate** runs its checks in parallel, six at a time by default (`WEB_CHECKS_JOBS`
  sets it); a failure re-runs alone, and a check that passes only then is reported as flaky.
  The draw-timing checks run alone in a serial lane, a browser never outlives its check, and
  Firefox picks its own port for the checks' connection.

## 0.31.4

The web page gets its third theme, Signal: the sound's way from the player to your ears, drawn as
one line.

- **Signal**: pick it from the menu's Theme row, with `T`, or with `?theme=signal`. It is dark
  only. Left to right the line runs from the player, through the computer, to the device svolder
  shows, then on to whatever comes after it. A solid dot is something svolder reads, with its
  live facts under the name. A plain ring is a hop you declared, because svolder cannot see an
  analogue signal. Moving dots show the sound flowing while the player plays; they stand still
  when it pauses, while the page is hidden and under reduced motion.
- **Choosing a hop** (click, tap, or Left and Right) opens what it offers underneath:
  - the DAC: the large volume with −, Mute and + (hold − or + to keep stepping; muted reads
    −∞ dB) and the tone curve to drag (the wheel or a two-finger pinch sets a band's Q). The
    DAC says `arrives as sent` when it reads the rate the computer sends,
    `changes the sound` when it does not, and neither while that is not read;
  - a linked Forkbeard amp: its power, meters and faults, writing to the amp only;
  - the computer: its sink, rate, volume and the computer EQ, read only;
  - the player: Now playing.
  Below the first screen come the band table with the input under it, beside the mode, the
  presets and the controls, in two columns, then the device's facts. The mode heads the second
  column; while Loudness is on its button carries a round − and + for the strength, and holding
  one, or a band's − or +, keeps stepping. Zoomed out on a tall window, the page stays together
  in the middle. A slim bar keeps the hops in reach once the line has scrolled away.
- **The tab's title** names the device and how the page stands with it, for example
  `Bifrost 3 · connected — svolder`, or `reconnecting` while svolder is out of reach.
- **Declaring hops on the page**: the menu's *Add device…* row adds an amp, headphones, speakers
  or anything else after the device, and a declared hop can be edited, moved or removed in place.
  A Forkbeard amp the daemon lists but has not linked is offered first, and choosing it links it.
  While an AutoEq preset is applied on the computer and no headphones are declared, the line ends
  with a ring named after the preset.
- **On a phone** the line is a strip across the top with the chosen hop in the middle. Swipe it
  or tap a neighbour; it stays at the top, with the menu button, as the page scrolls. The mode
  sits under the curve, and the shelf opens with the input and the presets, then the band table.
- **Trouble**: a device out of reach dashes the line into a hollow ring, stills what follows and
  shows a card with *Try now*. A fault keeps its controls live. A guest sees every control
  drawn but inert.

Signal found four things the shared core lacked; all are fixed for every theme:

- The page model carries the computer's route (sink, rate, format, volume, mute, the computer
  EQ), not only its words.
- The daemon reads the computer EQ's saved state, so a page can name the AutoEq preset that is
  applied. A newly applied preset shows within a second.
- On its way down the daemon stopped the computer's player before the web server, so an open
  page heard the computer go away and dropped its Now playing card. The servers now stop first,
  and a page keeps the last thing it heard, dimmed, while the connection is lost.
- A theme can place the menu button against the window instead of the page
  (`anchor(el, { fixed })`), which a pinned strip needs; Ember and Paper place it as before.

## 0.31.3

Paper's follow-ups, and releases that start themselves.

- **Paper**: the red volume note's arrow no longer runs over the tone section. It climbs the
  margin on the needle's side and ends at the volume's written value.
- **Paper**: The device (the facts table, every reading and Identify) starts folded. Its heading
  unfolds it, and the choice is kept per browser; it still opens in a private window.
- Paper: a device switch during a point drag ends the drag cleanly; a question returns the focus
  to the word that opened it after a redraw; the ruler's labels keep their gap after a resize; the
  Caveat font names its Medium weight.
- **Releases**: pushing a version tag starts the release workflow, which now also runs the whole
  test suite on arm64 before anything is published. Each release's notes open with the Beta
  paragraph.
- `svolder-sim --trace` stamps every line with the milliseconds since the simulator started, so
  two traces can be compared for pacing and reconnect time.

## 0.31.2

The web page gets its second theme, Paper: the device drawn as a spec sheet someone wrote on.

- **Paper**: pick it from the menu's Theme row, with `T`, or with `?theme=paper` in the address
  (`?theme=paper.light` and `?theme=paper.dark` choose the paper too). The volume is a ruler with
  a needle you drag; the tone is a curve with points you drag; every other control is written,
  and an underlined word acts (`− quieter · mute · louder +`, an input's name, a preset's name, a
  band's `−` and `+`). It covers everything Ember does, on a DAC and on an amp.
- **Handwritten notes**: in blue ink, Paper says in words what the EQ does, whether the rate
  matches all the way to the DAC, where the track is, the firmware and the link. One note at most
  is in red pencil and circled: a device off the air or at fault first, then a loud volume, then
  an EQ that can clip, then a path that changes the sound. **Last change** says what changed and
  when, and "from this browser" (or "from this phone") only when this page asked for it.
- **Light and dark paper**: Auto follows the system; Light and Dark are kept per browser. The
  inks stay the same on dark paper, a little lighter.
- Paper's fonts, Crimson Pro and Caveat, are shipped with svolder under the SIL Open Font Licence.

Paper found seven things the shared core lacked; all are fixed for every theme:

- The track's album is its own field in the model (it was only inside the source line).
- A theme gives the browser a colour for each scheme, and the page's colour scheme follows the
  one chosen, not only the system's.
- Switching themes keeps the new theme's own scheme, so a dark Paper reloads dark.
- At boot the scheme the server painted (from the address or the cookie) wins over a stored one.
- The toast's shadow is a theme colour, so it suits a light page.
- The band steps and the drag's writes live in the core, for any theme with a tone graph.
- The frame declares a serif and a hand font for themes to use.

The README now opens with what to know before plugging in (beta software, a low volume, cheap
headphones first) and sets up Home Assistant and an AI assistant in a few lines.

## 0.31.1

The official app 1.12 shows the Bifrost 3; nothing else on the wire changed. A run of the app
against the simulator found two gaps, closed here.

- **A write of the value a device already holds no longer fails with "no confirmation within
  2s"** when the device stays silent: after the usual wait, svolder answers with the value the
  device reported on this link, if it matches. A device that answers a repeat still answers at once.
- **The simulator and the fake device report the tone-control mode only when it changes**, as the
  official app needs (it answers every report of the mode with a write of it).
  `SVOLDER_FAKE_QUIET_SAME=1`, or `svolder-sim --quiet-same`, makes every control silent on a
  repeated value.
- **The simulator advertises again after a link closes**, in both layouts, so a phone finds it
  after a disconnect. `make e2e` checks it, and `SVOLDER_E2E_LEGACY=1` repeats the check in the
  layout a phone scans.

## 0.31.0

The web page gets themes: a shared core and frame, with today's look as the first theme, Ember.

- **A theme menu on every screen**: the round button left of the link's pill opens the Theme row,
  the theme's own rows, Device and Sign out (or Sign in), by mouse, touch or keyboard, for a guest
  too. `T` opens the theme **picker** from any theme; each browser keeps its own choice, and
  `?theme=ember` in the address always gets Ember back. Ember is the only theme for now.
- **Arrange sections** (`A`, or the menu's *Arrange sections…*): drag Ember's sections between the
  band across the top and the two columns, by handle or with the arrow keys; the order is kept in
  this browser, per device kind.
- **Hops**: `[[hop]]` blocks in the configuration declare what the sound goes through after the
  DAC (an amp, headphones, speakers), named by hand with a kind or linked to a configured device,
  with an optional note. `svolder hops` lists, adds, links, edits, removes and moves them, and the
  web API writes them the same way.
- **The page is drawn from one model**: the daemon sends the page a JSON model and a JSON event
  stream, and every write answers JSON; safety stays in the daemon (the volume rule, the slot-save
  confirmation, write pacing, sign-in and guest limits) unchanged.
- The sign-in page keeps Ember's look in every theme.

## 0.30.1

Fixes after the Loki Max presets.

- **TUI band presses on a Loki Max no longer queue behind each other**: a band write sent without
  waiting lets the next write go at once; only a preset recall, or a built-in setting, waits up to
  2 s for the band's report. A band whose report is late, or that was written again before it
  came, counts as unread after the next recall, so a setting writes it instead of trusting a report
  from before the recall.
- **The MCP question for a built-in setting names the level it was judged against**, and a `yes`
  counts only under that level, even when a band reports a lower one during the call.
- **A profile cannot make a device-slot action change the volume or the mode**: the loader
  refuses a `changes` list that names either.
- `svolder set save N --json` off a terminal without `--yes` prints its refusal as one JSON object.
- On a device with no volume or no toggle (the Loki Max), the Device screen's `+/-` and
  `Space`/`Enter` hints are dimmed with the reason, instead of lit and doing nothing.

## 0.30.0

The Loki Max's presets: its three device slots and its five built-in settings, from every front
end.

- **Recall and save the Loki Max's three device slots**: `p` in the TUI lists Preset 1 to 3, Enter
  recalls one and `u` saves the bands into it after `Save the bands to preset 2? [y/N]`;
  `svolder set recall 2` and `svolder set save 2` do the same from the CLI (a save asks on a
  terminal and needs `--yes` off one). The web page recalls a slot and saves to one behind a
  confirmation, Home Assistant's `Preset` select recalls one, and the MCP server recalls one after
  asking, since a slot can set a band as high as +12 dB. A save is offered only where it can ask.
- **The five built-in settings** (Flat, Bass Boost, Treble Boost, Smiley, Loudness) apply from the
  same list, `svolder preset list` and `svolder preset apply NAME`, the web page, Home Assistant
  and MCP, as one batch that lowers bands before it raises any. The one the bands match is marked.
  Over MCP a setting that raises a band more than 6 dB above its level of the last minute asks
  first.
- **A Loki Max's bands are read whichever way it reports them**, on the band's own id or the
  shorter one the official app's presets write.
- **After a recall, svolder asks the Loki Max for its bands once** if none are reported within a
  second, so no screen keeps the bands from before the recall. Nothing has been tried on a real
  Loki Max yet; the known issues list what a unit has to answer.
- Profiles gain an `action` kind (a command with a slot and no reading), `reads` (other ids a
  control is reported on) and a table of built-in settings; `svolder profiles show` lists them.

## 0.29.5

Ready for the Bifrost 3: firmware versions are asked for the way the official app asks, and the
last known bugs and the flaky tests are gone.

- **Firmware versions are asked for one at a time**, 300 ms apart and each after the previous
  answer, as the official app does, without holding up anything else on the link.
- **The signal path knows which USB port the computer is on** on a Gungnir 2 or a Yggdrasil: a DAC
  listening to USB 1 while the computer plays to USB 2 no longer reads `untouched`, and when both
  ports read connected the path says it cannot tell.
- **Switching to a device the daemon finds just then opens it**, instead of landing on the Devices
  screen with a failure that has already passed.
- Under the hood: three tests that failed now and then (a script run while another test forked, the
  squirrel click under load, the demo sweep's lock file) are fixed for good.

## 0.29.4

The web page's tone curve makes 0 dB easy to hit.

- **Dragging a band's point up or down sticks at 0 dB** while the pointer is within 10 px of the
  0 dB line (less on a short phone graph, never more than 1 dB), so a band goes back to doing
  nothing without aiming for one pixel. Outside that zone the point follows the pointer as before.
  The TUI's drag already treats the 0 dB row as 0 dB.
- **The protocol notes say how the official app paces its writes**: no rate limit, one write per
  step of a drag carrying only the value that moved, nothing on release, and nothing on a timer.
  They also record that svolder asks for firmware versions all at once where the app asks one at a
  time, a difference the Bifrost 3 will test.

## 0.29.3

The folded Now playing title, slower and seamless.

- **A title too long for the folded line loops slowly** to the left at 30 px a second, followed by
  a copy of itself after a short gap, so it comes round without a jump or a rewind. It rests two
  seconds at its start each lap, keeps its place when the line is redrawn for the same track, and
  stops when the card opens or the page is hidden.
- **The title is measured again when the web fonts arrive**, so the loop fits the text that shows.

## 0.29.2

The phone's folded Now playing line, motion for everyone, and fresh pictures.

- **The folded Now playing line on a phone keeps play and next.** Play is a round button and next
  a bare one beside it; previous waits for the open card. The fold arrow is a small chevron in the
  bottom-right corner, where the progress bar now stops short of it.
- **A title too long for the line fades at its right edge** instead of ending in "…", and glides to
  its end and back once for each new track and when the page shows again, in under five seconds,
  then rests at its start.
- **The small motions no longer follow the system's "reduce motion" setting**: the knob's glide,
  the meters' fall, the tone curve's easing and flashes, the glow's turn and the link dot's pulse.
  Only the first visit's default effect does (Glow instead of Smoke), and the effects menu's Off
  stills the page.
- **The README's web pictures are new**: they show Now playing, the visualizer and the strip.
- **`SVOLDER_FAKE_TITLE`** names the fake host's track, to see a long title on the fake.

## 0.29.1

A burn-down: what 0.29.0 left for later, the cheaper 0.28.0 known issues, and a switch for the
visualizer.

- **A "Visualizer" switch in the effects menu.** Off, the card is gone, nothing is captured for this
  page, and on a desktop Now playing takes the whole band at a lower height. The page remembers it
  and a reload starts without the card, so nothing moves.
- **A woken page keeps its glow.** The wait for the first reading dims the controls only, and a
  screen with nothing to read is ready at its first event instead of after five seconds.
- **On a phone the volume comes before the visualizer** for the keyboard and a screen reader.
- **A signal path too long for the card ends in a "…" chip** that names the hops left out; the
  verdict stays first.
- **A `?` key list for guests** and for pages with writes off, with the keys they have.
- **The TUI's Spectrum row never goes blank while it joins the daemon's frames.** Its own capture
  runs until the daemon's first frame lands, and a slow daemon has three seconds to answer.
- **A 174 BPM kick reads 174, not 87.**
- **Now playing, fixed.** A player whose pid could not be read is asked again; a position read just
  after a new track is read again half a second later; a player stream playing elsewhere wins over
  one not linked anywhere; a stream into the computer EQ names the sink the EQ feeds.
- **A session bus that goes away, or does not answer, is looked for again every 10 s** by the daemon
  and the TUI, and a Settings switch of Host audio no longer stalls the TUI while it refreshes.

## 0.29.0

The computer's music, drawn live. The daemon listens to what the computer plays, and the web page
and the TUI draw it and move with it.

- **A visualizer card on the web page.** It stands in the band beside Now playing on a desktop (the
  right half) and under the volume on a phone, and draws what the computer plays in three looks:
  lumiBars soft (the first), a spectrogram that drifts by, and lumiBars. A tap on the card, or `v`,
  goes to the next look, a small toggle in the corner adds the graph overlay (off at first: each
  channel's curve and its max hold, the highest level each frequency reached), and the page
  remembers both. The card and Now playing are translucent, so the smoke shows behind them. When the
  computer plays to a sink that is not this page's DAC, the card says which sink it shows; with no
  capture possible it says why, quietly. The card keeps its size either way, so nothing moves when
  it starts.
- **The daemon captures only while someone watches, and only listens.** One capture serves every
  page and TUI; it starts with the first one that looks and stops two seconds after the last, and
  with nobody looking nothing is captured or analysed. The capture is passive, so it does not keep
  the sink awake, and the daemon never sets a volume or a mute while watching.
- **The TUI's Spectrum and Level come from the daemon when joined.** A TUI joined to a daemon draws
  the daemon's analysis instead of recording the computer itself, and with no daemon, or an older
  one, it records as before. A beat makes the Spectrum row and the breathing borders pulse.
- **The smoke and the glow move with the music.** While the visualizer runs, the glow's level
  follows the music's loudness as well as the volume, and a beat sends a puff of smoke from the
  volume dial.
- **A hidden tab lets go of its streams.** A page hidden for two seconds closes its event streams
  and opens them again when it is shown, so a tab in the background costs the daemon nothing, not
  even the route reads. A woken page waits dimmed for its first reading, so a step from a stale
  level cannot be taken.
- **A "Visualizer delay" row** in the Device fold-out shows how far behind the sound the card draws,
  while it runs.
- **Fixed.** A flaky TUI test (a joined press with no player raced the daemon-version toast), and
  the web checks' Firefox runs, which could collide on geckodriver's fixed websocket port.
- **The capture leaves the sound card's rate alone.** `pw-record` is passive and asks for no rate
  of its own, so with several allowed rates the graph follows a 44.1 kHz or 96 kHz player while
  the visualizer watches; `make rate-check` measures it by hand. A known issue was added: on a
  phone the visualizer comes before the volume in the tab order.

## 0.28.0

Now playing on the web page, and the signal path from the player to the DAC. The daemon follows the
computer's music player and the way its sound reaches the DAC, and every front end reads the same
thing.

- **Now playing card.** The web page shows what the computer is playing: the cover, the title, the
  artist, the album and the player's name, ◀◀ ▶/▮▮ ▶▶ buttons that go to the player, and a progress
  bar with the time played and the length that moves on its own. On a desktop it is a band under the
  header, across both columns; on a phone it is one line above the volume that a tap opens into the
  whole card and a second tap folds. It follows the player that is playing, as the TUI does. With no
  player it says so in one quiet line. A guest, and a page with `[web] writes` off, see the card
  without the buttons. Amps' and preamps' pages get it too.
- **The card never moves.** It has the same size with a cover, without one and with no player, and
  a changed path stays inside it, so the volume under your finger does not jump when a track
  changes.
- **The signal path.** One line from the player to the DAC, on a DAC's page and in the TUI: it says
  `untouched` only when every hop is known to leave the signal alone (one rate from the player
  through the computer to what the DAC reads, volumes at 100 %, no mute, no computer EQ, nothing
  else mixing in), and otherwise names what changes it (`resampled 44.1 → 48 kHz`,
  `computer volume 54 %`, `host EQ on`, `2 other sounds mixing`, `DAC reads 44.1 kHz`) or what it
  could not read. When the computer plays to another sink, or the DAC listens to an input the
  computer does not feed, it says so and gives no verdict. It never says "bit-perfect": svolder
  cannot see the bits.
- **The TUI reads the daemon.** A TUI joined to a daemon takes now playing and the path from it and
  presses the transport through it, with a new Path row under Transport in the Host group; with no
  daemon, or a daemon that does not watch the computer, it follows the player itself as before.
  Switching Host audio in Settings takes effect at once.
- **A `[host]` table.** `[host] enabled` (on by default) lets svolderd watch the player and the
  route; `enabled = false` or `SVOLDER_HOST=off` in the daemon's environment turns it off. A daemon
  in a container has no session to watch and says so.
- **Covers are read locally, never from the network.** The picture is the one the player published:
  a file it wrote, or the cover in Spotify's own cache. svolder makes no request to any server for
  a cover, and a track whose picture is not on the disk simply has none.
- **Reading the route is light.** The daemon reads the computer's audio graph only while a page of a
  DAC or a TUI that shows the path is open, at most once in two seconds, and it only reads.
- **`make demo`** runs a fake player and a fake path, so the demo's page shows the card.

## 0.27.3

The mouse wheel on the web page's tone curve keeps adjusting the band it started on even when the
pointer drifts off its point, and a point is easier to catch with the wheel. The page scrolls
again as soon as the wheel stops changing the Q, so a trackpad near the graph never holds it.

## 0.27.2

Every library svolder builds on moves to its newest release, among them the Bluetooth stack
(btleplug 0.13.5 with bluez-async 0.9, which also opens adapters whose Modalias is of the
`bluetooth:` kind), tokio, rustls, hyper and the MCP server's rmcp. Building from source now needs
Rust 1.93 or later, the version svolder is built and tested with.

## 0.27.1

Small fixes after 0.27.0 on the web page's curve. A second drag can start right after the first
one, while its last changes are still on their way, and the device ends where the second drag
left the band. A wheel turn just after a drag keeps the Q it set, a quick wheel burst follows a Q
changed elsewhere, and a refused wheel step puts the device's Q back. A drag sends no repeated
value, and a touch-cancel or a third finger in a pinch ends cleanly. `svolder tui --demo`, broken
since 0.26.0, opens its fake device again, and so does any TUI started while the daemon is still
finding its device. The rest is under the hood: the curve's checks compare more of the page's
numbers with the core's.

## 0.27.0

Drag the Response graph's points on the web page, with a mouse or a finger.

- **Drag a band's point.** Press a band's point and drag it: across sets the frequency, up and down
  the gain, and the device follows the pointer while the curve, the peak line and its clip note
  follow at once, with a label beside the point (`1200 Hz · +1.45 dB · 1.40`). Each direction stays
  still until the pointer has gone a few pixels along it, so a sideways sweep does not change the
  gain. In Flat a drag works too and switches the mode to EQ.
- **Q by wheel, right drag or pinch.** The wheel over a point steps its Q a notch at a time, a right
  drag sideways widens and narrows the bell, and two fingers on the graph pinch it. The page still
  scrolls past the graph: a finger put down away from a point is the page's.
- **Points sit at their own gains.** A band's point is drawn at its own frequency and gain, as in
  the TUI, instead of on the summed curve, and in Flat it can be grabbed.
- The key list (`?`) says how the curve is dragged. The tone part waits to be redrawn while a point
  is held, so the graph does not jump back under the pointer.

## 0.26.1

Small fixes after 0.26.0. `svolder identify` on a device the daemon has not found yet says when it
tries again, even with several devices configured. The web page's Device section, opened after you
were signed out, now takes you to the sign-in page, and it sends its table less often while
readings come and go. The rest is under the hood: the end-to-end test copes with a slow second
identify, and the safety-test list says what it guards.

## 0.26.0

Show everything: every fact svolder knows about a device is now on screen in the TUI and on the web
page, so one look (or one screenshot for a bug report) says which device, which firmware, which
input, how good the radio link is and what is wrong.

- **Identify.** Ask a device to show itself on its front panel: `svolder identify`, the `b` key in
  the TUI, the Identify button in the web page's Device section, the MCP tool `identify` and an
  Identify button in Home Assistant. At most one request every two seconds goes out. What the unit
  does is not yet seen on real hardware; a Mesh Card offers no identify, as in the official app.
- **Firmware and DSP versions.** After each connect svolder asks Yggdrasil, Gungnir 2, Bifrost 3,
  Mimir, Eitr 2 and Mesh Card for their firmware versions (main, and DSP where there is one), as the
  official app does, and shows them everywhere.
- **Signal strength.** The strength of the advertisement svolder heard just before connecting, with
  how long ago it was heard: in the TUI, on the web page, in `svolder devices` and in Home
  Assistant. While connected, BlueZ gives no live value to an ordinary user, so none is shown.
- **The web page** gains a Device section under each device, closed until you open it: product,
  unit, firmware, DSP, profile, link, signal, and a table of every reading with when it was heard.
  Every active fault now shows in the card at the top (titled "Fault" or "N faults"), for every
  kind of device, and only there.
- **The TUI's Status pane** gains a Device row (product, product id, profile, signal), writes flags
  in both states ("USB not connected"), and names a refused passphrase or stale readings; the
  Devices list gains a signal column.
- **`svolder devices`** ends each line with the signal and uses the same link words as the TUI and
  the web.
- **Home Assistant** gains Signal, Firmware and DSP firmware diagnostic sensors.
- **Custom profiles:** a control keyed `link`, `signal`, `firmware`, `dsp_firmware`, `identify`,
  `eq_mode`, `eq_preset` or `eq_loudness` now fails to load, since those names belong to the MQTT
  bridge's own entities; rename it. A control on the passphrase function or the firmware-update
  family also fails to load.
- Fixed: after the passphrase in the configuration was changed while the device was answering a
  refresh, svolder could take the old code's answers as proof the new code worked.

## 0.25.6

The web UI is no longer marked experimental, and the README is now a short front page: what svolder
is, how to install it, the commands to start with, and where the details live.

- Signing in from a phone: a camera that previews the QR code can use its link up before the
  phone's browser opens it. The TUI's Web UI box now notices and shows a fresh link and QR code, so
  a second scan works.
- The web page's volume number stays centred when it is negative: the minus hangs to its left.
- The "Climbing to …" popup stays six seconds instead of vanishing the moment the volume gets
  there; stopping the climb yourself still takes it away at once.
- The web page's section titles are set in Geist (SIL Open Font License, listed in
  THIRD-PARTY-LICENSES).

## 0.25.5

Small fixes. When `svolder capture` stops a daemon you started by hand and the daemon is slow to
let go, the message now says so once instead of twice. The rest is under the hood: tighter checks
on the release workflow and the test suite, and clearer documentation.

## 0.25.4

The first release with downloads: ready-made packages for PCs (x86_64) and for 64-bit ARM boards
such as the Raspberry Pi 3, 4 and 5 (arm64), so svolder installs the way your system expects
instead of being built from source. The binaries are static, so one build runs on any Linux of its
architecture, and every file is built reproducibly: anyone can rebuild them byte for byte.

**`svolder capture`** (new since the last source-only version) records everything svolder can
learn about your device into one text file you can send us: what it advertises, every reading,
and every message both ways while you press its buttons. Your device's unit id, its passphrase,
your home directory and Bluetooth addresses are scrubbed out by default; read the file before you
send it. It is the most useful thing an owner of a Tyr, a Loki or a Bifrost 3 can do for svolder,
since we cannot test those ourselves.
