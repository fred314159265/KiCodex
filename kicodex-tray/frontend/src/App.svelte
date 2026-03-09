<script>
  import { onMount } from 'svelte';
  import { listen } from './lib/tauri.js';
  import Modal from './components/Modal.svelte';
  import ConfirmModal from './components/ConfirmModal.svelte';
  import Toast from './components/Toast.svelte';

  import Dashboard from './views/Dashboard.svelte';
  import Project from './views/Project.svelte';
  import Library from './views/Library.svelte';
  import Validate from './views/Validate.svelte';
  import Picker from './views/Picker.svelte';
  import AddProject from './views/AddProject.svelte';
  import TemplateEditor from './views/TemplateEditor.svelte';
  import ComponentForm from './views/ComponentForm.svelte';
  import RowForm from './views/RowForm.svelte';
  import SchemaEditor from './views/SchemaEditor.svelte';
  import TableEditor from './views/TableEditor.svelte';
  import PartTableEditor from './views/PartTableEditor.svelte';

  const ROUTES = {
    'dashboard': Dashboard,
    'project': Project,
    'library': Library,
    'validate': Validate,
    'picker': Picker,
    'add-project': AddProject,
    'template-editor': TemplateEditor,
    'component-form': ComponentForm,
    'row-form': RowForm,
    'schema-editor': SchemaEditor,
    'table-editor': TableEditor,
    'part-table-editor': PartTableEditor,
  };

  function parseRoute() {
    const hash = window.location.hash.slice(1) || 'dashboard';
    const [view, qs] = hash.split('?');
    const params = {};
    if (qs) {
      for (const part of qs.split('&')) {
        const [k, v] = part.split('=');
        if (k) params[decodeURIComponent(k)] = decodeURIComponent(v || '');
      }
    }
    return { view: view || 'dashboard', params };
  }

  const initial = parseRoute();
  let currentView = $state(initial.view);
  let params = $state(initial.params);
  let previousHash = $state(window.location.hash || '#dashboard');

  // Unsaved-changes confirm modal state
  let navConfirmOpen = $state(false);
  let navConfirmPending = false;
  let pendingHash = $state('');

  function isNavigationGuarded() {
    // Check legacy nav guard from part-table-editor-view.js
    if (window.__legacyNavGuard && window.__legacyNavGuard()) return true;
    return false;
  }

  function handleHashChange() {
    if (isNavigationGuarded()) {
      if (navConfirmPending) return;
      navConfirmPending = true;
      pendingHash = window.location.hash;
      navConfirmOpen = true;
      history.replaceState(null, '', previousHash);
      return;
    }
    previousHash = window.location.hash;
    const parsed = parseRoute();
    currentView = parsed.view;
    params = parsed.params;
  }

  function confirmNavLeave() {
    navConfirmOpen = false;
    navConfirmPending = false;
    window.__legacyNavGuard = null;
    history.replaceState(null, '', pendingHash);
    previousHash = pendingHash;
    const parsed = parseRoute();
    currentView = parsed.view;
    params = parsed.params;
  }

  function cancelNavLeave() {
    navConfirmOpen = false;
    navConfirmPending = false;
  }

  onMount(() => {
    window.addEventListener('hashchange', handleHashChange);

    const unlistenProjects = listen('projects-changed', () => {
      const parsed = parseRoute();
      currentView = parsed.view;
      // Force reactivity by creating new object
      params = { ...parsed.params };
    });

    const unlistenNavigate = listen('navigate', (event) => {
      window.location.hash = event.payload;
    });

    return () => {
      window.removeEventListener('hashchange', handleHashChange);
      unlistenProjects.then(fn => fn());
      unlistenNavigate.then(fn => fn());
    };
  });

  let ViewComponent = $derived(ROUTES[currentView] ?? Dashboard);
</script>

<ConfirmModal />
<Toast />

<Modal
  open={navConfirmOpen}
  title="Unsaved Changes"
  onclose={cancelNavLeave}
>
  {#snippet body()}
    <p class="py-2">You have unsaved changes. Discard and leave?</p>
  {/snippet}
  {#snippet footer()}
    <button class="btn" onclick={cancelNavLeave}>Stay</button>
    <button class="btn btn-error" onclick={confirmNavLeave}>Discard &amp; Leave</button>
  {/snippet}
</Modal>

<div class="p-4">
  <ViewComponent {params} />
</div>
