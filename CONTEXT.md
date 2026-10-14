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
