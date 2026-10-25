/* app.js — dark mode, explorer, callout folds, TOC highlight */

// ── Dark mode ────────────────────────────────────────────────────────────────
(function initTheme() {
  const stored = localStorage.getItem('theme');
  const preferred = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  const theme = stored || preferred;
  document.documentElement.setAttribute('data-theme', theme);
})();

document.addEventListener('DOMContentLoaded', function () {
  // ── Dark mode toggle ───────────────────────────────────────────────────────
  const toggle = document.getElementById('darkmode-toggle');
  const sunIcon = document.getElementById('icon-sun');
  const moonIcon = document.getElementById('icon-moon');

  function applyTheme(theme) {
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('theme', theme);
    if (sunIcon && moonIcon) {
      sunIcon.style.display = theme === 'dark' ? 'none' : '';
      moonIcon.style.display = theme === 'dark' ? '' : 'none';
    }
  }

  // Initialize icon state
  const currentTheme = document.documentElement.getAttribute('data-theme') || 'dark';
  applyTheme(currentTheme);

  if (toggle) {
    toggle.addEventListener('click', () => {
      const next = document.documentElement.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
      applyTheme(next);
    });
  }

  // ── Explorer toggle ────────────────────────────────────────────────────────
  const explorerToggle = document.getElementById('explorer-toggle');
  const explorerContent = document.getElementById('explorer-content');
  if (explorerToggle && explorerContent) {
    explorerToggle.addEventListener('click', () => {
      const expanded = explorerToggle.getAttribute('aria-expanded') === 'true';
      explorerToggle.setAttribute('aria-expanded', String(!expanded));
      explorerContent.style.display = expanded ? 'none' : '';
    });
  }

  // ── File tree ──────────────────────────────────────────────────────────────
  const treeContainer = document.getElementById('explorer-content');
  if (treeContainer) {
    loadFileTree(treeContainer);
  }

  // ── Callout folds ──────────────────────────────────────────────────────────
  document.querySelectorAll('.callout.foldable .callout-title').forEach(title => {
    title.addEventListener('click', () => {
      title.closest('.callout').classList.toggle('collapsed');
    });
  });

  // ── TOC active-heading tracking ────────────────────────────────────────────
  const tocLinks = document.querySelectorAll('.toc-item');
  if (tocLinks.length > 0) {
    const observer = new IntersectionObserver(
      entries => {
        let topmost = null;
        entries.forEach(e => {
          if (e.isIntersecting) topmost = e;
        });
        if (topmost) {
          const id = topmost.target.id;
          tocLinks.forEach(l => {
            l.classList.toggle('active', l.getAttribute('href') === '#' + id);
          });
        }
      },
      { rootMargin: '0px 0px -80% 0px', threshold: 0 }
    );
    document.querySelectorAll('.markdown-body h1, .markdown-body h2, .markdown-body h3, .markdown-body h4').forEach(h => {
      if (h.id) observer.observe(h);
    });
  }

  // ── Reload link ────────────────────────────────────────────────────────────
  const reloadLink = document.getElementById('reload-link');
  if (reloadLink) {
    reloadLink.addEventListener('click', async e => {
      e.preventDefault();
      reloadLink.textContent = '↻ Reloading…';
      try {
        const res = await fetch('/api/reload');
        const data = await res.json();
        reloadLink.textContent = `✓ ${data.notes} notes`;
        setTimeout(() => { reloadLink.textContent = '↻ Reload'; }, 2000);
      } catch {
        reloadLink.textContent = '✗ Error';
        setTimeout(() => { reloadLink.textContent = '↻ Reload'; }, 2000);
      }
    });
  }
});

// ── File tree loader & renderer ───────────────────────────────────────────────
async function loadFileTree(container) {
  try {
    const res = await fetch('/api/files');
    const nodes = await res.json();
    const currentSlug = window.__SLUG__ || '';
    container.innerHTML = '';
    renderTreeNodes(nodes, container, currentSlug);
    // Auto-open folder containing current note
    if (currentSlug) {
      const active = container.querySelector('a.active');
      if (active) {
        let parent = active.closest('.tree-folder');
        while (parent) {
          parent.classList.add('open');
          parent = parent.parentElement && parent.parentElement.closest('.tree-folder');
        }
      }
    }
  } catch (e) {
    container.innerHTML = '<p style="font-size:.8rem;color:var(--gray);padding:.5rem">No notes yet.</p>';
  }
}

function renderTreeNodes(nodes, container, currentSlug) {
  for (const node of nodes) {
    if (node.type === 'folder') {
      const folder = document.createElement('div');
      folder.className = 'tree-folder';
      const header = document.createElement('div');
      header.className = 'tree-folder-header';
      header.innerHTML = `<span class="tree-folder-icon">▶</span><span class="tree-folder-name">${esc(node.name)}</span>`;
      header.addEventListener('click', () => folder.classList.toggle('open'));
      folder.appendChild(header);
      const children = document.createElement('div');
      children.className = 'tree-children';
      renderTreeNodes(node.children || [], children, currentSlug);
      folder.appendChild(children);
      container.appendChild(folder);
    } else {
      const file = document.createElement('div');
      file.className = 'tree-file';
      const icon = kindIcon(node.kind);
      const a = document.createElement('a');
      a.href = '/note/' + node.slug;
      a.innerHTML = `<span class="tree-file-icon">${icon}</span>${esc(node.name)}`;
      if (node.slug === currentSlug) a.classList.add('active');
      file.appendChild(a);
      container.appendChild(file);
    }
  }
}

function kindIcon(kind) {
  if (kind === 'canvas') return '⬡';
  if (kind === 'excalidraw') return '✏';
  return '◦';
}

function esc(s) {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}
