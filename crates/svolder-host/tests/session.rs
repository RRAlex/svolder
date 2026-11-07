// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The live chain's `pw-cli` session against fake tools: a `pw-cli` that logs what it is
//! given and a `pw-dump` that prints a file. Nothing here runs a real PipeWire tool.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::{TempDir, install_script, lines};
use svolder_host::eq::live::{props_of, set_param_line, slot_params};
use svolder_host::eq::{Params, PwCliSession};
use svolder_host::fake::FakeParams;
use svolder_host::route::PwDump;

/// The fixture of the live chain, as `pw-dump` printed it.
fn fixture() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pw-dump/live-chain.json");
    std::fs::read_to_string(path).expect("the fixture is there")
}

/// The id of the chain's node in the fixture.
fn fixture_id() -> u32 {
    props_of(&fixture())
        .expect("the chain is in the fixture")
        .id
}

/// The fixture with the chain's node under another id, as a restarted chain has.
fn with_node_id(id: u32) -> String {
    let mut objects: Vec<serde_json::Value> = serde_json::from_str(&fixture()).unwrap();
    let node = objects
        .iter_mut()
        .find(|o| o["id"].as_u64() == Some(u64::from(fixture_id())))
        .unwrap();
    node["id"] = id.into();
    serde_json::to_string(&objects).unwrap()
}

/// The fixture with no object showing a format, as a suspended sink is dumped.
fn without_formats() -> String {
    let mut objects: Vec<serde_json::Value> = serde_json::from_str(&fixture()).unwrap();
    for object in &mut objects {
        if let Some(params) = object["info"]["params"].as_object_mut() {
            params.remove("Format");
        }
    }
    serde_json::to_string(&objects).unwrap()
}

/// A case's tools: a `pw-dump` printing `dump`, and a `pw-cli` that logs `start` and then each
/// line it is given until `quit`.
struct Tools {
    dir: TempDir,
}

impl Tools {
    fn new(label: &str) -> Tools {
        let dir = TempDir::new(label);
        let tools = Tools { dir };
        tools.dump(&fixture());
        install_script(
            &tools.dir.join("pw-dump"),
            &format!(
                "#!/bin/sh\ncat '{}'\n",
                tools.dir.join("dump.json").display()
            ),
        );
        tools.cli(
            "while read line; do\n  [ \"$line\" = quit ] && exit 0\n  echo \"$line\" >> \"$LOG\"\ndone\n",
        );
        tools
    }

    /// What `pw-dump` prints from now on.
    fn dump(&self, text: &str) {
        std::fs::write(self.dir.join("dump.json"), text).expect("the dump is written");
    }

    /// A `pw-cli` that logs `start`, then runs `body` (which sees `$LOG`).
    fn cli(&self, body: &str) {
        let log = self.dir.join("log");
        install_script(
            &self.dir.join("pw-cli"),
            &format!(
                "#!/bin/sh\nLOG='{}'\necho start >> \"$LOG\"\necho $$ > '{}'\necho \"remote 0 is named 'fake'\"\n{body}",
                log.display(),
                self.dir.join("pid").display()
            ),
        );
    }

    fn log(&self) -> Vec<String> {
        lines(&self.dir.join("log"))
    }

    fn session(&self) -> PwCliSession {
        PwCliSession::resolve(
            false,
            Some(self.dir.join("pw-cli").as_os_str()),
            PwDump::new(self.dir.join("pw-dump")),
        )
        .expect("the fake pw-cli is there")
        .with_restart_gap(Duration::ZERO)
    }

    /// Wait, for at most five seconds, until the fake `pw-cli` has ended (a zombie counts: the
    /// session reaps it when it next looks).
    async fn wait_dead(&self) {
        let pid = std::fs::read_to_string(self.dir.join("pid")).expect("the fake wrote its pid");
        let stat = format!("/proc/{}/stat", pid.trim());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = std::fs::read_to_string(&stat).ok().and_then(|text| {
                text.rsplit(") ")
                    .next()
                    .and_then(|rest| rest.chars().next())
            });
            if state.is_none_or(|state| state == 'Z') || Instant::now() > deadline {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Wait, for at most five seconds, until the log has `count` lines.
    async fn wait_for(&self, count: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let log = self.log();
            if log.len() >= count || Instant::now() > deadline {
                return log;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

fn gain(value: f64) -> Vec<(String, f64)> {
    vec![
        ("lpre:Mult".to_string(), value),
        ("rpre:Mult".to_string(), value),
    ]
}

fn pk() -> svolder_profiles::tone::apo::SourceFilter {
    svolder_profiles::tone::apo::SourceFilter {
        kind: svolder_profiles::tone::apo::FilterKind::Peaking,
        f: 1000.0,
        g: 6.0,
        q: 1.0,
    }
}

/// The line `send` gives `pw-cli` for `params` on the fixture's node.
fn line_for(params: &[(String, f64)]) -> String {
    set_param_line(fixture_id(), params)
}

/// A `pw-cli` that takes a line only for the node id in the file `valid`, and answers any other
/// the way the real one does; it touches `refused` when it does.
fn picky_cli(tools: &Tools) {
    picky_cli_saying(tools, false);
}

/// [`picky_cli`], answering with the second wording the real pw-cli has (the one it gives when
/// the id has been reused by another object) when `reused`. Both wordings are the real tool's.
fn picky_cli_saying(tools: &Tools, reused: bool) {
    let refusal = match reused {
        true => {
            "Error: \\\"set-param not implemented on object $id type:PipeWire:Interface:Port\\\""
        }
        false => "Error: \\\"set-param: unknown global '$id'\\\"",
    };
    std::fs::write(tools.dir.join("valid"), fixture_id().to_string()).unwrap();
    tools.cli(&format!(
        "while read line; do\n  [ \"$line\" = quit ] && exit 0\n  id=$(echo \"$line\" | cut -d' ' -f2)\n  if [ \"$id\" = \"$(cat '{}')\" ]; then echo \"$line\" >> \"$LOG\"; else touch '{}'; echo \"{refusal}\"; fi\ndone\n",
        tools.dir.join("valid").display(),
        tools.dir.join("refused").display()
    ));
}

/// Wait, for at most five seconds, until the session has read `pw-cli`'s refusal: the fake
/// writes it a moment after it touches `refused`, and the session's reader takes it from there.
async fn wait_for_refusal(session: &PwCliSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !session.refusal_read() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Wait, for at most five seconds, until `path` exists.
async fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn one_pw_cli_carries_many_sends() {
    let tools = Tools::new("session-many");
    let mut session = tools.session();
    for step in 1..=5 {
        assert!(
            session
                .send(fixture_id(), &gain(f64::from(step) / 10.0))
                .await
        );
    }
    let log = tools.wait_for(6).await;
    assert_eq!(log.iter().filter(|line| *line == "start").count(), 1);
    assert_eq!(log.len(), 6);
    assert_eq!(log[5], line_for(&gain(0.5)));
    session.close().await;
}

#[tokio::test]
async fn nothing_is_started_before_the_first_send() {
    let tools = Tools::new("session-lazy");
    let mut session = tools.session();
    let node = session.node().await.expect("the chain is in the dump");
    assert_eq!((node.id, node.rate_hz), (fixture_id(), Some(48_000)));
    assert!(tools.log().is_empty());
}

#[tokio::test]
async fn a_dead_pw_cli_is_restarted_and_the_newest_value_sent() {
    let tools = Tools::new("session-dead");
    // This pw-cli takes one line and ends.
    tools.cli("read line\necho \"$line\" >> \"$LOG\"\n");
    let mut session = tools.session();
    assert!(session.send(fixture_id(), &gain(0.1)).await);
    tools.wait_for(2).await;
    tools.wait_dead().await;
    assert!(session.send(fixture_id(), &gain(0.2)).await);
    let log = tools.wait_for(4).await;
    assert_eq!(log.iter().filter(|line| *line == "start").count(), 2);
    assert_eq!(log.last(), Some(&line_for(&gain(0.2))));
}

#[tokio::test]
async fn a_line_lost_with_a_dead_pw_cli_is_sent_again_after_the_restart() {
    let tools = Tools::new("session-lost");
    // This pw-cli ends without taking a line, as one that died with the line in its pipe.
    tools.cli("exit 0\n");
    let mut session = tools.session();
    // The first line may or may not be written before the process is gone.
    let _ = session
        .send(fixture_id(), &slot_params(7, Some(&pk()), 48_000))
        .await;
    tools.wait_dead().await;
    tools.cli(
        "while read line; do [ \"$line\" = quit ] && exit 0; echo \"$line\" >> \"$LOG\"; done\n",
    );
    assert!(session.send(fixture_id(), &gain(0.3)).await);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !tools.log().iter().any(|line| line.starts_with("set-param")) && Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let last = tools.log().last().expect("a line").clone();
    assert!(
        last.contains("\"l07:b0\""),
        "the older slot is sent again: {last}"
    );
    assert!(last.contains("\"lpre:Mult\" 0.3"), "{last}");
    session.close().await;
}

#[tokio::test]
async fn a_new_chain_does_not_inherit_the_values_sent_to_the_old_one() {
    let tools = Tools::new("session-new-chain");
    tools.cli("exit 0\n");
    let mut session = tools.session();
    let _ = session
        .send(fixture_id(), &slot_params(7, Some(&pk()), 48_000))
        .await;
    tools.wait_dead().await;
    // An apply put a new chain in place: what was sent to the old one is not on it.
    session.new_chain();
    tools.cli(
        "while read line; do [ \"$line\" = quit ] && exit 0; echo \"$line\" >> \"$LOG\"; done\n",
    );
    assert!(session.send(fixture_id(), &gain(0.3)).await);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !tools.log().iter().any(|line| line.starts_with("set-param")) && Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let last = tools.log().last().expect("a line").clone();
    assert!(
        !last.contains("\"l07:"),
        "the old chain's slot is not sent: {last}"
    );
    assert!(last.contains("\"lpre:Mult\" 0.3"), "{last}");
    session.close().await;
}

#[tokio::test]
async fn a_dead_pw_cli_is_not_started_again_within_the_gap() {
    let tools = Tools::new("session-gap");
    tools.cli("read line\necho \"$line\" >> \"$LOG\"\n");
    let mut session = tools.session().with_restart_gap(Duration::from_secs(3600));
    assert!(session.send(fixture_id(), &gain(0.1)).await);
    tools.wait_for(2).await;
    tools.wait_dead().await;
    assert!(
        !session.send(fixture_id(), &gain(0.2)).await,
        "the caller keeps the value to send again"
    );
    assert_eq!(
        tools.log().iter().filter(|line| *line == "start").count(),
        1
    );
}

#[tokio::test]
async fn a_new_node_id_is_found_before_the_next_send() {
    let tools = Tools::new("session-new-id");
    let mut session = tools.session();
    let old = fixture_id();
    assert!(session.send(fixture_id(), &gain(0.1)).await);
    // The chain restarted: its node has another id, and the next dump says so.
    tools.dump(&with_node_id(999));
    // Without a reason to look again the kept id is used.
    assert!(session.send(fixture_id(), &gain(0.2)).await);
    session.forget_node();
    assert_eq!(session.node().await.expect("found again").id, 999);
    assert!(session.send(999, &gain(0.3)).await);
    let log = tools.wait_for(4).await;
    assert!(log[1].starts_with(&format!("set-param {old} ")));
    assert!(log[2].starts_with(&format!("set-param {old} ")));
    assert!(log[3].starts_with("set-param 999 "), "{log:?}");
    // After forget_node no line names the old id.
    assert!(
        log[3..]
            .iter()
            .all(|line| !line.contains(&format!("set-param {old} ")))
    );
    session.close().await;
}

#[tokio::test]
async fn a_refused_set_param_makes_the_next_send_find_the_node_and_send_everything() {
    refused_send_is_repaired("session-refused-send", false).await;
}

#[tokio::test]
async fn a_reused_id_refusal_is_repaired_the_same_way() {
    refused_send_is_repaired("session-refused-reused", true).await;
}

async fn refused_send_is_repaired(label: &str, reused: bool) {
    let tools = Tools::new(label);
    picky_cli_saying(&tools, reused);
    let mut session = tools.session();
    assert!(session.send(fixture_id(), &gain(0.1)).await);
    // The chain restarted under another id; nothing tells the session but pw-cli's answer.
    tools.dump(&with_node_id(999));
    std::fs::write(tools.dir.join("valid"), "999").unwrap();
    let slot = slot_params(7, Some(&pk()), 48_000);
    assert!(
        session.send(fixture_id(), &slot).await,
        "written to the old id, refused"
    );
    wait_for_file(&tools.dir.join("refused")).await;
    wait_for_refusal(&session).await;
    assert!(session.send(999, &gain(0.4)).await);
    let log = tools.wait_for(3).await;
    assert_eq!(log.len(), 3, "{log:?}");
    let last = &log[2];
    assert!(last.starts_with("set-param 999 "), "{last}");
    assert!(
        last.contains("\"l07:b0\""),
        "the refused slot is sent again: {last}"
    );
    assert!(
        last.contains("\"lpre:Mult\" 0.4"),
        "the newest value wins: {last}"
    );
    assert!(!last.contains("\"lpre:Mult\" 0.1"));
    session.close().await;
}

#[tokio::test]
async fn settle_repairs_a_refusal_without_a_new_value() {
    let tools = Tools::new("session-refused-settle");
    picky_cli(&tools);
    let mut session = tools.session();
    assert!(
        session.settle(Some(999)).await,
        "nothing sent, nothing to repair"
    );
    assert!(session.send(fixture_id(), &gain(0.1)).await);
    tools.dump(&with_node_id(999));
    std::fs::write(tools.dir.join("valid"), "999").unwrap();
    assert!(
        session
            .send(fixture_id(), &slot_params(7, Some(&pk()), 48_000))
            .await
    );
    wait_for_file(&tools.dir.join("refused")).await;
    wait_for_refusal(&session).await;
    assert!(session.settle(Some(999)).await);
    let log = tools.wait_for(3).await;
    assert_eq!(log.len(), 3, "{log:?}");
    assert!(log[2].starts_with("set-param 999 "));
    assert!(log[2].contains("\"lpre:Mult\" 0.1") && log[2].contains("\"l07:b0\""));
    // Settled: nothing more to do.
    assert!(session.settle(Some(999)).await);
    assert_eq!(tools.log().len(), 3);
    session.close().await;
}

#[tokio::test]
async fn a_suspended_sink_is_not_a_change_of_rate() {
    let tools = Tools::new("session-rate");
    let mut session = tools.session();
    assert_eq!(session.node().await.unwrap().rate_hz, Some(48_000));
    tools.dump(&without_formats());
    session.forget_node();
    assert_eq!(
        session.node().await.unwrap().rate_hz,
        Some(48_000),
        "the last known rate is kept"
    );
    let fresh = tools.session();
    let mut fresh = fresh;
    assert_eq!(fresh.node().await.unwrap().rate_hz, None);
}

#[tokio::test]
async fn no_node_means_no_send_and_no_pw_cli() {
    let tools = Tools::new("session-no-node");
    tools.dump("[]");
    let mut session = tools.session();
    assert_eq!(session.node().await, None);
    assert!(!session.send(fixture_id(), &gain(0.1)).await);
    assert!(tools.log().is_empty());
}

#[tokio::test]
async fn no_pw_cli_reads_as_unavailable() {
    let tools = Tools::new("session-missing");
    let missing = tools.dir.join("no-such-pw-cli");
    let dump = || PwDump::new(tools.dir.join("pw-dump"));
    assert!(PwCliSession::resolve(false, Some(missing.as_os_str()), dump()).is_none());
    // A file that is not executable is no pw-cli either.
    std::fs::write(tools.dir.join("plain"), "x").unwrap();
    assert!(
        PwCliSession::resolve(false, Some(tools.dir.join("plain").as_os_str()), dump()).is_none()
    );
}

#[tokio::test]
async fn a_node_found_under_another_id_than_the_callers_gets_no_line() {
    let tools = Tools::new("session-unvouched");
    picky_cli_saying(&tools, false);
    let mut session = tools.session();
    assert!(session.send(fixture_id(), &gain(0.1)).await);
    tools.dump(&with_node_id(999));
    std::fs::write(tools.dir.join("valid"), "999").unwrap();
    // Written to the old id and refused; the chain has another node now.
    assert!(
        session
            .send(fixture_id(), &slot_params(7, Some(&pk()), 48_000))
            .await
    );
    wait_for_file(&tools.dir.join("refused")).await;
    wait_for_refusal(&session).await;
    // The caller has not looked at node 999: nothing is written to it, and it is told so.
    assert!(!session.send(fixture_id(), &gain(0.4)).await);
    assert!(!session.settle(Some(fixture_id())).await);
    assert!(!session.settle(None).await);
    let log = tools.log();
    assert!(
        log.iter().all(|line| !line.starts_with("set-param 999 ")),
        "{log:?}"
    );
    // Looked at, it gets the newest value of every port.
    assert!(session.send(999, &gain(0.4)).await);
    let log = tools.wait_for(3).await;
    assert!(log[2].starts_with("set-param 999 "), "{log:?}");
    session.close().await;
}

#[tokio::test]
async fn host_off_starts_nothing() {
    let tools = Tools::new("session-off");
    let off = PwCliSession::resolve(
        true,
        Some(tools.dir.join("pw-cli").as_os_str()),
        PwDump::new(tools.dir.join("pw-dump")),
    );
    assert!(off.is_none());
    assert!(tools.log().is_empty());
}

#[tokio::test]
async fn pw_cli_is_given_only_set_param_lines_for_the_named_node() {
    let tools = Tools::new("session-only-set-param");
    let mut session = tools.session();
    assert!(
        session
            .send(fixture_id(), &slot_params(7, Some(&pk()), 48_000))
            .await
    );
    assert!(session.send(fixture_id(), &gain(0.5)).await);
    session.close().await;
    let log = tools.log();
    assert!(log.len() >= 3, "{log:?}");
    let prefix = format!("set-param {} Props {{ params = [", fixture_id());
    for line in &log {
        assert!(
            line == "start" || line == "quit" || line.starts_with(&prefix),
            "{line}"
        );
    }
    assert_eq!(log.iter().filter(|line| *line == "start").count(), 1);
}

#[tokio::test]
async fn the_fake_records_the_lines_a_session_would_send() {
    let mut fake = FakeParams::new();
    let params = gain(0.25);
    assert!(fake.send(1, &params).await);
    fake.takes(false);
    assert!(!fake.send(1, &gain(0.5)).await);
    fake.takes(true);
    fake.set_node(None);
    assert!(!fake.send(1, &gain(0.5)).await);
    assert_eq!(fake.lines(), vec![set_param_line(1, &params)]);
    assert_eq!(fake.sent(), vec![params]);
    fake.forget_node();
    assert_eq!(fake.forgotten(), 1);
    assert!(fake.settle(Some(1)).await);
}

#[tokio::test]
async fn the_full_send_after_a_refusal_keeps_the_callers_order() {
    let tools = Tools::new("session-refused-order");
    picky_cli_saying(&tools, false);
    let mut session = tools.session();
    assert!(session.send(fixture_id(), &gain(0.9)).await);
    tools.dump(&with_node_id(999));
    std::fs::write(tools.dir.join("valid"), "999").unwrap();
    assert!(
        session
            .send(fixture_id(), &slot_params(7, Some(&pk()), 48_000))
            .await
    );
    wait_for_file(&tools.dir.join("refused")).await;
    wait_for_refusal(&session).await;
    // A cut goes before the boost it makes room for: the caller's order survives the repair.
    let mut falling = gain(0.2);
    falling.extend(slot_params(7, Some(&pk()), 48_000));
    assert!(session.send(999, &falling).await);
    let log = tools.wait_for(3).await;
    let last = &log[2];
    let (pre, slot) = (last.find("\"lpre:Mult\""), last.find("\"l07:b0\""));
    assert!(
        pre.is_some() && slot.is_some() && pre < slot,
        "the preamp comes first: {last}"
    );
    session.close().await;
}

#[tokio::test]
async fn the_full_send_after_a_refusal_puts_the_rest_before_a_rising_preamp() {
    let tools = Tools::new("session-refused-rising");
    picky_cli_saying(&tools, false);
    let mut session = tools.session();
    let mut first = slot_params(3, Some(&pk()), 48_000);
    first.extend(gain(0.5));
    assert!(session.send(fixture_id(), &first).await);
    tools.dump(&with_node_id(999));
    std::fs::write(tools.dir.join("valid"), "999").unwrap();
    assert!(
        session
            .send(fixture_id(), &slot_params(7, Some(&pk()), 48_000))
            .await
    );
    wait_for_file(&tools.dir.join("refused")).await;
    wait_for_refusal(&session).await;
    // A preamp that rises goes after every other port: the other slots' newest values never
    // play under the risen preamp.
    let mut rising = slot_params(7, Some(&pk()), 48_000);
    rising.extend(gain(0.9));
    assert!(session.send(999, &rising).await);
    let log = tools.wait_for(3).await;
    let last = &log[2];
    let (rest, pre) = (last.find("\"l03:b0\""), last.find("\"lpre:Mult\""));
    assert!(
        rest.is_some() && pre.is_some() && rest < pre,
        "the other ports come before the risen preamp: {last}"
    );
    session.close().await;
}
