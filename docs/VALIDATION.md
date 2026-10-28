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

Monitor publication additionally requires the GNOME ScreenCast portal on the
current user session bus, PipeWire, GStreamer 1.28 or newer, and the
`pipewiresrc`, `videoconvert`, `videoflip`, `videorate`, `capsfilter` and `appsink`
elements. The Linux-target Rust bindings use the existing GLib 0.18 family;
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
portal protocol test also rejects actual private-bus Cancel, Deny and unknown
responses emitted before the method reply. Treating Deny as success failed its
negative-response assertion; restoring that branch passed. No global portal
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

## Refs

`docs/TESTING.md` (how to run everything) · `internal/docs/COURSE_CORRECTION.md` (§4b
controls, §4c experiment choice, amended fast-probes rule) ·
`internal/docs/COCKPIT_RUNBOOK.md` · `internal/docs/COCKPIT_TEST_MAP.md` · issues #613 #618 #619
#620 #621 #622 #623 #624 #299 #79.
