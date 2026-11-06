// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The parts of a device's model: each view model's values, words, ranges and states as the JSON
//! a theme draws from, with no geometry (no coordinates, arcs or path strings). The volume's
//! marks are candidates before any spacing rule; the tone curve is its dB values.

use serde::Serialize;
use svolder_host::Progress;

use crate::web::view::{
    self, Bands, ControlView, DevicePresets, Facts, Knob, Meters, Playing, PowerView, Presets,
    Slider, Tone,
};

/// The link pill.
#[derive(Debug, Serialize)]
pub(crate) struct LinkModel {
    pub(crate) word: &'static str,
    /// `on`, `wait` or `off`.
    pub(crate) state: &'static str,
}

impl From<&view::LinkView> for LinkModel {
    fn from(link: &view::LinkView) -> Self {
        LinkModel {
            word: link.word,
            state: link.state,
        }
    }
}

/// A card above the view.
#[derive(Debug, Serialize)]
pub(crate) struct CardModel {
    /// `warn`, `fault` or `dim`.
    pub(crate) kind: &'static str,
    pub(crate) title: String,
    pub(crate) detail: String,
    /// The detail as text and code.
    pub(crate) spans: Vec<SpanModel>,
    pub(crate) faults: Vec<FaultModel>,
    pub(crate) retry_in: Option<u64>,
    pub(crate) try_now: bool,
}

/// A run of a card's words: text, or a command.
#[derive(Debug, Serialize)]
pub(crate) struct SpanModel {
    pub(crate) code: bool,
    pub(crate) text: String,
}

/// One active fault.
#[derive(Debug, Serialize)]
pub(crate) struct FaultModel {
    pub(crate) label: String,
    pub(crate) advice: String,
}

impl From<&view::Card> for CardModel {
    fn from(card: &view::Card) -> Self {
        CardModel {
            kind: card.kind,
            title: card.title.clone(),
            detail: card.detail.clone(),
            spans: card
                .spans()
                .into_iter()
                .map(|span| SpanModel {
                    code: span.code,
                    text: span.text,
                })
                .collect(),
            faults: card
                .faults
                .iter()
                .map(|fault| FaultModel {
                    label: fault.label.clone(),
                    advice: fault.advice.clone(),
                })
                .collect(),
            retry_in: card.retry_in,
            try_now: card.try_now,
        }
    }
}

/// The volume's knob.
#[derive(Debug, Serialize)]
pub(crate) struct VolumeModel {
    pub(crate) key: String,
    pub(crate) label: &'static str,
    pub(crate) lo: i64,
    pub(crate) hi: i64,
    pub(crate) raw: Option<i64>,
    /// The reading's words, `–` before one.
    pub(crate) text: String,
    pub(crate) target: Option<i64>,
    pub(crate) target_text: Option<String>,
    pub(crate) muted: bool,
    /// The mute toggle's key.
    pub(crate) mute: Option<String>,
    /// Whether the bottom step is silence.
    pub(crate) silence: bool,
    /// The first step the curve can clip from.
    pub(crate) risk: Option<i64>,
    /// The highest step `[web] max_volume_db` allows.
    pub(crate) cap: Option<i64>,
    pub(crate) unread: bool,
    /// The words of every step from `lo` to `hi`.
    pub(crate) texts: Vec<String>,
    /// The dB of every step: `null` for silence and for a control whose unit is not dB.
    pub(crate) db: Vec<Option<f64>>,
    /// The candidate labels: bottom, top, then −60, −40, −20 and −10 where strictly inside.
    pub(crate) marks: Vec<MarkModel>,
    pub(crate) readout: String,
    pub(crate) unit: Option<String>,
    pub(crate) sub: Option<String>,
    pub(crate) notes: Vec<NoteModel>,
    pub(crate) room: bool,
    pub(crate) aria: String,
}

/// A candidate label on the volume's scale.
#[derive(Debug, Serialize)]
pub(crate) struct MarkModel {
    /// The fractional step it stands at.
    pub(crate) at: f64,
    pub(crate) text: String,
}

/// A note under the knob.
#[derive(Debug, Serialize)]
pub(crate) struct NoteModel {
    pub(crate) text: String,
    pub(crate) red: bool,
    pub(crate) popup: bool,
    pub(crate) eq: bool,
}

impl From<&Knob> for VolumeModel {
    fn from(knob: &Knob) -> Self {
        let words_at = |raw: i64| {
            usize::try_from(raw - knob.lo)
                .ok()
                .and_then(|i| knob.words.get(i))
                .cloned()
        };
        VolumeModel {
            key: knob.key.clone(),
            label: "Volume",
            lo: knob.lo,
            hi: knob.hi,
            raw: knob.raw,
            text: knob
                .raw
                .and_then(words_at)
                .unwrap_or_else(|| "–".to_string()),
            target: knob.target,
            target_text: knob.target.and_then(words_at),
            muted: knob.muted,
            mute: knob.mute.clone(),
            silence: knob.silence,
            risk: knob.risk,
            cap: knob.cap,
            unread: knob.raw.is_none(),
            texts: knob.words.clone(),
            db: knob.levels.clone(),
            marks: knob
                .marks
                .iter()
                .map(|(at, text)| MarkModel {
                    at: *at,
                    text: text.clone(),
                })
                .collect(),
            readout: knob.readout.clone(),
            unit: knob.unit.clone(),
            sub: knob.sub.clone(),
            notes: knob
                .notes
                .iter()
                .map(|note| NoteModel {
                    text: note.text.clone(),
                    red: note.red,
                    popup: note.popup,
                    eq: note.eq,
                })
                .collect(),
            room: knob.room,
            aria: knob.aria.clone(),
        }
    }
}

/// One control, by its kind.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum ControlModel {
    /// A select.
    Choice {
        key: String,
        label: String,
        /// `segments` for up to five values, `list` for more.
        style: &'static str,
        options: Vec<OptionModel>,
    },
    /// A switch.
    Toggle {
        key: String,
        label: String,
        on: Option<bool>,
        /// What a press posts, as JSON.
        json: String,
    },
    /// A level other than the volume.
    Slider(SliderModel),
    /// A readout or a fault: its words and their tint.
    Readout {
        key: String,
        label: String,
        text: String,
        /// `good`, `faint` or empty.
        tone: &'static str,
    },
}

/// One value of a select.
#[derive(Debug, Serialize)]
pub(crate) struct OptionModel {
    pub(crate) label: String,
    pub(crate) on: bool,
    /// What a press posts, as JSON.
    pub(crate) json: String,
}

/// A level as a slider.
#[derive(Debug, Serialize)]
pub(crate) struct SliderModel {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) lo: i64,
    pub(crate) hi: i64,
    pub(crate) raw: Option<i64>,
    pub(crate) centre: Option<i64>,
    pub(crate) text: String,
    pub(crate) texts: Vec<String>,
    /// The reading's place along the track, as a percentage.
    pub(crate) at: Option<String>,
    /// Where the fill starts, as a percentage.
    pub(crate) from: String,
    pub(crate) ends: Option<(String, String)>,
}

impl From<&Slider> for SliderModel {
    fn from(slider: &Slider) -> Self {
        SliderModel {
            key: slider.key.clone(),
            label: slider.label.clone(),
            lo: slider.lo,
            hi: slider.hi,
            raw: slider.raw,
            centre: slider.centre,
            text: slider.text.clone(),
            texts: slider.words.clone(),
            at: slider.at.clone(),
            from: slider.from.clone(),
            ends: slider.ends.clone(),
        }
    }
}

impl From<&ControlView> for ControlModel {
    fn from(control: &ControlView) -> Self {
        let options = |options: &[view::Choice]| {
            options
                .iter()
                .map(|choice| OptionModel {
                    label: choice.label.clone(),
                    on: choice.on,
                    json: choice.json.clone(),
                })
                .collect()
        };
        match control {
            ControlView::Segments {
                key,
                label,
                options: choices,
            } => ControlModel::Choice {
                key: key.clone(),
                label: label.clone(),
                style: "segments",
                options: options(choices),
            },
            ControlView::List {
                key,
                label,
                options: choices,
            } => ControlModel::Choice {
                key: key.clone(),
                label: label.clone(),
                style: "list",
                options: options(choices),
            },
            ControlView::Switch {
                key,
                label,
                on,
                json,
            } => ControlModel::Toggle {
                key: key.clone(),
                label: label.clone(),
                on: *on,
                json: json.clone(),
            },
            ControlView::Slider(slider) => ControlModel::Slider(slider.into()),
            ControlView::Readout {
                key,
                label,
                text,
                tone,
            } => ControlModel::Readout {
                key: key.clone(),
                label: label.clone(),
                text: text.clone(),
                tone,
            },
        }
    }
}

/// An amp's meters.
#[derive(Debug, Serialize)]
pub(crate) struct MetersModel {
    /// Whether the meters read: there is no state line.
    pub(crate) live: bool,
    /// Why there are no readings.
    pub(crate) state: Option<&'static str>,
    pub(crate) meters: Vec<MeterModel>,
}

/// One meter.
#[derive(Debug, Serialize)]
pub(crate) struct MeterModel {
    pub(crate) key: String,
    pub(crate) channel: String,
    /// Its reading on its scale, 0 to 1.
    pub(crate) fraction: Option<f64>,
    pub(crate) text: String,
    /// The class-A block's share of the scale.
    pub(crate) class_a: f64,
}

impl From<&Meters> for MetersModel {
    fn from(meters: &Meters) -> Self {
        MetersModel {
            live: meters.state.is_none(),
            state: meters.state,
            meters: meters
                .arches
                .iter()
                .map(|arch| MeterModel {
                    key: arch.key.clone(),
                    channel: arch.channel.clone(),
                    fraction: arch.fraction,
                    text: arch.text.clone(),
                    class_a: arch.class_a,
                })
                .collect(),
        }
    }
}

/// The power button and, on an amp, its switches and readouts.
#[derive(Debug, Serialize)]
pub(crate) struct PowerModel {
    pub(crate) key: String,
    pub(crate) on: Option<bool>,
    pub(crate) json: String,
    pub(crate) word: &'static str,
    pub(crate) warming: bool,
    pub(crate) toggles: Vec<ControlModel>,
    pub(crate) readouts: Vec<ControlModel>,
}

impl From<&PowerView> for PowerModel {
    fn from(power: &PowerView) -> Self {
        PowerModel {
            key: power.key.clone(),
            on: power.on,
            json: power.json.clone(),
            word: power.word,
            warming: power.warming,
            toggles: power.toggles.iter().map(Into::into).collect(),
            readouts: power.readouts.iter().map(Into::into).collect(),
        }
    }
}

/// The tone control.
#[derive(Debug, Serialize)]
pub(crate) struct ToneModel {
    pub(crate) modes: Vec<ModeModel>,
    /// `flat`, `eq` or `loudness`; `null` before a reading.
    pub(crate) mode: Option<&'static str>,
    pub(crate) note: Option<&'static str>,
    pub(crate) loud: bool,
    pub(crate) strength: Option<String>,
    /// What the readings are.
    pub(crate) readings: ReadingsModel,
    /// The bands' curve in dB at 181 log-spaced frequencies from 20 Hz to 20 kHz.
    pub(crate) response: Option<Vec<f64>>,
    pub(crate) peak: Option<PeakModel>,
    pub(crate) markers: Vec<MarkerModel>,
    pub(crate) bands: Option<BandsModel>,
}

/// One value of the mode track.
#[derive(Debug, Serialize)]
pub(crate) struct ModeModel {
    pub(crate) name: &'static str,
    pub(crate) label: &'static str,
    pub(crate) on: bool,
}

/// The raw readings: each band's frequency, gain and Q steps, `null` where not read.
#[derive(Debug, Serialize)]
pub(crate) struct ReadingsModel {
    pub(crate) bands: Vec<[Option<i64>; 3]>,
    pub(crate) mode: Option<&'static str>,
}

/// The summed curve's peak.
#[derive(Debug, Serialize)]
pub(crate) struct PeakModel {
    pub(crate) db: f64,
    pub(crate) words: String,
}

/// One band's point: its frequency and the level it sits at.
#[derive(Debug, Serialize)]
pub(crate) struct MarkerModel {
    pub(crate) band: usize,
    pub(crate) hz: f64,
    pub(crate) db: f64,
}

/// The band table.
#[derive(Debug, Serialize)]
pub(crate) struct BandsModel {
    pub(crate) rows: Vec<BandRowModel>,
    /// Whether the rows are faint: the tone control is in Flat.
    pub(crate) faint: bool,
}

/// One band's row, in words.
#[derive(Debug, Serialize)]
pub(crate) struct BandRowModel {
    pub(crate) band: usize,
    pub(crate) f: String,
    pub(crate) g: String,
    pub(crate) q: String,
}

/// The tone control with its band table.
pub(super) fn tone(tone: &Tone, bands: Option<&Bands>) -> ToneModel {
    let curve = &tone.curve;
    ToneModel {
        modes: tone
            .modes
            .iter()
            .map(|mode| ModeModel {
                name: mode.name,
                label: mode.label,
                on: mode.on,
            })
            .collect(),
        mode: tone.mode,
        note: tone.note,
        loud: tone.loud,
        strength: tone.strength.clone(),
        readings: ReadingsModel {
            bands: curve.band_readings.clone(),
            mode: tone.mode,
        },
        response: curve.response.clone(),
        peak: curve
            .peak_db
            .zip(curve.peak.as_ref())
            .map(|(db, (_, words))| PeakModel {
                db,
                words: words.clone(),
            }),
        markers: curve
            .markers
            .iter()
            .map(|marker| MarkerModel {
                band: marker.band,
                hz: marker.hz,
                db: marker.db,
            })
            .collect(),
        bands: bands.map(|bands| BandsModel {
            rows: bands
                .rows
                .iter()
                .map(|row| BandRowModel {
                    band: row.band,
                    f: row.f.clone(),
                    g: row.g.clone(),
                    q: row.q.clone(),
                })
                .collect(),
            faint: bands.faint,
        }),
    }
}

/// The tone presets.
#[derive(Debug, Serialize)]
pub(crate) struct PresetsModel {
    pub(crate) status: StatusModel,
    pub(crate) rows: Vec<PresetRowModel>,
    pub(crate) editable: bool,
    pub(crate) recall: bool,
    pub(crate) note: Option<String>,
}

/// The status line: `lead`, the preset's name, `tail`.
#[derive(Debug, Serialize)]
pub(crate) struct StatusModel {
    pub(crate) lead: &'static str,
    pub(crate) name: Option<String>,
    pub(crate) tail: &'static str,
}

/// One preset's row.
#[derive(Debug, Serialize)]
pub(crate) struct PresetRowModel {
    pub(crate) name: String,
    pub(crate) built_in: bool,
    pub(crate) on: bool,
    /// What its recall and its delete post.
    pub(crate) json: String,
    /// What its rename posts beside the new name.
    pub(crate) rename_json: String,
}

impl From<&Presets> for PresetsModel {
    fn from(presets: &Presets) -> Self {
        PresetsModel {
            status: StatusModel {
                lead: presets.status.lead,
                name: presets.status.name.clone(),
                tail: presets.status.tail,
            },
            rows: presets
                .rows
                .iter()
                .map(|row| PresetRowModel {
                    name: row.name.clone(),
                    built_in: row.built_in,
                    on: row.on,
                    json: row.json.clone(),
                    rename_json: row.rename_json.clone(),
                })
                .collect(),
            editable: presets.editable,
            recall: presets.recall,
            note: presets.note.clone(),
        }
    }
}

/// A device's own presets.
#[derive(Debug, Serialize)]
pub(crate) struct DevicePresetsModel {
    pub(crate) slots: Vec<SlotModel>,
    pub(crate) builtins: Vec<SettingModel>,
    pub(crate) recall: bool,
    pub(crate) save: bool,
}

/// One slot.
#[derive(Debug, Serialize)]
pub(crate) struct SlotModel {
    pub(crate) name: String,
    pub(crate) slot: u8,
    pub(crate) recall_json: String,
    pub(crate) save_json: String,
}

/// One built-in setting.
#[derive(Debug, Serialize)]
pub(crate) struct SettingModel {
    pub(crate) name: String,
    pub(crate) on: bool,
    pub(crate) json: String,
}

impl From<&DevicePresets> for DevicePresetsModel {
    fn from(presets: &DevicePresets) -> Self {
        DevicePresetsModel {
            slots: presets
                .slots
                .iter()
                .map(|slot| SlotModel {
                    name: slot.name.clone(),
                    slot: slot.slot,
                    recall_json: slot.recall_json.clone(),
                    save_json: slot.save_json.clone(),
                })
                .collect(),
            builtins: presets
                .builtins
                .iter()
                .map(|setting| SettingModel {
                    name: setting.name.clone(),
                    on: setting.on,
                    json: setting.json.clone(),
                })
                .collect(),
            recall: presets.recall,
            save: presets.save,
        }
    }
}

/// The Device fold-out.
#[derive(Debug, Serialize)]
pub(crate) struct FactsModel {
    pub(crate) product: Option<String>,
    pub(crate) product_id: u16,
    /// The unit id; absent for a guest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) unit: Option<String>,
    pub(crate) firmware: String,
    pub(crate) dsp: Option<u16>,
    pub(crate) profile: String,
    pub(crate) link: String,
    pub(crate) signal: String,
    pub(crate) signal_age: Option<SignalAgeModel>,
    pub(crate) passphrase_note: Option<String>,
    pub(crate) rows: Vec<FactRowModel>,
    pub(crate) now_ms: u64,
    pub(crate) identify: bool,
    pub(crate) identify_offered: bool,
    pub(crate) identify_gap_secs: u64,
}

/// What ages the signal in place.
#[derive(Debug, Serialize)]
pub(crate) struct SignalAgeModel {
    pub(crate) head: String,
    pub(crate) heard_at_ms: u64,
    pub(crate) live: bool,
}

/// One reading in the All readings table.
#[derive(Debug, Serialize)]
pub(crate) struct FactRowModel {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) heard_at_ms: Option<u64>,
}

impl From<&Facts> for FactsModel {
    fn from(facts: &Facts) -> Self {
        FactsModel {
            product: facts.product.clone(),
            product_id: facts.product_id,
            unit: Some(facts.unit.clone()).filter(|unit| !unit.is_empty()),
            firmware: facts.firmware.clone(),
            dsp: facts.dsp,
            profile: facts.profile.clone(),
            link: facts.link.clone(),
            signal: facts.signal.clone(),
            signal_age: facts.signal_age.as_ref().map(|age| SignalAgeModel {
                head: age.head.clone(),
                heard_at_ms: age.heard_at_ms,
                live: age.live,
            }),
            passphrase_note: facts.passphrase_note.clone(),
            rows: facts
                .rows
                .iter()
                .map(|row| FactRowModel {
                    id: row.id.clone(),
                    label: row.label.clone(),
                    value: row.value.clone(),
                    heard_at_ms: row.heard_at_ms,
                })
                .collect(),
            now_ms: facts.now_ms,
            identify: facts.identify,
            identify_offered: facts.identify_offered,
            identify_gap_secs: facts.identify_gap_secs,
        }
    }
}

/// The Now playing card.
#[derive(Debug, Serialize)]
pub(crate) struct PlayingModel {
    pub(crate) now_ms: u64,
    pub(crate) sink_label: bool,
    pub(crate) buttons: bool,
    pub(crate) track: Option<TrackModel>,
    pub(crate) path: Option<PathModel>,
}

/// The followed player's track.
#[derive(Debug, Serialize)]
pub(crate) struct TrackModel {
    pub(crate) headline: String,
    pub(crate) artist: String,
    pub(crate) album: String,
    pub(crate) source: String,
    /// `playing`, `paused` or `stopped`.
    pub(crate) status: &'static str,
    pub(crate) cover: Option<String>,
    pub(crate) length_us: Option<u64>,
    pub(crate) progress: Option<Progress>,
}

/// The path the sound takes to the device.
#[derive(Debug, Serialize)]
pub(crate) struct PathModel {
    /// `ok`, `changed`, `other` or `unknown`.
    pub(crate) class: &'static str,
    pub(crate) words: String,
    pub(crate) hops: Vec<HopModel>,
}

/// One hop of the path.
#[derive(Debug, Serialize)]
pub(crate) struct HopModel {
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) changes: bool,
    pub(crate) unknown: bool,
}

impl From<&Playing> for PlayingModel {
    fn from(playing: &Playing) -> Self {
        PlayingModel {
            now_ms: playing.now_ms,
            sink_label: playing.sink_label,
            buttons: playing.buttons,
            track: playing.track.as_ref().map(|track| TrackModel {
                headline: track.headline.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                source: track.source.clone(),
                status: track.status,
                cover: track.cover.clone(),
                length_us: track.length_us,
                progress: track.progress,
            }),
            path: playing.path.as_ref().map(|path| PathModel {
                class: path.class,
                words: path.words.clone(),
                hops: path
                    .hops
                    .iter()
                    .map(|hop| HopModel {
                        label: hop.label.clone(),
                        detail: hop.detail.clone(),
                        changes: hop.changes,
                        unknown: hop.unknown,
                    })
                    .collect(),
            }),
        }
    }
}
