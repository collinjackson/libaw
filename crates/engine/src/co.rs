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

fn sum_field(co: &str, kind: &str, terrain_kind: Option<&str>, field: &str) -> f64 {
    let Some(d) = d2d(co) else { return 0.0 };
    let Some(list) = d["PowerIncreases"].as_array() else { return 0.0 };
    list.iter()
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
pub fn attack_bonus(co: &str, kind: &str, terrain_kind: &str, terrain_stars: u32) -> i32 {
    let mut v = (sum_field(co, kind, Some(terrain_kind), "PowerIncrease") * 100.0).round() as i32;
    if co == "Lash" {
        v += 10 * terrain_stars as i32;
    }
    v
}

pub fn defense_bonus(co: &str, kind: &str) -> i32 {
    (sum_field(co, kind, None, "DefenseIncrease") * 100.0).round() as i32
}

pub fn range_bonus(co: &str, kind: &str) -> i32 {
    sum_field(co, kind, None, "RangeIncrease").round() as i32
}

/// Luck roll range in percent points, inclusive. AWBW default 0..=9.
pub fn luck(co: &str) -> (i32, i32) {
    match d2d(co).map(|d| &d["LuckRange"]) {
        Some(l) if l.is_object() => (l["x"].as_i64().unwrap_or(0) as i32, l["y"].as_i64().unwrap_or(9) as i32),
        _ => (0, 9),
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
