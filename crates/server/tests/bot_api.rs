//! A bot's view of its turn without the full legal list: the summary, filtered `legal`,
//! per-unit `options`, and moves by destination.
use libaw_server::Hub;
use serde_json::{json, Value};

fn day22() -> Value {
    serde_json::from_slice(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),
        "/../../docs/eval-harness/test-states/game-1729758-turn-042-D22-pid4151012.json")).unwrap()).unwrap()
}

fn start(legal: Option<&str>) -> (Hub, String, Value) {
    let mut h = Hub::new();
    let g = h.handle(9, &json!({"type": "create", "turn": day22()}))[0].1["game"].as_str().unwrap().to_string();
    let cur = h.games[&g].state.current;
    let mut j = json!({"type": "join", "game": g, "player": cur});
    if let Some(l) = legal { j["legal"] = json!(l); }
    let out = h.handle(1, &j);
    let turn = out.iter().find(|(_, v)| v["type"] == "turn").expect("our turn").1.clone();
    (h, g, turn)
}

#[test]
fn the_summary_is_short_and_lists_what_can_act() {
    let (_, _, full) = start(None);
    let (_, _, sum) = start(Some("summary"));
    let n = full["legal"].as_array().unwrap().len();
    assert_eq!(sum["count"], n);
    assert!(sum["legal"].is_null());
    let units = sum["summary"]["units"].as_array().unwrap();
    assert!(!units.is_empty());
    let acts: u64 = units.iter().map(|u| u["actions"].as_u64().unwrap()).sum();
    let moves = full["legal"].as_array().unwrap().iter().filter(|a| a["type"] == "move").count() as u64;
    assert_eq!(acts, moves, "every move belongs to a listed unit");
    assert_eq!(sum["summary"]["end_turn"], true);
    eprintln!("full {} bytes, summary {} bytes", full.to_string().len(), sum.to_string().len());
    assert!(sum.to_string().len() * 10 < full.to_string().len(), "much smaller");
}

#[test]
fn filters_options_and_moves_by_destination() {
    let (mut h, g, sum) = start(Some("summary"));
    let u = sum["summary"]["units"][0]["id"].as_u64().unwrap();
    // legal, one unit.
    let one = h.handle(1, &json!({"type": "legal", "game": g, "filter": {"unit": u}, "id": 3}))[0].1.clone();
    assert_eq!(one["id"], 3);
    let l = one["legal"].as_array().unwrap();
    assert!(!l.is_empty() && l.iter().all(|a| a["unit_id"] == u), "{one}");
    // legal, only builds and end_turn.
    let b = h.handle(1, &json!({"type": "legal", "game": g, "filter": {"kind": ["build", "end_turn"]}}))[0].1.clone();
    assert!(b["legal"].as_array().unwrap().iter().all(|a| a["type"] == "build" || a["type"] == "end_turn"));
    // options: the same choices, grouped by tile.
    let o = h.handle(1, &json!({"type": "options", "game": g, "unit": u}))[0].1.clone();
    let tiles = o["tiles"].as_array().unwrap();
    let thens: usize = tiles.iter().map(|t| t["then"].as_array().unwrap().len()).sum();
    assert_eq!(thens, l.len(), "{o}");
    // A move by destination only: the server picks the path.
    let t = &tiles[tiles.len() - 1];
    let mv = json!({"type": "move", "unit_id": u, "to": t["to"], "then": t["then"][0]});
    let out = h.handle(1, &json!({"type": "act", "game": g, "action": mv}));
    assert_eq!(out[0].1["type"], "ok", "{:?}", out[0]);
    // Not our turn any more for that unit: options for it are now empty.
    let o2 = h.handle(1, &json!({"type": "options", "game": g, "unit": u}))[0].1.clone();
    assert_eq!(o2["tiles"].as_array().unwrap().len(), 0, "{o2}");
}
