// The documentation site's one script: the search box, the section menu on
// a phone, the heading that is being read, and a Copy button on code.
// Nothing here is needed to read a page.
(function () {
  'use strict';
  var root = window.MS_ROOT || '.';
  var input = document.getElementById('search');
  var results = document.getElementById('results');
  var selected = -1;
  var loading = null;

  function loadIndex() {
    if (window.MS_SEARCH) return Promise.resolve();
    if (!loading) {
      loading = new Promise(function (resolve) {
        var script = document.createElement('script');
        script.src = root + '/assets/search-index.js';
        script.onload = resolve;
        script.onerror = resolve;
        document.head.appendChild(script);
      });
    }
    return loading;
  }

  function score(entry, words) {
    var title = (entry.t + ' ' + entry.h).toLowerCase();
    var text = entry.x.toLowerCase();
    var total = 0;
    for (var i = 0; i < words.length; i++) {
      var word = words[i];
      var inTitle = title.indexOf(word);
      if (inTitle >= 0) total += entry.h.toLowerCase().indexOf(word) >= 0 || !entry.h ? 12 : 8;
      else if (text.indexOf(word) >= 0) total += 3;
      else return 0;
    }
    if (!entry.h) total += 2;
    return total;
  }

  function show() {
    var query = input.value.trim().toLowerCase();
    selected = -1;
    results.textContent = '';
    if (!query) { results.hidden = true; return; }
    var words = query.split(/\s+/);
    var found = [];
    var index = window.MS_SEARCH || [];
    for (var i = 0; i < index.length; i++) {
      var points = score(index[i], words);
      if (points) found.push([points, i]);
    }
    found.sort(function (a, b) { return b[0] - a[0] || a[1] - b[1]; });
    results.hidden = false;
    if (!found.length) {
      var none = document.createElement('p');
      none.textContent = index.length ? 'Nothing found for "' + input.value.trim() + '".' : 'The search index could not be loaded.';
      results.appendChild(none);
      return;
    }
    found.slice(0, 12).forEach(function (pair) {
      var entry = index[pair[1]];
      var link = document.createElement('a');
      link.href = root + '/' + entry.u;
      link.setAttribute('role', 'option');
      var where = document.createElement('span');
      where.className = 'where';
      where.textContent = entry.s + ' / ' + entry.t;
      var what = document.createElement('span');
      what.className = 'what';
      what.textContent = entry.h || entry.t;
      var text = document.createElement('span');
      text.className = 'text';
      text.textContent = entry.x;
      link.appendChild(where); link.appendChild(what); link.appendChild(text);
      results.appendChild(link);
    });
  }

  function move(by) {
    var links = results.querySelectorAll('a');
    if (!links.length) return;
    if (selected >= 0) links[selected].classList.remove('sel');
    selected = (selected + by + links.length) % links.length;
    links[selected].classList.add('sel');
    links[selected].scrollIntoView({ block: 'nearest' });
  }

  if (input && results) {
    input.addEventListener('focus', function () { loadIndex().then(show); });
    input.addEventListener('input', function () { loadIndex().then(show); });
    input.addEventListener('keydown', function (event) {
      if (event.key === 'ArrowDown') { event.preventDefault(); move(1); }
      else if (event.key === 'ArrowUp') { event.preventDefault(); move(-1); }
      else if (event.key === 'Enter') {
        var links = results.querySelectorAll('a');
        var link = links[selected >= 0 ? selected : 0];
        if (link) { event.preventDefault(); window.location.href = link.href; }
      } else if (event.key === 'Escape') { input.value = ''; results.hidden = true; input.blur(); }
    });
    document.addEventListener('keydown', function (event) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') { event.preventDefault(); input.focus(); input.select(); }
    });
    document.addEventListener('click', function (event) {
      if (!event.target.closest('.srch')) results.hidden = true;
    });
  }

  // The section's pages, behind a button on a narrow screen.
  var menu = document.querySelector('.menu');
  if (menu) {
    menu.addEventListener('click', function () {
      var open = document.body.classList.toggle('nav-open');
      menu.setAttribute('aria-expanded', open ? 'true' : 'false');
    });
  }
  var current = document.querySelector('.nav .ni.on');
  if (current && current.scrollIntoView) current.scrollIntoView({ block: 'nearest' });

  // Which heading is being read, in "On this page".
  var entries = Array.prototype.slice.call(document.querySelectorAll('.toc .ti'));
  var headings = entries.map(function (entry) { return document.getElementById(decodeURIComponent(entry.getAttribute('href').slice(1))); });
  function mark() {
    var at = -1;
    for (var i = 0; i < headings.length; i++) {
      if (headings[i] && headings[i].getBoundingClientRect().top <= 110) at = i;
    }
    entries.forEach(function (entry, i) { entry.classList.toggle('on', i === Math.max(at, 0)); });
  }
  if (entries.length) {
    var waiting = false;
    window.addEventListener('scroll', function () {
      if (waiting) return;
      waiting = true;
      window.requestAnimationFrame(function () { waiting = false; mark(); });
    }, { passive: true });
    mark();
  }

  // Copy on every code block, where the browser allows it.
  if (navigator.clipboard && window.isSecureContext !== false) {
    Array.prototype.forEach.call(document.querySelectorAll('.code'), function (block) {
      var button = document.createElement('button');
      button.type = 'button';
      button.className = 'copy';
      button.textContent = 'Copy';
      button.addEventListener('click', function () {
        navigator.clipboard.writeText(block.querySelector('pre').innerText).then(function () {
          button.textContent = 'Copied';
          setTimeout(function () { button.textContent = 'Copy'; }, 1500);
        }, function () { button.textContent = 'Not copied'; });
      });
      block.appendChild(button);
    });
  }

  // "Copy for LLM": the page as Markdown, with its samples and tables as they
  // are shown (already expanded from the source), ready to paste into a chat.
  function pageAsMarkdown() {
    var article = document.querySelector('main article');
    function clean(text) { return text.replace(/\s+/g, ' '); }
    function inline(node) {
      var out = '';
      Array.prototype.forEach.call(node.childNodes, function (child) {
        if (child.nodeType === 3) { out += clean(child.nodeValue); return; }
        if (child.nodeType !== 1) return;
        var tag = child.tagName.toLowerCase();
        if (tag === 'svg' || tag === 'button' || tag === 'script') return;
        if (tag === 'code') out += '`' + child.textContent + '`';
        else if (tag === 'strong' || tag === 'b') out += '**' + inline(child).trim() + '**';
        else if (tag === 'em' || tag === 'i') out += '*' + inline(child).trim() + '*';
        else if (tag === 'a') {
          // The link as written (relative), never the resolved address: on a
          // local copy that would be a path on this machine.
          var href = child.getAttribute('href') || '';
          out += href && href.charAt(0) !== '#' ? '[' + inline(child).trim() + '](' + href + ')' : inline(child);
        }
        else if (tag === 'br') out += '\n';
        else if (tag === 'img') out += child.alt ? '[' + child.alt + ']' : '';
        else out += inline(child);
      });
      return out;
    }
    function cell(node) { return inline(node).trim().replace(/\|/g, '\\|').replace(/\n/g, ' '); }
    function list(node, depth) {
      var ordered = node.tagName.toLowerCase() === 'ol', n = 0, out = '';
      Array.prototype.forEach.call(node.children, function (item) {
        if (item.tagName.toLowerCase() !== 'li') return;
        n += 1;
        var own = '', nested = '';
        Array.prototype.forEach.call(item.childNodes, function (child) {
          var tag = child.nodeType === 1 ? child.tagName.toLowerCase() : '';
          if (tag === 'ul' || tag === 'ol') nested += list(child, depth + 1);
          else if (tag === 'p' || tag === 'div') own += ' ' + inline(child);
          else if (child.nodeType === 3) own += clean(child.nodeValue);
          else if (child.nodeType === 1) { var holder = document.createElement('span'); holder.appendChild(child.cloneNode(true)); own += inline(holder); }
        });
        out += new Array(depth + 1).join('  ') + (ordered ? n + '. ' : '- ') + own.trim() + '\n' + nested;
      });
      return out;
    }
    function block(node) {
      var out = '';
      Array.prototype.forEach.call(node.children, function (el) {
        var tag = el.tagName.toLowerCase(), cls = el.classList;
        if (/^h[1-6]$/.test(tag)) out += new Array(Number(tag[1]) + 1).join('#') + ' ' + inline(el).trim() + '\n\n';
        else if (tag === 'p') out += inline(el).trim() + '\n\n';
        else if (tag === 'ul' || tag === 'ol') out += list(el, 0) + '\n';
        else if (cls.contains('code')) {
          var head = el.querySelector('.code-head');
          if (head) out += '`' + head.textContent.trim() + '`:\n\n';
          out += '```' + (el.getAttribute('data-lang') || '') + '\n' + el.querySelector('pre').textContent.replace(/\n+$/, '') + '\n```\n\n';
        } else if (tag === 'pre') out += '```\n' + el.textContent.replace(/\n+$/, '') + '\n```\n\n';
        else if (cls.contains('table') || tag === 'table') {
          var rows = el.querySelectorAll('tr');
          Array.prototype.forEach.call(rows, function (row, at) {
            var cells = Array.prototype.map.call(row.children, cell);
            out += '| ' + cells.join(' | ') + ' |\n';
            if (at === 0) out += '|' + cells.map(function () { return ' --- '; }).join('|') + '|\n';
          });
          out += '\n';
        } else if (cls.contains('note') || tag === 'blockquote') {
          out += block(el).trim().split('\n').map(function (line) { return '> ' + line; }).join('\n') + '\n\n';
        } else if (cls.contains('cards')) {
          Array.prototype.forEach.call(el.querySelectorAll('.card'), function (card) {
            var title = card.querySelector('.card-title'), text = card.querySelector('.card-text');
            out += '- [' + (title ? title.textContent.trim() : '') + '](' + (card.getAttribute('href') || '') + ')' + (text ? ': ' + clean(text.textContent).trim() : '') + '\n';
          });
          out += '\n';
        } else if (cls.contains('shots') || tag === 'figure') {
          Array.prototype.forEach.call(el.querySelectorAll('img'), function (img) { out += '- ' + (img.alt || 'Picture') + ' (' + (img.getAttribute('src') || '') + ')\n'; });
          out += '\n';
        } else if (tag === 'hr') out += '---\n\n';
        else if (el.children.length) out += block(el);
        else if (el.textContent.trim()) out += inline(el).trim() + '\n\n';
      });
      return out;
    }
    var source = document.querySelector('.foot code:last-of-type');
    return block(article).replace(/\n{3,}/g, '\n\n').trim() +
      '\n\n---\nMorningstar documentation' + (source ? ', page source: ' + source.textContent : '') + '\n';
  }
  function copyText(text) {
    if (navigator.clipboard && window.isSecureContext !== false) return navigator.clipboard.writeText(text);
    return new Promise(function (resolve, reject) {
      var area = document.createElement('textarea');
      area.value = text; area.setAttribute('readonly', ''); area.style.position = 'fixed'; area.style.opacity = '0';
      document.body.appendChild(area); area.select();
      var ok = false;
      try { ok = document.execCommand('copy'); } catch (e) { ok = false; }
      area.remove();
      if (ok) resolve(); else reject(new Error('copy refused'));
    });
  }
  var crumbs = document.querySelector('main .crumbs');
  if (crumbs && document.querySelector('main article')) {
    var llm = document.createElement('button');
    llm.type = 'button';
    llm.className = 'llm';
    llm.title = 'Copy this page as Markdown, to paste into an AI assistant';
    llm.textContent = 'Copy for LLM';
    llm.addEventListener('click', function () {
      copyText(pageAsMarkdown()).then(function () { llm.textContent = 'Copied as Markdown'; }, function () { llm.textContent = 'Not copied'; })
        .then(function () { setTimeout(function () { llm.textContent = 'Copy for LLM'; }, 1800); });
    });
    crumbs.appendChild(llm);
  }
})();
