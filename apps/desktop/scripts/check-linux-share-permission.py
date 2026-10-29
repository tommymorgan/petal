#!/usr/bin/env python3
"""S06 Cancel: retain the original native publication and changing Windows picture.

Import run_s06_cancel into the parent's retained, already-muted native room.
The original DP-1 selection and Windows VM must already be explicitly authorized.
This uses the actual Add share control while that share is active; the caller
cancels the actual owned GNOME dialog. It observes production picker events and
real Windows pixels without replacing the native invocation. It never stops
the original share, changes mute, edits permission stores, locks the host or
restarts services.

GNOME exposes Cancel/Share, not Deny: no runtime Deny claim is made by this
witness, and the complete S06 outline requires separate denial evidence.
"""
import importlib.util
import math
from pathlib import Path
import re
import time

_spec = importlib.util.spec_from_file_location(
    'permission_monitor_witness', Path(__file__).with_name('check-linux-monitor-publication.py'))
monitor = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(monitor)
audio = monitor.audio

_picture_spec = importlib.util.spec_from_file_location(
    'permission_picture_witness', Path(__file__).with_name('check-linux-picture-publication.py'))
picture = importlib.util.module_from_spec(_picture_spec)
_picture_spec.loader.exec_module(picture)


def run_s06_cancel(app, observe_participant, cancel_share, windows_vm, evidence_dir, timeout=90):
    """Leave the original share joined/muted/live; retire only the new attempt.

    observe_participant reads the actual original Linux participant from the
    owned SFU. cancel_share(label) must operate only this attempt's real native
    Cancel button, never return a fabricated portal result.
    """
    assert math.isfinite(timeout) and timeout >= 10
    evidence_dir = Path(evidence_dir)
    evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    mic = audio.wait_microphone(observe_participant)
    assert mic.get('muted', False), 'S06 requires the original microphone to remain muted'
    witness = audio.AudioWitness(app, observe_participant, True, mic['sid'])
    original = witness.settled(True, timeout=timeout)
    original_id, original_sid = original[0][0], original[1][0]

    def unchanged():
        participant = witness.sample()
        assert audio.invoke(app, 'shared_window_ids') == [original_id], \
            'Permission rejection retired or replaced the original native share'
        video = [track for track in participant.get('tracks', []) if track.get('type') in (1, 'VIDEO')]
        assert len(video) == 1 and video[0].get('sid') == original_sid, \
            'Permission rejection published another picture or retired/replaced the original SFU track'
        assert video[0].get('source') in (3, 'SCREEN_SHARE'), 'Original publication ceased to be a screen picture'

    def read_counter(nonce, value, path):
        deadline = time.monotonic() + timeout
        while True:
            unchanged()
            text = monitor.peer_frame(windows_vm, path)
            if ('selected monitor' in text.lower()
                    and re.search(r'\b' + nonce + r'\s*:\s*' + str(value) + r'\b', text)):
                return
            assert time.monotonic() < deadline, \
                'Original Windows picture stopped following the owned counter ' + str(value)
            time.sleep(.2)

    with picture.native_picker_events(app) as events, monitor.owned_counter() as (nonce, marker):
        attempted = False
        attempt_id = None

        def completed():
            return attempt_id is not None and any(
                event.get('attemptId') == attempt_id and event.get('open') is False
                for event in events())

        try:
            read_counter(nonce, 10, evidence_dir / 'before-cancel.png')
            unchanged()
            picture.click_add_share(app)
            attempted = True
            deadline = time.monotonic() + timeout
            while attempt_id is None:
                unchanged()
                opened = [event for event in events() if event.get('open') is True]
                if opened:
                    assert len(opened) == 1, 'Add share created ambiguous native selection attempts'
                    attempt_id = opened[0]['attemptId']
                    break
                assert time.monotonic() < deadline, 'Add share did not begin an actual native permission attempt'
                time.sleep(.05)
            assert not completed(), 'Selection settled before the real native Cancel action'
            cancel_share('S06-cancel')
            while not completed():
                unchanged()
                assert time.monotonic() < deadline, 'Cancelled native selection did not retire'
                time.sleep(.1)
            settled = witness.settled(True, timeout=timeout)
            assert settled == original, 'Cancellation changed the settled original publication identity'
            marker.stdin.write('11\n')
            marker.stdin.flush()
            read_counter(nonce, 11, evidence_dir / 'after-cancel.png')
            unchanged()
            return {
                'scenario': 'S06', 'permissionOutcome': 'cancel',
                'originalNativeShareId': original_id, 'originalVideoSid': original_sid,
                'originalPublicationRetained': True, 'counters': [10, 11],
                'realWindowsPixels': True, 'audioSamples': witness.samples,
                'actualNativeAttemptClosed': True,
                'runtimeDenyObserved': False, 'plannedAddShareAffordanceVerified': True,
            }
        finally:
            if attempted and not completed():
                # The original belongs to the caller; retire only this chooser.
                cancel_share('S06-owned-attempt-cleanup')
                deadline = time.monotonic() + 20
                while not completed():
                    assert time.monotonic() < deadline, 'Owned Add share attempt still needs native cleanup'
                    time.sleep(.1)


if __name__ == '__main__':
    raise SystemExit('No check ran: import run_s06_cancel into the retained native-app acceptance context')
