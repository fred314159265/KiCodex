// Template editor view — edit field definitions
const TemplateEditorView = {
  async render(container, params) {
    const libPath = params.lib;
    const templateName = params.template;
    const projectPath = params.project;

    if (!libPath || !templateName) { navigate('dashboard'); return; }

    const template = await invoke('get_template', { libPath, templateName });

    container.innerHTML = '';

    const bc = h('div', { className: 'breadcrumb' },
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
      h('a', { href: `#project?path=${encodeURIComponent(projectPath)}` }, projectPath.split(/[\\/]/).pop()),
      h('span', {}, ' / '),
      h('span', {}, `Template: ${templateName}`),
    );
    container.appendChild(bc);

    const header = h('div', { className: 'page-header' },
      h('h2', { className: 'page-title' }, `Template: ${templateName}`),
    );
    container.appendChild(header);

    const card = h('div', { className: 'card' });

    // Based-on selector
    const basedOnGroup = h('div', { className: 'form-group' });
    basedOnGroup.appendChild(h('label', { className: 'form-label' }, 'Based On'));
    const basedOnInput = h('input', {
      className: 'form-input',
      type: 'text',
      value: template.based_on || '',
      placeholder: '_base (or leave empty)',
    });
    basedOnGroup.appendChild(basedOnInput);
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

    // Fields
    const fieldsContainer = h('div', { id: 'template-fields' });
    const fieldRows = [];

    function addFieldRow(f = {}) {
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

      const rowEl = h('div', { className: 'schema-field-row' },
        keyInput, nameInput, descInput, typeSelect,
        h('div', { className: 'checkbox-cell' }, reqCb),
        h('div', { className: 'checkbox-cell' }, visCb),
        removeBtn,
      );

      fieldRows.push(row);
      fieldsContainer.appendChild(rowEl);
    }

    for (const f of template.fields) {
      addFieldRow(f);
    }

    card.appendChild(fieldsContainer);

    // Add field + Save buttons
    const actions = h('div', { className: 'btn-group', style: { marginTop: '12px' } },
      h('button', { type: 'button', className: 'btn', onClick: () => addFieldRow() }, 'Add Field'),
      h('button', { type: 'button', className: 'btn btn-primary', onClick: async () => {
        const fields = fieldRows.map(r => ({
          key: r.keyInput.value,
          display_name: r.nameInput.value,
          field_type: r.typeSelect.value || null,
          required: r.reqCb.checked,
          visible: r.visCb.checked,
          description: r.descInput.value || null,
        })).filter(f => f.key);

        try {
          await invoke('save_template', {
            libPath,
            templateName,
            template: {
              based_on: basedOnInput.value || null,
              exclude_from_bom: bomCb.checked,
              exclude_from_board: boardCb.checked,
              exclude_from_sim: simCb.checked,
              fields,
            },
          });
          navigate('project', { path: projectPath });
        } catch (e) {
          alert('Error: ' + e);
        }
      }}, 'Save Template'),
    );
    card.appendChild(actions);

    container.appendChild(card);
  }
};
