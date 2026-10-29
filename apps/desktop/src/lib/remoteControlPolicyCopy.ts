import type { RemoteControlPolicy } from '$lib/ipc';

/**
 * Settings copy for the remote-control consent policy.
 */
export const REMOTE_CONTROL_POLICY_TITLE = 'Remote control of my shared windows';
export const REMOTE_CONTROL_POLICY_DESCRIPTION =
  'A meeting control can turn it off for just one call.';

export const REMOTE_CONTROL_POLICY_OPTIONS: ReadonlyArray<{
  value: RemoteControlPolicy;
  label: string;
  hint: string;
}> = [
  { value: 'ask', label: 'Ask me each time', hint: 'You approve or deny every request.' },
  { value: 'auto', label: 'Allow automatically', hint: 'Anyone in the meeting can take control.' },
  { value: 'off', label: 'Off', hint: 'Requests are refused.' }
];
