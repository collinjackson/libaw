//! Greedy one-ply baseline: try every legal action on a copy of the board with average
//! luck, score the result, take the best; end the turn when nothing beats ending it.
//! A test opponent, not the real bot (that lives in the private awbw-bot repo).
//! Also `random`: a uniformly random legal action, preferring to act over ending the turn.

#[allow(unused_imports)]
use crate::prelude::*;
use crate::actions::{legal_actions, Action, Then};
use crate::data::tables;
use crate::rules::dist;
use crate::sim;
use crate::state::*;

const PROPERTY: f64 = 1500.0; // a property's worth in funds-equivalent
const STEP: f64 = 40.0; // per tile closer to a unit's goal

/// How good `s` is for `me`: my worth minus the enemies'.
pub fn score(s: &State, me: PlayerId) -> f64 {
    let mut v = 0.0;
    for u in s.units.values() {
        let sign = if s.allied(u.owner, me) { 1.0 } else { -1.0 };
        let cost = tables().unit(u.kind).cost as f64;
        v += sign * cost * u.hp.hi as f64 / 100.0;
        if sign > 0.0 && u.carried_by.is_none() {
            v -= STEP * goal_distance(s, u) as f64;
        }
    }
    for (&pos, b) in &s.buildings {
        if tables().tile(b.terrain).building.is_some_and(|k| matches!(k, crate::data::BuildingKind::Missile | crate::data::BuildingKind::PipeSeam)) {
            continue;
        }
        match b.owner {
            Some(o) if s.allied(o, me) => v += PROPERTY,
            Some(_) => v -= PROPERTY,
            None => {}
        }
        // Capture in progress counts for whoever stands on it.
        if b.capture < 20 {
            if let Some(u) = s.unit_at(pos.0, pos.1) {
                let sign = if s.allied(u.owner, me) { 1.0 } else { -1.0 };
                v += sign * PROPERTY * (20 - b.capture) as f64 / 20.0;
            }
        }
    }
    for p in &s.players {
        let sign = if s.allied(p.id, me) { 1.0 } else { -1.0 };
        v += sign * p.funds as f64 * 0.9; // money in the bank is worth a bit less than an army
    }
    if let Some(w) = sim::winner(s) {
        v += if s.player(me).is_some_and(|p| p.team == w) { 1e9 } else { -1e9 };
    }
    v
}

/// Foot soldiers head for the nearest property they don't own; everything else for the
/// nearest enemy unit.
fn goal_distance(s: &State, u: &Unit) -> u32 {
    let here = (u.x, u.y);
    let foot = crate::rules::is_foot(u.kind);
    let d = if foot {
        s.buildings.iter()
            .filter(|(_, b)| !b.owner.is_some_and(|o| s.allied(o, u.owner))
                && !tables().tile(b.terrain).building.is_some_and(|k| matches!(k, crate::data::BuildingKind::Missile | crate::data::BuildingKind::PipeSeam)))
            .map(|(&p, _)| dist(here, p)).min()
    } else {
        None
    };
    d.or_else(|| s.units.values().filter(|o| !s.allied(o.owner, u.owner) && o.carried_by.is_none())
        .map(|o| dist(here, (o.x, o.y))).min())
        // Fog: no enemy unit in sight; head for the enemy's properties instead.
        .or_else(|| s.buildings.iter().filter(|(_, b)| b.owner.is_some_and(|o| !s.allied(o, u.owner)))
            .map(|(&p, _)| dist(here, p)).min())
        .unwrap_or(0)
}

pub fn choose(s: &State) -> Action {
    ranked(s).into_iter().next().map(|(_, a)| a).unwrap_or(Action::EndTurn)
}

/// Every action that gains something by the greedy score, best first (what `choose` picks
/// from, and what a game log shows as the candidates). Ending the turn isn't listed.
pub fn ranked(s: &State) -> Vec<(f64, Action)> {
    let me = s.current;
    let base = score(s, me);
    let mut out: Vec<(f64, Action)> = vec![];
    for a in legal_actions(s) {
        if a == Action::EndTurn {
            continue;
        }
        let mut t = s.clone();
        let mut avg = |lo: i32, hi: i32| (lo + hi) / 2;
        if sim::step(&mut t, &a, &mut avg).is_err() {
            continue;
        }
        let mut gain = score(&t, me) - base;
        if let Action::Build { kind, .. } = &a {
            gain += 500.0 + 0.1 * tables().unit(kind).cost as f64; // spend money
        }
        if let Action::Move { then: Then::Wait, path, .. } = &a {
            if path.len() == 1 {
                gain -= 1.0; // standing still is a tie-break loser
            }
        }
        if gain > 0.0 {
            out.push((gain, a));
        }
    }
    // Stable: equal gains keep generation order, as `choose` did (first best wins).
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(core::cmp::Ordering::Equal));
    out
}

/// A random legal action; `rng` gives uniform u32s. Ends the turn 1 time in 20 when it could
/// still act, so games progress.
pub fn random(s: &State, rng: &mut dyn FnMut() -> u32) -> Action {
    let legal = legal_actions(s);
    if legal.len() > 1 && rng() % 20 != 0 {
        legal[rng() as usize % (legal.len() - 1)].clone()
    } else {
        Action::EndTurn
    }
}
