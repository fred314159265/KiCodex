export const state = $state({
  open: false,
  title: 'Confirm',
  message: '',
  confirmLabel: 'Confirm',
  cancelLabel: 'Cancel',
});

let resolveFn = null;

export function ask(message, { title = 'Confirm', confirmLabel = 'Confirm', cancelLabel = 'Cancel' } = {}) {
  state.open = true;
  state.message = message;
  state.title = title;
  state.confirmLabel = confirmLabel;
  state.cancelLabel = cancelLabel;
  return new Promise(resolve => { resolveFn = resolve; });
}

export function answer(yes) {
  state.open = false;
  resolveFn?.(yes);
  resolveFn = null;
}
