#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Check the files named on the command line: each carries an SPDX licence line and a copyright
# line in its first three lines, a `#!` line is the first line, and an executable file starts
# with one. Prints each failing list and exits 1 when any is not empty; prints nothing otherwise.

missing=
displaced=
unshebanged=
for f in "$@"; do
    head -n 3 "$f" | grep -q 'SPDX-License-Identifier: GPL-3.0-or-later' &&
        head -n 3 "$f" | grep -q 'SPDX-FileCopyrightText:' ||
        missing="$missing  $f
"
    if tail -n +2 "$f" | head -n 2 | grep -q '^#!'; then
        displaced="$displaced  $f
"
    fi
    if [ -x "$f" ] && [ "$(head -c 2 "$f")" != '#!' ]; then
        unshebanged="$unshebanged  $f
"
    fi
done

status=0
if [ -n "$missing" ]; then
    echo "spdx-check: files without an SPDX-License-Identifier: GPL-3.0-or-later line and an SPDX-FileCopyrightText line in their first three lines:"
    printf '%s' "$missing"
    status=1
fi
if [ -n "$displaced" ]; then
    echo "spdx-check: files whose #! line is not the first line:"
    printf '%s' "$displaced"
    status=1
fi
if [ -n "$unshebanged" ]; then
    echo "spdx-check: executable files without a #! first line:"
    printf '%s' "$unshebanged"
    status=1
fi
exit "$status"
