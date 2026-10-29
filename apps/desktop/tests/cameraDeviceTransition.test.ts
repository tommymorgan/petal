import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import ts from 'typescript';

// Execute the shipped IPC adapter. The native boundary below enforces the
// advertised device modes instead of echoing the arguments under test.
const nativeRequire = createRequire(import.meta.url);
const modes = {
  A: [{ width: 1280, height: 720, frameRateNumerator: 10, frameRateDenominator: 1 }],
  B: [{ width: 640, height: 480, frameRateNumerator: 30, frameRateDenominator: 1 }],
  C: [{ width: 1280, height: 720, frameRateNumerator: 15, frameRateDenominator: 2 }],
  D: [{ width: 1280, height: 720, frameRateNumerator: 30, frameRateDenominator: 1 }]
};
type Mode = { width: number; height: number; frameRate: number } | null;
function harness(query?: (id: string) => Promise<unknown>, platform = 'linux') {
  const session = { cameraDeviceId: 'A', cameraMode: { width: 1280, height: 720, frameRate: 10 } as Mode };
  let published = { deviceId: 'A', mode: session.cameraMode };
  const commands: string[] = [];
  const invoke = async (command: string, args: {
    preferredDeviceId: keyof typeof modes;
    deviceId: keyof typeof modes;
    width?: number | null;
    height?: number | null;
    frameRate?: number | null;
  }) => {
    if (command === 'list_camera_modes') {
      return query ? await query(args.preferredDeviceId) : modes[args.preferredDeviceId as keyof typeof modes];
    }
    assert.equal(command, 'set_camera_device');
    commands.push(args.deviceId);
    // Missing atomic mode fields reproduce the old native path: it inherits
    // A's explicit mode before the asynchronously normalized mode can arrive.
    const mode = 'width' in args
      ? args.width == null ? null : { width: args.width, height: args.height!, frameRate: args.frameRate! }
      : published.mode;
    const supported = modes[args.deviceId as keyof typeof modes];
    if (mode && !supported.some((m) => m.width === mode.width && m.height === mode.height &&
      m.frameRateNumerator / m.frameRateDenominator === mode.frameRate)) {
      return { applied: false, inRoom: true, error: 'The selected camera mode is not available' };
    }
    published = { deviceId: args.deviceId, mode };
    return { applied: true, inRoom: true, error: null };
  };
  const cache = new Map<string, object>();
  const require = (id: string): object => {
    if (id === '@tauri-apps/api/core') return { invoke };
    if (id === '$lib/ipc') return { hasTauriBridge: () => true, COMMANDS: {
      listCameraModes: 'list_camera_modes', setCameraDevice: 'set_camera_device'
    } };
    if (id === '$lib/platform') return { isLinux: () => platform === 'linux', isMac: () => platform === 'macos' };
    if (id === '$lib/stores/session.svelte') return {
      session,
      updateCameraMode: (mode: Mode) => { session.cameraMode = mode; },
      updateAudioDevices: (_mic: unknown, _speaker: unknown, id: string) => { session.cameraDeviceId = id; }
    };
    if (id.startsWith('$lib/data/') || id.startsWith('./')) {
      const name = id.split('/').at(-1)!;
      if (cache.has(name)) return cache.get(name)!;
      const source = readFileSync(new URL(`../src/lib/data/${name}.ts`, import.meta.url), 'utf8');
      const exports = {};
      cache.set(name, exports);
      const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
      runInNewContext(code, { exports, require, console });
      return exports;
    }
    return nativeRequire(id);
  };
  const adapter = require('$lib/data/cameraDevices') as {
    setCameraDevice(id: string): Promise<{ applied: boolean; error?: string }>;
  };
  return { adapter, session, commands, published: () => published };
}

test('switching a publishing camera submits a supported new-device mode atomically', async () => {
  const h = harness();
  const result = await h.adapter.setCameraDevice('B');
  assert.equal(result.applied, true, result.error);
  assert.equal(h.published().deviceId, 'B');
  assert.equal(h.published().mode, null, 'an unavailable resolution selects Auto');
});

test('same-resolution device switching uses its advertised fractional rate', async () => {
  const h = harness();
  const result = await h.adapter.setCameraDevice('C');
  assert.equal(result.applied, true, result.error);
  assert.equal(h.published().mode?.frameRate, 7.5);
});

test('Windows device switching normalizes unsupported resolution and preset rate before publication', async () => {
  const h = harness(undefined, 'windows');
  const changedRate = await h.adapter.setCameraDevice('D');
  assert.equal(changedRate.applied, true, changedRate.error);
  assert.equal(h.published().mode?.frameRate, 30);
  assert.equal(h.session.cameraMode?.frameRate, 30);
  const changedSize = await h.adapter.setCameraDevice('B');
  assert.equal(changedSize.applied, true, changedSize.error);
  assert.equal(h.published().mode, null);
  assert.equal(h.session.cameraMode, null);
});

test('a stale mode query cannot submit an older device selection', async () => {
  const pending = Promise.withResolvers<unknown>();
  const h = harness(async (id) => id === 'A'
    ? await pending.promise
    : modes[id as keyof typeof modes]);
  const old = h.adapter.setCameraDevice('A');
  await h.adapter.setCameraDevice('B');
  pending.resolve(modes.A);
  await old;
  assert.deepEqual(h.commands, ['B']);
  assert.equal(h.published().deviceId, 'B');
});
