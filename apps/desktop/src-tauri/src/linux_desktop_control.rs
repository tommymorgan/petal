//! Prepared Linux desktop authority. The common remote-control engine remains
//! the only grant/token store; this owner ties its admission to live EIS/capture.
use crate::linux_eis::Eis;
use crate::remote_control_core::{RemoteControlMessage, RemoteControlTargetKind};
use crate::sync_ext::MutexExt;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tokio_util::sync::CancellationToken;

/// How long an UNCLAIMED prepared desktop share may wait for a controller
/// request before the preparation ages out (human decision 2026-11-06:
/// unclaimed preparation may still age out quickly).
const PREPARED_UNCLAIMED_TIMEOUT: Duration = Duration::from_secs(300);

/// Sliding idle bound for an ACTIVE granted desktop session, renewed by
/// activation, by every admitted controller input, and by clipboard use
/// (human decision 2026-11-06: an actively granted session with a
/// connected controller must survive normal use indefinitely). A
/// controller that departs is retired through the roster-confirmed
/// disconnect path; this bound only reaps a session whose controller went
/// silent without any departure event reaching the host.
const ACTIVE_CONTROLLER_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Deadline policy for one desktop admission renewal. An ACTIVE granted
/// session moves onto the sliding idle window; unclaimed preparation and
/// consent-pending states keep the absolute `begin` bound — `min` with the
/// current deadline means no activity or request spam can ever EXTEND an
/// unclaimed preparation's life.
fn next_deadline(
    state: &LinuxDesktopControlState,
    now: Instant,
    current: Option<Instant>,
) -> Instant {
    if state.phase == "active" && state.controller_id.is_some() {
        return now + ACTIVE_CONTROLLER_IDLE_TIMEOUT;
    }
    current
        .unwrap_or_else(|| now + PREPARED_UNCLAIMED_TIMEOUT)
        .min(now + PREPARED_UNCLAIMED_TIMEOUT)
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LinuxDesktopControlState {
    phase: &'static str,
    pub(crate) window_id: Option<u32>,
    controller_id: Option<String>,
    controller_name: Option<String>,
    error: Option<String>,
}
impl Default for LinuxDesktopControlState {
    fn default() -> Self {
        Self {
            phase: "idle",
            window_id: None,
            controller_id: None,
            controller_name: None,
            error: None,
        }
    }
}
struct Authority {
    state: LinuxDesktopControlState,
    cancel: Option<CancellationToken>,
    eis: Option<Arc<Eis>>,
    app: Option<tauri::AppHandle>,
    deadline: Option<Instant>,
    clipboard: Option<Arc<crate::linux_host_clipboard::PortalClipboard>>,
    instance: String,
    controller_request: Option<String>,
}
static OWNER: LazyLock<Mutex<Authority>> = LazyLock::new(|| {
    Mutex::new(Authority {
        state: LinuxDesktopControlState::default(),
        cancel: None,
        eis: None,
        app: None,
        deadline: None,
        clipboard: None,
        instance: String::new(),
        controller_request: None,
    })
});
fn publish(owner: &Authority) {
    if let Some(app) = &owner.app {
        let _ = app.emit("linux-desktop-control-state-changed", owner.state.clone());
    }
}
pub(crate) fn snapshot() -> LinuxDesktopControlState {
    OWNER.lock_unpoisoned().state.clone()
}

/// The actively-controlled window when this host's single prepared desktop
/// share is under `controller_id`'s active grant, else None. Read by the
/// #820 stale-disconnect re-announce so a reconnecting controller's grants
/// are re-emitted for the desktop-control topology too, not only windows in
/// the generic remote-control grant map.
pub(crate) fn active_window_for_controller(controller_id: &str) -> Option<u32> {
    let owner = OWNER.lock_unpoisoned();
    (owner.state.phase == "active" && owner.state.controller_id.as_deref() == Some(controller_id))
        .then(|| owner.state.window_id)
        .flatten()
}
pub(crate) fn begin(app: &tauri::AppHandle, id: u32) -> Result<(), String> {
    let mut owner = OWNER.lock_unpoisoned();
    if !matches!(owner.state.phase, "idle" | "failed") {
        return Err("Desktop control is in use; stop the current controlled share first".into());
    }
    owner.app = Some(app.clone());
    owner.state = LinuxDesktopControlState {
        phase: "preparing",
        window_id: Some(id),
        ..Default::default()
    };
    owner.instance = id.to_string();
    owner.controller_request = None;
    owner.deadline = Some(Instant::now() + PREPARED_UNCLAIMED_TIMEOUT);
    publish(&owner);
    Ok(())
}
pub(crate) fn set_pending(id: u32, cancel: CancellationToken) -> Result<(), String> {
    let mut owner = OWNER.lock_unpoisoned();
    if owner.state.phase != "preparing" || owner.cancel.is_some() {
        return Err("Desktop preparation retired".into());
    }
    owner.state.window_id = Some(id);
    owner.instance = id.to_string();
    owner.cancel = Some(cancel);
    publish(&owner);
    Ok(())
}
pub(crate) fn ready(
    id: u32,
    eis: Arc<Eis>,
    clipboard: Option<Arc<crate::linux_host_clipboard::PortalClipboard>>,
) -> Result<(), String> {
    let mut owner = OWNER.lock_unpoisoned();
    if owner.state.window_id != Some(id)
        || owner.state.phase != "preparing"
        || !eis.is_live()
        || owner
            .deadline
            .is_none_or(|deadline| Instant::now() >= deadline)
    {
        return Err("Desktop preparation retired before commit".into());
    }
    owner.eis = Some(eis);
    owner.clipboard = clipboard;
    owner.state.phase = "ready";
    publish(&owner);
    Ok(())
}
pub(crate) fn finish(id: u32, error: Option<String>) {
    let mut owner = OWNER.lock_unpoisoned();
    if owner.state.window_id != Some(id) {
        return;
    }
    owner.eis.take();
    owner.cancel.take();
    owner.deadline.take();
    owner.clipboard.take();
    owner.controller_request = None;
    owner.state = LinuxDesktopControlState {
        phase: if error.is_some() { "failed" } else { "idle" },
        error,
        ..Default::default()
    };
    publish(&owner);
}
pub(crate) fn is_prepared(id: u32) -> bool {
    let owner = OWNER.lock_unpoisoned();
    owner.state.window_id == Some(id)
        && matches!(owner.state.phase, "ready" | "awaitingApproval" | "active")
        && owner.deadline.is_some_and(|d| Instant::now() < d)
        && owner.eis.as_ref().is_some_and(|e| e.is_live())
}
pub(crate) fn validate_request(message: &RemoteControlMessage) -> Result<(), String> {
    if !is_prepared(message.window_id) {
        return Err("Application windows are view-only; explicitly prepare a monitor for desktop control first".into());
    }
    let owner = OWNER.lock_unpoisoned();
    scope_admission(&owner.state, &owner.instance, message)?;
    if owner.state.phase == "active"
        && message.message_type == crate::remote_control_core::RemoteControlType::Request
        && crate::remote_control_core::remote_control_engine()
            .host_request_id(message.window_id, &message.controller_id)
            != message.request_id
    {
        return Err("Stop the current desktop grant before requesting a different attempt".into());
    }
    Ok(())
}
fn scope_admission(
    state: &LinuxDesktopControlState,
    instance: &str,
    message: &RemoteControlMessage,
) -> Result<(), String> {
    if state.window_id != Some(message.window_id)
        || !matches!(state.phase, "ready" | "awaitingApproval" | "active")
        || message.target_kind != Some(RemoteControlTargetKind::Display)
        || message.share_instance_id.as_deref() != Some(instance)
    {
        return Err("Desktop request names a retired or different share instance".into());
    }
    if state
        .controller_id
        .as_deref()
        .is_some_and(|id| id != message.controller_id)
    {
        return Err("Desktop control is in use by another named controller".into());
    }
    Ok(())
}

/// Host-side scope decision for a controller clipboard Copy request against
/// the prepared desktop share. The host — not the controller's echo — owns
/// the desktop scope, exactly like the paste stream path
/// (`apply_linux_clipboard_paste` binds `Display` + window id itself); an
/// echoed scope, when the controller sends one, must still match the
/// admitted desktop share exactly. The grant token (checked against the
/// engine's scoped key by `is_authorized_input`) provides the per-operation
/// correlation; `clipboard()`/`validate_request` re-validate live EIS
/// admission before any host effect.
pub(crate) fn clipboard_scope(
    state: &LinuxDesktopControlState,
    controller: &str,
    window_id: u32,
    target_kind: Option<RemoteControlTargetKind>,
    share_instance_id: Option<&str>,
) -> Result<(), String> {
    if state.window_id != Some(window_id)
        || !matches!(state.phase, "ready" | "awaitingApproval" | "active")
    {
        return Err("no explicitly prepared desktop share for this window".into());
    }
    if state
        .controller_id
        .as_deref()
        .is_some_and(|id| id != controller)
    {
        return Err("desktop control is in use by another named controller".into());
    }
    match (target_kind, share_instance_id) {
        (None, None) => Ok(()),
        (Some(RemoteControlTargetKind::Display), Some(instance))
            if instance == window_id.to_string() =>
        {
            Ok(())
        }
        _ => Err("request names a different desktop share instance than the one prepared".into()),
    }
}

/// Snapshot-only authority state for clipboard-scope tests. The EIS/clipboard
/// owners stay untouched: authorize-time decisions read only this snapshot;
/// execute-time `clipboard()`/`validate_request` remain the real host gates.
#[cfg(test)]
pub(crate) fn install_clipboard_scope_test_state(
    id: u32,
    phase: &'static str,
    controller: Option<&str>,
) {
    let mut owner = OWNER.lock_unpoisoned();
    owner.state = LinuxDesktopControlState {
        phase,
        window_id: Some(id),
        controller_id: controller.map(str::to_string),
        ..Default::default()
    };
}

#[cfg(test)]
pub(crate) fn reset_clipboard_scope_test_state() {
    OWNER.lock_unpoisoned().state = LinuxDesktopControlState::default();
}
pub(crate) fn status(
    id: u32,
    controller: &str,
    name: &str,
    request_id: Option<&str>,
    status: &str,
) {
    let mut owner = OWNER.lock_unpoisoned();
    if owner.state.window_id != Some(id) {
        return;
    }
    let changed = {
        let Authority {
            state,
            controller_request,
            ..
        } = &mut *owner;
        apply_controller_status(
            state,
            controller_request,
            controller,
            name,
            request_id,
            status,
        )
    };
    if !changed {
        return;
    }
    if owner.state.phase == "stopping" {
        if let Some(cancel) = &owner.cancel {
            cancel.cancel();
        }
        owner.eis.take();
        owner.clipboard.take();
    } else if owner.state.phase == "active" {
        // Activation (and every repeated active status — controller
        // presence evidence) renews the sliding idle window.
        owner.deadline = Some(next_deadline(&owner.state, Instant::now(), owner.deadline));
    }
    publish(&owner);
}

fn apply_controller_status(
    state: &mut LinuxDesktopControlState,
    attempt: &mut Option<String>,
    controller: &str,
    name: &str,
    request_id: Option<&str>,
    status: &str,
) -> bool {
    let Some(request_id) = request_id.filter(|id| !id.is_empty()) else {
        return false;
    };
    let matching = state.controller_id.as_deref() == Some(controller)
        && attempt.as_deref() == Some(request_id);
    match status {
        "awaitingConsent"
            if state.phase == "ready"
                || (state.phase == "awaitingApproval"
                    && state.controller_id.as_deref() == Some(controller)) =>
        {
            state.phase = "awaitingApproval";
            state.controller_id = Some(controller.into());
            state.controller_name = Some(name.into());
            *attempt = Some(request_id.into());
        }
        "active" if state.phase == "awaitingApproval" && matching => state.phase = "active",
        "denied" | "requestUnavailable" | "requestFailed" | "stopped" | "disabled"
            if state.phase == "awaitingApproval" && matching =>
        {
            state.phase = "ready";
            state.controller_id = None;
            state.controller_name = None;
            *attempt = None;
        }
        "stopped" | "disabled" if state.phase == "active" && matching => state.phase = "stopping",
        _ => return false,
    }
    true
}
pub(crate) fn retire(id: u32) {
    let mut owner = OWNER.lock_unpoisoned();
    if owner.state.window_id != Some(id) {
        return;
    }
    owner.state.phase = "stopping";
    // This is the first admission boundary. Pending native input rechecks this
    // cancellation before mutation; dropping EIS releases its live held state.
    if let Some(cancel) = &owner.cancel {
        cancel.cancel();
    }
    owner.eis.take();
    owner.clipboard.take();
    publish(&owner);
}

#[cfg(test)]
pub(crate) fn install_deadline_for_test(deadline: Option<Instant>) {
    OWNER.lock_unpoisoned().deadline = deadline;
}

#[cfg(test)]
pub(crate) fn deadline_for_test() -> Option<Instant> {
    OWNER.lock_unpoisoned().deadline
}
pub(crate) fn retire_controller(id: u32, controller: &str) {
    let current = OWNER.lock_unpoisoned().state.clone();
    if current.window_id == Some(id)
        && current.controller_id.as_deref() == Some(controller)
        && current.phase == "active"
    {
        retire(id);
    }
}

/// Departure-path coverage: retire the active admission held by
/// `controller_id` regardless of whether the generic remote-control grant
/// map recorded their window. A Linux desktop-control grant lives in THIS
/// authority; without this call a confirmed departure left the admission
/// active (measured live 2026-11-07: "in use by another named controller"
/// blocked the next controller for up to the 30-minute idle bound).
pub(crate) fn retire_controller_admission(controller_id: &str) {
    let id = {
        let owner = OWNER.lock_unpoisoned();
        (owner.state.phase == "active"
            && owner.state.controller_id.as_deref() == Some(controller_id))
        .then(|| owner.state.window_id)
        .flatten()
    };
    if let Some(id) = id {
        retire(id);
    }
}
pub(crate) fn retire_all() {
    let id = OWNER.lock_unpoisoned().state.window_id;
    if let Some(id) = id {
        retire(id);
    }
}
pub(crate) fn replay(message: &RemoteControlMessage) -> Result<(), String> {
    validate_request(message)?;
    if !crate::remote_control::is_authorized_input(message) {
        return Err("Desktop grant is no longer current".into());
    }
    let eis = {
        let mut owner = OWNER.lock_unpoisoned();
        if owner.state.phase != "active"
            || owner.state.controller_id.as_deref() != Some(message.controller_id.as_str())
        {
            return Err("No explicitly approved desktop controller".into());
        }
        // Every admitted controller input renews the sliding idle window.
        owner.deadline = Some(next_deadline(&owner.state, Instant::now(), owner.deadline));
        owner.eis.clone().ok_or("Desktop EIS session retired")?
    };
    eis.replay(message)
}

pub(crate) fn clipboard(
    message: &RemoteControlMessage,
) -> Result<Arc<crate::linux_host_clipboard::PortalClipboard>, String> {
    validate_request(message)?;
    let mut owner = OWNER.lock_unpoisoned();
    if owner.state.phase != "active"
        || owner.state.controller_id.as_deref() != Some(message.controller_id.as_str())
    {
        return Err("Desktop clipboard requires an active named controller".into());
    }
    // Clipboard use is controller activity: renew the sliding idle window.
    owner.deadline = Some(next_deadline(&owner.state, Instant::now(), owner.deadline));
    owner
        .clipboard
        .clone()
        .ok_or("Native desktop clipboard permission was not granted".into())
}

pub(crate) fn host_capabilities(
    id: u32,
) -> Vec<crate::remote_control_core::RemoteControlCapability> {
    use crate::remote_control_core::RemoteControlCapability::*;
    if !is_prepared(id) {
        return Vec::new();
    }
    let owner = OWNER.lock_unpoisoned();
    let mut capabilities = vec![
        LegacyControl,
        DiscretePointerV1,
        DiscreteScrollV1,
        ScopedPointerStreamV1,
        GlobalKeyboard,
    ];
    if owner.eis.as_ref().is_some_and(|eis| eis.supports_text()) {
        capabilities.push(UnicodeText);
    }
    capabilities
}

#[derive(Clone)]
struct NamedControllerConsent {
    window_id: u32,
    controller_id: String,
    controller_name: String,
    request_id: String,
    deadline: Instant,
}
impl NamedControllerConsent {
    fn new(
        window_id: u32,
        controller_id: String,
        controller_name: String,
        request_id: Option<String>,
    ) -> Option<Self> {
        Some(Self {
            window_id,
            controller_id,
            controller_name,
            request_id: request_id.filter(|id| !id.is_empty())?,
            deadline: Instant::now() + crate::remote_control::CONSENT_TIMEOUT,
        })
    }
    fn is_pending(&self, engine: &crate::remote_control_core::RemoteControlEngine) -> bool {
        Instant::now() < self.deadline
            && engine
                .pending_request_id(self.window_id, &self.controller_id)
                .as_deref()
                == Some(self.request_id.as_str())
    }
    fn detail(&self) -> String {
        format!("{} requests desktop-wide keyboard and pointer control, including desktop focus changes. The monitor picture does not confine keyboard input. Stopping control also ends this controlled monitor share. Allow only this named controller for this request?",
            self.controller_name)
    }
}

pub(crate) fn present_controller_consent(
    app: &tauri::AppHandle,
    payload: &crate::remote_control::ControlConsentRequestedPayload,
) {
    let Some(prompt) = NamedControllerConsent::new(
        payload.window_id,
        payload.controller_id.clone(),
        payload.controller_name.clone(),
        payload.request_id.clone(),
    ) else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let engine = crate::remote_control_core::remote_control_engine();
        if !prompt.is_pending(engine) {
            return;
        }
        let approved = confirm_pending(
            &app,
            "Allow named desktop controller?",
            &prompt.detail(),
            Some(prompt.clone()),
        )
        .await
        .is_ok()
            && prompt.is_pending(engine);
        let reason = if Instant::now() >= prompt.deadline {
            crate::remote_control_core::RemoteControlReason::ConsentTimedOut
        } else {
            crate::remote_control_core::RemoteControlReason::ConsentDenied
        };
        crate::remote_control::answer_consent(
            &app,
            prompt.window_id,
            &prompt.controller_id,
            &prompt.request_id,
            approved,
            reason,
        );
    });
}

/// App-owned, visible GTK confirmation. No caller-supplied acknowledgement.
/// Dismissal, timeout and stale targets all resolve to denial.
pub(crate) async fn confirm(
    app: &tauri::AppHandle,
    title: &str,
    detail: &str,
) -> Result<(), String> {
    confirm_pending(app, title, detail, None).await
}

async fn confirm_pending(
    app: &tauri::AppHandle,
    title: &str,
    detail: &str,
    pending: Option<NamedControllerConsent>,
) -> Result<(), String> {
    use gtk::prelude::*;
    let (answer, result) = tokio::sync::oneshot::channel();
    let title = title.to_string();
    let detail = detail.to_string();
    let app_main = app.clone();
    app.run_on_main_thread(move || {
        if pending.as_ref().is_some_and(|prompt| {
            !prompt.is_pending(crate::remote_control_core::remote_control_engine())
        }) {
            let _ = answer.send(false);
            return;
        }
        let parent = app_main
            .get_webview_window("main")
            .and_then(|w| w.gtk_window().ok());
        let dialog = gtk::MessageDialog::builder()
            .text(&title)
            .secondary_text(&detail)
            .message_type(gtk::MessageType::Warning)
            .modal(true)
            .build();
        if let Some(parent) = parent.as_ref() {
            dialog.set_transient_for(Some(parent));
        }
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Allow", gtk::ResponseType::Accept);
        dialog.set_default_response(gtk::ResponseType::Cancel);
        let answer = std::cell::RefCell::new(Some(answer));
        dialog.connect_response(move |dialog, response| {
            if let Some(answer) = answer.borrow_mut().take() {
                let _ = answer.send(response == gtk::ResponseType::Accept);
            }
            dialog.close();
        });
        let weak = dialog.downgrade();
        gtk::glib::timeout_add_local_once(Duration::from_secs(30), move || {
            if let Some(dialog) = weak.upgrade() {
                dialog.response(gtk::ResponseType::Cancel);
            }
        });
        if let Some(prompt) = pending {
            let weak = dialog.downgrade();
            gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
                let Some(dialog) = weak.upgrade() else {
                    return gtk::glib::ControlFlow::Break;
                };
                if !prompt.is_pending(crate::remote_control_core::remote_control_engine()) {
                    dialog.response(gtk::ResponseType::Cancel);
                    return gtk::glib::ControlFlow::Break;
                }
                gtk::glib::ControlFlow::Continue
            });
        }
        dialog.show_all();
    })
    .map_err(|e| format!("Cannot show desktop permission disclosure: {e}"))?;
    if result.await.unwrap_or(false) {
        Ok(())
    } else {
        Err("Desktop operation cancelled or consent expired".into())
    }
}
#[tauri::command]
pub(crate) fn linux_desktop_control_state() -> LinuxDesktopControlState {
    snapshot()
}
#[tauri::command]
pub(crate) async fn prepare_linux_desktop_control(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::session::SessionState>,
    window_id: u32,
) -> Result<LinuxDesktopControlState, String> {
    if !state.is_display_share(window_id) || state.active_share_frame(window_id).is_none() {
        return Err(
            "Application-window shares are view-only; select a current monitor share".into(),
        );
    }
    begin(&app, window_id)?;
    let result = async {
        confirm(&app, "Prepare desktop-wide remote control?",
            "Keyboard and other input can affect the entire desktop, including focus changes. The selected monitor picture does not confine keyboard input. Stopping control also ends this controlled monitor share. You will select one monitor again; a named controller still needs separate approval.").await?;
        let (connection, generation) = state.picture_connection_snapshot().ok_or("Join a room before preparing desktop control")?;
        state.pictures.start_control(app.clone(), connection, generation, window_id).await
    }.await;
    if let Err(error) = result {
        let id = snapshot().window_id.unwrap_or(window_id);
        finish(id, Some(error.clone()));
        return Err(error);
    }
    Ok(snapshot())
}
#[tauri::command]
pub(crate) async fn stop_linux_desktop_control(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::session::SessionState>,
    window_id: u32,
) -> Result<(), String> {
    if snapshot().window_id != Some(window_id) {
        return Err("This desktop-control share is no longer current".into());
    }
    retire(window_id);
    crate::remote_control::revoke_window(&app, window_id, "desktop control stopped");
    state.pictures.stop_one(&app, window_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_control_core::{test_support::input_message, RemoteControlType};
    #[test]
    fn desktop_scope_never_escalates_windows_or_reuses_retired_instances() {
        let mut state = LinuxDesktopControlState {
            phase: "ready",
            window_id: Some(42),
            ..Default::default()
        };
        let mut message = input_message(42, "host", RemoteControlType::Request);
        message.target_kind = Some(RemoteControlTargetKind::Window);
        message.share_instance_id = Some("42".into());
        assert!(scope_admission(&state, "42", &message).is_err());
        message.target_kind = Some(RemoteControlTargetKind::Display);
        assert!(scope_admission(&state, "42", &message).is_ok());
        message.share_instance_id = Some("previous".into());
        assert!(scope_admission(&state, "42", &message).is_err());
        message.share_instance_id = Some("42".into());
        state.phase = "awaitingApproval";
        state.controller_id = Some(message.controller_id.clone());
        assert!(scope_admission(&state, "42", &message).is_ok());
        message.controller_id = "competing-controller".into();
        assert!(scope_admission(&state, "42", &message).is_err());
        state.controller_id = None;
        for phase in ["idle", "preparing", "stopping", "failed"] {
            state.phase = phase;
            assert!(scope_admission(&state, "42", &message).is_err(), "{phase}");
        }
        state.phase = "ready";
        state.window_id = Some(43);
        assert!(scope_admission(&state, "43", &message).is_err());
    }
    #[test]
    fn clipboard_scope_binds_the_prepared_desktop_and_rejects_foreign_echoes() {
        let mut state = LinuxDesktopControlState {
            phase: "active",
            window_id: Some(42),
            ..Default::default()
        };
        // The host binds the scope itself when the controller echoes none.
        assert!(clipboard_scope(&state, "controller", 42, None, None).is_ok());
        // An echoed scope must name exactly the prepared desktop share.
        assert!(clipboard_scope(
            &state,
            "controller",
            42,
            Some(RemoteControlTargetKind::Display),
            Some("42")
        )
        .is_ok());
        assert!(clipboard_scope(
            &state,
            "controller",
            42,
            Some(RemoteControlTargetKind::Display),
            Some("previous")
        )
        .is_err());
        assert!(clipboard_scope(
            &state,
            "controller",
            42,
            Some(RemoteControlTargetKind::Window),
            Some("42")
        )
        .is_err());
        assert!(clipboard_scope(
            &state,
            "controller",
            42,
            Some(RemoteControlTargetKind::Display),
            None
        )
        .is_err());
        assert!(clipboard_scope(&state, "controller", 43, None, None).is_err());
        state.controller_id = Some("named".into());
        assert!(clipboard_scope(&state, "controller", 42, None, None).is_err());
        assert!(clipboard_scope(&state, "named", 42, None, None).is_ok());
        state.phase = "ready";
        assert!(clipboard_scope(&state, "named", 42, None, None).is_ok());
        state.phase = "awaitingApproval";
        assert!(clipboard_scope(&state, "named", 42, None, None).is_ok());
        for phase in ["idle", "preparing", "stopping", "failed"] {
            state.phase = phase;
            assert!(
                clipboard_scope(&state, "named", 42, None, None).is_err(),
                "{phase}"
            );
        }
    }
    #[test]
    fn next_deadline_pins_the_human_renewal_decision() {
        // Human decision 2026-11-06: an actively granted desktop session
        // must not expire after minutes. Live observations (2026-11-06):
        // every session died against ONE absolute 300s bound set in
        // `begin` — (b) ~4.9min after activation with input 100s before
        // expiry, (c) ~4.5min absolute — because nothing ever renewed it.
        let now = Instant::now();
        let mut state = LinuxDesktopControlState {
            phase: "active",
            window_id: Some(42),
            controller_id: Some("controller".into()),
            ..Default::default()
        };
        // Active + named controller: sliding idle window, far past any
        // absolute preparation bound.
        assert_eq!(
            next_deadline(&state, now, Some(now + PREPARED_UNCLAIMED_TIMEOUT)),
            now + ACTIVE_CONTROLLER_IDLE_TIMEOUT
        );
        assert_eq!(next_deadline(&state, now, None), now + ACTIVE_CONTROLLER_IDLE_TIMEOUT);
        // Unclaimed preparation keeps the absolute bound; activity can
        // never extend it (min with the current deadline).
        state.phase = "ready";
        state.controller_id = None;
        let soon = now + Duration::from_secs(60);
        assert_eq!(next_deadline(&state, now, Some(soon)), soon);
        assert_eq!(
            next_deadline(&state, now, Some(now + PREPARED_UNCLAIMED_TIMEOUT)),
            now + PREPARED_UNCLAIMED_TIMEOUT
        );
        assert_eq!(
            next_deadline(&state, now, None),
            now + PREPARED_UNCLAIMED_TIMEOUT
        );
        // A pending consent request must not extend the bound either —
        // request spam cannot keep an unclaimed preparation alive.
        state.phase = "awaitingApproval";
        state.controller_id = Some("controller".into());
        assert_eq!(next_deadline(&state, now, Some(soon)), soon);
        // Active without a named controller (transitional) falls back to
        // the unclaimed bound.
        state.phase = "active";
        state.controller_id = None;
        assert_eq!(next_deadline(&state, now, Some(soon)), soon);
    }

    #[test]
    fn activation_renews_the_deadline_instead_of_expiring_absolutely() {
        let _guard = crate::remote_control_core::test_support::state_test_lock();
        let now = Instant::now();
        install_clipboard_scope_test_state(42, "ready", None);
        install_deadline_for_test(Some(now + Duration::from_secs(60)));
        // Controller asks; consent is pending — the absolute bound holds.
        status(42, "controller", "Alice", Some("attempt-1"), "awaitingConsent");
        assert_eq!(deadline_for_test(), Some(now + Duration::from_secs(60)));
        // Sharer approves: the activation itself must move the session onto
        // the sliding idle window, not the dying preparation bound.
        status(42, "controller", "Alice", Some("attempt-1"), "active");
        assert_eq!(snapshot().phase, "active");
        assert!(
            deadline_for_test().is_some_and(|deadline| {
                deadline >= now + ACTIVE_CONTROLLER_IDLE_TIMEOUT
                    && deadline < now + ACTIVE_CONTROLLER_IDLE_TIMEOUT + Duration::from_secs(5)
            }),
            "activation must move the session onto the sliding idle window"
        );
        reset_clipboard_scope_test_state();
    }

    #[test]
    fn unclaimed_preparation_keeps_the_absolute_bound_through_request_spam() {
        let _guard = crate::remote_control_core::test_support::state_test_lock();
        let now = Instant::now();
        install_clipboard_scope_test_state(42, "ready", None);
        let soon = now + Duration::from_secs(60);
        install_deadline_for_test(Some(soon));
        status(42, "controller", "Alice", Some("attempt-1"), "awaitingConsent");
        status(42, "controller", "Alice", Some("attempt-1"), "awaitingConsent");
        assert_eq!(deadline_for_test(), Some(soon), "consent-pending must not extend");
        // Denial returns to unclaimed; the original bound still holds
        // (observation (a): unclaimed prep aged out on schedule).
        status(42, "controller", "Alice", Some("attempt-1"), "denied");
        assert_eq!(snapshot().phase, "ready");
        assert_eq!(deadline_for_test(), Some(soon));
        reset_clipboard_scope_test_state();
    }

    #[test]
    fn retire_controller_reaps_only_the_matching_active_admission() {
        let _guard = crate::remote_control_core::test_support::state_test_lock();
        install_clipboard_scope_test_state(42, "active", Some("controller"));
        retire_controller(42, "someone-else");
        assert_eq!(snapshot().phase, "active");
        // The confirmed-departure path (revoke_controller) relies on this
        // to retire a vanished controller's desktop admission with its
        // grant instead of waiting for any deadline to reap it.
        retire_controller(42, "controller");
        assert_eq!(snapshot().phase, "stopping");
        reset_clipboard_scope_test_state();
    }

    #[test]
    fn retire_controller_admission_covers_the_departure_gap() {
        let _guard = crate::remote_control_core::test_support::state_test_lock();
        // The exact live failure: the admission is active for a controller
        // with NO entry in the generic grant map, so revoke_controller's
        // per-window loop never reached it. The controller-wide retirement
        // must still reap it, and must not touch another controller's
        // admission.
        install_clipboard_scope_test_state(42, "active", Some("departed"));
        retire_controller_admission("someone-else");
        assert_eq!(snapshot().phase, "active", "a foreign controller must not retire it");
        retire_controller_admission("departed");
        assert_eq!(snapshot().phase, "stopping");
        reset_clipboard_scope_test_state();
    }
}

#[cfg(test)]
mod review_regressions {
    use super::*;
    use crate::remote_control_core::{
        test_support::input_message, ControlGrantKey, RemoteControlEngine, RemoteControlType,
    };
    #[test]
    fn named_native_consent_rejects_replaced_cancelled_expired_and_uncorrelated_attempts() {
        let engine = RemoteControlEngine::new();
        let mut message = input_message(42, "host", RemoteControlType::Request);
        message.request_id = Some("attempt-a".into());
        let key = ControlGrantKey::for_message(&message).unwrap();
        engine.store_pending_request(key, message.clone());
        let prompt = NamedControllerConsent::new(
            42,
            message.controller_id.clone(),
            "Alice".into(),
            Some("attempt-a".into()),
        )
        .unwrap();
        assert!(prompt.is_pending(&engine));
        message.request_id = Some("attempt-b".into());
        engine.store_pending_request(
            ControlGrantKey::for_message(&message).unwrap(),
            message.clone(),
        );
        assert!(
            !prompt.is_pending(&engine),
            "old dialog cannot approve a newer attempt"
        );
        let mut current = NamedControllerConsent::new(
            42,
            message.controller_id.clone(),
            "Alice".into(),
            message.request_id.clone(),
        )
        .unwrap();
        assert!(current.is_pending(&engine));
        current.deadline = Instant::now();
        assert!(
            !current.is_pending(&engine),
            "expiry cannot authorize even before the common timer runs"
        );
        let current = NamedControllerConsent::new(
            42,
            message.controller_id.clone(),
            "Alice".into(),
            message.request_id.clone(),
        )
        .unwrap();
        engine
            .take_pending_request(42, &message.controller_id, Some("attempt-b"))
            .unwrap();
        assert!(
            !current.is_pending(&engine),
            "cancelled requests no longer authorize a dialog response"
        );
        assert!(
            NamedControllerConsent::new(42, "controller".into(), "Alice".into(), None).is_none()
        );
    }
    #[test]
    fn pending_cancel_releases_only_its_exact_attempt_and_admits_next_controller() {
        let mut state = LinuxDesktopControlState {
            phase: "ready",
            window_id: Some(42),
            ..Default::default()
        };
        let mut attempt = None;
        assert!(apply_controller_status(
            &mut state,
            &mut attempt,
            "a",
            "Alice",
            Some("a1"),
            "awaitingConsent"
        ));
        assert!(apply_controller_status(
            &mut state,
            &mut attempt,
            "a",
            "Alice",
            Some("a2"),
            "awaitingConsent"
        ));
        assert!(!apply_controller_status(
            &mut state,
            &mut attempt,
            "a",
            "Alice",
            Some("a1"),
            "stopped"
        ));
        assert_eq!(state.phase, "awaitingApproval");
        assert!(apply_controller_status(
            &mut state,
            &mut attempt,
            "a",
            "Alice",
            Some("a2"),
            "stopped"
        ));
        assert_eq!(state.phase, "ready");
        assert_eq!(state.controller_id, None);
        assert!(apply_controller_status(
            &mut state,
            &mut attempt,
            "b",
            "Bob",
            Some("b1"),
            "awaitingConsent"
        ));
        assert!(!apply_controller_status(
            &mut state,
            &mut attempt,
            "a",
            "Alice",
            Some("a2"),
            "stopped"
        ));
        assert_eq!(state.controller_id.as_deref(), Some("b"));
    }
}
