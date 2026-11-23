// Покадровая съёмка сцен макета через headless Chrome (CDP, без зависимостей) и сборка GIF через ffmpeg.
//
//   node scripts/capture.mjs                     — все сцены → mockup/gifs/*.gif
//   node scripts/capture.mjs editing palette     — только выбранные
//   node scripts/capture.mjs --stills editing    — только PNG-кадры для проверки (каждые 0.5 с)
//
// Переменные окружения: CHROME (путь к chrome/edge), FFMPEG (путь к ffmpeg ≥ 4 с palettegen), FPS (по умолчанию 20).
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdirSync, readFileSync, rmSync, writeFileSync, existsSync, statSync } from 'node:fs';
import { join, resolve, extname } from 'node:path';
import { tmpdir } from 'node:os';

const ROOT = resolve(import.meta.dirname, '..');
const OUT = join(ROOT, 'gifs');
const WORK = process.env.FRAMES_DIR || join(tmpdir(), 'anvil-frames');
const FPS = Number(process.env.FPS || 20);
const W = 1040, H = 680;
const CHROME = process.env.CHROME || [
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  '/usr/bin/google-chrome', '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
].find(p => existsSync(p));
const FFMPEG = process.env.FFMPEG || 'ffmpeg';
const ALL = ['launch', 'editing', 'palette', 'customize', 'collab'];
const NAMES = { launch: '01-launch', editing: '02-editing', palette: '03-palette', customize: '04-customize', collab: '05-collab' };

const args = process.argv.slice(2);
const stills = args.includes('--stills');
const scenes = args.filter(a => !a.startsWith('--'));
const todo = scenes.length ? scenes : ALL;
const sleep = ms => new Promise(r => setTimeout(r, ms));

// --- статический сервер (file:// не даёт грузить скрипты одинаково во всех браузерах)
const MIME = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8' };
const server = createServer((req, res) => {
  const p = new URL(req.url, 'http://x').pathname;
  const file = join(ROOT, p === '/' ? 'index.html' : p);
  if (!file.startsWith(ROOT) || !existsSync(file) || !statSync(file).isFile()) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': MIME[extname(file)] || 'application/octet-stream' });
  res.end(readFileSync(file));
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}/index.html`;

// --- браузер
if (!CHROME) throw new Error('Не найден Chrome/Edge — задайте переменную CHROME');
const port = 9300 + Math.floor(Math.random() * 500);
const profile = join(WORK, 'profile');
mkdirSync(profile, { recursive: true });
const chrome = spawn(CHROME, [
  '--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`,
  '--hide-scrollbars', '--force-device-scale-factor=1', '--force-color-profile=srgb', '--font-render-hinting=none',
  '--no-first-run', '--no-default-browser-check', `--window-size=${W},${H}`, 'about:blank',
], { stdio: 'ignore' });

let target;
for (let i = 0; i < 100 && !target; i++) {
  try { target = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find(t => t.type === 'page'); }
  catch { await sleep(100); }
}
if (!target) throw new Error('Chrome не поднял DevTools');
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let seq = 0;
const pending = new Map();
ws.onmessage = e => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.rej(new Error(m.error.message)) : p.res(m.result); }
};
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, { res, rej }); ws.send(JSON.stringify({ id, method, params })); });
const evaluate = async expr => {
  const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
  return r.result.value;
};

await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', { width: W, height: H, deviceScaleFactor: 1, mobile: false });

mkdirSync(OUT, { recursive: true });
for (const id of todo) {
  await send('Page.navigate', { url: `${base}?capture=1&scene=${id}` });
  for (let i = 0; i < 100; i++) { if (await evaluate('!!(window.anvil && window.anvil.ready)').catch(() => false)) break; await sleep(50); }
  const dur = await evaluate('anvil.duration()');
  const dir = join(WORK, id);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const step = stills ? 0.5 : 1 / FPS;
  const n = Math.round(dur / step);
  process.stdout.write(`${id}: ${dur.toFixed(2)} с, ${n} кадров `);
  for (let i = 0; i < n; i++) {
    await evaluate(`anvil.seek(${(i * step).toFixed(4)})`);
    const { data } = await send('Page.captureScreenshot', { format: 'png', clip: { x: 0, y: 0, width: W, height: H, scale: 1 } });
    writeFileSync(join(dir, `${String(i).padStart(4, '0')}.png`), Buffer.from(data, 'base64'));
    if (i % 20 === 0) process.stdout.write('.');
  }
  process.stdout.write('\n');
  if (stills) { console.log(`  кадры: ${dir}`); continue; }
  const gif = join(OUT, `${NAMES[id] || id}.gif`);
  const vf = 'split[a][b];[a]palettegen=max_colors=256:stats_mode=full:reserve_transparent=0[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle';
  const r = spawnSync(FFMPEG, ['-y', '-v', 'error', '-framerate', String(FPS), '-i', join(dir, '%04d.png'), '-vf', vf, '-loop', '0', gif], { stdio: 'inherit' });
  if (r.status !== 0) throw new Error(`ffmpeg завершился с кодом ${r.status}`);
  const poster = join(OUT, `${NAMES[id] || id}.png`);
  spawnSync(FFMPEG, ['-y', '-v', 'error', '-i', join(dir, `${String(Math.floor(n * 0.62)).padStart(4, '0')}.png`), poster], { stdio: 'inherit' });
  console.log(`  → ${gif} (${(statSync(gif).size / 1048576).toFixed(2)} МБ)`);
}

ws.close();
chrome.kill();
server.close();
