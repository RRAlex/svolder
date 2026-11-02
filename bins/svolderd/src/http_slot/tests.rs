// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// A server that counts the stops it gets.
struct Fake {
    id: usize,
    stops: Arc<AtomicUsize>,
}

impl Server for Fake {
    type Config = usize;
    type Status = ();
    const NAME: &'static str = "fake";

    fn off() -> Self::Status {}
    fn config(&self) -> &usize {
        &self.id
    }
    fn serving(&self) -> bool {
        true
    }
    fn status(&self) -> Self::Status {}
    fn token_info(&self) -> Option<svolder_ipc::TokenInfo> {
        None
    }
    fn refresher(&self) -> Option<Refresher> {
        None
    }
    async fn stop(self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn starting_over_a_running_server_stops_it_first() {
    let slot: Slot<Fake> = Slot::default();
    let stops = Arc::new(AtomicUsize::new(0));
    for id in [1, 2] {
        let stops = stops.clone();
        slot.start(|| async move { Some(Fake { id, stops }) }).await;
    }
    assert_eq!(
        stops.load(Ordering::SeqCst),
        1,
        "the first server was left running"
    );
    assert_eq!(slot.with(|s| s.map(|s| s.id)), Some(2));
}
