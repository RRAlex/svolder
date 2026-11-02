// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a device on `auto` that refuses its code is given: one drop of the link, a fresh reading
//! of the code, and no second drop for the same refusal.

use svolder_ble::{Advertised, FakeTransport};

use super::*;

fn code(text: &str) -> Passphrase {
    Passphrase::from_text(text).expect("four digits")
}

/// A fake gungnir 2 whose code is `1234`, saying `how` about it on the air.
fn fake(how: Advertised) -> Arc<FakeTransport> {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let transport = FakeTransport::from_profile(
        &profile,
        "0102030405060708".parse().expect("a valid unit id"),
        3,
        code("1234"),
    );
    transport.advertise(how);
    Arc::new(transport)
}

/// An actor on `transport` whose entry says `entry`, with the log's lines kept.
fn actor_on(transport: &Arc<FakeTransport>, entry: &str) -> DeviceHandle {
    crate::log::hold_lines();
    let shared: Arc<dyn Transport> = Arc::clone(transport) as Arc<dyn Transport>;
    spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().expect("the built-in profiles load")),
        fake_config(entry),
        scan_for(&shared),
    )
    .expect("the actor starts")
}

/// Wait until the actor reports a link, and then a little longer, so a second attempt that was
/// going to follow the first would have shown.
async fn settled(handle: &DeviceHandle) {
    tokio::time::timeout(Duration::from_secs(60), async {
        while !handle.describe().connected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the actor never connected to the fake device");
    tokio::time::sleep(Duration::from_secs(120)).await;
}

/// The lines the log kept that say the device was refused.
fn refusal_lines() -> Vec<String> {
    crate::log::held_lines()
        .into_iter()
        .filter(|(_, line)| line.contains("refused the code"))
        .map(|(_, line)| line)
        .collect()
}

/// A `mute` set, which the device echoes when it accepts the code.
fn mute() -> Command {
    Command::Set {
        key: "mute".into(),
        value: "on".into(),
        step: 1,
        no_wait: false,
    }
}

/// How many of the changes of the link `handle` records say it came up.
fn ups(handle: &DeviceHandle) -> usize {
    marks(handle, 3600).iter().filter(|(_, up)| *up).count()
}

/// The device's own handle, to change its code the way the official app does.
fn change_code(transport: &FakeTransport, from: &str, to: &str) {
    let change = Message::command(
        svolder_proto::function::PASSPHRASE,
        Value::U32(code(to).encoded()),
    );
    let frame = svolder_proto::encode_command(&change, code(from)).expect("a frame");
    transport.device().lock().unwrap().handle(&frame);
}

/// An advertisement older than the device's code is corrected by one drop: the connect's refresh
/// is refused, the link is released, the code is read from what the device advertises next, and
/// the connect that follows is accepted and reads the device. The front ends see one link coming
/// up and never a link that was refused, and the log says it once.
#[tokio::test(start_paused = true)]
async fn a_stale_code_is_corrected_by_one_drop_and_the_device_is_read_afterwards() {
    let transport = fake(Advertised::UntilReleased(code("9999")));
    let handle = actor_on(&transport, "auto");
    settled(&handle).await;

    assert_eq!(transport.connects(), 2, "one drop, one more connect");
    assert_eq!(transport.releases(), 1);
    assert_eq!(ups(&handle), 1, "the refused link was never shown as up");
    assert_eq!(marks(&handle, 3600).len(), 1, "and never as down");
    assert!(matches!(
        ask(&handle, Command::State).await,
        Response::Values { values, .. } if !values.is_empty()
    ));
    assert!(matches!(ask(&handle, mute()).await, Response::Value(_)));
    assert_eq!(handle.describe().passphrase_note, None);
    assert_eq!(refusal_lines().len(), 1, "{:?}", crate::log::held_lines());
}

/// A fresh advertisement that carries the code that was refused changes nothing: exactly one
/// drop, and then a refused link that is kept, written nothing, and reported as it always was.
#[tokio::test(start_paused = true)]
async fn the_same_code_again_gives_one_drop_and_then_a_kept_refused_link() {
    let transport = fake(Advertised::Always(code("9999")));
    let handle = actor_on(&transport, "auto");
    settled(&handle).await;

    assert_eq!(transport.connects(), 2);
    assert_eq!(transport.releases(), 1, "one drop and no second");
    assert!(handle.describe().connected, "the refused link is kept");
    let seen = transport.device().lock().unwrap().commands_seen();
    match ask(&handle, mute()).await {
        Response::Error { code, message } => {
            assert_eq!(code, 3);
            assert!(message.contains("passphrase"), "{message}");
        }
        other => panic!("got {other:?}"),
    }
    assert_eq!(
        transport.device().lock().unwrap().commands_seen(),
        seen,
        "nothing is written while the code is refused"
    );
    assert_eq!(refusal_lines().len(), 1, "{:?}", crate::log::held_lines());
}

/// A device that is not heard again within the window is connected as it would have been, from
/// the advertisement the adapter holds: refused again, that link is kept.
#[tokio::test(start_paused = true)]
async fn no_advertisement_in_the_window_gives_one_drop_and_then_a_kept_refused_link() {
    let transport = fake(Advertised::Always(code("9999")));
    transport.go_quiet_after_release();
    let handle = actor_on(&transport, "auto");
    settled(&handle).await;

    assert_eq!(transport.connects(), 2);
    assert_eq!(transport.releases(), 1);
    assert!(handle.describe().connected);
    assert!(matches!(
        ask(&handle, mute()).await,
        Response::Error { code: 3, .. }
    ));
    assert_eq!(refusal_lines().len(), 1);
}

/// A code the user typed, and `none`, are the user's own entry: a refusal of either keeps the
/// link, as it always has, and nothing is released or listened for.
#[tokio::test(start_paused = true)]
async fn digits_and_none_never_drop() {
    for entry in ["9999", "none"] {
        let transport = fake(Advertised::UntilReleased(code("5555")));
        let handle = actor_on(&transport, entry);
        settled(&handle).await;

        assert_eq!(transport.connects(), 1, "{entry}");
        assert_eq!(transport.releases(), 0, "{entry}");
        assert!(handle.describe().connected, "{entry}");
        assert!(refusal_lines().is_empty(), "{entry}");
    }
}

/// A refusal on a write, long after the connect, is answered the same way: the device's code
/// changed from the official app while the link was up, the adapter's advertisement is the old
/// one, the write is refused, the link is released, and the device is read again under the new
/// code. Front ends see the link go down and come back, and the log says it once.
#[tokio::test(start_paused = true)]
async fn a_refusal_on_a_write_drops_the_link_once_and_reads_the_code_afresh() {
    let transport = fake(Advertised::Truthfully);
    let handle = actor_on(&transport, "auto");
    settled(&handle).await;
    assert_eq!(transport.connects(), 1);
    assert!(matches!(ask(&handle, mute()).await, Response::Value(_)));

    change_code(&transport, "1234", "5555");
    transport.advertise(Advertised::UntilReleased(code("1234")));
    assert!(matches!(
        ask(&handle, mute()).await,
        Response::Error { code: 3, .. }
    ));
    settled(&handle).await;

    assert_eq!(transport.releases(), 1);
    assert_eq!(transport.connects(), 2);
    assert_eq!(
        marks(&handle, 3600)
            .iter()
            .map(|(_, up)| *up)
            .collect::<Vec<_>>(),
        vec![true, false, true]
    );
    assert!(matches!(ask(&handle, mute()).await, Response::Value(_)));
    assert_eq!(refusal_lines().len(), 1, "{:?}", crate::log::held_lines());
}

/// A refusal the device volunteers on a link that was accepted, as a wake's refresh can earn,
/// is a refusal on a live link too.
#[tokio::test(start_paused = true)]
async fn a_refusal_the_device_volunteers_drops_the_link_once() {
    let transport = fake(Advertised::Truthfully);
    let handle = actor_on(&transport, "auto");
    settled(&handle).await;

    transport.push(&[Message::status(
        svolder_proto::function::PASSPHRASE_REJECTED,
        Value::Bool(true),
    )]);
    settled(&handle).await;

    assert_eq!(transport.releases(), 1);
    assert_eq!(transport.connects(), 2);
    assert!(handle.describe().connected);
    assert_eq!(refusal_lines().len(), 1);
}

/// A link the episode left standing that drops on its own, and comes back refused under the same
/// code, is the same refusal: it is reconnected, and not dropped a second time.
#[tokio::test(start_paused = true)]
async fn a_link_that_drops_on_its_own_is_the_same_refusal() {
    let transport = fake(Advertised::Always(code("9999")));
    let handle = actor_on(&transport, "auto");
    settled(&handle).await;
    assert_eq!((transport.connects(), transport.releases()), (2, 1));

    transport.sever();
    settled(&handle).await;

    assert_eq!((transport.connects(), transport.releases()), (3, 1));
    assert!(handle.describe().connected);
    assert_eq!(refusal_lines().len(), 1);
}

/// The device advertising another code than the one that was refused ends the episode: the
/// connect that reads it is refused in its turn, and given its own drop.
#[tokio::test(start_paused = true)]
async fn another_advertised_code_ends_the_episode() {
    let transport = fake(Advertised::Always(code("9999")));
    let handle = actor_on(&transport, "auto");
    settled(&handle).await;
    assert_eq!((transport.connects(), transport.releases()), (2, 1));

    transport.advertise(Advertised::Always(code("8888")));
    transport.sever();
    settled(&handle).await;

    assert_eq!((transport.connects(), transport.releases()), (4, 2));
    assert_eq!(refusal_lines().len(), 2, "one line for each episode");
}

/// A device that is refused is refused on the list the daemon answers `devices` with, and
/// is no longer once the actor lets the link go.
#[tokio::test(start_paused = true)]
async fn the_device_list_says_a_refused_link_is_refused() {
    let transport = fake(Advertised::Always(code("9999")));
    let handle = actor_on(&transport, "auto");
    settled(&handle).await;
    assert!(handle.describe().refused);

    // An actor that has the refusal recorded forgets it as the link is lost.
    let actor = connected_actor(fake(Advertised::Truthfully) as Arc<dyn Transport>).await;
    actor.info.lock().unwrap().refused = true;
    actor.set_connected(false);
    assert!(!actor.info.lock().unwrap().refused);

    let healthy = fake(Advertised::Truthfully);
    let ok = actor_on(&healthy, "auto");
    settled(&ok).await;
    assert!(!ok.describe().refused);
}

/// An actor over a session that has proven nothing, so only what a test does can end the
/// episode, whose drop is used.
async fn spent_actor() -> Actor {
    let mut actor = unproven_actor().await;
    actor.episode.dropped(code("9999"));
    actor.episode.listened();
    actor
}

/// An actor whose device has shown its code works, with the episode's drop used.
async fn proven_actor() -> Actor {
    let transport = fake(Advertised::Truthfully);
    let mut actor = connected_actor(transport as Arc<dyn Transport>).await;
    actor.episode.dropped(code("9999"));
    actor.episode.listened();
    actor
}

/// A changed passphrase entry ends the episode; a change of anything else about the entry does
/// not.
#[tokio::test]
async fn a_changed_entry_ends_the_episode() {
    let mut actor = spent_actor().await;
    let spent = actor.episode;
    assert_ne!(spent, Episode::default());

    let mut renamed = fake_config("1234");
    renamed.name = "other".into();
    actor.configure(renamed);
    assert_eq!(actor.episode, spent, "a rename is not a new code");

    let mut recoded = actor.config.clone();
    recoded.passphrase = "auto".into();
    actor.configure(recoded);
    assert_eq!(actor.episode, Episode::default());
}

/// A device that shows a code works ends the episode.
#[tokio::test]
async fn a_proven_code_ends_the_episode() {
    let mut actor = proven_actor().await;
    assert!(actor.session.as_ref().unwrap().code_accepted());
    actor.judge_passphrase();
    assert_eq!(actor.episode, Episode::default());
}

/// An entry changed while an attempt ran ended the episode the attempt carried, whatever the
/// attempt did to it.
#[tokio::test]
async fn an_entry_changed_during_an_attempt_ends_the_episode_it_carried() {
    let mut actor = spent_actor().await;
    let spent = actor.episode;
    let session = actor.session.take();
    let unchanged = Attempt {
        advert: None,
        heard_at_ms: None,
        session,
        outcome: Ok(()),
        episode: spent,
        entry: actor.config.passphrase.clone(),
    };
    actor.absorb(unchanged);
    assert_eq!(actor.episode, spent);

    let session = actor.session.take();
    let changed = Attempt {
        advert: None,
        heard_at_ms: None,
        session,
        outcome: Ok(()),
        episode: spent,
        entry: "5555".into(),
    };
    actor.absorb(changed);
    assert_eq!(actor.episode, Episode::default());
}

/// A transport that finds the device and cannot connect to it.
struct Unreachable(Arc<FakeTransport>);

#[async_trait::async_trait]
impl Transport for Unreachable {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        self.0.scan(window).await
    }

    async fn connect(&self, _unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        Err(BleError::Connection("out of reach".into()))
    }
}

/// An attempt carrying a spent episode that listens and then fails to connect leaves the episode
/// spent and listened: the device being away says nothing about its code, so the link that comes
/// back refused is the same refusal. One that finds nothing at all has not listened yet.
#[tokio::test(start_paused = true)]
async fn a_failed_connect_after_the_drop_does_not_end_the_episode() {
    let mut carried = Episode::default();
    carried.dropped(code("9999"));
    let mut listened = carried;
    listened.listened();

    let run = |transport: Arc<dyn Transport>, episode: Episode| async move {
        let scanner = scan_for(&transport);
        connect_attempt(
            transport,
            scanner,
            "0102030405060708".parse().unwrap(),
            Arc::new(Registry::builtin().unwrap()),
            fake_config("auto"),
            Carried {
                session: None,
                episode,
            },
            Arc::new(AtomicBool::new(false)),
        )
        .await
    };

    let fake = fake(Advertised::Truthfully);
    let found = run(Arc::new(Unreachable(Arc::clone(&fake))), carried).await;
    assert!(found.outcome.is_err());
    assert_eq!(found.episode, listened);

    // Nothing on the air and nothing held: the attempt never got to listen.
    let quiet = svolder_ble::FakeTransport::from_profile(
        &Registry::generic(0),
        "AABBCCDDEEFF0011".parse().unwrap(),
        3,
        code("1234"),
    );
    let lost = run(Arc::new(quiet) as Arc<dyn Transport>, carried).await;
    assert!(lost.outcome.is_err());
    assert_eq!(lost.episode, carried);
}

/// A transport over the fake whose first listen after a release hears the advertisement the
/// adapter held from before it, as a device-changed event raised by the release can read out.
struct Echoing {
    inner: Arc<FakeTransport>,
    held: Passphrase,
    echoed: AtomicBool,
}

#[async_trait::async_trait]
impl Transport for Echoing {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn sight(
        &self,
        unit: UnitId,
        window: Duration,
    ) -> Result<Option<Advertisement>, BleError> {
        if self.inner.releases() > 0 && !self.echoed.swap(true, Ordering::SeqCst) {
            let mut advert = self.inner.advertisement();
            advert.set_advertised_passphrase(self.held);
            return Ok(Some(advert));
        }
        self.inner.sight(unit, window).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        self.inner.connect(unit).await
    }
}

/// The listen that follows a drop is not taken in by the advertisement the release stirred up
/// out of the adapter's memory: it reads on to the device's own, and the one connect after the
/// drop is accepted.
#[tokio::test(start_paused = true)]
async fn the_listen_after_a_drop_skips_the_advertisement_the_release_stirred_up() {
    let fake = fake(Advertised::UntilReleased(code("9999")));
    crate::log::hold_lines();
    let shared = Arc::new(Echoing {
        inner: Arc::clone(&fake),
        held: code("9999"),
        echoed: AtomicBool::new(false),
    }) as Arc<dyn Transport>;
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().expect("the built-in profiles load")),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");
    settled(&handle).await;

    assert_eq!((fake.connects(), fake.releases()), (2, 1));
    assert!(matches!(ask(&handle, mute()).await, Response::Value(_)));
}

/// A link made after the drop that is refused too makes the code it was refused under what the
/// episode is about, so the next connect that reads that same code is the same refusal.
#[tokio::test(start_paused = true)]
async fn a_refusal_after_the_drop_is_what_the_episode_is_about() {
    let transport = fake(Advertised::ThenAlways {
        stale: code("9999"),
        next: code("8888"),
    });
    let handle = actor_on(&transport, "auto");
    settled(&handle).await;
    assert_eq!((transport.connects(), transport.releases()), (2, 1));

    transport.sever();
    settled(&handle).await;

    assert_eq!(
        (transport.connects(), transport.releases()),
        (3, 1),
        "8888 is what was refused last, and the device still advertises it"
    );
}
