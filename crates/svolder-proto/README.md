# svolder-proto

Codec for the Schiit Forkbeard BLE protocol. No I/O, no async.

- `Message`, `FunctionId`, `Value`, `Header`: the typed form of one command or status.
- `encode_command(&Message, Passphrase)`: bytes to write to the TX characteristic.
- `decode_notification(&[u8])`: parses one RX notification into a `Notification`, whose
  `messages` are the ones that parsed and whose `dropped` records the ones that did not.
- `encode_notification(&[Message])` and `decode_command(&[u8])`: the peripheral side of
  the same framing, for a simulator or a decode tool standing in for the device.
- `ManufacturerData::parse(&[u8])`: what an advertisement says about a device, including
  its recoverable passphrase and whether it is in bootloader mode.
- `FunctionId::class()`: `Normal`, `Sensitive` (passphrase) or `Restricted` (firmware
  update). Restricted functions cannot be encoded.

The wire format is documented in `docs/protocol.md` at the repository root.
