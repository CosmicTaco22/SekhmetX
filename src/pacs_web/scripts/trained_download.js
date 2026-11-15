(function() {
  const cfg = __PAYLOAD__;
  const p = cfg.profile;
  const rows = Array.from(document.querySelectorAll(p.row_list_sel));
  const row = rows[cfg.row_index];
  if (!row) {
    window.ipc.postMessage('DL_FAILED\tSelected study row is no longer visible on page.');
    return;
  }
  row.click();
  const useAnon = !!(cfg.anonymize && p.anon_steps && p.anon_steps.length > 0);
  const steps = useAnon ? p.anon_steps : (p.download_steps || []);
  if (steps.length === 0) {
    window.ipc.postMessage('DL_FAILED\tNo download steps trained for this portal.');
    return;
  }
  function runStep(idx) {
    if (idx >= steps.length) return;
    const st = steps[idx];
    const sel = (st.in_row && st.rel_selector) ? st.rel_selector : st.selector;
    const btn = (st.in_row && st.rel_selector) ? row.querySelector(sel) : document.querySelector(sel);
    if (!btn) {
      window.ipc.postMessage('DL_FAILED\tDownload step #' + (idx + 1) + ' button not found on page.');
      return;
    }
    btn.click();
    if (idx + 1 < steps.length) {
      setTimeout(() => runStep(idx + 1), 250);
    }
  }
  setTimeout(() => runStep(0), 150);
})();
