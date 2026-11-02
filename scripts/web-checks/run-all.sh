#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# make web-checks: every node harness and every browser check of the web UI's effects (the menu,
# the levels, the layer, the smoke, what stirs it, its notes), the node ones once and the
# browser ones in headless Chromium and headless Firefox, one after the other against throwaway
# fake daemons (never the installed binaries, never ports 8737/8738). One line per check, a
# total, exit 1 on any FAIL. Each check's whole output is kept in target/web-checks/out/. Nothing
# here takes a picture of the smoke: it is read with readPixels and float readbacks, numbers only.
# The mutation runners (mutate_*.py, node/mutate_*.sh) are slow and run by hand, one worker.
#
# Which gate when (the coverage map, COVERAGE.md, says what each check proves and where):
#   run-all.sh          the full gate (make web-checks, 41 min 8 s measured; the smoke's checks
#                       take 30 min 8 s of it): required before every merge, and whenever a smoke
#                       file (smoke.js, smoke-model.js, smoke-director.js, smoke-engine.js,
#                       smoke-notes.js, effects.js, under bins/svolderd/web/themes/ember/) or one
#                       of the smoke's own checks changed;
#   run-all.sh --quick  the quick gate (make web-checks-quick, 5 min 27 s measured): the node
#                       checks, the browser checks that do not exercise the smoke (the effects
#                       menu and layout, the Tone message, the ring click, the curve's mouse
#                       drag, Now playing, the visualizer, Arrange mode, the theme contract and
#                       the core's own rows, devices, completeness on every device kind, the
#                       Firefox preload) and one short smoke check per browser. For a change to the
#                       page, the styles or the templates that leaves the smoke alone. It
#                       refuses (exit 1) when a smoke file or this directory changed against
#                       WEB_CHECKS_BASE (default main) in the commits, the working tree or the
#                       untracked files; outside a git checkout it runs and says it could not
#                       tell.
set -uo pipefail

here=$(dirname "$(readlink -f "$0")")
root=$(dirname "$(dirname "$here")")
out=$root/target/web-checks/out
cargo=${CARGO:-cargo}

quick=0
refusal_only=0
case "${1:-}" in
    "") ;;
    --quick) quick=1 ;;
    --refusal-only) quick=1; refusal_only=1 ;;
    *) echo "usage: run-all.sh [--quick]" >&2; exit 2 ;;
esac

# The files whose change needs the full gate: the smoke's, effects.js (it starts and stops the
# smoke; the quick gate does not cycle effects) and the smoke's own checks and harness. The checks
# of the menu, the layout, the Tone message, the ring click, devices and the preload are not
# listed: they belong to the quick gate.
w=bins/svolderd/web/themes/ember
c=scripts/web-checks
smoke_files=(
    $w/smoke.js $w/smoke-model.js $w/smoke-director.js $w/smoke-engine.js $w/smoke-notes.js
    $w/effects.js
    $c/smoke_core.py $c/smoke_page.py $c/smoke_quick.py $c/smoke_probe.py $c/notes.py
    $c/notes_tune.py $c/haze.py $c/scroll.py $c/sources.py $c/sweep.py $c/touch.py
    $c/effects_cycle.py $c/tune.py $c/common.py $c/run-all.sh $c/COVERAGE.md $c/soak.py
    $c/mutate_smoke.py $c/mutate_notes.py $c/mutate_haze.py $c/mutate_scroll.py
    $c/mutate_sources.py $c/mutate_effects.py $c/mutate_registry.py $c/node
)

# refuse_if_smoke_changed: the quick gate says the full gate is needed and exits 1 when a smoke
# file changed since WEB_CHECKS_BASE; without a git checkout (or the base) it says it cannot tell.
refuse_if_smoke_changed() {
    local base=${WEB_CHECKS_BASE:-main} changed line f
    if ! git -C "$root" rev-parse --git-dir >/dev/null 2>&1 ||
        ! git -C "$root" rev-parse --verify -q "$base" >/dev/null 2>&1; then
        echo "web-checks: could not tell whether a smoke file changed (no git checkout or no" \
            "'$base'); running the quick gate anyway" >&2
        return 0
    fi
    if ! changed=$({
        git -C "$root" diff --name-only "$base"...HEAD || exit 1
        git -C "$root" diff --name-only HEAD || exit 1
        git -C "$root" ls-files --others --exclude-standard || exit 1
    } | sort -u; [ "${PIPESTATUS[0]}" -eq 0 ]); then
        echo "web-checks: could not tell whether a smoke file changed (git failed against" \
            "'$base'); running the quick gate anyway" >&2
        return 0
    fi
    while IFS= read -r line; do
        for f in "${smoke_files[@]}"; do
            case $line in
                "$f" | "$f"/*)
                    echo "web-checks: a smoke file changed ($line): run make web-checks" >&2
                    exit 1
                    ;;
            esac
        done
    done <<<"$changed"
}
if [ "$quick" -eq 1 ]; then
    refuse_if_smoke_changed
fi
# --refusal-only (used by gate_refusal.sh): only the quick gate's refusal, then stop.
if [ "$refusal_only" -eq 1 ]; then
    exit 0
fi

missing=()
command -v node >/dev/null || missing+=("node")
command -v python3 >/dev/null || missing+=("python3")
python3 -I -c 'import websockets' 2>/dev/null || missing+=("python3-websockets (apt package)")
python3 -I -c 'import numpy, PIL' 2>/dev/null ||
    missing+=("python3-numpy and python3-pil (apt packages)")
command -v chromium >/dev/null || [ -x /snap/bin/chromium ] || missing+=("chromium")
command -v firefox >/dev/null || [ -x /snap/bin/firefox ] || missing+=("firefox")
command -v geckodriver >/dev/null || [ -x /snap/bin/geckodriver ] || missing+=("geckodriver")
if [ ${#missing[@]} -gt 0 ]; then
    printf 'web-checks: missing %s\n' "${missing[@]}" >&2
    exit 1
fi

if ! "$cargo" build -q --manifest-path "$root/Cargo.toml" -p svolderd; then
    echo "web-checks: the svolderd build failed; no check ran against a stale binary" >&2
    exit 1
fi

# leftovers of earlier runs (profiles, daemon homes, web copies) go, then the old outputs
mkdir -p "$out"
find "$root/target/web-checks" -mindepth 1 -maxdepth 1 ! -name out -exec rm -rf {} +
find "$out" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
total=0
failed=0
child=

# A TERM or INT (or an early exit) stops the check that is running; the Python checks close their
# browsers and daemons on a TERM.
stop_child() {
    if [ -n "$child" ]; then
        kill "$child" 2>/dev/null
        wait "$child" 2>/dev/null
    fi
}
trap stop_child EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# run LABEL COMMAND...: one check; its output in $out/LABEL.out, its last line and verdict here.
run() {
    local label=$1 start=$SECONDS verdict=ok
    shift
    total=$((total + 1))
    "$@" >"$out/$label.out" 2>&1 &
    child=$!
    wait "$child" || verdict=FAIL
    child=
    local last
    last=$(tail -n 1 "$out/$label.out")
    if [ "$verdict" = FAIL ]; then
        failed=$((failed + 1))
    fi
    printf '%-5s %-28s %4ds  %s\n' "$verdict" "$label" $((SECONDS - start)) "$last"
    if [ "$verdict" = FAIL ]; then
        grep -a '^FAIL\|Error\|Traceback' "$out/$label.out" | head -n 5 | sed 's/^/        /'
    fi
}

# A retuned look number (node/look-values.mjs) fails node-model alone: the tests written for the
# approved look skip themselves in the other harnesses while it does.
for name in effects facts model sources notes director keep tone-curve tone-drag playing viz \
    core-store core-stream core-actions core-prefs core-registry core-climb core-loop ember-parts \
    level-write toast frame-keys switch ember-order contract \
    paper-tokens paper-words paper-live paper-ruler paper-draw paper-notes paper-fold; do
    run "node-$name" env LOOK_RETUNED_SKIPS=1 node "$here/node/$name.mjs"
done
run gate-refusal bash "$here/gate_refusal.sh"
if [ "$quick" -eq 1 ]; then
    browser_checks=(effects_menu effects_layout tone_note knob_click tone_drag playing \
        device_presets reduced_motion visualizer arrange contract core_rows paper smoke_quick)
else
    browser_checks=(smoke_core smoke_page effects_menu effects_layout sources notes sweep haze \
        scroll tone_note knob_click tone_drag playing device_presets reduced_motion visualizer \
        arrange contract core_rows paper effects_cycle)
fi
for browser in chromium firefox; do
    for name in "${browser_checks[@]}"; do
        run "$name-$browser" python3 "$here/$name.py" "$browser"
    done
done
run effects_devices-chromium python3 "$here/effects_devices.py"
run completeness-chromium python3 "$here/completeness.py" chromium
if [ "$quick" -eq 0 ]; then
    run touch-chromium python3 "$here/touch.py"
    run tone_touch-chromium python3 "$here/tone_touch.py"
fi
run firefox_preload python3 "$here/firefox_preload.py"

echo "web-checks: $((total - failed)) of $total checks passed in $SECONDS s" \
    "(output in target/web-checks/out)"
[ "$failed" -eq 0 ]
