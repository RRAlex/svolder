// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The default route and the sink the chain is put in front of.

use super::*;

#[tokio::test]
async fn the_default_route_sets_the_default_and_remove_restores_it() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("wpver"), "0.4.17").unwrap();
    let host = f.host(Manager::Systemd);
    let state = host.apply(request("HD 650")).await.unwrap();
    assert_eq!(state.route, Route::Default);
    assert_eq!(state.target, "dac");
    assert_eq!(state.previous_default.as_deref(), Some("dac"));
    assert_eq!(f.default_id(), "44", "the chain is the default output");
    let conf = std::fs::read_to_string(f.dir.join("config/svolder/host-eq.conf")).unwrap();
    assert!(conf.contains("target.object = \"dac\""), "{conf}");
    assert!(!conf.contains("filter.smart"), "{conf}");

    // Applied again while it is the default: the sink and the previous default are kept.
    let again = host.apply(request("Another")).await.unwrap();
    assert_eq!(again.target, "dac");
    assert_eq!(again.previous_default.as_deref(), Some("dac"));

    host.remove().await.unwrap();
    assert_eq!(f.default_id(), "42", "the default output is back");
}

#[tokio::test]
async fn a_previous_default_that_is_gone_is_not_restored() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("wpver"), "0.4.17").unwrap();
    let host = f.host(Manager::Systemd);
    let mut state = host.apply(request("HD 650")).await.unwrap();
    state.previous_default = Some("unplugged".to_string());
    state.save(&host.state).unwrap();
    host.remove().await.unwrap();
    assert_eq!(f.default_id(), "44");
}

// ---- the sink ----

#[tokio::test]
async fn the_sink_is_named_by_node_name_or_by_wpctl_id() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    let mut by_name = request("A");
    by_name.sink = Some("alsa_output.speakers".to_string());
    assert_eq!(
        host.apply(by_name).await.unwrap().target,
        "alsa_output.speakers"
    );
    let mut by_id = request("A");
    by_id.sink = Some("45".to_string());
    assert_eq!(host.apply(by_id).await.unwrap().target, "hdmi");
    let conf = std::fs::read_to_string(f.dir.join("config/svolder/host-eq.conf")).unwrap();
    assert!(conf.contains("node.name = \"hdmi\" }"), "{conf}");
}

#[tokio::test]
async fn a_sink_that_is_no_sink_or_is_the_chain_itself_is_refused() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    for (sink, said) in [
        ("99", "no sink with id 99"),
        ("svolder_eq", "the computer EQ itself"),
        ("  ", "not a sink name"),
    ] {
        let mut asked = request("A");
        asked.sink = Some(sink.to_string());
        let failure = host.apply(asked).await.unwrap_err();
        assert!(failure.what.contains(said), "{sink}: {}", failure.what);
    }
    assert_eq!(f.written(), Vec::<String>::new());
}

#[tokio::test]
async fn without_a_default_output_a_sink_must_be_named() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("default"), "").unwrap();
    let failure = f
        .host(Manager::Systemd)
        .apply(request("A"))
        .await
        .unwrap_err();
    assert!(
        failure.what.contains("no default output"),
        "{}",
        failure.what
    );
    assert!(failure.next.contains("--sink"), "{}", failure.next);
}
