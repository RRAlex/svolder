// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use svolder_proto::{FunctionId, Message, Value};

/// A scan row from a daemon that carried neither the product id nor the signal strength
/// still reads, as a row that has neither: a front end and a daemon of different builds
/// still understand each other's scans.
#[test]
fn a_scan_entry_without_the_product_id_or_the_signal_still_decodes() {
    let entry: ScanEntry = serde_json::from_str(
        r#"{"unit":"0102030405060708","product":null,"firmware":100,"passphrase":false}"#,
    )
    .expect("the older row decodes");
    assert_eq!(entry.unit, "0102030405060708");
    assert_eq!(entry.product_id, 0);
    assert_eq!(entry.rssi, None);
}

/// The words for a device a daemon has never found are its name and then the ending a front end
/// reads them by, so what the daemon says is what the front end recognises, in the same words as
/// the exit-codes page quotes.
#[test]
fn the_words_for_a_device_not_found_yet_end_as_the_front_ends_read_them() {
    let said = not_found_yet("amp");
    assert_eq!(said, "amp has not been found yet");
    assert_eq!(said.strip_suffix(NOT_FOUND_YET), Some("amp"));
}

/// Every state the bridge reports survives the trip to a client and back, in the words
/// the wire carries.
#[test]
fn the_mqtt_status_round_trips_through_a_version_reply() {
    for (mqtt, line) in [
        (MqttStatus::Off, r#"{"state":"off"}"#),
        (
            MqttStatus::Connecting {
                host: "broker".into(),
                port: 1883,
                retry_in_secs: None,
            },
            r#"{"state":"connecting","host":"broker","port":1883}"#,
        ),
        (
            MqttStatus::Connecting {
                host: "broker".into(),
                port: 1883,
                retry_in_secs: Some(4),
            },
            r#"{"state":"connecting","host":"broker","port":1883,"retry_in_secs":4}"#,
        ),
        (
            MqttStatus::Connected {
                host: "broker".into(),
                port: 1883,
                commands: true,
            },
            r#"{"state":"connected","host":"broker","port":1883,"commands":true}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&mqtt).unwrap(), line);
        let reply = Response::Version {
            version: "0.6.0".into(),
            mqtt: Some(mqtt),
            mcp: None,
            config: None,
            log: None,
            mcp_token: None,
            web: None,
            web_token: None,
        };
        let back: Response = serde_json::from_str(&serde_json::to_string(&reply).unwrap())
            .expect("the reply decodes");
        assert_eq!(back, reply);
    }
}

/// A client built before the bridge existed decodes a version reply that carries it, and
/// a daemon built before it answers one that this client reads as having none.
#[test]
fn the_mqtt_field_is_one_an_older_client_ignores_and_an_older_daemon_leaves_out() {
    /// The version reply as a client built before the bridge declares it.
    #[derive(Debug, Deserialize)]
    #[serde(tag = "result", rename_all = "snake_case")]
    enum OlderResponse {
        Version { version: String },
    }
    let newer = serde_json::to_string(&Response::Version {
        version: "0.6.0".into(),
        mqtt: Some(MqttStatus::Off),
        mcp: None,
        config: None,
        log: None,
        mcp_token: None,
        web: None,
        web_token: None,
    })
    .unwrap();
    assert!(newer.contains(r#""mqtt":{"state":"off"}"#), "{newer}");
    let OlderResponse::Version { version } =
        serde_json::from_str(&newer).expect("an older client decodes the newer reply");
    assert_eq!(version, "0.6.0");

    // And a state a newer daemon may report is one this client reads as unknown, with the
    // version beside it intact.
    let newer_state = r#"{"result":"version","version":"0.7.0","mqtt":{"state":"refused","host":"broker","port":1883}}"#;
    assert_eq!(
        serde_json::from_str::<Response>(newer_state).expect("the newer state decodes"),
        Response::Version {
            version: "0.7.0".into(),
            mqtt: Some(MqttStatus::Unknown),
            mcp: None,
            config: None,
            log: None,
            mcp_token: None,
            web: None,
            web_token: None,
        }
    );

    let older = r#"{"result":"version","version":"0.5.0"}"#;
    assert_eq!(
        serde_json::from_str::<Response>(older).expect("the older reply decodes"),
        Response::Version {
            version: "0.5.0".into(),
            mqtt: None,
            mcp: None,
            config: None,
            log: None,
            mcp_token: None,
            web: None,
            web_token: None,
        }
    );
}

/// The `config` path is additive too: written only when the daemon says it, read when present,
/// and an older reply without it decodes to none.
#[test]
fn the_config_path_is_additive() {
    let reply = Response::Version {
        version: "0.13.0".into(),
        mqtt: None,
        mcp: None,
        config: Some("/config/svolder/config.toml".into()),
        log: None,
        mcp_token: None,
        web: None,
        web_token: None,
    };
    let line = serde_json::to_string(&reply).unwrap();
    assert!(
        line.contains(r#""config":"/config/svolder/config.toml""#),
        "{line}"
    );
    assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), reply);
    let without = r#"{"result":"version","version":"0.12.0"}"#;
    assert_eq!(
        serde_json::from_str::<Response>(without).unwrap(),
        Response::Version {
            version: "0.12.0".into(),
            mqtt: None,
            mcp: None,
            config: None,
            log: None,
            mcp_token: None,
            web: None,
            web_token: None,
        }
    );
}

/// The MCP field is additive the same way: written only when there is one, read by a client
/// that knows it, ignored by one that does not, and an unknown state decodes as such.
#[test]
fn the_mcp_field_is_additive() {
    let listen: std::net::SocketAddr = "127.0.0.1:8737".parse().unwrap();
    for (mcp, line) in [
        (McpStatus::Off, r#"{"state":"off"}"#),
        (
            McpStatus::Listening {
                listen,
                auth: crate::config::McpAuth::Token,
                writes: false,
            },
            r#"{"state":"listening","listen":"127.0.0.1:8737","auth":"token","writes":false}"#,
        ),
        (
            McpStatus::Failed {
                listen,
                error: "address in use".into(),
            },
            r#"{"state":"failed","listen":"127.0.0.1:8737","error":"address in use"}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&mcp).unwrap(), line);
        let reply = Response::Version {
            version: "0.13.0".into(),
            mqtt: None,
            mcp: Some(mcp),
            config: None,
            log: None,
            mcp_token: None,
            web: None,
            web_token: None,
        };
        let back: Response = serde_json::from_str(&serde_json::to_string(&reply).unwrap())
            .expect("the reply decodes");
        assert_eq!(back, reply);
    }
    let without = serde_json::to_string(&Response::Version {
        version: "0.13.0".into(),
        mqtt: None,
        mcp: None,
        config: None,
        log: None,
        mcp_token: None,
        web: None,
        web_token: None,
    })
    .unwrap();
    assert!(!without.contains("mcp"), "{without}");
    let newer = r#"{"result":"version","version":"0.14.0","mcp":{"state":"oidc","issuer":"x"}}"#;
    assert_eq!(
        serde_json::from_str::<Response>(newer).expect("the newer state decodes"),
        Response::Version {
            version: "0.14.0".into(),
            mqtt: None,
            mcp: Some(McpStatus::Unknown),
            config: None,
            log: None,
            mcp_token: None,
            web: None,
            web_token: None,
        }
    );
}

/// The web field is additive the same way: written only when there is one, and an unknown state
/// decodes as such.
#[test]
fn the_web_field_is_additive() {
    let listen: std::net::SocketAddr = "127.0.0.1:8738".parse().unwrap();
    for (web, line) in [
        (
            WebStatus::Listening {
                listen,
                auth: crate::config::WebAuth::Token,
                writes: true,
                guest: crate::config::WebGuest::Read,
            },
            "{\"state\":\"listening\",\"listen\":\"127.0.0.1:8738\",\"auth\":\"token\",\
             \"writes\":true,\"guest\":\"read\"}",
        ),
        (
            WebStatus::Failed {
                listen,
                error: "address in use".into(),
            },
            r#"{"state":"failed","listen":"127.0.0.1:8738","error":"address in use"}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&web).unwrap(), line);
        let reply = Response::Version {
            version: "0.22.0".into(),
            mqtt: None,
            mcp: None,
            config: None,
            log: None,
            mcp_token: None,
            web: Some(web),
            web_token: None,
        };
        let back: Response = serde_json::from_str(&serde_json::to_string(&reply).unwrap())
            .expect("the reply decodes");
        assert_eq!(back, reply);
    }
    // A daemon with nothing to say about the web UI leaves the key out, as an older one does.
    let silent = Response::Version {
        version: "0.22.0".into(),
        mqtt: None,
        mcp: None,
        config: None,
        log: None,
        mcp_token: None,
        web: None,
        web_token: None,
    };
    let line = serde_json::to_string(&silent).unwrap();
    assert!(!line.contains("\"web"), "{line}");
    let newer = r#"{"result":"version","version":"0.23.0","web":{"state":"tunnelled","via":"x"}}"#;
    assert_eq!(
        serde_json::from_str::<Response>(newer).expect("the newer state decodes"),
        Response::Version {
            version: "0.23.0".into(),
            mqtt: None,
            mcp: None,
            config: None,
            log: None,
            mcp_token: None,
            web: Some(WebStatus::Unknown),
            web_token: None,
        }
    );
}

#[test]
fn request_round_trips_through_json() {
    let r = Request::Set {
        device: Some("bifrost".into()),
        key: "volume".into(),
        value: "100".into(),
        step: 1,
        no_wait: false,
    };
    let line = to_line(&RequestLine {
        id: 7,
        request: r.clone(),
    })
    .unwrap();
    assert!(line.ends_with('\n'));
    let back: RequestLine = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(back.id, 7);
    assert_eq!(back.request, r);
}

/// A subscribe line from before `host` existed asks for no host feed, one that asks for it says
/// so, and a plain one leaves the field out so a daemon from before sees the line it knew.
#[test]
fn a_subscribe_asks_for_the_host_only_when_it_says_so() {
    let old: RequestLine =
        serde_json::from_str(r#"{"id":1,"request":{"cmd":"subscribe","device":null}}"#).unwrap();
    assert_eq!(
        old.request,
        Request::Subscribe {
            device: None,
            host: false
        }
    );
    let asking: RequestLine = serde_json::from_str(
        r#"{"id":1,"request":{"cmd":"subscribe","device":"amp","host":true}}"#,
    )
    .unwrap();
    assert_eq!(
        asking.request,
        Request::Subscribe {
            device: Some("amp".into()),
            host: true
        }
    );
    let plain = to_line(&RequestLine {
        id: 1,
        request: Request::Subscribe {
            device: None,
            host: false,
        },
    })
    .unwrap();
    assert!(!plain.contains("host"), "{plain}");
}

/// A tone-control line written before `no_wait` existed still parses, with the field
/// defaulting to "wait for the echo": an older front end on the same machine keeps
/// working against a daemon that understands the field.
#[test]
fn eq_lines_without_no_wait_still_parse() {
    let set =
        r#"{"id":1,"request":{"cmd":"eq_set","device":null,"band":1,"param":"f","value":50.0}}"#;
    let back: RequestLine = serde_json::from_str(set).unwrap();
    assert_eq!(
        back.request,
        Request::EqSet {
            device: None,
            band: 1,
            param: "f".into(),
            value: 50.0,
            no_wait: false,
        }
    );
    let mode = r#"{"id":2,"request":{"cmd":"eq_mode","device":null,"mode":"eq"}}"#;
    let back: RequestLine = serde_json::from_str(mode).unwrap();
    assert_eq!(
        back.request,
        Request::EqMode {
            device: None,
            mode: "eq".into(),
            no_wait: false,
        }
    );
}

#[test]
fn reply_distinguishes_response_from_event() {
    let resp = Reply::Response {
        id: 1,
        response: Response::Ack,
    };
    let ev = Reply::Event {
        id: 1,
        event: Event::Error {
            message: "x".into(),
        },
    };
    let a: Reply = serde_json::from_str(&to_line(&resp).unwrap()).unwrap();
    let b: Reply = serde_json::from_str(&to_line(&ev).unwrap()).unwrap();
    assert!(matches!(a, Reply::Response { .. }));
    assert!(matches!(b, Reply::Event { .. }));
}

#[test]
fn a_status_event_names_the_device_it_came_from() {
    let reg = svolder_profiles::Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap();
    let m = Message::status(FunctionId::one(10), Value::U8(109));
    let reading = ValueObject::from_reading(&svolder_profiles::describe(profile, &m), &m);
    let event = Event::Status {
        device: "gungnir".into(),
        reading,
    };
    let line = to_line(&Reply::Event { id: 3, event }).unwrap();
    let back: Reply = serde_json::from_str(line.trim()).unwrap();
    let Reply::Event {
        event: Event::Status { device, reading },
        ..
    } = back
    else {
        panic!("expected a status event, got {back:?}");
    };
    assert_eq!(device, "gungnir");
    assert_eq!(reading.key.as_deref(), Some("volume"));
}

#[test]
fn value_object_carries_the_reading_fields() {
    let reg = svolder_profiles::Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap();
    let m = Message::status(FunctionId::one(10), Value::U8(109));
    let vo = ValueObject::from_reading(&svolder_profiles::describe(profile, &m), &m);
    assert_eq!(vo.id, "10");
    assert_eq!(vo.key.as_deref(), Some("volume"));
    assert_eq!(vo.label, "Volume");
    assert_eq!(vo.value_type, "u8");
    assert_eq!(vo.raw, serde_json::json!(109));
    assert!(vo.display.contains("dB"));
    assert_eq!(vo.value, Some(-6.021));
    assert_eq!(vo.unit.as_deref(), Some("dB"));
}

/// The daemon-wide verbs carry nothing but their name, so a front end writes them as a
/// `cmd` and nothing else.
#[test]
fn the_daemon_verbs_are_a_command_word_on_their_own() {
    for (line, request) in [
        (r#"{"cmd":"reload"}"#, Request::Reload),
        (r#"{"cmd":"version"}"#, Request::Version),
        (r#"{"cmd":"stop"}"#, Request::Stop),
    ] {
        assert_eq!(serde_json::to_string(&request).unwrap(), line);
        assert_eq!(serde_json::from_str::<Request>(line).unwrap(), request);
    }
}

/// One reply for every `Response` variant, named.
fn sample_response_replies() -> Vec<(&'static str, Reply)> {
    let reg = svolder_profiles::Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap();
    let m = Message::status(FunctionId::one(10), Value::U8(109));
    let vo = ValueObject::from_reading(&svolder_profiles::describe(profile, &m), &m);

    let device1 = DeviceInfo {
        signal: None,
        name: "device1".into(),
        unit: "12345678".into(),
        product_id: 2,
        product: Some("Gungnir 2".into()),
        firmware: 100,
        profile: "gungnir2-v2".into(),
        connected: true,
        stale: false,
        retry_in_secs: None,
        passphrase_note: None,
        refused: false,
        recalled: None,
    };

    let scan_entry = ScanEntry {
        unit: "12345678".into(),
        product_id: 2,
        product: Some("Gungnir 2".into()),
        firmware: 100,
        passphrase: false,
        rssi: Some(-50),
    };

    vec![
        (
            "Values",
            Reply::Response {
                id: 1,
                response: Response::Values {
                    device: "device1".into(),
                    values: vec![vo.clone()],
                },
            },
        ),
        (
            "Value",
            Reply::Response {
                id: 2,
                response: Response::Value(vo.clone()),
            },
        ),
        (
            "Devices",
            Reply::Response {
                id: 3,
                response: Response::Devices {
                    devices: vec![device1],
                },
            },
        ),
        (
            "Scan",
            Reply::Response {
                id: 4,
                response: Response::Scan {
                    entries: vec![scan_entry],
                },
            },
        ),
        (
            "Ack",
            Reply::Response {
                id: 5,
                response: Response::Ack,
            },
        ),
        (
            "Error",
            Reply::Response {
                id: 6,
                response: Response::Error {
                    code: 1,
                    message: "test error".into(),
                },
            },
        ),
        (
            "Reloaded",
            Reply::Response {
                id: 7,
                response: Response::Reloaded {
                    devices: vec!["device1".into()],
                },
            },
        ),
        (
            "Version",
            Reply::Response {
                id: 8,
                response: Response::Version {
                    version: "0.1.0".into(),
                    mqtt: Some(MqttStatus::Connected {
                        host: "192.168.1.10".into(),
                        port: 1883,
                        commands: false,
                    }),
                    mcp: None,
                    config: None,
                    log: None,
                    mcp_token: None,
                    web: None,
                    web_token: None,
                },
            },
        ),
        (
            "Stopping",
            Reply::Response {
                id: 9,
                response: Response::Stopping,
            },
        ),
        (
            "Identify",
            Reply::Response {
                id: 10,
                response: Response::Identify {
                    device: "device1".into(),
                    sent: true,
                },
            },
        ),
        (
            "Host",
            Reply::Response {
                id: 11,
                response: Response::Host(sample_host_view()),
            },
        ),
    ]
}

#[test]
fn every_response_variant_serializes_inside_a_reply() {
    // Every Response variant serializes and deserializes back to itself.
    let responses = sample_response_replies();

    for (name, reply) in responses {
        let line =
            to_line(&reply).unwrap_or_else(|e| panic!("Failed to serialize {}: {}", name, e));
        assert!(
            line.ends_with('\n'),
            "Line for {} should end with newline",
            name
        );
        let parsed: Reply = serde_json::from_str(line.trim())
            .unwrap_or_else(|e| panic!("Failed to parse {}: {}", name, e));
        assert_eq!(parsed, reply, "Round-trip failed for {}", name);
    }
}

/// A boolean reading's raw value is a JSON boolean, not the text of one.
#[test]
fn a_boolean_readings_raw_value_is_a_json_boolean() {
    let on = Message::status(FunctionId::one(5), Value::Bool(true));
    let off = Message::status(FunctionId::one(5), Value::Bool(false));
    assert_eq!(raw_json(&on), serde_json::json!(true));
    assert_eq!(raw_json(&off), serde_json::json!(false));
}

/// The history request and its reply are the JSON the protocol document shows, and the
/// request set an older client sends still decodes unchanged.
#[test]
fn the_history_request_and_reply_round_trip_as_the_spec_words_them() {
    let request = Request::History {
        device: Some("bifrost".into()),
        span_secs: 3600,
    };
    let line = serde_json::to_string(&request).unwrap();
    assert_eq!(
        line,
        r#"{"cmd":"history","device":"bifrost","span_secs":3600}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), request);

    let reply = Response::History {
        device: "bifrost".into(),
        now_ms: 1_700_000_000_000,
        transitions: vec![
            LinkChange {
                at_ms: 1_699_999_000_000,
                connected: true,
            },
            LinkChange {
                at_ms: 1_699_999_500_000,
                connected: false,
            },
        ],
    };
    let line = serde_json::to_string(&reply).unwrap();
    assert_eq!(
        line,
        r#"{"result":"history","device":"bifrost","now_ms":1700000000000,"transitions":[{"at_ms":1699999000000,"connected":true},{"at_ms":1699999500000,"connected":false}]}"#
    );
    assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), reply);

    // The older requests keep their words.
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"cmd":"state","device":null}"#).unwrap(),
        Request::State { device: None }
    );
}

/// The token description is its own additive field of the `version` reply: written only when
/// the daemon has one, absent from an older reply (which parses to none), carried apart from the
/// two statuses, and never the token.
#[test]
fn the_token_description_is_additive_and_carries_no_token() {
    use crate::socket::protocol::{TokenInfo, TokenKind};
    use crate::token::{Token, TokenSource};

    let fake = "fake-token-0123456789-abcdefghijklmnop-x3fQ";
    let token = Token::for_test(
        fake,
        TokenSource::Generated("/home/user/.local/state/svolder/mcp-token".into()),
    );
    let info = token.info();
    assert_eq!(
        info,
        TokenInfo {
            kind: TokenKind::State,
            path: Some("/home/user/.local/state/svolder/mcp-token".into()),
            tail: "3fQ".into(),
        }
    );
    let reply = Response::Version {
        version: "0.13.0".into(),
        mqtt: Some(MqttStatus::Off),
        mcp: Some(McpStatus::Off),
        config: None,
        log: None,
        mcp_token: Some(info.clone()),
        web: None,
        web_token: None,
    };
    let line = serde_json::to_string(&reply).unwrap();
    assert!(!line.contains(fake), "the reply carries the token");
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["mcp_token"]["kind"], "state");
    assert_eq!(value["mcp_token"]["tail"], "3fQ");
    assert!(!value["mqtt"].to_string().contains("3fQ"));
    assert!(!value["mcp"].to_string().contains("3fQ"));
    assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), reply);

    let env = serde_json::to_string(&TokenInfo {
        kind: TokenKind::Environment,
        path: None,
        tail: String::new(),
    })
    .unwrap();
    assert_eq!(env, r#"{"kind":"environment","tail":""}"#);

    let older = r#"{"result":"version","version":"0.13.0","mcp":{"state":"off"}}"#;
    let Response::Version { mcp_token, .. } = serde_json::from_str(older).unwrap() else {
        panic!("a version reply")
    };
    assert!(mcp_token.is_none());
    let without = serde_json::to_string(&Response::Version {
        version: "0.13.0".into(),
        mqtt: None,
        mcp: None,
        config: None,
        log: None,
        mcp_token: None,
        web: None,
        web_token: None,
    })
    .unwrap();
    assert!(!without.contains("mcp_token"), "{without}");
}

/// The `version` reply carries where the daemon logs, and a reply without it still decodes.
#[test]
fn version_reply_carries_the_log_and_decodes_without_it() {
    let reply = Response::Version {
        version: "0.13.2".into(),
        mqtt: None,
        mcp: None,
        config: None,
        log: Some("/home/user/.local/state/svolder/svolderd.log".into()),
        mcp_token: None,
        web: None,
        web_token: None,
    };
    let line = serde_json::to_string(&reply).unwrap();
    assert!(
        line.contains(r#""log":"/home/user/.local/state/svolder/svolderd.log""#),
        "{line}"
    );
    assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), reply);

    let none = serde_json::to_string(&Response::Version {
        version: "0.13.2".into(),
        mqtt: None,
        mcp: None,
        config: None,
        log: None,
        mcp_token: None,
        web: None,
        web_token: None,
    })
    .unwrap();
    assert!(!none.contains("log"), "{none}");

    let older = r#"{"result":"version","version":"0.13.1","config":"/c.toml"}"#;
    let Response::Version { log, config, .. } = serde_json::from_str(older).unwrap() else {
        panic!("a version reply");
    };
    assert_eq!((log, config.as_deref()), (None, Some("/c.toml")));
}

/// A device row from a daemon that does not judge passphrases carries no note and still reads,
/// as a row with none, and a row with none says nothing of it on the wire: a front end and a
/// daemon of different builds still understand each other's device lists.
#[test]
fn a_device_row_without_a_passphrase_note_still_decodes_and_says_none() {
    let older = r#"{"name":"amp","unit":"0102030405060708","product_id":2,"product":null,"firmware":3,"profile":"gungnir2-v2","connected":true,"stale":false}"#;
    let info: DeviceInfo = serde_json::from_str(older).expect("the older row decodes");
    assert_eq!(info.passphrase_note, None);
    assert!(
        !serde_json::to_string(&info)
            .unwrap()
            .contains("passphrase_note")
    );

    let newer = DeviceInfo {
        passphrase_note: Some(crate::PassphraseNote::Differs),
        refused: false,
        ..info
    };
    let wire = serde_json::to_string(&newer).unwrap();
    assert!(wire.contains(r#""passphrase_note":"differs""#), "{wire}");
    // An older client reads the same row as a map and ignores the key it does not know.
    let loose: serde_json::Value = serde_json::from_str(&wire).unwrap();
    assert_eq!(loose["name"], "amp");
}

/// One of every `Request` variant.
fn sample_requests() -> Vec<Request> {
    let device = || Some("bifrost".to_string());
    vec![
        Request::Devices,
        Request::State { device: device() },
        Request::Get {
            device: device(),
            key: "volume".into(),
        },
        Request::Set {
            device: device(),
            key: "volume".into(),
            value: "100".into(),
            step: 1,
            no_wait: false,
        },
        Request::Raw {
            device: device(),
            id: "10".into(),
            value_type: "u8".into(),
            value: "1".into(),
            no_wait: true,
        },
        Request::History {
            device: device(),
            span_secs: 60,
        },
        Request::EqShow { device: device() },
        Request::EqSet {
            device: device(),
            band: 1,
            param: "f".into(),
            value: 50.0,
            no_wait: false,
        },
        Request::EqMode {
            device: device(),
            mode: "eq".into(),
            no_wait: false,
        },
        Request::Passphrase {
            device: device(),
            digits: "1234".into(),
        },
        Request::Identify { device: device() },
        Request::Scan { window_ms: 5000 },
        Request::Subscribe {
            device: device(),
            host: false,
        },
        Request::Subscribe {
            device: None,
            host: true,
        },
        Request::Reload,
        Request::Version,
        Request::Stop,
        Request::Host,
        Request::HostPress {
            button: "pause".into(),
        },
        Request::HostFrames,
    ]
}

/// One of every `Event` variant (`Unknown` is what a decoder makes of a kind it does not know,
/// never something a daemon sends).
fn sample_events() -> Vec<Event> {
    let m = Message::status(FunctionId::one(10), Value::U8(109));
    let reg = svolder_profiles::Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap();
    let reading = ValueObject::from_reading(&svolder_profiles::describe(profile, &m), &m);
    vec![
        Event::Status {
            device: "fake".into(),
            reading,
        },
        Event::Connection {
            device: "fake".into(),
            connected: true,
        },
        Event::Fault {
            device: "fake".into(),
            label: "dc offset".into(),
        },
        Event::Error {
            message: "oops".into(),
        },
        Event::Host(sample_host_view()),
        Event::HostCapture(sample_capture_view()),
        Event::HostFrame(sample_frame_view()),
    ]
}

/// Add `"added_later": 1` to every JSON object, at any depth.
fn add_a_field_everywhere(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for inner in map.values_mut() {
                add_a_field_everywhere(inner);
            }
            map.insert("added_later".into(), 1.into());
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(add_a_field_everywhere),
        _ => {}
    }
}

/// `value` with a field a newer peer added to every object, decoded back.
fn decode_with_added_fields<T: Serialize + serde::de::DeserializeOwned>(original: &T) -> T {
    let mut json = serde_json::to_value(original).unwrap();
    add_a_field_everywhere(&mut json);
    serde_json::from_value(json).expect("a message with a field it does not know decodes")
}

/// A peer ignores a field it does not know, on every message: no type takes
/// `deny_unknown_fields`.
#[test]
fn a_field_a_newer_peer_adds_is_ignored_on_every_message() {
    for request in sample_requests() {
        assert_eq!(decode_with_added_fields(&request), request);
    }
    for (name, reply) in sample_response_replies() {
        assert_eq!(decode_with_added_fields(&reply), reply, "{name}");
    }
    for (id, event) in sample_events().into_iter().enumerate() {
        let reply = Reply::Event {
            id: id as u64,
            event,
        };
        assert_eq!(decode_with_added_fields(&reply), reply);
    }
}

/// Lines as the first releases wrote them still decode: every field added since is optional
/// on the wire. A line goes here whenever a field is added to a message.
#[test]
fn the_oldest_shapes_still_decode() {
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"cmd":"eq_set","device":null,"band":0,"param":"q","value":1.0}"#
        )
        .unwrap(),
        Request::EqSet {
            device: None,
            band: 0,
            param: "q".into(),
            value: 1.0,
            no_wait: false,
        }
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"cmd":"eq_mode","device":null,"mode":"flat"}"#)
            .unwrap(),
        Request::EqMode {
            device: None,
            mode: "flat".into(),
            no_wait: false,
        }
    );
    assert_eq!(
        serde_json::from_str::<Response>(r#"{"result":"version","version":"0.1.0"}"#).unwrap(),
        Response::Version {
            version: "0.1.0".into(),
            mqtt: None,
            mcp: None,
            config: None,
            log: None,
            mcp_token: None,
            web: None,
            web_token: None,
        }
    );
    let device: DeviceInfo = serde_json::from_str(
        r#"{"name":"d","unit":"0102030405060708","product_id":2,"product":null,"firmware":100,"profile":"gungnir2-v2","connected":true,"stale":false}"#,
    )
    .unwrap();
    assert_eq!(device.retry_in_secs, None);
    assert_eq!(device.passphrase_note, None);
    assert!(!device.refused);
    let entry: ScanEntry = serde_json::from_str(
        r#"{"unit":"0102030405060708","product":null,"firmware":100,"passphrase":false}"#,
    )
    .unwrap();
    assert_eq!((entry.product_id, entry.rssi), (0, None));
}

/// A kind of response, event or message this build does not know decodes as `Unknown`.
#[test]
fn a_kind_a_newer_peer_adds_decodes_as_unknown() {
    assert_eq!(
        serde_json::from_str::<Response>(r#"{"result":"hologram","x":1}"#).unwrap(),
        Response::Unknown
    );
    assert_eq!(
        serde_json::from_str::<Event>(r#"{"event":"hologram","x":1}"#).unwrap(),
        Event::Unknown
    );
    assert_eq!(
        serde_json::from_str::<Reply>(r#"{"msg":"future","id":1}"#).unwrap(),
        Reply::Unknown
    );
    assert_eq!(
        serde_json::from_str::<Reply>(
            r#"{"msg":"response","id":1,"response":{"result":"hologram"}}"#
        )
        .unwrap(),
        Reply::Response {
            id: 1,
            response: Response::Unknown
        }
    );
}

/// A kind this build knows, with a field missing or of the wrong type, is a decode failure and
/// never `Unknown`: only a kind it has never heard of is passed over.
#[test]
fn a_known_kind_with_a_bad_field_is_an_error_not_unknown() {
    for line in [
        r#"{"event":"connection"}"#,
        r#"{"event":"connection","device":1,"connected":true}"#,
    ] {
        assert!(serde_json::from_str::<Event>(line).is_err(), "{line}");
    }
    for line in [
        r#"{"result":"error"}"#,
        r#"{"result":"error","code":"one","message":"x"}"#,
    ] {
        assert!(serde_json::from_str::<Response>(line).is_err(), "{line}");
    }
}

/// The web UI's line in `daemon status` and the daemon's log, in each state.
#[test]
fn the_web_status_line_says_where_and_how() {
    use crate::config::{WebAuth, WebGuest};
    let at = "127.0.0.1:8738".parse().unwrap();
    let listening = |auth, writes, guest| WebStatus::Listening {
        listen: at,
        auth,
        writes,
        guest,
    };
    assert_eq!(WebStatus::Off.line(), "web: off");
    assert_eq!(
        listening(WebAuth::Token, true, WebGuest::None).line(),
        "web: listening on 127.0.0.1:8738 (token, writes)"
    );
    assert_eq!(
        listening(WebAuth::None, false, WebGuest::None).line(),
        "web: listening on 127.0.0.1:8738 (no auth, read-only)"
    );
    assert_eq!(
        listening(WebAuth::Token, true, WebGuest::Read).line(),
        "web: listening on 127.0.0.1:8738 (token, writes, guest read)"
    );
    assert_eq!(
        WebStatus::Failed {
            listen: at,
            error: "Address in use".into()
        }
        .line(),
        "web: cannot listen on 127.0.0.1:8738: Address in use"
    );
    assert_eq!(WebStatus::Unknown.line(), "web: unknown");
}

/// A `version` reply carries the web UI's status and its token's description when it has them,
/// and an older daemon's reply without them still decodes.
#[test]
fn a_version_reply_carries_the_web_fields_and_decodes_without_them() {
    let old = r#"{"result":"version","version":"0.21.1"}"#;
    let Response::Version { web, web_token, .. } = serde_json::from_str(old).unwrap() else {
        panic!("not a version reply");
    };
    assert_eq!((web, web_token), (None, None));
    let new = Response::Version {
        version: "0.22.0".into(),
        mqtt: None,
        mcp: None,
        config: None,
        log: None,
        mcp_token: None,
        web: Some(WebStatus::Off),
        web_token: None,
    };
    let line = serde_json::to_string(&new).unwrap();
    assert!(line.contains(r#""web":{"state":"off"}"#), "{line}");
    assert!(!line.contains("web_token"), "{line}");
    assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), new);
}

/// A `readings` answer crosses the wire as it is.
#[test]
fn a_readings_answer_round_trips() {
    let answer = Response::Readings {
        device: "dac".into(),
        now_ms: 100_000,
        bin_ms: 1000,
        series: vec![SeriesOut {
            id: "10".into(),
            key: Some("volume".into()),
            aggregate: svolder_profiles::history::Aggregate::Max,
            points: vec![(97_000, 50.0), (99_000, 60.0)],
        }],
        recalls: vec![RecallOut {
            at_ms: 98_000,
            name: "Warm".into(),
        }],
        link: vec![LinkChange {
            at_ms: 1_000,
            connected: true,
        }],
    };
    let line = serde_json::to_string(&answer).unwrap();
    assert!(line.contains(r#""aggregate":"max""#), "{line}");
    assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), answer);
}

/// A `readings` request crosses the wire as it is.
#[test]
fn a_readings_request_round_trips() {
    let ask = Request::Readings {
        device: Some("dac".into()),
        span_secs: 300,
    };
    let line = serde_json::to_string(&ask).unwrap();
    assert!(line.contains(r#""cmd":"readings""#), "{line}");
    assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), ask);
}

/// The memory's two events and the device row's memory cross the wire; an older peer's row
/// without it still decodes.
#[test]
fn the_recall_memory_and_the_climb_cross_the_wire() {
    for event in [
        Event::Recalled {
            device: "dac".into(),
            name: "Warm".into(),
        },
        Event::Climb {
            device: "dac".into(),
            key: "volume".into(),
            target: Some(127),
        },
        Event::Climb {
            device: "dac".into(),
            key: "volume".into(),
            target: None,
        },
    ] {
        let line = serde_json::to_string(&event).unwrap();
        assert_eq!(
            serde_json::from_str::<Event>(&line).unwrap(),
            event,
            "{line}"
        );
    }
    let old_row = r#"{"name":"dac","unit":"0102030405060708","product_id":2,"product":null,"firmware":3,"profile":"gungnir2-v2","connected":true,"stale":false}"#;
    let row: DeviceInfo = serde_json::from_str(old_row).unwrap();
    assert_eq!(row.recalled, None);
    assert!(!serde_json::to_string(&row).unwrap().contains("recalled"));
}

/// The memory's request crosses the wire.
#[test]
fn a_recalled_request_round_trips() {
    let ask = Request::Recalled {
        device: None,
        name: "Warm".into(),
    };
    let line = serde_json::to_string(&ask).unwrap();
    assert!(line.contains(r#""cmd":"recalled""#), "{line}");
    assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), ask);
}

/// The link check and its answer cross the wire, naming the code and the verdict.
#[test]
fn the_link_open_request_and_its_answer_round_trip() {
    let ask = Request::WebLinkOpen {
        code: "AAAAAAAAAAAAAAAAAAAAAA".into(),
    };
    let line = serde_json::to_string(&ask).unwrap();
    assert!(line.contains(r#""cmd":"web_link_open""#), "{line}");
    assert!(
        line.contains(r#""code":"AAAAAAAAAAAAAAAAAAAAAA""#),
        "{line}"
    );
    assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), ask);
    let answer = Response::WebLinkOpen { open: false };
    let line = serde_json::to_string(&answer).unwrap();
    assert!(line.contains(r#""result":"web_link_open""#), "{line}");
    assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), answer);
}

/// The two code requests and their answers cross the wire.
#[test]
fn the_web_code_requests_round_trip() {
    for ask in [Request::WebPair, Request::WebLink] {
        let line = serde_json::to_string(&ask).unwrap();
        assert_eq!(
            serde_json::from_str::<Request>(&line).unwrap(),
            ask,
            "{line}"
        );
    }
    assert!(
        serde_json::to_string(&Request::WebPair)
            .unwrap()
            .contains(r#""cmd":"web_pair""#)
    );
    for answer in [
        Response::WebPair {
            code: "482913".into(),
            expires_in_secs: 120,
        },
        Response::WebLink {
            code: "AAAAAAAAAAAAAAAAAAAAAA".into(),
            expires_in_secs: 120,
        },
    ] {
        let line = serde_json::to_string(&answer).unwrap();
        assert_eq!(
            serde_json::from_str::<Response>(&line).unwrap(),
            answer,
            "{line}"
        );
    }
}

#[test]
fn identify_round_trips() {
    let request = Request::Identify {
        device: Some("bifrost".into()),
    };
    let wire = serde_json::to_string(&request).unwrap();
    assert_eq!(wire, r#"{"cmd":"identify","device":"bifrost"}"#);
    assert_eq!(serde_json::from_str::<Request>(&wire).unwrap(), request);

    for sent in [true, false] {
        let answer = Response::Identify {
            device: "bifrost".into(),
            sent,
        };
        let wire = serde_json::to_string(&answer).unwrap();
        assert_eq!(
            wire,
            format!(r#"{{"result":"identify","device":"bifrost","sent":{sent}}}"#)
        );
        assert_eq!(serde_json::from_str::<Response>(&wire).unwrap(), answer);
    }
}

/// A device row and a reading from a daemon that keeps neither a signal nor a stamp decode as
/// having none; one that has them carries them under their own names.
#[test]
fn old_device_info_and_value_object_decode() {
    let row = r#"{"name":"amp","unit":"0102030405060708","product_id":2,"product":null,"firmware":3,"profile":"gungnir2-v2","connected":true,"stale":false}"#;
    let info: DeviceInfo = serde_json::from_str(row).expect("the older row decodes");
    assert_eq!(info.signal, None);
    let reading = r#"{"id":"10","key":"volume","label":"Volume","value_type":"u8","raw":109,"display":"109","value":-6.0,"unit":"dB"}"#;
    let vo: ValueObject = serde_json::from_str(reading).expect("the older reading decodes");
    assert_eq!(vo.heard_at_ms, None);

    let signal = crate::Signal {
        dbm: -62,
        source: crate::SignalSource::Advert,
        heard_at_ms: 1_700_000_000_000,
    };
    let newer = DeviceInfo {
        signal: Some(signal),
        ..info
    };
    let wire = serde_json::to_string(&newer).unwrap();
    assert!(
        wire.contains(r#""signal":{"dbm":-62,"source":"advert","heard_at_ms":1700000000000}"#),
        "{wire}"
    );
    assert_eq!(serde_json::from_str::<DeviceInfo>(&wire).unwrap(), newer);
    let stamped = ValueObject {
        heard_at_ms: Some(1_700_000_000_000),
        ..vo
    };
    let wire = serde_json::to_string(&stamped).unwrap();
    assert!(wire.contains(r#""heard_at_ms":1700000000000"#), "{wire}");
    assert_eq!(serde_json::from_str::<ValueObject>(&wire).unwrap(), stamped);
}

/// Absent, the signal and the stamp are left off the wire, so an older peer sees the row it knows.
#[test]
fn new_fields_are_skipped_when_absent() {
    let row = r#"{"name":"amp","unit":"0102030405060708","product_id":2,"product":null,"firmware":3,"profile":"gungnir2-v2","connected":true,"stale":false}"#;
    let info: DeviceInfo = serde_json::from_str(row).unwrap();
    assert!(!serde_json::to_string(&info).unwrap().contains("signal"));
    let reading = r#"{"id":"10","key":null,"label":"?","value_type":"u8","raw":1,"display":"1","value":null,"unit":null}"#;
    let vo: ValueObject = serde_json::from_str(reading).unwrap();
    assert!(!serde_json::to_string(&vo).unwrap().contains("heard_at_ms"));
}

/// A host view with a player and a route, as the daemon sends one.
fn sample_host_view() -> crate::host::HostView {
    crate::host::HostView {
        available: true,
        playing: Some(crate::host::PlayingView {
            title: "Song".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            player: "Spotify".into(),
            status: "playing".into(),
            length_us: Some(180_000_000),
            progress: Some(svolder_host::Progress {
                position_us: 1_000_000,
                rate_milli: 1000,
                read_at_ms: 5,
            }),
            cover: Some("0123456789abcdef".into()),
        }),
        route: Some(svolder_host::RouteFacts::default()),
        now_ms: 1_700_000_000_000,
    }
}

/// The host request, its press and the view in an answer and in an event each go over the wire
/// and come back as they went, the button as the plain word the CLI and the pages send.
#[test]
fn host_request_and_event_round_trip() {
    for (line, request) in [
        (r#"{"cmd":"host"}"#, Request::Host),
        (
            r#"{"cmd":"host_press","button":"next"}"#,
            Request::HostPress {
                button: "next".into(),
            },
        ),
    ] {
        assert_eq!(serde_json::to_string(&request).unwrap(), line);
        assert_eq!(serde_json::from_str::<Request>(line).unwrap(), request);
    }
    // A button this build does not know is the daemon's answer to give, not a decode failure.
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"cmd":"host_press","button":"rewind"}"#).unwrap(),
        Request::HostPress {
            button: "rewind".into()
        }
    );

    let view = sample_host_view();
    let answer = Reply::Response {
        id: 3,
        response: Response::Host(view.clone()),
    };
    let line = to_line(&answer).unwrap();
    assert_eq!(serde_json::from_str::<Reply>(line.trim()).unwrap(), answer);
    let event = Reply::Event {
        id: 4,
        event: Event::Host(view),
    };
    let line = to_line(&event).unwrap();
    assert_eq!(serde_json::from_str::<Reply>(line.trim()).unwrap(), event);
    assert!(line.contains(r#""event":"host""#), "{line}");
}

/// A client from before the host event decodes it as an event it does not know: its own enum
/// has no such kind, so the line is passed over, not fatal.
#[test]
fn an_old_client_decodes_event_host_as_unknown() {
    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(tag = "event", rename_all = "snake_case")]
    enum OldEvent {
        Error {
            #[allow(dead_code)]
            message: String,
        },
        #[serde(other)]
        Unknown,
    }
    let line = serde_json::to_string(&Event::Host(sample_host_view())).unwrap();
    assert_eq!(
        serde_json::from_str::<OldEvent>(&line).unwrap(),
        OldEvent::Unknown
    );
    let line = serde_json::to_string(&Response::Host(sample_host_view())).unwrap();
    assert!(line.starts_with(r#"{"result":"host""#), "{line}");
}

/// A frame as the daemon sends one: 240 bands a channel, the levels, a beat and the tempo.
fn sample_frame_view() -> crate::host::FrameView {
    crate::host::FrameView {
        t: 1_700_000_000_123,
        left: (0..240).map(|band| band as u8).collect(),
        right: (0..240).map(|band| 255 - band as u8).collect(),
        levels: [10, 20, 30, 40],
        beat: true,
        bpm: Some(120),
        phase: 77,
    }
}

/// The capture's state as the daemon sends it, running on a named sink.
fn sample_capture_view() -> crate::host::CaptureView {
    crate::host::CaptureView {
        state: crate::host::CaptureKind::Running,
        sink: Some("Schiit Gungnir 2".into()),
        why: None,
    }
}

/// The frames request and its two events go over the wire as the words the TUI and the docs
/// name, and come back as they went; the optional fields leave the line when they are empty.
#[test]
fn host_frames_request_and_events_round_trip() {
    assert_eq!(
        serde_json::to_string(&Request::HostFrames).unwrap(),
        r#"{"cmd":"host_frames"}"#
    );
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"cmd":"host_frames"}"#).unwrap(),
        Request::HostFrames
    );
    let capture = Event::HostCapture(sample_capture_view());
    let line = serde_json::to_string(&capture).unwrap();
    assert_eq!(
        line,
        r#"{"event":"host_capture","state":"running","sink":"Schiit Gungnir 2"}"#
    );
    assert_eq!(serde_json::from_str::<Event>(&line).unwrap(), capture);
    let gone = Event::HostCapture(crate::host::CaptureView {
        state: crate::host::CaptureKind::Unavailable,
        sink: None,
        why: Some("pw-record could not be started".into()),
    });
    let line = serde_json::to_string(&gone).unwrap();
    assert_eq!(
        line,
        r#"{"event":"host_capture","state":"unavailable","why":"pw-record could not be started"}"#
    );
    assert_eq!(serde_json::from_str::<Event>(&line).unwrap(), gone);
    let frame = Event::HostFrame(sample_frame_view());
    let line = to_line(&Reply::Event {
        id: 9,
        event: frame.clone(),
    })
    .unwrap();
    assert!(line.contains(r#""event":"host_frame""#), "{line}");
    assert!(line.len() < 4096, "a frame is {} bytes of text", line.len());
    assert_eq!(
        serde_json::from_str::<Reply>(line.trim()).unwrap(),
        Reply::Event {
            id: 9,
            event: frame
        }
    );
}

/// A client from before the frames events passes over them as events it does not know.
#[test]
fn an_old_client_decodes_the_frames_events_as_unknown() {
    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(tag = "event", rename_all = "snake_case")]
    enum OldEvent {
        Error {
            #[allow(dead_code)]
            message: String,
        },
        #[serde(other)]
        Unknown,
    }
    for event in [
        Event::HostCapture(sample_capture_view()),
        Event::HostFrame(sample_frame_view()),
    ] {
        let line = serde_json::to_string(&event).unwrap();
        assert_eq!(
            serde_json::from_str::<OldEvent>(&line).unwrap(),
            OldEvent::Unknown
        );
    }
}
