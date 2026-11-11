// Проверка веб-плеера в настоящем Chrome с экраном телефона.
// Запуск: node player-web/tests/browser-check.mjs <папка веб-сборки> <папка для кадров>
// Проходит демо-историю касаниями, снимает кадры и падает при любой ошибке JavaScript.
import { spawn } from 'node:child_process';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

const [root, shots] = process.argv.slice(2);
fs.mkdirSync(shots, { recursive: true });
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.png': 'image/png', '.svg': 'image/svg+xml', '.wav': 'audio/wav', '.webmanifest': 'application/manifest+json' };
const server = http.createServer((req, res) => {
  const file = path.join(root, decodeURIComponent(req.url.split('?')[0].replace(/\/$/, '/index.html')));
  fs.readFile(file, (err, data) => {
    res.writeHead(err ? 404 : 200, { 'content-type': types[path.extname(file)] || 'application/octet-stream' });
    res.end(err ? 'нет файла' : data);
  });
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://localhost:${server.address().port}/`;

const chrome = spawn('google-chrome', ['--headless=new', '--no-sandbox', '--disable-gpu', '--remote-debugging-port=9333', '--autoplay-policy=no-user-gesture-required', '--user-data-dir=/tmp/vn-chrome-' + Date.now(), 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let target;
for (let i = 0; i < 50 && !target; i++) {
  await sleep(200);
  try {
    target = (await (await fetch('http://127.0.0.1:9333/json')).json()).find((t) => t.type === 'page');
  } catch {}
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const waiting = new Map();
const problems = [];
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id && waiting.has(msg.id)) waiting.get(msg.id)(msg.result);
  if (msg.method === 'Runtime.exceptionThrown') problems.push(msg.params.exceptionDetails.exception?.description || msg.params.exceptionDetails.text);
  if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') problems.push(msg.params.args.map((a) => a.value || a.description).join(' '));
  if (msg.method === 'Network.loadingFailed' && !msg.params.canceled) problems.push('не загрузилось: ' + msg.params.errorText);
};
const send = (method, params = {}) => new Promise((r) => (waiting.set(++id, r), ws.send(JSON.stringify({ id, method, params }))));
const js = async (expression) => (await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })).result?.value;
const shot = async (name) => fs.writeFileSync(path.join(shots, name + '.png'), Buffer.from((await send('Page.captureScreenshot', { format: 'png' })).data, 'base64'));
const tapText = async (text) => {
  const ok = await js(`(() => { const b = [...document.querySelectorAll('button')].find(b => b.textContent.includes(${JSON.stringify(text)}) && b.offsetParent); if (b) b.click(); return !!b; })()`);
  if (!ok) problems.push(`нет кнопки «${text}»`);
  await sleep(500);
};
const tapStage = async (n = 1) => {
  for (let i = 0; i < n; i++) {
    await js(`document.getElementById('textbox').click()`);
    await sleep(350);
  }
};
const text = () => js(`document.getElementById('text').textContent`);

await send('Runtime.enable');
await send('Network.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', { width: 844, height: 390, deviceScaleFactor: 2, mobile: true });
await send('Page.navigate', { url: base });
await sleep(1500);
await shot('w1_title');
await tapText('Новая игра');
await sleep(1200);
await shot('w2_first_line');
const first = await text();
await tapStage(8);
await shot('w3_wardrobe');
await tapText('Худи');
await tapText('Длинные волосы');
await shot('w4_dressed');
await tapText('Готово');
await tapStage(2);
await sleep(1500);
await tapStage(6);
await shot('w5_choice');
const choices = await js(`[...document.querySelectorAll('.choice')].map(b => b.textContent)`);
await tapText('Помочь донести');
await sleep(800);
await tapStage(2);
await shot('w6_cafe');
await js(`document.querySelector('[data-act=quicksave]').click()`);
const afterSave = await text();
await tapStage(6);
await js(`document.querySelector('[data-act=menu]').click()`);
await sleep(300);
await tapText('Загрузить');
await shot('w7_saves');
await tapText('Быстрое сохранение');
await sleep(600);
const afterLoad = await text();
await shot('w8_loaded');
const vars = await js(`JSON.parse(localStorage.getItem(Object.keys(localStorage).find(k => k.endsWith('save:quick')))).state.vars`);

console.log(JSON.stringify({ first, choices, afterSave, afterLoad, vars, problems }, null, 1));
ws.close();
chrome.kill();
server.close();
const ok = problems.length === 0 && first.length > 5 && choices.length === 3 && afterSave === afterLoad && vars['доверие'] === 2;
process.exit(ok ? 0 : 1);
