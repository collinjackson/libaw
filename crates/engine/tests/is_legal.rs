//! `is_legal` and `unit_actions` give the same answers as the full legal list, over positions
//! from random play on every test state.
use libaw::actions::{is_legal, legal_actions, unit_actions, Action, Then};
use libaw::sim::step;

fn rng(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}

#[test]
fn is_legal_agrees_with_the_full_legal_list() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/eval-harness/test-states");
    let mut checked = 0;
    for f in std::fs::read_dir(dir).unwrap().flatten() {
        let turn: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(f.path()).unwrap()).unwrap();
        let mut s = libaw::awbw::state_from(&turn);
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..400 {
            if libaw::sim::winner(&s).is_some() {
                break;
            }
            let all = legal_actions(&s);
            for a in &all {
                assert!(is_legal(&s, a), "{a:?}");
            }
            // Each unit's list is exactly its slice of the full list, in the same order.
            for id in s.units.keys() {
                let mine: Vec<&Action> = all.iter().filter(|a| matches!(a, Action::Move { unit, .. } if unit == id)).collect();
                assert_eq!(unit_actions(&s, *id).iter().collect::<Vec<_>>(), mine);
            }
            // Near misses are refused exactly when the full list lacks them.
            for a in &all {
                if let Action::Move { unit, path, then } = a {
                    let mut variants = vec![
                        Action::Move { unit: *unit, path: path.clone(), then: Then::Wait },
                        Action::Move { unit: *unit, path: path[..path.len().max(2) - 1].to_vec(), then: then.clone() },
                        Action::Move { unit: unit + 1, path: path.clone(), then: then.clone() },
                    ];
                    let mut longer = path.clone();
                    longer.push((path.last().unwrap().0 + 1, path.last().unwrap().1));
                    variants.push(Action::Move { unit: *unit, path: longer, then: then.clone() });
                    for v in variants {
                        assert_eq!(is_legal(&s, &v), all.contains(&v), "{v:?}");
                        checked += 1;
                    }
                }
            }
            let build_miss = Action::Build { at: (0, 0), kind: "Infantry".into() };
            assert_eq!(is_legal(&s, &build_miss), all.contains(&build_miss));
            let a = all[(rng(&mut x) % all.len() as u64) as usize].clone();
            step(&mut s, &a, &mut |lo, hi| lo + (rng(&mut x) % (hi - lo + 1) as u64) as i32).unwrap();
        }
    }
    assert!(checked > 1000, "only {checked} near misses checked");
}
