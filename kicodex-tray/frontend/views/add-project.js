// Add Project view — setup wizard
const AddProjectView = {
  async render(container, params) {
    const projectPath = params.path;
    if (!projectPath) {
      container.innerHTML = '<div class="empty">No project path specified.</div>';
      return;
    }

    container.innerHTML = '';

    const breadcrumb = h('div', { className: 'breadcrumb' },
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
      h('span', {}, 'Set up Project'),
    );
    container.appendChild(breadcrumb);

    const projName = projectPath.split(/[\\/]/).pop() || projectPath;
    container.appendChild(h('h1', { className: 'page-title', style: { marginBottom: '4px' } }, projName));
    container.appendChild(h('div', { className: 'card-subtitle', style: { marginBottom: '16px' } }, projectPath));

    const content = h('div', { id: 'wizard-content' });
    container.appendChild(content);

    // Start scanning
    await wizardScan(content, projectPath);
  }
};

async function wizardScan(content, projectPath) {
  content.innerHTML = '<div class="loading">Scanning project for libraries...</div>';

  try {
    const result = await invoke('scan_project', { projectPath });

    // If config exists and already registered, auto-navigate to project
    if (result.has_config && result.already_registered) {
      content.innerHTML = '';
      content.appendChild(h('div', { className: 'summary', style: { flexDirection: 'column', gap: '8px' } },
        h('div', {}, 'This project is already registered in KiCodex.'),
        h('button', {
          className: 'btn btn-primary',
          onClick: () => navigate('project', { path: projectPath }),
        }, 'Go to Project'),
      ));
      return;
    }

    // If config exists but not registered, auto-register
    if (result.has_config && !result.already_registered) {
      content.innerHTML = '<div class="loading">Found kicodex.yaml — registering project...</div>';
      try {
        const addResult = await invoke('add_project', {
          projectPath,
          libraries: result.libraries.map(l => ({ ...l, is_new: true })),
        });
        wizardSuccess(content, projectPath, addResult);
      } catch (e) {
        content.innerHTML = '';
        content.appendChild(h('div', { className: 'card', style: { color: 'var(--error)' } },
          h('strong', {}, 'Registration failed: '), String(e),
        ));
      }
      return;
    }

    // Libraries found
    if (result.libraries.length > 0) {
      wizardSelectLibraries(content, projectPath, result.libraries);
      return;
    }

    // No libraries found
    wizardNoLibraries(content, projectPath);
  } catch (e) {
    content.innerHTML = '';
    content.appendChild(h('div', { className: 'card', style: { color: 'var(--error)' } },
      h('strong', {}, 'Scan failed: '), String(e),
    ));
  }
}

function wizardSelectLibraries(content, projectPath, libraries) {
  content.innerHTML = '';

  const newLibs = libraries.filter(l => l.is_new);
  const existingLibs = libraries.filter(l => !l.is_new);

  content.appendChild(h('div', { className: 'summary' },
    h('span', {}, `Found ${libraries.length} ${libraries.length !== 1 ? 'libraries' : 'library'}`),
    newLibs.length > 0
      ? h('span', { className: 'summary-ok' }, `${newLibs.length} new`)
      : null,
    existingLibs.length > 0
      ? h('span', { style: { color: 'var(--text-muted)' } }, `${existingLibs.length} already registered`)
      : null,
  ));

  const checkboxes = {};
  const list = h('div', { className: 'card' });

  for (const lib of libraries) {
    const item = h('div', { className: 'scan-item' });
    if (lib.is_new) {
      const cb = h('input', { type: 'checkbox', checked: 'checked' });
      checkboxes[lib.name] = cb;
      item.appendChild(cb);
    } else {
      item.appendChild(h('span', { style: { width: '16px', display: 'inline-block' } }));
    }
    item.appendChild(h('label', {},
      h('strong', {}, lib.name),
      ` (${lib.path}) `,
      h('span', { className: `badge ${lib.is_new ? 'badge-new' : 'badge-existing'}` },
        lib.is_new ? 'NEW' : 'REGISTERED'),
    ));
    list.appendChild(item);
  }
  content.appendChild(list);

  if (newLibs.length > 0) {
    const btnRow = h('div', { className: 'btn-group', style: { marginTop: '12px' } });
    const addBtn = h('button', { className: 'btn btn-primary' }, 'Add Selected');

    addBtn.addEventListener('click', async () => {
      const selected = libraries.map(l => ({
        name: l.name,
        path: l.path,
        is_new: l.is_new && (checkboxes[l.name]?.checked ?? false),
      }));

      if (!selected.some(l => l.is_new)) {
        return;
      }

      addBtn.disabled = true;
      addBtn.textContent = 'Registering...';

      try {
        const result = await invoke('add_project', { projectPath, libraries: selected });
        wizardSuccess(content, projectPath, result);
      } catch (e) {
        addBtn.disabled = false;
        addBtn.textContent = 'Add Selected';
        content.appendChild(h('div', { style: { color: 'var(--error)', marginTop: '8px', fontSize: '13px' } },
          'Error: ' + String(e),
        ));
      }
    });

    btnRow.appendChild(addBtn);
    content.appendChild(btnRow);
  }
}

function wizardNoLibraries(content, projectPath) {
  content.innerHTML = '';

  content.appendChild(h('div', { className: 'empty', style: { padding: '20px' } },
    'No libraries found in this project directory.'
  ));

  const options = h('div', { style: { display: 'flex', flexDirection: 'column', gap: '12px' } });

  // Option 1: Create new library
  const createCard = h('div', { className: 'card', style: { cursor: 'pointer' } },
    h('div', { className: 'card-title' }, 'Create new library'),
    h('div', { className: 'card-subtitle' }, 'Create a new library structure in this project'),
  );
  createCard.addEventListener('click', () => wizardCreateLibrary(content, projectPath));
  options.appendChild(createCard);

  // Option 2: Add existing library
  const existingCard = h('div', { className: 'card', style: { cursor: 'pointer' } },
    h('div', { className: 'card-title' }, 'Add existing library'),
    h('div', { className: 'card-subtitle' }, 'Browse for a library directory with library.yaml'),
  );
  existingCard.addEventListener('click', () => wizardAddExisting(content, projectPath));
  options.appendChild(existingCard);

  // Option 3: Add from Git
  const gitCard = h('div', { className: 'card', style: { cursor: 'pointer' } },
    h('div', { className: 'card-title' }, 'Add from Git'),
    h('div', { className: 'card-subtitle' }, 'Clone a library from a Git repository (added as submodule if project is a git repo)'),
  );
  gitCard.addEventListener('click', () => wizardAddGit(content, projectPath));
  options.appendChild(gitCard);

  content.appendChild(options);
}

function wizardCreateLibrary(content, projectPath) {
  content.innerHTML = '';

  const nameInput = h('input', { className: 'form-input', placeholder: 'e.g. my-components', type: 'text' });
  const errorDiv = h('div', { style: { color: 'var(--error)', fontSize: '13px', minHeight: '20px' } });

  let parentDir = null;
  const dirLabel = h('span', { style: { fontSize: '13px', color: 'var(--text-muted)' } }, 'No directory selected');
  const pickBtn = h('button', {
    className: 'btn btn-sm',
    onClick: async () => {
      const selected = await window.__TAURI__.dialog.open({
        directory: true,
        defaultPath: projectPath,
        title: 'Select parent directory for the library',
      });
      if (selected) {
        parentDir = selected;
        dirLabel.textContent = selected;
      }
    }
  }, 'Browse...');

  const createBtn = h('button', { className: 'btn btn-primary' }, 'Create & Register');
  const backBtn = h('button', { className: 'btn', onClick: () => wizardNoLibraries(content, projectPath) }, 'Back');

  createBtn.addEventListener('click', async () => {
    const name = nameInput.value.trim();
    if (!name) { errorDiv.textContent = 'Please enter a library name'; return; }
    if (!parentDir) { errorDiv.textContent = 'Please select a parent directory'; return; }

    createBtn.disabled = true;
    createBtn.textContent = 'Creating...';
    errorDiv.textContent = '';

    try {
      const libPath = await invoke('create_library', { name, parentDir });

      // Compute relative path from project to the created library
      const normalizedProject = projectPath.replace(/\\/g, '/').replace(/\/$/, '');
      const normalizedLib = libPath.replace(/\\/g, '/');
      let relPath = normalizedLib;
      if (normalizedLib.startsWith(normalizedProject + '/')) {
        relPath = normalizedLib.slice(normalizedProject.length + 1);
      }

      const libraries = [{ name, path: relPath, is_new: true }];
      const result = await invoke('add_project', { projectPath, libraries });
      wizardSuccess(content, projectPath, result);
    } catch (e) {
      errorDiv.textContent = String(e);
      createBtn.disabled = false;
      createBtn.textContent = 'Create & Register';
    }
  });

  content.appendChild(h('div', { className: 'card' },
    h('h3', { style: { marginBottom: '12px' } }, 'Create New Library'),
    h('div', { className: 'card-subtitle', style: { marginBottom: '12px' } },
      'Choose where to create the library directory. A new folder with the library name will be created inside it.',
    ),
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Library Name'),
      nameInput,
    ),
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Parent Directory'),
      h('div', { className: 'input-with-btn' }, dirLabel, pickBtn),
    ),
    errorDiv,
    h('div', { className: 'btn-group' }, backBtn, createBtn),
  ));
}

function wizardAddExisting(content, projectPath) {
  content.innerHTML = '';

  let libDir = null;
  const dirLabel = h('span', { style: { fontSize: '13px', color: 'var(--text-muted)' } }, 'No directory selected');
  const useRelative = h('input', { type: 'checkbox', checked: 'checked' });
  const errorDiv = h('div', { style: { color: 'var(--error)', fontSize: '13px', minHeight: '20px' } });

  const pickBtn = h('button', {
    className: 'btn',
    onClick: async () => {
      const selected = await window.__TAURI__.dialog.open({
        directory: true,
        title: 'Select library directory (contains library.yaml)',
      });
      if (selected) {
        libDir = selected;
        dirLabel.textContent = selected;
      }
    }
  }, 'Browse...');

  const addBtn = h('button', { className: 'btn btn-primary' }, 'Add & Register');
  const backBtn = h('button', { className: 'btn', onClick: () => wizardNoLibraries(content, projectPath) }, 'Back');

  addBtn.addEventListener('click', async () => {
    if (!libDir) { errorDiv.textContent = 'Please select a library directory'; return; }

    addBtn.disabled = true;
    addBtn.textContent = 'Registering...';
    errorDiv.textContent = '';

    try {
      // Determine path (relative or absolute)
      let libPath = libDir;
      if (useRelative.checked) {
        // Try to make relative to project
        libPath = libDir.replace(projectPath.replace(/\\/g, '/'), '').replace(/^[\\/]/, '');
        if (libPath === libDir) {
          // Couldn't make relative, use absolute
          libPath = libDir;
        }
      }

      // Derive name from directory
      const name = libDir.split(/[\\/]/).pop() || 'library';
      const libraries = [{ name, path: libPath, is_new: true }];
      const result = await invoke('add_project', { projectPath, libraries });
      wizardSuccess(content, projectPath, result);
    } catch (e) {
      errorDiv.textContent = String(e);
      addBtn.disabled = false;
      addBtn.textContent = 'Add & Register';
    }
  });

  content.appendChild(h('div', { className: 'card' },
    h('h3', { style: { marginBottom: '12px' } }, 'Add Existing Library'),
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Library Directory'),
      h('div', { className: 'input-with-btn' }, dirLabel, pickBtn),
    ),
    h('div', { className: 'form-group' },
      h('label', { style: { display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px', cursor: 'pointer' } },
        useRelative,
        'Use relative path (recommended for in-tree libraries)',
      ),
    ),
    errorDiv,
    h('div', { className: 'btn-group' }, backBtn, addBtn),
  ));
}

function wizardAddGit(content, projectPath) {
  content.innerHTML = '';

  const urlInput = h('input', { className: 'form-input', placeholder: 'https://github.com/user/repo.git', type: 'text' });
  const nameInput = h('input', { className: 'form-input', placeholder: 'e.g. shared-libs', type: 'text' });
  const errorDiv = h('div', { style: { color: 'var(--error)', fontSize: '13px', minHeight: '20px' } });

  let targetDir = null;
  const dirLabel = h('span', { style: { fontSize: '13px', color: 'var(--text-muted)' } }, 'No directory selected');
  const pickBtn = h('button', {
    className: 'btn btn-sm',
    onClick: async () => {
      const selected = await window.__TAURI__.dialog.open({
        directory: true,
        defaultPath: projectPath,
        title: 'Select directory to clone into (a subfolder with the library name will be created)',
      });
      if (selected) {
        targetDir = selected;
        dirLabel.textContent = selected;
      }
    }
  }, 'Browse...');

  const cloneBtn = h('button', { className: 'btn btn-primary' }, 'Clone & Register');
  const backBtn = h('button', { className: 'btn', onClick: () => wizardNoLibraries(content, projectPath) }, 'Back');

  cloneBtn.addEventListener('click', async () => {
    const gitUrl = urlInput.value.trim();
    const name = nameInput.value.trim();
    if (!gitUrl) { errorDiv.textContent = 'Please enter a Git URL'; return; }
    if (!name) { errorDiv.textContent = 'Please enter a library name'; return; }
    if (!targetDir) { errorDiv.textContent = 'Please select a target directory'; return; }

    cloneBtn.disabled = true;
    cloneBtn.textContent = 'Cloning...';
    errorDiv.textContent = '';

    try {
      const libPath = await invoke('add_git_library', { projectPath, gitUrl: gitUrl, name, targetDir });

      // Compute relative path from project to cloned library
      const normalizedProject = projectPath.replace(/\\/g, '/').replace(/\/$/, '');
      const normalizedLib = libPath.replace(/\\/g, '/');
      let relPath = normalizedLib;
      if (normalizedLib.startsWith(normalizedProject + '/')) {
        relPath = normalizedLib.slice(normalizedProject.length + 1);
      }

      const libraries = [{ name, path: relPath, is_new: true }];
      const result = await invoke('add_project', { projectPath, libraries });
      wizardSuccess(content, projectPath, result);
    } catch (e) {
      errorDiv.textContent = String(e);
      cloneBtn.disabled = false;
      cloneBtn.textContent = 'Clone & Register';
    }
  });

  content.appendChild(h('div', { className: 'card' },
    h('h3', { style: { marginBottom: '12px' } }, 'Add Library from Git'),
    h('div', { className: 'card-subtitle', style: { marginBottom: '12px' } },
      'If this project is a Git repo, the library will be added as a submodule. Otherwise it will be cloned.',
    ),
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Git URL'),
      urlInput,
    ),
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Library Name'),
      nameInput,
    ),
    h('div', { className: 'form-group' },
      h('label', { className: 'form-label' }, 'Clone Into Directory'),
      h('div', { className: 'input-with-btn' }, dirLabel, pickBtn),
    ),
    errorDiv,
    h('div', { className: 'btn-group' }, backBtn, cloneBtn),
  ));
}

function wizardSuccess(content, projectPath, result) {
  content.innerHTML = '';

  const card = h('div', { className: 'card' });
  card.appendChild(h('div', {
    className: 'summary',
    style: { background: 'var(--success-bg)', color: 'var(--success)', marginBottom: '12px' }
  },
    `Registered ${result.registered_count} ${result.registered_count !== 1 ? 'libraries' : 'library'} successfully!`,
  ));

  if (result.httplib_paths.length > 0) {
    card.appendChild(h('div', { style: { marginBottom: '12px' } },
      h('strong', {}, 'Add these files to your KiCad project libraries:'),
    ));
    const pathList = h('div', { style: { fontFamily: 'monospace', fontSize: '12px', background: 'var(--bg-secondary)', padding: '10px', borderRadius: '4px' } });
    for (const p of result.httplib_paths) {
      pathList.appendChild(h('div', { style: { marginBottom: '4px' } }, p));
    }
    card.appendChild(pathList);
  }

  card.appendChild(h('div', { className: 'btn-group', style: { marginTop: '16px' } },
    h('button', {
      className: 'btn btn-primary',
      onClick: () => navigate('project', { path: projectPath }),
    }, 'Go to Project'),
    h('button', {
      className: 'btn',
      onClick: () => navigate('dashboard'),
    }, 'Back to Dashboard'),
  ));

  content.appendChild(card);
}
