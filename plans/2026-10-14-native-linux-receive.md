# Feature: Native Linux hosted audio and share receiving

**Created**: 2026-10-14
**Goal**: Use native Petal on this Wayland computer for a hosted two-person speaker/microphone call and one independent incoming-share window with either supported native peer platform.
> Source PRD: [Interview-derived milestone requirements](../docs/requirements/native-linux-receive.md) — no pre-existing PRD; accepted boundary plus explicitly identified derived acceptance details.
> PRD coverage: plans/native-linux-receive.prd-coverage.md
> Context: [Glossary](../CONTEXT.md), [Integration map](../CONTEXT-MAP.md), [Scope ADR](../docs/adrs/0001-native-linux-receive-only.md), [Native media ADR](../docs/adrs/0002-linux-native-media-seams.md)

## Execution and evidence

This is the entire receive-only milestone, not a later-phase placeholder for the wider Linux port. Execute slice/task order below; the receiver slice deliberately precedes integrated audio acceptance because the audio tracer includes an updating incoming share. No living `.feature` files are authored during planning.

No new UI design is being invented: reuse existing room-entry/meeting controls; the remote-window interaction is specified in ADR 0002 using standard GTK desktop decoration. No Figma/fig2json artifact exists, so there are no fabricated Design references.

Task estimates are derived from the source seams described in ADR 0002 and analogous native session/compositor implementations; exclude generated files, lockfiles and vendored upstream bulk. Confidence is low until the Linux compiler diagnostics are grouped and the first real native frame/audio path runs. Two atomic scenarios exceed the ~500-line soft cap and must remain intact: Task 1.1 (~700) and Task 2.1 (~1000). `/slice-split` handles actual oversized diffs after building; do not pre-author later-task living scenarios.

A working build is not acceptance. Build and launch the executable in this computer's graphical Wayland session. Final evidence requires separate hosted calls with real macOS and Windows native peers and real speaker/microphone conversation; absent peers or hardware are reported as unvalidated, not replaced with a browser, local SFU, processing flag, or synthetic tone. Record only non-sensitive evidence using the requirements document's procedure. Plan-time fidelity audit verifies scenario wording only; it never marks behavior delivered.

## Slice 1: Enter a hosted room from the native Linux app

**Tracer bullet**: Launch the real Linux app and join an existing hosted room with a native peer.

**Touches**: Linux startup/module gates, session selection, meeting_core connection, hosted configuration, platform capabilities, existing room UI.

**Off-limits**: No media renderer or capture implementation, no browser fallback, no hosted deployment changes.

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

<!-- TODO -->
# Living: apps/desktop/features/linux-meeting-capabilities.feature::Unsupported publishing and input actions are unavailable
# Action: creates
# Status: TODO
# Living updated: NO
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

<!-- TODO -->
# Living: apps/desktop/features/linux-room-membership.feature::Join a hosted room with each native peer platform
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Join a hosted room with each native peer platform
  Given a released native Petal <peer> client is in hosted room 246810
  And Linux is configured explicitly for that same hosted Petal service
  And no LiveKit server API secret is present or required on the Linux client
  When I enter room code 246810 in the native Linux app and join
  Then Linux joins hosted room 246810 without changing the peer client's server
  And Linux and the <peer> client each show the other participant in the room

  Examples:
    | peer    |
    | macOS   |
    | Windows |
```

### Task 1.4: Report hosted join and configuration failures
<!-- estimate: ~350 reviewable lines | feature-files: apps/desktop/features/linux-room-membership.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-room-membership.feature::Recover from an invalid or refused hosted join
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Recover from an invalid or refused hosted join
  Given Linux is configured for the intended hosted Petal backend
  And joining room 000000 produces <failure>
  When I attempt to join room 000000
  Then I see an actionable join error
  And Linux remains outside the room without microphone publication or a remote window
  When I correct the room code to accessible hosted room 246810 and join
  Then Linux joins room 246810 normally

  Examples:
    | failure                 |
    | an invalid code         |
    | a refused access result |
```

<!-- TODO -->
# Living: apps/desktop/features/linux-room-membership.feature::Missing hosted configuration never falls back silently
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Missing hosted configuration never falls back silently
  Given I launch a Linux build with no baked hosted backend and no runtime backend override
  And no LiveKit server API secret is present on the client
  When I attempt a hosted room join
  Then I see a configuration error naming the missing hosted backend setting
  And Linux does not attempt to join a local server or an alternative backend
  And the app does not ask me to install a LiveKit server API secret
  When I configure only the intended hosted backend URL and restart the app without a server API secret
  And I join accessible hosted room 246810
  Then Linux joins successfully without any client dependency on a LiveKit server API secret
```

## Slice 2: Receive one share in an independent native window

**Tracer bullet**: An existing native peer publishes a changing editor window; Linux displays it in its own movable/resizable GTK window.

**Touches**: Linux compositor, native subscription feed, video_color conversion seam, track identity and cancellation.

**Off-limits**: No outgoing capture, new codecs, browser video bridge, GPU-specific renderer, or new global share cap.

### Task 2.1: Render the first share in a native window
<!-- estimate: ~1000 reviewable lines | feature-files: apps/desktop/features/linux-remote-windows.feature | demoable: yes -->

**Soft-cap flag**: low-confidence estimate; one scenario, preserved atomically.

<!-- TODO -->
# Living: apps/desktop/features/linux-remote-windows.feature::Receive a movable and resizable native share from each peer platform
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Receive a movable and resizable native share from each peer platform
  Given Linux and a native <peer> client are in hosted room 246810
  When the peer shares a non-sensitive editor window with a changing counter
  Then exactly one independent native remote window appears on Linux
  And its pixels show the editor and continuing counter changes
  When I move and resize the remote window using its desktop window controls
  Then the remote window moves and resizes independently
  And the main meeting window keeps its own position and size

  Examples:
    | peer    |
    | macOS   |
    | Windows |
```

### Task 2.2: Preserve source geometry and text
<!-- estimate: ~300 reviewable lines | feature-files: apps/desktop/features/linux-remote-windows.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-remote-windows.feature::Preserve source aspect and readable text across size changes
# Action: creates
# Status: TODO
# Living updated: NO
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

### Task 2.3: Track share replacement and retirement
<!-- estimate: ~220 reviewable lines | feature-files: apps/desktop/features/linux-remote-windows.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-remote-windows.feature::Late teardown cannot retire a replacement share
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario: Late teardown cannot retire a replacement share
  Given Linux is displaying a peer's share publication A
  When replacement publication B for that same share arrives showing "Replacement B"
  And publication A's delayed teardown arrives afterward
  Then exactly one remote window remains open for the share
  And it continues showing publication B's current changing content
  And publication A cannot replace or close that presentation
```

## Slice 3: Converse using native speakers and microphone

**Tracer bullet**: Two people hear each other through the native Linux audio path while the incoming share updates.

**Touches**: Linux session media ownership, transport/audio, PlatformAudio Linux ADM/APM, mute/error controls.

**Off-limits**: No second WebRTC participant, raw alternate mic stack, AI audio, camera publication, or new telemetry subsystem.

### Task 3.1: Attach native two-way room audio
<!-- estimate: ~500 reviewable lines | feature-files: apps/desktop/features/linux-meeting-audio.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-meeting-audio.feature::Converse with each native peer while receiving a share
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Converse with each native peer while receiving a share
  Given Linux and a native <peer> client are in hosted room 246810
  And Linux uses its real microphone and speakers rather than a headset
  And the <peer> client is publishing a share with visibly changing content
  When the Linux user says "Linux microphone check"
  And the peer user says "Peer speaker check"
  Then the peer hears "Linux microphone check"
  And "Peer speaker check" is audible through the Linux speakers
  And the Linux remote window continues displaying changing share content

  Examples:
    | peer    |
    | macOS   |
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
  Given Linux is joining hosted room 246810
  And its <direction> has no usable device
  When audio initialization attempts to use that direction
  Then Linux identifies the unavailable <direction> with an actionable error
  And Linux neither crashes nor claims that direction has working audio
  When I make that device available and retry the failed audio action
  Then the intended <direction> works in room 246810
  And Linux retains one room connection without duplicate microphone publication or playback

  Examples:
    | direction         |
    | microphone input  |
    | speaker output    |
```

### Task 3.3: Validate speaker echo cancellation
<!-- estimate: ~200 reviewable lines | feature-files: apps/desktop/features/linux-meeting-audio.feature | demoable: yes -->

<!-- TODO -->
# Living: apps/desktop/features/linux-meeting-audio.feature::Sustain a speaker and microphone conversation with both peer platforms
# Action: creates
# Status: TODO
# Living updated: NO
```gherkin
Scenario Outline: Sustain a speaker and microphone conversation with both peer platforms
  Given Linux and a native <peer> client are in a hosted two-person call
  And Linux uses the actual microphone and speakers selected for this computer
  When the two people converse for five minutes including overlapping speech
  Then both people can continue their conversation
  And the peer does not hear a persistent audible echo of their own speech
  And no escalating feedback loop occurs
  And acceptance evidence records the peer platform and the actual speaker-and-microphone observation

  Examples:
    | peer    |
    | macOS   |
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
  Given Linux is in hosted room 246810 with audible two-way speech and one incoming share
  When I leave that room
  Then the peer can no longer hear new speech from the Linux microphone
  And the peer's room publication list no longer contains Linux's microphone publication
  And Linux no longer plays that room's remote audio
  And its remote window closes
  When I rejoin hosted room 246810 while the peer's share is still active
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
  Given Linux is in hosted room 246810 with two-way audio and an active incoming share
  When a temporary signaling interruption enters Petal's existing reconnect flow
  Then the native Linux app visibly reports reconnecting
  When connectivity returns
  Then Linux is in hosted room 246810 again
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
  Given Linux has left room 246810 and joined a different room 135790
  When late events from room 246810 arrive after that room switch
  Then no room 246810 remote window opens
  And no room 246810 audio restarts
  And Linux remains in room 135790
```

