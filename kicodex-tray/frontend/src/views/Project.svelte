<script>
  import { onMount } from 'svelte';
  import { invoke } from '../lib/tauri.js';
  import Breadcrumb from '../components/Breadcrumb.svelte';
  import Modal from '../components/Modal.svelte';
  import { ask } from '../lib/confirm.svelte.js';
  import { showError, showInfo } from '../lib/toast.svelte.js';
  import { confirmAndRegister } from '../lib/kicad-register.js';

  let { params } = $props();
  let projectPath = $derived(params.path || '');

  let libraries = $state([]);
  let registeredNames = $state(new Set());
  let loading = $state(true);
  let error = $state('');

  // "New Library" modal state
  let newLibModalOpen = $state(false);
  let newLibName = $state('');
  let newLibError = $state('');
  let newLibCreating = $state(false);
  let newLibCreated = $state(false);
  let newLibRegStatus = $state('');
  let newLibShowReg = $state(false);

  // "Add Part Table" modal state
  let addPartModalOpen = $state(false);
  let addPartLib = $state(null);
  let addPartName = $state('');
  let addPartError = $state('');

  // "Scan" modal state
  let scanModalOpen = $state(false);
  let scanLibs = $state([]);
  let scanChecked = $state({});
  let scanAdding = $state(false);
  let scanAdded = $state(false);
  let scanAddedCount = $state(0);
  let scanRegStatus = $state('');
  let scanShowReg = $state(false);
  let scanUnregisteredNames = $state([]);

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
      [libraries, registeredNames] = await Promise.all([
        invoke('get_project_libraries', { projectPath }),
        invoke('get_kicad_lib_table_names', { projectPath }).catch(() => []).then(r => new Set(r)),
      ]);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    if (!projectPath) { navigate('dashboard'); return; }
    load();
  });

  async function removeProject() {
    const yes = await ask('Remove this project from KiCodex?', { title: 'Remove Project' });
    if (!yes) return;
    try { await invoke('remove_project', { projectPath }); navigate('dashboard'); }
    catch (e) { showError(e); }
  }

  async function unlinkLibrary(lib) {
    const yes = await ask(
      `Remove library "${lib.name}" from this project? The library files will be kept on disk.`,
      { title: 'Unlink Library' },
    );
    if (!yes) return;
    const regged = await invoke('get_kicad_lib_table_names', { projectPath }).catch(() => []);
    let removeFromLibTable = false;
    if (regged.includes(lib.name)) {
      removeFromLibTable = await ask(
        `Also remove "${lib.name}" from KiCad's sym-lib-table?`,
        { title: 'Update sym-lib-table', confirmLabel: 'Yes, Remove', cancelLabel: 'No' },
      );
    }
    try {
      await invoke('remove_library', { projectPath, libraryPath: lib.path });
      if (removeFromLibTable) await invoke('unregister_from_kicad_lib_table', { projectPath, libraryName: lib.name });
      load();
    } catch (e) { showError(e); }
  }

  async function deleteLibrary(lib) {
    const yes = await ask(
      `Permanently delete library "${lib.name}"? This will remove it from the project AND delete all files on disk. This cannot be undone.`,
      { title: 'Delete Library' },
    );
    if (!yes) return;
    try { await invoke('delete_library', { projectPath, libraryPath: lib.path }); load(); }
    catch (e) { showError(e); }
  }

  async function deletePartTable(libPath, name) {
    const yes = await ask(
      `Delete part table "${name}"? This will remove its data file and template.`,
      { title: 'Delete Part Table' },
    );
    if (!yes) return;
    try { await invoke('delete_part_table', { libPath, partTableName: name }); load(); }
    catch (e) { showError(e); }
  }

  async function registerInKiCad(lib) {
    try {
      await confirmAndRegister(projectPath, async () => {
        await invoke('register_in_kicad_lib_table', { projectPath, libraryNames: [lib.name] });
        registeredNames = new Set([...registeredNames, lib.name]);
      });
    } catch (e) { showError(e); }
  }

  // --- New Library modal ---
  function openNewLibModal() {
    newLibName = ''; newLibError = ''; newLibCreating = false; newLibCreated = false;
    newLibRegStatus = ''; newLibShowReg = false;
    newLibModalOpen = true;
  }

  async function createLibrary() {
    const name = newLibName.trim();
    if (!name) { newLibError = 'Name is required'; return; }
    newLibCreating = true; newLibError = '';
    try {
      await invoke('create_library', { name, parentDir: projectPath });
      await invoke('add_project', { projectPath, libraries: [{ name, path: name, is_new: true }] });
      const regged = await invoke('get_kicad_lib_table_names', { projectPath }).catch(() => []);
      newLibCreated = true;
      newLibShowReg = !regged.includes(name);
    } catch (e) {
      newLibError = String(e);
      newLibCreating = false;
    }
  }

  async function registerNewLib() {
    try {
      const done = await confirmAndRegister(projectPath, async () => {
        await invoke('register_in_kicad_lib_table', { projectPath, libraryNames: [newLibName.trim()] });
      });
      if (done) {
        newLibRegStatus = 'Registered in sym-lib-table.';
        newLibShowReg = false;
      }
    } catch (e) {
      newLibRegStatus = 'Error: ' + e;
    }
  }

  function doneNewLib() {
    newLibModalOpen = false;
    load();
  }

  // --- Add Part Table modal ---
  function openAddPartModal(lib) {
    addPartLib = lib; addPartName = ''; addPartError = '';
    addPartModalOpen = true;
  }

  function confirmAddPart() {
    const name = addPartName.trim();
    if (!name) { addPartError = 'Name is required'; return; }
    addPartModalOpen = false;
    navigate('template-editor', { lib: addPartLib.path, template: name, project: projectPath, mode: 'create' });
  }

  // --- Scan modal ---
  async function doScan() {
    try {
      const results = await invoke('scan_for_libraries', { path: projectPath });
      const newLibs = results.filter(r => r.is_new);
      if (newLibs.length === 0) { showInfo('No new libraries found.'); return; }
      scanLibs = newLibs;
      scanChecked = Object.fromEntries(newLibs.map(l => [l.name, true]));
      scanAdded = false; scanAdding = false; scanRegStatus = ''; scanShowReg = false;
      scanModalOpen = true;
    } catch (e) { showError('Error scanning: ' + e); }
  }

  async function confirmScan() {
    const selected = scanLibs.filter(l => scanChecked[l.name]);
    if (selected.length === 0) { scanModalOpen = false; return; }
    scanAdding = true;
    try {
      await invoke('add_project', { projectPath, libraries: selected });
      const names = selected.map(l => l.name);
      const regged = new Set(await invoke('get_kicad_lib_table_names', { projectPath }).catch(() => []));
      scanUnregisteredNames = names.filter(n => !regged.has(n));
      scanAddedCount = selected.length;
      scanAdded = true;
      scanShowReg = scanUnregisteredNames.length > 0;
    } catch (e) {
      showError(e);
      scanAdding = false;
    }
  }

  async function registerScanned() {
    try {
      const done = await confirmAndRegister(projectPath, async () => {
        const count = await invoke('register_in_kicad_lib_table', { projectPath, libraryNames: scanUnregisteredNames });
        scanRegStatus = `Registered ${count} ${count === 1 ? 'library' : 'libraries'} in sym-lib-table.`;
        scanShowReg = false;
      });
      if (!done) return;
    } catch (e) {
      scanRegStatus = 'Error: ' + e;
    }
  }

  function doneScan() {
    scanModalOpen = false;
    load();
  }
</script>

{#if loading}
  <div class="flex items-center gap-2 p-8 text-base-content/60">
    <span class="loading loading-spinner"></span>Loading...
  </div>
{:else if error}
  <div class="alert alert-error"><span>{error}</span></div>
{:else}
  <Breadcrumb crumbs={[
    { label: 'Dashboard', href: '#dashboard' },
    { label: projectPath.split(/[\/\\]/).pop() }
  ]} />

  <div class="flex flex-wrap items-start justify-between gap-4 mb-6">
    <h1 class="text-2xl font-bold">{projectPath.split(/[\/\\]/).pop()}</h1>
    <div class="flex flex-wrap gap-2">
      <button class="btn btn-sm" onclick={() => invoke('open_in_explorer', { path: projectPath })}>Open Folder</button>
      <button class="btn btn-sm" onclick={doScan}>Scan for Libraries</button>
      <button class="btn btn-sm btn-primary" onclick={openNewLibModal}>New Library</button>
      <button class="btn btn-sm btn-error" onclick={removeProject}>Remove Project</button>
    </div>
  </div>

  {#if libraries.length === 0}
    <p class="text-center text-base-content/60 py-8">No libraries found for this project.</p>
  {:else}
    {#each libraries as lib}
      <div class="mb-8">
        <div class="flex flex-wrap items-start justify-between gap-4 mb-3">
          <div>
            <h2 class="text-lg font-bold">{lib.name}</h2>
            {#if lib.description}<p class="text-base-content/60 text-sm">{lib.description}</p>{/if}
          </div>
          <div class="flex flex-wrap gap-2">
            <button class="btn btn-sm" onclick={() => navigate('validate', { project: projectPath })}>Validate</button>
            <button class="btn btn-sm" onclick={() => openAddPartModal(lib)}>Add Part Table</button>
            {#if !registeredNames.has(lib.name)}
              <button class="btn btn-sm" onclick={() => registerInKiCad(lib)}>Register in KiCad</button>
            {/if}
            <button class="btn btn-sm btn-error" onclick={() => unlinkLibrary(lib)}>Unlink</button>
            <button class="btn btn-sm btn-error" onclick={() => deleteLibrary(lib)}>Delete Library</button>
          </div>
        </div>

        {#if lib.part_tables.length === 0}
          <p class="text-center text-base-content/60 py-4">No part tables in this library.</p>
        {:else}
          <div class="overflow-x-auto">
            <table class="table table-zebra">
              <thead>
                <tr>
                  <th>Part Table</th><th>Template</th><th>Components</th><th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {#each lib.part_tables as ct}
                  <tr>
                    <td>
                      <a class="link link-primary" href={`#part-table-editor?lib=${encodeURIComponent(lib.path)}&type=${encodeURIComponent(ct.name)}&project=${encodeURIComponent(projectPath)}`}>
                        {ct.name}
                      </a>
                    </td>
                    <td>{ct.template_name}</td>
                    <td>{ct.component_count}</td>
                    <td>
                      <div class="flex gap-2">
                        <button class="btn btn-xs" onclick={() => navigate('template-editor', { lib: lib.path, template: ct.template_name, project: projectPath })}>Template</button>
                        <button class="btn btn-xs btn-error" onclick={() => deletePartTable(lib.path, ct.name)}>Delete</button>
                      </div>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>
    {/each}
  {/if}
{/if}

<!-- New Library Modal -->
<Modal open={newLibModalOpen} title="New Library" onclose={() => { if (!newLibCreated) newLibModalOpen = false; }}>
  {#snippet body()}
    {#if newLibCreated}
      <p class="text-success mb-3">Library "{newLibName.trim()}" created.</p>
      {#if newLibShowReg}
        <button class="btn btn-sm mb-2" onclick={registerNewLib}>Register in KiCad sym-lib-table</button>
      {/if}
      {#if newLibRegStatus}<p class="text-sm mt-1">{newLibRegStatus}</p>{/if}
    {:else}
      <fieldset class="fieldset">
        <label class="label" for="new-lib-name">Library Name</label>
        <input
          id="new-lib-name"
          class="input input-bordered w-full"
          type="text"
          placeholder="e.g. my-components"
          bind:value={newLibName}
          onkeydown={(e) => e.key === 'Enter' && createLibrary()}
          autofocus
        />
        {#if newLibError}<p class="text-error text-sm mt-1">{newLibError}</p>{/if}
      </fieldset>
      <p class="text-sm text-base-content/60 mt-2">Will be created in: {projectPath}</p>
    {/if}
  {/snippet}
  {#snippet footer()}
    {#if newLibCreated}
      <button class="btn btn-primary" onclick={doneNewLib}>Done</button>
    {:else}
      <button class="btn" onclick={() => newLibModalOpen = false}>Cancel</button>
      <button class="btn btn-primary" onclick={createLibrary} disabled={newLibCreating}>
        {newLibCreating ? 'Creating...' : 'Create'}
      </button>
    {/if}
  {/snippet}
</Modal>

<!-- Add Part Table Modal -->
<Modal open={addPartModalOpen} title="New Part Table" onclose={() => addPartModalOpen = false}>
  {#snippet body()}
    <fieldset class="fieldset">
      <label class="label" for="part-name">Part Table Name</label>
      <input
        id="part-name"
        class="input input-bordered w-full"
        type="text"
        placeholder="e.g. capacitors"
        bind:value={addPartName}
        onkeydown={(e) => e.key === 'Enter' && confirmAddPart()}
        autofocus
      />
      {#if addPartError}<p class="text-error text-sm mt-1">{addPartError}</p>{/if}
    </fieldset>
  {/snippet}
  {#snippet footer()}
    <button class="btn" onclick={() => addPartModalOpen = false}>Cancel</button>
    <button class="btn btn-primary" onclick={confirmAddPart}>Next</button>
  {/snippet}
</Modal>

<!-- Scan Libraries Modal -->
<Modal open={scanModalOpen} title={`Found ${scanLibs.length} new ${scanLibs.length === 1 ? 'library' : 'libraries'}`} onclose={() => !scanAdded && (scanModalOpen = false)}>
  {#snippet body()}
    {#if scanAdded}
      <p class="text-success mb-3">Added {scanAddedCount} {scanAddedCount === 1 ? 'library' : 'libraries'}.</p>
      {#if scanShowReg}
        <button class="btn btn-sm mb-2" onclick={registerScanned}>Register in KiCad sym-lib-table</button>
      {/if}
      {#if scanRegStatus}<p class="text-sm mt-1">{scanRegStatus}</p>{/if}
    {:else}
      <div class="flex flex-col gap-2">
        {#each scanLibs as lib}
          <label class="flex items-center gap-3 text-sm">
            <input type="checkbox" class="checkbox checkbox-sm" bind:checked={scanChecked[lib.name]} />
            <span class="font-medium">{lib.name}</span>
            <span class="text-base-content/60">{lib.path}</span>
          </label>
        {/each}
      </div>
    {/if}
  {/snippet}
  {#snippet footer()}
    {#if scanAdded}
      <button class="btn btn-primary" onclick={doneScan}>Done</button>
    {:else}
      <button class="btn" onclick={() => scanModalOpen = false}>Cancel</button>
      <button class="btn btn-primary" onclick={confirmScan} disabled={scanAdding}>
        {scanAdding ? 'Adding...' : 'Add Selected'}
      </button>
    {/if}
  {/snippet}
</Modal>
