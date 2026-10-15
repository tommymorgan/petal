//! Receive-only admission runs before Tauri argument decoding and dispatch.
//! Keeping unsupported commands out of the handler alone yields an ambiguous
//! "command not found", rather than the product's explicit Linux limitation.

pub(crate) const SCREEN_LIMITATION: &str =
    "Screen publication is unsupported on Linux (receive-only milestone)";
pub(crate) const CAMERA_LIMITATION: &str =
    "Camera publication is unsupported on Linux (receive-only milestone)";
pub(crate) const CONTROL_LIMITATION: &str = "Remote control requests, grants, input sending and replay are unsupported on Linux (receive-only milestone)";

pub(crate) fn require_camera_publication() -> Result<(), String> {
    Err(CAMERA_LIMITATION.to_string())
}

pub(crate) fn require_remote_input() -> Result<(), String> {
    Err(CONTROL_LIMITATION.to_string())
}

pub(crate) fn command_error(command: &str) -> Option<&'static str> {
    match command {
        "share_window"
        | "toggle_window_share"
        | "toggle_region_share"
        | "open_window_picker_window"
        | "toggle_window_picker_window"
        | "open_region_window"
        | "open_system_content_picker"
        | "set_share_audio_enabled"
        | "set_region_share_audio" => Some(SCREEN_LIMITATION),
        "start_camera_publish_command" => Some(CAMERA_LIMITATION),
        "remote_control_send"
        | "remote_control_set_active"
        | "remote_control_request_escalation"
        | "remote_control_answer_consent"
        | "remote_control_answer_escalation"
        | "set_remote_control_allowed"
        | "set_remote_control_policy"
        | "set_share_remote_control_allowed"
        | "set_share_control_mode"
        | "remote_clipboard_copy"
        | "remote_clipboard_paste"
        | "ai_chat_control_approve"
        | "ai_chat_control_resume" => Some(CONTROL_LIMITATION),
        _ => None,
    }
}

pub(crate) fn admit<R: tauri::Runtime>(
    invoke: tauri::ipc::Invoke<R>,
) -> Result<tauri::ipc::Invoke<R>, ()> {
    if let Some(error) = command_error(invoke.message.command()) {
        invoke.resolver.reject(error);
        Err(())
    } else {
        Ok(invoke)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_unsupported_commands_are_rejected_without_revoking_receive_or_audio() {
        for command in [
            "share_window",
            "toggle_window_share",
            "toggle_region_share",
            "open_window_picker_window",
            "toggle_window_picker_window",
            "open_region_window",
            "open_system_content_picker",
            "set_share_audio_enabled",
            "set_region_share_audio",
            "start_camera_publish_command",
            "remote_control_send",
            "remote_control_set_active",
            "remote_control_request_escalation",
            "remote_control_answer_consent",
            "remote_control_answer_escalation",
            "set_remote_control_allowed",
            "set_remote_control_policy",
            "set_share_remote_control_allowed",
            "set_share_control_mode",
            "remote_clipboard_copy",
            "remote_clipboard_paste",
            "ai_chat_control_approve",
            "ai_chat_control_resume",
        ] {
            let error = command_error(command).expect(command);
            assert!(error.contains("unsupported on Linux"), "{command}: {error}");
        }
        for command in [
            "join_room_command",
            "leave_room_command",
            "set_mic_muted",
            "toggle_menubar_mic",
            "set_audio_devices",
            "compositor_list_windows",
            "compositor_resize_window",
            "current_room",
            "room_presence",
        ] {
            assert_eq!(command_error(command), None, "{command} remains admitted");
        }
    }
}
