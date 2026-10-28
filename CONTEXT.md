# Petal meetings

Petal lets participants talk in a room and view another participant's shared application content as independent desktop windows.

## Language

**Room**:
A meeting space identified by a room code, which participants join and leave.
_Avoid_: call instance (a room can outlive one call).

**Participant**:
A person represented by one meeting identity in a room.
_Avoid_: window owner when referring to everyone in the room.

**Native peer**:
A participant using the Petal desktop application rather than its browser client.
_Avoid_: browser peer when claiming native interoperability.

**Share**:
A participant's published view of an application window or display.
_Avoid_: camera stream, remote window (the share and its presentation are different things).

**Sharer**:
The participant publishing a share.
_Avoid_: host (joining or creating a room does not imply sharing).

**Receiver**:
A participant viewing a share published by another participant.
_Avoid_: viewer-only participant (a receiver may still speak).

**Remote window**:
An independent desktop window presenting a received share.
_Avoid_: gallery tile, browser tab.

**Receive-only milestone**:
Linux receives a share but does not publish its own screen or camera or grant remote control; microphone publication remains enabled.
_Avoid_: listen-only, muted, read-only room.

**Camera publication**:
A participant's outgoing camera video, separate from screen/window shares and their count.
_Avoid_: camera share when a remotely controllable screen/window share is meant.

**Desktop-control preparation**:
A sharer's explicit setup of a desktop input session and its monitor picture, with no participant yet authorized to send input.
_Avoid_: granted control before a named controller is approved.

**Desktop-control grant**:
Explicit permission for a named controller to send desktop-wide input for a current share instance. The displayed monitor identifies the picture and pointer mapping; it does not confine keyboard focus to that monitor.
_Avoid_: window control or monitor-confined control for the Linux desktop-wide grant.
