// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_ipc::{CODE_USAGE, Reason, Response};
use svolder_profiles::tone::preset::NameRefusal;

use super::*;
use crate::handler::tests::daemon_over;

/// A shared daemon reading the file at `path`, with no user profiles.
fn shared(path: &std::path::Path) -> Arc<Daemon> {
    daemon_over(path.to_path_buf(), path.with_file_name("profiles")).shared()
}

fn dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("svolderd-cw-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn curve(gain: f64) -> Preset {
    use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
    preset::from_computer(
        "x",
        Correction {
            preamp_db: -2.0,
            filters: vec![SourceFilter {
                kind: FilterKind::Peaking,
                f: 1000.0,
                g: gain,
                q: 1.0,
            }],
        },
    )
}

/// A preset is saved once per name; a second save under the name is refused in the core's
/// words unless `force`, which replaces the curve; and the snapshot follows each save.
#[tokio::test]
async fn a_preset_is_saved_once_per_name_unless_forced() {
    let dir = dir("save");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let daemon = shared(&path);

    assert_eq!(
        daemon
            .save_preset(" Mine ", curve(3.0), false)
            .await
            .unwrap(),
        "Mine"
    );
    assert_eq!(
        daemon.config_snapshot().presets.len(),
        1,
        "the snapshot follows the save"
    );
    let refused = daemon
        .save_preset("mine", curve(4.0), false)
        .await
        .unwrap_err();
    assert!(
        matches!(refused, ConfigWriteError::Name(NameRefusal::Taken(_))),
        "{refused:?}"
    );
    assert_eq!(refused.reason(), Reason::Taken);
    assert!(refused.to_string().contains("Mine"), "{refused}");
    let blank = daemon
        .save_preset("  ", curve(4.0), false)
        .await
        .unwrap_err();
    assert!(
        matches!(blank, ConfigWriteError::Name(NameRefusal::Name(_))),
        "{blank:?}"
    );
    assert_eq!(blank.reason(), Reason::BadName);
    let builtin = daemon
        .save_preset("Flat", curve(4.0), false)
        .await
        .unwrap_err();
    assert_eq!(builtin.reason(), Reason::BadName, "{builtin}");

    assert_eq!(
        daemon.save_preset("MINE", curve(5.0), true).await.unwrap(),
        "MINE"
    );
    let presets = Config::load_from(&path).unwrap().presets;
    assert_eq!(presets.len(), 1, "{presets:?}");
    assert_eq!(presets[0].name, "MINE");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A write never holds an async thread: a ticker on the same single thread keeps ticking while
/// the edit sleeps.
#[tokio::test]
async fn an_edit_runs_off_the_async_thread() {
    let dir = dir("blocking");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let daemon = shared(&path);
    let ticks = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let ticking = Arc::clone(&ticks);
    let ticker = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(10)).await;
            ticking.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });
    daemon
        .edit_then_reload(Box::new(|_| {
            std::thread::sleep(Duration::from_millis(300));
            Ok(())
        }))
        .await
        .unwrap();
    ticker.abort();
    assert!(
        ticks.load(std::sync::atomic::Ordering::Relaxed) >= 5,
        "the async thread was held"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A writer's refusal comes back as it is, for the caller to match on.
#[tokio::test]
async fn a_writers_refusal_is_handed_back_typed() {
    let dir = dir("refused");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let daemon = shared(&path);
    let refused = daemon
        .edit_then_reload(Box::new(|path| Config::remove_preset(path, "nobody")))
        .await
        .unwrap_err();
    assert!(
        matches!(
            refused,
            ConfigWriteError::Refused(ConfigError::Missing { kind: "preset", .. })
        ),
        "{refused:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A reload that fails after a write (a user profile that will not parse) leaves the write in
/// the file, answers Ok, and the snapshot is the written file. Fails when the hold after the
/// write is taken out: the failed reload never holds one.
#[tokio::test]
async fn a_failed_reload_after_a_write_leaves_the_write_and_its_snapshot() {
    let dir = dir("failed-reload");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    std::fs::write(dir.join("profiles/broken.toml"), "not a profile = [").unwrap();
    let daemon = shared(&path);
    assert!(daemon.reload_all().await.is_err(), "the reload should fail");

    assert_eq!(
        daemon.save_preset("Mine", curve(3.0), false).await.unwrap(),
        "Mine"
    );
    assert_eq!(Config::load_from(&path).unwrap().presets.len(), 1);
    assert_eq!(daemon.config_snapshot().presets.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A reload that outlasts the wait does not hold the answer up: the write is answered Ok, and
/// the snapshot is the written file while the reload is still pending.
#[tokio::test]
async fn a_reload_that_outlasts_the_wait_is_answered_without_it() {
    let dir = dir("slow-reload");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let mut daemon = daemon_over(path.clone(), dir.join("profiles"));
    daemon.reload_wait = Duration::from_millis(100);
    let daemon = daemon.shared();
    let reloading = daemon.reloading.lock().await;
    let started = std::time::Instant::now();
    assert_eq!(
        daemon.save_preset("Mine", curve(3.0), false).await.unwrap(),
        "Mine"
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the wait did not end the answer"
    );
    assert_eq!(daemon.config_snapshot().presets.len(), 1);
    drop(reloading);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A read that started later is never put under by one that started earlier, whichever finishes
/// first: the snapshot never goes back past a write that was answered.
#[tokio::test]
async fn an_older_read_never_replaces_a_newer_one() {
    let dir = dir("monotonic");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let daemon = shared(&path);
    let older = ticket(&daemon.reads);
    let newer = ticket(&daemon.reads);
    let with = |name: &str| {
        let mut config = Config::default();
        config.presets.push(curve(1.0));
        config.presets[0].name = name.to_string();
        config
    };
    daemon.hold_snapshot(with("newer"), newer, None);
    daemon.hold_snapshot(with("older"), older, None);
    assert_eq!(daemon.config_snapshot().presets[0].name, "newer");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two edits in flight at once both end up in the file and in the snapshot.
#[tokio::test]
async fn two_interleaved_edits_both_reach_the_snapshot() {
    let dir = dir("interleaved");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let daemon = shared(&path);
    let (first, second) = tokio::join!(
        daemon.save_preset("First", curve(1.0), false),
        daemon.save_preset("Second", curve(2.0), false)
    );
    first.unwrap();
    second.unwrap();
    assert_eq!(Config::load_from(&path).unwrap().presets.len(), 2);
    assert_eq!(daemon.config_snapshot().presets.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The words of a writability check that did not finish, which the web shows.
#[test]
fn an_unfinished_check_says_it_could_not_be_checked() {
    assert_eq!(
        ConfigWriteError::Unchecked.to_string(),
        "the configuration could not be checked"
    );
}

/// A write under way when the daemon stops is waited for, even when the request that asked for
/// it is gone: the stop returns only once the edit's thread is done, so the end of the runtime
/// never cuts it.
#[tokio::test]
async fn shutdown_waits_for_a_write_under_way() {
    let dir = dir("settle");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let daemon = shared(&path);
    let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (began, finished) = (Arc::clone(&started), Arc::clone(&done));
    let asker = Arc::clone(&daemon);
    let request = tokio::spawn(async move {
        let _ = asker
            .edit_then_reload(Box::new(move |_| {
                began.store(true, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(400));
                finished.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }))
            .await;
    });
    while !started.load(std::sync::atomic::Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    request.abort();
    daemon.shutdown().await;
    assert!(
        done.load(std::sync::atomic::Ordering::SeqCst),
        "the stop returned with the write still running"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every config-write refusal has one reason, whichever way it came: the two refusals of a taken
/// name (the snapshot's and the writer's, which two racing saves reach) read the same.
#[test]
fn a_config_write_refusal_maps_to_one_reason() {
    use std::path::PathBuf;
    let path = PathBuf::from("/x/config.toml");
    let cases = [
        (ConfigWriteError::Unknown, Reason::ReadOnly),
        (ConfigWriteError::Off("off".into()), Reason::ReadOnly),
        (
            ConfigWriteError::Name(NameRefusal::Taken("Mine".into())),
            Reason::Taken,
        ),
        (
            ConfigWriteError::Name(NameRefusal::Reserved("custom".into())),
            Reason::BadName,
        ),
        (
            ConfigWriteError::Refused(ConfigError::Taken {
                kind: "preset",
                name: "Mine".into(),
            }),
            Reason::Taken,
        ),
        (
            ConfigWriteError::Refused(ConfigError::Missing {
                kind: "device",
                name: "x".into(),
            }),
            Reason::Missing,
        ),
        (
            ConfigWriteError::Refused(ConfigError::ReadOnly(path.clone())),
            Reason::ReadOnly,
        ),
        (
            ConfigWriteError::Refused(ConfigError::Busy(path)),
            Reason::FileBusy,
        ),
        (ConfigWriteError::Stopped, Reason::Internal),
        (ConfigWriteError::Unchecked, Reason::Internal),
    ];
    for (why, reason) in cases {
        let words = why.to_string();
        assert_eq!(why.reason(), reason, "{words}");
        let Response::Error {
            code,
            message,
            reason: given,
        } = why.answer()
        else {
            panic!("a refusal is an error answer");
        };
        assert_eq!((code, given), (CODE_USAGE, Some(reason)), "{words}");
        assert!(!message.is_empty());
    }
}

/// A scratch folder that goes when the test ends, a failed one included.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("svolderd-stamp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    Scratch(dir)
}

/// Give the file at `path` the modification time `secs` seconds after a fixed past instant, so
/// two states of a file differ whatever the clock's granularity.
fn stamp_at(path: &std::path::Path, secs: u64) {
    let at = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

/// A daemon that holds the file at `path` as stamped at `secs`.
fn stamped(path: &std::path::Path, text: &str, secs: u64) -> Arc<Daemon> {
    std::fs::write(path, text).unwrap();
    stamp_at(path, secs);
    shared(path)
}

/// A hand edit of the file is a change until the next reload takes it, and the reload clears it
/// without the file changing again.
#[tokio::test]
async fn a_hand_edit_is_a_change_until_the_next_reload() {
    let dir = scratch("hand");
    let path = dir.0.join("config.toml");
    let daemon = stamped(&path, "schema = 1\n", 0);
    assert!(!daemon.config_changed().await, "nothing was edited");

    stamp_at(&path, 5);
    assert!(daemon.config_changed().await, "a newer file");
    stamp_at(&path, 0);
    assert!(
        !daemon.config_changed().await,
        "the file as it was is not a change"
    );
    stamp_at(&path, 3);
    assert!(
        daemon.config_changed().await,
        "a different time, older than the last edit"
    );

    let version = |daemon: Arc<Daemon>| async move {
        use svolder_ipc::Handler;
        match daemon.handle(svolder_ipc::Request::Version).await {
            Response::Version { config_changed, .. } => config_changed,
            other => panic!("not a version answer: {other:?}"),
        }
    };
    assert!(
        version(Arc::clone(&daemon)).await,
        "the version answer says so"
    );
    daemon.reload_all().await.unwrap();
    assert!(!daemon.config_changed().await, "the reload took the file");
    assert!(!version(Arc::clone(&daemon)).await);
    stamp_at(&path, 1);
    assert!(
        daemon.config_changed().await,
        "an older file restored is a change too"
    );
}

/// A write the daemon makes never shows as a change, whichever path wrote it: the read-back
/// after it takes the stamp. Each step starts from a hand edit, so a write that did not take the
/// stamp would leave the change standing.
#[tokio::test]
async fn the_daemons_own_writes_are_not_changes() {
    use svolder_ipc::{DeviceConfigOp, Handler, Request, Server};
    let dir = scratch("own");
    let path = dir.0.join("config.toml");
    let daemon = stamped(&path, "schema = 1\n", 0);

    stamp_at(&path, 5);
    assert!(daemon.config_changed().await);
    daemon.save_preset("Mine", curve(3.0), false).await.unwrap();
    assert!(!daemon.config_changed().await, "save_preset");

    let device = |op| Request::DeviceConfig { op };
    let unit = "0102030405060708".to_string();
    for (step, op) in [
        (
            "add",
            DeviceConfigOp::Add {
                unit: unit.clone(),
                name: "desk".into(),
                profile: None,
                passphrase: None,
            },
        ),
        (
            "update",
            DeviceConfigOp::Update {
                device: "desk".into(),
                name: Some("den".into()),
                profile: None,
                passphrase: None,
            },
        ),
        (
            "remove",
            DeviceConfigOp::Remove {
                device: "den".into(),
            },
        ),
    ] {
        stamp_at(&path, 7);
        assert!(daemon.config_changed().await, "{step}: the hand edit");
        let answer = daemon.handle(device(op)).await;
        assert!(
            matches!(answer, Response::DeviceConfig { .. }),
            "{step}: {answer:?}"
        );
        assert!(!daemon.config_changed().await, "{step}");
    }

    stamp_at(&path, 9);
    assert!(daemon.config_changed().await);
    let answer = daemon
        .handle(Request::ServerSwitch {
            server: Server::Web,
            enabled: false,
        })
        .await;
    assert!(
        matches!(answer, Response::ServerSwitch { .. }),
        "{answer:?}"
    );
    assert!(!daemon.config_changed().await, "server_switch");
}

/// A reload the file refuses leaves the snapshot and its stamp as they were, so the change
/// stays; the refusal itself still goes to whoever asked.
#[tokio::test]
async fn a_refused_reload_leaves_the_change_standing() {
    let dir = scratch("refused");
    let path = dir.0.join("config.toml");
    let daemon = stamped(&path, "schema = 1\n", 0);
    std::fs::write(&path, "this is [not toml").unwrap();
    stamp_at(&path, 4);
    let refusal = daemon.reload_all().await.unwrap_err();
    assert!(refusal.contains("config.toml"), "{refusal}");
    assert!(
        daemon.config_changed().await,
        "the broken file is not taken"
    );

    std::fs::write(&path, "schema = 1\n").unwrap();
    stamp_at(&path, 6);
    daemon.reload_all().await.unwrap();
    assert!(
        !daemon.config_changed().await,
        "a fixed file reloads and clears it"
    );
}

/// A file that goes away, or that appears where there was none, is a change.
#[tokio::test]
async fn a_file_that_goes_or_appears_is_a_change() {
    let dir = scratch("gone");
    let path = dir.0.join("config.toml");
    let daemon = stamped(&path, "schema = 1\n", 0);
    std::fs::remove_file(&path).unwrap();
    assert!(daemon.config_changed().await, "the file went");

    let absent = shared(&dir.0.join("none.toml"));
    assert!(!absent.config_changed().await, "no file held, none there");
    std::fs::write(dir.0.join("none.toml"), "schema = 1\n").unwrap();
    assert!(absent.config_changed().await, "the file appeared");
    absent.reload_all().await.unwrap();
    assert!(!absent.config_changed().await);
}

/// The stamp follows the ticket rule with its snapshot: of two reads that finish out of order,
/// the later read's stamp stays, whichever is held last.
#[tokio::test]
async fn the_stamp_stays_with_the_newer_read() {
    let dir = scratch("ticket");
    let path = dir.0.join("config.toml");
    let daemon = stamped(&path, "schema = 1\n", 0);
    let at = |secs: u64| Some(std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs));
    let older = ticket(&daemon.reads);
    let newer = ticket(&daemon.reads);
    daemon.hold_snapshot(Config::default(), newer, at(8));
    daemon.hold_snapshot(Config::default(), older, at(2));
    stamp_at(&path, 8);
    assert!(
        !daemon.config_changed().await,
        "the older read's stamp did not replace the newer one's"
    );
    stamp_at(&path, 2);
    assert!(daemon.config_changed().await);
}

/// A caller that goes away while the write is on its way (a cancelled job, a web client that
/// left) leaves the write announced all the same: the file's stamp is the snapshot's, so the
/// daemon's own write is no hand edit, and the snapshot holds what was written.
#[tokio::test]
async fn a_caller_dropped_during_the_write_still_leaves_the_snapshot_and_stamp() {
    let dir = scratch("dropped");
    let path = dir.0.join("config.toml");
    let daemon = stamped(&path, "schema = 1\n", 0);
    let preset = curve(3.0);
    let edit: ConfigEdit = Box::new(move |path| {
        std::thread::sleep(Duration::from_millis(200));
        Config::add_preset(path, &preset)
    });
    let waited =
        tokio::time::timeout(Duration::from_millis(50), daemon.edit_then_reload(edit)).await;
    assert!(waited.is_err(), "the caller gave up before the write ended");
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        !daemon.config_changed().await,
        "the daemon's own write read as a hand edit"
    );
    assert_eq!(daemon.config_snapshot().presets.len(), 1);
}

/// A caller dropped during a write that adds a device still leaves the change applied: the
/// reload is part of the task the caller cannot split, so the device runs and the file is not
/// announced as a hand edit.
#[tokio::test]
async fn a_caller_dropped_during_the_write_still_has_the_change_applied() {
    let dir = scratch("dropped-reload");
    let path = dir.0.join("config.toml");
    let daemon = stamped(&path, "schema = 1\n", 0);
    let device = svolder_ipc::config::DeviceConfig {
        name: "Desk".into(),
        udid: "0102030405060708".into(),
        profile: Some("gungnir2-v2".into()),
        passphrase: "1234".into(),
        inputs: Default::default(),
    };
    let edit: ConfigEdit = Box::new(move |path| {
        std::thread::sleep(Duration::from_millis(200));
        Config::add_device(path, &device)
    });
    let waited =
        tokio::time::timeout(Duration::from_millis(50), daemon.edit_then_reload(edit)).await;
    assert!(waited.is_err(), "the caller gave up before the write ended");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!daemon.config_changed().await);
    assert!(
        daemon.resolve(Some("Desk")).is_ok(),
        "the device was never applied"
    );
}

/// A file that was read and is gone is told as missing, not as changed; a reload of it is refused
/// in words naming it and keeps the running configuration (it is not read as an empty one); the
/// file back is read again.
#[tokio::test]
async fn a_missing_file_is_told_as_missing_and_a_reload_of_it_is_refused() {
    use svolder_ipc::Handler;
    let dir = scratch("missing");
    let path = dir.0.join("config.toml");
    let daemon = stamped(
        &path,
        "schema = 1\n\n[[device]]\nname = \"amp\"\nudid = \"0102030405060708\"\n",
        0,
    );
    assert!(!daemon.config_missing().await);
    assert_eq!(daemon.config_snapshot().devices.len(), 1);

    std::fs::remove_file(&path).unwrap();
    assert!(daemon.config_missing().await);
    assert_eq!(
        daemon.config_changed_words().await,
        Some(svolder_ipc::socket::daemon_version::config_missing_line(
            &path.display().to_string()
        ))
    );
    match daemon.handle(svolder_ipc::Request::Version).await {
        Response::Version {
            config_changed,
            config_missing,
            ..
        } => assert!(config_changed && config_missing),
        other => panic!("not a version answer: {other:?}"),
    }

    let refused = daemon.handle(svolder_ipc::Request::Reload).await;
    let Response::Error { code, message, .. } = refused else {
        panic!("a reload of a missing file is refused: {refused:?}");
    };
    assert_eq!(code, CODE_USAGE);
    assert!(
        message.contains("config.toml is missing") && message.contains("keeps its running"),
        "{message}"
    );
    assert!(daemon.reload_all().await.is_err());
    assert_eq!(
        daemon.config_snapshot().devices.len(),
        1,
        "the running configuration is kept"
    );

    std::fs::write(&path, "schema = 1\n").unwrap();
    assert!(!daemon.config_missing().await);
    daemon.reload_all().await.unwrap();
    assert!(daemon.config_snapshot().devices.is_empty());
}

/// A daemon started with no file has none to miss: it reloads, and still holds nothing.
#[tokio::test]
async fn a_daemon_that_never_read_a_file_reloads_without_one() {
    let dir = scratch("never");
    let daemon = shared(&dir.0.join("none.toml"));
    assert!(!daemon.config_missing().await);
    assert!(daemon.reload_all().await.is_ok());
}

/// A write while the file the daemon read is gone is refused in the missing-file words: it keeps
/// the devices and the snapshot and does not make the file again from the template.
#[tokio::test]
async fn a_write_while_the_file_is_gone_is_refused_and_makes_no_file() {
    let dir = scratch("write-gone");
    let path = dir.0.join("config.toml");
    let daemon = stamped(
        &path,
        "schema = 1\n\n[[device]]\nname = \"amp\"\nudid = \"0102030405060708\"\n",
        0,
    );
    std::fs::remove_file(&path).unwrap();
    let refused = daemon
        .save_preset("mine", curve(3.0), false)
        .await
        .unwrap_err();
    assert!(matches!(refused, ConfigWriteError::Off(_)), "{refused:?}");
    assert!(refused.to_string().contains("is missing"), "{refused}");
    assert_eq!(refused.reason(), Reason::ReadOnly);
    assert!(!path.exists(), "the file was made again");
    assert_eq!(daemon.config_snapshot().devices.len(), 1);
    assert!(daemon.config_snapshot().presets.is_empty());
    assert!(daemon.config_missing().await, "the file is still told gone");

    // A daemon that never had a file writes a fresh one.
    let fresh = shared(&dir.0.join("fresh.toml"));
    fresh.save_preset("mine", curve(3.0), false).await.unwrap();
    assert!(dir.0.join("fresh.toml").exists());
}

/// A file deleted after the daemon's check and before its writer reads it, under the lock, is
/// not made again from the template: the write is refused in the missing-file words.
#[tokio::test]
async fn a_file_deleted_between_the_check_and_the_write_is_not_made_again() {
    let dir = scratch("write-race");
    let path = dir.0.join("config.toml");
    let daemon = stamped(
        &path,
        "schema = 1\n\n[[device]]\nname = \"amp\"\nudid = \"0102030405060708\"\n",
        0,
    );
    let refused = daemon
        .edit_then_reload(Box::new(|path| {
            std::fs::remove_file(path).unwrap();
            Config::add_preset(path, &curve(3.0))
        }))
        .await
        .unwrap_err();
    assert!(matches!(refused, ConfigWriteError::Off(_)), "{refused:?}");
    assert!(refused.to_string().contains("is missing"), "{refused}");
    assert_eq!(refused.reason(), Reason::ReadOnly);
    assert!(!path.exists(), "the file was made again");
    assert_eq!(daemon.config_snapshot().devices.len(), 1);
}
