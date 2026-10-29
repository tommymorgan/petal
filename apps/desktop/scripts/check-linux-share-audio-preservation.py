#!/usr/bin/env python3
"""S05: observe the real native share lifecycle and independent SFU audio state.

No capture/transport mocks, extra SDK participant, or source-text assertions.
The operator approves only the explicitly owned safe source in each real portal.
Change opens the existing native picker entrypoint while a share is active;
the witness never manufactures a change by stopping and restarting itself.
A fresh native share ID and SFU video SID are required after selection.

Import run_s05(app, observe_participant, approve_share) to reuse an already-owned
NativeLaunch. observe_participant must read the actual Linux participant from
the owned SFU; approve_share(label) drives the real portal, never synthesizes a
success. The CLI uses LiveKit's read-only ListParticipants endpoint with a
host-only, short-lived room-admin token file. No server secret enters Petal.
This does unmute the microphone; execution requires explicit authorization.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import stat
import time
import urllib.error
import urllib.parse
import urllib.request

_spec = importlib.util.spec_from_file_location(
    'share_audio_hosted', Path(__file__).with_name('check-linux-hosted-entry.py'))
hosted = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(hosted)

ROWS = ((True, 'start'), (False, 'start'), (True, 'change'),
        (False, 'change'), (True, 'stop'), (False, 'stop'))


def invoke(app, command, arguments=None):
    return hosted.native_async(app, 'return await window.__TAURI_INTERNALS__.invoke(%s, %s);'
                               % (json.dumps(command), json.dumps(arguments or {})))


def evaluate(app, expression):
    assert app.process.poll() is None, 'Owned native app exited'
    return app.native_input.evaluate(app.observe_processes(), expression)


def audio_track(participant):
    # Twirp protobuf JSON can omit the default AUDIO enum (0) and muted=false.
    tracks = participant.get('tracks', [])
    audio = [track for track in tracks if track.get('type', 0) in (0, 'AUDIO')]
    assert len(audio) == 1, 'Expected exactly one microphone and no computer/system/application audio'
    track = audio[0]
    assert track.get('source') in (2, 'MICROPHONE'), 'The sole audio publication must be the microphone'
    assert track.get('name') == 'petal-mic' and track.get('sid'), 'Missing real Petal microphone publication'
    return track


def wait_microphone(observe_participant, timeout=20):
    # Rendered membership precedes asynchronous native microphone attachment.
    # Wait only for absence; duplicates or incorrect audio remain hard failures.
    deadline = time.monotonic() + timeout
    while True:
        participant = observe_participant()
        if any(track.get('type', 0) in (0, 'AUDIO')
               for track in participant.get('tracks', [])):
            return audio_track(participant)
        assert time.monotonic() < deadline, 'Native microphone publication did not attach after joining'
        time.sleep(.1)


class AudioWitness:
    def __init__(self, app, observe_participant, muted, microphone_sid):
        self.app = app
        self.observe_participant = observe_participant
        self.muted = muted
        self.microphone_sid = microphone_sid
        self.samples = 0

    def sample(self):
        participant = self.observe_participant()
        microphone = audio_track(participant)
        assert microphone['sid'] == self.microphone_sid, 'Share action retired or replaced the microphone track'
        assert microphone.get('muted', False) is self.muted, 'Share action changed the SFU microphone mute state'
        pill = invoke(self.app, 'get_menubar_state')
        assert pill['micMuted'] is self.muted, 'Share action changed native microphone intent/UI state'
        self.samples += 1
        return participant

    def action(self, command, label, approve_share=None, timeout=90, *, arguments=None):
        # Start asynchronously so observations continue while portal permission,
        # first-frame publication and cleanup are pending. No command is mocked.
        self.sample()
        evaluate(self.app, """(() => {
            window.__linuxPictureAudioResult = null;
            window.__TAURI_INTERNALS__.invoke(%s, %s).then(
                value => { window.__linuxPictureAudioResult = {value}; },
                () => { window.__linuxPictureAudioResult = {error: true}; });
            return true;
        })()""" % (json.dumps(command), json.dumps(arguments or {})))
        deadline = time.monotonic() + timeout
        if approve_share is not None:
            # An unsupported change command can reject before a picker exists.
            # Observe that actual IPC result before asking the operator to act.
            permission_grace = min(deadline, time.monotonic() + .5)
            while time.monotonic() < permission_grace:
                self.sample()
                result = evaluate(self.app, 'window.__linuxPictureAudioResult')
                if result is not None:
                    assert not result.get('error'), label + ': production share command rejected'
                    return
                time.sleep(.05)
            approve_share(label)
        while time.monotonic() < deadline:
            self.sample()
            result = evaluate(self.app, 'window.__linuxPictureAudioResult')
            if result is not None:
                assert not result.get('error'), label + ': production share command rejected'
                return
            time.sleep(.2)
        raise AssertionError(label + ': production share command did not settle')

    def settled(self, shared, timeout=20):
        deadline, stable_since = time.monotonic() + timeout, None
        last = None
        while time.monotonic() < deadline:
            participant = self.sample()
            ids = invoke(self.app, 'shared_window_ids')
            video = [track for track in participant.get('tracks', [])
                     if track.get('type') in (1, 'VIDEO')]
            expected = (len(ids) == 1 and len(video) == 1
                        and video[0].get('source') in (3, 'SCREEN_SHARE')
                        and bool(video[0].get('sid'))) if shared else not ids and not video
            current = (tuple(ids), tuple(track.get('sid') for track in video))
            if expected:
                stable_since = (time.monotonic() if current != last or stable_since is None
                                else stable_since)
                if time.monotonic() - stable_since >= 3:
                    return current
            else:
                stable_since = None
            last = current
            time.sleep(.2)
        raise AssertionError('Native share IDs and actual SFU video publications did not settle')


def run_s05(app, observe_participant, approve_share, timeout=90, *, rows=ROWS):
    """Run the six-row outline (or an explicitly focused subset); leave muted/stopped.

    Caller owns consent and existing room lifecycle. No app/service is launched,
    stopped or restarted here. On failure, attempt only owned share stop and mic
    mute; never leave/disconnect the room or substitute a different source.
    """
    assert math.isfinite(timeout) and timeout >= 5, 'Use a finite timeout of at least five seconds'
    selected_rows = tuple(rows)
    assert selected_rows and len(set(selected_rows)) == len(selected_rows), 'Select distinct real S05 rows'
    assert set(selected_rows) <= set(ROWS), 'Unknown S05 row'
    assert invoke(app, 'shared_window_ids') == [], 'Begin with no owned Linux share'
    microphone_sid = wait_microphone(observe_participant)['sid']
    rows = []
    try:
        for muted, action in selected_rows:
            label = ('muted' if muted else 'unmuted') + '-' + action
            assert invoke(app, 'set_mic_muted', {'muted': muted}) is muted
            # Signaling may arrive after the command's local SDK completion.
            deadline = time.monotonic() + 10
            while True:
                microphone = audio_track(observe_participant())
                assert microphone['sid'] == microphone_sid, 'Setting mute replaced the microphone'
                if microphone.get('muted', False) is muted:
                    break
                assert time.monotonic() < deadline, 'Requested microphone mute state did not reach SFU'
                time.sleep(.1)
            witness = AudioWitness(app, observe_participant, muted, microphone_sid)
            witness.settled(False)
            if action != 'start':
                witness.action('toggle_window_picker_window', label + '-setup', approve_share, timeout)
                old = witness.settled(True)
            command = {'start': 'toggle_window_picker_window',
                       'change': 'open_window_picker_window', 'stop': 'stop_all_shares'}[action]
            arguments = {'replaceWindowId': old[0][0]} if action == 'change' else {}
            witness.action(command, label, None if action == 'stop' else approve_share, timeout,
                           arguments=arguments)
            result = witness.settled(action != 'stop')
            if action == 'change':
                assert result[0] != old[0] and result[1] != old[1], \
                    'Change must publish a fresh selected source, not leave the old share untouched'
            rows.append({'muteState': 'muted' if muted else 'unmuted',
                         'shareAction': action, 'audioSamples': witness.samples})
            if action != 'stop':
                witness.action('stop_all_shares', label + '-cleanup', timeout=timeout)
                witness.settled(False)
        return {'scenario': 'S05', 'rows': rows, 'microphoneRetained': True,
                'noOutputAudioObserved': True, 'speechOrAECVerified': False}
    finally:
        # Do not allow a failed assertion to leave an authorized unmute active.
        try:
            invoke(app, 'set_mic_muted', {'muted': True})
        finally:
            # Stop may need longer than native_async's five-second observation
            # deadline. Await real retirement, including a pending permission.
            evaluate(app, """(() => {
                window.__linuxPictureAudioCleanup = null;
                window.__TAURI_INTERNALS__.invoke('stop_all_shares').then(
                    () => { window.__linuxPictureAudioCleanup = true; },
                    () => { window.__linuxPictureAudioCleanup = false; });
                return true;
            })()""")
            deadline = time.monotonic() + timeout
            while True:
                result = evaluate(app, 'window.__linuxPictureAudioCleanup')
                if result is not None:
                    assert result is True, 'Owned share cleanup rejected'
                    break
                assert time.monotonic() < deadline, 'Owned share cleanup did not settle'
                time.sleep(.2)



class NoCredentialRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, new_url):
        # The room-admin bearer belongs only to the explicitly owned SFU.
        return None

def livekit_observer(url, token_file, room, identity):
    parsed = urllib.parse.urlsplit(url)
    assert parsed.scheme in ('http', 'https') and parsed.netloc, 'Use the owned SFU HTTP(S) URL'
    assert not parsed.username and not parsed.password and not parsed.query and not parsed.fragment
    assert stat.S_IMODE(token_file.stat().st_mode) & 0o077 == 0, 'Room-admin token file must be private'
    token = token_file.read_text().strip()
    assert token and not any(char.isspace() for char in token), 'Invalid room-admin token file'
    endpoint = url.rstrip('/') + '/twirp/livekit.RoomService/ListParticipants'
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoCredentialRedirect())

    def observe():
        request = urllib.request.Request(endpoint, data=json.dumps({'room': room}).encode(),
                                         headers={'Authorization': 'Bearer ' + token,
                                                  'Content-Type': 'application/json'})
        try:
            with opener.open(request, timeout=5) as response:
                participants = json.load(response).get('participants', [])
        except (urllib.error.URLError, ValueError):
            raise AssertionError('Owned SFU ListParticipants read failed') from None
        matches = [participant for participant in participants if participant.get('identity') == identity]
        assert len(matches) == 1, 'Expected the original Linux participant on the owned SFU'
        return matches[0]
    return observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--backend-url', required=True)
    parser.add_argument('--room-code', required=True)
    parser.add_argument('--expected-peer-name', required=True)
    parser.add_argument('--livekit-http-url', required=True)
    parser.add_argument('--livekit-room', required=True)
    parser.add_argument('--room-admin-token-file', type=Path, required=True)
    parser.add_argument('--evidence-dir', type=Path, required=True)
    parser.add_argument('--timeout', type=float, default=90)
    row_choices = {('muted' if muted else 'unmuted') + '-' + action: (muted, action)
                   for muted, action in ROWS}
    parser.add_argument('--row', action='append', choices=tuple(row_choices),
                        help='Run an explicit subset; omission runs all six S05 rows')
    parser.add_argument('--allow-microphone-unmute', action='store_true',
                        help='Required only with explicit authorization for selected unmuted rows')
    args = parser.parse_args()
    selected_rows = ROWS if args.row is None else tuple(row_choices[row] for row in args.row)
    if len(set(selected_rows)) != len(selected_rows):
        parser.error('Select each S05 row once')
    if any(not muted for muted, _ in selected_rows) and not args.allow_microphone_unmute:
        parser.error('Unmuted S05 rows require explicit authorization and --allow-microphone-unmute')
    assert math.isfinite(args.timeout) and args.timeout >= 5
    args.evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    with hosted.launch.NativeLaunch(args.executable, args.timeout, backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        hosted.assert_join_code(app, args.room_code)
        hosted.join_hosted_room(app, args.expected_peer_name)
        try:
            roster = invoke(app, 'room_presence')
            local = [participant for participant in roster if participant['isLocal']]
            assert len(local) == 1, 'Expected one original Linux identity'
            observe = livekit_observer(args.livekit_http_url, args.room_admin_token_file,
                                       args.livekit_room, local[0]['identity'])
            result = run_s05(app, observe, lambda label: print(
                'APPROVE_OWNED_SAFE_MONITOR ' + label + ' (leave an already selected source pressed)',
                flush=True), args.timeout, rows=selected_rows)
            path = args.evidence_dir / 'share-audio-preservation.json'
            path.write_text(json.dumps(result, indent=2) + '\n')
            path.chmod(0o600)
        finally:
            hosted.leave_hosted_room(app)
    print(f'PASS: {len(selected_rows)} requested S05 rows retained one microphone and published no output audio', flush=True)


if __name__ == '__main__':
    main()
