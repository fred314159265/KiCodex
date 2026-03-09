<script>
  import { toasts, dismiss } from '../lib/toast.svelte.js';
</script>

{#if toasts.length > 0}
  <div class="toast toast-top toast-end z-50 max-w-sm">
    {#each toasts as toast (toast.id)}
      <div class="alert {toast.type === 'error' ? 'alert-error' : 'alert-info'} flex items-start gap-2 shadow-lg">
        <span class="flex-1 text-sm">{toast.msg}</span>
        <button class="btn btn-xs btn-circle btn-ghost shrink-0" onclick={() => dismiss(toast.id)}>✕</button>
      </div>
    {/each}
  </div>
{/if}
