#!/usr/bin/env python3
"""Witness the complete Task2.2 scenario through real native compositor pixels.

Use the released Windows peer sharing windows-receive-editor.ps1 with
-GeometryPhaseFile. Supply measured DWM source dimensions, not requested sizes.
After FIDELITY_SOURCE_CHANGE_REQUIRED, change that file to changed. This process
keeps its original Linux room connection throughout. Only the owned window is
captured; evidence is private. No media, rendering or source-change API is faked.
"""
import argparse
import csv
import io
import json
from pathlib import Path
import re
import subprocess
import time

import numpy as np

# The adjacent witness owns native launch, AT-SPI and Mutter session lifetimes.
import importlib.util

_spec = importlib.util.spec_from_file_location(
    "receive", Path(__file__).with_name("check-linux-receive.py"))
receive = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(receive)

SCENARIO = "Preserve source aspect and readable text across size changes"
STEPS = (
    'Given a peer shares an editor window containing "Petal Linux 12345" and four visible corner markers',
    'And Linux displays that share in its native remote window',
    'When I resize the remote window to a different aspect ratio',
    'Then the whole received image remains visible without cropping or stretching',
    'And any unused area is letterboxed',
    'When the peer changes the source window dimensions and changes the text to "Petal Linux 67890"',
    'Then Linux displays the new source aspect ratio without stretching',
    'And all four corner markers remain visible',
    'When I view the received image at one-to-one image scale',
    'Then "Petal Linux 67890" is readable rather than the previous text',
)


def living_contract(path):
    selected, found, matches, feature = False, [], 0, False
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("Feature:"):
            feature = True
            continue
        assert not line.startswith(("Background:", "Rule:")), "Unbound fidelity Background/Rule"
        if line.startswith(("Scenario:", "Scenario Outline:")):
            kind, _, name = line.partition(":")
            selected = name.strip() == SCENARIO
            if selected:
                matches += 1
                assert kind == "Scenario", "Fidelity must retain its exact complete scenario"
            continue
        if selected:
            found.append(line)
    assert feature and matches == 1 and tuple(found) == STEPS, \
        "Fidelity witness requires every exact Task2.2 clause"


def dimensions(value):
    match = re.fullmatch(r"([1-9][0-9]*)x([1-9][0-9]*)", value)
    if not match:
        raise argparse.ArgumentTypeError("Expected measured WIDTHxHEIGHT")
    return tuple(map(int, match.groups()))


def array(image):
    return np.ndarray((image.get_height(), image.get_width(), 3), dtype=np.uint8,
                      buffer=image.get_pixels(), strides=(image.get_rowstride(), 3, 1))


def recognize(image):
    # Mutter can pad the owned window to monitor dimensions. Crop the largest
    # contiguous native-pixel rectangle before OCR; never upscale even at 1:1.
    pixels = array(image)
    lit = pixels.max(axis=2) > 12
    rectangles = []
    for top, bottom in ranges(lit.sum(axis=1) > 30):
        _, xs = np.nonzero(lit[top:bottom])
        left, right = int(xs.min()), int(xs.max()) + 1
        rectangles.append((left, int(top), right, int(bottom)))
    assert rectangles, "No visible native rectangle for OCR"
    left, top, right, bottom = max(
        rectangles, key=lambda box: (box[2] - box[0]) * (box[3] - box[1]))
    crop = image.new_subpixbuf(left, top, right - left, bottom - top)
    ok, png = crop.save_to_bufferv("png", [], [])
    assert ok, "Cannot encode owned native-pixel crop"
    result = subprocess.run(
        ["tesseract", "stdin", "stdout", "--psm", "6", "tsv"], input=png,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=20, check=True)
    lines = {}
    for row in csv.DictReader(io.StringIO(result.stdout.decode()),
                              delimiter="\t", quoting=csv.QUOTE_NONE):
        if row["level"] != "5" or not row["text"].strip():
            continue
        # Keep every OCR box in original compositor-stream coordinates.
        row["left"] = str(int(row["left"]) + left)
        row["top"] = str(int(row["top"]) + top)
        key = tuple(row[name] for name in ("page_num", "block_num", "par_num", "line_num"))
        lines.setdefault(key, []).append(row)
    result = []
    for words in lines.values():
        x = min(int(word["left"]) for word in words)
        y = min(int(word["top"]) for word in words)
        right = max(int(word["left"]) + int(word["width"]) for word in words)
        bottom = max(int(word["top"]) + int(word["height"]) for word in words)
        line = " ".join(word["text"] for word in words)
        if not re.fullmatch(r"CORNER\s+[1-4]", line, re.I):
            result.append((line, (x, y, right, bottom)))
        for index, word in enumerate(words[:-1]):
            if word["text"].upper() == "CORNER" and re.fullmatch("[1-4]", words[index + 1]["text"]):
                following = words[index + 1]
                result.append(("CORNER " + following["text"],
                               (int(word["left"]), int(word["top"]),
                                int(following["left"]) + int(following["width"]),
                                max(int(word["top"]) + int(word["height"]),
                                    int(following["top"]) + int(following["height"])))))
    return result


def content(lines, number):
    text = "\n".join(line for line, _ in lines)
    assert re.search(rf"Petal\s+Linux\s+{number}\b", text), f"Missing exact current text {number}"
    previous = "12345" if number == "67890" else "67890"
    assert not re.search(rf"Petal\s+Linux\s+{previous}\b", text), "Previous source text remains visible"
    assert "PETAL SAFE EDITOR" in text.upper(), "Missing actual safe editor fixture"
    candidates = {marker: [] for marker in (1, 2, 3, 4)}
    for line, box in lines:
        for match in re.finditer(r"^CORNER\s+([1-4])$", line, re.I):
            marker = int(match.group(1))
            if box not in candidates[marker]:
                candidates[marker].append(box)
    assert all(candidates.values()), "All four visible corner labels are required"
    assert all(len(boxes) <= 8 for boxes in candidates.values()), \
        "Ambiguous excessive corner labels"
    # Notepad may repeat CORNER 1 in its tab. Select the unique geometric four
    # corners, not the first text match or a waived duplicate. All four labels
    # must form aligned top/bottom pairs and aligned left/right columns.
    from itertools import product
    arrangements = []
    for boxes in product(*(candidates[marker] for marker in (1, 2, 3, 4))):
        centers = [((box[0] + box[2]) / 2, (box[1] + box[3]) / 2) for box in boxes]
        width = max(box[2] - box[0] for box in boxes)
        height = max(box[3] - box[1] for box in boxes)
        one, two, three, four = centers
        if (abs(one[1] - two[1]) <= height * .5 + 3 and
                abs(three[1] - four[1]) <= height * .5 + 3 and
                abs(one[0] - three[0]) <= width * .15 + 3 and
                abs(two[0] - four[0]) <= width * .15 + 3 and
                min(two[0] - one[0], four[0] - three[0]) > 2 * width and
                min(three[1] - one[1], four[1] - two[1]) > 2 * height):
            arrangements.append(dict(zip((1, 2, 3, 4), boxes)))
    assert len(arrangements) == 1, \
        f"Require one unambiguous four-corner geometric arrangement; found {len(arrangements)}"
    return arrangements[0]


def ranges(mask):
    edges = np.diff(np.r_[False, mask, False].astype(np.int8))
    return list(zip(np.flatnonzero(edges == 1), np.flatnonzero(edges == -1)))


def decoded_dimensions(app):
    # Existing production queries; return only connection presence and actual
    # Cairo-accepted dimensions, never persist room or owner identities.
    snapshot = receive.hosted.native_async(app, """
        const [room, windows] = await Promise.all([
            window.__TAURI_INTERNALS__.invoke('current_room'),
            window.__TAURI_INTERNALS__.invoke('compositor_list_windows')
        ]);
        return {
            connected: room != null,
            decodedSizes: windows.map(window => window.decodedSize)
        };
    """)
    assert snapshot["connected"], "Original native room connection is not connected"
    assert len(snapshot["decodedSizes"]) == 1, \
        "Require exactly one actual incoming native window summary"
    size = snapshot["decodedSizes"][0]
    assert isinstance(size, list) and len(size) == 2, \
        "Native renderer has not accepted actual decoded frame dimensions"
    assert all(type(value) is int and value > 0 for value in size), \
        "Decoded dimensions are absent or invalid"
    return tuple(size)


def geometry(image, markers, source, decoded, viewport, origin=None, letterbox=True):
    pixels = array(image)
    # Source and GTK titlebar are separated by real black bars at the deliberately
    # tall allocation. Identify the source rectangle from pixels, not SDK state.
    lit = pixels.max(axis=2) > 12
    marker_top = min(box[1] for box in markers.values())
    marker_bottom = max(box[3] for box in markers.values())
    runs = ranges(lit.sum(axis=1) > 30)
    assert runs, "No visible native window"
    title_top = int(runs[0][0])
    if origin is None:
        candidates = [(a, b) for a, b in runs if a <= marker_top and b >= marker_bottom]
        assert len(candidates) == 1, "No single complete visible source rectangle"
        preceding = [(a, b) for a, b in runs if b < candidates[0][0]]
        assert len(preceding) == 1, "No separately observable native titlebar above black letterbox"
        origin = int(preceding[0][1] - title_top)
    # Preserve measured titlebar HEIGHT, not a fabricated global GTK origin or
    # a stream origin that could change when Mutter repads a resized window.
    client_top = title_top + origin
    _, title_xs = np.nonzero(lit[title_top:client_top])
    client_left = int(title_xs.min())
    title_width = int(title_xs.max()) + 1 - client_left
    assert abs(title_width - viewport[0]) <= 4, \
        f"Native titlebar width {title_width} differs from allocation {viewport}"
    ox, oy = client_left, client_top
    lit[:oy] = False
    lit[oy + viewport[1]:] = False
    lit[:, :ox] = False
    lit[:, ox + viewport[0]:] = False
    runs = ranges(lit.sum(axis=1) > 30)
    candidates = [(a, b) for a, b in runs if a <= marker_top and b >= marker_bottom]
    assert len(candidates) == 1, "No single complete visible source rectangle"
    top, bottom = candidates[0]
    _, xs = np.nonzero(lit[top:bottom])
    left, right = int(xs.min()), int(xs.max()) + 1
    vw, vh = viewport
    assert 0 <= ox and 0 <= oy and ox + vw <= pixels.shape[1] and oy + vh <= pixels.shape[0], \
        "Actual native viewport lies outside compositor stream"
    # DWM describes the independently measured Windows visible frame. WGC/H264
    # can trim frame borders and align to even pixels: allow at most four SOURCE
    # pixels per axis, not four receiver pixels and not an arbitrary scale ratio.
    assert all(abs(actual - dwm) <= 4 for actual, dwm in zip(decoded, source)), \
        f"Decoded dimensions {decoded} do not match measured DWM source {source} within 4px border/alignment tolerance"
    sw, sh = decoded
    scale = min(vw / sw, vh / sh)
    expected = (ox + (vw - sw * scale) / 2, oy + (vh - sh * scale) / 2,
                sw * scale, sh * scale)
    observed = (left, int(top), right - left, int(bottom - top))
    assert all(abs(a - b) <= 4 for a, b in zip(observed, expected)), \
        f"Cropping/stretching/offset: measured image {observed}, expected {expected}, allocation {viewport}"
    if not letterbox:
        assert viewport == decoded and scale == 1.0, \
            f"One-to-one requires actual DrawingArea {viewport} == decoded dimensions {decoded}"
        # A decoded frame can contain black border pixels indistinguishable
        # from the canvas. Its nonblack bbox is not its full pixel extent.
        # Exact decoded/viewport equality establishes native scale; the image
        # geometry, four corners and unscaled OCR remain independently checked.
    for marker, box in markers.items():
        x, y = (box[0] + box[2]) / 2, (box[1] + box[3]) / 2
        nx, ny = (x - left) / (right - left), (y - top) / (bottom - top)
        assert (nx < .35 if marker in (1, 3) else nx > .65), f"Marker {marker} not at its source side"
        assert (ny < .35 if marker in (1, 2) else ny > .65), f"Marker {marker} not at its source edge"
    if letterbox:
        bar = int((vh - sh * scale) / 2)
        assert bar >= 30, "Fixture must exercise substantial unused area"
        for a, b in ((oy + 4, int(top) - 4), (int(bottom) + 4, oy + vh - 4)):
            strip = pixels[a:b, ox + 5:ox + vw - 5]
            assert strip.size and (strip.max(axis=2) <= 8).mean() >= .995, \
                "Unused native DrawingArea is not black-letterboxed"
    return origin, {"dwm_source_dimensions": source, "decoded_dimensions": decoded,
                    "dwm_border_alignment_tolerance_source_pixels": 4,
                    "viewport_allocation": viewport,
                    "viewport_origin_in_window_stream": (ox, oy), "image_bbox": observed,
                    "scale": scale, "corner_boxes": markers}


def resize(app, remote, target):
    area = receive.drawing_area(remote)
    deadline = time.monotonic() + app.timeout
    # Select EAST/SOUTH explicitly in separate normal WM operations. Selecting
    # WEST/NORTH with the first negative arrow reverses contraction semantics.
    with receive.DesktopControls(app, remote) as controls:
        active_grab = False
        try:
            for axis, select in ((0, 0xff53), (1, 0xff54)):
                before = receive.allocation_size(area)
                if before[axis] == target[axis]:
                    continue
                assert time.monotonic() < deadline, "Normal native resize exceeded deadline"
                active_grab = True
                controls.key(0xffe9, True)
                controls.key(0xffc5, True)  # Alt-F8: normal interactive resize
                controls.key(0xffc5, False)
                controls.key(0xffe9, False)
                time.sleep(.2)
                controls.key(select, True)  # Unmodified Right/Down selects EAST/SOUTH.
                controls.key(select, False)
                time.sleep(.2)
                while True:
                    current = receive.allocation_size(area)
                    assert current[1 - axis] == before[1 - axis], \
                        "Single-edge native resize changed the other allocation axis"
                    delta = target[axis] - current[axis]
                    if not delta:
                        break
                    assert time.monotonic() < deadline, "Normal native resize exceeded deadline"
                    symbol = ((0xff53 if delta > 0 else 0xff51) if axis == 0
                              else (0xff54 if delta > 0 else 0xff52))
                    controls.key(0xffe3, True)  # Control_L: measured fine resize.
                    for _ in range(min(abs(delta), 64)):
                        assert time.monotonic() < deadline, "Normal native resize exceeded deadline"
                        controls.key(symbol, True)
                        controls.key(symbol, False)
                        time.sleep(.05)
                    controls.key(0xffe3, False)
                    time.sleep(.2)
                    actual = receive.allocation_size(area)
                    assert abs(target[axis] - actual[axis]) < abs(delta), \
                        f"Normal native resize did not reduce axis {axis} error: {current} -> {actual}"
                controls.key(0xff0d, True)
                controls.key(0xff0d, False)  # Commit this edge BEFORE selecting next axis.
                active_grab = False
                time.sleep(.2)
        finally:
            try:
                for symbol in (0xffe9, 0xffe3, 0xffc5, 0xff53, 0xff51, 0xff54, 0xff52, 0xff0d):
                    controls.key(symbol, False)
            finally:
                if active_grab:
                    # Cancel only an unfinished owned resize so normal Leave can
                    # run; never send Escape after a committed operation.
                    try:
                        controls.key(0xff1b, True)
                    finally:
                        controls.key(0xff1b, False)
                    time.sleep(.2)
    actual = receive.allocation_size(area)
    assert actual == target, f"Normal OS resize did not reach actual native allocation {target}: {actual}"
    time.sleep(.5)


def observe(app, remote, meeting, capture, args, phase, source, number, origin=None, letterbox=True):
    deadline, error = time.monotonic() + args.timeout, "No frame"
    while time.monotonic() < deadline:
        current, main = receive.remote_and_main(app, 1)
        assert current == remote and main == meeting, "Original room/native windows were replaced"
        image = capture.image()
        receive.save_private_pixels(image, args.evidence_dir / f"{phase}-latest.png")
        try:
            markers = content(recognize(image), number)
            viewport = receive.allocation_size(receive.drawing_area(remote))
            if source is None:
                return None, {"viewport_allocation": viewport, "corner_boxes": markers}
            decoded = decoded_dimensions(app)
            observed_origin, proof = geometry(
                image, markers, source, decoded, viewport, origin, letterbox)
            path = args.evidence_dir / f"{phase}.json"
            path.write_text(json.dumps(proof, indent=2) + "\n")
            path.chmod(0o600)
            print(f"PASS: {phase}: actual compositor text, four corners, source geometry" +
                  (" and black letterboxes" if letterbox else " at one-to-one native scale"), flush=True)
            return observed_origin, proof
        except AssertionError as failure:
            error = str(failure)
        time.sleep(.5)
    raise AssertionError(f"{phase}: {error}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--backend-url", required=True)
    parser.add_argument("--room-code", required=True)
    parser.add_argument("--expected-peer-name", required=True)
    parser.add_argument("--initial-source", type=dimensions, required=True)
    parser.add_argument("--changed-source", type=dimensions, required=True)
    parser.add_argument("--resized-viewport", type=dimensions, default=(1000, 900))
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--feature", type=Path)
    args = parser.parse_args()
    assert args.timeout > 0
    assert abs(args.initial_source[0] / args.initial_source[1] -
               args.changed_source[0] / args.changed_source[1]) > .2, "Source aspects must actually differ"
    if args.feature:
        living_contract(args.feature)
    args.evidence_dir.mkdir(mode=0o700, parents=False, exist_ok=False)
    with receive.launch.NativeLaunch(args.executable, args.timeout, backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        receive.hosted.assert_join_code(app, args.room_code)
        receive.hosted.join_hosted_room(app, args.expected_peer_name)
        remote, meeting = receive.remote_and_main(app, args.timeout)
        capture = receive.WindowPixels(app, remote)
        try:
            _, before = observe(app, remote, meeting, capture, args, "initial-text", None, "12345")
            old = before["viewport_allocation"]
            resize(app, remote, args.resized_viewport)
            assert abs(old[0] / old[1] - args.resized_viewport[0] / args.resized_viewport[1]) > .2, \
                "Remote window did not change to a clearly different aspect ratio"
            origin, first = observe(app, remote, meeting, capture, args, "resized-initial",
                                    args.initial_source, "12345")
            print("FIDELITY_SOURCE_CHANGE_REQUIRED", flush=True)
            _, changed = observe(app, remote, meeting, capture, args, "changed-source",
                                 args.changed_source, "67890", origin)
            assert first["image_bbox"][2:] != changed["image_bbox"][2:], "Source pixel geometry never changed"
            resize(app, remote, tuple(changed["decoded_dimensions"]))
            observe(app, remote, meeting, capture, args, "one-to-one", args.changed_source,
                    "67890", origin, letterbox=False)
        finally:
            capture.close()
            # Normal Leave on success AND failed assertions before process teardown.
            receive.activate_owned_group_window(app, meeting, remote)
            receive.hosted.leave_hosted_room(app)
        deadline = time.monotonic() + min(args.timeout, 10)
        while any(node.get_name().startswith("Petal — ") for node in receive.owned_windows(app)):
            assert time.monotonic() < deadline, "Native Leave retained received window"
            time.sleep(.1)
    print("PASS: complete Task2.2 source fidelity scenario and normal native Leave", flush=True)


if __name__ == "__main__":
    main()
