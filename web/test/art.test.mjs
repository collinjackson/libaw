// node web/test/art.test.mjs: AWBW image names for tiles.
import assert from 'assert/strict';
import { awbwTileName } from '../js/art.js';

assert.equal(awbwTileName(1, { name: 'Plain' }), 'plain');
assert.equal(awbwTileName(18, { name: 'ESRoad', kind: 'road' }), 'esroad');
assert.equal(awbwTileName(112, { name: 'Missile Silo Empty' }), 'missilesiloempty');
assert.equal(awbwTileName(30, { name: 'Shoal' }), 'hshoaln');
assert.equal(awbwTileName(34, { name: 'Neutral City', building: 'city' }, null), 'neutralcity');
// A captured tower takes its new owner's colours, whatever the tile id says.
assert.equal(awbwTileName(133, { name: 'Neutral Com Tower', building: 'com-tower' }, 'bh'), 'blackholecomtower');
assert.equal(awbwTileName(42, { name: 'Orange Star HQ', building: 'hq' }, 'os'), 'orangestarhq');
assert.equal(awbwTileName(34, { building: 'city' }, 'zz'), null);
console.log('art ok');
