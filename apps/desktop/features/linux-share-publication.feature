Feature: Native Linux picture publication

  # S01: real GNOME Wayland portal, production Linux publisher and released Windows peer.
  Scenario: Publish a selected monitor to Windows
    Given Linux and the released Windows peer are in room abc-defg-hjk on the owned service
    When I select monitor DP-1 through the native Share action and change its visible counter from 10 to 11
    Then Windows shows that selected monitor and the changing counter in an independent share window
    When I stop that share
    Then its publication and Windows remote window close

  # S05: native commands and the original participant's independent SFU audio state.
  Scenario Outline: Share actions preserve independent audio state
    Given Linux has its microphone <mute_state> and no computer-audio publication
    When I <share_action> a picture share
    Then the microphone remains <mute_state>
    And no computer, system or application audio track is published
    Examples:
      | mute_state | share_action |
      | muted | start |
      | unmuted | start |
      | muted | change |
      | unmuted | change |
      | muted | stop |
      | unmuted | stop |

  # S08: actual first-frame admission delayed under an owned non-stop debugger.
  # Every row rejects captured A after retirement; Stop also delays A's start
  # and stop completion until after B publishes. Room transitions serialize cleanup.
  Scenario Outline: Retired share work cannot change the current generation
    Given a Linux monitor start from generation A is delayed
    When I <retire_action> and explicitly start the current share generation B where applicable
    And the old A start, frame and stop completions arrive late
    Then A cannot create a publication or remove or overwrite B
    And recovery selects no new content and restores no remote-control grant
    Examples:
      | retire_action |
      | stop the pending share |
      | leave the room |
      | replace the selected source |
      | disconnect and reconnect |

  # S10: real owned-process portal/registry/remote/configuration failures;
  # original and explicit-retry publications each rendered fresh Windows counters.
  Scenario Outline: Report a failed picture attempt and permit explicit retry
    Given Linux already publishes DP-1 to Windows
    When a new share attempt encounters <failure>
    Then Linux shows the failure and releases that attempt's capture, portal and publication resources
    And DP-1 continues unchanged without a browser or X11 fallback
    When I resolve the failure and explicitly retry
    Then the selected new picture can be published
    Examples:
      | failure |
      | ScreenCast portal unavailable |
      | required capture plugin missing |
      | capture failure before first frame |
      | native publication rejected |
