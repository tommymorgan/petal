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

  Scenario: Preserve source aspect and readable text across size changes
    Given a peer shares an editor window containing "Petal Linux 12345" and four visible corner markers
    And Linux displays that share in its native remote window
    When I resize the remote window to a different aspect ratio
    Then the whole received image remains visible without cropping or stretching
    And any unused area is letterboxed
    When the peer changes the source window dimensions and changes the text to "Petal Linux 67890"
    Then Linux displays the new source aspect ratio without stretching
    And all four corner markers remain visible
    When I view the received image at one-to-one image scale
    Then "Petal Linux 67890" is readable rather than the previous text
