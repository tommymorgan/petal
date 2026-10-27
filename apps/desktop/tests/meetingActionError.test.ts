import assert from 'node:assert/strict';
import test from 'node:test';
import { meetingActionErrorMessage } from '../src/lib/data/meetingActionError.ts';

test('meeting action errors map network failures to actionable copy', () => {
  assert.equal(
    meetingActionErrorMessage(new Error('Failed to fetch token'), 'create'),
    'Could not create the meeting. Petal could not reach the meeting server. Check your connection and try again.'
  );
});

test('meeting action errors do not expose raw object or arbitrary backend detail', () => {
  assert.equal(
    meetingActionErrorMessage({ message: 'exploded' }, 'join'),
    'Could not join the meeting. Check your connection and try again.'
  );
  assert.equal(
    meetingActionErrorMessage(new Error('opaque internal details'), 'leave'),
    'Could not leave the meeting. Check your connection and try again.'
  );
});

test('meeting action errors include invite and route-specific recovery copy', () => {
  assert.equal(
    meetingActionErrorMessage('invalid access code', 'join'),
    'Could not join the meeting. Check the invite link or meeting code and try again.'
  );
  assert.equal(
    meetingActionErrorMessage('goto failed', 'open'),
    'Meeting was created, but Petal could not open it. Try selecting it from Your Rooms.'
  );
});

test('native missing hosted configuration names only the setting and recovery action', () => {
  const message = meetingActionErrorMessage({
    kind: 'config',
    message: 'no token backend is configured (PETAL_BACKEND_URL was not set at build time)'
  }, 'join');
  assert.match(message, /PETAL_BACKEND_URL/);
  assert.match(message, /configur|set/i);
  assert.match(message, /restart/i);
  assert.doesNotMatch(message, /LIVEKIT_API|mint tokens|docs\/SELF_HOSTING/);
});

test('native hosted access refusals direct users to check the code or ask the host', () => {
  for (const status of ['401 Unauthorized', '403 Forbidden']) {
    const message = meetingActionErrorMessage({
      kind: 'config',
      message: `backend token endpoint returned HTTP ${status}`
    }, 'join');
    assert.match(message, /code|invite/i);
    assert.match(message, /host|access/i);
    assert.doesNotMatch(message, /HTTP|backend token/);
  }
});
