<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Dragging the curve's points

The Response graph's points on the [web page](web-ui.md): what a press, a drag, the wheel and a
pinch do to a band, and why each rule is what it is. The curve's maths is in
[the tone control](tone-control-and-presets.md#the-curve-in-the-page).

A boomy or harsh track sends the listener to the curve: at a desk with a mouse, on the couch with a
phone, they pull a band by ear until it sounds right, without typing a number. The TUI has dragged
points since 0.3.1; the page does since 0.27.0. What the graph shows is what the device gets, and a
phone page still scrolls past the graph.

**Where a point sits.** At its band's own frequency and gain, as in the TUI, and not on the summed
curve: where bands overlap a point may sit off the bright line, but a dragged point stays under the
pointer, which a point riding the sum could not. In Flat the points sit at their own gains too and
can be grabbed (the first move switches the mode to EQ); in Loudness, and while the mode is unread,
they sit at 0 dB and cannot be grabbed. A band not read in full draws and grabs nothing.

**Grabbing.** A press within reach of a point grabs it: 12 CSS px for a mouse, 24 for a finger,
which covers a thumb. Of two points in reach the nearer wins, on a tie the band already picked, then
the lower. A press away from every point grabs nothing, except a right press or a pinch, which takes
the band nearest in log frequency (as the TUI does). A grab picks the band in the table and opens
its editor, as a click on its row does; a press let go where it landed writes nothing. The cursor is
`grab` over a point in reach and `grabbing` during a drag. A guest, a page with writes off and a
lost stream have nothing to grab.

**Moving.** A left drag (or one finger) puts the point where the pointer is: across for the
frequency, up and down for the gain. Each axis stays where the press found it until the pointer has
gone 8 px along it, so a sideways sweep never nudges the gain and an up-and-down pull never nudges
the frequency (the TUI's rule, in pixels instead of cells); once free an axis stays free. The
gain has a sticky 0 dB: while the pointer is within `ZERO_HOLD_PX` (10 CSS px, never more than the
px of 1 dB on the plot, so a short phone plot has no zone of several dB) of the y the plot draws 0
dB at, the gain asked is the band's 0 dB step, the entry of the gain table that is 0 (a table with
none has no zone). One gain step is about 0.2 dB, a few pixels, so landing exactly on 0 dB by
pointer alone needs the right pixel, and putting a band back flat is the common move. The zone
is stateless, the same y always giving the same step, and absolute outside it: the point is under
the pointer again as soon as it leaves, jumping the few steps the zone covered. It is for the
gain of a left drag only: not the frequency, not the Q (a right drag, a pinch, the wheel), and not
until the y axis is free, so a press near the line that goes sideways keeps its gain. As the zone
yields one step the writer's rule holds, a value equal to the last sent is not written again, so
moving inside it writes nothing more. The TUI's drag has the same idea in cells, the whole row the
chart draws 0 dB on standing for 0 dB. The pointer is captured, so the drag goes on outside the
plot, and the map from pixels to values carries on past the edges: a gain below the graph's
-13 dB (the profile allows down to -20) is reached by dragging below the plot, the point then drawn
on the plot's edge as the curve is. A right drag
sideways sets the Q by its travel since the press as a ratio, halving for every 120 px to the right
(wider bell) and doubling for every 120 px to the left; Q's steps are fine and spread over 0.5 to 8,
so a ratio feels even across the range where a step count per pixel would not. The wheel over a
point steps that band's Q one device step a notch, up narrowing and down widening (the TUI's
direction), and away from every point scrolls the page. The wheel reaches 20 px from a point to
begin, wider than a press, as a hand rolling a wheel drifts; the notches that follow, each within
700 ms of the one before, are the burst's: they go on stepping its band wherever the pointer is on
the plot (and the page does not scroll), until 700 ms pass without one, the pointer leaves the plot
a press comes or the stream is lost, so a burst is not lost to a hand that moved a few pixels. Only
a notch that changes the Q renews the burst's hold, so a trackpad's small deltas that never make a
step, or a Q at its end, give the page back 700 ms after the last step. A mouse's notch arrives as a
pixel delta of 53 to 120, by system and browser, so an event of 40 px or more is one step whatever
its size and drops what was left over; smaller deltas (a trackpad's) add up, 40 px to a step; in
lines or pages an event is a step. The script reads `deltaMode` before `deltaY`, since Firefox
reports lines only to a page that has asked. Two fingers pinch the Q: spreading them twice as far
apart halves it. Every value lands on the nearest device step inside the band's window and ranges (a
coordinate that is not a number leaves the step as it was), so what is drawn is exactly what is
written. A burst of wheel notches goes on from the last Q sent, so a fast roll is not held back to
the last answer, for as long as the readings hold a Q the burst knows (one it sent, or the one it
began from, as a late answer is): another Q, from a right drag, a stepper or another front end,
starts it again from that Q. A notch over a band just let go of steps from where the drag left its
Q, and its write goes after the drag's answers, so the drag's last Q cannot land after it.

**What the page shows.** The point, the line, the area, the other points and the peak line with its
words follow the pointer at once, drawn by the page from the values it holds, with no easing
(`tone-motion.js` does not tween during a drag). A label beside the point gives its frequency, gain
and Q in the core's words, above the point and inside the plot. The band table shows only what the
device confirmed, its cells flashing as the answers land: the swaps of the tone part wait during a
drag, so the page writes the dragged band's cells itself from each write's answer. The volume knob's
red zone follows the device's echo, not the drag: the server draws it from readings, and the page
holds no state the device did not give beyond the drag's own held values.

**The swap hold.** While a drag runs, and until every write it sent is answered, `keep.js` holds the
swap of `part-tone` (graph and band table) in every browser, `moveBefore` or not: a swapped graph
would drop the pointer capture and the page's own drawing. The page draws from its held values
meanwhile, so the curve never jumps back to an older echo and forward again. A write from outside
(another front end) lands in the swap that follows the release. The latest swap then lands; after a
drag the device refused, or that lost the stream, the part is read again from the server, so the
page shows the device's reading.

**Writes.** They go out during the drag, so the sound follows the finger: per band parameter one
write in flight and the newest waiting (`newest` of `api.js`, one sender per parameter), and the
daemon's writer replaces a waiting write as for every slider. A waiting value that is the one last
sent for its parameter (the pointer went away and came back) is not written again. The release
writes where the pointer let go if that is not the last value sent. A drag writes the step it
shows, `raw`, from the tables
the screen carries, so the page never converts a number or words one. A drag begun in Flat switches
the mode to EQ with its first write; the page toasts "Mode set to EQ" once per drag and draws the
curve as EQ from the first move, since that is where the mode is going. A refused write toasts its
words once, ends the drag and writes nothing more, and the part is read again.

**Ending.** Releasing the button or lifting the finger ends a drag. `Esc`, a device switch and a
whole-screen swap end it where it is: the held value is written and the band is not put back where
it started (the TUI's behaviour). A lost stream ends it too, and then nothing more is written. A
release the page never heard (a button let go outside the window) is made up for by a pointer move
with no button down, a blur or a hidden page. A drag that has ended keeps only its hold on the swap
until its writes are answered: the pointer and the fingers are done with at once, so a touch or a
wheel on the graph is the page's own again without waiting a Bluetooth round trip, and a new grab
does not wait for the last answer: it begins from where the ended drag left its band (the figure's
readings do not have it yet) and each drag holds the swap once and lets it go once, so the part is
swapped when the last of them is answered. The new drag's writes go after the answers of the
drags before it (an earlier drag never waits for a later one), so the device ends on the newest
drag's value and not on a value an older drag still had waiting. A wheel notch the device refuses
puts the part among the swaps that wait, so it is read again and shows the device's Q.

**Touch.** A finger is read from the touch events, not the pointer's (the mouse path leaves a
`touch` pointer alone). The `touchstart` listener is non-passive and stands on the document beside
the wheel's, since the plot is swapped with its part and `preventDefault` must come before the
browser decides to scroll. A touch that starts within 24 px of a point takes it and is prevented, so
the page neither scrolls nor zooms for it; the touch's later moves go to listeners added to the plot
for the drag and taken off when it ends. Two touches on the plot make a pinch of the band picked or,
with none picked, the one nearest their midpoint; a second finger landing on a held point replaces
the drag by a pinch of that band, its writes still waited for. A touch anywhere else is never
prevented, so the page scrolls as usual. The plot's `touch-action: pan-x pan-y` keeps a pinch on it
from zooming whatever a script does. A pinch whose first finger already began to scroll the page
cannot be stopped, a limit of the platform ([known issues](../known-issues.md)). Touch and mouse
share one session, so the label, the drawing and the swap hold are the same.

**Why it is built so.** The page needs no round trip to draw: the step tables (`#tone-steps`) and
the readings (`data-readings`) come with the screen, and the curve maths is the core's ported to
`tone-curve.js` and held equal to it by a fixture. The gestures are a model fed plain numbers and
fake writes (`tone-drag.js`), so node proves them with no browser; the DOM layer is thin and apart
from the rest of the page so the themes work can lift both. Only what a browser alone can do (real
pointer and touch events, scroll, capture, the swap hold) is left to browser checks. The key list
(`?`) has a line for the gestures.

Not in it: adding or removing bands, changing a band's filter type, moving several bands at once, an
undo history (`Esc` ends a drag where it is), dragging by keyboard (the steppers and typed values
are that route), and the knob's zone following the drag.

[Back to the map](../architecture.md).
