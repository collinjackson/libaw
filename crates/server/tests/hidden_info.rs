//! What a seat must not learn about units it can't see (the fog-leak audit, 2026-11-14).
use libaw_server::Hub;
use serde_json::{json, Value};

fn p(id: u32, order: u32) -> Value {
    json!({"id": id, "team": id.to_string(), "co": "Andy", "funds": 5000, "eliminated": false,
        "order": order, "power": "N", "meter": 0, "cop_cost": 0, "scop_cost": 0, "power_attack": 0})
}
fn u(id: u32, owner: u32, kind: &str, x: u32, dived: bool) -> Value {
    json!({"id": id, "owner": owner, "kind": kind, "x": x, "y": 0, "hp": {"lo": 100, "hi": 100}, "fuel": 60, "ammo": 6,
        "moved": false, "stunned": false, "carried_by": null, "cargo": [], "dived": dived})
}
/// A 10x1 sea strip, no fog: player 1's Lander at x=0, player 2's Sub at x=6.
fn sea(dived: bool) -> Value {
    sea_fog(dived, false)
}
fn sea_fog(dived: bool, fog: bool) -> Value {
    json!({"width": 10, "height": 1, "terrain": vec![28; 10], "buildings": [],
        "units": {"1": u(1, 1, "Lander", 0, false), "2": u(2, 2, "Sub", 6, dived)},
        "players": [p(1, 0), p(2, 1)], "current": 1, "day": 3, "funds_per_property": 1000, "weather": "C",
        "move_override": {}, "powers": true, "seams": false, "dives": true, "deletes": false, "fog": fog,
        "partial": false, "banned": []})
}
fn events_to(out: &[(u64, Value)], conn: u64) -> usize {
    out.iter().filter(|(c, v)| *c == conn && v["type"] == "event").count()
}
fn sub_moves(dived: bool) -> usize {
    let mut h = Hub::new();
    let g = h.handle(9, &json!({"type": "create", "state": sea(dived)}))[0].1["game"].as_str().unwrap().to_string();
    h.handle(1, &json!({"type": "join", "game": g, "player": 1}));
    h.handle(2, &json!({"type": "join", "game": g, "player": 2}));
    h.handle(1, &json!({"type": "act", "game": g, "action": {"type": "end_turn"}}));
    let mv = json!({"type": "move", "unit_id": 2, "path": [[6, 0], [5, 0], [4, 0]], "then": {"type": "wait"}});
    let out = h.handle(2, &json!({"type": "act", "game": g, "action": mv}));
    assert!(out.iter().any(|(c, v)| *c == 2 && v["type"] == "ok"), "the move is legal: {out:?}");
    events_to(&out, 1)
}

#[test]
fn a_dived_subs_move_is_not_announced_without_fog() {
    assert_eq!(sub_moves(true), 0, "player 1 can't see the dived Sub, so it hears nothing");
    assert_eq!(sub_moves(false), 1, "a surfaced Sub's move is announced as usual");
}

/// Fog: an enemy move out of sight sends the seat nothing, and the seat's `seq` counts only its
/// own view's changes, so it can't count (or time) the moves it didn't see.
#[test]
fn an_unseen_move_in_fog_sends_nothing() {
    let mut h = Hub::new();
    let g = h.handle(9, &json!({"type": "create", "state": sea_fog(false, true)}))[0].1["game"].as_str().unwrap().to_string();
    let j1 = h.handle(1, &json!({"type": "join", "game": g, "player": 1}));
    h.handle(2, &json!({"type": "join", "game": g, "player": 2}));
    let seq0 = j1.iter().find(|(_, v)| v["type"] == "state").unwrap().1["seq"].as_u64().unwrap();
    let end = h.handle(1, &json!({"type": "act", "game": g, "action": {"type": "end_turn"}}));
    let seq1 = end.iter().find(|(c, v)| *c == 1 && v["type"] == "state").map(|(_, v)| v["seq"].as_u64().unwrap()).unwrap_or(seq0);
    // The Sub at x=6 moves to x=7: still far out of the Lander's sight.
    let mv = json!({"type": "move", "unit_id": 2, "path": [[6, 0], [7, 0]], "then": {"type": "wait"}});
    let out = h.handle(2, &json!({"type": "act", "game": g, "action": mv}));
    assert!(out.iter().any(|(c, v)| *c == 2 && v["type"] == "ok"), "{out:?}");
    assert!(out.iter().all(|(c, _)| *c != 1), "player 1 hears nothing: {out:?}");
    // Player 2's own messages carry its own count, not the game's action count.
    let ok = &out.iter().find(|(c, v)| *c == 2 && v["type"] == "ok").unwrap().1;
    assert!(ok["seq"].as_u64().unwrap() <= 3, "{ok}");
    let back = h.handle(2, &json!({"type": "act", "game": g, "action": {"type": "end_turn"}}));
    let turn = &back.iter().find(|(c, v)| *c == 1 && v["type"] == "turn").expect("player 1's turn").1;
    assert!(turn["seq"].as_u64().unwrap() <= seq1 + 1, "player 1's seq moved only with its own view: {turn}");
}
