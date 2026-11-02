#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of smoke-director.js; director.mjs (pointed at the copy) must fail each time.
# Run by hand, one worker: bash scripts/web-checks/node/mutate_director.sh
# (No mutant for `|| push === 0` (scrollStir gives 0 only for dy === 0, which the floor catches), for
# the away guards of noteAt and pointer (away resets the watch and stirs ignored while away are
# forgotten on return): they are equivalent.)
# A line is `from @@ to`; `\n` in either is a newline. Each `from` must occur in the file.
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
SRC=$ROOT/bins/svolderd/web/themes/ember/smoke-director.js
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/md-XXXX")
cp "$H/look-values.mjs" "$D/"  # the harness imports it from beside itself
web=../../../bins/svolderd/web/themes/ember
sed "s|$web/smoke-director.js|$D/smoke-director.js|; s|$web/smoke-model.js|$ROOT/bins/svolderd/web/themes/ember/smoke-model.js|" \
  "$H/director.mjs" > "$D/director.mjs"
ln -s "$ROOT/bins/svolderd/web/themes/ember/smoke-model.js" "$D/smoke-model.js"
cp "$SRC" "$D/smoke-director.js"  # the unmutated copy must pass, or every kill is a crash
node "$D/director.mjs" >/dev/null 2>&1 || { echo "BASELINE FAILS"; rm -rf "$D"; exit 2; }
i=0; killed=0
while IFS= read -r line; do
  from=${line%% @@ *}; to=${line#* @@ }
  i=$((i+1))
  python3 - "$SRC" "$D/smoke-director.js" "$from" "$to" <<'PY'
import sys
a, b = (x.replace('\\n', '\n') for x in sys.argv[3:5])
s = open(sys.argv[1]).read()
assert a in s, a
open(sys.argv[2], 'w').write(s.replace(a, b, 1))
PY
  [ $? -eq 0 ] || { echo "ANCHOR MISSING: $from"; rm -rf "$D"; exit 2; }
  if node "$D/director.mjs" >/dev/null 2>&1; then
    echo "SURVIVED: $from -> $to"
  else
    killed=$((killed+1))
    echo "killed:   $from -> $to"
  fi
done <<'LIST'
if (this.idle() || !Number.isFinite(now) @@ if (!Number.isFinite(now)
this.fadedAt = next ? t + TUNING.fadeSecs : null; @@ this.fadedAt = null;
if (now < this.fadeUntil) { @@ if (false) {
this.fadeUntil = now + TUNING.hazeFadeSecs; @@ this.fadeUntil = now + 100;
const dt = frameDt(now - this.last); @@ const dt = now - this.last;
    if (this.isVisible) this.last = this.time(now); @@     if (this.isVisible) this.last = this.time(now);\n    else this.watch.reset();
if (this.isLost) this.watch.reset(); @@ 
    this.watch.reset();\n    this.fadedAt @@     this.fadedAt
this.scrollV = 0;\n      return []; @@ return [];
if (!this.gate.allows(now)) return null; @@ 
this.gate.record(now); @@ 
const odds = big ? TUNING.bigRiseOdds : TUNING.settleOdds; @@ const odds = TUNING.settleOdds;
t, TUNING.clickOdds); @@ t, 1);
if (this.isAway || !this.canNote) return null; @@ if (this.isAway) return null;
if (this.isAway || !this.anchorAt || !(this.anchorAt.radius > 0)) return false; @@ if (this.isAway) return false;
if (this.puffQueue.length > TUNING.maxPuffs) { @@ if (false) {
this.random() * 2 * Math.PI), rgb @@ Math.random() * 2 * Math.PI), rgb
v <= MAX_SIDE @@ true
plan.shift = by; @@ plan.shift = null;
if (dy !== 0 && !this.isAway) this.lastStir = t; @@ 
if (!s.rest) this.lastStir = now; @@ 
if (this.pointerQueue.length > 0) {\n      this.lastStir = t; @@ if (this.pointerQueue.length > 0) {\n      this.lastStir = -Infinity;
this.lastStir = t;\n    return true;\n  }\n\n  /** A beat of the music @@ return true;\n  }\n\n  /** A beat of the music
if (this.hazeOn) { @@ if (true) {
this.hazeClock.step(dt) @@ this.hazeClock.step(dt + 1)
const peak = peakOf(this.meterTargets, meters.length); @@ const peak = null;
rest: !(this.meterShown[i] >= TUNING.meterRed), @@ rest: true,
this.puffQueue = [];\n      this.scrollDy @@ this.scrollDy
this.trimPuffs();\n    this.lastStir = t; @@ this.trimPuffs();
TUNING.clickOdds);\n    this.lastStir = t; @@ TUNING.clickOdds);
this.hazeOn = Boolean(on); @@ this.hazeOn = Boolean(on);\n    this.watch.reset();
if (this.isLost || this.isAway || !Number.isFinite(x) @@ if (this.isLost || !Number.isFinite(x)
this.watch.stir(agit, place.x, place.y, now, TUNING.settleOdds); @@ void agit;
this.watch.stir(TUNING.clickAgit, x, y, t, TUNING.clickOdds); @@ this.watch.stir(TUNING.clickAgit, x, y, t, TUNING.settleOdds);
for (const h of fed) { @@ for (const h of fed) {\n      this.watch.stir(2, h.x, h.y, now, 1);
meterAgitation(peak.fraction, TUNING.meterRed, dt) @@ meterAgitation(peak.fraction, 0, dt)
peakOf(this.meterTargets, meters.length) @@ peakOf(this.meterShown, meters.length)
this.watch.stir(stepStir(size), dot.x, dot.y, t, odds); @@ void odds;
}), here.x, here.y, t, TUNING.settleOdds); @@ }), here.x, here.y, t, 0.01);
const spot = this.noteSpot(notePosition(at, cell * 0.71, size), cell, size); @@ const spot = this.noteSpot(notePosition({ x: 1000, y: 100 }, cell * 0.71, size), cell, size);
if (this.isAway || !this.anchorAt || !(this.anchorAt.radius > 0)) return false; @@ if (!this.anchorAt || !(this.anchorAt.radius > 0)) return false;
add: [p.fx * share, -p.fy * share, 0, 0], @@ add: [p.fx * share, p.fy * share, 0, 0],
const dot = dialDot(this.anchorAt, at); @@ const dot = dialDot(this.anchorAt, 0.5);
dense = (p) => read(p.x, p.y, cell / 2) >= TUNING.noteDenseMin; @@ dense = (p) => read(p.x, p.y, cell / 2) >= 0;
dense = (p) => read(p.x, p.y, cell / 2) >= TUNING.noteDenseMin; @@ dense = (p) => read(p.x, p.y, cell) >= TUNING.noteDenseMin;
lattice.sort((a, b) => a.d - b.d); @@ lattice.sort((a, b) => b.d - a.d);
return lattice.find((p) => dense(p) && shown(p)) ?? null; @@ return lattice.find((p) => dense(p) && shown(p)) ?? place;
return lattice.find((p) => dense(p) && shown(p)) ?? null; @@ return lattice.find((p) => shown(p)) ?? null;
return lattice.find((p) => dense(p) && shown(p)) ?? null; @@ return lattice.find((p) => dense(p)) ?? null;
const step = cell * TUNING.noteFindStep; @@ const step = cell * 4;
const read = this.density?.(); @@ const read = null;
return !this.open || this.open(p.x, p.y, half) === true; @@ return !this.open || this.open(p.x, p.y, half * 4) === true;
return !this.open || this.open(p.x, p.y, half) === true; @@ return true;
lattice.push({ x: place.x + i * step, y: place.y + j * step, d: Math.hypot(i, j) }); @@ lattice.push({ x: place.x + i * step, y: place.y + j * step, d: Math.abs(i) + Math.abs(j) });
dt, this.shade.dial, @@ dt, this.shade.base,
if (!spot) return null; @@ if (!spot) { this.gate.record(now); return null; }
LIST
echo "$killed of $i director mutations killed"
rm -rf "$D"
