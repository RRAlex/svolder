// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MPRIS client against a private bus: each case starts a `dbus-daemon` of its own on a
//! socket in its own directory, registers fake players on it, and ends the daemon when it is
//! done. Nothing here reaches the session bus this process was started with.
//!
//! A host without `dbus-daemon` skips these cases, saying so.

mod common;

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::TempDir;
use dbus::Path;
use dbus::arg::{PropMap, RefArg, Variant};
use dbus::channel::{Channel, MatchingReceiver, Sender};
use dbus::message::{MatchRule, SignalArgs};
use dbus::nonblock::SyncConnection;
use dbus::nonblock::stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged;
use futures::StreamExt;
use svolder_host::players::{Button, Changes, Mpris, NoBus, NowPlaying, Players, Status};
use tokio::task::JoinHandle;

/// How long a case waits for the client to see what a fake did.
const WAIT: Duration = Duration::from_secs(5);

const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const ROOT: &str = "org.mpris.MediaPlayer2";

/// A `dbus-daemon` of the case's own, listening in the case's directory, ended when the case
/// lets go of it.
struct PrivateBus {
    daemon: Child,
    address: String,
    _dir: TempDir,
}

impl PrivateBus {
    /// Start one, or `None` (with a line saying the case is skipped) when the host has no
    /// `dbus-daemon`.
    fn start(label: &str) -> Option<Self> {
        let dir = TempDir::new(label);
        let listen = format!("--address=unix:path={}", dir.join("bus").display());
        let spawned = Command::new("dbus-daemon")
            .args(["--session", "--print-address", "--nofork", &listen])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let daemon = match spawned {
            Ok(daemon) => daemon,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("skipped: dbus-daemon is not installed, so there is no private bus");
                return None;
            }
            Err(e) => panic!("dbus-daemon did not start: {e}"),
        };
        // The guard holds the daemon before anything below can fail, so a daemon that prints
        // no address is ended with the case rather than left running.
        let mut bus = Self {
            daemon,
            address: String::new(),
            _dir: dir,
        };
        let output = bus.daemon.stdout.take().expect("the daemon's stdout");
        let (sending, printed) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut address = String::new();
            let _ = BufReader::new(output).read_line(&mut address);
            let _ = sending.send(address);
        });
        let address = printed
            .recv_timeout(WAIT)
            .expect("the daemon prints its address in time");
        bus.address = address.trim().to_string();
        assert!(!bus.address.is_empty(), "the daemon printed no address");
        Some(bus)
    }

    /// End the daemon now, as a bus going away would.
    fn end(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        self.end();
    }
}

/// What a fake player says about itself, and every transport call it was sent.
#[derive(Debug, Clone)]
struct State {
    identity: String,
    status: &'static str,
    title: String,
    artist: String,
    /// The `xesam:genre` strings the metadata carries; none leaves the key out.
    genre: Vec<String>,
    /// The `xesam:album` string; empty leaves the key out.
    album: String,
    /// The `mpris:artUrl` string; none leaves the key out.
    art_url: Option<String>,
    /// The `mpris:length` in microseconds; none leaves the key out.
    length: Option<i64>,
    /// What `Position` answers, in microseconds; none makes the property unknown.
    position: Option<i64>,
    /// What `Rate` answers; none makes the property unknown.
    rate: Option<f64>,
    calls: Vec<String>,
    /// How many of the next `GetAll` calls go unanswered, as a player too busy to answer.
    ignored_get_alls: usize,
}

/// A player on the private bus: it answers `Get` and `GetAll` from its [`State`], records every
/// MPRIS player method sent to it, and signals `PropertiesChanged` when a case changes it.
struct FakePlayer {
    connection: Arc<SyncConnection>,
    state: Arc<Mutex<State>>,
    driving: JoinHandle<()>,
}

impl FakePlayer {
    async fn start(bus: &PrivateBus, name: &str, state: State) -> Self {
        let connection = connect(&bus.address).await;
        let (connection, driving) = connection;
        let state = Arc::new(Mutex::new(state));
        let answering = state.clone();
        connection.start_receive(
            MatchRule::new_method_call(),
            Box::new(move |call, connection| {
                if let Some(reply) = answer(&call, &answering) {
                    let _ = connection.send(reply);
                }
                true
            }),
        );
        connection
            .request_name(name.to_string(), false, true, true)
            .await
            .expect("the fake player takes its name");
        Self {
            connection,
            state,
            driving,
        }
    }

    /// Change the status and signal it, as a player does when it starts or pauses.
    fn set_status(&self, status: &'static str) {
        self.state.lock().unwrap().status = status;
        let mut changed = PropMap::new();
        changed.insert(
            "PlaybackStatus".to_string(),
            Variant(Box::new(status.to_string())),
        );
        self.signal(changed);
    }

    /// Change the track and signal it, as a player does on to the next one.
    fn set_track(&self, title: &str, artist: &str) {
        let metadata = {
            let mut state = self.state.lock().unwrap();
            state.title = title.to_string();
            state.artist = artist.to_string();
            // A new track starts at its beginning.
            state.position = state.position.map(|_| 0);
            metadata(&state)
        };
        let mut changed = PropMap::new();
        changed.insert("Metadata".to_string(), Variant(Box::new(metadata)));
        self.signal(changed);
    }

    /// Change the track and signal it, as a player does that updates `Position` only a moment
    /// after the metadata: a read between the two finds the last track's position.
    fn set_track_then_position(&self, title: &str, artist: &str, position: i64, after: Duration) {
        let metadata = {
            let mut state = self.state.lock().unwrap();
            state.title = title.to_string();
            state.artist = artist.to_string();
            metadata(&state)
        };
        let mut changed = PropMap::new();
        changed.insert("Metadata".to_string(), Variant(Box::new(metadata)));
        self.signal(changed);
        let state = self.state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(after).await;
            state.lock().unwrap().position = Some(position);
        });
    }

    /// Move the position and signal `Seeked` with it, as a player does when it is seeked.
    fn seek(&self, position: i64) {
        self.state.lock().unwrap().position = Some(position);
        let message = dbus::Message::new_signal(MPRIS_PATH, PLAYER, "Seeked")
            .expect("the Seeked signal is built")
            .append1(position);
        self.connection.send(message).expect("the signal is sent");
    }

    /// Change the rate and the position it was changed at, and signal the rate alone, as a
    /// player does when it speeds up.
    fn set_rate(&self, rate: f64, position: i64) {
        {
            let mut state = self.state.lock().unwrap();
            state.rate = Some(rate);
            state.position = Some(position);
        }
        let mut changed = PropMap::new();
        changed.insert("Rate".to_string(), Variant(Box::new(rate)));
        self.signal(changed);
    }

    /// Change the status and signal only that it changed, leaving the client to ask for it.
    fn invalidate_status(&self, status: &'static str) {
        self.state.lock().unwrap().status = status;
        self.signal_with(PropMap::new(), vec!["PlaybackStatus".to_string()]);
    }

    fn signal(&self, changed: PropMap) {
        self.signal_with(changed, Vec::new());
    }

    fn signal_with(&self, changed: PropMap, invalidated: Vec<String>) {
        let signal = PropertiesPropertiesChanged {
            interface_name: PLAYER.to_string(),
            changed_properties: changed,
            invalidated_properties: invalidated,
        };
        let message = signal.to_emit_message(&Path::new(MPRIS_PATH).unwrap());
        self.connection.send(message).expect("the signal is sent");
    }

    /// Leave the next `GetAll` unanswered, as a player too busy to answer does.
    fn ignore_next_get_all(&self) {
        self.state.lock().unwrap().ignored_get_alls += 1;
    }

    fn calls(&self) -> Vec<String> {
        self.state.lock().unwrap().calls.clone()
    }
}

impl Drop for FakePlayer {
    /// Ending the connection takes the player's name off the bus.
    fn drop(&mut self) {
        self.driving.abort();
    }
}

/// A connection to the bus at `address`, and the task driving it.
async fn connect(address: &str) -> (Arc<SyncConnection>, JoinHandle<()>) {
    let address = address.to_string();
    let channel = tokio::task::spawn_blocking(move || {
        let mut channel = Channel::open_private(&address).expect("the private bus opens");
        channel
            .register()
            .expect("the private bus registers the connection");
        channel
    })
    .await
    .unwrap();
    let (resource, connection) =
        dbus_tokio::connection::from_channel::<SyncConnection>(channel).unwrap();
    let driving = tokio::spawn(async move {
        let _ = resource.await;
    });
    (connection, driving)
}

fn metadata(state: &State) -> HashMap<String, Variant<Box<dyn RefArg>>> {
    let mut metadata: HashMap<String, Variant<Box<dyn RefArg>>> = HashMap::new();
    metadata.insert(
        "mpris:trackid".to_string(),
        Variant(Box::new(Path::new("/org/fake/track/1").unwrap())),
    );
    metadata.insert(
        "xesam:title".to_string(),
        Variant(Box::new(state.title.clone())),
    );
    metadata.insert(
        "xesam:artist".to_string(),
        Variant(Box::new(vec![state.artist.clone()])),
    );
    if !state.genre.is_empty() {
        metadata.insert(
            "xesam:genre".to_string(),
            Variant(Box::new(state.genre.clone())),
        );
    }
    if !state.album.is_empty() {
        metadata.insert(
            "xesam:album".to_string(),
            Variant(Box::new(state.album.clone())),
        );
    }
    if let Some(art_url) = &state.art_url {
        metadata.insert(
            "mpris:artUrl".to_string(),
            Variant(Box::new(art_url.clone())),
        );
    }
    if let Some(length) = state.length {
        metadata.insert("mpris:length".to_string(), Variant(Box::new(length)));
    }
    metadata
}

/// The fake player's answer to one method call, or none for a `GetAll` it ignores.
fn answer(call: &dbus::Message, state: &Mutex<State>) -> Option<dbus::Message> {
    let mut state = state.lock().unwrap();
    let interface = call.interface().map(|i| i.to_string()).unwrap_or_default();
    let member = call.member().map(|m| m.to_string()).unwrap_or_default();
    Some(match (interface.as_str(), member.as_str()) {
        ("org.freedesktop.DBus.Properties", "GetAll") if state.ignored_get_alls > 0 => {
            state.ignored_get_alls -= 1;
            return None;
        }
        ("org.freedesktop.DBus.Properties", "GetAll") => {
            let asked: String = call.read1().unwrap_or_default();
            let mut props = PropMap::new();
            if asked == PLAYER {
                props.insert(
                    "PlaybackStatus".to_string(),
                    Variant(Box::new(state.status.to_string())),
                );
                props.insert("Metadata".to_string(), Variant(Box::new(metadata(&state))));
            } else if asked == ROOT {
                props.insert(
                    "Identity".to_string(),
                    Variant(Box::new(state.identity.clone())),
                );
            }
            call.method_return().append1(props)
        }
        ("org.freedesktop.DBus.Properties", "Get") => {
            let (asked, property): (String, String) = call.read2().unwrap_or_default();
            match (asked.as_str(), property.as_str()) {
                (ROOT, "Identity") => call
                    .method_return()
                    .append1(Variant(state.identity.clone())),
                (PLAYER, "PlaybackStatus") => call
                    .method_return()
                    .append1(Variant(state.status.to_string())),
                (PLAYER, "Position") => match state.position {
                    Some(position) => call.method_return().append1(Variant(position)),
                    None => unknown(call),
                },
                (PLAYER, "Rate") => match state.rate {
                    Some(rate) => call.method_return().append1(Variant(rate)),
                    None => unknown(call),
                },
                _ => unknown(call),
            }
        }
        (PLAYER, "Play" | "Pause" | "PlayPause" | "Stop" | "Next" | "Previous") => {
            state.calls.push(member);
            call.method_return()
        }
        _ => unknown(call),
    })
}

fn unknown(call: &dbus::Message) -> dbus::Message {
    call.error(
        &"org.freedesktop.DBus.Error.UnknownMethod".into(),
        c"the fake player does not do that",
    )
}

fn state(identity: &str, status: &'static str, title: &str, artist: &str) -> State {
    State {
        identity: identity.to_string(),
        status,
        title: title.to_string(),
        artist: artist.to_string(),
        genre: Vec::new(),
        album: String::new(),
        art_url: None,
        length: None,
        position: None,
        rate: None,
        calls: Vec::new(),
        ignored_get_alls: 0,
    }
}

fn playing(identity: &str, status: Status, title: &str, artist: &str) -> Option<NowPlaying> {
    Some(NowPlaying {
        title: title.to_string(),
        artist: artist.to_string(),
        identity: identity.to_string(),
        status,
        track_id: Some("/org/fake/track/1".to_string()),
        // The fake player lives in this process, on the bus the client reads.
        pid: Some(std::process::id()),
        ..NowPlaying::default()
    })
}

/// Wait until the client follows `want`, reading its changes as they come.
async fn until(mpris: &Mpris, changes: &mut Changes, want: &Option<NowPlaying>) {
    let deadline = Instant::now() + WAIT;
    while mpris.now_playing() != *want {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, changes.next()).await {
            Ok(Some(_)) => {}
            Ok(None) => panic!(
                "the changes ended at {:?}, not {want:?}",
                mpris.now_playing()
            ),
            Err(_) => panic!("still {:?}, not {want:?}", mpris.now_playing()),
        }
    }
}

/// Wait until what the client follows satisfies `want`, reading its changes as they come.
async fn until_matching(
    mpris: &Mpris,
    changes: &mut Changes,
    what: &str,
    want: impl Fn(&NowPlaying) -> bool,
) -> NowPlaying {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(now) = mpris.now_playing().filter(|now| want(now)) {
            return now;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, changes.next()).await {
            Ok(Some(_)) => {}
            Ok(None) => panic!("the changes ended before {what}"),
            Err(_) => panic!("still {:?}, not {what}", mpris.now_playing()),
        }
    }
}

/// Milliseconds since the Unix epoch now.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// A client on `bus`, and its changes from before anything is listed.
async fn client(bus: &PrivateBus) -> (Mpris, Changes) {
    let mpris = Mpris::at(&bus.address)
        .await
        .expect("the client reaches the private bus");
    let changes = mpris.changes();
    (mpris, changes)
}

#[tokio::test]
async fn a_bus_without_a_player_follows_none_and_presses_reach_nothing() {
    let Some(bus) = PrivateBus::start("mpris-empty") else {
        return;
    };
    // Something on the bus that answers as a player does, under a name that is not one.
    let stranger = FakePlayer::start(
        &bus,
        "org.example.NotAPlayer",
        state("Stranger", "Playing", "Not", "Followed"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    // The client lists the bus before it reads any signal, so once a player that joins after
    // it is followed, the listing is done; once that player leaves, what is left is what the
    // listing found.
    let visitor = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.visitor",
        state("Visitor", "Paused", "One", "V"),
    )
    .await;
    until(
        &mpris,
        &mut changes,
        &playing("Visitor", Status::Paused, "One", "V"),
    )
    .await;
    drop(visitor);
    until(&mpris, &mut changes, &None).await;
    assert!(!mpris.press(Button::Play).await);
    assert!(stranger.calls().is_empty(), "{:?}", stranger.calls());
}

#[test]
fn a_bus_that_never_answers_is_given_up_on_without_holding_anything_up() {
    // A socket that takes connections and never says a word, as a stuck bus does.
    let dir = TempDir::new("mpris-silent");
    let socket = dir.join("bus");
    let silent = std::os::unix::net::UnixListener::bind(&socket).expect("the silent bus binds");
    let live = PrivateBus::start("mpris-silent-live");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let silent_address = format!("unix:path={}", socket.display());
    let started = Instant::now();
    let (stuck, beside) = runtime.block_on(async {
        let beside = async {
            let bus = live.as_ref()?;
            // Once the connect to the silent bus is surely waiting on it, another connection
            // registers the way libdbus's own bus connections do, BlueZ's among them, and is
            // not held up by it.
            tokio::time::sleep(Duration::from_millis(200)).await;
            let address = bus.address.clone();
            let (sending, registered) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let reached = Channel::open_private(&address)
                    .and_then(|mut channel| channel.register())
                    .is_ok();
                let _ = sending.send(reached);
            });
            let reached = tokio::time::timeout(Duration::from_secs(1), registered).await;
            Some(matches!(reached, Ok(Ok(true))))
        };
        tokio::join!(Mpris::at(&silent_address), beside)
    });
    assert!(stuck.is_none());
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the connect waited {:?}",
        started.elapsed()
    );
    assert_ne!(
        beside,
        Some(false),
        "a connection registering on a live bus was held up by the silent one"
    );
    // A connect still waiting on the bus holds nothing the runtime waits for when it ends.
    let ending = Instant::now();
    drop(runtime);
    assert!(
        ending.elapsed() < Duration::from_secs(1),
        "the runtime took {:?} to end",
        ending.elapsed()
    );
    drop(silent);
}

#[tokio::test]
async fn no_bus_is_no_players() {
    let dir = TempDir::new("mpris-no-bus");
    let address = format!("unix:path={}", dir.join("no-bus").display());
    let started = Instant::now();
    assert!(Mpris::at(&address).await.is_none());
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn a_player_already_playing_is_followed_with_what_it_plays() {
    let Some(bus) = PrivateBus::start("mpris-already") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        state("Fake Player", "Playing", "So What", "Miles Davis"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    let want = playing("Fake Player", Status::Playing, "So What", "Miles Davis");
    until(&mpris, &mut changes, &want).await;
}

#[tokio::test]
async fn a_players_genre_array_is_read_into_now_playing() {
    let Some(bus) = PrivateBus::start("mpris-genre") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        State {
            genre: vec!["Jazz".to_string()],
            ..state("Fake Player", "Playing", "So What", "Miles Davis")
        },
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    let want = Some(NowPlaying {
        genre: vec!["Jazz".to_string()],
        ..playing("Fake Player", Status::Playing, "So What", "Miles Davis").unwrap()
    });
    until(&mpris, &mut changes, &want).await;
    assert_eq!(mpris.now_playing().unwrap().genre, ["Jazz"]);
}

#[tokio::test]
async fn a_player_that_joins_is_followed_and_one_that_leaves_is_let_go() {
    let Some(bus) = PrivateBus::start("mpris-joins") else {
        return;
    };
    let (mpris, mut changes) = client(&bus).await;
    let player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        state("Fake Player", "Paused", "Blue in Green", "Miles Davis"),
    )
    .await;
    let want = playing(
        "Fake Player",
        Status::Paused,
        "Blue in Green",
        "Miles Davis",
    );
    until(&mpris, &mut changes, &want).await;
    drop(player);
    until(&mpris, &mut changes, &None).await;
    assert!(!mpris.press(Button::Play).await);
}

#[tokio::test]
async fn a_new_track_and_a_pause_update_what_is_playing() {
    let Some(bus) = PrivateBus::start("mpris-track") else {
        return;
    };
    let player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        state("Fake Player", "Playing", "So What", "Miles Davis"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until(
        &mpris,
        &mut changes,
        &playing("Fake Player", Status::Playing, "So What", "Miles Davis"),
    )
    .await;
    player.set_track("Freddie Freeloader", "Miles Davis");
    until(
        &mpris,
        &mut changes,
        &playing(
            "Fake Player",
            Status::Playing,
            "Freddie Freeloader",
            "Miles Davis",
        ),
    )
    .await;
    player.set_status("Paused");
    until(
        &mpris,
        &mut changes,
        &playing(
            "Fake Player",
            Status::Paused,
            "Freddie Freeloader",
            "Miles Davis",
        ),
    )
    .await;
}

#[tokio::test]
async fn the_playing_player_is_followed_then_the_one_that_played_last() {
    let Some(bus) = PrivateBus::start("mpris-choice") else {
        return;
    };
    let alpha = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.alpha",
        state("Alpha", "Paused", "One", "A"),
    )
    .await;
    let beta = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.beta",
        state("Beta", "Playing", "Two", "B"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    // Beta plays and Alpha is paused: Beta, though Alpha is first by name.
    until(
        &mpris,
        &mut changes,
        &playing("Beta", Status::Playing, "Two", "B"),
    )
    .await;
    // Alpha starts too, after Beta: the one that started last.
    alpha.set_status("Playing");
    until(
        &mpris,
        &mut changes,
        &playing("Alpha", Status::Playing, "One", "A"),
    )
    .await;
    // Alpha pauses while Beta still plays: Beta.
    alpha.set_status("Paused");
    until(
        &mpris,
        &mut changes,
        &playing("Beta", Status::Playing, "Two", "B"),
    )
    .await;
    // Beta stops too: none plays, and Beta is the one that played last, though Alpha started
    // after it.
    beta.set_status("Stopped");
    until(
        &mpris,
        &mut changes,
        &playing("Beta", Status::Stopped, "Two", "B"),
    )
    .await;
    // Alpha plays again and pauses: now Alpha played last.
    alpha.set_status("Playing");
    alpha.set_status("Paused");
    until(
        &mpris,
        &mut changes,
        &playing("Alpha", Status::Paused, "One", "A"),
    )
    .await;
}

#[tokio::test]
async fn a_player_too_busy_to_answer_is_asked_again_on_its_next_signal() {
    let Some(bus) = PrivateBus::start("mpris-busy") else {
        return;
    };
    let (mpris, mut changes) = client(&bus).await;
    let mut busy = state("Busy", "Paused", "Slow", "B");
    busy.ignored_get_alls = 1;
    let player = FakePlayer::start(&bus, "org.mpris.MediaPlayer2.busy", busy).await;
    // Its first answer never comes, so nothing is known of it yet: nothing is followed.
    tokio::time::sleep(svolder_host::players::CALL_TIMEOUT + Duration::from_millis(200)).await;
    assert_eq!(mpris.now_playing(), None);
    // Its next signal has it asked again, and this time it answers.
    player.set_status("Playing");
    until(
        &mpris,
        &mut changes,
        &playing("Busy", Status::Playing, "Slow", "B"),
    )
    .await;
}

#[tokio::test]
async fn a_press_reaches_the_followed_player_and_no_other() {
    let Some(bus) = PrivateBus::start("mpris-press") else {
        return;
    };
    let alpha = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.alpha",
        state("Alpha", "Paused", "One", "A"),
    )
    .await;
    let beta = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.beta",
        state("Beta", "Playing", "Two", "B"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until(
        &mpris,
        &mut changes,
        &playing("Beta", Status::Playing, "Two", "B"),
    )
    .await;
    assert!(alpha.calls().is_empty() && beta.calls().is_empty());
    for button in Button::ALL {
        assert!(mpris.press(button).await, "{button:?}");
    }
    assert!(mpris.next().await);
    assert_eq!(beta.calls(), ["Previous", "Play", "Pause", "Next", "Next"]);
    assert!(alpha.calls().is_empty(), "{:?}", alpha.calls());
}

#[tokio::test]
async fn a_status_only_named_as_changed_is_asked_for_again() {
    let Some(bus) = PrivateBus::start("mpris-invalidated") else {
        return;
    };
    let alpha = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.alpha",
        state("Alpha", "Paused", "One", "A"),
    )
    .await;
    let _beta = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.beta",
        state("Beta", "Playing", "Two", "B"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until(
        &mpris,
        &mut changes,
        &playing("Beta", Status::Playing, "Two", "B"),
    )
    .await;
    // Alpha starts after Beta: once asked, it is the one that started last.
    alpha.invalidate_status("Playing");
    until(
        &mpris,
        &mut changes,
        &playing("Alpha", Status::Playing, "One", "A"),
    )
    .await;
}

#[tokio::test]
async fn a_player_that_misses_a_re_ask_keeps_its_answer_and_its_place() {
    let Some(bus) = PrivateBus::start("mpris-missed") else {
        return;
    };
    let _alpha = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.alpha",
        state("Alpha", "Paused", "One", "A"),
    )
    .await;
    let beta = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.beta",
        state("Beta", "Playing", "Two", "B"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until(
        &mpris,
        &mut changes,
        &playing("Beta", Status::Playing, "Two", "B"),
    )
    .await;
    // Beta stops: it played last, so it is still the one followed.
    beta.set_status("Paused");
    until(
        &mpris,
        &mut changes,
        &playing("Beta", Status::Paused, "Two", "B"),
    )
    .await;
    // Beta names its status as changed and then misses the question that follows: it keeps
    // what it said and its place.
    beta.ignore_next_get_all();
    beta.invalidate_status("Paused");
    tokio::time::sleep(svolder_host::players::CALL_TIMEOUT + Duration::from_millis(300)).await;
    assert_eq!(
        mpris.now_playing(),
        playing("Beta", Status::Paused, "Two", "B"),
        "a missed answer moved what is followed"
    );
    // Its next signal, whatever it says, has it asked again: what it plays now comes along.
    beta.state.lock().unwrap().status = "Playing";
    beta.set_track("Three", "B");
    until(
        &mpris,
        &mut changes,
        &playing("Beta", Status::Playing, "Three", "B"),
    )
    .await;
}

#[tokio::test]
async fn a_player_without_an_identity_goes_by_its_bus_name() {
    let Some(bus) = PrivateBus::start("mpris-identity") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fakeplayer.instance_1_2",
        state("", "Playing", "So What", "Miles Davis"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until(
        &mpris,
        &mut changes,
        &playing("fakeplayer", Status::Playing, "So What", "Miles Davis"),
    )
    .await;
}

#[tokio::test]
async fn a_bus_that_goes_away_lets_the_player_go_and_ends_the_changes() {
    let Some(mut bus) = PrivateBus::start("mpris-lost") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        state("Fake Player", "Playing", "So What", "Miles Davis"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until(
        &mpris,
        &mut changes,
        &playing("Fake Player", Status::Playing, "So What", "Miles Davis"),
    )
    .await;
    bus.end();
    let ended = tokio::time::timeout(WAIT, async { while changes.next().await.is_some() {} }).await;
    assert!(ended.is_ok(), "the changes did not end");
    assert_eq!(mpris.now_playing(), None);
    assert!(!mpris.press(Button::Play).await);
}

#[tokio::test]
async fn the_albums_cover_address_and_length_are_read_with_the_track() {
    let Some(bus) = PrivateBus::start("mpris-album") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        State {
            album: "Kind of Blue".to_string(),
            art_url: Some("file:///tmp/cover.jpg".to_string()),
            length: Some(562_000_000),
            ..state("Fake Player", "Playing", "So What", "Miles Davis")
        },
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    let want = Some(NowPlaying {
        album: "Kind of Blue".to_string(),
        art_url: Some("file:///tmp/cover.jpg".to_string()),
        length_us: Some(562_000_000),
        ..playing("Fake Player", Status::Playing, "So What", "Miles Davis").unwrap()
    });
    until(&mpris, &mut changes, &want).await;
}

#[tokio::test]
async fn position_and_rate_are_read_on_follow() {
    let Some(bus) = PrivateBus::start("mpris-progress") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        State {
            position: Some(30_000_000),
            rate: Some(1.5),
            ..state("Fake Player", "Playing", "So What", "Miles Davis")
        },
    )
    .await;
    let before = now_ms();
    let (mpris, mut changes) = client(&bus).await;
    let now = until_matching(&mpris, &mut changes, "a progress", |n| n.progress.is_some()).await;
    let progress = now.progress.unwrap();
    assert_eq!(
        (progress.position_us, progress.rate_milli),
        (30_000_000, 1500)
    );
    assert!(
        (before..=now_ms()).contains(&progress.read_at_ms),
        "read at {} outside {before}..",
        progress.read_at_ms
    );
}

#[tokio::test]
async fn a_player_that_does_not_answer_the_position_has_no_progress() {
    let Some(bus) = PrivateBus::start("mpris-no-progress") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        state("Fake Player", "Playing", "So What", "Miles Davis"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    let want = playing("Fake Player", Status::Playing, "So What", "Miles Davis");
    until(&mpris, &mut changes, &want).await;
    assert_eq!(mpris.now_playing().unwrap().progress, None);
}

#[tokio::test]
async fn a_player_without_a_rate_plays_at_the_normal_one() {
    let Some(bus) = PrivateBus::start("mpris-no-rate") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        State {
            position: Some(5_000_000),
            ..state("Fake Player", "Playing", "So What", "Miles Davis")
        },
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    let now = until_matching(&mpris, &mut changes, "a progress", |n| n.progress.is_some()).await;
    let progress = now.progress.unwrap();
    assert_eq!(
        (progress.position_us, progress.rate_milli),
        (5_000_000, 1000)
    );
}

#[tokio::test]
async fn a_seeked_signal_rereads_the_position() {
    let Some(bus) = PrivateBus::start("mpris-seeked") else {
        return;
    };
    let player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        State {
            position: Some(30_000_000),
            rate: Some(1.0),
            ..state("Fake Player", "Playing", "So What", "Miles Davis")
        },
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until_matching(&mpris, &mut changes, "a progress", |n| n.progress.is_some()).await;
    player.seek(120_000_000);
    let now = until_matching(&mpris, &mut changes, "the seeked position", |n| {
        n.progress.is_some_and(|p| p.position_us == 120_000_000)
    })
    .await;
    assert_eq!(now.progress.unwrap().rate_milli, 1000);
}

#[tokio::test]
async fn a_rate_change_rereads_the_progress() {
    let Some(bus) = PrivateBus::start("mpris-rate") else {
        return;
    };
    let player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        State {
            position: Some(30_000_000),
            rate: Some(1.0),
            ..state("Fake Player", "Playing", "So What", "Miles Davis")
        },
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until_matching(&mpris, &mut changes, "a progress", |n| n.progress.is_some()).await;
    player.set_rate(2.0, 31_000_000);
    let now = until_matching(&mpris, &mut changes, "the new rate", |n| {
        n.progress.is_some_and(|p| p.rate_milli == 2000)
    })
    .await;
    assert_eq!(now.progress.unwrap().position_us, 31_000_000);
}

#[tokio::test]
async fn the_followed_players_pid_is_known() {
    let Some(bus) = PrivateBus::start("mpris-pid") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        state("Fake Player", "Playing", "So What", "Miles Davis"),
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    let now = until_matching(&mpris, &mut changes, "a pid", |n| n.pid.is_some()).await;
    assert_eq!(now.pid, Some(std::process::id()));
}

#[tokio::test]
async fn a_track_change_resets_progress() {
    let Some(bus) = PrivateBus::start("mpris-reset") else {
        return;
    };
    let player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        State {
            position: Some(30_000_000),
            rate: Some(1.0),
            ..state("Fake Player", "Playing", "So What", "Miles Davis")
        },
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until_matching(&mpris, &mut changes, "a progress", |n| n.progress.is_some()).await;
    player.set_track("Freddie Freeloader", "Miles Davis");
    let now = until_matching(&mpris, &mut changes, "the new track at its start", |n| {
        n.title == "Freddie Freeloader" && n.progress.is_some_and(|p| p.position_us == 0)
    })
    .await;
    assert_eq!(now.progress.unwrap().rate_milli, 1000);
}

#[tokio::test]
async fn a_pause_rereads_the_position_where_it_stopped() {
    let Some(bus) = PrivateBus::start("mpris-pause-position") else {
        return;
    };
    let player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        State {
            position: Some(30_000_000),
            rate: Some(1.0),
            ..state("Fake Player", "Playing", "So What", "Miles Davis")
        },
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until_matching(&mpris, &mut changes, "a progress", |n| n.progress.is_some()).await;
    player.state.lock().unwrap().position = Some(41_000_000);
    player.set_status("Paused");
    until_matching(&mpris, &mut changes, "the paused position", |n| {
        n.status == Status::Paused && n.progress.is_some_and(|p| p.position_us == 41_000_000)
    })
    .await;
}

/// A player that moves `Position` a moment after it publishes a new track is read again once
/// the moment has passed, so the bar is not left at the last track's position.
#[tokio::test]
async fn a_position_that_moves_just_after_the_new_track_is_read_again() {
    let Some(bus) = PrivateBus::start("mpris-late-position") else {
        return;
    };
    let player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        State {
            position: Some(30_000_000),
            rate: Some(1.0),
            ..state("Fake Player", "Playing", "So What", "Miles Davis")
        },
    )
    .await;
    let (mpris, mut changes) = client(&bus).await;
    until_matching(&mpris, &mut changes, "a progress", |n| n.progress.is_some()).await;
    player.set_track_then_position(
        "Freddie Freeloader",
        "Miles Davis",
        1_000,
        Duration::from_millis(150),
    );
    let now = until_matching(&mpris, &mut changes, "the new track's own position", |n| {
        n.title == "Freddie Freeloader" && n.progress.is_some_and(|p| p.position_us == 1_000)
    })
    .await;
    assert_eq!(now.progress.unwrap().rate_milli, 1000);
}

/// A process id the bus did not give when the player joined is asked for again two seconds
/// later, and once more on a later signal if that did not give it either.
#[tokio::test]
async fn a_failed_pid_lookup_is_asked_again() {
    let Some(bus) = PrivateBus::start("mpris-pid-retry") else {
        return;
    };
    let _player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        state("Fake Player", "Playing", "So What", "Miles Davis"),
    )
    .await;
    let asked = Arc::new(Mutex::new(0usize));
    let counted = asked.clone();
    let pids: svolder_host::players::PidLookup = Arc::new(move |_owner| {
        let tries = {
            let mut asked = counted.lock().unwrap();
            *asked += 1;
            *asked
        };
        Box::pin(async move { (tries >= 2).then_some(4242) })
    });
    let started = Instant::now();
    let mpris = Mpris::at_with_pids(&bus.address, Some(pids))
        .await
        .expect("the client reaches the private bus");
    let mut changes = mpris.changes();
    let now = until_matching(&mpris, &mut changes, "a pid", |n| n.pid.is_some()).await;
    assert_eq!(now.pid, Some(4242));
    assert!(
        started.elapsed() >= svolder_host::players::PID_RETRY - Duration::from_millis(200),
        "the retry came after {:?}",
        started.elapsed()
    );
    assert_eq!(*asked.lock().unwrap(), 2, "asked again after it was given");
}

/// A player that did not give a process id is asked once more on its next signal, without
/// waiting for the timed retry.
#[tokio::test]
async fn a_failed_pid_lookup_is_asked_again_on_the_next_signal() {
    let Some(bus) = PrivateBus::start("mpris-pid-signal") else {
        return;
    };
    let player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        state("Fake Player", "Playing", "So What", "Miles Davis"),
    )
    .await;
    let asked = Arc::new(Mutex::new(0usize));
    let counted = asked.clone();
    let pids: svolder_host::players::PidLookup = Arc::new(move |_owner| {
        let tries = {
            let mut asked = counted.lock().unwrap();
            *asked += 1;
            *asked
        };
        Box::pin(async move { (tries >= 2).then_some(4242) })
    });
    let mpris = Mpris::at_with_pids(&bus.address, Some(pids))
        .await
        .expect("the client reaches the private bus");
    let mut changes = mpris.changes();
    until_matching(&mpris, &mut changes, "followed", |n| n.title == "So What").await;
    assert_eq!(mpris.now_playing().unwrap().pid, None);
    let before = Instant::now();
    player.set_status("Paused");
    let now = until_matching(&mpris, &mut changes, "a pid", |n| n.pid.is_some()).await;
    assert_eq!(now.pid, Some(4242));
    assert!(
        before.elapsed() < Duration::from_millis(1_500),
        "waited for the timer"
    );
}

/// A bus that took a connection and never answered is not connected to again while that connect
/// still waits on it: a second look comes back at once, and the bus sees one connection, not one
/// more thread blocked in libdbus per look.
#[tokio::test]
async fn a_look_at_a_bus_that_never_answered_does_not_add_a_blocked_connect() {
    let dir = TempDir::new("mpris-silent-twice");
    let socket = dir.join("bus");
    let silent = std::os::unix::net::UnixListener::bind(&socket).expect("the silent bus binds");
    let accepted = Arc::new(Mutex::new(Vec::new()));
    let keeping = accepted.clone();
    std::thread::spawn(move || {
        while let Ok((stream, _)) = silent.accept() {
            keeping.lock().unwrap().push(stream);
        }
    });
    let address = format!("unix:path={}", socket.display());
    assert!(Mpris::at(&address).await.is_none());
    let again = Instant::now();
    assert!(Mpris::at(&address).await.is_none());
    assert!(
        again.elapsed() < Duration::from_millis(500),
        "the second look waited {:?}",
        again.elapsed()
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        accepted.lock().unwrap().len(),
        1,
        "a second connect was made"
    );
}

/// A look says why it found no bus: nothing there is `Absent`, a bus that took the connection and
/// never answered is `Silent` (on the look that waited on it and on the look that was refused
/// because of it), which a caller looks at again.
#[tokio::test]
async fn a_look_says_whether_the_bus_is_absent_or_silent() {
    let missing = Mpris::look("unix:path=/nonexistent/svolder-test-bus").await;
    assert_eq!(missing.err(), Some(NoBus::Absent), "no socket");

    let dir = TempDir::new("mpris-silent-says");
    let socket = dir.join("bus");
    let silent = std::os::unix::net::UnixListener::bind(&socket).expect("the silent bus binds");
    let accepted = Arc::new(Mutex::new(Vec::new()));
    let keeping = accepted.clone();
    std::thread::spawn(move || {
        while let Ok((stream, _)) = silent.accept() {
            keeping.lock().unwrap().push(stream);
        }
    });
    let address = format!("unix:path={}", socket.display());
    assert_eq!(
        Mpris::look(&address).await.err(),
        Some(NoBus::Silent),
        "the look that waited"
    );
    let again = Instant::now();
    assert_eq!(
        Mpris::look(&address).await.err(),
        Some(NoBus::Silent),
        "the look refused while the first still waits"
    );
    assert!(again.elapsed() < Duration::from_millis(500));
}

/// A player the bus never gives a process id is asked for three times in all (when it joined, two
/// seconds on, and on its next signal) and no more, however many signals follow.
#[tokio::test]
async fn a_pid_the_bus_never_gives_is_asked_for_three_times_only() {
    let Some(bus) = PrivateBus::start("mpris-pid-cap") else {
        return;
    };
    let player = FakePlayer::start(
        &bus,
        "org.mpris.MediaPlayer2.fake",
        state("Fake Player", "Playing", "So What", "Miles Davis"),
    )
    .await;
    let asked = Arc::new(Mutex::new(0usize));
    let counted = asked.clone();
    let pids: svolder_host::players::PidLookup = Arc::new(move |_owner| {
        *counted.lock().unwrap() += 1;
        Box::pin(async { None })
    });
    let mpris = Mpris::at_with_pids(&bus.address, Some(pids))
        .await
        .expect("the client reaches the private bus");
    let mut changes = mpris.changes();
    until_matching(&mpris, &mut changes, "followed", |n| n.title == "So What").await;
    tokio::time::sleep(svolder_host::players::PID_RETRY + Duration::from_millis(500)).await;
    assert_eq!(*asked.lock().unwrap(), 2, "the timed retry did not come");
    for status in ["Paused", "Playing", "Paused", "Playing"] {
        player.set_status(status);
        until_matching(&mpris, &mut changes, "the status", |n| {
            n.status
                == if status == "Paused" {
                    Status::Paused
                } else {
                    Status::Playing
                }
        })
        .await;
    }
    assert_eq!(
        *asked.lock().unwrap(),
        3,
        "asked more often than the cap allows"
    );
}
