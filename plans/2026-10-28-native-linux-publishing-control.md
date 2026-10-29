# Feature: Native Linux picture sharing, camera and remote assistance

**Created**: 2026-10-28
**Goal**: Collaborate with the released Windows native peer by showing several independent Linux picture shares, publishing camera video and using explicitly authorized remote assistance in both directions.
> Source PRD: [Interview-derived expansion requirements](../docs/requirements/native-linux-publishing-control.md)
> PRD coverage: plans/native-linux-publishing-control.prd-coverage.md
> Context: [Glossary](../CONTEXT.md), [integration map](../CONTEXT-MAP.md), [ADR 0003](../docs/adrs/0003-linux-native-publishing-and-desktop-control.md), [Technical Spec](../docs/technical-specs/native-linux-publishing-control.md)

## Execution boundary

This is one complete plan for all agreed requirements, not a hidden later-phase placeholder. Local build is authorized; scenario metadata and living features record only exercised behavior. No PR, push or deployment is authorized. The earlier receive-only plan and its DONE progress and TODO speech/AEC acceptance remain unchanged. Device/input activation requires its existing action-specific authorization; this document grants none.

Reuse the current room/settings/received-window controls and native permission dialogs; no new visual layout exploration or Figma artifact is required. There is no invented Design reference. The Linux received-share surface is GTK, so controller interaction must work there, not merely in Svelte remote-window code.

Derive review units from the complete behavioral scenarios below. Estimates are low-confidence ranges of reviewable lines from the current Linux stub and analogous Windows/macOS session, camera and GTK receiver seams; exclude generated/lock/vendor bulk. They are not time promises or a global size cap. Cohesive portal/camera/input units are intentionally larger than a mechanical layer split: each must expose a usable, fail-closed path without successors. Split actual diffs only at a newly proven safe boundary, preserving scenario identity.

At each unit, retire obsolete Linux blanket-denial checks only for the capability actually completed, updating its callers and living behavior without compatibility shims. The historical receive-only plan is not edited. Task1.1 replaces the old combined living denial scenario with consumer-facing capability behavior; subsequent units extend that current living contract as capability support is enabled. Never keep a living assertion that a now-supported operation is unsupported, or publish future-task scenarios early.

All unit-head checks use the real native executable and current owned backend/LiveKit with the released Windows peer. Exercise pre-existing room/incoming-share/microphone behavior at every boundary. Stop on a missing required runtime prerequisite for that path, report it accurately and continue independent authorized work; do not install, activate unrelated hardware or modify running services by inference. A missing human listener leaves human audio acceptance open, not all engineering work blocked.

## Slice 1: Show Linux work

**Tracer bullet**: Publish a selected monitor to the Windows peer.

**Touches**: Linux session, portal capture, native publisher and existing share controls.

**Off-limits**: camera and input implementations; owned services and old acceptance.

### Task 1.1: Publish a monitor with safe failure and cancellation
<!-- estimate: ~1400–2300 reviewable lines (low confidence) | feature-files: apps/desktop/features/linux-publishing-interop.feature, apps/desktop/features/linux-share-publication.feature | demoable: yes -->
> Depends on: none.
> Shippable state: A complete picture-only monitor path works; camera/input remain unavailable. Failed or retired starts cannot leak capture or revive shares.
> Boundary verification: Launch the exact task-head executable, publish DP-1 to released Windows, cancel another attempt and leave during a delayed start; observe correct picture and no residual publication/device session. Repeat existing join, incoming-share and microphone smoke.
> Allocation correction: Full S06 stays unchanged and belongs to Task 1.2, where Add share exists. Task 1.1 still verifies cancellation of a real replacement attempt at its own boundary; it does not claim the later Add share scenario.

<!-- DONE -->
# Requirement: S01
# Living: apps/desktop/features/linux-share-publication.feature::Publish a selected monitor to Windows
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Publish a selected monitor to Windows
  Given Linux and the released Windows peer are in room abc-defg-hjk on the owned service
  When I select monitor DP-1 through the native Share action and change its visible counter from 10 to 11
  Then Windows shows that selected monitor and the changing counter in an independent share window
  When I stop that share
  Then its publication and Windows remote window close
```

<!-- DONE -->
# Requirement: S05
# Living: apps/desktop/features/linux-share-publication.feature::Share actions preserve independent audio state
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Share actions preserve independent audio state
  Given Linux has its microphone <mute_state> and no computer-audio publication
  When I <share_action> a picture share
  Then the microphone remains <mute_state>
  And no computer, system or application audio track is published
  Examples:
    | mute_state | share_action |
    | muted | start |
    | unmuted | start |
    | muted | change |
    | unmuted | change |
    | muted | stop |
    | unmuted | stop |
```

<!-- DONE -->
# Requirement: S08
# Living: apps/desktop/features/linux-share-publication.feature::Retired share work cannot change the current generation
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Retired share work cannot change the current generation
  Given a Linux monitor start from generation A is delayed
  When I <retire_action> and explicitly start the current share generation B where applicable
  And the old A start, frame and stop completions arrive late
  Then A cannot create a publication or remove or overwrite B
  And recovery selects no new content and restores no remote-control grant
  Examples:
    | retire_action |
    | stop the pending share |
    | leave the room |
    | replace the selected source |
    | disconnect and reconnect |
```

<!-- DONE -->
# Requirement: S10
# Living: apps/desktop/features/linux-share-publication.feature::Report a failed picture attempt and permit explicit retry
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Report a failed picture attempt and permit explicit retry
  Given Linux already publishes DP-1 to Windows
  When a new share attempt encounters <failure>
  Then Linux shows the failure and releases that attempt's capture, portal and publication resources
  And DP-1 continues unchanged without a browser or X11 fallback
  When I resolve the failure and explicitly retry
  Then the selected new picture can be published
  Examples:
    | failure |
    | ScreenCast portal unavailable |
    | required capture plugin missing |
    | capture failure before first frame |
    | native publication rejected |
```

<!-- DONE -->
# Requirement: X03
# Living: apps/desktop/features/linux-publishing-interop.feature::UI and direct commands expose only completed capability paths
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: UI and direct commands expose only completed capability paths
  Given this implementation unit's Linux executable is running
  When I inspect each publication and input action and invoke its direct command
  Then completed capabilities use their real native path with explicit per-operation permission state
  And incomplete camera or input capabilities remain unavailable without effects
  And missing permission or service is reported as a visible failure rather than success
```

<!-- DONE -->
# Requirement: X04
# Living: apps/desktop/features/linux-publishing-interop.feature::Report Linux native prerequisites without activating devices
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Report Linux native prerequisites without activating devices
  Given the documented Linux launch environment for this GNOME Wayland computer
  When I run the documented prerequisite check with a required native component unavailable
  Then it identifies the missing portal, PipeWire, capture/camera or input component and its effect
  And the check neither activates screen/camera/input nor installs software
  And documentation does not promise full cross-distro support
```

### Task 1.2: Publish multiple monitors and application windows
<!-- estimate: ~600–1100 reviewable lines (low confidence) | feature-files: apps/desktop/features/linux-share-publication.feature | demoable: yes -->
> Depends on: Task 1.1.
> Shippable state: Up to four independent monitor/window pictures work with individual teardown and correct geometry while capture authority remains valid; ended authority permits only explicit native re-selection, with unrelated shares unchanged. Camera/input remain unavailable.
> Boundary verification: Publish monitor and editor simultaneously to released Windows; add to four, reject five without eviction, stop one, resize, close a source and revoke its portal permission. For scale/resolution changes, preserve pictures while authority remains valid; if the OS ends authority, observe only the affected share retire, its visible explanation and explicit fresh-picker recovery without disturbing the editor or microphone. Repeat X03 UI/direct-command checks: monitor and window publication work; camera and input remain unavailable.

<!-- DONE -->
# Requirement: S06
# Living: apps/desktop/features/linux-share-publication.feature::Share permission never selects content implicitly
# Action: replaces
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Share permission never selects content implicitly
  Given Linux is already publishing monitor DP-1
  When I open Add share and <permission_outcome> the native permission flow
  Then no new picture is published and DP-1 remains unchanged
  And no other source is substituted
  Examples:
    | permission_outcome |
    | cancel |

Accepted limit (ADR 0003): the deny row is unverified on GNOME 51, which
offers no distinct Deny control; Cancel and dialog close are the only exits.
```

<!-- DONE -->
# Requirement: S07
# Living: apps/desktop/features/linux-share-publication.feature::Retire a lost shared source without substitution
# Action: replaces
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Retire a lost shared source without substitution
  Given Linux publishes the <source> and an unrelated picture share
  And the source has <control_state>
  When <source_loss>
  Then its publication, Windows remote window and associated grant retire
  And the unrelated share continues without source substitution
  Examples:
    | source | control_state | source_loss |
    | editor window | no input authority | the editor window closes |

Accepted limits (ADR 0003): the DP-1-disconnect row is unverified because
Mutter rejects a zero-monitor configuration on this single-monitor host, and
the scoped-revocation row is unverified because GNOME 51 exposes no external
per-share revocation API. Both reopen if the platform grows the control.
```

<!-- DONE -->
# Requirement: S02
# Living: apps/desktop/features/linux-share-publication.feature::Share only the selected application window
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Share only the selected application window
  Given Linux is in room abc-defg-hjk with Windows
  And the editor window shows counter 20 while an unrelated desktop window shows private counter 99
  When I select only the editor window through Add share and change its counter to 21
  Then Windows receives the editor picture and its changing counter
  And neither the unrelated window nor the rest of the desktop is in that publication
```

<!-- DONE -->
# Requirement: S09
# Living: apps/desktop/features/linux-share-publication.feature::Preserve the selected picture through geometry changes
# Action: replaces
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Preserve the selected picture through geometry changes
  Given Linux publishes the <source> containing a corner grid, readable text and a moving cursor to Windows
  And its native capture authority remains valid through the geometry change
  And an unrelated picture share and the independently muted microphone are active
  When I <geometry_change>
  Then Windows shows the current selected picture with correct aspect, orientation, readable text and cursor placement
  And it shows no frame from a retired source
  And the unrelated share and microphone state remain unchanged
  Examples:
    | source | geometry_change |
    | editor window | resize the editor from 1280 by 720 to 960 by 960 |
```

<!-- DONE -->
# Requirement: S09
# Living: apps/desktop/features/linux-share-publication.feature::Recover explicitly when geometry changes end capture authority
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Recover explicitly when geometry changes end capture authority
  Given Linux publishes DP-1 and an unrelated editor window to Windows with the microphone muted
  When I <geometry_change> and GNOME ends DP-1's capture authority
  Then only DP-1's publication, remote window and associated grant retire
  And Linux explains that capture access ended and offers an explicit action to share again
  And no replacement source or publication starts automatically
  And the editor share and microphone mute state remain unchanged
  When I explicitly choose to share again
  Then the native source picker opens and no new picture publishes before source selection
  When I explicitly select DP-1 in that picker
  Then Windows receives its current corner grid, readable counter and correctly placed moving cursor in a new share instance
  And no retired frame or input authority is restored
  And the editor share and microphone mute state remain unchanged
  Examples:
    | geometry_change |
    | change display scale from 100 percent to 150 percent |
    | change display resolution |
```

## Slice 2: Show the Linux camera

**Tracer bullet**: Turn on native camera video with matching local self-view.

**Touches**: CameraBackend, camera_session, native preview and existing camera controls.

**Off-limits**: input implementation; browser camera acquisition; old speech/AEC criteria.

### Task 2.1: Publish camera with one preview owner and complete lifecycle
<!-- estimate: ~1000–1800 reviewable lines (low confidence) | feature-files: apps/desktop/features/linux-camera-publication.feature | demoable: yes -->
> Depends on: Task 1.2.
> Shippable state: Native camera permission, device/mode selection, publication, preview ownership and error/retirement cleanup are complete; sharing and microphone keep working.
> Boundary verification: Use a permitted real camera with released Windows, overlap settings preview and publication, turn publication off then close preview, switch devices/modes and exercise denial/removal/late-start cleanup; observe actual video and last-consumer release. Repeat X03 UI/direct-command checks: camera now works while remote input remains unavailable. Controlled-device fixtures may prove multi-device races, but must be labeled separately from physical-camera video evidence.

<!-- DONE 2026-10-29: live-verified on junkman against the released Windows peer in room kwa-xjth-qav.
Evidence: SFU track petal-camera-<identity> VIDEO unmuted 640x480 with live LOW layer
(ssrc 2503472586, bitrate 833333); publisher health captured=4584 pushed=4576 at 15 fps;
Windows peer tile renders real image content (crop variance stdev 54, not a placeholder);
native self-view window "Petal — Camera self-view (HD Pro Webcam C920" exists (AT-SPI), no browser acquisition.
Evidence deviations (ADR 0003 platform limits): this desktop has no Camera portal backend
(auto-grant, no OS dialog to approve), and the changing-pixel clause is proven by SFU layer
liveness + rendered tile variance because the camera views a static scene (no physical marker). -->
# Requirement: C01
# Living: apps/desktop/features/linux-camera-publication.feature::Turn on native camera video with matching self-view
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Turn on native camera video with matching self-view
  Given Linux camera is off in a room with the released Windows peer
  When I explicitly turn camera on and approve native permission for camera Cam-A
  And I move a non-sensitive marker in front of Cam-A
  Then Windows receives its changing camera picture
  And Linux shows the matching native self-view without a browser acquisition
```

<!-- PARTIAL 2026-10-29: preview/publication overlap verified on junkman. Preview-first:
linux_camera_preview_open on a fresh process seeded the default C920 selection
and opened the native preview (deviceId 111) with no prior camera-on; publish-overlap:
preview opened during active publication, publication health kept climbing, preview
closed cleanly, camera_publish_state held publishing=true. Preview close no longer
reports a false loss (Settings only sets the error when the event carries one).
Verification of the publication-off-while-preview-identified row was cut short when
the C920 physically disconnected from USB mid-run (absent from lsusb; honest
"No camera is available" from then on); that row and C03-C05 reopen with the
camera replugged. Same-owner enforcement: ensure_capture adopts the serving
capture for a same-device/mode selection instead of racing a rebuild. -->
<!-- 2026-11-02 isolated follow-up: replugged C920 enumerates as node 35.
Petal ran on a separate headless Mutter display, runtime and session bus, with
no human-display or clipboard connection. Preview-open resolved with the C920,
but publication failed with "no camera frames within 5s"; publication and intent
returned false. Bounded media-only GStreamer probes also reported "target not
found"; two successful probes did not reproduce consistently, so no source-role
fix is established. Preview-open is not proof of rendered frames. The remaining
C02 publication-off/preview-retained row remains unverified. No shared PipeWire
restart, host input replay or display changes were performed. -->
<!-- Metadata-only November 2 follow-up: actual Petal enumeration and the installed
GStreamer capture-client registry both resolve C920 node 35, serial 43861, on the
same PipeWire daemon; the capture client has RWXM metadata permissions. The
guarded READY-only probe made zero stream-connect/activation calls. Basic node
visibility is confirmed; stream negotiation/linking and rendered frames are not. -->
<!-- Actual-desktop November 2 evening / November 3 UTC follow-up (authorized):
Preview-first publication reproduced a real first-frame timeout. Reusing the
capture changed its selection generation while its pump retained the old one.
Frames and failures now bind to the existing physical capture-status identity
and resolve the current generation only after that match; superseded producers
cannot feed the preview or publication. Two behavioral regressions cover adopted
capture delivery/failure and superseded producer frames/failure; all 11 focused
camera-owner tests pass.
Repeated permission preparation also installed duplicate GTK surface pumps.
Application registration now installs that pump once; the rebuilt actual app
shows exactly one preview and one self-view during publication.
The real C920 preview displays its room image, and released Windows 0.9.33
renders the camera at 640x480 with an unmuted SFU CAMERA track and a live layer.
Both preview-first and publication-first overlap retain one physical capture
link. Closing preview leaves publication running. Stopping publication clears
the actual Windows camera tile and self-view while the selected C920 preview
and its single capture remain; closing the last preview removes both surfaces
and the final capture link. The physical source becomes idle. The hardware LED
was not independently observed, so the complete C02 row is not marked DONE.
Evidence is private under /tmp/petal-evening-live-06h_7h0i; no screenshots were
published. Only the owned Windows Petal process was relaunched, without update,
to select the explicit owned backend and software encoding. -->
<!-- DONE November 5 attended C02 indicator check: actual C920 Settings preview
started before publication. The same running input node/link fed preview,
self-view and released Windows0.9.33 video640x480; no second acquisition.
Tommy observed LED ON. Publication Off removed the outgoing camera and Windows
video while the same one visible native preview/capture remained; Tommy again
observed ON. Closing Settings alone left that separate visible native preview
open, so the actual last preview window was then closed. Zero owned capture
nodes/links and no preview/self-view/outgoing camera remained; Tommy observed
OFF. Initial preview startup failed alongside USB firmware-change/re-enumeration
messages; one normal Settings close/reopen recovered. No manual reset/policy
change, no claim of durable hardware repair. Both microphones stayed muted,
no screen sharing. Clients left/quit, guard stopped, room empty; VM/release
unchanged. Evidence: /tmp/petal-c02-indicator-y4kia9yc/c02-complete.json,
human-led-observations.json, last-preview-closed.json and cleanup.json. -->
# Requirement: C02
# Living: apps/desktop/features/linux-camera-publication.feature::Share one camera owner between preview and publication
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Share one camera owner between preview and publication
  Given the visible Linux settings preview displays Cam-A
  When I turn camera publication on
  Then the same selected capture feeds the preview, self-view and outgoing camera
  And no second device acquisition occurs
  When I turn publication off
  Then Windows camera video stops while the visible settings preview is identified as still using Cam-A
  When I close that last preview
  Then Cam-A capture and its device indicator stop
```

<!-- DONE November 4 local C03 matrix: the actual Linux executable and unchanged
released Windows0.9.33 shared the owned room. With synthetic camera nodes present,
check_camera initially returned not-determined, not a platform authorized default.
Real private Camera portal denial showed denied-or-cancelled/camera off; cancelling
an actual pending portal request through production Off showed the definite
cancelled/camera-stays-off explanation and dismissed that dialog. Removing both
private camera sources showed No camera is available. All three rows ended with
publishing=false/intended=false, zero SFU video tracks, zero Windows video dimensions
and no private capture links. The cancellation row is explicit caller cancellation,
not a claim that the OS exposes separate denial/cancel response codes. Evidence:
/tmp/petal-autonomous-acceptance-lw46v89y/c03-matrix-*.json.
Independent Settings consent remains valid across publication Off/Leave. -->
# Requirement: C03
# Living: apps/desktop/features/linux-camera-publication.feature::Show accurate camera permission or absence state
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Show accurate camera permission or absence state
  Given Linux camera is off
  When I request camera and <permission_result>
  Then Linux shows <visible_state> without a camera publication
  And a presence flag or platform permission default cannot show camera permission as granted
  Examples:
    | permission_result | visible_state |
    | permission is denied | permission denied |
    | permission is cancelled | camera off |
    | no camera is available | no camera available |
```

<!-- PARTIAL 2026-11-02: fixed the missing mode-query core sync,
matched the returned completion sequence and drained queued format replies.
The live check also exposed incorrect SPA choice parsing: choices contain one
child header and packed value bodies, not repeated pod headers. Corrected the
parser and its binary test fixtures; removed the obsolete local sequence counter
and its implementation-only test. A disposable driver used the production
CameraRegistry against C920 node 35: three queries each returned the same 98 raw
mode/rate combinations across 19 sizes in about 1 second, including 640x480@30.
A nonexistent node was refused. Camera remained suspended with unchanged
metadata and no links. No capture pipeline, portal prompt, audio, desktop input,
clipboard or shared-service change was used. The driver was removed afterward.
A later device-free private PipeWire runtime exercised the actual capture module
against two client-owned synthetic producers: A 640x480@30 -> B 320x240@15 -> A.
Each delivered six changing NV12 frames with increasing timestamps; dropping a
pipeline stopped delivery and reopening worked. Terminating only producer A
produced NodeRemoved and the neutral camera-loss error, retired A's capture,
and left B delivering six more changing frames. No physical camera, audio,
human display/input/clipboard, portal prompt or shared-service restart was used.
The final focused Linux/clipboard boundary passes 78 tests. Disposable drivers
were removed and owned private runtimes stopped. These checks do not instantiate
the full camera owner or prove Settings selection races, self-view or outgoing
camera frames; C04 remains TODO. -->
<!-- November 2 evening / November 3 UTC actual-Settings follow-up:
Fresh Settings queried devices before native camera permission completed; its
resolution controls also queried the pre-discovery fallback device. Discovery
now refreshes after the native preview opens, then queries modes for the actual
selected device. The actual rebuilt UI enabled resolution selection and showed
the advertised supported/unsupported preset choices.
The fixed 15/30/60 FPS choices were insufficient for the real C920: its raw
1280x720 modes are 10, 7.5 and 5 FPS. Selecting the old 720p@30 combination
reproduced an explicit native publication refusal. Linux selectors now derive
sizes and rates from actual advertised modes and reject stale device-query
completions. Preferred FPS accepts fractional values; native validation and
capture retain the advertised rational rather than rounding it.
An Auto-preview -> explicit 480p@30 -> publication transition also reproduced
"all buffers have been removed" and retired the capture. Replacement teardown
is now serialized before opening the new stream; this is not yet a confirmed
explanation or a live-verified fix for that failure.
The integrated frontend and native executable build, but final native mode and
Settings coexistence acceptance is blocked by physical device loss. Kernel
logs identify USB 1-6.4.1.2 as C920 and report repeated descriptor/address
timeouts from 02:00:36 UTC, disconnect and failed enumeration at 02:02:47 UTC.
No /dev/video devices or PipeWire Video/Source nodes remain. Similar USB
failures predate this pass; the current code is not established as their cause.
No USB hub/controller reset was attempted because the protected remote
Trailboss connection may depend on the shared dock. C04 remains TODO. -->
<!-- November 3 / November 4 UTC follow-up:
After physical replug, direct V4L2/PipeWire capture and nine bounded
auto/on/auto capture cycles passed; the original auto/2000 ms policy was restored.
Fresh native Settings-preview-first publication then delivered actual 640x480
video to released Windows 0.9.33 through one capture, with captured=4333 and
pushed=4321 at about 24 fps. Selecting advertised 1280x720@10 through Settings
failed with "all buffers have been removed"; publication/intent became false,
Settings explained camera loss, and the Windows video disappeared. Kernel
descriptor -110/address -62 failures, disconnect and failed enumeration followed.
A device-only reset could not execute after disappearance; camera-port-only
power cycling did not recover it. Both USB2/empty-companion disable attributes
were verified restored to zero. No whole-hub/controller/shared-service reset.
Separately, a real private-producer/full-owner witness reproduced stale A
cancelling serving B before rejecting A. Exact generation/node/requested-mode
validation before adoption and atomically with post-discovery capture removal
fixes that race: the same B capture survives, real dispatched frames increase
5->14, and genuine old frames/completions cannot replace it. The permanent
three-variant regression and all 13 integrated owner tests pass; desktop builds.
Independent frozen-snapshot review found no issues in this narrow fix.
This establishes owner admission, not complete Settings/rendered-self-view/SFU
C04 acceptance, and does not establish a fix for the physical USB failure. -->
<!-- November 4 buffer-pool follow-up:
Capture-only Auto640x480/30->720/10 reproduced the failure on direct USB,
with old objects both retained and released; explicit source caps did not fix it.
The trace showed buffers removed then immediately replaced and streaming resumed.
pipewiresrc on-disconnect=error incorrectly made that normal rebuild terminal.
Changing only this setting to none passed 180 real frame-discarding test frames.
The rebuilt actual native app, with Settings preview retained, published720/10,
switched through real Settings to720/7.5 and640/30, and reported matching SFU
geometry and capture/push cadence. Preview/publication release independence and
last-consumer cleanup passed; final node suspended/no links and owned room empty.
All34 Linux-camera tests and desktop build passed; one independent narrow-source
review found no issues. No released Windows peer in this follow-up. Complete
Cam-A/Cam-B latest-selection, indicator and permission-loss acceptance remain
open; C04 staysTODO. This repairs false pool-loss, not all USB descriptor faults. -->
<!-- November 4 controlled-source repair: C04 remains TODO.
Atomic device/mode selection now normalizes an unsupported saved mode instead
of disabling the selected camera. Camera-only malformed H.264 STAP-A metadata
and the released receiver's old-unsubscribe/new-track race were identified.
Removing the camera metadata override and retaining the Linux publication SID
across capture handoffs restored released Windows0.9.33 video without peer or
server changes. Actual Settings A/B and rapid A/B/A/B selection produced native
GTK red/blue self-view pixels and Windows1280x720/10 then640x480/30 video;
Windows SSRC129349529 stayed stable, NACK/PLI0. Selected-source removal stopped
publication; explicit B selection recovered. Leave retained only the visible
preview; closing it left no links. Separate exact-production owner replay
rejected old A frames/completion/start while preserving B and advancing frames5→14.
These remain separate proofs, not the combined UI plus delayed-completion case.
Camera suite100 passed/1 ignored; separate real-NV12 SDK full-reconnect check
passed with retained/live encoding limits and source dimensions preserved.
Locked desktop build passed; independent narrow-source review found no issues.
Owned clients left/quit, private sources/portal/activated children stopped and
scratch profiles/helpers removed; installed Windows remains0.9.33, VM running.
No publication or physical recovery inferred. -->
<!-- November 5 UTC combined C04 acceptance: DONE.
Actual Settings selected A1280x720/10 then B640x480/30, including A/B/A/B
changes, in one native app with released Windows0.9.33. A throwaway instrumented
copy retained a genuine completed A owner record and actual A frame, then
externally replayed them through the original capture_started/frame_arrived
paths after B was visibly active. Natural pipeline-open completion cannot
overtake B because capture opening is serialized; this is explicitly a saved
record replay, not a naturally delayed OS open. The original status gate
rejected A's frame and completion returned Camera selection superseded.
B retained its capture status, publication, preview and exact supported mode.
Actual Settings stayed on B; 100 real GTK self-view DrawingArea snapshots over
14.195s and 65 released-Windows decoded observations over8.092s remained blue
across replay and30 subsequent B dispatches. Maximum observation gaps were
0.202s/0.152s; Windows presented frames advanced2690->2933 at640x480. GTK
snapshots invoke the actual renderer at an idle boundary, not compositor
scanout; no GTK critical/assertion occurred. Sampling is not every-frame proof.
The same work exposed overlapping Settings/capture-validation mode queries
overwriting their one completion slot. Whole-query serialization fixed all32
real concurrent requests (before4/32); rebuilt actual Settings/publication/
self-view passed. Camera suite107 passed/1 ignored, locked build passed and
independent frozen-source review found no issue in this narrow fix.
Evidence: /tmp/petal-autonomous-acceptance-lw46v89y/c04-combined-acceptance.json
and mode-query-concurrency-acceptance.json. No physical recovery inferred. -->
# Requirement: C04
# Living: apps/desktop/features/linux-camera-publication.feature::Latest camera device and mode selection wins
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Latest camera device and mode selection wins
  Given native camera permission is granted and the capture backend exposes Cam-A and Cam-B with their supported modes
  When I open the device selector and each device's mode selector
  Then Settings lists those available devices and each device's actual supported modes for selection
  When I select Cam-A at its advertised 1280 by 720 mode then quickly select Cam-B at its advertised 640 by 480 mode
  And the older Cam-A start and frame completions arrive after Cam-B is active
  Then Cam-B at the selected supported mode remains selected in settings, self-view and outgoing video
  And older device or mode frames cannot replace it
```

<!-- DONE November 5 C05, all three rows with actual C920+DP-1+unmuted
independent Volt276 microphone and unchanged released Windows0.9.33.
Permission loss: revoke only the owned camera clients' real source node ACL;
camera retires with an explanation, while the same display/microphone tracks,
states and Windows share decoding/audio reception continue. Restoring access
does not auto-enable camera; explicit On recovers. Native empty-app-id
WirePlumber policy ignores PermissionStore NO, so this is real node-access
loss, not portal-store revocation.
Capture failure: destroying only the owned camera input-stream node exposed
stuck intent/publication. Unique per-capture stream-name watching and
frame-independent retirement repaired it; the full failure/recovery row passed
with other media preserved. Actual Settings320x240→640x480 replacements remained
healthy; camera tests110 passed/1 ignored, locked build passed and independent
narrow review found no demonstrated defect.
Physical unplug: continuous observer armed before Tommy's actual unplug
captured camera off/explanation/no camera track and unchanged DP-1/microphone
SIDs/states with Windows share-frame progress/audio reception. Replug did not
auto-enable camera. Explicit On recovered640x480 Windows video with advancing
decoded frames and other media preserved. Earlier missing-snapshot/time-out
attempts were not counted as passes. Fresh physical row/cleanup receipts:
/tmp/petal-c05-physical-fresh-tsbif4x5/physical-row-complete.json and cleanup.json;
ACL/capture/mode evidence: /tmp/petal-c05-attended-gpswnl7c. Both owned clients
left/quit, room empty, no owned capture clients; VM/release unchanged. No
publication or shared-service change. C02 indicator and other gates stay open. -->
# Requirement: C05
# Living: apps/desktop/features/linux-camera-publication.feature::Retire a failed camera without changing other media
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Retire a failed camera without changing other media
  Given Linux publishes Cam-A, DP-1 and an unmuted microphone
  When Cam-A encounters <camera_failure>
  Then its camera publication stops and Linux explains the failure
  And no other camera is selected and DP-1 and microphone keep their states
  When I resolve the failure and explicitly turn the selected camera on again
  Then camera publication can recover
  Examples:
    | camera_failure |
    | device unplug |
    | capture failure |
    | permission loss |
```

<!-- DONE 2026-10-29: both rows live-verified on junkman. Turn-off row: stop_camera_publish_command
resolved, camera_publish_state publishing=false intended=false, SFU camera track removed
(only petal-mic remained), preview already closed so no owner remained. Leave row:
leave returned to room entry; late start_camera_publish_command rejected with
"not in room" and camera-intent logged true then immediately cleared (no surprise re-enable).
Witness: apps/desktop/scripts/check-linux-camera-publication.py (C01/C02-overlap/C07 rows). -->
<!-- November 4 lifecycle follow-up: real pending publication consent now closes
on Leave/Off. A queued On behind independent Settings consent snapshots room
generation and retirement before waiting, preventing an older On from reviving
after Off/Leave. Actual Settings→On→Off/Leave→Deny kept the independent dialog
valid and ended with intent/publication false, no second prompt or Windows video.
Owned cancellation cleanup does not retire a newer explicit On; its controlled
interleaving regression and a native Off→fresh On permission request passed.
Final camera suite107 passed/1 ignored and locked desktop build passed.
Independent source review cleared its newer-On cleanup finding. -->
# Requirement: C07
# Living: apps/desktop/features/linux-camera-publication.feature::Retired camera ownership cannot return
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Retired camera ownership cannot return
  Given a Linux room-owned camera start is delayed
  When I <camera_retirement>
  And that old start or reconnect completion later reports success
  Then no retired outgoing camera track or room self-view returns
  And capture has no owner unless a current publication or explicitly visible settings preview still needs it
  Examples:
    | camera_retirement |
    | turn camera publication off |
    | leave the room |
```

## Slice 3: Assist the Windows sharer from Linux

**Tracer bullet**: Control an explicitly authorized Windows window through the GTK received-share window.

**Touches**: GTK receiver interaction, portable control core and clipboard adapter.

**Off-limits**: Linux host input activation; unrelated Windows target-enforcement changes; new server state. Tommy's November 4 approval extends the local-only prerequisite repair to Windows background-key duplication and exact request/approval correlation across a new same-share request. Preserve target confinement, consent and security checks; do not replace the installed Windows release, publish or change shared services.

<!-- estimate: ~700–1200 reviewable lines (low confidence) | feature-files: apps/desktop/features/linux-remote-controller.feature | demoable: yes -->
<!-- actual 2026-10-29: ~1500 reviewable lines incl. tests + witness script; estimate undercounted the GTK input surface and the clipboard backend. -->
> Status 2026-11-04 (local acceptance only): R01–R04 are locally verified.
> The approved Windows keyboard repair closes R01's duplicate-background-key
> defect without changing target enforcement. Exact request identity now binds
> approval, denial, release, consent answers and timeout/cancellation work;
> missing or retired identities cannot activate or retire a newer request.
> Genuine encrypted A responses were held for 1.61 s while GTK cancelled A and
> requested B on the same share. All 21 held datagrams were explicitly released,
> with zero automatic acceptance. A's authenticated active/stopped statuses were
> refused; B stayed waiting with no input until its own real Windows Allow.
> Fresh B typed one background character without focus change, and current
> host revoke stopped it. Receipt: `petal-approved-prerequisite-acceptance.json`.
> The scope approval is personal Kizn Item
> `01a183b6-c2be-76d4-bf35-e390674bb675` revision 17. These local results do not
> authorize publication, installed-release replacement or Linux-host activation.
> Depends on: Task 2.1.
> Locally verified controller state: Linux uses only current Windows grants, handles rejection/revocation and releases held input. R04's separately authorized clipboard evidence is retained. Linux-host replay and host clipboard remain unavailable until Task 4.1.
> Boundary verification uses the unchanged released peer for earlier media rows and owned updated Windows candidates for the approved prerequisite repairs. It covers window/screen input, confinement, unsuccessful requests, held-input retirement and separately authorized clipboard. Exact-request control requires correlated endpoints; older uncorrelated approvals fail closed.
>
> Isolation prerequisites checked November 2: live VBoxManage metadata reports
> the Windows VM running, clipboard disabled, file transfer off and drag-and-drop
> disabled. Its GUI console and audio input/output remain enabled: host-input
> console automation and audio tests are not isolated by these settings.
> Clipboard isolation no longer requires a data-control-capable compositor:
> standard Wayland clipboard access works on owned software-only Mutter after
> creating a virtual keyboard through its private bus. The actual Linux
> clipboard backend now has private-runtime evidence; see R04 below.
> Neither VM settings nor the human desktop, input or clipboard were changed.

<!-- PARTIAL 2026-10-30 live vs released Windows peer, screen row:
Display share live (SFU track petal-window-39), control request -> real consent dialog ->
Allow clicked -> status 'active' -> controller pointer click moved the Windows cursor to the exact
normalized coordinates (cursor + tooltip visible on the Recycle Bin icon; /tmp/r31-vm/r01click2.png).
Keyboard events published under the grant and host-acknowledged (kind=Result) but no visible
key effect was observed (needs host-side focus diagnosis); MetaLeft is refused by design for window
shares (validate_window_key_code). Window row NOT yet live-verified (VM webview picker automation
blocked; see R03 note). -->
<!-- PARTIAL 2026-11-03 unattended actual-desktop check: GTK requests initially
failed locally because the compositor's AppHandle was never initialized.
Initializing it before receiver interaction allowed actual Windows consent,
approval and the native Controlling state. With the released 0.9.33 peer, a
selected Notepad window received pointer focus, letters and Enter. A separate
punctuation defect dropped hyphens; canonical DOM punctuation codes fixed it,
and independent Windows WM_GETTEXT observed the exact "petal-fixed-hyphen".
An independently published Screen 1 also received native pointer focus and
keyboard text "petal-monitor-keyboard" plus numbered lines through line44.
GTK smooth-scroll deltas are wheel detents, not browser pixels. Sending line
units fixed the 40x underscaling: one native detent changed the Windows editor's
first visible line 25 -> 22 through Screen 1 (1793674991252_inputwitness), and
30 -> 26 through its application-window share. Rapid keyboard text
"petal-ordered-edges" arrived exactly, with all keys/buttons up afterward.
Per-event async publication had also reordered pointer Down/Up; GTK now admits
each edge synchronously and uses one ordered publisher for edges, synthetic Ups,
Release and guarded requests. Prepared packets retain their original grant.
Synthetic pointer Ups retain the last known content position even after a
coalesced move has drained. Full target-confinement acceptance remains open.
Final checks pass: cargo build --lib --bin desktop, 73 Linux tests, 13 clipboard
tests and 16 control-core tests. bun run check reports zero errors and one
RoomRow ARIA warning; Vite build passes. The cameraValue declaration-order
type error was corrected. The final rebuilt ELF opens actual native Settings
and visibly renders the honest no-camera state; this does not prove modes
while the physical camera is absent. -->
<!-- PARTIAL 2026-11-03 focus/occlusion follow-through: an independently owned
always-on-top Windows editor covered the authorized editor and held foreground.
Real native-controller key/text/wheel traffic remained confined to the target;
the unshared editor received no key, button or wheel and kept its sentinel.
Covered clicks were refused; cursor restoration caused hover-only WM_MOUSEMOVE.
However, one background KeyZ down/up inserted "zz"; a foreground KeyQ pair
inserted one "q". R01 remains open for this Windows keyboard replay defect.
No Windows replay/target-enforcement change is authorized by this evidence.
See docs/VALIDATION.md and petal-r01-r02-acceptance.json. -->
<!-- VERIFIED 2026-11-04: the separately approved background-key repair closes
the defect above. The actual private GTK received-share window drove one Z pair
to the temporary Windows candidate: one Down, one WM_CHAR and one Up inserted
exactly one z while the owned editor remained background and foreground HWND
stayed unchanged. Foreground GTK typing also inserted one z. The native Windows
message-loop smoke verified exact "bzAé🦀", navigation/modifiers, Unicode,
an unrelated sentinel and focus preservation. Prior native screen/window
pointer, scroll and focus/occlusion evidence remains the basis for those rows.
No target-enforcement policy was changed. See docs/VALIDATION.md. -->
# Requirement: R01
# Living: apps/desktop/features/linux-remote-controller.feature::Linux controls an authorized Windows target
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Linux controls an authorized Windows target
  Given Windows publishes a <target_kind> to the native Linux received-share window
  When Linux requests control and Windows explicitly grants the current target
  Then Linux pointer movement, buttons, scroll and keyboard act through that native picture window
  And <target_enforcement>
  Examples:
    | target_kind | target_enforcement |
    | window | Windows keeps input confined to the authorized window under focus change and occlusion |
    | screen | Windows retains its existing explicitly granted screen or desktop behavior |
```

<!-- VERIFIED rows 2026-11-03 using the actual GTK request/cancel control, native
Windows consent and genuine delayed encrypted UDP over the owned SFU:
Deny's delayed terminal response visibly reported declined. A real cancelled
request and the actual eight-second GTK timeout both rejected the original
host's once-delivered late active response, with an unchanged Windows editor.
The timeout visibly reported "No answer from the sharer."; a fresh explicit
GTK request was required afterward. Holds were 1.99s/1.37s/8.55s, with no
automatic-accept counter delta; the longer private expiry gate had a 10s
watchdog and held 308 datagrams. All owned gates/apps were stopped normally.
Protocol limit remains separate: no request-attempt identifier is echoed, so
an older approval after a NEW explicit same-share request is not distinguished.
These rows do not establish full R02 acceptance while that correlation gap remains. -->
<!-- VERIFIED 2026-11-04: exact request identity closes the intervening-request
gap above. Real Windows Allow A was held on the encrypted incoming UDP path;
GTK cancelled A and explicitly requested B on the same share before release.
A's authenticated awaiting/active/stopped statuses were refused. GTK stayed
"Waiting for approval…" and an actual key press changed nothing. B's own Allow
then enabled one real background key insertion; current Windows revoke stopped
B and another key press changed nothing. Hold 1.612 s, 21 explicitly released
datagrams, zero automatic-accept delta. The prior decline/cancel/expiry rows
remain evidence for those outcomes. Local cancellation A also cannot relabel
itself as Release B: a behavioral regression and independent re-review cover
that separately found/fixed command-consumer defect. -->
# Requirement: R02
# Living: apps/desktop/features/linux-remote-controller.feature::Unsuccessful requests never enable Linux controller input
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Unsuccessful requests never enable Linux controller input
  Given Linux requests control of a Windows share
  When the request is <request_result>
  And a late response for that retired request arrives
  Then Linux visibly reports the non-controlling result and sends no remote input
  And an expired request requires a fresh explicit request
  Examples:
    | request_result |
    | declined |
    | cancelled |
    | expired |
```

<!-- PARTIAL 2026-10-30 live vs released Windows peer:
'Windows departs' row verified at the transport level -- killing the VM Petal process dropped the peer
from the SFU (only LinuxDBG remained) and Linux logged "'Petal Windows Test' disconnected" with the
grant gone; Linux-side Release also cleanly emitted status='stopped'. NOT yet live-verified: explicit
revoke affordance, graceful Leave, transport cable-off, share replacement, and held-input release
during an ending -- blocked by VM-side webview click automation (the Petal Sharer Pointer overlay +
WebView2 input-focus limits; native consent dialogs click fine, in-page webview buttons do not).
Also observed, needs code diagnosis: two exact-name clicks on the toolbar 'Stop sharing' left the
frontend and the SFU video track in disagreement in one intermediate read; later reads showed a
consistent sharing state, so this is recorded as unproven rather than a confirmed defect. -->
<!-- PARTIAL / BLOCKED 2026-11-03: independent Windows GetAsyncKeyState first
proved native Linux Release cleared remote-held Shift before local key-up.
The simultaneous Shift+left-button case then exposed a released Windows 0.9.33
retirement failure. After the Linux ordered-publication correction, both were
held at 1793675662813; native Release occurred at 1793675662892. Shift was up at
1793675663327, before local key-up at 1793675663615, but leftMouse remained down.
Host trace 1793675719184_wheel-log records Key Up seq6, Pointer Up seq7 and Release
received in that order. The host revoked before the queued pointer release
resolved, dropped it as no-longer-authorized/nothing-held, and rejected its own
synthesized pointer Up as "remote input superseded". A PID/title-guarded owned
guest reset released the stranded test button; this is cleanup, not a passing
acceptance result. Windows target enforcement and the released binary remain
unchanged, as required by Task 3.1. No timing sleep or host-version workaround
was added. Windows-initiated revocation, the other retirement rows and stale-
packet isolation remain unverified; R03 cannot pass against this observed path. -->
<!-- Separately authorized Windows prerequisite repair, 2026-11-03:
Tommy authorized fixing the held-input failure and reported the camera working
again. The Windows platform now records successfully injected mouse-button
ownership, including the accepted prefix of a partial Click. Native retirement
releases only those buttons, without a cursor move, target re-resolution, active
grant or cancellable replay. Failed releases retain ownership for retry; another
controller's held button is preserved. Shared grant teardown removes authority
before synchronizing with native cleanup, preventing a late Down after release.
Normal target, integrity, grant and cancellation checks remain in place.
The current Windows test artifact built and ran in the dedicated VM: 25 platform
tests passed. An explicitly enabled real-input fixture observed Left, Middle and
Right Down with GetAsyncKeyState, called the production controller cleanup with
no active grant or queued Up, and observed each native Up. The fixture restored
its cursor and destroyed its own window. This proves native cleanup, not the
full two-client R03 matrix. The installed released Windows peer is unchanged;
R03 remains TODO until a corrected application candidate passes paired testing. -->
<!-- VERIFIED post-login 2026-11-03: all six paired held-Shift/left-button
release rows passed: sharer disables control 33 ms, source Stop 27 ms,
Windows departure 25 ms, Linux leave 124 ms, transport disconnect 156 ms,
replacement 14 ms. Native GetAsyncKeyState and fixture Down/Up events establish
release within 600 ms, before the 1,200 ms expiry backstop.
The final corrected Windows PE additionally passed six delayed-input and six
delayed-approval rows against the actual native Linux app and owned SFU.
A private namespace NFQUEUE gate held genuine encrypted UDP without rewriting
payloads or sender identity, then explicitly released it after retirement.
Every accepted row had zero capacity/watchdog automatic-accept counter deltas;
overflow, timeout and failed harness attempts were excluded.
Global revoke, source Stop and replacement reached Windows native admission
with the original authenticated Linux sender and had no editor effect.
Room departure/disconnection can reject traffic below native admission.
A genuine late approval after local cancellation was also rejected at native
admission. Ordered active/stopped status delivery is not claimed as reordered
application callbacks: the flushed retirement batch leaves no input authority.
This exposed and fixed a Windows prerequisite: revoke_all emitted stopped only
locally; it now sends through the existing forced local+controller status path.
The before/after real-wire witness, independent review (no findings) and 137
shared remote-control tests pass. See docs/VALIDATION.md for the artifact and
evidence boundary. R03 is DONE locally; the installed Windows release is unchanged. -->
# Requirement: R03
# Living: apps/desktop/features/linux-remote-controller.feature::Retire Linux controller authority and held input
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Retire Linux controller authority and held input
  Given Linux controls a Windows share and holds a key and pointer button
  When <control_end>
  Then Linux stops controller input and releases held input through the existing protocol
  And old packets and responses cannot restore that grant
  Examples:
    | control_end |
    | Windows revokes control |
    | Windows stops the source |
    | Windows departs |
    | Linux leaves the room |
    | the transport disconnects |
    | the share instance is replaced |
```

<!-- PARTIAL 2026-11-02: the October 30 released-Windows check used a display
share, so remote_clipboard_copy/paste correctly refused with "remote clipboard
requires an application-window share". Full separately approved Windows
window-share transfers remain unverified.
A device-free private Mutter, private bus, virtual keyboard and synthetic
clipboard providers now exercise the actual SystemClipboardBackend. The smoke
reproduced an added trailing newline on reads; passing --no-newline to wl-paste
fixed it. Actual read/write/transfer reads preserve ASCII, Unicode/multiline,
one or two existing trailing newlines and exactly 1 MiB. Changing the local text
changes the sequence guard. Real URI-list offers are rejected as FileTransfer;
a real GTK HTML-only offer without plain-text aliases is rejected as NoText.
Both rejected source clipboards retain their original bytes.
All 78 focused Linux/clipboard regressions pass. This is native-backend evidence,
not Windows transport, separate-consent or full stale-grant acceptance. All
providers and input stayed inside the owned private compositor; the human
clipboard and VM were untouched. Disposable drivers were removed and private
runtimes stopped; R04 remains TODO. -->
<!-- VERIFIED post-login 2026-11-03: the actual ELF completed fresh onboarding
and joined the owned SFU room on an independent private Mutter display/bus.
The corrected Windows candidate shared an owned native editor, and its real
Allow dialog granted the exact current source. Separately invoked Copy and
Paste preserved Unicode, multiline text and trailing CRLF. Distinct exact
1 MiB UTF-8 payloads passed both directions; clipboard hashes matched and Paste
changed the real Windows editor to the expected bytes.
Every rejection row left both clipboards and the editor unchanged: missing
per-operation approval, 1 MiB + 1 byte, NUL, genuinely retired grant (both Copy
and Paste), file-list MIME and image-only MIME. A published retired-grant
operation had no host effect; publication alone was not counted as success.
The private launcher's WEBKIT_DISABLE_DMABUF_RENDERER=1 caused the earlier
WebKit view-transition abort at FrameRenderer::graphicsLayerFactory. Retaining
the production shared-memory renderer instead restored real route navigation;
no production bypass was added. Accessibility peers used consistent PID
visibility. Source bytes were read back from the private compositor before
each operation rather than assuming GTK set_content had taken ownership.
The host clipboard bridge and drag-and-drop remained disabled. No human
clipboard, media, shared-service configuration or installed Windows app was
changed. Owned providers/input/apps were retired, scratch files removed, the
verified synthetic guest clipboard cleared and the owned SFU room observed empty.
R04 is DONE locally; Task 3.1 and Task 4.1 are not thereby accepted or delivered. -->
# Requirement: R04
# Living: apps/desktop/features/linux-remote-controller.feature::Exchange separately authorized clipboard text as Linux controller
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Exchange separately authorized clipboard text as Linux controller
  Given Linux controls Windows through an active exact-current grant
  When copy and paste are each separately approved and transfer UTF-8 plain text of 1 MiB without NUL
  Then each intended clipboard receives exactly that text in its approved direction
  When <rejected_transfer> is attempted
  Then neither clipboard changes because of that rejected transfer
  Examples:
    | rejected_transfer |
    | copy without separate authorization |
    | paste without separate authorization |
    | text exceeding 1 MiB |
    | text containing NUL |
    | transfer naming a stale grant |
    | file transfer |
    | image transfer |
```

## Slice 4: Allow explicit desktop-wide Linux assistance

**Tracer bullet**: Prepare a monitor control session, then grant the Windows peer explicitly.

**Touches**: Linux portal/EIS backend, common grants, native share metadata and existing consent controls.

**Off-limits**: window-confined Linux input; hidden permission escalation; new Windows wire contracts.

### Task 4.1: Enable Linux desktop control and both clipboard roles safely
<!-- estimate: ~1600–2600 reviewable lines (low confidence) | feature-files: apps/desktop/features/linux-desktop-control.feature | demoable: yes -->
> Depends on: Task 3.1.
> Shippable state: Linux windows stay view-only; one explicit desktop-control session and controller work with same-session picture mapping, complete grant/clipboard validation and full revocation. No successor is needed for safety.
> Boundary verification: With released Windows, deny a Linux window request; prepare a monitor with explicit desktop disclosure and source re-selection; verify fresh-instance grant, scaled pointer corners, keyboard/scroll, competing request rejection and host-side clipboard bounds. Revoke during held input and replay retired packets, including EIS loss before release; observe no input, closed input session and controlled share retired. Repeat X03 UI/direct-command checks: only prepared/granted Linux desktops permit replay; application-window replay is still denied.

<!-- TODO -->
<!-- November 5 actual H01 window check: safety behavior verified, H01 NOT
accepted. Tommy completed the real native chooser for the owned editor only;
linux_picture_shares reported sourceKind=window, one share. Released Windows
decoded that picture, with frames161→269 across the control attempts. Its real
Control button stayed disabled with "Preparing remote control… it will enable
automatically", not an explicit view-only explanation. The real Windows
remote_control_set_active request command fulfilled with false; that return is
not proof of an active grant or an explicit host denial.
The actual Linux global-enable, per-share-enable and answer-consent commands
all rejected with the Linux-host unsupported error. Linux's real menu likewise
marks request/grant unsupported. No owned RemoteDesktop/InputCapture calls or
unattributed calls were observed, and the editor recorded no key/pointer events
or text changes. No camera, unmuted microphone, desktop grant or input injection.
Thus the live picture is safe under the current GLOBAL host refusal, but the
window-specific view-only feedback/host-capability contract remains unverified;
the misleading Windows preparing state must not be counted as H01 completion.
The earlier manual-picker blocker is resolved. Client/editor/guard/helper
cleanup verified, room empty, VM/release unchanged. Current receipts:
/tmp/petal-h01-resume-ri03q_pt/actual-window-boundary-check.json and cleanup.json.
No product code changes, publication or shared-service changes. -->
# Requirement: H01
# Living: apps/desktop/features/linux-desktop-control.feature::Linux window shares cannot grant desktop input
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Linux window shares cannot grant desktop input
  Given Linux shares only the editor application window with Windows
  When Windows attempts to request control and the corresponding Linux commands are invoked directly
  Then Linux explains that application-window shares are view-only for input
  And no input session, desktop grant or injected input results
  And failure never escalates to desktop control
```

<!-- TODO -->
# Requirement: H02
# Living: apps/desktop/features/linux-desktop-control.feature::Prepare and grant Linux desktop input separately
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Prepare and grant Linux desktop input separately
  Given Linux explicitly prepares a monitor with desktop-wide disclosure, valid same-session mapping and keyboard/pointer permission
  Then the ready state shows no controller and injects no input
  When Windows requests that committed current instance and Linux <app_decision> that named controller
  Then <grant_result>
  And a competing preparation or controller request cannot steal the existing session or grant
  Examples:
    | app_decision | grant_result |
    | approves | Windows can control Linux |
    | declines | no input is enabled |
    | cancels | no input is enabled |
    | lets the request expire | no input is enabled |
```

<!-- TODO -->
# Requirement: H02
# Living: apps/desktop/features/linux-desktop-control.feature::Failed native desktop permission preserves picture-only sharing
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Failed native desktop permission preserves picture-only sharing
  Given Linux publishes an ordinary monitor picture without input authority
  When Linux explicitly enables desktop control but the native permission attempt ends with <permission_failure>
  Then the original picture-only share remains unchanged and Linux shows the result
  And the failed attempt's portal/input resources close
  And no participant receives input authority and replay injects nothing
  Examples:
    | permission_failure |
    | denial |
    | cancellation |
    | native permission timeout |
    | missing keyboard permission |
    | missing pointer permission |
```

<!-- TODO -->
# Requirement: H03
# Living: apps/desktop/features/linux-desktop-control.feature::Bind pointer input to the committed control picture
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Bind pointer input to the committed control picture
  Given Linux explicitly prepares a combined desktop-control monitor source
  When source preparation encounters <mapping_case>
  Then <mapping_result>
  And no pending or active grant from the previous share instance survives a committed source replacement
  And input cannot be enabled by comparing opaque source IDs across unrelated sessions
  Examples:
    | mapping_case | mapping_result |
    | a valid same-session region on a 150 percent scaled monitor and a letterboxed Windows picture | a fresh grant maps pointer clicks to all four visible source corners in logical coordinates |
    | a valid region followed by resize | current picture coordinates follow the current logical geometry |
    | explicit selection of another monitor | the newly selected monitor picture is published with a new instance and requires a fresh request and approval |
    | missing mapping metadata | control remains unavailable without guessing coordinates |
    | ambiguous device region | control remains unavailable without guessing coordinates |
    | cancellation before replacement commit | the original picture-only share remains unchanged |
```

<!-- TODO -->
# Requirement: H04
# Living: apps/desktop/features/linux-desktop-control.feature::Disclose desktop-wide scope before and during control
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Disclose desktop-wide scope before and during control
  Given Linux has an ordinary picture-only monitor share and no input grant
  When I open Enable desktop control
  Then Linux discloses desktop-wide keyboard and other input scope including focus changes
  And it explains that the monitor picture does not confine keyboard input
  And it explains that ending control also ends this controlled monitor share
  When a named Windows controller is explicitly granted
  Then the active Linux state names that controller and desktop-wide scope with a visible stop action
  And an ordinary window or monitor share alone never authorizes input
```

<!-- TODO -->
# Requirement: H05
# Living: apps/desktop/features/linux-desktop-control.feature::Reject invalid input and fully retire Linux desktop authority
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Reject invalid input and fully retire Linux desktop authority
  Given Windows controls a Linux monitor with a current grant and held key and pointer button
  And an unrelated editor picture share remains live
  When <retirement_or_invalid_input>
  Then <expected_effect>
  And no stale or invalid packet can enable input after that result
  Examples:
    | retirement_or_invalid_input | expected_effect |
    | local stop | authority retires, held input releases and the input session closes |
    | portal permission revocation | authority retires, held input releases, the input session closes and the unrelated editor share continues |
    | source loss | authority retires, held input releases, the input session closes and the unrelated editor share continues |
    | controller departure | authority retires, held input releases and the input session closes |
    | room leave | authority retires, held input releases and the input session closes |
    | disconnect | authority retires, held input releases and the input session closes |
    | grant expiry | authority retires, held input releases and the input session closes |
    | share instance change | authority retires, held input releases and the input session closes |
    | EIS/device disappears before held-input release can be sent | admission retires first, teardown completes without awaiting release, compositor held state clears and unrelated media continues |
    | invalid token | no input is injected by that packet |
    | wrong controller identity | no input is injected by that packet |
    | retired share instance | no input is injected by that packet |
    | non-finite pointer coordinate | no input is injected by that packet |
    | pointer coordinate outside the allowed normalized range | no input is injected by that packet |
    | late packet from an ended grant | no input is injected by that packet |
```

<!-- TODO -->
# Requirement: H06
# Living: apps/desktop/features/linux-desktop-control.feature::End control without restoring broader authority
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: End control without restoring broader authority
  Given Linux publishes an actively controlled DP-1 monitor plus editor, camera and an unmuted microphone
  When I stop desktop control and the connection later reconnects
  And I retry the retired control operation without fresh source consent or preparation
  Then the input session and DP-1 controlled share end visibly
  And editor, camera and microphone retain their intended states
  And desktop control is not restored by reconnect or retry
  When I want to share DP-1 again
  Then explicit source consent is required and any later control needs fresh preparation and approval
```

<!-- TODO -->
# Requirement: R04
# Living: apps/desktop/features/linux-desktop-control.feature::Exchange separately authorized clipboard text as Linux desktop sharer
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Exchange separately authorized clipboard text as Linux desktop sharer
  Given Windows controls Linux through an active exact-current desktop grant
  When copy and paste are each separately approved and transfer UTF-8 plain text of 1 MiB without NUL
  Then each intended clipboard receives exactly that text in its approved direction
  When <rejected_transfer> is attempted
  Then neither clipboard changes because of that rejected transfer
  Examples:
    | rejected_transfer |
    | copy without separate authorization |
    | paste without separate authorization |
    | text exceeding 1 MiB |
    | text containing NUL |
    | transfer naming a stale grant |
    | file transfer |
    | image transfer |
```

## Slice 5: Validate the whole native collaboration outcome

**Tracer bullet**: Use camera, two shares and both assistance directions on the same owned room.

**Touches**: cross-capability acceptance, living evidence and human feedback.

**Off-limits**: invented delivery or acoustic claims; infrastructure changes; unrequested installation.

### Task 5.1: Record complete interoperability and remaining human acceptance
<!-- estimate: ~250–450 reviewable lines (low confidence) | feature-files: apps/desktop/features/linux-camera-publication.feature, apps/desktop/features/linux-publishing-interop.feature, apps/desktop/features/linux-share-publication.feature | demoable: yes -->
> Depends on: Task 4.1.
> Shippable state: All implementations are already complete and safe; this unit adds whole-outcome acceptance evidence, truthful support documentation and no new capability exposure.
> Boundary verification: Run the real native owned-stack scenario with two Linux shares plus camera, microphone toggles and both control directions. Repeat existing native regressions, inspect publication identities and teardown; record human observations only when actually made.

<!-- TODO -->
# Requirement: S03
# Living: apps/desktop/features/linux-share-publication.feature::Add independent shares up to the supported limit
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Add independent shares up to the supported limit
  Given Linux is publishing monitor DP-1 and the editor window to Windows
  When I add the terminal window and monitor HDMI-1
  Then all four screen/window publications remain independently visible on Windows
  When I try to add a fifth screen/window share
  Then Linux explains that the supported limit is four and preserves all four current shares
  And turning camera publication on does not consume or evict any screen/window slot
```

<!-- TODO -->
# Requirement: S04
# Living: apps/desktop/features/linux-share-publication.feature::Stop one share without stopping other media
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Stop one share without stopping other media
  Given Windows receives Linux shares DP-1 and editor plus Linux camera video
  And the Linux microphone is unmuted
  When I stop only the editor share
  Then only the editor publication and its Windows remote window close
  And DP-1, camera video and the unmuted microphone retain their previous states
```

<!-- TODO -->
# Requirement: C06
# Living: apps/desktop/features/linux-camera-publication.feature::Operate camera and two shares independently
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Operate camera and two shares independently
  Given Windows receives Linux camera and Linux shares DP-1 and editor
  When I mute then unmute the Linux microphone
  Then camera and both picture publications continue
  When I turn camera off and stop only editor
  Then DP-1 and the independently selected microphone state remain unchanged
  And camera did not consume either share slot
```

<!-- TODO -->
# Requirement: X01
# Living: apps/desktop/features/linux-publishing-interop.feature::Interoperate without a second publisher or peer upgrade
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Interoperate without a second publisher or peer upgrade
  Given both native clients explicitly use the same owned backend and trusted-TLS LiveKit service
  When Linux publishes shares and camera and both control directions are exercised with the released Windows client
  Then they use the existing native room identity and compatible media and control contracts
  And no extra publishing participant, browser-media fallback, Windows upgrade or client-side server secret is required
```

<!-- TODO -->
# Requirement: X02
# Living: apps/desktop/features/linux-publishing-interop.feature::Preserve the existing native meeting paths
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Preserve the existing native meeting paths
  Given the <native_client> runs with this task's implemented predecessors and no successors
  When I join and leave a room, use microphone controls, receive an incoming share and move, resize and close its remote window
  Then membership, incoming layout and lifecycle and microphone controls keep their existing supported behavior
  Examples:
    | native_client |
    | Linux |
    | released Windows |
    | supported macOS regression target |
```

<!-- TODO -->
# Requirement: X05
# Living: apps/desktop/features/linux-publishing-interop.feature::Record bounded real-peer acceptance without overclaiming it
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Record bounded real-peer acceptance without overclaiming it
  Given authorized real Linux and released Windows native clients use the owned service and explicitly selected non-sensitive sources
  When I exercise multiple picture shares, camera and both authorized control directions
  Then evidence records actual outcomes without sensitive media or clipboard contents
  And unobserved human camera-picture usability, conversation and acoustic AEC remain explicitly unvalidated
  And synthetic or programmatic checks are not represented as that human acceptance
```

<!-- TODO -->
# Requirement: X06
# Living: apps/desktop/features/linux-publishing-interop.feature::Keep the earlier receive-only acceptance record intact
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Keep the earlier receive-only acceptance record intact
  Given the earlier receive-only plan has earned progress and unfinished speech/AEC acceptance
  When this separate expansion plan is authored and executed
  Then its original scope, historical progress and unfinished acceptance remain unchanged
  And no selected new capability is reported implemented merely by being planned
  And this criterion is checked at the plan/document boundary rather than claimed as an application runtime feature
```

**Accepted limits amendment, 2026-10-29:** Tommy accepted the three
externally evidenced verification limits as platform behavior (ADR 0003):
S06's deny row (no Deny control in GNOME 51), and S07's DP-1-disconnect and
scoped-revocation rows (Mutter rejects zero-monitor configurations on this
single-monitor host; no external per-share revocation API). Those rows moved
out of the scenario Examples into recorded limits, the ledger carries them as
deviation rows, and every remaining Task1.2 scenario — S02, S06 cancel,
S07 editor-close and both S09 behaviors — is DONE with its living feature
updated. Scenario identities remain 36; no completed bookmark moved.

## Planning validation

**Planning result, 2026-10-28:** the full agreed source is planned in 5 slices, 6 ordered tasks and 35 scenario identities. Ledger completeness: **33/33 atomic criteria mapped; 33/33 covered rows have authored scenarios; 0 no-plan, deferred or unapproved-deviation rows.** Every task declares backward-only dependencies, a safe state without successors and concrete exact-head runtime checks. These are planned boundaries, not runtime proofs already obtained.

**Approved amendment, 2026-10-29:** S09 now preserves geometry while capture authority remains valid and requires explicit native re-selection after OS-ended authority, per ADR 0003. The plan has 36 scenario identities across the same 5 slices and 6 tasks. Both S09 scenarios remain in cohesive Task1.2; its dependency, feature-file set and existing estimate are unchanged, and its safe-state/boundary checks now include explicit recovery. No DONE scenario or completed bookmark moved. The S09 ledger quote and pointers are updated with Fidelity unverified; no new runtime acceptance or independent fidelity result is claimed. Existing living specifications are unchanged until the corresponding behavior is built and exercised.

**Independent design reader:** reviewed the polished Technical Spec and linked technical sources, explained the recommendation and cancellation/revocation/camera flows, and identified concrete gaps. Corrections specify camera preview/publication leases and actual factory/status interfaces, PipeWire device/mode enumeration, bounded appsink queues, new GTK controller work and EIS-loss cleanup. Scoped rereads found no remaining design ambiguity or symbol/ownership contradiction. Camera revocation observability and compositor held-input cleanup remain mandatory runtime acceptance gates, not established facts.

**Independent full-source planning audit:** re-derived all criteria and checked verbatim rows, pointers, clauses, task ownership and safe prefixes. Corrections add native permission timeout, separate copy/paste consent and role-owned clipboard scenarios, explicit mode enumeration, active-grant source-loss isolation, retry execution and repeated capability checks at enabling heads. Final scoped recheck found no remaining planned-coverage gap, dangling pointer or unsafe prefix. X06 is checked at the artifact/document boundary, not as an application feature.

**Implementation fidelity:** **0/33 covered rows verified; 33 unverified/not built.** None of these new living scenario identities has been delivered. No living specifications were created to satisfy the audit. A plan or independent reader agreeing with it is not proof of implemented behavior, native media, physical picture usability, speech or acoustic AEC.

**Executed planning checks:** source-to-ledger enumeration and exact quotes; every planned pointer/task owner; scenario metadata and dependency fields; local Markdown link resolution; gitignored ledger exclusion. Read-only `gst-inspect-1.0 pipewiresrc` and `pkg-config` establish installed interface/plugin versions only. No capture, camera or input session was activated. No implementation, push or deployment occurred.

Method provenance: personal Feature planning r8, Plan format r3, Planning checklist r3, Sequentially shippable review units r3 and PRD fidelity audit r3. Technical Spec work used the current ready effective base-r3 procedure with constrained pinned Humanizer finishing before fresh-reader review and after affected prose revisions. These pins record this planning pass; future work loads current canonical Kizn guidance rather than treating this note as its replacement.

Current operation: the two approved local-only prerequisites are complete and R01–R04 are locally verified. Keyboard repair preserves foreground behavior and target confinement; exact-attempt lifecycle correlation rejects delayed A after a fresh same-share B, with fresh B approval and current revocation proven on the real native pair. C02 now has attended ON/ON/OFF physical-indicator evidence and last-owner release; C03, controlled C04 and all C05 failure/recovery rows are locally accepted. Intermittent C920 USB re-enumeration remains observed, not causally resolved or durably repaired. These checks are not publication, full-product human acceptance or Linux-host activation. The affected Technical Spec and ADR projections were reconciled; an isolated reader found their endpoint-choice and camera-fallback ambiguities resolved.

Cleanup and remaining gates: owned clients and temporary capture/input resources are stopped, media room empty, test VM running with clipboard/drag-and-drop disabled and installed release unchanged. Bounded original receipts and the new attended C02/C05 receipts remain. No host firewall/shared-service or Trailboss/Core/pane change, logout/reboot, manual hardware/hub reset, push, PR or deployment occurred. Linux-host Task4.1 and whole-outcome acceptance remain open, including camera with two simultaneous shares and retained human audio/AEC criteria. Earlier evening exclusivity is not current availability; completed camera checks do not implicitly grant desktop-wide input or activate Task4.1.
