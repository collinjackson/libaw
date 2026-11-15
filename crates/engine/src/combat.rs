//! Attack and defence values: COs, com towers, terrain. Shared by the simulator and the
//! replay checker.

#[allow(unused_imports)]
use crate::prelude::*;
use crate::co;
use crate::data::tables;
use crate::rules::*;
use crate::state::*;

pub fn co_of(s: &State, p: PlayerId) -> String {
    s.player(p).map(|p| p.co.clone()).unwrap_or_default()
}

pub fn towers(s: &State, p: PlayerId) -> u32 {
    s.buildings.values()
        .filter(|b| b.owner == Some(p) && tables().tile(b.terrain).building == Some(crate::data::BuildingKind::ComTower))
        .count() as u32
}

/// Kindle's High Society: +3% attack per property owned: HQ, cities, bases, airports,
/// ports, com towers and labs (AWBW's rule for games started after 2024-08-13).
fn kindle_count(s: &State, p: PlayerId) -> i32 {
    s.buildings.values().filter(|b| b.owner == Some(p) && crate::rules::is_urban_property(b.terrain)).count() as i32
}

/// The owner's active power: "N", "Y" (CO power) or "S" (super power).
pub fn pw(s: &State, p: PlayerId) -> String {
    s.player(p).map(|p| p.power.clone()).unwrap_or_else(|| "N".into())
}

/// Attack and defence values (100 = neutral) from COs' day-to-day abilities and com towers.
/// `counter`: the unit is striking back. Sonja's counterattacks (and Kanbei's under Samurai
/// Spirit) multiply the CO's own day-to-day attack value by 1.5; com towers, the power
/// bonus and luck are not multiplied.
pub fn att_value(s: &State, att: &Unit, att_terrain: u16, counter: bool) -> u32 {
    let co = co_of(s, att.owner);
    let power = pw(s, att.owner);
    let t = tables().tile(att_terrain);
    let stars = stars(s, att, att_terrain);
    let counter_bonus = if counter {
        (100 + co::d2d_attack_bonus(&co, att.kind)) * (co::counter_pct(&co, &power) as i32 - 100) / 100
    } else {
        0
    };
    let v = 100 + 10 * towers(s, att.owner) as i32 * tower_multiplier(s, att.owner)
        + co::attack_bonus(&co, &power, att.kind, &bonus_terrain(att, att_terrain, t.kind.name()), stars)
        + counter_bonus
        + if co == "Kindle" && power == "S" { 3 * kindle_count(s, att.owner) } else { 0 };
    v.max(0) as u32
}

/// Colin's Power of Money: +3% attack per 1000 funds he holds when the attack is made
/// (not when the power was activated), in thousandths of a per cent point, unrounded.
pub fn att_milli(s: &State, att: &Unit) -> u32 {
    match s.player(att.owner) {
        Some(p) if p.co == "Colin" && p.power == "S" => (3 * p.funds.max(0)) as u32,
        _ => 0,
    }
}

pub fn def_value(s: &State, def: &Unit, attacker_kind: crate::data::UnitType) -> u32 {
    let co = co_of(s, def.owner);
    let mut v: i32 = 100 + co::defense_bonus(&co, &pw(s, def.owner), def.kind);
    if co == "Javier" {
        let m = tower_multiplier(s, def.owner);
        v += 10 * towers(s, def.owner) as i32 * m;
        if is_indirect(attacker_kind) {
            // +20 against indirect fire, +40 under Tower Shield, +80 under Tower of Power.
            v += match pw(s, def.owner).as_str() { "Y" => 40, "S" => 80, _ => 20 };
        }
    }
    v as u32
}

/// Javier's Tower Shield doubles his com tower (and indirect-defence) bonuses; Tower of
/// Power triples them. Everyone else: 1.
pub fn tower_multiplier(s: &State, p: PlayerId) -> i32 {
    if co_of(s, p) != "Javier" {
        return 1;
    }
    match pw(s, p).as_str() {
        "Y" => 2,
        "S" => 3,
        _ => 1,
    }
}

/// The terrain kind a CO's terrain-conditional bonus sees (Koal roads, Jake plains,
/// Kindle buildings). A bridge isn't a road here, and an emptied missile silo isn't plain.
pub fn bonus_terrain(u: &Unit, terrain: u16, kind: &str) -> String {
    let _ = u;
    let name = &tables().tile(terrain).name;
    if name.ends_with("Bridge") {
        return "Bridge".into();
    }
    if name.starts_with("Missile Silo") {
        return "Silo".into();
    }
    // Pipes are buildings in the terrain table, but Kindle's urban bonus is for properties.
    if kind == "Building" && !crate::rules::is_urban_property(terrain) {
        return "Pipe".into();
    }
    kind.to_string()
}

/// Terrain stars a unit gets; Lash's Prime Tactics doubles them.
pub fn stars(s: &State, u: &Unit, terrain: u16) -> u32 {
    let st = terrain_stars(terrain, u.kind);
    if co_of(s, u.owner) == "Lash" && pw(s, u.owner) == "S" { st * 2 } else { st }
}

