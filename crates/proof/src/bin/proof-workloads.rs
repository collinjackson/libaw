//! Writes the zkVM benchmark's three workloads and checks them natively:
//!   move: one action from the day-22 test position;
//!   turn: that player's whole turn;
//!   game: 15 days of play from there (both players).
//! The actions come from a fixed-seed random player that prefers acting to ending its turn,
//! so the files are the same on every run.
//!
//!   cargo run --release -p libaw-proof --bin proof-workloads -- OUT_DIR
use libaw::actions::{legal_actions, Action};
use libaw::sim::step;
use libaw_proof::{run, Input, Rolls};
use std::time::Instant;

const START: &str = "docs/eval-harness/test-states/game-1729758-turn-042-D22-pid4151012.json";
const SEED: u64 = 22;

fn main() {
    let out = std::env::args().nth(1).unwrap_or("bench/zkvm/workloads".into());
    std::fs::create_dir_all(&out).unwrap();
    let turn: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(START).unwrap()).unwrap();
    let start = libaw::awbw::state_from(&turn);
    let (day0, who0) = (start.day, start.current);

    let mut s = start.clone();
    let (mut rolls, mut pick) = (Rolls::new(SEED), Rolls::new(SEED ^ 0xa5a5));
    let mut actions = vec![];
    let (mut move_n, mut turn_n) = (None, None);
    let mut this_turn = 0;
    while libaw::sim::winner(&s).is_none() && s.day < day0 + 15 {
        let legal: Vec<Action> = legal_actions(&s).into_iter().filter(|a| *a != Action::EndTurn).collect();
        let a = if legal.is_empty() || this_turn >= 40 { Action::EndTurn } else { legal[pick.roll(0, legal.len() as i32 - 1) as usize].clone() };
        actions.push(libaw::awbw::action_to_contract(&a));
        let end = a == Action::EndTurn;
        step(&mut s, &a, &mut |lo, hi| rolls.roll(lo, hi)).unwrap();
        move_n.get_or_insert(actions.len());
        if end {
            this_turn = 0;
            if turn_n.is_none() && (s.current != who0 || s.day != day0) {
                turn_n = Some(actions.len());
            }
        } else {
            this_turn += 1;
        }
    }

    for (name, n) in [("move", move_n.unwrap()), ("turn", turn_n.unwrap()), ("game", actions.len())] {
        let input = Input { state: start.clone(), actions: actions[..n].to_vec(), seed: SEED };
        let json = serde_json::to_string(&input).unwrap();
        std::fs::write(format!("{out}/{name}.json"), &json).unwrap();
        let t = Instant::now();
        let o = run(serde_json::from_str(&json).unwrap()).unwrap();
        println!("{name}: {} actions, {} bytes, native {:.1} ms, {:?}", n, json.len(), t.elapsed().as_secs_f64() * 1e3, o);
    }
}
