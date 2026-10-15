Feature: Native Linux receive-only meeting capabilities

  Scenario: Unsupported publishing and input actions are unavailable
    Given the native Linux app is running
    When I inspect its room controls and invoke each unsupported command directly
    Then each action in the following table is visibly unavailable with a Linux limitation explanation
    And each corresponding command returns an explicit unsupported error
    And none of the listed effects occurs
      | action                 | prohibited effect              |
      | publish a screen share | local screen media publication |
      | publish the camera     | local camera media publication |
      | request remote control | control request sent to peer   |
      | grant remote control   | local input injection enabled  |
      | send remote input      | input event sent to peer       |
      | replay remote input    | local OS input injected        |
    And room joining, microphone controls, and receiving a share remain available
    And no unsupported command can publish local screen or camera media or inject input
