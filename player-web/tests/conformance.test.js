// Проверяет, что веб-движок (runtime.js) показывает ровно то же, что Rust-движок.
// Эталон trace.json записывает `cargo test -p vn-core` (см. crates/vn-core/tests/conformance.rs).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const VN = require('../runtime.js');

const trace = JSON.parse(fs.readFileSync(path.join(__dirname, 'trace.json'), 'utf8'));
const clone = (x) => JSON.parse(JSON.stringify(x));

// Сравнение с допуском на числа: Rust считает в f32, JavaScript — в f64.
function same(actual, expected, where) {
  if (typeof expected === 'number' && typeof actual === 'number') {
    assert.ok(Math.abs(actual - expected) <= 1e-5 * Math.max(1, Math.abs(expected)), `${where}: ${actual} ≠ ${expected}`);
  } else if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), `${where}: ожидался массив`);
    assert.strictEqual(actual.length, expected.length, `${where}: длина массива`);
    expected.forEach((e, i) => same(actual[i], e, `${where}[${i}]`));
  } else if (expected && typeof expected === 'object') {
    assert.ok(actual && typeof actual === 'object' && !Array.isArray(actual), `${where}: ожидался объект, получено ${JSON.stringify(actual)}`);
    assert.deepStrictEqual(Object.keys(actual).sort(), Object.keys(expected).sort(), `${where}: набор полей`);
    for (const k of Object.keys(expected)) same(actual[k], expected[k], `${where}.${k}`);
  } else {
    assert.strictEqual(actual, expected, where);
  }
}

function snapshot(rt, ok, cmd) {
  return { cmd, ok, cues: rt.takeCues(), view: rt.view(), state: rt.state, wardrobe: rt.wardrobe([]), hero_layers: rt.heroLayers() };
}

function run(rt, cmd) {
  switch (cmd[0]) {
    case 'advance':
      rt.advance();
      return true;
    case 'choose':
      return rt.choose(cmd[1]);
    case 'equip':
      return rt.equip(cmd[1], cmd[2]);
    case 'unequip':
      return rt.equip(cmd[1], null);
    default:
      throw new Error(`неизвестная команда ${cmd}`);
  }
}

trace.plays.forEach((play, n) => {
  test(`прохождение ${n + 1} (старт: ${play.start ?? 'начало'}, шагов: ${play.steps.length})`, () => {
    const rt = VN.Runtime.start(clone(trace.story), play.start ?? undefined);
    play.steps.forEach((step, i) => {
      const ok = i === 0 ? true : run(rt, step.cmd);
      same(clone(snapshot(rt, ok, step.cmd)), step, `шаг ${i} ${JSON.stringify(step.cmd)}`);
    });
  });
});

trace.restores.forEach((c) => {
  test(`загрузка сохранения: ${c.name}`, () => {
    const save = VN.parseSave(JSON.stringify(c.save));
    if (c.expect.error) {
      assert.throws(() => VN.Runtime.restore(clone(c.story), save), (e) => e instanceof VN.SaveError && e.code === c.expect.error);
      return;
    }
    const { rt, how } = VN.Runtime.restore(clone(c.story), save);
    same(how, c.expect.result, 'способ продолжения');
    same(clone(snapshot(rt, true, ['restore'])), c.expect.snapshot, 'картина после загрузки');
  });
});

test('сохранение записывается в том же формате', () => {
  const rt = VN.Runtime.restore(clone(trace.story), clone(trace.save_sample)).rt;
  same(clone(rt.save(trace.save_sample.saved_at)), trace.save_sample, 'save');
});

test('негодные сохранения отвергаются с понятной причиной', () => {
  const code = (json) => {
    try {
      VN.parseSave(json);
    } catch (e) {
      return e.code;
    }
    return 'ok';
  };
  assert.strictEqual(code('{не json'), 'corrupt');
  assert.strictEqual(code('{}'), 'corrupt');
  assert.strictEqual(code(JSON.stringify({ ...trace.save_sample, save_version: 99 })), 'too_new');
  assert.strictEqual(code(JSON.stringify(trace.save_sample)), 'ok');
});
