#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of the core's computer writes (core/actions.js); core-computer.mjs (run from the
# copy's own tree, so it imports the mutated file) must fail each time. The harness is deterministic,
# with no browser and no timing. Run by hand: bash scripts/web-checks/node/mutate_computer.sh
# A line is `name|file|from|to`.
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/mc-XXXX")
mkdir -p "$D/scripts/web-checks/node" "$D/bins/svolderd"
cp "$H"/core-computer.mjs "$D/scripts/web-checks/node/"
ln -s "$H/fixtures" "$D/scripts/web-checks/node/fixtures"
cp -r "$ROOT/bins/svolderd/web" "$D/bins/svolderd/web"
P=$D/bins/svolderd/web/core
i=0; killed=0; names=()
while IFS='|' read -r name file from to; do
  names+=("$name")
  echo "$name|$file|$from|$to" >> "$D/list"
done <<'LIST'
a point set drops its slot from the body|actions.js|slot === undefined ? { ...body, done } : { ...body, done, slot }|{ ...body, done }
a point remove drops its slot from the body|actions.js|slot === undefined ? {} : { slot }|{}
a point set sends its place as the slot|actions.js|{ ...body, done, slot }|{ ...body, done, slot: at }
a 409 for a moved point does not end the drag|actions.js|ended: Boolean(result.moved) && !done,|ended: false,
a 409 for a moved point is toasted in the daemon's words|actions.js|const said = moved ? POINT_MOVED : why;|const said = why;
a remove of a point that moved is toasted in the daemon's words|actions.js|movedPoint(answer) ? { ...answer, error: POINT_MOVED } : answer|answer
LIST
total=${#names[@]}
node "$D/scripts/web-checks/node/core-computer.mjs" >/dev/null 2>&1 || { echo "the unmutated copy fails core-computer.mjs"; rm -rf "$D"; exit 2; }
for name in "${names[@]}"; do
  cp "$P/actions.js" "$D/orig.js"
  while IFS='|' read -r n file from to; do
    [ "$n" = "$name" ] || continue
    python3 - "$P/$file" "$from" "$to" <<'PY'
import sys
s = open(sys.argv[1]).read()
assert s.count(sys.argv[2]) == 1, sys.argv[2]
open(sys.argv[1], 'w').write(s.replace(sys.argv[2], sys.argv[3], 1))
PY
  done < "$D/list"
  if out=$(node "$D/scripts/web-checks/node/core-computer.mjs" 2>&1); then
    echo "SURVIVED: $name"
  else
    killed=$((killed+1)); echo "killed:   $name ($(printf '%s\n' "$out" | grep -o 'core-computer\.mjs:[0-9]*' | head -n 1))"
  fi
  cp "$D/orig.js" "$P/actions.js"
done
echo "$killed of $total computer write mutations killed"
rm -rf "$D"
[ "$killed" -eq "$total" ]
