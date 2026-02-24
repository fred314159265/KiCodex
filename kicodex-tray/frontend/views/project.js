// Project view — library detail with part tables
const ProjectView = {
  async render(container, params) {
    const projectPath = params.path;
    if (!projectPath) { navigate('dashboard'); return; }

    const libraries = await invoke('get_project_libraries', { projectPath });
    const registeredNames = new Set(
      await invoke('get_kicad_lib_table_names', { projectPath }).catch(() => [])
    );

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

      const btnGroup = h('div', { className: 'btn-group' });
      btnGroup.appendChild(h('button', { className: 'btn', onClick: () => doValidate(lib, projectPath) }, 'Validate'));
      btnGroup.appendChild(h('button', { className: 'btn', onClick: () => doAddPartTable(lib, projectPath) }, 'Add Part Table'));
      if (!registeredNames.has(lib.name)) {
        const regBtn = h('button', { className: 'btn' }, 'Register in KiCad');
        regBtn.addEventListener('click', async () => {
          regBtn.disabled = true;
          try {
            await invoke('register_in_kicad_lib_table', { projectPath, libraryNames: [lib.name] });
            regBtn.remove();
          } catch (e) {
            regBtn.disabled = false;
            alert('Error: ' + e);
          }
        });
        btnGroup.appendChild(regBtn);
      }
      btnGroup.appendChild(h('button', { className: 'btn btn-danger', onClick: () => doUnlinkLibrary(lib, projectPath) }, 'Unlink'));
      btnGroup.appendChild(h('button', { className: 'btn btn-danger', onClick: () => doDeleteLibrary(lib, projectPath) }, 'Delete Library'));

      const header = h('div', { className: 'page-header' },
        h('div', {},
          h('h2', { className: 'page-title' }, lib.name),
          lib.description ? h('div', { className: 'card-subtitle' }, lib.description) : null,
        ),
        btnGroup,
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
              h('div', { className: 'btn-group' },
                h('button', {
                  className: 'btn btn-sm',
                  onClick: () => navigate('template-editor', { lib: lib.path, template: ct.template_name, project: projectPath })
                }, 'Template'),
                h('button', {
                  className: 'btn btn-sm btn-danger',
                  onClick: () => doDeletePartTable(lib.path, ct.name, projectPath)
                }, 'Delete'),
              ),
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

async function doUnlinkLibrary(lib, projectPath) {
  const yes = await window.__TAURI__.dialog.confirm(
    `Remove library "${lib.name}" from this project? The library files will be kept on disk.`,
    { title: 'Unlink Library', kind: 'warning' },
  );
  if (!yes) return;

  const registeredNames = await invoke('get_kicad_lib_table_names', { projectPath }).catch(() => []);
  let removeFromLibTable = false;
  if (registeredNames.includes(lib.name)) {
    removeFromLibTable = await window.__TAURI__.dialog.confirm(
      `Also remove "${lib.name}" from KiCad's sym-lib-table?`,
      { title: 'Update sym-lib-table', kind: 'info' },
    );
  }

  try {
    await invoke('remove_library', { projectPath, libraryPath: lib.path });
    if (removeFromLibTable) {
      await invoke('unregister_from_kicad_lib_table', { projectPath, libraryName: lib.name });
    }
    renderRoute();
  } catch (e) {
    alert('Error: ' + e);
  }
}

async function doDeleteLibrary(lib, projectPath) {
  const yes = await window.__TAURI__.dialog.confirm(
    `Permanently delete library "${lib.name}"? This will remove it from the project AND delete all files on disk. This cannot be undone.`,
    { title: 'Delete Library', kind: 'warning' },
  );
  if (!yes) return;
  try {
    await invoke('delete_library', { projectPath, libraryPath: lib.path });
    renderRoute();
  } catch (e) {
    alert('Error: ' + e);
  }
}

async function doDeletePartTable(libPath, name, projectPath) {
  const yes = await window.__TAURI__.dialog.confirm(`Delete part table "${name}"? This will remove its data file and template.`, { title: 'Delete Part Table', kind: 'warning' });
  if (!yes) return;
  try {
    await invoke('delete_part_table', { libPath, partTableName: name });
    renderRoute();
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
        // Show success state with Register option
        const registeredNames = await invoke('get_kicad_lib_table_names', { projectPath }).catch(() => []);
        body.innerHTML = '';
        body.appendChild(h('div', { style: { fontSize: '13px', color: 'var(--success)', marginBottom: '8px' } },
          `Library "${name}" created.`));
        if (!registeredNames.includes(name)) {
          const feedbackDiv = h('div', { style: { fontSize: '12px', minHeight: '16px' } });
          const regBtn = h('button', { className: 'btn btn-sm' }, 'Register in KiCad sym-lib-table');
          regBtn.addEventListener('click', async () => {
            regBtn.disabled = true;
            try {
              await invoke('register_in_kicad_lib_table', { projectPath, libraryNames: [name] });
              feedbackDiv.style.color = 'var(--success)';
              feedbackDiv.textContent = 'Registered in sym-lib-table.';
              regBtn.style.display = 'none';
            } catch (e) {
              regBtn.disabled = false;
              feedbackDiv.style.color = 'var(--error)';
              feedbackDiv.textContent = String(e);
            }
          });
          body.appendChild(regBtn);
          body.appendChild(feedbackDiv);
        }
        const closeBtn = h('button', { className: 'btn btn-primary', onClick: () => { overlay.remove(); renderRoute(); } }, 'Done');
        body.appendChild(h('div', { className: 'btn-group', style: { marginTop: '8px' } }, closeBtn));
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
  const body = h('div', { style: { padding: '16px', display: 'flex', flexDirection: 'column', gap: '12px' } },
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Library Name'),
      nameInput,
      errorDiv,
    ),
    h('div', { style: { fontSize: '13px', color: 'var(--text-muted)' } },
      'Will be created in: ' + projectPath,
    ),
    h('div', { className: 'btn-group' }, createBtn),
  );
  modal.appendChild(body);

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
          // Show registration option
          const names = selected.map(l => l.name);
          const registeredNames = new Set(
            await invoke('get_kicad_lib_table_names', { projectPath }).catch(() => [])
          );
          const unregisteredNames = names.filter(n => !registeredNames.has(n));
          const closeBtn = h('button', { className: 'btn btn-primary', onClick: () => { overlay.remove(); renderRoute(); } }, 'Done');
          confirmBtn.style.display = 'none';
          const addedCount = selected.length;
          modalBody.appendChild(h('div', { style: { fontSize: '13px', color: 'var(--success)' } },
            `Added ${addedCount} ${addedCount === 1 ? 'library' : 'libraries'}.`));
          if (unregisteredNames.length > 0) {
            const feedbackDiv = h('div', { style: { fontSize: '12px', minHeight: '16px' } });
            const regBtn = h('button', { className: 'btn btn-sm' }, 'Register in KiCad sym-lib-table');
            regBtn.addEventListener('click', async () => {
              regBtn.disabled = true;
              try {
                const count = await invoke('register_in_kicad_lib_table', { projectPath, libraryNames: unregisteredNames });
                feedbackDiv.style.color = 'var(--success)';
                feedbackDiv.textContent = `Registered ${count} ${count === 1 ? 'library' : 'libraries'} in sym-lib-table.`;
                regBtn.style.display = 'none';
              } catch (e) {
                regBtn.disabled = false;
                feedbackDiv.style.color = 'var(--error)';
                feedbackDiv.textContent = String(e);
              }
            });
            modalBody.appendChild(regBtn);
            modalBody.appendChild(feedbackDiv);
          }
          modalBody.appendChild(h('div', { className: 'btn-group' }, closeBtn));
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
    const modalBody = h('div', { style: { padding: '16px', display: 'flex', flexDirection: 'column', gap: '12px' } },
      listDiv,
      h('div', { className: 'btn-group' }, confirmBtn),
    );
    modal.appendChild(modalBody);

    overlay.appendChild(modal);
    overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
    document.body.appendChild(overlay);
  } catch (e) {
    alert('Error scanning: ' + e);
  }
}
