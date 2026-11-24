// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Now playing card: what the daemon's host view says is playing, and the path the sound
//! takes to this device, in the page's words. The host is the same for every device; the path
//! is not, because it is judged against this device's own readings (its name, its input and
//! the sample rate it reads). The card exists when the host is available: a player that is
//! not there gives the one quiet line.

use svolder_host::Progress;
use svolder_ipc::host::{
    DacFacts, DacStage, EndToEnd, HostView, PathLine, PathRow, PlayingView, ROUTE_NOT_READ,
    SETUP_LIMIT, SetupCheck, Verdict, path_line, path_row, plays_without_player, setup_check,
    sink_is_dac,
};
use svolder_profiles::Family;

use super::{Access, Snapshot, minus};

/// The Now playing card of a device's screen.
#[derive(Debug)]
pub(crate) struct Playing {
    /// The daemon's clock when the card was made, in unix milliseconds: what the page counts
    /// the track's time forward against. Left out of [`Playing::key`].
    pub(crate) now_ms: u64,
    /// The followed player's track; `None` says there is no player.
    pub(crate) track: Option<Track>,
    /// The path the sound takes to this device; `None` when there is nothing to say of it.
    pub(crate) path: Option<Path>,
    /// Whether the transport buttons are drawn: not for a guest, and not with writes off.
    pub(crate) buttons: bool,
    /// Whether the visualizer names the sink it listens to: the computer's sink is not this
    /// page's DAC (another sink, none read yet, or an amp, which plays nothing over the
    /// computer's audio). The part carries it for the card, which draws the name.
    pub(crate) sink_label: bool,
}

/// The track of the followed player.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Track {
    /// The title, or what stands in for one.
    pub(crate) headline: String,
    /// The artist, empty when the player gave none.
    pub(crate) artist: String,
    /// The album, empty when the player gave none.
    pub(crate) album: String,
    /// The album and the player's name, as one line.
    pub(crate) source: String,
    /// `playing`, `paused` or `stopped`.
    pub(crate) status: &'static str,
    /// The cover's hash, for the cover's address.
    pub(crate) cover: Option<String>,
    /// The track's length in microseconds.
    pub(crate) length_us: Option<u64>,
    /// Where the player was in the track when read, at what rate, and when.
    pub(crate) progress: Option<Progress>,
}

/// The path line: the hops and what they add up to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Path {
    /// What the path does to the signal, as the judge keys it (`Verdict::key`): `untouched`,
    /// `changed`, `not_this_dac`, `other_input` or `unknown`.
    pub(crate) verdict: &'static str,
    /// What the path does to the sound end to end, the DAC's own stage included, as the judge
    /// keys it (`EndToEnd::key`): the verdict, with an untouched one whose DAC changes the sound
    /// `changed`.
    pub(crate) end_to_end: &'static str,
    /// The Path row every theme draws: the label's state and words, each hop's words with their
    /// tones, the values.
    pub(crate) row: PathRow,
    /// The hops, player to DAC; none when the sound goes to another sink.
    pub(crate) hops: Vec<Hop>,
    /// The words of the verdict, one per change or thing not read.
    pub(crate) words: Vec<String>,
    /// The whole sentence behind short words (`the computer plays to DELL S2721QS`), for a
    /// tooltip; empty when the words say it all.
    pub(crate) title: String,
    /// The PipeWire config check of the sink, while the sink is this DAC; `None` for another
    /// sink.
    pub(crate) setup: Option<SetupView>,
}

/// The config check of the sink as the card shows it, in the judge's words.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SetupView {
    /// What the lines add up to, as the judge keys it (`SetupSummary::key`): `untouched`,
    /// `changes`, `unread`, `not_read` or `other_version`.
    pub(crate) outcome: &'static str,
    /// What the check cannot judge, the judge's sentence.
    pub(crate) limit: &'static str,
    /// The short chip: `config ✓`, `config 2` or `config ?`.
    pub(crate) chip: String,
    /// The summary in words (`PipeWire config lets it through untouched`).
    pub(crate) summary: String,
    /// The sink the check judges, as people name it.
    pub(crate) sink: String,
    /// What comes in from the player, in the judge's words (`44.1 kHz · S16LE · stereo`).
    pub(crate) from_player: String,
    /// What the card is open at, in the judge's words (`44.1 kHz · 32-bit (S32_LE) · stereo ·
    /// hw:2`, or `not open`).
    pub(crate) to_dac: String,
    /// The lines of the check, in the judge's order.
    pub(crate) lines: Vec<SetupLineView>,
}

/// One line of the config check.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SetupLineView {
    /// The condition's stable key (`rates`, `forced_rate`, `dither`, `format`, `plug`, `stereo`,
    /// `filter`).
    pub(crate) key: String,
    /// The condition as a row's name (`24-bit output`).
    pub(crate) label: &'static str,
    /// `pass`, `fail` or `unknown`.
    pub(crate) outcome: &'static str,
    /// What was found.
    pub(crate) words: String,
}

/// One hop of the path.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Hop {
    /// Which: `player`, `computer` or `DAC`.
    pub(crate) label: String,
    /// What is known of the signal there.
    pub(crate) detail: String,
    /// Whether this hop alters the signal.
    pub(crate) changes: bool,
    /// Whether something of this hop was not read.
    pub(crate) unknown: bool,
}

impl Playing {
    /// What the card says, apart from the clock and the time counted from it: two cards with
    /// the same key look the same, so the stream sends a card when its key changes. Every field
    /// a viewer can see is written out here, so a field added to the card is added to the key
    /// on purpose.
    pub(crate) fn key(&self) -> String {
        const SEP: char = '\u{1f}';
        let mut key = format!(
            "buttons={}{SEP}sink_label={}",
            self.buttons, self.sink_label
        );
        match &self.track {
            None => key.push_str(&format!("{SEP}no player")),
            Some(track) => {
                let progress = track
                    .progress
                    .map(|p| (p.position_us, p.rate_milli, p.read_at_ms));
                key.push_str(&format!(
                    "{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{:?}{SEP}{:?}{SEP}{progress:?}",
                    track.headline,
                    track.artist,
                    track.album,
                    track.source,
                    track.status,
                    track.cover,
                    track.length_us,
                ));
            }
        }
        if let Some(path) = &self.path {
            key.push_str(&format!(
                "{SEP}path={}{SEP}{}{SEP}{}{SEP}{}{SEP}{:?}",
                path.verdict,
                path.words.join(" · "),
                path.title,
                path.end_to_end,
                path.row
            ));
            for hop in &path.hops {
                key.push_str(&format!(
                    "{SEP}{}{SEP}{}{SEP}{}{SEP}{}",
                    hop.label, hop.detail, hop.changes, hop.unknown
                ));
            }
            match &path.setup {
                None => key.push_str(&format!("{SEP}config=none")),
                Some(setup) => {
                    key.push_str(&format!(
                        "{SEP}config={}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}",
                        setup.outcome,
                        setup.chip,
                        setup.summary,
                        setup.sink,
                        setup.from_player,
                        setup.to_dac
                    ));
                    for line in &setup.lines {
                        key.push_str(&format!(
                            "{SEP}{}{SEP}{}{SEP}{}{SEP}{}",
                            line.key, line.label, line.outcome, line.words
                        ));
                    }
                }
            }
        }
        key
    }
}

/// The card for the host `s` carries, on the device `s` describes; `None` while the host is not
/// available (switched off, or no session bus), when the page has no card at all.
pub(crate) fn playing(s: &Snapshot, family: Family) -> Option<Playing> {
    playing_of(s.host, s.access, Some((family, dac_facts(s))))
}

/// The card for `host`, for a viewer with `access`, on the device whose family and end of the
/// path `device` gives, or on none: a card with no path, which names the computer's sink.
pub(crate) fn playing_of(
    host: &HostView,
    access: Access,
    device: Option<(Family, DacFacts)>,
) -> Option<Playing> {
    if !host.available {
        return None;
    }
    let track = host.playing.as_ref().map(track);
    // A player's card has its path; so has a sound that plays with no MPRIS player to follow.
    let plays = track.is_some() || host.route.as_ref().is_some_and(plays_without_player);
    let path = match (plays, &host.route, &device) {
        // Only a DAC takes sound over the computer's audio: an amp or a preamp has no path.
        (true, Some(route), Some((Family::Dac, dac))) => {
            let check = route
                .sink
                .as_ref()
                .is_some_and(|sink| sink_is_dac(sink, dac))
                .then(|| setup_check(route));
            Some(path(&path_line(route, dac), check.as_ref()))
        }
        // The first read still in flight, or no `pw-dump`: the line says so, as the TUI's does.
        (_, None, Some((Family::Dac, _))) if track.is_some() => Some(Path {
            verdict: Verdict::Unknown.key(),
            end_to_end: EndToEnd::Unknown.key(),
            row: PathRow::not_read(),
            hops: Vec::new(),
            words: vec![ROUTE_NOT_READ.to_string()],
            title: String::new(),
            setup: None,
        }),
        _ => None,
    };
    Some(Playing {
        now_ms: host.now_ms,
        track,
        path,
        buttons: access.writes && !access.guest,
        sink_label: names_sink(host, device.as_ref()),
    })
}

/// Whether the visualizer should name the computer's sink: unless the sink is this page's DAC,
/// and never on a DAC page before its route is read (the sink may be that DAC, and a label that
/// then goes would blink once per page open).
fn names_sink(host: &HostView, device: Option<&(Family, DacFacts)>) -> bool {
    let Some((Family::Dac, dac)) = device else {
        return true;
    };
    match host.route.as_ref() {
        None => false,
        Some(route) => route
            .sink
            .as_ref()
            .is_none_or(|sink| !sink_is_dac(sink, dac)),
    }
}

/// The followed player's track as the card shows it.
fn track(view: &PlayingView) -> Track {
    let status = match view.status.as_str() {
        "playing" => "playing",
        "paused" => "paused",
        _ => "stopped",
    };
    let headline = match (view.title.trim(), status) {
        ("", "stopped") => "Stopped".to_string(),
        ("", _) => "Unknown track".to_string(),
        (title, _) => title.to_string(),
    };
    let album = view.album.trim().to_string();
    let source = [album.as_str(), view.player.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    Track {
        headline,
        artist: view.artist.trim().to_string(),
        album,
        source,
        status,
        // The hash goes into an address: only a plain run of letters and digits does.
        cover: view
            .cover
            .clone()
            .filter(|hash| !hash.is_empty() && hash.bytes().all(|b| b.is_ascii_alphanumeric())),
        length_us: view.length_us,
        progress: view.progress,
    }
}

/// What the device reports of its end of the path: its name, the input it reads and the sample
/// rate it reads, as far as the profile names them and the device has said.
fn dac_facts(s: &Snapshot) -> DacFacts {
    let Some(profile) = s.profile else {
        return DacFacts {
            display: s.info.name.clone(),
            ..DacFacts::default()
        };
    };
    let mut facts = DacFacts::read(profile, |id| s.raw(id));
    // The readings held while the link is down are from before the drop: the device's own stage
    // is not claimed from them.
    if !s.info.connected {
        facts.stage = DacStage::default();
    }
    facts
}

/// The config check as the card shows it.
fn setup_view(check: &SetupCheck) -> SetupView {
    SetupView {
        outcome: check.summary.key(),
        limit: SETUP_LIMIT,
        chip: check.chip(),
        summary: check.summary_words(),
        sink: check.sink.clone(),
        from_player: check.io.from_player.clone(),
        to_dac: check.io.to_dac.clone(),
        lines: check
            .lines
            .iter()
            .map(|line| SetupLineView {
                key: line.key.clone(),
                label: line.label(),
                outcome: line.outcome.key(),
                words: line.words.clone(),
            })
            .collect(),
    }
}

/// The path line as the card shows it, with the config check of its sink when there is one.
fn path(line: &PathLine, check: Option<&SetupCheck>) -> Path {
    Path {
        verdict: line.verdict.key(),
        end_to_end: line.end_to_end().key(),
        row: path_row(line, check),
        hops: line
            .hops
            .iter()
            .map(|hop| Hop {
                label: hop.label.clone(),
                detail: minus(&hop.detail),
                changes: hop.changes,
                unknown: hop.unknown,
            })
            .collect(),
        words: line.words.clone(),
        title: line.sentence().unwrap_or_default(),
        setup: check.map(setup_view),
    }
}

#[cfg(test)]
mod tests;
