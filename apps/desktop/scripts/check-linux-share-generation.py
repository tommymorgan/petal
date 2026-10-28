#!/usr/bin/env python3
"""S08 native pending-start retirement prefixes, with truthful scheduling limits.

A is delayed by leaving its real permission chooser unanswered. Each row drives
its actual native stop/leave/replacement/disconnection path, then explicitly
approves B and observes B through the actual SFU and Windows pixels. This does
not manufacture late frames: an unapproved A has captured none. Completion times
are recorded, not relabeled as deliberately late callbacks.

Use the retained, explicitly authorized native app. This module never launches
another participant, changes service settings, activates input or unmutes audio.
"""
import importlib.util
import json
import math
from pathlib import Path
import re
import time

_spec = importlib.util.spec_from_file_location(
    'generation_monitor_witness', Path(__file__).with_name('check-linux-monitor-publication.py'))
monitor = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(monitor)
audio = monitor.audio

RETIRE_ACTIONS = (
    'stop the pending share', 'leave the room', 'replace the selected source',
    'disconnect and reconnect',
)


def begin(app, key, command):
    audio.evaluate(app, """(() => {
        const result = {startedAt: performance.now(), result: null};
        window.__linuxShareGeneration[%s] = result;
        window.__TAURI_INTERNALS__.invoke(%s, {}).then(
            value => { result.result = {resolved: true}; result.finishedAt = performance.now(); },
            error => { result.result = {rejected: String(error)}; result.finishedAt = performance.now(); });
        return true;
    })()""" % (json.dumps(key), json.dumps(command)))


def result(app, key):
    return audio.evaluate(app, 'window.__linuxShareGeneration[%s]' % json.dumps(key))


def await_result(app, key, timeout):
    deadline = time.monotonic() + timeout
    while True:
        observed = result(app, key)
        if observed['result'] is not None:
            return observed
        assert time.monotonic() < deadline, key + ': actual native command did not settle'
        time.sleep(.1)


def run_s08_pending_start(
    app, observe_participant, approve_share, wait_permission, cancel_permission,
    retire_action, windows_vm, evidence_dir, *, rejoin_room=None,
    disconnect_participant=None, timeout=90,
):
    """Run one exact outlined native action; on success leave B joined/muted/live.

    wait_permission(label) observes the real chooser without approving it.
    approve_share/cancel_permission operate only that owned chooser. Rejoin must
    use normal native room entry, and disconnect_participant must remove only
    this owned original participant through the authorized SFU control path.
    No callback may seed session state or synthesize a successful permission.
    """
    assert retire_action in RETIRE_ACTIONS
    assert math.isfinite(timeout) and timeout >= 20
    if retire_action in ('leave the room', 'disconnect and reconnect'):
        assert rejoin_room is not None, 'This row requires actual normal native room re-entry'
    if retire_action == 'disconnect and reconnect':
        assert disconnect_participant is not None, 'This row requires an owned real SFU disconnection'
    evidence_dir = Path(evidence_dir)
    evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    room = audio.invoke(app, 'current_room')
    assert room is not None, 'Begin S08 in the retained real room'
    mic = audio.wait_microphone(observe_participant)
    assert mic.get('muted', False), 'S08 requires an already-muted real microphone'
    witness = audio.AudioWitness(app, observe_participant, True, mic['sid'])
    witness.settled(False, timeout=timeout)
    audio.evaluate(app, 'window.__linuxShareGeneration = {}; true')
    succeeded = False
    try:
        begin(app, 'A', 'open_window_picker_window')
        wait_permission('S08-A-delayed')
        assert result(app, 'A')['result'] is None, 'A must remain genuinely delayed in native permission'
        witness.settled(False, timeout=timeout)
        if retire_action == 'replace the selected source':
            # A new explicit source-selection intent must supersede the old
            # pending selection, not require the test to cancel A on its behalf.
            begin(app, 'B', 'open_window_picker_window')
            deadline = time.monotonic() + timeout
            while result(app, 'A')['result'] is None:
                witness.sample()
                replacement = result(app, 'B')['result']
                assert replacement is None, \
                    'Explicit source replacement rejected while old generation A was pending'
                assert audio.invoke(app, 'shared_window_ids') == [], 'Unapproved A created a publication'
                assert time.monotonic() < deadline, 'Explicit replacement did not retire old pending A'
                time.sleep(.1)
        else:
            if retire_action == 'stop the pending share':
                begin(app, 'retire', 'stop_all_shares')
            elif retire_action == 'leave the room':
                begin(app, 'retire', 'leave_room_command')
            else:
                disconnect_participant('S08-owned-participant-disconnect')
            if retire_action != 'disconnect and reconnect':
                retired = await_result(app, 'retire', timeout)
                assert retired['result'].get('resolved'), 'Actual native retirement command rejected'
            if retire_action != 'stop the pending share':
                deadline = time.monotonic() + timeout
                while audio.invoke(app, 'current_room') is not None:
                    assert time.monotonic() < deadline, 'Real room generation did not retire'
                    time.sleep(.1)
                assert audio.invoke(app, 'shared_window_ids') == [], 'Room retirement retained an active share'
                rejoin_room('S08-explicit-normal-rejoin')
                assert audio.invoke(app, 'current_room') == room, 'Re-entry did not return to the same owned room'
                # A new room correctly has a new microphone publication. Do not
                # mistake that intended lifecycle for a duplicated original mic.
                mic = audio.wait_microphone(observe_participant)
                assert mic.get('muted', False), 'Explicit re-entry unexpectedly unmuted capture'
                witness = audio.AudioWitness(app, observe_participant, True, mic['sid'])
            witness.settled(False, timeout=timeout)
            begin(app, 'B', 'open_window_picker_window')
        old = await_result(app, 'A', timeout)
        assert old['result'].get('rejected'), 'Retired, unapproved A must not resolve as published content'
        # A chooser must not silently reselect itself through recovery.
        assert audio.invoke(app, 'shared_window_ids') == [], 'A/recovery published content before B consent'
        assert result(app, 'B')['result'] is None, 'B settled before its explicit native consent'
        wait_permission('S08-B-current')
        approve_share('S08-B-current')
        current = await_result(app, 'B', timeout)
        assert current['result'].get('resolved'), 'Current generation B did not publish after explicit selection'
        publication = witness.settled(True, timeout=timeout)
        b_id, b_sid = publication[0][0], publication[1][0]

        def unchanged():
            participant = witness.sample()
            assert audio.invoke(app, 'shared_window_ids') == [b_id], 'Retired A removed or overwrote native B'
            video = [track for track in participant.get('tracks', []) if track.get('type') in (1, 'VIDEO')]
            assert len(video) == 1 and video[0].get('sid') == b_sid, 'A created or replaced a video publication after B'
            metadata = json.loads(participant.get('metadata') or '{}')
            assert metadata.get('petalWindowRemoteControl', {}).get(str(b_id)) is False, \
                'B did not retain its native view-only control advertisement'

        with monitor.owned_counter() as (nonce, marker):
            for counter in (10, 11):
                if counter == 11:
                    marker.stdin.write('11\n')
                    marker.stdin.flush()
                deadline = time.monotonic() + timeout
                while True:
                    unchanged()
                    text = monitor.peer_frame(windows_vm, evidence_dir / ('B-counter-' + str(counter) + '.png'))
                    if ('selected monitor' in text.lower()
                            and re.search(r'\b' + nonce + r'\s*:\s*' + str(counter) + r'\b', text)):
                        break
                    assert time.monotonic() < deadline, 'Current Windows B picture did not follow counter ' + str(counter)
                    time.sleep(.2)
        unchanged()
        retire = audio.evaluate(app, 'window.__linuxShareGeneration.retire || null')
        succeeded = True
        return {
            'scenario': 'S08', 'retireAction': retire_action,
            'oldStartRejected': True, 'currentNativeShareId': b_id, 'currentVideoSid': b_sid,
            'explicitBConsent': True, 'realWindowsPixels': True, 'counters': [10, 11],
            'noRemoteControlAdvertised': True,
            'oldStartCompletedAfterB': old['finishedAt'] > current['finishedAt'],
            'oldStopCompletedAfterB': bool(retire and retire['finishedAt'] > current['finishedAt']),
            'deliberatelyLateFrameObserved': False,
            'coverage': 'Real native pending-permission retirement/recovery prefix, not the full late-frame/stop-completion clause',
        }
    finally:
        if not succeeded:
            # This witness requires no initial share, so all picture work now
            # belongs to this row. Never leave/disconnect the caller's room.
            try:
                for key in ('A', 'B'):
                    state = audio.evaluate(app, 'window.__linuxShareGeneration[%s] || null' % json.dumps(key))
                    if state is not None and state['result'] is None:
                        cancel_permission('S08-owned-selector-cleanup')
            finally:
                # The disconnect row can legitimately have no SFU participant
                # yet; cleanup must still reach native ownership, not fail its
                # audio precondition before sending Stop.
                begin(app, 'cleanup', 'stop_all_shares')
                cleaned = await_result(app, 'cleanup', timeout)
                assert cleaned['result'].get('resolved'), 'Owned S08 picture cleanup rejected'


if __name__ == '__main__':
    raise SystemExit('No check ran: import run_s08_pending_start into the retained native-app acceptance context')
