// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The host's own audio as the TUI shows it: the default sink's volume, what the followed media
//! player plays, that player's transport, and what the sink plays as the capture of its monitor
//! reads it — a spectrum and a level.
//!
//! Nothing here reaches the host. The loop reads the sink, follows the players and runs the
//! capture, and folds what they say into a [`Host`]; the screens draw from it, and the keys and
//! clicks on the Host group ask the loop for what they mean.

use std::time::{Duration, Instant};

use svolder_host::fft::BANDS;
use svolder_host::{Button, NowPlaying, RouteFacts, SinkState, Status};
use svolder_ipc::host::{HostView, PlayingView};

use super::widgets::marquee::Marquee;

/// What joins a track's title to its artist on the `Now playing` row.
const BY: &str = " — ";
/// How far one press of `←→` on the Host volume row moves the sink's volume, in percent.
pub const VOLUME_STEP: u8 = 2;
/// The loudest the keys take the sink: 100 %, never a boost.
pub const VOLUME_MAX: u8 = 100;
/// What the Host group's heading says.
pub const HEADING: &str = "Host";
/// What the Host volume row is called.
pub const VOLUME: &str = "Host volume";
/// What the Transport row is called.
pub const TRANSPORT: &str = "Transport";
/// What the Path row is called.
pub const PATH: &str = "Path";
/// What a toast says to a press on the transport with no player to take it.
pub const NO_PLAYER: &str = "no player";
/// How the Transport row draws each of [`Button::ALL`], in its order: previous, play, pause,
/// next, in the plain geometric glyphs.
pub const GLYPHS: [&str; 4] = ["◀◀", "▶", "▮▮", "▶▶"];
/// Columns between two of the Transport row's buttons.
pub const BUTTON_GAP: usize = 2;
/// What the Host EQ row is called.
pub const HOST_EQ: &str = "Host EQ";
/// What the Host EQ row reads while the computer EQ is off.
pub const HOST_EQ_OFF: &str = "off";
/// What a press on the Host EQ row says when there is no computer EQ to put back.
pub const HOST_EQ_HINT: &str = "p then h on a preset";
/// What the Spectrum row is called.
pub const SPECTRUM: &str = "Spectrum";
/// The glyphs the Spectrum row draws a band in, from silence to the top of the scale.
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// How long a band takes to fall back from the top of the scale to silence while no louder
/// block lands on it.
pub const FALL: Duration = Duration::from_millis(750);
/// How long a beat of the daemon's frames pulses the Spectrum row and lifts the breathing
/// borders: short, so each beat reads as a beat and the next finds the pulse over.
pub const BEAT_PULSE: Duration = Duration::from_millis(200);

/// One row of the device screen's Host group, in the order it draws them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostRow {
    /// The sink's volume and mute.
    Volume,
    /// The followed player's four transport buttons.
    Transport,
    /// The computer EQ: its preset's name while it is on, else `off`.
    HostEq,
}

/// The Host group's rows, in the order the Controls pane draws them under the device's own.
pub const ROWS: [HostRow; 3] = [HostRow::Volume, HostRow::Transport, HostRow::HostEq];

impl HostRow {
    /// What the row is called.
    pub fn label(self) -> &'static str {
        match self {
            HostRow::Volume => VOLUME,
            HostRow::Transport => TRANSPORT,
            HostRow::HostEq => HOST_EQ,
        }
    }
}

/// What a daemon the TUI is joined to says of the host, while its host side is available: the
/// route to the default sink, which the Path row judges against the device's own readings.
/// The daemon's player rides in [`Host::playing`], as the TUI's own watcher's does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FromDaemon {
    /// The route as the daemon last read it; `None` before its first read of the graph. Boxed:
    /// the host's state is carried by every wait of the loop, and the route is large.
    pub route: Option<Box<RouteFacts>>,
}

/// What the TUI knows of the host's own audio.
#[derive(Debug, Clone)]
pub struct Host {
    /// Whether the loop reaches the host for this session at all: false under
    /// `SVOLDER_HOST=off`, and for an app no loop has linked (the one a test builds).
    pub linked: bool,
    /// Whether `SVOLDER_HOST=off` in the environment turned host audio off for this process,
    /// whatever `[tui] host_audio` says.
    pub held_off: bool,
    /// The sink as it last read: `None` before a read has answered, and on a host with no
    /// sink to read (no `wpctl`, or one that failed).
    pub sink: Option<SinkState>,
    /// The volume asked for last, in percent, while what was asked of the sink has not all been
    /// answered: what the next step is taken from, so two presses before the first answer move
    /// two steps.
    pub asked: Option<u8>,
    /// Whether the session bus answered, so there are players to follow, or none to find on it.
    pub bus: bool,
    /// What the followed player plays, or `None` with no player on the bus. The daemon's player
    /// while [`Host::daemon`] is set, else the one the loop's own watcher follows.
    pub playing: Option<NowPlaying>,
    /// What the joined daemon says of the host while it says its host side is available. Set,
    /// the daemon is the authority for the player, the transport and the path, and the loop's
    /// own watcher of the players stands aside; `None` on a direct session and on a daemon with
    /// no host side.
    pub daemon: Option<FromDaemon>,
    /// Where the `Now playing` row's words have been carried, for words too long for it: out
    /// and back for as long as they are, starting again with every new track.
    pub marquee: Marquee,
    /// Which of the Transport row's buttons its cursor is on, an index into [`Button::ALL`].
    pub button: usize,
    /// Whether the loop has a capture of the sink's monitor running.
    pub capturing: bool,
    /// How loud the capture reads, the louder of its two channels, smoothed over the blocks
    /// that have landed; `None` before the first.
    pub level: Option<f64>,
    /// How loud each of the capture's two channels reads, left then right, smoothed the same
    /// way over the same blocks; `None` before the first.
    pub channels: Option<[f64; 2]>,
    /// When the block the levels last moved on landed, which the next block's step is timed
    /// from; `None` before the first.
    pub level_at: Option<Instant>,
    /// The Spectrum row's bands.
    pub bands: Bands,
    /// When the last beat of the daemon's frames landed, `None` before the first.
    pub beat: Option<Instant>,
    /// Whether the frame last drawn put the Spectrum row on the screen: inside the Controls
    /// pane, which a short terminal can leave it no room in, and under no box. The loop sets
    /// this from what each frame reports.
    pub spectrum_on_screen: bool,
}

impl Host {
    /// A host nothing has been heard from yet, as of `now`.
    pub fn new(now: Instant) -> Host {
        Host {
            linked: false,
            held_off: false,
            sink: None,
            asked: None,
            bus: false,
            playing: None,
            daemon: None,
            marquee: Marquee::new(now),
            button: 0,
            capturing: false,
            level: None,
            channels: None,
            level_at: None,
            bands: Bands::default(),
            beat: None,
            spectrum_on_screen: false,
        }
    }

    /// The capture stopped, or never started: nothing it read is left to draw.
    pub fn capture_stopped(&mut self) {
        self.capturing = false;
        self.level = None;
        self.channels = None;
        self.level_at = None;
        self.bands = Bands::default();
        self.beat = None;
    }

    /// A beat of the daemon's frames landed at `now`.
    pub fn beat_landed(&mut self, now: Instant) {
        self.beat = Some(now);
    }

    /// How strongly the last beat pulses at `now`, `1.0` as it lands and falling evenly to
    /// `0.0` after [`BEAT_PULSE`].
    pub fn pulse(&self, now: Instant) -> f32 {
        let Some(landed) = self.beat else {
            return 0.0;
        };
        let age = now.saturating_duration_since(landed).as_secs_f32();
        (1.0 - age / BEAT_PULSE.as_secs_f32()).max(0.0)
    }

    /// The button the Transport row's cursor is on.
    pub fn button(&self) -> Button {
        Button::ALL[self.button.min(Button::ALL.len() - 1)]
    }

    /// Move the Transport row's cursor `dir` buttons along, held at either end.
    pub fn move_button(&mut self, dir: i64) {
        let last = Button::ALL.len() - 1;
        self.button = match dir > 0 {
            true => (self.button + 1).min(last),
            false => self.button.min(last).saturating_sub(1),
        };
    }

    /// The button that matches what the followed player is doing, which the Transport row
    /// draws in the accent: play while it plays, pause while it is paused, and none while it
    /// is stopped or there is no player.
    pub fn lit(&self) -> Option<Button> {
        match self.playing.as_ref()?.status {
            Status::Playing => Some(Button::Play),
            Status::Paused => Some(Button::Pause),
            Status::Stopped => None,
        }
    }

    /// Take what a read of the sink answered; with nothing more asked of the sink after it
    /// (`settled`), the next step is taken from what it read.
    pub fn fold_read(&mut self, state: Option<SinkState>, settled: bool) {
        self.sink = state;
        if settled {
            self.asked = None;
        }
    }

    /// The volume the next step is taken from, in percent: the one asked for last while the
    /// sink has not answered it, else the one it read; `None` before it has read.
    pub fn planned(&self) -> Option<u16> {
        self.asked
            .map(u16::from)
            .or_else(|| self.sink.as_ref().map(|sink| percent(sink.volume)))
    }

    /// The volume one step of `dir` (up for positive) asks for, in percent: [`VOLUME_STEP`]
    /// from [`Host::planned`], never past [`VOLUME_MAX`] nor below 0 %. `None` at either end,
    /// and before the sink has read. A sink something else boosted past 100 % comes down to
    /// 100 % on a step down, and a step up takes it nowhere.
    pub fn stepped(&self, dir: i64) -> Option<u8> {
        let now = self.planned()?;
        let max = u16::from(VOLUME_MAX);
        let step = u16::from(VOLUME_STEP);
        let next = match dir > 0 {
            true if now >= max => return None,
            true => (now + step).min(max),
            false if now == 0 => return None,
            false => now.saturating_sub(step).min(max),
        };
        u8::try_from(next).ok()
    }

    /// Take what the followed player plays now, `None` with no player left to follow, as of
    /// `now`: other words than the row said start its marquee again.
    pub fn fold_playing(&mut self, playing: Option<NowPlaying>, now: Instant) {
        if playing.as_ref().map(words) != self.playing.as_ref().map(words) {
            self.marquee.reset(now);
        }
        self.playing = playing;
    }

    /// Take the daemon's view of the host. An available one makes the daemon the authority: its
    /// player is the one shown and its route is the one judged. One that says the host side is
    /// off or unavailable gives the host back to the loop's own watcher, which leaves nothing
    /// shown until it speaks; before any available view it changes nothing.
    pub fn fold_daemon(&mut self, view: HostView, now: Instant) {
        if !view.available {
            if self.daemon.take().is_some() {
                self.lost_bus();
            }
            return;
        }
        self.daemon = Some(FromDaemon {
            route: view.route.map(Box::new),
        });
        self.bus = true;
        self.fold_playing(view.playing.map(now_playing_of), now);
    }

    /// Whether the joined daemon is the authority for the player and the path.
    pub fn daemon_owns(&self) -> bool {
        self.daemon.is_some()
    }

    /// The bus answered, and `playing` is what its followed player plays, if any.
    pub fn found_bus(&mut self, playing: Option<NowPlaying>, now: Instant) {
        self.bus = true;
        self.fold_playing(playing, now);
    }

    /// The bus went away, or never answered: there are no players to follow.
    pub fn lost_bus(&mut self) {
        self.bus = false;
        self.playing = None;
    }

    /// The words the `Now playing` row says of the followed player's track, `Title — Artist`,
    /// with whichever of the two it gave; `None` with no player.
    pub fn now_playing(&self) -> Option<String> {
        self.playing.as_ref().map(words)
    }
}

/// The Spectrum row's [`BANDS`] bands as it draws them: each block's own the moment it lands,
/// and between blocks each band falling back towards silence at one fixed rate — the whole
/// scale in [`FALL`] — so the row settles once the music stops.
#[derive(Debug, Clone, Copy, Default)]
pub struct Bands {
    /// Each band as the last block left it: that block's own, or where the band was still
    /// falling from an earlier, louder one.
    held: [f32; BANDS],
    /// When the last block landed; `None` before the first.
    landed: Option<Instant>,
}

impl Bands {
    /// Land a block's `bands` at `now`: each band takes the block's own, or where it was still
    /// falling from, whichever is higher. A band that is not a number is silence.
    pub fn land(&mut self, bands: &[f32; BANDS], now: Instant) {
        let fallen = self.at(now);
        for ((held, fresh), fallen) in self.held.iter_mut().zip(bands).zip(fallen) {
            let fresh = match fresh.is_finite() {
                true => fresh.clamp(0.0, 1.0),
                false => 0.0,
            };
            *held = fresh.max(fallen);
        }
        self.landed = Some(now);
    }

    /// Each band as the row draws it at `now`, `0.0..=1.0`: where the last block left it,
    /// fallen back by the time since.
    pub fn at(&self, now: Instant) -> [f32; BANDS] {
        let Some(landed) = self.landed else {
            return [0.0; BANDS];
        };
        let fall = now.saturating_duration_since(landed).as_secs_f32() / FALL.as_secs_f32();
        self.held.map(|held| (held - fall).max(0.0))
    }

    /// Whether any band is above silence at `now`: the row is still moving, falling back if
    /// nothing louder lands.
    pub fn moving(&self, now: Instant) -> bool {
        self.at(now).iter().any(|band| *band > 0.0)
    }
}

/// The glyph the Spectrum row draws a band `level` loud in: [`BLOCKS`]' lowest for silence and
/// its full block for the top of the scale, the nearest of the eight between.
pub fn glyph(level: f32) -> char {
    let level = match level.is_finite() {
        true => level.clamp(0.0, 1.0),
        false => 0.0,
    };
    let steps = (BLOCKS.len() - 1) as f32;
    BLOCKS[(level * steps).round() as usize]
}

/// `volume` (1.0 = 100 %) in whole percent.
pub fn percent(volume: f32) -> u16 {
    let percent = (volume * 100.0).round();
    match percent.is_finite() {
        true => percent.clamp(0.0, f32::from(u16::MAX)) as u16,
        false => 0,
    }
}

/// What the daemon's followed player plays, as the Now playing row holds it. The cover, the
/// genre and the player's own ids are not carried: the TUI draws none of them.
fn now_playing_of(view: PlayingView) -> NowPlaying {
    let status = match view.status.as_str() {
        "playing" => Status::Playing,
        "paused" => Status::Paused,
        _ => Status::Stopped,
    };
    NowPlaying {
        title: view.title,
        artist: view.artist,
        album: view.album,
        identity: view.player,
        status,
        length_us: view.length_us,
        progress: view.progress,
        ..NowPlaying::default()
    }
}

/// `Title — Artist`, or whichever of the two `playing` gives, or nothing for a track that gives
/// neither.
fn words(playing: &NowPlaying) -> String {
    [playing.title.as_str(), playing.artist.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(BY)
}

#[cfg(test)]
mod tests {
    use svolder_host::Status;

    use super::*;

    /// What a player plays: `title` by `artist`.
    fn playing(title: &str, artist: &str) -> NowPlaying {
        NowPlaying {
            genre: Vec::new(),
            title: title.into(),
            artist: artist.into(),
            identity: "mpv".into(),
            status: Status::Playing,
            ..Default::default()
        }
    }

    /// A host whose sink reads `volume`.
    fn reading(volume: f32) -> Host {
        let mut host = Host::new(Instant::now());
        host.fold_read(
            Some(SinkState {
                volume,
                muted: false,
                name: "USB DAC".into(),
                target: None,
            }),
            true,
        );
        host
    }

    /// A step is 2 %, capped at 100 % — never a boost — and at 0 %; a sink something else
    /// boosted comes down to 100 % and goes no higher.
    #[test]
    fn a_step_is_two_percent_and_never_past_a_hundred() {
        for (volume, up, down) in [
            (0.54, Some(56), Some(52)),
            (0.99, Some(100), Some(97)),
            (1.0, None, Some(98)),
            (0.01, Some(3), Some(0)),
            (0.0, Some(2), None),
            (1.2, None, Some(100)),
        ] {
            let host = reading(volume);
            assert_eq!(host.stepped(1), up, "up from {volume}");
            assert_eq!(host.stepped(-1), down, "down from {volume}");
        }
        assert_eq!(
            Host::new(Instant::now()).stepped(1),
            None,
            "stepped with no reading"
        );
    }

    /// A step is taken from the volume asked for last while the sink has not answered, and
    /// from what it reads once it has.
    #[test]
    fn a_step_follows_on_from_the_one_asked_for_until_the_sink_answers() {
        let mut host = reading(0.54);
        host.asked = Some(56);
        assert_eq!(host.stepped(1), Some(58));
        host.fold_read(host.sink.clone(), false);
        assert_eq!(
            host.stepped(1),
            Some(58),
            "an answer before the last forgot the ask"
        );
        host.fold_read(host.sink.clone(), true);
        assert_eq!(
            host.stepped(1),
            Some(56),
            "the ask outlived the answer to it"
        );
    }

    /// The Transport row's cursor moves a button at a time and stops at either end.
    #[test]
    fn the_transport_cursor_moves_a_button_at_a_time_and_stops_at_the_ends() {
        let mut host = Host::new(Instant::now());
        assert_eq!(host.button(), Button::Previous);
        host.move_button(-1);
        assert_eq!(
            host.button(),
            Button::Previous,
            "the cursor went round the start"
        );
        for wanted in [Button::Play, Button::Pause, Button::Next, Button::Next] {
            host.move_button(1);
            assert_eq!(host.button(), wanted);
        }
        host.move_button(-1);
        assert_eq!(host.button(), Button::Pause);
    }

    /// Play is lit while the player plays and pause while it is paused; nothing is lit while
    /// it is stopped or with no player.
    #[test]
    fn the_button_matching_the_players_state_is_lit() {
        let mut host = Host::new(Instant::now());
        assert_eq!(host.lit(), None);
        for (status, lit) in [
            (Status::Playing, Some(Button::Play)),
            (Status::Paused, Some(Button::Pause)),
            (Status::Stopped, None),
        ] {
            let now = NowPlaying {
                status,
                ..playing("So What", "Miles Davis")
            };
            host.fold_playing(Some(now), Instant::now());
            assert_eq!(host.lit(), lit, "{status:?}");
        }
    }

    /// A block's bands land at once, however high; between blocks each band falls back at one
    /// rate, the whole scale in [`FALL`]; a quieter block leaves a band where it was still
    /// falling from, a louder one takes it up at once, and the row settles once nothing more
    /// lands.
    #[test]
    fn a_band_lands_with_its_block_and_falls_back_at_one_rate_between_blocks() {
        let t0 = Instant::now();
        let mut bands = Bands::default();
        assert_eq!(bands.at(t0), [0.0; BANDS]);
        assert!(!bands.moving(t0), "a row nothing landed on moves");

        let mut loud = [0.0; BANDS];
        loud[3] = 1.0;
        loud[4] = 0.5;
        bands.land(&loud, t0);
        assert_eq!(bands.at(t0)[3], 1.0, "the block did not land as it is");
        let half = t0 + FALL / 2;
        let fallen = bands.at(half);
        assert!((fallen[3] - 0.5).abs() < 1e-6, "{fallen:?}");
        assert!(fallen[4].abs() < 1e-6, "{fallen:?}");

        let mut quiet = [0.0; BANDS];
        quiet[3] = 0.2;
        quiet[5] = 0.9;
        quiet[6] = f32::NAN;
        bands.land(&quiet, half);
        let landed = bands.at(half);
        assert!(
            (landed[3] - 0.5).abs() < 1e-6,
            "a quieter block cut a falling band short: {landed:?}"
        );
        assert_eq!(landed[5], 0.9, "a louder block did not land at once");
        assert_eq!(landed[6], 0.0, "a band that is not a number is not silence");
        assert!(bands.moving(half + FALL / 2));
        assert!(!bands.moving(half + FALL), "the row never settled");
    }

    /// A band is drawn in the nearest of the eight block glyphs: the lowest for silence and
    /// the full block for the top of the scale, whatever lies past either end.
    #[test]
    fn a_band_is_drawn_in_the_nearest_of_eight_blocks() {
        for (level, drawn) in [
            (0.0, '▁'),
            (1.0 / 7.0, '▂'),
            (3.0 / 7.0, '▄'),
            (0.5, '▅'),
            (6.0 / 7.0, '▇'),
            (1.0, '█'),
            (-1.0, '▁'),
            (2.0, '█'),
            (f32::NAN, '▁'),
        ] {
            assert_eq!(glyph(level), drawn, "{level}");
        }
    }

    #[test]
    fn a_volume_reads_in_whole_percent() {
        assert_eq!(percent(0.54), 54);
        assert_eq!(percent(0.565), 57);
        assert_eq!(percent(1.5), 150);
        assert_eq!(percent(-0.1), 0);
        assert_eq!(percent(f32::NAN), 0);
    }

    #[test]
    fn the_row_says_title_and_artist_or_whichever_the_player_gave() {
        let mut host = Host::new(Instant::now());
        assert_eq!(host.now_playing(), None);
        for (title, artist, said) in [
            ("So What", "Miles Davis", "So What — Miles Davis"),
            ("So What", "", "So What"),
            ("", "Miles Davis", "Miles Davis"),
            ("", "", ""),
        ] {
            host.fold_playing(Some(playing(title, artist)), Instant::now());
            assert_eq!(host.now_playing().as_deref(), Some(said));
        }
    }

    /// A new track starts the marquee again from the beginning; the same words, paused or
    /// played, leave it where it was.
    #[test]
    fn a_new_track_starts_the_marquee_again() {
        let start = Instant::now();
        let mut host = Host::new(start);
        host.fold_playing(Some(playing("So What", "Miles Davis")), start);
        host.marquee.offset = 5;
        let paused = NowPlaying {
            status: Status::Paused,
            ..playing("So What", "Miles Davis")
        };
        host.fold_playing(Some(paused), start);
        assert_eq!(host.marquee.offset, 5, "the same words moved the marquee");
        host.fold_playing(Some(playing("Blue in Green", "Miles Davis")), start);
        assert_eq!(
            host.marquee.offset, 0,
            "a new track left the marquee where it was"
        );
    }

    /// A genre that arrives or changes is not other words: the marquee stays where it was.
    #[test]
    fn a_genre_does_not_restart_the_marquee() {
        let start = Instant::now();
        let mut host = Host::new(start);
        host.fold_playing(Some(playing("So What", "Miles Davis")), start);
        host.marquee.offset = 5;
        let jazz = NowPlaying {
            genre: vec!["Jazz".into()],
            ..playing("So What", "Miles Davis")
        };
        assert_eq!(words(&jazz), words(&playing("So What", "Miles Davis")));
        host.fold_playing(Some(jazz), start);
        assert_eq!(host.marquee.offset, 5, "a genre restarted the marquee");
    }
}
