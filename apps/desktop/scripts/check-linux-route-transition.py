#!/usr/bin/env python3
"""Check native SPA navigation when a genuine WebKit transition cannot update."""
import argparse
import importlib.util
from pathlib import Path
import time

spec = importlib.util.spec_from_file_location("native_launch", Path(__file__).with_name("check-linux-launch.py"))
launch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launch)


def evaluate(app, expression):
    return app.native_input.evaluate(app.observe_processes(), expression)


def wait_for_route(app, route, selector):
    deadline = time.monotonic() + 2
    snapshot = None
    while time.monotonic() < deadline:
        snapshot = evaluate(app, """({
            route: location.pathname,
            mounted: !!document.querySelector(%r),
            sameDocument: window.__routeDocumentWitness === 'native-route-witness'
        })""" % selector)
        if snapshot["route"] == route and snapshot["mounted"] and snapshot["sameDocument"]:
            return
        time.sleep(0.05)
    raise AssertionError(f"Native route did not mount without a rendering callback: {snapshot}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    args = parser.parse_args()
    with launch.NativeLaunch(args.executable, 35) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        # Hold the genuine engine's update callback, not Svelte's router, DOM,
        # application stores or navigation-complete promise. This makes a
        # suspended rendering clock reproducible without blanking the desktop.
        assert evaluate(app, """(() => {
            if (typeof document.startViewTransition !== 'function') return false;
            window.__routeDocumentWitness = 'native-route-witness';
            window.__routeTransitionRequested = false;
            const start = document.startViewTransition.bind(document);
            const gate = new Promise(resolve => { window.__releaseRouteTransition = resolve; });
            document.startViewTransition = update => {
                document.startViewTransition = start;
                window.__routeTransitionRequested = true;
                const transition = start(async () => { await gate; await update(); });
                window.__heldRouteTransition = transition;
                transition.ready.catch(() => {});
                transition.finished.catch(() => {});
                return transition;
            };
            window.__TAURI_INTERNALS__.invoke('open_main_route', {route: '/meeting/abc-defg-hjk'})
                .catch(error => { window.__routeCommandError = String(error); });
            return true;
        })()""") is True, "Native WebKit does not expose the transition under test"
        try:
            wait_for_route(app, "/meeting/abc-defg-hjk", 'button[aria-label="Leave meeting"]')
            assert evaluate(app, "window.__routeTransitionRequested") is True, \
                "Navigation did not exercise the production transition hook"
            print("PASS: actual native meeting controls mounted while the genuine transition update remained held", flush=True)
        finally:
            evaluate(app, """(() => {
                window.__releaseRouteTransition();
                window.__heldRouteTransition?.skipTransition();
                return true;
            })()""")
        assert evaluate(app, """(() => {
            const leave = document.querySelector('button[aria-label="Leave meeting"]');
            if (!leave || leave.disabled) return false;
            leave.click();
            return true;
        })()""") is True, "Normal native Leave control was unavailable"
        wait_for_route(app, "/main", 'input[aria-label="Meeting name, invite link, or meeting code"]')
        print("PASS: same-document native return to room entry after the delayed callback was released", flush=True)
    print("PASS: native WebKit navigation without a browser, hosted-media claim or desktop power-policy change")


if __name__ == "__main__":
    main()
