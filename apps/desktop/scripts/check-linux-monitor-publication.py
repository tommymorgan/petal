#!/usr/bin/env python3
"""S01: real native portal -> publisher -> released Windows pixels and retirement.

Requires explicit permission to share the safe monitor and an already joined
released Windows peer in the named VirtualBox VM. Screenshots remain private.
No browser, synthetic media, extra SDK participant or receiver test API is used.
Import run_s01 to reuse an owned NativeLaunch and the actual SFU read observer.
"""
import argparse
from contextlib import contextmanager
import importlib.util
import json
import math
from pathlib import Path
import re
import secrets
import selectors
import subprocess
import sys
import time

from PIL import Image

_spec = importlib.util.spec_from_file_location(
    'monitor_audio_witness', Path(__file__).with_name('check-linux-share-audio-preservation.py'))
audio = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(audio)
hosted = audio.hosted


def counter_worker(nonce):
    import gi
    gi.require_version('Gtk', '3.0')
    from gi.repository import GLib, Gtk
    value = [10]
    window = Gtk.Window(title='Petal owned S01 monitor witness')
    window.set_default_size(1400, 650)
    window.connect('destroy', Gtk.main_quit)
    canvas = Gtk.DrawingArea()

    def draw(_widget, cr):
        cr.set_source_rgb(.08, .10, .14)
        cr.paint()
        cr.set_source_rgb(1, 1, 1)
        cr.select_font_face('monospace', 0, 1)
        cr.set_font_size(110)
        cr.move_to(80, 350)
        cr.show_text(nonce + ': ' + str(value[0]))

    def command(source, _condition):
        line = source.readline().strip()
        if line in ('', 'close'):
            window.destroy()
            return False
        if line == '11':
            value[0] = 11
            canvas.queue_draw()
        return True

    canvas.connect('draw', draw)
    window.add(canvas)
    GLib.io_add_watch(sys.stdin, GLib.IO_IN | GLib.IO_HUP, command)
    window.show_all()
    print('OWNED_COUNTER_READY', flush=True)
    Gtk.main()


@contextmanager
def owned_counter():
    # The nonce is never printed by the driver while capture is active. A stale
    # counter, terminal log, or another VM window cannot satisfy its pixel check.
    nonce = ''.join(secrets.choice('23456789') for _ in range(6))
    process = subprocess.Popen([sys.executable, str(Path(__file__).resolve()),
                                '--counter-worker', nonce],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True)
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ)
            assert selector.select(15), 'Owned native counter did not become ready'
            assert process.stdout.readline().strip() == 'OWNED_COUNTER_READY'
        yield nonce, process
    finally:
        if process.poll() is None:
            try:
                process.communicate('close\n', timeout=5)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.communicate(timeout=5)


def peer_frame(vm, path):
    result = subprocess.run(['VBoxManage', 'controlvm', vm, 'screenshotpng', str(path)],
                            capture_output=True, timeout=10)
    assert result.returncode == 0, 'Owned Windows VM screenshot failed; check its display without changing VM settings'
    path.chmod(0o600)
    # Small native title-bar text otherwise disappears from Tesseract's layout
    # analysis even when the source counter is legible. Retain the raw pixels.
    ocr_path = path.with_suffix('.ocr.png')
    with Image.open(path) as image:
        image.resize((image.width * 3, image.height * 3)).save(ocr_path)
    ocr_path.chmod(0o600)
    text = []
    # Sparse text finds the moving counter; block layout also retains the
    # colored native title bar when recursive desktop pictures confuse layout.
    for segmentation in ('11', '6'):
        result = subprocess.run(['tesseract', str(ocr_path), 'stdout', '--psm', segmentation],
                                capture_output=True, text=True, timeout=10)
        assert result.returncode == 0, 'Installed tesseract could not read the actual VM screenshot'
        text.append(result.stdout)
    return '\n'.join(text)


def run_s01(app, observe_participant, approve_share, windows_vm, evidence_dir, timeout=90):
    """Exercise the actual native Share/Stop controls; leave joined, muted, stopped.

    The caller owns room membership, explicit safe-monitor permission, and the
    VM. approve_share(label) approves that exact real portal source, never a mock.
    observe_participant reads this Linux participant from the actual owned SFU.
    """
    assert math.isfinite(timeout) and timeout >= 10
    evidence_dir = Path(evidence_dir)
    evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    microphone = audio.wait_microphone(observe_participant)
    assert microphone.get('muted', False), 'S01 witness requires an already-muted microphone'
    witness = audio.AudioWitness(app, observe_participant, True, microphone['sid'])
    witness.settled(False)
    try:
        clicked = audio.evaluate(app, '''(() => {
            const button = document.querySelector('button[aria-label="Share a window"]');
            if (!button || button.disabled) return false;
            button.click(); return true;
        })()''')
        assert clicked, 'Actual native Share control was unavailable'
        approve_share('S01')
        publication = witness.settled(True, timeout=timeout)
        with owned_counter() as (nonce, marker):
            for counter in (10, 11):
                if counter == 11:
                    marker.stdin.write('11\n')
                    marker.stdin.flush()
                deadline = time.monotonic() + timeout
                while True:
                    witness.sample()
                    text = peer_frame(windows_vm, evidence_dir / ('counter-' + str(counter) + '.png'))
                    if ('selected monitor' in text.lower()
                            and re.search(r'\b' + nonce + r'\s*:\s*' + str(counter) + r'\b', text)):
                        break
                    assert time.monotonic() < deadline, \
                        'Windows independent share did not render this run\'s actual counter ' + str(counter)
                    time.sleep(.2)
            clicked = audio.evaluate(app, '''(() => {
                const button = document.querySelector('button[aria-label="Stop sharing"]');
                if (!button || button.disabled) return false;
                button.click(); return true;
            })()''')
            assert clicked, 'Actual native Stop sharing control was unavailable'
            witness.settled(False, timeout=timeout)
            deadline, absent_since = time.monotonic() + timeout, None
            while True:
                witness.sample()
                text = peer_frame(windows_vm, evidence_dir / 'stopped.png')
                if 'selected monitor' not in text.lower():
                    absent_since = absent_since or time.monotonic()
                    if time.monotonic() - absent_since >= 2:
                        break
                else:
                    absent_since = None
                assert time.monotonic() < deadline, 'Windows independent share window did not retire'
                time.sleep(.2)
        return {'scenario': 'S01', 'counters': [10, 11],
                'nativeShareId': publication[0][0], 'videoSid': publication[1][0],
                'realWindowsPixels': True, 'publicationRemoved': True, 'remoteWindowClosed': True}
    finally:
        witness.action('stop_all_shares', 'S01-cleanup', timeout=timeout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--backend-url', required=True)
    parser.add_argument('--room-code', required=True)
    parser.add_argument('--expected-peer-name', required=True)
    parser.add_argument('--windows-vm', required=True)
    parser.add_argument('--livekit-http-url', required=True)
    parser.add_argument('--livekit-room', required=True)
    parser.add_argument('--room-admin-token-file', type=Path, required=True)
    parser.add_argument('--evidence-dir', type=Path, required=True)
    parser.add_argument('--timeout', type=float, default=90)
    parser.add_argument('--allow-desktop-share', action='store_true')
    args = parser.parse_args()
    if not args.allow_desktop_share:
        parser.error('Explicit safe-monitor authorization and --allow-desktop-share are required')
    with hosted.launch.NativeLaunch(args.executable, args.timeout, backend_url=args.backend_url) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        hosted.assert_join_code(app, args.room_code)
        hosted.join_hosted_room(app, args.expected_peer_name)
        try:
            roster = audio.invoke(app, 'room_presence')
            local = [participant for participant in roster if participant['isLocal']]
            assert len(local) == 1
            observe = audio.livekit_observer(args.livekit_http_url, args.room_admin_token_file,
                                             args.livekit_room, local[0]['identity'])
            result = run_s01(app, observe, lambda label: print(
                'APPROVE_OWNED_SAFE_MONITOR ' + label + ' (keep a preselected source pressed)',
                flush=True), args.windows_vm, args.evidence_dir, args.timeout)
            receipt = args.evidence_dir / 'monitor-publication.json'
            receipt.write_text(json.dumps(result, indent=2) + '\n')
            receipt.chmod(0o600)
        finally:
            hosted.leave_hosted_room(app)
    print('PASS: Windows rendered this run\'s counter 10 -> 11; normal Stop retired publication and window')


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--counter-worker':
        counter_worker(sys.argv[2])
    else:
        main()
