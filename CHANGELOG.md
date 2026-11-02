<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Changelog

Each version below becomes the top of its release notes. 0.25.4 is the first version with
downloads; the versions before it were source only.

## 0.31.3

Paper's follow-ups, and releases that start themselves.

- **Paper**: the red volume note's arrow no longer runs over the tone section. It climbs the
  margin on the needle's side and ends at the volume's written value.
- **Paper**: The device (the facts table, every reading and Identify) starts folded. Its heading
  unfolds it, and the choice is kept per browser; it still opens in a private window.
- Paper: a device switch during a point drag ends the drag cleanly; a question returns the focus
  to the word that opened it after a redraw; the ruler's labels keep their gap after a resize; the
  Caveat font names its Medium weight.
- **Releases**: pushing a version tag starts the release workflow, which now also runs the whole
  test suite on arm64 before anything is published. Each release's notes open with the Beta
  paragraph.
- `svolder-sim --trace` stamps every line with the milliseconds since the simulator started, so
  two traces can be compared for pacing and reconnect time.

## 0.31.2

The web page gets its second theme, Paper: the device drawn as a spec sheet someone wrote on.

- **Paper**: pick it from the menu's Theme row, with `T`, or with `?theme=paper` in the address
  (`?theme=paper.light` and `?theme=paper.dark` choose the paper too). The volume is a ruler with
  a needle you drag; the tone is a curve with points you drag; every other control is written,
  and an underlined word acts (`− quieter · mute · louder +`, an input's name, a preset's name, a
  band's `−` and `+`). It covers everything Ember does, on a DAC and on an amp.
- **Handwritten notes**: in blue ink, Paper says in words what the EQ does, whether the rate
  matches all the way to the DAC, where the track is, the firmware and the link. One note at most
  is in red pencil and circled: a device off the air or at fault first, then a loud volume, then
  an EQ that can clip, then a path that changes the sound. **Last change** says what changed and
  when, and "from this browser" (or "from this phone") only when this page asked for it.
- **Light and dark paper**: Auto follows the system; Light and Dark are kept per browser. The
  inks stay the same on dark paper, a little lighter.
- Paper's fonts, Crimson Pro and Caveat, are shipped with svolder under the SIL Open Font Licence.

Paper found seven things the shared core lacked; all are fixed for every theme:

- The track's album is its own field in the model (it was only inside the source line).
- A theme gives the browser a colour for each scheme, and the page's colour scheme follows the
  one chosen, not only the system's.
- Switching themes keeps the new theme's own scheme, so a dark Paper reloads dark.
- At boot the scheme the server painted (from the address or the cookie) wins over a stored one.
- The toast's shadow is a theme colour, so it suits a light page.
- The band steps and the drag's writes live in the core, for any theme with a tone graph.
- The frame declares a serif and a hand font for themes to use.

The README now opens with what to know before plugging in (beta software, a low volume, cheap
headphones first) and sets up Home Assistant and an AI assistant in a few lines.

## 0.31.1

The official app 1.12 shows the Bifrost 3; nothing else on the wire changed. A run of the app
against the simulator found two gaps, closed here.

- **A write of the value a device already holds no longer fails with "no confirmation within
  2s"** when the device stays silent: after the usual wait, svolder answers with the value the
  device reported on this link, if it matches. A device that answers a repeat still answers at once.
- **The simulator and the fake device report the tone-control mode only when it changes**, as the
  official app needs (it answers every report of the mode with a write of it).
  `SVOLDER_FAKE_QUIET_SAME=1`, or `svolder-sim --quiet-same`, makes every control silent on a
  repeated value.
- **The simulator advertises again after a link closes**, in both layouts, so a phone finds it
  after a disconnect. `make e2e` checks it, and `SVOLDER_E2E_LEGACY=1` repeats the check in the
  layout a phone scans.

## 0.31.0

The web page gets themes: a shared core and frame, with today's look as the first theme, Ember.

- **A theme menu on every screen**: the round button left of the link's pill opens the Theme row,
  the theme's own rows, Device and Sign out (or Sign in), by mouse, touch or keyboard, for a guest
  too. `T` opens the theme **picker** from any theme; each browser keeps its own choice, and
  `?theme=ember` in the address always gets Ember back. Ember is the only theme for now.
- **Arrange sections** (`A`, or the menu's *Arrange sections…*): drag Ember's sections between the
  band across the top and the two columns, by handle or with the arrow keys; the order is kept in
  this browser, per device kind.
- **Hops**: `[[hop]]` blocks in the configuration declare what the sound goes through after the
  DAC (an amp, headphones, speakers), named by hand with a kind or linked to a configured device,
  with an optional note. `svolder hops` lists, adds, links, edits, removes and moves them, and the
  web API writes them the same way.
- **The page is drawn from one model**: the daemon sends the page a JSON model and a JSON event
  stream, and every write answers JSON; safety stays in the daemon (the volume rule, the slot-save
  confirmation, write pacing, sign-in and guest limits) unchanged.
- The sign-in page keeps Ember's look in every theme.

## 0.30.1

Fixes after the Loki Max presets.

- **TUI band presses on a Loki Max no longer queue behind each other**: a band write sent without
  waiting lets the next write go at once; only a preset recall, or a built-in setting, waits up to
  2 s for the band's report. A band whose report is late, or that was written again before it
  came, counts as unread after the next recall, so a setting writes it instead of trusting a report
  from before the recall.
- **The MCP question for a built-in setting names the level it was judged against**, and a `yes`
  counts only under that level, even when a band reports a lower one during the call.
- **A profile cannot make a device-slot action change the volume or the mode**: the loader
  refuses a `changes` list that names either.
- `svolder set save N --json` off a terminal without `--yes` prints its refusal as one JSON object.
- On a device with no volume or no toggle (the Loki Max), the Device screen's `+/-` and
  `Space`/`Enter` hints are dimmed with the reason, instead of lit and doing nothing.

## 0.30.0

The Loki Max's presets: its three device slots and its five built-in settings, from every front
end.

- **Recall and save the Loki Max's three device slots**: `p` in the TUI lists Preset 1 to 3, Enter
  recalls one and `u` saves the bands into it after `Save the bands to preset 2? [y/N]`;
  `svolder set recall 2` and `svolder set save 2` do the same from the CLI (a save asks on a
  terminal and needs `--yes` off one). The web page recalls a slot and saves to one behind a
  confirmation, Home Assistant's `Preset` select recalls one, and the MCP server recalls one after
  asking, since a slot can set a band as high as +12 dB. A save is offered only where it can ask.
- **The five built-in settings** (Flat, Bass Boost, Treble Boost, Smiley, Loudness) apply from the
  same list, `svolder preset list` and `svolder preset apply NAME`, the web page, Home Assistant
  and MCP, as one batch that lowers bands before it raises any. The one the bands match is marked.
  Over MCP a setting that raises a band more than 6 dB above its level of the last minute asks
  first.
- **A Loki Max's bands are read whichever way it reports them**, on the band's own id or the
  shorter one the official app's presets write.
- **After a recall, svolder asks the Loki Max for its bands once** if none are reported within a
  second, so no screen keeps the bands from before the recall. Nothing has been tried on a real
  Loki Max yet; the known issues list what a unit has to answer.
- Profiles gain an `action` kind (a command with a slot and no reading), `reads` (other ids a
  control is reported on) and a table of built-in settings; `svolder profiles show` lists them.

## 0.29.5

Ready for the Bifrost 3: firmware versions are asked for the way the official app asks, and the
last known bugs and the flaky tests are gone.

- **Firmware versions are asked for one at a time**, 300 ms apart and each after the previous
  answer, as the official app does, without holding up anything else on the link.
- **The signal path knows which USB port the computer is on** on a Gungnir 2 or a Yggdrasil: a DAC
  listening to USB 1 while the computer plays to USB 2 no longer reads `untouched`, and when both
  ports read connected the path says it cannot tell.
- **Switching to a device the daemon finds just then opens it**, instead of landing on the Devices
  screen with a failure that has already passed.
- Under the hood: three tests that failed now and then (a script run while another test forked, the
  squirrel click under load, the demo sweep's lock file) are fixed for good.

## 0.29.4

The web page's tone curve makes 0 dB easy to hit.

- **Dragging a band's point up or down sticks at 0 dB** while the pointer is within 10 px of the
  0 dB line (less on a short phone graph, never more than 1 dB), so a band goes back to doing
  nothing without aiming for one pixel. Outside that zone the point follows the pointer as before.
  The TUI's drag already treats the 0 dB row as 0 dB.
- **The protocol notes say how the official app paces its writes**: no rate limit, one write per
  step of a drag carrying only the value that moved, nothing on release, and nothing on a timer.
  They also record that svolder asks for firmware versions all at once where the app asks one at a
  time, a difference the Bifrost 3 will test.

## 0.29.3

The folded Now playing title, slower and seamless.

- **A title too long for the folded line loops slowly** to the left at 30 px a second, followed by
  a copy of itself after a short gap, so it comes round without a jump or a rewind. It rests two
  seconds at its start each lap, keeps its place when the line is redrawn for the same track, and
  stops when the card opens or the page is hidden.
- **The title is measured again when the web fonts arrive**, so the loop fits the text that shows.

## 0.29.2

The phone's folded Now playing line, motion for everyone, and fresh pictures.

- **The folded Now playing line on a phone keeps play and next.** Play is a round button and next
  a bare one beside it; previous waits for the open card. The fold arrow is a small chevron in the
  bottom-right corner, where the progress bar now stops short of it.
- **A title too long for the line fades at its right edge** instead of ending in "…", and glides to
  its end and back once for each new track and when the page shows again, in under five seconds,
  then rests at its start.
- **The small motions no longer follow the system's "reduce motion" setting**: the knob's glide,
  the meters' fall, the tone curve's easing and flashes, the glow's turn and the link dot's pulse.
  Only the first visit's default effect does (Glow instead of Smoke), and the effects menu's Off
  stills the page.
- **The README's web pictures are new**: they show Now playing, the visualizer and the strip.
- **`SVOLDER_FAKE_TITLE`** names the fake host's track, to see a long title on the fake.

## 0.29.1

A burn-down: what 0.29.0 left for later, the cheaper 0.28.0 known issues, and a switch for the
visualizer.

- **A "Visualizer" switch in the effects menu.** Off, the card is gone, nothing is captured for this
  page, and on a desktop Now playing takes the whole band at a lower height. The page remembers it
  and a reload starts without the card, so nothing moves.
- **A woken page keeps its glow.** The wait for the first reading dims the controls only, and a
  screen with nothing to read is ready at its first event instead of after five seconds.
- **On a phone the volume comes before the visualizer** for the keyboard and a screen reader.
- **A signal path too long for the card ends in a "…" chip** that names the hops left out; the
  verdict stays first.
- **A `?` key list for guests** and for pages with writes off, with the keys they have.
- **The TUI's Spectrum row never goes blank while it joins the daemon's frames.** Its own capture
  runs until the daemon's first frame lands, and a slow daemon has three seconds to answer.
- **A 174 BPM kick reads 174, not 87.**
- **Now playing, fixed.** A player whose pid could not be read is asked again; a position read just
  after a new track is read again half a second later; a player stream playing elsewhere wins over
  one not linked anywhere; a stream into the computer EQ names the sink the EQ feeds.
- **A session bus that goes away, or does not answer, is looked for again every 10 s** by the daemon
  and the TUI, and a Settings switch of Host audio no longer stalls the TUI while it refreshes.

## 0.29.0

The computer's music, drawn live. The daemon listens to what the computer plays, and the web page
and the TUI draw it and move with it.

- **A visualizer card on the web page.** It stands in the band beside Now playing on a desktop (the
  right half) and under the volume on a phone, and draws what the computer plays in three looks:
  lumiBars soft (the first), a spectrogram that drifts by, and lumiBars. A tap on the card, or `v`,
  goes to the next look, a small toggle in the corner adds the graph overlay (off at first: each
  channel's curve and its max hold, the highest level each frequency reached), and the page
  remembers both. The card and Now playing are translucent, so the smoke shows behind them. When the
  computer plays to a sink that is not this page's DAC, the card says which sink it shows; with no
  capture possible it says why, quietly. The card keeps its size either way, so nothing moves when
  it starts.
- **The daemon captures only while someone watches, and only listens.** One capture serves every
  page and TUI; it starts with the first one that looks and stops two seconds after the last, and
  with nobody looking nothing is captured or analysed. The capture is passive, so it does not keep
  the sink awake, and the daemon never sets a volume or a mute while watching.
- **The TUI's Spectrum and Level come from the daemon when joined.** A TUI joined to a daemon draws
  the daemon's analysis instead of recording the computer itself, and with no daemon, or an older
  one, it records as before. A beat makes the Spectrum row and the breathing borders pulse.
- **The smoke and the glow move with the music.** While the visualizer runs, the glow's level
  follows the music's loudness as well as the volume, and a beat sends a puff of smoke from the
  volume dial.
- **A hidden tab lets go of its streams.** A page hidden for two seconds closes its event streams
  and opens them again when it is shown, so a tab in the background costs the daemon nothing, not
  even the route reads. A woken page waits dimmed for its first reading, so a step from a stale
  level cannot be taken.
- **A "Visualizer delay" row** in the Device fold-out shows how far behind the sound the card draws,
  while it runs.
- **Fixed.** A flaky TUI test (a joined press with no player raced the daemon-version toast), and
  the web checks' Firefox runs, which could collide on geckodriver's fixed websocket port.
- **The capture leaves the sound card's rate alone.** `pw-record` is passive and asks for no rate
  of its own, so with several allowed rates the graph follows a 44.1 kHz or 96 kHz player while
  the visualizer watches; `make rate-check` measures it by hand. A known issue was added: on a
  phone the visualizer comes before the volume in the tab order.

## 0.28.0

Now playing on the web page, and the signal path from the player to the DAC. The daemon follows the
computer's music player and the way its sound reaches the DAC, and every front end reads the same
thing.

- **Now playing card.** The web page shows what the computer is playing: the cover, the title, the
  artist, the album and the player's name, ◀◀ ▶/▮▮ ▶▶ buttons that go to the player, and a progress
  bar with the time played and the length that moves on its own. On a desktop it is a band under the
  header, across both columns; on a phone it is one line above the volume that a tap opens into the
  whole card and a second tap folds. It follows the player that is playing, as the TUI does. With no
  player it says so in one quiet line. A guest, and a page with `[web] writes` off, see the card
  without the buttons. Amps' and preamps' pages get it too.
- **The card never moves.** It has the same size with a cover, without one and with no player, and
  a changed path stays inside it, so the volume under your finger does not jump when a track
  changes.
- **The signal path.** One line from the player to the DAC, on a DAC's page and in the TUI: it says
  `untouched` only when every hop is known to leave the signal alone (one rate from the player
  through the computer to what the DAC reads, volumes at 100 %, no mute, no computer EQ, nothing
  else mixing in), and otherwise names what changes it (`resampled 44.1 → 48 kHz`,
  `computer volume 54 %`, `host EQ on`, `2 other sounds mixing`, `DAC reads 44.1 kHz`) or what it
  could not read. When the computer plays to another sink, or the DAC listens to an input the
  computer does not feed, it says so and gives no verdict. It never says "bit-perfect": svolder
  cannot see the bits.
- **The TUI reads the daemon.** A TUI joined to a daemon takes now playing and the path from it and
  presses the transport through it, with a new Path row under Transport in the Host group; with no
  daemon, or a daemon that does not watch the computer, it follows the player itself as before.
  Switching Host audio in Settings takes effect at once.
- **A `[host]` table.** `[host] enabled` (on by default) lets svolderd watch the player and the
  route; `enabled = false` or `SVOLDER_HOST=off` in the daemon's environment turns it off. A daemon
  in a container has no session to watch and says so.
- **Covers are read locally, never from the network.** The picture is the one the player published:
  a file it wrote, or the cover in Spotify's own cache. svolder makes no request to any server for
  a cover, and a track whose picture is not on the disk simply has none.
- **Reading the route is light.** The daemon reads the computer's audio graph only while a page of a
  DAC or a TUI that shows the path is open, at most once in two seconds, and it only reads.
- **`make demo`** runs a fake player and a fake path, so the demo's page shows the card.

## 0.27.3

The mouse wheel on the web page's tone curve keeps adjusting the band it started on even when the
pointer drifts off its point, and a point is easier to catch with the wheel. The page scrolls
again as soon as the wheel stops changing the Q, so a trackpad near the graph never holds it.

## 0.27.2

Every library svolder builds on moves to its newest release, among them the Bluetooth stack
(btleplug 0.13.5 with bluez-async 0.9, which also opens adapters whose Modalias is of the
`bluetooth:` kind), tokio, rustls, hyper and the MCP server's rmcp. Building from source now needs
Rust 1.93 or later, the version svolder is built and tested with.

## 0.27.1

Small fixes after 0.27.0 on the web page's curve. A second drag can start right after the first
one, while its last changes are still on their way, and the device ends where the second drag
left the band. A wheel turn just after a drag keeps the Q it set, a quick wheel burst follows a Q
changed elsewhere, and a refused wheel step puts the device's Q back. A drag sends no repeated
value, and a touch-cancel or a third finger in a pinch ends cleanly. `svolder tui --demo`, broken
since 0.26.0, opens its fake device again, and so does any TUI started while the daemon is still
finding its device. The rest is under the hood: the curve's checks compare more of the page's
numbers with the core's.

## 0.27.0

Drag the Response graph's points on the web page, with a mouse or a finger.

- **Drag a band's point.** Press a band's point and drag it: across sets the frequency, up and down
  the gain, and the device follows the pointer while the curve, the peak line and its clip note
  follow at once, with a label beside the point (`1200 Hz · +1.45 dB · 1.40`). Each direction stays
  still until the pointer has gone a few pixels along it, so a sideways sweep does not change the
  gain. In Flat a drag works too and switches the mode to EQ.
- **Q by wheel, right drag or pinch.** The wheel over a point steps its Q a notch at a time, a right
  drag sideways widens and narrows the bell, and two fingers on the graph pinch it. The page still
  scrolls past the graph: a finger put down away from a point is the page's.
- **Points sit at their own gains.** A band's point is drawn at its own frequency and gain, as in
  the TUI, instead of on the summed curve, and in Flat it can be grabbed.
- The key list (`?`) says how the curve is dragged. The tone part waits to be redrawn while a point
  is held, so the graph does not jump back under the pointer.

## 0.26.1

Small fixes after 0.26.0. `svolder identify` on a device the daemon has not found yet says when it
tries again, even with several devices configured. The web page's Device section, opened after you
were signed out, now takes you to the sign-in page, and it sends its table less often while
readings come and go. The rest is under the hood: the end-to-end test copes with a slow second
identify, and the safety-test list says what it guards.

## 0.26.0

Show everything: every fact svolder knows about a device is now on screen in the TUI and on the web
page, so one look (or one screenshot for a bug report) says which device, which firmware, which
input, how good the radio link is and what is wrong.

- **Identify.** Ask a device to show itself on its front panel: `svolder identify`, the `b` key in
  the TUI, the Identify button in the web page's Device section, the MCP tool `identify` and an
  Identify button in Home Assistant. At most one request every two seconds goes out. What the unit
  does is not yet seen on real hardware; a Mesh Card offers no identify, as in the official app.
- **Firmware and DSP versions.** After each connect svolder asks Yggdrasil, Gungnir 2, Bifrost 3,
  Mimir, Eitr 2 and Mesh Card for their firmware versions (main, and DSP where there is one), as the
  official app does, and shows them everywhere.
- **Signal strength.** The strength of the advertisement svolder heard just before connecting, with
  how long ago it was heard: in the TUI, on the web page, in `svolder devices` and in Home
  Assistant. While connected, BlueZ gives no live value to an ordinary user, so none is shown.
- **The web page** gains a Device section under each device, closed until you open it: product,
  unit, firmware, DSP, profile, link, signal, and a table of every reading with when it was heard.
  Every active fault now shows in the card at the top (titled "Fault" or "N faults"), for every
  kind of device, and only there.
- **The TUI's Status pane** gains a Device row (product, product id, profile, signal), writes flags
  in both states ("USB not connected"), and names a refused passphrase or stale readings; the
  Devices list gains a signal column.
- **`svolder devices`** ends each line with the signal and uses the same link words as the TUI and
  the web.
- **Home Assistant** gains Signal, Firmware and DSP firmware diagnostic sensors.
- **Custom profiles:** a control keyed `link`, `signal`, `firmware`, `dsp_firmware`, `identify`,
  `eq_mode`, `eq_preset` or `eq_loudness` now fails to load, since those names belong to the MQTT
  bridge's own entities; rename it. A control on the passphrase function or the firmware-update
  family also fails to load.
- Fixed: after the passphrase in the configuration was changed while the device was answering a
  refresh, svolder could take the old code's answers as proof the new code worked.

## 0.25.6

The web UI is no longer marked experimental, and the README is now a short front page: what svolder
is, how to install it, the commands to start with, and where the details live.

- Signing in from a phone: a camera that previews the QR code can use its link up before the
  phone's browser opens it. The TUI's Web UI box now notices and shows a fresh link and QR code, so
  a second scan works.
- The web page's volume number stays centred when it is negative: the minus hangs to its left.
- The "Climbing to …" popup stays six seconds instead of vanishing the moment the volume gets
  there; stopping the climb yourself still takes it away at once.
- The web page's section titles are set in Geist (SIL Open Font License, listed in
  THIRD-PARTY-LICENSES).

## 0.25.5

Small fixes. When `svolder capture` stops a daemon you started by hand and the daemon is slow to
let go, the message now says so once instead of twice. The rest is under the hood: tighter checks
on the release workflow and the test suite, and clearer documentation.

## 0.25.4

The first release with downloads: ready-made packages for PCs (x86_64) and for 64-bit ARM boards
such as the Raspberry Pi 3, 4 and 5 (arm64), so svolder installs the way your system expects
instead of being built from source. The binaries are static, so one build runs on any Linux of its
architecture, and every file is built reproducibly: anyone can rebuild them byte for byte.

**`svolder capture`** (new since the last source-only version) records everything svolder can
learn about your device into one text file you can send us: what it advertises, every reading,
and every message both ways while you press its buttons. Your device's unit id, its passphrase,
your home directory and Bluetooth addresses are scrubbed out by default; read the file before you
send it. It is the most useful thing an owner of a Tyr, a Loki or a Bifrost 3 can do for svolder,
since we cannot test those ourselves.
