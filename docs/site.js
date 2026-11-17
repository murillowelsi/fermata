// Fermata · o mínimo de JavaScript. A página funciona sem ele: os links baixam, os prints abrem e a lista de versões estática fica.
(function () {
  var REPO = 'murillowelsi/fermata';
  var FILES = { mac: 'Fermata-Installer.dmg', windows: 'Fermata-Windows-x64.zip' };

  // 1. ?baixar=1 nas páginas do Mac/Windows: inicia o download e mostra "O download começou"
  var os = document.body.getAttribute('data-os');
  if (os && /[?&]baixar=1\b/.test(location.search)) {
    var banner = document.getElementById('started');
    if (banner) banner.hidden = false;
    setTimeout(function () { location.href = 'https://github.com/' + REPO + '/releases/latest/download/' + FILES[os]; }, 400);
  }

  // 2. Página inicial: no Windows, o botão do Windows vem primeiro
  var pair = document.querySelector('.cta .pair');
  if (pair) {
    var plat = navigator.userAgent + ' ' + (navigator.platform || '') + ' ' + ((navigator.userAgentData && navigator.userAgentData.platform) || '');
    if (/Windows|Win32|Win64/.test(plat)) pair.classList.add('win-first');
  }

  // 3. Ampliar os prints num <dialog> (sem JS, o link abre a imagem)
  var dlg = document.getElementById('zoom');
  if (dlg && dlg.showModal) {
    var big = dlg.querySelector('img');
    document.querySelectorAll('a.thumb').forEach(function (a) {
      a.addEventListener('click', function (e) {
        e.preventDefault();
        var img = a.querySelector('img');
        big.src = a.href; big.alt = img ? img.alt : '';
        dlg.showModal();
      });
    });
    dlg.addEventListener('click', function (e) { if (e.target === dlg) dlg.close(); });
  }

  // 4. Versões vindas do GitHub (se falhar, fica o HTML estático)
  var list = document.getElementById('releases'), latest = document.getElementById('latest');
  if (!list && !latest) return;
  fetch('https://api.github.com/repos/' + REPO + '/releases')
    .then(function (r) { if (!r.ok) throw new Error(r.status); return r.json(); })
    .then(function (rels) {
      rels = rels.filter(function (r) { return !r.draft && !r.prerelease; });
      if (!rels.length) return;
      var clean = function (body) { var l = (body || '').split('\n').map(function (x) { return x.replace(/^[#*\-\s]+/, '').trim(); }).filter(Boolean)[0] || ''; return l.replace(/\*\*|`/g, '').replace(/\s*\(beta\)/gi, '').replace(/,?\s*beta\b/gi, ''); };
      var fmt = function (d) { return new Date(d).toLocaleDateString('pt-BR', { day: 'numeric', month: 'long', year: 'numeric' }); };
      // os textos estáticos (revisados) valem; a API só acrescenta versões novas e as datas
      var known = {};
      if (list) list.querySelectorAll('li[data-tag]').forEach(function (li) { known[li.getAttribute('data-tag')] = li; });
      rels.slice().reverse().forEach(function (rel) {
        if (!list) return;
        var li = known[rel.tag_name];
        if (!li) {
          li = document.createElement('li'); li.setAttribute('data-tag', rel.tag_name);
          var v = document.createElement('span'); v.className = 'v'; v.textContent = (rel.tag_name || '').replace(/^v/, '');
          var w = document.createElement('span'); w.className = 'w'; w.textContent = clean(rel.body);
          var a = document.createElement('a'); a.href = rel.html_url; a.textContent = 'Ver e baixar';
          li.appendChild(v); li.appendChild(w); li.appendChild(a); list.insertBefore(li, list.firstChild); known[rel.tag_name] = li;
        }
        var w2 = li.querySelector('.w'), sm = w2.querySelector('small') || w2.appendChild(document.createElement('small'));
        sm.textContent = fmt(rel.published_at);
      });
      if (latest) {
        var top = rels[0], k = known[top.tag_name];
        latest.querySelector('[data-v]').textContent = (top.tag_name || '').replace(/^v/, '');
        latest.querySelector('[data-what]').textContent = k ? k.querySelector('.w').firstChild.textContent.trim() : clean(top.body);
      }
    })
    .catch(function () {});
})();

// Tema: automático (segue o sistema), claro ou escuro. Guarda a escolha no navegador.
(function () {
  var root = document.documentElement, btn = document.getElementById('theme-btn');
  var order = ['auto', 'light', 'dark'], names = { auto: 'Automático', light: 'Claro', dark: 'Escuro' };
  function current() { try { return localStorage.getItem('fermata-theme') || 'auto'; } catch (e) { return 'auto'; } }
  function apply(t) {
    if (t === 'auto') root.removeAttribute('data-theme'); else root.setAttribute('data-theme', t);
    document.querySelectorAll('picture source[data-dark]').forEach(function (s) {
      s.media = t === 'auto' ? '(prefers-color-scheme: dark)' : (t === 'dark' ? 'all' : 'not all');
    });
    if (btn) { btn.querySelector('.lbl').textContent = names[t]; btn.setAttribute('aria-label', 'Tema: ' + names[t].toLowerCase() + '. Clique para trocar.'); btn.title = 'Tema: ' + names[t]; }
  }
  document.querySelectorAll('picture source[media="(prefers-color-scheme: dark)"]').forEach(function (s) { s.setAttribute('data-dark', ''); });
  apply(current());
  if (btn) btn.addEventListener('click', function () {
    var t = order[(order.indexOf(current()) + 1) % order.length];
    try { if (t === 'auto') localStorage.removeItem('fermata-theme'); else localStorage.setItem('fermata-theme', t); } catch (e) {}
    apply(t);
  });
})();
