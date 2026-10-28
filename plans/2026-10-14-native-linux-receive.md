# Feature: Native Linux self-hosted audio and share receiving

**Created**: 2026-10-14
**Goal**: Use native Petal on this Wayland computer for a two-person speaker/microphone call and one independent incoming-share window with the released Windows native peer, both explicitly configured for the same Tommy-owned self-hosted stateless backend and LiveKit service.
> Source PRD: [Interview-derived milestone requirements](../docs/requirements/native-linux-receive.md) — no pre-existing PRD; accepted boundary plus explicitly identified derived acceptance details.
> PRD coverage: plans/native-linux-receive.prd-coverage.md
> Context: [Glossary](../CONTEXT.md), [Integration map](../CONTEXT-MAP.md), [Scope ADR](../docs/adrs/0001-native-linux-receive-only.md), [Native media ADR](../docs/adrs/0002-linux-native-media-seams.md)

## Execution and evidence

This is the entire receive-only milestone, not a later-phase placeholder for the wider Linux port. Execute slice/task order below; the receiver slice deliberately precedes integrated audio acceptance because the audio tracer includes an updating incoming share. No living `.feature` files are authored during planning.

No new UI design is being invented: reuse existing room-entry/meeting controls; the remote-window interaction is specified in ADR 0002 using standard GTK desktop decoration. No Figma/fig2json artifact exists, so there are no fabricated Design references.

Task estimates are derived from the source seams described in ADR 0002 and analogous native session/compositor implementations; exclude generated files, lockfiles and vendored upstream bulk. Confidence is low until the Linux compiler diagnostics are grouped and the first real native frame/audio path runs. Two atomic scenarios exceed the ~500-line soft cap and must remain intact: Task 1.1 (~700) and Task 2.1 (~1000). `/slice-split` handles actual oversized diffs after building; do not pre-author later-task living scenarios.

A working build is not acceptance. Build and launch the executable in this computer's graphical Wayland session. Final evidence requires a real owned-stack call with the released Windows native peer and real speaker/microphone conversation; absent hardware is reported as unvalidated, not replaced with a browser, mock, dev-server token mint, processing flag, or synthetic tone. Tommy selected Windows instead of macOS on 2026-10-27, then selected “let's self-host for acceptance,” replacing the upstream-hosted-room requirement. Record only non-sensitive evidence using the requirements document's procedure. Plan-time fidelity audit verifies scenario wording only; it never marks behavior delivered.

**Global acceptance environment**: a real Tommy-owned stateless Petal backend and official non-development LiveKit service run on this workstation and are reachable from both native clients over trusted TLS. Linux and the released Windows client are explicitly configured for that same service; server API secrets stay on the backend/LiveKit service, never either client. The bounded placement is recorded in ADR 0001. No paid signup, public firewall/router/DNS change, or cloud deployment was used. “Hosted” in the preserved scenarios denotes this explicitly configured real service, not an upstream-only dependency. Task 1.3's preserved scenario describes membership after both peers are configured; it does not prohibit the accepted one-time Windows service configuration.

**Evidence continuity**: all previously DONE scenario text, living identities and task names remain unchanged. Eight scenarios are built locally and nine remain TODO. The earlier 2026-10-27 Task 1.3 membership and Task 1.4 partial observations remain historical upstream-hosted evidence. New owned-stack proof independently establishes trusted TLS, released Windows and Linux mutual membership, malformed-input recovery, actual server-refused identity recovery, same-profile URL-only configuration recovery, Task 2.1's changing native share with independent desktop movement/resize, and Task 2.2's source aspect, letterboxing and native-pixel text after source-size changes. Task 2.3 additionally passes the actual WGC B-before-late-A race under the narrow authorized Windows publisher exception. Audio and later lifecycle acceptance remain pending.

## Slice 1: Enter a hosted room from the native Linux app

**Tracer bullet**: Launch the real Linux app and join an existing hosted room with a native peer.

**Touches**: Linux startup/module gates, session selection, meeting_core connection, hosted configuration, platform capabilities, existing room UI.

**Off-limits**: No media renderer or capture implementation, no browser fallback, no upstream-hosted deployment changes. Provisioning the owned acceptance environment is separate from this client's membership implementation.

### Task 1.1: Launch the Linux desktop app
<!-- estimate: ~700 reviewable lines | feature-files: apps/desktop/features/linux-room-membership.feature | demoable: yes -->

**Soft-cap flag**: low-confidence estimate; one scenario, preserved atomically.

<!-- DONE -->
# Living: apps/desktop/features/linux-room-membership.feature::Launch the Linux app in the graphical session
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Launch the Linux app in the graphical session
  Given the locally built Linux executable is available on this computer
  And this computer is running its actual Wayland desktop session
  When I launch that executable from the documented Linux command
  Then Petal opens its native room-entry window
  And no browser is opened
  And neither preparing nor launching that executable requires macOS or Windows setup steps
```

### Task 1.2: Expose the receive-only capability boundary
<!-- estimate: ~250 reviewable lines | feature-files: apps/desktop/features/linux-meeting-capabilities.feature | demoable: yes -->

<!-- DONE -->
# Living: apps/desktop/features/linux-meeting-capabilities.feature::Unsupported publishing and input actions are unavailable
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Unsupported publishing and input actions are unavailable
  Given the native Linux app is running
  When I inspect its room controls and invoke each unsupported command directly
  Then each action in the following table is visibly unavailable with a Linux limitation explanation
  And each corresponding command returns an explicit unsupported error
  And none of the listed effects occurs
    | action                 | prohibited effect              |
    | publish a screen share | local screen media publication |
    | publish the camera     | local camera media publication |
    | request remote control | control request sent to peer   |
    | grant remote control   | local input injection enabled  |
    | send remote input      | input event sent to peer       |
    | replay remote input    | local OS input injected        |
  And room joining, microphone controls, and receiving a share remain available
  And no unsupported command can publish local screen or camera media or inject input
```

### Task 1.3: Join normal hosted rooms
<!-- estimate: ~450 reviewable lines | feature-files: apps/desktop/features/linux-room-membership.feature | demoable: yes -->
> Depends on: Task 1.2.
> Shippable state: the native Linux client can join and leave the configured hosted service with the Windows peer; unsupported publication/control actions remain unavailable, and received media/audio are not claimed before their later tasks.
> Boundary verification: run the exact task-head Linux executable without successor changes against the released Windows client; observe mutual participant presence, leave, and rejoin without duplicate membership.

**Soft-cap flag**: actual implementation and native-path verification exceed the estimate; kept as one cohesive membership boundary rather than splitting registration, connection ownership, or participant evidence into unusable prefixes.

**Verified 2026-10-27**: actual Linux/Windows mutual presence, Leave and fresh-launch rejoin without duplicate participants; Windows returned to its sole local tile after Linux left. Focused and exact living checks pass, including publishing/control denials while joined. An isolated removed-presence-bridge mutation fails both checks; restoring it restores both. One bounded basic review found a participant-name substring false pass in the smoke, corrected to distinct identity-keyed tiles. Rust: 1,330 passed, 2 ignored; desktop: 982 + 96 passed, 1 Windows-only skip; supported Linux Clippy targets and changed-module formatting pass; Svelte check has 0 errors and 1 existing warning. All-target Clippy still includes incompatible historical macOS probes. Audio and incoming-screen rendering remain unverified.


<!-- DONE -->
# Living: apps/desktop/features/linux-room-membership.feature::Join a hosted room with the Windows native peer
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Join a hosted room with the Windows native peer
  Given a released native Petal <peer> client is in hosted room abc-defg-hjk
  And Linux is configured explicitly for that same hosted Petal service
  And no LiveKit server API secret is present or required on the Linux client
  When I enter room code abc-defg-hjk in the native Linux app and join
  Then Linux joins hosted room abc-defg-hjk without changing the peer client's server
  And Linux and the <peer> client each show the other participant in the room

  Examples:
    | peer    |
    | Windows |
```

### Task 1.4: Report hosted join and configuration failures
<!-- estimate: ~350 reviewable lines | feature-files: apps/desktop/features/linux-room-membership.feature | demoable: yes -->
> Depends on: Task 1.3.
> Shippable state: failed Linux hosted joins remain outside the room with actionable recovery; a missing backend never falls through to local token minting. Existing hosted membership and native peer behavior remain intact without media successors.
> Boundary verification: exercise malformed invites, an actual server-refused identity and an unconfigured Linux build through native entry; correct the input or backend and join the released Windows peer without server secrets.

The invalid fixture uses an explicit invite URI: bare `000000` is a valid new meeting display name in the existing smart field, not an attempted room-code join. The owned-stack acceptance fixtures must include an accessible room and an isolated participant identity with actual server-side denial state. `abc-defg-hjk` and `def-ghjk-mno` remain public example codes, not proof of accessibility or refusal; an unknown valid-format code is not a denied-identity fixture.

**Historical verification 2026-10-27**: missing configuration reproduced a generic error after an unintended debug-mint fallback. The corrected native path names `PETAL_BACKEND_URL`, stays outside the room with no remote window or owned IPv4/IPv6 join destination, and recovers after a URL-only restart using the same profile and participant identity. Removing the setting-specific message made both the unit and native living assertions fail; restoring it restored green. Those earlier runs used the upstream-hosted Windows room and are not relabeled as owned-stack evidence.

**Owned-stack verification 2026-10-27**: the real backend handlers and official non-development LiveKit 1.13.11 run behind a trusted certificate. Windows reaches the backend without bypassing verification, creates its room through the released native client, and is independently observed in the owned LiveKit room. The parent creates a separate refusal room, reads the actual fresh Linux onboarding identity through the private `0700`/`0600` handshake, records its removal through the real admin handler, confirms token `403`, and only then resumes native Join. The real refusal exposed Rust's `HTTP 403 Forbidden` status phrase, which the original exact-status UI matcher misclassified as a connection failure. The corrected consumer regression fails before the fix and passes afterward. Both strict invalid/refused living rows now pass against the owned service: actionable error, no native membership or remote window, corrected-code membership with the Windows peer, and clean Leave. A Windows screenshot independently shows both native participants. The strict missing-configuration scenario also passes against the owned room, retaining the same profile and identity and proving zero owned IPv4/IPv6 join destinations before the URL-only restart. Client environments contain no LiveKit server API secrets. No upstream mutation, public network change, media acceptance or independent fidelity audit is claimed.

**Integration**: Rust library tests previously passed 1,330 tests with 2 ignored; supported Linux Clippy targets passed with existing warnings. The changed guard is formatted; the token module's existing formatting hunk bodies match Task 1.3's base exactly, so whole-module formatting remains a recorded baseline failure rather than a claimed PASS. The final desktop suite and Svelte gate are recorded in `docs/VALIDATION.md`. One fresh-launch setup encountered a stale AT-SPI node before onboarding; a separate owned launch completed the behavioral check. Native diagnostics also encountered a blanked desktop and were repeated with the actual unlocked desktop awake and a process-scoped idle inhibitor. No retry policy or rendering workaround was added to production code.

<!-- DONE -->
# Living: apps/desktop/features/linux-room-membership.feature::Recover from an invalid or refused hosted join
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Recover from an invalid or refused hosted join
  Given Linux is configured for the intended hosted Petal backend
  And joining <attempted input> produces <failure>
  When I attempt to join using <attempted input>
  Then I see an actionable join error
  And Linux remains outside the room without microphone publication or a remote window
  When I correct the room code to accessible hosted room abc-defg-hjk and join
  Then Linux joins room abc-defg-hjk normally

  Examples:
    | attempted input     | failure                 |
    | petal://join/000000 | an invalid code         |
    | def-ghjk-mno        | a refused access result |
```

<!-- DONE -->
# Living: apps/desktop/features/linux-room-membership.feature::Missing hosted configuration never falls back silently
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Missing hosted configuration never falls back silently
  Given I launch a Linux build with no baked hosted backend and no runtime backend override
  And no LiveKit server API secret is present on the client
  When I attempt a hosted room join
  Then I see a configuration error naming the missing hosted backend setting
  And Linux does not attempt to join a local server or an alternative backend
  And the app does not ask me to install a LiveKit server API secret
  When I configure only the intended hosted backend URL and restart the app without a server API secret
  And I join accessible hosted room abc-defg-hjk
  Then Linux joins successfully without any client dependency on a LiveKit server API secret
```

## Slice 2: Receive one share in an independent native window

**Tracer bullet**: An existing native peer publishes a changing editor window; Linux displays it in its own movable/resizable GTK window.

**Touches**: Linux compositor, native subscription feed, video_color conversion seam, track identity and cancellation.

**Off-limits**: No outgoing capture, new codecs, browser video bridge, GPU-specific renderer, or new global share cap.

### Task 2.1: Render the first share in a native window
<!-- estimate: ~1000 reviewable lines | feature-files: apps/desktop/features/linux-remote-windows.feature | demoable: yes -->

**Soft-cap flag**: low-confidence estimate; one scenario, preserved atomically.
> Depends on: Task 1.4.
> Shippable state: Linux can receive a real Windows window share into one independently managed native window while existing membership, Leave and unsupported-publication boundaries remain intact; audio is still unavailable and is not needed to make this receive-only window usable.
> Boundary verification: at the Task 2.1 head with no successors, publish the actual safe Windows editor through released Petal, observe continuing editor-counter pixels in exactly one Linux native window, move and resize it through desktop controls without changing the main window geometry, then Leave and confirm membership and the remote window are gone.

<!-- DONE -->
# Living: apps/desktop/features/linux-remote-windows.feature::Receive a movable and resizable native share from the Windows peer
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario Outline: Receive a movable and resizable native share from the Windows peer
  Given Linux and a native <peer> client are in hosted room abc-defg-hjk
  When the peer shares a non-sensitive editor window with a changing counter
  Then exactly one independent native remote window appears on Linux
  And its pixels show the editor and continuing counter changes
  When I move and resize the remote window using its desktop window controls
  Then the remote window moves and resizes independently
  And the main meeting window keeps its own position and size

  Examples:
    | peer    |
    | Windows |
```

> Verified locally on 2026-10-27: actual released Windows native share, continuing editor-counter pixels before and after normal OS Move/Resize, compositor-observed remote movement with unchanged meeting position/size, and normal Leave removing membership and the remote window before process teardown. Desktop observation was explicitly authorized; a non-resizable production mutant was rejected by the complete living outline. No audio or later task is accepted by this boundary.

### Task 2.2: Preserve source geometry and text
<!-- estimate: ~300 reviewable lines | feature-files: apps/desktop/features/linux-remote-windows.feature | demoable: yes -->

> Depends on: Task 2.1.
> Shippable state: The existing native receive window preserves the complete source aspect and black letterboxing across receiver and source size changes, and displays current source text at actual decoded one-to-one scale; Task 2.3 and audio successors are not required for this safe receive-only state.
> Boundary verification: At the Task 2.2 head with predecessors present and no successors, run check-linux-receive-fidelity.py against the released Windows native peer on the owned backend and LiveKit service; require all four source corners and initial text, normal OS receiver resize with min-fit geometry and black bars, actual peer source dimension/text changes, exact native viewport equality to Cairo-accepted decoded dimensions with scale 1 and current-text OCR, then normal Leave removing membership and the remote window. An isolated production min-to-max scaling mutant must fail this same scenario.

<!-- DONE -->
# Living: apps/desktop/features/linux-remote-windows.feature::Preserve source aspect and readable text across size changes
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Preserve source aspect and readable text across size changes
  Given a peer shares an editor window containing "Petal Linux 12345" and four visible corner markers
  And Linux displays that share in its native remote window
  When I resize the remote window to a different aspect ratio
  Then the whole received image remains visible without cropping or stretching
  And any unused area is letterboxed
  When the peer changes the source window dimensions and changes the text to "Petal Linux 67890"
  Then Linux displays the new source aspect ratio without stretching
  And all four corner markers remain visible
  When I view the received image at one-to-one image scale
  Then "Petal Linux 67890" is readable rather than the previous text
```

> Verified locally on 2026-10-27 by the parent on the actual released Windows native peer and owned service: the complete staged scenario passed on the original Linux room connection, including normal OS resize to a 1000×900 DrawingArea, source change from the measured initial DWM frame to 790×645 with new text and all four corner markers, centered min-fit geometry and black letterboxes, and normal OS contraction to the actual 788×644 decoded image dimensions. The final native DrawingArea was exactly 788×644 with scale 1; native-pixel OCR read "Petal Linux 67890" rather than the previous text. Normal Leave cleared membership and removed the native remote window. The unchanged production min-scale formula already satisfied the scenario; replacing min with max in an isolated copy caused the complete witness to reject cropped initial text after receiver resize (exit 1), with normal Leave cleanup. The candidate scenario run exited 0. DWM source bounds, decoded image dimensions and nonblack content bounds are distinct; a black source-border row is not a scale discrepancy. No Task 2.3 or audio behavior is accepted by this boundary.

> Final boundary checks on 2026-10-28: the post-review executable passed the exact complete living scenario via `--feature apps/desktop/features/linux-remote-windows.feature`, including source transition, decoded-size one-to-one text and normal Leave. One bounded independent basic review found no demonstrated defects. The changed Rust module passed rustfmt; serial library tests passed 1,332 with two ignored and Clippy completed with existing warnings. Svelte reported zero errors and one existing warning; the retained isolated Playwright runtime passed 984 frontend tests plus 96 harness tests, one Windows-only skip and panel-close. An initial parallel descriptor-count test race and a first frontend invocation missing its established compatibility runtime are documented in `docs/VALIDATION.md`; neither was hidden or patched into this unit.

### Task 2.3: Track share replacement and retirement
<!-- estimate: ~220 reviewable lines | feature-files: apps/desktop/features/linux-remote-windows.feature | demoable: yes -->

<!-- DONE -->
# Living: apps/desktop/features/linux-remote-windows.feature::Late teardown cannot retire a replacement share
# Action: creates
# Status: DONE
# Living updated: YES
```gherkin
Scenario: Late teardown cannot retire a replacement share
  Given Linux is displaying a peer's share publication A
  When replacement publication B for that same share arrives showing "Replacement B"
  And publication A's delayed teardown arrives afterward
  Then exactly one remote window remains open for the share
  And it continues showing publication B's current changing content
  And publication A cannot replace or close that presentation
```

> Acceptance-fixture decision on 2026-10-28: the released Windows sender does not republish on resize; normal stop/start changes share identity, and the bounded Windows-only NIC interruption retired A before any B. Tommy authorized a dedicated Windows native publisher for this task only: actual selected-HWND WGC pixels, the same real owned backend and LiveKit, and explicit B publication before A retirement. The released Windows app remains the final compatibility peer; no browser, synthetic media or receiver lifecycle injection is permitted.

> Boundary proof on 2026-10-28: the original Linux room connection subscribed A, then distinct-SID B for the same share, then received A's actual delayed unpublication. The exact complete living scenario observed three increasing source counters in A, three in B before teardown, and three newer B counters afterward, with the same one native remote window and unchanged meeting object. Normal Leave cleared membership and removed the remote window. An isolated copy with only the publication-SID guard removed established A and B, then failed at the missing native window after A retirement; normal Leave still completed. The production guard already worked and remains unchanged.

> One bounded basic review found an exact-HWND selection defect in the new fixture. A real hidden-selected/visible-sibling Windows witness reproduced capture of the unselected same-process/title sibling. The fixture now checks the unfiltered native HWND set before and after picker resolution and fails closed on ambiguity; the same actual witness then exited one before capture. Scoped re-review closed the finding. The corrected Windows binary passed byte-verified VM transfer and the full native `--feature` race again. Windows and Linux builds, serial library tests (1,332 passed, two ignored), Linux and Windows Clippy, changed-module rustfmt, Svelte (zero errors, one existing warning), and frontend checks (984 plus 96 passed, one Windows-only skip and panel-close passed) completed. Existing warnings remain; no audio or later lifecycle claim is made.

## Slice 3: Converse using native speakers and microphone

**Tracer bullet**: Two people hear each other through the native Linux audio path while the incoming share updates.

**Touches**: Linux session media ownership, transport/audio, PlatformAudio Linux ADM/APM, mute/error controls.

**Off-limits**: No second WebRTC participant, raw alternate mic stack, AI audio, camera publication, or new telemetry subsystem.

### Task 3.1: Attach native two-way room audio
<!-- estimate: ~500 reviewable lines | feature-files: apps/desktop/features/linux-meeting-audio.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-meeting-audio.feature::Converse with the Windows native peer while receiving a share
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Converse with the Windows native peer while receiving a share
  Given Linux and a native <peer> client are in hosted room abc-defg-hjk
  And Linux uses its real microphone and speakers rather than a headset
  And the <peer> client is publishing a share with visibly changing content
  When the Linux user says "Linux microphone check"
  And the peer user says "Peer speaker check"
  Then the peer hears "Linux microphone check"
  And "Peer speaker check" is audible through the Linux speakers
  And the Linux remote window continues displaying changing share content

  Examples:
    | peer    |
    | Windows |
```

### Task 3.2: Keep audio state truthful and recoverable
<!-- estimate: ~500 reviewable lines | feature-files: apps/desktop/features/linux-meeting-audio.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-meeting-audio.feature::Mute affects only outgoing microphone speech
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Mute affects only outgoing microphone speech
  Given Linux is conversing with a native peer while receiving updating share content
  When I mute Linux using the visible microphone control and speak
  Then the peer cannot hear that new Linux speech
  And Linux still plays incoming peer speech and displays share updates
  And the visible microphone control shows muted
  When I unmute Linux using the same control and speak
  Then the peer hears the new Linux speech
  And the visible microphone control shows unmuted
```

<!-- TODO -->
# Living: apps/desktop/features/linux-meeting-audio.feature::Recover an unavailable microphone or speaker without duplicating the call
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Recover an unavailable microphone or speaker without duplicating the call
  Given Linux is joining hosted room abc-defg-hjk
  And its <direction> has no usable device
  When audio initialization attempts to use that direction
  Then Linux identifies the unavailable <direction> with an actionable error
  And Linux neither crashes nor claims that direction has working audio
  When I make that device available and retry the failed audio action
  Then the intended <direction> works in room abc-defg-hjk
  And Linux retains one room connection without duplicate microphone publication or playback

  Examples:
    | direction         |
    | microphone input  |
    | speaker output    |
```

### Task 3.3: Validate speaker echo cancellation
<!-- estimate: ~200 reviewable lines | feature-files: apps/desktop/features/linux-meeting-audio.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-meeting-audio.feature::Sustain a speaker and microphone conversation with the Windows peer
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Sustain a speaker and microphone conversation with the Windows peer
  Given Linux and a native <peer> client are in a hosted two-person call
  And Linux uses the actual microphone and speakers selected for this computer
  When the two people converse for five minutes including overlapping speech
  Then both people can continue their conversation
  And the peer does not hear a persistent audible echo of their own speech
  And no escalating feedback loop occurs
  And acceptance evidence records the peer platform and the actual speaker-and-microphone observation

  Examples:
    | peer    |
    | Windows |
```

## Slice 4: Keep the call and remote window correct through lifecycle changes

**Tracer bullet**: Stop and restart a share while room audio continues, then leave/rejoin and reconnect without stale media.

**Touches**: Linux room generation and cancellation, subscriber teardown, existing reconnect flow, audio and window cleanup.

**Off-limits**: No new retry architecture or cross-platform media redesign; no browser or synthetic-peer acceptance substitution.

### Task 4.1: Retire stopped or disconnected shares
<!-- estimate: ~380 reviewable lines | feature-files: apps/desktop/features/linux-remote-windows.feature, apps/desktop/features/linux-room-lifecycle.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-remote-windows.feature::Stop and restart a share without interrupting audio
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Stop and restart a share without interrupting audio
  Given Linux is in a two-way audio call with a peer and displays that peer's active share
  When the peer stops sharing
  Then that share's remote window closes
  And two-way room audio continues
  When the peer starts a subsequent share showing "New share content"
  Then Linux displays "New share content" in one remote window
  And no duplicate window or previous frozen image is presented as current
```

<!-- TODO -->
# Living: apps/desktop/features/linux-room-lifecycle.feature::Retire a disconnected sharer presentation
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Retire a disconnected sharer presentation
  Given Linux is in a room displaying a participant's active share in a remote window
  When that sharer disconnects from the room
  Then that participant's remote window is removed
  And no frozen window is presented as that participant's live share
  When I leave the room
  Then Linux returns to its room-entry state normally
```

### Task 4.2: Leave and rejoin without resource leaks
<!-- estimate: ~280 reviewable lines | feature-files: apps/desktop/features/linux-room-lifecycle.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-room-lifecycle.feature::Leave and rejoin without leaking audio or windows
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Leave and rejoin without leaking audio or windows
  Given Linux is in hosted room abc-defg-hjk with audible two-way speech and one incoming share
  When I leave that room
  Then the peer can no longer hear new speech from the Linux microphone
  And the peer's room publication list no longer contains Linux's microphone publication
  And Linux no longer plays that room's remote audio
  And its remote window closes
  When I rejoin hosted room abc-defg-hjk while the peer's share is still active
  Then two-way audio and that active share return
  And there is one Linux call connection with no duplicate playback or remote window
```

### Task 4.3: Integrate existing room reconnection
<!-- estimate: ~450 reviewable lines | feature-files: apps/desktop/features/linux-room-lifecycle.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-room-lifecycle.feature::Reconnect without duplicate room media
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Reconnect without duplicate room media
  Given Linux is in hosted room abc-defg-hjk with two-way audio and an active incoming share
  When a temporary signaling interruption enters Petal's existing reconnect flow
  Then the native Linux app visibly reports reconnecting
  When connectivity returns
  Then Linux is in hosted room abc-defg-hjk again
  And current two-way audio and the active share resume
  And no duplicate playback or remote windows appear
```

### Task 4.4: Reject stale events after changing rooms
<!-- estimate: ~250 reviewable lines | feature-files: apps/desktop/features/linux-room-lifecycle.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-room-lifecycle.feature::Old room events cannot revive media in a new room
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Old room events cannot revive media in a new room
  Given Linux has left room abc-defg-hjk and joined a different room def-ghjk-mno
  When late events from room abc-defg-hjk arrive after that room switch
  Then no room abc-defg-hjk remote window opens
  And no room abc-defg-hjk audio restarts
  And Linux remains in room def-ghjk-mno
```

