// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The host's default audio sink, through WirePlumber's `wpctl`: its volume, whether it is
//! muted and what it is called, read from and written to `@DEFAULT_AUDIO_SINK@`.
//!
//! The default sink is the one read and changed, whichever device it is; its name comes along
//! so a caller can say which one that is. Every call that reaches `wpctl` runs it once and gives
//! it [`TIMEOUT`]: a missing tool, one that fails or one that does not answer in time reads as
//! no sink and writes nothing, never as an error.
//!
//! A fresh read asks `get-volume` and `inspect` only when the sink may have changed: on the
//! first read, after a read that failed, when the volume or the mute moved, and once the name it
//! holds is [`INSPECT_EVERY`] old, which catches a new default sink that happens to read the
//! same volume.
//!
//! The daemon reads the sink from two places on a 2 s beat. The sinks [`Wpctl::from_env`] makes
//! for one program share their naming and a published reading: a plain one reads fresh and
//! publishes what it read; one made [`held`](Wpctl::held) gives the published reading while it
//! is younger than [`READ_HELD`] and runs `wpctl` only when there is none, so the second reader
//! costs nothing while the first one is watching, and never makes the first one's reading
//! older. Any `set-*` call through any [`Wpctl`] of the program drops the published reading, so
//! a read after a write is fresh.

use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::process::Command;

/// The environment variable naming the `wpctl` to run, for [`Wpctl::from_env`]; unset or
/// empty, it is `wpctl` on `PATH`.
pub const WPCTL_ENV: &str = "SVOLDER_WPCTL";

/// The node every call reads or writes: WirePlumber's name for the default audio sink.
pub const DEFAULT_SINK: &str = "@DEFAULT_AUDIO_SINK@";

/// How long one `wpctl` call gets before it is killed and read as no answer.
pub const TIMEOUT: Duration = Duration::from_secs(1);

/// How long the name and the node `wpctl inspect` gave stand for the default sink while its
/// volume and mute read the same: a new default sink is caught within this.
pub const INSPECT_EVERY: Duration = Duration::from_secs(5);

/// How long a published reading of the default sink stands for a [held](Wpctl::held) reader:
/// a little over the 2 s beat of the reader that publishes it, so a held reader that comes just
/// before the next publication is still served.
pub const READ_HELD: Duration = Duration::from_millis(2500);

/// What the default sink said when it was read.
#[derive(Debug, Clone, PartialEq)]
pub struct SinkState {
    /// The volume, 1.0 at 100 %. More than 1.0 when something else boosted the sink.
    pub volume: f32,
    /// Whether the sink is muted.
    pub muted: bool,
    /// What the sink is called for people: its `node.description`, else its `node.name`;
    /// empty when `wpctl inspect` gave neither.
    pub name: String,
    /// The node name a capture of this sink's monitor targets: its `node.name`, and only when
    /// the node is a sink. `None` when `wpctl inspect` gave no name or named another kind of
    /// node, so nothing can be captured from it.
    pub target: Option<String>,
}

/// The host's default audio sink.
#[async_trait]
pub trait Sink: Send + Sync {
    /// The sink as it is now, or `None` when there is no sink to read (no tool, or it failed).
    async fn read(&self) -> Option<SinkState>;

    /// Set the volume to `volume` (1.0 = 100 %), capped to `0.0..=1.0`: never a boost.
    /// Whether the sink took it; a volume that is not a number is not sent.
    async fn set_volume(&self, volume: f32) -> bool;

    /// Mute the sink when it plays and unmute it when it is muted. Whether the sink took it.
    async fn toggle_mute(&self) -> bool;

    /// Mute the sink (`true`) or unmute it (`false`), whatever it was. Whether the sink took it.
    async fn set_mute(&self, muted: bool) -> bool;
}

/// The default sink through `wpctl`.
#[derive(Debug, Clone)]
pub struct Wpctl {
    /// The `wpctl` to run.
    program: OsString,
    /// How long each call gets.
    timeout: Duration,
    /// How long a naming stands while the volume and the mute read the same.
    inspect_every: Duration,
    /// What every clone of this sink shares: the naming and the published reading.
    shared: Arc<Shared>,
    /// Whether a read gives the published reading while it is younger than this, instead of
    /// reading fresh and publishing.
    held: Option<Duration>,
}

/// What clones of a [`Wpctl`] (and every one [`Wpctl::shared_for`] makes for the same program)
/// share, as they read the same sink.
#[derive(Debug)]
struct Shared {
    /// What `wpctl inspect` last named the sink, and what it read then: `None` before the first
    /// read and after one that failed.
    named: Mutex<Option<Named>>,
    /// The reading a fresh reader last published.
    held: Coalescer,
}

impl Shared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            named: Mutex::default(),
            held: Coalescer::default(),
        })
    }
}

/// The last reading a fresh reader published, which held readers take while it is young and
/// which a write drops.
#[derive(Debug, Default)]
pub struct Coalescer {
    /// The published reading and when, and how many writes there have been.
    state: Mutex<Published>,
}

#[derive(Debug, Default)]
struct Published {
    reading: Option<(tokio::time::Instant, SinkState)>,
    writes: u64,
}

impl Coalescer {
    /// What `fresh` reads, published for held readers (unless a write came while it ran, or it
    /// read nothing: a failed read drops what was published).
    pub async fn publish(
        &self,
        fresh: impl Future<Output = Option<SinkState>>,
    ) -> Option<SinkState> {
        let writes = crate::locked(&self.state).writes;
        let read = fresh.await;
        let mut state = crate::locked(&self.state);
        state.reading = match &read {
            Some(read) if state.writes == writes => {
                Some((tokio::time::Instant::now(), read.clone()))
            }
            _ => None,
        };
        read
    }

    /// The published reading when it is younger than `hold`, else what `fresh` reads, which is
    /// given back and not published (a held reader never makes the reading look younger).
    pub async fn held(
        &self,
        hold: Duration,
        fresh: impl Future<Output = Option<SinkState>>,
    ) -> Option<SinkState> {
        let young = match &crate::locked(&self.state).reading {
            Some((at, read)) if at.elapsed() < hold => Some(read.clone()),
            _ => None,
        };
        match young {
            Some(read) => Some(read),
            None => fresh.await,
        }
    }

    /// Note a write to the sink: the published reading is dropped, and so is the one a read under
    /// way would publish.
    pub fn wrote(&self) {
        let mut state = crate::locked(&self.state);
        state.writes += 1;
        state.reading = None;
    }
}

/// The state the sinks of each program share, for [`Wpctl::shared_for`] and for the writes that
/// drop a published reading.
type SharedFor = (OsString, Arc<Shared>);

fn registry() -> std::sync::MutexGuard<'static, Vec<SharedFor>> {
    static SHARED: OnceLock<Mutex<Vec<SharedFor>>> = OnceLock::new();
    crate::locked(SHARED.get_or_init(Mutex::default))
}

/// The default sink as `wpctl inspect` last named it, and as `get-volume` read at that moment.
#[derive(Debug, Clone)]
struct Named {
    /// The volume read with the naming.
    volume: f32,
    /// The mute read with the naming.
    muted: bool,
    /// What the sink is called for people.
    name: String,
    /// The node a capture targets, when it is a sink.
    target: Option<String>,
    /// When it was named.
    at: Instant,
}

impl Wpctl {
    /// The sink through the `wpctl` at `program` (a path, or a name looked up on `PATH`).
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            timeout: TIMEOUT,
            inspect_every: INSPECT_EVERY,
            shared: Shared::new(),
            held: None,
        }
    }

    /// The sink through the `wpctl` [`WPCTL_ENV`] names, else `wpctl` on `PATH`; `None` when
    /// [`host_unreached`](crate::host_unreached) says host audio is off for this process. See
    /// [`Wpctl::shared_for`].
    pub fn from_env() -> Option<Self> {
        if crate::host_unreached() {
            return None;
        }
        let program = std::env::var_os(WPCTL_ENV)
            .filter(|named| !named.is_empty())
            .unwrap_or_else(|| "wpctl".into());
        Some(Self::shared_for(program))
    }

    /// The sink through `program`, sharing its naming and its published reading with every
    /// other sink made here for the same program (and with their clones).
    pub fn shared_for(program: impl Into<OsString>) -> Self {
        let program = program.into();
        let mut all = registry();
        let at = all.iter().position(|(named, _)| *named == program);
        let shared = match at {
            Some(at) => Arc::clone(&all[at].1),
            None => {
                let shared = Shared::new();
                all.push((program.clone(), Arc::clone(&shared)));
                shared
            }
        };
        Self {
            shared,
            ..Self::new(program)
        }
    }

    /// The same sink as a held reader: a read gives the reading another sink of the program
    /// published within [`READ_HELD`], and reads fresh (publishing nothing) when there is none.
    pub fn held(self) -> Self {
        self.held_for(READ_HELD)
    }

    /// [`Wpctl::held`] with readings standing `hold` instead of [`READ_HELD`].
    pub fn held_for(mut self, hold: Duration) -> Self {
        self.held = Some(hold);
        self
    }

    /// The same sink with each call given `timeout` instead of [`TIMEOUT`].
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The same sink with a naming standing `every` instead of [`INSPECT_EVERY`].
    pub fn with_inspect_every(mut self, every: Duration) -> Self {
        self.inspect_every = every;
        self
    }

    /// `wpctl`'s standard output when it ran, exited 0 and did so in time; the child is killed
    /// when it runs out of time.
    /// A `set-*` call, done or not, drops the reading published for this program.
    pub async fn run(&self, args: &[&str]) -> Option<String> {
        let out = output_within(&self.program, args, self.timeout).await;
        if args.first().is_some_and(|verb| verb.starts_with("set-")) {
            self.shared.held.wrote();
            if let Some((_, shared)) = registry().iter().find(|(named, _)| *named == self.program) {
                shared.held.wrote();
            }
        }
        out
    }
}

/// `program`'s standard output when it ran with `args`, exited 0 and did so within `timeout`;
/// the child is killed when it runs out of time.
pub async fn output_within(program: &OsStr, args: &[&str], timeout: Duration) -> Option<String> {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .ok()?
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

impl Wpctl {
    /// The id of the sink whose `node.name` is `name`, found by reading `wpctl status` for the
    /// sink ids (see [`sink_ids`]) and inspecting each; `None` when there is no such sink or
    /// `wpctl` does not answer.
    pub async fn find_sink(&self, name: &str) -> Option<u32> {
        let status = self.run(&["status"]).await?;
        for id in sink_ids(&status) {
            let inspected = self.run(&["inspect", &id.to_string()]).await;
            if inspected
                .as_deref()
                .is_some_and(|text| parse_inspect(text).1.as_deref() == Some(name))
            {
                return Some(id);
            }
        }
        None
    }

    /// [`Wpctl::find_sink`], tried again every `poll` until `within` has passed: for a sink
    /// that a program just started is about to create.
    pub async fn wait_for_sink(&self, name: &str, within: Duration, poll: Duration) -> Option<u32> {
        let deadline = Instant::now() + within;
        loop {
            if let Some(id) = self.find_sink(name).await {
                return Some(id);
            }
            if Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(poll).await;
        }
    }
}

/// The ids of the sinks `wpctl status` lists, in its order: the numbered entries under its
/// `Sinks:` heading, and the entries tagged `[Audio/Sink]` under `Filters:` (WirePlumber 0.5
/// lists a filter-chain sink there).
pub fn sink_ids(status: &str) -> Vec<u32> {
    #[derive(PartialEq)]
    enum Section {
        Other,
        Sinks,
        Filters,
    }
    let mut ids = Vec::new();
    let mut section = Section::Other;
    for line in status.lines() {
        let line = line.trim_start_matches(|c: char| "│├└─ \t*".contains(c));
        if line.trim_end() == "Sinks:" {
            section = Section::Sinks;
        } else if line.trim_end() == "Filters:" {
            section = Section::Filters;
        } else if line.trim_end().ends_with(':')
            || matches!(line.trim_end(), "Audio" | "Video" | "Settings")
        {
            section = Section::Other;
        } else if section != Section::Other
            && (section == Section::Sinks || line.trim_end().ends_with("[Audio/Sink]"))
            && let Some((number, _)) = line.split_once('.')
            && let Ok(id) = number.parse()
        {
            ids.push(id);
        }
    }
    ids
}

impl Wpctl {
    /// One reading of the sink, asked of `wpctl` now (see [`Sink::read`] for what it asks).
    async fn read_fresh(&self) -> Option<SinkState> {
        let read = self
            .run(&["get-volume", DEFAULT_SINK])
            .await
            .as_deref()
            .and_then(parse_volume);
        let Some((volume, muted)) = read else {
            *crate::locked(&self.shared.named) = None;
            return None;
        };
        let standing = crate::locked(&self.shared.named).clone().filter(|named| {
            named.volume == volume
                && named.muted == muted
                && named.at.elapsed() < self.inspect_every
        });
        let (name, target) = match standing {
            Some(named) => (named.name, named.target),
            None => {
                let inspect = self.run(&["inspect", DEFAULT_SINK]).await;
                let (name, target) = inspect.as_deref().map(parse_inspect).unwrap_or_default();
                *crate::locked(&self.shared.named) = inspect.is_some().then(|| Named {
                    volume,
                    muted,
                    name: name.clone(),
                    target: target.clone(),
                    at: Instant::now(),
                });
                (name, target)
            }
        };
        Some(SinkState {
            volume,
            muted,
            name,
            target,
        })
    }
}

#[async_trait]
impl Sink for Wpctl {
    /// Runs `wpctl get-volume` on the default sink, and `wpctl inspect` after it when the sink
    /// may have changed (see the module's docs). The volume is what makes a sink: without it
    /// there is none, and the next read names the sink afresh; an `inspect` that fails leaves
    /// the sink unnamed and with nothing to capture, and is asked again on the next read.
    async fn read(&self) -> Option<SinkState> {
        match self.held {
            Some(hold) => self.shared.held.held(hold, self.read_fresh()).await,
            None => self.shared.held.publish(self.read_fresh()).await,
        }
    }

    async fn set_volume(&self, volume: f32) -> bool {
        if !volume.is_finite() {
            return false;
        }
        let volume = format!("{:.2}", volume.clamp(0.0, 1.0));
        self.run(&["set-volume", DEFAULT_SINK, &volume])
            .await
            .is_some()
    }

    async fn toggle_mute(&self) -> bool {
        self.run(&["set-mute", DEFAULT_SINK, "toggle"])
            .await
            .is_some()
    }

    async fn set_mute(&self, muted: bool) -> bool {
        let muted = if muted { "1" } else { "0" };
        self.run(&["set-mute", DEFAULT_SINK, muted]).await.is_some()
    }
}

/// The volume and the mute `wpctl get-volume` printed: `Volume: 0.54`, or `Volume: 0.54
/// [MUTED]` when muted. `None` for anything else.
fn parse_volume(text: &str) -> Option<(f32, bool)> {
    let line = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("Volume:"))?;
    let mut words = line.split_whitespace();
    let volume: f32 = words.next()?.parse().ok()?;
    if !volume.is_finite() || volume < 0.0 {
        return None;
    }
    let muted = words.any(|word| word == "[MUTED]");
    Some((volume, muted))
}

/// The name for people and the capture target `wpctl inspect` printed, from its
/// `key = "value"` lines (those marked `*` alike): `node.description`, else `node.name`, for
/// the first; `node.name` for the second, when `media.class`, if given, says the node is a
/// sink.
pub fn parse_inspect(text: &str) -> (String, Option<String>) {
    let mut description = None;
    let mut node = None;
    let mut class = None;
    for line in text.lines() {
        let line = line.trim_start();
        let line = line.strip_prefix('*').unwrap_or(line).trim();
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value)
            .to_string();
        match key.trim() {
            "node.description" => description = Some(value),
            "node.name" => node = Some(value),
            "media.class" => class = Some(value),
            _ => {}
        }
    }
    let is_sink = class.as_deref().is_none_or(|class| class.contains("Sink"));
    let target = node.clone().filter(|name| is_sink && !name.is_empty());
    let name = description
        .filter(|d| !d.is_empty())
        .or(node)
        .unwrap_or_default();
    (name, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_ids_under_sinks_are_sinks() {
        let status = "PipeWire 1.6.2\n\nAudio\n ├─ Devices:\n │      40. Card\n │\n ├─ Sinks:\n │  *   42. DAC [vol: 1.00]\n │      43. Speakers\n │\n ├─ Sources:\n │      50. Mic\n ├─ Filters:\n │      60. x [Stream/Output/Audio]\n └─ Streams:\n        70. y\n\nVideo\n ├─ Sinks:\n";
        assert_eq!(sink_ids(status), [42, 43]);
        assert_eq!(sink_ids(""), Vec::<u32>::new());
    }

    /// What `wpctl status` of WirePlumber 0.5 prints on a private PipeWire with a null sink and
    /// the filter chain running.
    const STATUS_WITH_CHAIN: &str = r#"PipeWire 'svolder-e2e-0' [1.6.2, user@host, cookie:1457588367]
 └─ Clients:
        23. WirePlumber                         [1.6.2, user@host, pid:737770]
        28. WirePlumber [export]                [1.6.2, user@host, pid:737770]
        33. pw-cat                              [1.6.2, user@host, pid:737895]
        39. wpctl                               [1.6.2, user@host, pid:739268]
        50. pipewire                            [1.6.2, user@host, pid:739242]
        51. pw-cli                              [1.6.2, user@host, pid:737979]

Audio
 ├─ Devices:
 │  
 ├─ Sinks:
 │  *   17. svolder e2e sink                    [vol: 1.00]
 │  
 ├─ Sources:
 │  
 ├─ Filters:
 │    - filter-chain-739242-7                                       
 │      49. svolder_eq.output                                            [Stream/Output/Audio]
 │      57. svolder_eq                                                   [Audio/Sink]
 │  
 └─ Streams:
        34. pw-cat                                                      
             32. output_FL       > svolder EQ:playback_FL	[active]
             35. output_FR       > svolder EQ:playback_FR	[active]

Video
 ├─ Devices:
 │  
 ├─ Sinks:
 │  
 ├─ Sources:
 │  
 ├─ Filters:
 │  
 └─ Streams:

Settings
 └─ Default Configured Devices:
"#;

    #[test]
    fn a_filter_chain_sink_under_filters_is_a_sink() {
        assert_eq!(sink_ids(STATUS_WITH_CHAIN), [17, 57]);
    }

    /// What `wpctl inspect` prints for a USB DAC, cut to the lines that matter and a few that
    /// do not.
    const INSPECT: &str = "id 56, type PipeWire:Interface:Node\n    \
        audio.channels = \"2\"\n    \
        audio.position = \"FL,FR\"\n  \
        * client.id = \"34\"\n  \
        * media.class = \"Audio/Sink\"\n  \
        * node.description = \"Schiit Modi+ Analog Stereo\"\n  \
        * node.name = \"alsa_output.usb-Schiit_Audio_Modi_-00.analog-stereo\"\n  \
        * node.nick = \"Modi+\"\n";

    #[test]
    fn a_volume_reads_as_a_fraction_and_unmuted() {
        assert_eq!(parse_volume("Volume: 0.54\n"), Some((0.54, false)));
        assert_eq!(parse_volume("Volume: 1.00"), Some((1.0, false)));
    }

    #[test]
    fn a_muted_volume_reads_muted() {
        assert_eq!(parse_volume("Volume: 0.54 [MUTED]\n"), Some((0.54, true)));
    }

    #[test]
    fn a_boosted_volume_reads_as_it_is() {
        assert_eq!(parse_volume("Volume: 1.30"), Some((1.3, false)));
    }

    #[test]
    fn anything_else_reads_as_no_volume() {
        for text in [
            "",
            "Volume:",
            "Volume: loud",
            "Volume: -0.2",
            "Volume: NaN",
            "Translate: 0.5",
        ] {
            assert_eq!(parse_volume(text), None, "{text:?}");
        }
    }

    #[test]
    fn inspect_names_the_sink_by_its_description_and_targets_its_node_name() {
        assert_eq!(
            parse_inspect(INSPECT),
            (
                "Schiit Modi+ Analog Stereo".to_string(),
                Some("alsa_output.usb-Schiit_Audio_Modi_-00.analog-stereo".to_string())
            )
        );
    }

    #[test]
    fn without_a_description_the_node_name_names_the_sink() {
        let text = "id 3\n  * node.name = \"hdmi-out\"\n";
        assert_eq!(
            parse_inspect(text),
            ("hdmi-out".to_string(), Some("hdmi-out".to_string()))
        );
    }

    #[test]
    fn inspect_without_either_name_leaves_the_sink_unnamed_and_untargeted() {
        assert_eq!(
            parse_inspect("id 3\n    audio.channels = \"2\"\n"),
            (String::new(), None)
        );
        assert_eq!(parse_inspect(""), (String::new(), None));
    }

    #[test]
    fn a_node_that_is_not_a_sink_is_never_a_capture_target() {
        let text = "  * media.class = \"Audio/Source\"\n  \
            * node.description = \"Webcam Microphone\"\n  \
            * node.name = \"alsa_input.usb-webcam\"\n";
        assert_eq!(parse_inspect(text), ("Webcam Microphone".to_string(), None));
    }
}
