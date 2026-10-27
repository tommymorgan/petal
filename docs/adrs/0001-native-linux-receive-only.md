# Native Linux receive-only milestone

Tommy accepted this boundary in the 2026-10-14 planning interview: a native Linux two-person call on this computer with speakers and microphone and one incoming share in an independent desktop window. On 2026-10-27 he selected the released Windows native peer instead of macOS, then selected “let's self-host for acceptance,” replacing the original upstream-hosted-room requirement with a real Tommy-owned stateless Petal backend and LiveKit service. A browser substitute, synthetic peer, mock/dev-server media path, processing flag, or headset-only proof does not satisfy the outcome.

Outgoing screen sharing, local camera publication, and remote control are explicitly out of scope for this milestone. This is the complete plan for that bounded outcome, not an incomplete first phase of a larger PRD. General Linux distribution support, installers, automatic updates, hover-to-share, and additional simultaneous-share guarantees are not acceptance promises; retain unsupported behavior honestly rather than presenting successful no-ops.

The target is the inspected x86_64 Arch-family Wayland desktop and its actual audio devices. A local Linux executable and the released Windows native client explicitly configured for the same owned service are sufficient client artifacts; no changes to the peer's released client or upstream-hosted deployment are required. The owned acceptance backend and LiveKit service are running and reachable from both clients under the bounded placement below. The self-hosting selection removes the upstream operator-access dependency; it does not authorize paid signup or arbitrary public firewall exposure. Endpoint configuration remains explicit, preserving the repository's no-implicit-backend rule. Server API secrets stay on the backend/LiveKit service, never on either client.

Previously completed scenarios and upstream-hosted runtime observations retain their original status and environment. They are historical implementation evidence, not owned-stack acceptance proof. Separate 2026-10-27 owned-stack checks now establish trusted TLS, Windows and Linux mutual membership, actual refused-identity recovery, malformed-invite correction, and same-profile URL-only configuration recovery. Native audio and incoming-share acceptance remain pending.

## Acceptance service placement

The bounded acceptance stack runs on this workstation: the unmodified `backend/api/` handlers use Vercel's local handler runtime, and the official LiveKit 1.13.11 standalone binary runs with generated server-only credentials and a non-dev configuration. Vercel's local runtime is a handler host, not a replacement token service; clients still use the real backend and LiveKit path with a non-empty backend URL.

Backend HTTP and LiveKit signaling bind to loopback. Client-facing TLS listeners bind only to the workstation's tailnet address, serving `junkman.tailb16ad4.ts.net` on dedicated acceptance ports; LiveKit advertises the workstation's reachable LAN address for media. No public firewall, router, DNS, or unrelated service configuration changes were needed. Trusted TLS, Windows reachability and native room membership were exercised independently of initial loopback readiness. These checks do not establish media acceptance.


## Command and control boundary

Linux's receive-only admission rejects unsupported publishing/control commands
before argument decoding or dispatch. An unknown-command error is not the
contract: each known unsupported entrypoint reports its Linux limitation.
The existing Gallery, pill and menubar controls use the same platform UI
classification; unavailable actions stay visible and disabled with an
explanation. Microphone/device controls and room entry are not Linux-disabled.

- Screen publication: `share_window`, `toggle_window_share`,
  `toggle_region_share`, picker/region-opening alternatives, and screen-audio
  enable commands are refused before capture, window creation or publication.
- Camera publication: `start_camera_publish_command` is refused at IPC, and
  the shared camera-intent/device-start seams refuse before intent changes,
  capture, self-heal or publication.
- Control requests and sending: `remote_control_send` (request, pointer, key,
  wheel and text drafts), `remote_control_set_active`, escalation requests and
  remote clipboard input commands are refused.
- Grants: meeting/share policy setters, consent/escalation approval,
  share-control mode changes and AI-control approval/resume are refused.
- Replay has no public replay IPC command. The Linux receiver refuses before
  subscribing or starting replay/clipboard workers; message admission also
  refuses before grants or replay queueing. The Linux input backend returns
  the same explicit unsupported limitation for direct replay calls.

These are capability-admission guarantees, not successful hosted membership,
microphone/speaker operation or rendered share acceptance. The Linux startup
session remains disconnected until the ordered connection/media tasks supply
those implementations. No successful no-op command stands in for them, and no
browser gallery participant substitutes for a native Linux receiver. Existing
macOS/Windows handlers, policy and media behavior remain unchanged.

Number 0001 was checked against the latest fetched remote default branch (`origin/main`, `7782718c0cc498615099f799533b67701665caee`); `docs/adrs/` had no ADRs there. Recheck numbering at PR time.
