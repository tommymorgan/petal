import type { LinuxDesktopControlState, LinuxPictureShare } from '../ipc';

export const DESKTOP_CONTROL_DISCLOSURE = 'Keyboard and focus are desktop-wide, not confined to the selected monitor. Stop controlled share ends this monitor share.';

export function desktopControlReceiverFeedback(viewOnly: boolean, preparationRequired: boolean): string | null {
  if (viewOnly) return 'View only — application-window shares cannot receive remote input';
  if (preparationRequired) return 'View only — the sharer must prepare desktop control before you request it';
  return null;
}

export function desktopControlPresentation(share: LinuxPictureShare, state: LinuxDesktopControlState | null) {
  if (share.sourceKind === 'window') return { label: 'View only', disclosure: null, canPrepare: false, canStop: false };
  const current = state?.windowId === share.windowId;
  const occupied = state !== null && !['idle', 'failed'].includes(state.phase);
  let label = state === null ? 'Desktop control status unavailable' : 'Picture only';
  if (current && state) {
    const controller = state.controllerName || state.controllerId || 'the requesting participant';
    switch (state.phase) {
      case 'preparing': label = 'Preparing desktop control — no controller'; break;
      case 'ready': label = 'Desktop control ready — no controller'; break;
      case 'awaitingApproval': label = `Waiting for approval: ${controller}`; break;
      case 'active': label = `${controller} controls your desktop`; break;
      case 'stopping': label = 'Stopping controlled share'; break;
      case 'failed': label = state.error || 'Desktop preparation failed; picture only'; break;
    }
  }
  return {
    label,
    disclosure: current && occupied ? DESKTOP_CONTROL_DISCLOSURE : null,
    canPrepare: state !== null && !occupied,
    canStop: current && occupied && state?.phase !== 'stopping'
  };
}

/** Owns async snapshots, native events and disposal for one mounted meeting. */
export function createLinuxDesktopControl(io: {
  read: () => Promise<LinuxDesktopControlState>;
  prepare: (windowId: number) => Promise<LinuxDesktopControlState>;
  stop: (windowId: number) => Promise<void>;
  changed: (state: LinuxDesktopControlState | null) => void;
  failed: (message: string) => void;
}) {
  let retired = false;
  let revision = 0;
  async function refresh() {
    if (retired) return;
    const attempt = ++revision;
    try {
      const state = await io.read();
      if (!retired && revision === attempt) io.changed(state);
    } catch {
      if (!retired && revision === attempt) io.changed(null);
    }
  }
  return {
    refresh,
    receive(state: LinuxDesktopControlState) {
      if (retired) return;
      revision += 1;
      io.changed(state);
    },
    async prepare(windowId: number) {
      if (retired) return;
      const attempt = ++revision;
      try {
        const state = await io.prepare(windowId);
        if (!retired && revision === attempt) io.changed(state);
      } catch (error) {
        if (!retired) io.failed(String(error));
      }
    },
    async stop(windowId: number) {
      if (retired) return;
      revision += 1;
      try {
        await io.stop(windowId);
        await refresh();
      } catch (error) {
        if (!retired) io.failed(String(error));
      }
    },
    dispose() {
      if (retired) return;
      retired = true;
      revision += 1;
      io.changed(null);
    }
  };
}
