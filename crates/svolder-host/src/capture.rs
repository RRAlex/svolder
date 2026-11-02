// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the sink plays, captured from its monitor through `pw-record` and handed on as blocks
//! of 16 bands and two levels ([`PwRecord`]) or as analysed frames of 240 bands per channel
//! ([`PwFrames`], through [`Analyser`]).
//!
//! The capture only ever records an output: `pw-record` runs with the stream property
//! `stream.capture.sink = true` and `--target` naming the sink, so PipeWire links it to that
//! sink's monitor, and without a sink name nothing starts ([`pw_record_args`] is the whole of
//! what it is run with). The stream is also passive (`node.passive = true`) and asks for no rate
//! of its own (`node.rate = 0`), so the tap does not keep the sink awake or pin the graph's
//! rate. Its samples stay in memory only as long as it takes to turn a block of them into a
//! [`Block`] or a [`Frame`]; nothing is written, kept or sent.

use std::ffi::OsString;
use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

use crate::analysis::{Analyser, FRAME_RATE, Frame};
use crate::fft::{BANDS, Spectrum, level};
use crate::players::unix_ms;

/// The environment variable naming the `pw-record` to run, for [`PwRecord::from_env`]; unset
/// or empty, it is `pw-record` on `PATH`.
pub const PW_RECORD_ENV: &str = "SVOLDER_PW_RECORD";

/// The rate the capture asks for, in Hz.
pub const RATE: u32 = 48_000;

/// The channels the capture asks for, interleaved left then right.
pub const CHANNELS: usize = 2;

/// The frames in one block: at [`RATE`], a block every 21⅓ ms.
pub const FRAMES: usize = 1024;

/// How many blocks wait for a reader before newer ones are dropped: a reader that falls behind
/// gets the blocks it missed dropped rather than an ever older backlog.
const QUEUE: usize = 8;

/// What one block of the sink's output looks like.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    /// The spectrum of both channels mixed, in the [`BANDS`] bands from 40 Hz to 16 kHz, each
    /// on the drawing scale ([`crate::fft::scale`]).
    pub bands: [f32; BANDS],
    /// The left channel's RMS level on the drawing scale.
    pub left: f32,
    /// The right channel's RMS level on the drawing scale.
    pub right: f32,
}

impl Block {
    /// The block `interleaved` stereo samples (left, right, left, …) make, through
    /// `spectrum`. A sample that is not a number counts as silence.
    pub fn of(spectrum: &mut Spectrum, interleaved: &[f32]) -> Self {
        let sample = |s: f32| if s.is_finite() { s } else { 0.0 };
        let frames = interleaved.chunks_exact(CHANNELS);
        let mono: Vec<f32> = frames
            .clone()
            .map(|f| (sample(f[0]) + sample(f[1])) / 2.0)
            .collect();
        Self {
            bands: spectrum.bands(&mono),
            left: level(frames.clone().map(|f| sample(f[0]))),
            right: level(frames.map(|f| sample(f[1]))),
        }
    }
}

/// Everything `pw-record` is run with to capture `sink`'s monitor: raw interleaved stereo
/// `f32` at [`RATE`] to standard output, from the node `sink` names with the stream marked as
/// a capture of a sink, as passive, so the tap never keeps the sink awake or the graph busy (a
/// suspended sink gives it no samples, which the readers show as silence), and with no rate of
/// its own (`node.rate = 0`), so the graph's rate follows the player and only our stream is
/// resampled to [`RATE`]. `scripts/rate-check.sh` repeats the properties and measures them.
pub fn pw_record_args(sink: &str) -> Vec<String> {
    [
        "--raw",
        "--format",
        "f32",
        "--rate",
        "48000",
        "--channels",
        "2",
        "-P",
        "{ stream.capture.sink = true node.passive = true node.rate = 0 }",
        "--target",
        sink,
        "-",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// A capture of a sink's output.
#[async_trait]
pub trait Capture: Send {
    /// Capture `sink`'s monitor (a node name, as [`SinkState::target`](crate::SinkState)
    /// gives it), ending any capture of another sink first; already capturing `sink`, it goes
    /// on as it is. Whether a capture runs now: an empty name starts nothing.
    fn start(&mut self, sink: &str) -> bool;

    /// End the capture, if one runs.
    fn stop(&mut self);

    /// The sink being captured, if a capture runs.
    fn target(&self) -> Option<String>;

    /// The next block the capture reads. `None` once, when a running capture ended on its own
    /// (the sink went away, the tool stopped), after which nothing runs; while nothing runs it
    /// never finishes, so it can sit in a `select!` arm, and a block not taken yet is kept for
    /// the next call when the arm loses.
    async fn next_block(&mut self) -> Option<Block>;
}

/// A capture through `pw-record` that gives [`Block`]s, read on a thread of its own. Dropping it
/// ends the capture as [`Capture::stop`] does.
#[derive(Debug)]
pub struct PwRecord {
    recorder: Recorder<Block>,
}

impl PwRecord {
    /// A capture through the `pw-record` at `program` (a path, or a name looked up on `PATH`).
    /// Nothing runs until [`Capture::start`].
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            recorder: Recorder::new(program.into(), read_blocks),
        }
    }

    /// A capture through the `pw-record` [`PW_RECORD_ENV`] names, else `pw-record` on `PATH`;
    /// `None` when [`host_off`](crate::host_off) says host audio is off for this process.
    pub fn from_env() -> Option<Self> {
        program_from_env().map(Self::new)
    }
}

#[async_trait]
impl Capture for PwRecord {
    fn start(&mut self, sink: &str) -> bool {
        self.recorder.start(sink)
    }

    fn stop(&mut self) {
        self.recorder.stop();
    }

    fn target(&self) -> Option<String> {
        self.recorder.target()
    }

    async fn next_block(&mut self) -> Option<Block> {
        self.recorder.next().await
    }
}

/// A capture of a sink's output as analysed [`Frame`]s: the same recording as [`Capture`],
/// ending the same ways, but read through an [`Analyser`] and handed on [`FRAME_RATE`] times a
/// second.
#[async_trait]
pub trait FrameCapture: Send {
    /// Capture `sink`'s monitor, as [`Capture::start`] does.
    fn start(&mut self, sink: &str) -> bool;

    /// End the capture, if one runs.
    fn stop(&mut self);

    /// The sink being captured, if a capture runs.
    fn target(&self) -> Option<String>;

    /// The next frame the capture reads, with the same rules as [`Capture::next_block`]: `None`
    /// once when the capture ended on its own, never finishing while nothing runs, and a frame
    /// not taken yet kept for the next call when the arm of a `select!` loses.
    async fn next_frame(&mut self) -> Option<Frame>;
}

/// A capture through `pw-record` that gives [`Frame`]s, read and analysed on a thread of its own,
/// each frame stamped with the time it was made ([`unix_ms`]). Dropping it ends the capture as
/// [`FrameCapture::stop`] does.
#[derive(Debug)]
pub struct PwFrames {
    recorder: Recorder<Frame>,
}

impl PwFrames {
    /// A capture through the `pw-record` at `program` (a path, or a name looked up on `PATH`).
    /// Nothing runs until [`FrameCapture::start`].
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            recorder: Recorder::new(program.into(), read_frames),
        }
    }

    /// A capture through the `pw-record` [`PW_RECORD_ENV`] names, else `pw-record` on `PATH`;
    /// `None` when [`host_off`](crate::host_off) says host audio is off for this process.
    pub fn from_env() -> Option<Self> {
        program_from_env().map(Self::new)
    }
}

#[async_trait]
impl FrameCapture for PwFrames {
    fn start(&mut self, sink: &str) -> bool {
        self.recorder.start(sink)
    }

    fn stop(&mut self) {
        self.recorder.stop();
    }

    fn target(&self) -> Option<String> {
        self.recorder.target()
    }

    async fn next_frame(&mut self) -> Option<Frame> {
        self.recorder.next().await
    }
}

/// The `pw-record` [`PW_RECORD_ENV`] names, else `pw-record`; `None` when host audio is off.
fn program_from_env() -> Option<OsString> {
    if crate::host_off() {
        return None;
    }
    Some(
        std::env::var_os(PW_RECORD_ENV)
            .filter(|named| !named.is_empty())
            .unwrap_or_else(|| "pw-record".into()),
    )
}

/// What a reading thread does with `pw-record`'s output: turn it into `T`s and send them on
/// until the output ends or nobody reads them.
type Reader<T> = fn(ChildStdout, &mpsc::Sender<T>);

/// The `pw-record` process and its reading thread, whatever the thread makes of the output.
#[derive(Debug)]
struct Recorder<T> {
    /// The `pw-record` to run.
    program: OsString,
    /// What the thread started with each process does.
    read: Reader<T>,
    /// The capture running now, if one is.
    running: Option<Running<T>>,
}

/// One running `pw-record`: ended when this is let go of.
#[derive(Debug)]
struct Running<T> {
    /// The sink it captures.
    target: String,
    /// The process, killed and reaped when this is dropped.
    child: Child,
    /// What the reading thread made of its output.
    items: mpsc::Receiver<T>,
}

impl<T> Drop for Running<T> {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl<T: Send + 'static> Recorder<T> {
    fn new(program: OsString, read: Reader<T>) -> Self {
        Self {
            program,
            read,
            running: None,
        }
    }

    fn spawn(&self, sink: &str) -> Option<Running<T>> {
        let mut child = Command::new(&self.program)
            .args(pw_record_args(sink))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let output = child.stdout.take()?;
        let (sender, items) = mpsc::channel(QUEUE);
        let running = Running {
            target: sink.to_string(),
            child,
            items,
        };
        // The thread ends when the output does (the process was killed or stopped) or when
        // nobody reads what it makes any more; a thread that cannot start leaves the process to
        // be killed with `running`.
        let read = self.read;
        std::thread::Builder::new()
            .name("svolder-capture".to_string())
            .spawn(move || read(output, &sender))
            .ok()?;
        Some(running)
    }

    fn start(&mut self, sink: &str) -> bool {
        if sink.trim().is_empty() {
            self.stop();
            return false;
        }
        if self.running.as_ref().is_some_and(|r| r.target == sink) {
            return true;
        }
        self.stop();
        self.running = self.spawn(sink);
        self.running.is_some()
    }

    fn stop(&mut self) {
        self.running = None;
    }

    fn target(&self) -> Option<String> {
        self.running.as_ref().map(|r| r.target.clone())
    }

    async fn next(&mut self) -> Option<T> {
        let Some(running) = self.running.as_mut() else {
            return std::future::pending().await;
        };
        let item = running.items.recv().await;
        if item.is_none() {
            self.running = None;
        }
        item
    }
}

/// Read `output` a block at a time and send each block's [`Block`] on, until the output ends
/// or nobody reads the blocks.
fn read_blocks(mut output: impl Read, blocks: &mpsc::Sender<Block>) {
    let mut spectrum = Spectrum::new(RATE, FRAMES);
    let mut bytes = vec![0u8; FRAMES * CHANNELS * size_of::<f32>()];
    let mut samples = vec![0.0f32; FRAMES * CHANNELS];
    while output.read_exact(&mut bytes).is_ok() {
        for (sample, raw) in samples.iter_mut().zip(bytes.chunks_exact(size_of::<f32>())) {
            *sample = f32::from_ne_bytes(raw.try_into().expect("four bytes to a sample"));
        }
        match blocks.try_send(Block::of(&mut spectrum, &samples)) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Closed(_)) => return,
        }
    }
}

/// Read `output` through an [`Analyser`] and send each [`Frame`] it completes on, stamped with
/// [`unix_ms`], until the output ends or nobody reads the frames. It reads a frame's worth of
/// samples at a time, so a frame is stamped within a frame of its last sample.
fn read_frames(mut output: impl Read, frames: &mpsc::Sender<Frame>) {
    let mut analyser = Analyser::new(RATE);
    let step = (RATE / FRAME_RATE) as usize;
    let mut bytes = vec![0u8; step * CHANNELS * size_of::<f32>()];
    let mut samples = vec![0.0f32; step * CHANNELS];
    while output.read_exact(&mut bytes).is_ok() {
        for (sample, raw) in samples.iter_mut().zip(bytes.chunks_exact(size_of::<f32>())) {
            *sample = f32::from_ne_bytes(raw.try_into().expect("four bytes to a sample"));
        }
        for mut frame in analyser.push(&samples) {
            frame.capture_ms = unix_ms();
            match frames.try_send(frame) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Closed(_)) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::{band_edges, scale};

    /// Interleaved stereo: a sine at `frequency` on both channels, `left` and `right` its
    /// amplitude on each.
    fn stereo(frequency: f32, left: f32, right: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|n| {
                let s = (2.0 * std::f32::consts::PI * frequency * n as f32 / RATE as f32).sin();
                [left * s, right * s]
            })
            .collect()
    }

    #[test]
    fn pw_record_captures_the_sinks_monitor_and_nothing_else() {
        assert_eq!(
            pw_record_args("alsa_output.usb-Schiit_Audio_Modi_-00.analog-stereo"),
            [
                "--raw",
                "--format",
                "f32",
                "--rate",
                "48000",
                "--channels",
                "2",
                "-P",
                "{ stream.capture.sink = true node.passive = true node.rate = 0 }",
                "--target",
                "alsa_output.usb-Schiit_Audio_Modi_-00.analog-stereo",
                "-",
            ]
        );
    }

    #[test]
    fn the_capture_is_passive_so_it_never_keeps_the_sink_awake() {
        let args = pw_record_args("hdmi-out");
        let at = args.iter().position(|a| a == "-P").expect("-P");
        assert!(args[at + 1].contains("node.passive = true"), "{args:?}");
        // Without it pw-record suggests its own rate and the graph starts at 48 kHz.
        assert!(args[at + 1].contains("node.rate = 0"), "{args:?}");
        // The rate stays the analyser's own; only our stream is resampled.
        let rate = args.iter().position(|a| a == "--rate").expect("--rate");
        assert_eq!(args[rate + 1], RATE.to_string());
    }

    #[test]
    fn the_rate_check_script_runs_the_capture_with_the_same_arguments() {
        let script =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/rate-check.sh");
        let script = std::fs::read_to_string(script).expect("scripts/rate-check.sh");
        let args = pw_record_args("hdmi-out");
        let at = args.iter().position(|a| a == "-P").expect("-P");
        assert!(
            script.contains(&format!("CAPTURE_PROPS='{}'", args[at + 1])),
            "{}",
            args[at + 1]
        );
        // The whole list, the properties and the sink as the script's variables, on the one
        // `pw-record` line (its continuation lines joined).
        let line: Vec<String> = args
            .iter()
            .enumerate()
            .map(|(i, a)| match i {
                i if i == at + 1 => "\"$CAPTURE_PROPS\"".to_string(),
                i if a == "hdmi-out" && args[i - 1] == "--target" => "\"$sink\"".to_string(),
                _ => a.clone(),
            })
            .collect();
        let script = script.replace("\\\n", " ");
        let script = script.split_whitespace().collect::<Vec<_>>().join(" ");
        let want = format!("pw-record {}", line.join(" "));
        assert!(script.contains(&want), "{want}");
    }

    #[test]
    fn the_capture_sink_property_and_the_target_go_together() {
        let args = pw_record_args("hdmi-out");
        let after = |flag: &str| {
            let at = args.iter().position(|a| a == flag).expect(flag);
            args[at + 1].as_str()
        };
        assert_eq!(
            after("-P"),
            "{ stream.capture.sink = true node.passive = true node.rate = 0 }"
        );
        assert_eq!(after("--target"), "hdmi-out");
        // The node is named, never left to PipeWire's default, which for a capture is a source.
        assert!(
            args.iter()
                .all(|a| !a.contains("SOURCE") && !a.contains("auto")),
            "{args:?}"
        );
        assert_eq!(args.iter().filter(|a| *a == "--target").count(), 1);
        assert_eq!(args.last().map(String::as_str), Some("-"));
    }

    #[test]
    fn a_block_levels_each_channel_and_draws_their_mix() {
        let mut spectrum = Spectrum::new(RATE, FRAMES);
        let block = Block::of(&mut spectrum, &stereo(1_000.0, 0.5, 0.25, FRAMES));
        // A sine's RMS is its amplitude over √2.
        assert!(
            (block.left - scale(0.5 / 2f32.sqrt())).abs() < 0.01,
            "{block:?}"
        );
        assert!(
            (block.right - scale(0.25 / 2f32.sqrt())).abs() < 0.01,
            "{block:?}"
        );
        let edges = band_edges();
        let loudest = (0..BANDS)
            .max_by(|&a, &b| block.bands[a].total_cmp(&block.bands[b]))
            .unwrap();
        assert!(edges[loudest] <= 1_000.0 && 1_000.0 < edges[loudest + 1]);
    }

    #[test]
    fn a_block_of_silence_is_empty() {
        let mut spectrum = Spectrum::new(RATE, FRAMES);
        let silent = Block {
            bands: [0.0; BANDS],
            left: 0.0,
            right: 0.0,
        };
        assert_eq!(
            Block::of(&mut spectrum, &vec![0.0; FRAMES * CHANNELS]),
            silent
        );
    }

    #[test]
    fn a_sample_that_is_not_a_number_counts_as_silence_and_spoils_nothing_else() {
        let mut spectrum = Spectrum::new(RATE, FRAMES);
        let clean = stereo(1_000.0, 0.5, 0.5, FRAMES);
        let mut spoiled = clean.clone();
        spoiled[100] = f32::NAN;
        spoiled[301] = f32::INFINITY;
        let clean = Block::of(&mut spectrum, &clean);
        let spoiled = Block::of(&mut spectrum, &spoiled);
        assert!((spoiled.left - clean.left).abs() < 0.01, "{spoiled:?}");
        assert!((spoiled.right - clean.right).abs() < 0.01, "{spoiled:?}");
        for (a, b) in spoiled.bands.iter().zip(clean.bands) {
            assert!((a - b).abs() < 0.05, "{spoiled:?} against {clean:?}");
        }
    }

    #[test]
    fn a_block_is_read_from_raw_f32_and_sent_on() {
        let samples = stereo(440.0, 0.5, 0.5, FRAMES * 2);
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_ne_bytes()).collect();
        let (sender, mut blocks) = mpsc::channel(QUEUE);
        read_blocks(bytes.as_slice(), &sender);
        let mut spectrum = Spectrum::new(RATE, FRAMES);
        let first = Block::of(&mut spectrum, &samples[..FRAMES * CHANNELS]);
        assert_eq!(blocks.try_recv().ok(), Some(first));
        assert!(blocks.try_recv().is_ok());
        assert!(
            blocks.try_recv().is_err(),
            "two blocks' worth makes two blocks"
        );
    }

    #[test]
    fn frames_are_read_from_raw_f32_analysed_and_stamped() {
        // Two seconds of a 1 kHz sine, panned left.
        let samples = stereo(1_000.0, 0.5, 0.0, RATE as usize * 2);
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_ne_bytes()).collect();
        let (sender, mut frames) = mpsc::channel(QUEUE * 8);
        let before = unix_ms();
        read_frames(bytes.as_slice(), &sender);
        let mut taken = Vec::new();
        while let Ok(frame) = frames.try_recv() {
            taken.push(frame);
        }
        assert_eq!(taken.len(), 60);
        let last = taken.last().unwrap();
        assert!(last.capture_ms >= before && last.capture_ms <= unix_ms());
        let band = (0..240).max_by_key(|&b| last.left[b]).unwrap();
        let edges = crate::analysis::band_edges();
        assert!(edges[band] <= 1_000.0 && 1_000.0 < edges[band + 1]);
        assert!(last.right.iter().all(|&b| b == 0));
    }
}
