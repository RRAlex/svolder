// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fake device: an in-memory control map that answers protocol frames.

use std::collections::BTreeMap;

use svolder_profiles::{Control, ControlKind, EqMode, EqParam, EqSpec, Profile, roles};
use svolder_proto::function::{
    FIRMWARE_FAMILY, FIRMWARE_VERSION_QUERY, IDENTIFY, PASSPHRASE, PASSPHRASE_REJECTED, REFRESH,
};
use svolder_proto::{
    FunctionId, ManufacturerData, Message, Passphrase, UnitId, Value, ValueType, decode_command,
    encode_notification,
};

/// An in-process stand-in for a real Forkbeard device.
pub struct FakeDevice {
    product_id: u16,
    unit: UnitId,
    firmware_build: u16,
    passphrase: Passphrase,
    profile: Profile,
    state: BTreeMap<FunctionId, Value>,
    meters: Vec<Meter>,
    seen: usize,
    /// What a save kept, by slot: the value of every control a recall sets, as it stood.
    slots: BTreeMap<u8, BTreeMap<FunctionId, Value>>,
    /// Whether a control is reported on the first extra id it is read from, in place of its own.
    report_aliases: bool,
    /// Whether a recall sets its controls without reporting them.
    silent_recall: bool,
    /// Whether a Command equal to the value a control holds goes unreported, for every control.
    quiet_same: bool,
}

/// A meter a signal can play on: the function it reports under, the phase a signal reads it
/// at, and the wire type its readings are reported in. Its scale is its control's, settled
/// against the device's own state each time a level is played.
struct Meter {
    id: FunctionId,
    /// This meter's position among the profile's scaled meters, which [`Signal::at`] reads as
    /// the channel to give it its own phase, apart from every other meter's.
    ///
    /// [`Signal::at`]: crate::Signal::at
    channel: usize,
    value_type: ValueType,
}

impl FakeDevice {
    /// Build a fake seeded to a sensible default for every control the profile lists.
    pub fn from_profile(
        profile: &Profile,
        unit: UnitId,
        firmware_build: u16,
        passphrase: Passphrase,
    ) -> Self {
        let mut state = BTreeMap::new();
        for control in &profile.controls {
            state.insert(control.id, default_value(control));
        }
        if let Some(eq) = &profile.eq {
            for (id, value) in tone_control_at_rest(eq) {
                state.insert(id, value);
            }
        }
        Self {
            product_id: profile.product_id.unwrap_or(0),
            unit,
            firmware_build,
            passphrase,
            profile: profile.clone(),
            state,
            meters: meters(profile),
            seen: 0,
            slots: BTreeMap::new(),
            report_aliases: false,
            silent_recall: false,
            quiet_same: false,
        }
    }

    /// Report a control on the first extra id its profile reads it from (a Loki Max band on
    /// `6.<band>`) instead of its own id, in a refresh and in every echo and report alike. Off by
    /// default, when a control is reported on its own id.
    pub fn report_aliases(mut self, on: bool) -> Self {
        self.report_aliases = on;
        self
    }

    /// Have a recall set the controls it changes without reporting them, as a unit that pushes
    /// nothing after a recall would, whether or not a control changed. Off by default, when a
    /// recall reports every control it sets in one notification, changed or not.
    pub fn silent_recall(mut self, on: bool) -> Self {
        self.silent_recall = on;
        self
    }

    /// Have every control stay silent on a Command equal to the value it holds, as a unit that
    /// reports changes only would. Off by default, when only the tone-control mode is silent on
    /// a repeat and every other control reports it.
    pub fn quiet_same(mut self, on: bool) -> Self {
        self.quiet_same = on;
        self
    }

    /// The advertisement this device would broadcast.
    pub fn manufacturer_data(&self) -> ManufacturerData {
        ManufacturerData {
            product_id: self.product_id,
            incomplete_dfu: false,
            unit_id: self.unit,
            encrypted_passphrase: encrypt_advert_field(self.passphrase, self.unit),
            firmware_build: self.firmware_build,
            module_hardware: 1,
            module_firmware: 7,
        }
    }

    /// TX frames handled so far, whether or not they decoded.
    pub fn commands_seen(&self) -> usize {
        self.seen
    }

    /// Force a control to a raw value without a command (test seam).
    pub fn set_raw(&mut self, id: FunctionId, value: Value) {
        self.state.insert(id, value);
    }

    /// Decode one TX frame and return the notifications a real device would send back.
    pub fn handle(&mut self, tx_frame: &[u8]) -> Vec<Vec<u8>> {
        self.seen += 1;
        let decoded = match decode_command(tx_frame) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };
        if !self.passphrase.is_none() {
            let expected = self.passphrase.encrypt_for_frame(&tx_frame[4..]);
            if decoded.passphrase_field != expected {
                return vec![frame(&[Message::status(
                    PASSPHRASE_REJECTED,
                    Value::Bool(true),
                )])];
            }
        }
        let mut out = Vec::new();
        for msg in &decoded.body.messages {
            let id = msg.id;
            if id.first() == FIRMWARE_FAMILY && id.second() != Some(FIRMWARE_VERSION_QUERY) {
                panic!("firmware-update frame reached the simulator: {id}");
            }
            if id == REFRESH {
                out.push(self.refresh_notification());
            } else if id == IDENTIFY {
                // Identify: the unit blinks; no status.
            } else if id.first() == FIRMWARE_FAMILY && id.second() == Some(FIRMWARE_VERSION_QUERY) {
                out.push(frame(&[Message::status(
                    id,
                    Value::U16(self.firmware_build),
                )]));
            } else if id == PASSPHRASE {
                if let Value::U32(v) = msg.value {
                    self.passphrase = Passphrase::from_encoded(v);
                }
            } else if let Some((action, slot)) = self.profile.action_of(id) {
                // A save or a recall carries no value and gets no status of its own.
                let (is_save, is_recall) = (
                    roles::save_action(&self.profile).is_some_and(|a| a.id == action.id),
                    roles::recall_action(&self.profile).is_some_and(|a| a.id == action.id),
                );
                if is_save {
                    self.save_slot(slot);
                } else if is_recall {
                    out.extend(self.recall_slot(slot));
                }
            } else if self.profile.actions.iter().any(|action| {
                id.depth() == action.id.depth() + 1
                    && id.as_slice().starts_with(action.id.as_slice())
            }) {
                // A slot the action does not have: the device does nothing.
            } else {
                let id = self.profile.read_as(id);
                let repeat = self.state.get(&id) == Some(&msg.value);
                let is_mode = self
                    .profile
                    .eq
                    .as_ref()
                    .is_some_and(|eq| eq.mode_id() == id);
                self.state.insert(id, msg.value.clone());
                // A unit reports the tone-control mode when it changes and says nothing on a
                // Command equal to the mode it is in; `quiet_same` extends that to every control.
                if repeat && (is_mode || self.quiet_same) {
                    continue;
                }
                out.push(frame(&[Message::status(
                    self.reported_id(id),
                    msg.value.clone(),
                )]));
            }
        }
        out
    }

    /// The id a reading of control `id` is reported on: its own, or with the alias form on the
    /// first extra id the profile reads it from.
    fn reported_id(&self, id: FunctionId) -> FunctionId {
        if !self.report_aliases {
            return id;
        }
        self.profile
            .control_by_id(id)
            .and_then(|control| control.reads.first().copied())
            .unwrap_or(id)
    }

    /// The controls a recall sets, in the profile's order: those any action names in `changes`.
    fn recalled_controls(&self) -> Vec<&Control> {
        self.profile
            .controls
            .iter()
            .filter(|control| {
                self.profile
                    .actions
                    .iter()
                    .any(|action| action.changes.contains(&control.key))
            })
            .collect()
    }

    /// Keep the current value of every control a recall sets in `slot`.
    fn save_slot(&mut self, slot: u8) {
        let kept = self
            .recalled_controls()
            .into_iter()
            .filter_map(|control| {
                self.state
                    .get(&control.id)
                    .map(|value| (control.id, value.clone()))
            })
            .collect();
        self.slots.insert(slot, kept);
    }

    /// Set every control a recall sets from `slot`, a slot never saved giving each control its
    /// seeded value (0 dB on a band), and report every one of them, changed or not, in one
    /// notification unless recalls are silent.
    fn recall_slot(&mut self, slot: u8) -> Option<Vec<u8>> {
        let saved = self.slots.get(&slot);
        let wanted: Vec<(FunctionId, Value)> = self
            .recalled_controls()
            .into_iter()
            .map(|control| {
                let value = saved
                    .and_then(|kept| kept.get(&control.id))
                    .cloned()
                    .unwrap_or_else(|| default_value(control));
                (control.id, value)
            })
            .collect();
        let mut reports = Vec::new();
        for (id, value) in wanted {
            self.state.insert(id, value.clone());
            reports.push(Message::status(self.reported_id(id), value));
        }
        (!reports.is_empty() && !self.silent_recall).then(|| frame(&reports))
    }

    /// One notification reading every meter that has a scale, `level` giving the level of
    /// each channel between silence and full scale.
    ///
    /// A level lands on the meter's scale through its inverse, with the zero a scale takes
    /// from a floor read from the device's own state. The readings are stored, so a later
    /// refresh reports the last of them, and `None` comes back from a device whose meters
    /// have no scale to place a level on.
    pub fn signal_frame(&mut self, level: &dyn Fn(usize) -> f64) -> Option<Vec<u8>> {
        if self.meters.is_empty() {
            return None;
        }
        let messages: Vec<Message> = self
            .meters
            .iter()
            .map(|meter| {
                let control = self
                    .profile
                    .control_by_id(meter.id)
                    .expect("a meter of the profile");
                let placed = self
                    .profile
                    .placed(control, |id| self.state.get(&id).and_then(Value::as_i64))
                    .expect("a meter with a scale");
                let raw = placed.raw_at(level(meter.channel));
                Message::status(meter.id, typed(meter.value_type, raw))
            })
            .collect();
        for message in &messages {
            self.state.insert(message.id, message.value.clone());
        }
        Some(frame(&messages))
    }

    /// Whether this device has a meter a signal can play on, which is a meter whose profile
    /// gives it a scale.
    pub fn plays_a_signal(&self) -> bool {
        !self.meters.is_empty()
    }

    fn refresh_notification(&self) -> Vec<u8> {
        let msgs: Vec<Message> = self
            .state
            .iter()
            .map(|(id, v)| Message::status(self.reported_id(*id), v.clone()))
            .collect();
        frame(&msgs)
    }
}

/// Encode status messages into one RX notification frame. Internal data, so an encode
/// failure (only possible if a message were a Command) yields an empty frame rather than
/// panicking.
fn frame(messages: &[Message]) -> Vec<u8> {
    encode_notification(messages).unwrap_or_default()
}

/// The advertised passphrase field: the encrypted form a real unit broadcasts. Inverse of
/// `Passphrase::from_advert`.
fn encrypt_advert_field(pass: Passphrase, unit: UnitId) -> u32 {
    if pass.is_none() {
        0
    } else {
        Passphrase::encrypt(pass.encoded(), Passphrase::advert_key(unit))
    }
}

/// The tone control's functions at their resting values: the mode flat, no loudness, and
/// every band at its centre frequency, 0 dB and its centre Q — raw 64 is the centre of both the
/// frequency and the Q scale, and `gain_zero` the raw value that means 0 dB. A device reports
/// these with the rest of its status, and a front end has no band to step until it hears them.
fn tone_control_at_rest(eq: &EqSpec) -> Vec<(FunctionId, Value)> {
    let mut values = vec![
        (eq.mode_id(), Value::U8(EqMode::Flat as u8)),
        (eq.loudness_id(), Value::U8(0)),
    ];
    for band in 0..eq.bands.len() {
        for (param, raw) in [
            (EqParam::Frequency, 64),
            (EqParam::Gain, eq.gain_zero),
            (EqParam::Q, 64),
        ] {
            if let Some(id) = eq.band_id(band, param) {
                values.push((id, Value::U8(raw)));
            }
        }
    }
    values
}

/// Every meter a profile's controls describe, in profile order: a meter needs a scale for a
/// level to land on, telling one on a real amp's profile (with no scale) apart from one a
/// signal can actually play. A meter's position in this list is the phase a signal reads it
/// at, so every channel of a multichannel profile sounds apart from the others rather than
/// several of them reading the same phase together.
fn meters(profile: &Profile) -> Vec<Meter> {
    profile
        .controls
        .iter()
        .filter(|control| control.kind == ControlKind::Meter)
        .filter(|control| control.scale().is_some())
        .enumerate()
        .map(|(channel, control)| Meter {
            id: control.id,
            channel,
            value_type: control.value_type,
        })
        .collect()
}

/// The sample rate, in hertz, a fake device reads from the start.
pub const SAMPLE_RATE_HZ: u32 = 48_000;

/// The raw value a sample rate control reads [`SAMPLE_RATE_HZ`] as: the one the profile labels
/// with that rate in kilohertz (`48 kHz`).
fn rate_value(control: &Control) -> Option<u8> {
    let label = format!("{} kHz", f64::from(SAMPLE_RATE_HZ) / 1000.0);
    control
        .values
        .iter()
        .find(|(_, name)| *name == label)
        .map(|(raw, _)| *raw)
}

/// A seeded default for one control.
fn default_value(control: &Control) -> Value {
    match control.kind {
        // A simulated amp streams its meters from the start, so a front end has them to draw
        // without a switch first.
        ControlKind::Toggle if control.key == svolder_profiles::meter::POWER_METERS => {
            Value::Bool(true)
        }
        ControlKind::Toggle | ControlKind::Fault => Value::Bool(false),
        // The device listens to its first input, so a computer on a fake DAC is on its USB 1.
        ControlKind::Readout if control.key == "usb1_connected" => Value::Bool(true),
        ControlKind::Meter => Value::U16(0),
        // The rate the device reads is the one `SAMPLE_RATE_HZ` names, found by its label in the
        // profile's own table.
        ControlKind::Select | ControlKind::Readout => {
            let rate = (control.kind == ControlKind::Readout && control.key == "sample_rate")
                .then(|| rate_value(control))
                .flatten();
            let raw = rate
                .or_else(|| control.values.iter().map(|(v, _)| *v).min())
                .unwrap_or(0);
            typed(control.value_type, i64::from(raw))
        }
        ControlKind::Level => {
            let mid = match control.range {
                Some((lo, hi)) => control.center.unwrap_or((lo + hi) / 2),
                None => 0,
            };
            typed(control.value_type, mid)
        }
    }
}

/// Build a `Value` of the given wire type from an integer, saturating into range.
fn typed(value_type: ValueType, n: i64) -> Value {
    match value_type {
        ValueType::Bool => Value::Bool(n != 0),
        ValueType::U8 => Value::U8(n.clamp(0, u8::MAX as i64) as u8),
        ValueType::U16 => Value::U16(n.clamp(0, u16::MAX as i64) as u16),
        ValueType::U32 => Value::U32(n.clamp(0, u32::MAX as i64) as u32),
        ValueType::U64 => Value::U64(n.max(0) as u64),
        ValueType::I8 => Value::I8(n.clamp(i8::MIN as i64, i8::MAX as i64) as i8),
        ValueType::I16 => Value::I16(n.clamp(i16::MIN as i64, i16::MAX as i64) as i16),
        ValueType::I32 => Value::I32(n.clamp(i32::MIN as i64, i32::MAX as i64) as i32),
        ValueType::I64 => Value::I64(n),
        ValueType::Data => Value::Data(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svolder_profiles::meter::{LogScale, Zero};
    use svolder_profiles::{ControlKind, Registry};
    use svolder_proto::{
        FunctionId, Message, Notification, Passphrase, UnitId, Value, decode_notification,
        encode_command,
    };

    fn gungnir() -> FakeDevice {
        let reg = Registry::builtin().unwrap();
        let profile = reg.get("gungnir2-v2").unwrap().clone();
        let unit = "0102030405060708".parse::<UnitId>().unwrap();
        FakeDevice::from_profile(
            &profile,
            unit,
            3,
            Passphrase::from_digits([1, 2, 3, 4]).unwrap(),
        )
    }

    #[test]
    fn an_amp_reports_its_meters_switched_on_from_the_start() {
        let profile = Registry::builtin().unwrap().get("aegir2").unwrap().clone();
        let device = FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse::<UnitId>().unwrap(),
            1,
            Passphrase::from_digits([1, 2, 3, 4]).unwrap(),
        );
        let id = profile.control_by_key("power_meters").unwrap().id;
        assert_eq!(device.state.get(&id), Some(&Value::Bool(true)));
    }

    /// The tone control's functions are part of what a refresh reports, at the resting values a
    /// device leaves the factory with: flat, no loudness, every band at its centre frequency,
    /// 0 dB and its centre Q. Without them a front end has no band to step.
    #[test]
    fn a_refresh_reports_the_tone_control_at_rest() {
        use svolder_profiles::{EqParam, Registry};
        let reg = Registry::builtin().unwrap();
        let eq = reg
            .get("gungnir2-v2")
            .unwrap()
            .eq
            .clone()
            .expect("the Gungnir 2 has a tone control");
        let mut d = gungnir();
        let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        let tx = encode_command(
            &Message::command(FunctionId::one(1), Value::Data(vec![])),
            pass,
        )
        .unwrap();
        let refresh = only(d.handle(&tx));
        let value = |id: FunctionId| {
            refresh
                .messages
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.value.clone())
        };
        assert_eq!(value(eq.mode_id()), Some(Value::U8(1)), "mode: flat");
        assert_eq!(value(eq.loudness_id()), Some(Value::U8(0)), "loudness: off");
        for band in 0..eq.bands.len() {
            assert_eq!(
                value(eq.band_id(band, EqParam::Frequency).unwrap()),
                Some(Value::U8(64)),
                "band {band} frequency at its centre"
            );
            assert_eq!(
                value(eq.band_id(band, EqParam::Gain).unwrap()),
                Some(Value::U8(eq.gain_zero)),
                "band {band} gain at 0 dB"
            );
            assert_eq!(
                value(eq.band_id(band, EqParam::Q).unwrap()),
                Some(Value::U8(64)),
                "band {band} Q at its centre"
            );
        }
    }

    /// The fake amplifier, whose meters read on the Aegir 2's scale (zero 160, full scale 350).
    fn fake_amp() -> FakeDevice {
        let reg = Registry::builtin().unwrap();
        let profile = reg.get("fake-amp").unwrap().clone();
        FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        )
    }

    /// A signal frame reads every scaled meter at once: full scale is the scale's full reading,
    /// half way up the live part is the reading the Aegir 2's scale puts there, and the readings
    /// stay on the device so a refresh reports the last of them.
    #[test]
    fn a_signal_frame_carries_one_meter_status_per_scaled_meter_and_none_for_a_dac() {
        let mut d = fake_amp();
        let frame = d
            .signal_frame(&|ch| if ch == 0 { 1.0 } else { 0.5 })
            .expect("meters to report");
        let n = decode_notification(&frame).unwrap();
        assert_eq!(n.messages.len(), 2);
        assert_eq!(n.messages[0].id, FunctionId::two(20, 1));
        assert_eq!(n.messages[0].value, Value::U16(350));
        assert_eq!(n.messages[1].id, FunctionId::two(20, 2));
        assert_eq!(n.messages[1].value, Value::U16(173));
        assert!(d.plays_a_signal());

        let refresh = decode_notification(&d.refresh_notification()).unwrap();
        let left = refresh
            .messages
            .iter()
            .find(|m| m.id == FunctionId::two(20, 1))
            .expect("the left meter in a refresh");
        assert_eq!(left.value, Value::U16(350));

        let mut dac = gungnir();
        assert!(
            dac.signal_frame(&|_| 1.0).is_none(),
            "a DAC has no meter to play on"
        );
        assert!(!dac.plays_a_signal());
    }

    /// A level outside the scale is pulled back onto it: nothing a caller can pass reads past
    /// the scale's full reading or below its zero.
    #[test]
    fn a_level_off_the_scale_is_clamped_to_it() {
        let mut d = fake_amp();
        let frame = d
            .signal_frame(&|ch| if ch == 0 { 12.0 } else { -3.0 })
            .expect("meters to report");
        let n = decode_notification(&frame).unwrap();
        assert_eq!(n.messages[0].value, Value::U16(350));
        assert_eq!(n.messages[1].value, Value::U16(160));
    }

    /// Every channel of a multichannel profile gets its own phase: reading the closure by the
    /// channel index it is handed, the five meters of `fake-five` come back five different
    /// raw values rather than several of them sharing one channel's reading.
    #[test]
    fn each_channel_of_a_multichannel_profile_reads_its_own_phase() {
        let reg = Registry::builtin().unwrap();
        let profile = reg.get("fake-five").unwrap().clone();
        let mut d = FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        );
        let frame = d
            .signal_frame(&|ch| ch as f64 / 10.0)
            .expect("meters to report");
        let n = decode_notification(&frame).unwrap();
        assert_eq!(n.messages.len(), 5);
        let raws: Vec<u16> = n
            .messages
            .iter()
            .map(|m| match &m.value {
                Value::U16(raw) => *raw,
                other => panic!("a meter reads a u16, got {other:?}"),
            })
            .collect();
        let unique: std::collections::BTreeSet<u16> = raws.iter().copied().collect();
        assert_eq!(
            unique.len(),
            raws.len(),
            "two channels read the same phase: {raws:?}"
        );
    }

    /// A built-in profile with `scale` on every meter that has a scale of its own.
    fn device_with_scale(name: &str, scale: LogScale) -> FakeDevice {
        let mut profile = Registry::builtin().unwrap().get(name).unwrap().clone();
        for control in &mut profile.controls {
            if control.kind == ControlKind::Meter {
                control.log_scale = Some(scale.clone());
            }
        }
        FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        )
    }

    fn first_meter_raw(device: &mut FakeDevice, level: f64) -> Value {
        let frame = device.signal_frame(&|_| level).expect("meters to report");
        decode_notification(&frame).unwrap().messages[0]
            .value
            .clone()
    }

    /// A level plays through the scale's inverse: on the Aegir 2's scale (zero 160, full 350)
    /// half way up the bar is 160 + sqrt(191) - 1, 173.
    #[test]
    fn a_log_scaled_meter_plays_the_signal_through_the_scales_inverse() {
        let scale = LogScale {
            zero: Zero::Fixed(160),
            full: 350,
            class_a: 0.25,
        };
        let mut d = device_with_scale("fake-amp", scale);
        assert_eq!(first_meter_raw(&mut d, 0.5), Value::U16(173));
        assert_eq!(first_meter_raw(&mut d, 0.0), Value::U16(160));
        assert_eq!(first_meter_raw(&mut d, 1.0), Value::U16(350));
        assert!(d.plays_a_signal());
    }

    /// A zero that follows a floor is read from the floor readout the device holds: 0 before
    /// it has reported gives a zero of 10, and a floor of 100 a zero of 150.
    #[test]
    fn a_meter_that_follows_a_floor_plays_against_the_floor_the_device_reports() {
        let scale = LogScale {
            zero: Zero::Floor("meter_floor".into()),
            full: 1000,
            class_a: 0.25,
        };
        let mut d = device_with_scale("tyr", scale);
        assert_eq!(first_meter_raw(&mut d, 1.0), Value::U16(1000));
        assert_eq!(first_meter_raw(&mut d, 0.0), Value::U16(10));
        d.set_raw(FunctionId::two(20, 2), Value::U16(100));
        assert_eq!(first_meter_raw(&mut d, 0.0), Value::U16(150));
    }

    /// A floor whose zero reaches full scale (700 puts the zero at 1050, past 1000) leaves no
    /// live part to place a level on: every level sends the zero.
    #[test]
    fn a_floor_past_full_scale_sends_the_zero_at_every_level() {
        let scale = LogScale {
            zero: Zero::Floor("meter_floor".into()),
            full: 1000,
            class_a: 0.25,
        };
        let mut d = device_with_scale("tyr", scale);
        d.set_raw(FunctionId::two(20, 2), Value::U16(700));
        for level in [0.0, 0.5, 1.0] {
            assert_eq!(first_meter_raw(&mut d, level), Value::U16(1050), "{level}");
        }
    }

    fn only(frames: Vec<Vec<u8>>) -> Notification {
        assert_eq!(frames.len(), 1, "expected exactly one notification");
        decode_notification(&frames[0]).unwrap()
    }

    #[test]
    fn advertises_its_identity() {
        let d = gungnir();
        let m = d.manufacturer_data();
        assert_eq!(m.product_id, 2);
        assert_eq!(m.unit_id, "0102030405060708".parse().unwrap());
        assert_eq!(m.firmware_build, 3);
        assert_eq!(m.passphrase().digits(), [1, 2, 3, 4]);
    }

    /// A profile with a sample rate readout starts at 48 kHz, the rate `SAMPLE_RATE_HZ` names,
    /// so a front end has a rate to compare the computer's against.
    #[test]
    fn the_sample_rate_reads_48_khz_from_the_start() {
        let reg = Registry::builtin().unwrap();
        for name in ["gungnir2-v2", "gungnir2-v1"] {
            let profile = reg.get(name).unwrap();
            let control = profile
                .controls
                .iter()
                .find(|c| c.key == "sample_rate")
                .unwrap();
            let device = FakeDevice::from_profile(
                profile,
                "0102030405060708".parse().unwrap(),
                3,
                Passphrase::from_digits([1, 2, 3, 4]).unwrap(),
            );
            let Value::U8(raw) = device.state.get(&control.id).cloned().unwrap() else {
                panic!("the rate is a u8");
            };
            let label = &control.values.iter().find(|(v, _)| *v == raw).unwrap().1;
            assert_eq!(label, "48 kHz", "{name}");
        }
        assert_eq!(SAMPLE_RATE_HZ, 48_000);
    }

    /// A DAC with a reading per USB port starts with the first input's port connected and the
    /// other not, so a front end can tell which port the computer is on.
    #[test]
    fn the_first_usb_port_reads_connected_from_the_start() {
        let reg = Registry::builtin().unwrap();
        let profile = reg.get("gungnir2-v2").unwrap();
        let device = FakeDevice::from_profile(
            profile,
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::from_digits([1, 2, 3, 4]).unwrap(),
        );
        let reads = |key: &str| {
            let control = profile.controls.iter().find(|c| c.key == key).unwrap();
            device.state.get(&control.id).cloned()
        };
        assert_eq!(reads("usb1_connected"), Some(Value::Bool(true)));
        assert_eq!(reads("usb2_connected"), Some(Value::Bool(false)));
    }

    #[test]
    fn refresh_returns_a_status_per_control() {
        let mut d = gungnir();
        let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        let frame = encode_command(
            &Message::command(FunctionId::one(1), Value::Data(vec![])),
            pass,
        )
        .unwrap();
        let n = only(d.handle(&frame));
        // gungnir2-v2 flattens to: input(2), phase(3), nos(4), mute(5), sample_rate(6),
        // volume(10), balance(11), usb1_connected(7.1), usb2_connected(7.2), spdif_locked(8),
        // fault_overcurrent(230.1). Assert a few are present.
        let ids: Vec<String> = n.messages.iter().map(|m| m.id.to_string()).collect();
        for id in ["2", "5", "10", "11"] {
            assert!(
                ids.iter().any(|i| i == id),
                "missing status {id} in {ids:?}"
            );
        }
        assert_eq!(d.commands_seen(), 1);
    }

    #[test]
    fn a_set_is_stored_and_echoed() {
        let mut d = gungnir();
        let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        let set =
            encode_command(&Message::command(FunctionId::one(2), Value::U8(4)), pass).unwrap();
        let echo = only(d.handle(&set));
        assert_eq!(echo.messages.len(), 1);
        assert_eq!(echo.messages[0].id, FunctionId::one(2));
        assert_eq!(echo.messages[0].value, Value::U8(4));
        // A later refresh reflects the new value.
        let refresh = encode_command(
            &Message::command(FunctionId::one(1), Value::Data(vec![])),
            pass,
        )
        .unwrap();
        let n = only(d.handle(&refresh));
        let input = n
            .messages
            .iter()
            .find(|m| m.id == FunctionId::one(2))
            .unwrap();
        assert_eq!(input.value, Value::U8(4));
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let mut d = gungnir();
        let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
        let set =
            encode_command(&Message::command(FunctionId::one(2), Value::U8(4)), wrong).unwrap();
        let n = only(d.handle(&set));
        assert_eq!(n.messages.len(), 1);
        assert_eq!(n.messages[0].id, FunctionId::two(250, 255));
    }

    #[test]
    fn version_query_answers() {
        let mut d = gungnir();
        let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        let q = encode_command(
            &Message::command(FunctionId::three(210, 2, 1), Value::U16(0)),
            pass,
        )
        .unwrap();
        let n = only(d.handle(&q));
        assert_eq!(n.messages[0].id, FunctionId::three(210, 2, 1));
        assert!(matches!(n.messages[0].value, Value::U16(_)));
    }

    /// A passphrase-set command whose value is not a `U32` is silently ignored: no status
    /// comes back, and the passphrase already in effect keeps working.
    #[test]
    fn a_non_u32_passphrase_value_is_ignored() {
        let mut d = gungnir();
        let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        let set = encode_command(
            &Message::command(FunctionId::one(250), Value::Bool(true)),
            pass,
        )
        .unwrap();
        assert!(
            d.handle(&set).is_empty(),
            "a passphrase set never answers with a status"
        );
        // The old passphrase is still the one in effect.
        let refresh = encode_command(
            &Message::command(FunctionId::one(1), Value::Data(vec![])),
            pass,
        )
        .unwrap();
        assert!(!only(d.handle(&refresh)).messages.is_empty());
    }

    /// A set carries its own wire type; the fake stores and echoes exactly what arrived,
    /// with nothing here to check it against the profile's own declared type for the id.
    #[test]
    fn a_set_of_the_wrong_wire_type_is_stored_and_echoed_as_sent() {
        let mut d = gungnir();
        let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        // Function 2 (input) is a u8 in the profile; send it as a u32 instead.
        let set =
            encode_command(&Message::command(FunctionId::one(2), Value::U32(4)), pass).unwrap();
        let echo = only(d.handle(&set));
        assert_eq!(echo.messages[0].value, Value::U32(4));
    }

    #[test]
    #[should_panic(expected = "firmware-update")]
    fn firmware_update_frame_panics() {
        // The codec refuses to encode 210.1, so assemble the frame by hand: a zero
        // passphrase field followed by a body. `encode_notification` builds a valid
        // [len][header][fids][value][checksum] body; its direction bit says Status, but
        // the device keys the panic on the function id alone. A device with no passphrase
        // skips the passphrase check, so the message reaches the per-message loop.
        let reg = Registry::builtin().unwrap();
        let profile = reg.get("gungnir2-v2").unwrap().clone();
        let mut open = FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        );
        let body = svolder_proto::encode_notification(&[Message::status(
            FunctionId::two(210, 1),
            Value::Bool(true),
        )])
        .unwrap();
        let mut frame = vec![0u8; 4];
        frame.extend_from_slice(&body);
        let _ = open.handle(&frame);
    }

    /// A fake Loki Max with no passphrase.
    fn loki() -> FakeDevice {
        let profile = Registry::builtin().unwrap().get("lokimax").unwrap().clone();
        FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        )
    }

    /// Send one command to a device with no passphrase and decode every notification it
    /// answers with.
    fn send(device: &mut FakeDevice, message: Message) -> Vec<Notification> {
        let frame = encode_command(&message, Passphrase::NONE).unwrap();
        device
            .handle(&frame)
            .iter()
            .map(|n| decode_notification(n).unwrap())
            .collect()
    }

    fn write_band(device: &mut FakeDevice, id: FunctionId, raw: u8) -> Vec<Notification> {
        send(device, Message::command(id, Value::U8(raw)))
    }

    /// The command of the action at `7.<action>` for `slot`: empty data.
    fn slot_command(action: u8, slot: u8) -> Message {
        Message::command(FunctionId::three(7, action, slot), Value::Data(Vec::new()))
    }

    /// The band `n` as the profile's control id.
    fn band(n: u8) -> FunctionId {
        FunctionId::three(6, n, 3)
    }

    /// Every band's reading in a refresh, as `(id, raw)` in the order reported.
    fn refreshed_bands(device: &mut FakeDevice) -> Vec<(FunctionId, u8)> {
        let refresh = send(
            device,
            Message::command(FunctionId::one(1), Value::Data(vec![])),
        );
        refresh[0]
            .messages
            .iter()
            .filter(|m| m.id.first() == 6)
            .map(|m| (m.id, m.value.as_i64().unwrap() as u8))
            .collect()
    }

    /// A band written on its two-id form (as the app's predefined settings do) sets the band a
    /// three-id write sets, and the device echoes on the band's own id.
    #[test]
    fn a_two_id_band_write_sets_the_band_and_echoes_on_the_band() {
        let mut d = loki();
        let echo = write_band(&mut d, FunctionId::two(6, 2), 20);
        assert_eq!(echo.len(), 1);
        assert_eq!(echo[0].messages.len(), 1);
        assert_eq!(echo[0].messages[0].id, band(2));
        assert_eq!(echo[0].messages[0].value, Value::U8(20));
        let bands = refreshed_bands(&mut d);
        assert_eq!(bands.len(), 6, "one reading per band: {bands:?}");
        assert!(bands.contains(&(band(2), 20)));
        assert!(bands.iter().all(|(id, _)| id.depth() == 3), "{bands:?}");
        // A three-id write sets the same one.
        write_band(&mut d, band(2), 9);
        let bands = refreshed_bands(&mut d);
        assert!(bands.contains(&(band(2), 9)));
        assert_eq!(bands.len(), 6);
    }

    /// With the alias form on, a band is reported on its two-id form in an echo and in a
    /// refresh alike, whichever form it was written on.
    #[test]
    fn with_short_reports_a_band_is_reported_on_its_two_id_form() {
        let mut d = loki().report_aliases(true);
        let echo = write_band(&mut d, band(4), 21);
        assert_eq!(echo[0].messages[0].id, FunctionId::two(6, 4));
        assert_eq!(echo[0].messages[0].value, Value::U8(21));
        let echo = write_band(&mut d, FunctionId::two(6, 5), 12);
        assert_eq!(echo[0].messages[0].id, FunctionId::two(6, 5));
        let bands = refreshed_bands(&mut d);
        assert_eq!(bands.len(), 6);
        assert!(bands.iter().all(|(id, _)| id.depth() == 2), "{bands:?}");
        assert!(bands.contains(&(FunctionId::two(6, 4), 21)));
        assert!(bands.contains(&(FunctionId::two(6, 5), 12)));
    }

    /// A save keeps the six bands in its slot; a recall puts them back and reports every band
    /// in one notification.
    #[test]
    fn a_save_then_a_recall_brings_the_saved_bands_back_and_reports_each() {
        let mut d = loki();
        for (n, raw) in [(1u8, 28u8), (2, 21), (3, 17), (4, 15), (5, 15), (6, 3)] {
            write_band(&mut d, band(n), raw);
        }
        assert!(
            send(&mut d, slot_command(1, 2)).is_empty(),
            "a save is answered with no status"
        );
        for n in 1..=6 {
            write_band(&mut d, band(n), 15);
        }
        let reported = send(&mut d, slot_command(2, 2));
        assert_eq!(reported.len(), 1, "every band in one notification");
        let got: Vec<(FunctionId, Value)> = reported[0]
            .messages
            .iter()
            .map(|m| (m.id, m.value.clone()))
            .collect();
        assert_eq!(
            got,
            vec![
                (band(1), Value::U8(28)),
                (band(2), Value::U8(21)),
                (band(3), Value::U8(17)),
                (band(4), Value::U8(15)),
                (band(5), Value::U8(15)),
                (band(6), Value::U8(3)),
            ],
            "every band, in band order"
        );
        let bands = refreshed_bands(&mut d);
        assert_eq!(bands[0], (band(1), 28));
        assert_eq!(bands[5], (band(6), 3));
        // Another slot is untouched by the save.
        send(&mut d, slot_command(2, 1));
        assert!(refreshed_bands(&mut d).iter().all(|(_, raw)| *raw == 15));
    }

    /// A slot that was never saved recalls every band at its 0 dB step, which is Flat.
    #[test]
    fn a_slot_never_saved_recalls_every_band_at_0_db() {
        let mut d = loki();
        for n in 1..=6 {
            write_band(&mut d, band(n), 30);
        }
        let reported = send(&mut d, slot_command(2, 3));
        assert_eq!(reported.len(), 1);
        assert_eq!(reported[0].messages.len(), 6);
        assert!(
            reported[0]
                .messages
                .iter()
                .all(|m| m.value == Value::U8(15))
        );
        assert!(refreshed_bands(&mut d).iter().all(|(_, raw)| *raw == 15));
    }

    /// With the silent-recall knob on, a recall sets the bands and reports nothing, so only a
    /// refresh shows them.
    #[test]
    fn a_silent_recall_sets_the_bands_and_reports_nothing() {
        let mut d = loki().silent_recall(true);
        write_band(&mut d, band(1), 28);
        send(&mut d, slot_command(1, 1));
        write_band(&mut d, band(1), 15);
        assert!(send(&mut d, slot_command(2, 1)).is_empty());
        assert_eq!(refreshed_bands(&mut d)[0], (band(1), 28));
        // Nor does one that changes nothing.
        assert!(send(&mut d, slot_command(2, 1)).is_empty());
    }

    /// A recall that leaves every band where it was still reports every band, in one
    /// notification, as a unit that pushes its bands after a recall does.
    #[test]
    fn a_recall_that_changes_nothing_still_reports_the_bands() {
        let mut d = loki();
        let reported = send(&mut d, slot_command(2, 1));
        assert_eq!(reported.len(), 1);
        let got: Vec<(FunctionId, Value)> = reported[0]
            .messages
            .iter()
            .map(|m| (m.id, m.value.clone()))
            .collect();
        assert_eq!(
            got,
            (1..=6)
                .map(|n| (band(n), Value::U8(15)))
                .collect::<Vec<_>>()
        );
    }

    /// An action command is stored nowhere as a reading and answered with no status of its
    /// own, so a refresh never lists a `7.x` function.
    #[test]
    fn an_action_command_gets_no_status_of_its_own() {
        let mut d = loki();
        assert!(send(&mut d, slot_command(1, 1)).is_empty());
        let recalled = send(&mut d, slot_command(2, 1));
        assert!(
            recalled
                .iter()
                .flat_map(|n| &n.messages)
                .all(|m| m.id.first() != 7),
            "a recall reports the bands, never a 7.x status"
        );
        assert!(
            send(&mut d, slot_command(2, 4)).is_empty(),
            "a slot the device does not have is ignored"
        );
        let refresh = send(
            &mut d,
            Message::command(FunctionId::one(1), Value::Data(vec![])),
        );
        assert!(
            refresh[0].messages.iter().all(|m| m.id.first() != 7),
            "{:?}",
            refresh[0].messages
        );
    }

    /// A fake of the named built-in profile with no passphrase.
    fn named(name: &str) -> FakeDevice {
        let profile = Registry::builtin().unwrap().get(name).unwrap().clone();
        FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        )
    }

    fn mode_of(name: &str) -> FunctionId {
        let profile = Registry::builtin().unwrap().get(name).unwrap().clone();
        profile.eq.expect("a tone control").mode_id()
    }

    /// The mode is reported when it changes and not on a Command equal to the mode the device is
    /// in, on every profile with a tone control.
    #[test]
    fn the_mode_is_reported_on_a_change_and_silent_on_a_repeat() {
        for name in ["bifrost3", "gungnir2-v2"] {
            let mut d = named(name);
            let mode = mode_of(name);
            let set = |d: &mut FakeDevice, raw: u8| send(d, Message::command(mode, Value::U8(raw)));
            let changed = set(&mut d, 2);
            assert_eq!(changed.len(), 1, "{name}: a change is reported");
            assert_eq!(changed[0].messages[0].id, mode);
            assert_eq!(changed[0].messages[0].value, Value::U8(2));
            assert!(set(&mut d, 2).is_empty(), "{name}: a repeat is silent");
            assert_eq!(
                set(&mut d, 3).len(),
                1,
                "{name}: the next change is reported"
            );
            // The resting mode is flat: writing it while flat is a repeat too.
            let mut flat = named(name);
            assert!(
                set(&mut flat, 1).is_empty(),
                "{name}: flat over flat is silent"
            );
        }
    }

    /// Every control but the mode is echoed on a repeat unless `quiet_same` is on.
    #[test]
    fn a_control_but_the_mode_still_echoes_a_repeat() {
        let mut d = named("gungnir2-v2");
        let set = |d: &mut FakeDevice| send(d, Message::command(FunctionId::one(2), Value::U8(4)));
        assert_eq!(set(&mut d).len(), 1);
        assert_eq!(set(&mut d).len(), 1, "the repeat is echoed");
    }

    /// Under `quiet_same` no control reports a repeat, but a change still is, and a refresh still
    /// reports every control.
    #[test]
    fn quiet_same_silences_a_repeat_on_every_control() {
        let mut d = named("gungnir2-v2").quiet_same(true);
        let set = |d: &mut FakeDevice, id: u8, raw: u8| {
            send(d, Message::command(FunctionId::one(id), Value::U8(raw)))
        };
        assert_eq!(set(&mut d, 2, 4).len(), 1, "a change is reported");
        assert!(set(&mut d, 2, 4).is_empty(), "a repeat is silent");
        let mode = mode_of("gungnir2-v2");
        assert!(send(&mut d, Message::command(mode, Value::U8(1))).is_empty());
        let refresh = send(
            &mut d,
            Message::command(FunctionId::one(1), Value::Data(vec![])),
        );
        assert_eq!(refresh.len(), 1);
        let reads = |id: FunctionId| {
            refresh[0]
                .messages
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.value.clone())
        };
        assert_eq!(reads(FunctionId::one(2)), Some(Value::U8(4)));
    }

    /// A refresh reports the mode, whatever was written before.
    #[test]
    fn a_refresh_still_reports_the_mode() {
        let mut d = named("bifrost3");
        let mode = mode_of("bifrost3");
        let _ = send(&mut d, Message::command(mode, Value::U8(2)));
        let _ = send(&mut d, Message::command(mode, Value::U8(2)));
        let refresh = send(
            &mut d,
            Message::command(FunctionId::one(1), Value::Data(vec![])),
        );
        let value = refresh[0]
            .messages
            .iter()
            .find(|m| m.id == mode)
            .map(|m| m.value.clone());
        assert_eq!(value, Some(Value::U8(2)));
    }
}
