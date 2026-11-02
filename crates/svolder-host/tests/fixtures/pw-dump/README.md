# pw-dump fixtures

What `route` (`crates/svolder-host/src/route.rs`) reads, as `pw-dump` printed it (PipeWire 1.6.2,
WirePlumber 1.6.2). One object per line. Captured read-only on the development machine on
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
| `host-eq-default.json` | `host-eq.json` with the `default.audio.sink` metadata naming `svolder_eq` (the default route). |

The host EQ was not applied when the fixtures were captured (`host-eq.toml` says `on = false`), so
the filter chain files are synthesised. Replace them with real captures when one is at hand.

In the real files a USB sink's identity is on its device object, not on the sink node: the node has
`device.bus = "usb"` and `device.id`, and the `Device` with that id has `device.vendor.id`
(`0x0414`), `device.vendor.name`, `device.product.id` and `device.product.name`; the node also has
`api.alsa.card.name`.
