# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Firefox's error collector sees an error thrown by the very first script of a page (BiDi
script.addPreloadScript). Usage: python3 firefox_preload.py"""
import sys
import common

b = common.Firefox()
try:
    b.goto("data:text/html,<script>throw new Error('early')</script><p>x</p>", settle=0.5)
    errs = b.collect_errors()
    ok = any('early' in e for e in errs)
    print('ok  ' if ok else 'FAIL', 'an error thrown by the first script is collected', errs)
    sys.exit(0 if ok else 1)
finally:
    b.close()
