#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# `make theme-css-check`: no style sheet of a theme (bins/svolderd/web/themes/*/*.css) names the
# frame's own parts. The frame draws the menu button, the menu, the picker and the layer they sit
# in (core/frame.css); a theme dresses them only through the `--frame-*` tokens. A theme's rule on
# `#frame`, `.fxmenu`, `.fxbtn`, `.fxpop`, `#fx-button`, `#fx-menu` or `#picker` could hide or move
# what must always be reachable.

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
THEMES = ROOT / "bins" / "svolderd" / "web" / "themes"
NAMES = re.compile(r"#frame\b|#fx-button\b|#fx-menu\b|#picker\b|\.fxbtn\b|\.fxmenu\b|\.fxpop\b")
COMMENT = re.compile(r"/\*.*?\*/", re.S)


def problems(path):
    text = path.read_text()
    # Blank out the comments but keep the line numbers.
    text = COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)
    for number, line in enumerate(text.splitlines(), 1):
        for found in NAMES.finditer(line):
            yield number, found.group(0)


def main():
    bad = 0
    for sheet in sorted(THEMES.glob("*/*.css")):
        for number, name in problems(sheet):
            print(f"{sheet.relative_to(ROOT)}:{number}: names the frame's {name}; "
                  "dress it with the --frame-* tokens")
            bad += 1
    if bad:
        sys.exit(1)
    print("theme-css-check: no theme sheet names the frame")


if __name__ == "__main__":
    main()
