// A bot that plays one unit at a time, without the full legal-action list (docs/socket-api.md,
// "A turn without the full list"). It joins with legal: "summary", asks the server for each
// unit's options, and moves by destination. Simple rules: capture if it can, else attack the
// first target in reach, else move as far as it can and wait; then build Infantry, then end the
// turn. Node 22+ (built-in WebSocket), no packages.
//
//   node unit_by_unit_bot.mjs ws://127.0.0.1:8765 GAME [PLAYER_ID]
const [url, game, player] = process.argv.slice(2);
const ws = new WebSocket(url);
const send = (msg) => ws.send(JSON.stringify(msg));

// Requests with an id get their reply matched here; broadcasts go to onBroadcast.
let nextId = 1;
const waiting = new Map();
const request = (msg) => new Promise((resolve) => {
  const id = nextId++;
  waiting.set(id, resolve);
  send({ ...msg, id });
});

let joined = false;
let busy = false;

ws.onopen = () => {
  send({ type: 'hello', name: 'unit-by-unit-bot' });
  send({ type: 'join', game, legal: 'summary', ...(player ? { player: Number(player) } : {}) });
};

ws.onmessage = ({ data }) => {
  const m = JSON.parse(data);
  if (m.id != null && waiting.has(m.id)) {
    waiting.get(m.id)(m);
    waiting.delete(m.id);
    return;
  }
  switch (m.type) {
    case 'joined':
      joined = true;
      console.log(`seat ${m.seat}, player ${m.player}`);
      break;
    case 'turn':
      if (!busy) {
        busy = true;
        playOne(m.summary).finally(() => { busy = false; });
      }
      break;
    case 'error':
      console.log('error:', m.message);
      if (!joined) ws.close(); // couldn't join: nothing to play
      break;
    case 'game_over':
      console.log('game over:', m.reason, 'winner team', m.winner);
      ws.close();
      break;
  }
};

// One action per turn prompt: the server sends a fresh summary after each one.
async function playOne(summary) {
  for (const u of summary.units) {
    const { tiles } = await request({ type: 'options', game, unit: u.id });
    const pick = choose(u, tiles);
    if (pick) {
      const r = await request({ type: 'act', game, action: { type: 'move', unit_id: u.id, to: pick.to, then: pick.then } });
      if (r.type === 'ok') return;
    }
  }
  for (const b of summary.builds) {
    if (b.units.includes('Infantry')) {
      const r = await request({ type: 'act', game, action: { type: 'build', at: b.at, unit: 'Infantry' } });
      if (r.type === 'ok') return;
    }
  }
  await request({ type: 'act', game, action: { type: 'end_turn' } });
}

/** Capture, else attack, else the farthest tile with a plain wait. */
function choose(u, tiles) {
  const all = tiles.flatMap((t) => t.then.map((then) => ({ to: t.to, then, steps: t.path.length })));
  return all.find((a) => a.then.type === 'capture')
    ?? all.find((a) => a.then.type === 'attack')
    ?? all.filter((a) => a.then.type === 'wait').sort((a, b) => b.steps - a.steps)[0];
}
