// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A frame tap: a `Transport` decorator that records every frame a session sends and every
//! notification it hears, as the bytes crossed the link, in order and with the time.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::StreamExt;
use svolder_proto::UnitId;
use tokio::time::Instant;

use crate::advert::Advertisement;
use crate::error::BleError;
use crate::transport::{Advertisements, Link, Listening, Notifications, Transport};

/// Which way a tapped frame went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameDirection {
    /// Written by the host to the device.
    Sent,
    /// Notified by the device to the host.
    Received,
}

/// One frame the tap saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TappedFrame {
    /// Time since the tap was made.
    pub at: Duration,
    /// Which way the frame went.
    pub direction: FrameDirection,
    /// The frame's bytes, as they crossed the link, decoded or not.
    pub bytes: Vec<u8>,
}

/// The record of a tap, shared by every clone: hand one clone to a [`TappedTransport`] and keep
/// another to read what it saw.
#[derive(Clone)]
pub struct FrameTap {
    frames: Arc<Mutex<Vec<TappedFrame>>>,
    start: Instant,
}

impl FrameTap {
    /// An empty tap; the times it records count from now.
    pub fn new() -> FrameTap {
        FrameTap {
            frames: Arc::default(),
            start: Instant::now(),
        }
    }

    /// How long ago the tap was made: the clock the frames' times are on.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// A copy of every frame seen so far, in the order they were seen.
    pub fn frames(&self) -> Vec<TappedFrame> {
        self.frames.lock().expect("tap mutex").clone()
    }

    /// Note one frame.
    fn record(&self, direction: FrameDirection, bytes: &[u8]) {
        // The time is read under the lock, so the order in the vector is the order of the times.
        let mut frames = self.frames.lock().expect("tap mutex");
        frames.push(TappedFrame {
            at: self.start.elapsed(),
            direction,
            bytes: bytes.to_vec(),
        });
    }
}

impl Default for FrameTap {
    fn default() -> FrameTap {
        FrameTap::new()
    }
}

/// A transport that does everything its inner transport does and records the frames of every
/// link it opens in a [`FrameTap`].
pub struct TappedTransport {
    inner: Arc<dyn Transport>,
    tap: FrameTap,
}

impl TappedTransport {
    /// Wrap `inner`; every link it opens records into `tap`.
    pub fn new(inner: Arc<dyn Transport>, tap: FrameTap) -> TappedTransport {
        TappedTransport { inner, tap }
    }
}

#[async_trait]
impl Transport for TappedTransport {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn held(&self) -> Result<Advertisements, BleError> {
        self.inner.held().await
    }

    async fn sight(
        &self,
        unit: UnitId,
        window: Duration,
    ) -> Result<Option<Advertisement>, BleError> {
        self.inner.sight(unit, window).await
    }

    async fn listen(&self, window: Duration, listening: &dyn Listening) -> Result<(), BleError> {
        self.inner.listen(window, listening).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        let inner = self.inner.connect(unit).await?;
        Ok(Box::new(TappedLink {
            inner,
            tap: self.tap.clone(),
        }))
    }

    fn adapter(&self) -> Option<&str> {
        self.inner.adapter()
    }

    async fn settle(&self, bound: Duration) {
        self.inner.settle(bound).await;
    }

    async fn settle_for(&self, unit: UnitId, bound: Duration) {
        self.inner.settle_for(unit, bound).await;
    }
}

/// A link that records each write before forwarding it and each notification as it is read.
struct TappedLink {
    inner: Box<dyn Link>,
    tap: FrameTap,
}

#[async_trait]
impl Link for TappedLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        self.tap.record(FrameDirection::Sent, frame);
        self.inner.write(frame).await
    }

    fn notifications(&self) -> Notifications {
        let tap = self.tap.clone();
        self.inner
            .notifications()
            .inspect(move |frame| tap.record(FrameDirection::Received, frame))
            .boxed()
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.inner.disconnect().await
    }
}

#[cfg(test)]
mod tests;
