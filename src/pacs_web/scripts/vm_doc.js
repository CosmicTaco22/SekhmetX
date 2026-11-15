function vmDoc() {
  if (document.querySelector('#sptGeneralDetailsInput') || document.querySelector('#mainTab-1-SV_studyList') || document.querySelector('#dataTableID')) return document;
  for (const fr of document.querySelectorAll('iframe')) {
    try {
      const d = fr.contentDocument;
      if (d && (d.querySelector('#sptGeneralDetailsInput') || d.querySelector('#mainTab-1-SV_studyList') || d.querySelector('#dataTableID'))) return d;
    } catch (_) {}
  }
  return document;
}
