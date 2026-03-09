<script>
  import { onMount } from 'svelte';

  let { params } = $props();
  let container = $state(null);

  onMount(() => {
    // Delegate to the existing imperative code loaded via <script> in index.html
    if (window.PartTableEditorView) {
      window.PartTableEditorView.render(container, params);
    }
    return () => {
      // No cleanup exposed by PartTableEditorView
    };
  });
</script>

<div bind:this={container}></div>
