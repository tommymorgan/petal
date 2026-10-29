//! Linux room membership on the shared native token/LiveKit connection.
//!
//! This adapter owns membership and presence; the ordered media tasks attach
//! microphone, playout and incoming-share resources to this same connection.
//! Monitor publication attaches to the same connection; camera/input remain unavailable.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::camera_session::ActiveCamera;
use crate::platform::cg::WindowFrame;
use crate::remote_control_core::RemoteControlPolicy;
pub(crate) use crate::room_generation::RoomGeneration;
use crate::sync_ext::MutexExt;
use crate::transport::audio::{MicTrack, SpeakerPlayout};
use crate::transport::publisher::RoomConnection;

const AUDIO_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const AUDIO_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

use tauri::Manager;

struct JoinedRoom {
    record: crate::rooms::RoomRecord,
    connection: Arc<RoomConnection>,
    generation: RoomGeneration,
    identity: String,
    presence: Arc<crate::presence::PresenceState>,
    receiver: crate::linux_compositor::LinuxReceiveFeed,
    microphone: Option<Arc<MicTrack>>,
    playout: Option<Arc<SpeakerPlayout>>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SharedWindowScreenStatus {
    NotShared,
    OnScreen(WindowFrame),
    OffScreen,
    Closed,
}

#[derive(Default)]
struct CameraPermissionState {
    retirement: u64,
    request: Option<Arc<CancellationToken>>,
}

/// Pending access belongs to this publication attempt, not Settings consumers.
pub(crate) struct CameraPermissionRequest<'a> {
    pending: &'a Mutex<CameraPermissionState>,
    cancel: Arc<CancellationToken>,
}

impl CameraPermissionRequest<'_> {
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
}

impl Drop for CameraPermissionRequest<'_> {
    fn drop(&mut self) {
        let mut pending = self.pending.lock_unpoisoned();
        if pending.request.as_ref().is_some_and(|cancel| Arc::ptr_eq(cancel, &self.cancel)) {
            pending.request.take();
        }
    }
}

pub struct SessionState {
    joined: Mutex<Option<JoinedRoom>>,
    transition_lock: tokio::sync::Mutex<()>,
    mic_control: tokio::sync::Mutex<()>,
    audio_control: Arc<tokio::sync::Mutex<()>>,
    camera_control: tokio::sync::Mutex<()>,
    generation: Arc<AtomicU64>,
    desired_mic_muted: AtomicBool,
    camera_intent: AtomicBool,
    camera_permission: Mutex<CameraPermissionState>,
    camera_healing: AtomicBool,
    /// The application-scoped camera owner (ADR 0003): one low-level
    /// capture shared by publication and Settings-preview leases.
    pub(crate) cameras: Arc<crate::linux_camera::LinuxCameraOwner>,
    /// The published local camera, if the Video control turned it on.
    /// Torn down by stop_camera_publish and by leave_room.
    camera: Mutex<Option<crate::camera_session::ActiveCamera>>,
    pub(crate) pictures: Arc<crate::linux_share::PictureOwner>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            joined: Mutex::new(None),
            transition_lock: tokio::sync::Mutex::new(()),
            mic_control: tokio::sync::Mutex::new(()),
            audio_control: Arc::new(tokio::sync::Mutex::new(())),
            camera_control: tokio::sync::Mutex::new(()),
            generation: Arc::new(AtomicU64::new(0)),
            desired_mic_muted: AtomicBool::new(true),
            camera_intent: AtomicBool::new(false),
            camera_permission: Mutex::new(CameraPermissionState::default()),
            camera_healing: AtomicBool::new(false),
            cameras: crate::linux_camera::global_owner(),
            camera: Mutex::new(None),
            pictures: Arc::new(crate::linux_share::PictureOwner::default()),
        }
    }
}

impl SessionState {
    pub(crate) fn begin_room_generation(&self) -> RoomGeneration {
        let mut pending = self.camera_permission.lock_unpoisoned();
        let value = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(cancel) = pending.request.take() {
            cancel.cancel();
        }
        RoomGeneration::new(self.generation.clone(), value)
    }

    pub(crate) fn current_room_generation(&self) -> RoomGeneration {
        RoomGeneration::new(
            self.generation.clone(),
            self.generation.load(Ordering::SeqCst),
        )
    }

    pub(crate) fn invalidate_room_generation(&self) {
        {
            let mut pending = self.camera_permission.lock_unpoisoned();
            self.generation.fetch_add(1, Ordering::SeqCst);
            if let Some(cancel) = pending.request.take() {
                cancel.cancel();
            }
        }
        self.pictures.cancel();
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

    pub(crate) fn picture_connection_snapshot(
        &self,
    ) -> Option<(Arc<RoomConnection>, RoomGeneration)> {
        self.joined
            .lock_unpoisoned()
            .as_ref()
            .filter(|joined| joined.generation.is_current())
            .map(|joined| (joined.connection.clone(), joined.generation.clone()))
    }

    pub(crate) fn current_room_record(&self) -> Option<crate::rooms::RoomRecord> {
        self.joined
            .lock_unpoisoned()
            .as_ref()
            .map(|joined| joined.record.clone())
    }

    pub(crate) fn remote_window_summaries(
        &self,
    ) -> Vec<crate::linux_compositor::RemoteWindowSummary> {
        self.joined
            .lock_unpoisoned()
            .as_ref()
            .map(|joined| joined.receiver.summaries())
            .unwrap_or_default()
    }

    pub(crate) fn is_share_active(&self, window_id: u32) -> bool {
        self.pictures.frame(window_id).is_some()
    }

    pub(crate) fn is_display_share(&self, window_id: u32) -> bool {
        self.pictures.source_kind(window_id)
            == Some(crate::transport::publisher::SharedSourceKind::Display)
    }

    pub(crate) fn active_share_frame(&self, window_id: u32) -> Option<WindowFrame> {
        self.pictures.frame(window_id)
    }

    pub(crate) fn active_share_pid(&self, _window_id: u32) -> Option<i32> {
        None
    }

    pub(crate) fn active_share_source_title(&self, window_id: u32) -> Option<String> {
        self.pictures.source_kind(window_id).map(|kind| {
            match kind {
                crate::transport::publisher::SharedSourceKind::Display => "Selected monitor",
                _ => "Selected window",
            }
            .to_string()
        })
    }

    pub(crate) fn shared_window_screen_status(&self, window_id: u32) -> SharedWindowScreenStatus {
        self.pictures
            .frame(window_id)
            .map(SharedWindowScreenStatus::OnScreen)
            .unwrap_or(SharedWindowScreenStatus::NotShared)
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
        self.desired_mic_muted.load(Ordering::SeqCst)
    }

    pub(crate) fn set_mic_muted(&self, muted: bool) -> Result<bool, String> {
        let joined = self.joined.lock_unpoisoned();
        let microphone = joined
            .as_ref()
            .and_then(|joined| joined.microphone.as_ref())
            .ok_or_else(|| "microphone unavailable".to_string())?;
        microphone.set_muted(muted);
        let reached = microphone.is_muted();
        if reached != muted {
            return Err("microphone did not reach requested mute state".to_string());
        }
        self.desired_mic_muted.store(reached, Ordering::SeqCst);
        Ok(reached)
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
        let audio_control = Arc::new(self.audio_control.clone().lock_owned().await);
        let recording_requested = recording_id.is_some();
        let playout_requested = playout_id.is_some();
        if let Some(id) = recording_id {
            preferences.set_recording_device(id);
        }
        if let Some(id) = playout_id {
            preferences.set_playout_device(id);
        }
        let connection = self
            .joined
            .lock_unpoisoned()
            .as_ref()
            .map(|joined| joined.connection.clone());
        let Some(connection) = connection else {
            return crate::transport::audio::AppliedAudioDevices::default();
        };
        apply_room_audio(
            self,
            connection,
            self.current_room_generation(),
            (preferences.recording_device(), preferences.playout_device()),
            recording_requested,
            playout_requested,
            audio_control,
        )
        .await
    }

    pub(crate) async fn lock_camera_control(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.camera_control.lock().await
    }

    pub(crate) async fn lock_camera_start(
        &self,
    ) -> Option<tokio::sync::MutexGuard<'_, ()>> {
        // Snapshot before waiting behind Settings/capture work. A newer Off
        // or room leave must retire even an On that has no permission token yet.
        let (generation, retirement) = {
            let pending = self.camera_permission.lock_unpoisoned();
            (self.current_room_generation(), pending.retirement)
        };
        let control = self.lock_camera_control().await;
        {
            let pending = self.camera_permission.lock_unpoisoned();
            if !generation.is_current() || retirement != pending.retirement {
                return None;
            }
            self.camera_intent.store(true, Ordering::SeqCst);
        }
        Some(control)
    }

    pub(crate) fn camera_publishing(&self) -> bool {
        self.camera.lock_unpoisoned().is_some()
    }

    pub(crate) fn camera_intent(&self) -> bool {
        self.camera_intent.load(Ordering::SeqCst)
    }

    pub(crate) fn set_camera_intent(&self, enabled: bool) {
        // Admission and retirement use the same lock: a request either joins
        // before Off and is cancelled, or observes Off and cannot prompt.
        let mut pending = self.camera_permission.lock_unpoisoned();
        self.camera_intent.store(enabled, Ordering::SeqCst);
        if !enabled {
            pending.retirement = pending.retirement.wrapping_add(1);
            if let Some(cancel) = pending.request.take() {
                cancel.cancel();
            }
        }
    }

    /// The caller MUST hold camera_control and own the failed start/capture.
    /// Cleanup is not a new user Off: advancing retirement here would reject
    /// an explicit On queued after Off but before this old failure settled.
    pub(crate) fn clear_camera_start_failure(&self) {
        let mut pending = self.camera_permission.lock_unpoisoned();
        self.camera_intent.store(false, Ordering::SeqCst);
        if let Some(cancel) = pending.request.take() {
            cancel.cancel();
        }
    }

    pub(crate) fn begin_camera_permission_request(
        &self,
        generation: &RoomGeneration,
    ) -> Result<CameraPermissionRequest<'_>, String> {
        let mut pending = self.camera_permission.lock_unpoisoned();
        if !generation.is_current() || !self.camera_intent() {
            return Err("Camera permission cancelled; the camera stays off".to_string());
        }
        let cancel = Arc::new(CancellationToken::new());
        if let Some(previous) = pending.request.replace(cancel.clone()) {
            previous.cancel();
        }
        Ok(CameraPermissionRequest { pending: &self.camera_permission, cancel })
    }

    pub(crate) fn try_begin_camera_heal(&self) -> bool {
        self.camera_healing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub(crate) fn end_camera_heal(&self) {
        self.camera_healing.store(false, Ordering::SeqCst);
    }

    pub(crate) fn take_active_camera(&self) -> Option<crate::camera_session::ActiveCamera> {
        self.camera.lock_unpoisoned().take()
    }

    pub(crate) fn put_active_camera(
        &self,
        camera: &mut Option<crate::camera_session::ActiveCamera>,
        generation: &RoomGeneration,
    ) -> bool {
        if !self.is_in_room() {
            return false;
        }
        // Leave invalidates the generation before taking this same lock. It
        // therefore either takes the adopted capture or makes adoption fail.
        let mut current = self.camera.lock_unpoisoned();
        if !generation.is_current()
            || !self.camera_intent()
            || camera.is_none()
            || current.is_some()
        {
            return false;
        }
        *current = camera.take();
        true
    }

    pub(crate) fn take_active_camera_matching(
        &self,
        status: &crate::transport::camera::CameraStatus,
    ) -> Option<crate::camera_session::ActiveCamera> {
        let mut camera = self.camera.lock_unpoisoned();
        if camera
            .as_ref()
            .is_some_and(|active| status.same_capture(&active.status))
        {
            return camera.take();
        }
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
    // Both consumers use the original connect-time fanout.
    let events = connection.take_compositor_events();
    let resilience_events = connection
        .take_resilience_events()
        .expect("RoomConnection resilience event receiver already taken");
    let receiver = match crate::linux_compositor::LinuxReceiveFeed::new(
        &app,
        generation.clone(),
        identity.clone(),
    ) {
        Ok(receiver) => receiver,
        Err(error) => {
            state.invalidate_room_generation();
            connection.room().close().await.ok();
            return Err(crate::meeting_core::RoomJoinError::RoomConnect(
                error.to_string(),
            ));
        }
    };
    {
        *state.joined.lock_unpoisoned() = Some(JoinedRoom {
            record: record.clone(),
            connection: connection.clone(),
            generation: generation.clone(),
            identity: identity.clone(),
            presence: presence.clone(),
            receiver: receiver.clone(),
            microphone: None,
            playout: None,
        });
    }
    crate::resilience::start_for_room(&app, resilience_events, generation.clone());
    // Controller-role remote control receiver: Status/Result packets from a
    // Windows sharer (and clipboard copy responses) arrive on this
    // subscription; host-role input stays refused inside handle_message.
    crate::remote_control::start_receiver_for_room(
        &app,
        connection.room(),
        identity.clone(),
        generation.clone(),
    );
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
                    // The shared resilience watcher owns terminal room cleanup.
                    // This branch only stops the native presentation consumer.
                    break;
                }
                receiver.handle_event(event);
            }
            receiver.stop();
        });
    }
    {
        let app = app.clone();
        let connection = connection.clone();
        let generation = generation.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<SessionState>();
            let audio_control = Arc::new(state.audio_control.clone().lock_owned().await);
            let preferences = app.state::<crate::transport::audio::AudioDevicePreferences>();
            let recording_device = preferences.recording_device();
            let playout_device = preferences.playout_device();
            let result = apply_room_audio(
                &state,
                connection,
                generation.clone(),
                (recording_device, playout_device),
                true,
                true,
                audio_control,
            )
            .await;
            if generation.is_current() {
                let event = match (result.mic_error, result.speaker_error) {
                    (Some(mic), Some(speaker)) => {
                        Some(crate::resilience_event::ResilienceEvent::MicDeviceFailed {
                            message: format!("{mic} {speaker}"),
                        })
                    }
                    (Some(message), None) => {
                        Some(crate::resilience_event::ResilienceEvent::MicDeviceFailed { message })
                    }
                    (None, Some(message)) => Some(
                        crate::resilience_event::ResilienceEvent::SpeakerDeviceFailed { message },
                    ),
                    (None, None) => None,
                };
                if let Some(event) = event {
                    let _ = tauri::Emitter::emit(&app, "resilience-event", event);
                }
            }
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

/// Initialize missing directions or switch their retained ADM handles in place.
/// The caller serializes startup/retry; Leave invalidates ownership immediately.
async fn apply_room_audio(
    state: &SessionState,
    connection: Arc<RoomConnection>,
    generation: RoomGeneration,
    devices: (Option<String>, Option<String>),
    recording_requested: bool,
    playout_requested: bool,
    audio_control: Arc<tokio::sync::OwnedMutexGuard<()>>,
) -> crate::transport::audio::AppliedAudioDevices {
    let (recording_device, playout_device) = devices;
    let mut result = crate::transport::audio::AppliedAudioDevices {
        in_room: true,
        ..Default::default()
    };
    if crate::transport::audio::audio_disabled_by_env() {
        result.mic_error =
            recording_requested.then(|| "Microphone disabled by PETAL_DISABLE_AUDIO".into());
        result.speaker_error =
            playout_requested.then(|| "Speakers disabled by PETAL_DISABLE_AUDIO".into());
        return result;
    }
    let (microphone, playout) = {
        let joined = state.joined.lock_unpoisoned();
        let Some(joined) = joined.as_ref().filter(|joined| {
            generation.is_current() && Arc::ptr_eq(&joined.connection, &connection)
        }) else {
            return crate::transport::audio::AppliedAudioDevices::default();
        };
        (joined.microphone.clone(), joined.playout.clone())
    };
    if playout_requested {
        let native_control = audio_control.clone();
        let native_generation = generation.clone();
        let work = tokio::task::spawn_blocking(move || -> Result<Arc<SpeakerPlayout>, String> {
            // A timeout cannot cancel native work: keep the permit until the
            // actual operation exits, including after the async caller leaves.
            let _control = native_control;
            if !native_generation.is_current() {
                return Err("room changed before speaker preparation".into());
            }
            if let Some(playout) = playout {
                match playout_device {
                    Some(id) => playout.set_playout_device(&id)?,
                    None => playout.use_default_playout_device()?,
                };
                Ok(playout)
            } else {
                crate::transport::audio::enable_managed_playout(playout_device)
                    .map(Arc::new)
                    .map_err(|error| error.to_string())
            }
        });
        let prepared = match tokio::time::timeout(AUDIO_START_TIMEOUT, work).await {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => {
                result.speaker_error = Some(
                    "Speaker initialization is still finishing. Retry after it completes.".into(),
                );
                result.mic_error = recording_requested.then(|| {
                    "Audio device initialization is still finishing. Retry after it completes."
                        .into()
                });
                return result;
            }
        };
        match prepared {
            Ok(playout) => {
                let mut joined = state.joined.lock_unpoisoned();
                if let Some(joined) = joined.as_mut().filter(|joined| {
                    generation.is_current() && Arc::ptr_eq(&joined.connection, &connection)
                }) {
                    joined.playout = Some(playout);
                    result.speaker_applied = true;
                }
            }
            Err(error) => {
                result.speaker_error = Some(format!(
                    "Speaker unavailable: {error}. Connect or select an output device and retry."
                ))
            }
        }
    }
    if !generation.is_current() {
        return crate::transport::audio::AppliedAudioDevices::default();
    }
    if recording_requested {
        if let Some(microphone) = microphone {
            let native_control = audio_control.clone();
            let native_generation = generation.clone();
            let work = tokio::task::spawn_blocking(move || {
                let _control = native_control;
                if !native_generation.is_current() {
                    return Err("room changed before microphone switch".into());
                }
                match recording_device {
                    Some(id) => microphone.set_recording_device(&id),
                    None => microphone.use_default_recording_device(),
                }
            });
            match tokio::time::timeout(AUDIO_START_TIMEOUT, work).await {
                Ok(Ok(Ok(_))) => result.mic_applied = generation.is_current(),
                Ok(Ok(Err(error))) => result.mic_error = Some(error),
                Ok(Err(error)) => result.mic_error = Some(error.to_string()),
                Err(_) => result.mic_error = Some("microphone switch timed out".into()),
            }
        } else {
            let native_control = audio_control.clone();
            let native_generation = generation.clone();
            let work = tokio::task::spawn_blocking(move || {
                let _control = native_control;
                if !native_generation.is_current() {
                    return Err(crate::transport::audio::AudioError::PlatformAudio(
                        livekit::AudioError::OperationFailed(
                            "room changed before microphone preparation".into(),
                        ),
                    ));
                }
                crate::transport::audio::prepare_microphone(recording_device, true)
            });
            let prepared = match tokio::time::timeout(AUDIO_START_TIMEOUT, work).await {
                Ok(Ok(Ok(prepared))) => Some(prepared),
                Ok(Ok(Err(error))) => {
                    result.mic_error = Some(error.to_string());
                    None
                }
                Ok(Err(error)) => {
                    result.mic_error = Some(error.to_string());
                    None
                }
                Err(_) => {
                    result.mic_error = Some("microphone preparation timed out".into());
                    None
                }
            };
            if let Some(prepared) = prepared {
                if !generation.is_current() {
                    return crate::transport::audio::AppliedAudioDevices::default();
                }
                let room = connection.room();
                let published = tokio::time::timeout(
                    AUDIO_START_TIMEOUT,
                    crate::transport::audio::publish_prepared_microphone(&room, &prepared),
                )
                .await;
                let mut prepared = Some(prepared);
                match published {
                    Ok(Ok(())) => {
                        let mut joined = state.joined.lock_unpoisoned();
                        if let Some(joined) = joined.as_mut().filter(|joined| {
                            generation.is_current() && Arc::ptr_eq(&joined.connection, &connection)
                        }) {
                            let microphone = Arc::new(
                                prepared
                                    .take()
                                    .expect("retained microphone")
                                    .into_mic_track(),
                            );
                            microphone.set_muted(state.mic_muted());
                            joined.microphone = Some(microphone);
                            result.mic_applied = true;
                            log::info!(
                                "linux session: native microphone attached to original room"
                            );
                        }
                    }
                    Ok(Err(error)) => result.mic_error = Some(error.to_string()),
                    Err(_) => result.mic_error = Some("microphone publish timed out".into()),
                }
                if let Some(prepared) = prepared {
                    prepared.mute_for_cleanup();
                    let _ = tokio::time::timeout(
                        AUDIO_CLEANUP_TIMEOUT,
                        crate::transport::audio::unpublish_prepared_microphone(&room, &prepared),
                    )
                    .await;
                }
            }
        }
        if let Some(error) = result.mic_error.as_mut() {
            *error = format!(
                "Microphone unavailable: {error}. Connect or select an input device and retry."
            );
        }
    }
    if generation.is_current() {
        result
    } else {
        crate::transport::audio::AppliedAudioDevices::default()
    }
}

pub async fn leave_room(app: &tauri::AppHandle, state: &SessionState) {
    let _transition = state.transition_lock.lock().await;
    leave_room_inner(app, state, true).await;
}

pub(crate) async fn cleanup_for_forced_disconnect(
    app: &tauri::AppHandle,
    state: &SessionState,
    generation: &crate::room_generation::RoomGeneration,
) {
    let _transition = state.transition_lock.lock().await;
    // A newer join may have won the transition lock while this event waited.
    if generation.is_current() {
        leave_room_inner(app, state, false).await;
    }
}

async fn leave_room_inner(app: &tauri::AppHandle, state: &SessionState, close: bool) {
    state.invalidate_room_generation();
    state.set_camera_intent(false);
    // The room-owned camera publication retires with the room (C07); an
    // explicitly visible Settings preview keeps its own lease.
    crate::camera_session::stop_camera_publish(app, state).await;
    state.cameras.room_left();
    crate::ai_chat::room_auth::forget();
    let joined = state.joined.lock_unpoisoned().take();
    if let Some(joined) = joined {
        joined.receiver.stop();
        if let Some(microphone) = joined.microphone.as_ref() {
            microphone.set_muted(true);
        }
        if close {
            if let Err(error) = joined.connection.room().close().await {
                log::warn!("linux session: room close failed: {error}");
            }
        }
        if let Err(error) = state.pictures.stop(app).await {
            log::warn!("linux picture cleanup after room retirement: {error}");
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

#[cfg(test)]
mod camera_permission_tests {
    use super::*;

    #[test]
    fn camera_permission_off_retires_pending_publication_request() {
        let state = SessionState::default();
        let generation = state.begin_room_generation();
        state.set_camera_intent(true);
        let request = state.begin_camera_permission_request(&generation).unwrap();
        state.set_camera_intent(false);
        assert!(request.cancellation().is_cancelled());
        assert!(state.begin_camera_permission_request(&generation).is_err());
    }

    #[test]
    fn camera_permission_leave_retires_pending_request_and_rejects_late_admission() {
        let state = SessionState::default();
        let generation = state.begin_room_generation();
        state.set_camera_intent(true);
        let request = state.begin_camera_permission_request(&generation).unwrap();
        state.invalidate_room_generation();
        assert!(request.cancellation().is_cancelled());
        assert!(state.begin_camera_permission_request(&generation).is_err());
    }

    #[test]
    fn camera_permission_completed_request_cannot_retire_its_successor() {
        let state = SessionState::default();
        let generation = state.begin_room_generation();
        state.set_camera_intent(true);
        let old = state.begin_camera_permission_request(&generation).unwrap();
        let current = state.begin_camera_permission_request(&generation).unwrap();
        assert!(old.cancellation().is_cancelled());
        drop(old);
        state.set_camera_intent(false);
        assert!(current.cancellation().is_cancelled());
    }

    #[tokio::test]
    async fn camera_permission_queued_on_cannot_cross_newer_off() {
        let state = SessionState::default();
        state.begin_room_generation();
        let settings_control = state.lock_camera_control().await;
        let queued_on = state.lock_camera_start();
        tokio::pin!(queued_on);
        assert!(futures_util::poll!(&mut queued_on).is_pending());
        state.set_camera_intent(false);
        drop(settings_control);
        assert!(queued_on.await.is_none());
        assert!(!state.camera_intent());
    }

    #[tokio::test]
    async fn camera_permission_queued_on_cannot_cross_room_leave() {
        let state = SessionState::default();
        state.begin_room_generation();
        let settings_control = state.lock_camera_control().await;
        let queued_on = state.lock_camera_start();
        tokio::pin!(queued_on);
        assert!(futures_util::poll!(&mut queued_on).is_pending());
        state.invalidate_room_generation();
        drop(settings_control);
        assert!(queued_on.await.is_none());
        assert!(!state.camera_intent());
    }

    #[tokio::test]
    async fn camera_permission_new_on_after_off_has_current_ownership() {
        let state = SessionState::default();
        let generation = state.begin_room_generation();
        let settings_control = state.lock_camera_control().await;
        let old_on = state.lock_camera_start();
        tokio::pin!(old_on);
        assert!(futures_util::poll!(&mut old_on).is_pending());
        state.set_camera_intent(false);
        let new_on = state.lock_camera_start();
        tokio::pin!(new_on);
        assert!(futures_util::poll!(&mut new_on).is_pending());
        drop(settings_control);
        assert!(old_on.await.is_none());
        let _current_control = new_on.await.expect("explicit new On remains valid");
        assert!(state.camera_intent());
        assert!(state.begin_camera_permission_request(&generation).is_ok());
    }

    #[tokio::test]
    async fn camera_permission_old_cancel_cleanup_preserves_newer_explicit_on() {
        let state = SessionState::default();
        let generation = state.begin_room_generation();
        let first_control = state.lock_camera_start().await.unwrap();
        let first_permission = state.begin_camera_permission_request(&generation).unwrap();
        state.set_camera_intent(false);
        assert!(first_permission.cancellation().is_cancelled());
        let second_on = state.lock_camera_start();
        tokio::pin!(second_on);
        assert!(futures_util::poll!(&mut second_on).is_pending());
        // The first start receives cancellation after the newer On queued.
        state.clear_camera_start_failure();
        state.clear_camera_start_failure();
        assert!(!state.camera_intent());
        drop(first_permission);
        drop(first_control);
        let _second_control = second_on.await.expect("old failure must not retire a newer explicit On");
        assert!(state.camera_intent());
        let second_permission = state.begin_camera_permission_request(&generation).unwrap();
        assert!(!second_permission.cancellation().is_cancelled());
    }
}
