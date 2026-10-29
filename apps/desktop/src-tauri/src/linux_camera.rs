//! One application-scoped Linux camera owner (ADR 0003): Camera-portal
//! permission, the restricted remote's PipeWire registry metadata, a single
//! low-level GStreamer capture, and separately cancellable Settings-preview
//! and room-publication leases over that one capture.
//!
//! The state machine in [`OwnerCore`] is pure (no portal, PipeWire, GTK or
//! Tauri types beyond plain data) so every lease/generation/invalidation
//! rule is unit-testable without a camera. The outer [`LinuxCameraOwner`]
//! attaches the real portal, registry, capture pump and GTK view surfaces.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::Emitter;
use tokio_util::sync::CancellationToken;

use crate::linux_camera_capture::{CameraCapturePipeline, CameraCaptureStatus};
use crate::linux_camera_metadata::{CameraRegistry, MetadataEvent};
use crate::linux_camera_portal::{CameraPortalSession, CAMERA_ACCESS_LOST};
use crate::linux_camera_view::CameraViewStore;
use crate::sync_ext::MutexExt;
use crate::transport::camera::{
    CameraBackend, CameraDeviceInfo, CameraError, CameraFrame, CameraMode, CameraStatus,
    CameraStatusSource, PreferredCameraMode,
};

/// Preview/publication events for the Settings surface.
pub(crate) const PREVIEW_STATE_EVENT: &str = "linux-camera-preview-state";
/// One portal permission dialog is bounded, like the ScreenCast path.
const PORTAL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub(crate) enum Permission {
    /// Never asked (or the last attempt was cancelled): nothing is claimed.
    #[default]
    NotDetermined,
    /// A live granted Camera portal session exists.
    Granted,
    /// The portal last answered with an explicit denial.
    Denied,
}

impl Permission {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Permission::NotDetermined => "not-determined",
            Permission::Granted => "authorized",
            Permission::Denied => "denied",
        }
    }
}

#[derive(Clone)]
struct OwnerDevice {
    node: u32,
    label: String,
}

impl OwnerDevice {
    fn id(&self) -> String {
        self.node.to_string()
    }

    fn info(&self) -> CameraDeviceInfo {
        CameraDeviceInfo {
            id: self.id(),
            name: self.label.clone(),
        }
    }
}

#[derive(Clone)]
struct OwnerSelection {
    generation: u64,
    node: u32,
    device_id: String,
    label: String,
    mode: Option<PreferredCameraMode>,
    used_default_fallback: bool,
}

struct OwnerCapture {
    generation: u64,
    node: u32,
    mode: Option<PreferredCameraMode>,
    negotiated: Option<(u32, u32, u32, u32)>,
    status: Arc<CameraCaptureStatus>,
    cancel: CancellationToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeaseId(u64);

struct PublicationLease {
    id: LeaseId,
    generation: u64,
    status: Arc<LeaseStatusSource>,
}

/// Per-lease identity for `CameraStatus::same_capture` (ADR 0003): each
/// publication lease gets its own source even when they share one capture.
struct LeaseStatusSource {
    capture: Arc<CameraCaptureStatus>,
    retired: Mutex<Option<String>>,
}

impl LeaseStatusSource {
    fn new(capture: Arc<CameraCaptureStatus>) -> Arc<Self> {
        Arc::new(Self {
            capture,
            retired: Mutex::new(None),
        })
    }

    fn retire(&self, reason: Option<String>) {
        let mut retired = self.retired.lock_unpoisoned();
        if retired.is_none() {
            *retired = reason;
        }
    }
}

impl CameraStatusSource for LeaseStatusSource {
    fn terminal_error(&self) -> Option<String> {
        self.retired
            .lock_unpoisoned()
            .clone()
            .or_else(|| self.capture.terminal_error())
    }

    fn frames_delivered(&self) -> u64 {
        self.capture.frames_delivered()
    }
}

/// The pure camera-ownership state machine.
#[derive(Default)]
struct OwnerCore {
    permission: Permission,
    devices: Vec<OwnerDevice>,
    selection: Option<OwnerSelection>,
    generation: u64,
    capture: Option<OwnerCapture>,
    publication: Option<PublicationLease>,
    preview: Option<LeaseId>,
    last_error: Option<String>,
}

impl OwnerCore {
    fn devices(&self) -> Vec<CameraDeviceInfo> {
        self.devices.iter().map(OwnerDevice::info).collect()
    }

    fn find_device(&self, device_id: Option<&str>) -> Result<OwnerDevice, String> {
        let device = match device_id {
            Some(id) if !id.is_empty() => self
                .devices
                .iter()
                .find(|device| &device.id() == id)
                .cloned()
                .ok_or("Selected camera is no longer available")?,
            _ => self
                .devices
                .first()
                .cloned()
                .ok_or("No camera is available")?,
        };
        Ok(device)
    }

    /// Latest selection wins (C04): every call bumps the generation, so any
    /// still-running older open or mode completion can be told apart and
    /// retired. Frames and leases are always bound to a generation.
    fn begin_selection(
        &mut self,
        device_id: Option<&str>,
        mode: Option<PreferredCameraMode>,
    ) -> OwnerSelection {
        let device = self
            .find_device(device_id)
            .expect("caller validated the device list");
        self.generation = self.generation.checked_add(1).expect("generation overflow");
        let selection = OwnerSelection {
            generation: self.generation,
            node: device.node,
            device_id: device.id(),
            label: device.label.clone(),
            mode,
            used_default_fallback: device_id.is_none(),
        };
        self.last_error = None;
        self.selection = Some(selection.clone());
        selection
    }

    /// Whether a capture opened for `generation` still has a consumer.
    fn wants_capture(&self, generation: u64) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|selection| selection.generation == generation)
            && (self.publication.is_some() || self.preview.is_some())
    }

    /// Register the one low-level capture. A stale generation is rejected
    /// (the caller tears that pipeline down); a duplicate of the serving
    /// capture is accepted while the caller drops the extra pipeline.
    fn capture_started(&mut self, capture: OwnerCapture) -> Result<(), String> {
        let Some(selection) = self.selection.clone() else {
            return Err("Camera selection retired".to_string());
        };
        if capture.generation != selection.generation || capture.node != selection.node {
            return Err("Camera selection superseded".to_string());
        }
        if self.capture.is_some() {
            return Ok(());
        }
        self.capture = Some(capture);
        Ok(())
    }

    fn capture_matches(
        &self,
        generation: u64,
        node: u32,
        mode: Option<PreferredCameraMode>,
    ) -> bool {
        self.capture.as_ref().is_some_and(|capture| {
            capture.generation == generation && capture.node == node && capture.mode == mode
        })
    }

    fn note_negotiated(&mut self, generation: u64, negotiated: (u32, u32, u32, u32)) {
        if let Some(capture) = self
            .capture
            .as_mut()
            .filter(|capture| capture.generation == generation)
        {
            capture.negotiated = Some(negotiated);
        }
    }

    fn current_capture_status(&self) -> Option<Arc<CameraCaptureStatus>> {
        self.capture.as_ref().map(|capture| capture.status.clone())
    }

    fn lease_publication(&mut self, next_lease: &AtomicU64) -> Result<PublicationLease, String> {
        let generation = self
            .selection
            .as_ref()
            .map(|selection| selection.generation)
            .ok_or("No camera is selected")?;
        let status = self
            .current_capture_status()
            .filter(|status| {
                self.capture
                    .as_ref()
                    .is_some_and(|capture| capture.generation == generation)
            })
            .ok_or(CAMERA_ACCESS_LOST.to_string())?;
        if status.is_retired() {
            return Err(CAMERA_ACCESS_LOST.to_string());
        }
        let id = LeaseId(next_lease.fetch_add(1, Ordering::SeqCst));
        let lease = PublicationLease {
            id,
            generation,
            status: LeaseStatusSource::new(status),
        };
        self.publication = Some(PublicationLease {
            id: lease.id,
            generation: lease.generation,
            status: lease.status.clone(),
        });
        Ok(lease)
    }

    fn lease_preview(
        &mut self,
        next_lease: &AtomicU64,
    ) -> Result<(LeaseId, String, String), String> {
        let selection = self
            .selection
            .clone()
            .ok_or("No camera is selected; turn the camera on first")?;
        let status = self
            .current_capture_status()
            .filter(|_| {
                self.capture
                    .as_ref()
                    .is_some_and(|capture| capture.generation == selection.generation)
            })
            .ok_or(CAMERA_ACCESS_LOST.to_string())?;
        if status.is_retired() {
            return Err(CAMERA_ACCESS_LOST.to_string());
        }
        let id = LeaseId(next_lease.fetch_add(1, Ordering::SeqCst));
        self.preview = Some(id);
        Ok((id, selection.device_id, selection.label))
    }

    /// Idempotent lease release. When the last consumer goes away the
    /// capture must stop (C02/C07): it is returned for off-thread teardown.
    fn release_lease(&mut self, id: LeaseId) -> Option<OwnerCapture> {
        if self
            .publication
            .as_ref()
            .is_some_and(|lease| lease.id == id)
        {
            if let Some(lease) = self.publication.take() {
                lease.status.retire(None);
            }
        } else if self.preview == Some(id) {
            self.preview = None;
        }
        (self.publication.is_none() && self.preview.is_none())
            .then(|| self.capture.take())
            .flatten()
    }

    /// Invalidate EVERY lease and the capture (C05): permission, registry or
    /// pipeline loss retires all consumers with the neutral explanation.
    fn invalidate_all(&mut self, reason: String) -> Option<OwnerCapture> {
        self.last_error = Some(reason.clone());
        if let Some(lease) = self.publication.take() {
            lease.status.retire(Some(reason.clone()));
        }
        self.preview = None;
        self.capture.take()
    }

    /// Room ownership ended: retire publication state. An explicitly visible
    /// Settings preview keeps its lease and the shared capture (C02).
    fn publication_retired_by_room(&mut self) -> Option<OwnerCapture> {
        if let Some(lease) = self.publication.take() {
            lease
                .status
                .retire(Some("Camera publication stopped".to_string()));
        }
        (self.publication.is_none() && self.preview.is_none())
            .then(|| self.capture.take())
            .flatten()
    }

    fn snapshot(&self) -> OwnerSnapshot {
        OwnerSnapshot {
            permission: self.permission,
            devices: self.devices(),
            selected: self.selection.as_ref().map(|selection| SelectedCamera {
                device_id: selection.device_id.clone(),
                label: selection.label.clone(),
                mode: selection.mode,
            }),
            capture_active: self.capture.is_some(),
            publication: self.publication.is_some(),
            preview: self.preview.is_some(),
            preview_device: self
                .preview
                .and_then(|_| self.selection.as_ref())
                .map(|selection| (selection.device_id.clone(), selection.label.clone())),
            used_default_fallback: self
                .selection
                .as_ref()
                .map(|selection| selection.used_default_fallback)
                .unwrap_or(false),
            negotiated: self.capture.as_ref().and_then(|capture| capture.negotiated),
            last_error: self.last_error.clone(),
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectedCamera {
    pub(crate) device_id: String,
    pub(crate) label: String,
    pub(crate) mode: Option<PreferredCameraMode>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OwnerSnapshot {
    permission: Permission,
    devices: Vec<CameraDeviceInfo>,
    selected: Option<SelectedCamera>,
    capture_active: bool,
    publication: bool,
    preview: bool,
    preview_device: Option<(String, String)>,
    used_default_fallback: bool,
    negotiated: Option<(u32, u32, u32, u32)>,
    last_error: Option<String>,
}

/// The application-scoped owner: portal session + registry + one capture.
pub(crate) struct LinuxCameraOwner {
    core: Mutex<OwnerCore>,
    portal: tokio::sync::Mutex<Option<CameraPortalSession>>,
    registry: Mutex<Option<Arc<CameraRegistry>>>,
    capture_serial: tokio::sync::Mutex<()>,
    view: CameraViewStore,
    next_lease: AtomicU64,
    app: Mutex<Option<tauri::AppHandle>>,
    publication_dispatch: Mutex<Option<Arc<LeaseDispatcher>>>,
}

static GLOBAL_OWNER: LazyLock<Arc<LinuxCameraOwner>> =
    LazyLock::new(|| Arc::new(LinuxCameraOwner::new()));

/// The one application-scoped camera owner (ADR 0003). The Tauri session
/// state and the platform-neutral `open_camera` path share this instance.
pub(crate) fn global_owner() -> Arc<LinuxCameraOwner> {
    Arc::clone(&GLOBAL_OWNER)
}

/// Frame dispatcher owned by the publication lease: delivers frames only
/// while its lease is the current publication of the same generation.
struct LeaseDispatcher {
    id: LeaseId,
    generation: u64,
    on_frame: Mutex<Option<Arc<dyn Fn(CameraFrame) + Send + Sync>>>,
}

impl LinuxCameraOwner {
    pub(crate) fn new() -> Self {
        Self {
            core: Mutex::new(OwnerCore::default()),
            portal: tokio::sync::Mutex::new(None),
            registry: Mutex::new(None),
            capture_serial: tokio::sync::Mutex::new(()),
            view: CameraViewStore::new(),
            next_lease: AtomicU64::new(1),
            app: Mutex::new(None),
            publication_dispatch: Mutex::new(None),
        }
    }

    pub(crate) fn set_app(&self, app: &tauri::AppHandle) {
        *self.app.lock_unpoisoned() = Some(app.clone());
        let view = self.view.clone();
        let main_app = app.clone();
        let _ = app.run_on_main_thread(move || {
            crate::linux_camera_view::start_camera_gtk_pump(main_app, view);
        });
    }

    pub(crate) fn permission_status(&self) -> Permission {
        self.core.lock_unpoisoned().permission
    }

    pub(crate) fn snapshot(&self) -> OwnerSnapshot {
        self.core.lock_unpoisoned().snapshot()
    }

    pub(crate) fn devices(&self) -> Vec<CameraDeviceInfo> {
        let core = self.core.lock_unpoisoned();
        if core.permission != Permission::Granted {
            // Without a live grant nothing can be listed truthfully.
            return Vec::new();
        }
        core.devices()
    }

    pub(crate) fn modes(&self, device_id: Option<&str>) -> Vec<CameraMode> {
        let registry = self.registry.lock_unpoisoned().clone();
        let Some(registry) = registry else {
            return Vec::new();
        };
        let node = {
            let core = self.core.lock_unpoisoned();
            match device_id {
                Some(id) if !id.is_empty() => id
                    .parse::<u32>()
                    .ok()
                    .and_then(|node| core.devices.iter().find(|device| device.node == node))
                    .map(|device| device.node),
                _ => core.devices.first().map(|device| device.node),
            }
        };
        let Some(node) = node else {
            return Vec::new();
        };
        match registry.modes(node) {
            Ok(modes) => modes,
            Err(error) => {
                log::warn!("linux camera modes unavailable: {error}");
                Vec::new()
            }
        }
    }

    /// Explicit permission request (Settings "Try again", permission rows).
    pub(crate) async fn request_permission(self: &Arc<Self>, app: &tauri::AppHandle) -> Permission {
        match self.ensure_access(app).await {
            Ok(()) => Permission::Granted,
            Err(error) => {
                let mut core = self.core.lock_unpoisoned();
                if error.contains("denied") {
                    core.permission = Permission::Denied;
                    core.last_error = Some(error);
                }
                core.permission
            }
        }
    }

    /// Ensure a granted Camera portal session with a live registry exists.
    /// A cancelled request leaves permission unclaimed (camera off); a
    /// denial is recorded as denied.
    pub(crate) async fn ensure_access(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
    ) -> Result<(), String> {
        self.set_app(app);
        {
            let core = self.core.lock_unpoisoned();
            if core.permission == Permission::Granted && self.registry.lock_unpoisoned().is_some() {
                return Ok(());
            }
        }
        let mut portal_guard = self.portal.lock().await;
        // Another task may have completed the dialog while we waited.
        {
            let core = self.core.lock_unpoisoned();
            if core.permission == Permission::Granted && self.registry.lock_unpoisoned().is_some() {
                return Ok(());
            }
        }
        // A stale session (access lost / re-request) must not be reused.
        self.close_session_locked(&mut portal_guard).await;

        let cancel = CancellationToken::new();
        let mut session = CameraPortalSession::connect().await?;
        let access = tokio::select! {
            _ = cancel.cancelled() => Err("Camera permission cancelled; the camera stays off".to_string()),
            result = tokio::time::timeout(PORTAL_TIMEOUT, session.request_access(&cancel)) =>
                result.unwrap_or_else(|_| Err("Camera permission timed out; turn the camera on again to retry".to_string())),
        };
        if let Err(error) = access {
            {
                let mut core = self.core.lock_unpoisoned();
                if error.contains("denied") {
                    core.permission = Permission::Denied;
                }
                core.last_error = Some(error.clone());
            }
            let _ = session.close().await;
            return Err(error);
        }
        let metadata_remote = session.remote().await?;
        let (events, receiver) = std::sync::mpsc::channel();
        let registry = Arc::new(CameraRegistry::connect(metadata_remote, events)?);
        {
            let mut slot = self.registry.lock_unpoisoned();
            *slot = Some(registry.clone());
        }
        // Initial enumeration is a bounded blocking wait.
        let wait_registry = registry.clone();
        let waited = tokio::task::spawn_blocking(move || wait_registry.wait_initial())
            .await
            .map_err(|e| format!("Camera registry worker failed: {e}"))?;
        if let Err(error) = waited {
            self.close_session_locked(&mut portal_guard).await;
            self.core.lock_unpoisoned().last_error = Some(error.clone());
            return Err(error);
        }
        *portal_guard = Some(session);
        self.refresh_devices_from(&registry);
        self.spawn_metadata_forwarder(registry.clone(), receiver);
        self.core.lock_unpoisoned().permission = Permission::Granted;
        Ok(())
    }

    async fn close_session_locked(
        &self,
        portal_guard: &mut tokio::sync::MutexGuard<'_, Option<CameraPortalSession>>,
    ) {
        if let Some(mut session) = portal_guard.take() {
            session.close().await;
        }
        // Take the registry out under a short-lived guard, then destroy it
        // off the async thread without holding the guard across the await.
        let retired = self.registry.lock_unpoisoned().take();
        if let Some(registry) = retired {
            let _ = tokio::task::spawn_blocking(move || drop(registry)).await;
        }
        let mut core = self.core.lock_unpoisoned();
        if core.permission != Permission::Denied {
            core.permission = Permission::NotDetermined;
        }
    }

    fn refresh_devices_from(&self, registry: &Arc<CameraRegistry>) {
        let devices: Vec<OwnerDevice> = registry
            .devices()
            .into_iter()
            .map(|device| OwnerDevice {
                node: device.node,
                label: device.label,
            })
            .collect();
        self.core.lock_unpoisoned().devices = devices;
    }

    fn spawn_metadata_forwarder(
        self: &Arc<Self>,
        registry: Arc<CameraRegistry>,
        receiver: std::sync::mpsc::Receiver<MetadataEvent>,
    ) {
        // The forwarder owns the receiver for the registry's lifetime; the
        // sender (inside the registry) dropping is what ends it.
        let owner = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            while let Ok(event) = receiver.recv() {
                owner.handle_metadata_event(&registry, event);
            }
        });
    }

    fn handle_metadata_event(
        self: &Arc<Self>,
        registry: &Arc<CameraRegistry>,
        event: MetadataEvent,
    ) {
        match event {
            MetadataEvent::Devices(_) => self.refresh_devices_from(registry),
            MetadataEvent::NodeRemoved(node) => {
                let generation = {
                    let core = self.core.lock_unpoisoned();
                    core.selection
                        .as_ref()
                        .filter(|selection| selection.node == node)
                        .map(|selection| selection.generation)
                };
                if let Some(generation) = generation {
                    let capture = self
                        .core
                        .lock_unpoisoned()
                        .invalidate_all(CAMERA_ACCESS_LOST.to_string());
                    registry.watch(None);
                    self.retire_capture_resources(capture);
                    self.emit_preview_state(false, None, Some(CAMERA_ACCESS_LOST));
                    log::warn!("linux camera node {node} removed; selection {generation} retired");
                }
            }
            MetadataEvent::AccessLost(reason) => {
                log::warn!("linux camera registry access lost: {reason}");
                let capture = {
                    let mut core = self.core.lock_unpoisoned();
                    core.last_error = Some(reason);
                    core.invalidate_all(CAMERA_ACCESS_LOST.to_string())
                };
                self.retire_capture_resources(capture);
                self.emit_preview_state(false, None, Some(CAMERA_ACCESS_LOST));
                // The registry connection itself is gone: close the session
                // so the next explicit start re-requests a fresh remote.
                if let Some(mut session) = self.portal.blocking_lock().take() {
                    let _ = tokio::spawn(async move {
                        session.close().await;
                    });
                }
                if let Some(registry) = self.registry.lock_unpoisoned().take() {
                    std::thread::spawn(move || drop(registry));
                }
                let mut core = self.core.lock_unpoisoned();
                if core.permission != Permission::Denied {
                    core.permission = Permission::NotDetermined;
                }
            }
        }
    }

    /// Resolve the preference against real devices and start the single
    /// low-level capture on the selected node. Called before the
    /// platform-neutral publish path acquires its lease.
    pub(crate) async fn prepare(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        device_id: Option<&str>,
        mode: Option<PreferredCameraMode>,
    ) -> Result<(), String> {
        self.ensure_access(app).await?;
        {
            let core = self.core.lock_unpoisoned();
            core.find_device(device_id).map(|_| ())?;
        }
        let selection = {
            let mut core = self.core.lock_unpoisoned();
            core.begin_selection(device_id, mode)
        };
        if let Some(mode) = mode {
            let registry = self.registry.lock_unpoisoned().clone();
            if let Some(registry) = registry {
                let supported = registry.modes(selection.node)?;
                let advertised = supported.iter().any(|candidate| {
                    candidate.width == mode.width
                        && candidate.height == mode.height
                        && (candidate.frame_rate_numerator as f64
                            / candidate.frame_rate_denominator.max(1) as f64)
                            .round() as u32
                            == mode.frame_rate
                });
                if !advertised {
                    self.core.lock_unpoisoned().selection = None;
                    return Err(format!(
                        "The selected camera mode ({}×{}@{}) is not available",
                        mode.width, mode.height, mode.frame_rate
                    ));
                }
            }
        }
        self.ensure_capture(
            selection.generation,
            selection.node,
            selection.mode,
            &selection.label,
        )
        .await
    }

    /// Start or replace the one low-level capture for `generation`.
    async fn ensure_capture(
        self: &Arc<Self>,
        generation: u64,
        node: u32,
        mode: Option<PreferredCameraMode>,
        label: &str,
    ) -> Result<(), String> {
        let _serial = self.capture_serial.lock().await;
        {
            let core = self.core.lock_unpoisoned();
            if core.capture_matches(generation, node, mode) {
                return Ok(());
            }
        }
        // Stop any previous capture for a superseded selection.
        let previous = self.core.lock_unpoisoned().capture.take();
        if let Some(previous) = previous {
            previous.cancel.cancel();
            self.retire_capture_resources(Some(previous));
        }
        let remote = {
            let portal = self.portal.lock().await;
            portal
                .as_ref()
                .ok_or("Camera permission is not granted; turn the camera on again")?
                .remote()
                .await?
        };
        let status = CameraCaptureStatus::new();
        let opened = tokio::task::spawn_blocking({
            let status = status.clone();
            move || CameraCapturePipeline::open(remote, node, mode, status)
        })
        .await
        .map_err(|e| format!("Native camera worker failed: {e}"))?;
        let pipeline = match opened {
            Ok(pipeline) => pipeline,
            Err(error) => {
                self.core.lock_unpoisoned().last_error = Some(error.clone());
                return Err(error);
            }
        };
        let cancel = CancellationToken::new();
        let capture = OwnerCapture {
            generation,
            node,
            mode,
            negotiated: None,
            status: status.clone(),
            cancel: cancel.clone(),
        };
        let accepted = self.core.lock_unpoisoned().capture_started(capture);
        if let Err(error) = accepted {
            // A newer selection won (or the selection retired) while this
            // capture opened: this pipeline never serves frames.
            cancel.cancel();
            let _ = tokio::task::spawn_blocking(move || drop(pipeline)).await;
            return Err(error);
        }
        // The watched node's registry removal invalidates everything (C05).
        if let Some(registry) = self.registry.lock_unpoisoned().clone() {
            registry.watch(Some(node));
        }
        self.spawn_capture_pump(pipeline, generation, label.to_string(), cancel);
        Ok(())
    }

    fn spawn_capture_pump(
        self: &Arc<Self>,
        pipeline: CameraCapturePipeline,
        generation: u64,
        label: String,
        cancel: CancellationToken,
    ) {
        let owner = Arc::clone(self);
        let view = self.view.clone();
        tokio::spawn(async move {
            let mut pipeline = pipeline;
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    frame = pipeline.next_frame() => match frame {
                        Ok(frame) => {
                            let negotiated = (frame.width, frame.height, 30, 1);
                            view.publish(frame.clone());
                            owner.frame_arrived(generation, frame, negotiated, &label);
                        }
                        Err(error) => {
                            owner.capture_failed(generation, &error, &label);
                            break;
                        }
                    },
                }
            }
            // Release OS media authority off the async runtime.
            let _ = tokio::task::spawn_blocking(move || drop(pipeline)).await;
        });
    }

    /// One captured frame arrived from the generation's pipeline.
    fn frame_arrived(
        &self,
        generation: u64,
        frame: CameraFrame,
        negotiated: (u32, u32, u32, u32),
        _label: &str,
    ) {
        let current = {
            let mut core = self.core.lock_unpoisoned();
            if !core.wants_capture(generation) {
                return;
            }
            core.note_negotiated(generation, negotiated);
            core.publication
                .as_ref()
                .map(|lease| (lease.id, lease.generation))
        };
        self.view
            .set_surface_state(current.is_some(), self.view.preview_surface_active(), None);
        let Some((lease_id, lease_generation)) = current else {
            return;
        };
        if lease_generation != generation {
            return;
        }
        let callback = {
            let slot = self.publication_dispatch.lock_unpoisoned();
            match slot.as_ref() {
                Some(dispatch) if dispatch.id == lease_id && dispatch.generation == generation => {
                    dispatch.on_frame.lock_unpoisoned().clone()
                }
                _ => None,
            }
        };
        if let Some(on_frame) = callback {
            on_frame(frame);
        }
    }

    fn capture_failed(&self, generation: u64, error: &str, _label: &str) {
        let capture = {
            let mut core = self.core.lock_unpoisoned();
            if !core
                .capture
                .as_ref()
                .is_some_and(|capture| capture.generation == generation)
            {
                return;
            }
            core.invalidate_all(CAMERA_ACCESS_LOST.to_string())
        };
        self.retire_capture_resources(capture);
        self.emit_preview_state(false, None, Some(CAMERA_ACCESS_LOST));
        log::warn!("linux camera capture failed at generation {generation}: {error}");
    }

    fn retire_capture_resources(&self, capture: Option<OwnerCapture>) {
        if let Some(capture) = capture {
            capture.cancel.cancel();
        }
        self.view.set_surface_state(false, false, None);
        self.view.clear();
    }

    pub(crate) async fn preview_open(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
    ) -> Result<(String, String), String> {
        self.ensure_access(app).await?;
        let selection = {
            let core = self.core.lock_unpoisoned();
            core.selection
                .clone()
                .ok_or("No camera is selected; turn the camera on first")?
        };
        self.ensure_capture(
            selection.generation,
            selection.node,
            selection.mode,
            &selection.label,
        )
        .await?;
        let (_lease, device_id, label) = {
            let mut core = self.core.lock_unpoisoned();
            core.preview.take();
            core.lease_preview(&self.next_lease)?
        };
        self.view.set_surface_state(
            self.core.lock_unpoisoned().publication.is_some(),
            true,
            Some(label.as_str()),
        );
        self.emit_preview_state(true, Some((device_id.clone(), label.clone())), None);
        Ok((device_id, label))
    }

    pub(crate) async fn preview_close(&self) -> Result<(), String> {
        self.close_preview();
        Ok(())
    }

    /// Room leave: the room-owned publication and its self-view retire; an
    /// explicitly visible Settings preview keeps the shared capture.
    pub(crate) fn room_left(&self) {
        let capture = self.core.lock_unpoisoned().publication_retired_by_room();
        if let Some(capture) = &capture {
            capture.cancel.cancel();
        }
        let preview_active = self.core.lock_unpoisoned().preview.is_some();
        self.view.set_surface_state(false, preview_active, None);
        if !preview_active {
            self.view.clear();
        }
    }

    /// Lease acquisition for the platform-neutral publish path. Blocking:
    /// called from `open_camera` on a blocking thread, after `prepare`.
    pub(crate) fn acquire_publication(
        &self,
        on_frame: Arc<dyn Fn(CameraFrame) + Send + Sync>,
    ) -> Result<(LeaseId, CameraStatus, Arc<LeaseDispatcher>, u64), CameraError> {
        let lease = {
            let mut core = self.core.lock_unpoisoned();
            if core.permission != Permission::Granted {
                return Err(CameraError::Operation(
                    "Camera permission is not granted; turn the camera on again".to_string(),
                ));
            }
            core.lease_publication(&self.next_lease)
                .map_err(CameraError::Operation)?
        };
        let dispatch = Arc::new(LeaseDispatcher {
            id: lease.id,
            generation: lease.generation,
            on_frame: Mutex::new(Some(on_frame)),
        });
        *self.publication_dispatch.lock_unpoisoned() = Some(dispatch.clone());
        Ok((
            lease.id,
            CameraStatus::new(lease.status),
            dispatch,
            lease.generation,
        ))
    }

    pub(crate) fn release_publication(&self, id: LeaseId) {
        let capture = {
            let mut core = self.core.lock_unpoisoned();
            core.release_lease(id)
        };
        {
            let mut slot = self.publication_dispatch.lock_unpoisoned();
            if slot.as_ref().is_some_and(|dispatch| dispatch.id == id) {
                *slot = None;
            }
        }
        if let Some(capture) = &capture {
            capture.cancel.cancel();
        }
        let preview_active = self.core.lock_unpoisoned().preview.is_some();
        self.view.set_surface_state(false, preview_active, None);
        if !preview_active {
            self.view.clear();
        }
    }

    fn emit_preview_state(
        &self,
        open: bool,
        device: Option<(String, String)>,
        error: Option<&str>,
    ) {
        let app = self.app.lock_unpoisoned().clone();
        let Some(app) = app else { return };
        let payload = serde_json::json!({
            "open": open,
            "deviceId": device.as_ref().map(|(id, _)| id),
            "label": device.as_ref().map(|(_, label)| label),
            "error": error,
        });
        let _ = app.emit(PREVIEW_STATE_EVENT, payload);
    }
}

impl LinuxCameraOwner {
    /// Negotiated geometry and fallback flag for an acquired lease, read
    /// once at lease creation. Later frames update the core, never this.
    fn lease_details(&self, generation: u64) -> (Option<(u32, u32, u32, u32)>, bool) {
        let core = self.core.lock_unpoisoned();
        (
            core.capture
                .as_ref()
                .filter(|capture| capture.generation == generation)
                .and_then(|capture| capture.negotiated),
            core.selection
                .as_ref()
                .map(|selection| selection.used_default_fallback)
                .unwrap_or(false),
        )
    }

    /// The selected device id for an acquired lease.
    fn lease_device_id(&self, generation: u64) -> String {
        self.core
            .lock_unpoisoned()
            .selection
            .as_ref()
            .filter(|selection| selection.generation == generation)
            .map(|selection| selection.device_id.clone())
            .unwrap_or_default()
    }

    /// Release the preview lease and, when it was the last consumer, stop
    /// the capture (C02). Sync: called from the Tauri command and from the
    /// GTK preview window's close handler on the main thread.
    pub(crate) fn close_preview(&self) {
        let capture = {
            let mut core = self.core.lock_unpoisoned();
            let lease = core.preview.take();
            lease.and_then(|lease| core.release_lease(lease))
        };
        if let Some(capture) = &capture {
            capture.cancel.cancel();
        }
        let publication_active = self.core.lock_unpoisoned().publication.is_some();
        self.view.set_surface_state(publication_active, false, None);
        if !publication_active {
            self.view.clear();
        }
        self.emit_preview_state(false, None, None);
    }

    /// The native preview window was closed by the user.
    pub(crate) fn close_preview_from_surface(&self) {
        self.close_preview();
    }
}

/// The Linux `CameraBackend`: a publication lease over the shared capture.
/// `stop` releases only this lease; the last consumer releases the capture.
pub(crate) struct LinuxCameraLease {
    owner: Arc<LinuxCameraOwner>,
    id: LeaseId,
    generation: u64,
    device_id: String,
    status: CameraStatus,
    negotiated: Option<(u32, u32, u32, u32)>,
    frame_rate: Option<(u32, u32)>,
    used_default_fallback: bool,
    stopped: bool,
}

impl LinuxCameraLease {
    /// Acquire the publication lease on the already-prepared capture. The
    /// platform-neutral `open_camera` calls this on a blocking thread after
    /// the portal dialog and capture start completed in `prepare`.
    pub(crate) fn open(
        on_frame: impl Fn(CameraFrame) + Send + Sync + 'static,
    ) -> Result<Box<Self>, CameraError> {
        let owner = global_owner();
        let (id, status, _dispatch, generation) = owner.acquire_publication(Arc::new(on_frame))?;
        let (negotiated, used_default_fallback) = owner.lease_details(generation);
        let device_id = owner.lease_device_id(generation);
        Ok(Box::new(Self {
            owner,
            id,
            generation,
            device_id,
            status,
            negotiated,
            frame_rate: negotiated.map(|(_, _, num, den)| (num, den)),
            used_default_fallback,
            stopped: false,
        }))
    }
}

impl CameraBackend for LinuxCameraLease {
    fn stop(&mut self) {
        if !self.stopped {
            self.stopped = true;
            self.owner.release_publication(self.id);
        }
    }

    fn dimensions(&self) -> (u32, u32) {
        self.negotiated
            .map(|(width, height, _, _)| (width, height))
            .unwrap_or((0, 0))
    }

    fn frame_rate(&self) -> (u32, u32) {
        self.frame_rate.unwrap_or((30, 1))
    }

    fn device_id(&self) -> &str {
        &self.device_id
    }

    fn used_default_fallback(&self) -> bool {
        self.used_default_fallback
    }

    fn status_handle(&self) -> CameraStatus {
        self.status.clone()
    }
}

impl Drop for LinuxCameraLease {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewOpened {
    pub(crate) device_id: String,
    pub(crate) label: String,
}

#[tauri::command]
pub(crate) async fn linux_camera_preview_open(
    app: tauri::AppHandle,
) -> Result<PreviewOpened, String> {
    let (device_id, label) = global_owner().preview_open(&app).await?;
    Ok(PreviewOpened { device_id, label })
}

#[tauri::command]
pub(crate) async fn linux_camera_preview_close() -> Result<(), String> {
    global_owner().preview_close().await
}

#[tauri::command]
pub(crate) fn linux_camera_state() -> OwnerSnapshot {
    global_owner().snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(node: u32, label: &str) -> OwnerDevice {
        OwnerDevice {
            node,
            label: label.to_string(),
        }
    }

    fn core_with_cameras() -> OwnerCore {
        let mut core = OwnerCore::default();
        core.permission = Permission::Granted;
        core.devices = vec![device(11, "Cam-A"), device(22, "Cam-B")];
        core
    }

    fn mode(width: u32) -> Option<PreferredCameraMode> {
        Some(PreferredCameraMode {
            width,
            height: width * 9 / 16,
            frame_rate: 30,
        })
    }

    fn capture_for(generation: u64, node: u32) -> OwnerCapture {
        OwnerCapture {
            generation,
            node,
            mode: None,
            negotiated: None,
            status: CameraCaptureStatus::new(),
            cancel: CancellationToken::new(),
        }
    }

    #[test]
    fn missing_devices_and_unknown_ids_fail_closed() {
        let mut core = OwnerCore::default();
        core.permission = Permission::Granted;
        assert!(core.find_device(None).is_err());
        assert!(core.find_device(Some("99")).is_err());
        core.devices = vec![device(11, "Cam-A")];
        assert!(core.find_device(None).is_ok());
        assert!(core.find_device(Some("99")).is_err());
        assert_eq!(core.find_device(Some("11")).unwrap().label, "Cam-A");
    }

    #[test]
    fn latest_selection_wins_and_old_completions_are_retired() {
        let mut core = core_with_cameras();
        let next = AtomicU64::new(1);
        let cam_a = core.begin_selection(Some("11"), mode(1280));
        let cam_b = core.begin_selection(Some("22"), mode(640));
        assert_eq!(cam_b.generation, cam_a.generation + 1);
        // Cam-A's late capture completion must not become the live capture.
        assert!(core
            .capture_started(capture_for(cam_a.generation, 11))
            .is_err());
        assert!(core.capture.is_none());
        // Cam-B's completion is accepted; a duplicate is not stored.
        core.capture_started(capture_for(cam_b.generation, 22))
            .unwrap();
        assert!(core.capture_matches(cam_b.generation, 22, None));
        core.capture_started(capture_for(cam_b.generation, 22))
            .unwrap();
        assert!(core.capture_matches(cam_b.generation, 22, None));
    }

    #[test]
    fn a_superseded_capture_is_no_longer_wanted() {
        let mut core = core_with_cameras();
        let first = core.begin_selection(Some("11"), None);
        core.capture_started(capture_for(first.generation, 11))
            .unwrap();
        let second = core.begin_selection(Some("22"), None);
        // ensure_capture removes the old capture before opening the new one.
        let previous = core.capture.take().unwrap();
        assert_eq!(previous.generation, first.generation);
        core.capture_started(capture_for(second.generation, 22))
            .unwrap();
        assert!(core.capture_matches(second.generation, 22, None));
        assert!(!core.wants_capture(first.generation));
    }

    #[test]
    fn publication_lease_release_keeps_preview_and_last_consumer_stops_capture() {
        let mut core = core_with_cameras();
        let next = AtomicU64::new(1);
        let selection = core.begin_selection(Some("11"), None);
        core.capture_started(capture_for(selection.generation, 11))
            .unwrap();
        let (preview, _, _) = core.lease_preview(&next).unwrap();
        let publication = core.lease_publication(&next).unwrap();
        // Releasing the publication keeps the shared capture for preview.
        assert!(core.release_lease(publication.id).is_none());
        let snapshot = core.snapshot();
        assert!(snapshot.capture_active);
        assert!(snapshot.preview);
        assert!(!snapshot.publication);
        assert!(publication.status.terminal_error().is_none());
        // The last consumer's release stops the capture (C02).
        assert!(core.release_lease(preview).is_some());
        assert!(!core.snapshot().capture_active);
    }

    #[test]
    fn invalidation_retires_every_lease_with_the_neutral_explanation() {
        let mut core = core_with_cameras();
        let next = AtomicU64::new(1);
        let selection = core.begin_selection(Some("11"), None);
        core.capture_started(capture_for(selection.generation, 11))
            .unwrap();
        let (preview, _, _) = core.lease_preview(&next).unwrap();
        let publication = core.lease_publication(&next).unwrap();
        assert!(core
            .invalidate_all(CAMERA_ACCESS_LOST.to_string())
            .is_some());
        assert!(publication
            .status
            .terminal_error()
            .is_some_and(|error| error.contains("Camera access or device lost")));
        let snapshot = core.snapshot();
        assert!(!snapshot.preview);
        assert!(!snapshot.publication);
        assert!(snapshot.last_error.is_some());
        assert!(core.release_lease(preview).is_none());
        // A retired capture cannot serve a new lease.
        assert!(core.lease_preview(&next).is_err());
        assert!(core.lease_publication(&next).is_err());
    }

    #[test]
    fn room_retirement_keeps_an_explicitly_open_preview() {
        let mut core = core_with_cameras();
        let next = AtomicU64::new(1);
        let selection = core.begin_selection(Some("11"), None);
        core.capture_started(capture_for(selection.generation, 11))
            .unwrap();
        let _preview = core.lease_preview(&next).unwrap();
        let publication = core.lease_publication(&next).unwrap();
        assert!(core.publication_retired_by_room().is_none());
        let snapshot = core.snapshot();
        assert!(!snapshot.publication);
        assert!(snapshot.preview);
        assert!(snapshot.capture_active);
        assert!(publication
            .status
            .terminal_error()
            .is_some_and(|error| error.contains("publication stopped")));
    }

    #[test]
    fn stale_generations_cannot_negotiate_or_win_a_lease() {
        let mut core = core_with_cameras();
        let next = AtomicU64::new(1);
        let selection = core.begin_selection(Some("11"), None);
        core.capture_started(capture_for(selection.generation, 11))
            .unwrap();
        let newer = core.begin_selection(Some("22"), None);
        core.capture.take();
        core.capture_started(capture_for(newer.generation, 22))
            .unwrap();
        // A late frame from the old generation updates nothing.
        core.note_negotiated(selection.generation, (640, 480, 15, 1));
        assert_eq!(
            core.capture.as_ref().and_then(|capture| capture.negotiated),
            None
        );
        assert!(!core.wants_capture(selection.generation));
        // Newer selection's leases still work; with one held, the newer
        // generation's capture is wanted.
        assert!(core.lease_publication(&next).is_ok());
        assert!(core.wants_capture(newer.generation));
    }

    #[test]
    fn frame_negotiation_is_recorded_for_lease_dimensions() {
        let mut core = core_with_cameras();
        let selection = core.begin_selection(Some("11"), None);
        core.capture_started(capture_for(selection.generation, 11))
            .unwrap();
        core.note_negotiated(selection.generation, (1280, 720, 30, 1));
        assert_eq!(core.snapshot().negotiated, Some((1280, 720, 30, 1)));
    }

    #[test]
    fn permission_strings_mirror_platform_permission_vocabulary() {
        assert_eq!(Permission::NotDetermined.as_str(), "not-determined");
        assert_eq!(Permission::Granted.as_str(), "authorized");
        assert_eq!(Permission::Denied.as_str(), "denied");
        // Default state claims nothing (X03).
        assert_eq!(OwnerCore::default().permission, Permission::NotDetermined);
    }
}
