#!/usr/bin/env python3
"""Drive hosted membership through the actual native Linux window.

Default mode diagnoses entry only and never submits. --join requires an
explicit backend and a real authorized native Windows peer already in that
room. Peer-side participant observation remains the parent's responsibility.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import time

spec = importlib.util.spec_from_file_location(
    "native_launch", Path(__file__).with_name("check-linux-launch.py")
)
launch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launch)

capability_spec = importlib.util.spec_from_file_location(
    "native_capabilities", Path(__file__).with_name("check-linux-capabilities.py")
)
capabilities = importlib.util.module_from_spec(capability_spec)
capability_spec.loader.exec_module(capabilities)


HOSTED_SCENARIO = "Join a hosted room with the Windows native peer"
HOSTED_STEPS = (
    "Given a released native Petal <peer> client is in hosted room abc-defg-hjk",
    "And Linux is configured explicitly for that same hosted Petal service",
    "And no LiveKit server API secret is present or required on the Linux client",
    "When I enter room code abc-defg-hjk in the native Linux app and join",
    "Then Linux joins hosted room abc-defg-hjk without changing the peer client's server",
    "And Linux and the <peer> client each show the other participant in the room",
    "Examples:",
    "| peer |",
    "| Windows |",
)


def living_contract(feature):
    """Select the exact implemented outline, including its complete peer set.

    The named room code is a fixture slot: --room-code supplies an authorized
    live room. Peer-side observations are collected separately by the parent,
    never inferred from Linux's roster or a fabricated peer.
    """
    selected = False
    seen_feature = False
    found = []
    for line in Path(feature).read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("Feature:"):
            seen_feature = True
            continue
        assert not line.startswith(("Background:", "Rule:")), \
            "Hosted living checks do not bind Background or Rule clauses"
        if line.startswith(("Scenario:", "Scenario Outline:")):
            kind, _, name = line.partition(":")
            selected = name.strip() == HOSTED_SCENARIO
            assert not selected or kind == "Scenario Outline", \
                "Hosted living checks must retain their complete peer outline"
            continue
        if selected:
            found.append(" ".join(line.split()) if line.startswith("|") else line)
    assert seen_feature and tuple(found) == HOSTED_STEPS, \
        "Hosted living scenario must bind every exact acceptance clause and the Windows example"


def assert_join_code(app, room_code):
    accepted = app.native_input.evaluate(app.observe_processes(), """(() => {
        const input = document.querySelector('input[aria-label="Meeting name, invite link, or meeting code"]');
        if (!(input instanceof HTMLInputElement) || input.disabled || input.readOnly) return false;
        input.focus();
        input.value = %s;
        input.dispatchEvent(new InputEvent('input', {
            bubbles: true, inputType: 'insertText', data: input.value
        }));
        return input.value === %s;
    })()""" % (json.dumps(room_code), json.dumps(room_code)))
    assert accepted is True, "Native production room input did not accept the typed fixture"
    deadline = time.monotonic() + 3
    action = None
    while time.monotonic() < deadline:
        action = app.native_input.evaluate(app.observe_processes(), """(() => {
            const input = document.querySelector('input[aria-label="Meeting name, invite link, or meeting code"]');
            const button = input?.closest('form')?.querySelector('button');
            return button ? {label: button.textContent.trim(), disabled: button.disabled} : null;
        })()""")
        if action == {"label": "Join", "disabled": False}:
            return
        time.sleep(0.1)
    assert action == {"label": "Join", "disabled": False}, (
        "Hosted room-code input must offer Join, not creation of another room; "
        f"native action was {action}. No action was submitted."
    )


def inspect_join_command(app):
    # Missing required roomName prevents a registered command from reaching
    # token/connect code. Do not supply any real room or participant identity.
    app.native_input.evaluate(app.observe_processes(), """(() => {
        window.__linuxHostedCommand = null;
        window.__TAURI_INTERNALS__.invoke('join_room_command', {}).then(
            () => { window.__linuxHostedCommand = {outcome: 'resolved-invalid-arguments'}; },
            error => {
                const message = String(error);
                const outcome = /not found|unknown command/i.test(message) ? 'unregistered'
                    : /unsupported/i.test(message) ? 'unsupported'
                    : /roomName|room_name/.test(message) ? 'required-room-argument-rejected'
                    : 'other-rejection';
                window.__linuxHostedCommand = {outcome};
            });
        return true;
    })()""")
    deadline = time.monotonic() + 5
    outcome = None
    while outcome is None and time.monotonic() < deadline:
        outcome = app.native_input.evaluate(app.observe_processes(), "window.__linuxHostedCommand")
        time.sleep(0.1)
    print(f"Native join command diagnostic (not scenario RED): {outcome or 'did not settle'}")


def native_async(app, body):
    app.native_input.evaluate(app.observe_processes(), """(() => {
        window.__linuxHostedResult = null;
        (async () => { %s })().then(
            value => { window.__linuxHostedResult = {value}; },
            () => { window.__linuxHostedResult = {error: true}; });
        return true;
    })()""" % body)
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        result = app.native_input.evaluate(app.observe_processes(), "window.__linuxHostedResult")
        if result is not None:
            assert not result.get("error"), "Native hosted observation IPC rejected"
            return result["value"]
        time.sleep(0.05)
    raise AssertionError("Native hosted observation IPC did not settle")


def membership_snapshot(app, peer_name):
    return native_async(app, """
        const [room, roster] = await Promise.all([
            window.__TAURI_INTERNALS__.invoke('current_room'),
            window.__TAURI_INTERNALS__.invoke('room_presence')
        ]);
        const peerName = %s;
        const localName = %s;
        const visibleTiles = [...document.querySelectorAll('[data-participant-key]')]
            .filter(node => node.getClientRects().length > 0
                && getComputedStyle(node).visibility !== 'hidden');
        const locals = roster.filter(p => p.isLocal && p.name === localName);
        const peers = roster.filter(p => !p.isLocal && p.name === peerName);
        // Gallery uses presence.identity as data-participant-key. Resolve
        // separate real participants before checking each tile's exact label:
        // a peer named "Linux" must not match the local fixture's text.
        const visibleParticipant = (participants, displayName) => {
            if (participants.length !== 1) return null;
            const tiles = visibleTiles.filter(
                node => node.dataset.participantKey === participants[0].identity
            );
            if (tiles.length !== 1) return null;
            const tile = tiles[0];
            const labels = tile.querySelectorAll(
                '.camera-off-name.active[aria-label], .name-chip[title]'
            );
            return [...labels].some(label =>
                (label.getAttribute('aria-label') ?? label.getAttribute('title')) === displayName
            ) ? tile : null;
        };
        const localTile = visibleParticipant(locals, localName + ' (you)');
        const peerTile = visibleParticipant(peers, peerName);
        return {
            inRoom: room !== null,
            routeMatchesRoom: room !== null && location.pathname === '/meeting/' + encodeURIComponent(room),
            oneLocal: locals.length === 1,
            onePeer: peers.length === 1,
            localVisible: localTile !== null,
            peerVisible: peerTile !== null && peerTile !== localTile,
            terminal: document.body.innerText.includes('Could not join this room')
        };
    """ % (json.dumps(peer_name), json.dumps(app.IDENTITY_FIXTURE)))


def join_hosted_room(app, peer_name):
    clicked = app.native_input.evaluate(app.observe_processes(), r"""(() => {
        const input = document.querySelector('input[aria-label="Meeting name, invite link, or meeting code"]');
        const button = input?.closest('form')?.querySelector('button');
        if (!button || button.disabled || button.textContent.trim() !== 'Join') return false;
        // Observe production failures without changing invocation or recovery.
        const redact = value => String(value)
            .replace(/[a-z]{3}-[a-z]{4}-[a-z]{3}/g, '<room-code>')
            .replace(/eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g, '<token>')
            .replace(/Bearer\s+\S+/gi, 'Bearer <redacted>')
            .replace(/([?&](?:token|access_token|key|secret)=)[^&\s]+/gi, '$1<redacted>')
            .slice(0, 1500);
        const originalError = console.error;
        window.__linuxHostedConsole = [];
        console.error = function(...args) {
            if (window.__linuxHostedConsole.length < 8) {
                window.__linuxHostedConsole.push(args.map(redact).join(' '));
            }
            return originalError.apply(this, args);
        };
        window.__linuxHostedDiagnostic = () => ({
            path: location.pathname.startsWith('/meeting/') ? '/meeting/<room>' : location.pathname,
            body: redact(document.body.innerText),
            errors: window.__linuxHostedConsole
        });
        button.click();
        return true;
    })()""")
    assert clicked is True, "Actual native Join action was not available"
    deadline = time.monotonic() + app.timeout
    snapshot = None
    while time.monotonic() < deadline:
        snapshot = membership_snapshot(app, peer_name)
        if snapshot["terminal"]:
            break
        if all(snapshot[key] for key in (
            "inRoom", "routeMatchesRoom", "oneLocal", "onePeer", "localVisible", "peerVisible"
        )):
            print("PASS: native Linux hosted room and rendered local/Windows peer participants", flush=True)
            return
        time.sleep(0.2)
    diagnostic = app.native_input.evaluate(
        app.observe_processes(), "window.__linuxHostedDiagnostic()"
    )
    raise AssertionError(
        "Native Linux must join the hosted room and show the real Windows peer "
        f"in native IPC and rendered participant tiles; observed {snapshot}; diagnostic {diagnostic}"
    )


def leave_hosted_room(app):
    clicked = app.native_input.evaluate(app.observe_processes(), """(() => {
        const button = document.querySelector('button[aria-label="Leave meeting"]');
        if (!button || button.disabled) return false;
        button.click();
        return true;
    })()""")
    assert clicked is True, "Actual native Leave meeting action was not available"
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        left = native_async(app, """
            const room = await window.__TAURI_INTERNALS__.invoke('current_room');
            const roster = await window.__TAURI_INTERNALS__.invoke('room_presence');
            return room === null && roster.length === 0 && location.pathname === '/main';
        """)
        if left:
            print("PASS: native Leave returned to room entry with no retained membership", flush=True)
            return
        time.sleep(0.2)
    raise AssertionError("Native Leave retained membership or failed to return to room entry")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--room-code", required=True)
    parser.add_argument("--timeout", type=float, default=45)
    parser.add_argument("--inspect-join-command", action="store_true")
    parser.add_argument("--join", action="store_true", help="Join an authorized real peer's hosted room")
    parser.add_argument("--backend-url")
    parser.add_argument("--expected-peer-name")
    parser.add_argument("--hold-seconds", type=float, default=0, help="Keep Linux joined for peer-side observation")
    parser.add_argument("--feature", type=Path, help="Bind the exact Task 1.3 hosted membership outline")
    args = parser.parse_args()
    if args.join and (not args.backend_url or not args.expected_peer_name):
        parser.error("--join requires --backend-url and --expected-peer-name")
    if args.feature:
        if not args.join:
            parser.error("--feature requires --join and its real hosted peer inputs")
        living_contract(args.feature)
    with launch.NativeLaunch(args.executable, args.timeout, backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        assert_join_code(app, args.room_code)
        if args.inspect_join_command:
            inspect_join_command(app)
        if args.join:
            join_hosted_room(app, args.expected_peer_name)
            capabilities.assert_denials(app)
            print("PASS: unsupported publishing/control commands remain denied while joined", flush=True)
            print("Awaiting parent Windows-side participant observation during hold", flush=True)
            deadline = time.monotonic() + args.hold_seconds
            while time.monotonic() < deadline:
                app.observe_processes()
                time.sleep(0.2)
            leave_hosted_room(app)
    print("PASS: actual native room entry offers Join for the provided room code")
    if args.join:
        print("Linux-side hosted membership checked; Windows-side observation must be reported separately")
    else:
        print("Diagnostic only: no hosted join, authentication or native peer presence was exercised")


if __name__ == "__main__":
    main()
