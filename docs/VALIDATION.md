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

#### Linux ↔ Windows clipboard acceptance, November 3, 2026

The actual Linux ELF joined the owned SFU room on a private headless Mutter
display, runtime and session bus. The corrected local Windows candidate shared
an owned WinForms editor in the dedicated test VM. Windows approved the real
control-consent dialog; Copy and Paste were explicitly invoked separately with
the current grant. The VM's host clipboard bridge and drag-and-drop stayed
disabled, and no human clipboard was read, written or connected.

Both directions preserved Unicode, multiline text and existing trailing CRLF.
Distinct exactly-1,048,576-byte UTF-8 payloads passed Copy and Paste: received
clipboard hashes matched, and Paste changed the actual Windows editor to the
expected text. Missing approval, 1 MiB + 1 byte, NUL, a genuinely retired grant,
file-list MIME and image-only MIME all left both clipboards and the Windows
editor unchanged. Retired-grant Copy and Paste could publish successfully but
had no host effect; publication success alone was not counted as acceptance.

The earlier private-renderer blocker was resolved without production changes.
The installed WebKitGTK 2.54.2 core identified
`FrameRenderer::graphicsLayerFactory()` during `document.startViewTransition`.
The private launcher had disabled the DMA-BUF renderer entirely; retaining
`WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` instead allowed real onboarding, room
navigation and transport. Private accessibility peers also need consistent PID
visibility; a bus in a separate PID namespace could not identify the app.
Clipboard source ownership was verified by reading the private compositor's
accepted bytes before each transfer, not by trusting `set_content()` alone.

This establishes the controller R04 outline locally, not Task 3.1 delivery,
Linux clipboard-host support, or delayed-response acceptance across every
retirement. Owned apps, virtual input, private compositors and scratch files
were removed; the empty owned SFU room and cleared synthetic guest clipboard
were observed. The resumed VM remains running with its configuration unchanged.

#### Linux → Windows stale-control retirement, November 3, 2026

The actual Linux GTK/WebKit app and owned Windows native editor used the
self-hosted SFU. A packet gate inside the Linux app's private user/network
namespace held genuine encrypted UDP, without rewriting payloads or sender
identity. It did not change the host firewall or shared networking.

All six endings passed delayed input and delayed approval checks: meeting-wide
Windows revocation, source Stop, Windows departure, Linux leave, exact owned
participant removal from the SFU, and share-instance replacement. Every
accepted row compared automatic-accept counters before and after; the delta
was zero. Capacity overflow, watchdog expiry and failed harness attempts were
excluded, not counted as successful replay witnesses.

For revocation, Stop and replacement, Windows logs identify the original
authenticated Linux sender and reject the old input without changing the
editor. Departure/disconnection can discard traffic below native admission.
A once-delivered genuine approval after local cancellation also reached Linux
native admission and was rejected. Ordered active/stopped statuses are not
reordered application callbacks: no input authority survives the drained
retirement batch.

The initial delayed global-revocation witness found that `revoke_all` notified
only the Windows UI. Linux could briefly send using the late approval until
Windows refused its first input. The fix sends the existing ordered `stopped`
status to the controller while preserving admission closure and native release
ordering. The final temporary PE SHA256 is
`3A9852B3363BCC2AE7A26260909D6B51CE53C3291585419A3D940D93517EBB85`.
It passed all twelve delayed-input/approval rows; 137 shared remote-control
tests pass. An independent source reviewer found no issue in the narrow fix.

Together with the earlier six paired held-key/button releases (14–156 ms,
under the 600 ms limit and before the 1,200 ms backstop), this establishes R03
locally. It does not establish R01/R02, request correlation across a fresh
pending request, Linux host replay, production delivery, or a released Windows
upgrade. The installed release, VM configuration and human clipboard remain
unchanged. The session's `petal-r03-wire-acceptance.json` records the accepted
rows, native diagnostics, excluded attempts and artifact identity.

#### Native request outcomes and window confinement, November 3, 2026

Actual GTK requests and Windows consent exercised R02's existing-request rows over the owned SFU.
Delayed Deny reported declined. Real GTK cancellation and the actual
eight-second request timeout rejected once-delivered original-owner approval;
the independent Windows editor remained unchanged. Before release, timeout
visibly reported “No answer from the sharer.” A fresh explicit GTK request
was needed afterward. Holds were 1.99 s, 1.37 s and 8.55 s respectively, with
zero automatic-accept counter deltas. The expiry-only private namespace gate
had a ten-second watchdog and held 308 datagrams; it removed its table on exit.
Those rows left the intervening-request correlation boundary open; the November 4 native proof below closes it.

For R01, a separate owned always-on-top editor covered the authorized window
and held foreground. Native-controller key/text/wheel traffic stayed in the
authorized editor; the unshared editor retained its sentinel and received no
key, button or wheel. Covered clicks were refused. Cursor restoration produced
hover-only `WM_MOUSEMOVE`, not button delivery. However, one background `KeyZ`
down/up inserted `zz`, whereas one foreground `KeyQ` pair inserted `q`.
That run left the duplicate-background-key defect open; the November 4 repair
below closes it under separate explicit authorization. Its dated receipt is
`petal-r01-r02-acceptance.json`; the installed release was unchanged.
The later receipt identifies the corrected temporary artifacts.

#### Approved keyboard and exact-request prerequisites, November 4, 2026

The actual GTK received-share window now sends one background Z pair that
produces exactly one Windows Down, one `WM_CHAR` and one Up. The owned editor
inserted one `z`; foreground HWND stayed unchanged. Actual GTK foreground
typing also inserted one `z`. The real Windows message-loop smoke passed exact
`bzAé🦀`, Unicode, navigation/modifier behavior, an unrelated sentinel and focus
preservation. Prior native window/screen pointer, wheel and confinement evidence
is retained; this closes R01's last observed defect.

For R02's new same-share boundary, real Windows Allow A was held on the incoming
encrypted UDP path. GTK cancelled A and requested B before release. The 1.612 s
hold released all 21 datagrams explicitly, with no automatic-accept delta.
The receiver authenticated A's awaiting/active/stopped responses and rejected
them as stale. B stayed “Waiting for approval…”; an actual GTK key changed no
Windows text. B's own real Allow then enabled one background character without
focus change. Current Windows revoke stopped B, and another key changed nothing.
No payload rewriting, fake status callback or sender impersonation was used.
Prior genuine decline/cancel/expiry rows remain evidence for those outcomes.

Exact identity also binds local cancellation, consent answers and timeout work.
Independent review found stale cancellation A could otherwise borrow B's ID;
that defect was fixed, behaviorally regression-tested and independently
re-reviewed with no remaining demonstrated issue in the bounded repair.
Upgraded controllers fail closed on older uncorrelated approvals.

Checks: 162 Linux remote-control tests; 173 Windows tests passed with two
native-only smokes ignored by default; the separately run real keyboard smoke
passed; 57 web lifecycle tests and three desktop behavioral tests passed.
Desktop typecheck and production frontend build, Linux application build and
Windows application build passed. Existing warnings remain. The Windows test
artifact uses the Common Controls v6 manifest documented below.
Web typecheck, test compilation and production build also passed.
`petal-approved-prerequisite-acceptance.json` records copied-artifact hashes,
exact editor observations, wire counters and review provenance.

Owned clients left and quit, the guarded editor and private input session
closed, the gate/forwarder exited and the owned SFU room has no participants.
The private compositor was stopped separately. The VM and installed Windows
release remain unchanged. No push, PR, deployment, shared-service, hardware or
human keyboard/clipboard change occurred. Physical camera acceptance remains
blocked; these results do not establish acoustic AEC or delivery.

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

Monitor publication additionally requires the GNOME ScreenCast portal on the
current user session bus, PipeWire, GStreamer 1.28 or newer, and the
`pipewiresrc`, `videoconvert`, `videocrop`, `videoflip`, `videorate`, `capsfilter`
and `appsink` elements. The Linux-target Rust bindings use the existing GLib 0.18 family;
there is no GTK4 or browser-media path. A missing capture element is a
per-attempt error, not permission to install software or capture a different
source. Camera and desktop input are separate, unavailable capabilities.

The monitor-entry assertion requires no room join or source selection:

```sh
/usr/bin/python3 apps/desktop/scripts/check-linux-capabilities.py \
  apps/desktop/src-tauri/target/debug/desktop --monitor-entry
```

It checks that the native Share command reports a missing room rather than
blanket Linux unavailability. It does not prove capture or publication. The
S01 gate in `plans/2026-10-28-native-linux-publishing-control.md` separately
requires explicit approval of a non-sensitive monitor, the actual native
picker, a changing counter rendered by the released Windows peer, and normal
Stop removing its publication and remote window. Do not activate a whole
monitor merely to run the admission assertion.

S01 was exercised on this GNOME Wayland computer at source snapshot
`8ea2059d` on October 28, 2026: the released Windows client rendered DP-1
in an independent share window, followed the owned counter from 10 to 11,
and closed that window after normal Stop. An isolated mutation suppressing
ongoing `PublishedTrack::push_frame` calls left a native publication active
but failed to sustain the Windows picture; the unchanged candidate then
passed the changing-counter and Stop checks again. This is monitor-path
evidence, not completion of the remaining publishing/control tasks.

GNOME preselects a sole monitor. Automation must inspect the source toggle's
settled `PRESSED` state before acting; do not toggle an already selected
source off. Approve Share only while the intended source remains pressed
and the native Share button is sensitive. Accepting an empty selection
crashed the tested GNOME portal backend; a transient activation state is
not settled source-selection proof.

`scripts/check-linux-monitor-publication.py` binds that picture gate to the
actual Windows VM pixels and actual SFU publications. Its retained-app
`run_s01(app, observe_participant, approve_share, windows_vm, evidence_dir)`
entrypoint creates a real owned GTK counter with a per-run nonce, requires
Windows to render its 10 → 11 transition, and checks both SFU unpublication
and sustained remote-window absence after normal UI Stop. It neither uses a
second SDK participant nor substitutes synthetic video. The standalone CLI
requires an explicitly named owned Windows VM, existing native peer/room,
private short-lived room-admin token file and `--allow-desktop-share`.
Screenshots stay in a new private evidence directory. Wake an idle VM's
display through its normal authorized flow; the witness does not change VM
settings or inject guest input.

The same real screenshot is read with sparse and block OCR segmentation:
sparse layout retains the nonce counter, while block layout retains a colored
native title bar when recursive desktop pictures confuse sparse layout.
Both predicates still come from the actual Windows frame; neither is seeded
from expected text or a test-only receiver API.

While a Linux monitor is active, More → Change monitor uses the existing native
`open_window_picker_window` entrypoint for a fresh monitor permission attempt.
The original picture keeps running until the new publisher accepts a real frame.
Only then does the owner promote the new share identity and retire the old
publication; the command waits for that retirement while the new picture keeps
pumping. Cancellation or failure before that commit releases only the new
attempt. The normal Share/Stop action still stops the owned active/pending work.
This is single-picture replacement, not Task1.2 multi-share support.

`scripts/check-linux-share-audio-preservation.py` exercises the S05 matrix using
real native commands plus read-only SFU publication observations. Its retained-app
`run_s05(app, observe_participant, approve_share)` entrypoint avoids creating
another room participant. The observer must read the actual original Linux
participant; the approval callback must operate the actual owned-source picker.
For a focused muted change, pass `rows=((True, 'change'),)`. The default runs all
six muted/unmuted start/change/stop rows; the CLI requires
`--allow-microphone-unmute` for any selected unmuted row, which still needs
explicit runtime authorization. It samples the sole microphone publication's
SID/mute state, native mute state, and absence of other audio publications while
operations are pending and settled. A change is one picker operation while the
original share is active, not a test-created stop/restart. Its host-only scoped
room-admin bearer is never passed to Petal or followed through a redirect/proxy.
These checks do not establish speech quality or acoustic AEC.

All six S05 rows passed against the local pre-S08 monitor candidate
(`06d8aa56` source lineage), with explicit permission for the unmuted rows.
Unmuted start/change/stop yielded 130/201/115 independent microphone samples;
the earlier muted rows also passed. An isolated mutation forcing real microphone
mute at monitor start failed specifically because the share action changed the
SFU microphone mute state. The unchanged original then passed unmuted start
again (136 samples). Each run remuted the microphone, stopped owned shares and
left normally. These are local acceptance results, not delivery or speech/AEC
acceptance; later production changes require current-head checks.

`scripts/check-linux-share-permission.py` separately exercises real native
Cancel while an original monitor remains live. A fresh nonce counter rendered
10 → 11 in Windows before/after Cancel, with the original native identity,
SFU video SID and sole muted microphone retained. Changing only attempt-local
cleanup to owner-wide cancellation failed after the real Cancel, once the
original publication had been established; the unchanged candidate then passed
again with 25 microphone samples and changing Windows pixels. The production
portal protocol test also rejects actual private-bus Cancel, other-failure
and unknown responses emitted before the method reply. Treating response 2
as success failed its negative-response assertion; restoring that branch
passed. Response 2 means the request ended another way, not a distinct
permission-denial code. No global portal
was replaced. GNOME offers no separate runtime Deny button here, and the planned
Add share affordance belongs to multi-share work: full S06 remains incomplete.

S08 also passed with real captured A frames delayed by a non-stop debugger
attached only to the owned native process. Pause after `source.next_frame()`
returns and before `is_current` admits the first frame; inspect only its width
and height, not captured bytes. For Stop, Leave, replacement and actual removal
of the owned Linux SFU participant, retire A before resuming that thread.
Every row rejected the stale 3840 × 1080 frame without publishing A, then
required fresh explicit B permission and actual Windows nonce counters 10 → 11.
B retained its native identity, sole video SID and view-only metadata.
Room retirement correctly created a new, still-muted microphone on re-entry.

The Stop row additionally held A after identity-qualified `finish` and before
its ready result, then explicitly started B. B published while both A's start
and Stop commands remained unresolved. Releasing A rejected its old start and
resolved Stop after B, without changing B's publication or Windows pixels.
Start B only after that held retirement boundary: queuing it beforehand can
leave its wakeup in the paused Tokio worker's non-stealable LIFO slot, which
tests debugger scheduling rather than share ownership. Leave/disconnection
serialize room cleanup; their old completion preceded B and is not claimed
as an after-B callback. The permission-only helper
`scripts/check-linux-share-generation.py` remains a separately labeled prefix,
not evidence that unapproved A produced frames.

Three ownership tests independently reject old frame admission, stale commit
and identity-unqualified cleanup. Isolated mutations removing those respective
guards each failed its corresponding assertion; all three passed after
restoration. These results establish the generation scenario, not S10 failure
recovery or completion of Task1.1.

All four S10 replacement failures were exercised against the corrected native
artifact, with the original muted participant and released Windows peer:
disconnect only B's newly opened private portal socket; remove only this
process's `videoflip` registry feature while retaining/restoring its reference;
shut down only B's restricted PipeWire remote before capture starts; and pass
an invalid owned `PETAL_SHARE_LADDER` value through the real native publisher.
The last fixture restores the process environment immediately after Rust has
read its owned value. It is native configuration rejection, not SFU rejection.
No shared portal, PipeWire, SFU or VM was stopped or reconfigured.

Each real More → Change monitor failure rendered a newly observed visible
warning. Its attempt-time socket identities eventually disappeared; asynchronous
GStreamer/connection retirement is not assumed complete at the first toast.
The original share ID/video SID, sole muted microphone and changing Windows
nonce counters 10 → 11 remained intact. Fresh explicit source consent after
restoring the resource produced a new share ID/video SID and changing Windows
pixels. No retry selected content or granted input automatically.

The native-publication fixture first reproduced a real cleanup panic:
`JoinHandle polled after completion`. Propagating a completed publisher error
left its handle in `publishing`, so cleanup awaited it again and the UI lost
the underlying error. The completed handle is now cleared before propagating
its result; cancellation/timeouts still retain unfinished work for retirement.
Metadata completion follows the same ownership rule. Repeating the real
failure now displays `Native monitor publication rejected: invalid video
publish configuration`, releases B, preserves A and permits explicit retry;
the owned corrected process recorded no repeat of that panic. Five focused
Linux share/portal tests and the native build passed. These are local build
results, not publication, deployment or completion of the remaining tasks.

The Task1.1 boundary additionally exercised a normal, traced native launch:
joined the existing private room, received the released Windows client's real
safe Notepad window, and read changing counters 890 → 891 from the independent
GTK window through owned window-only compositor pixels. The original sole
microphone remained muted with the same SID and no computer-audio track.
Normal Leave during a real, unapproved monitor chooser rejected that pending
start, removed the incoming GTK window and SFU membership, closed the chooser,
and left no share IDs. The launch's actual execution/activation observations
found no browser or foreign-platform preparation.

The VM peer's hardware H.264 path first reported `MFT is not asynchronous`,
`InitEncode` failure and repeated encode error `-7`; no incoming pixels were
claimed from that run. The same native Windows artifact works with its existing
process-local `PETAL_FORCE_SOFTWARE_ENCODE=1` override. Preserve the independent
`PETAL_BACKEND_URL` override as well when restarting this owned test client:
without it, its saved room code does not establish membership on the owned SFU.
Use the profile menu's normal Quit Petal action, not Alt-F4, which can destroy
the main window while leaving the single-instance process alive. VM, OS,
hardware permissions and shared services were not reconfigured.

The bounded basic review of `48490804b846 → 06c293716cb7` reported no
demonstrated defects; later native-module changes were formatting only.
The final native Linux layer gate passed 10 tests and built the executable.
Clippy completed with 67 warnings in unchanged modules; the frontend check
reported zero errors and the existing RoomRow accessibility warning.

Task1.2 is an unaccepted local candidate, not a completed task. The current
native Linux layer passed 20 tests; the unchanged frontend's earlier check
reported zero errors and the existing RoomRow warning. The changed UI was
bundled and the native executable rebuilt before exercising it. Actual
Add share → Cancel preserved the original share ID/video SID, muted microphone
and fresh Windows counters 10 → 11. This covers Cancel only; native runtime
Deny and the complete S06 outline remain unverified.

With explicit temporary pointer-only consent for the owned native chooser,
window selection now works. The driver captures only that active chooser,
locates the owned source's nonce in actual pixels, verifies its selected row,
and releases the pointer session before invoking Share. It does not use GTK's
fabricated `(0, 0)` screen coordinates or grant product remote control.

The October 29 candidate `985c57ceb50a` passed 20 native Linux tests and the
native build. Actual Windows pixels show the selected editor's counter 20 → 21
while the unrelated private fixture is foreground, without that private
fixture in the publication. The original microphone stayed muted.
The same candidate published DP-1 plus three independent application windows;
a fifth Add emitted no new-picker event and retained all four publication SIDs.
At that limit, per-source Change replaced only its target, rendering counters
70 → 71. Individual Stop and normal source closure exposed the unrelated
Windows pictures at 60, 50 and then 51 without changing their publication SIDs.

An isolated copy changed only connected refresh from 1000 ms to zero. The
same S02 witness then failed specifically because Windows never rendered
the initial editor counter 20. After restoring that copy's source, the
unchanged original candidate passed counters 20 → 21 and private-window
exclusion again, with 64 independent muted-microphone samples. S02 is now
recorded in the living feature; the remaining outline rows stay TODO.
The connected X03 gate also passed on this candidate: actual monitor counters
10 → 11, Stop removing the Windows remote window, 25 unfinished direct
operations rejected, and real Cancel displaying the failure warning. The
original microphone remained muted; camera and input were not activated.
The controls observer also now handles an already-open More menu without
closing it while waiting for the controls it needs.


Static window startup needed connected-only `pipewiresrc` keepalive: a real
idle window otherwise provided no Windows first picture after subscription.
The native pipeline also now applies validated per-buffer crop metadata using
`videocrop` before `videoflip`, which otherwise discards that metadata. It
consumes the metadata before cropping to avoid applying the origin twice,
resets cropping on uncropped buffers, and lets native renegotiation handle
changed dimensions. Seven real GStreamer tests cover padded stride, offsets,
changing bounds, uncropped reset, malformed crop and orientation.
Windows also rendered the real editor resize from 1280 × 720 to 960 × 960,
with all four corner labels, readable counter 30 → 31 and the same publication.
Fixture stdin commands must be serialized through an observed frame: sending
resize and counter commands together can leave the latter in Python's
read-ahead buffer rather than the GLib file-descriptor callback.

The renewed credential authorization restored the actual Windows desktop and
the existing released native peer in its owned room. Temporary credential
references were discarded; the credential file and VM settings were unchanged.
The window-geometry row then rendered 1280 × 720 → 960 × 960 with all four
corner labels, fresh counters 31 → 32, the same publication SID and an actual
cursor at the corresponding grid positions. This is window-row evidence, not
completion of the full S09 outline.

The picture fixture's `fullscreen` command presents an owned corner grid for
monitor checks. DP-1 and the independent window published simultaneously, and
Windows rendered the full-monitor counter 41. A guarded change from 100% to
150% scale then ended the monitor's capture authority: its publication and
Windows window retired, the unrelated window retained its SID, and Linux
displayed “Picture sharing access ended; select Share explicitly to retry.”
Counter 42 never reached Windows. This failed the original S09 uninterrupted
monitor-continuity criterion; it is not a successful geometry transition or
permission to reacquire capture silently. Tommy subsequently approved the
native-grant rule in ADR 0003: preserve geometry while authority remains valid,
otherwise retire only the affected share and require explicit native
re-selection. Fresh-picker recovery and a new current picture have not yet
been exercised after this scale-triggered retirement; S09 remains TODO.

The original 3840 × 1080 mode, 100% scale, layout and transform were restored
and verified before the independent rollback watchdog was disarmed. The
Windows VM subsequently entered Aborted state during this display-change
attempt. Read-only host coredump and kernel records later showed both aborts
were the same host-side VirtualBox GUI fault: its desktop-watchdog geometry
helper crashed inside Qt's accessible-name lookup with a null widget pointer.
An isolated offscreen Qt witness reproduced that exact fault after a wildcard
widget disconnect, while a receiver-scoped disconnect completed. This is a
host GUI defect, not a Windows guest or Petal media failure; a process-local
watchdog bypass exists for a future owned launch but has not been applied or
boot-tested. No further display mutation or credential reuse followed.

The amended recovery scenario passed end to end on the restored fixture: with
the editor share and muted microphone active, a guarded 100% → 150% scale
change ended only DP-1's capture authority, displayed the access-ended
explanation, and started no replacement. Explicit Share again opened the
native source picker with nothing published beforehand; explicitly selecting
DP-1 created a new share instance (fresh SID, new window ID) whose counter 62
reached Windows. The editor kept its SID throughout and rendered counter 52
after the recovered monitor share was stopped; the microphone stayed muted.
The original display configuration was again restored and verified before the
rollback watchdog was disarmed, and the VM remained running under the
process-local watchdog bypass. The same proof passed for the resolution row:
a guarded change from 3840×1080@59.968 to 3840×2160@59.940 also ended the
monitor grant, and explicit Share again plus fresh DP-1 selection produced a
new share instance rendering counter 82, with the editor still publishing its
original SID and rendering 71 after the monitor share stopped. Both geometry
rows of the recovery outline are therefore exercised; no authority-valid
monitor geometry transition was observed on this GNOME installation, because
every tested geometry change ends the capture grant. The window-geometry row
of the preservation outline remains separately verified for resize.

S06's cancel row passed with real UI: while DP-1 published counter 91, a
second Add share followed by the native chooser's Cancel published nothing,
left DP-1's SID unchanged, and DP-1 rendered counter 92 afterward with no
substitution. The deny row of that outline remains unverified because the
installed GNOME 51 chooser has no distinct Deny control; this is an
environmental acceptance gap, not a waiver. S07's editor-close row passed:
closing the shared editor window retired only its publication (window ID 1)
while the unrelated monitor share kept its SID and rendered counter 111. Its
DP-1-disconnect row is not exercisable on this single-monitor host (Mutter
rejects a zero-monitor configuration, and a physical unplug would require the
user to replug), and its scoped-revocation row has no authorized external
per-handle OS revocation API on GNOME 51. Both rows stay open and honestly
unverified; neither is waived.

Task1.2 remains incomplete: native runtime Deny, physical display
disconnection, scoped capture-permission revocation and the amended monitor
geometry/recovery scenarios remain unverified. Existing explicit runtime
consent covers temporary DP-1 configuration/disconnection, pointer movement
within owned test windows and revoking only Petal's capture grant, with the
original display configuration saved and restored. The separate acceptance
approval changes S09 as recorded above; neither grant authorizes camera or
product remote control.

The installed GNOME 51 chooser maps both Cancel and its ordinary close action
to response 1. Its response-2 paths represent session/backend failures rather
than a distinct user Deny action; see the
[GNOME 51 response handler](https://github.com/GNOME/xdg-desktop-portal-gnome/blob/51.0/src/screencast.c#L312-L355)
and [dialog callbacks](https://github.com/GNOME/xdg-desktop-portal-gnome/blob/51.0/src/screencastdialog.c).
That acceptance mismatch is unresolved, not waived by a protocol failure test.
The three blocked rows were accepted as documented platform limits
(ADR 0003): GNOME 51's chooser offers no distinct Deny control, Mutter
rejects a zero-monitor configuration on this single-monitor host, and no
external per-share capture-permission revocation API exists. They are
recorded as deviation rows in the coverage ledger — unverified limits that
reopen if the platform grows the control — not as satisfied criteria. With
that acceptance, every Task1.2 scenario is DONE in the plan and its living
feature records the exercised S06 cancel, S07 editor-close and both S09
behaviors.

Task 2.1 (native Linux camera) was exercised on this computer at source
snapshot `653eb3e4` on October 29, 2026, in room `kwa-xjth-qav` with the
released Windows peer muted. Turning the camera on published a live
`petal-camera-<identity>` H.264 640x480 track whose SFU layer carried a real
SSRC and bitrate while the publisher sustained 15 fps (captured=4584,
pushed=4576); the Windows peer's Linux tile rendered actual image content
rather than a placeholder, and a native "Camera self-view (HD Pro Webcam
C920" GTK window existed without any browser acquisition. Stopping removed
the SFU track, and after leaving the room a late camera start was rejected
with "not in room". `scripts/check-linux-camera-publication.py` binds those
C01/C07 gates to the real SFU and real command results.

Three bugs were found and fixed while exercising that path, all specific to
shipping `pipewire-rs` inside this binary: the bundled libwebrtc static
library defines hidden `pw_*` dlopen stubs that captured the crate's calls
(`-Wl,--wrap` now routes them to the real libpipewire through
`linux_pw_compat.rs`), `wait_initial` violated the thread-loop lock/wait
protocol and leaked the loop mutex, and `CameraRegistry::drop` called
`pw_thread_loop_stop` while holding that mutex (self-deadlock). Separately,
pipewire-rs listeners were dropped immediately after registration, which
sensed nothing; the handles now live exactly as long as the registry.

Camera acceptance limits on this desktop (recorded as the Task 2.1
deviation, ADR 0003): neither installed portal frontend implements
`org.freedesktop.impl.portal.Camera`, so the Camera portal auto-grants
without a dialog and its restricted remote lists zero camera nodes. The app
probes the portal once and then enumerates and captures through the session
PipeWire daemon directly — the platform-normal path for a host application,
with the in-app Camera button as the consent surface. C01's
"approve native permission" step therefore has no OS dialog to exercise
here, and the "changing pixels" clause is proven by SFU layer liveness plus
rendered tile variance because the camera views a static scene (no physical
marker was moved). C02's preview-first and publication-first overlap and C07
have live evidence. The later isolated checks below complete C03 and controlled
multi-device C04, all three C05 failure/recovery rows and C02's attended physical indicator.

After the November 3 physical replug, bounded direct V4L2 capture delivered
60 frames and three suspended-device reopen cycles delivered 30 frames each;
a PipeWire capture also delivered 30 frames. A camera-only autosuspend
auto/on/auto comparison completed nine 30-frame cycles without USB faults and
restored the original auto/2000 ms policy. This does not establish autosuspend
as the cause of the earlier failures.

The fresh native app then opened Settings preview before publication and
published the real C920 at Auto 640×480 to released Windows 0.9.33. The receiver
reported 640×480 video readyState 4; one owned PipeWire capture served the
overlapping preview/publication, and publisher health reached captured=4333,
pushed=4321 at about 24 fps. Selecting the advertised 1280×720 at 10 fps through
the actual Settings controls failed with "all buffers have been removed".
Publication and intent became false, Settings displayed the neutral camera-loss
message, and the Windows videos subsequently had zero dimensions/readyState 0.
Kernel records then showed USB `1-6.4.1.2` descriptor timeouts (`-110`), address
failures (`-62`), disconnect and failed re-enumeration. The failure is reproduced,
but its attribution to capture teardown, driver, firmware or USB hardware is
unresolved. A camera-only USB reset could not run because the device had already
disappeared. Cycling only its USB2 port and empty USB3 companion did not recover
it; both port-disable attributes were verified restored to zero. No whole-hub
or controller reset, shared-service restart or Windows update was performed.

A separate private PipeWire witness found and fixed an owner race: an older
selection could cancel the newer serving capture before its late start was
rejected. Generation/node/requested-mode admission now runs before adoption and
atomically with capture removal after discovery. Before the fix, B's frames
stalled at 35; afterward the same B capture survived with real dispatched frames
increasing 5→14. Controlled source removal also retired all affected leases and
the view mailbox, preserved a separate B stream, and required explicit restart
after A returned. These are actual-owner/private-producer checks, not full
Settings/self-view/SFU or DP-1/microphone acceptance. The integrated 13 focused
owner tests and desktop build passed; independent review found no issues in the
narrow fix. It is not established as a repair for the physical C920 USB failure.

#### C920 mode-switch buffer-pool repair, November 4, 2026

Direct connection to USB `1-7` did not eliminate the failure. A capture-only
Auto 640×480/30 → 1280×720/10 transition reproduced it without Petal, room
publication or overlapping preview leases, even after releasing the retired
pipeline objects. Exact source caps did not prevent it either. Fresh 720p
capture alone delivered 30 frames with both source and output at 1280×720/10.

The PipeWire trace identified a false loss transition: after allocating the
720p buffers, the source removed the pool, posted "all buffers have been
removed", immediately allocated replacement buffers and returned to streaming
before caller cleanup. `on-disconnect=error` made that normal rebuild fatal.
Changing only that property to `none` passed Auto → 720p → 640×480 twice:
30 real frames per stage, once with source caps pinned and once with the
original unpinned chain. All 180 frames were discarded, not saved as images.
Selected-node removal/core loss and actual pipeline ERROR/EOS remain terminal.

The rebuilt native app then passed the previously failing Settings-preview-first
flow in an owned room: 1280×720 at 10 fps, a real Settings change to 7.5 fps,
then 640×480 at 30 fps. SFU camera-track geometry matched each size; native
capture/push health was 10.0/10.0, approximately 7.5/7.5 and 30.0/30.0 fps.
The microphone stayed muted. Closing Settings preview left publication live;
turning publication off with a preview reopened removed only the camera track
and left the PipeWire source running for preview. Closing that last preview
released capture. Normal Leave/Quit ended the owned app with exit 0 and left
the room empty, the camera node suspended and no camera links.

All 34 focused Linux-camera tests passed, and `cargo build --locked --bin desktop`
succeeded. No new camera USB errors appeared from the successful discriminator
through native cleanup. This fixes the demonstrated false buffer-pool-loss
failure; it does not establish the cause or recovery of every earlier USB
descriptor timeout, full C04 multi-device/latest-selection acceptance, physical
indicator behavior or all permission-loss rows. No released Windows peer was
used in this follow-up. No reset, power-policy change, shared-service restart,
installed-release replacement or repository publication was performed.

#### Controlled-camera C03/C04 acceptance, November 4–5, 2026

C03 and controlled-source C04 are **passed locally**. The combined C04 witness
uses genuine saved A completion/frame replay through original owner paths;
it does not claim a naturally late OS open, physical recovery or host activation.

Actual native Settings exposed controlled A1280×720/10 and B640×480/30 through
a private Camera portal. A fixture-only `pipewiresink use-bufferpool=false`
change repaired producer starvation. Production selection now normalizes an
unsupported saved mode for the newly selected device atomically instead of
turning the camera off. Visible previews retain their own leases.

Two receiver failures were identified and repaired without changing released
Windows0.9.33 or the server. Removing the camera-only frame-metadata override
eliminated malformed H.264 STAP-A lengths: a passive receiver observed827 RTP
packets and no malformed packets. Retaining the Linux publication across
capture handoffs avoids the released receiver's identity-only old-unsubscribe
handler clearing a replacement track. Handoffs preserve the publication SID,
update native-source dimensions and retain the requested SDK encoding limits.

Through actual Settings, A/B selection, explicit modes and rapid A→B→A→B
selection produced matching native GTK self-view pixels and released Windows
video. Windows retained SSRC129349529 across640×480/30→1280×720/10→640×480/30;
NACK/PLI counts remained zero. Native snapshots showed red A and blue B, with
final B at640×480/30. Removing A stopped publication without silently selecting
B; explicit B selection and Camera On recovered. Leaving stopped remote video
while a visible preview retained capture; closing that preview left no links.

The combined witness used one throwaway instrumented native app, actual
Settings and unchanged Windows0.9.33. A1280×720/10 and B640×480/30 were selected
through their actual advertised selectors, including rapid A→B→A→B changes.
After visible B activation, an external marker released a saved genuine
already-completed A owner record and actual A frame through the original
`capture_started`/`frame_arrived` paths. Serialized capture opening prevents a
naturally late A open from overtaking B; no serialization was bypassed.
The original status gate rejected the old frame and completion returned
`Camera selection superseded`. B retained the same capture status, publication,
preview, supported mode and Settings selection with no owner error.

One hundred actual GTK self-view DrawingArea snapshots over14.195s and65
released-Windows decoded observations over8.092s remained blue across replay
and30 subsequent B dispatches. Maximum measured gaps were0.202s/0.152s;
Windows presented frames advanced2690→2933 at640×480. GTK snapshots invoke the
actual widget renderer at an idle boundary; natural-draw observers timed out,
so neither compositor scanout nor exhaustive frame-by-frame coverage is claimed.
No GTK critical/assertion occurred. The actual red A and blue B renders were
inspected, not inferred from owner state alone.

An early overlapping Settings/capture-validation check exposed a second
mode-query consumer overwriting the registry's one completion slot. A private
mutex now serializes the complete mode-query transaction, without blocking
PipeWire callbacks. Thirty-two real simultaneous-consumer queries returned all
correct advertised640×480/30 answers; before the fix,28 were superseded.
The rebuilt native UI also passed Camera On→Settings→explicit A mode with a
red self-view and active SFU camera. Independent frozen-source review found no
issue. Contending queries can wait for earlier queries before their own bounded
remote-answer deadline starts; this is not a five-second end-to-end guarantee.

An SDK integration check pushed real NV12 frames through B→A→fractional-rate B
and a client-local full reconnect: retained and live limits matched833333bps/10
for A and832500bps/29.97 for B, with640×480 restored after reconnect.
The final camera-filtered suite passed107 tests with one ignored SDK integration
test; that integration test passed separately before the permission follow-up,
and the final locked desktop build succeeded. Independent narrow-source review
found no remaining issue after the cancellation cleanup correction below.

C03 used the actual Linux executable and unchanged Windows0.9.33 in an owned
room. With synthetic nodes present, initial `check_camera` returned
`not-determined`, not a non-Linux authorized default. Real private portal denial
showed denied-or-cancelled/camera off. Production Off cancelled an actual pending
request through `Request.Close`, closed its GTK dialog and showed camera
cancelled/off. With both sources removed, a request showed no camera available.
All three rows left intent/publication false, no SFU/Windows video and no capture
links. The absence row used stored private portal YES; the native permission API
still cached the earlier Denied result, so that row does not claim it reported
Authorized. Portal response1 itself means denied **or** cancelled; only explicit
caller cancellation receives the definite cancellation explanation.

Real pending-publication dialogs now close on actual meeting Leave and the
production Off command, observed within0.393s and0.192s respectively. Publication
permission tokens retire on Off/room-generation changes without cancelling
independent Settings requests. A further real Settings→queued On→Off sequence
exposed stale start admission: On used to set intent after the newer Off.
Admission now snapshots retirement and room generation before waiting for camera
control. Actual queued Off and queued Leave checks preserved the independent
Settings dialog; after denying that request, intent/publication remained false,
no second prompt appeared, and Windows had no video.

Owned failure cleanup does not advance explicit retirement: deterministic
On₁→Off→queued On₂→delayed On₁ cancellation cleanup coverage protects the newer
On₂. A native Off→explicit On₂ check admitted a fresh real permission request
with intent true; denying it returned intent/publication to false. This runtime
check does not establish the exact internal cleanup ordering; the regression
controls that interleaving.

C05 is **DONE**: permission loss, capture failure and attended physical unplug/replug all passed with other media preserved.
The earlier private Camera portal/PipeWire/WirePlumber check established real
node-access-loss retirement and explicit recovery with synthetic blue640×480/30.
PermissionStore camera NO did not revoke this non-sandboxed native client:
installed WirePlumber grants access to its empty application ID. No fake sandbox
identity or permission result was used to manufacture portal-store revocation.

The November 5 attended check used the actual C920, authorized DP-1 share and
unmuted Volt276 microphone with unchanged released Windows0.9.33. Revoking only
the owned camera clients' actual source ACL retired camera intent/publication,
showed the neutral failure and removed its outgoing camera track. Restoring
access did not auto-enable camera; explicit On recovered it. DP-1 and microphone
kept their SIDs and states; Windows continued decoding the share and receiving
microphone audio. This completes the permission-loss row through real node ACL
loss, not through PermissionStore revocation.

Destroying only the owned camera `Stream/Input/Video` node initially exposed a
real capture-failure bug: frames stopped but camera intent/publication stayed
true. Each capture now gives its input node a unique process/counter-qualified
name and watches that exact node's removal. Retirement settles `next_frame`
without another sample; intentional teardown retires before NULL, so its later
node-removal event cannot report a false failure. No stall timeout or retry was
added, and source buffer rebuilds remain nonterminal.

The repaired actual failure-and-explicit-recovery row passed with all three
media live: camera retired with an explanation, while the same DP-1/microphone
tracks and Windows share decoding/audio reception continued. Real Settings
switches to320×240 and back to640×480 remained healthy with other media preserved.
Camera tests passed110 with1 ignored; the locked desktop build passed. One
independent narrow source review found no demonstrated defect; capture/owner
change attribution used its frozen manifest, while metadata had an exact diff.

The fresh attended physical row passed on November 5 with the actual C920,
authorized DP-1 share, independent unmuted Volt276 microphone and unchanged
released Windows0.9.33. A continuous observer was armed before the unplug cue.
Tommy physically unplugged the C920: camera intent/publication became false,
Linux explained the failure and the outgoing camera track disappeared. The same
DP-1/microphone SIDs and states persisted; Windows continued decoding3840×1080
share frames and receiving microphone audio.

After Tommy replugged the C920, camera stayed off with no outgoing camera track.
Explicit Camera On recovered640×480 video with advancing Windows decoded
frames, while DP-1/microphone remained unchanged. This completes the physical
row; the earlier missed-snapshot and timed-out attempts were not counted as
passes. Physical-indicator ownership is established separately by the attended
C02 check below; other expansion gates remain open.

Both owned clients then left and quit normally, the guard stopped, the temporary
guest helper was removed, the room was empty and no owned camera/voice PipeWire
clients remained. The Windows VM stayed running with clipboard/drag-and-drop
disabled; no installed release, shared service or repository publication changed.
Fresh receipts are under `/tmp/petal-c05-physical-fresh-tsbif4x5`:
`physical-before.json`, `physical-loss.json`, `physical-replug-no-auto-on.json`,
`physical-row-complete.json` and `cleanup.json`. The earlier permission-loss,
repaired capture-failure and mode-switch receipts remain under
`/tmp/petal-c05-attended-gpswnl7c`.

The earlier private-fixture clients both left and quit normally; its media room was empty.
Private sources, PipeWire/WirePlumber, portal/bus and14 ownership-verified
activated children stopped; guest helper, scratch profiles, standalone driver
binaries and the copied instrumented desktop were removed. Bounded evidence,
source snapshots, logs and GTK images remain under
`/tmp/petal-autonomous-acceptance-lw46v89y`; the main receipts are
`c04-combined-acceptance.json`, `c05-real-access-revocation-partial.json`,
`mode-query-concurrency-acceptance.json` and `cleanup-final.json`.
The Windows VM remains running with clipboard/drag-and-drop disabled and its
installed0.9.33 release unchanged. No physical reconnect, shared-service change,
installed-release replacement or repository publication was performed.

**November 5 attended C02 indicator acceptance: DONE.** Actual Settings preview
started before publication. Preview, native self-view and released Windows0.9.33
camera video at640×480 used the same one running PipeWire input node/link.
Tommy confirmed the physical C920 light **on**. Stopping publication removed the
outgoing camera track and Windows camera video while the same visible preview
and capture remained; Tommy again confirmed **on**. Closing Settings alone left
the separate native preview window visible. Closing that actual last preview
removed its capture node/link and all camera surfaces, with no outgoing camera;
Tommy confirmed **off**.

The first preview-start attempt failed alongside kernel USB firmware-change and
re-enumeration messages. One normal Settings close/reopen recovered before the
accepted sequence; no manual device reset or power-policy change was performed.
This result does not identify the USB failure cause or establish durable hardware
recovery. Both microphones stayed muted and no screen was shared during C02.

Both owned clients left and quit normally; the bounded guard stopped, guest
helper was removed, room was empty and no owned camera/voice clients remained.
The Windows VM stayed running, clipboard/drag-and-drop disabled and release
unchanged. Receipts under `/tmp/petal-c02-indicator-y4kia9yc` include
`c02-complete.json`, `human-led-observations.json`, `preview-only.json`,
`preview-and-publication.json`, `publication-off-preview-retained.json`,
`last-preview-closed.json` and `cleanup.json`. No publication or deployment.

**November 5 actual Linux-host H01 window check: safety verified, acceptance
incomplete.** Tommy selected only the task-owned editor in the real native
chooser. The Linux picture-share record reported one source of kind `window`;
unchanged released Windows0.9.33 decoded it at736×416, with received frames
advancing161→269 across control attempts. Both microphones stayed muted and
no camera or desktop-wide permission was requested.

The actual Windows Control button was disabled and said “Preparing remote
control… it will enable automatically”, rather than explaining view-only
operation. Invoking its real `remote_control_set_active` request command
fulfilled with `false`; this is not evidence of an active grant or an explicit
host denial. Real Linux global-enable, per-share-enable and answer-consent
commands each rejected with the explicit Linux-host unsupported error, and
Linux's actual menu labels request/grant control unsupported.

The header-only D-Bus observer saw zero owned RemoteDesktop/InputCapture calls
and zero unattributed calls. The editor had zero key/pointer events or text
changes; the picture kept decoding. These checks establish safe refusal under
the current global Linux-host block, not the complete window-specific
view-only feedback contract. H01 stays TODO; the Windows preparing state must
not be presented as a completed refusal/availability explanation.

The manual chooser prerequisite is now resolved. Both owned clients and editor
quit normally; the guard, observer and helper were removed, room empty, no owned
capture clients, VM/release unchanged. Current bounded receipts:
`/tmp/petal-h01-resume-ri03q_pt/actual-window-boundary-check.json` and
`cleanup.json`. The earlier chooser-timeout preflight remains historical at
`/tmp/petal-h01-window-boundary-lox9tjoo`; no product code, shared-service,
publication or deployment changes were made.


Run the native read-only prerequisite diagnostic with an intentionally missing
capture-plugin search path:

```sh
/usr/bin/python3 apps/desktop/scripts/check-linux-capabilities.py \
  apps/desktop/src-tauri/target/debug/desktop --prerequisites
```

The caller launches the production executable with an empty private GStreamer
plugin directory and registry, then invokes `linux_native_prerequisites`.
The command loads factory metadata without constructing capture elements and
reads only advertised ScreenCast properties from the portal's existing unique
bus owner. It neither starts a stopped portal service nor creates a portal
session, requests permission, opens a PipeWire remote, selects a device or
installs software. Its report names missing components and their monitor-sharing
effect. Camera and input remain unavailable in this unit. A positive component
report is not permission, a working capture stream or Windows-rendering proof.
The check observes owned portal method-call headers and rejects media/input
activation; it does not record selected content.

The missing-plugin diagnostic passed again on October 29 without portal,
screen, camera or input activation (4.27 seconds). Its earlier isolated
`pipewiresrc`-availability mutation failed the actual native diagnostic; the
restored command passed. The documented scope remains this GNOME Wayland
computer, not general distribution support.

For connected X03 acceptance, import `run_x03` from
`scripts/check-linux-capabilities.py` with the retained native app, original
participant observer, authorized monitor approval/Cancel callbacks, owned
Windows VM and private evidence directory. Unlike the disconnected default
CLI check, this runs the actual monitor Share/Windows-counter/Stop witness,
inspects native room controls and rejects 25 unfinished direct operations.
A real UI Share followed by native Cancel must render a new visible degraded
toast while leaving no publication; the original room and muted microphone
remain. This passed against the S08 candidate. Bypassing the native unfinished
window admission guard failed on the resulting command-not-found response;
the unchanged original then passed again, including real Windows 10 → 11
pixels and remote-window teardown. No camera or input capability was activated.

To report the actual installed environment instead of the deliberate
missing-plugin fixture, use `--prerequisite-report` in the same command.
It prints the native JSON component report and exits 1 when monitor
prerequisites are unavailable. Neither mode requests a device or permission;
the empty-plugin witness is not a report about the host's normal plugin set.

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

### Native Linux capability boundary

The receive-only results below are dated historical evidence. The current
capability check admits the native monitor Share action, requires a missing-room
error before permission when disconnected, and retains window/region/audio,
camera and input denials. It uses a reserved non-listening loopback backend so
inspecting the meeting route cannot join a real service or activate room media.
The current bounded living scenario is in
`apps/desktop/features/linux-publishing-interop.feature`. It replaces the
obsolete blanket screen-denial contract; the historical receive-only plan is
unchanged. Its unbound default native driver passed on 2026-10-28 against the
reserved loopback backend, checking controls and disconnected-room admission
only. This is not full X03 or S01 acceptance, a real connected-room check, or
Windows-rendering evidence.

Use the same actual Wayland/session environment as the launch check. WebKitGTK
can stop animation-frame/view-transition callbacks while the desktop is blanked;
the native route's URL previously changed while the old room-entry DOM remained.
The layout now allows snapshot preparation at most 250 ms before skipping the
visual transition and releasing navigation. Normal transitions remain enabled,
and the application does not reload the document or change desktop power policy.

`/usr/bin/python3 apps/desktop/scripts/check-linux-route-transition.py
apps/desktop/src-tauri/target/debug/desktop` holds a genuine WebKit transition's
update callback and invokes the existing native `open_main_route` command. The
original executable failed with the meeting URL and no mounted meeting controls;
the repaired executable mounted the actual controls before releasing the held
callback. Normal Leave then returned to room entry in the same document. This
is native navigation evidence, not hosted membership or audio acceptance.

Final repair checks on 2026-10-28 passed the native transition regression,
the complete native capability outline, frontend/native build, and Svelte
checking (613 files, zero errors, one existing RoomRow accessibility warning).
The frontend integration suite passed 980 application tests plus 96 harness
tests, with one Windows-media skip; panel-close passed. The capability driver
no longer waits for obsolete failed-join wording before inspecting real controls.
One bounded independent source review and its scoped driver recheck found no
demonstrated defects. These checks do not change the speech/AEC scenario markers.

Keep visual capability gates awake and unlocked with the process-lifetime idle
inhibitor below. A paused renderer still cannot display new frames until rendering
resumes. `ScreenSaver.GetActive` alone does not distinguish blanking from an
authentication lock; never bypass authentication or change the user's lock policy.

```sh
gnome-session-inhibit --inhibit=idle --app-id=com.petal.app \
  --reason='Native capability verification' \
  /usr/bin/python3 apps/desktop/scripts/check-linux-capabilities.py \
  apps/desktop/src-tauri/target/debug/desktop
gnome-session-inhibit --inhibit=idle --app-id=com.petal.app \
  --reason='Native capability verification' \
  /usr/bin/python3 apps/desktop/scripts/check-linux-capabilities.py \
  apps/desktop/src-tauri/target/debug/desktop \
  --feature apps/desktop/features/linux-publishing-interop.feature
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --locked \
  linux_unsupported_commands_are_rejected_without_revoking_receive_or_audio
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --locked \
  linux_replay_refuses_real_pointer_keyboard_and_text_packets
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --locked \
  admission_table_covers_audio_camera_window_and_unknown_video
```

The native check uses real IPC, nonempty production-shaped window/control
requests and the existing room-controls route. It requires an enabled monitor
Share control with disconnected-room admission, explicit per-operation Linux
errors for unfinished entrypoints, disabled camera/region/input controls, and
unchanged disconnected-room, picture-share and camera state. It does not apply
audio-device preferences or prove hosted membership, actual media, incoming
rendering, or monitor consent. Its current living scenario binds every step;
added, omitted or changed clauses fail the driver.
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

### Native incoming-image fidelity

The complete Task 2.2 scenario passed against the same released Windows peer
and owned backend/LiveKit stack. `scripts/windows-receive-editor.ps1` updates
the same real Notepad control with known text and four corner labels. Its
optional `-GeometryPhaseFile` switches between two actual source sizes; native
DWM and editor-coordinate measurements establish the source geometry without
changing Notepad preferences or generating video.

| Phase | Measured Windows DWM frame | Actual decoded pixels | Native DrawingArea |
|---|---|---|---|
| Initial source after receiver resize | 950 × 555 | 948 × 552 | 1000 × 900 |
| Changed source at one-to-one scale | 790 × 645 | 788 × 644 | 788 × 644 |

`scripts/check-linux-receive-fidelity.py` keeps the original native room
connection, uses normal OS resizing, and observes window-only compositor
pixels. Both source aspects retain all four corners and centered black
letterboxes at the tall allocation. The source changes from `Petal Linux 12345`
to `Petal Linux 67890`; native-resolution OCR reads the new text, not the old,
after contraction to the exact decoded dimensions. Normal Leave clears room
membership and removes the native remote window before process teardown.

The existing read-only `compositor_list_windows` result reports Linux
`decodedSize`: null before the first accepted frame, then the actual source
dimensions accepted by Cairo. This is distinct from DWM border geometry and
the receiver's window allocation. A black source-border row makes the lit
content bounds one row shorter than the full frame; it is not a scale error.
The one-to-one gate requires exact decoded/allocation equality and scale 1.0,
not equality between full-frame dimensions and a nonblack pixel bounding box.

An independent plain-copy mutation changed the production Cairo fit from
minimum scale to maximum scale. The same witness joined the real room and
received the source, then rejected the cropped image after receiver resizing.
The mutated process also completed normal Leave. Only the copy was mutated;
its source was restored afterward. The current candidate then passed every
scenario clause, including the source transition, native-pixel text and Leave.
This does not establish audio, acoustic AEC or later lifecycle acceptance.

The post-review executable also passed the exact complete living scenario.
The changed Rust module passed rustfmt; serial library tests passed 1,332
with two ignored, and Clippy completed with 63 existing warnings. The initial
parallel library run exposed an unchanged process-wide descriptor-count test
race; serial execution avoids concurrent descriptor changes. Svelte checked
613 files with zero errors and one existing accessibility warning. The full
frontend gate passed 984 tests plus 96 harness tests, one Windows-only skip
and the panel-close check, using the existing isolated Playwright compatibility
runtime. A first invocation without that runtime failed WebKit's host-library
prerequisite; no host libraries were installed and no validation was bypassed.

### Native replacement publication and late retirement

For Task 2.3 only, Tommy authorized the Windows-only
`windows_wgc_race_publisher` example as an acceptance fixture. It captures the
actual selected Notepad HWND through WGC, uses normal backend admission and
the same non-development LiveKit service, and publishes two distinct track
SIDs with the same share name. It feeds real fresh source frames to B while A
remains published; only an explicit later command unpublishes A. No receiver
event injection, generated media, browser peer or server secret is involved.
The released Windows app remains the final milestone compatibility peer.

Build the example with `cargo build --locked --target
x86_64-pc-windows-msvc --example windows_wgc_race_publisher`. On Windows,
provide the explicit backend URL, human 3-4-3 room code, decimal source HWND
and an initially empty UTF-8 control file. The example uses the existing room
credential conversion and generated-participant identity convention.
`scripts/windows-receive-editor.ps1 -ReplacementPhaseFile <path>` updates the
real editor with `Publication A` or `Replacement B` and a changing counter;
the default and geometry-fidelity modes are unchanged.

Run `scripts/check-linux-receive-replacement.py <linux-executable>
--backend-url <url> --room-code <code> --evidence-dir <private-directory>
--feature apps/desktop/features/linux-remote-windows.feature`. At
`REPLACEMENT_PUBLICATION_REQUIRED`, append `publish-b` and a newline to the
publisher control file, and write `b` to the editor phase file. Wait for
`OLD_PUBLICATION_RETIREMENT_REQUIRED` before appending `retire-a` and a newline.
Append `stop` and a newline after verification to close the owned publisher.
Its five-minute lifetime is an additional cleanup bound, not media evidence.

The actual original Linux room connection logged A subscription, B
subscription for the same share, then A unpublication. Window-only compositor
pixels showed three increasing A counters, three increasing B counters before
A teardown and three newer B counters afterward. The original native remote
and meeting windows remained the same objects. Normal Leave cleared
membership and removed the remote window. The candidate witness exited zero.

A separate plain-copy mutation removed only the publication-SID retirement
guard. The same real WGC race established A and B, then rejected the missing
native window immediately after A retirement, exiting one. Its normal Leave
also cleared membership. Mutter automatically destroys a window-capture
session when that window closes; capture cleanup accepts that specific
already-destroyed-session error and still reports other failures.

The bounded basic review also exposed a source-selection bug in the new
publisher: a hidden selected HWND could be replaced by an eligible sibling
with the same process and title. Two owned real Windows forms reproduced
that substitution before the fix. The publisher now checks the unfiltered
top-level HWND set before and after picker resolution and refuses ambiguous
or ineligible selections. The same hidden/sibling witness then exited one
before capture; scoped re-review found the corrected contract satisfied.

After that correction, the byte-verified Windows executable and original
Linux receiver passed the exact living `--feature` race and normal Leave
again. Serial Rust library tests passed 1,332 with two ignored; Linux and
Windows Clippy and changed-module rustfmt completed with existing warnings.
Svelte checked 613 files with zero errors and one existing warning. The
frontend gate passed 984 tests plus 96 harness tests, with one Windows-only
skip and a passing panel-close check in the existing isolated runtime.

The production SID guard already satisfied this contract; the acceptance
fixture and witness establish it rather than claiming a newly repaired race.
This does not establish acoustic AEC, audio, reconnect or later lifecycle
acceptance.

### Native room audio preparation — Task 3.1 remains unaccepted

On 2026-10-28, the Linux session began retaining the existing `MicTrack` and
`SpeakerPlayout` resources on its original `RoomConnection`. Preparation runs
on the blocking pool with the existing native-peer timeout convention.
Microphone preparation is muted; generation and connection ownership are
checked before attachment. Leave mutes the retained track before closing the
room and releasing its media. The Linux startup handler now registers the
existing microphone commands rather than rejecting those available controls.

A bounded actual-Wayland run against the real owned backend and non-development
LiveKit initialized the native ADM, enumerated the physical Volt input and
Leviathan output, and published the microphone pre-muted on the joined room.
The read-only native mute state stayed true, the existing explicit mute command
reached the retained track, and normal visible Leave returned to room entry
with no retained membership. No synthetic media, second participant, new
receiver API or speech recording was used. The initial mute-command invocation
had rejected because Linux did not register that existing command; the same
runtime path passed after registration.

The first device-selection attempt did **not** select those named devices:
every Linux ADM device identifier was empty, so the existing preference
normalization treated the selection as system default. The passing
initialization smoke therefore proves neither explicit speaker selection nor
audibility. The returned default input name was `Volt 276 Analog Stereo`.
Keep the empty-identifier defect visible for the next ordered audio-state
task; do not relabel this narrower smoke as its recovery evidence.

The Linux desktop build passed; serial Rust library tests passed 1,332 with
two ignored; Clippy completed with existing warnings and changed-file rustfmt
completed. Task 3.1 remains TODO, with no completed living feature: a real
two-person speech check while the released Windows client's share updates is
still required. The existing Windows video-test VM has audio input and output
disabled; this work did not change that configuration. Speaker audibility,
microphone speech reception, acoustic AEC and later lifecycle scenarios remain
unverified.

### Native device identity and retry — bounded Task 3.2 evidence

The 2026-10-28 native diagnostic `scripts/check-linux-audio-devices.py` exercises
the existing room form, device menu and microphone controls against the owned
TLS backend and non-development LiveKit. It first saves unavailable device keys
through the normal device command. Joining preserves the room but shows
actionable microphone and speaker errors; it must not publish a substitute
microphone from another device. Selecting the available physical devices in the
visible picker then initializes the missing directions without reconnecting or
duplicating microphone publication.

The passing run keeps the microphone muted throughout, reads bounded OS
stream/graph metadata, and uses visible normal Leave. It does not enable speech
or read, record or save audio samples. The actual owned
PulseAudio stream requested the physical Volt input while the user's existing
EasyEffects configuration routed it through `easyeffects_source`; the actual
PipeWire upstream graph independently confirmed that physical input in the
processing chain. The diagnostic records requested target, effective source and
verified upstream path separately. It neither disables processing nor changes
global device defaults. Speaker initialization is not a claim of audible output.

This runtime check exposed and corrected the audio menu's unconditional camera
enumeration: Linux correctly rejects camera publication, but that rejection
previously prevented the unrelated audio lists from rendering. Camera and audio
menus now enumerate only their own direction. Returned live device errors also
remain visible in the compact device picker.

Linux keeps shared-ADM lifetime separate from speaker enablement: acquiring a
microphone or enumeration handle cannot enable an output rejected by the speaker
selection. Native device work retains its serialization permit until the actual
blocking operation exits, even after the async deadline; queued stale-generation
work is rejected before entering native switching. Android is excluded from the
desktop-Linux native-name identifier branches.

The bounded stale-selection recovery check passed with one room, one microphone
publication, continuous mute and normal Leave. It does not establish total physical
device absence or any planned speech/audibility assertion. Task 3.2's two complete
living scenarios therefore remain TODO. Task 3.1 and the five-minute acoustic
conversation remain TODO as well; Tommy explicitly authorized continuing
independent machine-verifiable work without a listening partner.

A later 2026-10-28 isolated-room microphone smoke reached the visible native
mute/unmute controls and independently queried the actual owned SFU. The room
had only the Linux participant and one audio publication: initially muted,
then unmuted, then muted again. Native capture count was one after unmute and
zero across six post-remute samples. An initial diagnostic incorrectly required
exactly one capture after remute; recording the actual lifecycle distinguished
normal capture release from a duplicate. Normal Leave cleared both real SFU
membership and all owned native audio streams.

Before console handoff, the exact Windows VM process's host capture/playback
clients were correlated to the existing EasyEffects source/sink, not identified
solely by a generic VirtualBox stream name. System defaults were unchanged.
Windows was initially at sign-in. Tommy subsequently signed in, and the console
was returned from his desktop by save/resume without a Windows reboot. Optional
post-sign-in setup was deferred without enrolling services. During the resumed
check, the guest stopped responding: VirtualBox reported a flatlined Guest
Additions heartbeat, the display and input stopped advancing, and a subsequent
non-destructive save-state request timed out after 90 seconds. The VM still
reported `running`. Its log opened the default ALSA output shortly before the
heartbeat failure, but this ordering does not establish the cause.

Tommy then authorized a forced restart. The frozen VM required a forced process
stop; another ALSA-backed boot reached a black screen and did not respond to
guest-managed reboot or poweroff requests. Changing only this test VM's host
audio backend to PulseAudio produced a responsive Windows sign-in surface with
both audio directions enabled. System audio defaults and 3D settings were not
changed. This establishes a working recovery configuration, not a proven root
cause for the ALSA failure.

Tommy confirmed that the existing file-backed credential worked manually. The
earlier entry code removed its trailing line ending and bypassed the shell.
Known non-secret letter/digit probes visibly preserved both exclamation marks.
A submission with freshly established focus, released modifiers, paced input
and a settling delay subsequently reached Windows Welcome and the desktop
using that same file. No real credential was revealed or recorded; the exact
cause of the earlier input failure remains unproven.

The resumed check joined the actual native Linux application and released Windows
client to the same owned room. Each normal unmute/remute control changed its real
SFU publication while the other client stayed muted. Linux also observed the
Windows microphone-state event and an uncorked owned playout stream on the selected
physical speaker. Windows' existing production watcher reported 1,000 stream
frames, zero audible frames and peak 21 below its floor of 64; that logged window
was marked muted. It does not prove audible delivery during the unmuted interval.

Normal Linux Leave released membership and every owned capture/playback stream.
The attempted Windows Leave click did not remove its membership; cleanup instead
deleted only the owned SFU room after confirming that no unknown peers were in it.
The room was verified absent. The Windows VM and persistent console stayed running.
No diagnostic recording or synthetic media was used. Audible speech, peer-observed
silence and acoustic echo cancellation remain unverified; no scenario was promoted
on this bounded evidence.

### Disconnected sharer retirement — Task 4.1

The complete `Retire a disconnected sharer presentation` scenario passed on
2026-10-28 with the released Windows native client sharing a real Notepad counter.
The native Linux witness observed three increasing counter values, then the
original room's actual participant-disconnected event after normal Windows Leave.
The peer's tile, share summary and independent GTK window stayed absent for three
seconds while Linux retained its original membership and main window. Normal
Linux Leave returned to room entry. The microphone stayed muted throughout;
this scenario makes no speech or audibility claim.

From the repository root, with the actual Wayland/session environment and the
released Windows peer already sharing its non-sensitive changing editor:

```sh
python3 apps/desktop/scripts/check-linux-sharer-disconnect.py \
  apps/desktop/src-tauri/target/debug/desktop \
  --backend-url "$PETAL_BACKEND_URL" \
  --room-code "$ROOM_CODE" \
  --expected-peer-name 'Petal Windows Test' \
  --evidence-dir "$PRIVATE_EVIDENCE_DIR" \
  --feature apps/desktop/features/linux-room-lifecycle.feature
```

The private evidence directory must not already exist. At
`WINDOWS_SHARER_LEAVE_REQUIRED`, use the Windows client's normal Leave control.
The witness requires the real departure event, not merely a stopped counter,
and closes its window-only pixel stream before destruction. It records the
retirement and normal Leave result without retaining room or participant IDs.
Task 4.1's separate stop/restart-with-two-way-audio scenario remains TODO.

The same complete witness rejects an isolated native build that ignores share
unpublication and participant-disconnect retirement: it observes the actual
departure but cannot establish remote-window removal. Normal Linux Leave still
cleans up that mutant. The candidate's complete positive run and a bounded basic
review both pass.

### Share stop/restart — bounded Task 4.1 evidence

The real released Windows peer stopped and restarted the same owned Notepad
share on 2026-10-28. Linux observed the old publication's unpublication and three
seconds without its summary or GTK window. The original room, participant
identities and main window remained. Restart produced a distinct publication
SID and one remote window showing `New share content` with increasing real
editor counters. There was one room connection and one continuously muted
microphone publication; normal Leave returned to room entry.

```sh
python3 apps/desktop/scripts/check-linux-share-restart.py \
  apps/desktop/src-tauri/target/debug/desktop \
  --backend-url "$PETAL_BACKEND_URL" \
  --room-code "$ROOM_CODE" \
  --expected-peer-name 'Petal Windows Test' \
  --evidence-dir "$PRIVATE_EVIDENCE_DIR"
```

Run the real editor helper with `-RestartPhaseFile` pointing to a fresh guest
file; absent means `initial`. At `WINDOWS_SHARE_STOP_REQUIRED`, stop the actual
owned source through the Windows picker's source card, then send
`WINDOWS_SHARE_STOPPED` and newline to the witness. The meeting's share button
controls picker visibility; opening it is not proof that capture stopped.
At `WINDOWS_SHARE_RESTART_REQUIRED`, write `restarted` to the editor phase file,
normally share that same editor again, then send `WINDOWS_SHARE_RESTARTED` and
newline. The witness requires actual old unpublication, a distinct new SID and
advancing new-content pixels. `share-restart.json` records the bounded result.

This diagnostic has no living-feature binding and does not establish continuing
audible two-way speech. The complete stop/restart scenario remains TODO.


### Normal leave/rejoin — bounded Task 4.2 evidence

A private runtime diagnostic passed two normal join/Leave cycles in the same
native Linux process on 2026-10-28. The released Windows peer kept sharing its
changing Notepad window throughout. Linux displayed three increasing counter
values on each join and retained the same main window and participant identity.
Host-only SFU administration confirmed exactly one muted microphone publication
per join and no Linux membership or publication after each Leave. OS metadata
showed one owned capture and playback stream while joined, then neither after
Leave; remote GTK windows also disappeared. No server credentials entered the
native client, and no audio samples were recorded.

This checks resource retirement and reuse, not audible speech. Task 4.2 and its
full living scenario remain TODO until two-way speech is verified. The diagnostic
does not bind a partial substitute scenario or claim physical audio acceptance.
Keep the unlocked desktop visible during native navigation: GNOME idle blanking
stalled the webview's route transition during preparation. A bounded
`gnome-session-inhibit --inhibit=idle` guard prevented that environmental failure.

### SDK reconnect — bounded Task 4.3 evidence

On 2026-10-28 Linux's original connect-time resilience receiver was attached to
the existing room-event watcher. Linux shares the SDK-event/toast bridge, not
macOS's OS monitors or sender publication repair. Terminal cleanup checks the
captured generation after acquiring the Linux room-transition lock.

A private real-stack diagnostic interrupted only Linux's actual signaling
socket: it briefly paused the owned TLS listener, closed the uniquely matched
owned connection child, then resumed the listener in a `finally` block. No host
network policy or LiveKit server configuration changed. The normal Linux UI
showed Reconnecting, then Reconnected. Released-Windows editor counters advanced
from 3270–3272 before the interruption to 3275–3277 afterward. Linux retained
the original membership and main window, one remote window, one muted SFU
microphone publication, and one owned capture/playback stream pair. Normal Leave
removed membership, publication, native audio streams and the remote window.

The full Task 4.3 scenario remains TODO: muted publication and OS ownership do
not establish resumed audible speech. No partial living scenario was created.
An independent source review found no demonstrated defects in the bounded
integration; this is not full milestone acceptance or merge authorization.

### Different-room retirement — bounded Task 4.4 evidence

A second private real-stack diagnostic used actual host-side SFU participant
removal. Linux's shared forced-disconnect cleanup retired membership, both owned
audio streams and the remote window, then rendered normal room-entry controls.
Linux normally joined a different owned room. After the released Windows peer
left the old room through its normal Leave control, host-side SFU inspection
confirmed that old room was empty. Twelve samples retained the new membership,
no old-room remote windows, and unchanged native audio ownership. Normal Leave
released the new room's audio resources. The microphone stayed muted throughout.
An empty room need not have an active playback stream; the diagnostic records
its actual capture/playback counts rather than requiring incoming audio there.

A disposable native build now also exercises the actual in-flight video-event
race. Its original SDK consumer holds one genuine Windows `TrackSubscribed`
event after the outer generation check, without cloning or constructing it.
Normal Linux Leave releases both old audio streams; normal Join enters a
different empty owned room. Only then is that same event delivered to the
original feed. The production handler rejects it with cancellation and stale
generation both true. Raw receive-slot state remains `0 -> 0`; twelve samples
retain the new room, no old windows, one native capture and no playback.
Normal final Leave releases the new room's audio resources.

The same-room control delivers its held real event while current: raw slots
change `0 -> 1` and real Windows counter pixels advance. In a separate disposable
guard-removal counterexample, the stale event actually inserts a slot (`0 -> 1`).
That is measured under the backing-map mutex, not inferred from a missing log or
from summaries that hide cancelled slots. Deeper guards still prevent a visible
mutant window, so this is not a full living-scenario mutation result.

A separate disposable run holds a genuine audio `TrackSubscribed` event in
Linux's original connect-time compositor-event consumer, immediately before
its existing generation check. This is the Linux path, not the independent
audio-track logger used on other desktop platforms. Normal Leave releases
the old capture/playback pair; normal Join enters the other room. Releasing
the same audio event then reaches the original stale-generation check and
is rejected. Twelve samples retain the new membership, no old windows,
one capture and zero playback streams. Final Leave releases all owned audio.
No additional SDK receiver or audio observer was started for this check.

The remaining user-visible mutation gate uses genuine decoded Windows pixels
held before the original decoder's latest-frame lock, never on the GTK thread.
The first frame has not opened a window when normal Leave and Join B finish.
Releasing it then reaches the original cancellation/generation check and is
rejected; twelve samples retain B with no old window or playback. A same-room
control releases its frame while current and renders advancing Windows counters.

For the disposable mutant, disable old-slot retirement and the redundant
generation/cancellation barriers at the frame handoff and GTK pump. Releasing
the held real pixels now opens an actual old-room GTK window while B remains
joined. The same `stale` verifier exits `1` with `Late old-room frame opened a
native remote window`; this is not merely a backing-map or missing-log oracle.
Restore all original barriers and rerun that identical verifier: exit `0`.

Task 4.4's living scenario is now locally verified for delayed application
events and frame work after the normal room switch. This does not claim new
SDK/network callbacks emitted after Leave, audible speech or AEC. No synthetic
event/media, additional SDK observer, canonical instrumentation or public
receiver test API was added. The separate obsolete-disconnect regression remains
narrower unit evidence, not live acceptance.

Rust verification for the canonical reconnect integration: `cargo test --locked --lib` passed 1,353 tests with two
ignored; `cargo clippy --locked --lib --bin desktop` and
`cargo build --locked --bin desktop` passed. Clippy reported 67 warnings.
The formatter check also exposed an existing macOS display-test assertion
layout outside this change; introduced formatting differences were corrected,
without restyling that unrelated assertion.


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

## Windows held-button retirement — authorized Linux-controller prerequisite

The November 3, 2026 repair addresses a released-Windows failure observed during
Linux controller acceptance: a queued pointer Up lost authority during teardown,
and its replacement release was cancelled. Keyboard cleanup alone left the
native mouse button held.

Windows now tracks actual successful mouse-button injection, including a partial
Click that submitted Down but not Up. Controller, window and global cleanup
release only recorded ownership through the serialized native input lane, without
moving the cursor or requiring the retired target/grant. Another controller's
ownership is retained, failed native releases remain retryable, and normal
input still requires current target and grant validation. Grant teardown closes
admission before native cleanup so a late press cannot follow the release.

The current Windows library-test artifact passed 25 platform tests in the
dedicated Petal Windows Test VM. The separately enabled
`windows_remote_control::tests::held_button_retirement_native_smoke` also passed:
an owned, hit-testable native window received Left, Middle and Right Down; each
was independently observed with `GetAsyncKeyState`, released by production
controller cleanup without an active grant or queued Up, and observed up again.
The fixture restores the guest cursor and destroys its own window. It is ignored
by default because it injects real input; run it only in that unlocked, dedicated
guest with no mouse button already held.

For a current Windows library-test executable assigned to `$WindowsTestExe`:

```powershell
& $WindowsTestExe 'windows_remote_control::tests::' --test-threads=1
& $WindowsTestExe 'windows_remote_control::tests::held_button_retirement_native_smoke' --exact --ignored --nocapture --test-threads=1
```

The cross-linked test artifact needs a Common Controls v6 activation manifest:
`/MANIFEST:EMBED` and
`/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'`.
Without it the test process failed before entry with `STATUS_ENTRYPOINT_NOT_FOUND`.
The repair was built using the existing Windows SDK and cross-toolchain.

Post-login paired checks used the actual corrected Windows application,
native Linux controller and owned Windows editor. Each watcher observed both
Shift and left-button Down before the real retirement, then observed both
released within 600 ms, below the 1,200 ms held-input expiry backstop:

| Actual retirement | Slower observed key/button release |
|---|---:|
| Windows disables control | 33 ms |
| Windows stops the source | 27 ms |
| Windows departs | 25 ms |
| Linux leaves | 124 ms |
| Transport disconnects | 156 ms |
| Share instance is replaced | 14 ms |

The native fixture also recorded matching Up events and no held input.
Fourteen production Linux admission regressions passed, covering late approval
after revocation/cancellation, share replacement, retirement and stale request
tasks. Actual direct input after cancellation was refused without changing the
Windows editor. The later genuine delayed-wire matrix above supersedes the
earlier sender-attribution blocker and establishes R03 locally. SFU `SendData`
cannot impersonate the original sender and was not used as its substitute.
That earlier artifact lacked exact-attempt correlation. The November 4 approved
repair and genuine A/B proof above supersede that protocol limitation locally.

The installed released Windows peer was not replaced. These local checks do
not establish Task 3.1 delivery, camera acceptance, conversation or acoustic AEC.

## Refs

`docs/TESTING.md` (how to run everything) · `internal/docs/COURSE_CORRECTION.md` (§4b
controls, §4c experiment choice, amended fast-probes rule) ·
`internal/docs/COCKPIT_RUNBOOK.md` · `internal/docs/COCKPIT_TEST_MAP.md` · issues #613 #618 #619
#620 #621 #622 #623 #624 #299 #79.

