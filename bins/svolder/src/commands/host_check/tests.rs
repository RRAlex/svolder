// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder host check` against a stub daemon and a fake `pw-dump` printing a fixture: the words
//! come from the judges in `svolder-ipc`, and the command prints what they say.

use svolder_host::{Asound, PlayerMatch, PwDump, RouteFacts, fill_alsa, route};
use svolder_ipc::host::setup_check;
use svolder_ipc::host::{HostView, SetupSummary, Verdict};

use super::*;
use crate::test_support::stub_daemon::stub_daemon;
use crate::test_support::{install_script, temp_dir};

/// A route fixture of `svolder-host`, as `pw-dump` printed it (or as a file derived from one).
fn fixture(name: &str) -> String {
    let path = format!(
        "{}/../../crates/svolder-host/tests/fixtures/pw-dump/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The route the fixture `name` reads as, with no ALSA card to read, as the command reads it with
/// no `--player` (a lone stream is the assumed player's).
fn route_of(name: &str) -> RouteFacts {
    let mut facts = route(&fixture(name), None, "").expect("the fixture is a dump");
    if facts.player_match == PlayerMatch::NoMpris {
        facts.player_match = PlayerMatch::Assumed;
    }
    facts
}

fn globals<'a>(transport: &'a str, socket: Option<&'a str>) -> Globals<'a> {
    Globals {
        transport,
        adapter: None,
        socket,
        device: None,
        verbose: false,
    }
}

/// A `pw-dump` that prints the fixture `name` (when given) and exits `code`, in `dir`.
fn fake_dump(dir: &std::path::Path, name: Option<&str>, code: i32) -> PwDump {
    let program = dir.join("pw-dump");
    let body = match name {
        Some(name) => {
            let file = dir.join("dump.json");
            std::fs::write(&file, fixture(name)).expect("the dump is written");
            format!("#!/bin/sh\ncat '{}'\nexit {code}\n", file.display())
        }
        None => format!("#!/bin/sh\nexit {code}\n"),
    };
    install_script(&program, &body);
    PwDump::new(program)
}

/// `setup-pipe.json` with the default sink moved to `bits_s16_tri`, the sink its player plays into
/// (the one value edited, `default.audio.sink` in the `default` metadata): the test graph of the
/// `host check` sample in docs/usage/cli.md, which the test below compares with.
fn sample_graph() -> String {
    let mut graph: serde_json::Value =
        serde_json::from_str(&fixture("setup-pipe.json")).expect("a dump");
    let mut moved = 0;
    for object in graph.as_array_mut().expect("a list").iter_mut() {
        let Some(entries) = object.get_mut("metadata").and_then(|m| m.as_array_mut()) else {
            continue;
        };
        for entry in entries
            .iter_mut()
            .filter(|e| e["key"] == "default.audio.sink")
        {
            entry["value"] = serde_json::json!({ "name": "bits_s16_tri" });
            moved += 1;
        }
    }
    assert_eq!(moved, 1, "one default sink to move");
    graph.to_string()
}

/// The `host check` sample of docs/usage/cli.md is what the command prints on [`sample_graph`].
#[tokio::test]
async fn the_sample_in_the_cli_doc_is_what_the_command_prints() {
    let home = temp_dir("host-check-sample");
    let program = home.join("pw-dump");
    let file = home.join("dump.json");
    std::fs::write(&file, sample_graph()).expect("the dump is written");
    install_script(&program, &format!("#!/bin/sh\ncat '{}'\n", file.display()));
    let said = answer(
        &globals("fake", None),
        None,
        false,
        Some("Fixture Player"),
        Some(PwDump::new(program)),
        &Asound::new(home.join("no-asound")),
    )
    .await
    .expect("read");
    let doc = format!("{}/../../docs/usage/cli.md", env!("CARGO_MANIFEST_DIR"));
    let doc = std::fs::read_to_string(&doc).unwrap_or_else(|e| panic!("{doc}: {e}"));
    let start = "$ svolder host check --player \"Fixture Player\"\n";
    let block = doc.split_once(start).expect("the sample in cli.md").1;
    let block = block.split_once("```").expect("the sample ends").0;
    assert_eq!(said.out.join("\n") + "\n", block);
}

/// With no daemon and no `--player` MPRIS is never looked at: a lone stream is the assumed
/// player's (`player assumed`), never one said to have no MPRIS player.
#[tokio::test]
async fn a_lone_stream_with_no_daemon_and_no_player_flag_is_assumed_not_no_mpris() {
    let home = temp_dir("host-check-lone");
    let dump = fake_dump(&home, Some("one-stream.json"), 0);
    let facts = own_route(Some(dump), &Asound::new(home.join("no-asound")), None)
        .await
        .expect("a route");
    assert_eq!(facts.player_match, PlayerMatch::Assumed);
    let said = lines(&facts, &[], false).join("\n");
    assert!(said.contains("assumed"), "{said}");
    assert!(!said.contains("no MPRIS"), "{said}");
}

async fn through_daemon(route: Option<RouteFacts>, json: bool) -> Result<Answer, Failure> {
    let daemon = stub_daemon().await;
    daemon.hosts(HostView {
        available: true,
        route,
        ..HostView::default()
    });
    let socket = daemon.socket.display().to_string();
    // The dump is a program that must not run: the daemon's route is the answer.
    let home = temp_dir("host-check-daemon");
    let never = fake_dump(&home, None, 77);
    answer(
        &globals("daemon", Some(&socket)),
        Some(&Config::default()),
        json,
        None,
        Some(never),
        &Asound::new(home.join("no-asound")),
    )
    .await
}

/// A route read by the daemon prints `player → computer → sink`, the verdict, and the seven
/// lines under the summary, with no `pw-dump` of this command's own.
#[tokio::test]
async fn through_the_daemon_it_prints_the_path_and_the_seven_lines() {
    let route = route_of("schiit-sink.json");
    let said = through_daemon(Some(route.clone()), false)
        .await
        .expect("read");
    assert!(
        said.out[0].starts_with("player → computer → "),
        "{:?}",
        said.out
    );
    let check = setup_check(&route);
    assert!(said.out.contains(&check.summary_words()), "{:?}", said.out);
    assert_eq!(check.lines.len(), 7);
    for line in &check.lines {
        let printed = said
            .out
            .iter()
            .find(|l| l.trim_start().starts_with(line.label()))
            .unwrap_or_else(|| panic!("{} in {:?}", line.label(), said.out));
        assert!(printed.ends_with(&line.words), "{printed}");
    }
}

/// `--json` is `{ sink, path: {verdict, end_to_end, words, sentence, dac}, config: {sink,
/// outcome, summary, from_player, to_dac, limit, lines} }`.
#[tokio::test]
async fn json_has_the_sink_the_path_and_the_config() {
    let said = through_daemon(Some(route_of("schiit-sink.json")), true)
        .await
        .expect("read");
    assert_eq!(said.out.len(), 1);
    let value: serde_json::Value = serde_json::from_str(&said.out[0]).expect("one JSON object");
    let object = value.as_object().expect("an object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["config", "path", "sink"]);
    assert!(value["sink"].is_string());
    // The keys the other JSON surfaces have: the config names its sink, the path its DAC.
    assert_eq!(value["config"]["sink"], value["sink"]);
    assert!(value["path"]["dac"].is_string(), "{value}");
    assert!(value["path"]["verdict"].is_string());
    // `end_to_end` sits beside the verdict and is one of the same keys.
    assert!(Verdict::KEYS.contains(&value["path"]["end_to_end"].as_str().unwrap()));
    assert!(value["path"]["words"].is_array());
    assert!(value["path"].as_object().unwrap().contains_key("sentence"));
    assert!(value["config"]["summary"].is_string());
    // The two lines under the sink are the judge's words for the same route.
    let io = svolder_ipc::setup_check(&route_of("schiit-sink.json")).io;
    assert_eq!(value["config"]["from_player"], io.from_player);
    assert_eq!(value["config"]["to_dac"], io.to_dac);
    assert!(value["config"]["from_player"].is_string() && value["config"]["to_dac"].is_string());
    assert!(Verdict::KEYS.contains(&value["path"]["verdict"].as_str().unwrap()));
    assert!(SetupSummary::KEYS.contains(&value["config"]["outcome"].as_str().unwrap()));
    assert_eq!(value["config"]["limit"], svolder_ipc::host::SETUP_LIMIT);
    let lines = value["config"]["lines"].as_array().expect("lines");
    assert_eq!(lines.len(), 7);
    let first = lines[0].as_object().unwrap();
    let mut first_keys: Vec<&str> = first.keys().map(String::as_str).collect();
    first_keys.sort_unstable();
    assert_eq!(first_keys, ["key", "outcome", "words"]);
    assert_eq!(lines[0]["key"], "rates");
}

/// A sink that is none of the daemon's DACs has a null `path.dac`; one that is names the DAC.
#[test]
fn json_names_the_dac_the_path_is_judged_against() {
    let route = route_of("schiit-sink.json");
    let dac = DacFacts {
        display: "Gungnir 2".to_string(),
        ..DacFacts::default()
    };
    assert!(sink_is_dac(route.sink.as_ref().unwrap(), &dac));
    let out = lines(&route, &[dac], true);
    let value: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
    assert_eq!(value["path"]["dac"], "Gungnir 2", "{value}");
    assert_eq!(value["config"]["sink"], value["sink"]);
    let out = lines(&route, &[], true);
    let value: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
    assert!(value["path"]["dac"].is_null(), "{value}");
}

/// `end_to_end` is the verdict with the DAC's own stage added: the same route through a DAC
/// whose volume is below 0 dB keeps its verdict and reads `changed` end to end.
#[test]
fn json_end_to_end_adds_the_dacs_own_stage_to_the_verdict() {
    // The fixture's own streams and volumes make its verdict `changed`; a route with the pw-cat
    // stream as the player at full volume, the sink at full volume and nothing else mixing is
    // untouched, so the DAC's stage is the only difference.
    let mut route = route(&fixture("schiit-sink.json"), Some(1_790_719), "").expect("a dump");
    let rate = route.sink.as_ref().and_then(|sink| sink.rate_hz);
    let stream = route.player.as_mut().expect("a player");
    (stream.rate_hz, stream.volume_milli, stream.muted) = (rate, Some(1000), false);
    let sink = route.sink.as_mut().expect("a sink");
    (sink.volume_milli, sink.muted) = (Some(1000), false);
    route.others_mixing = 0;
    let mut dac = DacFacts {
        display: "Gungnir 2".to_string(),
        input: Some("USB".to_string()),
        rate_hz: route.sink.as_ref().and_then(|sink| sink.rate_hz),
        ..DacFacts::default()
    };
    let said = |dac: &DacFacts| -> serde_json::Value {
        serde_json::from_str(&lines(&route, std::slice::from_ref(dac), true)[0]).unwrap()
    };
    let flat = said(&dac);
    assert_ne!(flat["path"]["verdict"], "changed", "{flat}");
    assert_eq!(
        flat["path"]["end_to_end"], flat["path"]["verdict"],
        "{flat}"
    );
    dac.stage.volume_db = Some(-3.0);
    let lowered = said(&dac);
    assert_eq!(
        lowered["path"]["verdict"], flat["path"]["verdict"],
        "{lowered}"
    );
    assert_eq!(lowered["path"]["end_to_end"], "changed", "{lowered}");
}

/// The text always names the sink the config judged (also with no hops) and ends on the limit.
#[tokio::test]
async fn the_text_names_the_sink_and_ends_on_the_limit() {
    for name in ["schiit-sink.json", "stream-elsewhere.json"] {
        let route = route_of(name);
        let out = lines(&route, &[], false);
        let sink = setup_check(&route).sink;
        assert!(out.iter().any(|l| l.contains(&sink)), "{name}: {out:?}");
        // The limit sentence closes the text, wrapped and indented.
        let at = out
            .iter()
            .position(|l| l.trim_start().starts_with("svolder judges"))
            .unwrap_or_else(|| panic!("{name}: the limit in {out:?}"));
        let limit: Vec<&str> = out[at..].iter().map(|l| l.trim()).collect();
        assert_eq!(limit.join(" "), svolder_ipc::host::SETUP_LIMIT, "{name}");
        assert!(
            out[at..]
                .iter()
                .all(|l| l.starts_with("  ") && l.chars().count() <= 100)
        );
    }
}

/// A route with no sink prints no `config judged on:` line, and none that names nothing.
#[test]
fn a_route_with_no_sink_names_no_sink() {
    let out = lines(&RouteFacts::default(), &[], false);
    assert!(!out.iter().any(|l| l.contains("judged on")), "{out:?}");
    assert!(
        out.iter().any(|l| l == "PipeWire config not read"),
        "{out:?}"
    );
}

/// A changed route says so in the words and in the summary count; a failing line is no error.
#[tokio::test]
async fn a_failing_line_is_printed_and_is_no_error() {
    let said = through_daemon(Some(route_of("narrow-sink.json")), true)
        .await
        .expect("a failing line is not an error");
    let value: serde_json::Value = serde_json::from_str(&said.out[0]).unwrap();
    assert!(
        value["config"]["summary"]
            .as_str()
            .unwrap()
            .contains("changes the sound"),
        "{value}"
    );
    assert!(
        value["config"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["outcome"] == "fail"),
        "{value}"
    );
}

/// A route with no config facts came from an older daemon: say so, and list seven lines that
/// were not read.
#[tokio::test]
async fn an_older_daemon_s_route_reads_as_older() {
    let mut route = route_of("schiit-sink.json");
    route.setup = None;
    let said = through_daemon(Some(route), false).await.expect("read");
    assert!(
        said.out
            .iter()
            .any(|l| l == "PipeWire config not read (svolderd's version differs)"),
        "{:?}",
        said.out
    );
    // The route's lone stream is the player's (no MPRIS player is named), so `From player` reads
    // it; `To DAC` has no card facts from an older daemon.
    assert!(
        said.out
            .iter()
            .any(|l| l.trim_start().starts_with("From player")
                && l.ends_with("44.1 kHz · S16LE · stereo")),
        "{:?}",
        said.out
    );
    let unread = said.out.iter().filter(|l| l.ends_with("not read")).count();
    assert_eq!(unread, 8, "the seven lines and `To DAC`: {:?}", said.out);
}

/// With no daemon, one `pw-dump` of its own gives the same words.
#[tokio::test]
async fn without_a_daemon_it_reads_the_dump_itself() {
    let home = temp_dir("host-check-own");
    let dump = fake_dump(&home, Some("schiit-sink.json"), 0);
    let said = answer(
        &globals("fake", None),
        None,
        false,
        None,
        Some(dump),
        &Asound::new(home.join("no-asound")),
    )
    .await
    .expect("read");
    let mut expected = route_of("schiit-sink.json");
    fill_alsa(&mut expected, &Asound::new(home.join("no-asound")));
    assert_eq!(said.out, lines(&expected, &[], false));
}

/// A graph that cannot be read is exit 6, the computer's controls unavailable: a dump that
/// fails, one that is not a dump, and a host with `pw-dump` off.
#[tokio::test]
async fn a_graph_that_cannot_be_read_is_exit_6() {
    let home = temp_dir("host-check-fail");
    let asound = Asound::new(home.join("no-asound"));
    let failing = fake_dump(&home, None, 1);
    let garbage = {
        let program = home.join("garbage-dump");
        install_script(&program, "#!/bin/sh\necho 'not json'\n");
        PwDump::new(program)
    };
    for dump in [Some(failing), Some(garbage), None] {
        let dump_was_none = dump.is_none();
        let failure = answer(&globals("fake", None), None, false, None, dump, &asound)
            .await
            .expect_err("no graph");
        assert_eq!(failure.code, svolder_ipc::CODE_COMPUTER_UNAVAILABLE);
        assert_eq!(failure.code, 6);
        // A process with no `pw-dump` to run says host audio is off, not that PipeWire is down.
        let off = failure.what.contains("host audio is off");
        assert_eq!(off, dump_was_none, "{}", failure.what);
    }
}

/// `--player` names a stream by pid when it is a number, else by application name; a daemon's
/// route is of the player it follows, so `--player` reads the graph itself.
#[tokio::test]
async fn player_is_a_pid_or_a_name_and_reads_the_graph_itself() {
    assert_eq!(player_of("1234"), (Some(1234), String::new()));
    assert_eq!(player_of("mpv"), (None, "mpv".to_string()));
    assert_eq!(
        player_of(" Fixture Other "),
        (None, "Fixture Other".to_string())
    );

    let daemon = stub_daemon().await;
    daemon.hosts(HostView {
        available: true,
        route: Some(route_of("schiit-sink.json")),
        ..HostView::default()
    });
    let socket = daemon.socket.display().to_string();
    let home = temp_dir("host-check-player");
    let dump = fake_dump(&home, Some("two-streams.json"), 0);
    let said = answer(
        &globals("daemon", Some(&socket)),
        Some(&Config::default()),
        true,
        Some("Fixture Other"),
        Some(dump),
        &Asound::new(home.join("no-asound")),
    )
    .await
    .expect("read");
    // two-streams.json plays to the HDMI sink, schiit-sink.json to the USB one: the answer is
    // the dump's.
    let value: serde_json::Value = serde_json::from_str(&said.out[0]).unwrap();
    assert!(
        !value["sink"].as_str().unwrap().contains("Gungnir"),
        "{value}"
    );
}

/// What in `text` (the whole of the file `name`, its comment lines left out) writes PipeWire's
/// state, writes a file, or runs a process: the only process these files may reach is
/// `pw-dump`, through `output_within` in route.rs.
fn forbidden_in(name: &str, text: &str) -> Vec<String> {
    let words = [
        "pw-metadata",
        "pw-cli set",
        "wpctl set-",
        "fs::write",
        "File::create",
        "OpenOptions",
        "Command",
        ".spawn(",
    ];
    let mut found = Vec::new();
    for (at, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for word in words {
            if line.contains(word) {
                found.push(format!("{name}:{} names {word}", at + 1));
            }
        }
        if line.contains("output_within(") && !name.ends_with("/route.rs") {
            found.push(format!("{name}:{} runs a process", at + 1));
        }
    }
    found
}

/// The scan sees a forbidden line anywhere in a file, past a `#[cfg(test)] mod tests;` near its
/// top, and lets the comments that name the words through.
#[test]
fn the_scan_of_the_read_only_files_catches_what_it_guards() {
    let file = "//! `pw-metadata` is never run here.\n#[cfg(test)]\nmod tests;\nfn a() {}\n";
    assert_eq!(forbidden_in("a.rs", file), Vec::<String>::new());
    for line in [
        "std::process::Command::new(\"pw-metadata\")",
        "Command::new(\"wpctl set-volume\")",
        "std::fs::write(path, text)",
        "let f = File::create(path);",
        "OpenOptions::new()",
        "output_within(program, &[], t).await",
    ] {
        let file = format!("#[cfg(test)]\nmod tests;\nfn a() {{}}\n{line}\n");
        assert!(!forbidden_in("a.rs", &file).is_empty(), "{line}");
    }
    assert!(forbidden_in("crates/x/route.rs", "output_within(p, &[], t)").is_empty());
}

/// The read is read-only: the files of the check hold no command that writes PipeWire's state,
/// no file write and no process but `pw-dump`.
#[test]
fn the_files_of_the_check_never_write_pipewire() {
    let root = env!("CARGO_MANIFEST_DIR");
    let mut files = vec![
        format!("{root}/../../crates/svolder-host/src/route.rs"),
        format!("{root}/../../crates/svolder-ipc/src/host/path.rs"),
        format!("{root}/../../crates/svolder-ipc/src/host/sample.rs"),
        format!("{root}/../../crates/svolder-ipc/src/host/setup.rs"),
        format!("{root}/src/commands/host_check.rs"),
        format!("{root}/src/tui/run/setup.rs"),
    ];
    let mut folders = vec![format!("{root}/../../crates/svolder-host/src/route")];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).expect("the route folder") {
            let path = entry.expect("an entry").path();
            match path.is_dir() {
                true => folders.push(path.display().to_string()),
                false => files.push(path.display().to_string()),
            }
        }
    }
    files.sort();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("{file}: {e}"));
        // The fixtures a route test reads are read, never written: tests are scanned for the
        // PipeWire words only.
        let found = forbidden_in(file, &text);
        let found: Vec<&String> = found
            .iter()
            .filter(|f| !file.ends_with("tests.rs") || f.contains("pw-") || f.contains("wpctl"))
            .collect();
        assert!(found.is_empty(), "{found:?}");
    }
    assert!(files.len() >= 5, "{files:?}");
}

/// A daemon on a socket of the case's own that answers the three requests the check makes: the
/// route, the devices (one DAC) and that DAC's readings, with its input on `input`.
fn daemon_with_dac(route: RouteFacts, input: &str) -> std::path::PathBuf {
    let registry = crate::registry().expect("the profiles");
    let profile = registry.get("gungnir2-v2").expect("the Gungnir 2 profile");
    let control = profile.control_by_key("input").expect("an input control");
    let raw = (0..=u8::MAX)
        .find(|raw| control.value_name(*raw) == Some(input))
        .unwrap_or_else(|| panic!("an input named {input}"));
    let reading = svolder_ipc::ValueObject {
        id: control.id.to_string(),
        key: Some("input".into()),
        label: "Input".into(),
        value_type: "u8".into(),
        raw: serde_json::json!(raw),
        display: input.into(),
        value: None,
        unit: None,
        heard_at_ms: None,
    };
    let device = svolder_ipc::DeviceInfo {
        name: "dac".into(),
        unit: "0102030405060708".into(),
        product_id: 2,
        product: Some("Gungnir 2".into()),
        firmware: 3,
        profile: "gungnir2-v2".into(),
        connected: true,
        stale: false,
        retry_in_secs: None,
        passphrase_note: None,
        refused: false,
        recalled: None,
        signal: None,
        key_readings: Vec::new(),
        adapter_hint: None,
    };
    let socket = crate::test_support::short_socket("svolder-check-fake");
    let _ = std::fs::remove_file(&socket);
    let listener = tokio::net::UnixListener::bind(&socket).expect("the socket binds");
    tokio::spawn(async move {
        use svolder_ipc::socket::protocol::{Reply, Response};
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        while let Ok((stream, _)) = listener.accept().await {
            let (route, device, reading) = (route.clone(), device.clone(), reading.clone());
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut read = tokio::io::BufReader::new(read);
                let mut line = String::new();
                while read.read_line(&mut line).await.unwrap_or(0) > 0 {
                    let sent: serde_json::Value = serde_json::from_str(&line).expect("a request");
                    let id = sent["id"].as_u64().expect("an id");
                    let response = match sent["request"]["cmd"].as_str() {
                        Some("host") => Response::Host(HostView {
                            available: true,
                            route: Some(route.clone()),
                            ..HostView::default()
                        }),
                        Some("devices") => Response::Devices {
                            devices: vec![device.clone()],
                        },
                        Some("state") => Response::Values {
                            device: "dac".into(),
                            values: vec![reading.clone()],
                        },
                        other => panic!("unexpected request {other:?}"),
                    };
                    let reply =
                        svolder_ipc::socket::protocol::to_line(&Reply::Response { id, response })
                            .expect("a reply line");
                    let _ = write.write_all(reply.as_bytes()).await;
                    line.clear();
                }
            });
        }
    });
    socket
}

/// The DAC's verdict through the daemon: a Gungnir 2 as the sink with its input on Optical gets
/// `other_input`, as the other front ends say; with its input on USB the verdict is not that.
#[tokio::test]
async fn through_the_daemon_a_dac_sink_gets_the_dac_s_verdict() {
    let home = temp_dir("host-check-dac");
    let asound = Asound::new(home.join("no-asound"));
    let verdict = |said: &Answer| {
        let value: serde_json::Value = serde_json::from_str(&said.out[0]).unwrap();
        value["path"]["verdict"].as_str().unwrap().to_string()
    };
    for (input, other_input) in [("Optical", true), ("USB 1", false)] {
        let socket = daemon_with_dac(route_of("schiit-sink.json"), input);
        let socket_text = socket.display().to_string();
        let said = answer(
            &globals("daemon", Some(&socket_text)),
            Some(&Config::default()),
            true,
            None,
            Some(fake_dump(&home, None, 77)),
            &asound,
        )
        .await
        .expect("read");
        assert_eq!(
            verdict(&said) == "other_input",
            other_input,
            "{input}: {:?}",
            said.out
        );
        let _ = std::fs::remove_file(&socket);
    }
}
