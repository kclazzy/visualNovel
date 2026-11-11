// Движок визуальной новеллы для веб-плеера (телефон/браузер).
// Это точное повторение рантайма из crates/vn-core (runtime.rs, view.rs, save.rs).
// Любое расхождение ловит player-web/tests/conformance.test.js по эталонным трассам,
// которые записывает сам Rust-движок. Меняете одно — меняйте и другое.
(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.VN = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  const SAVE_VERSION = 1;
  const STEP_LIMIT = 10000;
  const BACKLOG_LIMIT = 500;
  const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);

  function valueText(v) {
    return v === true ? 'да' : v === false ? 'нет' : String(v);
  }

  function compare(a, cmp, b) {
    const ta = typeof a;
    if (ta !== typeof b || (ta === 'boolean' && cmp !== 'eq' && cmp !== 'ne')) return cmp === 'ne';
    const ord = a < b ? -1 : a > b ? 1 : 0;
    switch (cmp) {
      case 'eq': return ord === 0;
      case 'ne': return ord !== 0;
      case 'gt': return ord > 0;
      case 'lt': return ord < 0;
      case 'ge': return ord >= 0;
      default: return ord <= 0;
    }
  }

  function evalCond(c, vars) {
    switch (c.op) {
      case 'cmp': return own(vars, c.var) && compare(vars[c.var], c.cmp, c.value);
      case 'and': return c.items.every((x) => evalCond(x, vars));
      case 'or': return c.items.some((x) => evalCond(x, vars));
      default: return !evalCond(c.cond, vars);
    }
  }

  function applyVarOp(op, vars) {
    const v = op.value;
    const action = op.action || 'set';
    let next = v;
    if (typeof v === 'number' && action !== 'set') {
      const sign = action === 'add' ? 1 : -1;
      if (!own(vars, op.var)) next = sign * v;
      else if (typeof vars[op.var] === 'number') next = vars[op.var] + sign * v;
    }
    vars[op.var] = next;
  }

  function substitute(text, vars) {
    let out = '';
    let rest = text;
    for (;;) {
      const open = rest.indexOf('{');
      if (open < 0) break;
      const close = rest.indexOf('}', open);
      if (close < 0) break;
      const name = rest.slice(open + 1, close);
      out += rest.slice(0, open) + (own(vars, name) ? valueText(vars[name]) : rest.slice(open, close + 1));
      rest = rest.slice(close + 1);
    }
    return out + rest;
  }

  function spans(text, vars) {
    return substitute(text, vars)
      .split('*')
      .map((t, i) => ({ text: t, bold: i % 2 === 1 }))
      .filter((s) => s.text !== '');
  }

  const plainText = (text, vars) => spans(text, vars).map((s) => s.text).join('');

  function newStage() {
    return { background: null, bg_fade: 0, characters: [], hide_fade: 0, music: null, music_fade: 0, dialog: 'bottom', black: false, fade: 0 };
  }

  function applyStage(stage, e) {
    const shown = (id) => stage.characters.find((c) => c.character === id);
    const setExpression = (id, expression) => {
      const c = shown(id);
      if (c) c.expression = expression;
    };
    switch (e.type) {
      case 'background':
        stage.background = e.image === '' ? null : e.image;
        stage.bg_fade = e.fade;
        break;
      case 'show_character':
        stage.characters = stage.characters.filter((c) => c.character !== e.character);
        stage.characters.push({ character: e.character, expression: e.expression, pos: { ...e.pos }, fade: e.fade, move_time: 0 });
        break;
      case 'hide_character':
        stage.characters = stage.characters.filter((c) => c.character !== e.character);
        stage.hide_fade = e.fade;
        break;
      case 'move_character': {
        const c = shown(e.character);
        if (c) {
          c.pos = { ...e.pos };
          c.move_time = e.duration;
        }
        break;
      }
      case 'expression':
        setExpression(e.character, e.expression);
        break;
      case 'say':
        if (e.speaker != null && e.expression != null) setExpression(e.speaker, e.expression);
        break;
      case 'dialog_box':
        stage.dialog = e.pos;
        break;
      case 'play_music':
        stage.music = { track: e.track, volume: e.volume, looped: e.looped, fade: e.fade };
        stage.music_fade = e.fade;
        break;
      case 'stop_music':
        stage.music = null;
        stage.music_fade = e.fade;
        break;
      case 'music_volume':
        if (stage.music) stage.music.volume = e.volume;
        stage.music_fade = e.fade;
        break;
      case 'fade':
        stage.black = e.out;
        stage.fade = e.duration;
        break;
    }
  }

  const available = (options, vars) => options.filter((o) => o.cond == null || evalCond(o.cond, vars));

  class SaveError extends Error {
    constructor(code) {
      super(SaveError.messages[code] || code);
      this.code = code;
    }
  }
  SaveError.messages = {
    corrupt: 'Файл сохранения повреждён.',
    too_new: 'Сохранение сделано более новой версией игры. Обновите игру.',
    wrong_story: 'Это сохранение от другой истории.',
    incompatible: 'Сохранение несовместимо с этой версией истории: сцена, в которой оно сделано, удалена.',
  };

  function parseSave(json) {
    let v;
    try {
      v = JSON.parse(json);
    } catch (e) {
      throw new SaveError('corrupt');
    }
    const version = v && v.save_version;
    if (!Number.isInteger(version) || version < 1) throw new SaveError('corrupt');
    if (version > SAVE_VERSION) throw new SaveError('too_new');
    if (!v.state || typeof v.state.scene !== 'string') throw new SaveError('corrupt');
    return v;
  }

  class Runtime {
    constructor(story, state) {
      this.story = story;
      this.state = state;
      this.idx = 0;
      this.pending = { k: 'ended', title: '' };
      this.cues = [];
    }

    static start(story, sceneId) {
      const scene = sceneId != null ? sceneId : story.meta.start_scene != null ? story.meta.start_scene : '';
      const vars = {};
      for (const v of story.variables) vars[v.name] = v.initial;
      const state = {
        scene,
        event: null,
        vars,
        choices: [],
        backlog: [],
        stage: newStage(),
        outfit: {},
        unlocked: story.wardrobe.items.filter((i) => i.unlocked).map((i) => i.id).sort(),
        seen: [],
      };
      const rt = new Runtime(story, state);
      rt.run(false);
      return rt;
    }

    scene(id) {
      return this.story.scenes.find((s) => s.id === id);
    }
    character(id) {
      return this.story.characters.find((c) => c.id === id);
    }
    item(id) {
      return this.story.wardrobe.items.find((i) => i.id === id);
    }
    currentEvent() {
      const sc = this.scene(this.state.scene);
      return sc ? sc.events[this.idx] : undefined;
    }
    takeCues() {
      const c = this.cues;
      this.cues = [];
      return c;
    }

    options() {
      const e = this.currentEvent();
      return this.pending.k === 'choice' && e && e.type === 'choice' ? available(e.options, this.state.vars) : [];
    }

    advance() {
      switch (this.pending.k) {
        case 'delay':
          this.run(true);
          break;
        case 'text':
          if (this.state.event != null && !this.state.seen.includes(this.state.event)) {
            this.state.seen.push(this.state.event);
            this.state.seen.sort();
          }
          this.idx += 1;
          this.run(false);
          break;
        case 'wait':
        case 'wardrobe':
          this.idx += 1;
          this.run(false);
          break;
      }
    }

    choose(optionId) {
      const opt = this.options().find((o) => o.id === optionId);
      if (!opt || this.state.event == null) return false;
      for (const op of opt.effects || []) applyVarOp(op, this.state.vars);
      this.state.choices.push({ scene: this.state.scene, event: this.state.event, option: opt.id, text: opt.text });
      if (opt.goto != null) this.jump(opt.goto);
      else this.idx += 1;
      this.run(false);
      return true;
    }

    equip(slot, itemId) {
      if (itemId == null) {
        delete this.state.outfit[slot];
        return true;
      }
      const item = this.item(itemId);
      const ok = this.state.unlocked.includes(itemId) && !!item && item.slot === slot;
      if (ok) this.state.outfit[slot] = itemId;
      return ok;
    }

    jump(scene) {
      this.state.scene = scene;
      this.state.event = null;
      this.idx = 0;
    }

    run(skipDelay) {
      for (let n = 0; n < STEP_LIMIT; n++) {
        const sc = this.scene(this.state.scene);
        if (!sc) {
          this.pending = { k: 'error', message: `Сцена «${this.state.scene}» не найдена` };
          return;
        }
        const ev = sc.events[this.idx];
        if (!ev) {
          this.pending = { k: 'ended', title: '' };
          return;
        }
        this.state.event = ev.id;
        if ((ev.delay || 0) > 0 && !skipDelay) {
          this.pending = { k: 'delay', seconds: ev.delay };
          return;
        }
        skipDelay = false;
        const p = this.execute(ev, true);
        if (p) {
          this.pending = p;
          return;
        }
      }
      this.pending = { k: 'error', message: 'Сюжет зациклился: переходы идут по кругу без текста и выборов' };
    }

    execute(ev, fresh) {
      const st = this.state;
      applyStage(st.stage, ev);
      switch (ev.type) {
        case 'say':
          if (fresh) {
            const ch = ev.speaker != null ? this.character(ev.speaker) : undefined;
            st.backlog.push({ speaker: ch ? ch.name : '', text: plainText(ev.text, st.vars) });
            if (st.backlog.length > BACKLOG_LIMIT) st.backlog.shift();
          }
          return { k: 'text' };
        case 'choice':
          if (available(ev.options, st.vars).length > 0) return { k: 'choice' };
          break;
        case 'wait':
          return { k: 'wait', seconds: ev.seconds };
        case 'open_wardrobe':
          return { k: 'wardrobe', slots: ev.slots.slice() };
        case 'end':
          return { k: 'ended', title: ev.title };
        case 'goto':
          this.jump(ev.scene);
          return null;
        case 'branch':
          if (evalCond(ev.cond, st.vars)) {
            this.jump(ev.scene);
            return null;
          }
          break;
        case 'set_var':
          applyVarOp(ev, st.vars);
          break;
        case 'set_outfit':
          if (ev.item != null) st.outfit[ev.slot] = ev.item;
          else delete st.outfit[ev.slot];
          break;
        case 'unlock_item':
          if (!st.unlocked.includes(ev.item)) {
            st.unlocked.push(ev.item);
            st.unlocked.sort();
          }
          break;
        case 'sound':
          this.cues.push({ sound: ev.file, volume: ev.volume });
          break;
      }
      this.idx += 1;
      return null;
    }

    // ---------- картина для отрисовки (view.rs) ----------

    spriteLayers(ch, expression) {
      const found = ch.expressions.find((e) => e.name === expression) || ch.expressions[0];
      const body = found ? [found.image] : [];
      const w = this.story.wardrobe;
      if (w.hero !== ch.id) return body;
      const worn = (behind) =>
        w.slots
          .filter((s) => !!s.behind === behind && own(this.state.outfit, s.id))
          .map((s) => this.item(this.state.outfit[s.id]))
          .filter(Boolean)
          .map((i) => i.image);
      return [...worn(true), ...body, ...worn(false)];
    }

    view() {
      const st = this.state;
      const ev = this.currentEvent();
      const p = this.pending;
      let prompt;
      if (p.k === 'text' && ev && ev.type === 'say') {
        const ch = ev.speaker != null ? this.character(ev.speaker) : undefined;
        prompt = {
          kind: 'text',
          speaker: ch ? { name: ch.name, color: ch.color } : null,
          style: ev.style || 'dialogue',
          spans: spans(ev.text, st.vars),
          speed: ev.speed != null ? ev.speed : this.story.meta.text.speed,
          seen: st.event != null && st.seen.includes(st.event),
          sound: ev.sound ? ev.sound : null,
          color: (ev.look && ev.look.color) || this.story.meta.text.color,
          size: this.story.meta.text.size * ((ev.look && ev.look.size) || 1),
        };
      } else if (p.k === 'choice' && ev && ev.type === 'choice') {
        prompt = { kind: 'choice', prompt: plainText(ev.prompt, st.vars), options: this.options().map((o) => ({ id: o.id, text: plainText(o.text, st.vars) })) };
      } else if (p.k === 'delay' || p.k === 'wait') {
        prompt = { kind: 'wait', seconds: p.seconds };
      } else if (p.k === 'wardrobe') {
        prompt = { kind: 'wardrobe', slots: p.slots };
      } else if (p.k === 'ended') {
        prompt = { kind: 'ended', title: p.title };
      } else {
        prompt = { kind: 'error', message: p.message || 'Внутренняя ошибка движка' };
      }
      const sprites = [];
      for (const s of st.stage.characters) {
        const ch = this.character(s.character);
        if (!ch) continue;
        sprites.push({ id: ch.id, layers: this.spriteLayers(ch, s.expression), x: s.pos.x, y: s.pos.y, height: ch.height * s.pos.scale, fade: s.fade, move_time: s.move_time });
      }
      return {
        scene: st.scene,
        event: st.event,
        background: st.stage.background,
        bg_fade: st.stage.bg_fade,
        black: st.stage.black,
        fade: st.stage.fade,
        hide_fade: st.stage.hide_fade,
        sprites,
        music: st.stage.music ? { ...st.stage.music } : null,
        music_fade: st.stage.music_fade,
        dialog: st.stage.dialog,
        prompt,
      };
    }

    wardrobe(allowed) {
      const w = this.story.wardrobe;
      const st = this.state;
      return w.slots
        .filter((s) => !allowed || allowed.length === 0 || allowed.includes(s.id))
        .map((s) => ({
          id: s.id,
          name: s.name,
          items: w.items.filter((i) => i.slot === s.id && st.unlocked.includes(i.id)).map((i) => ({ id: i.id, name: i.name, image: i.image, equipped: st.outfit[s.id] === i.id })),
        }));
    }

    heroLayers() {
      const hero = this.story.wardrobe.hero != null ? this.character(this.story.wardrobe.hero) : undefined;
      if (!hero) return [];
      const shown = this.state.stage.characters.find((c) => c.character === hero.id);
      return this.spriteLayers(hero, shown ? shown.expression : '');
    }

    // ---------- сохранения (save.rs) ----------

    save(now) {
      const sc = this.scene(this.state.scene);
      const last = this.state.backlog[this.state.backlog.length - 1];
      const label = Array.from(`${sc ? sc.name : ''} — ${last ? last.text : ''}`).slice(0, 80).join('');
      return { save_version: SAVE_VERSION, story_id: this.story.meta.id, story_version: this.story.meta.story_version, saved_at: now, label, state: JSON.parse(JSON.stringify(this.state)) };
    }

    // Продолжить игру из сохранения на (возможно, обновлённой) истории.
    static restore(story, save) {
      if (save.story_id !== story.meta.id) throw new SaveError('wrong_story');
      const state = JSON.parse(JSON.stringify(save.state));
      state.choices = state.choices || [];
      state.backlog = state.backlog || [];
      state.stage = state.stage || newStage();
      state.outfit = state.outfit || {};
      state.unlocked = state.unlocked || [];
      state.seen = state.seen || [];
      for (const v of story.variables) if (!own(state.vars, v.name)) state.vars[v.name] = v.initial;
      const item = (id) => story.wardrobe.items.find((i) => i.id === id);
      for (const slot of Object.keys(state.outfit)) {
        const it = item(state.outfit[slot]);
        if (!it || it.slot !== slot) delete state.outfit[slot];
      }
      const unlocked = new Set(state.unlocked.filter((i) => item(i)));
      for (const i of story.wardrobe.items) if (i.unlocked) unlocked.add(i.id);
      state.unlocked = [...unlocked].sort();

      const findScene = (id) => story.scenes.find((s) => s.id === id);
      const sc = findScene(state.scene);
      let how;
      let idx = -1;
      if (sc) {
        idx = state.event != null ? sc.events.findIndex((e) => e.id === state.event) : -1;
        how = idx >= 0 ? { how: 'exact' } : { how: 'scene_restart' };
      } else {
        let target = null;
        let at = state.scene;
        const redirects = story.redirects || {};
        for (let n = 0; n < Object.keys(redirects).length; n++) {
          if (!own(redirects, at)) break;
          const next = redirects[at];
          if (findScene(next)) {
            target = next;
            break;
          }
          at = next;
        }
        if (target == null && story.meta.fallback_scene != null && findScene(story.meta.fallback_scene)) target = story.meta.fallback_scene;
        if (target == null) throw new SaveError('incompatible');
        state.scene = target;
        state.stage.characters = [];
        how = { how: 'redirected', to: target };
      }
      const rt = new Runtime(story, state);
      if (idx >= 0) {
        rt.idx = idx;
        const p = rt.execute(rt.currentEvent(), false);
        if (p) rt.pending = p;
        else rt.run(false);
      } else {
        rt.state.event = null;
        rt.run(false);
      }
      return { rt, how };
    }
  }

  return { Runtime, SaveError, parseSave, SAVE_VERSION };
});
