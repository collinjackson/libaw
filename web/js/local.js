// An in-page stand-in for libaw-server (docs/socket-api.md) for the move planner: the engine runs
// here as WebAssembly (web/pkg, built by `wasm-pack build crates/wasm --target web --out-dir
// ../../web/pkg`), so a position can be played forward with no server and nothing leaves the page.
// It speaks the subset app.js uses (hello, list, create, join, act, preview, state, leave) with the
// same reply and broadcast shapes, plus planner requests: undo, reset, luck.
//
// One seat plays every side: after each action the client takes the seat of whoever is to move,
// so a player can try their turn, end it, and try the opponent's reply.
import init, * as E from '../pkg/awbw_wasm.js';

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
    setTimeout(() => out.forEach((m) => this.onMessage(m)), 0);
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
        const id = msg.name || 'plan';
        this.game = { id, start: r.state, state: r.state, history: [], seq: 0, luck: msg.luck || 'random',
          seed: (msg.seed >>> 0) || ((Math.random() * 2 ** 32) >>> 0), log: [] };
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
        const before = g.state;
        const r = parse(E.act_luck(JSON.stringify(before), JSON.stringify(msg.action), g.luck, (g.seed + g.seq * 7919) >>> 0));
        g.history.push(before);
        g.state = r.state;
        g.seq += 1;
        out.push({ type: 'event', game: g.id, seq: g.seq, by: before.current, day: before.day, action: msg.action,
          label: r.label, ended_turn: r.state.current !== before.current || r.state.day !== before.day });
        this.broadcast(out);
        if (r.winner != null) out.push({ type: 'game_over', game: g.id, winner: r.winner, reason: 'last_standing', day: r.state.day });
        return { type: 'ok', seq: g.seq };
      }
      case 'undo': {
        need(g);
        if (!g.history.length) throw new Error('nothing to undo');
        g.state = g.history.pop();
        g.seq += 1;
        this.broadcast(out);
        return { type: 'ok', seq: g.seq };
      }
      case 'reset': {
        need(g);
        if (g.history.length) g.history = [];
        g.state = g.start;
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
    const legal = parse(E.legal(JSON.stringify(g.state))).actions.map((a) => a.action);
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
