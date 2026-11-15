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

export class Replay {
  /** opts: {data: {units, terrain, tmap}, art, shots: which to play (default all, in order),
   *  camera: zoom to each shot, before: timeline hook (story.timeline), overlays: {phase:
   *  ({layer, box, p, seg, st, before, after, replay}) => void} to draw a caller's phases (or
   *  replace a built-in one), onShot(shot, index)}. */
  constructor(svg, sb, { data, art = null, shots = null, camera = false, before = null, overlays = {}, onShot = () => {} } = {}) {
    this.art = art;
    this.sb = sb;
    this.data = data;
    this.board = new Board(svg, data.tmap, { art });
    this.states = statesOf(sb);
    this.order = sb.start.players.map((p) => p.id);
    this.shots = shots ?? allShots(sb);
    this.tl = timeline(sb, this.shots, { before });
    this.duration = this.tl.duration;
    this.camera = camera;
    this.overlays = overlays;
    this.onShot = onShot;
    this.lastShot = null;
  }

  /** The camera box for a shot: about 11 x 8 tiles around its tiles, inside the board. */
  boxFor(shot) {
    const W = this.sb.start.width, H = this.sb.start.height;
    if (!this.camera || !shot?.focus) return [0, 0, W, H];
    const pts = shot.tiles.length ? shot.tiles : [shot.focus];
    const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
    const cx = (Math.min(...xs) + Math.max(...xs) + 1) / 2, cy = (Math.min(...ys) + Math.max(...ys) + 1) / 2;
    const w = Math.min(W, Math.max(11, Math.max(...xs) - Math.min(...xs) + 5));
    const h = Math.min(H, Math.max(w * 0.72, Math.max(...ys) - Math.min(...ys) + 4));
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
    if (fx) fx();
    this.setCamera(seg, p);
    if (seg.shot !== this.lastShot) { this.lastShot = seg.shot; this.onShot(seg.shot, this.shots.indexOf(seg.shot)); }
  }

  /** The phase playing at t ('move', 'fire', ...). */
  phaseAt(t) { return segmentAt(this.tl, t)?.seg.phase ?? null; }

  setCamera(seg) {
    const [x, y, w, h] = this.boxFor(seg.shot);
    this.board.svg.setAttribute('viewBox', `${x * TILE} ${y * TILE} ${w * TILE} ${h * TILE}`);
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
      if (q > 0.35 && q < 0.65) this.flash(side.at[0], side.at[1], 1 - (q - 0.35) / 0.3, side.display_hp <= 0 ? 'fx-boom' : 'fx-hit');
      if (q > 0.45) this.pop(side.at[0], side.at[1], side.display_hp <= 0 ? 'KO' : `−${lost}`, (q - 0.45) / 0.55, 'fx-dmg');
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
