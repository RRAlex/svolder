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
