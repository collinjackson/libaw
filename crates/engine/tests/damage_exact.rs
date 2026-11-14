//! `damage_exact` (integers, for a zkVM) must give AWBW's double-precision result for every
//! input. A quick sweep runs with the tests; `cargo test --release -- --ignored` runs the big one.
use libaw::rules::{damage_exact, damage_f64, AttackParams};

fn params(base: u32, att_value: u32, att_milli: u32, att_hp: u8, defence: u32) -> AttackParams {
    // defence = 200 - def_value with no terrain stars.
    AttackParams { base, att_value, att_milli, att_hp, def_value: 200 - defence, def_terrain: 0, def_hp: 10, luck_min: 0, luck_max: 0 }
}

/// (mismatches, inputs checked, inputs on an exact x.95 boundary)
fn sweep(bases: impl Iterator<Item = u32> + Clone, atts: &[u32], millis: &[u32], lucks: std::ops::RangeInclusive<i32>, defs: impl Iterator<Item = u32> + Clone) -> (u64, u64, u64) {
    let (mut bad, mut n, mut edge) = (0, 0, 0);
    for base in bases {
        for &a in atts {
            for &m in millis {
                for hp in 1..=10u8 {
                    for d in defs.clone() {
                        let p = params(base, a, m, hp, d);
                        for luck in lucks.clone() {
                            n += 1;
                            let (x, y) = (damage_exact(&p, luck), damage_f64(&p, luck));
                            if x != y {
                                bad += 1;
                                if bad <= 5 {
                                    eprintln!("base {base} att {a}.{m:03} hp {hp} def {d} luck {luck}: exact {x}, f64 {y}");
                                }
                            }
                            let q = hp as i128 * (base as i128 * (1000 * a as i128 + m as i128) + 100_000 * luck as i128) * d as i128 + 5_000_000;
                            edge += (q > 0 && q % 100_000_000 == 0) as u64;
                        }
                    }
                }
            }
        }
    }
    (bad, n, edge)
}

#[test]
fn integer_damage_matches_awbw_doubles() {
    // The damage chart's values, the attack values COs and towers give, every defence.
    let bases = [1u32, 4, 5, 6, 7, 9, 10, 12, 14, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 85, 95, 105, 115, 120, 125, 155, 165, 175, 180, 195];
    let atts: Vec<u32> = (70..=200).step_by(5).collect();
    let (bad, n, edge) = sweep(bases.iter().copied(), &atts, &[0], -10..=19, 40..=200);
    eprintln!("{n} inputs, {edge} on an x.95 boundary, {bad} mismatches");
    assert_eq!(bad, 0);
    // The game-1749614 case: 79.95 on paper, 79 dealt (the boundary takes the f64 path).
    assert!(edge > 0);
}

#[test]
#[ignore]
fn integer_damage_matches_awbw_doubles_everywhere() {
    let atts: Vec<u32> = (50..=250).collect();
    let (bad, n, edge) = sweep(1..=200, &atts, &[0], -10..=19, 40..=200);
    eprintln!("whole: {n} inputs, {edge} boundary, {bad} mismatches");
    let (bad2, n2, _) = sweep((1..=200).step_by(7), &[90, 100, 110, 130], &(0..1000).step_by(3).collect::<Vec<_>>(), 0..=9, (40..=200).step_by(5));
    eprintln!("Colin's fractions: {n2} inputs, {bad2} mismatches");
    assert_eq!(bad + bad2, 0);
}
