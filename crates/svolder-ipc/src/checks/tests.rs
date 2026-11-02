// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The socket path every test resolves to.
fn sock() -> &'static Path {
    Path::new("/run/user/1000/svolder.sock")
}

/// A config naming one device, with a profile that is compiled into the binary.
const CONFIG_TEXT: &str = "schema = 1\n\n[[device]]\nname = \"bifrost\"\n\
                               udid = \"0011223344556677\"\nprofile = \"bifrost3\"\n";

/// A snapshot whose every fact a test sets by hand.
struct FakeProbes {
    bluez: BluezFacts,
    systemd: SystemdFacts,
    env: EnvFacts,
    config: Option<Result<String, String>>,
    daemons: Vec<RunningDaemon>,
}

impl Probes for FakeProbes {
    fn bluez(&self) -> BluezFacts {
        self.bluez.clone()
    }
    fn systemd(&self) -> SystemdFacts {
        self.systemd.clone()
    }
    fn env(&self) -> EnvFacts {
        self.env.clone()
    }
    fn config_text(&self) -> Option<Result<String, String>> {
        self.config.clone()
    }
    fn daemons(&self) -> Vec<RunningDaemon> {
        self.daemons.clone()
    }
}

/// A svolderd listening on the socket `on` names, `None` for one whose socket cannot be
/// told.
fn daemon_on(pid: u32, on: Option<&str>) -> RunningDaemon {
    RunningDaemon {
        pid,
        socket: on.map(PathBuf::from),
    }
}

/// A host with nothing wrong with it: two extended-advertising adapters, the units
/// installed and enabled on the resolved socket, a bluetoothd without the plugins that
/// fight for the same devices, and one configured device.
fn healthy() -> FakeProbes {
    FakeProbes {
        bluez: BluezFacts {
            reachable: Ok(()),
            adapters: vec![
                AdapterFacts {
                    name: "hci0".into(),
                    address: "DE:C1:BE:1B:EA:75".into(),
                    powered: true,
                    max_adv_len: Some(251),
                },
                AdapterFacts {
                    name: "hci1".into(),
                    address: "CA:B1:E5:0F:DA:C5".into(),
                    powered: true,
                    max_adv_len: Some(251),
                },
            ],
            unreadable: None,
        },
        systemd: SystemdFacts {
            present: true,
            socket_unit: Some(UnitFacts {
                installed: true,
                enabled: true,
                active: true,
                running: true,
            }),
            service_unit: Some(UnitFacts {
                installed: true,
                enabled: true,
                active: false,
                running: false,
            }),
            listen_stream: Some("/run/user/1000/svolder.sock".into()),
            bluetoothd_exec_start: Some(format!(
                "/usr/libexec/bluetooth/bluetoothd --noplugin={NOPLUGIN}"
            )),
            unreadable: None,
        },
        env: EnvFacts {
            runtime_dir: Some("/run/user/1000".into()),
            socket_dir_writable: Some(true),
            container: false,
            nixos: false,
            uid: 1000,
            wireplumber_version: Some("0.5.13".into()),
            wireplumber_midi_disabled: Some(true),
        },
        config: Some(Ok(CONFIG_TEXT.into())),
        daemons: Vec::new(),
    }
}

/// The one outcome named `check`, or a panic naming what was produced instead.
fn named(outcomes: Vec<Outcome>, check: &str) -> Outcome {
    outcomes
        .iter()
        .find(|o| o.check == check)
        .unwrap_or_else(|| panic!("no {check} check in {outcomes:?}"))
        .clone()
}

#[test]
fn a_healthy_host_passes_every_core_check() {
    let out = run(&healthy(), Path::new("/run/user/1000/svolder.sock"), false);
    assert!(
        out.iter()
            .all(|o| matches!(o.state, State::Pass | State::Note)),
        "{out:?}"
    );
    assert_eq!(exit_code(&out), 0);
}

#[test]
fn no_bluez_fails_with_the_systemctl_fix_and_no_adapter_fails_too() {
    let mut p = healthy();
    p.bluez.reachable = Err("org.bluez did not answer".into());
    let out = run(&p, sock(), false);
    let svc = out.iter().find(|o| o.check == "Bluetooth service").unwrap();
    assert!(matches!(svc.state, State::Fail));
    assert!(
        svc.fix
            .as_ref()
            .unwrap()
            .command
            .contains("sudo systemctl enable --now bluetooth")
    );
    assert!(matches!(named(out.clone(), "Adapters").state, State::Fail));
    assert_eq!(exit_code(&out), 2);
}

#[test]
fn missing_units_fail_with_a_fix_doctor_applies_itself_and_no_systemd_is_a_note() {
    let mut p = healthy();
    p.systemd.socket_unit = None;
    p.systemd.service_unit = None;
    let daemon = run(&p, sock(), false)
        .into_iter()
        .find(|o| o.check == "Daemon")
        .unwrap();
    assert!(matches!(daemon.state, State::Fail) && daemon.fix.as_ref().unwrap().applies_itself);
    assert_eq!(daemon.fix.unwrap().command, "svolder daemon install");
    p.systemd.present = false;
    assert!(matches!(
        run(&p, sock(), false)
            .into_iter()
            .find(|o| o.check == "Daemon")
            .unwrap()
            .state,
        State::Note
    ));
}

#[test]
fn the_socket_units_path_must_match_the_resolved_socket() {
    let mut p = healthy();
    p.systemd.listen_stream = Some("/run/user/1000/other.sock".into());
    let daemon = run(&p, sock(), false)
        .into_iter()
        .find(|o| o.check == "Daemon")
        .unwrap();
    assert!(matches!(daemon.state, State::Warn) && daemon.reason.contains("other.sock"));
}

/// Another svolderd is a row of its own, naming where it listens and the one command that
/// stops that one — which nothing ever runs for the reader.
#[test]
fn another_svolderd_warns_with_where_it_listens_and_what_stops_it() {
    let mut p = healthy();
    p.daemons = vec![daemon_on(4711, Some("/tmp/svolder-firstrun/svolder.sock"))];
    let out = run(&p, sock(), false);
    let other = named(out.clone(), OTHER_DAEMONS);
    assert!(matches!(other.state, State::Warn), "{other:?}");
    assert_eq!(
        other.reason,
        "another svolderd (pid 4711) is listening on /tmp/svolder-firstrun/svolder.sock"
    );
    let fix = other.fix.expect("the command that stops that daemon");
    assert_eq!(
        fix.command,
        "svolder --socket /tmp/svolder-firstrun/svolder.sock daemon stop"
    );
    assert!(!fix.applies_itself, "nothing stops a daemon for the reader");
    // The daemon check itself is still about the units, and says nothing about this.
    assert!(matches!(named(out.clone(), DAEMON).state, State::Pass));
    assert_eq!(exit_code(&out), 1, "{out:?}");
}

/// The daemon this session would reach is not another one: neither the one on the resolved
/// socket nor the one on the socket the unit listens on is worth a word, and one whose
/// socket cannot be told has no path to name.
#[test]
fn the_daemon_this_session_reaches_is_not_another_one() {
    let mut p = healthy();
    p.systemd.listen_stream = Some("/run/user/1000/svolder.sock".into());
    p.daemons = vec![
        daemon_on(11, Some("/run/user/1000/svolder.sock")),
        daemon_on(12, None),
    ];
    let out = run(&p, sock(), false);
    assert!(
        out.iter().all(|o| o.check != OTHER_DAEMONS),
        "nothing is said about the daemon this session reaches: {out:?}"
    );
    assert_eq!(exit_code(&out), 0, "{out:?}");
}

/// Every other daemon gets its own row, under the daemon check, each carrying the command
/// that stops that one.
#[test]
fn every_other_svolderd_reads_under_the_daemon_row_with_a_command_of_its_own() {
    let mut p = healthy();
    p.systemd.listen_stream = Some("/run/user/1000/svolder.sock".into());
    p.daemons = vec![
        daemon_on(11, Some("/run/user/1000/svolder.sock")),
        daemon_on(12, Some("/tmp/svolder-1000/svolder.sock")),
        daemon_on(13, Some("/tmp/svolder-firstrun/svolder.sock")),
    ];
    let out = run(&p, sock(), false);
    let names: Vec<&str> = out.iter().map(|o| o.check).collect();
    assert_eq!(
        names,
        [
            BLUETOOTH_SERVICE,
            ADAPTERS,
            RUNTIME,
            DAEMON,
            OTHER_DAEMONS,
            OTHER_DAEMONS,
            CONFIG,
        ]
    );
    let commands: Vec<String> = out
        .iter()
        .filter(|o| o.check == OTHER_DAEMONS)
        .map(|o| o.fix.as_ref().unwrap().command.clone())
        .collect();
    assert_eq!(
        commands,
        [
            "svolder --socket /tmp/svolder-1000/svolder.sock daemon stop",
            "svolder --socket /tmp/svolder-firstrun/svolder.sock daemon stop",
        ]
    );
}

#[test]
fn a_disabled_socket_unit_warns() {
    let mut p = healthy();
    p.systemd.socket_unit = Some(UnitFacts {
        installed: true,
        enabled: false,
        active: false,
        running: false,
    });
    let daemon = named(run(&p, sock(), false), DAEMON);
    assert!(matches!(daemon.state, State::Warn), "{daemon:?}");
    assert!(daemon.reason.contains("not enabled"), "{daemon:?}");
    assert_eq!(daemon.fix.unwrap().command, "svolder daemon install");
}

#[test]
fn dev_checks_need_two_extended_adapters_and_the_plugin_exclusions() {
    let mut p = healthy();
    p.bluez.adapters[0].max_adv_len = Some(31);
    p.systemd.bluetoothd_exec_start = Some("/usr/libexec/bluetooth/bluetoothd".into());
    let out = run(&p, sock(), true);
    assert!(
        out.iter()
            .any(|o| o.check == "Extended advertising" && matches!(o.state, State::Warn))
    );
    let plugins = out
        .iter()
        .find(|o| o.check == "bluetoothd plugins")
        .unwrap();
    assert!(
        matches!(plugins.state, State::Warn)
            && plugins
                .fix
                .as_ref()
                .unwrap()
                .command
                .contains("--noplugin=midi")
    );
    assert!(
        run(&healthy(), sock(), false)
            .iter()
            .all(|o| o.check != "Extended advertising")
    );
}

#[test]
fn the_bluetoothd_fix_clears_and_rewrites_the_discovered_exec_start() {
    let mut p = healthy();
    p.systemd.bluetoothd_exec_start = Some("/nix/store/xyz/bluetoothd --experimental".into());
    let plugins = named(run(&p, sock(), true), BLUETOOTHD_PLUGINS);
    let command = plugins.fix.unwrap().command;
    assert!(command.contains("/etc/systemd/system/bluetooth.service.d/noplugin.conf"));
    assert!(command.contains("ExecStart=\\nExecStart=/nix/store/xyz/bluetoothd --experimental"));
    assert!(command.contains(&format!("--noplugin={NOPLUGIN}")));
    assert!(command.contains("sudo systemctl daemon-reload && sudo systemctl restart bluetooth"));
}

#[test]
fn the_wireplumber_check_follows_the_installed_version() {
    let mut p = healthy();
    assert!(matches!(
        named(run(&p, sock(), true), WIREPLUMBER).state,
        State::Pass
    ));

    p.env.wireplumber_midi_disabled = Some(false);
    let modern = named(run(&p, sock(), true), WIREPLUMBER);
    assert!(matches!(modern.state, State::Warn), "{modern:?}");
    let command = modern.fix.unwrap().command;
    assert!(command.contains(WIREPLUMBER_DROP_IN), "{command}");
    assert!(
        command.contains("monitor.bluez-midi = disabled"),
        "{command}"
    );
    assert!(
        command.contains("systemctl --user restart wireplumber"),
        "{command}"
    );

    p.env.wireplumber_version = Some("0.4.17".into());
    let legacy = named(run(&p, sock(), true), WIREPLUMBER);
    assert!(matches!(legacy.state, State::Warn), "{legacy:?}");
    assert!(legacy.reason.contains("bluetooth.lua.d"), "{legacy:?}");
    assert!(legacy.fix.is_none(), "{legacy:?}");

    p.env.wireplumber_version = None;
    p.env.wireplumber_midi_disabled = None;
    assert!(matches!(
        named(run(&p, sock(), true), WIREPLUMBER).state,
        State::Note
    ));
}

#[test]
fn a_container_reports_what_it_needs() {
    let mut p = healthy();
    p.env.container = true;
    p.systemd.present = false;
    p.bluez.reachable = Err("no such file".into());
    let out = run(&p, sock(), false);
    assert!(
        out.iter()
            .any(|o| o.check == "Container" && o.reason.contains("/run/dbus/system_bus_socket"))
    );
    // An unmounted bus is a mount to add, and nothing is said about the group.
    let mount = named(out, CONTAINER).fix.unwrap().command;
    assert_eq!(
        mount,
        "run the container with \
             -v /run/dbus/system_bus_socket:/run/dbus/system_bus_socket:ro"
    );

    // A bus that answers with no adapter behind it is a group to join, and nothing is said
    // about the mount.
    p.bluez.adapters.clear();
    p.bluez.reachable = Ok(());
    let group = named(run(&p, sock(), false), CONTAINER);
    assert!(matches!(group.state, State::Warn), "{group:?}");
    assert_eq!(
        group.fix.unwrap().command,
        "run the container with --group-add \"$(getent group bluetooth | cut -d: -f3)\""
    );
}

#[test]
fn a_container_with_everything_mounted_passes_and_no_container_is_not_checked() {
    let mut p = healthy();
    p.env.container = true;
    p.systemd.present = false;
    let container = named(run(&p, sock(), false), CONTAINER);
    assert!(matches!(container.state, State::Pass), "{container:?}");
    assert!(container.reason.contains("no systemd"), "{container:?}");
    assert!(
        run(&healthy(), sock(), false)
            .iter()
            .all(|o| o.check != CONTAINER)
    );
}

#[test]
fn an_absent_or_unusable_adapter_warns_and_no_adapter_at_all_fails() {
    let mut p = healthy();
    p.config = Some(Ok(
        CONFIG_TEXT.replace("schema = 1", "schema = 1\nadapter = \"hci9\"")
    ));
    let missing = named(run(&p, sock(), false), ADAPTERS);
    assert!(matches!(missing.state, State::Warn), "{missing:?}");
    assert!(missing.reason.contains("hci9"), "{missing:?}");

    let mut p = healthy();
    p.bluez.adapters[0].powered = false;
    p.bluez.adapters[1].powered = false;
    let off = named(run(&p, sock(), false), ADAPTERS);
    assert!(matches!(off.state, State::Warn), "{off:?}");
    assert_eq!(off.fix.unwrap().command, "bluetoothctl power on");

    let mut p = healthy();
    p.bluez.adapters.clear();
    let none = named(run(&p, sock(), false), ADAPTERS);
    assert!(matches!(none.state, State::Fail), "{none:?}");
    assert_eq!(exit_code(&run(&p, sock(), false)), 2);
}

#[test]
fn the_runtime_check_warns_only_when_the_socket_is_the_tmp_fallback() {
    let mut p = healthy();
    p.env.runtime_dir = None;
    // No runtime directory and a socket in the private fallback: the one case to warn on,
    // and the path it names is the one that was resolved.
    let fallback = named(
        run(&p, Path::new("/tmp/svolder-1000/svolder.sock"), false),
        RUNTIME,
    );
    assert!(matches!(fallback.state, State::Warn), "{fallback:?}");
    assert!(
        fallback.reason.contains("/tmp/svolder-1000/svolder.sock"),
        "{fallback:?}"
    );

    // No runtime directory but a socket named elsewhere — the container image, whose
    // SVOLDER_SOCKET is /run/svolder/svolder.sock — is not a problem at all.
    let named_socket = named(
        run(&p, Path::new("/run/svolder/svolder.sock"), false),
        RUNTIME,
    );
    assert!(
        matches!(named_socket.state, State::Pass),
        "{named_socket:?}"
    );
    assert!(
        named_socket.reason.contains("/run/svolder/svolder.sock"),
        "{named_socket:?}"
    );
    assert!(
        !named_socket.reason.contains("/tmp/svolder-1000"),
        "{named_socket:?}"
    );

    let mut p = healthy();
    p.env.socket_dir_writable = Some(false);
    let unwritable = named(run(&p, sock(), false), RUNTIME);
    assert!(matches!(unwritable.state, State::Warn), "{unwritable:?}");
    assert!(
        unwritable.reason.contains("/run/user/1000"),
        "{unwritable:?}"
    );
}

#[test]
fn bluez_that_could_not_be_read_warns_without_a_fix_where_no_answer_fails_with_one() {
    // The same host, told two ways: the bus refused, and the bus was never asked.
    let mut refused = healthy();
    refused.bluez.reachable = Err("org.bluez did not answer".into());
    refused.bluez.adapters.clear();
    let service = named(run(&refused, sock(), false), BLUETOOTH_SERVICE);
    assert!(matches!(service.state, State::Fail), "{service:?}");
    assert!(service.fix.is_some(), "{service:?}");
    assert_eq!(exit_code(&run(&refused, sock(), false)), 2);

    let mut unread = refused;
    unread.bluez.unreadable = Some("timed out after 8 s".into());
    unread.env.container = true;
    let out = run(&unread, sock(), true);
    for check in [BLUETOOTH_SERVICE, ADAPTERS, CONTAINER] {
        let row = named(out.clone(), check);
        assert!(matches!(row.state, State::Warn), "{row:?}");
        assert!(row.fix.is_none(), "{row:?}");
        assert!(row.reason.contains("timed out after 8 s"), "{row:?}");
    }
    let extended = named(out.clone(), EXTENDED_ADVERTISING);
    assert!(matches!(extended.state, State::Note), "{extended:?}");
    assert_eq!(exit_code(&out), 1, "{out:?}");
}

#[test]
fn systemd_that_could_not_be_read_is_a_note_that_says_so() {
    let mut p = healthy();
    p.systemd.unreadable = Some("timed out after 8 s".into());
    p.systemd.present = false;
    let daemon = named(run(&p, sock(), false), DAEMON);
    assert!(matches!(daemon.state, State::Note), "{daemon:?}");
    assert!(daemon.reason.contains("could not be read"), "{daemon:?}");
    assert!(daemon.reason.contains("timed out after 8 s"), "{daemon:?}");
}

#[test]
fn an_unparsable_config_fails_with_one_line_of_the_parsers_message() {
    let mut p = healthy();
    p.config = Some(Ok("schema = 1\nthis is not toml\n".into()));
    let config = named(run(&p, sock(), false), CONFIG);
    assert!(matches!(config.state, State::Fail), "{config:?}");
    // The parser lays the offending line out under its message; a reason is read, and
    // logged, as one line, and the first is the one that says where the error is.
    let whole = Config::parse("schema = 1\nthis is not toml\n")
        .unwrap_err()
        .to_string();
    assert!(whole.contains('\n'), "{whole}");
    assert!(!config.reason.contains('\n'), "{config:?}");
    assert_eq!(config.reason, whole.lines().next().unwrap());
    assert!(config.reason.contains("line 2"), "{config:?}");
    assert!(config.fix.is_none(), "{config:?}");
    assert_eq!(exit_code(&run(&p, sock(), false)), 2);
}

#[test]
fn a_device_naming_a_profile_that_does_not_exist_fails() {
    let mut p = healthy();
    p.config = Some(Ok(CONFIG_TEXT.replace("bifrost3", "no-such-profile")));
    let config = named(run(&p, sock(), false), CONFIG);
    assert!(matches!(config.state, State::Fail), "{config:?}");
    assert!(config.reason.contains("no-such-profile"), "{config:?}");
}

#[test]
fn an_unreadable_config_fails_with_what_the_reader_said() {
    let mut p = healthy();
    p.config = Some(Err("/home/u/.config/svolder/config.toml: denied".into()));
    let config = named(run(&p, sock(), false), CONFIG);
    assert!(matches!(config.state, State::Fail), "{config:?}");
    assert!(config.reason.contains("denied"), "{config:?}");
}

#[test]
fn no_device_warns_with_setup_applying_itself() {
    for text in [Some(Ok("schema = 1\n".to_string())), None] {
        let mut p = healthy();
        p.config = text;
        let config = named(run(&p, sock(), false), CONFIG);
        assert!(matches!(config.state, State::Warn), "{config:?}");
        let fix = config.fix.unwrap();
        assert_eq!(fix.command, "svolder setup");
        assert!(fix.applies_itself);
        assert_eq!(exit_code(&run(&p, sock(), false)), 1);
    }
}

#[test]
fn an_outcome_is_one_json_object_of_check_state_reason_fix_and_applies_itself() {
    let mut p = healthy();
    p.systemd.socket_unit = None;
    p.systemd.service_unit = None;
    let value = serde_json::to_value(run(&p, sock(), false)).unwrap();
    let outcomes = value.as_array().unwrap();

    let daemon = outcomes.iter().find(|o| o["check"] == "Daemon").unwrap();
    let keys: Vec<&str> = daemon
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["applies_itself", "check", "fix", "reason", "state"],
        "{daemon}"
    );
    assert_eq!(daemon["state"], "fail");
    assert_eq!(daemon["fix"], "svolder daemon install");
    assert_eq!(daemon["applies_itself"], true);

    let service = outcomes
        .iter()
        .find(|o| o["check"] == "Bluetooth service")
        .unwrap();
    assert_eq!(service["state"], "pass");
    assert_eq!(service["fix"], serde_json::Value::Null);
    assert_eq!(service["applies_itself"], false);
}

/// The commands the two fixes a front end may run itself name are the ones this module
/// publishes, so a front end that dispatches on a command dispatches on what was written.
#[test]
fn the_fixes_a_front_end_applies_carry_the_commands_this_module_names() {
    let mut p = healthy();
    p.systemd.socket_unit = None;
    p.systemd.service_unit = None;
    p.config = None;
    let out = run(&p, sock(), false);
    assert_eq!(
        named(out.clone(), DAEMON).fix.unwrap().command,
        DAEMON_INSTALL
    );
    assert_eq!(named(out, CONFIG).fix.unwrap().command, SETUP);
}

#[test]
fn the_checks_come_out_in_the_order_they_are_read() {
    let mut p = healthy();
    p.env.container = true;
    let names: Vec<&str> = run(&p, sock(), true).iter().map(|o| o.check).collect();
    assert_eq!(
        names,
        [
            BLUETOOTH_SERVICE,
            ADAPTERS,
            RUNTIME,
            DAEMON,
            CONFIG,
            CONTAINER,
            EXTENDED_ADVERTISING,
            BLUETOOTHD_PLUGINS,
            WIREPLUMBER,
        ]
    );
}
