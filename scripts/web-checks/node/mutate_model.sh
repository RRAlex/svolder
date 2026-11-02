#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of smoke-model.js; model.mjs (pointed at the copy) must fail each time.
# Run by hand, slow: bash scripts/web-checks/node/mutate_model.sh
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
SRC=$ROOT/bins/svolderd/web/themes/ember/smoke-model.js
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/mm-XXXX")
cp "$H/look-values.mjs" "$D/"  # the harness imports it from beside itself
sed "s|../../../bins/svolderd/web/themes/ember/smoke-model.js|$D/smoke-model.js|" "$H/model.mjs" > "$D/model.mjs"
i=0; killed=0
while IFS='|' read from to; do
  i=$((i+1))
  python3 - "$SRC" "$D/smoke-model.js" "$from" "$to" <<'PY'
import sys
s = open(sys.argv[1]).read()
assert sys.argv[3] in s, sys.argv[3]
open(sys.argv[2], 'w').write(s.replace(sys.argv[3], sys.argv[4], 1))
PY
  [ $? -eq 0 ] || { echo "ANCHOR MISSING: $from"; rm -rf "$D"; exit 2; }
  if node "$D/model.mjs" >/dev/null 2>&1; then
    echo "SURVIVED: $from -> $to"
  else
    killed=$((killed+1))
    echo "killed:   $from -> $to"
  fi
done <<'LIST'
0.22 + 0.78 * unit(level)|0.2 + 0.8 * unit(level)
Math.exp((-4 * dt) / secs)|Math.exp((-2 * dt) / secs)
if (hidden) return 'stop';|
now >= fadedAt) return 'stop'|now > fadedAt + 5) return 'stop'
now - lastStir < TUNING.stirredSecs|now - lastStir < 0
 - TUNING.calmSlack| - 0.05
Math.min(Math.max(elapsedSecs, 0), TUNING.maxDt)|Math.max(elapsedSecs, 0)
1 - Math.exp(-TUNING.restPull * dt)|1 - Math.exp(-TUNING.restPull)
point.x / size[0]|point.x / size[1]
1 - point.y / size[1]|point.y / size[1]
return [-dx / cw, dy / ch]|return [dx / cw, dy / ch]
Math.round(width * scale)|Math.floor(width * scale)
if (!(secs > 0)) return to;|if (!(secs > 0)) return from;
const reach = 3 * radius;|const reach = 2 * radius;
Math.ceil((point.x + reach / aspect) * w)|Math.ceil((point.x + reach / aspect / 2) * w)
Math.max(0, Math.floor((point.y - reach) * h))|Math.floor((point.y - reach) * h)
if (![point.x, point.y, reach, aspect, w, h].every(Number.isFinite)|if (false
return random() < shade.greyShare ? shade.grey : shade.base;|return random() < 0.5 ? shade.grey : shade.base;
let a = seed >>> 0;|let a = 0;
grey: hslToRgb(greyHue, s / 3, l - TUNING.shadeGreyDark),|grey: hslToRgb(greyHue, s / 2, l - TUNING.shadeGreyDark),
grey: hslToRgb(greyHue, s / 3, l - TUNING.shadeGreyDark),|grey: hslToRgb(greyHue, s / 3, l),
Math.min(TUNING.shadeLightMax, l + TUNING.shadeLighter)|l + TUNING.shadeLighter
const greyHue = (((h + between(-TUNING.shadeGreyHue, TUNING.shadeGreyHue)) % 360) + 360) % 360;|const greyHue = h + between(-TUNING.shadeGreyHue, TUNING.shadeGreyHue) + 90;
const h = between(TUNING.shadeHueLo, TUNING.shadeHueHi);|const h = between(TUNING.shadeHueLo, TUNING.shadeHueHi + 20);
&& column.left >= edge && cw - column.right >= edge;|&& column.left >= 0 && cw - column.right >= 0;
weight = sideWeight(u) / sideWeight(src.u);|weight = 1;
(first ? 1 : 0)|0
this.primed = true;|this.primed = false;
Math.min(Math.max(dt, 0), TUNING.hazeMaxStep) : 0;|Math.max(dt, 0) : 0;
if (!usable(size)) return [];|
return 1 - (1 - TUNING.hazeInner) * u;|return 1;
gain: TUNING.hazeEdgeGain|gain: 1
Math.min(edge, extent / 2)|0
p.rgb = sidePick(this.shade, this.random);|p.rgb = this.shade.sides[0];
const angle = p.pt + p.wt * t;|const angle = 0;
Math.min(speed * TUNING.scrollPush, TUNING.maxScrollPush)|speed * TUNING.scrollPush
Math.sign(dy) * push|push
Math.max(0, Math.min(speed, TUNING.agitSpeedMax) - TUNING.agitScrollFloor)|Math.min(speed, TUNING.agitSpeedMax)
const speed = Math.abs(dy) / dt / shortSide;|const speed = Math.abs(dy) / shortSide;
TUNING.wanderRate * Math.sqrt(dt)|TUNING.wanderRate * dt
(2 * random() - 1)|random()
return (((from + step) % turn) + turn) % turn;|return from + step;
Math.sin(angle) * TUNING.restDrift * strength|TUNING.restDrift * strength
Math.cos(angle) * TUNING.restDrift * strength|TUNING.restDrift * strength
return good ? dt : 0;|return good ? dt * 4 : 0;
return good ? dt : 0;|return 0;
if (this.fed && !good) return null;|
const good = Number.isFinite(dt) && dt > 0;|const good = Number.isFinite(dt);
y = ((i % TUNING.hazeSideCount) + 0.5) * slot|y = ch / 2 + 0 * slot
Math.min(src.radius, TUNING.hazeSpreadMargin * cw)|src.radius
sides: [hslToRgb(h - step, s, l), hslToRgb(h + step, s, l)],|sides: [hslToRgb(h - step, s, l), hslToRgb(h - step, s, l)],
sides: [hslToRgb(h - step, s, l), hslToRgb(h + step, s, l)],|sides: [hslToRgb(h - 10, s, l), hslToRgb(h + 10, s, l)],
const step = between(TUNING.shadeStepLo, TUNING.shadeStepHi);|const step = between(5, 10);
dial: [...light],|dial: hslToRgb(h - step, s, l + 10),
dial: [...light],|dial: hslToRgb(h, s, l),
const s = between(TUNING.shadeSatLo, TUNING.shadeSatHi);|const s = between(TUNING.shadeSatLo, TUNING.shadeSatHi + 15);
const s = between(TUNING.shadeSatLo, TUNING.shadeSatHi);|const s = between(TUNING.shadeSatLo - 30, TUNING.shadeSatHi);
return shade.sides[random() < 0.5 ? 0 : 1];|return shade.sides[random() < 0.3 ? 0 : 1];
return shade.sides[random() < 0.5 ? 0 : 1];|return shade.sides[0];
LIST
echo "$killed of $i model mutations killed"
rm -rf "$D"
