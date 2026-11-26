// Plays a storyboard (docs/storyboard.md) on a Board: the whole turn, or the shots a caller
// picks. Time based: renderAt(t) draws the frame at t seconds, so the page's playback and a
// headless capture (frame by frame) show the same thing. Callers can add phases of their own
// (timeline `before`) and draw them (`overlays`).
import { Board, TILE } from './board.js';
import { statesOf, shots as allShots, timeline, segmentAt, alongPath } from './story.js';

const SVGNS = 'http://www.w3.org/2000/svg';
const ease = (p) => (p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2);

function el(name, attrs, parent) {
  const e = document.createElementNS(SVGNS, name);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  parent.appendChild(e);
  return e;
}

/** Units whose kill gets the mushroom cloud: bomber-sized hits only. Foot soldiers and vehicles
 *  don't, except the Mega Tank (player feedback, 2026-11-20). */
export const HEAVY = new Set(['Bomber', 'Black Bomb', 'Battleship', 'Mega Tank']);
/** Chunky kills: a bigger fireball with extra bursts, but no cloud (the Mech's bazooka, the heavy
 *  tanks, the big guns). */
export const CHUNKY = new Set(['Mech', 'Md.Tank', 'Neotank', 'Artillery', 'Rocket', 'Rockets', 'Piperunner', 'Missile', 'Missiles', 'Carrier', 'Cruiser', 'Stealth']);

/** Guns: bullets and flak, not shells. Their kills are a rattle of hits and flak puffs, with no
 *  fireball or smoke cloud (player feedback, 2026-11-20: the Anti-Air is bullets or flak). */
export const GUNS = new Set(['Infantry', 'Recon', 'Anti-Air']);

/** How big a kill by `kind` goes up: 'heavy', 'chunky', 'plain' or 'guns'. */
export function blastSize(kind) {
  return HEAVY.has(kind) ? 'heavy' : CHUNKY.has(kind) ? 'chunky' : GUNS.has(kind) ? 'guns' : 'plain';
}

export class Replay {
  /** opts: {data: {units, terrain, tmap}, art, shots: which to play (default all, in order),
   *  camera: zoom to each shot, aspect: the camera box's height / width (0.72; 1 for a square
   *  frame that a chat's square preview shows whole), hpBars: segmented HP bars (Board), capCounts: capture points drawn on buildings (Board), dur:
   *  timeline step lengths (story.timeline), before: timeline hook (story.timeline), overlays: {phase:
   *  ({layer, box, p, seg, st, before, after, replay}) => void} to draw a caller's phases (or
   *  replace a built-in one), onShot(shot, index)}. */
  constructor(svg, sb, { data, art = null, shots = null, camera = false, aspect = 0.72, hpBars = false, capCounts = true, dur = null, before = null, hold = null, overlays = {}, onShot = () => {} } = {}) {
    this.art = art;
    this.sb = sb;
    this.data = data;
    this.board = new Board(svg, data.tmap, { art, hpBars, capCounts });
    this.states = statesOf(sb);
    this.order = sb.start.players.map((p) => p.id);
    this.shots = shots ?? allShots(sb);
    this.tl = timeline(sb, this.shots, { before, dur, hold });
    this.duration = this.tl.duration;
    this.camera = camera;
    this.aspect = aspect;
    this.overlays = overlays;
    this.onShot = onShot;
    this.lastShot = null;
  }

  /** The camera box for a shot: about 11 tiles wide (by 11 * aspect) around its tiles, inside
   *  the board. A tall shot widens the box to keep its shape, while the board allows. */
  boxFor(shot) {
    const W = this.sb.start.width, H = this.sb.start.height;
    if (!this.camera || !shot?.focus) return [0, 0, W, H];
    const pts = shot.tiles.length ? shot.tiles : [shot.focus];
    const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
    const cx = (Math.min(...xs) + Math.max(...xs) + 1) / 2, cy = (Math.min(...ys) + Math.max(...ys) + 1) / 2;
    const tall = Math.max(...ys) - Math.min(...ys) + 4;
    const w = Math.min(W, Math.max(11, Math.max(...xs) - Math.min(...xs) + 5, tall / this.aspect));
    const h = Math.min(H, Math.max(w * this.aspect, tall));
    const x = Math.min(Math.max(0, cx - w / 2), W - w), y = Math.min(Math.max(0, cy - h / 2), H - h);
    return [x, y, w, h];
  }

  renderAt(t) {
    const at = segmentAt(this.tl, t);
    if (!at) { this.board.draw(this.sb.start, { order: this.order }); return; }
    const { seg, p } = at;
    const st = this.sb.steps[seg.step];
    const before = this.states[seg.step], after = this.states[seg.step + 1];
    let state = after, fx = null, under = null;
    const overlay = this.overlays[seg.phase];
    if (overlay) {
      state = before;
      fx = () => overlay({ layer: this.fxLayer, box: this.boxFor(seg.shot).map((v) => v * TILE), p, seg, st, before, after, replay: this });
    } else if (seg.phase === 'move') {
      state = JSON.parse(JSON.stringify(before));
      const u = state.units[st.unit_id];
      if (u) { [u.x, u.y] = alongPath(st.path, ease(p)); u.moved = false; }
    } else if (seg.phase === 'fire') {
      state = p < 0.45 ? before : after;
      fx = () => this.fireFx(st, before, p);
    } else if (seg.phase === 'capture') {
      // A finished capture crossfades the building to its new owner's colours (with art that draws
      // tiles); otherwise the board switches halfway through.
      const fade = st.done && this.art?.tile;
      state = fade || p < 0.5 ? before : after;
      fx = () => this.captureFx(st, p);
      if (fade) under = () => this.captureFade(st, after, p);
    } else if (seg.phase === 'power') {
      state = before;
      fx = () => this.powerFx(st, p);
    } else if (seg.phase === 'build') {
      fx = () => this.flash(st.unit.x, st.unit.y, 1 - p, 'fx-build');
    }
    // Units that have acted are greyed, as on AWBW; one moving keeps its colour until it stops.
    const shown = state;
    this.board.draw(shown, { order: this.order, mirrorOdd: true, marks: this.marks ?? {} });
    if (under) under();
    this.fxLayer = el('g', { class: 'fx' }, this.board.svg);
    this.shake = 0;
    if (fx) fx();
    this.setCamera(seg, p);
    if (seg.shot !== this.lastShot) { this.lastShot = seg.shot; this.onShot(seg.shot, this.shots.indexOf(seg.shot)); }
  }

  /** The phase playing at t ('move', 'fire', ...). */
  phaseAt(t) { return segmentAt(this.tl, t)?.seg.phase ?? null; }

  setCamera(seg) {
    const [x, y, w, h] = this.boxFor(seg.shot);
    // A blast shakes the picture (fireFx sets this.shake, in tiles); deterministic in t.
    const s = this.shake || 0, dx = s * Math.sin(seg.t0 * 97 + (this.shakeT ?? 0) * 71), dy = s * Math.cos(seg.t0 * 53 + (this.shakeT ?? 0) * 89);
    const up = y <= 0 ? (this.board.headroom ?? 0) : 0;   // the top row's tall sprites, as in Board.draw
    this.board.svg.setAttribute('viewBox', `${(x + dx) * TILE} ${(y + dy - up) * TILE} ${w * TILE} ${(h + up) * TILE}`);
    this.board.svg.style.aspectRatio = `${w} / ${h + up}`;
  }

  flash(x, y, a, cls = 'fx-hit') {
    el('circle', { cx: (x + 0.5) * TILE, cy: (y + 0.5) * TILE, r: TILE * (0.3 + 0.5 * (1 - a)), class: cls, opacity: Math.max(0, a) }, this.fxLayer);
  }

  pop(x, y, text, p, cls) {
    const rise = 18 * ease(Math.min(1, p));
    const tx = el('text', { x: (x + 0.5) * TILE, y: (y + 0.62) * TILE - rise, class: cls, opacity: p < 0.85 ? 1 : (1 - p) / 0.15 }, this.fxLayer);
    tx.textContent = text;
  }

  fireFx(st, before, p) {
    const a = st.attacker, d = st.defender;
    const guns = GUNS.has(before.units[a?.unit_id]?.kind);
    if (a && d && p < 0.45) {
      const [x0, y0] = a.at, [x1, y1] = d.at;
      // A shell, or for guns a burst of tracers one after another.
      for (let k = 0; k < (guns ? 4 : 1); k++) {
        const q = (p - k * 0.06) / 0.27;
        if (guns ? q < 0 || q > 1 : false) continue;
        const f = guns ? q : p / 0.45, x = x0 + (x1 - x0) * f, y = y0 + (y1 - y0) * f;
        if (!guns) { el('circle', { cx: (x + 0.5) * TILE, cy: (y + 0.5) * TILE, r: 4, class: 'fx-shot' }, this.fxLayer); continue; }
        const len = Math.hypot(x1 - x0, y1 - y0) || 1, bx = x - ((x1 - x0) / len) * 0.45, by = y - ((y1 - y0) / len) * 0.45;   // a dash about half a tile long
        el('line', { x1: (bx + 0.5) * TILE, y1: (by + 0.5) * TILE, x2: (x + 0.5) * TILE, y2: (y + 0.5) * TILE, class: 'fx-tracer' }, this.fxLayer);
      }
    }
    for (const side of [d, a]) {
      if (!side) continue;
      const u = before.units[side.unit_id];
      if (!u) continue;
      const q = side === d ? p : p - 0.25;   // the counterattack lands a beat later
      if (side.display_hp == null) {   // hidden HP (Sonja): a defender is hit, by an unknown amount
        if (side !== d) continue;    // an attacker may not have been countered at all
        if (q > 0.35 && q < 0.65) this.flash(side.at[0], side.at[1], 1 - (q - 0.35) / 0.3, 'fx-hit');
        if (q > 0.45) this.pop(side.at[0], side.at[1], '?', (q - 0.45) / 0.55, 'fx-dmg');
        continue;
      }
      const lost = Math.ceil(u.hp.hi / 10) - side.display_hp;
      if (lost <= 0) continue;
      const dead = side.display_hp <= 0;
      if (dead && q > 0.35) {
        const by = before.units[(side === d ? a : d)?.unit_id]?.kind;
        this.blast(side.at[0], side.at[1], (q - 0.35) / 0.65, blastSize(by));
      } else if (q > 0.35 && q < 0.65) this.flash(side.at[0], side.at[1], 1 - (q - 0.35) / 0.3, 'fx-hit');
      if (q > 0.45) this.pop(side.at[0], side.at[1], dead ? 'KO' : `−${lost}`, (q - 0.45) / 0.55, 'fx-dmg');
    }
  }

  /** A unit destroyed at (x, y), r from 0 to 1, by blastSize. Every size opens with a flash,
   *  flame tongues and spark streaks, then rolls into a lumpy dark smoke billow with glowing cracks
   *  that cool. 'chunky' is bigger, with two late side bursts; 'heavy' rises into a mushroom cloud
   *  whose cap billows the same way. Shakes the camera while it lands. (`true` still means heavy.) */
  blast(x, y, r, size = 'plain') {
    if (size === true) size = 'heavy'; else if (size === false) size = 'plain';
    if (size === 'guns') return this.flak(x, y, r);
    const heavy = size === 'heavy', chunky = size === 'chunky';
    const cx = (x + 0.5) * TILE, cy = (y + 0.5) * TILE, L = this.fxLayer, k = heavy ? 1.6 : chunky ? 1.25 : 1;
    const out = (a) => 1 - (1 - a) ** 3, clamp = (a) => Math.max(0, Math.min(1, a));
    this.shake = Math.max(this.shake || 0, (heavy ? 0.35 : chunky ? 0.25 : 0.18) * Math.max(0, 1 - r / 0.35));
    this.shakeT = r;
    if (r < 0.1) el('circle', { cx, cy, r: TILE * k * (0.4 + 3 * r), class: 'fx-flashw', opacity: 1 - r / 0.1 }, L);
    if (r < 0.6) el('circle', { cx, cy, r: TILE * k * (0.3 + 2.2 * out(r / 0.6)), class: 'fx-ring', opacity: 1 - r / 0.6 }, L);
    // Debris: fixed directions, thrown out and falling.
    for (let i = 0; i < 8; i++) {
      const ang = (i / 8) * Math.PI * 2 + 0.4, v = TILE * k * (0.9 + 0.35 * (i % 3)), t = Math.min(1, r / 0.7);
      if (t >= 1) continue;
      el('rect', { x: cx + Math.cos(ang) * v * t - 2.5, y: cy + Math.sin(ang) * v * t - 2.5 + TILE * 1.2 * t * t, width: 5, height: 5,
        class: 'fx-debris', opacity: 1 - t, transform: `rotate(${i * 40 + 700 * t} ${cx + Math.cos(ang) * v * t} ${cy + Math.sin(ang) * v * t + TILE * 1.2 * t * t})` }, L);
    }
    // Smoke billow (drawn first, so the fire sits in front of it): lumps that swell and drift up,
    // with ember cracks that glow, then cool. Heavy's billow is the ground skirt under the stem.
    const sm = clamp((r - 0.12) / 0.88), fadeS = r < 0.75 ? 1 : 1 - (r - 0.75) / 0.25;
    if (sm > 0) this.billow(cx, cy - TILE * (heavy ? -0.2 : 0.15 + 0.6 * sm), TILE * k * (0.35 + 0.35 * out(sm)) * (heavy ? 0.9 : 1),
      heavy ? 0.4 : 1, 1 - clamp(r / 0.6), fadeS * Math.min(1, sm * 4));
    // Flame tongues, pointing up and out, that shoot up and die back; a hot core under them.
    if (r < 0.45) {
      const n = heavy ? 9 : chunky ? 7 : 5, g = out(clamp(r / 0.15)) * (1 - clamp((r - 0.2) / 0.25));
      for (let i = 0; i < n; i++) {
        const a = -Math.PI / 2 + ((i / (n - 1)) - 0.5) * 2.6, len = TILE * k * (0.85 + 0.6 * ((i * 5) % 3) / 2) * g, w = TILE * k * 0.13;
        const tip = (s) => [cx + Math.cos(a) * len * s, cy + Math.sin(a) * len * s];
        const nx = -Math.sin(a) * w, ny = Math.cos(a) * w, [tx, ty] = tip(1), [ix, iy] = tip(0.6);
        el('path', { d: `M ${cx + nx} ${cy + ny} Q ${(cx + tx) / 2 + nx * 1.3} ${(cy + ty) / 2 + ny * 1.3} ${tx} ${ty} Q ${(cx + tx) / 2 - nx * 1.3} ${(cy + ty) / 2 - ny * 1.3} ${cx - nx} ${cy - ny} Z`, class: 'fx-fire' }, L);
        el('path', { d: `M ${cx + nx * 0.5} ${cy + ny * 0.5} Q ${(cx + ix) / 2 + nx * 0.6} ${(cy + iy) / 2 + ny * 0.6} ${ix} ${iy} Q ${(cx + ix) / 2 - nx * 0.6} ${(cy + iy) / 2 - ny * 0.6} ${cx - nx * 0.5} ${cy - ny * 0.5} Z`, class: 'fx-core' }, L);
      }
      el('circle', { cx, cy, r: TILE * k * (0.15 + 0.22 * g), class: 'fx-fire' }, L);
      el('circle', { cx, cy, r: TILE * k * (0.08 + 0.14 * g), class: 'fx-core' }, L);
    }
    // Spark streaks flying out.
    if (r < 0.4) for (let i = 0; i < 10; i++) {
      const a = (i / 10) * Math.PI * 2 + 0.9 * (i % 2), s = out(r / 0.4), d0 = TILE * k * (0.3 + 1.6 * s), d1 = d0 + TILE * k * 0.45 * (1 - s);
      el('line', { x1: cx + Math.cos(a) * d0, y1: cy + Math.sin(a) * d0, x2: cx + Math.cos(a) * d1, y2: cy + Math.sin(a) * d1, class: 'fx-spark', opacity: 1 - r / 0.4 }, L);
    }
    // Chunky: two smaller bursts go off a beat later, up-left and down-right of the first.
    if (chunky) for (const [dx, dy, t0] of [[-0.55, -0.35, 0.1], [0.5, 0.3, 0.2]]) {
      const s = (r - t0) / 0.35;
      if (s <= 0 || s >= 1) continue;
      const bx = cx + dx * TILE, by = cy + dy * TILE, b = out(s);
      el('circle', { cx: bx, cy: by, r: TILE * (0.15 + 0.4 * b), class: 'fx-fire', opacity: 1 - s }, L);
      el('circle', { cx: bx, cy: by, r: TILE * (0.08 + 0.2 * b), class: 'fx-core', opacity: 1 - s }, L);
    }
    if (!heavy) return;
    // Mushroom cloud: a stem and a billowing cap that rise and swell, fire cooling to smoke.
    const g = out(Math.min(1, r / 0.5)), cool = Math.min(1, r / 0.7), fade = r < 0.75 ? 1 : 1 - (r - 0.75) / 0.25;
    const top = cy - TILE * (0.4 + 1.7 * g), capW = TILE * (0.5 + 0.9 * g), capH = TILE * (0.3 + 0.45 * g);
    const grp = el('g', { opacity: fade }, L);
    el('path', { d: `M ${cx - TILE * 0.18} ${cy + TILE * 0.2} Q ${cx - TILE * 0.08} ${(cy + top) / 2} ${cx - TILE * 0.22} ${top + capH * 0.6}`
      + ` L ${cx + TILE * 0.22} ${top + capH * 0.6} Q ${cx + TILE * 0.08} ${(cy + top) / 2} ${cx + TILE * 0.18} ${cy + TILE * 0.2} Z`, class: 'fx-stem' }, grp);
    // The cap: hot orange lumps, the smoke billow fading in over them as it cools.
    this.billow(cx, top, capW, capH / capW, 0, 1, grp, 'fx-cloud');
    this.billow(cx, top, capW, capH / capW, 1 - cool, cool, grp);
  }

  /** A kill by guns at (x, y), r from 0 to 1: hit sparks rattle across the unit, then flak puffs
   *  pop around it one after another (a flash, then a small dark puff that drifts and fades). */
  flak(x, y, r) {
    const cx = (x + 0.5) * TILE, cy = (y + 0.5) * TILE, L = this.fxLayer;
    this.shake = Math.max(this.shake || 0, 0.1 * Math.max(0, 1 - r / 0.4));
    this.shakeT = r;
    const spots = [[0.1, -0.15], [-0.25, 0.1], [0.3, 0.2], [-0.05, -0.35], [0.2, -0.3], [-0.3, -0.2]];
    spots.forEach(([dx, dy], i) => {
      const s = (r - i * 0.07) / 0.45;
      if (s <= 0 || s >= 1) return;
      const px = cx + dx * TILE, py = cy + dy * TILE - TILE * 0.25 * s;
      if (s < 0.2) el('circle', { cx: px, cy: py, r: TILE * (0.12 + 0.6 * s), class: 'fx-core', opacity: 1 - s / 0.2 }, L);
      el('circle', { cx: px, cy: py, r: TILE * (0.13 + 0.2 * Math.sqrt(s)), class: 'fx-puff', opacity: (1 - s) * 0.9 }, L);
      // A few short sparks off each hit.
      if (s < 0.3) for (let j = 0; j < 3; j++) {
        const a = i * 1.7 + j * 2.1, d0 = TILE * 0.1, d1 = d0 + TILE * 0.28 * (1 - s / 0.3);
        el('line', { x1: px + Math.cos(a) * d0, y1: py + Math.sin(a) * d0, x2: px + Math.cos(a) * d1, y2: py + Math.sin(a) * d1, class: 'fx-spark', opacity: 1 - s / 0.3 }, L);
      }
    });
  }

  /** A lumpy smoke cloud centred at (cx, cy), `rx` wide and `squash` × as tall: a body plus a ring of
   *  lumps, with ember cracks at `glow` (0 to 1) brightness. Drawn into `parent` (the fx layer). */
  billow(cx, cy, rx, squash, glow, opacity, parent = this.fxLayer, cls = 'fx-billow') {
    if (opacity <= 0) return;
    const g = el('g', { opacity }, parent), ry = rx * squash;
    el('ellipse', { cx, cy, rx: rx * 0.85, ry: ry * 0.85, class: cls }, g);
    const n = 9, lumps = [];
    for (let i = 0; i < n; i++) {
      const a = (i / n) * Math.PI * 2, lr = Math.max(rx, ry) * (0.42 + 0.07 * ((i * 7) % 3)) * (i % 2 ? 0.8 : 1);
      const lx = cx + Math.cos(a) * rx * 0.7, ly = cy + Math.sin(a) * ry * 0.7;
      el('circle', { cx: lx, cy: ly, r: lr, class: cls }, g);
      lumps.push([lx, ly, lr]);
    }
    if (glow <= 0) return;
    // Embers: fire light caught in the folds, a glowing rim along the lower edge of every other lump.
    for (const [lx, ly, lr] of lumps.filter((_, i) => i % 2 === 0)) {
      const rr = lr * 0.72, a0 = 0.35, a1 = 2.4;
      el('path', { d: `M ${lx + Math.cos(a0) * rr} ${ly + Math.sin(a0) * rr} A ${rr} ${rr} 0 0 1 ${lx + Math.cos(a1) * rr} ${ly + Math.sin(a1) * rr}`,
        class: 'fx-crack', opacity: glow }, g);
    }
  }

  captureFx(st, p) {
    const [x, y] = st.at;
    if (p > 0.4) this.pop(x, y, st.done ? 'CAPTURED' : `${st.capture}`, (p - 0.4) / 0.6, st.done ? 'fx-cap done' : 'fx-cap');
  }

  /** The new owner's building fades in over the old one (in the terrain layer, so the unit
   *  capturing it stays on top), swelling a little and settling back: smoothstep for the fade,
   *  a damped sine for the swell. Not in AWBW itself; it just makes the change of hands read. */
  captureFade(st, after, p) {
    const [x, y] = st.at;
    const b = after.buildings.find(([bx, by]) => bx === x && by === y);
    const layer = this.board.svg.querySelector('g.terrain');
    const q = Math.min(1, Math.max(0, (p - 0.3) / 0.45));
    if (!b || !layer || q <= 0) return;
    const fade = q * q * (3 - 2 * q), swell = 1 + 0.16 * Math.sin(Math.PI * q) * (1 - q);
    const cx = (x + 0.5) * TILE, cy = (y + 1) * TILE;   // swell from the building's base
    const g = el('g', { opacity: fade, transform: `translate(${cx} ${cy}) scale(${swell}) translate(${-cx} ${-cy})` }, layer);
    this.art.tile(b[2].terrain, this.data.tmap[b[2].terrain], b[2].owner ?? null, g, { x: x * TILE, y: y * TILE, size: TILE });
  }

  powerFx(st, p) {
    const [vx, vy, vw, vh] = this.boxFor(null).map((v) => v * TILE);
    const a = p < 0.15 ? p / 0.15 : p > 0.85 ? (1 - p) / 0.15 : 1;
    el('rect', { x: vx, y: vy + vh * 0.38, width: vw, height: vh * 0.24, class: 'fx-power-bar', opacity: a * 0.9 }, this.fxLayer);
    const tx = el('text', { x: vx + vw / 2, y: vy + vh * 0.53, class: 'fx-power', opacity: a }, this.fxLayer);
    tx.textContent = `${st.co}: ${st.name}${st.super ? '!' : ''}`;
  }

  /** Real-time playback; returns a stop function. */
  play({ from = 0, speed = 1, loop = false, onEnd = () => {} } = {}) {
    let stopped = false;
    const t0 = performance.now() - (from / speed) * 1000;
    const tick = (now) => {
      if (stopped) return;
      let t = ((now - t0) / 1000) * speed;
      if (t >= this.duration) {
        if (loop) t %= this.duration;
        else { this.renderAt(this.duration); onEnd(); return; }
      }
      this.renderAt(t);
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    return () => { stopped = true; };
  }
}
