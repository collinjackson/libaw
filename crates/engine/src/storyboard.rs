//! A turn as a storyboard: the board at the turn's start plus its actions as plain steps, for
//! replay viewers and highlight reels (docs/storyboard.md). Built from an AWBW turn record: its
//! `gameState` is the turn's start and its `actions` what happened, in order, with AWBW's own
//! results (damage, captures), so a viewer shows what the game recorded, not a re-simulation.

use crate::awbw::{hp_display, items, num, state_from, unit_from};
use crate::state::Hp;
use serde_json::{json, Value};

pub const FORMAT: &str = "libaw-storyboard";
pub const VERSION: u32 = 1;

fn xy(v: &Value, x: &str, y: &str) -> Value {
    json!([num(&v[x]), num(&v[y])])
}

/// HP after an action, as the engine's range for a displayed value (AWBW records display HP).
fn hp_json(v: &Value) -> Value {
    serde_json::to_value(Hp::from_display(hp_display(v))).unwrap_or(Value::Null)
}

fn move_step(m: &Value) -> Option<Value> {
    let path: Vec<Value> = items(&m["path"]).into_iter().map(|p| json!([num(&p["x"]), num(&p["y"])])).collect();
    let id = num(&m["unit"]["units_id"]);
    if path.is_empty() || id == 0 {
        return None;
    }
    Some(json!({"type": "move", "unit_id": id, "path": path,
                "trapped": m["trapped"].as_bool().unwrap_or(false)}))
}

fn side(v: &Value) -> Value {
    if !v.is_object() {
        return Value::Null; // "?" in fog
    }
    json!({"unit_id": num(&v["units_id"]), "at": xy(v, "units_x", "units_y"), "hp": hp_json(&v["units_hit_points"]),
           "display_hp": hp_display(&v["units_hit_points"])})
}

/// Steps of one AWBW action record. A record that carries its own Move (archive form:
/// {"action":"Fire","Move":{..},"Fire":{..}}) gives the move first.
fn steps_of(a: &Value, out: &mut Vec<Value>) {
    let kind = a["action"].as_str().unwrap_or("");
    if kind != "Move" && a["Move"].is_object() {
        if let Some(m) = move_step(&a["Move"]) {
            out.push(m);
        }
    }
    let body = if a[kind].is_object() { &a[kind] } else { a };
    match kind {
        "Move" => out.extend(move_step(a)),
        "Fire" => out.push(json!({"type": "fire", "attacker": side(&body["attacker"]), "defender": side(&body["defender"])})),
        "Capt" => {
            let b = &body["buildingInfo"];
            let left = num(&b["buildings_capture"]);
            // A finished capture carries the building's new terrain (terrain_id) and its new owner in
            // buildings_team. A partial capture of an enemy building also names a team, but it's the
            // current owner, and there is no terrain_id.
            let team = &b["buildings_team"];
            let done = !team.is_null() && team.as_str() != Some("") && !b["terrain_id"].is_null();
            out.push(json!({"type": "capture", "at": xy(b, "buildings_x", "buildings_y"), "capture": left,
                            "done": done, "new_owner": if done { team.clone() } else { Value::Null }}));
        }
        "Build" => {
            let u = unit_from(&body["newUnit"]);
            out.push(json!({"type": "build", "unit": u}));
        }
        "Power" => out.push(json!({"type": "power", "player": num(&body["playerId"]), "co": body["coName"],
                                   "name": body["powerName"], "super": body["coPower"].as_str() == Some("S")})),
        "Join" => {
            let u = unit_from(&body["joinedUnit"]);
            out.push(json!({"type": "join", "from": num(&body["joinId"]), "into": u.id, "unit": u}));
        }
        "NextTurn" | "End" => out.push(json!({"type": "end", "next": num(&body["nextPId"])})),
        "" => {}
        other => out.push(json!({"type": "other", "action": other})),
    }
}

/// The storyboard of one AWBW turn record (a live `turn-NNN-*.json`).
pub fn from_awbw_turn(turn: &Value) -> Value {
    let start = state_from(turn);
    let mut steps = Vec::new();
    for a in items(&turn["actions"]) {
        steps_of(a, &mut steps);
    }
    let names: serde_json::Map<String, Value> = items(&turn["gameState"]["players"]).into_iter()
        .map(|p| (num(&p["players_id"]).to_string(), p["users_username"].clone()))
        .collect();
    let countries: serde_json::Map<String, Value> = items(&turn["gameState"]["players"]).into_iter()
        .map(|p| (num(&p["players_id"]).to_string(), p["countries_code"].clone()))
        .collect();
    json!({"format": FORMAT, "version": VERSION, "day": start.day, "player": start.current,
           "names": names, "countries": countries, "start": start, "steps": steps})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(id: u32, owner: u32, kind: &str, x: u16, y: u16, hp: u8) -> Value {
        json!({"units_id": id, "units_players_id": owner, "units_name": kind, "units_x": x, "units_y": y,
               "units_hit_points": hp, "units_fuel": 99, "units_ammo": 9, "units_moved": 0,
               "units_cargo1_units_id": 0, "units_cargo2_units_id": 0, "units_sub_dive": "N"})
    }

    #[test]
    fn steps_follow_the_record() {
        let acts = json!([
            {"action": "Move", "path": [{"x": 1, "y": 1}, {"x": 2, "y": 1}], "unit": unit(7, 1, "Tank", 2, 1, 10), "trapped": false},
            {"action": "Fire", "attacker": {"units_id": 7, "units_x": 2, "units_y": 1, "units_hit_points": 9},
             "defender": {"units_id": 8, "units_x": 3, "units_y": 1, "units_hit_points": 0}},
            {"action": "Capt", "buildingInfo": {"buildings_x": 4, "buildings_y": 4, "buildings_capture": 10, "buildings_team": null}},
            {"action": "Build", "newUnit": unit(9, 1, "Infantry", 0, 0, 10)},
            {"action": "Fire", "Move": {"path": [{"x": 5, "y": 5}], "unit": unit(10, 1, "Artillery", 5, 5, 10)},
             "Fire": {"attacker": {"units_id": 10, "units_x": 5, "units_y": 5, "units_hit_points": 10}, "defender": "?"}},
            {"action": "NextTurn", "nextPId": 2},
        ]);
        let mut out = Vec::new();
        for a in items(&acts) {
            steps_of(a, &mut out);
        }
        let kinds: Vec<&str> = out.iter().map(|s| s["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["move", "fire", "capture", "build", "move", "fire", "end"]);
        assert_eq!(out[0]["path"], json!([[1, 1], [2, 1]]));
        assert_eq!(out[1]["defender"]["display_hp"], 0);
        assert_eq!(out[1]["attacker"]["display_hp"], 9);
        assert_eq!(out[2]["done"], false);
        assert_eq!(out[3]["unit"]["kind"], "Infantry");
        assert!(out[5]["defender"].is_null());
    }

    #[test]
    fn a_partial_capture_of_an_enemy_building_is_not_done() {
        // Mid-capture of an owned building: the record names the current owner, with no terrain.
        let partial = json!({"action": "Capt", "buildingInfo": {"buildings_x": 3, "buildings_y": 2,
            "buildings_capture": 5, "buildings_team": "2"}});
        let done = json!({"action": "Capt", "buildingInfo": {"buildings_x": 3, "buildings_y": 2,
            "buildings_capture": 20, "buildings_team": "1", "terrain_id": 48, "buildings_players_id": 1}});
        let mut out = Vec::new();
        steps_of(&partial, &mut out);
        steps_of(&done, &mut out);
        assert_eq!(out[0]["done"], false);
        assert!(out[0]["new_owner"].is_null());
        assert_eq!(out[1]["done"], true);
        assert_eq!(out[1]["new_owner"], "1");
    }
}
