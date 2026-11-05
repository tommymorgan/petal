//! Admission for unfinished Linux paths runs before Tauri argument decoding.
//! Keeping unsupported commands out of the handler alone yields an ambiguous
//! "command not found", rather than the product's explicit Linux limitation.

const NATIVE_CONSENT_REQUIRED: &str =
    "Select a monitor or application window through Add share; raw window IDs cannot authorize Linux capture";
const REGION_LIMITATION: &str = "Region sharing is unsupported on Linux";
const PICKER_LIMITATION: &str =
    "This window picker command is unsupported on Linux; use the native Add share action";
const AUDIO_LIMITATION: &str =
    "Computer, system and application audio sharing are unsupported on Linux";
pub(crate) const CONTROL_LIMITATION: &str =
    "Linux application-window shares are view-only; explicitly prepare a monitor and approve a named desktop controller";


pub(crate) fn command_error(command: &str) -> Option<&'static str> {
    match command {
        "share_window" | "toggle_window_share" => Some(NATIVE_CONSENT_REQUIRED),
        "toggle_region_share" | "open_region_window" => Some(REGION_LIMITATION),
        "open_system_content_picker" => Some(PICKER_LIMITATION),
        "set_share_audio_enabled" | "set_region_share_audio" => Some(AUDIO_LIMITATION),
        "remote_control_request_escalation"
        | "remote_control_answer_escalation"
        | "set_remote_control_allowed"
        | "set_remote_control_policy"
        | "set_share_remote_control_allowed"
        | "set_share_control_mode"
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
    fn unsupported_features_remain_refused_without_blocking_named_desktop_consent() {
        for command in [
            "toggle_region_share",
            "open_region_window",
            "open_system_content_picker",
            "set_share_audio_enabled",
            "set_region_share_audio",
            "remote_control_request_escalation",
            "remote_control_answer_escalation",
            "set_remote_control_allowed",
            "set_remote_control_policy",
            "set_share_remote_control_allowed",
            "set_share_control_mode",
            "ai_chat_control_approve",
            "ai_chat_control_resume",
        ] {
            let error = command_error(command).expect(command);
            assert!(error.contains("unsupported on Linux") || error.contains("view-only"), "{command}: {error}");
        }
        for command in ["share_window", "toggle_window_share"] {
            assert_eq!(command_error(command), Some(NATIVE_CONSENT_REQUIRED));
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
            "open_window_picker_window",
            "linux_picture_shares",
            "stop_linux_picture_share",
            "stop_all_shares",
            "room_presence",
        ] {
            assert_eq!(command_error(command), None, "{command} remains admitted");
        }
    }

    #[test]
    fn controller_and_explicit_desktop_consent_commands_are_admitted() {
        // Task 3.1: the Linux controller path (requesting and using a Windows
        // grant, plus controller-side clipboard) is a supported capability.
        for command in [
            "remote_control_send",
            "remote_control_set_active",
            "remote_control_request_timed_out",
            "remote_clipboard_copy",
            "remote_clipboard_paste",
            "start_camera_publish_command",
        ] {
            assert!(
                command_error(command).is_none(),
                "{command} must be admitted"
            );
        }
        // Desktop consent is handled by the common grant core; arbitrary
        // per-window/global enable toggles cannot manufacture portal authority.
        assert!(command_error("remote_control_answer_consent").is_none());
        assert!(command_error("set_share_control_mode").is_some());
        assert!(command_error("set_share_audio_enabled").is_some());
        assert!(command_error("share_window").is_some());
    }
}
