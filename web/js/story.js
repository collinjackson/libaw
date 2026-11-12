// Pure helpers over a storyboard (docs/storyboard.md): apply its steps to the board, group them
// into shots, pick the highlights, and lay out a timeline a viewer can render at any time t
// (so a headless capture gets the same frames as the page). No DOM.

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

const value = (units, u, hpLost) => ((units?.[u.kind]?.cost ?? 0) * hpLost) / 10;

/** Shots: a unit's move with what it did there (fire, capture, join), or one other step on its
 *  own. Each shot: {from, to (exclusive), kind, score, focus: [x, y], player, tiles}. score is
 *  the drama: army value destroyed on both sides (a kill counts 1.5x), completed captures (an HQ
 *  most), powers. data: {units: data/units.json (costs), terrain: data/terrain.json (to spot an HQ)}. */
export function shots(sb, data, states = statesOf(sb)) {
  const unitsData = data?.units ?? data;
  const out = [];
  const steps = sb.steps;
  for (let i = 0; i < steps.length; i++) {
    const st = steps[i];
    if (st.type === 'end') continue;
    let j = i + 1;
    if (st.type === 'move' && j < steps.length && ['fire', 'capture', 'join'].includes(steps[j].type)) j++;
    const last = steps[j - 1];
    const before = states[i];
    let score = 0, focus = null;
    const tiles = [];
    if (st.type === 'move') { tiles.push(...st.path); focus = st.path[st.path.length - 1]; }
    if (last.type === 'fire') {
      for (const side of [last.attacker, last.defender]) {
        if (!side) continue;
        const u = before.units[side.unit_id];
        if (!u) continue;
        const lost = Math.max(0, Math.ceil(u.hp.hi / 10) - side.display_hp);
        score += value(unitsData, u, lost) * (side.display_hp <= 0 ? 1.5 : 1);
        tiles.push(side.at);
      }
      focus = last.defender?.at ?? focus;
    } else if (last.type === 'capture') {
      tiles.push(last.at); focus = last.at;
      if (last.done) {
        const b = before.buildings.find(([x, y]) => x === last.at[0] && y === last.at[1]);
        const name = String(data?.terrain?.[b?.[2]?.terrain]?.name ?? '');
        score += /HQ/i.test(name) ? 40000 : 4000;
      } else score += 500;
    } else if (last.type === 'power') {
      score += last.super ? 9000 : 6000;
    } else if (last.type === 'build') {
      tiles.push([last.unit.x, last.unit.y]); focus = [last.unit.x, last.unit.y];
      score += 100;
    }
    score += focusBonus(sb.focus, last, before);
    const dk = last.type === 'fire' && last.defender ? before.units[last.defender.unit_id]?.kind : null;
    const sig = `${last.type}:${dk ?? ''}:${last.type === 'fire' ? (last.defender?.display_hp ?? 1) <= 0 : !!last.done}`;
    out.push({ from: i, to: j, kind: last.type, score, focus, player: sb.player, tiles, sig });
    i = j - 1;
  }
  return out;
}

/** sb.focus (optional, from whoever made the storyboard): what moved the game, as
 *  [{type: 'fire', kill: true, kind: 'Infantry', owner: 102, weight}, {type: 'capture', done, weight},
 *  {type: 'power', weight}, {type: 'build', kind, weight}]. A shot that matches one ranks by its
 *  weight ahead of any that don't, so a reel shows what the odds turned on, not just the loudest hit. */
export function focusBonus(focus, last, before) {
  let best = 0;
  for (const f of focus ?? []) {
    if (f.type !== last.type) continue;
    if (last.type === 'fire') {
      const d = last.defender;
      const u = d && before.units[d.unit_id];
      if (!u || (f.kind && u.kind !== f.kind) || (f.owner != null && u.owner !== f.owner)) continue;
      if (f.kill && d.display_hp > 0) continue;
    } else if (last.type === 'capture') {
      if (f.done != null && !!last.done !== !!f.done) continue;
    } else if (last.type === 'build') {
      if (f.kind && last.unit.kind !== f.kind) continue;
    }
    best = Math.max(best, 1e5 * (f.weight ?? 1));
  }
  return best;
}

/** The n biggest shots, back in the order they happened. */
export function highlights(sb, data, n = 3, perSig = 2) {
  const all = shots(sb, data);
  // At most perSig shots alike (three Infantry kills in a row make a dull reel).
  const seen = new Map(), top = [];
  for (const s of [...all].filter((s) => s.score > 0).sort((a, b) => b.score - a.score)) {
    if ((seen.get(s.sig) ?? 0) >= perSig) continue;
    seen.set(s.sig, (seen.get(s.sig) ?? 0) + 1);
    top.push(s);
    if (top.length >= n) break;
  }
  return top.sort((a, b) => a.from - b.from);
}

export const DUR = { tile: 0.14, fire: 1.1, capture: 0.9, power: 1.6, build: 0.5, join: 0.5, other: 0.2, hold: 0.6 };

/** A timeline over the chosen shots (all of them for a full replay): segments
 *  {t0, t1, step index, phase} where phase is 'move' | 'fire' | 'capture' | 'power' | 'build' |
 *  'join' | 'hold' | 'other'. Steps between chosen shots are applied as a cut (no time). */
export function timeline(sb, chosen) {
  const segs = [];
  let t = 0;
  for (const sh of chosen) {
    for (let i = sh.from; i < sh.to; i++) {
      const st = sb.steps[i];
      const d = st.type === 'move' ? Math.max(1, st.path.length - 1) * DUR.tile : (DUR[st.type] ?? DUR.other);
      segs.push({ t0: t, t1: t + d, step: i, phase: st.type, shot: sh });
      t += d;
    }
    segs.push({ t0: t, t1: t + DUR.hold, step: sh.to - 1, phase: 'hold', shot: sh });
    t += DUR.hold;
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
