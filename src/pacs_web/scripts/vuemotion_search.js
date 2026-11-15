(function() {
  const cfg = __PAYLOAD__;
  __VM_DOC__
  function getRows(doc) {
    let rows = Array.from(doc.querySelectorAll('#dataTableID tr[id^="ptRecord_"]'));
    if (rows.length === 0) {
      rows = Array.from(doc.querySelectorAll('#dataTableID tbody tr')).filter(r => r.querySelector('td'));
    }
    return rows.slice(0, 50);
  }
  function runSearch() {
    const doc = vmDoc();
    const input = doc.querySelector('#sptGeneralDetailsInput');
    const btn = doc.querySelector('#sptSearchSubmitBtn');
    if (!input) return false;
    const prevKey = getRows(doc)
      .map(r => (r.id || '') + ':' + (r.innerText || '').slice(0, 40))
      .join('|');
    input.focus();
    try {
      const win = doc.defaultView || window;
      const nativeSetter = Object.getOwnPropertyDescriptor(win.HTMLInputElement.prototype, 'value').set;
      if (nativeSetter) nativeSetter.call(input, cfg.query || '');
      else input.value = cfg.query || '';
    } catch (_) {
      input.value = cfg.query || '';
    }
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    if (btn) btn.click();
    else if (input.form) input.form.submit();
    else input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', keyCode: 13, bubbles: true }));
    let tries = 0;
    const t = setInterval(function() {
      tries++;
      const d = vmDoc();
      const rows = getRows(d);
      const curKey = rows.map(r => (r.id || '') + ':' + (r.innerText || '').slice(0, 40)).join('|');
      const changed = curKey !== prevKey;
      if ((rows.length > 0 && (changed || tries >= 6)) || tries >= 24) {
        clearInterval(t);
        const out = rows.map((r, i) => {
          const link = r.querySelector('a.dtPatientLink') || r.querySelector('a');
          const name = (link ? (link.innerText || link.title || '') : '').replace(/\s+/g, ' ').trim();
          const cells = Array.from(r.querySelectorAll('td'))
            .map(td => (td.innerText || '').replace(/\s+/g, ' ').trim())
            .filter(s => s.length > 0 && s !== '.' && s !== '-');
          return { index: i, name: name || cells[0] || ('Patient #' + (i + 1)), summary: cells.join(' · ') };
        });
        window.ipc.postMessage('PATIENT_RESULTS\t' + JSON.stringify(out));
      }
    }, 250);
    return true;
  }
  if (!runSearch()) {
    const doc = vmDoc();
    const searchTab = doc.querySelector('#mainTab-0, a[href*="default.aspx"]');
    if (searchTab) searchTab.click();
    else if (cfg.login_url) window.location.href = cfg.login_url;
    let w = 0;
    const wt = setInterval(function() {
      w++;
      if (runSearch() || w >= 20) {
        clearInterval(wt);
        if (w >= 20) window.ipc.postMessage('SEARCH_ERR\tVue Motion search bar not found.');
      }
    }, 300);
  }
})();
