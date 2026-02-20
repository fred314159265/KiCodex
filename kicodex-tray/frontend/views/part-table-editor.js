// Part table editor view — jspreadsheet CE powered data grid
const PartTableEditorView = {
  async render(container, params) {
    const libPath = params.lib;
    const componentTypeName = params.type;
    const projectPath = params.project;

    if (!libPath || !componentTypeName) { navigate('dashboard'); return; }

    document.getElementById('app').classList.add('view-wide');

    const data = await invoke('get_part_table_data', { libPath, componentTypeName });

    container.innerHTML = '';

    const bcParts = [
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
    ];
    if (projectPath) {
      bcParts.push(h('a', { href: `#project?path=${encodeURIComponent(projectPath)}` }, projectPath.split(/[\\/]/).pop()));
    } else {
      // Standalone library — link back to library view
      const libName = libPath.split(/[\\/]/).pop() || libPath;
      bcParts.push(h('a', { href: `#library?path=${encodeURIComponent(libPath)}` }, libName));
    }
    bcParts.push(h('span', {}, ' / '));
    bcParts.push(h('span', {}, data.name));
    const bc = h('div', { className: 'breadcrumb' }, ...bcParts);
    container.appendChild(bc);

    const fields = data.template.fields;
    const checkboxKeys = ['exclude_from_bom', 'exclude_from_board', 'exclude_from_sim'];

    // Track component IDs by row index
    const rowIds = data.components.map(c => c.id);

    // Dirty state tracking — rows with unsaved changes
    const dirtyRows = new Set();

    // Build jspreadsheet column definitions
    const columns = [];
    const requiredColIndices = new Set();
    columns.push({ title: 'ID', name: 'id', type: 'text', readOnly: true, width: 80 });

    for (const f of fields) {
      if (f.key === 'id') continue;
      if (checkboxKeys.includes(f.key)) continue;
      const col = { title: f.display_name, name: f.key, type: 'text', width: 140 };
      if (f.required) {
        requiredColIndices.add(columns.length);
        col.title += ' *';
      }
      if (f.field_type === 'kicad_symbol' || f.field_type === 'kicad_footprint') {
        col._kicadKind = f.field_type === 'kicad_symbol' ? 'symbol' : 'footprint';
        col.render = (td, value, x, y) => {
          td.innerText = value || '';
          td.style.position = 'relative';
          const btn = document.createElement('button');
          btn.className = 'cell-browse-btn';
          btn.addEventListener('mousedown', (ev) => {
            ev.preventDefault();
            ev.stopPropagation();
            openPicker(col._kicadKind, (val) => {
              ws.setValueFromCoords(x, y, val);
            }, value || '');
          });
          td.appendChild(btn);
        };
      }
      columns.push(col);
    }

    // Checkbox columns for exclude flags
    columns.push({ title: 'BOM', name: 'exclude_from_bom', type: 'checkbox', width: 55 });
    columns.push({ title: 'Board', name: 'exclude_from_board', type: 'checkbox', width: 60 });
    columns.push({ title: 'Sim', name: 'exclude_from_sim', type: 'checkbox', width: 55 });

    // Auto-scale text column widths to fill available space
    const ROW_HEADER_WIDTH = 50; // jspreadsheet row-number gutter
    const availableWidth = container.parentElement.clientWidth - ROW_HEADER_WIDTH;
    const fixedTotal = columns.reduce((sum, c) => sum + (c.type === 'checkbox' || c.name === 'id' ? c.width : 0), 0);
    const flexCols = columns.filter(c => c.type !== 'checkbox' && c.name !== 'id');
    if (flexCols.length > 0) {
      const flexWidth = Math.max(140, Math.floor((availableWidth - fixedTotal) / flexCols.length));
      flexCols.forEach(c => c.width = flexWidth);
    }

    // Build data as array-of-arrays matching column order
    const rows = data.components.map(comp => {
      return columns.map(col => {
        if (col.type === 'checkbox') return comp[col.name] === 'true';
        return comp[col.name] || '';
      });
    });

    // Save button
    const saveBtn = h('button', { className: 'btn', onClick: saveAll }, 'Save');
    saveBtn.disabled = true;

    // Header with buttons
    const header = h('div', { className: 'page-header' },
      h('h2', { className: 'page-title' }, `${data.name} (${data.components.length} components)`),
      h('div', { className: 'btn-group' },
        saveBtn,
        h('button', { className: 'btn', onClick: () => {
          ws.insertRow();
        }}, 'Add Row'),
        h('button', { className: 'btn', onClick: () => {
          navigate('component-form', { lib: libPath, type: componentTypeName, project: projectPath, mode: 'add' });
        }}, 'Add in Form'),
        h('button', { className: 'btn btn-danger', onClick: () => {
          deleteSelectedRows();
        }}, 'Delete Selected'),
        h('button', { className: 'btn', onClick: () => {
          navigate('template-editor', { lib: libPath, template: data.template_name, project: projectPath });
        }}, 'Edit Template'),
      ),
    );
    container.appendChild(header);

    // Error message area for validation
    const errorDiv = h('div', { className: 'field-error-msg', style: { display: 'none' } });
    container.appendChild(errorDiv);

    if (data.components.length === 0) {
      container.appendChild(h('div', { className: 'empty', id: 'empty-msg' }, 'No components yet. Click Add Row to create one.'));
    }

    // Grid container
    const wrapper = document.createElement('div');
    wrapper.id = 'grid';
    container.appendChild(wrapper);

    // Initialize jspreadsheet
    const spreadsheet = jspreadsheet(wrapper, {
      worksheets: [{
        columns,
        data: rows.length > 0 ? rows : [[]],
        minDimensions: [columns.length, 0],
        allowInsertColumn: false,
        allowDeleteColumn: false,
        allowRenameColumn: false,
        columnDrag: false,
        tableOverflow: true,
        tableWidth: '100%',
        tableHeight: 'calc(100vh - 180px)',
      }],
      onafterchanges: handleChanges,
      onbeforeinsertrow: handleBeforeInsertRow,
      oninsertrow: handleInsertRow,
      onbeforedeleterow: handleBeforeDeleteRow,
      oncopy: handleCopy,
    });

    const ws = spreadsheet[0];

    // If no data, track the placeholder empty row so rowIds stays in sync with the grid
    if (rows.length === 0) {
      rowIds.push(null);
    }

    // Clear any dirty state triggered during jspreadsheet init (e.g. checkbox defaults)
    dirtyRows.clear();

    // Track cells with validation highlights so we can clear them
    const highlightedCells = new Set();
    // Suppress handleChanges during save to prevent setValueFromCoords re-entry
    let _saving = false;

    function updateTitle() {
      header.querySelector('.page-title').textContent = `${data.name} (${rowIds.length} components)`;
    }

    function updateSaveButtonState() {
      if (dirtyRows.size > 0) {
        saveBtn.disabled = false;
        saveBtn.className = 'btn btn-primary';
        saveBtn.textContent = 'Save';
      } else {
        saveBtn.disabled = true;
        saveBtn.className = 'btn';
        saveBtn.textContent = 'Save';
      }
    }

    function shiftDirtyRows(startIdx, delta) {
      const updated = new Set();
      for (const idx of dirtyRows) {
        if (idx >= startIdx) {
          const shifted = idx + delta;
          if (shifted >= 0) updated.add(shifted);
        } else {
          updated.add(idx);
        }
      }
      dirtyRows.clear();
      for (const idx of updated) dirtyRows.add(idx);
    }

    // --- Validation ---

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
      errorDiv.style.display = 'none';
      errorDiv.textContent = '';
    }

    function showValidationErrors(errors) {
      clearValidationHighlights();
      for (const err of errors) {
        const cell = ws.getCellFromCoords(err.colIdx, err.rowIdx);
        if (cell) cell.style.outline = '2px solid var(--error)';
        highlightedCells.add(`${err.colIdx},${err.rowIdx}`);
      }
      errorDiv.textContent = errors.map(e => e.message).join(' | ');
      errorDiv.style.display = '';
    }

    // --- Save ---

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
      while (rowIds.length < gridRows) {
        rowIds.push(null);
      }
      while (rowIds.length > gridRows) {
        rowIds.pop();
      }
    }

    function markUntrackedRowsAsDirty() {
      for (let i = 0; i < rowIds.length; i++) {
        if (!rowIds[i] && !isRowEmpty(i)) {
          dirtyRows.add(i);
        }
      }
    }

    async function saveAll() {
      ensureRowIdsSync();
      markUntrackedRowsAsDirty();

      // Remove dirty unsaved rows that are completely empty (e.g. trailing blank rows)
      const emptyNewRows = [];
      for (const rowIdx of dirtyRows) {
        if (rowIdx >= 0 && rowIdx < rowIds.length && !rowIds[rowIdx] && isRowEmpty(rowIdx)) {
          emptyNewRows.push(rowIdx);
        }
      }
      // Delete from bottom up so indices stay valid
      emptyNewRows.sort((a, b) => b - a);
      for (const rowIdx of emptyNewRows) {
        ws.deleteRow(rowIdx);
      }
      if (dirtyRows.size === 0) {
        updateSaveButtonState();
        return;
      }

      const errors = validateDirtyRows();
      if (errors.length > 0) {
        showValidationErrors(errors);
        return;
      }

      saveBtn.textContent = 'Saving...';
      saveBtn.disabled = true;

      ensureRowIdsSync();

      const sorted = [...dirtyRows]
        .filter(idx => idx >= 0 && idx < rowIds.length)
        .sort((a, b) => a - b);
      try {
        _saving = true;
        for (const rowIdx of sorted) {
          await commitRow(rowIdx);
        }
        dirtyRows.clear();
        clearValidationHighlights();
        updateSaveButtonState();
      } catch (e) {
        console.error('Save error:', e);
        errorDiv.textContent = `Save failed: ${e}`;
        errorDiv.style.display = '';
        saveBtn.disabled = false;
        saveBtn.className = 'btn btn-primary';
        saveBtn.textContent = 'Save';
      } finally {
        _saving = false;
      }
    }

    // --- Event handlers ---

    // jspreadsheet copy includes innerHTML of custom-rendered cells,
    // which picks up the browse button markup. Strip all HTML tags.
    function handleCopy(worksheet, coords, text, style) {
      return text.replace(/<[^>]*>/g, '');
    }

    function handleChanges(instance, records) {
      // Ignore changes triggered by setValueFromCoords during save
      if (_saving) return;

      // Remove empty message if present
      const emptyDiv = container.querySelector('#empty-msg');
      if (emptyDiv) emptyDiv.remove();

      for (const rec of records) {
        // Defensively extend rowIds if onafterchanges fires before oninsertrow
        // has had a chance to grow rowIds for paste-created rows
        while (rowIds.length <= rec.y) {
          rowIds.push(null);
        }
        dirtyRows.add(rec.y);
      }
      updateSaveButtonState();

      // Clear validation highlights on edited cells
      for (const rec of records) {
        const key = `${rec.x},${rec.y}`;
        if (highlightedCells.has(key)) {
          const cell = ws.getCellFromCoords(rec.x, rec.y);
          if (cell) cell.style.outline = '';
          highlightedCells.delete(key);
        }
      }
      if (highlightedCells.size === 0) {
        errorDiv.style.display = 'none';
        errorDiv.textContent = '';
      }
    }

    async function commitRow(rowIdx) {
      const rowData = ws.getRowData(rowIdx);
      const fieldValues = {};

      columns.forEach((col, colIdx) => {
        if (col.name === 'id' || !col.name) return;
        if (col.type === 'checkbox') {
          fieldValues[col.name] = rowData[colIdx] ? 'true' : '';
        } else {
          fieldValues[col.name] = rowData[colIdx] || '';
        }
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
    }

    function handleBeforeInsertRow() {
      // return false to cancel; any other return allows the insert
    }

    function handleInsertRow(worksheet, records) {
      // records is an array of {row, data} from jspreadsheet spreadsheet-level event
      const rowNumber = records[0].row;
      const numRows = records.length;
      rowIds.splice(rowNumber, 0, ...new Array(numRows).fill(null));
      // Shift dirty indices for the insertion
      shiftDirtyRows(rowNumber, numRows);
      // Mark inserted rows as dirty (they need saving)
      for (let i = 0; i < numRows; i++) {
        dirtyRows.add(rowNumber + i);
      }
      updateSaveButtonState();
      updateTitle();
      // Remove empty message if present
      const emptyDiv = container.querySelector('#empty-msg');
      if (emptyDiv) emptyDiv.remove();
    }

    function handleBeforeDeleteRow(worksheet, rowIndices) {
      // rowIndices is an array of row indices from jspreadsheet spreadsheet-level event
      const rowNumber = rowIndices[0];
      const numRows = rowIndices.length;
      for (let i = 0; i < numRows; i++) {
        const id = rowIds[rowNumber + i];
        if (id) {
          invoke('delete_component', { libPath, componentTypeName, id }).catch(e => {
            console.error('Delete error:', e);
          });
        }
      }
      rowIds.splice(rowNumber, numRows);
      // Remove deleted rows from dirty set, then shift remaining indices down
      for (let i = 0; i < numRows; i++) {
        dirtyRows.delete(rowNumber + i);
      }
      shiftDirtyRows(rowNumber + numRows, -numRows);
      updateSaveButtonState();
      updateTitle();
    }

    function deleteSelectedRows() {
      const selected = ws.getSelected(true);
      if (!selected || selected.length === 0) return;
      // Get unique row indices from selection
      const rowSet = new Set();
      for (const cell of selected) {
        rowSet.add(cell[1]);
      }
      const sortedRows = [...rowSet].sort((a, b) => b - a); // delete from bottom up
      for (const row of sortedRows) {
        const id = rowIds[row];
        if (id && !confirm(`Delete component ${id}?`)) continue;
        ws.deleteRow(row);
      }
    }

    // --- Navigation guard ---

    function cleanup() {
      navigationGuard = null;
      window.removeEventListener('beforeunload', beforeUnloadHandler);
    }

    function beforeUnloadHandler(e) {
      if (dirtyRows.size > 0) {
        e.preventDefault();
        e.returnValue = '';
      }
    }

    navigationGuard = () => dirtyRows.size > 0;

    window.addEventListener('beforeunload', beforeUnloadHandler);

  }
};
