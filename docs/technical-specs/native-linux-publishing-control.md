# Native Linux publishing and remote assistance

Status: proposed implementation design for accepted planning scope, 2026-10-28. No new media or input path has been implemented or exercised by this planning work. Source: [requirements](../requirements/native-linux-publishing-control.md). Architecture authority: [ADR 0003](../adrs/0003-linux-native-publishing-and-desktop-control.md). Execution: [plan](../../plans/2026-10-28-native-linux-publishing-control.md).

## Change

Add native outgoing monitor/window pictures, camera publication and both directions of remote assistance to the existing Linux client. Retain Tauri/WebKitGTK, the native room connection and the released Windows peer on the owned backend/LiveKit service. Microphone state remains independent; screen sharing carries no computer audio.

Linux application-window shares are view-only for input. Tommy explicitly accepted a separate desktop-wide Linux control grant after source investigation did not establish supported GNOME window confinement. Linux controlling Windows retains the Windows implementation's target enforcement. This exception is accepted product scope; the mechanisms in ADR 0003 are engineering recommendations, not human technical sign-off.

The old receive-only plan and its speech/AEC obligations remain unchanged. This expansion supplies neither acoustic acceptance nor a cross-distribution support promise.

## Responsibilities

| Owner | Change | Retained contract |
|---|---|---|
| Linux session/share owner | Explicit source picker, independent share slots, generation-bound start/stop and native frame capture | Existing room identity, share IDs/instances, metadata and publication admission |
| Native frame adapter | Restricted portal PipeWire remote to bounded GStreamer sample handoff | `RoomConnection.publish_window_at` and `PublishedTrack.push_frame`; no second media participant |
| Camera backend/session | Portal permission, actual devices/modes, one native capture owner for self-view and publication | Existing CameraBackend/NV12 and camera_session lifecycle |
| GTK received-share window | Controller request state, content-coordinate pointer/scroll/keyboard events and visible stop | Existing request/grant/revocation and Windows replay enforcement |
| Linux input backend | Explicit combined monitor/control session and EIS device/region lifetime | Common controller/token/share-instance/expiry admission and bounded clipboard protocol |

Full source/lifetime and input preparation decisions, including the four-share engineering limit, are in ADR 0003. An application share ID is not a PipeWire node ID. Camera publication is not a share slot. A desktop-control grant authorizes a controller; a selected monitor only identifies its displayed picture and pointer mapping, not a keyboard security boundary.

## Representative interactions

### Share pictures and camera

Each Add share action opens a native source selection for one monitor or window. Only its returned stream on its restricted PipeWire remote may enter that share’s publisher. Independent sessions permit independently stopping shares. Cancel leaves existing shares unchanged. Four screen/window publications are admitted initially; the next Add explains the limit instead of evicting one. Camera has its own owner and does not consume a slot.

The camera permission flow precedes device capture. The application-scoped coordinator in ADR 0003 owns one low-level capture and separate Settings-preview and room-publication leases. Native GTK transient preview/self-view surfaces consume its frames; Settings never acquires a second WebKit stream. Publication off/room leave releases the corresponding lease, while an explicitly visible Settings preview can continue with an honest indicator. Closing the last consumer releases capture. Permission loss invalidates all leases; source/device loss never silently chooses another camera.

The Camera portal supplies a restricted PipeWire remote. Enumerate its camera nodes and actual format parameters through the PipeWire registry/node APIs, bind selection to the remote generation and node identity, and invalidate it on removal. The portal itself has no device/mode listing method. ADR 0003 owns these source and selection rules.

Use native frame buffers and reusable storage. Explicitly bound appsink, any intermediate queue and the application handoff to one pending latest frame, dropping superseded buffers. Keep GTK work on the main thread and capture/conversion off it. Do not transport frames through base64/webview events. Missing pipeline elements produce a prerequisite error, not an automatic install.

### Linux controls Windows

Add request/stop controls and pointer/scroll/keyboard handling to the existing GTK received-share window, which currently only renders and handles its normal window lifecycle. Reuse the established control command semantics, not an assumed existing GTK control widget. Its input handler stays inactive until a matching current grant exists. Hit-test the displayed picture rather than its decoration or letterbox and forward only while the native window has the required local focus. Windows remains responsible for enforcing its shared-window target; Linux must not replace that enforcement with coordinate assumptions.

Decline, timeout, revoke, disconnect, source/instance retirement and local focus/input cancellation return the window to a non-controlling state and release held input through the existing protocol. Existing clipboard consent and payload limits apply in both roles; no images, files, hidden clipboard polling or retained contents.

### Windows controls Linux

1. The Linux sharer selects an existing monitor share’s separate Enable desktop control action. The disclosure says desktop-wide input can affect the focused desktop, not just the displayed monitor, and that stopping control also stops this monitor share.
2. A fresh nonpersistent RemoteDesktop session selects keyboard/pointer and one monitor through the ScreenCast interface. The user selects the monitor again. Preparation never grants a peer authority.
3. After a valid first frame and an unambiguous stream-to-EIS region binding, the share owner commits the selected combined-session picture, rotates its instance and retires old pending/granted requests. Cancellation or failure before commit preserves the original picture-only share. The ready state visibly says no controller is authorized yet.
4. Windows sends a fresh ordinary screen-control request for the committed instance. Linux separately approves that named controller. Only then can the common grant gate admit input to the EIS backend. A request for a window share, an old instance or an already occupied desktop-control session is rejected without fallback.
5. Revoke first disables application admission, releases held input where possible and closes EIS/the combined portal session. That controlled monitor share ends; unrelated shares, camera and microphone retain their states. Reconnection requires explicit preparation and a fresh grant.

There is at most one prepared Linux desktop-input session and one active controller. A competing request reports in-use; it cannot steal authority. Readying a second monitor requires explicitly ending the first control session.

The stream’s optional v5 `mapping_id` must match a live EIS absolute-device region. Map normalized picture coordinates through that region’s logical geometry, including fractional scale and letterboxing. Missing metadata, ambiguous binding, missing input devices, region removal or an out-of-date generation prevents input. Do not infer a global monitor identity by comparing opaque IDs across unrelated portal sessions. After ConnectToEIS, do not use Notify methods as a fallback.

## Decisions and tradeoffs

ADR 0003 selects explicit portal ownership plus a native PipeWire/GStreamer adapter because the current SDK capturer wrapper does not expose sufficient capture-session/input-region association. It reuses the existing publisher and camera coordinator while adding native dependency checks. Extending the SDK wrapper remains a credible alternative only if it supplies equivalent ownership/metadata; reusing frames alone is insufficient.

A combined capture/control session supplies a documented same-session input mapping. The cost is another explicit monitor picker and source-instance transition. Preparing the session before the peer’s final request preserves the released Windows request/grant wire contract. Closing the whole session on revocation drops OS input permission promptly, at the visible cost of ending that controlled picture share. Silently retaining/reacquiring a broader OS session is excluded.

A direct V4L2 camera path would create separate device permission and conversion ownership. The Camera portal route shares the native frame adapter. GStreamer handles format conversion/decoding instead of expanding a custom raw PipeWire implementation; package/plugin availability and native binding versions must be checked against the existing GTK/GLib toolchain during build.

## Risks and required proof

| Risk | Response and exact evidence needed |
|---|---|
| Advertised APIs mistaken for working media/input | Read-only observations are ScreenCast v5, RemoteDesktop v2, Camera v1/present and installed pipewiresrc metadata. Build must separately prove first frames, native Windows rendering and authorized input. |
| Optional mapping/device support | Fail closed and explain unavailable control for that attempt. Real GNOME acceptance must prove the combined-session mapping, scaled corners, resize and region invalidation; do not replace this with private Mutter or unscoped input. |
| Pixel/source mismatch after control preparation | Publish the combined-session source, commit one new instance and require a fresh request. Exercise cancellation, changed monitor selection and late old packets with the released peer. |
| Leaked capture or stale asynchronous completion | One generation owner per source/room, idempotent retirement, explicitly bounded appsink/worker queues; exercise cancel/leave/rapid replacement and observe track, device and portal-session release. |
| Camera permission or preview duplication | Real portal outcome, registry-derived device/mode metadata and separate preview/publication leases on one owner; exercise preview overlap, off, device loss and last-consumer closure. Camera-present metadata alone proves no permission. |
| Camera permission revocation has no dedicated portal signal | Follow ADR 0003's concrete PipeWire core/node/pipeline loss transitions and report access/device loss without inventing a permission cause. Real revocation during capture must terminate it observably on this backend; otherwise camera acceptance remains blocked. |
| Protocol regression or accidental broad authority | Preserve the existing wire and common grant admission; exercise Windows target confinement, Linux window denial, explicit desktop disclosure, revocation and rejected tokens/instances/coordinates. |
| EIS loss prevents held-input release | Invalidate the grant first and close the session even if release cannot be delivered. Exercise device/EIS loss while input is held; observe compositor teardown clears held state, late packets are rejected and unrelated media stays live. |
| Unusable intermediate delivery | Keep each capability disabled until its complete native path and failure cleanup exist; prove every exact task head without successors as specified in the plan. |

No new backend deployment, DNS/firewall change, credential distribution or browser participant is needed. Dependency absence is an actionable build prerequisite, not authorization to install software during planning. Preserve running applications and owned services during future acceptance.

## Readiness and review

The accepted product decisions are resolved. Implementation still needs the exact-unit runtime proof above; optional metadata support and camera formats are explicit fail-closed integration checks, not claims already established by this spec. If real target evidence contradicts the documented path, report that prerequisite/design failure rather than claiming the feature accepted or bypassing its boundary.

Fresh-reader findings and the final planning audit are recorded in the plan’s planning-validation section after they run. Review proves neither runtime behavior nor Tommy’s hands-on acceptance. Planning completion does not authorize build, publication or device/input activation.
