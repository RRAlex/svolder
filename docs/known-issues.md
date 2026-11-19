# Known issues

What svolder's code does on purpose but imperfectly, or could do better but does not yet: real
limitations kept as they are, and findings judged larger than worth fixing right now. Each entry is
a present-tense statement of what the code does and why, naming the file it lives in.

The file has two parts:

- **Things you may run into** is for people who use the CLI, the TUI, the daemon or Home Assistant:
  something shown or printed wrong, a wait, a key that does nothing, a config the tool refuses or
  reads wrong. It is grouped by area: Bluetooth and devices, Daemon, CLI, TUI, Web UI.
- **Maintainer notes** is for people who change the code: code shape, missing tests, and races or
  edges no user meets outside contrived cases. It is grouped by the part of the code it concerns.

An entry reads ``- [kind] **Title.** Text (`files`).`` The kind is one of:

- `bug`: wrong or surprising behaviour we can fix;
- `limit`: imposed by hardware, BlueZ, a dependency or the protocol;
- `observed`: seen on real hardware or in real runs, and not checkable by reading code;
- `trade-off`: chosen on purpose, with the reason;
- `debt`: code shape, such as duplication, size or visibility;
- `test`: a missing or weak test.

An entry that belongs to upcoming work carries a tag after a comma in its kind:
`with Bifrost 3 testing`, as in `[test, with Bifrost 3 testing]`, or `with hardware` for one that
waits on a device svolder has not met.

To add an entry, put it under its area, in kind order (bug, limit, observed, trade-off, debt, test),
in the part whose reader would meet it; when unsure between the parts, use Maintainer notes. Drop an
entry once the code it describes changes. A cheap issue found in a branch's own diff is fixed in
that branch rather than listed here.

## Things you may run into

### Bluetooth and devices

- [limit] **An adapter with no `Modalias` cannot be opened.** The Bluetooth library svolder opens
  adapters through (btleplug 0.13 over bluez-async 0.9) leaves an adapter that exposes no
  `Modalias` property out of its list, and some on-board and UART controllers expose none, so a
  session cannot open it: naming it with `--adapter` exits 3, and so does naming none on a host
  with no other controller. `doctor` and the setup wizard read BlueZ directly and still list it;
  the wizard's Adapter step grades it 0 out of 5 with the open error as its note
  (`crates/svolder-ble/src/bluez/transport.rs`, documented in `docs/usage/bluetooth.md`).
- [limit] **The fresh reading after a refusal can wait out a scan window and a second connect.**
  The drop listens for at most one scan window (5 seconds), split between two listens when the
  first advertisement heard repeats the refused code: the second is a guard for a transport that
  reports a device when something about it changes, not only when it advertises. The connect that
  follows can itself be refused for several seconds by an adapter that has just released the
  device. A refusal is therefore reported seconds after the first refused answer, and a device that
  another central takes in the window is not read at all (`crates/svolder-ble/src/episode.rs`).
- [limit] **Discovery cannot be limited to LE.** svolder starts discovery with no service filter
  (BlueZ 5.87 crashes when one matches), and btleplug 0.13 asks BlueZ for discovery over both
  transports, so a dual-mode controller runs BR/EDR inquiry alongside LE scanning for the whole
  window (seen on a TP-Link UB500 pair);
  an LE-only discovery tried by hand did not reduce missed windows
  (`crates/svolder-ble/src/bluez/transport.rs`).
- [limit] **The signal strength is the advertisement's from before the connect.** A device shows
  the strength its advertisement carried when the daemon last listened for it, with its age, and
  it does not refresh while the link is up: BlueZ publishes `Device1.RSSI` only while it is
  discovering and drops it when discovery ends, and an unprivileged user cannot ask the kernel for a
  connected link's strength (the management call is refused with permission denied)
  (`bins/svolderd/src/device/publish.rs`, `crates/svolder-ipc/src/socket/signal.rs`).
- [limit, with Bifrost 3 testing] **Recently-off devices still find their way back one at a
  time.** The daemon's shared listen finds every device it is looking for in one window, but when
  it answers nothing for a device, that device's connect falls back to the devices the adapter
  holds, and each cached device's `find` takes the transport's scan lock for its own window in
  turn (`bins/svolderd/src/scanner.rs`, `bins/svolderd/src/device/attempt.rs`,
  `crates/svolder-ble/src/bluez/transport.rs`).
- [observed, with hardware] **Amp meter scales are unconfirmed on a real amp.** Each amp's meter
  reads on a logarithmic scale built from a zero, a full scale and a class-A block, and the
  profiles carry those constants; none has been checked against an amp. Open questions: where
  each amp's zero and full scale really sit, whether the Wotan's two channels have different full
  scales, as their profile has them, how far above a reported floor the zero belongs on the Tyr
  and the Wotan, how often an amp sends readings, what an amp does with
  its meters when they are switched off (whether readings stop or go on), whether it reports the
  switch and its floors at connect and after a refresh (until a floor arrives a Tyr or Wotan meter
  reads from a zero of 0, so an idle amp fills much of the bar), and what physical quantity the
  readings measure (`profiles/`, `crates/svolder-profiles/src/meter.rs`).
- [observed, with Bifrost 3 testing] **The onboard MediaTek adapter scans poorly.** The onboard
  MediaTek adapter (`hci0`) as a central, against a device that advertises every 1.28 s, misses
  about half of its 5-second scan windows, where a TP-Link UB500 Plus catches about five in six; a
  USB dongle is preferred as central for this reason
  (`crates/svolder-ble/src/bluez/transport.rs`).
- [observed, with Bifrost 3 testing] **A 1.28 s advertiser is missed by whole scan windows.** A
  device that advertises every 1.28 s, BlueZ's default when an advertiser names no interval, is
  missed by a whole 5-second `svolder scan` window about a quarter of the time on a TP-Link UB500
  central, and by several windows in a row now and then; svolder cannot change a device's
  interval, and whether Forkbeard devices advertise that slowly is unknown.
- [observed, with Bifrost 3 testing] **A late echo after a write that was not confirmed.** A
  Bifrost 3 echoes every write, repeats of the held value and of the tone-control mode included.
  A write svolder makes is answered with the device's own report received after it; one that gets
  no report within the set timeout, or only a report of another value (which cannot be told from
  one the device sent before the write), is answered "sent, not confirmed" (code 4) with that last
  report in words, never with an older reading. A unit that echoes repeats more than 2 s late and
  stays silent after a recall could have that late echo read as the post-recall report
  (`crates/svolder-ble/src/session.rs`).
- [observed, with hardware] **Loki Max band reports are unverified.** Which id a real Loki Max
  reports a band on is unknown. svolder reads a band from a report on `6.<b>` or on any
  `6.<b>.<n>` as well as on `6.<b>.3`, so any of them lands; a report on a form outside those
  leaves the band unread (`profiles/lokimax.toml`).
- [observed, with hardware] **A Loki Max's answers are unknown.** Nothing says which form a unit
  reports its bands on after a recall or a built-in setting; whether it reports them at all
  (svolder asks for a refresh one second after a recall that no report follows, once); whether
  a `6.<b>` write behaves as a `6.<b>.3` write; whether a `6.<b>` write is echoed (if not, a
  built-in setting stops after its first write with a timeout); whether the device answers a save;
  and what it does with a slot outside 1 to 3, which svolder never sends
  (`profiles/lokimax.toml`, `crates/svolder-ble/src/session.rs`).
- [limit, with hardware] **A preset recall waits up to 2 s for a band's echo.** The TUI sends
  every band write without waiting, and the daemon goes on to the next write at once; but a device
  slot's recall asked right after a band write goes out only once that band's echo is in, up to 2 s
  on a unit that never echoes a band write, and every write asked behind the recall waits with it.
  A built-in setting or a preset waits the same way before it starts, and only the writes to the
  tone control, to the bands it holds and to a slot's recall or save wait behind it
  (`bins/svolderd/src/writer.rs`).
- [limit, with hardware] **An echo more than 2 s late can count as a report after a second recall.**
  A band whose echo has not come 2 s after its write is unreported after the next recall of a device
  slot, and that recall ends the mark: if the echo comes later still, after a second recall, the
  second recall takes it for the device's report, and a built-in setting planned then can skip that
  band. A band written without waiting that the device's actor leaves unanswered for 60 s gets no
  mark at all. Whether a real Loki Max ever echoes that late is unknown
  (`bins/svolderd/src/writer.rs`).
- [observed, with hardware] **On a unit that stays silent after a recall, a band echo can cancel
  the refresh.** The echo of any band write made within the second after a recall (from the
  TUI, the web page or the CLI, over the daemon or not) counts as the device reporting what the
  recall changed, so it clears the one-second refresh guard on a unit that stays silent after a
  recall: no refresh follows and the other bands keep their values from before the recall until
  something else reports. This matters only on a unit that pushes nothing after a recall. The
  daemon puts a recall behind the echoes of the band writes in flight when it goes out, which
  spares only those writes (`crates/svolder-ble/src/session.rs`, `bins/svolderd/src/writer.rs`).
- [observed, with Bifrost 3 testing] **Remote lock's meaning is a best guess.** The app shows
  nothing for Stjarna's and Hati's `201`; "Remote lock" is the likeliest reading, and nothing the
  device or the app does confirms it (`profiles/stjarna.toml`, `profiles/hati.toml`).
- [observed, with Bifrost 3 testing] **Tyr's over-temperature channel is assumed.** svolder takes
  Tyr's over-temperature fault to come with a channel id and listens on `230.2.1` only: a report on
  another channel, or a plain `230.2`, raises no fault (`profiles/tyr.toml`).
- [observed, with Bifrost 3 testing] **Tyr's over-current fault id is assumed.** svolder listens
  for Tyr's over-current on a plain `230.1` only, as `amp-common` defines it, so a Tyr that reports
  it with a channel id, as Gjallarhorn and Wotan do, raises no fault (`profiles/tyr.toml`,
  `profiles/amp-common.toml`).
- [observed] **The Skoll's headphone status has no known meaning.** The Skoll reports a boolean on
  `5` that the app shows nothing for; svolder shows it as the "Headphone output" readout, whose
  meaning is unconfirmed (`profiles/skoll.toml`).
- [observed, with Bifrost 3 testing] **What a unit does on identify is unverified.** svolder sends
  function 200 with no payload, at most once every 2 seconds, and it goes out on a refused link
  too; what a unit does with it (a blink, a timed cue), whether a repeat extends the cue, and
  whether a unit under a wrong passphrase answers it at all have not been seen on real hardware
  (`crates/svolder-ble/src/session.rs`, `crates/svolder-proto/src/function.rs`).
- [observed, with Bifrost 3 testing] **A silent device holds a connect to the drain bound.** The
  refresh waits for every id a profile names, so a device that never reports `21.1`, `201` or a
  three-fid fault holds a connect until the 2 s drain bound (`crates/svolder-ble/src/session.rs`
  `awaited`). A `set` made just after a device comes back on waits for the refresh's burst the
  same way, up to that bound, before it goes out.
- [observed, with Bifrost 3 testing] **A held link's survival of a device walk's scan is known on
  dongles only.** On two TP-Link UB500 dongles the link the daemon held survived the walk's
  10-second scan with no `link lost`, and the held simulator was not heard (it does not advertise
  while linked); the onboard MediaTek adapter and a real device have not been tried
  (`scripts/e2e.sh`, `bins/svolderd/src/handler/probe.rs`).
- [trade-off] **A refusal is handled differently from the app.** svolder keeps the link when the
  device refuses the passphrase and reports the refusal, where the app disconnects when the
  advertised passcode does not match the stored one. A device on `auto` is the one exception, once
  per refusal: svolder releases the link, reads the code from the device's next advertisement and
  connects again, and keeps the link only if that is refused too (the entry below). A code typed as
  digits, and `none`, keep the link at the first refusal. Only a status of exactly `250.255` counts
  as a refusal, whatever its direction; a `250.255.<n>` does not. The `250.255` itself is handed to
  callers and published like any other status (`crates/svolder-ble/src/session.rs`,
  `crates/svolder-ble/src/state.rs`).
- [trade-off] **Temperature reporting is switched off, as the app does.** What an amp streams while
  it is on is unknown (`profiles/amp-common.toml`, `crates/svolder-ble/src/session.rs`).
- [trade-off] **Freya 2's gain can be sent in Passive.** The app refuses it there; svolder sends it
  and the device decides (`profiles/freya2.toml`).
- [trade-off] **A Mesh Card is offered no identify.** The app offers none on a Mesh Card, so svolder
  does not either: its profile sets `identify = false`, the daemon and a direct session refuse the
  request in words, and the TUI dims `b`, the web hides the button and Home Assistant gets none.
  `svolder raw 200` can still send the function (`profiles/meshcard.toml`,
  `crates/svolder-proto/src/function.rs`).
- [trade-off] **An absent device fails at the connect timeout, and a stale advertisement under a
  typed code is not corrected while its link is up.** When a window hears nothing of the device,
  device selection (`select_device`) and the daemon's `discover` fall back to the advertisement
  the adapter holds (`Transport::held`), and `BluezTransport::find` to the peripheral it holds, so
  a held advertisement can feed profile and passphrase resolution stale data; the cost is an
  absent device failing at the connect timeout rather than at device selection. A device a
  central holds does not advertise, and the daemon reads an advertisement again only at its next
  connect attempt, which comes when the link drops. Under `auto` a refusal of a code resolved from
  a stale advertisement is therefore answered once: svolder releases the link, listens for the
  device's next advertisement for one scan window and connects again with the code it carries.
  When that advertisement carries the same code, or none is heard, the refused link is kept
  (above), nothing is written to it, and the user power-cycles the device. The same refusal earns
  no second drop while the code svolder would use is the one that was refused: the episode ends
  when the device shows another code works, the entry changes, or a connect reads another code
  from the device's advertisement. A code configured as digits is never dropped over: it gets the
  passphrase note, and a corrected entry ends it. A direct session reads one advertisement at its
  start, again once per refusal, and under `auto` again before each reconnect after its link
  drops (`crates/svolder-ble/src/bluez/transport.rs`, `crates/svolder-ble/src/episode.rs`,
  `bins/svolderd/src/device/attempt.rs`, `bins/svolder/src/backend/direct.rs`,
  `bins/svolder/src/commands/device/select.rs`).
- [trade-off, with Bifrost 3 testing] **A verb against an absent device takes 20 seconds or more.**
  A session's `connect` keeps trying for `SessionConfig::connect_deadline`, 20 seconds, so a verb
  run without the daemon against a device the adapter still holds but that is gone fails after the
  selection's 5-second window, those 20 seconds, and up to `CLOSE_GRACE` (8 seconds) more while
  the transport takes the abandoned attempt down. A device the adapter does not hold fails at
  selection instead, after `WINDOWS` windows of `WINDOW` (15 seconds), and through the daemon a
  write to a disconnected device is refused at once (`crates/svolder-ble/src/session.rs`,
  `bins/svolder/src/scanning.rs`).
- [trade-off] **A recall does not ask.** The TUI, the CLI, the web page and Home Assistant send a
  Loki Max's slot recall at once, as the app does, though the slot's bands are the device's to say
  and a recalled band can be up to 12 dB above 0. The MCP server asks first (below). The save
  asks wherever it is offered (`bins/svolder/src/commands/device/device_presets.rs`,
  `bins/svolderd/src/mcp/tools.rs`).

### Daemon

- [bug] **Renaming or removing a device leaves its hops, and a rename drops the link.** A `[[hop]]`
  that links a device or sits `after` it is not renamed or removed with it, so the file then loads
  with the warning that the hop names no configured device; `default_device` is carried along, hops
  are not. Devices are matched by their configured name, so a rename through the daemon, the TUI's
  editor or the web page stops the device's actor and starts one under the new name: the link drops
  and is made again (`crates/svolder-ipc/src/config/devices.rs`, `bins/svolderd/src/handler.rs`).
- [bug] **A walk whose front end died holds the walk slot for up to 10 minutes.** A front end that
  dies without cancelling its walk leaves it running until the 10-minute idle bound, and another
  front end cannot start a walk until then (`bins/svolderd/src/handler/walk.rs`).
- [limit] **An MCP client that cannot ask the user gets `confirm`.** A volume rise of more than
  6 dB, a higher gain or mode, an unmute, a power-on and any change of a control that asks on every
  change (the Lyr 5's pre-out, the Loki Max's mode), a Loki Max built-in setting that raises a band
  more than 6 dB and any recall of a Loki Max slot ask the user through MCP form elicitation,
  which only a client on protocol 2026-07-28 or newer that declared form elicitation can answer;
  any other client gets a tool error telling the model to ask the user and repeat the call with
  `"confirm": true`, which the daemon then honours without being able to check that the user was
  asked. Leave `[mcp] writes` off, or set `max_volume_db`, for such a client
  (`bins/svolderd/src/mcp/tools.rs`).
- [limit] **In a container the web and MCP token lives in the writable layer, and the container is
  judged by its marker files.** The token a fresh container generates is kept under
  `/tmp/.local/state/svolder`, so it changes at every recreate (keep it under `/config`, as
  `docs/usage/daemon.md` shows); and `daemon logs` and `daemon status` take a daemon for
  containerised by `/.dockerenv`, `/run/.containerenv` or pid 1's cgroup, so one that logs to a file
  inside a container is still read as a file. Under host networking the container's name cannot be
  read from inside, so the sentences say `<container>` (`crates/svolder-ipc/src/checks/live.rs`).
- [limit] **A device that is away between attempts has no steady signal series.** The series has one
  point per advertisement a connect attempt or a scan heard, so a connected device has none after
  its connect and one that is away has none at all until a scan or a walk hears it; a periodic
  listen would fill it. A hearing by the `scan` verb or a walk updates the device's `signal` and its
  series, and the MQTT feed's `signal` follows at the actor's next hearing
  (`bins/svolderd/src/device/publish.rs`, `bins/svolderd/src/device.rs`).
- [limit] **An own SOFA file that is crafted can tie up a core and cannot be cancelled.** The SOFA
  reader inflates chunked data with no total cap, so a file under the 64 MiB limit can take a long
  time. The import runs as a job off the request path, but the job cannot be interrupted once the
  parser has it (`crates/svolder-speakers/src/sofa.rs`, `parse`).
- [limit] **A cancelled virtual speakers' fetch can stay alive until the 120 s timeout.** Cancel is
  seen between reads of the download, not while waiting for the response headers, so a stalled
  server keeps a cancelled fetch, and the store's gate with it, until the client's timeout
  (`crates/svolder-speakers/src/fetch.rs`).
- [trade-off] **MCP judges a built-in setting against the floors it read at call
  time.** A band that another front end lowers after the call and before the writer applies the
  setting is raised back, and was judged against the earlier floor, so a rise can land larger than
  the one asked about, by up to the band's whole range, 24 dB on bands 1 and 6 (the floor at
  +12 dB, the band lowered to -12 dB within the window, a setting that targets +12 dB). It is the
  window `set_control` has (`bins/svolderd/src/mcp/tools.rs`).
- [trade-off] **Home Assistant's mute, power, gain and Preset are not guarded.** With
  `commands = true`, a `mute OFF`, a `power ON`, a gain select set higher or a Loki Max's Preset
  select (a slot, or a built-in setting that raises a band) from Home Assistant goes to the device
  at once: the bridge clamps the volume to `max_volume_db` and climbs it above three quarters of its
  range, but it asks nothing, having no one to ask, where the MCP server asks first. The web page
  does the same for a signed-in browser with `[web] writes` on, and so does the TUI. The rule
  that names those changes is shared (`lets_sound_through`), so a front end that can ask has it to
  hand (`bins/svolderd/src/mqtt_bridge/commands.rs`, `bins/svolderd/src/web/writes.rs`,
  `crates/svolder-profiles/src/safety.rs`).
- [trade-off] **MCP judges its question and its cap on its own reading of the volume.** The 6 dB
  question and the `max_volume_db` cap are decided on the volume the MCP server read when the tool
  was called; a knob turned down between that reading and the write can make an allowed change
  land as a larger rise than the one asked about. Above three quarters of the range the daemon's
  climb paces the rise (`bins/svolderd/src/mcp/tools.rs`).
- [trade-off] **A timed-out MCP glide or recall keeps running.** When the daemon's deadline
  passes, the tool answers with an error, but the glide or recall stays in the writer until it
  ends: a write is bounded by its own deadline and a climb by its own end, so the device may
  still move after the error (`bins/svolderd/src/handler.rs`, `bins/svolderd/src/writer.rs`).
- [trade-off] **The MCP listener has no TLS and no OAuth.** It speaks plain HTTP with a bearer
  token, so a LAN client reaches it through a reverse proxy that terminates TLS, and a single
  sign-on cannot validate tokens yet: `McpAuth` is the seam a later `auth = "oidc"` would fill
  (`bins/svolderd/src/mcp.rs`).
- [trade-off] **A mode and a band sent together can leave the band unheard.** A band command from
  Home Assistant or the web page, answered while the tone control reads `flat`, switches it to
  `eq` unless a mode command arrived after the band. When a `flat` sent before the band is still
  on its way to the device as the band is answered — the two held during a preset recall and
  released together, or published back to back by a script or a scene — the switch is skipped:
  the device ends in that `flat`, with the band written but not heard. Queuing the switch behind
  that mode write would close this; it is left because this way round never overrides a mode
  asked for later (`bins/svolderd/src/writer.rs`, `bins/svolderd/src/web/writes/tone.rs`).
- [trade-off, 0.28.0] **A player that rewrites one cover file for every track keeps the first
  picture.** The daemon reads a cover once for each `mpris:artUrl` and keeps it while the address
  stays the same, so a player that writes every track's art to the same path shows the first
  track's picture for the whole session. The common players change the address per track or per
  album; keying the cache on the track as well would cost one local read per track
  (`bins/svolderd/src/host.rs`).
- [limit, 0.32.0] **Low, sharp filters play with f32 noise.** PipeWire's biquads run in f32 (and
  with fused multiply-adds on arm64), so a filter below about 40 Hz with a high Q plays with
  rounding noise 40 to 55 dB under the music at 48 kHz and more, worse as the rate rises, and its
  response can differ from the curve drawn. The computer EQ refuses only a filter that is not
  stable in those coefficients, never one that is merely noisy
  (`crates/svolder-host/src/eq/manage.rs`).
- [trade-off] **The unit's filesystem drop-in is opt-in.** `systemd/svolderd-filesystem.conf` is
  not installed by `svolder daemon install` or the packages until it is proven on a host where
  user namespaces work (systemd 254 and later): still missing are a run showing a write outside
  the allowed paths fail, a first start with no `~/.local/state/svolder`, and the daemon's web,
  IPC and EQ writes working under it (`systemd/svolderd-filesystem.conf`).
- [limit] **Where unprivileged user namespaces are restricted, the filesystem settings do
  nothing, silently.** With `kernel.apparmor_restrict_unprivileged_userns=1` (Ubuntu 26.04) a
  user unit's mount-namespace settings (`ProtectSystem` and the rest of the drop-in) are accepted
  and have no effect, and nothing says so; a `doctor` line could
  (`systemd/svolderd-filesystem.conf`).
- [trade-off] **The daemon tells a changed configuration by `config.toml`'s timestamp alone.** An
  edit of a user profile in the profiles folder, which a reload reads too, is not told. A change
  that lands within the same filesystem timestamp tick as the daemon's last read is never told:
  milliseconds on ext4 and xfs, one to two seconds on vfat and some FUSE or 9p mounts. The
  Overview's web line is read when it opens, at a fleet event, at a wake and every 10 seconds while
  one stands, so a hand edit made while it is open and no line stands shows at the next of those
  (`bins/svolderd/src/handler/config_write.rs`).
- [trade-off] **A switch variable is read by the daemon alone.** With `SVOLDER_WEB_ENABLED=1` and no
  `enabled` in the file, `svolder web on` writes nothing, so removing the variable later turns the
  page off again; a verb run with another environment than the daemon's (a host CLI against a
  container's socket) goes through the daemon, whose environment decides. A switch value that is not
  1, 0, true, false, yes, no, on or off stops the daemon at its start (exit 1, the variable named),
  as a bad `SVOLDER_WEB_LISTEN` does, so a container under `restart: unless-stopped` restarts in a
  loop. A daemon that listens on another socket than the one the verb resolves cannot be told from
  no daemon: the verb writes its own file and says nothing answers on that socket
  (`bins/svolder/src/commands/server_switch.rs`, `bins/svolderd/src/handler/server_switch.rs`).
- [trade-off] **`daemon stop` waits the full request time for a silent daemon.** `svolder daemon
  stop` sends its `Stop` on the default request timeout, 65 seconds, so a daemon that takes the line
  and says nothing holds the verb that long (`bins/svolder/src/commands/daemon/lifecycle.rs`).
- [trade-off] **A device can be pinned to a base profile.** The profile lists of the web page, the
  wizard and the TUI's editor offer the base profiles (`dac-common`, `amp-common`, `mesh-eq-5`),
  which have no product id, marked `base`, and the daemon accepts any registry name
  (`known_profile`) (`bins/svolderd/src/handler/device_config.rs`).
- [trade-off] **A deleted `config.toml` is only offered back.** While the file is missing the daemon
  keeps its running configuration and says to restore the file and reload; to start afresh with no
  file, run `svolder daemon restart`, which reads a missing file as an empty configuration
  (`crates/svolder-ipc/src/socket/daemon_version.rs`, `crates/svolder-ipc/src/config.rs`).
- [trade-off] **Turning the web guest view on or off restarts the web server.** The reload sees a
  changed `[web]` section and stops and starts the server on its address, so open event streams
  end, and a listen port of 0 changes (`bins/svolderd/src/http_slot.rs:93`).
- [trade-off] **A flat chain for crossfeed can sit below 0 dB.** With crossfeed on and the EQ off, the
  chain plays flat at the lowest level its loaded configuration allows (the rule for a change of the
  sink's rate), so a chain loaded with a -6 dB curve keeps -6 dB until it restarts
  (`crates/svolder-host/src/eq/control.rs`, `preamp`).
- [trade-off] **A crossfeed reset whose chain part fails is retried every second without a limit.** The
  view's note says so; the retries stop when the host accepts the change
  (`crates/svolder-host/src/eq/control/xf.rs`, `finish_reset`).
- [limit] **Computer EQ live updates are not retried once given up.** After 30 failed sends the
  controller gives up live updates ("pw-cli would not take the changes") and never tries again,
  although `pw-cli` works again about a second after a refusal window; only an apply (an edit, a
  preset or a toggle) clears it. A chain left that way can play a curve at a different rate than
  the one it was built for (filters shifted in frequency, under unity) while the view says
  `pw-cli` refused (`crates/svolder-host/src/eq/control.rs`).
- [limit] **A third graph rebuild can still hide a computer EQ edit.** An edit is sent again once,
  a look later, when no look saw it land; a third rebuild of the audio graph between that resend
  and the next look leaves the chain at the configuration's value until the next edit while the view
  says live. It takes three aligned rebuilds, rarer than the two the resend covers; under unity
  (`crates/svolder-host/src/eq/control.rs`, `docs/design/host-control-preamp.md`).
- [limit] **A restart with no daemon running plays the older computer EQ config.** Once the rate
  plays the points again after a rate that did not, nothing rewrites the configuration; a chain that
  restarts while no daemon runs plays the older one (consistent, under unity) until the daemon's
  first look (`crates/svolder-host/src/eq/manage.rs`).
- [limit] **Loudness follows the computer's sink volume only.** An application's own volume slider
  (a per-stream volume) never reaches the sink, so loudness cannot see it and does not follow it
  (`crates/svolder-host/src/eq/control/ld.rs`).
- [limit] **Loudness follows a volume change made outside svolder within a second.** A change made
  through svolder is followed on the next 50 ms tick; one made elsewhere (a desktop volume key) is
  read at the chain's one-second look, so the tone lags it by up to a second. It is cut-only, so
  never louder; it is heard as a short late tilt (`crates/svolder-host/src/eq/control/ld.rs`).
- [observed] **Each loudness step resets the changed EQ filters inside PipeWire.** PipeWire 1.6.2
  clears a filter's state when its coefficients change, so every line of a loudness glide leaves a
  ~3 ms low tick, about 11 dB or more under the music; a glide makes about 20 a second. Not heard
  on music in a listening test (`crates/svolder-host/src/eq/control/ld.rs`).
- [limit] **The loudness curve is within 2.7 dB of ISO 226:2003 at its deepest.** Two shelves with
  four constants fit the contours to 0.7 dB at 5 dB of depth and 2.7 dB at 40; no four constants do
  better at 40 dB under the 6 dB treble cap (`crates/svolder-host/src/eq/loudness.rs`,
  `scripts/loudness-fit.py`).
- [trade-off] **A new owner over a chain that holds a loudness stage plays one tick quieter.** A
  daemon or TUI that starts over a node left holding a stage begins from a depth of 0, so its first
  line steps the stage to 0.5 dB and its low section is cut for the stage the node held: up to about
  6 dB quieter across the band at strength 100 % for one tick, then the look settles it. Never
  louder (`crates/svolder-host/src/eq/control/ld.rs`).
- [trade-off] **A loudness glide can dip a little under its target.** A line toward a shallower
  depth cuts its low section for the high shelves the node may still hold: about 0.15 dB across the
  band at the end of a glide up, and up to about 1.5 dB while a look finds `pw-cli` lagging, until a
  look confirms the node and puts the exact stage on. Quieter, never louder
  (`crates/svolder-host/src/eq/control/ld.rs`, `ld_line`).
- [limit] **The first second after the first volume write may follow the default sink alone.** A
  write's reading is the default sink's; the other node's share in the path is learned at the next
  look (1.0 until then), so with that node under 100 % loudness follows the default sink's volume
  alone for up to a second. Cut-only, corrected by the next look
  (`crates/svolder-host/src/eq/control/ld.rs`, `ld_take_hint`).
- [limit] **The path verdict and the PipeWire config check stop at the player's stream and the
  sink's converter.** They judge what enters the system's audio pipeline from the player and every
  hop after it, at most 24 bits: svolder cannot see inside a player (its own normalization,
  ReplayGain, volume, EQ or float maths), whether the music is a file or a stream, an S32 stream's
  bits below the top 24, a float stream's bits beyond the sink, or anything past the sink's
  converter (ALSA `hw`, the USB driver, the DAC). The words say so
  (`crates/svolder-ipc/src/host/path.rs`, `setup.rs`).
- [limit] **A pro-audio sink's AUX channels against a stereo stream read `channels remapped`.** An
  `AUX0 AUX1` sink fed an `FL FR` stream is called changed, though PipeWire may copy the channels
  one to one; it is to be settled with a capture (`crates/svolder-ipc/src/host/path.rs`,
  `channel_words`).
- [limit] **A stream rate missing from `clock.allowed-rates`, into a sink fixed at the stream's
  rate, may hide a resample.** The path line does not name it; the config check's `rates` line fails
  it (`crates/svolder-ipc/src/host/path.rs`, `computer_hop`).
- [limit] **A continuous rate range in a card's `stream0` file lists no rate.** The kernel's offer
  then reads as unknown rather than a list, and the sink's own offer is used
  (`crates/svolder-host/src/route/asound.rs`, `kernel_offer`).
- [limit] **A float stream into a sink of 24 bits or more is not judged for width.** A float's bits
  beyond the sink, and an S32 stream's low 8 bits into S24, are outside the check (the first entry)
  (`crates/svolder-ipc/src/host/path.rs`, `width_words`).
- [limit] **Home Assistant, MCP and `svolder host check` do not tell DACs of one family name
  apart.** The judged DAC is the first DAC-family device whose name matches the sink, so a config
  with two Schiit DACs of one family name is not told apart
  (`bins/svolderd/src/mqtt_bridge/computer.rs`, `bins/svolderd/src/mcp/tools/host.rs` `path_of`,
  `bins/svolder/src/commands/host_check.rs` `lines`).
- [limit] **On the CLI and MCP a Bluetooth or virtual sink passes `no plug` as "not an ALSA sink",
  and the path can read `untouched` up to that sink.** `svolder host check` and MCP `get_host`
  judge the computer's side for any sink (`dac` null). After a Bluetooth sink a lossy codec
  changes the sound; after a virtual sink (an EQ app, a loopback) processing does. Both are past
  the sink, where the check cannot see, so the verdict is true only up to that sink. The TUI, the
  web page and Home Assistant read `not_this_dac` for such a sink
  (`crates/svolder-ipc/src/host/setup.rs`, `crates/svolder-ipc/src/host/path.rs`,
  [host-config-check.md](design/host-config-check.md)).
- [limit] **A DAC that has not connected since svolderd started is not recognised as the sink's
  DAC.** Its profile comes with its first connection, so the Home Assistant path sensor reads
  `not_this_dac` and the PipeWire config sensor reads unknown until it connects: never a false OK
  (`bins/svolderd/src/mqtt_bridge/computer.rs`).
- [observed] **PipeWire can play a half-applied filter for one quantum after a live update.**
  PipeWire's filter chain applies a live control update one value at a time, and nothing stops the
  audio thread from playing in between. Rarely (measured about 1 in 6,000 risky updates on PipeWire
  1.6.2; at ten glide ends a day, roughly once every year or two) one quantum, about 1/50 s at 1024
  samples and 48 kHz (a smaller quantum makes it likelier), plays a half-applied filter. A small
  step is harmless (a loudness glide step: about +0.3 dB). These are loud:
  - the daemon stopping gracefully, a TUI with no daemon quitting, a reconfigure that halts the
    computer's side, or loudness turned off, while loudness plays at a depth: +31 to +43 dB in the
    bass measured, up to +50 dB worked out;
  - the end of a loudness glide back to flat: +29 dB in the bass measured;
  - adding or removing a computer EQ point, a live preset recall that changes the number of points,
    the EQ turned on or off live: an unstable filter or full scale measured;
  - dragging an existing point: up to about +17 dB worked out, not measured;
  - switching the virtual speakers or crossfeed: up to about 6 dB worked out.

  After a bad quantum the chain recovers by itself. Live edits of the computer EQ have had this
  since they began (0.35), so v0.36.7 has it. svolder cannot close it without giving up live updates
  or a much larger redesign; the fix belongs in PipeWire. The order in which svolder sends a stage's
  values keeps every whole section quiet, and the preamp lab proves that; it cannot see a tear
  inside one section (`crates/svolder-host/src/eq/control/ld.rs`, `ld_line`;
  `crates/svolder-host/src/eq/live.rs`).
- [trade-off] **With no identified player the computer's hop skips the width, channel and
  stream-filter words.** The player hop already says unknown, so the later words would only repeat
  it (`crates/svolder-ipc/src/host/path.rs`, `computer_hop`).
- [trade-off] **`dmix` always reads as a change.** With an ALSA `dmix` device the path says `ALSA
  dmix mixes` and the `no plug` line fails, even when nothing else plays: other ALSA clients can mix
  in at any time, so svolder never calls it untouched (`crates/svolder-ipc/src/host/setup.rs`,
  `plug_line`).
- [trade-off] **The `24-bit output` line fails on every 16-bit sink.** It judges the sink's format,
  not the stream's, on every surface, so a 16-bit stream through an S16LE sink that is exact still
  reads `PipeWire config changes the sound: 1` while the path says `untouched`
  (`crates/svolder-ipc/src/host/setup.rs`, `format_line`).

### CLI

- [bug] **`eq import --autoeq` refuses a correction of more than 16 filters.** The import names the
  computer as its side, so the computer's 16-filter limit applies, where a library entry may hold up
  to 63 filters. AutoEq's published files have ten filters, so only a mirror or a hand-made index
  trips it (`bins/svolderd/src/handler/autoeq.rs`).
- [limit] **The process manager's pid file is written after the old chain stops.** `host eq apply`
  tries the runtime directory before anything of the chain that is on is touched, but the pid it
  keeps there is only known once the new chain starts; a runtime directory that fills up between
  the two fails the apply with the old chain already stopped
  (`crates/svolder-host/src/eq/manage.rs`, `start_process`).
- [limit] **`doctor` grades adapters against a held device.** `doctor`'s Adapter grade row, shown
  when there are two or more adapters, grades each against the first configured device, which a
  running daemon may be holding, and a held device stops advertising: on such a host the grades
  read low across the board and the row's recommendation is worth less than it looks. `doctor`
  observes and starts nothing, so it cannot put the daemon down to measure properly; the Devices
  row beside it says outright when the daemon holds a device, which is the reader's cue
  (`bins/svolder/src/commands/doctor.rs`).
- [limit] **A laptop's Bluetooth switch counts against a USB dongle.** `doctor` takes a blocked
  platform switch (one not named `hciN`, such as `thinkpad_bluetooth`) as blocking every adapter,
  including a dongle chosen with `adapter = "hciN"`, though the switch may govern only the
  laptop's own radio; the dongle then reads as blocked when it is not. It is rare, and the
  switch's name is all the kernel gives to go on (`crates/svolder-ipc/src/checks/bluetooth.rs`, `block_of`).
- [limit] **Two Ctrl-Cs a millisecond apart can count as one.** Signals that arrive before
  `watch`'s receiving task has read the one before are delivered once (Tokio coalesces them, as
  the kernel does a standard signal still pending), so two presses within about a millisecond
  can stop the loop and leave the release waited for, until a third press gives up on it
  (`bins/svolder/src/commands/device/watch.rs`).
- [limit] **`svolder setup` offers no web or MCP step.** The wizard walks the device, its name, the
  passphrase and the daemon; the web page and MCP are turned on afterwards with `svolder web on`,
  `svolder mcp on` or the TUI's Settings (`bins/svolder/src/wizard.rs`).
- [limit] **`svolder host speakers show` with no daemon names only the saved set's id.** The sets'
  sidecars are the daemon's to read, so without it the label, licence and loudness of the chosen set
  are not known to the command (`bins/svolder/src/commands/host_speakers.rs`, `show`).
- [limit] **`svolder host check` with no daemon and no `--player` reads the player as unknown.** The
  verdict is then `unknown`: following the MPRIS player as the daemon does is outside its one
  `pw-dump` read, so name the player with `--player` (`bins/svolder/src/commands/host_check.rs`,
  `own_route`).
- [trade-off] **`svolder set` is outside the volume's glide.** `svolder set volume max`, a raw
  number above three quarters of the range and `svolder raw` on the volume write that step at
  once, as does `svolder set volume up --step N` that lands above three quarters with N of 4 or
  more: an explicit command is taken as meant. The TUI's bar, keys and wheel, and Home
  Assistant's slider, climb above three quarters at a tenth of the range a second instead
  (`bins/svolder/src/commands/device/values.rs`). The keys' and wheel's pace keeps its own credit,
  apart from the bar's glide, so bar drags interleaved with wheel notches can approach twice the
  glide's rate (`bins/svolder/src/tui/app/glide.rs`).
- [trade-off] **The wizard's Adapter step takes seconds per adapter.** The wizard's Adapter step
  grades the adapters one after another, so that each count is what that adapter heard alone, for
  `GRADE_WINDOWS × GRADE_WINDOW` (five 1-second windows) each: on a host with three dongles,
  fifteen seconds between naming the device and seeing the next prompt; announced
  (`Wizard::busy`) but not shortened (`bins/svolder/src/wizard.rs`, `bins/svolder/src/scanning.rs`).
- [trade-off] **`daemon install` waits five seconds for a stuck daemon.** A daemon that will not let
  its socket go holds `daemon install` for `STOP_WAIT` (five seconds) and then fails the verb, with
  the unit files written and nothing enabled; running the verb again once the daemon is dealt with
  finishes the install (`bins/svolder/src/commands/daemon/install.rs`, with the wait in
  `bins/svolder/src/commands/daemon/lifecycle.rs`).
- [trade-off] **Move to computer flattens the DAC first.** Putting the preset that is on the DAC
  on the computer, with `svolder host eq apply NAME` or `Enter` in the presets box on the computer's side, asks first and on
  yes flattens the DAC before the apply, so the correction is never on both; a failed apply leaves
  the DAC flat, and the message says how to put the curve back (`svolder eq load NAME`, or `Enter`
  on it in the presets box) (`bins/svolder/src/commands/host_eq/verbs.rs`,
  `bins/svolder/src/tui/boxes/presets/computer.rs`).
- [trade-off] **A walk with no daemon lists no rows while it listens.** The local walk's scan screen
  shows "listening…" and then the result, because the scan answers all at once; only a daemon's walk
  lists the rows heard so far (`bins/svolder/src/wizard.rs`, `bins/svolder/src/tui/wizard.rs`).
- [trade-off] **A preset list's fit reads a finer grid than the previews.** The CLI's and the TUI's
  preset lists take each imported preset's fit as found, on its own 256-point grid, while the
  previews, the overlay and the TUI's tone header measure the gap on 181 points, so a line and a
  header can differ by a hair (`crates/svolder-profiles/src/tone/fit.rs`).
- [debt] **Most CLI paths read any failed connect as no daemon.** `svolder_ipc::is_running` is a
  bare connect, so a socket that is there but cannot be opened (another user's, a path too long)
  reads as no daemon for `svolder devices`, the reload after a CLI write, the capture hand-over and
  the summary, which then go on without it; `web on|off` tells the two apart through
  `Client::connect_if_any` and names the socket (`crates/svolder-ipc/src/socket/client.rs`,
  `bins/svolder/src/commands/device.rs`, `bins/svolder/src/commands/device/devices.rs`,
  `bins/svolder/src/commands/capture/handover.rs`).
- [trade-off] **`svolder host crossfeed` with no daemon assumes 48 kHz.** `show` has no sink to ask,
  so it works the delay floor out at 48 kHz; a daemon uses the sink's rate, so the two can differ by a
  few hundredths of a millisecond (`bins/svolder/src/commands/host_crossfeed.rs`, `offline`).

### TUI

- [bug] **The TUI writes some settings itself while a daemon runs.** User preset writes, the MQTT
  keys, the Web UI and MCP on/off keys and the guest view go through the daemon, but the Connect
  keys for the listen addresses and the other MCP and web settings, the `[tui]` and `[autoeq]`
  rows and a device's own edits write `config.toml` in the TUI's own process and then ask the
  daemon to reload (`bins/svolder/src/tui/run/connect.rs:227`,
  `bins/svolder/src/tui/run/look.rs:267`, `bins/svolder/src/tui/run/devices.rs:206`).
- [limit] **A key pressed in the instant the terminal is resized can be lost.** The first key
  after the window is grown or shrunk, or after a run of Esc presses closes boxes, such as a `q` or
  Ctrl-C, can go unread until the next input (a second press reads both), and a frame at the old
  size can stay until the next input too. The cause is in crossterm 0.29.0's unix reader (`try_read`
  returns at the first token of a batch of readiness events; the Esc case is likely the same); there
  is no upstream fix, and svolder does not vendor or fork the library for it
  (`bins/svolder/tests/tui_pty/harness.rs:516`).
- [limit] **Over SSH in a VTE terminal a pairing code and the MCP token cannot reach the local
  clipboard.** VTE (GNOME Terminal, Ptyxis) ignores OSC 52, and only the Web UI box's link is a
  terminal hyperlink (OSC 8), so a code or token copied in a session with no display has no route
  to the clipboard of the computer in front of you; the box says the sequence was sent, never that
  it was copied. The ways round it are `ssh -X` (the copy then pipes to `xclip` or `xsel` and
  lands on your display's clipboard), a Shift+drag selection of the text on screen, or running
  `svolder mcp token` or `svolder web pair` in a terminal on the local computer
  (`bins/svolder/src/tui/clipboard.rs`, `bins/svolder/src/tui/run/web_box.rs`).
- [limit] **A session that inherits a local `DISPLAY` over SSH copies to that display.** A shell
  that keeps the desktop's `DISPLAY` or `WAYLAND_DISPLAY` (a tmux started at the desktop and
  attached over SSH) makes the copy tools run against that session's clipboard, which is the
  computer's, not the one you sit at, and the note says `copied with xclip` without naming it
  (`bins/svolder/src/tui/clipboard.rs`).
- [limit] **`svolder demo` fails to start under a long `TMPDIR`.** It makes its private directory,
  and so its daemon socket, under the temp directory; a `TMPDIR` of over about 90 bytes makes the
  socket path pass 108 bytes (`bins/svolder/src/commands/demo.rs`).
- [limit] **A demo whose daemon died says so with a toast, not a word on the screen.** When the
  daemon that `svolder tui --demo` started ends, the TUI toasts it at once and again every 30 s; the
  Status pane carries no word for it, and the TUI's player watcher and sink poll still start for a
  daemon that is gone, so the screen keeps what it last drew (`bins/svolder/src/tui/start.rs`,
  `bins/svolder/src/commands/demo.rs`).
- [limit] **The own-file prompt of the virtual speakers' set list has no path completion.** `Tab`
  there switches the file's room, so a path is typed whole, and the daemon reads it on its own
  machine (`bins/svolder/src/tui/boxes/speakers.rs`).
- [trade-off] **A quit with Settings writes queued behind a held lock gives the later ones up.**
  Settings writes run one at a time, each waiting up to two seconds for the file's lock; when one
  fails at quit (a lock the daemon holds past its wait) the writes still queued fail the same way,
  so a quit waits once, and each is toasted `not saved: the file is busy`
  (`bins/svolder/src/tui/run/writes.rs`).
- [trade-off] **A joined TUI starts its own capture when its daemon dies, and a daemon too slow to
  answer is left alone for 30 s.** While the daemon's frames connection is open the TUI's own
  capture stays stopped, and when the daemon goes away the own capture takes over, so the Spectrum
  row never waits on a daemon; a request for the frames that connects and is not answered within 3 s
  (`Opened::Mute`) bars the connection for `FRAMES_RETRY`, 30 s, and the own capture feeds the row
  meanwhile (`bins/svolder/src/tui/run/host/frames.rs`).
- [trade-off] **The footer has no tooltip.** The footer is only the setup wizard's key line, which
  has no app state and gets no mouse-move events; the main screens' key words have tooltips on
  their pane borders (`bins/svolder/src/tui/boxes/tooltip.rs:386`).
- [trade-off] **Cover art is drawn on the device screen's Status pane only, and some terminals
  get half-blocks.** Other screens keep the picture but draw none. WezTerm and Konsole are never
  asked for the kitty or sixel protocol (the image library's own exclusion), so they show
  half-block cells; `cover_art` is read once at start, so turning it from off to auto later draws
  half-blocks until the TUI restarts (`bins/svolder/src/tui/screens/device/art.rs`,
  `bins/svolder/src/tui/art/probe.rs:49`).
- [trade-off] **A replaced kitty picture stays in the terminal's memory.** The image library keeps
  the picture's id private, so svolder sends no delete and the terminal evicts old pictures itself
  (`bins/svolder/src/tui/art/engine.rs:204`).
- [trade-off] **A terminal answer split across two reads can leave one stray Esc key.** The
  answer filter waits a short time for the rest, and one that arrives later is read as an Esc
  (`bins/svolder/src/tui/art/filter.rs:91`).
- [trade-off] **A click far from a volume bar's level writes less than the bar's end value.** The
  click is paced by the climb of the volume rule, so the level reaches the clicked place a step at
  a time (`bins/svolder/src/tui/screens/device/controls.rs`).
- [trade-off] **A Loki Max's own preset list takes no mouse.** The TUI's preset list does; the
  device's list in its own screen answers keys only
  (`bins/svolder/src/tui/screens/device_presets.rs`).
- [trade-off] **A tone edit waits out another front end's preset recall.** While a recall from
  Home Assistant or MCP is running on the daemon, a tone-control edit from the TUI waits for it to
  finish, and the TUI waits with it, redrawing and reading keys again only once the write is
  answered, because it waits for each write's reply; the wait ends at the daemon's 60 s deadline
  at the latest (`bins/svolderd/src/writer.rs`, `bins/svolder/src/tui/run/events.rs`,
  `bins/svolder/src/backend.rs`).
- [trade-off] **Imported presets are fitted on the UI thread.** An imported preset is fitted the
  first time the tone screen's header, the device screen's Tone pane or the preset list draws it,
  or when the list opens (`presets::reread`), and `fit_for` keeps each fit for the rest of the
  process; with ten or more imports the screen pauses for about 0.3 s once
  (`bins/svolder/src/tui/boxes/presets.rs`, `crates/svolder-profiles/src/tone/preset.rs`).
- [trade-off] **Bands past the ninth have no digit key.** The tone screen's digit keys reach a band
  by `1`-`9` only, so a custom profile with more than nine bands (every built-in tone control has
  three or five) has bands past the ninth that only `↑↓` and the mouse can focus
  (`bins/svolder/src/tui/app/tone.rs`, `bins/svolder/src/tui/keys.rs`).
- [trade-off] **A recalled preset's name lives as long as the daemon.** The name a recall put up
  (for the `*` the tone screen's header and the device screen's Tone pane show) is kept by the
  daemon in memory, per device and for every front end, and written nowhere: restarting the
  daemon starts back at whatever the readings alone say, and a TUI session with no daemon keeps
  the name for that session only (`bins/svolderd/src/writer.rs`,
  `bins/svolder/src/tui/boxes/presets.rs`, `PresetsBox::loaded_preset`).
- [trade-off] **The Tone pane has no edit line.** `Enter`'s edit line for a band's frequency, gain
  and Q opens on the tone screen only; the device screen's Tone pane keeps `←→`'s one-step nudge and
  has no edit line or `Enter` hint of its own (`bins/svolder/src/tui/boxes/edit_line.rs`,
  `bins/svolder/src/tui/app/controls.rs`).
- [trade-off] **The 80-column edit line does not say what its arrows do.** At 80 columns the edit
  line writes its keys short, `Tab ←→ ↑↓ · ↵ writes · Esc cancels`, with no word for moving
  between the fields or stepping one, since the full form (`Tab/⇧Tab ←→ moves · ↑↓ steps ·
  Enter writes · Esc cancels`) does not fit beside the three fields at 78 inner cells
  (`bins/svolder/src/tui/screens/tone.rs`, `LINE_KEYS_SHORT`).
- [trade-off] **The clipping-risk note is shortened at 80 columns.** The clipping-risk note drops
  "on loud masters" at 80 columns (`peaks at +6.0 dB — keep the volume at or below -6.0 dB to avoid
  clipping`), since the whole sentence does not fit the Bands block's bottom border there; from
  about 92 columns it prints in full (`bins/svolder/src/tui/screens/tone.rs`).
- [trade-off] **A failed write does not stop a TUI preset load.** The TUI does not stop a preset
  load part-way on a failed write, unlike the CLI's `eq load`: every key's writes, presets included,
  are sent in order without waiting for the device's answer, and `take_intents` goes on past a
  write that failed, so a mid-plan failure is noticed (`App::write_failed`) but does not hold back
  the writes behind it (`bins/svolder/src/tui/run/input.rs`, `bins/svolder/src/tui/run/events.rs`).
- [trade-off] **A preset saved from the computer's points shows in the list when it is next
  opened.** Joined to a daemon, `n` on the computer's side sends the save and the list on screen
  does not refresh; closing and opening it (`p`) shows the new name
  (`bins/svolder/src/tui/run/computer.rs`).
- [trade-off] **The tone screen shades the gap per column at its own width.** Its header reads the
  daemon's 181-point spans, so at a span's edge the shade and the web page's can differ by a column;
  the second-source overlay, the marks for where a preset lives and the comparison are the web
  page's alone (`bins/svolder/src/tui/screens/tone/fit.rs`).
- [bug] **The TUI's own config writes do not ask a daemon whether its file is missing.** The CLI
  refuses a write while the daemon says its file is missing, but the TUI's writers go straight to
  the file, so a write made there recreates it from the template and the daemon's next reload takes
  it as the whole configuration, dropping every device (`bins/svolder/src/tui/run/`).

- [limit] **The path line counts the DAC's tone control as shaping whenever it is on.** A tone mode
  of EQ or loudness is named as the DAC shaping the sound even with every band at 0 dB, and the
  verdict stays `untouched` while the DAC lowers the volume: the DAC's stage is its own clause after
  the verdict (`crates/svolder-ipc/src/host/path.rs`, `DacStage`).
- [trade-off] **A long Path row scrolls its config chip with it.** With `scroll_long_lines` on (the
  default) a Path row wider than its room scrolls and the chip is in sight only part of the time;
  with scrolling off the verdict is cut with `…` so the chip stays whole. `k` always opens the box
  (`bins/svolder/src/tui/screens/device/controls.rs`, `path_rows`).

### Web UI

- [bug] **Some controls are faint or small.** Hairline edges of buttons and fields, in the pages and
  the popups alike, fall under 3:1 in all four themes (291 pairs held in the contrast baseline,
  Chromium), and 186 targets on the theme pages, none in a popup, are under 24 px or cut by an
  ancestor at phone width: hairline edges of active controls in all four themes, the graph's gap
  wash against its plot, Paper's dark red note, a guest's dimmed Signal table heads, Ember's 23 px
  `button.viz-graph` and 14 px `button.np-fold`, Paper's 20 px band steppers, Signal's slide
  carousel cut at its edge, Night drive's `nd-time`; Night drive's rotated pink `dB` label stands
  over its pink neon line at 1.0:1 where they cross. Text a canvas or a pattern paints, and Ember's
  text over its smoke, the sweep cannot measure (`scripts/web-checks/contrast-baseline.json`,
  `scripts/web-checks/phone-baseline.json`).
- [limit] **Plain HTTP sends the sign-in over the network.** On a listen address off loopback the
  cookie, the token typed on the sign-in page and the pairing code cross the network in the
  clear; svolderd speaks no TLS, which a reverse proxy adds (`bins/svolderd/src/web.rs`).
- [limit] **A standalone window needs HTTPS.** Browsers install a web app only from a secure
  origin, so over plain HTTP "Add to Home Screen" gives an icon that opens a browser tab
  (`bins/svolderd/src/web/router.rs`).
- [limit] **A link's address on a wildcard listen is a guess.** Without `[web] url`, `svolder web
  --qr` and the TUI's Web box use the first up LAN IPv4 address, which may not be the one a phone
  reaches; and a phone signed in to an IP address loses its sign-in when that address changes,
  since the cookie belongs to the address. With `[web] hosts` set and no `[web] url`, the first LAN
  address is not among the names the server answers to, so the phone is refused (403): set `[web]
  url` (`bins/svolder/src/commands/web.rs`).
- [limit] **More than five tabs of one daemon queue their streams.** Browsers open at most six
  HTTP/1.1 connections to one host, and each tab holds one for its event stream
  (`bins/svolderd/src/web/events.rs`).
- [limit] **Guests' streams count against the server's connections.** With `guest = "read"`,
  anyone who reaches the page may hold an event stream open, and 64 of them fill the listener's
  connections until they close; keep guest read to a network you trust
  (`bins/svolderd/src/http.rs`, `bins/svolderd/src/web/events.rs`).
- [limit] **A guest's read view on a wildcard listen can be read through DNS rebinding.** With
  `guest = "read"` on `0.0.0.0` or `::` and no `[web] hosts`, the server answers any `Host`, so a
  web page a visitor on the network opens can make their browser read the guest view (device
  names, values, preset names); the config warns, and `[web] hosts` closes it
  (`crates/svolder-ipc/src/config.rs`, `bins/svolderd/src/http.rs`).
- [limit] **A pinch whose first finger already began to scroll cannot be stopped.** On the web
  page's graph, a finger put down away from every point is the page's, and once the browser has
  begun to scroll for it a second finger cannot take the touch back; put both fingers down
  together, or the first on a point, to pinch the Q (`bins/svolderd/web/themes/ember/tone-graph.js`).
- [bug] **An open list does not move when the window narrows.** A phone turned with a list open
  scrolls the popup's pane sideways, by up to 198 px, until the list closes
  (`bins/svolderd/web/core/list.js`).
- [bug] **A page left on a walk across a daemon restart keeps its old controls.** The page learns
  of the new run only from a walk event of that run; until one comes its controls stay, and a press
  names the old run, which the daemon refuses as `no_such_job`; the page then rests on its idle card
  with no word that the daemon restarted. A writer switched to read-only on the add page leaves
  their walk running until it ends or hits the 10-minute idle bound
  (`bins/svolderd/web/core/popups/devices/walk-wire.js`).
- [limit] **A fetch of a virtual speakers' set shows no progress on the web page.** The route
  answers once, when the download, check and conversion are done, and the daemon's job events do not
  reach the page, so a running fetch says "Fetching..." with Cancel and no count; the TUI and the
  CLI show one (`bins/svolderd/web/core/popups/speakers/wire.js`). A progress bar needs the
  speakers' job stream forwarded to the web events.
- [trade-off] **The Devices popup words some readings as numbers.** A choice's raw value (the input)
  and a readout's (the sample rate) have no words in the page's model, so their range under a graph
  and in its text alternative is drawn as numbers, and the readings table shows the daemon's words
  with the raw figure (`63 (−26.1 dB)`) where the strip shows `−26.1 dB`; only a connected device's
  last reading is worded. The series route could send each series' value names, and a table row a
  key and a raw value (`bins/svolderd/web/core/popups/devices/draw.js`,
  `bins/svolderd/src/web/model/device.rs`).
- [trade-off] **The web UI's endpoints are private to its page.** Everything under `api/v1/` is
  shaped for the page and may change in any release; scripts use the CLI, the socket, MQTT or MCP
  (`bins/svolderd/src/web/api.rs`).
- [trade-off] **The sign-in page wears Ember's look in every theme.** The sign-in page is a
  template of its own that links Ember's style sheet, and no theme draws into it, so a person who
  chose another theme sees Ember's look until they are signed in
  (`bins/svolderd/templates/signin.html`).
- [trade-off] **A narrow Now playing cell in an arranged band drops its cover.** When Arrange puts
  three sections in the top band on a desktop, a Now playing cell under 400 px keeps the title, the
  artist, the buttons and the bar and leaves the cover out, since the cover and the transport do
  not both fit (`bins/svolderd/web/themes/ember/style.css`).
- [trade-off] **`svolder web` and a click on the TUI's Web UI link open the browser with the
  one-use link on `xdg-open`'s command line.** For the second or two before the page spends it,
  the code in the link is readable by other local users, so on a computer that others log into
  use `svolder web --qr` or `svolder web pair` (`bins/svolder/src/commands/web.rs`,
  `bins/svolder/src/tui/run/web_box.rs`).
- [limit] **A browser on software WebGL runs Smoke on the CPU.** A browser whose WebGL2 is drawn in
  software (llvmpipe, SwiftShader) passes the page's check for Smoke, so the page offers it and
  runs it, and the simulation then loads a processor core; pick Glow in the effects menu there
  (`bins/svolderd/web/themes/ember/smoke.js`).
- [trade-off] **The Devices popup follows a change made elsewhere at its 10-second re-read.** A
  profile, passphrase or default changed in another tab or the TUI reaches a device's page when the
  page reads the list again (every 10 seconds, or at once when the change moves the family, the
  link's word or a hop), and a resting tab sees it when it wakes. The ages in the facts count from
  the page's own clock, and the daemon sends unchanged readings about once a minute, so an age can
  be up to about two minutes behind (`bins/svolderd/web/core/popups/devices/wire.js`,
  `bins/svolderd/src/web/view/facts.rs`).
- [trade-off] **Removing a device moves the page behind its popup without a word.** The page falls
  to the fleet's first device, the Overview names the device removed, and no toast says so; with the
  last device removed every theme shows its no-device card, whose look has not been checked by eye
  (`bins/svolderd/web/core/stream.js`).
- [limit] **Night drive's dash EQ key has three looks not checked by eye.** A device with three
  input chips beside the key, an amp's gauges beside it and a phone held sideways (landscape) with
  the key have been measured by the checks only, never looked at
  (`bins/svolderd/web/themes/nightdrive/draw/dash.js`).
- [limit] **Below 1280 px with the visualizer on, the config chip pushes the last hops of a changed
  path into the dots chip.** One of three hops at 820 px and two at 1100 px move there; the hops
  stay in the verdict's words and the dots chip's tooltip, and `playing.py` pins the counts
  (`bins/svolderd/web/themes/ember`, `signal`).
- [limit] **On a phone Paper's moved notes stand outside `#p-notes`, and a redraw while the config
  chip has focus drops the focus.** The popup's Escape returns to the chip by its id, `p-config`
  (`bins/svolderd/web/themes/paper`).
- [trade-off] **A cancelled AutoEq download runs out its thread.** A preview or import that is
  cancelled stops waiting at once, but a download already under way runs on its blocking thread to
  its limit (20 seconds for a correction, 60 for the index), since the HTTP client has no cancel
  (`bins/svolderd/src/handler/autoeq.rs`, `crates/svolder-autoeq/src/client.rs`).
- [trade-off] **The Directions picture draws only the two measured angles.** The view carries no
  per-angle ear differences, so a No room · HRTF set shows its two speakers and not every measured
  angle; that needs a field in the view (`bins/svolderd/web/core/popups/speakers/pictures.js`).
- [trade-off] **The room choice cards pick the set in use, else the first fetched one.** There is no
  "last used per room" memory (`bins/svolderd/web/core/popups/speakers/draw.js`).
- [trade-off] **The W2 glow's red over zone is under 3:1 against the face.** In the warm backlight
  glow the zone's red (`.sg-w2 .zone`) reads about 2.0 to 2.8 to 1 against the amber face, below
  the 3:1 that graphics ask for. The numerals' `+` signs still mark the zone, and the look was kept
  as built (`bins/svolderd/web/themes/signal/style.css`).
- [trade-off] **Signal's browser bar colour follows the theme, not the palette.** Someone who keeps
  Warm amber, Phosphor or Oxblood and brass in Signal gets Blueprint's deep blue bar over their
  ground, because the colour the page tells the browser is one per theme. Following the palette
  would mean `paintChrome` taking its ground (`bins/svolderd/src/web/themes.rs`,
  `bins/svolderd/web/core/scheme.js`). - [observed] **At 900 px wide Signal's Player hop label can
  be clipped.** The label ("Fake Player",
  its 48 kHz line) was seen cut at the left edge of the line at that width in the stills; where
  the labels are placed is `bins/svolderd/web/themes/signal/draw/line.js`.

## Maintainer notes

### Bluetooth and sessions

- [test, with Bifrost 3 testing] **The release bookkeeping is proven over a fake peer.** The BlueZ
  transport records each connect's task and each release's per device until it ends, so
  `settle_for` waits for its own device's work only and two settles both wait; a link nobody took
  is taken down once, through itself; a caller that leaves mid-open has the open stopped and the
  connection taken down at once; a panic in the open steps still takes it down; a link whose
  `disconnect` failed is asked again by its drop, on a recorded task. The bookkeeping is
  unit-tested over a fake peer, and the stop of a scan start whose caller left over a fake
  discovery. Only `make e2e` exercises the transport's wiring (the release closure choosing
  `link.disconnect()` or the peripheral's own take-down, `Peer for Peripheral`,
  `Discovery for Adapter`, `open` building the `Hold` with its device, the `settle_for` calls in
  `connect`), and what only an adapter shows is untested: that BlueZ's `Disconnect` cancels a
  connect still in progress, which the release of an open the caller left relies on
  (`crates/svolder-ble/src/bluez/releases.rs`, `crates/svolder-ble/src/bluez/transport.rs`).

### Daemon and IPC

- [bug] **A probe dropped unfinished starts no actor until the next reload.** A probe future dropped
  by a runtime shutdown frees the slot but does not start the device's actor; one whose `Shutdown`
  is not answered within `SHUTDOWN_DEADLINE` also leaves the slot, so the walk's writes go on and
  the actor starts while the probe's session is still closing, and only a wedged actor reaches it. A
  unit whose spawn failed at a reload is retried, and logged, at every probe end; `start_unwatched`
  appends the actor it starts to the set, so the order differs from the file's until the next reload
  (`bins/svolderd/src/handler/probe.rs`, `bins/svolderd/src/handler.rs`).
- [bug] **An own set's id is 32 bits of the source's SHA-256, and a save overwrites the id.** `own-`
  plus 8 hex characters, and `Store::save` replaces an existing id, the pinned one included. Sixteen
  hex characters, or refusing a save over the pinned id whose source differs, would close it
  (`crates/svolder-speakers/src/import.rs`).
- [limit] **With no live updates, a sink rate change does not re-check the set's WAV.** The chain is
  not watched then, and `rebuild_for_missing_set` runs from `watch` only. svolder never removes a
  pinned WAV, so only one deleted by hand reaches it (`crates/svolder-host/src/eq/control.rs`,
  `step`).
- [trade-off] **`Client::request` skips a mismatched response id.** `Client::request`'s reply loop
  skips a `Reply::Response` whose id does not match the pending request (`_ => continue`) rather
  than reporting a protocol desync. Every caller today awaits each request to its end on its
  `Client`, so this fires only on a confused server, or for a caller that dropped a request future
  after its line went out, whose late answer it then passes over. An id-0 `Error` reply, which the
  server sends for a request line it cannot read, is taken as the answer to the pending request
  (`crates/svolder-ipc/src/socket/client.rs`).
- [trade-off] **`accept_loop` retries a failed `accept` forever.** `accept_loop`'s 100 ms
  retry-and-continue after a failed `accept` has no cap, so a permanently broken listener retries
  forever instead of eventually reporting itself dead; the comment beside it explains why retrying
  at all is intentional for a long-lived daemon (`crates/svolder-ipc/src/socket/server.rs`).
- [trade-off] **Link changes are kept twice.** The link `History` each device's actor answers
  `history` and `readings` with, and the `history::Link` the TUI's graphs hold (seeded from the
  `readings` answer and extended by what the TUI sees itself), are kept apart; merging them is
  debt (`bins/svolderd/src/device/link.rs`, `crates/svolder-profiles/src/history.rs`).
- [trade-off, 0.28.0] **The position and the rate are asked for with the player's own timeout.**
  The follower asks a player for `Position` and `Rate` one after the other when it follows it, on
  each signal and once more 500 ms after a new track, each bounded by the one-second call timeout,
  so a player that hangs adds up to two seconds to the stall its `Identity` read already causes
  (`crates/svolder-host/src/players.rs`).
- [trade-off, 0.28.0] **A track is first sent with no cover, then with its hash.** The cover is
  read off the host task, so a front end sees a new track with `cover: null` and then the same
  track with the picture's hash; the cover's box is fixed, so nothing moves, and waiting for the
  read would delay every track change (`bins/svolderd/src/host.rs`).
- [trade-off] **A stream replays every ended job to a follower that opens or lags.** A family
  replays the last state of up to 16 ended jobs, results included (512 KiB each), and a `progress`
  payload has no bound, so a subscriber that lagged reads states it already had; states are
  idempotent, and a front end must not count events. `stop_all` aborts a job given up on after its
  bound, but a `spawn_blocking` thread it left runs on until it looks at its flag, and the runtime's
  stop bound ends the process regardless; the bounded end is covered by the unit tests of the job
  bound, not by a process-level test (`bins/svolderd/src/handler/jobs.rs`,
  `crates/svolder-ipc/src/socket/protocol/timeouts.rs`).
- [trade-off] **A client's wait for the first line of a stream has no bound of its own.**
  `Client::stream` waits as long as the caller lets it; the TUI wraps the open in a timeout
  (`OPEN_WITHIN`), and the CLI's whole connect-and-request sequences in their own, not in
  `Client::request_within`, because their bound covers more than one request
  (`crates/svolder-ipc/src/socket/client.rs`, `bins/svolder/src/tui/run/host/frames.rs`).
- [limit] **A chain that dies is restarted by the exit watch only after it was applied or seen.**
  The watch is armed when the chain is first applied or seen in the graph, so a non-container daemon
  that finds no chain at start still does not apply one; a chain that dies while the daemon is a
  container's and the state says on is restarted as any death is (a crash loop leaves the EQ off). A
  unit that systemd restarts itself (`Restart=on-failure`) may also be restarted through apply by
  the watch; a second apply replaces the first and never stacks
  (`crates/svolder-host/src/eq/control/exit.rs`).
- [limit] **Crossfeed and the virtual speakers run up to 384 kHz.** Above it they turn off with a
  note and turning them on is refused: their headroom is shown up to it
  (`crates/svolder-host/src/eq/manage.rs` (`STAGE_RATE_MAX_HZ`)).
- [limit] **Sinks above 768 kHz are not covered by the stage cost.** A stage a configuration holds
  is built again with the graph at whatever rate the sink moves to, until the next look turns it
  off (about a second); its cost takes in rates up to 768 kHz (`CONFIG_STAGE_RATES`), not above.
- [limit] **The chain's f32 coefficients cannot hold low, sharp bands at high rates.** An HSC at 20
  Hz, -12 dB, Q 0.1 or a PK at 20 Hz, -12 dB, Q 10 at 176.4 kHz play more than 3 dB off at DC or
  their centre, and an LSC at 21.5 Hz, +9.63 dB, Q 1.91 at 384 kHz plays +3.5 dB at DC for +9.6; the
  preamp is measured on the f32 coefficients, so it is safe, but not what the view shows, and a
  fidelity refusal would break `every_hand_corner_is_playable_at_every_rate`
  (`crates/svolder-host/src/eq/manage.rs` (`check_playable`)).
- [limit] **A paused stream is not proved to keep a pass-through.** A real PipeWire cannot be made
  to pause a stream in `make host-e2e` (`pw-cat` stays running under a stop signal), so a corked
  `idle` stream node is pinned by the `props_of` fixtures only, and the lab never sets `playing`
  (`crates/svolder-host/src/eq/live/tests.rs`, `crates/svolder-host/src/eq/lab.rs`).
- [limit] **A chain started with nothing playing plays its saved config for up to a second.** After
  the first stream starts, the chain plays the config it was started from until the next look (up to
  `WATCH_EVERY`, 1 s) finds it run and sends everything again, so crossfeed turned on idle is heard
  about a second late and an idle edit plays the applied curve for that second; the config is
  self-consistent, so it is safe (`crates/svolder-host/src/eq/control.rs`).
- [limit] **A kernel without pidfd cannot be waited on for the computer EQ's chain.** `Host::exited`
  waits on the chain's process through a pidfd (Linux 5.3 and later); where none can be opened it
  never resolves, so a chain that dies there is not restarted until the next apply or remove
  (`crates/svolder-host/src/eq/manage/exit.rs`).
- [trade-off] **A new default sink can reach the capture about 2 s later.** While the computer's
  volume is watched, the capture's look takes the reading the volume read published up to 2.5 s ago
  (`Coalescer::held`), so one `wpctl get-volume` serves both readers each 2 s beat; a change of the
  default sink reaches the capture up to about 2 s later than a fresh read would show it, the EQ's
  own writes drop the published reading, and a failed read is not shared (each reader times out for
  itself) (`crates/svolder-host/src/sink.rs`).
- [limit] **A recall that waits for a cold fit can be dropped.** A recall through the fleet that
  waits for an imported preset's first fit is answered "a newer write reached ... so <name> was not
  recalled" when any write or recall reached the device's writer meanwhile; the user repeats it, and
  only the first recall of a cold preset waits (`bins/svolderd/src/fleet.rs`,
  `bins/svolderd/src/writer.rs`).
- [limit] **`recall::warm` remembers its fits in a capped set.** The set holds as many keys as the
  profiles crate's fit cache (4096); if that cache is cleared on its own at the cap, a later first
  ask can fit inline once (`bins/svolderd/src/recall/fits.rs`,
  `crates/svolder-profiles/src/tone/preset.rs`).
- [trade-off] **A node found at the -120 dB floor bounds no preamp.** A chain 0.34.5 or 0.35.0 left
  silent holds a multiplier of 1e-6 (-120 dB) or 0, so a held preamp at or below -119.9 dB
  (`SILENT_HELD_DB`) is taken as no bound and the first look sends the saved preamp. A chain another
  process started on a curve that truly needs the floor (a peak of about 120 dB) is then bounded only
  by the saved curve's own preamp until the next apply (`crates/svolder-host/src/eq/control.rs`
  (`watch`)).
- [debt] **A leftover note hides the pass-through note.** In the view a leftover `note` (a restart
  or failure text) wins over "chain kept as a pass-through until idle", and `host eq status` and
  `remove` find the pass-through by that note, so after a restart they say stopped or removed for a
  chain that is kept; an explicit view field would fix it (`crates/svolder-host/src/eq/control.rs`
  (`view`)).
- [debt] **The hop and preset write routes answer without a `reason` and keep their own status
  table.** The device and series routes share one refusal table and carry a `reason`; the hop and
  preset routes answer from a `ConfigWriteError` with statuses the page's checks pin (409 for a
  preset's refusals), and folding them in changes those answers. The older request families (state,
  set, devices) answer without a reason, and only not-found-yet and not-connected carry one; a bad
  request of any kind other than `device_config` echoes serde's words
  (`bins/svolderd/src/web/writes/config_edit.rs`, `crates/svolder-ipc/src/socket/server.rs`).
- [debt] **The daemon sends the `strip` events to every stream.** The `needs` parse knows `meters`,
  `host` and `walk` and ignores `strip`, so every stream gets the strip whether or not it asked, and
  the page drops them client-side; the series route's `secs` over the ring's half hour is held to it
  and not refused (`bins/svolderd/src/web/events.rs`, `bins/svolderd/src/web/events/device.rs`,
  `bins/svolderd/src/web/api.rs`).
- [debt] **The MQTT client still fits inline on its own task.** The page model, the MCP tools, a
  recall through the fleet and a computer recall make an imported preset's first fit on a blocking
  thread (`recall::warm`), but the bridge's state publish reads which preset is on
  (`preset_state`) and its preset select recalls through `writer.recall`, both from synchronous
  code, so a cold imported preset fits there on the runtime (`bins/svolderd/src/mqtt_bridge/state.rs`,
  `bins/svolderd/src/mqtt_bridge/commands.rs`).
- [debt] **A reconcile dropped before any stop lets its new actors go.** Their handles drop and each
  actor closes its link by itself, unawaited; only the stop waits for actors
  (`bins/svolderd/src/handler.rs`).
- [debt] **Front ends know the missing-config refusal only by its words.** The wire
  carries `read_only` or no reason for it, so a front end matches the text; a
  `Reason::ConfigMissing` would let it match the reason (`crates/svolder-ipc/src/error.rs`,
  `bins/svolderd/src/handler/config_write.rs`).
- [debt] **The route's `pw-dump` is a read of its own beside the sink's `get-volume`.** The route
  carries the sink's volume and mute too, so a later pass could feed one read from the other and
  save about 0.5 spawn a second; the EQ's own 1 s look (`control.rs`) is a third read and is
  untouched (`bins/svolderd/src/host.rs` (`ROUTE_EVERY`), `crates/svolder-host/src/eq/control.rs`).
- [debt] **The computer EQ's process scan runs on the async threads.** `chains()` and `Argv::find`
  read `/proc` synchronously, and are called from `status`, `stop_process` and now `exited`: small,
  and blocking (`crates/svolder-host/src/eq/manage.rs`, `crates/svolder-host/src/eq/process.rs`).
- [debt] **Crashed virtual speakers' writes leave files nothing sweeps.** A store write goes through
  `<id>.tmp-<pid>` and a download through `.<id>.part`; after a crash they stay, since `list()`
  ignores them and only the import folder is swept at the daemon's start. A start sweep would do
  (`crates/svolder-speakers/src/store.rs`, `write_atomic`; `fetch.rs`, `Part`).
- [debt] **The controller's set reads run under its lock.** `refresh_sets` (a read and SHA-256 of
  every set, up to about 20 MB) runs at every speakers op and every computer construction, inside
  `HostService::reconfigure` with its `running` lock held, and `analysis::analyse` (about 4 ms at
  48 kHz, 12 ms at 192 kHz in release, 5 to 10 times that on a Raspberry Pi) runs in the same read.
  The clock's refresh reads sidecars only. Reading and checking only the chosen set, and moving the
  analysis to `spawn_blocking` or to import time with a cached sidecar, would remove the pause
  (`crates/svolder-host/src/eq/control/vs.rs`, `speakers`, `with_speakers`, `read_sets`).
- [test, with Bifrost 3 testing] **The shared listen is proven on fakes and on one device.** Every
  actor looks for its device through one `Scanner`: the listen that runs hears every device
  looked for while it runs, answers each the moment it is heard (the one that leaves nobody
  waiting once the scan has stopped) and stops once nobody is left; a device asked for more than
  a quarter second into a listen (or as it stops) and not heard in it is answered at the end of
  the next one. The scanner is tested over scripted transports. `BluezTransport::listen`, and the
  transport's own scan lock that makes a connect's `find` wait for a running scan instead of
  meeting BlueZ `InProgress`, run only in `make e2e`, with one peripheral: two Forkbeards on one
  adapter are the test still to make: check with two devices coming back in one window, since an
  early answer's connect may end the adapter's LE scan and leave the other device unheard in that
  window (`bins/svolderd/src/scanner.rs`,
  `crates/svolder-ble/src/bluez/transport.rs`).
- [test] **The walk's fleet turn has no test of its own.** `walk_turn` in the events page, the fleet
  event after a walk that wrote a device, is covered through the route tests of the walk's events;
  the device shows within a second anyway, by the page's second tick
  (`bins/svolderd/src/web/events/page.rs`).
- [test] **The preamp lab does not model a graph that has not run.** Its node always holds what is
  sent: no lab mode lists zeros, drops sends and starts from the config on the first run, so the
  not-run look's ordering and bound cases are pinned by `control/tests/idle.rs` and host-e2e S1 and
  S2 only (`crates/svolder-host/src/eq/lab.rs`).
- [test] **`the_reaper_thread_is_named` fails under heavy load.** It should wait on a condition
  (`crates/svolder-host/src/eq/manage/tests/process.rs`).
- [test] **Nothing guards the capture's held sink reader.** The daemon builds the capture's look
  with `sink.held()` so the two readers cost one `wpctl` call a beat; a test of the sink proves the
  sharing, and none proves the daemon's wiring uses it (`bins/svolderd/src/host.rs`).
- [debt] **The no-daemon TUI hands its sink task the loudness hint through a static.** `LOOP_HINT`
  is process-wide because the TUI makes `HostLink::session` before `ComputerLink::live`; wiring the
  hint through the first would remove it. Harmless: one loop per process
  (`bins/svolder/src/tui/run/computer.rs`).
- [debt] **Which flag sends a full line to a graph that has not run, one tick after an apply, is not
  pinned.** `resend` set by the `save_state` foreign-stamp check, or `reload` (which re-adopts the
  whole state and re-arms `unvouched`); harmless, an extra full line
  (`crates/svolder-host/src/eq/control.rs`).
- [debt] **`dacs_of` repeats the web view's reading fold.** It copies `Snapshot::raw`'s four-line
  reading-to-integer fold and the `dac_facts` rule that clears a stage while the link is down; one
  shared helper would end it (`bins/svolderd/src/mqtt_bridge/computer.rs`,
  `bins/svolderd/src/web/view.rs`).
- [test] **Two web tests fail under heavy load.**
  `two_recalls_waiting_for_fits_end_on_the_later_request_whichever_fit_ends_first` uses real fit
  timing and failed in 2 of 3 full runs, passing alone;
  `a_page_with_no_device_gets_the_model_fleet_and_host` failed once in 4 on the order of the playing
  and paused events (`bins/svolderd/src/web/view/tests.rs`,
  `bins/svolderd/src/web/events/tests.rs`).

### CLI

- [limit] **A unit's chain and a process chain look the same.** The computer EQ finds its
  PipeWire chain by its exact command line, which is the same under svolder's user unit and under
  the background-process manager (only the process's cgroup would tell them apart, and svolder
  does not read it); while the unit is active, a process-manager chain started by hand with the
  same command line is left running, so two could stack. svolder's own apply and remove never
  produce this (`crates/svolder-host/src/eq/manage.rs`).
- [limit] **The computer EQ's chain can be signalled by pid.** On a kernel without `pidfd_open`
  (before Linux 5.3), or where it is refused for a reason other than "no such process", the
  chain is signalled by pid after its command line is checked, which leaves the short window in
  which that pid could be given to another process (`crates/svolder-host/src/eq/process.rs`).
- [trade-off] **The computer EQ's saved state is re-applied as written.** Turning the computer EQ
  on again (Mode `EQ`, on the computer's side), while it is off, puts the last one back from what
  `host-eq.toml` kept: its filters are validated and a preamp that is not a number or is above
  +30 dB is refused, but a hand-edited preamp far below the safe value applies as written, only
  quieter (`crates/svolder-host/src/eq/manage.rs`,
  `bins/svolder/src/tui/boxes/presets/computer.rs`).
- [trade-off] **The computer EQ reads PipeWire's graph once a second while it is on.** The
  controller runs one `pw-dump` every 20 ticks for as long as the chain is on and live, whether
  or not anyone watches: it is how a change of the sink's rate or a restart of the chain is seen
  within about a second and every slot sent again at the new rate. On a graph with many nodes the
  dump is large. A narrower read (a dump on the sink's format change only) would need a PipeWire
  subscription the code does not have (`crates/svolder-host/src/eq/control.rs`).
- [trade-off] **The status line hardcodes the word "Status".** `format.rs`'s status line hardcodes
  the word "Status", dropping the wire's Command/Status distinction; harmless today because
  `ValueObject`, the only thing this renders, represents a device's reported readings and never
  carries a Command (`bins/svolder/src/format.rs`).
### TUI: device screen

- [trade-off] **A history graph's columns are recomputed every frame.** `graph::columns` walks
  every sample inside the span for each graph on each draw, and `Link::columns` copies up to 4096
  changes. Measured in release: about 115 µs per series at the cap (14,400 samples, 60-minute
  span), 4.6 ms for 40 such graphs; a Kraken's five meters at 60 minutes cost about 0.6 ms per
  frame, a tenth of that at the default 5 minutes. A cache would have to be keyed on the sample
  count and a column boundary of a window that slides with every instant, which changes what a
  frame draws between boundaries, so it is left until a frame budget asks for it
  (`bins/svolder/src/tui/widgets/graph.rs`).

### TUI: effects

- [trade-off] **The trace starts one frame after its screen appears.** The animation layer is
  armed from the frame that has just been drawn (`arm_effects`), so under the `trace` transition
  (the default) a new screen's trace, or its body's sweep, starts one frame after the frame that
  first shows the screen: the screen is visible untraced for that frame. The `noise` and `melt`
  transitions start before that frame (`Effects::on_navigation`) and cover it. Arming the trace
  before the draw would also need its first frame held at its start while everything else running
  advances by the time since the frame before, which the one `process_effects` call a frame makes
  does not offer (`bins/svolder/src/tui/run.rs`, `bins/svolder/src/tui/run/look.rs`,
  `bins/svolder/src/tui/decoration/effects/passage.rs`).
- [trade-off] **Two narrow effects edges stay by design.** A terminal shrunk below 80×24 while an
  overlay is open and grown back before it closes traces nothing when the overlay lifts: the frames
  under an overlay are never handed to `Effects::appeared`, so the ones recorded before it count as
  still drawn. The light running round a focused frame can paint a wider catch-up flash than its
  usual three-cell band on the lap that also turns `active()` false, which can sit on the contour
  for up to one slow tick; noted as a bounded, self-correcting edge in `lap`'s doc
  (`bins/svolder/src/tui/decoration/effects/keyed.rs`, `bins/svolder/src/tui/run/look.rs`).

### TUI: mascot

- [trade-off] **With effects on, a lost link shows the lantern and a returned link a hop.** The
  ship rowing out and back is kept for effects off (`Arrive`, `Depart`, `RowIn`, `RowOut`); with
  effects on, `Brain::start` picks the lantern and the hop instead
  (`bins/svolder/src/tui/decoration/mascot/brain.rs:649`).

### TUI: event loop and opening devices

- [trade-off] **A joined TUI takes the whole frames stream for the VU rows.** The socket's frames
  request has no levels-only form, so a joined TUI whose visualizer, title meter and effects are
  all off still receives the bands (about 700 bytes a frame) when only the VU rows want a level;
  the web route has `?only=levels` for this
  (`bins/svolder/src/tui/run/host/frames.rs`, `Backend::host_frames`).
- [trade-off] **The VU rows hold the host's capture from 100 by 45, with everything else off.** The
  capture runs while the rows show, which is a size-only rule: a terminal of 100 columns by 45 rows
  or more starts `pw-record` even when the visualizer, the title meter and the effects are off and
  nothing else wants a level. `host_audio = false` is the way to switch it off
  (`bins/svolder/src/tui/app/meters.rs`).
- [trade-off] **`render::frame_areas` derives the layout the screens drew.** `frame_areas` calls
  the same pure layout functions (`curve_of`, `fault_line`, `logo_of`, each screen's `frames_of`)
  that the screens' `draw` functions use, rather than reading it back from them: the alternative
  changes five `draw` signatures for a function that cannot disagree with itself, and `render`'s
  `assert_bordered` cases compare what `frame_areas` reports with the cells drawn
  (`bins/svolder/src/tui/render.rs`).
- [trade-off] **The TUI's reload after its own write is not awaited at quit.** `send_owed` does not
  keep its task's handle, so a quit within a few milliseconds of the send, while the task is still
  in the version ask, drops the reload at runtime shutdown, and the next ask announces the TUI's own
  change as a hand edit. The 60-second poll of the file is 300 ms under `cfg(test)` so the loop test
  sees a second ask; no test runs the real period (`bins/svolder/src/tui/run/look.rs`,
  `bins/svolder/src/tui/run.rs`).
- [debt] **A frame with a cover still builds the Controls and Tone lines up to four times.** The
  layout is worked out once a frame, but its cover growth builds both panes to count their lines,
  the draw builds them again in colour, and the squirrel's home builds its pane once more; the
  clock's tick lays the screen out again for a title longer than the room beside the cover.
  Counting the lines without building them is the next step
  (`bins/svolder/src/tui/screens/device/art.rs`, `panes_need`).
- [trade-off] **The cover's picture is sized by the box the frame just drew.** `Art::steer` runs
  after the capture is steered; a capture that starts or stops there and moves the Status pane
  resizes the picture one frame later rather than laying the screen out again every frame
  (`bins/svolder/src/tui/art/engine.rs`, `Drawn::drawn_art_box`).
- [test] **A taken preset name's daemon answer is proved in loop and daemon tests only.** The TUI
  checks a name against its own copy of the library before it sends, so the daemon's answer to a
  taken name shows on screen only after a race; no pty case reaches it
  (`bins/svolder/src/tui/run/input.rs:30`).

- - [debt] **The Linkwitz, Meier and Mid/Side bass numbers are placeholders.** Their names, ranges
  and defaults are svolder's own choices; Mid/Side bass's own lag (about 1.9 ms at 120 Hz) is not
  shown as delay, and Bauer at 15 % is 15.06 dB of feed, just past bs2b's 15 dB limit
  (`crates/svolder-host/src/eq/crossfeed.rs`, `SPECS`). The preset names are wire ids: renaming one
  breaks an automation that selects it.
- [test] **No test reaches the rate at which the stage cannot play.** A coefficient past the chain's
  limit turns crossfeed off with a note, but no rate in the engine's ranges gets there
  (`crates/svolder-host/src/eq/control/xf.rs`).
- [test] **Most TUI loop tests wait on fixed real time.** `quit_after` deadlines remain in the loop
  tests of `mouse`, `link`, `keys`, `exit`, `meters`, `writes`, `daemon`, `curve` and `devices`;
  with 300 ms of delay injected into a frame draw seven of them fail
  (`a_daemon_restarted_under_the_tui_is_reopened_without_a_key`,
  `a_double_click_on_a_bands_gain_opens_the_line_on_it`,
  `a_left_click_on_the_painted_squirrel_plays_the_hop` among them); `pointer.rs` and `toasts.rs`
  already wait on the screen or a daemon condition (`bins/svolder/src/tui/run/tests/`).
- [limit] **The peak search assumes one peak per half octave.** `response`'s peak refinement runs
  a golden-section search over the two sample intervals either side of the best of 1024 samples
  and over a half-octave window round each filter's centre, and a golden-section search finds one
  maximum per window: two narrow boosts almost on top of each other could hide a peak the grid
  only covers coarsely, leaving the computer EQ's preamp, and the clipping-risk mark the `eq` verbs
  and the tone screen show, a fraction of a decibel short
  (`crates/svolder-profiles/src/tone/response.rs`).
- [trade-off] **Built-in presets are keyed by band count.** `tone::preset::builtins` gives Warm,
  Bass and Presence to a five- or three-band tone control by its band count alone (`shaped`), as
  fixed Hz, dB and Q chosen for the mesh-eq profiles' own bands: a future five- or three-band
  profile with other centres would still get a correct Flat (on its own centres) but a Warm, Bass
  and Presence shaped for mesh-eq's frequencies rather than its own. Harmless today: only
  `mesh-eq-5` and `mesh-eq-3` carry an `[eq]` block (`crates/svolder-profiles/src/tone/preset.rs`).
- [trade-off] **Fits match only within one build.** A fit is a floating-point optimisation whose
  exponentials, logarithms and sines come from the C library the binary is linked with: a CLI
  built against glibc (`make install`) beside a static musl daemon (the release packages', the
  container's) may land a fitted step one apart, so the two could disagree on whether an imported
  preset is `on`. Routing those calls through the pure-Rust `libm` crate would make every build
  agree, but no test could show it short of running the suite under musl
  (`crates/svolder-profiles/src/tone/fit.rs`, `crates/svolder-profiles/src/tone/response.rs`).
- [trade-off] **Key readings come in each product's own order.** `Profile::key_readings` gives
  `sample_rate volume input` for the Gungnir 2 (v2), Yggdrasil and Bifrost 3, `volume sample_rate
  input` for the Mimir, `volume sample_rate` for the Mesh Card and Eitr 2, `input sample_rate` for
  the Gungnir 2 (v1), and the Kraken's `meter_1 meter_2 warming_up meter_3 meter_4 meter_5`; making
  them equal moves controls in the profile files or gives the mark an order of its own, and moves
  the rows of every front end's readings table (`crates/svolder-profiles/src/model.rs`,
  `crates/svolder-profiles/tests/builtin.rs`).

### Web page

- [limit] **Chromium's Back button skips history entries pushed without a user gesture.** A popup
  opened by code, not from a tap or a key, would not be stepped by phone Back, and the checks'
  `history.back()` does not show it; every popup opens from a gesture, and the router says so in the
  console when a push has none (`bins/svolderd/web/core/router.js`).
- [debt, with Ember] **Ember keeps its own band step reading and writer chain.** `bandSteps` and
  `bandWriter` moved to the core for Paper, and Ember's `tone-graph.js` still reads its steps and
  writes its bands through its own chain (it uses the core's `BAND_PARAM`)
  (`bins/svolderd/web/core/tone-drag.js`, `bins/svolderd/web/themes/ember/tone-graph.js`).
- [trade-off] **The `ember-parts` fixture is 4.25 MB.** It holds the markup of every part of every
  device kind, so the node check compares the browser-drawn page with the daemon's own drawing
  byte for byte; a smaller fixture would prove less (`scripts/web-checks/node/fixtures/ember-parts.json`).
- [debt] **The theme style check is name-based, and a theme script that timed out may
  still run.** `theme-css-check.py` finds the frame's selectors by their names, so a theme that
  styles one through another selector is not seen; a theme script that timed out is given up on,
  but the browser can still run it when it arrives late
  (`scripts/theme-css-check.py`, `bins/svolderd/web/core/switch.js`).
- [bug, with Paper] **`?theme=paper` resets a stored Dark to Auto when the cookie names another
  theme.** The page paints the scheme the server sent and takes it as the person's choice at boot,
  so a link that switches the theme while another theme's cookie is set loses a stored Dark; the
  alternative is a flash of the wrong scheme (`bins/svolderd/web/core/core.js`).
- [debt] **The computer side rewrites the daemon's words with patterns.** `plain()` turns the
  daemon's path verdicts ("PipeWire is on USB 2") into the page's words with regular expressions,
  so a reworded verdict falls through unchanged. The real fix is a structured verdict from the
  daemon, a wire change (`bins/svolderd/web/themes/signal/draw/computer.js`).
- [debt] **The router's history has a few narrow edges.** The next `popstate` after a close counts
  as the router's own, so a Back of the person still in flight when Escape lands can step one entry
  too far, onto another device or off the page; a switch under a staying popup can leave two entries
  that name one device, and Back between them reads that device again and shows nothing new; an
  entry pushed by code that bypasses the router has no place; a page loaded on an entry the router
  did not write starts a new chain, so Back across documents can show the device before a *Show on
  the page*; the carried devices live in `sessionStorage`, so a tab that cannot write it forgets
  them at a reload; a fleet that lists no device replaces the model without telling `shown`, so the
  address still names the dropped device; a push the router delays behind its own step back is
  judged for a gesture a few milliseconds late (`bins/svolderd/web/core/router.js`,
  `bins/svolderd/web/core/stream.js`).
- [debt] **A theme can reach members meant for the frame.** `core.modal.key`, `core.popups.add`,
  `core.router.register`, `core.stream.open` and `close`, `core.stream.want('theme', …)` and
  `core.popups.listed` are on the one `core` every theme mounts with; a narrower object for a theme
  is a larger change than any popup needs. A popup added by hand with `core.popups.add` (the checks'
  sample) has no menu row and no viewer gate (`bins/svolderd/web/core/registry.js`,
  `bins/svolderd/web/core/popups/popups.js`).
- [debt] **Night drive's sheet and Signal's Add device sheet are not on the modal stack.** A popup
  opened over one makes it inert but leaves it standing behind
  (`bins/svolderd/web/themes/nightdrive/sheet-wire.js`,
  `bins/svolderd/web/themes/signal/hops-wire.js`).
- [debt] **The popup frame has two layout corners left.** In a place for lists a panel with room
  neither under nor over its chip stands against the pane's bottom, over the chip; `reach` reads
  the pane's overflow after a draw, a `put` and a size change of the box, so content that grows by
  other means is seen at the next draw (`bins/svolderd/web/core/list.js`,
  `bins/svolderd/web/core/popups/frame.js`).
- [debt] **A few core parts keep hidden coupling.** `freeze.js` gives an open list's hold back
  before a gesture's without being told which is which, which no theme reads today; `createLive`
  reads a reading that names no device as the shown device's (`bins/svolderd/web/core/freeze.js`,
  `bins/svolderd/web/core/keys.js`).
- [debt] **Help's Here and its About depend on the page's own state.** Here is read at every `show`,
  so a theme that draws the focused control again while Help stands leaves it showing the page's
  keys alone; About names `location.host`, which behind a proxy is the proxy's address, not the
  daemon's listen address (`bins/svolderd/web/core/popups/help/wire.js`,
  `bins/svolderd/web/themes/nightdrive/key-table.js`).
- [debt] **The computer's model read names the shown device.** `api/v1/host?device=` is a read, not
  an action, so a Devices popup that acts on the computer for another device still sees the host's
  model for the shown one until that read takes a device (`bins/svolderd/web/core/computer.js`).
- [debt] **The Devices popup names every listed device in its want, and `FILE_MAX` repeats a daemon
  limit.** The want is `follow=A,B,C` with the shown device included (the daemon follows a device
  once, so only the name is redundant); the EQ library page's 64 KiB upload limit repeats
  `apo::MAX_BYTES`, and the daemon still refuses past it, so a change of the limit needs both
  (`bins/svolderd/web/core/popups/devices/wire.js`,
  `bins/svolderd/web/core/popups/eq-library/wire.js`).

### Simulator and fake transport

- [trade-off] **`FakeDevice::handle` panics on a restricted frame.** `FakeDevice::handle` panics
  on a firmware-update frame (the `210` family but the `210.2` version query), which svolder never
  sends, so a test whose code sends one fails on the spot. In `svolder-sim` the panic lands in a
  GATT write callback while the device mutex is held: the process stays up, but the poisoned
  mutex makes every later write, and the `--signal` ticks, panic too, so the simulator stops
  answering until it is restarted. A panic hook that ends the process would show that sooner; it
  is left out because only a frame the codec already refuses to build can reach the panic
  (`crates/svolder-fake/src/device.rs`, `bins/svolder-sim/src/peripheral.rs`).
- [trade-off] **`drop_links` disconnects every device on its adapter.** `svolder-sim`'s `drop_links`
  disconnects every device connected on its adapter, which is fine for a dedicated e2e controller
  and not for one shared with anything else (`bins/svolder-sim/src/peripheral.rs`).
- [trade-off] **The devices `SVOLDER_FAKE_MORE` adds take few knobs.** The other `SVOLDER_FAKE_*`
  knobs (a fault, a severed or vanished link, the code, the firmware) describe the first fake
  device alone: each added device is seeded for its profile with the code `1234` and firmware `3`
  and never drops its link. `SVOLDER_FAKE_SIGNAL` is the exception: it plays on the meters of
  every device that has them (`crates/svolder-ble/src/fake/fleet.rs`).
- [debt] **The fake's scan hears every unit at once.** The picture of the walk's pick is a moment of
  listening; a daemon knob that slows the fake's scan would let a browser see it
  (`crates/svolder-ble/src/fake/fleet.rs`).

### Tests and tooling

- [limit] **`cargo audit` warns about two unmaintained crates and finds no vulnerability.** `paste`
  (RUSTSEC-2024-0436) comes in through simba, nalgebra's numeric layer (the EQ's curve fitting, in
  `svolder-profiles`), and `ttf-parser` (RUSTSEC-2026-0192) through `owned_ttf_parser` and
  `ab_glyph`, a dev-dependency of `bins/svolder` for the README screenshots; neither has an
  advisory against it, and svolder cannot replace either. `make audit` allows exactly these two
  ids and fails on any other advisory or warning; they are watched at each dependency refresh
  (`Cargo.lock`).
- [limit] **nalgebra stays at 0.34.** `levenberg-marquardt` 0.15.0, the latest, pins nalgebra 0.34,
  so 0.35 waits for it; `generic-array` 0.14.7 (0.14.9 is out) and `matchit` 0.8.4 (0.8.6 is out)
  are held by their dependents too (`crates/svolder-profiles/Cargo.toml`).
- [observed] **The Rust 1.100.0 and cargo-auditable paths are unproven until CI runs.** The release
  image's cargo-auditable 0.7.7 and the 1.100.0 toolchain of the `check` workflow have not run
  until CI or `make build-image` does; `make audit` writes its advisory database to
  `~/.cargo/advisory-db` (`Makefile`, `.github/workflows/check.yml`).
- [observed] **The `playing` check of Ember fails now and then in Firefox.** In the runs of
  0.31.2 it failed three times, with a 2 px overflow of the title marquee under load, a
  geckodriver websocket 405 and a websockets `InvalidMessage`, and each time passed when run alone;
  a failed full run is rerun with `python3 scripts/web-checks/playing.py firefox`
  (`scripts/web-checks/playing.py`).
- [observed] **`make e2e` fails after about fifty runs until bluetoothd restarts.** Once
  bluetoothd has handed out GATT handles past 0x3ff on the peripheral adapter, about fifty runs
  after it started, `make e2e` fails at the first reconnect after a simulator restart
  (`ATT error: 0x01`): BlueZ 5.85 stops updating its GATT Database Hash there, so the central
  keeps a stale cache. Restart bluetoothd (`scripts/e2e.sh`).
- [observed] **A few `haze.py` checks fail about one run in ten.** The haze's side sources wander
  from random phases that the wide and resize sections do not seed, so a side band or the column
  is read at an unlucky moment now and then; running `haze.py` again in that browser passes. The
  phone section is seeded through `effects.seed`; seeding the others needs the haze's random drawn
  from the seeded generator too (`scripts/web-checks/haze.py`, `bins/svolderd/web/themes/ember/smoke.js`).
- [observed] **`sources-chromium` failed once on a bound of the dye sum.** One full `make
  web-checks` failed its "40 full-size steps, one per frame: finite, max |v| < 60, the dye sum
  under 2.2 x the resting cloud" check with a dye sum of 5071.67 against a bound of 2.2 x 2305.26
  = 5071.56, 0.002 % over; the same check passed 45 of 45 three times run alone, and no smoke
  file had changed. The bound is not loosened; running `sources.py` again in that browser passes
  (`scripts/web-checks/sources.py`).
- [observed] **The cause of a flaky MCP test is inferred.**
  `a_reload_that_turns_mcp_off_closes_the_port` flaked once, and the cause, a foreign process taking
  the freed port, is inferred and not reproduced; the test asks whether the MCP server answers
  (`bins/svolderd/tests/mcp.rs`).
- [trade-off] **Firefox's Content Security Policy reports are not seen.** `make web-shots` fails on
  any uncaught error Chromium or Firefox logs while a page loads, and `make web-checks` on every
  error a script or a click raises after the load in both (`scripts/web-checks/`); both fail on any
  Content Security Policy report Chromium logs, and Firefox writes none in this mode. The policy is
  the daemon's one header, so Chromium's check stands for both (`scripts/web-shots.sh`).
- [debt] **The style and routes checks read code by pattern.** `theme-css-check.py` holds the
  frame's selectors, imports, key listeners and device-aware calls to what a pattern sees: an import
  path built elsewhere and handed to `import()`, a key looked up in a map (`KEY_STEPS[event.key]`),
  a listener reached through a call's result or an event name put together, an options object built
  before a call, a `{ device }` nested in an argument and a `post` to a device route in a popup's
  file go unseen; a theme sheet's own `@media` breakpoints are literals, the non-colour frame tokens
  (`--frame-mono`, `--frame-radius`, `--frame-tap`, `--frame-toast-lift`) are not checked, and its
  self-test takes about 37 seconds. `routes-doc-check.py` reads a call's body only as an object
  literal at the call, lists the walk's follow stream as one row, and ties no check to the IPC ops
  (`server_switch`, `daemon reload`) (`scripts/theme-css-check.py`, `scripts/routes-doc-check.py`).
- [debt] **A failed web check is re-run whole.** A check that fails is run again as a whole, in its
  browser; there is no way to re-run only the failed case of a check, so a late case costs the
  check's full time again (`scripts/web-checks/run-all.sh`).
- [test, with hardware] **An unnamed run takes BlueZ's first adapter, which no dongle test can steer.** With no
  `--adapter`, no configured `adapter` and no remembered one, the transport opens the first adapter
  BlueZ lists, which on a machine with a built-in controller need not be the one that hears the
  device (`bins/svolder/src/backend/direct.rs`).
- [test] **The happy path of `tui::wizard::setup` through a real terminal is untested.** The pty
  harness drives `tui::run` in a real terminal, and one case runs `svolder setup` to hover its key
  line and quit, but none walks the happy path; its refusals and the choice of front end are tested
  over a `Claim` seam, and a real `Guard` cannot be built in a unit test
  (`bins/svolder/src/tui/terminal.rs`).
- [test] **The browser checks leave some paths to node or by hand.** `popups.py` presses Tab forward
  only (Shift+Tab on a hidden control is node's), no browser check presses Tab through a real radio
  group at a popup's end (the stop and the Sound popup's tabpanel are proved in node, whose jsdom
  has no Tab of its own), and the drag-ending Escape on Ember's curve is not pressed; Ember's `0`,
  `f` and `v` keys are held by the `keys.py` spy alone; `keys.py` cannot count a handler property or
  a listener added in a state it never enters; `completeness.py` compares no counts between themes;
  the Devices popup's `reach()` on a pane the wire puts, the timed re-read, "not connected since
  HH:MM", a rename and a drop are node only (the fake cannot bring a dropped device back without a
  restart); `eq_library.py` proves a fit gap only where the fixture's measurement has one, so a
  preview with no gap or no boost is node's; the walk's wrong passphrase and a scan that takes time
  need a fake knob (`svolder-sim --advertised-passphrase` has it for the radio); whether Escape
  reaches the stack while a native `<select>` list is open has not been tried in a browser; a wheel
  over the veil in Firefox and a list placed beside classic scrollbars have not been looked at by
  hand (`scripts/web-checks/popups.py`, `scripts/web-checks/keys.py`,
  `scripts/web-checks/completeness.py`).
- [observed] **Under heavy load a few pty harness cases time out now and then.** With 16 busy loops
  running, cases such as cover-under-box, the MQTT rows and the divider failed once in a while and
  passed on a rerun; on a normal load they are green (`bins/svolder/tests/tui_pty/cases.rs`).
- [test] **Some harness upkeep is by hand.** `mutate_computer.sh` is not in `SHELL_RUNNERS` of
  `snippets.py`, so its 140 snippets rot unseen; `ember-parts.json`, the fixture the node check
  compares against, carries a key card cut out by a regular expression, and nothing regenerates it
  from the server; `keys.py` adds a hop `HD 650` to its own daemon and removes it, and a check that
  dies between leaves it there (a temporary home, so nothing outlives the run); `make network-check`
  asks about the host target only (`scripts/web-checks/snippets.py`,
  `scripts/web-checks/node/fixtures/ember-parts.json`).
- [test] **The preamp lab has no "choose a set mid-drag" event.** Its world has no store or
  convolver model; the speakers take no part in the preamp, which
  `the_speakers_take_no_part_of_the_preamp` pins (`crates/svolder-host/src/eq/lab.rs`,
  `crates/svolder-host/src/eq/control/tests/vs.rs`).
- [test] **The catalogue's sources are checked only by hand.** `make speakers-catalogue-check` needs
  the network and is not in `make check`; it also shows any storage host a redirect leads to that
  the catalogue does not list.
- [test] **The contrast sweep measures a failing config but not an unknown chip.** No fake knob
  produces `config ?`, and `contrast-baseline.json` may need a refresh for the `config/*` states
  after a full browser run (`scripts/web-checks/sweep_states.py`, `visit_config`).
- [test] **`tui_pty` has no case for a failing config through the demo daemon.**
  `SVOLDER_FAKE_CONFIG` does not reach the demo's daemon; the failing box is covered by unit tests
  (`bins/svolder/tests`).
- [test] **`doctor`'s PipeWire config row has no test of its wiring.** `config_row` and `route_on`
  are tested; the call in `outcomes_over` that feeds the row from the probed daemon is not
  (`bins/svolder/src/commands/doctor.rs`).
