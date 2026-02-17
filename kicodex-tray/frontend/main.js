// KiCodex Frontend — Router & Helpers

// Defer Tauri API access to DOMContentLoaded to avoid Windows timing issue
// where window.__TAURI__ is undefined during top-level script execution.
let invoke;

// Navigation guard: views can set this to block navigation when dirty
let navigationGuard = null;
let _previousHash = window.location.hash;

function initTauri() {
  if (invoke) return true;
  if (window.__TAURI__ && window.__TAURI__.core) {
    invoke = window.__TAURI__.core.invoke;
    return true;
  }
  return false;
}

// Parse hash route: "#view?key=val&key2=val2"
function parseRoute() {
  const hash = window.location.hash.slice(1) || 'dashboard';
  const [view, qs] = hash.split('?');
  const params = {};
  if (qs) {
    for (const part of qs.split('&')) {
      const [k, v] = part.split('=');
      params[decodeURIComponent(k)] = decodeURIComponent(v || '');
    }
  }
  return { view, params };
}

function navigate(view, params = {}) {
  const qs = Object.entries(params)
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
    .join('&');
  window.location.hash = qs ? `${view}?${qs}` : view;
}

// View registry
const views = {
  dashboard: DashboardView,
  project: ProjectView,
  library: LibraryView,
  'part-table-editor': PartTableEditorView,
  'component-form': ComponentFormView,
  validate: ValidateView,
  'add-project': AddProjectView,
  'template-editor': TemplateEditorView,
};

// Custom in-page confirm modal (confirm() is non-blocking in Tauri WebView2)
let _navConfirmPending = false;
function showNavConfirm(message) {
  return new Promise((resolve) => {
    const overlay = h('div', { className: 'modal-overlay' },
      h('div', { className: 'modal', style: { width: '400px' } },
        h('div', { className: 'modal-header' }, h('h3', {}, 'Unsaved Changes')),
        h('div', { style: { padding: '16px', fontSize: '14px' } }, message),
        h('div', { style: { padding: '12px 16px', display: 'flex', gap: '8px', justifyContent: 'flex-end', borderTop: '1px solid var(--border)' } },
          h('button', { className: 'btn', onClick: () => { overlay.remove(); resolve(false); } }, 'Stay'),
          h('button', { className: 'btn btn-danger', onClick: () => { overlay.remove(); resolve(true); } }, 'Discard & Leave'),
        ),
      ),
    );
    document.body.appendChild(overlay);
  });
}

async function renderRoute() {
  // Check navigation guard BEFORE touching the DOM so the current view stays intact
  if (navigationGuard && navigationGuard()) {
    if (_navConfirmPending) return; // modal already showing, ignore duplicate hashchange
    _navConfirmPending = true;
    const wantsToLeave = await showNavConfirm('You have unsaved changes. Discard and leave?');
    _navConfirmPending = false;
    if (!wantsToLeave) {
      history.replaceState(null, '', _previousHash);
      return;
    }
    navigationGuard = null;
  }
  _previousHash = window.location.hash;

  const container = document.getElementById('app');

  if (!initTauri()) {
    container.innerHTML =
      '<div style="padding:40px;color:#dc2626;">' +
      '<h2>Tauri IPC not available</h2>' +
      '<p>window.__TAURI__ is undefined. Ensure <code>withGlobalTauri: true</code> is set in tauri.conf.json.</p></div>';
    return;
  }

  container.classList.remove('view-wide');
  container.innerHTML = '<div class="loading">Loading...</div>';

  const { view, params } = parseRoute();
  const renderer = views[view];
  if (renderer) {
    try {
      await renderer.render(container, params);
    } catch (err) {
      container.innerHTML = `<div class="card" style="color:var(--error)">
        <strong>Error:</strong> ${escapeHtml(String(err))}
      </div>`;
    }
  } else {
    container.innerHTML = '<div class="empty">View not found</div>';
  }
}

window.addEventListener('hashchange', () => {
  renderRoute();
});
window.addEventListener('DOMContentLoaded', () => {
  renderRoute();

  // Re-render current view when backend reports project changes
  if (window.__TAURI__ && window.__TAURI__.event) {
    window.__TAURI__.event.listen('projects-changed', () => {
      renderRoute();
    });
    window.__TAURI__.event.listen('navigate', (event) => {
      window.location.hash = event.payload;
    });
  }
});

// Utility: escape HTML
function escapeHtml(s) {
  const el = document.createElement('span');
  el.textContent = s;
  return el.innerHTML;
}

// Utility: create element helper
function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'className') el.className = v;
    else if (k === 'style' && typeof v === 'object') Object.assign(el.style, v);
    else if (k.startsWith('on')) el.addEventListener(k.slice(2).toLowerCase(), v);
    else el.setAttribute(k, v);
  }
  for (const child of children) {
    if (typeof child === 'string') el.appendChild(document.createTextNode(child));
    else if (child) el.appendChild(child);
  }
  return el;
}
