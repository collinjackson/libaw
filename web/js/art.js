// Art sets for the board. The default (no art set) is libaw's own placeholder art in
// assets/sprites.svg. `awbwArt` draws units with Advance Wars By Web's sprites, loaded from
// AWBW's own site at view time: libaw ships no AWBW art, only where to find it. A sprite that
// fails to load falls back to libaw's.
const SVGNS = 'http://www.w3.org/2000/svg';
export const AWBW_BASE = 'https://awbw.amarriner.com/terrain/aw2/';

export const spriteId = (kind) => `u-${kind.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')}`;

/** AWBW's file name for a unit kind: lower case, spaces dropped ("Md.Tank" -> "md.tank",
 *  "Mega Tank" -> "megatank", "Anti-Air" -> "anti-air"). */
export const awbwUnitName = (kind) => kind.toLowerCase().replace(/\s+/g, '');

/** countries: {playerId: AWBW country code ("os", "bm", "ci", ...)}, e.g. a storyboard's `countries`. */
export function awbwArt(countries, { base = AWBW_BASE } = {}) {
  return {
    name: 'awbw',
    unit(u, parent, { x, y, size, flip }) {
      const code = countries?.[u.owner];
      if (!code) return false;
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
  };
}
