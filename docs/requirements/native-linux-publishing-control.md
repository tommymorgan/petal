# Native Linux sharing, camera and remote assistance

Status: accepted planning scope; not implemented by this document. Date: 2026-10-28.

## Problem and goal

Native Linux participants can join, use microphone controls and receive shared content, but cannot yet show their own work, publish their camera or participate fully in remote assistance. The outcome is two-way collaboration with the released Windows native peer: several independently controlled picture shares, camera video and explicitly authorized remote input.

## Source and decision authority

No earlier expansion PRD was supplied or found. This is the interview-derived source for the new plan, grounded in Tommy’s accepted choices in Kizn Item `01a183b6-c2be-76d4-bf35-e390674bb675`, revision 7. User decisions: all three capability groups; picture-only sharing; multiple simultaneous shares; camera separate from the share count; both control directions; retain Tauri/WebKitGTK; Linux window shares view-only for input, with a separate explicit desktop-wide Linux control grant. That last choice explicitly narrows the earlier window-scoped Linux-host requirement.

The detailed acceptance criteria below are engineering-derived checks of that agreed scope and inherited contracts, not additional quoted user instructions. The implementation limit of four screen/window shares and the combined-session control lifecycle are engineering choices in [ADR 0003](../adrs/0003-linux-native-publishing-and-desktop-control.md), not numbers or mechanisms selected by Tommy.

## Accepted exclusions

Computer/system/application audio sharing, window-confined Linux input injection, Gecko migration, browser/X11 fallback, general GNOME extensions, cross-distro parity, installer release and public infrastructure changes are excluded. Camera video is not a remotely controllable share. Microphone audio remains independent. These exclusions do not remove existing receive-only acceptance obligations.

Planning permission grants no screen/camera activation, input injection, implementation, provisioning, push or deployment authority. Use only the authorization in effect when building or exercising each future scenario. Preserve currently running applications and the existing Windows VM and owned services.

## Acceptance criteria

### Screen/window publication

- **S01**: Linux can explicitly select a monitor, publish its changing picture natively, and stop it; the released Windows peer sees that monitor in an independent share window.
- **S02**: Linux can explicitly select an application window and publish that window’s changing picture without publishing unrelated desktop content.
- **S03**: Linux can publish at least two screen/window shares simultaneously, add another up to the documented supported limit, and report a reached limit without replacing an existing share. Camera publication does not consume a screen/window share slot.
- **S04**: Stopping one Linux share removes only that publication and its remote window; other shares, camera and microphone keep their existing states.
- **S05**: Starting, changing and stopping Linux screen/window shares never publishes computer, system or application audio and never changes the independently selected microphone mute state.
- **S06**: Only content explicitly selected through the native permission flow is published; cancelling or denying it publishes nothing and leaves existing shares unchanged.
- **S07**: Closing a shared window, removing its monitor or revoking capture permission retires the affected publication, remote window and associated control grant without substituting a different source.
- **S08**: After stop, leave, source replacement or reconnect, late capture/start/stop results cannot resurrect or remove a newer share; recovery never silently chooses new content or restores control authority.
- **S09**: While native capture authority remains valid, published picture, cursor and source geometry remain correct through resize and display-scale/resolution changes; a Windows receiver sees the selected source with correct aspect, orientation and readable content, without stale frames from a retired source. If the OS ends capture authority during a geometry change, only the affected share stops, Linux explains the interruption and offers an explicit action to share again, and resuming requires fresh native source selection. Other shares and microphone state remain unchanged; geometry changes alone do not stop a share whose authority remains valid.
- **S10**: Unavailable portal/capture components and capture/publication failures produce a useful visible error, release failed-attempt resources and permit an explicit retry without changing existing shares or falling back to browser/X11 capture.

### Camera publication

- **C01**: Explicitly turning on the Linux camera publishes changing camera video to the released Windows peer and displays a matching local self-view; camera is off before that action.
- **C02**: Visible settings preview and outgoing camera publication share one selected-device capture owner. Turning publication off stops outgoing video; capture stops when neither publication nor visible preview needs it, and any remaining preview is visibly identified.
- **C03**: Camera denial, cancellation or no available camera produces an accurate visible state with no camera publication; camera presence or a non-Linux permission default is never reported as granted permission.
- **C04**: The Linux user can enumerate available camera devices and supported modes and select them; rapid changes honor the latest selection, and frames from the old device or mode cannot replace the new self-view or publication.
- **C05**: Device removal, capture failure and permission loss stop the affected camera publication, explain the failure and allow explicit recovery without silently selecting another camera or disturbing shares or microphone.
- **C06**: Linux can publish camera video alongside at least two simultaneous screen/window shares and independently mute or unmute the microphone; camera and each share remain separately controllable.
- **C07**: Turning camera publication off and leaving the room retire the corresponding outgoing track and room-owned self-view; late starts or reconnect results cannot re-enable camera, and no device remains captured without a current visible-preview or publication owner.

### Linux controlling Windows

- **R01**: A Linux receiver can request and, after the Windows sharer grants it, use pointer, buttons, scroll and keyboard in the native received-share window. A Windows window grant retains Windows target confinement; a Windows screen grant retains its existing explicit screen/desktop semantics.
- **R02**: Declined, cancelled and expired control requests and late responses never enable input; Linux shows the resulting state and requires a fresh request after expiry.
- **R03**: Windows revocation, source stop, peer departure, room leave, disconnect and share-instance replacement end Linux controller input and release held input; stale packets or responses cannot restore the grant.
- **R04**: Linux supports the existing separately authorized copy/paste exchange only for the active controller and exact current grant: UTF-8 plain text up to 1 MiB, no NUL, no files or images. Denied, oversized and stale-grant transfers change neither clipboard.

### Windows controlling Linux

- **H01**: Individual Linux application-window shares are view-only for remote input. Their UI and direct commands cannot acquire or replay desktop input authority, and a failed window-control attempt never falls back to desktop control.
- **H02**: A Windows peer can control Linux only after the Linux sharer separately approves a clearly disclosed desktop-wide grant for that controller and the native desktop permission flow succeeds. Denial, cancellation, timeout or missing requested device permission enables no input.
- **H03**: For Linux desktop control, pointer input maps to the monitor picture actually published by the same authorized capture/control session, including letterboxing, resize and fractional scale. Reselecting content is explicit, rotates the share instance and requires fresh authorization; missing or ambiguous mapping fails closed.
- **H04**: Linux desktop-control consent and active-state UI state that keyboard and other desktop input are desktop-wide, including focus changes; they never promise confinement to the selected window or monitor. A normal picture-only share never implicitly grants input.
- **H05**: Local stop, permission revocation, source loss, controller departure, room leave, disconnect, expiry and share-instance change revoke Linux desktop authority, release held input and close its input session. Invalid tokens, wrong controllers, retired instances, non-finite or out-of-range coordinates and late packets inject nothing.
- **H06**: Reconnect and retry never automatically restore Linux desktop control. Ending its combined capture/control session ends that controlled monitor share visibly while unrelated picture shares, camera and microphone remain unchanged; sharing again requires explicit source consent.

### Interoperability and evidence

- **X01**: All new publication and control use the existing native room identity, transport and compatible wire contracts with the released Windows peer on the same owned backend and LiveKit service; no extra publishing participant, browser-media fallback, new server secret on a client or Windows upgrade is required.
- **X02**: Room entry/leave, incoming share windows, their layout and lifecycle, microphone controls and existing supported Windows/macOS behavior keep working at every implementation boundary.
- **X03**: Linux UI availability and direct command results agree with actual completed capability support. Camera and input remain unavailable until their complete implementations exist; per-operation permission or service failure is shown rather than reported as success.
- **X04**: The documented Linux run path identifies required portal, PipeWire, native camera/capture and input components and reports an unavailable component without activating media or installing it automatically. This milestone targets this GNOME Wayland computer, not promised cross-distro parity.
- **X05**: Acceptance uses the real Linux executable and released Windows peer on the owned service, explicitly selected non-sensitive sources and authorized camera/input actions. Programmatic media checks do not claim human camera-picture usability, conversation or acoustic echo-cancellation acceptance, and evidence does not retain sensitive media or clipboard contents.
- **X06**: The receive-only plan, its earned progress and unfinished speech/AEC acceptance remain historical and unchanged; the new complete plan does not mark these new features implemented or waive old acceptance.

## Acceptance environment and status

Use the same Tommy-owned stateless backend and real LiveKit service configured explicitly in both native clients with trusted TLS. Keep server API secrets server-side. A synthetic source or in-memory transport can establish a bounded mechanism only; it does not replace real native-peer acceptance. Ask for only the human observation that cannot be established by machine evidence, and continue independent authorized work when a listener is absent.

Every criterion starts unimplemented and unaudited. The [plan](../../plans/2026-10-28-native-linux-publishing-control.md) owns execution. Its local gitignored coverage ledger owns planned mapping and later fidelity. [Technical direction](../technical-specs/native-linux-publishing-control.md) summarizes the design; [ADR 0003](../adrs/0003-linux-native-publishing-and-desktop-control.md) owns the consequential architecture choices.
