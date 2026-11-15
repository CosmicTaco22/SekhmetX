(function() {
  const cfg = __PAYLOAD__;
  __VM_DOC__
  const doc = vmDoc();
  let rows = Array.from(doc.querySelectorAll('#dataTableID tr[id^="ptRecord_"]'));
  if (rows.length === 0) {
    rows = Array.from(doc.querySelectorAll('#dataTableID tbody tr')).filter(r => r.querySelector('td'));
  }
  const row = rows[cfg.patient_index];
  if (!row) {
    window.ipc.postMessage('SEARCH_ERR\tPatient row no longer visible.');
    return;
  }
  const link = row.querySelector('a.dtPatientLink') || row.querySelector('a');
  if (!link) {
    window.ipc.postMessage('SEARCH_ERR\tPatient hyperlink not found.');
    return;
  }
  link.click();
  let tries = 0;
  let lipClicked = false;
  const t = setInterval(function() {
    tries++;
    const d = vmDoc();
    const lip = d.querySelector('#mainTab-1-lipLeft, [id$="-lipLeft"]');
    const list = d.querySelector('#mainTab-1-SV_studyList');
    if (lip && (!list || list.offsetParent === null) && !lipClicked) {
      lipClicked = true;
      lip.click();
    }
    const cards = Array.from(d.querySelectorAll('#mainTab-1-SV_studyList div[id$="_study"]')).slice(0, 50);
    if (cards.length > 0 || tries >= 35) {
      clearInterval(t);
      const out = cards.map((card, i) => {
        const infoEl = card.querySelector('.scc1') || card;
        const lines = (infoEl.innerText || '').split('\n').map(s => s.trim()).filter(Boolean);
        const date = lines[0] || '';
        const modality = lines[1] || '';
        const description = lines.slice(2).join(' ') || lines.join(' ');
        const icon = card.querySelector('span[id$="_studyIcon"]');
        const has_report = !!(icon && (icon.classList.contains('report') || (icon.title || '').toLowerCase().includes('informe')));
        return { index: i, date, modality, description, has_report };
      });
      window.ipc.postMessage('PATIENT_STUDIES\t' + JSON.stringify(out));
    }
  }, 300);
})();
