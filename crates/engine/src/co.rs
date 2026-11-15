//! CO day-to-day abilities, read from data/cos.json (the replay player's CO table), plus the
//! few the table doesn't spell out (Javier, Lash, Rachel, Sami's capture and transports).

#[allow(unused_imports)]
use crate::prelude::*;
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

/// The sum of a power field over the active entries, in hundredths (the table holds
/// decimals such as 0.15; each converts exactly to whole hundredths).
fn sum_field(co: &str, power: &str, kind: &str, terrain_kind: Option<&str>, field: &str) -> i32 {
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
        .map(|v| crate::num::round(v * 100.0) as i32)
        .sum()
}

/// Attack bonus in percent points (e.g. +20 for Max's tanks). `terrain_kind` is the
/// attacker's tile kind ("Plain", "Road", "Building", ...).
/// Any active power also gives every unit +10% attack and +10% defence.
pub fn attack_bonus(co: &str, power: &str, kind: &str, terrain_kind: &str, terrain_stars: u32) -> i32 {
    let mut v = sum_field(co, power, kind, Some(terrain_kind), "PowerIncrease");
    if power != "N" {
        v += 10;
    }
    if co == "Lash" {
        v += 10 * terrain_stars as i32;
    }
    v
}

pub fn defense_bonus(co: &str, power: &str, kind: &str) -> i32 {
    let v = sum_field(co, power, kind, None, "DefenseIncrease");
    v + if power != "N" { 10 } else { 0 }
}

pub fn range_bonus(co: &str, power: &str, kind: &str) -> i32 {
    sum_field(co, power, kind, None, "RangeIncrease") / 100
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

/// One luck roll for a range from `luck`, the way AWBW rolls it: good luck uniform in
/// 0..=hi and, for COs with bad luck (lo < 0: Sonja, Flak, Jugger), bad luck uniform in
/// 0..=-lo, rolled separately and subtracted. `rng` gives uniform u32s.
pub fn roll_luck(lo: i32, hi: i32, rng: &mut dyn FnMut() -> u32) -> i32 {
    let mut uniform = |max: i32| if max <= 0 { 0 } else { (rng() % (max as u32 + 1)) as i32 };
    if lo < 0 {
        let good = uniform(hi);
        good - uniform(-lo)
    } else {
        lo + uniform(hi - lo)
    }
}

/// Build price multiplier, with an active power's own value taking over (Hachi: 0.5).
/// Unit price multiplier in per cent (Kanbei 120, Colin 80, Hachi 90, Hachi's super 50).
pub fn price_pct(co: &str, power: &str) -> i64 {
    active(co, power).into_iter().rev().find_map(|d| d["UnitPriceMultiplier"].as_f64()).map(|v| crate::num::round(v * 100.0) as i64).unwrap_or(100)
}

/// What `cost` costs this CO now, rounded half up (integer arithmetic).
pub fn price(cost: i64, co: &str, power: &str) -> i64 {
    (cost * price_pct(co, power) + 50) / 100
}

pub fn property_fund_increase(co: &str) -> i64 {
    d2d(co).and_then(|d| d["PropertyFundIncrease"].as_i64()).unwrap_or(0)
}

pub fn air_fuel_decrease(co: &str) -> i32 {
    d2d(co).and_then(|d| d["AirFuelUsageDecrease"].as_i64()).unwrap_or(0) as i32
}

/// Counterattack multiplier in per cent, applied to the CO's own attack value: Sonja 150
/// always, Kanbei 150 under Samurai Spirit. An active power's value takes over.
pub fn counter_pct(co: &str, power: &str) -> u32 {
    active(co, power).into_iter().rev().find_map(|d| d["CounterAttackPower"].as_f64())
        .map(|v| crate::num::round(v * 100.0) as u32).unwrap_or(100)
}

/// The CO's flat day-to-day attack bonus for a unit kind (no terrain conditions, no power).
pub fn d2d_attack_bonus(co: &str, kind: &str) -> i32 {
    sum_field(co, "N", kind, None, "PowerIncrease")
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
        "Hawke" => (5, 9), "Koal" | "Sonja" => (3, 5), "Olaf" | "Jugger" => (3, 7), "Sami" => (3, 8),
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
    pub loss_fuel_pct: i32,
    /// Movement points added to the user's units for the rest of the turn.
    pub moves: i32,
    pub weather: Option<char>,
    /// Which of the user's units get `moves`: "all", "foot", "vehicle" (ground, not foot)
    /// or "direct" (non-foot units that fire at range 1).
    pub moves_for: &'static str,
    /// Refill fuel and ammo of all the user's units, carried ones included (Jess; the next
    /// turn's record shows carried units full).
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
    /// Displayed HP each missile takes (3; Sturm's meteors 4 and 8).
    pub missile_hp: i32,
    /// Multiply the user's funds (Colin's Gold Rush: 1.5).
    pub funds_pct: i64,
    /// Attack +3% per 1000 funds the user holds, for the power's duration (Colin's super).
    pub funds_attack: bool,
    /// A 9 HP unit of this kind on every city the user owns with no unit on it, ready to
    /// act (Sensei: Infantry, Mech).
    pub spawn: Option<&'static str>,
}

/// Does a unit of this kind get a power's movement bonus?
pub fn moves_apply(kind: &str, moves_for: &str) -> bool {
    let u = tables().unit(kind);
    let foot = crate::rules::is_foot(kind);
    let ground = !matches!(u.move_type.as_str(), "A" | "S" | "L");
    match moves_for {
        "foot" => foot,
        "vehicle" => ground && !foot,
        // Max's powers: every unit that isn't an indirect, a foot soldier or a transport;
        // the unarmed Black Bomb counts.
        "direct" => (!foot && u.range[0] <= 1 && u.range[1] == 1) || kind == "Black Bomb",
        _ => true,
    }
}

pub fn instant(co: &str, power: &str) -> Option<Instant> {
    let i = |gain, loss, loss_fuel_pct, moves, weather| Some(Instant { gain, loss, loss_fuel_pct, moves, weather, moves_for: "all", resupply: false, act_again: false, urban_loss: 0, drain: false, missiles: 0, stun: false, missile_hp: 3, funds_pct: 100, funds_attack: false, spawn: None });
    let only = |moves, moves_for, resupply| Some(Instant { gain: 0, loss: 0, loss_fuel_pct: 100, moves, weather: None, moves_for, resupply, act_again: false, urban_loss: 0, drain: false, missiles: 0, stun: false, missile_hp: 3, funds_pct: 100, funds_attack: false, spawn: None });
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
        ("Nell", "S") => i(0, 0, 100, 0, None),
        ("Kindle", "Y") => Some(Instant { urban_loss: 3, ..i(0, 0, 100, 0, None).unwrap() }),
        ("Sasha", "Y") => Some(Instant { drain: true, ..i(0, 0, 100, 0, None).unwrap() }),
        ("Rachel", "S") => Some(Instant { missiles: 3, ..i(0, 0, 100, 0, None).unwrap() }),
        // Not seen in our recorded games (ballpark until checked): Sturm's meteors, Olaf's
        // Blizzard, Colin's Gold Rush.
        ("Sturm", "Y") => Some(Instant { missiles: 1, missile_hp: 4, ..i(0, 0, 100, 0, None).unwrap() }),
        ("Sturm", "S") => Some(Instant { missiles: 1, missile_hp: 8, ..i(0, 0, 100, 0, None).unwrap() }),
        ("Olaf", "Y") => i(0, 0, 100, 0, Some('S')),
        ("Colin", "Y") => Some(Instant { funds_pct: 150, ..i(0, 0, 100, 0, None).unwrap() }),
        // Also unseen in records, from AWBW's published CO rules: Colin's Power of Money,
        // Sensei's spawns, and two ongoing supers handled elsewhere (Sasha's War Bonds pays
        // funds for damage dealt in sim::attack; Hachi's Merchant Union builds ground units
        // on cities in actions::builds).
        ("Colin", "S") => Some(Instant { funds_attack: true, ..i(0, 0, 100, 0, None).unwrap() }),
        ("Sensei", "Y") => Some(Instant { spawn: Some("Infantry"), ..i(0, 0, 100, 0, None).unwrap() }),
        ("Sensei", "S") => Some(Instant { spawn: Some("Mech"), ..i(0, 0, 100, 0, None).unwrap() }),
        ("Sasha", "S") | ("Hachi", "S") => i(0, 0, 100, 0, None),
        // Luck and stat powers: their ongoing effects come from the CO tables; nothing instant.
        ("Nell", "Y") | ("Flak", _) | ("Jugger", _) | ("Eagle", "Y") | ("Kanbei", _) | ("Grit", _) | ("Hachi", "Y") => i(0, 0, 100, 0, None),
        ("Von Bolt", "S") => Some(Instant { missiles: 1, stun: true, ..i(0, 0, 100, 0, None).unwrap() }),
        ("Andy", "Y") => i(2, 0, 100, 0, None),
        ("Andy", "S") => i(5, 0, 100, 1, None),
        ("Hawke", "Y") => i(1, 1, 100, 0, None),
        ("Hawke", "S") => i(2, 2, 100, 0, None),
        ("Drake", "Y") => i(0, 1, 50, 0, None),
        ("Drake", "S") => i(0, 2, 50, 0, Some('R')),
        ("Olaf", "S") => i(0, 2, 100, 0, Some('S')),
        ("Adder", "Y") | ("Koal", "Y") => i(0, 0, 100, 1, None),
        ("Adder", "S") | ("Koal", "S") => i(0, 0, 100, 2, None),
        // Powers with no instant effect beyond their ongoing ones.
        ("Grimm", _) | ("Javier", _) | ("Lash", _) | ("Sonja", _) | ("Rachel", "Y") | ("Kindle", "S") => i(0, 0, 100, 0, None),
        _ => None,
    }
}

/// Every CO in the data, "No CO" first, with how much of their powers the engine models:
/// "full" (ongoing and instant effects), "partial" (ongoing only; unit-specific or missile
/// effects not yet), or "none" (No CO).
pub fn roster() -> Vec<(String, &'static str)> {
    let mut v: Vec<(String, &'static str)> = tables().cos.keys().cloned().collect::<Vec<_>>()
        .into_iter().filter(|c: &String| c != "No CO" && power_base(c).is_some())
        .map(|c| {
            // A CO whose COP costs as much as the super has no COP (Von Bolt).
            let no_cop = power_base(&c).is_some_and(|(y, s)| y == s);
            let st = if (no_cop || instant(&c, "Y").is_some()) && instant(&c, "S").is_some() { "full" } else { "partial" };
            (c, st)
        })
        .collect();
    v.sort();
    v.insert(0, ("No CO".to_string(), "none"));
    v
}
