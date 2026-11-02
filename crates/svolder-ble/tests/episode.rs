// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a refused code earns a device on `auto`: the fresh listen, the session's own account of a
//! refusal, and the episode that keeps the drop to one.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use svolder_ble::{
    Advertised, Advertisement, BleError, Episode, FakeTransport, Link, Session, Transport,
    listen_afresh,
};
use svolder_fake::FakeDevice;
use svolder_profiles::Registry;
use svolder_proto::{FunctionId, Message, Passphrase, UnitId, Value, encode_command};

const WINDOW: Duration = Duration::from_secs(5);

fn code(text: &str) -> Passphrase {
    Passphrase::from_text(text).expect("four digits")
}

fn unit() -> UnitId {
    "0102030405060708".parse().expect("a unit id")
}

/// A fake gungnir 2 whose passphrase is `1234`, and its device.
fn fake() -> (Arc<FakeTransport>, Arc<Mutex<FakeDevice>>) {
    let profile = Registry::builtin()
        .expect("profiles")
        .get("gungnir2-v2")
        .expect("a profile")
        .clone();
    let transport = Arc::new(FakeTransport::from_profile(
        &profile,
        unit(),
        3,
        code("1234"),
    ));
    let device = transport.device();
    (transport, device)
}

/// A session over `transport`, keyed with what the transport's advertisement carries now, as a
/// device on `auto` is.
fn auto_session(transport: &Arc<FakeTransport>) -> Session {
    let profile = Registry::builtin()
        .expect("profiles")
        .get("gungnir2-v2")
        .expect("a profile")
        .clone();
    let advert = transport.advertisement();
    let keyed = advert.manufacturer.passphrase();
    Session::new(
        Arc::clone(transport) as Arc<dyn Transport>,
        advert,
        profile,
        keyed,
    )
}

/// A transport that answers each listen with the next thing a test queued, and counts them.
struct Scripted {
    heard: Mutex<VecDeque<Option<Advertisement>>>,
    listens: AtomicUsize,
}

impl Scripted {
    fn new(heard: Vec<Option<Advertisement>>) -> Self {
        Self {
            heard: Mutex::new(heard.into()),
            listens: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl Transport for Scripted {
    async fn scan(&self, _window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        unreachable!("the listen goes through sight")
    }

    async fn sight(
        &self,
        _unit: UnitId,
        _window: Duration,
    ) -> Result<Option<Advertisement>, BleError> {
        self.listens.fetch_add(1, Ordering::SeqCst);
        Ok(self.heard.lock().unwrap().pop_front().flatten())
    }

    async fn connect(&self, _unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        Err(BleError::NotFound)
    }
}

/// The fake's advertisement saying `text` as the device's code.
fn saying(text: &str) -> Advertisement {
    let (transport, _) = fake();
    let mut advert = transport.advertisement();
    advert.set_advertised_passphrase(code(text));
    advert
}

/// The first advertisement answers when it carries another code than the refused one, and the
/// listen asks no further.
#[tokio::test(start_paused = true)]
async fn a_different_code_ends_the_listen_at_once() {
    let transport = Scripted::new(vec![Some(saying("1234")), Some(saying("5555"))]);
    let heard = listen_afresh(&transport, unit(), code("9999"), WINDOW)
        .await
        .unwrap()
        .expect("an advertisement");
    assert_eq!(heard.manufacturer.passphrase(), code("1234"));
    assert_eq!(transport.listens.load(Ordering::SeqCst), 1);
}

/// An advertisement that repeats the refused code may be the one held from before the release,
/// so the listen goes on once for the rest of the window, and what that hears is the answer.
#[tokio::test(start_paused = true)]
async fn the_refused_code_again_is_listened_past() {
    let transport = Scripted::new(vec![Some(saying("9999")), Some(saying("1234"))]);
    let heard = listen_afresh(&transport, unit(), code("9999"), WINDOW)
        .await
        .unwrap()
        .expect("an advertisement");
    assert_eq!(heard.manufacturer.passphrase(), code("1234"));
    assert_eq!(transport.listens.load(Ordering::SeqCst), 2);
}

/// A window that hears the refused code twice, or once and then nothing, answers with what it
/// heard, and listens no more than twice.
#[tokio::test(start_paused = true)]
async fn the_refused_code_twice_or_then_silence_answers_with_it() {
    for second in [Some(saying("9999")), None] {
        let transport = Scripted::new(vec![Some(saying("9999")), second]);
        let heard = listen_afresh(&transport, unit(), code("9999"), WINDOW)
            .await
            .unwrap()
            .expect("an advertisement");
        assert_eq!(heard.manufacturer.passphrase(), code("9999"));
        assert_eq!(transport.listens.load(Ordering::SeqCst), 2);
    }
}

/// Nothing heard in the window is nothing, after one listen.
#[tokio::test(start_paused = true)]
async fn silence_answers_nothing_after_one_listen() {
    let transport = Scripted::new(vec![None, Some(saying("1234"))]);
    let heard = listen_afresh(&transport, unit(), code("9999"), WINDOW)
        .await
        .unwrap();
    assert!(heard.is_none());
    assert_eq!(transport.listens.load(Ordering::SeqCst), 1);
}

/// A session reports a refusal only for the link it holds: not before the device has answered,
/// not once a changed code has lifted it, and not for a refusal an earlier link earned when
/// the device now accepts the very code it refused then.
#[tokio::test(start_paused = true)]
async fn refused_is_about_the_link_held_now() {
    let (transport, device) = fake();
    transport.advertise(Advertised::Always(code("9999")));
    let mut session = auto_session(&transport);
    assert!(!session.refused(), "no link yet");

    session.connect().await.unwrap();
    assert!(session.refused(), "the connect's refresh was refused");

    // A changed code lifts the refusal.
    session.set_passphrase(code("1234"));
    assert!(!session.refused(), "a changed code lifts it");

    // Refused again under the old code, then the device comes to hold that code.
    session.set_passphrase(code("9999"));
    session.connect().await.unwrap();
    assert!(session.refused());
    let change = Message::command(FunctionId::one(250), Value::U32(code("9999").encoded()));
    let frame = encode_command(&change, code("1234")).unwrap();
    device.lock().unwrap().handle(&frame);
    session.set_passphrase(code("9999"));
    assert!(session.refused(), "the same code handed back lifts nothing");
    session.connect().await.unwrap();
    assert!(
        !session.refused(),
        "the device accepts the code a refusal of an earlier link was about"
    );
    assert!(session.code_accepted());
}

/// A refusal of the connect's refresh, a stale advertisement the release ends: one reread
/// leaves the session keyed with the device's own code and connecting accepted.
#[tokio::test(start_paused = true)]
async fn a_reread_after_a_stale_advertisement_corrects_the_code() {
    let (transport, _device) = fake();
    transport.advertise(Advertised::UntilReleased(code("9999")));
    let mut session = auto_session(&transport);
    session.connect().await.unwrap();
    assert!(session.refused());
    assert_eq!(session.passphrase(), code("9999"));

    assert!(session.reread_code(WINDOW).await, "a fresh advertisement");
    assert!(!session.is_connected(), "the link was released");
    assert_eq!(transport.releases(), 1);
    assert_eq!(session.passphrase(), code("1234"));
    assert_eq!(
        session.advertisement().manufacturer.passphrase(),
        code("1234")
    );

    session.connect().await.unwrap();
    assert!(!session.refused());
    assert!(session.code_accepted());
    assert_eq!(transport.connects(), 2);
}

/// A device that is not heard again keeps the session as it was: the connect that follows is
/// the one it would have made anyway.
#[tokio::test(start_paused = true)]
async fn a_reread_that_hears_nothing_keeps_the_code() {
    let (transport, _device) = fake();
    transport.advertise(Advertised::UntilReleased(code("9999")));
    transport.go_quiet_after_release();
    let mut session = auto_session(&transport);
    session.connect().await.unwrap();

    assert!(!session.reread_code(WINDOW).await);
    assert_eq!(session.passphrase(), code("9999"));
    assert_eq!(transport.releases(), 1);

    session.connect().await.unwrap();
    assert!(session.refused(), "refused again, as it would have been");
}

/// A fresh advertisement that carries the refused code changes nothing but the link.
#[tokio::test(start_paused = true)]
async fn a_reread_that_hears_the_same_code_keeps_it() {
    let (transport, _device) = fake();
    transport.advertise(Advertised::Always(code("9999")));
    let mut session = auto_session(&transport);
    session.connect().await.unwrap();

    assert!(session.reread_code(WINDOW).await);
    assert_eq!(session.passphrase(), code("9999"));
    session.connect().await.unwrap();
    assert!(session.refused());
}

/// An episode drops once, only for `auto`, only for a refusal, and ends where it says it ends.
#[tokio::test(start_paused = true)]
async fn an_episode_gives_one_drop_for_auto_and_ends_where_it_says() {
    let (transport, _device) = fake();
    transport.advertise(Advertised::Always(code("9999")));
    let mut session = auto_session(&transport);
    let mut episode = Episode::default();
    assert!(!episode.due(true, &session), "no link, no refusal");

    session.connect().await.unwrap();
    assert!(episode.due(true, &session));
    assert!(
        !episode.due(false, &session),
        "an entry that is not auto never drops"
    );

    episode.dropped(session.passphrase());
    assert!(!episode.due(true, &session), "the drop is spent");
    assert_eq!(episode.listen_against(), Some(code("9999")));
    episode.listened();
    assert_eq!(episode.listen_against(), None);
    assert!(!episode.due(true, &session), "listening does not renew it");

    // A refusal is no proof; ending is.
    episode.observe(&session);
    assert!(!episode.due(true, &session));
    episode.end();
    assert!(episode.due(true, &session), "a new episode has its drop");
}

/// A device that shows its code works ends the episode.
#[tokio::test(start_paused = true)]
async fn a_proven_code_ends_the_episode() {
    let (transport, _device) = fake();
    let mut session = auto_session(&transport);
    session.connect().await.unwrap();
    assert!(session.code_accepted());

    let mut episode = Episode::default();
    episode.dropped(code("9999"));
    episode.observe(&session);
    assert_eq!(episode, Episode::default());
}

/// A transport over the fake whose first listen after a release hears the advertisement the
/// adapter held from before it, as a device-changed event raised by the release can read out.
struct Echoing {
    inner: Arc<FakeTransport>,
    held: Passphrase,
    echoed: std::sync::atomic::AtomicBool,
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

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        self.inner.connect(unit).await
    }
}

/// An advertisement the release itself brought out of the adapter's memory is not the device's
/// word: the session reads on to the one that follows.
#[tokio::test(start_paused = true)]
async fn a_reread_is_not_taken_in_by_the_advertisement_the_release_stirred_up() {
    let (fake, _device) = fake();
    fake.advertise(Advertised::UntilReleased(code("9999")));
    let transport = Arc::new(Echoing {
        inner: Arc::clone(&fake),
        held: code("9999"),
        echoed: false.into(),
    });
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let advert = fake.advertisement();
    let keyed = advert.manufacturer.passphrase();
    let mut session = Session::new(transport as Arc<dyn Transport>, advert, profile, keyed);
    session.connect().await.unwrap();
    assert!(session.refused());

    assert!(session.reread_code(WINDOW).await);
    assert_eq!(session.passphrase(), code("1234"));
    session.connect().await.unwrap();
    assert!(session.code_accepted());
}

/// What an episode is about is the code it was refused under: reading another one from the
/// advertisement ends it, reading the same one does not, and a refusal of the link made after
/// the drop is what the episode is about from then on.
#[test]
fn an_episode_is_about_the_code_that_was_refused() {
    let mut episode = Episode::default();
    episode.code_seen(code("1234"));
    assert_eq!(episode, Episode::default(), "nothing to end before a drop");

    episode.dropped(code("9999"));
    episode.listened();
    let spent = episode;
    episode.code_seen(code("9999"));
    assert_eq!(episode, spent, "the same code is the same refusal");

    episode.refused_again(code("5555"));
    episode.code_seen(code("9999"));
    assert_eq!(
        episode,
        Episode::default(),
        "9999 is no longer what it is about"
    );

    let mut again = spent;
    again.code_seen(code("1234"));
    assert_eq!(again, Episode::default(), "another code ends it");
}

/// A device that answers a refresh in full under the very code it was refused under proves
/// nothing, so the episode stays; under another code it ends.
#[tokio::test(start_paused = true)]
async fn a_proof_under_the_refused_code_ends_nothing() {
    let (transport, _device) = fake();
    let mut session = auto_session(&transport);
    session.connect().await.unwrap();
    assert!(session.code_accepted());

    let mut same = Episode::default();
    same.dropped(session.passphrase());
    let spent = same;
    same.observe(&session);
    assert_eq!(same, spent);

    let mut other = Episode::default();
    other.dropped(code("9999"));
    other.observe(&session);
    assert_eq!(other, Episode::default());
}
