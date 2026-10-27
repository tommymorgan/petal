Feature: Native Linux room membership

  Scenario: Launch the Linux app in the graphical session
    Given the locally built Linux executable is available on this computer
    And this computer is running its actual Wayland desktop session
    When I launch that executable from the documented Linux command
    Then Petal opens its native room-entry window
    And no browser is opened
    And neither preparing nor launching that executable requires macOS or Windows setup steps

  Scenario Outline: Join a hosted room with the Windows native peer
    Given a released native Petal <peer> client is in hosted room abc-defg-hjk
    And Linux is configured explicitly for that same hosted Petal service
    And no LiveKit server API secret is present or required on the Linux client
    When I enter room code abc-defg-hjk in the native Linux app and join
    Then Linux joins hosted room abc-defg-hjk without changing the peer client's server
    And Linux and the <peer> client each show the other participant in the room

    Examples:
      | peer    |
      | Windows |

  Scenario Outline: Recover from an invalid or refused hosted join
    Given Linux is configured for the intended hosted Petal backend
    And joining <attempted input> produces <failure>
    When I attempt to join using <attempted input>
    Then I see an actionable join error
    And Linux remains outside the room without microphone publication or a remote window
    When I correct the room code to accessible hosted room abc-defg-hjk and join
    Then Linux joins room abc-defg-hjk normally

    Examples:
      | attempted input     | failure                 |
      | petal://join/000000 | an invalid code         |
      | def-ghjk-mno        | a refused access result |

  Scenario: Missing hosted configuration never falls back silently
    Given I launch a Linux build with no baked hosted backend and no runtime backend override
    And no LiveKit server API secret is present on the client
    When I attempt a hosted room join
    Then I see a configuration error naming the missing hosted backend setting
    And Linux does not attempt to join a local server or an alternative backend
    And the app does not ask me to install a LiveKit server API secret
    When I configure only the intended hosted backend URL and restart the app without a server API secret
    And I join accessible hosted room abc-defg-hjk
    Then Linux joins successfully without any client dependency on a LiveKit server API secret
