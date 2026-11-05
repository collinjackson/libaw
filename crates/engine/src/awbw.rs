//! Reading AWBW replay-API turn files (one JSON file per player turn, as saved by
//! awbw_chart's awbw_replay_export.py) into engine state.

use crate::data::tables;
use crate::*;
use serde_json::Value;
use std::collections::BTreeMap;

/// PHP arrays come through as either JSON objects or lists. Iterate either.
pub fn items(v: &Value) -> Vec<&Value> {
    match v {
        Value::Object(m) => m.values().collect(),
        Value::Array(a) => a.iter().collect(),
        _ => vec![],
    }
}

pub fn num(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or_else(|| n.as_f64().unwrap_or(0.0).round() as i64),
        Value::String(s) => s.trim().parse::<f64>().map(|f| f.round() as i64).unwrap_or(0),
        _ => 0,
    }
}

pub fn opt_num(v: &Value) -> Option<i64> {
    match v {
        Value::Null => None,
        Value::String(s) if s.trim().is_empty() => None,
        _ => Some(num(v)),
    }
}

pub fn hp_display(v: &Value) -> u8 {
    // Displayed HP; fractional values (if any) round up the way AWBW displays them.
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0).ceil() as u8,
        _ => num(v) as u8,
    }
}

pub fn unit_from(v: &Value) -> Unit {
    Unit {
        id: num(&v["units_id"]) as u32,
        owner: num(&v["units_players_id"]) as u32,
        kind: v["units_name"].as_str().unwrap_or("?").to_string(),
        x: num(&v["units_x"]) as u16,
        y: num(&v["units_y"]) as u16,
        hp: Hp::from_display(hp_display(&v["units_hit_points"])),
        fuel: num(&v["units_fuel"]) as i32,
        ammo: num(&v["units_ammo"]) as i32,
        moved: num(&v["units_moved"]) != 0,
        carried_by: None,
        cargo: [&v["units_cargo1_units_id"], &v["units_cargo2_units_id"]]
            .iter()
            .map(|c| num(c) as u32)
            .filter(|&c| c != 0)
            .collect(),
        dived: v["units_sub_dive"].as_str() == Some("Y"),
    }
}

pub fn state_from(turn: &Value) -> State {
    let gs = &turn["gameState"];
    let mut terrain_cells = vec![];
    let (mut w, mut h) = (0u16, 0u16);
    for col in items(&gs["terrain"]) {
        for c in items(col) {
            if c.is_null() {
                continue;
            }
            let (x, y) = (num(&c["tiles_x"]) as u16, num(&c["tiles_y"]) as u16);
            w = w.max(x + 1);
            h = h.max(y + 1);
            terrain_cells.push((x, y, num(&c["terrain_id"]) as u16));
        }
    }
    let mut buildings = BTreeMap::new();
    for col in items(&gs["buildings"]) {
        for b in items(col) {
            if b.is_null() {
                continue;
            }
            let (x, y) = (num(&b["buildings_x"]) as u16, num(&b["buildings_y"]) as u16);
            w = w.max(x + 1);
            h = h.max(y + 1);
            buildings.insert(
                (x, y),
                Building {
                    terrain: num(&b["terrain_id"]) as u16,
                    owner: opt_num(&b["buildings_players_id"]).map(|p| p as u32),
                    capture: num(&b["buildings_capture"]) as i32,
                },
            );
        }
    }
    let mut terrain = vec![1u16; w as usize * h as usize];
    for (x, y, t) in terrain_cells {
        terrain[y as usize * w as usize + x as usize] = t;
    }
    let mut units: BTreeMap<UnitId, Unit> = BTreeMap::new();
    for u in items(&gs["units"]) {
        let u = unit_from(u);
        units.insert(u.id, u);
    }
    let carriers: Vec<(UnitId, Vec<UnitId>)> =
        units.values().map(|u| (u.id, u.cargo.clone())).collect();
    for (t, cargo) in carriers {
        for c in cargo {
            if let Some(cu) = units.get_mut(&c) {
                cu.carried_by = Some(t);
            }
        }
    }
    // Player funds live in the top-level list; team/CO in gameState.players.
    let mut players: Vec<Player> = items(&gs["players"])
        .into_iter()
        .map(|p| Player {
            id: num(&p["players_id"]) as u32,
            team: p["players_team"].as_str().map(str::to_string)
                .unwrap_or_else(|| num(&p["players_id"]).to_string()),
            co: p["co_name"].as_str().unwrap_or("").to_string(),
            funds: num(&p["players_funds"]),
            eliminated: p["players_eliminated"].as_str() == Some("Y"),
            order: num(&p["players_order"]) as u32,
            power: p["players_co_power_on"].as_str().unwrap_or("N").to_string(),
        })
        .collect();
    for p in items(&turn["players"]) {
        let id = num(&p["id"]) as u32;
        if let Some(pl) = players.iter_mut().find(|q| q.id == id) {
            pl.funds = num(&p["funds"]);
            if pl.order == 0 {
                pl.order = num(&p["order"]) as u32;
            }
        }
    }
    players.sort_by_key(|p| p.order);
    let current = num(&gs["currentTurnPId"]) as u32;
    let mut s = State {
        width: w,
        height: h,
        terrain,
        buildings,
        units,
        players,
        current,
        day: num(&turn["day"]) as u32,
        funds_per_property: 1000,
        move_override: BTreeMap::new(),
        weather: gs["gameWeather"]["code"].as_str().and_then(|c| c.chars().next()).unwrap_or('C'),
    };
    // Funds per property: the current player's income over its income properties.
    if let Some(p) = items(&gs["players"]).into_iter().find(|p| num(&p["players_id"]) as u32 == current) {
        let n = s.buildings.values()
            .filter(|b| b.owner == Some(current) && crate::rules::is_income_property(b.terrain))
            .count() as i64;
        let inc = num(&p["players_income"]);
        if n > 0 && inc > 0 && inc % n == 0 {
            let co = p["co_name"].as_str().unwrap_or("");
            s.funds_per_property = inc / n - crate::co::property_fund_increase(co);
        }
    }
    let _ = tables();
    s
}
