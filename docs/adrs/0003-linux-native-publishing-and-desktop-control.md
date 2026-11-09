# Own Linux portal sessions; grant desktop input separately

Status: proposed implementation architecture for the accepted 2026-10-28 scope. Tommy accepted the product boundary in [the requirements](../requirements/native-linux-publishing-control.md); the mechanisms below are engineering recommendations under planning authority, not completed implementation or permission to activate devices.

Use explicit native XDG portal sessions and a Linux PipeWire/GStreamer frame adapter feeding the existing native LiveKit publisher. Keep the Tauri/WebKitGTK shell, the existing room identity, share protocol, camera-session coordinator and remote-control grant core. This gives capture and input one explicit source/lifetime owner rather than hiding the source selection inside a second media participant or an SDK capturer that does not expose the required portal metadata.

## Capture ownership

Use one nonpersistent ScreenCast session per picture-only share, selecting one monitor or window per native picker invocation. Support four simultaneous screen/window shares, matching the existing macOS admission limit as an initial engineering limit; camera is independent. Do not confuse this implementation choice with a user-selected numeric limit. Each share owns its session, restricted PipeWire remote, selected stream and cancellation generation. Its application share ID is independent of its PipeWire node ID. A node ID is valid only in the owning live session/remote; do not persist or reuse it as a window identity. On this ScreenCast v5 target do not require v6 `pipewire-serial`.

A bounded native GStreamer pipeline consumes that exact restricted remote and stream, negotiates publisher-compatible BGRA for shares or NV12 for camera, and hands frames to the existing publisher. Decode/convert camera formats when needed. Explicitly configure appsink for one queued buffer with drop-oldest/latest-frame behavior; apply the same bound to any intermediate queue and the application handoff. Its unlimited default is forbidden. Use reusable storage and respect strides, orientation and color metadata. Keep GTK objects on the main thread and capture/convert work off it. Pipeline error/EOS, portal closure and room generation cancellation converge on the same idempotent stop owner. Never reconnect a failed stream to an arbitrary PipeWire node.

**Accepted product amendment, 2026-10-29:** Tommy approved respecting the native capture grant rather than requiring uninterrupted monitor publication after GNOME ends it. Preserve correct picture/cursor geometry while authority remains valid; a display change alone is not a stop trigger. If the OS ends authority, retire only that share, explain the interruption and offer an explicit action to share again. Recovery opens fresh native source selection before publishing a new share instance; never silently reacquire authority or substitute content. Unrelated shares and microphone intent remain unchanged, and no input authority is restored. The guarded 100% → 150% scale run ended the monitor grant on this computer; this decision changes acceptance, not that observed result, the VM-abort diagnosis or the remaining runtime gates.

**Accepted platform limits, 2026-10-29:** Tommy accepted three verification
limits as documented platform behavior on this GNOME 51 installation, after
each was externally evidenced: (1) the portal chooser offers no distinct Deny
control — Cancel and dialog close are the only exits, so consent testing
covers cancel only; (2) DP-1 physical-disconnection retirement cannot be
exercised because Mutter rejects a zero-monitor configuration on this
single-monitor host and a physical unplug requires the user to replug;
(3) per-share OS capture-permission revocation has no external API — Mutter
`Session.Stop` requires the exact creating D-Bus sender, which is Petal's own
backend, so external scoped revocation cannot be simulated honestly. These
are recorded limits, not satisfied criteria: the deny, disconnect and
revocation rows stay unverified, and any future GNOME release that adds the
control or API reopens them.

**Accepted camera fallback, 2026-10-29 (Option A):** Tommy chose the
direct-PipeWire fallback after this computer's portal was evidenced dead for
cameras: xdg-desktop-portal 1.22.1 auto-grants `AccessCamera` and returns a
restricted remote, but neither installed frontend (gnome 51, gtk) implements
`org.freedesktop.impl.portal.Camera`, so that remote lists zero camera nodes
forever. The app probes the portal once per process; when the granted remote
enumerates nothing it enumerates and captures through the session PipeWire
daemon directly — the platform-normal path for a host application, where the
in-app Camera button is the consent surface, exactly like the microphone. A
desktop whose frontends implement the Camera interface keeps the portal
path; the fallback is per-probe, not a removal of the portal flow. Evidence
deviations recorded with C01 in the plan: no OS dialog exists to exercise
the approval step here, and pixel-change proof substitutes SFU layer
liveness plus rendered-tile variance because the camera views a static
scene.

Camera uses Camera.AccessCamera/OpenPipeWireRemote. The portal supplies permission and a restricted remote, not a device/mode enumeration API. Discover camera Video/Source nodes through that remote's PipeWire registry and enumerate each selected node's SPA_PARAM_EnumFormat parameters. Keep a selection bound to the remote generation and node object identity/serial; invalidate it on registry removal or permission loss before a reused node ID can identify another device. Derive labels and modes from returned properties/parameters, not hard-coded V4L2 paths. Missing usable metadata produces an explicit unavailable state.

Camera has no portal Session.Closed or documented revocation-specific signal. Treat PipeWire core error/disconnect, selected-node registry removal and capture pipeline ERROR/EOS as immediate access/device-loss transitions: invalidate all consumer leases, unpublish, close previews and release capture before considering a retry. Use a neutral “camera access or device lost” explanation unless an explicit portal denial identifies permission as the cause. Actual revocation while capturing is an acceptance gate: verify this GNOME backend exposes a terminating observable condition. If it leaves capture alive with no observable revocation, this path is not accepted; report the missing prerequisite instead of asserting revocation detection.

Buffer-pool removal is not camera loss: PipeWire can remove and immediately
replace all buffers while negotiating a supported camera mode. Keep
`pipewiresrc`'s `on-disconnect=none`; `error` incorrectly makes that normal
rebuild fatal. Selected-node removal and core loss remain registry-driven,
and actual pipeline ERROR/EOS still retires the capture. Do not infer device
removal from buffer-pool turnover alone.

Use an application-scoped Linux camera coordinator with separately cancellable Settings-preview and room-publication leases. It owns the selected device/mode generation and the single low-level PipeWire/GStreamer capture. The existing `open_camera(..., on_frame)` factory returns `Box<dyn CameraBackend>`; its Linux implementation should return a publication-lease handle and deliver that lease's NV12 frames to the existing callback, preserving camera_session's publication lifecycle without opening another device. `CameraBackend::stop` releases only that lease. Give each publication lease its own `CameraStatusSource` identity so `CameraStatus::same_capture` cannot mistake a retired lease for a new one on the same physical capture. The owner's low-level open path must not recursively call `open_camera`. Device/mode selection is serialized at this owner; stale completions cannot replace its current generation. Releasing the room publication lease on off/leave does not release a still-visible Settings preview; the last lease closes the pipeline/remote. Permission revocation invalidates every lease and requires an explicit new user action.

Feed the native GTK Settings preview from the same latest-frame owner, using the existing Linux compositor's DrawingArea/color-conversion pattern on the main thread. Settings opens a clearly titled native transient preview and closes its lease when that preview or Settings closes; closing it does not secretly start another capture. It uses standard GTK decoration, not a new custom layout or WebKit video acquisition. Show publication state in room controls and any remaining Settings preview explicitly. No WebKit getUserMedia acquisition for Linux preview, second publisher identity, direct V4L2 fallback or unconditional non-macOS permission success.

**Amended 2026-11-06 (Tommy's decision):** the in-room self-view renders inside the local meeting tile, not in a floating GTK window. The publication lease's frames go through the shared `camera_self_view` latest-frame slot and `next_self_view_frame` pull (the Windows path), so there is still one capture and no WebKit acquisition. This WebKitGTK has no `RTCPeerConnection`, and a `canvas.captureStream()` MediaStream never plays (readyState stays 0; the GStreamer GL sink also crashed the web process on NVIDIA), so Linux draws pulled frames onto the tile's own `<canvas>` on a timer rather than through `<video>`. The self-view follows publication: it starts after publish succeeds and stops with camera off or room leave.

## Linux desktop control

Linux window shares remain view-only for remote input. Starting a picture share never requests input devices. A controlled monitor is a separate explicit mode: the sharer chooses “Enable desktop control” and approves its desktop-wide scope before a fresh RemoteDesktop session selects keyboard/pointer plus one monitor through ScreenCast.SelectSources and starts through RemoteDesktop.Start. Use nonpersistent permission. Explain that the native picker requires selecting a monitor again. Preparation grants no participant control; it exposes a visible “Desktop control ready; no controller” state, and a subsequent named-controller request still requires the normal explicit application approval.

Because desktop input is shared focus rather than independent per-window destinations, admit at most one prepared Linux desktop-control session and one active controller in a room at a time. A second preparation or controller request reports “in use”; it cannot replace the current session or steal authority. The sharer must explicitly stop the current control session first. This is an engineering concurrency policy, not a new user-selected share-count limit.

The controlled monitor picture must come from that same combined session. Replace the selected existing monitor publication atomically only after its first valid frame, rotate its share instance and invalidate old grants; on cancellation keep the original view-only share unchanged. A changed monitor selection is an explicit source replacement, never an inferred cross-session identity match. Do not convert a window share to desktop control. Requests sent before this committed transition are denied/retired; the controller must request again against the new instance. Never transfer an old pending approval to the replacement. Retain the existing screen-control request/grant exchange after preparation, with the exact-attempt correlation extension approved below; control verification uses an updated temporary Windows candidate.

Use RemoteDesktop.ConnectToEIS and libei for input, matching the combined stream's v5 `mapping_id` to its absolute-device region. Normalize received picture coordinates after letterboxing and map through that region's logical geometry, not physical video dimensions or guessed monitor origins. Require an unambiguous live mapping and the requested keyboard/pointer devices before accepting input. Optional mapping metadata that is missing means control unavailable for that attempt, not broader fallback. Once connected to EIS, never mix in Notify methods, private Mutter APIs, XTest or uinput. Keyboard and desktop focus remain desktop-wide even though pointer coordinates track the displayed monitor; consent and active UI must say so.

The existing application grant core still checks controller identity, current room/share instance, token, expiry, event validity and clipboard scope before reaching this platform backend. Revoke first at the application admission boundary, release held input where the live input device permits, then close the EIS and portal session. A lost device/session must not retain an application grant. The initial design closes the whole combined session when control ends, so that controlled monitor share also ends visibly. Other shares, camera and microphone continue. Keeping that share alive would require a new view-only capture consent/session; do not reacquire it silently. This lifecycle favors promptly dropping OS input authority over retaining picture capture after revocation.

If EIS or its device disappears before held-input releases can be sent, local revocation still completes: invalidate the application grant first, attempt releases only on a live device, and close the session without waiting for an impossible acknowledgment. Confirm compositor device teardown leaves no held input and later packets are rejected; unrelated media must remain live. Do not keep authority alive while retrying release.

Linux controlling Windows reuses Windows target enforcement and adds controller actions to the actual GTK received-share window, not only the unused Svelte remote-window surface. Tommy's November 4, 2026 bounded local-only prerequisite approval permits repairing background Windows key duplication and extending endpoint request/status correlation so an older approval cannot authorize a new same-share request. Preserve consent, target confinement and fail-closed retirement; no server-side grant store or installed released-Windows replacement is authorized. Keep clipboard exchange explicitly authorized, plain text only, bounded to the inherited 1 MiB/no-NUL contract.

## Alternatives and costs

- The vendored WebRTC DesktopCapturer already has Linux/PipeWire support, but its current Rust wrapper exposes frames and source lists rather than the portal session/stream-to-EIS binding needed here. Reusing it alone would leave source/lifetime association unproven; extending that wrapper is a credible alternative if it can provide equivalent ownership and metadata without a second picker or hidden input grant.
- Direct V4L2 camera capture creates a separate permission/device path and remains excluded. The portal/PipeWire route retains one frame adapter; the accepted empty-portal-remote fallback uses the session PipeWire daemon, not V4L2.
- Direct PipeWire buffer handling could avoid GStreamer, but would also own camera format decoding/conversion and buffer negotiation. GStreamer adds native runtime/build dependencies; missing elements must be reported, not auto-installed or hidden by a browser path.
- Target-confined Linux window input was the original preference. Supported GNOME portal/EIS input did not establish that confinement, so Tommy explicitly chose view-only windows plus separately disclosed desktop-wide control. This does not claim no future compositor integration could provide it.

## Source and verification limits

Read-only metadata on this computer reports ScreenCast v5, RemoteDesktop v2 and Camera v1 with a camera present. `gst-inspect-1.0 pipewiresrc` reports PipeWire plugin 1.6.10 and the `fd`, `path`, `target-object` and `on-disconnect` properties. These establish advertised interfaces, not permission, a working media pipeline or successful input. Runtime acceptance remains in the [plan](../../plans/2026-10-28-native-linux-publishing-control.md).

Read-only pkg-config checks report libei 1.6.0, GStreamer core/app/video 1.28.8 and PipeWire 1.6.10. Camera metadata discovery uses the documented [PipeWire node parameter enumeration API](https://docs.pipewire.org/group__pw__node.html). These checks did not open a capture stream or input session.

Sources: [ScreenCast stream/lifecycle contract](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html), [combined RemoteDesktop and EIS contract](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html), [Camera contract](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Camera.html), [PipeWire GStreamer source](https://github.com/PipeWire/pipewire/blob/master/src/gst/gstpipewiresrc.c), [appsink API](https://gstreamer.freedesktop.org/documentation/app/appsink.html). The current web ScreenCast documentation describes v6; only fields introduced by v5 or earlier are relied on here. `mapping_id` is optional and introduced in v5; `id` is session-local except explicitly restored sessions, not a global window identifier.

Code seams under `apps/desktop/src-tauri/src/`: `transport/publisher.rs` (`RoomConnection.publish_window_at`, `PublishedTrack.push_frame`), `transport/camera/mod.rs`, `camera_session.rs`, `session_linux.rs`, `remote_control_core.rs`, `remote_control.rs`, `linux_capabilities.rs` and `linux_compositor.rs` for the GTK received-share window. Existing [ADR 0002](0002-linux-native-media-seams.md) remains the receive-only milestone's historical decision; its prohibition on camera publication is not the scope of this separately authorized expansion.

Number 0003 follows the local 0001/0002 files; the checked remote default revision `7782718c0cc498615099f799533b67701665caee` had no root docs/adrs entries. Recheck availability before publication. No capture, camera or input action was activated for this decision.
