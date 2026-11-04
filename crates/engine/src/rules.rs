//! Rules: movement, capture, combat, start of turn.
//!
//! Sources are AWBW's own pages and recorded games (see docs/sources.md). Anything still
//! unmodelled is listed in NOTES.md.

use crate::data::tables;
use crate::state::*;

pub const CAPTURE_POINTS: i32 = 20;

pub fn move_cost(terrain: u16, move_type: &str) -> Option<u8> {
    tables().tile(terrain).r#move.get(move_type).copied()
}

/// Cost of walking `path` (which starts at the unit's tile). Checks adjacency, bounds,
/// passability, movement range and enemy blocking. Returns the movement cost spent.
pub fn path_cost(s: &State, u: &Unit, path: &[(u16, u16)]) -> Result<u32, String> {
    let info = tables().unit(&u.kind);
    let mut cost = 0u32;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = (a.0 as i32 - b.0 as i32).abs() + (a.1 as i32 - b.1 as i32).abs();
        if d != 1 {
            return Err(format!("path step {a:?}->{b:?} not adjacent"));
        }
        let t = s.terrain_at(b.0, b.1);
        let c = move_cost(t, &info.move_type)
            .ok_or_else(|| format!("{} can't enter {} at {b:?}", u.kind, tables().tile(t).name))?;
        cost += c as u32;
        if let Some(o) = s.unit_at(b.0, b.1) {
            if !s.allied(o.owner, u.owner) {
                return Err(format!("path through enemy {} at {b:?}", o.kind));
            }
        }
    }
    Ok(cost)
}

pub fn movement_range(s: &State, u: &Unit) -> u32 {
    let mut m = tables().unit(&u.kind).r#move as u32;
    let co = s.player(u.owner).map(|p| p.co.as_str()).unwrap_or("");
    // Day-to-day movement changes.
    if co == "Sami" && matches!(u.kind.as_str(), "APC" | "T-Copter" | "Lander" | "Black Boat") {
        m += 1;
    }
    m
}

/// How many capture points a unit takes off a building per capture action.
pub fn capture_amount(s: &State, u: &Unit, display_hp: u8) -> i32 {
    let co = s.player(u.owner).map(|p| p.co.as_str()).unwrap_or("");
    let hp = display_hp as i32;
    if co == "Sami" {
        hp * 3 / 2
    } else {
        hp
    }
}

pub fn is_income_property(terrain: u16) -> bool {
    matches!(
        tables().tile(terrain).building.as_deref(),
        Some("City" | "Base" | "Airport" | "Port" | "HQ")
    )
}

/// Which unit classes a building repairs and resupplies.
pub fn services(terrain: u16, kind: &str) -> bool {
    let mt = tables().unit(kind).move_type.as_str();
    match tables().tile(terrain).building.as_deref() {
        Some("City" | "HQ") => matches!(mt, "F" | "B" | "T" | "W"),
        Some("Base") => matches!(mt, "F" | "B" | "T" | "W" | "P"),
        Some("Airport") => mt == "A",
        Some("Port") => matches!(mt, "S" | "L"),
        _ => false,
    }
}

/// Combat: the range of damage (internal HP points) an attack can do, before rounding
/// questions are settled. AWBW's formula:
///   (B * ACO/100 + L) * (AHP/10) * (200 - (DCO + DTR*DHP)) / 100, L = luck 0..=9.
pub struct AttackParams {
    pub base: u32,
    pub att_value: u32,
    pub att_hp: u8,
    pub def_value: u32,
    pub def_terrain: u32,
    pub def_hp: u8,
    pub luck_max: u32,
}

pub fn damage_range(p: &AttackParams) -> (u32, u32) {
    let f = |luck: u32| -> f64 {
        let a = p.base as f64 * p.att_value as f64 / 100.0 + luck as f64;
        let d = (200.0 - (p.def_value as f64 + p.def_terrain as f64 * p.def_hp as f64)) / 100.0;
        (a * p.att_hp as f64 / 10.0 * d).max(0.0)
    };
    // Floor with a small epsilon, the way AWBW avoids float artefacts like 54.9999.
    let fl = |v: f64| (v + 1e-6).floor() as u32;
    (fl(f(0)), fl(f(p.luck_max)))
}

/// Terrain defence stars that apply to a unit (air units get none).
pub fn terrain_stars(terrain: u16, kind: &str) -> u32 {
    if tables().unit(kind).move_type == "A" {
        0
    } else {
        tables().tile(terrain).defense as u32
    }
}

/// Uses the primary weapon? Primary needs ammo; units with a second weapon use it against
/// footsoldiers and copters (AW2/AWBW convention, to be confirmed against replays).
pub fn uses_primary(att: &Unit, def_kind: &str) -> bool {
    let info = tables().unit(&att.kind);
    if info.ammo == 0 {
        return false;
    }
    if !info.second_weapon {
        return att.ammo > 0;
    }
    let secondary_target = matches!(def_kind, "Infantry" | "Mech" | "B-Copter" | "T-Copter");
    let mech_like = att.kind == "Mech";
    let primary_ok = if mech_like {
        !matches!(def_kind, "Infantry" | "Mech" | "B-Copter" | "T-Copter")
    } else {
        !secondary_target
    };
    primary_ok && att.ammo > 0
}

pub fn dist(a: (u16, u16), b: (u16, u16)) -> u32 {
    ((a.0 as i32 - b.0 as i32).abs() + (a.1 as i32 - b.1 as i32).abs()) as u32
}

pub fn display(hp: u8) -> u8 {
    hp.div_ceil(10)
}
