import assert from 'node:assert/strict';
import test from 'node:test';

import { remoteControlStatusEffect } from '../src/lib/remoteControlFeedback.ts';

test('active Windows operation feedback does not terminate remote control', () => {
  for (const status of [
    'accessibilityDenied',
    'requestFailed',
    'targetPaused',
    'targetUnavailable',
    'notForeground',
    'occluded',
    'integrityBlocked',
    'secureField',
    'unsupportedRoute',
    'staleShareInstance',
    'injectionTimeout'
  ] as const) {
    assert.equal(remoteControlStatusEffect(status), 'feedback', status);
  }
  assert.equal(remoteControlStatusEffect('active'), 'activate');
  assert.equal(remoteControlStatusEffect('stopped'), 'terminate');
  assert.equal(remoteControlStatusEffect('disabled'), 'terminate');
});
