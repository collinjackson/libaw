//! Turn, movement and capture rules checked against AWBW's rules and recorded games:
//! power weather, transport movement, labs on maps without HQs, unloading over sea, and
//! ammo on pipe-seam attacks. See docs/sources.md.

use awbw_engine::actions::{Action, Pos, Then};
use awbw_engine::data::tables;
use awbw_engine::rules::{at_unit_limit, can_unload_from, is_income_property, league_capture_limit, movement_range};
use awbw_engine::{fog, sim, Building, Hp, State, Unit, UnitId};

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

/// A board with only these enemy units on it (kind, x, y), the player to move as `co`.
fn missile_board(co: &str, enemies: &[(&str, u16, u16)]) -> (State, Vec<UnitId>) {
    let mut s = state();
    let me = s.current;
    with_co(&mut s, co, "N");
    let foe = s.players.iter().find(|p| p.id != me).unwrap().id;
    s.player_mut(foe).unwrap().co = "Andy".into();
    s.units.clear();
    let mut ids = vec![];
    for (i, &(kind, x, y)) in enemies.iter().enumerate() {
        let id = 1000 + i as UnitId;
        s.units.insert(id, Unit {
            id, owner: foe, kind: kind.into(), x, y, hp: Hp::exact(100), fuel: 50, ammo: 0,
            moved: false, stunned: false, carried_by: None, cargo: vec![], dived: false,
        });
        ids.push(id);
    }
    for b in s.buildings.values_mut() {
        b.capture = 20;
    }
    (s, ids)
}

#[test]
fn rachels_missiles_are_all_aimed_before_any_lands() {
    // Three Infantry in one corner, two expensive tanks far away. The foot-soldier missile
    // and the HP missile both pick the Infantry, the value missile picks the tanks.
    let far = (state().width - 3, state().height - 3);
    let (mut s, ids) = missile_board("Rachel", &[
        ("Infantry", 2, 2), ("Infantry", 3, 2), ("Infantry", 2, 3),
        ("Md.Tank", far.0, far.1), ("Neotank", far.0 + 1, far.1),
    ]);
    let me = s.current;
    assert!(sim::power_effects(&mut s, me, "S"));
    let hp: Vec<Hp> = ids.iter().map(|id| s.units[id].hp).collect();
    assert_eq!(hp, [Hp::exact(40), Hp::exact(40), Hp::exact(40), Hp::exact(70), Hp::exact(70)]);
}

#[test]
fn von_bolt_aims_at_the_most_value_and_stuns_it() {
    let far = (state().width - 3, state().height - 3);
    let (mut s, ids) = missile_board("Von Bolt", &[
        ("Infantry", 2, 2), ("Infantry", 3, 2), ("Infantry", 2, 3),
        ("Md.Tank", far.0, far.1), ("Neotank", far.0 + 1, far.1),
    ]);
    let me = s.current;
    assert!(sim::power_effects(&mut s, me, "S"));
    let hit: Vec<(u8, bool)> = ids.iter().map(|id| (s.units[id].hp.hi, s.units[id].stunned)).collect();
    assert_eq!(hit, [(100, false), (100, false), (100, false), (70, true), (70, true)]);
}

#[test]
fn a_missile_with_nothing_to_hit_is_not_fired_and_floors_at_a_tenth() {
    let (mut s, _) = missile_board("Von Bolt", &[]);
    let me = s.current;
    assert!(sim::power_effects(&mut s, me, "S"));
    // A 2.0 HP unit loses 1.9, not 1.0.
    let (mut s, ids) = missile_board("Von Bolt", &[("Md.Tank", 4, 4)]);
    s.units.get_mut(&ids[0]).unwrap().hp = Hp::exact(20);
    assert!(sim::power_effects(&mut s, me, "S"));
    assert_eq!(s.units[&ids[0]].hp, Hp::exact(1));
}

/// A game without fog: my Fighter at `start`, an enemy hidden Stealth two tiles along the
/// row, nothing else on the board. Returns the state, the Fighter's id and the three tiles
/// from the Fighter onwards (the Stealth is on the middle one).
fn hidden_stealth_board(gap: u16) -> (State, UnitId, [Pos; 3]) {
    let mut s = state();
    (s.fog, s.partial) = (false, false);
    let me = s.current;
    let foe = s.players.iter().find(|p| p.id != me).unwrap().id;
    let id = with_co(&mut s, "Andy", "N").id;
    s.units.retain(|k, _| *k == id);
    let y = s.height / 2;
    let x = 3;
    let tiles = [(x + 1, y), (x + 2, y), (x + 3, y)];
    let u = s.units.get_mut(&id).unwrap();
    (u.kind, u.x, u.y, u.fuel, u.ammo, u.moved, u.hp) = ("Fighter".into(), x + 2 - gap, y, 99, 9, false, Hp::exact(100));
    s.units.insert(7000, Unit {
        id: 7000, owner: foe, kind: "Stealth".into(), x: tiles[1].0, y: tiles[1].1, hp: Hp::exact(100), fuel: 60, ammo: 6,
        moved: false, stunned: false, carried_by: None, cargo: vec![], dived: true,
    });
    for t in tiles {
        s.buildings.remove(&t);
    }
    (s, id, tiles)
}

#[test]
fn hidden_units_are_out_of_sight_without_fog_too() {
    // Two tiles away: unseen, and walking into it is a trap.
    let (mut s, id, tiles) = hidden_stealth_board(2);
    let me = s.current;
    let view = fog::player_view(&s, me);
    assert!(!view.units.contains_key(&7000) && view.partial);
    let start = (s.units[&id].x, s.units[&id].y);
    sim::step(&mut s, &Action::Move { unit: id, path: vec![start, tiles[0], tiles[1], tiles[2]], then: Then::Wait }, &mut |l: i32, h: i32| (l + h) / 2).unwrap();
    let u = &s.units[&id];
    assert_eq!(((u.x, u.y), u.moved, u.fuel), (tiles[0], true, 98), "stopped one short, having walked one tile");

    // It can't be attacked by moving up to it either: it wasn't revealed before the move.
    let (mut s, id, tiles) = hidden_stealth_board(2);
    let start = (s.units[&id].x, s.units[&id].y);
    assert!(awbw_engine::actions::attack_targets(&s, &s.units[&id], tiles[0], true).is_empty());
    let attack = Action::Move { unit: id, path: vec![start, tiles[0]], then: Then::Attack { target: tiles[1] } };
    assert!(sim::step(&mut s, &attack, &mut |l: i32, h: i32| (l + h) / 2).is_err());
}

#[test]
fn a_unit_beside_a_hidden_unit_or_a_property_under_it_reveals_it() {
    // Already next to it: seen, attackable, and it blocks the path like any enemy.
    let (mut s, id, tiles) = hidden_stealth_board(1);
    let me = s.current;
    assert!(fog::player_view(&s, me).units.contains_key(&7000));
    assert_eq!(awbw_engine::actions::attack_targets(&s, &s.units[&id], tiles[0], false), vec![tiles[1]]);
    let through = Action::Move { unit: id, path: vec![tiles[0], tiles[1], tiles[2]], then: Then::Wait };
    assert!(sim::step(&mut s, &through, &mut |l: i32, h: i32| (l + h) / 2).is_err());

    // Two tiles away, but it is over one of my properties: seen.
    let (mut s, _, tiles) = hidden_stealth_board(2);
    s.buildings.insert(tiles[1], Building { terrain: building_id("City"), owner: Some(me), capture: 20 });
    assert!(fog::player_view(&s, me).units.contains_key(&7000));
}

#[test]
fn no_builds_at_the_unit_limit() {
    let mut s = state();
    let me = s.current;
    s.player_mut(me).unwrap().funds = 100_000;
    let mine = s.units.values().filter(|u| u.owner == me).count() as u32;
    // Free up a base so there is something to build on.
    let base = *s.buildings.iter().find(|(_, b)| b.owner == Some(me)).unwrap().0;
    s.buildings.get_mut(&base).unwrap().terrain = building_id("Base");
    s.units.retain(|_, u| (u.x, u.y) != base || u.owner != me);
    let mine = mine.min(s.units.values().filter(|u| u.owner == me).count() as u32);
    s.unit_limit = Some(mine + 1);
    assert!(!at_unit_limit(&s, me) && !awbw_engine::actions::builds(&s).is_empty());
    s.unit_limit = Some(mine);
    assert!(at_unit_limit(&s, me) && awbw_engine::actions::builds(&s).is_empty());
    s.unit_limit = None;
    assert!(!awbw_engine::actions::builds(&s).is_empty());
    assert_eq!(state().unit_limit, Some(50), "AWBW's default");
}

#[test]
fn reaching_the_capture_limit_wins() {
    for (short_by, wins) in [(1, true), (2, false)] {
        let mut s = state();
        let me = s.current;
        let foe = s.players.iter().find(|p| p.id != me).unwrap().id;
        let id = with_co(&mut s, "Andy", "N").id;
        let owned = s.buildings.values().filter(|b| b.owner == Some(me) && is_income_property(b.terrain)).count() as u32;
        s.capture_limit = Some(owned + short_by);
        // A city of the other player's, one point from falling.
        let target = *s.buildings.iter().find(|(_, b)| b.owner == Some(foe)).unwrap().0;
        let b = s.buildings.get_mut(&target).unwrap();
        (b.terrain, b.capture) = (building_id("City"), 1);
        s.units.get_mut(&id).unwrap().kind = "Infantry".into();
        place(&mut s, id, target);
        sim::step(&mut s, &Action::Move { unit: id, path: vec![target], then: Then::Capture }, &mut |l: i32, h: i32| (l + h) / 2).unwrap();
        assert_eq!(s.player(foe).unwrap().eliminated, wins, "limit {short_by} above what I held");
        assert_eq!(sim::winner(&s).is_some(), wins);
    }
    // League games set the limit to 70% of the map's income properties, rounded down.
    let s = state();
    let on_map = s.buildings.values().filter(|b| is_income_property(b.terrain)).count() as u32;
    assert_eq!(league_capture_limit(&s), on_map * 7 / 10);
    assert_eq!(s.capture_limit, None, "off unless set");
}

#[test]
fn senseis_spawns_stop_at_the_unit_limit() {
    let mut s = state();
    let me = s.current;
    with_co(&mut s, "Sensei", "N");
    s.units.clear();
    let city = building_id("City");
    let mine: Vec<Pos> = s.buildings.iter().filter(|(_, b)| b.owner == Some(me)).map(|(k, _)| *k).collect();
    assert!(mine.len() >= 3);
    for k in &mine {
        s.buildings.get_mut(k).unwrap().terrain = city;
    }
    s.unit_limit = Some(2);
    assert!(sim::power_effects(&mut s, me, "Y"));
    let mut spawned: Vec<Pos> = s.units.values().map(|u| (u.x, u.y)).collect();
    spawned.sort_by_key(|&(x, y)| (y, x));
    let mut by_row = mine.clone();
    by_row.sort_by_key(|&(x, y)| (y, x));
    assert_eq!(spawned, by_row[..2], "the first two cities, row by row");
}
