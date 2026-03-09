<script>
  let { open = false, title = '', onclose, body, footer } = $props();
</script>

{#if open}
  <div class="modal modal-open">
    <div class="modal-box">
      <div class="flex items-center justify-between mb-4">
        <h3 class="font-bold text-lg">{title}</h3>
        <button class="btn btn-sm btn-circle btn-ghost" onclick={onclose}>✕</button>
      </div>
      {@render body()}
      {#if footer}
        <div class="modal-action">
          {@render footer()}
        </div>
      {/if}
    </div>
    <div class="modal-backdrop" onclick={onclose}></div>
  </div>
{/if}
