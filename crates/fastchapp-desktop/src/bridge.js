(() => {
  'use strict';
  const send = (action, value = '', fields = {}) => window.ipc.postMessage(JSON.stringify({action, value, fields}));
  const interactive = el => el.closest('button,input,select,textarea,a,label');
  document.addEventListener('click', event => {
    const button = event.target.closest('button[data-action]');
    if (!button || button.disabled) return;
    event.preventDefault();
    send(button.dataset.action, button.dataset.value || '');
  });
  document.addEventListener('submit', event => {
    const form = event.target.closest('form[data-action]');
    if (!form) return;
    event.preventDefault();
    const fields = {};
    for (const [key, value] of new FormData(form)) {
      if (typeof value === 'string') (fields[key] ||= []).push(value);
    }
    send(form.dataset.action, form.dataset.value || '', fields);
  });
  document.addEventListener('mousedown', event => {
    if (event.button !== 0) return;
    const resize = event.target.closest('[data-window-resize]');
    if (resize) { event.preventDefault(); send('window_resize', resize.dataset.windowResize); return; }
    if (event.detail < 2 && event.target.closest('[data-window-drag]') && !interactive(event.target)) {
      event.preventDefault(); send('window_drag');
    }
  });
  document.addEventListener('dblclick', event => {
    if (event.target.closest('[data-window-drag]') && !interactive(event.target)) send('window_toggle_maximize');
  });
  window.FastChapp = Object.freeze({apply(update) {
    for (const patch of update.patches || []) {
      for (const element of document.querySelectorAll(patch.selector)) {
        switch (patch.op) {
          case 'html': element.innerHTML = patch.value; break;
          case 'text': element.textContent = patch.value; break;
          case 'value': element.value = patch.value; break;
          case 'attr':
            if (patch.value == null) element.removeAttribute(patch.name);
            else element.setAttribute(patch.name, patch.value);
            break;
        }
      }
    }
    // Native window metadata is consumed by Rust, never by a browser timer.
  }});
})();
