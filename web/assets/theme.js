(() => {
  'use strict';

  if (window.location.pathname === '/mod' || window.location.pathname.startsWith('/mod/')) return;

  const storageKey = 'adelia.stylesheet';
  const themes = Object.freeze([
    { name: 'Yotsuba B (default)', file: '' },
    { name: 'Burichan', file: 'burichan.css' },
    { name: 'Caffe', file: 'caffe.css' },
    { name: 'Confraria', file: 'confraria.css' },
    { name: 'Dark', file: 'dark.css' },
    { name: 'Dark Roach', file: 'dark_roach.css' },
    { name: 'Favela', file: 'favela.css' },
    { name: 'Ferus', file: 'ferus.css' },
    { name: 'Futaba', file: 'futaba.css' },
    { name: 'Futaba Classic', file: 'futaba-classic.css' },
    { name: 'Futaba Light', file: 'futaba-light.css' },
    { name: 'Gentoochan', file: 'gentoochan.css' },
    { name: 'Green Dark', file: 'greendark.css' },
    { name: 'Jungle', file: 'jungle.css' },
    { name: 'Luna', file: 'luna.css' },
    { name: 'Miku', file: 'miku.css' },
    { name: 'Nigrachan', file: 'nigrachan.css' },
    { name: 'Northboard CB', file: 'northboard_cb.css' },
    { name: 'Notsuba', file: 'notsuba.css' },
    { name: 'Novo Jungle', file: 'novo_jungle.css' },
    { name: 'Photon', file: 'photon.css' },
    { name: 'Piwnichan', file: 'piwnichan.css' },
    { name: 'Ricechan', file: 'ricechan.css' },
    { name: 'Roach', file: 'roach.css' },
    { name: 'Rugby', file: 'rugby.css' },
    { name: 'Sharp', file: 'sharp.css' },
    { name: 'Sis', file: 'sis.css' },
    { name: 'Stripes', file: 'stripes.css' },
    { name: 'Szalet', file: 'szalet.css' },
    { name: 'Terminal 2', file: 'terminal2.css' },
    { name: 'Test Orange', file: 'testorange.css' },
    { name: 'Uboachan Gray', file: 'uboachan-gray.css' },
    { name: 'v8ch', file: 'v8ch.css' },
    { name: 'Wasabi', file: 'wasabi.css' },
    { name: 'Yotsuba', file: 'yotsuba.css' }
  ]);
  const allowedThemes = new Set(themes.map(theme => theme.file));

  const storage = {
    get() {
      try { return window.localStorage.getItem(storageKey) || ''; } catch (_) { return ''; }
    },
    set(value) {
      try {
        if (value) window.localStorage.setItem(storageKey, value);
        else window.localStorage.removeItem(storageKey);
      } catch (_) { /* Preferences remain available for this page in private mode. */ }
    }
  };

  function normalizeTheme(value) {
    return allowedThemes.has(value) ? value : '';
  }

  function applyTheme(value) {
    const theme = normalizeTheme(value);
    let link = document.getElementById('adelia-theme');
    if (!theme) {
      link?.remove();
    } else {
      if (!link) {
        link = document.createElement('link');
        link.id = 'adelia-theme';
        link.rel = 'stylesheet';
        document.head.append(link);
      }
      const href = `/assets/stylesheets/${theme}`;
      if (link.getAttribute('href') !== href) link.setAttribute('href', href);
    }
    document.documentElement.dataset.stylesheet = theme || 'default';
    return theme;
  }

  function sendThemeToFrames(value, source) {
    document.querySelectorAll('iframe').forEach(frame => {
      if (frame.contentWindow && frame.contentWindow !== source) {
        frame.contentWindow.postMessage({ type: 'adelia-theme', value }, window.location.origin);
      }
    });
  }

  function broadcastTheme(value) {
    const message = { type: 'adelia-theme', value };
    if (window.top !== window) {
      window.top.postMessage(message, window.location.origin);
    } else {
      sendThemeToFrames(value);
    }
  }

  const initialTheme = applyTheme(storage.get());

  window.addEventListener('storage', event => {
    if (event.key === storageKey || event.key === null) applyTheme(event.newValue || '');
  });

  window.addEventListener('message', event => {
    if (event.origin !== window.location.origin || event.data?.type !== 'adelia-theme') return;
    const theme = applyTheme(event.data.value);
    if (window.top === window) sendThemeToFrames(theme, event.source);
  });

  window.addEventListener('DOMContentLoaded', () => {
    const openers = Array.from(document.querySelectorAll('[data-options-open]'));
    if (!openers.length) return;

    const handler = document.createElement('div');
    handler.id = 'options-handler';
    handler.className = 'options-handler';
    handler.hidden = true;
    handler.innerHTML = `
      <div class="options-background" data-options-close></div>
      <div class="options-window" role="dialog" aria-modal="true" aria-labelledby="options-title">
        <button class="options-close" type="button" aria-label="Close options" data-options-close>&times;</button>
        <div class="options-tablist" role="tablist" aria-label="Option categories">
          <button class="options-tab-icon active" type="button" role="tab" aria-selected="true">
            <span class="options-tab-symbol" aria-hidden="true">&#9881;</span>
            <span>General</span>
          </button>
        </div>
        <section class="options-tab" role="tabpanel">
          <h2 id="options-title">General</h2>
          <div class="options-field">
            <label for="options-stylesheet">Stylesheet</label>
            <select id="options-stylesheet"></select>
            <p class="options-help">Your choice is saved on this device and used on every public board page.</p>
            <p class="options-status" role="status" aria-live="polite"></p>
            <button class="options-default" type="button">Restore Yotsuba B</button>
          </div>
        </section>
      </div>`;
    document.body.append(handler);

    const dialog = handler.querySelector('.options-window');
    const select = handler.querySelector('#options-stylesheet');
    const status = handler.querySelector('.options-status');
    const defaultButton = handler.querySelector('.options-default');
    let previousFocus = null;

    themes.forEach(theme => {
      const option = document.createElement('option');
      option.value = theme.file;
      option.textContent = theme.name;
      select.append(option);
    });
    select.value = initialTheme;

    function themeName(value) {
      return themes.find(theme => theme.file === value)?.name || themes[0].name;
    }

    function chooseTheme(value) {
      const theme = applyTheme(value);
      storage.set(theme);
      select.value = theme;
      status.textContent = `Using ${themeName(theme)}.`;
      broadcastTheme(theme);
    }

    function showOptions(event) {
      event?.preventDefault();
      previousFocus = event?.currentTarget || document.activeElement;
      select.value = normalizeTheme(storage.get());
      status.textContent = '';
      handler.hidden = false;
      document.body.classList.add('options-open');
      openers.forEach(opener => opener.setAttribute('aria-expanded', 'true'));
      select.focus();
    }

    function hideOptions() {
      if (handler.hidden) return;
      handler.hidden = true;
      document.body.classList.remove('options-open');
      openers.forEach(opener => opener.setAttribute('aria-expanded', 'false'));
      previousFocus?.focus({ preventScroll: true });
    }

    openers.forEach(opener => {
      opener.setAttribute('aria-expanded', 'false');
      opener.addEventListener('click', showOptions);
    });
    handler.querySelectorAll('[data-options-close]').forEach(closer => closer.addEventListener('click', hideOptions));
    select.addEventListener('change', () => chooseTheme(select.value));
    defaultButton.addEventListener('click', () => chooseTheme(''));

    handler.addEventListener('keydown', event => {
      if (event.key === 'Escape') {
        event.preventDefault();
        hideOptions();
        return;
      }
      if (event.key !== 'Tab') return;
      const focusable = Array.from(dialog.querySelectorAll('button:not([disabled]), select:not([disabled])'));
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    });
  });
})();
