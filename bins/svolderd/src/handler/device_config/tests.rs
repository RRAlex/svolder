// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use svolder_ipc::{
    CODE_USAGE, Config, ConfiguredDevice, DeviceConfigOp, Handler, PassphraseKind, Reason, Request,
    Response,
};

use crate::handler::Daemon;
use crate::handler::tests::daemon_over;

const DAC: &str = "0102030405060708";
const AMP: &str = "1112131415161718";

/// A scratch folder with a configuration file holding `text`, and a shared daemon reading it.
fn daemon_on(tag: &str, text: &str) -> (std::path::PathBuf, std::path::PathBuf, Arc<Daemon>) {
    let dir = std::env::temp_dir().join(format!("svolderd-dc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, text).unwrap();
    let daemon = daemon_over(path.clone(), dir.join("profiles")).shared();
    (dir, path, daemon)
}

async fn ask(daemon: &Daemon, op: DeviceConfigOp) -> Response {
    daemon.handle(Request::DeviceConfig { op }).await
}

/// The devices of an answer that is one.
fn listed(answer: Response) -> Vec<ConfiguredDevice> {
    match answer {
        Response::DeviceConfig { devices, .. } => devices,
        other => panic!("not the configured devices: {other:?}"),
    }
}

/// The reason of an answer that refuses, which is always code 1.
fn refused(answer: Response) -> Reason {
    match answer {
        Response::Error {
            code: CODE_USAGE,
            reason: Some(reason),
            ..
        } => reason,
        other => panic!("not a refusal with a reason: {other:?}"),
    }
}

/// The profiles of an answer that is one.
fn offered(answer: Response) -> Vec<svolder_ipc::ProfileChoice> {
    match answer {
        Response::DeviceConfig { profiles, .. } => profiles,
        other => panic!("not the configured devices: {other:?}"),
    }
}

/// The answer's profiles and the writes' judgement are one list: every name listed is one a
/// pin takes, a name not listed is refused, and a user's profile file counts in both from the
/// moment it is in the folder.
#[tokio::test]
async fn the_listed_profiles_are_the_ones_a_pin_takes() {
    let (dir, _, daemon) = daemon_on("offered", "");
    let folder = dir.join("profiles");
    let before = offered(ask(&daemon, DeviceConfigOp::List).await);
    let builtin = svolder_profiles::Registry::builtin().unwrap();
    let names: Vec<&str> = before.iter().map(|choice| choice.name.as_str()).collect();
    assert_eq!(names, builtin.names());
    let bifrost = before.iter().find(|c| c.name == "bifrost3").unwrap();
    assert_eq!(bifrost.display.as_deref(), Some("Bifrost 3"));
    assert!(!bifrost.base);
    let base = before.iter().find(|c| c.name == "dac-common").unwrap();
    assert_eq!((base.display.as_deref(), base.base), (None, true));
    for choice in &before {
        assert!(super::known_profile(&folder, Some(&choice.name)).is_ok());
    }
    assert!(super::known_profile(&folder, Some("desk-dac")).is_err());
    assert!(super::known_profile(&folder, None).is_ok());

    std::fs::write(
        folder.join("desk-dac.toml"),
        "schema = 1\nname = \"desk-dac\"\nextends = \"bifrost3\"\ndisplay = \"Desk DAC\"\n",
    )
    .unwrap();
    let after = offered(ask(&daemon, DeviceConfigOp::List).await);
    assert_eq!(after.len(), before.len() + 1);
    assert!(after.iter().any(|c| c.name == "desk-dac" && !c.base));
    assert!(super::known_profile(&folder, Some("desk-dac")).is_ok());

    // Profiles that cannot be read: nothing is offered and no pin is taken, while the devices
    // are still listed.
    std::fs::write(folder.join("broken.toml"), "schema = ").unwrap();
    let answer = ask(&daemon, DeviceConfigOp::List).await;
    assert!(matches!(&answer, Response::DeviceConfig { profiles, .. } if profiles.is_empty()));
    for name in ["bifrost3", "desk-dac"] {
        assert!(super::known_profile(&folder, Some(name)).is_err(), "{name}");
    }
    assert!(super::known_profile(&folder, None).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

fn add(unit: &str, name: &str) -> DeviceConfigOp {
    DeviceConfigOp::Add {
        unit: unit.into(),
        name: name.into(),
        profile: None,
        passphrase: None,
    }
}

fn names(daemon: &Daemon) -> Vec<String> {
    daemon.devices().iter().map(|d| d.name.clone()).collect()
}

/// An add writes the block into the daemon's file, the daemon starts the device's actor, and
/// the answer lists the device as an editor needs it, never with its passphrase.
#[tokio::test]
async fn an_add_writes_the_file_starts_the_actor_and_answers_the_devices() {
    let (dir, path, daemon) = daemon_on("add", "schema = 1\n");
    let answer = ask(
        &daemon,
        DeviceConfigOp::Add {
            unit: DAC.into(),
            name: " dac ".into(),
            profile: Some("gungnir2-v2".into()),
            passphrase: Some("1234".into()),
        },
    )
    .await;
    let text = serde_json::to_string(&answer).unwrap();
    assert!(!text.contains("1234"), "the passphrase is in the answer");
    let devices = listed(answer);
    assert_eq!(devices.len(), 1, "{devices:?}");
    assert_eq!(devices[0].name, "dac");
    assert_eq!(devices[0].unit, DAC);
    assert_eq!(devices[0].profile.as_deref(), Some("gungnir2-v2"));
    assert_eq!(devices[0].passphrase, PassphraseKind::Set);
    assert!(!devices[0].default);
    assert_eq!(names(&daemon), ["dac"], "the reload started its actor");
    let file = Config::load_from(&path).unwrap();
    assert_eq!(file.devices[0].passphrase, "1234");

    // A second one with nothing but a unit and a name reads `auto` and has no pin.
    let devices = listed(ask(&daemon, add(AMP, "amp")).await);
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[1].passphrase, PassphraseKind::Auto);
    assert_eq!(devices[1].profile, None);
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("passphrase = \"auto\"")
    );
    assert_eq!(listed(ask(&daemon, DeviceConfigOp::List).await), devices);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// What an add is refused for, each with its reason, and the file left as it was.
#[tokio::test]
async fn an_add_is_refused_with_a_reason() {
    let (dir, path, daemon) = daemon_on("add-refused", "schema = 1\n");
    listed(ask(&daemon, add(DAC, "dac")).await);
    let before = std::fs::read_to_string(&path).unwrap();
    let with = |profile: Option<&str>, passphrase: Option<&str>| DeviceConfigOp::Add {
        unit: AMP.into(),
        name: "amp".into(),
        profile: profile.map(str::to_string),
        passphrase: passphrase.map(str::to_string),
    };
    let cases = [
        (add(AMP, "DAC"), Reason::Taken),
        (add(DAC, "twin"), Reason::Invalid),
        (add("not a unit", "amp"), Reason::Invalid),
        (add(AMP, "  "), Reason::Invalid),
        (add(AMP, "a\nb"), Reason::Invalid),
        (with(Some("no-such-profile"), None), Reason::Invalid),
        (with(None, Some("12345")), Reason::Invalid),
    ];
    for (op, reason) in cases {
        let said = format!("{op:?}");
        assert_eq!(refused(ask(&daemon, op).await), reason, "{said}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before, "{said}");
    }
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// An update changes only the keys it carries, on the device its name or its unit id names;
/// `profile: null` takes the pin off.
#[tokio::test]
async fn an_update_changes_only_what_it_carries() {
    let (dir, path, daemon) = daemon_on(
        "update",
        "default_device = \"dac\"\n\n[[device]]\nname = \"dac\"\nudid = \"0102030405060708\"\n\
         profile = \"gungnir2-v2\"\npassphrase = \"1234\"\n\n[device.inputs]\n1 = \"TV\"\n",
    );
    daemon.reload_all().await.unwrap();
    let update = |device: &str| DeviceConfigOp::Update {
        device: device.into(),
        name: None,
        profile: None,
        passphrase: None,
    };

    // Nothing carried: nothing changed.
    let devices = listed(ask(&daemon, update("DAC")).await);
    assert_eq!(devices[0].passphrase, PassphraseKind::Set);
    assert_eq!(devices[0].profile.as_deref(), Some("gungnir2-v2"));
    assert!(devices[0].default);

    // The passphrase alone, by unit id.
    let mut op = update(&DAC.to_ascii_lowercase());
    if let DeviceConfigOp::Update { passphrase, .. } = &mut op {
        *passphrase = Some("none".into());
    }
    let devices = listed(ask(&daemon, op).await);
    assert_eq!(devices[0].passphrase, PassphraseKind::None);
    assert_eq!(devices[0].profile.as_deref(), Some("gungnir2-v2"));

    // The pin taken off.
    let mut op = update("dac");
    if let DeviceConfigOp::Update { profile, .. } = &mut op {
        *profile = Some(None);
    }
    assert_eq!(listed(ask(&daemon, op).await)[0].profile, None);

    // The name: the default follows it, the inputs stay, the actor runs under the new name.
    let mut op = update("dac");
    if let DeviceConfigOp::Update { name, .. } = &mut op {
        *name = Some("desk".into());
    }
    let devices = listed(ask(&daemon, op).await);
    assert_eq!(devices[0].name, "desk");
    assert!(devices[0].default);
    assert_eq!(names(&daemon), ["desk"]);
    let file = Config::load_from(&path).unwrap();
    assert_eq!(file.default_device.as_deref(), Some("desk"));
    assert_eq!(
        file.devices[0].inputs.get(&1).map(String::as_str),
        Some("TV")
    );
    assert_eq!(file.devices[0].passphrase, "none");

    assert_eq!(refused(ask(&daemon, update("dac")).await), Reason::Missing);
    let mut op = update("desk");
    if let DeviceConfigOp::Update { profile, .. } = &mut op {
        *profile = Some(Some("no-such-profile".into()));
    }
    assert_eq!(refused(ask(&daemon, op).await), Reason::Invalid);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A rename onto another device's name is `taken`.
#[tokio::test]
async fn a_rename_onto_a_taken_name_is_refused() {
    let (dir, _, daemon) = daemon_on("rename-taken", "schema = 1\n");
    listed(ask(&daemon, add(DAC, "dac")).await);
    listed(ask(&daemon, add(AMP, "amp")).await);
    let op = DeviceConfigOp::Update {
        device: "amp".into(),
        name: Some("Dac".into()),
        profile: None,
        passphrase: None,
    };
    assert_eq!(refused(ask(&daemon, op).await), Reason::Taken);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// `set_default` marks the device and `remove` takes it out, the default with it, and stops its
/// actor; a device nothing configures is `missing` for both.
#[tokio::test]
async fn a_default_is_set_and_a_device_removed() {
    let (dir, path, daemon) = daemon_on("default-remove", "schema = 1\n");
    listed(ask(&daemon, add(DAC, "dac")).await);
    listed(ask(&daemon, add(AMP, "amp")).await);
    let set = |device: &str| DeviceConfigOp::SetDefault {
        device: device.into(),
    };
    let remove = |device: &str| DeviceConfigOp::Remove {
        device: device.into(),
    };
    let devices = listed(ask(&daemon, set(AMP)).await);
    assert_eq!(
        devices.iter().map(|d| d.default).collect::<Vec<_>>(),
        [false, true]
    );
    assert_eq!(
        Config::load_from(&path).unwrap().default_device.as_deref(),
        Some("amp"),
        "the default is written under the name the file spells"
    );
    assert_eq!(refused(ask(&daemon, set("nobody")).await), Reason::Missing);
    assert_eq!(
        refused(ask(&daemon, remove("nobody")).await),
        Reason::Missing
    );

    let devices = listed(ask(&daemon, remove("AMP")).await);
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "dac");
    assert_eq!(
        names(&daemon),
        ["dac"],
        "the removed device's actor stopped"
    );
    assert_eq!(Config::load_from(&path).unwrap().default_device, None);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// `clear_default` leaves no default, and acknowledges one that is clear already.
#[tokio::test]
async fn the_default_is_cleared() {
    let (dir, path, daemon) = daemon_on("clear-default", "schema = 1\n");
    listed(ask(&daemon, add(DAC, "dac")).await);
    let set = DeviceConfigOp::SetDefault {
        device: "dac".into(),
    };
    listed(ask(&daemon, set).await);
    assert_eq!(
        Config::load_from(&path).unwrap().default_device.as_deref(),
        Some("dac")
    );
    for _ in 0..2 {
        let devices = listed(ask(&daemon, DeviceConfigOp::ClearDefault).await);
        assert!(devices.iter().all(|device| !device.default));
        assert_eq!(Config::load_from(&path).unwrap().default_device, None);
    }
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file that cannot be written refuses every write as `read_only` and still lists.
#[tokio::test]
async fn a_read_only_file_refuses_a_write_as_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, path, daemon) = daemon_on(
        "read-only",
        "[[device]]\nname = \"dac\"\nudid = \"0102030405060708\"\n",
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(
        refused(ask(&daemon, add(AMP, "amp")).await),
        Reason::ReadOnly
    );
    let remove = DeviceConfigOp::Remove {
        device: "dac".into(),
    };
    assert_eq!(refused(ask(&daemon, remove).await), Reason::ReadOnly);
    assert_eq!(listed(ask(&daemon, DeviceConfigOp::List).await).len(), 1);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// Once the stop has begun a write is refused as `stopping` and the file is left alone.
#[tokio::test]
async fn a_write_after_the_stop_began_is_refused_as_stopping() {
    let (dir, path, daemon) = daemon_on("stopping", "schema = 1\n");
    daemon.shutdown().await;
    assert_eq!(
        refused(ask(&daemon, add(DAC, "dac")).await),
        Reason::Stopping
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "schema = 1\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The view says whether the daemon holds a link, and since when it holds none once it had one.
#[tokio::test(start_paused = true)]
async fn the_view_says_connected_and_since_when_not() {
    let (dir, _, daemon) = daemon_on(
        "view",
        "[[device]]\nname = \"dac\"\nudid = \"0102030405060708\"\npassphrase = \"1234\"\n",
    );
    let before = listed(ask(&daemon, DeviceConfigOp::List).await);
    assert!(!before[0].connected, "no actor yet");
    assert_eq!(before[0].not_connected_since_ms, None);

    daemon.reload_all().await.unwrap();
    for _ in 0..500 {
        if listed(ask(&daemon, DeviceConfigOp::List).await)[0].connected {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let linked = listed(ask(&daemon, DeviceConfigOp::List).await);
    assert!(linked[0].connected);
    assert_eq!(linked[0].not_connected_since_ms, None);

    let handle = daemon.devices()[0].clone();
    let history = Arc::clone(&crate::locks::locked(&handle.info).history);
    crate::locks::locked(&handle.info).connected = false;
    crate::locks::locked(&history).record_for_test(1_700_000_000_000, false);
    let lost = listed(ask(&daemon, DeviceConfigOp::List).await);
    assert!(!lost[0].connected);
    assert_eq!(lost[0].not_connected_since_ms, Some(1_700_000_000_000));
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two writers at once each find their device and write it under the file's one lock, so
/// neither loses the other's change: an update of the profile and one of the passphrase, sent
/// together, both land, round after round.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_updates_at_once_both_land() {
    let (dir, path, daemon) = daemon_on(
        "two-updates",
        "[[device]]\nname = \"dac\"\nudid = \"0102030405060708\"\n",
    );
    for round in 0..12 {
        let code = format!("{:04}", 1000 + round);
        let pin = DeviceConfigOp::Update {
            device: "dac".into(),
            name: None,
            profile: Some(Some("gungnir2-v2".into())),
            passphrase: None,
        };
        let key = DeviceConfigOp::Update {
            device: "dac".into(),
            name: None,
            profile: None,
            passphrase: Some(code.clone()),
        };
        let (pinned, keyed) = tokio::join!(
            tokio::spawn({
                let daemon = Arc::clone(&daemon);
                async move { ask(&daemon, pin).await }
            }),
            tokio::spawn({
                let daemon = Arc::clone(&daemon);
                async move { ask(&daemon, key).await }
            }),
        );
        listed(pinned.unwrap());
        listed(keyed.unwrap());
        let file = Config::load_from(&path).unwrap();
        assert_eq!(
            (
                file.devices[0].profile.as_deref(),
                file.devices[0].passphrase.as_str()
            ),
            (Some("gungnir2-v2"), code.as_str()),
            "round {round}: one update wrote over the other"
        );
        let unpin = DeviceConfigOp::Update {
            device: "dac".into(),
            name: None,
            profile: Some(None),
            passphrase: None,
        };
        listed(ask(&daemon, unpin).await);
    }
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `set_default` and a `remove` of the same device, sent together, never leave the file
/// naming a default it does not configure, whichever goes first.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_remove_racing_a_set_default_leaves_no_dangling_default() {
    let (dir, path, daemon) = daemon_on(
        "default-race",
        "[[device]]\nname = \"dac\"\nudid = \"0102030405060708\"\n",
    );
    for round in 0..12 {
        listed(ask(&daemon, add(AMP, "amp")).await);
        let set = DeviceConfigOp::SetDefault {
            device: "amp".into(),
        };
        let remove = DeviceConfigOp::Remove {
            device: "amp".into(),
        };
        let (set, removed) = tokio::join!(
            tokio::spawn({
                let daemon = Arc::clone(&daemon);
                async move { ask(&daemon, set).await }
            }),
            tokio::spawn({
                let daemon = Arc::clone(&daemon);
                async move { ask(&daemon, remove).await }
            }),
        );
        listed(removed.unwrap());
        // The default is set and then removed with the device, or refused as missing.
        match set.unwrap() {
            Response::DeviceConfig { .. } => {}
            other => assert_eq!(refused(other), Reason::Missing, "round {round}"),
        }
        let file = Config::load_from(&path).unwrap();
        assert_eq!(file.devices.len(), 1, "round {round}");
        assert_eq!(
            file.default_device, None,
            "round {round}: the file names a default it does not configure"
        );
    }
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// While another writer holds the file's lock past the wait, a write is refused as `file_busy`
/// and nothing is written.
#[tokio::test]
async fn a_write_that_waits_out_the_files_lock_is_refused_as_file_busy() {
    let (dir, path, daemon) = daemon_on("file-busy", "schema = 1\n");
    let mut lock = path.clone().into_os_string();
    lock.push(".lock");
    let held = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&lock)
        .unwrap();
    held.lock().expect("the test takes the file's lock");
    assert_eq!(
        refused(ask(&daemon, add(DAC, "dac")).await),
        Reason::FileBusy
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "schema = 1\n");
    drop(held);
    listed(ask(&daemon, add(DAC, "dac")).await);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}
