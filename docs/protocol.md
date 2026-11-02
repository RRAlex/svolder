# Schiit Forkbeard BLE protocol

The wire protocol Schiit's Forkbeard devices speak, as learned from Schiit's Forkbeard Android app,
version 1.11 (see `LEGAL.md`). svolder's profiles and codec follow it; notes on where a profile
differs from the app on purpose are marked "svolder".

## 1. Discovery (BLE advertising)

Scan filter: service UUID `2DA00001-9F4C-4A75-BDEC-FCC5C2877BBF`.

Manufacturer-specific data payload is exactly **27 bytes** (after the 2-byte company ID,
which the protocol gives no meaning). All multi-byte fields little-endian.

| Offset | Size | Field |
|--------|------|-------|
| 0      | 2    | Product ID (u16) |
| 2      | 3    | unused / not parsed |
| 5      | 1    | bit0 = incomplete DFU flag |
| 6      | 8    | Device UDID (8 bytes, unique per unit; also read as u64 LE) |
| 14     | 4    | Encrypted public passphrase (u32) |
| 18     | 2    | Device firmware build (u16). **0 means the unit is in DFU/bootloader mode** |
| 20     | 1    | Forkbeard module hardware version |
| 21     | 1    | Forkbeard module firmware build |
| 22     | 5    | unused / not parsed |

A device resumes connectable advertising, with the encoded passcode in the payload as before,
after a link to it closes. A central forgets a device's advertisement when it connects to it and
finds it again only from a fresh one, so a device that stayed silent would be gone from its list.

### Product IDs

| ID | Device | Notes |
|----|--------|-------|
| 1  | Kara | preamp |
| 2  | Gungnir 2 | DAC. Firmware build >= 2 enables EQ/balance/volume ("v2"); the only firmware gate |
| 3  | Jord | not released |
| 4  | Wotan | power amp |
| 5  | Vidar 2 | power amp |
| 6  | Aegir 2 | power amp |
| 7  | Kraken | 5-channel power amp |
| 8  | Yggdrasil | DAC |
| 9  | Urd | not released |
| 10 | Bifrost 3 | DAC |
| 11 | Mimir | DAC |
| 12 | Saga 2 | preamp |
| 13 | Stjarna | phono preamp |
| 14 | Lyr 5 | headphone amp / preamp |
| 15 | Skoll | phono preamp |
| 16 | Forkbeard Module | bare module, Nordic firmware |
| 17 | Gjallarhorn | power amp |
| 18 | Tyr | mono power amp |
| 19 | Freya | preamp (the three-mode Freya) |
| 20 | Mesh Card | DAC card |
| 21 | Eitr 2 | DAC |
| 22 | Mjolnir X | not released |
| 23 | Loki Max | EQ |
| 24 | Freya 2 | preamp, released. The app names it "Freya"; svolder calls it "Freya 2" so the two Freyas can be told apart |
| 25, 26 | - | not mapped |
| 27 | Hati | phono preamp |

## 2. GATT

| Role | UUID | Properties used |
|------|------|-----------------|
| Service | `2DA00001-9F4C-4A75-BDEC-FCC5C2877BBF` | |
| TX (host -> device) | `2DA00002-9F4C-4A75-BDEC-FCC5C2877BBF` | write without response |
| RX (device -> host) | `2DA00003-9F4C-4A75-BDEC-FCC5C2877BBF` | notify (or indicate if the char advertises it). Enable via CCCD `0x2902` |

Connect sequence, as the app does it:
1. Connect over LE (no auto-connect).
2. Request high connection priority and an MTU of 512, discover services, find TX and RX, and
   enable notifications on RX. The device is ready once both characteristics are known.
3. Send **function 1** (status refresh). The device answers with one Status message per function,
   in one flood. The other times a client sends function `1` are listed under "Power state and
   the refresh on power-on" (section 4).
4. On devices with subsystems, query firmware versions with `210.2.<subsystem>`.

Nothing else is sent on connect.

The app asks one subsystem at a time, lowest number first: once the device is ready it decides to
ask, and 300 ms later sends the first `210.2.<sub>` (Command, Data, empty); it asks for the next
subsystem still unknown only once the previous one's report has arrived, again 300 ms after. It
neither retries nor times out, so a report that never comes ends the chain. svolder asks the same
way over the subsystems its profile lists (`firmware_subsystems`, lowest first): the first
`210.2.<sub>` goes out 300 ms after its refresh burst is read, each next one 300 ms after the
previous one's report, and a report that never comes leaves the later subsystems unasked on that
link; each report arrives as an ordinary Status. A device that refused the passphrase is asked
nothing; once a corrected code has been proven by its refresh, the chain begins. No other
`210` function is ever sent.

Writes:
- One write is in flight at a time: the app waits for each write to complete before the next. The
  wait ends when the write completes, the link drops, the message is discarded or an error occurs.
- A message sent before the device's stored record is loaded is held and sent 50 ms later.
- If the link is not connected, the app retries up to 3 times, 50 ms apart. If the write call
  itself fails, it retries up to 3 times after 100, 200 and 400 ms, then discards the message.
- Write-without-response is preferred; the TX characteristic must advertise write or
  write-without-response.
- On BlueZ, a write-without-response returns immediately, so a small delay between writes (tens
  of ms) is a reasonable substitute for waiting.

Pacing. The protocol has no rate limit, and the app adds none:
- Every write waits only for the one before it. A newer value never replaces an older one still
  waiting, nothing is merged and nothing is dropped for being old, so a gesture that crosses 40
  steps sends 40 writes, in order. There is no watchdog: a write whose completion never comes
  holds the ones behind it until the link drops. A retried write goes to the back of the line.
- A continuous control (the EQ graph's points, the loudness strength, a balance slider, a preamp
  dial) sends one Command each time its own raw value changes during the gesture, carrying that one
  value only (a frequency drag writes `E.b.2` alone, never the band's three values), and sends
  nothing when the finger lifts: the last value sent is the final one. A value equal to the last
  one sent is not sent again.
- A DAC's volume is stepped by buttons, one step per tap from the volume last reported. A held
  button repeats only when the device reports the volume, so the device's own answers pace it.
- The app's displayed values move only when the device reports them, so it relies on a Status for
  every accepted write (inferred: nothing else moves them).
- No periodic traffic: no keep-alive, no periodic function `1`, no signal-strength read of a
  connected device. Function `1` goes out only on the triggers in section 4; the only timed sends
  are one-off delays (the firmware chain above, a tone-control page's second function `1` after
  250 ms).
- The app connects 100 ms after deciding to (500 ms on a retry), and after a failed attempt waits
  1, 2, 4, 8, then 10 s, counted from the previous attempt's start, beginning a new series after
  the fifth.

Open, for svolder to try on a real device: whether a device reports a value again when a write
leaves it unchanged (if it does, the app's mode write-back of section 4 would repeat), and how
fast a burst of writes it takes without dropping one. The app's band sliders run 0 to 100, so it never writes a band value above 100 from them;
its graph can.

A device the app has flagged for a wrong passphrase (section 5) gets no messages at all except
function `200` (identify) and, while a passphrase change is pending, `250`.

## 3. Frame format

### Host -> device (one message per write)

```
[passphrase u32 LE][len u16 LE][header u8][fid1][fid2?][fid3?][value bytes...][checksum u16 LE]
```

- `len` = 1 (header) + number of fids + number of value bytes. It does not count itself,
  the passphrase, or the checksum.
- `checksum` is computed over `[len][header][fids][value]`:
  `sum = (Σ bytes) mod 65536; checksum = ~sum & 0xFFFF`.
- `passphrase` field: `0` if the device has no passphrase set. Otherwise
  `encrypt(encoded_passphrase, key)` where `key` is the u32 LE formed from the **last 4
  bytes of the frame body** (the bytes right before where the checksum ends, i.e. the
  last two message bytes plus the two checksum bytes). See section 5.

### Device -> host (one notification)

```
[len u16 LE][header][fids][value] ... (repeated, possibly several messages) ... [checksum u16 LE]
```

No passphrase prefix. Checksum is over everything before it, same algorithm. Parse
messages in a loop until fewer than 2 bytes remain.

### Header byte

| Bits | Meaning |
|------|---------|
| 7    | 0 = Command (host -> device), 1 = Status (device -> host) |
| 6-5  | Value type: `00` Data (raw bytes), `01` Boolean, `10` Integer |
| 4    | Boolean: the value itself. Integer: 1 = signed, 0 = unsigned |
| 3-2  | Integer size: `00` 8-bit, `01` 16-bit, `10` 32-bit, `11` 64-bit |
| 1-0  | Number of function-ID bytes that follow (1-3) |

Integers are little-endian. Booleans carry no value bytes (the bit in the header is the
value). Data type carries whatever follows the fids.

Useful header values:

| Header | Meaning |
|--------|---------|
| `0x01` | Command, Data type, empty payload, 1 fid (used for "refresh" and "identify") |
| `0x21` | Command, Boolean false, 1 fid |
| `0x31` | Command, Boolean true, 1 fid |
| `0x41` | Command, UInt8, 1 fid |
| `0x42` | Command, UInt8, 2 fids |
| `0x43` | Command, UInt8, 3 fids |
| `0x49` | Command, UInt32, 1 fid |

Worked example, no passphrase, Freya "select input 3" (function 2, UInt8 3):

```
body      = 03 00  41 02 03        ; len=3 (header + 1 fid + 1 value byte), header=UInt8/1fid, fid=2, value=3
checksum  = ~(0x03+0x00+0x41+0x02+0x03) & 0xFFFF = ~0x0049 = 0xFFB6  -> B6 FF
frame     = 00 00 00 00  03 00  41 02 03  B6 FF
```

> **Parsing leniency.** Inside a notification whose checksum verified,
> a message that fails to parse (wrong payload size for its type) is skipped and parsing continues
> with the next message. A declared length of zero or one that runs past the available bytes stops
> parsing; messages already parsed are kept. A single leftover byte before the checksum is ignored.
> Fixed-size types must have exactly their size in payload bytes; `Data` takes whatever remains;
> `Bool` has none. Unlike the rest of this leniency, svolder rejects a message with zero
> function-id bytes.

## 4. Function IDs

`fid1` selects the function; `fid2`/`fid3` are sub-addresses. The app builds
Command messages with these IDs and matches Status replies on the same IDs.

### Common to all devices

| IDs | Type | Meaning |
|-----|------|---------|
| `1` | Data, empty | Refresh: device replies with Status for every function it has |
| `200` | Data, empty | Identify yourself (visual cue on the unit) |
| `201` | Boolean | Status only. Stjarna and Hati: remote locked, a best guess, since the app shows nothing for it. Every other device ignores it |
| `210.1` | Data, empty | Enter DFU (firmware update). svolder never sends it: its codec refuses the firmware-update family, all but the version query `210.2` |
| `210.2.<sub>` | UInt16 | Request (Command, Data, empty) / report (Status, UInt16) firmware version for subsystem `<sub>` (Yggdrasil, Gungnir, Bifrost 3: 1 = Main PIC, 2 = DSP; Mimir, Eitr, Mesh Card: 1) |
| `210.3.<sub>` | Data | DFU chunks, pulled by the device during a firmware update. svolder never sends them |
| `230.1` | Boolean | Fault: overcurrent (Freya 2: input overcurrent). The app shows no `230.x` fault on a DAC. Some amps use a third fid per channel, see Power amps |
| `230.2` | Boolean | Fault: over-temperature (amps), headphone fault (Kara/Saga/Lyr), output overcurrent (Freya 2) |
| `250` | UInt32 | Set passphrase (encoded, see section 5). Device sends Status `250.255` when a command's passphrase was wrong; see section 5 for what the app does with it |

Firmware updates are the app's job: svolder only reads the firmware versions (`210.2`) and stays
away from a device whose advertisement says it is in its bootloader (section 1).

### Preamps

| Device | IDs | Type | Meaning / values |
|--------|-----|------|------------------|
| Freya (19) | `2` | UInt8 | Input 1-5 (1-2 balanced XLR, 3-5 single-ended) |
| | `3` | UInt8 | Mode 1-3: Passive, Buffer, Tube Gain (mode 3 is the only one with gain, 3.8x) |
| | `4` | Boolean | Mute |
| | `6` | UInt8 | Volume 0-127 (128-step relay attenuator, -80 dB..0 dB) |
| Freya 2 (24) | `2` | UInt8 | Input 1-5 (1-2 balanced XLR, 3-5 single-ended) |
| | `3.1` | UInt8 | Mode, **0-based on the wire** (the app shows wire + 1): 0 Passive, 1 solid-state voltage gain & solid-state current gain, 2 tube voltage gain & solid-state current gain, 3 solid-state voltage gain & tube current gain, 4 tube voltage gain & tube current gain |
| | `3.2` | UInt8 | Gain: 1 Low (1x), 2 High (4x). The app refuses it in Passive ("Gain not available in Passive") |
| | `4.1` | Boolean | Mute |
| | `6.1` | UInt8 | Volume 0-127 (the same 128-step -80 dB..0 dB table as Freya and Kara) |
| | `6.2` | UInt8 | Balance, analog, 0-200, 100 = centred |
| | `10` | Boolean | Power: true = awake (on), false = standby. svolder keys it `power` |
| | `230.1` / `230.2` | Boolean | Status: input / output overcurrent |
| Kara (1) | `2` | UInt8 | Input 1-5 |
| | `3` | UInt8 | Mode 1-3: Passive, Low Gain, High Gain (mode 3 = 4x gain) |
| | `4` | Boolean | Mute |
| | `5` | Boolean | Headphone output (status only) |
| | `6` | UInt8 | Volume 0-127 |
| Saga 2 (12) | `2` | UInt8 | Input 1-4 |
| | `3` | UInt8 | Mode 1-3: Passive, Low Gain, High Gain |
| | `4` | Boolean | Mute |
| | `5` | Boolean | Headphone output (status only) |
| | `6` | UInt8 | Volume 0-63 (64 steps, -64 dB..0 dB) |
| | `10` | Boolean | Status only: power, true = on, false = off (the app greys the UI out when false) |
| Lyr 5 (14) | `2` | UInt8 | Input 1-2: RCA 1, RCA 2 |
| | `3` | UInt8 | Gain 1-2: Low Gain, High Gain. The app's display factors are 1.0x and 7.0x |
| | `4.1` | Boolean | Headphone mute, true = muted |
| | `4.2` | Boolean | Pre-out, **true = enabled** (names 0 Pre-Out Disabled, 1 Pre-Out Enabled) |
| | `6` | UInt8 | Volume 0-63 |
| | `7` | UInt8 | Mode 1-2: **1 Solid State, 2 Tube** |
| | `22` | UInt8 | Status only, the class icon: 1 class A, 2 class AB, 3 warning |

Freya 2 has no subsystems and is not OTA-capable.

### DACs

Yggdrasil (8), Gungnir 2 with firmware >= 2 (2), Bifrost 3 (10), Mimir (11), Mesh Card (20),
Eitr 2 (21).

| IDs | Type | Meaning |
|-----|------|---------|
| `2` | UInt8 | Input. Gungnir 1-5: USB 1, USB 2, Optical, Coaxial, AES. Yggdrasil 1-6: USB 1, USB 2, Optical, Coaxial, BNC, AES. Bifrost 3 1-3: USB, Optical, Coaxial. Mimir 1-4: USB 1, Optical, Coaxial, AES. Mesh Card and Eitr have no input control |
| `3` | Boolean | Phase invert |
| `4` | Boolean | Filter (NOS on/off). Not on Eitr |
| `5` | Boolean | Mute |
| `6` | UInt8 | **Yggdrasil/Gungnir/Bifrost 3:** sample-rate status (read-only). **Mimir/Mesh Card/Eitr:** volume 0-127 |
| `7.1`, `7.2` | Boolean | **Yggdrasil/Gungnir:** status, USB 1 / USB 2 connected |
| `7` | Boolean | **Bifrost 3:** status, USB connected (one port) |
| `8` | Boolean / UInt8 | Status, S/PDIF locked. **Yggdrasil/Gungnir:** Boolean. **Bifrost 3:** UInt8, locked when the value is 1 (svolder reads the integer as a boolean the same way) |
| `9` | UInt8 | **Mimir/Mesh Card/Eitr:** sample-rate status |
| `10` | UInt8 | **Yggdrasil/Gungnir/Bifrost 3:** volume 0-127 (127 = 0 dB, 0 = mute). **Mimir, Mesh Card, Eitr:** status, USB connected (Boolean) |
| `11` | UInt8 / Boolean | **Yggdrasil/Gungnir/Bifrost 3:** balance 1-127, 64 = centred (every DAC's balance runs 1-127). **Mimir:** status, S/PDIF locked (Boolean) |
| `8` (Mimir/Mesh Card/Eitr) | UInt8 | Balance 1-127, 64 = centred |
| `7` (Mimir/Mesh Card/Eitr) or `12` (Ygg/Gungnir/Bifrost 3) | | EQ block, see below |

Sample-rate values: 0 none, 1 = 44.1, 2 = 48, 3 = 88.2, 4 = 96, 5 = 176.4, 6 = 192,
7 = 352.8, 8 = 384 kHz.

EQ block (`E` = 7 or 12 as above, `b` = band number starting at 1). The app writes and reads the
three band values in this order:

| IDs | Type | Meaning |
|-----|------|---------|
| `E.b.1` | UInt8 | Band **Q**, 0-127 (0 = 0.5, 64 = 4.3, 127 = 8.0) |
| `E.b.2` | UInt8 | Band **frequency**, 0-127 (log scale around a per-band centre) |
| `E.b.3` | UInt8 | Band **gain**, 0-127 (0 = -20 dB, 98 = 0 dB, 127 = +6 dB) |
| `E.50` | UInt8 | Tone-control mode: 1 flat/bypass, 2 EQ, 3 loudness |
| `E.52` | UInt8 | Loudness strength 0-127 |

The app sends Q on sub-ID 1, frequency on 2 and gain on 3 for every EQ device, and reads Status
replies the same way.

Bands: Yggdrasil/Gungnir/Bifrost 3 have 5 bands (centres 40, 160, 632, 2515, 10000 Hz in the conversion; 2520 is the band's UI default);
Mimir/Mesh Card/Eitr have 3 bands (63, 630, 6300 Hz). The Bifrost 3 converts as the Yggdrasil
does: the same bands, frequency windows and scales, and gain and Q ranges.

The app sends the mode (`E.50`) back as a Command whenever a mode Status arrives, with no
comparison against the mode it shows, sometimes twice. That is a UI side effect, not a protocol
rule: a Command equal to the current mode changes nothing, and svolder does not copy the echo. A
device reports the mode when it changes and stays silent on a Command equal to the mode it is in,
and the app depends on that: a device that answered every Command of the mode with a Status would
make it write the mode forever. Whether a real unit is silent on a repeat is unverified;
svolder's fake device and simulator are, for the mode.

Bifrost 3 extras: subsystems 1 Main PIC and 2 DSP, output levels
2.0 Vrms single-ended and 4.0 Vrms balanced, and a 128-step attenuation table (-100..0 dB).

### Power amps

Aegir 2 (6), Vidar 2 (5), Gjallarhorn (17), Kraken (7), Tyr (18), Wotan (4).

| IDs | Type | Meaning |
|-----|------|---------|
| `4` | Boolean | Mute. The app sends it only on Gjallarhorn; on Aegir 2, Vidar 2, Kraken, Tyr and Wotan it is a status lamp, and with `10` true and `4` true the amp is **warming up** (svolder reads that as a "Warming up" readout) |
| `10` | Boolean | Power: **true = awake (on), false = standby** (names 0 Standby, 1 On). Not on Gjallarhorn. svolder keys it `power` |
| `20.1`, `20.2` | UInt16 | Power-meter reading, left and right (Aegir 2, Vidar 2, Gjallarhorn, Wotan; status only, streamed; see "Meter scale" below) |
| `20.1`..`20.5` | UInt16 | Kraken: power-meter reading per channel (status only, streamed) |
| `20.1` / `20.2` | UInt16 | Tyr: `20.1` meter reading, `20.2` floor reading (status only). The floor sets the meter's zero |
| `20.3` / `20.4` | UInt16 | Wotan: meter floor, right and left (status only). Each sets its channel's zero |
| `21.1` | Boolean | Temperature reporting. Whenever an amp reports `21.1` true, the app answers with Command `21.1` false, on every amp; svolder does the same |
| `21.2` | Boolean | Power meters on/off. While it reads off, or the amp is in standby or warming up, the amp's meters are off |
| `2` | UInt8 | Wotan only: input 1-2 |
| `3` | Boolean | Wotan only: feedback on/off |
| `230.1` / `230.2` | Boolean | Over-current / over-temperature. Aegir 2, Vidar 2, Kraken: two fids. **Wotan:** `230.1.<1,2>` and `230.2.<1,2>` per channel. **Gjallarhorn:** `230.1.<1,2>` per channel, `230.2` plain. **Tyr:** `230.1` (whether with a third fid is unverified; svolder reads the plain form) and `230.2.<channel>` (svolder reads channel 1) |

#### Meter scale

An amp's meter reading `r` (0 to 65535) is placed on the amp's own logarithmic scale. With a zero
`z` and a full scale `f`, the fraction `p` of the bar's live part is

```
p = clamp(log10(max(0, r - z) + 1) / log10(f - z + 1), 0, 1)
```

so `p` is 0 at or below the zero and 1 at or above full scale. The bar has two parts: a class-A
block over its first fraction (a fixed fraction of the bar's length, not a reading), and the live
part over the rest, filled to `p`. The zero is fixed on the Aegir 2, Vidar 2, Gjallarhorn and
Kraken. On the Tyr and the Wotan it follows the floor reading `m` the amp sends: `z = m + max(10, ⌊m / 2⌋)`, the floor plus half again and at least 10 above it, and `z = 0` until a floor has
arrived. A floor whose zero reaches the full scale leaves the bar empty. The Wotan's two channels
have separate floors and separate full scales. Each amp's zero or floor rule, full scale and
class-A fraction are the constants in its profile (`profiles/`); none of them is confirmed on a
real amp.

### Phono preamps

Skoll (15), Stjarna (13), Hati (27).

| IDs | Type | Meaning |
|-----|------|---------|
| `2` | UInt8 | Input 1-2: Skoll Balanced, Single-Ended; Stjarna and Hati RCA 1, RCA 2 |
| `3` | UInt8 | Gain 1-4 (Skoll and Hati 40/50/60/70 dB; Stjarna 40/47/55/60 dB) |
| `4` | Boolean | Mute |
| `5` | Boolean | Headphone (Skoll) |
| `7.1` | Boolean | Mono mode (Stjarna, Hati) |
| `7.2` | Boolean | Subsonic filter |
| `10` | Boolean | Power, true = awake, false = standby (Stjarna, Hati). svolder keys it `power` |
| `11.1` | UInt8 | Load capacitance. Skoll and Stjarna offer 1-4 = 50, 100, 150, 200 pF (Stjarna's name table runs to 8 = 400 pF). Hati 1-8: 50 … 400 pF in 50 pF steps |
| `11.2` | UInt8 | Load resistance. Skoll 1-5: 10, 50, 100, 150 Ω, 47 kΩ. Stjarna offers 1-5: 10, 50, 100, 150, 200 Ω (its table runs to 9 = 47 kΩ). Hati 1-17: 10, 50, 100, 150, 400, 450, 500, 550, 1000, 1050, 1100, 1150, 1400, 1450, 1500, 1550 Ω, 47 kΩ |
| `201` | Boolean | Status: remote locked (Stjarna, Hati; a best guess, see Common) |

### Loki Max (23)

| IDs | Type | Meaning |
|-----|------|---------|
| `2` | UInt8 | Input 1-2: XLR, RCA |
| `3` | UInt8 | Mode: 1 Process, 2 Bypass |
| `6.b.3` | UInt8 | Band gain 0-30, 15 = 0 dB, six fixed bands `b` = 1 to 6 at 20, 120, 550, 2000, 6000, 20000 Hz with ranges ±12, ±9, ±6, ±6, ±9, ±12 dB, linear over each side of 15. Q is fixed and never on the wire. The graph and the sliders write `6.b.3` |
| `6.b` | UInt8 | The same band, written by the predefined settings (two fids) |
| `7.1.<p>` | Data, empty | **Save** preset `<p>` |
| `7.2.<p>` | Data, empty | **Recall** preset `<p>` |

The app reads a band from any Status whose first id is `6`, whose second id is 1 to 6 and whose
value is a UInt8, whatever the third id: a report on `6.b`, on `6.b.3` or on `6.b.<n>` sets band
`b`. Until a band is reported the app shows it at 0 dB. Which of those forms a real unit reports
is unverified.

The device keeps three preset slots, `p` = 1 to 3, named "Preset 1" to "Preset 3" in the app. A
save and a recall are each a Command with an empty Data value on three ids (header `0x03`,
`len` = 4); the app checks `p` is 1 to 3 before it sends. The app sends nothing else around either
one, waits for nothing and does not refresh. It never changes the bands on screen itself: they
follow the device's band reports alone, so the device is presumed to push its bands after a recall
(inferred, not seen). No id reports the active slot, and the app handles no Status on `7.x`. A
save shows a "Save as Preset" list (Preset 1 to 3, Cancel), sends at once and then says "Preset
Saved"; it never asks before it overwrites. The mode (`3`) is untouched by a save or a recall.

The app offers five predefined settings. Each is six `6.b` writes (two fids), bands 1 to 6, back
to back; the raw steps, bands 1 to 6, are:

| Setting | Steps |
|---------|-------|
| Flat | 15 15 15 15 15 15 |
| Bass Boost | 28 21 17 15 15 15 |
| Treble Boost | 15 15 15 17 21 28 |
| Smiley | 23 16 8 8 16 23 |
| Loudness | 30 22 18 15 23 28 |

Not known without a unit: which form it reports a band on; whether it reports its bands after a
recall or a predefined setting; whether `6.b` and `6.b.3` writes behave alike; whether it answers a
save; what it does with a slot outside 1 to 3.

**What svolder does.** svolder offers the three slots (recall and save) and the five settings as
the app has them, and reads a band from `6.b` and from any `6.b.<n>` as well as `6.b.3`, as the app
does. It writes a band as `6.b.3`, and a setting's bands as `6.b`. It departs from the app in
three places:

- A save asks first. The app does not, but a save overwrites the slot on the device and cannot be
  undone: the TUI and the CLI ask (`--yes` answers), the web page asks in the page and its request
  needs `"confirm": true` for a save, and the MCP server, MQTT and Home Assistant do not offer a
  save. A recall does not ask, as in the app.
- A refresh follows a recall that no band report follows. A recall and a save are written without
  waiting for anything, as the app does; but if none of the bands a recall changes is reported
  within one second of it, svolder writes function `1` once, so a unit that pushes nothing after a
  recall does not leave the screens on the bands from before it. A save is followed by nothing.
- A setting's writes go cuts first, then boosts. Every band that goes down is written before any
  goes up, and a band already at its value is not written, so no boost is heard before a cut. The
  app writes bands 1 to 6. The order changes nothing on the wire but what is heard in between.

A recall and a save have no echo, and svolder waits for none. Each band a setting writes does
wait: it is confirmed by a report on `6.b` or any `6.b.<n>`, and a setting stops at the first band
that is not, so a unit that never reports the two-id form stops one after its first write. After a
recall, svolder takes each band for unread until that band reports anew, and plans the next setting
with it written whatever it last read.

### Power state and the refresh on power-on

A client keeps a power state per device: **on**, **standby**, **off** or **warming up**. Only the
models below have one; on every other model the state is **on** for good.

The state changes only on an incoming **Status** message with a **Boolean** value, matched on the
first function id alone. Nothing else changes it: a Status with another value type on the same id,
a Command from the device, or the client's own writes. On the models with a power button (all of
the models below except Saga 2) the app sends Command `10` with the negation of the last reported
value and lets the state wait for the device's Status. Saga 2 has no power button and is never
sent `10`; its state follows Status `10` alone.

| Model | How the state is derived |
|-------|--------------------------|
| Freya 2 (24), Stjarna (13), Hati (27) | Status `10`: true = on, false = standby |
| Saga 2 (12) | Status `10`: true = on, false = **off** |
| Aegir 2 (6), Vidar 2 (5), Kraken (7), Tyr (18), Wotan (4) | The client remembers the last Status `10` and the last Status `4`, both **false** at the start. It recomputes the state on every Status `10` or `4`: `10` false = standby; `10` true and `4` true = warming up; `10` true and `4` false = on |
| Kara (1), Gungnir 2 (2, either firmware), Yggdrasil (8), Bifrost 3 (10), Mimir (11), Lyr 5 (14), Skoll (15), Gjallarhorn (17), Freya (19), Mesh Card (20), Eitr 2 (21), Loki Max (23) | No power state; always on. Gjallarhorn's `4` is mute only, and it has no `10` |
| Jord (3), Urd (9), Forkbeard Module (16), Mjolnir X (22) | Not handled by the app: no power state |

**The refresh on coming back on.** When the state goes from standby, off or warming up to **on**,
the app sends **one** function `1` (Command, Data, empty).
- **Once per transition.** A Status that leaves the state unchanged sends nothing: on followed by
  on sends nothing. Leaving on (to standby, off or warming up) sends nothing either. Warming up to
  on sends one, for example when an amp's `4` turns false while `10` stays true. Standby to
  warming up sends nothing.
- **Per Status message, in arrival order.** The messages of one notification are evaluated one at
  a time, in order, and so are the notifications. The send does not wait for the end of the flood.
- **The starting state.** A client starts following a device's state from **on**, with an amp's
  remembered `10` and `4` false. The app starts afresh like this each time it begins following the
  device again (it does so whenever it returns to the device's controls from another part of the
  app, and when its stored device list changes). A link drop and reconnect while the device is
  followed keeps the state and the remembered values, so a device that was in standby when the
  link dropped and is on after the reconnect gets this function `1` as well as the connect
  refresh.
- **The amp quirk on a fresh start.** The remembered `10` is false until the first Status `10`
  arrives. If the flood carries Status `4` before Status `10`, the first `4` computes standby, and
  the `10` true that follows computes on (or warming up if `4` was true): one **extra** function
  `1` right after the connect refresh. If `10` comes first, nothing extra is sent. A client that
  judges the state only once `10` has reported avoids the extra send.
- **While the passcode is refused** (section 5) no function `1` is sent.

**The other function-1 triggers** (all Command, Data, empty):
- **Device ready.** The app has found TX and RX and enabled notifications: one function `1`.
- **Following a device.** Each time the app starts following a device's state (see "The starting
  state") and a connection to that device is under way or up, it sends one function `1`. A
  connection counts from the moment the connect starts.
- **Passcode accepted.** After the passcode of a refused device is accepted (section 5), the app
  sends no function `1` at that moment in practice; the next one is sent when it next starts
  following the device.
- **Tone-control editors** (the DAC balance and parametric-EQ editors, the Freya 2 balance editor,
  the Loki Max EQ editor). When the app opens one and a connection is under way or up, it sends
  function `1` **twice**: at once and again 250 ms later. It does the same when the device becomes
  ready while the editor is open.
- Identify (`200`) sends no function `1`.

**svolder's identify and the app's.** svolder sends the same frame (`200`, Data, empty, header
`0x01`) with no function `1`, and sends it on a link the device has refused the passphrase on, as
the app does. It differs in how the link comes about and in pacing. The app opens a short
connection of its own for identify and none for a refused device. svolder sends it over the link
its daemon (or its own session) already holds, so a device that is down answers "not connected"
and the request waits for no connection of its own; the daemon's reconnect loop is the one
connect path. svolder also sends at most one identify per link every 2 seconds, so a held key, a
repeated button or several front ends on one daemon send one frame; a request inside the gap
says so and sends nothing. What the unit does on receiving it (one blink, a timed cue, whether a
repeat extends it) has not been seen on a real unit.

**Delivery of a function `1`.** The same rules apply to every one of these sends:
- No connection under way or up for the device: the message is dropped.
- A connection under way but the link not up: the message is held and goes out once the link is
  ready (checked every 50 ms). A disconnect discards held messages.
- The link up but the device not yet ready (TX not found): the message is discarded, with no
  retry.
- The link gone: the message is discarded, after the retries in section 2.
- The device's passcode is refused: the message is discarded.
- Reads and writes do not depend on the power state.

svolder sends function `1` when a device goes from standby, off or warming up to on, and after a
refused passcode is corrected; see section 6.

## 5. Passphrase scheme

A device can have a 4-digit passphrase. It is **not** a security mechanism: the current
passphrase is recoverable from the advertisement by anyone who knows the transform below.
The app does exactly this to check whether its stored passphrase is still correct.

When the device sends Status `250.255` (a command's passphrase was wrong), the app compares the
passphrase it has stored for the device with the one decoded from the advertisement. If the
advertised one is 0 or differs, it flags the device as having a wrong passphrase and disconnects.
From then on it sends that device nothing but `200` and `250` (section 2). If the two match, the
`250.255` is dropped. The full behaviour is under "Refusal: Status `250.255`" and "Entering a
passcode" at the end of this section.

### Encoding 4 digits into a u32

```
d0..d3 are the digits (0-9)
v = (d0 & 15)
  | (d1 & 15) << 8
  | (d2 & 15) << 12
  | (d3 & 15) << 4
  | (~d2 & 15) << 16
  | (~d1 & 15) << 20
  | (~d3 & 15) << 24
  | (~d0 & 15) << 28
encoded = rotl32(v, 2)
```

Decode: `v = rotr32(encoded, 2); d0 = v & 15; d1 = (v >> 8) & 15; d2 = (v >> 12) & 15; d3 = (v >> 4) & 15`.

### Encrypt / decrypt with a key

```
encrypt(p, key): x = p ^ key;  return ~((x >> 16) | (x << 16))      (all u32)
decrypt(e, key): x = ~e;        return ((x >> 16) | (x << 16)) ^ key
```

### Key for the advertised passphrase (offset 14 in manufacturer data)

```
u = UDID as u64 LE
key = (byte(u >> 32) << 16) | (byte(u >> 16) << 24) | (byte(u) << 8) | byte(u >> 56)
passphrase_encoded = decrypt(advertised_u32, key)     ; 0 means no passphrase
```

### Key for outgoing frames

`key` = u32 LE of the last 4 bytes of `[len][header][fids][value][checksum]`, i.e. the
final two message bytes and the two checksum bytes. Each frame therefore carries a
differently-scrambled copy of the same passphrase.

### Setting a passphrase

Send Command `250`, UInt32, value = `encoded`. Send `0` to clear.

### Refusal: Status `250.255`

What the client keeps per device (UDID):
- **The stored passcode**, in encoded form. A connection reads its own copy once, when its link
  comes up, and does not refresh it while the link stays up.
- **Whether the device was refused**, which the app forgets when it restarts.
- **The "change pending" mark**, in memory, set while a new passcode is being set for the device.

**Matching.** A refusal is a **Status** with fid1 `250` and fid2 `255`. Fid3, the value type and the
value are ignored. A Command with these ids is not a refusal.

**What the app does with one.**
- **A passcode is stored.** The app decodes the passcode from the most recent advertisement it
  holds for the device (offset 14, decrypted with the UDID key; a raw 0 decodes to 0) and compares
  it with the stored encoded passcode.
  - **The advertised value is 0 or differs.** The app sets the refused flag and disconnects. If the
    flag was already set, it still disconnects.
  - **The two match.** The `250.255` is dropped and nothing else happens.
- **No passcode is stored, or the device is not in the stored list yet** (during the add flow, for
  example): no comparison, no flag, no disconnect.
- The `250.255` itself shows nothing. The rest of the notification is read as usual, before and
  after it, and so is everything a refused device sends.

**The flag's lifetime.** It is never persisted. A disconnect does not clear it, and neither does
any later message being accepted. Only an accepted passcode entry (below) clears it. After a
restart the app reconnects with the same stored passcode and, if that is still wrong, is refused
again.

**While refused:**
- **Outgoing.** Every message is discarded silently except `200` (identify), and `250` while a
  change is pending for that device. Every function `1` is discarded.
- **Connection.** The app does not connect a refused device and disconnects one that is ready
  (within about half a second). The exception is a device with a change pending, which is
  connected like any wanted device. Identify does not connect a refused device either.
- **After the disconnect.** The device stays on the list of wanted devices but leaves the list of
  discovered ones, so a reconnect waits for a fresh advertisement.

### Entering a passcode

The app has two 4-digit entry flows. They are the only ways to give it a passcode. Neither changes
the passcode of a device that works.

**Enter the current passcode.** Used when a device with a non-zero advertised passcode field
(offset 14) is added. The user enters the code once.
- The app encodes the digits and compares them with the passcode decoded from the advertisement.
- **Mismatch.** Nothing is stored and nothing is sent.
- **Match.** The entry is accepted (below).
- **No advertisement held for the device.** An error; nothing is stored or sent.
- The app opens no connection and sends nothing. The advertisement alone decides.

**Set a new passcode.** Used when a device whose advertised passcode field is 0 is added, or to
correct the passcode of a refused device. The user enters the code twice.
- **The second entry differs.** Nothing is sent.
- **While the new passcode is being set,** the device carries the change-pending mark and the app
  asks for a connection to it, even when it is refused.
- **No advertisement held for the device.** The digits are not kept and nothing is written.
- **Otherwise** the app keeps the digits and sends Command `250`, UInt32 = the new encoded
  passcode, at once if the link is ready or else when the device becomes ready.
- **The passphrase field of that frame** is built like any frame's, from the connection's copy of
  the stored passcode: 0 for a device without one, the old (refused) passcode for a refused
  device. Whether a device accepts a `250` under a wrong passphrase is not known.
- **The device's answer decides.** The app watches for any Status whose fid1 is `250`, checking
  neither fid2 nor fid3.
  - **It carries a UInt32 (4 bytes) equal to the new encoded passcode.** The device has echoed the
    new code, and the entry is accepted (below).
  - **Any other such Status:** another value, another value type, or any Status when the digits
    were not kept. The change failed and nothing is stored.
- **No timeout.** A refusal (`250.255`) does not end this flow, so a device that answers only with a
  refusal leaves it waiting.
- When the flow ends (close, back or success) the change-pending mark is cleared and the
  connection request withdrawn.

**Accepted (both flows), in this order:**
1. **Store the new encoded passcode** for every stored entry with that UDID (adding the device to
   the group if it is not in it yet).
2. **Wait about a second** before the next connection attempt.
3. **Restart discovery.** The app stops scanning, which empties the list of discovered devices, and
   starts again. A device whose link is down is reconnected only after a fresh advertisement. A
   link that is up is not explicitly closed and reopened.
4. **Clear the refused flag.** No function `1` is sent at this point in practice; the next one is
   sent when the app next starts following the device ("Following a device", section 4).
5. **Clear the change-pending mark** and go back.

Once nothing wants the device, the link is closed on the connection manager's next pass, and the
next connection reads the new stored passcode.

**When the passcode counts as valid.** The refused flag is cleared only by step 4, and only an entry
by the user reaches it: the entered code must match the advertisement (first flow), or the device
must echo the new value in a Status `250` (second flow). No other incoming message clears the flag:
a normal Status flood after a reconnect does not.

**The stale copy on a live link.** A link that stays up across a change keeps putting the old
passcode in its frames until the next reconnect, because each connection reads the stored passcode
only once.

## 6. Linux implementation notes

- BlueZ exposes the same manufacturer data via D-Bus (`ManufacturerData` on the Device1
  object, keyed by company ID); the 27-byte payload is the value.
- Use `bleak` (Python) or `btleplug` (Rust) for a quick start. Write to TX with
  `response=False`, subscribe to RX notifications.
- Send function `1` right after subscribing and build the state from the Status flood.
  Everything the app shows comes from those replies; there is no separate "read".
- Send function `1` again whenever a device with a power state goes from standby, off or
  warming up to on (section 4), which svolder does so that the readings a waking device
  changes are read again. Judge the state from Boolean statuses only, on the first function id alone (`10`, and
  on the amplifiers `4`), and only once `10` has reported, so the order of a flood does not read
  as a wake. Send nothing while the device refuses the passphrase, and finish one such refresh's
  replies before a write whose echo you wait for, or an old value in them will be taken for the
  echo.
- A device that reports `250.255` is refusing the passphrase (section 5). After the passphrase is
  corrected, send function `1`: the connect's was refused and what it would have read is still
  unread.
- svolder's attenuation and EQ conversions are in `crates/svolder-profiles` (`db_tables.rs`,
  `convert.rs`) and the device profiles under `profiles/`.
