# pw-dump fixtures

What `route` (`crates/svolder-host/src/route.rs`) reads, as `pw-dump` printed it (PipeWire 1.6.2,
WirePlumber 0.5.13). One object per line. Captured read-only on the development machine on
2026-10-26; user name, host name and machine id are replaced (`user`, `host`, zeros) and the title
of a browser tab is replaced by `A video - YouTube`. Objects a route never reads (modules,
factories, video and microphone nodes, other clients and devices) are left out; each object that
stays is as `pw-dump` printed it, apart from the replacements above. On this machine the default
sink is an HDMI output (`LG TV`), not a USB DAC.

| File | Origin |
|---|---|
| `one-stream.json` | Real. A silent stream (`pw-cat --playback --raw --volume 0 --rate 44100`, zero samples, 7 s) plays into the default HDMI sink; an idle Firefox stream (corked) and an idle speech-dispatcher stream are linked to it too. The USB sink (id 34, `USB Audio Speakers`) is in the graph, suspended. |
| `two-streams.json` | Real. As `one-stream.json` with a second silent stream, 48 kHz and named `Fixture Other` (`-P '{ application.name = "Fixture Other" }'`), so two streams run at once. |
| `sink-switched.json` | Synthesised from `one-stream.json`: the `default.audio.sink` metadata names the USB sink (id 34), the three streams' links are moved to its input ports, its state is `running` with a `Format` of S32LE at 48 kHz added, and the HDMI sink is `suspended` with no `Format`. What a default-sink switch leaves in the graph. |
| `schiit-sink.json` | Synthesised from `sink-switched.json`: the USB sink and its device carry a Schiit identity instead (vendor `0x30be`, product `Gungnir 2`, node name `alsa_output.usb-Schiit_Audio_Gungnir_2-00.analog-stereo`, description `Schiit Gungnir 2 Analog Stereo`). A matched DAC. |
| `stream-elsewhere.json` | Synthesised from `one-stream.json`: the silent stream's two links are moved to the USB sink's (id 34) input ports and that sink is `running` with a `Format` of S32LE at 48 kHz added; the default sink stays the HDMI one. A player that plays to another sink. |
| `host-eq.json` | Synthesised from `one-stream.json`: the computer EQ's filter chain added (an `Audio/Sink` node `svolder_eq` at volume 0.125 linear, a `Stream/Output/Audio` node `svolder_eq.output`, their ports), the silent stream's links moved to the filter's input ports and two links from its output to the HDMI sink. The default sink stays the HDMI one (the smart route). The nodes are copies of real ones with their properties edited. |
| `live-chain.json` | Real, from a private PipeWire instance (a null sink, no sound card): the live chain of `render_live_conf` in front of the null sink `svolder_e2e_sink` (smart route), a silent `pw-cat` stream (volume 0) into it so the sink runs at 48 kHz, and `PwCliSession` having set the preamp to 0.5 and a +6 dB peak at 1 kHz, Q 1 in slot 07 through the real `pw-cli`. The node's other slots read back as the config's flat point. User, host and paths are replaced. |
| `live-chain-crossfeed.json` | Real, from a private PipeWire instance (`make host-e2e` with `HOST_E2E_PWDUMP_DIR`): the chain `svolderd` builds with the EQ on and crossfeed off, so the crossfeed stage's ports (`ldir:b0` … `rmix:Gain 2`) read back at their bypass values, in front of the null sink `svolder_e2e_sink`, a left-only tone stream playing into it. User, host and paths are replaced. |
| `live-chain-loudness.json` | Real, from a private PipeWire instance (`make host-e2e` with `HOST_E2E_PWDUMP_DIR`, same capture point as `live-chain-crossfeed.json`): the chain `svolderd` builds with the EQ on and crossfeed off, now with the loudness stage's ports (`lld1:b0` … `rld2:a2`, the identity) beside the crossfeed's. User, host and paths are replaced. |
| `narrow-sink.json` | Synthesised from `schiit-sink.json`: the USB sink's negotiated `Format` is S16LE instead of S32LE. A sink running narrower than 24 bits. |
| `plug-sink.json` | Synthesised from `schiit-sink.json`: the USB sink's `api.alsa.path` and its follower's `device` read `plughw:1`. A sink ALSA converts for. |
| `rate-enum-sink.json` | Synthesised from `schiit-sink.json`: the USB sink's `EnumFormat` rate is the choice `{"default": 48000, "alt1": 44100, "alt2": 48000, "alt3": 96000}` instead of a range. A DAC that offers discrete rates. |
| `mono-sink.json` | Synthesised from `schiit-sink.json`: the USB sink's negotiated `Format` has 1 channel, `["MONO"]`. |
| `setup-pipe.json` | Real, from a private PipeWire instance (`make host-e2e-bits`'s rig, no sound card): a null sink (`svolder_e2e_sink`, the default), pipe-tunnel sinks `bits_s16`, `bits_s24`, `bits_s32`, `bits_s24_44`, `bits_s16_tri` (S16LE, running, `dither.method = triangular`), `bits_s16_noise` and `bits_s16_graph` (an `audioconvert.filter-graph.0` builtin mixer), a `pw-cat` stream `bits_player` (S16 with `dither.noise = 2`) playing into `bits_s16_tri`, and the `settings` metadata with allowed rates `[ 44100 48000 96000 ]` and force-rate 96000. User, host and paths are replaced. |
| `host-eq-default.json` | `host-eq.json` with the `default.audio.sink` metadata naming `svolder_eq` (the default route). |
| `level-muted-hard.json` | Synthesised from `host-eq.json`: the filter node's hard `mute` is true. |
| `level-muted-soft.json` | Synthesised from `host-eq.json`: the HDMI sink's `softMute` is true. |
| `level-unequal.json` | Synthesised from `host-eq.json`: the filter node's two channels differ (`channelVolumes` 0.125 and 0.064): the lowest counts. |
| `level-no-volumes.json` | Synthesised from `host-eq.json`: the sink and the filter node list no `channelVolumes`. |
| `level-default-muted.json` | Synthesised from `host-eq-default.json`: the filter node's hard `mute` is true (the default route). |

The host EQ was not applied when the fixtures were captured (`host-eq.toml` says `on = false`), so
the filter chain files are synthesised. Replace them with real captures when one is at hand.

In the real files a USB sink's identity is on its device object, not on the sink node: the node has
`device.bus = "usb"` and `device.id`, and the `Device` with that id has `device.vendor.id`
(`0x0414`), `device.vendor.name`, `device.product.id` and `device.product.name`; the node also has
`api.alsa.card.name`.

## asound/

The `asound` folder beside this one holds the kernel's files for the card of a USB DAC (card 3,
as `/proc/asound` printed them, read-only commands) in three trees:

- `idle/`: `stream0` (the whole file, real, captured while the card was stopped) and
  `pcm0p/sub0/hw_params` reading `closed`. Real.
- `playing/`: `pcm0p/sub0/hw_params` of the device open at S32_LE, 2 channels, 44100 Hz (real);
  its `stream0` is a copy of the idle capture, not a capture taken while playing.
- `second/`: derived, for the readers' device and substream numbers: `stream1` is a copy of the
  real `stream0`, and `pcm1p/sub1/hw_params` is the real playing file with format S24_3LE and rate
  48000 edited in.

`SVOLDER_ASOUND_DIR` points the readers at such a tree.
