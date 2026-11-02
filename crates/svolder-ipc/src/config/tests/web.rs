// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[web]` block: its keys, defaults, warnings and writer.

use super::*;

/// An absent `[web]` table is the web UI off on loopback port 8738, signed in by token, with
/// writes on and no guest; every key reads back as written.
#[test]
fn the_web_table_reads_its_keys_and_defaults() {
    assert_eq!(Config::parse("").unwrap().web, WebConfig::default());
    let web = WebConfig::default();
    assert!(!web.enabled);
    assert_eq!(web.listen, "127.0.0.1:8738".parse().unwrap());
    assert_eq!(
        (web.auth, web.writes, web.guest),
        (WebAuth::Token, true, WebGuest::None)
    );
    let text = r#"
[web]
enabled = true
listen = "0.0.0.0:9000"
auth = "token"
writes = false
max_volume_db = -12.5
guest = "read"
url = "https://svolder.home.lan"
hosts = ["svolder.home.lan", " media.local:9000 "]
origins = ["HTTPS://Svolder.Home.Lan:443", "http://media.local:9000"]
token_file = "/run/secrets/web"
"#;
    let web = Config::parse(text).unwrap().web;
    assert!(web.enabled && !web.writes);
    assert_eq!(web.listen, "0.0.0.0:9000".parse().unwrap());
    assert_eq!(web.max_volume_db, Some(-12.5));
    assert_eq!(web.guest, WebGuest::Read);
    assert_eq!(web.url.as_deref(), Some("https://svolder.home.lan/"));
    assert_eq!(web.hosts, ["svolder.home.lan", "media.local:9000"]);
    assert_eq!(
        web.origins,
        ["https://svolder.home.lan", "http://media.local:9000"]
    );
    assert_eq!(web.token_file.as_deref(), Some("/run/secrets/web"));
    let blank = Config::parse("[web]\nurl = \"\"\ntoken_file = \" \"\n")
        .unwrap()
        .web;
    assert_eq!((blank.url, blank.token_file), (None, None));
}

/// Each refusal names its key, as `[mcp]`'s do.
#[test]
fn a_bad_web_key_is_refused_naming_it() {
    for (line, words) in [
        (
            "listen = \"nowhere\"",
            "web.listen \"nowhere\" must be an address and port",
        ),
        (
            "auth = \"none\"\nlisten = \"0.0.0.0:8738\"",
            "web.auth = \"none\" needs a loopback listen address",
        ),
        (
            "guest = \"everyone\"",
            "web.guest \"everyone\" must be \"none\" or \"read\"",
        ),
        (
            "max_volume_db = nan",
            "web.max_volume_db must be a finite number",
        ),
        (
            "url = \"ftp://x/\"",
            "web.url \"ftp://x/\" must be an http:// or https:// address",
        ),
        (
            "url = \"https://x/?a=1\"",
            "web.url \"https://x/?a=1\" must be an http://",
        ),
        (
            "url = \"https://x/#top\"",
            "web.url \"https://x/#top\" must be an http://",
        ),
        (
            "url = \"https://\"",
            "web.url \"https://\" must be an http://",
        ),
        (
            "hosts = [\"http://x\"]",
            "web.hosts entry \"http://x\" must be a host name",
        ),
        (
            "origins = [\"null\"]",
            "web.origins: \"null\" would admit any sandboxed page",
        ),
        (
            "origins = [\"svolder.lan\"]",
            "web.origins: \"svolder.lan\" must be a scheme and a host",
        ),
        (
            "origins = [\"https://x/path\"]",
            "web.origins: \"https://x/path\" must be a scheme",
        ),
    ] {
        let err = Config::parse(&format!("[web]\n{line}\n")).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains(words)),
            "{line}: {err:?}"
        );
    }
    // A loopback listen may go without auth.
    let open = Config::parse("[web]\nauth = \"none\"\nlisten = \"[::1]:8738\"\n").unwrap();
    assert_eq!(open.web.auth, WebAuth::None);
}

/// An origin compares as its scheme, its host in lower case, and its port only where it is
/// not the scheme's own.
#[test]
fn origins_compare_without_case_or_the_default_port() {
    assert_eq!(
        normal_origin("HTTP://Media.Local:80").as_deref(),
        Some("http://media.local")
    );
    assert_eq!(
        normal_origin("https://x:443/").as_deref(),
        Some("https://x")
    );
    assert_eq!(
        normal_origin("https://x:8443").as_deref(),
        Some("https://x:8443")
    );
    assert_eq!(
        normal_origin("http://[::1]:8738").as_deref(),
        Some("http://[::1]:8738")
    );
    assert_eq!(
        normal_origin("http://[::1]").as_deref(),
        Some("http://[::1]")
    );
    for bad in [
        "null",
        "x",
        "ftp://x",
        "http://",
        "http://a@b",
        "http://x:port",
        "http://x/p",
    ] {
        assert_eq!(normal_origin(bad), None, "{bad}");
    }
}

/// A guest setting that cannot apply, and a web UI and an MCP server that would meet on one
/// port, are said once as the file loads; neither stops it loading.
#[test]
fn web_settings_that_cannot_work_together_are_warned_about() {
    let quiet = Config::parse("[web]\nenabled = true\n[mcp]\nenabled = true\n").unwrap();
    assert!(quiet.warnings().is_empty(), "{:?}", quiet.warnings());
    let guest =
        Config::parse("[web]\nenabled = true\nauth = \"none\"\nguest = \"read\"\n").unwrap();
    assert_eq!(
        guest.warnings(),
        ["web.guest = \"read\" has no effect with web.auth = \"none\": every visitor is signed in"]
    );
    // A guest's read view on a wildcard listen answers any Host, so DNS rebinding can read it.
    let rebind = "[web]\nenabled = true\nlisten = \"0.0.0.0:9000\"\nguest = \"read\"\n";
    assert_eq!(
        Config::parse(rebind).unwrap().warnings(),
        [
            "web.guest = \"read\" on a wildcard listen with no web.hosts answers any Host name: a \
          page on the web can read the view through DNS rebinding; list the names in web.hosts"
        ]
    );
    for quiet in [
        "[web]\nenabled = true\nlisten = \"0.0.0.0:9000\"\nguest = \"read\"\nhosts = [\"a.lan\"]\n",
        "[web]\nenabled = true\nlisten = \"192.168.1.5:9000\"\nguest = \"read\"\n",
        "[web]\nenabled = true\nlisten = \"0.0.0.0:9000\"\n",
    ] {
        let warned = Config::parse(quiet).unwrap().warnings();
        assert!(warned.is_empty(), "{quiet}: {warned:?}");
    }
    for (web, mcp) in [
        ("127.0.0.1:9000", "127.0.0.1:9000"),
        ("0.0.0.0:9000", "127.0.0.1:9000"),
        ("127.0.0.1:9000", "[::]:9000"),
    ] {
        let text = format!(
            "[web]\nenabled = true\nlisten = \"{web}\"\n[mcp]\nenabled = true\nlisten = \"{mcp}\"\n"
        );
        let warned = Config::parse(&text).unwrap().warnings();
        assert_eq!(
            warned,
            [format!(
                "[web] listen {web} and [mcp] listen {mcp} share port 9000; the second to start \
                 cannot listen"
            )],
            "{web} {mcp}"
        );
    }
    let apart = "[web]\nenabled = true\nlisten = \"127.0.0.1:9000\"\n\
                 [mcp]\nenabled = true\nlisten = \"192.168.1.5:9000\"\n";
    assert!(Config::parse(apart).unwrap().warnings().is_empty());
    let off = "[web]\nlisten = \"127.0.0.1:8737\"\n[mcp]\nenabled = true\n";
    assert!(Config::parse(off).unwrap().warnings().is_empty());
}

/// The template documents every `[web]` key after `[mcp]`, and uncommented it is the defaults.
#[test]
fn the_template_names_every_web_key_and_parses_back_to_the_defaults() {
    let template = Config::template();
    for key in [
        "# svolderd's web UI for a browser, off until enabled.\n",
        "[web]",
        "# enabled = false",
        "# listen = \"127.0.0.1:8738\"",
        "# auth = \"token\"",
        "# writes = true",
        "# max_volume_db = -10.0",
        "# guest = \"none\"",
        "# url = \"\"",
        "# hosts = []",
        "# origins = []",
        "# token_file = \"\"",
        "0.0.0.0:8738",
        "plain HTTP",
    ] {
        assert!(template.contains(key), "{key}");
    }
    assert!(template.find("[mcp]").expect("[mcp]") < template.find("[web]").expect("[web]"));
    assert_eq!(Config::parse(&template).unwrap().web, WebConfig::default());
}

/// The Web box's writer sets `enabled` and `listen` and nothing else, keeping the comments.
#[test]
fn update_web_writes_only_enabled_and_listen() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, Config::template()).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    Config::update_web(&path, |web| {
        web.enabled = true;
        web.listen = "0.0.0.0:8738".parse().unwrap();
        web.writes = false;
        web.guest = WebGuest::Read;
    })
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let web = Config::parse(&text).unwrap().web;
    assert!(web.enabled);
    assert_eq!(web.listen, "0.0.0.0:8738".parse().unwrap());
    assert!(web.writes, "writes is not the box's to change");
    assert_eq!(web.guest, WebGuest::None);
    assert!(
        text.contains("# Whether the page may change a device"),
        "{text}"
    );
    assert_eq!(text.lines().count(), before.lines().count());
    let untouched = std::fs::read_to_string(&path).unwrap();
    Config::update_web(&path, |_| {}).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap() == untouched);
}
