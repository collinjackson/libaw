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
            meter: num(&p["players_co_power"]),
            cop_cost: num(&p["players_co_max_power"]),
            scop_cost: num(&p["players_co_max_spower"]),
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

// ---------------------------------------------------------------------------------------
// Writing: engine state -> AWBW turn-file JSON, so the referee can send any position to a
// bot that reads AWBW's format (docs/eval-harness/CONTRACT.md).

use crate::actions::{Action, Then};
use serde_json::{json, Map};

/// `s` as an AWBW turn file. `template` is a turn file of the same game (usually the one the
/// game started from): it supplies the terrain and per-player fields the engine doesn't
/// model (country, CO meter, portraits). Units, buildings, funds, powers, the player to move,
/// day and weather come from `s`. AWBW's derived indexes (`unit_map`, `players_units`, ...)
/// are dropped rather than left stale; the contract only promises the core keys.
pub fn to_turn_json(s: &State, template: &Value) -> Value {
    let mut v = template.clone();
    let tgs = &template["gameState"];
    let tplayers: Vec<&Value> = items(&tgs["players"]);
    let tp = |id: PlayerId| tplayers.iter().find(|p| num(&p["players_id"]) as u32 == id).copied();
    let country = |id: PlayerId| tp(id).and_then(|p| p["countries_name"].as_str()).unwrap_or("").to_string();
    let ccode = |id: PlayerId| tp(id).and_then(|p| p["countries_code"].as_str()).unwrap_or("").to_string();

    // Units: keep each unit's template object (extra fields), else one of its kind, else any.
    let tunits: Vec<&Value> = items(&tgs["units"]);
    let by_id: BTreeMap<u32, &Value> = tunits.iter().map(|u| (num(&u["units_id"]) as u32, *u)).collect();
    let mut units = Map::new();
    for u in s.units.values() {
        let base = by_id.get(&u.id).copied()
            .or_else(|| tunits.iter().find(|t| t["units_name"].as_str() == Some(&u.kind)).copied())
            .or_else(|| tunits.first().copied());
        let mut o = base.cloned().unwrap_or_else(|| json!({}));
        let info = tables().unit(&u.kind);
        let (x, y) = match u.carried_by.and_then(|t| s.units.get(&t)) {
            Some(t) => (t.x, t.y),
            None => (u.x, u.y),
        };
        let cargo = |i: usize| u.cargo.get(i).copied().unwrap_or(0);
        let fields = json!({
            "units_id": u.id, "units_players_id": u.owner, "units_name": u.kind,
            "units_movement_points": info.r#move, "units_vision": info.vision,
            "units_fuel": u.fuel, "units_fuel_per_turn": info.fuel_per_turn,
            "units_sub_dive": if u.dived { "Y" } else { "N" }, "units_ammo": u.ammo,
            "units_short_range": info.range[0], "units_long_range": info.range[1],
            "units_second_weapon": if info.second_weapon { "Y" } else { "N" },
            "units_cost": info.cost, "units_movement_type": info.move_type,
            "units_x": x, "units_y": y, "units_moved": u.moved as u8,
            "units_hit_points": u.hp.display_hi(),
            "units_cargo1_units_id": cargo(0), "units_cargo2_units_id": cargo(1),
            "units_carried": if u.carried_by.is_some() { "Y" } else { "N" },
            "countries_code": ccode(u.owner),
        });
        if let (Value::Object(o), Value::Object(f)) = (&mut o, fields) {
            o.extend(f);
        }
        units.insert(u.id.to_string(), o);
    }

    // Buildings, nested x -> y like AWBW's. Tile ids carry the owner's country.
    let tb: BTreeMap<(u16, u16), &Value> = items(&tgs["buildings"]).into_iter()
        .flat_map(items).filter(|b| !b.is_null())
        .map(|b| ((num(&b["buildings_x"]) as u16, num(&b["buildings_y"]) as u16), b)).collect();
    let mut cols: BTreeMap<u16, Map<String, Value>> = BTreeMap::new();
    for (&(x, y), b) in &s.buildings {
        let mut o = tb.get(&(x, y)).map(|v| (*v).clone()).unwrap_or_else(|| json!({}));
        let tid = owned_tile(b.terrain, b.owner.map(&country).as_deref());
        let f = json!({
            "buildings_x": x, "buildings_y": y, "buildings_capture": b.capture,
            "buildings_players_id": b.owner, "terrain_id": tid.to_string(),
            "terrain_name": tables().tile(tid).name,
            "buildings_team": b.owner.map(|o| s.team_of(o)),
            "countries_code": b.owner.map(&ccode).unwrap_or_default(),
        });
        if let (Value::Object(o), Value::Object(f)) = (&mut o, f) {
            o.extend(f);
        }
        cols.entry(x).or_default().insert(y.to_string(), o);
    }

    // Players: funds, power, elimination, income (from which the reader derives funds per
    // property).
    let income = |p: &Player| {
        let n = s.buildings.values()
            .filter(|b| b.owner == Some(p.id) && crate::rules::is_income_property(b.terrain)).count() as i64;
        n * (s.funds_per_property + crate::co::property_fund_increase(&p.co))
    };
    let mut gplayers = Map::new();
    for p in &s.players {
        let mut o = tp(p.id).cloned().unwrap_or_else(|| json!({}));
        let f = json!({
            "players_id": p.id, "players_team": p.team, "co_name": p.co, "players_funds": p.funds,
            "players_eliminated": if p.eliminated { "Y" } else { "N" }, "players_order": p.order,
            "players_co_power_on": p.power, "players_income": income(p),
            "players_co_power": p.meter, "players_co_max_power": p.cop_cost, "players_co_max_spower": p.scop_cost,
        });
        if let (Value::Object(o), Value::Object(f)) = (&mut o, f) {
            o.extend(f);
        }
        gplayers.insert(p.id.to_string(), o);
    }
    let tops: Vec<Value> = s.players.iter().map(|p| {
        let mut o = items(&template["players"]).into_iter()
            .find(|q| num(&q["id"]) as u32 == p.id).cloned().unwrap_or_else(|| json!({}));
        let f = json!({"id": p.id, "funds": p.funds, "order": p.order, "team": p.team,
            "eliminated": if p.eliminated { "Y" } else { "N" }, "co_power_on": p.power});
        if let (Value::Object(o), Value::Object(f)) = (&mut o, f) {
            o.extend(f);
        }
        o
    }).collect();

    let gs = v["gameState"].as_object_mut().expect("template has gameState");
    for k in ["unit_map", "players_units", "players_units_count", "players_buildings"] {
        gs.remove(k);
    }
    gs.insert("units".into(), Value::Object(units));
    gs.insert("buildings".into(), Value::Object(
        cols.into_iter().map(|(x, m)| (x.to_string(), Value::Object(m))).collect()));
    gs.insert("players".into(), Value::Object(gplayers));
    gs.insert("currentTurnPId".into(), json!(s.current));
    let mut weather = tgs["gameWeather"].clone();
    if !weather.is_object() {
        weather = json!({});
    }
    weather["code"] = json!(s.weather.to_string());
    gs.insert("gameWeather".into(), weather);
    v["day"] = json!(s.day);
    v["players"] = Value::Array(tops);
    v
}

/// The tile id for building `tid` owned by the country named `country` (None: neutral).
/// A captured HQ becomes the captor's city, as on AWBW.
pub fn owned_tile(tid: u16, country: Option<&str>) -> u16 {
    let t = tables().tile(tid);
    let Some(kind) = t.building.as_deref() else { return tid };
    if matches!(kind, "Missile" | "PipeSeam") {
        return tid;
    }
    let prefix = country.unwrap_or("Neutral");
    if t.name.starts_with(prefix) {
        return tid;
    }
    let want = if kind == "HQ" { "City" } else { kind };
    tables().terrain.iter()
        .filter(|(_, i)| i.building.as_deref() == Some(want) && i.name.starts_with(prefix))
        .map(|(&id, _)| id).min().unwrap_or(tid)
}

/// What the turn files can show of a state, as one comparable string: positions, displayed
/// HP, fuel, ammo, flags, cargo, buildings, funds, powers, turn. Round-trip tests compare it.
pub fn fingerprint(s: &State) -> String {
    let mut out = format!("cur {} day {} weather {}\n", s.current, s.day, s.weather);
    for p in &s.players {
        out += &format!("p {} {} {} {} {} {}\n", p.id, p.team, p.co, p.funds, p.power, p.eliminated);
    }
    for u in s.units.values() {
        // A carried unit's own x/y isn't shown; it rides at its transport's tile.
        let (x, y) = u.carried_by.and_then(|t| s.units.get(&t)).map(|t| (t.x, t.y)).unwrap_or((u.x, u.y));
        out += &format!("u {} {} {} {},{} hp{} f{} a{} m{} c{:?} in{:?} d{}\n", u.id, u.owner, u.kind,
            x, y, u.hp.display_hi(), u.fuel, u.ammo, u.moved, u.cargo, u.carried_by, u.dived);
    }
    for (p, b) in &s.buildings {
        out += &format!("b {:?} {:?} {} {}\n", p, b.owner, b.capture, tables().tile(b.terrain).building.as_deref().unwrap_or(""));
    }
    out += &format!("fpp {}\n", s.funds_per_property);
    out
}

// ---------------------------------------------------------------------------------------
// The contract's action format <-> engine actions.

fn xy(v: &Value) -> Option<(u16, u16)> {
    let a = v.as_array()?;
    Some((a.first()?.as_u64()? as u16, a.get(1)?.as_u64()? as u16))
}

pub fn action_to_contract(a: &Action) -> Value {
    match a {
        Action::EndTurn => json!({"type": "end_turn"}),
        Action::Build { at, kind } => json!({"type": "build", "at": [at.0, at.1], "unit": kind}),
        Action::Move { unit, path, then } => {
            let then = match then {
                Then::Wait => json!({"type": "wait"}),
                Then::Attack { target } => json!({"type": "attack", "target": [target.0, target.1]}),
                Then::Capture => json!({"type": "capture"}),
                Then::Load => json!({"type": "load"}),
                Then::Join => json!({"type": "join"}),
                Then::Supply => json!({"type": "supply"}),
                Then::Repair { target } => json!({"type": "repair", "target": [target.0, target.1]}),
                Then::Unload { cargo, to } => json!({"type": "unload", "cargo_id": cargo, "to": [to.0, to.1]}),
            };
            json!({"type": "move", "unit_id": unit,
                   "path": path.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>(), "then": then})
        }
    }
}

/// A contract reply as one of the legal actions in `s`. A move matches a legal move of the
/// same unit to the same tile with the same follow-up; when its path differs from the
/// engine's (several cheapest paths), the reply's path is kept if it's a valid path. Err
/// says why the reply isn't legal here.
pub fn action_from_contract(s: &State, v: &Value) -> Result<Action, String> {
    let legal = crate::actions::legal_actions(s);
    match v["type"].as_str() {
        Some("end_turn") => Ok(Action::EndTurn),
        Some("build") => {
            let at = xy(&v["at"]).ok_or("build: bad at")?;
            let kind = v["unit"].as_str().ok_or("build: no unit")?.to_string();
            let a = Action::Build { at, kind };
            legal.contains(&a).then_some(a).ok_or_else(|| "build: not legal here".into())
        }
        Some("move") => {
            let unit = v["unit_id"].as_u64().ok_or("move: no unit_id")? as u32;
            let path: Vec<(u16, u16)> = v["path"].as_array().ok_or("move: no path")?
                .iter().map(xy).collect::<Option<_>>().ok_or("move: bad path")?;
            let end = *path.last().ok_or("move: empty path")?;
            let t = &v["then"];
            let then = match t["type"].as_str() {
                Some("wait") => Then::Wait,
                Some("attack") => Then::Attack { target: xy(&t["target"]).ok_or("attack: bad target")? },
                Some("capture") => Then::Capture,
                Some("load") => Then::Load,
                Some("join") => Then::Join,
                Some("supply") => Then::Supply,
                Some("repair") => Then::Repair { target: xy(&t["target"]).ok_or("repair: bad target")? },
                Some("unload") => Then::Unload {
                    cargo: t["cargo_id"].as_u64().ok_or("unload: no cargo_id")? as u32,
                    to: xy(&t["to"]).ok_or("unload: bad to")?,
                },
                other => return Err(format!("move: unknown then {other:?}")),
            };
            let m = legal.iter().find(|a| matches!(a, Action::Move { unit: u, path: p, then: th }
                if *u == unit && p.last() == Some(&end) && *th == then))
                .ok_or_else(|| format!("move: not legal here ({unit} to {end:?}, {then:?})"))?;
            let Action::Move { path: lp, .. } = m else { unreachable!() };
            let keep = path.len() >= 1 && path[0] == lp[0] && path_ok(s, unit, &path, lp);
            Ok(Action::Move { unit, path: if keep { path } else { lp.clone() }, then })
        }
        other => Err(format!("unknown type {other:?}")),
    }
}

/// A reply's path is usable if it's connected, passes no enemy, and costs no more than the
/// engine's cheapest path to the same tile (both are within the unit's movement).
fn path_ok(s: &State, unit: UnitId, path: &[(u16, u16)], best: &[(u16, u16)]) -> bool {
    let Some(u) = s.units.get(&unit) else { return false };
    let cost = |p: &[(u16, u16)]| -> Option<u32> {
        let mut c = 0;
        for w in p.windows(2) {
            if crate::rules::dist(w[0], w[1]) != 1 {
                return None;
            }
            if s.unit_at(w[1].0, w[1].1).is_some_and(|o| !s.allied(o.owner, u.owner)) {
                return None;
            }
            c += crate::rules::step_cost(s, u, s.terrain_at(w[1].0, w[1].1))?;
        }
        Some(c)
    };
    matches!((cost(path), cost(best)), (Some(a), Some(b)) if a <= b)
}
