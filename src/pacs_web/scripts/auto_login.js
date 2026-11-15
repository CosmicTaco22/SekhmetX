(function() {
  const cfg = __PAYLOAD__;
  const nativeSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  function setField(el, val) {
    el.focus();
    if (nativeSetter) nativeSetter.call(el, val);
    else el.value = val;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }
  let attempts = 0;
  const timer = setInterval(function() {
    attempts++;
    if (document.querySelector('#sptGeneralDetailsInput')) {
      clearInterval(timer);
      return;
    }
    const pw = document.querySelector('input[type="password"]');
    if (pw && pw.offsetParent !== null) {
      clearInterval(timer);
      const inputs = Array.from(document.querySelectorAll('input[type="text"], input[type="email"], input:not([type])'))
        .filter(el => el.offsetParent !== null && !el.disabled && !el.readOnly);
      const userEl = inputs[0];
      if (userEl && cfg.user) setField(userEl, cfg.user);
      if (cfg.pass) setField(pw, cfg.pass);
      setTimeout(function() {
        const btn = document.querySelector('button[type="submit"], input[type="submit"], #btnLogin, .login-btn')
          || Array.from(document.querySelectorAll('button')).find(b => /log\s*in|sign\s*in|iniciar|ingresar|acceder|entrar/i.test(b.textContent || ''));
        if (btn) btn.click();
        else if (pw.form) pw.form.submit();
      }, 180);
    } else if (attempts >= 24) {
      clearInterval(timer);
    }
  }, 250);
})();
