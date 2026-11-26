// The ghost (Replay's `ghost` option): a player's cursor and menus acted out ahead of each step,
// after AWBW Ghost Input by onetrickwolf (MIT, docs/sources.md), whose behaviour and timings this
// follows in libaw's own code. This file plans the timeline segments from a storyboard and works
// out what the menus list; replay.js draws them. No DOM.
import { FOLLOW } from './story.js';

/** Timings in seconds: Ghost Input's, a little shorter. */
export const GT = {
  select: 0.22,   // the cursor on a unit (or base) before it's clicked
  aim: 0.55,      // a move's range up and its arrow traced, the cursor leading
  menu: 0.5,      // a menu: open with the chosen option hovered, then pressed...
  press: 0.12,    // ...for this long at the end, before it closes
  target: 0.38,   // hover a landing, repair, bomb or silo tile, then press it
  attack: 0.55,   // hover an attack's target with the damage box up, then press it
  idle: 4,        // the cursor hides this long after the turn's last step
};

/** The cursor's travel over `tiles` tiles: 0.06 s + 0.035 a tile, from 0.12 to 0.7 (Ghost
 *  Input's mouse timing). */
export const hopTime = (tiles) => Math.min(0.7, Math.max(0.12, 0.06 + 0.035 * tiles));
const dist = (a, b) => Math.hypot(b[0] - a[0], b[1] - a[1]);

/** The menu option that picks each kind of step. */
export const LABEL = { fire: 'Fire', capture: 'Capt', join: 'Join', load: 'Load', unload: 'Unload', supply: 'Supply', hide: 'Hide',
  unhide: 'Unhide', explode: 'Explode', launch: 'Launch', repair: 'Repair', delete: 'Delete' };
const THEN = { wait: 'Wait', attack: 'Fire', attack_seam: 'Fire', capture: 'Capt', load: 'Load', join: 'Join', supply: 'Supply',
  hide: 'Hide', unhide: 'Unhide', delete: 'Delete', launch: 'Launch', repair: 'Repair', unload: 'Unload' };
/** AWBW's order for the unit menu. */
export const MENU_ORDER = ['Fire', 'Unload', 'Repair', 'Supply', 'Hide', 'Unhide', 'Capt', 'Launch', 'Explode', 'Delete', 'Wait'];

/** The unit menu as the player saw it, in AWBW's order: `thens` is what libaw's engine offers
 *  there (wasm `options`, contract format), or null without the engine (then just the chosen
 *  option, Wait, and what the unit alone tells). A Load or Join is offered alone. AWBW drops cargo
 *  only from where a transport stands, offers Delete only to a unit that hasn't moved, and
 *  Hide/Unhide to every Sub and Stealth. */
export function unitMenu(thens, chosen, { moved = false, unit = null } = {}) {
  if (chosen === 'Load' || chosen === 'Join') return [chosen];
  const have = new Set(thens ? thens.map((t) => THEN[t.type]).filter(Boolean) : []);
  have.delete('Load'); have.delete('Join');
  if (moved) have.delete('Unload');
  if (!moved) have.add('Delete'); else have.delete('Delete');
  if (unit?.kind === 'Black Bomb') have.add('Explode');
  if (unit?.kind === 'Sub' || unit?.kind === 'Stealth') have.add(unit.dived ? 'Unhide' : 'Hide');
  have.add('Wait');
  have.add(chosen);
  return [...MENU_ORDER.filter((o) => have.has(o)), ...[...have].filter((o) => !MENU_ORDER.includes(o))];
}

const unitOn = (s, [x, y]) => Object.values(s.units).find((w) => w.carried_by == null && w.x === x && w.y === y) ?? null;

/** The unit that acts in step i ({id, at, unit}), on the board before it, or null (fog, or a
 *  step no unit does). Supply names only the units supplied: the APC is the one that just moved,
 *  or the player's APC beside the first of them. */
export function actorOf(sb, states, i) {
  const st = sb.steps[i], s = states[i];
  const of = (v) => (v && v.carried_by == null ? { id: v.id, at: [v.x, v.y], unit: v } : null);
  switch (st.type) {
    case 'move': return of(s.units[st.unit_id]);
    case 'fire': return st.attacker ? of(s.units[st.attacker.unit_id]) : null;
    case 'capture': return of(unitOn(s, st.at));
    case 'join': return of(s.units[st.from]);
    case 'load': return of(s.units[st.unit_id]);
    case 'unload': return of(s.units[st.transport_id]);
    case 'hide': case 'unhide': case 'delete': case 'explode': case 'repair': return of(s.units[st.unit_id]);
    case 'launch': return of(unitOn(s, st.silo));
    case 'supply': {
      const prev = sb.steps[i - 1];
      if (prev?.type === 'move' && s.units[prev.unit_id]?.kind === 'APC') return of(s.units[prev.unit_id]);
      const first = s.units[st.supplied?.[0]];
      if (!first) return null;
      return of(Object.values(s.units).find((o) => o.kind === 'APC' && o.owner === first.owner && o.carried_by == null
        && Math.abs(o.x - first.x) + Math.abs(o.y - first.y) === 1));
    }
    default: return null;
  }
}

/** Every tile within r steps of [x, y] on a w by h board, as "x,y" keys. */
export function diamond([x, y], r, w, h) {
  const out = new Set();
  for (let dy = -r; dy <= r; dy++)
    for (let dx = -(r - Math.abs(dy)); dx <= r - Math.abs(dy); dx++)
      if (x + dx >= 0 && y + dy >= 0 && x + dx < w && y + dy < h) out.add(`${x + dx},${y + dy}`);
  return out;
}

/** The ghost's segments for each step: {pre: {i: [seg]}, post: {i: [seg]}}, to play before and
 *  after step i (story.timeline's `before` and `after`). Every segment has {phase, dur, ghost:
 *  true}, and what it shows: the cursor's `from`/`to` (a glide over `glide` seconds, then a rest)
 *  or `at`, a `press` (seconds at its end), a `menu` ({kind: 'unit' | 'cargo' | 'build', options,
 *  chosen, ...}), and tiles to mark. pace scales every duration (0 plans nothing).
 *
 *  A move: the cursor glides to the unit ('hop'), its range and arrow ('aim', or 'select' for a
 *  unit acting in place), the move itself, then the menu at the destination ('menu', after the
 *  move). What the unit then does in the same command (story.FOLLOW) was the menu's choice; an
 *  attack then glides to the target with the damage box ('target'), an unload lists the cargo and
 *  shows the landing tiles ('drop'), and so on. A step on its own (an unload by a transport that
 *  already moved, a build) starts with the cursor going to its unit or base. */
export function ghostPlan(sb, states, { pace = 1 } = {}) {
  const pre = {}, post = {};
  if (!(pace > 0)) return { pre, post };
  const W = sb.start.width, H = sb.start.height;
  let cur = null, lit = null;   // lit: the unit acting, kept in colour while it does
  const seg = (phase, dur, extra = {}) => ({ phase, ghost: true, ...(lit != null ? { lit } : {}), ...extra, dur: dur * pace,
    ...(extra.glide != null ? { glide: extra.glide * pace } : {}), ...(extra.press != null ? { press: extra.press * pace } : {}) });
  const add = (to, i, s) => (to[i] ??= []).push(s);
  const hop = (i, to) => {
    if (cur && dist(cur, to) > 0) add(pre, i, seg('hop', hopTime(dist(cur, to)), { from: cur, to }));
    cur = to;
  };
  // The cursor's glide from where it is to `to`, then `rest` seconds there.
  const reach = (i, phase, to, rest, extra = {}) => {
    const g = cur && dist(cur, to) > 0 ? hopTime(dist(cur, to)) : 0;
    add(pre, i, seg(phase, g + rest, { from: cur, to, glide: g, press: GT.press, ...extra }));
    cur = to;
  };
  const menu = (to, i, at, m) => add(to, i, seg('menu', GT.menu, { at, press: GT.press, menu: m }));
  sb.steps.forEach((st, i) => {
    const next = sb.steps[i + 1], prev = sb.steps[i - 1];
    lit = null;
    if (st.type === 'move') {
      const a = actorOf(sb, states, i);
      if (!a) return;   // a unit the record doesn't show
      const end = st.path[st.path.length - 1], moved = st.path.length > 1;
      hop(i, st.path[0]);
      if (moved) add(pre, i, seg('aim', GT.aim));
      else add(pre, i, seg('select', GT.select, { at: end, range: !a.unit.moved }));
      cur = end;
      if (st.trapped) return;   // ran into a hidden unit: no menu
      // The menu's choice: what the unit does next in the same command. AWBW drops cargo only from
      // where a transport stands, so after a move that's a Wait and the unload a command of its own.
      const goes = next && FOLLOW.has(next.type) && actorOf(sb, states, i + 1)?.id === a.id && (next.type !== 'unload' || !moved);
      const chosen = goes ? LABEL[next.type] : 'Wait';
      menu(post, i, end, { kind: 'unit', unit: a.id, to: end, chosen, moved, options: unitMenu(null, chosen, { moved, unit: a.unit }) });
      return;
    }
    if (st.type === 'build') {
      const at = [st.unit.x, st.unit.y];
      hop(i, at);
      add(pre, i, seg('select', GT.select, { at }));
      menu(pre, i, at, { kind: 'build', chosen: st.unit.kind, owner: st.unit.owner, options: [st.unit.kind] });
      return;
    }
    if (!FOLLOW.has(st.type)) return;
    const a = actorOf(sb, states, i);
    if (!a) return;
    const joined = prev?.type === 'move' && prev.unit_id === a.id && !prev.trapped && (st.type !== 'unload' || prev.path.length === 1);
    const lone = st.type === 'unload' && a.unit.moved && !joined;   // a moved transport clicked again: Unload alone
    if (!lone) lit = a.id;
    if (!joined) {
      hop(i, a.at);
      add(pre, i, seg('select', GT.select, { at: a.at, range: !a.unit.moved }));
      menu(pre, i, a.at, { kind: 'unit', unit: a.id, to: a.at, chosen: LABEL[st.type], moved: false, lone,
        options: lone ? ['Unload'] : unitMenu(null, LABEL[st.type], { unit: a.unit }) });
    }
    cur = a.at;
    const s = states[i];
    switch (st.type) {
      case 'fire':
        if (st.defender) reach(i, 'target', st.defender.at, GT.attack, { attack: true });
        else add(pre, i, seg('target', GT.target, { at: a.at, attack: true }));
        break;
      case 'unload': {
        const cargo = (a.unit.cargo ?? []).map((id) => s.units[id]).filter(Boolean);
        const out = s.units[st.unit_id];
        if (out && !cargo.includes(out)) cargo.unshift(out);
        menu(pre, i, a.at, { kind: 'cargo', chosen: out?.kind ?? null, chosenId: st.unit_id, owner: a.unit.owner,
          options: cargo.map((u) => u.kind), ids: cargo.map((u) => u.id) });
        reach(i, 'drop', st.to, GT.target, { tiles: [st.to] });
        break;
      }
      case 'repair': {
        const t = s.units[st.target_id];
        if (!t) break;
        const near = Object.values(s.units).filter((o) => o.carried_by == null && o.owner === a.unit.owner && o.id !== a.id
          && Math.abs(o.x - a.at[0]) + Math.abs(o.y - a.at[1]) === 1).map((o) => [o.x, o.y]);
        reach(i, 'repair', [t.x, t.y], GT.target, { tiles: near.length ? near : [[t.x, t.y]] });
        break;
      }
      case 'explode':
        add(pre, i, seg('blast', GT.target, { at: a.at, press: GT.press, area: [...diamond(st.at ?? a.at, 3, W, H)] }));
        break;
      case 'launch':
        reach(i, 'silo', st.target, GT.target, { radius: 2 });
        menu(pre, i, st.target, { kind: 'unit', chosen: 'Fire Silo', options: ['Fire Silo'] });
        break;
      default: break;
    }
  });
  return { pre, post };
}

/** Where the ghost's cursor is in ghost segment `seg` at fraction p, on step `st`:
 *  {at: [x, y], press: bool}. `along(path, f)` gives a point along a move's path. */
export function ghostCursor(seg, p, st, along) {
  const ease = (q) => (q < 0.5 ? 2 * q * q : 1 - (-2 * q + 2) ** 2 / 2);
  const dur = seg.dur || 1, pressed = seg.press != null && seg.phase !== 'menu' && p > 1 - seg.press / dur;
  if (seg.phase === 'hop') return { at: [seg.from[0] + (seg.to[0] - seg.from[0]) * p, seg.from[1] + (seg.to[1] - seg.from[1]) * p], press: false };
  // On the unit a moment (the click), then leading the arrow as it's drawn.
  if (seg.phase === 'aim') return { at: p < 0.15 ? st.path[0] : along(st.path, ease(Math.min(1, (p - 0.15) / 0.6))), press: false };
  if (seg.to) {
    const g = (seg.glide ?? 0) / dur;
    if (seg.from && p < g) { const q = p / g; return { at: [seg.from[0] + (seg.to[0] - seg.from[0]) * q, seg.from[1] + (seg.to[1] - seg.from[1]) * q], press: false }; }
    return { at: seg.to, press: pressed };
  }
  return { at: seg.at ?? null, press: pressed };
}

/** Where the cursor rests after a ghost segment. */
export const ghostRest = (seg, st) => seg.to ?? seg.at ?? (seg.phase === 'aim' ? st.path[st.path.length - 1] : null);
