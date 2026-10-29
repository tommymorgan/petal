#!/usr/bin/env python3
"""Real native Task1.2 picture sources and selected-window privacy witness.

Import run_s02 into an already joined, muted native room with the existing
Windows peer. Callbacks must select/focus only these owned GTK sources through
the actual desktop; no camera, remote-input grant or fabricated media is used.
"""
import argparse
from contextlib import contextmanager
import importlib.util
import json
from pathlib import Path
import re
import secrets
import selectors
import subprocess
import sys
import time

_spec = importlib.util.spec_from_file_location(
    'picture_monitor_witness', Path(__file__).with_name('check-linux-monitor-publication.py'))
monitor = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(monitor)
audio = monitor.audio


def picture_worker(label, nonce, initial, width, height):
    import gi
    gi.require_version('Gtk', '3.0')
    from gi.repository import GLib, Gtk
    value = [initial]
    window = Gtk.Window(title=f'Petal owned {label} {nonce}')
    window.set_default_size(width, height)
    window.connect('destroy', Gtk.main_quit)
    canvas = Gtk.DrawingArea()

    def draw(widget, cr):
        w, h = widget.get_allocated_width(), widget.get_allocated_height()
        cr.set_source_rgb(.07, .09, .13)
        cr.paint()
        cr.set_source_rgb(.20, .24, .30)
        cr.set_line_width(1)
        for x in range(0, w, 40):
            cr.move_to(x, 0)
            cr.line_to(x, h)
        for y in range(0, h, 40):
            cr.move_to(0, y)
            cr.line_to(w, y)
        cr.stroke()
        cr.select_font_face('monospace', 0, 1)
        cr.set_font_size(28)
        for text, x, y, color in (
            ('TL', 12, 36, (1, .25, .25)),
            ('TR', max(12, w - 68), 36, (.25, 1, .25)),
            ('BL', 12, max(36, h - 16), (.35, .55, 1)),
            ('BR', max(12, w - 68), max(36, h - 16), (1, 1, .25)),
        ):
            cr.set_source_rgb(*color)
            cr.move_to(x, y)
            cr.show_text(text)
        cr.set_source_rgb(1, 1, 1)
        cr.set_font_size(min(74, max(28, w / 15)))
        cr.move_to(60, h / 2)
        cr.show_text(nonce + ': ' + str(value[0]))
        cr.set_font_size(28)
        cr.move_to(60, h / 2 + 56)
        cr.show_text(label + ' — actual GTK source')

    def command(source, _condition):
        fields = source.readline().strip().split()
        if not fields or fields == ['close']:
            window.destroy()
            return False
        if len(fields) == 2 and fields[0] == 'set':
            value[0] = int(fields[1])
            canvas.queue_draw()
        elif len(fields) == 3 and fields[0] == 'resize':
            w, h = int(fields[1]), int(fields[2])
            assert 300 <= w <= 2000 and 250 <= h <= 1600
            window.resize(w, h)
        elif fields == ['fullscreen']:
            window.fullscreen()
        else:
            raise AssertionError('Unknown owned picture source command')
        return True

    canvas.connect('draw', draw)
    window.add(canvas)
    GLib.io_add_watch(sys.stdin, GLib.IO_IN | GLib.IO_HUP, command)
    window.show_all()
    print('OWNED_PICTURE_READY', flush=True)
    Gtk.main()


@contextmanager
def owned_picture(label, initial, width=1280, height=720):
    assert re.fullmatch('[A-Z][A-Z0-9_-]{0,20}', label)
    nonce = ''.join(secrets.choice('23456789') for _ in range(8))
    title = f'Petal owned {label} {nonce}'
    process = subprocess.Popen(
        [sys.executable, str(Path(__file__).resolve()), '--picture-worker',
         label, nonce, str(initial), str(width), str(height)],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True,
    )
    try:
        with selectors.DefaultSelector() as ready:
            ready.register(process.stdout, selectors.EVENT_READ)
            assert ready.select(15), 'Owned GTK picture source did not become ready'
            assert process.stdout.readline().strip() == 'OWNED_PICTURE_READY'
        yield nonce, title, process
    finally:
        if process.poll() is None:
            try:
                process.communicate('close\n', timeout=5)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.communicate(timeout=5)


def source_command(process, command):
    assert process.poll() is None, 'Owned GTK source unexpectedly exited'
    process.stdin.write(command + '\n')
    process.stdin.flush()


def visible_button(app, label):
    return audio.evaluate(app, '''(() => {
        const label = %s;
        const buttons = [...document.querySelectorAll('button')].filter(button =>
            button.getClientRects().length > 0 && getComputedStyle(button).visibility !== 'hidden' &&
            (button.getAttribute('aria-label') === label || button.textContent.trim() === label));
        if (buttons.length !== 1 || buttons[0].disabled) return false;
        buttons[0].click();
        return true;
    })()''' % json.dumps(label))


def click_add_share(app):
    if not visible_button(app, 'Add share'):
        assert visible_button(app, 'More meeting controls'), 'Actual native More control is unavailable'
        deadline = time.monotonic() + 5
        while not visible_button(app, 'Add share'):
            assert time.monotonic() < deadline, 'Actual native Add share affordance is unavailable'
            time.sleep(.05)


@contextmanager
def native_picker_events(app):
    """Observe real production picker events; never replace native invoke."""
    event_id = audio.hosted.native_async(app, """
        if (window.__linuxPicturePickerWitness != null) throw new Error('Picker observer already active');
        const events = [];
        const callback = window.__TAURI_INTERNALS__.transformCallback(event => events.push(event.payload));
        window.__linuxPicturePickerWitness = {events, callback};
        return await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
            event: 'linux-share-picker-open', target: {kind: 'Any'}, handler: callback
        });
    """)
    try:
        yield lambda: audio.evaluate(app, 'window.__linuxPicturePickerWitness.events')
    finally:
        audio.hosted.native_async(app, """
            window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener('linux-share-picker-open', %s);
            await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten', {
                event: 'linux-share-picker-open', eventId: %s
            });
            window.__TAURI_INTERNALS__.unregisterCallback(window.__linuxPicturePickerWitness.callback);
            delete window.__linuxPicturePickerWitness;
            return true;
        """ % (json.dumps(event_id), json.dumps(event_id)))


def current_pictures(app, witness):
    participant = witness.sample()
    pictures = audio.invoke(app, 'linux_picture_shares')
    ids = audio.invoke(app, 'shared_window_ids')
    assert sorted(ids) == sorted(picture['windowId'] for picture in pictures), \
        'Native committed source summaries disagree with active share IDs'
    video = [track for track in participant.get('tracks', []) if track.get('type') in (1, 'VIDEO')]
    assert len(video) == len(ids), 'Native active source count differs from actual SFU video publications'
    by_name = {track.get('name'): track for track in video}
    assert len(by_name) == len(video), 'Ambiguous native source publication names'
    for source in pictures:
        track = by_name.get('petal-window-' + str(source['windowId']))
        assert track is not None and track.get('sid'), 'Missing actual SFU publication for current source'
        assert track.get('source') in (3, 'SCREEN_SHARE'), 'Picture is not an actual screen-share publication'
    return pictures, by_name


def run_s02(app, observe_participant, select_window, activate_source,
            windows_vm, evidence_dir, timeout=90):
    """Actual Add -> native window permission -> Windows pixels, not a decode buffer.

    select_window(title, pid) must approve only that owned window in the actual
    native chooser. activate_source(title, pid) must establish actual focus on
    the unrelated owned source, not merely request it. A desktop-capture mutant
    then sees the private source instead of the selected public counter.
    """
    evidence_dir = Path(evidence_dir)
    evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    mic = audio.wait_microphone(observe_participant)
    assert mic.get('muted', False), 'Selected-window witness requires a muted original microphone'
    witness = audio.AudioWitness(app, observe_participant, True, mic['sid'])
    witness.settled(False, timeout=timeout)
    publication = None
    try:
        with owned_picture('PUBLIC_EDITOR', 20) as public, owned_picture('PRIVATE', 99, 900, 500) as private:
            public_nonce, public_title, public_process = public
            private_nonce, private_title, private_process = private
            click_add_share(app)
            select_window(public_title, public_process.pid)
            deadline = time.monotonic() + timeout
            while True:
                witness.sample()
                ids = audio.invoke(app, 'shared_window_ids')
                if ids:
                    publication = witness.settled(True, timeout=timeout)
                    break
                assert time.monotonic() < deadline, 'Explicitly selected application window did not publish'
                time.sleep(.1)
            pictures, _ = current_pictures(app, witness)
            assert len(pictures) == 1 and pictures[0]['sourceKind'] == 'window', \
                'The actual selected source must be a window, never a desktop fallback'

            def read_counter(value):
                deadline = time.monotonic() + timeout
                while True:
                    assert witness.settled(True, timeout=timeout) == publication
                    text = monitor.peer_frame(windows_vm, evidence_dir / ('counter-' + str(value) + '.png'))
                    assert private_nonce not in text, 'Unrelated private desktop source leaked into Windows pixels'
                    if re.search(r'\b' + public_nonce + r'\s*:\s*' + str(value) + r'\b', text):
                        return
                    assert time.monotonic() < deadline, 'Windows did not render selected window counter ' + str(value)
                    time.sleep(.2)

            read_counter(20)
            activate_source(private_title, private_process.pid)
            source_command(public_process, 'set 21')
            read_counter(21)
            pictures, _ = current_pictures(app, witness)
            assert len(pictures) == 1 and pictures[0]['sourceKind'] == 'window'
            return {'scenario': 'S02', 'actualWindowsCounters': [20, 21],
                    'unrelatedOwnedSourceExcluded': True, 'nativeSourceKind': 'window',
                    'nativeShareId': publication[0][0], 'videoSid': publication[1][0],
                    'microphoneSamples': witness.samples, 'muted': True}
    finally:
        witness.action('stop_all_shares', 'S02-owned-cleanup', timeout=timeout)
        witness.settled(False, timeout=timeout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--picture-worker', nargs=5, metavar=('LABEL', 'NONCE', 'INITIAL', 'WIDTH', 'HEIGHT'))
    args = parser.parse_args()
    if args.picture_worker:
        label, nonce, initial, width, height = args.picture_worker
        assert re.fullmatch('[A-Z][A-Z0-9_-]{0,20}', label)
        assert re.fullmatch('[23456789]{8}', nonce)
        picture_worker(label, nonce, int(initial), int(width), int(height))
    else:
        parser.error('Import run_s02 with an owned native room and authorized source-selection callbacks')


if __name__ == '__main__':
    main()
