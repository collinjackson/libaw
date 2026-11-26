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
// Unit status badges: capturing beats loaded beats hidden beats stunned; AWBW's icons for each.
{
  const { unitStatus } = await import('../js/board.js');
  const inf = { kind: 'Infantry', owner: 1 };
  assert.equal(unitStatus(inf, { capture: 12, owner: 2 }), 'capture');
  assert.equal(unitStatus(inf, { capture: 12, owner: null }), 'capture');
  assert.equal(unitStatus(inf, { capture: 20, owner: 2 }), null);           // not started
  assert.equal(unitStatus(inf, { capture: 12, owner: 1 }), null);           // its own building
  assert.equal(unitStatus({ kind: 'Tank', owner: 1 }, { capture: 12, owner: 2 }), null);
  assert.equal(unitStatus({ kind: 'APC', owner: 1, cargo: [7] }, null), 'load');
  assert.equal(unitStatus({ kind: 'Sub', owner: 1, dived: true }, null), 'hide');
  assert.equal(unitStatus({ kind: 'Tank', owner: 1, stunned: true }, null), 'stun');
  const made = [];
  globalThis.document = { createElementNS: (_, tag) => { const n = { tag, attrs: {}, style: {}, setAttribute(k, v) { this.attrs[k] = String(v); }, addEventListener() {} }; made.push(n); return n; } };
  const kids = [];
  assert.equal(awbwArt({}).badge('hide', { appendChild: (c) => kids.push(c) }, { x: 40, y: 80, size: 40 }), true);
  assert.deepEqual([kids[0].attrs.href, kids[0].attrs.x, kids[0].attrs.y, kids[0].attrs.width],
    ['https://awbw.amarriner.com/terrain/aw1/subdive2.gif', '40', '100', '20']);
  delete globalThis.document;
}
console.log('art badges ok');
// Overhang: a sprite taller than wide stands above its tile, once measured; sheet tiles never do.
{
  const loaded = [];
  globalThis.Image = class { set src(u) { loaded.push(u); this.naturalWidth = 16; this.naturalHeight = u.includes('hq') ? 31 : 16; queueMicrotask(() => this.onload()); } };
  const art = awbwArt({ 1: 'os' });
  const hq = [42, { building: 'hq' }, 1, { x: 0, y: 0, size: 40 }];
  assert.equal(art.overhang(...hq), 0);                    // not measured yet
  await art.measure([hq, [1, { name: 'Plain' }, null, { x: 40, y: 0, size: 40 }]]);
  assert.equal(art.overhang(...hq), 15 / 16);
  assert.equal(art.overhang(1, { name: 'Plain' }, null, { x: 40, y: 0, size: 40 }), 0);
  delete globalThis.Image;
}
console.log('art overhang ok');
