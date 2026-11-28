//! Resigning (AWBW): in a team game only on your own turn, in a 1v1 at any time; a player out
//! while the game goes on leaves their properties neutral and their HQ a neutral city.
use libaw::{sim, state::State};
use serde_json::json;

/// A 4x1 board: player 2's HQ (Blue Moon's, 47) at (0, 0) and City (43) at (1, 0), a Tank of
/// theirs at (2, 0); `teams` gives each player's team, player 1 to move.
fn board(teams: &[(u32, &str)]) -> State {
    let p = |id: u32, team: &str, order: usize| json!({"id": id, "team": team, "co": "Andy", "funds": 0, "eliminated": false,
        "order": order, "power": "N"});
    let players: Vec<_> = teams.iter().enumerate().map(|(i, &(id, t))| p(id, t, i)).collect();
    serde_json::from_value(json!({
        "width": 4, "height": 1, "terrain": [47, 43, 1, 1],
        "buildings": [[0, 0, {"terrain": 47, "owner": 2, "capture": 20}], [1, 0, {"terrain": 43, "owner": 2, "capture": 12}]],
        "units": {"5": {"id": 5, "owner": 2, "kind": "Tank", "x": 2, "y": 0, "hp": {"lo": 100, "hi": 100}, "fuel": 70, "ammo": 9,
            "moved": false, "stunned": false, "carried_by": null, "cargo": [], "dived": false}},
        "players": players, "current": 1, "day": 3, "funds_per_property": 1000, "weather": "C",
        "move_override": {}, "banned": [],
    }))
    .unwrap()
}

#[test]
fn a_team_game_refuses_a_resign_off_turn_and_takes_one_on_turn() {
    let mut s = board(&[(1, "A"), (2, "A"), (3, "B")]);
    let e = sim::resign(&mut s, 2).unwrap_err();
    assert!(e.contains("own turn"), "{e}");
    assert!(!s.player(2).unwrap().eliminated);
    s.current = 2;
    sim::resign(&mut s, 2).unwrap();
    assert!(s.player(2).unwrap().eliminated);
    assert_ne!(s.current, 2, "resigning ends the turn");
    // The game goes on (team A has player 1): properties neutral, the HQ a neutral City, units gone.
    let hq = &s.buildings[&(0, 0)];
    assert_eq!((hq.owner, hq.terrain), (None, 34));
    assert_eq!(s.terrain[0], 34);
    let city = &s.buildings[&(1, 0)];
    assert_eq!((city.owner, city.terrain, city.capture), (None, 34, 20));
    assert!(s.units.is_empty());
    assert!(sim::winner(&s).is_none());
}

#[test]
fn a_1v1_takes_a_resign_at_any_time_and_ends() {
    let mut s = board(&[(1, "1"), (2, "2")]);
    sim::resign(&mut s, 2).unwrap();
    assert_eq!(sim::winner(&s).as_deref(), Some("1"));
    // Game over: the board is left as it stood.
    assert_eq!(s.buildings[&(0, 0)].owner, Some(2));
    assert_eq!(s.current, 1);
}

#[test]
fn a_free_for_all_takes_a_resign_off_turn_for_now() {
    // AWBW's rule for 3+ players with no shared teams is unverified; allowed here.
    let mut s = board(&[(1, "1"), (2, "2"), (3, "3")]);
    sim::resign(&mut s, 2).unwrap();
    assert!(s.player(2).unwrap().eliminated && sim::winner(&s).is_none());
    assert_eq!(s.buildings[&(0, 0)].owner, None);
    assert!(sim::resign(&mut s, 2).is_err(), "only once");
}
