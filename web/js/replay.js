// Plays a storyboard (docs/storyboard.md) on a Board: the whole turn, or the shots a caller
// picks. Time based: renderAt(t) draws the frame at t seconds, so the page's playback and a
// headless capture (frame by frame) show the same thing. Callers can add phases of their own
// (timeline `before`) and draw them (`overlays`).
import { Board, TILE } from './board.js';
import { statesOf, shots as allShots, timeline, segmentAt, alongPath, remapTime } from './story.js';
import { GT, ghostPlan, ghostCursor, ghostRest, unitMenu, diamond } from './ghost.js';

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
/** The ghost's timings, in seconds (ghost.js). */
export const GHOST = GT;

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
  /** ghost: act out each step first, as the player's cursor and menus would (after AWBW Ghost
   *  Input by onetrickwolf; ghost.js plans it): the cursor glides to the unit, its range and the
   *  path arrow, the move, the menu at the destination with the choice pressed, then an attack's
   *  target with the damage box, an unload's cargo list and landing tiles, a build's menu, and so
   *  on. ghostEngine(wasm module) fills in what needs libaw's engine (ranges, menus, damage); without
   *  it the ghost still plays, showing less. ghostPace scales the ghost's segments (0: none), for a
   *  page that's behind; setGhostPace changes it while playing. */
  constructor(svg, sb, { data, art = null, shots = null, camera = false, aspect = 0.72, hpBars = false, capCounts = true, dur = null, before = null, hold = null, overlays = {}, onShot = () => {}, ghost = false, ghostPace = 1, even = false } = {}) {
    this.art = art;
    this.sb = sb;
    this.data = data;
    this.board = new Board(svg, data.tmap, { art, hpBars, capCounts, even });
    this.states = statesOf(sb);
    this.order = sb.start.players.map((p) => p.id);
    this.shots = shots ?? allShots(sb);
    // What the engine works out for the ghost, by step: ranges (Sets of "x,y"), the unit menu's
    // options, the damage box, landing tiles, the build menu.
    this.ghost = ghost ? { move: {}, attack: {}, menu: {}, damage: {}, drop: {}, build: {} } : null;
    this.timing = { before, dur, hold };
    this.ghostPace = ghostPace;
    this.layout();
    this.camera = camera;
    this.aspect = aspect;
    this.overlays = overlays;
    this.onShot = onShot;
    this.lastShot = null;
  }

  /** Lays out the timeline: the caller's phases and the ghost's (at this.ghostPace). */
  layout() {
    const { before, dur, hold } = this.timing;
    const plan = this.ghost ? ghostPlan(this.sb, this.states, { pace: this.ghostPace, menus: this.ghost.menu }) : { pre: {}, post: {}, asks: {} };
    this.plan = plan;
    const pre = (st, i, sh) => [...(plan.pre[i] ?? []), ...((before && before(st, i, sh)) ?? [])];
    const post = (st, i) => (plan.post[i] ?? []).map((x) => ({ ...x, post: true }));
    this.tl = timeline(this.sb, this.shots, { before: pre, after: post, dur, hold });
    this.duration = this.tl.duration;
  }

  /** Change how fast the ghost acts (1: as planned, 0.5: twice as fast, 0: not at all) while
   *  playing: lays out the timeline again and returns the time in it that shows what t showed
   *  (story.remapTime), so a page that keeps its own clock carries on from there. */
  setGhostPace(pace, t = 0) {
    if (!this.ghost || pace === this.ghostPace) return t;
    const old = this.tl;
    this.ghostPace = pace;
    this.layout();
    return remapTime(old, this.tl, t);
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
    let state = after, fx = null, under = null, marks = this.marks ?? {}, arrow = null;
    const overlay = this.overlays[seg.phase];
    if (seg.ghost) {
      ({ state, marks, arrow, fx } = this.ghostFrame(seg, p, st, before, after, marks));
    } else if (overlay) {
      state = before;
      fx = () => overlay({ layer: this.fxLayer, box: this.boxFor(seg.shot).map((v) => v * TILE), p, seg, st, before, after, replay: this });
    } else if (seg.phase === 'move') {
      state = JSON.parse(JSON.stringify(before));
      const u = state.units[st.unit_id];
      if (u) { [u.x, u.y] = alongPath(st.path, ease(p)); u.moved = false; }
      if (this.ghost) arrow = [st.path, ease(p), 1];
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
    } else if (seg.phase === 'unload') {
      // The cargo rolls out of the transport onto its tile.
      const t = before.units[st.transport_id];
      state = JSON.parse(JSON.stringify(after));
      const u = state.units[st.unit_id];
      if (u && t) { const f = ease(p); u.x = t.x + (st.to[0] - t.x) * f; u.y = t.y + (st.to[1] - t.y) * f; u.moved = p >= 1; }
    } else if (seg.phase === 'explode' || seg.phase === 'launch') {
      state = p < 0.35 ? before : after;
      fx = () => this.areaFx(st, before, p);
    } else if (seg.phase === 'repair') {
      const u = after.units[st.target_id];
      if (u) fx = () => { this.flash(u.x, u.y, 1 - p, 'fx-build'); if (p > 0.2) this.pop(u.x, u.y, '+', (p - 0.2) / 0.8, 'fx-cap'); };
    }
    // Units that have acted are greyed, as on AWBW; one moving keeps its colour until it stops.
    const shown = state;
    this.board.draw(shown, { order: this.order, mirrorOdd: true, marks });
    if (arrow) this.arrow(...arrow);
    if (under) under();
    this.fxLayer = el('g', { class: 'fx' }, this.board.svg);
    this.popLayer = el('g', { class: 'fx-pops' }, this.board.svg);   // captures, repairs: over everything
    this.shake = 0;
    if (fx) fx();
    if (this.ghost && !this.idle && t < this.duration + GT.idle) { const c = this.cursorAt(t); if (c) this.cursor(c.at, c.p, c.press); }
    this.board.svg.appendChild(this.popLayer);   // after the ghost's cursor and menus, which sit on the same tile
    this.setCamera(seg, p);
    if (seg.shot !== this.lastShot) { this.lastShot = seg.shot; this.onShot(seg.shot, this.shots.indexOf(seg.shot)); }
  }

  /** The move arrow along path from fraction a to b of its length, under the units: drawn in the
   *  style of the console games' (and AWBW's): a thick bright-red path with a round tail and a
   *  wide head, its shadow down and to the right. */
  arrow(path, a, b) {
    const layer = this.board.svg.querySelector('g.marks');
    if (!layer || b <= a) return;
    const n = path.length - 1, pts = [];
    pts.push(alongPath(path, a));
    for (let i = 1; i < n; i++) if (i / n > a && i / n < b) pts.push(path[i]);
    pts.push(alongPath(path, b));
    const xy = pts.map(([x, y]) => [(x + 0.5) * TILE, (y + 0.5) * TILE]);
    const head = b >= 1 && xy.length > 1;
    let ang = 0;
    if (head) {   // the line stops at the head's base, which sits on the last tile
      const [x1, y1] = xy[xy.length - 1], [x0, y0] = xy[xy.length - 2];
      ang = Math.atan2(y1 - y0, x1 - x0);
      const back = Math.min(TILE * 0.05, Math.hypot(x1 - x0, y1 - y0));
      xy[xy.length - 1] = [x1 - Math.cos(ang) * back, y1 - Math.sin(ang) * back];
    }
    const d = 'M' + xy.map((p) => p.join(' ')).join(' L');
    const w = TILE * 0.36, sh = TILE * 0.09;
    const tip = head ? (() => {
      const [x, y] = pts[pts.length - 1].map((v) => (v + 0.5) * TILE), deg = (ang * 180) / Math.PI;
      const L = TILE * 0.42, B = TILE * 0.06, W = TILE * 0.42;   // tip ahead of the centre, base just behind
      return { d: `M ${L} 0 L ${-B} ${-W} L ${-B} ${W} Z`, t: `translate(${x} ${y}) rotate(${deg})` };
    })() : null;
    const g = el('g', { class: 'fx-arrow' }, layer);
    for (const [dx, colour] of [[sh, '#5a1e0c'], [0, '#ff2c14']]) {
      const part = el('g', { transform: `translate(${dx} ${dx})` }, g);
      el('path', { d, fill: 'none', stroke: colour, 'stroke-width': w, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, part);
      if (tip) el('path', { d: tip.d, fill: colour, transform: tip.t }, part);
    }
  }

  /** Where the ghost's cursor is at t: one cursor for the whole turn, as a player's. It glides
   *  (in a straight line, like a mouse) from wherever it last was to each step's unit, traces a
   *  move's path with the arrow, rests on the destination while the menu is up, glides on to an
   *  attack's target (or a landing tile, ...), and waits where it stopped until the next step.
   *  {at: [x, y], p: 1, press}, or null before the first step (or with no ghost segments). */
  cursorAt(t) {
    let pos = null;
    for (const seg of this.tl.segs) {
      if (seg.t0 > t) break;
      if (!seg.ghost) continue;
      const st = this.sb.steps[seg.step], dur = seg.t1 - seg.t0, p = dur > 0 ? Math.min(1, (t - seg.t0) / dur) : 1;
      if (t < seg.t1) { const c = ghostCursor(seg, p, st, alongPath); return { at: c.at ?? pos, p: 1, press: c.press }; }
      pos = ghostRest(seg, st) ?? pos;
    }
    return pos ? { at: pos, p: 1, press: false } : null;
  }

  /** A target cursor (four corner brackets) closing in on tile [x, y]; pressed, they close tight. */
  cursor(at, p, press = false) {
    if (!at) return;
    const [x, y] = at, s = (press ? 1 : 4) + 6 * (1 - ease(Math.min(1, p / 0.6))), L = 11;
    const g = el('g', { class: 'fx-cursor', fill: 'none', stroke: '#fff', 'stroke-width': 4, 'stroke-linecap': 'square' }, this.fxLayer);
    const x0 = x * TILE - s + 3, y0 = y * TILE - s + 3, x1 = (x + 1) * TILE + s - 3, y1 = (y + 1) * TILE + s - 3;
    for (const [cx, cy, dx, dy] of [[x0, y0, 1, 1], [x1, y0, -1, 1], [x0, y1, 1, -1], [x1, y1, -1, -1]])
      el('path', { d: `M ${cx} ${cy + dy * L} L ${cx} ${cy} L ${cx + dx * L} ${cy}`, style: 'filter: drop-shadow(0 0 2px #000)' }, g);
  }

  /** A ghost segment's frame: which board, the tiles marked, the arrow, and its menu or damage
   *  box. A unit acting keeps its colour until it's done (AWBW greys it after). A unit held down
   *  on the page (this.marks) keeps its ranges over the ghost's. */
  ghostFrame(seg, p, st, before, after, marks) {
    const g = this.ghost, i = seg.step, W = this.sb.start.width, H = this.sb.start.height;
    const keys = (ts) => new Set((ts ?? []).map(([x, y]) => `${x},${y}`));
    let state = seg.post ? after : before, arrow = null, m = {};
    if (seg.phase === 'aim') {
      m = { move: g.move[i] };
      arrow = [st.path, 0, ease(Math.min(1, Math.max(0, p - 0.15) / 0.6))];
    } else if (seg.phase === 'select') m = seg.range ? { move: g.move[i] } : {};
    else if (seg.phase === 'target') m = { attack: g.attack[i] ?? keys(st.defender ? [st.defender.at] : []) };
    else if (seg.phase === 'drop') m = { drop: g.drop[i] ?? keys(seg.tiles) };
    else if (seg.phase === 'repair') m = { repair: keys(seg.tiles) };
    else if (seg.phase === 'blast') m = { blast: new Set(seg.area) };
    else if (seg.phase === 'silo') m = { silo: diamond(ghostCursor(seg, p, st, alongPath).at.map(Math.round), seg.radius, W, H) };
    const lit = seg.post ? seg.menu?.unit : seg.lit;
    if (lit != null && state.units[lit]?.moved) { state = JSON.parse(JSON.stringify(state)); state.units[lit].moved = false; }
    const fx = () => { if (seg.menu) this.menu(seg, p); if (seg.phase === 'target') this.damageBox(seg, p); };
    return { state, marks: this.marks?.move || this.marks?.attack ? marks : m, arrow, fx };
  }

  /** A ghost menu beside its tile, kept inside the camera box: its options, the chosen one
   *  hovered, then pressed for the segment's last `press` seconds. A unit menu lists what the
   *  engine offered there when ghostEngine ran (else the plan's guess), a build menu every unit the
   *  building makes with its price (greyed when the player couldn't afford it), the cargo list the
   *  transport's load. libaw's own drawing, not AWBW's. */
  menu(seg, p) {
    const m = seg.menu, g = this.ghost, [tx, ty] = seg.at;
    let items;
    if (m.kind === 'build') items = (g.build[seg.step] ?? [{ unit: m.chosen }]).map((b) => ({ label: b.unit, kind: b.unit, cost: b.cost, off: b.off }));
    else if (m.kind === 'cargo') items = m.options.map((k, j) => ({ label: k, kind: k, id: m.ids?.[j] }));
    else items = ((!m.lone && g.menu[seg.step]) || m.options).map((label) => ({ label }));
    const ci = Math.max(0, items.findIndex((it) => (m.chosenId != null ? it.id === m.chosenId : it.label === m.chosen)));
    const MAX = 7, from = Math.max(0, Math.min(ci - 3, items.length - MAX)), shown = items.slice(from, from + MAX);
    const RH = 24, PAD = 4, icon = shown.some((it) => it.kind), cost = shown.some((it) => it.cost != null);
    const longest = Math.max(...shown.map((it) => it.label.length));
    const w = Math.round(PAD * 2 + 16 + (icon ? 24 : 0) + longest * 7.4 + (cost ? 50 : 0));
    const more = [from > 0, from + MAX < items.length];
    const h = PAD * 2 + shown.length * RH + (more[0] ? 8 : 0) + (more[1] ? 8 : 0);
    const [bx, by, bw, bh] = this.boxFor(seg.shot).map((v) => v * TILE);
    let x = (tx + 1) * TILE + 4;
    if (x + w > bx + bw - 2) x = tx * TILE - 4 - w;
    x = Math.max(bx + 2, Math.min(x, bx + bw - w - 2));
    const y = Math.max(by + 2, Math.min(ty * TILE - 2, by + bh - h - 2));
    const pressed = p > 1 - (seg.press ?? 0) / Math.max(1e-6, seg.dur);
    const G = el('g', { class: 'fx-menu', style: 'filter: drop-shadow(0 2px 4px rgba(0,0,0,.55))' }, this.fxLayer);
    el('rect', { x, y, width: w, height: h, rx: 7, fill: '#151b27', 'fill-opacity': 0.95, stroke: '#3a4458', 'stroke-width': 1 }, G);
    const arrowAt = (yy, up) => el('path', { d: up ? `M ${x + w / 2 - 5} ${yy + 6} L ${x + w / 2} ${yy + 1} L ${x + w / 2 + 5} ${yy + 6} Z`
      : `M ${x + w / 2 - 5} ${yy + 1} L ${x + w / 2} ${yy + 6} L ${x + w / 2 + 5} ${yy + 1} Z`, fill: '#8a93a8' }, G);
    if (more[0]) arrowAt(y + 2, true);
    let ry = y + PAD + (more[0] ? 8 : 0);
    shown.forEach((it, j) => {
      const on = from + j === ci, dy = on && pressed ? 1 : 0;
      if (on) el('rect', { x: x + 3, y: ry + 1, width: w - 6, height: RH - 2, rx: 5, fill: pressed ? '#b5821c' : '#f2b53a' }, G);
      const row = el('g', { opacity: it.off && !on ? 0.42 : 1, transform: `translate(0 ${dy})` }, G);
      let lx = x + PAD + 8;
      if (it.kind) { this.menuIcon(it.kind, m.owner ?? this.sb.player, lx - 2, ry + 2, RH - 4, row); lx += 24; }
      const tx_ = el('text', { x: lx, y: ry + RH / 2 + 4.5, fill: on ? '#1d1606' : '#eef1f7', style: 'font: 700 13px system-ui, sans-serif' }, row);
      tx_.textContent = it.label;
      if (it.cost != null) {
        const c = el('text', { x: x + w - PAD - 8, y: ry + RH / 2 + 4.5, fill: on ? '#3b2c00' : '#aab2c3', 'text-anchor': 'end', style: 'font: 600 12px system-ui, sans-serif; font-variant-numeric: tabular-nums' }, row);
        c.textContent = it.cost.toLocaleString('en-US');
      }
      ry += RH;
    });
    if (more[1]) arrowAt(ry, false);
  }

  /** A unit sprite in a menu row: the art set's (AWBW's, hotlinked) or libaw's own. */
  menuIcon(kind, owner, x, y, size, parent) {
    const u = { id: -1, kind, owner, x: x / TILE, y: y / TILE, hp: { lo: 100, hi: 100 }, moved: false, cargo: [] };
    const idx = Math.max(0, this.order.indexOf(owner)) % 8;
    const g = el('g', { class: `unit p${idx}` }, parent);
    const box = { x, y, size, flip: false };
    if (this.art?.unit?.(u, g, box)) return;
    el('use', { href: `#u-${kind.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')}`, x, y, width: size, height: size }, g);
  }

  /** The damage box over an attack's target once the cursor is on it: the damage the shot could
   *  do (lowest to highest luck), and the counterattack's, as libaw's engine works them out. */
  damageBox(seg, p) {
    const d = this.ghost.damage[seg.step];
    if (!d || !seg.to || p < (seg.glide ?? 0) / Math.max(1e-6, seg.dur)) return;
    const [tx, ty] = seg.to, [bx, by, bw, bh] = this.boxFor(seg.shot).map((v) => v * TILE);
    const range = ([a, b]) => (a === b ? `${a}%` : `${a}–${b}%`);
    const counter = d.counter && d.counter[1] > 0;
    const w = 104, h = counter ? 50 : 34;
    let y = ty * TILE - h - 8;
    if (y < by + 2) y = (ty + 1) * TILE + 8;
    y = Math.min(y, by + bh - h - 2);
    const x = Math.max(bx + 2, Math.min((tx + 0.5) * TILE - w / 2, bx + bw - w - 2));
    const G = el('g', { class: 'fx-damage', style: 'filter: drop-shadow(0 2px 4px rgba(0,0,0,.55))' }, this.fxLayer);
    el('rect', { x, y, width: w, height: h, rx: 7, fill: '#151b27', 'fill-opacity': 0.95, stroke: '#3a4458', 'stroke-width': 1 }, G);
    const line = (yy, label, value, colour) => {
      el('text', { x: x + 9, y: yy, fill: '#aab2c3', style: 'font: 600 11px system-ui, sans-serif' }, G).textContent = label;
      el('text', { x: x + w - 9, y: yy, fill: colour, 'text-anchor': 'end', style: 'font: 800 14px system-ui, sans-serif; font-variant-numeric: tabular-nums' }, G).textContent = value;
    };
    line(y + 22, 'Damage', range(d.dmg), '#ffd25e');
    if (counter) line(y + 40, 'Counter', range(d.counter), '#ff8a6a');
  }

  /** Black Bomb and missile silo: a blast over the area, every unit in it hit (never killed). */
  areaFx(st, before, p) {
    const at = st.type === 'explode' ? st.at : st.target, r = st.type === 'explode' ? 3 : 2;
    if (p < 0.7) this.blast(at[0], at[1], p / 0.7, st.type === 'explode' ? 'heavy' : 'chunky');
    if (p < 0.6) for (const k of diamond(at, r, this.sb.start.width, this.sb.start.height)) {
      const [x, y] = k.split(',').map(Number);
      el('rect', { x: x * TILE + 2, y: y * TILE + 2, width: TILE - 4, height: TILE - 4, rx: 6, class: 'fx-area', opacity: 0.55 * (1 - p / 0.6) }, this.fxLayer);
    }
    if (p > 0.4 && st.damage)
      for (const u of Object.values(before.units))
        if (u.carried_by == null && Math.abs(u.x - at[0]) + Math.abs(u.y - at[1]) <= r && u.id !== st.unit_id) this.pop(u.x, u.y, `−${st.damage}`, (p - 0.4) / 0.6, 'fx-dmg');
  }

  /** Fill in what the ghost shows from libaw's engine (`m`: the wasm module, web/pkg): each
   *  moving unit's range and each shooter's targets, the unit menus' options, the damage boxes,
   *  the landing tiles of unloads and the build menus, each worked out on the board before its step
   *  (with any CO power used earlier in the turn). Errors leave the plan's guesses. relayout: lay
   *  the timeline out again with the engine's menus (a menu of Wait alone isn't opened); only
   *  before playing starts, since it moves later segments. */
  ghostEngine(m, { relayout = false } = {}) {
    if (!this.ghost || !m) return;
    const g = this.ghost, steps = this.sb.steps, keys = (ps) => new Set(ps.map(([x, y]) => `${x},${y}`));
    const js = [];
    const json = (i) => (js[i] ??= JSON.stringify(this.powered(i)));
    const call = (f) => { try { const r = JSON.parse(f()); return r.error ? null : r; } catch { return null; } };
    const segsOf = (i) => [...(this.plan.pre[i] ?? []), ...(this.plan.post[i] ?? [])];
    let spent = 0;
    const funds0 = this.sb.start.players.find((q) => q.id === this.sb.player)?.funds ?? Infinity;
    steps.forEach((s, i) => {
      const segs = segsOf(i);
      // Ranges: a move's (or a unit selected in place), a shooter's targets.
      const id = s.type === 'move' ? s.unit_id : s.type === 'fire' ? s.attacker?.unit_id : segs.find((x) => x.phase === 'select' && x.range) ? segs.find((x) => x.menu?.unit != null)?.menu.unit : null;
      if (s.type === 'fire' && id != null) {
        // The targets the menu's Fire offered from where the shooter stands.
        const r = call(() => m.options(json(i), id, s.attacker.at[0], s.attacker.at[1]));
        if (r) g.attack[i] = keys(r.options.filter((o) => o.target && o.type.startsWith('attack')).map((o) => o.target));
      } else if (id != null && (s.type !== 'move' || this.states[i].units[id])) {
        const r = call(() => m.reach(json(i), id));
        if (r) g.move[i] = keys(r.moves);
      }
      // The unit menu: what the engine offers the unit where it stops.
      // (also for a move whose guessed menu was Wait alone, so not planned: the engine may know more)
      const mu = this.plan.asks?.[i] ?? segs.find((x) => x.menu?.kind === 'unit' && x.menu.unit != null && !x.menu.lone)?.menu;
      if (mu) {
        const r = call(() => m.options(json(i), mu.unit, mu.to[0], mu.to[1]));
        if (r) g.menu[i] = unitMenu(r.options, mu.chosen, { moved: mu.moved, unit: this.states[i].units[mu.unit] });
      }
      if (s.type === 'fire' && s.attacker && s.defender) {
        // Before the move that brought the attacker here, if one did.
        const prev = steps[i - 1], k = prev?.type === 'move' && prev.unit_id === s.attacker.unit_id ? i - 1 : i;
        const path = k < i ? prev.path : [s.attacker.at];
        const r = call(() => m.preview(json(k), JSON.stringify({ type: 'move', unit_id: s.attacker.unit_id, path, then: { type: 'attack', target: s.defender.at } })));
        if (r) {
          const span = (a, b) => [Math.min(a, b), Math.max(a, b)];
          g.damage[i] = { dmg: span(r.defender_hp - r.low_luck.defender_hp, r.defender_hp - r.high_luck.defender_hp),
            counter: span(r.attacker_hp - r.low_luck.attacker_hp, r.attacker_hp - r.high_luck.attacker_hp) };
        }
      }
      if (s.type === 'unload') {
        const t = this.states[i].units[s.transport_id];
        const r = t && call(() => m.options(json(i), t.id, t.x, t.y));
        if (r) g.drop[i] = keys([s.to, ...r.options.filter((o) => o.type === 'unload' && o.cargo_id === s.unit_id).map((o) => o.to)]);
      }
      if (s.type === 'build') {
        const r = call(() => m.build_menu(json(i), s.unit.x, s.unit.y));
        if (r?.units?.length) {
          const left = funds0 - spent;
          g.build[i] = r.units.map((b) => ({ ...b, off: b.unit !== s.unit.kind && b.cost > left }));
          spent += r.units.find((b) => b.unit === s.unit.kind)?.cost ?? 0;
        }
      }
    });
    if (relayout) this.layout();
  }

  /** The board before step i with the CO powers used earlier in the turn switched on. */
  powered(i) {
    const st = JSON.parse(JSON.stringify(this.states[i]));
    for (const s of this.sb.steps.slice(0, i))
      if (s.type === 'power') { const q = st.players.find((x) => x.id === s.player); if (q) q.power = s.super ? 'S' : 'Y'; }
    return st;
  }

  /** The phase playing at t ('move', 'fire', ...). */
  phaseAt(t) { return segmentAt(this.tl, t)?.seg.phase ?? null; }

  setCamera(seg) {
    const [x, y, w, h] = this.boxFor(seg.shot);
    // A blast shakes the picture (fireFx sets this.shake, in tiles); deterministic in t.
    const s = this.shake || 0, dx = s * Math.sin(seg.t0 * 97 + (this.shakeT ?? 0) * 71), dy = s * Math.cos(seg.t0 * 53 + (this.shakeT ?? 0) * 89);
    const up = y <= 0 ? (this.board.headroom ?? 0) : 0;   // the top row's tall sprites, as in Board.draw
    const down = this.board.even && y + h >= this.sb.start.height ? (this.board.headroom ?? 0) : 0;
    this.board.svg.setAttribute('viewBox', `${(x + dx) * TILE} ${(y + dy - up) * TILE} ${w * TILE} ${(h + up + down) * TILE}`);
    this.board.svg.style.aspectRatio = `${w} / ${h + up + down}`;
  }

  flash(x, y, a, cls = 'fx-hit') {
    el('circle', { cx: (x + 0.5) * TILE, cy: (y + 0.5) * TILE, r: TILE * (0.3 + 0.5 * (1 - a)), class: cls, opacity: Math.max(0, a) }, this.fxLayer);
  }

  pop(x, y, text, p, cls) {
    const rise = 18 * ease(Math.min(1, p));
    const top = -(this.board.headroom ?? 0) * TILE + 14;   // on the top row, it rises no higher than the board's edge
    const half = text.length * 5 + 4, W = this.board.state.width * TILE;   // and stays inside it at the sides
    const tx = el('text', { x: Math.min(Math.max((x + 0.5) * TILE, half), W - half), y: Math.max(top, (y + 0.62) * TILE - rise), class: cls, opacity: p < 0.85 ? 1 : (1 - p) / 0.15 }, this.popLayer ?? this.fxLayer);
    tx.textContent = text;
  }

  fireFx(st, before, p) {
    const a = st.attacker, d = st.defender;
    const guns = GUNS.has(before.units[a?.unit_id]?.kind);
    if (a && d && p < 0.45) {
      const [x0, y0] = a.at, [x1, y1] = d.at;
      // A shell. Guns (infantry, recon, anti-air) draw no projectile: the shooter shakes instead.
      if (!guns) {
        const f = p / 0.45, x = x0 + (x1 - x0) * f, y = y0 + (y1 - y0) * f;
        el('circle', { cx: (x + 0.5) * TILE, cy: (y + 0.5) * TILE, r: 4, class: 'fx-shot' }, this.fxLayer);
      }
    }
    if (a && guns && p < 0.4) this.jolt(a.unit_id, p);
    // A counterattack with guns shakes the defender a beat later.
    const au = before.units[a?.unit_id], countered = au && a.display_hp != null && a.display_hp < Math.ceil(au.hp.hi / 10);
    if (d && countered && GUNS.has(before.units[d.unit_id]?.kind) && p > 0.25 && p < 0.65) this.jolt(d.unit_id, p - 0.25);
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

  /** A unit firing a gun: a quick, small shake of its sprite (q from 0, over about 0.4). */
  jolt(id, q) {
    const g = this.board.svg.querySelector(`g.unit[data-id="${id}"]`);
    if (!g) return;
    const k = 1.6 * Math.max(0, 1 - q / 0.4);
    g.setAttribute('transform', `translate(${(k * Math.sin(q * 190)).toFixed(2)} ${(0.5 * k * Math.cos(q * 230)).toFixed(2)})`);
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
