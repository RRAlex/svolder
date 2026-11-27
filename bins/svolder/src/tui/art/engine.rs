// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Getting the cover of the track playing from the daemon to the screen, a step at a time, none
//! of them on the loop: the picture's bytes are fetched over the socket on a task, decoded on a
//! blocking thread, and sized and encoded for the terminal's protocol on another, each time for
//! the track the app names now. What a step brings for a track that has gone is dropped.
//!
//! [`Art::steer`] runs after every frame and starts whatever the next step is; [`Art::next`] is
//! the loop's arm that waits for a step to finish, and [`Art::fold`] takes what it brought into
//! the app, where the next frame draws it.

use std::collections::HashSet;
use std::io::Cursor;
use std::sync::Arc;

use image::{DynamicImage, ImageReader, Limits};
use ratatui::layout::{Rect, Size};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::{FilterType, Resize};
use svolder_host::cover::Cover;
use svolder_ipc::CoverArt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::backend::Backend;
use crate::tui::app::{App, Screen};
use crate::tui::screens::device::{art_box, art_slot};

/// The longest side a decoded picture is kept at, in pixels: more than the grown Status pane's
/// cell box holds on a terminal with big cells, little enough that a large cover costs no memory.
const KEPT_PX: u32 = 1024;

/// The widest and tallest picture that is decoded, in pixels, and the most memory it may take:
/// the daemon checks a cover's first bytes and its size in bytes, not what it decodes to.
const MAX_SIDE_PX: u32 = 8192;
const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;

/// A picture sized and encoded for a box of cells, ready to be drawn by the frame.
#[derive(Clone)]
pub struct Picture {
    /// The picture as the terminal's protocol draws it.
    pub(crate) protocol: Protocol,
    /// The cover it is of, as the daemon names it.
    pub(crate) hash: String,
    /// Whether a frame has written it: the kitty protocol sends the picture once, in the first
    /// write, so that write must not be one something drawn over the picture wipes.
    pub(crate) sent: Arc<std::sync::atomic::AtomicBool>,
}

impl Picture {
    /// The cells it covers.
    pub fn size(&self) -> Size {
        self.protocol.size()
    }
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("hash", &self.hash)
            .field("size", &self.size())
            .finish_non_exhaustive()
    }
}

/// A task that is ended when this is dropped.
struct Task(JoinHandle<()>);

impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// What a step brought.
pub(in crate::tui) enum Done {
    /// The bytes of the cover `hash`, or none.
    Fetched { hash: String, cover: Option<Cover> },
    /// The cover `hash` decoded, or none when it would not.
    Decoded {
        hash: String,
        image: Option<Arc<DynamicImage>>,
    },
    /// What draws for the terminal, made off the loop.
    Picker(Picker),
    /// The cover `hash` sized for `size` cells, or none when it could not be.
    Built {
        hash: String,
        size: Size,
        protocol: Option<Protocol>,
    },
}

/// The cover art of the loop's session.
pub(in crate::tui) struct Art {
    /// What draws for the terminal; `None` until the setting asks for art.
    picker: Option<Picker>,
    /// Whether the picker is being made.
    making: bool,
    /// Whether the daemon lists `host_cover`, once it has said.
    daemon_serves: Option<bool>,
    /// The fetch in flight, and the cover it is for.
    fetching: Option<(String, Task)>,
    /// The cover being decoded.
    decoding: Option<String>,
    /// The picture being sized, and the box it is for.
    building: Option<(String, Size)>,
    /// The covers the daemon had no bytes for or that would not decode, which are not asked for
    /// again.
    missing: HashSet<String>,
    /// The decoded cover.
    image: Option<(String, Arc<DynamicImage>)>,
    /// The cover and box of the picture the app holds.
    shown: Option<(String, Size)>,
    /// Where the picture the last frame drew was, by the cover it is of: what a sixel picture
    /// left on the terminal is cleared by the frame that draws something else there.
    drawn: Option<(String, Rect)>,
    /// Whether a box covered the picture when the last frame was asked about.
    covered: bool,
    tx: mpsc::UnboundedSender<Done>,
    rx: mpsc::UnboundedReceiver<Done>,
}

impl Default for Art {
    /// No art: a loop with no terminal of its own, and a test.
    fn default() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Art {
            picker: None,
            making: false,
            daemon_serves: None,
            fetching: None,
            decoding: None,
            building: None,
            missing: HashSet::new(),
            image: None,
            shown: None,
            drawn: None,
            covered: false,
            tx,
            rx,
        }
    }
}

impl Art {
    /// Art drawn with `picker`, the terminal as it was probed at the start, or none with `None`.
    pub(in crate::tui) fn with(picker: Option<Picker>) -> Art {
        Art {
            picker,
            ..Art::default()
        }
    }

    /// Say whether the daemon lists `host_cover`; a daemon that does not is never asked.
    pub(in crate::tui) fn daemon_serves(&mut self, serves: bool) {
        self.daemon_serves = Some(serves);
        if !serves {
            self.fetching = None;
        }
    }

    /// Whether the frame about to be drawn at `area` must begin with a repaint of every cell: a
    /// sixel picture the last frame drew and this one does not draw again as it was (the track
    /// changed, the box did, or the picture went) is kept by the terminal under cells that read
    /// the same. Called before each frame.
    pub(in crate::tui) fn clears_before(&mut self, app: &App, area: Rect) -> bool {
        // A box over the picture counts as the picture not being drawn: what the box leaves of
        // itself on the pixels goes when it closes, by the same repaint.
        let covered = app.host.cover.picture.is_some() && app.modal_open();
        let now = art_slot(area, app)
            .filter(|_| !covered)
            .zip(app.host.cover.picture.as_ref())
            .map(|(slot, picture)| (picture.hash.clone(), slot));
        let owed = (self.drawn.is_some() || self.covered)
            && self.drawn != now
            && self
                .picker
                .as_ref()
                .is_some_and(|picker| picker.protocol_type() == ProtocolType::Sixel);
        self.drawn = now;
        self.covered = covered;
        owed
    }

    /// Wait for the next step to finish, and say what it brought.
    pub(in crate::tui) async fn next(&mut self) -> Done {
        match self.rx.recv().await {
            Some(done) => done,
            // Never: this holds a sender itself.
            None => std::future::pending().await,
        }
    }

    /// Whether any step is under way, which the loop's own clock need not know but a test does.
    #[cfg(test)]
    pub(in crate::tui) fn busy(&self) -> bool {
        self.making || self.fetching.is_some() || self.decoding.is_some() || self.building.is_some()
    }

    /// Put the picture the app holds as `picture`, built for the box `size` (cropped to fill it):
    /// it is the box a frame asks for again, not the picture's own size.
    fn show(&mut self, app: &mut App, picture: Option<(Picture, Size)>) {
        self.shown = picture
            .as_ref()
            .map(|(picture, size)| (picture.hash.clone(), *size));
        app.host.cover.picture = picture.map(|(picture, _)| picture);
    }

    /// Give up everything held for a track: nothing is fetched, built or drawn.
    fn stop(&mut self, app: &mut App) {
        self.fetching = None;
        self.decoding = None;
        self.building = None;
        self.image = None;
        if app.host.cover.picture.is_some() {
            self.show(app, None);
        }
    }

    /// Start whatever the next step toward the picture of the track the app names is, for a
    /// frame `area` large. Called after every frame, and cheap when there is nothing to start.
    pub(in crate::tui) fn steer(&mut self, app: &mut App, backend: Option<&Backend>, area: Rect) {
        if app.tui.cover_art != CoverArt::Auto {
            self.stop(app);
            app.host.cover.font = None;
            return;
        }
        // Art turned on after the start has no probe to go by: half-blocks, until the next one,
        // made off the loop (inside tmux that runs a `tmux` subprocess). With the art off none
        // is made at all.
        let Some(picker) = self.picker.as_ref() else {
            if !self.making {
                self.making = true;
                let tx = self.tx.clone();
                tokio::task::spawn_blocking(move || {
                    let _ = tx.send(Done::Picker(Picker::halfblocks()));
                });
            }
            return;
        };
        let font = picker.font_size();
        app.host.cover.font = Some((font.width, font.height));
        let Some(hash) = app.host.cover.hash.clone() else {
            self.stop(app);
            return;
        };
        // The picture of another track goes at once, whatever the next one takes.
        if app
            .host
            .cover
            .picture
            .as_ref()
            .is_some_and(|p| p.hash != hash)
        {
            self.show(app, None);
        }
        if self.image.as_ref().is_some_and(|(held, _)| *held != hash) {
            self.image = None;
        }
        if self
            .fetching
            .as_ref()
            .is_some_and(|(held, _)| *held != hash)
        {
            self.fetching = None;
        }
        if self.decoding.as_ref().is_some_and(|held| *held != hash) {
            self.decoding = None;
        }
        if self.missing.contains(&hash) {
            app.host.cover.failed = Some(hash);
            return;
        }
        let Some((_, image)) = self.image.as_ref() else {
            self.fetch(&hash, backend);
            return;
        };
        // The picture is kept as it is while another screen shows; the device screen draws it.
        if app.screen != Screen::Device {
            return;
        }
        // The box the frame just drawn gave this cover at these cells, else one worked out now.
        let size = match app
            .drawn
            .drawn_art_box(area, &hash, (font.width, font.height))
        {
            Some(drawn) => drawn,
            None => art_box(area, app),
        };
        let Some(size) = size else {
            // No room for it at this size: the words keep the row.
            self.show(app, None);
            return;
        };
        if self.shown.as_ref() == Some(&(hash.clone(), size))
            || self.building.as_ref() == Some(&(hash.clone(), size))
        {
            return;
        }
        let image = image.as_ref().clone();
        let (picker, tx) = (
            self.picker.clone().unwrap_or_else(Picker::halfblocks),
            self.tx.clone(),
        );
        self.building = Some((hash.clone(), size));
        tokio::task::spawn_blocking(move || {
            let protocol = build(&picker, image, size);
            let _ = tx.send(Done::Built {
                hash,
                size,
                protocol,
            });
        });
    }

    /// Ask the daemon for the bytes of the cover `hash`, one ask at a time.
    fn fetch(&mut self, hash: &str, backend: Option<&Backend>) {
        if self.fetching.is_some() || self.decoding.is_some() || self.daemon_serves == Some(false) {
            return;
        }
        let Some(asked) = backend.and_then(|backend| backend.host_cover(hash)) else {
            return;
        };
        let (tx, hash) = (self.tx.clone(), hash.to_string());
        let named = hash.clone();
        let task = tokio::spawn(async move {
            let cover = asked.await;
            let _ = tx.send(Done::Fetched { hash, cover });
        });
        self.fetching = Some((named, Task(task)));
    }

    /// Take what a step brought: for the track the app names now it is kept and the next step
    /// follows; for another it is dropped.
    pub(in crate::tui) fn fold(&mut self, done: Done, app: &mut App) {
        let current = |hash: &str| app.host.cover.hash.as_deref() == Some(hash);
        match done {
            Done::Picker(picker) => {
                self.making = false;
                self.picker.get_or_insert(picker);
            }
            Done::Fetched { hash, cover } => {
                if self
                    .fetching
                    .as_ref()
                    .is_some_and(|(held, _)| *held == hash)
                {
                    self.fetching = None;
                }
                if !current(&hash) {
                    return;
                }
                let Some(cover) = cover else {
                    app.host.cover.failed = Some(hash.clone());
                    self.missing.insert(hash);
                    return;
                };
                self.decoding = Some(hash.clone());
                let tx = self.tx.clone();
                tokio::task::spawn_blocking(move || {
                    let image = decode(&cover.bytes).map(Arc::new);
                    let _ = tx.send(Done::Decoded { hash, image });
                });
            }
            Done::Decoded { hash, image } => {
                if self.decoding.as_ref() == Some(&hash) {
                    self.decoding = None;
                }
                if !current(&hash) {
                    return;
                }
                match image {
                    Some(image) => {
                        // The layout shapes the box by the cover before the first build.
                        app.host.cover.extent = Some((hash.clone(), image.width(), image.height()));
                        self.image = Some((hash, image));
                    }
                    None => {
                        app.host.cover.failed = Some(hash.clone());
                        self.missing.insert(hash);
                    }
                }
            }
            Done::Built {
                hash,
                size,
                protocol,
            } => {
                // Only the build asked for last counts: a picture for another size, made while the
                // window was being dragged, is dropped.
                if self.building != Some((hash.clone(), size)) {
                    return;
                }
                self.building = None;
                if !current(&hash) {
                    return;
                }
                match protocol {
                    Some(protocol) => self.show(
                        app,
                        Some((
                            Picture {
                                protocol,
                                hash,
                                sent: Arc::default(),
                            },
                            size,
                        )),
                    ),
                    None => {
                        app.host.cover.failed = Some(hash.clone());
                        self.missing.insert(hash);
                    }
                }
            }
        }
    }
}

/// The picture in `bytes`, at most [`KEPT_PX`] a side, or `None` for one that is not a JPEG, PNG
/// or WebP picture or that asks for more than [`MAX_DECODE_BYTES`] to decode.
pub(super) fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE_PX);
    limits.max_image_height = Some(MAX_SIDE_PX);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    Some(match image.width().max(image.height()) > KEPT_PX {
        true => image.thumbnail(KEPT_PX, KEPT_PX),
        false => image,
    })
}

/// `image` cropped from the centre to the shape of a box of `size` cells (it is never shrunk to
/// fit, so it fills the box) and encoded for what `picker` draws, or `None` when it could not be.
pub(in crate::tui) fn build(picker: &Picker, image: DynamicImage, size: Size) -> Option<Protocol> {
    let font = picker.font_size();
    let (box_w, box_h) = (
        u64::from(size.width) * u64::from(font.width),
        u64::from(size.height) * u64::from(font.height),
    );
    let (w, h) = (u64::from(image.width()), u64::from(image.height()));
    let image = if box_w == 0 || box_h == 0 || w == 0 || h == 0 {
        image
    } else if w * box_h > h * box_w {
        // Wider than the box: keep the middle columns.
        let keep = u32::try_from((h * box_w / box_h).max(1)).unwrap_or(u32::MAX);
        image.crop_imm((image.width() - keep) / 2, 0, keep, image.height())
    } else if w * box_h < h * box_w {
        let keep = u32::try_from((w * box_h / box_w).max(1)).unwrap_or(u32::MAX);
        image.crop_imm(0, (image.height() - keep) / 2, image.width(), keep)
    } else {
        image
    };
    picker
        .new_protocol(image, size, Resize::Scale(Some(FilterType::CatmullRom)))
        .ok()
}

#[cfg(test)]
mod tests;
