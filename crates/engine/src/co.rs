//! CO day-to-day abilities, read from data/cos.json (the replay player's CO table), plus the
//! few the table doesn't spell out (Javier, Lash, Rachel, Sami's capture and transports).

use crate::data::tables;
use serde_json::Value;

fn d2d(co: &str) -> Option<&'static Value> {
    tables().cos.get(co).map(|c| &c["d2d"])
}

fn affects(entry: &Value, kind: &str) -> bool {
    entry["AffectedUnits"].as_array().is_some_and(|a| a.iter().any(|u| u == "all" || u == kind))
}

/// The CO's active tables: day-to-day, plus the CO power ("Y") or super power ("S") on top.
fn active(co: &str, power: &str) -> Vec<&'static Value> {
    let Some(c) = tables().cos.get(co) else { return vec![] };
    let mut v = vec![&c["d2d"]];
    match power {
        "Y" => v.push(&c["cop"]),
        "S" => v.push(&c["scop"]),
        _ => {}
    }
    v
}

fn sum_field(co: &str, power: &str, kind: &str, terrain_kind: Option<&str>, field: &str) -> f64 {
    active(co, power)
        .into_iter()
        .filter_map(|d| d["PowerIncreases"].as_array())
        .flatten()
        .filter(|e| affects(e, kind))
        .filter(|e| match e["ConditionalTerrain"].as_str() {
            None => true,
            Some(t) => terrain_kind == Some(t),
        })
        .filter_map(|e| e[field].as_f64())
        .sum()
}

/// Attack bonus in percent points (e.g. +20 for Max's tanks). `terrain_kind` is the
/// attacker's tile kind ("Plain", "Road", "Building", ...).
/// Any active power also gives every unit +10% attack and +10% defence.
pub fn attack_bonus(co: &str, power: &str, kind: &str, terrain_kind: &str, terrain_stars: u32) -> i32 {
    let mut v = (sum_field(co, power, kind, Some(terrain_kind), "PowerIncrease") * 100.0).round() as i32;
    if power != "N" {
        v += 10;
    }
    if co == "Lash" {
        v += 10 * terrain_stars as i32;
    }
    v
}

pub fn defense_bonus(co: &str, power: &str, kind: &str) -> i32 {
    let v = (sum_field(co, power, kind, None, "DefenseIncrease") * 100.0).round() as i32;
    v + if power != "N" { 10 } else { 0 }
}

pub fn range_bonus(co: &str, power: &str, kind: &str) -> i32 {
    sum_field(co, power, kind, None, "RangeIncrease").round() as i32
}

/// Luck roll range in percent points, inclusive. AWBW default 0..=9.
/// The power's LuckRange, if it has one, replaces the day-to-day one.
pub fn luck(co: &str, power: &str) -> (i32, i32) {
    let l = active(co, power).into_iter().rev().map(|d| &d["LuckRange"]).find(|l| l.is_object());
    match l {
        Some(l) => (l["x"].as_i64().unwrap_or(0) as i32, l["y"].as_i64().unwrap_or(9) as i32),
        None => (0, 9),
    }
}

pub fn price_multiplier(co: &str) -> f64 {
    d2d(co).and_then(|d| d["UnitPriceMultiplier"].as_f64()).unwrap_or(1.0)
}

pub fn property_fund_increase(co: &str) -> i64 {
    d2d(co).and_then(|d| d["PropertyFundIncrease"].as_i64()).unwrap_or(0)
}

pub fn air_fuel_decrease(co: &str) -> i32 {
    d2d(co).and_then(|d| d["AirFuelUsageDecrease"].as_i64()).unwrap_or(0) as i32
}

/// Counterattack damage multiplier (Sonja 1.5).
pub fn counter_multiplier(co: &str) -> f64 {
    d2d(co).and_then(|d| d["CounterAttackPower"].as_f64()).unwrap_or(1.0)
}

/// Base power costs (CO power, super power) in meter units (funds of damage x 10), before
/// growth from use. Read from day-1 states of 165 recorded games; the last six are from
/// AWBW's published star counts (9000 funds a star) and not yet seen in a replay.
/// A CO with no regular power (Von Bolt) has both equal to the super's cost.
pub fn power_base(co: &str) -> Option<(i64, i64)> {
    const STAR: i64 = 90_000;
    Some(match co {
        "Adder" => (2, 5), "Andy" | "Grimm" | "Grit" | "Jake" | "Javier" | "Jess" | "Kindle"
        | "Max" | "Nell" | "Rachel" => (3, 6), "Drake" | "Lash" => (4, 7), "Eagle" => (3, 9),
        "Hawke" => (5, 9), "Koal" | "Sonja" => (3, 5), "Olaf" => (3, 7), "Sami" => (3, 8),
        "Sasha" => (2, 6), "Von Bolt" => (10, 10),
        "Colin" | "Sensei" => (2, 6), "Flak" => (3, 6), "Hachi" => (3, 5), "Kanbei" => (4, 7),
        "Sturm" => (6, 10),
        _ => return None,
    }).map(|(a, b)| (a * STAR, b * STAR))
}

/// Powers the player could activate now: none while one is on or when powers are off.
pub fn powers_ready(s: &crate::state::State, pid: u32) -> Vec<&'static str> {
    let Some(p) = s.player(pid) else { return vec![] };
    if !s.powers || p.power != "N" || p.scop_cost == 0 {
        return vec![];
    }
    let mut out = vec![];
    if p.cop_cost < p.scop_cost && p.meter >= p.cop_cost {
        out.push("Y");
    }
    if p.meter >= p.scop_cost {
        out.push("S");
    }
    out
}

/// A power's effects at the moment it's activated (HP, fuel, movement, weather). Values read
/// from the recorded Power actions (`hpChange`, `global`, `weatherReplace`); every recorded
/// use of a CO and power agrees. Ongoing effects (attack, defense, luck, range) live in the
/// CO data. Powers that change chosen units (missiles, Kindle, Jake, Sami...) aren't here yet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Instant {
    /// Displayed HP added to the user's side's units (heals reach carried units).
    pub gain: i32,
    /// Displayed HP removed from the other side's units (not carried ones); never kills.
    pub loss: i32,
    /// Fuel multiplier for the units that lose HP (Drake halves it, rounding up).
    pub loss_fuel: f64,
    /// Movement points added to the user's units for the rest of the turn.
    pub moves: i32,
    pub weather: Option<char>,
    /// Which of the user's units get `moves`: "all", "foot", "vehicle" (ground, not foot)
    /// or "direct" (non-foot units that fire at range 1).
    pub moves_for: &'static str,
    /// Refill fuel and ammo of the user's units that aren't being carried (Jess).
    pub resupply: bool,
    /// The user's non-foot units that already acted can act again (Eagle's super).
    pub act_again: bool,
    /// Displayed HP removed from enemy units standing on properties (Kindle's COP).
    pub urban_loss: i32,
    /// Drain other players' meters by their super cost x the user's funds / 50000 (Sasha's COP).
    pub drain: bool,
    /// Missiles: each takes 3 displayed HP (never kills) from enemy units within 2 tiles of
    /// its target (Rachel's super: 3, Von Bolt's: 1).
    pub missiles: u8,
    /// Units the missiles hit can't act in their next turn (Von Bolt).
    pub stun: bool,
}

/// Does a unit of this kind get a power's movement bonus?
pub fn moves_apply(kind: &str, moves_for: &str) -> bool {
    let u = tables().unit(kind);
    let foot = crate::rules::is_foot(kind);
    let ground = !matches!(u.move_type.as_str(), "A" | "S" | "L");
    match moves_for {
        "foot" => foot,
        "vehicle" => ground && !foot,
        "direct" => !foot && u.range[0] <= 1 && u.range[1] == 1,
        _ => true,
    }
}

pub fn instant(co: &str, power: &str) -> Option<Instant> {
    let i = |gain, loss, loss_fuel, moves, weather| Some(Instant { gain, loss, loss_fuel, moves, weather, moves_for: "all", resupply: false, act_again: false, urban_loss: 0, drain: false, missiles: 0, stun: false });
    let only = |moves, moves_for, resupply| Some(Instant { gain: 0, loss: 0, loss_fuel: 1.0, moves, weather: None, moves_for, resupply, act_again: false, urban_loss: 0, drain: false, missiles: 0, stun: false });
    match (co, power) {
        ("Sami", "Y") => only(1, "foot", false),
        ("Sami", "S") => only(2, "foot", false),
        ("Jess", "Y") => only(1, "vehicle", true),
        ("Jess", "S") => only(2, "vehicle", true),
        ("Max", "Y") => only(1, "direct", false),
        ("Max", "S") => only(2, "direct", false),
        ("Jake", "Y") => only(0, "vehicle", false),
        ("Jake", "S") => only(2, "vehicle", false),
        ("Eagle", "S") => Some(Instant { act_again: true, ..only(0, "all", false).unwrap() }),
        ("Nell", "S") => i(0, 0, 1.0, 0, None),
        ("Kindle", "Y") => Some(Instant { urban_loss: 3, ..i(0, 0, 1.0, 0, None).unwrap() }),
        ("Sasha", "Y") => Some(Instant { drain: true, ..i(0, 0, 1.0, 0, None).unwrap() }),
        ("Rachel", "S") => Some(Instant { missiles: 3, ..i(0, 0, 1.0, 0, None).unwrap() }),
        ("Von Bolt", "S") => Some(Instant { missiles: 1, stun: true, ..i(0, 0, 1.0, 0, None).unwrap() }),
        ("Andy", "Y") => i(2, 0, 1.0, 0, None),
        ("Andy", "S") => i(5, 0, 1.0, 1, None),
        ("Hawke", "Y") => i(1, 1, 1.0, 0, None),
        ("Hawke", "S") => i(2, 2, 1.0, 0, None),
        ("Drake", "Y") => i(0, 1, 0.5, 0, None),
        ("Drake", "S") => i(0, 2, 0.5, 0, Some('R')),
        ("Olaf", "S") => i(0, 2, 1.0, 0, Some('S')),
        ("Adder", "Y") | ("Koal", "Y") => i(0, 0, 1.0, 1, None),
        ("Adder", "S") | ("Koal", "S") => i(0, 0, 1.0, 2, None),
        // Powers with no instant effect beyond their ongoing ones.
        ("Grimm", _) | ("Javier", _) | ("Lash", _) | ("Sonja", _) | ("Rachel", "Y") | ("Kindle", "S") => i(0, 0, 1.0, 0, None),
        _ => None,
    }
}

/// Every CO in the data, "No CO" first, with how much of their powers the engine models:
/// "full" (ongoing and instant effects), "partial" (ongoing only; unit-specific or missile
/// effects not yet), or "none" (No CO).
pub fn roster() -> Vec<(String, &'static str)> {
    let mut v: Vec<(String, &'static str)> = tables().cos.keys().cloned().collect::<Vec<_>>()
        .into_iter().filter(|c: &String| c != "No CO" && power_base(c).is_some())
        .map(|c| { let st = if instant(&c, "Y").is_some() && instant(&c, "S").is_some() { "full" } else { "partial" }; (c, st) })
        .collect();
    v.sort();
    v.insert(0, ("No CO".to_string(), "none"));
    v
}
