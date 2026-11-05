<script lang="ts">
  import type { LinuxDesktopControlState, LinuxPictureShare } from '$lib/ipc';
  import { desktopControlPresentation, DESKTOP_CONTROL_DISCLOSURE } from '$lib/meeting/linuxDesktopControl';

  let { shares, state, onPrepare, onStop }: {
    shares: LinuxPictureShare[];
    state: LinuxDesktopControlState | null;
    onPrepare?: (windowId: number) => void;
    onStop?: (windowId: number) => void;
  } = $props();
</script>

{#each shares as share (share.windowId)}
  {@const control = desktopControlPresentation(share, state)}
  <div class="share-control" aria-live="polite">
    <span>{share.title}: {control.label}</span>
    {#if control.disclosure}<small>{control.disclosure}</small>{/if}
    {#if control.canPrepare && onPrepare}
      <button type="button" title={DESKTOP_CONTROL_DISCLOSURE} onclick={() => onPrepare?.(share.windowId)}>Prepare desktop control</button>
    {/if}
    {#if control.canStop && onStop}
      <button type="button" onclick={() => onStop?.(share.windowId)}>Stop controlled share</button>
    {/if}
  </div>
{/each}

<style>
  .share-control { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 8px 12px; color: var(--text-primary); font: 400 12px var(--font-ui); }
  small { flex-basis: 100%; color: var(--text-secondary); }
  button { color: var(--text-primary); background: var(--surface-raised); border: 1px solid var(--hairline-strong); border-radius: var(--radius-card); padding: 6px 10px; cursor: pointer; }
</style>
