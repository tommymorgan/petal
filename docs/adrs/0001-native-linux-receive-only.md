# Native Linux receive-only milestone

Tommy accepted this boundary in the 2026-10-14 planning interview: a native Linux two-person call on this computer, in normal hosted Petal rooms, with speakers and microphone and one incoming share in an independent desktop window. On 2026-10-27 he selected the released Windows native peer instead of macOS for acceptance. A browser substitute and headset-only proof do not satisfy the outcome.

Outgoing screen sharing, local camera publication, and remote control are explicitly out of scope for this milestone. This is the complete plan for that bounded outcome, not an incomplete first phase of a larger PRD. General Linux distribution support, installers, automatic updates, hover-to-share, and additional simultaneous-share guarantees are not acceptance promises; retain unsupported behavior honestly rather than presenting successful no-ops.

The target is the inspected x86_64 Arch-family Wayland desktop and its actual audio devices. A local executable is sufficient; do not require hosted service changes or changes to other participants' released clients. Hosted endpoint configuration must be explicit, preserving the repository's no-implicit-backend rule and keeping server secrets off clients.

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
