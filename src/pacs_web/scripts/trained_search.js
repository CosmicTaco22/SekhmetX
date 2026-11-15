(function() {
  const cfg = __PAYLOAD__;
  const p = cfg.profile;
  const input = document.querySelector(p.search_input_sel);
  if (!input) {
    window.ipc.postMessage('SEARCH_ERR\tSearch input not found on page.');
    return;
  }
  input.focus();
  input.value = cfg.query || '';
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
  input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }));
  input.dispatchEvent(new KeyboardEvent('keyup', { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }));
  if (input.form) {
    const btn = input.form.querySelector('button[type="submit"], input[type="submit"], button');
    if (btn) btn.click();
    else if (typeof input.form.requestSubmit === 'function') input.form.requestSubmit();
  }
  let tries = 0;
  const timer = setInterval(function() {
    tries++;
    const rows = Array.from(document.querySelectorAll(p.row_list_sel)).slice(0, 50);
    if (rows.length > 0 || tries >= 20) {
      clearInterval(timer);
      const out = rows.map((r, i) => ({
        index: i,
        text: (r.innerText || r.textContent || '').replace(/\s+/g, ' ').trim()
      })).filter(x => x.text.length > 0);
      window.ipc.postMessage('SEARCH_RESULTS\t' + JSON.stringify(out));
    }
  }, 250);
})();
