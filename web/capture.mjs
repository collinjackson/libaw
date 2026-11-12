// Record web/replay.html as a GIF and/or MP4, frame by frame (renderAt(t)), so the file shows
// exactly what the page does. Needs puppeteer (npm i -g puppeteer) and ffmpeg.
//
//   node web/capture.mjs 'http://127.0.0.1:8000/web/replay.html?sb=...' out/reel [--fps 15] [--width 720] [--gif] [--mp4]
//
// Serve the repo root first (python3 -m http.server 8000 --bind 127.0.0.1).
import { mkdtempSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { execFileSync } from 'child_process';
import { createRequire } from 'module';

const args = process.argv.slice(2);
const flag = (n, d) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : d; };
const [url, out] = args.filter((a, i) => !a.startsWith('--') && !(i > 0 && ['--fps', '--width'].includes(args[i - 1])));
if (!url || !out) { console.error('usage: node web/capture.mjs URL OUT_PREFIX [--fps 15] [--width 720] [--gif] [--mp4]'); process.exit(2); }
const fps = Number(flag('--fps', 15)), width = Number(flag('--width', 720));
const wantGif = args.includes('--gif') || !args.includes('--mp4'), wantMp4 = args.includes('--mp4');

// A global install is not on Node's ESM path: find it through npm's root.
const require = createRequire(import.meta.url);
let puppeteer;
try { puppeteer = require('puppeteer'); } catch {
  const root = execFileSync('npm', ['root', '-g']).toString().trim();
  puppeteer = require(join(root, 'puppeteer'));
}

const browser = await puppeteer.launch({ headless: true });
const page = await browser.newPage();
await page.setViewport({ width, height: Math.round(width * 0.75), deviceScaleFactor: 1 });
const u = new URL(url); u.searchParams.set('capture', '1');
await page.goto(u.toString(), { waitUntil: 'networkidle0' });
await page.waitForFunction(() => window.__replay?.ready);
const duration = await page.evaluate(() => window.__replay.duration);
const dir = mkdtempSync(join(tmpdir(), 'libaw-capture-'));
const n = Math.max(1, Math.ceil(duration * fps));
const screen = await page.$('#screen');
for (let f = 0; f <= n; f++) {
  await page.evaluate((t) => window.__replay.renderAt(t), Math.min(duration, f / fps));
  // Sprites from an art set's site: let any new image finish before the shot.
  await page.evaluate(() => Promise.all([...document.querySelectorAll('#board image')].map((im) =>
    new Promise((ok) => { const i = new Image(); i.onload = i.onerror = ok; i.src = im.getAttribute('href'); }))));
  writeFileSync(join(dir, `f${String(f).padStart(5, '0')}.png`), await screen.screenshot());
}
await browser.close();
const frames = join(dir, 'f%05d.png');
if (wantGif) {
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-framerate', String(fps), '-i', frames, '-vf',
    `split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle`,
    '-loop', '0', `${out}.gif`]);
}
if (wantMp4) {
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-framerate', String(fps), '-i', frames,
    '-vf', 'pad=ceil(iw/2)*2:ceil(ih/2)*2', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', `${out}.mp4`]);
}
rmSync(dir, { recursive: true, force: true });
console.log(JSON.stringify({ frames: n + 1, duration, gif: wantGif ? `${out}.gif` : null, mp4: wantMp4 ? `${out}.mp4` : null }));
