// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Building the transport the daemon talks through: a real BlueZ adapter, or the
//! in-process fake the tests drive.

use std::sync::Arc;

use svolder_ble::{FakeTransport, Transport};
use svolder_profiles::Registry;

/// Build the transport named by `--transport`. `fake` is the in-process device the CLI's
/// `--transport fake` also builds, from the same `SVOLDER_FAKE_*` environment
/// ([`FakeTransport::transport_from_env`]), and with `SVOLDER_FAKE_MORE` several; `ble` is a
/// real adapter via BlueZ, with `adapter` naming the controller.
pub async fn make(
    name: &str,
    adapter: Option<&str>,
    registry: &Registry,
) -> Result<Arc<dyn Transport>, String> {
    match name {
        "fake" => FakeTransport::transport_from_env(registry),
        "ble" => svolder_ble::BluezTransport::new(adapter)
            .await
            .map(|t| Arc::new(t) as Arc<dyn Transport>)
            .map_err(|e| e.to_string()),
        other => Err(format!("unknown transport {other:?}; use `fake` or `ble`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_unknown_transport_name_is_refused() {
        let registry = Registry::builtin().unwrap();
        let err = make("carrier-pigeon", None, &registry)
            .await
            .err()
            .expect("an unknown transport name is refused");
        assert!(err.contains("carrier-pigeon"), "got {err}");
    }
}
