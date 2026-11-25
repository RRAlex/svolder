<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Changelog

Each version below becomes the top of its release notes. 0.25.4 is the first version with
downloads; the versions before it were source only.

## 0.39.8

- **The volume works the same in every theme, from one place.** Ember's knob, Paper's and Signal's
  volume and Night drive's dash now share one volume ask in the page's core: one cap, one way to
  read an answer, one rule for a repeated press. A press that would change where the volume ends up
  is always sent, so a quieter press is never lost behind a slower raise; a press that changes
  nothing is not sent. Paper no longer sends the device's volume from a drag that carries on after
  the computer's side is shown, and a press held on the target while the device climbs to it is
  sent again once, which the daemon folds into the same glide.
- Contributors: the build images retry each package download five times, and CI shows the release
  build image's whole log, so a package mirror that answers 503 no longer fails a release build
  quietly.

## 0.39.7

A release for contributors: nothing you use changes.

- Contributors: `make test` links with lld when `ld.lld` (or a versioned `ld.lld-NN`) is installed,
  and every dev and test target of the Makefile uses the same linker setting, so going from one to
  another rebuilds nothing because of the linker; release builds, packages, Docker images and CI
  keep the system linker (`NO_LLD=1` turns lld off). `make test NEXTEST=1` runs the tests under
  cargo-nextest, with the preamp lab and loudness tests and the tests that start a real MQTT broker
  in capped pools. The scenes event test waits for a later event that reaches every page instead
  of a fixed window.

## 0.39.6

A tidy-up release: nothing you use changes.

- **Less dead code.** Test-only helpers in the host, IPC and protocol crates are compiled only for
  tests, and code nothing called any more is gone from the web pages (core and the themes):
  helpers, word keys and CSS rules left over from earlier layouts.
- **Known issues brought up to date.** Entries for things already fixed are gone, and timeouts,
  sizes and file pointers that had moved are corrected.
- Contributors: svolder-host builds at opt-level 1 in dev and test builds (its lab and headroom
  tests run in about half the time, same results); the web check runner weighs each check by its
  measured duration; the mascot and MQTT reconnect tests run shorter with the same checks, and a new
  test pins the reconnect wait's growth; the MQTT bridge tests settle on the set of what the bridge
  published, in any order, so a loaded machine no longer fails them; `check-no-bluez` and
  `check-tests-private` are listed in `.PHONY`.

## 0.39.5

- **A volume glide that waited is judged again when it goes out.** A glide queued behind a write
  that was slow to answer kept the volume it had read before it waited; if the device's knob was
  turned down in the meantime, the glide could still be sent whole (54 straight to 99 on a
  Gungnir 2, where the volume rule allows a few steps at a time above three quarters of the range).
  It is now judged from the readings at the moment it is sent: it goes no higher than three
  quarters of the range (or the level the volume already had above that) and stops there instead
  of climbing on.
- **The TUI's history graphs keep what they drew.** The host level line, the meters and the
  Telemetry graphs no longer redraw past stretches at other heights from one frame to the next: a
  drawn column keeps its height while the graph's span and width stay the same, and the graph moves
  on one column at a time.
- **The TUI's focused Transport key is boxed whole.** The highlight now covers the whole key, a
  cell either side of the glyph (`▶` included), and is exactly the area the mouse takes for it.
- **Signal: the meters beside the volume, the hops evenly spread.** The computer side's level
  meters sit to the right of the volume, above the tone curve, at every width (they were a third
  column on wide windows). The hops on the signal line stand at 1/(n+1), 2/(n+1) … of its width,
  so three hops sit at a quarter, the middle and three quarters; a name too long for its place is
  cut with an ellipsis and shown whole as its tooltip.
- **New pictures in the README:** the computer side's tone screen with the stages strip and the EQ
  panel open, and the web Sound popup on its EQ library tab.
- Contributors: `make screenshots` also draws `docs/images/computer.webp`, and
  `scripts/web-shot.py --sound` (run by `make web-shots`) draws `docs/images/web-sound.webp`; the
  history graphs lay their column grid from a series' first sample (`history::columns`); the
  release workflow refuses up front when the tag already has a published release.

## 0.39.4

- **The computer's chain shows its stages and what it plays at.** Everywhere the computer's side is
  shown, each stage (EQ, Loudness, Space) now reads with its level and ends on the daemon's own
  words for the whole chain: `plays −14.8 dB in all`, `climbing to 0.0 dB`, `pass-through 0.0 dB`,
  `pass-through, untouched`, or `pass-through −7.6 dB until the music pauses` for a cut that is
  held.
  The TUI's tone screen has a strip, `EQ · Loudness · Space · Bypass`, with one panel under it in
  the place of the Points pane (the Points table, the Loudness rows, the Space rows or the Bypass
  row); `Tab` and `⇧Tab` step the stages, `␣` turns one on or off, `L` and `X` open the Loudness and
  Space panels, and `f` is the bypass. The four web themes draw the same line under the mode (a
  stage word opens the Sound popup at its tab), Ember and Paper with a `Bypass` toggle, Signal with
  a `Bypass` chip, Night drive with `SPACE −7.0 · −14.8 dB` on its EQ line and a `BYPASS` key. In
  the TUI the panels replace the old Loudness and Crossfeed sections, the crossfeed model is a row
  of its own, and `←→` on Plays only highlight (`Enter` plays the choice).
- **A bypass for the whole computer.** One change turns the EQ, loudness, crossfeed and virtual
  speakers off and, turned off again, brings back exactly the ones that were on, with their config
  as it is now. It is `svolder host bypass on|off` (bare `svolder host bypass` says where it stands,
  `--json` gives it with the levels), the TUI's `f`, the web themes' Bypass control, a Bypass switch
  in Home Assistant, the MCP tool `set_host_bypass` (listed only while `[mcp] writes` is on) and the
  GNOME indicator's Computer EQ switch, which now means "the computer changes the sound". Turning
  any stage on from anywhere ends it, and it is kept across a restart. `api/v1/host` carries the
  bypass and the levels. An older daemon is told so in words, and an older indicator or card keeps
  what it had.
- **Turning a stage off no longer jumps the sound.** Crossfeed off, the virtual speakers off, the EQ
  off into a pass-through, the bypass, and a stage that goes off by itself start at the level you
  were hearing and climb to 0 dB at 6.4 dB a second (the pace the DAC's own volume rises at), so the
  speakers' roughly 7 dB no longer arrive at once. A hand edit, a preset load or a scene recall
  still plays at its own level at once. Loudness turned off as the last stage glides to flat instead
  of snapping.
- **A pass-through can be called untouched.** A chain svolder starts is now written from an identity
  config (every filter flat, no stage) and gets its curve and stages live, so a change of the
  output's sample rate can only put back a flat chain: a pass-through can climb to 0 dB, and a
  softer preset plays at its own level at once while music plays (`level held until the music
  pauses` remains for a chain the command line applied or an output that takes no live changes). A
  pass-through that reached exactly 0 dB on such a chain, with every port read at its identity
  value, is `pass-through, untouched` and the Path's computer hop is green; anything else stays
  yellow. A daemon that restarts over a chain it did not start never says untouched until its next
  apply.
- **The idle pass-through goes sooner.** A pass-through kept for a stream goes after 3 s of nothing
  playing (three looks in a row), not 30 s; a seek, a track change or a pause of two seconds keeps
  it. `SVOLDER_EQ_IDLE_TICKS` is kept.
- **Home Assistant.** The computer's device gains a Bypass switch (a binary sensor while commands
  are off) and three dB sensors, Chain level, EQ level and Space level, while the daemon serves
  them. The Chain level sensor carries the daemon's words as an attribute, and the chain card shows
  them over its modules beside a Bypass key; the dashboard has a Bypass badge.
- **The GNOME indicator** shows the chain's words on the Computer EQ row and flips its switch with
  the bypass; an older `svolder` keeps the EQ-only switch.
- **For contributors**: `ChainLevels`, `BypassView`, `HeldStages` (svolder-host `eq::levels`),
  `EqView.levels` and `.bypass`, `EqControl::bypass`, `HostEqOp::Bypass`, `HostEqState.bypass`
  (`[bypass]` in host-eq.toml, kept by a daemon's own apply), `ApplyRequest.identity`,
  `EqControl::{preamp_target, climb_ceiling, start_climb, climb_unless_hands, untouched}`
  (`eq/control/climb.rs`), `ChainStages.untouched`. The preamp lab has a seventh profile,
  `generate_climb`, and a level rule (never up by more than 0.5 dB between two states); host-e2e has
  phases P0 to P5 (`make host-e2e`). Web core: `levelsOf`, `bypassOf`, `actions.computer.setBypass`
  and a `{computer: 'bypass'}` contract target. `PanelComputerEq.level` and `.switch` are additive
  (panel `v` stays 1). The TUI's `app/stages.rs`, `screens/tone/stages.rs` and the design page
  `docs/design/tui-stages.md`; Home Assistant's `computer::bypass` and the Chain level sensor's
  `attributes` topic.

## 0.39.3

- **The Path works with a player that has no MPRIS name.** mpv without its MPRIS plugin used to
  leave the Path row on `nothing playing` while the music played. With one stream playing into the
  DAC's sink, the path is now judged from that stream on, and the player hop reads
  `player (no MPRIS)` with a dim `?` (it cannot be named or controlled). With several streams and
  no MPRIS player nothing is judged and the row says `no MPRIS player`; `nothing playing` is left
  for when no stream plays at all. The TUI, the web page (all four themes), the GNOME indicator
  (`Player (no MPRIS)`), `svolder host check`, MCP and Home Assistant all show it. The daemon also
  reads the route again at once when an MPRIS player appears or goes. On its own (no daemon, no
  `--player`) `svolder host check` takes a lone stream as the player's (`player assumed`). With
  nothing playing the path words in MCP, Home Assistant and `--json` now read `nothing playing`
  (was `no player`).
- **The PipeWire config box shows what goes in and what comes out.** Under `Sink`, two new lines:
  `From player`, the player stream's format as PipeWire negotiated it (`44.1 kHz · S16LE ·
  stereo`), and `To DAC`, what the card is actually open at (`44.1 kHz · 32-bit (S32_LE) · stereo ·
  hw:2`, or `not open`). They are in the TUI's `k` box, the web page's config popup, `svolder host
  check` (text and `--json`) and MCP. In the GNOME indicator, hovering the DAC segment ends with the
  same words (`DAC: untouched · 44.1 kHz · 32-bit · stereo`); a long line wraps at its dots to two
  lines instead of being cut, and the menu no longer changes width as the pointer moves.
- **The TUI's clock has room.** One blank cell now follows the `mouse: on HH:MM` corner, so the
  clock no longer touches the window's right edge.
- **For contributors**: `PlayerMatch::NoMpris`, `Hop.unnamed`, `svolder_ipc::host::{NOTHING_PLAYING,
  NO_MPRIS_PLAYER, plays_without_player, IoLines, io_lines}`, `SetupCheck.io` and
  `StreamFacts.channels`; the host check JSON's `config` gains `from_player` and `to_dac`. The fake
  daemon takes `SVOLDER_FAKE_ROUTE=no-mpris`. `scripts/gnome-extension.sh only FIXTURE [SCALE]`
  runs one headless load.

## 0.39.2

- **The GNOME indicator's Path strip says what it means.** Beside the three segments of the popup's
  Path strip, a green line reads `untouched end to end` when nothing on the way changes the sound;
  what is off still shows there in yellow (`DAC: volume −24.1 dB · tone control`). Hovering a
  segment shows that segment's status on the same line, in its colour (`Player: untouched`,
  `Computer: resampled · sink filter`, `DAC: not read`), and the line goes back when the pointer
  leaves. The words are the Path row's own, so the strip, the Path row and the config box always
  agree. An extension of this version still draws a 0.39.1 `svolder`'s stream as 0.39.1 did.
- **For contributors**: `svolder panel --json` gives every Path segment its `words` and the path its
  `rest` (`{text, tone}`); both are additive and `v` stays 1. `off` is built from the Path row's
  `changes`. `make gnome-extension-check` also loads the extension at text scale 1.25
  (`SVOLDER_HEADLESS_TEXT_SCALE`).

## 0.39.1

- **The PipeWire config box says why the Path row is yellow.** The box (`k` or a click on the Path
  row in the terminal; on the web page the `PipeWire config…` menu row, the `config` chip, or
  Ember's `Path` label) judges only the computer's PipeWire config, so a row turned yellow by
  something else (the DAC's tone control or lowered volume, the player's or the computer's volume,
  another sound mixing in, the computer's EQ, a resample) used to open on seven green lines. It now
  has a `What changes the sound` section naming each hop that changes it (`player`, `computer`,
  `DAC USB 1`) with the same yellow words as the row: under the summary in the terminal, under the
  seven lines on the web page. An untouched path shows no section, and a config that fails alone is
  still explained by its own lines.
- **GNOME indicator install message.** `make install-gnome-extension` now says plainly that
  `gnome-extensions enable` reports that the extension "does not exist" until you log out and back
  in (GNOME Shell on Wayland finds new extensions only at login); the GNOME guide says the same.
- **For contributors**: `PathRow` gains `changes`, one entry per hop with a yellow word, built by
  `path_row` in svolder-ipc; the TUI box and the web popup only draw it. Each `RowHop` carries a
  stable `key` (`player`, `computer`, `dac`).

## 0.39.0

- **A GNOME top-bar indicator** (GNOME Shell 50 and 51; off until you enable it). The DAC sits in
  the bar as a box whose light is the Path row's colour (green when the sound is untouched end to
  end, yellow when something changes it, a hollow ring when the DAC is away or the Path has no
  verdict), its outline tinted by the sample rate (eight rates, one colour each; any other rate
  stays plain), the four-bar Bluetooth signal (yellow when weak, dim when the reading is old) and
  the volume in dB. A muted DAC shows a muted-speaker glyph instead of the figure (and, since a mute
  changes the sound, a yellow light). Scrolling on it changes the volume. Click it for a popup: the
  header (name, rate, signal in dBm), mute, a volume slider that writes once when you let go, input,
  DAC preset, computer EQ (switch and preset), the Path as three green, yellow or grey segments with
  the words only for what is off, scenes, and Open svolder (the TUI in a terminal). It needs
  `svolder` on the `PATH` GNOME Shell was started with (a copy only in `~/.cargo/bin` is not found)
  and a running daemon. Its own calls start none (the stream only connects to the socket, and clicks
  run with `--no-start`, and only while the stream is live); with `svolderd.socket` installed
  systemd starts the daemon when the stream connects, and Open svolder runs `svolder tui`, which
  follows the normal launcher chain. It never confirms anything for you: a call that needs a
  confirmation says so and points you at the TUI. A DAC that is away, or no device at all, draws the
  away state instead of failing; with several devices it follows `-d`, else `default_device`, else
  the only device. When something is wrong the popup says it in one line (`svolder is not on PATH`,
  `the svolder daemon is not running`, and so on) and the stream starts again by itself.
  - **Install**: the `.deb`, the `.rpm`, the Arch `PKGBUILD` and the binary tarball carry the
    extension (in `/usr/share/gnome-shell/extensions/svolder-indicator@rralex.github.io/` for the
    packages); from a checkout, `make install-gnome-extension`. Enable it in the Extensions app or
    with `gnome-extensions enable svolder-indicator@rralex.github.io`; on Wayland a new extension
    shows after you log out and in. [The usage page](docs/usage/gnome-shell.md) has the legend for
    the sample-rate colours and the signal rule.
  - **Signal**: the bars are 4 at -55 dBm or stronger, 3 at -67, 2 at -80 and 1 (weak) below. While
    a Bluetooth LE link is up BlueZ reports no strength, so the figure is the last advertisement
    before connecting and the bars go dim two minutes after connecting.
- **`svolder panel --json`** prints what the indicator draws: one JSON object per line, the first
  as soon as the state is known and then one per change (at most four a second, never the same line
  twice), ending with exit 3 when the daemon stops. It never starts a daemon, and a DAC that is
  away or no device at all is streamed as such, not refused. The shape and the arguments of every
  action are in [the CLI page](docs/usage/cli.md#panel).
- **`--no-start`** (and `SVOLDER_NO_START`), a new global flag: use only a daemon that already
  answers on the socket, and start no socket unit, no daemon and no direct Bluetooth session; exit
  3 when none answers. See [environment variables](docs/usage/environment.md).
- **`svolder host eq on`, `host eq off` and `host eq preset NAME [--yes]`** switch the computer EQ
  and put a preset on it from the command line (the TUI, the web page, MCP and Home Assistant
  already could); `--json` prints the state.
- **For contributors**: `PanelView` in `svolder-ipc` is the one model the extension draws (the Path
  judge's tones and words, one signal rule next to `LIVE_FRESH_SECS`, every choice with the CLI
  arguments that select it); its golden JSON Lines files in `contrib/gnome-shell/tests/fixtures/`
  are written by a Rust test (`SVOLDER_BLESS=1` rewrites them) and read by the extension's gjs
  tests. `make gnome-extension-check` runs the gjs tests, packs the zip and loads the extension in
  a headless GNOME Shell with a fake `svolder`; `make install-gnome-extension` and
  `make uninstall-gnome-extension` handle a source install. The design is in
  [the design note](docs/design/gnome-indicator.md).

## 0.38.3

- **The Home Assistant dashboard leaves no gap.** Each device's "other" list (USB connected,
  Identify, loudness strength, the NOS filter and the like) now sits in the right-hand column under
  Scenes and the computer's chain, instead of on a row of its own under the EQ card; the
  diagnostics follow.

## 0.38.2

- **The svolder Home Assistant dashboard shows every DAC and amp again.** The `custom:svolder`
  dashboard showed only the computer: every DAC and amp section was missing, because it only
  recognised unit ids written in lower case and the daemon publishes them in upper case. Each
  device now gets its faceplate, EQ curve and the rest under its computer.
- **Tone control mode and balance sit where they act.** The tone control mode (flat, EQ,
  loudness) is a row of chips on the EQ card under a "Mode" label, and the balance is a slider on
  the faceplate (a plain readout when the device gives no range or `commands` is off); neither is
  in the "other" section any more.
- **Smaller fixes.** The scenes card's "No scenes saved yet." no longer wraps one word per line,
  and under the `svolder` theme Home Assistant's own select fields are dark with readable text and
  name (copy the new `themes/svolder.yaml` into your Home Assistant to get this one).
- **Contributors**: the Home Assistant check fixtures carry the daemon's real identifier forms
  (upper-case unit ids, a computer id with underscores); new cases in `cards.mjs`,
  `strategy.mjs` and `ha_cards.py`.

## 0.38.1

- **The Path row reads at a glance, in the terminal and on the web page.** The word `Path` itself
  is green when the sound is untouched end to end and yellow when anything changes it, and the
  DAC's own volume and tone control count: a path that is untouched up to a DAC at -24 dB, or with
  its tone control on, is yellow. The verdict word and the check mark are gone. Under the label
  the hops (`player → computer → DAC USB 1`) and the values they share (`48 kHz · volume 0 dB`,
  or `44.1 → 48 kHz` when the computer resamples) sit dim, and only the words of what changes the
  sound are yellow (`computer resampled`, `volume -24.1 dB`, `tone control`): a format that merely
  differs, `S16LE` into `S32LE`, stays plain, and the formats stay in `svolder host check`. A
  `config N` tag appears only when the PipeWire config check fails (and turns the label yellow),
  and a hop with something not read shows a dim `?`. Clicking the row in the terminal (or `k`), or
  the word `Path` on the web page, still opens the config box.
  - **TUI**: the row is two lines, the hops and then the values, each wrapping at its own
    separators (a wrapped line never starts with `·` or an arrow); on a narrow pane the values
    wrap alone and the hops line scrolls, so the values and the `config N` tag stay in sight.
    The tooltip is the label's words, the two lines and the config check's count.
  - **Web page**: Ember draws the same row: the word `Path` (green or yellow; a button that
    opens the PipeWire config popup when the sink is the DAC, its tooltip and screen-reader words
    saying why), the hops as chips with the shared values dim in the last chip, and a yellow
    border and yellow words only on a chip that changes the sound, the DAC's own volume and tone
    control included. The `config N` chip shows only on a failing check. On a card too small for
    the row the plain hops go into the three dots first, then the changed ones, and the DAC's chip
    last. A chip that wraps starts bare, the arrow ending the line before it, a value (`44.1 → 48
    kHz`, `volume -24.1 dB`) never splits, and no line starts with `·`. Signal takes the colour of
    its Path line from the end-to-end result, and when only the DAC changes the sound it says `the
    DAC changes the sound` in yellow instead of `untouched`, its tooltip and screen-reader text
    leading with the label's words; Paper and Night drive keep their own words.
  - **Machine readable**: the web model's `playing.path` gains `row` and `end_to_end`, MCP
    `get_host` `path` and `svolder host check --json` gain `end_to_end` beside `verdict`:
    `verdict` keeps its meaning (the path up to the DAC's input) and `end_to_end` adds the DAC's
    own volume, tone control, balance, mute, phase and NOS filter. Home Assistant is not
    changed and has no `end_to_end` yet (Known issues).
  - **For contributors**: `svolder_ipc::host::path_row` builds the row once (`PathRow`, `RowHop`,
    `RowWord`, `Tone`, `EndToEnd`) from the path line and the config check, and every front end
    draws it; `Hop` records each change at the hop that makes it, with its short form, beside the
    full words the CLI, MCP and Home Assistant keep unchanged. `Hop` and `PathLine` no longer
    derive `Eq`. Ember draws `playing.path.row` in `draw/parts.js` (`pathRow`); `.np-verdict` and
    `.np-lead` are gone.

## 0.38.0

- **Coming from 0.37.2?** Two things matter. Home Assistant: each computer is now its own device,
  named by its host name, so its entities get new ids (`switch.reframe_eq`, not
  `switch.this_computer_eq`); a dashboard, automation or script that names a `this_computer_*`
  entity has to be pointed at the new id, and every daemon on one broker should be upgraded together
  (below). And two safety fixes to the computer EQ's live updates, older than this release (below):
  a line that changed the EQ's points and a crossfeed or speakers stage that costs less could play
  up to 2 dB over unity for one torn quantum, and a stage the controller had lost track of could
  play up to 3 dB over.
- **Scenes**: save the sound of the whole config under a name and recall it later, from every front
  end. A scene holds the computer's EQ (its curve and stated preamp), loudness, crossfeed and
  virtual speakers, and, for each device that was connected when it was saved, its tone control and
  plain sound settings (on a Bifrost 3: input, phase, NOS and balance). A recall puts every part
  back at once, each by its own safe order (apart from PipeWire's half-applied filter under Known
  issues), and the answer says how each part fared (set, set in part, left as it was, failed). No
  volume, gain, mute or power is saved or recalled (a changed input or phono load can still change
  what you hear). A scene is a snapshot of values, so a preset you rename, edit or delete later
  never changes it. A scene recalled onto a device that is not connected leaves that device alone
  and says so; a recall that would play one preset on a device and on the computer at once asks
  first. The daemon keeps scenes and says which one is on, the same everywhere (the scene last
  recalled while it still matches, else the first that matches); the scenes are `[[scene]]` blocks
  in `config.toml` and the scene last recalled survives a restart. A scene with crossfeed or virtual
  speakers on reads as no scene after a restart, since those never stay on. Names are trimmed, at
  most 40 characters, and `custom` is reserved. Scenes need the daemon:
  - **TUI**: a Scenes box, opened by `z` on every screen and by a `Scenes z` row in the Esc menu
    (only while the daemon keeps scenes): one row per scene with a lamp for the one that is on and
    its parts as dim words (`computer · Desk DAC · Hall (not connected)`); `Enter` recalls, `n`
    saves the sound as it is now under a name, `u` saves over the focused scene (asks), `c` renames,
    `x` deletes (asks). A click on a row only focuses it and a second click recalls. A recall that
    would play one preset twice asks its question in the box's last row. The Status pane's first
    line names the scene that is on (`Scene  Late night`, `reading…` while the daemon is still
    reading a device). Every refusal and every part's outcome is the daemon's own words in one
    toast. Without a daemon `z` says `scenes are kept by the daemon: svolder daemon install`.
  - **Web**: a Scenes popup, a writer's only (a guest and a read-only page have no row), opened by
    `Scenes…` in the menu after Sound: the list of saved scenes with a lamp and `On now` for the
    scene that is on, the parts of each as chips (a device that is away dashed), a tap on the name
    recalls and says how each part fared, a pencil renames and a bin deletes (each asks in place),
    `Save current as…` saves (a taken name asks `Save over …?`), and the both-sides question is
    asked in place. In all four themes.
  - **CLI**: `svolder scene [list | show NAME | save NAME [--force] | load NAME [--yes] | rename OLD
    NEW | delete NAME [--yes]] [--json]`. A recall prints one line per part and exits 3 when a part
    is `partial` or `failed`; the both-sides question asks `[y/N]` at a terminal (`--yes` answers
    it, with no terminal and no `--yes` it is refused with the daemon's words); delete asks the same
    way. With no daemon running `list` and `show` read the config (no scene is marked on, no
    connection is said); the other verbs say scenes are kept by the daemon and exit 3, and a daemon
    without scenes exits 7.
  - **MCP**: five tools, `list_scenes` and, with `[mcp] writes`, `load_scene`, `save_scene`,
    `rename_scene`, `delete_scene`. A plain recall asks nothing; it asks when a scene would play one
    preset on a device and on the computer, or sets a built-in band more than 6 dB above its lowest
    level of the last minute (`load_scene` takes `confirm_both_sides` and `confirm` for a client
    that cannot ask, one never answering the other). Saving over a taken name needs `replace: true`
    (asks once); delete asks once; rename asks nothing.
  - **Home Assistant**: the computer's device gains a scene select (`select.<host>_scene`: the
    scenes plus `custom`; a sensor while `commands` is off) and a text (`text.<host>_scene_save`)
    that saves the current settings under a typed name (a taken name is refused in the daemon's
    words). The select's attributes carry the scenes with their parts, whether which scene is on is
    known, the scene last recalled, and the last recall's and save's answers (each with a rising
    `seq`) for the dashboard card; a recall the daemon refuses or asks about (a preset that would
    play twice) shows as `outcome` with the daemon's words. Rename, delete and the both-sides
    question are not on Home Assistant.
- **Home Assistant: each computer is its own device.** Two computers on one broker no longer
  overwrite each other: each computer's device is named by its host name
  (`svolder_computer_<host>`), with its own topics, ids and bridge availability
  (`svolder/bridge/<host>/availability`), and every DAC and amp is `via_device` of its computer. The
  old `This computer` entities are cleared on the first start, so dashboards and automations must be
  pointed at the new entity ids (`switch.reframe_eq`). A DAC that two daemons serve shows under the
  last to announce it; an older daemon on the same broker loses its `This computer` device each time
  a newer one starts, so upgrade every daemon on one broker together; a container without host
  networking needs a `hostname:` or each new container is a new device (see the MQTT guide).
- **Home Assistant: a dashboard that builds itself, and svolder's own cards.** A `custom:svolder`
  dashboard strategy (`contrib/home-assistant/svolder.js` and `svolder/`, one JS resource, nothing
  to install from HACS) builds one page per svolder computer from Home Assistant's registries: the
  computer's section first, then one section per DAC or amp it serves, drawn with svolder's cards.
  The cards are a device faceplate (volume dial with dB and raw/127, Mute and Phase invert keys,
  readouts, input and preset chips, link strip), an EQ curve (drag a point to move a band), the
  computer's chain (EQ, Loudness, Crossfeed, Speakers) and a scenes list with Save and the recall
  answer; one column on a phone and two on a tablet. With `commands` off the cards show the same
  values and move nothing. The cards write only through the entities they are handed, one write for
  each gesture.
- **TUI: a click anywhere on the Path row opens the PipeWire config box**, not only on the `config`
  chip (every line the row wraps to, the label included), and the Host group of the device screen
  leaves an empty line before its Path and Spectrum rows where the pane has rows to spare (and has
  kept the blanks between its groups); the gaps never cut a row, so a small terminal gets fewer.
  A Path hop wider than the pane (the DAC's) now breaks at its commas onto indented lines, five
  lines at most, instead of cutting the whole row with `…`; a line the row breaks onto starts with
  `→` for a hop and with no `·`. The Transport row shows the playing track's time after the
  buttons, `2:41 / 9:22` and a thin bar, advancing once a second while playing (elapsed alone when
  the player gives no length). The README's first terminal image shows the Path row and the time.
- **The Path says `untouched` for players that name no channel positions.** Spotify's two
  unpositioned channels into a stereo sink, and an `AUX0 AUX1` sink against an `FL FR` stream, read
  `untouched` instead of `channels remapped`: PipeWire pairs channels with no names one to one. A
  layout mixing named and unnamed positions with the same set on both sides reads unknown (`channels
  partly unnamed`). One channel into an `FL FR` sink says `mono on both channels` only for `MONO` or
  an unnamed position; `FC` reads `mono centre 3 dB lower on both channels` and `FL` or `FR` alone
  `one channel silent` (both changes). The matching known issue is gone.
- **Two safety fixes to the computer EQ's live updates (older than scenes).** (1) A live line that
  changes the EQ's points and a crossfeed (or speakers) stage that costs less, in one torn quantum,
  could play +0.2 to +2.0 dB over unity (for example a crossfeed drag pending when a point drag is
  released). Such a line now sends the stage first, and when the node may hold the loaded config's
  slots (a sink rate change put them back) it holds the preamp low for both orders, like the level
  held for a stream: it says `level held until the music pauses` and comes up once the music has
  paused and the chain has been idle (one apply). (2) After a command-line apply replaced the
  chain's node, a node that already played the crossfeed stage was taken for one that played none,
  and the next live line that changed the points was sent over a stage whose cost was unknown, up to
  +3 dB over unity in one torn quantum (found by the preamp lab's new boundary check). Such a line
  now first puts the stage to the bypass in a line of its own.
- **Known issues** (all in `docs/known-issues.md`):
  - A volume glide that waits behind a stuck write can be judged from an old volume reading: with a
    knob turned down at the device in the same short window, the glide can jump the volume back up
    to a level it had before the turn (never above it). Rare; the writer was the same in 0.37.2.
    Known, not yet fixed.
  - A scene recall rebuilds the computer EQ's chain when it needs another speakers set, a stage the
    running chain lacks, or live updates are off, as turning that on by hand does, so a player that
    is playing may pause briefly; the answer notes only the set's rebuild. Every other recall goes
    live.
  - A scene recall that changes the number of computer EQ points, or turns the EQ on or off live,
    is one of the live updates PipeWire can play half-applied (known since 0.37.0).
  - Home Assistant cannot answer a scene's both-sides question, and a DAC two daemons serve shows
    under the last to announce it (above).
- **Contributors**: scenes are `svolder-profiles` `scene.rs` (a device's part: `held_controls`,
  capture, `plan`, `matches`; `device_presets::plan_values`), `svolder-host` `eq/scene.rs` and
  `eq/control/scene.rs` (`ComputerPart`, `EqControl::computer_part` and `recall` as one change,
  `ComputerPart::matches`; `EqView.stated_preamp_db`; the preamp lab's sixth profile,
  `generate_scene`, and a boundary check on every line), `svolder-ipc` `config/scenes/`
  (`[[scene]]`, `Config::{add,update,rename,remove}_scene`, `scenes::on`, `Scene::part_names`,
  `ConfigError::SceneName`) and `socket/protocol/scenes.rs` (`Request::Scene`, `SceneOp`,
  `Response::Scene`, `PartOutcome`, `Event::Scenes`, `ScenesView`, feature `scenes`, reasons
  `nothing_to_save`), svolderd `scenes.rs` and `scenes/` (the view watch, the memory in
  `scenes.toml` in the state home, save, recall, upkeep), `Writer::apply_values`,
  `Writer::set_if_needed` and `Writer::busy`, the routes `api/v1/scenes*` with the page model's
  `scenes` and a `scenes` SSE event for a writer, the five MCP tools, `mqtt_bridge/scenes.rs` and
  `svolder-mqtt` `computer/scenes.rs` (a `text` component); `svolder-mqtt` `computer::Computer` and
  `topics::{host_id, computer_unit}`, the bridge reading `/proc/sys/kernel/hostname` once at start
  (`localhost` when empty); the TUI's `boxes/scenes.rs`, `app/scenes.rs`, `run/scenes.rs`,
  `screens/scenes.rs` (`Update::Scenes`, `Intent::Scene`, `menu::entries(app)`, `When::HasScenes`);
  the TUI's `keys::Scene` is now `KeyContext`, the intro's `demo::Scene` `demo::Planned` and the
  stills module `screenshots::shots`; web `core/scenes.js`, `core/scenes-actions.js`,
  `core/popups/scenes/`; `svolder` `commands/scene.rs` and `StubServer::scenes()`; the HA strategy
  `contrib/home-assistant/svolder.js` and `svolder/` with `svolder/cards/` (plain custom elements,
  no import of Home Assistant), checked by `scripts/web-checks/ha/` (`strategy.mjs`, `cards.mjs`,
  `ha_cards.py`); host-e2e phase SC (`HOST_E2E_ONLY=S bash scripts/host-e2e/run.sh` runs it alone);
  `make web-checks-affected` runs every web check that names an id, popup, menu row, note or event a
  change adds or removes, and `make gate-fails LOG=<file>` lists the checks still red in a gate log;
  scratch directories the repo makes in the temp dir live under one `svolder` directory (0700,
  refused unless a real directory the user owns): `svolder_host::scratch::{scratch_base,
  scratch_base_in}`, the preamp lab's `lab-<pid>-<n>`, the unit-test homes `unit-<pid>`, and the web
  gate lock is `/tmp/svolder/web-gate.lock`; the design page `docs/design/scenes.md`.

## 0.37.2

- **Coming from 0.36.7?** This release also carries 0.36.8 to 0.37.1, listed below it. Two items
  matter most. A safety fix (0.36.11): the computer EQ could play over unity, in one case up to
  about 4.5 dB for about a second, in the other with clamped filters after a later restart of the
  chain (up to +36 dB in the worst lab case); both are fixed. And a known issue (0.37.0): PipeWire
  can rarely play a half-applied filter for one quantum after a live update, which can be loud while
  loudness plays at a depth; see Known issues under 0.37.0.
- **The MCP token opens in a box you can copy from.** In the TUI's Settings, Connect category, `v`
  or `Enter` on the MCP token row opens an `MCP token` box like the Web UI box's link: the token
  whole on a line of its own, so a selection with the mouse is the token and nothing else. `c` or a
  click on it copies it, `Esc` closes it, and it closes by itself after a minute (before, the token
  was printed for 10 seconds in the right column, cut into pieces beside the other column's text).
- **How many clients use the MCP server and the web UI.** The daemon counts each server's open
  connections and the distinct clients seen in the past hour and the past day (addresses, not
  people: programs on this computer that use the same loopback address are one), in memory only and
  as counts: no address is stored, logged or shown, and the counts end with the daemon. Only a
  connection that passes the token check counts: a wrong token, a guest or a signed-out visitor
  never does (a server with no token counts every connection). `svolder daemon status` prints `mcp
  clients: ...` and `web clients: ...` lines after each server's line, the TUI's Connect category
  shows the same words under the MCP status and the Web UI row, and the Web UI box has an Activity
  row. An older daemon, or a server that is off, shows nothing. MQTT has no counts, since MQTT
  carries no sender.
- **`svolder mqtt set commands on|off`** turns the MQTT client's command topics on or off from the
  command line, as the TUI's MQTT commands row does. With a daemon it asks the daemon and fails if
  the client still reports the other mode; with none it writes `commands` to the file. With the
  client off, every `mqtt set` verb and `mqtt password` prints what it saved (`mqtt: off (saved:
  host = broker.lan)`).
- **The web UI's Signal theme starts in the Blueprint palette** (it was Warm amber, which looked too
  much like Ember), and Blueprint leads the Palette menu. A palette you picked yourself stays, Warm
  amber included, and is still one click away. The browser's address bar takes Blueprint's blue
  (known issue: it does that for the other palettes too).
- **Contributors**: svolder-ipc `Activity`, `ServerActivity` (`line()`, `short()`),
  `Version.activity` and `DaemonInfo.activity`; svolderd's `activity.rs` tracker (hashed addresses,
  4096 clients, hour and day windows on tokio's clock), `Slot::activity()`, the `Counted` handle in
  each request's extensions and an `Activity` passed to `accept_loop`; the TUI's `Modal::Token`
  (`screens/settings/token_box.rs`) and `WebRow::Activity`; Signal's `style.css` root now holds
  Blueprint's tokens and Warm amber is the `data-palette="amber"` block, `palette.js` lists
  Blueprint first (`DEFAULT_PALETTE`), and the node check `signal-palette.mjs`; the design section
  "Client activity" in `docs/design/daemon.md`.

## 0.37.1

- **The path says `untouched` only when nothing in the computer changes the sound.** The signal path
  line (web, TUI) used to check the rates, the volumes and the filter chain. It now also names a
  sink narrower than the stream (`cut to 16 bits (S32 → S16)`), dither, a channel mix or remap, a
  filter graph in a converter, ALSA `plug` or `dmix` and a card that converts, and a clock rate
  forced so that a stream is resampled (`resampled 44.1 → 48 kHz (rate forced)`). What could not be
  read is said as unknown, never as clean. The note under `untouched` says that the player may still
  change the sound itself (loudness normalization, ReplayGain, its own EQ) and that svolder judges
  up to 24 bits.
- **A PipeWire config check**: seven conditions of the PipeWire config (rates, forced rate, dither,
  24-bit output, no plug, stereo to stereo, no filter in the sink), each pass, fail or unknown,
  with a summary (`PipeWire config lets it through untouched`, `PipeWire config changes the sound:
  2`) and the limit of what it cannot judge. It is read only: the same read-only `pw-dump` svolder
  already runs, plus the card's `/proc/asound` files, and nothing is ever changed. A one-shot
  `host` request (the CLI, `doctor`, MCP) reads it once, shared among requests that arrive together.
  - **CLI**: `svolder host check [--json] [--player NAME]` prints the path from the player through
    the computer to its sink and the seven lines (`--json` has `path.dac` and `config.sink` too, as
    MCP and Home Assistant do; for a sink that is none of your DACs the CLI and MCP judge the
    computer's side alone, where the TUI, the web and Home Assistant read `not_this_dac`); a failing
    line is not an error, exit 6 when the graph cannot be read. `svolder doctor` gains a `PipeWire
    config` row.
  - **TUI**: the Device screen's Path row wears a `config ✓` / `config N` / `config ?` chip right
    after the verdict, and `k` (or a click on the chip) opens a PipeWire config box with the lines.
    Without a daemon the TUI reads `pw-dump` itself while the box is open.
  - **Web**: the same chip beside the verdict in Ember, Signal and Paper, and a *PipeWire config…*
    row in the menu (every theme) that opens a popup listing all seven lines and the limit.
  - **MCP**: `get_host` gains `path` and `config`.
  - **Home Assistant**: `This computer` gains a `Path` sensor and a `PipeWire config` problem sensor
    (`computer_config`), both read-only. While MQTT is connected the daemon reads the route every
    30 s for them.
  - **One set of keys on every JSON surface**: the path verdict is `untouched`, `changed`,
    `not_this_dac`, `other_input` or `unknown`; the config check's outcome is `untouched`,
    `changes`, `unread`, `not_read` or `other_version`, and a line's `pass`, `fail` or `unknown`;
    `words` is a list everywhere; `config.limit` carries the limit sentence.
- **What the verdict covers.** svolder asserts only what enters the system's audio pipeline from the
  player (the player's stream) and every hop after it. Whether the music is a local file or a
  stream, and anything the player does before its stream, is outside it, and svolder reads no music
  file to judge it (only what enters the pipeline from the player). What the check does not see:
  inside a player, past the sink's converter (ALSA `hw`, the USB driver, the DAC), and bits above
  24. These limits are under Known issues.
- **Contributors**: `RouteFacts.setup` (`SetupFacts`) read from the same `pw-dump`, the new
  `Asound`/`fill_alsa` read of `/proc/asound` (`SVOLDER_ASOUND_DIR`, dev-only), `Verdict::KEYS`,
  `SetupOutcome::KEYS`, `SetupSummary::KEYS` and `SETUP_LIMIT` in svolder-ipc as the one judge,
  `computer_line`, `setup_check`, the daemon's slow watchers (`SlowHolds`) and one-shot `host`
  reads, `SVOLDER_FAKE_CONFIG` (dev-only, `fail`) and a changed fake route with a narrow sink and
  dither; host-e2e gains phase B (`make host-e2e-bits`: known 16- and 24-bit samples through pipe
  sinks of a private PipeWire, with negative controls) and phase C (`svolder host check --json` on
  the same rows); the node harness `core-setup.mjs` and the browser check `setup_popup.py`;
  `scripts/docs-check.py` fails on "bit-perfect" or "bit-exact" in shipped words; the design page
  `docs/design/host-config-check.md`.

## 0.37.0

- **Loudness for the computer's sound**: as the computer's volume falls under a reference, a stage
  in the computer EQ's filter chain takes the middle and the top of the band down, so the bass stays
  where the volume left it (after the equal-loudness contours of ISO 226:2003). It only cuts: it
  never raises a frequency and never moves the EQ's preamp, so it is never louder than without it
  (apart from the PipeWire behaviour under Known issues below). It follows the computer's own
  volume: a change made through svolder on the next 50 ms tick, one made elsewhere (a desktop key)
  within a second, the stage gliding about 10 dB of depth a second; muted or unread, it holds. While
  it cannot follow the volume (for example a chain that takes no live updates, or one held for a
  sample rate) a change is still accepted and saved from every surface and takes effect when it can
  follow again. A computer EQ chain made by an earlier version is rebuilt once, the first time
  loudness is turned on, so a player that is playing may pause briefly. Strength is 0 to 100 %
  (default 50 %) and the reference 1 to 100 % (default 100 %), also set to the current volume with
  one key, button or word. The readout says what the stage plays
  at the volume it read (at 35 % with strength 50 %: bass +7.3, mids −7.3, treble +2.1 dB). It works
  with the EQ off (the chain plays flat for loudness alone), stays on across restarts, plays at
  every sample rate up to 768 kHz, and a graceful stop sends it flat. It is not the device's own
  Loudness mode; where both are on the screens say so. It is on every surface, and shown only while
  the daemon serves it:
  - **TUI**: a Loudness row on the computer's Tone pane (`Loudness ‹ on › ████░░ 50 %` with a dim
    line of the stage's gains) and a section on the tone screen with a Reference row; `←→`
    strength, `␣` on and off, `r` the reference is the current volume, the mouse on the switch and
    the bar. The computer's rows and the tone screen's sections now follow the chain's order: EQ,
    Loudness, Crossfeed or Space (the Crossfeed section is under the Points table, not over it).
  - **Web**: a Loudness tab in the Sound popup, second after EQ library, with a switch, a strength
    slider, the reference with "Use current volume", the stage's curve and the live readout, in
    every theme; Night drive's top line says "loudness on".
  - **CLI**: `svolder host loudness [--json] [show|on|off|strength N|reference N|reference now]`.
  - **MCP**: `get_host` carries `loudness`, and the new write tool `set_host_loudness` (under
    `[mcp] writes`) sets it.
  - **MQTT and Home Assistant**: a Loudness switch and a Loudness strength number on `This
    computer`.
- **Fixed: a false "level held until the music pauses" note.** After a preset was applied, the
  computer EQ could say its level was held, and restart the chain once idle, because the preamp read
  back from PipeWire at six decimals (a preamp of −6 or −2 dB read back a hair low) counted as a
  lower cap. The note and the idle restart came with 0.36.11, which was not published; v0.36.7, the
  last published version, does not have them, so no published version showed this note. A preamp
  read back at the configuration's own value now bounds nothing.
- **Fixed: a fresh chain gave up live updates while loudness glided.** With the EQ turned on, a
  preset recalled or the daemon restarted at a low volume, the check that a chain takes live
  changes could count the glide's own lines, which land a tick late, as refusals about half the
  time, so the computer said it takes changes only when a drag ends and loudness froze. The check
  now counts a hit for the value it was armed with or any value sent for that port since. Loudness
  is new in this version, so no published version had this problem.
- **Fixed: crossfeed, speakers and loudness settings changed before any chain was applied were lost
  at a restart.** They are now kept in `host-eq-settings.toml` beside the computer EQ's state file
  and read back at the start (with loudness and crossfeed off, as after any restart).
- **Known issues** (all in `docs/known-issues.md`):
  - **PipeWire can play a half-applied filter for one quantum after a live update.** PipeWire's
    filter chain applies a live control update one value at a time, and nothing stops the audio
    thread from playing in between. Rarely (measured about 1 in 6,000 risky updates on PipeWire
    1.6.2; at ten glide ends a day, roughly once every year or two) one quantum, about 1/50 s at
    1024 samples and 48 kHz (a smaller quantum makes it likelier), plays a half-applied filter. A
    small step is harmless (a loudness glide step: about +0.3 dB). These are loud:
    - the daemon stopping gracefully, a TUI with no daemon quitting, a reconfigure that halts the
      computer's side, or loudness turned off, while loudness plays at a depth: +31 to +43 dB in the
      bass measured, up to +50 dB worked out;
    - the end of a loudness glide back to flat: +29 dB in the bass measured;
    - adding or removing a computer EQ point, a live preset recall that changes the number of
      points, the EQ turned on or off live: an unstable filter or full scale measured;
    - dragging an existing point: up to about +17 dB worked out, not measured;
    - switching the virtual speakers or crossfeed: up to about 6 dB worked out.

    After a bad quantum the chain recovers by itself. Live edits of the computer EQ have had this
    since they began (0.35), so v0.36.7 has it. svolder cannot close it without giving up live
    updates or a much larger redesign; the fix belongs in PipeWire.
  - Each loudness step resets the changed filters' state inside PipeWire, a short (about 3 ms) low
    tick, about 11 dB or more under the music, that a listening test did not hear.
- **Contributors**: `crates/svolder-host/src/eq/loudness.rs` (settings, the maths, the view; the
  constants come from `make loudness-fit`, the ISO 226:2003 fit in `scripts/loudness-fit.py`, which
  needs `python3-numpy`), `eq/control/ld.rs` (the controller: level, glide, `keeps_chain()`,
  `LevelHint` for a lock-free volume write), `HostEqOp::Loudness`, `EqView.loudness`, `[loudness]`
  in `host-eq.toml` and `POST api/v1/host/loudness`; the chain's two identity sections per
  channel (`ld1`, `ld2`) after slot 16 and `route::computer_level`/`Params::level` reading the
  volume from the dump the look already takes; `ChainStages.loudness` and the path word "loudness
  on"; `make preamp-lab` runs five profiles (the fifth, loudness, with the stage in the model, the
  stage alone held to unity, a liveness rule and directed cases for its seeds), and host-e2e gains
  phases L1 to L6; the node harness `core-loudness.mjs`, the browser check `loudness_popup.py`;
  the first-live-value read-back keeps the value it was armed with (`ReadBack`) and
  `FakeParams::lands_one_line_late`; the preamp lab, `FakeParams` (`set_level`, `reads`,
  `with_loudness_ports`) and the TUI's no-daemon controller carry loudness; test sockets and
  temporary directories fit the socket path length under any `TMPDIR`; `HostEqSettings` and
  `host-eq-settings.toml` (settings kept before any chain was applied; a malformed file is logged
  and ignored); `make gate` reuses a passed slow check whose inputs are unchanged and lists every
  check's duration; the preamp lab runs about 5 times faster, with `make preamp-lab-outcomes` and
  `TRACE=1`; the env table gains `SVOLDER_LAB_OUT`, `SVOLDER_LAB_THREADS` and `SVOLDER_LAB_TRACE`;
  the MCP `get_host` loudness field is `lift_db`, as in the view.

## 0.36.11

- **A preset recalled while music plays no longer pauses it**: loading a preset on the computer
  while music plays used to restart the computer EQ's chain, which paused players such as Spotify
  (going from a curve with a deep preamp, such as -6.2 dB, to Flat). Now the preset plays at once
  with the level held at the earlier curve's preamp, the computer side says "level held until the
  music pauses", and about 30 seconds after the music stops the chain starts again at the preset's
  own level.
- **Safety: the computer EQ could play over unity for up to about a second.** Two cases, both fixed.
  With crossfeed's Meier or M/S bass model held in the EQ's configuration (after a live update
  failed), dragging an EQ point up as the sink changed rate could play up to about 4.5 dB above
  unity. And the computer EQ could write its configuration at a sample rate its curve cannot play
  (while waiting for a rate it plays at, or within about a second of reading a command-line apply);
  a later restart of the chain then played clamped filters (up to +36 dB over unity in the worst lab
  case) or an unstable one. Both were present in 0.36.7 (the second since the
  virtual speakers, 0.36.0, for one path, and older for the other). Both are gone: the preamp always
  pays for the stage the configuration holds, and no configuration is written whose points do not
  play at its rate.
- **A computer EQ edit could be lost to quick graph rebuilds**: two rebuilds of the audio graph at
  the same sample rate within about two seconds (skipping quickly through tracks of mixed rates)
  could leave the chain playing the curve it was last applied with instead of your edit, while the
  computer side showed the edit as live. A new boost or cut was not heard (never over unity).
  The controller now sends the edit again when a look still finds it missing.
- **After a failed live change the chain is put right**: after a refused line followed by a failed
  turn-off or apply, the computer EQ could keep playing the change it had taken back (flat, or
  virtual speakers on) while the view said otherwise; the next line now corrects it.
- **Crossfeed and the virtual speakers run up to 384 kHz**; an output above that turns them off with
  a note, and turning them on there is refused (with the EQ off too). Meier with a delay may play up
  to about 1 dB quieter at extremes: its level now also covers the moment a graph is rebuilt at
  705.6 or 768 kHz.
- **Contributors**: `EqControl::cap_at` also bounds the preamp by the points' own preamp less the
  configuration's stage cost; `HostEqState` gains `crossfeed_loaded`, priced at the dearest usual
  rate by the next owner, which applies the chain again without it; `LEVEL_HELD_NOTE` and the idle
  lift of a held recall; `STAGE_RATE_MAX_HZ` (384 kHz) while `CONFIG_STAGE_RATES` folds play rates
  up to 768 kHz; `EqControl::reverted` confirms the ports of every line up to the newest one a look
  saw land (`line_of`) and sends a line no look saw land again once (`chased`); `make preamp-lab`
  runs four profiles (ordinary, targeted, speakers, high-rate) and gains `Event::Playing`, directed
  cases for seeds 1291, 172526 and 1203992 and the daemon-restart case, and an ignored
  `stage_cost_table`; host-e2e gains phase R (a paplay stream through a private pipewire-pulse) and
  needs `paplay` and `pactl`.

## 0.36.10

- **Identify from the Devices list**: `b identify` is a word on the Devices list's top border,
  between `update` and `x remove` (click it or press `b`: the device under the cursor shows
  itself), and the device editor (`u`) takes `b` for the device being edited, on every row but
  Name.
- **Connect rows that say what they are**: in Settings › Connect the "MQTT bridge" row is now
  **MQTT client** (as are the CLI help, `svolder doctor` and the daemon's words; config keys, the
  `svolder mqtt` verbs and the MQTT topics are unchanged). MCP server, MCP writes, MQTT client and
  MQTT commands are drawn as choices (`‹ off | on ›`) with the value in force lit; status rows are
  plain text. Turning the MQTT client on with no broker host says "set MQTT broker host first".
  The MCP server and Web UI rows now change as soon as the daemon answers (before, they kept the
  old state until another setting changed).
- **The selection reads clearly**: the Controls pane's Transport and Input rows highlight only the
  label and the value in force, as the Target choice does, not the whole line.
- **The Path line fits**: a path longer than its row wraps at its ` · ` and ` → ` separators onto
  up to three lines when the pane has room, and otherwise scrolls slowly, holding still under the
  pointer; hovering it shows the whole path. A new **Scroll long lines** setting (Settings ›
  Effects, `[tui] scroll_long_lines`) stops it and the Now playing line from scrolling.
- **Contributors**: `switch::styles`/`choice` draw every TUI choice; `widgets/slow_scroll.rs`
  (time-pure offsets), `screens/device/path_line.rs`, `Hit::Path`; the TUI rereads the config after
  every daemon switch answer.

## 0.36.9

- **Signal's popups in its own look**: Devices, Sound, Help and the questions share the theme's
  schematic look: the title is a tab on the box's top edge, dotted wires lead in and out, the side
  list and the Sound tabs are hops on a wire with the chosen one boxed and lit, and a device's
  readings are terminals on a bus with a thin numeral over a small graph.
- **Signal palettes**: a Palette row in Signal's menu: Warm amber (the default), Phosphor, Blueprint,
  and Oxblood and brass. A palette recolours the whole theme, popups included; the hops keep their
  meaning and the computer's level meters keep their warm glow. The choice is kept per browser.
- **The TUI's choices read the same way**: the computer's `Mode ‹ Off | EQ ›` (Tone pane and the EQ
  screen's top right), the Space row and the crossfeed on/off word are drawn like `Target`: the
  value in force is bright, the others dim, and with the keys on a row only its label and the value
  the arrows move take the selected box.
- **A Preamp scale**: the EQ library preview's Preamp row gains a small scale (ticks and a needle) in
  every theme.
- **Contributors**: Signal's colours are tokens end to end (`--sg-*`), the palettes are
  `data-palette` rules in `style.css` with `themes/signal/palette.js`; `contrast.py` sweeps Signal
  once per palette; `switch::styles` and `switch::choice` are the TUI's one drawing of a choice.

## 0.36.8

- **Contributors: a shorter web gate**: seven of the smoke checks (smoke_core, smoke_page, haze,
  effects_cycle, scroll, smoke_quick, sources) take `--part logic|timing|all`; their logic part runs in
  the parallel pool and only their timing part (`NAME_timing-ENGINE`) in the serial lane, beside notes
  and nightdrive_timing, which stay whole. A failed serial check is re-run once alone and listed as
  flaky when it then passes. The serial lane goes from about 35 to about 17 minutes.

## 0.36.7

- **Tests**: the virtual speakers download tests no longer depend on the order a folder lists
  its files in, which differs between filesystems.

## 0.36.6

- **Computer EQ, a safer restart**: PipeWire can give a restarted EQ chain the same node id it
  had before. svolder now tells the chain apart by its serial, which PipeWire never reuses, and
  checks the chain before each change it sends, so a restarted chain is always seen as new: its
  preamp is capped by the settings it started with, and the crossfeed and virtual speakers turn
  off as for any restart. Before, after a boost was softened, a rate change could play a chain
  that came back under its old id louder than unity.

## 0.36.5

- **Tests**: the check that the EQ chain's reaper thread is named waits for the thread to name
  itself, so it no longer fails on a slow machine.

## 0.36.4

- **Phone fixes**: on a phone, Signal's toasts and the "old readings" mark sit above the taller
  Now playing bar, and Ember's path chip for the DAC shows its volume as the bare level, so the
  row of chips stays whole.

## 0.36.3

- **Reconnects come sooner**: the daemon retries a missing device after 2, 5 and 15 seconds and
  then every 30 seconds (it waited up to a minute before), and one connect attempt is cut off
  after 45 seconds with its reason in the log, so a stuck Bluetooth call no longer holds up
  reconnecting.
- **"The svolder daemon stopped."**: when the daemon goes away, the TUI and every command say so
  in those words, with the command that starts it again, instead of a raw "Broken pipe" or a
  claim that the device did not come back; the TUI picks the daemon up again by itself when it
  returns. `scan` and `doctor` on a busy adapter say it is busy with another scan or connection.
- **A write is confirmed by the device**: the answer to a write is the device's own report
  received after it; on a weak link an older report is no longer shown as the answer, and when
  none arrives within 2 seconds the command says "sent, not confirmed" with the device's last
  report (exit code 4).
- **Old readings are marked**: while the link is down, or the daemon stopped, the TUI and every
  web theme mark the readings as from before the drop, until the first fresh reading.
- **The path line names what the DAC changes**: a volume below 0 dB, a tone control that is not
  flat, a balance off centre, mute, phase invert and the NOS filter.
- **Signal on a phone**: the Now playing time no longer runs out of its row; the progress bar
  takes its own line above the buttons.
- **README pictures**: the device screen and the animation show the VU rows, and a new Signal
  phone picture shows the level meters.

## 0.36.2

- **svolder no longer crashes `bluetoothd` on BlueZ 5.87 when it scans**: the scan asks the
  adapter for every device and keeps the Forkbeards by their advertisement, where it used to
  filter by service, which BlueZ 5.87 cannot do without crashing.

## 0.36.1

- **`untouched` says what it cannot see**: the signal path's verdict covers the computer and the
  DAC; what the player does inside itself, such as loudness normalization or its own EQ, happens
  inside the player, where svolder cannot look. The web verdict's tooltip says so, and so do the web and TUI
  guides.
- **Level meters for what the computer plays**: Signal's computer area shows two needle dials,
  left and right, with the reading in VU under each and a lamp that lights when that channel
  reaches full scale and stays lit for 1.2 seconds. 0 VU is a level of -18 dBFS by default; `−`,
  `+` and Reset under the dials move it from -30 to -6 dBFS, a dB at a time. The needle follows the
  RMS level with a VU meter's swing, about 0.3 seconds to settle. The level is read at the sound
  card's monitor, so it follows the computer's volume and mute only where the sink's monitor does
  and includes what the computer EQ's chain adds.
- **Three glows, and a Spectrum view**: a tap on a dial face (or Enter or Space on it) turns the
  glow through subtle, warm backlight and filament, the default; the Meters | Spectrum chips swap
  the dials for a line of the music's spectrum, which Signal did not have before. The glow, the
  view and the reference are kept per browser. The dials sit in a third column from 1240 px wide,
  beside the volume from 720 px, and after the volume on a phone. Nothing is captured while the
  section is off the page or the tab is hidden.
- **The terminal UI has the meters too**: from a terminal of 100 by 45 columns and rows, with host
  audio on, the Status pane under Now playing (and under the cover) draws `VU L` and `VU R`, each a
  needle on a 40-cell track with its reading in VU and a lamp (`○`, `●`). `[tui] vu_reference`
  (-30 to -6 dBFS, default -18; Settings, Host audio, VU reference) sets what reads 0 VU, and the
  `Level from` row says it. A whole number outside the range is held at the nearest end; any other
  value is refused like any other bad `[tui]` value.
- **A peak at full scale is now an over, and only that**: the frames' peak byte reads 255 only for
  a sample at or above 16-bit full scale, and at most 254 otherwise, so a master peaking at -0.1
  dBFS no longer reads as full. A square or clipped master can still light the lamp when it is
  resampled on the way (a 44.1 kHz stream into a 48 kHz graph, say), which overshoots by up to
  1.9 dB.
- **`GET api/v1/host/frames?only=levels`** streams frames with just `t` and `lv` (about 45 bytes
  each instead of about 700), the same `hello` and `capture` events and the same holds; any other
  query is the full stream. Signal's Meters view holds it.
- **Docs**: `docs/design/vu-meter.md` (the scale, the ballistics, the over lamp, the golden file),
  Signal's meters in `docs/design/web-signal.md`, the levels-only frames and the over rule in
  `docs/design/host-capture.md`, and the web, TUI and configuration pages say how to use them.
- **Contributors**:
  - `svolder_host::vu` (pure, no clock): `Scale` (-20 to +3 VU, linear in amplitude), `Reference`,
    `Needle` (second order, omega 14.2 and zeta 0.83, 1 ms steps, a gap over 2000 ms snaps) and
    `Lamp`; `analysis::OVER` and `peak_byte`; `Levels` on `capture::Block`, filled by `Block::of`
    and `Frame::block()`. `make vu-golden` writes the needle's path for a fixed script to
    `scripts/web-checks/fixtures/vu-ballistics.json` and `make vu-golden-check`, in `make check`,
    holds it to the engine.
  - The web core gains `core/vu-model.js`, the page's twin of `vu`, checked against the golden file
    by `node/vu.mjs`, and `core.levels`, the levels-only stream. Signal gains `draw/vu.js`,
    `draw/spectrum.js` and `vu-wire.js`; `signal_vu.py` and `node/signal-vu.mjs` check them.
  - `TuiSettingKind::Number` and `TuiValue::Number` take a whole-number `[tui]` key in the plain-key
    table; `Host` boxes its `bands` and a `vu::Vu` (the loop's by-value bound is 512 bytes);
    `capture_target` and the loop's tick count the VU rows. `make vu-shots` draws their stills.
  - `make host-e2e` gains phase U (the VU levels): `scripts/host-e2e/vu.py`, self-check
    `scripts/host-e2e/vu_selftest.py`.
  - A doc link to a test-only function in the TUI's status code is plain code, so `cargo doc` with
    `-D warnings` passes.

## 0.36.0

- **Virtual speakers**: the computer's sound on headphones can come from two speakers in front of
  you, at about 30 degrees to either side (45 degrees for the With room · BRIR set), in place of
  from inside your head. There are two choices, each shown with its technical term: **With room ·
  BRIR** (the speakers in a measured room) and **No room · HRTF** (the same speakers in the open).
  They run as one more stage of the computer EQ's filter chain, after crossfeed, with no extra
  PipeWire package. The design is in `docs/design/virtual-speakers.md`, the guide in
  `docs/usage/eq.md`.
- **Sets come from a short catalogue, on request**: svolderd downloads the one you pick, checks its
  checksum, converts it to one small file (with exactly 1 ms of silence in front of its earliest
  impulse, so PipeWire's resampling at other sink rates cuts nothing) and keeps only that, in
  `~/.local/state/svolder/speakers/` (at most 32; the oldest unused one goes). The catalogue lists
  five No room · HRTF sets and one With room · BRIR set, all under licences that allow any use, and
  the licence and credit line show wherever a set is chosen. Nothing is bundled and a set's source
  is only ever fetched over HTTPS, from the catalogue's hosts, within 120 s and 64 MiB.
- **Your own file works too**: import a SOFA file, or a WAV of 4 or 14 channels, and it becomes a
  set of its own, with its own licence line.
- **One choice with crossfeed**: Off, Crossfeed or Virtual speakers; turning one on turns the other
  off in the same update, so the Home Assistant crossfeed switch and `set_host_crossfeed` work as
  before. Every set is scaled so that it never plays above 0 dB at any frequency, which makes it
  quieter than Off; the view says by how much ("about 7 dB quieter").
- **Virtual speakers turn themselves off as crossfeed does**: after a restart, when the output
  changes, when the filter chain restarts, and on a graceful stop, each with a note; also when the
  chosen set's file is missing or damaged, with "virtual speakers off: ... is missing or damaged,
  fetch it again". The choice of set is kept. Turning them on rebuilds the chain once (one short
  gap) when the running chain does not hold the set yet, as after a choice made while they were off;
  off is live, and choosing a set rebuilds only while they play.
- **`svolder host speakers`**: `show`, `on`, `off`, `list [--room with|no]`, `fetch ID`, `use ID
  [--on]`, `import FILE [--label NAME] [--room with|no]` and `forget ID`, all with `--json`, through
  the daemon. Ctrl-C during a fetch or an import cancels it on the daemon and exits 130; `show`
  works without a daemon from the saved state.
- **The terminal UI has a Space choice**: the Tone pane's Crossfeed row and the tone screen's
  section become Off, Crossfeed or Virtual speakers where the daemon serves them, with the set, its
  licence line, the angles, the length, the level and two curves; `S` opens the set list (fetch with
  progress, choose, forget, import a file, filter by With room · BRIR or No room · HRTF).
- **The Sound popup has a third tab, Virtual speakers**: the switch in the header, three pictures of
  the chosen set (Arrivals, Directions, Decay: side by side on a wide window, one at a time with
  chips on a narrow one, the chip remembered), the two choices, the licence line, the sets list
  (fetch with cancel, use, forget) and your own file to import (up to 32 MiB). A set with no timing
  data says "Not measured for this set." in Arrivals and Decay.
- **MCP**: `get_host` also reads the computer's virtual speakers, and a new tool `set_host_speakers
  {on?, set?}` chooses a fetched set and turns them on or off; it needs `[mcp] writes` and never
  fetches.
- **Home Assistant**: the `This computer` device gains a `Virtual speakers` switch and a `Virtual
  speakers set` select of the fetched sets (each option as `Label · With room · BRIR` or `Label ·
  No room · HRTF`), with the chosen set's licence, credit line, source page, angles and loudness
  change as attributes; a binary sensor and a sensor while `commands` is off. Choosing a set does
  not turn the speakers on, and nothing is fetched from Home Assistant.
- **The signal path line names the stage**: it says `crossfeed on`, `virtual speakers on` or
  `host EQ on` (or a combination) for the chain in the path, and `EQ pass-through` for the flat
  chain kept while a stream plays, and never `untouched` while a chain is in the path; the Night
  drive's top line uses the same words.
- **Docs**: `docs/design/virtual-speakers.md` (the graph, the conversion, the catalogue, the
  lifecycle and the contract), a Virtual speakers section in `docs/usage/eq.md`, and the CLI, TUI,
  web, MCP, Home Assistant and glossary pages say how to use them.
- **Contributors**:
  - New crate `svolder-speakers` (pure, no PipeWire; the fetch client behind the `net` feature): the
    catalogue, the SOFA reader on `sofar`'s `hdf` module and the `zip` reader, the converter
    (lead-in fit, 0.5 s cap with a fade, the level rule on a 16-times zero-padded FFT, the loudness
    change, the curve, the timing analysis), the store (sidecar, checksum, 32 sets, a pin that holds
    every set a running chain was started with, `list_sidecars` without reading the WAVs) and the
    fetch client; synthetic SOFA fixtures are committed (`make speakers-fixtures`), and `make
    speakers-catalogue-check` (network, not in `make check`) checks every catalogue entry.
  - `svolder_host::eq::speakers`: the set, the view (`SpeakersView`, `SetView`, `OffReason`), the
    oracle `speakers_response` and the convolver stage in the live config (`lvs` and `rvs` mixers
    with dry `Gain 1` and wet `Gain 2` and `Gain 3`; the convolvers are in the config only while a
    set is chosen); `eq::control::vs` is the controller (choose rebuilds while they play, on
    rebuilds when the chain lacks the set, off is live, the check before every apply, the resets,
    the config rewrite without a restart); `host-eq.toml` gains `speakers`, `speakers_on` and
    `speakers_loaded`; `HostEqOp::Speakers` and `EqView.speakers` are on the wire.
  - `svolderd`: the `speakers` request family (`catalogue`, `fetch`, `import`, `forget`, `follow`,
    `cancel`) with fetch and import as jobs, one `Arc<Store>` handed to the host service,
    the web routes `api/v1/speakers/*` and `api/v1/host/speakers`, and `SVOLDER_SPEAKERS_URL`;
    `svolder_ipc::SPEAKERS_OFF_PREFIX` lets a client know fetching is off without linking
    svolder-speakers.
  - `svolder_host::route`: `RouteFacts.stages` (`ChainStages`: EQ, crossfeed, speakers), filled by
    svolderd from the EQ view when the route has a chain; `None` reads as `host EQ on`.
  - `svolder_mqtt::computer::speakers` (entities, options, state, parse) and the `speakers`
    parameter of `computer::entries`; `EqSwitch::speakers` in svolderd and the bridge's speakers
    publish and command in `mqtt_bridge/computer.rs`.
  - `bins/svolder/src/speakers.rs` is the one client of the daemon's sets family (the CLI's and the
    TUI's), which cancels a fetch or an import on the daemon when its call is dropped; the stub
    daemon gained a scripted `speakers` service.
  - TUI: `Xf::Space` and `Xf::Set` stops, `App.speakers_box` boxed to keep the loop future small;
    `cargo test -p svolder --bin svolder -- --ignored speakers_stills` writes the text looks.
  - Web: `core/speakers-actions.js`, `core/popups/speakers/{draw,pictures,wire}.js`, a third tab in
    `core/popups/sound`, the held key `speakers`; `SVOLDER_HOST=fake` serves two synthetic sets
    (`fake-no-room`, `fake-with-room`, labelled "Synthetic") from a store of its own in the daemon's
    private runtime directory, made fresh at start and removed at stop, never the person's sets;
    `core-speakers.mjs` and `speakers_popup.py`; the speakers tab turns on container queries.
  - `make host-e2e` gains the Virtual speakers phases V0 to V11 and the example `vs_expect`; every
    daemon the run starts gets `SVOLDER_SPEAKERS_URL` and `SVOLDER_AUTOEQ_URL` on a closed port.
  - The signal path line's words come from `ChainStages`; `of_view` returns `None` when the view
    cannot vouch.

## 0.35.1

- **The computer EQ puts a stale chain right sooner**: when the `pw-cli` that feeds the chain ends
  right after taking a line, the next look (within a second) starts it again and sends the newest
  values; and when the chain's slots go back to old values at the same node and rate (also while
  crossfeed plays), the next look notices it and sends them again. A slow `pw-cli` is not mistaken
  for a revert.
- **A computer EQ chain that dies is put right**: a chain that is killed or crashes is noticed at
  once and started again with crossfeed off and the note "the computer EQ chain stopped and was
  started again"; the EQ turns off, with a note saying the chain stopped, only when the restarted
  chain dies quickly (within 10 s) a second time, and a PipeWire restart never turns it off. A chain
  that ran for crossfeed alone is removed rather than restarted. The view no longer says live while
  no chain runs.
- **A daemon that starts over a chain it did not start puts the saved state on it**: a run left in
  the middle of a drag, or another program's, is sent the saved points and preamp at the first look
  (preamp lowered first) when it plays other values; the view no longer shows the file while the
  chain plays something else.
- **A preset reloaded under the same name ends a held point**: the web page follows the daemon's new
  load count, so a point held in a drag lets go when the preset is reloaded.
- **Turning crossfeed or the EQ off no longer pauses the player**: with both off while a stream
  plays, where live updates work, the chain stays as a flat pass-through (0 dB, or the lower level
  its loaded curve needs), switched live with no sink swap, and goes once nothing has played through
  it for about 30 s; turning crossfeed or the EQ back on in that time reuses it, and the view says
  "chain kept as a pass-through until idle".
- **Nothing plays above 0 dB at any usual output rate (44.1 to 384 kHz)**: the crossfeed's cost
  counts the most of its worst gain over those rates, so a change of the output's rate, a crossfeed
  drag or the one-second repeat between a rate change and the next look can no longer play above 0
  dB. Meier with a delay plays about 1 to 2 dB quieter for it (up to about 3.5 dB at extremes, for
  example 0.5 ms at 44.1 or 48 kHz: preamp -3.6 to -5.9 dB at 50 %, -2.0 to -3.4 dB at the default
  level); Bauer, Linkwitz, Mid/Side bass and Meier at its own lag are unchanged.
- **No silence at 352.8 or 384 kHz**: the computer EQ no longer goes silent while it says live when
  an earlier curve its chain still holds cannot play at that rate; the chain is applied again with
  the points on screen, and the EQ turns off with a note only when that fails. Crossfeed alone, or a
  pass-through, is no longer removed there for kept EQ points it does not play (with a note that
  the EQ turned off, though it was off); turning the EQ on at such a rate is refused instead.
- **No silence after turning crossfeed or the EQ on while nothing played**: it could leave the sound
  silent once music started (also in 0.34.5 and 0.35.0), because the chain's preamp was sent at -120
  dB. A filter chain that has never played now gets every value again when it first plays, at the
  preamp the view says; the saved config plays for up to a second first. A chain that 0.34.5 or
  0.35.0 left silent plays again after the upgrade: a start that finds the preamp at -120 dB sends
  the saved one.
- **A volume change from an MCP client that climbs is quieter on the system**: above three quarters
  of the range the change waits on the device's writer, no longer reading the level every 50 ms,
  and answers with one reading, the level reached.
- **Fewer `wpctl` runs**: the daemon reads the default sink's volume once every 2 s however many
  things watch it (the capture takes the reading the computer's volume read just made); an outside
  volume change still reaches the view within the same 2 s. While playing with the EQ off the daemon
  runs about 1.2 of these helper runs a second instead of about 1.8 (from the design, not a
  measurement).
- **The TUI no longer burns a core over a wide cover**: a cover that is not square (a video's 16:9
  thumbnail) was encoded again on every frame, about a whole core with sixel or half-blocks, or the
  picture re-sent to the terminal many times a second with kitty; it is now built once for its cover
  and box (about 3% of a core at 158x76). A frame also lays the device screen out once instead of
  five to eight times, the closed help overlay is no longer laid out, and a frame goes to the
  terminal in one write.
- **A bigger cover in the TUI on large terminals**: from 100x45 the device screen's Status pane
  grows upward while a cover shows, and the cover grows with it, taking only the rows the Controls
  and Tone panes do not draw in, so every row of both stays in sight; a cover that fails to load
  keeps the usual layout, and smaller terminals are unchanged.
- **Preset writes with `--transport ble` or `fake` reach a running daemon**: `svolder preset ...`
  and `hops` now ask a daemon that reads the same configuration file to reload, as the other
  transports do; before, its Home Assistant bridge kept offering the old list.
- **No flash of "saving..." in the TUI**: the wait shows only after 250 ms, so a quick local write
  (Connect, device edits) draws no frame of it; a write that ends within those 250 ms still clears
  an earlier toast.
- **A preset recall that waits for its first fit says when it was overtaken**: a fleet recall of an
  imported preset whose first fit to the device is still being worked out is dropped when another
  write or recall reaches the device meanwhile, and answers "a newer write reached <device> while
  the preset's fit was made, so <name> was not recalled"; repeat it to recall the preset.
- **Volume steps on the web page go on from the right place**: when the stream's reading of a step
  arrived before the write's answer, the next step used the old base; it now goes on from the
  reading in either order (Signal, Paper, Night drive and Ember).
- **The Sound popup is a proper tab list**: screen readers announce its two tabs as tabs, with the
  page below them as their panel, and Tab at the end of a popup whose last control is a radio group
  goes round to the first control, as at any other end.
- **Docs**: the README and `docs/architecture.md` have a picture of how the parts fit together, and
  the README names the models confirmed on real hardware; the life of one volume change has its own
  page, `docs/design/volume-change.md`; `docs/usage/daemon.md` names the threads `top -H` shows.
- **Contributors**:
  - The longest files and test files are split by area (pure moves; test names unchanged):
    `svolderd`'s `device/tests.rs` and `writer/tests.rs`, the TUI's `settings/tests.rs` and
    `run/devices/tests.rs`, `svolder-host`'s `eq/manage/tests.rs`, the tone screen's `tone.rs` and
    its tests, the MCP tools' `tools.rs` (into `guard`, `schema` and `describe`) and its tests, and
    the web page's `eq-library/wire.js` (into `upload-wire.js` and `saved-wire.js`).
  - The TUI loop tests in `pointer.rs` and `toasts.rs` wait on the screen or a daemon condition
    instead of fixed deadlines; `.cargo/config.toml` sets `RUST_MIN_STACK` to 8 MiB so test threads
    do not overflow on the TUI loop futures.
  - `svolder_host::eq`: `Link::ended()` (a `pw-cli` that has ended), `Chain::exited` (a chain whose
    process ends without an apply or remove having stopped it), `reverted()` compares every port
    the look before held at its sent value, the lab event `SlotRevert`; `systemctl --user` calls
    run on tokio's process with a 5 s timeout; the chain's reaper thread is `svolder-eq-reap`.
  - `svolder_host::sink`: `Wpctl::shared_for`, `Wpctl::held`, `Coalescer` and `READ_HELD`; any
    `set-*` call drops the published reading; the TUI's own sink reads fresh at every poll.
  - Web: Ember's knob and press code use `core/volume-ask.js` (`shownRaw`, `baseOf`, `askExpiry`,
    `noteReading`, `ackAsk`) instead of copies; a writer's popup re-reads the viewer's rights at
    every model, fleet, host and stream event; `frame.put` closes the list or the inline ask whose
    markup it replaces; frame.css is regrouped, pixel-equal in four themes.
  - `svolder_host::eq`: `EqView.loads` (serde default 0; bumped by every successful
    `EqControl::load` and every state-file reload); `EqControl` watches `Chain::exited` with one
    task of its own armed when the chain is first applied or seen (`control/exit.rs`, `DEATH_WINDOW`
    200 ticks), a failed restart backs off 1, 2, 4 and 8 s; `FakeChain::die()`; `EqControl.foreign`
    makes the first look compare a start-up chain with the saved points (`differs_from_points`);
    `LiveNode.playing` (a `running` stream node linked into the chain's sink in `pw-dump`; a monitor
    capture never counts), `keep_passthrough`, `check_idle`, `IDLE_KEEP` (600 ticks) and
    `SVOLDER_EQ_IDLE_TICKS` to shorten it for tests, `FakeParams::set_playing`; the live crossfeed
    stage's cost is `config_stage_cost_db` (also over 352.8 and 384 kHz), with the preamp lab's
    rates widened and regressions pinned (seeds 1515, 5457, 15327, 12272 and 20568); a failed
    crossfeed start from EQ off leaves no chain, and a failed crossfeed change re-sends the stage as
    it stands; `played_preamp` floors at -120 dB and a non-finite coefficient is refused;
    `scripts/host-e2e` X3e kills the chain three times and asserts the restart, the second death
    ridden out and the third turning the EQ off, and X5 the pass-through.
  - `watch` reads a chain whose slots all list `b0` 0 as a graph that has not run (`graph_ran`):
    such a look takes no preamp bound, checks no read back and makes no foreign resend, and the
    first look that finds the graph run sends everything again (`not_run`); host-e2e phases S1 and
    S2 (on with nothing playing, then a tone) and S3 (a pause that suspends the chain).
  - The tui-harness crossfeed unfold case waits for the tone screen drawn whole, no longer failing
    on a first run.
  - TUI frames: `Drawn::begin_frame` and `end_frame` keep the device layout for one frame (checked
    against a fresh layout under `cfg(test)`), `test_support::work` counts pane builds and layouts
    per frame, and the terminal is `terminal::FrameTerminal`, a 256 KiB `BufWriter` over stdout,
    instead of `ratatui::DefaultTerminal`.
  - The first fit of an imported preset to a tone control (the page model, a recall, the MCP
    `list_presets` and `load_preset`, a computer recall's holder check) runs on a blocking thread
    behind the permits AutoEq's previews use (`recall::fitted`, `recall::warm`), no longer on the
    runtime's thread.
  - The makefile test (`bins/svolder/tests/makefile.rs`) passes under `make test T=...`: it drops
    the exported `T`.

## 0.35.0

- **Crossfeed for the computer's sound**: feeds a low-passed, delayed copy of each channel into
  the other channel at a set level. It works beside the computer EQ and plays with the EQ on or off.
  Four models: Bauer (the bs2b maths, with the presets "Jan Meier", "Chu Moy" and "bs2b default",
  mildest first), Linkwitz (a first-order low-pass cross path), Meier (only what differs between the
  channels is crossed) and Mid/Side bass (the bass blended toward mono). Each has Level (percent and
  dB) and Crossover, and every model but Mid/Side bass a Delay; changing one makes the values
  "Custom, edited from" the preset. The preamp is lowered for the crossfeed's loudest point, so
  nothing plays above 0 dB at any frequency and turning it on never asks.
- **It never stays on by surprise**: crossfeed is off after the daemon (or the TUI) starts, after
  the output changes, after the filter chain restarts and after a graceful stop (a TUI with no
  daemon quitting counts), each with a note saying why; the model and numbers are kept. With both
  the EQ and crossfeed off no chain runs.
- **Where to find it**: the terminal UI's tone screen (`e`, on the computer's side) has a Crossfeed
  section, folded to one line until you `Tab` or press `X` into it, and the device screen's Tone
  pane has a Crossfeed row (`←→` on or off, `,` `.` preset); the web page's menu row *Sound…* opens
  a popup whose second tab is Crossfeed, with a picture of what each ear hears, and whose first tab
  is the EQ library, which was the *EQ library…* row; `svolder host crossfeed` shows and changes it
  from a terminal; the MCP server's `get_host` reads it and `set_host_crossfeed` changes it (with
  `[mcp] writes = true`); Home Assistant gets a Crossfeed switch and a Crossfeed preset select on
  "This computer", while host audio is on.
- `host eq status` says when the chain runs for crossfeed alone, and `host eq remove` while crossfeed
  plays turns the EQ off and keeps the chain. `POST api/v1/host/crossfeed` changes it from the web
  page (write-gated).
- Contributors: `svolder_host::eq::crossfeed` (models, presets, ranges, the stage's coefficients, own
  lag, worst gain, `CrossfeedView` with its curves; the one table is `SPECS`, no front end holds a
  word or range); a fixed crossfeed stage of six nodes per channel in the live graph (off in every
  configuration); `HostEqState` gains `crossfeed`, `crossfeed_on` and `flat`; `EqView.crossfeed`;
  `HostEqOp::Crossfeed`; `EqControl` splits "a chain runs" from "the EQ is on"; one
  `svolder_ipc::host::run_eq_op` serves the daemon and the TUI; `svolder_host::eq::parse_kind`;
  `svolder_mqtt::computer::entries` takes the `CrossfeedView` and `computer::topics_of` lists every
  entity's discovery topic; the TUI's `app/crossfeed.rs`; the web page's `core/popups/sound/` and
  `core/popups/crossfeed/` (the frame's `draw` takes `head` and `bar`); the preamp lab models the
  stage. The design is in `docs/design/crossfeed.md`.

## 0.34.5

- **`make demo` uses far less CPU**: the demo targets build and run an optimised `demo` profile
  (target/demo) instead of a debug build: with music playing the demo took about 27 % of a core,
  now about 4 %, and it rebuilds faster too.
- **The TUI no longer analyses the audio twice**: when the demo's daemon stops mid-run, the TUI
  stopped relying on it and kept a spectrum and tempo analysis of its own running for the rest of
  the session; it no longer does, the stopped daemon is reaped (no zombie) and a toast says so.
  A wide Spectrum row ends with a dim word, `daemon` or `own`, for where its bands come from.
- **Lighter redraws**: while only the Spectrum row moves, the TUI redraws 20 times a second
  instead of 30; keys, drags and animations keep 30.
- **The TUI stays live while another program writes the config**: Settings, Connect and device
  changes no longer freeze keys and frames while the daemon or another svolder holds
  `config.toml`'s lock; the writes go one at a time, in order, and `q` or Ctrl-C quits once the
  write in hand is through.
- **Track time no longer jumps back**: Signal's Now playing time and the player hop's time stayed
  right only until something else redrew them (a facts reading, a DAC volume turn); one clock now
  counts the track time in every theme.
- **Night drive's volume keys** go on from the device's reading once another hand moved the
  volume, as Signal and Paper already did.
- **Config fixes**: a deleted `config.toml` is no longer recreated from the template by a daemon
  write or by a CLI command run with `--transport ble` or `--transport fake`; a passphrase of the
  wrong type in `config.toml` is no longer quoted back in an error (the line and column stay); a
  file deleted right after a hand edit toasts the missing-file notice in the TUI too; `svolder
  web` and identify against a too-old daemon say so in the same words as the other commands.
- **Contributors**:
  - `make web-touched` lists the web checks a change affects (from what each check loaded on its
    last run, `scripts/web-checks/load-map.json`); it never lists too few: a check with no record,
    or a file no record names, lists more.
  - The full web gate reuses a check's last pass when nothing it depends on changed (its script,
    the web areas it loads, the files cargo builds the binaries from, the browsers); a flaky pass
    is never kept; `make web-checks-cold` runs everything and is required before a tag.
  - `make check` runs the test suite once (on a private bus with no BlueZ) instead of twice;
    `make test T='-p <crate>'` tests one crate.
  - Tests that failed only on a heavily loaded machine wait on what they need instead of fixed
    sleeps (the TUI `waiting` tests, two flaky tests, the process-manager tests).
  - Every thread svolder and svolderd start has a name, so `top -H` shows which one burns.
  - Web themes share more of the core: the track clock (`core/clock.js`), the volume ask
    (`core/volume-ask.js`), gestures and placement; large TUI files split (Settings, the backend,
    the setup wizard's walk).

## 0.34.4

- **Signal's failed cover stays away**: a cover whose picture could not be loaded came back on
  each update of the player and flashed above the title on a phone; it is now left out until the
  track's cover changes, and a new cover is tried again.

## 0.34.3

- **Night drive turns the EQ off from its main view**: the dash has an `EQ` key that turns the EQ
  of the side shown off and on: the computer's EQ, or the device's tone mode between Flat and EQ
  (Loudness counts as off). Before, the device's Flat was only in the DEVICE sheet.
- **A bigger cover in Signal**: the player puts the track's cover to the right of the title,
  progress and transport, 200 to 300 px wide in the amber glow; on a phone it sits above the title
  at 240 px. With no cover there is no empty box.
- **The Loki Max's preset list takes the mouse**: in the TUI's device preset list (`p` on the
  device screen) a click on a slot or built-in does what Enter does, the wheel moves the focus, and
  a click on `more ↑` or `more ↓` goes to the first or last row.
- **Tooltips in the setup wizard**: resting the pointer on a key in `svolder setup`'s key line
  shows what the key does, as the TUI's tooltips do, with the same delay and words; the
  `[tui] tooltips` setting applies.
- **Night drive's track time no longer jumps back**: a volume or mute press made the dash's track
  time return to the last reading of the player (a pause, a seek or a new track).
- **Tested on real hardware**: the README and the release notes now say some real Schiit devices
  have been tested, not only the fake device and the simulator; the README's pictures no longer
  link their text versions.
- **`make demo` and the other demo targets run again**: since 0.34.0 they stopped at once with
  `env: '-u': No such file or directory`, because the line that sets the demo's AutoEq address put
  an assignment before `env`'s own options. `make check` now runs that line, so it cannot break
  again unnoticed.
- **Contributors**: every temporary file the make targets and scripts make, and the tests the make
  targets run, lives under `/tmp/svolder`, which is made 0700 and refused when it is a link or not
  yours, and `make check` fails on a directory its run left there; the pty harness cases wait on
  what the screen shows under one deadline and hold on a loaded machine.

## 0.34.2

- **Contributors**: the daemon's fake-host test asks for the host's view until the fake host is up,
  instead of once, so a slow runner no longer reads it before it has started.

## 0.34.1

- **Contributors**: the tests that start a daemon of their own find the workspace's `svolderd` with
  cargo's newer build layout too (Rust 1.100 runs a test binary from `target/<profile>/build/`), so
  `make check` passes on the compiler CI and the release builds use.

## 0.34.0

- **A guest link in the terminal**: `svolder web guest on|off` turns the web page's read-only guest
  view on or off with no hand edit, and a running daemon applies it at once (it restarts the web
  server, so open pages reconnect). The TUI's Web UI box has a *Guest view* row (`g` from any row)
  that does the same; while it is on, a second QR code of the plain page link sits beside the
  sign-in code, captioned as read-only for anyone on your network. The box now makes its link as
  soon as the daemon's status arrives.
- **Cover art in the TUI**: the device screen's Status pane draws the cover of the track the daemon
  follows, as the terminal allows: the kitty graphics protocol (Ghostty, kitty), else sixel (foot),
  else half-block cells (GNOME Terminal, Ptyxis, tmux, SSH). `[tui] cover_art = "auto" | "off"` and
  a Settings row choose; off asks the terminal nothing, and auto asks it once at start-up, waiting
  at most about 100 ms for the answer. It needs a 0.34.0 daemon.
- **A divider and growing bars**: the device screen's Controls and Tone panes share the width evenly
  and the bar between them moves: drag it, nudge it with `<` and `>`, reset it with `|` or a
  double-click (`device_split` under `[tui]`). The Volume, Balance and Loudness bars and an amp's
  meters now run across their whole pane.
- **Shorter Path words**: a sound that goes elsewhere reads `→ NAME` (or `player → NAME`) in the TUI
  and the web page, which keeps the whole sentence as a tooltip.
- **The squirrel makes entrances**: after an hour it comes out and stays, then zaps out and back in
  every 30 minutes, and each arrival (a typed code, a fault, a lost or returned link, power, an
  input switch) takes one of seven entrances with a matching exit. Nothing enters while a box or
  the help has the keys; with effects off it pops in whole.
- **Tooltips switch**: `[tui] tooltips = true` (the default) turns the TUI's hover tooltips on or
  off, from Settings, Behaviour, Tooltips; they need mouse capture.
- **The preset list by mouse**: click a row to load it, scroll the wheel to move the focus, click
  `more ↑` or `more ↓` for the first or last row. The list's writes (save, update, rename, delete,
  import) and the CLI's `eq save|import|rename|delete` now go through the daemon while one reads
  the same file, and a taken name is refused in the web page's words; they need a 0.34.0 daemon (an
  older one is refused with a `svolder daemon restart` hint).
- **MQTT that makes sense**: Settings, Connect has a username row and an editable, masked password
  row; the status says why the bridge is not connected (login refused, not reachable, TLS port
  8883) in the TUI, `svolder daemon status` and a new `svolder doctor` row. New `svolder mqtt on|off`,
  `mqtt set host|port|username VALUE` and `mqtt password` (no echo, never an argument);
  `SVOLDER_MQTT_ENABLED`, `_HOST`, `_PORT` and `_USERNAME` set defaults (the file wins), and
  `SVOLDER_MQTT_PASSWORD` still wins over the file. Writing a password makes a config.toml that
  group or others could read 0600, and says so. `svolder setup` ends with a line pointing to
  them.
- **Demos with real AutoEq**: `make demo*` searches AutoEq's real repository (`AUTOEQ_URL` names
  another base). With the fake transport and no `SVOLDER_AUTOEQ_URL`, the EQ library, `eq search`
  and `eq import --autoeq` say AutoEq is off for the fake device instead of a connect error.
- **Smaller changes**: lint and release builds use Rust 1.100.0 and a refreshed Ubuntu 26.04 base;
  14 compatible crate updates.
- For contributors: `make tui-harness` runs the real `svolder tui --demo` in a pseudo-terminal with
  a screen parser (opt-in, about 5 s); `svolder-ipc` is split into smaller modules and 19 public
  items narrowed (no behaviour or wire change); long functions are split; the crates are refreshed;
  `make mascot-shots` draws a still of each entrance. New socket requests and features:
  `guest_switch`, `host_cover`, `preset_write`, `mqtt_set`.

## 0.33.1

- **Night drive's wet road**: the reflection now lies only on the floor grid. The bands that
  repeated the ridge between the horizon and the grid's far row are gone.
- For contributors: `camera.js`'s `farRow(h, H)` is the one home of the floor's far row (the wet
  road and the side grid use it), and the Night drive checks pin that nothing wet is drawn above it.

## 0.33.0

- **A Devices popup on the web page**: the menu's *Devices…* row lists every configured device with
  a dot (connected or not) and the tag *Shown* on the one the page shows. **Overview** graphs every
  device's key readings over the last 30 minutes at once (signal strength first, then what the
  profile marks). A device's own page has its settings (name, profile, passphrase: Automatic, None
  or a 4-digit code, Make default), its facts, *Identify*, **All readings** and *Remove…*, each
  saved by its own control; *Show on the page* switches the shown device while the popup stays open.
  The four themes' Device fold-outs keep one line, *Device settings…* (*Devices…* for a reader),
  that opens it. A guest, or a viewer of a page with `[web] writes` off, gets the same popup
  read-only.
- **Add a device from the web**: **+ Add a device** at the end of the Devices popup runs the walk of
  `svolder setup` and the terminal's `n`, step by step: a warning first when a device is connected,
  the devices heard as they are heard, the name, the 4-digit passphrase when the device needs one,
  then the device's page. Reloading the page mid-walk takes it up where it stands; a walk started
  from the terminal shows read-only. The menu's `Device: <name> ›` row is gone.
- **AutoEq through the daemon**: `svolderd` is the only process that reaches AutoEq, and `svolder`
  links no HTTP client or TLS. The web page has an **EQ library…** popup (for a viewer who may
  write): search, group the headphones with their measurements, preview a correction for the device
  on show or the computer (target, fit, where it misses, how far to lower the volume), compare two
  measurements, save it as a preset, import a parametric EQ file, and write a device's curve over a
  preset of your own. With an imported preset on, each theme's tone graph shows its target, the
  places the bands miss it by more than 2 dB and the fit (`fit 0.9 dB`), and the fit follows a band
  you edit (the TUI's tone header too). `svolder eq search`, `eq import --autoeq` and the TUI's `a`
  ask the daemon; with none running, or under `--transport ble` or `fake`, they say AutoEq needs
  svolderd. Headphone names that read the same as letters and digits (`Koss UR 20`, `Koss UR20`) are
  one headphone.
- **A Help popup**: the menu's *Help…* row, or `?` in every theme (a guest too), lists the keys of
  what has the focus, the keys that work everywhere, and the daemon's version (once signed in). It
  replaces Ember's key card. Night drive's `↑` and `↓` do nothing for a guest.
- **The TUI and the CLI on the daemon**: with `svolderd` running, `svolder setup`, `svolder add` and
  the TUI's `n` run the device walk on the daemon (no Adapter step, a warning naming a device that
  holds a link, a walk running elsewhere is refused with where it started). The TUI's device editor,
  removal, default and the Web UI and MCP rows, and the CLI's `add UNIT` and `remove`, are the
  daemon's writes; a daemon too old for them is told to `svolder daemon restart` (exit 7). Ctrl-C,
  SIGTERM or a dropped ssh session cancel the walk that run started and leave with 130. The TUI's
  add-a-device scan lists devices as they are heard, and its AutoEq search box never waits for the
  daemon. Pairing codes are masked (`Tab` shows them) and never read back.
- **`config.toml` changed or missing**: `svolder daemon reload` makes the running daemon read the
  file and the profiles again. The daemon says when the file changed since its last reload (`svolder
  daemon status`, `svolder doctor`, the TUI, the Overview) and a hand edit takes effect at `svolder
  daemon reload` or SIGHUP, never on its own. A deleted file is told as missing, and every write the
  daemon is asked for is refused in those words instead of making a fresh file that would drop every
  device; `daemon reload` refuses and keeps the running configuration.
- **First run in a container**: `docker exec -it svolder svolder setup` and `docker exec -it svolder
  svolder` are the way in, with `docker logs -f svolder`. `svolder web on|off` and `svolder mcp
  on|off` switch the servers (a running daemon applies it at once; with none the verb writes
  `config.toml`); `SVOLDER_WEB_ENABLED=1` and `SVOLDER_MCP_ENABLED=1` do it from the environment,
  and a missing `config.toml` takes the environment's defaults. The compose file caps the log at
  three files of 10 MB, `svolder daemon logs` points to `docker logs` or `podman logs`, and the
  packages and image install `ca-certificates`.
- **Smaller changes**: a popup closes from outside only when the press and the release are both
  beside its box, and the page does not scroll under it; the browser tab's title names the open
  popup; an inline question is read to a screen reader with its answer. Signal's menu row *Add
  device…* reads *Add to the chain…* (devices are added from the Devices popup). Popup headings and
  Signal's lesser words read at a higher contrast. The web page is built for Firefox 140 and
  Chromium 142 or later. A page that showed "No devices" takes up the first device added. A device's
  name is checked when written (not blank, at most 40 characters). The daemon keeps a device's
  signal strengths for half an hour. Profiles can mark key readings, which `svolder devices --json`
  carries as `key_readings`. A daemon stopping during a reload closes every device it started, and
  `svolderd.service` allows 60 s to stop.
- **Security and community files**: `SECURITY.md` (private reports through GitHub's advisory form),
  `CODE_OF_CONDUCT.md`, issue forms and a pull request template.
- For contributors: the socket's `version` answer lists `features`, errors carry a typed `reason`,
  an unknown request answers code 7, and jobs (walk, AutoEq) have a registry with a stream and
  replay for late subscribers; while it runs the daemon is the one writer of the configured devices
  (`[[device]]`, `default_device`). The new `svolder-walk` crate holds the walk. The web core gained
  the modal stack, the history router, the key table, the popup frame and its shared parts; `make
  routes-doc-check` holds the routes to `docs/usage/web-ui.md`, `make network-check` joins `make
  check`, `make tui-hangup-check` (a pty run) is new, and new browser checks cover contrast, phone
  widths, popups and keys (the web checks need the apt package `node-jsdom`).

## 0.32.7

- **Doctor on a laptop's Bluetooth switch**: when the platform's own Bluetooth switch
  (`thinkpad_bluetooth`, `dell-bluetooth` and the like) is blocked, `doctor` says
  `sudo rfkill unblock bluetooth` for a soft block, or that a hardware switch or the firmware blocks
  it, in place of `bluetoothctl power on`.
- **Rootless Docker and Podman**: `svolder web --qr` and the pair link treat the address of
  slirp4netns or pasta (10.0.2.x behind 10.0.2.2) like a bridge address: they refuse it and ask for
  `[web] url`. The daemon docs say to use `ports:` and `[web] url` under rootless Docker.
- **A computer side in the demo**: `make demo-computer`, and `svolder tui --demo` without `--host`,
  now show a simulated computer in the TUI's Target row (volume, mute, EQ points and presets), which
  the web page shares, so a change in one shows in the other. Your real audio stays untouched.
- **A sandboxed `svolderd.service`**: the unit now refuses new privileges, set-id files, personality
  changes, real-time scheduling and new namespaces, allows one native system-call architecture, and
  limits sockets to Unix, IPv4, IPv6 and netlink. An opt-in drop-in,
  `systemd/svolderd-filesystem.conf`, also makes the file system read-only except the config, state
  and runtime directories; it needs systemd 254 and a host that allows user namespaces, and it is
  not proven end to end yet. See "The unit's sandbox" in `docs/usage/daemon.md`.
- **Toolchain and audit**: for contributors, release builds and the `check` workflow use Rust
  1.99.1, `rust-version` stays 1.93, and a `minimum` CI job builds and tests on 1.93.1. `make audit`
  runs `cargo audit` and allows only the `paste` and `ttf-parser` unmaintained warnings, by id; it
  needs the network and cargo-audit and is not part of `make check`. The release image has
  cargo-auditable, and `make static` builds with it, so the shipped binaries carry their dependency
  list.
- **Test broker and unit-check**: for contributors, the MQTT test broker sits behind a
  `broker-tests` feature in svolder-mqtt, svolderd and svolder; `make check` and `make test` enable
  it, and a plain `cargo test` no longer builds about 175 extra crates for svolder-mqtt.
  `make unit-check` verifies the shipped units offline with `systemd-analyze --user verify` where
  `systemd-analyze` is installed (the target prints SKIPPED for that step otherwise) and is part of
  `make check`. A flaky unit test that swept the scratch directory is fixed.

## 0.32.6

- **Faster CI**: the release run skips its test job when `check.yml` already passed on the tagged
  commit (push main, wait for `check.yml` to pass, then push the tag), and `make check` skips its
  private-bus test run where the system bus already has no BlueZ, as on GitHub's runners.
- **Home Assistant dashboard: Computer EQ**: `dashboard.yaml` has a small on/off badge for the
  computer's EQ in the Tone control heading, shown only while the `This computer` entities exist.

## 0.32.5

Fixes from a first user's Raspberry Pi 4 and Docker set-up.

- **Docker first start**: when `./config` or `./run` did not exist, Docker creates them owned by
  root and the daemon could not write its configuration. The image's entrypoint now hands a
  root-owned folder to your user (`PUID` and `PGID`, which compose takes from `HOST_UID` and
  `HOST_GID` in `.env`), says on stderr when a folder belongs to someone else, and runs the daemon
  without root's group; the published-image steps no longer need `mkdir` and `chown`.
  `docker exec svolder svolder ...` runs as the daemon's user.
- **Doctor on a Raspberry Pi**: an adapter blocked by rfkill gets `sudo rfkill unblock bluetooth`
  (a hardware block says so; each adapter is judged by its own switch), and a Bluetooth service
  that is not on the system bus gets `sudo systemctl enable --now bluetooth` (install bluez if it
  is missing). The Bluetooth guide has a Raspberry Pi section.
- **Web and MCP in Docker**: the compose files now give the container the host's network and the
  image listens on every interface, so the steps are `enabled = true` under `[web]` or `[mcp]` in
  `config.toml`, then `docker exec svolder svolder web token` to sign in; the QR code and pair link
  carry the host's LAN address. A native `svolderd` on the same machine must not serve the same
  ports. `svolder web --qr` in a container on a bridge network says to set `[web] url`. New
  `SVOLDER_WEB_LISTEN`, `SVOLDER_MCP_LISTEN` and `SVOLDER_WEB_URL` set only the defaults; the file
  wins. The web-address picker also skips Podman and CNI interfaces.
- For contributors: `make sim-app` starts the simulator for the Schiit phone app, after checking
  BlueZ's flags and printing the drop-in when they are missing; the release workflow check now
  requires every Dockerfile `COPY` source to be staged into the image's build context.
- For contributors: `make check` also runs the tests on a system bus with no BlueZ, as GitHub's
  runners have (`make check-no-bluez`), and a check workflow runs `make check` on every push to
  main, so a release's own test step holds no surprise.

## 0.32.4

- Editing a device so that it no longer shares a unit with another device is accepted; the edit is judged by the file it would leave, not the file it starts from.
- For contributors: `make tests-check` (was `make mutants`) checks that the web tests catch a deliberate break. It is slow, by hand only and not part of any gate.
- A test race in the events stream's host-hold test is fixed: it reads until the pause it asserts has arrived.

## 0.32.3

A fix release: the TUI's Target row is reached and drawn like the rows around it, a Yes/No question
on the web page owns the keyboard, a unit is configured once, and the daemon's reads of
`config.toml` leave its async threads. It also carries 0.31.3 to 0.32.2, which were never
released on their own; their notes follow.

- **Target row reached going down**: `↓` from the row above Volume (Mute) lands on the Target row
  first instead of skipping it, a further `↓` going on to Volume.
- **Target row chaser**: the Target row has a cursor of its own on the device and tone-control
  screens, so the trail and the shimmer play when the cursor moves onto or off it.
- **Target row look**: with the Target row selected, the side shown is in the highlight box, bold,
  and the other side plain dim, in colour and in monochrome; unselected, the side shown stays bold.
  A device with no volume gets the same look, keys and chaser on its Host Volume switch.
- **One unit, one device**: a unit id is configured once. `svolder add`, the setup wizard and the
  TUI's device editor refuse a second device on a unit another device carries, naming both, and
  the setup wizard's scan list leaves configured units out with a line saying which devices they
  are. A file that already has two `[[device]]` blocks for one unit (the wizard used to allow it)
  still starts the daemon and loads in every command: the first block is used, the later one is
  left out, and a warning says so (also in `svolder doctor`), for example `devices "dac" and
  "amp" are both configured for unit 1A2B3C4D5E6F7A8B; "amp" is left out and "dac" is used. Run
  `svolder remove amp` or edit the file to take one of the two blocks out`. `svolder remove` works
  on such a file; other device writes refuse until it is fixed.
- **Hop edits**: `svolder hops edit` and the page's hop edit read the hop they change under the
  configuration file's lock, so an edit that races another write keeps the other change; the page's
  answer to a hop write lists the file just written.
- **Web page, a question owns the keyboard**: while Ember's Yes/No question is open, the arrow
  keys, `m`, `0` and `f` no longer change the volume, the mute or the tone behind it; only the
  question's own keys act, and `T` no longer opens the theme picker behind it.
- **Web page, Paper's lost or waiting sheet is out of reach**: a list or a form open on the sheet
  no longer keeps it clickable when the stream is lost or the page waits for the device again; the
  form stays, with what was typed, and is usable again when the page wakes.
- **Web page, no theme can start**: when no theme starts, the menu and the picker no longer draw
  the failed theme's rows, and a card picked in the picker tries that theme.
- **Web page, waking after a lost link**: a page that was waking when its link dropped keeps its
  dial and sliders inert until the first reading of the reopened stream, as a page that woke
  without a drop does.
- **Web page, a device that leaves the fleet**: when the daemon stops listing a device the page
  follows, the page's stream is opened again without it, once, instead of going on asking for it.
- **Web page, a refusal is kept**: a volume raise the page drops after a refused write no longer
  replaces the refusal as the answer to that write.
- **The web page's config reads leave the async threads**: the live stream, page loads, tone and
  preset saves, the preset and hop writes and the computer's presets no longer read `config.toml`
  on the daemon's async threads.

Internal:

- `make mutants` runs every node web mutant, and `make mutants FULL=1` the browser ones too, with each runner's kills and wall time and a non-zero exit when a mutant survives. The browser runners share one pool (`MUT_WORKERS`, default from the cores and the CPU pressure), and Ember's and Paper's mutants run only the cases they name.
- **Ember's drawing**: its dial, meters and tone curve, on the first draw and on live updates
  alike, round a value that falls exactly halfway to the even digit, as the daemon's own text does
  (0.125 is `0.12`).
- **Container runs**: the two `backend` tests that cannot pass inside a container skip themselves
  there, with the reason on stderr.
- **Dependencies**: `fixedbitset` 0.5.9, `itoa` 1.0.19 and `uuid` 1.29.2, patch bumps only.
- **The web checks**: browser checks hold `/tmp/svolder-web-gate.lock` shared and the timing checks
  exclusive, the job count follows the cores and the CPU pressure, `make web-checks-task` runs the
  logic-only checks in Chromium alone, each run writes its per-check times to
  `target/web-checks/timing.tsv`, and the README in `scripts/web-checks/` describes the ladder. The
  quick and task gates reuse a check's pass while the files it reads are unchanged and report it as
  `cached` (`WEB_CHECKS_NO_CACHE=1` runs everything), and Night drive's Still and readout rates are
  checked against a fake clock.
  New cases: a stream's `close()` and its wait while lost, a fleet that prunes, a kept refusal,
  and Night drive's mouse hover at the tone band's edges.

## 0.32.2

Groundwork for the Bifrost 3: the simulator can play the unit's USB, S/PDIF and sample-rate
readings, the TUI picks between the DAC and the computer, and `make help` reads as a short menu.

- **The simulator's knobs**: `svolder-sim --profile bifrost3` starts with `--usb on|off`,
  `--spdif-locked on|off` and `--sample-rate HZ` (`44100` or `44.1k`; USB off, S/PDIF unlocked and
  48 kHz by default) and, while it runs, takes the lines `usb 1|0`, `spdif 1|0` and `rate 96000`
  on its standard input. Each change goes to the connected central at once as a Status, so the
  Status pane, `status --json` and the web page's readings follow. A bad value is one line on
  standard error and the sim keeps going. The S/PDIF lock goes out as a UInt8, as a Bifrost 3
  sends it.
- **Firmware labels**: a unit's readings 210.2.1 and 210.2.2 read `Firmware` and `DSP firmware`
  in the CLI, the TUI and the web page.
- **Target in the TUI**: a `Target` row on the Controls pane and at the top of the tone screen
  picks the DAC or the computer with `←` and `→`, the same switch as the web page's. The Volume and
  Mode rows keep their own values; the row says why when the computer cannot be used.
- **`make help`** is in sections with one short line per target; `make help T=<target>` gives the
  details, with each setting and the values it takes. On a terminal the sections and targets are
  bold, the settings dim and the warning about your real audio yellow; a pipe, `NO_COLOR` and
  `TERM=dumb` get plain text. `make help-check` is part of `make check`.
- **`third-party-licenses` is now `3rdpartylicenses`**, the make target that writes the notices.
- **Docs**: when you read BlueZ state by hand with several adapters, `bluetoothctl list` then
  `select <adapter address>` first, since a bare `bluetoothctl info` answers for its default
  controller only; the simulator's standard input when it runs as a background job.

Internal:

- **Dependencies**: `cargo update` within the current versions (rmcp 3.9, ureq-proto 0.7, toml 1.2,
  unicode-width 0.2.3 and 32 more); builds and the whole test suite pass, and `cargo audit` (0.22.2)
  finds no known vulnerability, only two unmaintained-crate warnings (`paste` and `ttf-parser`),
  which are in the known issues.

## 0.32.1

A burn-down after the Computer flip: the computer's EQ is steadier in the four web themes and in
the daemon, and a machine that was missing its chain now finds it.

- **Ember**: the computer's points table has `−` and `+` steppers for the point you picked, in the
  look of the DAC's table (hold one to keep stepping; the last value is written when you let go).
  The tone part's DAC and Computer swap now waits for wheel and stepper bursts on Ember's computer
  curve, as it already did for drags and steppers, so a flip no longer replaces the panel under
  your hand.
- **Paper**: a number typed in a cell reads `2.9k` and a comma (`+3,2`), and a word that is no
  number is said and the cell put back. The wheel and the arrow keys on a point are one burst, so
  ten notches make ten live updates and one saved write instead of ten saved writes. A tap that
  adds a point needs under 6 px and 600 ms (it was 8 px with no time limit), and a second finger
  anywhere on the page drops a waiting tap, so it adds no point.
- **Night drive**: the computer's points table has `−` and `+` steppers on each value (a phone
  shows a point as a block), and a held stepper ends when its point is removed by another front
  end instead of going on to the point that took its number. The lit bell, its label and a drag
  follow the points when they are put in order. Two fingers pinch the point in hand; the question
  for a preset a device plays too is asked in the sheet with Apply and Keep.
- **Signal**: an arrow key on a point is a live update and one saved write (two posts for one key),
  a typed value equal to the point's own value writes nothing, and the player hop reads
  `PipeWire volume 54 %`.
- **In every theme**: a typed value is rounded to its field (frequency to 0.01 Hz, gain to 0.1 dB,
  Q to 0.01) and Enter on a cell that has not changed writes nothing, even for a +14 dB point.
  Removing the point in hand puts none in hand. A tap on a full table says `16 is the most`. The
  tone curve, the marker and the label follow a point that moved when the table was put in order.
  A comma in a typed value is read both ways, as a decimal mark and as a thousands separator, and
  the field's range picks the one that fits (`1,000` Hz is 1000, `20,500` Hz is 20.5); when the
  range cannot tell (`20,000` Hz) the cell keeps its value and says `Write 20 or 20k.`
- **Where the computer's chain is found**: a machine whose wireplumber says it is older than 0.5
  while its `wpctl` is 0.5 lists the filter-chain sink under `Filters:`; svolder now reads both
  headings, so the computer EQ finds its chain there.
- **The daemon**: a refused turn-on no longer keeps the sink's rate in mind, so the same load
  plays once the sink is back at a rate it can; retries while a turn-off is held write nothing to
  the state file, and a late success keeps the preamp it measured at the rate that played. A save
  over a state file written by the command line meanwhile sends the screen's points at the next look, to the
  command line's chain. The terminal of `pw-cli` reads raw, so a 16-point update is not cut at
  4095 bytes.
- **The TUI**: the write queue's bound of 64 and the move back to the DAC (the computer's EQ goes
  off first, and a refusal leaves both sides as they were) have tests.
- **Docs**: what applies after a live softening, what a reload does to the node bound, and the
  quieter-until-an-apply price of the node cap, in `docs/design/host-control-preamp.md`.

Internal:

- **The computer side's shared web code** (the typed-number words, the point hand with its drag,
  tap, touch, pinch, wheel and key bursts, the table, the computer view, the presets and the
  volume steps) moved into the web core and the four themes call it; their requests are the same
  but for the lines above. Each helper has a node case written first and mutants that fail it
  (`mutate_computer.sh`), and `contract.py` pins the core's members.
- **The preamp lab** has a liveness rule (a quiet node plays the view's curve under its preamp), a
  reload-writes-nothing rule, a quiet tail after every sequence and a targeted short run beside the
  ordinary one; the controller's checked request and the session's deliver, settle and node run
  once, over a small link trait, for the daemon and the lab.
- **Checks**: Night drive's wet-road case pins the measured draw time (it raced the draw budget
  under Firefox load), a second case pins the budget's hysteresis, and `ND_ONLY=wet_road` runs
  them alone. The 16-point drag and the chain's node id before and after a restart are in
  `make host-e2e`. `computer_paper` is in both lists of `run-all.sh`.

## 0.32.0

The web page, the terminal and Home Assistant can now change the computer's own sound: one switch
flips the volume and the tone control from your device to the computer, whose EQ you edit point by
point while the music plays, with no gap, in every theme.

- **The switch**: `DAC │ This computer` flips the volume control and the tone control together, on a
  DAC's page and an amp's. Flipping writes nothing and the side is kept per viewer; a guest can
  flip to look but every control stays quiet. Where the computer cannot be used, a sentence says why.
- **The computer's volume**: the default sink's volume and mute, 0 to 100 % in steps of 1 %, never
  a boost. The first paint reads `—` rather than a live 0 %.
- **The live computer EQ**: up to 16 points (peak, low shelf, high shelf), the first always there.
  Drag one, turn the wheel or pinch for its Q, tap an empty spot or `+ Add point` to add one, type
  a value, change a type or remove a point; the sound changes with no gap. The table is put in
  order by frequency when an edit ends, never under a finger, and removing the last point sets it
  to 0 dB. The preamp follows the curve's peak so the signal never passes unity, and a line says
  by how much. A sample-rate change that leaves the curve unplayable turns the EQ off and says why,
  and an EQ chain left by an older svolder is named in the note until the EQ is turned off and on
  or edited. Off takes the EQ out of the path and keeps the points. The path line says
  `host EQ on` for a chain in the path, flat or not. On a PipeWire that does not take live
  changes it applies when the drag ends and says so once.
- **Shared presets**: the same list on both sides; built-ins of the shown device and saved ones
  play on the computer as written, and `Save as…` keeps the computer's points. A preset that a
  device plays too would play twice, so the web page, the TUI and MCP ask first (Yes flattens the
  device, No changes nothing).
- **Ember**: a pill beside the device's name; the curve, the points table and the presets are the
  DAC's own parts.
- **Paper**: `showing: the DAC · this computer` under the title, the chosen one underlined.
- **Signal**: choosing the computer's hop is the switch, and its facts (format, other sounds, the
  path verdict) stay below the controls.
- **Night drive**: two keys by the menu button, and on a phone one small `DAC │ PC` pill in the top
  left; the dots sit on the mountains and a drag works only inside the curve's band, so a tap
  outside it is a tap on the scene. The mouse wheel over the volume number and `↑` `↓` set the
  volume, a faded dash fades to a fifth rather than a tenth, the phone's panel is two translucent
  rows over the scene (about nine tenths), and choosing Full measures the phone's speed at once
  instead of waiting, so the scene settles in about two seconds.
- **The TUI**: `C` flips the EQ tab (`EQ for ‹ NAME | Computer ›`, mode `Off | EQ`, a Points
  pane with `a` add, `x` remove, `y` type, Enter or a double-click to type values) and the
  Controls tab's Volume row. The separate host keys are gone; the computer-side preset list
  applies on the computer and asks the both-sides question.
- **MCP**: `get_host`, `set_host_volume`, `host_mute` and `set_host_eq`, with the volume rule's
  cap and its question for a rise of more than 6 dB.
- **MQTT and Home Assistant**: one `This computer` device with a computer-EQ switch (ON puts the
  kept points back, OFF removes the EQ) and a sensor naming the applied preset.
- **Packages and Docker**: the deb and rpm recommend the PipeWire tools, the AUR lists them with
  what each is for, and the Nix package puts them on its PATH. The image carries them, and the
  opt-in `docker-compose.host-audio.yml` mounts the PipeWire socket; `make docker` sets it up.
- **`make demo-computer`** (and `make demo-computer-amp`) shows the page with a simulated
  computer, your own audio untouched.
- **`make host-e2e`** plays a tone through the real daemon and chain on a private PipeWire and
  WirePlumber with no sound card, drags a point, changes a type and the sample rate, and checks
  the level at each step. It is not part of `make check`.

## 0.31.5

The web page gets its fourth theme, Night drive: a synthwave scene that plays your music. 0.31.4
and 0.31.5 ship as one release.

- **Night drive**: pick it from the menu's Theme row, with `T`, or with `?theme=nightdrive`. It is dark only. A sun sits on the horizon above a road and a
  night sky of stars; the music drives all of it.
  - **The mountains are the analyzer and the tone curve.** The ridge is the live spectrum, a
    smooth line through the levels with a slow swell at silence. Drag a point on it to tune the
    EQ; the wheel or a two-finger pinch sets a band's Q. The curve rests when nothing tunes, and a
    lit pink highlight on the curve shows where a dot acts.
  - **The floor is heat.** Its cells rise into short neon bars with the music and glow on loud
    ones; the heat's low end is spread over more colours.
  - **The mood** follows the song: the night and the sun follow the music's own range, and the
    stars, the sun's rays and the wet road answer the band's envelope.
  - **Sky events** are rare: shooting stars and showers, rays turning slowly while they show.
  - **The dash** holds the volume, the input, the mode, the EQ line and Now playing, with a very
    small cover when there is one. Like the HUD's words it fades after 15 s, to a faint 10 % on
    a desktop and on a phone held sideways, and wakes on a move or a key; the first tap or click
    on a faded dash only wakes it, however long it is held.
  - **The sheet** under the scene carries every control and fact the device has, an amp's
    meters and faults included, so nothing needs the scene to be reached.
  - **On a phone** the scene keeps the top, the dash shrinks to its rows at the bottom in
    portrait, and the sheet follows, with more room for the tuning.
  - **Motion levels** (Full, Calm, Still) set how much moves. A frame
    budget watches the draws: a slow screen steps down, a phone draws at most 60 a second in Full
    and blurs nothing, and the scene returns once the draws measure under budget. A readout names
    the level in use on a slow phone.
  - Its fonts, Russo One and Mr Dafoe, are shipped with svolder.

Night drive and Signal's second pass gave every theme these, from the shared core:

- **The tab's title** is the core's and names the device and its state.
- **Held − and +**: pressing a volume, band or strength − or + keeps stepping, in every theme.
- **Space plays and pauses** the computer's player.
- **The theme picker** has a close button, scrolls on a phone, and, with the menu, follows the
  visible window and scrolls inside a short one.
- **The menu button** is the same 44 px button in the corner in every theme, on an opaque backdrop.
- **The device's link** is said in words beside its name in every theme (a green dot while
  connected, the state's word otherwise). Ember's connected pill is gone from beside the menu.
- Now playing shows the album cover in Signal and Night drive, and in monochrome in Paper.
- A muted volume reads −∞ dB in Ember, as in the other themes.
- Signal's guest check reads the real requests, so a guest is verified to write nothing.
- A canvas loop may run under reduced motion and at a pace, and the analysis rules (the held
  bass, the minute's range) are the core's.

Also:

- **The CLI demo**: `svolder tui --demo --host --web` opts the demo into the computer's real
  audio and the web page; the default demo stays safe. `make demo` uses them, and
  `make demo-dac` and `make demo-amp` start a DAC or an amp demo.
- **The TUI's Spectrum row** spans the width of the Controls pane, and its colours are softer.
- **Fixes in every theme with a volume:** pressing − or + to a step you had asked for a moment
  before, after another hand set the device back, is written again (Signal, Paper and Night
  drive dropped it). A change made right after switching to another device can no longer reach
  the previous one. Space is named in the key lists. On a phone, choosing a hop in Signal scrolls
  all the way back to the top.
- **The demo** can no longer hang at exit while it removes the computer EQ it applied.
- **The web gate** runs its checks in parallel, six at a time by default (`WEB_CHECKS_JOBS`
  sets it); a failure re-runs alone, and a check that passes only then is reported as flaky.
  The draw-timing checks run alone in a serial lane, a browser never outlives its check, and
  Firefox picks its own port for the checks' connection.

## 0.31.4

The web page gets its third theme, Signal: the sound's way from the player to your ears, drawn as
one line.

- **Signal**: pick it from the menu's Theme row, with `T`, or with `?theme=signal`. It is dark
  only. Left to right the line runs from the player, through the computer, to the device svolder
  shows, then on to whatever comes after it. A solid dot is something svolder reads, with its
  live facts under the name. A plain ring is a hop you declared, because svolder cannot see an
  analogue signal. Moving dots show the sound flowing while the player plays; they stand still
  when it pauses, while the page is hidden and under reduced motion.
- **Choosing a hop** (click, tap, or Left and Right) opens what it offers underneath:
  - the DAC: the large volume with −, Mute and + (hold − or + to keep stepping; muted reads
    −∞ dB) and the tone curve to drag (the wheel or a two-finger pinch sets a band's Q). The
    DAC says `arrives as sent` when it reads the rate the computer sends,
    `changes the sound` when it does not, and neither while that is not read;
  - a linked Forkbeard amp: its power, meters and faults, writing to the amp only;
  - the computer: its sink, rate, volume and the computer EQ, read only;
  - the player: Now playing.
  Below the first screen come the band table with the input under it, beside the mode, the
  presets and the controls, in two columns, then the device's facts. The mode heads the second
  column; while Loudness is on its button carries a round − and + for the strength, and holding
  one, or a band's − or +, keeps stepping. Zoomed out on a tall window, the page stays together
  in the middle. A slim bar keeps the hops in reach once the line has scrolled away.
- **The tab's title** names the device and how the page stands with it, for example
  `Bifrost 3 · connected — svolder`, or `reconnecting` while svolder is out of reach.
- **Declaring hops on the page**: the menu's *Add device…* row adds an amp, headphones, speakers
  or anything else after the device, and a declared hop can be edited, moved or removed in place.
  A Forkbeard amp the daemon lists but has not linked is offered first, and choosing it links it.
  While an AutoEq preset is applied on the computer and no headphones are declared, the line ends
  with a ring named after the preset.
- **On a phone** the line is a strip across the top with the chosen hop in the middle. Swipe it
  or tap a neighbour; it stays at the top, with the menu button, as the page scrolls. The mode
  sits under the curve, and the shelf opens with the input and the presets, then the band table.
- **Trouble**: a device out of reach dashes the line into a hollow ring, stills what follows and
  shows a card with *Try now*. A fault keeps its controls live. A guest sees every control
  drawn but inert.

Signal found four things the shared core lacked; all are fixed for every theme:

- The page model carries the computer's route (sink, rate, format, volume, mute, the computer
  EQ), not only its words.
- The daemon reads the computer EQ's saved state, so a page can name the AutoEq preset that is
  applied. A newly applied preset shows within a second.
- On its way down the daemon stopped the computer's player before the web server, so an open
  page heard the computer go away and dropped its Now playing card. The servers now stop first,
  and a page keeps the last thing it heard, dimmed, while the connection is lost.
- A theme can place the menu button against the window instead of the page
  (`anchor(el, { fixed })`), which a pinned strip needs; Ember and Paper place it as before.

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
