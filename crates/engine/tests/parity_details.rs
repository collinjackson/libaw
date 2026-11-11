//! Smaller AWBW rules that no recorded game in the corpus exercises: War Bonds pricing, the
//! cap on power costs, Jake's Piperunners, who a heal reaches, Eagle's copters out of fuel,
//! counterattacks on hidden units, and Kindle on pipes. See docs/awbw-rule-findings.md.

use awbw_engine::actions::{Action, Pos, Then};
use awbw_engine::data::tables;
use awbw_engine::{co, combat, sim, Hp, PlayerId, State, Unit, UnitId};

fn state() -> State {
    let turn: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/eval-harness/test-states/game-1755654-turn-020-D11-pid4205798.json"
    ))
    .unwrap();
    awbw_engine::awbw::state_from(&turn)
}

fn unit(id: UnitId, owner: PlayerId, kind: &str, at: Pos) -> Unit {
    let info = tables().unit(kind);
    Unit {
        id, owner, kind: kind.into(), x: at.0, y: at.1, hp: Hp::exact(100), fuel: info.fuel as i32, ammo: info.ammo as i32,
        moved: false, stunned: false, carried_by: None, cargo: vec![], dived: false,
    }
}

/// An empty board in a game without fog: the player to move (`me`) as `co` with `power`
/// on, the other player (`foe`) as `foe_co`. No buildings are mid-capture.
fn board(co: &str, power: &str, foe_co: &str) -> (State, PlayerId, PlayerId) {
    let mut s = state();
    (s.fog, s.partial) = (false, false);
    let me = s.current;
    let foe = s.players.iter().find(|p| p.id != me).unwrap().id;
    let p = s.player_mut(me).unwrap();
    (p.co, p.power) = (co.into(), power.into());
    let p = s.player_mut(foe).unwrap();
    (p.co, p.power) = (foe_co.into(), "N".into());
    s.units.clear();
    (s, me, foe)
}

fn add(s: &mut State, u: Unit) -> UnitId {
    let id = u.id;
    s.units.insert(id, u);
    id
}

fn avg(lo: i32, hi: i32) -> i32 {
    (lo + hi) / 2
}

#[test]
fn war_bonds_pays_on_the_victims_price_to_its_owner() {
    for (foe_co, price) in [("Andy", 7000), ("Kanbei", 8400), ("Colin", 5600), ("Hachi", 6300)] {
        let (mut s, me, foe) = board("Sasha", "S", foe_co);
        let mine = add(&mut s, unit(1, me, "Md.Tank", (4, 5)));
        let theirs = add(&mut s, unit(2, foe, "Tank", (5, 5)));
        let before = s.player(me).unwrap().funds;
        sim::step(&mut s, &Action::Move { unit: mine, path: vec![(4, 5)], then: Then::Attack { target: (5, 5) } }, &mut avg).unwrap();
        let lost = 10 - s.units.get(&theirs).map_or(0, |u| u.hp.display_hi()) as i64;
        assert!(lost > 0);
        assert_eq!(s.player(me).unwrap().funds - before, price * lost / 20, "against {foe_co}");
    }
}

#[test]
fn power_costs_stop_growing_after_the_tenth_use() {
    let (mut s, me, _) = board("Andy", "N", "Andy");
    let (base, super_base) = co::power_base("Andy").unwrap();
    let p = s.player_mut(me).unwrap();
    (p.cop_cost, p.scop_cost) = (base, super_base);
    for used in 1..=12i64 {
        sim::spend_power(&mut s, me, "Y");
        let p = s.player(me).unwrap();
        let fifths = 5 + used.min(10);
        assert_eq!((p.cop_cost, p.scop_cost), (base * fifths / 5, super_base * fifths / 5), "after {used} uses");
    }
}

#[test]
fn jakes_powers_lengthen_piperunner_range() {
    assert_eq!(co::range_bonus("Jake", "N", "Piperunner"), 0);
    assert_eq!(co::range_bonus("Jake", "Y", "Piperunner"), 1);
    assert_eq!(co::range_bonus("Jake", "S", "Piperunner"), 1);
    assert_eq!(co::range_bonus("Jake", "S", "Battleship"), 0);
}

#[test]
fn heals_reach_own_units_and_only_andys_reaches_cargo() {
    for (who, power, gain, cargo_healed) in [("Andy", "Y", 20, true), ("Hawke", "Y", 10, false)] {
        let (mut s, me, foe) = board(who, "N", "Andy");
        // The other player is a teammate here.
        let team = s.player(me).unwrap().team.clone();
        s.player_mut(foe).unwrap().team = team;
        let apc = add(&mut s, unit(1, me, "APC", (4, 4)));
        let rider = add(&mut s, Unit { hp: Hp::exact(50), carried_by: Some(apc), ..unit(2, me, "Infantry", (4, 4)) });
        s.units.get_mut(&apc).unwrap().cargo.push(rider);
        let tank = add(&mut s, Unit { hp: Hp::exact(50), ..unit(3, me, "Tank", (6, 6)) });
        let ally = add(&mut s, Unit { hp: Hp::exact(50), ..unit(4, foe, "Tank", (8, 8)) });
        assert!(sim::power_effects(&mut s, me, power));
        assert_eq!(s.units[&tank].hp, Hp::exact(50 + gain), "{who}: own unit");
        assert_eq!(s.units[&rider].hp, Hp::exact(if cargo_healed { 50 + gain } else { 50 }), "{who}: carried unit");
        assert_eq!(s.units[&ally].hp, Hp::exact(50), "{who}: a teammate's unit");
    }
}

#[test]
fn an_eagle_copter_out_of_fuel_crashes() {
    for (fuel, survives) in [(0, false), (1, true)] {
        let (mut s, me, _) = board("Eagle", "N", "Andy");
        let at = (4, 4);
        s.buildings.remove(&at);
        let copter = add(&mut s, Unit { fuel, ..unit(1, me, "B-Copter", at) });
        let day = s.day.max(2);
        sim::start_turn(&mut s, me, day);
        assert_eq!(s.units.contains_key(&copter), survives, "fuel {fuel}");
    }
}

#[test]
fn a_counterattack_cannot_hurt_a_hidden_stealth() {
    let (mut s, me, foe) = board("Andy", "N", "Andy");
    let stealth = add(&mut s, Unit { dived: true, ..unit(1, me, "Stealth", (4, 5)) });
    let gun = add(&mut s, unit(2, foe, "Anti-Air", (5, 5)));
    let ammo = s.units[&gun].ammo;
    sim::step(&mut s, &Action::Move { unit: stealth, path: vec![(4, 5)], then: Then::Attack { target: (5, 5) } }, &mut avg).unwrap();
    assert!(s.units.contains_key(&gun), "the Anti-Air survives the first strike");
    assert_eq!(s.units[&stealth].hp, Hp::exact(100));
    assert_eq!(s.units[&gun].ammo, ammo - 1, "it still fired");

    // Revealed, it takes the counter as usual.
    let (mut s, me, foe) = board("Andy", "N", "Andy");
    let stealth = add(&mut s, unit(1, me, "Stealth", (4, 5)));
    add(&mut s, unit(2, foe, "Anti-Air", (5, 5)));
    sim::step(&mut s, &Action::Move { unit: stealth, path: vec![(4, 5)], then: Then::Attack { target: (5, 5) } }, &mut avg).unwrap();
    assert!(s.units[&stealth].hp.hi < 100);
}

#[test]
fn kindles_urban_bonus_is_for_properties_not_pipes() {
    let u = unit(1, 0, "Piperunner", (0, 0));
    let id = |name: &str| *tables().terrain.iter().find(|(_, t)| t.name == name).unwrap().0;
    let city = *tables().terrain.iter().find(|(_, t)| t.building.as_deref() == Some("City")).unwrap().0;
    assert_eq!(combat::bonus_terrain(&u, id("HPipe"), "Building"), "Pipe");
    assert_eq!(combat::bonus_terrain(&u, city, "Building"), "Building");
    assert_eq!(co::attack_bonus("Kindle", "N", "Piperunner", "Pipe", 0), 0);
    assert_eq!(co::attack_bonus("Kindle", "N", "Piperunner", "Building", 0), 40);
}
