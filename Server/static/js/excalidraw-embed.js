(function () {
  const embeds = document.querySelectorAll('.excalidraw-embed');
  if (!embeds.length) return;

  function loadScript(src) {
    return new Promise(function (resolve, reject) {
      var s = document.createElement('script');
      s.src = src;
      s.onload = resolve;
      s.onerror = reject;
      document.head.appendChild(s);
    });
  }

  async function renderAll() {
    try {
      await loadScript('https://cdnjs.cloudflare.com/ajax/libs/lz-string/1.5.0/lz-string.min.js');
      await loadScript('https://unpkg.com/react@18.3.1/umd/react.production.min.js');
      await loadScript('https://unpkg.com/react-dom@18.3.1/umd/react-dom.production.min.js');
      await loadScript('https://unpkg.com/@excalidraw/excalidraw@0.17.6/dist/excalidraw.production.min.js');
    } catch (e) {
      embeds.forEach(function (el) { el.textContent = '⚠️ Failed to load Excalidraw library.'; });
      return;
    }

    const isDark = document.documentElement.getAttribute('data-theme') === 'dark';

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

        const svg = await ExcalidrawLib.exportToSvg({
          elements: data.elements,
          appState: Object.assign({}, data.appState || {}, {
            exportWithDarkMode: isDark,
            exportBackground: true,
            viewBackgroundColor: isDark ? '#1c1c1e' : '#ffffff',
            theme: isDark ? 'dark' : 'light',
          }),
          files: data.files || null,
        });

        if (!isDark) {
          svg.removeAttribute('filter');
          const bg = svg.querySelector('rect');
          if (bg) bg.setAttribute('fill', '#ffffff');
        }

        svg.style.cssText = 'width:100%;height:auto;display:block;border-radius:6px';
        embed.innerHTML = '';
        embed.appendChild(svg);
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
