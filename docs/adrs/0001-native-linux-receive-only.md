# Native Linux receive-only milestone

Tommy accepted this boundary in the 2026-10-14 planning interview: a native Linux two-person call on this computer, in normal hosted Petal rooms, with speakers and microphone and one incoming share in an independent desktop window. Both macOS and Windows native peers are acceptance targets. A browser substitute and headset-only proof do not satisfy the outcome.

Outgoing screen sharing, local camera publication, and remote control are explicitly out of scope for this milestone. This is the complete plan for that bounded outcome, not an incomplete first phase of a larger PRD. General Linux distribution support, installers, automatic updates, hover-to-share, and additional simultaneous-share guarantees are not acceptance promises; retain unsupported behavior honestly rather than presenting successful no-ops.

The target is the inspected x86_64 Arch-family Wayland desktop and its actual audio devices. A local executable is sufficient; do not require hosted service changes or changes to other participants' released clients. Hosted endpoint configuration must be explicit, preserving the repository's no-implicit-backend rule and keeping server secrets off clients.

Number 0001 was checked against the latest fetched remote default branch (`origin/main`, `7782718c0cc498615099f799533b67701665caee`); `docs/adrs/` had no ADRs there. Recheck numbering at PR time.
