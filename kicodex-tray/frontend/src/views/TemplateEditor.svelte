<script>
  import { onMount } from 'svelte';
  import { invoke } from '../lib/tauri.js';
  import Breadcrumb from '../components/Breadcrumb.svelte';
  import { ask } from '../lib/confirm.svelte.js';
  import { showError } from '../lib/toast.svelte.js';

  let { params } = $props();
  let libPath = $derived(params.lib || '');
  let templateName = $derived(params.template || '');
  let projectPath = $derived(params.project || '');
  let isCreateMode = $derived(params.mode === 'create');

  let template = $state(null);
  let availableTemplates = $state([]);
  let loading = $state(true);
  let error = $state('');

  let basedOn = $state('');
  let excludeFromBom = $state(false);
  let excludeFromBoard = $state(false);
  let excludeFromSim = $state(false);
  let fieldRows = $state([]);
  let validationErrors = $state([]);

  // Track original keys for rename detection: fieldRow index -> original key
  // We use a parallel array since $state arrays of objects don't support Map well
  let originalKeys = [];
  let deletedFieldKeys = [];

  const DEFAULT_FIELDS = [
    { key: 'value',       display_name: 'Value',       field_type: null,              required: true,  visible: true,  description: null },
    { key: 'description', display_name: 'Description',  field_type: null,              required: true,  visible: false, description: null },
    { key: 'footprint',   display_name: 'Footprint',    field_type: 'kicad_footprint', required: true,  visible: false, description: null },
    { key: 'symbol',      display_name: 'Symbol',       field_type: 'kicad_symbol',    required: true,  visible: false, description: null },
    { key: 'datasheet',   display_name: 'Datasheet',    field_type: 'url',             required: false, visible: false, description: null },
  ];

  function navigate(view, p = {}) {
    const qs = Object.entries(p)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    window.location.hash = qs ? `${view}?${qs}` : view;
  }

  onMount(async () => {
    if (!libPath || !templateName) { navigate('dashboard'); return; }
    try {
      const [tmpl, templates] = await Promise.all([
        isCreateMode
          ? Promise.resolve({ based_on: null, exclude_from_bom: false, exclude_from_board: false, exclude_from_sim: false, fields: DEFAULT_FIELDS })
          : invoke('get_template', { libPath, templateName }),
        invoke('list_templates', { libPath, exclude: templateName }).catch(() => []),
      ]);
      template = tmpl;
      availableTemplates = templates;
      basedOn = tmpl.based_on || '';
      excludeFromBom = tmpl.exclude_from_bom;
      excludeFromBoard = tmpl.exclude_from_board;
      excludeFromSim = tmpl.exclude_from_sim;
      fieldRows = tmpl.fields.map(f => ({ ...f, field_type: f.field_type || '' }));
      // Initialize originalKeys: for create mode, all are new so no original keys
      originalKeys = isCreateMode
        ? tmpl.fields.map(() => null)
        : tmpl.fields.map(f => f.key);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  function addField() {
    fieldRows = [...fieldRows, { key: '', display_name: '', description: '', field_type: '', required: false, visible: false }];
    originalKeys = [...originalKeys, null];
  }

  async function removeField(i) {
    const origKey = originalKeys[i];
    if (origKey) {
      const deleteData = await ask(
        `Also delete the '${origKey}' column data from all components using this template?`,
        { title: `Remove Field: ${origKey}`, confirmLabel: 'Delete Data Too', cancelLabel: 'Keep Data' },
      );
      if (deleteData) deletedFieldKeys.push(origKey);
    }
    fieldRows = fieldRows.filter((_, idx) => idx !== i);
    originalKeys = originalKeys.filter((_, idx) => idx !== i);
  }

  function validate() {
    const errors = [];
    const seenKeys = new Map();
    for (let i = 0; i < fieldRows.length; i++) {
      const key = fieldRows[i].key.trim();
      const dn = fieldRows[i].display_name.trim();
      if (!key) {
        errors.push(`Row ${i + 1}: Key is empty.`);
      } else {
        if (seenKeys.has(key)) {
          errors.push(`Row ${i + 1}: Duplicate key "${key}".`);
        } else {
          seenKeys.set(key, i);
        }
        if (!dn) errors.push(`Row ${i + 1}: Display name is empty for key "${key}".`);
      }
    }
    if (fieldRows.length === 0) errors.push('At least one field is required.');
    validationErrors = errors;
    return errors.length === 0;
  }

  async function save() {
    if (!validate()) return;

    const fields = fieldRows.map(r => ({
      key: r.key.trim(),
      display_name: r.display_name.trim(),
      field_type: r.field_type || null,
      required: r.required,
      visible: r.visible,
      description: r.description || null,
    }));

    const renames = [];
    for (let i = 0; i < fieldRows.length; i++) {
      const orig = originalKeys[i];
      const cur = fieldRows[i].key.trim();
      if (orig && cur && orig !== cur) {
        renames.push({ from: orig, to: cur });
      }
    }

    const templateData = {
      based_on: basedOn || null,
      exclude_from_bom: excludeFromBom,
      exclude_from_board: excludeFromBoard,
      exclude_from_sim: excludeFromSim,
      fields,
    };

    try {
      if (isCreateMode) {
        await invoke('add_part_table', { libPath, componentTypeName: templateName, template: templateData });
      } else {
        await invoke('save_template', {
          libPath,
          templateName,
          template: templateData,
          renames: renames.length > 0 ? renames : null,
          deletions: deletedFieldKeys.length > 0 ? deletedFieldKeys : null,
        });
      }
      if (projectPath) navigate('project', { path: projectPath });
      else navigate('library', { path: libPath });
    } catch (e) {
      showError(e);
    }
  }

  let crumbs = $derived.by(() => {
    const c = [{ label: 'Dashboard', href: '#dashboard' }];
    if (projectPath) {
      c.push({ label: projectPath.split(/[\/\\]/).pop(), href: `#project?path=${encodeURIComponent(projectPath)}` });
    } else if (libPath) {
      const name = libPath.split(/[\/\\]/).pop() || libPath;
      c.push({ label: name, href: `#library?path=${encodeURIComponent(libPath)}` });
    }
    c.push({ label: isCreateMode ? `New Part Table: ${templateName}` : `Template: ${templateName}` });
    return c;
  });
</script>

{#if loading}
  <div class="flex items-center gap-2 p-8 text-base-content/60">
    <span class="loading loading-spinner"></span>Loading...
  </div>
{:else if error}
  <div class="alert alert-error"><span>{error}</span></div>
{:else if template}
  <Breadcrumb {crumbs} />
  <h2 class="text-xl font-bold mb-4">{isCreateMode ? `New Part Table: ${templateName}` : `Template: ${templateName}`}</h2>

  <div class="card bg-base-100 shadow">
    <div class="card-body gap-4">
      <fieldset class="fieldset">
        <label class="label" for="based-on">Based On</label>
        <select id="based-on" class="select select-bordered w-full max-w-xs" bind:value={basedOn}>
          <option value="">(none)</option>
          {#each availableTemplates as name}
            <option value={name}>{name}</option>
          {/each}
        </select>
      </fieldset>

      <div>
        <p class="font-semibold text-sm mb-1">Default Exclude Flags</p>
        <p class="text-xs text-base-content/60 mb-2">(used when not overridden on individual components)</p>
        <div class="flex flex-wrap gap-4">
          <label class="flex items-center gap-2 text-sm cursor-pointer">
            <input type="checkbox" class="checkbox checkbox-sm" bind:checked={excludeFromBom} />Exclude from BOM
          </label>
          <label class="flex items-center gap-2 text-sm cursor-pointer">
            <input type="checkbox" class="checkbox checkbox-sm" bind:checked={excludeFromBoard} />Exclude from Board
          </label>
          <label class="flex items-center gap-2 text-sm cursor-pointer">
            <input type="checkbox" class="checkbox checkbox-sm" bind:checked={excludeFromSim} />Exclude from Sim
          </label>
        </div>
      </div>

      <div class="grid grid-cols-[1fr_1fr_1fr_120px_40px_40px_32px] gap-2 text-xs font-semibold text-base-content/60 border-b border-base-200 pb-1">
        <span>Key</span><span>Display Name</span><span>Description</span><span>Type</span>
        <span class="text-center">Req</span><span class="text-center">Vis</span><span></span>
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

      {#if validationErrors.length > 0}
        <div class="alert alert-error text-sm">
          <ul class="list-disc list-inside">
            {#each validationErrors as err}<li>{err}</li>{/each}
          </ul>
        </div>
      {/if}

      <div class="flex gap-2 mt-2">
        <button class="btn btn-sm" onclick={addField}>Add Field</button>
        <button class="btn btn-sm btn-primary" onclick={save}>{isCreateMode ? 'Create Part Table' : 'Save Template'}</button>
      </div>
    </div>
  </div>
{/if}
