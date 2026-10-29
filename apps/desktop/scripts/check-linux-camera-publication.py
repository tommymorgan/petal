#!/usr/bin/env python3
"""Task2.1 camera publication witness against the real desktop.

Drive the already-implemented Linux camera flow in a joined, muted native
room with the existing Windows peer: portal probe (dead on desktops without
a Camera portal backend), session-daemon fallback, SFU track publication,
preview/publication overlap, stop, and post-leave rejection. Assertions bind
to observable state (SFU track layers, publish state, command results); no
fabricated media is used. The CLI reads the owned SFU the same way the S05
witness does: read-only ListParticipants with a host-only room-admin token.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import stat
import sys
import time
import urllib.error
import urllib.request
from urllib.parse import urlsplit

_spec = importlib.util.spec_from_file_location(
    'hosted_witness', Path(__file__).with_name('check-linux-hosted-entry.py'))
hosted = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(hosted)

_launch_spec = importlib.util.spec_from_file_location(
    'launch_witness', Path(__file__).with_name('check-linux-launch.py'))
launch = importlib.util.module_from_spec(_launch_spec)
_launch_spec.loader.exec_module(launch)


class NoCredentialRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def make_observer(http_url, room, token_file, identity):
    """Read the Linux participant from the owned SFU; no secrets in Petal."""
    url = http_url.rstrip('/') + '/twirp/livekit.RoomService/ListParticipants'
    parsed = urlsplit(url)
    assert parsed.scheme == 'https' or parsed.hostname in ('localhost', '127.0.0.1'), \
        'Owned SFU HTTP url must be explicit'
    assert not parsed.username and not parsed.password
    assert stat.S_IMODE(token_file.stat().st_mode) & 0o077 == 0, \
        'Room-admin token file must be private'
    token = token_file.read_text().strip()
    assert token and not any(char.isspace() for char in token), 'Invalid room-admin token file'
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoCredentialRedirect())

    def observe():
        request = urllib.request.Request(
            endpoint=url, data=json.dumps({'room': room}).encode(),
            headers={'Authorization': 'Bearer ' + token,
                     'Content-Type': 'application/json'})
        try:
            with opener.open(request, timeout=5) as response:
                participants = json.load(response).get('participants', [])
        except (urllib.error.URLError, ValueError):
            raise AssertionError('Owned SFU ListParticipants read failed') from None
        matches = [p for p in participants if p.get('identity') == identity]
        assert len(matches) == 1, 'Expected the original Linux participant on the owned SFU'
        return matches[0]
    return observe


def invoke_async(app, command, marker):
    """Fire a Tauri command; poll the marker the page sets on completion."""
    app.native_input.evaluate(app.observe_processes(), f"""(() => {{
        window.{marker} = 'pending';
        const w = window.__TAURI__?.invoke
            ? window.__TAURI__
            : {{invoke: (c) => window.__TAURI_INTERNALS__.invoke(c)}};
        w.invoke('{command}').then(
            (r) => {{ window.{marker} = 'ok:' + JSON.stringify(r).slice(0, 160); }},
            (e) => {{ window.{marker} = 'rejected:' + (e && String(e.message || e).slice(0, 120)); }});
        return true;
    }})()""")
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        value = app.native_input.evaluate(
            app.observe_processes(), f'window.{marker}')
        if value != 'pending':
            return value
        time.sleep(0.5)
    return 'timeout'


def camera_track(participant):
    video = [t for t in participant.get('tracks', []) if t.get('type') == 'VIDEO']
    return video[0] if video else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--backend-url', required=True)
    parser.add_argument('--room-code', required=True)
    parser.add_argument('--expected-peer-name', required=True)
    parser.add_argument('--livekit-http-url', required=True)
    parser.add_argument('--livekit-room', required=True)
    parser.add_argument('--room-admin-token-file', type=Path, required=True)
    parser.add_argument('--timeout', type=float, default=180)
    args = parser.parse_args()

    checks = []

    def record(name, ok, detail=''):
        checks.append((name, ok, detail))
        print(('PASS' if ok else 'FAIL') + f': {name}' + (f' -- {detail}' if detail else ''))

    with launch.NativeLaunch(args.executable, args.timeout, backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        hosted.join_hosted_room(app, args.expected_peer_name)
        roster = hosted.native_async(
            app, 'return await window.__TAURI_INTERNALS__.invoke(%s, {});'
                 % json.dumps('room_presence'))
        local = [p for p in roster if p.get('isLocal')]
        assert len(local) == 1, 'Expected one original Linux identity'
        observe = make_observer(args.livekit_http_url, args.livekit_room,
                                args.room_admin_token_file, local[0]['identity'])

        # C01: start; the portal probe yields to the session daemon on
        # desktops without a Camera portal backend, and the SFU track must
        # carry a real layer (ssrc + dimensions), not just exist muted.
        outcome = invoke_async(app, 'start_camera_publish_command', '__c01')
        record('C01 camera start command resolved',
               outcome.startswith('ok:') or '"published":true' in outcome, outcome)
        deadline = time.monotonic() + 30
        track = None
        while time.monotonic() < deadline:
            track = camera_track(observe())
            if track:
                break
            time.sleep(1)
        live_layer = bool(track) and bool(track.get('layers')) and \
            any(layer.get('ssrc') for layer in track['layers'])
        record('C01 SFU camera track live with layer',
               live_layer, json.dumps(track)[:160] if track else 'no track')

        # C02: preview opens while publishing; publication survives close.
        preview = invoke_async(app, 'linux_camera_preview_open', '__c02')
        record('C02 preview opened during publication',
               preview.startswith('ok:'), preview)
        state = invoke_async(app, 'camera_publish_state', '__c02s')
        record('C02 publishing state held', '"publishing":true' in state, state)
        closed = invoke_async(app, 'linux_camera_preview_close', '__c02c')
        record('C02 preview closed', closed.startswith('ok:'), closed)

        # C07: stop clears publication and the SFU track.
        stopped = invoke_async(app, 'stop_camera_publish_command', '__c07')
        record('C07 stop resolved', stopped.startswith('ok:'), stopped)
        time.sleep(2)
        gone = camera_track(observe()) is None
        record('C07 SFU camera track removed', gone)
        state = invoke_async(app, 'camera_publish_state', '__c07s')
        record('C07 state publishing=false', '"publishing":false' in state, state)

        # C07: leave, then a late start must be rejected outright.
        hosted.leave_hosted_room(app)
        time.sleep(2)
        late = invoke_async(app, 'start_camera_publish_command', '__c07l')
        record('C07 late start rejected after leave',
               'rejected' in late and 'not in room' in late, late)

    failed = [name for name, ok, _ in checks if not ok]
    print(f'{len(checks) - len(failed)}/{len(checks)} camera checks passed')
    if failed:
        print('FAILED: ' + ', '.join(failed))
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
