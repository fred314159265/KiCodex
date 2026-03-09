// Part table editor view — jspreadsheet CE powered data grid
// Self-contained: includes all helpers needed (invoke, navigate, h, openPicker)
// Navigation guard is exposed via window.__legacyNavGuard for App.svelte to check.

(function () {
  function ensureUrlCellStyles() {
    if (document.getElementById('pte-url-cell-styles')) return;
    const style = document.createElement('style');
    style.id = 'pte-url-cell-styles';
    style.textContent = 'body.pte-ctrl-down .pte-url-cell:hover { cursor: pointer !important; text-decoration: underline; }';
    document.head.appendChild(style);
    document.addEventListener('keydown', (ev) => { if (ev.key === 'Control') document.body.classList.add('pte-ctrl-down'); });
    document.addEventListener('keyup',   (ev) => { if (ev.key === 'Control') document.body.classList.remove('pte-ctrl-down'); });
    window.addEventListener('blur', () => document.body.classList.remove('pte-ctrl-down'));
  }

  function getInvoke() {
    if (window.__TAURI__ && window.__TAURI__.core) return window.__TAURI__.core.invoke;
    throw new Error('Tauri IPC not available');
  }

  function navigate(view, params) {
    const qs = Object.entries(params || {})
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    window.location.hash = qs ? `${view}?${qs}` : view;
  }

  function h(tag, attrs, ...children) {
    const el = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (k === 'className') el.className = v;
      else if (k === 'style' && typeof v === 'object') Object.assign(el.style, v);
      else if (k.startsWith('on') && typeof v === 'function') {
        const event = k === 'onMousedown' ? 'mousedown' : k.slice(2).toLowerCase();
        el.addEventListener(event, v);
      }
      else el.setAttribute(k, v);
    }
    for (const child of children) {
      if (typeof child === 'string') el.appendChild(document.createTextNode(child));
      else if (child) el.appendChild(child);
    }
    return el;
  }

  function escapeHtml(s) {
    const el = document.createElement('span');
    el.textContent = s;
    return el.innerHTML;
  }

  function openPicker(kind, callback, currentValue) {
    const invoke = getInvoke();
    const existing = document.querySelector('.pte-picker-overlay');
    if (existing) existing.remove();

    const overlay = h('div', { className: 'pte-picker-overlay' });
    Object.assign(overlay.style, {
      position: 'fixed', inset: '0', background: 'rgba(0,0,0,0.5)',
      display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: '9999',
    });

    const modal = h('div', {});
    Object.assign(modal.style, {
      background: 'var(--b1, #fff)', borderRadius: '8px', width: '700px', height: '500px',
      display: 'flex', flexDirection: 'column', overflow: 'hidden', boxShadow: '0 8px 32px rgba(0,0,0,0.25)',
    });

    const modalHeader = h('div', {});
    Object.assign(modalHeader.style, {
      padding: '12px 16px', borderBottom: '1px solid #e5e7eb',
      display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    });
    const closeBtn = h('button', { onClick: () => overlay.remove() }, '✕');
    closeBtn.style.cssText = 'background:none;border:none;cursor:pointer;font-size:18px;';
    modalHeader.appendChild(h('span', {}, `Select ${kind === 'symbol' ? 'Symbol' : 'Footprint'}`));
    modalHeader.appendChild(closeBtn);
    modal.appendChild(modalHeader);

    const cols = h('div', {});
    Object.assign(cols.style, { display: 'flex', flex: '1', overflow: 'hidden' });

    function makeCol(title) {
      const col = h('div', {});
      Object.assign(col.style, { display: 'flex', flexDirection: 'column', flex: '1', borderRight: '1px solid #e5e7eb', overflow: 'hidden' });
      const colHead = h('div', {}, title);
      Object.assign(colHead.style, { padding: '8px 12px', fontWeight: '600', fontSize: '13px', borderBottom: '1px solid #e5e7eb' });
      col.appendChild(colHead);
      const filterInput = h('input', { type: 'text', placeholder: 'Filter... (or /regex/)' });
      Object.assign(filterInput.style, { margin: '6px', padding: '4px 8px', border: '1px solid #d1d5db', borderRadius: '4px', fontSize: '13px' });
      col.appendChild(filterInput);
      const list = h('div', {});
      Object.assign(list.style, { flex: '1', overflowY: 'auto' });
      col.appendChild(list);
      return { col, filterInput, list };
    }

    const left = makeCol('Libraries');
    const right = makeCol('Entries');
    cols.appendChild(left.col);
    cols.appendChild(right.col);
    modal.appendChild(cols);
    overlay.appendChild(modal);
    overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
    document.body.appendChild(overlay);

    let libraries = [];
    let entries = [];
    let selectedLib = null;

    function parseFilter(text) {
      const m = (text || '').match(/^\/(.*)\/([gimsuy]*)$/);
      if (m) {
        try { const re = new RegExp(m[1], m[2]); return s => re.test(s); } catch { return () => false; }
      }
      const f = (text || '').toLowerCase();
      return s => !f || s.toLowerCase().includes(f);
    }

    function renderLibs(filter) {
      left.list.innerHTML = '';
      const test = parseFilter(filter);
      let selEl = null;
      for (const lib of libraries) {
        if (!test(lib)) continue;
        const isSel = lib === selectedLib;
        const item = h('div', { onClick: () => selectLib(lib) }, lib);
        Object.assign(item.style, { padding: '4px 12px', cursor: 'pointer', fontSize: '13px', background: isSel ? '#570df8' : '', color: isSel ? '#fff' : '' });
        left.list.appendChild(item);
        if (isSel) selEl = item;
      }
      if (selEl) selEl.scrollIntoView({ block: 'start' });
    }

    async function selectLib(lib) {
      selectedLib = lib;
      renderLibs(left.filterInput.value);
      right.list.innerHTML = '<div style="padding:8px 12px;font-size:13px;color:#6b7280;">Loading...</div>';
      try {
        entries = await invoke('list_kicad_entries', { kind, libName: lib });
        renderEntries(right.filterInput.value);
      } catch (e) {
        right.list.innerHTML = `<div style="padding:8px 12px;font-size:13px;color:#ef4444;">${escapeHtml(String(e))}</div>`;
      }
    }

    function renderEntries(filter) {
      right.list.innerHTML = '';
      const test = parseFilter(filter);
      let selEl = null;
      for (const entry of entries) {
        if (!test(entry)) continue;
        const isSel = currentValue === `${selectedLib}:${entry}`;
        const item = h('div', { onClick: () => { callback(`${selectedLib}:${entry}`); overlay.remove(); } }, entry);
        Object.assign(item.style, { padding: '4px 12px', cursor: 'pointer', fontSize: '13px', background: isSel ? '#570df8' : '', color: isSel ? '#fff' : '' });
        right.list.appendChild(item);
        if (isSel) selEl = item;
      }
      if (!right.list.children.length) right.list.innerHTML = '<div style="padding:8px 12px;font-size:13px;color:#6b7280;">No entries found</div>';
      if (selEl) selEl.scrollIntoView({ block: 'start' });
    }

    left.filterInput.addEventListener('input', () => renderLibs(left.filterInput.value));
    right.filterInput.addEventListener('input', () => renderEntries(right.filterInput.value));

    invoke('list_kicad_libraries', { kind }).then(libs => {
      libraries = libs;
      const preselect = currentValue ? currentValue.split(':') : null;
      if (preselect && preselect.length === 2 && libs.includes(preselect[0])) selectLib(preselect[0]);
      else renderLibs('');
    }).catch(e => {
      left.list.innerHTML = `<div style="padding:8px 12px;color:#ef4444;">${escapeHtml(String(e))}</div>`;
    });
  }

  window.PartTableEditorView = {
    async render(container, params) {
      const invoke = getInvoke();
      const libPath = params.lib;
      const componentTypeName = params.type;
      const projectPath = params.project;

      if (!libPath || !componentTypeName) { navigate('dashboard'); return; }

      window.__legacyNavGuard = null;

      document.getElementById('app').classList.add('view-wide');

      const data = await invoke('get_part_table_data', { libPath, componentTypeName });
      container.innerHTML = '';

      const bcParts = [h('a', { href: '#dashboard' }, 'Dashboard'), h('span', {}, ' / ')];
      if (projectPath) {
        bcParts.push(h('a', { href: `#project?path=${encodeURIComponent(projectPath)}` }, projectPath.split(/[\\/]/).pop()));
      } else {
        const libName = libPath.split(/[\\/]/).pop() || libPath;
        bcParts.push(h('a', { href: `#library?path=${encodeURIComponent(libPath)}` }, libName));
      }
      bcParts.push(h('span', {}, ' / '));
      bcParts.push(h('span', {}, data.name));
      container.appendChild(h('div', { className: 'breadcrumb' }, ...bcParts));

      const fields = data.template.fields;
      const checkboxKeys = ['exclude_from_bom', 'exclude_from_board', 'exclude_from_sim'];
      const rowIds = data.components.map(c => c.id);
      const dirtyRows = new Set();

      const columns = [];
      const requiredColIndices = new Set();
      columns.push({ title: 'ID', name: 'id', type: 'text', readOnly: true, width: 80 });

      for (const f of fields) {
        if (f.key === 'id') continue;
        if (checkboxKeys.includes(f.key)) continue;
        const col = { title: f.display_name, name: f.key, type: 'text', width: 140 };
        if (f.required) { requiredColIndices.add(columns.length); col.title += ' *'; }
        if (f.field_type === 'url') {
          ensureUrlCellStyles();
          col.render = (td, value) => {
            td.innerText = value || '';
            if (value && (value.startsWith('http://') || value.startsWith('https://'))) {
              td.classList.add('pte-url-cell');
              td.title = 'Ctrl+click to open';
              td.addEventListener('mousedown', (ev) => {
                if (ev.ctrlKey) {
                  ev.preventDefault(); ev.stopPropagation();
                  invoke('open_url', { url: value });
                }
              });
            } else {
              td.classList.remove('pte-url-cell');
              td.title = '';
            }
          };
        } else if (f.field_type === 'kicad_symbol' || f.field_type === 'kicad_footprint') {
          col._kicadKind = f.field_type === 'kicad_symbol' ? 'symbol' : 'footprint';
          col.render = (td, value, x, y) => {
            td.innerText = value || '';
            td.style.position = 'relative';
            const btn = document.createElement('button');
            btn.className = 'btn btn-xs btn-ghost';
            btn.textContent = '…';
            btn.style.cssText = 'position:absolute;right:2px;top:50%;transform:translateY(-50%);min-height:0;height:20px;padding:0 4px;line-height:1;';
            btn.addEventListener('mousedown', (ev) => {
              ev.preventDefault(); ev.stopPropagation();
              openPicker(col._kicadKind, (val) => { ws.setValueFromCoords(x, y, val); }, value || '');
            });
            td.appendChild(btn);
          };
        }
        columns.push(col);
      }

      columns.push({ title: 'BOM', name: 'exclude_from_bom', type: 'checkbox', width: 55 });
      columns.push({ title: 'Board', name: 'exclude_from_board', type: 'checkbox', width: 60 });
      columns.push({ title: 'Sim', name: 'exclude_from_sim', type: 'checkbox', width: 55 });

      const ROW_HEADER_WIDTH = 50;
      const availableWidth = container.parentElement.clientWidth - ROW_HEADER_WIDTH;
      const fixedTotal = columns.reduce((sum, c) => sum + (c.type === 'checkbox' || c.name === 'id' ? c.width : 0), 0);
      const flexCols = columns.filter(c => c.type !== 'checkbox' && c.name !== 'id');
      if (flexCols.length > 0) {
        const flexWidth = Math.max(140, Math.floor((availableWidth - fixedTotal) / flexCols.length));
        flexCols.forEach(c => c.width = flexWidth);
      }

      const rows = data.components.map(comp => columns.map(col => {
        if (col.type === 'checkbox') return comp[col.name] === 'true';
        return comp[col.name] || '';
      }));

      const fieldNameToColIdx = new Map(columns.map((col, i) => [col.name, i]));

      // Snapshot of each row's values as last saved to disk (null = unsaved new row).
      // Used by resyncDirtyRows() after undo/redo to recompute dirty state accurately.
      const savedData = rows.map(r => [...r]);

      const saveBtn = h('button', { className: 'btn', onClick: saveAll }, 'Save');
      saveBtn.disabled = true;

      const validateBtn = h('button', { className: 'btn', onClick: runServerValidation }, 'Validate');

      const header = h('div', { className: 'flex flex-wrap items-center justify-between gap-4 mb-3' },
        h('h2', { className: 'text-xl font-bold' }, `${data.name} (${data.components.length} components)`),
        h('div', { className: 'flex flex-wrap gap-2' },
          saveBtn,
          validateBtn,
          h('button', { className: 'btn', onClick: () => { ws.insertRow(); } }, 'Add Row'),
          h('button', { className: 'btn', onClick: () => {
            navigate('component-form', { lib: libPath, type: componentTypeName, project: projectPath, mode: 'add' });
          } }, 'Add in Form'),
          h('button', { className: 'btn btn-error', onMousedown: e => { e.preventDefault(); e.stopPropagation(); }, onClick: () => { deleteSelectedRows(); } }, 'Delete Selected'),
          h('button', { className: 'btn', onClick: () => {
            navigate('template-editor', { lib: libPath, template: data.template_name, project: projectPath });
          } }, 'Edit Template'),
        ),
      );
      container.appendChild(header);

      const errorDiv = h('div', { className: 'field-error-msg' });
      errorDiv.style.display = 'none';
      container.appendChild(errorDiv);

      if (data.components.length === 0) {
        container.appendChild(h('div', { className: 'empty', id: 'empty-msg' }, 'No components yet. Click Add Row to create one.'));
      }

      const wrapper = document.createElement('div');
      wrapper.id = 'grid';
      container.appendChild(wrapper);

      const spreadsheet = jspreadsheet(wrapper, {
        worksheets: [{
          columns,
          data: rows.length > 0 ? rows : [[]],
          minDimensions: [columns.length, 0],
          allowInsertColumn: false, allowDeleteColumn: false, allowRenameColumn: false,
          columnDrag: false, tableOverflow: true, tableWidth: '100%', tableHeight: 'calc(100vh - 112px)',
        }],
        onafterchanges: handleChanges,
        onbeforeinsertrow: handleBeforeInsertRow,
        oninsertrow: handleInsertRow,
        onbeforedeleterow: handleBeforeDeleteRow,
        oncopy: handleCopy,
        onundo: () => resyncDirtyRows(),
        onredo: () => resyncDirtyRows(),
      });

      const ws = spreadsheet[0];
      if (rows.length === 0) rowIds.push(null);
      dirtyRows.clear();

      const highlightedCells = new Set();
      const serverValidationCells = new Set();
      let _saving = false;

      function updateTitle() {
        header.querySelector('h2').textContent = `${data.name} (${rowIds.length} components)`;
      }

      function updateSaveButtonState() {
        if (dirtyRows.size > 0) {
          saveBtn.disabled = false; saveBtn.className = 'btn btn-primary'; saveBtn.textContent = 'Save';
        } else {
          saveBtn.disabled = true; saveBtn.className = 'btn'; saveBtn.textContent = 'Save';
        }
      }

      function shiftDirtyRows(startIdx, delta) {
        const updated = new Set();
        for (const idx of dirtyRows) {
          if (idx >= startIdx) { const s = idx + delta; if (s >= 0) updated.add(s); }
          else updated.add(idx);
        }
        dirtyRows.clear();
        for (const idx of updated) dirtyRows.add(idx);
      }

      function validateDirtyRows() {
        const errors = [];
        for (const rowIdx of dirtyRows) {
          if (rowIdx < 0 || rowIdx >= rowIds.length) continue;
          const rowData = ws.getRowData(rowIdx);
          for (const colIdx of requiredColIndices) {
            const val = rowData[colIdx];
            if (val === null || val === undefined || String(val).trim() === '') {
              errors.push({ rowIdx, colIdx, message: `Row ${rowIdx + 1}: "${columns[colIdx].title.replace(' *', '')}" is required` });
            }
          }
        }
        return errors;
      }

      function clearValidationHighlights() {
        for (const key of highlightedCells) {
          const [colIdx, rowIdx] = key.split(',').map(Number);
          const cell = ws.getCellFromCoords(colIdx, rowIdx);
          if (cell) cell.style.outline = '';
        }
        highlightedCells.clear();
        errorDiv.style.display = 'none'; errorDiv.textContent = '';
      }

      function showValidationErrors(errors) {
        clearValidationHighlights();
        for (const err of errors) {
          const cell = ws.getCellFromCoords(err.colIdx, err.rowIdx);
          if (cell) cell.style.outline = '2px solid red';
          highlightedCells.add(`${err.colIdx},${err.rowIdx}`);
        }
        errorDiv.textContent = errors.map(e => e.message).join(' | ');
        errorDiv.style.display = '';
      }

      function clearServerValidation() {
        for (const key of serverValidationCells) {
          const [colIdx, rowIdx] = key.split(',').map(Number);
          const cell = ws.getCellFromCoords(colIdx, rowIdx);
          if (cell) { cell.style.backgroundColor = ''; cell.title = ''; }
        }
        serverValidationCells.clear();
        validateBtn.textContent = 'Validate';
        validateBtn.className = 'btn';
      }

      async function runServerValidation() {
        clearServerValidation();
        validateBtn.textContent = 'Validating…';
        validateBtn.disabled = true;
        try {
          const result = await invoke('validate_library', { libPath, projectPath: projectPath || null });
          const tableResult = result.part_tables.find(t => t.name === componentTypeName);
          if (!tableResult) return;

          // Merge errors and warnings into a cell→messages map
          const cellMsgs = new Map(); // "colIdx,rowIdx" -> { isError, messages[] }
          function addIssue(issue, isError) {
            if (!issue.row) return; // table-level issues (missing column) — skip cell highlight
            const rowIdx = issue.row - 1;
            const colIdx = fieldNameToColIdx.get(issue.field);
            if (colIdx === undefined) return;
            const key = `${colIdx},${rowIdx}`;
            if (!cellMsgs.has(key)) cellMsgs.set(key, { isError, messages: [] });
            const entry = cellMsgs.get(key);
            if (isError) entry.isError = true;
            entry.messages.push(issue.message);
          }
          for (const issue of tableResult.errors)   addIssue(issue, true);
          for (const issue of tableResult.warnings) addIssue(issue, false);

          for (const [key, { isError, messages }] of cellMsgs) {
            const [colIdx, rowIdx] = key.split(',').map(Number);
            const cell = ws.getCellFromCoords(colIdx, rowIdx);
            if (cell) {
              cell.style.backgroundColor = isError ? 'rgba(239,68,68,0.25)' : 'rgba(234,179,8,0.25)';
              cell.title = messages.join('\n');
            }
            serverValidationCells.add(key);
          }

          const errCount  = tableResult.errors.length;
          const warnCount = tableResult.warnings.length;
          if (errCount === 0 && warnCount === 0) {
            validateBtn.textContent = '✓ Valid';
            validateBtn.className = 'btn btn-success';
          } else {
            const parts = [];
            if (errCount)  parts.push(`${errCount} error${errCount  !== 1 ? 's' : ''}`);
            if (warnCount) parts.push(`${warnCount} warning${warnCount !== 1 ? 's' : ''}`);
            validateBtn.textContent = parts.join(', ');
            validateBtn.className = errCount ? 'btn btn-error' : 'btn btn-warning';
          }
        } catch (e) {
          console.error('Validation error:', e);
          validateBtn.textContent = 'Validate';
        } finally {
          validateBtn.disabled = false;
        }
      }

      function resyncDirtyRows() {
        dirtyRows.clear();
        const gridData = ws.getData();
        for (let rowIdx = 0; rowIdx < gridData.length; rowIdx++) {
          const saved = savedData[rowIdx];
          if (!saved) {
            if (!isRowEmpty(rowIdx)) dirtyRows.add(rowIdx);
            continue;
          }
          const current = gridData[rowIdx];
          const differs = columns.some((col, colIdx) => String(current[colIdx] ?? '') !== String(saved[colIdx] ?? ''));
          if (differs) dirtyRows.add(rowIdx);
        }
        updateSaveButtonState();
      }

      function isRowEmpty(rowIdx) {
        const rowData = ws.getRowData(rowIdx);
        for (let c = 1; c < columns.length; c++) {
          if (columns[c].type === 'checkbox') continue;
          const val = rowData[c];
          if (val !== null && val !== undefined && String(val).trim() !== '') return false;
        }
        return true;
      }

      function ensureRowIdsSync() {
        const gridRows = ws.getData().length;
        while (rowIds.length < gridRows) rowIds.push(null);
        while (rowIds.length > gridRows) rowIds.pop();
      }

      function markUntrackedRowsAsDirty() {
        for (let i = 0; i < rowIds.length; i++) {
          if (!rowIds[i] && !isRowEmpty(i)) dirtyRows.add(i);
        }
      }

      async function saveAll() {
        ensureRowIdsSync();
        markUntrackedRowsAsDirty();
        const emptyNewRows = [];
        for (const rowIdx of dirtyRows) {
          if (rowIdx >= 0 && rowIdx < rowIds.length && !rowIds[rowIdx] && isRowEmpty(rowIdx)) emptyNewRows.push(rowIdx);
        }
        emptyNewRows.sort((a, b) => b - a);
        for (const rowIdx of emptyNewRows) ws.deleteRow(rowIdx);
        if (dirtyRows.size === 0) { updateSaveButtonState(); return; }
        const errors = validateDirtyRows();
        if (errors.length > 0) { showValidationErrors(errors); return; }
        saveBtn.textContent = 'Saving...'; saveBtn.disabled = true;
        ensureRowIdsSync();
        const sorted = [...dirtyRows].filter(idx => idx >= 0 && idx < rowIds.length).sort((a, b) => a - b);
        try {
          _saving = true;
          for (const rowIdx of sorted) await commitRow(rowIdx);
          dirtyRows.clear(); clearValidationHighlights(); updateSaveButtonState();
        } catch (e) {
          console.error('Save error:', e);
          errorDiv.textContent = `Save failed: ${e}`; errorDiv.style.display = '';
          saveBtn.disabled = false; saveBtn.className = 'btn btn-primary'; saveBtn.textContent = 'Save';
        } finally { _saving = false; }
      }

      function handleCopy(worksheet, coords, text) { return text.replace(/<[^>]*>/g, ''); }

      function handleChanges(instance, records) {
        if (_saving) return;
        const emptyDiv = container.querySelector('#empty-msg');
        if (emptyDiv) emptyDiv.remove();
        for (const rec of records) {
          while (rowIds.length <= rec.y) rowIds.push(null);
          dirtyRows.add(rec.y);
        }
        updateSaveButtonState();
        for (const rec of records) {
          const key = `${rec.x},${rec.y}`;
          if (highlightedCells.has(key)) {
            const cell = ws.getCellFromCoords(rec.x, rec.y);
            if (cell) cell.style.outline = '';
            highlightedCells.delete(key);
          }
          if (serverValidationCells.has(key)) {
            const cell = ws.getCellFromCoords(rec.x, rec.y);
            if (cell) { cell.style.backgroundColor = ''; cell.title = ''; }
            serverValidationCells.delete(key);
          }
        }
        if (!highlightedCells.size) { errorDiv.style.display = 'none'; errorDiv.textContent = ''; }
        if (!serverValidationCells.size) { validateBtn.textContent = 'Validate'; validateBtn.className = 'btn'; }
      }

      async function commitRow(rowIdx) {
        const rowData = ws.getRowData(rowIdx);
        const fieldValues = {};
        columns.forEach((col, colIdx) => {
          if (col.name === 'id' || !col.name) return;
          if (col.type === 'checkbox') fieldValues[col.name] = rowData[colIdx] ? 'true' : '';
          else fieldValues[col.name] = rowData[colIdx] || '';
        });
        const id = rowIds[rowIdx];
        if (!id) {
          const result = await invoke('add_component', { libPath, componentTypeName, fields: fieldValues });
          const newId = result && result.id ? result.id : result;
          rowIds[rowIdx] = newId;
          ws.setValueFromCoords(0, rowIdx, newId, true);
          updateTitle();
        } else {
          await invoke('update_component', { libPath, componentTypeName, id, fields: fieldValues });
        }
        savedData[rowIdx] = ws.getRowData(rowIdx).map(v => v);
      }

      function handleBeforeInsertRow() {}

      function handleInsertRow(worksheet, records) {
        const rowNumber = records[0].row;
        const numRows = records.length;
        rowIds.splice(rowNumber, 0, ...new Array(numRows).fill(null));
        savedData.splice(rowNumber, 0, ...new Array(numRows).fill(null));
        shiftDirtyRows(rowNumber, numRows);
        for (let i = 0; i < numRows; i++) dirtyRows.add(rowNumber + i);
        updateSaveButtonState(); updateTitle();
        const emptyDiv = container.querySelector('#empty-msg');
        if (emptyDiv) emptyDiv.remove();
      }

      function handleBeforeDeleteRow(worksheet, rowIndices) {
        const rowNumber = rowIndices[0];
        const numRows = rowIndices.length;
        for (let i = 0; i < numRows; i++) {
          const id = rowIds[rowNumber + i];
          if (id) invoke('delete_component', { libPath, componentTypeName, id }).catch(e => console.error('Delete error:', e));
        }
        rowIds.splice(rowNumber, numRows);
        savedData.splice(rowNumber, numRows);
        for (let i = 0; i < numRows; i++) dirtyRows.delete(rowNumber + i);
        shiftDirtyRows(rowNumber + numRows, -numRows);
        updateSaveButtonState(); updateTitle();
      }

      function showDeleteConfirm(message) {
        return new Promise((resolve) => {
          const close = (result) => { modal.remove(); resolve(result); };
          const modal = h('div', { className: 'modal modal-open' },
            h('div', { className: 'modal-box' },
              h('h3', { className: 'font-bold text-lg mb-3' }, 'Confirm Delete'),
              h('p', { className: 'text-sm' }, message),
              h('div', { className: 'modal-action' },
                h('button', { className: 'btn', onClick: () => close(false) }, 'Cancel'),
                h('button', { className: 'btn btn-error', onClick: () => close(true) }, 'Delete'),
              ),
            ),
            h('div', { className: 'modal-backdrop', onClick: () => close(false) }),
          );
          document.body.appendChild(modal);
        });
      }

      async function deleteSelectedRows() {
        const selectedRows = ws.getSelectedRows();
        if (!selectedRows || !selectedRows.length) return;
        const sortedRows = [...selectedRows].sort((a, b) => b - a);
        const savedIds = sortedRows.map(r => rowIds[r]).filter(Boolean);
        if (savedIds.length > 0) {
          const msg = savedIds.length === 1
            ? `Delete component ${savedIds[0]}?`
            : `Delete ${savedIds.length} components: ${savedIds.join(', ')}?`;
          if (!await showDeleteConfirm(msg)) return;
        }
        for (const row of sortedRows) ws.deleteRow(row);
      }

      // Expose nav guard for App.svelte
      window.__legacyNavGuard = () => dirtyRows.size > 0;
    },
  };
})();
