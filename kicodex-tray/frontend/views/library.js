// Library view — standalone library detail with part tables
const LibraryView = {
  async render(container, params) {
    const libraryPath = params.path;
    if (!libraryPath) { navigate('dashboard'); return; }

    const lib = await invoke('get_library_detail', { libraryPath });

    container.innerHTML = '';

    const bc = h('div', { className: 'breadcrumb' },
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
      h('span', {}, lib.name),
    );
    container.appendChild(bc);

    // Header with actions
    const header = h('div', { className: 'page-header' },
      h('div', {},
        h('h1', { className: 'page-title' }, lib.name),
        lib.description ? h('div', { className: 'card-subtitle' }, lib.description) : null,
      ),
      h('div', { className: 'btn-group' },
        h('button', { className: 'btn', onClick: () => invoke('open_in_explorer', { path: libraryPath }) }, 'Open Folder'),
        h('button', { className: 'btn', onClick: () => navigate('validate', { lib: libraryPath }) }, 'Validate'),
        h('button', { className: 'btn btn-primary', onClick: () => doAddPartTableStandalone(lib, libraryPath) }, 'Add Part Table'),
        h('button', { className: 'btn btn-danger', onClick: () => doRemoveStandaloneLibrary(libraryPath) }, 'Remove Library'),
      ),
    );
    container.appendChild(header);

    if (lib.part_tables.length === 0) {
      container.appendChild(h('div', { className: 'empty' }, 'No part tables in this library. Click "Add Part Table" to create one.'));
      return;
    }

    const table = h('table', { className: 'data-table' });
    const thead = h('thead', {},
      h('tr', {},
        h('th', {}, 'Part Table'),
        h('th', {}, 'Template'),
        h('th', {}, 'Components'),
        h('th', {}, 'Actions'),
      ),
    );
    table.appendChild(thead);
    const tbody = h('tbody');
    for (const ct of lib.part_tables) {
      const tr = h('tr', {},
        h('td', {},
          h('a', {
            href: `#part-table-editor?lib=${encodeURIComponent(lib.path)}&type=${encodeURIComponent(ct.name)}`,
            style: { color: 'var(--accent)', textDecoration: 'none' }
          }, ct.name),
        ),
        h('td', {}, ct.template_name),
        h('td', {}, String(ct.component_count)),
        h('td', {},
          h('div', { className: 'btn-group' },
            h('button', {
              className: 'btn btn-sm',
              onClick: () => navigate('template-editor', { lib: lib.path, template: ct.template_name })
            }, 'Template'),
            h('button', {
              className: 'btn btn-sm btn-danger',
              onClick: () => doDeletePartTableStandalone(lib.path, ct.name)
            }, 'Delete'),
          ),
        ),
      );
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);
    container.appendChild(h('div', { className: 'table-wrap' }, table));
  }
};

async function doRemoveStandaloneLibrary(libraryPath) {
  const yes = await window.__TAURI__.dialog.confirm(
    'Remove this library from KiCodex? (Files on disk will not be deleted)',
    { title: 'Remove Library', kind: 'warning' },
  );
  if (!yes) return;
  try {
    await invoke('remove_standalone_library', { libraryPath });
    navigate('dashboard');
  } catch (e) {
    alert('Error: ' + e);
  }
}

async function doDeletePartTableStandalone(libPath, name) {
  const yes = await window.__TAURI__.dialog.confirm(`Delete part table "${name}"? This will remove its data file and template.`, { title: 'Delete Part Table', kind: 'warning' });
  if (!yes) return;
  try {
    await invoke('delete_part_table', { libPath, partTableName: name });
    renderRoute();
  } catch (e) {
    alert('Error: ' + e);
  }
}

async function doAddPartTableStandalone(lib, libraryPath) {
  const overlay = h('div', { className: 'modal-overlay' });
  const modal = h('div', { className: 'modal', style: { width: '400px' } });

  const nameInput = h('input', {
    className: 'form-input',
    type: 'text',
    placeholder: 'e.g. capacitors',
    autofocus: true,
  });

  const errorDiv = h('div', { style: { color: 'var(--error)', fontSize: '12px', minHeight: '16px' } });

  const createBtn = h('button', {
    className: 'btn btn-primary',
    onClick: () => {
      const name = nameInput.value.trim();
      if (!name) { errorDiv.textContent = 'Name is required'; return; }
      overlay.remove();
      navigate('template-editor', { lib: libraryPath, template: name, mode: 'create' });
    },
  }, 'Next');

  nameInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') createBtn.click();
  });

  modal.appendChild(h('div', { className: 'modal-header' },
    h('h3', {}, 'New Part Table'),
    h('button', { className: 'modal-close', onClick: () => overlay.remove() }, '\u00d7'),
  ));
  modal.appendChild(h('div', { style: { padding: '16px', display: 'flex', flexDirection: 'column', gap: '12px' } },
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Part Table Name'),
      nameInput,
      errorDiv,
    ),
    h('div', { className: 'btn-group' }, createBtn),
  ));

  overlay.appendChild(modal);
  overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
  document.body.appendChild(overlay);
  nameInput.focus();
}
