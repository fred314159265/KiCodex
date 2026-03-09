import { invoke } from './tauri.js';
import { ask } from './confirm.svelte.js';
import { showInfo } from './toast.svelte.js';

/**
 * Show a confirmation before modifying sym-lib-table, and a restart
 * reminder afterward if KiCad has the project open.
 *
 * @param {string} projectPath
 * @param {() => Promise<any>} doRegister - callback that performs the actual registration
 * @returns {Promise<boolean>} true if registration was performed, false if cancelled
 */
export async function confirmAndRegister(projectPath, doRegister) {
  const confirmed = await ask(
    'This will modify KiCad\'s sym-lib-table file. There is a small chance this could corrupt the file \u2014 make sure it is tracked in version control or backed up first.',
    { title: 'Modify sym-lib-table?', confirmLabel: 'Register', cancelLabel: 'Cancel' },
  );
  if (!confirmed) return false;

  const wasActive = await invoke('is_project_active_in_kicad', { projectPath }).catch(() => false);

  await doRegister();

  if (wasActive) {
    showInfo('Please restart KiCad for the changes to take effect.');
  }

  return true;
}
