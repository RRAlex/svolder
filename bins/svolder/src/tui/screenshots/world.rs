// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The world the scenes are drawn in: a Gungnir 2 and an Aegir 2 on the fake device, one host
//! playing "So What", the Sennheiser HD 800 S preset on the DAC, and every clock pinned, so the
//! same code draws the same pictures on any machine on any day.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use svolder_autoeq::Entry;
use svolder_fake::signal::Signal;
use svolder_host::fft::BANDS;
use svolder_host::{Block, NowPlaying, SinkState, Status};
use svolder_ipc::{ColorDepth, TuiConfig, ValueObject};
use svolder_profiles::convert::display_to_raw;
use svolder_profiles::roles;
use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{Imported, Preset, plan_recall};
use svolder_profiles::{EqParam, Profile, Registry};
use svolder_proto::message::Message;
use svolder_proto::{FunctionId, Value};

use crate::backend::{Backend, DeviceRow, Update};
use crate::tui::app::App;
use crate::tui::run::Look;
use crate::tui::screenshots::text::PINNED_PATH;
use crate::tui::widgets::graph;

/// The header clock every picture shows.
pub(in crate::tui::screenshots) const CLOCK: &str = "14:02";

/// The wall clock the pictures pin, in unix milliseconds: 2026-10-01 12:00 UTC.
pub(in crate::tui::screenshots) const WALL_MS: u64 = 1_790_856_000_000;

/// The preset on the DAC, named as its measurement is.
pub(in crate::tui::screenshots) const PRESET: &str = "Sennheiser HD 800 S";

/// The unit id the header shows.
pub(in crate::tui::screenshots) const UNIT: &str = "0102030405060708";

/// Where in the signal the stills read it.
pub(in crate::tui::screenshots) const STILL_SIGNAL_AT: Duration = Duration::from_secs(320);

/// Where in the signal the animation's first frame reads it.
pub(in crate::tui::screenshots) const ANIMATION_SIGNAL_AT: Duration = Duration::from_secs(312);

/// How far back the fed history reaches: the graphs' five-minute span.
pub(in crate::tui::screenshots) const HISTORY: Duration = Duration::from_secs(300);

/// How often the host's capture delivers a block.
pub(in crate::tui::screenshots) const HOST_BLOCK: Duration = Duration::from_millis(250);

/// How often the amp's meters report.
const METER_EVERY: Duration = Duration::from_millis(100);

/// The DAC volume the stills show.
pub(in crate::tui::screenshots) const STILL_VOLUME_DB: f64 = -24.0;

/// The DAC volume the animation starts at, before its step up to [`STILL_VOLUME_DB`].
pub(in crate::tui::screenshots) const START_VOLUME_DB: f64 = -30.0;

/// The host's sink, as the Host pane names it.
pub(in crate::tui::screenshots) const SINK: &str = "Digital Output (S/PDIF)";

/// The host sink's volume.
const SINK_VOLUME: f32 = 0.54;

/// What a pinned instant lies ahead of the real clock, so a history reaching back from it
/// never underflows an [`Instant`].
const BASE_AHEAD: Duration = Duration::from_secs(3600);

/// The delay between the bands' copies of the signal, so the spectrum ripples.
const BAND_LAG: Duration = Duration::from_millis(370);

/// The HD 800 S's measurement, as the AutoEq index lists it.
pub(in crate::tui::screenshots) fn measurement() -> Entry {
    Entry {
        name: PRESET.into(),
        source: "oratory1990".into(),
        form: "over-ear".into(),
        rig: None,
    }
}

/// A correction written by hand in the shape of the HD 800 S's, so no AutoEq data is committed.
pub(in crate::tui::screenshots) fn stand_in() -> Correction {
    let filter = |kind, f, g, q| SourceFilter { kind, f, g, q };
    Correction {
        preamp_db: -5.5,
        filters: vec![
            filter(FilterKind::LowShelf, 105.0, 5.0, 0.7),
            filter(FilterKind::Peaking, 300.0, -1.5, 1.0),
            filter(FilterKind::Peaking, 6000.0, -3.0, 2.0),
            filter(FilterKind::HighShelf, 10_000.0, 1.0, 0.7),
        ],
    }
}

/// The preset the DAC wears, made the way the import screen makes one.
pub(in crate::tui::screenshots) fn preset(correction: Correction) -> Preset {
    Preset::imported(
        PRESET,
        Imported {
            origin: Some(measurement().origin()),
            correction,
        },
    )
}

/// A current-thread runtime on a paused clock, for the async fakes to run in.
pub(in crate::tui::screenshots) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("a runtime builds")
}

/// What the scenes share: how it looks, the preset the DAC wears, and the instant every scene
/// counts from.
pub(in crate::tui::screenshots) struct World {
    /// The look every picture is drawn with: amber, true colour, default `[tui]`.
    pub look: Look,
    /// The preset on the DAC.
    pub preset: Preset,
    /// The instant a scene starts at.
    pub base: Instant,
}

impl World {
    /// A world whose preset is `correction`. It holds the wall clock at [`WALL_MS`], so build it
    /// on the thread that draws.
    pub fn new(correction: Correction) -> World {
        graph::hold_wall_ms(WALL_MS);
        World {
            look: Look::new(
                TuiConfig::default(),
                false,
                ColorDepth::TrueColor,
                std::env::temp_dir().join("svolder-screenshots-never-written.toml"),
            ),
            preset: preset(correction),
            base: Instant::now() + BASE_AHEAD,
        }
    }
}

/// The two devices the Devices screen lists.
fn device_rows() -> Vec<DeviceRow> {
    [("Gungnir", "Gungnir 2"), ("Aegir 2", "Aegir 2")]
        .into_iter()
        .map(|(name, product)| DeviceRow {
            name: name.into(),
            product: Some(product.into()),
            connected: Some(true),
            retry_in_secs: None,
            ..DeviceRow::default()
        })
        .collect()
}

/// Whether the snapshot keeps `value`: every reading but a meter's, and not one whose id is
/// not a function id at all.
pub(super) fn kept_in_snapshot(profile: &Profile, value: &ValueObject) -> bool {
    let Ok(id) = value.id.parse::<FunctionId>() else {
        return false;
    };
    profile
        .control_by_id(id)
        .is_none_or(|control| control.kind != svolder_profiles::ControlKind::Meter)
}

/// Report one reading of `id` as of `now`, the way a device's feed does.
fn push(app: &mut App, id: FunctionId, value: Value, now: Instant) {
    let message = Message::status(id, value);
    let reading = svolder_profiles::describe(&app.profile, &message);
    let value = ValueObject::from_reading(&reading, &message);
    app.apply(Update::Status(value), now);
}

/// Report the control `key` reading `value`.
fn push_key(app: &mut App, key: &str, value: Value, now: Instant) {
    let id = app.profile.control_by_key(key).expect(key).id;
    push(app, id, value, now);
}

/// Give `app` what every scene's device has: the look, the pinned clock and config path, a live
/// session on the unit, the facts a daemon lists for it (its product, profile and a signal heard
/// three minutes ago), and a link history with one dropout a few hours back.
fn prepare(app: &mut App, world: &World) {
    app.set_look(&world.look.tui, &world.look.effects);
    app.mascot.capable = world.look.mascot_capable();
    app.clock = CLOCK.into();
    app.config_path = PathBuf::from(PINNED_PATH);
    app.session.connected = true;
    app.session.feed_open = true;
    app.session.daemon = true;
    app.session.unit = Some(UNIT.into());
    app.session.facts = Some(DeviceRow {
        name: app.session.device_name.clone(),
        product: Some(app.profile.display.clone()),
        connected: Some(true),
        profile: Some(app.profile.name.clone()),
        product_id: app.profile.product_id,
        signal: Some(svolder_ipc::Signal {
            dbm: -58,
            source: svolder_ipc::SignalSource::Advert,
            heard_at_ms: WALL_MS - 3 * 60_000,
        }),
        ..DeviceRow::default()
    });
    const HOUR_MS: u64 = 3_600_000;
    app.readings.link.seed([
        (WALL_MS - 12 * HOUR_MS, true),
        (WALL_MS - 7 * HOUR_MS, false),
        (WALL_MS - 7 * HOUR_MS + 4 * 60_000, true),
    ]);
}

/// The host every scene shares: linked, on the bus, capturing the sink, playing "So What".
fn host(app: &mut App) {
    app.host.linked = true;
    app.host.bus = true;
    app.host.capturing = true;
    app.host.sink = Some(SinkState {
        volume: SINK_VOLUME,
        muted: false,
        name: SINK.into(),
        target: Some("scene-sink".into()),
    });
    app.host.playing = Some(NowPlaying {
        genre: Vec::new(),
        title: "So What".into(),
        artist: "Miles Davis".into(),
        identity: "mpv".into(),
        status: Status::Playing,
        ..Default::default()
    });
}

/// An app on the built-in profile `profile_name`, named `name`, holding what the fake device
/// reports on connecting, apart from its meters: those start with the history fed afterwards,
/// since an older sample would fold into the newest bin of a series that already had one.
async fn device(world: &World, profile_name: &str, name: &str, now: Instant) -> App {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get(profile_name)
        .expect("the profile is built in")
        .clone();
    let (mut backend, _gate) = Backend::fake_over(profile.clone(), Duration::from_secs(5)).await;
    let mut snapshot = backend.snapshot().await.expect("the fake answers");
    snapshot.retain(|value| kept_in_snapshot(&profile, value));
    drop(backend);
    let mut app = App::new(profile, name.into(), &world.look.tui, device_rows());
    app.replace_all(snapshot, now);
    prepare(&mut app, world);
    host(&mut app);
    app
}

/// The Gungnir 2 on S/PDIF at `volume_db`, wearing the preset and standing on the host.
pub(in crate::tui::screenshots) async fn gungnir(
    world: &World,
    now: Instant,
    volume_db: f64,
) -> App {
    let mut app = device(world, "gungnir2-v2", "Gungnir", now).await;
    push_key(&mut app, "input", Value::U8(3), now);
    push_key(&mut app, "spdif_locked", Value::Bool(true), now);
    push_key(&mut app, "sample_rate", Value::U8(2), now);
    set_volume(&mut app, volume_db, now);
    app.presets.saved = vec![world.preset.clone()];
    let eq = app.profile.eq.clone().expect("a tone control");
    let plan = plan_recall(&eq, |id| app.raw_of(id), &world.preset).expect("the preset fits");
    for write in plan.writes {
        let id = write.id(&eq).expect("the write has a function");
        push(&mut app, id, Value::U8(write.raw()), now);
    }
    app.presets.loaded_preset = Some(PRESET.into());
    app
}

/// The Aegir 2, awake and out of standby, its two meters ready to be fed.
pub(in crate::tui::screenshots) async fn aegir2(world: &World, now: Instant) -> App {
    let mut app = device(world, "aegir2", "Aegir 2", now).await;
    push_key(&mut app, "power", Value::Bool(true), now);
    push_key(&mut app, "power_meters", Value::Bool(true), now);
    app
}

/// The level of `channel` of the seeded signal at `t`, between 0 and 1.
pub(in crate::tui::screenshots) fn level(t: Duration, channel: usize) -> f64 {
    Signal::new(1).at(t, channel)
}

/// The capture's block at `t`: each band the signal a little later than the one below it and a
/// little quieter, and the two channel levels the signal's two channels.
pub(in crate::tui::screenshots) fn block(t: Duration) -> Block {
    let mut bands = [0.0_f32; BANDS];
    for (k, band) in bands.iter_mut().enumerate() {
        let fade = 1.0 - k as f64 / 24.0;
        *band = (level(t + BAND_LAG * k as u32, k % 2) * fade) as f32;
    }
    Block {
        bands,
        left: level(t, 0) as f32,
        right: level(t, 1) as f32,
    }
}

/// The instants `step` apart over the `HISTORY` before `end`, oldest first, each with how far
/// before `end` it lies.
fn back_in_time(step: Duration, end: Instant) -> impl Iterator<Item = (Duration, Instant)> {
    let count = (HISTORY.as_millis() / step.as_millis()) as u32;
    (0..=count).rev().map(move |n| (step * n, end - step * n))
}

/// Feed the host's capture over the `HISTORY` before `end`, oldest first, the signal at
/// `signal_end` being the moment `end`.
pub(in crate::tui::screenshots) fn feed_host_history(
    app: &mut App,
    signal_end: Duration,
    end: Instant,
) {
    for (back, at) in back_in_time(HOST_BLOCK, end) {
        app.heard_block(&block(signal_end - back), at);
    }
}

/// Feed the amp's two meters over the `HISTORY` before `end`, oldest first.
pub(in crate::tui::screenshots) fn feed_meter_history(
    app: &mut App,
    signal_end: Duration,
    end: Instant,
) {
    for (back, at) in back_in_time(METER_EVERY, end) {
        feed_meter(app, signal_end - back, at);
    }
}

/// Report both of the amp's meters at the signal's time `t`, as of `now`: each channel's level
/// is the fraction of its meter's live part, turned into the raw reading the amp's scale puts
/// there.
pub(in crate::tui::screenshots) fn feed_meter(app: &mut App, t: Duration, now: Instant) {
    for (key, channel) in [("meter_1", 0), ("meter_2", 1)] {
        let control = app.profile.control_by_key(key).expect("a meter");
        let placed = app.placed(control).expect("the meter has a scale");
        let raw = u16::try_from(placed.raw_at(level(t, channel))).expect("a reading");
        push_key(app, key, Value::U16(raw), now);
    }
}

/// Report the DAC volume at the step nearest `db`.
pub(in crate::tui::screenshots) fn set_volume(app: &mut App, db: f64, now: Instant) {
    let volume = roles::volume(&app.profile).expect("a volume");
    let raw = display_to_raw(volume, db).expect("the volume converts");
    let id = volume.id;
    push(app, id, Value::U8(u8::try_from(raw).expect("a step")), now);
}

/// Raise the band with the lowest gain by `db` and answer which band it was; the first of
/// several with the same gain.
pub(in crate::tui::screenshots) fn rise(app: &mut App, db: f64, now: Instant) -> usize {
    let eq = app.profile.eq.clone().expect("a tone control");
    let gains: Vec<(u8, FunctionId)> = (0..eq.bands.len())
        .map(|band| {
            let id = eq.band_id(band, EqParam::Gain).expect("a band");
            let raw = app.raw_of(id).expect("the band reads");
            (u8::try_from(raw).expect("a step"), id)
        })
        .collect();
    let band = (0..gains.len())
        .min_by(|&a, &b| {
            eq.gain_db(gains[a].0)
                .total_cmp(&eq.gain_db(gains[b].0))
                .then(a.cmp(&b))
        })
        .expect("a band");
    let (raw, id) = gains[band];
    push(app, id, Value::U8(eq.gain_raw(eq.gain_db(raw) + db)), now);
    band
}
