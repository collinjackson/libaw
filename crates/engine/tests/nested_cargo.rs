//! A transport's cargo goes down with it, and so does its cargo's cargo.
use libaw::{sim, Hp, Player, State, Unit};
use std::collections::BTreeMap;

fn unit(id: u32, kind: &str, carried_by: Option<u32>, cargo: Vec<u32>) -> Unit {
    let info = libaw::data::tables().unit(kind);
    Unit { id, owner: 1, kind: kind.into(), x: 1, y: 0, hp: Hp::exact(100), fuel: info.fuel as i32, ammo: info.ammo as i32,
        moved: false, stunned: false, carried_by, cargo, dived: false }
}

#[test]
fn a_lost_lander_takes_its_apc_and_the_apcs_infantry() {
    let p = |id| Player { id, team: id.to_string(), co: "Andy".into(), funds: 0, eliminated: false, order: id, power: "N".into(),
        meter: 0, cop_cost: 0, scop_cost: 0, power_attack: 0 };
    let mut s = State {
        width: 3, height: 1, terrain: vec![28; 3], buildings: BTreeMap::new(), units: BTreeMap::new(), players: vec![p(1), p(2)],
        current: 2, day: 5, funds_per_property: 1000, weather: 'C', move_override: BTreeMap::new(), powers: false,
        seams: false, dives: false, deletes: false, fog: false, partial: false, banned: vec![], unit_limit: None, capture_limit: None,
    };
    s.units.insert(1, unit(1, "Lander", None, vec![2]));
    s.units.insert(2, unit(2, "APC", Some(1), vec![3]));
    s.units.insert(3, unit(3, "Infantry", Some(2), vec![]));
    sim::remove_unit(&mut s, 1);
    assert!(s.units.is_empty(), "left behind: {:?}", s.units.keys().collect::<Vec<_>>());
}
