// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use svolder_ipc::config::{WebAuth, WebGuest};
use svolder_ipc::{Response, WebStatus};

use crate::test_support::stub_daemon::{STUB_FRESH_LINK_CODE, STUB_LINK_CODE, STUB_PAIR_CODE};
use crate::tui::app::{ConnChange, Intent};
use crate::tui::boxes::connections::Modal;
use crate::tui::boxes::web_box::WebBox;

use super::super::test_support::*;
use super::super::*;

fn network() -> WebStatus {
    WebStatus::Listening {
        listen: "0.0.0.0:8738".parse().unwrap(),
        auth: WebAuth::Token,
        writes: true,
        guest: WebGuest::None,
    }
}

/// A link is asked of the daemon and shown with the base a phone reaches; once it ends while
/// the box is open, a fresh one is asked for.
#[tokio::test]
async fn a_link_comes_from_the_daemon_and_is_replaced_when_it_ends() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    let link = web.link.clone().expect("a link");
    assert!(
        link.text.ends_with(&format!("/#login={STUB_LINK_CODE}")),
        "{}",
        link.text
    );
    assert!(link.until > Instant::now() + Duration::from_secs(100));
    if let Some(Modal::Web(web)) = &mut watching.app.overlays.conn.modal {
        web.link.as_mut().unwrap().until = Instant::now();
    }
    web_box::tick(&mut watching, Instant::now());
    web_box::settle(&mut watching).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert!(
        web.link.as_ref().unwrap().until > Instant::now(),
        "the ended link was not replaced"
    );
}

/// The link on show, as the box holds it.
fn shown_link(watching: &Watching) -> crate::tui::boxes::web_box::Code {
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    web.link.clone().expect("a link")
}

/// Whether the box says that the link on show replaced one that was used.
fn says_replaced(watching: &Watching) -> bool {
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    web.link_replaced
}

/// A link the daemon says a browser used is replaced by a fresh one, whole (text and expiry), and
/// the box says so; the check asks nothing while no link shows.
#[tokio::test]
async fn a_link_a_browser_used_is_replaced_by_a_fresh_one() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    web_box::check(&mut watching, Instant::now());
    assert!(watching.web_check.is_none(), "a check with no link on show");
    assert!(!web_box::checking(&watching, Instant::now()));
    assert_eq!(daemon.link_open_asks(), 0);
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    assert!(shown_link(&watching).text.ends_with(STUB_LINK_CODE));
    assert!(web_box::checking(&watching, Instant::now()));
    daemon.spends_link(STUB_LINK_CODE);
    web_box::check(&mut watching, Instant::now());
    web_box::settle_check(&mut watching).await;
    web_box::settle(&mut watching).await;
    let link = shown_link(&watching);
    assert!(link.text.ends_with(STUB_FRESH_LINK_CODE), "{}", link.text);
    assert!(link.until > Instant::now() + Duration::from_secs(100));
    assert!(says_replaced(&watching), "the box did not say it");
    assert_eq!(daemon.link_open_asks(), 1);
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert!(!web.link_replacing, "the pending note stayed");
    // The fresh link is open: the next check leaves it, and so does the note.
    web_box::check(&mut watching, Instant::now());
    web_box::settle_check(&mut watching).await;
    assert!(watching.web_code.is_none());
    assert!(shown_link(&watching).text.ends_with(STUB_FRESH_LINK_CODE));
    assert!(says_replaced(&watching));
    // A link asked for by hand carries no such note.
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    assert!(!says_replaced(&watching));
}

/// A link still open is left alone, and so is an answer from a daemon that cannot say.
#[tokio::test]
async fn an_open_link_is_not_replaced() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    web_box::check(&mut watching, Instant::now());
    web_box::settle_check(&mut watching).await;
    assert!(watching.web_code.is_none(), "an open link was replaced");
    assert!(shown_link(&watching).text.ends_with(STUB_LINK_CODE));
    assert!(!says_replaced(&watching));
    assert_eq!(daemon.link_open_asks(), 1);
}

/// A daemon that answers an error, or does not answer, is not asked again about that link, and a
/// new link is asked about afresh.
#[tokio::test]
async fn a_daemon_that_cannot_say_is_not_asked_again_about_that_link() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let code = STUB_LINK_CODE.to_string();
    let mut shown = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut shown).await;
    // No answer (a daemon restarting) is no verdict either way: the next second asks again.
    web_box::checked(&mut shown, (code.clone(), None), Instant::now());
    assert!(shown.web_code.is_none(), "no answer replaced the link");
    assert!(web_box::checking(&shown, Instant::now()));
    let error = Some(Response::Error {
        code: 1,
        message: "unknown request".into(),
    });
    web_box::checked(&mut shown, (code, error), Instant::now());
    assert!(shown.web_code.is_none(), "an error replaced the link");
    assert!(!web_box::checking(&shown, Instant::now()));
    web_box::check(&mut shown, Instant::now());
    assert!(shown.web_check.is_none(), "a link was asked about again");
    assert_eq!(daemon.link_open_asks(), 0);
}

/// A link with a second or less left is not asked about and a spent verdict on it does not
/// rotate it: a code that has just ended on the daemon is not a code a browser used.
#[tokio::test]
async fn a_link_about_to_end_is_not_rotated_as_used() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    if let Some(Modal::Web(web)) = &mut watching.app.overlays.conn.modal {
        web.link.as_mut().unwrap().until = Instant::now() + Duration::from_millis(500);
    }
    assert!(!web_box::checking(&watching, Instant::now()));
    let spent = (
        STUB_LINK_CODE.to_string(),
        Some(Response::WebLinkOpen { open: false }),
    );
    web_box::checked(&mut watching, spent, Instant::now());
    assert!(watching.web_code.is_none(), "a link about to end rotated");
    assert!(!says_replaced(&watching));
}

/// A spent verdict while a code ask is in flight asks for nothing more and says nothing; the box
/// being closed, a verdict changes nothing.
#[tokio::test]
async fn a_spent_verdict_is_ignored_while_an_ask_is_in_flight_or_the_box_is_closed() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    daemon.spends_link(STUB_LINK_CODE);
    // A hand ask is on its way when the verdict comes.
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    assert!(watching.web_code.is_some());
    let spent = (
        STUB_LINK_CODE.to_string(),
        Some(Response::WebLinkOpen { open: false }),
    );
    web_box::checked(&mut watching, spent.clone(), Instant::now());
    web_box::settle(&mut watching).await;
    assert!(watching.web_code.is_none(), "a second ask was made");
    assert!(shown_link(&watching).text.ends_with(STUB_FRESH_LINK_CODE));
    assert!(!says_replaced(&watching), "the note followed a hand ask");
    // The box closes; the verdict (on the link it held) changes nothing.
    watching.app.overlays.conn.modal = None;
    web_box::checked(&mut watching, spent, Instant::now());
    assert!(watching.web_code.is_none());
    assert!(watching.app.overlays.conn.modal.is_none());
}

/// A link that is refused when it replaces a spent one leaves no note, and none is held for the
/// link asked for next.
#[tokio::test]
async fn a_refused_replacement_shows_no_note() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    let spent = (
        STUB_LINK_CODE.to_string(),
        Some(Response::WebLinkOpen { open: false }),
    );
    web_box::checked(&mut watching, spent, Instant::now());
    daemon.says_web(Some(WebStatus::Off), None);
    web_box::settle(&mut watching).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert!(web.link.is_none() && !web.link_replaced && !web.link_replacing);
    daemon.says_web(Some(network()), None);
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    assert!(!says_replaced(&watching));
}

/// A link that has ended stays as it is for the tick to replace, as a spent one that ended is not
/// replaced by the check; an answer about a link no longer on show changes nothing.
#[tokio::test]
async fn an_ended_link_and_an_answer_about_another_link_are_not_rotated() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    daemon.spends_link(STUB_LINK_CODE);
    let spent = (
        STUB_LINK_CODE.to_string(),
        Some(Response::WebLinkOpen { open: false }),
    );
    // The verdict is about some other link than the one on show.
    let other = ("another-code".to_string(), spent.1.clone());
    web_box::checked(&mut watching, other, Instant::now());
    assert!(watching.web_code.is_none());
    // The link on show ended before the verdict came.
    if let Some(Modal::Web(web)) = &mut watching.app.overlays.conn.modal {
        web.link.as_mut().unwrap().until = Instant::now();
    }
    web_box::checked(&mut watching, spent, Instant::now());
    assert!(watching.web_code.is_none(), "an ended link was rotated");
    assert!(!says_replaced(&watching));
    assert!(!web_box::checking(&watching, Instant::now()));
    web_box::check(&mut watching, Instant::now());
    assert!(watching.web_check.is_none());
    assert_eq!(daemon.link_open_asks(), 0, "an ended link was asked about");
}

/// A pairing code lands on its row; a daemon whose web UI is off answers with the way to turn it
/// on, which the box says.
#[tokio::test]
async fn a_pairing_code_lands_and_an_off_web_ui_says_how_to_turn_it_on() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let mut watching = taken(Intent::WebPair, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert_eq!(
        web.pair.as_ref().map(|c| c.text.as_str()),
        Some(STUB_PAIR_CODE)
    );
    daemon.says_web(Some(WebStatus::Off), None);
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    let said = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .unwrap_or_default();
    assert!(said.contains("the web UI is off"), "{said}");
}

/// Web UI and Reach write `[web]` in the file and have the daemon reload, keeping the port.
#[tokio::test]
async fn the_web_rows_write_their_keys_and_reload() {
    let (daemon, path, watching) = on_a_daemon().await;
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());
    let watching = taken(
        Intent::Connection(ConnChange::WebListen("0.0.0.0:8738".parse().unwrap())),
        &opener,
        watching,
        &mut look,
    )
    .await;
    let _ = taken(
        Intent::Connection(ConnChange::WebEnabled(true)),
        &opener,
        watching,
        &mut look,
    )
    .await;
    let config = svolder_ipc::Config::load_from(&path).unwrap();
    assert!(config.web.enabled);
    assert_eq!(config.web.listen, "0.0.0.0:8738".parse().unwrap());
    assert_eq!(daemon.reloads(), 2);
}

/// Copying the link sends it by OSC 52; the box asks for no code while it is closed.
#[tokio::test]
async fn the_link_is_copied_and_no_code_is_asked_for_while_the_box_is_closed() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    let web = WebBox {
        link: Some(crate::tui::boxes::web_box::Code {
            text: "http://192.168.1.20:8738/#login=abc".into(),
            until: Instant::now() + Duration::from_secs(90),
        }),
        ..WebBox::default()
    };
    watching.app.overlays.conn.modal = Some(Modal::Web(web));
    let opener = opener_on_nothing(path);
    let (mut look, sent) = look_over(&state_home());
    let mut watching = taken(Intent::CopyWebLink, &opener, watching, &mut look).await;
    let copied = sent.lock().unwrap().clone();
    assert_eq!(
        copied,
        [crate::tui::boxes::connections::osc52(
            "http://192.168.1.20:8738/#login=abc"
        )]
    );
    watching.app.overlays.conn.modal = None;
    web_box::tick(&mut watching, Instant::now() + Duration::from_secs(600));
    assert!(
        watching.web_code.is_none(),
        "a code was asked for with the box closed"
    );
    let watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    assert!(
        watching.web_code.is_none(),
        "a link was asked for with the box closed"
    );
}

/// Replacing the web token writes a fresh one into the web store alone, has the daemon reload
/// and takes the new tail in; a token from the environment is refused, naming the variable.
#[tokio::test]
async fn replacing_the_web_token_touches_only_the_web_store_and_reloads() {
    let (daemon, path, watching) = on_a_daemon().await;
    let home = state_home();
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&home);
    let watching = taken(Intent::RotateWebToken, &opener, watching, &mut look).await;
    let made = svolder_ipc::token::peek_with(svolder_ipc::token::Store::WEB, None, None, &home)
        .unwrap()
        .expect("a web token was made");
    let mcp = svolder_ipc::token::peek_with(svolder_ipc::token::Store::MCP, None, None, &home);
    assert!(matches!(mcp, Ok(None)), "the MCP token was touched");
    assert_eq!(daemon.reloads(), 1);
    let crate::tui::boxes::connections::TokenView::Present { masked, .. } =
        &watching.app.overlays.conn.web_token
    else {
        panic!("the new tail was not taken in");
    };
    assert!(
        masked.ends_with(&svolder_ipc::token::tail(made.value())),
        "{masked}"
    );

    look.token_site.web_env = Some("fake-env-web-token-not-a-secret".into());
    let watching = taken(Intent::RotateWebToken, &opener, watching, &mut look).await;
    let said = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .unwrap_or_default();
    assert!(said.contains("SVOLDER_WEB_TOKEN"), "{said}");
    assert_eq!(daemon.reloads(), 1, "a refused replacement told the daemon");
}

/// A link that ended while the web UI is off is not asked for again every tick: the refusal is
/// said once and the ended link goes.
#[tokio::test]
async fn a_refused_link_replacement_is_said_once_and_not_asked_again() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(WebStatus::Off), None);
    let web = WebBox {
        link: Some(crate::tui::boxes::web_box::Code {
            text: "http://192.168.1.20:8738/#login=abc".into(),
            until: Instant::now(),
        }),
        ..WebBox::default()
    };
    watching.app.overlays.conn.modal = Some(Modal::Web(web));
    let opener = opener_on_nothing(path);
    let (_look, _) = look_over(&state_home());
    web_box::tick(&mut watching, Instant::now());
    assert!(
        watching.web_code.is_some(),
        "the ended link was not asked for"
    );
    web_box::settle(&mut watching).await;
    let said = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .unwrap_or_default();
    assert!(said.contains("the web UI is off"), "{said}");
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert!(web.link.is_none(), "the ended link stayed");
    web_box::tick(&mut watching, Instant::now() + Duration::from_secs(1));
    assert!(
        watching.web_code.is_none(),
        "a refused link was asked for again"
    );
    drop(opener);
}

/// A link made for one listen address is replaced once the address changes while the box is
/// open, so its QR code never carries the old base.
#[tokio::test]
async fn a_link_is_replaced_when_the_listen_address_changes() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    watching.app.overlays.conn.web.listen = "127.0.0.1:8738".parse().unwrap();
    watching.app.overlays.conn.web_status = Some(network());
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert_eq!(
        web.link_for.map(|(file, _)| file),
        Some("127.0.0.1:8738".parse().unwrap())
    );
    watching.app.overlays.conn.web.listen = "0.0.0.0:8738".parse().unwrap();
    web_box::tick(&mut watching, Instant::now());
    web_box::settle(&mut watching).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert_eq!(
        web.link_for.map(|(file, _)| file),
        Some("0.0.0.0:8738".parse().unwrap())
    );
    // With nothing changed, a tick asks for nothing.
    web_box::tick(&mut watching, Instant::now());
    assert!(
        watching.web_code.is_none(),
        "an unchanged link was asked for again"
    );
}

/// A link whose base is this computer's because none a phone reaches was found is marked, for
/// the box to say so instead of drawing its QR code.
#[tokio::test]
async fn a_link_on_the_fallback_base_is_marked_local() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    watching.app.overlays.conn.web.url = Some("not an address".into());
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert!(web.link_local, "the loopback fallback was not marked");
    watching.app.overlays.conn.web.url = Some("https://svolder.home.lan/".into());
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle(&mut watching).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert!(!web.link_local, "a url a phone can use was marked local");
}

/// Asking for a code while another ask is in flight says so instead of dropping the key.
#[tokio::test]
async fn a_second_ask_while_one_is_in_flight_says_so() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    let mut watching = taken(Intent::WebPair, &opener, watching, &mut look).await;
    let said = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .unwrap_or_default();
    assert!(said.contains("already asking"), "{said}");
    web_box::settle(&mut watching).await;
}

/// A web box showing a link and a pairing code, both good for another ninety seconds.
fn web_box_with_codes() -> WebBox {
    let code = |text: &str| crate::tui::boxes::web_box::Code {
        text: text.into(),
        until: Instant::now() + Duration::from_secs(90),
    };
    WebBox {
        link: Some(code("http://192.168.1.20:8738/#login=abc")),
        pair: Some(code("123456")),
        link_for: Some((
            "0.0.0.0:8738".parse().unwrap(),
            "0.0.0.0:8738".parse().unwrap(),
        )),
        link_local: true,
        ..WebBox::default()
    }
}

/// A token replace that succeeds ends the codes the daemon minted: the box drops the link and the
/// pairing code it shows, and a refusal leaves them.
#[tokio::test]
async fn a_replaced_web_token_clears_the_shown_link_and_code_and_a_refusal_keeps_them() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    look.token_site.web_env = Some("fake-env-web-token-not-a-secret".into());
    let watching = taken(Intent::RotateWebToken, &opener, watching, &mut look).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert!(
        web.link.is_some() && web.pair.is_some(),
        "a refusal cleared the codes"
    );
    look.token_site.web_env = None;
    let watching = taken(Intent::RotateWebToken, &opener, watching, &mut look).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert!(web.link.is_none(), "the link outlived the replace");
    assert!(web.pair.is_none(), "the pairing code outlived the replace");
    assert!(web.link_for.is_none() && !web.link_local);
    drop(watching);
}

/// A code asked just before a token replace is dropped with the codes shown: its answer would
/// carry a code minted under the old token.
#[tokio::test]
async fn a_code_in_flight_is_dropped_when_the_codes_are_forgotten() {
    let (daemon, _path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    web_box::ask(&mut watching, true);
    assert!(watching.web_code.is_some(), "the ask did not go out");
    web_box::forget_codes(&mut watching);
    assert!(watching.web_code.is_none(), "the ask in flight stayed");
    web_box::ask(&mut watching, true);
    assert!(
        watching.web_code.is_some(),
        "a new code could not be asked for"
    );
}

/// A look built for a case refuses every link without running an opener.
#[test]
fn a_test_look_refuses_every_link() {
    let (look, _) = look_over(&state_home());
    assert_eq!(
        (look.browser)("http://127.0.0.1:1/").unwrap_err(),
        "no browser in a test"
    );
}

/// What a test sees of a look: the links the clipboard was sent and the browser was asked for.
type Record = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

/// A look whose browser records the links it was asked to open and answers `answer`.
fn look_opening(answer: Result<(), String>) -> (crate::tui::run::Look, Record, Record) {
    let (mut look, copies) = look_over(&state_home());
    let opened = Record::default();
    let record = std::sync::Arc::clone(&opened);
    look.browser = std::sync::Arc::new(move |link| {
        record.lock().unwrap().push(link.to_string());
        answer.clone()
    });
    (look, copies, opened)
}

/// The text of the toast showing, or empty.
fn said(watching: &Watching) -> String {
    watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .unwrap_or_default()
}

/// A click on the link sends it to the terminal, hands it to the browser opener and says both,
/// without calling an OSC 52 send a copy.
#[tokio::test]
async fn a_click_on_the_link_copies_it_and_opens_it() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, copies, opened) = look_opening(Ok(()));
    let mut watching = taken(Intent::OpenWebLink, &opener, watching, &mut look).await;
    let link = "http://192.168.1.20:8738/#login=abc";
    assert_eq!(
        *copies.lock().unwrap(),
        [crate::tui::boxes::connections::osc52(link)]
    );
    assert!(
        said(&watching).contains("copying the link and opening the browser"),
        "{}",
        said(&watching)
    );
    web_box::settle_copy(&mut watching).await;
    assert_eq!(*opened.lock().unwrap(), [link]);
    let said = said(&watching);
    assert!(
        said.contains("sent to the terminal's clipboard (OSC 52)"),
        "{said}"
    );
    assert!(said.contains("opened in the browser"), "{said}");
    assert!(
        !said.contains("copied"),
        "OSC 52 alone is not a copy: {said}"
    );
}

/// An opener that fails, or a session with no desktop, leaves the copy done and a note saying why
/// no browser was opened: not an error toast.
#[tokio::test]
async fn a_browser_that_cannot_open_leaves_the_copy_and_a_note() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, copies, _) = look_opening(Err("no desktop".into()));
    let mut watching = taken(Intent::OpenWebLink, &opener, watching, &mut look).await;
    assert_eq!(copies.lock().unwrap().len(), 1);
    web_box::settle_copy(&mut watching).await;
    let toast = watching.app.toast().expect("a toast");
    assert_eq!(
        toast.kind,
        crate::tui::boxes::toast::ToastKind::Info,
        "a warning, not a note"
    );
    assert!(
        toast.text.contains("OSC 52") && toast.text.contains("no browser was opened (no desktop)"),
        "{}",
        toast.text
    );
}

/// An opener that does not come back does not hold the loop: the click returns at once having
/// sent the sequence, a second click while it runs starts no second opener, and the outcome is
/// said when the opener is released.
#[tokio::test]
async fn a_slow_opener_does_not_hold_the_loop() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, copies) = look_over(&state_home());
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = std::sync::Mutex::new(gate);
    let calls = Record::default();
    let seen = Record::clone(&calls);
    look.browser = std::sync::Arc::new(move |link| {
        seen.lock().unwrap().push(link.to_string());
        gate.lock().unwrap().recv().map_err(|e| e.to_string())
    });
    let started = Instant::now();
    let watching = taken(Intent::OpenWebLink, &opener, watching, &mut look).await;
    let mut watching = taken(Intent::OpenWebLink, &opener, watching, &mut look).await;
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the click waited for the opener"
    );
    assert_eq!(copies.lock().unwrap().len(), 2, "both clicks copy");
    assert!(watching.web_copy.is_some());
    let said_now = said(&watching);
    assert!(
        said_now.contains("link sent to the terminal; the clipboard tool and the browser get it"),
        "{said_now}"
    );
    release.send(()).unwrap();
    web_box::settle_copy(&mut watching).await;
    assert!(
        watching.web_copy.is_some(),
        "the waiting click did not start"
    );
    release.send(()).unwrap();
    web_box::settle_copy(&mut watching).await;
    assert_eq!(
        calls.lock().unwrap().len(),
        2,
        "each click opens the browser, one opener at a time"
    );
    let said_late = said(&watching);
    assert!(said_late.contains("opened in the browser"), "{said_late}");
}

/// A copy task that ends without an answer drops the copy waiting behind it, so it cannot run
/// after a newer one and leave the clipboard holding older text.
#[tokio::test]
async fn a_copy_that_ends_unanswered_drops_the_one_waiting() {
    let (_daemon, _path, mut watching) = on_a_daemon().await;
    let (look, _sent, _tools) = look_copying(&[], crate::tui::clipboard::Tried::NoDisplay);
    watching.web_copy_next = Some(web_box::Next {
        what: "link",
        text: LINK.to_string(),
        terminal: None,
        copier: std::sync::Arc::clone(&look.copier),
        browser: None,
    });
    web_box::copy_ended(&mut watching, None);
    assert!(watching.web_copy_next.is_none() && watching.web_copy.is_none());
}

/// A click on the link made while a copy is in flight opens the browser when that copy ends, and
/// its note does not say the link was sent again.
#[tokio::test]
async fn a_click_made_while_a_copy_is_in_flight_opens_the_browser_after_it() {
    use crate::tui::clipboard::Tried;
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, _sent, tools) = look_copying(&["DISPLAY"], Tried::Copied("xclip"));
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = std::sync::Mutex::new(gate);
    let record = std::sync::Arc::clone(&tools);
    look.copier = std::sync::Arc::new(move |text| {
        let first = record.lock().unwrap().is_empty();
        record.lock().unwrap().push(text.to_string());
        if first {
            let _ = gate.lock().unwrap().recv();
        }
        Tried::Copied("xclip")
    });
    let calls = Record::default();
    let seen = Record::clone(&calls);
    look.browser = std::sync::Arc::new(move |link| {
        seen.lock().unwrap().push(link.to_string());
        Ok(())
    });
    let watching = taken(Intent::CopyWebLink, &opener, watching, &mut look).await;
    let mut watching = taken(Intent::OpenWebLink, &opener, watching, &mut look).await;
    let now = said(&watching);
    assert!(
        !now.contains("again") && now.contains("link sent to the terminal;"),
        "{now}"
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "the browser waits its turn"
    );
    release.send(()).unwrap();
    web_box::settle_copy(&mut watching).await;
    web_box::settle_copy(&mut watching).await;
    assert_eq!(
        *calls.lock().unwrap(),
        [LINK],
        "the browser opened the link"
    );
    let said = said(&watching);
    assert!(said.contains("opened in the browser"), "{said}");
}

/// A click with no link on show, or one that has ended, copies and opens nothing.
#[tokio::test]
async fn a_click_with_no_link_shown_does_nothing() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, copies, opened) = look_opening(Ok(()));
    let mut watching = taken(Intent::OpenWebLink, &opener, watching, &mut look).await;
    if let Some(Modal::Web(web)) = &mut watching.app.overlays.conn.modal {
        *web = web_box_with_codes();
        web.link.as_mut().unwrap().until = Instant::now();
    }
    watching.app.now = Instant::now() + Duration::from_secs(1);
    let watching = taken(Intent::OpenWebLink, &opener, watching, &mut look).await;
    assert!(copies.lock().unwrap().is_empty());
    assert!(opened.lock().unwrap().is_empty());
    assert_eq!(said(&watching), "");
}

/// With no desktop session the opener is not started at all (it would take the terminal for a
/// terminal browser); with one it is run.
#[test]
fn no_desktop_session_means_no_opener_is_started() {
    use std::ffi::OsString;
    use std::path::Path;
    let env = |set: &'static [&'static str]| {
        move |name: &str| set.contains(&name).then(|| OsString::from("x"))
    };
    assert!(!web_box::desktop_session(env(&[])));
    assert!(!web_box::desktop_session(|_| Some(OsString::new())));
    assert!(web_box::desktop_session(env(&["DISPLAY"])));
    assert!(web_box::desktop_session(env(&["WAYLAND_DISPLAY"])));
    let missing = Path::new("/nonexistent/xdg-open");
    let why = web_box::open_in_browser(false, missing, "http://127.0.0.1:8738/").unwrap_err();
    assert_eq!(why, "no desktop");
    let why = web_box::open_in_browser(true, missing, "http://127.0.0.1:8738/").unwrap_err();
    assert!(why.contains("could not be run"), "{why}");
    assert!(
        web_box::open_in_browser(true, Path::new("/bin/true"), "http://127.0.0.1:8738/").is_ok()
    );
}

/// What a test sees of the clipboard tool: the texts it was handed.
type Tools = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

/// A look whose environment holds `set` (each with a value) and whose clipboard tool records
/// what it is handed and answers `tried`.
fn look_copying(
    set: &'static [&'static str],
    tried: crate::tui::clipboard::Tried,
) -> (crate::tui::run::Look, Record, Tools) {
    let (mut look, sent) = look_over(&state_home());
    look.env =
        std::sync::Arc::new(move |name| set.contains(&name).then(|| std::ffi::OsString::from("1")));
    let tools = Tools::default();
    let record = std::sync::Arc::clone(&tools);
    look.copier = std::sync::Arc::new(move |text| {
        record.lock().unwrap().push(text.to_string());
        tried.clone()
    });
    (look, sent, tools)
}

const LINK: &str = "http://192.168.1.20:8738/#login=abc";

/// `c` sends the sequence plain and wrapped for tmux and hands the text to the tool, and a tool that ran
/// is named: "copied with wl-copy".
#[tokio::test]
async fn a_tool_that_ran_is_named_and_the_sequence_is_wrapped_for_tmux() {
    use crate::tui::clipboard::Tried;
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, sent, tools) = look_copying(&["TMUX"], Tried::Copied("wl-copy"));
    let mut watching = taken(Intent::CopyWebLink, &opener, watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;
    let plain = crate::tui::boxes::connections::osc52(LINK);
    let expected = format!(
        "{plain}\x1bPtmux;{}\x1b\\",
        plain.replace('\x1b', "\x1b\x1b")
    );
    assert!(expected.starts_with("\x1b]52;") && expected.contains("\x1bPtmux;\x1b\x1b]52;"));
    assert_eq!(*sent.lock().unwrap(), [expected]);
    assert_eq!(*tools.lock().unwrap(), [LINK]);
    assert_eq!(said(&watching), "link copied with wl-copy");
}

/// With no display the box says the sequence was sent, never that it was copied, and points at
/// the terminal's own link handling.
#[tokio::test]
async fn with_no_display_the_note_says_sent_and_points_at_the_links_menu() {
    use crate::tui::clipboard::Tried;
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, sent, _) = look_copying(&["STY"], Tried::NoDisplay);
    let mut watching = taken(Intent::CopyWebLink, &opener, watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;
    let expected = format!("\x1bP{}\x1b\\", crate::tui::boxes::connections::osc52(LINK));
    assert_eq!(*sent.lock().unwrap(), [expected], "screen's wrapping");
    let said = said(&watching);
    assert_eq!(
        said,
        "link sent to the terminal's clipboard (OSC 52) — if your terminal ignores it, \
         Ctrl+click the link to open it, Shift+right-click it to copy it"
    );
}

/// A tool that is missing or fails leaves the sequence's note and says why the tool did not run,
/// as a note, not an error.
#[tokio::test]
async fn a_failing_tool_is_said_in_a_note() {
    use crate::tui::clipboard::Tried;
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let why = "xclip did not run: exited with exit status: 1";
    let (mut look, _, _) = look_copying(&["DISPLAY"], Tried::Failed(why.into()));
    let mut watching = taken(Intent::CopyWebLink, &opener, watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;
    let toast = watching.app.toast().expect("a toast");
    assert_eq!(toast.kind, crate::tui::boxes::toast::ToastKind::Info);
    assert_eq!(
        toast.text,
        format!("link sent to the terminal's clipboard (OSC 52); {why}")
    );
}

/// A pairing code is copied when it is shown, the same way (sequence and tool, no hyperlink
/// involved), while a link is not copied just by being shown.
#[tokio::test]
async fn a_pairing_code_is_copied_when_it_is_shown_and_a_link_is_not() {
    use crate::tui::clipboard::Tried;
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let (mut look, sent, tools) = look_copying(&["WAYLAND_DISPLAY"], Tried::Copied("wl-copy"));
    let mut watching = taken(Intent::WebLink, &opener, watching, &mut look).await;
    web_box::settle_and_copy(&mut watching, &mut look).await;
    assert!(
        sent.lock().unwrap().is_empty(),
        "a link was copied by showing it"
    );
    assert!(tools.lock().unwrap().is_empty());
    let mut watching = taken(Intent::WebPair, &opener, watching, &mut look).await;
    web_box::settle_and_copy(&mut watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;
    assert_eq!(
        *sent.lock().unwrap(),
        [crate::tui::boxes::connections::osc52(STUB_PAIR_CODE)]
    );
    assert_eq!(*tools.lock().unwrap(), [STUB_PAIR_CODE]);
    assert_eq!(said(&watching), "code copied with wl-copy");
}

/// A pairing code shown while its tool fails or is missing still shows; the note says it was only
/// sent to the terminal and why the tool did not run, with no error box.
#[tokio::test]
async fn a_pairing_code_stays_shown_when_its_tool_fails() {
    use crate::tui::clipboard::Tried;
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
    let opener = opener_on_nothing(path);
    let why = "no clipboard tool is installed (xclip, xsel)";
    let (mut look, _, _) = look_copying(&["DISPLAY"], Tried::Failed(why.into()));
    let mut watching = taken(Intent::WebPair, &opener, watching, &mut look).await;
    web_box::settle_and_copy(&mut watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        panic!("closed")
    };
    assert_eq!(
        web.pair.as_ref().map(|c| c.text.as_str()),
        Some(STUB_PAIR_CODE)
    );
    let toast = watching.app.toast().expect("a toast");
    assert_eq!(
        toast.kind,
        crate::tui::boxes::toast::ToastKind::Info,
        "an error box"
    );
    assert_eq!(
        toast.text,
        format!("code sent to the terminal's clipboard (OSC 52); {why}")
    );
    // With no display at all the way round is to read it off the box.
    let (mut look, _, _) = look_copying(&[], Tried::NoDisplay);
    let mut watching = taken(Intent::WebPair, &opener, watching, &mut look).await;
    web_box::settle_and_copy(&mut watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;
    assert!(
        said(&watching).ends_with("read it off the box"),
        "{}",
        said(&watching)
    );
}

/// A refused code is not copied, though an older code still shows.
#[tokio::test]
async fn a_refused_pairing_code_copies_nothing() {
    use crate::tui::clipboard::Tried;
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(WebStatus::Off), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, sent, tools) = look_copying(&["DISPLAY"], Tried::Copied("xclip"));
    let mut watching = taken(Intent::WebPair, &opener, watching, &mut look).await;
    web_box::settle_and_copy(&mut watching, &mut look).await;
    assert!(watching.web_copy.is_none());
    assert!(sent.lock().unwrap().is_empty());
    assert!(tools.lock().unwrap().is_empty());
}

/// A tool still running after its wait is handed the text and the note does not say copied.
#[tokio::test]
async fn a_tool_still_running_is_worded_as_handed_to() {
    use crate::tui::clipboard::Tried;
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, _, _) = look_copying(&["DISPLAY"], Tried::Handed("xclip"));
    let mut watching = taken(Intent::CopyWebLink, &opener, watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;
    let said = said(&watching);
    assert_eq!(said, "link handed to xclip, which has not said it took it");
}

/// A copy made while another is in flight reaches the tool when that one ends, with its own
/// text, and the note said so at once; only the newest waits.
#[tokio::test]
async fn a_copy_made_while_one_is_in_flight_reaches_the_tool_after_it() {
    use crate::tui::clipboard::Tried;
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says_web(Some(network()), None);
    watching.app.overlays.conn.modal = Some(Modal::Web(web_box_with_codes()));
    let opener = opener_on_nothing(path);
    let (mut look, sent, tools) = look_copying(&["DISPLAY"], Tried::Copied("xclip"));
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = std::sync::Mutex::new(gate);
    let record = std::sync::Arc::clone(&tools);
    look.copier = std::sync::Arc::new(move |text| {
        let first = record.lock().unwrap().is_empty();
        record.lock().unwrap().push(text.to_string());
        if first {
            let _ = gate.lock().unwrap().recv();
        }
        Tried::Copied("xclip")
    });
    let watching = taken(Intent::CopyWebLink, &opener, watching, &mut look).await;
    let watching = taken_with_link(watching, "http://192.168.1.20:8738/#login=two");
    let watching = taken(Intent::CopyWebLink, &opener, watching, &mut look).await;
    let watching = taken_with_link(watching, "http://192.168.1.20:8738/#login=three");
    let mut watching = taken(Intent::CopyWebLink, &opener, watching, &mut look).await;
    assert_eq!(
        sent.lock().unwrap().len(),
        3,
        "every copy goes to the terminal at once"
    );
    let now = said(&watching);
    assert!(
        now.contains("the clipboard tool gets it when the last copy ends"),
        "{now}"
    );
    release.send(()).unwrap();
    web_box::settle_copy(&mut watching).await;
    assert!(
        watching.web_copy.is_some(),
        "the waiting copy did not start"
    );
    web_box::settle_copy(&mut watching).await;
    assert_eq!(
        *tools.lock().unwrap(),
        [LINK, "http://192.168.1.20:8738/#login=three"],
        "the first and the newest reach the tool, the one in between does not"
    );
    assert_eq!(said(&watching), "link copied with xclip");
    assert!(watching.web_copy.is_none() && watching.web_copy_next.is_none());
}

/// The box shows `link` instead of what it showed.
fn taken_with_link(mut watching: Watching, link: &str) -> Watching {
    if let Some(Modal::Web(web)) = watching.app.overlays.conn.modal.as_mut() {
        web.link.as_mut().unwrap().text = link.to_string();
    }
    watching
}
