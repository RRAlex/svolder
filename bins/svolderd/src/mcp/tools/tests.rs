// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use svolder_ipc::Config;
use svolder_profiles::Registry;

use super::*;
use crate::device::Command;
use crate::fleet::Unavailable;
use crate::recall::{Applied, Recalled};
use crate::writer::{Options, Outcome};

/// What runs with the key of a read.
type ReadHook = Box<dyn Fn(&str) + Send>;

/// A one-device fleet over a built-in profile whose controls read what `readings` says (by
/// key, as raw steps) and which records every `set` it is sent.
struct Scripted {
    readings: StdMutex<HashMap<String, i64>>,
    sets: StdMutex<Vec<(String, String)>>,
    /// The `(key, raw)` of every glide `glide` was sent.
    glides: StdMutex<Vec<(String, i64)>>,
    /// Whether a climb stops where the reading is, as one another front end cut short.
    stuck: StdMutex<bool>,
    /// Whether a read never answers once a climb has begun, as a daemon too busy to.
    deaf: StdMutex<bool>,
    /// Whether a glide has been sent while `deaf` is set.
    gone_deaf: StdMutex<bool>,
    /// The names of the presets `recall` was asked for.
    recalls: StdMutex<Vec<String>>,
    /// The words every read is answered with as an error, when set.
    failing_reads: StdMutex<Option<String>>,
    /// The names of the built-in settings `apply` was asked for.
    applied: StdMutex<Vec<String>>,
    /// Every command `submit` was sent, as the stand-ins log one.
    submitted: StdMutex<Vec<String>>,
    /// The device's memory, as its row reports it.
    recalled: StdMutex<Option<String>>,
    /// The tone control's readings, which `eq_show` answers.
    tone: StdMutex<Vec<ValueObject>>,
    /// The user's presets, which `config` answers.
    presets: StdMutex<Vec<Preset>>,
    /// Every reading the device holds, which `state` answers.
    state: StdMutex<Vec<ValueObject>>,
    /// The signal the device's row carries.
    signal: StdMutex<Option<svolder_ipc::Signal>>,
    /// What an identify is answered with, and how many were sent.
    identify: StdMutex<Option<Response>>,
    identifies: StdMutex<usize>,
    profile: Profile,
    /// The events `events` hands out, once.
    events: StdMutex<Option<tokio::sync::mpsc::Receiver<Event>>>,
    /// Run with the key of every read the fleet answers, as another front end acting between two
    /// reads of a tool.
    on_read: StdMutex<Option<ReadHook>>,
}

fn scripted(profile: &str, readings: &[(&str, i64)]) -> Arc<Scripted> {
    let registry = Registry::builtin().expect("the built-in profiles");
    scripted_over(registry.get(profile).expect("a built-in").clone(), readings)
}

/// [`scripted`] over a profile of the test's own.
fn scripted_over(profile: Profile, readings: &[(&str, i64)]) -> Arc<Scripted> {
    Arc::new(Scripted {
        readings: StdMutex::new(readings.iter().map(|(k, v)| (k.to_string(), *v)).collect()),
        sets: StdMutex::new(Vec::new()),
        glides: StdMutex::new(Vec::new()),
        stuck: StdMutex::new(false),
        deaf: StdMutex::new(false),
        gone_deaf: StdMutex::new(false),
        recalls: StdMutex::new(Vec::new()),
        failing_reads: StdMutex::new(None),
        applied: StdMutex::new(Vec::new()),
        submitted: StdMutex::new(Vec::new()),
        recalled: StdMutex::new(None),
        tone: StdMutex::new(Vec::new()),
        presets: StdMutex::new(Vec::new()),
        state: StdMutex::new(Vec::new()),
        signal: StdMutex::new(None),
        identify: StdMutex::new(None),
        identifies: StdMutex::new(0),
        profile,
        events: StdMutex::new(None),
        on_read: StdMutex::new(None),
    })
}

impl Scripted {
    /// The scripted reading of `key`, after the read hook has run.
    fn read_hooked(&self, key: &str) -> Option<i64> {
        if let Some(hook) = &*self.on_read.lock().unwrap() {
            hook(key);
        }
        self.readings.lock().unwrap().get(key).copied()
    }
}

#[async_trait]
impl Fleet for Scripted {
    fn resolve(&self, _selector: Option<&str>) -> Result<DeviceInfo, String> {
        Ok(DeviceInfo {
            signal: *self.signal.lock().unwrap(),
            name: "amp".to_string(),
            unit: "0102030405060708".to_string(),
            product_id: 2,
            product: Some("Amp".to_string()),
            firmware: 3,
            profile: self.profile.name.clone(),
            connected: true,
            stale: false,
            retry_in_secs: None,
            passphrase_note: None,
            refused: false,
            recalled: self.recalled.lock().unwrap().clone(),
        })
    }

    async fn request(&self, request: Request) -> Response {
        if *self.gone_deaf.lock().unwrap() {
            std::future::pending::<()>().await;
        }
        match request {
            Request::Get { .. } if self.failing_reads.lock().unwrap().is_some() => {
                Response::Error {
                    code: 3,
                    message: self.failing_reads.lock().unwrap().clone().unwrap(),
                }
            }
            Request::Get { key, .. } => match self.read_hooked(&key) {
                Some(raw) => Response::Value(reading(&key, raw)),
                None => Response::Error {
                    code: 4,
                    message: format!("no value reported yet for {key}"),
                },
            },
            Request::Set { key, value, .. } => {
                self.sets.lock().unwrap().push((key.clone(), value.clone()));
                Response::Value(reading(&key, value.parse().unwrap_or(0)))
            }
            Request::State { .. } => Response::Values {
                device: "amp".into(),
                values: self.state.lock().unwrap().clone(),
            },
            Request::EqShow { .. } => Response::Values {
                device: "amp".into(),
                values: self.tone.lock().unwrap().clone(),
            },
            Request::Devices => Response::Devices {
                devices: vec![self.resolve(None).expect("the one device")],
            },
            Request::Identify { .. } => {
                *self.identifies.lock().unwrap() += 1;
                self.identify
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or(Response::Identify {
                        device: "amp".into(),
                        sent: true,
                    })
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    /// A glide, judged as the device's writer judges it ([`safety::judge`] by
    /// [`safety::Rules::writer`]) on the scripted reading: one that may land at once answers with
    /// its echo, as a set does, and one that climbs moves the reading to its target, unless
    /// `stuck` holds it where it is, as a climb another front end cut short. Every glide is
    /// recorded among the sets too, so a test counting writes counts it.
    async fn glide(&self, _device: &DeviceInfo, key: &str, raw: i64) -> Glided {
        self.glides.lock().unwrap().push((key.to_string(), raw));
        *self.gone_deaf.lock().unwrap() = *self.deaf.lock().unwrap();
        self.sets
            .lock()
            .unwrap()
            .push((key.to_string(), raw.to_string()));
        let now = self.readings.lock().unwrap().get(key).copied();
        let control = self.profile.control_by_key(key).expect("a control");
        let situation = safety::Situation {
            reading: now,
            base: now,
            floor_db: None,
            asked_db: None,
        };
        match safety::judge(control, &situation, raw, &safety::Rules::writer()).motion {
            safety::Motion::Glide { target, .. } => {
                if !*self.stuck.lock().unwrap() {
                    self.readings
                        .lock()
                        .unwrap()
                        .insert(key.to_string(), target);
                }
                Glided::Climbing {
                    first: None,
                    target,
                }
            }
            _ => Glided::Landed(Response::Value(reading(key, raw))),
        }
    }

    async fn recall(
        &self,
        device: &DeviceInfo,
        name: &str,
        fitting: Vec<Preset>,
    ) -> Result<(Recalled, Response), String> {
        self.recalls.lock().unwrap().push(name.to_string());
        crate::recall::plan(&device.name, &self.profile, &fitting, name, |_| None)
            .map(|recalled| (recalled, Response::Ack))
    }

    /// A built-in setting, planned from the scripted readings as the writer plans one; the
    /// readings take its values.
    async fn apply(&self, device: &DeviceInfo, name: &str) -> Result<(Applied, Response), String> {
        self.applied.lock().unwrap().push(name.to_string());
        let readings = self.readings.lock().unwrap().clone();
        let applied = crate::recall::plan_setting(&device.name, &self.profile, name, |id| {
            let control = self.profile.control_by_id(id)?;
            readings.get(&control.key).copied()
        })?;
        for write in &applied.writes {
            self.readings
                .lock()
                .unwrap()
                .insert(write.key.clone(), write.raw);
        }
        Ok((applied, Response::Ack))
    }

    async fn submit(&self, _device: &DeviceInfo, command: Command, _options: Options) -> Outcome {
        self.submitted
            .lock()
            .unwrap()
            .push(crate::test_support::shown(&command));
        Outcome::Done(Response::Ack)
    }

    fn profile(&self, _device: &DeviceInfo) -> Option<Profile> {
        Some(self.profile.clone())
    }

    async fn events(&self) -> BoxStream<'static, Event> {
        match self.events.lock().unwrap().take() {
            Some(rx) => tokio_stream::wrappers::ReceiverStream::new(rx).boxed(),
            None => futures::stream::empty().boxed(),
        }
    }

    fn config(&self) -> Result<Config, String> {
        Ok(Config {
            presets: self.presets.lock().unwrap().clone(),
            ..Config::default()
        })
    }
}

fn reading(key: &str, raw: i64) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: "10".to_string(),
        key: Some(key.to_string()),
        label: key.to_string(),
        value_type: "u8".to_string(),
        raw: json!(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    }
}

fn call(tool: &str, arguments: Value) -> CallToolRequestParams {
    let mut call = CallToolRequestParams::new(tool.to_string());
    call.arguments = arguments.as_object().cloned();
    call
}

fn set(key: &str, value: Value, confirm: bool) -> CallToolRequestParams {
    call(
        "set_control",
        json!({"key": key, "value": value, "confirm": confirm}),
    )
}

async fn run(
    fleet: &Arc<Scripted>,
    cap: Option<f64>,
    request: CallToolRequestParams,
    can_ask: bool,
) -> CallToolResponse {
    Tools::new(Arc::clone(fleet) as Arc<dyn Fleet>, true, cap)
        .call(request, Caller { can_ask })
        .await
        .expect("an answer")
}

/// A fresh server over `fleet`, to run several calls through one memory of levels.
fn server(fleet: &Arc<Scripted>, cap: Option<f64>) -> Tools {
    Tools::new(Arc::clone(fleet) as Arc<dyn Fleet>, true, cap)
}

fn is_error(response: &CallToolResponse) -> bool {
    matches!(response, CallToolResponse::Complete(r) if r.is_error == Some(true))
}

fn asks(response: &CallToolResponse) -> bool {
    matches!(response, CallToolResponse::InputRequired(_))
}

fn went(response: &CallToolResponse) -> bool {
    matches!(response, CallToolResponse::Complete(r) if r.is_error == Some(false))
}

fn error_words(response: &CallToolResponse) -> String {
    match response {
        CallToolResponse::Complete(r) => format!("{:?}", r.content),
        other => format!("{other:?}"),
    }
}

fn db(profile: &Profile, raw: i64) -> f64 {
    convert::raw_to_display(profile.control_by_key("volume").unwrap(), raw).unwrap()
}

/// The raw step whose level is the highest one at most `rise` dB above step `from`, or, with
/// `more`, the lowest one above that.
fn step_above(profile: &Profile, from: i64, rise: f64, more: bool) -> i64 {
    let base = db(profile, from);
    let candidates = (0..=127).filter(|r| {
        let d = db(profile, *r) - base;
        if more { d > rise } else { d <= rise && d > 0.0 }
    });
    if more {
        candidates.min_by(|a, b| db(profile, *a).total_cmp(&db(profile, *b)))
    } else {
        candidates.max_by(|a, b| db(profile, *a).total_cmp(&db(profile, *b)))
    }
    .expect("a step")
}

#[tokio::test]
async fn the_largest_rise_that_is_six_db_or_less_goes_through_and_the_next_asks() {
    let fleet = scripted("gungnir2-v2", &[("volume", 60)]);
    let profile = fleet.profile.clone();
    let within = step_above(&profile, 60, MAX_RISE_DB, false);
    let beyond = step_above(&profile, 60, MAX_RISE_DB, true);

    let answer = run(
        &fleet,
        None,
        set("volume", json!(db(&profile, within)), false),
        true,
    )
    .await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(fleet.sets.lock().unwrap().len(), 1);

    let answer = run(
        &fleet,
        None,
        set("volume", json!(db(&profile, beyond)), false),
        true,
    )
    .await;
    assert!(asks(&answer));
    assert_eq!(fleet.sets.lock().unwrap().len(), 1, "asking sets nothing");
}

#[tokio::test]
async fn a_level_that_cannot_be_read_or_is_silent_counts_as_the_largest_rise() {
    for current in [None, Some(0)] {
        let readings: Vec<(&str, i64)> = current.map(|c| ("volume", c)).into_iter().collect();
        let fleet = scripted("gungnir2-v2", &readings);
        let answer = run(&fleet, None, set("volume", json!(-60), false), false).await;
        assert!(is_error(&answer), "{current:?}: {answer:?}");
        assert!(fleet.sets.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn setting_the_same_level_or_lower_never_asks() {
    let fleet = scripted("gungnir2-v2", &[("volume", 100)]);
    let profile = fleet.profile.clone();
    for raw in [100, 99, 50, 1] {
        let answer = run(
            &fleet,
            None,
            set("volume", json!(db(&profile, raw)), false),
            false,
        )
        .await;
        assert!(went(&answer), "{raw}: {answer:?}");
    }
    // Silence has no number; a mute is how it is asked for.
    let answer = run(&fleet, None, set("volume", json!(-200), false), false).await;
    assert!(is_error(&answer) && error_words(&answer).contains("between"));
}

#[tokio::test]
async fn small_steps_add_up_to_one_rise_within_the_minute() {
    let fleet = scripted("gungnir2-v2", &[("volume", 20)]);
    let profile = fleet.profile.clone();
    let tools = server(&fleet, None);
    let mut raw = 20;
    let mut asked_at = None;
    for _ in 0..30 {
        let next = step_above(&profile, raw, 3.0, false);
        fleet.readings.lock().unwrap().insert("volume".into(), raw);
        let answer = tools
            .call(
                set("volume", json!(db(&profile, next)), false),
                Caller { can_ask: true },
            )
            .await
            .unwrap();
        if asks(&answer) {
            asked_at = Some(db(&profile, next) - db(&profile, 20));
            break;
        }
        assert!(went(&answer), "{answer:?}");
        raw = next;
    }
    let rise = asked_at.expect("a staircase asks");
    assert!(
        rise > MAX_RISE_DB && rise < MAX_RISE_DB + 3.5,
        "asked at {rise} dB"
    );
}

#[tokio::test(start_paused = true)]
async fn the_floor_is_the_lowest_level_of_the_last_minute() {
    let floors = Floors::default();
    floors.record("amp", "volume", -50.0);
    floors.record("amp", "volume", -40.0);
    assert_eq!(floors.floor("AMP", "Volume"), Some(-50.0));
    tokio::time::advance(Duration::from_secs(59)).await;
    assert_eq!(floors.floor("amp", "volume"), Some(-50.0));
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(floors.floor("amp", "volume"), None);
    floors.record("amp", "volume", -30.0);
    floors.reset("amp", "volume", -10.0);
    assert_eq!(floors.floor("amp", "volume"), Some(-10.0));
    assert_eq!(floors.floor("amp", "balance"), None);
}

#[tokio::test]
async fn a_lowering_is_allowed_above_the_cap_and_a_rise_to_it_is_not() {
    let fleet = scripted("gungnir2-v2", &[("volume", 127)]);
    let profile = fleet.profile.clone();
    // 0 dB with a cap of -20: a smaller drop that still ends above the cap goes through.
    let answer = run(&fleet, Some(-20.0), set("volume", json!(-3), false), false).await;
    assert!(went(&answer), "{answer:?}");
    // A rise that ends above it does not, confirmed or not.
    fleet.readings.lock().unwrap().insert("volume".into(), 10);
    for confirm in [false, true] {
        let answer = run(
            &fleet,
            Some(-20.0),
            set("volume", json!(-10), confirm),
            false,
        )
        .await;
        assert!(
            is_error(&answer)
                && error_words(&answer).contains(
                    "raising the volume to -10.0 dB is above this server's limit of -20.0 dB \
                     (max_volume_db); choose a level at or below it"
                ),
            "{answer:?}"
        );
    }
    let _ = profile;
}

#[tokio::test]
async fn a_step_that_rounds_above_the_cap_is_refused() {
    let fleet = scripted("gungnir2-v2", &[("volume", 10)]);
    let profile = fleet.profile.clone();
    let cap = (50..127)
        .map(|r| db(&profile, r))
        .find(|d| {
            let volume = profile.control_by_key("volume").unwrap();
            let raw = convert::display_to_raw(volume, *d - 0.01).unwrap();
            db(&profile, raw) > *d - 0.01
        })
        .expect("a cap between steps");
    let at = cap - 0.01;
    let answer = run(&fleet, Some(at), set("volume", json!(at), true), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(fleet.sets.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_client_that_can_ask_is_asked_whatever_the_model_confirms() {
    let fleet = scripted("gungnir2-v2", &[("volume", 20)]);
    let answer = run(&fleet, None, set("volume", json!(-5), true), true).await;
    assert!(asks(&answer), "{answer:?}");
    assert!(fleet.sets.lock().unwrap().is_empty());
    let answer = run(&fleet, None, set("volume", json!(-5), true), false).await;
    assert!(went(&answer), "{answer:?}");
}

#[tokio::test]
async fn a_declined_answer_is_final_and_an_accepted_one_must_match_the_question() {
    let fleet = scripted("gungnir2-v2", &[("volume", 20)]);
    let asked = run(&fleet, None, set("volume", json!(-5), false), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    let state = asked.request_state.clone().unwrap();
    let retry = |action: Value, state: &str| {
        let mut request = set("volume", json!(-5), false);
        request.request_state = Some(state.to_string());
        request.input_responses = Some([(ASK.to_string(), action)].into_iter().collect());
        request
    };
    let declined = run(
        &fleet,
        None,
        retry(json!({"action": "decline"}), &state),
        true,
    )
    .await;
    assert!(is_error(&declined) && error_words(&declined).contains("declined"));
    let cancelled = run(
        &fleet,
        None,
        retry(json!({"action": "cancel"}), &state),
        true,
    )
    .await;
    assert!(is_error(&cancelled));
    let yes = json!({"action": "accept", "content": {"confirm": true}});
    let stale = run(
        &fleet,
        None,
        retry(yes.clone(), "level|amp|volume|1|none"),
        true,
    )
    .await;
    assert!(asks(&stale), "a state that is not the question's");
    assert!(fleet.sets.lock().unwrap().is_empty());
    let accepted = run(&fleet, None, retry(yes, &state), true).await;
    assert!(went(&accepted), "{accepted:?}");
}

#[tokio::test]
async fn a_band_in_db_follows_the_same_rule_with_its_own_window() {
    let fleet = scripted("lokimax", &[("band1", 3), ("band2", 15)]);
    // band1: -12 + 3*0.8 = -9.6 dB; +24 dB steps up is far more than 6.
    let answer = run(&fleet, None, set("band1", json!(12), false), true).await;
    assert!(asks(&answer), "{answer:?}");
    let answer = run(&fleet, None, set("band1", json!(-5), false), true).await;
    assert!(went(&answer), "{answer:?}");
    // Out of the band's own range is refused, not clamped.
    let answer = run(&fleet, None, set("band2", json!(12), false), false).await;
    assert!(
        is_error(&answer) && error_words(&answer).contains("between -9"),
        "{answer:?}"
    );
    // The cap is the volume's alone.
    let answer = run(&fleet, Some(-40.0), set("band2", json!(5), true), false).await;
    assert!(went(&answer), "{answer:?}");
}

#[tokio::test]
async fn a_gain_select_asks_when_it_goes_up_and_not_when_it_goes_down() {
    let fleet = scripted("hati", &[("gain", 2)]);
    let up = run(&fleet, None, set("gain", json!("70 dB"), false), true).await;
    assert!(asks(&up), "{up:?}");
    let up_old = run(&fleet, None, set("gain", json!("70 dB"), false), false).await;
    assert!(is_error(&up_old) && error_words(&up_old).contains("confirm"));
    let down = run(&fleet, None, set("gain", json!("40 dB"), false), true).await;
    assert!(went(&down), "{down:?}");
    let same = run(&fleet, None, set("gain", json!("50 dB"), false), true).await;
    assert!(went(&same), "{same:?}");
    // An unreadable setting may be a rise.
    let fleet = scripted("hati", &[]);
    let unread = run(&fleet, None, set("gain", json!("40 dB"), false), true).await;
    assert!(asks(&unread));
}

#[tokio::test]
async fn an_unmute_asks_and_is_refused_above_the_cap() {
    let fleet = scripted("gungnir2-v2", &[("volume", 100), ("mute", 1)]);
    let profile = fleet.profile.clone();
    let level = db(&profile, 100);
    let answer = run(&fleet, None, call("mute", json!({"on": false})), true).await;
    assert!(asks(&answer), "{answer:?}");
    let old = run(&fleet, None, call("mute", json!({"on": false})), false).await;
    assert!(is_error(&old));
    let old = run(
        &fleet,
        None,
        call("mute", json!({"on": false, "confirm": true})),
        false,
    )
    .await;
    assert!(went(&old), "{old:?}");
    // Through set_control too.
    let answer = run(&fleet, None, set("mute", json!(false), false), true).await;
    assert!(asks(&answer));
    // Muting never asks.
    let answer = run(&fleet, None, call("mute", json!({"on": true})), true).await;
    assert!(went(&answer), "{answer:?}");
    // Above the cap: refused whatever is confirmed.
    let capped = Some(level - 5.0);
    for can_ask in [true, false] {
        let answer = run(
            &fleet,
            capped,
            call("mute", json!({"on": false, "confirm": true})),
            can_ask,
        )
        .await;
        assert!(
            is_error(&answer)
                && error_words(&answer).contains(&format!(
                    "unmuting amp would play at {:.1} dB, above this server's limit of {:.1} dB \
                     (max_volume_db); lower the volume first",
                    level,
                    level - 5.0
                )),
            "{answer:?}"
        );
    }
    // A volume that cannot be read asks.
    let fleet = scripted("gungnir2-v2", &[]);
    let answer = run(&fleet, None, call("mute", json!({"on": false})), true).await;
    assert!(asks(&answer));
}

#[tokio::test]
async fn mute_finds_the_mute_of_a_lyr_5() {
    let fleet = scripted("lyr5", &[("volume", 10)]);
    let answer = run(&fleet, None, call("mute", json!({"on": true})), true).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(fleet.sets.lock().unwrap()[0].0, "headphone_mute");
}

#[tokio::test]
async fn a_level_with_a_unit_in_text_is_read_in_that_unit_and_refused_outside_its_range() {
    let volume = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .control_by_key("volume")
        .unwrap()
        .clone();
    assert_eq!(
        units::number_in("-20 dB", volume.unit.as_deref()),
        Some(-20.0)
    );
    assert_eq!(
        units::number_in(" -20DB", volume.unit.as_deref()),
        Some(-20.0)
    );
    assert_eq!(units::number_in("-20", volume.unit.as_deref()), Some(-20.0));
    assert_eq!(units::number_in("loud", volume.unit.as_deref()), None);
    assert!(level_raw(&volume, &json!("max")).is_err());
    assert!(level_raw(&volume, &json!(f64::NAN)).is_err());
    assert!(level_raw(&volume, &json!(true)).is_err());
    assert_eq!(level_raw(&volume, &json!(0)).unwrap(), 127);
    for out in [json!(3), json!(1e308), json!(-1e308), json!(-97)] {
        let refused = level_raw(&volume, &out).unwrap_err();
        assert!(refused.contains("between -96.33 and 0"), "{out}: {refused}");
    }
}

#[test]
fn the_write_tools_are_listed_only_with_writes() {
    let fleet: Arc<dyn Fleet> = Arc::new(Unavailable);
    let names = |writes| -> Vec<String> {
        Tools::new(Arc::clone(&fleet), writes, None)
            .list()
            .iter()
            .map(|t| t.name.to_string())
            .collect()
    };
    assert_eq!(names(false).len(), 4);
    assert_eq!(names(true).len(), 8);
    for tool in Tools::new(fleet, true, None).list() {
        let description = tool.description.expect("a description");
        assert!(description.ends_with('.'), "{description}");
    }
}

#[tokio::test]
async fn an_unmute_on_a_device_with_no_volume_asks_what_it_lets_through() {
    for profile in ["gungnir2-v1", "hati", "gjallarhorn"] {
        let fleet = scripted(profile, &[]);
        assert!(
            fleet
                .profile
                .control_by_key("mute")
                .is_some_and(|c| c.mutes_output),
            "{profile}"
        );
        assert!(
            fleet
                .profile
                .control_by_key("volume")
                .is_none_or(|c| !svolder_profiles::safety::in_db(c)),
            "{profile}"
        );
        let asked = run(&fleet, None, call("mute", json!({"on": false})), true).await;
        let CallToolResponse::InputRequired(asked) = asked else {
            panic!("{profile}: {asked:?}")
        };
        let text = format!("{:?}", asked.input_requests);
        assert!(text.contains("full output"), "{text}");
        let old = run(&fleet, None, call("mute", json!({"on": false})), false).await;
        assert!(
            is_error(&old) && error_words(&old).contains("confirm"),
            "{profile}"
        );
        let old = run(
            &fleet,
            None,
            call("mute", json!({"on": false, "confirm": true})),
            false,
        )
        .await;
        assert!(went(&old), "{profile}: {old:?}");
        // Muting never asks.
        let muted = run(&fleet, None, call("mute", json!({"on": true})), true).await;
        assert!(went(&muted), "{profile}");
    }
}

#[tokio::test]
async fn a_control_that_asks_on_change_asks_whatever_the_value() {
    let fleet = scripted("lyr5", &[("preout", 0)]);
    for value in [json!(true), json!(false)] {
        let answer = run(&fleet, None, set("preout", value, false), true).await;
        assert!(asks(&answer), "{answer:?}");
    }
    let old = run(&fleet, None, set("preout", json!(true), true), false).await;
    assert!(went(&old), "{old:?}");
    let fleet = scripted("lokimax", &[("mode", 1)]);
    let answer = run(&fleet, None, set("mode", json!("Bypass"), false), true).await;
    assert!(asks(&answer), "{answer:?}");
}

#[tokio::test]
async fn a_devices_own_reading_lowers_the_floor() {
    let fleet = scripted("gungnir2-v2", &[("volume", 60)]);
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    *fleet.events.lock().unwrap() = Some(rx);
    let tools = server(&fleet, None);
    let stop = CancellationToken::new();
    let watching = tools.watch(stop.clone());
    let mut low = reading("volume", 10);
    low.unit = Some("dB".to_string());
    low.value = Some(-70.0);
    tx.send(Event::Status {
        device: "amp".to_string(),
        reading: low,
    })
    .await
    .unwrap();
    for _ in 0..200 {
        if tools.floors.floor("amp", "volume") == Some(-70.0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(tools.floors.floor("amp", "volume"), Some(-70.0));
    // The device is now at raw 60, far above that floor: a small step asks.
    let profile = fleet.profile.clone();
    let next = step_above(&profile, 60, 2.0, false);
    let answer = tools
        .call(
            set("volume", json!(db(&profile, next)), false),
            Caller { can_ask: true },
        )
        .await
        .unwrap();
    assert!(asks(&answer), "{answer:?}");
    stop.cancel();
    watching.await.unwrap();
}

fn power(on: bool) -> CallToolRequestParams {
    set("power", json!(on), false)
}

#[tokio::test]
async fn powering_on_a_device_with_no_volume_asks_what_it_lets_through() {
    // A standby amplifier (two of the five), Tyr, and the phono stages.
    for profile in ["vidar2", "kraken", "tyr", "hati", "stjarna"] {
        let fleet = scripted(profile, &[]);
        assert!(
            fleet
                .profile
                .control_by_key("power")
                .is_some_and(|c| c.powers_output),
            "{profile}"
        );
        let asked = run(&fleet, None, power(true), true).await;
        let CallToolResponse::InputRequired(asked) = asked else {
            panic!("{profile}: {asked:?}")
        };
        let text = format!("{:?}", asked.input_requests);
        assert!(
            text.contains("powering on") && text.contains("full output"),
            "{text}"
        );
        let old = run(&fleet, None, power(true), false).await;
        assert!(
            is_error(&old) && error_words(&old).contains("confirm"),
            "{profile}"
        );
        assert!(fleet.sets.lock().unwrap().is_empty(), "{profile}");
        let old = run(&fleet, None, set("power", json!(true), true), false).await;
        assert!(went(&old), "{profile}: {old:?}");
        // Powering off never asks, for either kind of client.
        for can_ask in [true, false] {
            let off = run(&fleet, None, power(false), can_ask).await;
            assert!(went(&off), "{profile}: {off:?}");
        }
    }
}

#[tokio::test]
async fn powering_on_a_freya_2_asks_at_its_current_volume_and_is_refused_above_the_cap() {
    let fleet = scripted("freya2", &[("volume", 100)]);
    let level = convert::raw_to_display(fleet.profile.control_by_key("volume").unwrap(), 100)
        .expect("a level");
    let asked = run(&fleet, None, power(true), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    let text = format!("{:?}", asked.input_requests);
    assert!(text.contains("its current volume of"), "{text}");
    let old = run(&fleet, None, power(true), false).await;
    assert!(is_error(&old) && error_words(&old).contains("confirm"));
    let old = run(&fleet, None, set("power", json!(true), true), false).await;
    assert!(went(&old), "{old:?}");
    // Above the cap: refused whatever is confirmed, and powering off still goes through.
    for can_ask in [true, false] {
        let answer = run(
            &fleet,
            Some(level - 5.0),
            set("power", json!(true), true),
            can_ask,
        )
        .await;
        assert!(
            is_error(&answer)
                && error_words(&answer).contains(&format!(
                    "powering on amp would play at {:.1} dB, above this server's limit of {:.1} dB \
                     (max_volume_db); lower the volume first",
                    level,
                    level - 5.0
                )),
            "{answer:?}"
        );
        let off = run(&fleet, Some(level - 5.0), power(false), can_ask).await;
        assert!(went(&off), "{off:?}");
    }
}

#[tokio::test]
async fn an_unreported_volume_is_not_silence() {
    // The unmute and the power-on say the volume is not known, never "silence".
    for (profile, request) in [
        ("gungnir2-v2", call("mute", json!({"on": false}))),
        ("freya2", power(true)),
    ] {
        let fleet = scripted(profile, &[]);
        let asked = run(&fleet, None, request, true).await;
        let CallToolResponse::InputRequired(asked) = asked else {
            panic!("{profile}: {asked:?}")
        };
        let text = format!("{:?}", asked.input_requests);
        assert!(
            text.contains("a volume the device has not reported yet") && !text.contains("silence"),
            "{profile}: {text}"
        );
    }
    // A real silence still reads as silence.
    let fleet = scripted("gungnir2-v2", &[("volume", 0), ("mute", 1)]);
    let asked = run(&fleet, None, call("mute", json!({"on": false})), true).await;
    let text = format!("{asked:?}");
    assert!(asks(&asked) && text.contains("silence"), "{text}");
    // A rise from an unreported volume says so too.
    let fleet = scripted("gungnir2-v2", &[]);
    let asked = run(&fleet, None, set("volume", json!(-60), false), true).await;
    let text = format!("{asked:?}");
    assert!(
        asks(&asked) && text.contains("has not reported yet") && !text.contains("silence"),
        "{text}"
    );
}

/// An unreported volume is never a reason to refuse: with a cap set, a target at or below it, an
/// unmute and a power-on ask, in the words of an unknown volume; only a target above the cap is
/// refused.
#[tokio::test]
async fn with_a_cap_an_unreported_volume_asks_and_refuses_only_above_the_cap() {
    let cap = Some(-20.0);
    for (profile, request) in [
        ("gungnir2-v2", call("mute", json!({"on": false}))),
        ("gungnir2-v2", set("volume", json!(-60), false)),
        ("freya2", set("power", json!(true), false)),
    ] {
        let fleet = scripted(profile, &[]);
        let asked = run(&fleet, cap, request.clone(), true).await;
        let text = format!("{asked:?}");
        assert!(
            asks(&asked) && text.contains("has not reported yet"),
            "{profile}: {text}"
        );
        let old = run(&fleet, cap, request.clone(), false).await;
        let words = error_words(&old);
        assert!(
            is_error(&old) && words.contains("confirm") && words.contains("has not reported yet"),
            "{profile}: {words}"
        );
        assert!(fleet.sets.lock().unwrap().is_empty(), "{profile}");
        let mut confirmed = request.clone();
        if let Some(arguments) = confirmed.arguments.as_mut() {
            arguments.insert("confirm".into(), json!(true));
        }
        let old = run(&fleet, cap, confirmed, false).await;
        assert!(went(&old), "{profile}: {old:?}");
    }
    // A target above the cap is the cap's own business, whatever the volume reads.
    let fleet = scripted("gungnir2-v2", &[]);
    for can_ask in [true, false] {
        let answer = run(&fleet, cap, set("volume", json!(-10), true), can_ask).await;
        assert!(
            is_error(&answer)
                && error_words(&answer).contains(
                    "raising the volume to -10.0 dB is above this server's limit of -20.0 dB \
                     (max_volume_db); choose a level at or below it"
                ),
            "{answer:?}"
        );
    }
}

/// The kind-aware volume test and `roles::volume` agree on every built-in profile: a profile
/// has a volume control exactly when one of its controls passes `is_volume`.
#[test]
fn the_kind_aware_volume_test_agrees_with_the_policys_lookup() {
    let registry = Registry::builtin().expect("the built-in profiles");
    let mut with_volume = 0;
    for name in registry.names() {
        let profile = registry.get(name).unwrap();
        let by_kind = profile.controls.iter().any(roles::is_volume);
        assert_eq!(by_kind, roles::volume(profile).is_some(), "{name}");
        with_volume += usize::from(by_kind);
    }
    assert!(with_volume > 0, "no built-in profile has a volume");
}

/// What `get_state` says of `profile`'s balance when the device reads raw step `raw`.
fn balance_state(profile: &str, raw: i64) -> Value {
    let registry = Registry::builtin().unwrap();
    let profile = registry.get(profile).unwrap();
    let control = profile.control_by_key("balance").unwrap();
    let mut value = reading("balance", raw);
    value.id = control.id.to_string();
    describe_control(profile, control, &[value])
}

#[test]
fn a_balance_reads_from_its_centre_with_its_range_shifted_to_match() {
    let freya = balance_state("freya2", 100);
    assert_eq!(freya["value"], json!(0));
    assert_eq!(freya["range"], json!({"min": -100, "max": 100}));
    assert_eq!(balance_state("freya2", 0)["value"], json!(-100));
    let mesh = balance_state("mesh-eq-3", 64);
    assert_eq!(mesh["value"], json!(0));
    assert_eq!(mesh["range"], json!({"min": -63, "max": 63}));
}

/// A balance is read from its raw step, counted from its centre, whatever converted value a
/// reading happens to carry.
#[test]
fn a_balance_reads_from_its_raw_step_whatever_value_the_reading_carries() {
    let registry = Registry::builtin().unwrap();
    let control = registry
        .get("freya2")
        .unwrap()
        .control_by_key("balance")
        .unwrap();
    let mut right = reading("balance", 130);
    right.value = Some(130.0);
    assert_eq!(value_of(control, &right), json!(30));
}

#[tokio::test]
async fn setting_a_balance_to_zero_centres_it_and_minus_100_is_hard_left() {
    for (profile, centre) in [("freya2", "100"), ("mesh-eq-3", "64")] {
        let fleet = scripted(profile, &[]);
        let answer = run(&fleet, None, set("balance", json!(0), false), false).await;
        assert!(went(&answer), "{profile}: {answer:?}");
        assert_eq!(
            fleet.sets.lock().unwrap().last().unwrap(),
            &("balance".to_string(), centre.to_string()),
            "{profile}"
        );
    }
    let fleet = scripted("freya2", &[]);
    let answer = run(&fleet, None, set("balance", json!(-100), false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(fleet.sets.lock().unwrap()[0].1, "0");
    let answer = run(&fleet, None, set("balance", json!(100), false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(fleet.sets.lock().unwrap()[1].1, "200");
}

#[tokio::test]
async fn a_balance_past_either_end_is_refused_naming_the_shifted_bounds() {
    let fleet = scripted("freya2", &[]);
    for out in [json!(101), json!(-101)] {
        let answer = run(&fleet, None, set("balance", out.clone(), false), false).await;
        assert!(is_error(&answer), "{out}: {answer:?}");
        let words = error_words(&answer);
        assert!(words.contains("between -100 and 100"), "{out}: {words}");
    }
    assert!(fleet.sets.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_reply_to_setting_a_balance_counts_it_from_its_centre() {
    for (asked, shown) in [(0, "0"), (-10, "-10")] {
        let fleet = scripted("freya2", &[]);
        let answer = run(&fleet, None, set("balance", json!(asked), false), false).await;
        assert!(went(&answer), "{answer:?}");
        let CallToolResponse::Complete(result) = &answer else {
            panic!("{answer:?}");
        };
        let reply = result
            .structured_content
            .as_ref()
            .expect("a structured reply");
        assert_eq!(reply["value"], json!(asked), "{reply}");
        assert_eq!(reply["display"], json!(shown), "{reply}");
    }
}

#[test]
fn the_volume_in_db_keeps_its_own_unit_and_range() {
    let registry = Registry::builtin().unwrap();
    let volume = registry
        .get("gungnir2-v2")
        .unwrap()
        .control_by_key("volume")
        .unwrap();
    assert_eq!(level_raw(volume, &json!(0)).unwrap(), 127);
    let range = level_range(volume).expect("a range");
    assert_eq!(range["max"], json!(0.0));
    assert!(range["min"].as_f64().unwrap() < -96.0);
}

/// `get_state` reads a readout an `only_while` gates as its control allows: warming up reads
/// false while the amp is off or has not said, and true only while it is on and warming up.
#[test]
fn a_gated_readout_reads_false_while_its_gate_reads_false() {
    let registry = Registry::builtin().unwrap();
    let tyr = registry.get("tyr").unwrap();
    let warming = tyr.control_by_key("warming_up").unwrap();
    let power = tyr.control_by_key("power").unwrap();
    let at = |control: &Control, raw: Value| {
        let mut value = reading(&control.key, 0);
        value.id = control.id.to_string();
        value.raw = raw;
        value
    };
    let warm = at(warming, json!(true));
    for (gate, shown) in [
        (Some(json!(false)), false),
        (None, false),
        (Some(json!(true)), true),
    ] {
        let mut values = vec![warm.clone()];
        values.extend(gate.map(|raw| at(power, raw)));
        assert_eq!(
            describe_control(tyr, warming, &values)["value"],
            json!(shown),
            "{values:?}"
        );
    }
    let cool = [at(warming, json!(false)), at(power, json!(true))];
    assert_eq!(describe_control(tyr, warming, &cool)["value"], json!(false));
}

/// A device echoes a passphrase change with a status on function 250 that carries the new code in
/// its encoded form. `get_state` answers a remote client, so even a profile that names a control
/// on that function shows no row for it and no code.
#[tokio::test]
async fn the_passphrase_never_reaches_a_remote_view_in_get_state() {
    let registry = Registry::builtin().unwrap();
    let mut profile = registry.get("gungnir2-v2").unwrap().clone();
    let mut code = profile.control_by_key("input").unwrap().clone();
    code.id = svolder_proto::function::PASSPHRASE;
    code.key = "code".to_string();
    code.label = "Code".to_string();
    code.kind = ControlKind::Readout;
    code.values.clear();
    profile.controls.push(code);
    let fleet = scripted_over(profile, &[]);
    let mut echo = reading("code", 12_648_430);
    echo.id = "250".to_string();
    *fleet.state.lock().unwrap() = vec![reading("input", 1), echo];
    let answer = run(&fleet, None, call("get_state", json!({})), false).await;
    assert!(went(&answer), "{answer:?}");
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let state = result.structured_content.as_ref().expect("a state");
    let text = state.to_string();
    assert!(
        !text.contains("12648430") && !text.contains("\"code\""),
        "{text}"
    );
    let controls = state["controls"].as_array().expect("controls");
    assert!(controls.iter().any(|c| c["key"] == "input"), "{text}");
}

/// `load_preset` recalls through the device's writer, which plans it and holds other front ends'
/// tone writes until it is done, and reports the plan the writer made.
#[tokio::test]
async fn load_preset_recalls_through_the_devices_writer() {
    let fleet = scripted("gungnir2-v2", &[]);
    let answer = run(
        &fleet,
        None,
        call("load_preset", json!({"name": "warm"})),
        false,
    )
    .await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.recalls.lock().unwrap(), ["warm"]);
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let reply = result
        .structured_content
        .as_ref()
        .expect("a structured reply");
    assert_eq!(reply["preset"], json!("Warm"));
    assert!(reply["writes"].as_u64().is_some_and(|n| n > 0), "{reply}");
}

/// A rise of 6 dB or less that lands above three quarters of the range needs no question but
/// climbs, through the device's writer, and the tool reports the level the device reached.
#[tokio::test]
async fn a_rise_above_three_quarters_climbs_and_reports_the_level_reached() {
    let fleet = scripted("gungnir2-v2", &[("volume", 120)]);
    let answer = run(&fleet, None, set("volume", json!(0), false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.glides.lock().unwrap(), [("volume".to_string(), 127)]);
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let reply = result
        .structured_content
        .as_ref()
        .expect("a structured reply");
    assert_eq!(reply["value"], json!(0.0));
}

/// Every volume write goes through the device's writer, which judges it on its own reading: a
/// rise to three quarters and a lowering land at once there, and the tool reports the echo.
#[tokio::test]
async fn every_volume_write_goes_through_the_writer_and_one_that_may_land_at_once_lands() {
    let fleet = scripted("gungnir2-v2", &[("volume", 90)]);
    for raw in [95, 60] {
        let answer = run(
            &fleet,
            None,
            set("volume", json!(db(&fleet.profile, raw)), false),
            false,
        )
        .await;
        assert!(went(&answer), "{raw}: {answer:?}");
        let CallToolResponse::Complete(result) = &answer else {
            panic!("{answer:?}");
        };
        let reply = result
            .structured_content
            .as_ref()
            .expect("a structured reply");
        assert_eq!(reply["value"], json!(db(&fleet.profile, raw)), "{raw}");
    }
    assert_eq!(
        *fleet.glides.lock().unwrap(),
        [("volume".to_string(), 95), ("volume".to_string(), 60)]
    );
    assert_eq!(
        fleet.sets.lock().unwrap().last(),
        Some(&("volume".to_string(), "60".to_string()))
    );
}

/// An MCP climb another front end cuts short is reported, once the wait runs out, at the level
/// the device reached: not an error, and not a hang.
#[tokio::test(start_paused = true)]
async fn a_climb_cut_short_is_reported_at_the_level_reached() {
    let fleet = scripted("gungnir2-v2", &[("volume", 120)]);
    *fleet.stuck.lock().unwrap() = true;
    let started = Instant::now();
    let answer = run(&fleet, None, set("volume", json!(0), false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert!(started.elapsed() >= CLIMB_WAIT, "{:?}", started.elapsed());
    assert!(
        started.elapsed() < CLIMB_WAIT + 2 * CLIMB_POLL,
        "{:?}",
        started.elapsed()
    );
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let reply = result
        .structured_content
        .as_ref()
        .expect("a structured reply");
    assert_eq!(reply["value"], json!(db(&fleet.profile, 120)));
}

/// A daemon that stops answering reads once the climb has begun does not hold the tool past its
/// wait: it answers with an error then.
#[tokio::test(start_paused = true)]
async fn a_daemon_that_stops_answering_does_not_hold_a_climb_past_the_wait() {
    let fleet = scripted("gungnir2-v2", &[("volume", 120)]);
    *fleet.stuck.lock().unwrap() = true;
    *fleet.deaf.lock().unwrap() = true;
    let started = Instant::now();
    let answer = run(&fleet, None, set("volume", json!(0), false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(
        started.elapsed() < CLIMB_WAIT + 2 * CLIMB_POLL,
        "{:?}",
        started.elapsed()
    );
}

/// The tone control's readings once `preset` is on: every value its recall writes on a device
/// that had read nothing.
fn tone_of(profile: &Profile, preset: &Preset) -> Vec<ValueObject> {
    let eq = profile.eq.as_ref().expect("a tone control");
    let recall = preset.eq_writes(profile, |_| None).expect("a plan");
    recall
        .plan
        .writes
        .iter()
        .filter_map(|write| {
            let m = svolder_proto::Message::status(
                write.id(eq)?,
                svolder_proto::Value::U8(write.raw()),
            );
            Some(ValueObject::from_reading(
                &svolder_profiles::describe(profile, &m),
                &m,
            ))
        })
        .collect()
}

/// `list_presets` marks the one preset that is on: the one recalled while it matches, else the
/// first that matches.
#[tokio::test]
async fn list_presets_marks_the_preset_that_is_on() {
    let fleet = scripted("gungnir2-v2", &[]);
    let eq = fleet.profile.eq.clone().expect("a tone control");
    let warm = svolder_profiles::tone::preset::builtins(&eq)
        .into_iter()
        .find(|p| p.name == "Warm")
        .expect("Warm");
    let mut twin = warm.clone();
    twin.name = "Warm too".into();
    *fleet.tone.lock().unwrap() = tone_of(&fleet.profile, &warm);
    *fleet.presets.lock().unwrap() = vec![twin];
    let on_of = |answer: &CallToolResponse| -> Vec<String> {
        let CallToolResponse::Complete(result) = answer else {
            panic!("{answer:?}");
        };
        let reply = result
            .structured_content
            .as_ref()
            .expect("a structured reply");
        reply["presets"]
            .as_array()
            .expect("a list")
            .iter()
            .filter(|p| p["on"] == json!(true))
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect()
    };
    let first = run(&fleet, None, call("list_presets", json!({})), false).await;
    assert_eq!(on_of(&first), ["Warm"]);
    *fleet.recalled.lock().unwrap() = Some("Warm too".into());
    let recalled = run(&fleet, None, call("list_presets", json!({})), false).await;
    assert_eq!(on_of(&recalled), ["Warm too"]);
}

/// The tool names a server lists.
fn listed(fleet: &Arc<Scripted>, writes: bool) -> Vec<String> {
    Tools::new(Arc::clone(fleet) as Arc<dyn Fleet>, writes, None)
        .list()
        .iter()
        .map(|t| t.name.to_string())
        .collect()
}

/// The structured answer of a call that went through.
fn structured(response: &CallToolResponse) -> Value {
    match response {
        CallToolResponse::Complete(r) => r.structured_content.clone().expect("a structured answer"),
        other => panic!("not a complete answer: {other:?}"),
    }
}

#[test]
fn mcp_identify_listed_only_with_writes() {
    let fleet = scripted("gungnir2-v2", &[]);
    assert!(!listed(&fleet, false).contains(&"identify".to_string()));
    assert!(listed(&fleet, true).contains(&"identify".to_string()));
    let tool = Tools::new(Arc::clone(&fleet) as Arc<dyn Fleet>, true, None)
        .list()
        .into_iter()
        .find(|t| t.name == "identify")
        .expect("listed with writes");
    assert_eq!(
        tool.annotations.expect("annotations").read_only_hint,
        Some(false)
    );
    let properties = tool.input_schema["properties"]
        .as_object()
        .expect("properties")
        .clone();
    assert_eq!(properties.keys().collect::<Vec<_>>(), ["device"]);
    assert_eq!(tool.input_schema["required"], json!([]));
}

#[tokio::test]
async fn mcp_identify_is_refused_naming_the_setting_while_writes_are_off() {
    let fleet = scripted("gungnir2-v2", &[]);
    let off = Tools::new(Arc::clone(&fleet) as Arc<dyn Fleet>, false, None);
    let reply = off
        .call(call("identify", json!({})), Caller { can_ask: true })
        .await
        .expect("an answer");
    assert!(is_error(&reply));
    assert!(
        error_words(&reply).contains("[mcp] writes"),
        "{}",
        error_words(&reply)
    );
    assert_eq!(*fleet.identifies.lock().unwrap(), 0, "nothing was sent");
}

#[tokio::test]
async fn mcp_identify_answers_sent() {
    let fleet = scripted("gungnir2-v2", &[]);
    let reply = run(&fleet, None, call("identify", json!({})), false).await;
    assert!(went(&reply), "{reply:?}");
    assert_eq!(structured(&reply), json!({"device": "amp", "sent": true}));
    assert_eq!(*fleet.identifies.lock().unwrap(), 1);

    // Inside the session's gap nothing goes out: the answer says so, and it is not an error.
    *fleet.identify.lock().unwrap() = Some(Response::Identify {
        device: "amp".into(),
        sent: false,
    });
    let reply = run(
        &fleet,
        None,
        call("identify", json!({"device": "amp"})),
        false,
    )
    .await;
    assert!(went(&reply), "{reply:?}");
    assert_eq!(structured(&reply), json!({"device": "amp", "sent": false}));
}

/// A device whose profile offers no identify answers a tool error saying so, and the daemon is
/// never asked.
#[tokio::test]
async fn mcp_identify_is_refused_for_a_profile_that_offers_none() {
    let fleet = scripted("meshcard", &[]);
    let reply = run(&fleet, None, call("identify", json!({})), false).await;
    assert!(is_error(&reply), "{reply:?}");
    assert!(
        error_words(&reply).contains("amp offers no identify"),
        "{}",
        error_words(&reply)
    );
    assert_eq!(*fleet.identifies.lock().unwrap(), 0);
}

#[tokio::test]
async fn mcp_identify_says_so_when_the_device_is_down() {
    let fleet = scripted("gungnir2-v2", &[]);
    *fleet.identify.lock().unwrap() = Some(Response::Error {
        code: 3,
        message: svolder_ipc::NOT_CONNECTED.to_string(),
    });
    let reply = run(&fleet, None, call("identify", json!({})), false).await;
    assert!(is_error(&reply));
    assert!(
        error_words(&reply).contains(svolder_ipc::NOT_CONNECTED),
        "{}",
        error_words(&reply)
    );
    let reply = run(
        &fleet,
        None,
        call("identify", json!({"surprise": 1})),
        false,
    )
    .await;
    assert!(error_words(&reply).contains("surprise"));
}

#[tokio::test]
async fn list_devices_carries_the_signal_when_there_is_one() {
    let fleet = scripted("gungnir2-v2", &[]);
    let reply = run(&fleet, None, call("list_devices", json!({})), false).await;
    assert_eq!(
        structured(&reply),
        json!({"devices": [{"name": "amp", "product": "Amp", "connected": true}]}),
        "no signal known: no key"
    );
    *fleet.signal.lock().unwrap() = Some(svolder_ipc::Signal {
        dbm: -62,
        source: svolder_ipc::SignalSource::Advert,
        heard_at_ms: 1_792_931_696_789,
    });
    let reply = run(&fleet, None, call("list_devices", json!({})), false).await;
    assert_eq!(
        structured(&reply)["devices"][0]["signal"],
        json!({"dbm": -62, "source": "advert", "heard_at_ms": 1_792_931_696_789u64})
    );
}

/// The six bands of a Loki Max as raw steps, by key.
fn bands(raw: [i64; 6]) -> Vec<(&'static str, i64)> {
    ["band1", "band2", "band3", "band4", "band5", "band6"]
        .into_iter()
        .zip(raw)
        .collect()
}

const FLAT: [i64; 6] = [15; 6];

fn load(name: &str, confirm: bool) -> CallToolRequestParams {
    call("load_preset", json!({"name": name, "confirm": confirm}))
}

/// `list_presets` on a Loki Max lists its three slots, never marked, then its five built-in
/// settings, the one the bands read marked on.
#[tokio::test]
async fn list_presets_on_a_loki_max_lists_three_slots_and_five_builtins() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let answer = run(&fleet, None, call("list_presets", json!({})), false).await;
    assert!(went(&answer), "{answer:?}");
    let reply = structured(&answer);
    let rows = reply["presets"].as_array().expect("a list");
    assert_eq!(rows.len(), 8, "{reply}");
    assert_eq!(
        rows[0],
        json!({"name": "Preset 1", "slot": 1, "built_in": false, "on": false})
    );
    assert_eq!(rows[2]["name"], json!("Preset 3"));
    assert!(rows[..3].iter().all(|row| row["on"] == json!(false)));
    let names: Vec<&str> = rows[3..]
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["Flat", "Bass Boost", "Treble Boost", "Smiley", "Loudness"]
    );
    assert!(rows[3..].iter().all(|row| row["built_in"] == json!(true)));
    let on: Vec<&str> = rows
        .iter()
        .filter(|row| row["on"] == json!(true))
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert_eq!(on, ["Flat"]);
    // With a band unread, none is on.
    let fleet = scripted("lokimax", &bands(FLAT)[..5]);
    let reply = structured(&run(&fleet, None, call("list_presets", json!({})), false).await);
    assert!(
        reply["presets"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["on"] == json!(false)),
        "{reply}"
    );
}

#[tokio::test]
async fn a_builtin_setting_within_6_db_of_the_floor_goes_through_unasked() {
    // Band 1 reads 6.4 dB; Bass Boost takes it to 10.4 dB, 4 dB up, and no other band rises by
    // more than 6 dB.
    let mut raw = FLAT;
    raw[0] = 23;
    let fleet = scripted("lokimax", &bands(raw));
    let answer = run(&fleet, None, load("bass boost", false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
    let reply = structured(&answer);
    assert_eq!(reply["preset"], json!("Bass Boost"));
    assert_eq!(reply["writes"], json!(3));
}

#[tokio::test]
async fn a_builtin_setting_that_raises_a_band_more_than_6_db_asks_first() {
    let fleet = scripted("lokimax", &bands(FLAT));
    // A client that cannot ask is told the question's words and to repeat with confirm.
    let answer = run(&fleet, None, load("Bass Boost", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(
        words.contains(
            "raising 20 Hz on amp from 0.0 dB to 10.4 dB is more than 6 dB above its lowest \
             level in the last minute (0.0 dB)"
        ),
        "{words}"
    );
    assert!(
        !words.contains("120 Hz"),
        "only the band that asks is named: {words}"
    );
    assert!(words.contains("repeat this call with"), "{words}");
    assert!(fleet.applied.lock().unwrap().is_empty());
    // A client that can ask is sent the question, whatever the model confirms.
    let asked = run(&fleet, None, load("Bass Boost", true), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    assert_eq!(
        asked.request_state.as_deref(),
        Some("setting|amp|Bass Boost|band1=10.4@0.0")
    );
    assert!(fleet.applied.lock().unwrap().is_empty());
    // The yes of a client that cannot ask goes through, and the floor is reset for the band.
    let tools = server(&fleet, None);
    let answer = tools
        .call(load("bass boost", true), Caller { can_ask: false })
        .await
        .expect("an answer");
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
    assert_eq!(
        tools.floors.floor("amp", "band1"),
        Some(db_of(&fleet, "band1", 28))
    );
}

/// A band's level in dB at raw step `raw`.
fn db_of(fleet: &Arc<Scripted>, key: &str, raw: i64) -> f64 {
    convert::raw_to_display(fleet.profile.control_by_key(key).unwrap(), raw).unwrap()
}

/// A band the device has not reported counts as the lowest there is, as it does for a band set
/// by hand, so a built-in setting over unread bands asks, wording each as a level.
#[tokio::test]
async fn a_builtin_setting_over_unread_bands_asks_as_a_band_set_by_hand_does() {
    let fleet = scripted("lokimax", &[]);
    let answer = run(&fleet, None, load("Flat", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(
        words.contains("from a level the device has not reported yet to 0.0 dB"),
        "{words}"
    );
    assert!(fleet.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_declined_builtin_setting_writes_nothing() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let asked = run(&fleet, None, load("Bass Boost", false), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    let state = asked.request_state.clone().unwrap();
    let retry = |action: Value| {
        let mut request = load("Bass Boost", false);
        request.request_state = Some(state.clone());
        request.input_responses = Some([(ASK.to_string(), action)].into_iter().collect());
        request
    };
    for no in [json!({"action": "decline"}), json!({"action": "cancel"})] {
        let answer = run(&fleet, None, retry(no), true).await;
        assert!(is_error(&answer) && error_words(&answer).contains("declined"));
    }
    assert!(fleet.applied.lock().unwrap().is_empty());
    assert!(fleet.submitted.lock().unwrap().is_empty());
    // The yes of the question asked goes through.
    let yes = json!({"action": "accept", "content": {"confirm": true}});
    let answer = run(&fleet, None, retry(yes), true).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
}

#[tokio::test]
async fn a_slot_recall_asks_first_naming_the_12_db_it_can_raise() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let answer = run(&fleet, None, load("Preset 2", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(
        words.contains(
            "recalling Preset 2 on amp sets its bands to what the device saved there, which can \
             raise a band to as much as 12.0 dB"
        ),
        "{words}"
    );
    let asked = run(&fleet, None, load("preset 2", false), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    assert_eq!(asked.request_state.as_deref(), Some("recall|amp|2"));
    assert!(fleet.submitted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_slot_recall_without_confirm_from_a_client_that_cannot_ask_is_refused() {
    let fleet = scripted("lokimax", &bands(FLAT));
    for name in ["Preset 3", "3"] {
        let answer = run(
            &fleet,
            None,
            call("load_preset", json!({"name": name})),
            false,
        )
        .await;
        assert!(is_error(&answer), "{answer:?}");
        assert!(error_words(&answer).contains("confirm"));
    }
    let answer = run(&fleet, None, load("Preset 3", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(fleet.submitted.lock().unwrap().is_empty());
    assert!(fleet.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_confirmed_slot_recall_sends_the_recall() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let answer = run(&fleet, None, load("Preset 2", true), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.submitted.lock().unwrap(), ["set recall 2"]);
    let reply = structured(&answer);
    assert_eq!(reply["preset"], json!("Preset 2"));
    assert_eq!(reply["slot"], json!(2));
    // A slot outside 1..=3 is no slot.
    let answer = run(&fleet, None, load("Preset 4", true), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(error_words(&answer).contains("Preset 1, Preset 2, Preset 3, Flat"));
    assert_eq!(fleet.submitted.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn load_preset_refuses_a_save() {
    let fleet = scripted("lokimax", &bands(FLAT));
    for name in ["save", "Save", "SAVE 2", "save preset 2", "Save Preset 1"] {
        for can_ask in [false, true] {
            let answer = run(&fleet, None, load(name, true), can_ask).await;
            assert!(is_error(&answer), "{name}: {answer:?}");
            assert!(
                error_words(&answer).contains(
                    "saving a preset is not offered here, because it overwrites what the device \
                     holds; amp can be saved to from its own front ends"
                ),
                "{name}: {answer:?}"
            );
        }
    }
    assert!(fleet.submitted.lock().unwrap().is_empty());
    assert!(fleet.applied.lock().unwrap().is_empty());
    assert!(fleet.sets.lock().unwrap().is_empty());
}

/// A name that only begins with the save action's key is no save: a built-in setting called
/// `Saved Mix` loads, and an unknown name is told what it can be, not that saving is not offered.
#[tokio::test]
async fn a_name_that_merely_starts_with_save_is_not_refused_as_a_save() {
    let mut profile = Registry::builtin()
        .expect("the built-in profiles")
        .get("lokimax")
        .expect("a built-in")
        .clone();
    profile
        .builtin_settings
        .as_mut()
        .expect("settings")
        .settings[0]
        .name = "Saved Mix".into();
    let fleet = scripted_over(profile, &bands(FLAT));
    let answer = run(&fleet, None, load("saved mix", false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(structured(&answer)["preset"], json!("Saved Mix"));
    for name in ["savings", "save the day", "saves 2"] {
        let answer = run(&fleet, None, load(name, true), false).await;
        assert!(is_error(&answer), "{name}: {answer:?}");
        assert!(
            !error_words(&answer).contains("saving a preset is not offered"),
            "{name}: {answer:?}"
        );
    }
}

/// A yes recorded under one floor does not answer the question asked under another: the state of
/// a built-in setting's question names each band that asks with its target and floor.
#[tokio::test]
async fn a_yes_under_one_floor_does_not_answer_the_question_under_another() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let tools = server(&fleet, None);
    let asked = tools
        .call(load("Bass Boost", false), Caller { can_ask: true })
        .await
        .expect("an answer");
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    let state = asked.request_state.clone().expect("a state");
    let yes = json!({"action": "accept", "content": {"confirm": true}});
    let retry = |state: &str| {
        let mut request = load("Bass Boost", false);
        request.request_state = Some(state.to_string());
        request.input_responses = Some([(ASK.to_string(), yes.clone())].into_iter().collect());
        request
    };
    // Another front end lowers band 1 meanwhile, and the floor with it.
    tools.floors.record("amp", "band1", -12.0);
    let again = tools
        .call(retry(&state), Caller { can_ask: true })
        .await
        .expect("an answer");
    let CallToolResponse::InputRequired(again) = again else {
        panic!("a yes under the old floor went through: {again:?}")
    };
    assert_ne!(again.request_state.as_deref(), Some(state.as_str()));
    assert!(fleet.applied.lock().unwrap().is_empty());
    // The yes of the question asked under this floor goes through.
    let state = again.request_state.expect("a state");
    let answer = tools
        .call(retry(&state), Caller { can_ask: true })
        .await
        .expect("an answer");
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
}

/// The state of a built-in setting's question names the floor each band was judged against, not
/// the floor of the moment it is built: a lower reading another front end lets the floors note
/// while a later band is read does not change it.
#[tokio::test]
async fn a_floor_noted_between_judging_and_the_state_does_not_change_the_state() {
    let flat = scripted("lokimax", &bands(FLAT));
    let tools = server(&flat, None);
    tools.floors.record("amp", "band1", -3.0);
    let state_of = |tools: &Tools| {
        let fleet = Arc::clone(&flat);
        let tools = tools.clone();
        async move {
            fleet.applied.lock().unwrap().clear();
            let asked = tools
                .call(load("Bass Boost", false), Caller { can_ask: true })
                .await
                .expect("an answer");
            let CallToolResponse::InputRequired(asked) = asked else {
                panic!("{asked:?}")
            };
            asked.request_state.expect("a state")
        }
    };
    let quiet = state_of(&tools).await;
    // The same call, with band 1's floor dropping as band 2 is read for its judgement, after
    // the plan's first read of every band and band 1's judgement.
    let floors = Arc::clone(&tools.floors);
    let band2_reads = std::sync::atomic::AtomicUsize::new(0);
    *flat.on_read.lock().unwrap() = Some(Box::new(move |key| {
        if key == "band2" && band2_reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            floors.record("amp", "band1", -30.0);
        }
    }));
    let moving = state_of(&tools).await;
    // The hook fired during the call, or the test proves nothing: band 1's floor is the lowered one.
    assert_eq!(
        tools.floors.floor("amp", "band1"),
        Some(-30.0),
        "the floor never moved during the call"
    );
    assert_eq!(moving, quiet);
}

#[tokio::test]
async fn set_control_refuses_an_action_key() {
    let fleet = scripted("lokimax", &bands(FLAT));
    for key in ["recall", "save", "Recall"] {
        let answer = run(&fleet, None, set(key, json!(2), true), false).await;
        assert!(is_error(&answer), "{key}: {answer:?}");
        assert!(error_words(&answer).contains("has no control"), "{key}");
    }
    assert!(fleet.sets.lock().unwrap().is_empty());
    assert!(fleet.submitted.lock().unwrap().is_empty());
}

/// A failed read is the answer, not "no built-in is on".
#[tokio::test]
async fn list_presets_on_a_loki_max_answers_a_failed_read_with_its_error() {
    let fleet = scripted("lokimax", &bands(FLAT));
    *fleet.failing_reads.lock().unwrap() = Some("the device is not connected".to_string());
    let answer = run(&fleet, None, call("list_presets", json!({})), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(error_words(&answer).contains("the device is not connected"));
}

/// A setting the bands already read has nothing to write, and says so without a batch.
#[tokio::test]
async fn a_builtin_setting_the_device_is_already_on_says_so_and_sends_nothing() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let answer = run(&fleet, None, load("flat", false), false).await;
    assert!(went(&answer), "{answer:?}");
    let reply = structured(&answer);
    assert_eq!(reply["writes"], json!(0));
    assert_eq!(reply["message"], json!("Flat is already on"));
    assert!(fleet.applied.lock().unwrap().is_empty());
}

/// The writer plans again from its own readings, so a band that reads its target now but was
/// more than 6 dB lower within the last minute is asked about too: another front end may lower
/// it before the writer sends the setting.
#[tokio::test]
async fn a_band_on_target_now_but_far_above_its_floor_still_asks() {
    // Bass Boost is 28 21 17 15 15 15. Band 1 already reads 28; band 2 still has to rise.
    let raw = [28, 15, 15, 15, 15, 15];
    let fleet = scripted("lokimax", &bands(raw));
    let tools = server(&fleet, None);
    tools.floors.record("amp", "band1", 0.0);
    let answer = tools
        .call(load("Bass Boost", false), Caller { can_ask: false })
        .await
        .expect("an answer");
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(
        words.contains(
            "writing 20 Hz on amp at 10.4 dB is more than 6 dB above its lowest level in the \
             last minute (0.0 dB)"
        ),
        "{words}"
    );
    assert!(!words.contains("120 Hz"), "{words}");
    assert!(fleet.applied.lock().unwrap().is_empty());
    // With no lower level in the last minute the same setting goes through unasked.
    let fleet = scripted("lokimax", &bands(raw));
    let answer = run(&fleet, None, load("Bass Boost", false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
}
