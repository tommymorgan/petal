//! Receive-side GTK3 compositor with native controller input (Task 3.1).
//! The session's original LiveKit event consumer feeds this adapter; GTK
//! objects never leave the main thread. Each decoder has one latest-frame
//! slot, drained by a single GTK timer rather than queued GUI callbacks for
//! every video frame.
//!
//! Controller input follows the same shape as the macOS/Windows control
//! overlay: a Request/Release affordance, request/consent timeouts, pointer /
//! wheel / key drafts published over the remote-control data channel while a
//! Windows grant is active, synthetic releases when control ends, and
//! clipboard chords routed to the remote clipboard commands. The registry of
//! live receive targets (`linux_remote_control`) is maintained from the same
//! room events that create and retire windows, so controller authority can
//! only ever name a share that is currently on screen.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use gtk::atk::prelude::*;
use gtk::gdk;
use gtk::gdk::ModifierType;
use gtk::prelude::*;
use livekit::track::{RemoteTrack, VideoQuality};
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit::RoomEvent;
use tokio_util::sync::CancellationToken;

use crate::remote_control_core::{
    RemoteControlAction, RemoteControlDraft, RemoteControlModifiers, RemoteControlTargetKind,
    RemoteControlType,
};
use crate::room_generation::RoomGeneration;
use crate::sync_ext::MutexExt;
use crate::transport::publisher::{self, SharedSourceKind};
use crate::video_color::VideoColorProfile;

type WindowKey = (String, u32);
type Slots = Arc<Mutex<HashMap<WindowKey, Arc<ReceiveSlot>>>>;

fn wire_target_kind(kind: SharedSourceKind) -> RemoteControlTargetKind {
    match kind {
        SharedSourceKind::Window => RemoteControlTargetKind::Window,
        SharedSourceKind::Display | SharedSourceKind::DisplayRegion => {
            RemoteControlTargetKind::Display
        }
    }
}

/// Cross-thread command queue for the GTK controller surfaces. Producers are
/// the remote-control receiver (status/overlay flips) on async worker
/// threads; the 16 ms GTK pump is the sole consumer, so GTK objects stay on
/// the main thread by construction.
#[derive(Debug, Clone)]
enum ControllerCommand {
    Status {
        key: WindowKey,
        request_id: String,
        status: String,
        message: String,
    },
    RequestFailed {
        key: WindowKey,
        reservation: crate::linux_remote_control::ControlRequestReservation,
        message: String,
    },
    SetActive {
        key: WindowKey,
        active: bool,
    },
}

static CONTROLLER_COMMANDS: LazyLock<Mutex<VecDeque<ControllerCommand>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));

fn queue_controller_command(command: ControllerCommand) {
    CONTROLLER_COMMANDS.lock_unpoisoned().push_back(command);
}

/// Queue a full controller status (wire status string + user-facing message)
/// for the GTK surface of one receive window. Safe from any thread.
pub(crate) fn queue_controller_status(
    owner_identity: &str,
    window_id: u32,
    request_id: &str,
    status: &str,
    message: &str,
) {
    queue_controller_command(ControllerCommand::Status {
        key: (owner_identity.to_string(), window_id),
        request_id: request_id.to_string(),
        status: status.to_string(),
        message: message.to_string(),
    });
}

pub(crate) fn queue_controller_request_failed(
    owner_identity: &str,
    window_id: u32,
    reservation: crate::linux_remote_control::ControlRequestReservation,
    message: &str,
) {
    queue_controller_command(ControllerCommand::RequestFailed {
        key: (owner_identity.to_string(), window_id),
        reservation,
        message: format!("Remote control request could not be sent: {message}"),
    });
}

/// Queue an overlay flip (active/inactive) for the GTK surface of one receive
/// window. Safe from any thread.
pub(crate) fn queue_controller_set_active(owner_identity: &str, window_id: u32, active: bool) {
    queue_controller_command(ControllerCommand::SetActive {
        key: (owner_identity.to_string(), window_id),
        active,
    });
}

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
        APP_HANDLE.get_or_init(|| app.clone());
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
        // No room channel exists anymore: every controller grant is void and
        // every receive target is gone (leave/disconnect teardown, R03).
        crate::linux_remote_control::retire_all_controllers();
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
                let share_instance = slot.summary.lock_unpoisoned().share_instance_id.clone();
                // A new subscription for the same key is a replacement: the
                // previous share instance (and any controller grant naming
                // it) is retired BEFORE the new target becomes current.
                let replaced = self
                    .slots
                    .lock_unpoisoned()
                    .insert(key.clone(), slot.clone());
                if let Some(previous) = replaced {
                    previous.stop();
                    crate::linux_remote_control::retire_controller_for(&key.0, key.1);
                }
                crate::linux_remote_control::upsert_receive_target(
                    crate::linux_remote_control::ReceiveTargetInfo {
                        owner_identity: key.0.clone(),
                        window_id: key.1,
                        target_kind: wire_target_kind(source_kind),
                        share_instance_id: share_instance,
                    },
                );
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
                // Every grant the departed sharer issued is void (R03).
                crate::linux_remote_control::retire_controllers_for_owner(&owner);
            }
            RoomEvent::ParticipantMetadataChanged { participant, .. } => {
                let owner = participant.identity().to_string();
                let metadata = participant.metadata();
                for (key, slot) in self.slots.lock_unpoisoned().iter() {
                    if key.0 != owner {
                        continue;
                    }
                    let mut summary = slot.summary.lock_unpoisoned();
                    if let Some(title) =
                        publisher::shared_window_title_from_metadata(&metadata, key.1)
                    {
                        summary.source_title = title;
                    }
                    // A re-published share carries a NEW instance id: the
                    // old share (and any controller grant naming it) is
                    // retired and cannot be restored by late packets.
                    let source_kind = publisher::shared_window_kind_from_metadata(&metadata, key.1);
                    let share_instance =
                        publisher::shared_window_share_instance_from_metadata(&metadata, key.1);
                    let instance_changed = share_instance != summary.share_instance_id
                        || source_kind != summary.source_kind;
                    if instance_changed {
                        summary.source_kind = source_kind;
                        summary.share_instance_id = share_instance.clone();
                        crate::linux_remote_control::retire_controller_for(&key.0, key.1);
                    }
                    crate::linux_remote_control::upsert_receive_target(
                        crate::linux_remote_control::ReceiveTargetInfo {
                            owner_identity: key.0.clone(),
                            window_id: key.1,
                            target_kind: wire_target_kind(summary.source_kind),
                            share_instance_id: summary.share_instance_id.clone(),
                        },
                    );
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
                // Source stopped: its receive target and any controller
                // grant naming it are retired with it (R03).
                crate::linux_remote_control::retire_controller_for(&key.0, key.1);
            }
        }
    }
}

/// The one process-wide Tauri handle, captured when the first receive feed is
/// created. GTK controller surfaces need it to publish drafts; it is the same
/// app object for every room generation.
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

fn app_handle() -> Option<&'static tauri::AppHandle> {
    APP_HANDLE.get()
}

/// GTK button events carry no DOM-style click count; reproduce the double /
/// triple-click arithmetic GTK itself uses (same button within the multi-click
/// window) so the host can synthesize real double clicks (#373 parity).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ClickArithmetic {
    last_button: Option<u8>,
    last_time_ms: u32,
    count: u32,
}

impl ClickArithmetic {
    pub(crate) const MULTI_CLICK_WINDOW_MS: u32 = 400;

    fn new() -> Self {
        Self {
            last_button: None,
            last_time_ms: 0,
            count: 0,
        }
    }

    pub(crate) fn press(&mut self, button: u8, time_ms: u32) -> u32 {
        let repeated = self.last_button == Some(button)
            && time_ms.saturating_sub(self.last_time_ms) <= Self::MULTI_CLICK_WINDOW_MS;
        self.count = if repeated { (self.count + 1).min(3) } else { 1 };
        self.last_button = Some(button);
        self.last_time_ms = time_ms;
        self.count
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControllerMode {
    Idle,
    Requesting,
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeldKey {
    key: String,
    code: String,
    location: u8,
}

#[derive(Debug, Clone, Default)]
struct PendingMove {
    x: f64,
    y: f64,
    button: i16,
    buttons: u16,
    alt: bool,
    ctrl: bool,
    meta: bool,
    shift: bool,
}

#[derive(Debug, Clone)]
struct PendingWheel {
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
    modifiers: RemoteControlModifiers,
}

#[derive(Default)]
struct ControllerState {
    mode: Option<ControllerMode>,
    request_serial: u64,
    request_id: Option<String>,
    timeout_source: Option<gtk::glib::SourceId>,
    feedback_source: Option<gtk::glib::SourceId>,
    seq: u64,
    source_size: Option<(u32, u32)>,
    held_buttons: u8,
    held_keys: Vec<HeldKey>,
    pending_move: Option<PendingMove>,
    pending_wheel: Option<PendingWheel>,
    move_flush: Option<gtk::glib::SourceId>,
    wheel_flush: Option<gtk::glib::SourceId>,
    last_move_quantized: Option<(u16, u16)>,
    last_pointer_position: (f64, f64),
    /// Ctrl is held locally while we learn whether the next key is a
    /// clipboard chord (mirrors the web control overlay).
    clipboard_modifier: Option<HeldKey>,
    clipboard_consumed: bool,
    clipboard_shortcut_code: Option<String>,
    clicks: ClickArithmetic,
}

impl ControllerState {
    fn mode(&self) -> ControllerMode {
        self.mode.unwrap_or(ControllerMode::Idle)
    }

    fn next_seq(&mut self) -> u64 {
        self.seq = self.seq.saturating_add(1).max(1);
        self.seq
    }
}

#[derive(Clone)]
struct ControllerUi {
    key: WindowKey,
    button: gtk::Button,
    label: gtk::Label,
    state: Rc<RefCell<ControllerState>>,
}

fn dom_modifiers(state: ModifierType) -> RemoteControlModifiers {
    RemoteControlModifiers {
        alt: state.contains(ModifierType::MOD1_MASK),
        ctrl: state.contains(ModifierType::CONTROL_MASK),
        meta: state.contains(ModifierType::MOD4_MASK),
        shift: state.contains(ModifierType::SHIFT_MASK),
    }
}

/// GDK button number (1 primary, 2 middle, 3 secondary) → DOM button index.
fn dom_button(gdk_button: u32) -> Option<i16> {
    match gdk_button {
        1 => Some(0),
        2 => Some(1),
        3 => Some(2),
        _ => None,
    }
}

fn dom_button_mask(dom_button: i16) -> u16 {
    match dom_button {
        0 => 1,
        1 => 4,
        2 => 2,
        _ => 0,
    }
}

fn quantized(x: f64, y: f64) -> (u16, u16) {
    let clamp01 = |value: f64| value.clamp(0.0, 1.0);
    (
        (clamp01(x) * f64::from(u16::MAX)).round() as u16,
        (clamp01(y) * f64::from(u16::MAX)).round() as u16,
    )
}

fn spawn_controller_send(draft: RemoteControlDraft) {
    let Some(app) = app_handle().cloned() else {
        return;
    };
    if let Err(error) = crate::remote_control::linux_queue_controller_input(app, draft) {
        log::debug!("linux controller: draft publish failed: {error}");
    }
}

fn input_draft(
    state: &mut ControllerState,
    key: &WindowKey,
    message_type: RemoteControlType,
) -> RemoteControlDraft {
    RemoteControlDraft { request_id: None, message_type,
    action: None,
    window_id: key.1,
    target_owner_id: Some(key.0.clone()),
    seq: state.next_seq(),
    target_kind: None,
    share_instance_id: None,
    controller_capabilities: Vec::new(),
    x: None,
    y: None,
    button: None,
    buttons: None,
    click_count: None,
    delta_x: None,
    delta_y: None,
    delta_mode: None,
    key: None,
    code: None,
    repeat: false,
    location: None,
    text: None,
    grant_token: None,
    modifiers: RemoteControlModifiers::default(), }
}

fn move_draft(pending: &PendingMove, key: &WindowKey, seq: u64) -> RemoteControlDraft {
    RemoteControlDraft { request_id: None, message_type: RemoteControlType::Pointer,
    action: Some(RemoteControlAction::Move),
    window_id: key.1,
    target_owner_id: Some(key.0.clone()),
    seq,
    x: Some(pending.x),
    y: Some(pending.y),
    button: Some(pending.button),
    buttons: Some(pending.buttons),
    modifiers: RemoteControlModifiers {
        alt: pending.alt,
        ctrl: pending.ctrl,
        meta: pending.meta,
        shift: pending.shift,
    },
    ..empty_draft() }
}

fn wheel_draft(pending: &PendingWheel, key: &WindowKey, seq: u64) -> RemoteControlDraft {
    RemoteControlDraft { request_id: None, message_type: RemoteControlType::Wheel,
    action: None,
    window_id: key.1,
    target_owner_id: Some(key.0.clone()),
    seq,
    x: Some(pending.x),
    y: Some(pending.y),
    delta_x: Some(pending.delta_x),
    delta_y: Some(pending.delta_y),
    // GDK smooth deltas, like discrete directions, are wheel units,
    // not DOM pixels. Preserve them as line-mode input for the host.
    delta_mode: Some(1),
    modifiers: pending.modifiers.clone(),
    ..empty_draft() }
}

impl ControllerUi {
    fn new(state: Rc<RefCell<ControllerState>>, key: WindowKey) -> Self {
        let button = gtk::Button::with_label("Request control");
        if let Some(accessible) = button.accessible() {
            accessible.set_name("Request remote control");
        }
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        let ui = Self {
            key,
            button,
            label,
            state,
        };
        ui.refresh_controls("Request control", "");
        ui
    }

    fn refresh_controls(&self, button_text: &str, status: &str) {
        self.button.set_label(button_text);
        self.label.set_text(status);
    }

    fn idle_presentation(&self) {
        self.refresh_controls("Request control", "");
    }

    /// Everything local that must happen when control ends, whatever ended
    /// it: synthetic releases for held input, dropped pending coalesced
    /// input, cleared clipboard-chord state, back to the idle affordance.
    /// Grant retirement itself belongs to the authority layer
    /// (`linux_remote_control`), not the presentation.
    fn deactivate(&self, status_text: &'static str) {
        let key = self.key.clone();
        let mut state = self.state.borrow_mut();
        state.mode = Some(ControllerMode::Idle);
        state.request_serial += 1;
        Self::clear_timeout(&mut state);
        let held_keys = std::mem::take(&mut state.held_keys);
        let held_buttons = std::mem::take(&mut state.held_buttons);
        let pending_move = state.pending_move.take();
        let _pending_wheel = state.pending_wheel.take();
        state.last_move_quantized = None;
        state.clipboard_modifier = None;
        state.clipboard_consumed = false;
        state.clipboard_shortcut_code = None;
        for held in held_keys {
            let mut draft = input_draft(&mut state, &key, RemoteControlType::Key);
            draft.action = Some(RemoteControlAction::Up);
            draft.key = Some(held.key);
            draft.code = Some(held.code);
            draft.location = Some(held.location);
            spawn_controller_send(draft);
        }
        if held_buttons != 0 {
            for dom in 0..3i16 {
                if held_buttons & (dom_button_mask(dom) as u8) != 0 {
                    let mut draft = input_draft(&mut state, &key, RemoteControlType::Pointer);
                    draft.action = Some(RemoteControlAction::Up);
                    draft.button = Some(dom);
                    draft.buttons = Some(0);
                    let (x, y) = pending_move
                        .as_ref()
                        .map(|pending| (pending.x, pending.y))
                        .unwrap_or(state.last_pointer_position);
                    draft.x = Some(x);
                    draft.y = Some(y);
                    spawn_controller_send(draft);
                }
            }
        }
        // A pending wheel flush is dropped, not sent: control ended, so no
        // further input may reach the sharer. `pending_wheel` was taken above
        // and deliberately discarded.
        drop(state);
        self.show_feedback(status_text);
        self.idle_presentation();
    }

    fn clear_timeout(state: &mut ControllerState) {
        if let Some(source) = state.timeout_source.take() {
            source.remove();
        }
    }

    fn clear_feedback(&self) {
        let mut state = self.state.borrow_mut();
        if let Some(source) = state.feedback_source.take() {
            source.remove();
        }
    }

    fn show_feedback(&self, text: &str) {
        self.clear_feedback();
        self.label.set_text(text);
        let state = self.state.clone();
        let label = self.label.clone();
        let source = gtk::glib::timeout_add_local(Duration::from_millis(3_000), move || {
            let mode = state.borrow().mode();
            label.set_text(match mode {
                ControllerMode::Active => "Controlling",
                ControllerMode::Requesting => "Waiting for the sharer…",
                ControllerMode::Idle => "",
            });
            if let Ok(mut state) = state.try_borrow_mut() {
                state.feedback_source = None;
            }
            gtk::glib::ControlFlow::Break
        });
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.feedback_source = Some(source);
        }
    }

    fn start_timeout(&self, timeout: Duration) {
        let mut state = self.state.borrow_mut();
        Self::clear_timeout(&mut state);
        let serial = state.request_serial + 1;
        state.request_serial = serial;
        let key = self.key.clone();
        let controller_state = self.state.clone();
        let ui_label = self.label.clone();
        let ui_button = self.button.clone();
        let request_id = state.request_id.clone();
        state.timeout_source = Some(gtk::glib::timeout_add_local(timeout, move || {
            let mut state = controller_state.borrow_mut();
            if state.request_serial == serial && state.mode() == ControllerMode::Requesting {
                state.mode = Some(ControllerMode::Idle);
                state.timeout_source = None;
                drop(state);
                ui_label.set_text("No answer from the sharer.");
                ui_button.set_label("Request control");
                let (owner, window_id) = key.clone();
                if let (Some(app), Some(request_id)) = (app_handle().cloned(), request_id.as_ref()) {
                    // The command is synchronous: retire this exact pending
                    // attempt before returning control to GTK or publishing B.
                    let _ = crate::remote_control::remote_control_request_timed_out(
                        app, window_id, Some(owner), request_id.clone());
                }
            }
            gtk::glib::ControlFlow::Break
        }));
    }

    fn on_toggle(&self) {
        let mut state = self.state.borrow_mut();
        match state.mode() {
            ControllerMode::Idle => {
                let (owner, window_id) = self.key.clone();
                let Some(app) = app_handle().cloned() else {
                    drop(state);
                    self.show_feedback("Remote control is not available.");
                    return;
                };
                let reservation =
                    match crate::linux_remote_control::reserve_control_request(&owner, window_id, None) {
                        Ok(reservation) => reservation,
                        Err(error) => {
                            drop(state);
                            self.show_feedback(&error);
                            return;
                        }
                    };
                let Some(request_id) = crate::linux_remote_control::reserved_control_request_id(&owner, window_id, reservation) else {
                    drop(state);
                    self.idle_presentation();
                    return;
                };
                state.request_id = Some(request_id);
                state.mode = Some(ControllerMode::Requesting);
                drop(state);
                self.refresh_controls("Cancel request", "Requesting control…");
                self.start_timeout(request_timeout());
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = crate::remote_control::linux_remote_control_request(
                        app,
                        window_id,
                        owner,
                        reservation,
                        None,
                    )
                    .await
                    {
                        log::warn!("linux controller: control request failed: {error}");
                    }
                });
            }
            ControllerMode::Requesting => {
                // Withdraw the pending request: the sharer's consent prompt
                // resolves through the Release the command publishes.
                state.mode = Some(ControllerMode::Idle);
                state.request_serial += 1;
                Self::clear_timeout(&mut state);
                drop(state);
                self.idle_presentation();
                self.release_remote();
            }
            ControllerMode::Active => {
                drop(state);
                self.deactivate("Control released.");
                self.release_remote();
            }
        }
    }

    fn release_remote(&self) {
        let (owner, window_id) = self.key.clone();
        let mut draft = empty_draft();
        draft.message_type = RemoteControlType::Release;
        draft.window_id = window_id;
        draft.target_owner_id = Some(owner.clone());
        draft.request_id = self.state.borrow().request_id.clone();
        spawn_controller_send(draft);
    }

    /// Apply a wire status packet from the sharer.
    fn apply_status(&self, status: &str, message: &str, request_id: Option<&str>) {
        let mut state = self.state.borrow_mut();
        if let Some(request_id) = request_id { state.request_id = Some(request_id.to_string()); }
        match status {
            "active" => {
                if !crate::linux_remote_control::controller_is_active(&self.key.0, self.key.1) {
                    return;
                }
                state.mode = Some(ControllerMode::Active);
                state.request_serial += 1;
                Self::clear_timeout(&mut state);
                drop(state);
                self.refresh_controls("Release control", "Controlling");
            }
            "awaitingConsent" => {
                if state.mode() == ControllerMode::Requesting {
                    drop(state);
                    self.refresh_controls("Cancel request", "Waiting for approval…");
                    self.start_timeout(consent_timeout());
                }
            }
            "stopped" | "disabled" => {
                drop(state);
                self.deactivate("Control stopped.");
            }
            // Denied, failed requests and per-operation feedback: visible
            // transient result, never a controlling state (R02).
            _ => {
                let was_requesting = state.mode() == ControllerMode::Requesting;
                if was_requesting {
                    state.mode = Some(ControllerMode::Idle);
                    state.request_serial += 1;
                    Self::clear_timeout(&mut state);
                }
                drop(state);
                if was_requesting {
                    self.idle_presentation();
                }
                self.show_feedback(if message.is_empty() { status } else { message });
            }
        }
    }

    fn apply_active(&self, active: bool) {
        if active {
            self.apply_status("active", "", None);
        } else {
            let idle = self.state.borrow().mode() == ControllerMode::Idle;
            if !idle {
                self.deactivate("Control stopped.");
            }
        }
    }

    fn flush_pending_move(&self) {
        let key = self.key.clone();
        let mut state = self.state.borrow_mut();
        state.move_flush = None;
        let Some(pending) = state.pending_move.take() else {
            return;
        };
        if state.mode() != ControllerMode::Active {
            return;
        }
        let coordinate = quantized(pending.x, pending.y);
        if state.last_move_quantized == Some(coordinate) {
            return;
        }
        state.last_move_quantized = Some(coordinate);
        state.last_pointer_position = (pending.x, pending.y);
        let seq = state.next_seq();
        spawn_controller_send(move_draft(&pending, &key, seq));
    }

    fn schedule_move_flush(&self) {
        let mut state = self.state.borrow_mut();
        if state.move_flush.is_some() {
            return;
        }
        let ui_state = self.state.clone();
        let key = self.key.clone();
        state.move_flush = Some(gtk::glib::idle_add_local(move || {
            let mut state = ui_state.borrow_mut();
            state.move_flush = None;
            let Some(pending) = state.pending_move.take() else {
                return gtk::glib::ControlFlow::Break;
            };
            if state.mode() != ControllerMode::Active {
                return gtk::glib::ControlFlow::Break;
            }
            let coordinate = quantized(pending.x, pending.y);
            if state.last_move_quantized == Some(coordinate) {
                return gtk::glib::ControlFlow::Break;
            }
            state.last_move_quantized = Some(coordinate);
            state.last_pointer_position = (pending.x, pending.y);
            let seq = state.next_seq();
            spawn_controller_send(move_draft(&pending, &key, seq));
            gtk::glib::ControlFlow::Break
        }));
    }

    fn flush_pending_wheel(&self) {
        let key = self.key.clone();
        let mut state = self.state.borrow_mut();
        state.wheel_flush = None;
        let Some(pending) = state.pending_wheel.take() else {
            return;
        };
        if state.mode() != ControllerMode::Active {
            return;
        }
        let seq = state.next_seq();
        spawn_controller_send(wheel_draft(&pending, &key, seq));
    }

    fn schedule_wheel_flush(&self) {
        let mut state = self.state.borrow_mut();
        if state.wheel_flush.is_some() {
            return;
        }
        let ui_state = self.state.clone();
        let key = self.key.clone();
        state.wheel_flush = Some(gtk::glib::idle_add_local(move || {
            let mut state = ui_state.borrow_mut();
            state.wheel_flush = None;
            let Some(pending) = state.pending_wheel.take() else {
                return gtk::glib::ControlFlow::Break;
            };
            if state.mode() != ControllerMode::Active {
                return gtk::glib::ControlFlow::Break;
            }
            let seq = state.next_seq();
            spawn_controller_send(wheel_draft(&pending, &key, seq));
            gtk::glib::ControlFlow::Break
        }));
    }

    fn send_pointer(
        &self,
        action: RemoteControlAction,
        x: f64,
        y: f64,
        button: i16,
        buttons: u16,
        click_count: Option<u32>,
        modifiers: RemoteControlModifiers,
    ) {
        let key = self.key.clone();
        let mut state = self.state.borrow_mut();
        state.last_pointer_position = (x, y);
        let seq = state.next_seq();
        let draft = RemoteControlDraft { request_id: None, message_type: RemoteControlType::Pointer,
        action: Some(action),
        window_id: key.1,
        target_owner_id: Some(key.0),
        seq,
        x: Some(x),
        y: Some(y),
        button: Some(button),
        buttons: Some(buttons),
        click_count,
        modifiers,
        ..empty_draft() };
        spawn_controller_send(draft);
    }

    fn send_key(
        &self,
        action: RemoteControlAction,
        held: &HeldKey,
        repeat: bool,
        modifiers: RemoteControlModifiers,
    ) {
        let key = self.key.clone();
        let mut state = self.state.borrow_mut();
        let seq = state.next_seq();
        let draft = RemoteControlDraft { request_id: None, message_type: RemoteControlType::Key,
        action: Some(action),
        window_id: key.1,
        target_owner_id: Some(key.0),
        seq,
        key: Some(held.key.clone()),
        code: Some(held.code.clone()),
        repeat: action == RemoteControlAction::Down && repeat,
        location: Some(held.location),
        modifiers,
        ..empty_draft() };
        spawn_controller_send(draft);
    }

    fn invoke_clipboard(&self, operation: &str) {
        let (owner, window_id) = self.key.clone();
        let grant_token = crate::linux_remote_control::active_grant_token(window_id, &owner);
        let Some(grant_token) = grant_token else {
            self.show_feedback("Control is not active.");
            return;
        };
        let Some(app) = app_handle().cloned() else {
            return;
        };
        let owner = Some(owner);
        match operation {
            "copy" => {
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = crate::remote_control::remote_clipboard_copy(
                        app,
                        window_id,
                        owner,
                        Some(grant_token),
                    )
                    .await
                    {
                        log::debug!("linux controller: remote copy failed: {error}");
                    }
                });
            }
            _ => {
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = crate::remote_control::remote_clipboard_paste(
                        app,
                        window_id,
                        owner,
                        Some(grant_token),
                    )
                    .await
                    {
                        log::debug!("linux controller: remote paste failed: {error}");
                    }
                });
            }
        }
    }
}

fn empty_draft() -> RemoteControlDraft {
    RemoteControlDraft { request_id: None, message_type: RemoteControlType::Unknown,
    action: None,
    window_id: 0,
    target_owner_id: None,
    seq: 0,
    target_kind: None,
    share_instance_id: None,
    controller_capabilities: Vec::new(),
    x: None,
    y: None,
    button: None,
    buttons: None,
    click_count: None,
    delta_x: None,
    delta_y: None,
    delta_mode: None,
    key: None,
    code: None,
    repeat: false,
    location: None,
    text: None,
    grant_token: None,
    modifiers: RemoteControlModifiers::default(), }
}

fn request_timeout() -> Duration {
    Duration::from_millis(crate::remote_control::CONTROLLER_REQUEST_TIMEOUT_MS)
}

fn consent_timeout() -> Duration {
    // The sharer's consent window plus a margin, mirroring the web surface
    // (REMOTE_CONTROL_CONSENT_TIMEOUT_MS in remoteControlFeedback.ts).
    crate::remote_control::CONSENT_TIMEOUT + Duration::from_secs(5)
}

struct NativeWindow {
    window: gtk::Window,
    area: gtk::DrawingArea,
    surface: Rc<RefCell<Option<gtk::cairo::ImageSurface>>>,
    title: String,
    controller: ControllerUi,
}

impl NativeWindow {
    fn new(feed: &LinuxReceiveFeed, key: WindowKey, pixels: &Pixels) -> Self {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_decorated(true);
        window.set_resizable(true);
        window.set_default_size(pixels.width.min(1000), pixels.height.min(700));

        let state = Rc::new(RefCell::new(ControllerState::default()));
        let controller = ControllerUi::new(state.clone(), key.clone());

        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.pack_start(&controller.button, false, false, 0);
        bar.pack_start(&controller.label, false, false, 0);

        let area = gtk::DrawingArea::new();
        if let Some(accessible) = area.accessible() {
            accessible.set_name("Remote shared window");
        }
        area.set_can_focus(true);
        area.add_events(
            gdk::EventMask::POINTER_MOTION_MASK
                | gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::BUTTON_RELEASE_MASK
                | gdk::EventMask::SCROLL_MASK
                | gdk::EventMask::SMOOTH_SCROLL_MASK
                | gdk::EventMask::KEY_PRESS_MASK
                | gdk::EventMask::KEY_RELEASE_MASK
                | gdk::EventMask::FOCUS_CHANGE_MASK,
        );

        let surface: Rc<RefCell<Option<gtk::cairo::ImageSurface>>> = Rc::new(RefCell::new(None));
        let draw_surface = surface.clone();
        let draw_state = state.clone();
        area.connect_draw(move |widget, context| {
            context.set_source_rgb(0.0, 0.0, 0.0);
            let _ = context.paint();
            let controlling = draw_state.borrow().mode() == ControllerMode::Active;
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
            if controlling {
                // Control-active cue: a bright border around the picture.
                let width = f64::from(widget.allocated_width());
                let height = f64::from(widget.allocated_height());
                context.set_source_rgb(0.98, 0.75, 0.18);
                context.set_line_width(3.0);
                context.rectangle(1.5, 1.5, width - 3.0, height - 3.0);
                let _ = context.stroke();
            }
            gtk::glib::Propagation::Proceed
        });

        Self::connect_input(&area, &controller);

        {
            let toggle_controller = controller.clone();
            controller.button.connect_clicked(move |_| {
                toggle_controller.on_toggle();
            });
        }

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.pack_start(&bar, false, false, 0);
        vbox.pack_start(&area, true, true, 0);
        window.add(&vbox);
        let slots = feed.slots.clone();
        window.connect_delete_event(move |_, _| {
            if let Some(slot) = slots.lock_unpoisoned().remove(&key) {
                slot.stop();
                // The viewer closed this share's window: its receive target
                // and any controller grant naming it retire with it.
                crate::linux_remote_control::retire_controller_for(&key.0, key.1);
            }
            gtk::glib::Propagation::Proceed
        });
        Self {
            window,
            area,
            surface,
            title: String::new(),
            controller,
        }
    }

    fn connect_input(area: &gtk::DrawingArea, controller: &ControllerUi) {
        let key = controller.key.clone();

        let press_controller = controller.clone();
        let press_area = area.clone();
        area.connect_button_press_event(move |_, event| {
            if press_controller.state.borrow().mode() != ControllerMode::Active {
                return gtk::glib::Propagation::Proceed;
            }
            press_area.grab_focus();
            let Some(dom) = dom_button(event.button()) else {
                return gtk::glib::Propagation::Stop;
            };
            let (area_w, area_h) = drawing_size(&press_area);
            let point = normalized_point(&press_controller, area_w, area_h, event.position());
            let Some((x, y)) = point else {
                return gtk::glib::Propagation::Stop;
            };
            let modifiers = dom_modifiers(event.state());
            let (buttons, click_count) = {
                let mut state = press_controller.state.borrow_mut();
                state.held_buttons |= dom_button_mask(dom) as u8;
                let click_count = state.clicks.press(dom as u8, event.time());
                (u16::from(state.held_buttons), click_count)
            };
            press_controller.flush_pending_move();
            press_controller.flush_pending_wheel();
            press_controller.send_pointer(
                RemoteControlAction::Down,
                x,
                y,
                dom,
                buttons,
                Some(click_count),
                modifiers,
            );
            gtk::glib::Propagation::Stop
        });

        let release_controller = controller.clone();
        let release_area = area.clone();
        area.connect_button_release_event(move |_, event| {
            if release_controller.state.borrow().mode() != ControllerMode::Active {
                return gtk::glib::Propagation::Proceed;
            }
            let Some(dom) = dom_button(event.button()) else {
                return gtk::glib::Propagation::Stop;
            };
            let (area_w, area_h) = drawing_size(&release_area);
            let point = normalized_point(&release_controller, area_w, area_h, event.position());
            let modifiers = dom_modifiers(event.state());
            let buttons = {
                let mut state = release_controller.state.borrow_mut();
                state.held_buttons &= !(dom_button_mask(dom) as u8);
                u16::from(state.held_buttons)
            };
            // A release outside the contained media rect still releases the
            // held button: reuse the last known pointer position rather than
            // stranding a press on the sharer (R03).
            let (x, y) = point.unwrap_or_else(|| {
                release_controller
                    .state
                    .borrow()
                    .pending_move
                    .as_ref()
                    .map(|pending| (pending.x, pending.y))
                    .unwrap_or((0.5, 0.5))
            });
            release_controller.send_pointer(
                RemoteControlAction::Up,
                x,
                y,
                dom,
                buttons,
                None,
                modifiers,
            );
            gtk::glib::Propagation::Stop
        });

        let motion_controller = controller.clone();
        let motion_area = area.clone();
        area.connect_motion_notify_event(move |_, event| {
            if motion_controller.state.borrow().mode() != ControllerMode::Active {
                return gtk::glib::Propagation::Proceed;
            }
            let (area_w, area_h) = drawing_size(&motion_area);
            let Some((x, y)) =
                normalized_point(&motion_controller, area_w, area_h, event.position())
            else {
                return gtk::glib::Propagation::Proceed;
            };
            let modifiers = dom_modifiers(event.state());
            let dom = current_dom_button(&motion_controller);
            {
                let mut state = motion_controller.state.borrow_mut();
                state.pending_move = Some(PendingMove {
                    x,
                    y,
                    button: dom,
                    buttons: u16::from(state.held_buttons),
                    alt: modifiers.alt,
                    ctrl: modifiers.ctrl,
                    meta: modifiers.meta,
                    shift: modifiers.shift,
                });
            }
            motion_controller.schedule_move_flush();
            gtk::glib::Propagation::Proceed
        });

        let scroll_controller = controller.clone();
        let scroll_area = area.clone();
        area.connect_scroll_event(move |_, event| {
            if scroll_controller.state.borrow().mode() != ControllerMode::Active {
                return gtk::glib::Propagation::Proceed;
            }
            let (area_w, area_h) = drawing_size(&scroll_area);
            let Some((x, y)) =
                normalized_point(&scroll_controller, area_w, area_h, event.position())
            else {
                return gtk::glib::Propagation::Proceed;
            };
            let modifiers = dom_modifiers(event.state());
            let (delta_x, delta_y) = match event.direction() {
                gdk::ScrollDirection::Up => (0.0, -1.0),
                gdk::ScrollDirection::Down => (0.0, 1.0),
                gdk::ScrollDirection::Left => (-1.0, 0.0),
                gdk::ScrollDirection::Right => (1.0, 0.0),
                _ => event.delta(),
            };
            {
                let mut state = scroll_controller.state.borrow_mut();
                let pending = state.pending_wheel.get_or_insert(PendingWheel {
                    x,
                    y,
                    delta_x: 0.0,
                    delta_y: 0.0,
                    modifiers: modifiers.clone(),
                });
                pending.x = x;
                pending.y = y;
                pending.delta_x += delta_x;
                pending.delta_y += delta_y;
                pending.modifiers = modifiers;
            }
            scroll_controller.schedule_wheel_flush();
            gtk::glib::Propagation::Stop
        });

        Self::connect_keys(area, controller);

        let focus_controller = controller.clone();
        area.connect_focus_out_event(move |_, _| {
            // Losing key focus must not leave the sharer with stuck input:
            // release everything we hold, keep the grant itself.
            let held_keys = {
                let mut state = focus_controller.state.borrow_mut();
                state.pending_move.take();
                state.pending_wheel.take();
                state.last_move_quantized = None;
                state.clipboard_modifier = None;
                state.clipboard_consumed = false;
                std::mem::take(&mut state.held_keys)
            };
            let held_buttons = {
                let mut state = focus_controller.state.borrow_mut();
                std::mem::take(&mut state.held_buttons)
            };
            if focus_controller.state.borrow().mode() == ControllerMode::Active {
                for held in held_keys {
                    focus_controller.send_key(
                        RemoteControlAction::Up,
                        &held,
                        false,
                        RemoteControlModifiers::default(),
                    );
                }
                for dom in 0..3i16 {
                    if held_buttons & dom_button_mask(dom) as u8 != 0 {
                        focus_controller.send_pointer(
                            RemoteControlAction::Up,
                            0.5,
                            0.5,
                            dom,
                            0,
                            None,
                            RemoteControlModifiers::default(),
                        );
                    }
                }
            }
            gtk::glib::Propagation::Proceed
        });
    }

    fn connect_keys(area: &gtk::DrawingArea, controller: &ControllerUi) {
        let down_controller = controller.clone();
        let down_area = area.clone();
        area.connect_key_press_event(move |_, event| {
            if down_controller.state.borrow().mode() != ControllerMode::Active {
                return gtk::glib::Propagation::Proceed;
            }
            down_area.grab_focus();
            let keyval_name = event
                .keyval()
                .name()
                .map(|name| name.to_string())
                .unwrap_or_default();
            let Some(fields) = crate::linux_remote_control::dom_key_fields(
                &keyval_name,
                event.keyval().to_unicode(),
            ) else {
                return gtk::glib::Propagation::Stop;
            };
            let held = HeldKey {
                key: fields.key.clone(),
                code: fields.code.clone(),
                location: fields.location,
            };
            let modifiers = dom_modifiers(event.state());

            // Clipboard chord: hold Ctrl, then c/v performs the remote
            // clipboard operation instead of a key press (R04).
            if fields.code == "ControlLeft" || fields.code == "ControlRight" {
                let mut state = down_controller.state.borrow_mut();
                if state.held_keys.iter().any(|k| k.code == held.code) {
                    return gtk::glib::Propagation::Stop;
                }
                state.clipboard_modifier = Some(held.clone());
                drop(state);
                down_controller.send_key(RemoteControlAction::Down, &held, false, modifiers);
                let mut state = down_controller.state.borrow_mut();
                state.held_keys.push(held);
                return gtk::glib::Propagation::Stop;
            }
            let is_clipboard_chord =
                matches!(fields.code.as_str(), "KeyC" | "KeyV") && modifiers.ctrl;
            if is_clipboard_chord {
                let operation = if fields.code == "KeyC" {
                    "copy"
                } else {
                    "paste"
                };
                let mut state = down_controller.state.borrow_mut();
                state.clipboard_consumed = true;
                state.clipboard_shortcut_code = Some(fields.code.clone());
                drop(state);
                down_controller.flush_pending_move();
                down_controller.flush_pending_wheel();
                down_controller.invoke_clipboard(operation);
                return gtk::glib::Propagation::Stop;
            }
            let mut state = down_controller.state.borrow_mut();
            if let Some(pending) = state.clipboard_modifier.clone() {
                // An ordinary shortcut: the held Ctrl down has already been
                // sent when it was pressed, so nothing to flush.
                let _ = pending;
                state.clipboard_modifier = None;
            }
            let repeat = state.held_keys.iter().any(|k| k == &held);
            if !repeat {
                state.held_keys.push(held.clone());
            }
            drop(state);
            down_controller.send_key(RemoteControlAction::Down, &held, repeat, modifiers);
            gtk::glib::Propagation::Stop
        });

        let up_controller = controller.clone();
        area.connect_key_release_event(move |_, event| {
            if up_controller.state.borrow().mode() != ControllerMode::Active {
                return gtk::glib::Propagation::Proceed;
            }
            let keyval_name = event
                .keyval()
                .name()
                .map(|name| name.to_string())
                .unwrap_or_default();
            let Some(fields) = crate::linux_remote_control::dom_key_fields(
                &keyval_name,
                event.keyval().to_unicode(),
            ) else {
                return gtk::glib::Propagation::Stop;
            };
            let held = HeldKey {
                key: fields.key,
                code: fields.code,
                location: fields.location,
            };
            let modifiers = dom_modifiers(event.state());
            let mut state = up_controller.state.borrow_mut();
            if let Some(shortcut) = state.clipboard_shortcut_code.clone() {
                if shortcut == held.code {
                    state.clipboard_shortcut_code = None;
                    return gtk::glib::Propagation::Stop;
                }
            }
            if held.code == "ControlLeft" || held.code == "ControlRight" {
                state.clipboard_consumed = false;
                state.clipboard_modifier = None;
            }
            let still_held = state.held_keys.iter().any(|k| k == &held);
            state.held_keys.retain(|k| k != &held);
            drop(state);
            if still_held {
                up_controller.send_key(RemoteControlAction::Up, &held, false, modifiers);
            }
            gtk::glib::Propagation::Stop
        });
    }
}

fn drawing_size(area: &gtk::DrawingArea) -> (f64, f64) {
    (
        f64::from(area.allocated_width()),
        f64::from(area.allocated_height()),
    )
}

fn current_dom_button(controller: &ControllerUi) -> i16 {
    let state = controller.state.borrow();
    for dom in 0..3i16 {
        if state.held_buttons & dom_button_mask(dom) as u8 != 0 {
            return dom;
        }
    }
    0
}

fn normalized_point(
    controller: &ControllerUi,
    area_width: f64,
    area_height: f64,
    position: (f64, f64),
) -> Option<(f64, f64)> {
    let source = controller.state.borrow().source_size?;
    crate::linux_remote_control::normalized_control_point(
        area_width,
        area_height,
        source.0,
        source.1,
        position,
    )
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
        drain_controller_commands(&mut windows);
        slots.clear();
        slots.extend(feed.slots.lock_unpoisoned().values().cloned());
        windows.retain(|key, native| {
            let keep = slots.iter().any(|slot| &slot.key == key);
            if !keep {
                native.controller.deactivate("Share ended.");
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
                        if let Ok(mut controller_state) = native.controller.state.try_borrow_mut() {
                            controller_state.source_size = Some(decoded_size);
                        }
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

fn drain_controller_commands(windows: &mut HashMap<WindowKey, NativeWindow>) {
    let mut commands = CONTROLLER_COMMANDS.lock_unpoisoned();
    while let Some(command) = commands.pop_front() {
        match command {
            ControllerCommand::Status {
                key,
                request_id,
                status,
                message,
            } => {
                if !crate::linux_remote_control::controller_status_is_current(&key.0, key.1, &request_id) {
                    continue;
                }
                if let Some(native) = windows.get(&key) {
                    native.controller.apply_status(&status, &message, Some(&request_id));
                }
            }
            ControllerCommand::RequestFailed {
                key,
                reservation,
                message,
            } => {
                if crate::linux_remote_control::control_request_is_current(
                    &key.0,
                    key.1,
                    reservation,
                ) {
                    if let Some(native) = windows.get(&key) {
                        native.controller.apply_status("requestFailed", &message, None);
                    }
                }
            }
            ControllerCommand::SetActive { key, active } => {
                if let Some(native) = windows.get(&key) {
                    native.controller.apply_active(active);
                }
            }
        }
    }
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
        // feed.stop()/retire paths mutate the shared controller registry;
        // serialize with the other registry tests.
        let _guard = crate::remote_control_core::test_support::state_test_lock();
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
