// AWBW's look for a storyboard, as settings a viewer can offer:
//
//   const art = awbwLook(sb, { animate, shoals, water });
//   new Replay(svg, sb, { data, art, hpBars, ... });
//
// animate: AWBW's animated set (units march, owned buildings flash; the browser plays the GIFs)
//   or its still one.
// shoals: 'custom' (AWBW's default: seas and shoals joined to their neighbours) or 'classic'.
//   Custom needs `water`, a table of where those tiles sit on AWBW's terrain sprite sheet:
//   {sheet: URL, width, height, tiles: {clear|rain|snow: {name: [x, y, w, h]}}}. libaw doesn't
//   ship it; a page that wants custom shoals provides one.
// Images load straight from AWBW. One that won't load is swapped for libaw's art (art.js).
import { AWBW_BASE, AWBW_STILL_BASE, awbwArt, awbwWaterName } from './art.js';

export function awbwLook(sb, { animate = true, shoals = 'custom', water = null } = {}) {
  const base = animate ? AWBW_BASE : AWBW_STILL_BASE;
  if (shoals !== 'custom' || !water) return awbwArt(sb.countries, { base });
  const weather = { R: 'rain', S: 'snow' }[sb.start.weather] ?? 'clear';
  const tiles = water.tiles?.[weather] ?? {};
  const w = sb.start.width, h = sb.start.height, ids = sb.start.terrain;
  const bld = new Map(sb.start.buildings.map(([x, y, b]) => [`${x},${y}`, b.terrain]));
  const idAt = (x, y) => (x < 0 || y < 0 || x >= w || y >= h ? undefined : bld.get(`${x},${y}`) ?? ids[y * w + x]);
  const at = new Map();
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const n = awbwWaterName(idAt, x, y);
    if (n && tiles[n]) at.set(`${x},${y}`, n);
  }
  return awbwArt(sb.countries, {
    base, nameAt: (id, x, y) => at.get(`${x},${y}`) ?? null,
    sheet: { url: water.sheet, width: water.width, height: water.height, tiles },
  });
}
