<script>
  import { onMount } from 'svelte';
  import { invoke } from '../lib/tauri.js';
  import Breadcrumb from '../components/Breadcrumb.svelte';
  import { confirmAndRegister } from '../lib/kicad-register.js';

  let { params } = $props();
  let projectPath = $derived(params.path || '');

  /** @type {'loading'|'already'|'select'|'none'|'create'|'existing'|'git'|'success'|'error'} */
  let step = $state('loading');
  let scanResult = $state(null);
  let addResult = $state(null);
  let stepError = $state('');

  // Select libraries step
  let libCheckboxes = $state({});

  // Create library step
  let createName = $state('');
  let createParentDir = $state('');
  let createError = $state('');
  let createBusy = $state(false);

  // Add existing step
  let existingDir = $state('');
  let useRelative = $state(true);
  let existingError = $state('');
  let existingBusy = $state(false);

  // Git step
  let gitUrl = $state('');
  let gitName = $state('');
  let gitTargetDir = $state('');
  let gitError = $state('');
  let gitBusy = $state(false);

  // Success step
  let regStatus = $state('');
  let showReg = $state(false);
  let unregisteredNames = $state([]);

  function navigate(view, p = {}) {
    const qs = Object.entries(p)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    window.location.hash = qs ? `${view}?${qs}` : view;
  }

  onMount(async () => {
    if (!projectPath) { step = 'error'; stepError = 'No project path specified.'; return; }
    await scan();
  });

  async function scan() {
    step = 'loading';
    stepError = '';
    try {
      scanResult = await invoke('scan_project', { projectPath });

      if (scanResult.has_config && scanResult.already_registered) {
        step = 'already';
        return;
      }

      if (scanResult.has_config && !scanResult.already_registered) {
        // Auto-register
        const result = await invoke('add_project', {
          projectPath,
          libraries: scanResult.libraries.map(l => ({ ...l, is_new: true })),
        });
        await showSuccess(result);
        return;
      }

      if (scanResult.libraries.length > 0) {
        libCheckboxes = Object.fromEntries(scanResult.libraries.filter(l => l.is_new).map(l => [l.name, true]));
        step = 'select';
        return;
      }

      step = 'none';
    } catch (e) {
      step = 'error';
      stepError = String(e);
    }
  }

  async function addSelected() {
    const selected = scanResult.libraries.map(l => ({
      name: l.name,
      path: l.path,
      is_new: l.is_new && (libCheckboxes[l.name] ?? false),
    }));
    if (!selected.some(l => l.is_new)) return;
    try {
      const result = await invoke('add_project', { projectPath, libraries: selected });
      await showSuccess(result);
    } catch (e) {
      stepError = String(e);
    }
  }

  async function showSuccess(result) {
    addResult = result;
    const allNames = result.httplib_paths.map(p => p.split(/[\/\\]/).pop().replace('.kicad_httplib', ''));
    const regged = new Set(await invoke('get_kicad_lib_table_names', { projectPath }).catch(() => []));
    unregisteredNames = allNames.filter(n => !regged.has(n));
    showReg = unregisteredNames.length > 0;
    regStatus = '';
    step = 'success';
  }

  async function registerInKiCad() {
    try {
      const done = await confirmAndRegister(projectPath, async () => {
        const count = await invoke('register_in_kicad_lib_table', { projectPath, libraryNames: unregisteredNames });
        regStatus = `Added ${count} entr${count === 1 ? 'y' : 'ies'} to sym-lib-table.`;
        showReg = false;
      });
      if (!done) return;
    } catch (e) {
      regStatus = 'Error: ' + String(e);
    }
  }

  async function doCreate() {
    if (!createName.trim()) { createError = 'Please enter a library name'; return; }
    if (!createParentDir) { createError = 'Please select a parent directory'; return; }
    createBusy = true; createError = '';
    try {
      const libPath = await invoke('create_library', { name: createName.trim(), parentDir: createParentDir });
      const np = projectPath.replace(/\\/g, '/').replace(/\/$/, '');
      const nl = libPath.replace(/\\/g, '/');
      const relPath = nl.startsWith(np + '/') ? nl.slice(np.length + 1) : nl;
      const result = await invoke('add_project', { projectPath, libraries: [{ name: createName.trim(), path: relPath, is_new: true }] });
      await showSuccess(result);
    } catch (e) {
      createError = String(e);
      createBusy = false;
    }
  }

  async function pickCreateDir() {
    const s = await window.__TAURI__.dialog.open({ directory: true, defaultPath: projectPath, title: 'Select parent directory' });
    if (s) createParentDir = s;
  }

  async function doExisting() {
    if (!existingDir) { existingError = 'Please select a library directory'; return; }
    existingBusy = true; existingError = '';
    try {
      let libPath = existingDir;
      if (useRelative) {
        const rel = existingDir.replace(projectPath.replace(/\\/g, '/'), '').replace(/^[\/\\]/, '');
        if (rel !== existingDir) libPath = rel;
      }
      const name = existingDir.split(/[\/\\]/).pop() || 'library';
      const result = await invoke('add_project', { projectPath, libraries: [{ name, path: libPath, is_new: true }] });
      await showSuccess(result);
    } catch (e) {
      existingError = String(e);
      existingBusy = false;
    }
  }

  async function pickExistingDir() {
    const s = await window.__TAURI__.dialog.open({ directory: true, title: 'Select library directory (contains library.yaml)' });
    if (s) existingDir = s;
  }

  async function doGit() {
    if (!gitUrl.trim()) { gitError = 'Please enter a Git URL'; return; }
    if (!gitName.trim()) { gitError = 'Please enter a library name'; return; }
    if (!gitTargetDir) { gitError = 'Please select a target directory'; return; }
    gitBusy = true; gitError = '';
    try {
      const libPath = await invoke('add_git_library', { projectPath, gitUrl: gitUrl.trim(), name: gitName.trim(), targetDir: gitTargetDir });
      const np = projectPath.replace(/\\/g, '/').replace(/\/$/, '');
      const nl = libPath.replace(/\\/g, '/');
      const relPath = nl.startsWith(np + '/') ? nl.slice(np.length + 1) : nl;
      const result = await invoke('add_project', { projectPath, libraries: [{ name: gitName.trim(), path: relPath, is_new: true }] });
      await showSuccess(result);
    } catch (e) {
      gitError = String(e);
      gitBusy = false;
    }
  }

  async function pickGitDir() {
    const s = await window.__TAURI__.dialog.open({ directory: true, defaultPath: projectPath, title: 'Select directory to clone into' });
    if (s) gitTargetDir = s;
  }

  let projName = $derived(projectPath.split(/[\/\\]/).pop() || projectPath);
</script>

<Breadcrumb crumbs={[{ label: 'Dashboard', href: '#dashboard' }, { label: 'Set up Project' }]} />
<h1 class="text-2xl font-bold mb-1">{projName}</h1>
<p class="text-base-content/60 text-sm mb-6">{projectPath}</p>

{#if step === 'loading'}
  <div class="flex items-center gap-2 text-base-content/60">
    <span class="loading loading-spinner"></span>Scanning project for libraries...
  </div>

{:else if step === 'error'}
  <div class="card bg-base-100 shadow">
    <div class="card-body">
      <p class="text-error"><strong>Error:</strong> {stepError}</p>
    </div>
  </div>

{:else if step === 'already'}
  <div class="card bg-base-100 shadow">
    <div class="card-body gap-4">
      <p>This project is already registered in KiCodex.</p>
      <button class="btn btn-primary w-fit" onclick={() => navigate('project', { path: projectPath })}>Go to Project</button>
    </div>
  </div>

{:else if step === 'select'}
  {@const newLibs = scanResult.libraries.filter(l => l.is_new)}
  {@const existingLibs = scanResult.libraries.filter(l => !l.is_new)}
  <div class="flex flex-wrap gap-3 mb-4">
    <span>Found {scanResult.libraries.length} {scanResult.libraries.length !== 1 ? 'libraries' : 'library'}</span>
    {#if newLibs.length > 0}<span class="badge badge-success">{newLibs.length} new</span>{/if}
    {#if existingLibs.length > 0}<span class="text-base-content/60 text-sm">{existingLibs.length} already registered</span>{/if}
  </div>

  <div class="card bg-base-100 shadow mb-4">
    <div class="card-body gap-2">
      {#each scanResult.libraries as lib}
        <div class="flex items-center gap-3">
          {#if lib.is_new}
            <input type="checkbox" class="checkbox checkbox-sm" bind:checked={libCheckboxes[lib.name]} />
          {:else}
            <span class="w-4"></span>
          {/if}
          <span class="font-medium">{lib.name}</span>
          <span class="text-base-content/60 text-sm">({lib.path})</span>
          <span class="badge {lib.is_new ? 'badge-success' : 'badge-ghost'} badge-sm">{lib.is_new ? 'NEW' : 'REGISTERED'}</span>
        </div>
      {/each}
    </div>
  </div>
  {#if stepError}<p class="text-error text-sm mb-2">{stepError}</p>{/if}
  {#if newLibs.length > 0}
    <button class="btn btn-primary" onclick={addSelected}>Add Selected</button>
  {/if}

{:else if step === 'none'}
  <p class="text-center text-base-content/60 py-6 mb-4">No libraries found in this project directory.</p>
  <div class="flex flex-col gap-4 max-w-md">
    <div class="card bg-base-100 shadow cursor-pointer hover:shadow-md" role="button" tabindex="0"
      onclick={() => { step = 'create'; createName = ''; createParentDir = ''; createError = ''; createBusy = false; }}
      onkeydown={(e) => e.key === 'Enter' && (step = 'create')}
    >
      <div class="card-body py-4">
        <h3 class="card-title text-base">Create new library</h3>
        <p class="text-sm text-base-content/60">Create a new library structure in this project</p>
      </div>
    </div>
    <div class="card bg-base-100 shadow cursor-pointer hover:shadow-md" role="button" tabindex="0"
      onclick={() => { step = 'existing'; existingDir = ''; existingError = ''; existingBusy = false; }}
      onkeydown={(e) => e.key === 'Enter' && (step = 'existing')}
    >
      <div class="card-body py-4">
        <h3 class="card-title text-base">Add existing library</h3>
        <p class="text-sm text-base-content/60">Browse for a library directory with library.yaml</p>
      </div>
    </div>
    <div class="card bg-base-100 shadow cursor-pointer hover:shadow-md" role="button" tabindex="0"
      onclick={() => { step = 'git'; gitUrl = ''; gitName = ''; gitTargetDir = ''; gitError = ''; gitBusy = false; }}
      onkeydown={(e) => e.key === 'Enter' && (step = 'git')}
    >
      <div class="card-body py-4">
        <h3 class="card-title text-base">Add from Git</h3>
        <p class="text-sm text-base-content/60">Clone a library from a Git repository (added as submodule if project is a git repo)</p>
      </div>
    </div>
  </div>

{:else if step === 'create'}
  <div class="card bg-base-100 shadow max-w-lg">
    <div class="card-body gap-4">
      <h3 class="font-bold text-lg">Create New Library</h3>
      <p class="text-sm text-base-content/60">Choose where to create the library directory. A new folder with the library name will be created inside it.</p>
      <fieldset class="fieldset">
        <label class="label">Library Name</label>
        <input class="input input-bordered w-full" type="text" placeholder="e.g. my-components" bind:value={createName} />
      </fieldset>
      <fieldset class="fieldset">
        <label class="label">Parent Directory</label>
        <div class="join w-full">
          <span class="join-item flex items-center px-3 border border-base-300 bg-base-200 text-sm text-base-content/60 max-w-[200px] truncate">
            {createParentDir || 'No directory selected'}
          </span>
          <button class="btn btn-sm join-item" onclick={pickCreateDir}>Browse...</button>
        </div>
      </fieldset>
      {#if createError}<p class="text-error text-sm">{createError}</p>{/if}
      <div class="flex gap-2">
        <button class="btn" onclick={() => step = 'none'}>Back</button>
        <button class="btn btn-primary" onclick={doCreate} disabled={createBusy}>{createBusy ? 'Creating...' : 'Create & Register'}</button>
      </div>
    </div>
  </div>

{:else if step === 'existing'}
  <div class="card bg-base-100 shadow max-w-lg">
    <div class="card-body gap-4">
      <h3 class="font-bold text-lg">Add Existing Library</h3>
      <fieldset class="fieldset">
        <label class="label">Library Directory</label>
        <div class="join w-full">
          <span class="join-item flex items-center px-3 border border-base-300 bg-base-200 text-sm text-base-content/60 max-w-[200px] truncate">
            {existingDir || 'No directory selected'}
          </span>
          <button class="btn btn-sm join-item" onclick={pickExistingDir}>Browse...</button>
        </div>
      </fieldset>
      <label class="flex items-center gap-2 text-sm cursor-pointer">
        <input type="checkbox" class="checkbox checkbox-sm" bind:checked={useRelative} />
        Use relative path (recommended for in-tree libraries)
      </label>
      {#if existingError}<p class="text-error text-sm">{existingError}</p>{/if}
      <div class="flex gap-2">
        <button class="btn" onclick={() => step = 'none'}>Back</button>
        <button class="btn btn-primary" onclick={doExisting} disabled={existingBusy}>{existingBusy ? 'Registering...' : 'Add & Register'}</button>
      </div>
    </div>
  </div>

{:else if step === 'git'}
  <div class="card bg-base-100 shadow max-w-lg">
    <div class="card-body gap-4">
      <h3 class="font-bold text-lg">Add Library from Git</h3>
      <p class="text-sm text-base-content/60">If this project is a Git repo, the library will be added as a submodule. Otherwise it will be cloned.</p>
      <fieldset class="fieldset">
        <label class="label">Git URL</label>
        <input class="input input-bordered w-full" type="text" placeholder="https://github.com/user/repo.git" bind:value={gitUrl} />
      </fieldset>
      <fieldset class="fieldset">
        <label class="label">Library Name</label>
        <input class="input input-bordered w-full" type="text" placeholder="e.g. shared-libs" bind:value={gitName} />
      </fieldset>
      <fieldset class="fieldset">
        <label class="label">Clone Into Directory</label>
        <div class="join w-full">
          <span class="join-item flex items-center px-3 border border-base-300 bg-base-200 text-sm text-base-content/60 max-w-[200px] truncate">
            {gitTargetDir || 'No directory selected'}
          </span>
          <button class="btn btn-sm join-item" onclick={pickGitDir}>Browse...</button>
        </div>
      </fieldset>
      {#if gitError}<p class="text-error text-sm">{gitError}</p>{/if}
      <div class="flex gap-2">
        <button class="btn" onclick={() => step = 'none'}>Back</button>
        <button class="btn btn-primary" onclick={doGit} disabled={gitBusy}>{gitBusy ? 'Cloning...' : 'Clone & Register'}</button>
      </div>
    </div>
  </div>

{:else if step === 'success'}
  <div class="card bg-base-100 shadow max-w-lg">
    <div class="card-body gap-4">
      <div class="alert alert-success">
        <span>Registered {addResult.registered_count} {addResult.registered_count !== 1 ? 'libraries' : 'library'} successfully!</span>
      </div>

      {#if addResult.httplib_paths.length > 0}
        <div>
          <p class="font-semibold text-sm mb-2">Add these files to your KiCad project libraries:</p>
          <div class="bg-base-200 rounded p-3 font-mono text-xs">
            {#each addResult.httplib_paths as p}
              <div class="mb-1">{p}</div>
            {/each}
          </div>
        </div>
        {#if showReg}
          <button class="btn btn-sm w-fit" onclick={registerInKiCad}>Register in KiCad sym-lib-table</button>
        {/if}
        {#if regStatus}<p class="text-sm">{regStatus}</p>{/if}
      {/if}

      <div class="flex gap-2">
        <button class="btn btn-primary" onclick={() => navigate('project', { path: projectPath })}>Go to Project</button>
        <button class="btn" onclick={() => navigate('dashboard')}>Back to Dashboard</button>
      </div>
    </div>
  </div>
{/if}
