<script lang="ts">
  import Icon from './Icon.svelte';
  import { ov } from '../lib/overlay.svelte';
</script>

{#if ov.notice}
  {#key ov.notice}
    <div class="toast {ov.notice.level}" class:low={ov.view === 'settings'} role="status" aria-live="polite">
      <span class="mark" aria-hidden="true"></span>
      <span class="msg">{ov.notice.message}</span>
      <button class="dismiss" onclick={() => ov.dismissNotice()} aria-label="Dismiss"><Icon name="x" size={12} /></button>
    </div>
  {/key}
{/if}

<style>
  .toast {
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 44px;
    z-index: 7;
    display: flex;
    gap: 9px;
    align-items: flex-start;
    padding: 9px 8px 9px 12px;
    border-radius: var(--r-md);
    font-size: 12px;
    line-height: 1.4;
    background: color-mix(in srgb, var(--panel-solid) 94%, transparent);
    backdrop-filter: blur(24px) saturate(1.4);
    box-shadow:
      0 0 0 1px var(--hairline-strong),
      var(--shadow);
    animation: pop var(--dur) var(--ease) both;
  }
  .toast.low {
    bottom: 10px;
  }
  .mark {
    flex: none;
    width: 7px;
    height: 7px;
    margin-top: 5px;
    border-radius: 50%;
    background: #34c759;
  }
  .warn .mark {
    background: #ff9f0a;
  }
  .error .mark {
    background: var(--danger);
  }
  .msg {
    flex: 1;
    min-width: 0;
    white-space: pre-line;
    overflow-wrap: anywhere;
    max-height: 7.2em;
    overflow: auto;
  }
  .dismiss {
    display: grid;
    place-items: center;
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 6px;
    color: var(--ink-3);
  }
  .dismiss:hover {
    background: var(--hover);
    color: var(--ink);
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
</style>
