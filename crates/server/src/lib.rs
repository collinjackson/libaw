//! libaw socket API, protocol v1 (docs/socket-api.md): games held in memory, played by clients
//! over WebSocket with JSON messages. The engine is the referee: a client sends one action at a
//! time, the server checks it against the legal actions of what that player can see, applies it
//! with the game's seeded luck, and tells every client in the game what changed.
//!
//! `Hub::handle` is the whole protocol as a pure function of (message in) -> (messages out), so
//! it is tested without sockets; `serve` only moves frames between sockets and the hub.

use awbw_engine::actions::{legal_actions, Action};
use awbw_engine::awbw::{action_from_contract, action_to_contract, state_from, to_turn_json};
use awbw_engine::{fog, sim, view, Hp, PlayerId, State};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

pub const PROTOCOL: u32 = 1;
pub type ConnId = u64;
/// Outgoing messages: (connection, message).
pub type Out = Vec<(ConnId, Value)>;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Format {
    /// The engine's own state JSON (crates/engine/src/state.rs).
    Libaw,
    /// An AWBW replay turn file (`gameState`, ...), as the bot contract uses. Only for games
    /// created from one (it needs that file as a template).
    Awbw,
}

struct Seat {
    player: PlayerId,
    conn: Option<ConnId>,
    token: Option<String>,
    format: Format,
    name: String,
}

pub struct Game {
    pub id: String,
    pub state: State,
    template: Option<Value>,
    seed: u64,
    rng: u64,
    max_days: u32,
    seats: Vec<Seat>,
    spectators: Vec<(ConnId, Format)>,
    seq: u64,
    pub over: Option<Value>,
}

impl Game {
    fn seat_of_conn(&self, conn: ConnId) -> Option<&Seat> {
        self.seats.iter().find(|s| s.conn == Some(conn))
    }

    /// Everyone subscribed: (conn, player or None for a spectator, format).
    fn audience(&self) -> Vec<(ConnId, Option<PlayerId>, Format)> {
        let mut v: Vec<_> = self.seats.iter().filter_map(|s| s.conn.map(|c| (c, Some(s.player), s.format))).collect();
        v.extend(self.spectators.iter().map(|&(c, f)| (c, None, f)));
        v
    }

    /// What `who` sees: its player's fog view, or everything for a spectator.
    fn view_for(&self, who: Option<PlayerId>) -> State {
        match who {
            Some(p) => fog::player_view(&self.state, p),
            None => self.state.clone(),
        }
    }

    fn state_msg(&self, who: Option<PlayerId>, format: Format) -> Value {
        let v = self.view_for(who);
        let state = match (format, &self.template) {
            (Format::Awbw, Some(t)) => to_turn_json(&v, t),
            _ => serde_json::to_value(&v).unwrap_or(Value::Null),
        };
        json!({"type": "state", "game": self.id, "format": fmt_name(format), "day": v.day,
               "current": v.current, "seq": self.seq, "state": state})
    }

    fn turn_msg(&self) -> Value {
        let v = fog::player_view(&self.state, self.state.current);
        let legal: Vec<Value> = legal_actions(&v).iter().map(action_to_contract).collect();
        json!({"type": "turn", "game": self.id, "player": self.state.current, "day": self.state.day,
               "seq": self.seq, "legal": legal})
    }

    fn players_json(&self) -> Vec<Value> {
        self.state.players.iter().enumerate().map(|(i, p)| {
            let seat = self.seats.iter().find(|s| s.player == p.id);
            json!({"seat": i, "id": p.id, "co": p.co, "team": p.team, "funds_visible_to_all": !self.state.fog,
                   "taken": seat.is_some_and(|s| s.token.is_some()), "name": seat.map(|s| s.name.clone())})
        }).collect()
    }

    fn summary(&self) -> Value {
        json!({"game": self.id, "day": self.state.day, "current": self.state.current, "fog": self.state.fog,
               "powers": self.state.powers, "banned": self.state.banned, "seed": self.seed, "max_days": self.max_days,
               "players": self.players_json(), "over": self.over.is_some()})
    }

    /// After the position changed: per-viewer states, the turn prompt or the game's end.
    fn after_change(&mut self, out: &mut Out) {
        for (c, who, f) in self.audience() {
            out.push((c, self.state_msg(who, f)));
        }
        if self.over.is_none() {
            if let Some(t) = sim::winner(&self.state) {
                self.over = Some(json!({"type": "game_over", "game": self.id, "winner": t, "reason": "last_standing",
                                        "day": self.state.day}));
            } else if self.max_days > 0 && self.state.day > self.max_days {
                self.over = Some(json!({"type": "game_over", "game": self.id, "winner": Value::Null,
                                        "reason": "day_limit", "day": self.max_days}));
            }
        }
        match &self.over {
            Some(m) => {
                for (c, _, _) in self.audience() {
                    out.push((c, m.clone()));
                }
            }
            None => {
                let cur = self.state.current;
                if let Some(c) = self.seats.iter().find(|s| s.player == cur).and_then(|s| s.conn) {
                    out.push((c, self.turn_msg()));
                }
            }
        }
    }
}

fn fmt_name(f: Format) -> &'static str {
    match f {
        Format::Libaw => "libaw",
        Format::Awbw => "awbw",
    }
}

#[derive(Default)]
pub struct Hub {
    pub games: BTreeMap<String, Game>,
    next_game: u64,
    next_token: u64,
    names: HashMap<ConnId, String>,
}

fn err(conn: ConnId, re: &Value, message: impl Into<String>) -> (ConnId, Value) {
    (conn, with_id(json!({"type": "error", "re": re["type"], "message": message.into()}), re))
}

/// Echo the request's `id` (if any) on a direct reply, so a client can match them up.
fn with_id(mut v: Value, req: &Value) -> Value {
    if !req["id"].is_null() {
        v["id"] = req["id"].clone();
    }
    v
}

impl Hub {
    pub fn new() -> Hub {
        Hub::default()
    }

    fn token(&mut self, game: &str, player: PlayerId) -> String {
        self.next_token += 1;
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let mut h: u64 = 0xcbf29ce484222325 ^ (t as u64) ^ self.next_token.wrapping_mul(0x9E3779B97F4A7C15);
        for b in game.bytes().chain(player.to_le_bytes()) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        format!("{h:016x}")
    }

    /// A connection closed: free its seats (they can be taken back with the token) and drop
    /// it as a spectator.
    pub fn disconnect(&mut self, conn: ConnId) {
        self.names.remove(&conn);
        for g in self.games.values_mut() {
            for s in g.seats.iter_mut().filter(|s| s.conn == Some(conn)) {
                s.conn = None;
            }
            g.spectators.retain(|&(c, _)| c != conn);
        }
    }

    pub fn handle(&mut self, conn: ConnId, msg: &Value) -> Out {
        let t = msg["type"].as_str().unwrap_or("");
        match t {
            "hello" => {
                if let Some(n) = msg["name"].as_str() {
                    self.names.insert(conn, n.chars().take(40).collect());
                }
                vec![(conn, with_id(json!({"type": "welcome", "protocol": PROTOCOL, "server": "libaw-server",
                                           "version": env!("CARGO_PKG_VERSION")}), msg))]
            }
            "list" => {
                let games: Vec<Value> = self.games.values().map(|g| g.summary()).collect();
                vec![(conn, with_id(json!({"type": "games", "games": games}), msg))]
            }
            "create" => self.create(conn, msg),
            "join" => self.join(conn, msg),
            "spectate" => self.spectate(conn, msg),
            "leave" => {
                let Some(g) = self.game_mut(msg) else { return vec![err(conn, msg, "no such game")] };
                for s in g.seats.iter_mut().filter(|s| s.conn == Some(conn)) {
                    s.conn = None;
                }
                g.spectators.retain(|&(c, _)| c != conn);
                vec![(conn, with_id(json!({"type": "left", "game": g.id}), msg))]
            }
            "state" => {
                let Some(g) = self.game(msg) else { return vec![err(conn, msg, "no such game")] };
                let (who, f) = match g.seat_of_conn(conn) {
                    Some(s) => (Some(s.player), s.format),
                    None => match g.spectators.iter().find(|&&(c, _)| c == conn) {
                        Some(&(_, f)) => (None, f),
                        None => return vec![err(conn, msg, "join or spectate the game first")],
                    },
                };
                vec![(conn, with_id(g.state_msg(who, f), msg))]
            }
            "legal" => {
                let Some(g) = self.game(msg) else { return vec![err(conn, msg, "no such game")] };
                match g.seat_of_conn(conn) {
                    Some(s) if s.player == g.state.current && g.over.is_none() => vec![(conn, with_id(g.turn_msg(), msg))],
                    Some(_) => vec![err(conn, msg, "not your turn")],
                    None => vec![err(conn, msg, "you hold no seat in this game")],
                }
            }
            "act" => self.act(conn, msg),
            "resign" => self.resign(conn, msg),
            "" => vec![err(conn, msg, "message has no type")],
            other => vec![err(conn, msg, format!("unknown message type {other:?}"))],
        }
    }

    fn game(&self, msg: &Value) -> Option<&Game> {
        msg["game"].as_str().and_then(|g| self.games.get(g))
    }

    fn game_mut(&mut self, msg: &Value) -> Option<&mut Game> {
        msg["game"].as_str().and_then(|g| self.games.get_mut(g))
    }

    fn create(&mut self, conn: ConnId, msg: &Value) -> Out {
        let (mut s, template) = if msg["turn"].is_object() {
            (state_from(&msg["turn"]), Some(msg["turn"].clone()))
        } else if msg["state"].is_object() {
            match serde_json::from_value::<State>(msg["state"].clone()) {
                Ok(s) => (s, None),
                Err(e) => return vec![err(conn, msg, format!("state: {e}"))],
            }
        } else {
            return vec![err(conn, msg, "create needs turn (an AWBW turn file) or state (engine state JSON)")];
        };
        if s.players.len() < 2 || s.width == 0 || s.terrain.len() != s.width as usize * s.height as usize {
            return vec![err(conn, msg, "the position needs a map and at least two players")];
        }
        // The referee sees everything; units start with exact HP; disbanding is allowed.
        s.partial = false;
        s.deletes = true;
        for u in s.units.values_mut() {
            u.hp = Hp::exact(u.hp.hi);
        }
        if let Some(f) = msg["fog"].as_bool() {
            s.fog = f;
        }
        if let Some(p) = msg["powers"].as_bool() {
            s.powers = p;
        }
        if let Some(b) = msg["banned"].as_array() {
            s.banned = b.iter().filter_map(|x| x.as_str().map(String::from)).collect();
        }
        let seed = msg["seed"].as_u64().unwrap_or_else(|| {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)
        });
        self.next_game += 1;
        let id = msg["name"].as_str().filter(|n| !n.is_empty() && !self.games.contains_key(*n))
            .map(String::from).unwrap_or_else(|| format!("g{}", self.next_game));
        let seats = s.players.iter().map(|p| Seat { player: p.id, conn: None, token: None, format: Format::Libaw, name: String::new() }).collect();
        let g = Game {
            id: id.clone(), state: s, template, seed, rng: seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407),
            max_days: msg["max_days"].as_u64().unwrap_or(0) as u32, seats, spectators: vec![], seq: 0, over: None,
        };
        let reply = json!({"type": "created", "game": id, "seed": seed, "players": g.players_json(),
                           "awbw_format": g.template.is_some()});
        self.games.insert(id, g);
        vec![(conn, with_id(reply, msg))]
    }

    fn parse_format(msg: &Value, has_template: bool) -> Result<Format, String> {
        match msg["format"].as_str().unwrap_or("libaw") {
            "libaw" => Ok(Format::Libaw),
            "awbw" if has_template => Ok(Format::Awbw),
            "awbw" => Err("format awbw needs a game created from an AWBW turn file".into()),
            f => Err(format!("unknown format {f:?} (libaw or awbw)")),
        }
    }

    fn join(&mut self, conn: ConnId, msg: &Value) -> Out {
        let name = self.names.get(&conn).cloned().unwrap_or_default();
        let gid = msg["game"].as_str().unwrap_or("").to_string();
        let Some(g) = self.games.get(&gid) else { return vec![err(conn, msg, "no such game")] };
        let format = match Self::parse_format(msg, g.template.is_some()) {
            Ok(f) => f,
            Err(e) => return vec![err(conn, msg, e)],
        };
        // Which seat: by player id, by seat index, or the first free one.
        let idx = if let Some(p) = msg["player"].as_u64() {
            g.seats.iter().position(|s| s.player as u64 == p)
        } else if let Some(i) = msg["seat"].as_u64() {
            (i < g.seats.len() as u64).then_some(i as usize)
        } else {
            g.seats.iter().position(|s| s.token.is_none())
        };
        let Some(i) = idx else { return vec![err(conn, msg, "no such seat, or no free seat")] };
        let given = msg["token"].as_str();
        let player = g.seats[i].player;
        let token = match (&g.seats[i].token, given) {
            (None, _) => self.token(&gid, player),
            (Some(t), Some(x)) if t == x => t.clone(),
            (Some(_), _) => return vec![err(conn, msg, "seat taken (send its token to take it back)")],
        };
        let g = self.games.get_mut(&gid).unwrap();
        let seat = &mut g.seats[i];
        seat.conn = Some(conn);
        seat.token = Some(token.clone());
        seat.format = format;
        if !name.is_empty() {
            seat.name = name;
        }
        let mut out = vec![(conn, with_id(json!({"type": "joined", "game": gid, "player": player, "seat": i,
                                                  "token": token, "format": fmt_name(format), "info": g.summary()}), msg))];
        out.push((conn, g.state_msg(Some(player), format)));
        match &g.over {
            Some(m) => out.push((conn, m.clone())),
            None if g.state.current == player => out.push((conn, g.turn_msg())),
            None => {}
        }
        out
    }

    fn spectate(&mut self, conn: ConnId, msg: &Value) -> Out {
        let Some(g) = self.game_mut(msg) else { return vec![err(conn, msg, "no such game")] };
        let format = match Self::parse_format(msg, g.template.is_some()) {
            Ok(f) => f,
            Err(e) => return vec![err(conn, msg, e)],
        };
        g.spectators.retain(|&(c, _)| c != conn);
        g.spectators.push((conn, format));
        let mut out = vec![(conn, with_id(json!({"type": "joined", "game": g.id, "spectator": true,
                                                  "format": fmt_name(format), "info": g.summary()}), msg))];
        out.push((conn, g.state_msg(None, format)));
        if let Some(m) = &g.over {
            out.push((conn, m.clone()));
        }
        out
    }

    fn act(&mut self, conn: ConnId, msg: &Value) -> Out {
        let Some(g) = self.game_mut(msg) else { return vec![err(conn, msg, "no such game")] };
        if g.over.is_some() {
            return vec![err(conn, msg, "the game is over")];
        }
        let Some(seat) = g.seat_of_conn(conn) else { return vec![err(conn, msg, "you hold no seat in this game")] };
        let me = seat.player;
        if me != g.state.current {
            return vec![err(conn, msg, "not your turn")];
        }
        // Read against what the player can see; applied to the real state (in fog a path can
        // run into a hidden unit, as on AWBW).
        let v = fog::player_view(&g.state, me);
        let a = match action_from_contract(&v, &msg["action"]) {
            Ok(a) => a,
            Err(e) => return vec![err(conn, msg, format!("illegal: {e}"))],
        };
        let before = g.state.clone();
        let label = view::describe(&g.state, &a);
        // Luck comes from the game's seeded generator: the same seed and the same actions give
        // the same game.
        let mut rng = g.rng;
        let mut roll = |lo: i32, hi: i32| {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            lo + ((rng >> 33) as i32).rem_euclid(hi - lo + 1)
        };
        let stepped = sim::step(&mut g.state, &a, &mut roll);
        g.rng = rng;
        if let Err(e) = stepped {
            g.state = before;
            return vec![err(conn, msg, format!("referee refused: {e}"))];
        }
        g.seq += 1;
        let mut out = vec![(conn, with_id(json!({"type": "ok", "game": g.id, "seq": g.seq}), msg))];
        // The event: in fog only the mover's side and spectators hear what was done; the others
        // learn it from their next state.
        let event = json!({"type": "event", "game": g.id, "seq": g.seq, "by": me, "day": before.day,
                           "action": action_to_contract(&a), "label": label, "ended_turn": a == Action::EndTurn});
        for (c, who, _) in g.audience() {
            if !g.state.fog || who.is_none_or(|p| g.state.allied(p, me)) {
                out.push((c, event.clone()));
            }
        }
        g.after_change(&mut out);
        out
    }

    fn resign(&mut self, conn: ConnId, msg: &Value) -> Out {
        let Some(g) = self.game_mut(msg) else { return vec![err(conn, msg, "no such game")] };
        if g.over.is_some() {
            return vec![err(conn, msg, "the game is over")];
        }
        let Some(me) = g.seat_of_conn(conn).map(|s| s.player) else { return vec![err(conn, msg, "you hold no seat in this game")] };
        if let Some(p) = g.state.player_mut(me) {
            p.eliminated = true;
        }
        if g.state.current == me {
            let _ = sim::end_turn(&mut g.state);
        }
        g.seq += 1;
        let mut out = vec![(conn, with_id(json!({"type": "ok", "game": g.id, "seq": g.seq}), msg))];
        let ev = json!({"type": "event", "game": g.id, "seq": g.seq, "by": me, "day": g.state.day,
                        "action": {"type": "resign"}, "label": "resign", "ended_turn": true});
        for (c, _, _) in g.audience() {
            out.push((c, ev.clone()));
        }
        // Alive teams decide it: resigning can end the game before the rout rule would.
        let alive: std::collections::BTreeSet<String> = g.state.players.iter().filter(|p| !p.eliminated).map(|p| p.team.clone()).collect();
        if alive.len() == 1 {
            g.over = Some(json!({"type": "game_over", "game": g.id, "winner": alive.into_iter().next(),
                                 "reason": "resign", "day": g.state.day}));
        }
        g.after_change(&mut out);
        out
    }
}

/// Serve the protocol on `listener` until the process ends.
pub async fn serve(listener: TcpListener) {
    let hub = Arc::new(Mutex::new(Hub::new()));
    let conns: Arc<Mutex<HashMap<ConnId, mpsc::UnboundedSender<Value>>>> = Arc::default();
    let mut next: ConnId = 0;
    while let Ok((tcp, _)) = listener.accept().await {
        next += 1;
        let id = next;
        let (hub, conns) = (hub.clone(), conns.clone());
        tokio::spawn(async move {
            let Ok(ws) = tokio_tungstenite::accept_async(tcp).await else { return };
            let (mut sink, mut stream) = ws.split();
            let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
            conns.lock().unwrap().insert(id, tx);
            let writer = tokio::spawn(async move {
                while let Some(v) = rx.recv().await {
                    if sink.send(Message::Text(v.to_string().into())).await.is_err() {
                        break;
                    }
                }
            });
            while let Some(Ok(m)) = stream.next().await {
                let text = match m {
                    Message::Text(t) => t.to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                let out = match serde_json::from_str::<Value>(&text) {
                    Ok(v) => hub.lock().unwrap().handle(id, &v),
                    Err(e) => vec![(id, json!({"type": "error", "re": Value::Null, "message": format!("bad JSON: {e}")}))],
                };
                let cs = conns.lock().unwrap();
                for (c, v) in out {
                    if let Some(tx) = cs.get(&c) {
                        let _ = tx.send(v);
                    }
                }
            }
            hub.lock().unwrap().disconnect(id);
            conns.lock().unwrap().remove(&id);
            writer.abort();
        });
    }
}
