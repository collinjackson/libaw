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
        if (side.display_hp <= 0) drop(side.unit_id);
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
    case 'join': drop(step.from); units[step.into] = clone(step.unit); break;
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

/** Shots: a unit's move with what it did there (fire, capture, join), or one other step on its
 *  own. Each shot: {from, to (exclusive), kind, focus: [x, y] or null, player, tiles}. */
export function shots(sb) {
  const out = [];
  const steps = sb.steps;
  for (let i = 0; i < steps.length; i++) {
    const st = steps[i];
    if (st.type === 'end') continue;
    let j = i + 1;
    if (st.type === 'move' && j < steps.length && ['fire', 'capture', 'join'].includes(steps[j].type)) j++;
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
    }
    out.push({ from: i, to: j, kind: last.type, focus, player: sb.player, tiles });
    i = j - 1;
  }
  return out;
}

export const DUR = { tile: 0.14, fire: 1.1, capture: 0.9, power: 1.6, build: 0.5, join: 0.5, other: 0.2, hold: 0.6 };

/** A timeline over the chosen shots (all of them for a full replay): segments
 *  {t0, t1, step index, phase, shot} where phase is 'move' | 'fire' | 'capture' | 'power' |
 *  'build' | 'join' | 'hold' | 'other'. Steps between chosen shots are applied as a cut (no time).
 *  before(step, i, shot) -> [{phase, dur}]: extra segments a caller plays ahead of a step (its
 *  own phases, drawn by the viewer's overlays). dur(step) -> seconds or null: a caller's length for
 *  a step (e.g. longer for a kill), else DUR's. hold(shot) -> seconds or null: the pause after a shot. */
export function timeline(sb, chosen, { before = null, dur = null, hold = null } = {}) {
  const segs = [];
  let t = 0;
  for (const sh of chosen) {
    for (let i = sh.from; i < sh.to; i++) {
      const st = sb.steps[i];
      for (const x of (before ? before(st, i, sh) : null) ?? []) {
        segs.push({ t0: t, t1: t + x.dur, step: i, phase: x.phase, shot: sh });
        t += x.dur;
      }
      const d = dur?.(st) ?? (st.type === 'move' ? Math.max(1, st.path.length - 1) * DUR.tile : (DUR[st.type] ?? DUR.other));
      segs.push({ t0: t, t1: t + d, step: i, phase: st.type, shot: sh });
      t += d;
    }
    const h = hold?.(sh) ?? DUR.hold;   // the pause on the shot's end (hold: e.g. long enough to read its caption)
    segs.push({ t0: t, t1: t + h, step: sh.to - 1, phase: 'hold', shot: sh });
    t += h;
  }
  return { segs, duration: t };
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
