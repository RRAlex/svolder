<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The daemon

Running `svolderd`: the socket it serves, release builds and packages, containers, logging, the
configuration file both sides read, the IPC protocol and the `daemon` verbs. Back to the
[usage index](README.md).

## Daemon and config

`svolderd` reads `config.toml`, opens one session to each configured device, and keeps every session
connected — scanning again and reconnecting whenever a device drops off — while serving requests
over a Unix socket. `svolder` uses a running daemon automatically (see
["Transport and Defaults"](bluetooth.md#transport-and-defaults)); this section covers the daemon
itself, the socket it serves, the configuration file both sides read, and the wire protocol between
them.

A device that does not come back is not scanned for over and over at the same rate: the waits
between reconnect attempts are 2, 5, 15 and then 30 seconds, the last step repeating for as long
as the device stays away. One attempt is itself cut off after 45 seconds (listen, connect,
service discovery and the first refresh; releasing its link can take up to about 13 seconds more,
so about a minute in the worst case): a step that never answers fails it with a reason in the log,
and the next wait follows the schedule. Every attempt scans the adapter, so a daemon configured
for a DAC that is switched off would otherwise keep that adapter discovering for as long as it
is off — power spent on a device that is not there, and a scan every other user of the radio on
the host has to share the air with. Two things end a wait where it stands and start the schedule
over, so acting on a device you have just switched back on never waits out a long step.

One is a command that needs the link — a `set`, an `eq` write, a passphrase change — which is still
refused while the link is down, as it always is. The other is a `reload` (or a SIGHUP, an `add`, a
`remove`) that changes this device's entry: a corrected passphrase, another `profile`, new input
labels, since the next attempt is the one that uses them.

Everything else leaves the schedule alone: reads are answered from the last state the device
confirmed; a `raw` is checked against the protected function ids first — that refusal stands whether
the device is there or not — and then turned away for want of a link without ending the wait; and a
reload that only re-reads the profiles, or that hands this device the entry it already has, says
nothing about the device being back. The wait is shown, in the same words (`next attempt in 15 s`),
wherever the device's link is: its `svolder daemon status` line (below), its `svolder devices` line,
its line in the summary bare `svolder` prints off a terminal, and its row on the TUI's Devices
screen — and it is in the daemon's own log (`connect failed: ...; retrying in 15 s`).

### Running the daemon

Most of the time nothing installs the daemon on purpose: `--transport auto`'s launcher chain (see
["Transport and Defaults"](bluetooth.md#transport-and-defaults)) starts one of its own the first
time a command needs it. That daemon is detached into a session of its own and keeps running once
the command that started it has ended — it holds the device's link, which is the point of it —
logging to `$XDG_STATE_HOME/svolder/svolderd.log` (`svolder daemon logs` reads it, timestamps
included) until the login ends or `svolder daemon stop` ends it. Only a daemon that never answered
within its two seconds is killed by the chain itself.

A session with a home, a runtime directory or a `SVOLDER_SOCKET` of its own resolves another path
and starts a daemon of its own there, which goes on holding that device's link after the session
that started it is gone — and nothing a later command asks reaches it, since a command only ever
asks the one socket it resolves. `svolder daemon status` and `svolder doctor` name every other
listening svolderd of this user's, whoever started it — the launcher for another session, a
hand-started one with a runtime directory of its own, or the Docker Compose one on a network of its
own — each with the socket it listens on and the command that stops that one, so one nothing here
reaches is still one you can see and end.

Where a daemon listens is read off the listening socket it holds (its descriptors in `/proc`,
matched against the socket table of its own network namespace, `/proc/<pid>/net/unix`), whatever it
was told; its `--socket` argument or `SVOLDER_SOCKET` stands in only when this user cannot read
those two. A daemon that has not bound its socket yet is named once it has. A daemon in a container
— a mount namespace of its own — is named through its root, as
`/proc/<pid>/root/run/svolder/svolder.sock`, so the path printed, and the stop command beside it,
work from the host.

To have the daemon hold every device's session across the whole login instead — so a command never
pays the seconds a fresh connect costs — install the binaries and enable the systemd user units:

```
make install            # binaries to ~/.local/bin
make install-service    # svolder daemon install, then the linger hint
```

`make install` then restarts a daemon that is running (the user units' service, or the autospawned
one) through `svolder daemon restart --if-running`, so the new build takes over; it starts none,
does nothing as root (each user runs `svolder daemon restart` for their own), and
`make install RESTART=0` skips it. A restart that fails (a `config.toml` that does not parse, say)
prints `make: the daemon was not restarted; run 'svolder daemon restart'` and the install still
succeeds. A TUI connected to that daemon reopens its device by itself once the new daemon answers
(see ["tui"](tui.md#tui)), so the restart costs it a toast and not a key press.

`sudo make install PREFIX=/usr/local` installs system-wide instead, with the units in
`/etc/systemd/user`; `UNITDIR` and `SYSTEMCTL` are overridable for either target. The units go to
the user directory when `PREFIX` and `$(HOME)/.local` name the same path once resolved (a trailing
slash on `HOME`, a `.` or `..`, or a symlink, which is followed where the path exists); for a
`~/.local` that does not exist yet behind a symlinked home, pass `UNITDIR`.

`make install-service` is a wrapper around `svolder daemon install` (see "daemon" below), which
writes `svolderd.socket` and `svolderd.service` into `~/.config/systemd/user` from the resolved
socket path and this binary's own directory, then enables both; `--on-demand` (from
`svolder daemon install --on-demand`, not exposed by the Makefile) enables only the socket, so the
daemon starts on the first command that needs it rather than at login. `make uninstall` runs
`svolder daemon uninstall` before removing the installed binaries. A user unit stops at logout
unless linger is enabled; `daemon install` prints the exact command:

```
loginctl enable-linger $USER
```

Run as root (`sudo make install-service PREFIX=/usr/local`), `daemon install` writes the two unit
files into `/etc/systemd/user`, the unit directory for every user, and runs no `systemctl`, since
root has no user session to enable them in; each user then runs the command it prints,
`systemctl --user enable --now svolderd.socket svolderd.service`. `daemon uninstall` as root
likewise only removes the files and names the
`systemctl --user disable --now svolderd.socket svolderd.service` each user runs.

On NixOS (detected from `/etc/NIXOS`), `daemon install` writes nothing and instead prints the
home-manager block to put in your own configuration; in a container it says the daemon is the
container's own process (see "Running in a container" below) and installs nothing either way.

### The unit's stop timeout

`svolderd.service` sets `TimeoutStopSec=60`. A stop that goes as slowly as it can (a job, the MQTT
bridge, a device that does not answer, a write, the transport's settle and the end of the runtime,
each at its bound) takes 51 seconds, and the unit gives it 60 so it does not depend on the service
manager's own default: systemd's is 90 seconds (checked on systemd 259 with `systemctl show -p
DefaultTimeoutStopUSec`, and in `systemd-system.conf(5)`), and a manager configured with a lower
one would send SIGKILL in the middle of a worst-case stop and leave a device's session open.

### The unit's sandbox

`svolderd.service` carries these restrictions, in the order the unit lists them:

- `NoNewPrivileges`: the daemon and what it starts cannot gain privileges.
- `RestrictSUIDSGID`: no set-id files can be created.
- `LockPersonality`: the execution domain cannot be changed.
- `RestrictRealtime`: no real-time scheduling.
- `RestrictNamespaces`: no new namespaces.
- `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK`: Unix, IPv4, IPv6 and netlink
  sockets only. Bluetooth goes through BlueZ over D-Bus and needs none of its own.
- `SystemCallArchitectures=native`.

The session bus, the system bus and PipeWire stay reachable, and so do the programs the daemon
runs (`systemctl --user`, `wpctl`, `pw-cli`, `pipewire`). On a host with
`kernel.apparmor_restrict_unprivileged_userns=1` (seen on Ubuntu 26.04) these were seen to hold:
`NoNewPrivs: 1`, `Seccomp: 2`, packet and Bluetooth sockets refused, the daemon running.

#### A stricter filesystem

The unit does not restrict the file system. `svolderd-filesystem.conf`, a drop-in for the
service, does; neither `svolder daemon install` nor the packages install it. It adds:

- `ProtectSystem=strict` and `ProtectHome=read-only`: the file system is read-only to the daemon,
  except the paths below.
- `ProtectKernelTunables` and `ProtectControlGroups`: `/proc/sys`, `/sys` and the control groups
  are read-only.
- `ReadWritePaths=-%E %t`: the configuration directory (`$XDG_CONFIG_HOME`: `config.toml`, the
  computer EQ's `host-eq.conf`, its user unit and autostart entry) and the runtime directory (the
  socket, its lock, the computer EQ's pid file).
- `StateDirectory=svolder` (mode 0700): systemd creates `$XDG_STATE_HOME/svolder` (tokens,
  `host-eq.toml`) before the daemon starts and leaves it writable.
- `CacheDirectory=svolder` (mode 0700): the same for `$XDG_CACHE_HOME/svolder`, where the daemon
  keeps AutoEq's cached index; without it the read-only file system would stop the
  daemon caching the index it fetches.

To turn it on, make the drop-in with `systemctl --user edit --drop-in=filesystem svolderd.service`,
paste this into the editor, save, and restart (`edit` reloads the manager itself); this works for
every install, packages included:

<!-- drop-in: compared with systemd/svolderd-filesystem.conf by make unit-check -->
```
[Service]
ProtectSystem=strict
ProtectHome=read-only
ProtectKernelTunables=yes
ProtectControlGroups=yes
ReadWritePaths=-%E %t
StateDirectory=svolder
StateDirectoryMode=0700
CacheDirectory=svolder
CacheDirectoryMode=0700
```

```
systemctl --user restart svolderd.service
```

From a checkout, `mkdir -p ~/.config/systemd/user/svolderd.service.d` and copying
`systemd/svolderd-filesystem.conf` there as `filesystem.conf` does the same, then
`systemctl --user daemon-reload` before the restart.

To turn it off, delete `~/.config/systemd/user/svolderd.service.d/filesystem.conf`, then run
`systemctl --user daemon-reload` and `systemctl --user restart svolderd.service`. (`systemctl
--user revert svolderd.service` also works but removes every drop-in of the service, not only
this one.)

With a `--config` kept elsewhere, or a `--socket` in a directory outside the runtime directory,
add that path to `ReadWritePaths=` in the drop-in, for example `ReadWritePaths=-%E %t /srv/svolder`
(a later `ReadWritePaths=` line adds to the earlier ones).

The conditions:

- systemd 254 or later. From 254 a user manager's `StateDirectory=` and `%S` follow
  `$XDG_STATE_HOME`; before it they name the configuration home.
- A host that lets a user manager create user namespaces. On a host with
  `kernel.apparmor_restrict_unprivileged_userns=1` the settings are accepted and do nothing: no
  warning, and the whole file system stays writable.
- A session whose `XDG_CONFIG_HOME` and `XDG_STATE_HOME` reach the user manager
  (`systemctl --user import-environment`, `environment.d`); otherwise the paths are the defaults
  under the home.
- A configuration directory that exists: `-%E` skips one that does not, so on an account without
  `~/.config` the daemon cannot write its config until `mkdir -p ~/.config` has made it. The
  drop-in does not use `ConfigurationDirectory=` to create it, since systemd would change the mode
  of an existing `~/.config/svolder`.
- It has not been proven end to end.

`ProtectKernelModules=yes` is in neither file: on the host named above a service carrying it
failed to start with status 218/CAPABILITIES.

### AutoEq

`svolderd` is the process that reaches AutoEq's repository for the web page's EQ library: it loads
the index once at a time, caches it in `$XDG_CACHE_HOME/svolder/autoeq/INDEX.md` (refetched after
`[autoeq] refresh_weeks`), downloads each correction within `eq.md`'s limits (up to four AutoEq jobs
at once), fits it to a device's tone control in a worker thread, and saves an import through the
configuration writer. `SVOLDER_AUTOEQ_URL` names another base, read once at start. It verifies TLS
against the system's certificates: the packages depend on `ca-certificates` and the container image
installs it. Under the opt-in filesystem drop-in the cache directory is writable
(`CacheDirectory=svolder`); in a container the image puts the cache in the `/config` volume
(`XDG_CACHE_HOME=/config/cache`).

### Release builds and packages

The release artifacts are built in a container (`packaging/Dockerfile.build`), so a host needs
Docker, make and a git checkout for them, and a clean tree: an artifact carries the last commit's
version, time and source (`ALLOW_DIRTY=1` builds from uncommitted changes anyway, to try the
packaging). Each target writes into `target/dist`; the release is built for two architectures,
x86_64 and arm64 (`aarch64`), the arm64 one in an arm64 container under QEMU user-mode emulation on
an x86_64 host (about ten minutes for the three binaries), which needs
`sudo apt install qemu-user qemu-user-binfmt` there. `make static` and `make docker` build x86_64
unless told otherwise: on an arm64 host, `make static ARCH=aarch64`; `make docker` is x86_64 only:

```
make static                 # svolder, svolderd, svolder-sim: x86_64, static, libdbus built in
make static ARCH=aarch64    # the same for arm64, into target/dist/aarch64
make 3rdpartylicenses       # THIRD-PARTY-LICENSES: every crate's licence, then libdbus's, musl's,
                            # IBM Plex's, Geist's, Crimson Pro's, Caveat's, Russo One's, Mr Dafoe's and htmx's
make packages               # both architectures, then PKGBUILD, SHA256SUMS and RELEASE-NOTES.md
make packages ARCH=x86_64   # one architecture's packages (ARCH=aarch64 the other's); no combining
make release-files          # PKGBUILD, SHA256SUMS and RELEASE-NOTES.md from both architectures
make docker                 # the container image, from the same static binaries
make repro-check            # make packages twice; fails if any artifact differs
```

The release is built the same way on GitHub: pushing a `v*` tag starts
`.github/workflows/release.yml` (by hand, `gh workflow run release.yml --ref vX -f version=X` runs it
for a tag that has no release yet). It builds each architecture on a native runner
(`make packages ARCH=...`) without write access and runs `make check` on an arm64 runner, unless
`check.yml` (which runs on every push to main) already passed on that very commit, which the run
asks the API first; a by-hand run always tests. The images and the release wait for that check. So
push main, wait for `check.yml` to pass, then push the tag. The release then pushes the
multi-architecture image, and a last job combines the files (`make release-files`), attaches the
compose file (`packaging/release-compose.sh`: `docker-compose.yml` with its `image:` line and the
comment above it changed for `ghcr.io/rralex/svolder:VERSION`), checks the draft against
`SHA256SUMS` and publishes it; a failed run leaves no release. The image is tagged `VERSION` (both
architectures) and, as each architecture is built on its own runner, `VERSION-amd64` and
`VERSION-arm64`; there is no `latest`, so pin a version. `docker-compose.yml` is the one asset
outside `SHA256SUMS`. `make workflow-check` (part of `make check`) allows the workflow's two
triggers and nothing else (`workflow_dispatch`, and `push` of `v*` tags with no branch or path
filter); it also fails on a job with a write scope beyond its own (`contents` for the release job,
`packages` for the image jobs), a step that runs `make` with a token in its environment, a `run:`
that interpolates untrusted input, a persisted clone token or an action that is not pinned to a
commit SHA.

The binaries are static, so one build runs on any x86_64 Linux: they reach the system bus through
the libdbus built into them, and PipeWire through its own tools (`wpctl`, `pw-record`) when those
are installed. musl has no name-service switch, so a `.local` MQTT host resolves only where the
system's DNS resolver answers mDNS (systemd-resolved with `MulticastDNS=yes`); otherwise name the
broker by address or DNS name.

The .deb and the .rpm carry the three binaries in `/usr/bin`, the user units in
`/usr/lib/systemd/user` (filled in for `/usr/bin` and the session's runtime directory, as a root
`svolder daemon install` writes them), the docs, `LICENSE`, `AUTHORS` and `THIRD-PARTY-LICENSES`,
and depend on `bluez` and `dbus`. Installed with `sudo apt install ./svolder_<version>_amd64.deb`
(`_arm64.deb` on arm64, the Raspberry Pi OS 64-bit package) or
`sudo dnf install ./svolder-<version>-1.x86_64.rpm` (there is no arm64 .rpm), a package enables
nothing: each user turns the daemon on with
`systemctl --user enable --now svolderd.socket svolderd.service`.

The binary tarballs, `svolder-<version>-x86_64-linux.tar.gz` and
`svolder-<version>-aarch64-linux.tar.gz`, hold the binaries, the docs, the licences and the unit
templates; with the binaries on `PATH`, `svolder daemon install` writes and enables units for
wherever they are. `PKGBUILD` is a `svolder-bin` package for both architectures around those
tarballs, which it downloads from the release page (`releases/download/v<version>/`), their
checksums filled in; the AUR package needs `.SRCINFO` too, from `makepkg --printsrcinfo > .SRCINFO`.

The release assets, all in `target/dist`:

```
svolder_<version>_amd64.deb      svolder-<version>-x86_64-linux.tar.gz    THIRD-PARTY-LICENSES
svolder_<version>_arm64.deb      svolder-<version>-aarch64-linux.tar.gz   PKGBUILD
svolder-<version>-1.x86_64.rpm   svolder-<version>-source.tar.gz          SHA256SUMS
```

`SHA256SUMS` lists the other eight files and is written last; `sha256sum -c SHA256SUMS` in
`target/dist` checks them. `RELEASE-NOTES.md` is the release page's text and not an asset: a
Beta paragraph (what has and has not been tested, and to start with the volume low), the
version's section of `CHANGELOG.md` followed by each older section down to the first version with a tag, under `### Also in this release` (a version without one, or one still the placeholder, stops
`make release-files`), then the install lines, a line per file and the check
(`packaging/release-notes.md.in`, filled in by `packaging/release-files.sh` with `packaging/release-notes.sh`, which put the sections at the template's `@CHANGELOG@`
line). The bare arm64
binaries are in `target/dist/aarch64`, which is not an asset either.

`svolder-<version>-source.tar.gz` is the commit with every crate the build uses vendored beside it
(libdbus's source inside `libdbus-sys`); it builds with no network (`cargo build --offline`, or the
`static` target's own `cargo auditable build` line for the static binaries). The binaries carry GPL
code, so whoever publishes them publishes this tarball beside them, as the GPL asks, and
THIRD-PARTY-LICENSES and the .deb's copyright point to it. `flake.nix` builds from the checkout
with nixpkgs' toolchain instead (`nix build`, `nix run`), with the units in `lib/systemd/user` for a
NixOS configuration's `systemd.packages`.

The builds are reproducible on a best-effort basis: the container's base image is pinned by digest,
its build tools by an Ubuntu archive snapshot (`SNAPSHOT`), Rust (`RUST_VERSION`), cargo-about and
cargo-auditable by version, and the crates by `Cargo.lock`; every file and package carries the last
commit's time (`SOURCE_DATE_EPOCH`). The release binaries are built with `cargo auditable build`,
which embeds their dependency list (derived from `Cargo.lock`) for `cargo audit bin`. A plain
`cargo build --offline` from the source tarball gives working binaries without that list, so they
are not byte-identical to the released ones. `make repro-check` builds everything twice, both architectures, and compares
the sha256 of each artifact, the arm64 ones included. With the arm64 build image and the crates
cached it takes about 13 minutes here, most of it the emulated arm64 build; from a cold start (the
image and the crates fetched, a first emulated build) add roughly 25 minutes. The container image
is outside it: `useradd` records the day it ran.

Run it directly, for testing or without installing it:

```
Keep sessions to configured Schiit Forkbeard devices and serve them over a socket

Usage: svolderd [OPTIONS]

Options:
      --config <CONFIG>        Configuration file to read [default: /home/user/.config/svolder/config.toml]
      --socket <SOCKET>        Socket to listen on. Defaults to the runtime directory's `svolder.sock`
      --transport <TRANSPORT>  Transport backend: `ble` (a real BlueZ adapter) or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: ble]
      --adapter <ADAPTER>      Bluetooth adapter to use with --transport ble (e.g. hci0). Defaults to the first [env: SVOLDER_ADAPTER=]
  -v, --verbose                Log every command and status (the same as RUST_LOG=debug)
  -h, --help                   Print help
  -V, --version                Print version
```

An adapter that is not ready when the daemon starts — Bluetooth still coming up, the dongle not
plugged in yet — makes `svolderd` exit 3, and the unit restarts it every 5 seconds until the adapter
appears (`After=bluetooth.target` does nothing in a user unit, and the unit sets
`StartLimitIntervalSec=0` so systemd never gives up on it). Nothing in `svolderd` retries the
adapter itself yet. A unit that has already been marked `failed` — by an older install, or by a
`systemctl --user stop` during a restart — is cleared with:

```
systemctl --user reset-failed svolderd
```

`svolderd` refuses to start when another instance already answers on the resolved socket (exit 3) —
this makes it safe to run `svolder daemon install` twice, or `svolder daemon restart`, or to start
it by hand while the systemd unit is also enabled. `svolderd.socket` hands `svolderd` a listening
socket over `LISTEN_FDS` (accepted when `LISTEN_PID` matches this process or is unset) rather than
the daemon binding its own, which is what lets systemd start the daemon lazily on the first
connection when only the socket unit is enabled; without a handed-over socket `svolderd` binds the
resolved path itself, as it always did.

SIGINT and SIGTERM close every device session before the process exits: a session with a connect in
flight can take up to 24 seconds (three `close_grace` periods: the attempt standing down, the
release it leaves behind, and the close's own wait) and the daemon waits up to 30 seconds for each,
all at once, then 8 more for the transport. Both SIGHUP and the `reload` IPC request (see "IPC
protocol" below, and the `daemon` verbs' `restart`) re-read `config.toml` and the profile registry —
the profile directory is the config file's own directory plus `profiles/`, which the default config
path makes `~/.config/svolder/profiles`, the same directory the CLI reads — and reconcile the
running devices against what the file now says: an added device gets a session, a removed one's
session ends, and one still configured keeps its connection, without a restart.

The MQTT client follows the same reload: the same `[mqtt]` section keeps its connection and
publishes the new device set, a changed one restarts the MQTT client, and a removed one stops it
(see ["MQTT and Home Assistant"](mqtt.md#mqtt-and-home-assistant)). `svolderd` runs the Bluetooth
service, adapter and config checks from ["doctor"](cli.md#doctor) against itself at startup and logs
their lines, so a daemon that starts unhealthy says why in its own log before anything asks it a
question.

`svolderd` refuses the firmware-update family and function 250 through `raw`, exactly as the CLI
does (see ["raw"](cli.md#raw)).

### Several front ends on one daemon

Every front end that asks the same socket shares the one session the daemon holds: two TUIs side by
side show the same device, and a change made in one shows in the other. To try it without a device,
give a daemon a configuration of its own naming the fake device, and start it on the fake transport
with that configuration's directory as its `XDG_CONFIG_HOME` (the two `SVOLDER_FAKE_*` variables
make it the fake amp, whose meters move):

```
mkdir -p /tmp/sv-home/svolder
printf '[[device]]\nname = "fake"\nudid = "0102030405060708"\n' > /tmp/sv-home/svolder/config.toml
XDG_CONFIG_HOME=/tmp/sv-home SVOLDER_FAKE_PROFILE=fake-amp SVOLDER_FAKE_SIGNAL=1 \
  svolderd --transport fake --socket /tmp/sv.sock
```

Then, in each of two other terminals, with neither `SVOLDER_TRANSPORT` nor any `SVOLDER_FAKE_*`
variable in their environment:

```
svolder --transport daemon --socket /tmp/sv.sock tui
```

and end the daemon when you are done:

```
svolder --socket /tmp/sv.sock daemon stop
```

A front end with `SVOLDER_TRANSPORT=fake` in its environment and no `--transport` on its command
line opens a private fake session of its own instead of asking the daemon, so what it shows is not
what the daemon holds; `--transport daemon` on the command line wins over the variable.

### Logging

`svolderd` logs lifecycle events to stderr at `info` by default. `RUST_LOG` sets the level —
`error`, `warn`, `info`, `debug`, or `trace`, either bare or as `svolderd=debug` — and `-v` forces
at least `debug`. At `debug`, the daemon logs every decoded command it sends and every status it
receives, one line each naming the device, the function id, the wire type, and the value; the frame
bytes themselves are not logged, because they are not visible outside the session that reads them.

Each line starts with the local date and time, `2026-10-04 18:04:26 svolderd: listening on ...`, and
the transport's own lines (`svolder-ble: ...`) carry the same stamp. A `warning:` or `error:`
follows `svolderd:` for those two levels. When the daemon's stderr is the systemd journal
(`JOURNAL_STREAM` names it), the line has neither the stamp nor `svolderd: `, since the journal
stamps each line and tags it with the unit and pid (`... myhost svolderd[812]: error: ...`).
`svolder daemon status` names where the log goes, and `svolder daemon logs` reads it.

### Thread names

`top -H -p <pid>` (or `ps -L`) shows which thread uses the CPU. The runtime's workers are
`svolderd-tokio` in the daemon and `svolder-tokio` in `svolder` (the TUI and the verbs). The rest
are named for their job: `svolder-capture` reads the frames of the computer's sound capture,
`svolder-mpris-connect` opens the connection to the session bus for the player, `svolder-eq-reap`
waits for the computer EQ's chain process to end, `svolder-reap` waits for a short helper to end
(the clipboard tool, the start-up sound, the browser `svolder web` opens) so none lingers as a
zombie, and `svolder-demo-watch` looks at the daemon a `--demo` run started.

### Configuration file

`~/.config/svolder/config.toml` (or `$XDG_CONFIG_HOME/svolder/config.toml`; with `$HOME` unset too,
`~` is the home directory the account database lists for the user, so the path is still absolute):

```toml
schema         = 1
adapter        = "hci0"                 # optional, default first adapter; blank means unset
default_device = "bifrost"              # optional, used when a command names no device; blank means unset
# socket = "/run/user/1000/svolder.sock"   # optional override; see the resolution order

[daemon]                                # optional
autospawn = true                        # start a daemon of svolder's own when --transport auto finds none

[mqtt]                                  # optional; see "MQTT and Home Assistant" in mqtt.md
enabled = true                          # off, and no connection at all, unless this and host are set
host    = "192.168.1.10"                # the broker

[mcp]                                   # optional; see "mcp" in mcp.md
enabled = true                          # off, and no listener at all, unless this is set
listen  = "127.0.0.1:8737"              # loopback by default; a private LAN address is allowed with auth = "token"

[web]                                   # optional; see "Web UI" and "web" in web-ui.md
enabled = true                          # off, and no listener at all, unless this is set
listen  = "127.0.0.1:8738"              # loopback by default; "0.0.0.0:8738" reaches a phone, over plain HTTP

[[device]]
name       = "bifrost"
udid       = "1A2B3C4D5E6F7A8B"          # 8-byte unit id from the advertisement, hex
profile    = "bifrost3"                 # optional, else chosen by product id + firmware
passphrase = "auto"                     # "auto" (recover from advertisement) | "none" | four digits

[device.inputs]                         # optional per-device input label overrides
3 = "TV optical"

[[device]]
name = "gungnir"
udid = "0102030405060708"

[autoeq]                                # optional; see "eq search" in eq.md
refresh_weeks = 13                      # weeks before the cached index is fetched again, 1 to 99, or "never"

[host]                                  # optional
enabled = true                          # the daemon reads the music player and the DAC's route,
                                        # and analyses what plays while a page or a TUI looks

[tui]                                   # optional, all defaults shown
intro      = true                       # play the wordmark while the device opens
intro_effect = "random"                 # random | fade | particles, how the wordmark comes in
intro_sound = false                     # a short sound with the wordmark intro (pw-play)
transition = "trace"                    # none | trace | noise | melt, from one screen to the next
palette    = "amber"                    # amber | ice | red | neon
# accent   = "blue"                     # overrides every family's accent; blank means unset
colors     = "auto"                     # auto | 16 | 256 | true
monochrome = false
theme_background = true                 # each palette paints its own greys under the screen
mouse      = true
title_meter = true                      # one bar per channel in the terminal's title
confirm_quit = true                     # q asks before quitting; Ctrl-C quits at once
host_audio = true                       # what the host plays, its volume and its transport
visualizer = true                       # a spectrum of what the host plays
graphs     = true                       # draw the graphs: Telemetry, the level history, the connection strip
graph_span = "5m"                       # 1m | 2m | 3m | 5m | 10m | 15m | 30m | 60m, the Telemetry and level graphs
link_span  = "12h"                      # 1h | 3h | 6h | 12h | 24h, the connection strip
effects    = true                       # true | false | a table naming each switch, see below

[tui.accents]                           # optional, overrides one family's accent at a time
# dac    = "#e0a030"                    # an ANSI colour name (its bright- form too), or "#rrggbb"; blank means unset
# amp    = "#b86a2a"
# preamp = "#f6cc7a"

[tui.spectrum]                          # optional, a palette's own cool-to-hot stops
# stops = ["#5a1a08", "#c04a10", "#e0a030", "#f6dca0"]
```

`schema` must be `1`. Every field but `name` and `udid` is optional; a bare `[[device]]` with only
those two uses `auto` passphrase recovery and the profile its product id and firmware select.
`default_device` should name one of the configured devices: a file whose default names none still
loads, and `svolder` prints `svolder: default_device "NAME" names no configured device` to stderr
(`svolderd` logs the same words), while a device verb that would use that default with no `-d` exits
1 with the same words rather than reaching for another device — `svolder add` of the named device,
or a `default_device` that names an existing one, ends it; a blank `default_device`, `adapter`,
`accent`, or `accents.*` value parses as though the key were absent, rather than as a config error,
so clearing one from the Devices editor or the Settings box (see ["tui"](tui.md#tui)) is a matter of
writing an empty string.

`svolder config path`, `config show`, and `add` (below) inspect and extend this file without opening
a Bluetooth transport; `svolder config template` prints this whole file with every key commented out
at the value svolder uses while it is absent, which is what `setup`, `add`, a `doctor` fix, and a
first Settings save all start a new file from, and each writer — the `[[device]]` array's, the
`[tui]` table's, `adapter`, and `default_device` — replaces a key's commented line in place when it
sets one, appends when the file opens no such table yet, and preserves everything it does not touch,
comments included; clearing a key (writing it blank, or the Devices editor's Default row back off)
puts the commented default line back rather than leaving an empty one on file.

`[daemon]` decides how a command that finds no daemon listening gets one: `autospawn`, on by
default, is what lets `--transport auto`'s launcher chain (see
["Transport and Defaults"](bluetooth.md#transport-and-defaults)) start `svolderd` itself rather than
falling straight through to a direct session; `autospawn = false` turns that step off, so `auto`
either finds an already-running daemon or opens a direct session, never starting one of its own.

A direct session (`--transport ble`, or `auto` with no daemon running) honours the same file: `-d`
matches a configured `name` (resolving to its `udid`); with no `-d`, `default_device` or the single
configured device is tried before falling back to the single device in range; and a configured
`profile`, `passphrase`, and `[device.inputs]` override what the advertisement alone would select.

A device's `passphrase` is checked against the code its advertisement carries, without connecting,
wherever svolder holds an advertisement for it: at each connect attempt of the daemon and of a
direct session, and when a reload changes the entry of a device the daemon holds one for. `auto` is
never questioned. `none` while the advertisement carries a code, four digits that differ from it,
and four digits while the advertisement carries none each earn a note, and an advertisement that
does not decode as a code earns none. The check is best effort and changes nothing svolder sends: an
advertisement can be older than the device's code (BlueZ may hand back one from its cache), so the
words say that the passphrase does not match the code the device advertises, never that it is wrong.

The note goes away when the entry is corrected or set to `auto`, when a later advertisement matches,
and when the device shows the code works by answering one of svolder's own frames without refusing
it: the refresh a connect ends with, when every function the profile names answered it, or the echo
of a `set` that waited for one. A status the device sends on its own proves nothing, since it sends
those whatever the code. A device that never completes its connect burst (see "known-issues": a
device that does not report every function its profile names) never proves the code that way, so a
note from an old advertisement can last until a reconnect or an entry change.

After `svolder passphrase set`, the advertisement `svolder` holds is updated to the new code, which
the device advertises next, so `auto` and the note agree with what the device now holds. The note is
shown as the Status pane's `Passphrase` line (see ["tui"](tui.md#tui)), as one line on stderr naming
the device and the fix (`` svolder: gungnir: the passphrase does not match the code the device
advertises; set `passphrase` in the config to the right code, or to `auto` ``) before a device
verb's own output, in the daemon's log as a warning when the verdict changes (not at every attempt),
and in `devices --json` as `passphrase_note` (`differs`, `none_configured` or `no_code_advertised`,
absent for no note). It never carries a digit of any code: the code the advertisement carries stays
on the daemon's side of the socket.

`[tui]` is read only by `svolder tui` (see ["tui"](tui.md#tui), and
["Palette and the wordmark"](tui.md#palette-and-the-wordmark) for what these draw). `intro` plays
the wordmark while the device opens; skipped with any key, or removed outright with `intro = false`.
`intro_sound`, off by default, plays a short sound with the wordmark through PipeWire's `pw-play`
(see ["Palette and the wordmark"](tui.md#palette-and-the-wordmark)). `intro_effect` says how the
wordmark comes in — `random` (the default), `fade` or `particles` (see
["Palette and the wordmark"](tui.md#palette-and-the-wordmark)) — and any other word is a config
error naming the three.

`palette` names one of the four shipped presets — `amber` (the default), `ice`, `red`, `neon` — each
of which gives every device family (DAC, amp, preamp) its own accent colour rather than one fixed
colour for everything: with no `accent` and no matching `[tui.accents]` entry, a device's accent is
the palette's own colour for its family — the amber preset's DAC colour on a DAC, and so on.

`accent` overrides every family's accent at once, and is what the Settings box's three accent rows
report — as `blue (all)`, say — while it is on file; setting one family's accent from that box
clears `accent`, since a key that overrides all three would go on drawing over the row just written.
`[tui.accents]` (or the dotted `accents.dac`/`accents.amp`/`accents.preamp` form) overrides one
family's accent at a time. Either form takes one of the 16 ANSI colour names (`black`, `red`,
`green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, and each with a `bright-` prefix) or a
`#rrggbb` value; a value outside those, or a `palette` outside the four presets, is a config error.

`colors` forces a colour-depth tier instead of detecting the terminal's own. `theme_background`, on
by default, has each palette paint its own dark greys under the screen, its panes and its boxes (see
["Theme background"](tui.md#theme-background)); off, the terminal's own background shows.
`transition` names how one screen gives way to the next — `none`, `trace` (the default), `noise` or
`melt` (see ["Effects"](tui.md#effects)) — and any other word is a config error naming the four, the
way a `palette` outside the presets is. `title_meter`, on by default, is the bars the terminal's
title carries, one per channel, while the device streams its meters, or the host capture's two while
the level comes from it (see ["Title meter"](tui.md#title-meter)).

`confirm_quit`, on by default, has `q` ask `Quit svolder? [Y/n]` before the TUI quits;
`confirm_quit = false` quits on the one `q`, and Ctrl-C quits at once either way. `host_audio`, on
by default, has the TUI reach the host's own audio: the Status pane's `Now playing` row and the
device screen's Host group, the host sink's volume and the media player's transport (see
["Host audio"](tui.md#host-audio)); `host_audio = false` leaves the host alone, and
`SVOLDER_HOST=off` in the environment does the same for that one process whatever the file says.
`visualizer`, on by default, is the switch for the device screen's Spectrum row, what the host plays
in sixteen bands (see ["Host audio"](tui.md#host-audio)).

`graphs`, on by default, is the switch for every graph the screens draw; `graphs = false` draws each
screen without them. `graph_span` is how much history the Telemetry and level graphs span, one of
`1m`, `2m`, `3m`, `5m` (the default), `10m`, `15m`, `30m` or `60m`, and `link_span` how much the
connection strip spans, one of `1h`, `3h`, `6h`, `12h` (the default) or `24h`; any other value is a
config error naming the allowed set, the way a `transition` outside the four is. `[tui.spectrum]`'s
`stops` gives the hue-drift mode (see ["Effects"](tui.md#effects)) a spectrum of its own to travel —
three to five `#rrggbb` colours, cool to hot — in place of the palette's built-in one; with none
set, the palette's own stops are what it travels instead.

`effects` is `true` (every animation on, the default), `false` (every animation off), or a table
naming each switch on its own — `enabled`, `eased`, `glow`, `shimmer`, `breathing`, `frames`,
`fault_presence`, `peak_hold`, `hue_drift`, `spectrum_drift`, `list_fade`, and `mascot` — for
example `effects = { shimmer = false, hue_drift = true }`; a key outside those twelve is a config
error (`tui.effects: unknown key "breathng"; the keys are enabled, eased, glow, shimmer, breathing,
frames, fault_presence, peak_hold, hue_drift, spectrum_drift, list_fade, mascot`), as is a value
that is not `true` or `false` (`tui.effects.glow must be true or false`); a key the table omits
keeps its own default, which is `true` for every switch but `hue_drift` and `spectrum_drift`, which
default to `false` (see ["Effects"](tui.md#effects) for what each one draws).

`config show --json` emits `effects` as the bare `true`/`false` word when every switch is at one of
those two defaults and as the full table otherwise, and omits `accent` — and each family under
`accents` — from the object entirely when the file names none, rather than printing them as `null`.
`[tui.spectrum]` is emitted as the table the file spells it in (`"spectrum": { "stops": [...] }`),
and left out entirely when the file names no stops.

### Socket

Both `svolderd` and `svolder` resolve the socket path the same way: the `--socket` flag, then
`SVOLDER_SOCKET`, then `socket` in the config file, then `$XDG_RUNTIME_DIR/svolder.sock`, then
`/run/user/<uid>/svolder.sock` if that directory exists, then `/tmp/svolder-<uid>/svolder.sock` in a
directory created with mode 0700. A blank override is no override: an empty `--socket`, an empty
`SVOLDER_SOCKET`, or `socket = ""` falls through to the next source instead of naming the empty
path. This works over SSH on any systemd distribution; a socket under `/run/user/<uid>` or
`$XDG_RUNTIME_DIR` goes away at logout unless linger is enabled (see above). See
["Environment variables"](environment.md#environment-variables) near the end of this reference for
`SVOLDER_SOCKET` alongside every other `SVOLDER_*` variable the workspace reads.

`svolderd` takes an exclusive lock on `<socket path>.lock` (mode 0600, left in place when the daemon
exits) before it opens a device, so a second daemon on the same socket path exits with "another
svolderd is already running" instead of competing for the devices; a socket handed over by a service
manager with no path takes no lock.

### Running in a container

`Dockerfile` builds the runtime image from the static `svolder` and `svolderd` that `make static`
builds in the build container (the same binaries the packages carry). The image starts as root only
to run its entrypoint (a small shell script), which hands `/config` and `/run/svolder`
to `PUID:PGID` (default 1000:1000; compose passes `HOST_UID` and `HOST_GID` from `.env`) when Docker
created them root-owned, because `./config` or `./run` did not exist, and then drops to that user,
keeping the groups compose's `group_add` gives (the bluetooth group) but not root's group 0. A folder owned by anyone else is never re-owned. The
entrypoint does nothing of this when the container does not start as root (a compose `user:` of your
own) or when it runs under rootless Docker or Podman, where the container's
root is already your user; `docker exec svolder svolder ...` runs the CLI as the same user. `svolderd`
is the container's own process, supervised by Docker's restart policy rather than a
systemd user unit — so `svolder daemon install` run inside a container detects it (`/.dockerenv` or
`/run/.containerenv`) and installs nothing, saying so instead.

`make docker` builds the static binaries and the image, tagged `svolder:local` and
`svolder:<version>`, from a clean tree (`ALLOW_DIRTY=1` builds from uncommitted changes anyway); it
prints `SKIP: docker not found` or `SKIP: docker not accessible` and exits 0 rather than failing
when Docker is not installed or this user cannot reach its daemon (for example, not being in the
`docker` group). `docker-compose.yml` is a runnable example:

Run `make docker` first: the compose file runs the image it builds (`svolder:local`) and builds none
of its own. The first time, `make docker` also writes a `.env` next to `docker-compose.yml` with
`BLUETOOTH_GID` (the host's `bluetooth` group, from `getent group bluetooth`), the value the compose
file cannot guess and refuses to run without, `HOST_UID` and `HOST_GID` (your `id -u` and `id -g`),
and creates `./config` and `./run` so they belong to you rather than to root. Starting the compose
file without `make docker` is fine too: with those keys in `.env` (or none, for 1000:1000) the
first start creates and repairs the folders. Then, and again after each update:

```
make docker
docker compose up -d
```

The container has no `bluetoothd` of its own; it reaches the adapter through the host's system
D-Bus, bind-mounted read-only at `/run/dbus/system_bus_socket`, and needs a user allowed to talk to
it — `group_add` with the host's `bluetooth` group id (the `BLUETOOTH_GID` that `make docker` wrote
to `.env`), or root. The compose file also runs the container without Docker's default AppArmor
profile (`apparmor=unconfined`) and without an SELinux label (`label=disable`): the host's bus
daemon enforces the container process's profile, which carries no D-Bus rules, so a confined
container is refused at its first message whatever user it runs as; the options change nothing in
the host's policy, are ignored where neither system is present, and the process keeps Docker's
capability set and seccomp filter.

`XDG_CONFIG_HOME=/config` inside the container, so the configuration file lives at
`./config/svolder/config.toml` on the host, editable the same way as an uninstalled `svolder`'s.
`SVOLDER_SOCKET=/run/svolder/svolder.sock` is bind-mounted at `./run/svolder.sock` (a
`svolder.sock.lock` appears beside it), so a client on the host reaches the daemon with:

```
SVOLDER_SOCKET=./run/svolder.sock svolder status
```

(`svolder` already honours `SVOLDER_SOCKET`, the flag's own environment source — see "Socket"
above.)

#### The first run

The image holds the `svolder` CLI beside the daemon, and `docker exec svolder svolder ...` runs it
as the daemon's user, so a first run writes no file by hand. With the container up
(`docker compose up -d`):

```
docker exec -it svolder svolder setup
docker exec -it svolder svolder
docker exec svolder svolder web on
docker exec svolder svolder web token
docker logs -f svolder
```

- `svolder setup` is the wizard: it shows how the host looks (the Container row names a mount that
  is missing), listens for the device, asks what to call it and its passphrase, and writes the
  `[[device]]` block. `-it` gives it the terminal it needs.
- `svolder` with no verb is the terminal UI, on the daemon the container runs, once a device is
  configured; while none is, it opens the same wizard as `svolder setup`. The TUI's Settings box
  has the Connect category, where the Web UI box and the MCP server row turn the two servers on.
- `svolder web on` (and `svolder mcp on`) turns a server on without the TUI: the daemon writes
  `enabled = true` under `[web]` (or `[mcp]`), reloads, and the verb prints
  `web: listening on 0.0.0.0:8738 (token, writes)`; `web off` and `mcp off` turn it off again. No
  restart is needed either way. `svolder web token` prints the token the sign-in page takes.
- `svolder mqtt on`, `svolder mqtt set host|port|username VALUE`, `svolder mqtt set commands on|off`
  and `svolder mqtt password` set up the Home Assistant MQTT client the same way (see ["MQTT and
  Home Assistant"](mqtt.md)); a compose file can set `SVOLDER_MQTT_ENABLED`, `SVOLDER_MQTT_HOST`,
  `SVOLDER_MQTT_PORT`,
  `SVOLDER_MQTT_USERNAME` and `SVOLDER_MQTT_PASSWORD` instead.
- `docker logs -f svolder` follows the daemon's log, where its warnings go: the daemon writes to
  the container's own log, which only the host reads (`podman logs` under Podman). Inside the
  container `svolder daemon logs` reads nothing and says so, naming both commands, and
  `svolder daemon status` ends with ``log: not a file; in a container, the container's own log
  (`docker logs <container>` or `podman logs <container>` on the host)``. The compose file caps
  that log (`logging:`, the `json-file` driver with `max-size: "10m"`
  and `max-file: "3"`): Docker's default has no cap and fills a small disk such as a Raspberry
  Pi's SD card.

Everything these write is one file, `/config/svolder/config.toml` in the container and
`./config/svolder/config.toml` on the host, made from the template (every key commented out at its
default) the first time something is written; it can still be edited by hand, and
`docker compose restart svolder` has the daemon read it again.

The compose file can turn the servers on as well, with no file at all: under `environment:` it
carries `SVOLDER_WEB_ENABLED: "1"` and `SVOLDER_MCP_ENABLED: "1"` commented out. Each is the
default of its table's `enabled`: the page is on while the file has no `enabled` under `[web]`,
and an `enabled` in the file, `true` or `false`, wins (so `svolder web off` still turns off a page
the variable turned on). `1`, `true`, `yes` and `on` read as on, `0`, `false`, `no` and `off` as
off; any other value stops the daemon at its start with a line naming the variable.

The image carries the PipeWire client tools (`pw-cli`, `pw-dump`, `pw-record`, `pipewire`, `wpctl`)
but no PipeWire daemon and no session bus. The computer's volume and EQ are opt-in, through
`docker-compose.host-audio.yml`, so a Bluetooth-only start with no user session is unaffected.
`make docker` writes more keys to `.env`, each only when missing (a line already there is
never rewritten): `HOST_UID` (your `id -u`), `HOST_GID`, `HOST_RUNTIME_DIR` (`$XDG_RUNTIME_DIR`, else
`/run/user/<uid>`) and, when the host's `pipewire-0` socket exists there (or `DOCKER_HOST_AUDIO=1`
asks: `DOCKER_HOST_AUDIO=1 make docker`), `COMPOSE_FILE=docker-compose.yml:docker-compose.host-audio.yml`. The override mounts the
host's runtime directory read-only at `/run/host-runtime` with `create_host_path: false` (Docker
creates nothing on the host: with no session the container refuses to start), sets
`PIPEWIRE_REMOTE=/run/host-runtime/pipewire-0` (a restarted PipeWire's new socket is seen without
restarting the container; the daemon's own `pipewire` child keeps its files in the private tmpfs), gives the container a private 0700 tmpfs as `XDG_RUNTIME_DIR`, and, since the host's runtime
directory is 0700 and its user's, needs the daemon to run as `HOST_UID`, which the entrypoint's drop
already does (`HOST_UID:HOST_GID`), so the first start still repairs root-owned folders and the
files the daemon creates in those bind mounts are yours on the host. The entrypoint names a folder
owned by a different uid on stderr (`docker logs svolder`), with the `.env` line that fixes it. Mounting the whole runtime directory exposes every
socket in it (the session bus, the audio and agent sockets) to the container, read-only but
connectable, which a single-socket mount would not; it is the price of the socket staying fresh
after a PipeWire restart. To turn it off, set `COMPOSE_FILE=docker-compose.yml` in `.env` (a line already there is kept, so
`make docker` does not switch it on again, which deleting the line would let it do while PipeWire
runs on the host; a `COMPOSE_FILE` exported in your shell beats `.env`), then `docker compose up -d`. For the published image, put
the override's contents in `docker-compose.override.yml` beside `docker-compose.yml`, or copy
`docker-compose.host-audio.yml` there and set `COMPOSE_FILE` and the two keys in `.env` by hand.
Through the socket the daemon reads and sets the computer's volume and carries the computer EQ as a
`pipewire` child of the daemon inside the container, which ends with it; it is applied again at the
daemon's start when `host-eq.toml` says it was on. The computer's crossfeed is the exception: it is
turned off at the daemon's start (the saved settings stay) and at its graceful stop, so it never
outlives the daemon that was asked to play it ([the resets](eq.md#it-turns-itself-off)). A one-shot
`svolder host eq` in the container refuses, printing `in a container the computer EQ runs with the
daemon. start svolderd, then try again`. Without the override the daemon has no host side
(`available` is `false` in `host`), the web page draws no Now playing card and no path line, and a
TUI on the host that joins the daemon follows the host's own player and volume with a watcher of its
own, as it does with no daemon at all.

The daemon captures what the computer plays only while a page's visualizer or a joined TUI asks
for it, and stops 2 s after the last one stops asking; with nobody looking it reads and analyses
nothing. It only ever listens to the output (the default sink's monitor) and never changes the
sink's volume or mute. Without `pw-record` the visualizer reports that it cannot capture.

#### The MCP server in the container

The compose file gives the container the host's network (`network_mode: host`), so the server is
reached at the host's own address and needs no published port. The image sets
`SVOLDER_MCP_LISTEN=0.0.0.0:8737` (and `SVOLDER_WEB_LISTEN` for the web page), which changes only
the default address: the server listens on every interface of the host, and the token (asked for by
default) is the check. Nothing answers until the server is turned on (see "The first run" above:
`svolder mcp on`, `svolder web on`, the two variables, or `enabled = true` under `[mcp]` or `[web]`
in `./config/svolder/config.toml`). A `listen` in that file still wins. The ports are the host's: a
`svolderd` running natively on the same machine must not have web or MCP enabled on 8737 or 8738 at
once; the listener that finds the port taken reports `cannot listen on ADDR: WHY` in `svolder daemon
status` and the daemon keeps its socket and its devices. To keep the server on the host's loopback
only, write `listen = "127.0.0.1:8737"` in the file.

Without host networking (a `ports:` list in place of `network_mode: host`, on a host such as Docker
Desktop, or rootless Docker, where the host network is a private one of its own and does not reach
the LAN), publish `8737:8737` and `8738:8738`; the container's
address is then its own bridge one, so set `[web] url` to the address your phone reaches (see
below). Under rootless Docker, use `ports:` and `[web] url` instead of host networking:
`network_mode: host` there is RootlessKit's own network (the container sees 10.0.2.100 behind
10.0.2.2), so the page is not reachable from the LAN or the host. The `--qr` code and pair link
refuse that address, and the bridge ones in Docker's 172.16.0.0/12 and Podman's 10.88.0.0/16,
and ask for `[web] url`; a bridge in any other range (Docker's address pools continue at
192.168.0.0/20 after 172.31, and `default-address-pools` can be anywhere) is not recognised, so set
`url` whenever the container is not on host networking. Give such a container a `hostname:` (the
computer's name) too: the Home Assistant device and ids come from the host name, which is
otherwise the container's id and changes each time it is created (see "The computer's EQ" in
[Home Assistant and MQTT](mqtt.md)).

A generated token would live in the container's own writable layer and change on every recreate
(`make docker`, then `docker compose up -d`), breaking every client, so keep the token under the
mounted `./config` instead. These two commands, run on the host in the directory that holds
`docker-compose.yml`, make the token file (mode 0600, which the daemon requires, owned by you, which
is the uid the container runs as):

```
mkdir -p config
( umask 077; head -c 32 /dev/urandom | base64 | tr '+/' '-_' | tr -d '=\n' > config/mcp-token )
```

Then `config/svolder/config.toml` (the file the container reads at `/config/svolder/config.toml`)
names it:

```toml
[mcp]
enabled = true
token_file = "/config/mcp-token"
hosts = ["localhost", "127.0.0.1"]
```

`docker compose up -d --force-recreate` starts the container with the section. The wildcard
address names no host, so with no `hosts` every `Host` header is let through and the token is the only
check. `hosts` turns that into a list, and the two names above keep a page that a DNS-rebinding
trick points at the server's port from being served: a client on the host uses
`http://127.0.0.1:8737/mcp` as in ["Connecting Claude Code"](mcp.md#connecting-claude-code) and is
accepted, any other name is refused.

A client that reaches the container through another name, such as a reverse proxy's, must be added
to `hosts`. An `SVOLDER_MCP_TOKEN` in
`.env` would not do on its own: compose reads `.env` to fill in the file's own `${...}` references
and passes nothing into the container unless the service lists it under `environment:`. The token
for the client is the container's, so ask the container:

```
docker exec svolder svolder mcp token
```

#### The web UI in the container

The page is reached the same way as the MCP server: the container shares the host's network and
listens on every interface by default (`SVOLDER_WEB_LISTEN`), so the only steps are turning it on
(`docker exec svolder svolder web on`, which writes `enabled = true` under `[web]` in
`config/svolder/config.toml`, the file the container reads at `/config/svolder/config.toml`) and
the sign-in, `docker exec svolder svolder web token`. The page is
then reached from this computer and your network, and `svolder web --qr` and `web pair` carry the
host's LAN address, as a native install's do; `listen = "127.0.0.1:8738"` keeps it on the host's loopback.

A generated token would live in the container's own writable layer and change on every recreate,
signing every browser out, so keep it under the mounted `./config`, where the host can read it too:
make the file with the MCP token's two commands, `config/web-token` in place of `config/mcp-token`,
and name it in `config/svolder/config.toml`. `url`, the address links and QR codes carry, is needed
only without host networking, where the container cannot know the address a phone reaches it by:

```toml
[web]
enabled = true
token_file = "/config/web-token"
url = "http://192.0.2.10:8738/"
```

`docker compose up -d --force-recreate` starts the container with the section. A wildcard `listen`
names no host, so with no `hosts` every `Host` header is let through and the sign-in is the check;
the page speaks plain HTTP, so on a network you do not trust put HTTPS in front of it (see
["Behind a proxy"](web-ui.md#behind-a-proxy) under "Web UI"). The token is
`docker exec svolder svolder web token`, or the file on the host. `svolder web` asks the daemon
through its socket, so it works from the host too,
`SVOLDER_SOCKET=./run/svolder.sock svolder web --qr`, though it takes `[web] url` from the
configuration it reads on the host; `docker exec svolder svolder web --qr` takes it from the
container's.

### IPC protocol

The wire format is newline-delimited JSON, one object per line, in both directions — meant for a
future front end other than `svolder` itself, not only for the CLI. A client sends a `RequestLine`,
`{"id": <u64>, "request": {"cmd": ...}}`; the daemon answers with a `Reply` tagged `"msg"`, either
`"response"` (the same `id`, and a `"result"`-tagged response) or `"event"` (pushed on a stream a
`subscribe` request opened, tagged `"event"`). Requests: `devices`, `state`, `get`, `set`, `raw`,
`eq_show`, `eq_set` (`band` is 1-based, as the user typed it), `eq_mode`, `passphrase`, `identify`,
`history`, `readings`, `recalled`, `web_pair`, `web_link`, `web_link_open`, `scan` (`window_ms` is
held to 30 seconds by the daemon, which answers for the window it listened for without saying so),
`subscribe`, `reload`, `version`, `stop`, `host`, `host_press`, `scene`.

See `crates/svolder-ipc/src/socket/protocol.rs` for every field's shape. A line the daemon cannot
parse as a request is answered with `{"result":"error","code":1,"message":"bad request: ..."}` —
carrying the line's own `id` when it had a numeric one, else `0` — and the connection keeps serving.
Either end reads a line of at most 1 MiB (1048576 bytes, not counting the newline): a request line
over that, or one that is not UTF-8, is answered the same way (`bad request: the line is longer than
1048576 bytes`, `bad request: the line is not UTF-8`; an over-long line is skipped through its
newline and carries no `id`, so `0`) and the connection keeps serving, and a client that reads a
reply line over the cap fails with `the daemon's reply line is longer than 1048576 bytes`.

`reload` (`{"cmd":"reload"}`) does what SIGHUP does — re-reads `config.toml` and the profile
registry and reconciles the running devices against them — and answers
`{"result":"reloaded","devices":[...]}` naming every configured device now running, or an `error`
response naming the config line that would not parse; `add` and `remove` (below), the TUI's device
and token writes and `svolder mcp token --rotate` send it whenever a daemon answers (the preset
writers send it only to a daemon that reads the same file, and say nothing, see
["The tone control"](mqtt.md#the-tone-control) under
["MQTT and Home Assistant"](mqtt.md#mqtt-and-home-assistant)), so the devices a running
`svolder devices` or the TUI's Devices screen shows are never stale and a rotated MCP token takes
effect at once (`svolder daemon restart` stops and starts the daemon and sends none).

`version` (`{"cmd":"version"}`) answers `{"result":"version","version":"...","mqtt":{...}}`: the
daemon's own package version, and where its MQTT client stands — `{"state":"off"}`,
`{"state":"connecting","host":"...","port":1883}` (with `"retry_in_secs"` while it waits between
attempts), `{"state":"trouble","host":"...","port":1883,"problem":"refused"}` (`problem` is
`refused`, `unreachable` or `tls`; with `"retry_in_secs"` as `connecting` has it), or
`{"state":"connected","host":"...","port":1883,"commands":false}` — all of which
`daemon status` (below) prints. The reply also carries `"mcp":{...}` for the MCP server:
`{"state":"off"}`, `{"state":"listening","listen":"127.0.0.1:8737","auth":"token","writes":false}`
(the address as bound, so a configured port 0 shows the one the system chose) or
`{"state":"failed","listen":"...","error":"..."}`.

The reply also carries `"config":"PATH"`, the configuration file the daemon reads, `"log":"PATH"` or
`"log":"journal"`, where its stderr goes (absent when it is a terminal or a pipe), and
`"mcp_token":{"kind":"environment"|"file"|"state","path":"PATH","tail":"3fQ"}` while its MCP server
has a token: where the token comes from (the daemon's environment, `[mcp] token_file`, or the
generated file in its state home), the file's path for the last two, and the token's last three
characters (empty for a token under 16 characters), which lets a client tell whether it reads the
same token; never the token itself.

The reply carries `"web":{...}` for the web UI the same way, `{"state":"off"}`,
`{"state":"listening","listen":"127.0.0.1:8738","auth":"token","writes":true,"guest":"none"}`
(`auth` is `token` or `none`, `guest` is `none` or `read`) or
`{"state":"failed","listen":"...","error":"..."}`, and `"web_token":{...}` with the same shape as
`mcp_token` while the web UI has a token, both of which `daemon status` and the TUI's Web UI box
read. The reply carries `"activity":{"mcp":{...},"web":{...}}` for each of the two servers that
serves, each `{"open":2,"hour":3,"day":5,"counted_secs":86400}` (plus `"capped":true` while the
table of clients was full, see "What a client is" under `daemon status`); a server that is off or
failed has no entry, and a daemon too old to count sends none, which a front end shows as nothing,
never as a zero. A client that does not know the `mqtt`, `mcp`, `config`, `log`, `mcp_token`, `web`,
`web_token` or `activity` field ignores it, a daemon too old to send one answers without it, and a
`state` a client does not know decodes as unknown rather than failing the reply.
`"config_changed":true` appears only while the configuration file's modification time differs from
the one the daemon's snapshot was read at (a hand edit not yet reloaded); a daemon too old to send
it reads as unchanged. `"config_missing":true` appears (with `config_changed`) only while the file
the daemon read is gone; a daemon too old to send it reads as there. `stop` (`{"cmd":"stop"}`)
answers `{"result":"stopping"}` and then ends the daemon's accept loop, as a SIGTERM would;
`daemon stop` (below) sends it when systemd is not managing the daemon.

`identify` (`{"cmd":"identify","device":"fake"}`) asks a device to show itself (function `200`) over
the link the daemon holds, and answers `{"result":"identify","device":"fake","sent":true}`;
`"sent":false` means one went out within the last 2 seconds and this request sent nothing. A device
the daemon has no link to answers the `device not connected` error (code 3) without ending the wait
before its next attempt. A daemon older than this request answers it with the `bad request` error
below, which `svolder identify` turns into `the daemon cannot identify a device, which this
svolder asks it to`.

`history` (`{"cmd":"history","device":"fake","span_secs":3600}`) answers how a device's link has
come and gone:
`{"result":"history","device":"fake","now_ms":1700000000000,"transitions":[{"at_ms":1699999000000,"connected":true},{"at_ms":1699999500000,"connected":false}]}`.
Each transition is a change of the link the daemon made, stamped with its wall clock in unix
milliseconds, oldest first; the reply carries those inside the last `span_secs` seconds and the one
before them, which says how the span began, and `now_ms` is the daemon's clock when it answered.

The daemon keeps at most 4096 transitions per device held for 24 hours plus the transition before
them, so a `span_secs` over a day is a day. A device that has never connected answers an empty list;
an unknown device answers the usual unknown-device error. A daemon older than this request answers
it with the `bad request` error above.

`readings` (`{"cmd":"readings","device":"fake","span_secs":1800}`) answers the last `span_secs`
seconds (at most 1800) of a device's readings, its recalls and its link, which the daemon keeps in a
ring per device for the front ends' graphs:
`{"result":"readings","device":"fake","now_ms":1700000000000,"bin_ms":1000,"series":[{"id":"10","key":"volume","aggregate":"last","points":[[1699999999000,109.0]]}],"recalls":[{"at_ms":1699999500000,"name":"HD650
warm"}],"link":[{"at_ms":1699999000000,"connected":true}]}`.

A series is one function that has had a numeric reading up to the end of the span (`id` dotted,
`key` the profile's key or `null`), binned by the second, `aggregate` saying how a bin's samples
were joined (`max` for a meter, a level or a fault so a peak never vanishes, `last` for the rest)
and `points` as `[unix milliseconds, value]`, oldest first, a true or false reading as 1 or 0. A
series starts with its newest point from before the span, when there is one, stamped at its own
time, which says what held when the span began, then the points inside it; a function reported only
before the span is answered with that one point. `link` carries the changes inside the span and the
one before them, which says how the span began. The TUI starts its graphs from it when it joins a
daemon, and the web page's strip reads it.

`recalled` (`{"cmd":"recalled","device":"fake","name":"HD650 warm"}`, answered `{"result":"ack"}`)
tells the daemon that a front end recalled a preset by writes of its own, which the writer cannot
tell from a band edit; `svolder eq load`, like the TUI, sends its own writes and then `recalled`; a
recall the daemon plans itself (MQTT, MCP, the web page) is noted by the daemon. Either way the
daemon keeps one memory per device of the preset last recalled by any front end, since it started: a
`devices` row carries it as `"recalled"` (absent when none), and a `subscribe` stream carries two
events more, in the usual `{"msg":"event","id":…,"event":…}` line:
`{"event":"recalled","device":"fake","name":"HD650 warm"}` when the memory changes and
`{"event":"climb","device":"fake","key":"volume","target":125}` when the daemon's climb of a level
starts, is steered or ends (`target` is `null` once it has ended).

A `devices` row also carries `"key_readings":["signal","volume"]`: the ids of the readings a front
end graphs first for that device, `signal` (the radio signal strength, which every device has) and
then the controls its profile marks `key_reading` (see the
[profile format](../../profiles/README.md)), in the profile's order; each id is one `readings`
keeps a series for. While the device has not been seen it is `["signal"]` alone, the profile's keys
joining once the daemon knows the profile; the key is absent only from a daemon too old to send it.
`svolder devices --json` prints it while a daemon answers, and not from the configuration alone.

`host` (`{"cmd":"host"}`) answers what the daemon knows of the computer's music player and the route
its sound takes to the default sink:
`{"result":"host","available":true,"playing":{"title":…,"artist":…,"album":…,"player":…,"status":"playing","length_us":…,"progress":{"position_us":…,"rate_milli":1000,"read_at_ms":…},"cover":"<hash>"},"route":{"sink":{…},"player":{…},"player_match":"pid","others_mixing":0,"host_eq":true,"stages":{"eq":true,"crossfeed":false,"speakers":false}},"now_ms":…}`.
`available` is `false` (and nothing else but `now_ms` follows) when the host side is off: `[host]
enabled = false`, `SVOLDER_HOST=off` in the daemon's environment, or no session bus. `playing` is
absent while there is no player. `route.stages` (`eq`, `crossfeed`, `speakers`) says which stages of
the computer's filter chain play when `host_eq` is true and the daemon's EQ view can vouch for them;
it is `null` otherwise, and a client then says only that a chain is on. The daemon reads the
PipeWire graph for `route` while a front end that shows the path is connected (a `subscribe` that
asked for the host, the event stream of a DAC's web page: every two seconds), every 30 s while the
MQTT client is connected, and once, within one second, for a plain `host` request (a script,
`svolder host check`, `svolder doctor`, MCP `get_host`; requests that arrive together share the
read). See [host-config-check.md](../design/host-config-check.md#reads). Lengths and positions are
in microseconds, the rate in thousandths (1000 is the normal rate), `read_at_ms` and `now_ms` in
unix milliseconds on the daemon's clock, and a volume in thousandths on `wpctl`'s scale (1000 is 100
%). `host_press` (`{"cmd":"host_press","button":"next"}`, `button` being `previous`, `play`, `pause`
or `next`) presses that button on the followed player and answers `{"result":"ack"}`, or an error
when the word is no button, the host side is off, there is no player or the player refused. The
computer's own volume is `{"cmd":"host_volume","pct":40}`, a whole percent from 0 to 100 on the wire
(more is refused; a boosted sink is shown as read, above 100, and never written), and `host_mute`
takes `muted`. A `subscribe` that carries `"host":true`
(`{"cmd":"subscribe","device":null,"host":true}`; without it, as before, no host events and no route
reads) also gets the view as a `{"event":"host",…}` event, whatever device it names (a selector that
names no device gets its error and the stream ends), once after the connection snapshot and then
each time the player or the route changes; a daemon from before the field ignores it and sends none,
and a client from before the event decodes it as one it does not know and skips it.

`scene` is one request for the four things done to a scene, with the op inside it:
`{"cmd":"scene","op":{"op":"save","name":"Desk","force":false}}`,
`{"op":"recall","name":"Desk","confirm":false}`, `{"op":"rename","from":"Desk","to":"Den"}` and
`{"op":"delete","name":"Desk"}`. A scene is the sound settings of the computer and of each connected
device saved under a name, and the daemon keeps them (the scene design page is
[scenes.md](../design/scenes.md)); a daemon that does so lists `scenes` among the `features` of its
`version` answer, and a client sends no scene request to one that does not. The answer is
`{"result":"scene","name":"Desk","outcome":[...]}`: `name` as the config spells it, and `outcome`
one `{"who","status","notes"}` per part for a recall (`who` is `computer` or the device's name,
`status` is `done`, `partial`, `left` or `failed`, `notes` the daemon's words for what was left out)
and for a save the `left` parts of the configured devices that are not connected, else empty. A
refusal is an `error` whose `reason` is `taken`, `bad_name`, `missing`, `read_only`, `file_busy`,
`invalid`, `stopping`, `internal`, `busy` (a device is still reading or applying a change, for a
save) or `nothing_to_save`. A recall that would play one preset on a device and on the computer at
once changes nothing and answers `{"result":"ask",...}` with the question in `message`; sending the
same request with `"confirm":true` recalls the scene as it was saved. Every `subscribe` stream,
whatever device it names, also carries
`{"event":"scenes","scenes":[{"name","parts":[{"who","present"}]}],"on","recalled","known"}`, once
after the snapshot and then each time the view changes; a client from before the event skips it.
`on` is absent while no scene is on or while that is not known, `recalled` is the scene last
recalled (kept across a restart in the state home), and `known` is false while a device is still
reading a setting that could change which scene is on.

`web_pair` and `web_link` ask the web UI for a pairing code or a link code (see
["Signing in"](web-ui.md#signing-in) under "Web UI"):
`{"result":"web_pair","code":"482913","expires_in_secs":300}` and
`{"result":"web_link","code":"…","expires_in_secs":300}`, the link code 22 base64url characters;
with the web UI off or failed they answer an `error` naming the fix. The socket already gives full
control of every device, so a code grants nothing the socket does not. `web_link_open` takes a link
code (`{"cmd":"web_link_open","code":"…"}`) and answers `{"result":"web_link_open","open":true}`
while a browser can still sign in with it, `false` once it was used, has lasted its five minutes or
was never made, and spends nothing, with the same `error` as the two above while the web UI is off
or failed; the TUI's Web UI box asks it to replace a link a browser has used.

One round trip, a `get` request for `volume` and its reply:

```
{"id":1,"request":{"cmd":"get","device":null,"key":"volume"}}
{"msg":"response","id":1,"response":{"result":"value","id":"10","key":"volume","label":"Volume","value_type":"u8","raw":109,"display":"109 (-6.0 dB)","value":-6.021,"unit":"dB"}}
```

`display` is for a person: a volume at silence reads `0 (silence)`. A balance reads its distance
from the centre (`0`). A program reads `raw` and `value`; `value` is `null` at silence, which JSON
has no number for.

## daemon

Check, install or control the background service directly, rather than letting the launcher chain
start one of its own and leave it running (see
["Transport and Defaults"](bluetooth.md#transport-and-defaults) and "Daemon and config" above).

```
Check, install or control the background service

Usage: svolder daemon [OPTIONS] <COMMAND>

Commands:
  status     Print the daemon's unit states, version and connected devices
  logs       Print the daemon's log
  reload     Have the daemon read its configuration and profiles again and apply them
  restart    Restart the daemon
  stop       Stop the daemon
  install    Write the systemd user units and enable them
  uninstall  Take the systemd user units out again
  help       Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Two environment variables, for development and testing rather than everyday use: `SVOLDER_SYSTEMCTL`
names the `systemctl` binary — every part of svolder that runs one looks it up through the same
function, so these verbs, `doctor`'s Daemon check and the daemon's own startup checks all run the
one the variable names — and `SVOLDER_UNITDIR` overrides where `install` writes and `uninstall`
removes the unit files.

### daemon status

Unit states when systemd manages the daemon, the socket otherwise; the daemon's own version, its
MQTT client, its MCP server, its web UI and the devices it holds either way. The MQTT client's line
follows the daemon's: `mqtt: off`, `mqtt: connecting to HOST:PORT` — with `(retry in N s)` while it
waits between attempts — `mqtt: not connected to HOST:PORT: WHY` once an attempt failed in a way
with words (`the broker refused the login (check username and password)`, `not reachable`,
`port 8883 is TLS, which svolder does not speak`) — or `mqtt: connected to HOST:PORT (read-only)`
or `(commands on)` (see
["MQTT and Home Assistant"](mqtt.md#mqtt-and-home-assistant)); a daemon too old to report one gets
no such line, and a state a newer daemon reports that this `svolder` does not know reads
`mqtt: unknown`. (`svolder mqtt set KEY VALUE` and `mqtt password` print the same line after they
write a key; while the client is off it adds what it saved, `mqtt: off (saved: host = broker.lan)`,
and `mqtt set commands on|off` fails with `(written, and the daemon has not acted on it: svolder
daemon status)` when the bridge still reports the other mode.)

The MCP server's line follows the MQTT client's: `mcp: off`, `mcp: listening on ADDR (token,
read-only)` — `writes` where the write tools are offered, `no auth` in place of `token` for `auth =
"none"` — or `mcp: cannot listen on ADDR: WHY`, with the same rules for a daemon too old to report
one (no line) and a state this `svolder` does not know (`mcp: unknown`). The web UI's line follows
it: `web: off`, `web: listening on ADDR (token, writes)` — `read-only` in place of `writes`, `no
auth` in place of `token`, and `, guest read` after them where visitors may look — or `web: cannot
listen on ADDR: WHY`, with the same two rules (`web: unknown`). A device the daemon is waiting to
try again for carries the wait, as whole seconds.

While a server serves, and the daemon counts, a line follows its own: `mcp clients: 2 connections
open; 3 clients in the past hour, 5 in the past day` after the MCP server's line and `web clients:
...` after the web UI's. A window the daemon has not lived through yet is said as what it saw:
`mcp clients: 0 connections open; 5 clients since the daemon started 40 min ago` (an hour or more
in, `3 clients in the past hour, 5 since the daemon started 6 h ago`, and a day or more in, `in the
past day`); a full table says `at least 5`. A server that is off or failed, and a daemon that does
not count, get no such line. The TUI's Connect category and Web UI box say the same counts in the
same words (the Web UI box in its short form).

A daemon whose `config.toml` changed since its last reload (a hand edit it has not taken) gets a
line after its version lines, `config: PATH changed since the last reload: svolder daemon reload
(or SIGHUP)`, naming the file the daemon reads, which a container's daemon keeps elsewhere. A
`config.toml` that was deleted gets `config: PATH is missing: the daemon keeps its running
configuration; restore the file, then svolder daemon reload` instead: a reload of a missing file is
refused (exit 1, the running configuration kept), never read as an empty configuration.

A daemon whose version is not this svolder's gets a line after its own, `mismatch: daemon vX.Y.Z is
older than this svolder vA.B.C: svolder daemon restart` for a daemon left running from before an
upgrade, or `mismatch: daemon vX.Y.Z is newer than this svolder vA.B.C: restart svolder` for a
daemon that is the later build (compared in semver order); a daemon of this svolder's own version
gets none. Every other svolderd of this user's listening on a socket this session does not reach —
one the launcher started for a session with a home or a runtime directory of its own, one started by
hand with a runtime directory of its own, or one in a container with a network of its own, whatever
it was told — gets an `also running` line naming its pid, where it listens (the listening socket it
holds, or its `--socket`/`SVOLDER_SOCKET` when its descriptors or its namespace's socket table
cannot be read) and the command that stops that one; there is no such line where there is no such
daemon.

The output ends with a `log:` line. A daemon that answers names its own log in its `version` reply,
and the line says it: the file's path, or `log: the journal` for a daemon whose stderr is the
journal. A daemon of this svolder's version or newer that names none writes no file, because its
stderr is a terminal or a pipe, and the line says `log: not a file (stderr is a terminal or a
pipe)`; in a container where systemd does not manage the daemon, where that pipe is usually the
container's log, it says ``log: not a file; in a container, the container's own log (`docker logs
<container>` or `podman logs <container>` on the host)``. With no daemon answering, and for an older
one that cannot say, the line is the autospawn log file's path,
`$XDG_STATE_HOME/svolder/svolderd.log`, and is printed only where systemd does not manage the
daemon. Exits 0 whatever it finds.

```
Print the daemon's unit states, version and connected devices

Usage: svolder daemon status [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

With no systemd user session at all (autospawn or a direct session serve instead) and no daemon
answering:

```
$ svolder daemon status
systemd: no user session; the daemon is whatever started one
daemon: nothing answers on /run/user/1000/svolder.sock
log: /home/user/.local/state/svolder/svolderd.log
```

With the units installed and a daemon holding two devices, bridged to a broker read-only:

```
$ svolder daemon status
svolderd.socket: installed, enabled, active
svolderd.socket listens on /run/user/1000/svolder.sock
svolderd.service: installed, enabled, active
daemon: 0.38.2 on /run/user/1000/svolder.sock
mqtt: connected to 192.168.1.10:1883 (read-only)
mcp: listening on 127.0.0.1:8737 (token, read-only)
mcp clients: 0 connections open; 2 clients in the past hour, 3 in the past day
web: listening on 127.0.0.1:8738 (token, writes)
web clients: 1 connection open; 1 client in the past hour, 2 in the past day
device bifrost: connected (Bifrost 3)
device gungnir: not connected, next attempt in 15 s (Gungnir 2)
also running: svolderd pid 4711 on /tmp/svolder-firstrun/svolder.sock — stop it with: svolder --socket /tmp/svolder-firstrun/svolder.sock daemon stop
log: the journal
```

#### What a client is

The counts are of clients, which are addresses, not people. A client is a distinct peer address that
passed the server's token check: for the MCP server a request that carries the bearer token, for
the web UI a request that carries this server's sign-in cookie. A connection that only ever sends a
wrong token, a guest (`guest = "read"`) or a signed-out visitor, and the sign-in POST itself (the
next request, with the cookie, counts) is never counted, open or seen. A server with no token
(`auth = "none"`) counts every request that reaches it (the MCP server before its `Host` check, the
web UI after its own).

- **Open** is the connections of those clients now: it counts from the connection's first request
  that passed until it closes. An MCP client mostly holds no connection between calls, so its
  server often reads 0 open; a web page holds its event stream, so the web UI's open is about open
  tabs.
- **The past hour and the past day** are the distinct clients seen within those windows. A client
  with a connection open is always seen; one without was seen when its last connection closed.
  Before the daemon has run an hour or a day the line says what it saw instead of claiming the
  window.
- **One address is one client.** Programs on this computer that reach the server through the same
  address read as one client: `127.0.0.1` and `::1` are two, and the computer's own network address
  is another (an IPv4 address that arrives as an IPv6 one folds into its IPv4 address); an IPv6
  temporary address counts apart from the stable one; clients behind a NAT or a proxy read as one.
- **In memory only.** Nothing is written, logged or sent but the counts. An address is held as a
  64-bit hash under keys drawn at random when the daemon starts, so no address is stored, and none
  can be read back from anything the daemon sends, shows or logs. The table holds at most 4096
  clients; one more drops the oldest with no connection open, and the line then says `at least` for
  the day (the hour's figure can also fall short while that lasts). The counts survive a reload and
  a restart of a server and end with the daemon.

### daemon logs

Reads the log a running daemon says it writes (the `log` field of its `version` reply):
`journalctl --user -u svolderd --no-pager -o short-iso` for the journal, or the last 200 lines of
the file it names. With no daemon answering, it reads the journal where systemd manages the daemon,
else the autospawn log file (`$XDG_STATE_HOME/svolder/svolderd.log`). `-f`/`--follow` follows either
one instead of printing what is there and exiting.

Before the first line it says on standard error what it reads, so a log nothing is writing to shows:
`svolder: reading /home/user/.local/state/svolder/svolderd.log (last written 2026-10-02 09:41)` for
a file, `svolder: reading the journal` for the journal. On a terminal, with `NO_COLOR` unset, each
line's timestamp is dim, a `warning:` line is yellow and an `error:` line red; through a pipe or a
redirect, or with `NO_COLOR` set, the lines are plain.

A daemon of this svolder's version or newer that writes no file gets `svolder: the daemon does not
log to a file (stderr is a terminal or a pipe); reading /home/user/.local/state/svolder/svolderd.log
instead` ahead of that line (`reading the journal instead` where systemd keeps the log). Asking a
daemon where it logs connects to its socket, so with socket activation it starts the daemon, as
`daemon status` does; the ask is given two seconds, and a daemon that does not answer in that time
is read as one that said nothing. A configuration that does not parse does not stop it: the socket
asked is the one `--socket` or the default names.

In a container the daemon's stderr is usually the container's log, which nothing inside the
container can read. There, for a daemon that writes no file (and with no daemon answering and no
log file), `logs` reads nothing, exits 0 and says where the log is: ``svolder: the daemon does not
log to a file; in a container what it says is the container's own log, read on the host: `docker
logs <container>` for Docker, `podman logs <container>` for Podman (the compose file names the
container `svolder`)``, with `-f` in both commands under `--follow`. A daemon started by hand in a
container writes to the terminal it was started in. Where systemd manages the daemon inside the
container, its log is the journal and `logs` reads it as anywhere.

```
Print the daemon's log

Usage: svolder daemon logs [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
  -f, --follow                 Follow the log instead of printing what is there and exiting
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Before anything has been autospawned:

```
$ svolder daemon logs
svolder: no daemon has logged anything to /home/user/.local/state/svolder/svolderd.log yet
```

### daemon reload

Sends the running daemon its `reload` request (see "IPC protocol" above): it reads `config.toml` and
the user's profiles again and applies them, starting, stopping or reconnecting devices, the MQTT
bridge, the MCP server and the web UI as the files now say, and keeping every link that did not
change. It prints `reloaded: the daemon now holds` and the devices the daemon runs. A file the
daemon cannot parse is refused in the daemon's words, naming the file and the line; nothing changes
and the verb exits with the daemon's own code, 1. When the file is this svolder's own too (one file
for both, the usual case), this svolder's read of it fails first: the verb exits 1 with that parse
error, and the daemon is never asked. A `config.toml` that is gone is refused the same way, exit 1
(`PATH is missing: the daemon keeps its running configuration; restore the file, then try again`):
the daemon keeps what it runs, and a reload takes the file again once it is back. A daemon started
with no file at all still reloads. To drop the file on purpose, run `svolder daemon restart`: the
new daemon starts with no file and so no devices. Or put a file back (`svolder config template`
prints a fresh one to save at the path `svolder config path` names) and run `svolder daemon
reload`. Where no daemon answers it exits 3, as every verb that needs
one does. It is the way out of the `config: … changed since the last reload` line that `daemon
status`, `doctor`, the TUI and the web page show after a hand edit; `systemctl --user reload
svolderd` and a SIGHUP do the same.

```
Have the daemon read its configuration and profiles again and apply them

Usage: svolder daemon reload [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### daemon restart

`systemctl --user restart svolderd.service` where the units are installed and `svolderd.socket`
listens on the resolved socket; otherwise starts `svolderd` itself, detached, the way autospawn does
— so `--socket` pointed at another daemon's socket restarts that daemon rather than systemd's.

Where it stops a daemon itself (no systemd), it waits for that daemon to exit, which is when the
lock beside its socket can be taken, before it starts the new one: up to 32 seconds, the daemon's
own shutdown deadline and two seconds more. If the old daemon is still there after that, it says so
on standard error and starts the new one anyway, which exits 3 on the lock.

`--if-running` restarts only a daemon that is running and starts none: with systemd's units it
restarts `svolderd.service` only when it is running — active, or still starting or reloading —
otherwise it restarts the daemon answering on the socket. Where there is none it says
`nothing to restart` and succeeds. `make install` runs it.

```
Restart the daemon

Usage: svolder daemon restart [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --if-running             Restart only a daemon that is running, and start none
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### daemon stop

`systemctl --user stop svolderd.service svolderd.socket` where the units are installed and
`svolderd.socket` listens on the resolved socket; otherwise sends the daemon its own `stop` request
(see "IPC protocol" above), which ends its accept loop as a SIGTERM would. A `--socket` the socket
unit does not listen on is another daemon's, and that daemon is the one asked to stop — which is
what the `also running` lines of `daemon status` and doctor's "Other daemons" rows print. "Nothing
to stop" where neither the units nor a daemon answering are there.

A stop that ends the daemon in order also turns the computer's crossfeed off and removes a chain
that ran for crossfeed alone; a computer EQ the chain carries stays as it was.

```
Stop the daemon

Usage: svolder daemon stop [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### daemon install

Writes `svolderd.socket` and `svolderd.service` into `~/.config/systemd/user` (as root,
`/etc/systemd/user`; or wherever `SVOLDER_UNITDIR` names) from the resolved socket path and this
binary's own directory, then `daemon-reload`s and `enable --now`s both; `--on-demand` enables only
the socket, so the daemon starts on the first command that needs it. On NixOS, prints the
home-manager block instead of writing anything; in a container, says the daemon is the container's
own process and installs nothing.

On a machine where neither unit runs (none installed yet, or both stopped), a daemon answering on
the target socket is one the launcher chain started, and it is asked to stop before the units are
enabled — otherwise systemd would unlink its socket file, bind its own listener there, and leave it
running, holding the device against the service systemd then starts. `svolderd` guards the same hole
from its own side: a daemon whose socket file is unlinked or replaced by another inode stops, saying
so in its log, rather than serving a path nothing can reach. A socket handed over by the service
manager is not watched — that path belongs to systemd.

A socket unit already running on another path than the one written keeps that path until it is
stopped, so the install stops it and `svolderd.service` after `daemon-reload` and before
`enable --now` starts them again on the new path, saying
`stopped svolderd.socket and svolderd.service, which listened on PATH`; a daemon a front end started
on the new path is asked to stop first, as above.

```
Write the systemd user units and enable them

Usage: svolder daemon install [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --on-demand              Enable only the socket, so the daemon starts on the first command
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

As root, it writes both files and makes no `systemctl` call; `--on-demand` names the socket alone in
the command:

```
# svolder daemon install
svolder: wrote svolderd.socket and svolderd.service into /etc/systemd/user, for every user
svolder: each user enables them with: systemctl --user enable --now svolderd.socket svolderd.service
```

Example:

```
$ svolder daemon install
svolder: wrote /home/user/.config/systemd/user/svolderd.socket
svolder: wrote /home/user/.config/systemd/user/svolderd.service
svolder: stopped the daemon svolder had started on /run/user/1000/svolder.sock
svolder: enabled svolderd.socket and svolderd.service
svolder: to keep the daemon running after logout: loginctl enable-linger user
```

### daemon uninstall

Disables both units, removes the two unit files (wherever they are), and `daemon-reload`s — each
step taken whatever the ones before it did, so a unit systemd has already forgotten still has a file
removed and a file already gone is one less thing to disable. As root, it removes the files and
makes no `systemctl` call, then says that each user who enabled them runs
`systemctl --user disable --now svolderd.socket svolderd.service`. On NixOS, says to take the block
out of your home-manager configuration instead.

```
Take the systemd user units out again

Usage: svolder daemon uninstall [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```
