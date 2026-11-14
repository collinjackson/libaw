//! Combat rules that recorded games can't settle on their own (rounding, counterattack
//! bonuses, rarely used CO powers, out-of-ammo attacks), from AWBW's rules as clarified for this project. See docs/sources.md.

use libaw::combat::{att_milli, att_value, def_value};
use libaw::data::tables;
use libaw::rules::{damage_range, is_urban_property, mg_damage, AttackParams};
use libaw::{co, State, Unit};

/// A neutral attack: 100% attack and defence, the given base damage, HP, terrain and luck.
fn params(base: u32, att_hp: u8, def_terrain: u32, def_hp: u8, luck: i32) -> AttackParams {
    AttackParams { base, att_value: 100, att_milli: 0, att_hp, def_value: 100, def_terrain, def_hp, luck_min: luck, luck_max: luck }
}

fn state() -> State {
    let turn: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/eval-harness/test-states/game-1755654-turn-020-D11-pid4205798.json"
    ))
    .unwrap();
    libaw::awbw::state_from(&turn)
}

/// Give the player to move this CO and power state, and return one of their units.
fn with_co(s: &mut State, co: &str, power: &str) -> Unit {
    let pid = s.current;
    let p = s.player_mut(pid).unwrap();
    p.co = co.into();
    p.power = power.into();
    s.units.values().find(|u| u.owner == pid && u.kind == "Infantry")
        .or_else(|| s.units.values().find(|u| u.owner == pid))
        .cloned()
        .unwrap()
}

#[test]
fn damage_is_rounded_to_one_decimal_before_the_fraction_is_dropped() {
    // 0.5 * (5 + 5 luck) * 0.99 (one star under a 1 HP defender) = 4.95: counts as 5.
    assert_eq!(damage_range(&params(5, 5, 1, 1, 5)), (5, 5));
    // (13 + 9) * 0.95 = 20.90: 20.
    assert_eq!(damage_range(&params(13, 10, 1, 5, 9)), (20, 20));
    // 55 * 0.9 (a 9 HP attacker) = 49.5: 49.
    assert_eq!(damage_range(&params(55, 9, 0, 10, 0)), (49, 49));
    // Bad luck can't push damage below zero.
    assert_eq!(damage_range(&params(5, 10, 0, 10, -9)), (0, 0));
}

#[test]
fn rounding_follows_awbw_floating_point() {
    // game-1749614, turn 26: a 10 HP Tank at 130% hits an 8 HP Infantry at 110% defence on a
    // plain with no luck. On paper 97.5 * 0.82 = 79.95, but in double precision the product
    // falls just short, so AWBW deals 79 and the Infantry lives on 1 HP.
    let p = AttackParams { base: 75, att_value: 130, att_milli: 0, att_hp: 10, def_value: 110, def_terrain: 1, def_hp: 8, luck_min: 0, luck_max: 0 };
    assert_eq!(damage_range(&p), (79, 79));
    // A 5 HP Neotank on a 5 HP Infantry in a city at best luck: 67 * 0.85 = 56.95 on paper,
    // 56.949999999999996 in double precision. AWBW compares that with the double nearest
    // to 56.95 and keeps 56 (its public damage calculator shows 53-56%).
    assert_eq!(damage_range(&params(125, 5, 3, 5, 9)), (56, 56));
}

#[test]
fn sonja_counterattacks_multiply_only_her_own_attack_value() {
    let mut s = state();
    let u = with_co(&mut s, "Sonja", "N");
    let t = s.terrain_at(u.x, u.y);
    // Half of her base 100; whatever her com towers add is not multiplied.
    assert_eq!(att_value(&s, &u, t, true), att_value(&s, &u, t, false) + 50);
}

#[test]
fn kanbei_counterattacks_gain_half_his_attack_only_under_samurai_spirit() {
    let mut s = state();
    let u = with_co(&mut s, "Kanbei", "S");
    let t = s.terrain_at(u.x, u.y);
    assert_eq!(att_value(&s, &u, t, true), att_value(&s, &u, t, false) + 65); // half of 130
    for power in ["N", "Y"] {
        let u = with_co(&mut s, "Kanbei", power);
        assert_eq!(att_value(&s, &u, t, true), att_value(&s, &u, t, false), "power {power}");
    }
}

#[test]
fn javier_defence_against_indirect_fire() {
    let mut s = state();
    for (power, bonus) in [("N", 20), ("Y", 40), ("S", 80)] {
        let u = with_co(&mut s, "Javier", power);
        assert_eq!(def_value(&s, &u, "Artillery"), def_value(&s, &u, "Tank") + bonus, "power {power}");
    }
}

#[test]
fn kindle_high_society_counts_com_towers_and_labs() {
    let id = |b: &str| *tables().terrain.iter().find(|(_, t)| t.building.as_deref() == Some(b)).unwrap().0;
    for b in ["HQ", "City", "Base", "Airport", "Port", "ComTower", "Lab"] {
        assert!(is_urban_property(id(b)), "{b}");
    }
    let mut s = state();
    let u = with_co(&mut s, "Kindle", "S");
    let t = s.terrain_at(u.x, u.y);
    let before = att_value(&s, &u, t, false);
    // Hand her a lab she didn't own: +3%.
    let key = *s.buildings.iter().find(|(k, b)| b.owner != Some(u.owner) && **k != (u.x, u.y)).unwrap().0;
    let b = s.buildings.get_mut(&key).unwrap();
    b.owner = Some(u.owner);
    b.terrain = id("Lab");
    assert_eq!(att_value(&s, &u, t, false), before + 3);
}

#[test]
fn colin_power_of_money_uses_the_funds_held_at_each_attack() {
    let mut s = state();
    let u = with_co(&mut s, "Colin", "S");
    s.player_mut(u.owner).unwrap().funds = 12_345;
    assert_eq!(att_milli(&s, &u), 37_035); // +37.035%
    s.player_mut(u.owner).unwrap().funds = 2_000; // spent since the power was activated
    assert_eq!(att_milli(&s, &u), 6_000);
    let u = with_co(&mut s, "Colin", "N");
    assert_eq!(att_milli(&s, &u), 0);
    // Base 100 at 100% + 37.035%: 137.035, so 137.
    assert_eq!(damage_range(&AttackParams { att_milli: 37_035, ..params(100, 10, 0, 10, 0) }), (137, 137));
}

#[test]
fn out_of_ammo_machine_guns() {
    assert_eq!(mg_damage("Mech", "Artillery"), Some(32));
    for (att, dmg) in [("Mech", 6), ("B-Copter", 6), ("Tank", 6), ("Md.Tank", 8), ("Neotank", 10), ("Mega Tank", 10)] {
        assert_eq!(mg_damage(att, "Piperunner"), Some(dmg), "{att}");
    }
}

#[test]
fn rockets_share_the_penalties_of_max_and_sensei() {
    assert_eq!(co::attack_bonus("Max", "N", "Rocket", "Plain", 0), -10);
    assert_eq!(co::range_bonus("Max", "N", "Rocket"), -1);
    assert_eq!(co::attack_bonus("Sensei", "N", "Rocket", "Plain", 0), -10);
}

#[test]
fn jugger_power_costs() {
    assert_eq!(co::power_base("Jugger"), Some((3 * 90_000, 7 * 90_000)));
}

#[test]
fn bad_luck_is_rolled_apart_from_good_luck() {
    let mut seq = [7u32, 3, 12, 25, 31].into_iter();
    let mut rng = move || seq.next().unwrap();
    // Sonja: good luck 0..=9 and bad luck 0..=9, one roll each.
    assert_eq!(co::roll_luck(-9, 9, &mut rng), 7 - 3);
    // No bad luck: a single roll.
    assert_eq!(co::roll_luck(0, 9, &mut rng), 2);
    // Flak: good 0..=24, bad 0..=9.
    assert_eq!(co::roll_luck(-9, 24, &mut rng), 0 - 1);
}
