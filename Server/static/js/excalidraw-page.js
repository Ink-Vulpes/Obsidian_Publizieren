/* excalidraw-page.js — standalone Excalidraw page renderer */
(async function () {
  const container = document.getElementById('excalidraw-container');
  if (!container) return;

  const raw = window.__EXCALIDRAW_RAW__;
  if (!raw) return;

  function showError(msg) {
    container.innerHTML = '<p style="color:var(--gray);padding:1rem">⚠️ ' + msg + '</p>';
  }

  // ── 1. Load deps ────────────────────────────────────────────────────────────
  function loadScript(src) {
    return new Promise(function (resolve, reject) {
      if (document.querySelector('script[src="' + src + '"]')) { resolve(); return; }
      var s = document.createElement('script');
      s.src = src;
      s.onload = resolve;
      s.onerror = function () { reject(new Error('Failed to load ' + src)); };
      document.head.appendChild(s);
    });
  }

  try {
    await loadScript('https://cdnjs.cloudflare.com/ajax/libs/lz-string/1.5.0/lz-string.min.js');
    await loadScript('https://unpkg.com/react@18.3.1/umd/react.production.min.js');
    await loadScript('https://unpkg.com/react-dom@18.3.1/umd/react-dom.production.min.js');
    await loadScript('https://unpkg.com/@excalidraw/excalidraw@0.17.6/dist/excalidraw.production.min.js');
  } catch (e) {
    showError('Could not load Excalidraw library: ' + e.message);
    return;
  }

  // ── 2. Decompress ──────────────────────────────────────────────────────────
  let data = raw;
  if (raw.compressed) {
    if (typeof LZString === 'undefined') { showError('lz-string not loaded.'); return; }
    const json = LZString.decompressFromBase64(raw.compressed);
    if (!json) { showError('Could not decompress drawing data.'); return; }
    try { data = JSON.parse(json); } catch (e) { showError('Invalid JSON in drawing.'); return; }
  }

  if (!data.elements || data.elements.length === 0) {
    showError('No drawable elements found.');
    return;
  }

  if (typeof ExcalidrawLib === 'undefined' || !ExcalidrawLib.exportToSvg) {
    showError('Excalidraw library not loaded.');
    return;
  }

  // ── 3. Render ──────────────────────────────────────────────────────────────
  try {
    // Always export light-on-white. Do NOT spread data.appState — it may
    // carry a dark viewBackgroundColor that would override our settings.
    const svg = await ExcalidrawLib.exportToSvg({
      elements: data.elements,
      appState: {
        exportWithDarkMode: false,
        exportBackground: true,
        viewBackgroundColor: '#ffffff',
        theme: 'light',
        currentItemFontFamily: 1,
      },
      files: data.files || null,
    });

    // Force the background rect to white (belt-and-suspenders: some Excalidraw
    // versions bake a dark fill into the rect regardless of viewBackgroundColor).
    const bgRect = svg.querySelector('rect');
    if (bgRect) bgRect.setAttribute('fill', '#ffffff');

    // Remove any dark-mode CSS filter Excalidraw may have added as an attribute.
    svg.removeAttribute('filter');

    svg.style.width = '100%';
    svg.style.height = 'auto';
    svg.style.maxHeight = '75vh';
    svg.style.display = 'block';
    svg.style.borderRadius = '4px';

    container.style.background = '#ffffff';
    container.style.alignItems = 'unset';
    container.style.justifyContent = 'unset';
    container.innerHTML = '';
    container.appendChild(svg);
  } catch (e) {
    showError('Render error: ' + e.message);
    console.error('Excalidraw render error', e);
  }
})();
