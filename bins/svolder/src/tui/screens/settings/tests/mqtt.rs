// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Connect category's changed-config line and the MQTT rows.

use super::*;

/// While the daemon says its file changed since the last reload, every Connect row's text says
/// so in the daemon's words with the way to reload, and it goes once the daemon says unchanged.
#[test]
fn a_changed_config_is_a_line_on_every_connect_row() {
    let mut app = connect_app();
    let (own, file) = (std::path::PathBuf::from("/x/c.toml"), "/x/c.toml");
    let mut info = daemon_with_config(Some(file));
    info.config_changed = true;
    app.overlays.conn.daemon_said(&info, &own);
    for setting in CATEGORIES[CONNECT].1 {
        select(&mut app, *setting);
        let words = right_words(&app);
        let line = format!(
            "config: {file} changed since the last reload: svolder daemon reload (or SIGHUP)"
        );
        assert!(words.contains(&line), "{setting:?}: {words}");
    }

    // A deleted file is told in the daemon's words for it, not as a change to reload.
    info.config_missing = true;
    app.overlays.conn.daemon_said(&info, &own);
    for setting in CATEGORIES[CONNECT].1 {
        select(&mut app, *setting);
        let words = right_words(&app);
        assert!(
            words.contains(&format!("config: {file} is missing:")),
            "{setting:?}: {words}"
        );
        assert!(
            !words.contains("changed since the last reload"),
            "{setting:?}: {words}"
        );
    }

    info.config_changed = false;
    info.config_missing = false;
    app.overlays.conn.daemon_said(&info, &own);
    for setting in CATEGORIES[CONNECT].1 {
        select(&mut app, *setting);
        assert!(
            !right_words(&app).contains("changed since the last reload"),
            "{setting:?}"
        );
    }
}

/// A config with the broker's host and, as asked, a username and a password.
fn mqtt_config(username: Option<&str>, password: Option<&str>) -> svolder_ipc::Config {
    let mut text = "schema = 1\n[mqtt]\nhost = \"broker.example.com\"\n".to_string();
    if let Some(username) = username {
        text.push_str(&format!("username = \"{username}\"\n"));
    }
    if let Some(password) = password {
        text.push_str(&format!("password = \"{password}\"\n"));
    }
    svolder_ipc::Config::parse(&text).expect("the config parses")
}

/// The username row says the name or `not set`; Enter types it, on what the file holds, and an
/// empty line takes it away.
#[test]
fn the_username_row_types_the_name_and_an_empty_line_clears_it() {
    let mut app = connect_app();
    select(&mut app, Setting::MqttUsername);
    assert_eq!(Setting::MqttUsername.value(&app), "not set");
    assert_eq!(Setting::MqttUsername.label(), "MQTT username");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(shown(&app).contains("MQTT username"), "{}", shown(&app));
    type_text(&mut app, "svolder");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::MqttUsername(Some(
            "svolder".into()
        )))]
    );

    app.overlays
        .conn
        .load_with(&mqtt_config(Some("svolder"), None), false);
    assert_eq!(Setting::MqttUsername.value(&app), "svolder");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.overlays.conn.modal,
        Some(Modal::Editing(Line {
            field: Field::Username,
            text: "svolder".into(),
            caret: 7
        }))
    );
    for _ in 0..7 {
        press(&mut app, KeyCode::Backspace);
    }
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::MqttUsername(None))]
    );
    // An unchanged name asks for nothing.
    press(&mut app, KeyCode::Enter);
    for _ in 0..7 {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "svolder");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
}

/// The password row says set, not set, set but not sent for want of a username, or that the
/// environment's variable wins; the view cannot hold the password.
#[test]
fn the_password_row_says_how_it_stands() {
    let mut app = connect_app();
    let load = |app: &mut App, config: &svolder_ipc::Config, env: bool| {
        app.overlays.conn.load_with(config, env);
        Setting::MqttPassword.value(app)
    };
    assert_eq!(load(&mut app, &mqtt_config(None, None), false), "not set");
    assert_eq!(
        load(
            &mut app,
            &mqtt_config(Some("u"), Some("fake-secret")),
            false
        ),
        "set"
    );
    assert_eq!(
        load(&mut app, &mqtt_config(None, Some("fake-secret")), false),
        "set — not sent: no username"
    );
    assert_eq!(
        load(&mut app, &mqtt_config(Some("u"), Some("fake-secret")), true),
        "from SVOLDER_MQTT_PASSWORD"
    );
    assert_eq!(
        load(&mut app, &mqtt_config(Some("u"), None), true),
        "from SVOLDER_MQTT_PASSWORD"
    );
    assert_eq!(
        load(&mut app, &mqtt_config(None, None), true),
        "from SVOLDER_MQTT_PASSWORD — not sent: no username"
    );
    assert!(!format!("{:?}", app.overlays.conn).contains("fake-secret"));
    select(&mut app, Setting::MqttPassword);
    assert!(
        right_words(&app).contains("SVOLDER_MQTT_PASSWORD"),
        "{}",
        right_words(&app)
    );
}

/// The password is typed on an empty line, masked while typed, shown with Tab, saved as one
/// change whose `Debug` hides it, and an empty line clears a password that is set.
#[test]
fn the_password_is_typed_masked_shown_with_tab_and_never_read_back() {
    let mut app = connect_app();
    app.overlays
        .conn
        .load_with(&mqtt_config(Some("u"), Some("fake-secret")), false);
    select(&mut app, Setting::MqttPassword);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.overlays.conn.modal,
        Some(Modal::Editing(Line {
            field: Field::Password,
            text: String::new(),
            caret: 0
        })),
        "the line starts empty, the password is not read back"
    );
    type_text(&mut app, "hunter2");
    let masked = shown(&app);
    assert!(!masked.contains("hunter2"), "{masked}");
    assert!(masked.contains("•••••••"), "{masked}");
    assert!(
        !format!("{:?}", app.overlays.conn).contains("hunter2"),
        "the modal's Debug hides the password"
    );
    press(&mut app, KeyCode::Tab);
    assert!(shown(&app).contains("hunter2"), "{}", shown(&app));
    press(&mut app, KeyCode::Tab);
    assert!(!shown(&app).contains("hunter2"));
    let saved = press(&mut app, KeyCode::Enter);
    assert_eq!(
        saved,
        vec![Intent::Connection(ConnChange::MqttPassword(Some(
            "hunter2".to_string().into()
        )))]
    );
    assert!(!format!("{saved:?}").contains("hunter2"));
    assert!(app.overlays.conn.modal.is_none());

    // An empty line keeps a password that is in the file; Delete twice clears it.
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("unchanged"), "{toast}");
    press(&mut app, KeyCode::Enter);
    assert!(
        press(&mut app, KeyCode::Delete).is_empty(),
        "one Delete only asks"
    );
    assert!(app.overlays.conn.modal.is_some());
    press(&mut app, KeyCode::Char('x'));
    assert!(
        press(&mut app, KeyCode::Delete).is_empty(),
        "a key between takes the ask back"
    );
    assert_eq!(
        press(&mut app, KeyCode::Delete),
        vec![Intent::Connection(ConnChange::MqttPassword(None))]
    );
    assert!(app.overlays.conn.modal.is_none());
    // Esc keeps it as it was.
    press(&mut app, KeyCode::Enter);
    type_text(&mut app, "typed");
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(app.overlays.conn.modal.is_none());
}

/// An empty line with no password in the file has nothing to clear, and says so; a password
/// saved with no username, or with the variable set, says what keeps it from being used.
#[test]
fn saving_a_password_says_what_would_keep_it_from_being_used() {
    let mut app = connect_app();
    app.overlays.conn.load_with(&mqtt_config(None, None), false);
    select(&mut app, Setting::MqttPassword);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Delete);
    assert!(press(&mut app, KeyCode::Delete).is_empty());
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("no password"), "{toast}");

    press(&mut app, KeyCode::Enter);
    type_text(&mut app, "hunter2");
    assert_eq!(press(&mut app, KeyCode::Enter).len(), 1);
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("no username"), "{toast}");
    assert!(!toast.contains("hunter2"));

    app.overlays
        .conn
        .load_with(&mqtt_config(Some("u"), None), true);
    press(&mut app, KeyCode::Enter);
    type_text(&mut app, "hunter2");
    assert_eq!(press(&mut app, KeyCode::Enter).len(), 1);
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("SVOLDER_MQTT_PASSWORD"), "{toast}");
}

/// The status row says why the bridge is not connected, in the words the CLI uses.
#[test]
fn the_mqtt_status_row_says_the_trouble_in_words() {
    use svolder_ipc::MqttProblem;
    let mut app = connect_app();
    select(&mut app, Setting::MqttStatus);
    for (problem, port, word, line) in [
        (
            MqttProblem::Refused,
            1883,
            "login refused",
            "the broker refused the login (check username and password)",
        ),
        (
            MqttProblem::Unreachable,
            1883,
            "not reachable",
            "not reachable",
        ),
        (
            MqttProblem::Tls,
            8883,
            "TLS port",
            "port 8883 is TLS, which svolder does not speak",
        ),
    ] {
        app.overlays.conn.mqtt_status = Some(MqttStatus::Trouble {
            host: "ha.lan".into(),
            port,
            problem,
            retry_in_secs: Some(4),
        });
        assert_eq!(Setting::MqttStatus.value(&app), word);
        assert!(right_words(&app).contains(line), "{}", right_words(&app));
    }
}

/// Clearing a username the environment also gives says why it comes back.
#[test]
fn clearing_a_username_the_environment_gives_says_so() {
    let mut app = connect_app();
    app.overlays
        .conn
        .load_with(&mqtt_config(Some("svolder"), None), false);
    app.overlays.conn.mqtt.username_in_env = true;
    select(&mut app, Setting::MqttUsername);
    press(&mut app, KeyCode::Enter);
    for _ in 0..7 {
        press(&mut app, KeyCode::Backspace);
    }
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::MqttUsername(None))]
    );
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("SVOLDER_MQTT_USERNAME"), "{toast}");
}
