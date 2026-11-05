//! The engine for the play-a-bot page, as WebAssembly for a server-side function. Every call
//! takes and returns JSON text; the game state travels with each request (the page holds it).

use awbw_engine::actions::{legal_actions, Action};
use awbw_engine::awbw::{action_from_contract, action_to_contract, state_from};
use awbw_engine::view::{describe, frame, log_header};
use awbw_engine::{baseline, sim, Hp, State};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

fn rng(seed: u32) -> impl FnMut() -> u32 {
    let mut r = (seed as u64).wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    move || { r = r.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (r >> 33) as u32 }
}

fn err(e: impl std::fmt::Display) -> String {
    json!({"error": e.to_string()}).to_string()
}

fn load(state: &str) -> Result<State, String> {
    serde_json::from_str(state).map_err(|e| format!("bad state: {e}"))
}

fn done(s: &State, extra: Value) -> String {
    let mut v = json!({"state": s, "frame": frame(s, ""), "winner": sim::winner(s)});
    if let (Value::Object(o), Value::Object(e)) = (&mut v, extra) {
        o.extend(e);
    }
    v.to_string()
}

/// A new game from an AWBW turn file (a map's day-1 start).
#[wasm_bindgen]
pub fn start(turn_file: &str) -> String {
    let v: Value = match serde_json::from_str(turn_file) { Ok(v) => v, Err(e) => return err(e) };
    let mut s = state_from(&v);
    for u in s.units.values_mut() {
        u.hp = Hp::exact(u.hp.hi);
    }
    done(&s, json!({"header": log_header(&s, "")}))
}

/// Legal actions for the player to move, in the contract's format, with labels.
#[wasm_bindgen]
pub fn legal(state: &str) -> String {
    let s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let out: Vec<Value> = legal_actions(&s).iter()
        .map(|a| json!({"action": action_to_contract(a), "label": describe(&s, a)})).collect();
    json!({"actions": out}).to_string()
}

/// Apply one action (contract format) for the player to move, if legal.
#[wasm_bindgen]
pub fn act(state: &str, action: &str, seed: u32) -> String {
    let mut s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let v: Value = match serde_json::from_str(action) { Ok(v) => v, Err(e) => return err(e) };
    let a = match action_from_contract(&s, &v) { Ok(a) => a, Err(e) => return err(e) };
    let label = describe(&s, &a);
    let mut next = rng(seed);
    let mut roll = |lo: i32, hi: i32| lo + (next() as i32).rem_euclid(hi - lo + 1);
    if let Err(e) = sim::step(&mut s, &a, &mut roll) {
        return err(e);
    }
    done(&s, json!({"label": label}))
}

/// Play the bot's whole turn: `level` is "random" or "greedy". Returns a frame per action.
#[wasm_bindgen]
pub fn bot_turn(state: &str, level: &str, seed: u32) -> String {
    let mut s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let mut next = rng(seed);
    let me = s.current;
    let mut frames = vec![];
    for _ in 0..300 {
        if sim::winner(&s).is_some() || s.current != me {
            break;
        }
        let a = match level {
            "greedy" => baseline::choose(&s),
            _ => baseline::random(&s, &mut next),
        };
        let label = describe(&s, &a);
        let mut roll = |lo: i32, hi: i32| lo + (next() as i32).rem_euclid(hi - lo + 1);
        if sim::step(&mut s, &a, &mut roll).is_err() {
            let _ = sim::end_turn(&mut s);
        }
        frames.push(frame(&s, &label));
        if a == Action::EndTurn {
            break;
        }
    }
    if s.current == me && sim::winner(&s).is_none() {
        let _ = sim::end_turn(&mut s);
        frames.push(frame(&s, "end turn"));
    }
    done(&s, json!({"frames": frames}))
}
