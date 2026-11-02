#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of smoke-model.js; sources.mjs (pointed at the copy) must fail each time.
# Run by hand, slow: bash scripts/web-checks/node/mutate_sources.sh
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
SRC=$ROOT/bins/svolderd/web/themes/ember/smoke-model.js
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/mm-XXXX")
cp "$H/look-values.mjs" "$D/"  # the harness imports it from beside itself
web=../../../bins/svolderd/web/themes/ember
sed "s|$web/smoke-model.js|$D/smoke-model.js|" "$H/sources.mjs" > "$D/sources.mjs"
i=0; killed=0
while IFS='|' read from to; do
  i=$((i+1))
  python3 - "$SRC" "$D/smoke-model.js" "$from" "$to" <<'PY'
import sys
s = open(sys.argv[1]).read()
assert sys.argv[3] in s, sys.argv[3]
open(sys.argv[2], 'w').write(s.replace(sys.argv[3], sys.argv[4], 1))
PY
  if node "$D/sources.mjs" >/dev/null 2>&1; then
    echo "SURVIVED: $from -> $to"
  else
    killed=$((killed+1))
    echo "killed:   $from -> $to"
  fi
done <<'LIST'
Math.min((dist / dt / shortSide) * TUNING.pointerPush, TUNING.maxPush)|\
(dist / dt / shortSide) * TUNING.pointerPush
(dist / dt / shortSide) * TUNING.pointerPush|(dist / shortSide) * TUNING.pointerPush
dx / dist) * push|dy / dist) * push
(dy / dist) * push|(-dy / dist) * push
const dye = pressed ?|const dye = true ?
(dist / shortSide) * TUNING.pointerDyePerSide|(dist / dt / shortSide) * TUNING.pointerDyePerSide
TUNING.pointerDyeMax) : 0|1e9) : 0
if (!(dt > 0 && shortSide > 0|if (!(shortSide > 0
-135 + 270 * unit(at)|-135 + 270 * at
x + radius * Math.sin(a), y: y - radius * Math.cos(a)|\
x + radius * Math.cos(a), y: y - radius * Math.sin(a)
tx: Math.cos(a), ty: Math.sin(a)|tx: -Math.cos(a), ty: Math.sin(a)
tx: Math.cos(a), ty: Math.sin(a)|tx: Math.sin(a), ty: Math.cos(a)
unit(size / TUNING.puffFullSize)|(size / TUNING.puffFullSize)
TUNING.puffFloor + (1 - TUNING.puffFloor) * unit(level)|1
(direction > 0 ? 1 : direction < 0 ? -1 : 0)|1
dye = TUNING.puffDye * share|dye = TUNING.puffDye
TUNING.meterDrift * f + (f > 0 && f >= red|TUNING.meterDrift * f + (f > 0 && f > red
f >= red ? TUNING.meterRedKick : 0|f >= red ? 0 : 0
dye: TUNING.meterDye * f,|dye: TUNING.meterDye,
if (!previous || !next) return null;|if (!next) return null;
raw === before) return null|raw === before + 1000) return null
raw > before ? 1 : -1|raw > before ? -1 : 1
Math.abs(raw - before) / (hi - lo)|Math.abs(raw - before)
Number.isFinite(before) && Number.isFinite(raw) && hi > lo|Number.isFinite(raw) && hi > lo
y: 1 - point.y / size[1]|y: point.y / size[1]
LIST
echo "$killed of $i source mutations killed"
rm -rf "$D"
