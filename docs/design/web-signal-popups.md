# Signal's palettes and popups

The tokens of the theme are in [web-signal.md](web-signal.md#look), in `style.css` on
`:root[data-theme="signal"]`. This page is the two things built on them: the palettes a viewer
chooses from, and how the frame's popups are dressed.

## Palettes

The menu's *Palette* row (`palette.js`) chooses one of four, kept per browser through `core.prefs`
(`palette`, the id: `blueprint`, `amber`, `phosphor`, `oxblood`, the menu's order; a value that is
none of them is Blueprint, the default, and a stored id is kept whichever it is). The choice is the
`data-palette` attribute of the root (none for Blueprint; Warm amber is `data-palette="amber"`), and
a palette is a rule on `:root[data-theme="signal"][data-palette="<id>"]` that sets the ground,
panel, well, hover, field, track, ink, mid, dim, faint, thin, accent, device, player, computer,
on-accent, tint, scrim, notch, the lamp colours, the grid and the box's fill, glow and series glow;
no other token changes. The hops keep their meaning in every palette (the player, the
computer, the device each in the palette's own hue). The computer's dials keep their warm glow in
every palette: their face and the `sg-w1`..`sg-w3` washes are fixed colours. Phosphor has a green
accent with an amber device; Blueprint's accent and device are one cyan; Oxblood and brass's are one
brass. The contrast sweep (`scripts/web-checks/contrast.py`) runs once per palette; Signal's picture
in the menu and the picker is always Blueprint.

## The popups

The frame's popups (`core/popups/frame.js`) keep their markup; Signal's sheet dresses
them (`fp-*`, the questions' `ask`, the `+` sheet): the box has the accent's edge and a faint grid,
the title (`fp-title`) is a tab on its top edge, dotted stubs (the box's `::before` and `::after`)
lead in and out where the window has room (860 px and wider, not the Virtual speakers tab), the side
list and the Sound tabs are hops on a dotted wire (the one chosen has a lamp, the accent's edge and
a notch pointing at its page), the Devices readings are terminals on a bus with a thin numeral over
a graticule strip, and a tick scale runs along the box's inner bottom edge. On a phone the box fills
the window (the tab hangs inside its top edge), the close button is 44 px and the wire comes in at
the top left. Control edges in a popup (keys, fields, the close button, the pill row) are the thin
colour lifted toward the ink (`--sg-control`), so they stand at 3:1 or more on every surface in every
palette. No animation is added, so reduced motion
changes nothing. The Preamp row of the EQ library's preview carries a small scale (ticks and a needle,
`fpe-mini`, drawn by `core/popups/eq-library/draw.js` in every theme from the frame's tokens); the
chosen side-list entry and Sound tab show their focus ring in the ink, inside the lit box.

