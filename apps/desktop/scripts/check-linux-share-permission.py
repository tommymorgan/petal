#!/usr/bin/env python3
"""S06 Cancel: retain the original native publication and changing Windows picture.

Import run_s06_cancel into the parent's retained, already-muted native room.
The original DP-1 selection and Windows VM must already be explicitly authorized.
This opens the real native monitor-selection command while that share is active;
the caller cancels the actual owned GNOME dialog. It never stops the original
share, changes mute, edits permission stores, locks the host or restarts services.

Current Linux UI calls this selection 'Change monitor', not the plan's 'Add
share'. This witness proves the shared native selection/cancellation boundary;
it does not assert that the planned Add share affordance exists. GNOME exposes
Cancel/Share, not Deny: no runtime Deny claim is made by this witness.
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

    attempted = False
    try:
        with monitor.owned_counter() as (nonce, marker):
            read_counter(nonce, 10, evidence_dir / 'before-cancel.png')
            unchanged()
            audio.evaluate(app, """(() => {
                window.__linuxPermissionPreservation = null;
                window.__TAURI_INTERNALS__.invoke('open_window_picker_window', {}).then(
                    () => { window.__linuxPermissionPreservation = {resolved: true}; },
                    error => { window.__linuxPermissionPreservation = {rejected: String(error)}; });
                return true;
            })()""")
            attempted = True
            # A command-level denial is not evidence of cancelling a real
            # permission flow. Observe early failure before asking for Cancel.
            deadline = time.monotonic() + timeout
            grace = time.monotonic() + .5
            while time.monotonic() < grace:
                unchanged()
                early = audio.evaluate(app, 'window.__linuxPermissionPreservation')
                assert early is None, 'Selection settled before the native permission Cancel action'
                time.sleep(.05)
            cancel_share('S06-cancel')
            while True:
                unchanged()
                outcome = audio.evaluate(app, 'window.__linuxPermissionPreservation')
                if outcome is not None:
                    assert outcome.get('rejected'), 'Cancelling permission must reject, never publish a selected replacement'
                    break
                assert time.monotonic() < deadline, 'Cancelled native selection did not retire'
                time.sleep(.2)
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
                'nativeRejection': outcome['rejected'],
                'runtimeDenyObserved': False, 'plannedAddShareAffordanceVerified': False,
            }
    finally:
        if attempted and audio.evaluate(app, 'window.__linuxPermissionPreservation') is None:
            # Do not use stop_all_shares: the original belongs to the caller
            # and must not be destroyed to clean up our pending selector.
            cancel_share('S06-owned-attempt-cleanup')
            deadline = time.monotonic() + 20
            while audio.evaluate(app, 'window.__linuxPermissionPreservation') is None:
                assert time.monotonic() < deadline, 'Owned permission attempt still needs native cancellation cleanup'
                time.sleep(.2)


if __name__ == '__main__':
    raise SystemExit('No check ran: import run_s06_cancel into the retained native-app acceptance context')
