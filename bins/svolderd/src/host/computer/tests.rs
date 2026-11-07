// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use svolder_host::eq::{HostEqState, Manager, Route};
use svolder_host::fake::{FakeChain, FakeParams, FakePlayers, FakeSink, SinkWrite};
use svolder_host::{Sink, SinkState};

use super::super::tests::{pass, settle};
use super::*;
use crate::host::{HostService, Parts, found};
use crate::locks::locked;

/// The sink's name in every test.
const SINK: &str = "Schiit Gungnir 2";

/// A computer over fakes, kept so a test can look at what was written to each.
struct Rig {
    service: HostService,
    sink: FakeSink,
    chain: FakeChain,
    params: FakeParams,
}

/// A scratch directory for a state file, taken away with the test.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "svolderd-computer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Dir(path)
    }

    fn state(&self) -> PathBuf {
        self.0.join("host-eq.toml")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sink_state(volume: f32) -> SinkState {
    SinkState {
        volume,
        muted: false,
        name: SINK.to_string(),
        target: None,
    }
}

fn computer_over(sink: &FakeSink, chain: &FakeChain, params: &FakeParams, keep: Keep) -> Computer {
    let sink: Arc<dyn Sink> = Arc::new(sink.clone());
    let eq = EqControl::new(Arc::new(chain.clone()), Box::new(params.clone()), keep);
    Computer::new(Ok(sink), Ok(eq))
}

fn parts(computer: Computer) -> Parts {
    Parts {
        connect: found(Arc::new(FakePlayers::new(None))),
        routes: None,
        covers: Arc::new(|_| None),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
        computer: Some(Arc::new(computer)),
    }
}

fn rig_keeping(keep: Keep) -> Rig {
    let (sink, chain, params) = (
        FakeSink::new(Some(sink_state(0.42))),
        FakeChain::new(),
        FakeParams::new(),
    );
    let service = HostService::idle();
    service.run_with(parts(computer_over(&sink, &chain, &params, keep)));
    Rig {
        service,
        sink,
        chain,
        params,
    }
}

fn rig() -> Rig {
    rig_keeping(Keep::Memory)
}

impl Rig {
    /// Run a new computer over the same fakes, as a reload that rebuilds it does.
    fn restart(&self, keep: Keep) {
        self.service.run_with(parts(computer_over(
            &self.sink,
            &self.chain,
            &self.params,
            keep,
        )));
    }

    async fn eq(&self, op: HostEqOp) -> Result<(), String> {
        self.service
            .host_eq(op, &[], |_| Vec::new(), |_| async { Ok(()) })
            .await
            .map_err(|refused| refused.to_string())
    }

    /// Whether anything at all was written to the fake sink, chain or `pw-cli`.
    fn wrote(&self) -> bool {
        !self.sink.writes().is_empty()
            || !self.chain.calls().is_empty()
            || !self.params.lines().is_empty()
            || !self.params.sent().is_empty()
    }
}

fn gain(g: f64, done: bool) -> HostEqOp {
    HostEqOp::Set {
        index: 1,
        slot: None,
        kind: None,
        f: None,
        g: Some(g),
        q: None,
        done,
    }
}

#[tokio::test(start_paused = true)]
async fn the_host_view_carries_volume_and_eq() {
    let rig = rig();
    let _watching = rig.service.watching();
    settle().await;

    let view = rig.service.view();
    assert_eq!(
        view.volume,
        Some(HostVolume {
            pct: 42,
            muted: false,
            sink: SINK.to_string()
        })
    );
    let eq = view.eq.expect("the EQ's view");
    assert!(!eq.on);
    assert_eq!(eq.points.len(), 1, "point 1 is always there");
    assert_eq!(eq.max, 16);
    assert_eq!(eq.unavailable, None);
}

#[tokio::test(start_paused = true)]
async fn a_volume_above_100_is_refused() {
    let rig = rig();
    settle().await;
    for pct in [101, 120, 255] {
        let why = rig.service.host_volume(pct).await.unwrap_err();
        assert_eq!(why, "the computer's volume goes up to 100 %");
    }
    assert!(
        rig.sink.writes().is_empty(),
        "a refused write sends nothing"
    );
}

#[tokio::test(start_paused = true)]
async fn a_volume_write_reads_back() {
    let rig = rig();
    let _watching = rig.service.watching();
    settle().await;

    let volume = rig.service.host_volume(30).await.expect("taken");
    assert_eq!(volume.pct, 30);
    assert_eq!(rig.sink.writes(), [SinkWrite::Volume(0.30)]);
    assert_eq!(
        rig.service.view().volume.map(|v| v.pct),
        Some(30),
        "a watcher is told what was read back"
    );
    assert_eq!(rig.service.host_volume(0).await.expect("taken").pct, 0);
    assert_eq!(rig.service.host_volume(100).await.expect("taken").pct, 100);
}

#[tokio::test(start_paused = true)]
async fn a_boosted_sink_reads_as_it_is() {
    let rig = rig();
    rig.sink.set(Some(sink_state(1.2)));
    let _watching = rig.service.watching();
    settle().await;
    assert_eq!(rig.service.view().volume.map(|v| v.pct), Some(120));

    let volume = rig.service.host_volume(100).await.expect("100 is accepted");
    assert_eq!(volume.pct, 100);
    assert_eq!(rig.sink.writes(), [SinkWrite::Volume(1.0)]);
    assert!(rig.service.host_volume(101).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn mute_sets_and_clears() {
    let rig = rig();
    settle().await;
    assert!(rig.service.host_mute(true).await.expect("taken").muted);
    assert!(!rig.service.host_mute(false).await.expect("taken").muted);
    assert_eq!(
        rig.sink.writes(),
        [SinkWrite::Mute(true), SinkWrite::Mute(false)]
    );
}

#[tokio::test(start_paused = true)]
async fn nobody_watching_reads_no_volume() {
    let rig = rig();
    pass(Duration::from_secs(10)).await;
    assert_eq!(rig.sink.reads(), 0);
    assert_eq!(rig.service.view().volume, None);

    let watching = rig.service.watching();
    settle().await;
    assert_eq!(rig.sink.reads(), 1, "the first watcher is read for at once");
    pass(Duration::from_secs(2)).await;
    assert_eq!(rig.sink.reads(), 2, "then every 2 s");
    pass(Duration::from_secs(1)).await;
    assert_eq!(rig.sink.reads(), 2, "and no faster");

    drop(watching);
    settle().await;
    let reads = rig.sink.reads();
    pass(Duration::from_secs(10)).await;
    assert_eq!(rig.sink.reads(), reads, "the last one gone, the reads stop");
    assert_eq!(rig.service.view().volume, None, "and nothing stale is kept");
}

#[tokio::test(start_paused = true)]
async fn two_watchers_see_a_drag_at_most_ten_times_a_second_and_its_end() {
    let rig = rig();
    rig.eq(HostEqOp::Mode { on: true }).await.expect("on");
    settle().await;
    let computer = locked_computer(&rig.service);

    // What the service hands every watcher, counted at the source with the clock it keeps.
    let sent: Arc<StdMutex<Vec<(Instant, EqView)>>> = Arc::default();
    let record = Arc::clone(&sent);
    let mirror = tokio::spawn({
        let computer = Arc::clone(&computer);
        async move {
            computer
                .mirror(move |view| record.lock().unwrap().push((Instant::now(), view)))
                .await;
        }
    });
    // Two watchers of the host view, each counting the views it is woken for.
    let counts: Vec<Arc<AtomicUsize>> = (0..2).map(|_| Arc::default()).collect();
    let ends: Vec<Arc<StdMutex<Option<f64>>>> = (0..2).map(|_| Arc::default()).collect();
    let mut tasks = Vec::new();
    for (count, end) in counts.iter().zip(&ends) {
        let (count, end) = (Arc::clone(count), Arc::clone(end));
        let mut changes = rig.service.changes();
        tasks.push(tokio::spawn(async move {
            while changes.changed().await.is_ok() {
                count.fetch_add(1, Ordering::SeqCst);
                let view = changes.borrow_and_update().clone();
                *end.lock().unwrap() = view.eq.and_then(|eq| eq.points.first().map(|p| p.g));
            }
        }));
    }
    settle().await;
    let started = Instant::now();
    sent.lock().unwrap().clear();
    counts
        .iter()
        .for_each(|count| count.store(0, Ordering::SeqCst));

    // A drag at 100 values a second for 1.5 s, and its end.
    for step in 1..=150u32 {
        let g = f64::from(step) * 0.05;
        rig.eq(gain(g, false)).await.expect("a drag step");
        pass(Duration::from_millis(10)).await;
    }
    rig.eq(gain(8.0, true)).await.expect("the drag ends");
    pass(Duration::from_millis(500)).await;
    let elapsed = started.elapsed();

    let sent = sent.lock().unwrap().clone();
    for pair in sent.windows(2) {
        assert!(
            pair[1].0 - pair[0].0 >= VIEW_EVERY,
            "two views closer than {VIEW_EVERY:?}"
        );
    }
    let allowed = (elapsed.as_millis() / VIEW_EVERY.as_millis()) as usize + 1;
    assert!(
        (5..=allowed).contains(&sent.len()),
        "{} views in {elapsed:?}",
        sent.len()
    );
    assert_eq!(
        sent.last()
            .and_then(|(_, view)| view.points.first())
            .map(|p| p.g),
        Some(8.0),
        "the end of the drag is always sent"
    );
    for (count, end) in counts.iter().zip(&ends) {
        assert!(
            count.load(Ordering::SeqCst) <= allowed,
            "a watcher was woken too often"
        );
        assert_eq!(
            *end.lock().unwrap(),
            Some(8.0),
            "every watcher sees the end"
        );
    }
    mirror.abort();
    tasks.iter().for_each(|task| task.abort());
}

/// The service's computer, as its tasks hold it.
fn locked_computer(service: &HostService) -> Arc<Computer> {
    service.computer().expect("a running computer")
}

#[tokio::test(start_paused = true)]
async fn starting_or_reloading_changes_no_audio() {
    let dir = Dir::new();
    let rig = rig_keeping(Keep::File(dir.state()));
    settle().await;
    let watching = rig.service.watching();
    pass(Duration::from_secs(5)).await;
    drop(watching);
    let _ = rig.service.view_now().await;
    assert!(
        !rig.wrote(),
        "starting and a watcher coming and going write nothing"
    );

    // With the EQ on, a reload that rebuilds the computer and a daemon start write nothing more.
    rig.eq(HostEqOp::Mode { on: true }).await.expect("on");
    rig.eq(gain(3.0, true)).await.expect("a point moved");
    settle().await;
    let (calls, sent, lines) = (
        rig.chain.calls().len(),
        rig.params.sent().len(),
        rig.params.lines().len(),
    );
    let writes = rig.sink.writes().len();
    rig.restart(Keep::File(dir.state()));
    settle().await;
    let watching = rig.service.watching();
    pass(Duration::from_secs(5)).await;
    drop(watching);
    assert_eq!(rig.chain.calls().len(), calls, "the chain was not touched");
    assert_eq!(rig.params.sent().len(), sent);
    assert_eq!(rig.params.lines().len(), lines);
    assert_eq!(rig.sink.writes().len(), writes);
}

/// A computer over fresh fakes (a chain that runs nothing) and the state file in `dir`, started
/// as a daemon in a container (or not) starts it.
fn started(dir: &Dir, container: bool) -> Rig {
    let (sink, chain, params) = (
        FakeSink::new(Some(sink_state(0.42))),
        FakeChain::new(),
        FakeParams::new(),
    );
    let service = HostService::idle();
    let computer = computer_over(&sink, &chain, &params, Keep::File(dir.state()));
    service.run_with_in(parts(computer), container);
    Rig {
        service,
        sink,
        chain,
        params,
    }
}

#[tokio::test(start_paused = true)]
async fn in_a_container_the_chain_is_applied_again_only_when_it_was_on() {
    // Saved as on, no chain running: applied, and the view says on.
    let on = Dir::new();
    save(&on, true);
    let rig = started(&on, true);
    settle().await;
    assert_eq!(rig.chain.calls().len(), 1);
    assert!(rig.service.view().eq.unwrap().on);

    // The host going off and coming back on is a start of the computer side again: a chain that
    // ended with the old one is applied again.
    rig.service.stop();
    let again = started(&on, true);
    settle().await;
    assert_eq!(
        again.chain.calls().len(),
        1,
        "applied again after the toggle"
    );
    assert!(again.service.view().eq.unwrap().on);

    // Saved as off, or never saved: nothing.
    let off = Dir::new();
    save(&off, false);
    let saved_off = started(&off, true);
    let fresh = self::rig();
    settle().await;
    assert!(saved_off.chain.calls().is_empty() && fresh.chain.calls().is_empty());
    assert!(fresh.params.sent().is_empty());
}

#[tokio::test(start_paused = true)]
async fn outside_a_container_the_same_toggle_applies_nothing_and_the_view_says_off() {
    let on = Dir::new();
    save(&on, true);
    for _ in 0..2 {
        let rig = started(&on, false);
        settle().await;
        assert!(!rig.wrote(), "nothing is applied");
        assert!(
            !rig.service.view().eq.unwrap().on,
            "the view never says on while no chain runs"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_second_watcher_joining_or_leaving_reads_nothing_more() {
    let rig = rig();
    let first = rig.service.watching();
    settle().await;
    assert_eq!(rig.sink.reads(), 1);
    let second = rig.service.watching();
    settle().await;
    drop(first);
    settle().await;
    assert_eq!(rig.sink.reads(), 1, "only 0 to n reads at once");
    drop(second);
}

#[tokio::test(start_paused = true)]
async fn volume_writes_are_coalesced_to_the_newest_value_at_most_ten_a_second() {
    let rig = rig();
    let service = Arc::new(rig.service);
    let mut tasks = Vec::new();
    for step in 0..20u8 {
        let service = Arc::clone(&service);
        tasks.push(tokio::spawn(async move {
            service.host_volume(5 + step * 4).await
        }));
        pass(Duration::from_millis(10)).await;
    }
    for task in tasks {
        task.await.unwrap().expect("answered");
    }
    let writes = rig.sink.writes();
    assert!(writes.len() <= 4, "{writes:?}");
    assert_eq!(
        writes.last(),
        Some(&SinkWrite::Volume(0.81)),
        "the last value is written"
    );
}

/// Several devices play the preset: the refusal names all of them and a yes flattens all.
#[tokio::test(start_paused = true)]
async fn several_holders_are_all_named_and_all_flattened() {
    let rig = rig();
    let eq = svolder_profiles::Registry::builtin()
        .unwrap()
        .get("mesh-eq-5")
        .unwrap()
        .eq
        .clone()
        .unwrap();
    let presets = preset::builtins(&eq);
    let holders = |_: &str| vec!["amp".to_string(), "dac".to_string()];
    let ask = |confirm| HostEqOp::Preset {
        name: "Warm".into(),
        device: None,
        confirm,
    };
    let refused = rig
        .service
        .host_eq(ask(false), &presets, holders, |_| async { Ok(()) })
        .await
        .unwrap_err();
    assert_eq!(
        refused,
        EqRefusal::Both {
            name: "Warm".into(),
            devices: vec!["amp".into(), "dac".into()],
        }
    );
    assert_eq!(
        refused.to_string(),
        "Warm is on the amp and the dac too: the correction would play twice"
    );
    let flattened: Arc<StdMutex<Vec<String>>> = Arc::default();
    let record = Arc::clone(&flattened);
    rig.service
        .host_eq(ask(true), &presets, holders, move |names| async move {
            record.lock().unwrap().extend(names);
            Ok(())
        })
        .await
        .expect("applied");
    assert_eq!(*flattened.lock().unwrap(), ["amp", "dac"]);
}

/// A reload where no session bus answers (a container) finishes the following task each time;
/// it is started again alone. The computer, its chain and its view stay as they are: nothing is
/// removed or applied, and the host view does not change.
#[tokio::test(start_paused = true)]
async fn a_reload_with_no_session_bus_keeps_the_computer_and_its_chain() {
    let rig = rig();
    let mut absent = parts(computer_over(
        &rig.sink,
        &rig.chain,
        &rig.params,
        Keep::Memory,
    ));
    absent.connect = Arc::new(|| Box::pin(async { Err(crate::host::NoBus::Absent) }));
    rig.service.run_with_in(absent, true);
    locked(&rig.service.running).wanted = Some((true, crate::host::HostMode::Real));
    settle().await;
    rig.eq(HostEqOp::Mode { on: true }).await.expect("on");
    settle().await;
    let applied = rig.chain.calls().len();
    assert_eq!(applied, 1);
    let computer = rig.service.computer().expect("a computer");
    let mut view = rig.service.shared.view.subscribe();
    view.mark_unchanged();
    for _ in 0..3 {
        rig.service.reconfigure(
            &svolder_ipc::config::HostConfig::default(),
            crate::host::HostMode::Real,
        );
        settle().await;
    }
    assert_eq!(rig.chain.calls().len(), applied, "no remove, no apply");
    assert!(
        Arc::ptr_eq(&computer, &rig.service.computer().unwrap()),
        "the same computer keeps running"
    );
    assert!(!view.has_changed().unwrap(), "the host view never dropped");
    assert!(rig.service.view().eq.is_some_and(|eq| eq.on));
}

/// A yes to the question flattens the devices first; a computer that then refuses the preset
/// says the devices were flattened, so the caller knows what to put back.
#[tokio::test(start_paused = true)]
async fn a_refusal_after_the_flattening_says_the_devices_were_flattened() {
    let rig = rig();
    let eq = svolder_profiles::Registry::builtin()
        .unwrap()
        .get("mesh-eq-5")
        .unwrap()
        .eq
        .clone()
        .unwrap();
    let presets = preset::builtins(&eq);
    let holders = |_: &str| vec!["dac".to_string()];
    rig.chain.refuse(Some(svolder_host::eq::EqFailure {
        what: "the chain would not start".into(),
        next: "Try again".into(),
    }));
    let ask = |confirm| HostEqOp::Preset {
        name: "Warm".into(),
        device: None,
        confirm,
    };
    let why = rig
        .service
        .host_eq(ask(true), &presets, holders, |_| async { Ok(()) })
        .await
        .unwrap_err()
        .to_string();
    assert!(why.contains("the chain would not start"), "{why}");
    assert!(why.contains("The dac was flattened"), "{why}");
    assert!(why.contains("svolder eq load Warm"), "{why}");
    // With no device to flatten the refusal is the controller's alone.
    let why = rig
        .service
        .host_eq(
            ask(true),
            &presets,
            |_: &str| Vec::new(),
            |_| async { Ok(()) },
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(!why.contains("flattened"), "{why}");
}

/// A saved state with one point, as on or off.
fn save(dir: &Dir, on: bool) {
    HostEqState {
        on,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: Route::Smart,
        manager: Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![SourceFilter {
            kind: FilterKind::Peaking,
            f: 100.0,
            g: 2.0,
            q: 1.0,
        }],
        slots: vec![1],
    }
    .save(&dir.state())
    .expect("the state is saved");
}

#[tokio::test(start_paused = true)]
async fn no_tool_says_what_is_missing_in_the_view() {
    let sink: Result<Arc<dyn Sink>, String> = Err("no wpctl on this computer".to_string());
    let service = HostService::idle();
    service.run_with(parts(Computer::new(
        sink,
        Err("no pw-cli on this computer".to_string()),
    )));
    settle().await;
    let eq = service.view_now().await.eq.expect("a view");
    assert_eq!(
        eq.unavailable.as_deref(),
        Some("computer controls unavailable: no pw-cli on this computer")
    );
    assert_eq!(
        service.host_volume(10).await.unwrap_err(),
        "computer controls unavailable: no wpctl on this computer"
    );
    assert_eq!(
        service
            .host_eq(
                HostEqOp::Mode { on: true },
                &[],
                |_| Vec::new(),
                |_| async { Ok(()) }
            )
            .await
            .unwrap_err()
            .to_string(),
        "computer controls unavailable: no pw-cli on this computer"
    );
}

#[tokio::test(start_paused = true)]
async fn an_unknown_chain_status_keeps_the_state_and_says_it_is_unknown() {
    let dir = Dir::new();
    save(&dir, true);
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    let (sink, chain, params) = (
        FakeSink::new(Some(sink_state(0.42))),
        FakeChain::with_state(state),
        FakeParams::new(),
    );
    chain.cannot_tell(true);
    let service = HostService::idle();
    service.run_with_in(
        parts(computer_over(
            &sink,
            &chain,
            &params,
            Keep::File(dir.state()),
        )),
        true,
    );
    settle().await;
    let eq = service.view().eq.unwrap();
    assert!(eq.on && eq.running_unknown, "{eq:?}");
    assert!(
        chain.calls().is_empty(),
        "nothing is applied over a chain that may run"
    );
}

#[tokio::test(start_paused = true)]
async fn the_newest_volume_is_written_though_its_requester_goes_away() {
    let rig = rig();
    let service = Arc::new(rig.service);
    let first = service.host_volume(10).await;
    assert!(first.is_ok());
    let asking = {
        let service = Arc::clone(&service);
        tokio::spawn(async move { service.host_volume(70).await })
    };
    settle().await;
    asking.abort();
    pass(Duration::from_millis(300)).await;
    assert_eq!(rig.sink.writes().last(), Some(&SinkWrite::Volume(0.70)));
}

#[tokio::test(start_paused = true)]
async fn a_mute_makes_a_later_volume_answer_read_the_sink_again() {
    let rig = rig();
    assert!(!rig.service.host_volume(20).await.unwrap().muted);
    rig.service.host_mute(true).await.unwrap();
    let volume = rig.service.host_volume(20).await.unwrap();
    assert!(volume.muted, "an answer never reports a stale mute");
}
