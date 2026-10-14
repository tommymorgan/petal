//! Linux startup's disconnected session boundary.
//!
//! Native room connection, audio and incoming-share ownership are delivered by
//! their ordered tasks. This boundary cannot join, publish, or inject input;
//! shared read-only consumers observe the actual empty session at launch.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::camera_session::ActiveCamera;
use crate::platform::cg::WindowFrame;
use crate::remote_control_core::RemoteControlPolicy;
pub(crate) use crate::room_generation::RoomGeneration;
use crate::transport::publisher::RoomConnection;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SharedWindowScreenStatus {
    NotShared,
    OnScreen(WindowFrame),
    OffScreen,
    Closed,
}

#[derive(Default)]
pub struct SessionState {
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
        false
    }

    pub(crate) fn control_channel_snapshot(&self) -> Option<(Arc<RoomConnection>, String)> {
        None
    }

    pub(crate) fn current_room_record(&self) -> Option<crate::rooms::RoomRecord> {
        None
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
pub fn current_room() -> Option<String> {
    None
}

#[tauri::command]
pub fn room_presence() -> Vec<crate::presence::PresentParticipant> {
    Vec::new()
}

pub async fn leave_room(_app: &tauri::AppHandle, state: &SessionState) {
    // Invalidates any pending shared camera operation before process teardown.
    state.invalidate_room_generation();
    state.set_camera_intent(false);
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
