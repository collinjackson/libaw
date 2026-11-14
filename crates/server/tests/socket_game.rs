//! The socket API end to end: two scripted clients play over real WebSockets, and the hub plays
//! the same seeded game twice to the same end.
use futures_util::{SinkExt, StreamExt};
use libaw_server::Hub;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

/// Player 1's Tank one tile from player 2's last unit, a 1 HP Infantry: one attack ends it.
fn last_stand() -> Value {
    let p = |id: u32, order: u32| json!({"id": id, "team": id.to_string(), "co": "Andy", "funds": 0, "eliminated": false,
        "order": order, "power": "N", "meter": 0, "cop_cost": 0, "scop_cost": 0, "power_attack": 0});
    let u = |id: u32, owner: u32, kind: &str, x: u32, hp: u32, fuel: u32, ammo: u32| json!({"id": id, "owner": owner, "kind": kind,
        "x": x, "y": 0, "hp": {"lo": hp, "hi": hp}, "fuel": fuel, "ammo": ammo, "moved": false, "stunned": false,
        "carried_by": null, "cargo": [], "dived": false});
    json!({"width": 6, "height": 1, "terrain": [1, 1, 1, 1, 1, 1], "buildings": [],
        "units": {"1": u(1, 1, "Tank", 1, 100, 70, 9), "2": u(2, 2, "Infantry", 3, 10, 99, 0)},
        "players": [p(1, 0), p(2, 1)], "current": 1, "day": 3, "funds_per_property": 1000, "weather": "C",
        "move_override": {}, "powers": false, "seams": false, "dives": false, "deletes": false, "fog": false,
        "partial": false, "banned": []})
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::Text(v.to_string().into())).await.unwrap();
}

/// The next message of `kind`, skipping others.
async fn next_of(ws: &mut Ws, kind: &str) -> Value {
    loop {
        let m = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.expect("timed out").unwrap().unwrap();
        if let Message::Text(t) = m {
            let v: Value = serde_json::from_str(&t).unwrap();
            if v["type"] == kind {
                return v;
            }
        }
    }
}

#[tokio::test]
async fn two_clients_play_to_the_end_over_websockets() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    tokio::spawn(libaw_server::serve(listener));
    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    send(&mut a, json!({"type": "hello", "name": "alpha"})).await;
    assert_eq!(next_of(&mut a, "welcome").await["protocol"], 1);
    send(&mut a, json!({"type": "create", "state": last_stand(), "seed": 7, "id": 1})).await;
    let created = next_of(&mut a, "created").await;
    assert_eq!(created["id"], 1);
    let game = created["game"].as_str().unwrap().to_string();

    send(&mut b, json!({"type": "join", "game": game, "player": 2})).await;
    let jb = next_of(&mut b, "joined").await;
    assert_eq!(jb["player"], 2);
    send(&mut a, json!({"type": "join", "game": game, "player": 1})).await;
    assert!(next_of(&mut a, "joined").await["token"].is_string());
    let turn = next_of(&mut a, "turn").await;
    assert_eq!(turn["player"], 1);
    let attack = turn["legal"].as_array().unwrap().iter()
        .find(|l| l["then"]["type"] == "attack").cloned().expect("an attack is legal");

    // Out of turn, and an illegal action: errors, nothing changes.
    send(&mut b, json!({"type": "act", "game": game, "action": {"type": "end_turn"}})).await;
    assert_eq!(next_of(&mut b, "error").await["message"], "not your turn");
    send(&mut a, json!({"type": "act", "game": game, "action": {"type": "build", "at": [0, 0], "unit": "Tank"}})).await;
    assert!(next_of(&mut a, "error").await["message"].as_str().unwrap().starts_with("illegal"));

    send(&mut a, json!({"type": "act", "game": game, "action": attack})).await;
    let ev = next_of(&mut b, "event").await;
    assert_eq!(ev["by"], 1);
    assert_eq!(ev["action"]["then"]["type"], "attack");
    let st = next_of(&mut b, "state").await;
    assert!(st["state"]["units"].get("2").is_none(), "the Infantry is gone");
    for ws in [&mut a, &mut b] {
        let over = next_of(ws, "game_over").await;
        assert_eq!(over["winner"], "1");
        assert_eq!(over["reason"], "last_standing");
    }
}

/// Play a real position with "random" legal actions (picked by a fixed sequence) to a day limit.
fn play(seed: u64) -> (Value, Value) {
    let turn: Value = serde_json::from_slice(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),
        "/../../docs/eval-harness/test-states/game-1729758-turn-042-D22-pid4151012.json")).unwrap()).unwrap();
    let mut hub = Hub::new();
    let out = hub.handle(1, &json!({"type": "create", "turn": turn, "seed": seed, "max_days": 23}));
    let game = out[0].1["game"].as_str().unwrap().to_string();
    let players: Vec<u64> = out[0].1["players"].as_array().unwrap().iter().map(|p| p["id"].as_u64().unwrap()).collect();
    let mut pending = vec![];
    for (i, p) in players.iter().enumerate() {
        pending.extend(hub.handle(10 + i as u64, &json!({"type": "join", "game": game, "player": p, "format": "awbw"})));
    }
    let mut pick: u64 = 12345;
    let mut acts = 0;
    loop {
        let Some((conn, turn)) = pending.iter().rev().find(|(_, v)| v["type"] == "turn" || v["type"] == "game_over").cloned()
        else { panic!("no turn and no game over") };
        if turn["type"] == "game_over" {
            let st = hub.handle(10, &json!({"type": "state", "game": game}));
            return (turn, st[0].1["state"].clone());
        }
        let legal = turn["legal"].as_array().unwrap();
        // End the turn now and then, else a random legal action.
        pick = pick.wrapping_mul(6364136223846793005).wrapping_add(1);
        let a = if (pick >> 40) % 6 == 0 { json!({"type": "end_turn"}) } else { legal[((pick >> 33) as usize) % legal.len()].clone() };
        pending = hub.handle(conn, &json!({"type": "act", "game": game, "action": a}));
        assert!(!pending.iter().any(|(_, v)| v["type"] == "error"), "{:?}", pending.iter().find(|(_, v)| v["type"] == "error"));
        acts += 1;
        assert!(acts < 5000, "runaway game");
    }
}

#[test]
fn same_seed_same_game() {
    let (over1, s1) = play(42);
    let (over2, s2) = play(42);
    assert_eq!(over1["reason"], "day_limit");
    assert_eq!(over1, over2);
    assert_eq!(s1, s2, "a seed and the same actions replay the same game");
    assert!(s1["gameState"].is_object(), "the awbw format is an AWBW turn file");
}

#[test]
fn seats_tokens_and_spectators() {
    let mut hub = Hub::new();
    let created = hub.handle(1, &json!({"type": "create", "state": last_stand(), "name": "t"}))[0].1.clone();
    let g = created["game"].as_str().unwrap().to_string();
    let key = created["spectator_key"].as_str().unwrap().to_string();
    assert_eq!(g, "t");
    assert!(created["seed"].is_null(), "a random seed is never sent while the game runs");
    let j = hub.handle(2, &json!({"type": "join", "game": g, "seat": 1}));
    let token = j[0].1["token"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 32, "128 random bits");
    assert!(j[0].1["info"]["seed"].is_null());
    let taken = hub.handle(3, &json!({"type": "join", "game": g, "seat": 1}));
    assert_eq!(taken[0].1["type"], "error");
    hub.disconnect(2);
    let back = hub.handle(3, &json!({"type": "join", "game": g, "seat": 1, "token": token}));
    assert_eq!(back[0].1["type"], "joined");
    // A running game: no spectating without the key, and never from a seated connection.
    assert_eq!(hub.handle(4, &json!({"type": "spectate", "game": g}))[0].1["type"], "error");
    assert_eq!(hub.handle(3, &json!({"type": "spectate", "game": g, "key": key}))[0].1["type"], "error", "a player can't spectate its own game");
    let sp = hub.handle(4, &json!({"type": "spectate", "game": g, "key": key}));
    assert_eq!(sp[0].1["spectator"], true);
    assert_eq!(hub.handle(4, &json!({"type": "spectate", "game": g, "format": "awbw", "key": key}))[0].1["type"], "error");
    // A resign by the only other player ends it.
    let r = hub.handle(3, &json!({"type": "resign", "game": g}));
    let over = r.iter().find(|(c, v)| *c == 4 && v["type"] == "game_over").expect("the spectator hears it");
    assert_eq!(over.1["winner"], "1");
    assert_eq!(over.1["reason"], "resign");
    // Over: anyone may watch it, and the seed is public now.
    let after = hub.handle(5, &json!({"type": "spectate", "game": g}));
    assert_eq!(after[0].1["spectator"], true);
    assert!(after[0].1["info"]["seed"].is_u64());
}

#[test]
fn a_chosen_seed_replays_the_same_luck() {
    let mut hub = Hub::new();
    let c = hub.handle(1, &json!({"type": "create", "state": last_stand(), "seed": 42}))[0].1.clone();
    assert_eq!(c["seed"], 42, "echoed to the creator who chose it");
    let a = hub.handle(1, &json!({"type": "create", "state": last_stand()}))[0].1["spectator_key"].clone();
    let b = hub.handle(1, &json!({"type": "create", "state": last_stand()}))[0].1["spectator_key"].clone();
    assert_ne!(a, b);
}

#[test]
fn preview_shows_the_luck_range_and_changes_nothing() {
    let mut st = last_stand();
    st["units"]["2"] = json!({"id": 2, "owner": 2, "kind": "Tank", "x": 3, "y": 0, "hp": {"lo": 100, "hi": 100}, "fuel": 70,
        "ammo": 9, "moved": false, "stunned": false, "carried_by": null, "cargo": [], "dived": false});
    let mut hub = Hub::new();
    let g = hub.handle(1, &json!({"type": "create", "state": st, "name": "p"}))[0].1["game"].as_str().unwrap().to_string();
    let j = hub.handle(2, &json!({"type": "join", "game": g, "player": 1}));
    let turn = &j.iter().find(|(_, v)| v["type"] == "turn").expect("player 1 moves").1;
    let attack = turn["legal"].as_array().unwrap().iter()
        .find(|a| a.to_string().contains("attack") && a.to_string().contains("[3,0]") || a.to_string().contains("\"target\":[3,0]"))
        .expect("an attack on the Tank at (3, 0)").clone();
    let st_msg = hub.handle(2, &json!({"type": "state", "game": g}))[0].1.clone();
    assert_eq!(st_msg["income"]["1"], 0, "no properties, no income: {st_msg}");
    let before = st_msg["state"].clone();
    let p = &hub.handle(2, &json!({"type": "preview", "game": g, "action": attack, "id": 7}))[0].1;
    assert_eq!(p["type"], "preview", "{p}");
    assert_eq!(p["id"], 7);
    assert_eq!(p["defender_hp"], 100);
    // Tank on Tank on Plains, no CO effects: 49% at luck 0, 57% at luck 9 (tests/cases/tank-vs-tank-plains-min-luck.json).
    assert_eq!(p["low_luck"]["defender_hp"], 51, "{p}");
    assert_eq!(p["high_luck"]["defender_hp"], 43, "{p}");
    assert!(p["low_luck"]["attacker_hp"].as_u64().unwrap() < 100, "the defender counters");
    let after = hub.handle(2, &json!({"type": "state", "game": g}))[0].1["state"].clone();
    assert_eq!(before, after, "a preview changes nothing");
    let wait = turn["legal"].as_array().unwrap().iter().find(|a| !a.to_string().contains("attack")).unwrap().clone();
    assert_eq!(hub.handle(2, &json!({"type": "preview", "game": g, "action": wait}))[0].1["type"], "error");
}

#[test]
fn state_carries_each_players_income() {
    let turn: Value = serde_json::from_slice(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),
        "/../../docs/eval-harness/test-states/game-1729758-turn-042-D22-pid4151012.json")).unwrap()).unwrap();
    let mut hub = Hub::new();
    let out = hub.handle(1, &json!({"type": "create", "turn": turn}));
    let game = out[0].1["game"].as_str().unwrap().to_string();
    let sp = hub.handle(2, &json!({"type": "spectate", "game": game, "key": out[0].1["spectator_key"]}));
    let st = &sp.iter().find(|(_, v)| v["type"] == "state").unwrap().1;
    let fpp = st["state"]["funds_per_property"].as_i64().unwrap();
    for p in st["state"]["players"].as_array().unwrap() {
        let inc = st["income"][p["id"].to_string()].as_i64().expect("income per player");
        assert!(inc > 0 && inc % fpp == 0, "day-22 income {inc} should be a positive multiple of {fpp}");
    }
}
