<script lang="ts">
  import { ov } from '../lib/overlay.svelte';
</script>

{#if ov.notice}
  {#key ov.notice}
    <div class="toast {ov.notice.level}" role="status" aria-live="polite">
      <span class="mark"></span>
      <span class="msg">{ov.notice.message}</span>
      <button class="dismiss" onclick={() => ov.dismissNotice()}>Dismiss</button>
    </div>
  {/key}
{/if}

<style>
  .toast {
    position: absolute;
    left: 14px;
    right: 14px;
    bottom: 46px;
    z-index: 5;
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px 12px 10px 14px;
    border-radius: var(--r-md);
    font-size: 12.5px;
    line-height: 1.4;
    background: color-mix(in srgb, var(--panel-solid) 88%, transparent);
    border: 1px solid var(--hairline-strong);
    box-shadow: var(--shadow);
    animation: pop var(--dur) var(--ease) both;
  }
  .mark {
    flex: none;
    width: 8px;
    height: 8px;
    margin-top: 5px;
    border-radius: 50%;
    background: var(--cat-snippet);
  }
  .warn .mark {
    background: var(--cat-file);
  }
  .error .mark {
    background: var(--cat-sensitive);
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
    flex: none;
    padding: 2px 8px;
    border-radius: var(--r-sm);
    font-size: 11px;
    font-weight: 600;
    color: var(--ink-2);
  }
  .dismiss:hover {
    background: color-mix(in srgb, var(--ink) 10%, transparent);
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
