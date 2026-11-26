// Draws a libaw state as one SVG, using the symbols in assets/sprites.svg (inlined into the
// page once). Team colours come from CSS classes .p0 .. .p7 on each unit/building group, by
// the player's place in turn order; neutral buildings get .pn.
import { key, displayHp, hpUnknown, unitsByTile, buildingsByTile, terrainAt } from './logic.js';

export const TILE = 40;
const SVGNS = 'http://www.w3.org/2000/svg';

function el(name, attrs = {}, parent) {
  const e = document.createElementNS(SVGNS, name);
  for (const [k, v] of Object.entries(attrs)) if (v != null) e.setAttribute(k, v);
  if (parent) parent.appendChild(e);
  return e;
}

function use(id, x, y, parent, extra = {}) {
  return el('use', { href: `#${id}`, x: x * TILE, y: y * TILE, width: TILE, height: TILE, ...extra }, parent);
}

/** A unit's status badge, or null: 'capture' (infantry or mech on a building it is
 *  capturing), 'load' (carrying cargo), 'hide' (a dived sub or hidden stealth), 'stun'. */
export const BADGE_LETTER = { capture: 'C', load: 'L', hide: 'H', stun: 'S' };
export function unitStatus(u, b) {
  if (b && b.capture < 20 && b.owner !== u.owner && (u.kind === 'Infantry' || u.kind === 'Mech')) return 'capture';
  if (u.cargo?.length) return 'load';
  if (u.dived) return 'hide';
  if (u.stunned) return 'stun';
  return null;
}

export class Board {
  /** art: optional art set (web/js/art.js), {unit(u, parent, box), tile(id, entry, owner, parent,
   *  box)}, each -> true when it drew; without one, or when it declines, libaw's own art is used. */
  /** opts: capCounts: a building's capture points left drawn on it (otherwise the capturing unit's
   *  C badge marks it, as AWBW does). art (an art set), hpBars: a damaged unit's HP as ten segments above it (a recording
   *  nobody can hover; easier to read than the corner number) instead of the number. */
  /** even: the same margin below the bottom row as the headroom above the top one, so the board
   *  sits centred in a frame instead of low. */
  constructor(svg, terrainMap, { art = null, hpBars = false, capCounts = true, even = false } = {}) {
    this.svg = svg;
    this.even = even;
    this.hpBars = hpBars;
    this.capCounts = capCounts;
    this.tmap = terrainMap;
    this.art = art;
    this.onTile = () => {};
    this.onHover = () => {};
    svg.addEventListener('click', (e) => { const t = this.tileOf(e); if (t) this.onTile(t[0], t[1], e); });
    svg.addEventListener('mousemove', (e) => {
      const t = this.tileOf(e);
      const k = t && key(t[0], t[1]);
      if (k !== this.hoverKey) { this.hoverKey = k; this.hoverTile = t; this.onHover(t); this.drawCursor(t); }
    });
    svg.addEventListener('mouseleave', () => { this.hoverKey = null; this.hoverTile = null; this.onHover(null); this.drawCursor(null); });
  }

  tileOf(e) {
    if (!this.state) return null;
    // Through the SVG's own transform, so a board letterboxed to fit the window (CSS max-height
    // with the viewBox's aspect kept) still maps clicks to the right tile.
    const m = this.svg.getScreenCTM();
    if (!m) return null;
    const q = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    const x = Math.floor(q.x / TILE), y = Math.floor(q.y / TILE);
    return x >= 0 && y >= 0 && x < this.state.width && y < this.state.height ? [x, y] : null;
  }

  /** order: player ids in turn order (colour index). marks: {move: Set, attack: Set, pick: Set}. */
  draw(state, { order = [], visible = null, marks = {}, selected = null, mirrorOdd = true } = {}) {
    this.state = state;
    const s = this.svg;
    s.replaceChildren();
    // Headroom above the top row, for sprites that stand taller than their tile and HP bars.
    this.headroom = this.headroomFor(state);
    const head = this.headroom;
    const foot = this.even ? head : 0;
    s.setAttribute('viewBox', `0 ${-head * TILE} ${state.width * TILE} ${(state.height + head + foot) * TILE}`);
    s.style.aspectRatio = `${state.width} / ${state.height + head + foot}`;
    const cls = (owner) => (owner == null ? 'pn' : `p${Math.max(0, order.indexOf(owner)) % 8}`);
    const terrain = el('g', { class: 'terrain' }, s);
    const name = (x, y) => this.tmap[terrainAt(state, x, y)] ?? { base: 'plain' };
    const isWaterish = (x, y) => x >= 0 && y >= 0 && x < state.width && y < state.height &&
      ['sea', 'reef', 'bridge-h', 'bridge-v'].includes(name(x, y).base);
    const joins = (x, y, kind) => {
      if (x < 0 || y < 0 || x >= state.width || y >= state.height) return false;
      const d = name(x, y);
      if (d.kind === kind) return true;
      if (kind === 'road' && d.base?.startsWith('bridge')) return true;
      if (kind === 'river' && d.base?.startsWith('bridge')) return false;
      return false;
    };
    const blds = buildingsByTile(state);
    for (let y = 0; y < state.height; y++) {
      for (let x = 0; x < state.width; x++) {
        const d = name(x, y);
        const b = blds.get(key(x, y));
        if (this.art?.tile?.(terrainAt(state, x, y), d, d.building ? (b?.owner ?? null) : null, terrain,
                            { x: x * TILE, y: y * TILE, size: TILE })) {
          if (this.capCounts && d.building && b && b.capture < 20) {
            const g = el('g', { class: 'bld' }, terrain);
            el('rect', { x: x * TILE + TILE - 17, y: y * TILE + 1, width: 16, height: 12, rx: 3, class: 'capbg' }, g);
            el('text', { x: x * TILE + TILE - 9, y: y * TILE + 10.5, class: 'cap' }, g).textContent = b.capture;
          }
          continue;
        }
        use(`t-${d.base ?? 'plain'}`, x, y, terrain);
        if (d.base === 'shoal') {
          for (const [side, dx, dy] of [['n', 0, -1], ['e', 1, 0], ['s', 0, 1], ['w', -1, 0]])
            if (isWaterish(x + dx, y + dy)) use(`t-shoal-${side}`, x, y, terrain);
        }
        if (d.kind) {
          use(`t-${d.kind}-c`, x, y, terrain);
          // Arms toward neighbours of the same kind (the id's own directions as a fallback).
          for (const [side, dx, dy] of [['n', 0, -1], ['e', 1, 0], ['s', 0, 1], ['w', -1, 0]])
            if (joins(x + dx, y + dy, d.kind) || (d.arms?.includes(side) && !this.anyNeighbourKind(state, x, y, d.kind)))
              use(`t-${d.kind}-${side}`, x, y, terrain);
        }
        const bname = d.building;
        if (bname) {
          const g = el('g', { class: `bld ${cls(b?.owner ?? null)}` }, terrain);
          use(`t-bld-${bname}`, x, y, g);
          if (this.capCounts && b && b.capture < 20) {
            el('rect', { x: x * TILE + TILE - 17, y: y * TILE + 1, width: 16, height: 12, rx: 3, class: 'capbg' }, g);
            el('text', { x: x * TILE + TILE - 9, y: y * TILE + 10.5, class: 'cap' }, g).textContent = b.capture;
          }
        }
      }
    }
    // Fog: shade what this player can't see.
    if (visible) {
      const fog = el('g', { class: 'fog' }, s);
      for (let y = 0; y < state.height; y++)
        for (let x = 0; x < state.width; x++) if (!visible.has(key(x, y))) use('o-fog', x, y, fog);
    }
    // Highlights.
    const hl = el('g', { class: 'marks' }, s);
    for (const [kind, set] of Object.entries(marks))
      for (const k of set ?? []) {
        const [x, y] = k.split(',').map(Number);
        el('rect', { x: x * TILE + 1, y: y * TILE + 1, width: TILE - 2, height: TILE - 2, rx: 6, class: `mark ${kind}` }, hl);
      }
    // Units.
    const units = el('g', { class: 'units' }, s);
    // HP bars stick out above the tile, so they go in a layer over every unit.
    const bars = [];
    for (const u of unitsByTile(state).values()) {
      const idx = order.indexOf(u.owner);
      const g = el('g', { class: `unit ${cls(u.owner)}${u.moved ? ' moved' : ''}${selected === u.id ? ' selected' : ''}`, 'data-id': u.id }, units);
      const flip = mirrorOdd && idx % 2 === 1;
      const id = `u-${u.kind.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')}`;
      const at = flip ? el('g', { transform: `translate(${(2 * u.x + 1) * TILE} 0) scale(-1 1)` }, g) : g;
      if (!this.art?.unit?.(u, at, { x: u.x * TILE, y: u.y * TILE, size: TILE, flip })) use(id, u.x, u.y, at);
      const hp = displayHp(u);
      if (hpUnknown(u) && this.hpBars) {
        // Hidden HP: an empty bar with a "?".
        bars.push((layer) => {
          const bg = el('g', { class: `hp-of ${cls(u.owner)}`, 'data-id': u.id }, layer);
          el('rect', { x: u.x * TILE + 1, y: u.y * TILE - 4, width: TILE - 2, height: 13, rx: 2, class: 'hpbar' }, bg);
          el('text', { x: u.x * TILE + TILE / 2, y: u.y * TILE + 6, class: 'hp hp-unknown' }, bg).textContent = '?';
        });
      } else if (hpUnknown(u)) {
        el('rect', { x: u.x * TILE + TILE - 16, y: u.y * TILE + TILE - 15, width: 15, height: 14, rx: 3, class: 'hpbg' }, g);
        el('text', { x: u.x * TILE + TILE - 8.5, y: u.y * TILE + TILE - 3.5, class: 'hp hp-unknown' }, g).textContent = '?';
      } else if (hp < 10 && this.hpBars) {
        // Ten segments across the top of the tile, coloured by how hurt the unit is.
        const tone = hp >= 7 ? 'hi' : hp >= 4 ? 'mid' : 'lo', sw = (TILE - 4) / 10;
        bars.push((layer) => {
          const bg = el('g', { class: `hp-of ${cls(u.owner)}`, 'data-id': u.id }, layer);
          el('rect', { x: u.x * TILE + 1, y: u.y * TILE - 4, width: TILE - 2, height: 13, rx: 2, class: 'hpbar' }, bg);
          for (let i = 0; i < 10; i++)
            el('rect', { x: u.x * TILE + 2 + i * sw + 0.5, y: u.y * TILE - 2, width: sw - 1, height: 9,
              class: i < hp ? `hpseg ${tone}` : 'hpseg off' }, bg);
        });
      } else if (hp < 10) {
        el('rect', { x: u.x * TILE + TILE - 16, y: u.y * TILE + TILE - 15, width: 15, height: 14, rx: 3, class: 'hpbg' }, g);
        el('text', { x: u.x * TILE + TILE - 8.5, y: u.y * TILE + TILE - 3.5, class: 'hp' }, g).textContent = hp;
      }
      // One status badge in the bottom-left corner, as AWBW shows it: capturing, loaded, hidden
      // (a dived sub or hidden stealth), stunned.
      const status = unitStatus(u, blds.get(key(u.x, u.y)));
      if (status) {
        const box = { x: u.x * TILE, y: u.y * TILE, size: TILE };
        if (!this.art?.badge?.(status, g, box)) {
          const bs = Math.round(TILE * 0.42), bx = box.x + 1, by = box.y + TILE - bs - 1;
          const b = el('g', { class: `ubadge ${status}` }, g);
          el('rect', { x: bx, y: by, width: bs, height: bs, rx: 2 }, b);
          el('text', { x: bx + bs / 2, y: by + bs * 0.78 }, b).textContent = BADGE_LETTER[status];
        }
      }
    }
    const hpLayer = el('g', { class: 'hpbars' }, s);
    for (const draw of bars) draw(hpLayer);
    this.cursorLayer = el('g', { class: 'cursor' }, s);
    if (this.hoverTile) this.drawCursor(this.hoverTile);   // a redraw (playback) keeps the cursor
  }

  /** Let the art learn how tall the top row's sprites are, in each of these states (a capture
   *  can change a building's sprite), so the first frame drawn already has its headroom. */
  measureArt(states) {
    if (!this.art?.measure) return Promise.resolve();
    const tiles = [];
    for (const state of states) {
      const blds = buildingsByTile(state);
      for (let x = 0; x < state.width; x++) {
        const id = terrainAt(state, x, 0), d = this.tmap[id] ?? {}, b = blds.get(key(x, 0));
        tiles.push([id, d, d.building ? (b?.owner ?? null) : null, { x: x * TILE, y: 0, size: TILE }]);
      }
    }
    return this.art.measure(tiles);
  }

  /** How far (in tiles) the top row's drawing reaches above the board, rounded up to a whole
   *  tile: a fraction would set every row off the pixel grid, and seams show between rows. */
  headroomFor(state) {
    let up = 0;
    const blds = buildingsByTile(state);
    for (let x = 0; x < state.width; x++) {
      const d = this.tmap[terrainAt(state, x, 0)] ?? {};
      const b = blds.get(key(x, 0));
      up = Math.max(up, this.art?.overhang?.(terrainAt(state, x, 0), d, d.building ? (b?.owner ?? null) : null, { x: x * TILE, y: 0, size: TILE }) ?? 0);
    }
    if (this.hpBars) for (const u of unitsByTile(state).values()) if (u.y === 0 && (displayHp(u) < 10 || hpUnknown(u))) up = Math.max(up, 4 / TILE);
    return Math.ceil(up);
  }

  anyNeighbourKind(state, x, y, kind) {
    return [[0, -1], [1, 0], [0, 1], [-1, 0]].some(([dx, dy]) => {
      const nx = x + dx, ny = y + dy;
      return nx >= 0 && ny >= 0 && nx < state.width && ny < state.height && this.tmap[terrainAt(state, nx, ny)]?.kind === kind;
    });
  }

  drawCursor(t) {
    if (!this.cursorLayer) return;
    this.cursorLayer.replaceChildren();
    if (t) use('o-cursor', t[0], t[1], this.cursorLayer);
  }
}
