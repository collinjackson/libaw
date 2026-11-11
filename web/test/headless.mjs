// End-to-end check of the web client's modules against a running libaw-server: creates a game
// on the skirmish map, seats two clients built from web/js (net.js + logic.js), plays it
// with logic.simplePick until it ends or the action cap, and checks the client-side helpers
// (movesByDest, menuAt, buildsAt) agree with every legal list along the way. Node 22+.
//
//   node web/test/headless.mjs ws://127.0.0.1:8765 [MAX_DAYS] [SEED]
import { readFileSync } from 'node:fs';
import { Conn } from '../js/net.js';
import * as L from '../js/logic.js';

const [url = 'ws://127.0.0.1:8765', maxDays = '8', seed = '7'] = process.argv.slice(2);
const turnFile = JSON.parse(readFileSync(new URL('../states/skirmish.json', import.meta.url)));
const fail = (msg) => { console.error('FAIL:', msg); process.exit(1); };

const host = await new Conn(url).open();
const created = await host.request({ type: 'create', turn: turnFile, max_days: Number(maxDays), seed: Number(seed) });
const game = created.game;
console.log(`game ${game}: ${created.players.map((p) => `${p.id} ${p.co}`).join(' vs ')}`);

let actions = 0, checks = 0;
const done = new Promise((resolve) => {
  const seat = async (i) => {
    const c = await new Conn(url).open();
    let state = null;
    const acted = new Set();
    c.onMessage = (m) => {
      if (m.type === 'state') state = m.state;
      if (m.type === 'game_over') { resolve(m); }
      if (m.type === 'turn') {
        // The client helpers must cover every legal action exactly once.
        const movers = L.movableUnits(m.legal);
        let n = 0;
        for (const id of movers) for (const acts of L.movesByDest(m.legal, id).values()) {
          n += acts.length;
          const menu = L.menuAt(acts);
          if (!menu.length || menu.reduce((s, e) => s + e.actions.length, 0) !== acts.length) fail('menuAt lost actions');
        }
        const builds = m.legal.filter((a) => a.type === 'build');
        for (const b of builds) if (!L.buildsAt(m.legal, ...b.at).includes(b)) fail('buildsAt missed a build');
        n += builds.length + L.powersNow(m.legal).length + m.legal.filter((a) => a.type === 'end_turn').length;
        if (n !== m.legal.length) fail(`helpers cover ${n} of ${m.legal.length} legal actions`);
        checks++;
        if (actions++ > 600) { c.send({ type: 'resign', game }); return; }
        const a = L.simplePick(m.legal, acted);
        if (a.type === 'move') acted.add(a.unit_id);
        if (a.type === 'end_turn') acted.clear();
        L.describe(a, state);
        c.send({ type: 'act', game, action: a });
      }
      if (m.type === 'error') fail(`server error: ${m.message}`);
    };
    const j = await c.request({ type: 'join', game, seat: i });
    console.log(`seat ${i}: player ${j.player}`);
  };
  seat(0); seat(1);
});
const over = await done;
console.log(`game over: ${over.reason}, winner ${over.winner}, day ${over.day}; ${actions} actions, ${checks} legal lists checked`);
if (checks < 10) fail('too few turns played');
console.log('ok');
process.exit(0);
