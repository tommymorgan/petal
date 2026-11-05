import assert from 'node:assert/strict';
import test from 'node:test';
import { createLinuxDesktopControl, desktopControlPresentation } from '../src/lib/meeting/linuxDesktopControl.ts';
import type { LinuxDesktopControlState } from '../src/lib/ipc.ts';

const state = (phase: LinuxDesktopControlState['phase'], windowId = 7): LinuxDesktopControlState => ({ phase, windowId, controllerId: null, controllerName: null, error: null });
const deferred = <T>() => Promise.withResolvers<T>();

test('window shares remain view only and desktop preparation is explicit', () => {
  assert.equal(desktopControlPresentation({ windowId: 7, title: 'Editor', sourceKind: 'window' }, state('active')).canPrepare, false);
  assert.equal(desktopControlPresentation({ windowId: 7, title: 'Display', sourceKind: 'display' }, null).canPrepare, false);
  const display = desktopControlPresentation({ windowId: 7, title: 'Display', sourceKind: 'display' }, state('idle'));
  assert.equal(display.canPrepare, true);
});

test('prepared and occupied desktop sessions can stop but cannot prepare competing shares', () => {
  const share = { windowId: 7, title: 'Display', sourceKind: 'display' as const };
  for (const phase of ['preparing', 'ready', 'awaitingApproval', 'active'] as const) {
    const current = desktopControlPresentation(share, state(phase));
    assert.equal(current.canPrepare, false);
    assert.equal(current.canStop, true);
    const other = desktopControlPresentation({ ...share, windowId: 8 }, state(phase));
    assert.equal(other.canPrepare, false);
    assert.equal(other.canStop, false);
    const window = desktopControlPresentation({ ...share, sourceKind: 'window' }, state(phase));
    assert.equal(window.canPrepare, false);
    assert.equal(window.canStop, false);
  }
  assert.equal(desktopControlPresentation(share, state('stopping')).canStop, false);
  assert.equal(desktopControlPresentation(share, state('failed')).canPrepare, true);
});

test('state events win over older snapshots and command completions; disposal cannot resurrect state', async () => {
  const first = deferred<LinuxDesktopControlState>();
  const prepare = deferred<LinuxDesktopControlState>();
  const changes: Array<LinuxDesktopControlState | null> = [];
  const owner = createLinuxDesktopControl({ read: () => first.promise, prepare: () => prepare.promise, stop: async () => {}, changed: (s) => changes.push(s), failed: () => assert.fail('unexpected error') });
  const refresh = owner.refresh();
  owner.receive(state('ready'));
  first.resolve(state('idle'));
  await refresh;
  assert.equal(changes.at(-1)?.phase, 'ready');
  const pending = owner.prepare(7);
  owner.receive(state('active'));
  prepare.resolve(state('ready'));
  await pending;
  assert.equal(changes.at(-1)?.phase, 'active');
  owner.dispose();
  owner.receive(state('ready'));
  assert.equal(changes.at(-1), null);
});


test('late preparation and snapshot results after disposal cannot update the meeting', async () => {
  const snapshot = deferred<LinuxDesktopControlState>();
  const preparation = deferred<LinuxDesktopControlState>();
  const changes: Array<LinuxDesktopControlState | null> = [];
  const owner = createLinuxDesktopControl({
    read: () => snapshot.promise,
    prepare: () => preparation.promise,
    stop: async () => {},
    changed: (s) => changes.push(s),
    failed: () => assert.fail('disposed errors must not reach the meeting')
  });
  const read = owner.refresh();
  const prepare = owner.prepare(7);
  owner.dispose();
  snapshot.resolve(state('active'));
  preparation.resolve(state('ready'));
  await Promise.all([read, prepare]);
  owner.dispose();
  assert.deepEqual(changes, [null]);
});

test('preparation cancellation reports failure without replacing the last known native state', async () => {
  const changes: Array<LinuxDesktopControlState | null> = [];
  const failures: string[] = [];
  const owner = createLinuxDesktopControl({
    read: async () => state('idle'),
    prepare: async () => { throw new Error('Cancelled'); },
    stop: async () => {},
    changed: (s) => changes.push(s),
    failed: (message) => failures.push(message)
  });
  await owner.refresh();
  await owner.prepare(7);
  assert.deepEqual(changes.map((s) => s?.phase), ['idle']);
  assert.equal(failures.length, 1);
});
