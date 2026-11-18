<script lang="ts" generics="T extends string">
  let { value, options, label, onchange }: { value: T | null; options: Array<{ id: T; label: string }>; label: string; onchange: (value: T) => void } = $props();
</script>

<div class="seg" role="radiogroup" aria-label={label}>
  {#each options as o (o.id)}
    <button role="radio" aria-checked={value === o.id} onclick={() => onchange(o.id)}>{o.label}</button>
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    border-radius: var(--r-sm);
    background: var(--field);
  }
  button {
    height: 22px;
    padding: 0 10px;
    border-radius: calc(var(--r-sm) - 2px);
    font-size: 12px;
    font-weight: 500;
    color: var(--ink-2);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  button:hover {
    color: var(--ink);
  }
  button[aria-checked='true'] {
    color: var(--ink);
    background: var(--panel-solid);
    box-shadow:
      0 0 0 0.5px var(--hairline-strong),
      0 1px 2px rgb(0 0 0 / 0.12);
  }
</style>
