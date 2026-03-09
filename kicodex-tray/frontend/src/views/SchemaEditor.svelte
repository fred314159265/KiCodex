<script>
  import { onMount } from 'svelte';
  import { invoke } from '../lib/tauri.js';
  import Breadcrumb from '../components/Breadcrumb.svelte';
  import { showError } from '../lib/toast.svelte.js';

  let { params } = $props();
  let libPath = $derived(params.lib || '');
  let schemaName = $derived(params.schema || '');
  let projectPath = $derived(params.project || '');

  let schema = $state(null);
  let loading = $state(true);
  let error = $state('');

  // Form state
  let inherits = $state('');
  let excludeFromBom = $state(false);
  let excludeFromBoard = $state(false);
  let excludeFromSim = $state(false);
  let fieldRows = $state([]);

  function navigate(view, p = {}) {
    const qs = Object.entries(p)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    window.location.hash = qs ? `${view}?${qs}` : view;
  }

  onMount(async () => {
    if (!libPath || !schemaName) { navigate('dashboard'); return; }
    try {
      schema = await invoke('get_schema', { libPath, schemaName });
      inherits = schema.inherits || '';
      excludeFromBom = schema.exclude_from_bom;
      excludeFromBoard = schema.exclude_from_board;
      excludeFromSim = schema.exclude_from_sim;
      fieldRows = schema.fields.map(f => ({ ...f }));
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  function addField() {
    fieldRows = [...fieldRows, { key: '', display_name: '', description: '', field_type: '', required: false, visible: false }];
  }

  function removeField(i) {
    fieldRows = fieldRows.filter((_, idx) => idx !== i);
  }

  async function save() {
    const fields = fieldRows
      .filter(r => r.key.trim())
      .map(r => ({
        key: r.key.trim(),
        display_name: r.display_name.trim(),
        field_type: r.field_type || null,
        required: r.required,
        visible: r.visible,
        description: r.description || null,
      }));
    try {
      await invoke('save_schema', {
        libPath,
        schemaName,
        schema: {
          inherits: inherits || null,
          exclude_from_bom: excludeFromBom,
          exclude_from_board: excludeFromBoard,
          exclude_from_sim: excludeFromSim,
          fields,
        },
      });
      navigate('project', { path: projectPath });
    } catch (e) {
      showError(e);
    }
  }

  let crumbs = $derived.by(() => {
    const c = [{ label: 'Dashboard', href: '#dashboard' }];
    if (projectPath) c.push({ label: projectPath.split(/[\/\\]/).pop(), href: `#project?path=${encodeURIComponent(projectPath)}` });
    c.push({ label: `Schema: ${schemaName}` });
    return c;
  });
</script>

{#if loading}
  <div class="flex items-center gap-2 p-8 text-base-content/60">
    <span class="loading loading-spinner"></span>Loading...
  </div>
{:else if error}
  <div class="alert alert-error"><span>{error}</span></div>
{:else if schema}
  <Breadcrumb {crumbs} />
  <h2 class="text-xl font-bold mb-4">Schema: {schemaName}</h2>

  <div class="card bg-base-100 shadow">
    <div class="card-body gap-4">
      <fieldset class="fieldset">
        <label class="label" for="inherits">Inherits</label>
        <input id="inherits" class="input input-bordered w-full max-w-sm" type="text" bind:value={inherits} placeholder="_base (or leave empty)" />
      </fieldset>

      <div>
        <p class="font-semibold text-sm mb-1">Default Exclude Flags</p>
        <p class="text-xs text-base-content/60 mb-2">(used when not overridden on individual components)</p>
        <div class="flex flex-wrap gap-4">
          <label class="flex items-center gap-2 text-sm cursor-pointer">
            <input type="checkbox" class="checkbox checkbox-sm" bind:checked={excludeFromBom} />
            Exclude from BOM
          </label>
          <label class="flex items-center gap-2 text-sm cursor-pointer">
            <input type="checkbox" class="checkbox checkbox-sm" bind:checked={excludeFromBoard} />
            Exclude from Board
          </label>
          <label class="flex items-center gap-2 text-sm cursor-pointer">
            <input type="checkbox" class="checkbox checkbox-sm" bind:checked={excludeFromSim} />
            Exclude from Sim
          </label>
        </div>
      </div>

      <!-- Field rows header -->
      <div class="grid grid-cols-[1fr_1fr_1fr_120px_40px_40px_32px] gap-2 text-xs font-semibold text-base-content/60 border-b border-base-200 pb-1">
        <span>Key</span>
        <span>Display Name</span>
        <span>Description</span>
        <span>Type</span>
        <span class="text-center">Req</span>
        <span class="text-center">Vis</span>
        <span></span>
      </div>

      {#each fieldRows as row, i}
        <div class="grid grid-cols-[1fr_1fr_1fr_120px_40px_40px_32px] gap-2 items-center">
          <input class="input input-bordered input-sm" type="text" bind:value={row.key} placeholder="field_key" />
          <input class="input input-bordered input-sm" type="text" bind:value={row.display_name} placeholder="Display Name" />
          <input class="input input-bordered input-sm" type="text" bind:value={row.description} placeholder="Help text" />
          <select class="select select-bordered select-sm" bind:value={row.field_type}>
            <option value="">(none)</option>
            <option value="kicad_symbol">kicad_symbol</option>
            <option value="kicad_footprint">kicad_footprint</option>
            <option value="url">url</option>
          </select>
          <div class="flex justify-center"><input type="checkbox" class="checkbox checkbox-sm" bind:checked={row.required} /></div>
          <div class="flex justify-center"><input type="checkbox" class="checkbox checkbox-sm" bind:checked={row.visible} /></div>
          <button class="btn btn-xs btn-ghost text-error" onclick={() => removeField(i)}>✕</button>
        </div>
      {/each}

      <div class="flex gap-2 mt-2">
        <button class="btn btn-sm" onclick={addField}>Add Field</button>
        <button class="btn btn-sm btn-primary" onclick={save}>Save Schema</button>
      </div>
    </div>
  </div>
{/if}
