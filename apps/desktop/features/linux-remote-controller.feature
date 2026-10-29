Feature: Native Linux remote controller

  # R01 verified locally November 4: prior native window/screen pointer,
  # wheel and focus/occlusion confinement evidence is retained. The approved
  # temporary Windows repair now inserts one background character per GTK
  # key pair without stealing focus; foreground input remains functional.
  Scenario Outline: Linux controls an authorized Windows target
    Given Windows publishes a <target_kind> to the native Linux received-share window
    When Linux requests control and Windows explicitly grants the current target
    Then Linux pointer movement, buttons, scroll and keyboard act through that native picture window
    And <target_enforcement>
    Examples:
      | target_kind | target_enforcement |
      | window | Windows keeps input confined to the authorized window under focus change and occlusion |
      | screen | Windows retains its existing explicitly granted screen or desktop behavior |

  # R02: no unsuccessful request path ever enables input; visible results,
  # no remote input, expired requests need a fresh explicit request.
  # Prior genuine delayed decline/cancel/expiry rows are retained.
  # November 4: hold real approval A, cancel A, request B on the same share,
  # then release A's authenticated approval/stopped responses. B stays waiting
  # and sends no input until its own Allow; current host revoke still stops B.
  Scenario Outline: Unsuccessful requests never enable Linux controller input
    Given Linux requests control of a Windows share
    When the request is <request_result>
    And a late response for that retired request arrives
    Then Linux visibly reports the non-controlling result and sends no remote input
    And an expired request requires a fresh explicit request
    Examples:
      | request_result |
      | declined |
      | cancelled |
      | expired |

  # R03: every ending stops controller input, releases held input through
  # the existing protocol, and retired grants cannot be restored.
  # November 3: all six paired held-key/button releases passed (14–156 ms).
  # The final corrected Windows candidate also passed six genuine delayed-input
  # and six delayed-approval endings through a private namespace packet gate.
  # Meeting-wide revocation now sends stopped to the controller, not only locally.
  # Every accepted packet witness had zero automatic-accept counter deltas;
  # overflow/watchdog attempts were excluded. See docs/VALIDATION.md.
  Scenario Outline: Retire Linux controller authority and held input
    Given Linux controls a Windows share and holds a key and pointer button
    When <control_end>
    Then Linux stops controller input and releases held input through the existing protocol
    And old packets and responses cannot restore that grant
    Examples:
      | control_end |
      | Windows revokes control |
      | Windows stops the source |
      | Windows departs |
      | Linux leaves the room |
      | the transport disconnects |
      | the share instance is replaced |

  # R04: clipboard text exchange is separately authorized per direction,
  # bounded to 1 MiB UTF-8 without NUL, and every rejected transfer leaves
  # both clipboards untouched.
  # Verified November 3 against the corrected local Windows candidate and an
  # independent private Mutter clipboard: distinct exact-1-MiB Copy/Paste,
  # Unicode/multiline text and every rejection row. Both clipboards and the
  # Windows editor were checked; the human clipboard was never connected.
  Scenario Outline: Exchange separately authorized clipboard text as Linux controller
    Given Linux controls Windows through an active exact-current grant
    When copy and paste are each separately approved and transfer UTF-8 plain text of 1 MiB without NUL
    Then each intended clipboard receives exactly that text in its approved direction
    When <rejected_transfer> is attempted
    Then neither clipboard changes because of that rejected transfer
    Examples:
      | rejected_transfer |
      | copy without separate authorization |
      | paste without separate authorization |
      | text exceeding 1 MiB |
      | text containing NUL |
      | transfer naming a stale grant |
      | file transfer |
      | image transfer |
