# Reuse the native room and audio stack; render Linux shares with GTK3

For this milestone use `meeting_core::connect_room`, the existing native subscription coordinator, and LiveKit `PlatformAudio` rather than introducing a second browser participant or copying the Windows capture session. Add a Linux-specific session adapter owning one connected room, microphone, managed playout, incoming-share feed, and generation-bound cleanup. Keep the real Windows session selected only for Windows; repair Linux module, command, camera, and Unix dependency gates without redesigning macOS/Windows media.

Use a native GTK3 top-level window with normal window-manager move/resize controls and a `DrawingArea`/Cairo image surface for received video. Tauri already uses GTK3 on Linux. Consume `NativeVideoStream` frames, obtain I420 planes and the actual color profile, and use the shared color-conversion seam to produce native-endian ARGB32 (BGRA bytes on this x86_64 target). Preserve aspect ratio by fitting the image and letterboxing. A GPU-specific renderer is unnecessary to prove this bounded outcome; it would add driver/lifecycle uncertainty before a Linux path works. This is not a claim of performance parity with the native macOS/Windows renderers.

All GTK objects and drawing stay on Tauri/GTK's main thread; decode and conversion stay off it. Use a latest-frame handoff bounded to one pending frame and at most one pending main-thread drain. Reuse output/backing storage at stable dimensions rather than allocating or copying whole frames unnecessarily. The existing `video_color::convert_i420_to_bgra` allocates its result; implementation must inspect references before extending that seam for reusable output storage. Do not move GTK objects across threads, enqueue unbounded frames, or turn video into base64/webview messages.

Keep existing native publication admission and share-identity/track-SID rules. Acceptance exercises one incoming share; that does not authorize adding a new one-share admission cap or silently replacing arbitrary shares. Teardown owns cancellation: a retired track, participant, or room generation must not resurrect a window or audio. Reuse the existing reconnect flow; do not add a new retry subsystem.

Audio uses the vendored WebRTC ADM's Linux PulseAudio/ALSA support through the existing `PlatformAudio` interface and software APM; echo cancellation, noise suppression, and automatic gain control are not replaced by raw `cpal` capture. The source establishes support and configuration, not a working acoustic result on this machine. Real hosted calls using speakers/microphone with both native peer platforms remain mandatory acceptance evidence.

## Evidence and consequences

- `apps/desktop/src-tauri/src/meeting_core.rs:63-155`: shared connection seam.
- `apps/desktop/src-tauri/src/transport/native_subscription.rs`: native publication admission owner; retain its policy.
- `apps/desktop/src-tauri/src/transport/subscriber.rs:2399-2405,3066-3078,3312-3383`: platform feed, decoded frame extraction, and missing Linux feed.
- `apps/desktop/src-tauri/src/video_color.rs:318-360`: existing I420 conversion.
- `apps/desktop/vendor/livekit/src/platform_audio/mod.rs` and `processing.rs`: Linux ADM support and APM defaults.
- Tauri 2.12.2 `Window::gtk_window` and `WebviewWindow::default_vbox`: GTK handles are main-thread-only. Use Linux-target direct dependencies matching the already resolved GTK/Cairo versions, not a GTK4 stack beside GTK3.

Normal GTK window decoration is the deliberate Linux milestone choice; recreating macOS borderless overlays is not required for independence, movement, or resize. The established Svelte room controls are reused, not redesigned. Remote camera viewing is not an acceptance promise, and local camera publication must stay unavailable.

Number 0002 was checked against fetched `origin/main` at `7782718c0cc498615099f799533b67701665caee`; no root `docs/adrs/` entries existed. Recheck numbering at PR time.
