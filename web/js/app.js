// The libaw web client: lobby, board, play. Talks to libaw-server over the socket API
// (docs/socket-api.md). Static files only; serve the repo root (web/play_flakbot.sh does) so
// the page can read ../data/*.json and the bundled states.
import { Conn } from './net.js';
import { LocalConn } from './local.js';
import { Board, TILE } from './board.js';
import * as L from './logic.js';

const $ = (id) => document.getElementById(id);
const params = new URLSearchParams(location.search);
// planner.html: the engine runs in the page (js/local.js) and one seat plays every side.
const PLANNER = document.body.dataset.planner === '1';

const STATES = [
  { label: 'Skirmish (original map, day 1)', path: 'states/skirmish.json' },
  { label: 'Test state: game 1755106, day 5', path: '../docs/eval-harness/test-states/game-1755106-turn-008-D5-pid4204600.json' },
  { label: 'Test state: game 1755654, day 11', path: '../docs/eval-harness/test-states/game-1755654-turn-020-D11-pid4205798.json' },
  { label: 'Test state: game 1729758, day 22', path: '../docs/eval-harness/test-states/game-1729758-turn-042-D22-pid4151012.json' },
];

const ui = {
  conn: null, game: null, me: null, spectator: false, info: null, state: null, legal: [], turnPlayer: null,
  selected: null, mode: null, // mode: {kind: 'pick', actions, tiles} for a second pick (fire, unload, ...)
  data: { units: {}, terrain: {}, tmap: {} }, over: null,
};

// ---------- setup ----------
async function loadData() {
  const [units, terrain, tmap, sprites] = await Promise.all([
    fetch('../data/units.json').then((r) => r.json()),
    fetch('../data/terrain.json').then((r) => r.json()),
    fetch('assets/terrain-map.json').then((r) => r.json()),
    fetch('assets/sprites.svg').then((r) => r.text()),
  ]);
  $('sprites').innerHTML = sprites;
  Object.assign(ui.data, { units, terrain, tmap });
}

const board = { b: null };

function setupTheme() {
  const saved = safeGet('libaw:theme');
  if (saved) document.documentElement.dataset.theme = saved;
  $('btn-theme').onclick = () => {
    const dark = getComputedStyle(document.documentElement).colorScheme === 'dark';
    const next = dark ? 'light' : 'dark';
    document.documentElement.dataset.theme = next;
    safeSet('libaw:theme', next);
  };
}
function safeGet(k) { try { return localStorage.getItem(k); } catch { return null; } }
function safeSet(k, v) { try { localStorage.setItem(k, v); } catch { /* private mode */ } }

// ---------- log ----------
function log(text, cls = '') {
  const li = document.createElement('li');
  li.textContent = text;
  if (cls) li.className = cls;
  $('log').prepend(li);
  while ($('log').children.length > 200) $('log').lastChild.remove();
}

// ---------- connection + lobby ----------
async function connect() {
  ui.conn?.close();
  const url = $('server').value.trim();
  $('connstatus').textContent = 'connecting…';
  try {
    ui.conn = await new Conn(url).open();
  } catch (e) {
    $('connstatus').textContent = e.message;
    return false;
  }
  ui.conn.onMessage = onMessage;
  ui.conn.onClose = () => { $('connstatus').textContent = 'disconnected'; setTurnInfo('disconnected'); };
  const w = await ui.conn.request({ type: 'hello', name: 'libaw-web' });
  $('connstatus').textContent = `${w.server} ${w.version ?? ''} · protocol ${w.protocol}`;
  $('lobby-in').hidden = false;
  await refreshGames();
  return true;
}

async function refreshGames() {
  const { games } = await ui.conn.request({ type: 'list' });
  const box = $('games');
  box.replaceChildren();
  if (!games.length) box.innerHTML = '<div class="muted">No games yet.</div>';
  for (const g of games) {
    const d = document.createElement('div');
    d.className = 'game';
    d.innerHTML = `<div><b>${esc(g.game)}</b> · day ${g.day}${g.over ? ' · over' : ''}${g.fog ? ' · fog' : ''}</div>`;
    const row = document.createElement('div');
    row.className = 'row';
    g.players.forEach((p, i) => {
      const b = document.createElement('button');
      b.textContent = p.taken ? `${p.co} (${p.name || 'taken'})` : `Play ${p.co}`;
      b.disabled = p.taken && !tokenFor(g.game, i);
      b.className = `p${i}`;
      b.onclick = () => join(g.game, i);
      row.append(b);
    });
    const s = document.createElement('button');
    s.textContent = 'Watch';
    s.onclick = () => spectate(g.game);
    row.append(s);
    d.append(row);
    box.append(d);
  }
}

async function createGame() {
  const st = STATES[Number($('newstate').value)];
  const turn = await fetch(st.path).then((r) => r.json());
  try {
    const c = await ui.conn.request({ type: 'create', turn, max_days: Number($('newdays').value) || 0,
      fog: $('newfog').checked, powers: $('newpowers').checked });
    log(`created ${c.game} (seed ${c.seed})`);
    await refreshGames();
  } catch (e) { log(`create: ${e.message}`, 'err'); }
}

const tokenKey = (game, seat) => `libaw:${$('server').value.trim()}:${game}:${seat}`;
const tokenFor = (game, seat) => safeGet(tokenKey(game, seat));

async function join(game, seat) {
  resetGame();
  try {
    const token = tokenFor(game, seat);
    const j = await ui.conn.request({ type: 'join', game, seat, ...(token ? { token } : {}) });
    Object.assign(ui, { game, me: j.player, spectator: false, info: j.info });
    safeSet(tokenKey(game, seat), j.token);
    history.replaceState(null, '', `?server=${encodeURIComponent($('server').value.trim())}&game=${encodeURIComponent(game)}&seat=${seat}`);
    log(`joined ${game} as player ${j.player}`);
  } catch (e) { log(`join: ${e.message}`, 'err'); }
}

async function spectate(game) {
  resetGame();
  try {
    const j = await ui.conn.request({ type: 'spectate', game });
    Object.assign(ui, { game, me: null, spectator: true, info: j.info });
    history.replaceState(null, '', `?server=${encodeURIComponent($('server').value.trim())}&game=${encodeURIComponent(game)}&spectate=1`);
    log(`watching ${game}`);
  } catch (e) { log(`spectate: ${e.message}`, 'err'); }
}

function resetGame() {
  if (ui.game && ui.conn) ui.conn.send({ type: 'leave', game: ui.game });
  Object.assign(ui, { game: null, me: null, state: null, legal: [], selected: null, mode: null, over: null, turnPlayer: null });
  closeMenu();
}

// ---------- server messages ----------
function onMessage(m) {
  if (m.game && m.game !== ui.game) return;
  switch (m.type) {
    case 'state':
      if (PLANNER) ui.me = m.state.current;
      ui.state = m.state;
      ui.income = m.income ?? null;   // per player id, from the server (engine rule, CO bonus included)
      ui.previews = new Map();   // damage previews hold for one state only
      if (m.state.current !== ui.me) { ui.legal = []; ui.selected = null; ui.mode = null; closeMenu(); }
      render();
      break;
    case 'turn':
      ui.legal = m.legal;
      ui.turnPlayer = m.player;
      render();
      break;
    case 'event': {
      const who = m.by === ui.me && !PLANNER ? 'You' : playerName(m.by);
      log(`Day ${m.day} · ${who}: ${m.label ?? L.describe(m.action, ui.state)}`, m.by === ui.me ? 'me' : '');
      if (m.ended_turn) { const nx = nextPlayer(m.by); banner(nx === ui.me ? 'Your turn' : `${playerName(nx)}'s turn`); }
      break;
    }
    case 'game_over': {
      ui.over = m;
      const won = m.winner == null ? 'No winner' : `Team ${m.winner} wins`;
      log(`Game over: ${won} (${m.reason.replace('_', ' ')}), day ${m.day}`);
      banner(`Game over · ${won}`, 0);
      render();
      break;
    }
    case 'error':
      if (m.id == null) log(`error: ${m.message}`, 'err');
      break;
  }
}

function playerName(id) {
  const p = ui.info?.players?.find((q) => q.id === id);
  return p ? `${p.name || 'Player ' + (p.seat + 1)} (${p.co})` : `player ${id}`;
}
function nextPlayer(id) {
  const ps = ui.state?.players ?? [];
  const i = ps.findIndex((p) => p.id === id);
  return ps[(i + 1) % ps.length]?.id;
}

// ---------- rendering ----------
function order() { return (ui.state?.players ?? []).map((p) => p.id); }

function render() {
  const s = ui.state;
  $('empty').hidden = !!s;
  $('board').hidden = !s;
  if (!s) return;
  const marks = {};
  if (ui.selected != null) {
    const dests = L.movesByDest(ui.legal, ui.selected);
    marks.move = new Set(dests.keys());
    marks.attack = new Set(L.attackTargets(ui.legal, ui.selected).keys());
  }
  if (ui.mode?.kind === 'pick') marks.pick = new Set(ui.mode.tiles.keys());
  const visible = s.fog && ui.me != null
    ? L.visibleTiles(s, ui.me, ui.data.units, (t) => ui.data.terrain[t]?.name) : null;
  board.b.draw(s, { order: order(), visible, marks, selected: ui.selected });
  renderPlayers();
  const myTurn = isMyTurn();
  const cur = s.players.find((p) => p.id === s.current);
  setTurnInfo(`<b>Day ${s.day}</b> · ${myTurn ? '<b>your turn</b>' : `${esc(playerName(s.current))} to move`}${cur ? ` · ${esc(cur.co)}` : ''}${ui.spectator ? ' · watching' : ''}${ui.over ? ' · <b>game over</b>' : ''}`);
  $('btn-end').disabled = !myTurn;
  $('btn-resign').disabled = !(ui.game && ui.me != null && !ui.over);
  if (PLANNER) { $('btn-undo').disabled = !ui.conn?.canUndo; $('btn-reset').disabled = !ui.conn?.canUndo; }
  const pw = L.powersNow(ui.legal);
  $('btn-power').disabled = !myTurn || !pw.some((a) => a.power === 'Y');
  $('btn-super').disabled = !myTurn || !pw.some((a) => a.power === 'S');
}

function isMyTurn() {
  return !ui.over && ui.me != null && ui.state?.current === ui.me && ui.legal.length > 0;
}

function setTurnInfo(html) { $('turninfo').innerHTML = html; }

function renderPlayers() {
  const s = ui.state;
  $('players-card').hidden = false;
  $('info-card').hidden = false;
  $('gametitle').textContent = `Game ${ui.game}`;
  const box = $('players');
  box.replaceChildren();
  s.players.forEach((p, i) => {
    const d = document.createElement('div');
    d.className = `player p${i % 8}${p.id === s.current ? ' current' : ''}`;
    const funds = p.funds == null || (s.fog && p.id !== ui.me && ui.me != null) ? '?' : `${Number(p.funds).toLocaleString()} G`;
    const pw = p.power === 'Y' ? '<span class="badge on">COP</span>' : p.power === 'S' ? '<span class="badge on">SCOP</span>' : '';
    const props = s.buildings.filter(([, , b]) => b.owner === p.id).length;
    const units = Object.values(s.units).filter((u) => u.owner === p.id).length;
    const scop = p.scop_cost || 0, cop = p.cop_cost || 0, meter = p.meter || 0;
    d.innerHTML = `<span class="sw"></span><span class="nm">${esc(playerName(p.id))}${p.id === ui.me ? ' <span class="badge">you</span>' : ''}${pw}${p.eliminated ? ' <span class="badge">out</span>' : ''}</span><span class="funds">${funds}</span>
      <span class="sub">${props} properties${ui.income?.[p.id] != null ? ` · +${Number(ui.income[p.id]).toLocaleString()} G/day` : ''} · ${units} units · army ${L.armyValue(s, p.id, ui.data.units).toLocaleString()} G${s.fog && p.id !== ui.me ? ' (seen)' : ''}${s.powers ? ` · meter ${meter.toLocaleString()}${scop ? ' / ' + scop.toLocaleString() : ''}` : ''}</span>
      ${s.powers && scop ? `<span class="meter"><i style="width:${Math.min(100, (100 * meter) / scop)}%"></i>${cop ? `<b style="left:${(100 * cop) / scop}%"></b>` : ''}</span>` : ''}`;
    box.append(d);
  });
  const wx = { C: 'Clear', R: 'Rain', S: 'Snow' }[s.weather] ?? s.weather;
  $('weather').textContent = `Weather: ${wx}${s.fog ? ' · Fog of war' : ''}${s.powers ? ' · CO powers on' : ''}`;
}

function showInfo(t) {
  const s = ui.state;
  if (!s || !t) { $('info').innerHTML = 'Hover a tile.'; return; }
  const [x, y] = t;
  const tid = L.terrainAt(s, x, y);
  const ter = ui.data.terrain[tid] ?? { name: `terrain ${tid}` };
  const b = L.buildingsByTile(s).get(L.key(x, y));
  const u = L.unitsByTile(s).get(L.key(x, y));
  let html = `<div class="title">${esc(ter.name)} <span class="muted">(${x}, ${y})</span></div><div class="kv"><span>Defense</span><span>${'★'.repeat(ter.defense ?? 0) || '–'}</span>`;
  if (b) html += `<span>Owner</span><span>${b.owner == null ? 'neutral' : esc(playerName(b.owner))}</span><span>Capture</span><span>${b.capture} / 20</span>`;
  html += '</div>';
  if (u) {
    const info = ui.data.units[u.kind] ?? {};
    html += `<div class="title" style="margin-top:8px">${esc(u.kind)} <span class="muted">· ${esc(playerName(u.owner))}</span></div><div class="kv">
      <span>HP</span><span>${L.displayHp(u)} / 10 <span class="muted">(${u.hp.lo === u.hp.hi ? u.hp.hi : u.hp.lo + '–' + u.hp.hi})</span></span>
      <span>Fuel</span><span>${u.fuel} / ${info.fuel ?? '?'}</span>
      <span>Ammo</span><span>${info.ammo ? `${u.ammo} / ${info.ammo}` : '–'}</span>
      <span>Move</span><span>${info.move ?? '?'} · ${info.move_type ?? ''}</span>
      <span>Range</span><span>${info.range ? info.range.join('–') : '?'}</span>
      <span>Vision</span><span>${info.vision ?? '?'}</span>
      ${u.cargo?.length ? `<span>Carrying</span><span>${u.cargo.map((c) => esc(s.units[c]?.kind ?? c)).join(', ')}</span>` : ''}
      ${u.moved ? '<span>Status</span><span>moved</span>' : ''}</div>`;
  }
  $('info').innerHTML = html;
  hoverPreview(x, y);
}

/** AWBW shows the damage % while the cursor is on a target: with a unit selected and this tile
 * one of its targets, ask the server's `preview` for the shortest-path attack (once per state)
 * and add the range to the tile panel, if the cursor is still there when it answers. */
async function hoverPreview(x, y) {
  if (ui.selected == null || !isMyTurn() || !ui.conn) return;
  const k = L.key(x, y);
  const acts = L.attackTargets(ui.legal, ui.selected).get(k);
  if (!acts) return;
  const a = [...acts].sort((p, q) => p.path.length - q.path.length)[0];
  const ck = `${ui.selected}:${k}`;
  ui.previews ??= new Map();
  if (!ui.previews.has(ck)) {
    ui.previews.set(ck, ui.conn.request({ type: 'preview', game: ui.game, action: a })
      .then((p) => (p.type === 'preview' ? L.damageLabel(p) : null)).catch(() => null));
  }
  const label = await ui.previews.get(ck);
  if (label && board.b.hoverKey === k) {
    $('info').insertAdjacentHTML('afterbegin', `<div class="title preview">Attack: ${esc(label)}</div>`);
  }
}

// ---------- play ----------
function onTile(x, y) {
  if (!ui.state) return;
  const k = L.key(x, y);
  if (ui.mode?.kind === 'pick') {
    const acts = ui.mode.tiles.get(k);
    if (acts) { chooseAmong(acts, x, y); return; }
    ui.mode = null; render(); return;
  }
  closeMenu();
  if (!isMyTurn()) { ui.selected = null; render(); return; }
  const unit = L.unitsByTile(ui.state).get(k);
  if (ui.selected != null) {
    const dests = L.movesByDest(ui.legal, ui.selected);
    if (dests.has(k)) { openMoveMenu(dests.get(k), x, y); return; }
    // Fire straight from where the unit stands or can reach: click the target.
    const targets = L.attackTargets(ui.legal, ui.selected);
    if (targets.has(k)) { chooseAmong(targets.get(k), x, y); return; }
  }
  if (unit && unit.owner === ui.me && L.movableUnits(ui.legal).has(unit.id)) {
    ui.selected = ui.selected === unit.id ? null : unit.id;
    render();
    return;
  }
  ui.selected = null;
  const builds = L.buildsAt(ui.legal, x, y);
  if (builds.length) { openBuildMenu(builds, x, y); render(); return; }
  render();
}

/** Several legal actions point at one picked tile (e.g. firing from different squares). */
function chooseAmong(acts, x, y) {
  ui.mode = null;
  if (acts.length === 1) { confirmFire(acts[0], x, y); return; }
  // Prefer the shortest path (fire from where it stands when it can), else let the player choose.
  const short = [...acts].sort((a, b) => a.path.length - b.path.length);
  if (short[0].path.length === 1 || short.length <= 1) { confirmFire(short[0], x, y); return; }
  openMenu(x, y, short.map((a) => ({ label: `From ${a.path[a.path.length - 1].join(',')}`, run: () => confirmFire(a, x, y) })), 'Fire from');
}

/** An attack shows its damage range first (the server's `preview`: the move played on a copy at
 * the lowest and highest luck); anything else, or a preview that fails, goes straight through. */
async function confirmFire(a, x, y) {
  if (a.then?.type !== 'attack') { act(a); return; }
  let label = 'Fire';
  try {
    const p = await ui.conn.request({ type: 'preview', game: ui.game, action: a });
    if (p.type === 'preview') label = `Fire: ${L.damageLabel(p)}`;
  } catch { /* an older server without preview: fire without it */ }
  openMenu(x, y, [{ label, run: () => act(a) }, { label: 'Cancel', run: () => { ui.mode = null; render(); } }], 'Attack');
}

function openMoveMenu(acts, x, y) {
  const items = L.menuAt(acts).map((entry) => ({
    label: entry.label,
    run: () => {
      const needsPick = ['attack', 'unload', 'repair', 'launch', 'attack_seam'].includes(entry.kind);
      if (!needsPick) { act(entry.actions[0]); return; }
      if (entry.kind === 'unload') {
        const byCargo = new Map();
        for (const a of entry.actions) {
          if (!byCargo.has(a.then.cargo_id)) byCargo.set(a.then.cargo_id, []);
          byCargo.get(a.then.cargo_id).push(a);
        }
        if (byCargo.size > 1) {
          openMenu(x, y, [...byCargo].map(([cid, as]) => ({ label: ui.state.units[cid]?.kind ?? `unit ${cid}`, run: () => startPick(as) })), 'Unload');
          return;
        }
      }
      startPick(entry.actions);
    },
  }));
  openMenu(x, y, items);
}

function startPick(actions) {
  const tiles = new Map();
  for (const a of actions) {
    const t = L.pickTile(a);
    const k = L.key(t[0], t[1]);
    if (!tiles.has(k)) tiles.set(k, []);
    tiles.get(k).push(a);
  }
  ui.mode = { kind: 'pick', tiles };
  closeMenu();
  render();
}

function openBuildMenu(builds, x, y) {
  const items = builds
    .map((a) => ({ a, cost: ui.data.units[a.unit]?.cost ?? 0 }))
    .sort((p, q) => p.cost - q.cost)
    .map(({ a, cost }) => ({ label: a.unit, note: `${cost.toLocaleString()} G`, run: () => act(a) }));
  openMenu(x, y, items, 'Build');
}

function openMenu(x, y, items, head) {
  const m = $('menu');
  m.replaceChildren();
  if (head) { const h = document.createElement('div'); h.className = 'hd'; h.textContent = head; m.append(h); }
  for (const it of items) {
    const b = document.createElement('button');
    b.innerHTML = `<span>${esc(it.label)}</span>${it.note ? `<span class="cost">${esc(it.note)}</span>` : ''}`;
    b.onclick = (e) => { e.stopPropagation(); closeMenu(); it.run(); };
    m.append(b);
  }
  const svg = $('board'), wrap = $('boardwrap');
  const r = svg.getBoundingClientRect(), wr = wrap.getBoundingClientRect();
  const scale = r.width / (ui.state.width * TILE);
  m.style.left = `${r.left - wr.left + wrap.scrollLeft + (x + 1) * TILE * scale + 6}px`;
  m.style.top = `${r.top - wr.top + wrap.scrollTop + y * TILE * scale}px`;
  m.hidden = false;
}

function closeMenu() { $('menu').hidden = true; }

async function act(action) {
  ui.selected = null;
  ui.mode = null;
  closeMenu();
  try {
    await ui.conn.request({ type: 'act', game: ui.game, action });
  } catch (e) {
    log(`not allowed: ${e.message}`, 'err');
    render();
  }
}

function banner(text, ms = 1600) {
  const b = $('banner');
  b.textContent = text;
  b.hidden = false;
  clearTimeout(banner.t);
  if (ms) banner.t = setTimeout(() => { b.hidden = true; }, ms);
}

function esc(s) { return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])); }

// ---------- planner ----------
async function plannerLoad() {
  let turn;
  try {
    const f = $('turnfile').files?.[0];
    turn = f ? JSON.parse(await f.text()) : await fetch(STATES[Number($('newstate').value)].path).then((r) => r.json());
  } catch (e) { log(`could not read the position: ${e.message}`, 'err'); return; }
  resetGame();
  $('log').replaceChildren();
  ui.conn = await new LocalConn().open();
  ui.conn.onMessage = onMessage;
  try {
    const c = await ui.conn.request({ type: 'create', turn, luck: $('luck').value, name: 'plan' });
    const j = await ui.conn.request({ type: 'join', game: c.game });
    Object.assign(ui, { game: c.game, me: j.player, spectator: false, info: j.info });
    log(`loaded: day ${ui.conn.game.state.day}, ${playerName(ui.conn.game.state.current)} to move, luck ${$('luck').value}`);
  } catch (e) { log(`load: ${e.message}`, 'err'); }
}

async function plannerRequest(type) {
  if (!ui.conn) return;
  ui.selected = null; ui.mode = null; ui.over = null; closeMenu();
  try { await ui.conn.request({ type, game: ui.game }); log(type === 'undo' ? 'undone' : 'back to the loaded position'); }
  catch (e) { log(e.message, 'err'); }
}

function setupPlanner() {
  $('btn-create').onclick = plannerLoad;
  $('btn-undo').onclick = () => plannerRequest('undo');
  $('btn-reset').onclick = () => plannerRequest('reset');
  $('luck').onchange = () => { ui.conn?.request({ type: 'luck', game: ui.game, luck: $('luck').value }).catch(() => {}); log(`luck: ${$('luck').value}`); };
  document.addEventListener('keydown', (e) => {
    if (e.target.matches('input, select') || e.metaKey || e.ctrlKey) return;
    if (e.key === 'u' || e.key === 'U') plannerRequest('undo');
  });
}

// ---------- wire up ----------
async function main() {
  setupTheme();
  await loadData();
  board.b = new Board($('board'), ui.data.tmap);
  board.b.onTile = onTile;
  board.b.onHover = showInfo;
  STATES.forEach((s, i) => { const o = document.createElement('option'); o.value = i; o.textContent = s.label; $('newstate').append(o); });
  if (PLANNER) setupPlanner();
  else {
    if (params.get('server')) $('server').value = params.get('server');
    $('btn-connect').onclick = connect;
    $('btn-create').onclick = createGame;
    $('btn-refresh').onclick = refreshGames;
    $('btn-leave').onclick = () => { resetGame(); $('players-card').hidden = true; render(); refreshGames(); };
  }
  $('btn-end').onclick = () => act({ type: 'end_turn' });
  // Resigning is final: ask first. The server answers with game_over to every seat and spectator.
  $('btn-resign').onclick = async () => {
    if (!ui.game || !window.confirm('Resign this game? This cannot be undone.')) return;
    try { await ui.conn.request({ type: 'resign', game: ui.game }); } catch (e) { console.warn('resign failed', e); }
  };
  $('btn-power').onclick = () => act({ type: 'power', power: 'Y' });
  $('btn-super').onclick = () => act({ type: 'power', power: 'S' });
  document.addEventListener('keydown', (e) => {
    if (e.target.matches('input, select')) return;
    if (e.key === 'Escape') { ui.selected = null; ui.mode = null; closeMenu(); render(); }
    if ((e.key === 'e' || e.key === 'E') && isMyTurn() && !e.metaKey && !e.ctrlKey) act({ type: 'end_turn' });
  });
  if (!PLANNER && params.get('game')) {
    if (await connect()) {
      if (params.get('spectate')) await spectate(params.get('game'));
      else await join(params.get('game'), Number(params.get('seat') ?? 0));
    }
  }
}
main().catch((e) => log(`failed to start: ${e.message}`, 'err'));
