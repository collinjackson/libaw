// An in-page stand-in for libaw-server (docs/socket-api.md) for the move planner: the engine runs
// here as WebAssembly (web/pkg, built by `wasm-pack build crates/wasm --target web --out-dir
// ../../web/pkg`), so a position can be played forward with no server and nothing leaves the page.
// It speaks the subset app.js uses (hello, list, create, join, act, preview, state, leave) with the
// same reply and broadcast shapes, plus planner requests: undo, reset, luck, bot.
//
// A bot can take one player: FlakBot through awbw-bot's HTTP contract on this machine (POST /act
// with an AWBW turn file; awbw-bot allows localhost pages), or the engine's greedy baseline in the
// page. Its whole turn is one step back for undo: undo returns to before the end_turn that gave
// it the move.
//
// One seat plays every side: after each action the client takes the seat of whoever is to move,
// so a player can try their turn, end it, and try the opponent's reply.
import init, * as E from '../pkg/libaw_wasm.js';

let ready = null;
const engine = (wasm) => (ready ??= init(wasm ? { module_or_path: wasm } : undefined));   // wasm: bytes, for Node

export class LocalConn {
  constructor() {
    this.onMessage = () => {};
    this.onClose = () => {};
    this.game = null;
  }

  async open(wasm) {
    await engine(wasm);
    return this;
  }

  close() {}
  send(msg) { this.request(msg).catch(() => {}); }

  /** Answers like the server: the direct reply resolves (an error rejects), broadcasts go to
   * onMessage after it, in the server's order. */
  async request(msg) {
    const out = [];
    let reply;
    try {
      reply = this.handle(msg, out);
    } catch (e) {
      const r = { type: 'error', re: msg.type, message: e.message, id: msg.id };
      this.onMessage(r);
      throw Object.assign(new Error(e.message), { reply: r });
    }
    // The caller handles its reply before the broadcasts, as over a socket (a macrotask: an
    // awaiting caller resumes only after microtasks queued now, so a microtask would beat it).
    // A bot that now has the move starts after these are out, so its moves follow them.
    setTimeout(() => { out.forEach((m) => this.onMessage(m)); this.wakeBot(); }, 0);
    return reply;
  }

  handle(msg, out) {
    const g = this.game;
    switch (msg.type) {
      case 'hello':
        return { type: 'welcome', protocol: 1, server: 'libaw-planner (in page)', version: '' };
      case 'list':
        return { type: 'games', games: g ? [this.summary()] : [] };
      case 'create': {
        const r = msg.turn ? parse(E.position(JSON.stringify(msg.turn))) : { state: msg.state };
        // AWBW usernames, for the player list and the log.
        const names = {};
        for (const p of Object.values(msg.turn?.gameState?.players ?? {})) names[Number(p.players_id)] = p.users_username;
        for (const p of r.state.players) if (names[p.id] && !p.name) p.name = names[p.id];
        // The game's settings when the caller knows them (the site's AWBW loader does).
        if (msg.powers != null) r.state.powers = !!msg.powers;
        if (Array.isArray(msg.banned)) r.state.banned = msg.banned;
        const id = msg.name || 'plan';
        this.game = { id, start: r.state, state: r.state, history: [], seq: 0, luck: msg.luck || 'random',
          seed: (msg.seed >>> 0) || ((Math.random() * 2 ** 32) >>> 0), template: msg.turn ?? null, bot: null, busy: false };
        return { type: 'created', game: id, seed: this.game.seed, players: r.state.players.map((p) => p.id), awbw_format: false };
      }
      case 'join':
      case 'spectate':
        need(g);
        this.broadcast(out);
        return { type: 'joined', game: g.id, player: g.state.current, seat: 0, token: 'local', format: 'libaw', info: this.info() };
      case 'state':
        need(g);
        this.broadcast(out, false);
        return { type: 'state', ...this.stateMsg() };
      case 'legal':
        need(g);
        out.push(this.turnMsg());
        return { type: 'ok' };
      case 'preview': {
        need(g);
        return parse(E.preview(JSON.stringify(g.state), JSON.stringify(msg.action)));
      }
      case 'act': {
        need(g);
        if (g.busy || this.botMoves()) throw new Error('the bot has the move');
        g.history.push(g.state);
        this.apply(msg.action, out);
        return { type: 'ok', seq: g.seq };
      }
      case 'bot': {
        need(g);
        g.bot = msg.player == null ? null : { player: msg.player, kind: msg.kind || 'engine', url: msg.url || '' };
        this.broadcast(out);
        return { type: 'ok' };
      }
      case 'undo': {
        need(g);
        g.over = false;
        if (g.busy) throw new Error('the bot is still moving');
        if (!g.history.length) throw new Error('nothing to undo');
        g.state = g.history.pop();
        g.seq += 1;
        this.broadcast(out);
        return { type: 'ok', seq: g.seq };
      }
      case 'reset': {
        need(g);
        if (g.history.length) g.history = [];
        if (g.busy) throw new Error('the bot is still moving');
        g.state = g.start;
        g.over = false;
        g.seq += 1;
        this.broadcast(out);
        return { type: 'ok', seq: g.seq };
      }
      case 'luck':
        need(g);
        g.luck = msg.luck;
        return { type: 'ok' };
      case 'leave':
      case 'resign':
        return { type: 'left' };
      default:
        throw new Error(`the planner doesn't do ${msg.type}`);
    }
  }

  /** One action by the luck mode: state, event and broadcasts into `out`. */
  apply(action, out) {
    const g = this.game, before = g.state;
    const r = parse(E.act_luck(JSON.stringify(before), JSON.stringify(action), g.luck, (g.seed + g.seq * 7919) >>> 0));
    g.state = r.state;
    g.seq += 1;
    out.push({ type: 'event', game: g.id, seq: g.seq, by: before.current, day: before.day, action,
      label: r.label, ended_turn: r.state.current !== before.current || r.state.day !== before.day });
    this.broadcast(out);
    g.over = r.winner != null;
    if (r.winner != null) out.push({ type: 'game_over', game: g.id, winner: r.winner, reason: 'last_standing', day: r.state.day });
    return r;
  }

  botMoves() {
    const g = this.game;
    return !!g?.bot && g.state.current === g.bot.player && !g.over && parse(E.legal(JSON.stringify(g.state))).actions.length > 0;
  }

  /** Plays the bot's turn if it has the move (after the caller's reply and broadcasts). */
  wakeBot() {
    const g = this.game;
    if (!g || g.busy || !this.botMoves()) return;
    g.busy = true;
    setTimeout(() => this.botTurn(g).finally(() => { g.busy = false; this.onMessage({ type: 'bot_done', game: g.id }); }), 0);
  }

  async botTurn(g) {
    const emit = (out) => out.forEach((m) => this.onMessage(m));
    for (let n = 0; n < 400 && this.game === g && this.botMoves(); n++) {
      const player = g.state.current;
      let action;
      try {
        action = g.bot.kind === 'flakbot' ? await this.askFlakBot(g) : this.engineMove(g);
      } catch (e) {
        this.onMessage({ type: 'error', message: `bot: ${e.message}; it ends its turn` });
        action = { type: 'end_turn' };
      }
      if (this.game !== g || g.state.current !== player) return;   // reset or undone meanwhile
      const out = [];
      try { this.apply(action, out); } catch (e) {
        this.onMessage({ type: 'error', message: `bot move refused (${e.message}); it ends its turn` });
        this.apply({ type: 'end_turn' }, out);
      }
      emit(out);
      if (g.bot.kind === 'flakbot') await new Promise((r) => setTimeout(r, 150));   // watchable
    }
  }

  /** The engine's greedy baseline, one action at a time. */
  engineMove(g) {
    return parse(E.bot_move(JSON.stringify(g.state))).action;
  }

  async askFlakBot(g) {
    if (!g.template) throw new Error('FlakBot needs a position loaded from an AWBW turn file');
    const s = g.state;
    const body = { player_id: s.current, seed: g.seq, game: `planner-${g.seed}`, powers: !!s.powers, banned: s.banned ?? [],
      state: parse(E.turn_file(JSON.stringify(s), JSON.stringify(g.template))), bot: 'plan2~FlakBot' };
    const r = await fetch(`${g.bot.url.replace(/\/$/, '')}/act`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    const a = await r.json();
    if (a.error) throw new Error(a.error);
    delete a.why;
    return a;
  }

  summary() {
    const g = this.game;
    return { game: g.id, day: g.state.day, over: false, fog: !!g.state.fog,
      players: g.state.players.map((p) => ({ id: p.id, co: p.co, taken: false })) };
  }

  info() {
    return { players: this.game.state.players.map((p, i) => ({ id: p.id, seat: i, co: p.co, name: p.name || '' })) };
  }

  stateMsg() {
    const g = this.game;
    return { game: g.id, format: 'libaw', day: g.state.day, current: g.state.current, seq: g.seq,
      income: parse(E.income(JSON.stringify(g.state))), state: g.state };
  }

  turnMsg() {
    const g = this.game;
    // The bot's seat gets no legal actions here, so the board can't move its units.
    const legal = g.bot && g.state.current === g.bot.player ? [] : parse(E.legal(JSON.stringify(g.state))).actions.map((a) => a.action);
    return { type: 'turn', game: g.id, player: g.state.current, day: g.state.day, seq: g.seq, legal };
  }

  broadcast(out, turn = true) {
    out.push({ type: 'state', ...this.stateMsg() });
    if (turn) out.push(this.turnMsg());
  }

  get canUndo() { return !!this.game?.history.length; }
}

function parse(text) {
  const v = JSON.parse(text);
  if (v && v.error) throw new Error(v.error);
  return v;
}

function need(g) {
  if (!g) throw new Error('no position loaded');
}
