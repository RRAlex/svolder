<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Contributing

svolder is a Rust workspace: a command-line tool and terminal UI, a background daemon with a web
page, and the crates under them. [docs/architecture.md](docs/architecture.md) is the map of how they
fit together and where each kind of change goes; [docs/glossary.md](docs/glossary.md) explains the
words svolder uses in its own sense. Read the map first.

## Build

The build needs Rust 1.93 or later, a C compiler, `make`, and libdbus's development files, found
through `pkg-config`. `make check` also needs rustfmt, clippy, `python3` with PyYAML (the licence
and workflow checks) and a git checkout (the SPDX check), and runs on the host: in a container two
of its tests fail ([docs/known-issues.md](docs/known-issues.md)). On Ubuntu or Debian:

    sudo apt install build-essential cargo rustc pkg-config libdbus-1-dev rustfmt rust-clippy \
      python3 python3-yaml git

`make deps` checks all of it and, when something is missing, names it and prints the command that
installs it on this distribution: apt, dnf, pacman, zypper or apk, a `nix-shell` line on NixOS.
Every target that builds runs the same check first. Where the distribution's Rust is older than
1.93, [rustup](https://rustup.rs) installs a current one. Before `make` is installed,
`sh scripts/build-deps.sh check` runs the check.

    make deps       # what the build and `make check` need that is missing, and how to install it
    make build      # debug build of every crate
    make check      # format check, clippy, tests
    make help       # every target

Two more targets draw the documentation's pictures. `make screenshots` (the README's pictures)
needs ffmpeg and the `fonts-ubuntu` and `fonts-dejavu-core` packages. `make web-shots` (the web
UI's pictures, with the README's two, shot with the fake host so Now playing and the visualizer
show) needs headless Chromium and Firefox, ffmpeg, `python3-websockets` and one `make screenshots`
first.

## Try a change without a device

    make demo-safe

builds the workspace and runs `svolder tui --demo`: the TUI on a fake device, on a daemon of its
own.
The daemon, its configuration, its state and its socket live in a private directory made for the
run and removed when it ends, so nothing of yours is read or written. `make demo` is not that: it
adds `--host --web`, so the fake device sits beside your computer's real audio (the real volume,
media player and computer EQ, which stay changed after it) and the web UI listens on a free port.

The fake is a Gungnir 2. `SVOLDER_FAKE_PROFILE` picks another profile (`fake-amp`, `fake-mono`,
`fake-five` or any built-in one) and `SVOLDER_FAKE_SIGNAL=1` plays a music-like signal on the
meters:

    SVOLDER_FAKE_PROFILE=fake-amp SVOLDER_FAKE_SIGNAL=1 target/debug/svolder tui --demo

To use the fake without the demo, give every call the fake transport and a configuration of its own,
so a hand-run binary never reaches your real configuration, state or daemon:

    dir=$(mktemp -d) && chmod 700 "$dir"
    XDG_CONFIG_HOME=$dir/config XDG_STATE_HOME=$dir/state XDG_RUNTIME_DIR=$dir/run \
      XDG_DATA_HOME=$dir/data XDG_CACHE_HOME=$dir/cache SVOLDER_TRANSPORT=fake \
      target/debug/svolder inspect

`docs/design/testing.md` says how the fake device, the transport and the simulator work, and
[docs/usage/environment.md](docs/usage/environment.md) lists every environment variable; a new
variable needs `make docs-env`, which regenerates that file's table from the built binary
(`env_vars_doc.rs` fails without it).

## Which gate to run when

| Gate | Run it when | Needs |
| --- | --- | --- |
| `make test` | iterating on Rust | a build |
| `make check` | before every commit meant to stay, and before a merge | the build tools, a git checkout |
| `make safety-tests` | after touching a test file the safety list names (`make check` runs it) | nothing |
| `make docs-check` | after editing any Markdown (`make check` runs it) | `python3` |
| `make workflow-check` | after editing a workflow (`make check` runs it) | `python3` with PyYAML |
| `make web-checks-quick` | after a change to the web page, its styles or templates that leaves the smoke alone | node, chromium, firefox and geckodriver, about 38 minutes (much longer with `WEB_CHECKS_JOBS=1`; `WEB_CHECKS_JOBS` sets how many checks run at once) |
| `make web-checks` | before merging any change to the web page, and whenever a smoke file changed | node, both browsers, over an hour (six checks at a time, `WEB_CHECKS_JOBS` sets it) |
| `make e2e` | after a change to the transport, the session, the scanner, the daemon's reconnect or the simulator | two Bluetooth controllers; not part of `make check` |
| `make docs-check-merge` | before a merge to main | `python3` |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | before a merge, and after editing a doc comment | nothing |

`make check` does not run the web checks, `make e2e` or the soak, so a machine without browsers or
controllers can still run the ordinary gate. The quick web gate refuses after a smoke file changed,
so it never stands in for the full one. [docs/design/testing.md](docs/design/testing.md) has each
gate in depth, and [scripts/web-checks/README.md](scripts/web-checks/README.md) says which web check
proves which rule.

Run `cargo fmt --all` before committing; `make check` fails on a file it would change.

## Commits

- The subject is `Area: a plain sentence`, at most 100 characters. The area is the part of the
  project the change touches (`Docs`, `TUI`, `Daemon`, `CLI`, `Web`, `Build`, `Checks`, `Safety`,
  `README`).
- One change per commit. A commit that moves code does not also change behaviour.
- The commit is whole: it builds, and the gates its change calls for pass.
- No trailers: a commit message ends with its body, with no attribution line of any kind.
- A test that reads a document follows the document when it moves; the commit says so.

## Where new code goes

- A file does one job. A file that does several is split along them; a long file with one job
  stays whole.
- A family of files goes in its folder, named for the family (`widgets/`, `boxes/`, `commands/`).
- A folder's parent module lists its children, one line each, so the folder's file is its table
  of contents.
- A module file is `x.rs` beside its folder `x/`, never `x/mod.rs`, and its tests are in
  `x/tests.rs`, or in a `mod tests` at the foot of the file when they are few.
- A function does one job; a long one that is one linear list (a table of keys, a `match` of
  intents) stays whole.
- A test helper several modules share lives in the narrowest `test_support.rs` covering them.
- A large test module is a parent (`x/tests.rs`) listing its topic files, one per subject.
- An item takes the narrowest visibility that compiles; nothing becomes `pub` to make a move
  easy.
- No alias, `pub use` or wrapper is kept for an old path: a caller follows the module.
- A renamed module takes its type names, its words in comments and docs, and its test names with
  it.
- A web theme is a folder under `bins/svolderd/web/themes/` and one `THEMES` entry in
  `bins/svolderd/src/web/themes.rs`. What every theme shares (the stream, the model, the actions,
  the frame) is in `web/core/`, which no theme edits to suit itself; a gap there is fixed there.
  [Web themes](docs/design/web-themes.md) says how.

## Comments and documents

- A comment says what the code does and why it is that way, in the present tense. It never tells
  the story of how the code came to be: no "was", "used to", "no longer", "now", and no
  before-and-after.
- No process tags in a shipped file: no review-tag numbers, no pointers into a spec by section, no
  task numbers or milestone names, and none of the words that name a review's verdicts.
  `make docs-check` fails on review tags, section pointers and those words.
- Prose is British English, present tense, wrapped at 100 columns, and names no version number
  except as an example of a format.
- Every source and Markdown file starts with the SPDX licence header the files beside it carry;
  `make check` fails on a source file without one.
- Shipped files describe svolder and the protocol as [docs/protocol.md](docs/protocol.md) does.
  They never point into the official app's code, files or classes; see [LEGAL.md](LEGAL.md).

### The documentation check

`scripts/docs-check.py` (`make docs-check`) fails on:

- a relative Markdown link, in the README, this file or the Markdown under `docs/`, `profiles/` and
  `scripts/`, whose file or `#anchor` does not exist;
- a process tag in a tracked text file;
- a code span that looks like a repo path (`crates/svolder-ipc/`, `keys.rs`) in a shipped Markdown
  file or a `//` comment of the Rust sources that is no tracked file or folder; a span that is not
  a repo path goes on the `PATH_ALLOW` list in `scripts/docs-check.py`, with its reason;
- a line budget: 300 lines for `docs/architecture.md` and 400 for each file in `docs/design/`. A
  document that outgrows its budget is split in two, not squeezed.

## Versions

Every change that reaches main bumps the version: the minor number for a feature, the patch number
for a fix. A version is carried in four places, and a bump changes them together:

1. `version` under `[workspace.package]` in `Cargo.toml`;
2. `Cargo.lock`, refreshed with `cargo update --workspace --offline`;
3. the two examples that print it: the `daemon: … on …` line of `svolder daemon status` in
   `docs/usage/daemon.md` and the `v…` corner of the Menu (`Esc`) mock-up in `docs/usage/tui.md`;
4. the version's `## X` section at the top of `CHANGELOG.md`, summing up every version since the
   last release (`make packages` refuses a version without one, or one still the placeholder).

## The repository's folders

[docs/architecture.md](docs/architecture.md) lists the crates and what each owns. The rest of the
tree:

    profiles/                 built-in profiles, embedded at build time
    contrib/home-assistant/   a dashboard and a theme for Home Assistant, and notes on using them
    systemd/                  the daemon's user-unit template
    docs/                     protocol reference, usage guide, design notes, map, glossary, known
                              issues
    docs/images/              the README's pictures and their text twins; `make screenshots` and
                              `make web-shots` draw them
    scripts/                  checks and tools: `e2e.sh`, `docs-check.py`, `safety-tests.list`,
                              the licence and header checks, `screenshots.sh`, `docker.sh`
    scripts/web-checks/       the web UI's node and browser checks, tuning aids and mutation
                              runners; `make web-checks` runs them, `make web-checks-quick` a part
    packaging/                the release build container, the package scripts, cargo-about's
                              settings and the PKGBUILD template
    flake.nix                 the Nix build
    Dockerfile                the container image; `make docker` builds it
    docker-compose.yml        an example service, with the host bind-mounts it needs

`bins/svolderd` holds the web UI (`web/`, `templates/`) and the MCP server beside the daemon.

## Adding a device

A device is a TOML file and a few lines of registration, no behaviour:
[profiles/README.md](profiles/README.md) describes the format and lists the steps.

## Milestones

A milestone is a piece of work larger than a fix, such as a feature or a restructuring.

1. It starts on its own branch. Its working notes, a design and a plan, live on that branch only,
   in a folder under `docs/` that is not a shipped doc. They are the author's scaffolding and never
   reach main.
2. While it is built, each commit passes the gates its change calls for.
3. At the merge, the lasting decisions are folded into [docs/design/](docs/design/) and the
   [glossary](docs/glossary.md), and the notes are deleted in the same merge. A new public document
   is added to the `DOCS` list in `scripts/docs-check.py`.
4. `make docs-check-merge` runs before the merge. It runs the documentation check and then fails
   while any entry of `docs/` is not on that list, so a branch's working notes cannot reach main.
5. The merge is a merge commit on main (`git merge --no-ff`), and carries the version bump.
