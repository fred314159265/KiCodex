// Picker modal — two-column KiCad library/entry browser
let _pickerCallback = null;

function openPicker(kind, callback, currentValue) {
  _pickerCallback = callback;
  showPickerModal(kind, currentValue);
}

async function showPickerModal(kind, currentValue) {
  // Remove existing modal
  const existing = document.querySelector('.modal-overlay');
  if (existing) existing.remove();

  let libraries = [];
  try {
    libraries = await invoke('list_kicad_libraries', { kind });
  } catch (e) {
    alert('Could not load KiCad libraries: ' + e);
    return;
  }

  const overlay = h('div', { className: 'modal-overlay' });
  const modal = h('div', { className: 'modal' });

  const header = h('div', { className: 'modal-header' },
    h('h3', {}, `Select ${kind === 'symbol' ? 'Symbol' : 'Footprint'}`),
    h('button', { className: 'modal-close', onClick: () => overlay.remove() }, '\u00d7'),
  );
  modal.appendChild(header);

  const body = h('div', { className: 'modal-body' });
  const columns = h('div', { className: 'picker-columns' });

  // Left column: libraries
  const leftCol = h('div', { className: 'picker-col' });
  leftCol.appendChild(h('div', { className: 'picker-col-header' }, 'Libraries'));
  const leftFilter = h('div', { className: 'picker-filter' });
  const leftInput = h('input', { type: 'text', placeholder: 'Filter... (or /regex/)' });
  leftFilter.appendChild(leftInput);
  leftCol.appendChild(leftFilter);
  const leftList = h('div', { className: 'picker-list' });
  leftCol.appendChild(leftList);

  // Right column: entries
  const rightCol = h('div', { className: 'picker-col' });
  rightCol.appendChild(h('div', { className: 'picker-col-header' }, 'Entries'));
  const rightFilter = h('div', { className: 'picker-filter' });
  const rightInput = h('input', { type: 'text', placeholder: 'Filter... (or /regex/)' });
  rightFilter.appendChild(rightInput);
  rightCol.appendChild(rightFilter);
  const rightList = h('div', { className: 'picker-list' });
  rightCol.appendChild(rightList);

  columns.appendChild(leftCol);
  columns.appendChild(rightCol);
  body.appendChild(columns);
  modal.appendChild(body);
  overlay.appendChild(modal);
  document.body.appendChild(overlay);

  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) overlay.remove();
  });

  let selectedLib = null;
  let entries = [];

  function parseFilter(text) {
    const raw = text || '';
    const m = raw.match(/^\/(.*)\/([gimsuy]*)$/);
    if (m) {
      try {
        const re = new RegExp(m[1], m[2]);
        return { test: s => re.test(s), isRegex: true, isError: false };
      } catch {
        return { test: () => false, isRegex: true, isError: true };
      }
    }
    const f = raw.toLowerCase();
    return { test: s => !f || s.toLowerCase().includes(f), isRegex: false, isError: false };
  }

  function updateInputState(input, filter) {
    input.classList.toggle('regex-active', filter.isRegex && !filter.isError);
    input.classList.toggle('regex-error', filter.isError);
  }

  function renderLibraries(filterText) {
    leftList.innerHTML = '';
    const filter = parseFilter(filterText);
    updateInputState(leftInput, filter);
    for (const lib of libraries) {
      if (!filter.test(lib)) continue;
      const item = h('div', {
        className: `picker-item ${lib === selectedLib ? 'selected' : ''}`,
        onClick: () => selectLibrary(lib),
      }, lib);
      leftList.appendChild(item);
    }
  }

  async function selectLibrary(lib) {
    selectedLib = lib;
    renderLibraries(leftInput.value);
    rightList.innerHTML = '<div class="loading">Loading...</div>';
    try {
      entries = await invoke('list_kicad_entries', { kind, libName: lib });
      renderEntries(rightInput.value);
    } catch (e) {
      rightList.innerHTML = `<div class="empty">${escapeHtml(String(e))}</div>`;
    }
  }

  function renderEntries(filterText) {
    rightList.innerHTML = '';
    const filter = parseFilter(filterText);
    updateInputState(rightInput, filter);
    for (const entry of entries) {
      if (!filter.test(entry)) continue;
      const item = h('div', {
        className: 'picker-item',
        onClick: () => {
          if (_pickerCallback) {
            _pickerCallback(`${selectedLib}:${entry}`);
            _pickerCallback = null;
          }
          overlay.remove();
        },
      }, entry);
      rightList.appendChild(item);
    }
    if (rightList.children.length === 0) {
      rightList.appendChild(h('div', { className: 'empty' }, 'No entries found'));
    }
  }

  leftInput.addEventListener('input', () => renderLibraries(leftInput.value));
  rightInput.addEventListener('input', () => renderEntries(rightInput.value));

  // Pre-select library and entry if a current value like "Device:R" is provided
  const preselect = currentValue ? currentValue.split(':') : null;
  if (preselect && preselect.length === 2 && libraries.includes(preselect[0])) {
    await selectLibrary(preselect[0]);
    // Scroll to and highlight the matching entry
    const targetEntry = preselect[1];
    for (const item of rightList.children) {
      if (item.textContent === targetEntry) {
        item.classList.add('selected');
        item.scrollIntoView({ block: 'nearest' });
        break;
      }
    }
    // Scroll the selected library into view
    for (const item of leftList.children) {
      if (item.textContent === preselect[0]) {
        item.scrollIntoView({ block: 'nearest' });
        break;
      }
    }
  } else {
    renderLibraries('');
  }
}
