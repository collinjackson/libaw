// Pure helpers over the socket API's state and legal actions: no DOM, so the headless test
// (web/test/headless.mjs) runs the same code the page does.

export const key = (x, y) => `${x},${y}`;
export const sameTile = (a, b) => a[0] === b[0] && a[1] === b[1];
export const displayHp = (u) => Math.ceil((u.hp?.hi ?? 100) / 10);

/** Units on the board (not carried), by tile. */
export function unitsByTile(state) {
  const m = new Map();
  for (const u of Object.values(state.units)) if (u.carried_by == null) m.set(key(u.x, u.y), u);
  return m;
}

/** Buildings by tile: {terrain, owner, capture}. */
export function buildingsByTile(state) {
  return new Map(state.buildings.map(([x, y, b]) => [key(x, y), b]));
}

/** Terrain id at a tile. */
export const terrainAt = (state, x, y) => state.terrain[y * state.width + x];

/** A unit's move actions grouped by the tile they end on: Map "x,y" -> [actions]. */
export function movesByDest(legal, unitId) {
  const m = new Map();
  for (const a of legal) {
    if (a.type !== 'move' || a.unit_id !== unitId) continue;
    const end = a.path[a.path.length - 1];
    const k = key(end[0], end[1]);
    if (!m.has(k)) m.set(k, []);
    m.get(k).push(a);
  }
  return m;
}

/** Units that have at least one legal move. */
export function movableUnits(legal) {
  return new Set(legal.filter((a) => a.type === 'move').map((a) => a.unit_id));
}

/** Build actions at a tile. */
export const buildsAt = (legal, x, y) => legal.filter((a) => a.type === 'build' && a.at[0] === x && a.at[1] === y);

/** Power actions available now ("Y", "S"). */
export const powersNow = (legal) => legal.filter((a) => a.type === 'power');

/** Tiles any of a unit's moves can attack: Map "x,y" -> [actions]. */
export function attackTargets(legal, unitId) {
  const m = new Map();
  for (const a of legal) {
    if (a.type !== 'move' || a.unit_id !== unitId || a.then?.type !== 'attack') continue;
    const k = key(a.then.target[0], a.then.target[1]);
    if (!m.has(k)) m.set(k, []);
    m.get(k).push(a);
  }
  return m;
}

const THEN_ORDER = ['attack', 'capture', 'launch', 'attack_seam', 'join', 'load', 'unload', 'supply', 'repair', 'hide', 'unhide', 'wait', 'delete'];
export const THEN_LABEL = {
  attack: 'Fire', capture: 'Capture', launch: 'Launch', attack_seam: 'Hit seam', join: 'Join', load: 'Load',
  unload: 'Unload', supply: 'Supply', repair: 'Repair', hide: 'Dive / Hide', unhide: 'Surface / Unhide', wait: 'Wait',
  delete: 'Delete unit',
};

/**
 * The menu for a unit moving to a tile: one entry per action kind, in AWBW's usual order.
 * Kinds that need a second pick (attack, unload, repair, launch, seam) carry every choice.
 */
export function menuAt(actions) {
  const by = new Map();
  for (const a of actions) {
    const t = a.then?.type ?? 'wait';
    if (!by.has(t)) by.set(t, []);
    by.get(t).push(a);
  }
  return THEN_ORDER.filter((t) => by.has(t)).map((t) => ({ kind: t, label: THEN_LABEL[t], actions: by.get(t) }));
}

/** Where a second-pick action points: attack/repair/launch/seam target, or an unload's drop tile. */
export function pickTile(a) {
  const t = a.then;
  if (!t) return null;
  if (t.type === 'unload') return t.to;
  return t.target ?? null;
}

/** A short line for the log, for actions we sent or saw. */
export function describe(a, state) {
  if (!a) return '';
  if (a.type === 'end_turn') return 'ended the turn';
  if (a.type === 'build') return `built ${a.unit} at ${a.at.join(',')}`;
  if (a.type === 'power') return a.power === 'S' ? 'used the super power' : 'used the CO power';
  if (a.type === 'move') {
    const u = state?.units?.[a.unit_id];
    const end = a.path[a.path.length - 1];
    const what = u ? u.kind : `unit ${a.unit_id}`;
    const then = a.then?.type ?? 'wait';
    const tail = then === 'wait' ? '' : ` and ${THEN_LABEL[then]?.toLowerCase() ?? then}${pickTile(a) ? ' ' + pickTile(a).join(',') : ''}`;
    return `${what} to ${end.join(',')}${tail}`;
  }
  return a.type;
}

/**
 * Tiles a player can see, for dimming fog: within each own unit's vision (Manhattan), plus own
 * properties. An approximation of the engine's fog (forests and reefs hide units unless
 * adjacent; mountains add vision for foot units): the server's state is already the player's
 * real view, this only shades the board.
 */
export function visibleTiles(state, me, unitInfo, terrainName) {
  const seen = new Set();
  for (const [x, y, b] of state.buildings) if (b.owner === me) seen.add(key(x, y));
  for (const u of Object.values(state.units)) {
    if (u.owner !== me || u.carried_by != null) continue;
    let r = unitInfo[u.kind]?.vision ?? 2;
    const tn = terrainName(terrainAt(state, u.x, u.y));
    if (tn === 'Mountain' && ['Infantry', 'Mech'].includes(u.kind)) r += 3;
    for (let dy = -r; dy <= r; dy++)
      for (let dx = -r + Math.abs(dy); dx <= r - Math.abs(dy); dx++) {
        const x = u.x + dx, y = u.y + dy;
        if (x >= 0 && y >= 0 && x < state.width && y < state.height) seen.add(key(x, y));
      }
  }
  return seen;
}

/** A simple bot for the headless test: attack if it can, else capture, else build, else end. */
export function simplePick(legal, acted) {
  const fresh = (a) => !(a.type === 'move' && acted.has(a.unit_id));
  return (
    legal.find((a) => fresh(a) && a.type === 'move' && a.then?.type === 'attack') ||
    legal.find((a) => fresh(a) && a.type === 'move' && a.then?.type === 'capture') ||
    legal.find((a) => a.type === 'build' && ['Infantry', 'Tank', 'Artillery'].includes(a.unit)) ||
    legal.find((a) => fresh(a) && a.type === 'move' && a.path.length > 1 && (a.then?.type ?? 'wait') === 'wait') ||
    { type: 'end_turn' }
  );
}

/** "deals 49–57%, counter 14–20%" from a server `preview` reply (HP is 0-100 internal, so lost HP is
 * the damage %). The counter's range runs either way between the two luck runs, so it is sorted. */
export function damageLabel(p) {
  const range = (a, b) => { const [lo, hi] = [Math.min(a, b), Math.max(a, b)]; return lo === hi ? `${lo}%` : `${lo}–${hi}%`; };
  const dealt = range(p.defender_hp - p.low_luck.defender_hp, p.defender_hp - p.high_luck.defender_hp);
  const lost = range(p.attacker_hp - p.low_luck.attacker_hp, p.attacker_hp - p.high_luck.attacker_hp);
  return lost === '0%' ? `deals ${dealt}` : `deals ${dealt}, counter ${lost}`;
}
