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
