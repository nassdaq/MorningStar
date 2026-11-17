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
})();
