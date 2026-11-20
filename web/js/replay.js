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

/** Units whose kill gets the mushroom cloud: the big guns and the bombs. */
export const HEAVY = new Set(['Bomber', 'Rocket', 'Rockets', 'Artillery', 'Battleship', 'Piperunner', 'Missile', 'Missiles', 'Black Bomb', 'Carrier', 'Mega Tank']);

export class Replay {
  /** opts: {data: {units, terrain, tmap}, art, shots: which to play (default all, in order),
   *  camera: zoom to each shot, aspect: the camera box's height / width (0.72; 1 for a square
   *  frame that a chat's square preview shows whole), hpBars: segmented HP bars (Board), dur:
   *  timeline step lengths (story.timeline), before: timeline hook (story.timeline), overlays: {phase:
   *  ({layer, box, p, seg, st, before, after, replay}) => void} to draw a caller's phases (or
   *  replace a built-in one), onShot(shot, index)}. */
  constructor(svg, sb, { data, art = null, shots = null, camera = false, aspect = 0.72, hpBars = false, dur = null, before = null, overlays = {}, onShot = () => {} } = {}) {
    this.art = art;
    this.sb = sb;
    this.data = data;
    this.board = new Board(svg, data.tmap, { art, hpBars });
    this.states = statesOf(sb);
    this.order = sb.start.players.map((p) => p.id);
    this.shots = shots ?? allShots(sb);
    this.tl = timeline(sb, this.shots, { before, dur });
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
    // Units that acted keep their colour: a replay greyed out as it goes is hard to follow.
    const shown = { ...state, units: Object.fromEntries(Object.entries(state.units).map(([k, u]) => [k, { ...u, moved: false }])) };
    this.board.draw(shown, { order: this.order, mirrorOdd: true });
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
    this.board.svg.setAttribute('viewBox', `${(x + dx) * TILE} ${(y + dy) * TILE} ${w * TILE} ${h * TILE}`);
    this.board.svg.style.aspectRatio = `${w} / ${h}`;
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
    if (a && d && p < 0.45) {
      const q = p / 0.45;
      const [x0, y0] = a.at, [x1, y1] = d.at;
      const x = x0 + (x1 - x0) * q, y = y0 + (y1 - y0) * q;
      el('circle', { cx: (x + 0.5) * TILE, cy: (y + 0.5) * TILE, r: 4, class: 'fx-shot' }, this.fxLayer);
    }
    for (const side of [d, a]) {
      if (!side) continue;
      const u = before.units[side.unit_id];
      if (!u) continue;
      const lost = Math.ceil(u.hp.hi / 10) - side.display_hp;
      if (lost <= 0) continue;
      const q = side === d ? p : p - 0.25;   // the counterattack lands a beat later
      const dead = side.display_hp <= 0;
      if (dead && q > 0.35) {
        const by = before.units[(side === d ? a : d)?.unit_id]?.kind;
        this.blast(side.at[0], side.at[1], (q - 0.35) / 0.65, HEAVY.has(by));
      } else if (q > 0.35 && q < 0.65) this.flash(side.at[0], side.at[1], 1 - (q - 0.35) / 0.3, 'fx-hit');
      if (q > 0.45) this.pop(side.at[0], side.at[1], dead ? 'KO' : `−${lost}`, (q - 0.45) / 0.55, 'fx-dmg');
    }
  }

  /** A unit destroyed at (x, y), r from 0 to 1: a fireball with debris and smoke, or for a heavy
   *  hitter (HEAVY) a mushroom cloud with a shockwave. Shakes the camera while it lands. */
  blast(x, y, r, heavy) {
    const cx = (x + 0.5) * TILE, cy = (y + 0.5) * TILE, L = this.fxLayer, k = heavy ? 1.6 : 1;
    const out = (a) => 1 - (1 - a) ** 3;
    this.shake = Math.max(this.shake || 0, (heavy ? 0.35 : 0.18) * Math.max(0, 1 - r / 0.35));
    this.shakeT = r;
    if (r < 0.12) el('circle', { cx, cy, r: TILE * k * (0.4 + 3 * r), class: 'fx-flashw', opacity: 1 - r / 0.12 }, L);
    // Shockwave ring.
    if (r < 0.6) el('circle', { cx, cy, r: TILE * k * (0.3 + 2.2 * out(r / 0.6)), class: 'fx-ring', opacity: 1 - r / 0.6 }, L);
    // Debris: fixed directions, thrown out and falling.
    for (let i = 0; i < 8; i++) {
      const ang = (i / 8) * Math.PI * 2 + 0.4, v = TILE * k * (0.9 + 0.35 * (i % 3)), t = Math.min(1, r / 0.7);
      if (t >= 1) continue;
      el('rect', { x: cx + Math.cos(ang) * v * t - 2.5, y: cy + Math.sin(ang) * v * t - 2.5 + TILE * 1.2 * t * t, width: 5, height: 5,
        class: 'fx-debris', opacity: 1 - t, transform: `rotate(${i * 40 + 700 * t} ${cx + Math.cos(ang) * v * t} ${cy + Math.sin(ang) * v * t + TILE * 1.2 * t * t})` }, L);
    }
    if (!heavy) {
      // Fireball, cooling to a smoke puff that drifts up.
      const f = out(Math.min(1, r / 0.35));
      if (r < 0.55) {
        el('circle', { cx, cy, r: TILE * (0.25 + 0.55 * f), class: 'fx-fire', opacity: 1 - Math.max(0, r - 0.3) / 0.25 }, L);
        el('circle', { cx, cy, r: TILE * (0.12 + 0.3 * f), class: 'fx-core', opacity: 1 - r / 0.55 }, L);
      }
      if (r > 0.25) for (let i = 0; i < 3; i++) {
        const s = (r - 0.25) / 0.75;
        el('circle', { cx: cx + (i - 1) * TILE * 0.25, cy: cy - TILE * (0.2 + 0.9 * s) - i * 4, r: TILE * (0.25 + 0.3 * s), class: 'fx-smoke', opacity: 0.85 * (1 - s) }, L);
      }
      return;
    }
    // Mushroom cloud: a stem and a cap that rise and swell, fire cooling to smoke.
    const g = out(Math.min(1, r / 0.5)), cool = Math.min(1, r / 0.7), fade = r < 0.75 ? 1 : 1 - (r - 0.75) / 0.25;
    const top = cy - TILE * (0.4 + 1.7 * g), capW = TILE * (0.5 + 0.9 * g), capH = TILE * (0.3 + 0.45 * g);
    const grp = el('g', { opacity: fade }, L);
    el('ellipse', { cx, cy: cy + TILE * 0.25, rx: TILE * (0.5 + 1.1 * g), ry: TILE * (0.18 + 0.25 * g), class: 'fx-smoke' }, grp);
    el('path', { d: `M ${cx - TILE * 0.18} ${cy + TILE * 0.2} Q ${cx - TILE * 0.08} ${(cy + top) / 2} ${cx - TILE * 0.22} ${top + capH * 0.6}`
      + ` L ${cx + TILE * 0.22} ${top + capH * 0.6} Q ${cx + TILE * 0.08} ${(cy + top) / 2} ${cx + TILE * 0.18} ${cy + TILE * 0.2} Z`, class: 'fx-stem' }, grp);
    el('ellipse', { cx, cy: top + capH * 0.55, rx: capW * 1.05, ry: capH * 0.45, class: 'fx-smoke' }, grp);
    el('ellipse', { cx, cy: top, rx: capW, ry: capH, class: 'fx-cloud' }, grp);
    el('ellipse', { cx, cy: top, rx: capW, ry: capH, class: 'fx-cloud-cool', opacity: cool }, grp);
    el('ellipse', { cx, cy: top - capH * 0.1, rx: capW * 0.55, ry: capH * 0.5, class: 'fx-core', opacity: 1 - cool }, grp);
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
