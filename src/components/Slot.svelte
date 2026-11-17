<script lang="ts">
  /** An empty icon position; see src/icons.css for how to fill it. */
  let { name, size = 16 }: { name: string; size?: number } = $props();
</script>

<span class="slot" data-slot={name} style:--size="{size}px" aria-hidden="true"></span>
