Feature: Native Linux room lifecycle

  Scenario: Retire a disconnected sharer presentation
    Given Linux is in a room displaying a participant's active share in a remote window
    When that sharer disconnects from the room
    Then that participant's remote window is removed
    And no frozen window is presented as that participant's live share
    When I leave the room
    Then Linux returns to its room-entry state normally

  Scenario: Old room events cannot revive media in a new room
    Given Linux has left room abc-defg-hjk and joined a different room def-ghjk-mno
    When late events from room abc-defg-hjk arrive after that room switch
    Then no room abc-defg-hjk remote window opens
    And no room abc-defg-hjk audio restarts
    And Linux remains in room def-ghjk-mno
