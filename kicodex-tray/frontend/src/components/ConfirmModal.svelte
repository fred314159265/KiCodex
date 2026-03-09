<script>
  import { state, answer } from '../lib/confirm.svelte.js';
</script>

{#if state.open}
  <div class="modal modal-open">
    <div class="modal-box">
      <h3 class="font-bold text-lg mb-3">{state.title}</h3>
      <p class="text-sm">{state.message}</p>
      <div class="modal-action">
        <button class="btn" onclick={() => answer(false)}>{state.cancelLabel}</button>
        <button class="btn btn-error" onclick={() => answer(true)}>{state.confirmLabel}</button>
      </div>
    </div>
    <div class="modal-backdrop" onclick={() => answer(false)}></div>
  </div>
{/if}
