let toasts = $state([]);
let _id = 0;

function add(type, msg, duration) {
  const id = ++_id;
  toasts.push({ id, type, msg: String(msg) });
  setTimeout(() => dismiss(id), duration);
}

export function dismiss(id) {
  const i = toasts.findIndex(t => t.id === id);
  if (i !== -1) toasts.splice(i, 1);
}

export function showError(msg) { add('error', msg, 6000); }
export function showInfo(msg)  { add('info',  msg, 4000); }

export { toasts };
