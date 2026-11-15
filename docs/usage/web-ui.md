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

The page is off until something turns it on; without a `[web]` section, or with
`enabled = false`, the daemon opens no port. `svolder web on` turns it on (and `svolder web off`
off again, see ["web on, web off"](#web-on-web-off)), which writes this to `config.toml`:

```toml
[web]
enabled = true
```

`SVOLDER_WEB_ENABLED=1` in the daemon's environment turns it on where the file has no `enabled`
key, for a container's compose file; an `enabled` in the file wins.

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

### In Docker

The compose file gives the container the host's network, and the image listens on every interface by
default (`SVOLDER_WEB_LISTEN=0.0.0.0:8738`), so the only steps are turning it on (`docker exec
svolder svolder web on`, or `SVOLDER_WEB_ENABLED: "1"` in the compose file's `environment:`, or
`enabled = true` under `[web]` in `config.toml` under `config/svolder` and `docker compose restart
svolder`) and the sign-in, `docker exec svolder svolder web token`. `svolder web --qr` and `web
pair` carry the host's LAN address. Only a container on a bridge network (no `network_mode: host`)
needs `url` in the file, or `SVOLDER_WEB_URL` in the container's environment, the address your phone
reaches, e.g. `http://<host-ip>:8738`, and says so when it is missing. A `listen` in the file still
wins. See ["The web UI in the container"](daemon.md#the-web-ui-in-the-container) under "The daemon".

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
or the daemon's version. A guest opens the Devices popup read-only (see Devices): the device list,
the Overview, each device's facts without its unit id, its signal, key graphs and All readings, and
nothing that changes a device.

Every control is drawn and inert, a touch on one says `Sign in to change this` with a Sign in
button, and every write answers 401. The header's Sign in button opens the sign-in page, and a link
from `svolder web` or `svolder web --qr` signs the guest's browser in from the guest's page. The
default `guest = "none"` shows a visitor the sign-in page and answers the API 401 with nothing in
the body. With `auth = "none"` there are no guests, since every visitor is signed in, and the config
says so (`web.guest = "read" has no effect with web.auth = "none"`). `svolder web guest on` and
`svolder web guest off` set the key and apply it to a running daemon (see
["web guest on, web guest off"](#web-guest-on-web-guest-off)). The TUI's Web UI box
has the same switch, a Guest view row (`g` is its key), and, while the view is on, draws a second QR
code of the plain page beside the sign-in one for a phone to open (see the TUI's ["The Web UI
box"](tui.md#the-web-ui-box)).

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
with no power button included, and the amp's Power section lists none.

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
read`, `DAC input not read`), and when the computer plays to another sink it says so in
short (`→ Speakers`, or `player → Speakers` for a player routed elsewhere; the whole sentence, `the
computer plays to Speakers`, is the line's tooltip). Until the daemon has read the route (the first
read is under way, or `pw-dump` is missing) the line says `route not read`. While the DAC listens to an input the
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
in one quiet line. Ember's Device fold-out shows `Visualizer delay N ms` while the card runs: how
far behind the sound the frames arrive, the median of the last 2 s; up to about 120 ms is not
noticed in the pictures.

While the card runs, the effects follow the music too: the Glow's brightness rises above the
volume's with how loud it plays and falls back when it stops, and the smoke puffs from the knob on
each beat, on time. Both are the volume's alone when the card is hidden, the tab is in the
background or no capture runs, and Off shows neither.

**A device's facts.** At the foot of each device's screen (Ember's quiet `Device` fold-out, Paper's
written word, Signal's and Night drive's button) stands *Device settings…*, which reads *Devices…*
for a viewer who may not write (a guest, or a page with `[web] writes` off); it opens that device's
page in the Devices popup, read-only for that viewer. Ember's fold-out also holds `Visualizer delay`
while the card runs. The page says the product with its product id, the unit id, the firmware and
the DSP build the device reported (`3 (advertised)` and a dash until it has, the first being the
build the device advertised), the profile it is read through, the link (`connected`, `disconnected`,
then `passphrase refused` or `readings stale` where that holds, joined by ` · `), the signal
strength and the passphrase note. The signal reads `−62 dBm advert, 3 min ago`: where it was
measured and how long ago, and `—` while the daemon has never heard one. Under the key graphs **All
readings**, a disclosure that starts closed, holds a table of every reading the daemon holds, named
or not (never the one that echoes a passphrase change, which carries the code), with its function
address, the profile's name for it (`?` when it has none), its value and how long ago the daemon
heard it. The ages count up on the page. The part is sent again when a fact or a value changes (at
most once a second; an amp's meters, which the page shows live, do not count, so their rows in the
table are as they stood when the part was last sent), and when a reading is heard again with the
same value in a later minute, so "Heard" for a value that does not change is accurate to about a
minute; opening the page reads it afresh, so the table is current when you look.

The **Identify** button on that page (not shown for a device whose profile offers no identify, the
Mesh Card; present but off, with its reason beside it, while the device is not connected) asks the
unit to show itself, to tell one from others. The page answers with a popup: `Identify sent`; `Just
sent — wait 2 s` when one went out within the last two seconds (nothing is sent); or the daemon's
words when the device is down (`device not connected, next attempt in N s`). It is a write, and what
the unit does on the command (a blink, a timed cue) is not verified on hardware yet. A guest or a
page with `[web] writes` off has no Devices popup; the only Identify it still sees is Signal's, in
the amp area, which is inert there.

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

**The switch to this computer.** One switch on the page flips the volume and the tone control from
the device you are looking at to the computer `svolderd` runs on: the volume then sets the computer's
volume (1 % a press, 0 to 100 %, never a boost) and its mute, and the tone control is the computer
EQ. It is drawn on every device's page, an amp's too, while the daemon serves the computer; with
no device shown there is none. Ember puts a pill with the device's name and `This computer` beside
the name, Paper writes `showing: the DAC · this computer` under the title, Signal makes the
computer's hop the switch and keeps its facts (format, other sounds, the path verdict) below, and
Night drive keeps two keys left of the menu button (on a phone one `DAC │ PC` pill in the top left).
Flipping writes nothing, and the side is yours: it is kept in your browser, other viewers keep theirs,
and a guest can flip to look but every control stays quiet. Where the computer cannot be used, a
sentence says why (`computer controls unavailable: no pw-cli on this computer`) in the switch's
place and the device's side stays.

On the computer side the mode reads `Off | EQ` (no Loudness). The curve is the points, up to 16:
drag one for its frequency and gain while the sound changes with no gap, turn the wheel or pinch two
fingers for its Q, tap an empty spot to add a peak there, or use `+ Add point`. Each row of the
table has a type (`PK`, `LSC`, `HSC`), a frequency, a gain and a Q you can step or type, and `×`. A
line says `7 of 16 points · preamp −6.0 dB`: the preamp lowers the whole signal so a boost never
clips, and the line says by how much. Off takes the EQ out of the path and keeps the points; editing
while Off turns it on. The presets are the same list as the device's, saved ones included, and
`Save as…` keeps the computer's points under a name. A preset a device's own tone control plays
too would play twice, so the page asks (`Warm is on the Gungnir 2 too: flatten the Gungnir 2 and apply on the
computer?`): Yes flattens the device first, No changes nothing.

**This computer's points.** Flipped to this computer, the table lists the points by frequency, and
the `#` column counts that order. A row never jumps under your finger: the table is put in order
when you let go of a drag, enter a typed value, add a point or recall a preset. The `×` of the only
point left sets it to 0 dB and keeps its frequency, type and Q.

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

The page model carries the list as `presets`: `status` (`lead`, `name`, `tail`: the line above the
list), `rows`, `editable` (save, rename and delete show: the viewer may write and the configuration
can be written), `recall` (a recall may be made from here) and `note` (why editing is hidden, for a
viewer who may write). A row holds `name`, `built_in`, `imported` (an AutoEq or file preset: its
bands are fitted, and an overwrite with the device's curve is refused), `on` (the lamp), and `json`
and `rename_json`, the bodies its recall and delete post and its rename posts beside the new name.

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
switched off), `A` arranges the sections (below), `T` opens the theme picker when nothing covers the
page (every theme has it, a guest too; over an open question, a popup or a theme's own sheet it does
nothing, and in the picker it closes it), Space plays and pauses the computer's player (every theme
has it; not while the focus is on a button, a link, a slider or another control Space itself works,
nor while a menu or a dialog is open, and nothing where the page has no player) and `?` opens
**Help** (below), which lists the keys and says how the curve is dragged. A guest, and a page with
`[web] writes` off, have `v`, `A`, `T` and `?` only, and Help lists just those; `v` is listed only
where the screen has a visualizer card. The footer names a few keys at desktop width.

**Help** is the menu's *Help…* row, or `?` in every theme (not while a field has the focus, nor
over a question, another popup or a theme's own sheet; from the open menu it closes the menu
first). It is open to a guest too and has three pages:

- **Here**: the keys of what had the keyboard focus when Help was opened (the dial, a slider, a
  point of the curve, a list), under the name of that control, then the keys of the whole page.
  After a click on an empty part of the page nothing has the focus, and Here says so and shows
  the keys of the whole page alone.
- **Everywhere**: the keys that work on every screen of the theme, `T`, Space and `?` among
  them.
- **About**: the svolder version (not shown to a guest), the address of the daemon the page
  talks to, and links to the project, which open in a new tab.

Help shows the keys that work now: a key that needs a right the viewer has not, or a control the
screen has not, is left out. On a phone Help fills the screen and shows its list of pages or one
page, with a way back at the top. Escape, the close button, a click beside the box and Back close
it; Back from a page goes to the list first. While Help is open the browser tab's title names it
and its page (`Bifrost 3 · connected · Help · About — svolder`), so the history's entries read
apart.

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

**The menu** is the round button just left of the link's pill (at the top right on a screen with no
pill), on every screen and for a guest too. It opens a small list: first the **Theme** row (the
theme's name, with a small picture; it opens the picker), then the theme's own rows (Ember's are
headed *Ember's effects*: the three levels below, the one that shows checked, and under them, when
the screen has a Now playing card, a **Visualizer** switch, then **Arrange sections…**), then
**Devices…** (the Devices popup, below; read-only for a guest and where `[web] writes` is off), **EQ
library…** (below; not for a guest, nor where `[web] writes` is off), **Help…** (the Help popup,
above) and **Sign out**, or **Sign in** for a guest. Another device is shown from the Devices popup.
A click or a tap picks a row and closes the list, and Escape, `q`, a click outside or the button
again close it. By keyboard: `Tab` reaches the button first, `Enter` or `Space` opens it, the arrows
move between the rows (round the ends, skipping any that is disabled), `Enter` or `Space` picks, and
the focus returns to the button. The **picker** (the Theme row, or `T` from any theme) shows a card
for each theme, the one in use marked; Escape or a click outside closes it, and a pick of another
theme changes the page to it. Each browser keeps its own choice, and `?theme=ember` in the address
always gets Ember back. See [the frame](../design/frame.md).

**Devices** is the menu's *Devices…* row, in every theme, for everyone. A guest and a page with
`[web] writes` off read it: the list, the Overview, a device's state, key graphs, facts, signal and
All readings, and *Show on the page* (it picks the device their own page shows and writes nothing);
there are no settings, no *Make default*, no *Remove…*, no *Identify*, no *+ Add a device* and no
config line, and the daemon refuses every write whatever the page draws. A guest's facts have no
unit id; a signed-in viewer of a page with writes off has it. Their list is the devices the page
knows, kept current as devices come and go. It stays open while you look around and closes with its
×, Escape, a click beside it or Back:

- **The list** on its left names **Overview** and then every configured device, with a full dot
  for one that is connected and a ring for one that is not, and the tag **Shown** on the device
  the page shows, and last **+ Add a device**. On a phone the list is the first screen and an
  entry opens its page.
- **Overview** shows every device at once: its name, whether it is connected (or since when it
  is not, when the daemon knows), and its key graphs over the last 30 minutes. The signal
  strength comes first, then what the device's profile marks as worth watching (on a DAC the
  volume, the sample rate and the input; on an amp its meters and its state). Each graph has
  its name, the last reading and the range it moved in; a reading that did not change, or was
  heard once or not at all, is said in a line of words instead of a flat graph. A device that
  is not connected is drawn dimmed. The graphs are read again every ten seconds. When the
  daemon's `config.toml` changed since its last reload (a hand edit it has not taken), the
  Overview opens with the daemon's words, `config: PATH changed since the last reload: svolder
  daemon reload (or SIGHUP)`, for a signed-in writer only; it goes at the next read after a reload,
  with nothing pressed on the page. When the file is gone the words are `config: PATH is missing:
  the daemon keeps its running configuration; restore the file, then svolder daemon reload`, in
  the same place.
- **A device's entry** opens that device's page: its settings, the same graphs, its facts and
  *Identify*, **All readings**, and **Show on the page** (at the right of the name), which makes it
  the device the page shows. The popup stays open, the tag moves, and Back still closes the popup
  rather than going back to the device before. The page of the shown device says it is the one
  shown.
- **A device's settings** are saved one by one, each by its own control, and say at the control
  when something was refused:
  - **Name**: type it and press Enter or *Save*. A name another device has, or none, is refused
    in words under the field and what you typed stays. A renamed device keeps its page: the popup
    follows it to its new name, the list and the tab's title with it, and when it is the device
    the page shows, so does the page.
  - **Profile**: the button names the profile and opens a list; *Automatic* lets the daemon pick
    the profile from what the device advertises.
  - **Passphrase**: *Change* opens a small form with three choices: *Automatic* (read from what
    the device advertises), *None*, or *A 4-digit code*, which you type (the field is never
    filled in and has a *Show code* switch). On a device that is playing, the page asks first,
    since a wrong code makes the device refuse the link and the sound stops. A passphrase typed
    here, or in *+ Add a device*, travels over HTTP on the local network, like everything else on
    the page, unless the page is served over HTTPS (behind a proxy that does it).
  - **Make default**: makes the device the one the daemon uses when none is named. The device
    that is the default says so.
  - **Remove…**: asks once, naming the device (and saying so when it is playing), with *Keep*
    chosen; on *Remove* the device leaves the configuration, the popup goes to the Overview with a
    line that says what was removed, and when it was the device the page shows, the page moves to
    the first device left. The device itself is not changed, and it can be added again.
  A setting changed somewhere else (another browser, the terminal) shows on the page within ten
  seconds.
- **+ Add a device** is the walk `svolder setup` and the terminal's `n` run, with the daemon's own
  words, step by step on the page. If a device is connected it warns first, naming it: the scan
  and the passphrase test may briefly disturb its sound (*Cancel* has the focus; *Go on* carries
  on). Then it listens for ten seconds and lists what it hears, a device that is configured already
  saying which one; pick one (a configured unit cannot be picked), name it (the line under the
  field is the name an empty answer takes) and, for a device that turns the automatic passphrase
  away, type its four digits (the field is never filled in and has a *Show* switch). *Back* steps
  back, *Stop adding* ends the walk, and leaving the page (another entry, Back, Escape, closing the
  popup) stops a walk you started. When it is done the page says the device was added and whether
  it is connected, with *Open its page*; on a first install the page behind shows the new device
  too. Only one walk runs at a time: if one was started from the terminal the page says so and who
  started it, and shows its step with nothing to press; a walk of this page that was running when
  you reloaded is taken up where it stands.
- **The facts** under the graphs are the ones the themes' Device fold-outs showed (the product,
  the unit id, the firmware, the DSP build, the profile, the link, the signal), with **Identify**
  under them: it asks the unit to show itself, and on a device that is not connected it is there
  but off, with its reason beside it. **All readings** is a disclosure that starts closed: opened,
  it lists every reading with its value and how long ago it was heard (the last known ones, as
  such, for a device that is not connected); closed, nothing of it is drawn, so a browser's
  find-in-page does not look inside it.

**EQ library** is the menu's *EQ library…* row, in every theme, between *Devices…* and *Help…*, for
whoever may change things (a guest and a page with `[web] writes` off do not have it). It is about
the side the page shows: its title says which (`EQ library · Kara`, `EQ library · This computer`),
and it closes when another device is shown. Its list on the left has three pages:

- **AutoEq** searches AutoEq's index (through the daemon: the page never reaches the internet). Type
  a headphone's name (at most 60 characters); the search is sent after a short pause, or at once on
  Enter. The answer groups the headphones, each a disclosure of its measurements (source, form and
  rig), AutoEq's recommended one first; it says how many headphones matched, how many more were left
  out (*narrow the search*), how big the index is and when its copy was fetched, and the daemon's
  note when it had to use an older copy. Choosing a measurement draws its **preview** for the side:
  a graph of the target (dashed), what the device's bands can make of it (the line), the places
  where they miss it widely (shaded), and the numbers the daemon worked out: the fit, the worst miss
  and where it is, and how far to lower the volume when the bands boost. **Compare with…** lists the
  other measurements of the same headphone and draws the second target (dotted) with its numbers
  beside the first's. For the computer there is no fit: the graph is the correction as the computer
  plays it. Under the preview, a name (the headphone's by default) and **Save as a preset**: it only
  saves, it does not put the preset on, and says so. A name that is taken is refused at the field
  with *Use “NAME (source)”* offered; a refusal of any other kind is worded under the field.
- **Upload a file** reads a parametric EQ file (Equalizer APO text, the format AutoEq writes and
  `svolder eq import` reads; 64 KiB at most) in the page and saves it as a preset named after the
  file, or after the name you type. Save is off, with its reason, until a file is chosen; a file
  svolder cannot read is refused in the daemon's words.
- **Saved presets** lists the presets of the side, each marked *In the library* or *On Kara now*
  (the one the device's bands match), and *built in* or *imported* where it is. **Overwrite with
  current…** writes the device's bands over a preset of your own after one question (*Keep it*
  chosen). It is off, with its reason, for a built-in preset, an imported one, the computer's side,
  a device that is not connected, one with no tone control and one whose tone control has not been
  read yet. The page does not say which preset a device's own slot holds, because the device does
  not say.

While an imported preset is on the device (or recalled and edited) and the EQ is on, each theme's
tone graph shows what the TUI's does: the **target** as a dotted line, the **gap** shaded where the
bands miss it by more than 2 dB (the daemon's spans, never a band of your own), and the fit in
words, `fit 0.9 dB`. Ember, Signal and Paper draw them on their curve; Night drive writes
`fit 0.9 dB` in its EQ line, draws the target as a faint ghost ridge behind the mountains and tints
the floor under the gap (a slow phone gives the ghost ridge and the tint up before it lowers the
density). There is none for the computer, for a built-in preset or in Flat and Loudness.

Recall, rename and delete stay where each theme shows the presets. The terminal UI draws the fit
and overwrites a preset with `u`, and `i` imports a file; comparing two measurements is not there
yet.

**Ember's effects** have three levels, chosen in the menu:

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
for Firefox 140 and Chromium 142 or later.

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
  both are about the tone only the red note's arrow is drawn; the red note is also circled. The
  volume is above both columns, so a red note about it has its arrow go along the gap under them
  and up the sheet's margin on the needle's side, then in to the written volume, clear of the tone
  section and the controls. On a phone each note follows its section.
- **The device.** Near the foot, *Device settings…* is a written word that opens the device's page
  in the Devices popup: the facts, *All readings* and Identify are there. A guest, or a page with
  writes off, reads *Devices…* and gets the page read-only.
- **A guest, or a page with writes off,** sees the same words with no underline (but *Devices…*,
  which opens the read-only page), and a tap says *Sign in to change this*.
- **A lost connection** reads *Reconnecting to svolder…* under the name and fades the sheet; a
  device out of reach is a red note with *try now*.

Paper has no effects, no visualizer and no Arrange mode; it ignores the page's effects setting.
See [Paper](../design/web-paper.md).

**Signal** is the page's third theme: the sound's way from the player to the ears drawn as one line,
on a dark ground. Pick it from the menu's *Theme* row or with `T`, or open the page with
`?theme=signal`. It is dark only; the only row it adds to the menu is *Add to the chain…*.

- **The line.** Left to right: the player (green), the computer (yellow), the device svolder shows
  (amber), then whatever comes after it. A solid dot is something svolder reads, with its live facts
  under the name; a plain ring with italic words is a hop you declared (with `svolder hops` or the
  menu's *Add to the chain…*), because svolder cannot see an analogue signal. After the DAC the line
  runs
  on thinner and slower. Moving dots show the sound flowing while the player plays; they stop when
  it pauses, while the page is hidden and always under reduced motion. A stretch that the computer's
  EQ changes is yellow. The DAC says `arrives as sent` when it reads the rate the computer sends,
  else `changes the sound`. While the DAC's rate is not read it says neither.
- **Choosing a hop.** Click or tap it, or use Left and Right (Home and End go to the ends) with the
  line focused. The chosen hop is boxed and what it offers opens underneath. The shown device is
  chosen when the page loads. The DAC opens the large volume with *−*, Mute and *+* (hold *−* or *+*
  to keep stepping; muted reads *−∞ dB*) and the tone curve with a point to drag for each band (on a
  touch screen a two-finger pinch on the curve sets a band's Q, the wheel does it with a mouse). The
  input, mode and presets are in the shelf below (the mode is under the curve on a phone). A linked
  Forkbeard amp opens its power, meters and faults, with *Unlink*; its controls write to the amp,
  never to the DAC, and the DAC's volume stays in the DAC's label. A declared hop opens its name and
  note with *Edit*, *Move earlier*, *Move later* and *Remove*, which asks once. The player opens Now
  playing with its transport and the path in words, and the track's cover to the right of them (a
  fifth of the area wide, 200 to 300 px, above the title on a phone; a track with no cover leaves
  no box); the computer opens its sink, rate, volume, mute, the computer EQ and the preset applied,
  read only.
- **Add to the chain….** A row of the menu, while a device is shown. It opens the sheet *After the*
  (device): Amp, Headphones, Speakers or Other, a name and an optional note. A Forkbeard amp the
  daemon lists that is not yet linked comes first as *name (connected)*, which links it. When you
  use an AutoEq preset on the computer and no headphones hop is declared, the line ends with a ring
  named after the preset, *from your AutoEq preset*, for as long as it is applied; it is never
  saved.
- **Below the first screen.** In two columns from 900 px wide (one below that; the band table needs
  the width): the band table with the input under it, beside the mode, the presets and the controls
  (and a Loki Max's slots and settings), then the device's facts across the full width. While
  Loudness is on, its chip carries a round *−* and *+* and the strength under its name; hold either,
  or a band's *−* or *+*, to keep stepping. *Save as*, *Rename* and *Delete* each ask in place; the
  facts start folded (the choice is kept in this browser). Once the line has scrolled out of view a
  slim bar pins at the top with the same hops, the chosen one ringed and the device's volume beside
  its name; a tap on it chooses a hop and goes back to the top. The Now playing bar stays at the
  bottom except while the player is chosen. On a window taller than 1100 px the page stops growing
  and its parts stay together as one block, centred in the window, with no gap growing between them.
- **A phone.** The line is a strip across the top with the chosen hop in the middle; swipe it or tap
  a neighbour to choose. The strip stays at the top as the page scrolls, and so does the menu
  button. The shelf opens with the input and the presets, then the band table.
- **Trouble.** A device out of reach dashes the line into a hollow ring (red for a lost link, amber
  while it waits or rests), stills what follows, and shows a card with *Try now* and the seconds to
  the next try; the last heard volume and curve stay in view, dimmed. A lost connection to svolder
  reads *Reconnecting to svolder…* and greys the page. A guest, or a page with writes off, sees
  every control drawn but inert, and a tap says why. The browser's tab reads the device and the
  state, for example *Bifrost 3 · connected — svolder*.

Signal has no effects, no visualizer and no Arrange mode. See [Signal](../design/web-signal.md).

**Night drive** is the page's fourth theme: the music drawn as an outrun road at dusk. Pick it from
the menu's *Theme* row or with `T`, or open the page with `?theme=nightdrive`. It is dark only, and it
draws on canvases, so it follows the music only when svolderd can capture the computer's audio (the
`analysis` feed; the same capture the visualizer uses).

- **The scene.** A striped sun on the horizon, mountains along it that are the live spectrum (20 Hz
  at the left edge, 20 kHz at the right), and a floor of lit cells rolling toward you, one row of the
  music at each step. The louder the music is against the loudest of the last minute, the warmer and
  brighter the night; at a sustained peak embers rise from the floor and the road dries, in the quiet
  it turns wet and hazy, a drop sends a flare through the sun and one wave down the floor. After a
  long loud bass the sun throws rays; a sharp rise in the highs sends a shooting star, and held
  highs a shower of them. These are rare by design, and the same music gives the same events on any
  screen.
- **The top line, the volume and the EQ line** are over the scene: the device, its input and rate
  (and `not connected` beside its name when the link is down), what the computer does to the sound
  (`host EQ on · changes the sound`, in amber) at the right, the volume as a large neon number (`−∞ dB`
  when muted) and, under it, the tone control's state (`EQ`, the preset, the gains, or `LOUDNESS` or
  `FLAT`). After 15 seconds with no input they fade out; a move, a key, a touch or the wheel brings
  them back at once, a volume changed elsewhere shows for 5 seconds, and trouble keeps them (or brings
  them back). The mouse cursor hides over the scene after 2 seconds still.
- **The dash** along the bottom: `−` `MUTE` `+` (hold one and it repeats), `EQ` (lit while the EQ of
  the side shown is on: a press turns it off, a press on the dark key turns it on; on the computer
  side it is the computer's EQ, on the device's side the tone mode goes between Flat and EQ, and
  Loudness counts as off; a device with no Flat and EQ has no key), the input as chips (a
  list when there are more than three), the cover and the track with its progress bar, previous,
  play or pause and next, and `DEVICE ▴`. An amp has its power key and gauges where the input would
  be. Space plays and pauses the computer's player, as in every theme. The mouse wheel over the big
  volume number steps the volume (up is louder) and `↑` `↓` do too, when no control has the keyboard
  focus and no sheet or question is open; a held arrow repeats like a held `+`, and a step is the
  device's own, or 1 % on the computer. The arrows are a writer's: for a guest they do nothing. The
  wheel anywhere else on the scene still sets the Q. On a desktop and on a phone held sideways the
  dash fades to a fifth with the HUD, and the first tap on a faded dash only wakes it and presses
  nothing; a key held on it, a list open on it, the keyboard focus on it and a mouse resting over it
  keep it up. On a phone held upright the dash is a translucent two-row panel over the foot of the
  scene that never fades and leaves the scene about nine tenths of the height. On a phone the switch
  is one small pill in the top left corner, across from the menu button: `DAC │ PC` (`AMP │ PC` for
  an amp), the side shown lit; a desktop keeps two keys left of the menu button.
- **Tuning on the mountains.** Move the pointer over the mountains and they turn into the tone
  curve with a bubble for each band: drag a bubble for its frequency and gain, drag the diamond at
  either end of the lit bell for its width (Q), scroll the wheel for the Q of the nearest band, or
  pinch two fingers. A touch only counts inside the band the curve is drawn in (+12 dB to -12 dB and a fingertip beyond); above or below it, it is a touch on the scene. They turn back about a second and a half after you let go. A guest never sees the curve (a tap on the
  mountains says why it does nothing); a page with writes off and a device with no bands only see it.
- **The sheet.** `DEVICE ▴`, or *Device…* in the menu, opens everything else about the device on a
  neon sheet: the mode (Loudness with its `−` and `+` under its name), the input, the band table
  (typed values, a step a tap), the presets (Save as, Rename and Delete ask in place), a Loki Max's
  slots, the controls, an amp's meters (their needles move as the readings arrive) and, folded, the device's facts. Close it with ✕, Escape or a
  tap on the scene. On a phone held upright it slides up from the dash; held sideways it comes in from
  the right.
- **Motion** is the theme's menu setting: *Full* draws at the screen's rate; *Calm* about ten a
  second, with the road and the sky's events stopped; *Still* about one a second. With none chosen,
  a system set for reduced motion starts on Calm. While the music is silent or the player is not
  playing, a phone draws at most 30 a second. Choosing Full lifts a phone's 30 a second cap at once and the
  scene steps down within about a second if the phone cannot keep up.
- **A slow phone.** The scene watches what it costs. If drawing is slow it gives up the wet road
  first, then draws at a lower density, then at most 30 a second, and brings them back in the other
  order once it has been comfortable for 10 seconds. A phone starts at 30 a second, comes up to at
  most 60, and a canvas never has more than 2.5 million pixels.
- **Trouble.** A device out of reach turns the scene grey and still and puts a card in the middle
  with *Try now* and the seconds to the next try. A lost connection to svolder greys the page and says
  *Reconnecting to svolder…*. With no music analysis the mountains take the shape of the tone curve
  and the line says why. A guest sees every control drawn, inert, and a tap says why. See
  [Night drive](../design/web-nightdrive.md).

**Night drive's readout.**

On a slow phone, Night drive can show what it costs. Add `nd-debug` to the address, for example
`?theme=nightdrive&nd-debug`, and a small box under the device line reads, twice a second: the draws a
second (measured, not asked for) and the pace asked, the mean and 95th-percentile draw time of the
last 60 draws, the canvas size in device pixels and the density the screen has and the one it is
drawn at, how far it has stepped down and whether the wet road is allowed, the Motion level, the long
tasks of the last 10 seconds (`n/a` where the browser has none), what the music reaches in the sky
and what the mood hears of it, and the switches in force.

Name layers after it, separated by commas, to turn them off one at a time and find the cost:
`?theme=nightdrive&nd-debug=noglow,nofloor`. The names are `noglow` (the glow of the sun, the
mountains and the bubbles, and the page's CSS shadows), `nofloor` (the road's heat grid), `nosky`
(the sky, the sun and the stars), `noridge` (the mountains), `nodash` (the dash along the bottom),
`nohud` (the top line, the volume and the EQ line), `nooverlay` (the tone overlay), `nomood` (the
night, the sun, the heat and the road's speed stay as they were, with no flare and no wave), `nowet`
(the wet road), `noembers` (the embers), `nohaze` (the haze over the far floor) and `dpr1` (one
backing pixel per CSS pixel). A word that is not a name is listed as unknown. The parameter goes in
the query, not after a `#`, which the page drops at once; a step to another device drops it from the
address, and the box stays until the page is reloaded. Signing in with a link also drops the query
(the sign-in page goes on to `./`), so open the link once, then the address with the query.

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
`api/v1/devices/DEVICE/` (`model`, `strip`, `series`), `api/v1/events` (the server-sent stream),
`api/v1/fleet` and `api/v1/host` answer `401` with an empty body to a visitor who is neither signed
in nor a guest, and `404` for an unknown device.

`GET api/v1/devices/DEVICE/series?ids=signal,volume&secs=1800` answers the last `secs` seconds (at
most the half hour the daemon keeps, which is also what it answers without `secs`) of the readings
named, one point a second at most: `signal` is the signal strength in dBm, heard whenever the daemon
catches the device's advertisement (so not while it is connected), and every other id is the key of
one of the device's controls, whose points are raw readings. Without `ids` it answers the device's
key readings, the ones its profile marks. An id that names nothing is listed under `refused` and
the others are still answered. An id asked twice is answered once, and more than 32 different ids
in one request are a `400` that says so.

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

**The routes at a glance**, every one the daemon serves under `api/v1/` with its method. A segment
that varies is in capitals (`DEVICE` is a device's name or unit id, `JOB` a job's number, `N`, `KEY`
and `BAND` what the paragraphs above name). *Takes* is the body's fields (a GET's query fields start
with `?`), `none` for a route that reads nothing, and `not listed` where the paragraphs above and
below are all there is. The table is checked against the daemon's sources and the page's scripts
(`make routes-doc-check`): a route or a field missing here, or one the daemon does not serve, fails.

| Method | Route | Takes |
|---|---|---|
| POST | `api/v1/autoeq/import` | `force`, `name`, `target`, `variant` |
| POST | `api/v1/autoeq/preview` | `compare`, `target`, `variant` |
| POST | `api/v1/autoeq/search` | `query`, `refresh` |
| POST | `api/v1/autoeq/upload` | `force`, `name`, `text` |
| GET | `api/v1/devices` | `none` |
| POST | `api/v1/devices` | `name`, `passphrase`, `profile`, `unit` |
| POST | `api/v1/devices/DEVICE/connect` | not listed |
| POST | `api/v1/devices/DEVICE/controls/KEY` | not listed |
| POST | `api/v1/devices/DEVICE/default` | `none` |
| POST | `api/v1/devices/DEVICE/device-presets/apply` | not listed |
| POST | `api/v1/devices/DEVICE/device-presets/recall` | not listed |
| POST | `api/v1/devices/DEVICE/device-presets/save` | not listed |
| POST | `api/v1/devices/DEVICE/edit` | `name`, `passphrase`, `profile` |
| POST | `api/v1/devices/DEVICE/eq/bands/BAND` | not listed |
| POST | `api/v1/devices/DEVICE/eq/loudness` | not listed |
| POST | `api/v1/devices/DEVICE/eq/mode` | not listed |
| POST | `api/v1/devices/DEVICE/identify` | not listed |
| GET | `api/v1/devices/DEVICE/model` | not listed |
| POST | `api/v1/devices/DEVICE/presets` | `force`, `from`, `name` |
| POST | `api/v1/devices/DEVICE/presets/delete` | not listed |
| POST | `api/v1/devices/DEVICE/presets/recall` | not listed |
| POST | `api/v1/devices/DEVICE/presets/rename` | not listed |
| POST | `api/v1/devices/DEVICE/remove` | `none` |
| GET | `api/v1/devices/DEVICE/series` | `?ids`, `?secs` |
| GET | `api/v1/devices/DEVICE/strip` | not listed |
| POST | `api/v1/devices/DEVICE/volume` | not listed |
| POST | `api/v1/devices/DEVICE/volume/stop` | not listed |
| GET | `api/v1/events` | not listed |
| GET | `api/v1/fleet` | not listed |
| POST | `api/v1/hops` | not listed |
| POST | `api/v1/hops/edit` | not listed |
| POST | `api/v1/hops/move` | not listed |
| POST | `api/v1/hops/remove` | not listed |
| GET | `api/v1/host` | not listed |
| GET | `api/v1/host/cover` | not listed |
| POST | `api/v1/host/eq` | not listed |
| POST | `api/v1/host/eq/points` | not listed |
| POST | `api/v1/host/eq/points/N` | not listed |
| POST | `api/v1/host/eq/points/N/remove` | not listed |
| POST | `api/v1/host/eq/preset` | not listed |
| GET | `api/v1/host/frames` | not listed |
| POST | `api/v1/host/mute` | not listed |
| POST | `api/v1/host/press` | not listed |
| POST | `api/v1/host/volume` | not listed |
| GET | `api/v1/session` | not listed |
| POST | `api/v1/walk` | `none` |
| POST | `api/v1/walk/JOB/answer` | `choice`, `text`, `run` |
| POST | `api/v1/walk/JOB/back` | `run` |
| POST | `api/v1/walk/JOB/cancel` | `run` |

The configured devices are read and written by a signed-in viewer with writes on, and by nobody else
(`401` and `403` as for the other writes, the list included): `GET api/v1/devices` lists them (unit,
name, pinned profile, the kind of passphrase, the default mark, whether connected and since when
not), `POST api/v1/devices` adds one (`{"unit":"…","name":"…"}`, with `profile` and `passphrase`
where wanted), and `POST api/v1/devices/DEVICE/edit` (`name`, `profile`, `passphrase`, each only
when the body carries it; `"profile": null` goes back to the detected profile), `/remove` and
`/default` act on the device its name or unit id names. Each answers the list as it stands
afterwards, and beside it `profiles`: every profile a device can be pinned to, by name
(`{"name":"bifrost3","display":"Bifrost 3"}`; `display` is left out where it would repeat the name,
and `"base":true` marks a profile that describes no product of its own), and `config_changed`: the
daemon's words that `config.toml` changed since its last reload or is missing, or `null`. An edit's
`profile` is taken when it is one of those names and refused as `invalid` otherwise; the list is
empty while a file in the profiles folder cannot be read, and every pin is refused then. A
passphrase travels in a request's body only, and no answer carries one. A refusal is
`{"error":"…","reason":"…"}`: `422` for `taken` and `invalid`, `404` for `missing`, `409` for
`read_only` and `file_busy`, `503` for `stopping`. While the daemon's `config.toml` is gone a write
is a `409` `read_only` too, and the page shows the daemon's missing-file words in place of the
read-only sentence. A body that is not what a route takes is a `400` with no `reason`, whose words
name the key at fault and never what the body held; a body over 4 KB is a `413` in plain text, which
the page never sends.

The device walk (scan, pick, name, passphrase test, write; see the design's
[device walk](../design/device-walk.md)) is run by the daemon and driven by the same viewers only
(`401`, `403`): `POST api/v1/walk` starts it and answers `{"job":N,"run":R}`;
`POST api/v1/walk/N/answer` takes `{"choice":N}` (from 0) or `{"text":"…"}` (a name, a passphrase),
and `/back` and `/cancel` take nothing; each answers `204`. `R` is the daemon's run stamp: job ids
restart with each run, so a body that adds `"run":R` to any of the three is refused `no_such_job`
when `R` is not the daemon's run now (a body with no `run` is let through). Where the walk stands
comes on `api/v1/events` with `needs=walk`, as `walk` events `{"run":R,"job":N,"state":{…}}`, each
state whole (its prompt, the devices heard so far, why the last answer was refused), the walk's last
state first. A refusal is
`{"error":"…","reason":"…"}`: `409` for `busy` (another walk runs, its origin named) and
`not_waiting` (the walk is working on an answer, or has ended), `404` for `no_such_job`, `503` for
`stopping`. A walk nobody answers for 10 minutes ends cancelled. The passphrase travels in the
answer's body only, and no state or refusal carries it.

AutoEq is the daemon's: a signed-in viewer with writes on (a guest never makes the daemon reach the
internet, so `401` and `403` as above, the search included) searches the index with `POST
api/v1/autoeq/search` (`{"query":"hd650"}`, at most 60 characters; `"refresh":true` fetches the
index again), previews a measurement with `/preview` and saves one with `/import`, and saves a
file's text with `/upload`. A search answers `groups` (`{"key","name","variants"}`, each variant a
measurement with its `words`, AutoEq's recommended one first), `more` (headphones left out past
200), `measurements`, `fetched_unix` and a `note` when the cached copy was used. A measurement is
the `variant` a search answered (`name`, `source`, `form`, `rig`), and the side is
`"target":{"side":"device","device":"NAME"}` or `{"side":"computer"}`; an op that depends on a side
names it, there is no default side (a body without one is a `400`). `/preview` (`variant`, `target`,
`compare` for a second measurement) answers once, after the daemon's download and fit:
`{"preview":{…},"compare":{…}}`, each a `target` on 181 log-spaced points, the correction's
`preamp_db` and `peak_db`, and for a device a `fit` with its `response`, its `gap` spans (`from_hz`,
`to_hz`, where the fit misses by more than 2 dB), `rms_db`, `worst_db` and `worst_hz`. `/import`
(`variant`, `name`, `force`, `target`) and `/upload` (`text` of at most 64 KiB, `name`, `force`)
answer `{"name","filters","skipped"}`. The reasons add `not_listed` (`404`) and `unreachable`
(`502`) to the list above, and `busy` is `409`; a request that the client abandons cancels its
download. The upload's body may be 128 KiB and the others' 8 KiB. `POST
api/v1/devices/DEVICE/presets` also takes `"force":true`, which writes the device's bands over the
user's preset of that name (a built-in, an imported preset and the computer's side are refused,
`422` and `409`).

While an imported preset is on the device, or recalled and edited, the device's model carries it as
`tone.overlay` for the curve to draw (`name`, `origin`, `target`, `fit`, `gap` spans as `[from_hz,
to_hz]`, `gap_db`, `rms_db`, `worst_db`, `worst_hz`, `preamp_db`, `peak_db`); the numbers are the
daemon's, worked out from the bands on the device now: an edited band moves them with the curve.

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
svolder: no daemon answers on /run/user/1000/svolder.sock: the daemon serves the web UI; svolder daemon restart starts one, and `svolder web on` turns the web UI on
```

```
Open the web UI, turn it on or off, or print a code or its token

Usage: svolder web [OPTIONS]
       svolder web <COMMAND>

Commands:
  on     Turn the web UI on: `[web] enabled = true`, which a running daemon acts on at once
  off    Turn the web UI off: `[web] enabled = false`, which a running daemon acts on at once
  guest  Let visitors without a sign-in read the web UI's device screens, or not: `[web] guest`
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

### web on, web off

Turns the web UI on or off. With a daemon running, the daemon makes the switch: it writes
`enabled` under `[web]` in the configuration file it reads and reloads, which starts or stops the
page at once, with no restart, also for a page that never ran. The verb prints the line
`svolder daemon status` has for the page, `web: listening on 127.0.0.1:8738 (token, writes)` or
`web: off`, on standard output.

The verb exits 0 only when the page stands as asked. A page turned on that cannot listen exits 1
with ``web: cannot listen on ADDR: WHY ([web] enabled = true is written; `svolder web off` takes
it back)``. A page the daemon's answer still shows as it was, because its reload failed or took
longer than the five seconds the write waits, exits 1 with `web: off ([web] enabled = true is
written and the daemon has not acted on it: svolder daemon status)`. A daemon that refuses the
write (a read-only file, a file that does not parse, a daemon that is stopping) exits 1 with its
words. A daemon too old to make the switch is not written around: ``the daemon (VERSION) cannot
switch its servers, which this svolder asks it to. Run `svolder daemon restart` ``.

With no daemon (no socket, or one nothing listens on) the verb writes the key itself, prints
`web: on in PATH` (the configuration file), and says on standard error `svolder: nothing answers
on SOCKET: the web UI starts with the next daemon`. A file that is not there is made from the
template. A socket that cannot be tried (one this user may not open, a path too long for a
socket) may have a daemon behind it, so the verb writes nothing and exits 1 with `cannot reach
the daemon's socket SOCKET: WHY`. Where `SVOLDER_WEB_ENABLED` already turns the page on and the
file has no `enabled`, `web on` writes nothing and `web off` writes `enabled = false`, which wins
over the variable.

```
Turn the web UI on: `[web] enabled = true`, which a running daemon acts on at once

Usage: svolder web on [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

```
Turn the web UI off: `[web] enabled = false`, which a running daemon acts on at once

Usage: svolder web off [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### web guest on, web guest off

Lets visitors without a sign-in read the page, or stops it: `guest = "read"` or `guest = "none"`
under `[web]` (see "Guest read"). It is the same switch as `web on`: with a daemon running, the
daemon writes the key in the configuration file it reads and reloads, and the running page serves
the new mode at once, with no restart. The verb prints the line `svolder daemon status` has for the
page, which says `guest read` while guests may look, and exits 0 when the page stands as asked.
A page that is off is not a failure: the key is written and a note on standard error says that no
one sees it until `svolder web on`. A page the answer still shows with the old mode exits 1 with
`... ([web] guest = "read" is written and the daemon has not acted on it: svolder daemon status)`;
a daemon that refuses the write exits 1 with its words, and a daemon too old to have the request is
not written around: ``the daemon (VERSION) cannot switch the guest view, which this svolder asks it
to. Run `svolder daemon restart` ``.

With no daemon the verb writes `guest` itself, prints `web guest: on in PATH`, and says on standard
error that the mode applies with the next daemon (a daemon that started meanwhile and reads the
file is told to reload). With `auth = "none"` there are no guests: `guest on` still exits 0, with a
note on standard error that every visitor is signed in and the guest view changes nothing.

```
`[web] guest = "read"`, which a running daemon acts on at once

Usage: svolder web guest on [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

```
`[web] guest = "none"`, which a running daemon acts on at once

Usage: svolder web guest off [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

```
Let visitors without a sign-in read the web UI's device screens, or not: `[web] guest`

Usage: svolder web guest [OPTIONS] <COMMAND>

Commands:
  on    `[web] guest = "read"`, which a running daemon acts on at once
  off   `[web] guest = "none"`, which a running daemon acts on at once
  help  Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
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
