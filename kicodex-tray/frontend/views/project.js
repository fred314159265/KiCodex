// Project view — library detail with tables
const ProjectView = {
  async render(container, params) {
    const projectPath = params.path;
    if (!projectPath) { navigate('dashboard'); return; }

    const libraries = await invoke('get_project_libraries', { projectPath });

    container.innerHTML = '';

    const bc = h('div', { className: 'breadcrumb' },
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
      h('span', {}, projectPath.split(/[\\/]/).pop()),
    );
    container.appendChild(bc);

    if (libraries.length === 0) {
      container.appendChild(h('div', { className: 'empty' }, 'No libraries found for this project.'));
      return;
    }

    for (const lib of libraries) {
      const section = h('div', { style: { marginBottom: '24px' } });

      const header = h('div', { className: 'page-header' },
        h('div', {},
          h('h2', { className: 'page-title' }, lib.name),
          lib.description ? h('div', { className: 'card-subtitle' }, lib.description) : null,
        ),
        h('div', { className: 'btn-group' },
          h('button', { className: 'btn', onClick: () => invoke('open_in_explorer', { path: projectPath }) }, 'Open Folder'),
          h('button', { className: 'btn', onClick: () => doValidate(lib, projectPath) }, 'Validate'),
          h('button', { className: 'btn', onClick: () => doAddTable(lib, projectPath) }, 'Add Table'),
        ),
      );
      section.appendChild(header);

      if (lib.tables.length === 0) {
        section.appendChild(h('div', { className: 'empty' }, 'No tables in this library.'));
      } else {
        const table = h('table', { className: 'data-table' });
        const thead = h('thead', {},
          h('tr', {},
            h('th', {}, 'Table'),
            h('th', {}, 'Schema'),
            h('th', {}, 'Rows'),
            h('th', {}, 'Actions'),
          ),
        );
        table.appendChild(thead);
        const tbody = h('tbody');
        for (const t of lib.tables) {
          const tr = h('tr', {},
            h('td', {},
              h('a', {
                href: `#table-editor?lib=${encodeURIComponent(lib.path)}&table=${encodeURIComponent(t.name)}&project=${encodeURIComponent(projectPath)}`,
                style: { color: 'var(--accent)', textDecoration: 'none' }
              }, t.name),
            ),
            h('td', {}, t.schema_name),
            h('td', {}, String(t.row_count)),
            h('td', {},
              h('button', {
                className: 'btn btn-sm',
                onClick: () => navigate('schema-editor', { lib: lib.path, schema: t.schema_name, project: projectPath })
              }, 'Schema'),
            ),
          );
          tbody.appendChild(tr);
        }
        table.appendChild(tbody);
        section.appendChild(h('div', { className: 'table-wrap' }, table));
      }

      container.appendChild(section);
    }
  }
};

async function doValidate(lib, projectPath) {
  navigate('validate', { project: projectPath });
}

async function doAddTable(lib, projectPath) {
  const name = prompt('Table name (lowercase, e.g. "capacitors"):');
  if (!name) return;
  try {
    await invoke('add_table', { libPath: lib.path, tableName: name });
    renderRoute();
  } catch (e) {
    alert('Error: ' + e);
  }
}
