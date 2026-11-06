# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

CARGO ?= cargo

PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
# The user unit directory when PREFIX is the user's own ~/.local, the system one otherwise.
# Both sides are compared resolved: realpath follows symlinks where the path exists and abspath
# stands in where it does not, so a HOME with a trailing slash, a PREFIX with a `.` or `..` in
# it, and a PREFIX spelled through a symlink to an existing ~/.local all match.
resolved = $(or $(realpath $(1)),$(abspath $(1)))
ifeq ($(call resolved,$(PREFIX)),$(call resolved,$(HOME)/.local))
  UNITDIR ?= $(abspath $(HOME)/.config/systemd/user)
else
  UNITDIR ?= /etc/systemd/user
endif
# The systemctl `svolder daemon install` and `svolder daemon restart` run (they add `--user`
# themselves). `SYSTEMCTL=echo` makes install, install-service and uninstall dry runs that print
# what they would have asked systemd.
SYSTEMCTL ?= systemctl
# Whether `make install` restarts a daemon that is running, so the new build takes over; 0 skips it.
RESTART ?= 1
# Where packaging targets write what they build; the scripts, the Dockerfile and
# .dockerignore name it too, so it is not a setting.
DIST := target/dist
# The static release build's architecture, x86_64 or aarch64. Each has its own musl target, build
# image (packaging/Dockerfile.build, built for that platform and run natively, under QEMU
# user-mode emulation when it is not the host's) and target directory; the bare x86_64 binaries
# go to $(DIST), the aarch64 ones to $(DIST)/aarch64.
# Only the make command line sets it: an ARCH in the environment (amd64, arm64) means something else.
ARCH_GIVEN := $(filter command line,$(origin ARCH))
ARCH := $(if $(ARCH_GIVEN),$(ARCH),x86_64)
ifeq ($(ARCH),x86_64)
PLATFORM := linux/amd64
STATIC_DIR := target/static
STATIC_OUT := $(DIST)
else ifeq ($(ARCH),aarch64)
PLATFORM := linux/arm64
STATIC_DIR := target/static-aarch64
STATIC_OUT := $(DIST)/aarch64
# The bundled C code is built with musl-gcc, as on x86_64 (cc-rs would otherwise pick the glibc
# aarch64-linux-gnu-gcc); outline atomics would call libgcc's __aarch64_ldadd* helpers, which
# libdbus's link lacks.
STATIC_ENV := -e CC_aarch64_unknown_linux_musl=musl-gcc -e CFLAGS_aarch64_unknown_linux_musl=-mno-outline-atomics
else
$(error ARCH must be x86_64 or aarch64)
endif
STATIC_TARGET := $(ARCH)-unknown-linux-musl
BUILD_IMAGE ?= svolder-build:$(ARCH)
# A command in the build container, as the caller's uid, with the checkout at /src and
# CARGO_HOME inside target/static, so nothing it writes is root's or shared with a host build.
IN_BUILD = docker run --rm --platform $(PLATFORM) --user "$$(id -u):$$(id -g)" -v "$(CURDIR)":/src -w /src \
  -e CARGO_HOME=/src/target/static/cargo-home
# The workspace version, which the packages carry.
VERSION := $(shell sed -n 's/^version = "\(.*\)"$$/\1/p' Cargo.toml | head -n 1)
# The last commit's time: every file, package and image the release targets build carries it.
SOURCE_DATE_EPOCH ?= $(shell git log -1 --format=%ct 2>/dev/null)
# 1 lets the release targets build from uncommitted changes, to try the packaging itself; the
# artifacts then carry the last commit's version, time and source, not what they were built from.
ALLOW_DIRTY ?= 0
# The release assets, in $(DIST): `make packages` writes them, packaging/release-files.sh lists
# them in SHA256SUMS (it writes PKGBUILD, SHA256SUMS and RELEASE-NOTES.md; the notes are not an
# asset and SHA256SUMS does not list itself).
ASSETS = svolder_$(VERSION)_amd64.deb svolder_$(VERSION)_arm64.deb svolder-$(VERSION)-1.x86_64.rpm \
  svolder-$(VERSION)-x86_64-linux.tar.gz svolder-$(VERSION)-aarch64-linux.tar.gz \
  svolder-$(VERSION)-source.tar.gz THIRD-PARTY-LICENSES PKGBUILD SHA256SUMS
# What repro-check compares: the assets, the notes and the bare binaries.
ARTIFACTS = svolder svolderd svolder-sim aarch64/svolder aarch64/svolderd aarch64/svolder-sim \
  $(ASSETS) RELEASE-NOTES.md

.PHONY: help deps deps-build deps-lint deps-check deps-test build docs-env check license-check build-image release-tree third-party-licenses static packages release-files repro-check spdx-check workflow-check test safety-tests docs-check docs-check-merge fmt fmt-check clippy clean clean-all clean-rust demo demo-safe demo-dac demo-amp screenshots web-icons rate-check rate-check-test web-shots web-snippets theme-css-check web-checks web-checks-quick paper-shots signal-shots nightdrive-shots e2e docker install install-service uninstall

help: ## Show available targets
	@grep -E '^[a-z0-9-]+:.*##' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-22s %s\n", $$1, $$2}'

# deps-build, deps-lint and deps-check run first in the targets that build, lint or check on this
# host: they stop before cargo starts when something is missing, and print nothing otherwise.
deps: ## Check the tools and libraries the build and `make check` need, and name the command that installs what is missing
	@CARGO=$(CARGO) sh scripts/build-deps.sh check

deps-build:
	@CARGO=$(CARGO) sh scripts/build-deps.sh --quiet build

deps-lint:
	@CARGO=$(CARGO) sh scripts/build-deps.sh --quiet lint

deps-check:
	@CARGO=$(CARGO) sh scripts/build-deps.sh --quiet check

deps-test:
	@sh scripts/build-deps-test.sh

build: deps-build ## Build every crate
	$(CARGO) build --workspace

# The block between the markers is generated, not hand-written: this target is the only
# thing that should touch it, from whatever the just-built binary's own table says now.
docs-env: deps-build ## Rewrite docs/usage/environment.md's environment-variable table from the built binary
	$(CARGO) build -q -p svolder
	awk '/<!-- env-table:begin -->/ { print; system("SVOLDER_HOST=off target/debug/svolder help env --markdown"); skip=1; next } /<!-- env-table:end -->/ { skip=0 } !skip' docs/usage/environment.md > docs/usage/environment.md.new
	mv docs/usage/environment.md.new docs/usage/environment.md

check: deps-check fmt-check clippy safety-tests docs-check license-check spdx-check workflow-check web-snippets theme-css-check rate-check-test deps-test test ## Format check, lints, safety tripwire, docs check, licence check, SPDX header check, workflow check, web mutation snippets, rate-check parser self-test, build-deps self-test, tests

license-check: deps-check ## Fail if a dependency's licence is not compatible with GPL-3.0-or-later
	CARGO=$(CARGO) python3 scripts/check-licenses.py --self-test
	CARGO=$(CARGO) python3 scripts/check-licenses.py

build-image: ## Build the container the release builds run in (needs docker)
	docker build -q --platform $(PLATFORM) -f packaging/Dockerfile.build -t $(BUILD_IMAGE) packaging

# A release artifact names a commit: its version, its time and its source tarball.
release-tree:
	@[ -n "$(SOURCE_DATE_EPOCH)" ] || { echo "release builds need a git checkout: the version's time and the source tarball come from the last commit"; exit 1; }
	@[ "$(ALLOW_DIRTY)" = 1 ] || [ -z "$$(git status --porcelain)" ] \
	  || { echo "release builds need a clean tree: commit first (ALLOW_DIRTY=1 tries the packaging anyway)"; exit 1; }

third-party-licenses: build-image ## Write the licences of everything in the binaries to target/dist/THIRD-PARTY-LICENSES (needs docker)
	$(IN_BUILD) $(BUILD_IMAGE) packaging/third-party-licenses.sh

# Its own target directory keeps the musl build apart from the host's and each architecture's apart.
static: build-image ## Build static svolder, svolderd and svolder-sim in a container into target/dist (needs docker); ARCH=aarch64 builds arm64 into target/dist/aarch64
	$(IN_BUILD) $(STATIC_ENV) $(BUILD_IMAGE) cargo build --release --locked --target $(STATIC_TARGET) --target-dir $(STATIC_DIR) \
	  -p svolder -p svolderd -p svolder-sim
	mkdir -p $(STATIC_OUT)
	install -m755 $(addprefix $(STATIC_DIR)/$(STATIC_TARGET)/release/,svolder svolderd svolder-sim) $(STATIC_OUT)/

# One architecture's packages: the .deb and the binary tarball, and for x86_64 also the .rpm and
# the source tarball (`git archive` of the last commit, to which the container adds the vendored
# crates). The static binaries and THIRD-PARTY-LICENSES come first; the licence file does not
# depend on the architecture.
ifeq ($(ARCH_GIVEN),)
packages: release-tree ## Build both architectures' packages (.deb x2, .rpm, tarballs x2, source tarball), then PKGBUILD, SHA256SUMS and RELEASE-NOTES.md into target/dist (needs docker, a git checkout); ARCH=x86_64 or ARCH=aarch64 builds that architecture's alone
	$(MAKE) packages ARCH=x86_64
	$(MAKE) packages ARCH=aarch64
	$(MAKE) release-files
else
packages: release-tree static third-party-licenses
	git archive --format=tar --prefix=svolder-$(VERSION)/ -o target/source.tar HEAD
	$(IN_BUILD) -e SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) $(BUILD_IMAGE) packaging/package.sh $(VERSION) target/source.tar $(ARCH)
endif

# What combines the two architectures: PKGBUILD, SHA256SUMS (last) and RELEASE-NOTES.md.
release-files: ## Write PKGBUILD, SHA256SUMS and RELEASE-NOTES.md in target/dist from both architectures' packages (run by `make packages` without ARCH)
	@sh packaging/release-files.sh $(VERSION)

# The second build starts from empty musl target directories (the crates stay downloaded),
# so every artifact is built again and checked against the first one's sha256.
repro-check: ## Build the release artifacts for both architectures twice and fail if any differs (needs docker; about 13 minutes with the arm64 image and crates cached, about 40 cold)
	$(if $(ARCH_GIVEN),$(error repro-check covers both architectures: leave ARCH unset))
	$(MAKE) -s packages
	sha256sum $(addprefix $(DIST)/,$(ARTIFACTS)) > target/repro.sha256
	rm -rf target/static/x86_64-unknown-linux-musl target/static/release target/static-aarch64/aarch64-unknown-linux-musl target/static-aarch64/release target/package
	$(MAKE) -s packages
	sha256sum -c target/repro.sha256 || { echo "repro-check: differs; compare with diffoscope"; exit 1; }
	@echo "repro-check: every artifact is the same on a second build"

spdx-check: deps-check ## Fail if a source file lacks its SPDX licence or copyright header
	@files=$$(git ls-files) || { echo "spdx-check: git ls-files failed; this check needs a git checkout"; exit 1; }; \
	files=$$(echo "$$files" | grep -E '\.rs$$|\.toml$$|\.ya?ml$$|^Dockerfile|^scripts/|^systemd/|^packaging/|^Makefile$$|^flake\.nix$$|^bins/svolderd/templates/.*\.html$$|^bins/svolderd/web/.*\.(css|js|svg)$$' | grep -v '^bins/svolderd/web/themes/ember/htmx\.min\.js$$'); \
	if [ -z "$$files" ]; then echo "spdx-check: no source files found; this check needs a git checkout"; exit 1; fi; \
	echo "$$files" | tr '\n' '\0' | xargs -0 sh scripts/spdx-check.sh || exit 1; \
	echo "spdx-check: $$(echo "$$files" | wc -l) files carry the SPDX headers"

theme-css-check: ## Fail if a theme's style sheet names the frame's own parts (the menu button, its menu, the picker): a theme dresses them with the --frame-* tokens only
	@python3 -I scripts/theme-css-check.py

workflow-check: ## Fail if a GitHub Actions workflow could start by itself (any trigger but workflow_dispatch and a v* tag push), uses an action not pinned to a commit SHA, or runs a make target that does not exist
	python3 -I scripts/workflow-check.py --self-test
	python3 -I scripts/workflow-check.py

# SVOLDER_TEST_PREBUILT: this one cargo invocation builds every binary in the workspace
# before it runs a test, so a test that needs a sibling binary uses the one on disk. Without it
# the daemon a test spawns is built into target/test-daemon, a directory of its own: bins/svolder
# enables tokio/test-util as a dev-dependency, which this workspace build unifies into
# svolderd's tokio and a single-package `cargo build -p svolderd` does not, so the two are
# different artifacts, and sharing target/debug/svolderd would relink the one svolderd's own
# suite spawns.
# The suite runs with the config, state, data and cache homes beside a TMPDIR of its own, with
# SVOLDER_WIREPLUMBER naming a program that is not there (no test runs the host's WirePlumber)
# and without SVOLDER_MCP_TOKEN and SVOLDER_WEB_TOKEN, so a test that reaches for the user's real
# files (the MCP token under $XDG_STATE_HOME, say) finds a scratch copy instead, whatever the
# test does. The homes are not under TMPDIR, so each unit-test binary pins its own under it
# (svolder-unit-*). HOME stays as it is: cargo finds its registry through it. The run fails
# naming whatever a test left in TMPDIR, apart from those pinned homes. The scratch directory
# has a short fixed prefix so the longest test socket path stays under the 108 bytes a socket
# address holds.
test: deps-build ## Run all tests
	@scratch=$$(mktemp -d /tmp/sv.XXXXXX) || exit 1; \
	trap 'rm -rf "$$scratch"' EXIT; \
	mkdir "$$scratch/t" || exit 1; \
	env -u SVOLDER_MCP_TOKEN -u SVOLDER_WEB_TOKEN TMPDIR="$$scratch/t" \
	  XDG_CONFIG_HOME="$$scratch/config" \
	  XDG_STATE_HOME="$$scratch/state" \
	  XDG_DATA_HOME="$$scratch/data" XDG_CACHE_HOME="$$scratch/cache" SVOLDER_WIREPLUMBER="$$scratch/no-wireplumber" \
	  SVOLDER_AUTOEQ_URL=http://127.0.0.1:9/ SVOLDER_TEST_PREBUILT=1 $(CARGO) test --workspace --all-features \
	  || exit $$?; \
	left=$$(find "$$scratch/t" -mindepth 1 -maxdepth 1 ! -name 'svolder-unit-*' -exec basename {} \;); \
	if [ -n "$$left" ]; then \
	  echo 'make test: tests left these in their temporary directory:' >&2; \
	  echo "$$left" | sed 's/^/  /' >&2; \
	  exit 1; \
	fi

safety-tests: ## Fail if a safety test has gone missing: the volume's glide, the firmware-update and passphrase refusals (profile loader and the layers below), the capture's argv and scrub, the remote views
	@missing=''; \
	n=0; \
	while IFS='|' read -r path fn || [ -n "$$path" ]; do \
	  n=$$((n + 1)); \
	  case "$$path" in '#'*) continue ;; esac; \
	  if [ -z "$$path" ] && [ -z "$$fn" ]; then continue; fi; \
	  if [ -z "$$path" ] || [ -z "$$fn" ]; then \
	    echo "scripts/safety-tests.list:$$n: expected 'path|function', got '$$path|$$fn'" >&2; \
	    exit 1; \
	  fi; \
	  grep -q "fn $$fn(" "$$path" 2>/dev/null \
	    || missing="$$missing\n  $$path: $$fn"; \
	done < scripts/safety-tests.list; \
	if [ -n "$$missing" ]; then \
	  printf 'missing safety test:%b\n' "$$missing" >&2; \
	  exit 1; \
	fi

docs-check: ## Fail on a broken documentation link, a stale path, an over-long design doc or a process tag
	@python3 scripts/docs-check.py --self-test
	@python3 scripts/docs-check.py

docs-check-merge: ## Run before a merge to main: docs-check, and docs/ holds only shipped docs
	@python3 scripts/docs-check.py --merge

fmt: deps-lint ## Format the code
	$(CARGO) fmt --all

fmt-check: deps-lint ## Fail if code is not formatted
	$(CARGO) fmt --all -- --check

clippy: deps-lint ## Lint with warnings as errors
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

clean: ## Remove build output
	$(CARGO) clean

# Docker's images and the container's .env, config/ and run/ stay: they belong to an installed
# container, not to a build.
clean-all: clean ## Remove build output, the `nix build` link and the crates this checkout downloaded, so the next build starts as a new checkout's
	@if [ -L result ]; then rm result; fi
	@sh scripts/clean-downloads.sh

clean-rust: clean-all ## Everything clean-all removes, and cargo's whole download cache: every Rust project on this machine downloads its crates again
	@sh scripts/clean-downloads.sh all

# PROFILE=<fake profile> and SIGNAL=1 reach the fake device as SVOLDER_FAKE_PROFILE and
# SVOLDER_FAKE_SIGNAL.
# A knob not given is cleared from the shell's environment, so the default is the default.
DEMO_KNOBS = env $(if $(PROFILE),SVOLDER_FAKE_PROFILE=$(PROFILE),-u SVOLDER_FAKE_PROFILE) $(if $(SIGNAL),SVOLDER_FAKE_SIGNAL=$(SIGNAL),-u SVOLDER_FAKE_SIGNAL)

demo: deps-build ## Build, then try the TUI and the web UI on a fake device with YOUR COMPUTER'S REAL AUDIO (volume, player, EQ); PROFILE=fake-amp SIGNAL=1 pick the device
	$(CARGO) build --workspace
	$(DEMO_KNOBS) target/debug/svolder tui --demo --host --web

demo-dac: deps-build ## Like `make demo` on the fake DAC with its tone control (your computer's real audio)
	@$(MAKE) --no-print-directory demo PROFILE= SIGNAL=

demo-amp: deps-build ## Like `make demo` on the fake amplifier with its meters moving (your computer's real audio)
	@$(MAKE) --no-print-directory demo PROFILE=fake-amp SIGNAL=1

demo-safe: deps-build ## Build, then try the TUI on a fake device and a throwaway daemon (touches nothing of yours)
	$(CARGO) build --workspace
	$(DEMO_KNOBS) target/debug/svolder tui --demo

screenshots: deps-build ## Draw the README's pictures into docs/images from the TUI's own renderer (needs ffmpeg, fonts-ubuntu, fonts-dejavu-core; downloads one AutoEq file once)
	@bash scripts/screenshots.sh

web-icons: ## Draw the web UI's icons from the wordmark into bins/svolderd/web (needs ffmpeg)
	$(CARGO) test -q -p svolderd --bin svolderd -- --ignored --exact web::icons::draw
	for size in 192 512; do \
	  ffmpeg -v error -y -fflags +bitexact -i target/web-icons/icon-$$size.ppm -map_metadata -1 \
	    -flags:v +bitexact -pred mixed bins/svolderd/web/icon-$$size.png; \
	done

rate-check-test: ## Run the rate-check script's parsers on saved tool output (no PipeWire)
	@bash scripts/rate-check.sh --self-test >/dev/null || bash scripts/rate-check.sh --self-test

rate-check: ## Measure by hand whether the host capture leaves the graph's rate to the player (the real PipeWire session with silent streams; never part of check)
	@bash scripts/rate-check.sh

web-shots: ## Shoot the web UI in headless Firefox and Chromium into target/web-shots, and the README's two pictures (needs both browsers, ffmpeg and python3-websockets; run make screenshots once first)
	@bash scripts/web-shots.sh

paper-shots: deps-build ## Shoot Paper's stills: DAC and amp, desktop and phone, light and dark, into target/web-shots/paper (Chromium)
	@$(CARGO) build -q -p svolderd
	@python3 scripts/web-checks/paper_shots.py

signal-shots: deps-build ## Shoot Signal's stills: DAC, amp chosen, declared hop chosen, lost link, + sheet, linked amp shown, tablet, phone, into target/web-shots/signal (Chromium)
	@$(CARGO) build -q -p svolderd
	@python3 scripts/web-checks/signal_shots.py

nightdrive-shots: deps-build ## Shoot Night drive's stills: desktop at rest, a bubble held, the sheet, out of reach, no capture, phone upright and sideways, into target/web-shots/nightdrive (Chromium)
	@$(CARGO) build -q -p svolderd
	@python3 scripts/web-checks/nightdrive_shots.py

web-snippets: ## Fail if a web mutation runner's snippet does not occur exactly once in its file (needs python3 only)
	@python3 scripts/web-checks/snippets.py

web-checks: deps-build ## Run the web UI's node harnesses and browser checks in headless Chromium and Firefox (needs node, python3-websockets, python3-numpy, python3-pil, both browsers, geckodriver; never takes a picture of the smoke; over an hour at the default WEB_CHECKS_JOBS=6, a failure re-run alone)
	@bash scripts/web-checks/run-all.sh

web-checks-quick: deps-build ## Quick web gate: the node harnesses, the browser checks that leave the smoke alone and one short smoke check per browser (about 38 minutes at the default WEB_CHECKS_JOBS=6, a failure re-run alone); refuses after a smoke file changed, then run web-checks
	@bash scripts/web-checks/run-all.sh --quick

e2e: deps-build ## End-to-end test over BlueZ (needs two extended-advertising controllers); not part of check
	@bash scripts/e2e.sh

docker: ## Build the container image (skips without Docker)
	@VERSION=$(VERSION) SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) ALLOW_DIRTY=$(ALLOW_DIRTY) sh scripts/docker.sh

install: deps-build ## Build release, install the binaries under PREFIX (default ~/.local) and restart a running daemon (RESTART=0 skips)
	$(CARGO) build --release --workspace
	install -Dm755 target/release/svolder  $(BINDIR)/svolder
	install -Dm755 target/release/svolderd $(BINDIR)/svolderd
	install -Dm755 target/release/svolder-sim $(BINDIR)/svolder-sim
ifneq ($(RESTART),0)
	@if [ "$$(id -u)" = 0 ]; then \
	  echo "Each user restarts their own daemon: svolder daemon restart"; \
	else \
	  SVOLDER_SYSTEMCTL=$(SYSTEMCTL) $(BINDIR)/svolder daemon restart --if-running \
	    || echo "make: the daemon was not restarted; run 'svolder daemon restart'"; \
	fi
endif
	@echo "Installed to $(BINDIR). Run 'make install-service' to write and enable the units."

# One thing writes the unit files, and it is the verb: it renders them for the socket this
# session resolves and the binary it was run from, writes them where UNITDIR says, and enables
# them. A machine installed by hand and one installed by make end up with the same two files.
install-service: install ## Write and enable the user units through `svolder daemon install` (as root: write them for every user, each user enables them)
	SVOLDER_UNITDIR=$(UNITDIR) SVOLDER_SYSTEMCTL=$(SYSTEMCTL) $(BINDIR)/svolder daemon install

uninstall: ## Remove the installed binaries and the user units (as root: remove the files, each user disables them)
	SVOLDER_UNITDIR=$(UNITDIR) SVOLDER_SYSTEMCTL=$(SYSTEMCTL) \
	  $(BINDIR)/svolder daemon uninstall || true
	rm -f $(BINDIR)/svolder $(BINDIR)/svolderd $(BINDIR)/svolder-sim
