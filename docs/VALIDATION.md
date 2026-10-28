# Validation — how Petal knows whether it works

This is the canonical map from **question** to **harness**: which instrument is
allowed to answer which question, and what is currently unanswered. Its
companion is `docs/TESTING.md`, the how-to-run reference (and
`docs/TESTING-START-HERE.md` if you are new). Read this document
when deciding **"is this evidence enough to close the issue?"** — that decision
has gone wrong here more than 30 confirmed times in one audit (2026-07-28,
issues #621–#624), always in the same way: *a signal was read as distinguishing
two states it cannot distinguish.* The lead exhibit: this repo's own
`docs/TESTING.md` cockpit-status block published "passed 6/6" from a run
artifact containing **no conclusion event at all**, while the real latest run
was a `SHARE-N2N` infra-fail. The document recording whether the system worked
was itself unable to tell.

## The four axes

A validation setup is a point in a four-axis space, each axis running cheap →
real. **The axes are independent** — a harness can be maximally real on one
axis and synthetic on another, and most of ours are (the cockpit is prod
transport + real capture + one host; `camera_cadence_probe` is local transport
+ synthetic source + cross-architecture endpoints).

| Axis | cheap → real |
|---|---|
| **Transport** | none → local SFU (`livekit-server --dev`) → cloud SFU → cloud over real distance |
| **Source** | synthetic frames → test-pattern window → real app window → real user content |
| **Endpoints** | web ⇄ web (control) → web ⇄ native → native ⇄ native → cross-architecture |
| **Hosts** | one machine → two machines |

Notes vs. the naive version of these axes, learned the hard way:

- **A publisher with no subscriber is not a cheap endpoint rung — it is off the
  axis entirely.** `publish_probe` alone measures a pipeline whose dominant
  cost (the receive-side jitter buffer: **131.6 of 138 ms** measured on real
  capture) does not exist. It reported ~20 ms flat with a ~0 ms jitter buffer
  across every simulcast ladder; two rounds of ladder conclusions drawn from it
  inverted on real capture hours later (`internal/docs/COURSE_CORRECTION.md`, amended
  fast-probes rule). A lower layer does not even *bound* the real number.
- **Web ⇄ web is a control, not "the easy tier".** It contains zero Petal
  native code; a defect reproducing there is not ours. Its value is
  discriminating "our pipeline" from "the media server / the browser".
- **Cross-architecture does not require two hosts.** The universal build's
  x86_64 slice under Rosetta 2 runs the real x86_64 code path (SIMD selection,
  codec/pixel-format branches) on this machine. It does not exercise Intel
  silicon/GPU/hardware encoders, and its timings are pessimistic — a defect
  that reproduces is real; one that doesn't is weak evidence, say so.
- **The two-host end of the Hosts axis is closed by policy.** There is no
  second Mac and never will be (`internal/docs/COURSE_CORRECTION.md` §3). Everything
  above one host is either re-scoped locally or honestly marked unvalidated.

## The ladder

Layers, what each can answer, and what it structurally cannot. A harness at
layer N may never decide a question that belongs to layer >N (Rule 1 below).

| # | Layer | Answers | Cannot answer |
|---|---|---|---|
| 0 | Pure unit / contract tests | arithmetic; wire-format lockstep across Rust/backend/web | whether anything real calls it, in the right order (see CLAUDE.md's #497 pattern) |
| 1 | Real capture or real UI, no transport | ScreenCaptureKit geometry/content (`capture_probe`); AppKit env (`bare_window_probe`) | anything about publish/subscribe or latency |
| 2 | Real native app as injection target, local transport | remote-control replay on real UI (TextEdit matrix) | anything about video |
| 3 | Local SFU + synthetic frames | wire format, pub/sub plumbing, event ordering a real SFU produces, cross-arch pipeline deltas | **latency, at all** (dominant cost absent — see above); real-capture cadence |
| 4 | Local SFU + real capture | config ranking, per-stage attribution, press-to-photon estimates | absolute production latency; prod server behavior |
| 5 | Cloud SFU, one machine | does the prod server behave like `--dev`; real token/backend path | network distance effects; independent clocks (sender and receiver share one clock and one CPU) |
| 6 | Cloud, two machines, same platform | real clocks, no shared CPU/GPU contention | cross-platform negotiation — **no live harness; closed by policy** |
| 7 | Cloud, two machines, cross-platform | codec/arch negotiation on real hardware | — (**no live harness**; the code-path half is answerable at L3/4 via Rosetta) |
| 8 | L7 + real user content and network | what users actually get | — (**nothing exists here**) |

Windows is a supported second native platform, so tiers 6 and 7 are meaningful
for Windows hardware and Mac ↔ Windows sessions. No cross-platform live harness
is maintained here; those tiers remain validation gaps rather than missing
Windows product functionality.

## Harness map

Every harness/probe/script/gate, its layer, what a pass proves, and what it
cannot prove. "L" = ladder layer above.

| Harness | L | A pass proves | Cannot prove |
|---|---|---|---|
| `cargo test --lib` (default + `cockpit-privileged` configs) | 0 | pure logic given its inputs; contract fixtures (`contracts/petal-contracts.json`) | that the real event/lifecycle path calls it (CLAUDE.md: #376/#466/#497 all shipped green and failed live) |
| `petal-scorecard-gate` | 0 | a scorecard JSON's p95 is under a ceiling (**default 150 ms — stale; real target <100 ms**, CLAUDE.md Known deviations) | anything about how the scorecard was produced |
| `apps/desktop` `npm test` (incl. rendered-pixel toast check, #422) | 0 | frontend units; one real rendered-pixel truncation check via headless CDP | native/media behavior |
| `backend npm test` / `web-harness npm test` | 0 | handler/source correctness; web/native contract pinning | **the live deployment** (deploys are separate manual `vercel --prod` steps) |
| `scripts/ci-local.sh` | 0 | the aggregate of the above, both Rust feature configs, per-config labelled test totals, static loopback inventory | runtime behavior; `--no-default-features`, release builds, the `harness/` crate (its own header says so) |
| `remote-control-local-loopback.mjs --check-only` | 0 | contract + harness inventory still exist; Swift sentinel typechecks | that any replay works |
| `backend npm run test:local` | 3 | token/JWT/room plumbing against a local SFU | media |
| `share_lifecycle_probe` (+ `--late-joiner`) | 3 | pub/sub event ordering a real SFU delivers (e.g. `TrackSubscribed(new)` before `TrackUnpublished(old)`); has in-run positive controls | latency; real capture |
| `startup_layer_probe` | 3 | simulcast startup layer selection/ramp mechanics; `--pin-lowest` positive control | latency, real-capture fps (synthetic source) |
| `audio_probe` / `camera_cadence_probe` | 3 | transport round-trip for synthetic tone / synthetic NV12; cross-arch pipeline deltas (identical input bytes) | device capture; receiver layer-selection policy (probe README says so) |
| `petal-harness` live runner (`--features live-io`) | 3 | synthetic BGRA bots publish, subscriber sees frames, scorecard emitted | impairment (records the `--impairment` label, **applies no shaping**); latency meaning |
| `capture_probe` (`--geometry`) | 1 | real ScreenCaptureKit capture + numeric geometry/content assertions; manufactures #531's failure as control | transport, latency |
| `frontmost_probe` | 1 | foreground-ownership timeline in ms | anything media |
| `bare_window_probe` / `mint_token` | 1/– | AppKit env isolation / token utility | product behavior |
| `publish_probe` + `subscribe_probe` (paired) | 4 | real capture through local SFU with embedded-timestamp latency; **unpaired `publish_probe` is off-ladder** (no subscriber → no jitter buffer) | prod server, absolute latency |
| `compositor_probe` | 4 | decoded frames into the real display-layer type | the real compositor window lifecycle (plain AppKit window; no retire/reveal — the class of gap that hid #416) |
| `rc-live-suite.sh` (30-case matrix) | 2+4 | web controller → local SFU → native replay into real TextEdit; host-side effects | prod path; its "Live status" lines in TESTING.md go stale silently (the 2026-07-05 "19/0/9" was recorded hours before the join path broke, unnoticed for two days) |
| `rc-live-suite.sh --press-to-photon` | 4 | web-input → estimated-browser-display p95 (software `expectedDisplayTime`, not a photodiode) | physical photon time; prod latency |
| `.github/workflows/nightly-loopback.yml` | 2+4 | the `--live` loopback nightly, **only if** a self-hosted TCC-granted Mac runner is registered | anything, when no runner exists (unverified whether one currently does) |
| Autotest socket + `autotest-run.mjs` scenarios | – | driver, not an oracle: state/accessibility preflights | correctness of anything it drives |
| Test Cockpit (`cockpit.mjs`, `test_cockpit/`) | 5 | scenarios against **prod** SFU on one machine; native+headless-web, `SHARE-N2N` via second local binary (`target-peer/`); verdicts carry `evidenceBasis` (`HostEffect`/`ContentVerified` vs. weaker `WireShape`/`LivenessProxy`/`Scaffold`) | two-host effects; `RC-01..06` and `RES-04` are ⛔ Gap in `internal/docs/COCKPIT_TEST_MAP.md`; `RC-P1080` is a narrow smoke, not the 30-case matrix |
| Quick prod cross-client check (fixed test room, TESTING.md) | 5 | manual native receiver + browser test-pattern sharer in a prod room | real user content; the browser share freezes when its tab is backgrounded (watchdog retires the window — a rig artifact, not a product bug) |
| Manual cross-client test (TESTING.md) | 5 | two independently-permissioned real clients, real devices | repeatability; nothing records it |
| `release-smoke.sh` | 5-gate | signed-artifact static assertions (team, rpath, baked backend URL) + human clean-TCC checklist + petal.log marker assertions with byte-offset run boundaries and the `MovingFrameLiveness` share-liveness latch (the #622 gates, merged) | that a human actually exercised each checklist item; the gates fail closed on missing evidence but cannot judge what they weren't asked to grep |
| `verify-backend-live.sh` / `verify-web-harness-live.sh` | 5 | the **deployed** endpoints serve current behavior (catches "forgot to redeploy") | media |
| Rosetta x86_64 tier (TESTING.md) | 3/4 | the x86_64 *code path* on one host; reproduced defect = real | Intel silicon/GPU/hardware encoders; representative timings |
| `cross-machine-rc-suite.sh` | 6/7 | (by design) the 32-case suite with a genuinely separate SSH-reachable Mac, all four arch pairings, rejects Rosetta | **anything yet — never live-validated** (#79 is closed, but no run against a real second Mac was ever recorded); requires the second Mac that will not exist |
| `web-harness/` | control | a browser peer with zero native code; the standard second peer for every tier | anything about our native pipeline — a defect here is not ours |

### Native clipboard validation boundary

The native clipboard extension has two distinct questions. Layer-0 tests can
prove byte limits, UTF-8/NUL/file rejection, stream-header correlation,
sequence guards, lifecycle clearing, and the absence of clipboard text from
logs/ledgers. The existing macOS AX tests and `rc-live-suite.sh` cases 10/11
prove host-side native Copy/Paste actuation, not cross-machine clipboard
transfer.

A separate OS session or a second machine is required to prove that B's
clipboard is copied to A, that A's changed sequence rejects a delayed Copy
response, and that A's text updates B before native Paste. Two Petal processes
on one desktop share the same OS clipboard: that setup cannot distinguish A
from B and must never be reported as transfer evidence. Browser peers are
likewise not evidence for the native stream; they intentionally ignore the
native-only Copy request/topic.

Petal's keyboard clipboard semantics are fixed at the boundary (Copy B→A,
Paste A→B). A keyboard Copy→Paste is not a supported native/lossless B-local
workflow. Validate B-local behavior, when needed, by using the target
application's reachable in-window context menu, toolbar, or dropdown for both
operations; do not mix that UI Copy with Petal keyboard Paste.

### Native Linux launch boundary

Prepare the local executable with Linux tooling only, from the repository root:

```sh
npm --prefix apps/desktop ci
npm --prefix apps/desktop run build
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --bin desktop
```

Launch it directly in this computer's existing Wayland desktop:

```sh
XDG_RUNTIME_DIR=/run/user/1000 \
DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus \
WAYLAND_DISPLAY=wayland-0 GDK_BACKEND=wayland \
apps/desktop/src-tauri/target/debug/desktop
```

No `tauri dev`, browser, macOS signing/permissions, Windows SDK, or installer
step is involved. This requires the system GTK3/WebKitGTK runtime and normal
Linux build dependencies; general distribution packaging is not this gate.

The Linux main window uses an ordinary opaque, decorated GTK toplevel that is
visible before WebKit loads, rather than the macOS/Windows hidden-window
first-paint reveal lifecycle.

Linux startup selects WebKit's shared-memory renderer transport before GTK
initialization. In the installed
[WebKitGTK 2.52.6 implementation](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebKit/UIProcess/gtk/AcceleratedBackingStore.cpp),
`WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` retains shared-memory transport and its
backing store without enabling hardware-buffer imports. Disabling the DMA-BUF
renderer entirely instead leaves an empty transport and a null backing store;
the actual Petal frontend reproduced a native compositor-callback crash in
that configuration. This process-local UI policy needs no launch workaround
variables and does not change macOS/Windows or the independent native
GTK/Cairo incoming-video design.

`/usr/bin/python3 apps/desktop/scripts/check-linux-launch.py
apps/desktop/src-tauri/target/debug/desktop` runs the same production executable
with a fresh Linux profile. AT-SPI drives native actions and requires the actual
visible room-entry window and entry controls. This WebKit's accessible editor
does not advertise `EditableText`, so the driver enables the toolkit's existing
loopback inspector for its owned child only. It verifies listener socket
ownership before dispatching the real name input event and checks native
accessible text plus the real Done button; it never seeds onboarding stores,
changes validation, sends desktop-wide keys, or opens an inspector browser.
Its owned execution trace rejects browser/openers and foreign-platform setup
programs. The driver requires the system Soup 3 GI bindings alongside AT-SPI.
Failed runs retain their owned scratch directory and logs for diagnosis.
Before launch, a private session-bus monitor watches only relevant URI-open and
application-action message headers. Connection credentials attribute requests
to owned process IDs; unrelated requests are ignored and message bodies are
never accessed. Together with the execution trace, this checks browser
activation even when no new browser process would be created. AT-SPI requires
a showing native window with positive geometry and real entry controls, not
a screenshot or an unfulfilled manual approval.

After the launch observations, the check terminates only its temporary traced
process group. `strace --kill-on-exit` prevents traced children from being
detached and left running; PASS is printed only after cleanup completes.
This does not assert or change the application's separate normal Quit flow.

The focused check also exercises two real onboarding fixtures: the fresh blank
name cannot enable Done, while an escaped name containing quotes, markup-like
text and Unicode must pass the actual input handler, enable Done and reach the
native room-entry screen. Neither fixture writes application stores directly.

The executable living contract uses the same native launch operations, with
each exact Given/When/Then step bound separately:

```sh
/usr/bin/python3 apps/desktop/scripts/check-linux-launch.py \
  apps/desktop/src-tauri/target/debug/desktop \
  --feature apps/desktop/features/linux-room-membership.feature
```

Both modes require the actual desktop session variables documented above.
Unknown or missing living steps fail rather than silently skipping an assertion.

This launch gate does not establish hosted membership, audio, camera, incoming
share rendering, or remote-control support. Task 1.1's native launch scenario
passed the actual-session focused and bound living checks, including real
blank/escaped identity fixtures and every acceptance clause.

The production-window mutation was confined to an independent plain copy of
frozen candidate `c601226c886b5511638e3b3cc71d77ed4a4292c5`: changing only the
Linux main-window URL from `main.html` to `settings.html` made both checks exit
1 at the real room-entry assertion. Both native processes remained alive until
the deadline and logged frontend first paint; no mapped Settings AX surface or
inspector URL observation is claimed. All 73,294 original non-VCS files matched
their SHA256/kind/mode manifest afterward, and all 464 traced process IDs had
no living tracees. Only the copied config was restored; the unchanged original
focused and living checks independently passed after mutation (7.77 s and
12.72 s).

#### Separate Linux validation constraints

The existing rendered-UI Playwright tests use packaged browser runtimes, not
Petal's production GTK/WebKit path. On Arch, the Ubuntu-targeted WebKit test
runtime required ICU 74, libxml2 and libflite compatibility libraries. The
Chromium and WebKit chat-wheel tests passed with SHA256-verified Ubuntu packages
in an owned, isolated browser cache and library search path. This required no
host installation, ABI-alias symlinks or validation bypass. These libraries
apply only to browser-test processes; the native launch gate uses the system
GTK/WebKit runtime and the actual Wayland session.

Whole-tree `cargo fmt --check` remains a recorded baseline failure, not a PASS:
planning base `6ec97e6f` and the Task 1.1 candidate each produced 634 diagnostic
hunks across 103 files. Ordered hunk bodies matched byte-for-byte after
normalizing only source-root paths and diagnostic header line numbers. The new
Linux startup/session modules passed `rustfmt --edition 2021 --check`. Task-owned
formatting differences were corrected without suppressing formatter rules or
restyling unrelated baseline files.

### Native Linux receive-only capability boundary

Use the same actual Wayland/session environment as the launch check, with
GNOME awake and unlocked. On this WebKitGTK runtime, idle screen blanking
stalled animation-frame/view-transition callbacks: the real native route's
URL changed while the prior room-entry DOM remained mounted. Waking the
already-unlocked session restored actual meeting controls; no routing or
renderer workaround was needed. `ScreenSaver.GetActive` alone does not
distinguish blanking from an authentication lock. Do not bypass authentication
or change the user's lock policy to run a gate.

Keep each owned native gate awake with the standard, process-lifetime idle
inhibitor below. It does not permanently change desktop settings:

```sh
gnome-session-inhibit --inhibit=idle --app-id=com.petal.app \
  --reason='Native capability verification' \
  /usr/bin/python3 apps/desktop/scripts/check-linux-capabilities.py \
  apps/desktop/src-tauri/target/debug/desktop
gnome-session-inhibit --inhibit=idle --app-id=com.petal.app \
  --reason='Native capability verification' \
  /usr/bin/python3 apps/desktop/scripts/check-linux-capabilities.py \
  apps/desktop/src-tauri/target/debug/desktop \
  --feature apps/desktop/features/linux-meeting-capabilities.feature
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --locked \
  linux_unsupported_commands_are_rejected_without_revoking_receive_or_audio
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --locked \
  linux_replay_refuses_real_pointer_keyboard_and_text_packets
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --locked \
  admission_table_covers_audio_camera_window_and_unknown_video
```

The native check uses real IPC, nonempty production-shaped window/control
requests, the existing room-controls route, and actual device enumeration and
preference commands. It requires explicit Linux errors from every covered
unsupported entrypoint, disabled visible controls with limitation explanations,
and unchanged disconnected-room/camera state. The exact six-row living table
and each step are bound; an added, omitted or changed clause fails the driver.
Network-only replay is independently checked against the actual Linux replay
backend with decoded pointer, keyboard, wheel and text packets.

Positive evidence here is **eligibility and local control availability**:
room entry and microphone controls are not Linux-disabled, device preferences
are accepted, and native subscription admission retains canonical share/audio
tracks. The room-controls snapshot is explicitly taken in the existing
failed-join state, not a successful meeting. This does not prove hosted joining,
working microphone/speaker audio, rendered incoming shares, or acoustic AEC;
those require the later ordered tasks and real native peers. Do not replace
them with a browser bridge or a successful no-op.

### Native Linux hosted membership

With the same awake, unlocked Wayland environment as the native checks above:

```sh
gnome-session-inhibit --inhibit=idle --app-id=com.petal.app \
  --reason='Native hosted-entry diagnostic' \
  /usr/bin/python3 apps/desktop/scripts/check-linux-hosted-entry.py \
  apps/desktop/src-tauri/target/debug/desktop \
  --room-code abc-defg-hjk --inspect-join-command
```

This diagnostic types the public valid-format fixture through the production
room-entry input and requires an enabled Join action. It never submits Join.
The optional command inspection omits the required room argument and reports
the rejection category without requesting a token or asserting membership.
Before Linux membership was implemented, command inspection reported
`unregistered` for `join_room_command`. The actual native Join action then
reproduced that missing-command failure against the Windows peer. A diagnostic
PASS alone is not scenario RED or proof of hosted membership.

For the real hosted check, start the released Windows native client in an
authorized test room. Supply its code and displayed participant name explicitly:

```sh
gnome-session-inhibit --inhibit=idle --app-id=com.petal.app \
  --reason='Native hosted membership verification' \
  /usr/bin/python3 apps/desktop/scripts/check-linux-hosted-entry.py \
  apps/desktop/src-tauri/target/debug/desktop \
  --room-code "$PETAL_TEST_ROOM_CODE" --join \
  --backend-url https://app.petal.live \
  --expected-peer-name "$PETAL_TEST_PEER_NAME" --hold-seconds 180
```

This submits the production Join action in the actual Linux window, requires
the joined room and local/Windows identities in native IPC and visible
participant tiles, then submits Leave and requires an empty membership and
the room-entry route. The hold allows separate observation of the Linux
participant in the Windows client; that reciprocal check is not automated
by the Linux driver. Keep both desktop sessions awake and unlocked.

On 2026-10-27, the earlier upstream-hosted runs passed Linux membership and
Leave, and the released Windows client visibly showed both participants.
Separate owned-stack checks later that day established trusted TLS from
both operating systems, room creation through released Windows Petal 0.9.33,
and its presence through the real LiveKit SDK. Linux then joined that owned
room, rendered both participants, and appeared in the Windows native gallery.
The owned stack uses unmodified backend handlers and official non-development
LiveKit 1.13.11, not a replacement token service. These checks do not prove
audio, incoming screen rendering, or acoustic AEC.

No client-side LiveKit server API secret is supplied. Windows replaces macOS
for this milestone under Tommy's 2026-10-27 scope correction. The public
parser fixture is not an accessible hosted-room fixture; never substitute a
browser, local SFU, mock peer, or diagnostic-only PASS for real membership.

The focused and living checks both reject an isolated production mutation
that removes Linux's `presence::start_for_room` bridge: the real hosted Join
succeeds, but the native roster and rendered participant assertions fail.
Restoring that bridge restores both checks.

Linux IPC and rendered tiles are not independent proof of continued
server-side membership. In an isolated forced-close probe, the vendored SDK's
manual `Room::close()` returned success while its cached connection state and
participant snapshot still looked connected. That probe did not earn a
negative-control pass. Observe the other native client during Join and after
Leave; do not infer reciprocal membership from Linux's cached state.

### Native Linux hosted join recovery

Build the frontend and native binary with `PETAL_BACKEND_URL` absent when
checking an unconfigured build. An empty runtime value is a separate, explicit
local-development opt-out; it is not the missing-configuration fixture.

With the actual Wayland/session environment above and a released Windows peer
already in an authorized hosted test room:

```sh
gnome-session-inhibit --inhibit=idle --app-id=com.petal.app \
  --reason='Native hosted join recovery' \
  /usr/bin/python3 apps/desktop/scripts/check-linux-join-recovery.py \
  apps/desktop/src-tauri/target/debug/desktop \
  --failure-kind missing --backend-url "$PETAL_TEST_BACKEND_URL" \
  --room-code "$PETAL_TEST_ROOM_CODE" \
  --expected-peer-name "$PETAL_TEST_PEER_NAME" \
  --feature apps/desktop/features/linux-room-membership.feature
```

The driver attempts Join without a backend override or server API secret.
It requires a visible configuration action naming `PETAL_BACKEND_URL`, no
native room ownership or remote window, and no owned IPv4/IPv6 `connect` or
`sendto` destination during the attempt. It restarts the same binary with
only the backend URL added, preserving the same isolated profile and actual
onboarding identity, then requires the hosted Windows participant and Leave.
This checks configuration recovery, not working audio or incoming rendering.

On 2026-10-27, an isolated copy with only the missing-setting message branch
removed failed both its focused unit assertion and the actual native living
check. Restoring the branch restored the five helper tests; the unchanged
candidate then passed strict living execution in 13.49 seconds. The
same-profile restart and real Windows membership checks remained enabled.
Removing the separate HTTP 401/403 recovery branch also failed its consumer
test. Those historical mutations alone did not establish native refusal.

The owned-stack check subsequently exposed the actual Rust status text,
`backend token endpoint returned HTTP 403 Forbidden`. The helper's original
exact-status matcher fell through to connection advice. The regression now
uses real `401 Unauthorized` and `403 Forbidden` shapes: it failed before the
fix and passes afterward. A native run against the actual denied identity
also failed before the fix and passes with actionable access advice after it.

The same driver strictly binds the complete invalid/refused living outline
with `--feature apps/desktop/features/linux-room-membership.feature`:

- Invalid row: `--failure-kind invalid --attempted-code petal://join/000000`.
  Bare `000000` is a valid meeting name, not a code-join attempt.
- Refused row: `--failure-kind refused --attempted-code "$PETAL_REFUSED_ROOM_CODE"
  --refusal-fixture-dir "$PETAL_REFUSAL_FIXTURE_DIR"`. The fixture must be an
  existing, isolated room on the owned service, not an unknown code.

The parent owns the private `0700` fixture directory. After actual onboarding,
the driver publishes `0600` `ready.json` containing the real participant ID.
The parent records only that identity's removal through the real admin handler,
confirms `/api/token` returns `403`, and atomically publishes `0600` `resume.json`
with the matching ID. The driver waits at most 120 seconds before Join; stale
files, symlinks, mismatched identities and incorrect ownership/modes fail closed.
Admin credentials never enter either client or the driver.

On 2026-10-27 both strict recovery rows passed against the owned service:
actionable error, no native membership or remote window, same-profile
corrected-code admission with the released Windows peer, and clean Leave.
A Windows native screenshot independently showed both participants during
the corrected join. The strict missing-configuration scenario also passed:
zero owned IPv4/IPv6 join destinations before a URL-only restart preserving
the same profile and identity, followed by Windows-room membership and Leave.
Both clients used explicit backend configuration without server API secrets.
Those Task 1.4 checks exercised trusted TLS and native membership, not audio
or incoming-share acceptance. Task 2.1's later incoming-window evidence is
recorded below. Earlier upstream investigation made no mutation and is not
being relabeled as owned-stack proof.

The final Task 1.4 desktop integration gate passed 984 tests plus 96 harness
tests, with one Windows-only integration skip. Panel-close validation passed.
Svelte checked 613 files with zero errors and the existing `RoomRow.svelte`
accessibility warning. The frontend and native executable rebuilt successfully.
Rust library and Clippy results above are retained earlier checks; they were
not rerun for the later one-line status-phrase classification fix.

### Native incoming-window development checks

The native receiver consumes the original room connection's video events and
renders an incoming window through GTK3/Cairo, independently of the meeting
WebView. No browser media path or second room connection is used. The complete
Task 2.1 outline passed locally on 2026-10-27 with the released Windows peer.

On 2026-10-27, the released Windows artifact shared a real, non-sensitive
Notepad document whose counter changed in the editor once per second. Its
virtual-machine hardware encoder initially reported
`WEBRTC_VIDEO_CODEC_UNINITIALIZED`; the existing release-supported
`PETAL_FORCE_SOFTWARE_ENCODE=1` override selected OpenH264 without changing
the artifact or the H264 codec. Actual Linux decoding and compositor pixels
then showed the editor and three increasing counter values in exactly one
native GTK window. Normal Leave cleared membership and removed that window.

The final post-review executable repeated that pixel/Leave smoke successfully.
Rust library checks passed 1,332 tests (two ignored); the new compositor's two
focused tests and formatting check passed. Clippy completed with existing
warnings. Frontend checks passed 984 and 96 tests (one Windows-only skip);
Svelte reported zero errors and one existing accessibility warning.

`scripts/check-linux-receive.py` observes received pixels with Mutter's
window-only ScreenCast/PipeWire path. Full geometry acceptance additionally
requires explicit human authorization and `--observe-desktop`: it briefly
captures the monitor, including other visible windows, and keeps the evidence
in a private directory. It never acquires a privileged screenshot bus name.
Unique owned-client pixel patches locate both native windows in actual desktop
pixels; GTK's fabricated Wayland screen origin is never used as position proof.
The meeting is arranged separately before the measurement baseline.

| Observation | Remote visible origin | Remote native allocation | Meeting visible origin | Meeting native allocation |
|---|---|---|---|---|
| Baseline | (1446, 260) | 1000 × 641 | (914, 216) | 504 × 818 |
| Normal desktop Move | (1486, 300) | 1000 × 641 | (914, 216) | 504 × 818 |
| Normal desktop Resize | (1486, 300) | 1030 × 671 | (914, 216) | 504 × 818 |

Changing editor-counter pixels remained observable after resizing. Normal
Leave cleared membership and removed the received window. An unrelated
desktop authorization modal had blocked earlier keyboard window-manager
operations; cancelling its request without approving privileges restored
actual resize behavior in the owned GTK fixture.

A disposable, byte-verified copy rejected both erased converted pixels and a
suppressed original room event feed at the corresponding pixel/window
assertions. The candidate source and executable hashes were unchanged by those
mutations. The full desktop-observed outline also rejected a native production
mutation that made the received GTK window non-resizable, specifically at the
unchanged native-allocation assertion; the current executable then passed the
complete living outline again, including remote-window removal before process
teardown. Audio, acoustic AEC and later lifecycle scenarios remain separate
gates; Task 2.1 is not a completed receive-and-conversation milestone.

## Rules — each traceable to a real incident

1. **A harness may not decide a question above its layer.** The synthetic
   probes (L3) priced the jitter buffer at "≤10 ms, do not start here"; real
   capture measured **131.6 ms**. That mispricing parked #214 — the dominant
   latency lever — at P3 for three weeks (`internal/docs/COURSE_CORRECTION.md` §4c).
2. **A probe's numbers decide nothing until that probe has reproduced one
   known real-path effect through the same pipe.** Verbatim from the amended
   fast-probes rule: a fast answer from a path the user is not on is not a
   cheap answer, it is a wrong one with a short feedback loop.
3. **A control never observed to fail has not been shown to work.**
   `startup_layer_probe` once divided decoded frames by an absolute timestamp,
   so a healthy 30 fps stream read as "FAIL — #299 reproduces" **while its
   positive control could never fail** (fixed on `main`, `9713f69f`). Inverse
   case: `screencapture -l` silently returns a fully transparent image for our
   nonactivating NSPanels — a known-good component (the share border) reading
   0 px is what exposed the blind instrument before #196 was mis-confirmed.
   Run the control **before** the first real reading, on the same class of
   thing, through the same instrument (`internal/docs/COURSE_CORRECTION.md` §4b).
4. **Zero evidence renders INSUFFICIENT DATA, never a pass.** The cockpit
   reported "no regression" on runs that measured nothing — p95=0 was
   indistinguishable from perfection (#621; fixed on `main`, `e1181d5f`).
   `ci-local.sh` blocks when the test total can't be parsed rather than
   trusting exit codes; `release-smoke.sh` (branch) dies with `INSUFFICIENT
   DATA` when nothing was logged since the run boundary. Gates also **fail
   closed**: the old `otool | grep` rpath check passed when `otool` itself
   errored (empty pipe → no match → "no CLT rpath").
5. **A summary statistic without per-run values is not reportable.** Seven
   identical runs spanned 15.9–92.7 ms (5.8×) behind a single quoted median
   (§4c.3); #288's "80 ms text penalty" was `+312.6 / +66.7 / −3.8` across
   three runs — the effect lived entirely in runs already flagged as
   contaminated. Per-run first; a pooled number that disagrees with the
   per-run table is an artifact.
6. **Liveness is not throughput.** A locked screen leaves every health signal
   green while capture delivers nothing ("stream alive, source not drawing" —
   TESTING.md); the pre-fix release-smoke "frame pump heartbeat" marker was
   satisfiable by one static frame re-pushed forever. Pair every
   alive/connected signal with a produced-output counter, and treat *alive
   with zero output* as a distinct, alarmed state.
7. **A result not posted did not happen; absence of a report means "unknown".**
   Three decisive gate-failing runs sat on disk nine hours while the issue was
   reasoned about as "probably stale" (§4).

## Gaps

### A. Layers with no harness at all

- **L6–L8 (anything two-host).** `cross-machine-rc-suite.sh` exists but has
  never run against a real second Mac (#79) and never will by policy. Real
  clocks, real network distance, and real user content are permanently
  unmeasured; every latency claim is a shared-clock, shared-CPU number.
- **Glass-to-glass at L5+.** Nothing has ever measured latency against the
  real cloud SFU. The only figure (187.2 ms avg, #179) is L4, stale (#613
  lists five pipeline-touching commits since), and 2× over the <100 ms target.
- **The last pipeline stage.** `compositor::push_frame` takes no `frame_id`
  (its signature in `compositor.rs` is `app, owner_identity, window_id,
  cv_pixel_buffer, source_width, source_height`), so no capture-side
  timestamp can be correlated with
  a compositor present. **Every latency figure ends before the screen and is
  therefore a lower bound.**
- **`RES-04`** (display sleep) has no runnable scenario; **RC-01..06** are ⛔
  Gap in the cockpit journey table.

### B. Harnesses whose layer is lower than people assume when quoting them

This is where the damage came from — the number was real, the question wasn't.

- `publish_probe`/`startup_layer_probe` (L3/off-ladder) quoted for **latency
  and ladder ranking** — two full measurement rounds inverted on real capture.
- The cockpit-status block in TESTING.md (a doc) quoted as a **run verdict** —
  "passed 6/6" from an artifact with no conclusion event.
- `petal-scorecard-gate`'s 150 ms default quoted as **the** target — the real
  target is <100 ms; a green gate at 150 does not mean the product is right.
- `rc-live-suite.sh` "Live status" lines quoted as **current** — they record
  one hand-run moment and rot silently (the 19/0/9 incident).
- `--press-to-photon` quoted as photon time — it is a software
  `expectedDisplayTime` estimate.

### C. Harnesses whose coverage is narrower than their name implies

Distinct failure from B: the harness is at the right layer but covers less of
it than the name says.

- **`ci-local.sh` ran 987 of 1118 tests** for weeks: the `cockpit-privileged`
  module was feature-gated out, so ~131 tests — and the module's *compilation*
  — were skipped while the gate stayed green; code that did not compile passed
  "local CI" (#624; fixed on `main`, both configs now built and tested with
  per-config totals). The exemplar of this category.
- `petal-harness --impairment` records the label, applies no network shaping.
- `RC-P1080` covers one smoke path, not the 30-case matrix its "remote
  control" tag suggests.
- `remote-control-local-loopback.mjs --check-only` (what CI runs) proves the
  contract and inventory *exist*, not that replay works.
- `release-smoke.sh` covers a marker list, not the behaviors between markers;
  markers are only as strong as what emits them (see Rule 6).
- Rosetta tier covers the x86_64 code path, not Intel hardware.

## Currently unvalidated (as of 2026-09-05)

- Everything in Gap A.
- **The #622 release-gate fixes, as gates.** Commit `51225e17` (now on
  `main`) makes `release-smoke.sh` assert on evidence produced: a
  `MovingFrameLiveness` latch in `session/share.rs` requiring **5 pushes
  carrying affirmative changed-content evidence** (dirty rects or a changed
  snapshot hash — a frozen share cannot satisfy it), byte-offset run
  boundaries so `--assert-log` only greps output appended after the checklist
  invocation, `otool` failing closed, and `update-testing-status.mjs` refusing
  to publish a status block without completeness evidence. Per Rule 3, a gate
  is not shown to work until it has been observed to fail on a manufactured
  violation; that negative test is recorded for the frozen-share case (the
  liveness marker only fires after several changed frames) and not for the
  others. #622 itself is still open.
- The nightly loopback's runner status (whether a self-hosted Mac runner is
  currently registered) — unverified.
- Whether `--text-primary`'s placeholder token and other Known-deviations
  items affect any visual check — see CLAUDE.md.

## Update 2026-08-14 -- what the RC suite now proves, and the residue

The suite (30 cases then; 32 since the consent cases landed 2026-08-22) was
live again at **27 pass / 2 fail / 1 skip** — the last recorded run; both
failures (#811, #820) have since closed and no post-fix run has been recorded
(details + defect table: TESTING.md's Live status log). What that number is
and is not evidence of:

**It IS evidence of** (ladder rungs 2+4): grant negotiation and session
restore across release/re-request cycles, real host-side injection into
TextEdit (typing, Cmd+A/C/V, drag-select via AXSelectedText, vertical scroll
via AX visible range, modifiers, keycodes, release/TTL/revoke semantics,
non-focus-stealing), share stability across the whole run (zero ROI ack
timeouts, zero wedge restarts, zero teardowns), and target-observation latency
p95 within budget.

**It is NOT evidence of** (each was nearly miscited during the session that
produced the number):

- the #804 ROI **abandonment fallback** -- healthy SCK acks every ROI on
  attempt 1, so that path runs only in unit tests;
- the #806 **static-source hold** -- the suite's target never goes silent for
  more than ~9s and the watchdog arms at 45s; the separate hidden-app recipe
  in TESTING.md is the validation for that path;
- anything timing-dependent (the #804 fps/ROI lock race was microseconds
  wide);
- horizontal scroll (#811, the one route that genuinely does not exist) and
  case 30's test-only republish hook (#808 residue).

**Standing lesson, second occurrence:** cases 5 and 22 failed for months on
predicates describing a wire shape (`action: 'down'`) the harness stopped
publishing -- the same class as #455's case-4/case-7 findings. A publish-metric
predicate is an assertion about the HARNESS, not the product; when one times
out, diff it against what `harnessApi.ts` actually publishes before reading it
as a product failure.

## Refs

`docs/TESTING.md` (how to run everything) · `internal/docs/COURSE_CORRECTION.md` (§4b
controls, §4c experiment choice, amended fast-probes rule) ·
`internal/docs/COCKPIT_RUNBOOK.md` · `internal/docs/COCKPIT_TEST_MAP.md` · issues #613 #618 #619
#620 #621 #622 #623 #624 #299 #79.
