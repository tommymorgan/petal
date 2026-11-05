Feature: Explicit Linux desktop control and its authority boundaries
  Living record of the desktop-control scenarios actually verified on this
  GNOME Wayland computer against the owned temporary Windows peer on
  November 6, 2026. Partially verified and unverified rows stay visible in
  plans/2026-10-28-native-linux-publishing-control.md Task 4.1.

  Scenario: Linux window shares cannot grant desktop input
    Given Linux shares only the owned editor application window with Windows
    When Windows opens the real receiver and the host invokes preparation and per-share enable commands directly
    Then the receiver's Control action is disabled and explains that application-window shares cannot receive remote input
    And the receiver advertises view-only metadata while actually decoding the shared picture
    And direct preparation is rejected with the view-only explanation before any portal request
    And no input session, desktop grant or injected input results

  Scenario: Disclose desktop-wide scope before and during control
    Given Linux has an ordinary picture-only monitor share and no input grant
    When desktop control is prepared and a named Windows controller is granted
    Then a real native disclosure precedes the fresh combined source selection
    And the active state names the controller, repeats the desktop-wide keyboard and focus scope and offers an obvious stop
    And stopping control revokes the controller first and ends only the controlled share

  Scenario: Stopping control preserves unrelated shares and media
    Given Linux publishes a controlled monitor share plus an editor window share, a real camera and an unmuted microphone
    When control is stopped through the real stop action
    Then the input authority retires and the controlled monitor share ends
    And the editor share survives, camera capture continues without interruption and the microphone stays live
    And replay against the retired share is rejected
    And sharing again requires fresh native source consent and preparation

  # Verified live on 2026-11-06 at DP-1 150 percent scale: absolute pointer
  # mapping to all four corners and center, evdev keyboard with shift pairs,
  # proportional scroll, malformed packets (null and out-of-range both sides)
  # rejected with logged reasons and zero injection, refusal returning to the
  # prepared state with the picture preserved, missing keyboard/pointer
  # permission failing closed with the exact error while the picture share
  # survived, unclaimed prepared states and quiet active grants expiring
  # fail-closed within roughly five minutes, and held input auto-released by
  # the designed ~1.3 second TTL.

  Scenario: Separately approved clipboard text transfers in both directions
    Given Windows controls a prepared Linux desktop through an active grant
    When the controller requests copy and, separately, paste with the host
    approving each operation through its own dialog
    Then exactly 1048576 bytes of verified synthetic text reach the controller
    on copy and the host on paste, with content hashes matching on both sides
    And a request against a retired grant is refused before anything is sent

  # Verified live 2026-11-06 after three fixes (host-owned clipboard scope
  # admission with logged rejections, fully async bounded consent dialogs,
  # uninitialized-clipboard sequence handling): copy and paste each moved
  # exactly 1 MiB of known synthetic content with per-operation human consent.
  # Two earlier approved copies transferred the human's own freshly-copied
  # content (26 and 23 bytes) after normal desktop use replaced the synthetic;
  # both were redacted by the observing agent and the guest temp was deleted.
  # Not exercised live: the oversize/NUL/file/image rejection rows (unit-proven;
  # the observing agent's own automation gate refused the post-retirement
  # attempt before any product IPC, so that refusal is not product evidence —
  # the host-side post-retirement rejection logged earlier is).
  # Not verified live: request expiry at the consent dialog, resize and other
  # monitor mapping rows, portal revocation, source loss, controller
  # departure, room leave, disconnect, EIS-loss teardown, reconnect
  # no-restore, wrong-controller packets, and the oversize/NUL/file/image
  # clipboard rejection rows. Composition/Text input is dropped host-side by
  # design, so arbitrary Unicode text entry is unsupported while keymap keys
  # work.
