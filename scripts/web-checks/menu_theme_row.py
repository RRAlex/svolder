# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The menu's Theme row ("Theme: <name> ›") in a browser, in every theme, at 1280 and at 360 px:
it sits on one line (its words span one line box, the row is one line's height), the open menu
is as wide as its longest row allows within the viewport, and the page has no sideways scroll.

    python3 scripts/web-checks/menu_theme_row.py [chromium|firefox]

With MENU_ROW_SHOTS=<folder> the open menu of each theme and width is saved there as
`<theme>-<width>-<tag>.png` (tag from MENU_ROW_TAG, `after` by default)."""
import os
import sys
import time

import common
import modal
from modal import check, load

browser = modal.browser
SIZES = [(1280, 900), (360, 740)]
SHOTS = os.environ.get('MENU_ROW_SHOTS')
TAG = os.environ.get('MENU_ROW_TAG', 'after')

# how many lines the row's words make, the row's height, and the menu's reach
MEASURE = """(() => {
  const row = document.getElementById('menu-theme');
  const pop = row.closest('.fxpop');
  // the row's own words (its text nodes and its `›`), not the preview's drawing
  const tops = new Set();
  for (const node of row.childNodes) {
    if (node.nodeType === 1 && !node.classList.contains('mgo')) continue;
    const range = document.createRange();
    range.selectNodeContents(node);
    for (const r of range.getClientRects()) if (r.width > 0) tops.add(Math.round(r.top / 4));
  }
  const box = pop.getBoundingClientRect();
  return { text: row.textContent, lines: tops.size, height: Math.round(row.getBoundingClientRect().height),
           left: Math.round(box.left), right: Math.round(box.right), view: innerWidth,
           scrolls: document.documentElement.scrollWidth > innerWidth + 1,
           clipped: row.scrollWidth > row.clientWidth + 1 || pop.scrollWidth > pop.clientWidth + 1 };
})()"""


def main():
    d = common.Daemon(host='fake')
    try:
        for theme in modal.THEMES:
            b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=SIZES[0])
            try:
                load(b, d, theme)
                for w, h in SIZES:
                    b.resize(w, h)
                    time.sleep(0.5)
                    b.page('(__core.frame.openMenu(), 1)')
                    time.sleep(0.5)
                    m = b.page(MEASURE)
                    if SHOTS:
                        os.makedirs(SHOTS, exist_ok=True)
                        with open(os.path.join(SHOTS, f'{theme}-{w}-{TAG}.png'), 'wb') as out:
                            out.write(b.shot())
                    tag = f'a_theme_row_is_one_line ({theme}, {w} px)'
                    check(f'{tag}: its words are one line and the row is one line tall',
                          m['lines'] == 1 and m['height'] <= 48, m)
                    check(f'{tag}: the menu fits the viewport, nothing clipped, no sideways scroll',
                          m['left'] >= 0 and m['right'] <= m['view'] and not m['clipped'] and not m['scrolls'], m)
                    b.page('(__core.frame.closeMenu(), 1)')
            finally:
                b.close()
    finally:
        d.stop()
    print(f'menu_theme_row.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
