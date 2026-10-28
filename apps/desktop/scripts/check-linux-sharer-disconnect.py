#!/usr/bin/env python3
"""Witness only Task4.1's disconnected-sharer scenario, without listening.

On an authorized Windows machine run windows-receive-editor.ps1 and share its
changing safe Notepad window using RELEASED Petal (never the WGC race fixture).
Keep Windows joined until WINDOWS_SHARER_LEAVE_REQUIRED, then click its normal
Leave. This script observes real pixels, the original room's production presence
and native-window summaries, and the existing ParticipantDisconnected log. It
never publishes media, injects events, changes receiver state or captures a
monitor. --feature optionally binds the exact complete living scenario; omit it
for the first runtime proof before the living file exists.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import re
import time

_spec = importlib.util.spec_from_file_location(
    "disconnect_fidelity", Path(__file__).with_name("check-linux-receive-fidelity.py"))
fidelity = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fidelity)
receive = fidelity.receive

SCENARIO = "Retire a disconnected sharer presentation"
STEPS = (
    "Given Linux is in a room displaying a participant's active share in a remote window",
    "When that sharer disconnects from the room",
    "Then that participant's remote window is removed",
    "And no frozen window is presented as that participant's live share",
    "When I leave the room",
    "Then Linux returns to its room-entry state normally",
)


def living_contract(path):
    selected, found, matches, features = False, [], 0, 0
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("Feature:"):
            features += 1
            selected = False
            continue
        assert not line.startswith(("Background:", "Rule:", "@")), \
            "Unbound disconnect Background/Rule/tag"
        if line.startswith(("Scenario:", "Scenario Outline:")):
            kind, _, name = line.partition(":")
            selected = name.strip() == SCENARIO
            if selected:
                matches += 1
                assert kind == "Scenario", "Disconnect must retain its complete scenario"
            continue
        if selected:
            found.append(line)
    assert features == 1 and matches == 1 and tuple(found) == STEPS, \
        "Disconnect witness requires every exact Task4.1 disconnected-sharer clause"


def snapshot(app):
    # Read existing production APIs; identifiers stay in memory, never evidence.
    return receive.hosted.native_async(app, """
        const [room, roster, windows] = await Promise.all([
            window.__TAURI_INTERNALS__.invoke('current_room'),
            window.__TAURI_INTERNALS__.invoke('room_presence'),
            window.__TAURI_INTERNALS__.invoke('compositor_list_windows')
        ]);
        const visible = [...document.querySelectorAll('[data-participant-key]')]
            .filter(n => n.getClientRects().length > 0 && getComputedStyle(n).visibility !== 'hidden')
            .map(n => n.dataset.participantKey);
        return {room, roster, windows, visible,
            routeMatches: room !== null && location.pathname === '/meeting/' + encodeURIComponent(room),
            leaveAvailable: !!document.querySelector('button[aria-label="Leave meeting"]:not(:disabled)')};
    """)


def original_room(app, baseline, main):
    assert app.process.poll() is None, "Linux exited instead of keeping the room joined"
    current = snapshot(app)
    state = receive.hosted.native_async(
        app, "return await window.__TAURI_INTERNALS__.invoke('get_menubar_state');")
    assert state['micMuted'] is True, "Disconnect witness unexpectedly enabled microphone audio"
    assert current['room'] == baseline['room'] and current['routeMatches'], \
        "Linux lost or replaced its original joined room"
    local = [p for p in current['roster'] if p['isLocal']]
    assert len(local) == 1 and local[0]['identity'] == baseline['local'], \
        "Original Linux membership was replaced"
    assert local[0]['identity'] in current['visible'] and current['leaveAvailable'], \
        "Joined local tile or normal Leave is missing"
    windows = receive.owned_windows(app)
    assert main in windows, "Original native meeting window disappeared"
    assert [w for w in windows if not w.get_name().startswith('Petal — ')] == [main], \
        "Original native main window was replaced or duplicated"
    return current, windows


def departure_event(app, peer, offset):
    # Presence's existing consumer of this original RoomConnection logs actual
    # ParticipantDisconnected. Match the exact safe fixture name and identity,
    # not a substring such as another participant's departure. Never retain log.
    text = (app.trace.parent / 'process.log').read_text(errors='replace')[offset:]
    pattern = (r"presence: '" + re.escape(peer['name']) + r"' \("
               + re.escape(peer['identity']) + r"\) disconnected from '[^'\n]*' -- roster \d+ -> \d+")
    return re.search(pattern, text) is not None


def live_pixels(app, remote, main, baseline, capture, args):
    deadline, counters, failure = time.monotonic() + args.timeout, [], 'No native editor pixels'
    while time.monotonic() < deadline:
        current, windows = original_room(app, baseline, main)
        assert remote in windows and len(windows) == 2, "Initial native remote window was lost or duplicated"
        assert len(current['windows']) == 1, "Initial native share summary is ambiguous"
        summary = current['windows'][0]
        assert summary['ownerIdentity'] == baseline['peer']['identity'] and not summary['hidden'], \
            "Visible incoming share must belong to the selected Windows participant"
        assert baseline['peer']['identity'] in current['visible'], "Windows participant left before readiness"
        receive.drawing_area(remote)
        try:
            image = capture.image()
            text = '\n'.join(line for line, _ in fidelity.recognize(image))
            assert re.search(r'PETAL\s+SAFE\s+EDITOR', text, re.I), "Missing safe editor pixels"
            match = re.search(r'PETAL\s+RECEIVE\s+COUNTER\s*[:=]?\s*(\d+)', text, re.I)
            assert match, "Missing actual editor counter"
            value = int(match[1])
            if not counters or value > counters[-1]:
                counters.append(value)
                receive.save_private_pixels(image, args.evidence_dir / 'live-share.png')
                if len(counters) == 3:
                    return counters
        except AssertionError as error:
            failure = str(error)
        time.sleep(.3)
    raise AssertionError(f'Live share not established: {failure}; counters {counters}')


def wait_for_retirement(app, main, baseline, args, offset):
    deadline, stable_since, samples = time.monotonic() + args.timeout, None, 0
    departed = False
    while time.monotonic() < deadline:
        current, windows = original_room(app, baseline, main)
        peer_id = baseline['peer']['identity']
        absent = not any(p['identity'] == peer_id for p in current['roster'])
        departed = departed or departure_event(app, baseline['peer'], offset)
        retired = (departed and absent and peer_id not in current['visible']
                   and not current['windows'] and windows == [main])
        if retired:
            stable_since = time.monotonic() if stable_since is None else stable_since
            samples += 1
            # Observe continued absence, not a transient unmap followed by a
            # frozen remap. Zero native top-levels AND zero summaries rule out
            # a frozen presentation, rather than treating stopped counters as removal.
            if time.monotonic() - stable_since >= 3:
                return samples
        else:
            assert stable_since is None, "Retired participant presentation or presence reappeared"
        time.sleep(.2)
    raise AssertionError(
        'Windows departure must produce original-room ParticipantDisconnected, remove its '
        'roster/tile and every remote GTK window/summary, while Linux remains joined; '
        f'disconnect event observed: {departed}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--backend-url', required=True)
    parser.add_argument('--room-code', required=True)
    parser.add_argument('--expected-peer-name', required=True)
    parser.add_argument('--timeout', type=float, default=90)
    parser.add_argument('--evidence-dir', type=Path, required=True)
    parser.add_argument('--feature', type=Path)
    args = parser.parse_args()
    assert math.isfinite(args.timeout) and args.timeout > 0
    assert re.fullmatch(r'[A-Za-z0-9 _-]{1,80}', args.expected_peer_name), \
        'Use a non-sensitive ASCII Windows fixture name for exact existing-log matching'
    if args.feature:
        living_contract(args.feature)
    args.evidence_dir.mkdir(mode=0o700, parents=False, exist_ok=False)
    with receive.launch.NativeLaunch(args.executable, args.timeout,
                                    backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        state = receive.hosted.native_async(
            app, "return await window.__TAURI_INTERNALS__.invoke('get_menubar_state');")
        assert state['micMuted'] is True, "Disconnect witness must never transmit microphone audio"
        receive.hosted.assert_join_code(app, args.room_code)
        receive.hosted.join_hosted_room(app, args.expected_peer_name)
        capture = None
        try:
            remote, main_window = receive.remote_and_main(app, args.timeout)
            initial = snapshot(app)
            peers = [p for p in initial['roster'] if not p['isLocal']]
            locals_ = [p for p in initial['roster'] if p['isLocal']]
            assert len(peers) == 1 and peers[0]['name'] == args.expected_peer_name, \
                'Require exactly the released Windows sharer and Linux, no extra observer'
            assert len(locals_) == 1 and initial['room'] is not None
            baseline = {'room': initial['room'], 'local': locals_[0]['identity'], 'peer': peers[0]}
            capture = receive.WindowPixels(app, remote)
            counters = live_pixels(app, remote, main_window, baseline, capture, args)
            # The stream belongs only to the proven original native window;
            # close it before real destruction, never read a cached final frame.
            capture.close()
            capture = None
            offset = len((app.trace.parent / 'process.log').read_text(errors='replace'))
            # Recheck live ownership immediately before the parent-action marker.
            current, windows = original_room(app, baseline, main_window)
            assert remote in windows and len(current['windows']) == 1
            assert any(p['identity'] == peers[0]['identity'] for p in current['roster'])
            print('WINDOWS_SHARER_LEAVE_REQUIRED', flush=True)
            samples = wait_for_retirement(app, main_window, baseline, args, offset)
            print('SHARER_PRESENTATION_RETIRED_LINUX_STILL_JOINED', flush=True)
        finally:
            try:
                if capture is not None:
                    capture.close()
            finally:
                receive.hosted.leave_hosted_room(app)
        app.assert_room_entry(fresh_identity=False)
        deadline = time.monotonic() + min(args.timeout, 10)
        while any(w.get_name().startswith('Petal — ') for w in receive.owned_windows(app)):
            assert time.monotonic() < deadline, 'Normal Linux Leave retained a frozen remote window'
            time.sleep(.1)
        proof = {'scenario': SCENARIO, 'live_counters': counters,
                 'original_room_disconnect_observed': True,
                 'retired_window_absence_samples': samples, 'absence_seconds': 3,
                 'linux_stayed_joined': True, 'normal_leave_room_entry': True}
        path = args.evidence_dir / 'sharer-disconnect.json'
        path.write_text(json.dumps(proof, indent=2) + '\n')
        path.chmod(0o600)
    print('PASS: disconnected sharer presentation removed; Linux stayed joined; normal Leave returned to room entry',
          flush=True)


if __name__ == '__main__':
    main()
