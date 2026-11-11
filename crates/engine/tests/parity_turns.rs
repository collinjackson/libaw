//! Turn, movement and capture rules checked against AWBW's rules and recorded games:
//! power weather, transport movement, labs on maps without HQs, unloading over sea, and
//! ammo on pipe-seam attacks. See docs/sources.md.

use awbw_engine::actions::{Action, Pos, Then};
use awbw_engine::data::tables;
use awbw_engine::rules::{can_unload_from, movement_range};
use awbw_engine::{sim, Building, Hp, State, Unit, UnitId};

fn state() -> State {
    let turn: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/eval-harness/test-states/game-1755654-turn-020-D11-pid4205798.json"
    ))
    .unwrap();
    awbw_engine::awbw::state_from(&turn)
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

/// Put a unit on a tile, ready to act, clearing whatever stood there.
fn place(s: &mut State, id: UnitId, at: Pos) {
    let there: Vec<UnitId> = s.units.values().filter(|o| o.id != id && (o.x, o.y) == at).map(|o| o.id).collect();
    for o in there {
        s.units.remove(&o);
    }
    let u = s.units.get_mut(&id).unwrap();
    (u.x, u.y, u.moved, u.carried_by) = (at.0, at.1, false, None);
}

fn building_id(kind: &str) -> u16 {
    *tables().terrain.iter().find(|(_, t)| t.building.as_deref() == Some(kind)).unwrap().0
}

fn terrain_named(name: &str) -> u16 {
    *tables().terrain.iter().find(|(_, t)| t.name == name).unwrap().0
}

fn cast(s: &mut State, co: &str, power: &str) {
    let pid = s.current;
    with_co(s, co, "N");
    sim::spend_power(s, pid, power);
    assert!(sim::power_effects(s, pid, power));
}

#[test]
fn power_weather_ends_at_the_casters_next_turn() {
    let mut s = state();
    let me = s.current;
    let players = s.players.iter().filter(|p| !p.eliminated).count();
    s.weather = 'C';
    cast(&mut s, "Olaf", "Y");
    assert_eq!(s.weather, 'S');
    for _ in 1..players {
        sim::end_turn(&mut s).unwrap();
        assert_eq!(s.weather, 'S', "still snowing on the other players' turns");
    }
    sim::end_turn(&mut s).unwrap();
    assert_eq!((s.current, s.weather), (me, 'C'));
}

#[test]
fn a_later_weather_power_takes_over_the_timer() {
    let mut s = state();
    if s.players.len() != 2 {
        return;
    }
    let a = s.current;
    s.weather = 'C';
    cast(&mut s, "Olaf", "Y");
    sim::end_turn(&mut s).unwrap();
    let b = s.current;
    cast(&mut s, "Olaf", "Y");
    sim::end_turn(&mut s).unwrap(); // a's power ends; b's snow goes on
    assert_eq!((s.current, s.weather), (a, 'S'));
    sim::end_turn(&mut s).unwrap();
    assert_eq!((s.current, s.weather), (b, 'C'));
}

#[test]
fn transports_of_sami_and_sensei_move_one_further_and_no_more() {
    let mut s = state();
    for (co, power, want) in [("Andy", "N", 6), ("Sami", "N", 7), ("Sami", "Y", 7), ("Sami", "S", 7), ("Sensei", "N", 7)] {
        let mut u = with_co(&mut s, co, power);
        u.kind = "T-Copter".into();
        s.move_override.clear();
        assert_eq!(movement_range(&s, &u), want, "{co} {power}");
    }
}

#[test]
fn losing_the_last_lab_on_a_map_without_hqs_eliminates() {
    let (hq, lab, city) = (building_id("HQ"), building_id("Lab"), building_id("City"));
    for (hq_kept, labs, out) in [(false, 1, true), (false, 2, false), (true, 1, false)] {
        let mut s = state();
        let me = s.current;
        let foe = s.players.iter().find(|p| p.id != me).unwrap().id;
        let id = with_co(&mut s, "Andy", "N").id;
        let theirs: Vec<Pos> = s.buildings.iter().filter(|(_, b)| b.owner == Some(foe)).map(|(k, _)| *k).collect();
        assert!(theirs.len() >= 3);
        for (i, k) in theirs.iter().enumerate() {
            s.buildings.get_mut(k).unwrap().terrain =
                if i == 0 && hq_kept { hq } else if (1..=labs).contains(&i) { lab } else { city };
        }
        let target = theirs[1];
        s.buildings.get_mut(&target).unwrap().capture = 1;
        s.units.get_mut(&id).unwrap().kind = "Infantry".into();
        place(&mut s, id, target);
        sim::step(&mut s, &Action::Move { unit: id, path: vec![target], then: Then::Capture }, &mut |l: i32, h: i32| (l + h) / 2).unwrap();
        assert_eq!(s.buildings[&target].owner, Some(me));
        assert_eq!(s.player(foe).unwrap().eliminated, out, "HQ kept: {hq_kept}, labs: {labs}");
    }
}

#[test]
fn only_cruisers_and_carriers_unload_over_sea() {
    for water in ["Sea", "Reef"] {
        for t in ["APC", "T-Copter", "Lander", "Black Boat"] {
            assert!(!can_unload_from(t, terrain_named(water)), "{t} on {water}");
        }
        for t in ["Cruiser", "Carrier"] {
            assert!(can_unload_from(t, terrain_named(water)), "{t} on {water}");
        }
    }
    assert!(can_unload_from("T-Copter", terrain_named("Plain")));
}

#[test]
fn seam_attacks_spend_ammo_even_when_empty() {
    let mut s = state();
    let u = with_co(&mut s, "Andy", "N");
    let target: Pos = if u.x > 0 { (u.x - 1, u.y) } else { (u.x + 1, u.y) };
    place(&mut s, u.id, (u.x, u.y));
    let blockers: Vec<UnitId> = s.units.values().filter(|o| (o.x, o.y) == target).map(|o| o.id).collect();
    for o in blockers {
        s.units.remove(&o);
    }
    let seam = terrain_named("Pipe Seam");
    s.buildings.insert(target, Building { terrain: seam, owner: None, capture: 99 });
    let w = s.width as usize;
    s.terrain[target.1 as usize * w + target.0 as usize] = seam;
    let m = s.units.get_mut(&u.id).unwrap();
    m.kind = "Mech".into();
    m.ammo = 0;
    m.hp = Hp::exact(100);
    sim::step(&mut s, &Action::Move { unit: u.id, path: vec![(u.x, u.y)], then: Then::AttackSeam { target } }, &mut |l: i32, h: i32| (l + h) / 2).unwrap();
    assert_eq!(s.units[&u.id].ammo, -1);
    assert_eq!(s.buildings[&target].capture, 98); // the machine gun's 1%
}

/// The player to move starts a turn with one unit of `kind` at exactly `hp` on a city of
/// theirs and `funds` in hand (no income). Returns its HP afterwards and what its repair cost.
fn repair(co: &str, kind: &str, hp: u8, funds: i64) -> (Hp, i64) {
    let run = |hp: u8| {
        let mut s = state();
        let me = s.current;
        let id = with_co(&mut s, co, "N").id;
        let city = building_id("City");
        let at = *s.buildings.iter().find(|(_, b)| b.owner == Some(me)).unwrap().0;
        s.buildings.get_mut(&at).unwrap().terrain = city;
        place(&mut s, id, at);
        let u = s.units.get_mut(&id).unwrap();
        (u.kind, u.hp, u.fuel) = (kind.into(), Hp::exact(hp), 1);
        s.funds_per_property = 0;
        s.player_mut(me).unwrap().funds = funds;
        let day = s.day;
        sim::start_turn(&mut s, me, day);
        assert_eq!(s.units[&id].fuel, tables().unit(kind).fuel as i32, "resupplied either way");
        (s.units[&id].hp, s.player(me).unwrap().funds)
    };
    let ((after, left), (_, left_if_healthy)) = (run(hp), run(100));
    (after, left_if_healthy - left)
}

#[test]
fn property_repairs_keep_exact_hp() {
    let rich = 100_000;
    // A chip-damaged unit is topped up to exactly 10.0 for free.
    assert_eq!(repair("Andy", "Tank", 95, rich), (Hp::exact(100), 0));
    // From display 9: one point's price.
    assert_eq!(repair("Andy", "Infantry", 85, rich), (Hp::exact(100), 100));
    // 8.0 goes to exactly 10.0; 7.9 gains exactly 2.0. Both pay for two points.
    assert_eq!(repair("Andy", "Infantry", 80, rich), (Hp::exact(100), 200));
    assert_eq!(repair("Andy", "Infantry", 79, rich), (Hp::exact(99), 200));
    assert_eq!(repair("Andy", "Infantry", 34, rich), (Hp::exact(54), 200));
    // Rachel: 7.0 and up to 10.0, otherwise +3.0.
    assert_eq!(repair("Rachel", "Infantry", 75, rich), (Hp::exact(100), 200));
    assert_eq!(repair("Rachel", "Infantry", 70, rich), (Hp::exact(100), 300));
    assert_eq!(repair("Rachel", "Infantry", 69, rich), (Hp::exact(99), 300));
}

#[test]
fn property_repairs_are_all_or_nothing() {
    // Two points of a Tank cost 1400: with 1000 there is no repair at all.
    assert_eq!(repair("Andy", "Tank", 50, 1000), (Hp::exact(50), 0));
    assert_eq!(repair("Andy", "Tank", 50, 1400), (Hp::exact(70), 1400));
}
