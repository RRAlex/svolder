// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Mutex;

use super::*;

/// One fix, as a check hands it over.
fn fix(command: &str, applies_itself: bool) -> Fix {
    Fix {
        command: command.to_string(),
        applies_itself,
    }
}

/// One outcome, as a check hands it over.
fn outcome(check: &'static str, state: State, fix: Option<Fix>) -> Outcome {
    Outcome {
        check,
        state,
        reason: "because".into(),
        fix,
    }
}

/// An answer given before the question was asked, and the question it was given to.
struct FakeAsk {
    /// What the answer is.
    answer: bool,
    /// What was asked, once it has been.
    asked: Mutex<Option<String>>,
}

impl FakeAsk {
    /// Someone who will answer `answer` to whatever they are asked.
    fn saying(answer: bool) -> FakeAsk {
        FakeAsk {
            answer,
            asked: Mutex::new(None),
        }
    }

    /// What was asked, or nothing when nothing was.
    fn asked(&self) -> Option<String> {
        self.asked.lock().expect("the question").clone()
    }
}

impl Ask for FakeAsk {
    fn yes_no(&self, question: &str) -> std::io::Result<bool> {
        *self.asked.lock().expect("the question") = Some(question.to_string());
        Ok(self.answer)
    }
}

/// One device heard, as a window reports it.
fn seen(udid: &str, rssi: Option<i16>) -> ScanEntry {
    ScanEntry {
        unit: udid.to_string(),
        product: Some("Gungnir 2".into()),
        product_id: 2,
        firmware: 3,
        passphrase: false,
        rssi,
    }
}

/// Only the fixes this verb can apply itself are offered, only on a terminal or under
/// `--yes`, and only a yes applies them.
#[test]
fn only_unprivileged_fixes_are_offered_and_only_on_a_terminal_or_with_yes() {
    let fixes = vec![
        fix("sudo systemctl enable --now bluetooth", false),
        fix(DAEMON_INSTALL, true),
    ];
    let yes = FakeAsk::saying(true);
    assert_eq!(offer_with(&yes, &fixes, false, false), Vec::new());
    assert_eq!(yes.asked(), None, "a pipe was asked a question");
    assert_eq!(
        offer_with(&yes, &fixes, true, false),
        vec![fix(DAEMON_INSTALL, true)]
    );
    assert!(
        yes.asked().expect("a terminal is asked").contains("[y/N]"),
        "{:?}",
        yes.asked()
    );

    let no = FakeAsk::saying(false);
    assert_eq!(offer_with(&no, &fixes, true, false), Vec::new());

    // `--yes` is the answer, so nothing is asked and nothing needs a terminal.
    let never = FakeAsk::saying(false);
    assert_eq!(
        offer_with(&never, &fixes, false, true),
        vec![fix(DAEMON_INSTALL, true)]
    );
    assert_eq!(never.asked(), None);
}

/// Nothing to apply is nothing to ask about.
#[test]
fn a_run_with_no_fixes_of_its_own_asks_nothing() {
    let ask = FakeAsk::saying(true);
    let root = vec![fix("sudo systemctl enable --now bluetooth", false)];
    assert_eq!(offer_with(&ask, &root, true, false), Vec::new());
    assert_eq!(ask.asked(), None);
}

/// The question names how many fixes there are and lists them, so an answer is given to
/// something the user has read.
#[test]
fn the_question_lists_the_fixes_it_is_about() {
    let asked = question(&[fix(DAEMON_INSTALL, true), fix(SETUP, true)]);
    assert!(asked.contains("2 fixes"), "{asked}");
    assert!(
        asked.contains(DAEMON_INSTALL) && asked.contains(SETUP),
        "{asked}"
    );
    assert!(question(&[fix(SETUP, true)]).contains("1 fix "), "{asked}");
}

/// A device the windows heard passes, with how loud it was and whether a daemon holds it;
/// one they did not warns, with the step that follows as its fix.
#[test]
fn a_device_row_says_whether_it_was_seen_and_what_to_do_when_it_was_not() {
    let udid = "0102030405060708";
    let row = device_row("amp", udid, Ok(vec![seen(udid, Some(-62))]), false);
    assert_eq!(row.check, DEVICES);
    assert_eq!(row.state, State::Pass);
    assert_eq!(row.reason, "amp: seen, -62 dBm");
    assert!(row.fix.is_none());

    let quiet = Failure::of(Kind::NothingInRange, format!("no device {udid} seen"));
    let missing = device_row("amp", udid, Err(quiet), false);
    assert_eq!(missing.state, State::Warn);
    assert_eq!(
        missing.reason,
        format!("amp: not seen in {WINDOWS} windows")
    );
    let fix = missing.fix.expect("a step to take");
    assert_eq!(fix.command, next_step(Kind::NothingInRange));
    assert!(!fix.applies_itself, "a scan is not doctor's to run");
}

/// A device the daemon holds passes on the daemon's own word, whatever the windows heard —
/// a device a central holds stops advertising, so a scan for it hears nothing and that is
/// no trouble to report.
#[test]
fn a_device_the_daemon_holds_passes_whatever_a_scan_heard() {
    let udid = "0102030405060708";
    let quiet = Failure::of(Kind::NothingInRange, format!("no device {udid} seen"));
    let held = device_row("amp", udid, Err(quiet), true);
    assert_eq!(held.state, State::Pass);
    assert_eq!(
        held.reason,
        "amp: connected (held by the daemon; not advertising)"
    );
    assert!(held.fix.is_none(), "{held:?}");

    // And a device the windows did hear while the daemon held it reads the same way: what
    // the daemon says is the whole of the answer.
    let both = device_row("amp", udid, Ok(vec![seen(udid, Some(-40))]), true);
    assert_eq!(both.reason, held.reason);
}

/// With no device to look for there is no row about the devices there are none of, and
/// neither case reaches a backend.
#[tokio::test]
async fn a_run_with_no_device_to_look_for_says_nothing_about_devices() {
    let g = Globals {
        transport: "fake",
        adapter: None,
        socket: None,
        device: None,
        verbose: false,
    };
    assert_eq!(device_rows(&g, None).await, Vec::new());
    assert_eq!(device_rows(&g, Some(&Config::default())).await, Vec::new());
}

/// A host with no configuration file gets the row that offers it one, naming the file this
/// front end would write; a host that has one gets no such row.
#[test]
fn the_config_file_row_appears_only_when_there_is_no_file() {
    let path = Path::new("/tmp/nowhere/config.toml");
    let row = config_file_row(path, true).expect("a row about the missing file");
    assert_eq!(row.check, CONFIG_FILE);
    assert_eq!(row.state, State::Warn);
    assert_eq!(row.reason, "there is no config file yet");
    let fix = row.fix.expect("the file to write");
    assert!(fix.command.contains(&path.display().to_string()), "{fix:?}");
    assert!(fix.applies_itself);
    assert_eq!(config_file_row(path, false), None);
}

/// A search that failed for a reason of its own says that reason, and drops a step that
/// would send the reader back to the verb they are running.
#[test]
fn a_device_row_that_could_not_be_looked_for_says_why() {
    let no_adapter = Failure::of(Kind::AdapterMissing, "no adapter named hci9");
    let row = device_row("amp", "0102030405060708", Err(no_adapter), false);
    assert_eq!(row.state, State::Warn);
    assert_eq!(row.reason, "amp: no adapter named hci9");
    assert!(row.fix.is_none(), "{row:?}");
}

/// The grade row lists every adapter's catch rate, and warns with the one-line config
/// change when the configured adapter is not the best of them.
#[test]
fn the_grade_row_warns_when_the_configured_adapter_is_not_the_best() {
    let grades = vec![
        Grade {
            adapter: "hci0".into(),
            seen: 2,
            of: 5,
            note: None,
        },
        Grade {
            adapter: "hci1".into(),
            seen: 5,
            of: 5,
            note: None,
        },
    ];
    let worse = grade_row(&grades, Some("hci0"), "amp");
    assert_eq!(worse.check, ADAPTER_GRADE);
    assert_eq!(worse.state, State::Warn);
    assert!(worse.reason.starts_with("hci0 2/5, hci1 5/5"), "{worse:?}");
    let fix = worse.fix.expect("the config change");
    assert_eq!(fix.command, "set adapter = \"hci1\" in config.toml");
    assert!(fix.applies_itself);

    let best = grade_row(&grades, Some("hci1"), "amp");
    assert_eq!(best.state, State::Pass);
    assert!(best.fix.is_none());
}

/// Adapters that caught the device equally often are a tie the first of them wins, so the
/// adapter this row would write is the one the adapters row names; a later adapter that
/// caught it more often still wins.
#[test]
fn a_tie_goes_to_the_first_adapter_and_a_better_one_still_wins() {
    let all = |seen: [u8; 3]| {
        ["hci0", "hci1", "hci2"]
            .iter()
            .zip(seen)
            .map(|(adapter, seen)| Grade {
                adapter: (*adapter).to_string(),
                seen,
                of: 5,
                note: None,
            })
            .collect::<Vec<_>>()
    };
    let tied = grade_row(&all([5, 5, 5]), Some("hci9"), "amp");
    assert_eq!(tied.state, State::Warn);
    assert_eq!(
        tied.fix.expect("the config change").command,
        "set adapter = \"hci0\" in config.toml"
    );

    let later = grade_row(&all([3, 3, 5]), Some("hci9"), "amp");
    assert_eq!(
        later.fix.expect("the config change").command,
        "set adapter = \"hci2\" in config.toml"
    );
}

/// An adapter that heard nothing carries the reason it gave, and a device no adapter heard
/// at all is the tie where every one of them caught nothing: the row lists what was found
/// and asks for no change, because there is no better adapter to change to.
#[test]
fn grades_carry_their_notes_and_a_device_nobody_heard_asks_for_no_change() {
    let grades = vec![
        Grade {
            adapter: "hci0".into(),
            seen: 0,
            of: 5,
            note: Some("no adapter named hci0".into()),
        },
        Grade {
            adapter: "hci1".into(),
            seen: 0,
            of: 5,
            note: None,
        },
    ];
    let row = grade_row(&grades, Some("hci1"), "amp");
    assert_eq!(row.state, State::Pass);
    assert_eq!(row.reason, "hci0 0/5 (no adapter named hci0), hci1 0/5");
    assert!(row.fix.is_none(), "{row:?}");
}

/// An adapter the config names that is not there at all grades below whatever is, so the
/// row offers the best one present.
#[test]
fn an_adapter_the_host_does_not_have_is_below_the_one_it_does() {
    let grades = vec![Grade {
        adapter: "hci1".into(),
        seen: 3,
        of: 5,
        note: None,
    }];
    let row = grade_row(&grades, Some("hci9"), "amp");
    assert_eq!(row.state, State::Warn);
    assert_eq!(
        row.fix.expect("the config change").command,
        "set adapter = \"hci1\" in config.toml"
    );
}

/// This verb's own rows go where the table puts them: the grades under the adapters, the
/// file to create and the devices under the config.
#[test]
fn this_verbs_rows_are_spliced_where_the_table_puts_them() {
    let host = vec![
        outcome(checks::BLUETOOTH_SERVICE, State::Pass, None),
        outcome(checks::ADAPTERS, State::Pass, None),
        outcome(checks::RUNTIME, State::Pass, None),
        outcome(checks::DAEMON, State::Pass, None),
        outcome(checks::CONFIG, State::Pass, None),
        outcome(checks::CONTAINER, State::Pass, None),
    ];
    let order: Vec<&str> = spliced(
        host,
        Some(outcome(ADAPTER_GRADE, State::Pass, None)),
        vec![
            outcome(CONFIG_FILE, State::Warn, None),
            outcome(DEVICES, State::Pass, None),
        ],
    )
    .iter()
    .map(|o| o.check)
    .collect();
    assert_eq!(
        order,
        [
            checks::BLUETOOTH_SERVICE,
            checks::ADAPTERS,
            ADAPTER_GRADE,
            checks::RUNTIME,
            checks::DAEMON,
            checks::CONFIG,
            CONFIG_FILE,
            DEVICES,
            checks::CONTAINER,
        ]
    );
}

/// The fixes are offered on a terminal and not into a pipe or a JSON array, and the guided
/// first run among them needs the other end of the conversation as well.
#[test]
fn the_fixes_are_offered_on_a_terminal_and_the_wizard_needs_both_ends_of_one() {
    assert_eq!(asking(true, false, true), (true, true));
    assert_eq!(
        asking(true, false, false),
        (true, false),
        "answers from a file"
    );
    assert_eq!(asking(true, true, true), (false, false), "a JSON array");
    assert_eq!(asking(false, false, true), (false, false), "a pipe");
}

/// Each state is read by its own mark, and only a line that did not pass carries its fix.
#[test]
fn a_line_carries_its_mark_and_a_failing_one_carries_its_fix() {
    let warn = outcome(checks::CONFIG, State::Warn, Some(fix(SETUP, true)));
    assert_eq!(
        lines(&warn),
        vec![
            "! Config — because".to_string(),
            format!("    fix: {SETUP}")
        ]
    );
    let pass = outcome(checks::CONFIG, State::Pass, None);
    assert_eq!(lines(&pass), vec!["✓ Config — because".to_string()]);
    assert_eq!(mark(State::Fail), '✗');
    assert_eq!(mark(State::Note), '·');
}

/// Only the fixes that apply themselves are collected, only from the rows that printed
/// them, and in the order the rows are read.
#[test]
fn the_collected_fixes_are_the_ones_this_verb_showed_and_can_apply() {
    let path = Path::new("/tmp/nowhere/config.toml");
    let outcomes = vec![
        outcome(
            checks::BLUETOOTH_SERVICE,
            State::Fail,
            Some(fix("sudo systemctl enable --now bluetooth", false)),
        ),
        outcome(checks::DAEMON, State::Fail, Some(fix(DAEMON_INSTALL, true))),
        outcome(checks::CONFIG, State::Warn, Some(fix(SETUP, true))),
        config_file_row(path, true).expect("a row about the missing file"),
        // A row that passed prints no fix, so it offers none either.
        outcome(DEVICES, State::Pass, Some(fix("dance", true))),
    ];
    let commands: Vec<String> = fixes(&outcomes)
        .into_iter()
        .map(|fix| fix.command)
        .collect();
    assert_eq!(
        commands,
        vec![
            DAEMON_INSTALL.to_string(),
            SETUP.to_string(),
            write_template(path).command,
        ]
    );
}

/// The daemon fix is the `daemon install` verb, and it is reached for by the command it
/// carries; a fix nothing here recognises says that rather than pretending to have run.
///
/// With no daemon answering, the rows that look at devices read over a session of this
/// process's own: the launcher chain would start a daemon, and a check that started one
/// would be reporting on its own work. With one answering, nothing is forced.
#[test]
fn with_no_daemon_answering_the_rows_look_over_a_session_of_this_process_s_own() {
    let looking = |transport: &str, daemon: bool| {
        looking_only(
            &Globals {
                transport,
                adapter: Some("hci1"),
                socket: Some("/run/user/1000/svolder.sock"),
                device: Some("amp"),
                verbose: true,
            },
            daemon,
        )
    };
    for transport in ["auto", "daemon"] {
        assert_eq!(looking(transport, false).transport, "ble", "{transport}");
        assert_eq!(looking(transport, true).transport, transport);
    }
    for transport in ["ble", "fake"] {
        assert_eq!(looking(transport, false).transport, transport);
        assert_eq!(looking(transport, true).transport, transport);
    }
    // Everything else is the run's own: only where the rows read changes.
    let forced = looking("auto", false);
    assert_eq!(forced.adapter.as_deref(), Some("hci1"));
    assert_eq!(
        forced.socket.as_deref(),
        Some("/run/user/1000/svolder.sock")
    );
    assert_eq!(forced.device.as_deref(), Some("amp"));
    assert!(forced.verbose);
}

/// Nothing installs anything here: what the verb does to a machine is its own tests', and
/// this is about which fix reaches it.
///
/// The guided first run is no command to apply — it is a conversation, and the run that
/// offered it walks it itself — so it falls through with everything else this does not
/// recognise rather than pretending to have run.
#[tokio::test]
async fn the_daemon_fix_installs_and_the_guided_run_is_not_a_command_to_apply() {
    /// A `daemon install` that records that it was asked rather than installing.
    struct Recorded(Mutex<u8>);

    #[async_trait::async_trait(?Send)]
    impl Verbs for Recorded {
        async fn install_daemon(&self) -> Result<String, String> {
            *self.0.lock().expect("the record") += 1;
            Ok("installed the systemd user units".to_string())
        }
    }

    let path = Path::new("/tmp/nowhere/config.toml");
    let verbs = Recorded(Mutex::new(0));
    assert_eq!(
        apply_with(&verbs, &fix(DAEMON_INSTALL, true), path).await,
        Ok("installed the systemd user units".to_string())
    );
    assert_eq!(*verbs.0.lock().expect("the record"), 1);

    let guided = apply_with(&verbs, &fix(SETUP, true), path)
        .await
        .expect_err("not a command");
    assert!(guided.contains(SETUP), "{guided}");
    let unknown = apply_with(&verbs, &fix("dance", true), path)
        .await
        .expect_err("nothing dances");
    assert!(unknown.contains("dance"), "{unknown}");
    assert_eq!(
        *verbs.0.lock().expect("the record"),
        1,
        "only the daemon fix is the daemon verb"
    );
}

/// The adapter a grade's fix names is the one that is written.
#[test]
fn the_adapter_fix_names_the_adapter_to_write() {
    assert_eq!(
        adapter_in("set adapter = \"hci1\" in config.toml"),
        Some("hci1")
    );
    assert_eq!(adapter_in(SETUP), None);
}

/// The config file and the adapter key are written where the fixes said they would be.
#[tokio::test]
async fn the_config_fixes_write_the_file_they_name() {
    // The configuration's own directory does not exist yet: the fix makes it.
    let home = crate::test_support::temp_dir("doctor");
    let path = home.0.join("svolder").join("config.toml");

    let done = apply(&write_template(&path), &path)
        .await
        .expect("the file is written");
    assert!(done.contains(&path.display().to_string()), "{done}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file"),
        Config::template()
    );

    apply(&fix("set adapter = \"hci1\" in config.toml", true), &path)
        .await
        .expect("the adapter is written");
    assert_eq!(
        Config::load_from(&path).expect("the file parses").adapter,
        Some("hci1".to_string())
    );

    // A file that is already there is not written over by the fix that makes the first one.
    assert!(apply(&write_template(&path), &path).await.is_err());
}

/// A daemon whose version is not this svolder's grades the daemon check `!`, naming both
/// versions and the restart verb as the fix; a matching one leaves the check as it was.
#[test]
fn a_daemon_of_another_version_grades_the_daemon_check_with_a_restart_fix() {
    let own = env!("CARGO_PKG_VERSION");
    let mut outcomes = vec![outcome(checks::DAEMON, State::Pass, None)];

    grade_daemon_version(&mut outcomes, own);
    assert_eq!(outcomes[0].state, State::Pass, "a matching daemon");
    assert!(outcomes[0].fix.is_none());

    grade_daemon_version(&mut outcomes, "0.0.1-other");
    assert_eq!(outcomes[0].state, State::Warn);
    assert_eq!(
        lines(&outcomes[0]),
        [
            format!(
                "! {} — daemon v0.0.1-other is older than this svolder v{own}",
                checks::DAEMON
            ),
            "    fix: svolder daemon restart".to_string(),
        ]
    );
    assert!(
        !outcomes[0].fix.as_ref().expect("a fix").applies_itself,
        "stopping a daemon drops its links, so the reader runs it"
    );
}

/// A daemon newer than this svolder is named as such and gets no restart fix: restarting it
/// would change nothing.
#[test]
fn a_newer_daemon_is_graded_without_a_restart_fix() {
    let own = env!("CARGO_PKG_VERSION");
    let mut outcomes = vec![outcome(checks::DAEMON, State::Pass, None)];
    grade_daemon_version(&mut outcomes, "99.0.0");
    assert_eq!(outcomes[0].state, State::Warn);
    assert_eq!(
        lines(&outcomes[0]),
        [format!(
            "! {} — daemon v99.0.0 is newer than this svolder v{own}",
            checks::DAEMON
        )]
    );
}

/// A daemon check that already warned keeps its own fix and gains the version in its words.
#[test]
fn a_daemon_check_that_already_warned_keeps_its_fix() {
    let mut outcomes = vec![Outcome {
        reason: "svolderd.socket is installed but not enabled".into(),
        ..outcome(checks::DAEMON, State::Warn, Some(fix(DAEMON_INSTALL, true)))
    }];
    grade_daemon_version(&mut outcomes, "0.0.1-other");
    assert!(
        outcomes[0]
            .reason
            .starts_with("svolderd.socket is installed")
    );
    assert!(outcomes[0].reason.contains("daemon v0.0.1-other is older"));
    assert_eq!(outcomes[0].fix.as_ref().unwrap().command, DAEMON_INSTALL);
}

/// The daemon's version is asked of the daemon the probe found, and only under a transport that
/// uses a daemon.
#[test]
fn the_version_is_asked_only_of_a_probed_daemon_under_a_daemon_transport() {
    let socket = Path::new("/run/user/1000/svolder.sock");
    for transport in ["auto", "daemon"] {
        assert_eq!(version_asked_of(Some(socket), transport), Some(socket));
        assert_eq!(version_asked_of(None, transport), None, "{transport}");
    }
    for transport in ["ble", "fake"] {
        assert_eq!(
            version_asked_of(Some(socket), transport),
            None,
            "{transport}"
        );
        assert_eq!(version_asked_of(None, transport), None, "{transport}");
    }
}

/// The MCP row: absent while nothing turns the server on, a failure for a server that could
/// not start and for a token file others can read, and a pass once the daemon says it listens.
#[test]
fn the_mcp_row_grades_the_server_and_its_token_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("svolder-doctor-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("token");
    std::fs::write(&file, "secret-token-for-the-test").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let listen: std::net::SocketAddr = "127.0.0.1:8737".parse().unwrap();
    let on = McpConfig {
        enabled: true,
        ..McpConfig::default()
    };
    let listening = McpStatus::Listening {
        listen,
        auth: svolder_ipc::config::McpAuth::Token,
        writes: false,
    };

    assert!(mcp_row(&McpConfig::default(), false, &file, None).is_none());
    assert!(mcp_row(&McpConfig::default(), false, &file, Some(&McpStatus::Off)).is_none());

    let up = mcp_row(&on, false, &file, Some(&listening)).expect("a row");
    assert_eq!(up.state, State::Pass);
    assert_eq!(up.reason, "listening on 127.0.0.1:8737 (token, read-only)");
    let unasked = mcp_row(&on, false, &file, None).expect("a row");
    assert_eq!(unasked.state, State::Note);
    // A daemon that answers "off" for a section that is on has not read the file.
    let unread = mcp_row(&on, false, &file, Some(&McpStatus::Off)).expect("a row");
    assert_eq!(unread.state, State::Warn);
    assert_eq!(
        unread.reason,
        "on in the config, but the daemon has not read it: svolder daemon restart (or SIGHUP)"
    );

    let failed = McpStatus::Failed {
        listen,
        error: "Address already in use (os error 98)".into(),
    };
    let down = mcp_row(&on, false, &file, Some(&failed)).expect("a row");
    assert_eq!(down.state, State::Fail);
    assert_eq!(
        down.reason,
        "cannot listen on 127.0.0.1:8737: Address already in use (os error 98)"
    );

    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let loose = mcp_row(&on, false, &file, Some(&listening)).expect("a row");
    assert_eq!(loose.state, State::Fail);
    assert!(loose.reason.contains("0644"), "{}", loose.reason);
    assert!(!loose.reason.contains("secret-token"));
    // The environment's token outranks the file, so the file's mode is not its concern.
    assert_eq!(
        mcp_row(&on, true, &file, Some(&listening))
            .expect("a row")
            .state,
        State::Pass
    );
    // A named token file that is not there is a failure; a generated one not made yet is not.
    let named = McpConfig {
        token_file: Some(dir.join("missing").display().to_string()),
        ..on.clone()
    };
    assert_eq!(
        mcp_row(&named, false, &file, None).expect("a row").state,
        State::Fail
    );
    assert_eq!(
        mcp_row(&on, false, &dir.join("not-yet"), None)
            .expect("a row")
            .state,
        State::Note
    );
    // Writes without a ceiling on the volume are a warning; with one they are fine.
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let writes = McpConfig {
        writes: true,
        ..on.clone()
    };
    let open = mcp_row(&writes, false, &file, Some(&listening)).expect("a row");
    assert_eq!(open.state, State::Warn);
    assert!(open.reason.contains("max_volume_db"), "{}", open.reason);
    let capped = McpConfig {
        max_volume_db: Some(-20.0),
        ..writes
    };
    assert_ne!(
        mcp_row(&capped, false, &file, Some(&listening))
            .expect("a row")
            .state,
        State::Warn
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// The Web UI row: absent while nothing turns it on, a failure for a server that could not
/// start, an open address with no auth and a token file others can read, a warning for a section
/// the daemon has not read, a note for plain HTTP on the network, and a pass once it listens.
#[test]
fn the_web_row_grades_the_server_and_its_token_file() {
    use std::os::unix::fs::PermissionsExt;
    use svolder_ipc::WebStatus;
    use svolder_ipc::config::{WebAuth, WebConfig, WebGuest};
    let dir = std::env::temp_dir().join(format!("svolder-doctor-web-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("token");
    std::fs::write(&file, "web-secret-token-for-the-test").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let listen: std::net::SocketAddr = "127.0.0.1:8738".parse().unwrap();
    let on = WebConfig {
        enabled: true,
        ..WebConfig::default()
    };
    let listening = WebStatus::Listening {
        listen,
        auth: WebAuth::Token,
        writes: true,
        guest: WebGuest::None,
    };

    assert!(web_row(&WebConfig::default(), false, &file, None).is_none());
    assert!(web_row(&WebConfig::default(), false, &file, Some(&WebStatus::Off)).is_none());

    let up = web_row(&on, false, &file, Some(&listening)).expect("a row");
    assert_eq!((up.check, up.state), ("Web UI", State::Pass));
    assert_eq!(up.reason, "listening on 127.0.0.1:8738 (token, writes)");

    let failed = WebStatus::Failed {
        listen,
        error: "Address already in use (os error 98)".into(),
    };
    let down = web_row(&on, false, &file, Some(&failed)).expect("a row");
    assert_eq!(down.state, State::Fail);
    assert!(
        down.reason.contains("Address already in use"),
        "{}",
        down.reason
    );

    let open = WebConfig {
        listen: "0.0.0.0:8738".parse().unwrap(),
        auth: WebAuth::None,
        ..on.clone()
    };
    assert_eq!(
        web_row(&open, false, &file, None).unwrap().state,
        State::Fail
    );

    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let named = WebConfig {
        token_file: Some(file.display().to_string()),
        ..on.clone()
    };
    let loose = web_row(&named, false, &file, Some(&listening)).expect("a row");
    assert_eq!(loose.state, State::Fail);
    assert!(loose.reason.contains("0644"), "{}", loose.reason);
    assert!(!loose.reason.contains("web-secret-token"));
    // The environment's token outranks the file, so the file's mode is not its concern.
    assert_eq!(
        web_row(&named, true, &file, Some(&listening))
            .unwrap()
            .state,
        State::Pass
    );
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();

    let unread = web_row(&on, false, &file, Some(&WebStatus::Off)).expect("a row");
    assert_eq!(unread.state, State::Warn);
    assert!(
        unread.reason.contains("daemon restart"),
        "{}",
        unread.reason
    );

    let network = WebConfig {
        listen: "0.0.0.0:8738".parse().unwrap(),
        ..on
    };
    let plain = web_row(&network, false, &file, Some(&listening)).expect("a row");
    assert_eq!(plain.state, State::Note);
    assert!(plain.reason.contains("plain HTTP"), "{}", plain.reason);
    let _ = std::fs::remove_dir_all(dir);
}

/// A fake daemon on a fresh socket that reads one request line and answers it with `reply`, or
/// holds the connection without answering when there is none.
fn fake_daemon(reply: Option<svolder_ipc::socket::protocol::Reply>) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let socket = std::env::temp_dir().join(format!(
        "svolder-doctor-fake-{}-{seq}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&socket);
    let listener = tokio::net::UnixListener::bind(&socket).expect("the socket binds");
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            let (read, mut write) = stream.into_split();
            let mut read = tokio::io::BufReader::new(read);
            let mut line = String::new();
            let _ = read.read_line(&mut line).await;
            if let Some(reply) = &reply {
                let text = svolder_ipc::socket::protocol::to_line(reply).expect("a reply line");
                let _ = write.write_all(text.as_bytes()).await;
            }
            held.push((read, write));
        }
    });
    socket
}

/// What the doctor reads of the daemon on a socket: its version and its MCP and web statuses
/// when it answers a version, and nothing for any other answer or for silence past the wait.
#[tokio::test]
async fn the_daemon_answer_is_the_version_mcp_and_web_of_a_version_reply_and_nothing_else() {
    use svolder_ipc::socket::protocol::{Reply, Response};
    let socket = fake_daemon(Some(Reply::Response {
        id: 1,
        response: Response::Version {
            version: "9.9.9".into(),
            mqtt: None,
            mcp: Some(McpStatus::Off),
            log: None,
            config: None,
            mcp_token: None,
            web: Some(svolder_ipc::WebStatus::Off),
            web_token: None,
        },
    }));
    assert_eq!(
        daemon_answer(&socket).await,
        Some((
            "9.9.9".to_string(),
            Some(McpStatus::Off),
            Some(svolder_ipc::WebStatus::Off)
        ))
    );
    let _ = std::fs::remove_file(&socket);

    let socket = fake_daemon(Some(Reply::Response {
        id: 1,
        response: Response::Ack,
    }));
    assert_eq!(daemon_answer(&socket).await, None);
    let _ = std::fs::remove_file(&socket);
}

/// A daemon that takes the connection and never answers is given up on after the wait.
#[tokio::test(start_paused = true)]
async fn a_silent_daemon_is_given_up_on_for_the_doctor() {
    let socket = fake_daemon(None);
    assert_eq!(daemon_answer(&socket).await, None);
    let _ = std::fs::remove_file(&socket);
}

/// The outcomes a host with no configuration file gets are the rows `run --json` prints, one
/// JSON array of checks, and collecting them offers and applies no fix.
#[tokio::test]
async fn outcomes_over_a_scratch_home_are_the_rows_run_prints() {
    let dir = crate::test_support::temp_dir("doctor-outcomes");
    let socket = dir.as_ref().join("sv.sock");
    let socket = socket.to_str().expect("a utf-8 path");
    let g = Globals {
        transport: "fake",
        adapter: None,
        socket: Some(socket),
        device: None,
        verbose: false,
    };
    let rows = outcomes(&g, false).await;
    let checks: Vec<&str> = rows.iter().map(|row| row.check).collect();
    assert!(checks.contains(&CONFIG_FILE), "{checks:?}");
    let json = serde_json::to_value(&rows).expect("the outcomes serialise");
    let printed = json.as_array().expect("one JSON array");
    assert_eq!(printed.len(), rows.len());
    assert!(
        printed
            .iter()
            .all(|row| row["check"].is_string() && row["state"].is_string())
    );
}

/// With `connect` off, the look never connects to the socket: where systemd's socket unit is
/// listening, a connect is what would start the service.
#[tokio::test]
async fn outcomes_without_connecting_leave_the_socket_alone() {
    let dir = crate::test_support::temp_dir("doctor-no-connect");
    let socket = dir.as_ref().join("sv.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("the socket binds");
    listener.set_nonblocking(true).expect("non-blocking");
    let socket = socket.to_str().expect("a utf-8 path");
    let g = Globals {
        transport: "fake",
        adapter: None,
        socket: Some(socket),
        device: None,
        verbose: false,
    };
    let rows = outcomes_with(&g, false, false).await;
    assert!(rows.iter().any(|row| row.check == CONFIG_FILE));
    assert!(
        matches!(
            listener.accept(),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
        ),
        "nothing connected to the socket"
    );
}

/// With `connect` off and a device in the config file, the whole look never connects to the
/// socket: where systemd's socket unit listens with its service idle, a connect would start the
/// service. The run reads the config file it is given and reports a row for its device.
#[tokio::test]
async fn a_look_with_a_configured_device_does_not_connect_to_the_socket() {
    let dir = crate::test_support::temp_dir("doctor-device-no-connect");
    let socket = dir.as_ref().join("sv.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("the socket binds");
    listener.set_nonblocking(true).expect("non-blocking");
    let config = dir.as_ref().join("config.toml");
    std::fs::write(
        &config,
        "[[device]]\nname = \"amp\"\nudid = \"0102030405060708\"\npassphrase = \"1234\"\n",
    )
    .expect("the config is written");
    let socket = socket.to_str().expect("a utf-8 path");
    let g = Globals {
        transport: "fake",
        adapter: None,
        socket: Some(socket),
        device: None,
        verbose: false,
    };
    let rows = outcomes_over(&g, false, false, &config).await;
    assert!(
        rows.iter().any(|row| row.check == DEVICES),
        "a row for the configured device: {rows:?}"
    );
    assert!(
        matches!(
            listener.accept(),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
        ),
        "nothing connected to the socket"
    );
}
