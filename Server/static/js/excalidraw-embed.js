(function () {
  const embeds = document.querySelectorAll('.excalidraw-embed');
  const pageContainer = document.getElementById('excalidraw-container');
  if (!embeds.length && !pageContainer) return;

  // ── Standalone page: watch for any SVG added to #excalidraw-container
  // and immediately force a white background. This runs regardless of which
  // code renders the SVG first, so timing races don't matter.
  if (pageContainer) {
    function fixSvgBackground(svg) {
      // Force the background rect (always the first rect in Excalidraw SVGs) to white.
      const bgRect = svg.querySelector('rect');
      if (bgRect) bgRect.setAttribute('fill', '#ffffff');
      // Remove any dark-mode invert filter baked in by exportToSvg.
      svg.removeAttribute('filter');
      pageContainer.style.background = '#ffffff';
      pageContainer.style.alignItems = 'unset';
      pageContainer.style.justifyContent = 'unset';
    }

    // Fix any SVG already present (e.g. synchronous render completed before this runs).
    const existingSvg = pageContainer.querySelector('svg');
    if (existingSvg) fixSvgBackground(existingSvg);

    // Watch for future SVG insertions (e.g. async render completes after this).
    const obs = new MutationObserver(function (mutations) {
      for (const m of mutations) {
        for (const node of m.addedNodes) {
          if (node.nodeName === 'svg') { fixSvgBackground(node); return; }
          if (node.nodeType === 1) {
            const svg = node.querySelector('svg');
            if (svg) { fixSvgBackground(svg); return; }
          }
        }
      }
    });
    obs.observe(pageContainer, { childList: true, subtree: false });
  }

  function loadScript(src) {
    return new Promise(function (resolve, reject) {
      if (document.querySelector('script[src="' + src + '"]')) { resolve(); return; }
      var s = document.createElement('script');
      s.src = src;
      s.onload = resolve;
      s.onerror = reject;
      document.head.appendChild(s);
    });
  }

  // Render a single Excalidraw drawing (elements + optional files) into a container.
  // Always uses a white background so drawings are readable in dark and light page themes.
  async function renderToContainer(container, data) {
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

    // Belt-and-suspenders: force background rect to white in case Excalidraw
    // baked a dark fill from the drawing's own appState.
    const bgRect = svg.querySelector('rect');
    if (bgRect) bgRect.setAttribute('fill', '#ffffff');
    // Remove any CSS invert filter added for dark mode.
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
  }

  async function renderAll() {
    try {
      await loadScript('https://cdnjs.cloudflare.com/ajax/libs/lz-string/1.5.0/lz-string.min.js');
      await loadScript('https://unpkg.com/react@18.3.1/umd/react.production.min.js');
      await loadScript('https://unpkg.com/react-dom@18.3.1/umd/react-dom.production.min.js');
      await loadScript('https://unpkg.com/@excalidraw/excalidraw@0.17.6/dist/excalidraw.production.min.js');
    } catch (e) {
      const msg = '⚠️ Failed to load Excalidraw library.';
      embeds.forEach(function (el) { el.textContent = msg; });
      if (pageContainer) pageContainer.textContent = msg;
      return;
    }

    // ── Standalone excalidraw page ────────────────────────────────────────────
    if (pageContainer && window.__EXCALIDRAW_RAW__) {
      try {
        let data = window.__EXCALIDRAW_RAW__;
        if (data.compressed) {
          const json = LZString.decompressFromBase64(data.compressed);
          if (!json) { pageContainer.textContent = '⚠️ Could not decompress drawing data.'; }
          else {
            data = JSON.parse(json);
            if (data.elements && data.elements.length) {
              await renderToContainer(pageContainer, data);
            } else {
              pageContainer.textContent = '⚠️ No drawable elements found.';
            }
          }
        } else if (data.elements && data.elements.length) {
          await renderToContainer(pageContainer, data);
        } else {
          pageContainer.textContent = '⚠️ No drawable elements found.';
        }
      } catch (e) {
        pageContainer.textContent = '⚠️ Render error: ' + e.message;
        console.error('Excalidraw page render error', e);
      }
    }

    // ── Embedded excalidraw blocks (in markdown notes) ────────────────────────
    for (const embed of embeds) {
      try {
        const raw = JSON.parse(embed.getAttribute('data-excalidraw'));
        let data = raw;

        if (raw.compressed) {
          const json = LZString.decompressFromBase64(raw.compressed);
          if (!json) { embed.textContent = '⚠️ Could not decompress diagram.'; continue; }
          data = JSON.parse(json);
        }

        if (!data.elements || !data.elements.length) {
          embed.textContent = '⚠️ No drawable elements found.';
          continue;
        }

        await renderToContainer(embed, data);
      } catch (e) {
        embed.textContent = '⚠️ Render error: ' + e.message;
        console.error('Excalidraw embed error', e);
      }
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', renderAll);
  } else {
    renderAll();
  }
})();
