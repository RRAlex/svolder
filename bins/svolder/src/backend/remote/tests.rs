// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::backend::Backend;
use crate::test_support::stub_daemon::{
    CANONICAL, CANONICAL_UNIT, OTHER, PRODUCT, STUB_VERSION, mute, stub_daemon,
};

/// How long a case waits for the stub daemon to answer.
const DEADLINE: Duration = Duration::from_secs(5);

/// Opening a daemon whose configuration names no device fails naming that daemon by the
/// socket it was reached on, and sends the user to add a device rather than to logs that
/// have nothing more to say.
#[tokio::test]
async fn opening_a_daemon_with_no_device_names_its_socket_and_the_step_to_add_one() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        daemon.empty();
        let Err(failure) = RemoteBackend::open(&daemon.socket, None, true).await else {
            panic!("a daemon with no device opened a session");
        };
        assert_eq!(failure.code, 2);
        assert!(
            failure
                .what
                .contains(&format!("the daemon on {}", daemon.socket.display())),
            "{failure}"
        );
        assert!(
            failure.next.contains(&format!(
                "`svolder --socket {} add <udid> --name NAME`",
                daemon.socket.display()
            )),
            "the step does not reach the daemon it names: {failure}"
        );
    })
    .await
    .expect("the case finished in time");
}

/// Opening the update feed leaves the backend able to write.
///
/// Subscribing consumes the client it is called on, so the feed gets a connection of its
/// own and this backend keeps the one every write goes over. Without that, every value key
/// in a daemon-backed TUI is refused.
#[tokio::test]
async fn a_write_goes_through_after_the_feed_is_open_and_its_echo_arrives_on_it() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        let mut backend = RemoteBackend::open(&daemon.socket, None, true)
            .await
            .expect("the stub daemon answers");
        backend.open_feed().await.expect("the feed opens");

        let hello = backend.next_status().await.expect("the feed reads");
        assert!(
            matches!(hello, Some(Update::Connection { connected: true })),
            "got {hello:?}"
        );

        let answer = backend
            .write(Request::Set {
                device: None,
                key: "mute".into(),
                value: "on".into(),
                step: 1,
                no_wait: true,
            })
            .await
            .expect("the write goes through with the feed open");
        assert!(
            answer.is_none(),
            "a no-wait write is acknowledged, not answered"
        );

        let echo = backend.next_status().await.expect("the feed reads");
        assert!(
            matches!(&echo, Some(Update::Status(vo)) if vo.display == "on"),
            "got {echo:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A recall the backend notes is held by the daemon, which lists it on the next open.
#[tokio::test]
async fn a_noted_recall_reaches_the_daemon_and_is_listed_at_the_next_open() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        let mut backend = RemoteBackend::open(&daemon.socket, None, true)
            .await
            .expect("the stub daemon answers");
        assert_eq!(backend.recalled_as_opened(), None);
        backend.note_recalled("Warm").await;
        assert_eq!(daemon.memory().as_deref(), Some("Warm"));
        let reopened = RemoteBackend::open(&daemon.socket, None, true)
            .await
            .expect("the stub daemon answers");
        assert_eq!(reopened.recalled_as_opened().as_deref(), Some("Warm"));
    })
    .await
    .expect("test timed out");
}

/// The device the daemon resolved is the one the feed asks for, and the only one it reads.
///
/// Opening with no selector leaves the daemon to pick which of its devices to answer for,
/// and its reply names the one it picked. The subscription asks for that device rather
/// than for all of them, and an event about another device — this stub fans every event
/// out to every subscriber — is not this backend's to report: its rows are one device's.
#[tokio::test]
async fn a_daemon_resolved_device_is_what_the_feed_asks_for_and_all_it_reads() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        let mut backend = RemoteBackend::open(&daemon.socket, None, true)
            .await
            .expect("the stub daemon answers");
        assert_eq!(
            backend.device.as_deref(),
            Some(CANONICAL),
            "the name the daemon answered for is not what the backend went on to name"
        );

        backend.open_feed().await.expect("the feed opens");
        // The hello event is the daemon saying it has the subscription and is pumping it,
        // so the selector it recorded on the way is there to read.
        let hello = backend.next_status().await.expect("the feed reads");
        assert!(
            matches!(hello, Some(Update::Connection { connected: true })),
            "got {hello:?}"
        );
        assert_eq!(
            daemon.subscribed(),
            vec![Some(CANONICAL.to_string())],
            "the subscription asked for every device the daemon holds"
        );

        daemon.publish(Event::Connection {
            device: OTHER.into(),
            connected: false,
        });
        daemon.publish(Event::Status {
            device: OTHER.into(),
            reading: mute("on"),
        });
        daemon.publish(Event::Status {
            device: CANONICAL.into(),
            reading: mute("off"),
        });

        let heard = backend.next_status().await.expect("the feed reads");
        assert!(
            matches!(&heard, Some(Update::Status(vo)) if vo.display == "off"),
            "the other device's events reached this backend: got {heard:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A selector the daemon had to resolve is replaced by the name it answered with, and
/// that name is what every later ask and every event is matched against.
///
/// `-d` takes a configured name, a product name or a unit-id prefix, in any case, while
/// the daemon answers and publishes under the one name it lists a device by. Keeping the
/// selector as it was typed would leave the feed matching a product name against that
/// canonical name and reading nothing at all, and the device list matching it against
/// nothing and showing no unit id.
#[tokio::test]
async fn a_selector_the_daemon_had_to_resolve_is_replaced_by_the_name_it_answered_with() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        let mut backend = RemoteBackend::open(&daemon.socket, Some(PRODUCT), true)
            .await
            .expect("the stub daemon answers");
        assert_eq!(
            backend.device.as_deref(),
            Some(CANONICAL),
            "the backend kept the selector rather than the name the daemon resolved"
        );
        assert_eq!(
            backend.unit().await.as_deref(),
            Some(CANONICAL_UNIT),
            "the device list is matched by a name it does not carry"
        );

        backend.open_feed().await.expect("the feed opens");
        let hello = backend.next_status().await.expect("the feed reads");
        assert!(
            matches!(hello, Some(Update::Connection { connected: true })),
            "got {hello:?}"
        );
        assert_eq!(
            daemon.subscribed(),
            vec![Some(CANONICAL.to_string())],
            "the subscription asked for a name the daemon does not publish under"
        );

        daemon.publish(Event::Status {
            device: CANONICAL.into(),
            reading: mute("on"),
        });
        let heard = backend.next_status().await.expect("the feed reads");
        assert!(
            matches!(&heard, Some(Update::Status(vo)) if vo.display == "on"),
            "the selected device's own events did not reach the backend: got {heard:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A device the daemon reported keeps the name and product it answered with, and its
/// link state is known either way round — with the wait before the next attempt while
/// the daemon has one: the daemon is the one holding the link.
#[test]
fn a_reported_device_becomes_a_row_carrying_its_link_state() {
    let mut info = DeviceInfo {
        signal: None,
        name: "gungnir".into(),
        unit: "0102030405060708".into(),
        product_id: 2,
        product: Some("Gungnir 2".into()),
        firmware: 3,
        profile: "gungnir2-v2".into(),
        connected: true,
        stale: false,
        retry_in_secs: None,
        passphrase_note: None,
        refused: false,
        recalled: None,
    };
    assert_eq!(
        device_row(&info),
        DeviceRow {
            name: "gungnir".into(),
            product: Some("Gungnir 2".into()),
            connected: Some(true),
            retry_in_secs: None,
            signal: None,
            profile: Some("gungnir2-v2".into()),
            product_id: Some(2),
            stale: false,
            refused: false,
        }
    );
    info.connected = false;
    info.retry_in_secs = Some(15);
    let away = device_row(&info);
    assert_eq!(away.connected, Some(false));
    assert_eq!(away.retry_in_secs, Some(15), "the wait was not carried");
    info.stale = true;
    info.refused = true;
    info.signal = Some(svolder_ipc::Signal {
        dbm: -58,
        source: svolder_ipc::SignalSource::Advert,
        heard_at_ms: 7,
    });
    let flagged = device_row(&info);
    assert!(
        flagged.stale && flagged.refused,
        "the flags were not carried"
    );
    assert_eq!(flagged.signal, info.signal, "the signal was not carried");
}

/// The daemon's own version, asked for over a connection of the request's own, so the
/// backend that made it stays free the whole time.
#[tokio::test]
async fn the_daemon_version_request_answers_over_its_own_connection() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        let backend = crate::backend::Backend::stub_remote(&daemon.socket).await;
        let request = backend
            .daemon_version_request()
            .expect("a remote backend has a daemon to ask");
        assert_eq!(
            request.await.map(|info| info.version).as_deref(),
            Some(STUB_VERSION)
        );
    })
    .await
    .expect("the case finished in time");
}

/// A socket file this case made, taken away once the case is over however it ends.
struct Removed(PathBuf);

impl Drop for Removed {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A daemon that takes the connection and never answers is given up on after the wait the
/// request is given, with no version: the request ends rather than living for the session.
#[tokio::test]
async fn a_daemon_that_never_answers_the_version_request_is_given_up_on() {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    let socket =
        std::env::temp_dir().join(format!("svolder-silent-{}-{n}.sock", std::process::id()));
    let listener = tokio::net::UnixListener::bind(&socket).expect("the socket binds");
    let _removed = Removed(socket.clone());
    let silent = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });
    let answer = tokio::time::timeout(
        DEADLINE,
        svolder_ipc::socket::daemon_version::version_within(&socket, Duration::from_millis(50)),
    )
    .await
    .expect("the request was not given up on in its wait");
    assert_eq!(answer, None);
    silent.abort();
}

/// A daemon that knows `history` answers the link changes it holds, over the span asked for.
#[tokio::test]
async fn the_daemon_s_link_history_is_asked_for_and_taken() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        let changes = vec![
            LinkChange {
                at_ms: 1_000,
                connected: true,
            },
            LinkChange {
                at_ms: 5_000,
                connected: false,
            },
        ];
        daemon.history(changes.clone());
        let mut backend = RemoteBackend::open(&daemon.socket, None, true)
            .await
            .expect("the stub daemon answers");
        assert_eq!(backend.link_history(86_400).await, Some(changes));
        assert_eq!(daemon.history_spans(), [86_400]);
    })
    .await
    .expect("the case finished in time");
}

/// A daemon too old to know `history` answers its unknown-request error, which is no history
/// to take: the front end goes on with its own observations.
#[tokio::test]
async fn an_older_daemon_s_unknown_request_error_is_no_history() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        let mut backend = RemoteBackend::open(&daemon.socket, None, true)
            .await
            .expect("the stub daemon answers");
        assert_eq!(backend.link_history(86_400).await, None);
        // The connection is still good for what comes next.
        assert!(backend.unit().await.is_some());
    })
    .await
    .expect("the case finished in time");
}

/// The daemon's note on the device's passphrase is read from its device list when it is asked
/// for, fresh each time, and a daemon that lists none (or is too old to judge) gives none.
#[tokio::test]
async fn the_daemon_s_passphrase_note_is_asked_for_and_taken_fresh() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        let backend = RemoteBackend::open(&daemon.socket, Some(CANONICAL), true)
            .await
            .expect("the stub daemon answers");
        assert_eq!(backend.passphrase_note().await, None);
        daemon.notes(Some(svolder_ipc::PassphraseNote::Differs));
        assert_eq!(
            backend.passphrase_note().await,
            Some(svolder_ipc::PassphraseNote::Differs)
        );
        daemon.notes(None);
        assert_eq!(backend.passphrase_note().await, None);
        let mut whole = super::super::Backend::Remote(Box::new(backend));
        daemon.notes(Some(svolder_ipc::PassphraseNote::NoneConfigured));
        let line = whole.passphrase_note_line().await.expect("a line");
        assert!(line.starts_with("fake: "), "{line}");
    })
    .await
    .expect("test timed out");
}

/// The note and the device's own row come from one device list: a single ask of the daemon.
#[tokio::test]
async fn the_note_and_the_row_are_taken_from_one_ask() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        let backend = RemoteBackend::open(&daemon.socket, Some(CANONICAL), true)
            .await
            .expect("the stub daemon answers");
        let backend = super::super::Backend::Remote(Box::new(backend));
        daemon.notes(Some(svolder_ipc::PassphraseNote::Differs));
        let asked = daemon.device_asks();
        let facts = backend.own_facts().await;
        assert_eq!(daemon.device_asks(), asked + 1, "more than one ask");
        assert_eq!(facts.note, Some(svolder_ipc::PassphraseNote::Differs));
        let row = facts.row.expect("the device's row");
        assert_eq!(row.name, CANONICAL);
        assert_eq!(row.product.as_deref(), Some("Gungnir 2"));
    })
    .await
    .expect("test timed out");
}

/// The note is found for a device named the way the daemon resolves a selector — in another case,
/// or by a prefix of its unit id — and the question is bounded at a second, as the device list's
/// own is.
#[tokio::test]
async fn the_note_is_found_by_any_selector_the_daemon_resolves_and_the_ask_is_bounded() {
    tokio::time::timeout(DEADLINE, async {
        let daemon = stub_daemon().await;
        daemon.notes(Some(svolder_ipc::PassphraseNote::Differs));
        let backend = RemoteBackend::open(&daemon.socket, Some(CANONICAL), true)
            .await
            .expect("the stub daemon answers");
        for selector in ["FAKE", "01020304", PRODUCT] {
            assert_eq!(
                backend.note_of(Some(selector)).await,
                Some(svolder_ipc::PassphraseNote::Differs),
                "{selector}"
            );
        }
    })
    .await
    .expect("test timed out");
    assert!(NOTE_WAIT <= Duration::from_secs(1));
}

/// Opening over the daemon reads its device list once: the rows, the unit id and the passphrase
/// note as opened all come from that one read.
#[tokio::test]
async fn opening_over_the_daemon_asks_for_the_device_list_once() {
    let daemon = stub_daemon().await;
    let (mut backend, rows) = Backend::open_on(&daemon.socket, None)
        .await
        .expect("the stub answers");
    assert_eq!(rows.len(), 2);
    assert!(backend.unit().await.is_some());
    assert!(backend.passphrase_note_as_opened().is_none());
    assert_eq!(daemon.device_asks(), 1);
    backend.close().await;
}

/// A backend opened without connecting reads no device, so it holds no readings and sends no
/// state read.
#[tokio::test]
async fn a_backend_opened_without_connecting_holds_no_readings_and_asks_for_none() {
    let daemon = stub_daemon().await;
    let remote = RemoteBackend::open(&daemon.socket, None, false)
        .await
        .expect("connects");
    assert!(remote.state().is_empty());
    assert!(daemon.asked_for().is_empty(), "a state read was sent");
}
