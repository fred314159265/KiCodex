<script>
  /**
   * Picker — used as a modal overlay by RowForm/ComponentForm via openPicker().
   * This component is also exported as a standalone view if navigated to directly,
   * but primarily it's invoked programmatically via the global openPicker helper
   * that both form views use.
   *
   * Since RowForm/ComponentForm render Picker inline as a modal (not via routing),
   * we expose the Picker as a component that form views import directly.
   */
  import { invoke } from '../lib/tauri.js';

  let { kind = 'symbol', currentValue = '', onselect, onclose } = $props();

  let libraries = $state([]);
  let entries = $state([]);
  let selectedLib = $state(null);
  let leftFilter = $state('');
  let rightFilter = $state('');
  let loadingLibs = $state(true);
  let loadingEntries = $state(false);
  let libError = $state('');
  let entryError = $state('');

  function parseFilter(text) {
    const raw = text || '';
    const m = raw.match(/^\/(.*)\/([gimsuy]*)$/);
    if (m) {
      try {
        const re = new RegExp(m[1], m[2]);
        return { test: s => re.test(s), isRegex: true, isError: false };
      } catch {
        return { test: () => false, isRegex: true, isError: true };
      }
    }
    const f = raw.toLowerCase();
    return { test: s => !f || s.toLowerCase().includes(f), isRegex: false, isError: false };
  }

  let leftFilterObj = $derived(parseFilter(leftFilter));
  let rightFilterObj = $derived(parseFilter(rightFilter));

  let filteredLibs = $derived(libraries.filter(l => leftFilterObj.test(l)));
  let filteredEntries = $derived(entries.filter(e => rightFilterObj.test(e)));

  async function loadLibraries() {
    loadingLibs = true;
    libError = '';
    try {
      libraries = await invoke('list_kicad_libraries', { kind });
      // Pre-select if currentValue like "Device:R"
      const preselect = currentValue ? currentValue.split(':') : null;
      if (preselect && preselect.length === 2 && libraries.includes(preselect[0])) {
        await selectLibrary(preselect[0]);
      }
    } catch (e) {
      libError = String(e);
    } finally {
      loadingLibs = false;
    }
  }

  async function selectLibrary(lib) {
    selectedLib = lib;
    loadingEntries = true;
    entryError = '';
    entries = [];
    try {
      entries = await invoke('list_kicad_entries', { kind, libName: lib });
    } catch (e) {
      entryError = String(e);
    } finally {
      loadingEntries = false;
    }
  }

  function selectEntry(entry) {
    onselect?.(`${selectedLib}:${entry}`);
    onclose?.();
  }

  // Initialize on mount
  import { onMount, tick } from 'svelte';
  onMount(loadLibraries);

  let libList = $state(null);
  let entryList = $state(null);

  $effect(() => {
    // Re-run when selected lib or filtered list changes
    selectedLib; filteredLibs;
    tick().then(() => libList?.querySelector('.bg-primary')?.scrollIntoView({ block: 'start' }));
  });

  $effect(() => {
    // Re-run when entries or selection changes
    filteredEntries; currentValue; selectedLib;
    tick().then(() => entryList?.querySelector('.bg-primary')?.scrollIntoView({ block: 'start' }));
  });
</script>

<div class="modal modal-open">
  <div class="modal-box max-w-3xl h-[80vh] flex flex-col p-0">
    <div class="flex items-center justify-between px-4 py-3 border-b border-base-200">
      <h3 class="font-bold text-lg">Select {kind === 'symbol' ? 'Symbol' : 'Footprint'}</h3>
      <button class="btn btn-sm btn-circle btn-ghost" onclick={onclose}>✕</button>
    </div>

    <div class="flex flex-1 overflow-hidden">
      <!-- Left: Libraries -->
      <div class="flex flex-col w-1/2 border-r border-base-200 overflow-hidden">
        <div class="px-3 py-2 font-semibold text-sm border-b border-base-200">Libraries</div>
        <div class="px-3 py-2 border-b border-base-200">
          <input
            class="input input-bordered input-sm w-full {leftFilterObj.isError ? 'input-error' : leftFilterObj.isRegex ? 'input-success' : ''}"
            type="text"
            placeholder="Filter... (or /regex/)"
            bind:value={leftFilter}
          />
        </div>
        <div class="flex-1 overflow-y-auto" bind:this={libList}>
          {#if loadingLibs}
            <div class="flex items-center gap-2 p-4 text-base-content/60">
              <span class="loading loading-spinner loading-sm"></span>Loading...
            </div>
          {:else if libError}
            <div class="p-4 text-error text-sm">{libError}</div>
          {:else if filteredLibs.length === 0}
            <div class="p-4 text-center text-base-content/60 text-sm">No libraries found</div>
          {:else}
            {#each filteredLibs as lib}
              <button
                class="w-full text-left px-3 py-1.5 text-sm hover:bg-base-200 {lib === selectedLib ? 'bg-primary text-primary-content' : ''}"
                onclick={() => selectLibrary(lib)}
              >{lib}</button>
            {/each}
          {/if}
        </div>
      </div>

      <!-- Right: Entries -->
      <div class="flex flex-col w-1/2 overflow-hidden">
        <div class="px-3 py-2 font-semibold text-sm border-b border-base-200">Entries</div>
        <div class="px-3 py-2 border-b border-base-200">
          <input
            class="input input-bordered input-sm w-full {rightFilterObj.isError ? 'input-error' : rightFilterObj.isRegex ? 'input-success' : ''}"
            type="text"
            placeholder="Filter... (or /regex/)"
            bind:value={rightFilter}
          />
        </div>
        <div class="flex-1 overflow-y-auto" bind:this={entryList}>
          {#if loadingEntries}
            <div class="flex items-center gap-2 p-4 text-base-content/60">
              <span class="loading loading-spinner loading-sm"></span>Loading...
            </div>
          {:else if entryError}
            <div class="p-4 text-error text-sm">{entryError}</div>
          {:else if !selectedLib}
            <div class="p-4 text-center text-base-content/60 text-sm">Select a library</div>
          {:else if filteredEntries.length === 0}
            <div class="p-4 text-center text-base-content/60 text-sm">No entries found</div>
          {:else}
            {#each filteredEntries as entry}
              <button
                class="w-full text-left px-3 py-1.5 text-sm hover:bg-base-200 {currentValue === `${selectedLib}:${entry}` ? 'bg-primary text-primary-content' : ''}"
                onclick={() => selectEntry(entry)}
              >{entry}</button>
            {/each}
          {/if}
        </div>
      </div>
    </div>
  </div>
  <div class="modal-backdrop" onclick={onclose}></div>
</div>
