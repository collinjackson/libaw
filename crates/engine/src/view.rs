//! What a viewer needs: compact per-action frames, a game log header, action labels.

use crate::data::tables;
use crate::state::*;
use serde_json::Value;
use std::collections::BTreeMap;

/// One frame of a game log: the position after an action.
pub fn frame(s: &State, label: &str) -> Value {
    let units: Vec<Value> = s.units.values().filter(|u| u.carried_by.is_none())
        .map(|u| serde_json::json!([u.id, u.owner, u.kind, u.x, u.y, u.hp.display_hi(), u.cargo.len()])).collect();
    let buildings: Vec<Value> = s.buildings.iter()
        .map(|(&(x, y), b)| serde_json::json!([x, y, b.owner, b.terrain, b.capture])).collect();
    let funds: Vec<Value> = s.players.iter().map(|p| serde_json::json!([p.id, p.funds])).collect();
    serde_json::json!({"a": label, "day": s.day, "cur": s.current, "u": units, "b": buildings, "f": funds})
}

/// Static part of a game log: the map and players.
pub fn log_header(s: &State, title: &str) -> Value {
    let names: BTreeMap<u16, Value> = s.terrain.iter().chain(s.buildings.values().map(|b| &b.terrain))
        .map(|&t| (t, serde_json::json!({"name": tables().tile(t).name, "kind": tables().tile(t).kind,
            "building": tables().tile(t).building})))
        .collect();
    serde_json::json!({
        "title": title, "w": s.width, "h": s.height, "terrain": s.terrain, "tiles": names,
        "players": s.players.iter().map(|p| serde_json::json!({"id": p.id, "co": p.co, "team": p.team})).collect::<Vec<_>>(),
    })
}

pub fn describe(s: &State, a: &crate::actions::Action) -> String {
    use crate::actions::{Action, Then};
    match a {
        Action::EndTurn => "end turn".into(),
        Action::Power { kind } => if kind == "S" { "super power".into() } else { "CO power".into() },
        Action::Build { at, kind } => format!("build {kind} at {at:?}"),
        Action::Move { unit, path, then } => {
            let k = s.units.get(unit).map(|u| u.kind.clone()).unwrap_or_default();
            let to = path.last().copied().unwrap_or_default();
            let t = match then {
                Then::Wait => "wait".to_string(),
                Then::Attack { target } => format!("attack {target:?}"),
                Then::AttackSeam { target } => format!("attack seam {target:?}"),
                Then::Capture => "capture".into(),
                Then::Load => "load".into(),
                Then::Join => "join".into(),
                Then::Supply => "supply".into(),
                Then::Hide => if k == "Sub" { "dive".into() } else { "hide".into() },
                Then::Unhide => "surface".into(),
                Then::Launch { target } => format!("launch silo at {target:?}"),
                Then::Repair { target } => format!("repair {target:?}"),
                Then::Unload { to, .. } => format!("unload to {to:?}"),
            };
            format!("{k} {:?} -> {to:?}, {t}", path.first().copied().unwrap_or_default())
        }
    }
}
