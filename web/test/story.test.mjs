// node web/test/story.test.mjs: the storyboard helpers on a small made-up turn.
import assert from 'assert/strict';
import * as S from '../js/story.js';

const unit = (id, owner, kind, x, y, hi = 100) => ({ id, owner, kind, x, y, hp: { lo: hi - 9, hi }, fuel: 99, ammo: 9, moved: false, carried_by: null, cargo: [], dived: false });
const sb = {
  format: 'libaw-storyboard', version: 1, day: 3, player: 1, names: { 1: 'Red', 2: 'Blue' },
  start: { width: 8, height: 6, terrain: Array(48).fill(1), players: [{ id: 1 }, { id: 2 }],
    buildings: [[5, 5, { terrain: 34, owner: null, capture: 20 }]],
    units: { 7: unit(7, 1, 'Tank', 1, 1), 8: unit(8, 2, 'Infantry', 3, 1), 9: unit(9, 1, 'Infantry', 5, 4) } },
  steps: [
    { type: 'move', unit_id: 7, path: [[1, 1], [2, 1]] },
    { type: 'fire', attacker: { unit_id: 7, at: [2, 1], hp: { lo: 91, hi: 100 }, display_hp: 10 }, defender: { unit_id: 8, at: [3, 1], hp: { lo: 0, hi: 0 }, display_hp: 0 } },
    { type: 'move', unit_id: 9, path: [[5, 4], [5, 5]] },
    { type: 'capture', at: [5, 5], capture: 10, done: false, new_owner: null },
    { type: 'build', unit: unit(10, 1, 'Mech', 0, 0) },
    { type: 'end', next: 2 },
  ],
};
const data = { units: { Tank: { cost: 7000 }, Infantry: { cost: 1000 }, Mech: { cost: 3000 } }, terrain: { 34: { name: 'City' } } };

const st = S.statesOf(sb);
assert.equal(st.length, sb.steps.length + 1);
assert.deepEqual([st[1].units[7].x, st[1].units[7].y], [2, 1]);
assert.equal(st[2].units[8], undefined, 'a 0 HP defender is gone');
assert.equal(st[4].buildings[0][2].capture, 10);
assert.equal(st[5].units[10].kind, 'Mech');
assert.equal(sb.start.units[7].x, 1, 'applyStep leaves its input alone');

const shots = S.shots(sb, data);
assert.deepEqual(shots.map((s) => s.kind), ['fire', 'capture', 'build']);
assert.equal(shots[0].score, 1000 * 10 / 10 * 1.5, 'a kill counts 1.5x its value');
const hl = S.highlights(sb, data, 2);
assert.deepEqual(hl.map((s) => s.kind), ['fire', 'capture'], 'top 2, in the order they happened');

const tl = S.timeline(sb, hl);
assert.ok(tl.duration > 2 && tl.duration < 5);
assert.equal(S.segmentAt(tl, 0).seg.phase, 'move');
assert.equal(S.segmentAt(tl, tl.duration).seg.phase, 'hold');
assert.deepEqual(S.alongPath([[0, 0], [2, 0], [2, 2]], 0.5), [2, 0]);
console.log('story ok');
