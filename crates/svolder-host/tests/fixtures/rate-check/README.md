# rate-check fixtures

The text `scripts/rate-check.sh --self-test` runs its parsers on (the self-test is part of
`make check`). The `pw-dump` JSON it also reads is in the folder beside this one.

| File | Origin |
|---|---|
| `wpctl-inspect.txt` | Real. `wpctl inspect @DEFAULT_AUDIO_SINK@` (WirePlumber 1.6.2), captured read-only on the development machine on 2026-10-26; the default sink is an HDMI output. |
| `pw-metadata.txt` | Real. `pw-metadata -n settings 0 clock.allowed-rates` (PipeWire 1.6.2) with six allowed rates. |
| `pw-top-idle.txt` | Real. `pw-top -b -n 2` with nothing playing: every driver idle, rate 0 (the decimal comma is the machine's locale). |
| `pw-top-running.txt` | Synthesised from the last pass of `pw-top-idle.txt`: three passes, and in the third the HDMI sink's row is edited to a running one (state `R`, quantum 1024, rate 44100, a format) as `pw-top` prints a driver that has a stream. The running row was not captured from a session in use. |
