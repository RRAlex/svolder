// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[mcp]` block: its defaults, keys, bounds, template and writer.

use super::*;

#[test]
fn mcp_defaults_off_on_loopback_with_a_token_and_read_only() {
    let c = Config::parse("").unwrap();
    assert!(!c.mcp.enabled);
    assert_eq!(c.mcp.listen.to_string(), "127.0.0.1:8737");
    assert_eq!(c.mcp.auth, McpAuth::Token);
    assert!(!c.mcp.writes);
    assert_eq!(c.mcp.max_volume_db, None);
    assert!(c.mcp.origins.is_empty());
    assert_eq!(c.mcp.token_file, None);
}

#[test]
fn mcp_block_parses_every_key() {
    let text = "[mcp]\n\
                enabled = true\n\
                listen = \"192.168.1.10:9000\"\n\
                auth = \"token\"\n\
                writes = true\n\
                max_volume_db = -12.5\n\
                origins = [\"https://claude.ai:443\", \" \"]\n\
                hosts = [\"mcp.home.lan\", \" \", \"10.0.0.5:8737\"]\n\
                token_file = \"/run/secrets/mcp\"\n";
    let c = Config::parse(text).unwrap();
    assert!(c.mcp.enabled);
    assert_eq!(c.mcp.listen.to_string(), "192.168.1.10:9000");
    assert!(c.mcp.writes);
    assert_eq!(c.mcp.max_volume_db, Some(-12.5));
    assert_eq!(c.mcp.origins, vec!["https://claude.ai:443".to_string()]);
    assert_eq!(c.mcp.hosts, ["mcp.home.lan", "10.0.0.5:8737"]);
    assert_eq!(c.mcp.token_file.as_deref(), Some("/run/secrets/mcp"));
}

#[test]
fn mcp_auth_none_needs_a_loopback_listen_address() {
    let err =
        Config::parse("[mcp]\nauth = \"none\"\nlisten = \"192.168.1.10:8737\"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m)
            if m == "mcp.auth = \"none\" needs a loopback listen address"),
        "got {err:?}"
    );
    // The wildcard address is not loopback either.
    assert!(Config::parse("[mcp]\nauth = \"none\"\nlisten = \"0.0.0.0:8737\"\n").is_err());
    for listen in ["127.0.0.1:8737", "[::1]:8737", "127.0.0.2:1"] {
        let text = format!("[mcp]\nauth = \"none\"\nlisten = \"{listen}\"\n");
        assert_eq!(
            Config::parse(&text).unwrap().mcp.auth,
            McpAuth::None,
            "{listen}"
        );
    }
    // Token auth is fine anywhere.
    assert!(Config::parse("[mcp]\nlisten = \"192.168.1.10:8737\"\n").is_ok());
}

#[test]
fn a_bad_mcp_listen_auth_or_cap_is_an_error_naming_the_key() {
    let err = Config::parse("[mcp]\nlisten = \"nowhere\"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("mcp.listen")),
        "got {err:?}"
    );
    assert!(Config::parse("[mcp]\nauth = \"oidc\"\n").is_err());
    let err = Config::parse("[mcp]\nmax_volume_db = nan\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("mcp.max_volume_db")),
        "got {err:?}"
    );
}

#[test]
fn a_bad_mcp_origins_entry_is_refused() {
    for (bad, said) in [
        ("null", "\"null\" would admit any sandboxed page"),
        ("NULL", "would admit any sandboxed page"),
        (
            "https://claude.ai",
            "needs a port; rmcp matches a port-less origin on every port",
        ),
        ("http://localhost:", "needs a port"),
        ("http://[::1]", "needs a port"),
        ("claude.ai:443", "needs a scheme"),
    ] {
        let err = Config::parse(&format!("[mcp]\norigins = [\"{bad}\"]\n")).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.starts_with("mcp.origins: ") && m.contains(said)),
            "{bad}: {err:?}"
        );
    }
    for good in [
        "https://claude.ai:443",
        "http://localhost:*",
        "http://[::1]:8080",
    ] {
        assert!(
            Config::parse(&format!("[mcp]\norigins = [\"{good}\"]\n")).is_ok(),
            "{good}"
        );
    }
}

#[test]
fn a_bad_mcp_hosts_entry_is_an_error_naming_the_key() {
    for bad in [
        "https://mcp.home.lan",
        "mcp.home.lan/x",
        "*.home.lan",
        "a b",
        "u@h",
    ] {
        let err = Config::parse(&format!("[mcp]\nhosts = [\"{bad}\"]\n")).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains("mcp.hosts")),
            "{bad}: {err:?}"
        );
    }
}

#[test]
fn the_template_names_every_mcp_key_and_parses_back_to_the_defaults() {
    let template = Config::template();
    for key in [
        "[mcp]",
        "# enabled = false",
        "# listen = \"127.0.0.1:8737\"",
        "# auth = \"token\"",
        "# writes = false",
        "# max_volume_db = -10.0",
        "# origins = []",
        "# hosts = []",
        "# token_file = \"\"",
    ] {
        assert!(template.contains(key), "{key}");
    }
    assert_eq!(Config::parse(&template).unwrap().mcp, McpConfig::default());
}

/// A change of the MCP rows writes only the key it touches, keeps the comments the template
/// carries and leaves every other table alone.
#[test]
fn update_mcp_writes_only_the_changed_key() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, Config::template()).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();

    Config::update_mcp(&path, |mcp| mcp.enabled = true).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    let config = Config::parse(&text).unwrap();
    assert!(config.mcp.enabled);
    assert!(!config.mcp.writes);
    assert!(text.contains("enabled = true"), "{text}");
    assert!(text.contains("# The address it listens on"), "{text}");
    assert_eq!(text.lines().count(), before.lines().count());

    Config::update_mcp(&path, |mcp| mcp.listen = "127.0.0.1:9000".parse().unwrap()).unwrap();
    Config::update_mcp(&path, |mcp| mcp.writes = true).unwrap();
    let config = Config::load_from(&path).unwrap();
    assert!(config.mcp.enabled && config.mcp.writes);
    assert_eq!(config.mcp.listen.port(), 9000);

    let untouched = std::fs::read_to_string(&path).unwrap();
    Config::update_mcp(&path, |_| {}).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap() == untouched);
}
