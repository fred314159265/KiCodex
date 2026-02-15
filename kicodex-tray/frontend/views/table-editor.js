// Table editor view — data grid with add/edit/delete
const TableEditorView = {
  async render(container, params) {
    const libPath = params.lib;
    const tableName = params.table;
    const projectPath = params.project;

    if (!libPath || !tableName) { navigate('dashboard'); return; }

    const data = await invoke('get_table_rows', { libPath, tableName });

    container.innerHTML = '';

    const bc = h('div', { className: 'breadcrumb' },
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
      h('a', { href: `#project?path=${encodeURIComponent(projectPath)}` }, projectPath.split(/[\\/]/).pop()),
      h('span', {}, ' / '),
      h('span', {}, data.name),
    );
    container.appendChild(bc);

    const header = h('div', { className: 'page-header' },
      h('h2', { className: 'page-title' }, `${data.name} (${data.rows.length} rows)`),
      h('div', { className: 'btn-group' },
        h('button', { className: 'btn btn-primary', onClick: () => {
          navigate('row-form', { lib: libPath, table: tableName, project: projectPath, mode: 'add' });
        }}, 'Add Component'),
      ),
    );
    container.appendChild(header);

    if (data.rows.length === 0) {
      container.appendChild(h('div', { className: 'empty' }, 'No components yet. Click Add Component to create one.'));
      return;
    }

    // Build table
    const fields = data.schema.fields;
    const visibleKeys = ['id', ...fields.map(f => f.key).filter(k => k !== 'id')];

    const table = h('table', { className: 'data-table' });
    const thead = h('thead', {},
      h('tr', {},
        ...visibleKeys.map(k => {
          const field = fields.find(f => f.key === k);
          return h('th', {}, field ? field.display_name : k);
        }),
        h('th', { style: { width: '40px', textAlign: 'center' } }, 'BOM'),
        h('th', { style: { width: '40px', textAlign: 'center' } }, 'Board'),
        h('th', { style: { width: '40px', textAlign: 'center' } }, 'Sim'),
        h('th', {}, 'Actions'),
      ),
    );
    table.appendChild(thead);

    const tbody = h('tbody');
    for (const row of data.rows) {
      const tr = h('tr', {},
        ...visibleKeys.map(k => h('td', { title: row[k] || '' }, row[k] || '')),
        h('td', { style: { textAlign: 'center' } }, row['exclude_from_bom'] === 'true' ? '\u2713' : ''),
        h('td', { style: { textAlign: 'center' } }, row['exclude_from_board'] === 'true' ? '\u2713' : ''),
        h('td', { style: { textAlign: 'center' } }, row['exclude_from_sim'] === 'true' ? '\u2713' : ''),
        h('td', {},
          h('button', { className: 'btn btn-sm', onClick: () => {
            navigate('row-form', {
              lib: libPath, table: tableName, project: projectPath,
              mode: 'edit', id: row.id
            });
          }}, 'Edit'),
          h('button', { className: 'btn btn-sm btn-danger', style: { marginLeft: '4px' }, onClick: async () => {
            if (!confirm(`Delete row ${row.id}?`)) return;
            try {
              await invoke('delete_row', { libPath: libPath, tableName, id: row.id });
              renderRoute();
            } catch (e) { alert('Error: ' + e); }
          }}, 'Del'),
        ),
      );
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);
    container.appendChild(h('div', { className: 'table-wrap' }, table));
  }
};
