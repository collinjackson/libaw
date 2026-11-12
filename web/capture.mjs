// Record web/replay.html as a GIF and/or MP4, frame by frame (renderAt(t)), so the file shows
// exactly what the page does. Needs puppeteer (npm i -g puppeteer) and ffmpeg.
//
//   node web/capture.mjs 'http://127.0.0.1:8000/web/replay.html?sb=...' out/turn [--fps 15] [--width 720] [--gif] [--mp4] [--vf FILTERS] [--pages N]
//
// --pages N records the frames on N pages at once (renderAt(t) depends only on t), for machines
// with spare cores: a slow cloud CPU spent most of a reel taking screenshots one by one.
//
// Any page that sets window.__replay = {duration, renderAt(t), ready} records the same way.
//
// Serve the repo root first (python3 -m http.server 8000 --bind 127.0.0.1).
import { mkdtempSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { pathToFileURL } from 'url';
import { execFileSync } from 'child_process';
import { createRequire } from 'module';

const args = process.argv.slice(2);
const flag = (n, d) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : d; };
const [url, out] = args.filter((a, i) => !a.startsWith('--') && !(i > 0 && ['--fps', '--width', '--vf', '--pages'].includes(args[i - 1])));
if (!url || !out) { console.error('usage: node web/capture.mjs URL OUT_PREFIX [--fps 15] [--width 720] [--gif] [--mp4]'); process.exit(2); }
const fps = Number(flag('--fps', 15)), width = Number(flag('--width', 720));
const pages = Math.max(1, Number(flag('--pages', 1)) || 1);
const wantGif = args.includes('--gif') || !args.includes('--mp4'), wantMp4 = args.includes('--mp4');

// A global install is not on Node's ESM path: find it through npm's root.
const require = createRequire(import.meta.url);
// puppeteer is ESM from v24: import it (a require of it fails on Node < 22.12).
let puppeteer;
try { puppeteer = (await import('puppeteer')).default; } catch {
  const root = execFileSync('npm', ['root', '-g']).toString().trim();
  puppeteer = (await import(pathToFileURL(require.resolve(join(root, 'puppeteer'))).href)).default;
}

// CHROME_ARGS: extra flags, e.g. "--no-sandbox" in a container.
const t0 = Date.now();
// One browser per page: a background tab's screenshots never finish, so each page gets a
// browser of its own where it is the page in front.
const browsers = [];
const launch = async () => { const b = await puppeteer.launch({ headless: true, args: (process.env.CHROME_ARGS || '').split(' ').filter(Boolean) }); browsers.push(b); return b; };
// A page with the reel loaded and ready, its errors logged (so a page that never gets ready
// says why).
const u = new URL(url); u.searchParams.set('capture', '1');
async function open() {
  const page = await (await launch()).newPage();
  await page.setViewport({ width, height: Math.round(width * 0.75), deviceScaleFactor: 1 });
  page.on('pageerror', (e) => console.error('page error:', e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('page console:', m.text()); });
  page.on('requestfailed', (r) => console.error('request failed:', r.url(), r.failure()?.errorText));
  page.on('response', (r) => { if (r.status() >= 400 && !r.url().endsWith('/favicon.ico')) console.error('page got', r.status(), r.url()); });
  await page.goto(u.toString(), { waitUntil: 'networkidle0' });
  await page.waitForFunction(() => window.__replay?.ready, { timeout: 20000 });
  return page;
}
const first = await open();
const duration = await first.evaluate(() => window.__replay.duration);
const tLoaded = Date.now();
const dir = mkdtempSync(join(tmpdir(), 'libaw-capture-'));
const n = Math.max(1, Math.ceil(duration * fps));
// Page k records frames k, k + pages, k + 2 * pages, ...
async function record(page, k) {
  const screen = await page.$('#screen');
  for (let f = k; f <= n; f += pages) {
    await page.evaluate((t) => window.__replay.renderAt(t), Math.min(duration, f / fps));
    // Sprites from an art set's site: let any image not seen before finish before the shot (each
    // sprite once a page, not once a frame: on a slow machine the per-frame wait dominated).
    await page.evaluate(() => {
      const seen = (window.__captureSeen ||= new Set());
      const fresh = [...new Set([...document.querySelectorAll('#board image')].map((im) => im.getAttribute('href')))]
        .filter((src) => src && !seen.has(src));
      fresh.forEach((src) => seen.add(src));
      return Promise.all(fresh.map((src) => new Promise((ok) => { const i = new Image(); i.onload = i.onerror = ok; i.src = src; })));
    });
    writeFileSync(join(dir, `f${String(f).padStart(5, '0')}.png`), await screen.screenshot({ optimizeForSpeed: true }));
  }
}
const others = await Promise.all(Array.from({ length: Math.min(pages, n + 1) - 1 }, open));
await Promise.all([first, ...others].map((page, k) => record(page, k)));
await Promise.all(browsers.map((b) => b.close()));
const tFrames = Date.now();
const frames = join(dir, 'f%05d.png');
// --vf FILTERS: an ffmpeg filter chain to run on the frames first (e.g. a palette or pixel look).
const pre = flag('--vf', '') ? `${flag('--vf', '')},` : '';
if (wantGif) {
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-framerate', String(fps), '-i', frames, '-vf',
    `${pre}split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle`,
    '-loop', '0', `${out}.gif`]);
}
if (wantMp4) {
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-framerate', String(fps), '-i', frames,
    '-vf', `${pre}pad=ceil(iw/2)*2:ceil(ih/2)*2`, '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', `${out}.mp4`]);
}
rmSync(dir, { recursive: true, force: true });
const secs = (a, b) => Math.round((b - a) / 100) / 10;
console.log(JSON.stringify({ frames: n + 1, duration, seconds: { load: secs(t0, tLoaded), frames: secs(tLoaded, tFrames), encode: secs(tFrames, Date.now()) }, gif: wantGif ? `${out}.gif` : null, mp4: wantMp4 ? `${out}.mp4` : null }));
