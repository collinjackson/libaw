// node web/test/logic.test.mjs: terrain lookups.
import assert from 'assert/strict';
import { terrainAt, displayHp, hpUnknown } from '../js/logic.js';

// AWBW's turn files list buildings apart from the terrain grid, so the grid can hold a plain (1)
// under a building: the building's own tile (38, an Orange Star city) must win, or the board
// draws no buildings at all.
const state = { width: 2, height: 1, terrain: [1, 3], buildings: [[0, 0, { terrain: 38, owner: 7, capture: 20 }]] };
assert.equal(terrainAt(state, 0, 0), 38);
assert.equal(terrainAt(state, 1, 0), 3);
// A capture changes the building's tile; a new buildings list is read afresh.
const after = { ...state, buildings: [[0, 0, { terrain: 39, owner: 8, capture: 20 }]] };
assert.equal(terrainAt(after, 0, 0), 39);
// States without buildings still work.
assert.equal(terrainAt({ width: 1, height: 1, terrain: [5] }, 0, 0), 5);
// Hidden HP ("?", Sonja) is any value from 1 to 100: unknown, though valued as a whole unit.
assert.equal(hpUnknown({ hp: { lo: 1, hi: 100 } }), true);
assert.equal(hpUnknown({ hp: { lo: 91, hi: 100 } }), false);
assert.equal(hpUnknown({ hp: { lo: 1, hi: 10 } }), false);
assert.equal(displayHp({ hp: { lo: 1, hi: 100 } }), 10);
console.log('logic ok');
