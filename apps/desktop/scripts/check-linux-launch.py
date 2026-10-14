#!/usr/bin/env python3
"""Drive the production Linux executable in the real desktop, never a browser.

Usage from repository root:
  /usr/bin/python3 apps/desktop/scripts/check-linux-launch.py \
    apps/desktop/src-tauri/target/debug/desktop

Build separately with npm run build and cargo build --bin desktop. This check
uses a fresh Linux-only home, real AT-SPI controls, and an owned exec trace.
It does not enable Petal autotest hooks or seed application stores. A private
session-bus monitor also checks owned browser activation requests.
"""

import argparse
from contextlib import ExitStack, contextmanager
import json
import os
from pathlib import Path
import queue
import re
import signal
import shutil
import socket
import subprocess
import sys
import tempfile
import time

import gi

gi.require_version("Atspi", "2.0")
gi.require_version("Soup", "3.0")
from gi.repository import Atspi, Gio, GLib, Soup


KNOWN_CONTROLS = {
    "Continue", "Your name", "Done", "User profile",
    "Meeting name, invite link, or meeting code", "Create/Join",
}
BROWSER_EXEC = re.compile(
    r'execve\("[^"\n]*/(?:xdg-open|gio|firefox(?:-bin)?|chromium(?:-browser)?|'
    r'google-chrome(?:-stable)?|brave(?:-browser)?|microsoft-edge(?:-stable)?|'
    r'sensible-browser|x-www-browser)"'
)
FOREIGN_EXEC = re.compile(
    r'execve\("[^"\n]*/(?:osascript|xcrun|xcodebuild|codesign|spctl|hdiutil|'
    r'powershell(?:\.exe)?|pwsh|msbuild(?:\.exe)?|signtool(?:\.exe)?|'
    r'cmd\.exe|wine|wine64)"'
)


def descendants(root):
    """Read only child lists of processes owned by this launch."""
    result = {root}
    pending = [root]
    while pending:
        pid = pending.pop()
        try:
            children = Path(f"/proc/{pid}/task/{pid}/children").read_text().split()
        except FileNotFoundError:
            continue
        for child in map(int, children):
            if child not in result:
                result.add(child)
                pending.append(child)
    return result


class NativeInput:
    """Target this launch's WebKit inspector, never the desktop keyboard."""

    def __init__(self):
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            self.port = reservation.getsockname()[1]

    def owns_listener(self, pids):
        address = f"0100007F:{self.port:04X}"
        listeners = {
            row[9] for line in Path("/proc/net/tcp").read_text().splitlines()[1:]
            if (row := line.split())[1] == address and row[3] == "0A"
        }
        if not listeners:
            return False
        for pid in pids:
            try:
                for descriptor in Path(f"/proc/{pid}/fd").iterdir():
                    try:
                        target = os.readlink(descriptor)
                    except FileNotFoundError:
                        continue
                    if target.startswith("socket:[") and target[8:-1] in listeners:
                        return True
            except (FileNotFoundError, PermissionError):
                continue
        raise AssertionError("Native inspector port belongs to an unrelated process")

    @staticmethod
    def wait_for(predicate, message):
        deadline = time.monotonic() + 5
        context = GLib.MainContext.default()
        while not predicate():
            assert time.monotonic() < deadline, message
            while context.pending():
                context.iteration(False)
            time.sleep(0.01)

    def fill_name(self, pids, name):
        self.wait_for(lambda: self.owns_listener(pids), "Owned native inspector did not listen")
        session = Soup.Session(timeout=2)
        session.set_proxy_resolver(None)
        connection = None
        try:
            request = Soup.Message.new("GET", f"http://127.0.0.1:{self.port}/")
            page = session.send_and_read(request, None).get_data().decode()
            assert request.get_status() == 200, "Native inspector target discovery failed"
            targets = set(re.findall(r"/socket/\d+/\d+/WebPage", page))
            assert len(targets) == 1, "Expected exactly one owned native WebKit page"
            route = targets.pop()
            replies = {}
            page_targets = set()
            connected = []

            def received(_connection, _kind, payload):
                message = json.loads(payload.get_data())
                if message.get("method") == "Target.targetCreated":
                    target = message["params"]["targetInfo"]
                    if target["type"] == "page" and not target.get("isProvisional", False):
                        page_targets.add(target["targetId"])
                elif message.get("method") == "Target.dispatchMessageFromTarget":
                    reply = json.loads(message["params"]["message"])
                    if "id" in reply:
                        replies[reply["id"]] = reply
                elif "id" in message and "error" in message:
                    replies["transport_error"] = True

            def ready(client, result, _data):
                try:
                    web_socket = client.websocket_connect_finish(result)
                    web_socket.connect("message", received)
                    connected.append(web_socket)
                except GLib.Error as error:
                    connected.append(error)

            assert self.owns_listener(pids), "Native inspector listener lost its owner"
            session.websocket_connect_async(
                Soup.Message.new("GET", f"ws://127.0.0.1:{self.port}{route}"),
                None, None, GLib.PRIORITY_DEFAULT, None, ready, None,
            )
            self.wait_for(lambda: connected, "Native inspector WebSocket did not connect")
            assert not isinstance(connected[0], GLib.Error), "Native inspector WebSocket was rejected"
            connection = connected[0]
            self.wait_for(lambda: page_targets, "Native inspector did not identify its page target")
            assert len(page_targets) == 1, "Native inspector exposed ambiguous page targets"
            # Exercise the production DOM input handler, not Svelte stores,
            # onboarding flags, native command mocks, or validation overrides.
            expression = """(() => {
                if (location.protocol !== 'tauri:' || location.hostname !== 'localhost') return false;
                const input = document.querySelector('input[aria-label="Your name"]');
                if (!(input instanceof HTMLInputElement) || input.disabled || input.readOnly) return false;
                input.focus();
                input.value = %s;
                input.dispatchEvent(new InputEvent('input', {
                    bubbles: true, inputType: 'insertText', data: input.value
                }));
                return input.value === %s;
            })()""" % (json.dumps(name), json.dumps(name))
            command = {"id": 1, "method": "Runtime.evaluate",
                       "params": {"expression": expression, "returnByValue": True}}
            assert self.owns_listener(pids), "Native inspector listener lost its owner"
            connection.send_text(json.dumps({
                "id": 2, "method": "Target.sendMessageToTarget",
                "params": {"targetId": page_targets.pop(), "message": json.dumps(command)},
            }))
            self.wait_for(lambda: 1 in replies or "transport_error" in replies,
                          "Native inspector did not acknowledge input")
            reply = replies.get(1, {})
            assert "error" not in reply and reply.get("result", {}).get("result", {}).get("value") is True, \
                "Native production name input handler did not accept the edit"
        finally:
            if connection is not None:
                connection.close(1000, "Native launch input complete")
            session.abort()


class BrowserActivations:
    """Observe relevant message headers, never desktop message bodies."""

    def __init__(self, address):
        self.address = address
        self.monitor = None
        self.query = None
        self.pending = queue.SimpleQueue()
        self.sender_pids = {}

    def __enter__(self):
        flags = Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION
        try:
            self.monitor = Gio.DBusConnection.new_for_address_sync(self.address, flags, None, None)
            self.query = Gio.DBusConnection.new_for_address_sync(self.address, flags, None, None)
            self.monitor.set_exit_on_close(False)
            self.query.set_exit_on_close(False)

            def header_only(_connection, message, incoming, _data):
                if incoming and message.get_message_type() == Gio.DBusMessageType.METHOD_CALL:
                    interface = message.get_interface()
                    member = message.get_member()
                    if (interface == "org.freedesktop.portal.OpenURI"
                            or (interface == "org.freedesktop.Application"
                                and member in ("Open", "Activate", "ActivateAction"))
                            or (interface == "org.gtk.Actions" and member == "Activate")):
                        self.pending.put(message.get_sender())
                    # A monitor must not reply to another connection's calls.
                    return None
                return message

            self.monitor.add_filter(header_only, None)
            rules = [
                "type='method_call',interface='org.freedesktop.portal.OpenURI'",
                "type='method_call',interface='org.freedesktop.Application',member='Open'",
                "type='method_call',interface='org.freedesktop.Application',member='Activate'",
                "type='method_call',interface='org.freedesktop.Application',member='ActivateAction'",
                "type='method_call',interface='org.gtk.Actions',member='Activate'",
            ]
            self.monitor.call_sync(
                "org.freedesktop.DBus", "/org/freedesktop/DBus",
                "org.freedesktop.DBus.Monitoring", "BecomeMonitor",
                GLib.Variant("(asu)", (rules, 0)), None, Gio.DBusCallFlags.NONE, 5000, None,
            )
            return self
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def check(self, owned_pids):
        assert not self.monitor.is_closed(), "Browser activation monitor disconnected"
        while True:
            try:
                sender = self.pending.get_nowait()
            except queue.Empty:
                break
            assert sender, "Browser activation request had no attributable sender"
            if sender not in self.sender_pids:
                try:
                    reply = self.query.call_sync(
                        "org.freedesktop.DBus", "/org/freedesktop/DBus",
                        "org.freedesktop.DBus", "GetConnectionUnixProcessID",
                        GLib.Variant("(s)", (sender,)), None,
                        Gio.DBusCallFlags.NONE, 1000, None,
                    )
                except GLib.Error:
                    raise AssertionError("Could not attribute a browser activation request during launch") from None
                self.sender_pids[sender] = reply.unpack()[0]
            assert self.sender_pids[sender] not in owned_pids, "Owned startup activated a browser or URI opener through the desktop bus"

    def __exit__(self, _kind, _value, _traceback):
        for connection in (self.monitor, self.query):
            if connection is not None and not connection.is_closed():
                connection.close_sync(None)


def controls(pids):
    found = {}
    native_window = False
    # Deliver accessibility state/children changes before reading cached nodes.
    context = GLib.MainContext.default()
    while context.pending():
        context.iteration(False)
    desktop = Atspi.get_desktop(0)
    for index in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(index)
        if app is None or app.get_process_id() not in pids:
            continue
        pending = [app]
        visited = 0
        while pending:
            node = pending.pop()
            if node is None:
                continue
            node.clear_cache()
            visited += 1
            if visited > 4000:
                raise AssertionError("Owned Petal accessibility tree exceeded traversal bound")
            states = node.get_state_set()
            showing = states.contains(Atspi.StateType.SHOWING)
            if showing and node.get_role() in (Atspi.Role.FRAME, Atspi.Role.WINDOW):
                component = node.get_component_iface()
                if component is not None:
                    bounds = component.get_extents(Atspi.CoordType.WINDOW)
                    native_window = native_window or (bounds.width > 0 and bounds.height > 0)
            # Names are compared internally; never print arbitrary desktop content.
            name = node.get_name()
            if showing and name in KNOWN_CONTROLS:
                if name in ("Your name", "Meeting name, invite link, or meeting code"):
                    # WebKit advertises editable entry state and Text, but its
                    # AT-SPI implementation does not expose EditableText.
                    if (node.get_role() == Atspi.Role.ENTRY
                            and states.contains(Atspi.StateType.EDITABLE)
                            and node.get_text_iface() is not None):
                        found[name] = node
                else:
                    action = node.get_action_iface()
                    if action is not None and action.get_n_actions() > 0:
                        found[name] = node
            for child_index in range(node.get_child_count()):
                pending.append(node.get_child_at_index(child_index))
    return native_window, found


def click(node):
    assert node.get_state_set().contains(Atspi.StateType.SENSITIVE), "Control is not actionable"
    assert node.get_action_iface().do_action(0), "Native control action was rejected"


@contextmanager
def launch_scratch():
    scratch = tempfile.mkdtemp(prefix="petal-linux-launch-")
    try:
        yield scratch
    except BaseException:
        print(f"Launch failure evidence retained in owned scratch: {scratch}", flush=True)
        raise
    else:
        shutil.rmtree(scratch)


class NativeLaunch:
    """The production launch operations shared by focused and living checks."""

    ENTRY_CONTROLS = ("Meeting name, invite link, or meeting code", "Create/Join", "User profile")
    IDENTITY_FIXTURE = 'Linux <launch> "check" & Δ'

    def __init__(self, executable, timeout):
        self.executable = Path(executable)
        self.timeout = timeout
        self.desktop = None
        self.process = None
        self.resources = ExitStack()
        self.owned_pids = set()

    def __enter__(self):
        return self

    def assert_executable(self):
        self.executable = self.executable.resolve(strict=True)
        assert os.access(self.executable, os.X_OK), "Locally built executable is not executable"
        with self.executable.open("rb") as binary:
            assert binary.read(4) == b"\x7fELF", "Locally built Linux ELF executable is required"

    def assert_actual_desktop(self):
        runtime = os.environ.get("XDG_RUNTIME_DIR", "")
        wayland = os.environ.get("WAYLAND_DISPLAY", "")
        bus = os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
        assert runtime and wayland and Path(runtime, wayland).is_socket(), "Actual Wayland session is required"
        assert bus, "Actual desktop session bus is required"
        self.desktop = (runtime, wayland, bus)

    def launch(self):
        assert self.desktop is not None, "Check the actual desktop before launching"
        runtime, wayland, bus = self.desktop
        scratch = self.resources.enter_context(launch_scratch())
        home = Path(scratch)
        self.activations = self.resources.enter_context(BrowserActivations(bus))
        self.native_input = NativeInput()
        # No inherited platform SDK/signing/test configuration can prepare this
        # launch. Retain only the actual graphical session and isolated profile.
        env = {
            "HOME": scratch, "PATH": "/usr/bin:/bin", "LANG": "C.UTF-8",
            "XDG_CONFIG_HOME": str(home / "config"),
            "XDG_DATA_HOME": str(home / "data"),
            "XDG_CACHE_HOME": str(home / "cache"),
            "XDG_RUNTIME_DIR": runtime, "WAYLAND_DISPLAY": wayland,
            "DBUS_SESSION_BUS_ADDRESS": bus, "GDK_BACKEND": "wayland",
            "GTK_MODULES": "atk-bridge", "NO_AT_BRIDGE": "0",
            "WEBKIT_INSPECTOR_HTTP_SERVER": f"127.0.0.1:{self.native_input.port}",
        }
        self.trace = home / "exec.trace"
        output = self.resources.enter_context((home / "process.log").open("w"))
        self.process = subprocess.Popen(
            ["/usr/bin/strace", "--kill-on-exit", "-f", "-e", "trace=execve",
             "-o", str(self.trace), str(self.executable)],
            env=env, stdout=output, stderr=output, start_new_session=True,
        )

    def observe_processes(self):
        current = descendants(self.process.pid)
        self.owned_pids.update(current)
        if self.trace.exists():
            # Includes short-lived children forked by worker threads.
            self.owned_pids.update(map(int, re.findall(r"(?m)^(\d+)\s", self.trace.read_text())))
        self.activations.check(self.owned_pids)
        return current

    def assert_room_entry(self):
        deadline = time.monotonic() + self.timeout
        completed = False
        identity_entered = False
        continued = False
        done_clicked = False
        last_controls = None
        while time.monotonic() < deadline:
            assert self.process.poll() is None, "Production Linux app exited before room entry appeared"
            current = self.observe_processes()
            window, visible = controls(current)
            observed = tuple(sorted(
                (name, node.get_role_name(), node.get_state_set().contains(Atspi.StateType.SENSITIVE))
                for name, node in visible.items()
            ))
            if observed != last_controls:
                print(f"Owned control transition: {observed}", flush=True)
                last_controls = observed
            if window and all(label in visible for label in self.ENTRY_CONTROLS):
                assert identity_entered and done_clicked, \
                    "Fresh native launch bypassed the real blank/escaped identity fixtures"
                completed = True
                break
            if "Your name" in visible and "Done" in visible and not identity_entered:
                entry = visible["Your name"]
                # Real adversarial fixture: a fresh blank identity must not be
                # able to complete the production onboarding form.
                assert Atspi.Text.get_text(entry.get_text_iface(), 0, -1) == "", \
                    "Fresh Linux profile did not start with a blank native name editor"
                assert not visible["Done"].get_state_set().contains(Atspi.StateType.SENSITIVE), \
                    "Production onboarding allowed a blank identity"
                assert entry.get_component_iface().grab_focus(), "Native name editor could not take focus"
                # Quotes, markup-like text and Unicode travel through the real
                # input handler, not a seeded store or application-state setter.
                self.native_input.fill_name(current, self.IDENTITY_FIXTURE)
                entry.clear_cache()
                assert Atspi.Text.get_text(entry.get_text_iface(), 0, -1) == self.IDENTITY_FIXTURE, \
                    "Native name editor did not retain the escaped identity fixture"
                identity_entered = True
            elif (identity_entered and not done_clicked and "Done" in visible
                  and visible["Done"].get_state_set().contains(Atspi.StateType.SENSITIVE)):
                click(visible["Done"])
                done_clicked = True
            elif ("Continue" in visible and not continued
                  and visible["Continue"].get_state_set().contains(Atspi.StateType.SENSITIVE)):
                click(visible["Continue"])
                continued = True
            time.sleep(0.2)
        assert completed, "Petal did not show its native room-entry window and real entry controls"
        time.sleep(2)
        assert self.process.poll() is None, "Production Linux app exited after first paint"
        window, visible = controls(self.observe_processes())
        assert window and all(label in visible for label in self.ENTRY_CONTROLS), \
            "Native room-entry surface disappeared after first paint"

    def assert_no_browser(self):
        execution = self.trace.read_text()
        self.owned_pids.update(map(int, re.findall(r"(?m)^(\d+)\s", execution)))
        assert not BROWSER_EXEC.search(execution), "Startup launched a browser or URL opener"
        self.activations.check(self.owned_pids)

    def assert_no_foreign_setup(self):
        # Native Linux artifact preparation is evidenced by the documented
        # Linux npm/Cargo build; launch independently checks its artifact and
        # actual process trace instead of trusting inherited SDK configuration.
        self.assert_executable()
        assert not FOREIGN_EXEC.search(self.trace.read_text()), \
            "Startup required macOS or Windows preparation"

    def __exit__(self, kind, value, traceback):
        try:
            if self.process is not None:
                if self.process.poll() is None:
                    os.killpg(self.process.pid, signal.SIGTERM)
                    try:
                        self.process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(self.process.pid, signal.SIGKILL)
                        self.process.wait(timeout=5)
                if kind is None:
                    # Observe the complete trace after cleanup, not just the
                    # part written when the room-entry assertion returned.
                    self.assert_no_browser()
                    self.assert_no_foreign_setup()
        except BaseException:
            self.resources.__exit__(*sys.exc_info())
            raise
        return self.resources.__exit__(kind, value, traceback)


SCENARIO = "Launch the Linux app in the graphical session"
STEP_BINDINGS = {
    ("Given", "the locally built Linux executable is available on this computer"): "assert_executable",
    ("Given", "this computer is running its actual Wayland desktop session"): "assert_actual_desktop",
    ("When", "I launch that executable from the documented Linux command"): "launch",
    ("Then", "Petal opens its native room-entry window"): "assert_room_entry",
    ("Then", "no browser is opened"): "assert_no_browser",
    ("Then", "neither preparing nor launching that executable requires macOS or Windows setup steps"): "assert_no_foreign_setup",
}


def living_steps(feature):
    """Bind this implemented scenario strictly; never silently skip a clause."""
    steps = []
    selected = False
    feature_seen = False
    phase = None
    for line in Path(feature).read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("Feature:"):
            feature_seen = True
            continue
        if line.startswith(("Background:", "Rule:", "Scenario Outline:", "Examples:")):
            raise AssertionError("Unsupported living syntax must not bypass native launch assertions")
        if line.startswith("Scenario:"):
            selected = line.removeprefix("Scenario:").strip() == SCENARIO
            phase = None
            continue
        if not selected:
            continue
        keyword, separator, text = line.partition(" ")
        assert separator and keyword in ("Given", "When", "Then", "And"), \
            "Unbound living step cannot be treated as passed"
        if keyword == "And":
            assert phase is not None, "Living And step has no preceding phase"
            keyword = phase
        else:
            phase = keyword
        key = (keyword, text)
        assert key in STEP_BINDINGS, "Living step has no production operation binding"
        steps.append(key)
    assert feature_seen and steps == list(STEP_BINDINGS), \
        "Living launch scenario must bind every exact acceptance step in order"
    return steps


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--timeout", type=float, default=45)
    parser.add_argument("--feature", type=Path, help="Run the bound Task 1.1 living scenario")
    args = parser.parse_args()
    steps = living_steps(args.feature) if args.feature else list(STEP_BINDINGS)
    with NativeLaunch(args.executable, args.timeout) as world:
        for key in steps:
            getattr(world, STEP_BINDINGS[key])()
            if args.feature:
                print(f"Checked living step: {key[0]} {key[1]}", flush=True)
    print("PASS: visible native Wayland room-entry surface and real entry controls")
    print("PASS: blank identity rejected; escaped identity reached real native room entry")
    print("PASS: no owned browser/opener execution or desktop-bus activation")
    print("PASS: Linux-only profile required no macOS/Windows preparation; owned tracees cleaned up")


if __name__ == "__main__":
    main()
