#!/usr/bin/env python3
"""Exercise the Linux capability boundary in the production native executable."""
import argparse
import importlib.util
import json
from pathlib import Path
import time

spec = importlib.util.spec_from_file_location("native_launch", Path(__file__).with_name("check-linux-launch.py"))
launch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launch)

COMMANDS = [
    ("share_window", {"windowId": 427, "color": "#4f46e5"}),
    ("toggle_window_share", {"windowId": 427, "frame": {"x": 20, "y": 30, "width": 800, "height": 600}, "color": "#4f46e5"}),
    ("toggle_region_share", {"windowLabel": "region-427", "color": "#4f46e5"}),
    ("open_window_picker_window", {"color": "#4f46e5"}),
    ("toggle_window_picker_window", {}),
    ("open_region_window", {"userName": "Linux capability fixture", "followCursor": False}),
    ("open_system_content_picker", {}),
    ("set_share_audio_enabled", {"windowId": 427, "enabled": True}),
    ("set_region_share_audio", {"windowLabel": "region-427", "enabled": True}),
    ("start_camera_publish_command", {}),
    ("remote_control_set_active", {"windowId": 427, "ownerIdentity": "native-host", "active": True}),
    ("set_remote_control_allowed", {"allowed": True}),
    ("set_remote_control_policy", {"policy": "ask"}),
    ("set_share_remote_control_allowed", {"windowId": 427, "allowed": True}),
    ("set_share_control_mode", {"windowId": 427, "mode": "fullControl"}),
    ("remote_control_answer_consent", {"windowId": 427, "controllerId": "native-controller", "approve": True}),
    ("remote_control_answer_escalation", {"windowId": 427, "controllerId": "native-controller", "approve": True}),
    ("remote_control_request_escalation", {"windowId": 427, "ownerIdentity": "native-host"}),
    ("remote_clipboard_copy", {"windowId": 427, "ownerIdentity": "native-host", "grantToken": "native-grant-427"}),
    ("remote_clipboard_paste", {"windowId": 427, "ownerIdentity": "native-host", "grantToken": "native-grant-427"}),
    ("ai_chat_control_approve", {"sessionId": 427, "requestId": "native-request-427", "sessionScope": True}),
    ("ai_chat_control_resume", {"sessionId": 427}),
]
for event in [
    {"kind": "request"},
    {"kind": "text", "text": "Linux <input> Δ", "grantToken": "native-grant-427"},
    {"kind": "pointer", "action": "down", "x": 0.25, "y": 0.75, "button": 0, "grantToken": "native-grant-427"},
    {"kind": "key", "action": "down", "key": "a", "code": "KeyA", "grantToken": "native-grant-427"},
    {"kind": "wheel", "deltaX": 1, "deltaY": -2, "grantToken": "native-grant-427"},
]:
    COMMANDS.append(("remote_control_send", {"draft": {
        "windowId": 427, "targetOwnerId": "native-host", "seq": len(COMMANDS) + 1, **event,
    }}))

SCENARIO = "Unsupported publishing and input actions are unavailable"
TABLE = [
    ("action", "prohibited effect"),
    ("publish a screen share", "local screen media publication"),
    ("publish the camera", "local camera media publication"),
    ("request remote control", "control request sent to peer"),
    ("grant remote control", "local input injection enabled"),
    ("send remote input", "input event sent to peer"),
    ("replay remote input", "local OS input injected"),
]


def native_async(app, body):
    app.native_input.evaluate(app.observe_processes(), """(() => {
        window.__linuxCapabilityCheck = null;
        (async () => { %s })().then(
            value => { window.__linuxCapabilityCheck = {value}; },
            error => { window.__linuxCapabilityCheck = {error: String(error)}; });
        return true;
    })()""" % body)
    deadline = time.monotonic() + 10
    result = None
    while result is None and time.monotonic() < deadline:
        result = app.native_input.evaluate(app.observe_processes(), "window.__linuxCapabilityCheck")
        time.sleep(0.05)
    if result is None:
        diagnostic = app.native_input.evaluate(app.observe_processes(), """({
            route: location.pathname, visibility: document.visibilityState,
            state: typeof window.__linuxCapabilityCheck,
            native: typeof window.__TAURI_INTERNALS__
        })""")
        raise AssertionError(f"Native capability operation did not settle: {diagnostic}")
    assert "error" not in result, result
    return result["value"]


def assert_denials(app):
    results = native_async(app, """
        return await Promise.all(%s.map(async ([command, args]) => {
            try { return [command, 'resolved', await window.__TAURI_INTERNALS__.invoke(command, args)]; }
            catch (error) { return [command, 'rejected', String(error)]; }
        }));
    """ % json.dumps(COMMANDS))
    assert len(results) == len(COMMANDS)
    for command, status, result in results:
        assert status == "rejected" and "unsupported" in str(result).lower() and "linux" in str(result).lower(), \
            f"{command} must explicitly reject as unsupported on Linux, got {status}: {result}"


def inspect_room_controls(app):
    return native_async(app, """
        const inspected = new Promise((resolve, reject) => {
            const transitions = [];
            const errors = [];
            const onError = event => errors.push(String(event.message || event.reason));
            window.addEventListener('error', onError);
            window.addEventListener('unhandledrejection', onError);
            function cleanup() {
                observer.disconnect();
                window.removeEventListener('error', onError);
                window.removeEventListener('unhandledrejection', onError);
                clearTimeout(timer);
            }
            const timer = setTimeout(() => {
                inspect();
                cleanup();
                reject(JSON.stringify({message: 'Room controls did not appear', transitions, errors}));
            }, 5000);
            const observer = new MutationObserver(inspect);
            let moreClicked = false;
            function inspect() {
                const buttons = [...document.querySelectorAll('button')];
                const snapshot = {
                    route: location.pathname, visibility: document.visibilityState,
                    text: document.body.innerText.slice(0, 2000),
                    buttons: buttons.map(b => ({
                        aria: b.getAttribute('aria-label'), text: b.textContent.trim(),
                        disabled: b.disabled, display: getComputedStyle(b).display,
                        visibility: getComputedStyle(b).visibility,
                        rects: b.getClientRects().length
                    }))
                };
                if (JSON.stringify(snapshot) !== JSON.stringify(transitions.at(-1))) {
                    if (transitions.length === 16) transitions.shift();
                    transitions.push(snapshot);
                }
                const visible = b => b.getClientRects().length > 0 && getComputedStyle(b).visibility !== 'hidden';
                const label = b => b.getAttribute('aria-label') || b.textContent.trim();
                const more = buttons.find(b => visible(b) && label(b) === 'More meeting controls');
                if (more && !moreClicked) { moreClicked = true; more.click(); return; }
                const camera = buttons.find(b => visible(b) && label(b).startsWith('Turn camera on'));
                const share = buttons.find(b => visible(b) && label(b).startsWith('Share a window'));
                const mic = buttons.find(b => visible(b) && /^(Mute|Unmute) microphone$/.test(label(b)));
                if (!camera || !share || !mic) return;
                const actions = ['Request remote control', 'Grant remote control', 'Send remote input', 'Replay remote input'];
                const remote = actions.map(action => buttons.find(b => visible(b) && label(b).startsWith(action)));
                if (remote.some(b => !b)) return;
                const describe = b => ({
                    disabled: b.disabled,
                    label: b.getAttribute('aria-label') || b.textContent.trim(),
                    explanation: b.title,
                    visible: visible(b)
                });
                cleanup();
                resolve({camera: describe(camera), share: describe(share), mic: describe(mic),
                    remote: remote.map(describe), text: document.body.innerText, route: location.pathname,
                    diagnostic: {transitions, errors}});
            }
            observer.observe(document.body, {childList: true, subtree: true, attributes: true});
            inspect();
        });
        await window.__TAURI_INTERNALS__.invoke('open_main_route', {route: '/meeting/abc-defg-hjk'});
        return await inspected;
    """)


class CapabilityScenario:
    def __init__(self, app):
        self.app = app

    def running(self):
        self.app.assert_executable()
        self.app.assert_actual_desktop()
        self.app.launch()
        self.app.assert_room_entry()
        accepted = self.app.native_input.evaluate(self.app.observe_processes(), """(() => {
            const entry = document.querySelector('input[aria-label="Meeting name, invite link, or meeting code"]');
            if (!(entry instanceof HTMLInputElement)) throw Error('Production room input missing');
            entry.value = 'abc-defg-hjk';
            entry.dispatchEvent(new InputEvent('input', {bubbles: true, inputType: 'insertText', data: entry.value}));
            return entry.value === 'abc-defg-hjk' && !entry.disabled;
        })()""")
        assert accepted is True, "Production room-entry input did not accept the valid code"
        # As with the real name editor, observe the next DOM state in a separate
        # owned-inspector evaluation. Admission must not depend on animation
        # callbacks being scheduled while GTK/WebKit is between paints.
        self.entry = self.app.native_input.evaluate(self.app.observe_processes(), """(() => {
            const entry = document.querySelector('input[aria-label="Meeting name, invite link, or meeting code"]');
            const join = [...document.querySelectorAll('button')].find(b => b.textContent.trim() === 'Join');
            return {entry: !!entry && !entry.disabled, join: !!join && !join.disabled};
        })()""")

    def inspect_and_invoke(self):
        assert_denials(self.app)
        self.controls = inspect_room_controls(self.app)

    def unavailable(self):
        for control in [self.controls["camera"], self.controls["share"], *self.controls["remote"]]:
            assert control["disabled"] and control["visible"], {"control": control, "diagnostic": self.controls["diagnostic"]}
            assert "unsupported on Linux" in control["explanation"], control

    def errors(self):
        assert_denials(self.app)

    def no_effects(self):
        self.snapshot = native_async(self.app, """
            return {
                camera: await window.__TAURI_INTERNALS__.invoke('camera_publish_state'),
                room: await window.__TAURI_INTERNALS__.invoke('current_room'),
                presence: await window.__TAURI_INTERNALS__.invoke('room_presence')
            };
        """)
        assert self.snapshot["camera"]["intended"] is False, self.snapshot
        assert self.snapshot["camera"]["publishing"] is False, self.snapshot
        assert self.snapshot["room"] is None and self.snapshot["presence"] == [], self.snapshot

    def supported(self):
        assert self.entry["entry"] and self.entry["join"], self.entry
        assert not self.controls["mic"]["disabled"] and self.controls["mic"]["visible"], self.controls
        devices = native_async(self.app, """
            const devices = await window.__TAURI_INTERNALS__.invoke('list_audio_devices');
            const recording = devices.recording[0];
            const playout = devices.playout[0];
            if (!recording || !playout) throw Error('Real microphone/speaker device choices are unavailable');
            const applied = await window.__TAURI_INTERNALS__.invoke('set_audio_devices', {
                recordingId: recording.id, playoutId: playout.id
            });
            return {recording: recording.name, playout: playout.name, applied};
        """)
        assert devices["recording"] and devices["playout"], devices
        print("Capability-only evidence: controls inspected outside a connected room; "
              f"Meeting route evidence: {self.controls['route']}; failed-join state: "
              f"{'Could not join this room' in self.controls['text']}. "
              "hosted joining, working media and received-share rendering remain unvalidated.", flush=True)

    def invariant(self):
        assert_denials(self.app)
        self.no_effects()


STEP_BINDINGS = {
    "Given the native Linux app is running": "running",
    "When I inspect its room controls and invoke each unsupported command directly": "inspect_and_invoke",
    "Then each action in the following table is visibly unavailable with a Linux limitation explanation": "unavailable",
    "And each corresponding command returns an explicit unsupported error": "errors",
    "And none of the listed effects occurs": "no_effects",
    "And room joining, microphone controls, and receiving a share remain available": "supported",
    "And no unsupported command can publish local screen or camera media or inject input": "invariant",
}


def living_steps(feature):
    scenarios, steps, table = [], [], []
    for raw in Path(feature).read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith(('#', 'Feature:')):
            continue
        if line.startswith("Scenario:"):
            scenarios.append(line.removeprefix("Scenario:").strip())
        elif line.startswith("|"):
            table.append(tuple(cell.strip() for cell in line.strip("|").split("|")))
        else:
            assert line in STEP_BINDINGS, f"Unbound living step: {line}"
            steps.append(STEP_BINDINGS[line])
    assert scenarios == [SCENARIO], scenarios
    assert steps == list(STEP_BINDINGS.values()), steps
    assert table == TABLE, table
    return steps


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--feature")
    args = parser.parse_args()
    steps = living_steps(args.feature) if args.feature else list(STEP_BINDINGS.values())
    with launch.NativeLaunch(args.executable, args.timeout) as app:
        scenario = CapabilityScenario(app)
        for step in steps:
            getattr(scenario, step)()
    print("PASS: Linux native capability boundary (not hosted/media acceptance)")


if __name__ == "__main__":
    main()
