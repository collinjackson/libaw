//! Attack and defence values: COs, com towers, terrain. Shared by the simulator and the
//! replay checker.

use crate::co;
use crate::data::tables;
use crate::rules::*;
use crate::state::*;

pub fn co_of(s: &State, p: PlayerId) -> String {
    s.player(p).map(|p| p.co.clone()).unwrap_or_default()
}

pub fn towers(s: &State, p: PlayerId) -> u32 {
    s.buildings.values()
        .filter(|b| b.owner == Some(p) && tables().tile(b.terrain).building.as_deref() == Some("ComTower"))
        .count() as u32
}

/// The owner's active power: "N", "Y" (CO power) or "S" (super power).
pub fn pw(s: &State, p: PlayerId) -> String {
    s.player(p).map(|p| p.power.clone()).unwrap_or_else(|| "N".into())
}

/// Attack and defence values (100 = neutral) from COs' day-to-day abilities and com towers.
pub fn att_value(s: &State, att: &Unit, att_terrain: u16) -> u32 {
    let co = co_of(s, att.owner);
    let t = tables().tile(att_terrain);
    let stars = stars(s, att, att_terrain);
    let v = 100 + 10 * towers(s, att.owner) as i32 * tower_multiplier(s, att.owner)
        + co::attack_bonus(&co, &pw(s, att.owner), &att.kind, &bonus_terrain(att, att_terrain, &t.kind), stars)
        + s.player(att.owner).filter(|p| p.power != "N").map_or(0, |p| p.power_attack);
    v.max(0) as u32
}

pub fn def_value(s: &State, def: &Unit, attacker_kind: &str) -> u32 {
    let co = co_of(s, def.owner);
    let mut v: i32 = 100 + co::defense_bonus(&co, &pw(s, def.owner), &def.kind);
    if co == "Javier" {
        let m = tower_multiplier(s, def.owner);
        v += 10 * towers(s, def.owner) as i32 * m;
        if is_indirect(attacker_kind) {
            v += 20 * m;
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
    kind.to_string()
}

/// Terrain stars a unit gets; Lash's Prime Tactics doubles them.
pub fn stars(s: &State, u: &Unit, terrain: u16) -> u32 {
    let st = terrain_stars(terrain, &u.kind);
    if co_of(s, u.owner) == "Lash" && pw(s, u.owner) == "S" { st * 2 } else { st }
}

