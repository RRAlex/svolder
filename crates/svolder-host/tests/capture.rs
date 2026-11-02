// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The capture against a fake `pw-record`: a script in the case's own directory that logs the
//! arguments it was run with and its pid, then writes a sine the case prepared as raw `f32`.
//! Nothing here runs the real `pw-record`.

mod common;

use std::time::{Duration, Instant};

use common::{TempDir, install_script, lines};
use svolder_host::analysis;
use svolder_host::capture::{
    CHANNELS, Capture, FRAMES, FrameCapture, PwFrames, PwRecord, RATE, pw_record_args,
};
use svolder_host::fft::{BANDS, band_edges, scale};

/// A `pw-record` that logs its arguments one per line to `argv`, one line per run to `runs`
/// and then its pid to `pid`, writes `sine.raw` to its output, and then, unless `ends` is there,
/// holds its output open as a live capture does.
const FAKE_PW_RECORD: &str = r#"#!/bin/sh
here=$(dirname "$0")
printf '%s\n' "$@" > "$here/argv"
echo run >> "$here/runs"
echo $$ > "$here/pid"
cat "$here/sine.raw"
[ -e "$here/ends" ] && exit 0
exec sleep 30
"#;

/// How long a case waits for a block or for the fake to end.
const WAIT: Duration = Duration::from_secs(5);

/// A fake `pw-record` in a directory of its own, `blocks` blocks of a 1 kHz sine ready for it
/// to write: amplitude 0.5 on the left, 0.25 on the right.
fn fake(label: &str, blocks: usize) -> (TempDir, PwRecord) {
    let dir = install(label, blocks);
    let capture = PwRecord::new(dir.join("pw-record"));
    (dir, capture)
}

/// The directory of [`fake`], without a capture on it.
fn install(label: &str, blocks: usize) -> TempDir {
    let dir = TempDir::new(label);
    install_script(&dir.join("pw-record"), FAKE_PW_RECORD);
    let bytes: Vec<u8> = (0..FRAMES * blocks)
        .flat_map(|n| {
            let s = (2.0 * std::f32::consts::PI * 1_000.0 * n as f32 / RATE as f32).sin();
            [0.5 * s, 0.25 * s]
        })
        .flat_map(f32::to_ne_bytes)
        .collect();
    assert_eq!(bytes.len(), FRAMES * CHANNELS * 4 * blocks);
    std::fs::write(dir.join("sine.raw"), bytes).unwrap();
    dir
}

/// The fake's pid, once it has written it.
fn pid(dir: &TempDir) -> u32 {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(pid) = std::fs::read_to_string(dir.join("pid"))
            && let Ok(pid) = pid.trim().parse()
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "the fake never wrote its pid");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Whether the process `pid` is still running (a zombie is not).
fn running(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(") ")
                .and_then(|(_, rest)| rest.chars().next())
        })
        .is_some_and(|state| state != 'Z' && state != 'X')
}

async fn next(capture: &mut PwRecord) -> Option<svolder_host::Block> {
    tokio::time::timeout(WAIT, capture.next_block())
        .await
        .expect("the capture gave a block or ended in time")
}

#[tokio::test]
async fn a_capture_runs_pw_record_on_the_sinks_monitor_and_draws_what_it_plays() {
    let (dir, mut capture) = fake("capture-runs", 4);
    let sink = "alsa_output.usb-Schiit_Audio_Modi_-00.analog-stereo";
    assert!(capture.start(sink));
    assert_eq!(capture.target().as_deref(), Some(sink));
    let block = next(&mut capture).await.expect("a block of the sine");
    assert_eq!(lines(&dir.join("argv")), pw_record_args(sink));

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
    assert!(
        edges[loudest] <= 1_000.0 && 1_000.0 < edges[loudest + 1],
        "{block:?}"
    );
}

#[tokio::test]
async fn stopping_the_capture_ends_pw_record() {
    let (dir, mut capture) = fake("capture-stop", 2);
    assert!(capture.start("hdmi-out"));
    next(&mut capture).await.expect("a block");
    let pid = pid(&dir);
    assert!(running(pid));
    let stopping = Instant::now();
    capture.stop();
    assert!(
        stopping.elapsed() < WAIT,
        "stop waited {:?}",
        stopping.elapsed()
    );
    assert!(!running(pid), "pw-record is still running after stop");
    assert_eq!(capture.target(), None);
    // Nothing runs, so no block ever comes.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), capture.next_block())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn dropping_the_capture_ends_pw_record() {
    let (dir, mut capture) = fake("capture-drop", 2);
    assert!(capture.start("hdmi-out"));
    next(&mut capture).await.expect("a block");
    let pid = pid(&dir);
    assert!(running(pid));
    let dropping = Instant::now();
    drop(capture);
    assert!(
        dropping.elapsed() < WAIT,
        "drop waited {:?}",
        dropping.elapsed()
    );
    assert!(!running(pid), "pw-record is still running after drop");
}

#[tokio::test]
async fn without_a_sink_name_nothing_is_captured() {
    let (dir, mut capture) = fake("capture-no-sink", 2);
    assert!(!capture.start(""));
    assert!(!capture.start("  "));
    assert_eq!(capture.target(), None);
    assert!(lines(&dir.join("runs")).is_empty(), "pw-record ran");
}

#[tokio::test]
async fn a_missing_tool_captures_nothing() {
    let dir = TempDir::new("capture-missing");
    let mut capture = PwRecord::new(dir.join("no-pw-record"));
    assert!(!capture.start("hdmi-out"));
    assert_eq!(capture.target(), None);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), capture.next_block())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_capture_that_ends_on_its_own_says_so_once_and_then_runs_nothing() {
    let (dir, mut capture) = fake("capture-ends", 3);
    std::fs::write(dir.join("ends"), "").unwrap();
    assert!(capture.start("hdmi-out"));
    for _ in 0..3 {
        assert!(next(&mut capture).await.is_some());
    }
    assert_eq!(next(&mut capture).await, None);
    assert_eq!(capture.target(), None);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), capture.next_block())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_same_sink_goes_on_and_another_one_replaces_it() {
    let (dir, mut capture) = fake("capture-switch", 2);
    assert!(capture.start("hdmi-out"));
    let first = pid(&dir);
    assert!(capture.start("hdmi-out"));
    assert!(running(first), "the capture of the same sink was ended");
    assert_eq!(lines(&dir.join("runs")).len(), 1, "the same sink ran twice");
    std::fs::remove_file(dir.join("pid")).unwrap();
    assert!(capture.start("alsa_output.usb-dac"));
    let second = pid(&dir);
    assert_ne!(first, second);
    assert!(!running(first), "the first capture is still running");
    assert!(running(second));
    assert_eq!(capture.target().as_deref(), Some("alsa_output.usb-dac"));
    assert_eq!(
        lines(&dir.join("argv")),
        pw_record_args("alsa_output.usb-dac")
    );
}

async fn next_frame(capture: &mut PwFrames) -> Option<analysis::Frame> {
    tokio::time::timeout(WAIT, capture.next_frame())
        .await
        .expect("the capture gave a frame or ended in time")
}

#[tokio::test]
async fn a_frames_capture_runs_pw_record_and_analyses_what_it_plays() {
    // 60 blocks of 1024 frames: 38 frames of 1600, written at once. The queue holds 8 and drops
    // the newer ones when it is full, so only the first 8 frames are certain to arrive, in order.
    let dir = install("frames-run", 60);
    let mut capture = PwFrames::new(dir.join("pw-record"));
    let sink = "alsa_output.usb-Schiit_Audio_Modi_-00.analog-stereo";
    assert!(capture.start(sink));
    assert_eq!(capture.target().as_deref(), Some(sink));
    let before = svolder_host::unix_ms();
    let mut taken = Vec::new();
    for _ in 0..8 {
        taken.push(next_frame(&mut capture).await.expect("a frame of the sine"));
    }
    assert_eq!(lines(&dir.join("argv")), pw_record_args(sink));
    let stamps: Vec<u64> = taken.iter().map(|f| f.capture_ms).collect();
    assert!(stamps.windows(2).all(|w| w[0] <= w[1]), "{stamps:?}");
    assert!(
        stamps[0] >= before && stamps[7] <= svolder_host::unix_ms(),
        "{stamps:?} outside {before}.."
    );
    // The eighth frame's window is full of the sine.
    let frame = taken[7];
    let edges = analysis::band_edges();
    let loudest = (0..analysis::BANDS)
        .max_by_key(|&band| frame.left[band])
        .unwrap();
    assert!(edges[loudest] <= 1_000.0 && 1_000.0 < edges[loudest + 1]);
    // The right channel is half the left's amplitude: 6 dB, 21 of the 255 steps.
    let (l, r) = (frame.left[loudest], frame.right[loudest]);
    assert!(
        (i32::from(l) - i32::from(r) - 21).abs() <= 6,
        "{l} against {r}"
    );
    assert!(frame.levels[0] > frame.levels[1] && frame.levels[1] > 0);

    let pid = pid(&dir);
    assert!(running(pid));
    capture.stop();
    assert!(!running(pid), "pw-record is still running after stop");
    assert_eq!(capture.target(), None);
}

#[tokio::test]
async fn a_frames_capture_that_ends_says_so_once() {
    let dir = install("frames-ends", 10);
    std::fs::write(dir.join("ends"), "").unwrap();
    let mut capture = PwFrames::new(dir.join("pw-record"));
    assert!(!capture.start(""));
    assert!(capture.start("hdmi-out"));
    // 10 blocks of 1024 frames: 6 whole frames of 1600.
    for _ in 0..6 {
        assert!(next_frame(&mut capture).await.is_some());
    }
    assert!(next_frame(&mut capture).await.is_none());
    assert_eq!(capture.target(), None);
    let mut missing = PwFrames::new(dir.join("no-pw-record"));
    assert!(!missing.start("hdmi-out"));
}
