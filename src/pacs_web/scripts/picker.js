(function() {
  const cfg = __PAYLOAD__;
  if (window.__sekhmetPickerCleanup) window.__sekhmetPickerCleanup();
  let lastEl = null;
  let lastOutline = '';
  function cssEscape(s) {
    return (window.CSS && CSS.escape) ? CSS.escape(s) : s.replace(/([^\w-])/g, '\\$1');
  }
  function selectorFor(el, generalizeRow) {
    if (!el || el.nodeType !== 1) return '';
    const parts = [];
    let cur = el;
    while (cur && cur.nodeType === 1 && cur.tagName.toLowerCase() !== 'html') {
      if (cur.id && !generalizeRow) {
        parts.unshift('#' + cssEscape(cur.id));
        break;
      }
      let seg = cur.tagName.toLowerCase();
      if (cur.classList && cur.classList.length > 0) {
        const cls = Array.from(cur.classList).filter(c => !c.startsWith('sekhmet')).slice(0, 2);
        if (cls.length) seg += '.' + cls.map(cssEscape).join('.');
      }
      if (generalizeRow && cur === el) {
        // Generalize repeating row element (no :nth-of-type on the row itself)
      } else if (cur.parentElement) {
        const sibs = Array.from(cur.parentElement.children).filter(c => c.tagName === cur.tagName);
        if (sibs.length > 1) {
          seg += ':nth-of-type(' + (sibs.indexOf(cur) + 1) + ')';
        }
      }
      parts.unshift(seg);
      cur = cur.parentElement;
    }
    return parts.join(' > ');
  }
  function relativeSelector(ancestor, desc) {
    const parts = [];
    let cur = desc;
    while (cur && cur !== ancestor && cur.nodeType === 1) {
      let seg = cur.tagName.toLowerCase();
      if (cur.classList && cur.classList.length > 0) {
        const cls = Array.from(cur.classList).filter(c => !c.startsWith('sekhmet')).slice(0, 2);
        if (cls.length) seg += '.' + cls.map(cssEscape).join('.');
      }
      if (cur.parentElement) {
        const sibs = Array.from(cur.parentElement.children).filter(c => c.tagName === cur.tagName);
        if (sibs.length > 1) seg += ':nth-of-type(' + (sibs.indexOf(cur) + 1) + ')';
      }
      parts.unshift(seg);
      cur = cur.parentElement;
    }
    return parts.join(' > ');
  }
  function onMove(e) {
    if (lastEl && lastEl !== e.target) lastEl.style.outline = lastOutline;
    lastEl = e.target;
    lastOutline = lastEl.style.outline || '';
    lastEl.style.outline = '3px solid #22c55e';
  }
  function onClick(e) {
    const multiStep = (cfg.step === 'download' || cfg.step === 'anon');
    if (multiStep && !(e.ctrlKey || e.metaKey)) {
      return; // Allow normal clicks to open dropdown menus / popups!
    }
    e.preventDefault();
    e.stopPropagation();
    let target = e.target;
    if (cfg.step === 'row') {
      const row = target.closest('tr[id], tbody > tr, li, [role="row"], .row');
      if (row) target = row;
    }
    const sel = selectorFor(target, false);
    const rowSel = (cfg.step === 'row') ? selectorFor(target, true) : '';
    let inRow = false;
    let relSel = '';
    if (cfg.row_sel) {
      try {
        const matchedRow = target.closest(cfg.row_sel);
        if (matchedRow) {
          inRow = true;
          relSel = relativeSelector(matchedRow, target);
        }
      } catch (_) {}
    }
    if (!multiStep) cleanup();
    window.ipc.postMessage('PICKED\t' + JSON.stringify({
      step: cfg.step,
      selector: sel,
      row_selector: rowSel,
      in_row: inRow,
      rel_selector: relSel
    }));
  }
  function cleanup() {
    if (lastEl) lastEl.style.outline = lastOutline;
    document.removeEventListener('mouseover', onMove, true);
    document.removeEventListener('click', onClick, true);
    window.__sekhmetPickerCleanup = null;
  }
  document.addEventListener('mouseover', onMove, true);
  document.addEventListener('click', onClick, true);
  window.__sekhmetPickerCleanup = cleanup;
})();
