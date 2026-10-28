#!/usr/bin/env python3
"""Witness Task 2.1 in the actual Wayland desktop and a real Windows room.

Start windows-receive-editor.ps1 on the authorized Windows peer and share its
Notepad window using the released Petal client before running this check.
No media is generated or injected here. Pixel evidence comes from the desktop
compositor, not the SDK, IPC state, a browser, or an offscreen decode buffer.
Requires AT-SPI, Mutter ScreenCast/RemoteDesktop, GStreamer PipeWire, tesseract
and NumPy. Window-only capture never includes unrelated desktop windows.
Full acceptance additionally requires --observe-desktop and explicit human
authorization: it briefly captures the monitor, including other visible
windows. Unique owned-pixel matches establish real positions; the option
alone never waives a geometry assertion. Retained evidence is private.
"""
import argparse
import importlib.util
from pathlib import Path
import re
import subprocess
import time


def load_helper(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


hosted = load_helper("receive_hosted", "check-linux-hosted-entry.py")
launch = hosted.launch
Atspi, Gio, GLib = launch.Atspi, launch.Gio, launch.GLib

SCENARIO = "Receive a movable and resizable native share from the Windows peer"
STEPS = (
    "Given Linux and a native <peer> client are in hosted room abc-defg-hjk",
    "When the peer shares a non-sensitive editor window with a changing counter",
    "Then exactly one independent native remote window appears on Linux",
    "And its pixels show the editor and continuing counter changes",
    "When I move and resize the remote window using its desktop window controls",
    "Then the remote window moves and resizes independently",
    "And the main meeting window keeps its own position and size",
    "Examples:", "| peer |", "| Windows |",
)


def living_contract(feature):
    selected = False
    found = []
    seen_feature = False
    matches = 0
    for raw in feature.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("Feature:"):
            seen_feature = True
            continue
        assert not line.startswith(("Background:", "Rule:")), "Unbound receive Background/Rule"
        if line.startswith(("Scenario:", "Scenario Outline:")):
            kind, _, name = line.partition(":")
            selected = name.strip() == SCENARIO
            if selected:
                matches += 1
                assert kind == "Scenario Outline", "Receive scenario must retain the full outline"
            continue
        if selected:
            found.append(" ".join(line.split()) if line.startswith("|") else line)
    assert seen_feature and matches == 1 and tuple(found) == STEPS, \
        "Receive witness requires every exact Task2.1 clause and the complete Windows example"


def owned_windows(app):
    # Only immediate application children are top-levels; never mistake a web
    # document's frame role for an independent native desktop window.
    launch.controls(app.observe_processes())  # flush accessibility events
    pids = app.observe_processes()
    desktop = Atspi.get_desktop(0)
    windows = []
    for i in range(desktop.get_child_count()):
        application = desktop.get_child_at_index(i)
        if application is None or application.get_process_id() not in pids:
            continue
        for j in range(application.get_child_count()):
            node = application.get_child_at_index(j)
            if node is None:
                continue
            node.clear_cache()
            if (node.get_role() in (Atspi.Role.FRAME, Atspi.Role.WINDOW)
                    and node.get_state_set().contains(Atspi.StateType.SHOWING)
                    and node.get_component_iface() is not None):
                windows.append(node)
    return windows


def allocation_size(node):
    """Native allocation only; Wayland GTK SCREEN x/y are fabricated origins."""
    node.clear_cache()
    rect = node.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
    assert rect.width > 0 and rect.height > 0, "Native widget is not realized/mapped"
    return (rect.width, rect.height)


def drawing_area(window):
    pending = [window]
    visited = 0
    found = []
    while pending:
        node = pending.pop()
        if node is None:
            continue
        visited += 1
        assert visited <= 4000, "Remote accessibility tree exceeded bound"
        node.clear_cache()
        if (node.get_name() == "Remote shared window"
                and node.get_state_set().contains(Atspi.StateType.SHOWING)
                and node.get_component_iface() is not None):
            found.append(node)
        for index in range(node.get_child_count()):
            pending.append(node.get_child_at_index(index))
    assert len(found) == 1, "Native remote window needs exactly one mapped receive DrawingArea"
    return found[0]


def remote_and_main(app, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        assert app.process.poll() is None, "Linux app exited during native receive"
        windows = owned_windows(app)
        remote = [node for node in windows if node.get_name().startswith("Petal — ")]
        assert len(remote) <= 1, "More than one independent native remote window appeared"
        if remote:
            main = [node for node in windows if node not in remote]
            assert len(main) == 1, "Receive must retain exactly one separate main meeting window"
            allocation_size(remote[0])
            drawing_area(remote[0])
            return remote[0], main[0]
        time.sleep(0.2)
    raise AssertionError("Missing exactly one mapped independent native remote GTK window (Task2.1 RED)")


def owned_bus():
    """A private connection lets teardown release only this witness's sessions."""
    return Gio.DBusConnection.new_for_address_sync(
        launch.os.environ["DBUS_SESSION_BUS_ADDRESS"],
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None, None)


def assert_owned_active(app, window):
    window.clear_cache()
    assert window.get_process_id() in app.observe_processes(), "Native target is not owned by this launch"
    assert window.get_state_set().contains(Atspi.StateType.ACTIVE), \
        "Owned remote GTK window must be actually ACTIVE before window selection or desktop input"


def activate_owned(app, window):
    window.clear_cache()
    if not window.get_state_set().contains(Atspi.StateType.ACTIVE):
        # A request's boolean is not a focus observation. GTK can return False
        # for a window already focused by the desktop.
        window.get_component_iface().grab_focus()
        time.sleep(0.5)
    assert_owned_active(app, window)


class WindowPixels:
    """Bounded compositor capture of only the verified focused owned window."""

    SERVICE = "org.gnome.Mutter.ScreenCast"

    def __init__(self, app, window):
        import gi
        gi.require_version("Gst", "1.0")
        gi.require_version("GstVideo", "1.0")
        gi.require_version("GdkPixbuf", "2.0")
        from gi.repository import Gst, GstVideo, GdkPixbuf
        self.gst, self.video, self.pixbuf = Gst, GstVideo, GdkPixbuf
        self.bus = owned_bus()
        self.session = self.subscription = self.pipeline = None
        Gst.init(None)
        try:
            activate_owned(app, window)
            self.session = self.call("/org/gnome/Mutter/ScreenCast", self.SERVICE,
                                     "CreateSession", GLib.Variant("(a{sv})", ({},))).unpack()[0]
            assert_owned_active(app, window)
            stream = self.call(self.session, self.SERVICE + ".Session", "RecordWindow",
                        GLib.Variant("(a{sv})", ({"cursor-mode": GLib.Variant("u", 0)},))).unpack()[0]
            self.start_stream(app, stream, lambda: self.call(
                self.session, self.SERVICE + ".Session", "Start"))
        except BaseException:
            self.close()
            raise

    def start_stream(self, app, stream, start):
        nodes = []

        def stream_added(connection, sender, object_path, interface_name, signal_name, parameters):
            nodes.append(parameters.unpack()[0])

        self.subscription = self.bus.signal_subscribe(
            self.SERVICE, self.SERVICE + ".Stream", "PipeWireStreamAdded", stream,
            None, Gio.DBusSignalFlags.NONE, stream_added)
        start()
        deadline = time.monotonic() + min(app.timeout, 10)
        context = GLib.MainContext.default()
        while not nodes and time.monotonic() < deadline:
            while context.pending():
                context.iteration(False)
            app.observe_processes()
            time.sleep(0.02)
        assert nodes, "Compositor stream supplied no PipeWire node"
        self.pipeline = self.gst.parse_launch(
            f"pipewiresrc path={nodes[0]} do-timestamp=true ! videoconvert ! "
            "video/x-raw,format=RGB ! appsink name=frames max-buffers=1 drop=true sync=false")
        self.pipeline.set_state(self.gst.State.PLAYING)

    def call(self, path, interface, method, args=None):
        return self.bus.call_sync(self.SERVICE, path, interface, method, args, None,
                                  Gio.DBusCallFlags.NONE, 5000, None)

    def close(self):
        if self.pipeline is not None:
            self.pipeline.set_state(self.gst.State.NULL)
            self.pipeline = None
        if self.subscription is not None:
            self.bus.signal_unsubscribe(self.subscription)
            self.subscription = None
        try:
            if self.session is not None:
                session, self.session = self.session, None
                self.call(session, self.SERVICE + ".Session", "Stop")
        finally:
            if not self.bus.is_closed():
                self.bus.close_sync(None)

    def image(self):
        sample = self.pipeline.get_by_name("frames").emit("try-pull-sample", 10 * self.gst.SECOND)
        if sample is None:
            message = self.pipeline.get_bus().pop_filtered(self.gst.MessageType.ERROR)
            detail = ": " + str(message.parse_error()[0]) if message else ""
            raise AssertionError("No owned compositor window pixels" + detail)
        info = self.video.VideoInfo.new_from_caps(sample.get_caps())
        buffer = sample.get_buffer()
        ok, mapped = buffer.map(self.gst.MapFlags.READ)
        assert ok, "Cannot map owned compositor window pixels"
        try:
            image = self.pixbuf.Pixbuf.new_from_bytes(
                GLib.Bytes.new(bytes(mapped.data)), self.pixbuf.Colorspace.RGB,
                False, 8, info.width, info.height, info.stride[0])
        finally:
            buffer.unmap(mapped)
        return image

    def counter(self, output):
        image = self.image()
        # Stream dimensions can be monitor-sized padding around the selected
        # window. Only this window is rendered; never treat stream size as its
        # desktop geometry or capture other desktop windows.
        image.savev(str(output), "png", [], [])
        image = image.scale_simple(image.get_width() * 2, image.get_height() * 2,
                                  self.pixbuf.InterpType.BILINEAR)
        ok, png = image.save_to_bufferv("png", [], [])
        assert ok, "Unable to encode compositor pixels for OCR"
        ocr = subprocess.run(["tesseract", "stdin", "stdout", "--psm", "6"],
                             input=png, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             timeout=15, check=True)
        text = ocr.stdout.decode("utf-8", errors="replace")
        if "PETAL SAFE EDITOR" not in text.upper():
            return None
        match = re.search(r"PETAL\s+RECEIVE\s+COUNTER\s*[:=]?\s*(\d+)", text, re.I)
        return int(match.group(1)) if match else None


class DesktopControls:
    """Short-lived owned Mutter input session, using normal OS window controls."""

    SERVICE = "org.gnome.Mutter.RemoteDesktop"

    def __init__(self, app, window):
        self.app, self.window = app, window
        self.bus = owned_bus()
        self.session = None
        self.started = False

    def call(self, path, interface, method, args=None):
        return self.bus.call_sync(self.SERVICE, path, interface, method, args, None,
                                  Gio.DBusCallFlags.NONE, 5000, None)

    def __enter__(self):
        try:
            activate_owned(self.app, self.window)
            self.session = self.call("/org/gnome/Mutter/RemoteDesktop", self.SERVICE,
                                     "CreateSession").unpack()[0]
            self.call(self.session, self.SERVICE + ".Session", "Start")
            self.started = True
        except BaseException:
            self.bus.close_sync(None)
            raise
        return self

    def key(self, symbol, pressed):
        self.call(self.session, self.SERVICE + ".Session", "NotifyKeyboardKeysym",
                  GLib.Variant("(ub)", (symbol, pressed)))

    def window_action(self, shortcut):
        assert_owned_active(self.app, self.window)
        try:
            self.key(0xffe9, True)  # Alt_L
            self.key(shortcut, True)  # F7 / F8
            self.key(shortcut, False)
            self.key(0xffe9, False)
            time.sleep(0.2)
            for symbol in (0xff53, 0xff54):  # right, down
                for _ in range(4):
                    self.key(symbol, True)
                    self.key(symbol, False)
                    time.sleep(0.05)
            self.key(0xff0d, True)
            self.key(0xff0d, False)
        finally:
            for symbol in (0xffe9, shortcut, 0xff53, 0xff54, 0xff0d):
                self.key(symbol, False)
        time.sleep(0.5)
        assert_owned_active(self.app, self.window)

    def __exit__(self, kind, value, traceback):
        try:
            if self.started:
                self.call(self.session, self.SERVICE + ".Session", "Stop")
                self.started = False
        finally:
            self.bus.close_sync(None)


class DesktopObservation(WindowPixels):
    """Explicitly authorized monitor pixels and linked normal pointer input."""

    REMOTE = "org.gnome.Mutter.RemoteDesktop"

    def __init__(self, app, capture):
        self.gst, self.video, self.pixbuf = capture.gst, capture.video, capture.pixbuf
        self.bus = owned_bus()
        self.session = self.subscription = self.pipeline = None
        self.input_session = None
        self.input_started = False
        try:
            self.input_session = self.input_call(
                "/org/gnome/Mutter/RemoteDesktop", self.REMOTE, "CreateSession").unpack()[0]
            session_id = self.input_call(
                self.input_session, "org.freedesktop.DBus.Properties", "Get",
                GLib.Variant("(ss)", (self.REMOTE + ".Session", "SessionId"))).unpack()[0]
            self.session = self.call(
                "/org/gnome/Mutter/ScreenCast", self.SERVICE, "CreateSession",
                GLib.Variant("(a{sv})", ({
                    "remote-desktop-session-id": GLib.Variant("s", session_id),
                },))).unpack()[0]
            self.stream = self.call(
                self.session, self.SERVICE + ".Session", "RecordMonitor",
                GLib.Variant("(sa{sv})", ("", {"cursor-mode": GLib.Variant("u", 0)}))).unpack()[0]
            parameters = self.call(
                self.stream, "org.freedesktop.DBus.Properties", "GetAll",
                GLib.Variant("(s)", (self.SERVICE + ".Stream",))).unpack()[0]["Parameters"]
            self.monitor_origin = parameters["position"]
            self.monitor_size = parameters["size"]
            self.start_stream(app, self.stream, self.start_input)
        except BaseException:
            self.close()
            raise

    def input_call(self, path, interface, method, args=None):
        return self.bus.call_sync(self.REMOTE, path, interface, method, args, None,
                                  Gio.DBusCallFlags.NONE, 5000, None)

    def start_input(self):
        self.input_call(self.input_session, self.REMOTE + ".Session", "Start")
        self.input_started = True

    def pointer(self, x, y):
        self.input_call(self.input_session, self.REMOTE + ".Session", "NotifyPointerMotionAbsolute",
                        GLib.Variant("(sdd)", (self.stream, float(x), float(y))))

    def button(self, pressed):
        self.input_call(self.input_session, self.REMOTE + ".Session", "NotifyPointerButton",
                        GLib.Variant("(ib)", (272, pressed)))  # Linux BTN_LEFT

    def drag_owned_title(self, app, window, before, target_x):
        assert_owned_active(app, window)
        x, y, width, _ = before
        self.pointer(x + width / 2, y + 18)
        self.button(True)
        try:
            for step in range(1, 11):
                self.pointer(x + width / 2 + (target_x - x) * step / 10, y + 18)
                time.sleep(0.04)
        finally:
            self.button(False)
        time.sleep(0.5)
        assert_owned_active(app, window)

    def close(self):
        try:
            if self.input_started:
                self.input_call(self.input_session, self.REMOTE + ".Session", "Stop")
                self.input_started = False
                self.session = None  # Linked input Stop also stops the screencast.
        finally:
            super().close()


def save_private_pixels(image, path):
    image.savev(str(path), "png", [], [])
    path.chmod(0o600)


def locate_owned_pixels(reference, desktop):
    """Find one owned client patch; report its visible origin in monitor pixels.

    The window-only reference includes native decoration and padding. Neither
    its stream dimensions nor GTK SCREEN coordinates supply desktop positions.
    Only a unique full-patch match in actual monitor pixels supplies the origin.
    """
    import numpy as np

    def array(image):
        return np.ndarray((image.get_height(), image.get_width(), 3), dtype=np.uint8,
                          buffer=image.get_pixels(), strides=(image.get_rowstride(), 3, 1))

    ref, screen = array(reference), array(desktop)
    ys, xs = np.nonzero(ref.max(axis=2) > 5)
    assert len(xs), "Owned window reference has no visible pixels"
    x0, y0, x1, y1 = map(int, (xs.min(), ys.min(), xs.max() + 1, ys.max() + 1))
    patch = ref[y0 + 45:y0 + 109, x0 + 12:x0 + 192]
    assert patch.shape == (64, 180, 3), "Owned reference is too small for a distinctive patch"
    grey = patch.astype(np.int16).sum(axis=2)
    gradient = (np.abs(np.diff(grey, axis=0, prepend=grey[:1])) +
                np.abs(np.diff(grey, axis=1, prepend=grey[:, :1])))
    chosen = []
    for flat in np.argsort(gradient.ravel())[::-1]:
        py, px = divmod(int(flat), patch.shape[1])
        if all(abs(py - ay) + abs(px - ax) > 8 for ay, ax in chosen):
            chosen.append((py, px))
        if len(chosen) == 12:
            break
    assert chosen and gradient[chosen[0]] > 0, "Owned pixel patch has no distinctive edges"
    py, px = chosen[0]
    mask = np.max(np.abs(screen.astype(np.int16) - patch[py, px].astype(np.int16)), axis=2) <= 3
    cy, cx = np.nonzero(mask)
    cy, cx = cy - py, cx - px
    valid = ((cy >= 0) & (cx >= 0) &
             (cy + patch.shape[0] <= screen.shape[0]) & (cx + patch.shape[1] <= screen.shape[1]))
    cy, cx = cy[valid], cx[valid]
    for py, px in chosen[1:]:
        valid = np.max(np.abs(screen[cy + py, cx + px].astype(np.int16) -
                              patch[py, px].astype(np.int16)), axis=1) <= 3
        cy, cx = cy[valid], cx[valid]
    matches = []
    for yy, xx in zip(cy, cx):
        error = np.abs(screen[yy:yy + 64, xx:xx + 180].astype(np.int16) -
                       patch.astype(np.int16)).mean()
        if error <= 2:
            matches.append((int(xx - 12), int(yy - 45), x1 - x0, y1 - y0))
    assert len(matches) == 1, f"Expected one visible owned pixel match; got {len(matches)}"
    return matches[0]


def activate_owned_group_window(app, target, other):
    # GTK grab_focus cannot raise an inactive Wayland top-level reliably.
    # Normal Alt-F6 cycles only the already active owned application's windows.
    for _ in range(3):
        target.clear_cache()
        if target.get_state_set().contains(Atspi.StateType.ACTIVE):
            return
        other.clear_cache()
        assert_owned_active(app, other)
        with DesktopControls(app, other) as controls:
            try:
                controls.key(0xffe9, True)
                controls.key(0xffc3, True)  # Alt-F6: cycle owned application group
                controls.key(0xffc3, False)
            finally:
                controls.key(0xffc3, False)
                controls.key(0xffe9, False)
        time.sleep(0.3)
    assert_owned_active(app, target)


def exercise_observed_desktop(app, remote, main, capture, directory):
    import json

    desktop = DesktopObservation(app, capture)
    try:
        remote_reference = capture.image()
        activate_owned_group_window(app, main, remote)
        main_capture = WindowPixels(app, main)
        try:
            main_reference = main_capture.image()
        finally:
            main_capture.close()
        image = desktop.image()
        before_main = locate_owned_pixels(main_reference, image)
        before_remote = locate_owned_pixels(remote_reference, image)
        # Arrange only owned windows before establishing the measurement
        # baseline. Keep the meeting's identifying patch visible throughout.
        target_x = before_remote[0] - before_main[2] - 80
        assert target_x >= 20, "Monitor has insufficient room for the owned-window geometry fixture"
        desktop.drag_owned_title(app, main, before_main, target_x)
        activate_owned_group_window(app, remote, main)
        image = desktop.image()
        save_private_pixels(image, directory / "desktop-baseline.png")
        baseline_main = locate_owned_pixels(main_reference, image)
        baseline_remote = locate_owned_pixels(remote_reference, image)
        main_size, remote_size = allocation_size(main), allocation_size(remote)
        assert baseline_main[0] + baseline_main[2] < baseline_remote[0], \
            "Owned windows overlap after fixture arrangement"
        with DesktopControls(app, remote) as controls:
            controls.window_action(0xffc4)
            image = desktop.image()
            save_private_pixels(image, directory / "desktop-moved.png")
            moved_main = locate_owned_pixels(main_reference, image)
            moved_remote = locate_owned_pixels(remote_reference, image)
            assert moved_remote[:2] != baseline_remote[:2], "Remote window did not actually move"
            assert moved_main == baseline_main and allocation_size(main) == main_size, \
                "Moving the remote window changed the meeting's actual position or size"
            controls.window_action(0xffc5)
        assert allocation_size(remote) != remote_size, "Remote window did not actually resize"
        remote_reference = capture.image()  # Resized content can have a different scale.
        image = desktop.image()
        save_private_pixels(image, directory / "desktop-resized.png")
        resized_main = locate_owned_pixels(main_reference, image)
        resized_remote = locate_owned_pixels(remote_reference, image)
        assert resized_main == baseline_main and allocation_size(main) == main_size, \
            "Resizing the remote window changed the meeting's actual position or size"
        proof = {
            "monitor_origin": desktop.monitor_origin,
            "main_visible_pixels": [baseline_main, moved_main, resized_main],
            "remote_visible_pixels": [baseline_remote, moved_remote, resized_remote],
            "remote_native_allocations": [remote_size, allocation_size(remote)],
            "main_native_allocation": main_size,
        }
        path = directory / "desktop-geometry.json"
        path.write_text(json.dumps(proof, indent=2) + "\n")
        path.chmod(0o600)
        print("PASS: actual desktop pixels prove remote movement and unchanged meeting position/size", flush=True)
        print("PASS: actual remote native allocation resized with received pixels still observable", flush=True)
    finally:
        desktop.close()


def changing_pixels(app, remote, main, capture, directory, phase):
    baseline = allocation_size(main)
    counters = []
    deadline = time.monotonic() + app.timeout
    while time.monotonic() < deadline:
        current, current_main = remote_and_main(app, 1)
        assert current == remote and current_main == main, "Native windows were replaced during observation"
        assert allocation_size(main) == baseline, "Main meeting window changed allocation during receive"
        allocation_size(drawing_area(remote))
        value = capture.counter(directory / f"{phase}-latest.png")
        if value is not None and (not counters or value > counters[-1]):
            counters.append(value)
            if len(counters) >= 3:
                print(f"PASS: {phase}: compositor pixels show safe editor and three increasing counters", flush=True)
                return
        time.sleep(1)
    raise AssertionError(f"{phase}: mapped native pixels did not show safe editor and continuing counter changes")


def exercise_native_controls(app, remote, main):
    before_main, before_remote = allocation_size(main), allocation_size(remote)
    with DesktopControls(app, remote) as controls:
        controls.window_action(0xffc4)  # Alt-F7: normal desktop Move
        assert allocation_size(main) == before_main, "Remote move changed main allocation"
        controls.window_action(0xffc5)  # Alt-F8: normal desktop Resize
    deadline = time.monotonic() + min(app.timeout, 10)
    while time.monotonic() < deadline:
        assert allocation_size(main) == before_main, "Remote resize changed main allocation"
        if allocation_size(remote) != before_remote:
            print("PASS: actual native remote allocation resized; main allocation unchanged", flush=True)
            return
        app.observe_processes()
        time.sleep(0.2)
    raise AssertionError(
        "Normal OS shortcuts did not produce an actual native resize. "
        "Command delivery is not geometry proof; verify owned-fixture key events and desktop bindings.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--backend-url", required=True)
    parser.add_argument("--room-code", required=True)
    parser.add_argument("--expected-peer-name", required=True)
    parser.add_argument("--timeout", type=float, default=45)
    parser.add_argument("--evidence-dir", type=Path, required=True,
                        help="New private directory for safe compositor content crops; retained on failure")
    parser.add_argument("--feature", type=Path,
                        help="Bind the entire exact Task2.1 outline; no Example filtering")
    parser.add_argument("--observe-desktop", action="store_true",
                        help="Requires explicit authorization: briefly captures the monitor, including other windows")
    args = parser.parse_args()
    assert args.timeout > 0, "Timeout must be positive"
    if args.feature:
        living_contract(args.feature)
    args.evidence_dir.mkdir(mode=0o700, parents=False, exist_ok=False)
    with launch.NativeLaunch(args.executable, args.timeout, backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        hosted.assert_join_code(app, args.room_code)
        hosted.join_hosted_room(app, args.expected_peer_name)
        remote, meeting = remote_and_main(app, app.timeout)
        print("PASS: exactly one mapped native remote GTK top-level separate from meeting", flush=True)
        capture = WindowPixels(app, remote)
        try:
            changing_pixels(app, remote, meeting, capture, args.evidence_dir, "before-resize")
            if args.observe_desktop:
                exercise_observed_desktop(app, remote, meeting, capture, args.evidence_dir)
            else:
                exercise_native_controls(app, remote, meeting)
            changing_pixels(app, remote, meeting, capture, args.evidence_dir, "after-resize")
        finally:
            capture.close()
        remote_name = remote.get_name()
        hosted.leave_hosted_room(app)
        deadline = time.monotonic() + min(app.timeout, 10)
        while any(node.get_name() == remote_name for node in owned_windows(app)):
            assert time.monotonic() < deadline, "Native Leave retained the received window"
            time.sleep(0.1)
        print("PASS: native Leave removed the received window before process teardown", flush=True)
    if args.observe_desktop:
        print("PASS: complete Task2.1 native pixels, desktop movement/resize, and independent meeting geometry", flush=True)
        return
    raise AssertionError(
        "UNSATISFIED Task2.1: compositor editor pixels and native allocation checks are not global "
        "position proof. Actual outer-desktop observation must establish remote movement and "
        "unchanged main-window position and size. Supply a trusted owned-window compositor "
        "frame-rect observation or authorize privacy-safe desktop-region capture; no GTK fake "
        "SCREEN origin, sent-command acknowledgement, or CLI boolean can satisfy this gate.")


if __name__ == "__main__":
    main()
