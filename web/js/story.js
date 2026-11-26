// Pure helpers over a storyboard (docs/storyboard.md): apply its steps to the board, group them
// into shots, and lay out a timeline a viewer can render at any time t (so a headless capture
// gets the same frames as the page). No DOM. Choosing which shots to show is the caller's.

const clone = (x) => JSON.parse(JSON.stringify(x));
const bkey = (x, y) => `${x},${y}`;

/** The board after one step (a new object; the input is left alone). Steps for units the
 *  record doesn't show (fog) are skipped. */
export function applyStep(state, step) {
  const s = clone(state);
  const units = s.units;
  const drop = (id) => {
    const u = units[id];
    if (!u) return;
    for (const c of u.cargo ?? []) delete units[c];
    delete units[id];
  };
  switch (step.type) {
    case 'move': {
      const u = units[step.unit_id];
      if (u) { const [x, y] = step.path[step.path.length - 1]; Object.assign(u, { x, y, moved: true }); }
      break;
    }
    case 'fire':
      for (const side of [step.attacker, step.defender]) {
        if (!side || !units[side.unit_id]) continue;
        // display_hp is null when the record hides HP (Sonja): the unit lives on, HP unknown.
        if (side.display_hp != null && side.display_hp <= 0) drop(side.unit_id);
        else units[side.unit_id].hp = side.hp;
      }
      break;
    case 'capture': {
      const b = s.buildings.find(([x, y]) => x === step.at[0] && y === step.at[1]);
      if (b) {
        b[2].capture = step.done ? 20 : step.capture;
        if (step.done && step.new_owner != null) b[2].owner = Number(step.new_owner);
      }
      break;
    }
    case 'build': units[step.unit.id] = clone(step.unit); break;
    // Eagle's super power (Lightning Strike) lets his units that aren't foot soldiers act again.
    case 'power':
      if (step.super && step.co === 'Eagle')
        for (const u of Object.values(units)) if (u.owner === step.player && !['Infantry', 'Mech'].includes(u.kind)) u.moved = false;
      break;
    case 'join': drop(step.from); units[step.into] = clone(step.unit); break;
    case 'load': {
      const u = units[step.unit_id], t = units[step.transport_id];
      if (u) Object.assign(u, { carried_by: step.transport_id, moved: true });
      if (t && !(t.cargo ??= []).includes(step.unit_id)) t.cargo.push(step.unit_id);
      break;
    }
    case 'unload': {
      const u = units[step.unit_id], t = units[step.transport_id];
      if (t) t.cargo = (t.cargo ?? []).filter((c) => c !== step.unit_id);
      if (u) Object.assign(u, { x: step.to[0], y: step.to[1], carried_by: null, moved: true });
      break;
    }
    case 'hide': case 'unhide':
      if (units[step.unit_id]) Object.assign(units[step.unit_id], { dived: step.type === 'hide', moved: true });
      break;
    case 'delete': drop(step.unit_id); break;
    case 'repair': {
      const u = units[step.target_id];
      if (u && step.display_hp != null) u.hp = { lo: Math.max(1, step.display_hp * 10 - 9), hi: step.display_hp * 10 };
      if (units[step.unit_id]) units[step.unit_id].moved = true;
      break;
    }
    // Area damage that never kills: Black Bomb (3 tiles round the bomb, which goes) and a silo
    // (2 tiles round its target, the launcher's silo left empty).
    case 'explode': case 'launch': {
      const [cx, cy] = step.type === 'explode' ? step.at : step.target, r = step.type === 'explode' ? 3 : 2;
      if (step.type === 'explode') drop(step.unit_id);
      else {
        const b = s.buildings.find(([x, y]) => x === step.silo[0] && y === step.silo[1]);
        if (b) b[2].terrain = 112;   // Missile Silo Empty
        const who = Object.values(units).find((u) => u.carried_by == null && u.x === step.silo[0] && u.y === step.silo[1]);
        if (who) who.moved = true;
      }
      const d = (step.damage ?? 0) * 10;
      for (const u of Object.values(units))
        if (u.carried_by == null && Math.abs(u.x - cx) + Math.abs(u.y - cy) <= r)
          u.hp = { lo: Math.max(1, u.hp.lo - d), hi: Math.max(1, u.hp.hi - d) };
      break;
    }
    default: break;
  }
  return s;
}

/** Every state along the way: states[i] is the board before step i; states[n] after the last. */
export function statesOf(sb) {
  const out = [sb.start];
  for (const st of sb.steps) out.push(applyStep(out[out.length - 1], st));
  return out;
}

/** Shots: a unit's move with what it did there (FOLLOW: fire, capture, join, ...), or one other step on its
 *  own. Each shot: {from, to (exclusive), kind, focus: [x, y] or null, player, tiles}. */
export function shots(sb) {
  const out = [];
  const steps = sb.steps;
  for (let i = 0; i < steps.length; i++) {
    const st = steps[i];
    if (st.type === 'end') continue;
    let j = i + 1;
    if (st.type === 'move' && j < steps.length && FOLLOW.has(steps[j].type)) j++;
    const last = steps[j - 1];
    let focus = null;
    const tiles = [];
    if (st.type === 'move') { tiles.push(...st.path); focus = st.path[st.path.length - 1]; }
    if (last.type === 'fire') {
      for (const side of [last.attacker, last.defender]) if (side) tiles.push(side.at);
      focus = last.defender?.at ?? focus;
    } else if (last.type === 'capture') {
      tiles.push(last.at); focus = last.at;
    } else if (last.type === 'build') {
      tiles.push([last.unit.x, last.unit.y]); focus = [last.unit.x, last.unit.y];
    } else if (last.type === 'unload' || last.type === 'explode' || last.type === 'launch') {
      const at = last.to ?? last.at ?? last.target;
      tiles.push(at); focus = at;
      if (last.silo) tiles.push(last.silo);
    }
    out.push({ from: i, to: j, kind: last.type, focus, player: sb.player, tiles });
    i = j - 1;
  }
  return out;
}

/** Steps that finish the move before them (one shot): what the unit did where it stopped. */
export const FOLLOW = new Set(['fire', 'capture', 'join', 'load', 'unload', 'supply', 'hide', 'unhide', 'explode', 'launch', 'repair', 'delete']);

export const DUR = { tile: 0.14, fire: 1.1, capture: 0.9, power: 1.6, build: 0.5, join: 0.5, unload: 0.35, explode: 1.2, launch: 1.2, repair: 0.5, other: 0.2, hold: 0.6 };

/** A timeline over the chosen shots (all of them for a full replay): segments
 *  {t0, t1, step index, phase, shot} where phase is 'move' | 'fire' | 'capture' | 'power' |
 *  'build' | 'join' | 'hold' | 'other'. Steps between chosen shots are applied as a cut (no time).
 *  before(step, i, shot) -> [{phase, dur}]: extra segments a caller plays ahead of a step (its
 *  own phases, drawn by the viewer's overlays); a segment's other fields are kept on it.
 *  after(step, i, shot): the same, played right after the step. dur(step) -> seconds or null: a caller's length for
 *  a step (e.g. longer for a kill), else DUR's. hold(shot) -> seconds or null: the pause after a shot. */
export function timeline(sb, chosen, { before = null, after = null, dur = null, hold = null } = {}) {
  const segs = [];
  let t = 0;
  for (const sh of chosen) {
    for (let i = sh.from; i < sh.to; i++) {
      const st = sb.steps[i];
      const extra = (xs) => {
        for (const x of xs ?? []) {
          segs.push({ ...x, t0: t, t1: t + x.dur, step: i, phase: x.phase, shot: sh });
          t += x.dur;
        }
      };
      extra(before?.(st, i, sh));
      const d = dur?.(st) ?? (st.type === 'move' ? Math.max(1, st.path.length - 1) * DUR.tile : (DUR[st.type] ?? DUR.other));
      segs.push({ t0: t, t1: t + d, step: i, phase: st.type, shot: sh });
      t += d;
      extra(after?.(st, i, sh));
    }
    const h = hold?.(sh) ?? DUR.hold;   // the pause on the shot's end (hold: e.g. long enough to read its caption)
    segs.push({ t0: t, t1: t + h, step: sh.to - 1, phase: 'hold', shot: sh });
    t += h;
  }
  return { segs, duration: t };
}

/** The time in timeline `to` that shows what `from` shows at t: the same segment (by step, phase
 *  and its place among that step's segments of the phase) at the same fraction; when that
 *  segment is gone from `to`, the start of the next one that's still there. Past the end, the
 *  same time past `to`'s end. For a viewer whose timeline changes while it plays. */
export function remapTime(from, to, t) {
  const id = (segs) => {
    const n = new Map();
    return segs.map((g) => { const k = `${g.step}|${g.phase}`, c = n.get(k) ?? 0; n.set(k, c + 1); return `${k}|${c}`; });
  };
  const a = id(from.segs), b = new Map(id(to.segs).map((k, i) => [k, to.segs[i]]));
  if (t >= from.duration) return to.duration + (t - from.duration);
  let i = from.segs.findIndex((g) => t < g.t1);
  if (i < 0) i = from.segs.length - 1;
  const g = from.segs[i], h = b.get(a[i]);
  if (h) return h.t0 + (h.t1 - h.t0) * Math.min(1, Math.max(0, (t - g.t0) / Math.max(1e-9, g.t1 - g.t0)));
  for (let j = i + 1; j < a.length; j++) if (b.has(a[j])) return b.get(a[j]).t0;
  return to.duration;
}

/** The segment at time t and how far through it (0..1). */
export function segmentAt(tl, t) {
  if (!tl.segs.length) return null;
  const seg = tl.segs.find((s) => t < s.t1) ?? tl.segs[tl.segs.length - 1];
  const p = Math.min(1, Math.max(0, (t - seg.t0) / Math.max(1e-6, seg.t1 - seg.t0)));
  return { seg, p };
}

/** Where a moving unit is at progress p along its path (tile units, fractional). */
export function alongPath(path, p) {
  if (path.length < 2) return path[0];
  const f = p * (path.length - 1);
  const i = Math.min(path.length - 2, Math.floor(f));
  const r = f - i;
  return [path[i][0] + (path[i + 1][0] - path[i][0]) * r, path[i][1] + (path[i + 1][1] - path[i][1]) * r];
}

export { bkey };
