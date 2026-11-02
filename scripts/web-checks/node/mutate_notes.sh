#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates copies of smoke-model.js and smoke-notes.js; notes.mjs (pointed at the copies) must fail
# each time. Each list line: file|from|to; a line that ends in a backslash goes on at column 0.
# Run by hand, slow: bash scripts/web-checks/node/mutate_notes.sh
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
W=$ROOT/bins/svolderd/web/themes/ember
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/mn-XXXX")
cp "$H/look-values.mjs" "$D/"  # the harness imports it from beside itself
web=../../../bins/svolderd/web/themes/ember
sed -e "s|$web/smoke-model.js|$D/smoke-model.js|" \
    -e "s|$web/smoke-notes.js|$D/smoke-notes.js|" "$H/notes.mjs" > "$D/notes.mjs"
i=0; killed=0
while IFS='|' read file from to; do
  i=$((i+1))
  cp "$W/smoke-model.js" "$D/smoke-model.js"; cp "$W/smoke-notes.js" "$D/smoke-notes.js"
  python3 - "$D/$file" "$from" "$to" <<'PY'
import sys
s = open(sys.argv[1]).read()
assert sys.argv[2] in s, sys.argv[2]
open(sys.argv[1], 'w').write(s.replace(sys.argv[2], sys.argv[3], 1))
PY
  if node "$D/notes.mjs" >/dev/null 2>&1; then
    echo "SURVIVED: $file: $from -> $to"
  else
    killed=$((killed+1))
    echo "killed:   $file: $from -> $to"
  fi
done <<'LIST'
smoke-model.js|Math.exp(-(now - this.at) / TUNING.agitDecaySecs)|Math.exp(-(now - this.at) / 5)
smoke-model.js|Math.exp(-(now - this.at) / TUNING.agitDecaySecs)|\
Math.exp((now - this.at) / TUNING.agitDecaySecs)
smoke-model.js|if (!(now > this.at)) return;|if (now === this.at) return;
smoke-model.js|if (this.best) this.best.amount *= k;|
smoke-model.js|amount >= this.best.amount|amount <= this.best.amount
smoke-model.js|this.sum += amount;|this.sum = amount;
smoke-model.js|if (!(amount > 0 && Number.isFinite(amount) && Number.isFinite(x)|\
if (!(Number.isFinite(amount) && Number.isFinite(x)
smoke-model.js|&& Number.isFinite(now))) return;|)) return;
smoke-model.js|if (last !== undefined && now - last < this.cooldown) return false;|\
if (last !== undefined && now - last <= this.cooldown) return false;
smoke-model.js|if (last !== undefined && now - last < this.cooldown) return false;|
smoke-model.js|return this.live(now) < this.cap;|return this.live(now) <= this.cap;
smoke-model.js|return this.live(now) < this.cap;|return true;
smoke-model.js|return this.stamps.filter((t) => now - t < this.life).length;|\
return this.stamps.filter((t) => now - t <= this.life).length;
smoke-model.js|this.stamps = this.stamps.filter((t) => now - t < this.life);|this.stamps = [];
smoke-model.js|if (!Number.isFinite(now)) return false;|
smoke-model.js|if (!Number.isFinite(now)) return;|
smoke-model.js|const a = stepAgitation(size);|const a = size;
smoke-model.js|return a > 0 ? Math.max(a, TUNING.agitStepMin) : 0;|return a > 0 ? a : 0;
smoke-model.js|fx: Math.cos(angle) * TUNING.clickPuffPush,|fx: Math.cos(angle) * 99,
smoke-model.js|if (now - this.at > TUNING.burstGapSecs) this.end();|\
if (now - this.at >= TUNING.burstGapSecs) this.end();
smoke-model.js|if (now - this.at > TUNING.burstGapSecs) this.end();|
smoke-model.js|if (direction < 0) {|if (direction <= 0) {
smoke-model.js|if (direction < 0) {|if (false) {
smoke-model.js|const direction = both ? afterDb - beforeDb : share;|const direction = share;
smoke-model.js|if (both) this.rise += afterDb - beforeDb;|if (both) this.rise = afterDb - beforeDb;
smoke-model.js|else this.dbs = false;|
smoke-model.js|if (Number.isFinite(share) && share > 0) this.share += share;|\
if (Number.isFinite(share) && share > 0) this.share = share;
smoke-model.js|return rise > TUNING.bigRiseDb \|\| riseShare|return rise >= TUNING.bigRiseDb \|\| riseShare
smoke-model.js|riseShare > TUNING.bigRiseShare \|\| enteredRed === true;|\
riseShare >= TUNING.bigRiseShare \|\| enteredRed === true;
smoke-model.js|\|\| enteredRed === true;|;
smoke-model.js|rise > TUNING.bigRiseDb \|\||false \|\|
smoke-model.js|riseShare: this.dbs ? 0 : this.share }|riseShare: this.share }
smoke-model.js|agitSpeedFloor: 0.6,|agitSpeedFloor: 2,
smoke-model.js|agitPointer: 3.5,|agitPointer: 2.2,
smoke-model.js|if (value > TUNING.settleFrom) this.armedAt = now;|if (value >= 0) this.armedAt = now;
smoke-model.js|if (value >= TUNING.settleBelow \|\| now - this.armedAt < TUNING.settleSecs) return null;|\
if (now - this.armedAt < TUNING.settleSecs) return null;
smoke-model.js|if (value >= TUNING.settleBelow \|\| now - this.armedAt < TUNING.settleSecs) return null;|\
if (value >= TUNING.settleBelow) return null;
smoke-model.js|this.odds = Math.max(this.odds, odds);|this.odds = odds;
smoke-model.js|this.odds = Math.max(this.odds, odds);|this.odds = Math.min(this.odds || 1, odds);
smoke-model.js|if (!(random() < odds)) return null;|if (!(random() <= odds)) return null;
smoke-model.js|    this.agitation.reset();|
smoke-model.js|    this.armedAt = null;|
smoke-model.js|    this.odds = 0;|
smoke-model.js|const at = this.agitation.where();|const at = { x: 0, y: 0 };
smoke-model.js|settleOdds: 1 / 3,|settleOdds: 1 / 2,
smoke-model.js|clickOdds: 1 / 2,|clickOdds: 1 / 3,
smoke-model.js|bigRiseOdds: 2 / 3,|bigRiseOdds: 3 / 4,
smoke-model.js|burstGapSecs: 1,|burstGapSecs: 2,
smoke-model.js|bigRiseDb: 10,|bigRiseDb: 12,
smoke-model.js|bigRiseShare: 0.125,|bigRiseShare: 0.2,
smoke-model.js|const glyph = Math.min(2, Math.floor(random() * 3));|\
const glyph = Math.floor(random() * 2);
smoke-model.js|const tilt = (random() * 2 - 1) * TUNING.noteTiltDeg;|\
const tilt = random() * TUNING.noteTiltDeg;
smoke-model.js|const tilt = (random() * 2 - 1) * TUNING.noteTiltDeg;|\
const tilt = (random() * 2 - 1) * 2 * TUNING.noteTiltDeg;
smoke-model.js|TUNING.noteSizeMin + span * random()|TUNING.noteSizeMin + span * random() * 2
smoke-model.js|const span = TUNING.noteSizeMax - TUNING.noteSizeMin;|\
const span = TUNING.noteSizeMax;
smoke-model.js|return positive(size) * TUNING.agitStep;|\
return Math.abs(positive(size)) * TUNING.agitStep * 2;
smoke-model.js|const excess = (unit(fraction) - red) / (1 - red);|\
const excess = (unit(fraction) - red);
smoke-model.js|return positive(excess) * positive(dt) * TUNING.agitMeter;|\
return positive(excess) * TUNING.agitMeter;
smoke-model.js|return positive(excess) * positive(dt) * TUNING.agitMeter;|\
return Math.abs(excess) * positive(dt) * TUNING.agitMeter;
smoke-model.js|Math.min(dist / dt / shortSide, TUNING.agitSpeedMax)|dist / dt / shortSide
smoke-model.js|Math.max(0, speed - TUNING.agitSpeedFloor) * dt * TUNING.agitPointer|\
Math.max(0, speed) * dt * TUNING.agitPointer
smoke-model.js|Math.max(0, speed - TUNING.agitSpeedFloor) * dt * TUNING.agitPointer|\
Math.max(0, speed - TUNING.agitSpeedFloor) * TUNING.agitPointer
smoke-model.js|if (!(dt > 0 && shortSide > 0 && dist > 0 && Number.isFinite(dist))) return 0;|\
if (!(dt > 0 && shortSide > 0)) return 0;
smoke-model.js|agitSpeedFloor: 0.6,|agitSpeedFloor: 1.5,
smoke-model.js|agitSpeedMax: 8,|agitSpeedMax: 12,
smoke-model.js|agitPointer: 3.5,|agitPointer: 1,
smoke-model.js|agitMeter: 10,|agitMeter: 3,
smoke-model.js|agitDecaySecs: 0.5,|agitDecaySecs: 0.3,
smoke-model.js|noteGlyphHeight: 0.64,|noteGlyphHeight: 0.4,
smoke-model.js|i < Math.min(limit, fractions.length)|i < fractions.length
smoke-model.js|if (Number.isFinite(f) && (best|if ((best
smoke-model.js|Math.min(Math.max(v, reach), Math.max(reach, extent - reach))|\
Math.min(Math.max(v, reach), extent - reach)
smoke-model.js|Math.min(Math.max(v, reach), Math.max(reach, extent - reach))|\
Math.min(v, Math.max(reach, extent - reach))
smoke-model.js|return height / TUNING.noteGlyphHeight;|return height;
smoke-model.js|return rgb.map((c) => c + (1 - c) * share);|return rgb.map((c) => c * share);
smoke-model.js|return rgb.map((c) => c + (1 - c) * share);|return rgb.map((c) => c + (1 - c));
smoke-model.js|noteCooldown: 4,|noteCooldown: 3,
smoke-model.js|noteLife: 3,|noteLife: 2,
smoke-model.js|noteCap: 2,|noteCap: 3,
smoke-model.js|noteTiltDeg: 25,|noteTiltDeg: 30,
smoke-model.js|noteSizeMin: 0.06,|noteSizeMin: 0.05,
smoke-model.js|noteSizeMax: 0.09,|noteSizeMax: 0.1,
smoke-model.js|agitDecaySecs: 0.5,|agitDecaySecs: 1,
smoke-model.js|noteHalo: 0.25,|noteHalo: 1.5,
smoke-notes.js|const HEAD_TILT = -0.4;|const HEAD_TILT = 0;
smoke-notes.js|    ctx.rect(px(sx), py(g.tops[i]), STEM_W * cell, (hy - g.tops[i]) * cell);|
smoke-notes.js|  if (g.flag) {|  if (false) {
smoke-notes.js|  if (g.beam) {|  if (false) {
smoke-notes.js|    ctx.clip();|    
smoke-notes.js|ctx.shadowColor = 'rgb(0, 255, 0)';|ctx.shadowColor = 'rgb(0, 0, 255)';
smoke-notes.js|    ctx.fillStyle = 'rgb(255, 0, 0)';|    ctx.fillStyle = 'rgb(0, 255, 0)';
smoke-notes.js|    ctx.stroke();|    
smoke-notes.js|[0.29, 0.72], [0.6, 0.64]|[0.29, 0.72]
smoke-notes.js|{ heads: [[0.42, 0.72]], tops: [0.18] },|\
{ heads: [[0.42, 0.72]], tops: [0.18], flag: true },
smoke-notes.js|{ heads: [[0.36, 0.72]], tops: [0.2], flag: true },|\
{ heads: [[0.36, 0.72]], tops: [0.2], flag: true, beam: true },
smoke-notes.js|[0.6, 0.64]|[0.99, 0.64]
smoke-notes.js|    ctx.save();|
smoke-notes.js|ctx.fillRect(0, 0, 3 * cell, cell);|ctx.fillRect(0, 0, 3 * cell, 2 * cell);
smoke-notes.js|ctx.shadowBlur = halo.blur * cell;|ctx.shadowBlur = 0;
smoke-notes.js|ctx.lineWidth = halo.width * cell;|ctx.lineWidth = 0;
smoke-model.js|agitPointer: 3.5,|agitPointer: 35,
smoke-model.js|Math.min(dist / dt / shortSide, TUNING.agitSpeedMax)|Math.min(dist / dt / shortSide, 1.2)
smoke-model.js|settleSecs: 0.8,|settleSecs: 0,
smoke-model.js|settleBelow: 0.25,|settleBelow: 50,
smoke-model.js|agitStepMin: 1.5,|agitStepMin: 0.1,
LIST
echo "$killed of $i note mutations killed"
rm -rf "$D"
