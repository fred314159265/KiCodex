// Dashboard view — project-centric with discovered projects
const DashboardView = {
  async render(container, params) {
    const [projects, discovered] = await Promise.all([
      invoke('get_projects'),
      invoke('get_discovered_projects'),
    ]);

    container.innerHTML = '';

    // Header with action buttons
    const header = h('div', { className: 'page-header' },
      h('h1', { className: 'page-title' }, 'Dashboard'),
      h('div', { className: 'btn-group' },
        h('button', { className: 'btn', onClick: () => promptAddProject() }, 'Add Project'),
        h('button', { className: 'btn btn-primary', onClick: () => promptNewLibrary() }, 'New Library'),
      )
    );
    container.appendChild(header);

    // Section 1: Your Projects (grouped by project_path)
    const grouped = {};
    for (const p of projects) {
      if (!grouped[p.project_path]) {
        grouped[p.project_path] = { libraries: [], active: p.active };
      }
      grouped[p.project_path].libraries.push(p);
      if (p.active) grouped[p.project_path].active = true;
    }

    const projectPaths = Object.keys(grouped);

    if (projectPaths.length > 0) {
      container.appendChild(h('h2', { className: 'section-title' }, 'Your Projects'));
      const grid = h('div', { className: 'card-grid' });

      for (const projPath of projectPaths) {
        const group = grouped[projPath];
        const libs = group.libraries;
        const projName = projPath.split(/[\\/]/).pop() || projPath;
        const totalTables = libs.reduce((sum, l) => sum + l.table_count, 0);

        const card = h('div', { className: 'card', style: { cursor: 'pointer' } },
          h('div', { className: 'card-header' },
            h('span', { className: 'card-title' },
              h('span', { className: `dot ${group.active ? 'dot-active' : 'dot-inactive'}` }),
              projName
            ),
          ),
          h('div', { className: 'card-subtitle' }, projPath),
          h('div', { style: { marginTop: '8px', fontSize: '13px', color: 'var(--text-muted)' } },
            `${libs.length} ${libs.length !== 1 ? 'libraries' : 'library'}, ${totalTables} table${totalTables !== 1 ? 's' : ''}`
          ),
        );
        card.addEventListener('click', () => {
          navigate('project', { path: projPath });
        });
        grid.appendChild(card);
      }
      container.appendChild(grid);
    }

    // Section 2: Discovered Projects (open in KiCad, not registered)
    if (discovered.length > 0) {
      const section = h('div', { style: { marginTop: '24px' } });
      section.appendChild(h('h2', { className: 'section-title' }, 'Open in KiCad'));
      const grid = h('div', { className: 'card-grid' });

      for (const d of discovered) {
        const card = h('div', { className: 'card' },
          h('div', { className: 'card-header' },
            h('span', { className: 'card-title' },
              h('span', { className: 'dot dot-active' }),
              d.name
            ),
          ),
          h('div', { className: 'card-subtitle' }, d.project_path),
          h('div', { style: { marginTop: '10px' } },
            h('button', {
              className: 'btn btn-primary btn-sm',
              onClick: (e) => {
                e.stopPropagation();
                navigate('add-project', { path: d.project_path });
              }
            }, 'Set up KiCodex'),
          ),
        );
        grid.appendChild(card);
      }
      section.appendChild(grid);
      container.appendChild(section);
    }

    // Empty state
    if (projectPaths.length === 0 && discovered.length === 0) {
      container.appendChild(h('div', { className: 'empty' },
        'No projects found. Open a project in KiCad or use "Add Project" to get started.'
      ));
    }
  }
};

async function promptAddProject() {
  try {
    const selected = await window.__TAURI__.dialog.open({
      directory: true,
      title: 'Select KiCad project directory',
    });
    if (selected) {
      navigate('add-project', { path: selected });
    }
  } catch (e) {
    console.error('Failed to open folder picker:', e);
  }
}

async function promptNewLibrary() {
  // Show inline modal instead of alert/prompt
  const overlay = h('div', { className: 'modal-overlay' });
  const modal = h('div', { className: 'modal', style: { width: '400px' } });

  const nameInput = h('input', {
    className: 'form-input',
    placeholder: 'Library name',
    type: 'text',
  });

  let dirPath = null;
  const dirLabel = h('span', {
    style: { fontSize: '13px', color: 'var(--text-muted)' }
  }, 'No directory selected');

  const pickBtn = h('button', {
    className: 'btn btn-sm',
    onClick: async () => {
      const selected = await window.__TAURI__.dialog.open({
        directory: true,
        title: 'Select parent directory for library',
      });
      if (selected) {
        dirPath = selected;
        dirLabel.textContent = selected;
      }
    }
  }, 'Choose Directory');

  const errorDiv = h('div', {
    style: { color: 'var(--error)', fontSize: '13px', minHeight: '20px' }
  });

  const createBtn = h('button', {
    className: 'btn btn-primary',
    onClick: async () => {
      const name = nameInput.value.trim();
      if (!name) { errorDiv.textContent = 'Please enter a library name'; return; }
      if (!dirPath) { errorDiv.textContent = 'Please select a parent directory'; return; }
      try {
        createBtn.disabled = true;
        createBtn.textContent = 'Creating...';
        await invoke('create_library', { name, parentDir: dirPath });
        overlay.remove();
        renderRoute();
      } catch (e) {
        errorDiv.textContent = String(e);
        createBtn.disabled = false;
        createBtn.textContent = 'Create';
      }
    }
  }, 'Create');

  modal.appendChild(h('div', { className: 'modal-header' },
    h('h3', {}, 'New Library'),
    h('button', { className: 'modal-close', onClick: () => overlay.remove() }, '\u00d7'),
  ));
  modal.appendChild(h('div', { style: { padding: '16px', display: 'flex', flexDirection: 'column', gap: '12px' } },
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Library Name'),
      nameInput,
    ),
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Parent Directory'),
      h('div', { className: 'input-with-btn' }, dirLabel, pickBtn),
    ),
    errorDiv,
    h('div', { className: 'btn-group' }, createBtn),
  ));

  overlay.appendChild(modal);
  overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
  document.body.appendChild(overlay);
}
