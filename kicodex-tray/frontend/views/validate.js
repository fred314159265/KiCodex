// Validate view — validation results grouped by table
const ValidateView = {
  async render(container, params) {
    const projectPath = params.project;
    const libParam = params.lib;

    if (!projectPath && !libParam) { navigate('dashboard'); return; }

    container.innerHTML = '';

    // Build breadcrumb based on context
    const bcParts = [
      h('a', { href: '#dashboard' }, 'Dashboard'),
      h('span', {}, ' / '),
    ];
    if (projectPath) {
      bcParts.push(h('a', { href: `#project?path=${encodeURIComponent(projectPath)}` }, projectPath.split(/[\\/]/).pop()));
    } else {
      const libName = libParam.split(/[\\/]/).pop() || libParam;
      bcParts.push(h('a', { href: `#library?path=${encodeURIComponent(libParam)}` }, libName));
    }
    bcParts.push(h('span', {}, ' / '));
    bcParts.push(h('span', {}, 'Validate'));
    const bc = h('div', { className: 'breadcrumb' }, ...bcParts);
    container.appendChild(bc);

    // Determine library path
    let libPath;
    if (libParam) {
      // Standalone library — validate directly
      libPath = libParam;
    } else {
      // Project-based — find the library from project entries
      const projects = await invoke('get_projects');
      const entry = projects.find(p => p.project_path === projectPath);
      if (!entry) {
        container.appendChild(h('div', { className: 'empty' }, 'Project not found'));
        return;
      }
      libPath = entry.library_path;
    }

    container.appendChild(h('div', { className: 'loading' }, 'Validating...'));

    try {
      const result = await invoke('validate_library', {
        libPath,
        projectPath: projectPath || null,
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
        `across ${result.part_tables.length} part table${result.part_tables.length !== 1 ? 's' : ''}`
      ));
      container.appendChild(summary);

      // Per-component-type results
      for (const table of result.part_tables) {
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
