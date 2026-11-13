// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Unit tests of the daemon's entry point.

/// A unit test never resolves the user's own homes, with or without `make test`: every home is
/// under the temporary directory.
#[test]
fn unit_tests_resolve_scratch_homes() {
    let scratch = std::env::temp_dir();
    let homes = [
        svolder_ipc::config::config_home(),
        svolder_ipc::config::state_home(),
        std::path::PathBuf::from(std::env::var_os("XDG_DATA_HOME").expect("a data home")),
        svolder_ipc::config::cache_home(),
        std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").expect("a runtime dir")),
    ];
    for home in homes {
        assert!(
            home.starts_with(&scratch),
            "{home:?} is not under {scratch:?}"
        );
    }
    assert!(std::env::var_os("SVOLDER_MCP_TOKEN").is_none());
    assert!(std::env::var_os("SVOLDER_WEB_TOKEN").is_none());
}

/// A fake daemon's AutoEq base is the address nothing listens on whether the transport came as
/// the flag or from the environment variable clap reads, and a configured URL still wins.
#[test]
fn a_fake_daemon_never_defaults_to_the_real_autoeq() {
    use clap::Parser;
    let fake = "http://127.0.0.1:9/results/";
    let by_flag = crate::Args::parse_from(["svolderd", "--transport", "fake"]);
    assert_eq!(by_flag.transport, "fake");
    assert_eq!(
        crate::autoeq_client(&by_flag, None).base(),
        fake,
        "the flag"
    );
    let ble = crate::Args::parse_from(["svolderd", "--transport", "ble"]);
    let real = crate::autoeq_client(&ble, None);
    assert!(
        real.base().starts_with("https://"),
        "a real transport keeps AutoEq's own"
    );
    let url = Some("http://127.0.0.1:1/x/".to_string());
    assert_eq!(
        crate::autoeq_client(&by_flag, url).base(),
        "http://127.0.0.1:1/x/"
    );
    assert_eq!(
        crate::autoeq_client(&by_flag, Some("  ".into())).base(),
        fake,
        "blank"
    );
}
