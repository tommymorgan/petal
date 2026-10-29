Feature: Native Linux picture publication

  # S01: real GNOME Wayland portal, production Linux publisher and released Windows peer.
  Scenario: Publish a selected monitor to Windows
    Given Linux and the released Windows peer are in room abc-defg-hjk on the owned service
    When I select monitor DP-1 through the native Share action and change its visible counter from 10 to 11
    Then Windows shows that selected monitor and the changing counter in an independent share window
    When I stop that share
    Then its publication and Windows remote window close

  # S02: real owned GTK sources, native window consent and released Windows pixels.
  # Connected-only refresh mutation failed the first Windows counter; the original passed again.
  Scenario: Share only the selected application window
    Given Linux is in room abc-defg-hjk with Windows
    And the editor window shows counter 20 while an unrelated desktop window shows private counter 99
    When I select only the editor window through Add share and change its counter to 21
    Then Windows receives the editor picture and its changing counter
    And neither the unrelated window nor the rest of the desktop is in that publication

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

  # S09: 100% -> 150% scale ended the monitor's native capture authority;
  # explicit Share again + fresh native DP-1 selection published a new share
  # instance (fresh SID) with counter 62 while the editor share kept its SID
  # and the microphone stayed muted.
  Scenario: Recover explicitly when geometry changes end capture authority
    Given Linux publishes DP-1 and an unrelated editor window to Windows with the microphone muted
    When GNOME ends DP-1's capture authority during a display geometry change
    Then only DP-1's publication, remote window and associated grant retire
    And Linux explains that capture access ended and offers an explicit action to share again
    And no replacement source or publication starts automatically
    And the editor share and microphone mute state remain unchanged
    When I explicitly choose to share again
    Then the native source picker opens and no new picture publishes before source selection
    When I explicitly select DP-1 in that picker
    Then Windows receives its current corner grid, readable counter and correctly placed moving cursor in a new share instance
    And no retired frame or input authority is restored
    And the editor share and microphone mute state remain unchanged

  # S06: real native picker Cancel with DP-1 already publishing counter 91;
  # nothing published, DP-1 SID unchanged, counter 92 rendered after, no
  # substitution. Deny is an accepted platform limit (ADR 0003): GNOME 51
  # offers no distinct Deny control.
  Scenario Outline: Share permission never selects content implicitly
    Given Linux is already publishing monitor DP-1
    When I open Add share and cancel the native permission flow
    Then no new picture is published and DP-1 remains unchanged
    And no other source is substituted

  # S07: closing the shared editor window retired only its publication
  # while the unrelated monitor share kept its SID and rendered counter 111.
  # Disconnect and scoped revocation are accepted platform limits (ADR 0003).
  Scenario Outline: Retire a lost shared source without substitution
    Given Linux publishes the editor window and an unrelated picture share
    And the source has no input authority
    When the editor window closes
    Then its publication, Windows remote window and associated grant retire
    And the unrelated share continues without source substitution

  # S09: window resize 1280x720 -> 960x960 with all four corners, fresh
  # counters, correct cursor placement and the same publication SID while
  # authority stayed valid.
  Scenario Outline: Preserve the selected picture through geometry changes
    Given Linux publishes the editor window containing a corner grid, readable text and a moving cursor to Windows
    And its native capture authority remains valid through the geometry change
    And an unrelated picture share and the independently muted microphone are active
    When I resize the editor from 1280 by 720 to 960 by 960
    Then Windows shows the current selected picture with correct aspect, orientation, readable text and cursor placement
    And it shows no frame from a retired source
    And the unrelated share and microphone state remain unchanged
