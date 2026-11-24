// Art sets for the board. The default (no art set) is libaw's own placeholder art in
// assets/sprites.svg. `awbwArt` draws units and terrain with Advance Wars By Web's sprites, loaded
// from AWBW's own site at view time: libaw ships no AWBW art, only where to find it. A sprite that
// fails to load falls back to libaw's.
const SVGNS = 'http://www.w3.org/2000/svg';
export const AWBW_BASE = 'https://awbw.amarriner.com/terrain/aw2/';

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

/** countries: {playerId: AWBW country code ("os", "bm", "ci", ...)}, e.g. a storyboard's `countries`.
 *  missing: image names (as awbwTileName and `${code}${awbwUnitName(kind)}` give them) known not to
 *  load, drawn with libaw's art from the start. Without it a missing image shows the browser's
 *  broken-image icon until its error fires, which a recording can catch in a frame. */
export function awbwArt(countries, { base = AWBW_BASE, missing = new Set() } = {}) {
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
      img.setAttribute('href', `${base}${code}${awbwUnitName(u.kind)}.gif`);
      img.style.imageRendering = 'pixelated';
      img.addEventListener('error', () => {
        const use = document.createElementNS(SVGNS, 'use');
        for (const [k, v] of Object.entries({ href: `#${spriteId(u.kind)}`, x, y, width: size, height: size })) use.setAttribute(k, v);
        img.replaceWith(use);
      }, { once: true });
      g.appendChild(img);
      return true;
    },
    /** One tile, bottom-aligned in a box two tiles tall (AWBW's buildings and mountains stand
     *  taller than a tile); rows are drawn top to bottom, so they overlap the row above. */
    tile(id, d, owner, parent, { x, y, size }) {
      const name = awbwTileName(id, d, owner == null ? null : countries?.[owner]);
      if (!name || missing.has(name)) return false;
      // AWBW's woods, mountains and buildings are drawn over a plain: their GIFs are transparent.
      if (name !== 'plain' && !/sea|shoal|reef|river|bridge/.test(name)) {
        const under = document.createElementNS(SVGNS, 'image');
        for (const [k, v] of Object.entries({ x, y, width: size, height: size, href: `${base}plain.gif` })) under.setAttribute(k, v);
        under.style.imageRendering = 'pixelated';
        parent.appendChild(under);
      }
      const img = document.createElementNS(SVGNS, 'image');
      for (const [k, v] of Object.entries({ x, y: y - size, width: size, height: 2 * size, preserveAspectRatio: 'xMidYMax meet' })) img.setAttribute(k, v);
      img.setAttribute('href', `${base}${name}.gif`);
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
