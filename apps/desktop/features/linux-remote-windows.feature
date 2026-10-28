Feature: Native Linux remote windows

  Scenario Outline: Receive a movable and resizable native share from the Windows peer
    Given Linux and a native <peer> client are in hosted room abc-defg-hjk
    When the peer shares a non-sensitive editor window with a changing counter
    Then exactly one independent native remote window appears on Linux
    And its pixels show the editor and continuing counter changes
    When I move and resize the remote window using its desktop window controls
    Then the remote window moves and resizes independently
    And the main meeting window keeps its own position and size

    Examples:
      | peer    |
      | Windows |
