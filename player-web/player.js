// Веб-плеер: рисует картину (`view`) движка runtime.js и передаёт ему действия игрока.
// Сам ничего в сюжете не решает. Сохранения лежат в localStorage браузера.
(async function () {
  'use strict';
  const $ = (id) => document.getElementById(id);
  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  };
  const stage = $('stage');
  const overlay = $('overlay');
  const url = (p) => encodeURI(p);
  const SLOTS = 8;

  let story;
  try {
    story = await (await fetch('story.json', { cache: 'no-cache' })).json();
  } catch (e) {
    stage.textContent = 'Не удалось загрузить историю (story.json). Откройте игру через веб-адрес, а не как файл.';
    return;
  }
  document.title = story.meta.name;

  // ---------- хранилище ----------
  const KEY = `vn:${story.meta.id}:`;
  const store = {
    get(k) {
      try {
        return localStorage.getItem(KEY + k);
      } catch (e) {
        return null;
      }
    },
    set(k, v) {
      try {
        localStorage.setItem(KEY + k, v);
        return true;
      } catch (e) {
        return false;
      }
    },
  };
  const settings = Object.assign({ textSpeed: 1, music: 0.7, sfx: 0.9, autoDelay: 1.5 }, JSON.parse(store.get('settings') || '{}'));
  const saveSettings = () => store.set('settings', JSON.stringify(settings));

  function readSave(slot) {
    const raw = store.get('save:' + slot);
    if (!raw) return null;
    try {
      return VN.parseSave(raw);
    } catch (e) {
      return { error: e.message };
    }
  }
  function writeSave(slot) {
    if (!rt) return false;
    return store.set('save:' + slot, JSON.stringify(rt.save(Math.floor(Date.now() / 1000))));
  }
  const allSlots = () => ['auto', 'quick', ...Array.from({ length: SLOTS }, (_, i) => 'slot' + (i + 1))];
  const slotTitle = (s) => (s === 'auto' ? 'Автосохранение' : s === 'quick' ? 'Быстрое сохранение' : 'Слот ' + s.slice(4));
  function latestSave() {
    return allSlots().map(readSave).filter((s) => s && !s.error).sort((a, b) => b.saved_at - a.saved_at)[0] || null;
  }
  const when = (secs) => new Date(secs * 1000).toLocaleString('ru-RU', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' });

  // ---------- состояние плеера ----------
  let rt = null;
  let view = null;
  let auto = false;
  let skip = false;
  let timer = null; // пауза / автопереход
  const typing = { spans: [], total: 0, shown: 0, raf: 0, last: 0, speed: 40 };

  function toast(msg) {
    const t = $('toast');
    t.textContent = msg;
    t.classList.add('show');
    clearTimeout(toast.t);
    toast.t = setTimeout(() => t.classList.remove('show'), 2600);
  }

  // ---------- размер сцены ----------
  function layout() {
    const [w, h] = story.meta.resolution;
    const scale = Math.min(window.innerWidth / w, window.innerHeight / h);
    stage.style.width = w * scale + 'px';
    stage.style.height = h * scale + 'px';
    stage.style.setProperty('--h', h * scale + 'px');
    stage.style.setProperty('--text', story.meta.text.size);
  }
  window.addEventListener('resize', layout);
  layout();
  $('text').style.color = `rgb(${story.meta.text.color.join(',')})`;

  // ---------- звук ----------
  const music = { el: null, track: null, target: 0, ramp: 0 };
  function rampVolume(audio, to, seconds, then) {
    clearInterval(audio._ramp);
    const from = audio.volume;
    const steps = Math.max(1, Math.round((seconds * 1000) / 50));
    let n = 0;
    audio._ramp = setInterval(() => {
      n++;
      audio.volume = Math.min(1, Math.max(0, from + ((to - from) * n) / steps));
      if (n >= steps) {
        clearInterval(audio._ramp);
        if (then) then();
      }
    }, 50);
  }
  function syncMusic(v) {
    const want = v.music;
    const fade = v.music_fade || 0;
    if (!want || want.track !== music.track) {
      if (music.el) {
        const old = music.el;
        rampVolume(old, 0, fade, () => old.pause());
        music.el = null;
        music.track = null;
      }
      if (want) {
        const a = new Audio(url(want.track));
        a.loop = want.looped;
        a.volume = 0;
        a.play().catch(() => (music.blocked = true));
        music.el = a;
        music.track = want.track;
      }
    }
    if (want && music.el) {
      music.target = want.volume;
      rampVolume(music.el, want.volume * settings.music, fade);
    }
  }
  function playSound(file, volume) {
    const a = new Audio(url(file));
    a.volume = Math.min(1, volume * settings.sfx);
    a.play().catch(() => {});
  }
  // Браузер не даёт играть звук до первого касания — повторяем после него.
  window.addEventListener('pointerdown', () => {
    if (music.blocked && music.el) music.el.play().then(() => (music.blocked = false)).catch(() => {});
  });

  // ---------- сцена ----------
  let shownBg = null;
  function syncBackground(v) {
    if (v.background === shownBg) return;
    const bg = $('bg');
    const old = $('bg-old');
    old.style.backgroundImage = bg.style.backgroundImage;
    bg.style.transition = 'none';
    bg.style.opacity = 0;
    bg.style.backgroundImage = v.background ? `url("${url(v.background)}")` : 'none';
    void bg.offsetWidth;
    bg.style.transition = `opacity ${v.bg_fade}s`;
    bg.style.opacity = 1;
    shownBg = v.background;
  }

  const spriteEls = new Map();
  function syncSprites(v, instant) {
    const box = $('sprites');
    const alive = new Set(v.sprites.map((s) => s.id));
    for (const [id, node] of spriteEls) {
      if (alive.has(id)) continue;
      spriteEls.delete(id);
      node.style.transition = `opacity ${instant ? 0 : v.hide_fade}s`;
      node.style.opacity = 0;
      setTimeout(() => node.remove(), (instant ? 0 : v.hide_fade) * 1000 + 50);
    }
    for (const s of v.sprites) {
      let node = spriteEls.get(s.id);
      const fresh = !node;
      if (fresh) {
        node = el('div', 'sprite');
        box.appendChild(node);
        spriteEls.set(s.id, node);
      }
      const key = s.layers.join('|');
      if (node.dataset.layers !== key) {
        node.dataset.layers = key;
        node.replaceChildren(...s.layers.map((src) => Object.assign(el('img'), { src: url(src), draggable: false })));
      }
      const move = fresh || instant ? 0 : s.move_time;
      node.style.transition = `opacity ${fresh && !instant ? s.fade : 0}s, left ${move}s, bottom ${move}s, height ${move}s`;
      if (fresh) void node.offsetWidth;
      node.style.left = s.x * 100 + '%';
      node.style.bottom = (1 - s.y) * 100 + '%';
      node.style.height = s.height * 100 + '%';
      node.style.opacity = 1;
    }
  }

  // ---------- текст ----------
  function drawText() {
    const box = $('text');
    box.replaceChildren();
    let left = Math.floor(typing.shown);
    for (const s of typing.spans) {
      const chars = Array.from(s.text);
      const visible = chars.slice(0, Math.max(0, left)).join('');
      const rest = chars.slice(Math.max(0, left)).join('');
      left -= chars.length;
      const wrap = (t, hidden) => {
        if (!t) return;
        const node = el(s.bold ? 'b' : 'span', hidden ? 'rest' : '', t);
        box.appendChild(node);
      };
      wrap(visible, false);
      wrap(rest, true);
    }
    $('textbox').classList.toggle('done', typing.shown >= typing.total);
  }
  function tick(now) {
    typing.shown = Math.min(typing.total, typing.shown + ((now - typing.last) / 1000) * typing.speed);
    typing.last = now;
    drawText();
    if (typing.shown < typing.total) typing.raf = requestAnimationFrame(tick);
    else textDone();
  }
  function startText(p) {
    cancelAnimationFrame(typing.raf);
    typing.spans = p.spans;
    typing.total = p.spans.reduce((n, s) => n + Array.from(s.text).length, 0);
    typing.speed = Math.max(1, p.speed * settings.textSpeed);
    const instant = (skip && p.seen) || settings.textSpeed >= 5;
    typing.shown = instant ? typing.total : 0;
    typing.last = performance.now();
    drawText();
    if (instant) textDone();
    else typing.raf = requestAnimationFrame(tick);
  }
  function finishText() {
    cancelAnimationFrame(typing.raf);
    typing.shown = typing.total;
    drawText();
    textDone();
  }
  function textDone() {
    clearTimeout(timer);
    const p = view && view.prompt;
    if (!p || p.kind !== 'text') return;
    if (skip && p.seen) timer = setTimeout(advance, 60);
    else if (auto) timer = setTimeout(advance, settings.autoDelay * 1000 + typing.total * 25);
    else if (skip) setSkip(false); // дошли до непрочитанного
  }

  // ---------- отрисовка картины движка ----------
  function render(instant) {
    clearTimeout(timer);
    view = rt.view();
    for (const c of rt.takeCues()) if (!instant) playSound(c.sound, c.volume);
    syncBackground(view);
    syncSprites(view, instant);
    syncMusic(view);
    const black = $('black');
    black.style.transition = `opacity ${instant ? 0 : view.fade}s`;
    black.style.opacity = view.black ? 1 : 0;
    stage.classList.toggle('dialog-top', view.dialog === 'top');
    stage.classList.toggle('dialog-center', view.dialog === 'center');

    const p = view.prompt;
    const tb = $('textbox');
    tb.style.display = p.kind === 'text' ? '' : 'none';
    $('choices').style.display = p.kind === 'choice' ? 'flex' : 'none';
    if (p.kind === 'text') {
      tb.className = p.style;
      // Оформление реплики: цвет и размер приходят из движка уже рассчитанными.
      $('text').style.fontSize = `calc(var(--h, 400px) * ${p.size})`;
      $('text').style.color = `rgb(${p.color.join(',')})`;
      $('name').textContent = p.speaker ? p.speaker.name : '';
      $('name').style.color = p.speaker ? `rgb(${p.speaker.color.join(',')})` : '';
      if (p.sound && !instant) playSound(p.sound, 1);
      startText(p);
    } else if (p.kind === 'choice') {
      setSkip(false);
      const box = $('choices');
      box.replaceChildren();
      if (p.prompt) box.appendChild(el('div', 'prompt', p.prompt));
      p.options.forEach((o, i) => {
        const b = el('button', 'choice', `${i + 1}. ${o.text}`);
        b.onclick = (e) => {
          e.stopPropagation();
          if (rt.choose(o.id)) {
            writeSave('auto');
            render();
          }
        };
        box.appendChild(b);
      });
    } else if (p.kind === 'wait') {
      timer = setTimeout(advance, skip ? 0 : p.seconds * 1000);
    } else if (p.kind === 'wardrobe') {
      setSkip(false);
      showWardrobe(p.slots, () => {
        closeOverlay();
        advance();
      });
    } else if (p.kind === 'ended') {
      setSkip(false);
      setAuto(false);
      showEnding(p.title);
    } else if (p.kind === 'error') {
      showPanel('Ошибка в истории', (body) => {
        body.appendChild(el('p', '', p.message));
        body.appendChild(button('В главное меню', showTitle));
      });
    }
  }

  function advance() {
    if (!rt) return;
    const wasText = view && view.prompt.kind === 'text';
    rt.advance();
    if (wasText) writeSave('auto');
    render();
  }

  function setAuto(on) {
    auto = on;
    document.querySelector('[data-act=auto]').classList.toggle('on', on);
    if (on && view && view.prompt.kind === 'text' && typing.shown >= typing.total) textDone();
    if (!on) clearTimeout(timer);
  }
  function setSkip(on) {
    skip = on;
    document.querySelector('[data-act=skip]').classList.toggle('on', on);
    if (on && view && view.prompt.kind === 'text') {
      if (view.prompt.seen) finishText();
      else {
        skip = false;
        document.querySelector('[data-act=skip]').classList.remove('on');
        toast('Этот текст ещё не прочитан');
      }
    }
  }

  // ---------- управление ----------
  function tap() {
    if (stage.classList.contains('hidden-ui')) return stage.classList.remove('hidden-ui');
    if (!rt || overlay.style.display === 'block') return;
    if (view.prompt.kind !== 'text') return;
    if (typing.shown < typing.total) finishText();
    else advance();
  }
  stage.addEventListener('click', (e) => {
    if (e.target.closest('#hud, #overlay, #choices')) return;
    tap();
  });
  window.addEventListener('keydown', (e) => {
    if (e.key === ' ' || e.key === 'Enter') tap();
    else if (e.key === 'Escape') overlay.style.display === 'block' ? (rt ? closeOverlay() : null) : showMenu();
    else if (e.key === 'F5') {
      e.preventDefault();
      quickSave();
    } else if (e.key === 'F9') {
      e.preventDefault();
      loadSlot('quick');
    }
  });
  function quickSave() {
    toast(writeSave('quick') ? 'Сохранено' : 'Не удалось сохранить: хранилище браузера недоступно');
  }
  $('hud').addEventListener('click', (e) => {
    const act = e.target.dataset.act;
    if (!act || !rt) return;
    e.stopPropagation();
    if (act === 'menu') showMenu();
    else if (act === 'auto') setAuto(!auto);
    else if (act === 'skip') setSkip(!skip);
    else if (act === 'log') showLog();
    else if (act === 'quicksave') quickSave();
    else if (act === 'quickload') loadSlot('quick');
    else if (act === 'hide') stage.classList.add('hidden-ui');
  });

  // ---------- игра ----------
  function resetStage() {
    shownBg = null;
    $('bg').style.backgroundImage = 'none';
    $('bg-old').style.backgroundImage = 'none';
    for (const node of spriteEls.values()) node.remove();
    spriteEls.clear();
    setAuto(false);
    skip = false;
    document.querySelector('[data-act=skip]').classList.remove('on');
  }
  function newGame() {
    resetStage();
    rt = VN.Runtime.start(story);
    closeOverlay();
    render();
  }
  function loadSave(save) {
    try {
      const r = VN.Runtime.restore(story, save);
      resetStage();
      rt = r.rt;
      closeOverlay();
      render(true);
      if (r.how.how === 'scene_restart') toast('История обновилась: продолжаем с начала сцены');
      else if (r.how.how === 'redirected') toast('История обновилась: сцена изменена, продолжаем с новой');
    } catch (e) {
      toast(e.message);
    }
  }
  function loadSlot(slot) {
    const s = readSave(slot);
    if (!s) return toast('Слот пуст');
    if (s.error) return toast(s.error);
    loadSave(s);
  }

  // ---------- экраны ----------
  function closeOverlay() {
    overlay.style.display = 'none';
    overlay.className = '';
    overlay.replaceChildren();
  }
  function button(text, onclick, disabled) {
    const b = el('button', 'btn', text);
    b.disabled = !!disabled;
    b.onclick = (e) => {
      e.stopPropagation();
      onclick();
    };
    return b;
  }
  function showPanel(title, fill, back, backLabel) {
    clearTimeout(timer);
    overlay.replaceChildren();
    overlay.className = '';
    overlay.style.display = 'block';
    const head = el('h2', '', title);
    if (back) head.appendChild(button(backLabel || '← Назад', back));
    overlay.appendChild(head);
    const body = el('div');
    overlay.appendChild(body);
    fill(body);
  }
  function resume() {
    closeOverlay();
    if (view && view.prompt.kind === 'wait') render();
  }

  function showTitle() {
    rt = null;
    view = null;
    clearTimeout(timer);
    if (music.el) music.el.pause();
    music.el = null;
    music.track = null;
    overlay.replaceChildren();
    overlay.className = 'title';
    overlay.style.display = 'block';
    overlay.appendChild(el('h1', '', story.meta.name));
    overlay.appendChild(el('div', 'sub', [story.meta.author, 'версия ' + story.meta.version].filter(Boolean).join(' · ')));
    const menu = el('div', 'menu');
    const latest = latestSave();
    menu.append(button('Новая игра', newGame), button('Продолжить', () => loadSave(latest), !latest), button('Загрузить', () => showSaves('load', showTitle)), button('Настройки', () => showSettings(showTitle)));
    overlay.appendChild(menu);
  }

  function showMenu() {
    if (!rt) return;
    showPanel('Пауза', (body) => {
      const menu = el('div', 'menu');
      menu.append(button('Вернуться в игру', resume), button('Сохранить', () => showSaves('save', showMenu)), button('Загрузить', () => showSaves('load', showMenu)));
      if (story.wardrobe.anytime && story.wardrobe.hero) menu.append(button('Гардероб', () => showWardrobe([], showMenu)));
      menu.append(button('История диалогов', () => showLog(showMenu)), button('Настройки', () => showSettings(showMenu)), button('На весь экран', toggleFullscreen), button('В главное меню', showTitle));
      body.appendChild(menu);
    });
  }

  function showSaves(mode, back) {
    showPanel(mode === 'save' ? 'Сохранить' : 'Загрузить', (body) => {
      const grid = el('div', 'slots');
      for (const slot of allSlots()) {
        if (mode === 'save' && (slot === 'auto' || slot === 'quick')) continue;
        const s = readSave(slot);
        const card = el('button', 'slot' + (s ? '' : ' empty'));
        card.appendChild(el('b', '', slotTitle(slot)));
        card.appendChild(el('span', '', s ? s.error || s.label : 'Пусто'));
        if (s && !s.error) card.appendChild(el('small', '', when(s.saved_at)));
        card.onclick = () => {
          if (mode === 'save') {
            toast(writeSave(slot) ? 'Сохранено' : 'Не удалось сохранить');
            showSaves(mode, back);
          } else loadSlot(slot);
        };
        grid.appendChild(card);
      }
      body.appendChild(grid);
    }, back);
  }

  function slider(body, label, key, min, max, step, onchange) {
    const row = el('div', 'row');
    row.appendChild(el('label', '', label));
    const input = Object.assign(el('input'), { type: 'range', min, max, step, value: settings[key] });
    input.oninput = () => {
      settings[key] = Number(input.value);
      saveSettings();
      if (onchange) onchange();
    };
    row.appendChild(input);
    body.appendChild(row);
  }
  function showSettings(back) {
    showPanel('Настройки', (body) => {
      slider(body, 'Скорость текста', 'textSpeed', 0.3, 5, 0.1);
      slider(body, 'Пауза авточтения', 'autoDelay', 0.3, 5, 0.1);
      slider(body, 'Громкость музыки', 'music', 0, 1, 0.05, () => {
        if (music.el) music.el.volume = music.target * settings.music;
      });
      slider(body, 'Громкость звуков', 'sfx', 0, 1, 0.05);
      body.appendChild(button('На весь экран', toggleFullscreen));
    }, back);
  }
  function toggleFullscreen() {
    if (document.fullscreenElement) document.exitFullscreen();
    else if (document.documentElement.requestFullscreen) document.documentElement.requestFullscreen().catch(() => toast('Браузер не разрешил полноэкранный режим'));
  }

  function showLog(back) {
    showPanel('История диалогов', (body) => {
      body.className = 'log';
      for (const line of rt.state.backlog) {
        const p = el('p');
        if (line.speaker) p.appendChild(el('b', '', line.speaker + ': '));
        p.appendChild(document.createTextNode(line.text));
        body.appendChild(p);
      }
      body.scrollIntoView(false);
    }, back || resume);
  }

  function showWardrobe(slots, done) {
    showPanel('Гардероб', (body) => {
      const wrap = el('div', 'wardrobe');
      const doll = el('div', 'doll');
      const racks = el('div', 'racks');
      const draw = () => {
        doll.replaceChildren(...rt.heroLayers().map((src) => Object.assign(el('img'), { src: url(src) })));
        racks.replaceChildren();
        for (const slot of rt.wardrobe(slots)) {
          const rack = el('div', 'rack');
          rack.appendChild(el('div', '', slot.name));
          const items = el('div', 'items');
          const none = el('button', 'item' + (slot.items.some((i) => i.equipped) ? '' : ' on'), 'Ничего');
          none.onclick = () => {
            rt.equip(slot.id, null);
            draw();
          };
          items.appendChild(none);
          for (const it of slot.items) {
            const b = el('button', 'item' + (it.equipped ? ' on' : ''), it.name);
            b.onclick = () => {
              rt.equip(slot.id, it.id);
              draw();
            };
            items.appendChild(b);
          }
          rack.appendChild(items);
          racks.appendChild(rack);
        }
      };
      draw();
      wrap.append(doll, racks);
      body.appendChild(wrap);
    }, () => {
      syncSprites(rt.view(), true);
      done();
    }, 'Готово');
  }

  function showEnding(title) {
    showPanel(title || 'Конец', (body) => {
      body.appendChild(el('p', '', 'Спасибо за игру!'));
      const menu = el('div', 'menu');
      menu.append(button('История диалогов', () => showLog(() => showEnding(title))), button('В главное меню', showTitle));
      body.appendChild(menu);
    });
  }

  if ('serviceWorker' in navigator && (location.protocol === 'https:' || location.hostname === 'localhost')) navigator.serviceWorker.register('sw.js').catch(() => {});
  showTitle();
})();
