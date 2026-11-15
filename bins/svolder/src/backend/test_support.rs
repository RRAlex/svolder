// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Test constructors of [`Backend`]: backends over the in-process fake device or a stub daemon.

use super::*;

impl Backend {
    /// A backend over the in-process fake device, connected and refreshed, beside the gate
    /// that decides whether its link can be severed and reconnected.
    ///
    /// `connect_deadline` is how long one reconnect attempt keeps trying, which a case that
    /// drives a front end across a dropped link sets long enough to outlast the front end's
    /// own timers.
    pub async fn fake(connect_deadline: Duration) -> (Backend, std::sync::Arc<FakeGate>) {
        let (direct, gate) = direct::test_support::fake_backend_with(connect_deadline).await;
        (Backend::Direct(Box::new(direct)), gate)
    }

    /// The same over the `fake-amp` profile, whose meters have a scale, so a test can play a
    /// signal on them through the gate.
    pub async fn fake_amp(connect_deadline: Duration) -> (Backend, std::sync::Arc<FakeGate>) {
        let (direct, gate) =
            direct::test_support::fake_backend_on("fake-amp", connect_deadline).await;
        (Backend::Direct(Box::new(direct)), gate)
    }

    /// The same over a profile the case built, for a device with controls no built-in has.
    pub async fn fake_over(
        profile: Profile,
        connect_deadline: Duration,
    ) -> (Backend, std::sync::Arc<FakeGate>) {
        let (direct, gate) =
            direct::test_support::fake_backend_over(profile, connect_deadline).await;
        (Backend::Direct(Box::new(direct)), gate)
    }

    /// Say that this backend is on the configured device called `name`, which a fake device has
    /// no configuration file to have been picked out of.
    pub(crate) fn configured_as(&mut self, name: &str) {
        if let Backend::Direct(direct) = self {
            direct.configured = Some(name.to_string());
        }
    }

    /// A backend over the daemon listening at `socket`, opened the way a verb opens one over a
    /// running daemon: what a case that is about the daemon half of a path drives.
    pub(crate) async fn stub_remote(socket: &std::path::Path) -> Backend {
        Backend::stub_remote_named(socket, None)
            .await
            .expect("the daemon on the socket answers")
    }

    /// The same, asking the daemon for the device `selector` names, so a case can see which
    /// device a reopen asked it about.
    pub(crate) async fn stub_remote_named(
        socket: &std::path::Path,
        selector: Option<&str>,
    ) -> Result<Backend, Failure> {
        RemoteBackend::open(socket, selector, true)
            .await
            .map(|remote| Backend::Remote(Box::new(remote)))
    }
}
