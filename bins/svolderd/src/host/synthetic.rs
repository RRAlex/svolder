// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fake host's capture: synthetic music, made as samples and run through the same
//! [`Analyser`] a real capture is, at real time, only while it is started.
//!
//! The music is a loop of its own, the same every time it runs: a kick at 120 BPM (a sine that
//! falls from 100 to 50 Hz), a bass note that changes every two seconds, hats on the off-beat
//! (noise from a fixed seed, high-passed by differencing) leaning right, and a chord of three
//! sines as a pad that pans slowly from left to right. A page or a TUI that shows it sees a
//! tempo, beats, a spectrum with bass and treble, and a stereo image that moves.

use std::collections::VecDeque;
use std::f64::consts::TAU;
use std::time::Duration;

use async_trait::async_trait;
use svolder_host::analysis::FRAME_RATE;
use svolder_host::capture::RATE;
use svolder_host::{Analyser, Frame, FrameCapture, unix_ms};
use tokio::time::{Interval, MissedTickBehavior};

/// The beat period: 120 BPM.
const BEAT_SECONDS: f64 = 0.5;

/// The pad's chord, in Hz: A3, C#4 and E4.
const PAD_HZ: [f64; 3] = [220.0, 277.18, 329.63];

/// The samples, per channel, that make up one frame.
const HOP: usize = (RATE / FRAME_RATE) as usize;

/// The music, a sample at a time.
struct Music {
    /// How many samples have been made.
    n: u64,
    /// The state of the noise generator.
    noise: u64,
    /// The last noise sample, which the next is differenced from.
    last: f64,
}

impl Music {
    fn new() -> Self {
        Music {
            n: 0,
            noise: 0x2545_f491_4f6c_dd1d,
            last: 0.0,
        }
    }

    /// A noise sample in -1 ..= 1, from a xorshift generator with a fixed seed.
    fn noise(&mut self) -> f64 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 7;
        self.noise ^= self.noise << 17;
        (self.noise >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }

    /// The next `frames` samples, interleaved left then right.
    fn block(&mut self, frames: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(frames * 2);
        for _ in 0..frames {
            let t = self.n as f64 / f64::from(RATE);
            self.n += 1;
            let beat = t % BEAT_SECONDS;
            let kick = 0.6
                * (-8.0 * beat).exp()
                * (TAU * (50.0 * beat + 2.0 * (1.0 - (-25.0 * beat).exp()))).sin();
            let bass_hz = if ((t / 2.0) as u64).is_multiple_of(2) {
                55.0
            } else {
                41.2
            };
            let bass = 0.10 * (TAU * bass_hz * t).sin() * (0.6 + 0.4 * (TAU * t / 4.0).sin());
            let noise = self.noise();
            let hat = 0.15
                * (-60.0 * ((t + BEAT_SECONDS / 2.0) % BEAT_SECONDS)).exp()
                * (noise - self.last);
            self.last = noise;
            let pad: f64 = PAD_HZ.iter().map(|hz| 0.05 * (TAU * hz * t).sin()).sum();
            let pan = 0.5 + 0.4 * (TAU * t / 6.0).sin();
            let left = kick + bass + 0.7 * hat + pad * (1.0 - 0.8 * pan);
            let right = kick + bass + hat + pad * (0.2 + 0.8 * pan);
            out.push(left.clamp(-1.0, 1.0) as f32);
            out.push(right.clamp(-1.0, 1.0) as f32);
        }
        out
    }
}

/// A frames capture of synthetic music: it runs from [`FrameCapture::start`] to
/// [`FrameCapture::stop`], making a frame every [`FRAME_RATE`]th of a second of the clock.
pub(super) struct SyntheticFrames {
    /// The sink it was started on, while it runs.
    target: Option<String>,
    analyser: Analyser,
    music: Music,
    /// The clock a frame is made on, while it runs.
    clock: Option<Interval>,
    /// Frames made and not yet taken.
    queue: VecDeque<Frame>,
}

impl SyntheticFrames {
    pub(super) fn new() -> Self {
        SyntheticFrames {
            target: None,
            analyser: Analyser::new(RATE),
            music: Music::new(),
            clock: None,
            queue: VecDeque::new(),
        }
    }
}

#[async_trait]
impl FrameCapture for SyntheticFrames {
    fn start(&mut self, sink: &str) -> bool {
        if sink.trim().is_empty() {
            self.stop();
            return false;
        }
        if self.target.as_deref() == Some(sink) {
            return true;
        }
        self.stop();
        self.target = Some(sink.to_string());
        let mut clock =
            tokio::time::interval(Duration::from_micros(1_000_000 / u64::from(FRAME_RATE)));
        clock.set_missed_tick_behavior(MissedTickBehavior::Delay);
        self.clock = Some(clock);
        true
    }

    fn stop(&mut self) {
        self.target = None;
        self.clock = None;
        self.queue.clear();
        self.analyser = Analyser::new(RATE);
        self.music = Music::new();
    }

    fn target(&self) -> Option<String> {
        self.target.clone()
    }

    async fn next_frame(&mut self) -> Option<Frame> {
        loop {
            if let Some(frame) = self.queue.pop_front() {
                return Some(frame);
            }
            let Some(clock) = self.clock.as_mut() else {
                return std::future::pending().await;
            };
            clock.tick().await;
            let samples = self.music.block(HOP);
            for mut frame in self.analyser.push(&samples) {
                frame.capture_ms = unix_ms();
                self.queue.push_back(frame);
            }
        }
    }
}
