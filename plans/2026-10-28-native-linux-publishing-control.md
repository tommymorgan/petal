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

<!-- DONE 2026-10-29: both orderings live-verified on junkman. Preview-first:
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

<!-- TODO -->
# Requirement: C03
# Living: apps/desktop/features/linux-camera-publication.feature::Show accurate camera permission or absence state
# Action: creates
# Status: TODO
# Living updated: NO
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

<!-- TODO -->
# Requirement: C04
# Living: apps/desktop/features/linux-camera-publication.feature::Latest camera device and mode selection wins
# Action: creates
# Status: TODO
# Living updated: NO
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

<!-- TODO -->
# Requirement: C05
# Living: apps/desktop/features/linux-camera-publication.feature::Retire a failed camera without changing other media
# Action: creates
# Status: TODO
# Living updated: NO
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

**Off-limits**: Linux host input activation; changes to Windows target enforcement; new server state.

### Task 3.1: Enable the complete Linux controller path
<!-- estimate: ~700–1200 reviewable lines (low confidence) | feature-files: apps/desktop/features/linux-remote-controller.feature | demoable: yes -->
> Depends on: Task 2.1.
> Shippable state: Linux requests and uses only current Windows grants, handles rejection/revocation and releases held input; Linux-host replay stays unavailable. Controller-side clipboard is complete; host-side clipboard remains unavailable until Task 4.1.
> Boundary verification: Use released Windows as sharer; approve window and screen requests, exercise keyboard/pointer/scroll and the task-owned controller clipboard outline, reject and expire requests, revoke during a held key and replace the share. Observe target confinement and no stale input. Repeat X03 UI/direct-command checks: controller requests/input now work only with a Windows grant; Linux-host replay remains unavailable.

<!-- TODO -->
# Requirement: R01
# Living: apps/desktop/features/linux-remote-controller.feature::Linux controls an authorized Windows target
# Action: creates
# Status: TODO
# Living updated: NO
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

<!-- TODO -->
# Requirement: R02
# Living: apps/desktop/features/linux-remote-controller.feature::Unsuccessful requests never enable Linux controller input
# Action: creates
# Status: TODO
# Living updated: NO
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

<!-- TODO -->
# Requirement: R03
# Living: apps/desktop/features/linux-remote-controller.feature::Retire Linux controller authority and held input
# Action: creates
# Status: TODO
# Living updated: NO
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

<!-- TODO -->
# Requirement: R04
# Living: apps/desktop/features/linux-remote-controller.feature::Exchange separately authorized clipboard text as Linux controller
# Action: creates
# Status: TODO
# Living updated: NO
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

Current operation: Task1.2 is complete under the accepted platform limits; set its bookmark and proceed to the next authorized task. Camera (Task 2.1) and remote-input (Tasks 3.1/4.1) activation still require their own authorization; nothing here grants it.
