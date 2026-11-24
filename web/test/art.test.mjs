// node web/test/art.test.mjs: AWBW image names for tiles, and which way units face.
import assert from 'assert/strict';
import { awbwTileName, awbwFacing, awbwArt } from '../js/art.js';

assert.equal(awbwTileName(1, { name: 'Plain' }), 'plain');
assert.equal(awbwTileName(18, { name: 'ESRoad', kind: 'road' }), 'esroad');
assert.equal(awbwTileName(113, { name: 'Pipe Seam' }), 'hpipeseam');   // AWBW has no pipeseam.gif
assert.equal(awbwTileName(114, { name: 'Pipe Seam' }), 'vpipeseam');
assert.equal(awbwTileName(112, { name: 'Missile Silo Empty' }), 'missilesiloempty');
assert.equal(awbwTileName(30, { name: 'Shoal' }), 'hshoaln');
assert.equal(awbwTileName(195, { name: 'Teleport Tile' }), 'teleporter');   // AWBW has no teleporttile.gif
// A known-missing image goes straight to libaw's art (no document needed: nothing is drawn).
{
  const art = awbwArt({ 1: 'os' }, { missing: new Set(['teleporter', 'osmegatank']) });
  assert.equal(art.tile(195, { name: 'Teleport Tile' }, null, null, { x: 0, y: 0, size: 16 }), false);
  assert.equal(art.unit({ owner: 1, kind: 'Mega Tank' }, null, { x: 0, y: 0, size: 16 }), false);
}
assert.equal(awbwTileName(34, { name: 'Neutral City', building: 'city' }, null), 'neutralcity');
// A captured tower takes its new owner's colours, whatever the tile id says.
assert.equal(awbwTileName(133, { name: 'Neutral Com Tower', building: 'com-tower' }, 'bh'), 'blackholecomtower');
assert.equal(awbwTileName(42, { name: 'Orange Star HQ', building: 'hq' }, 'os'), 'orangestarhq');
assert.equal(awbwTileName(34, { building: 'city' }, 'zz'), null);
// Orange Star faces right and Blue Moon left, as on AWBW's own board.
assert.equal(awbwFacing('os'), 'right');
assert.equal(awbwFacing('bm'), 'left');
const art = awbwArt({ 1: 'ge', 2: 'yc' });
assert.equal(art.facing({ owner: 1 }), 'right');
assert.equal(art.facing({ owner: 2 }), 'left');
assert.equal(art.facing({ owner: 3 }), null);
console.log('art ok');
