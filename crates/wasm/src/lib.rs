//! The engine for the play-a-bot page, as WebAssembly for a server-side function. Every call
//! takes and returns JSON text; the game state travels with each request (the page holds it).

use libaw::actions::{legal_actions, Action};
use libaw::awbw::{action_from_contract, action_to_contract, state_from};
use libaw::view::{describe, frame, log_header};
use libaw::{baseline, sim, Hp, State};
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
            let (c, sc) = libaw::co::power_base(co).unwrap_or((0, 0));
            p.co = if c == 0 { libaw::data::Co::NoCO } else { libaw::data::Co::from_name(co).unwrap_or(libaw::data::Co::NoCO) };
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
    json!(libaw::co::roster().into_iter().map(|(n, st)| json!({"name": n, "powers": st})).collect::<Vec<_>>()).to_string()
}

/// Legal actions for the player to move, in the contract's format, with labels.
#[wasm_bindgen]
pub fn legal(state: &str) -> String {
    let s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let out: Vec<Value> = legal_actions(&s).iter()
        .map(|a| json!({"action": action_to_contract(a), "label": describe(&s, a)})).collect();
    json!({"actions": out}).to_string()
}

/// Where a unit can move and what it could fire on (`actions::reach`): {"moves": [[x, y]...], "attack": [...]}.
#[wasm_bindgen]
pub fn reach(state: &str, unit_id: u32) -> String {
    let s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let Some(u) = s.units.get(&unit_id) else { return err(format!("no unit {unit_id}")) };
    let (moves, attack) = libaw::actions::reach(&s, u);
    json!({"moves": moves, "attack": attack}).to_string()
}

/// What a unit may do after moving to (x, y) (its own tile: no move), as the menu there would
/// offer it (`actions::options_at`): {"options": [then, ...]}, each `then` in the contract's format.
#[wasm_bindgen]
pub fn options(state: &str, unit_id: u32, x: u16, y: u16) -> String {
    let s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let Some(u) = s.units.get(&unit_id) else { return err(format!("no unit {unit_id}")) };
    let out: Vec<Value> = libaw::actions::options_at(&s, u, (x, y)).into_iter()
        .map(|then| action_to_contract(&Action::Move { unit: unit_id, path: vec![(x, y)], then })["then"].clone()).collect();
    json!({"options": out}).to_string()
}

/// The build menu of the building at (x, y) (`actions::build_menu`): {"units": [{"unit", "cost"}, ...]},
/// cheapest first, at its owner's prices.
#[wasm_bindgen]
pub fn build_menu(state: &str, x: u16, y: u16) -> String {
    let s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let out: Vec<Value> = libaw::actions::build_menu(&s, (x, y)).into_iter().map(|(u, c)| json!({"unit": u, "cost": c})).collect();
    json!({"units": out}).to_string()
}

/// Apply one action (contract format) for the player to move, if legal.
#[wasm_bindgen]
pub fn act(state: &str, action: &str, seed: u32) -> String {
    let mut s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let v: Value = match serde_json::from_str(action) { Ok(v) => v, Err(e) => return err(e) };
    let a = match action_from_contract(&s, &v) { Ok(a) => a, Err(e) => return err(e) };
    let label = describe(&s, &a);
    let mut next = rng(seed);
    let mut roll = |lo: i32, hi: i32| libaw::co::roll_luck(lo, hi, &mut next);
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
        let mut roll = |lo: i32, hi: i32| libaw::co::roll_luck(lo, hi, &mut next);
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

// ---------- the move planner (web/planner.html): the whole engine runs in the page ----------

/// A luck roll by planner mode. Rolls come in attack order (the strike, then the counter;
/// Sonja's Counter Break reverses them, and "best"/"worst" then favour the wrong side).
/// "best": the mover's strike rolls high and the counter low; "worst" the reverse;
/// "average": the middle of the range (rounded down); anything else rolls with the seed.
fn planner_roll(mode: &str, seed: u32) -> impl FnMut(i32, i32) -> i32 + '_ {
    let mut next = rng(seed);
    let mut n = 0u32;
    move |lo: i32, hi: i32| {
        n += 1;
        let first = n % 2 == 1;
        match mode {
            "best" => if first { hi } else { lo },
            "worst" => if first { lo } else { hi },
            "average" => (lo + hi).div_euclid(2),
            _ => libaw::co::roll_luck(lo, hi, &mut next),
        }
    }
}

/// Apply one action with the planner's luck mode ("random", "average", "best", "worst").
#[wasm_bindgen]
pub fn act_luck(state: &str, action: &str, mode: &str, seed: u32) -> String {
    let mut s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let v: Value = match serde_json::from_str(action) { Ok(v) => v, Err(e) => return err(e) };
    let a = match action_from_contract(&s, &v) { Ok(a) => a, Err(e) => return err(e) };
    let label = describe(&s, &a);
    let mut roll = planner_roll(mode, seed);
    if let Err(e) = sim::step(&mut s, &a, &mut roll) {
        return err(e);
    }
    done(&s, json!({"label": label}))
}

/// The server's `preview` (docs/socket-api.md): an attack played on copies at the lowest and the
/// highest luck rolls, as HP after. Nothing changes.
#[wasm_bindgen]
pub fn preview(state: &str, action: &str) -> String {
    use libaw::actions::Then;
    let s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let v: Value = match serde_json::from_str(action) { Ok(v) => v, Err(e) => return err(e) };
    let a = match action_from_contract(&s, &v) { Ok(a) => a, Err(e) => return err(e) };
    let Action::Move { unit, then: Then::Attack { target }, .. } = &a else { return err("preview takes a move that attacks") };
    let Some(def) = s.unit_at(target.0, target.1).map(|u| u.id) else { return err("no unit at the target") };
    let hp = |s: &State, id| s.units.get(&id).map_or(0, |u| u.hp.hi);
    let run = |high: bool| -> Result<Value, String> {
        let mut t = s.clone();
        let mut roll = |lo: i32, hi: i32| if high { hi } else { lo };
        sim::step(&mut t, &a, &mut roll)?;
        Ok(json!({"attacker_hp": hp(&t, *unit), "defender_hp": hp(&t, def)}))
    };
    match (run(false), run(true)) {
        (Ok(low), Ok(high)) => json!({"type": "preview", "target": [target.0, target.1], "attacker_hp": hp(&s, *unit),
            "defender_hp": hp(&s, def), "low_luck": low, "high_luck": high}).to_string(),
        (Err(e), _) | (_, Err(e)) => err(e),
    }
}

/// Each player's income at their next turn start: {"<player id>": funds}.
#[wasm_bindgen]
pub fn income(state: &str) -> String {
    let s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let m: serde_json::Map<String, Value> = s.players.iter().map(|p| {
        let n = s.buildings.values().filter(|b| b.owner == Some(p.id) && libaw::rules::is_income_property(b.terrain)).count() as i64;
        (p.id.to_string(), json!(n * (s.funds_per_property + libaw::co::property_fund_increase(&p.co))))
    }).collect();
    Value::Object(m).to_string()
}

/// A position from an AWBW turn file as it stands (HP ranges kept, unlike `start`), for planning
/// the turn of a game in progress.
#[wasm_bindgen]
pub fn position(turn_file: &str) -> String {
    let v: Value = match serde_json::from_str(turn_file) { Ok(v) => v, Err(e) => return err(e) };
    let s = state_from(&v);
    done(&s, json!({"header": log_header(&s, "")}))
}

/// The position as an AWBW turn file (the bot contract's state), using the turn file the planner
/// loaded as the template for what the engine doesn't model.
#[wasm_bindgen]
pub fn turn_file(state: &str, template: &str) -> String {
    let s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let t: Value = match serde_json::from_str(template) { Ok(v) => v, Err(e) => return err(e) };
    libaw::awbw::to_turn_json(&s, &t).to_string()
}

/// The engine's greedy baseline's next action for the player to move, in the contract format.
#[wasm_bindgen]
pub fn bot_move(state: &str) -> String {
    let s = match load(state) { Ok(s) => s, Err(e) => return err(e) };
    let a = baseline::ranked(&s).into_iter().next().map(|(_, a)| a).unwrap_or(Action::EndTurn);
    json!({"action": action_to_contract(&a), "label": describe(&s, &a)}).to_string()
}

#[cfg(test)]
mod planner_tests {
    use super::*;

    #[test]
    fn best_rolls_high_then_low() {
        let mut r = planner_roll("best", 1);
        assert_eq!((r(0, 9), r(0, 9)), (9, 0));
        let mut w = planner_roll("worst", 1);
        assert_eq!((w(0, 9), w(0, 9)), (0, 9));
        let mut a = planner_roll("average", 1);
        assert_eq!(a(-10, 9), -1);
    }
}
