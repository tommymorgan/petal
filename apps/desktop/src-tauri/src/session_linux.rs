//! Linux room membership on the shared native token/LiveKit connection.
//!
//! This adapter owns membership and presence; the ordered media tasks attach
//! microphone, playout and incoming-share resources to this same connection.
//! Screen/camera publication and remote input remain unavailable on Linux.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::camera_session::ActiveCamera;
use crate::platform::cg::WindowFrame;
use crate::remote_control_core::RemoteControlPolicy;
pub(crate) use crate::room_generation::RoomGeneration;
use crate::sync_ext::MutexExt;
use crate::transport::publisher::RoomConnection;

use tauri::Manager;

struct JoinedRoom {
    record: crate::rooms::RoomRecord,
    connection: Arc<RoomConnection>,
    identity: String,
    presence: Arc<crate::presence::PresenceState>,
    receiver: crate::linux_compositor::LinuxReceiveFeed,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SharedWindowScreenStatus {
    NotShared,
    OnScreen(WindowFrame),
    OffScreen,
    Closed,
}

#[derive(Default)]
pub struct SessionState {
    joined: Mutex<Option<JoinedRoom>>,
    transition_lock: tokio::sync::Mutex<()>,
    mic_control: tokio::sync::Mutex<()>,
    camera_control: tokio::sync::Mutex<()>,
    generation: Arc<AtomicU64>,
    camera_intent: AtomicBool,
    camera_healing: AtomicBool,
}

impl SessionState {
    pub(crate) fn begin_room_generation(&self) -> RoomGeneration {
        let value = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        RoomGeneration::new(self.generation.clone(), value)
    }

    pub(crate) fn current_room_generation(&self) -> RoomGeneration {
        RoomGeneration::new(
            self.generation.clone(),
            self.generation.load(Ordering::SeqCst),
        )
    }

    pub(crate) fn invalidate_room_generation(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn is_in_room(&self) -> bool {
        self.joined.lock_unpoisoned().is_some()
    }

    pub(crate) fn control_channel_snapshot(&self) -> Option<(Arc<RoomConnection>, String)> {
        self.joined
            .lock_unpoisoned()
            .as_ref()
            .map(|joined| (joined.connection.clone(), joined.identity.clone()))
    }

    pub(crate) fn current_room_record(&self) -> Option<crate::rooms::RoomRecord> {
        self.joined
            .lock_unpoisoned()
            .as_ref()
            .map(|joined| joined.record.clone())
    }

    pub(crate) fn remote_window_summaries(&self) -> Vec<crate::linux_compositor::RemoteWindowSummary> {
        self.joined.lock_unpoisoned().as_ref()
            .map(|joined| joined.receiver.summaries())
            .unwrap_or_default()
    }

    pub(crate) fn is_share_active(&self, _window_id: u32) -> bool {
        false
    }

    pub(crate) fn is_display_share(&self, _window_id: u32) -> bool {
        false
    }

    pub(crate) fn active_share_frame(&self, _window_id: u32) -> Option<WindowFrame> {
        None
    }

    pub(crate) fn active_share_pid(&self, _window_id: u32) -> Option<i32> {
        None
    }

    pub(crate) fn active_share_source_title(&self, _window_id: u32) -> Option<String> {
        None
    }

    pub(crate) fn shared_window_screen_status(&self, _window_id: u32) -> SharedWindowScreenStatus {
        SharedWindowScreenStatus::NotShared
    }

    pub(crate) fn remote_control_allowed(&self) -> bool {
        false
    }

    pub(crate) fn remote_control_policy(&self) -> RemoteControlPolicy {
        RemoteControlPolicy::Off
    }

    pub(crate) fn share_allows_remote_control(&self, _window_id: u32) -> bool {
        false
    }

    pub(crate) async fn lock_mic_control(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.mic_control.lock().await
    }

    pub(crate) fn mic_muted(&self) -> bool {
        true
    }

    pub(crate) fn set_mic_muted(&self, _muted: bool) -> Result<bool, String> {
        Err("Linux microphone session is not available".to_string())
    }

    pub(crate) fn set_ai_chat_ducking(&self, _duck: bool) {
        log::warn!("Linux microphone session is not available for AI chat ducking");
    }

    pub(crate) async fn set_audio_devices(
        &self,
        recording_id: Option<String>,
        playout_id: Option<String>,
        preferences: &crate::transport::audio::AudioDevicePreferences,
    ) -> crate::transport::audio::AppliedAudioDevices {
        // Persist device intent before any room exists, as on the native peers.
        if let Some(id) = recording_id {
            preferences.set_recording_device(id);
        }
        if let Some(id) = playout_id {
            preferences.set_playout_device(id);
        }
        crate::transport::audio::AppliedAudioDevices::default()
    }

    pub(crate) async fn lock_camera_control(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.camera_control.lock().await
    }

    pub(crate) fn camera_publishing(&self) -> bool {
        false
    }

    pub(crate) fn camera_intent(&self) -> bool {
        self.camera_intent.load(Ordering::SeqCst)
    }

    pub(crate) fn set_camera_intent(&self, enabled: bool) {
        self.camera_intent.store(enabled, Ordering::SeqCst);
    }

    pub(crate) fn try_begin_camera_heal(&self) -> bool {
        self.camera_healing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub(crate) fn end_camera_heal(&self) {
        self.camera_healing.store(false, Ordering::SeqCst);
    }

    pub(crate) fn take_active_camera(&self) -> Option<ActiveCamera> {
        None
    }

    pub(crate) fn put_active_camera(&self, camera: &mut Option<ActiveCamera>) -> bool {
        // Never consume a capture without a live room; the caller owns cleanup.
        camera.is_none()
    }

    pub(crate) fn take_active_camera_matching(
        &self,
        _status: &crate::transport::camera::CameraStatus,
    ) -> Option<ActiveCamera> {
        None
    }
}

#[tauri::command]
pub(crate) async fn join_room_command(
    app: tauri::AppHandle,
    rooms: tauri::State<'_, crate::rooms::RoomsState>,
    state: tauri::State<'_, SessionState>,
    room_name: String,
    identity: String,
    display_name: String,
    remote_control_allowed: bool,
    remote_control_policy: Option<RemoteControlPolicy>,
    identity_palette_index: Option<u8>,
) -> Result<crate::rooms::RoomRecord, crate::meeting_core::RoomJoinError> {
    // Accept the shared wire payload, never its unsupported control intent.
    let _ = (remote_control_allowed, remote_control_policy);
    let _transition = state.transition_lock.lock().await;
    let record = crate::meeting_core::persist_joined_room_record(&rooms, &room_name)?;
    {
        let joined = state.joined.lock_unpoisoned();
        if let Some(joined) = joined.as_ref() {
            if joined.record.id == record.id
                && joined.connection.room().connection_state()
                    != livekit::ConnectionState::Disconnected
            {
                return Ok(joined.record.clone());
            }
        }
    }
    leave_room_inner(&app, &state, true).await;
    let generation = state.begin_room_generation();
    let connected =
        match crate::meeting_core::connect_room(&rooms, record, &identity, &display_name).await {
            Ok(connected) => connected,
            Err(error) => {
                state.invalidate_room_generation();
                crate::ai_chat::room_auth::forget();
                return Err(error);
            }
        };
    let record = connected.room_record;
    let connection = connected.room_connection;
    let presence = Arc::new(crate::presence::PresenceState::default());
    // The same original connect-time queue serves native media and disconnect.
    let events = connection.take_compositor_events();
    connection.discard_resilience_events();
    let receiver = match crate::linux_compositor::LinuxReceiveFeed::new(
        &app, generation.clone(), identity.clone(),
    ) {
        Ok(receiver) => receiver,
        Err(error) => {
            state.invalidate_room_generation();
            connection.room().close().await.ok();
            return Err(crate::meeting_core::RoomJoinError::RoomConnect(error.to_string()));
        }
    };
    {
        *state.joined.lock_unpoisoned() = Some(JoinedRoom {
            record: record.clone(),
            connection: connection.clone(),
            identity: identity.clone(),
            presence: presence.clone(),
            receiver: receiver.clone(),
        });
    }
    crate::presence::start_for_room(
        &app,
        connection.room(),
        presence,
        record.name.clone(),
        identity.clone(),
        display_name,
        generation.clone(),
    );
    if let Some(mut events) = events {
        let app = app.clone();
        let connection = connection.clone();
        let generation = generation.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let event = tokio::select! {
                    biased;
                    _ = receiver.cancelled() => break,
                    event = events.recv() => event,
                };
                let Some(event) = event else { break };
                if !generation.is_current() {
                    break;
                }
                if matches!(event, livekit::RoomEvent::Disconnected { .. }) {
                    if let Some(state) = app.try_state::<SessionState>() {
                        let _transition = state.transition_lock.lock().await;
                        let owns_room = state
                            .joined
                            .lock_unpoisoned()
                            .as_ref()
                            .map(|joined| Arc::ptr_eq(&joined.connection, &connection))
                            .unwrap_or(false);
                        if generation.is_current() && owns_room {
                            leave_room_inner(&app, &state, false).await;
                        }
                    }
                    break;
                }
                receiver.handle_event(event);
            }
            receiver.stop();
        });
    }
    match connection
        .publish_identity_palette_index(identity_palette_index)
        .await
    {
        Ok(palette_index) if generation.is_current() => {
            connection.commit_identity_palette_index(palette_index);
        }
        Ok(_) => {}
        Err(error) => log::warn!("linux session: identity color metadata failed: {error}"),
    }
    if connection.room().connection_state() == livekit::ConnectionState::Disconnected {
        leave_room_inner(&app, &state, false).await;
        return Err(crate::meeting_core::RoomJoinError::RoomConnect(
            "room disconnected during join".to_string(),
        ));
    }
    crate::analytics::meeting_joined();
    Ok(record)
}

#[tauri::command]
pub fn current_room(state: tauri::State<'_, SessionState>) -> Option<String> {
    state.current_room_record().map(|record| record.name)
}

#[tauri::command]
pub fn room_presence(
    state: tauri::State<'_, SessionState>,
) -> Vec<crate::presence::PresentParticipant> {
    state
        .joined
        .lock_unpoisoned()
        .as_ref()
        .map(|joined| joined.presence.snapshot())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn leave_room_command(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
) -> Result<(), ()> {
    leave_room(&app, &state).await;
    Ok(())
}

pub async fn leave_room(app: &tauri::AppHandle, state: &SessionState) {
    let _transition = state.transition_lock.lock().await;
    leave_room_inner(app, state, true).await;
}

async fn leave_room_inner(app: &tauri::AppHandle, state: &SessionState, close: bool) {
    state.invalidate_room_generation();
    state.set_camera_intent(false);
    crate::ai_chat::room_auth::forget();
    let joined = state.joined.lock_unpoisoned().take();
    if let Some(joined) = joined {
        joined.receiver.stop();
        if close {
            if let Err(error) = joined.connection.room().close().await {
                log::warn!("linux session: room close failed: {error}");
            }
        }
        let _ = tauri::Emitter::emit(
            app,
            "room-left",
            serde_json::json!({ "roomName": joined.record.name }),
        );
        crate::menubar::update_meeting_state(app, false, 0);
        crate::analytics::meeting_left();
    }
}

#[tauri::command]
pub async fn set_display_name(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
    name: String,
) -> Result<(), String> {
    let snapshot = state.joined.lock_unpoisoned().as_ref().map(|joined| {
        (
            joined.connection.clone(),
            joined.presence.clone(),
            joined.record.name.clone(),
        )
    });
    let Some((connection, presence, room_name)) = snapshot else {
        return Ok(());
    };
    let name = name.trim();
    let display = if name.is_empty() { "Guest" } else { name };
    connection
        .room()
        .local_participant()
        .set_name(display.to_string())
        .await
        .map_err(|error| format!("set_name failed: {error}"))?;
    crate::presence::set_local_name(&app, &presence, &room_name, display);
    Ok(())
}

pub(crate) fn share_audio_state_for_state(
    _state: &SessionState,
    window_id: u32,
) -> crate::screen_audio::ShareAudioState {
    crate::screen_audio::ShareAudioState::inactive(window_id)
}

pub(crate) async fn set_share_audio_enabled_for_state(
    _app: &tauri::AppHandle,
    _state: &SessionState,
    _window_id: u32,
    _enabled: bool,
) -> Result<crate::screen_audio::ShareAudioState, String> {
    Err("Screen sharing is unsupported on Linux".to_string())
}
