Feature: Native Linux publication and input capability boundaries

  Scenario: Monitor admission and unfinished Linux capabilities are explicit
    Given the native Linux app is running
    When I inspect its room controls and invoke unfinished commands directly
    Then the monitor Share action is visibly enabled
    And monitor selection without a room reports the missing room before requesting permission
    And camera controls are enabled while region and input controls remain unavailable with Linux explanations
    And unfinished region, audio and input commands report unsupported while the camera command reports the missing room, and raw window IDs require native consent
    And no room, picture or camera publication is created
    And room-entry and microphone controls remain available
    And repeated unfinished actions leave the native app without publication or input effects

  # X04: --prerequisites uses a private empty registry and watches actual activation calls.
  # Documentation scope is also reviewed against docs/VALIDATION.md.
  Scenario: Report Linux native prerequisites without activating devices
    Given the documented Linux launch environment for this GNOME Wayland computer
    When I run the documented prerequisite check with a required native component unavailable
    Then it identifies the missing portal, PipeWire, capture/camera or input component and its effect
    And the check neither activates screen/camera/input nor installs software
    And documentation does not promise full cross-distro support

  # X03: run_x03 reuses the real Windows-pixel witness in an already joined native app.
  Scenario: UI and direct commands expose only completed capability paths
    Given this implementation unit's Linux executable is running
    When I inspect each publication and input action and invoke its direct command
    Then completed capabilities use their real native path with explicit per-operation permission state
    And incomplete camera or input capabilities remain unavailable without effects
    And missing permission or service is reported as a visible failure rather than success
