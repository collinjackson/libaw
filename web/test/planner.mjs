// The planner's in-page engine (js/local.js) under Node: load a mid-game position, fire with best
// and worst luck, check the previews bracket the results, undo and reset.
//   node web/test/planner.mjs        (after wasm-pack build crates/wasm --target web --out-dir ../../web/pkg)
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { LocalConn } from '../js/local.js';

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const turn = JSON.parse(readFileSync(here('../../docs/eval-harness/test-states/game-1729758-turn-042-D22-pid4151012.json'), 'utf8'));
const c = await new LocalConn().open(readFileSync(here('../pkg/awbw_wasm_bg.wasm')));
const seen = [];
c.onMessage = (m) => seen.push(m);
const tick = () => new Promise((r) => setTimeout(r, 0));

await c.request({ type: 'create', turn, luck: 'best' });
const j = await c.request({ type: 'join', game: 'plan' });
await tick();
const turnMsg = seen.findLast((m) => m.type === 'turn');
assert.equal(turnMsg.player, j.player);
const attack = turnMsg.legal.find((a) => a.then?.type === 'attack');
assert.ok(attack, 'this position has an attack');
const p = await c.request({ type: 'preview', game: 'plan', action: attack });
const st0 = seen.findLast((m) => m.type === 'state').state;

const defHp = async (luck) => {
  await c.request({ type: 'luck', game: 'plan', luck });
  await c.request({ type: 'act', game: 'plan', action: attack });
  await tick();
  const s = seen.findLast((m) => m.type === 'state').state;
  const d = Object.values(s.units).find((u) => u.x === attack.then.target[0] && u.y === attack.then.target[1]);
  await c.request({ type: 'undo', game: 'plan' });
  return d ? d.hp.hi : 0;
};
const best = await defHp('best'), worst = await defHp('worst');
assert.equal(best, p.high_luck.defender_hp, 'best luck = the preview high');
assert.equal(worst, p.low_luck.defender_hp, 'worst luck = the preview low');
assert.ok(best <= worst);
await tick();
assert.deepEqual(seen.findLast((m) => m.type === 'state').state, st0, 'undo restores the position');

await c.request({ type: 'act', game: 'plan', action: { type: 'end_turn' } });
await tick();
assert.notEqual(seen.findLast((m) => m.type === 'state').state.current, st0.current, 'the other side moves next');
await c.request({ type: 'reset', game: 'plan' });
await tick();
assert.deepEqual(seen.findLast((m) => m.type === 'state').state, st0);
assert.ok(!c.canUndo);
console.log(`planner ok: attack ${JSON.stringify(attack.then)} leaves the defender at ${best}-${worst} (preview ${p.high_luck.defender_hp}-${p.low_luck.defender_hp})`);
