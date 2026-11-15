//! Rules that AWBW's end-to-end suite (github.com/advancewarsbyweb/awbw-functional-tests) plays
//! through on the live site, checked here on synthetic boards: building, moving, a two-turn
//! HQ capture that wins the game, and damage calculator results that sit on rounding and CO
//! rules. The suite drives the site's UI; these describe the same behaviour in engine terms.

use libaw::actions::{legal_actions, unit_actions, Action, Pos, Then};
use libaw::data::{tables, BuildingKind};
use libaw::{sim, Hp, PlayerId, State, Unit, UnitId};
use serde_json::json;

const OS: PlayerId = 1;
const BM: PlayerId = 2;

/// A board of plains with the given buildings ([x, y, terrain id, owner]), two players as
/// `os_co` and `bm_co`, Orange Star to move on day 1 with no funds. No fog.
fn board(w: u16, h: u16, buildings: &[(u16, u16, u16, Option<PlayerId>)], os_co: &str, bm_co: &str) -> State {
    let mut terrain = vec![1u16; w as usize * h as usize];
    for &(x, y, t, _) in buildings {
        terrain[y as usize * w as usize + x as usize] = t;
    }
    let bs: Vec<_> = buildings.iter().map(|&(x, y, t, o)| json!([x, y, {"terrain": t, "owner": o, "capture": 20}])).collect();
    let player = |id: PlayerId, co: &str, order: u32| json!({
        "id": id, "team": id.to_string(), "co": co, "funds": 0, "eliminated": false, "order": order, "power": "N",
    });
    serde_json::from_value(json!({
        "width": w, "height": h, "terrain": terrain, "buildings": bs, "units": {},
        "players": [player(OS, os_co, 0), player(BM, bm_co, 1)],
        "current": OS, "day": 1, "funds_per_property": 1000, "weather": "C", "move_override": {}, "banned": [],
    }))
    .unwrap()
}

/// The suite's two-player test map, 5x5 plains: Orange Star's Base at (1, 1) and HQ at (3, 3),
/// Blue Moon's Base at (3, 1) and HQ at (1, 3). Each Base is two tiles above the enemy HQ.
fn test_map() -> State {
    board(5, 5, &[(1, 1, 39, Some(OS)), (3, 1, 44, Some(BM)), (1, 3, 47, Some(BM)), (3, 3, 42, Some(OS))], "Andy", "Andy")
}

fn avg(lo: i32, hi: i32) -> i32 {
    (lo + hi) / 2
}

fn offered(s: &State, unit: UnitId, to: Pos, then: Then) -> bool {
    legal_actions(s).iter().any(|a| matches!(a, Action::Move { unit: u, path, then: t } if *u == unit && path.last() == Some(&to) && *t == then))
}

#[test]
fn build_then_capture_the_enemy_hq_over_two_turns_to_win() {
    let mut s = test_map();
    // Day 1: two income properties pay 2000 at the start of the turn.
    sim::start_turn(&mut s, OS, 1);
    assert_eq!(s.player(OS).unwrap().funds, 2000);

    // Infantry is offered at the Base; building it costs 1000 and the new unit can't act yet.
    let build = Action::Build { at: (1, 1), kind: "Infantry".into() };
    assert!(legal_actions(&s).contains(&build));
    sim::step(&mut s, &build, &mut avg).unwrap();
    assert_eq!(s.player(OS).unwrap().funds, 1000);
    let inf = s.unit_at(1, 1).expect("built").id;
    let u = &s.units[&inf];
    assert_eq!((u.owner, u.hp, u.fuel, u.moved), (OS, Hp::exact(100), tables().unit("Infantry").fuel as i32, true));
    assert!(unit_actions(&s, inf).is_empty(), "a unit built this turn waits for its owner's next turn");
    // The Base is taken for the rest of the turn.
    assert!(!legal_actions(&s).iter().any(|a| matches!(a, Action::Build { at: (1, 1), .. })));

    // Ending the turn hands play to Blue Moon, and back again on day 2.
    sim::step(&mut s, &Action::EndTurn, &mut avg).unwrap();
    assert_eq!((s.current, s.day), (BM, 1));
    sim::step(&mut s, &Action::EndTurn, &mut avg).unwrap();
    assert_eq!((s.current, s.day, s.player(OS).unwrap().funds), (OS, 2, 3000));
    assert!(!s.units[&inf].moved);

    // Day 2: the Infantry walks two plains to Blue Moon's HQ; a 10 HP unit takes 10 of 20.
    assert!(offered(&s, inf, (1, 3), Then::Capture));
    sim::step(&mut s, &Action::Move { unit: inf, path: vec![(1, 1), (1, 2), (1, 3)], then: Then::Capture }, &mut avg).unwrap();
    let hq = &s.buildings[&(1, 3)];
    assert_eq!((hq.owner, hq.capture), (Some(BM), 10));
    assert!(sim::winner(&s).is_none());

    // The half-done capture survives Blue Moon's turn.
    sim::step(&mut s, &Action::EndTurn, &mut avg).unwrap();
    sim::step(&mut s, &Action::EndTurn, &mut avg).unwrap();
    assert_eq!((s.current, s.day, s.buildings[&(1, 3)].capture), (OS, 3, 10));

    // Day 3: Capture again without moving. The HQ falls, Blue Moon is out and Orange Star wins.
    assert!(offered(&s, inf, (1, 3), Then::Capture));
    sim::step(&mut s, &Action::Move { unit: inf, path: vec![(1, 3)], then: Then::Capture }, &mut avg).unwrap();
    let hq = &s.buildings[&(1, 3)];
    assert_eq!(hq.owner, Some(OS));
    assert_eq!(tables().tile(hq.terrain).building, Some(BuildingKind::City), "a captured HQ becomes a city");
    assert!(s.player(BM).unwrap().eliminated);
    assert_eq!(sim::winner(&s).as_deref(), Some("1"));
}

/// Damage percent range (no luck, best luck) of `att` on `def` as AWBW's damage calculator
/// gives it: each side's unit, display HP and the terrain it stands on, Orange Star attacking
/// as `att_co`, Blue Moon defending as `def_co` with `def_power` on.
fn calc(att_co: &str, att: (&str, u8, u16), def_co: &str, def_power: &str, def: (&str, u8, u16)) -> (u8, u8) {
    let mut s = board(2, 1, &[], att_co, def_co);
    s.player_mut(BM).unwrap().power = def_power.into();
    s.terrain = vec![att.2, def.2];
    let unit = |id: UnitId, owner, (kind, hp, _): (&str, u8, u16), x| {
        let info = tables().unit(kind);
        Unit {
            id, owner, kind: kind.into(), x, y: 0, hp: Hp::exact(hp * 10), fuel: info.fuel as i32, ammo: info.ammo as i32,
            moved: false, stunned: false, carried_by: None, cargo: vec![], dived: false,
        }
    };
    let (a, d) = (unit(1, OS, att, 0), unit(2, BM, def, 1));
    (sim::damage(&s, &a, &d, 0, false), sim::damage(&s, &a, &d, 9, false))
}

const PLAIN: u16 = 1;
const WOOD: u16 = 3;
const ROAD: u16 = 15;
const CITY: u16 = 34;

#[test]
fn calculator_rounds_up_from_x95_and_a_hit_of_099_does_one() {
    // 8 HP Tank on a Tank in a wood: best luck 0.8 * 64 * 0.8 = 40.96, so 41.
    assert_eq!(calc("Andy", ("Tank", 8, PLAIN), "Andy", "N", ("Tank", 10, WOOD)), (35, 41));
    // Infantry on a 1 HP Md.Tank, no luck: 1 * 0.99 (one star under 1 HP) = 0.99, so 1.
    assert_eq!(calc("Andy", ("Infantry", 10, PLAIN), "Andy", "N", ("Md.Tank", 1, PLAIN)), (1, 9));
}

#[test]
fn calculator_keeps_double_precision_just_below_x95() {
    // 5 HP Neotank on 5 HP Infantry in a city: 56.95 on paper at best luck, 56 in doubles.
    assert_eq!(calc("Andy", ("Neotank", 5, PLAIN), "Andy", "N", ("Infantry", 5, CITY)), (53, 56));
    // 5 HP Tank on a 6 HP Recon: 39.95 on paper with no luck, 39 in doubles.
    assert_eq!(calc("Andy", ("Tank", 5, PLAIN), "Andy", "N", ("Recon", 6, PLAIN)), (39, 44));
}

#[test]
fn calculator_javier_super_against_indirect_fire() {
    assert_eq!(calc("Andy", ("Artillery", 10, PLAIN), "Javier", "N", ("Tank", 10, ROAD)), (56, 63));
    assert_eq!(calc("Andy", ("Artillery", 10, PLAIN), "Javier", "S", ("Tank", 10, ROAD)), (7, 7));
}

#[test]
fn calculator_max_and_sensei_rockets_take_ten_percent_off() {
    assert_eq!(calc("Andy", ("Rocket", 10, PLAIN), "Andy", "N", ("Md.Tank", 10, PLAIN)), (49, 57));
    for co in ["Max", "Sensei"] {
        assert_eq!(calc(co, ("Rocket", 10, PLAIN), "Andy", "N", ("Md.Tank", 10, PLAIN)), (44, 52), "{co}");
    }
}
