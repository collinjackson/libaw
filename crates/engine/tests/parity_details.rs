//! Smaller AWBW rules that no recorded game in the corpus exercises: War Bonds pricing, the
//! cap on power costs, Jake's Piperunners, who a heal reaches, Eagle's copters out of fuel,
//! counterattacks on hidden units, and Kindle on pipes. See docs/awbw-rule-findings.md.

use awbw_engine::actions::{Action, Pos, Then};
use awbw_engine::data::tables;
use awbw_engine::rules::{base_damage_now, move_cost};
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

/// Two neighbouring tiles that treads can cross, with no building on either.
fn land_pair(s: &State) -> (Pos, Pos) {
    let open = |p: Pos| move_cost(s.terrain_at(p.0, p.1), "T").is_some() && !s.buildings.contains_key(&p);
    (0..s.height)
        .flat_map(|y| (0..s.width - 1).map(move |x| ((x, y), (x + 1, y))))
        .find(|&(a, b)| open(a) && open(b))
        .unwrap()
}

#[test]
fn a_blocked_drop_in_place_does_not_cost_the_transport_its_move() {
    let (mut s, me, foe) = board("Andy", "N", "Andy");
    let (here, there) = land_pair(&s);
    let apc = add(&mut s, unit(1, me, "APC", here));
    let rider = add(&mut s, Unit { carried_by: Some(apc), ..unit(2, me, "Infantry", here) });
    s.units.get_mut(&apc).unwrap().cargo.push(rider);
    add(&mut s, unit(3, foe, "Infantry", there)); // in the way (unseen, in a fog game)
    sim::step(&mut s, &Action::Move { unit: apc, path: vec![here], then: Then::Unload { cargo: rider, to: there } }, &mut avg).unwrap();
    assert_eq!(s.units[&apc].cargo, vec![rider], "still aboard");
    assert!(!s.units[&apc].moved, "and the APC may still move");
}

#[test]
fn transports_join_while_their_cargo_fits() {
    let (mut s, me, _) = board("Andy", "N", "Andy");
    let (here, there) = land_pair(&s);
    let a = add(&mut s, Unit { hp: Hp::exact(50), ..unit(1, me, "APC", here) });
    let rider = add(&mut s, Unit { carried_by: Some(a), ..unit(2, me, "Infantry", here) });
    s.units.get_mut(&a).unwrap().cargo.push(rider);
    let b = add(&mut s, Unit { hp: Hp::exact(40), ..unit(3, me, "APC", there) });
    assert!(awbw_engine::actions::options_at(&s, &s.units[&a], there).contains(&Then::Join));
    sim::step(&mut s, &Action::Move { unit: a, path: vec![here, there], then: Then::Join }, &mut avg).unwrap();
    assert!(!s.units.contains_key(&a));
    assert_eq!((s.units[&b].hp, s.units[&b].cargo.clone()), (Hp::exact(90), vec![rider]));
    assert_eq!(s.units[&rider].carried_by, Some(b));

    // Two loaded APCs have one slot between them: no join.
    let (mut s, me, _) = board("Andy", "N", "Andy");
    for (id, at) in [(1, here), (3, there)] {
        let apc = add(&mut s, Unit { hp: Hp::exact(50), ..unit(id, me, "APC", at) });
        let rider = add(&mut s, Unit { carried_by: Some(apc), ..unit(id + 1, me, "Infantry", at) });
        s.units.get_mut(&apc).unwrap().cargo.push(rider);
    }
    assert!(!awbw_engine::actions::options_at(&s, &s.units[&1], there).contains(&Then::Join));
}

#[test]
fn black_boat_repairs_keep_exact_hp() {
    // (HP, fuel short, funds) -> (HP after, cost): a Tank costs 700 a point.
    for (hp, low_fuel, funds, after, cost) in [
        (74, false, 5000, 84, 700),   // exactly +1.0, the fraction kept
        (90, false, 5000, 100, 700),  // 9.0 is still a paid repair
        (95, true, 5000, 100, 0),     // 9.x topped up free, since it needed fuel
        (95, false, 5000, 95, 0),     // ... but not when it needed nothing
        (50, true, 600, 50, 0),       // can't pay: resupply only
    ] {
        let (mut s, me, _) = board("Andy", "N", "Andy");
        let (here, there) = land_pair(&s);
        let boat = add(&mut s, unit(1, me, "Black Boat", here));
        let tank = add(&mut s, Unit { hp: Hp::exact(hp), ..unit(2, me, "Tank", there) });
        if low_fuel {
            s.units.get_mut(&tank).unwrap().fuel = 5;
        }
        s.player_mut(me).unwrap().funds = funds;
        sim::step(&mut s, &Action::Move { unit: boat, path: vec![here], then: Then::Repair { target: there } }, &mut avg).unwrap();
        assert_eq!((s.units[&tank].hp, funds - s.player(me).unwrap().funds), (Hp::exact(after), cost), "from {hp}");
        assert_eq!(s.units[&tank].fuel, tables().unit("Tank").fuel as i32, "resupplied either way");
    }
}

#[test]
fn under_sonjas_super_a_defender_that_cannot_fire_voids_the_attack() {
    let (mut s, me, foe) = board("Andy", "N", "Sonja");
    s.player_mut(foe).unwrap().power = "S".into();
    let (here, there) = land_pair(&s);
    let tank = add(&mut s, unit(1, me, "Tank", here));
    let gun = add(&mut s, Unit { ammo: 0, ..unit(2, foe, "Anti-Air", there) });
    sim::step(&mut s, &Action::Move { unit: tank, path: vec![here], then: Then::Attack { target: there } }, &mut avg).unwrap();
    assert_eq!((s.units[&tank].hp, s.units[&gun].hp), (Hp::exact(100), Hp::exact(100)));
}

#[test]
fn max_speeds_up_the_black_bomb_but_not_transports() {
    for (kind, fast) in [("Black Bomb", true), ("Tank", true), ("Fighter", true), ("APC", false), ("Infantry", false), ("Artillery", false)] {
        assert_eq!(co::moves_apply(kind, "direct"), fast, "{kind}");
    }
}

#[test]
fn sasha_gets_nothing_extra_when_properties_pay_nothing() {
    let (mut s, me, _) = board("Sasha", "N", "Andy");
    s.funds_per_property = 0;
    let before = s.player(me).unwrap().funds;
    let day = s.day.max(2);
    sim::start_turn(&mut s, me, day);
    assert_eq!(s.player(me).unwrap().funds, before);
}

#[test]
fn a_cruiser_out_of_ammo_cannot_fire_at_aircraft() {
    let cruiser = unit(1, 0, "Cruiser", (0, 0));
    assert!(base_damage_now(&cruiser, "B-Copter").is_some());
    assert!(base_damage_now(&Unit { ammo: 0, ..cruiser }, "B-Copter").is_none());
}
