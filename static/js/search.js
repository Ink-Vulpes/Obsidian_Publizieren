/* search.js — keyboard-driven search overlay */

document.addEventListener('DOMContentLoaded', function () {
  const overlay = document.getElementById('search-overlay');
  const backdrop = document.getElementById('search-backdrop');
  const input = document.getElementById('search-bar');
  const results = document.getElementById('search-results');
  const trigger = document.getElementById('search-trigger');
  const closeKey = document.getElementById('search-close-key');

  if (!overlay || !input) return;

  let debounceTimer = null;
  let focusedIndex = -1;

  function open() {
    overlay.classList.remove('hidden');
    input.value = '';
    results.innerHTML = '';
    focusedIndex = -1;
    setTimeout(() => input.focus(), 50);
  }

  function close() {
    overlay.classList.add('hidden');
    input.blur();
  }

  // Triggers
  if (trigger) trigger.addEventListener('click', open);
  if (backdrop) backdrop.addEventListener('click', close);
  if (closeKey) closeKey.addEventListener('click', close);

  // Keyboard shortcut Ctrl+K / Cmd+K
  document.addEventListener('keydown', e => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
      e.preventDefault();
      overlay.classList.contains('hidden') ? open() : close();
    }
    if (e.key === 'Escape') close();
  });

  // Arrow keys inside overlay
  overlay.addEventListener('keydown', e => {
    const items = results.querySelectorAll('.search-result');
    if (!items.length) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      focusedIndex = Math.min(focusedIndex + 1, items.length - 1);
      updateFocus(items);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      focusedIndex = Math.max(focusedIndex - 1, 0);
      updateFocus(items);
    } else if (e.key === 'Enter') {
      const focused = results.querySelector('.search-result.focused');
      if (focused) {
        window.location.href = focused.getAttribute('href');
        close();
      }
    }
  });

  function updateFocus(items) {
    items.forEach((item, i) => item.classList.toggle('focused', i === focusedIndex));
    if (focusedIndex >= 0) items[focusedIndex].scrollIntoView({ block: 'nearest' });
  }

  // Search on input
  input.addEventListener('input', () => {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => runSearch(input.value.trim()), 180);
  });

  async function runSearch(q) {
    focusedIndex = -1;
    if (!q) { results.innerHTML = ''; return; }
    results.innerHTML = '<div class="search-no-results">Searching…</div>';
    try {
      const res = await fetch('/api/search?q=' + encodeURIComponent(q));
      const data = await res.json();
      renderResults(data, q);
    } catch {
      results.innerHTML = '<div class="search-no-results">Search error.</div>';
    }
  }

  function renderResults(data, q) {
    if (!data.length) {
      results.innerHTML = `<div class="search-no-results">No results for "<strong>${esc(q)}</strong>"</div>`;
      return;
    }
    results.innerHTML = '';
    data.forEach(item => {
      const a = document.createElement('a');
      a.className = 'search-result';
      a.href = '/note/' + item.slug;
      a.addEventListener('click', close);

      const title = document.createElement('div');
      title.className = 'search-result-title';
      title.innerHTML = highlight(esc(item.title), q);

      const excerpt = document.createElement('div');
      excerpt.className = 'search-result-excerpt';
      excerpt.innerHTML = highlight(esc(item.excerpt), q);

      a.appendChild(title);
      if (item.excerpt) a.appendChild(excerpt);

      if (item.tags && item.tags.length) {
        const tags = document.createElement('div');
        tags.className = 'search-result-tags';
        item.tags.slice(0, 5).forEach(t => {
          const span = document.createElement('span');
          span.className = 'tag';
          span.textContent = '#' + t;
          tags.appendChild(span);
        });
        a.appendChild(tags);
      }

      results.appendChild(a);
    });
  }

  function highlight(text, q) {
    if (!q) return text;
    const re = new RegExp('(' + q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'gi');
    return text.replace(re, '<mark>$1</mark>');
  }

  function esc(s) {
    return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
  }
});
