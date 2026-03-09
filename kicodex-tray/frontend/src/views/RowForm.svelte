<script>
  import { onMount } from 'svelte';
  import { invoke } from '../lib/tauri.js';
  import Breadcrumb from '../components/Breadcrumb.svelte';
  import Picker from './Picker.svelte';
  import { showError } from '../lib/toast.svelte.js';

  let { params } = $props();
  let libPath = $derived(params.lib || '');
  let tableName = $derived(params.table || '');
  let projectPath = $derived(params.project || '');
  let mode = $derived(params.mode || 'add');
  let editId = $derived(params.id || '');

  let data = $state(null);
  let loading = $state(true);
  let error = $state('');
  let formValues = $state({});
  let excludeFlags = $state({ exclude_from_bom: false, exclude_from_board: false, exclude_from_sim: false });

  // Picker state
  let pickerOpen = $state(false);
  let pickerKind = $state('symbol');
  let pickerCurrentValue = $state('');
  let pickerFieldKey = $state('');

  function navigate(view, p = {}) {
    const qs = Object.entries(p)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    window.location.hash = qs ? `${view}?${qs}` : view;
  }

  onMount(async () => {
    if (!libPath || !tableName) { navigate('dashboard'); return; }
    try {
      data = await invoke('get_table_rows', { libPath, tableName });
      const fields = data.schema.fields;
      const initial = {};
      const existing = mode === 'edit' && editId ? data.rows.find(r => r.id === editId) : null;
      for (const f of fields) {
        if (f.key === 'id') continue;
        initial[f.key] = existing ? (existing[f.key] || '') : '';
      }
      formValues = initial;
      excludeFlags = {
        exclude_from_bom: existing?.exclude_from_bom === 'true',
        exclude_from_board: existing?.exclude_from_board === 'true',
        exclude_from_sim: existing?.exclude_from_sim === 'true',
      };
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  function openPicker(kind, fieldKey, currentValue) {
    pickerKind = kind;
    pickerFieldKey = fieldKey;
    pickerCurrentValue = currentValue;
    pickerOpen = true;
  }

  function onPickerSelect(value) {
    formValues = { ...formValues, [pickerFieldKey]: value };
    pickerOpen = false;
  }

  async function submit(e) {
    e.preventDefault();
    const fields = { ...formValues };
    for (const [k, v] of Object.entries(excludeFlags)) {
      fields[k] = v ? 'true' : '';
    }
    try {
      if (mode === 'edit') {
        await invoke('update_row', { libPath, tableName, id: editId, fields });
      } else {
        await invoke('add_row', { libPath, tableName, fields });
      }
      navigate('table-editor', { lib: libPath, table: tableName, project: projectPath });
    } catch (err) {
      showError(err);
    }
  }

  let crumbs = $derived.by(() => {
    const c = [{ label: 'Dashboard', href: '#dashboard' }];
    if (data) {
      c.push({ label: data.name, href: `#table-editor?lib=${encodeURIComponent(libPath)}&table=${encodeURIComponent(tableName)}&project=${encodeURIComponent(projectPath)}` });
    }
    c.push({ label: mode === 'edit' ? `Edit #${editId}` : 'Add Component' });
    return c;
  });
</script>

{#if loading}
  <div class="flex items-center gap-2 p-8 text-base-content/60">
    <span class="loading loading-spinner"></span>Loading...
  </div>
{:else if error}
  <div class="alert alert-error"><span>{error}</span></div>
{:else if data}
  <Breadcrumb {crumbs} />
  <h2 class="text-xl font-bold mb-4">{mode === 'edit' ? `Edit Component #${editId}` : 'Add Component'}</h2>

  <form class="card bg-base-100 shadow" onsubmit={submit}>
    <div class="card-body gap-4">
      {#each data.schema.fields as field}
        {#if field.key !== 'id'}
          {@const isKicad = field.field_type === 'kicad_symbol' || field.field_type === 'kicad_footprint'}
          <fieldset class="fieldset">
            <label class="label" for={`field-${field.key}`}>
              {field.display_name}{#if field.required}<span class="text-error ml-1">*</span>{/if}
            </label>
            {#if isKicad}
              <div class="join w-full">
                <input
                  id={`field-${field.key}`}
                  class="input input-bordered join-item flex-1"
                  type="text"
                  bind:value={formValues[field.key]}
                  placeholder="Library:Entry"
                />
                <button
                  type="button"
                  class="btn join-item"
                  onclick={() => openPicker(field.field_type === 'kicad_symbol' ? 'symbol' : 'footprint', field.key, formValues[field.key])}
                >Browse</button>
              </div>
            {:else}
              <input
                id={`field-${field.key}`}
                class="input input-bordered w-full"
                type={field.field_type === 'url' ? 'url' : 'text'}
                bind:value={formValues[field.key]}
                placeholder={field.description || ''}
              />
            {/if}
          </fieldset>
        {/if}
      {/each}

      <div class="divider my-2 text-sm">Component Settings</div>

      <label class="flex items-center gap-2 text-sm cursor-pointer">
        <input type="checkbox" class="checkbox checkbox-sm" bind:checked={excludeFlags.exclude_from_bom} />
        Exclude from BOM
      </label>
      <label class="flex items-center gap-2 text-sm cursor-pointer">
        <input type="checkbox" class="checkbox checkbox-sm" bind:checked={excludeFlags.exclude_from_board} />
        Exclude from Board
      </label>
      <label class="flex items-center gap-2 text-sm cursor-pointer">
        <input type="checkbox" class="checkbox checkbox-sm" bind:checked={excludeFlags.exclude_from_sim} />
        Exclude from Sim
      </label>

      <div class="flex gap-2 mt-2">
        <button type="submit" class="btn btn-primary">{mode === 'edit' ? 'Save' : 'Add'}</button>
        <button type="button" class="btn" onclick={() => navigate('table-editor', { lib: libPath, table: tableName, project: projectPath })}>Cancel</button>
      </div>
    </div>
  </form>
{/if}

{#if pickerOpen}
  <Picker kind={pickerKind} currentValue={pickerCurrentValue} onselect={onPickerSelect} onclose={() => pickerOpen = false} />
{/if}
