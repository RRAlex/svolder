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

.PHONY: help deps deps-build deps-lint deps-check deps-test build docs-env check license-check build-image release-tree 3rdpartylicenses static packages release-files repro-check spdx-check network-check network-check-test unit-check workflow-check audit test safety-tests docs-check docs-check-merge routes-doc-check fmt fmt-check clippy clean clean-all clean-rust demo demo-safe demo-dac demo-amp demo-computer demo-computer-amp screenshots web-icons rate-check rate-check-test release-notes-test release-dry-run web-shots web-snippets tests-check theme-css-check tui-hangup-check web-checks web-checks-quick web-checks-task paper-shots signal-shots nightdrive-shots e2e host-e2e preamp-lab sim-app sim-app-test docker install install-service uninstall help-check

##@ Start
##@ Try it
##@ Install
##@ Check
##@ Long checks (not part of check)
#: Closes a pty on `svolder add` at a daemon walk's name step (a hangup, as a closed terminal
#: window is) against a fake daemon in temp homes, and fails unless the client leaves as a
#: cancelled walk (130 or SIGHUP's own death) and never by SIGABRT. Needs python3 only.
tui-hangup-check: deps-build ## A closed terminal during a daemon walk: no abort
	@$(CARGO) build -p svolder -p svolderd
	@python3 scripts/tui-hangup-check.py 3

#: Checks that the web tests catch a deliberate break: every web mutant runner, one after the other,
#: with its kills and wall time, then a summary; fails when a mutant survives. Slow, by hand only,
#: never a gate. FULL=1 adds the browser ones, many hours. The node runners take seconds to a few
#: minutes. The browser runners run their mutants in a pool (MUT_WORKERS, default from the cores and
#: the CPU pressure), timing mutants alone, and re-run every mutant the pool did not kill alone: only
#: that result counts.
#: Needs node and python3; FULL=1 also the browser checks (a built svolderd, Chromium).
#: FULL=1|0   also run the browser runners (default 0: the node ones only)
#: MUT_WORKERS=<number>   workers of each browser runner (default from the cores and CPU pressure)
tests-check: ## Check the web tests catch a deliberate break (by hand)
	@bash scripts/web-checks/mutants.sh $(if $(filter 1,$(FULL)),--full)

##@ Pictures and generated docs
##@ Release builds (docker)
##@ Clean

##@ Start
#: Give T=<target> to show that target's details and settings.
help: ## This list; make help T=<target> for one target's details
	@sh scripts/make-help.sh Makefile $(T)

# Fails if a summary would not fit `make help`'s one line, or a #: line has no target below it.
help-check:
	@sh scripts/make-help.sh --check Makefile

# deps-build, deps-lint and deps-check run first in the targets that build, lint or check on this
# host: they stop before cargo starts when something is missing, and print nothing otherwise.
#: Checks the tools and libraries the build and `make check` need, and prints the command that
#: installs what is missing.
deps: ## Check the build's tools; name the command to install them
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
##@ Pictures and generated docs
#: Rewritten from the built binary's own list of environment variables.
docs-env: deps-build ## Rewrite docs/usage/environment.md's variable table
	$(CARGO) build -q -p svolder
	awk '/<!-- env-table:begin -->/ { print; system("SVOLDER_HOST=off target/debug/svolder help env --markdown"); skip=1; next } /<!-- env-table:end -->/ { skip=0 } !skip' docs/usage/environment.md > docs/usage/environment.md.new
	mv docs/usage/environment.md.new docs/usage/environment.md

##@ Check
#: Format check, lints, safety tripwire, docs check, licence check, SPDX header check, network check, systemd unit check, workflow
#: check, web mutation snippets, theme CSS check, rate-check parser self-test, build-deps self-test, sim-app preflight self-test,
#: tests and this help's own check.
check: deps-check fmt-check clippy safety-tests docs-check license-check spdx-check network-check unit-check workflow-check web-snippets theme-css-check routes-doc-check rate-check-test network-check-test release-notes-test deps-test sim-app-test help-check test check-no-bluez ## Everything a commit must pass (make help T=check)

license-check: deps-check ## Fail on a licence not compatible with GPL-3.0-or-later
	CARGO=$(CARGO) python3 scripts/check-licenses.py --self-test
	CARGO=$(CARGO) python3 scripts/check-licenses.py

##@ Release builds (docker)
build-image: ## Build the container the release builds run in
	docker build -q --platform $(PLATFORM) -f packaging/Dockerfile.build -t $(BUILD_IMAGE) packaging

# A release artifact names a commit: its version, its time and its source tarball.
release-tree:
	@[ -n "$(SOURCE_DATE_EPOCH)" ] || { echo "release builds need a git checkout: the version's time and the source tarball come from the last commit"; exit 1; }
	@[ "$(ALLOW_DIRTY)" = 1 ] || [ -z "$$(git status --porcelain)" ] \
	  || { echo "release builds need a clean tree: commit first (ALLOW_DIRTY=1 tries the packaging anyway)"; exit 1; }

#: The licences of everything inside the binaries, gathered in the release container.
3rdpartylicenses: build-image ## Write target/dist/THIRD-PARTY-LICENSES
	$(IN_BUILD) $(BUILD_IMAGE) packaging/third-party-licenses.sh

# Its own target directory keeps the musl build apart from the host's and each architecture's apart.
#: ARCH=x86_64|aarch64   the architecture (default x86_64); aarch64 goes into target/dist/aarch64
static: build-image ## Static svolder, svolderd, svolder-sim into target/dist
	$(IN_BUILD) $(STATIC_ENV) $(BUILD_IMAGE) cargo auditable build --release --locked --target $(STATIC_TARGET) --target-dir $(STATIC_DIR) \
	  -p svolder -p svolderd -p svolder-sim
	mkdir -p $(STATIC_OUT)
	install -m755 $(addprefix $(STATIC_DIR)/$(STATIC_TARGET)/release/,svolder svolderd svolder-sim) $(STATIC_OUT)/

# One architecture's packages: the .deb and the binary tarball, and for x86_64 also the .rpm and
# the source tarball (`git archive` of the last commit, to which the container adds the vendored
# crates). The static binaries and THIRD-PARTY-LICENSES come first; the licence file does not
# depend on the architecture.
ifeq ($(ARCH_GIVEN),)
#: Both architectures' .deb, .rpm and tarballs, the source tarball, then PKGBUILD, SHA256SUMS and
#: RELEASE-NOTES.md. Needs a git checkout.
#: ARCH=x86_64|aarch64   that architecture's packages alone (default: both, then PKGBUILD, sums, notes)
packages: release-tree ## Every release package into target/dist
	$(MAKE) packages ARCH=x86_64
	$(MAKE) packages ARCH=aarch64
	$(MAKE) release-files
else
packages: release-tree static 3rdpartylicenses
	git archive --format=tar --prefix=svolder-$(VERSION)/ -o target/source.tar HEAD
	$(IN_BUILD) -e SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) $(BUILD_IMAGE) packaging/package.sh $(VERSION) target/source.tar $(ARCH)
endif

# What combines the two architectures: PKGBUILD, SHA256SUMS (last) and RELEASE-NOTES.md.
#: Written from both architectures' packages; `make packages` without ARCH runs it.
release-files: ## PKGBUILD, SHA256SUMS, RELEASE-NOTES.md in target/dist
	@sh packaging/release-files.sh $(VERSION)

# Builds the packages, installs the .deb in a throwaway container and starts the release image
# with the compose file a release attaches; its containers and image are named
# svolder-dryrun-<random>.
#: About 4 minutes with the build images and crates cached, about 20 cold, nearly all of it
#: `make packages`. Needs Docker and a clean tree (ALLOW_DIRTY=1 to try it anyway).
#: SKIP_PACKAGES=1   reuse target/dist instead of building the packages
release-dry-run: ## Dry-run a release: packages, .deb, container first run
	@MAKE="$(MAKE)" ALLOW_DIRTY=$(ALLOW_DIRTY) SKIP_PACKAGES=$(SKIP_PACKAGES) bash scripts/release-dry-run.sh

# The second build starts from empty musl target directories (the crates stay downloaded),
# so every artifact is built again and checked against the first one's sha256.
#: Both architectures: about 13 minutes with the arm64 image and the crates cached, about 40 cold.
repro-check: ## Build the release twice; fail if any file differs
	$(if $(ARCH_GIVEN),$(error repro-check covers both architectures: leave ARCH unset))
	$(MAKE) -s packages
	sha256sum $(addprefix $(DIST)/,$(ARTIFACTS)) > target/repro.sha256
	rm -rf target/static/x86_64-unknown-linux-musl target/static/release target/static-aarch64/aarch64-unknown-linux-musl target/static-aarch64/release target/package
	$(MAKE) -s packages
	sha256sum -c target/repro.sha256 || { echo "repro-check: differs; compare with diffoscope"; exit 1; }
	@echo "repro-check: every artifact is the same on a second build"

##@ Check
spdx-check: deps-check ## Fail if a source file lacks its SPDX header
	@files=$$(git ls-files) || { echo "spdx-check: git ls-files failed; this check needs a git checkout"; exit 1; }; \
	files=$$(echo "$$files" | grep -E '\.rs$$|\.toml$$|\.ya?ml$$|^Dockerfile|^scripts/|^systemd/|^packaging/|^Makefile$$|^flake\.nix$$|^bins/svolderd/templates/.*\.html$$|^bins/svolderd/web/.*\.(css|js|svg)$$' | grep -v '^bins/svolderd/web/themes/ember/htmx\.min\.js$$'); \
	if [ -z "$$files" ]; then echo "spdx-check: no source files found; this check needs a git checkout"; exit 1; fi; \
	echo "$$files" | tr '\n' '\0' | xargs -0 sh scripts/spdx-check.sh || exit 1; \
	echo "spdx-check: $$(echo "$$files" | wc -l) files carry the SPDX headers"

NETWORK_CHECK_PKG ?= svolder
#: svolderd is the only process that reaches the internet: `svolder` links no HTTP client or TLS.
network-check: ## Fail if the svolder client binary links ureq or rustls
	@# `cargo tree -i` prints the package's dependents when svolder links it, and says "nothing to
	@# print" (another crate of the workspace has the package) or "did not match any packages"
	@# (none has) when it does not. Fail closed: a clean answer is one of those two, and anything
	@# else (a dependent line, a new wording, any other cargo failure) fails the check.
	@for crate in ureq rustls; do \
	  out=$$($(CARGO) tree -p $(NETWORK_CHECK_PKG) -e normal -i $$crate 2>&1); code=$$?; \
	  if echo "$$out" | grep -qE "^$$crate v"; then \
	    echo "network-check: $(NETWORK_CHECK_PKG) links $$crate (only svolderd may reach the internet):"; echo "$$out" | grep -E "^(ureq|rustls) "; exit 1; \
	  fi; \
	  if ! echo "$$out" | grep -qE "nothing to print|did not match any packages"; then \
	    echo "network-check: cargo tree gave an answer this check does not know as clean (exit $$code):"; echo "$$out"; exit 1; \
	  fi; \
	done; \
	echo "network-check: $(NETWORK_CHECK_PKG) links no ureq and no rustls"

#: svolderd links both crates, so network-check must fail on it: proves the check can fail.
network-check-test: ## Prove network-check can fail (on svolderd)
	@if $(MAKE) --no-print-directory network-check NETWORK_CHECK_PKG=svolderd >/dev/null 2>&1; then \
	  echo "network-check-test: network-check passed on svolderd, which links ureq and rustls: it cannot fail"; exit 1; \
	fi; \
	echo "network-check-test: network-check fails on svolderd, as it must"

UNIT_SRC ?= systemd
UNIT_DOC ?= docs/usage/daemon.md
#: The shipped units, with the placeholders filled in, verify offline.
unit-check: ## Fail if a shipped systemd unit does not verify
	@# The drop-in's settings lines (comment and blank lines left out) must equal those of the
	@# fenced block after the `<!-- drop-in` marker in $(UNIT_DOC).
	@want=$$(grep -v -e '^#' -e '^$$' $(UNIT_SRC)/svolderd-filesystem.conf); \
	have=$$(awk '/^<!-- drop-in/{m=1;next} m&&/^```/{if(f){exit}f=1;next} m&&f' $(UNIT_DOC) | grep -v -e '^#' -e '^$$'); \
	if [ -z "$$have" ] || [ "$$want" != "$$have" ]; then echo "unit-check: the drop-in block in $(UNIT_DOC) differs from $(UNIT_SRC)/svolderd-filesystem.conf"; exit 1; fi; \
	echo "unit-check: the docs' drop-in block matches the file"
	@if ! command -v systemd-analyze >/dev/null 2>&1; then echo "unit-check: SKIPPED (systemd-analyze not found)"; exit 0; fi; \
	mkdir -p target; d=$$(mktemp -d target/unit-check.XXXXXX) || exit 1; \
	mkdir -p "$$d/bin" "$$d/plain" "$$d/dropin/svolderd.service.d" && printf '#!/bin/sh\n' > "$$d/bin/svolderd" && chmod +x "$$d/bin/svolderd"; \
	for unit in svolderd.service svolderd.socket; do \
	  sed -e "s|__BINDIR__|$$PWD/$$d/bin|g" -e 's|__SOCKET__|%t/svolder.sock|g' $(UNIT_SRC)/$$unit > "$$d/plain/$$unit"; \
	done; \
	cp "$$d/plain/svolderd.service" "$$d/plain/svolderd.socket" "$$d/dropin/"; \
	cp $(UNIT_SRC)/svolderd-filesystem.conf "$$d/dropin/svolderd.service.d/filesystem.conf"; \
	status=0; : > "$$d/shown"; \
	for dir in plain dropin; do \
	  out=$$(systemd-analyze --user verify "$$PWD/$$d/$$dir/svolderd.service" "$$PWD/$$d/$$dir/svolderd.socket" 2>&1); s=$$?; \
	  if [ $$s -ne 0 ]; then status=$$s; echo "$$out"; fi; \
	  echo "$$out" | grep -E 'svolderd|unit-check\.' >> "$$d/shown"; \
	done; \
	if [ -s "$$d/shown" ]; then cat "$$d/shown"; status=1; fi; \
	rm -r "$$d"; \
	if [ $$status -ne 0 ]; then echo "unit-check: systemd-analyze verify failed or warned about our units ($$status)"; exit 1; fi; \
	echo "unit-check: the units verify, plain and with the filesystem drop-in"

#: The menu button, its menu and the picker belong to the frame: a theme dresses them with the
#: --frame-* tokens only.
#: Also fails when a theme leaves a colour token of the frame unset in a scheme it lists, or the
#: frame's sheet turns to its phone layout at another width than the core's PHONE_MAX_PX.
theme-css-check: ## Fail if a theme styles the frame's own parts
	@python3 -I scripts/theme-css-check.py --self-test
	@python3 -I scripts/theme-css-check.py

#: Fails if a workflow could start by itself (any trigger but workflow_dispatch and a v* tag push),
#: uses an action not pinned to a commit SHA, or runs a make target that does not exist.
workflow-check: ## Fail on a GitHub workflow that is unsafe or broken
	python3 -I scripts/workflow-check.py --self-test
	python3 -I scripts/workflow-check.py

#: Needs the network (it fetches the RustSec advisory database and writes it to
#: ~/.cargo/advisory-db) and cargo-audit, so it is not part of `make check`. The two known
#: unmaintained warnings are allowed by id: paste (RUSTSEC-2024-0436, via nalgebra) and ttf-parser
#: (RUSTSEC-2026-0192, via ab_glyph); any other advisory or warning fails.
audit: ## Fail on an advisory in Cargo.lock (needs network)
	@$(CARGO) audit --version >/dev/null 2>&1 || { echo "audit: cargo-audit is not installed: cargo install --locked cargo-audit" >&2; exit 1; }
	$(CARGO) audit --deny warnings --ignore RUSTSEC-2024-0436 --ignore RUSTSEC-2026-0192

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
##@ Start
test: deps-build ## Run all tests
	@scratch=$$(mktemp -d /tmp/sv.XXXXXX) || exit 1; \
	trap 'rm -rf "$$scratch"' EXIT; \
	mkdir "$$scratch/t" || exit 1; \
	env -u SVOLDER_MCP_TOKEN -u SVOLDER_WEB_TOKEN -u SVOLDER_WEB_ENABLED -u SVOLDER_MCP_ENABLED \
	  -u SVOLDER_WEB_LISTEN -u SVOLDER_MCP_LISTEN -u SVOLDER_WEB_URL TMPDIR="$$scratch/t" \
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

##@ Check
#: make check's tests on a private system bus with no BlueZ, as the release runner has. Starts a
#: private D-Bus daemon with no BlueZ on it, points DBUS_SYSTEM_BUS_ADDRESS at it and
#: runs `make test`, so a test that assumes BlueZ answers fails here as it does on the release
#: runner, which has a system bus and no BlueZ. Needs dbus-daemon. Skipped, with a line saying
#: why, when the system bus already has no BlueZ (running or startable: scripts/bus-has-bluez.sh
#: exits 1), since `make test` just ran under exactly those conditions; a bus that cannot be
#: asked gets the run.
check-no-bluez: ## make test on a bus with no BlueZ (skipped if none)
	@sh scripts/bus-has-bluez.sh; rc=$$?; \
	if [ $$rc -eq 1 ]; then \
	  echo 'make check-no-bluez: skipped, the system bus has no BlueZ (running or startable), so make test already ran without it'; \
	  exit 0; \
	fi; \
	command -v dbus-daemon >/dev/null || { echo 'make check-no-bluez: dbus-daemon is not installed' >&2; exit 1; }; \
	bus=$$(mktemp -d /tmp/sv-bus.XXXXXX) || exit 1; \
	trap 'kill $$pid 2>/dev/null; rm -rf "$$bus"' EXIT; \
	dbus-daemon --session --nofork --address="unix:path=$$bus/bus" & pid=$$!; \
	i=0; while [ ! -S "$$bus/bus" ] && [ $$i -lt 50 ]; do i=$$((i+1)); sleep 0.1; done; \
	[ -S "$$bus/bus" ] || { echo 'make check-no-bluez: the private bus did not start' >&2; exit 1; }; \
	DBUS_SYSTEM_BUS_ADDRESS="unix:path=$$bus/bus" $(MAKE) --no-print-directory test

##@ Check
#: The volume's glide, the firmware-update and passphrase refusals (profile loader and the layers
#: below), the capture's argv and scrub, the remote views.
safety-tests: ## Fail if a safety test has gone missing
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

#: Also fails on an over-long design doc.
docs-check: ## Fail on a broken doc link, stale path or process tag
	@python3 scripts/docs-check.py --self-test
	@python3 scripts/docs-check.py

#: The table of docs/usage/web-ui.md against the routes the daemon serves.
routes-doc-check: ## Fail if the web routes and their docs table disagree
	@python3 -I scripts/routes-doc-check.py --self-test
	@python3 -I scripts/routes-doc-check.py

docs-check-merge: ## Before a merge to main: docs-check, shipped docs only
	@python3 scripts/docs-check.py --merge

##@ Start
fmt: deps-lint ## Format the code
	$(CARGO) fmt --all

##@ Check
fmt-check: deps-lint ## Fail if code is not formatted
	$(CARGO) fmt --all -- --check

clippy: deps-lint ## Lint with warnings as errors
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

##@ Clean
clean: ## Remove build output
	$(CARGO) clean

# Docker's images and the container's .env, config/ and run/ stay: they belong to an installed
# container, not to a build.
#: The next build starts as a new checkout's would.
clean-all: clean ## clean, plus the nix link and this checkout's crates
	@if [ -L result ]; then rm result; fi
	@sh scripts/clean-downloads.sh

#: Every Rust project on this machine downloads its crates again.
clean-rust: clean-all ## clean-all, plus cargo's whole download cache
	@sh scripts/clean-downloads.sh all

# PROFILE=<fake profile> and SIGNAL=1 reach the fake device as SVOLDER_FAKE_PROFILE and
# SVOLDER_FAKE_SIGNAL.
# A knob not given is cleared from the shell's environment, so the default is the default.
DEMO_KNOBS = env $(if $(PROFILE),SVOLDER_FAKE_PROFILE=$(PROFILE),-u SVOLDER_FAKE_PROFILE) $(if $(SIGNAL),SVOLDER_FAKE_SIGNAL=$(SIGNAL),-u SVOLDER_FAKE_SIGNAL)

##@ Try it
#: Drives your computer's real volume, media player and EQ: a volume or player change stays after
#: the demo; the computer EQ it applied is removed when it ends.
#: PROFILE=fake-amp|fake-mono|fake-five|bifrost3|<profile>   the fake device, any name in profiles/ (default gungnir2-v2, a DAC with tone control; fake-amp, -mono, -five: 2, 1, 5 meters)
#: SIGNAL=1   a demo signal moves the meters (a profile with meters: fake-amp, fake-mono, fake-five)
demo: deps-build ## TUI + web UI on a fake device; drives your real audio
	$(CARGO) build --workspace
	$(DEMO_KNOBS) target/debug/svolder tui --demo --host --web

demo-dac: deps-build ## make demo on the fake DAC; drives your real audio
	@$(MAKE) --no-print-directory demo PROFILE= SIGNAL=

demo-amp: deps-build ## make demo on the fake amp; drives your real audio
	@$(MAKE) --no-print-directory demo PROFILE=fake-amp SIGNAL=1

#: The computer side (volume, EQ points, presets) is simulated in the demo's daemon, which the TUI
#: and the web page share.
demo-computer: deps-build ## TUI + web UI on a fake DAC; your audio untouched
	$(CARGO) build --workspace
	$(DEMO_KNOBS) target/debug/svolder tui --demo --web

demo-computer-amp: deps-build ## demo-computer on the fake amp; your audio untouched
	@$(MAKE) --no-print-directory demo-computer PROFILE=fake-amp SIGNAL=1

#: Touches nothing of yours.
demo-safe: deps-build ## TUI alone on a fake device; your audio untouched
	$(CARGO) build --workspace
	$(DEMO_KNOBS) target/debug/svolder tui --demo

#: Starts svolder-sim for the official Schiit phone app (legacy advertisement, decoded frames
#: printed), after checking bluetoothd's flags read-only; when they are missing it prints the
#: systemd drop-in and the commands, changes nothing and starts nothing. Uses a real adapter,
#: so pick one no device of yours uses. Passphrase 1234. The app lists it as svolder-sim.
#: A=<hciN>   the Bluetooth adapter the sim advertises on (default hci0)
#: MODEL=<profile>   the built-in device profile it emulates (default gungnir2-v2)
sim-app: deps-build ## Simulator for the Schiit phone app; uses a real adapter
	$(CARGO) build -p svolder-sim
	@SIM_APP_BIN="$${CARGO_TARGET_DIR:-target}/debug/svolder-sim" sh scripts/sim-app.sh --adapter $(or $(A),hci0) --model $(or $(MODEL),gungnir2-v2)

sim-app-test:
	@sh scripts/sim-app-test.sh

##@ Pictures and generated docs
#: Needs ffmpeg, fonts-ubuntu and fonts-dejavu-core; downloads one AutoEq file once.
screenshots: deps-build ## Draw the README's TUI pictures into docs/images
	@bash scripts/screenshots.sh

web-icons: ## Draw the web UI's icons from the wordmark (ffmpeg)
	$(CARGO) test -q -p svolderd --bin svolderd -- --ignored --exact web::icons::draw
	for size in 192 512; do \
	  ffmpeg -v error -y -fflags +bitexact -i target/web-icons/icon-$$size.ppm -map_metadata -1 \
	    -flags:v +bitexact -pred mixed bins/svolderd/web/icon-$$size.png; \
	done

##@ Check
rate-check-test: ## Run rate-check's parsers on saved output (no PipeWire)
	@bash scripts/rate-check.sh --self-test >/dev/null || bash scripts/rate-check.sh --self-test
release-notes-test: ## Run the release-notes extraction on a temp repo
	@bash packaging/release-notes.sh --self-test >/dev/null || bash packaging/release-notes.sh --self-test

##@ Long checks (not part of check)
#: Measures on the real PipeWire session with silent streams.
rate-check: ## By hand: does the capture leave the rate to the player?
	@bash scripts/rate-check.sh

##@ Pictures and generated docs
#: Into target/web-shots. Needs both browsers, ffmpeg and python3-websockets; run
#: make screenshots once first.
web-shots: ## Web UI stills in Firefox and Chromium, README pictures
	@bash scripts/web-shots.sh

#: Chromium: DAC and amp, desktop and phone, light and dark.
paper-shots: deps-build ## Paper's stills into target/web-shots/paper
	@$(CARGO) build -q -p svolderd
	@python3 scripts/web-checks/paper_shots.py

#: Chromium: DAC, amp chosen, declared hop chosen, lost link, + sheet, linked amp shown, tablet, phone.
signal-shots: deps-build ## Signal's stills into target/web-shots/signal
	@$(CARGO) build -q -p svolderd
	@python3 scripts/web-checks/signal_shots.py

#: Chromium: desktop at rest, a bubble held, the sheet, out of reach, no capture, phone upright
#: and sideways (DAC and computer side).
nightdrive-shots: deps-build ## Night drive's stills into target/web-shots/nightdrive
	@$(CARGO) build -q -p svolderd
	@python3 scripts/web-checks/nightdrive_shots.py

##@ Check
#: Needs python3 only.
web-snippets: ## Fail on a moved mutant snippet or an unlisted node harness
	@python3 scripts/web-checks/snippets.py
	@python3 -I scripts/web-checks/node-list.py --self-test
	@python3 -I scripts/web-checks/node-list.py

##@ Long checks (not part of check)
#: The node harnesses and the browser checks, headless; a failure is re-run alone. Never takes a
#: picture of the smoke. Needs node, node-jsdom, python3-websockets, python3-numpy, python3-pil, both browsers
#: and geckodriver.
#: WEB_CHECKS_JOBS=<number>   how many checks run side by side (default: from the cores and the CPU pressure)
web-checks: deps-build ## Every web check in Chromium and Firefox (over an hour)
	@bash scripts/web-checks/run-all.sh

#: The node harnesses, the browser checks that leave the smoke alone and one short smoke check per
#: browser. Refuses after a smoke file changed: run web-checks then.
#: WEB_CHECKS_JOBS=<number>   how many checks run side by side (default: from the cores and the CPU pressure)
web-checks-quick: deps-build ## The quick web gate (about 38 minutes)
	@bash scripts/web-checks/run-all.sh --quick

#: The quick gate with the logic-only checks (Space key, device presets, core rows) in Chromium
#: only. For a task or a lane end; refuses like the quick gate.
#: WEB_CHECKS_JOBS=<number>   how many checks run side by side (default: from the cores and the CPU pressure)
web-checks-task: deps-build ## The task web gate (quick, logic checks in Chromium only)
	@bash scripts/web-checks/run-all.sh --task

#: Needs two extended-advertising controllers.
e2e: deps-build ## End to end over BlueZ
	@bash scripts/e2e.sh

#: A private PipeWire and WirePlumber: no sound card, never touches your session.
host-e2e: deps-build ## The computer EQ end to end on a private PipeWire
	@CARGO_TARGET_DIR="$(CURDIR)/target" $(CARGO) build -q -p svolderd
	@bash scripts/host-e2e/run.sh

#: Random event sequences against a model of PipeWire.
#: SEED=<number>   the first sequence's seed (default 1000)
#: RUNS=<number>   how many sequences (default 5000)
preamp-lab: deps-build ## Long run of the computer EQ's preamp lab
	@scratch=$$(mktemp -d /tmp/sv.XXXXXX) || exit 1; \
	trap 'rm -rf "$$scratch"' EXIT; \
	SVOLDER_LAB_SEED="$(or $(SEED),1000)" SVOLDER_LAB_RUNS="$(or $(RUNS),5000)" TMPDIR="$$scratch" \
	  $(CARGO) test --release -p svolder-host --lib -- --ignored --nocapture the_lab_finds_nothing_in_a_long_run

##@ Release builds (docker)
docker: ## Build the container image (skips without Docker)
	@VERSION=$(VERSION) SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) ALLOW_DIRTY=$(ALLOW_DIRTY) sh scripts/docker.sh

##@ Install
#: PREFIX=<directory>   where the binaries go, in PREFIX/bin (default ~/.local)
#: RESTART=0|1   0 leaves a running daemon alone (default 1: restart it on the new build)
install: deps-build ## Build release, install, restart a running daemon
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
#: Through `svolder daemon install`. As root: writes them for every user, and each user enables them.
install-service: install ## Write and enable the user units
	SVOLDER_UNITDIR=$(UNITDIR) SVOLDER_SYSTEMCTL=$(SYSTEMCTL) $(BINDIR)/svolder daemon install

#: As root: removes the files, and each user disables them.
uninstall: ## Remove the installed binaries and the user units
	SVOLDER_UNITDIR=$(UNITDIR) SVOLDER_SYSTEMCTL=$(SYSTEMCTL) \
	  $(BINDIR)/svolder daemon uninstall || true
	rm -f $(BINDIR)/svolder $(BINDIR)/svolderd $(BINDIR)/svolder-sim
