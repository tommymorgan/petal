#!/usr/bin/env python3
"""Machine-only Task4.1 stop/restart witness; speech/AEC acceptance stays unverified.

Use actual owned Notepad and the released Windows client's normal sharing UI.
After WINDOWS_SHARE_STOP_REQUIRED, stop sharing and send WINDOWS_SHARE_STOPPED
plus newline on stdin. After WINDOWS_SHARE_RESTART_REQUIRED, change only the
owned editor's RestartPhaseFile to restarted, share it normally, and send
WINDOWS_SHARE_RESTARTED plus newline. Each action and observation is bounded by
--timeout. No living-feature binding, generated media, or receiver test API.
"""
import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import select
import sys
import time

_spec = importlib.util.spec_from_file_location(
    'restart_disconnect', Path(__file__).with_name('check-linux-sharer-disconnect.py'))
disconnect = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(disconnect)
receive = disconnect.receive
EVENT = re.compile(r'linux incoming (subscribed|unpublished) window_id=(\d+) sid=(\S+)')


def log_text(app):
    return (app.trace.parent / 'process.log').read_text(errors='replace')


def events(app):
    # Existing original-room production consumer, never a second SDK observer.
    return [(m[1], int(m[2]), m[3]) for m in EVENT.finditer(log_text(app))]


def joined(app, baseline, main):
    current, windows = disconnect.original_room(app, baseline, main)
    peers = [p for p in current['roster'] if not p['isLocal']]
    assert len(peers) == 1 and peers[0]['identity'] == baseline['peer']['identity'], \
        'Original Windows peer must stay joined without extra participants'
    assert peers[0]['identity'] in current['visible'], 'Windows roster tile disappeared'
    text = log_text(app)
    assert text.count('RoomConnection connected:') == 1, 'Room connection was omitted or duplicated'
    assert text.count('audio: published microphone track') == 1, \
        'Application microphone publication was omitted or duplicated'
    assert 'audio: mic track unmuted' not in text, 'Diagnostic transmitted microphone audio'
    assert len(windows) <= 2 and len(current['windows']) <= 1, 'Duplicate native share resources'
    for summary in current['windows']:
        assert summary['ownerIdentity'] == baseline['peer']['identity'], 'Unexpected share owner'
    return current, windows


class ParentActions:
    def __init__(self):
        self.pending = b''

    def wait(self, marker, reply, app, baseline, main, timeout):
        print(marker, flush=True)
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            joined(app, baseline, main)
            if b'\n' in self.pending:
                line, self.pending = self.pending.split(b'\n', 1)
                assert line.rstrip(b'\r') == reply.encode('ascii'), 'Unexpected parent action reply'
                assert not self.pending, 'Parent must reply only after each requested real action'
                return
            if select.select([sys.stdin.fileno()], [], [], .2)[0]:
                chunk = os.read(sys.stdin.fileno(), 1024)
                assert chunk, 'Parent stdin closed before action acknowledgement'
                self.pending += chunk
                assert len(self.pending) <= 1024, 'Parent action acknowledgement is oversized'
        raise AssertionError('Timed out awaiting ' + reply)


def observe_pixels(app, remote, main, baseline, capture, args, phase):
    deadline = time.monotonic() + args.timeout
    counters, failure = [], 'No current native editor image'
    while time.monotonic() < deadline:
        current, windows = joined(app, baseline, main)
        assert remote in windows and len(windows) == 2, 'Independent remote native window disappeared'
        assert len(current['windows']) == 1 and not current['windows'][0]['hidden'], \
            'Exactly one visible incoming share is required'
        receive.drawing_area(remote)
        try:
            image = capture.image()
            text = '\n'.join(line for line, _ in disconnect.fidelity.recognize(image))
            assert re.search(r'PETAL\s+SAFE\s+EDITOR', text, re.I), 'Missing safe editor pixels'
            if phase == 'restarted':
                assert re.search(r'New\s+share\s+content', text, re.I), 'Missing restarted editor text'
            else:
                assert not re.search(r'New\s+share\s+content', text, re.I), 'Editor started in restarted phase'
            match = re.search(r'PETAL\s+RECEIVE\s+COUNTER\s*[:=]?\s*(\d+)', text, re.I)
            assert match, 'Missing actual editor counter'
            value = int(match[1])
            if not counters or value > counters[-1]:
                counters.append(value)
                receive.save_private_pixels(image, args.evidence_dir / (phase + '-share.png'))
                if len(counters) == 3:
                    return counters
        except AssertionError as error:
            failure = str(error)
        time.sleep(.3)
    raise AssertionError(phase + ' live pixels not established: ' + failure)


def retired(app, baseline, main, old_publication, args):
    deadline, stable_since, samples = time.monotonic() + args.timeout, None, 0
    while time.monotonic() < deadline:
        current, windows = joined(app, baseline, main)
        observed = events(app)
        unpublished = ('unpublished', *old_publication) in observed
        absent = not current['windows'] and windows == [main] and unpublished
        if absent:
            stable_since = time.monotonic() if stable_since is None else stable_since
            samples += 1
            if time.monotonic() - stable_since >= 3:
                return samples
        else:
            assert stable_since is None, 'Stopped presentation reappeared during stable absence'
        time.sleep(.2)
    raise AssertionError('Stop must retire original publication and every remote summary/window')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--backend-url', required=True)
    parser.add_argument('--room-code', required=True)
    parser.add_argument('--expected-peer-name', required=True)
    parser.add_argument('--timeout', type=float, default=90)
    parser.add_argument('--evidence-dir', type=Path, required=True)
    args = parser.parse_args()
    assert math.isfinite(args.timeout) and args.timeout >= 5, 'Timeout must be finite and at least 5 seconds'
    assert re.fullmatch(r'[A-Za-z0-9 _-]{1,80}', args.expected_peer_name), \
        'Use a non-sensitive ASCII Windows fixture name'
    args.evidence_dir.mkdir(mode=0o700, parents=False, exist_ok=False)
    actions = ParentActions()
    with receive.launch.NativeLaunch(args.executable, args.timeout, backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        state = receive.hosted.native_async(
            app, "return await window.__TAURI_INTERNALS__.invoke('get_menubar_state');")
        assert state['micMuted'] is True, 'Diagnostic must begin microphone-muted'
        receive.hosted.assert_join_code(app, args.room_code)
        receive.hosted.join_hosted_room(app, args.expected_peer_name)
        capture = None
        try:
            remote, main_window = receive.remote_and_main(app, args.timeout)
            initial = disconnect.snapshot(app)
            peers = [p for p in initial['roster'] if not p['isLocal']]
            local = [p for p in initial['roster'] if p['isLocal']]
            assert len(peers) == 1 and peers[0]['name'] == args.expected_peer_name
            assert len(local) == 1 and initial['room'] is not None
            baseline = {'room': initial['room'], 'local': local[0]['identity'], 'peer': peers[0]}
            capture = receive.WindowPixels(app, remote)
            initial_counters = observe_pixels(app, remote, main_window, baseline, capture, args, 'initial')
            capture.close()
            capture = None
            before = events(app)
            subscriptions = [(window, sid) for kind, window, sid in before if kind == 'subscribed']
            assert len(subscriptions) == 1, 'Exactly one initial share publication must be observed'
            old = subscriptions[0]
            assert not any(kind == 'unpublished' for kind, _, _ in before), 'Initial share was already retired'
            actions.wait('WINDOWS_SHARE_STOP_REQUIRED', 'WINDOWS_SHARE_STOPPED',
                         app, baseline, main_window, args.timeout)
            samples = retired(app, baseline, main_window, old, args)
            actions.wait('WINDOWS_SHARE_RESTART_REQUIRED', 'WINDOWS_SHARE_RESTARTED',
                         app, baseline, main_window, args.timeout)
            deadline = time.monotonic() + args.timeout
            while True:
                current, windows = joined(app, baseline, main_window)
                remotes = [w for w in windows if w != main_window]
                if len(remotes) == 1 and len(current['windows']) == 1:
                    restarted = remotes[0]
                    break
                assert time.monotonic() < deadline, 'Restart did not open one independent remote window'
                time.sleep(.2)
            capture = receive.WindowPixels(app, restarted)
            restarted_counters = observe_pixels(
                app, restarted, main_window, baseline, capture, args, 'restarted')
            after = events(app)
            subscribed = [(window, sid) for kind, window, sid in after if kind == 'subscribed']
            assert len(subscribed) == 2 and subscribed[0] == old, 'Restart duplicated or omitted subscriptions'
            new = subscribed[1]
            assert new[1] != old[1], 'Restart reused original publication SID'
            assert after.index(('unpublished', *old)) < after.index(('subscribed', *new)), \
                'New publication arrived before original unpublication'
            assert ('unpublished', *new) not in after, 'Restarted publication was retired'
            joined(app, baseline, main_window)
        finally:
            try:
                if capture is not None:
                    capture.close()
            finally:
                receive.hosted.leave_hosted_room(app)
        app.assert_room_entry(fresh_identity=False)
        deadline = time.monotonic() + min(args.timeout, 10)
        while any(w.get_name().startswith('Petal — ') for w in receive.owned_windows(app)):
            assert time.monotonic() < deadline, 'Normal Leave retained a remote native window'
            time.sleep(.1)
        proof = {'diagnostic': 'Task4.1 machine-only share stop/restart',
                 'initial_counters': initial_counters, 'restarted_counters': restarted_counters,
                 'new_share_content_ocr': True, 'stable_absence_seconds': 3,
                 'absence_samples': samples, 'original_room_identity_main_preserved': True,
                 'windows_peer_stayed_joined': True, 'one_muted_microphone_publication': True,
                 'one_room_connection': True, 'one_restarted_native_remote_window': True,
                 'original_unpublication_and_distinct_restart_sid_observed': True,
                 'normal_leave_room_entry': True, 'audible_audio_verified': False,
                 'aec_verified': False, 'full_scenario_accepted': False}
        path = args.evidence_dir / 'share-restart.json'
        path.write_text(json.dumps(proof, indent=2) + '\n')
        path.chmod(0o600)
    print('PASS: machine-only native share stop/restart; speech and AEC remain unverified', flush=True)


if __name__ == '__main__':
    main()
