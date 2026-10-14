Feature: Native Linux room membership

  Scenario: Launch the Linux app in the graphical session
    Given the locally built Linux executable is available on this computer
    And this computer is running its actual Wayland desktop session
    When I launch that executable from the documented Linux command
    Then Petal opens its native room-entry window
    And no browser is opened
    And neither preparing nor launching that executable requires macOS or Windows setup steps
