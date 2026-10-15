//! Native Linux launch uses the existing Tauri/GTK3 webview and room UI.
//! Windows hover/capture panels and macOS launch preparation never run here.

use tauri::Manager;

pub(crate) fn run() {
    // Select WebKit's shared-memory buffer transport before GTK/runtime threads
    // start. A complete DMA-BUF opt-out leaves WebKitGTK 2.52.6 without the
    // backing store needed by compositor callbacks; FORCE_SHM retains it
    // without hardware-buffer imports. Native GTK/Cairo video is independent.
    std::env::set_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1");
    let log_path = crate::logging::init();
    log::info!(
        "petal: Linux native startup (log file: {})",
        log_path.display()
    );
    crate::platform::fd::log_startup_descriptor_limits();
    crate::load_env_file(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env"));
    crate::load_env_file(concat!(env!("CARGO_MANIFEST_DIR"), "/.env"));

    tauri::Builder::default()
        .manage(crate::session::SessionState::default())
        .manage(crate::transport::audio::AudioDevicePreferences::default())
        .manage(crate::transport::camera::CameraDevicePreferences::default())
        .manage(crate::diagnostics::DiagnosticsState::default())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            crate::reveal_main_window(app, "single-instance");
            crate::show_and_activate_main_window(app, "single-instance");
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            app.get_webview_window("main")
                .ok_or("native Linux main WebKit view was not created")?;
            // GTK owns an ordinary mapped toplevel before WebKit loads. Do
            // not apply the hidden macOS/Windows first-paint reveal lifecycle:
            // keep its native surface stable across the initial load.
            crate::MAIN_WINDOW_REVEALED.store(true, std::sync::atomic::Ordering::Release);
            let app_data_dir = app.path().app_data_dir()?;
            crate::share_priority::initialize(app_data_dir.clone());
            crate::debug_settings::initialize(&app_data_dir);
            crate::plugins::store::initialize(&app_data_dir);
            crate::analytics::init(&app_data_dir);
            crate::ai_chat::settings::initialize(&app_data_dir);
            app.manage(crate::rooms::RoomsState::load(app_data_dir));
            Ok(())
        })
        .invoke_handler({
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
                crate::frontend_ready,
                crate::get_build_info,
                crate::restart_app,
                crate::launch_location::launch_location_class,
                crate::launch_location::open_applications_folder,
                crate::launch_location::reveal_running_bundle,
                crate::permissions::check_screen_recording,
                crate::permissions::request_screen_recording,
                crate::permissions::check_microphone,
                crate::permissions::request_microphone,
                crate::permissions::check_camera,
                crate::permissions::request_camera,
                crate::permissions::check_accessibility,
                crate::permissions::request_accessibility,
                crate::permissions::open_privacy_settings,
                crate::rooms::list_rooms,
                crate::rooms::create_room,
                crate::rooms::rename_room,
                crate::rooms::forget_room,
                crate::rooms::reset_local_rooms,
                crate::rooms::list_room_occupancy,
                crate::session::join_room_command,
                crate::session::leave_room_command,
                crate::session::current_room,
                crate::session::room_presence,
                crate::session::set_display_name,
                crate::menubar::get_menubar_state,
                crate::transport::audio::list_audio_devices,
                crate::transport::audio::set_audio_devices,
                crate::camera_session::list_camera_devices,
                crate::camera_session::list_camera_modes,
                crate::camera_session::set_camera_device,
                crate::camera_session::set_camera_prefs,
                crate::camera_session::camera_publish_state,
                crate::main_window::open_main_route,
                crate::main_window::show_main_window,
                crate::settings_window::open_settings_window,
                crate::debug_settings::debug_mode_settings,
                crate::debug_settings::set_debug_mode,
                crate::ai_chat::commands::ai_chat_settings,
                crate::logging::set_sentry_enabled,
                crate::logging::export_logs,
                crate::logging::log_updater_event,
                crate::updater::check_compatible_update_available,
                crate::updater::run_launch_update_check,
                crate::updater::download_and_install_compatible_update,
                crate::quit::quit_app,
            ];
            move |invoke| match crate::linux_capabilities::admit(invoke) {
                Ok(invoke) => handler(invoke),
                Err(()) => true,
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running native Linux Petal");
}
