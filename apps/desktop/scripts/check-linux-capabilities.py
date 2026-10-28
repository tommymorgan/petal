#!/usr/bin/env python3
"""Exercise the Linux capability boundary in the production native executable."""
import argparse
import importlib.util
import json
from pathlib import Path
import socket
import tempfile
import time

spec = importlib.util.spec_from_file_location("native_launch", Path(__file__).with_name("check-linux-launch.py"))
launch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launch)

COMMANDS = [
    ("share_window", {"windowId": 427, "color": "#4f46e5"}),
    ("toggle_window_share", {"windowId": 427, "frame": {"x": 20, "y": 30, "width": 800, "height": 600}, "color": "#4f46e5"}),
    ("toggle_region_share", {"windowLabel": "region-427", "color": "#4f46e5"}),
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

SCENARIO = "Monitor admission and unfinished Linux capabilities are explicit"


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


def inspect_room_controls(app, *, navigate=True):
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
                const region = buttons.find(b => visible(b) && label(b).startsWith('Petal View'));
                const mic = buttons.find(b => visible(b) && /^(Mute|Unmute) microphone$/.test(label(b)));
                if (!camera || !share || !mic || !region) return;
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
                resolve({camera: describe(camera), share: describe(share), mic: describe(mic), region: describe(region),
                    remote: remote.map(describe), text: document.body.innerText, route: location.pathname,
                    diagnostic: {transitions, errors}});
            }
            observer.observe(document.body, {childList: true, subtree: true, attributes: true});
            inspect();
        });
        if (%s) await window.__TAURI_INTERNALS__.invoke('open_main_route', {route: '/meeting/abc-defg-hjk'});
        return await inspected;
    """ % json.dumps(navigate))


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

    def monitor_available(self):
        share = self.controls["share"]
        assert share["visible"] and not share["disabled"], \
            {"control": share, "diagnostic": self.controls["diagnostic"]}
        assert "unsupported" not in share["explanation"].lower(), share

    def monitor_admission(self):
        assert_monitor_start_requires_room(self.app, launch_app=False)

    def unavailable(self):
        for control in [self.controls["camera"], self.controls["region"], *self.controls["remote"]]:
            assert control["disabled"] and control["visible"], {"control": control, "diagnostic": self.controls["diagnostic"]}
            assert "unsupported on Linux" in control["explanation"], control

    def errors(self):
        assert_denials(self.app)

    def no_effects(self):
        self.snapshot = native_async(self.app, """
            return {
                camera: await window.__TAURI_INTERNALS__.invoke('camera_publish_state'),
                room: await window.__TAURI_INTERNALS__.invoke('current_room'),
                presence: await window.__TAURI_INTERNALS__.invoke('room_presence'),
                shares: await window.__TAURI_INTERNALS__.invoke('shared_window_ids'),
            };
        """)
        assert self.snapshot["camera"]["intended"] is False, self.snapshot
        assert self.snapshot["camera"]["publishing"] is False, self.snapshot
        assert self.snapshot["room"] is None and self.snapshot["presence"] == [], self.snapshot
        assert self.snapshot["shares"] == [], self.snapshot

    def supported(self):
        assert self.entry["entry"] and self.entry["join"], self.entry
        assert not self.controls["mic"]["disabled"] and self.controls["mic"]["visible"], self.controls
        print("Capability-only evidence: controls inspected outside a connected room; "
              f"Meeting route evidence: {self.controls['route']}; failed-join state: "
              f"{'Could not join this room' in self.controls['text']}. "
              "hosted joining, working media and received-share rendering remain unvalidated.", flush=True)

    def invariant(self):
        assert_denials(self.app)
        self.no_effects()


def assert_monitor_start_requires_room(app, *, launch_app=True):
    """S01 entry boundary, not capture or Windows-rendering acceptance.

    Invoke the same native picker command used by sharing consumers, with no
    room and no selected source. Do not navigate to a meeting, join a service,
    select content, or open any media device to obtain this first RED.
    """
    if launch_app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
    before = native_async(app, """
        return await window.__TAURI_INTERNALS__.invoke('current_room');
    """)
    assert before is None, "Monitor-entry fixture must not have a joined room"
    outcomes = native_async(app, """
        return await Promise.all(['toggle_window_picker_window', 'open_window_picker_window'].map(async command => {
            try {
                await window.__TAURI_INTERNALS__.invoke(command, {});
                return {command, status: 'resolved'};
            } catch (error) {
                return {command, status: 'rejected', error: String(error)};
            }
        }));
    """)
    for outcome in outcomes:
        assert outcome["status"] == "rejected", \
            f"{outcome['command']}: monitor selection must require a joined room before capture permission"
        error = outcome["error"].lower()
        assert "room" in error and "unsupported" not in error and "not found" not in error, \
            (f"{outcome['command']}: native monitor selection must report the missing room, not blanket "
             f"Linux unsupported or an unregistered command: {outcome['error']}")
    after = native_async(app, """
        return await window.__TAURI_INTERNALS__.invoke('current_room');
    """)
    assert after is None, "Opening monitor sharing must not join a room implicitly"


def run_x03(app, observe_participant, approve_share, cancel_share, windows_vm, evidence_dir, timeout=90):
    """Exercise this unit's connected native UI/direct-command boundary.

    The caller owns the existing room, muted peer and explicit safe-monitor
    permission. This does not launch participants or authorize camera/input.
    """
    spec = importlib.util.spec_from_file_location(
        'capability_monitor_witness', Path(__file__).with_name('check-linux-monitor-publication.py'))
    monitor = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(monitor)
    audio = monitor.audio
    room = audio.invoke(app, 'current_room')
    assert room is not None and audio.invoke(app, 'shared_window_ids') == []
    microphone = audio.wait_microphone(observe_participant)
    assert microphone.get('muted', False), 'X03 requires an already-muted microphone'
    witness = audio.AudioWitness(app, observe_participant, True, microphone['sid'])
    boundary = CapabilityScenario(app)
    boundary.controls = inspect_room_controls(app, navigate=False)
    boundary.monitor_available()
    boundary.unavailable()
    boundary.errors()
    witness.settled(False)
    camera = audio.invoke(app, 'camera_publish_state')
    assert camera['intended'] is False and camera['publishing'] is False
    try:
        picture = monitor.run_s01(app, observe_participant, approve_share,
                                  windows_vm, evidence_dir, timeout=timeout)
        # Wait for any earlier informational toast to retire. The next warning
        # must belong to the real rejected UI selection, not a stale failure.
        deadline = time.monotonic() + 10
        warning = """[...document.querySelectorAll('.toast-anchor')].some(
            el => el.getClientRects().length > 0 && el.querySelector('.icon.warning'))"""
        while audio.evaluate(app, warning):
            assert time.monotonic() < deadline, 'Earlier warning obscures the new failure observation'
            time.sleep(.1)
        audio.evaluate(app, """(() => {
            const share = document.querySelector('button[aria-label="Share a window"]');
            if (!share || share.disabled || !share.getClientRects().length)
                throw Error('Native Share action is unavailable');
            share.click();
            return true;
        })()""")
        cancel_share('X03-visible-permission-failure')
        deadline = time.monotonic() + 10
        while not audio.evaluate(app, warning):
            witness.sample()
            assert time.monotonic() < deadline, 'Rejected native selection did not render a visible failure warning'
            time.sleep(.1)
        witness.settled(False)
        boundary.errors()
        witness.settled(False)
        camera = audio.invoke(app, 'camera_publish_state')
        assert camera['intended'] is False and camera['publishing'] is False
        assert audio.invoke(app, 'current_room') == room, 'Capability checks changed the original room'
        return {'scenario': 'X03', 'nativePicture': picture,
                'unfinishedCommandsRejected': len(COMMANDS),
                'visiblePermissionFailure': True,
                'originalMicrophoneRetained': True, 'cameraAndInputActivated': False}
    finally:
        witness.action('stop_all_shares', 'X03-owned-picture-cleanup', timeout=timeout)
        witness.settled(False)


def read_native_prerequisites(app):
    """Read the production diagnostic without joining or selecting content."""
    app.assert_executable()
    app.assert_actual_desktop()
    app.launch()
    app.assert_room_entry()
    outcome = native_async(app, """
        try {
            const report = await window.__TAURI_INTERNALS__.invoke('linux_native_prerequisites', {});
            return {status: 'resolved', report};
        } catch (error) {
            return {status: 'rejected', error: String(error)};
        }
    """)
    assert outcome["status"] == "resolved", \
        ("X04 native prerequisite diagnostic must resolve without opening devices; "
         f"got {outcome}")
    report = outcome["report"]
    assert isinstance(report, dict) and isinstance(report.get("components"), list), \
        f"Native prerequisite diagnostic returned no component report: {report}"
    return report


def assert_missing_capture_prerequisite(app):
    """A real empty plugin search path must produce a useful native diagnosis."""
    report = read_native_prerequisites(app)
    pipewire = [component for component in report["components"] if component.get("name") == "pipewiresrc"]
    assert len(pipewire) == 1, f"Native report must identify the PipeWire capture element: {report}"
    pipewire = pipewire[0]
    assert pipewire.get("available") is False, \
        f"Empty native plugin directory must make pipewiresrc unavailable: {pipewire}"
    assert "pipewiresrc" in pipewire.get("reason", ""), \
        f"Missing-component diagnosis must name what is missing: {pipewire}"
    assert "monitor" in pipewire.get("effect", "").lower(), \
        f"Missing-component diagnosis must explain its effect: {pipewire}"
    assert report.get("capabilities") == {"monitor": False, "camera": False, "input": False}, report
    scenario = CapabilityScenario(app)
    scenario.no_effects()
    app.observe_processes()


STEP_BINDINGS = {
    "Given the native Linux app is running": "running",
    "When I inspect its room controls and invoke unfinished commands directly": "inspect_and_invoke",
    "Then the monitor Share action is visibly enabled": "monitor_available",
    "And monitor selection without a room reports the missing room before requesting permission": "monitor_admission",
    "And camera, region and input controls remain unavailable with Linux explanations": "unavailable",
    "And unfinished window, region, audio, camera and input commands return explicit unsupported errors": "errors",
    "And no room, picture or camera publication is created": "no_effects",
    "And room-entry and microphone controls remain available": "supported",
    "And repeated unfinished actions leave the native app without publication or input effects": "invariant",
}


def living_steps(feature):
    steps, selected, found = [], False, 0
    for raw in Path(feature).read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith(('#', 'Feature:')):
            continue
        if line.startswith(('Scenario:', 'Scenario Outline:')):
            kind, _, name = line.partition(':')
            selected = name.strip() == SCENARIO
            if selected:
                assert kind == 'Scenario', 'Capability boundary is not an outline'
                found += 1
            continue
        if selected:
            assert line in STEP_BINDINGS, f"Unbound living step: {line}"
            steps.append(STEP_BINDINGS[line])
    assert found == 1, 'Select exactly one capability-boundary scenario'
    assert steps == list(STEP_BINDINGS.values()), steps
    return steps


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--feature")
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--monitor-entry", action="store_true",
                       help="Check S01 no-room monitor-start admission without selecting content")
    modes.add_argument("--prerequisites", action="store_true",
                       help="Check X04 missing capture-plugin diagnosis with a private empty registry")
    modes.add_argument("--prerequisite-report", action="store_true",
                       help="Report the real native prerequisites without selecting devices; exit 1 if unavailable")
    args = parser.parse_args()
    if args.feature and (args.monitor_entry or args.prerequisites or args.prerequisite_report):
        parser.error("Focused admission/prerequisite checks do not bind a complete living scenario")
    # Bind but do not listen. A meeting-route mount gets connection refused,
    # not a token from any real backend; it cannot start room audio/media.
    with socket.socket() as rejected_backend:
        rejected_backend.bind(("127.0.0.1", 0))
        backend = f"http://127.0.0.1:{rejected_backend.getsockname()[1]}"
        if args.prerequisites:
            with tempfile.TemporaryDirectory(prefix="petal-empty-capture-plugins-") as empty_plugins:
                with launch.NativeLaunch(args.executable, args.timeout, backend_url=backend,
                                         forbid_media=True, capture_plugin_directory=empty_plugins) as app:
                    assert_missing_capture_prerequisite(app)
            print("PASS: missing native capture component diagnosed without portal/media/input activation")
            return
        with launch.NativeLaunch(args.executable, args.timeout, backend_url=backend, forbid_media=True) as app:
            if args.prerequisite_report:
                report = read_native_prerequisites(app)
                CapabilityScenario(app).no_effects()
                app.observe_processes()
                print(json.dumps(report, indent=2))
                available = report.get("capabilities", {}).get("monitor") is True
            else:
                available = None
            if args.monitor_entry:
                assert_monitor_start_requires_room(app)
                print("PASS: S01 native monitor-start room admission only; media/Windows proof remains separate")
                return
            if available is None:
                steps = living_steps(args.feature) if args.feature else list(STEP_BINDINGS.values())
                scenario = CapabilityScenario(app)
                for step in steps:
                    getattr(scenario, step)()
        if available is not None:
            if not available:
                raise SystemExit(1)
            return
    print("PASS: native monitor admission and unfinished capability boundary; not hosted/media acceptance")




if __name__ == "__main__":
    main()
