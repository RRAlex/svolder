// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The virtual speakers in the controller: every lifecycle row, every reset, the guards that
//! leave no filter hanging, and the set's WAV.

use super::*;
use crate::eq::live::OutputSink;
use crate::eq::manage::SpeakersConf;
use svolder_speakers::{SetMeta, Store, StoreError};

fn sink(name: &str) -> OutputSink {
    OutputSink {
        name: name.to_string(),
        description: Some(format!("{name} (description)")),
        rate_hz: Some(48_000),
    }
}

fn spk_on() -> SpeakersEdit {
    SpeakersEdit {
        on: Some(true),
        set: None,
        done: true,
    }
}

fn spk_off() -> SpeakersEdit {
    SpeakersEdit {
        on: Some(false),
        set: None,
        done: true,
    }
}

fn spk_set(id: &str) -> SpeakersEdit {
    SpeakersEdit {
        on: None,
        set: Some(id.to_string()),
        done: true,
    }
}

fn xf_edit(on: bool) -> CrossfeedEdit {
    CrossfeedEdit {
        on: Some(on),
        done: true,
        ..CrossfeedEdit::default()
    }
}

/// A node as a chain built with a set shows it: preamp, crossfeed and speakers ports, the
/// speakers' stage off.
fn vnode(id: u32, rate_hz: u32) -> LiveNode {
    let mut node = node(id, Some(rate_hz));
    node.params.extend(crate::eq::live::crossfeed_params(
        &crate::eq::CrossfeedStage::bypass(),
    ));
    node.params.extend(crate::eq::live::speakers_params(false));
    node
}

/// Four diracs: a set a convolver could play.
pub(super) fn put_set(store: &Store, id: &str, label: &str) {
    put_set_at(store, id, label, 10);
}

/// [`put_set`] with the diracs starting at sample `first`.
fn put_set_at(store: &Store, id: &str, label: &str, first: usize) {
    let mut channels = vec![vec![0.0f32; 64]; 4];
    for (n, channel) in channels.iter_mut().enumerate() {
        channel[first + n] = 0.5;
    }
    let wav = svolder_speakers::wav::write(48_000, &channels);
    let meta = SetMeta {
        id: id.to_string(),
        room: svolder_speakers::Room::No,
        label: label.to_string(),
        licence: "CC-BY-4.0".to_string(),
        credit: "Measured by the authors".to_string(),
        source_url: "https://example.invalid/set".to_string(),
        source_sha256: "0".repeat(64),
        wav_sha256: String::new(),
        rate: 48_000,
        taps: 64,
        taps_ms: 64.0 / 48.0,
        angles: None,
        loudness_db: -6.9,
        scale_db: -3.0,
        worst_gain_db: -0.1,
        cut: false,
        own: false,
        added: 1,
        used: 1,
    };
    store.save(meta, &wav, None).unwrap();
}

/// What a [`HookChain`] is asked to do, before the fake does it.
type Hook = Box<dyn FnMut(&ChainCall) -> Option<EqFailure> + Send>;

/// A [`FakeChain`] with a hook that runs at every apply and every config write, before the fake
/// does them: it may do something meanwhile (a forget racing the apply) or refuse the call (the
/// call is then not recorded by the fake).
#[derive(Clone, Default)]
pub(super) struct HookChain {
    pub(super) inner: FakeChain,
    hook: Arc<Mutex<Option<Hook>>>,
}

impl HookChain {
    pub(super) fn over(inner: FakeChain) -> HookChain {
        HookChain {
            inner,
            hook: Arc::default(),
        }
    }

    pub(super) fn set(&self, hook: impl FnMut(&ChainCall) -> Option<EqFailure> + Send + 'static) {
        *self.hook.lock().unwrap() = Some(Box::new(hook));
    }

    fn run(&self, call: ChainCall) -> Result<(), EqFailure> {
        match self
            .hook
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|hook| hook(&call))
        {
            Some(failure) => Err(failure),
            None => Ok(()),
        }
    }
}

#[async_trait::async_trait]
impl Chain for HookChain {
    async fn apply(&self, request: ApplyRequest) -> Result<HostEqState, EqFailure> {
        self.run(ChainCall::Apply(request.clone()))?;
        self.inner.apply(request).await
    }

    async fn remove(&self) -> Result<(), EqFailure> {
        self.inner.remove().await
    }

    async fn status(&self) -> crate::eq::manage::Status {
        self.inner.status().await
    }

    async fn keep_conf(&self, request: ApplyRequest) -> Result<(), EqFailure> {
        self.run(ChainCall::KeepConf(request.clone()))?;
        self.inner.keep_conf(request).await
    }

    fn unavailable(&self) -> Option<EqFailure> {
        self.inner.unavailable()
    }

    fn state(&self) -> Option<HostEqState> {
        self.inner.state()
    }

    async fn exited(&self) {
        self.inner.exited().await
    }
}

fn no() -> EqFailure {
    EqFailure {
        what: "no".into(),
        next: "later".into(),
    }
}

struct Rig {
    control: EqControl,
    chain: FakeChain,
    hook: HookChain,
    params: FakeParams,
    store: Arc<Store>,
    dir: Dir,
}

/// A controller over a state file and a store with the sets `set-a` and `set-b` fetched, the
/// output `a` seen, and a node that lists the speakers' controls; no set chosen yet. The chain
/// is a [`HookChain`] with no hook.
fn rig() -> Rig {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    put_set(&store, "set-b", "Dummy head, B");
    let chain = FakeChain::new();
    let hook = HookChain::over(chain.clone());
    let params = FakeParams::new();
    params.set_output(Some(sink("a")));
    params.with_speakers_ports(false);
    let control = EqControl::new(
        Arc::new(hook.clone()),
        Box::new(params.clone()),
        Keep::File(dir.state()),
    )
    .with_speakers(Arc::clone(&store));
    Rig {
        control,
        chain,
        hook,
        params,
        store,
        dir,
    }
}

/// A rig with the EQ on and set `set-a` chosen and playing.
async fn playing() -> Rig {
    let mut rig = rig();
    rig.control.set_on(true).await.unwrap();
    rig.control.speakers(spk_set("set-a")).await.unwrap();
    rig.control.speakers(spk_on()).await.unwrap();
    rig
}

fn applies(chain: &FakeChain) -> Vec<ApplyRequest> {
    chain
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            ChainCall::Apply(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn removes(chain: &FakeChain) -> usize {
    chain
        .calls()
        .iter()
        .filter(|call| **call == ChainCall::Remove)
        .count()
}

fn keeps(chain: &FakeChain) -> Vec<ApplyRequest> {
    chain
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            ChainCall::KeepConf(request) => Some(request),
            _ => None,
        })
        .collect()
}

async fn looks(control: &mut EqControl, seconds: u64) {
    for _ in 0..seconds * WATCH_EVERY {
        control.tick().await;
    }
}

fn gain2(sent: &[(String, f64)]) -> Option<f64> {
    value(sent, "lvs:Gain 2")
}

fn speakers(control: &EqControl) -> crate::eq::SpeakersView {
    control
        .view()
        .speakers
        .expect("the controller serves speakers")
}

fn state_file(rig: &Rig) -> HostEqState {
    HostEqState::load(&rig.dir.state()).unwrap().unwrap()
}

// ----- the view and the contract -----

#[test]
fn a_view_from_before_the_speakers_parses_with_none() {
    let view: EqView = serde_json::from_str(
        r#"{"on":false,"points":[],"preamp_db":0.0,"preset":null,"live":false,"note":null,
            "unavailable":null,"max":16}"#,
    )
    .unwrap();
    assert_eq!(view.speakers, None);
    let text = serde_json::to_string(&view).unwrap();
    assert!(!text.contains("speakers"), "{text}");
}

#[tokio::test]
async fn a_controller_with_no_store_does_not_serve_the_speakers() {
    let (mut control, chain, _) = setup(Keep::Memory);
    assert_eq!(control.view().speakers, None);
    assert!(!control.serves_speakers());
    let failure = control.speakers(spk_on()).await.unwrap_err();
    assert!(failure.what.contains("not available"), "{failure:?}");
    assert!(chain.calls().is_empty());
}

#[tokio::test]
async fn the_view_lists_the_fetched_sets_and_the_chosen_one_with_its_picture() {
    let mut rig = rig();
    let view = speakers(&rig.control);
    assert_eq!(view.set, None);
    let ids: Vec<_> = view.sets.iter().map(|set| set.id.as_str()).collect();
    assert_eq!(ids, ["set-a", "set-b"]);
    rig.control.speakers(spk_set("set-b")).await.unwrap();
    let view = speakers(&rig.control);
    assert_eq!(view.set.as_deref(), Some("set-b"));
    assert_eq!(view.label, "Dummy head, B");
    assert_eq!(view.room_name.as_deref(), Some("No room"));
    assert_eq!(view.term.as_deref(), Some("HRTF"));
    assert_eq!(view.licence, "CC-BY-4.0");
    assert_eq!(view.credit.as_deref(), Some("Measured by the authors"));
    assert!(!view.on);
    assert!(!view.curve.hz.is_empty(), "the picture is drawn");
}

#[tokio::test]
async fn the_chosen_set_carries_its_timing_for_the_pictures() {
    let mut rig = rig();
    assert_eq!(speakers(&rig.control).timing, None, "no set, no timing");
    rig.control.speakers(spk_set("set-b")).await.unwrap();
    let view = speakers(&rig.control);
    let timing = view.timing.clone().expect("a chosen set has timing");
    // set-b is a No room set of one Dirac a path: two direct arrivals an ear, nothing more.
    assert!(timing.arrivals.tail.is_none(), "No room has no tail");
    assert_eq!(timing.arrivals.left.len(), 2, "{:?}", timing.arrivals);
    assert!(!timing.decay.times_ms.is_empty());
    assert_eq!(
        speakers(&rig.control).timing,
        view.timing,
        "served again, not changed"
    );
    // The set is fetched again with its impulses moved: the timing is worked out again, so it
    // differs (a kept value would be equal).
    put_set_at(&rig.store, "set-b", "Dummy head, B", 30);
    assert_eq!(
        speakers(&rig.control).timing,
        view.timing,
        "kept until the catalogue says so"
    );
    rig.control.sets_changed();
    let again = speakers(&rig.control).timing;
    assert!(again.is_some());
    assert_ne!(again, view.timing, "worked out again after a refetch");
}

#[test]
fn a_speakers_view_from_before_the_timing_parses_with_none() {
    let view: crate::eq::SpeakersView =
        serde_json::from_str(r#"{"on":false,"label":"x"}"#).unwrap();
    assert_eq!(view.timing, None);
}

// ----- the lifecycle -----

#[tokio::test]
async fn a_set_chosen_while_off_is_only_kept_and_no_chain_loads_it() {
    let mut rig = rig();
    rig.control.speakers(spk_set("set-a")).await.unwrap();
    assert!(rig.chain.calls().is_empty(), "no chain, no WAV loaded");
    assert!(rig.params.sent().is_empty());
    // As crossfeed's settings, the choice is kept in memory until a chain's state exists.
    assert!(!rig.dir.state().exists());
    assert_eq!(speakers(&rig.control).set.as_deref(), Some("set-a"));
}

/// A set chosen, or turned on, is marked used in its sidecar, so the store evicts by last use;
/// turning them off uses nothing.
#[tokio::test]
async fn a_set_chosen_or_turned_on_is_marked_used_in_the_store() {
    let mut rig = rig();
    let used = |rig: &Rig, id: &str| rig.store.check(id).unwrap().used;
    rig.control.speakers(spk_set("set-a")).await.unwrap();
    assert!(used(&rig, "set-a") > 1, "chosen while off");
    assert_eq!(used(&rig, "set-b"), 1, "the other set is not touched");
    rig.store.touch("set-a", 1).unwrap();
    rig.control.set_on(true).await.unwrap();
    rig.control.speakers(spk_on()).await.unwrap();
    assert!(used(&rig, "set-a") > 1, "turned on");
    rig.store.touch("set-a", 1).unwrap();
    rig.control.speakers(spk_off()).await.unwrap();
    assert_eq!(used(&rig, "set-a"), 1, "off is no use");
}

#[tokio::test]
async fn on_with_no_chain_applies_one_flat_with_the_set_and_sends_the_stage() {
    let mut rig = rig();
    rig.control.speakers(spk_set("set-a")).await.unwrap();
    rig.control.speakers(spk_on()).await.unwrap();
    let calls = applies(&rig.chain);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].flat, "the EQ plays flat");
    let conf = calls[0].speakers.clone().expect("the set is in the config");
    assert_eq!(
        conf,
        SpeakersConf {
            id: "set-a".into(),
            wav: rig.store.wav_path("set-a").unwrap(),
            on: false,
        },
        "the config holds the stage OFF"
    );
    let sent = rig.params.sent();
    assert_eq!(gain2(sent.last().unwrap()), Some(1.0));
    assert_eq!(value(sent.last().unwrap(), "lvs:Gain 1"), Some(0.0));
    assert!(speakers(&rig.control).on);
    assert!(state_file(&rig).speakers_on);
}

#[tokio::test]
async fn on_with_no_set_chosen_and_a_set_that_is_not_fetched_are_refused_in_words() {
    let mut rig = rig();
    let failure = rig.control.speakers(spk_on()).await.unwrap_err();
    assert!(failure.what.contains("no set is chosen"), "{failure:?}");
    let failure = rig.control.speakers(spk_set("nope")).await.unwrap_err();
    assert!(failure.what.contains("nope"), "{failure:?}");
    let failure = rig
        .control
        .speakers(SpeakersEdit::default())
        .await
        .unwrap_err();
    assert!(failure.what.contains("names nothing"), "{failure:?}");
    assert!(rig.chain.calls().is_empty(), "a refusal changes nothing");
    assert_eq!(speakers(&rig.control).set, None);
}

#[tokio::test]
async fn on_and_off_are_live_with_no_rebuild_and_the_config_loses_its_convolvers() {
    let mut rig = playing().await;
    let built = applies(&rig.chain).len();
    assert_eq!(
        built, 2,
        "the EQ's chain, then the one with the set (one rebuild)"
    );
    let sends = rig.params.sent().len();
    rig.control.speakers(spk_off()).await.unwrap();
    assert_eq!(applies(&rig.chain).len(), built, "off is not a rebuild");
    assert_eq!(removes(&rig.chain), 0, "the chain runs on");
    let sent = rig.params.sent();
    assert_eq!(gain2(&sent[sends]), Some(0.0), "the bypass went live");
    assert_eq!(value(&sent[sends], "lvs:Gain 1"), Some(1.0));
    let kept = keeps(&rig.chain);
    assert_eq!(
        kept.last().unwrap().speakers,
        None,
        "the config is rewritten without the convolvers"
    );
    let state = state_file(&rig);
    assert!(!state.speakers_on && state.speakers.as_deref() == Some("set-a"));
    assert!(!speakers(&rig.control).on);

    // On again: the chain still has the convolvers, so it is live again too.
    let sends = rig.params.sent().len();
    rig.control.speakers(spk_on()).await.unwrap();
    assert_eq!(
        applies(&rig.chain).len(),
        built,
        "on is not a rebuild either"
    );
    assert_eq!(gain2(&rig.params.sent()[sends]), Some(1.0));
    assert!(speakers(&rig.control).on);
}

#[tokio::test]
async fn another_set_while_on_is_one_rebuild_and_plays_it() {
    let mut rig = playing().await;
    let built = applies(&rig.chain).len();
    let sends = rig.params.sent().len();
    rig.control.speakers(spk_set("set-b")).await.unwrap();
    let calls = applies(&rig.chain);
    assert_eq!(calls.len(), built + 1, "one rebuild");
    assert_eq!(calls.last().unwrap().speakers.as_ref().unwrap().id, "set-b");
    assert_eq!(gain2(&rig.params.sent()[sends]), Some(1.0));
    assert!(speakers(&rig.control).on);
    assert_eq!(state_file(&rig).speakers.as_deref(), Some("set-b"));
    // The same set again changes nothing.
    rig.control.speakers(spk_set("set-b")).await.unwrap();
    assert_eq!(applies(&rig.chain).len(), built + 1);
}

#[tokio::test]
async fn the_eq_off_with_the_speakers_on_keeps_one_chain_flat_and_both_off_removes_it() {
    let mut rig = playing().await;
    rig.control.set_on(false).await.unwrap();
    assert_eq!(removes(&rig.chain), 0, "the speakers keep the chain");
    assert!(speakers(&rig.control).on && !rig.control.view().on);
    rig.control.speakers(spk_off()).await.unwrap();
    assert_eq!(removes(&rig.chain), 1, "both off removes it");
    assert!(!state_file(&rig).on);

    let mut rig = rig_with_set_chosen().await;
    rig.control.speakers(spk_on()).await.unwrap();
    rig.control.speakers(spk_off()).await.unwrap();
    assert_eq!(removes(&rig.chain), 1);
    assert_eq!(applies(&rig.chain).len(), 1, "two chains never stack");
}

async fn rig_with_set_chosen() -> Rig {
    let mut rig = rig();
    rig.control.speakers(spk_set("set-a")).await.unwrap();
    rig
}

#[tokio::test]
async fn the_speakers_take_no_part_of_the_preamp() {
    let mut rig = rig();
    rig.control.set_on(true).await.unwrap();
    rig.control.edit(gain(0, 6.0, true)).await.unwrap();
    let before = rig.control.view().preamp_db;
    rig.control.speakers(spk_set("set-a")).await.unwrap();
    rig.control.speakers(spk_on()).await.unwrap();
    assert!((rig.control.view().preamp_db - before).abs() < 1e-9);
    rig.control.speakers(spk_on()).await.unwrap_or(());
    rig.params.set_playing(true);
    let heard = rig.control.view().levels.unwrap().total_db;
    rig.control.speakers(spk_off()).await.unwrap();
    // A stream plays: the sound stays where it was (the preamp falls by the set's level) and
    // climbs back to the curve's own preamp.
    let view = rig.control.view();
    assert!((view.levels.unwrap().total_db - heard).abs() < 1e-9);
    for _ in 0..80 {
        rig.control.tick().await;
    }
    assert!((rig.control.view().preamp_db - before).abs() < 1e-9);
}

// ----- exclusive with crossfeed -----

#[tokio::test]
async fn crossfeed_on_turns_the_speakers_off_in_the_same_update_and_the_other_way_round() {
    let mut rig = playing().await;
    let sends = rig.params.sent().len();
    rig.control.crossfeed(xf_edit(true)).await.unwrap();
    let sent = rig.params.sent();
    assert_eq!(sent.len(), sends + 1, "one update");
    assert_eq!(value(&sent[sends], "lmix:Gain 2"), Some(1.0));
    assert_eq!(
        gain2(&sent[sends]),
        Some(0.0),
        "the speakers' bypass is in it"
    );
    assert_eq!(value(&sent[sends], "lvs:Gain 1"), Some(1.0));
    let view = rig.control.view();
    assert!(view.crossfeed.unwrap().on);
    let sp = view.speakers.unwrap();
    assert!(!sp.on);
    assert!(sp.note.is_none() || !sp.note.unwrap().is_empty());
    assert!(!state_file(&rig).speakers_on);
    assert!(state_file(&rig).crossfeed_on);

    let sends = rig.params.sent().len();
    rig.control.speakers(spk_on()).await.unwrap();
    let sent = rig.params.sent();
    assert_eq!(sent.len(), sends + 1, "one update");
    assert_eq!(
        value(&sent[sends], "lmix:Gain 2"),
        Some(0.0),
        "crossfeed's bypass"
    );
    assert_eq!(gain2(&sent[sends]), Some(1.0));
    let view = rig.control.view();
    let xf = view.crossfeed.unwrap();
    assert!(!xf.on);
    assert!(xf.note.unwrap().contains("virtual speakers"));
    assert!(view.speakers.unwrap().on);
    let state = state_file(&rig);
    assert!(state.speakers_on && !state.crossfeed_on);
}

#[tokio::test]
async fn a_failed_speakers_start_leaves_crossfeed_playing() {
    let mut rig = rig_with_set_chosen().await;
    rig.control.set_on(true).await.unwrap();
    rig.control.crossfeed(xf_edit(true)).await.unwrap();
    rig.chain.refuse(Some(EqFailure {
        what: "no".into(),
        next: "later".into(),
    }));
    assert!(rig.control.speakers(spk_on()).await.is_err());
    rig.chain.refuse(None);
    let view = rig.control.view();
    assert!(view.crossfeed.unwrap().on, "crossfeed plays on");
    assert!(!view.speakers.unwrap().on);
}

/// A speakers start whose live line is refused and whose fallback apply fails too is taken back,
/// but the session repairs a failed line with the next one: the stage goes again, off, within
/// the next look, so the repair cannot leave the speakers playing under a preamp that rises.
#[tokio::test]
async fn a_failed_speakers_start_sends_the_stage_off_again() {
    let mut rig = playing().await;
    rig.control.speakers(spk_off()).await.unwrap();
    looks(&mut rig.control, 1).await;
    rig.params.takes(false);
    rig.hook
        .set(|call| matches!(call, ChainCall::Apply(_)).then(no));
    let sent_before = rig.params.sent().len();
    assert!(rig.control.speakers(spk_on()).await.is_err());
    assert_eq!(rig.params.sent().len(), sent_before, "the line was refused");
    rig.hook.set(|_| None);
    rig.params.takes(true);
    let sends = rig.params.sent().len();
    looks(&mut rig.control, 3).await;
    let sent = &rig.params.sent()[sends..];
    assert!(
        sent.iter().any(|send| gain2(send) == Some(0.0)),
        "the stage goes again, off: {sent:?}"
    );
    assert!(!speakers(&rig.control).on);
}

// ----- the resets -----

/// A state file the way a session that crashed with the speakers on left it.
fn crashed_with_speakers_on(dir: &Dir, flat: bool) {
    HostEqState {
        on: true,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![pk(1000.0, 3.0, 1.0)],
        slots: vec![1],
        crossfeed: None,
        crossfeed_on: false,
        flat,
        speakers: Some("set-a".into()),
        speakers_on: true,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: None,
        bypass: None,
    }
    .save(&dir.state())
    .unwrap();
}

#[tokio::test]
async fn the_owner_start_turns_speakers_found_on_off_and_rewrites_the_config() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    crashed_with_speakers_on(&dir, false);
    let chain = FakeChain::with_state(HostEqState::load(&dir.state()).unwrap().unwrap());
    let params = FakeParams::new();
    params.with_speakers_ports(true);
    let mut control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params.clone()),
        Keep::File(dir.state()),
    )
    .with_speakers(store.clone());
    let view = speakers(&control);
    assert!(!view.on, "never on from a file");
    assert_eq!(view.off_reason.as_deref(), Some("restart"));
    assert!(view.note.unwrap().contains("restart"));
    control.sync_running().await;
    // The stage is bypassed live, the config rewritten, nothing restarted.
    assert_eq!(gain2(params.sent().last().unwrap()), Some(0.0));
    assert!(applies(&chain).is_empty(), "no restart");
    assert_eq!(keeps(&chain).last().unwrap().speakers, None);
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(!state.speakers_on && state.on);
    assert_eq!(state.speakers.as_deref(), Some("set-a"), "the choice stays");
    assert!(
        store.forget("set-a").is_err(),
        "the chain may still hold its convolvers: the WAV stays"
    );
}

#[tokio::test]
async fn the_owner_start_removes_a_chain_that_ran_for_the_speakers_alone() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    crashed_with_speakers_on(&dir, true);
    let chain = FakeChain::with_state(HostEqState::load(&dir.state()).unwrap().unwrap());
    let mut control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(FakeParams::new()),
        Keep::File(dir.state()),
    )
    .with_speakers(store);
    control.sync_running().await;
    assert_eq!(removes(&chain), 1);
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(!state.on && !state.flat && !state.speakers_on);
    assert_eq!(state.speakers.as_deref(), Some("set-a"));
}

#[tokio::test]
async fn the_owner_start_with_no_chain_running_still_rewrites_the_config() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    crashed_with_speakers_on(&dir, false);
    let chain = FakeChain::with_state(HostEqState::load(&dir.state()).unwrap().unwrap());
    chain.die();
    let mut control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(FakeParams::new()),
        Keep::File(dir.state()),
    )
    .with_speakers(store);
    control.tick().await;
    assert_eq!(
        keeps(&chain).last().map(|request| request.speakers.clone()),
        Some(None)
    );
    assert!(
        !HostEqState::load(&dir.state())
            .unwrap()
            .unwrap()
            .speakers_on
    );
}

#[tokio::test]
async fn the_output_changing_turns_the_speakers_off_on_either_chain() {
    let mut rig = playing().await;
    looks(&mut rig.control, 2).await;
    assert!(speakers(&rig.control).on, "the same output is no change");
    rig.params.set_output(Some(sink("b")));
    looks(&mut rig.control, 1).await;
    let view = speakers(&rig.control);
    assert!(!view.on);
    assert_eq!(view.off_reason.as_deref(), Some("output-changed"));
    assert!(view.note.unwrap().contains("b (description)"));
    assert_eq!(
        rig.params.sent().iter().rev().find_map(|send| gain2(send)),
        Some(0.0)
    );
    assert_eq!(removes(&rig.chain), 0);
    assert!(rig.control.view().on, "the EQ plays on");
    assert_eq!(keeps(&rig.chain).last().unwrap().speakers, None);

    let mut rig = rig_with_set_chosen().await;
    rig.control.speakers(spk_on()).await.unwrap();
    rig.params.set_output(Some(sink("b")));
    looks(&mut rig.control, 1).await;
    assert!(!speakers(&rig.control).on);
    assert_eq!(
        removes(&rig.chain),
        1,
        "a chain for the speakers alone goes"
    );
    assert!(!rig.control.view().on);
}

#[tokio::test]
async fn a_chain_that_restarts_turns_the_speakers_off() {
    let mut rig = playing().await;
    rig.params.set_node(Some(vnode(9, 48_000)));
    looks(&mut rig.control, 1).await;
    let view = speakers(&rig.control);
    assert!(!view.on);
    assert_eq!(view.off_reason.as_deref(), Some("chain-restarted"));
    assert_eq!(removes(&rig.chain), 0, "the EQ's chain stays");

    let mut rig = rig_with_set_chosen().await;
    rig.control.speakers(spk_on()).await.unwrap();
    rig.params.set_node(Some(vnode(9, 48_000)));
    looks(&mut rig.control, 1).await;
    assert!(!speakers(&rig.control).on);
    assert_eq!(removes(&rig.chain), 1);
}

#[tokio::test]
async fn a_graceful_stop_leaves_no_stage_no_flat_chain_and_no_crash_flag() {
    let mut rig = playing().await;
    assert!(state_file(&rig).speakers_on);
    rig.control.shutdown().await;
    assert_eq!(gain2(rig.params.sent().last().unwrap()), Some(0.0));
    let view = speakers(&rig.control);
    assert!(!view.on);
    assert_eq!(view.off_reason.as_deref(), Some("stopped"));
    let state = state_file(&rig);
    assert!(!state.speakers_on && state.on && state.speakers.as_deref() == Some("set-a"));
    assert_eq!(removes(&rig.chain), 0);
    assert_eq!(keeps(&rig.chain).last().unwrap().speakers, None);

    let mut rig = rig_with_set_chosen().await;
    rig.control.speakers(spk_on()).await.unwrap();
    rig.control.shutdown().await;
    assert_eq!(removes(&rig.chain), 1);
    let state = state_file(&rig);
    assert!(!state.on && !state.flat && !state.speakers_on);
}

#[tokio::test]
async fn a_chain_that_dies_is_started_again_with_the_speakers_off_and_no_convolvers() {
    let mut rig = playing().await;
    let built = applies(&rig.chain).len();
    rig.chain.die();
    tokio::task::yield_now().await;
    rig.control.tick().await;
    let calls = applies(&rig.chain);
    assert_eq!(calls.len(), built + 1, "started again once");
    assert_eq!(
        calls.last().unwrap().speakers,
        None,
        "without the convolvers"
    );
    let view = speakers(&rig.control);
    assert!(!view.on);
    assert_eq!(view.off_reason.as_deref(), Some("chain-restarted"));

    let mut rig = rig_with_set_chosen().await;
    rig.control.speakers(spk_on()).await.unwrap();
    rig.chain.die();
    tokio::task::yield_now().await;
    rig.control.tick().await;
    assert!(!speakers(&rig.control).on);
    assert!(!state_file(&rig).on, "no chain is wanted for nothing");
    assert_eq!(applies(&rig.chain).len(), 1, "and none is started again");
}

#[tokio::test]
async fn a_new_rate_sends_the_stage_again_and_is_no_reset() {
    let mut rig = playing().await;
    let sends = rig.params.sent().len();
    rig.params.set_node(Some(vnode(1, 44_100)));
    looks(&mut rig.control, 1).await;
    assert!(speakers(&rig.control).on, "a rate change is not a reset");
    let sent = rig.params.sent();
    assert_eq!(gain2(sent.last().expect("sent")), Some(1.0));
    assert!(sent.len() > sends);
}

/// The stages run up to 384 kHz: a sink that moves above it turns the speakers off with a note
/// that says why, and turning them on there is refused in words.
#[tokio::test]
async fn above_384_khz_the_speakers_turn_off_and_are_refused() {
    let mut rig = playing().await;
    rig.params.set_node(Some(vnode(1, 705_600)));
    looks(&mut rig.control, 2).await;
    let view = speakers(&rig.control);
    assert!(!view.on, "off above 384 kHz");
    assert!(
        view.note
            .as_deref()
            .is_some_and(|note| note.contains("384 kHz")),
        "{:?}",
        view.note
    );
    // The line that turned them off carried the dry mixers (climb steps after it carry none).
    let sent = rig.params.sent();
    assert_eq!(sent.iter().rev().find_map(|send| gain2(send)), Some(0.0));
    let refused = rig.control.speakers(spk_on()).await.unwrap_err();
    assert!(refused.what.contains("384 kHz"), "{refused:?}");
}

/// With no chain running the rate is the sink's own: the speakers turned on there above 384 kHz
/// are refused, and no chain is applied for them.
#[tokio::test]
async fn the_speakers_on_with_no_chain_above_384_khz_are_refused() {
    let mut rig = rig_with_set_chosen().await;
    rig.params.set_node(Some(vnode(1, 705_600)));
    let refused = rig.control.speakers(spk_on()).await.unwrap_err();
    assert!(refused.what.contains("384 kHz"), "{refused:?}");
    assert!(applies(&rig.chain).is_empty(), "no chain for them");
    assert!(!speakers(&rig.control).on);
}

#[tokio::test]
async fn a_graph_that_never_ran_is_sent_the_stage_when_it_first_runs() {
    let mut rig = playing().await;
    let sends = rig.params.sent().len();
    // The node is listed and its graph has not started: every control reads 0.
    let mut idle = vnode(1, 48_000);
    for value in idle.params.values_mut() {
        *value = 0.0;
    }
    idle.params.insert("l01:b0".into(), 0.0);
    rig.params.set_node(Some(idle));
    looks(&mut rig.control, 1).await;
    assert!(speakers(&rig.control).on, "the view still says on");
    // The graph runs: it holds its config, the stage off, and the stage goes again.
    let mut ran = vnode(1, 48_000);
    ran.params.insert("l01:b0".into(), 1.0);
    rig.params.set_node(Some(ran));
    looks(&mut rig.control, 1).await;
    let sent = rig.params.sent();
    assert!(sent.len() > sends);
    assert_eq!(gain2(sent.last().unwrap()), Some(1.0));
}

#[tokio::test]
async fn a_revert_of_the_graph_at_the_same_node_and_rate_sends_the_stage_again() {
    let mut rig = playing().await;
    looks(&mut rig.control, 1).await;
    let sends = rig.params.sent().len();
    rig.params
        .set_ports(&[("lvs:Gain 2", 0.0), ("lvs:Gain 1", 1.0)]);
    looks(&mut rig.control, 1).await;
    assert!(speakers(&rig.control).on, "the view says on");
    let sent = rig.params.sent();
    assert!(sent.len() > sends, "the stage was sent again");
    assert_eq!(gain2(sent.last().unwrap()), Some(1.0));
}

#[tokio::test]
async fn a_chain_that_keeps_reverting_gives_the_speakers_up() {
    let mut rig = playing().await;
    looks(&mut rig.control, 1).await;
    for _ in 0..8 {
        rig.params.set_ports(&[("lvs:Gain 2", 0.0)]);
        looks(&mut rig.control, 1).await;
    }
    let view = speakers(&rig.control);
    assert!(!view.on);
    assert!(view.note.unwrap().contains("going back"));
}

#[tokio::test]
async fn a_node_found_playing_the_stage_that_this_controller_does_not_hold_is_bypassed() {
    let mut rig = rig();
    rig.control.set_on(true).await.unwrap();
    rig.params.with_speakers_ports(true);
    rig.control.edit(gain(0, 2.0, true)).await.unwrap();
    looks(&mut rig.control, 1).await;
    assert!(
        rig.params
            .sent()
            .iter()
            .any(|send| gain2(send) == Some(0.0)),
        "the bypass went out"
    );
}

#[tokio::test]
async fn the_speakers_off_with_a_stream_playing_keep_the_chain_as_a_pass_through() {
    let mut rig = rig_with_set_chosen().await;
    rig.control.speakers(spk_on()).await.unwrap();
    rig.params.set_playing(true);
    let built = applies(&rig.chain).len();
    rig.control.speakers(spk_off()).await.unwrap();
    assert_eq!(removes(&rig.chain), 0, "the sink stays");
    assert_eq!(applies(&rig.chain).len(), built, "no swap");
    assert_eq!(gain2(rig.params.sent().last().unwrap()), Some(0.0));
    assert!(rig.control.passthrough());
    assert_eq!(keeps(&rig.chain).last().unwrap().speakers, None);
    // Once nothing plays for a while, it goes.
    rig.params.set_playing(false);
    rig.control.idle_keep = 5;
    looks(&mut rig.control, 1).await;
    assert_eq!(removes(&rig.chain), 1);
}

// ----- the set's WAV -----

#[tokio::test]
async fn a_wav_that_goes_missing_before_the_apply_is_applied_without_the_stage_with_a_note() {
    let mut rig = playing().await;
    std::fs::remove_file(rig.store.wav_path("set-a").unwrap()).unwrap();
    // A rebuild with the stage on (a live update that cannot go makes the end of a drag apply).
    rig.control.live = false;
    rig.control.edit(gain(0, 3.0, true)).await.unwrap();
    let request = applies(&rig.chain).pop().unwrap();
    assert_eq!(request.speakers, None, "no config names a WAV that is gone");
    let view = speakers(&rig.control);
    assert!(!view.on);
    assert_eq!(view.off_reason.as_deref(), Some("set-missing"));
    assert!(
        view.note
            .unwrap()
            .contains("missing or damaged, fetch it again"),
        "the note says what to do"
    );
    assert!(!state_file(&rig).speakers_on);
    assert!(
        view.sets.iter().all(|set| set.id != "set-a"),
        "the list shows it as not fetched"
    );
}

#[tokio::test]
async fn a_damaged_wav_is_refused_when_chosen_or_turned_on() {
    let mut rig = rig_with_set_chosen().await;
    let path = rig.store.wav_path("set-a").unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[60] ^= 0xFF;
    std::fs::write(&path, bytes).unwrap();
    let failure = rig.control.speakers(spk_on()).await.unwrap_err();
    assert!(failure.what.contains("set-a"), "{failure:?}");
    assert!(rig.chain.calls().is_empty(), "nothing was applied");
    assert!(!speakers(&rig.control).on);
}

#[tokio::test]
async fn a_wav_that_goes_missing_while_the_chain_holds_it_rebuilds_the_chain_at_a_rate_change() {
    let mut rig = playing().await;
    let built = applies(&rig.chain).len();
    std::fs::remove_file(rig.store.wav_path("set-a").unwrap()).unwrap();
    // The sink changes rate: the chain re-opens the file and would go silent.
    rig.params.set_node(Some(vnode(1, 44_100)));
    looks(&mut rig.control, 1).await;
    let calls = applies(&rig.chain);
    assert_eq!(calls.len(), built + 1, "the chain was applied again");
    assert_eq!(calls.last().unwrap().speakers, None);
    assert!(!speakers(&rig.control).on);
    assert_eq!(
        speakers(&rig.control).off_reason.as_deref(),
        Some("set-missing")
    );

    // The same with the stage off and the convolvers idle.
    let mut rig = playing().await;
    rig.control.speakers(spk_off()).await.unwrap();
    let built = applies(&rig.chain).len();
    std::fs::remove_file(rig.store.wav_path("set-a").unwrap()).unwrap();
    rig.params.set_node(Some(vnode(1, 44_100)));
    looks(&mut rig.control, 1).await;
    assert_eq!(applies(&rig.chain).len(), built + 1);
    assert!(
        rig.control
            .view()
            .note
            .unwrap()
            .contains("could not be read")
    );
}

#[tokio::test]
async fn the_set_a_running_chain_holds_is_pinned_in_the_store_until_the_chain_goes() {
    let mut rig = playing().await;
    assert!(matches!(
        rig.store.forget("set-a"),
        Err(StoreError::InUse(_))
    ));
    // Off: the convolvers stay idle in the chain, so the WAV stays.
    rig.control.speakers(spk_off()).await.unwrap();
    assert!(matches!(
        rig.store.forget("set-a"),
        Err(StoreError::InUse(_))
    ));
    // Another set rebuilds the chain: the old one is free, the new one is pinned.
    rig.control.speakers(spk_set("set-b")).await.unwrap();
    rig.control.speakers(spk_on()).await.unwrap();
    assert!(matches!(
        rig.store.forget("set-b"),
        Err(StoreError::InUse(_))
    ));
    // The chain goes: nothing is held.
    rig.control.speakers(spk_off()).await.unwrap();
    rig.control.set_on(false).await.unwrap();
    rig.store.forget("set-b").unwrap();
}

#[tokio::test]
async fn a_failed_apply_keeps_the_old_set_pinned() {
    let mut rig = playing().await;
    rig.chain.refuse(Some(EqFailure {
        what: "no".into(),
        next: "later".into(),
    }));
    assert!(rig.control.speakers(spk_set("set-b")).await.is_err());
    rig.chain.refuse(None);
    assert_eq!(speakers(&rig.control).set.as_deref(), Some("set-a"));
    assert!(matches!(
        rig.store.forget("set-a"),
        Err(StoreError::InUse(_))
    ));
    rig.store.forget("set-b").unwrap();
}

// ----- the edges -----

#[tokio::test]
async fn the_config_on_disk_never_holds_the_stage_on_while_live_updates_work() {
    let mut rig = playing().await;
    rig.control.speakers(spk_set("set-b")).await.unwrap();
    rig.control.edit(gain(0, 2.0, true)).await.unwrap();
    rig.control.speakers(spk_off()).await.unwrap();
    rig.control.speakers(spk_on()).await.unwrap();
    for request in applies(&rig.chain).into_iter().chain(keeps(&rig.chain)) {
        assert!(
            request.speakers.is_none_or(|conf| !conf.on),
            "a live chain's config always has the stage off"
        );
    }
}

#[tokio::test]
async fn without_live_updates_the_stage_is_in_the_config_and_the_start_finds_it() {
    let mut rig = rig_with_set_chosen().await;
    rig.control.set_on(true).await.unwrap();
    rig.control.live = false;
    rig.control.speakers(spk_on()).await.unwrap();
    let request = applies(&rig.chain).pop().unwrap();
    assert!(request.speakers.unwrap().on, "the config holds the stage");
    assert!(speakers(&rig.control).on);
    assert!(state_file(&rig).speakers_on);

    // A new owner finds speakers_on and applies the chain without the stage.
    let chain = FakeChain::with_state(state_file(&rig));
    let mut again = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(FakeParams::new()),
        Keep::File(rig.dir.state()),
    )
    .with_speakers(Arc::clone(&rig.store));
    again.sync_running().await;
    assert_eq!(
        keeps(&chain).last().map(|request| request.speakers.clone()),
        Some(None)
    );
    assert!(!state_file(&rig).speakers_on);
}

#[tokio::test]
async fn a_live_send_that_does_not_take_puts_the_stage_in_the_config() {
    let mut rig = rig_with_set_chosen().await;
    rig.control.set_on(true).await.unwrap();
    rig.params.takes(false);
    rig.control.speakers(spk_on()).await.unwrap();
    let request = applies(&rig.chain).pop().unwrap();
    assert!(request.speakers.unwrap().on, "the config holds it instead");
    assert!(speakers(&rig.control).on);
}

#[tokio::test]
async fn a_chain_from_before_the_stage_is_rebuilt_once_to_load_the_set() {
    let mut rig = rig_with_set_chosen().await;
    rig.control.set_on(true).await.unwrap();
    rig.params.without_speakers_ports();
    let built = applies(&rig.chain).len();
    rig.params.with_speakers_ports(false);
    rig.control.speakers(spk_on()).await.unwrap();
    assert_eq!(applies(&rig.chain).len(), built + 1);
    assert!(speakers(&rig.control).note.unwrap().contains("rebuilt"));
}

#[tokio::test]
async fn the_speakers_are_refused_in_words_where_the_chain_cannot_take_them() {
    let mut rig = rig_with_set_chosen().await;
    rig.control.set_on(true).await.unwrap();
    rig.control.legacy = true;
    let failure = rig.control.speakers(spk_on()).await.unwrap_err();
    assert!(failure.what.contains("older svolder"), "{failure:?}");
    rig.control.legacy = false;
    rig.control.held = true;
    assert!(rig.control.speakers(spk_on()).await.is_err());
    rig.control.held = false;
    rig.chain.refuse(Some(EqFailure {
        what: "no pipewire".into(),
        next: "install it".into(),
    }));
    let failure = rig.control.speakers(spk_on()).await.unwrap_err();
    assert!(failure.what.contains("no pipewire"));
    assert!(!speakers(&rig.control).on);
}

#[tokio::test]
async fn a_container_restart_with_the_chain_gone_applies_the_eq_again_without_the_speakers() {
    let mut rig = playing().await;
    rig.chain.die();
    // The owner is told the chain is gone, as `sync_running` finds it at start.
    let dir_state = state_file(&rig);
    assert!(dir_state.speakers_on);
    let off = rig.control.sync_running().await;
    assert!(off, "the EQ is seen as not running");
    assert!(!speakers(&rig.control).on);
    // The container's start finds the state on and no chain: the EQ is applied again, without
    // the convolvers.
    let built = applies(&rig.chain).len();
    let mut again = EqControl::new(
        Arc::new(rig.chain.clone()),
        Box::new(rig.params.clone()),
        Keep::File(rig.dir.state()),
    )
    .with_speakers(Arc::clone(&rig.store));
    assert!(again.reapply_if_on().await.unwrap());
    assert!(!speakers(&again).on);
    let calls = applies(&rig.chain);
    assert_eq!(calls.len(), built + 1);
    assert_eq!(calls.last().unwrap().speakers, None);
}

#[tokio::test]
async fn a_reload_of_the_state_from_outside_drops_the_speakers_that_played() {
    let mut rig = playing().await;
    // The command line wrote the state with no speakers on while they played.
    let mut state = state_file(&rig);
    state.speakers_on = false;
    state.save(&rig.dir.state()).unwrap();
    assert!(rig.control.reload_if_changed());
    let view = speakers(&rig.control);
    assert!(!view.on);
    assert_eq!(view.off_reason.as_deref(), Some("chain-restarted"));
}

// ----- the pin across a rebuild, retries -----

/// A forget that runs while the chain is being applied with `set-b`: what it answered.
fn forget_a_while_b_applies(rig: &Rig, refuse: bool) -> Arc<Mutex<Vec<Result<(), StoreError>>>> {
    let answers = Arc::new(Mutex::new(Vec::new()));
    let (store, seen) = (Arc::clone(&rig.store), Arc::clone(&answers));
    rig.hook.set(move |call| {
        let ChainCall::Apply(request) = call else {
            return None;
        };
        if request
            .speakers
            .as_ref()
            .is_none_or(|conf| conf.id != "set-b")
        {
            return None;
        }
        seen.lock().unwrap().push(store.forget("set-a"));
        refuse.then(no)
    });
    answers
}

#[tokio::test]
async fn a_forget_racing_another_set_s_apply_is_refused_whether_the_apply_takes_or_not() {
    for refuse in [false, true] {
        // Another set while they play.
        let mut rig = playing().await;
        let answers = forget_a_while_b_applies(&rig, refuse);
        let result = rig.control.speakers(spk_set("set-b")).await;
        assert_eq!(result.is_err(), refuse);
        assert_eq!(
            *answers.lock().unwrap(),
            [Err(StoreError::InUse("set-a".into()))],
            "the old set is pinned until the chain that plays it is replaced (refuse {refuse})"
        );
        assert!(rig.store.wav_path("set-a").unwrap().exists());
        let (held, free) = match refuse {
            false => ("set-b", "set-a"),
            true => ("set-a", "set-b"),
        };
        rig.hook.set(|_| None);
        assert!(matches!(rig.store.forget(held), Err(StoreError::InUse(_))));
        rig.store.forget(free).unwrap();

        // Another set turned on while the idle convolvers hold the first.
        let mut rig = playing().await;
        rig.control.speakers(spk_off()).await.unwrap();
        rig.control.speakers(spk_set("set-b")).await.unwrap();
        let answers = forget_a_while_b_applies(&rig, refuse);
        let result = rig.control.speakers(spk_on()).await;
        assert_eq!(result.is_err(), refuse);
        assert_eq!(
            *answers.lock().unwrap(),
            [Err(StoreError::InUse("set-a".into()))],
            "the idle set is pinned until the chain that holds it is replaced (refuse {refuse})"
        );
        assert!(rig.store.wav_path("set-a").unwrap().exists());
    }
}

#[tokio::test]
async fn a_new_owner_pins_the_set_the_chain_holds_when_another_was_chosen_while_off() {
    let mut rig = playing().await;
    rig.control.speakers(spk_off()).await.unwrap();
    rig.control.speakers(spk_set("set-b")).await.unwrap();
    // The owner starts again (a daemon restart, a TUI quit and reopened) over the same state,
    // chain and directory: the chain still holds set-a's convolvers, idle.
    let store = Arc::new(Store::new(rig.store.dir()));
    let mut again = EqControl::new(
        Arc::new(rig.chain.clone()),
        Box::new(rig.params.clone()),
        Keep::File(rig.dir.state()),
    )
    .with_speakers(Arc::clone(&store));
    again.sync_running().await;
    looks(&mut again, 1).await;
    assert!(
        matches!(store.forget("set-a"), Err(StoreError::InUse(_))),
        "the set the chain holds is pinned, not the one chosen"
    );
    store.forget("set-b").unwrap();
    // Its WAV goes anyway (by hand): a rate change re-opens it, so the chain is rebuilt.
    std::fs::remove_file(store.wav_path("set-a").unwrap()).unwrap();
    let built = applies(&rig.chain).len();
    rig.params.set_node(Some(vnode(1, 44_100)));
    looks(&mut again, 1).await;
    let calls = applies(&rig.chain);
    assert_eq!(calls.len(), built + 1, "the chain was applied again");
    assert_eq!(calls.last().unwrap().speakers, None);
}

#[tokio::test]
async fn a_failed_apply_after_a_failed_check_takes_the_stage_out_once_the_chain_takes_applies() {
    let mut rig = playing().await;
    std::fs::remove_file(rig.store.wav_path("set-a").unwrap()).unwrap();
    rig.control.live = false;
    let mut left = 1;
    rig.hook.set(move |call| {
        let refused = matches!(call, ChainCall::Apply(_)) && left > 0;
        left -= usize::from(refused);
        refused.then(no)
    });
    assert!(rig.control.edit(gain(0, 3.0, true)).await.is_err());
    let view = speakers(&rig.control);
    assert!(!view.on);
    assert_eq!(view.off_reason.as_deref(), Some("set-missing"));
    // The running chain still has the stage on: it is taken out once the chain takes an apply.
    let built = applies(&rig.chain).len();
    looks(&mut rig.control, 3).await;
    let calls = applies(&rig.chain);
    assert_eq!(calls.len(), built + 1, "applied again, once");
    assert_eq!(calls.last().unwrap().speakers, None);
}

#[tokio::test]
async fn a_rebuild_for_a_missing_set_that_fails_is_tried_again_until_it_takes() {
    for on in [true, false] {
        let mut rig = playing().await;
        if !on {
            rig.control.speakers(spk_off()).await.unwrap();
        }
        std::fs::remove_file(rig.store.wav_path("set-a").unwrap()).unwrap();
        let mut left = 2;
        rig.hook.set(move |call| {
            let refused = matches!(call, ChainCall::Apply(_)) && left > 0;
            left -= usize::from(refused);
            refused.then(no)
        });
        let built = applies(&rig.chain).len();
        rig.params.set_node(Some(vnode(1, 44_100)));
        looks(&mut rig.control, 4).await;
        let calls = applies(&rig.chain);
        assert_eq!(calls.len(), built + 1, "tried until it took (on {on})");
        assert_eq!(calls.last().unwrap().speakers, None);
        assert!(!speakers(&rig.control).on);
        // Once taken, nothing more is tried.
        looks(&mut rig.control, 2).await;
        assert_eq!(applies(&rig.chain).len(), built + 1);
    }
}

#[tokio::test]
async fn a_config_rewrite_that_fails_is_tried_once_a_second() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    crashed_with_speakers_on(&dir, false);
    let inner = FakeChain::with_state(HostEqState::load(&dir.state()).unwrap().unwrap());
    let chain = HookChain::over(inner);
    let writes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&writes);
    chain.set(move |call| {
        let write = matches!(call, ChainCall::KeepConf(_));
        counted.fetch_add(usize::from(write), Ordering::SeqCst);
        write.then(no)
    });
    let params = FakeParams::new();
    params.with_speakers_ports(true);
    let mut control = EqControl::new(Arc::new(chain), Box::new(params), Keep::File(dir.state()))
        .with_speakers(store);
    control.sync_running().await;
    let before = writes.load(Ordering::SeqCst);
    looks(&mut control, 2).await;
    let tries = writes.load(Ordering::SeqCst) - before;
    assert!(
        (1..=3).contains(&tries),
        "about once a second, not at every tick: {tries} in 2 s"
    );
}

#[tokio::test]
async fn a_crossfeed_start_that_fails_gives_the_speakers_back_as_they_were() {
    let mut rig = playing().await;
    let before = speakers(&rig.control);
    // The send does not take and the apply that would hold the stage in the config fails.
    rig.params.takes(false);
    rig.hook
        .set(|call| matches!(call, ChainCall::Apply(_)).then(no));
    assert!(rig.control.crossfeed(xf_edit(true)).await.is_err());
    let view = speakers(&rig.control);
    assert!(view.on, "the chain still plays them");
    assert_eq!(view.note, before.note);
    assert_eq!(rig.control.sp.for_output.as_deref(), Some("a"));
}

#[tokio::test]
async fn a_crossfeed_start_that_rebuilt_the_chain_and_failed_leaves_the_speakers_off_in_words() {
    let mut rig = playing().await;
    // A chain from before the crossfeed stage is rebuilt (without the convolvers) to add it;
    // the send then does not take, and the apply that would hold the stage fails.
    rig.params.without_crossfeed_ports();
    rig.params.takes(false);
    let mut applied = 0;
    let params = rig.params.clone();
    rig.hook.set(move |call| {
        let apply = matches!(call, ChainCall::Apply(_));
        applied += usize::from(apply);
        // The chain built without the convolvers lists none of the speakers' controls.
        params.without_speakers_ports();
        (apply && applied > 1).then(no)
    });
    assert!(rig.control.crossfeed(xf_edit(true)).await.is_err());
    assert_eq!(applies(&rig.chain).last().unwrap().speakers, None);
    let view = speakers(&rig.control);
    assert!(!view.on, "the chain has no convolvers: nothing plays");
    assert!(view.note.unwrap().contains("rebuilt without them"));
}

#[tokio::test]
async fn the_list_read_on_the_clock_finds_new_sets_and_keeps_damaged_ones_hidden() {
    let mut rig = rig();
    let path = rig.store.wav_path("set-b").unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[60] ^= 0xFF;
    std::fs::write(&path, bytes).unwrap();
    rig.control.sets_changed();
    put_set(&rig.store, "set-c", "Dummy head, C");
    looks(&mut rig.control, 11).await;
    let ids: Vec<_> = speakers(&rig.control)
        .sets
        .iter()
        .map(|set| set.id.clone())
        .collect();
    assert_eq!(ids, ["set-a", "set-c"]);
}

/// A chain that runs for the speakers alone plays the EQ flat, so its kept points are not checked
/// at a new rate: the chain and the speakers stay, a set chosen there rebuilds it, turning the EQ
/// on is refused, and the state keeps the preamp the points were saved with.
#[tokio::test]
async fn speakers_alone_stay_on_at_a_rate_their_kept_points_cannot_play_at() {
    let mut rig = rig();
    rig.control
        .load("Shelf".into(), None, super::passthrough::shelf())
        .await
        .unwrap();
    rig.control.set_on(false).await.unwrap();
    rig.control.speakers(spk_set("set-a")).await.unwrap();
    rig.control.speakers(spk_on()).await.unwrap();
    assert!(applies(&rig.chain).last().expect("applied").flat);
    let gone = removes(&rig.chain);
    rig.params.set_node(Some(vnode(1, 192_000)));
    looks(&mut rig.control, 1).await;
    assert_eq!(removes(&rig.chain), gone, "the speakers' chain stays");
    let note = rig.control.view().note.unwrap_or_default();
    assert!(!note.contains("computer EQ turned off"), "{note}");
    assert!(speakers(&rig.control).on);
    assert_eq!(rig.control.view().points[0].g, 14.5, "the points are kept");
    // A set chosen at that rate rebuilds the flat chain: its request is not refused for points
    // it does not play, and states no preamp above 0 dB.
    rig.control.speakers(spk_set("set-b")).await.unwrap();
    let rebuilt = applies(&rig.chain).last().cloned().expect("applied");
    assert!(rebuilt.flat && rebuilt.rate_hz == Some(192_000));
    assert!(rebuilt.correction.preamp_db <= 0.0);
    assert!(speakers(&rig.control).on);
    // Turning the EQ on there is refused, and nothing it cannot play is sent.
    let refused = rig.control.set_on(true).await.unwrap_err();
    assert!(refused.what.contains("point 1"), "{}", refused.what);
    nothing_past_ten(&rig.params);
    assert!(speakers(&rig.control).on);
    let state = state_file(&rig);
    assert!(state.speakers_on && state.flat);
    assert!(
        state.preamp_db > -20.0,
        "the saved preamp is a curve's, not the floor: {}",
        state.preamp_db
    );
}

/// The speakers turned off on a chain that runs flat for them while loudness is on: the chain
/// stays (loudness keeps it), the speakers go off live.
#[tokio::test]
async fn speakers_off_with_loudness_on_keeps_the_chain() {
    let mut rig = rig();
    rig.params.with_loudness_ports();
    rig.control.speakers(spk_set("set-a")).await.unwrap();
    rig.control.speakers(spk_on()).await.unwrap();
    assert!(rig.control.runs && rig.control.flat);
    rig.control
        .loudness(LoudnessEdit {
            on: Some(true),
            done: true,
            ..LoudnessEdit::default()
        })
        .await
        .unwrap();
    rig.control.speakers(spk_off()).await.unwrap();
    assert_eq!(removes(&rig.chain), 0, "loudness keeps the chain");
    assert!(rig.control.runs && rig.control.flat);
    assert!(!rig.control.view().speakers.unwrap().on);
    looks(&mut rig.control, 40).await;
    assert_eq!(removes(&rig.chain), 0, "not a pass-through that idles away");
}
