//! Rules: movement, capture, combat, start of turn.
//!
//! Sources are AWBW's own pages and recorded games (see docs/sources.md). Anything still
//! unmodelled is listed in docs/awbw-rule-findings.md (Open).

#[allow(unused_imports)]
use crate::prelude::*;
use crate::data::{tables, BuildingKind as B, MoveType as M, TerrainKind, UnitType as U};
use crate::state::*;

pub const CAPTURE_POINTS: i32 = 20;

pub fn move_cost(terrain: u16, move_type: M) -> Option<u8> {
    tables().tile(terrain).move_cost(move_type)
}

/// The weather a CO's units feel: Olaf ignores snow and treats rain as snow; Drake ignores rain.
pub fn felt_weather(weather: char, co: impl crate::data::AsCo) -> char {
    use crate::data::Co;
    match (co.as_co(), weather) {
        (Some(Co::Olaf), 'S') => 'C',
        (Some(Co::Olaf), 'R') => 'S',
        (Some(Co::Drake), 'R') => 'C',
        _ => weather,
    }
}

/// Movement cost in weather, from AWBW's terrain chart (terrain.php). Impassable stays
/// impassable; only listed cells change.
pub fn move_cost_in(terrain: u16, move_type: M, weather: char) -> Option<u8> {
    let clear = move_cost(terrain, move_type)?;
    let t = tables().tile(terrain);
    // An emptied missile silo is listed as plain but moves like a building in weather.
    let class = if t.name == "Reef" { "Reef" } else if t.name.starts_with("Missile Silo") { "Building" } else if t.building == Some(B::Port) || t.name.ends_with("Port") { "Port" } else { t.kind.name() };
    let c = match (weather, move_type, class) {
        ('S', M::F, "Plain" | "Forest" | "River") => 2,
        ('S', M::F, "Mountain") => 4,
        ('S', M::B, "Mountain") => 2,
        ('S', M::T, "Plain" | "Forest") => 2,
        ('S', M::W, "Plain" | "Forest") => 3,
        ('S', M::A, _) => 2,
        ('S', M::S | M::L, "Sea" | "Reef" | "Port") => 2,
        ('R', M::T, "Plain") => 2,
        ('R', M::T, "Forest") => 3,
        ('R', M::W, "Plain") => 3,
        ('R', M::W, "Forest") => 4,
        _ => clear,
    };
    Some(c)
}

/// Cost of walking `path` (which starts at the unit's tile). Checks adjacency, bounds,
/// passability, movement range and enemy blocking. Returns the movement cost spent.
/// Cost for `u` to enter a tile of terrain `t` now (weather, CO), or None if impassable.
pub fn step_cost(s: &State, u: &Unit, t: u16) -> Option<u32> {
    let info = tables().unit(u.kind);
    let co = s.player(u.owner).map(|p| p.co).unwrap_or(crate::data::Co::NoCO);
    let w = felt_weather(s.weather, co);
    let pw = s.player(u.owner).map(|p| p.power).unwrap_or_default();
    let c = move_cost_in(t, info.move_type, w)?;
    // Lash's powers, and Sturm always: every passable tile costs 1, except in snow.
    if ((co == "Lash" && pw != "N") || co == "Sturm") && w != 'S' {
        return Some(1);
    }
    Some(c as u32)
}

pub fn path_cost(s: &State, u: &Unit, path: &[(u16, u16)]) -> Result<u32, String> {
    let _ = tables();
    let mut cost = 0u32;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = (a.0 as i32 - b.0 as i32).abs() + (a.1 as i32 - b.1 as i32).abs();
        if d != 1 {
            return Err(format!("path step {a:?}->{b:?} not adjacent"));
        }
        let t = s.terrain_at(b.0, b.1);
        let c = step_cost(s, u, t)
            .ok_or_else(|| format!("{} can't enter {} at {b:?}", u.kind, tables().tile(t).name))?;
        cost += c;
        if let Some(o) = s.unit_at(b.0, b.1) {
            if !s.allied(o.owner, u.owner) {
                return Err(format!("path through enemy {} at {b:?}", o.kind));
            }
        }
    }
    Ok(cost)
}

pub fn movement_range(s: &State, u: &Unit) -> u32 {
    if let Some(&m) = s.move_override.get(&u.id) {
        return m;
    }
    let mut m = tables().unit(u.kind).r#move as u32;
    let co = s.player(u.owner).map(|p| p.co).unwrap_or(crate::data::Co::NoCO);
    // Day-to-day movement changes. Sami's and Sensei's transports move one further, and no
    // more under Sami's powers: the +2 a Power record shows for a loaded transport is the
    // bonus of the foot soldier it carries (all 41 recorded transport moves under Victory
    // March stay within +1).
    if matches!(co, crate::data::Co::Sami | crate::data::Co::Sensei) && matches!(u.kind, U::APC | U::TCopter | U::Lander | U::BlackBoat) {
        m += 1;
    }
    // Drake's naval units move one further (Landers and Black Boats in 3 league games).
    if co == "Drake" && matches!(u.kind.info().move_type, M::S | M::L) {
        m += 1;
    }
    m
}

/// How many capture points a unit takes off a building per capture action.
pub fn capture_amount(s: &State, u: &Unit, display_hp: u8) -> i32 {
    let co = s.player(u.owner).map(|p| p.co).unwrap_or(crate::data::Co::NoCO);
    let hp = display_hp as i32;
    let pw = s.player(u.owner).map(|p| p.power).unwrap_or_default();
    if co == "Sami" && pw == "S" {
        20 // Victory March: captures complete in one action.
    } else if co == "Sami" {
        hp * 3 / 2
    } else {
        hp
    }
}

pub fn is_income_property(terrain: u16) -> bool {
    matches!(tables().tile(terrain).building, Some(B::City | B::Base | B::Airport | B::Port | B::HQ))
}

/// Properties Kindle's High Society counts: every kind a player can own, com towers and
/// labs included (not only the ones that pay income).
pub fn is_urban_property(terrain: u16) -> bool {
    matches!(tables().tile(terrain).building, Some(B::City | B::Base | B::Airport | B::Port | B::HQ | B::ComTower | B::Lab))
}

/// A league game's capture limit: 70% of the map's income properties, rounded down (what
/// the public pages of seven recorded league games show).
/// AWBW's league games (and its default for new games): 50 units a player.
pub const LEAGUE_UNIT_LIMIT: u32 = 50;

pub fn league_capture_limit(s: &State) -> u32 {
    (s.buildings.values().filter(|b| is_income_property(b.terrain)).count() * 7 / 10) as u32
}

/// Has this player reached the game's unit limit? Carried units count.
pub fn at_unit_limit(s: &State, pid: PlayerId) -> bool {
    s.unit_limit.is_some_and(|n| s.units.values().filter(|u| u.owner == pid).count() as u32 >= n)
}

/// Which unit classes a building repairs and resupplies.
pub fn services(terrain: u16, kind: U) -> bool {
    let mt = kind.info().move_type;
    match tables().tile(terrain).building {
        Some(B::City | B::HQ) => matches!(mt, M::F | M::B | M::T | M::W),
        Some(B::Base) => matches!(mt, M::F | M::B | M::T | M::W | M::P),
        Some(B::Airport) => mt == M::A,
        Some(B::Port) => matches!(mt, M::S | M::L),
        _ => false,
    }
}

/// Combat: the range of damage (internal HP points) an attack can do. AWBW's formula:
///   (AHP/10) * (B * ACO/100 + L) * (200 - (DCO + DTR*DHP)) / 100, L = luck,
/// worked out in double-precision floating point, rounded to one decimal and then cut to a
/// whole point. So a raw x.95 or more counts as x+1, but only when the floating-point
/// product itself reaches x.95: 97.5 * 0.82 comes out a hair under 79.95 and stays 79.
pub struct AttackParams {
    pub base: u32,
    /// Attack value in per cent, counterattack bonus included (see `combat::att_value`).
    pub att_value: u32,
    /// Extra attack in thousandths of a per cent point (Colin's Power of Money: 3 per fund
    /// held, i.e. +3% per 1000 funds, not rounded).
    pub att_milli: u32,
    pub att_hp: u8,
    pub def_value: u32,
    pub def_terrain: u32,
    pub def_hp: u8,
    pub luck_min: i32,
    pub luck_max: i32,
}

/// AWBW computes damage in double precision (PHP floats), and its result depends on how the
/// doubles round: game-1749614 turn 26 is 79.95 on paper and AWBW dealt 79. `damage_f64` is
/// that computation, in AWBW's order of operations. `damage_exact` gives the same answer for
/// every input with integer arithmetic (for a zkVM, where floats are emulated and slow): the
/// exact value is a multiple of 1e-8, and double rounding can only matter when it sits exactly
/// on a "x.95" boundary, so only those rare inputs fall back to `damage_f64`.
pub fn damage_range(p: &AttackParams) -> (u32, u32) {
    (damage_exact(p, p.luck_min), damage_exact(p, p.luck_max))
}

/// Damage at one luck roll, as AWBW's doubles give it.
pub fn damage_f64(p: &AttackParams, luck: i32) -> u32 {
    let attack = p.att_value as f64 + p.att_milli as f64 / 1000.0;
    let raw = (p.att_hp as f64 / 10.0) * (p.base as f64 * (attack / 100.0) + luck as f64);
    let defence = 200 - (p.def_value as i64 + p.def_terrain as i64 * p.def_hp as i64);
    let raw = raw * (defence as f64 / 100.0);
    if raw.is_nan() || raw <= 0.0 {
        return 0;
    }
    // One decimal the way AWBW rounds it: the whole tenths below the
    // value, one more when the value is at or above the double nearest to "those tenths
    // and a half". Then drop the fraction.
    let mut tenths = crate::num::floor(raw * 10.0);
    if (tenths + 1.0) / 10.0 == raw {
        tenths += 1.0; // the product is exactly the next tenth
    }
    if raw >= (tenths + 0.5) / 10.0 {
        tenths += 1.0;
    }
    (tenths as u64 / 10) as u32
}

/// The same result as `damage_f64`, in integers. raw = N / 1e8 exactly, with
/// N = hp * (base * (1000 * attack + milli) + 100000 * luck) * defence. Rounding to one decimal
/// and then dropping the fraction is floor(raw + 0.05), except when raw is exactly k - 0.05:
/// there the doubles decide (79.95 gave 79), so those inputs take the f64 path.
pub fn damage_exact(p: &AttackParams, luck: i32) -> u32 {
    let defence = 200 - (p.def_value as i128 + p.def_terrain as i128 * p.def_hp as i128);
    let n = p.att_hp as i128
        * (p.base as i128 * (1000 * p.att_value as i128 + p.att_milli as i128) + 100_000 * luck as i128)
        * defence;
    if n <= 0 {
        return 0;
    }
    let q = n + 5_000_000;
    if q % 100_000_000 == 0 {
        return damage_f64(p, luck);
    }
    (q / 100_000_000) as u32
}

/// Terrain defence stars that apply to a unit (air units get none).
pub fn terrain_stars(terrain: u16, kind: U) -> u32 {
    if kind.info().move_type == M::A {
        0
    } else {
        tables().tile(terrain).defense as u32
    }
}

/// Uses the primary weapon? Primary needs ammo; units with a second weapon use it against
/// footsoldiers and copters (AW2/AWBW convention, to be confirmed against replays).
pub fn uses_primary(att: &Unit, def_kind: U) -> bool {
    let info = att.kind.info();
    if info.ammo == 0 {
        return false;
    }
    if att.kind == U::Cruiser && def_kind.info().move_type == M::A {
        return false; // anti-air gun, no ammo
    }
    if !info.second_weapon {
        return att.ammo > 0;
    }
    !secondary_target(att.kind, def_kind) && att.ammo > 0
}

/// Targets a unit hits with its second weapon (machine gun / anti-air gun), which costs no
/// ammo. False for units without one.
/// Machine-gun base damage of a second-weapon unit with no ammo against a target it would
/// normally hit with its main weapon (the damage chart only holds each pair's main weapon).
/// From the published AW2 damage chart; checked against replays only for B-Copter vs
/// Neotank and Mech vs Md.Tank (the only out-of-ammo attacks in our records). None: the
/// machine gun can't hit it.
pub fn mg_damage(att_kind: U, def_kind: U) -> Option<u8> {
    let row: &[(U, u8)] = match att_kind {
        U::Mech => &[(U::Recon, 18), (U::APC, 20), (U::Artillery, 32), (U::Rocket, 35), (U::Missile, 35), (U::AntiAir, 6), (U::Tank, 6), (U::MdTank, 1), (U::Neotank, 1), (U::MegaTank, 1), (U::Piperunner, 6)],
        U::BCopter => &[(U::Recon, 30), (U::APC, 20), (U::Artillery, 25), (U::Rocket, 35), (U::Missile, 35), (U::AntiAir, 6), (U::Tank, 6), (U::MdTank, 1), (U::Neotank, 1), (U::MegaTank, 1), (U::Piperunner, 6)],
        U::Tank => &[(U::Recon, 40), (U::APC, 45), (U::Artillery, 45), (U::Rocket, 55), (U::Missile, 30), (U::AntiAir, 5), (U::Tank, 6), (U::MdTank, 1), (U::Neotank, 1), (U::MegaTank, 1), (U::Piperunner, 6)],
        // No Md.Tank entry: AWBW's second-weapon table (ATTACK2) has none, so its game page
        // offers no Fire on a Md.Tank once a Md.Tank's ammo is gone.
        U::MdTank => &[(U::Recon, 45), (U::APC, 45), (U::Artillery, 45), (U::Rocket, 55), (U::Missile, 35), (U::AntiAir, 7), (U::Tank, 8), (U::Neotank, 1), (U::MegaTank, 1), (U::Piperunner, 8)],
        U::Neotank | U::MegaTank => &[(U::Recon, 65), (U::APC, 65), (U::Artillery, 65), (U::Rocket, 75), (U::Missile, 55), (U::AntiAir, 17), (U::Tank, 10), (U::MdTank, 1), (U::Neotank, 1), (U::MegaTank, 1), (U::Piperunner, 10)],
        _ => &[],
    };
    row.iter().find(|(k, _)| *k == def_kind).map(|&(_, v)| v)
}

/// Base damage `att` deals `def` with the weapon it would use now, or None if it can't
/// fire at it: the chart's value; out of ammo, its second weapon's (the chart's value for
/// second-weapon targets, the machine-gun table otherwise) or nothing.
pub fn base_damage_now(att: &Unit, def_kind: U) -> Option<u8> {
    let info = att.kind.info();
    if info.ammo > 0 && att.ammo == 0 {
        // A Cruiser has no second weapon as AWBW counts them: out of ammo it can't fire at
        // all, not even its guns at aircraft. (AWBW's game page offers Fire, but the server
        // answers "Unit is out of ammos!": checked live in game 1769988. Its shots at
        // aircraft, a Stealth included, use no ammo while it has some.)
        if att.kind == U::Cruiser {
            return None;
        }
        if secondary_target(att.kind, def_kind) {
            return tables().base_damage(att.kind, def_kind);
        }
        if info.second_weapon && att.kind != U::Cruiser {
            return mg_damage(att.kind, def_kind);
        }
        return None;
    }
    tables().base_damage(att.kind, def_kind)
}

pub fn secondary_target(att_kind: U, def_kind: U) -> bool {
    if !att_kind.info().second_weapon && att_kind != U::Cruiser {
        return false;
    }
    match att_kind {
        U::Cruiser => def_kind.info().move_type == M::A,
        // The data says it has one, but it fires its cannon (and spends ammo) on Infantry
        // too (game-1750944): an indirect has no machine gun.
        U::Piperunner => false,
        _ => matches!(def_kind, U::Infantry | U::Mech | U::BCopter | U::TCopter),
    }
}

pub fn dist(a: (u16, u16), b: (u16, u16)) -> u32 {
    ((a.0 as i32 - b.0 as i32).abs() + (a.1 as i32 - b.1 as i32).abs()) as u32
}

pub fn display(hp: u8) -> u8 {
    hp.div_ceil(10)
}

pub fn is_foot(kind: U) -> bool {
    matches!(kind, U::Infantry | U::Mech)
}

pub fn is_indirect(kind: U) -> bool {
    kind.info().range[1] > 1
}

/// Transport capacity.
pub fn capacity(kind: U) -> usize {
    match kind {
        U::APC | U::TCopter => 1,
        U::Lander | U::Cruiser | U::Carrier | U::BlackBoat => 2,
        _ => 0,
    }
}

/// Can `attacker` damage a dived Sub or hidden Stealth of kind `hidden`? Only Cruisers and
/// Subs hit a dived Sub, only Fighters and Stealths a hidden Stealth. Counterattacks too.
pub fn can_hit_hidden(attacker: U, hidden: U) -> bool {
    match hidden {
        U::Stealth => matches!(attacker, U::Fighter | U::Stealth),
        U::Sub => matches!(attacker, U::Cruiser | U::Sub),
        _ => false,
    }
}

/// Can this transport unload while standing on `terrain`? APCs, T-Copters, Landers and
/// Black Boats can't from sea or reef (none of about 3,900 recorded unloads is); Cruisers
/// and Carriers unload anywhere.
pub fn can_unload_from(transport: U, terrain: u16) -> bool {
    matches!(transport, U::Cruiser | U::Carrier) || tables().tile(terrain).kind != TerrainKind::Sea
}

/// Can transport `t` carry unit kind `l`?
pub fn can_carry(t: U, l: U) -> bool {
    let mt = l.info().move_type;
    match t {
        U::APC | U::TCopter | U::BlackBoat => is_foot(l),
        U::Lander => matches!(mt, M::F | M::B | M::T | M::W),
        U::Cruiser => matches!(l, U::BCopter | U::TCopter),
        U::Carrier => mt == M::A,
        _ => false,
    }
}
