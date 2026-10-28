#!/usr/bin/env python3
"""Witness Task2.3 using an authorized real-WGC Windows publisher.

Start windows_wgc_race_publisher with a changing windows-receive-editor source
and -ReplacementPhaseFile initially a. At REPLACEMENT_PUBLICATION_REQUIRED,
append publish-b to the publisher control file and change the source phase to b.
At OLD_PUBLICATION_RETIREMENT_REQUIRED, append retire-a. This witness reads
actual SDK event logs and captures only its original native GTK window. It never
injects media, SDK events, renderer state or receiver lifecycle commands.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import time

_spec = importlib.util.spec_from_file_location(
    "fidelity", Path(__file__).with_name("check-linux-receive-fidelity.py"))
fidelity = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fidelity)
receive = fidelity.receive

SCENARIO = "Late teardown cannot retire a replacement share"
STEPS = (
    "Given Linux is displaying a peer's share publication A",
    'When replacement publication B for that same share arrives showing "Replacement B"',
    "And publication A's delayed teardown arrives afterward",
    "Then exactly one remote window remains open for the share",
    "And it continues showing publication B's current changing content",
    "And publication A cannot replace or close that presentation",
)
EVENT = re.compile(r"linux incoming (subscribed|unpublished) window_id=(\d+) sid=(\S+)")


def living_contract(path):
    selected, found, matches, feature = False, [], 0, False
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("Feature:"):
            feature = True
            continue
        assert not line.startswith(("Background:", "Rule:")), "Unbound replacement Background/Rule"
        if line.startswith(("Scenario:", "Scenario Outline:")):
            kind, _, name = line.partition(":")
            selected = name.strip() == SCENARIO
            if selected:
                matches += 1
                assert kind == "Scenario", "Replacement must retain its complete scenario"
            continue
        if selected:
            found.append(line)
    assert feature and matches == 1 and tuple(found) == STEPS, \
        "Replacement witness requires every exact Task2.3 clause"


def events(app):
    # These lines are emitted by the original RoomConnection's production
    # compositor event consumer, not an additional participant or SDK observer.
    text = (app.trace.parent / "process.log").read_text(errors="replace")
    return [(match[1], int(match[2]), match[3]) for match in EVENT.finditer(text)]


def original_windows(app, remote, main):
    current, meeting = receive.remote_and_main(app, 1)
    assert current == remote and meeting == main, "Original native windows were replaced"
    receive.drawing_area(current)


def pixels(capture, directory, phase, label):
    image = capture.image()
    receive.save_private_pixels(image, directory / f"{phase}-latest.png")
    text = "\n".join(line for line, _ in fidelity.recognize(image))
    assert re.search(r"PETAL\s+SAFE\s+EDITOR", text, re.I), "Missing actual safe editor pixels"
    assert label in text, f"Missing current source label {label}"
    other = "Publication A" if label == "Replacement B" else "Replacement B"
    assert other not in text, f"Previous publication image {other} replaced the current presentation"
    match = re.search(r"PETAL\s+RECEIVE\s+COUNTER\s*[:=]?\s*(\d+)", text, re.I)
    assert match, "Missing real editor counter"
    return int(match[1])


def observe(app, remote, main, capture, args, phase, label, *, after=None,
            old_sid=None, require_retired=False):
    deadline = time.monotonic() + args.timeout
    counters, failure = [], "No current native image"
    while time.monotonic() < deadline:
        original_windows(app, remote, main)
        observed = events(app)
        retired = old_sid is not None and any(
            kind == "unpublished" and sid == old_sid for kind, _, sid in observed)
        if old_sid is not None and not require_retired:
            assert not retired, "A teardown arrived before replacement B was visibly established"
        try:
            value = pixels(capture, args.evidence_dir, phase, label)
        except AssertionError as error:
            failure = str(error)
            if require_retired and "Previous publication image" in str(error):
                raise
        else:
            if (after is None or value > after) and (not counters or value > counters[-1]):
                if not require_retired or retired:
                    counters.append(value)
                    if len(counters) == 3:
                        print(f"PASS: {phase}: original native window displays {label} and changing counters",
                              flush=True)
                        return counters
        time.sleep(.3)
    raise AssertionError(f"{phase}: {failure}; observed counters {counters}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--backend-url", required=True)
    parser.add_argument("--room-code", required=True)
    parser.add_argument("--expected-peer-name", default="Native WGC race fixture")
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--feature", type=Path)
    args = parser.parse_args()
    assert args.timeout > 0
    if args.feature:
        living_contract(args.feature)
    args.evidence_dir.mkdir(mode=0o700, parents=False, exist_ok=False)
    with receive.launch.NativeLaunch(args.executable, args.timeout,
                                    backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        receive.hosted.assert_join_code(app, args.room_code)
        receive.hosted.join_hosted_room(app, args.expected_peer_name)
        remote, main_window = receive.remote_and_main(app, args.timeout)
        capture = None
        try:
            capture = receive.WindowPixels(app, remote)
            initial = observe(app, remote, main_window, capture, args, "initial", "Publication A")
            first = events(app)
            subscriptions = [(window, sid) for kind, window, sid in first if kind == "subscribed"]
            assert len(subscriptions) == 1, "Initial native share publication is ambiguous"
            window_id, old_sid = subscriptions[0]
            assert not any(kind == "unpublished" for kind, _, _ in first), \
                "Initial share was already retired"
            print("REPLACEMENT_PUBLICATION_REQUIRED", flush=True)
            replacement = observe(app, remote, main_window, capture, args, "replacement",
                                  "Replacement B", after=initial[-1], old_sid=old_sid)
            middle = events(app)
            subscriptions = [(window, sid) for kind, window, sid in middle if kind == "subscribed"]
            assert len(subscriptions) == 2, "Expected exactly A and B subscriptions"
            new_window, new_sid = subscriptions[1]
            assert new_window == window_id and new_sid != old_sid, \
                "B must replace the same share with a genuinely distinct publication SID"
            print("OLD_PUBLICATION_RETIREMENT_REQUIRED", flush=True)
            continuing = observe(app, remote, main_window, capture, args, "after-old-retirement",
                                 "Replacement B", after=replacement[-1], old_sid=old_sid,
                                 require_retired=True)
            final = events(app)
            assert final.index(("subscribed", window_id, new_sid)) < \
                final.index(("unpublished", window_id, old_sid)), "A teardown was not late"
            assert ("unpublished", window_id, new_sid) not in final, "Current publication B was retired"
            assert final.count(("subscribed", window_id, old_sid)) == 1, "A unexpectedly resubscribed"
            proof = {"events": final, "initial_counters": initial,
                     "replacement_counters": replacement, "after_retirement_counters": continuing}
            path = args.evidence_dir / "replacement.json"
            path.write_text(json.dumps(proof, indent=2) + "\n")
            path.chmod(0o600)
        finally:
            if capture is not None:
                capture.close()
            receive.hosted.leave_hosted_room(app)
        deadline = time.monotonic() + min(args.timeout, 10)
        while any(node.get_name().startswith("Petal — ") for node in receive.owned_windows(app)):
            assert time.monotonic() < deadline, "Normal Leave retained the native received window"
            time.sleep(.1)
    print("PASS: complete Task2.3 real-WGC replacement, late A teardown and normal native Leave", flush=True)


if __name__ == "__main__":
    main()
