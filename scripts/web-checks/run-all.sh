#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# make web-checks: every node harness and every browser check of the web UI's effects (the menu,
# the levels, the layer, the smoke, what stirs it, its notes), the node ones once and the
# browser ones in headless Chromium and headless Firefox, WEB_CHECKS_JOBS at a time (default 6;
# 1 runs them one after the other, the serial lane's checks after the others) against throwaway
# fake daemons (never the
# installed binaries, never ports 8737/8738; each check has its own daemon home, free ports,
# browser profile and output files). A check that fails beside others is run again alone: passing
# is `flaky` (counted as passed, listed in the total), failing again is FAIL. One line per check
# as it finishes, a total, exit 1 on any FAIL. Each check's whole output is kept in target/web-checks/out/. Nothing
# here takes a picture of the smoke: it is read with readPixels and float readbacks, numbers only.
# The mutation runners (mutate_*.py, node/mutate_*.sh) are slow and run by hand, one worker.
#
# Which gate when (the coverage map, COVERAGE.md, says what each check proves and where):
#   run-all.sh          the full gate (make web-checks, over an hour at WEB_CHECKS_JOBS=6; the smoke's checks
#                       take about half of it): required before every merge, and whenever a smoke
#                       file (smoke.js, smoke-model.js, smoke-director.js, smoke-engine.js,
#                       smoke-notes.js, effects.js, under bins/svolderd/web/themes/ember/) or one
#                       of the smoke's own checks changed;
#   run-all.sh --quick  the quick gate (make web-checks-quick, about 38 min at WEB_CHECKS_JOBS=6, much longer with WEB_CHECKS_JOBS=1): the node
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
n=$c/node
smoke_files=(
    $w/smoke.js $w/smoke-model.js $w/smoke-director.js $w/smoke-engine.js $w/smoke-notes.js
    $w/effects.js
    $c/smoke_core.py $c/smoke_page.py $c/smoke_quick.py $c/smoke_probe.py $c/notes.py
    $c/notes_tune.py $c/haze.py $c/scroll.py $c/sources.py $c/sweep.py $c/touch.py
    $c/effects_cycle.py $c/tune.py $c/common.py $c/run-all.sh $c/COVERAGE.md $c/soak.py
    $c/mutate_smoke.py $c/mutate_notes.py $c/mutate_haze.py $c/mutate_scroll.py
    $c/mutate_sources.py $c/mutate_effects.py $c/mutate_registry.py
    $n/director.mjs $n/effects.mjs $n/look-values.mjs $n/model.mjs $n/notes.mjs $n/sources.mjs
    $n/mutate_director.sh $n/mutate_model.sh $n/mutate_notes.sh $n/mutate_sources.sh
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

if ! "$cargo" build -q --manifest-path "$root/Cargo.toml" -p svolderd -p svolder; then
    echo "web-checks: the svolderd/svolder build failed; no check ran against a stale binary" >&2
    exit 1
fi

# leftovers of earlier runs (profiles, daemon homes, web copies) go, then the old outputs; this
# runs before any check starts, never while one runs
mkdir -p "$out"
find "$root/target/web-checks" -mindepth 1 -maxdepth 1 ! -name out -exec rm -rf {} +
find "$out" -mindepth 1 -maxdepth 1 -exec rm -rf {} +

jobs_n=${WEB_CHECKS_JOBS:-6}
case $jobs_n in
    '' | *[!0-9]* | 0) echo "web-checks: WEB_CHECKS_JOBS must be a number of 1 or more" >&2; exit 2 ;;
esac

# A TERM or INT (or an early exit) stops every check that is running. Each check runs in a process
# group of its own (setsid) whose id is kept in $out/LABEL.pgid, so the kill reaches the check's
# children too, even when its wrapper was itself killed; the Python checks close their browsers and
# daemons on a TERM.
stop_children() {
    local f pg pid
    trap - EXIT TERM INT
    for pid in $(jobs -rp); do kill -TERM "$pid" 2>/dev/null; done
    for f in "$out"/*.pgid; do
        [ -e "$f" ] || continue
        read -r pg <"$f"
        kill -0 -- "-$pg" 2>/dev/null && kill -TERM -- "-$pg" 2>/dev/null
    done
    wait 2>/dev/null
    for f in "$out"/*.pgid; do
        [ -e "$f" ] || continue
        read -r pg <"$f"
        kill -0 -- "-$pg" 2>/dev/null && kill -KILL -- "-$pg" 2>/dev/null
    done
    return 0
}
trap stop_children EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# excerpt FILE: what failed in a check's output, indented.
excerpt() {
    grep -a '^FAIL\|Error\|Traceback' "$1" | head -n 5 | sed 's/^/        /'
}

# one_check LABEL FILE FAILWORD COMMAND...: one check, run in the background (the caller waits for
# it): its whole output goes to FILE, its verdict (`ok` or FAILWORD: `FAIL`, or `retry` for a
# failure that will be run again) and seconds to $out/LABEL.verdict, and its line (label, seconds,
# last line) is printed as it finishes. A check with no verdict file at the end was lost (killed,
# crashed) and counts as FAIL.
one_check() {
    local label=$1 file=$2 fail_word=$3 start=$SECONDS verdict=ok c= last msg term=
    shift 3
    # a TERM while the check is being launched is held until its pid is known, then acted on
    trap 'term=1' TERM
    rm -f "$out/$label.verdict"
    setsid -w "$@" >"$file" 2>&1 &
    c=$!
    echo "$c" >"$out/$label.pgid"
    trap 'rm -f "$out/$label.pgid"; kill -TERM -- "-$c" 2>/dev/null; wait "$c" 2>/dev/null; exit 143' TERM
    if [ -n "$term" ]; then kill -TERM "$BASHPID"; fi
    wait "$c" || verdict=$fail_word
    rm -f "$out/$label.pgid"
    last=$(tail -n 1 "$file")
    printf '%s %s\n' "$verdict" $((SECONDS - start)) >"$out/$label.verdict"
    msg=$(printf '%-5s %-28s %4ds  %s' "$verdict" "$label" $((SECONDS - start)) "$last")
    if [ "$verdict" != ok ]; then
        msg+=$'\n'$(excerpt "$file")
    fi
    printf '%s\n' "$msg"
}

# A check is "label|script|argument": python3 script.py argument (node ones: label|node/x.mjs|).
# check_cmd ENTRY sets the array CMD.
check_cmd() {
    local script=${1#*|} arg
    arg=${script#*|}
    script=${script%%|*}
    CMD=()
    case $script in
        *.mjs) CMD=(env LOOK_RETUNED_SKIPS=1 node "$here/$script") ;;
        *.sh) CMD=(bash "$here/$script") ;;
        *) CMD=(python3 "$here/$script"); [ -n "$arg" ] && CMD+=("$arg") ;;
    esac
}

# run_pool JOBS ENTRY...: the entries, JOBS at a time, in the order given.
run_pool() {
    local n=$1 entry label
    shift
    for entry in "$@"; do
        while [ "$(jobs -rp | wc -l)" -ge "$n" ]; do wait -n 2>/dev/null; done
        label=${entry%%|*}
        check_cmd "$entry"
        one_check "$label" "$out/$label.out" "$pool_fail" "${CMD[@]}" &
    done
    wait
}

# Seconds a check took in a measured run (the quick gate): the longest start first so
# the long browser checks are not left for the end. A check not listed counts as 60.
declare -A weight=(
    [nightdrive_theme]=570 [signal_theme]=450 [paper]=300 [visualizer]=240 [effects_menu]=160
    [nightdrive_phone]=90 [nightdrive_states]=90 [space_key]=45 [ember_link]=15
    [completeness]=100 [playing]=140 [tone_note]=120 [contract]=150 [frame_button]=140
    [nightdrive_sheet]=175 [arrange]=110 [tone_drag]=80 [core_rows]=75 [computer_core]=45 [computer_ember]=170 [computer_paper]=150 [computer_signal]=170 [sweep]=300
    [notes]=300 [effects_cycle]=200 [scroll]=200 [sources]=150 [smoke_page]=200
    [smoke_core]=200 [haze]=300 [touch]=100 [tone_touch]=100
    [computer_nightdrive]=200
)
by_weight() {
    local e script
    for e in "$@"; do
        script=${e#*|}
        script=${script%%|*}
        printf '%s %s\n' "${weight[${script%.py}]:-60}" "$e"
    done | sort -s -k1,1nr | cut -d' ' -f2-
}

# A retuned look number (node/look-values.mjs) fails node-model alone: the tests written for the
# approved look skip themselves in the other harnesses while it does.
node_entries=()
for name in effects facts model sources notes director keep tone-curve tone-drag playing viz \
    core-store core-stream core-actions core-computer core-prefs core-registry core-climb core-loop core-title core-hold ember-parts \
    level-write toast frame-keys switch ember-order contract \
    paper-tokens paper-words paper-live paper-ruler paper-draw paper-notes paper-fold \
    signal-line signal-draw nightdrive-scene nightdrive-sky nightdrive-hud nightdrive-tune nightdrive-draw nightdrive-ridge nightdrive-floor nightdrive-budget nightdrive-debug nightdrive-glow nightdrive-mood nightdrive-states nightdrive-controls signal-controls; do
    node_entries+=("node-$name|node/$name.mjs|")
done
node_entries+=("gate-refusal|gate_refusal.sh|")

# The checks that read a pace, a calm rate (simulated frames a second against a band) or a time
# series from the page's own frames: they need the machine to themselves, so they run one at a time
# after the parallel part, on both gates. (nightdrive_timing is Night drive's draw cost, its draws and floor
# rows a second and the budget's rate rule; the rest of that theme, nightdrive_theme, runs in the pool. scroll
# holds the smoke's velocity to bounds that depend on how fast the wheel steps and the frames come.)
serial_scripts=(smoke_core smoke_page haze effects_cycle scroll smoke_quick nightdrive_timing)

if [ "$quick" -eq 1 ]; then
    browser_checks=(effects_menu effects_layout tone_note knob_click tone_drag playing \
        device_presets reduced_motion visualizer arrange contract frame_button space_key ember_link core_rows computer_core computer_ember computer_paper computer_signal paper signal_theme nightdrive_theme nightdrive_sheet nightdrive_phone nightdrive_states nightdrive_timing smoke_quick \
        computer_nightdrive)
else
    browser_checks=(smoke_core smoke_page effects_menu effects_layout sources notes sweep haze \
        scroll tone_note knob_click tone_drag playing device_presets reduced_motion visualizer \
        arrange contract frame_button space_key ember_link core_rows computer_core computer_ember computer_paper computer_signal paper signal_theme nightdrive_theme nightdrive_sheet nightdrive_phone nightdrive_states nightdrive_timing effects_cycle \
        computer_nightdrive)
fi
entries=()
serial_entries=()
for browser in chromium firefox; do
    for name in "${browser_checks[@]}"; do
        entry="$name-$browser|$name.py|$browser"
        case " ${serial_scripts[*]} " in
            *" $name "*) serial_entries+=("$entry") ;;
            *) entries+=("$entry") ;;
        esac
    done
done
entries+=("effects_devices-chromium|effects_devices.py|" "completeness-chromium|completeness.py|chromium")
if [ "$quick" -eq 0 ]; then
    entries+=("touch-chromium|touch.py|" "tone_touch-chromium|tone_touch.py|")
fi
entries+=("firefox_preload|firefox_preload.py|")

total=$(( ${#node_entries[@]} + ${#entries[@]} + ${#serial_entries[@]} ))

# The node harnesses and the gate's own refusal check: a second or two, one at a time.
pool_fail=FAIL
run_pool 1 "${node_entries[@]}"

# The browser checks, longest first, JOBS at a time. With more than one job a failure is not
# final: it is run again alone below. With one job the order is the listed one and a failure stands.
if [ "$jobs_n" -gt 1 ]; then
    pool_fail=retry
    mapfile -t ordered < <(by_weight "${entries[@]}")
else
    pool_fail=FAIL
    ordered=("${entries[@]}")
fi
run_pool "$jobs_n" "${ordered[@]}"

# A check that failed beside others is run once more alone: passing is `flaky` (counted as passed;
# both outputs stay, the first in LABEL.out, the second in LABEL.again.out), failing again is FAIL.
flaky=()
for entry in "${ordered[@]}"; do
    label=${entry%%|*}
    word=
    [ -r "$out/$label.verdict" ] && read -r word _ <"$out/$label.verdict"
    if [ "$word" = retry ]; then
        echo "again $label (alone)"
        check_cmd "$entry"
        one_check "$label" "$out/$label.again.out" FAIL "${CMD[@]}" &
        wait $!
        read -r word _ <"$out/$label.verdict"
        if [ "$word" = ok ]; then
            flaky+=("$label")
            echo "flaky $label"
        fi
    fi
done

# The serial lane, one at a time on a quiet machine; a failure stands.
pool_fail=FAIL
if [ ${#serial_entries[@]} -gt 0 ]; then
    run_pool 1 "${serial_entries[@]}"
fi

# The verdict: a check with no verdict file was lost (killed, crashed) and is a FAIL.
failed=0
for entry in "${node_entries[@]}" "${ordered[@]}" "${serial_entries[@]}"; do
    label=${entry%%|*}
    word=
    [ -r "$out/$label.verdict" ] && read -r word _ <"$out/$label.verdict"
    if [ "$word" != ok ]; then
        failed=$((failed + 1))
        if [ -z "$word" ]; then
            echo "FAIL  $label: lost, no verdict (the check's process was killed or crashed)"
        fi
    fi
done

note=
if [ ${#flaky[@]} -gt 0 ]; then
    note=" (${#flaky[@]} flaky: ${flaky[*]})"
fi
echo "web-checks: $((total - failed)) of $total checks passed in $SECONDS s$note" \
    "(output in target/web-checks/out)"
[ "$failed" -eq 0 ]
