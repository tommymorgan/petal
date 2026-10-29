import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setupRemoteControlUi } from '../src/remoteControlUi.ts';
import type { HarnessContext } from '../src/context.ts';
import type { RemoteControlStatusMessage } from '../src/trackNames.ts';

// Exercise the real controller packet consumer, including grant/UI mutation.
test('cancel A, request B on the same share: delayed A replies cannot activate or retire B', () => {
  const previousDocument = globalThis.document;
  Object.assign(globalThis, { document: { querySelectorAll: () => [], getElementById: () => null } });
  try {
    const ctx = {
      state: { room: { localParticipant: { identity: 'controller' } }, activeRemoteControl: null },
      ui: { logEvent: () => {}, showToast: () => {} }, cb: {}
    } as unknown as HarnessContext;
    const ui = setupRemoteControlUi(ctx);
    const pending = (requestId: string) => ({ tileId: 'tile', targetUserId: 'host', windowId: 7,
      pointerId: null, grantToken: null, requestId });
    const packet = (requestId: string | undefined, status: RemoteControlStatusMessage['status'], token = 'grant-a') => ({
      v: 1, kind: 'status', targetUserId: 'controller', controllerId: 'host', windowId: 7, seq: 1,
      requestId, status, message: status, grantToken: token
    } satisfies RemoteControlStatusMessage);
    const receive = (message: RemoteControlStatusMessage) => ui.handleRemoteControlPayload(
      new TextEncoder().encode(JSON.stringify(message)), 'host');
    ctx.state.activeRemoteControl = pending('A');
    const oldApproval = packet('A', 'active');
    ctx.state.activeRemoteControl = null; // explicit cancellation retires A
    receive(oldApproval);
    assert.equal(ctx.state.activeRemoteControl, null, 'unsolicited active cannot restore retired authority');
    const newer = pending('B');
    ctx.state.activeRemoteControl = newer;
    for (const status of ['active', 'awaitingConsent', 'denied', 'requestFailed', 'stopped', 'disabled'] as const) {
      receive(packet('A', status));
      assert.equal(ctx.state.activeRemoteControl, newer);
      assert.equal(newer.grantToken, null);
    }
    receive(packet(undefined, 'active'));
    assert.equal(newer.grantToken, null, 'mixed-version replies fail closed');
    receive(packet('B', 'active', 'grant-b'));
    assert.equal(ctx.state.activeRemoteControl?.grantToken, 'grant-b');
    ctx.state.activeRemoteControl!.nextInputSeq = 4;
    for (const status of ['active', 'denied', 'stopped', 'disabled'] as const) receive(packet('A', status));
    assert.equal(ctx.state.activeRemoteControl?.grantToken, 'grant-b');
    receive(packet('B', 'active', 'grant-b'));
    assert.equal(ctx.state.activeRemoteControl?.nextInputSeq, 4, 'duplicate active must not rewind input sequence');
    receive(packet('B', 'stopped', 'grant-b'));
    assert.equal(ctx.state.activeRemoteControl, null, 'current host revoke still terminates authority');
  } finally {
    Object.assign(globalThis, { document: previousDocument });
  }
});
