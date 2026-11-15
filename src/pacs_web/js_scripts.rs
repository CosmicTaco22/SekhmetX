pub(super) fn build_picker_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  if (window.__sekhmetPickerCleanup) window.__sekhmetPickerCleanup();
  let lastEl = null;
  let lastOutline = '';
  function cssEscape(s) {{
    return (window.CSS && CSS.escape) ? CSS.escape(s) : s.replace(/([^\w-])/g, '\\$1');
  }}
  function selectorFor(el, generalizeRow) {{
    if (!el || el.nodeType !== 1) return '';
    const parts = [];
    let cur = el;
    while (cur && cur.nodeType === 1 && cur.tagName.toLowerCase() !== 'html') {{
      if (cur.id && !generalizeRow) {{
        parts.unshift('#' + cssEscape(cur.id));
        break;
      }}
      let seg = cur.tagName.toLowerCase();
      if (cur.classList && cur.classList.length > 0) {{
        const cls = Array.from(cur.classList).filter(c => !c.startsWith('sekhmet')).slice(0, 2);
        if (cls.length) seg += '.' + cls.map(cssEscape).join('.');
      }}
      if (generalizeRow && cur === el) {{
        // Generalize repeating row element (no :nth-of-type on the row itself)
      }} else if (cur.parentElement) {{
        const sibs = Array.from(cur.parentElement.children).filter(c => c.tagName === cur.tagName);
        if (sibs.length > 1) {{
          seg += ':nth-of-type(' + (sibs.indexOf(cur) + 1) + ')';
        }}
      }}
      parts.unshift(seg);
      cur = cur.parentElement;
    }}
    return parts.join(' > ');
  }}
  function relativeSelector(ancestor, desc) {{
    const parts = [];
    let cur = desc;
    while (cur && cur !== ancestor && cur.nodeType === 1) {{
      let seg = cur.tagName.toLowerCase();
      if (cur.classList && cur.classList.length > 0) {{
        const cls = Array.from(cur.classList).filter(c => !c.startsWith('sekhmet')).slice(0, 2);
        if (cls.length) seg += '.' + cls.map(cssEscape).join('.');
      }}
      if (cur.parentElement) {{
        const sibs = Array.from(cur.parentElement.children).filter(c => c.tagName === cur.tagName);
        if (sibs.length > 1) seg += ':nth-of-type(' + (sibs.indexOf(cur) + 1) + ')';
      }}
      parts.unshift(seg);
      cur = cur.parentElement;
    }}
    return parts.join(' > ');
  }}
  function onMove(e) {{
    if (lastEl && lastEl !== e.target) lastEl.style.outline = lastOutline;
    lastEl = e.target;
    lastOutline = lastEl.style.outline || '';
    lastEl.style.outline = '3px solid #22c55e';
  }}
  function onClick(e) {{
    const multiStep = (cfg.step === 'download' || cfg.step === 'anon');
    if (multiStep && !(e.ctrlKey || e.metaKey)) {{
      return; // Allow normal clicks to open dropdown menus / popups!
    }}
    e.preventDefault();
    e.stopPropagation();
    let target = e.target;
    if (cfg.step === 'row') {{
      const row = target.closest('tr[id], tbody > tr, li, [role="row"], .row');
      if (row) target = row;
    }}
    const sel = selectorFor(target, false);
    const rowSel = (cfg.step === 'row') ? selectorFor(target, true) : '';
    let inRow = false;
    let relSel = '';
    if (cfg.row_sel) {{
      try {{
        const matchedRow = target.closest(cfg.row_sel);
        if (matchedRow) {{
          inRow = true;
          relSel = relativeSelector(matchedRow, target);
        }}
      }} catch (_) {{}}
    }}
    if (!multiStep) cleanup();
    window.ipc.postMessage('PICKED\t' + JSON.stringify({{
      step: cfg.step,
      selector: sel,
      row_selector: rowSel,
      in_row: inRow,
      rel_selector: relSel
    }}));
  }}
  function cleanup() {{
    if (lastEl) lastEl.style.outline = lastOutline;
    document.removeEventListener('mouseover', onMove, true);
    document.removeEventListener('click', onClick, true);
    window.__sekhmetPickerCleanup = null;
  }}
  document.addEventListener('mouseover', onMove, true);
  document.addEventListener('click', onClick, true);
  window.__sekhmetPickerCleanup = cleanup;
}})();"#
    )
}

pub(super) fn build_trained_search_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  const p = cfg.profile;
  const input = document.querySelector(p.search_input_sel);
  if (!input) {{
    window.ipc.postMessage('SEARCH_ERR\tSearch input not found on page.');
    return;
  }}
  input.focus();
  input.value = cfg.query || '';
  input.dispatchEvent(new Event('input', {{ bubbles: true }}));
  input.dispatchEvent(new Event('change', {{ bubbles: true }}));
  input.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
  input.dispatchEvent(new KeyboardEvent('keyup', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
  if (input.form) {{
    const btn = input.form.querySelector('button[type="submit"], input[type="submit"], button');
    if (btn) btn.click();
    else if (typeof input.form.requestSubmit === 'function') input.form.requestSubmit();
  }}
  let tries = 0;
  const timer = setInterval(function() {{
    tries++;
    const rows = Array.from(document.querySelectorAll(p.row_list_sel)).slice(0, 50);
    if (rows.length > 0 || tries >= 20) {{
      clearInterval(timer);
      const out = rows.map((r, i) => ({{
        index: i,
        text: (r.innerText || r.textContent || '').replace(/\s+/g, ' ').trim()
      }})).filter(x => x.text.length > 0);
      window.ipc.postMessage('SEARCH_RESULTS\t' + JSON.stringify(out));
    }}
  }}, 250);
}})();"#
    )
}

pub(super) fn build_trained_download_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  const p = cfg.profile;
  const rows = Array.from(document.querySelectorAll(p.row_list_sel));
  const row = rows[cfg.row_index];
  if (!row) {{
    window.ipc.postMessage('DL_FAILED\tSelected study row is no longer visible on page.');
    return;
  }}
  row.click();
  const useAnon = !!(cfg.anonymize && p.anon_steps && p.anon_steps.length > 0);
  const steps = useAnon ? p.anon_steps : (p.download_steps || []);
  if (steps.length === 0) {{
    window.ipc.postMessage('DL_FAILED\tNo download steps trained for this portal.');
    return;
  }}
  function runStep(idx) {{
    if (idx >= steps.length) return;
    const st = steps[idx];
    const sel = (st.in_row && st.rel_selector) ? st.rel_selector : st.selector;
    const btn = (st.in_row && st.rel_selector) ? row.querySelector(sel) : document.querySelector(sel);
    if (!btn) {{
      window.ipc.postMessage('DL_FAILED\tDownload step #' + (idx + 1) + ' button not found on page.');
      return;
    }}
    btn.click();
    if (idx + 1 < steps.length) {{
      setTimeout(() => runStep(idx + 1), 250);
    }}
  }}
  setTimeout(() => runStep(0), 150);
}})();"#
    )
}

const VM_DOC_HELPER: &str = r#"function vmDoc() {
    if (document.querySelector('#sptGeneralDetailsInput') || document.querySelector('#mainTab-1-SV_studyList') || document.querySelector('#dataTableID')) return document;
    for (const fr of document.querySelectorAll('iframe')) {
      try {
        const d = fr.contentDocument;
        if (d && (d.querySelector('#sptGeneralDetailsInput') || d.querySelector('#mainTab-1-SV_studyList') || d.querySelector('#dataTableID'))) return d;
      } catch (_) {}
    }
    return document;
  }"#;

pub(super) fn build_vuemotion_search_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  {VM_DOC_HELPER}
  function getRows(doc) {{
    let rows = Array.from(doc.querySelectorAll('#dataTableID tr[id^="ptRecord_"]'));
    if (rows.length === 0) {{
      rows = Array.from(doc.querySelectorAll('#dataTableID tbody tr')).filter(r => r.querySelector('td'));
    }}
    return rows.slice(0, 50);
  }}
  function runSearch() {{
    const doc = vmDoc();
    const input = doc.querySelector('#sptGeneralDetailsInput');
    const btn = doc.querySelector('#sptSearchSubmitBtn');
    if (!input) return false;
    const prevKey = getRows(doc)
      .map(r => (r.id || '') + ':' + (r.innerText || '').slice(0, 40))
      .join('|');
    input.focus();
    try {{
      const win = doc.defaultView || window;
      const nativeSetter = Object.getOwnPropertyDescriptor(win.HTMLInputElement.prototype, 'value').set;
      if (nativeSetter) nativeSetter.call(input, cfg.query || '');
      else input.value = cfg.query || '';
    }} catch (_) {{
      input.value = cfg.query || '';
    }}
    input.dispatchEvent(new Event('input', {{ bubbles: true }}));
    input.dispatchEvent(new Event('change', {{ bubbles: true }}));
    if (btn) btn.click();
    else if (input.form) input.form.submit();
    else input.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', keyCode: 13, bubbles: true }}));
    let tries = 0;
    const t = setInterval(function() {{
      tries++;
      const d = vmDoc();
      const rows = getRows(d);
      const curKey = rows.map(r => (r.id || '') + ':' + (r.innerText || '').slice(0, 40)).join('|');
      const changed = curKey !== prevKey;
      if ((rows.length > 0 && (changed || tries >= 6)) || tries >= 24) {{
        clearInterval(t);
        const out = rows.map((r, i) => {{
          const link = r.querySelector('a.dtPatientLink') || r.querySelector('a');
          const name = (link ? (link.innerText || link.title || '') : '').replace(/\s+/g, ' ').trim();
          const cells = Array.from(r.querySelectorAll('td'))
            .map(td => (td.innerText || '').replace(/\s+/g, ' ').trim())
            .filter(s => s.length > 0 && s !== '.' && s !== '-');
          return {{ index: i, name: name || cells[0] || ('Patient #' + (i + 1)), summary: cells.join(' · ') }};
        }});
        window.ipc.postMessage('PATIENT_RESULTS\t' + JSON.stringify(out));
      }}
    }}, 250);
    return true;
  }}
  if (!runSearch()) {{
    const doc = vmDoc();
    const searchTab = doc.querySelector('#mainTab-0, a[href*="default.aspx"]');
    if (searchTab) searchTab.click();
    else if (cfg.login_url) window.location.href = cfg.login_url;
    let w = 0;
    const wt = setInterval(function() {{
      w++;
      if (runSearch() || w >= 20) {{
        clearInterval(wt);
        if (w >= 20) window.ipc.postMessage('SEARCH_ERR\tVue Motion search bar not found.');
      }}
    }}, 300);
  }}
}})();"#
    )
}

pub(super) fn build_vuemotion_open_patient_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  {VM_DOC_HELPER}
  const doc = vmDoc();
  let rows = Array.from(doc.querySelectorAll('#dataTableID tr[id^="ptRecord_"]'));
  if (rows.length === 0) {{
    rows = Array.from(doc.querySelectorAll('#dataTableID tbody tr')).filter(r => r.querySelector('td'));
  }}
  const row = rows[cfg.patient_index];
  if (!row) {{
    window.ipc.postMessage('SEARCH_ERR\tPatient row no longer visible.');
    return;
  }}
  const link = row.querySelector('a.dtPatientLink') || row.querySelector('a');
  if (!link) {{
    window.ipc.postMessage('SEARCH_ERR\tPatient hyperlink not found.');
    return;
  }}
  link.click();
  let tries = 0;
  let lipClicked = false;
  const t = setInterval(function() {{
    tries++;
    const d = vmDoc();
    const lip = d.querySelector('#mainTab-1-lipLeft, [id$="-lipLeft"]');
    const list = d.querySelector('#mainTab-1-SV_studyList');
    if (lip && (!list || list.offsetParent === null) && !lipClicked) {{
      lipClicked = true;
      lip.click();
    }}
    const cards = Array.from(d.querySelectorAll('#mainTab-1-SV_studyList div[id$="_study"]')).slice(0, 50);
    if (cards.length > 0 || tries >= 35) {{
      clearInterval(t);
      const out = cards.map((card, i) => {{
        const infoEl = card.querySelector('.scc1') || card;
        const lines = (infoEl.innerText || '').split('\n').map(s => s.trim()).filter(Boolean);
        const date = lines[0] || '';
        const modality = lines[1] || '';
        const description = lines.slice(2).join(' ') || lines.join(' ');
        const icon = card.querySelector('span[id$="_studyIcon"]');
        const has_report = !!(icon && (icon.classList.contains('report') || (icon.title || '').toLowerCase().includes('informe')));
        return {{ index: i, date, modality, description, has_report }};
      }});
      window.ipc.postMessage('PATIENT_STUDIES\t' + JSON.stringify(out));
    }}
  }}, 300);
}})();"#
    )
}

pub(super) fn build_vuemotion_download_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  {VM_DOC_HELPER}
  const doc = vmDoc();
  const lip = doc.querySelector('#mainTab-1-lipLeft, [id$="-lipLeft"]');
  const list = doc.querySelector('#mainTab-1-SV_studyList');
  if (lip && (!list || list.offsetParent === null)) {{
    lip.click();
  }}
  setTimeout(function() {{
    const d = vmDoc();
    const cards = Array.from(d.querySelectorAll('#mainTab-1-SV_studyList div[id$="_study"]'));
    const card = cards[cfg.study_index];
    if (!card) {{
      window.ipc.postMessage('DL_FAILED\tSelected study not found in Vue Motion list.');
      return;
    }}
    card.click();
    setTimeout(function() {{
      const d2 = vmDoc();
      const saveBtn = d2.querySelector('#mainTab-1-SV-saveAnchorSpan, #mainTab-1-SV-saveBtn');
      if (!saveBtn) {{
        window.ipc.postMessage('DL_FAILED\tGuardar button not found in Vue Motion toolbar.');
        return;
      }}
      saveBtn.click();
      setTimeout(function() {{
        const d3 = vmDoc();
        let target = null;
        if (cfg.anonymize) {{
          const links = Array.from(d3.querySelectorAll('ul.subnav.subnavpos2 a.vuitItemLink, ul.subnav a.vuitItemLink'));
          target = links.find(a => /an[oó]nimo|anonim|anonymous/i.test(a.innerText || a.textContent || ''));
        }}
        if (!target) {{
          target = d3.querySelector('#mainTab-1-SV-saveStudyOption a.vuitItemLink');
        }}
        if (!target) {{
          window.ipc.postMessage('DL_FAILED\tDownload DICOM ZIP option not found in menu.');
          return;
        }}
        target.click();
      }}, 250);
    }}, 500);
  }}, 250);
}})();"#
    )
}

pub(super) fn build_auto_login_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  const nativeSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  function setField(el, val) {{
    el.focus();
    if (nativeSetter) nativeSetter.call(el, val);
    else el.value = val;
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }}
  let attempts = 0;
  const timer = setInterval(function() {{
    attempts++;
    if (document.querySelector('#sptGeneralDetailsInput')) {{
      clearInterval(timer);
      return;
    }}
    const pw = document.querySelector('input[type="password"]');
    if (pw && pw.offsetParent !== null) {{
      clearInterval(timer);
      const inputs = Array.from(document.querySelectorAll('input[type="text"], input[type="email"], input:not([type])'))
        .filter(el => el.offsetParent !== null && !el.disabled && !el.readOnly);
      const userEl = inputs[0];
      if (userEl && cfg.user) setField(userEl, cfg.user);
      if (cfg.pass) setField(pw, cfg.pass);
      setTimeout(function() {{
        const btn = document.querySelector('button[type="submit"], input[type="submit"], #btnLogin, .login-btn')
          || Array.from(document.querySelectorAll('button')).find(b => /log\s*in|sign\s*in|iniciar|ingresar|acceder|entrar/i.test(b.textContent || ''));
        if (btn) btn.click();
        else if (pw.form) pw.form.submit();
      }}, 180);
    }} else if (attempts >= 24) {{
      clearInterval(timer);
    }}
  }}, 250);
}})();"#
    )
}

