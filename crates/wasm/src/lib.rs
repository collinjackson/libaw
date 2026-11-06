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

/// A new game with chosen COs: `opts` is {"cos": [first player's, second's], "powers": bool}.
/// "No CO" (or an unknown name) plays without a CO; powers need `powers` and a CO.
#[wasm_bindgen]
pub fn start_with(turn_file: &str, opts: &str) -> String {
    let v: Value = match serde_json::from_str(turn_file) { Ok(v) => v, Err(e) => return err(e) };
    let o: Value = serde_json::from_str(opts).unwrap_or(json!({}));
    let mut s = state_from(&v);
    for u in s.units.values_mut() {
        u.hp = Hp::exact(u.hp.hi);
    }
    for (i, p) in s.players.iter_mut().enumerate() {
        if let Some(co) = o["cos"][i].as_str() {
            let (c, sc) = awbw_engine::co::power_base(co).unwrap_or((0, 0));
            p.co = if c == 0 { "No CO".into() } else { co.to_string() };
            p.cop_cost = c;
            p.scop_cost = sc;
            p.meter = 0;
            p.power = "N".into();
        }
    }
    s.powers = o["powers"].as_bool().unwrap_or(true);
    done(&s, json!({"header": log_header(&s, "")}))
}

/// The COs a game can use, "No CO" first: [{name, powers: "full" | "partial" | "none"}].
#[wasm_bindgen]
pub fn cos() -> String {
    json!(awbw_engine::co::roster().into_iter().map(|(n, st)| json!({"name": n, "powers": st})).collect::<Vec<_>>()).to_string()
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
        // Greedy also reports its top candidates, so the page can show why it moved.
        let (a, why) = match level {
            "greedy" => {
                let r = baseline::ranked(&s);
                let why: Vec<Value> = r.iter().take(3).map(|(g, c)| json!({"label": describe(&s, c), "score": (g * 10.0).round() / 10.0})).collect();
                (r.into_iter().next().map(|(_, a)| a).unwrap_or(Action::EndTurn), why)
            }
            _ => (baseline::random(&s, &mut next), vec![]),
        };
        let label = describe(&s, &a);
        let mut roll = |lo: i32, hi: i32| lo + (next() as i32).rem_euclid(hi - lo + 1);
        if sim::step(&mut s, &a, &mut roll).is_err() {
            let _ = sim::end_turn(&mut s);
        }
        let mut f = frame(&s, &label);
        if !why.is_empty() {
            f["why"] = json!(why);
        }
        frames.push(f);
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
