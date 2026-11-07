#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of the theme Night drive's sources (the camera, the axis, the music's levels, the sky, the ear, the
# look, the budget, the floor's drawing, the tuning, the HUD's words and the dash); Night drive's node harnesses
# (nightdrive-*.mjs, run from the copy's own tree, so they import the mutated files) must fail each time (a harness
# that fails on the mutated copy while the unmutated copy passes is a kill: the harnesses are deterministic, with no
# browser and no timing). Run by hand, slow: bash scripts/web-checks/node/mutate_nightdrive.sh
# A line is `name|file|from|to`; lines of one name are one mutation (all its edits at once).
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/mn-XXXX")
# the copy goes on every way out: the end, an error, an interrupt
trap 'rm -rf "$D"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# the harnesses sit where they sit in the repo, so their relative imports reach the copy of the web dir
mkdir -p "$D/scripts/web-checks/node" "$D/bins/svolderd"
cp "$H"/nightdrive-*.mjs "$D/scripts/web-checks/node/"
ln -s "$H/fixtures" "$D/scripts/web-checks/node/fixtures"
cp -r "$ROOT/bins/svolderd/web" "$D/bins/svolderd/web"
P=$D/bins/svolderd/web/themes/nightdrive
i=0; killed=0; names=()
while IFS='|' read -r name file from to; do
  case " ${names[*]} " in *" $name "*) ;; *) names+=("$name");; esac
  echo "$name|$file|$from|$to" >> "$D/list"
done <<'LIST'
the camera's K is 12 again|camera.js|K: 8, spread: 0.24|K: 12, spread: 0.24
a band's column is off by one|axis.js|Math.floor(axisX(bandCentreHz(i), 1) * COLUMNS)),|Math.floor(axisX(bandCentreHz(i), 1) * COLUMNS) + 1),
the ridge reads over -72 dB|music.js|(72 * v - 12) / 60|(72 * v) / 72
the rows ignore the tempo|music.js|export const rowRate = (bpm) => (bpm ? Math.min(6, Math.max(2, (2 * bpm) / 60)) : 3.5);|export const rowRate = () => 3.5;
the rays come every 6 s|sky.js|raysGap: 60,|raysGap: 6,
a star comes every half second|sky.js|starGap: 5,|starGap: 0.5,
a shower comes every 4 s|sky.js|showerGap: 40,|showerGap: 4,
the highs' average is taken per frame|sky.js|avg += (highs - avg) * (1 - Math.exp(-dt / SKY.starAvg));|avg += (highs - avg) * 0.1;
the level's easing is taken per frame|sky.js|b.env += (level - b.env) * (1 - Math.exp(-dt / SKY.envTau));|b.env += (level - b.env) * 0.05;
the diamonds use the closed form|tune.js|if (Math.abs(gainDb) < FLAT_DB) {|if (true) {
the grace is 0|tune.js|GRACE_MS = 1500;|GRACE_MS = 0;
the EQ line drops the preset's name|hud.js|const preset = device.presets?.status?.name;|const preset = null;
muted reads -0.0|words.js|silence: '−∞',|silence: '−0.0',
four inputs are drawn as chips|draw/dash.js|CHIPS_MAX = 3;|CHIPS_MAX = 4;
the ear's moment comes every second|mood.js|momentGap: 15,|momentGap: 1,
the embers rise in Calm|look.js|motion === 'full' && !off.has('noembers')|motion !== 'still' && !off.has('noembers')
the wet road comes back with no window after an idle drop|budget.js|if (idle && !wet && !droppedIdle) {|if (idle && !wet) {
the heat's curve is linear|draw/road.js|heatCurve: 0.6,|heatCurve: 1,
the near edge's lift does not fade|draw/road.js|const fade = GY[k] <= geo.h ? 1 : Math.max(0, 1 - (GY[k] - geo.h) / span);|const fade = 1;
the wet road's slices sway each by their own phase|draw/mood.js|const wob = sway * w|const wob = Math.sin(t * 1.6 + k * 0.8) * w
the fade waits a minute|hud-wire.js|FADE_AFTER_S = 15;|FADE_AFTER_S = 60;
the ridge is smoothed over 0.6 of a band|ridge.js|sigma: 0.87,|sigma: 0.6,
a tap adds below half a morph|tune-wire.js|if (morph.m < 0.5) {|if (morph.m < -1) {
the computer's EQ off is not dimmed|tune-wire.js|const ghosted = (tone) => onComputer() && tone?.mode === 'off' && !turnedOn;|const ghosted = () => false;
a host event leaves the cached computer tone|tune-wire.js|for (const event of ['model', 'device', 'host', 'fleet']) live.onReading(event, forget);|
a drag with the EQ off keeps the dim|tune-wire.js|if (!entry.wrote && computerNow()?.mode === 'off') turnedOn = true;|
a probe does not lift the cap|budget.js|if (liftCap && capped|if (false && capped
a probe misses slow draws|budget.js|recent.length >= BUDGET.probeDraws &&|false &&
a probe misses seldom draws|budget.js|const seldom = inSpan <|const seldom = false && inSpan <
the kept tier is ignored|budget.js|let step = start?.step ?? 0;|let step = 0;
the kept cap is ignored|budget.js|let capped = start ? Boolean(start.capped) : coarse;|let capped = coarse;
the touch band is taller than the curve window|tune.js|const BAND = { up: WINDOW.base + WINDOW.span, down: WINDOW.span - WINDOW.base, margin: 26 };|const BAND = { up: 0.32, down: 0.1, margin: 26 };
the touch band has no margin|tune.js|const BAND = { up: WINDOW.base + WINDOW.span, down: WINDOW.span - WINDOW.base, margin: 26 };|const BAND = { up: WINDOW.base + WINDOW.span, down: WINDOW.span - WINDOW.base, margin: 0 };
a gain beyond the window is drawn beyond it|tune.js|base - (Math.max(-gMax, Math.min(gMax, db)) * span) / gMax|base - (db * span) / gMax
the lone point loses its remove|draw/bands.js|  const remove = button('point-remove'|  const remove = n > 1 && button('point-remove'
LIST
total=${#names[@]}
# the copy itself must pass, or a failure below proves nothing
for h in "$D"/scripts/web-checks/node/nightdrive-*.mjs; do
  node "$h" >/dev/null 2>&1 || { echo "the unmutated copy fails $(basename "$h")"; exit 2; }
done
for name in "${names[@]}"; do
  rm -rf "$D/orig"; mkdir "$D/orig"; mkdir -p "$D/orig/draw"; cp "$P"/*.js "$D/orig/"; cp "$P"/draw/*.js "$D/orig/draw/"
  while IFS='|' read -r n file from to; do
    [ "$n" = "$name" ] || continue
    python3 - "$P/$file" "$from" "$to" <<'PY'
import sys
s = open(sys.argv[1]).read()
assert s.count(sys.argv[2]) == 1, sys.argv[2]
open(sys.argv[1], 'w').write(s.replace(sys.argv[2], sys.argv[3], 1))
PY
  done < "$D/list"
  failed=0; where=
  for h in "$D"/scripts/web-checks/node/nightdrive-*.mjs; do
    if ! out=$(node "$h" 2>&1); then
      failed=1
      # the failing test: the harness's own line in the stack, with the last test that passed before it
      where=$(printf '%s\n' "$out" | grep -o 'nightdrive-[a-z]*\.mjs:[0-9]*' | head -n 1)
      break
    fi
  done
  cp "$D"/orig/*.js "$P/"; cp "$D"/orig/draw/*.js "$P/draw/"
  if [ "$failed" -eq 1 ]; then
    killed=$((killed+1)); echo "killed:   $name (${where:-?})"
  else
    echo "SURVIVED: $name"
  fi
done
echo "$killed of $total Night drive source mutations killed"
[ "$killed" -eq "$total" ]
