// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A transport whose adapter can be changed while it runs.
//!
//! Everything that listens or connects holds one `Arc<dyn Transport>`; wrapping the real
//! transport in a [`SwitchableTransport`] lets the daemon point all of them at another adapter by
//! swapping what it wraps, with no one holding a stale handle. A link opened before the swap
//! stays on the adapter it was opened on until its owner closes it.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use svolder_proto::UnitId;

use crate::advert::Advertisement;
use crate::error::BleError;
use crate::transport::{Advertisements, Link, Listening, Transport};

/// What the wrapper is pointed at now.
struct Current {
    transport: Arc<dyn Transport>,
    /// The adapter's name, kept here because [`Transport::adapter`] lends a `&str` that must
    /// outlive the lock; one short string is leaked for each swap, and swaps are rare.
    name: Option<&'static str>,
}

/// A [`Transport`] that forwards to another, which [`SwitchableTransport::swap`] replaces.
pub struct SwitchableTransport {
    current: RwLock<Current>,
}

fn current_of(transport: Arc<dyn Transport>) -> Current {
    let name = transport
        .adapter()
        .map(|name| &*Box::leak(name.to_string().into_boxed_str()));
    Current { transport, name }
}

impl SwitchableTransport {
    /// Wrap `inner`.
    pub fn new(inner: Arc<dyn Transport>) -> SwitchableTransport {
        SwitchableTransport {
            current: RwLock::new(current_of(inner)),
        }
    }

    /// Point everything that uses this transport at `inner` from now on.
    pub fn swap(&self, inner: Arc<dyn Transport>) {
        let next = current_of(inner);
        *self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
    }

    /// The transport in force now.
    fn inner(&self) -> Arc<dyn Transport> {
        Arc::clone(
            &self
                .current
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .transport,
        )
    }
}

#[async_trait]
impl Transport for SwitchableTransport {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.inner().scan(window).await
    }

    async fn held(&self) -> Result<Advertisements, BleError> {
        self.inner().held().await
    }

    async fn sight(
        &self,
        unit: UnitId,
        window: Duration,
    ) -> Result<Option<Advertisement>, BleError> {
        self.inner().sight(unit, window).await
    }

    async fn listen(&self, window: Duration, listening: &dyn Listening) -> Result<(), BleError> {
        self.inner().listen(window, listening).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        self.inner().connect(unit).await
    }

    fn adapter(&self) -> Option<&str> {
        self.current
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .name
    }

    async fn settle(&self, bound: Duration) {
        self.inner().settle(bound).await;
    }

    async fn settle_for(&self, unit: UnitId, bound: Duration) {
        self.inner().settle_for(unit, bound).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transport that only names its adapter.
    struct Named(&'static str);

    #[async_trait]
    impl Transport for Named {
        async fn scan(&self, _: Duration) -> Result<Advertisements, BleError> {
            Err(BleError::NotFound)
        }
        async fn connect(&self, _: UnitId) -> Result<Box<dyn Link>, BleError> {
            Err(BleError::NotFound)
        }
        fn adapter(&self) -> Option<&str> {
            Some(self.0)
        }
    }

    /// Swapping changes the adapter every holder of the wrapper sees, and what it forwards to.
    #[tokio::test]
    async fn a_swap_changes_the_adapter_every_holder_sees() {
        let wrapper = Arc::new(SwitchableTransport::new(Arc::new(Named("hci0"))));
        let held: Arc<dyn Transport> = wrapper.clone();
        assert_eq!(held.adapter(), Some("hci0"));
        wrapper.swap(Arc::new(Named("hci2")));
        assert_eq!(held.adapter(), Some("hci2"));
    }
}
