<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The web page's visualizer

The card that draws what the computer plays, from the daemon's analysis frames
([host-capture.md](host-capture.md)). It stands beside Now playing on a desktop and under the
volume on a phone; the [web UI](web-ui.md#now-playing) says where it sits and why nothing around it
moves. Our own renderer, written from the looks the user chose and the public documentation of the
options they were chosen from; nothing is vendored and nothing is read from a library's source,
because a strong copyleft licence would then reach the page. It is Canvas 2D, which a phone draws
well enough and the page's CSP already allows.

## Which files do what

- `visualizer.js` is the card's canvas layer and wiring: the stream, the looks, the chip, the
  graph toggle, the delay row, the sink label. It owns the stream, the model and the spectrogram's
  history, not the card's element, so a swap of the screen puts the picture in the new card and the
  stream keeps running.
- `viz-model.js` and `viz-colour.js` are the rules without a canvas (decoding, smoothing, the
  max hold, the clock offset and the delay, the beat schedule, the band maps; the base scale and
  the stereo tint) and are tested in node (`node/viz.mjs`).
- `viz-pref.js` is a classic script in the page's head that reads `on` from the same key before the
  first paint and says `data-viz="off"` on the root element, so the style sheet leaves the card out
  of the layout from the first frame (a module would run after it).
- `visible.js` says whether something is on show: the tab in front (`document.visibilityState`) and
  the element inside the viewport (`IntersectionObserver`). `watchVisible` calls `onVisible` at
  once whenever the answer changes, `onShow` when it first holds and `onHide` once it has not held
  for `GRACE_MS` (2 s). Anything that costs the daemon something while it is open reuses it.
- `PARTS.visualizer` in `themes/ember/draw/parts.js` is the markup (a button holding the canvas, the chip, the
  toggle, the quiet line, the sink label); it has no part of its own, since a swap would drop the
  canvas. It is drawn inside `.p-band` with Now playing, exactly when Now playing is. On a phone
  `visualizer.js` moves the card in the page's order to just after the volume (or the meters), where
  the style sheet already shows it, so a keyboard or a screen reader meets the volume first; the
  window's `resize` moves it back into the band on a desktop width. The style sheet's own `display`
  on the band says which is which, and a locked screen's inert column keeps the card out.

## What costs what

The card costs nothing when nobody looks. The frames stream is opened by `onShow` and closed by
`onHide`, so a hidden tab, or a card scrolled out of the viewport, lets the stream go after 2 s and
the daemon's capture stops 2 s after that; a card shown again inside the grace keeps its stream. The
animation frames are stopped at once when the card is not on show (not after the grace) and are
running only while the capture's state is `running`: with no capture possible, or while it starts,
nothing animates. The page follows `capture` events on an open stream, so a capture that comes back
(`pw-record` started, the sink returned) is drawn without a reload; a stream the browser gave up
on is asked for again while the card is on show, after 3 s, then 6, 12 and up to 30 s, starting over
at a good open.

## The looks

Each look is drawn from the model's smoothed bands (0.4 of the last frame kept per 33 ms, the
reference setting) and the canvas is sized to the box at the screen's pixel ratio (to 3).

- **Spectrogram**: a canvas that is not on the page holds the waterfall, a card's
  width in 12 s. Each draw shifts it left by the time since the last (the canvas drawn onto itself,
  columns put at the right edge), so no history is redrawn; the shift draws with `copy`, since the
  history is semi-transparent and would thicken laid over itself. Rows are frequency on a log
  scale, row 0 the highest; a row between two bands' centres takes a mix of them. Colour comes from
  `viz-colour.js`'s `pixel(l, r)` and opacity from its `alpha(l, r)`: the louder channel's level
  times 4, at most 1, so silence is transparent and a quarter of the scale (quiet music) is fully
  drawn. The history starts transparent, as if silence had played, so the first frames are never
  white.
- **lumiBars**: 240 bars, 60 % of their slot, brightness the level to the power 2.2 (the mapping
  of the chosen mockup, whose levels were on the same 0 to 1 scale), the left channel on a prism
  gradient and the right on a rainbow, added (`lighter`) on a transparent ground. **lumiBars soft**
  (the default) is the same at power 2.0, erased toward transparent over the top and bottom 30 %.
- **The graph**, over any look and on or off: each channel's curve (full alpha, 1.5 px), a fill of
  its own colour at 10 % alpha (plain alpha, so the two never add up to white), and the max hold,
  dotted. With silence the curves lie flat along the foot. Frequency runs along the card, level up.

A tap on the card (the face is a button, so Enter and Space too) or `v` cycles the look, in the
order spectrogram, lumiBars, lumiBars soft, so from the default soft look the next is the
spectrogram; a chip names it for a second. The corner toggle is the graph's, off at first. Both are
kept in `localStorage` under `svolder.viz.2` as `{look?, graph?, hinted}` (every access in
try/catch, the page whole without it): `look` and `graph` are written only once the viewer has
chosen them, and the first time the card runs it says "tap to change the look" on the chip and keeps
only that it was shown, so a default that changes later reaches every browser that never chose. What
is kept wins over the defaults; the number is the layout's version, and the key an earlier card used
(`svolder.viz`) is never read. Reduced motion changes nothing in the card: it draws, and the chip
fades as for anyone.

Why these. The spectrogram's base scale runs between a neutral grey and the page's warm grey, a true
middle between the two, and the tint appears only where the channels differ, so a mono signal reads
as plain grey and stereo as colour; it starts as silence (transparent) and never white. The max hold
lasts 2 s, longer than the 0.5 s of the settings the looks were chosen from, so a peak can be read
before it falls. A tap cycles the looks because that is simpler than a menu, so the looks are not
in the effects menu; the card's own on/off switch is (below).

## The switch

The effects menu's last row is **Visualizer**, a switch under the three levels (a
`menuitemcheckbox`: the arrows reach it, Enter and Space flip it, the menu closes as it does for a
level). The row shows only where the screen has the band the card stands in (a page with no Now
playing has nothing to switch). It is on by default and kept, once chosen, as `on` in
`svolder.viz.2` with the other choices, every access in try/catch and the page whole without
storage (the choice then lasts as long as the page).

Off takes the card out of the page, not out of sight: `syncVisualizer` removes the element (and
keeps it, to put back), releases the watch, closes the frames stream and stops the animation, so
the daemon holds no capture for that page and nothing runs. A screen swap while it is off removes
the new card the same way. On puts the card back in the band and binds it again. The desktop band
is then Now playing alone across its full width at a lower fixed height: 168 px below 1100 px and
188 px from there (the cover grows from 64 to 84 px), the least that holds a changed path's two
rows of chips with the room under them, measured in both browsers
(`SVOLDER_FAKE_ROUTE=changed`); `playing.py` checks every chip inside at 820, 1100 and 1280 px. A
phone has no card at all. The page moves once, by the visualizer's height, when it is switched (the
person's own act), and does not move otherwise: `viz-pref.js` and the style sheet draw a reload
the same way from the first frame. The `v` key does nothing while the card is off, and the key
list leaves its `v` row out.


## When there is nothing to draw

The card keeps its box. With no capture (`unavailable`: no `pw-record`, a container, host audio off,
`SVOLDER_FAKE_CAPTURE=off`) it shows the daemon's reason in one quiet line. When the captured sink
is not this page's DAC (another sink, or an amp page) the daemon's Now playing part says so
(`data-sink-label`, from `Playing::sink_label`) and the card names the sink, `Playing to <sink>`,
since the visualizer still shows what the computer plays. A DAC page whose route is not read yet
names nothing: the sink may be that DAC, and a label that then went would blink once per page open.
A capture that has gone quiet (a suspended sink, which a passive tap hears nothing from) leaves the
capture running: with no frame for a second the curves fall and the waterfall goes on drifting out
to transparent, so a stalled stream falls and never freezes.

## Time

The clock offset is taken from each stream's `hello` (the daemon's clock against the page's
`performance.timeOrigin + performance.now()`), and not corrected by frames: a frame's `t` against
that offset is its delay. Correcting by `t` would hide the steady delay that the page is to show and
plan around, and the offset is taken again on every stream, so it cannot drift for long. The
median of the last 2 s is the Device fold-out's "Visualizer delay N ms" row (`facts.js`
`setVizDelay`, written only while the card runs). A look that moves continuously tolerates a frame
up to about 120 ms behind the sound, and a frame is dropped only when a newer one has arrived. A
beat is scheduled ahead from the tempo and phase, so its pulse lands on time even with a steady
delay; with no tempo, a beat flag more than 120 ms late is not pulsed, since a late pulse is worse
than none. The card itself does not pulse on beats (the effects layer does).

## The box

The card keeps its box whatever the page shows around it. On a desktop it is the right half of the
band from 1100 px, and below that Now playing takes three fifths so that a changed path still
fits; on a phone it is a 180 px card under the volume. With the card switched off Now playing has
the whole band (see the switch above). Under the volume on a phone there is a gap
that the volume keeps for the tone control's note: cutting it would let a note move the card, so
it stays. The card is translucent like Now playing and the tone control's panels (`--glass`, the
card colour at 70 %), so the smoke shows behind it; the canvas paints no ground, and the chip, the
toggle and the sink label keep a small dark backdrop of their own to stay readable over it.

## A page that rests

A page hidden for the 2 s grace closes the frames stream and the device stream both, and a page
shown again opens them. The visualizer forgets the old capture when it rests, so a card shown again
draws nothing until the new stream speaks. The screen of a woken page waits, dimmed and inert as a
lost link's is but with no notice, until the stream's first reading, because a step from the knob
is absolute and would otherwise raise a volume someone lowered while the tab rested. The wait dims
the controls only (the body's `waking` class): the effects stay in colour, since the page is not
lost, and a screen with nothing to read (a card that says the device is not found) is ready at its
first `parts` event rather than waiting. The wait ends after 5 s whatever the stream says, so a
stream that stays silent does not leave the page inert.

## The effects hear it

Each draw hands the effects layer the loudness of the frame (`effects.loudness(model.levels)`) and
asks `model.beatDue(now)`; a beat due is `effects.beat()`. When the drawing stops (the card hidden,
scrolled away, no capture) the visualizer says `effects.music(null)`, and the page is as it was
without frames. So the effects hear frames only while the card is on show, as the stream is only
open then. `effects.js` keeps the volume's level apart from the music's: the glow's level is
`min(1, volume + 0.3 * loudness)` (`glowLevel`, `MUSIC`), the loudest RMS level counting from 0.4
over 0.5 of its scale; the layer carries the class `hearing` meanwhile, which shortens the glow's
fade to 0.3 s.
Smoke gets the same level and a beat puffs from the dial's track at the volume's fraction (where a
step puffs), outward, with a strength of 0.4 to 1 by loudness and no stir: a beat never arms a
note. While music plays only a change of the two decimals the glow can show is written.

## The phone's folded line

Under 820 px Now playing is a line 72 px tall above the volume (the card under the volume is the
visualizer's). It holds the cover, the title and artist, a round 40 px play and pause and a bare
next (a 32 × 44 px tap area; previous waits for the open card), and a small quiet chevron in the
bottom-right corner, where the 3 px progress bar stops short of it. The chevron is the line's
focusable button; a tap on the line opens it too. The widths that shaped it were the user's own: 341
px (Firefox at 125 % text size), 426 px (Chrome) and 784 px (Firefox's desktop mode), where the
title keeps 125, 210 and 568 px and never goes under the buttons.

The folded title is one line that is clipped, not ellipsised: its last 14 px fade (`mask-image`)
from the first paint, with no script, so a title that fits is touched only if it ends in those 14
px. `fitTitle` marks one that runs past its room `.cut`, which only lets its text span move, and
gives it a copy of its text, a span with `aria-hidden="true"` that cannot be selected and stands 40
px behind the text. A cut title loops: `loopPlan` sets the lap, a rest of 2 s with the title at its
start (readable from its first letter), then a linear move of the text and the gap at 30 px a
second, which puts the copy where the text started, so the wrap has no visible jump. The laps repeat
for as long as the line is folded, the page shows and the title is cut: Web Animations on the text
span and the copy, `transform` only, with no easing anywhere and no way back. The motion ignores
`prefers-reduced-motion` like the page's other small motions. It is not stopped by the effects menu
either; opening the card stops it (the title wraps there), and so does a hidden page.

`syncLoop` keeps the loop as the card is, and is idempotent: called again with a loop running for
the text's width it leaves it alone. It runs at the first paint (without waiting for the fonts),
when the part is swapped, when the line folds or opens, when the page hides or shows, when the card
is resized, and when `document.fonts` says a font has loaded (`font-display: swap` changes the
text's width after the first measure). A title that stops being cut (a resize, a font) loses its
copy and rests at its start. A track is its title and artist (the copy is not part of it); the part
carries no track id. A new track starts at its rest. A part swapped in by the stream's parts event
for the same track (a press, a pause) takes the loop up where it was, from the old span's
`currentTime`, so nothing snaps; a whole-screen redraw (a screen switch, a reconnect) starts it at
its rest. A track that comes while the card is open or the page is hidden starts when the line folds
or the page shows. An open card, and a desktop's band, are left whole.

The fake host's title can be set for a look at all this: `SVOLDER_FAKE_TITLE` (see
[the environment](../usage/environment.md)) names the fake track, e.g. one too long for 341 px.

## Principles

**It costs nothing when nobody looks.** A tab in the background neither draws nor holds a capture,
and the daemon is let go of within seconds of the card leaving the screen.

**Nothing on the page moves when the visualizer starts, comes or goes.** The card's box is fixed,
and what changes is what is drawn inside it. The one exception is the person's own switch: turning
the card off or on moves the page once.

**Watching never changes the sound.** The page only listens to frames the daemon made.

## Checks

`scripts/web-checks/visualizer.py` runs in Chromium and Firefox against the fake host: the card
draws, the looks cycle and the chip names them, the graph toggles, the choice survives a reload,
`v` cycles, a hidden page draws nothing at once and closes its stream after 2 s, a card below the
fold opens none, no capture keeps the box, the placement at 820, 1280 and 390 px (and the card's
place in the page's order on a phone and back), nothing moving when it starts, the delay row, an
amp page's sink label, a guest and reduced motion, the effects hearing the frames (the glow's level
over the volume's while the card runs, the smoke's beat puffs, neither with the card hidden or no
capture), a woken page's wait (the glow in colour, a screen with nothing to read ready at its first
event) and the Visualizer switch (in the menu, off removes the card with its loop and its stream,
the band's lower height, one move, the choice across a reload and without storage, on again, a
phone and an amp page). `playing.py` holds the changed path's chips inside the band at both heights
and checks the dots chip; `effects_menu.py` the switch row as a menu item.
`VIZ_SHOTS=<dir>` keeps stills for looking.
