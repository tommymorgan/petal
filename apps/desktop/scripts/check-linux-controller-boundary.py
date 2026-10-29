#!/usr/bin/env python3
"""Task 3.1 controller-boundary smoke on a real joined hosted room.

Launches the native Linux app against the owned acceptance backend, joins a
fresh room through the real UI, then invokes the newly registered
controller-role commands directly: each must reject honestly at the
open-share/authority gate (no Windows grant exists in this room). Also
asserts the app stays healthy (room still current) afterwards. Host-side
commands still name the Linux limitation.
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

capabilities_spec = importlib.util.spec_from_file_location(
    "native_capabilities", Path(__file__).with_name("check-linux-capabilities.py")
)
capabilities = importlib.util.module_from_spec(capabilities_spec)
capabilities_spec.loader.exec_module(capabilities)


def native_async(app, body):
    return capabilities.native_async(app, body)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    parser.add_argument("--backend-url", required=True)
    parser.add_argument("--timeout", type=float, default=90)
    args = parser.parse_args()

    for attempt in range(5):
        try:
            return run(args)
        except AssertionError as error:
            if "unrelated process" not in str(error) or attempt == 4:
                raise
            time.sleep(1)


def run(args):
    with launch.NativeLaunch(args.executable, args.timeout, backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        room = f"t31-{int(time.time()) % 100000:05d}"
        app.native_input.evaluate(
            app.observe_processes(),
            f"""(() => {{
                const input = document.querySelector('input');
                if (!input) throw new Error('no join input');
                input.value = {json.dumps(room)};
                input.dispatchEvent(new Event('input', {{bubbles: true}}));
                return true;
            }})()""",
        )
        app.native_input.evaluate(
            app.observe_processes(),
            """(() => {
                const done = [...document.querySelectorAll('button')].find(b => /done|continue/i.test(b.textContent));
                if (!done) throw new Error('no continue button');
                done.click();
                return true;
            })()""",
        )
        deadline = time.monotonic() + 30
        joined = None
        while time.monotonic() < deadline:
            joined = native_async(
                app,
                "return await window.__TAURI_INTERNALS__.invoke('current_room');",
            )
            if joined and joined.get("name") == room:
                break
            time.sleep(0.5)
        assert joined and joined.get("name") == room, f"did not join {room}: {joined}"
        print(f"joined hosted room {room}", flush=True)

        checks = native_async(
            app,
            """
            const invoke = window.__TAURI_INTERNALS__.invoke;
            const out = {};
            out.send = await invoke('remote_control_send', {draft: {
                kind: 'pointer', action: 'down', windowId: 427,
                targetOwnerId: 'windows-peer', seq: 1, x: 0.5, y: 0.5, button: 0,
            }}).then(v => ['resolved', String(v)], e => ['rejected', String(e)]);
            out.setActive = await invoke('remote_control_set_active', {
                windowId: 427, ownerIdentity: 'windows-peer', active: true,
            }).then(v => ['resolved', String(v)], e => ['rejected', String(e)]);
            out.copy = await invoke('remote_clipboard_copy', {
                windowId: 427, ownerIdentity: 'windows-peer', grantToken: 'native-grant-427',
            }).then(v => ['resolved', String(v)], e => ['rejected', String(e)]);
            out.paste = await invoke('remote_clipboard_paste', {
                windowId: 427, ownerIdentity: 'windows-peer', grantToken: 'native-grant-427',
            }).then(v => ['resolved', String(v)], e => ['rejected', String(e)]);
            out.consent = await invoke('remote_control_answer_consent', {
                windowId: 427, controllerId: 'x', requestId: 'boundary-attempt', approve: true,
            }).then(v => ['resolved', String(v)], e => ['rejected', String(e)]);
            out.roomAfter = await invoke('current_room');
            return out;
            """,
        )
        for name in ("send", "setActive", "copy", "paste"):
            status, message = checks[name]
            message = message.lower()
            assert status == "rejected" and "is not open" in message, \
                f"{name} must reject at the open-share gate in-room: {checks[name]}"
            print(f"{name}: rejected honestly -> {checks[name][1]}", flush=True)
        status, message = checks["consent"]
        assert status == "rejected" and "unsupported on linux" in message.lower(), \
            f"host-side consent must keep naming the Linux limitation: {checks['consent']}"
        print(f"consent (host-role): {checks['consent'][1]}", flush=True)
        assert checks["roomAfter"] and checks["roomAfter"].get("name") == room, \
            f"room must survive the probes: {checks['roomAfter']}"
        print("room still current after probes", flush=True)
    print("PASS: controller commands reject honestly in a real joined room; host-role stays refused", flush=True)


if __name__ == "__main__":
    main()
