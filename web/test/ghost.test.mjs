// node web/test/ghost.test.mjs: the replay ghost's plan (web/js/ghost.js), the new step types and
// remapping a time when the timeline changes.
import assert from 'assert/strict';
import * as S from '../js/story.js';
import * as G from '../js/ghost.js';

const unit = (id, owner, kind, x, y, extra = {}) => ({ id, owner, kind, x, y, hp: { lo: 91, hi: 100 }, fuel: 99, ammo: 9, moved: false, carried_by: null, cargo: [], dived: false, ...extra });
const sb = {
  format: 'libaw-storyboard', version: 1, day: 3, player: 1,
  start: { width: 10, height: 8, terrain: Array(80).fill(1), players: [{ id: 1, funds: 9000 }, { id: 2 }],
    buildings: [[0, 0, { terrain: 39, owner: 1, capture: 20 }], [6, 6, { terrain: 111, owner: null, capture: 20 }]],
    units: { 7: unit(7, 1, 'Tank', 1, 1), 8: unit(8, 2, 'Infantry', 3, 1), 20: unit(20, 1, 'APC', 5, 3, { cargo: [21] }),
      21: unit(21, 1, 'Infantry', 5, 3, { carried_by: 20 }), 30: unit(30, 1, 'Black Bomb', 8, 1), 40: unit(40, 1, 'Mech', 6, 6),
      9: unit(9, 2, 'Tank', 7, 2) } },
  steps: [
    { type: 'move', unit_id: 7, path: [[1, 1], [2, 1]] },
    { type: 'fire', attacker: { unit_id: 7, at: [2, 1], hp: { lo: 91, hi: 100 }, display_hp: 10 }, defender: { unit_id: 8, at: [3, 1], hp: { lo: 41, hi: 50 }, display_hp: 5 } },
    { type: 'move', unit_id: 20, path: [[5, 3], [5, 4]] },
    { type: 'unload', transport_id: 20, unit_id: 21, to: [5, 5] },
    { type: 'move', unit_id: 30, path: [[8, 1]] },
    { type: 'explode', unit_id: 30, at: [8, 1], damage: 5 },
    { type: 'move', unit_id: 40, path: [[6, 6]] },
    { type: 'launch', silo: [6, 6], target: [7, 2], damage: 3 },
    { type: 'build', unit: unit(50, 1, 'Infantry', 0, 0) },
    { type: 'end', next: 2 },
  ],
};

// The new steps on the board.
const st = S.statesOf(sb);
assert.equal(st[4].units[21].carried_by, null, 'unloaded');
assert.deepEqual([st[4].units[21].x, st[4].units[21].y], [5, 5]);
assert.deepEqual(st[4].units[20].cargo, []);
assert.equal(st[6].units[30], undefined, 'the bomb goes');
assert.equal(st[6].units[9].hp.hi, 50, 'a Black Bomb takes 5 HP within 3 tiles');
assert.equal(st[8].units[9].hp.hi, 20, 'a silo 3 within 2 of its target');
assert.equal(st[8].buildings[1][2].terrain, 112, 'the silo is spent');
assert.equal(S.applyStep(st[6], { type: 'explode', unit_id: 0, at: [7, 2], damage: 9 }).units[9].hp.hi, 1, 'area damage never kills');
assert.deepEqual(S.shots(sb).map((s) => s.kind), ['fire', 'unload', 'explode', 'launch', 'build']);
console.log('steps ok');

// Menus: AWBW's order, a Load or Join alone, no Unload after a move.
assert.deepEqual(G.unitMenu([{ type: 'wait' }, { type: 'attack', target: [1, 1] }, { type: 'capture' }], 'Capt', { moved: true }), ['Fire', 'Capt', 'Wait']);
assert.deepEqual(G.unitMenu([{ type: 'wait' }, { type: 'unload', cargo_id: 1, to: [0, 0] }], 'Wait', { moved: true }), ['Wait']);
assert.deepEqual(G.unitMenu([{ type: 'wait' }, { type: 'unload', cargo_id: 1, to: [0, 0] }], 'Unload', { moved: false }), ['Unload', 'Delete', 'Wait']);
assert.deepEqual(G.unitMenu(null, 'Load'), ['Load']);
assert.deepEqual(G.unitMenu(null, 'Explode', { moved: true, unit: { kind: 'Black Bomb' } }), ['Explode', 'Wait']);
console.log('menus ok');

// The plan: each step's segments, in order, with the cursor where the player's would be.
const plan = G.ghostPlan(sb, st);
const phases = (i) => [...(plan.pre[i] ?? []).map((x) => x.phase), '|', ...(plan.post[i] ?? []).map((x) => x.phase)].join(' ');
assert.equal(phases(0), 'aim | menu', 'the first move: no glide in, the cursor just appears');
assert.equal(plan.post[0][0].menu.chosen, 'Fire');
assert.equal(phases(1), 'target |', 'the attack follows its move: the menu was already up');
assert.deepEqual([plan.pre[1][0].from, plan.pre[1][0].to], [[2, 1], [3, 1]], 'the cursor glides to the target');
assert.equal(plan.post[2][0].menu.chosen, 'Wait', "a transport that moves waits; it's clicked again to unload");
assert.equal(phases(3), 'select menu menu drop |');
assert.deepEqual(plan.pre[3][1].menu.options, ['Unload'], 'a moved transport offers Unload alone');
assert.deepEqual(plan.pre[3][2].menu.options, ['Infantry'], 'then its cargo');
assert.deepEqual(plan.pre[3][3].to, [5, 5], 'then the landing tile');
assert.equal(phases(4), 'hop select | menu', 'a unit acting in place is selected, not aimed');
assert.equal(plan.post[4][0].menu.chosen, 'Explode');
assert.equal(phases(5), 'blast |');
assert.equal(plan.pre[5][0].area.length, 4 + 5 + 4 + 3 + 1, 'the blast area, cut at the board edge');
assert.equal(phases(7), 'silo menu |');
assert.deepEqual(plan.pre[7][1].menu.options, ['Fire Silo']);
assert.equal(phases(8), 'hop select menu |');
assert.equal(plan.pre[8][2].menu.kind, 'build');
// The cursor is continuous: each glide starts where the last segment left it.
let at = null;
sb.steps.forEach((step, i) => {
  for (const seg of [...(plan.pre[i] ?? []), ...(plan.post[i] ?? [])]) {
    if (seg.from && at) assert.deepEqual(seg.from, at, `step ${i} ${seg.phase} starts where the cursor is`);
    at = G.ghostRest(seg, step) ?? at;
  }
});
// Its position mid-glide.
assert.deepEqual(G.ghostCursor({ phase: 'target', dur: 1, glide: 0.5, from: [0, 0], to: [2, 0], press: 0.1 }, 0.25, null, S.alongPath).at, [1, 0]);
assert.equal(G.ghostCursor({ phase: 'target', dur: 1, glide: 0.5, from: [0, 0], to: [2, 0], press: 0.1 }, 0.95, null, S.alongPath).press, true);
console.log('plan ok');

