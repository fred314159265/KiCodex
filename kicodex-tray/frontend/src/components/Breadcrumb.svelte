<script>
  /** @type {{ crumbs: Array<{label: string, href?: string}> }} */
  let { crumbs } = $props();
</script>

<div class="breadcrumbs text-sm mb-4 sticky top-0 z-10 bg-base-100 -mx-4 px-4 -mt-4 pt-4 pb-1">
  <ul>
    {#each crumbs as crumb}
      <li>
        {#if crumb.href}
          <a href={crumb.href}>{crumb.label}</a>
        {:else}
          <span>{crumb.label}</span>
        {/if}
      </li>
    {/each}
  </ul>
</div>
