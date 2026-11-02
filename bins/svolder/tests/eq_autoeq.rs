// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder eq search` and `svolder eq import --autoeq`, against an index and files served by
//! a listener of the test's own on 127.0.0.1: nothing here reaches a real network address.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

mod common;

use common::TempPath;

const PEQ: &str = "Preamp: -6.1 dB\r\n\
    Filter 1: ON LSC Fc 105 Hz Gain 6.4 dB Q 0.70\r\n\
    Filter 2: ON PK Fc 8800 Hz Gain 5.1 dB Q 1.42\r\n\
    Filter 3: ON LP Fc 18000 Hz\r\n";

const HD650: &str = "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990\n\
- [Sennheiser HD 650](./crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650) by crinacle on GRAS 43AG-7\n\
- [Sennheiser HD 650](./Rtings/over-ear/Sennheiser%20HD%20650) by Rtings\n";

const INDEX_HEAD: &str = "This is a list of all equalization profiles.\n\n";

/// A 127.0.0.1 server: `route` maps a request path to a body, or to nothing for a 404. Every
/// path asked for is recorded.
struct Server {
    url: String,
    paths: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    addr: std::net::SocketAddr,
}

impl Server {
    fn start(route: impl Fn(&str) -> Option<String> + Send + Sync + 'static) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let paths = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let route = Arc::new(route);
        let (paths2, stop2) = (paths.clone(), stop.clone());
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stop2.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let (route, paths) = (route.clone(), paths2.clone());
                thread::spawn(move || {
                    let mut head = Vec::new();
                    let mut byte = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        match stream.read(&mut byte) {
                            Ok(1) => head.push(byte[0]),
                            _ => return,
                        }
                    }
                    let head = String::from_utf8_lossy(&head).into_owned();
                    let path = head.split(' ').nth(1).unwrap_or("/").to_string();
                    paths.lock().unwrap().push(path.clone());
                    let (status, body) = match route(&path) {
                        Some(body) => ("200 OK", body),
                        None => ("404 Not Found", String::new()),
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        Server {
            url: format!("http://{addr}/results/"),
            paths,
            stop,
            addr,
        }
    }

    /// A server with `index` (after the usual three header lines) and every file as `PEQ`.
    fn with_index(index: String) -> Server {
        Server::start(move |path| match path {
            "/results/INDEX.md" => Some(format!("{INDEX_HEAD}{index}")),
            p if p.ends_with("ParametricEQ.txt") => Some(PEQ.to_string()),
            _ => None,
        })
    }

    fn paths(&self) -> Vec<String> {
        self.paths.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::net::TcpStream::connect(self.addr);
    }
}

fn scratch(what: &str) -> TempPath {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    TempPath(std::env::temp_dir().join(format!(
        "svolder-eq-autoeq-{what}-{}-{seq}",
        std::process::id()
    )))
}

/// Run the CLI over the fake transport with `home` as every XDG directory and `url` as the
/// AutoEq base.
fn svolder(args: &[&str], home: &std::path::Path, url: &str) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", url)
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", home.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", home.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", home.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", home.join("units"))
        .env("XDG_CACHE_HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_RUNTIME_DIR", home)
        .env("XDG_STATE_HOME", home)
        .env_remove("HTTP_PROXY")
        .env_remove("http_proxy")
        .env_remove("ALL_PROXY")
        .args(args)
        .output()
        .expect("the binary runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn config_text(home: &std::path::Path) -> String {
    std::fs::read_to_string(home.join("svolder").join("config.toml")).unwrap_or_default()
}

/// `n` distinct one-measurement headphones named `Phone 01`, `Phone 02`, ...
fn phones(n: usize) -> String {
    (1..=n)
        .map(|i| {
            format!("- [Phone {i:02}](./oratory1990/over-ear/Phone%20{i:02}) by oratory1990\n")
        })
        .collect()
}

#[test]
fn search_prints_one_line_per_headphone_with_the_others_counted() {
    let server = Server::with_index(format!(
        "{HD650}- [Sennheiser HD 6XX](./oratory1990/over-ear/Sennheiser%20HD%206XX) by oratory1990\n"
    ));
    let home = scratch("search");
    let (code, out, err) = svolder(&["eq", "search", "hd650"], &home.0, &server.url);
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(out, "Sennheiser HD 650  oratory1990 · over-ear  +2 more\n");
    assert_eq!(err, "");
    assert!(home.0.join("svolder/autoeq/INDEX.md").exists());

    let (code, out, _) = svolder(
        &["eq", "search", "sennheiser", "--all"],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0);
    assert_eq!(
        out,
        "Sennheiser HD 650  oratory1990 · over-ear\n\
         Sennheiser HD 650  crinacle · over-ear · GRAS 43AG-7\n\
         Sennheiser HD 650  Rtings · over-ear\n\
         Sennheiser HD 6XX  oratory1990 · over-ear\n"
    );
    // The second search read the cache.
    let index_fetches = server
        .paths()
        .iter()
        .filter(|p| p.ends_with("INDEX.md"))
        .count();
    assert_eq!(index_fetches, 1, "{:?}", server.paths());

    let (code, out, _) = svolder(
        &["eq", "search", "hd", "650", "--json"],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0);
    assert_eq!(
        out.lines().next().unwrap(),
        r#"{"form":"over-ear","more":2,"name":"Sennheiser HD 650","rig":null,"source":"oratory1990"}"#
    );
}

#[test]
fn search_stops_at_thirty_lines() {
    let server = Server::with_index(phones(40));
    let home = scratch("search-cap");
    let (code, out, err) = svolder(&["eq", "search", "phone"], &home.0, &server.url);
    assert_eq!(code, 0, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 31, "{out}");
    assert_eq!(lines[0], "Phone 01  oratory1990 · over-ear");
    assert_eq!(lines[30], "… 10 more: narrow the search");
    let (_, json, _) = svolder(&["eq", "search", "phone", "--json"], &home.0, &server.url);
    assert_eq!(json.lines().count(), 40, "json is not capped");
}

#[test]
fn search_refresh_with_no_query_only_refreshes() {
    let server = Server::with_index(phones(3));
    let home = scratch("search-refresh");
    let (code, out, err) = svolder(&["eq", "search", "--refresh"], &home.0, &server.url);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "");
    assert_eq!(err, "svolder: refreshed the AutoEq index (3 headphones)\n");
    svolder(
        &["eq", "search", "--refresh", "phone"],
        &home.0,
        &server.url,
    );
    let index_fetches = server
        .paths()
        .iter()
        .filter(|p| p.ends_with("INDEX.md"))
        .count();
    assert_eq!(index_fetches, 2);
}

#[test]
fn search_without_a_query_or_a_match_refuses() {
    let server = Server::with_index(phones(3));
    let home = scratch("search-none");
    let (code, out, err) = svolder(&["eq", "search"], &home.0, &server.url);
    assert_eq!((code, out.as_str()), (1, ""), "{err}");
    assert!(err.contains("give a headphone name"), "{err}");
    let (code, _, err) = svolder(&["eq", "search", "zzz"], &home.0, &server.url);
    assert_eq!(code, 1);
    assert!(err.contains("no headphone matches \"zzz\""), "{err}");
}

#[test]
fn search_without_a_network_names_the_file_import() {
    let home = scratch("search-offline");
    let (code, out, err) = svolder(&["eq", "search", "hd650"], &home.0, "http://127.0.0.1:9/");
    assert_eq!((code, out.as_str()), (1, ""), "{err}");
    assert!(err.contains("no cached AutoEq index"), "{err}");
    assert!(err.contains("svolder eq import FILE"), "{err}");
}

#[test]
fn import_autoeq_saves_the_one_match_with_its_origin() {
    let server = Server::with_index(phones(2));
    let home = scratch("import-one");
    let (code, out, err) = svolder(
        &["eq", "import", "--autoeq", "phone02"],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(out, "");
    assert_eq!(
        err,
        "svolder: imported \"Phone 02\" (2 filters, preamp -6.1 dB, 1 skipped)\n"
    );
    let config = config_text(&home.0);
    assert!(
        config.contains("name = \"Phone 02\"\norigin = \"AutoEq · oratory1990 · over-ear\"\n"),
        "{config}"
    );
    assert!(server.paths().contains(
        &"/results/oratory1990/over-ear/Phone%2002/Phone%2002%20ParametricEQ.txt".to_string()
    ));

    let (code, _, err) = svolder(
        &["eq", "import", "--autoeq", "phone02"],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 1);
    assert!(err.contains("already exists"), "{err}");
    let (code, _, err) = svolder(
        &[
            "eq", "import", "--autoeq", "phone02", "--name", "Mine", "--force",
        ],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("imported \"Mine\""), "{err}");
}

#[test]
fn import_autoeq_lists_several_matches_and_saves_nothing() {
    let server = Server::with_index(phones(3));
    let home = scratch("import-many");
    let (code, out, err) = svolder(&["eq", "import", "--autoeq", "phone"], &home.0, &server.url);
    assert_eq!((code, out.as_str()), (1, ""), "{err}");
    assert!(err.contains("\"phone\" matches 3 headphones:"), "{err}");
    for name in ["Phone 01", "Phone 02", "Phone 03"] {
        assert!(
            err.contains(&format!("  {name}\n")) || err.contains(&format!("  {name}")),
            "{err}"
        );
    }
    assert!(!config_text(&home.0).contains("[[preset]]"));
    assert!(
        !server
            .paths()
            .iter()
            .any(|p| p.ends_with("ParametricEQ.txt"))
    );
}

#[test]
fn import_autoeq_measured_by_picks_the_source() {
    let server = Server::with_index(HD650.to_string());
    let home = scratch("import-by");
    let (code, _, err) = svolder(
        &[
            "eq",
            "import",
            "--autoeq",
            "hd650",
            "--measured-by",
            "CRINACLE",
        ],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0, "{err}");
    assert!(server.paths().contains(
        &"/results/crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650/Sennheiser%20HD%20650%20ParametricEQ.txt"
            .to_string()
    ));
    assert!(
        config_text(&home.0).contains("origin = \"AutoEq · crinacle · over-ear · GRAS 43AG-7\""),
        "{}",
        config_text(&home.0)
    );

    let (code, _, err) = svolder(
        &[
            "eq",
            "import",
            "--autoeq",
            "hd650",
            "--measured-by",
            "nobody",
            "--name",
            "x",
        ],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 1);
    assert!(
        err.contains("the sources are oratory1990, crinacle, Rtings"),
        "{err}"
    );

    // With no --measured-by the first line, the recommended one, is taken.
    let (code, _, err) = svolder(
        &["eq", "import", "--autoeq", "hd650", "--name", "rec"],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0, "{err}");
    assert!(config_text(&home.0).contains("origin = \"AutoEq · oratory1990 · over-ear\""));
}

#[test]
fn import_autoeq_names_a_missing_file() {
    let server = Server::start(|path| match path {
        "/results/INDEX.md" => Some(format!("{INDEX_HEAD}{}", phones(1))),
        _ => None,
    });
    let home = scratch("import-404");
    let (code, out, err) = svolder(&["eq", "import", "--autoeq", "phone"], &home.0, &server.url);
    assert_eq!((code, out.as_str()), (1, ""), "{err}");
    assert!(err.contains("answered 404"), "{err}");
    assert!(!err.contains("--measured-by"), "{err}");
    assert!(!config_text(&home.0).contains("[[preset]]"));
}

#[test]
fn import_autoeq_refuses_a_file_that_is_not_an_eq() {
    let server = Server::start(|path| match path {
        "/results/INDEX.md" => Some(format!("{INDEX_HEAD}{}", phones(1))),
        _ => Some("<html>hello</html>".into()),
    });
    let home = scratch("import-junk");
    let (code, _, err) = svolder(&["eq", "import", "--autoeq", "phone"], &home.0, &server.url);
    assert_eq!(code, 1);
    assert!(err.contains("not a parametric EQ"), "{err}");
}

#[test]
fn import_wants_exactly_one_source() {
    let home = scratch("import-args");
    let url = "http://127.0.0.1:9/";
    for args in [
        &["eq", "import"][..],
        &["eq", "import", "a.txt", "--autoeq", "x"][..],
        &["eq", "import", "a.txt", "--measured-by", "x"][..],
    ] {
        let (code, _, err) = svolder(args, &home.0, url);
        assert_eq!(code, 1, "{args:?}: {err}");
        assert!(
            err.starts_with("error:") || err.contains("--measured-by goes with --autoeq"),
            "{args:?}: {err}"
        );
    }
}

#[test]
fn import_autoeq_takes_an_exact_name_over_longer_ones() {
    let server = Server::with_index(
        "- [HD 650 mod](./x/over-ear/HD%20650%20mod) by x\n\
         - [HD 650](./x/over-ear/HD%20650) by x\n"
            .to_string(),
    );
    let home = scratch("import-exact");
    let (code, _, err) = svolder(
        &["eq", "import", "--autoeq", "hd 650"],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0, "{err}");
    assert!(err.starts_with("svolder: imported \"HD 650\" ("), "{err}");
    let (code, _, err) = svolder(&["eq", "import", "--autoeq", "650"], &home.0, &server.url);
    assert_eq!(code, 1);
    assert!(err.contains("matches 2 headphones"), "{err}");
}

#[test]
fn search_pads_the_name_column() {
    let server = Server::with_index(
        "- [HD 6](./a/over-ear/HD%206) by a\n\
         - [HD 6 Long Name](./a/over-ear/HD%206%20Long%20Name) by a\n\
         - [HD 6 Long Name](./b/in-ear/HD%206%20Long%20Name) by b\n"
            .to_string(),
    );
    let home = scratch("search-pad");
    let (_, out, _) = svolder(&["eq", "search", "hd6"], &home.0, &server.url);
    assert_eq!(
        out,
        "HD 6            a · over-ear\nHD 6 Long Name  a · over-ear  +1 more\n"
    );
    let (_, out, _) = svolder(&["eq", "search", "hd6", "--all"], &home.0, &server.url);
    assert_eq!(
        out,
        "HD 6            a · over-ear\nHD 6 Long Name  a · over-ear\nHD 6 Long Name  b · in-ear\n"
    );
}

#[test]
fn a_404_offers_measured_by_only_with_another_source() {
    let server = Server::start(|path| match path {
        "/results/INDEX.md" => Some(format!("{INDEX_HEAD}{HD650}")),
        _ => None,
    });
    let home = scratch("import-404-many");
    let (code, _, err) = svolder(&["eq", "import", "--autoeq", "hd650"], &home.0, &server.url);
    assert_eq!(code, 1);
    assert!(err.contains("--measured-by"), "{err}");
}

#[test]
fn a_failed_refresh_prints_from_the_cache_and_exits_non_zero() {
    let good = Server::with_index(phones(2));
    let bad = Server::start(|_| None);
    let home = scratch("search-refresh-fails");
    svolder(&["eq", "search", "phone"], &home.0, &good.url);
    let (code, out, err) = svolder(&["eq", "search", "phone", "--refresh"], &home.0, &bad.url);
    assert_eq!(code, 1, "{err}");
    assert_eq!(out.lines().count(), 2, "{out}");
    assert!(err.contains("(refresh failed: "), "{err}");
    // An automatic refresh that fails keeps succeeding.
    let path = home.0.join("svolder/autoeq/INDEX.md");
    let old = std::fs::File::options().write(true).open(&path).unwrap();
    old.set_modified(
        std::time::SystemTime::now() - std::time::Duration::from_secs(200 * 24 * 3600),
    )
    .unwrap();
    let (code, out, err) = svolder(&["eq", "search", "phone"], &home.0, &bad.url);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.lines().count(), 2);
    let (code, _, _) = svolder(&["eq", "search", "--refresh"], &home.0, &bad.url);
    assert_eq!(code, 1);
}

#[test]
fn import_autoeq_takes_the_literal_name_when_two_names_read_alike() {
    let server = Server::with_index(
        "- [Cambridge Audio Melomania 1](./oratory1990/over-ear/Cambridge%20Audio%20Melomania%201) by oratory1990\n\
         - [Cambridge Audio Melomania 1+](./oratory1990/over-ear/Cambridge%20Audio%20Melomania%201%2B) by oratory1990\n"
            .to_string(),
    );
    let home = scratch("import-alike");
    for (asked, saved, file) in [
        (
            "Cambridge Audio Melomania 1+",
            "Cambridge Audio Melomania 1+",
            "Melomania%201%2B",
        ),
        (
            "Cambridge Audio Melomania 1",
            "Cambridge Audio Melomania 1",
            "Melomania%201/",
        ),
    ] {
        let (code, _, err) = svolder(&["eq", "import", "--autoeq", asked], &home.0, &server.url);
        assert_eq!(code, 0, "{asked}: {err}");
        assert!(err.contains(&format!("imported \"{saved}\"")), "{err}");
        assert!(
            server.paths().iter().any(|p| p.contains(file)),
            "{file} in {:?}",
            server.paths()
        );
    }
    // A query that is neither is still ambiguous.
    let (code, _, err) = svolder(
        &["eq", "import", "--autoeq", "melomania1"],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 1);
    assert!(err.contains("matches 2 headphones"), "{err}");
}

#[test]
fn import_autoeq_merges_names_that_differ_only_in_case() {
    let server = Server::with_index(
        "- [Beyerdynamic DT 770 Pro (32 Ohm)](./oratory1990/over-ear/Beyerdynamic%20DT%20770%20Pro%20%2832%20Ohm%29) by oratory1990\n\
         - [Beyerdynamic DT 770 Pro (32 ohm)](./crinacle/over-ear/Beyerdynamic%20DT%20770%20Pro%20%2832%20ohm%29) by crinacle\n"
            .to_string(),
    );
    let home = scratch("import-case");
    // The literal name wins.
    let (code, _, err) = svolder(
        &[
            "eq",
            "import",
            "--autoeq",
            "Beyerdynamic DT 770 Pro (32 ohm)",
        ],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0, "{err}");
    // Ignoring case, the two are one headphone with two measurements.
    let (code, _, err) = svolder(
        &[
            "eq",
            "import",
            "--autoeq",
            "BEYERDYNAMIC DT 770 PRO (32 OHM)",
            "--measured-by",
            "crinacle",
            "--name",
            "Merged",
        ],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("imported \"Merged\""), "{err}");
    assert!(
        server.paths().iter().any(|p| p.contains("crinacle")),
        "{:?}",
        server.paths()
    );
}

#[test]
fn import_autoeq_measured_by_picks_among_headphones_before_refusing() {
    let server = Server::with_index(
        "- [Koss UR 20](./oratory1990/over-ear/Koss%20UR%2020) by oratory1990\n\
         - [Koss UR20](./crinacle/over-ear/Koss%20UR20) by crinacle\n"
            .to_string(),
    );
    let home = scratch("import-measured");
    let (code, _, err) = svolder(&["eq", "import", "--autoeq", "ur20"], &home.0, &server.url);
    assert_eq!(code, 1);
    assert!(err.contains("matches 2 headphones"), "{err}");
    let (code, _, err) = svolder(
        &[
            "eq",
            "import",
            "--autoeq",
            "ur20",
            "--measured-by",
            "crinacle",
        ],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("imported \"Koss UR20\""), "{err}");
}

#[test]
fn import_autoeq_literal_name_still_merges_the_case_spellings_for_measured_by() {
    let server = Server::with_index(
        "- [Beyerdynamic DT 770 Pro (32 Ohm)](./oratory1990/over-ear/Beyerdynamic%20DT%20770%20Pro%20%2832%20Ohm%29) by oratory1990\n\
         - [Beyerdynamic DT 770 Pro (32 ohm)](./crinacle/over-ear/Beyerdynamic%20DT%20770%20Pro%20%2832%20ohm%29) by crinacle\n"
            .to_string(),
    );
    let home = scratch("import-case-literal");
    let (code, _, err) = svolder(
        &[
            "eq",
            "import",
            "--autoeq",
            "Beyerdynamic DT 770 Pro (32 Ohm)",
            "--measured-by",
            "crinacle",
        ],
        &home.0,
        &server.url,
    );
    assert_eq!(code, 0, "{err}");
    assert!(server.paths().iter().any(|p| p.contains("crinacle")));
}
