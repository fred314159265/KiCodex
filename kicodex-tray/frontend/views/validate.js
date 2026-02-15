// Validate view — validation results grouped by table
const ValidateView = {
  async render(container, params) {
    const projectPath = params.project;
    if (!projectPath) { navigate('dashboard'); return; }

    container.innerHTML = '';

    const bc = h('div', { className: 'breadcrumb' },
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
      h('a', { href: `#project?path=${encodeURIComponent(projectPath)}` }, projectPath.split(/[\\/]/).pop()),
      h('span', {}, ' / '),
      h('span', {}, 'Validate'),
    );
    container.appendChild(bc);

    // Get the library path
    const projects = await invoke('get_projects');
    const entry = projects.find(p => p.project_path === projectPath);
    if (!entry) {
      container.appendChild(h('div', { className: 'empty' }, 'Project not found'));
      return;
    }

    container.appendChild(h('div', { className: 'loading' }, 'Validating...'));

    try {
      const result = await invoke('validate_library', {
        libPath: entry.library_path,
        projectPath,
      });

      // Clear loading
      container.lastChild.remove();

      const header = h('div', { className: 'page-header' },
        h('h2', { className: 'page-title' }, `Validation: ${result.library}`),
      );
      container.appendChild(header);

      // Summary
      const summary = h('div', { className: 'summary' });
      if (result.error_count === 0 && result.warning_count === 0) {
        summary.appendChild(h('span', { className: 'summary-ok' }, 'No issues found'));
      } else {
        if (result.error_count > 0) {
          summary.appendChild(h('span', { className: 'summary-errors' },
            `${result.error_count} error${result.error_count !== 1 ? 's' : ''}`
          ));
        }
        if (result.warning_count > 0) {
          summary.appendChild(h('span', { className: 'summary-warnings' },
            `${result.warning_count} warning${result.warning_count !== 1 ? 's' : ''}`
          ));
        }
      }
      summary.appendChild(h('span', { style: { color: 'var(--text-muted)' } },
        `across ${result.tables.length} table${result.tables.length !== 1 ? 's' : ''}`
      ));
      container.appendChild(summary);

      // Per-table results
      for (const table of result.tables) {
        const issues = [...table.errors, ...table.warnings];
        if (issues.length === 0) continue;

        const group = h('div', { className: 'validation-group' });
        group.appendChild(h('div', { className: 'validation-group-title' },
          `${table.name} (${table.file})`
        ));

        for (const err of table.errors) {
          group.appendChild(renderIssue('error', err));
        }
        for (const warn of table.warnings) {
          group.appendChild(renderIssue('warn', warn));
        }
        container.appendChild(group);
      }

    } catch (e) {
      container.lastChild.remove();
      container.appendChild(h('div', { className: 'card', style: { color: 'var(--error)' } },
        `Validation error: ${e}`
      ));
    }
  }
};

function renderIssue(severity, issue) {
  const prefix = issue.row
    ? `Row ${issue.row}${issue.id ? ` (id=${issue.id})` : ''}: `
    : '';
  return h('div', { className: `issue issue-${severity}` },
    h('strong', {}, severity === 'error' ? '[ERROR] ' : '[WARN] '),
    `${prefix}${issue.message}`,
  );
}
