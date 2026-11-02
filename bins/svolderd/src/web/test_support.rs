// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the web UI's tests share: a [`Web`] over a fleet of the test's own, nominally bound to
//! `127.0.0.1:8738`, and one request through its router without a socket.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use futures::StreamExt;
use http_body_util::{BodyExt, BodyStream};
use hyper::{Request, StatusCode};
use svolder_ipc::config::WebConfig;
use tower::ServiceExt;

use super::*;
use crate::fleet::Fleet;

pub(crate) mod stub;

/// The address every test's server says it is bound to.
pub(crate) const BOUND: &str = "127.0.0.1:8738";

/// A server's state over `fleet` as `config` describes it, bound (nominally) to [`BOUND`],
/// with the embedded assets and no token unless `config` asks for one, when it holds `token`.
pub(crate) fn web_with(config: WebConfig, fleet: Arc<dyn Fleet>, token: Option<&str>) -> Web {
    Web::new(
        config,
        BOUND.parse().unwrap(),
        fleet,
        token.map(Key::for_test),
        Assets::Embedded,
        Default::default(),
    )
}

/// [`web_with`] with no token.
pub(crate) fn web(config: WebConfig, fleet: Arc<dyn Fleet>) -> Web {
    web_with(config, fleet, None)
}

/// A [`Web`] over `daemon`'s fleet with no sign-in.
pub(crate) fn web_over(daemon: &FakeDaemon) -> Web {
    web(open(), Arc::clone(&daemon.fleet))
}

/// What one request came back with.
#[derive(Debug)]
pub(crate) struct Reply {
    pub(crate) status: StatusCode,
    pub(crate) headers: hyper::HeaderMap,
    pub(crate) body: String,
}

impl Reply {
    /// The value of header `name`, when there is exactly one.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

/// `request` through `web`'s router, read to the end.
pub(crate) async fn call(web: &Web, request: Request<Body>) -> Reply {
    let response = router::router(web.clone())
        .oneshot(request)
        .await
        .expect("the router never fails");
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("a body").to_bytes();
    Reply {
        status: parts.status,
        headers: parts.headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

/// A GET of `path` with the server's own `Host`.
pub(crate) fn get(path: &str) -> Request<Body> {
    Request::get(path)
        .header("host", BOUND)
        .body(Body::empty())
        .unwrap()
}

/// The JSON the page carries in its data block `id` (`svolder-page` is the model of the page,
/// `svolder-themes` the list of themes), parsed.
pub(crate) fn data_block(body: &str, id: &str) -> serde_json::Value {
    let open = format!("<script type=\"application/json\" id=\"{id}\">");
    let start = body
        .find(&open)
        .unwrap_or_else(|| panic!("no data block {id}: {body}"));
    let text = &body[start + open.len()..];
    let text = &text[..text.find("</script>").expect("the block's end")];
    serde_json::from_str(text).expect("the data block is JSON")
}

/// The model of the page `body` carries.
pub(crate) fn page_model(body: &str) -> serde_json::Value {
    data_block(body, "svolder-page")
}

/// A POST of `body` to `path` with the server's own `Host`, the `Origin` its page would send and
/// a JSON content type.
pub(crate) fn post(path: &str, body: &str) -> Request<Body> {
    Request::post(path)
        .header("host", BOUND)
        .header("origin", format!("http://{BOUND}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// A loopback config with no sign-in, the web UI on.
pub(crate) fn open() -> WebConfig {
    WebConfig {
        enabled: true,
        auth: svolder_ipc::config::WebAuth::None,
        ..WebConfig::default()
    }
}

/// `StatusCode::OK` and the other codes the tests compare with.
pub(crate) use hyper::StatusCode as Code;

/// A daemon over the in-process fake device of a built-in profile, under a configured name,
/// connected, with its configuration in a directory of its own (removed on drop), and the web
/// UI's fleet over it.
pub(crate) struct FakeDaemon {
    pub(crate) daemon: Arc<crate::handler::Daemon>,
    pub(crate) fleet: Arc<dyn Fleet>,
    pub(crate) dir: std::path::PathBuf,
}

impl FakeDaemon {
    /// Shut the daemon's actors down.
    pub(crate) async fn stop(self) {
        self.daemon.shutdown().await;
    }
}

impl Drop for FakeDaemon {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A [`FakeDaemon`] of `profile`'s device named `name`.
pub(crate) async fn fake_daemon(profile: &str, name: &str) -> FakeDaemon {
    fake_daemon_with(profile, name, "").await
}

/// [`fake_daemon`] with the lines of `extra` appended to its `config.toml`.
pub(crate) async fn fake_daemon_with(profile: &str, name: &str, extra: &str) -> FakeDaemon {
    fake_daemon_on(profile, name, extra, crate::host::HostMode::Off).await
}

/// [`fake_daemon`] whose host is the fake one: a fake player with a cover, and a route on a
/// Schiit sink named for the device.
pub(crate) async fn fake_daemon_playing(profile: &str, name: &str) -> FakeDaemon {
    fake_daemon_on(profile, name, "", crate::host::HostMode::Fake).await
}

/// [`fake_daemon_with`] with the host in `mode`.
async fn fake_daemon_on(
    profile: &str,
    name: &str,
    extra: &str,
    mode: crate::host::HostMode,
) -> FakeDaemon {
    fleet_daemon(&[(profile, name, "0102030405060708")], extra, mode).await
}

/// A [`FakeDaemon`] of several devices, each `(profile, name, unit id)`, all connected: the
/// first answers as the single one does, the others answer their own units
/// (`SVOLDER_FAKE_MORE`'s fleet).
pub(crate) async fn fake_daemon_fleet(devices: &[(&str, &str, &str)]) -> FakeDaemon {
    fleet_daemon(devices, "", crate::host::HostMode::Off).await
}

/// The fake daemon over `devices`, with the lines of `extra` appended to its `config.toml`.
async fn fleet_daemon(
    devices: &[(&str, &str, &str)],
    extra: &str,
    mode: crate::host::HostMode,
) -> FakeDaemon {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("svolderd-web-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.toml");
    let mut text = "schema = 1\n".to_string();
    for (_, name, udid) in devices {
        let quoted = format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""));
        text.push_str(&format!(
            "[[device]]\nname = {quoted}\nudid = \"{udid}\"\npassphrase = \"1234\"\n"
        ));
    }
    text.push_str(extra);
    std::fs::write(&config, text).unwrap();
    let registry = Arc::new(svolder_profiles::Registry::builtin().unwrap());
    let member = |(profile, _, udid): &(&str, &str, &str)| {
        let built = registry.get(profile).expect("a built-in").clone();
        svolder_ble::FakeTransport::from_profile(
            &built,
            udid.parse().unwrap(),
            3,
            svolder_proto::Passphrase::from_text("1234").unwrap(),
        )
    };
    let transport: Arc<dyn svolder_ble::Transport> = match devices {
        [only] => Arc::new(member(only)),
        [first, more @ ..] => {
            let more: Vec<String> = more.iter().map(|(p, _, u)| format!("{p}@{u}")).collect();
            Arc::new(
                svolder_ble::FakeFleet::from_more(&registry, member(first), &more.join(","), false)
                    .expect("a fleet"),
            )
        }
        [] => panic!("a fake daemon needs a device"),
    };
    let daemon = crate::handler::Daemon::new(
        transport,
        registry,
        config.clone(),
        dir.join("profiles"),
        dir.join("s.sock"),
    )
    .with_host_mode(mode)
    .shared();
    let loaded = svolder_ipc::Config::load_from(&config).unwrap();
    daemon.reconcile(&loaded).await;
    daemon.host_to(&loaded.host);
    let connected = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let svolder_ipc::Response::Devices { devices } = daemon
                .handle_from(crate::writer::Origin::Socket, svolder_ipc::Request::Devices)
                .await
                && devices.iter().all(|d| d.connected)
                && !devices.is_empty()
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(connected.is_ok(), "the fake device never connected");
    let fleet: Arc<dyn Fleet> = Arc::new(crate::fleet::DaemonFleet::new(
        Arc::downgrade(&daemon),
        crate::writer::Origin::Web,
        crate::writer::Mode::Latest,
    ));
    FakeDaemon { daemon, fleet, dir }
}

/// The events a stream sends until `until` holds of them, as (name, data) pairs, waiting no
/// longer than five seconds.
pub(crate) async fn events_until(
    body: Body,
    until: impl Fn(&[(String, String)]) -> bool,
) -> Vec<(String, String)> {
    events_within(body, until, Duration::from_secs(5)).await
}

/// The events a stream sends until `until` holds of them or `span` is up, as (name, data) pairs.
pub(crate) async fn events_within(
    body: Body,
    until: impl Fn(&[(String, String)]) -> bool,
    span: Duration,
) -> Vec<(String, String)> {
    let mut frames = BodyStream::new(body);
    let mut text = String::new();
    let mut seen = Vec::new();
    let _ = tokio::time::timeout(span, async {
        while let Some(Ok(frame)) = frames.next().await {
            if let Ok(data) = frame.into_data() {
                text.push_str(&String::from_utf8_lossy(&data));
            }
            while let Some(end) = text.find("\n\n") {
                let block: String = text.drain(..end + 2).collect();
                if let Some(retry) = block.lines().find_map(|l| l.strip_prefix("retry:")) {
                    seen.push(("retry".to_string(), retry.trim().to_string()));
                }
                let name = block
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("event: ")
                            .or_else(|| l.strip_prefix("event:"))
                    })
                    .unwrap_or("message")
                    .trim()
                    .to_string();
                let data: Vec<&str> = block
                    .lines()
                    .filter_map(|l| l.strip_prefix("data: ").or_else(|| l.strip_prefix("data:")))
                    .collect();
                seen.push((name, data.join("\n")));
            }
            if until(&seen) {
                return;
            }
        }
    })
    .await;
    seen
}
