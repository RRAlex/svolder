// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The keys of the Settings box's Connect category: what each row asks for, the line a host, a
//! port or an address is typed into, and the question before the MCP token is replaced.
//!
//! Nothing here writes or reads a stored secret. A row asks for one [`Intent`], which the loop
//! carries out against the configuration file, the token store and the terminal; the token's text
//! never passes through the app except while the loop has put it there to be revealed. The one
//! secret that goes the other way is the broker password, typed on an empty line (masked unless
//! Tab shows it) and handed on inside the one intent that writes it, where it reads `***` in every
//! `Debug`; the app never holds what is already set.

use crossterm::event::KeyCode;
use svolder_ipc::token::TokenSource;

use super::{App, ConnChange, Intent, SettingsBox};
use crate::tui::boxes::connections::{AdapterList, Field, Line, Modal, TokenView};
use crate::tui::boxes::web_box::{self, WebBox, WebRow};
use crate::tui::screens::settings::{self, Setting};

impl Line {
    /// A line holding `text`, the cursor at its end.
    fn new(field: Field, text: String) -> Line {
        Line {
            field,
            caret: text.chars().count(),
            text,
        }
    }
}

impl App {
    /// What `Enter`, `Space` or an arrow asks of a Connect row: a switch turns, a text row opens
    /// its edit line, and a row that only reports does nothing.
    pub(super) fn connection_step(&mut self, setting: Setting) -> Vec<Intent> {
        // The Web UI box shows the state and makes codes whatever file the daemon reads; its two
        // writes are refused as every row's are.
        if setting == Setting::WebUi {
            self.overlays.conn.modal = Some(Modal::Web(WebBox::default()));
            let listening = matches!(
                self.overlays.conn.web_status,
                Some(svolder_ipc::WebStatus::Listening { .. })
            );
            return match listening && self.overlays.conn.daemon_up == Some(true) {
                true => vec![Intent::WebLink],
                false => Vec::new(),
            };
        }
        if let Some(words) = self.overlays.conn.read_only() {
            self.warn(words);
            return Vec::new();
        }
        if setting == Setting::MqttEnabled
            && !self.overlays.conn.mqtt.enabled
            && self.overlays.conn.mqtt.host.is_none()
        {
            // the switch cannot go on without a broker: say so and show where it is set
            self.warn("set MQTT broker host first");
            self.select_setting(Setting::MqttHost);
            return Vec::new();
        }
        let conn = &self.overlays.conn;
        let change = match setting {
            Setting::McpEnabled => ConnChange::McpEnabled(!conn.mcp.enabled),
            Setting::McpWrites => ConnChange::McpWrites(!conn.mcp.writes),
            Setting::MqttEnabled => ConnChange::MqttEnabled(!conn.mqtt.enabled),
            Setting::MqttCommands => ConnChange::MqttCommands(!conn.mqtt.commands),
            Setting::McpListen => return self.edit(Field::Listen, conn.mcp.listen.to_string()),
            Setting::MqttHost => {
                return self.edit(Field::Host, conn.mqtt.host.clone().unwrap_or_default());
            }
            Setting::MqttPort => return self.edit(Field::Port, conn.mqtt.port.to_string()),
            // Only `v` and Enter (see `settings_box_key`) open the token box.
            Setting::McpToken => {
                self.notify("v or Enter shows the token, c copies it, R replaces it");
                return Vec::new();
            }
            Setting::MqttUsername => {
                return self.edit(
                    Field::Username,
                    conn.mqtt.username.clone().unwrap_or_default(),
                );
            }
            // The line starts empty whatever is set: a password is never read back.
            Setting::MqttPassword => return self.edit(Field::Password, String::new()),
            _ => return Vec::new(),
        };
        vec![Intent::Connection(change)]
    }

    /// Open the Bluetooth adapter list and ask for the scan that fills it.
    pub(super) fn open_adapters(&mut self) -> Vec<Intent> {
        if let Some(words) = self.overlays.conn.read_only() {
            self.warn(words);
            return Vec::new();
        }
        self.overlays.conn.modal = Some(Modal::Adapters(AdapterList::default()));
        vec![Intent::ListAdapters]
    }

    /// A key in the adapter list: ↑↓ move, Enter saves the adapter under the cursor, Esc or `q`
    /// closes the list. Every other key is swallowed. Enter while the scan still runs, or after
    /// it failed, does nothing.
    fn adapters_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let Some(Modal::Adapters(list)) = self.overlays.conn.modal.as_mut() else {
            return Vec::new();
        };
        let last = list
            .rows
            .as_ref()
            .map_or(0, |rows| rows.len().saturating_sub(1));
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.overlays.conn.modal = None,
            KeyCode::Up => list.at = list.at.saturating_sub(1),
            KeyCode::Down => list.at = (list.at + 1).min(last),
            KeyCode::Enter => {
                if let Some((address, name)) = list.pick() {
                    self.overlays.conn.modal = None;
                    return vec![Intent::PickAdapter { address, name }];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Put the Settings cursor on `setting`, in whichever category lists it.
    fn select_setting(&mut self, setting: Setting) {
        let found = settings::CATEGORIES
            .iter()
            .enumerate()
            .find_map(|(category, (_, rows))| {
                rows.iter()
                    .position(|row| *row == setting)
                    .map(|row| (category, row))
            });
        if let Some((category, row)) = found {
            self.overlays.settings = Some(SettingsBox { category, row });
        }
    }

    /// Open the edit line on `field`, holding `text`.
    fn edit(&mut self, field: Field, text: String) -> Vec<Intent> {
        self.overlays.conn.password_shown = false;
        self.overlays.conn.password_clear_armed = false;
        self.overlays.conn.modal = Some(Modal::Editing(Line::new(field, text)));
        Vec::new()
    }

    /// What `v`, `c` and `R` ask of the token row: to open the token box, to copy the token, or
    /// to ask before replacing it. Replacing is refused, naming the source, for a token svolder
    /// does not own.
    pub(super) fn token_key(&mut self, key: char) -> Vec<Intent> {
        if let Some(words) = self.overlays.conn.token_refusal() {
            self.warn(words);
            return Vec::new();
        }
        let TokenView::Present { source, .. } = &self.overlays.conn.token else {
            let words = match &self.overlays.conn.token {
                TokenView::Missing => "no token yet: the daemon makes one when MCP first starts",
                TokenView::Unreadable(_) => "the token cannot be read; see the right column",
                _ => "the token is not read yet",
            };
            self.warn(words);
            return Vec::new();
        };
        match key {
            'v' => {
                self.overlays.conn.modal = Some(Modal::Token);
                vec![Intent::RevealToken]
            }
            'c' => vec![Intent::CopyToken],
            _ => match source {
                TokenSource::Generated(_) => {
                    self.overlays.conn.modal = Some(Modal::Rotating);
                    Vec::new()
                }
                other => {
                    let words = format!(
                        "the token comes from {}, which svolder does not own: rotate it there",
                        other.describe()
                    );
                    self.warn(words);
                    Vec::new()
                }
            },
        }
    }

    /// A key while the category has a box open: the edit line's typing, or the answer to the
    /// question. Every other key is swallowed, as the preset list's box swallows them.
    pub(super) fn connection_modal_key(&mut self, code: KeyCode) -> Vec<Intent> {
        match self.overlays.conn.modal.clone() {
            Some(Modal::Editing(_)) => self.type_line(code),
            Some(Modal::Rotating) => {
                match code {
                    KeyCode::Char('y' | 'Y') => {
                        self.overlays.conn.modal = None;
                        // The daemon may have answered while the question was up.
                        if let Some(words) = self.overlays.conn.token_refusal() {
                            self.warn(words);
                            return Vec::new();
                        }
                        return vec![Intent::RotateToken];
                    }
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('n' | 'N' | 'q') => {
                        self.overlays.conn.modal = None;
                    }
                    _ => {}
                }
                Vec::new()
            }
            Some(Modal::Web(_)) => self.web_box_key(code),
            Some(Modal::Adapters(_)) => self.adapters_key(code),
            Some(Modal::Token) => match code {
                KeyCode::Char('c') => vec![Intent::CopyToken],
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | 'v') => {
                    self.overlays.conn.hide_token();
                    Vec::new()
                }
                _ => Vec::new(),
            },
            None => Vec::new(),
        }
    }

    /// A key in the Web UI box: ↑↓ move between its rows; Enter or ←→ on Web UI turns it on or
    /// off, ←→ on Reach moves the listen address, Enter or ←→ on Guest view (and `g` on any row)
    /// turns the guest view; `l` a fresh link, `c` copies it, `p` a pairing
    /// code (Enter on the Link and Pairing code rows are `l` and `p`), `r` asks before the token
    /// is replaced; Esc or `q` closes the box and forgets its codes.
    /// Every other key is swallowed.
    fn web_box_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let Some(Modal::Web(web)) = self.overlays.conn.modal.clone() else {
            return Vec::new();
        };
        if web.rotating {
            return self.web_rotating_key(code);
        }
        // Enter on a code's row asks for that code, as the row's own key does.
        let code = match (code, web.row) {
            (KeyCode::Enter, WebRow::Link) => KeyCode::Char('l'),
            (KeyCode::Enter, WebRow::Pairing) => KeyCode::Char('p'),
            (code, _) => code,
        };
        let stops = WebRow::STOPS;
        let at = stops.iter().position(|r| *r == web.row).unwrap_or(0);
        let conn = &self.overlays.conn;
        let daemon = conn.daemon_up == Some(true);
        let change = match (code, web.row) {
            (KeyCode::Esc | KeyCode::Char('q'), _) => {
                self.overlays.conn.modal = None;
                return Vec::new();
            }
            (KeyCode::Up, _) => return self.web_row(stops[at.saturating_sub(1)]),
            (KeyCode::Down, _) => return self.web_row(stops[(at + 1).min(stops.len() - 1)]),
            (KeyCode::Enter | KeyCode::Left | KeyCode::Right, WebRow::WebUi) => {
                ConnChange::WebEnabled(!conn.web.enabled)
            }
            (KeyCode::Left | KeyCode::Right, WebRow::Reach) => {
                ConnChange::WebListen(web_box::stepped_listen(conn.web.listen))
            }
            (KeyCode::Enter | KeyCode::Left | KeyCode::Right, WebRow::Guest)
            | (KeyCode::Char('g'), _) => {
                ConnChange::WebGuest(conn.web.guest != svolder_ipc::config::WebGuest::Read)
            }
            (KeyCode::Char('l' | 'p'), _) if !daemon => {
                self.warn("the daemon serves the web UI: start it to make a link or a code");
                return Vec::new();
            }
            (KeyCode::Char('l'), _) => return vec![Intent::WebLink],
            (KeyCode::Char('p'), _) => return vec![Intent::WebPair],
            (KeyCode::Char('c'), _) if web.link.is_some() => return vec![Intent::CopyWebLink],
            (KeyCode::Char('c'), _) => {
                self.warn("no link yet: l makes one");
                return Vec::new();
            }
            (KeyCode::Char('r'), _) => {
                if let Some(Modal::Web(web)) = self.overlays.conn.modal.as_mut() {
                    web.rotating = true;
                }
                return Vec::new();
            }
            _ => return Vec::new(),
        };
        if let Some(words) = self.overlays.conn.read_only() {
            self.warn(words);
            return Vec::new();
        }
        vec![Intent::Connection(change)]
    }

    /// Move the Web UI box's cursor to `row`.
    fn web_row(&mut self, row: WebRow) -> Vec<Intent> {
        if let Some(Modal::Web(web)) = self.overlays.conn.modal.as_mut() {
            web.row = row;
        }
        Vec::new()
    }

    /// The keys of the question before the web token is replaced: `y` and `Enter` replace it,
    /// unless the token is not svolder's to replace; `Esc`, `n` and `q` close the question.
    fn web_rotating_key(&mut self, code: KeyCode) -> Vec<Intent> {
        match code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                if let Some(Modal::Web(web)) = self.overlays.conn.modal.as_mut() {
                    web.rotating = false;
                }
                // The daemon may have answered while the question was up.
                if let Some(words) = self.overlays.conn.web_token_refusal() {
                    self.warn(words);
                    return Vec::new();
                }
                if let TokenView::Present { source, .. } = &self.overlays.conn.web_token
                    && !matches!(source, TokenSource::Generated(_))
                {
                    let words = format!(
                        "the web token comes from {}, which svolder does not own: rotate it there",
                        source.describe()
                    );
                    self.warn(words);
                    return Vec::new();
                }
                vec![Intent::RotateWebToken]
            }
            KeyCode::Esc | KeyCode::Char('n' | 'N' | 'q') => {
                if let Some(Modal::Web(web)) = self.overlays.conn.modal.as_mut() {
                    web.rotating = false;
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The edit line's keys: letters typed where the cursor is, the arrows, `Home` and `End`
    /// move it, `Backspace` takes the character before it, `Enter` saves and `Esc` cancels.
    fn type_line(&mut self, code: KeyCode) -> Vec<Intent> {
        if code == KeyCode::Enter {
            return self.saved_line();
        }
        let on_password = matches!(
            &self.overlays.conn.modal,
            Some(Modal::Editing(Line {
                field: Field::Password,
                ..
            }))
        );
        // Clearing a saved password is its own key, asked twice: Delete on the password line.
        // Any other key takes the first ask back.
        let armed = std::mem::take(&mut self.overlays.conn.password_clear_armed);
        if on_password && code == KeyCode::Delete {
            if !armed {
                self.overlays.conn.password_clear_armed = true;
                self.notify("Delete again clears the saved password");
                return Vec::new();
            }
            self.overlays.conn.modal = None;
            if !self.overlays.conn.mqtt.password_in_file {
                self.notify("no password is written in the config file to clear");
                return Vec::new();
            }
            return vec![Intent::Connection(ConnChange::MqttPassword(None))];
        }
        let Some(Modal::Editing(line)) = self.overlays.conn.modal.as_mut() else {
            return Vec::new();
        };
        let len = line.text.chars().count();
        match code {
            KeyCode::Tab if line.field == Field::Password => {
                self.overlays.conn.password_shown = !self.overlays.conn.password_shown;
            }
            KeyCode::Esc => self.overlays.conn.modal = None,
            KeyCode::Left => line.caret = line.caret.saturating_sub(1),
            KeyCode::Right => line.caret = (line.caret + 1).min(len),
            KeyCode::Home => line.caret = 0,
            KeyCode::End => line.caret = len,
            KeyCode::Backspace if line.caret > 0 => {
                let at = byte_at(&line.text, line.caret - 1);
                line.text.remove(at);
                line.caret -= 1;
            }
            KeyCode::Char(letter) if len < line.field.max() && !letter.is_control() => {
                let at = byte_at(&line.text, line.caret);
                line.text.insert(at, letter);
                line.caret += 1;
            }
            _ => {}
        }
        Vec::new()
    }

    /// Close the edit line on what was typed, or say what is wrong with it and keep it up.
    fn saved_line(&mut self) -> Vec<Intent> {
        let Some(Modal::Editing(line)) = self.overlays.conn.modal.clone() else {
            return Vec::new();
        };
        let typed = line.text.trim();
        let change = match line.field {
            Field::Username => match typed {
                "" if self.overlays.conn.mqtt.username.is_none() => None,
                "" => {
                    if self.overlays.conn.mqtt.username_in_env {
                        self.notify(crate::commands::mqtt::USERNAME_ENV_NOTE);
                    }
                    Some(ConnChange::MqttUsername(None))
                }
                name if self.overlays.conn.mqtt.username.as_deref() == Some(name) => None,
                name => Some(ConnChange::MqttUsername(Some(name.to_string()))),
            },
            Field::Password => match line.text.as_str() {
                // An empty line keeps what is saved; Delete clears it.
                "" => {
                    self.overlays.conn.modal = None;
                    self.notify("password unchanged (Delete on the empty line clears it)");
                    return Vec::new();
                }
                // Spaces are the user's: a password is saved as typed.
                password => Some(ConnChange::MqttPassword(Some(password.to_string().into()))),
            },
            Field::Listen => match typed.parse::<std::net::SocketAddr>() {
                Ok(listen) if listen != self.overlays.conn.mcp.listen => {
                    Some(ConnChange::McpListen(listen))
                }
                Ok(_) => None,
                Err(_) => return self.refuse("an address is HOST:PORT, such as 127.0.0.1:8737"),
            },
            Field::Host => match typed {
                "" => return self.refuse("a host is a name or an address"),
                host if host.chars().any(char::is_whitespace) => {
                    return self.refuse("a host has no spaces in it");
                }
                host if self.overlays.conn.mqtt.host.as_deref() == Some(host) => None,
                host => Some(ConnChange::MqttHost(host.to_string())),
            },
            Field::Port => match typed.parse::<u16>() {
                Ok(port) if port != 0 && port != self.overlays.conn.mqtt.port => {
                    Some(ConnChange::MqttPort(port))
                }
                Ok(0) | Err(_) => return self.refuse("a port is a number from 1 to 65535"),
                Ok(_) => None,
            },
        };
        self.overlays.conn.modal = None;
        self.overlays.conn.password_shown = false;
        if change.is_some()
            && let Some(words) = self.overlays.conn.read_only()
        {
            self.warn(words);
            return Vec::new();
        }
        if let Some(ConnChange::MqttPassword(Some(_))) = &change {
            self.password_saved_note();
        }
        change.map(Intent::Connection).into_iter().collect()
    }

    /// What a password just saved still needs: a username to be sent with, and the variable that
    /// outranks the file where the daemon has it too.
    fn password_saved_note(&mut self) {
        let mqtt = &self.overlays.conn.mqtt;
        let words = match (&mqtt.username, mqtt.password_in_env) {
            (_, true) => "password saved; SVOLDER_MQTT_PASSWORD is set and wins over it",
            (None, false) => "password saved, but no username is set so it is not sent",
            (Some(_), false) => return,
        };
        self.notify(words);
    }

    /// Say why the line cannot be saved, with the line still up.
    fn refuse(&mut self, why: &str) -> Vec<Intent> {
        self.warn(why);
        Vec::new()
    }
}

/// Where character `at` of `text` starts, in bytes, and its end for a cursor past the last one.
fn byte_at(text: &str, at: usize) -> usize {
    text.char_indices()
        .nth(at)
        .map_or(text.len(), |(index, _)| index)
}
