// Art sets for the board. The default (no art set) is libaw's own placeholder art in
// assets/sprites.svg. `awbwArt` draws units and terrain with Advance Wars By Web's sprites, loaded
// from AWBW's own site at view time: libaw ships no AWBW art, only where to find it. A sprite that
// fails to load falls back to libaw's.
const SVGNS = 'http://www.w3.org/2000/svg';
// AWBW's animated set (units march, owned buildings flash); same names and sizes as the still
// aw2/ set, which has every one of them too (checked 2026-11-24).
export const AWBW_BASE = 'https://awbw.amarriner.com/terrain/ani/';
export const AWBW_STILL_BASE = 'https://awbw.amarriner.com/terrain/aw2/';
const AWBW_SITE = 'https://awbw.amarriner.com/';
/** AWBW's corner icons for a unit's status (C, L, hidden, stunned), under terrain/. */
export const AWBW_BADGES = { capture: 'aw1/capture', load: 'aw1/load', hide: 'aw1/subdive2', stun: 'stun' };

export const spriteId = (kind) => `u-${kind.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')}`;

/** AWBW's file name for a unit kind: lower case, spaces dropped ("Md.Tank" -> "md.tank",
 *  "Mega Tank" -> "megatank", "Anti-Air" -> "anti-air"). */
export const awbwUnitName = (kind) => kind.toLowerCase().replace(/\s+/g, '');

/** AWBW's country names by code, as its building images spell them ("os" -> "orangestar"). */
export const AWBW_COUNTRIES = {
  os: 'orangestar', bm: 'bluemoon', ge: 'greenearth', yc: 'yellowcomet', bh: 'blackhole', rf: 'redfire',
  gs: 'greysky', bd: 'browndesert', ab: 'amberblossom', js: 'jadesun', ci: 'cobaltice', pc: 'pinkcosmos',
  tg: 'tealgalaxy', pl: 'purplelightning', ar: 'acidrain', wn: 'whitenova', aa: 'azureasteroid',
  ne: 'noireclipse', sc: 'silverclaw', uw: 'umberwilds',
};

/** The countries whose AWBW unit sprites face left; the rest face right (read off each country's
 *  Anti-Air guns). AWBW draws them as they are, so a map's sides don't always face each other. */
export const AWBW_FACES_LEFT = new Set(['bm', 'yc', 'bh', 'rf', 'ab', 'pc', 'tg', 'ar', 'ne', 'sc']);

/** Which way an AWBW country's unit sprites face: 'left' or 'right'. */
export const awbwFacing = (code) => (AWBW_FACES_LEFT.has(code) ? 'left' : 'right');

// Tiles whose AWBW image names don't follow their terrain names: shoals, pipe seams (both are
// "Pipe Seam"; AWBW's images say which way the pipe runs) and the Teleport Tile ("teleporter").
// Checked against AWBW 2026-11-24: every other non-building name, and every country's Lab and
// Com Tower, loads. The ids 1000+ (Sea-N-E-S-W, Shoal-AN...) are the Replay Player's own and
// have no AWBW image; they fall back to libaw's tile.
const AWBW_NAMES = { 29: 'hshoal', 30: 'hshoaln', 31: 'vshoal', 32: 'vshoale', 113: 'hpipeseam', 114: 'vpipeseam',
  195: 'teleporter' };

/** AWBW's image name for a tile: the terrain's name in lower case without spaces ("ESRoad" ->
 *  "esroad", "Neutral City" -> "neutralcity"); a building takes its current owner's country
 *  (ownerCode), since a capture changes the owner and not the tile. d: the terrain-map entry. */
export function awbwTileName(id, d, ownerCode) {
  if (AWBW_NAMES[id]) return AWBW_NAMES[id];
  if (d?.building) {
    const country = ownerCode ? AWBW_COUNTRIES[ownerCode] : 'neutral';
    return country ? `${country}${d.building.replace(/-/g, '')}` : null;
  }
  return d?.name ? d.name.toLowerCase().replace(/\s+/g, '') : null;
}

// AWBW's map renderer joins seas and shoals to their neighbours: it names each sea and shoal tile
// by what's around it ("sea<mask>", "shoal<0-80>") and draws that tile from its terrain sprite
// sheet (terrain/terrain_spritesheet.png; terrain/tilesets.js says where each name sits). Its
// default "new" shoals are the ones players call custom; the classic ones are aw2/'s GIFs.
const RIVER_TO = {   // the river ids whose water runs off the tile on that side
  n: [5, 6, 9, 10, 12, 13, 14], s: [5, 6, 7, 8, 11, 12, 14], e: [4, 6, 7, 10, 11, 13, 14], w: [4, 6, 8, 9, 11, 12, 13],
};
const isRiver = (id) => id >= 4 && id <= 14;

/** AWBW's joined name for the sea (28) or shoal (29-32) at x, y, or null for any other tile.
 *  idAt(x, y): the terrain id there (a building's own id), undefined off the map. */
export function awbwWaterName(idAt, x, y) {
  const id = idAt(x, y);
  if (id === 28) {
    // Eight neighbours from the top left, clockwise; a bit set where land meets this sea. A river
    // flowing in from a side wets that side's two corners instead.
    const ring = [[-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0]];
    const inflow = { 1: ['s', 0x05], 3: ['w', 0x14], 5: ['n', 0x50], 7: ['e', 0x41] };
    let land = 0;
    ring.forEach(([dx, dy], k) => {
      const n = idAt(x + dx, y + dy) ?? 32;   // off the map counts as water
      if ((n >= 26 && n <= 33) || n === 195) return;
      if (isRiver(n) && inflow[k] && RIVER_TO[inflow[k][0]].includes(n)) land |= inflow[k][1];
      else land |= 1 << k;
    });
    // A corner only counts where neither side next to it is land.
    land &= ~(((land << 1) | (land >> 1) | (land >> 7)) & 0x55);
    return `sea${land}`;
  }
  if (id >= 29 && id <= 32) {
    // Up, left, right, down, in base 3: 0 open water (or the map edge), 1 joins this shoal (shoal,
    // teleporter, a bridge or river running into it), 2 land.
    const sides = [[0, -1, 's', 26], [-1, 0, 'e', 27], [1, 0, 'w', 27], [0, 1, 'n', 26]];
    let v = 0;
    sides.forEach(([dx, dy, flow, bridge], k) => {
      const n = idAt(x + dx, y + dy);
      let t = 2;
      if (n === undefined || n === 28 || n === 33) t = 0;
      else if ((n >= 29 && n <= 32) || n === 195 || n === bridge) t = 1;
      else if (isRiver(n) && RIVER_TO[flow].includes(n)) t = 1;
      v += 3 ** k * t;
    });
    return `shoal${v}`;
  }
  return null;
}

/** countries: {playerId: AWBW country code ("os", "bm", "ci", ...)}, e.g. a storyboard's `countries`.
 *  missing: image names (as awbwTileName and `${code}${awbwUnitName(kind)}` give them) known not to
 *  load, drawn with libaw's art from the start. Without it a missing image shows the browser's
 *  broken-image icon until its error fires, which a recording can catch in a frame.
 *  src: (name) => an image URL to draw for that name instead of base's GIF, or null. A recording
 *  uses it to show one frame of an animated GIF chosen by its own clock (a browser plays GIFs by
 *  the wall clock, which a frame-by-frame recording doesn't follow).
 *  nameAt: (id, x, y) => the image name for the tile at board position x, y, or null for
 *  awbwTileName's (e.g. awbwWaterName's joined seas and shoals, with src or sheet drawing them).
 *  sheet: {url, width, height, tiles: {name: [x, y, w, h]}}: names drawn as that region of one
 *  sprite sheet image (AWBW's terrain sheet holds its joined seas and custom shoals). Drawn by
 *  clipping, so the sheet needs no CORS headers. */
export function awbwArt(countries, { base = AWBW_BASE, missing = new Set(), src = () => null, nameAt = () => null, sheet = null } = {}) {
  const href = (name) => src(name) ?? `${base}${name}.gif`;
  const tall = new Map();   // sprite name -> height / width, once loaded
  return {
    name: 'awbw',
    /** Which way u's sprite faces as drawn with flip false ('left' or 'right'), or null when this
     *  art set has no sprite for it. libaw's own sprites face right. */
    facing(u) {
      const code = countries?.[u.owner];
      return code ? awbwFacing(code) : null;
    },
    unit(u, parent, { x, y, size, flip }) {
      const code = countries?.[u.owner];
      if (!code || missing.has(`${code}${awbwUnitName(u.kind)}`)) return false;
      // AWBW's sprites already face their side's way: undo the board's mirror for odd seats.
      const g = flip ? document.createElementNS(SVGNS, 'g') : parent;
      if (flip) { g.setAttribute('transform', `translate(${2 * x + size} 0) scale(-1 1)`); parent.appendChild(g); }
      const img = document.createElementNS(SVGNS, 'image');
      for (const [k, v] of Object.entries({ x, y, width: size, height: size, preserveAspectRatio: 'xMidYMax meet' })) img.setAttribute(k, v);
      img.setAttribute('href', href(`${code}${awbwUnitName(u.kind)}`));
      img.style.imageRendering = 'pixelated';
      img.addEventListener('error', () => {
        const use = document.createElementNS(SVGNS, 'use');
        for (const [k, v] of Object.entries({ href: `#${spriteId(u.kind)}`, x, y, width: size, height: size })) use.setAttribute(k, v);
        img.replaceWith(use);
      }, { once: true });
      g.appendChild(img);
      return true;
    },
    /** A unit's status badge (board.js unitStatus) with AWBW's icon, in the tile's bottom-left
     *  corner at half a tile, as AWBW draws it (its game page takes these from terrain/aw1/). */
    badge(status, parent, { x, y, size }) {
      const file = AWBW_BADGES[status];
      if (!file) return false;
      const img = document.createElementNS(SVGNS, 'image');
      const s = size / 2;
      for (const [k, v] of Object.entries({ x, y: y + size - s, width: s, height: s, href: `${AWBW_SITE}terrain/${file}.gif` })) img.setAttribute(k, v);
      img.style.imageRendering = 'pixelated';
      img.addEventListener('error', () => img.remove(), { once: true });
      parent.appendChild(img);
      return true;
    },
    /** One tile, bottom-aligned in a box two tiles tall (AWBW's buildings and mountains stand
     *  taller than a tile); rows are drawn top to bottom, so they overlap the row above. */
    /** How far (in tiles) this tile's sprite stands above its tile: AWBW's HQ, bases and
     *  mountains are taller than they are wide. 0 until measure() has loaded the sprite. */
    overhang(id, d, owner, { x, y, size }) {
      const name = this.nameFor(id, d, owner, { x, y, size });
      if (!name || (!src(name) && sheet?.tiles?.[name])) return 0;
      return Math.max(0, (tall.get(name) ?? 1) - 1);
    },
    nameFor(id, d, owner, { x, y, size }) {
      return nameAt(id, Math.round(x / size), Math.round(y / size)) ?? awbwTileName(id, d, owner == null ? null : countries?.[owner]);
    },
    /** Load the sprites for these tiles ([[id, entry, owner, box]]) to learn their heights; resolves
     *  when all have loaded or failed, so a page can size its board before the first frame. */
    measure(tiles) {
      if (typeof Image === 'undefined') return Promise.resolve();
      const wait = new Promise((done) => setTimeout(done, 4000));   // never hold a page up for long
      return Promise.race([wait, Promise.all(tiles.map(([id, d, owner, box]) => {
        const name = this.nameFor(id, d, owner, box);
        if (!name || tall.has(name) || (!src(name) && sheet?.tiles?.[name])) return null;
        return new Promise((done) => {
          const im = new Image();
          im.onload = () => { if (im.naturalWidth) tall.set(name, im.naturalHeight / im.naturalWidth); done(); };
          im.onerror = () => { tall.set(name, 1); done(); };
          im.src = href(name);
        });
      }))]);
    },
    tile(id, d, owner, parent, { x, y, size }) {
      const name = this.nameFor(id, d, owner, { x, y, size });
      if (!name || missing.has(name)) return false;
      const cell = !src(name) && sheet?.tiles?.[name];
      if (cell) {
        // A region of the sheet: an inner <svg> whose viewBox is the tile's rectangle.
        const box = document.createElementNS(SVGNS, 'svg');
        for (const [k, v] of Object.entries({ x, y, width: size, height: size, viewBox: cell.join(' '), preserveAspectRatio: 'none' })) box.setAttribute(k, v);
        const img = document.createElementNS(SVGNS, 'image');
        for (const [k, v] of Object.entries({ x: 0, y: 0, width: sheet.width, height: sheet.height, href: sheet.url })) img.setAttribute(k, v);
        img.style.imageRendering = 'pixelated';
        img.addEventListener('error', () => {
          const use = document.createElementNS(SVGNS, 'use');
          for (const [k, v] of Object.entries({ href: `#t-${d?.base ?? 'sea'}`, x, y, width: size, height: size })) use.setAttribute(k, v);
          box.replaceWith(use);
        }, { once: true });
        box.appendChild(img);
        parent.appendChild(box);
        return true;
      }
      // AWBW's woods, mountains and buildings are drawn over a plain: their GIFs are transparent.
      if (name !== 'plain' && !/sea|shoal|reef|river|bridge/.test(name)) {
        const under = document.createElementNS(SVGNS, 'image');
        for (const [k, v] of Object.entries({ x, y, width: size, height: size, href: href('plain') })) under.setAttribute(k, v);
        under.style.imageRendering = 'pixelated';
        parent.appendChild(under);
      }
      const img = document.createElementNS(SVGNS, 'image');
      for (const [k, v] of Object.entries({ x, y: y - size, width: size, height: 2 * size, preserveAspectRatio: 'xMidYMax meet' })) img.setAttribute(k, v);
      img.setAttribute('href', href(name));
      img.style.imageRendering = 'pixelated';
      img.addEventListener('error', () => {
        const use = document.createElementNS(SVGNS, 'use');
        for (const [k, v] of Object.entries({ href: `#t-${d?.base ?? 'plain'}`, x, y, width: size, height: size })) use.setAttribute(k, v);
        img.replaceWith(use);
      }, { once: true });
      parent.appendChild(img);
      return true;
    },
  };
}
