#!/usr/bin/env python3
"""Exercise join errors and real hosted recovery in the owned native window.

No fake backend or synthetic peer is accepted. The parent independently verifies
hosted refusal and Windows membership; room-code arguments are private fixtures.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import time

spec = importlib.util.spec_from_file_location(
    "hosted_entry", Path(__file__).with_name("check-linux-hosted-entry.py")
)
hosted = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hosted)

MISSING_SCENARIO = 'Missing hosted configuration never falls back silently'
MISSING_STEPS = (
    'Given I launch a Linux build with no baked hosted backend and no runtime backend override',
    'And no LiveKit server API secret is present on the client',
    'When I attempt a hosted room join',
    'Then I see a configuration error naming the missing hosted backend setting',
    'And Linux does not attempt to join a local server or an alternative backend',
    'And the app does not ask me to install a LiveKit server API secret',
    'When I configure only the intended hosted backend URL and restart the app without a server API secret',
    'And I join accessible hosted room abc-defg-hjk',
    'Then Linux joins successfully without any client dependency on a LiveKit server API secret',
)

RECOVERY_SCENARIO = 'Recover from an invalid or refused hosted join'
RECOVERY_STEPS = (
    'Given Linux is configured for the intended hosted Petal backend',
    'And joining <attempted input> produces <failure>',
    'When I attempt to join using <attempted input>',
    'Then I see an actionable join error',
    'And Linux remains outside the room without microphone publication or a remote window',
    'When I correct the room code to accessible hosted room abc-defg-hjk and join',
    'Then Linux joins room abc-defg-hjk normally',
    'Examples:',
    '| attempted input | failure |',
    '| petal://join/000000 | an invalid code |',
    '| def-ghjk-mno | a refused access result |',
)


def living_contract(feature, failure_kind):
    scenario = MISSING_SCENARIO if failure_kind == 'missing' else RECOVERY_SCENARIO
    expected = MISSING_STEPS if failure_kind == 'missing' else RECOVERY_STEPS
    expected_kind = 'Scenario' if failure_kind == 'missing' else 'Scenario Outline'
    selected = False
    seen_feature = False
    found = []
    count = 0
    for raw in Path(feature).read_text().splitlines():
        line = raw.strip()
        if line.startswith('Feature:'):
            seen_feature = True
            continue
        if not line or line.startswith('#'):
            continue
        assert not line.startswith(('Background:', 'Rule:')), 'Unbound shared living clauses'
        if line.startswith(('Scenario:', 'Scenario Outline:')):
            kind, _, name = line.partition(':')
            selected = name.strip() == scenario
            if selected:
                count += 1
                assert kind == expected_kind, 'Living scenario kind must retain its exact examples contract'
            continue
        if selected:
            found.append(' '.join(line.split()) if line.startswith('|') else line)
    assert seen_feature and count == 1 and tuple(found) == expected, \
        'Every recovery living clause and complete example set must bind exactly and in order'


def enter_and_submit(app, value):
    accepted = app.native_input.evaluate(app.observe_processes(), """(() => {
        const input = document.querySelector('input[aria-label="Meeting name, invite link, or meeting code"]');
        if (!input || input.disabled) return false;
        input.value = %s;
        input.dispatchEvent(new InputEvent('input', {bubbles: true, inputType: 'insertText', data: input.value}));
        return true;
    })()""" % json.dumps(value))
    assert accepted, 'Native room input is not available for recovery'
    time.sleep(0.2)
    clicked = app.native_input.evaluate(app.observe_processes(), """(() => {
        const input = document.querySelector('input[aria-label="Meeting name, invite link, or meeting code"]');
        const button = input?.closest('form')?.querySelector('button');
        if (!button || button.disabled) return false;
        button.click();
        return true;
    })()""")
    assert clicked, 'Native entry submit was unavailable'


def assert_outside(app):
    snapshot = hosted.native_async(app, """
        return {
            room: await window.__TAURI_INTERNALS__.invoke('current_room'),
            presence: await window.__TAURI_INTERNALS__.invoke('room_presence'),
            windows: await window.__TAURI_INTERNALS__.invoke('plugin:window|get_all_windows')
        };
    """)
    assert snapshot['room'] is None and snapshot['presence'] == [], \
        'Failed join retained native membership/publication ownership'
    assert not any(label.startswith(('remote-', 'remote_', 'petal-remote-', 'share-'))
                   for label in snapshot['windows']), 'Failed join opened a remote window'


def assert_failure(app, kind):
    deadline = time.monotonic() + app.timeout
    observed = False
    returned = False
    while time.monotonic() < deadline:
        state = app.native_input.evaluate(app.observe_processes(), """(() => {
            const text = document.body.innerText;
            const category = %s;
            const actionable = category === 'missing'
                ? /PETAL_BACKEND_URL/.test(text) && /configur|set|backend URL|restart/i.test(text)
                : category === 'invalid'
                    ? /invite|meeting code|room code/i.test(text) && /paste|enter|check|correct|valid/i.test(text)
                    : /access|denied|refus|not found/i.test(text) && /check|ask|host|correct|try/i.test(text);
            return {
                actionable,
                secretDemand: /install.{0,40}(?:API|secret)|set.{0,20}LIVEKIT_API_(?:KEY|SECRET)/i.test(text),
                entry: location.pathname === '/main' && !!document.querySelector('input[aria-label="Meeting name, invite link, or meeting code"]')
            };
        })()""" % json.dumps(kind))
        assert not state['secretDemand'], 'Native join demanded a server API secret'
        observed = observed or state['actionable']
        returned = state['entry']
        if observed and returned:
            assert_outside(app)
            return
        time.sleep(0.05)
    assert observed, 'Native join did not show an actionable error for the failure category'
    assert returned, 'Native failed join did not restore room entry'


def recover(app, args):
    hosted.assert_join_code(app, args.room_code)
    hosted.join_hosted_room(app, args.expected_peer_name)
    print('PASS: corrected configuration/code joined the actual hosted Windows room', flush=True)
    deadline = time.monotonic() + args.hold_seconds
    while time.monotonic() < deadline:
        app.observe_processes()
        time.sleep(0.2)
    hosted.leave_hosted_room(app)


def assert_no_network_join(app, offset):
    # AF_UNIX desktop/session traffic is not a backend attempt. Every IPv4/6
    # destination during the join is rejected, including DNS or a local SFU.
    # The inspector client is the driver (not a tracee); this is not an app
    # connection exception or an inferred check of the configured URL field.
    with app.trace.open() as trace:
        trace.seek(offset)
        observed = trace.read()
    assert not re.search(r'(?:connect|sendto)\([^\n]*AF_INET6?\b', observed), \
        'Missing configuration attempted an IPv4/IPv6 connection or datagram'
    print('PASS: owned native syscall trace contains no network join destination', flush=True)


def start(app, *, fresh_identity=True):
    app.assert_executable()
    app.assert_actual_desktop()
    app.launch()
    app.assert_room_entry(fresh_identity=fresh_identity)


def persisted_identity(app):
    identity = app.native_input.evaluate(app.observe_processes(), """(() => {
        const saved = JSON.parse(localStorage.getItem('petal:onboarding-session:v1'));
        return saved && {name: saved.name, participantId: saved.participantId};
    })()""")
    assert identity and identity['name'] == app.IDENTITY_FIXTURE and identity['participantId'], \
        'Native restart did not retain the real onboarding identity'
    return identity


def await_refusal_fixture(app, directory, participant_id):
    metadata = directory.lstat()
    assert stat.S_ISDIR(metadata.st_mode) and metadata.st_uid == os.getuid() \
        and stat.S_IMODE(metadata.st_mode) == 0o700, \
        'Refusal fixture directory must be an owned 0700 directory, not a symlink'
    ready = directory / 'ready.json'
    resume = directory / 'resume.json'
    assert not ready.exists() and not ready.is_symlink() \
        and not resume.exists() and not resume.is_symlink(), \
        'Refusal fixture handshake requires fresh ready/resume paths'
    with os.fdopen(os.open(ready, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600), 'w') as output:
        json.dump({'participantId': participant_id}, output)
    print('Awaiting parent denial of the actual onboarded identity before Join', flush=True)
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        assert app.process.poll() is None, 'Native app exited while awaiting its refusal fixture'
        app.observe_processes()
        try:
            descriptor = os.open(resume, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        except FileNotFoundError:
            time.sleep(0.1)
            continue
        with os.fdopen(descriptor) as incoming:
            metadata = os.fstat(incoming.fileno())
            assert stat.S_ISREG(metadata.st_mode) and metadata.st_uid == os.getuid() \
                and stat.S_IMODE(metadata.st_mode) == 0o600 and metadata.st_size <= 4096, \
                'Refusal resume must be an owned regular 0600 JSON file'
            payload = json.loads(incoming.read(4097))
        assert payload == {'participantId': participant_id}, \
            'Parent refusal acknowledgement does not match the actual onboarded identity'
        return
    raise AssertionError('Parent did not acknowledge the actual refused identity within 120 seconds')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--backend-url', required=True)
    parser.add_argument('--room-code', required=True)
    parser.add_argument('--expected-peer-name', required=True)
    parser.add_argument('--failure-kind', choices=['invalid', 'refused', 'missing'], required=True)
    parser.add_argument('--attempted-code')
    parser.add_argument('--refusal-fixture-dir', type=Path,
                        help='Private parent-owned handshake directory for a real server-side refusal')
    parser.add_argument('--timeout', type=float, default=45)
    parser.add_argument('--hold-seconds', type=float, default=0)
    parser.add_argument('--feature', type=Path)
    args = parser.parse_args()
    if args.failure_kind != 'missing' and not args.attempted_code:
        parser.error('invalid/refused requires an actual --attempted-code fixture')
    if (args.failure_kind == 'refused') != (args.refusal_fixture_dir is not None):
        parser.error('refused mode requires --refusal-fixture-dir; other modes must not use it')
    if args.feature:
        if not re.fullmatch(r'[a-z]{3}-[a-z]{4}-[a-z]{3}', args.room_code):
            parser.error('living recovery requires an accessible 3-4-3 room-code fixture')
        if args.failure_kind == 'invalid' and args.attempted_code != 'petal://join/000000':
            parser.error('invalid living row requires its explicit malformed invite, not meeting-name creation')
        if args.failure_kind == 'refused' and (
            not re.fullmatch(r'[a-z]{3}-[a-z]{4}-[a-z]{3}', args.attempted_code)
            or args.attempted_code == args.room_code
        ):
            parser.error('refused living row requires a distinct valid-format denied room fixture')
        if args.failure_kind == 'missing' and args.attempted_code is not None:
            parser.error('missing living scenario uses --room-code, not an attempted-code override')
        living_contract(args.feature, args.failure_kind)
    missing = args.failure_kind == 'missing'
    with hosted.launch.launch_scratch() as profile:
        with hosted.launch.NativeLaunch(args.executable, args.timeout,
                                       backend_url=None if missing else args.backend_url,
                                       trace_network=missing, profile_root=profile) as app:
            start(app)
            identity = persisted_identity(app)
            if args.failure_kind == 'refused':
                await_refusal_fixture(app, args.refusal_fixture_dir, identity['participantId'])
            offset = app.trace.stat().st_size if missing else 0
            enter_and_submit(app, args.room_code if missing else args.attempted_code)
            assert_failure(app, args.failure_kind)
            if missing:
                time.sleep(1)
                assert_no_network_join(app, offset)
            print('PASS: actionable native failure left no membership or remote window', flush=True)
            if not missing:
                recover(app, args)
        if missing:
            # Keep the same binary, HOME and XDG profile. Only the backend URL
            # changes; onboarding and the failed room record survive restart.
            with hosted.launch.NativeLaunch(args.executable, args.timeout,
                                           backend_url=args.backend_url, profile_root=profile) as app:
                start(app, fresh_identity=False)
                assert persisted_identity(app) == identity, 'Restart replaced the participant identity'
                print('PASS: the same isolated native profile and identity survived restart', flush=True)
                recover(app, args)


if __name__ == '__main__':
    main()
