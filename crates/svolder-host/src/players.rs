// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The host's media players, over MPRIS on the session bus: which one is followed, what it
//! plays, and the four transport buttons pressed on it.
//!
//! Every `org.mpris.MediaPlayer2.*` name on the bus is a player. The one followed is whichever
//! is `Playing` (the one that started last, if several), else whichever played last (stopped
//! last), else the first by name. The choice is remade on `NameOwnerChanged` (a player joins or
//! leaves) and on `PropertiesChanged` naming a status change, never by polling; other changes
//! (a new track) just update what the followed player plays.
//!
//! Besides the track, the followed player's playback position and rate are read when it is
//! followed and again whenever something that moves them changes (a status change, a new track,
//! a rate change, or `Seeked`); a reader works the position out from the read stamp. A player
//! that updates `Position` a moment after it publishes a new track is read once more
//! [`POSITION_REREAD`] after the track changed. The player's process id is asked of the bus when
//! the player joins, and twice more, [`PID_RETRY`] later and on its next signal, while the bus
//! has not given it.
//!
//! A player that does not answer when asked what it plays is kept by name, with whatever it
//! said before, and asked again on its next signal — one slow answer never loses it or its
//! place.
//!
//! Nothing is ever sent to a player except a press a caller asks for: reading goes through
//! `Properties.GetAll` and `Get`, and the bus's own signals and calls — nothing else.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use dbus::Message;
use dbus::arg::{ArgType, PropMap, RefArg};
use dbus::channel::{Channel, MatchingReceiver};
use dbus::message::MatchRule;
use dbus::nonblock::stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged};
use dbus::nonblock::{Proxy, SyncConnection};
use dbus::strings::BusName;
use futures::future::BoxFuture;
use futures::stream::{BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

/// Every MPRIS player's bus name starts with this.
pub const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";

/// Where every MPRIS player keeps its object.
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";

/// The interface a player's identity is on.
const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";

/// The interface a player's status, track and transport are on.
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

/// The namespace every player's bus name is in, as a match rule's `arg0namespace` takes it.
const MPRIS_NAMESPACE: &str = "org.mpris.MediaPlayer2";

/// The bus itself, which lists the names and says when one changes hands.
const BUS_NAME: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";

/// The standard interface every property change is signalled on.
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

/// How long opening the bus, registering on it and asking for its signals may take together.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long one call to the bus or to a player gets.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(1);

/// How long after a player joined without a process id the bus is asked for it again.
pub const PID_RETRY: Duration = Duration::from_secs(2);

/// How many times, after the first, a player's process id is asked for while the bus has not
/// given it.
const PID_RETRIES: u8 = 2;

/// How long after a new track its player's `Position` is read once more: a player that updates
/// it a moment after the metadata is read between the two otherwise.
pub const POSITION_REREAD: Duration = Duration::from_millis(500);

/// What a process id lookup answers for the connection named: the bus's own, unless a case
/// puts another in its place ([`Mpris::at_with_pids`]).
pub type PidLookup = Arc<dyn Fn(String) -> BoxFuture<'static, Option<u32>> + Send + Sync>;

/// Why a look found no bus to follow, which decides whether looking again is worth it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoBus {
    /// There is none to reach: no session bus, or it refused the connection. Looking again finds
    /// one only if a bus has since started, which a caller that never saw one does not wait for.
    Absent,
    /// A bus took the connection and has not answered yet, within [`CONNECT_TIMEOUT`] or while an
    /// earlier look at it is still waiting on it. It may answer later, so a caller looks again,
    /// and counts it as a bus seen: a [`NoBus::Absent`] look after it is looked again too.
    Silent,
}

/// What a player says it is doing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Status {
    /// Playing a track.
    Playing,
    /// Holding a track where it stopped.
    Paused,
    /// Not playing anything.
    #[default]
    Stopped,
}

impl Status {
    /// The status an MPRIS `PlaybackStatus` names, or `None` for anything else.
    fn parse(text: &str) -> Option<Self> {
        match text {
            "Playing" => Some(Self::Playing),
            "Paused" => Some(Self::Paused),
            "Stopped" => Some(Self::Stopped),
            _ => None,
        }
    }
}

/// What the followed player plays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NowPlaying {
    /// The track's title (`xesam:title`); empty when the player gave none.
    pub title: String,
    /// The track's artists (`xesam:artist`), joined by `, `; empty when the player gave none.
    pub artist: String,
    /// The track's genres (`xesam:genre`); empty when the player gave none.
    pub genre: Vec<String>,
    /// What the player calls itself (`Identity`), else the part of its bus name after
    /// [`MPRIS_PREFIX`] up to the next dot.
    pub identity: String,
    /// What the player is doing.
    pub status: Status,
    /// The track's album (`xesam:album`); empty when the player gave none.
    pub album: String,
    /// The track's cover as the player published it (`mpris:artUrl`), an address of any kind.
    pub art_url: Option<String>,
    /// The track's identifier (`mpris:trackid`, an object path as text), which names the track
    /// across the player's signals.
    pub track_id: Option<String>,
    /// The track's length in microseconds (`mpris:length`).
    pub length_us: Option<u64>,
    /// Where the player is in the track; `None` when it does not answer `Position`.
    pub progress: Option<Progress>,
    /// The process id of the followed player's bus connection, as the bus says it.
    pub pid: Option<u32>,
}

/// Where a player was in its track when it was last asked, and how fast it plays on.
///
/// The position now is `position_us` plus the time since `read_at_ms` times the rate, while the
/// player is playing; a paused or stopped player stays at `position_us`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// `Position` in microseconds when it was read.
    pub position_us: u64,
    /// `Rate` in thousandths (1000 is the normal rate); 1000 when the player gave none.
    pub rate_milli: u32,
    /// When `Position` was read, in milliseconds since the Unix epoch.
    pub read_at_ms: u64,
}

/// The clock, in milliseconds since the Unix epoch: the stamp of a [`Progress`] read.
pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// One of the four transport buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// Back to the previous track.
    Previous,
    /// Start or resume playing.
    Play,
    /// Pause where it is.
    Pause,
    /// On to the next track.
    Next,
}

impl Button {
    /// The four buttons in the order a transport row draws them.
    pub const ALL: [Button; 4] = [Button::Previous, Button::Play, Button::Pause, Button::Next];

    /// The MPRIS method the button calls on the player.
    pub fn method(self) -> &'static str {
        match self {
            Button::Previous => "Previous",
            Button::Play => "Play",
            Button::Pause => "Pause",
            Button::Next => "Next",
        }
    }
}

/// What the followed player plays, each time that changes: `None` when no player is followed
/// any more. The stream ends when the players can no longer be followed (the bus went away).
pub type Changes = BoxStream<'static, Option<NowPlaying>>;

/// The host's media players, one of them followed.
#[async_trait]
pub trait Players: Send + Sync {
    /// What the followed player plays, or `None` when there is no player.
    fn now_playing(&self) -> Option<NowPlaying>;

    /// Every change of [`Players::now_playing`] from now on.
    fn changes(&self) -> Changes;

    /// Press `button` on the followed player. Whether a player was there to take it: with
    /// none, nothing is sent.
    async fn press(&self, button: Button) -> bool;

    /// Press [`Button::Previous`].
    async fn previous(&self) -> bool {
        self.press(Button::Previous).await
    }

    /// Press [`Button::Play`].
    async fn play(&self) -> bool {
        self.press(Button::Play).await
    }

    /// Press [`Button::Pause`].
    async fn pause(&self) -> bool {
        self.press(Button::Pause).await
    }

    /// Press [`Button::Next`].
    async fn next(&self) -> bool {
        self.press(Button::Next).await
    }
}

/// The session bus's MPRIS players.
///
/// It keeps a connection of its own and a task following the players on it, both ended when
/// it is dropped. Made inside a tokio runtime, which runs them.
pub struct Mpris {
    /// The connection presses go out on.
    connection: Arc<SyncConnection>,
    /// What the followed player plays, as the following task last published it.
    now: watch::Receiver<Option<NowPlaying>>,
    /// The followed player's unique bus name, which presses are sent to.
    target: Arc<Mutex<Option<String>>>,
    /// The task following the players and the one driving the connection, both ended with
    /// this.
    _tasks: [Aborting; 2],
}

impl Mpris {
    /// The players on this user's session bus: the one `DBUS_SESSION_BUS_ADDRESS` names, else
    /// `$XDG_RUNTIME_DIR/bus` when it is there. `None` when there is no session bus, it does not
    /// answer, or [`host_off`](crate::host_off) says host audio is off for this process.
    ///
    /// No bus is ever started to answer: a session without one simply has no players.
    pub async fn session() -> Option<Self> {
        Self::look_session().await.ok()
    }

    /// [`Mpris::session`], saying why when there is no bus to follow: a bus that is not there
    /// ([`NoBus::Absent`], also when host audio is off) or one that has not answered
    /// ([`NoBus::Silent`]).
    pub async fn look_session() -> Result<Self, NoBus> {
        if crate::host_off() {
            return Err(NoBus::Absent);
        }
        let address = session_address(
            std::env::var_os("DBUS_SESSION_BUS_ADDRESS").as_deref(),
            std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
        )
        .ok_or(NoBus::Absent)?;
        Self::look(&address).await
    }

    /// The players on the bus at `address`, a D-Bus address such as
    /// `unix:path=/run/user/1000/bus`. `None` when that bus cannot be reached within
    /// [`CONNECT_TIMEOUT`], and, while an earlier look at the same address is still blocked in
    /// libdbus (a bus that took the connection and never answered), at once.
    ///
    /// It returns once it is connected and listening; the players already on the bus are asked
    /// what they play right after, and what they say arrives as a change.
    pub async fn at(address: &str) -> Option<Self> {
        Self::look(address).await.ok()
    }

    /// [`Mpris::at`], saying why when there is no bus to follow.
    pub async fn look(address: &str) -> Result<Self, NoBus> {
        Self::look_with_pids(address, None).await
    }

    /// [`Mpris::at`], the players' process ids asked of `pids` rather than of the bus: for a
    /// case that has to make the lookup fail.
    #[doc(hidden)]
    pub async fn at_with_pids(address: &str, pids: Option<PidLookup>) -> Option<Self> {
        Self::look_with_pids(address, pids).await.ok()
    }

    async fn look_with_pids(address: &str, pids: Option<PidLookup>) -> Result<Self, NoBus> {
        tokio::time::timeout(CONNECT_TIMEOUT, Self::connect(address.to_string(), pids))
            .await
            .map_err(|_| NoBus::Silent)?
    }

    async fn connect(address: String, pids: Option<PidLookup>) -> Result<Self, NoBus> {
        // A bus that never answers leaves the thread below blocked in libdbus; a look at the same
        // bus while it still is would add one more, so it is refused until that one ends.
        let blocked = Blocked::take(&address).ok_or(NoBus::Silent)?;
        // Opening the connection and saying `Hello` block in libdbus, and on a bus that never
        // answers they block for good (libdbus's own timeout does not start until the bus has
        // taken the connection). They run on a thread of their own that hands the channel
        // back, not on the runtime's blocking pool that a shutting-down runtime waits for, so
        // a connect given up on holds nothing the runtime waits on; the thread drops the
        // channel, closing it, once libdbus lets go.
        //
        // `Hello` goes out as an ordinary call rather than through libdbus's own registering,
        // which holds a process-wide lock while it waits — a bus that never answers would
        // otherwise stall every other D-Bus connection this process opens (BlueZ's among them).
        let (handing, handed) = oneshot::channel();
        std::thread::Builder::new()
            .name("svolder-mpris-connect".to_string())
            .spawn(move || {
                let _blocked = blocked;
                let opened = match Channel::open_private(&address) {
                    Err(_) => Err(NoBus::Absent),
                    Ok(channel) => {
                        let hello = Message::new_method_call(BUS_NAME, BUS_PATH, BUS_NAME, "Hello")
                            .map_err(|_| NoBus::Absent);
                        // A bus that took the connection and did not say hello back may still
                        // be starting up.
                        hello.and_then(|hello| {
                            channel
                                .send_with_reply_and_block(hello, CALL_TIMEOUT)
                                .map(|_| channel)
                                .map_err(|_| NoBus::Silent)
                        })
                    }
                };
                let _ = handing.send(opened);
            })
            .map_err(|_| NoBus::Absent)?;
        let channel = handed.await.map_err(|_| NoBus::Absent)??;
        // libdbus is never told this connection registered, so it holds no unique name for
        // it: `SyncConnection::unique_name()` must not be called on it, as it unwraps that
        // name and would panic.
        let (resource, connection) =
            dbus_tokio::connection::from_channel::<SyncConnection>(channel)
                .map_err(|_| NoBus::Absent)?;
        let (heard, hearing) = mpsc::unbounded_channel();
        let lost = heard.clone();
        // Held by a guard from here on, so a connect given up on, or refused below, ends the
        // connection rather than leaving it driven by a task nobody holds.
        let driving = Aborting(tokio::spawn(async move {
            let _ = resource.await;
            let _ = lost.send(Heard::Lost);
        }));
        listen(&connection, heard).await.ok_or(NoBus::Absent)?;
        let (publish, now) = watch::channel(None);
        let target = Arc::new(Mutex::new(None));
        let follower = Follower {
            connection: connection.clone(),
            players: BTreeMap::new(),
            clock: 0,
            publish,
            target: target.clone(),
            pids,
            due: Vec::new(),
        };
        let following = Aborting(tokio::spawn(follower.run(hearing)));
        Ok(Self {
            connection,
            now,
            target,
            _tasks: [following, driving],
        })
    }
}

/// The addresses a connect thread is still working on, which no other connect starts on.
static CONNECTING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// The claim on one bus address by the connect thread working on it, let go when it ends.
struct Blocked(String);

impl Blocked {
    /// The claim on `address`, or `None` while a connect thread still holds it.
    fn take(address: &str) -> Option<Self> {
        crate::locked(&CONNECTING)
            .insert(address.to_string())
            .then(|| Blocked(address.to_string()))
    }
}

impl Drop for Blocked {
    fn drop(&mut self) {
        crate::locked(&CONNECTING).remove(&self.0);
    }
}

/// A task that is ended when its handle is let go of.
struct Aborting(JoinHandle<()>);

impl Drop for Aborting {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[async_trait]
impl Players for Mpris {
    fn now_playing(&self) -> Option<NowPlaying> {
        self.now.borrow().clone()
    }

    fn changes(&self) -> Changes {
        let mut now = self.now.clone();
        now.mark_unchanged();
        futures::stream::unfold(now, |mut now| async move {
            now.changed().await.ok()?;
            let playing = now.borrow_and_update().clone();
            Some((playing, now))
        })
        .boxed()
    }

    async fn press(&self, button: Button) -> bool {
        let Some(owner) = crate::locked(&self.target).clone() else {
            return false;
        };
        let Ok(owner) = BusName::new(owner) else {
            return false;
        };
        let player = Proxy::new(owner, MPRIS_PATH, CALL_TIMEOUT, self.connection.clone());
        let reply: Result<(), dbus::Error> = player
            .method_call(PLAYER_INTERFACE, button.method(), ())
            .await;
        reply.is_ok()
    }
}

/// The session bus's address: `DBUS_SESSION_BUS_ADDRESS` when it says one, else the socket
/// `bus` under `XDG_RUNTIME_DIR` when that exists, else none.
fn session_address(named: Option<&OsStr>, runtime_dir: Option<&OsStr>) -> Option<String> {
    if let Some(address) = named
        .and_then(OsStr::to_str)
        .filter(|a| !a.trim().is_empty())
    {
        return Some(address.to_string());
    }
    let socket = Path::new(runtime_dir?).join("bus");
    socket
        .exists()
        .then(|| format!("unix:path={}", socket.display()))
}

/// What the bus said, as the following task reads it.
#[derive(Debug)]
enum Heard {
    /// A player's name changed hands: `owner` is its new unique name, empty when it left.
    Owner { name: String, owner: String },
    /// The player at the unique name `sender` changed some of its properties.
    Changed { sender: String, said: Said },
    /// The connection to the bus ended.
    Lost,
}

/// What one `PropertiesChanged` said about a player.
#[derive(Debug, Default)]
struct Said {
    status: Option<Status>,
    track: Option<Track>,
    identity: Option<String>,
    /// Whether it named properties to read again rather than giving their values.
    invalidated: bool,
    /// Whether something that moves the position changed (the status, the track, the rate, a
    /// seek): the position and the rate are read again.
    moved: bool,
}

/// Ask the bus for the three signals the players are followed by, and send each one that
/// arrives to `heard`. `None` when the bus refuses any.
async fn listen(connection: &SyncConnection, heard: mpsc::UnboundedSender<Heard>) -> Option<()> {
    let owners = heard.clone();
    connection.start_receive(
        MatchRule::new_signal(BUS_NAME, "NameOwnerChanged").with_sender(BUS_NAME),
        Box::new(move |message, _| {
            if let Ok((name, _, owner)) = message.read3::<String, String, String>()
                && name.starts_with(MPRIS_PREFIX)
            {
                let _ = owners.send(Heard::Owner { name, owner });
            }
            true
        }),
    );
    let seeks = heard.clone();
    connection.start_receive(
        MatchRule::new_signal(PROPERTIES_INTERFACE, "PropertiesChanged").with_path(MPRIS_PATH),
        Box::new(move |message, _| {
            let sender = message.sender().map(|s| s.to_string());
            if let (Some(sender), Ok(signal)) =
                (sender, message.read_all::<PropertiesPropertiesChanged>())
            {
                let said = said_of(&signal);
                let _ = heard.send(Heard::Changed { sender, said });
            }
            true
        }),
    );
    connection.start_receive(
        MatchRule::new_signal(PLAYER_INTERFACE, "Seeked").with_path(MPRIS_PATH),
        Box::new(move |message, _| {
            if let Some(sender) = message.sender().map(|s| s.to_string()) {
                let said = Said {
                    moved: true,
                    ..Said::default()
                };
                let _ = seeks.send(Heard::Changed { sender, said });
            }
            true
        }),
    );
    // The bus sends a signal only to a connection that asked for it; these ask for the ones
    // the receivers above read, and no more.
    let names = format!(
        "type='signal',sender='{BUS_NAME}',path='{BUS_PATH}',interface='{BUS_NAME}',\
         member='NameOwnerChanged',arg0namespace='{MPRIS_NAMESPACE}'"
    );
    let properties = format!(
        "type='signal',path='{MPRIS_PATH}',interface='{PROPERTIES_INTERFACE}',\
         member='PropertiesChanged'"
    );
    connection.add_match_no_cb(&names).await.ok()?;
    let seeked =
        format!("type='signal',path='{MPRIS_PATH}',interface='{PLAYER_INTERFACE}',member='Seeked'");
    connection.add_match_no_cb(&properties).await.ok()?;
    connection.add_match_no_cb(&seeked).await.ok()?;
    Some(())
}

/// What a `PropertiesChanged` says, for either of the two MPRIS interfaces.
fn said_of(signal: &PropertiesPropertiesChanged) -> Said {
    let props = &signal.changed_properties;
    match signal.interface_name.as_str() {
        PLAYER_INTERFACE => Said {
            status: props.get("PlaybackStatus").and_then(|v| status_of(&*v.0)),
            track: props.get("Metadata").map(|v| track_of(&*v.0)),
            identity: None,
            invalidated: signal
                .invalidated_properties
                .iter()
                .any(|p| p == "PlaybackStatus" || p == "Metadata"),
            moved: ["PlaybackStatus", "Metadata", "Rate"].iter().any(|key| {
                props.contains_key(*key) || signal.invalidated_properties.iter().any(|p| p == key)
            }),
        },
        ROOT_INTERFACE => Said {
            identity: props
                .get("Identity")
                .and_then(|v| inner(&*v.0).as_str().map(str::to_string)),
            invalidated: signal
                .invalidated_properties
                .iter()
                .any(|p| p == "Identity"),
            ..Said::default()
        },
        _ => Said::default(),
    }
}

/// `arg` with every variant around it taken off.
fn inner(mut arg: &dyn RefArg) -> &dyn RefArg {
    while arg.arg_type() == ArgType::Variant {
        match arg.as_iter().and_then(|mut items| items.next()) {
            Some(held) => arg = held,
            None => break,
        }
    }
    arg
}

/// The status a `PlaybackStatus` value names.
fn status_of(arg: &dyn RefArg) -> Option<Status> {
    Status::parse(inner(arg).as_str()?)
}

/// What a player's `Metadata` says about the track.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Track {
    title: String,
    /// The artists, joined by `, `.
    artist: String,
    genre: Vec<String>,
    album: String,
    art_url: Option<String>,
    track_id: Option<String>,
    length_us: Option<u64>,
}

/// The strings a metadata value holds: one plain string or an array of them.
fn strings_of(value: &dyn RefArg) -> Vec<String> {
    match value.as_str() {
        Some(one) => vec![one.to_string()],
        None => value
            .as_iter()
            .map(|all| {
                all.filter_map(|a| inner(a).as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// The track a `Metadata` value describes, each part empty or `None` when it holds none.
fn track_of(arg: &dyn RefArg) -> Track {
    let mut track = Track::default();
    let mut artists = Vec::new();
    if let Some(mut items) = inner(arg).as_iter() {
        while let (Some(key), Some(value)) = (items.next(), items.next()) {
            let value = inner(value);
            let text = || value.as_str().map(str::to_string).filter(|t| !t.is_empty());
            match key.as_str() {
                Some("xesam:title") => track.title = value.as_str().unwrap_or_default().to_string(),
                Some("xesam:artist") => artists = strings_of(value),
                Some("xesam:genre") => track.genre = strings_of(value),
                Some("xesam:album") => track.album = value.as_str().unwrap_or_default().to_string(),
                Some("mpris:artUrl") => track.art_url = text(),
                Some("mpris:trackid") => track.track_id = text(),
                Some("mpris:length") => track.length_us = microseconds_of(value),
                _ => {}
            }
        }
    }
    track.artist = artists.join(", ");
    track
}

/// A non-negative integer of any width as a count of microseconds.
fn microseconds_of(value: &dyn RefArg) -> Option<u64> {
    match value.as_i64() {
        Some(count) => u64::try_from(count).ok(),
        None => value.as_u64(),
    }
}

/// An MPRIS `Rate` in thousandths; 1000 (the normal rate) when the player gave none or a rate
/// that is not a positive number.
fn rate_milli(rate: Option<f64>) -> u32 {
    match rate {
        Some(rate) if rate.is_finite() && rate > 0.0 => (rate * 1000.0).round() as u32,
        _ => 1000,
    }
}

/// The name a player without an `Identity` goes by: its bus name after [`MPRIS_PREFIX`], up to
/// the next dot (`org.mpris.MediaPlayer2.firefox.instance_1_2` is `firefox`).
fn name_identity(name: &str) -> String {
    let rest = name.strip_prefix(MPRIS_PREFIX).unwrap_or(name);
    rest.split('.').next().unwrap_or(rest).to_string()
}

/// What the choice reads of one player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seen {
    /// What it is doing.
    status: Status,
    /// When it last started or stopped playing, on the follower's own count: for a player
    /// playing now, when it started; for any other, when it stopped. The higher the later, 0
    /// when it has not been seen playing.
    played: u64,
}

/// Which player to follow, from players listed by bus name: a playing one over any other
/// (the one that started last, between players playing), then the one that stopped playing
/// last, then the first listed.
fn choose(players: &[Seen]) -> Option<usize> {
    let mut chosen: Option<usize> = None;
    for (at, seen) in players.iter().enumerate() {
        let better = match chosen {
            None => true,
            Some(best) => {
                let best = players[best];
                let playing = seen.status == Status::Playing;
                let best_playing = best.status == Status::Playing;
                (playing, seen.played) > (best_playing, best.played)
            }
        };
        if better {
            chosen = Some(at);
        }
    }
    chosen
}

/// One player the follower knows of.
#[derive(Debug, Clone)]
struct Player {
    /// The unique bus name that owns the player's name, which its signals come from.
    owner: String,
    /// What it said when it was last asked; `None` while it has not answered, until the next
    /// signal it sends has it asked again.
    said: Option<Answer>,
    /// Whether it named properties as changed and then did not answer when asked for them:
    /// it keeps what it said before, and its place, until its next signal has it asked again.
    stale: bool,
    /// The process id the bus gives its connection, asked when the player joined.
    pid: Option<u32>,
    /// How many times the process id has been asked for again.
    pid_tries: u8,
}

/// What a player says about itself, and when it last started or stopped playing.
#[derive(Debug, Clone)]
struct Answer {
    identity: String,
    status: Status,
    track: Track,
    progress: Option<Progress>,
    /// See [`Seen::played`].
    played: u64,
}

/// The task that follows the players: it keeps what each said, chooses the one followed and
/// publishes what that one plays.
struct Follower {
    connection: Arc<SyncConnection>,
    /// Every player on the bus, by bus name, whether it has answered yet or not.
    players: BTreeMap<String, Player>,
    /// The last count handed to a player that started or stopped playing.
    clock: u64,
    publish: watch::Sender<Option<NowPlaying>>,
    /// The followed player's unique name, shared with [`Mpris::press`].
    target: Arc<Mutex<Option<String>>>,
    /// Where a process id is asked, `None` for the bus.
    pids: Option<PidLookup>,
    /// What is to be done later, and when.
    due: Vec<(Instant, Later)>,
}

/// Something the follower does a while after the signal that asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Later {
    /// Ask for the process id of the player `name`, owned by `owner`, again.
    Pid { name: String, owner: String },
    /// Read the position of the player owned by `owner` once more.
    Position { owner: String },
}

/// Sleep until `at`, or for good with none.
async fn nap(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

impl Follower {
    async fn run(mut self, mut hearing: mpsc::UnboundedReceiver<Heard>) {
        self.list().await;
        loop {
            let next = self.due.iter().map(|(at, _)| *at).min();
            tokio::select! {
                heard = hearing.recv() => match heard {
                    Some(Heard::Owner { name, owner }) => {
                        self.players.remove(&name);
                        if !owner.is_empty() {
                            let said = self.first_answer(&name, &owner).await;
                            self.join(name, owner, said).await;
                        }
                    }
                    Some(Heard::Changed { sender, said }) => self.changed(&sender, said).await,
                    Some(Heard::Lost) | None => break,
                },
                () = nap(next) => self.later().await,
            }
            self.follow();
        }
        self.players.clear();
        self.follow();
    }

    /// The next count, for a player that starts or stops playing now.
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Ask the bus for the players already on it and each of them what it plays. The ones
    /// playing now share one count, so between them the first by name is followed.
    async fn list(&mut self) {
        let bus = Proxy::new(BUS_NAME, BUS_PATH, CALL_TIMEOUT, self.connection.clone());
        let listed: Result<(Vec<String>,), dbus::Error> =
            bus.method_call(BUS_NAME, "ListNames", ()).await;
        let Ok((names,)) = listed else {
            return;
        };
        let stamp = self.tick();
        for name in names.into_iter().filter(|n| n.starts_with(MPRIS_PREFIX)) {
            let owned: Result<(String,), dbus::Error> = bus
                .method_call(BUS_NAME, "GetNameOwner", (name.as_str(),))
                .await;
            let Ok((owner,)) = owned else {
                continue;
            };
            let said = self.ask(&name, &owner).await.map(|mut said| {
                if said.status == Status::Playing {
                    said.played = stamp;
                }
                said
            });
            self.join(name, owner, said).await;
        }
        self.follow();
    }

    /// Take the player `name` owned by `owner` into the players, with what it first said and the
    /// process id the bus gives it; the id is asked for again later if the bus did not.
    async fn join(&mut self, name: String, owner: String, said: Option<Answer>) {
        let pid = self.pid_of(&owner).await;
        if pid.is_none() {
            self.defer(
                PID_RETRY,
                Later::Pid {
                    name: name.clone(),
                    owner: owner.clone(),
                },
            );
        }
        self.players.insert(
            name,
            Player {
                owner,
                said,
                stale: false,
                pid,
                pid_tries: 0,
            },
        );
    }

    /// Do `what` in `after`; a position read already due for the same player moves to then.
    fn defer(&mut self, after: Duration, what: Later) {
        if matches!(what, Later::Position { .. }) {
            self.due.retain(|(_, due)| *due != what);
        }
        self.due.push((Instant::now() + after, what));
    }

    /// Do what has come due.
    async fn later(&mut self) {
        let now = Instant::now();
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.due)
            .into_iter()
            .partition(|(at, _)| *at <= now);
        self.due = waiting;
        for (_, what) in ready {
            match what {
                Later::Pid { name, owner } => self.retry_pid(&name, &owner).await,
                Later::Position { owner } => {
                    let names = self.names_of(&owner);
                    if names.is_empty() {
                        continue;
                    }
                    let progress = self.progress_of(&owner).await;
                    for name in names {
                        if let Some(said) = self
                            .players
                            .get_mut(&name)
                            .and_then(|player| player.said.as_mut())
                        {
                            said.progress = progress.or(said.progress);
                        }
                    }
                }
            }
        }
    }

    /// The names of the players the connection `owner` owns.
    fn names_of(&self, owner: &str) -> Vec<String> {
        self.players
            .iter()
            .filter(|(_, p)| p.owner == owner)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Ask again for the process id of the player `name` owned by `owner`, if it still is the
    /// one and has none and has been asked for fewer than [`PID_RETRIES`] times since it joined.
    async fn retry_pid(&mut self, name: &str, owner: &str) {
        let Some(player) = self.players.get_mut(name) else {
            return;
        };
        if player.owner != owner || player.pid.is_some() || player.pid_tries >= PID_RETRIES {
            return;
        }
        player.pid_tries += 1;
        let pid = self.pid_of(owner).await;
        if let Some(player) = self.players.get_mut(name)
            && player.owner == owner
        {
            player.pid = pid;
        }
    }

    /// What a player that has not answered before says, stamped now when it is playing.
    async fn first_answer(&mut self, name: &str, owner: &str) -> Option<Answer> {
        let mut said = self.ask(name, owner).await?;
        if said.status == Status::Playing {
            said.played = self.tick();
        }
        Some(said)
    }

    /// Ask the player `name`, owned by `owner`, what it is and what it plays, unstamped.
    /// `None` when it does not answer as a player in time.
    async fn ask(&self, name: &str, owner: &str) -> Option<Answer> {
        let proxy = Proxy::new(
            BusName::new(owner.to_string()).ok()?,
            MPRIS_PATH,
            CALL_TIMEOUT,
            self.connection.clone(),
        );
        let props: PropMap = proxy.get_all(PLAYER_INTERFACE).await.ok()?;
        let status = props
            .get("PlaybackStatus")
            .and_then(|v| status_of(&*v.0))
            .unwrap_or(Status::Stopped);
        let track = props
            .get("Metadata")
            .map(|v| track_of(&*v.0))
            .unwrap_or_default();
        let identity = proxy
            .get::<String>(ROOT_INTERFACE, "Identity")
            .await
            .ok()
            .filter(|i| !i.trim().is_empty())
            .unwrap_or_else(|| name_identity(name));
        let progress = self.progress_of(owner).await;
        Some(Answer {
            identity,
            status,
            track,
            progress,
            played: 0,
        })
    }

    /// Where the player owned by `owner` is in its track, with the time it was read at. `None`
    /// when it does not answer `Position`; a `Rate` it does not answer is the normal one.
    async fn progress_of(&self, owner: &str) -> Option<Progress> {
        let proxy = Proxy::new(
            BusName::new(owner.to_string()).ok()?,
            MPRIS_PATH,
            CALL_TIMEOUT,
            self.connection.clone(),
        );
        let position: i64 = proxy.get(PLAYER_INTERFACE, "Position").await.ok()?;
        let read_at_ms = unix_ms();
        let rate: Option<f64> = proxy.get(PLAYER_INTERFACE, "Rate").await.ok();
        Some(Progress {
            position_us: u64::try_from(position).unwrap_or(0),
            rate_milli: rate_milli(rate),
            read_at_ms,
        })
    }

    /// The process id the bus gives the connection `owner`, or `None` when it does not say.
    async fn pid_of(&self, owner: &str) -> Option<u32> {
        if let Some(pids) = &self.pids {
            return pids(owner.to_string()).await;
        }
        let bus = Proxy::new(BUS_NAME, BUS_PATH, CALL_TIMEOUT, self.connection.clone());
        let asked: Result<(u32,), dbus::Error> = bus
            .method_call(BUS_NAME, "GetConnectionUnixProcessID", (owner,))
            .await;
        asked.ok().map(|(pid,)| pid)
    }

    /// Fold what the player at `sender` said into every name it owns. A player that has not
    /// answered yet is asked again, and so is one that named properties as changed or did not
    /// answer the last time it was asked for them; one that does not answer then keeps what it
    /// said before and is asked again on its next signal.
    async fn changed(&mut self, sender: &str, said: Said) {
        for name in self.names_of(sender) {
            // A player the bus gave no process id is asked once more on this signal.
            if self.players[&name].pid.is_none() {
                self.retry_pid(&name, sender).await;
            }
            let (before, stale) = {
                let player = &self.players[&name];
                (player.said.clone(), player.stale)
            };
            let Some(before) = before else {
                let first = self.first_answer(&name, sender).await;
                if let Some(player) = self.players.get_mut(&name) {
                    player.said = first;
                }
                continue;
            };
            let mut now = before.clone();
            // Asked again for what it named as changed, or for what it did not answer last
            // time. Without an answer it keeps what it said before, its place among the
            // players with it, and is asked again on its next signal.
            let mut still_stale = false;
            let mut asked_all = false;
            if said.invalidated || stale {
                match self.ask(&name, sender).await {
                    Some(asked) => {
                        asked_all = true;
                        now = Answer {
                            played: before.played,
                            ..asked
                        };
                    }
                    None => still_stale = true,
                }
            }
            if let Some(status) = said.status {
                now.status = status;
            }
            if let Some(track) = &said.track {
                now.track = track.clone();
            }
            if said.moved && !asked_all {
                now.progress = self.progress_of(sender).await;
            }
            if let Some(identity) = said.identity.as_ref().filter(|i| !i.trim().is_empty()) {
                now.identity = identity.clone();
            }
            // Starting or stopping moves the stamp: a playing player's is when it started, any
            // other's when it stopped.
            if (now.status == Status::Playing) != (before.status == Status::Playing) {
                now.played = self.tick();
            }
            let new_track = now.track != before.track;
            if let Some(player) = self.players.get_mut(&name) {
                player.said = Some(now);
                player.stale = still_stale;
            }
            if new_track {
                self.defer(
                    POSITION_REREAD,
                    Later::Position {
                        owner: sender.to_string(),
                    },
                );
            }
        }
    }

    /// Choose the player to follow and publish what it plays, when that changed.
    fn follow(&mut self) {
        // Only players that have answered can be followed: nothing is known of the others.
        let listed: Vec<(&Player, &Answer)> = self
            .players
            .values()
            .filter_map(|p| Some((p, p.said.as_ref()?)))
            .collect();
        let seen: Vec<Seen> = listed
            .iter()
            .map(|(_, said)| Seen {
                status: said.status,
                played: said.played,
            })
            .collect();
        let followed = choose(&seen).map(|at| listed[at]);
        *crate::locked(&self.target) = followed.map(|(player, _)| player.owner.clone());
        let playing = followed.map(|(player, said)| NowPlaying {
            title: said.track.title.clone(),
            artist: said.track.artist.clone(),
            genre: said.track.genre.clone(),
            identity: said.identity.clone(),
            status: said.status,
            album: said.track.album.clone(),
            art_url: said.track.art_url.clone(),
            track_id: said.track.track_id.clone(),
            length_us: said.track.length_us,
            progress: said.progress,
            pid: player.pid,
        });
        self.publish.send_if_modified(|now| {
            if *now == playing {
                return false;
            }
            *now = playing;
            true
        });
    }
}

#[cfg(test)]
mod tests;
