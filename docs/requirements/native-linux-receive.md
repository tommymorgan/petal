# Native Linux receive-only meeting milestone

> Source: Tommy's planning interview, 2026-10-14, and accepted peer/hosting changes, 2026-10-27.
> Status: accepted product boundary; detailed behaviors below operationalize that boundary for planning. No pre-existing PRD was supplied or found.
> Implementation status: six scenarios built locally, eleven TODO. Trusted TLS, mutual native membership, join/configuration recovery and Task 2.1's changing native share with independent desktop movement/resize now pass on the selected owned stack. Earlier upstream-hosted observations remain historical. Source fidelity, audio, later lifecycle acceptance and a fresh independent fidelity audit remain pending.

## 1. Outcome and accepted boundary

A locally built native Petal executable runs on this x86_64 Arch-family Wayland desktop. Tommy can join a real Petal room through a Tommy-owned self-hosted stateless backend and LiveKit service, converse through speakers and microphone with a Windows native peer, and receive one share in an independent desktop window. Both native clients explicitly use the same owned service. A browser client, synthetic publisher, mock backend, or `livekit-server --dev` is not final acceptance evidence.

The 2026-10-14 interview selected native peers and normal upstream-hosted rooms. On 2026-10-27 Tommy replaced macOS acceptance with the released Windows native peer, then selected “let's self-host for acceptance,” replacing the upstream-hosted-room requirement. This removes the upstream operator-access dependency, not the need to deploy a real reachable backend and LiveKit service. Speakers and microphone rather than headset-only, two-way audio plus receiving one share, and the exclusions for outgoing screen sharing, camera publication, and remote control remain unchanged. Earlier upstream evidence retains its original environment and is not proof of owned-stack acceptance. See [ADR 0001](../adrs/0001-native-linux-receive-only.md).

## 2. Acceptance criteria

Each numbered item is atomic for the coverage ledger. The concrete lifecycle/error requirements are derived acceptance details, not claims that Tommy specified their implementation.

- **AC1:** Launching the locally built Linux executable on this Wayland desktop opens Petal's native room-entry window without opening a browser or requiring macOS or Windows setup steps.
- **AC2:** Using an existing valid room code, Linux and a released Windows native peer explicitly configured for the same Tommy-owned self-hosted stateless Petal backend and LiveKit service join the same room without server API secrets on either client; both participants see each other in the room.
- **AC3:** In that call, speech from the Linux microphone is audible at the peer and speech from the peer is audible through the Linux speakers while an incoming share is updating.
- **AC4:** In that call using speakers and microphone, a five-minute conversation including overlapping speech completes without a persistent audible echo of the peer's own speech or an escalating feedback loop.
- **AC5:** Muting Linux stops its microphone speech reaching the peer without stopping incoming audio or share updates; unmuting restores outgoing speech, with the visible mute control matching the actual state.
- **AC6:** A share published by the Windows native peer appears on Linux as one independent native desktop window showing the sharer's changing content; moving and resizing it does not move or resize the main meeting window.
- **AC7:** Resizing the remote window preserves the received image's aspect ratio without cropping; changing the source window's dimensions updates the image without stretching, and a source text change remains readable at a one-to-one image scale.
- **AC8:** When the peer stops sharing, its remote window closes while two-way room audio continues; a subsequent share from that peer displays current content in one remote window, not a duplicate or a stale image.
- **AC9:** Leaving the room stops Linux microphone publication and remote speaker playback and closes its remote window; rejoining the same room restores a single call and the active share without duplicate playback or windows.
- **AC10:** When a temporary signaling interruption enters Petal's existing reconnect flow, Linux visibly reports reconnecting; after connectivity returns it rejoins the same room and restores current audio and the active incoming share without duplicate playback or remote windows.
- **AC11:** Screen-sharing publication, local camera publication, and remote-control actions are visibly unavailable on Linux in this milestone; attempting their commands cannot publish those media or inject input.

## 3. Edge cases

- **E1:** An invalid or refused hosted room join shows an actionable error and leaves Linux outside the room, with no microphone publication or remote window; correcting the room code permits a normal join.
- **E2:** If no usable microphone or speaker is available, Linux identifies the unavailable audio direction without crashing or claiming working audio; making that device available and retrying restores the intended audio state without duplicating the room connection.
- **E3:** If the sharer disconnects from the room, Linux removes that participant's remote window rather than leaving a frozen window presented as live; Linux can still leave normally.
- **E4:** If a share publication is replaced before the old publication's delayed teardown arrives, Linux keeps the replacement's current image and exactly one remote window.
- **E5:** A build with no configured hosted backend shows a configuration error when hosted joining is attempted; it does not silently join a local server or another backend, and never requires a LiveKit server API secret on the client.
- **E6:** After leaving one room and joining a different room, late events from the first room cannot reopen its remote window or restart its audio in the second room.

## 4. Non-goals

No outgoing Linux screen capture, local camera publication, or remote control. No new renderer design for the browser, upstream-hosted deployment changes, new codec protocol, general distribution matrix, installer/updater delivery, or guarantee for multiple simultaneous incoming shares. Provisioning the owned acceptance backend and LiveKit service is in scope; this does not authorize paid signup or arbitrary public firewall exposure. These exclusions are deliberate scope, not deferred rows belonging to this milestone. Existing macOS/Windows native behavior must be preserved.

## 5. Acceptance environment and evidence

Run the Linux executable in the actual graphical Wayland session, not merely in a headless test. Deploy a real Tommy-owned stateless Petal backend and LiveKit service and establish reachability from Linux and the released Windows native client before acceptance. Configure both clients explicitly for that same service; do not change global source-build defaults or install server API secrets on either client. The backend holds the LiveKit server credentials and issues scoped client tokens. Self-hosting selection is not evidence of deployment, reachability, or acceptance. Keep room codes, identities, credentials, and personal audio/video out of committed evidence.

For image validation share a non-sensitive editor/test-pattern window with a visible changing counter, corner markers, and known text. Observe receiver pixels, aspect, movement and resize directly. For audio, both ends listen to real speech including overlap with the Linux speakers active; a processing-option flag, synthetic tone, energy counter, or video recording with no audible track is insufficient evidence for AC4. The five-minute duration is a sustained-use observation, not a new latency/performance guarantee.

Record exact app revisions, service environment, peer platform, device names privately, and pass/fail for each exercised behavior. Peer hardware and a listening partner are prerequisites for final acoustic acceptance; unavailable hardware means unvalidated, not passed. An accessible room and an isolated identity actually denied by the owned service are prerequisites for join/recovery evidence; an unknown valid-format room code is not a refusal fixture. Intermediate local/synthetic checks may unblock implementation but cannot replace the owned-stack native call or real speaker/microphone observations. Mocks, dev-server token minting, and processing-option flags do not establish media acceptance. Preserve successful upstream-hosted observations as historical evidence under that environment; do not relabel them as owned-stack proof or reopen completed scenarios solely for this scope change. See [VALIDATION.md](../VALIDATION.md) for the distinction between layers of evidence.
