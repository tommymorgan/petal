//! Receive-only GTK3 compositor. The session's original LiveKit event consumer
//! feeds this adapter; GTK objects never leave the main thread. Each decoder has
//! one latest-frame slot, drained by a single GTK timer rather than queued GUI
//! callbacks for every video frame.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use gtk::atk::prelude::*;
use gtk::prelude::*;
use livekit::track::{RemoteTrack, VideoQuality};
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit::RoomEvent;
use tokio_util::sync::CancellationToken;

use crate::room_generation::RoomGeneration;
use crate::sync_ext::MutexExt;
use crate::transport::publisher::{self, SharedSourceKind};
use crate::video_color::VideoColorProfile;

type WindowKey = (String, u32);
type Slots = Arc<Mutex<HashMap<WindowKey, Arc<ReceiveSlot>>>>;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteWindowSummary {
    window_id: u32,
    owner_identity: String,
    owner_display_name: String,
    source_title: String,
    hidden: bool,
    source_kind: SharedSourceKind,
    share_instance_id: Option<String>,
    content_crop: Option<(f64, f64, f64, f64)>,
    /// Actual decoded pixels accepted by the native renderer, not window allocation.
    decoded_size: Option<(u32, u32)>,
}

struct Pixels {
    width: i32,
    height: i32,
    bgra: Vec<u8>,
}

fn frame_dimensions_valid(width: u32, height: u32) -> bool {
    // Match the native subscriber's libwebrtc RTC_CHECK guard, BEFORE to_i420.
    (1..=16_384).contains(&width) && (1..=16_384).contains(&height)
}

impl Pixels {
    fn from_i420(
        planes: (&[u8], &[u8], &[u8]),
        strides: (usize, usize, usize),
        width: u32,
        height: u32,
        profile: VideoColorProfile,
    ) -> Option<Self> {
        if !frame_dimensions_valid(width, height) {
            return None;
        }
        let bgra = crate::video_color::convert_i420_to_bgra(
            planes.0, strides.0, planes.1, strides.1, planes.2, strides.2, width, height, profile,
        )?;
        Some(Self {
            width: width as i32,
            height: height as i32,
            bgra,
        })
    }

    fn into_surface(self) -> Result<gtk::cairo::ImageSurface, gtk::cairo::Error> {
        // Cairo ARgb32 on Linux little-endian is native BGRA; opaque alpha means
        // no additional premultiply pass or pixel copy is necessary.
        gtk::cairo::ImageSurface::create_for_data(
            self.bgra,
            gtk::cairo::Format::ARgb32,
            self.width,
            self.height,
            self.width * 4,
        )
    }
}

struct ReceiveSlot {
    key: WindowKey,
    sid: String,
    summary: Mutex<RemoteWindowSummary>,
    latest: Mutex<Option<Pixels>>,
    cancel: CancellationToken,
}

impl ReceiveSlot {
    fn stop(&self) {
        self.cancel.cancel();
        self.latest.lock_unpoisoned().take();
    }
}

#[derive(Clone)]
pub(crate) struct LinuxReceiveFeed {
    slots: Slots,
    cancel: CancellationToken,
    generation: RoomGeneration,
    local_identity: String,
}

impl LinuxReceiveFeed {
    pub(crate) fn new(
        app: &tauri::AppHandle,
        generation: RoomGeneration,
        local_identity: String,
    ) -> Result<Self, tauri::Error> {
        let feed = Self {
            slots: Arc::new(Mutex::new(HashMap::new())),
            cancel: CancellationToken::new(),
            generation,
            local_identity,
        };
        let ui_feed = feed.clone();
        app.run_on_main_thread(move || start_gtk_pump(ui_feed))?;
        Ok(feed)
    }

    pub(crate) async fn cancelled(&self) {
        self.cancel.cancelled().await;
    }

    pub(crate) fn stop(&self) {
        self.cancel.cancel();
        for (_, slot) in self.slots.lock_unpoisoned().drain() {
            slot.stop();
        }
    }

    pub(crate) fn summaries(&self) -> Vec<RemoteWindowSummary> {
        self.slots
            .lock_unpoisoned()
            .values()
            .filter(|slot| !slot.cancel.is_cancelled())
            .map(|slot| slot.summary.lock_unpoisoned().clone())
            .collect()
    }

    pub(crate) fn handle_event(&self, event: RoomEvent) {
        if self.cancel.is_cancelled() || !self.generation.is_current() {
            return;
        }
        match event {
            RoomEvent::TrackSubscribed {
                track,
                publication,
                participant,
            } => {
                let owner = participant.identity().to_string();
                if owner == self.local_identity {
                    return;
                }
                let RemoteTrack::Video(video) = track else {
                    return;
                };
                let Some(window_id) = publisher::window_id_from_track_name(&video.name()) else {
                    // Cameras and arbitrary video tracks never become native windows.
                    return;
                };
                let metadata = participant.metadata();
                let source_kind = publisher::shared_window_kind_from_metadata(&metadata, window_id);
                let source_title =
                    publisher::shared_window_title_from_metadata(&metadata, window_id)
                        .unwrap_or_else(|| video.name());
                let name = participant.name();
                let slot = Arc::new(ReceiveSlot {
                    key: (owner.clone(), window_id),
                    sid: publication.sid().to_string(),
                    summary: Mutex::new(RemoteWindowSummary {
                        window_id,
                        owner_identity: owner.clone(),
                        owner_display_name: if name.is_empty() { owner.clone() } else { name },
                        source_title,
                        hidden: false,
                        source_kind,
                        share_instance_id: publisher::shared_window_share_instance_from_metadata(
                            &metadata, window_id,
                        ),
                        content_crop: None,
                        decoded_size: None,
                    }),
                    latest: Mutex::new(None),
                    cancel: self.cancel.child_token(),
                });
                let key = (owner, window_id);
                if let Some(previous) = self.slots.lock_unpoisoned().insert(key, slot.clone()) {
                    previous.stop();
                }
                log::info!(
                    "linux incoming subscribed window_id={} sid={}",
                    window_id,
                    slot.sid
                );
                // Both controls are required for simulcast: the quality request
                // alone can leave a subscription on the tiny editor layer.
                publication.set_video_quality(VideoQuality::High);
                let livekit::prelude::TrackDimension(width, height) = publication.dimension();
                if width > 0 && height > 0 {
                    publication
                        .update_video_dimensions(livekit::prelude::TrackDimension(width, height));
                }
                let profile =
                    publisher::shared_window_color_profile_from_metadata(&metadata, window_id)
                        .unwrap_or_else(VideoColorProfile::legacy_publish_default);
                let generation = self.generation.clone();
                tauri::async_runtime::spawn(async move {
                    let mut stream = NativeVideoStream::new(video.rtc_track());
                    loop {
                        let frame = tokio::select! {
                            biased;
                            _ = slot.cancel.cancelled() => break,
                            frame = stream.next() => frame,
                        };
                        let Some(frame) = frame else { break };
                        if !generation.is_current() || slot.cancel.is_cancelled() {
                            break;
                        }
                        let width = frame.buffer.width();
                        let height = frame.buffer.height();
                        if !frame_dimensions_valid(width, height) {
                            continue;
                        }
                        let i420 = frame.buffer.to_i420();
                        let (y, u, v) = i420.data();
                        let (ys, us, vs) = i420.strides();
                        let Some(pixels) = Pixels::from_i420(
                            (y, u, v),
                            (ys as usize, us as usize, vs as usize),
                            width,
                            height,
                            profile,
                        ) else {
                            continue;
                        };
                        let mut latest = slot.latest.lock_unpoisoned();
                        if slot.cancel.is_cancelled() || !generation.is_current() {
                            break;
                        }
                        *latest = Some(pixels);
                    }
                });
            }
            RoomEvent::TrackUnpublished {
                publication,
                participant,
            } => {
                if let Some(id) = publisher::window_id_from_track_name(&publication.name()) {
                    self.remove_if_sid(
                        &(participant.identity().to_string(), id),
                        &publication.sid().to_string(),
                    );
                    log::info!(
                        "linux incoming unpublished window_id={} sid={}",
                        id,
                        publication.sid()
                    );
                }
            }
            RoomEvent::ParticipantDisconnected(participant) => {
                let owner = participant.identity().to_string();
                self.slots.lock_unpoisoned().retain(|key, slot| {
                    if key.0 == owner {
                        slot.stop();
                        false
                    } else {
                        true
                    }
                });
            }
            RoomEvent::ParticipantMetadataChanged { participant, .. } => {
                let owner = participant.identity().to_string();
                let metadata = participant.metadata();
                for (key, slot) in self.slots.lock_unpoisoned().iter() {
                    if key.0 == owner {
                        let mut summary = slot.summary.lock_unpoisoned();
                        if let Some(title) =
                            publisher::shared_window_title_from_metadata(&metadata, key.1)
                        {
                            summary.source_title = title;
                        }
                    }
                }
            }
            // A resize republish may unsubscribe the old SID before the new
            // subscription lands. Keep the native window until actual unpublish
            // or room/participant teardown, not an unrelated subscription event.
            _ => {}
        }
    }

    fn remove_if_sid(&self, key: &WindowKey, sid: &str) {
        let mut slots = self.slots.lock_unpoisoned();
        if slots.get(key).is_some_and(|slot| slot.sid == sid) {
            if let Some(slot) = slots.remove(key) {
                slot.stop();
            }
        }
    }
}

struct NativeWindow {
    window: gtk::Window,
    area: gtk::DrawingArea,
    surface: Rc<RefCell<Option<gtk::cairo::ImageSurface>>>,
    title: String,
}

impl NativeWindow {
    fn new(feed: &LinuxReceiveFeed, key: WindowKey, pixels: &Pixels) -> Self {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_decorated(true);
        window.set_resizable(true);
        window.set_default_size(pixels.width.min(1000), pixels.height.min(700));
        let area = gtk::DrawingArea::new();
        if let Some(accessible) = area.accessible() {
            accessible.set_name("Remote shared window");
        }
        let surface: Rc<RefCell<Option<gtk::cairo::ImageSurface>>> = Rc::new(RefCell::new(None));
        let draw_surface = surface.clone();
        area.connect_draw(move |widget, context| {
            context.set_source_rgb(0.0, 0.0, 0.0);
            let _ = context.paint();
            if let Some(surface) = draw_surface.borrow().as_ref() {
                let width = f64::from(widget.allocated_width());
                let height = f64::from(widget.allocated_height());
                let scale =
                    (width / f64::from(surface.width())).min(height / f64::from(surface.height()));
                context.translate(
                    (width - f64::from(surface.width()) * scale) / 2.0,
                    (height - f64::from(surface.height()) * scale) / 2.0,
                );
                context.scale(scale, scale);
                if context.set_source_surface(surface, 0.0, 0.0).is_ok() {
                    let _ = context.paint();
                }
            }
            gtk::glib::Propagation::Proceed
        });
        window.add(&area);
        let slots = feed.slots.clone();
        window.connect_delete_event(move |_, _| {
            if let Some(slot) = slots.lock_unpoisoned().remove(&key) {
                slot.stop();
            }
            gtk::glib::Propagation::Proceed
        });
        Self {
            window,
            area,
            surface,
            title: String::new(),
        }
    }
}

fn start_gtk_pump(feed: LinuxReceiveFeed) {
    let mut windows: HashMap<WindowKey, NativeWindow> = HashMap::new();
    // Reuse the snapshot allocation; only Arc handles cross the lock boundary,
    // not a newly copied owner identity on every GTK tick.
    let mut slots = Vec::new();
    gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
        if feed.cancel.is_cancelled() || !feed.generation.is_current() {
            for (_, native) in windows.drain() {
                native.window.close();
            }
            feed.stop();
            return gtk::glib::ControlFlow::Break;
        }
        slots.clear();
        slots.extend(feed.slots.lock_unpoisoned().values().cloned());
        windows.retain(|key, native| {
            let keep = slots.iter().any(|slot| &slot.key == key);
            if !keep {
                native.window.close();
            }
            keep
        });
        for slot in &slots {
            if slot.cancel.is_cancelled() || !feed.generation.is_current() {
                continue;
            }
            let pixels = slot.latest.lock_unpoisoned().take();
            if let Some(pixels) = pixels {
                if !windows.contains_key(&slot.key) {
                    windows.insert(
                        slot.key.clone(),
                        NativeWindow::new(&feed, slot.key.clone(), &pixels),
                    );
                }
                let native = windows.get_mut(&slot.key).expect("inserted native window");
                match pixels.into_surface() {
                    Ok(surface) => {
                        let decoded_size = (surface.width() as u32, surface.height() as u32);
                        *native.surface.borrow_mut() = Some(surface);
                        native.area.queue_draw();
                        let mut summary = slot.summary.lock_unpoisoned();
                        summary.decoded_size = Some(decoded_size);
                        if native.title != summary.source_title {
                            native.title.clone_from(&summary.source_title);
                            native
                                .window
                                .set_title(&format!("Petal — {}", native.title));
                        }
                        if !native.window.is_visible() {
                            native.window.show_all();
                        }
                    }
                    Err(error) => log::warn!("Linux incoming frame rejected by Cairo: {error}"),
                }
            }
        }
        gtk::glib::ControlFlow::Continue
    });
}

#[tauri::command]
pub(crate) fn compositor_list_windows(
    state: tauri::State<'_, crate::session::SessionState>,
) -> Vec<RemoteWindowSummary> {
    state.remote_window_summaries()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_i420_becomes_opaque_native_pixels_without_plane_overread() {
        // Independent limited-range BT.601 red reference, including padded Y.
        let frame = Pixels::from_i420(
            (&[81, 81, 0, 81, 81, 0], &[90], &[240]),
            (3, 1, 1),
            2,
            2,
            VideoColorProfile::BT601_VIDEO,
        )
        .unwrap();
        for pixel in frame.bgra.chunks_exact(4) {
            assert!(pixel[0] < 3 && pixel[1] < 3 && pixel[2] > 250);
            assert_eq!(pixel[3], 255);
        }
        assert!(Pixels::from_i420(
            (&[81; 3], &[90], &[240]),
            (2, 1, 1),
            2,
            2,
            VideoColorProfile::BT601_VIDEO
        )
        .is_none());
        assert!(Pixels::from_i420(
            (&[81; 4], &[], &[240]),
            (2, 1, 1),
            2,
            2,
            VideoColorProfile::BT601_VIDEO
        )
        .is_none());
        assert!(Pixels::from_i420(
            (&[81; 4], &[90], &[240]),
            (1, 1, 1),
            2,
            2,
            VideoColorProfile::BT601_VIDEO
        )
        .is_none());
    }

    #[test]
    fn stale_unpublish_cannot_remove_a_replacement_or_another_owners_window() {
        let generation = RoomGeneration::new(Arc::new(std::sync::atomic::AtomicU64::new(1)), 1);
        let feed = LinuxReceiveFeed {
            slots: Arc::new(Mutex::new(HashMap::new())),
            cancel: CancellationToken::new(),
            generation,
            local_identity: "local".into(),
        };
        let slot = |owner: &str, sid: &str| {
            Arc::new(ReceiveSlot {
                key: (owner.into(), 7),
                sid: sid.into(),
                cancel: feed.cancel.child_token(),
                latest: Mutex::new(None),
                summary: Mutex::new(RemoteWindowSummary {
                    window_id: 7,
                    owner_identity: owner.into(),
                    owner_display_name: owner.into(),
                    source_title: "Editor".into(),
                    hidden: false,
                    source_kind: SharedSourceKind::Window,
                    share_instance_id: None,
                    content_crop: None,
                    decoded_size: None,
                }),
            })
        };
        let alice = slot("alice", "new");
        let bob = slot("bob", "other");
        feed.slots
            .lock_unpoisoned()
            .insert(("alice".into(), 7), alice.clone());
        feed.slots
            .lock_unpoisoned()
            .insert(("bob".into(), 7), bob.clone());
        feed.remove_if_sid(&("alice".into(), 7), "old");
        assert_eq!(feed.summaries().len(), 2);
        assert!(!alice.cancel.is_cancelled());
        feed.remove_if_sid(&("alice".into(), 7), "new");
        assert!(alice.cancel.is_cancelled());
        assert!(!bob.cancel.is_cancelled());
        assert_eq!(feed.summaries()[0].owner_identity, "bob");
        feed.stop();
        assert!(bob.cancel.is_cancelled());
        assert!(feed.summaries().is_empty());
    }
}
