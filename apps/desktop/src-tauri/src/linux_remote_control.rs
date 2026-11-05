//! Linux CONTROLLER-side remote control (Task 3.1).
//!
//! Linux may request, hold and use a Windows sharer's explicit grant, and
//! exchange separately authorized clipboard text as the controller. Linux
//! never acts as the remote-control HOST: request/consent/input packets
//! addressed to a local share stay refused (Task 4.1 owns that path), and
//! `remote_control::input::replay` keeps refusing real injection.
//!
//! Grant bookkeeping lives in the portable `remote_control_core` engine
//! (`controller_grants`), exactly like the Windows controller. This module
//! adds the Linux-shaped validation around it: a grant is installed only for
//! the CURRENT live received-share instance, and any retirement of that share
//! (stop, departure, replacement, leave) drops the grant so late packets and
//! responses cannot restore it.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::remote_control_core::{
    canonical_operation_fingerprint, remote_control_engine, ControllerGrantEnvelope,
    DiscreteAdmission, RemoteControlAction, RemoteControlCapability, RemoteControlMessage,
    RemoteControlTargetKind, RemoteControlType,
};
use crate::sync_ext::MutexExt;

/// Wire-facing shape of one live received share, maintained by
/// `linux_compositor` from real room events. Controller-side authority is
/// always resolved against this registry, never against packet claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReceiveTargetInfo {
    pub owner_identity: String,
    pub window_id: u32,
    pub target_kind: RemoteControlTargetKind,
    /// The sharer's opaque instance id; `None` for a legacy sharer that
    /// publishes none (envelope-less legacy request shape).
    pub share_instance_id: Option<String>,
}

type WindowKey = (String, u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControllerAuthority {
    Idle,
    Pending,
    Active,
    LegacyActive,
}

struct ReceiveTarget {
    info: ReceiveTargetInfo,
    authority: ControllerAuthority,
    reservation: Option<ControlRequestReservation>,
    /// Delivery guard for already-admitted GTK status commands, not authority.
    status_request_id: Option<String>,
}

// Always acquire this lock before the portable engine's lock. Target changes,
// request cancellation and status admission must be serialized with input.
static RECEIVE_TARGETS: LazyLock<Mutex<HashMap<WindowKey, ReceiveTarget>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Local publication task identity; wire request identity is an opaque UUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ControlRequestReservation(u64);

static NEXT_REQUEST_RESERVATION: AtomicU64 = AtomicU64::new(1);

/// Called on the GTK thread before spawning the request task. A globally
/// unique serial also prevents a retired/reopened target from reusing it.
pub(crate) fn reserve_control_request(
    owner_identity: &str,
    window_id: u32,
    request_id: Option<String>,
) -> Result<ControlRequestReservation, String> {
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    let target = targets
        .get_mut(&(owner_identity.to_string(), window_id))
        .ok_or_else(|| format!("remote window {window_id} is not open"))?;
    let reservation = ControlRequestReservation(
        NEXT_REQUEST_RESERVATION.fetch_add(1, Ordering::Relaxed),
    );
    target.reservation = Some(reservation);
    target.status_request_id = None;
    target.authority = ControllerAuthority::Idle;
    let id = remote_control_engine().begin_controller_request(window_id, owner_identity, request_id);
    target.status_request_id = Some(id);
    Ok(reservation)
}
pub(crate) fn reserved_control_request_id(
    owner: &str, window_id: u32, reservation: ControlRequestReservation,
) -> Option<String> {
    let targets = RECEIVE_TARGETS.lock_unpoisoned();
    let target = targets.get(&(owner.to_string(), window_id))?;
    if target.reservation != Some(reservation) { return None; }
    remote_control_engine().controller_request_id(window_id, owner)
}

pub(crate) fn control_request_is_current(
    owner_identity: &str,
    window_id: u32,
    reservation: ControlRequestReservation,
) -> bool {
    RECEIVE_TARGETS.lock_unpoisoned()
        .get(&(owner_identity.to_string(), window_id))
        .is_some_and(|target| target.reservation == Some(reservation))
}

pub(crate) fn control_request_can_publish(
    owner_identity: &str,
    window_id: u32,
    reservation: ControlRequestReservation,
) -> bool {
    RECEIVE_TARGETS.lock_unpoisoned()
        .get(&(owner_identity.to_string(), window_id))
        .is_some_and(|target| target.reservation == Some(reservation)
            && target.authority == ControllerAuthority::Pending)
}

/// Cleanup belongs only to the failing local task, never a newer request or
/// an already-approved grant. Retain its serial for queued UI failure checks.
pub(crate) fn fail_control_request(
    owner_identity: &str,
    window_id: u32,
    reservation: ControlRequestReservation,
) -> bool {
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    let Some(target) = targets.get_mut(&(owner_identity.to_string(), window_id)) else {
        return false;
    };
    if target.reservation != Some(reservation)
        || !matches!(target.authority, ControllerAuthority::Idle | ControllerAuthority::Pending)
    {
        return false;
    }
    target.authority = ControllerAuthority::Idle;
    remote_control_engine().remove_controller_grant(window_id, owner_identity);
    remote_control_engine().retire_controller_request(window_id, owner_identity);
    true
}

pub(crate) fn upsert_receive_target(info: ReceiveTargetInfo) -> Option<ReceiveTargetInfo> {
    let key = (info.owner_identity.clone(), info.window_id);
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    if let Some(previous) = targets.get(&key) {
        if previous.info == info {
            return Some(previous.info.clone());
        }
    }
    remote_control_engine().remove_controller_grant(info.window_id, &info.owner_identity);
    targets
        .insert(key, ReceiveTarget { info, authority: ControllerAuthority::Idle, reservation: None, status_request_id: None })
        .map(|previous| previous.info)
}

/// Synchronous test fixture cancellation; runtime callers must carry the
/// request identity captured by their originating action.
#[cfg(test)]
pub(crate) fn cancel_control_request(owner_identity: &str, window_id: u32) {
    if let Some(id) = remote_control_engine().controller_request_id(window_id, owner_identity) {
        cancel_control_request_for_attempt(owner_identity, window_id, &id);
    }
}
/// Command-bound cancellation: the target lock covers identity admission and
/// local authority retirement, so a newer reservation/grant survives old work.
pub(crate) fn cancel_control_request_for_attempt(owner: &str, window_id: u32, request_id: &str) -> bool {
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    let Some(target) = targets.get_mut(&(owner.to_string(), window_id)) else { return false; };
    if !remote_control_engine().cancel_controller_request(window_id, owner, request_id) {
        return false;
    }
    target.authority = ControllerAuthority::Idle;
    target.reservation = None;
    // Permit the exact queued stopped presentation; the next reserve clears it.
    target.status_request_id = Some(request_id.to_string());
    true
}
/// Atomically retire only the still-pending timed-out attempt, never a successor
/// or an approval that arrived before the queued timeout command.
pub(crate) fn time_out_control_request(owner: &str, window_id: u32, request_id: &str) -> bool {
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    let Some(target) = targets.get_mut(&(owner.to_string(), window_id)) else { return false; };
    if !remote_control_engine().retire_pending_controller_request(window_id, owner, request_id) {
        return false;
    }
    target.authority = ControllerAuthority::Idle;
    target.status_request_id = None;
    target.reservation = None;
    remote_control_engine().remove_controller_grant(window_id, owner);
    true
}

pub(crate) fn controller_is_active(owner_identity: &str, window_id: u32) -> bool {
    RECEIVE_TARGETS.lock_unpoisoned()
        .get(&(owner_identity.to_string(), window_id))
        .is_some_and(|target| matches!(target.authority,
            ControllerAuthority::Active | ControllerAuthority::LegacyActive))
}

/// Resolve the live receive target for one remote window. `owner_hint` (from
/// the viewer surface) must name an actually-open share; without a hint the
/// window id must be unambiguous.
pub(crate) fn receive_target(
    window_id: u32,
    owner_hint: Option<&str>,
) -> Option<ReceiveTargetInfo> {
    let targets = RECEIVE_TARGETS.lock_unpoisoned();
    if let Some(owner) = owner_hint {
        if !owner.is_empty() {
            return targets.get(&(owner.to_string(), window_id)).map(|target| target.info.clone());
        }
    }
    let mut matches = targets.values().map(|target| &target.info).filter(|t| t.window_id == window_id);
    let only = matches.next().cloned();
    if matches.next().is_some() {
        // Ambiguous without a hint: refuse rather than guess an owner.
        return None;
    }
    only
}

pub(crate) fn receive_targets() -> Vec<ReceiveTargetInfo> {
    RECEIVE_TARGETS
        .lock_unpoisoned()
        .values()
        .map(|target| target.info.clone())
        .collect()
}

/// Controller capabilities advertised on a request, mirroring the Windows
/// controller so a Windows host negotiates the same routes.
pub(crate) fn controller_capabilities(
    target_kind: RemoteControlTargetKind,
) -> Vec<RemoteControlCapability> {
    match target_kind {
        RemoteControlTargetKind::Window => vec![
            RemoteControlCapability::LegacyControl,
            RemoteControlCapability::DiscretePointerV1,
            RemoteControlCapability::DiscreteScrollV1,
            RemoteControlCapability::WindowLocalPointer,
            RemoteControlCapability::GlobalKeyboard,
            RemoteControlCapability::UiaInvoke,
            RemoteControlCapability::UnicodeText,
        ],
        RemoteControlTargetKind::Display => vec![
            RemoteControlCapability::LegacyControl,
            RemoteControlCapability::DiscretePointerV1,
            RemoteControlCapability::DiscreteScrollV1,
            RemoteControlCapability::GlobalKeyboard,
            RemoteControlCapability::UnicodeText,
        ],
        RemoteControlTargetKind::Unknown => Vec::new(),
    }
}

/// Fill a Request message from the live receive target. A legacy sharer
/// without a share instance keeps the legacy envelope-less shape (macOS
/// parity); a capable sharer gets the negotiated envelope the Windows host
/// requires before it will even park a request for consent.
pub(crate) fn prepare_control_request(
    message: &mut RemoteControlMessage,
    window_id: u32,
    owner_identity: &str,
    reservation: ControlRequestReservation,
    request_id: Option<String>,
) -> Result<(), String> {
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    let target = targets
        .get_mut(&(owner_identity.to_string(), window_id))
        .ok_or_else(|| format!("remote window {window_id} is not open"))?;
    if target.reservation != Some(reservation) {
        return Err("remote control request was cancelled or superseded".to_string());
    }
    let info = &target.info;
    if let Some(share_instance_id) = info.share_instance_id.clone() {
        message.target_kind = Some(info.target_kind);
        message.share_instance_id = Some(share_instance_id);
        message.controller_capabilities = controller_capabilities(info.target_kind);
    }
    let id = remote_control_engine().controller_request_id(window_id, owner_identity)
        .ok_or_else(|| "remote control request was cancelled or superseded".to_string())?;
    if request_id.as_ref().is_some_and(|expected| expected != &id) {
        return Err("remote control request was superseded".to_string());
    }
    message.request_id = Some(id);
    target.authority = ControllerAuthority::Pending;
    Ok(())
}

/// The controller-grant token currently active for one remote window, if any.
/// Clipboard commands require it as a separate per-operation authorization.
pub(crate) fn active_grant_token(window_id: u32, owner_identity: &str) -> Option<String> {
    let targets = RECEIVE_TARGETS.lock_unpoisoned();
    if targets.get(&(owner_identity.to_string(), window_id))?.authority != ControllerAuthority::Active {
        return None;
    }
    remote_control_engine()
        .controller_grant(window_id, owner_identity)
        .map(|grant| grant.grant_token)
}
pub(crate) fn controller_status_is_current(
    owner: &str, window_id: u32, request_id: &str,
) -> bool {
    RECEIVE_TARGETS.lock_unpoisoned().get(&(owner.to_string(), window_id))
        .is_some_and(|target| target.status_request_id.as_deref() == Some(request_id))
}

/// Admit only the exact locally live request and an identical active grant.
pub(crate) fn record_controller_status(message: &RemoteControlMessage, status: &str) -> bool {
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    let Some(target) = targets.get_mut(&(message.controller_id.clone(), message.window_id)) else {
        return false;
    };
    let engine = remote_control_engine();
    if target.authority == ControllerAuthority::Idle || !engine.controller_response_matches(message) {
        return false;
    }
    match status {
        "stopped" | "disabled" => {
            target.authority = ControllerAuthority::Idle;
            target.reservation = None;
            engine.retire_controller_request(message.window_id, &message.controller_id);
            engine.remove_controller_grant(message.window_id, &message.controller_id);
            return true;
        }
        "active" => {}
        _ => {
            if status != "awaitingConsent" && target.authority == ControllerAuthority::Pending {
                target.authority = ControllerAuthority::Idle;
                target.reservation = None;
                engine.retire_controller_request(message.window_id, &message.controller_id);
            }
            return true;
        }
    }

    if !matches!(target.authority,
        ControllerAuthority::Pending | ControllerAuthority::Active | ControllerAuthority::LegacyActive)
    {
        return false;
    }
    let has_negotiated_envelope = message.target_kind.is_some()
        || message.share_instance_id.is_some()
        || message.control_session_id.is_some()
        || message.result_capability.is_some()
        || !message.host_capabilities.is_empty();
    if !has_negotiated_envelope {
        if target.info.share_instance_id.is_some()
            || target.authority == ControllerAuthority::Active
        {
            return false;
        }
        target.authority = ControllerAuthority::LegacyActive;
        engine.activate_controller_request(message);
        return true;
    }

    let (Some(target_kind), Some(share_instance_id), Some(control_session_id), Some(grant_token)) = (
        message.target_kind,
        message.share_instance_id.clone(),
        message.control_session_id.clone(),
        message.grant_token.clone(),
    ) else {
        return false;
    };
    let reliable_result = message
        .result_capability
        .as_ref()
        .is_some_and(|capability| capability.version == 2 && !capability.retry_enabled);
    if target.info.target_kind != target_kind
        || target.info.share_instance_id.as_deref() != Some(share_instance_id.as_str())
        || message.host_capabilities.is_empty()
        || !reliable_result
    {
        return false;
    }
    if target.authority != ControllerAuthority::Pending {
        return target.authority == ControllerAuthority::Active
            && engine.controller_grant(message.window_id, &message.controller_id).is_some_and(|grant| {
                grant.target_kind == target_kind
                    && grant.share_instance_id == share_instance_id
                    && grant.control_session_id == control_session_id
                    && grant.grant_token == grant_token
                    && grant.host_capabilities == message.host_capabilities
            });
    }

    engine.install_controller_grant(
        message.window_id,
        message.controller_id.clone(),
        ControllerGrantEnvelope {
            target_kind,
            share_instance_id,
            control_session_id,
            grant_token,
            full_pointer: false,
            host_capabilities: message.host_capabilities.clone(),
            next_input_seq: 1,
        },
    );
    target.authority = ControllerAuthority::Active;
    engine.activate_controller_request(message);
    true
}

/// Controller-side grant cleanup when a controlled share retires. Dropping
/// the engine grant is what makes later input sends refuse locally and stale
/// packets meaningless on the host.
pub(crate) fn retire_controller_for(owner_identity: &str, window_id: u32) -> bool {
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    targets.remove(&(owner_identity.to_string(), window_id));
    remote_control_engine().retire_controller_request(window_id, owner_identity);
    remote_control_engine()
        .remove_controller_grant(window_id, owner_identity)
        .is_some()
}

pub(crate) fn retire_controllers_for_owner(owner_identity: &str) -> usize {
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    let engine = remote_control_engine();
    let mut retired = 0;
    targets.retain(|(owner, window_id), _| {
        if owner != owner_identity {
            return true;
        }
        engine.retire_controller_request(*window_id, owner);
        if engine.remove_controller_grant(*window_id, owner).is_some() {
            retired += 1;
        }
        false
    });
    retired
}

/// Room teardown: no channel exists, so every controller grant is void.
pub(crate) fn retire_all_controllers() {
    let mut targets = RECEIVE_TARGETS.lock_unpoisoned();
    targets.clear();
    remote_control_engine().clear_controller_grants();
}

fn random_input_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS CSPRNG unavailable for remote-control token");
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut value, "{byte:02x}").expect("writing to String cannot fail");
    }
    value
}

/// Stamp a controller input message with the active grant. Mirrors the
/// Windows controller: the capability matrix decides whether the op is
/// supported; discrete ops (Click/Wheel/Key/Text) get the v2
/// admission envelope; the lossy drag stream (Move/Down/Up) keeps the
/// legacy grant-token-only shape. Unlike Windows, no pending-operation map
/// is kept: results are advisory feedback (retry stays disabled).
pub(crate) fn prepare_outbound_input(message: &mut RemoteControlMessage) -> Result<bool, String> {
    let targets = RECEIVE_TARGETS.lock_unpoisoned();
    let Some(target) = targets.get(&(message.target_user_id.clone(), message.window_id)) else {
        return Ok(false);
    };
    match target.authority {
        ControllerAuthority::LegacyActive => return Ok(true),
        ControllerAuthority::Active => {}
        ControllerAuthority::Idle | ControllerAuthority::Pending => return Ok(false),
    }
    let Some(grant) =
        remote_control_engine().next_controller_grant(message.window_id, &message.target_user_id)
    else {
        return Ok(false);
    };
    message.target_kind = Some(grant.target_kind);
    message.share_instance_id = Some(grant.share_instance_id.clone());
    message.host_capabilities = grant.host_capabilities.clone();
    message.grant_token = Some(grant.grant_token.clone());

    let supported = match (message.message_type, message.action) {
        (
            RemoteControlType::Pointer,
            Some(RemoteControlAction::Move | RemoteControlAction::Down),
        ) => grant
            .host_capabilities
            .contains(&RemoteControlCapability::DiscretePointerV1),
        (RemoteControlType::Pointer, Some(RemoteControlAction::Up)) => {
            if grant
                .host_capabilities
                .contains(&RemoteControlCapability::UiaInvoke)
            {
                message.action = Some(RemoteControlAction::Click);
                message.buttons = Some(0);
                true
            } else {
                grant
                    .host_capabilities
                    .contains(&RemoteControlCapability::DiscretePointerV1)
            }
        }
        (RemoteControlType::Pointer, Some(RemoteControlAction::Click)) => {
            grant
                .host_capabilities
                .contains(&RemoteControlCapability::UiaInvoke)
                || grant
                    .host_capabilities
                    .contains(&RemoteControlCapability::DiscretePointerV1)
        }
        (RemoteControlType::Wheel, _) => {
            grant
                .host_capabilities
                .contains(&RemoteControlCapability::DiscreteScrollV1)
                || grant
                    .host_capabilities
                    .contains(&RemoteControlCapability::UiaScroll)
        }
        (RemoteControlType::Key, _) => grant
            .host_capabilities
            .contains(&RemoteControlCapability::GlobalKeyboard),
        (RemoteControlType::Text, _) => grant
            .host_capabilities
            .contains(&RemoteControlCapability::UnicodeText),
        _ => false,
    };
    if !supported {
        return Ok(false);
    }

    let v2_discrete = matches!(
        (message.message_type, message.action),
        (RemoteControlType::Pointer, Some(RemoteControlAction::Click))
            | (RemoteControlType::Wheel, _)
            | (
                RemoteControlType::Key,
                Some(RemoteControlAction::Down | RemoteControlAction::Up)
            )
            | (RemoteControlType::Text, _)
    );
    if !v2_discrete {
        // Keep exact desktop scope only when the host negotiated the scoped
        // continuous stream; older hosts retain their legacy-token shape.
        crate::remote_control_core::prepare_continuous_pointer_envelope(message);
        return Ok(true);
    }

    message.control_session_id = Some(grant.control_session_id.clone());
    message.input_id = Some(random_input_id());
    message.input_seq = Some(grant.next_input_seq);
    message.operation_fingerprint_version = Some(1);
    let admission = DiscreteAdmission {
        controller_id: message.controller_id.clone(),
        window_id: message.window_id,
        target_kind: message.target_kind,
        share_instance_id: message.share_instance_id.clone(),
        control_session_id: grant.control_session_id.clone(),
        input_id: message.input_id.clone().expect("input id assigned"),
        input_seq: grant.next_input_seq,
        operation_fingerprint: String::new(),
    };
    message.operation_fingerprint = Some(canonical_operation_fingerprint(message, &admission));
    Ok(true)
}

/// GTK keyval name → DOM `KeyboardEvent.key` / `.code` / `.location`.
/// Windows-targeted replay consumes the DOM-shaped names the macOS/Windows
/// controllers already send, so GTK's `_L`/`_R` suffixes map onto locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyFields {
    pub key: String,
    pub code: String,
    pub location: u8,
}

pub(crate) fn dom_key_fields(keyval_name: &str, unicode: Option<char>) -> Option<KeyFields> {
    let named = |key: &str, code: &str| {
        Some(KeyFields {
            key: key.to_string(),
            code: code.to_string(),
            location: 0,
        })
    };
    match keyval_name {
        "Return" | "KP_Enter" => named("Enter", "Enter"),
        "Escape" => named("Escape", "Escape"),
        "BackSpace" => named("Backspace", "Backspace"),
        "Tab" | "KP_Tab" => named("Tab", "Tab"),
        "space" => named(" ", "Space"),
        "Delete" | "KP_Delete" => named("Delete", "Delete"),
        "Insert" | "KP_Insert" => named("Insert", "Insert"),
        "Home" | "KP_Home" => named("Home", "Home"),
        "End" | "KP_End" => named("End", "End"),
        "Page_Up" | "KP_Page_Up" => named("PageUp", "PageUp"),
        "Page_Down" | "KP_Page_Down" => named("PageDown", "PageDown"),
        "Left" | "KP_Left" => named("ArrowLeft", "ArrowLeft"),
        "Right" | "KP_Right" => named("ArrowRight", "ArrowRight"),
        "Up" | "KP_Up" => named("ArrowUp", "ArrowUp"),
        "Down" | "KP_Down" => named("ArrowDown", "ArrowDown"),
        "Shift_L" => Some(KeyFields {
            key: "Shift".into(),
            code: "ShiftLeft".into(),
            location: 1,
        }),
        "Shift_R" => Some(KeyFields {
            key: "Shift".into(),
            code: "ShiftRight".into(),
            location: 2,
        }),
        "Control_L" => Some(KeyFields {
            key: "Control".into(),
            code: "ControlLeft".into(),
            location: 1,
        }),
        "Control_R" => Some(KeyFields {
            key: "Control".into(),
            code: "ControlRight".into(),
            location: 2,
        }),
        "Alt_L" => Some(KeyFields {
            key: "Alt".into(),
            code: "AltLeft".into(),
            location: 1,
        }),
        "Alt_R" => Some(KeyFields {
            key: "Alt".into(),
            code: "AltRight".into(),
            location: 2,
        }),
        "Meta_L" | "Super_L" => Some(KeyFields {
            key: "Meta".into(),
            code: "MetaLeft".into(),
            location: 1,
        }),
        "Meta_R" | "Super_R" => Some(KeyFields {
            key: "Meta".into(),
            code: "MetaRight".into(),
            location: 2,
        }),
        "Caps_Lock" => named("CapsLock", "CapsLock"),
        "Num_Lock" => named("NumLock", "NumLock"),
        "Scroll_Lock" => named("ScrollLock", "ScrollLock"),
        "Pause" => named("Pause", "Pause"),
        "Print" => named("PrintScreen", "PrintScreen"),
        "F1" => named("F1", "F1"),
        "F2" => named("F2", "F2"),
        "F3" => named("F3", "F3"),
        "F4" => named("F4", "F4"),
        "F5" => named("F5", "F5"),
        "F6" => named("F6", "F6"),
        "F7" => named("F7", "F7"),
        "F8" => named("F8", "F8"),
        "F9" => named("F9", "F9"),
        "F10" => named("F10", "F10"),
        "F11" => named("F11", "F11"),
        "F12" => named("F12", "F12"),
        _ => {
            // Keypad digits keep DOM keypad codes.
            let keypad = match keyval_name {
                "KP_0" => Some(("0", "Numpad0")),
                "KP_1" => Some(("1", "Numpad1")),
                "KP_2" => Some(("2", "Numpad2")),
                "KP_3" => Some(("3", "Numpad3")),
                "KP_4" => Some(("4", "Numpad4")),
                "KP_5" => Some(("5", "Numpad5")),
                "KP_6" => Some(("6", "Numpad6")),
                "KP_7" => Some(("7", "Numpad7")),
                "KP_8" => Some(("8", "Numpad8")),
                "KP_9" => Some(("9", "Numpad9")),
                "KP_Multiply" => Some(("*", "NumpadMultiply")),
                "KP_Add" => Some(("+", "NumpadAdd")),
                "KP_Subtract" => Some(("-", "NumpadSubtract")),
                "KP_Decimal" => Some((".", "NumpadDecimal")),
                "KP_Divide" => Some(("/", "NumpadDivide")),
                _ => None,
            };
            if let Some((key, code)) = keypad {
                return Some(KeyFields {
                    key: key.into(),
                    code: code.into(),
                    location: 3,
                });
            }
            // Printable: DOM `key` is the produced text; `code` is the
            // physical key, which for Latin layouts is Key<A-Z>/Digit<0-9>.
            let produced = unicode?;
            let code = if produced.is_ascii_alphabetic() {
                format!("Key{}", produced.to_ascii_uppercase())
            } else if produced.is_ascii_digit() {
                format!("Digit{produced}")
            } else {
                match produced {
                    '-' | '_' => "Minus",
                    '=' | '+' => "Equal",
                    '[' | '{' => "BracketLeft",
                    ']' | '}' => "BracketRight",
                    '\\' | '|' => "Backslash",
                    ';' | ':' => "Semicolon",
                    '\'' | '"' => "Quote",
                    ',' | '<' => "Comma",
                    '.' | '>' => "Period",
                    '/' | '?' => "Slash",
                    '`' | '~' => "Backquote",
                    '!' => "Digit1",
                    '@' => "Digit2",
                    '#' => "Digit3",
                    '$' => "Digit4",
                    '%' => "Digit5",
                    '^' => "Digit6",
                    '&' => "Digit7",
                    '*' => "Digit8",
                    '(' => "Digit9",
                    ')' => "Digit0",
                    _ => return Some(KeyFields {
                        key: produced.to_string(),
                        code: produced.to_string(),
                        location: 0,
                    }),
                }.to_string()
            };
            Some(KeyFields {
                key: produced.to_string(),
                code,
                location: 0,
            })
        }
    }
}

/// Inverse of the compositor's letterbox fit: GTK-area-local point →
/// normalized 0..1 source coordinates, `None` outside the contained media
/// rect. Mirrors `normalizedControlPoint` in the web control overlay.
pub(crate) fn normalized_control_point(
    area_width: f64,
    area_height: f64,
    source_width: u32,
    source_height: u32,
    point: (f64, f64),
) -> Option<(f64, f64)> {
    if area_width <= 0.0 || area_height <= 0.0 || source_width == 0 || source_height == 0 {
        return None;
    }
    let source_width = f64::from(source_width);
    let source_height = f64::from(source_height);
    let bounds_aspect = area_width / area_height;
    let media_aspect = source_width / source_height;
    let (left, top, width, height) = if media_aspect > bounds_aspect {
        let height = area_width / media_aspect;
        (0.0, (area_height - height) / 2.0, area_width, height)
    } else {
        let width = area_height * media_aspect;
        ((area_width - width) / 2.0, 0.0, width, area_height)
    };
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let clamp01 = |value: f64| value.clamp(0.0, 1.0);
    Some((
        clamp01((point.0 - left) / width),
        clamp01((point.1 - top) / height),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_control_core::test_support::{
        controller_status_packet, input_message, state_test_lock,
    };

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        state_test_lock()
    }

    fn target(owner: &str, window_id: u32, instance: Option<&str>) {
        upsert_receive_target(ReceiveTargetInfo {
            owner_identity: owner.to_string(),
            window_id,
            target_kind: RemoteControlTargetKind::Window,
            share_instance_id: instance.map(str::to_string),
        });
    }

    fn request(owner: &str, window_id: u32) {
        let mut message = input_message(window_id, owner, RemoteControlType::Request);
        prepare_control_request(&mut message, window_id, owner).unwrap();
    }

    fn prepare_control_request(
        message: &mut RemoteControlMessage,
        window_id: u32,
        owner: &str,
    ) -> Result<(), String> {
        let reservation = reserve_control_request(owner, window_id, None)?;
        super::prepare_control_request(message, window_id, owner, reservation, None)
    }

    #[test]
    fn cancelled_reservation_cannot_prepare_or_publish_after_release() {
        let _guard = test_lock();
        let owner = "delayed-request-host";
        let window_id = 0x4100_0030;
        target(owner, window_id, Some("instance-a"));
        // GTK reserves synchronously, but its spawned task has not been polled.
        let delayed = reserve_control_request(owner, window_id, None).unwrap();
        cancel_control_request(owner, window_id);
        let mut message = input_message(window_id, owner, RemoteControlType::Request);
        assert!(super::prepare_control_request(&mut message, window_id, owner, delayed, None).is_err());
        assert!(!control_request_can_publish(owner, window_id, delayed));
        let active = controller_status_packet(window_id, owner, "active", "grant", "instance-a");
        assert!(!record_controller_status(&active, "active"));
        let mut input = input_message(window_id, owner, RemoteControlType::Wheel);
        assert_eq!(prepare_outbound_input(&mut input), Ok(false));

        let fresh = reserve_control_request(owner, window_id, None).unwrap();
        super::prepare_control_request(&mut message, window_id, owner, fresh, None).unwrap();
        assert!(control_request_can_publish(owner, window_id, fresh));
        assert!(!fail_control_request(owner, window_id, delayed));
        assert!(control_request_can_publish(owner, window_id, fresh));
        let active = controller_status_packet(window_id, owner, "active", "grant", "instance-a");
        assert!(record_controller_status(&active, "active"));
        assert_eq!(prepare_outbound_input(&mut input), Ok(true));
        retire_all_controllers();
    }

    #[test]
    fn cancelled_prepared_request_cannot_retry_or_fail_a_newer_request() {
        let _guard = test_lock();
        let owner = "retry-request-host";
        let window_id = 0x4100_0031;
        target(owner, window_id, Some("instance-a"));
        let old = reserve_control_request(owner, window_id, None).unwrap();
        let mut message = input_message(window_id, owner, RemoteControlType::Request);
        super::prepare_control_request(&mut message, window_id, owner, old, None).unwrap();
        assert!(control_request_can_publish(owner, window_id, old));
        // Publication failed and is sleeping before a retry; GTK cancels.
        cancel_control_request(owner, window_id);
        assert!(!control_request_can_publish(owner, window_id, old));
        let fresh = reserve_control_request(owner, window_id, None).unwrap();
        super::prepare_control_request(&mut message, window_id, owner, fresh, None).unwrap();
        assert!(!fail_control_request(owner, window_id, old));
        assert!(!control_request_is_current(owner, window_id, old));
        assert!(control_request_can_publish(owner, window_id, fresh));
        assert!(fail_control_request(owner, window_id, fresh));
        assert!(!control_request_can_publish(owner, window_id, fresh));
        assert!(control_request_is_current(owner, window_id, fresh));
        let newer = reserve_control_request(owner, window_id, None).unwrap();
        assert!(!control_request_is_current(owner, window_id, fresh),
            "queued failure feedback must not reset a newer GTK request");
        super::prepare_control_request(&mut message, window_id, owner, newer, None).unwrap();
        let active = controller_status_packet(window_id, owner, "active", "grant", "instance-a");
        assert!(record_controller_status(&active, "active"));
        assert!(!fail_control_request(owner, window_id, newer),
            "a publish error must not revoke an already-approved grant");
        retire_all_controllers();
    }

    #[test]
    fn a_late_active_status_cannot_restore_a_revoked_grant() {
        let _guard = test_lock();
        let owner = "revoked-host";
        let window_id = 0x4100_0020;
        target(owner, window_id, Some("instance-a"));
        let mut request = input_message(window_id, owner, RemoteControlType::Request);
        prepare_control_request(&mut request, window_id, owner).unwrap();
        let active =
            controller_status_packet(window_id, owner, "active", "old-grant", "instance-a");
        assert!(record_controller_status(&active, "active"));
        let stopped =
            controller_status_packet(window_id, owner, "stopped", "old-grant", "instance-a");
        assert!(record_controller_status(&stopped, "stopped"));
        assert!(!record_controller_status(&active, "active"));
        assert_eq!(active_grant_token(window_id, owner), None);
        let mut input = input_message(window_id, owner, RemoteControlType::Wheel);
        assert_eq!(prepare_outbound_input(&mut input), Ok(false));
        retire_all_controllers();
    }

    #[test]
    fn a_repeated_active_status_does_not_rewind_the_input_sequence() {
        let _guard = test_lock();
        let owner = "repeated-status-host";
        let window_id = 0x4100_0021;
        target(owner, window_id, Some("instance-a"));
        let mut request = input_message(window_id, owner, RemoteControlType::Request);
        prepare_control_request(&mut request, window_id, owner).unwrap();
        let active =
            controller_status_packet(window_id, owner, "active", "grant", "instance-a");
        assert!(record_controller_status(&active, "active"));
        let mut first = input_message(window_id, owner, RemoteControlType::Wheel);
        assert_eq!(prepare_outbound_input(&mut first), Ok(true));
        assert_eq!(first.input_seq, Some(1));
        assert!(record_controller_status(&active, "active"));
        let mut second = input_message(window_id, owner, RemoteControlType::Wheel);
        assert_eq!(prepare_outbound_input(&mut second), Ok(true));
        assert_eq!(second.input_seq, Some(2));
        retire_all_controllers();
    }
    #[test]
    fn old_attempt_statuses_cannot_activate_or_retire_a_successor_request() {
        let _guard = test_lock();
        let owner = "exact-attempt-host";
        let window_id = 0x4100_0032;
        target(owner, window_id, Some("same-share"));
        request(owner, window_id);
        let old = controller_status_packet(window_id, owner, "active", "old-grant", "same-share");
        cancel_control_request(owner, window_id);
        request(owner, window_id);
        let fresh = controller_status_packet(window_id, owner, "active", "fresh-grant", "same-share");
        assert_ne!(old.request_id, fresh.request_id);
        assert!(!controller_status_is_current(owner, window_id, old.request_id.as_deref().unwrap()),
            "an already-queued GTK status for A cannot apply to B");
        assert!(!time_out_control_request(owner, window_id, old.request_id.as_deref().unwrap()),
            "a queued native timeout command for A cannot retire pending B");
        let mut input = input_message(window_id, owner, RemoteControlType::Wheel);
        for status in ["active", "awaitingConsent", "denied", "requestFailed", "stopped", "disabled"] {
            assert!(!record_controller_status(&old, status), "{status} A while B pending");
            assert!(!controller_is_active(owner, window_id));
            assert_eq!(prepare_outbound_input(&mut input), Ok(false));
        }
        let mut missing = fresh.clone();
        missing.request_id = None;
        assert!(!record_controller_status(&missing, "active"));
        assert!(record_controller_status(&fresh, "active"));
        assert!(!time_out_control_request(owner, window_id, fresh.request_id.as_deref().unwrap()),
            "a queued B timeout cannot revoke B after its genuine approval");
        assert_eq!(prepare_outbound_input(&mut input), Ok(true));
        assert_eq!(input.input_seq, Some(1));
        for status in ["active", "denied", "requestFailed", "stopped", "disabled"] {
            assert!(!record_controller_status(&old, status), "{status} A while B active");
            assert_eq!(active_grant_token(window_id, owner).as_deref(), Some("fresh-grant"));
        }
        assert!(record_controller_status(&fresh, "active"));
        assert_eq!(prepare_outbound_input(&mut input), Ok(true));
        assert_eq!(input.input_seq, Some(2), "duplicate B must not rewind sequencing");
        assert!(record_controller_status(&fresh, "stopped"), "current host revoke still terminates B");
        assert!(!record_controller_status(&fresh, "active"));
        assert_eq!(prepare_outbound_input(&mut input), Ok(false));
        retire_all_controllers();
    }

    #[test]
    fn cancellation_rejects_approval_and_preserves_the_share_for_a_new_request() {
        let _guard = test_lock();
        let owner = "cancelled-host";
        let window_id = 0x4100_0022;
        target(owner, window_id, Some("instance-a"));
        let active = controller_status_packet(window_id, owner, "active", "grant", "instance-a");
        assert!(!record_controller_status(&active, "active"));
        request(owner, window_id);
        cancel_control_request(owner, window_id);
        assert!(!record_controller_status(&active, "active"));
        assert!(receive_target(window_id, Some(owner)).is_some());
        let mut input = input_message(window_id, owner, RemoteControlType::Wheel);
        assert_eq!(prepare_outbound_input(&mut input), Ok(false));
        request(owner, window_id);
        let active = controller_status_packet(window_id, owner, "active", "grant", "instance-a");
        assert!(record_controller_status(&active, "active"));
        assert_eq!(prepare_outbound_input(&mut input), Ok(true));
        cancel_control_request(owner, window_id);
        assert!(!controller_is_active(owner, window_id));
        assert_eq!(active_grant_token(window_id, owner), None);
        assert_eq!(prepare_outbound_input(&mut input), Ok(false));
        retire_all_controllers();
    }

    #[test]
    fn legacy_input_requires_an_explicit_pending_request_and_active_status() {
        let _guard = test_lock();
        let owner = "legacy-host";
        let window_id = 0x4100_0023;
        target(owner, window_id, None);
        let mut input = input_message(window_id, owner, RemoteControlType::Wheel);
        assert_eq!(prepare_outbound_input(&mut input), Ok(false));
        request(owner, window_id);
        assert_eq!(prepare_outbound_input(&mut input), Ok(false));
        let mut active = input_message(window_id, owner, RemoteControlType::Status);
        active.controller_id = owner.to_owned();
        active.request_id = remote_control_engine().controller_request_id(window_id, owner);
        assert!(record_controller_status(&active, "active"));
        assert!(controller_is_active(owner, window_id));
        assert_eq!(prepare_outbound_input(&mut input), Ok(true));
        cancel_control_request(owner, window_id);
        assert!(!record_controller_status(&active, "active"));
        assert_eq!(prepare_outbound_input(&mut input), Ok(false));
        retire_all_controllers();
    }

    #[test]
    fn active_status_installs_grant_only_for_the_current_share_instance() {
        let _guard = test_lock();
        let owner = "win-host";
        let window_id = 0x4100_0001;
        target(owner, window_id, Some("instance-a"));
        request(owner, window_id);
        let packet =
            controller_status_packet(window_id, owner, "active", "grant-token-1", "instance-a");
        assert!(record_controller_status(&packet, "active"));
        assert_eq!(
            active_grant_token(window_id, owner),
            Some("grant-token-1".to_string())
        );

        // Replacement: a new instance is live; a LATE active for the retired
        // instance must not restore control (R03).
        target(owner, window_id, Some("instance-b"));
        let stale =
            controller_status_packet(window_id, owner, "active", "grant-token-1", "instance-a");
        assert!(!record_controller_status(&stale, "active"));

        // Fresh instance's grant installs.
        request(owner, window_id);
        let fresh =
            controller_status_packet(window_id, owner, "active", "grant-token-2", "instance-b");
        assert!(record_controller_status(&fresh, "active"));
        assert_eq!(
            active_grant_token(window_id, owner),
            Some("grant-token-2".to_string())
        );

        retire_all_controllers();
        assert_eq!(active_grant_token(window_id, owner), None);
    }

    #[test]
    fn stopped_status_and_share_retirement_release_the_grant() {
        let _guard = test_lock();
        let owner = "win-host";
        let window_id = 0x4100_0002;
        target(owner, window_id, Some("instance-a"));
        request(owner, window_id);
        let packet =
            controller_status_packet(window_id, owner, "active", "grant-token-9", "instance-a");
        assert!(record_controller_status(&packet, "active"));

        // Revoke arrives as a stopped status.
        let stopped =
            controller_status_packet(window_id, owner, "stopped", "grant-token-9", "instance-a");
        assert!(record_controller_status(&stopped, "stopped"));
        assert_eq!(active_grant_token(window_id, owner), None);

        // Share retirement (stop/depart/replace/leave) also drops it.
        request(owner, window_id);
        let packet =
            controller_status_packet(window_id, owner, "active", "grant-token-10", "instance-a");
        assert!(record_controller_status(&packet, "active"));
        assert!(retire_controller_for(owner, window_id));
        assert_eq!(active_grant_token(window_id, owner), None);
        assert!(receive_target(window_id, Some(owner)).is_none());

        // Owner-wide retirement (peer departure) clears every grant.
        target(owner, 0x4100_0003, Some("instance-c"));
        request(owner, 0x4100_0003);
        let second =
            controller_status_packet(0x4100_0003, owner, "active", "grant-token-10", "instance-c");
        assert!(record_controller_status(&second, "active"));
        assert_eq!(retire_controllers_for_owner(owner), 1);
        assert_eq!(active_grant_token(0x4100_0003, owner), None);
    }

    #[test]
    fn scoped_desktop_pointer_generated_by_controller_keeps_instance_without_discrete_markers() {
        let _guard = test_lock();
        let owner = "linux-scoped-host";
        let id = 0x6500_0043;
        upsert_receive_target(ReceiveTargetInfo { owner_identity: owner.into(), window_id: id,
            target_kind: RemoteControlTargetKind::Display, share_instance_id: Some("desktop-current".into()) });
        request(owner, id);
        let mut active = controller_status_packet(id, owner, "active", "desktop-token", "desktop-current");
        active.target_kind = Some(RemoteControlTargetKind::Display);
        active.host_capabilities = vec![RemoteControlCapability::DiscretePointerV1, RemoteControlCapability::ScopedPointerStreamV1];
        assert!(record_controller_status(&active, "active"));
        for action in [RemoteControlAction::Move, RemoteControlAction::Down, RemoteControlAction::Up] {
            let mut input = input_message(id, owner, RemoteControlType::Pointer);
            input.action = Some(action);
            assert_eq!(prepare_outbound_input(&mut input), Ok(true));
            let key = crate::remote_control_core::ControlGrantKey {
                window_id: id,
                controller_id: "local-controller".into(),
                target_kind: Some(RemoteControlTargetKind::Display),
                share_instance_id: Some("desktop-current".into()),
            };
            remote_control_engine().install_grant(key.clone(), "desktop-token".into());
            assert!(crate::remote_control::is_authorized_input(&input));
            assert!(!crate::remote_control_core::is_v2_discrete_attempt(&input));
            input.share_instance_id = Some("retired-desktop".into());
            assert!(!crate::remote_control::is_authorized_input(&input));
            remote_control_engine().revoke_grant(&key);
        }
        retire_controller_for(owner, id);
    }

    #[test]
    fn outbound_input_is_stamped_from_the_active_grant() {
        let _guard = test_lock();
        let owner = "win-host";
        let window_id = 0x4100_0004;
        target(owner, window_id, Some("instance-a"));
        request(owner, window_id);
        let packet =
            controller_status_packet(window_id, owner, "active", "grant-token-4", "instance-a");
        assert!(record_controller_status(&packet, "active"));

        // Discrete wheel: v2 envelope with fingerprint.
        let mut wheel = input_message(window_id, owner, RemoteControlType::Wheel);
        assert_eq!(prepare_outbound_input(&mut wheel), Ok(true));
        assert_eq!(wheel.grant_token.as_deref(), Some("grant-token-4"));
        assert_eq!(wheel.share_instance_id.as_deref(), Some("instance-a"));
        assert_eq!(wheel.control_session_id.as_deref(), Some("control-session"));
        assert!(wheel.input_id.as_deref().is_some_and(|id| id.len() == 32));
        assert_eq!(wheel.input_seq, Some(1));
        assert_eq!(wheel.operation_fingerprint_version, Some(1));
        assert!(wheel
            .operation_fingerprint
            .as_deref()
            .is_some_and(|value| value.len() == 64));

        // Sequence advances for the next discrete op.
        let mut key = input_message(window_id, owner, RemoteControlType::Key);
        key.key = Some("a".into());
        key.code = Some("KeyA".into());
        key.action = Some(RemoteControlAction::Down);
        assert_eq!(prepare_outbound_input(&mut key), Ok(true));
        assert_eq!(key.input_seq, Some(2));

        // Lossy drag stream keeps the legacy shape: grant token, no envelope.
        let mut move_down = input_message(window_id, owner, RemoteControlType::Pointer);
        move_down.action = Some(RemoteControlAction::Down);
        assert_eq!(prepare_outbound_input(&mut move_down), Ok(true));
        assert_eq!(move_down.grant_token.as_deref(), Some("grant-token-4"));
        assert_eq!(move_down.control_session_id, None);
        assert_eq!(move_down.target_kind, None);
        assert_eq!(move_down.share_instance_id, None);

        // Retirement refuses input rather than silently falling back to v1.
        retire_controller_for(owner, window_id);
        let mut late = input_message(window_id, owner, RemoteControlType::Wheel);
        assert_eq!(prepare_outbound_input(&mut late), Ok(false));
        assert_eq!(late.grant_token, None);
        assert_eq!(late.control_session_id, None);
    }

    #[test]
    fn unsupported_operation_kinds_are_refused_from_the_grant() {
        let _guard = test_lock();
        let owner = "win-host";
        let window_id = 0x4100_0005;
        target(owner, window_id, Some("instance-a"));
        // Display grants advertise no UIA invoke; a window-local pointer Up
        // on a display share must not upgrade to an atomic click.
        upsert_receive_target(ReceiveTargetInfo {
            owner_identity: owner.to_string(),
            window_id,
            target_kind: RemoteControlTargetKind::Display,
            share_instance_id: Some("instance-a".into()),
        });
        request(owner, window_id);
        let mut packet =
            controller_status_packet(window_id, owner, "active", "grant-token-5", "instance-a");
        packet.target_kind = Some(RemoteControlTargetKind::Display);
        assert!(record_controller_status(&packet, "active"));

        let mut unsupported = input_message(window_id, owner, RemoteControlType::Text);
        unsupported.text = Some("hello".into());
        // Host without UnicodeText refuses text.
        assert_eq!(prepare_outbound_input(&mut unsupported), Ok(false));
        retire_all_controllers();
    }

    #[test]
    fn control_request_uses_only_a_live_receive_target() {
        let _guard = test_lock();
        let owner = "win-host";
        let window_id = 0x4100_0006;
        let mut request = RemoteControlMessage { request_id: None, controller_capabilities: Vec::new(),
        ..input_message(window_id, owner, RemoteControlType::Request) };
        // No live target → explicit error, no envelope.
        assert!(prepare_control_request(&mut request, window_id, owner)
            .unwrap_err()
            .contains("is not open"));

        target(owner, window_id, Some("instance-z"));
        assert!(prepare_control_request(&mut request, window_id, owner).is_ok());
        assert_eq!(request.target_kind, Some(RemoteControlTargetKind::Window));
        assert_eq!(request.share_instance_id.as_deref(), Some("instance-z"));
        assert!(request
            .controller_capabilities
            .contains(&RemoteControlCapability::UiaInvoke));

        // A display share advertises the display capability set.
        upsert_receive_target(ReceiveTargetInfo {
            owner_identity: owner.to_string(),
            window_id,
            target_kind: RemoteControlTargetKind::Display,
            share_instance_id: Some("instance-z".into()),
        });
        let mut display_request = input_message(window_id, owner, RemoteControlType::Request);
        assert!(prepare_control_request(&mut display_request, window_id, owner).is_ok());
        assert!(!display_request
            .controller_capabilities
            .contains(&RemoteControlCapability::UiaInvoke));

        // Legacy sharer without an instance id: legacy shape.
        target(owner, window_id, None);
        let mut legacy = input_message(window_id, owner, RemoteControlType::Request);
        assert!(prepare_control_request(&mut legacy, window_id, owner).is_ok());
        assert_eq!(legacy.target_kind, None);
        assert_eq!(legacy.share_instance_id, None);

        // Ambiguous window id without an owner hint refuses.
        let other_owner = "win-host-2";
        target(other_owner, window_id, Some("instance-other"));
        assert!(receive_target(window_id, None).is_none());
        assert!(receive_target(window_id, Some(other_owner)).is_some());
        retire_all_controllers();
    }

    #[test]
    fn gtk_keyvals_map_to_dom_key_names() {
        assert_eq!(
            dom_key_fields("a", Some('a')),
            Some(KeyFields {
                key: "a".into(),
                code: "KeyA".into(),
                location: 0
            })
        );
        assert_eq!(
            dom_key_fields("5", Some('5')),
            Some(KeyFields {
                key: "5".into(),
                code: "Digit5".into(),
                location: 0
            })
        );
        assert_eq!(
            dom_key_fields("Shift_R", None).map(|f| (f.key, f.code, f.location)),
            Some(("Shift".into(), "ShiftRight".into(), 2))
        );
        assert_eq!(
            dom_key_fields("Control_L", None).map(|f| (f.key, f.code, f.location)),
            Some(("Control".into(), "ControlLeft".into(), 1))
        );
        assert_eq!(
            dom_key_fields("Return", None).map(|f| (f.key, f.code)),
            Some(("Enter".into(), "Enter".into()))
        );
        assert_eq!(
            dom_key_fields("Left", None).map(|f| f.key),
            Some("ArrowLeft".into())
        );
        assert_eq!(
            dom_key_fields("KP_7", None).map(|f| (f.key, f.code, f.location)),
            Some(("7".into(), "Numpad7".into(), 3))
        );
        assert_eq!(dom_key_fields("VoidSymbol", None), None);
    }

    #[test]
    fn printable_punctuation_keeps_dom_codes_and_produced_text() {
        for (plain, shifted, code) in [
            ('-', '_', "Minus"),
            ('=', '+', "Equal"),
            ('[', '{', "BracketLeft"),
            (']', '}', "BracketRight"),
            ('\\', '|', "Backslash"),
            (';', ':', "Semicolon"),
            ('\'', '"', "Quote"),
            (',', '<', "Comma"),
            ('.', '>', "Period"),
            ('/', '?', "Slash"),
            ('`', '~', "Backquote"),
            ('1', '!', "Digit1"),
        ] {
            for produced in [plain, shifted] {
                let fields = dom_key_fields("", Some(produced)).unwrap();
                assert_eq!(fields.key, produced.to_string());
                assert_eq!(fields.code, code);
                assert_eq!(fields.location, 0);
            }
        }
    }

    #[test]
    fn letterboxed_points_normalize_like_the_web_overlay() {
        // Wide source in a tall area: pillarbox bars left/right.
        let point = normalized_control_point(100.0, 100.0, 200, 100, (50.0, 50.0));
        assert_eq!(point, Some((0.5, 0.5)));
        // Inside the left bar → clamped to 0.
        assert_eq!(
            normalized_control_point(100.0, 100.0, 200, 100, (0.0, 50.0)),
            Some((0.0, 0.5))
        );
        // Tall source in a wide area: letterbox bars top/bottom.
        assert_eq!(
            normalized_control_point(200.0, 100.0, 100, 100, (100.0, 50.0)),
            Some((0.5, 0.5))
        );
        assert_eq!(
            normalized_control_point(0.0, 100.0, 100, 100, (0.0, 0.0)),
            None
        );
        assert_eq!(
            normalized_control_point(100.0, 100.0, 0, 100, (0.0, 0.0)),
            None
        );
    }
}
