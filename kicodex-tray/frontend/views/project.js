// Project view — library detail with part tables
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

    // Project-level header with actions
    const projectHeader = h('div', { className: 'page-header' },
      h('h1', { className: 'page-title' }, projectPath.split(/[\\/]/).pop()),
      h('div', { className: 'btn-group' },
        h('button', { className: 'btn', onClick: () => invoke('open_in_explorer', { path: projectPath }) }, 'Open Folder'),
        h('button', { className: 'btn', onClick: () => doScanForLibraries(projectPath) }, 'Scan for Libraries'),
        h('button', { className: 'btn btn-primary', onClick: () => doNewLibrary(projectPath) }, 'New Library'),
        h('button', { className: 'btn btn-danger', onClick: () => doRemoveProject(projectPath) }, 'Remove Project'),
      ),
    );
    container.appendChild(projectHeader);

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
          h('button', { className: 'btn', onClick: () => doValidate(lib, projectPath) }, 'Validate'),
          h('button', { className: 'btn', onClick: () => doAddPartTable(lib, projectPath) }, 'Add Part Table'),
        ),
      );
      section.appendChild(header);

      if (lib.part_tables.length === 0) {
        section.appendChild(h('div', { className: 'empty' }, 'No part tables in this library.'));
      } else {
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
                href: `#part-table-editor?lib=${encodeURIComponent(lib.path)}&type=${encodeURIComponent(ct.name)}&project=${encodeURIComponent(projectPath)}`,
                style: { color: 'var(--accent)', textDecoration: 'none' }
              }, ct.name),
            ),
            h('td', {}, ct.template_name),
            h('td', {}, String(ct.component_count)),
            h('td', {},
              h('button', {
                className: 'btn btn-sm',
                onClick: () => navigate('template-editor', { lib: lib.path, template: ct.template_name, project: projectPath })
              }, 'Template'),
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

async function doRemoveProject(projectPath) {
  const yes = await window.__TAURI__.dialog.confirm('Remove this project from KiCodex?', { title: 'Remove Project', kind: 'warning' });
  if (!yes) return;
  try {
    await invoke('remove_project', { projectPath });
    navigate('dashboard');
  } catch (e) {
    alert('Error: ' + e);
  }
}

async function doNewLibrary(projectPath) {
  const overlay = h('div', { className: 'modal-overlay' });
  const modal = h('div', { className: 'modal', style: { width: '400px' } });

  const nameInput = h('input', {
    className: 'form-input',
    type: 'text',
    placeholder: 'e.g. my-components',
    autofocus: true,
  });

  const errorDiv = h('div', { style: { color: 'var(--error)', fontSize: '12px', minHeight: '16px' } });

  const createBtn = h('button', {
    className: 'btn btn-primary',
    onClick: async () => {
      const name = nameInput.value.trim();
      if (!name) { errorDiv.textContent = 'Name is required'; return; }
      try {
        createBtn.disabled = true;
        createBtn.textContent = 'Creating...';
        await invoke('create_library', { name, parentDir: projectPath });
        await invoke('add_project', {
          projectPath,
          libraries: [{ name, path: name, is_new: true }],
        });
        overlay.remove();
        renderRoute();
      } catch (e) {
        errorDiv.textContent = String(e);
        createBtn.disabled = false;
        createBtn.textContent = 'Create';
      }
    },
  }, 'Create');

  nameInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') createBtn.click();
  });

  modal.appendChild(h('div', { className: 'modal-header' },
    h('h3', {}, 'New Library'),
    h('button', { className: 'modal-close', onClick: () => overlay.remove() }, '\u00d7'),
  ));
  modal.appendChild(h('div', { style: { padding: '16px', display: 'flex', flexDirection: 'column', gap: '12px' } },
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Library Name'),
      nameInput,
      errorDiv,
    ),
    h('div', { style: { fontSize: '13px', color: 'var(--text-muted)' } },
      'Will be created in: ' + projectPath,
    ),
    h('div', { className: 'btn-group' }, createBtn),
  ));

  overlay.appendChild(modal);
  overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
  document.body.appendChild(overlay);
  nameInput.focus();
}

async function doValidate(lib, projectPath) {
  navigate('validate', { project: projectPath });
}

async function doAddPartTable(lib, projectPath) {
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
      navigate('template-editor', { lib: lib.path, template: name, project: projectPath, mode: 'create' });
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

async function doScanForLibraries(projectPath) {
  try {
    const results = await invoke('scan_for_libraries', { path: projectPath });
    const newLibs = results.filter(r => r.is_new);
    if (newLibs.length === 0) {
      alert('No new libraries found.');
      return;
    }

    const overlay = h('div', { className: 'modal-overlay' });
    const modal = h('div', { className: 'modal', style: { width: '450px' } });

    const checkboxes = [];
    const listDiv = h('div', { style: { display: 'flex', flexDirection: 'column', gap: '8px' } });
    for (const lib of newLibs) {
      const cb = h('input', { type: 'checkbox', checked: true });
      checkboxes.push({ cb, lib });
      listDiv.appendChild(h('label', { style: { display: 'flex', alignItems: 'center', gap: '8px', fontSize: '13px' } },
        cb,
        h('span', {}, lib.name),
        h('span', { style: { color: 'var(--text-muted)', marginLeft: '4px' } }, lib.path),
      ));
    }

    const confirmBtn = h('button', {
      className: 'btn btn-primary',
      onClick: async () => {
        const selected = checkboxes.filter(c => c.cb.checked).map(c => c.lib);
        if (selected.length === 0) { overlay.remove(); return; }
        try {
          confirmBtn.disabled = true;
          confirmBtn.textContent = 'Adding...';
          await invoke('add_project', { projectPath, libraries: selected });
          overlay.remove();
          renderRoute();
        } catch (e) {
          alert('Error: ' + e);
          confirmBtn.disabled = false;
          confirmBtn.textContent = 'Add Selected';
        }
      }
    }, 'Add Selected');

    modal.appendChild(h('div', { className: 'modal-header' },
      h('h3', {}, `Found ${newLibs.length} new ${newLibs.length === 1 ? 'library' : 'libraries'}`),
      h('button', { className: 'modal-close', onClick: () => overlay.remove() }, '\u00d7'),
    ));
    modal.appendChild(h('div', { style: { padding: '16px', display: 'flex', flexDirection: 'column', gap: '12px' } },
      listDiv,
      h('div', { className: 'btn-group' }, confirmBtn),
    ));

    overlay.appendChild(modal);
    overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
    document.body.appendChild(overlay);
  } catch (e) {
    alert('Error scanning: ' + e);
  }
}
