# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Which part of a split browser check runs: `--part logic|timing|all` on its command line (default
all, what a by-hand run does). The logic part holds the cases that read state, geometry and sums;
the timing part holds the cases that read the wall clock, a frame rate or a frame gap and need the
machine to themselves (run-all.sh runs them in its serial lane, labelled NAME_timing-ENGINE).

Import it before reading the command line: the flag is taken out of sys.argv here, so a check reads
its own arguments (the engine, a section) as it always did. A check counts a case with
`parts.counts(timing)` (its `timing=True` argument) and skips a whole stretch of a run with
`parts.part_has('logic')` or `parts.part_has('timing')`."""
import sys

PARTS = ('all', 'logic', 'timing')


def _take_part(argv):
    """Remove `--part X` or `--part=X` from `argv` and return X ('all' when absent)."""
    part = 'all'
    i = 1
    while i < len(argv):
        if argv[i] == '--part' and i + 1 < len(argv):
            part = argv[i + 1]
            del argv[i:i + 2]
        elif argv[i].startswith('--part='):
            part = argv[i].split('=', 1)[1]
            del argv[i]
        else:
            i += 1
    if part not in PARTS:
        sys.exit(f'usage: --part {"|".join(PARTS)} (got {part!r})')
    return part


PART = _take_part(sys.argv)


def part_has(kind):
    """True when the selected part runs the `kind` cases ('logic' or 'timing')."""
    return PART in ('all', kind)


def counts(timing):
    """True when a case of the given kind is counted in the selected part."""
    return part_has('timing' if timing else 'logic')


def part_tag():
    """` --part X` for the summary line of a part run, empty for the whole check."""
    return '' if PART == 'all' else f' --part {PART}'
