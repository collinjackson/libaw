// A minimal JavaScript client for libaw-server (docs/socket-api.md): joins a game and always
// plays the first legal action that isn't "end turn", up to 20 a turn. Node 22+ (built-in
// WebSocket), no packages.
//
//   node first_legal_bot.mjs ws://127.0.0.1:8765 GAME [PLAYER_ID]
const [url, game, player] = process.argv.slice(2);
const ws = new WebSocket(url);
const send = (msg) => ws.send(JSON.stringify(msg));
let actsThisTurn = 0;
let joined = false;

ws.onopen = () => {
  send({ type: 'hello', name: 'first-legal-bot' });
  send({ type: 'join', game, ...(player ? { player: Number(player) } : {}) });
};

ws.onmessage = ({ data }) => {
  const m = JSON.parse(data);
  switch (m.type) {
    case 'joined':
      joined = true;
      console.log(`seat ${m.seat}, player ${m.player}`);
      break;
    case 'turn': {
      const move = m.legal.find((a) => a.type !== 'end_turn');
      const action = move && actsThisTurn < 20 ? move : { type: 'end_turn' };
      actsThisTurn = action.type === 'end_turn' ? 0 : actsThisTurn + 1;
      send({ type: 'act', game, action });
      break;
    }
    case 'event':
      console.log(`day ${m.day}: player ${m.by}: ${m.label}`);
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
