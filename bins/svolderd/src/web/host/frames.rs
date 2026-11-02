// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The frames stream: `GET api/v1/host/frames`, the daemon's analysis of what the computer plays
//! as server-sent events, for the page's visualizer.
//!
//! It opens with `retry: 2000` and a `hello` event: `{"now_ms", "capture", "sink", "why"}`,
//! the daemon's clock (the page's offset to it comes from here, taken again on each reconnect;
//! each frame's `t` gives the delay behind it, not a correction of the offset) and where the
//! capture stands (`capture` is `starting`, `running` or `unavailable`;
//! `sink` is the captured sink's name while it runs and `why` the reason while it is
//! unavailable, each null otherwise). Then each change of the capture is a `capture` event of
//! the same shape without the clock, and while it runs every frame is a `frame` event:
//!
//! ```text
//! {"t": 1700000000123, "l": "<base64 of 240 bytes>", "r": "<base64 of 240 bytes>",
//!  "lv": [rmsL, rmsR, peakL, peakR], "beat": 0, "bpm": 120, "phase": 77}
//! ```
//!
//! `t` is the capture's unix milliseconds on the daemon's clock, `l` and `r` the bands of the
//! channels from 20 Hz up (a byte maps dB over a range that is a tuning, not a promise), `bpm`
//! a number or null and `phase` 0–255 along the beat. A keep-alive comment goes out every 3 s, so a
//! connection that is gone is noticed within that even while no frame flows.
//!
//! The stream holds the daemon's capture for as long as it is open: the capture starts with
//! the first and stops 2 s after the last. It is read as the device stream is: a guest may,
//! and a rotated token ends it. A page that reads slowly loses the frames it is too late for,
//! never the capture's pace. With no host (off) the route is a 404.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use hyper::{HeaderMap, StatusCode};
use svolder_host::{Frame, unix_ms};
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;

use crate::host::{CaptureState, FramesEvent};
use crate::web::Web;
use crate::web::api::reader;
use crate::web::router::error;

/// How many events wait for the connection before newer frames are dropped.
const QUEUE: usize = 8;

/// How often a comment keeps the stream open, and shows a connection that is gone: short, as
/// while the capture is unavailable no frame is written, and the stream holds the capture.
const KEEP_ALIVE: Duration = Duration::from_secs(3);

/// The frames stream, for a viewer who may read the page.
pub(super) async fn frames(State(web): State<Web>, headers: HeaderMap) -> Response {
    // Subscribed before the cookie is looked at: a token rotated between the two ends the
    // stream, instead of slipping past a check that read the old one.
    let signed_out = web.signed_out.subscribe();
    if let Err(refused) = reader(&web, &headers) {
        return refused;
    }
    let Some(events) = web.fleet.host_frames() else {
        return error(StatusCode::NOT_FOUND, "host audio is not available");
    };
    let (tx, rx) = mpsc::channel(QUEUE);
    tokio::spawn(follow(web.clone(), events, signed_out, tx));
    Sse::new(ReceiverStream::new(rx).map(Ok::<_, Infallible>))
        .keep_alive(KeepAlive::new().interval(KEEP_ALIVE).text(""))
        .into_response()
}

/// Send the events until the page goes or a rotated token signs it out; the stream, and with it
/// the hold on the capture, is dropped when this ends. A frame the page has no room for is
/// dropped; the capture's state is waited for, but never past a sign-out. It keeps `web`, which
/// the `signed_out` channel belongs to, so the channel does not close under it.
async fn follow(
    _web: Web,
    mut events: crate::fleet::FramesWatch,
    mut signed_out: watch::Receiver<u64>,
    tx: mpsc::Sender<Event>,
) {
    let hello = Event::default()
        .retry(Duration::from_secs(2))
        .comment("svolder");
    if !send(&tx, &mut signed_out, hello).await {
        return;
    }
    let mut first = true;
    loop {
        let event = tokio::select! {
            biased;
            _ = signed_out.changed() => return,
            () = tx.closed() => return,
            event = events.next() => match event {
                Some(event) => event,
                None => return,
            },
        };
        let sent = match event {
            FramesEvent::Capture(state) => {
                let (name, clock) = match std::mem::take(&mut first) {
                    true => ("hello", Some(unix_ms())),
                    false => ("capture", None),
                };
                let event = Event::default()
                    .event(name)
                    .data(capture_json(&state, clock));
                send(&tx, &mut signed_out, event).await
            }
            // A frame the page has no room for is dropped before it is encoded.
            FramesEvent::Frame(_) if tx.capacity() == 0 => true,
            FramesEvent::Frame(frame) => {
                let event = Event::default().event("frame").data(frame_json(&frame));
                !matches!(
                    tx.try_send(event),
                    Err(mpsc::error::TrySendError::Closed(_))
                )
            }
        };
        if !sent {
            return;
        }
    }
}

/// Send `event`, waiting for room as long as the page is there and signed in: `false` when the
/// page has gone or a rotated token signed it out, so a client that stopped reading cannot hold
/// the stream, and the capture with it, past either.
async fn send(
    tx: &mpsc::Sender<Event>,
    signed_out: &mut watch::Receiver<u64>,
    event: Event,
) -> bool {
    tokio::select! {
        biased;
        _ = signed_out.changed() => false,
        sent = tx.send(event) => sent.is_ok(),
    }
}

/// The capture's state as the page reads it, with the daemon's clock for `hello`.
fn capture_json(state: &CaptureState, now_ms: Option<u64>) -> String {
    let view = state.view();
    let mut json = serde_json::Map::new();
    if let Some(now) = now_ms {
        json.insert("now_ms".into(), now.into());
    }
    json.insert(
        "capture".into(),
        match view.state {
            svolder_ipc::CaptureKind::Starting => "starting",
            svolder_ipc::CaptureKind::Running => "running",
            svolder_ipc::CaptureKind::Unavailable => "unavailable",
        }
        .into(),
    );
    json.insert("sink".into(), view.sink.into());
    json.insert("why".into(), view.why.into());
    serde_json::Value::Object(json).to_string()
}

/// A frame as the page reads it.
fn frame_json(frame: &Frame) -> String {
    serde_json::json!({
        "t": frame.capture_ms,
        "l": base64(&frame.left),
        "r": base64(&frame.right),
        "lv": frame.levels,
        "beat": u8::from(frame.beat),
        "bpm": frame.bpm,
        "phase": frame.phase,
    })
    .to_string()
}

/// `bytes` in standard base64 (RFC 4648, with padding), which `atob` reads on the page.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        let sextet = |at: u32| char::from(ALPHABET[(group >> at & 63) as usize]);
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests;
