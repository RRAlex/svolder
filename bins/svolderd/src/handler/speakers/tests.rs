// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The speakers ops through the daemon, against a loopback server: no test reaches a network and
//! none touches the person's sets (every store is a temporary directory).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use svolder_autoeq::test_server::{Reply, Server};
use svolder_ipc::{
    Event, Handler, JobEvent, JobState, Request, SpeakersCatalogue, SpeakersSaved, SpeakersSet,
};
use svolder_speakers::{Client, Limits, Store, wav};

use super::*;
use crate::handler::tests::daemon_over;

/// A test's directory in the temp dir, removed when it drops.
struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "svolderd-speakers-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Dir(path)
    }

    fn store(&self) -> PathBuf {
        self.0.join("speakers")
    }

    /// The names in the store's directory.
    fn left(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.store())
            .map(|dir| {
                dir.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dirac(at: usize, gain: f32) -> Vec<f32> {
    let mut v = vec![0.0; 400];
    v[at] = gain;
    v
}

/// A 4-channel WAV: a set the importer takes as it is.
fn set_wav(first: usize) -> Vec<u8> {
    wav::write(
        48000,
        &[
            dirac(first, 0.5),
            dirac(first + 4, 0.25),
            dirac(first + 10, 0.125),
            dirac(first + 20, 0.75),
        ],
    )
}

fn daemon_with(dir: &Dir, base: Option<String>) -> Arc<Daemon> {
    let client = Client::new(base, Limits::default());
    daemon_over(dir.0.join("config.toml"), dir.0.join("profiles"))
        .with_speakers(client, Store::new(&dir.store()))
        .shared()
}

fn ask(op: SpeakersOp) -> Request {
    Request::Speakers { op }
}

fn job_of(answer: Response) -> u64 {
    match answer {
        Response::Job { job } => job,
        other => panic!("not a job: {other:?}"),
    }
}

fn reason_of(answer: Response) -> Option<Reason> {
    match answer {
        Response::Error { reason, .. } => reason,
        other => panic!("not refused: {other:?}"),
    }
}

async fn follow(daemon: &Daemon) -> BoxStream<'static, Event> {
    daemon
        .stream(ask(SpeakersOp::Follow))
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the stream")
}

/// The events of job `job` up to and including its final state.
async fn events_of(events: &mut BoxStream<'static, Event>, job: u64) -> Vec<JobState> {
    let mut seen = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(20), events.next())
            .await
            .expect("an event in time")
            .expect("the stream is open");
        if let Event::Speakers(JobEvent { job: id, state }) = event
            && id == job
        {
            let last = state.is_final();
            seen.push(state);
            if last {
                return seen;
            }
        }
    }
}

fn saved_of(states: &[JobState]) -> SpeakersSaved {
    match states.last() {
        Some(JobState::Done { payload }) => serde_json::from_value(payload.clone()).unwrap(),
        other => panic!("not done: {other:?}"),
    }
}

async fn catalogue(daemon: &Daemon) -> SpeakersCatalogue {
    match daemon.handle(ask(SpeakersOp::Catalogue)).await {
        Response::SpeakersCatalogue(found) => *found,
        other => panic!("{other:?}"),
    }
}

fn find<'a>(catalogue: &'a SpeakersCatalogue, id: &str) -> &'a SpeakersSet {
    catalogue
        .sets
        .iter()
        .find(|set| set.id == id)
        .unwrap_or_else(|| panic!("{id} is not listed"))
}

/// Poll until `done` holds, no longer than five seconds.
async fn until(mut done: impl FnMut() -> bool) {
    for _ in 0..500 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the condition never held");
}

#[tokio::test]
async fn a_daemon_given_no_store_answers_internal_and_keeps_nothing() {
    let dir = Dir::new("unset");
    let daemon = daemon_over(dir.0.join("config.toml"), dir.0.join("profiles")).shared();
    for op in [
        SpeakersOp::Catalogue,
        SpeakersOp::Fetch {
            id: "mit-normal".into(),
        },
        SpeakersOp::Forget { id: "x".into() },
    ] {
        assert_eq!(
            reason_of(daemon.handle(ask(op)).await),
            Some(Reason::Internal)
        );
    }
    assert!(dir.left().is_empty());
}

#[tokio::test]
async fn the_catalogue_lists_every_entry_unfetched_and_says_whether_fetching_is_on() {
    let dir = Dir::new("list");
    let server = Server::start(|_| Reply::Status(404));
    let daemon = daemon_with(&dir, Some(server.url.clone()));
    let found = catalogue(&daemon).await;
    assert_eq!(found.sets.len(), svolder_speakers::entries().len());
    for (set, entry) in found.sets.iter().zip(svolder_speakers::entries()) {
        assert_eq!(set.id, entry.id);
        assert!(!set.fetched && set.damaged.is_none() && !set.own);
        assert_eq!(set.bytes, entry.bytes);
        assert_eq!(set.licence, entry.licence);
    }
    assert!(found.fetching && found.note.is_none());
    assert_eq!(found.max_sets, svolder_speakers::MAX_SETS);
    assert_eq!(server.hits(), 0, "a listing reaches nothing");

    let off = daemon_with(&Dir::new("list-off"), Some("http://127.0.0.1:9/".into()));
    let found = catalogue(&off).await;
    assert!(!found.fetching);
    assert_eq!(found.note.as_deref(), Some(svolder_speakers::OFF_WORDS));
    assert!(svolder_speakers::OFF_WORDS.starts_with(svolder_ipc::SPEAKERS_OFF_PREFIX));
}

#[tokio::test]
async fn an_import_is_a_job_that_keeps_the_set_and_a_forget_removes_it() {
    let dir = Dir::new("import");
    let daemon = daemon_with(&dir, Some("http://127.0.0.1:9/".into()));
    let file = dir.0.join("my room.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let mut events = follow(&daemon).await;
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Import {
                path: file.display().to_string(),
                label: None,
                room: Some(SpeakersRoom::With),
            }))
            .await,
    );
    let saved = saved_of(&events_of(&mut events, job).await);
    assert!(saved.set.own && saved.set.fetched);
    assert_eq!(saved.set.label, "my room");
    assert_eq!(saved.set.room, SpeakersRoom::With);
    assert!(saved.set.id.starts_with("own-"));

    let listed = catalogue(&daemon).await;
    assert_eq!(
        listed.sets.last().unwrap().id,
        saved.set.id,
        "own sets follow the catalogue's"
    );
    assert!(find(&listed, &saved.set.id).fetched);

    assert!(matches!(
        daemon
            .handle(ask(SpeakersOp::Forget {
                id: saved.set.id.clone()
            }))
            .await,
        Response::Ack
    ));
    assert!(
        catalogue(&daemon)
            .await
            .sets
            .iter()
            .all(|set| set.id != saved.set.id)
    );
    assert!(dir.left().is_empty());
    assert_eq!(
        reason_of(
            daemon
                .handle(ask(SpeakersOp::Forget { id: saved.set.id }))
                .await
        ),
        Some(Reason::Missing)
    );
    assert_eq!(
        reason_of(
            daemon
                .handle(ask(SpeakersOp::Forget { id: "../x".into() }))
                .await
        ),
        Some(Reason::Invalid)
    );
}

#[tokio::test]
async fn an_import_that_cannot_be_read_fails_in_words_and_keeps_nothing() {
    let dir = Dir::new("bad");
    let daemon = daemon_with(&dir, None);
    let junk = dir.0.join("junk.sofa");
    std::fs::write(&junk, b"not a set at all").unwrap();
    let mut events = follow(&daemon).await;
    for path in [junk.clone(), dir.0.clone(), dir.0.join("missing.wav")] {
        let job = job_of(
            daemon
                .handle(ask(SpeakersOp::Import {
                    path: path.display().to_string(),
                    label: None,
                    room: None,
                }))
                .await,
        );
        match events_of(&mut events, job).await.last() {
            Some(JobState::Failed { message, reason }) => {
                assert_eq!(*reason, Reason::Invalid, "{message}");
                assert!(!message.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(dir.left().is_empty());
}

#[tokio::test]
#[allow(
    clippy::await_holding_lock,
    reason = "the forget under test takes the gate with try_lock, so it answers busy and never waits"
)]
async fn a_forget_is_refused_while_the_set_plays_and_while_a_write_runs() {
    let dir = Dir::new("busy");
    let daemon = daemon_with(&dir, None);
    let file = dir.0.join("a.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let mut events = follow(&daemon).await;
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Import {
                path: file.display().to_string(),
                label: None,
                room: None,
            }))
            .await,
    );
    let id = saved_of(&events_of(&mut events, job).await).set.id;
    let store = daemon.speakers_store().expect("the daemon's store");
    store.set_in_use(&[&id]);
    let refused = daemon
        .handle(ask(SpeakersOp::Forget { id: id.clone() }))
        .await;
    assert_eq!(reason_of(refused), Some(Reason::Busy));
    assert_eq!(dir.left().len(), 2, "the set stayed");
    store.set_in_use(&[]);
    {
        let core = daemon.speakers.core.clone().unwrap();
        let _held = core.gate();
        let refused = daemon
            .handle(ask(SpeakersOp::Forget { id: id.clone() }))
            .await;
        assert_eq!(reason_of(refused), Some(Reason::Busy));
    }
    assert!(matches!(
        daemon.handle(ask(SpeakersOp::Forget { id })).await,
        Response::Ack
    ));
}

#[tokio::test]
async fn a_fetch_of_an_unlisted_set_or_with_fetching_off_is_refused_before_a_job() {
    let dir = Dir::new("refuse");
    let server = Server::start(|_| Reply::Status(404));
    let daemon = daemon_with(&dir, Some(server.url.clone()));
    let refused = daemon
        .handle(ask(SpeakersOp::Fetch {
            id: "nowhere".into(),
        }))
        .await;
    assert_eq!(reason_of(refused), Some(Reason::NotListed));
    assert_eq!(server.hits(), 0, "an id that is not listed reaches nothing");
    assert_eq!(daemon.speakers.jobs().running(), 0);

    let off = daemon_with(&Dir::new("refuse-off"), Some("http://127.0.0.1:9/".into()));
    let refused = off
        .handle(ask(SpeakersOp::Fetch {
            id: "mit-normal".into(),
        }))
        .await;
    assert_eq!(reason_of(refused), Some(Reason::Unreachable));
    assert_eq!(off.speakers.jobs().running(), 0);
}

#[tokio::test]
async fn a_source_that_answers_404_fails_the_job_as_not_listed() {
    let dir = Dir::new("404");
    let server = Server::start(|_| Reply::Status(404));
    let daemon = daemon_with(&dir, Some(server.url.clone()));
    let mut events = follow(&daemon).await;
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Fetch {
                id: "mit-normal".into(),
            }))
            .await,
    );
    match events_of(&mut events, job).await.last() {
        Some(JobState::Failed { reason, .. }) => assert_eq!(*reason, Reason::NotListed),
        other => panic!("{other:?}"),
    }
    assert!(dir.left().is_empty());
}

#[tokio::test]
async fn a_download_reports_its_progress_and_a_changed_source_keeps_nothing() {
    let dir = Dir::new("changed");
    let bytes = svolder_speakers::find("mit-normal").unwrap().bytes as usize;
    // The right number of bytes, the wrong ones: the checksum refuses them.
    let server = Server::start(move |_| Reply::Flood(bytes));
    let daemon = daemon_with(&dir, Some(server.url.clone()));
    let mut events = follow(&daemon).await;
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Fetch {
                id: "mit-normal".into(),
            }))
            .await,
    );
    let states = events_of(&mut events, job).await;
    let progress: Vec<SpeakersProgress> = states
        .iter()
        .filter_map(|state| match state {
            JobState::Progress { payload } => serde_json::from_value(payload.clone()).ok(),
            _ => None,
        })
        .collect();
    assert!(!progress.is_empty(), "{states:?}");
    assert!(
        progress
            .iter()
            .all(|p| p.id == "mit-normal" && p.total == bytes as u64)
    );
    assert!(progress.windows(2).all(|w| w[0].received < w[1].received));
    match states.last() {
        Some(JobState::Failed { message, reason }) => {
            assert_eq!(*reason, Reason::Unreachable);
            assert!(message.contains("source changed"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    assert!(dir.left().is_empty(), "{:?}", dir.left());
}

#[tokio::test]
async fn a_cancelled_fetch_ends_at_once_keeps_nothing_and_the_next_start_is_not_refused() {
    let dir = Dir::new("cancel");
    // The source takes the request and never answers.
    let server = Server::start(|_| Reply::Silent);
    let daemon = daemon_with(&dir, Some(server.url.clone()));
    let mut events = follow(&daemon).await;
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Fetch {
                id: "mit-normal".into(),
            }))
            .await,
    );
    until(|| server.hits() == 1).await;
    // One slot: a second start is refused while the first runs.
    let second = daemon
        .handle(ask(SpeakersOp::Fetch {
            id: "cipic-003".into(),
        }))
        .await;
    assert_eq!(reason_of(second), Some(Reason::Busy));
    assert!(matches!(
        daemon.handle(ask(SpeakersOp::Cancel { job })).await,
        Response::Ack
    ));
    let states = events_of(&mut events, job).await;
    assert_eq!(states.last(), Some(&JobState::Cancelled));
    // The job ended while the thread still waits for the source; ending the source ends it.
    drop(server);
    let core = daemon.speakers.core.clone().unwrap();
    drop(core.gate());
    assert!(dir.left().is_empty(), "{:?}", dir.left());
    assert_eq!(
        reason_of(daemon.handle(ask(SpeakersOp::Cancel { job: 999 })).await),
        Some(Reason::NoSuchJob)
    );
}

#[test]
fn two_imports_of_one_file_at_once_neither_clash_nor_leave_a_temporary_file() {
    let dir = Dir::new("clash");
    let core = Arc::new(Core::new(
        Client::new(None, Limits::default()),
        Store::new(&dir.store()),
    ));
    let file = dir.0.join("same.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let runs: Vec<_> = (0..4)
        .map(|_| {
            let (core, file) = (Arc::clone(&core), file.clone());
            std::thread::spawn(move || core.import(&file, None, None, &Cancel::new()))
        })
        .collect();
    let ids: Vec<String> = runs
        .into_iter()
        .map(|run| run.join().unwrap().expect("imported").set.id)
        .collect();
    assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(dir.left().len(), 2, "{:?}", dir.left());
    assert!(core.store.check(&ids[0]).is_ok());
}

/// A write waits for the gate: with it held, an import of a readable file writes nothing, and it
/// goes through the moment the gate is let go. Without the gate it would finish at once.
#[test]
fn a_write_waits_for_the_gate_and_writes_nothing_before_it() {
    let dir = Dir::new("gate");
    let core = Arc::new(Core::new(
        Client::new(None, Limits::default()),
        Store::new(&dir.store()),
    ));
    let file = dir.0.join("g.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let held = core.gate();
    let waiting = {
        let (core, file) = (Arc::clone(&core), file.clone());
        std::thread::spawn(move || core.import(&file, None, None, &Cancel::new()))
    };
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !waiting.is_finished(),
        "the import did not wait for the gate"
    );
    assert!(dir.left().is_empty());
    drop(held);
    waiting
        .join()
        .unwrap()
        .expect("imported once the gate was free");
    assert_eq!(dir.left().len(), 2);
}

#[test]
fn an_import_cancelled_while_it_waits_for_the_gate_keeps_nothing() {
    let dir = Dir::new("waiting");
    let core = Arc::new(Core::new(
        Client::new(None, Limits::default()),
        Store::new(&dir.store()),
    ));
    let file = dir.0.join("w.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let cancel = Cancel::new();
    let held = core.gate();
    let waiting = {
        let (core, file, cancel) = (Arc::clone(&core), file.clone(), cancel.clone());
        std::thread::spawn(move || core.import(&file, None, None, &cancel))
    };
    cancel.cancel();
    drop(held);
    let failure = waiting.join().unwrap().expect_err("cancelled");
    assert!(failure.cancelled);
    assert!(dir.left().is_empty());
}

#[tokio::test]
async fn the_follow_op_is_a_stream_and_refused_as_a_plain_request() {
    let dir = Dir::new("follow");
    let daemon = daemon_with(&dir, None);
    let answer = daemon.handle(ask(SpeakersOp::Follow)).await;
    assert!(matches!(answer, Response::Error { .. }));
    assert!(
        daemon
            .stream(ask(SpeakersOp::Cancel { job: 1 }))
            .await
            .is_err()
    );
    assert!(daemon.stream(ask(SpeakersOp::Catalogue)).await.is_err());
}

/// A FIFO with no writer is refused before it is opened: the job fails in words at once and the
/// one speakers slot is free again.
#[tokio::test]
async fn a_pipe_is_refused_as_not_a_regular_file_and_does_not_hold_the_slot() {
    let dir = Dir::new("fifo");
    let daemon = daemon_with(&dir, None);
    let fifo = dir.0.join("pipe");
    let name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let mut events = follow(&daemon).await;
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Import {
                path: fifo.display().to_string(),
                label: None,
                room: None,
            }))
            .await,
    );
    match events_of(&mut events, job).await.last() {
        Some(JobState::Failed { message, reason }) => {
            assert_eq!(*reason, Reason::Invalid);
            assert!(message.contains("not a regular file"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(daemon.speakers.jobs().running(), 0);
}

fn linked(dir: &Dir, chosen: Option<String>) -> (Arc<Daemon>, Arc<std::sync::atomic::AtomicU32>) {
    let daemon = daemon_with(dir, None);
    let changes = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counted = Arc::clone(&changes);
    daemon.link_speakers(SpeakersLinks {
        chosen: Box::new(move || chosen.clone()),
        changed: Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }),
    });
    (daemon, changes)
}

async fn import_file(
    daemon: &Daemon,
    events: &mut BoxStream<'static, Event>,
    path: &std::path::Path,
) -> String {
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Import {
                path: path.display().to_string(),
                label: None,
                room: None,
            }))
            .await,
    );
    saved_of(&events_of(events, job).await).set.id
}

#[tokio::test]
async fn the_controller_is_told_after_every_op_that_changes_the_sets() {
    let dir = Dir::new("changed");
    let (daemon, changes) = linked(&dir, None);
    let mut events = follow(&daemon).await;
    let file = dir.0.join("a.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let id = import_file(&daemon, &mut events, &file).await;
    assert_eq!(changes.load(Ordering::SeqCst), 1);
    // A refused import and a refused forget change nothing.
    let junk = dir.0.join("junk.sofa");
    std::fs::write(&junk, b"nope").unwrap();
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Import {
                path: junk.display().to_string(),
                label: None,
                room: None,
            }))
            .await,
    );
    events_of(&mut events, job).await;
    let _ = daemon
        .handle(ask(SpeakersOp::Forget {
            id: "absent".into(),
        }))
        .await;
    assert_eq!(changes.load(Ordering::SeqCst), 1);
    daemon.handle(ask(SpeakersOp::Forget { id })).await;
    assert_eq!(changes.load(Ordering::SeqCst), 2);
}

/// The 33rd set makes room, and the set the person chose is not the one that goes, whether or
/// not it plays.
#[tokio::test]
async fn the_chosen_set_is_never_the_one_the_33rd_import_evicts() {
    let dir = Dir::new("chosen");
    let plain = daemon_with(&dir, None);
    let mut events = follow(&plain).await;
    let mut ids = Vec::new();
    for n in 0..svolder_speakers::MAX_SETS {
        let file = dir.0.join(format!("s{n}.wav"));
        std::fs::write(&file, set_wav(n)).unwrap();
        ids.push(import_file(&plain, &mut events, &file).await);
    }
    // The one a plain 33rd save would evict first: the oldest by (used, added, id).
    let store = plain.speakers_store().unwrap();
    let mut metas: Vec<_> = ids.iter().map(|id| store.check(id).unwrap()).collect();
    metas.sort_by(|a, b| (a.used, a.added, &a.id).cmp(&(b.used, b.added, &b.id)));
    let chosen = metas[0].id.clone();
    let (daemon, _) = linked(&dir, Some(chosen.clone()));
    let mut events = follow(&daemon).await;
    let file = dir.0.join("33rd.wav");
    std::fs::write(&file, set_wav(60)).unwrap();
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Import {
                path: file.display().to_string(),
                label: None,
                room: None,
            }))
            .await,
    );
    let saved = saved_of(&events_of(&mut events, job).await);
    assert_eq!(saved.evicted.len(), 1, "{:?}", saved.evicted);
    assert_ne!(saved.evicted[0], chosen);
    assert!(daemon.speakers_store().unwrap().check(&chosen).is_ok());
}

#[tokio::test]
async fn the_start_sweep_clears_the_import_leftovers_and_keeps_the_sets() {
    let dir = Dir::new("sweep");
    let daemon = daemon_with(&dir, None);
    let mut events = follow(&daemon).await;
    let file = dir.0.join("a.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let id = import_file(&daemon, &mut events, &file).await;
    let leftovers = dir.store().join(".import");
    std::fs::create_dir_all(&leftovers).unwrap();
    std::fs::write(leftovers.join("upload-1"), b"half a file").unwrap();
    std::fs::write(leftovers.join("upload-2"), b"").unwrap();
    daemon.sweep_speakers_imports();
    assert_eq!(std::fs::read_dir(&leftovers).unwrap().count(), 0);
    assert_eq!(
        dir.left(),
        [
            ".import".to_string(),
            format!("{id}.toml"),
            format!("{id}.wav")
        ]
    );
    assert!(daemon.speakers_store().unwrap().check(&id).is_ok());
}

/// The daemon's view of the computer EQ, once `ready` holds for it.
async fn eq_view_once(
    daemon: &Daemon,
    ready: impl Fn(&svolder_host::eq::SpeakersView) -> bool,
) -> svolder_host::eq::SpeakersView {
    let mut changes = daemon.host().changes();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let view = daemon.host().view_now().await;
            if let Some(speakers) = view.eq.and_then(|eq| eq.speakers)
                && ready(&speakers)
            {
                return speakers;
            }
            changes.changed().await.expect("the host's view stays open");
        }
    })
    .await
    .expect("the view in time")
}

/// A refusal because the set is in use, as the speakers service words it: not a held gate.
fn refused_in_use(answer: Response) {
    match answer {
        Response::Error {
            message, reason, ..
        } => {
            assert_eq!(reason, Some(Reason::Busy));
            assert!(message.contains("plays now"), "{message}");
        }
        other => panic!("not refused: {other:?}"),
    }
}

fn speakers_eq(on: Option<bool>, set: Option<String>) -> Request {
    Request::HostEq {
        op: svolder_ipc::HostEqOp::Speakers {
            on,
            set,
            done: true,
        },
    }
}

/// One store in the daemon: the computer's controller plays a set from the store the speakers
/// service keeps, so a forget of the set its chain holds is refused through the daemon, and the
/// service's changes and the chosen set go through the links to the computer that runs.
#[tokio::test]
async fn the_computer_serves_the_speakers_from_the_daemon_s_one_store() {
    let dir = Dir::new("one-store");
    let daemon = daemon_with(&dir, None);
    let params = svolder_host::fake::FakeParams::new();
    params.with_speakers_ports(false);
    daemon
        .host()
        .run_with_eq(svolder_host::fake::FakeChain::new(), params);
    daemon.link_speakers_to_host();
    assert!(
        eq_view_once(&daemon, |view| view.sets.is_empty())
            .await
            .set
            .is_none()
    );

    // A set the service imports reaches the controller's list through the link.
    let mut events = follow(&daemon).await;
    let file = dir.0.join("a.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let id = import_file(&daemon, &mut events, &file).await;
    eq_view_once(&daemon, |view| view.sets.iter().any(|set| set.id == id)).await;

    // Chosen and playing through the daemon: the service sees the choice, and the forget is
    // refused because the chain holds the set.
    let answer = daemon
        .handle(speakers_eq(Some(true), Some(id.clone())))
        .await;
    assert!(matches!(answer, Response::Ack), "{answer:?}");
    let view = eq_view_once(&daemon, |view| view.on).await;
    assert_eq!(view.set.as_deref(), Some(id.as_str()));
    assert_eq!(
        (daemon.host().chosen_speakers_fn())(),
        Some(id.clone()),
        "the service's link reads the computer's choice"
    );
    refused_in_use(
        daemon
            .handle(ask(SpeakersOp::Forget { id: id.clone() }))
            .await,
    );
    assert!(daemon.speakers_store().unwrap().check(&id).is_ok());
}

/// A refused change of the speakers whose set the store cannot play carries the reason
/// `missing` (a front end reads the sets again on it); a refusal for anything else carries none.
#[tokio::test]
async fn a_refused_choice_of_a_set_the_store_lacks_says_missing() {
    let dir = Dir::new("missing");
    let daemon = daemon_with(&dir, None);
    let params = svolder_host::fake::FakeParams::new();
    params.with_speakers_ports(false);
    daemon
        .host()
        .run_with_eq(svolder_host::fake::FakeChain::new(), params);
    daemon.link_speakers_to_host();
    eq_view_once(&daemon, |view| view.sets.is_empty()).await;
    let mut events = follow(&daemon).await;
    let file = dir.0.join("a.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let id = import_file(&daemon, &mut events, &file).await;
    eq_view_once(&daemon, |view| view.sets.iter().any(|set| set.id == id)).await;

    let gone = daemon
        .handle(speakers_eq(Some(true), Some("never-fetched".to_string())))
        .await;
    assert_eq!(reason_of(gone), Some(Reason::Missing));
    let nothing = daemon.handle(speakers_eq(None, None)).await;
    assert!(matches!(nothing, Response::Error { .. }), "{nothing:?}");
    assert_eq!(reason_of(nothing), None, "a set that plays is not missing");
}

/// The words a forget of the set that plays is refused with name the set by its label, the same
/// words the TUI says before it asks.
#[tokio::test]
async fn a_forget_of_the_set_that_plays_names_it_by_its_label() {
    let dir = Dir::new("label");
    let daemon = daemon_with(&dir, None);
    let file = dir.0.join("a.wav");
    std::fs::write(&file, set_wav(10)).unwrap();
    let mut events = follow(&daemon).await;
    let job = job_of(
        daemon
            .handle(ask(SpeakersOp::Import {
                path: file.display().to_string(),
                label: Some("Desk room".to_string()),
                room: None,
            }))
            .await,
    );
    let id = saved_of(&events_of(&mut events, job).await).set.id;
    daemon.speakers_store().unwrap().set_in_use(&[&id]);
    match daemon.handle(ask(SpeakersOp::Forget { id })).await {
        Response::Error { message, .. } => assert_eq!(
            message,
            svolder_host::eq::speakers::plays_now_words("Desk room")
        ),
        other => panic!("not refused: {other:?}"),
    }
}

/// The sets-changed link may be called from a thread with no runtime of its own (the service's
/// blocking work): the controller reads the sets again on the daemon's runtime.
#[tokio::test]
async fn the_sets_changed_link_works_from_a_plain_thread() {
    let dir = Dir::new("plain-thread");
    let daemon = daemon_with(&dir, None);
    daemon.host().run_with_eq(
        svolder_host::fake::FakeChain::new(),
        svolder_host::fake::FakeParams::new(),
    );
    eq_view_once(&daemon, |view| view.sets.is_empty()).await;
    let store = daemon.speakers_store().unwrap();
    let saved = svolder_speakers::import(
        &store,
        &set_wav(10),
        "Own file",
        svolder_speakers::Room::No,
        1,
        None,
    )
    .unwrap();
    let changed = daemon.host().sets_changed_fn();
    std::thread::spawn(changed).join().expect("no panic");
    eq_view_once(&daemon, |view| {
        view.sets.iter().any(|set| set.id == saved.meta.id)
    })
    .await;
}

/// A state as a running chain's apply leaves it: on, holding `loaded`'s convolvers.
fn state_holding(
    loaded: Option<&str>,
    chosen: Option<&str>,
    on: bool,
) -> svolder_host::eq::HostEqState {
    svolder_host::eq::HostEqState {
        on,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: svolder_host::eq::Route::Smart,
        manager: svolder_host::eq::Manager::Process,
        previous_default: None,
        preamp_db: 0.0,
        filters: Vec::new(),
        slots: Vec::new(),
        crossfeed: None,
        crossfeed_on: false,
        flat: false,
        speakers: chosen.map(str::to_string),
        speakers_on: false,
        speakers_loaded: loaded.map(str::to_string),
    }
}

/// With no controller running (the host off or fake), the set a chain left running holds is
/// pinned from the state file, so neither a forget nor an eviction takes its WAV.
#[tokio::test]
async fn the_set_a_running_chain_holds_is_pinned_from_the_state_with_no_controller() {
    let dir = Dir::new("seed");
    let daemon = daemon_with(&dir, None);
    let mut events = follow(&daemon).await;
    let mut ids = Vec::new();
    for n in 0..3 {
        let file = dir.0.join(format!("s{n}.wav"));
        std::fs::write(&file, set_wav(10 + n)).unwrap();
        ids.push(import_file(&daemon, &mut events, &file).await);
    }
    let store = daemon.speakers_store().unwrap();
    pin_from_state(
        &store,
        Some(&state_holding(Some(&ids[0]), Some(&ids[1]), true)),
    );
    refused_in_use(
        daemon
            .handle(ask(SpeakersOp::Forget { id: ids[0].clone() }))
            .await,
    );
    assert!(store.check(&ids[0]).is_ok());

    // A state from before `speakers_loaded`: the speakers on means the config holds the chosen set.
    let mut older = state_holding(None, Some(&ids[1]), true);
    older.speakers_on = true;
    pin_from_state(&store, Some(&older));
    assert!(matches!(
        store.forget(&ids[1]),
        Err(svolder_speakers::StoreError::InUse(_))
    ));

    // A chain that runs for the speakers alone (the EQ off, playing flat) holds them too.
    let mut flat = state_holding(Some(&ids[2]), Some(&ids[2]), false);
    flat.flat = true;
    pin_from_state(&store, Some(&flat));
    assert!(matches!(
        store.forget(&ids[2]),
        Err(svolder_speakers::StoreError::InUse(_))
    ));

    // A removed chain (its state off, no set loaded) holds nothing, and neither does a missing
    // state.
    pin_from_state(&store, Some(&state_holding(None, Some(&ids[2]), false)));
    store.forget(&ids[2]).unwrap();
    pin_from_state(&store, None);
    store.forget(&ids[1]).unwrap();
}
