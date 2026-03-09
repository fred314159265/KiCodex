<script>
  import { onMount } from 'svelte';
  import { invoke, listen } from '../lib/tauri.js';
  import Modal from '../components/Modal.svelte';
  import { ask } from '../lib/confirm.svelte.js';
  import { showError } from '../lib/toast.svelte.js';

  let { params } = $props();

  let projects = $state([]);
  let discovered = $state([]);
  let loading = $state(true);

  // New library modal
  let newLibModalOpen = $state(false);
  let newLibName = $state('');
  let newLibDir = $state('');
  let newLibError = $state('');
  let newLibCreating = $state(false);

  function navigate(view, p = {}) {
    const qs = Object.entries(p)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    window.location.hash = qs ? `${view}?${qs}` : view;
  }

  async function load() {
    loading = true;
    try {
      [projects, discovered] = await Promise.all([
        invoke('get_projects'),
        invoke('get_discovered_projects'),
      ]);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    load();
    const unlisten = listen('projects-changed', load);
    return () => unlisten.then(fn => fn());
  });

  let standaloneEntries = $derived(projects.filter(p => !p.project_path));
  let projectEntries = $derived(projects.filter(p => p.project_path));

  let groupedProjects = $derived.by(() => {
    const grouped = {};
    for (const p of projectEntries) {
      if (!grouped[p.project_path]) grouped[p.project_path] = { libraries: [], active: false };
      grouped[p.project_path].libraries.push(p);
      if (p.active) grouped[p.project_path].active = true;
    }
    return grouped;
  });

  async function promptAddProject() {
    try {
      const selected = await window.__TAURI__.dialog.open({
        directory: true,
        title: 'Select KiCad project directory',
      });
      if (selected) navigate('add-project', { path: selected });
    } catch (e) { console.error(e); }
  }

  async function promptOpenLibrary() {
    try {
      const selected = await window.__TAURI__.dialog.open({
        directory: true,
        title: 'Select library directory (containing library.yaml)',
      });
      if (selected) {
        try {
          await invoke('register_standalone_library', { libraryPath: selected });
          load();
        } catch (e) { showError(e); }
      }
    } catch (e) { console.error(e); }
  }

  function openNewLibModal() {
    newLibName = '';
    newLibDir = '';
    newLibError = '';
    newLibCreating = false;
    newLibModalOpen = true;
  }

  async function pickNewLibDir() {
    const selected = await window.__TAURI__.dialog.open({
      directory: true,
      title: 'Select parent directory for library',
    });
    if (selected) newLibDir = selected;
  }

  async function createLibrary() {
    const name = newLibName.trim();
    if (!name) { newLibError = 'Please enter a library name'; return; }
    if (!newLibDir) { newLibError = 'Please select a parent directory'; return; }
    newLibCreating = true;
    newLibError = '';
    try {
      await invoke('create_library', { name, parentDir: newLibDir });
      newLibModalOpen = false;
      load();
    } catch (e) {
      newLibError = String(e);
      newLibCreating = false;
    }
  }

  async function removeStandaloneLib(libraryPath) {
    const yes = await ask('Remove this library from KiCodex? (Files on disk will not be deleted)', { title: 'Remove Library' });
    if (!yes) return;
    try {
      await invoke('remove_standalone_library', { libraryPath });
      load();
    } catch (e) { showError(e); }
  }

  async function removeProject(projectPath) {
    const yes = await ask('Remove this project from KiCodex?', { title: 'Remove Project' });
    if (!yes) return;
    try {
      await invoke('remove_project', { projectPath });
      load();
    } catch (e) { showError(e); }
  }
</script>

{#if loading}
  <div class="flex items-center gap-2 p-8 text-base-content/60">
    <span class="loading loading-spinner"></span>Loading...
  </div>
{:else}
  <div class="flex flex-wrap items-center justify-between gap-4 mb-6">
    <h1 class="text-2xl font-bold">Dashboard</h1>
    <div class="flex flex-wrap gap-2">
      <button class="btn btn-sm" onclick={promptAddProject}>Add Project</button>
      <button class="btn btn-sm" onclick={promptOpenLibrary}>Open Library</button>
      <button class="btn btn-sm btn-primary" onclick={openNewLibModal}>New Library</button>
    </div>
  </div>

  {#if standaloneEntries.length === 0 && Object.keys(groupedProjects).length === 0 && discovered.length === 0}
    <p class="text-center text-base-content/60 py-12">
      No projects or libraries found. Open a project in KiCad, use "Add Project", or "New Library" to get started.
    </p>
  {/if}

  <!-- Your Libraries -->
  {#if standaloneEntries.length > 0}
    <h2 class="text-lg font-semibold mb-3">Your Libraries</h2>
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 mb-8">
      {#each standaloneEntries as lib}
        <div class="card bg-base-100 shadow cursor-pointer hover:shadow-md transition-shadow" role="button" tabindex="0"
          onclick={() => navigate('library', { path: lib.library_path })}
          onkeydown={(e) => e.key === 'Enter' && navigate('library', { path: lib.library_path })}
        >
          <div class="card-body py-4 px-5">
            <div class="flex items-start justify-between gap-2">
              <h3 class="card-title text-base">{lib.name}</h3>
              <button class="btn btn-xs btn-ghost text-base-content/40 hover:text-error shrink-0"
                onclick={(e) => { e.stopPropagation(); removeStandaloneLib(lib.library_path); }}
                title="Remove library"
              >✕</button>
            </div>
            <p class="text-xs text-base-content/60 break-all">{lib.library_path}</p>
            <p class="text-sm text-base-content/60">{lib.part_table_count} part table{lib.part_table_count !== 1 ? 's' : ''}</p>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Your Projects -->
  {#if Object.keys(groupedProjects).length > 0}
    <h2 class="text-lg font-semibold mb-3">Your Projects</h2>
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 mb-8">
      {#each Object.entries(groupedProjects) as [projPath, group]}
        {@const projName = projPath.split(/[\/\\]/).pop() || projPath}
        {@const totalTypes = group.libraries.reduce((s, l) => s + l.part_table_count, 0)}
        <div class="card bg-base-100 shadow cursor-pointer hover:shadow-md transition-shadow" role="button" tabindex="0"
          onclick={() => navigate('project', { path: projPath })}
          onkeydown={(e) => e.key === 'Enter' && navigate('project', { path: projPath })}
        >
          <div class="card-body py-4 px-5">
            <div class="flex items-start justify-between gap-2">
              <h3 class="card-title text-base flex items-center gap-2">
                <span class="badge badge-xs {group.active ? 'badge-success' : 'badge-ghost'}"></span>
                {projName}
              </h3>
              <button class="btn btn-xs btn-ghost text-base-content/40 hover:text-error shrink-0"
                onclick={(e) => { e.stopPropagation(); removeProject(projPath); }}
                title="Remove project"
              >✕</button>
            </div>
            <p class="text-xs text-base-content/60 break-all">{projPath}</p>
            <p class="text-sm text-base-content/60">{group.libraries.length} {group.libraries.length !== 1 ? 'libraries' : 'library'}, {totalTypes} part table{totalTypes !== 1 ? 's' : ''}</p>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Open in KiCad (discovered) -->
  {#if discovered.length > 0}
    <h2 class="text-lg font-semibold mb-3">Open in KiCad</h2>
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 mb-8">
      {#each discovered as d}
        <div class="card bg-base-100 shadow">
          <div class="card-body py-4 px-5">
            <div class="flex items-center gap-2">
              <span class="badge badge-xs badge-success"></span>
              <h3 class="card-title text-base">{d.name}</h3>
            </div>
            <p class="text-xs text-base-content/60 break-all">{d.project_path}</p>
            <div class="card-actions mt-2">
              <button class="btn btn-sm btn-primary" onclick={(e) => { e.stopPropagation(); navigate('add-project', { path: d.project_path }); }}>
                Set up KiCodex
              </button>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
{/if}

<!-- New Library Modal -->
<Modal open={newLibModalOpen} title="New Library" onclose={() => newLibModalOpen = false}>
  {#snippet body()}
    <div class="flex flex-col gap-4">
      <fieldset class="fieldset">
        <label class="label" for="lib-name">Library Name</label>
        <input id="lib-name" class="input input-bordered w-full" type="text" placeholder="Library name" bind:value={newLibName} />
      </fieldset>
      <fieldset class="fieldset">
        <label class="label">Parent Directory</label>
        <div class="join w-full">
          <span class="join-item flex items-center px-3 border border-base-300 bg-base-200 text-sm text-base-content/60 max-w-[200px] truncate">
            {newLibDir || 'No directory selected'}
          </span>
          <button class="btn btn-sm join-item" onclick={pickNewLibDir}>Choose...</button>
        </div>
      </fieldset>
      {#if newLibError}<p class="text-error text-sm">{newLibError}</p>{/if}
    </div>
  {/snippet}
  {#snippet footer()}
    <button class="btn" onclick={() => newLibModalOpen = false}>Cancel</button>
    <button class="btn btn-primary" onclick={createLibrary} disabled={newLibCreating}>
      {newLibCreating ? 'Creating...' : 'Create'}
    </button>
  {/snippet}
</Modal>
