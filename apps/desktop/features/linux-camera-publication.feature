Feature: Native Linux camera publication

  # C01: real Camera portal grant (auto-grant desktops fall back to the
  # session daemon per ADR 0003 deviation), production publisher, released
  # Windows peer, native self-view window.
  Scenario: Turn on native camera video with matching self-view
    Given Linux camera is off in a room with the released Windows peer
    When I explicitly turn camera on and approve native permission for camera Cam-A
    And I move a non-sensitive marker in front of Cam-A
    Then Windows receives its changing camera picture
    And Linux shows the matching native self-view without a browser acquisition

  # C02 DONE November 5: one unchanged capture feeds preview/publication.
  # Tommy observed LED ON with both, ON with publication off/preview retained,
  # then OFF after the actual last native preview closed; capture links zero.
  Scenario: Share one camera owner between preview and publication
    Given the visible Linux settings preview displays Cam-A
    When I turn camera publication on
    Then the same selected capture feeds the preview, self-view and outgoing camera
    And no second device acquisition occurs
    When I turn publication off
    Then Windows camera video stops while the visible settings preview is identified as still using Cam-A
    When I close that last preview
    Then Cam-A capture and its device indicator stop

  # C03 locally verified November 4: real private Camera portal denial,
  # actual pending-request cancellation through production Off, and zero
  # private camera sources. All show the corresponding native state with
  # no SFU/Windows video or capture links. Presence alone reported
  # not-determined, never the non-Linux authorized default.
  Scenario Outline: Show accurate camera permission or absence state
    Given Linux camera is off
    When I request camera and <permission_result>
    Then Linux shows <visible_state> without a camera publication
    And a presence flag or platform permission default cannot show camera permission as granted
    Examples:
      | permission_result | visible_state |
      | permission is denied | permission denied |
      | permission is cancelled | camera off |
      | no camera is available | no camera available |

  # C04: latest-selection-wins across devices and modes.
  # Combined actual Settings/native GTK self-view/released Windows check passes:
  # genuine saved A completion/frame replay is rejected after B becomes active.
  # GTK widget snapshots and decoded B observations span replay; no OS-late-open
  # or monitor-scanout claim. Native mode queries now serialize their shared slot.
  Scenario: Latest camera device and mode selection wins
    Given native camera permission is granted and the capture backend exposes Cam-A and Cam-B with their supported modes
    When I open the device selector and each device's mode selector
    Then Settings lists those available devices and each device's actual supported modes for selection
    When I select Cam-A at its advertised 1280 by 720 mode then quickly select Cam-B at its advertised 640 by 480 mode
    And the older Cam-A start and frame completions arrive after Cam-B is active
    Then Cam-B at the selected supported mode remains selected in settings, self-view and outgoing video
    And older device or mode frames cannot replace it

  # C05 DONE: actual C920 node-ACL loss, repaired capture-stream failure and
  # attended physical unplug/replug all preserve DP-1/unmuted Volt276 and
  # released Windows share decoding/audio reception. Restoration/replug never
  # auto-enables camera; explicit On recovers video. Permission loss uses real
  # owned-client node ACLs, not ineffective empty-app-id PermissionStore NO.
  Scenario Outline: Retire a failed camera without changing other media
    Given Linux publishes Cam-A, DP-1 and an unmuted microphone
    When Cam-A encounters <camera_failure>
    Then its camera publication stops and Linux explains the failure
    And no other camera is selected and DP-1 and microphone keep their states
    When I resolve the failure and explicitly turn the selected camera on again
    Then camera publication can recover
    Examples:
      | camera_failure |
      | device unplug |
      | capture failure |
      | permission loss |

  # C07: retired ownership cannot return through late completions.
  Scenario Outline: Retired camera ownership cannot return
    Given a Linux room-owned camera start is delayed
    When I <camera_retirement>
    And that old start or reconnect completion later reports success
    Then no retired outgoing camera track or room self-view returns
    And capture has no owner unless a current publication or explicitly visible settings preview still needs it
    Examples:
      | camera_retirement |
      | turn camera publication off |
      | leave the room |
