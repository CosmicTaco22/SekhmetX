(function() {
  const cfg = __PAYLOAD__;
  __VM_DOC__
  const doc = vmDoc();
  const lip = doc.querySelector('#mainTab-1-lipLeft, [id$="-lipLeft"]');
  const list = doc.querySelector('#mainTab-1-SV_studyList');
  if (lip && (!list || list.offsetParent === null)) {
    lip.click();
  }
  setTimeout(function() {
    const d = vmDoc();
    const cards = Array.from(d.querySelectorAll('#mainTab-1-SV_studyList div[id$="_study"]'));
    const card = cards[cfg.study_index];
    if (!card) {
      window.ipc.postMessage('DL_FAILED\tSelected study not found in Vue Motion list.');
      return;
    }
    card.click();
    setTimeout(function() {
      const d2 = vmDoc();
      const saveBtn = d2.querySelector('#mainTab-1-SV-saveAnchorSpan, #mainTab-1-SV-saveBtn');
      if (!saveBtn) {
        window.ipc.postMessage('DL_FAILED\tGuardar button not found in Vue Motion toolbar.');
        return;
      }
      saveBtn.click();
      setTimeout(function() {
        const d3 = vmDoc();
        let target = null;
        if (cfg.anonymize) {
          const links = Array.from(d3.querySelectorAll('ul.subnav.subnavpos2 a.vuitItemLink, ul.subnav a.vuitItemLink'));
          target = links.find(a => /an[oó]nimo|anonim|anonymous/i.test(a.innerText || a.textContent || ''));
        }
        if (!target) {
          target = d3.querySelector('#mainTab-1-SV-saveStudyOption a.vuitItemLink');
        }
        if (!target) {
          window.ipc.postMessage('DL_FAILED\tDownload DICOM ZIP option not found in menu.');
          return;
        }
        target.click();
      }, 250);
    }, 500);
  }, 250);
})();
