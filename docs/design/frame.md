<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The frame

The web page is a frame, the core and a theme. The frame is what the page draws over every theme
and never hands to one: the menu's button, its menu, the picker of themes, the popups, and the
reserved key `T`. A theme is drawn into `#theme`; the frame lives in `#frame`, first in the page and
above the theme by `z-index`. This page says what the frame does, how a theme is chosen and loaded,
and what a theme gives it. The code is `bins/svolderd/web/core/` (`frame.js`, `menu.js`,
`picker.js`, `switch.js`, `frame.css`) and `bins/svolderd/src/web/themes.rs`. What the theme Ember
adds to the frame (its menu rows) is in [themes-and-effects.md](themes-and-effects.md); the page
around it is in [web-ui.md](web-ui.md).

## The button and its guard

The button is one node, drawn by the frame, dressed from the `--frame-*` tokens the theme sets on
`:root` (size, background, colour, border, radius; frame.css has defaults). A theme says where it
sits by anchoring it to an element (`core.frame.anchor(el)`: Ember's empty `.fxslot` beside the link
pill), and the frame places it over that element in page coordinates, again at a resize, a font's
arrival and `realign()`; with no anchor it sits at the top right, 16 px in. A theme may ask it to
fade (`fade(true)`): after 2 s without a pointer move, touch or key it goes to opacity 0, and any of
those brings it back at once; it never fades while the menu or the picker is open.

A theme cannot hide, cover or move the button. Its displayed, visible, clickable, positioned and
opaque declarations are `!important` in a cascade layer declared before every other (`@layer
frame-guard` in frame.css): an important declaration in an earlier layer beats every important
declaration of a later layer or of none, whatever the selectors. `make theme-css-check` (part of
`make check`) fails when a theme's style sheet names `#frame`, `.fxbtn`, `.fxmenu`, `.fxpop`,
`#fx-button`, `#fx-menu` or `#picker`. `effects_menu.py` adds a more specific `!important` sheet and
finds the button shown.

## The menu

The rows are drawn when a theme is mounted and again each time the menu opens (the theme's optional
`menuOpening()` runs first, for work done only for a person who looks), and kept in step by
`realign()`. In order: the **Theme** row (`#menu-theme`, "Theme: Ember ›", with the theme's
picture from `preview(box)`), the rows the theme gives (`menuRows(core)`: headings, radios, checks,
actions and rules, each with the fields `menu.js` describes), a rule, **Device** (`#menu-device`;
disabled with one device, a list of radios `[data-device]` with two or more) and **Sign out**
(`#menu-sign-out`) or, for a guest, **Sign in** (`#menu-sign-in`, `./?sign-in`). The arrows reach
every enabled row round the ends, Home and End jump, Escape and `q` close, and the focus opens on
the checked radio. After a pick the focus goes back to the button.

## The picker and the key `T`

The picker is a modal dialog with a card for each theme the page lists: a picture drawn by the theme
(160 by 104), its name and blurb, the one in use marked. `T` (or `t`, no Ctrl, Alt or Meta, not in
an input, a text area, a select or an editable element) toggles it, in the capture phase on the
window, before any theme sees the key. While it is open the page under it is `inert` and the keys
that move in it are its own; Escape, a click outside or a pick closes it.

## The choice, loading and the fallback

The choice is per browser: `localStorage` and the cookie `svolder_theme` (`<id>` or
`<id>.light` / `<id>.dark`), the cookie so the server paints the first frame in the right theme.
The server's `chosen(headers, query)` picks `?theme=<id>` when it names a shipped theme, else the
cookie's theme, else Ember (the first); a scheme the theme lacks is dropped. The page then keeps a
valid `?theme=`, says `There is no theme “x”.` for one that names nothing (an empty one is ignored),
and makes the rendered theme the choice when the stored one differs.

A theme's files are its style sheets, its meta tags and its scripts. The server lists them for each
theme (`Theme` in `themes.rs`: `styles`, `metas`, `scripts`, one source for the first paint and the
page's list), renders the chosen theme's at first paint, and `switch.js` adds the rest on a switch:
the metas first (a script may read them as it loads), then the sheets and the scripts not already
in the page, each awaited, with a bound of 5 s. Whichever theme ends up mounted has its files in
place first, the fallback included; the other themes' sheets and metas go only after the new theme
is mounted. Scripts are plain files of the page's own origin (the policy has no inline script).
A switch unmounts the theme on show and mounts the new one; a failed load leaves the theme on show
with a popup. A theme that does not load, cannot run or throws while it mounts is replaced by
Ember with the popup `<Label> could not start here; showing Ember` and the choice set to Ember;
when Ember cannot start either, a popup that stays says so, with a way to reload.

## Light, dark and the browser's colours

A theme with both schemes says so (`schemes`), and the core keeps the person's word (Auto, Light or
Dark) per browser. The server paints the chosen scheme in the first byte: the cookie says
`<id>.dark`, the page opens with `data-scheme="dark"` and no flash of the other scheme can come
first. At boot the scheme the server painted is the choice, over whatever is stored; a switch
rewrites the choice from the theme it mounts, never from the one it leaves. The browser's own
colours follow the scheme: a theme names `theme_color` and, for a dark scheme, `theme_color_dark`;
the page carries one `theme-color` meta for each scheme (one, once a scheme is painted), and the
core's `paintChrome` sets `theme-color` and `color-scheme` again on every change of scheme. The
toast's shadow is a token, `--frame-toast-shadow`, so a dark theme sets its own; a test fails when
the frame's sheet names a colour outside a token.

## What a theme gives the frame

`menuButton` (`{ icon: 'waves' | 'ink', label }`), `menuRows(core)`, `menuOpening()` (optional),
`preview(box)`, and the `--frame-*` tokens in its style sheet. It never receives the button or the
menu, and it binds no key `T`.
