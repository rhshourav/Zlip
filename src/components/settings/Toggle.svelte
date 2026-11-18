<script lang="ts">
  let { checked, label, onchange }: { checked: boolean; label: string; onchange: (value: boolean) => void } = $props();
</script>

<button class="sw" role="switch" aria-checked={checked} aria-label={label} onclick={() => onchange(!checked)}>
  <span class="knob"></span>
</button>

<style>
  .sw {
    position: relative;
    flex: none;
    width: 36px;
    height: 22px;
    border-radius: 999px;
    background: var(--knob-off);
    transition: background var(--dur) var(--ease);
  }
  .sw[aria-checked='true'] {
    background: var(--accent);
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    box-shadow:
      0 1px 2px rgb(0 0 0 / 0.25),
      0 0 0 0.5px rgb(0 0 0 / 0.08);
    transition: transform var(--dur) var(--ease);
  }
  .sw[aria-checked='true'] .knob {
    transform: translateX(14px);
  }
</style>
