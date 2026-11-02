<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The web UI

The daemon's web UI: turning it on, signing in, using it from a phone, its writes and limits, and
the `web` verbs. Back to the [usage index](README.md).

## Web UI

`svolderd` can serve a page for a browser: on this computer, or on a phone on your network. It shows
a device's screen the way the TUI does, with the volume knob, every control the device's profile
declares, the tone control with its presets and band values, the readings and, on an amp, the
meters, and it reads and writes through the same daemon every other front end uses. Everything
under `api/v1/` is private to the page and may change in any release, so a script uses the CLI, the
socket, MQTT or MCP instead. This section is the guide; "web" below is the reference for every
`[web]` key, the refusals, the limits and the `svolder web` commands.

### Turning it on

The page is off unless `config.toml` turns it on; without a `[web]` section, or with
`enabled = false`, the daemon opens no port:

```toml
[web]
enabled = true
```

The defaults are `listen = "127.0.0.1:8738"` (this computer only), `auth = "token"`,
`writes = true`, `guest = "none"`, no `max_volume_db`, no `url`, and empty `hosts` and `origins`.
Run `svolder daemon restart`, or reload the running daemon (SIGHUP) to have it read the section; a
reload that leaves `[web]` alone keeps the listener, its port and the pages open on it, and re-reads
the token, while one that changes it restarts the listener, which closes every open page's live
stream (a page opens a new one in two seconds). `svolder daemon status` says where it stands, on the
line after the MCP server's:

```
web: listening on 127.0.0.1:8738 (token, writes)
```

`read-only` replaces `writes` where `writes = false`, `no auth` replaces `token` for
`auth = "none"`, and `, guest read` ends the parentheses where `guest = "read"`. The line reads
`web: off` while nothing turns the page on, and `web: cannot listen on ADDR: WHY` for an address
that could not be bound or a token that could not be had; the daemon keeps its socket and its
devices either way. `svolder doctor` has a "Web UI" row for the same state. The TUI reaches it all
from Settings, Connect, Web UI (see ["Connect"](tui.md#connect) under "Settings box"): `Enter` opens
a box that turns the page on and off, moves it between this computer and your network, and makes the
link, the pairing code and the new token below.

### Signing in

With `auth = "token"` a browser signs in before it sees anything (unless `guest = "read"`, see
below), by any of three ways:

- **The link.** `svolder web` asks the daemon for a link code and hands the link to `xdg-open`, so
  the browser opens already signed in. The code lasts five minutes and signs in one browser: the
  same link opened a second time lands on the sign-in page and says it has been used. The command
  prints the page's address on standard error, for the visits after that
  (`svolder: opening the web UI in your browser: http://127.0.0.1:8738/`); when `xdg-open` is
  missing or fails, it says why and prints the link on standard output to open yourself.
- **The token.** `svolder web token` prints the token, as `svolder mcp token` does the MCP server's,
  and the sign-in page's Token field takes it (Show uncovers what is typed).
- **A pairing code.** `svolder web pair` prints six digits and says on standard error where to type
  them: the sign-in page's "or pair this browser" field. One code is open at a time, a new one
  replaces it, it signs in one browser within five minutes, and five wrong tries end it.

A link carries a code and never the token. `xdg-open`'s arguments are readable by every local user
while it runs, a terminal's scrollback keeps what it printed, and a QR code can be photographed; a
code is spent the moment it is used and lasts five minutes, where the token is a three-year sign-in.
The code travels in the link's fragment (`#login=…`), which a browser never sends to a server and a
proxy never logs, and the page drops it from the address bar before it spends it. Even so, on a
computer other people log into prefer `svolder web --qr` (below) or `svolder web pair`: the link
`svolder web` opens passes its code to the browser on its command line, which other local users can
read for the second or two before the page spends it.

A browser that signs in keeps the sign-in for three years from the day it signed in, and the cookie
is renewed on every visit, so a page used now and then does not lapse. After the three years the
sign-in page says `Your sign-in on this browser ended after three years. Sign in again.` The
footer's Sign out ends this browser's sign-in only.

`svolder web token --rotate` replaces the token and signs every browser out at once, closing their
live streams and ending every open code; a restart of the daemon signs nobody out, since a sign-in
is a signed cookie and the daemon keeps nothing about it. Open codes survive a change of `[web]`
that restarts the web server, as long as the token is the same; a restart of the daemon ends them.
With `auth = "none"` (loopback only) every browser is signed in and there is nothing to sign in to.

### From a phone

A phone cannot reach `127.0.0.1`. Set `listen = "0.0.0.0:8738"` in `[web]` (or press `←` or `→` on
the TUI box's Reach row, which moves it between this computer and your network and keeps the port),
then run `svolder daemon restart`, or let the TUI's own reload do it. `svolder web --qr` prints the
link as a QR code, then the link itself and `Scan within 5 minutes; it signs in one browser.`; the
phone's camera opens it, signed in. A camera that only previews the QR code can use the link up
before the phone's browser opens it. The TUI's Web UI box notices and shows a fresh link and QR
code with a note, so a second scan works; it does the same when the code ends another way while the
box is open, such as a daemon restart or a token replaced elsewhere. The link `svolder web --qr`
printed is printed once and stays used: run it again for a new one.

The address in the link is `[web] url` when that is set, else the listen address when it names one
host, else, on a wildcard listen, the first up network interface's IPv4 address, skipping loopback
and the interfaces Docker and libvirt make (`docker…`, `br-…`, `veth…`, `virbr…`). On a loopback
listen with no `url` the command refuses: `a phone cannot reach 127.0.0.1; choose your network in
the TUI's Web box, or set [web] listen`. Where no address is found it says
`no network address of this computer was found: set [web] url`.

A phone can also type: `svolder web pair`'s six digits go in the sign-in page's pairing field.
Anyone on the network can use up a pairing code's five tries by sending wrong ones, so when codes
keep ending, use the link or `svolder web --qr`, which nobody else's tries can use up.

The listener speaks plain HTTP. On a listen address off loopback the cookie, the token typed on the
sign-in page and a pairing code cross the network in the clear, so anyone who can watch the network
can copy a sign-in. On a network you do not trust, keep the page on loopback, or put HTTPS in front
of it (see "Behind a proxy"). `svolder doctor` notes a listen address off loopback.

### Guest read

`guest = "read"` lets anyone who reaches the page look without signing in. A guest sees the devices,
their values, the link, faults with their advice, the meters, the strip, the preset names with the
one that is on, and the declared hops with their notes; never a unit id, a passphrase note, a path
or the daemon's version. The Device fold-out is there without the unit id and the passphrase note,
and its Identify button is inert.

Every control is drawn and inert, a touch on one says `Sign in to change this` with a Sign in
button, and every write answers 401. The header's Sign in button opens the sign-in page, and a link
from `svolder web` or `svolder web --qr` signs the guest's browser in from the guest's page. The
default `guest = "none"` shows a visitor the sign-in page and answers the API 401 with nothing in
the body. With `auth = "none"` there are no guests, since every visitor is signed in, and the config
says so (`web.guest = "read" has no effect with web.auth = "none"`).

### Writes and the volume

With `writes = false` the page is a read-only dashboard: every control is inert for everyone, every
write answers 403, and the footer says `Read-only: [web] writes is off`. With the default
`writes = true` a signed-in browser changes what the TUI can, and a volume it writes follows the
rule Home Assistant's slider does (see
["MQTT and Home Assistant"](mqtt.md#mqtt-and-home-assistant)), with nothing asked:

- `max_volume_db` is a cap. A volume above it is set to the cap, whatever the page asked. A cap
  below every step of the device (under -80 dB on a Freya or Freya 2, under -64 dB on a Lyr 5 or
  Saga 2) still lets the page turn the volume down: a lowering lands on the quietest step, and a
  raise from a known volume is refused, naming the cap. A touch on the dial during a climb, or a
  write at the current level, is not a raise either and also lands on the quietest step.
- A volume at or below three quarters of the range is written as asked. A raise above three quarters
  climbs: from below, three quarters of the range is written at once and the volume climbs a tenth
  of the range a second from there; from above, nothing is written at once and the climb starts from
  the reading. The knob shows the climb, a dotted arc from the level to its target, the target tick
  and a note.
- A touch on the dial stops a climb, whoever started it, and so does Mute.
- A volume the device has not reported yet gets at most three quarters of the range first; until a
  reading arrives the knob is inert and reads `–`.
- Unmuting, powering a device on and raising a gain are not guarded here, as on the TUI and in Home
  Assistant (see "Home Assistant's mute, power and gain are not guarded" in `docs/known-issues.md`).

### Behind a proxy

TLS, and a host name of your own, are a reverse proxy's job: put Caddy, nginx or Traefik in front of
the listener and forward to it. Every address the page uses is relative, so the page can sit under a
sub-path as long as the path ends in `/` (`https://svolder.home.lan/amp/`), and the proxy strips the
prefix. Three things must hold:

- The page's live streams are server-sent events, which nginx buffers unless told not to: send
  `X-Accel-Buffering: no`, or set `proxy_buffering off` for the page's location.
- The `Host` header reaches the daemon as the browser sent it (nginx's
  `proxy_set_header Host $host`), or `[web] hosts` names it; a wildcard listen with no `hosts` lets
  any `Host` through. A request carries only a `Host` the daemon answers to (403 otherwise).
- A POST carries the page's own `Origin`, which must match the `Host` the daemon sees, or be listed
  in `origins`: `origins = ["https://svolder.home.lan"]`. A proxy that rewrites `Host` to the
  upstream's address needs it.

Set `[web] url = "https://svolder.home.lan/amp/"` too, so the links `svolder web --qr` and the TUI
make carry the proxy's address, and the cookie goes out `Secure`. HTTPS is also what lets "Add to
Home Screen" open the page in a window of its own: browsers install a standalone app only from a
secure origin, so over plain HTTP the icon opens a browser tab.

### The page

The header's switcher lists the configured devices, each with its family (`DAC`, `amp`, `preamp`); a
click shows that device, and its address, `./?device=NAME`, can be bookmarked. One device shows its
name alone. The link's pill sits at the right. A device that is not reachable shows a card over its
screen: `NAME is out of reach` with `Next attempt in N s.`, a countdown and, for a browser that may
write, a Try now button that ends the wait; `Looking for NAME…` for a device with no link yet in
this daemon's run (a daemon restart included: it shows no knob until the link is back), which
carries the same `Next attempt in N s.`, countdown and Try now while an attempt is scheduled;
`NAME refused its passphrase`, which says to run `svolder doctor`; and, with no device configured,
`No devices yet. svolder setup adds one.` Active faults show as one card, `Fault` or `N faults`,
with a line for each: its name and the profile's advice. It is the same on every device, an amp
with no power button included, and the amp's Power section no longer lists faults.

**Now playing.** When svolderd watches the computer's music player (the default; `SVOLDER_HOST=off`
or `[host] enabled = false` turns it off, and a daemon with no session bus, as in a container, has
nothing to watch) the page shows what is playing, above the volume and the tone control. On a
desktop it is a band across the page under the header, a fixed height: the cover, the title, the
artist, the album and the player's name, ◀◀ ▶/▮▮ ▶▶, a progress bar with the time played and the
length, and the signal path. On a phone it is one compact line above the volume (cover, title,
artist, a round play/pause and a next, a thin bar along its foot, a small arrow in its bottom-right
corner); a tap on it opens the whole card (with previous too) and a second tap folds it. A title too
long for the line fades at its end and scrolls past in a slow loop (a copy of it follows after a
gap), resting two seconds at its start each time; opening the card stops it. It follows the player
that is playing, as the TUI does, and the page counts the time itself, so the bar moves with no
traffic; a seek or a new track is picked up when the player says so.

The cover is the picture the player itself publishes (a file it wrote, or Spotify's own cache);
svolder never asks the network for one. A track with no cover leaves its place empty, and the card
does not move: it has the same size with art, without it, and with no player at all, where the
card is one quiet line, `No player`, with no buttons. The buttons go to the player that is
playing; a guest, and a page with `[web] writes` off, see the card without them.

The **signal path** is one line from the player to the DAC: what the hops add up to first, then
each hop (the player's stream, the computer's sink, the DAC's own input and sample rate). It says
`untouched` only when every hop is known to leave the signal alone: the stream and the sink at one
rate, both at 100 % and not muted, no host EQ, nothing else mixing in, and the rate the DAC reads
equal to the one the computer sends. Otherwise it names what changes it (`resampled 44.1 → 48 kHz`,
`computer volume 54 %`, `host EQ on`, `DAC reads 44.1 kHz`) or what it could not read (`DAC rate not
read`, `DAC input not read`), and when the computer plays to another sink it says so (`the
computer plays to Speakers`). Until the daemon has read the route (the first read is under way, or
`pw-dump` is missing) the line says `route not read`. While the DAC listens to an input the
computer does not feed (Optical, Coaxial, AES or BNC) the line says `the DAC listens to Optical` and
gives no verdict, since the computer's stream is not what the DAC plays. So does a Gungnir 2 or a
Yggdrasil on one USB port while the computer is on the other (`the DAC listens to USB 1 · the
computer is on USB 2`); when both ports read connected, or a reading is missing, the line says
`both USB ports connected` or `USB ports not read` and is never `untouched`. A DAC with no input
choice (the Eitr 2, the Mesh Card) takes USB alone and counts as on USB. It never says more than
the readings show: svolder cannot see the bits. A DAC's page has a path line; an amp's or a
preamp's has none. On a desktop the card's height is fixed, so a path with more to say than fits
ends in a chip of three dots (the verdict first, always; the dots' tooltip lists the hops left
out, and a screen reader still reads them).

**The visualizer.** Beside Now playing (the band's right half on a desktop, a 180 px card under the
volume on a phone, under the meters on an amp's page) the page draws what the computer plays, live,
from the daemon's own analysis of the sound. A tap on the card (or `v` on a desktop) changes the
look: **lumiBars soft** (the default: 240 bars of 1/24 octave, the left channel in a prism and the
right in a rainbow, faded toward the top and the foot), a **spectrogram** (a waterfall drifting
right to left over about 12 s, frequency up the card on a log scale, grey where both channels agree,
tinted cyan where the left is louder and orange where the right is) and **lumiBars** (the same bars
without the fade); a chip names the look for a second, and a tap goes round in that order. The card
and Now playing are translucent like the tone control's panel, so the smoke shows behind them, and
silence draws nothing. The corner button **Graph** shows (it is off at first) or hides the two
channels' curves over any look, each with its **max hold** (the highest level each frequency reached
in the last 2 s, dotted). A look or graph you pick is kept in this browser; until you pick one the
card follows the defaults, and the first time it runs it says `tap to change the look`. It shows
what the computer plays to its default sink; when that is not this device's DAC (another sink, or an
amp's page, which takes no sound from the computer) it says `Playing to <sink>` on the card.

The **Visualizer** switch in the effects menu (below the three levels) turns the card off, and on
again: it is on by default and kept in this browser. Off, the card is gone from the page and the
page asks the daemon for no frames, so nothing is captured for it; on a desktop Now playing then
has the whole width of the band at a lower height, and a phone shows no card. Switching moves the
page once, and nothing else does. A page with no Now playing has no row.

The card costs nothing when nobody looks: the page asks the daemon for frames only while the tab is
in front and the card is on screen, lets the stream go 2 s after it is not (a phone's background
tab, or the card scrolled out of view), and the daemon stops capturing 2 s after that. When no
capture is possible (no `pw-record`, a container, `[host]` off) the card keeps its size and says why
in one quiet line. The Device fold-out shows `Visualizer delay N ms` while the card runs: how far
behind the sound the frames arrive, the median of the last 2 s; up to about 120 ms is not noticed in
the pictures.

While the card runs, the effects follow the music too: the Glow's brightness rises above the
volume's with how loud it plays and falls back when it stops, and the smoke puffs from the knob on
each beat, on time. Both are the volume's alone when the card is hidden, the tab is in the
background or no capture runs, and Off shows neither.

**The Device fold-out.** At the foot of each device's screen a quiet `Device` line opens a fold-out
(closed until you open it; the page remembers, in this browser, whether you left it open, and works
without that). It says the product with its product id, the unit id (not to a guest), the firmware
and the DSP build the device reported (`3 (advertised)` and a dash until it has, the first being
the build the device advertised), the profile it is read through, the link (`connected`,
`disconnected`, then `passphrase refused` or `readings stale` where that holds, joined by ` · `),
the signal strength and the passphrase note. The signal reads `−62 dBm advert, 3 min ago`: where it
was measured and how long ago, and `—` while the daemon has never heard one. Under the facts an
**All readings** table lists every reading the daemon holds, named or not (never the one that echoes
a passphrase change, which carries the code), with its function address, the profile's name for it
(`?` when it has none), its value and how long ago the daemon heard it. The ages count up on the
page. The part is sent again when a fact or a value changes (at most once a second; an amp's meters,
which the page shows live, do not count, so their rows in the table are as they stood when the part
was last sent), and when a reading is heard again with the same value in a later minute, so "Heard"
for a value that does not change is accurate to about a minute; opening the fold-out reads it
afresh, so the table is current when you look.

The **Identify** button (not shown for a device whose profile offers no identify, the Mesh Card)
asks the unit to show itself, to tell one from others. The page answers with a popup: `Identify
sent`; `Just sent — wait 2 s` when one went out within the last two seconds (nothing is sent); or
the daemon's words when the device is down (`device not connected, next attempt in N s`). It is a
write: a guest or a page with `[web] writes` off has it inert, and what the unit does on the command
(a blink, a timed cue) is not verified on hardware yet.

When the page loses its stream the controls dim and the pill says `Reconnecting to svolder…`; when
the stream returns, that notice goes. When a device's link comes back (its `is out of reach` card
goes away) the pill's dot rings once and a toast says `Reconnected to NAME`; a first link, a daemon
reload, or a `Looking for NAME…` or passphrase card in its place shows no toast.

**The volume knob.** Click the ring to set the volume where you click: the step the click's angle
stands for along the dial's sweep, written under the same rule as any other (the cap holds, a click
above three quarters of the range climbs, a click on the step it already holds writes nothing). A
click while a climb is shown only stops it; the next click jumps. A click in the gap at the bottom
of the dial, in the centre, on the readout (its number, unit and the line under it) or out beyond
the ring does nothing, and a press that moves 6 px or more is a drag.

Or drag round the dial, turn the mouse wheel over it, press − and + (a held button repeats after
about 0.4 s, then every 70 ms), or focus it and use `←` `→` `↑` `↓` for one step, Page Up and Page
Down for eight and Home for the lowest. A device with a silence step has a detent there: crossing
between silence and the step above it takes four steps of travel, so a wheel or a drag does not fall
into silence by accident. Mute is the button between − and +.

The dial lights its ticks up to the level and draws the cap's tick, a red zone where a boosting EQ
curve can clip, and, while the volume climbs, the target; a note under it says what a boost peaks
at, in a slot that is kept for it so that its coming moves nothing below. The cursor says how the
dial moves: over its centre it is the up-and-down arrow (the centre drags up and down), over its
ring a pointing hand (a click sets the volume there, a drag turns it and the hand closes while it
does), and, while the volume climbs, the busy arrow over the rest of the page. The wheel steps
either way.

**Messages.** What the page has to say, such as a refusal, a saved change, a lost link or the
climb's words (`Climbing to −1.0 dB, a tenth of the range a second. Touch the dial or press Mute to
stop.`), comes as a popup at the bottom right (centred on a phone), above the strip. Up to four
stack, the newest at the bottom, each leaving after about two seconds, except the climb's: it stays
six seconds from when it was last said, whether the climb ends sooner or runs on, and newer words
replace it in place; a touch on the dial or Mute that stops the climb, or a lost link, takes it away
at once. A popup never moves the page.

**The tone control.** The mode is a three-way switch (Flat, EQ, Loudness), the curve is drawn as the
device would play it (while the mode is Flat, a faint line shows what EQ would do, and stepping a
band switches the mode to EQ with a toast), and under it the band table gives each band's frequency,
gain and Q in the wording every front end uses (`40.4 Hz`, `+1.45 dB`, `0.50`). Choose a band and
its editor shows a stepper for each value, − and + that repeat while held; tapping the value types
one instead, in its unit (`1200 Hz`, `-3 dB`, `0.7`): `Enter` writes it, `Esc` puts the old one
back, and a value the device cannot take is refused with a popup, or set to the nearest step it has.

With a mouse, the curve's points are handles. Press a band's point and drag it: across sets the
frequency, up and down the gain, and the curve, the peak line and a label beside the point (`1200 Hz
· +1.45 dB · 1.40`) follow at once while the device follows the pointer. A drag keeps each direction
still until the pointer has gone a few pixels along it, so a sideways sweep does not change the
gain. The gain also sticks at 0 dB while the pointer is within a few pixels of the 0 dB line, so
a band can be put back flat without hunting for the step; past that it follows the pointer again.
Press with the right button and drag sideways to set the Q (right widens the bell, left
narrows it), or turn the wheel over a point (within about 20 pixels of it): a notch up narrows it by
one step (a trackpad's small movements add up to a step), and while you keep turning the wheel goes
on with that band even if the pointer drifts off the point. Pressing a point also picks its band in
the table. In Flat the points can be dragged too, and the first move switches the mode to EQ; in
Loudness they cannot. A guest, a page with writes off and a lost connection have nothing to drag.
`Esc` ends a drag where it is; the band stays where the pointer last put it. The wheel away from a
point, or after a pause of a moment, scrolls the page as usual.

With a finger the same points are handles: put a finger on a point (within about a thumb's width
of it) and drag, and the page does not scroll while you do. Two fingers on the graph pinch the Q:
spreading them twice as far apart halves it (a wider bell), bringing them together narrows it. A
pinch sets the Q of the band picked in the table, or, with none picked, of the band nearest the
middle of your fingers; a second finger put down while the first holds a point pinches that
point's band. A finger put down anywhere else on the graph scrolls the page as it always did, and
a pinch on the graph does not zoom the page.

In Loudness mode the Loudness button carries a round − on its left and a round + on its right that
lower and raise the strength, and repeat while held; the strength shows as `strength N` under the
label, and the round buttons are described by it for a screen reader. The three buttons are an equal
third of the switch, except where the screen is too narrow for the Loudness button's words: there it
is wider than the other two. Either way it has one width in every mode, so nothing moves when the
mode changes, and the − and + are touch targets as tall as the button and as wide as their ends.
Nothing below the curve moves in any mode.

**Presets.** The list shows the presets that fit the device, the built-ins first, with a lamp on the
one that is on and a line above them saying which (`On: NAME`,
`NAME, edited. Save the bands to keep them.`, `Mode Flat: no preset is on.` or
`No preset matches these bands.`).

A tap recalls a preset, whichever front end recalled it last; Save as preset names the device's
present bands (at most 40 characters), and the pencil and bin on your own presets rename and delete
them, asking before the delete (`The bands on the device stay as they are.`); one of those forms is
open at a time, and a preset recalled from the keyboard keeps the focus. A name already taken, or
one a preset cannot have, is refused with a popup. Where `config.toml` cannot be written, the list
says `Saving presets is off: WHY` and keeps recalling.

**A Loki Max's presets.** A device that keeps presets of its own (the Loki Max) has a Presets list
of its own where a DAC has the tone presets: its three slots, `Preset 1` to `Preset 3`, then the
built-in settings under `Built in`, with a lamp on the setting the six bands read (none while a band
has not reported, and none on a slot: the device does not say which one it holds). A tap on a slot
recalls it, and the device then reports its bands; a tap on a built-in sets the bands to it, the
cuts first. A popup says what was sent (`Recalled Preset 2`, `Bass Boost on`). The page asks nothing
before a recall or a built-in setting, as it asks nothing before a band is moved.

With `[web] writes` on and a signed-in browser, each slot also has a save icon. It opens a question
in place, `Save the bands to Preset 2? What the device holds there is replaced.`, with Save and
Keep: Keep closes it and sends nothing, and the daemon refuses a save whose request does not say it
was confirmed. A guest, or a page with `writes = false`, sees no save icon and its buttons do
nothing. The question stays put while the page is updated, and one question is open at a time.

**An amp** has the meters instead of the knob: one arch of segments per meter on its own scale, with
a line that says why there are no readings (standby, warming up, meters off, no link), and the power
button above its switches and readouts.

**The strip** along the bottom is one bar a second of what the device reports, the last few minutes
and growing: a DAC's volume in its range, tinted by the sample rate and red where the curve could
clip, an amp's louder meter, with a mark where the input, the sample rate or the S/PDIF lock changed
or a preset was recalled; a second without a link has no bar.

**Keys** on a desktop, none while a text field has focus or with Ctrl, Alt or Meta held, and the
arrows on a focused control belonging to that control: `←` `→` change the volume one step, `m`
mutes, `0` centres the balance, `f` sets the tone control to Flat, `p` focuses the preset that is
on, `v` changes the visualizer's look (a guest has it too; it does nothing with the visualizer
switched off), `A` arranges the sections (below), `T` opens the theme picker (every theme has it,
a guest too) and `?` shows and hides the list, which also says how the curve is dragged. A guest,
and a page with `[web] writes` off, have `v`, `A`, `T` and `?` only, and `?` lists just those; `v`
is listed only where the screen has a visualizer card. The footer lists the keys at desktop width.
The list closes by itself after 6 seconds, and at once on any key, click, scroll, wheel or touch.

**Arrange sections** (`A`, or *Arrange sections…* in the menu) is Ember's: it moves the screen's
sections between the band across the top, the left column and the right column. An accent bar
reads *Arranging Ember* with *Reset to default* and *Done*; each section shows a dashed outline
and a ⠿ handle. Drag a section by its handle with a mouse or a finger, or focus a handle and use
the arrow keys (up and down within a zone, left and right between zones); a line shows where a
dragged section lands. The dial, the curve and the controls of the sections are inert until you
finish (the status card, with its buttons, and the Device fold-out stay as they are, and live). A
device whose link is lost cannot be arranged until it is back. *Done* keeps the order, *Reset to
default* puts Ember's own back (kept by *Done*) and Escape puts back the order you started with.
The order is kept in this browser, per device kind (a DAC's is not an amp's), and on a phone it
reads band, left, right. The status card and the Device fold-out stay where they are.

**The menu** is the round button just left of the link's pill (at the top right on a screen with
no pill), on every screen and for a guest too. It opens a small list: first the **Theme** row (the
theme's name, with a small picture; it opens the picker), then the theme's own rows (Ember's are
headed *Ember's effects*: the three levels below, the one that shows checked, and under them, when
the screen has a Now playing card, a **Visualizer** switch, then **Arrange sections…**), then
**Device** (the device shown; with two or more it lists them to pick one) and **Sign out**, or
**Sign in** for a guest. A click or a tap picks a row and closes the list, and Escape, `q`, a
click outside or the button again close it. By keyboard: `Tab` reaches the button first, `Enter`
or `Space` opens it, the arrows move between the rows (round the ends, skipping any that is
disabled), `Enter` or `Space` picks, and the focus returns to the button. The **picker** (the
Theme row, or `T` from any theme) shows a card for each theme, the one in use marked; Escape or a
click outside closes it, and a pick of another theme changes the page to it. Each browser keeps
its own choice, and `?theme=ember` in the address always gets Ember back. See [the
frame](../design/frame.md).

**Paper** is the page's second theme: the same device drawn as a sheet of squared paper, with the
words in print, the controls written out and a few handwritten notes. Pick it from the menu's
*Theme* row or with `T`, or open the page with `?theme=paper` (`?theme=paper.light` or
`?theme=paper.dark` also chooses the paper). Its menu rows, headed *Paper*, are *Auto* (follows the
system), *Light* and *Dark*; the choice is kept in this browser, and the page opens in the paper
you chose from the first moment, with no flash of the other one. `?theme=ember` always gets Ember
back.

- **Written controls.** The volume is a ruler with a pencil needle: drag the needle, tap a place on
  the ruler, use the arrow keys on it (Page Up and Page Down ten steps, Home the bottom) or tap
  *− quieter*, *mute* and *louder +* under it. The needle stops at the volume cap. Every other
  control is a word: *an underlined word acts*. A choice's current value has a red pencil
  underline and the other values a faint dotted one; a word that opens a list (the tone mode, a
  preset, the slot to save over) shows a small list of options; a slider is a printed scale you
  drag or move with the arrows. The EQ's curve has a round point for each band to drag, and a table
  of every band's frequency, gain and Q with a *−* and a *+* to tap. Saving over a slot on the
  device, and deleting a preset, ask a question in the page's own words first; the safe answer,
  *keep*, is the one with the focus.
- **Notes.** The short lines in blue handwriting say what the numbers mean: what the EQ is doing,
  the sample rates along the signal path (*rate matches all the way*, or the rates with an arrow
  between them), the track's time and album, the firmware, the link, an amp's power readings, and
  the last change. At most one note is red, the most urgent: the device is out of reach or
  faulted, then a volume risk or cap, then the EQ's clipping peak, a path that changes the sound,
  the passphrase note. The last change reads like *last change: −23.5 dB to −20 dB, 14:32*, and
  adds *from this phone* or *from this browser* only for a change this page asked for in the last
  three seconds. Several fields changed together tell one. On a wide screen the notes sit in a band
  under the columns. The EQ's note and the red note each have an arrow to their section, and when
  both are about the tone only the red note's arrow is drawn; the red note is also circled. On a
  phone each note follows its section.
- **A guest, or a page with writes off,** sees the same words with no underline, and a tap says
  *Sign in to change this*.
- **A lost connection** reads *Reconnecting to svolder…* under the name and fades the sheet; a
  device out of reach is a red note with *try now*.

Paper has no effects, no visualizer and no Arrange mode; it ignores the page's effects setting.
See [Paper](../design/web-paper.md).

- **Smoke** (the default): live smoke behind the whole page, a small fluid simulation drawn on the
  graphics chip (WebGL2). A faint haze fills the window behind everything, denser in the margins
  beside the content column (on a narrow window or a phone, along the window's own edges, more
  lightly), and drifts slowly in every direction; it does not change with the volume.

  A cloud spreads out from the knob of a DAC, or from between the meters of an amp, wandering slowly
  in every direction and not rising in a plume, bigger and brighter as the volume or the music is
  louder. The smoke is a palette of three related cool tones (blue to violet): the haze beside the
  content mixes two of them and the knob's or the meters' cloud is the third, drawn anew each time
  the page loads and kept until it is reloaded. It greys and settles to a stop while the device is
  out of reach or the page has lost its link to svolder.
- **Glow**: a soft violet glow behind the whole page, turning slowly; it fades out at its own
  edges, not at the content column's.
- **Off**: neither; the page's background alone.

The level is remembered by this browser (in its local storage) and is changed only by you, from the
menu; the page never changes it. A browser that refuses to store anything still switches levels for
as long as the page stays open. Where nothing is remembered, the level is Smoke, or Glow when the
system asks for reduced motion (its setting is read when the page loads) or when the browser cannot
run Smoke. A level you picked is kept even then: you can pick Smoke under reduced motion and it
runs, with the notes below behaving as they do elsewhere.

**What stirs the smoke.** The pointer, or a finger on the screen, pushes it along the way it moves
(a drag also leaves a little dye, and a drag on the knob stirs it as well); the page never takes the
pointer for it, so nothing you press behaves differently. A volume step, from this page or any front
end (the knob, a key, the TUI), puffs a little smoke at the knob's dot, pushed along the dial in the
direction of the step, a bigger puff for a bigger step; a climb of many steps gives a stream rather
than one burst.

An amp's meters each give off smoke from their own place, more as the meter is louder and with a
push of their own once it is red. Scrolling the page stirs it too: the smoke already drawn moves
with the page, and the scroll adds a broad push over the visible page along the way the page moves,
stronger for a faster scroll up to a limit and gentler than a flick of the finger; a page too short
to scroll does nothing. The first reading after the page loads, a device is switched to or the link
comes back is not a step. A hidden tab runs nothing.

**The notes.** A faint music note (a quaver, a pair of quavers or a crotchet) sometimes forms in the
smoke where you have been stirring it, tilted either way, and the flow carries it off and dissolves
it. A note is carved out of the smoke, not added to it: a gap in the smoke round the note's shape.
It forms where the stir was when the page shows smoke there that is dense enough to carve; otherwise
at the nearest such spot anywhere in the window, and when there is none, no note forms. They come in
three ways, all of them at random and rare:

- *As a stir settles.* After the pointer or a finger (an ordinary stir of it, half a second of
  moving the mouse about is enough), a scroll, a volume step or a meter's peak into its red zone has
  stirred the smoke and then dies down, a note may form where it happened: about one settle in
  three. Never while the stir is still going, and never for an untouched page.
- *When a control is clicked.* A click on a button, a switch, a choice, a preset, a tone-control
  mode or a slider on the screen may form one near the click as its small puff settles: about one
  click in two. The device tabs and the effects menu in the header do not count.
- *After a big volume rise.* A volume rise of more than 10 dB over one burst of steps (a climb
  included), or one that takes the volume into the dial's red zone, forms a note more often than an
  ordinary stir: about two settles in three. A dial that has no dB counts a rise of an eighth of its
  range.

They share one gate: at least four seconds apart and never more than two showing. None form on Glow
or Off, or while the device is out of reach, and a stir still settling when the link drops, the
device goes away or the level changes forms none afterwards.

**When Smoke is not offered.** Smoke needs WebGL2 with a floating-point colour buffer. A browser
without them shows Glow in its place, keeps your choice, and greys Smoke out in the menu with the
reason as its title (`This browser cannot draw the smoke (no WebGL2)` or
`(no half-float rendering)`; `Smoke could not start here` when it failed to start). When the browser
resets its graphics while the smoke runs, the page shows Glow for the rest of its life and says so
once (`Smoke stopped: the browser's graphics were reset; showing the glow`); reload the page to try
again.

`?effects=smoke`, `?effects=glow` or `?effects=off` on the page's address sets the level for that
view alone, stores nothing and does not stop the menu from working; it is how the README's pictures
of the page are kept to Glow. An unknown value is ignored.

The glow and the smoke are centred on the knob of a DAC and on the meters of an amp, and follow them
as the page scrolls or the window changes size.

The system's reduced-motion setting is read once: it makes Glow the first default in place of Smoke,
until you pick a level. The small motions (the dial's glide, the meters' fall, the tone curve's
easing, the glow's turn, the title's loop) run whatever it says; the effects menu's Off level stills
the rest of the page, and the title's loop stops only when the Now playing card opens. It is built
for Firefox 121 and Chromium 120 or later.

## web

svolderd serves the web UI on a port of its own, `127.0.0.1:8738` by default. The
`[web]` table is off until enabled ("Web UI" above is the guide):

```toml
[web]
enabled       = false                   # no listener at all unless true
listen        = "127.0.0.1:8738"        # a socket address; loopback by default
auth          = "token"                 # "token", or "none" for a loopback listen address only
writes        = true                    # false makes a read-only dashboard
# max_volume_db = -10.0                 # optional cap, in dB, on a volume the page sets
guest         = "none"                  # "read" lets anyone who reaches the page look
# url         = "https://svolder.home.lan/"   # the address links and QR codes carry
hosts         = []                      # extra Host names clients may use, e.g. a proxy's
origins       = []                      # extra Origins a proxy presents
# token_file  = "/run/secrets/web"      # optional; read the token from this file
```

These are config errors, each naming its key:
`web.listen "X" must be an address and port, such as "127.0.0.1:8738"`;
`web.auth = "none" needs a loopback listen address`; `web.guest "X" must be "none" or "read"`;
`web.max_volume_db must be a finite number`; `web.url "X" must be an http:// or https:// address
with no query or fragment, such as "https://svolder.home.lan/"` (an address without a final `/` gets
one); `web.hosts entry "X" must be a host name or address, with an optional port (such as
"svolder.home.lan" or "svolder.home.lan:8738")`;
`web.origins: "null" would admit any sandboxed page`; and `web.origins: "X" must be a scheme and a
host, with a port where it is not the scheme's own, such as "https://svolder.home.lan"`.

An origin is `http` or `https`, the host and, where it is not the scheme's own (80, 443), the port;
unlike `[mcp] origins`, a port is not required. Two warnings, printed by `svolder` on standard error
and logged by the daemon, say what cannot work and change nothing: `web.guest = "read" has no effect
with web.auth = "none": every visitor is signed in`, and `[web] listen A and [mcp] listen B share
port P; the second to start cannot listen` (both sections on, on one port, on addresses that
overlap).

The token comes from the first of these that is set: the `SVOLDER_WEB_TOKEN` environment variable
(when not empty), the file `token_file` names, and a file svolder generates,
`$XDG_STATE_HOME/svolder/web-token` (`~/.local/state/svolder/web-token`; 32 random bytes, base64url
without padding, 43 characters, mode 0600). It is a different token from the MCP server's. A token
file, generated or named, that group or others can read is refused with its mode named, and with the
environment variable or `token_file` set svolder generates nothing. It is never logged and never
shown by `config show` or `daemon status`.

The server follows a reload like the MCP server: an unchanged `[web]` keeps its port and reads the
token again, a changed one restarts the listener, `enabled = false` closes the port. An address that
cannot be bound, or a token that cannot be had, is logged without the token and shown by
`svolder daemon status` as `web: cannot listen on ADDR: WHY`; the daemon keeps serving its socket
and its devices, and the next reload tries again. The preset writes of the page and the TUI's Web UI
box send the reload, so a section a hand edit changed meanwhile takes effect then.

**Every request** meets, in order:

- A `Host` header the server answers to, else `403` with an empty body. The list is the address it
  is bound to (with its port; on a loopback address also `localhost`, `127.0.0.1` and `[::1]`; every
  `Host` where `listen` is `0.0.0.0` or `::` and `hosts` is empty) plus `hosts`, where an entry
  without a port matches any port and one with a port that port only. With `hosts` set, a wildcard
  listen answers to those names and the loopback names only.
- For a POST, one `Origin` header, an `http` or `https` origin whose host and port are the request's
  own `Host`, or one of `origins`; none, `null`, a second `Origin` or any other is `403` with an
  empty body. A GET is not checked for an `Origin`.
- For a POST, a `Content-Type` of `application/json`, else `415`
  (`a request's body is JSON (Content-Type: application/json)`), so a cross-site HTML form cannot
  send one; and a body of at most 16 KiB, else `413`; a body that does not arrive in 5 seconds is
  `408`.

A request head over 16 KiB, or one that takes more than 5 seconds to arrive (and, on a connection
kept alive, the next one), is dropped by the shared accept loop, which holds at most 64 connections
at once and closes one past that as it is accepted.

Every answer, a refusal included, carries `X-Content-Type-Options: nosniff`,
`Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`, `Cross-Origin-Opener-Policy: same-origin`,
`Cross-Origin-Resource-Policy: same-origin` and `Cache-Control: no-store` (the files under `assets/`
say `no-cache` and carry an `ETag`, answered `304` to a matching `If-None-Match`); an HTML answer
adds a `Content-Security-Policy` that allows nothing but the page's own files: no inline script, no
inline style, nothing from another origin, no framing. The page's fonts, scripts and stylesheet are
served by the daemon, so a page loads nothing from elsewhere.

**The cookie** is `svolder_web_<port>`, named for the port the server is bound to, since browsers
share cookies between ports of one host: `v1.<login seconds>.<mac>`, an HMAC-SHA256 of the login
time keyed by the token. It is set `HttpOnly; SameSite=Strict; Max-Age=34560000` (400 days, renewed
by every page load with the same login time), with `Secure` where `[web] url` starts `https://` or a
proxy sent `X-Forwarded-Proto: https`, and with no `Path` or `Domain`, so a browser scopes it to the
directory the page is mounted at. A cookie whose login is more than three years old is refused, and
the sign-in page says so. Nothing is stored on the server: a rotated token invalidates every cookie,
and a restart invalidates none.

**The endpoints** are private; they change with the page, and a script has no business with them.
Their status codes, for anyone reading a log or a proxy's trace: `GET /` is the page, or the sign-in
page for a browser that is not signed in (unless `guest = "read"`); `assets/FILE` and
`manifest.webmanifest` need no sign-in, and an unknown file or path is `404` with an empty body;
`GET api/v1/session` answers `{"signed_in":…,"guest":…,"writes":…}`.

`POST login` takes `{"token":"…"}` or `{"link":"…"}` (`400` for neither or both, `401` for a wrong
token or for a link code that has been used or has ended) and `POST pair` takes `{"code":"…"}`
(`401` for a wrong code, saying how many tries are left, or for one that has ended), each answering
`200` with the cookie; `POST logout` answers `200` and clears it. Reads under
`api/v1/devices/DEVICE/` (`screen`, `parts/PART`, `model`, `strip`, `events`, the server-sent
stream), `api/v1/fleet` and `api/v1/host` answer `401` with an empty body to a visitor who is
neither signed in nor a guest, and `404` for an unknown device or part.

Writes (`controls/KEY`, `volume`, `volume/stop`, `connect`, `eq/mode`, `eq/bands/N`, `eq/loudness`,
`presets`, `presets/recall`, `presets/rename`, `presets/delete`, `device-presets/recall`,
`device-presets/save`, `device-presets/apply`, `hops`, `hops/edit`, `hops/remove`, `hops/move`)
answer `401` to a guest
(`Sign in to change this`) or an unsigned visitor, `403` while `writes = false`
(`Read-only: [web] writes is off`), `400` for a body that does not parse or a control the page
cannot change, `404` for an unknown device, control, band or preset, `409` with the words of a
refusal (the daemon's, a volume rule's, a configuration file that cannot be written), `422` for a
value that is not a number in its unit, a name a preset or a hop cannot have or a hop that cannot
take that edit, and `204` for a write a newer one replaced. Every error body is `{"error":"…"}`.

A write that lands answers JSON with `parts`, the parts of the device's model it changed as they
stand after it (a control: `controls` and, on an amp, `power`; the mode, a band and the loudness:
`tone`; a preset's recall: `tone` and `presets`; a preset's save, rename and delete: `presets`; a
slot's recall or save and a built-in setting: `device_presets`), and where the page says something
about the write, `toast`, its words (`Mode set to EQ`, `Recalled Preset 2`, `Saved to Preset 2`,
`Bass Boost on`, `Bass Boost is already on`).

The hop writes edit the configuration file as `svolder hops` does and answer `{"hops": [...]}`, the
hops as the fleet's model lists them. `hops` takes `{"name", "kind", "note"?, "after"?}` for a
declared hop or `{"device", "note"?, "after"?}` for a configured device, and puts it last;
`hops/edit` takes `{"id", "name"?, "kind"?, "note"?, "after"?}`, a key left out keeping what the hop
has, `"note": null` taking its note away and `"after": ""` taking its `after` away (`null` or a missing
`after` keeps it); a hop that links a device keeps its name and kind;
`hops/remove` takes `{"id"}`; `hops/move` takes `{"id", "to"}`, counted from 0. A configuration file
that cannot be written is a `409` that starts `Saving hops is off:`, the writer's refusals are a
`422` and an id no hop carries is a `404`.

`device-presets/recall` takes `{"slot": N}` and `device-presets/save` takes `{"slot": N,
"confirm": true}`, each sent as the device's recall or save command of that slot in order behind the
writes in flight; a slot the device lacks is a `422` (`a slot is 1 to 3`), and a save without
`"confirm": true`, or with anything else as `confirm`, is a `422` that sends nothing.
`device-presets/apply` takes `{"name": "Bass Boost"}` and applies that built-in setting as one batch
(a name the device does not list is a `409` that lists its settings); all three are a `404` on a
device with no presets of its own, and answer the part for the page or `{"slot","name"}` and
`{"name","writes"}` as JSON. A slot is no control, so `controls/recall` and `controls/save` are a
`404`.

`eq/bands/N` takes `{"param": "f"|"g"|"q", …}` and exactly one of `"step"` (one step either way),
`"value"` (a number, or text in the parameter's unit) and `"raw"` (the device's step itself, 0 to
127; another number is a `422`, `a step is 0 to 127`, and a step a band has no value for a `409`,
`that step has no value`); more or fewer than one is a `400`.

### web

Opens the web UI in this computer's browser, signed in, or with `--qr` prints the link for a phone.
It asks the running daemon for a link code (it needs the daemon, which serves the page, so
`--transport` does not apply), builds the link from the page's address and the code, and either
hands it to `xdg-open` or paints it.

Without `--qr` it says on standard error `svolder: opening the web UI in your browser: ADDRESS`,
where ADDRESS is the listen address with a wildcard one read as loopback; when `xdg-open` cannot be
run or cannot open the browser it says why, `svolder: WHY; open this link within 5 minutes, once:`,
and prints the link on standard output. With `--qr` it prints the QR code in half-block characters,
the link, and `Scan within 5 minutes; it signs in one browser.`, and the address in it is the
phone's (see "From a phone"). With no daemon, or a daemon whose web UI is off, it exits 1 saying so
and naming the fix:

```
svolder: no daemon answers on /run/user/1000/svolder.sock: the daemon serves the web UI; turn it on in the TUI's Settings, Connect, Web UI, or set [web] enabled = true and run svolder daemon restart
```

```
Open the web UI, or print a link, a code or its token

Usage: svolder web [OPTIONS]
       svolder web <COMMAND>

Commands:
  pair   Print a six-digit code that signs in one browser within five minutes
  token  Print the token the web UI's sign-in page takes, and nothing else
  help   Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --qr                     Print the link as a QR code for a phone, in place of opening it here
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### web pair

Prints a six-digit pairing code on standard output, and on standard error where to type it and how
long it lasts, with the way round a code that has been used up:

```
svolder: type it within 5 minutes on the web UI's sign-in page, under "or pair this browser": http://192.0.2.10:8738/; if it stops working, `svolder web` opens a link and `svolder web --qr` shows one for a phone, and neither can be used up by anyone else's tries
```

One code is open at a time and a new one replaces it; it signs in one browser and five wrong tries
end it, which anyone who can reach the page can do, hence the fallback the message names. The
address is the phone's where one can be worked out, else this computer's. It gives the same errors
as `svolder web` where there is no web UI to ask.

```
Print a six-digit code that signs in one browser within five minutes

Usage: svolder web pair [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### web token

Prints the token the sign-in page's Token field takes and nothing else on standard output, so
`$(svolder web token)` is the token; messages go to standard error.

It behaves as ["mcp token"](mcp.md#mcp-token) does for the MCP server, over its own token
(`SVOLDER_WEB_TOKEN`, `[web] token_file`, `$XDG_STATE_HOME/svolder/web-token`): the first call
creates the generated file, `--rotate` replaces it, prints the new one and sends a running daemon a
reload, with the same three notes on standard error
(`svolder: the daemon now accepts only the new token`,
`svolder: no daemon is running, so none uses the new token yet`, `svolder: the daemon did not reload
(WHY): it still accepts the old token until you run svolder daemon restart`), the same refusals for
a token from the environment variable or `token_file` or from another token than the daemon's, and
the same warning when the daemon uses another token than the one printed. Rotating also signs every
browser out and ends every open code.

```
Print the token the web UI's sign-in page takes, and nothing else

Usage: svolder web token [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --rotate                 Replace the generated token with a fresh one before printing it
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```
