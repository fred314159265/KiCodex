// Template editor view — edit field definitions
const TemplateEditorView = {
  async render(container, params) {
    const libPath = params.lib;
    const templateName = params.template;
    const projectPath = params.project;
    const isCreateMode = params.mode === 'create';

    if (!libPath || !templateName) { navigate('dashboard'); return; }

    const defaultFields = [
      { key: 'value', display_name: 'Name', field_type: null, required: true, visible: true, description: null },
      { key: 'description', display_name: 'Description', field_type: null, required: true, visible: true, description: null },
      { key: 'footprint', display_name: 'Footprint', field_type: 'kicad_footprint', required: true, visible: true, description: null },
      { key: 'symbol', display_name: 'Symbol', field_type: 'kicad_symbol', required: true, visible: true, description: null },
    ];

    const [template, availableTemplates] = await Promise.all([
      isCreateMode
        ? { based_on: null, exclude_from_bom: false, exclude_from_board: false, exclude_from_sim: false, fields: defaultFields }
        : invoke('get_template', { libPath, templateName }),
      invoke('list_templates', { libPath, exclude: templateName }).catch(() => []),
    ]);

    container.innerHTML = '';

    const bcParts = [
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
    ];
    if (projectPath) {
      bcParts.push(h('a', { href: `#project?path=${encodeURIComponent(projectPath)}` }, projectPath.split(/[\\/]/).pop()));
    } else {
      // Standalone library — link back to library view
      const libName = libPath.split(/[\\/]/).pop() || libPath;
      bcParts.push(h('a', { href: `#library?path=${encodeURIComponent(libPath)}` }, libName));
    }
    bcParts.push(h('span', {}, ' / '));
    bcParts.push(h('span', {}, isCreateMode ? `New Part Table: ${templateName}` : `Template: ${templateName}`));
    const bc = h('div', { className: 'breadcrumb' }, ...bcParts);
    container.appendChild(bc);

    const header = h('div', { className: 'page-header' },
      h('h2', { className: 'page-title' }, isCreateMode ? `New Part Table: ${templateName}` : `Template: ${templateName}`),
    );
    container.appendChild(header);

    const card = h('div', { className: 'card' });

    // Based-on selector (dropdown)
    const basedOnGroup = h('div', { className: 'form-group' });
    basedOnGroup.appendChild(h('label', { className: 'form-label' }, 'Based On'));
    const basedOnSelect = h('select', { className: 'form-select' },
      h('option', { value: '' }, '(none)'),
      ...availableTemplates.map(name =>
        h('option', { value: name }, name)
      ),
    );
    basedOnSelect.value = template.based_on || '';
    basedOnGroup.appendChild(basedOnSelect);
    card.appendChild(basedOnGroup);

    // Default Exclude Flags
    const flagsHeading = h('div', { style: { marginBottom: '8px' } },
      h('strong', {}, 'Default Exclude Flags'),
      h('div', { style: { fontSize: '12px', color: 'var(--text-muted)', marginTop: '2px' } }, '(used when not overridden on individual components)'),
    );
    card.appendChild(flagsHeading);
    const flagsRow = h('div', { style: { display: 'flex', gap: '16px', marginBottom: '16px' } });
    const bomCb = h('input', { type: 'checkbox' });
    if (template.exclude_from_bom) bomCb.checked = true;
    const boardCb = h('input', { type: 'checkbox' });
    if (template.exclude_from_board) boardCb.checked = true;
    const simCb = h('input', { type: 'checkbox' });
    if (template.exclude_from_sim) simCb.checked = true;

    flagsRow.appendChild(h('label', { style: { fontSize: '13px', display: 'flex', alignItems: 'center', gap: '4px' } }, bomCb, 'Exclude from BOM'));
    flagsRow.appendChild(h('label', { style: { fontSize: '13px', display: 'flex', alignItems: 'center', gap: '4px' } }, boardCb, 'Exclude from Board'));
    flagsRow.appendChild(h('label', { style: { fontSize: '13px', display: 'flex', alignItems: 'center', gap: '4px' } }, simCb, 'Exclude from Sim'));
    card.appendChild(flagsRow);

    // Field header
    const fieldHeader = h('div', { className: 'schema-field-row' },
      h('span', { className: 'schema-field-header' }, 'Key'),
      h('span', { className: 'schema-field-header' }, 'Display Name'),
      h('span', { className: 'schema-field-header' }, 'Description'),
      h('span', { className: 'schema-field-header' }, 'Type'),
      h('span', { className: 'schema-field-header checkbox-cell' }, 'Req'),
      h('span', { className: 'schema-field-header checkbox-cell' }, 'Vis'),
      h('span', {}),
    );
    card.appendChild(fieldHeader);

    // Track original field keys for rename/delete detection
    const originalKeys = new Map(); // maps row object -> original key
    const deletedFieldKeys = []; // keys of removed fields where user chose to also delete CSV data

    // Fields
    const fieldsContainer = h('div', { id: 'template-fields' });
    const fieldRows = [];

    function addFieldRow(f = {}, isExisting = false) {
      const row = {};
      const keyInput = h('input', { type: 'text', value: f.key || '', placeholder: 'field_key' });
      const nameInput = h('input', { type: 'text', value: f.display_name || '', placeholder: 'Display Name' });
      const descInput = h('input', { type: 'text', value: f.description || '', placeholder: 'Help text' });
      const typeSelect = h('select', {},
        h('option', { value: '' }, '(none)'),
        h('option', { value: 'kicad_symbol' }, 'kicad_symbol'),
        h('option', { value: 'kicad_footprint' }, 'kicad_footprint'),
        h('option', { value: 'url' }, 'url'),
      );
      if (f.field_type) typeSelect.value = f.field_type;
      const reqCb = h('input', { type: 'checkbox' });
      if (f.required) reqCb.checked = true;
      const visCb = h('input', { type: 'checkbox' });
      if (f.visible) visCb.checked = true;
      const removeBtn = h('button', { className: 'btn-icon', type: 'button', onClick: () => {
        const idx = fieldRows.indexOf(row);
        if (idx >= 0) {
          const origKey = originalKeys.get(row);
          if (origKey) {
            // This field existed before — ask about CSV data deletion
            const choice = confirm(
              `Remove field '${origKey}'?\n\n` +
              `OK = Also delete the '${origKey}' column and its data from all components using this template.\n` +
              `Cancel = Remove from template only (CSV data is kept).`
            );
            if (choice) {
              deletedFieldKeys.push(origKey);
            }
          }
          fieldRows.splice(idx, 1);
          rowEl.remove();
        }
      }}, '\u00d7');

      row.keyInput = keyInput;
      row.nameInput = nameInput;
      row.descInput = descInput;
      row.typeSelect = typeSelect;
      row.reqCb = reqCb;
      row.visCb = visCb;

      // Track original key for existing fields
      if (isExisting && f.key) {
        originalKeys.set(row, f.key);
      }

      const rowEl = h('div', { className: 'schema-field-row' },
        keyInput, nameInput, descInput, typeSelect,
        h('div', { className: 'checkbox-cell' }, reqCb),
        h('div', { className: 'checkbox-cell' }, visCb),
        removeBtn,
      );

      row.rowEl = rowEl;
      fieldRows.push(row);
      fieldsContainer.appendChild(rowEl);
    }

    for (const f of template.fields) {
      addFieldRow(f, !isCreateMode);
    }

    card.appendChild(fieldsContainer);

    // Error message container (above save button)
    const errorMsg = h('div', { className: 'field-error-msg', style: { display: 'none' } });

    // Validation function
    function validateFields() {
      const errors = [];

      // Clear previous highlights
      for (const row of fieldRows) {
        row.keyInput.classList.remove('form-input-error');
        row.nameInput.classList.remove('form-input-error');
      }

      if (fieldRows.length === 0) {
        errors.push('At least one field is required.');
      }

      const seenKeys = new Map(); // key -> first row index
      for (let i = 0; i < fieldRows.length; i++) {
        const row = fieldRows[i];
        const key = row.keyInput.value.trim();
        const displayName = row.nameInput.value.trim();

        if (!key) {
          row.keyInput.classList.add('form-input-error');
          errors.push(`Row ${i + 1}: Key is empty.`);
        } else {
          if (seenKeys.has(key)) {
            row.keyInput.classList.add('form-input-error');
            fieldRows[seenKeys.get(key)].keyInput.classList.add('form-input-error');
            errors.push(`Row ${i + 1}: Duplicate key "${key}".`);
          } else {
            seenKeys.set(key, i);
          }

          if (!displayName) {
            row.nameInput.classList.add('form-input-error');
            errors.push(`Row ${i + 1}: Display name is empty for key "${key}".`);
          }
        }
      }

      if (errors.length > 0) {
        errorMsg.textContent = errors.join(' ');
        errorMsg.style.display = 'block';
        return false;
      }

      errorMsg.style.display = 'none';
      return true;
    }

    // Add field + Save buttons
    card.appendChild(errorMsg);
    const actions = h('div', { className: 'btn-group', style: { marginTop: '12px' } },
      h('button', { type: 'button', className: 'btn', onClick: () => addFieldRow() }, 'Add Field'),
      h('button', { type: 'button', className: 'btn btn-primary', onClick: async () => {
        if (!validateFields()) return;

        const fields = fieldRows.map(r => ({
          key: r.keyInput.value.trim(),
          display_name: r.nameInput.value.trim(),
          field_type: r.typeSelect.value || null,
          required: r.reqCb.checked,
          visible: r.visCb.checked,
          description: r.descInput.value || null,
        }));

        // Compute renames: fields whose key changed from their original
        const renames = [];
        for (const row of fieldRows) {
          const origKey = originalKeys.get(row);
          const currentKey = row.keyInput.value.trim();
          if (origKey && currentKey && origKey !== currentKey) {
            renames.push({ from: origKey, to: currentKey });
          }
        }

        const templateData = {
          based_on: basedOnSelect.value || null,
          exclude_from_bom: bomCb.checked,
          exclude_from_board: boardCb.checked,
          exclude_from_sim: simCb.checked,
          fields,
        };

        try {
          if (isCreateMode) {
            await invoke('add_part_table', {
              libPath,
              componentTypeName: templateName,
              template: templateData,
            });
          } else {
            await invoke('save_template', {
              libPath,
              templateName,
              template: templateData,
              renames: renames.length > 0 ? renames : null,
              deletions: deletedFieldKeys.length > 0 ? deletedFieldKeys : null,
            });
          }
          if (projectPath) {
            navigate('project', { path: projectPath });
          } else {
            navigate('library', { path: libPath });
          }
        } catch (e) {
          alert('Error: ' + e);
        }
      }}, isCreateMode ? 'Create Part Table' : 'Save Template'),
    );
    card.appendChild(actions);

    container.appendChild(card);
  }
};
