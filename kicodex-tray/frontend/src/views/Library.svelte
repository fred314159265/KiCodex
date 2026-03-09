<script>
  import { onMount } from 'svelte';
  import { invoke } from '../lib/tauri.js';
  import Breadcrumb from '../components/Breadcrumb.svelte';
  import Modal from '../components/Modal.svelte';
  import { ask } from '../lib/confirm.svelte.js';
  import { showError } from '../lib/toast.svelte.js';

  let { params } = $props();
  let libraryPath = $derived(params.path || '');

  let lib = $state(null);
  let loading = $state(true);
  let error = $state('');

  let addModalOpen = $state(false);
  let addName = $state('');
  let addError = $state('');

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
      lib = await invoke('get_library_detail', { libraryPath });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    if (!libraryPath) { navigate('dashboard'); return; }
    load();
  });

  async function removeLibrary() {
    const yes = await ask('Remove this library from KiCodex? (Files on disk will not be deleted)', { title: 'Remove Library' });
    if (!yes) return;
    try {
      await invoke('remove_standalone_library', { libraryPath });
      navigate('dashboard');
    } catch (e) {
      showError(e);
    }
  }

  async function deletePartTable(name) {
    const yes = await ask(`Delete part table "${name}"? This will remove its data file and template.`, { title: 'Delete Part Table' });
    if (!yes) return;
    try {
      await invoke('delete_part_table', { libPath: libraryPath, partTableName: name });
      load();
    } catch (e) {
      showError(e);
    }
  }

  function openAddModal() {
    addName = '';
    addError = '';
    addModalOpen = true;
  }

  function confirmAdd() {
    const name = addName.trim();
    if (!name) { addError = 'Name is required'; return; }
    addModalOpen = false;
    navigate('template-editor', { lib: libraryPath, template: name, mode: 'create' });
  }
</script>

{#if loading}
  <div class="flex items-center gap-2 p-8 text-base-content/60">
    <span class="loading loading-spinner"></span>
    Loading...
  </div>
{:else if error}
  <div class="alert alert-error"><span>{error}</span></div>
{:else if lib}
  <Breadcrumb crumbs={[{ label: 'Dashboard', href: '#dashboard' }, { label: lib.name }]} />

  <div class="flex flex-wrap items-start justify-between gap-4 mb-6">
    <div>
      <h1 class="text-2xl font-bold">{lib.name}</h1>
      {#if lib.description}<p class="text-base-content/60 text-sm">{lib.description}</p>{/if}
    </div>
    <div class="flex flex-wrap gap-2">
      <button class="btn btn-sm" onclick={() => invoke('open_in_explorer', { path: libraryPath })}>Open Folder</button>
      <button class="btn btn-sm" onclick={() => navigate('validate', { lib: libraryPath })}>Validate</button>
      <button class="btn btn-sm btn-primary" onclick={openAddModal}>Add Part Table</button>
      <button class="btn btn-sm btn-error" onclick={removeLibrary}>Remove Library</button>
    </div>
  </div>

  {#if lib.part_tables.length === 0}
    <p class="text-center text-base-content/60 py-8">No part tables in this library. Click "Add Part Table" to create one.</p>
  {:else}
    <div class="overflow-x-auto">
      <table class="table table-zebra">
        <thead>
          <tr>
            <th>Part Table</th>
            <th>Template</th>
            <th>Components</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each lib.part_tables as ct}
            <tr>
              <td>
                <a class="link link-primary" href={`#part-table-editor?lib=${encodeURIComponent(lib.path)}&type=${encodeURIComponent(ct.name)}`}>
                  {ct.name}
                </a>
              </td>
              <td>{ct.template_name}</td>
              <td>{ct.component_count}</td>
              <td>
                <div class="flex gap-2">
                  <button class="btn btn-xs" onclick={() => navigate('template-editor', { lib: lib.path, template: ct.template_name })}>Template</button>
                  <button class="btn btn-xs btn-error" onclick={() => deletePartTable(ct.name)}>Delete</button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{/if}

<Modal open={addModalOpen} title="New Part Table" onclose={() => addModalOpen = false}>
  {#snippet body()}
    <fieldset class="fieldset">
      <label class="label" for="add-name">Part Table Name</label>
      <input
        id="add-name"
        class="input input-bordered w-full"
        type="text"
        placeholder="e.g. capacitors"
        bind:value={addName}
        onkeydown={(e) => e.key === 'Enter' && confirmAdd()}
        autofocus
      />
      {#if addError}<p class="text-error text-sm mt-1">{addError}</p>{/if}
    </fieldset>
  {/snippet}
  {#snippet footer()}
    <button class="btn" onclick={() => addModalOpen = false}>Cancel</button>
    <button class="btn btn-primary" onclick={confirmAdd}>Next</button>
  {/snippet}
</Modal>
