// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::rc::Rc;

use super::*;
use crate::test_support::TempDir;
use crate::which::is_executable;

/// What a recording player has been asked, and what has happened to what it started.
#[derive(Default)]
pub(in crate::tui) struct Log {
    /// The arguments of each play.
    pub plays: Vec<Vec<OsString>>,
    /// How many clips have been stopped.
    pub stops: usize,
}

pub(in crate::tui) struct Recorder(pub Rc<RefCell<Log>>);

struct Clip(Rc<RefCell<Log>>);

impl Player for Recorder {
    fn play(&self, args: &[OsString]) -> io::Result<Box<dyn Playing>> {
        self.0.borrow_mut().plays.push(args.to_vec());
        Ok(Box::new(Clip(Rc::clone(&self.0))))
    }
}

impl Playing for Clip {
    fn stop(&mut self) {
        self.0.borrow_mut().stops += 1;
    }
}

/// A player that cannot run anything.
struct Broken;

impl Player for Broken {
    fn play(&self, _: &[OsString]) -> io::Result<Box<dyn Playing>> {
        Err(io::Error::other("no pw-play"))
    }
}

/// A scratch directory of the test's own, removed when the test ends.
pub(in crate::tui) fn scratch(name: &str) -> TempDir {
    let dir =
        std::env::temp_dir().join(format!("svolder-intro-sound-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    TempDir(dir)
}

/// A sound over a recording player, and what the player is asked.
pub(in crate::tui) fn recorded(name: &str) -> (IntroSound, Rc<RefCell<Log>>, TempDir) {
    let log = Rc::new(RefCell::new(Log::default()));
    let dir = scratch(name);
    let sound = IntroSound::new(Box::new(Recorder(Rc::clone(&log))), &dir);
    (sound, log, dir)
}

/// The words of an argument list.
fn words(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn each_raw_rate_is_its_own_hertz() {
    let hz = [
        (1, 44_100),
        (2, 48_000),
        (3, 88_200),
        (4, 96_000),
        (5, 176_400),
        (6, 192_000),
        (7, 352_800),
        (8, 384_000),
    ];
    for (raw, want) in hz {
        assert_eq!(hz_of(Some(raw)), Some(want), "raw {raw}");
    }
}

#[test]
fn no_rate_for_zero_for_out_of_range_and_for_no_reading() {
    assert_eq!(hz_of(Some(0)), None);
    assert_eq!(hz_of(Some(9)), None);
    assert_eq!(hz_of(Some(-1)), None);
    assert_eq!(hz_of(Some(255)), None);
    assert_eq!(hz_of(None), None);
}

#[test]
fn the_rate_is_read_by_key_from_the_readings() {
    use std::time::Instant;
    use svolder_ipc::ValueObject;

    fn reading(id: &str, key: Option<&str>, raw: i64) -> (FunctionId, Reading) {
        let value = ValueObject {
            heard_at_ms: None,
            id: id.into(),
            key: key.map(Into::into),
            label: "x".into(),
            value_type: "u8".into(),
            raw: serde_json::json!(raw),
            display: "x".into(),
            value: None,
            unit: None,
        };
        (
            id.parse().expect("a function id"),
            Reading {
                value,
                received: Instant::now(),
                rate: Default::default(),
            },
        )
    }
    let mut readings = BTreeMap::new();
    assert_eq!(rate_of(&readings), None, "no readings");
    readings.extend([reading("10", Some("volume"), 3)]);
    assert_eq!(rate_of(&readings), None, "no sample rate control");
    // Fid 9 (Mimir, Mesh Card, Eitr) and fid 6 (Gungnir) are both found by the key.
    readings.extend([reading("9", Some("sample_rate"), 4)]);
    assert_eq!(rate_of(&readings), Some(96_000));
    readings.extend([reading("9", Some("sample_rate"), 0)]);
    assert_eq!(rate_of(&readings), None, "raw zero asks for no rate");
}

#[test]
fn the_arguments_for_a_rate_and_for_none() {
    let file = Path::new("/run/user/1000/svolder-intro-1.ogg");
    assert_eq!(
        words(&arguments(Some(44_100), file)),
        [
            "-P",
            "{ media.name = \"svolder intro\" application.name = \"svolder\" node.rate = 1/44100 }",
            "/run/user/1000/svolder-intro-1.ogg",
        ]
    );
    assert_eq!(
        words(&arguments(None, file)),
        [
            "-P",
            "{ media.name = \"svolder intro\" application.name = \"svolder\" }",
            "/run/user/1000/svolder-intro-1.ogg",
        ]
    );
}

#[test]
fn the_clip_is_the_one_shipped() {
    assert_eq!(&CLIP[..4], b"OggS");
    assert_eq!(CLIP.len(), 84_533);
}

#[test]
fn starting_writes_a_private_file_and_plays_it_once() {
    let (mut sound, log, dir) = recorded("once");
    sound.start(Some(48_000));
    sound.start(Some(96_000));
    assert_eq!(log.borrow().plays.len(), 1, "a second start plays nothing");
    let file = dir.join(format!("svolder-intro-{}.ogg", std::process::id()));
    assert_eq!(std::fs::read(&file).expect("the clip is written"), CLIP);
    assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(log.borrow().plays[0][2], file.as_os_str());
    assert!(words(&log.borrow().plays[0])[1].contains("node.rate = 1/48000"));
    drop(sound);
    assert!(!file.exists(), "leaving removes the file");
    assert_eq!(log.borrow().stops, 1);
}

#[test]
fn a_file_left_by_an_earlier_run_is_written_over() {
    let (mut sound, _log, dir) = recorded("leftover");
    let file = dir.join(format!("svolder-intro-{}.ogg", std::process::id()));
    std::fs::write(&file, b"stale").unwrap();
    sound.start(None);
    assert_eq!(std::fs::read(&file).unwrap(), CLIP);
}

#[test]
fn stopping_kills_the_clip_and_removes_the_file() {
    let (mut sound, log, dir) = recorded("stop");
    sound.start(None);
    sound.stop();
    assert_eq!(log.borrow().stops, 1);
    assert!(
        !dir.join(format!("svolder-intro-{}.ogg", std::process::id()))
            .exists()
    );
    sound.stop();
    assert_eq!(log.borrow().stops, 1, "nothing is stopped twice");
}

#[test]
fn a_player_that_fails_is_silent_and_leaves_nothing_playing() {
    let dir = scratch("broken");
    let mut sound = IntroSound::new(Box::new(Broken), &dir);
    sound.start(Some(48_000));
    sound.stop();
    assert!(sound.playing.is_none());
}

#[test]
fn a_directory_that_cannot_be_written_is_silent() {
    let log = Rc::new(RefCell::new(Log::default()));
    let mut sound = IntroSound::new(
        Box::new(Recorder(Rc::clone(&log))),
        Path::new("/nonexistent-svolder-dir"),
    );
    sound.start(None);
    assert!(log.borrow().plays.is_empty());
}

fn wanted() -> TuiConfig {
    TuiConfig {
        intro_sound: true,
        ..TuiConfig::default()
    }
}

#[test]
fn the_sound_is_planned_only_when_every_condition_holds() {
    let dir = Path::new("/tmp");
    let pw = || Some(PathBuf::from("/usr/bin/pw-play"));
    assert!(IntroSound::planned(&wanted(), false, pw(), dir).is_some());

    let mut off = wanted();
    off.intro_sound = false;
    assert!(
        IntroSound::planned(&off, false, pw(), dir).is_none(),
        "intro_sound off"
    );
    let mut off = wanted();
    off.intro = false;
    assert!(
        IntroSound::planned(&off, false, pw(), dir).is_none(),
        "intro off"
    );
    let mut off = wanted();
    off.host_audio = false;
    assert!(
        IntroSound::planned(&off, false, pw(), dir).is_none(),
        "host_audio off"
    );
    assert!(
        IntroSound::planned(&wanted(), true, pw(), dir).is_none(),
        "SVOLDER_HOST=off"
    );
    assert!(
        IntroSound::planned(&wanted(), false, None, dir).is_none(),
        "no pw-play"
    );
}

#[test]
fn the_default_config_plays_no_sound() {
    assert!(!TuiConfig::default().intro_sound);
}

#[test]
fn pw_play_is_found_on_the_path_only_when_it_can_be_run() {
    let dir = scratch("path");
    let bin = dir.join("pw-play");
    std::fs::write(&bin, b"#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!is_executable(&bin), "a file nobody may run is not found");
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(is_executable(&bin));
    assert!(!is_executable(&dir), "a directory is not a program");
}

/// The intro and the open, run together as `svolder tui` runs them, on a paused clock.
mod flow {
    use std::time::Duration;

    use crossterm::event::{Event, KeyCode};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::backend::Failure;
    use crate::recovery::Kind;
    use crate::tui::decoration::intro::test_event_stream;
    use crate::tui::start::intro_then_open;
    use crate::tui::theme::Theme;

    /// How long an open takes to answer, and what it answers with.
    enum Open {
        Device(Duration, Option<u32>),
        Failing(Duration),
    }

    /// Run the intro over an open, with a key sent `key_at` into it when asked, and answer what
    /// the log shows afterwards and the opened rate.
    async fn run(
        name: &str,
        open: Open,
        key_at: Option<Duration>,
    ) -> (Rc<RefCell<Log>>, Option<Result<Option<u32>, Failure>>) {
        let (mut sound, log, _dir) = recorded(name);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal");
        let (mut events, tx) = test_event_stream();
        let keys = async {
            if let Some(at) = key_at {
                tokio::time::sleep(at).await;
                tx.send(Ok(Event::Key(KeyCode::Esc.into()))).expect("a key");
            }
            std::future::pending::<()>().await
        };
        let mut opening = Box::pin(async move {
            match open {
                Open::Device(after, rate) => {
                    tokio::time::sleep(after).await;
                    Ok(rate)
                }
                Open::Failing(after) => {
                    tokio::time::sleep(after).await;
                    Err(Failure::of(Kind::Usage, "no device"))
                }
            }
        });
        let theme = Theme::new(false);
        let mut plan = crate::tui::decoration::intro::IntroPlan::fade(true);
        let ((opened, _gave_up, _), ()) = tokio::select! {
            done = intro_then_open(
                &mut terminal,
                &mut events,
                &theme,
                &mut plan,
                &mut opening,
                Some(&mut sound),
                |rate: &Option<u32>| *rate,
            ) => (done, ()),
            () = keys => unreachable!("the keys never end"),
        };
        // The sound is still held, as the run holds it through the device screen.
        drop(sound);
        (log, opened)
    }

    #[tokio::test(start_paused = true)]
    async fn a_device_answering_over_the_mark_starts_the_sound_at_its_rate() {
        let (log, opened) = run(
            "flow-a",
            Open::Device(Duration::from_millis(300), Some(44_100)),
            None,
        )
        .await;
        assert!(matches!(opened, Some(Ok(Some(44_100)))));
        let log = log.borrow();
        assert_eq!(log.plays.len(), 1);
        assert!(words(&log.plays[0])[1].contains("node.rate = 1/44100"));
        // The mark ran to its end on its own, so the sound was left to play: only the drop at
        // the end of the run stopped it.
        assert_eq!(log.stops, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_device_with_no_rate_starts_the_sound_with_none_asked_for() {
        let (log, _) = run(
            "flow-b",
            Open::Device(Duration::from_millis(300), None),
            None,
        )
        .await;
        let log = log.borrow();
        assert_eq!(log.plays.len(), 1);
        assert!(!words(&log.plays[0])[1].contains("node.rate"));
    }

    #[tokio::test(start_paused = true)]
    async fn a_skip_key_stops_the_sound_while_the_mark_is_up() {
        let (mut sound, log, _dir) = recorded("flow-c");
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal");
        let (mut events, tx) = test_event_stream();
        let mut opening = Box::pin(async { Ok::<_, Failure>(Some(48_000u32)) });
        tx.send(Ok(Event::Key(KeyCode::Esc.into()))).expect("a key");
        let theme = Theme::new(false);
        let (opened, _, _) = intro_then_open(
            &mut terminal,
            &mut events,
            &theme,
            &mut crate::tui::decoration::intro::IntroPlan::fade(true),
            &mut opening,
            Some(&mut sound),
            |rate: &Option<u32>| *rate,
        )
        .await;
        assert!(opened.is_some());
        let log = log.borrow();
        assert_eq!(
            log.plays.len(),
            1,
            "the device answered first, so it started"
        );
        assert_eq!(log.stops, 1, "the key stopped it before the run ended");
    }

    #[tokio::test(start_paused = true)]
    async fn an_intro_skipped_before_the_device_answers_plays_nothing() {
        let (log, opened) = run(
            "flow-d",
            Open::Device(Duration::from_secs(10), Some(48_000)),
            Some(Duration::from_millis(200)),
        )
        .await;
        assert!(opened.is_none());
        assert!(log.borrow().plays.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn an_intro_that_ends_before_the_device_answers_plays_nothing() {
        let (log, opened) = run(
            "flow-e",
            Open::Device(Duration::from_secs(10), Some(48_000)),
            None,
        )
        .await;
        assert!(opened.is_none());
        assert!(log.borrow().plays.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn an_open_that_fails_plays_nothing() {
        let (log, opened) = run("flow-f", Open::Failing(Duration::from_millis(300)), None).await;
        assert!(matches!(opened, Some(Err(_))));
        assert!(log.borrow().plays.is_empty());
    }

    /// A device that answers during the build starts the sound at once, and the build still
    /// runs its whole length and hands its exit on.
    #[tokio::test(start_paused = true)]
    async fn a_device_answering_mid_build_starts_the_sound_and_the_build_runs_on() {
        let (mut sound, log, _dir) = recorded("flow-p");
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal");
        let (mut events, _tx) = test_event_stream();
        let mut opening = Box::pin(async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            Ok::<_, Failure>(Some(48_000u32))
        });
        let theme = Theme::from_config(
            &svolder_ipc::TuiConfig::default(),
            false,
            svolder_ipc::ColorDepth::TrueColor,
        );
        let started = tokio::time::Instant::now();
        let (opened, gave_up, exit) = intro_then_open(
            &mut terminal,
            &mut events,
            &theme,
            &mut crate::tui::decoration::intro::IntroPlan::particles(7),
            &mut opening,
            Some(&mut sound),
            |rate: &Option<u32>| *rate,
        )
        .await;
        assert!(opened.is_some() && !gave_up && exit.is_some());
        assert!(started.elapsed() >= Duration::from_millis(3600));
        assert_eq!(log.borrow().plays.len(), 1);
        assert_eq!(
            log.borrow().stops,
            0,
            "a full intro leaves the sound playing"
        );
    }
}

/// A stand-in for `pw-play`: a script of the test's own that waits, so nothing is played. The
/// directory holding it goes when the returned guard does.
fn stand_in(name: &str, body: &str) -> (PwPlay, TempDir) {
    let dir = scratch(name);
    let script = dir.join("pw-play");
    crate::test_support::install_script(&script, &format!("#!/bin/sh\n{body}\n"));
    (PwPlay(script), dir)
}

#[test]
fn the_real_player_is_killed_by_stop_and_reaped() {
    let started = std::time::Instant::now();
    let (player, _dir) = stand_in("kill", "exec sleep 30");
    let mut playing = player.play(&["-P".into()]).expect("the stand-in runs");
    playing.stop();
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "stop waited for the clip"
    );
}

#[test]
fn a_clip_that_ends_by_itself_is_reaped_without_a_stop() {
    let (player, _dir) = stand_in("ends", "exit 3");
    let child = player.spawn(&[]).expect("the stand-in runs");
    let proc_entry = PathBuf::from(format!("/proc/{}", child.id()));
    let child = Arc::new(Mutex::new(child));
    spawn_reaper(Arc::clone(&child));
    std::thread::sleep(REAP_EVERY * 3);
    // A zombie keeps its /proc entry until it is waited on, so the entry being gone before this
    // test asks anything proves the reaper waited on it.
    assert!(!proc_entry.exists(), "the ended clip is still a zombie");
    // Already reaped by the reaper: its status is there to read and a kill has nothing to hit.
    let status = locked(&child).try_wait().expect("the child can be asked");
    assert_eq!(status.and_then(|s| s.code()), Some(3));
    let mut playing = Running(child);
    // Stopping a clip that has already ended and been reaped is harmless.
    playing.stop();
}

#[test]
fn a_symlink_planted_at_the_path_is_not_followed() {
    let dir = scratch("symlink");
    let target = dir.join("victim");
    std::fs::write(&target, b"untouched").unwrap();
    let path = dir.join("planted.ogg");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    write_private(&path, CLIP).expect("the link is replaced by a file");
    assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
    assert!(!path.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(&path).unwrap(), CLIP);
    // A link that dangles is no way in either.
    let gone = dir.join("gone.ogg");
    std::os::unix::fs::symlink(dir.join("never-made"), &gone).unwrap();
    write_private(&gone, CLIP).expect("the dangling link is replaced");
    assert!(!dir.join("never-made").exists());
}
