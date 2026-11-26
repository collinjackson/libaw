// node web/test/art.test.mjs: AWBW image names for tiles, and which way units face.
import assert from 'assert/strict';
import { awbwTileName, awbwFacing, awbwArt, awbwWaterName } from '../js/art.js';

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

// Joined seas and shoals, AWBW's names. A map:  P = plain (1), S = sea (28), H = shoal (29).
{
  const grid = ['PPPP', 'PHHP', 'SSSS', 'SSSS'];
  const ids = { P: 1, S: 28, H: 29 };
  const idAt = (x, y) => (grid[y]?.[x] ? ids[grid[y][x]] : undefined);
  assert.equal(awbwWaterName(idAt, 0, 0), null);
  assert.equal(awbwWaterName(idAt, 1, 3), 'sea0');        // open sea all round (the edge is water)
  // Shoal at (1,1): up land 2, left land 2, right shoal 1, down sea 0 -> 2 + 2*3 + 1*9 + 0 = 17.
  assert.equal(awbwWaterName(idAt, 1, 1), 'shoal17');
  // Sea at (0,2): left is off the map (water), up is plain (bit 1), up-right is shoal (water).
  assert.equal(awbwWaterName(idAt, 0, 2), 'sea2');
  // Sea at (3,2): up plain (bit 1), up-left shoal (water), up-right off map; corners cleared by sides.
  assert.equal(awbwWaterName(idAt, 3, 2), 'sea2');
}
// A name in the sheet's table is drawn as that region of the sheet: an inner <svg> whose viewBox
// is the tile's rectangle, holding the whole sheet image (no CORS needed; nothing is read back).
{
  const made = [];
  const node = (tag) => { const n = { tag, attrs: {}, kids: [], style: {}, setAttribute(k, v) { this.attrs[k] = String(v); },
    appendChild(c) { this.kids.push(c); }, addEventListener() {}, replaceWith() {} }; made.push(n); return n; };
  globalThis.document = { createElementNS: (_, tag) => node(tag) };
  const sheet = { url: 'https://example.test/sheet.png', width: 64, height: 48, tiles: { sea0: [32, 16, 16, 16] } };
  const art = awbwArt({}, { sheet, nameAt: () => 'sea0' });
  const parent = node('g');
  assert.equal(art.tile(28, { name: 'Sea' }, null, parent, { x: 32, y: 48, size: 16 }), true);
  const [box] = parent.kids;
  assert.equal(box.tag, 'svg');
  assert.deepEqual([box.attrs.x, box.attrs.y, box.attrs.width, box.attrs.viewBox], ['32', '48', '16', '32 16 16 16']);
  assert.deepEqual([box.kids[0].tag, box.kids[0].attrs.href, box.kids[0].attrs.width], ['image', sheet.url, '64']);
  delete globalThis.document;
}
console.log('art sheet ok');
