#!/usr/bin/env python3
"""Diagnose native Linux device selection/retry without claiming audible speech.

Uses a real owned hosted service and physical devices. A stale saved device
selection is the failure fixture, not an injected audio failure or fake ADM.
The microphone remains muted throughout. This reads bounded OS routing metadata,
not audio samples. It does not bind human speech or all-hardware-unavailable
scenarios.
"""
import argparse
import importlib.util
import json
import re
import subprocess
import time
from pathlib import Path

spec = importlib.util.spec_from_file_location('hosted', Path(__file__).with_name('check-linux-hosted-entry.py'))
hosted = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hosted)


def wait(app, predicate, message, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        assert app.process.poll() is None, 'Owned native app exited'
        if predicate():
            return
        time.sleep(.1)
    body = app.native_input.evaluate(app.observe_processes(), 'document.body.innerText')
    body = re.sub(r'[a-z]{3}-[a-z]{4}-[a-z]{3}', '<private-room>', body)
    raise AssertionError(message + '; native UI: ' + body[:3000])


def click(app, selector, text=None):
    return app.native_input.evaluate(app.observe_processes(), """(() => {
      const nodes = [...document.querySelectorAll(%s)];
      const button = nodes.find(node => !node.disabled && node.getClientRects().length &&
        (%s === null || node.textContent.trim() === %s));
      if (!button) return false;
      button.click(); return true;
    })()""" % (json.dumps(selector), json.dumps(text), json.dumps(text)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--backend-url', required=True)
    parser.add_argument('--room-code', required=True)
    parser.add_argument('--recording-name', required=True)
    parser.add_argument('--playout-name', required=True)
    parser.add_argument('--evidence-dir', required=True, type=Path)
    parser.add_argument('--processed-source',
                        help='Existing OS processing source to verify back to the selected physical input; does not change system routing')
    args = parser.parse_args()
    args.evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    with hosted.launch.NativeLaunch(args.executable, 40, backend_url=args.backend_url,
                                    profile_root=str(args.evidence_dir)) as app:
        app.assert_executable()
        app.assert_actual_desktop()
        app.launch()
        app.assert_room_entry()
        devices = hosted.native_async(app, "return await window.__TAURI_INTERNALS__.invoke('list_audio_devices');")
        chosen = {}
        for direction, name in [('recording', args.recording_name), ('playout', args.playout_name)]:
            matches = [d for d in devices[direction] if d['name'] == name]
            assert len(matches) == 1 and matches[0]['id'], 'Selected physical device is not uniquely addressable'
            chosen[direction] = matches[0]
        hosted.native_async(app, "return await window.__TAURI_INTERNALS__.invoke('set_audio_devices', {recordingId:'petal-unavailable-saved-input',playoutId:'petal-unavailable-saved-output'});")
        hosted.assert_join_code(app, args.room_code)
        assert click(app, 'form button', 'Join'), 'Real native Join control unavailable'
        wait(app, lambda: hosted.native_async(app, "return (await window.__TAURI_INTERNALS__.invoke('current_room')) !== null;"), 'Native room did not join')
        wait(app, lambda: app.native_input.evaluate(app.observe_processes(), "document.body.innerText.includes('Microphone unavailable:') && document.body.innerText.includes('Speaker unavailable:')"), 'Initial device failures were not shown as actionable native UI errors')
        before = (args.evidence_dir/'process.log').read_text(errors='replace')
        assert 'audio: published microphone track' not in before, 'Unavailable selected microphone silently published from another device'
        print('PASS: unavailable saved devices show actionable native errors, retain the room and do not publish a substitute microphone', flush=True)
        assert click(app, 'button[aria-label="Microphone options"]'), 'Visible device menu unavailable'
        wait(app, lambda: app.native_input.evaluate(app.observe_processes(), "!!document.querySelector('[role=dialog][aria-label=\"Audio devices\"] [data-device-kind=microphone]')"), 'Real device picker did not enumerate')
        assert click(app, 'button[data-device-kind=microphone]', chosen['recording']['name']), 'Real microphone option not actionable'
        wait(app, lambda: app.native_input.evaluate(app.observe_processes(), "document.body.innerText.includes('Switched microphone')"), 'Microphone retry did not report a live result')
        assert click(app, 'button[data-device-kind=speaker]', chosen['playout']['name']), 'Real speaker option not actionable'
        wait(app, lambda: app.native_input.evaluate(app.observe_processes(), "document.body.innerText.includes('Switched speaker')"), 'Speaker retry did not report a live result')
        # Observe actual native transport/publication rather than a command echo.
        after = (args.evidence_dir/'process.log').read_text(errors='replace')
        assert after.count('RoomConnection connected:') == 1, 'Recovery reconnected or duplicated the room'
        assert after.count('audio: published microphone track') == 1, 'Recovery duplicated or omitted the microphone publication'
        assert 'audio: pre-muted microphone track before publish' in after
        assert 'audio: mic track unmuted' not in after, 'Diagnostic unexpectedly transmitted speech'
        state = hosted.native_async(app, "return await window.__TAURI_INTERNALS__.invoke('get_menubar_state');")
        assert state['micMuted'] is True
        # Native capture routing is observable while the published track is
        # muted. Never enable real speech merely to inspect OS metadata.
        assert click(app, 'button[aria-label="Microphone options"]'), 'Could not close device menu'
        pids = app.observe_processes()
        def os_json(command):
            return json.loads(subprocess.check_output(command, text=True, timeout=3))
        source_rows = os_json(['pactl','-f','json','list','sources'])
        sources = {row['index']: row['name'] for row in source_rows}
        targets = [row['name'] for row in source_rows if row.get('description') == args.recording_name]
        assert len(targets) == 1, 'Physical PulseAudio device is not uniquely identified'
        target = targets[0]
        def selected_capture():
            streams = os_json(['pactl','-f','json','list','source-outputs'])
            clients = os_json(['pactl','-f','json','list','clients'])
            owned_clients = {str(row['index']) for row in clients if str(row.get('properties',{}).get('application.process.id','')).isdigit() and int(row['properties']['application.process.id']) in pids}
            owned = [row for row in streams if str(row.get('client')) in owned_clients]
            for row in owned:
                actual = sources.get(row['source'],'')
                requested = row.get('properties',{}).get('target.object')
                path = []
                if actual == target:
                    path = [target]
                elif args.processed_source and actual == args.processed_source and requested == target:
                    # Respect an existing system processing chain. Prove the
                    # selected hardware is upstream, not merely its label.
                    graph = os_json(['pw-dump'])
                    names = {item['id']:item.get('info',{}).get('props',{}).get('node.name','') for item in graph if item.get('type')=='PipeWire:Interface:Node'}
                    links = [(item['info']['output-node-id'],item['info']['input-node-id']) for item in graph if item.get('type')=='PipeWire:Interface:Link']
                    pending = [(node,[actual]) for node,name in names.items() if name==actual]
                    visited = set()
                    while pending:
                        node, chain = pending.pop(0)
                        if node in visited:
                            continue
                        visited.add(node)
                        if names[node] == target:
                            path = chain
                            break
                        pending.extend((source,chain+[names.get(source,'')]) for source,destination in links if destination==node)
                if path:
                    (args.evidence_dir/'capture-route.json').write_text(json.dumps({
                        'effectiveSource':actual,'requestedTarget':requested,
                        'physicalSource':target,'verifiedUpstreamPath':path
                    },indent=2)+'\n')
                    return True
            return False
        wait(app, selected_capture, 'Owned muted capture is not connected to the selected physical input', timeout=4)
        state = hosted.native_async(app, "return await window.__TAURI_INTERNALS__.invoke('get_menubar_state');")
        assert state['micMuted'] is True
        assert 'audio: mic track unmuted' not in (args.evidence_dir/'process.log').read_text(errors='replace')
        print('PASS: visible retry selected the real capture device, retained one room and one muted microphone publication; speaker initialization succeeded (audibility unverified)', flush=True)
        hosted.leave_hosted_room(app)
        app.assert_no_browser()
        app.assert_no_foreign_setup()
        (args.evidence_dir/'audio-devices.json').write_text(json.dumps({'staleSelectionsRejected':True,'oneRoom':True,'oneMutedMicrophone':True,'physicalCaptureSelected':True,'speakerInitialized':True,'audibleSpeechVerified':False,'normalLeave':True}, indent=2)+'\n')


if __name__ == '__main__':
    main()
