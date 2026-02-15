// Row form view — schema-aware form for add/edit
const RowFormView = {
  async render(container, params) {
    const libPath = params.lib;
    const tableName = params.table;
    const projectPath = params.project;
    const mode = params.mode || 'add';
    const editId = params.id;

    if (!libPath || !tableName) { navigate('dashboard'); return; }

    const data = await invoke('get_table_rows', { libPath, tableName });
    const fields = data.schema.fields;
    let existingRow = null;
    if (mode === 'edit' && editId) {
      existingRow = data.rows.find(r => r.id === editId);
      if (!existingRow) {
        container.innerHTML = '<div class="empty">Row not found</div>';
        return;
      }
    }

    container.innerHTML = '';

    const bc = h('div', { className: 'breadcrumb' },
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
      h('a', { href: `#table-editor?lib=${encodeURIComponent(libPath)}&table=${encodeURIComponent(tableName)}&project=${encodeURIComponent(projectPath)}` }, data.name),
      h('span', {}, ' / '),
      h('span', {}, mode === 'edit' ? `Edit #${editId}` : 'Add Component'),
    );
    container.appendChild(bc);

    const header = h('div', { className: 'page-header' },
      h('h2', { className: 'page-title' }, mode === 'edit' ? `Edit Component #${editId}` : 'Add Component'),
    );
    container.appendChild(header);

    const form = h('form', { className: 'card' });
    const inputs = {};

    for (const field of fields) {
      if (field.key === 'id') continue;

      const group = h('div', { className: 'form-group' });
      const label = h('label', { className: 'form-label' },
        field.display_name,
        field.required ? h('span', { className: 'required' }, '*') : null,
      );
      group.appendChild(label);

      const value = existingRow ? (existingRow[field.key] || '') : '';
      const isKicad = field.field_type === 'kicad_symbol' || field.field_type === 'kicad_footprint';

      if (isKicad) {
        const inputDiv = h('div', { className: 'input-with-btn' });
        const input = h('input', {
          className: 'form-input',
          type: 'text',
          value,
          placeholder: 'Library:Entry',
        });
        inputs[field.key] = input;
        const kind = field.field_type === 'kicad_symbol' ? 'symbol' : 'footprint';
        const browseBtn = h('button', {
          type: 'button',
          className: 'btn',
          onClick: () => openPicker(kind, (val) => { input.value = val; }),
        }, 'Browse');
        inputDiv.appendChild(input);
        inputDiv.appendChild(browseBtn);
        group.appendChild(inputDiv);
      } else {
        const input = h('input', {
          className: 'form-input',
          type: field.field_type === 'url' ? 'url' : 'text',
          value,
          placeholder: field.description || '',
        });
        inputs[field.key] = input;
        group.appendChild(input);
      }

      form.appendChild(group);
    }

    // Component Settings — per-component exclude overrides
    const excludeSep = h('div', { style: { marginTop: '20px', marginBottom: '8px', borderTop: '1px solid var(--border)', paddingTop: '12px' } },
      h('strong', {}, 'Component Settings'),
    );
    form.appendChild(excludeSep);

    const excludeFlags = [
      { key: 'exclude_from_bom', label: 'Exclude from BOM' },
      { key: 'exclude_from_board', label: 'Exclude from Board' },
      { key: 'exclude_from_sim', label: 'Exclude from Sim' },
    ];
    const excludeInputs = {};
    for (const flag of excludeFlags) {
      const cb = h('input', { type: 'checkbox' });
      if (existingRow && existingRow[flag.key] === 'true') cb.checked = true;
      excludeInputs[flag.key] = cb;
      const group = h('div', { className: 'form-group', style: { display: 'flex', alignItems: 'center', gap: '8px' } },
        cb,
        h('label', { className: 'form-label', style: { margin: 0 } }, flag.label),
      );
      form.appendChild(group);
    }

    const btnGroup = h('div', { className: 'btn-group', style: { marginTop: '16px' } },
      h('button', { type: 'submit', className: 'btn btn-primary' }, mode === 'edit' ? 'Save' : 'Add'),
      h('button', { type: 'button', className: 'btn', onClick: () => {
        navigate('table-editor', { lib: libPath, table: tableName, project: projectPath });
      }}, 'Cancel'),
    );
    form.appendChild(btnGroup);

    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const fieldValues = {};
      for (const [key, input] of Object.entries(inputs)) {
        fieldValues[key] = input.value;
      }
      for (const [key, cb] of Object.entries(excludeInputs)) {
        fieldValues[key] = cb.checked ? 'true' : '';
      }
      try {
        if (mode === 'edit') {
          await invoke('update_row', { libPath, tableName, id: editId, fields: fieldValues });
        } else {
          await invoke('add_row', { libPath, tableName, fields: fieldValues });
        }
        navigate('table-editor', { lib: libPath, table: tableName, project: projectPath });
      } catch (err) {
        alert('Error: ' + err);
      }
    });

    container.appendChild(form);
  }
};
