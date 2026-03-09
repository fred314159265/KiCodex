<script>
  import { onMount } from 'svelte';
  import { invoke } from '../lib/tauri.js';
  import Breadcrumb from '../components/Breadcrumb.svelte';

  let { params } = $props();

  let projectPath = $derived(params.project || '');
  let libParam = $derived(params.lib || '');

  let loading = $state(true);
  let result = $state(null);
  let error = $state('');

  function navigate(view, p = {}) {
    const qs = Object.entries(p)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    window.location.hash = qs ? `${view}?${qs}` : view;
  }

  let crumbs = $derived.by(() => {
    const c = [{ label: 'Dashboard', href: '#dashboard' }];
    if (projectPath) {
      const name = projectPath.split(/[\/\\]/).pop();
      c.push({ label: name, href: `#project?path=${encodeURIComponent(projectPath)}` });
    } else if (libParam) {
      const name = libParam.split(/[\/\\]/).pop() || libParam;
      c.push({ label: name, href: `#library?path=${encodeURIComponent(libParam)}` });
    }
    c.push({ label: 'Validate' });
    return c;
  });

  onMount(async () => {
    if (!projectPath && !libParam) { navigate('dashboard'); return; }

    let libPath = libParam;
    if (!libPath && projectPath) {
      const projects = await invoke('get_projects');
      const entry = projects.find(p => p.project_path === projectPath);
      if (!entry) { error = 'Project not found'; loading = false; return; }
      libPath = entry.library_path;
    }

    try {
      result = await invoke('validate_library', {
        libPath,
        projectPath: projectPath || null,
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });
</script>

<Breadcrumb {crumbs} />

{#if loading}
  <div class="flex items-center gap-2 p-8 text-base-content/60">
    <span class="loading loading-spinner"></span>
    Validating...
  </div>
{:else if error}
  <div class="alert alert-error"><span>{error}</span></div>
{:else if result}
  <div class="flex items-center justify-between mb-4">
    <h2 class="text-xl font-bold">Validation: {result.library}</h2>
  </div>

  <div class="flex flex-wrap gap-3 mb-6">
    {#if result.error_count === 0 && result.warning_count === 0}
      <span class="badge badge-success badge-lg">No issues found</span>
    {:else}
      {#if result.error_count > 0}
        <span class="badge badge-error badge-lg">{result.error_count} error{result.error_count !== 1 ? 's' : ''}</span>
      {/if}
      {#if result.warning_count > 0}
        <span class="badge badge-warning badge-lg">{result.warning_count} warning{result.warning_count !== 1 ? 's' : ''}</span>
      {/if}
    {/if}
    <span class="text-base-content/60 text-sm self-center">across {result.part_tables.length} part table{result.part_tables.length !== 1 ? 's' : ''}</span>
  </div>

  {#each result.part_tables as table}
    {#if table.errors.length > 0 || table.warnings.length > 0}
      <div class="card bg-base-100 shadow mb-4">
        <div class="card-body py-3">
          <h3 class="card-title text-base">{table.name} <span class="text-base-content/60 font-normal text-sm">({table.file})</span></h3>
          <div class="flex flex-col gap-1">
            {#each table.errors as err}
              <div class="alert alert-error py-2 text-sm">
                <span><strong>[ERROR]</strong> {err.row ? `Row ${err.row}${err.id ? ` (id=${err.id})` : ''}: ` : ''}{err.message}</span>
              </div>
            {/each}
            {#each table.warnings as warn}
              <div class="alert alert-warning py-2 text-sm">
                <span><strong>[WARN]</strong> {warn.row ? `Row ${warn.row}${warn.id ? ` (id=${warn.id})` : ''}: ` : ''}{warn.message}</span>
              </div>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  {/each}
{/if}
