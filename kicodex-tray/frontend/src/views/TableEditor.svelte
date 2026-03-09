<script>
  import { onMount } from 'svelte';
  import { invoke } from '../lib/tauri.js';
  import Breadcrumb from '../components/Breadcrumb.svelte';
  import { ask } from '../lib/confirm.svelte.js';
  import { showError } from '../lib/toast.svelte.js';

  let { params } = $props();
  let libPath = $derived(params.lib || '');
  let tableName = $derived(params.table || '');
  let projectPath = $derived(params.project || '');

  let data = $state(null);
  let loading = $state(true);
  let error = $state('');

  function navigate(view, p = {}) {
    const qs = Object.entries(p)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    window.location.hash = qs ? `${view}?${qs}` : view;
  }

  async function load() {
    loading = true;
    error = '';
    try {
      data = await invoke('get_table_rows', { libPath, tableName });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    if (!libPath || !tableName) { navigate('dashboard'); return; }
    load();
  });

  async function deleteRow(id) {
    if (!await ask(`Delete row ${id}?`, { title: 'Delete Row', confirmLabel: 'Delete' })) return;
    try {
      await invoke('delete_row', { libPath, tableName, id });
      load();
    } catch (e) {
      showError(e);
    }
  }

  let crumbs = $derived.by(() => {
    const c = [{ label: 'Dashboard', href: '#dashboard' }];
    if (projectPath) {
      c.push({ label: projectPath.split(/[\/\\]/).pop(), href: `#project?path=${encodeURIComponent(projectPath)}` });
    }
    if (data) c.push({ label: data.name });
    return c;
  });

  let visibleKeys = $derived.by(() => {
    if (!data) return [];
    const fields = data.schema.fields;
    return ['id', ...fields.map(f => f.key).filter(k => k !== 'id')];
  });
</script>

{#if loading}
  <div class="flex items-center gap-2 p-8 text-base-content/60">
    <span class="loading loading-spinner"></span>
    Loading...
  </div>
{:else if error}
  <div class="alert alert-error"><span>{error}</span></div>
{:else if data}
  <Breadcrumb {crumbs} />

  <div class="flex flex-wrap items-center justify-between gap-4 mb-6">
    <h2 class="text-xl font-bold">{data.name} <span class="text-base-content/60 font-normal text-base">({data.rows.length} rows)</span></h2>
    <div class="flex gap-2">
      <button class="btn btn-sm btn-primary" onclick={() => navigate('row-form', { lib: libPath, table: tableName, project: projectPath, mode: 'add' })}>
        Add Component
      </button>
      <button class="btn btn-sm" onclick={() => navigate('schema-editor', { lib: libPath, schema: data.schema_name, project: projectPath })}>
        Edit Schema
      </button>
    </div>
  </div>

  {#if data.rows.length === 0}
    <p class="text-center text-base-content/60 py-8">No components yet. Click Add Component to create one.</p>
  {:else}
    <div class="overflow-x-auto">
      <table class="table table-zebra table-sm">
        <thead>
          <tr>
            {#each visibleKeys as k}
              <th>{data.schema.fields.find(f => f.key === k)?.display_name ?? k}</th>
            {/each}
            <th class="text-center">BOM</th>
            <th class="text-center">Board</th>
            <th class="text-center">Sim</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each data.rows as row}
            <tr>
              {#each visibleKeys as k}
                <td title={row[k] || ''}>{row[k] || ''}</td>
              {/each}
              <td class="text-center">{row['exclude_from_bom'] === 'true' ? '✓' : ''}</td>
              <td class="text-center">{row['exclude_from_board'] === 'true' ? '✓' : ''}</td>
              <td class="text-center">{row['exclude_from_sim'] === 'true' ? '✓' : ''}</td>
              <td>
                <div class="flex gap-1">
                  <button class="btn btn-xs" onclick={() => navigate('row-form', { lib: libPath, table: tableName, project: projectPath, mode: 'edit', id: row.id })}>Edit</button>
                  <button class="btn btn-xs btn-error" onclick={() => deleteRow(row.id)}>Del</button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{/if}
