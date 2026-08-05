(() => {
  'use strict';

  const storage = {
    get(key) {
      try { return window.localStorage.getItem(key); } catch (_) { return null; }
    },
    set(key, value) {
      try { window.localStorage.setItem(key, value); } catch (_) { /* private mode */ }
    }
  };

  function initRememberedFields() {
    document.querySelectorAll('[data-remember]').forEach(field => {
      const name = field.dataset.remember;
      const key = `adelia.${name}`;
      let value = storage.get(key) || '';
      if (name === 'name' && value.includes('##')) {
        value = value.split('##', 1)[0];
        storage.set(key, value);
      }
      if (value && !field.value) field.value = value;
      field.addEventListener('input', () => {
        const storedValue = name === 'name' ? field.value.split('##', 1)[0] : field.value;
        storage.set(key, storedValue);
      });
    });
  }

  function revealPostForm(form, focusTarget = null) {
    if (!form) return;
    form.hidden = false;
    const toggle = form.nextElementSibling;
    if (toggle?.matches('[data-post-form-toggle]')) {
      toggle.hidden = true;
      toggle.querySelector('a')?.setAttribute('aria-expanded', 'true');
    }
    if (focusTarget) {
      window.requestAnimationFrame(() => {
        focusTarget.focus({ preventScroll: true });
        focusTarget.scrollIntoView({ behavior: 'smooth', block: 'center' });
      });
    }
  }

  function initPostFormToggle() {
    const forms = document.querySelectorAll('[data-post-form]');
    forms.forEach((form, index) => {
      if (!form.id) form.id = forms.length === 1 ? 'post-form' : `post-form-${index + 1}`;

      const toggle = document.createElement('div');
      toggle.className = 'show-post-form';
      toggle.dataset.postFormToggle = '';

      const link = document.createElement('a');
      link.href = `#${form.id}`;
      link.textContent = form.querySelector('input[name="thread"]')
        ? 'Post a Reply'
        : 'Start a New Thread';
      link.setAttribute('aria-controls', form.id);
      link.setAttribute('aria-expanded', 'false');
      toggle.append('[', link, ']');

      form.hidden = true;
      form.insertAdjacentElement('afterend', toggle);
      link.addEventListener('click', event => {
        event.preventDefault();
        revealPostForm(form);
      });

      if (window.location.hash === '#post-body') revealPostForm(form);
    });

    document.addEventListener('click', event => {
      const trigger = event.target.closest('.quick-reply-link');
      if (!trigger) return;
      const form = document.querySelector('[data-post-form]');
      const textarea = form?.querySelector('textarea[name="body"]');
      if (!form || !textarea) return;
      event.preventDefault();
      revealPostForm(form, textarea);
    });
  }

  function initQuotes() {
    document.addEventListener('click', event => {
      const quote = event.target.closest('[data-quote-id]');
      if (!quote) return;
      const textarea = document.querySelector('textarea[name="body"]');
      if (!textarea) return;
      event.preventDefault();
      revealPostForm(textarea.closest('[data-post-form]'));
      const prefix = textarea.value && !textarea.value.endsWith('\n') ? '\n' : '';
      const citation = `${prefix}>>${quote.dataset.quoteId}\n`;
      const start = textarea.selectionStart ?? textarea.value.length;
      const end = textarea.selectionEnd ?? textarea.value.length;
      textarea.setRangeText(citation, start, end, 'end');
      textarea.focus({ preventScroll: true });
      textarea.scrollIntoView({ behavior: 'smooth', block: 'center' });
    });
  }

  function highlightHash() {
    document.querySelectorAll('.post.highlighted').forEach(post => post.classList.remove('highlighted'));
    const id = window.location.hash.slice(1);
    if (!/^\d+$/.test(id)) return;
    const post = document.getElementById(id);
    if (post) post.classList.add('highlighted');
  }

  function initImages() {
    document.addEventListener('click', event => {
      const link = event.target.closest('[data-expand-image]');
      if (!link || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey) return;
      const image = link.querySelector('img');
      if (!image) return;
      event.preventDefault();
      const expanded = image.classList.toggle('expanded');
      image.src = expanded ? link.dataset.full : link.dataset.thumb;
      if (expanded) {
        image.removeAttribute('width');
        image.removeAttribute('height');
      }
    });
  }

  function initCatalog() {
    const grid = document.querySelector('[data-catalog-grid]');
    if (!grid) return;
    const sort = document.querySelector('[data-catalog-sort]');
    const size = document.querySelector('[data-catalog-size]');
    sort?.addEventListener('change', () => {
      const key = sort.value;
      const cards = Array.from(grid.querySelectorAll('.catalog-thread'));
      cards.sort((left, right) => Number(right.dataset[key]) - Number(left.dataset[key]));
      cards.forEach(card => grid.append(card));
    });
    size?.addEventListener('change', () => {
      grid.classList.remove('size-small', 'size-medium', 'size-large');
      grid.classList.add(`size-${size.value}`);
      storage.set('adelia.catalogSize', size.value);
    });
    const savedSize = storage.get('adelia.catalogSize');
    if (savedSize && ['small', 'medium', 'large'].includes(savedSize)) {
      size.value = savedSize;
      size.dispatchEvent(new Event('change'));
    }
  }

  function initFrames() {
    const shell = document.querySelector('.frame-shell');
    const toggle = document.querySelector('[data-frame-toggle]');
    if (!shell || !toggle) return;
    const mobile = () => window.matchMedia('(max-width: 720px)').matches;
    if (!mobile() && storage.get('adelia.sidebarHidden') === 'true') {
      shell.classList.add('sidebar-hidden');
    }
    const update = () => {
      const open = mobile()
        ? shell.classList.contains('sidebar-open')
        : !shell.classList.contains('sidebar-hidden');
      toggle.textContent = open ? 'Hide boards' : 'Boards';
      toggle.setAttribute('aria-expanded', String(open));
    };
    toggle.addEventListener('click', () => {
      if (mobile()) {
        shell.classList.toggle('sidebar-open');
      } else {
        shell.classList.toggle('sidebar-hidden');
        storage.set('adelia.sidebarHidden', String(shell.classList.contains('sidebar-hidden')));
      }
      update();
    });
    shell.querySelectorAll('.frame-sidebar a[target="main"]').forEach(link => {
      link.addEventListener('click', () => {
        if (mobile()) shell.classList.remove('sidebar-open');
        update();
      });
    });
    window.addEventListener('resize', update);
    update();
  }

  function initForms() {
    document.querySelectorAll('form[data-confirm]').forEach(form => {
      form.addEventListener('submit', event => {
        if (!window.confirm(form.dataset.confirm)) event.preventDefault();
      });
    });
    document.querySelectorAll('.post-controls').forEach(form => {
      form.addEventListener('submit', event => {
        if (!form.querySelector('input[name="post_id"]:checked')) {
          event.preventDefault();
          window.alert('Select at least one post first.');
        }
      });
    });
    document.querySelectorAll('[data-post-form]').forEach(form => {
      form.addEventListener('submit', () => {
        form.querySelectorAll('button[type="submit"]').forEach(button => {
          button.disabled = true;
          button.textContent = 'Posting…';
        });
      });
    });
  }

  function showPendingConfirmation() {
    const params = new URLSearchParams(window.location.search);
    if (params.get('submitted') !== 'pending') return;
    const notice = document.querySelector('.moderation-notice');
    if (!notice) return;
    notice.textContent = 'Your post was received and is awaiting moderator approval.';
    notice.setAttribute('role', 'status');
    notice.scrollIntoView({ block: 'center' });
  }

  initRememberedFields();
  initPostFormToggle();
  initQuotes();
  initImages();
  initCatalog();
  initFrames();
  initForms();
  showPendingConfirmation();
  highlightHash();
  window.addEventListener('hashchange', highlightHash);
})();
