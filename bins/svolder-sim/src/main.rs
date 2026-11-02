// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Serve the in-process fake Forkbeard device as a real BLE peripheral: advertise the
//! Forkbeard service and manufacturer data, and answer TX writes with RX notifications,
//! so the CLI can be exercised over an actual radio. Runs until Ctrl-C or SIGTERM.
//!
//! - [`legacy_adv`]: the advertisement as a Forkbeard device sends it, for legacy scanners.
//! - [`peripheral`]: the BlueZ side: the GATT service, the advertisement and the run loop.

use std::sync::{Arc, Mutex};

use clap::{Parser, ValueEnum};
use svolder_fake::FakeDevice;
use svolder_proto::{Passphrase, UnitId};

mod legacy_adv;
mod peripheral;

/// How the advertisement is laid out on the air.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Layout {
    /// Everything in one extended advertising PDU; needs a controller with a large
    /// `MaxAdvLen` on both ends, and scanners that listen for extended advertising.
    Extended,
    /// Flags and the service UUID in a 31-byte advertisement, the manufacturer payload in the
    /// 31-byte scan response, as a Forkbeard device does; visible to legacy-only scanners such
    /// as the official phone app. Needs bluetoothd started with `--experimental`.
    Legacy,
}

/// Which id a Loki Max band is reported on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum LokiReports {
    /// The band's own three-id form, `6.<band>.3`.
    #[default]
    Full,
    /// The two-id form, `6.<band>`.
    Short,
}

/// What `--signal` says when the profile it was given has no meter to play on.
const NO_SCALED_METER: &str =
    "svolder-sim: --signal needs a profile whose meters have a scale, such as fake-amp or aegir2";

#[derive(Parser)]
#[command(
    name = "svolder-sim",
    version,
    long_version = svolder_proto::long_version!(),
    about = "Serve a fake Forkbeard device as a BLE peripheral"
)]
struct Args {
    /// Bluetooth adapter to advertise on, e.g. hci1.
    #[arg(long)]
    adapter: String,
    /// Device profile to emulate.
    #[arg(long, default_value = "gungnir2-v2")]
    profile: String,
    /// 8-byte unit id in hex.
    #[arg(long, default_value = "0102030405060708")]
    unit: String,
    /// Advertised firmware build.
    #[arg(long, default_value_t = 3)]
    firmware: u16,
    /// Passphrase: four digits, or `none`.
    #[arg(long, default_value = "1234")]
    passphrase: String,
    /// Seed all fault controls active.
    #[arg(long)]
    faults: bool,
    /// Print every frame a central writes and every notification sent back, decoded, on stderr.
    #[arg(long)]
    trace: bool,
    /// How the advertisement is laid out on the air.
    #[arg(long, value_enum, default_value_t = Layout::Extended)]
    layout: Layout,
    /// Play a signal on the meters, ten readings a second, as a device passing music would.
    /// Needs a profile whose meters have a scale, such as `fake-amp` or `aegir2`.
    #[arg(long)]
    signal: bool,
    /// Which id a band of a Loki Max is reported on: its three-id form, or the two-id form.
    #[arg(long, value_enum, default_value_t = LokiReports::Full)]
    loki_reports: LokiReports,
    /// Have a recall of a device slot set the bands without reporting them, as a unit that
    /// pushes nothing after a recall would.
    #[arg(long)]
    silent_recall: bool,
    /// Report a change only: a Command equal to the value a control holds gets no Status, on
    /// every control. Without it only the tone-control mode is silent on a repeat.
    #[arg(long)]
    quiet_same: bool,
    /// Accept every pairing request a central makes (Just Works), as a Forkbeard device does
    /// when a phone asks to bond. The agent is bluetoothd's default while this runs, so it
    /// answers for every adapter on the host.
    #[arg(long)]
    accept_pairing: bool,
}

/// The fake device the flags describe: its faults, and how it reports a Loki Max's bands and its
/// recalls.
fn seed_device(
    args: &Args,
    profile: &svolder_profiles::Profile,
    unit: UnitId,
    passphrase: Passphrase,
) -> FakeDevice {
    let mut device = FakeDevice::from_profile(profile, unit, args.firmware, passphrase)
        .report_aliases(args.loki_reports == LokiReports::Short)
        .silent_recall(args.silent_recall)
        .quiet_same(args.quiet_same);
    if args.faults {
        for control in &profile.controls {
            if control.kind == svolder_profiles::ControlKind::Fault {
                device.set_raw(control.id, svolder_proto::Value::Bool(true));
            }
        }
    }
    device
}

// One thread runs every task, so the writes bluetoothd hands over as separate D-Bus calls start
// in the order they arrived: on several threads two writes that arrive together race, and the
// device can apply the later one first, as no real unit would.
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let reg = svolder_profiles::Registry::builtin()?;
    let profile = reg
        .get(&args.profile)
        .ok_or_else(|| format!("no profile {}", args.profile))?
        .clone();
    let unit: UnitId = args.unit.parse()?;
    let passphrase = match args.passphrase.as_str() {
        "none" => Passphrase::NONE,
        digits => {
            let d: Vec<u8> = digits
                .chars()
                .filter_map(|c| c.to_digit(10).map(|n| n as u8))
                .collect();
            let arr: [u8; 4] = d
                .as_slice()
                .try_into()
                .map_err(|_| "passphrase must be four digits or `none`")?;
            Passphrase::from_digits(arr)?
        }
    };
    let device = seed_device(&args, &profile, unit, passphrase);
    if args.signal && !device.plays_a_signal() {
        eprintln!("{NO_SCALED_METER}");
        std::process::exit(2);
    }
    let manufacturer_bytes = device.manufacturer_data().to_bytes().to_vec();
    let device = Arc::new(Mutex::new(device));

    peripheral::run(&args, device, manufacturer_bytes).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fake device the simulator serves, for the profile named.
    fn fake(profile: &str) -> FakeDevice {
        let reg = svolder_profiles::Registry::builtin().expect("the built-in profiles");
        let profile = reg.get(profile).expect("a built-in profile").clone();
        FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().expect("a unit id"),
            3,
            Passphrase::NONE,
        )
    }

    /// `--loki-reports short` has the fake report a band on its two-id form (`full` is the
    /// default), and `--silent-recall` has a recall set the bands without a report.
    #[test]
    fn loki_reports_short_and_silent_recall_reach_the_fake() {
        use svolder_proto::{FunctionId, Message, Value, decode_notification, encode_command};
        let reg = svolder_profiles::Registry::builtin().expect("the built-in profiles");
        let profile = reg.get("lokimax").expect("the Loki Max profile").clone();
        let unit: UnitId = "0102030405060708".parse().expect("a unit id");
        let handle = |device: &mut FakeDevice, command: Message| {
            let frame = encode_command(&command, Passphrase::NONE).expect("a frame");
            device
                .handle(&frame)
                .iter()
                .map(|n| decode_notification(n).expect("a notification"))
                .collect::<Vec<_>>()
        };
        let band = Message::command(FunctionId::three(6, 3, 3), Value::U8(20));
        let recall = || Message::command(FunctionId::three(7, 2, 1), Value::Data(Vec::new()));

        let plain = Args::parse_from(["svolder-sim", "--adapter", "hci0"]);
        let mut device = seed_device(&plain, &profile, unit, Passphrase::NONE);
        assert_eq!(
            handle(&mut device, band.clone())[0].messages[0].id,
            FunctionId::three(6, 3, 3)
        );
        assert!(!handle(&mut device, recall()).is_empty());

        let both = Args::parse_from([
            "svolder-sim",
            "--adapter",
            "hci0",
            "--loki-reports",
            "short",
            "--silent-recall",
        ]);
        let mut device = seed_device(&both, &profile, unit, Passphrase::NONE);
        assert_eq!(
            handle(&mut device, band)[0].messages[0].id,
            FunctionId::two(6, 3)
        );
        assert!(handle(&mut device, recall()).is_empty());
    }

    /// `--signal` is a switch, off unless it is asked for.
    #[test]
    fn the_signal_is_off_unless_it_is_asked_for() {
        let quiet = Args::parse_from(["svolder-sim", "--adapter", "hci0"]);
        assert!(!quiet.signal);
        let playing = Args::parse_from(["svolder-sim", "--adapter", "hci0", "--signal"]);
        assert!(playing.signal);
    }

    /// Only a profile whose meters have a scale can play a signal: the fake amplifier, or a
    /// profile with a logarithmic scale on its meters; on anything else `--signal` is
    /// refused, and the refusal names the profiles that work.
    #[test]
    fn only_a_profile_with_a_scaled_meter_can_play_a_signal() {
        assert!(fake("fake-amp").plays_a_signal());
        assert!(!fake("gungnir2-v2").plays_a_signal());
        let mut profile = svolder_profiles::Registry::builtin()
            .expect("the built-in profiles")
            .get("tyr")
            .expect("a built-in profile")
            .clone();
        for control in &mut profile.controls {
            if control.kind == svolder_profiles::ControlKind::Meter {
                control.range = None;
                control.log_scale = Some(svolder_profiles::meter::LogScale {
                    zero: svolder_profiles::meter::Zero::Fixed(160),
                    full: 350,
                    class_a: 0.25,
                });
            }
        }
        let logarithmic = FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().expect("a unit id"),
            3,
            Passphrase::NONE,
        );
        assert!(logarithmic.plays_a_signal());
        assert!(NO_SCALED_METER.contains("fake-amp"), "{NO_SCALED_METER}");
        assert!(NO_SCALED_METER.contains("aegir2"), "{NO_SCALED_METER}");
    }
}
