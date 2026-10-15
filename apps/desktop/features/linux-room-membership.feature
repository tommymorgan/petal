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
