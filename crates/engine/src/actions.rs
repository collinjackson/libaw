//! Legal actions: what the player to move may do now. Shapes follow the opponent API
//! contract (docs/eval-harness/CONTRACT.md) so bots and the harness share one vocabulary.
//!
//! v1 scope: no fog, no dived Subs or hidden Stealths (banned in v1 tournaments), no CO
//! power activation yet, no silo launches; seam attacks only when `State::seams`.

use crate::co;
use crate::data::tables;
use crate::rules::*;
use crate::state::*;
use std::collections::{BTreeMap, BinaryHeap, HashMap};

pub type Pos = (u16, u16);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Then {
    Wait,
    Attack { target: Pos },
    Capture,
    Load,
    Join,
    Supply,
    Repair { target: Pos },
    Unload { cargo: UnitId, to: Pos },
    /// Hit a pipe seam (only generated when `State::seams`).
    AttackSeam { target: Pos },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Move { unit: UnitId, path: Vec<Pos>, then: Then },
    Build { at: Pos, kind: String },
    /// Activate the CO power ("Y") or super power ("S"). Only generated when `State::powers`.
    Power { kind: String },
    EndTurn,
}

/// Every tile `u` can reach this turn, with the cheapest cost and a path to it.
/// Allies may be passed through; enemies block. Ends on occupied tiles are included
/// (they're only legal for Load/Join, which `move_options` decides).
pub fn reachable(s: &State, u: &Unit) -> BTreeMap<Pos, (u32, Vec<Pos>)> {
    let range = movement_range(s, u).min(u.fuel.max(0) as u32);
    let start = (u.x, u.y);
    let mut best: HashMap<Pos, u32> = HashMap::from([(start, 0)]);
    let mut prev: HashMap<Pos, Pos> = HashMap::new();
    let mut heap = BinaryHeap::from([std::cmp::Reverse((0u32, start))]);
    while let Some(std::cmp::Reverse((c, p))) = heap.pop() {
        if best.get(&p).is_some_and(|&b| b < c) {
            continue;
        }
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (p.0 as i32 + dx, p.1 as i32 + dy);
            if !s.in_bounds(nx, ny) {
                continue;
            }
            let n = (nx as u16, ny as u16);
            let Some(sc) = step_cost(s, u, s.terrain_at(n.0, n.1)) else { continue };
            let nc = c + sc;
            if nc > range {
                continue;
            }
            if let Some(o) = s.unit_at(n.0, n.1) {
                if !s.allied(o.owner, u.owner) {
                    continue;
                }
            }
            if best.get(&n).is_none_or(|&b| nc < b) {
                best.insert(n, nc);
                prev.insert(n, p);
                heap.push(std::cmp::Reverse((nc, n)));
            }
        }
    }
    best.into_iter()
        .map(|(p, c)| {
            let mut path = vec![p];
            let mut q = p;
            while let Some(&r) = prev.get(&q) {
                path.push(r);
                q = r;
            }
            path.reverse();
            (p, (c, path))
        })
        .collect()
}

/// Enemy tiles `u` can attack from `from`. `moved`: whether it moves first this action
/// (indirect units can't move and fire).
pub fn attack_targets(s: &State, u: &Unit, from: Pos, moved: bool) -> Vec<Pos> {
    let info = tables().unit(&u.kind);
    if info.range[1] == 0 {
        return vec![];
    }
    let indirect = is_indirect(&u.kind);
    if indirect && moved {
        return vec![];
    }
    let pw = s.player(u.owner).map(|p| p.power.clone()).unwrap_or_else(|| "N".into());
    let co = s.player(u.owner).map(|p| p.co.clone()).unwrap_or_default();
    let mut maxr = info.range[1] as i32;
    if indirect {
        maxr = (maxr + co::range_bonus(&co, &pw, &u.kind)).max(2);
    }
    let minr = info.range[0] as u32;
    s.units.values()
        .filter(|d| d.carried_by.is_none() && !s.allied(d.owner, u.owner) && !d.dived)
        .filter(|d| {
            let dd = dist(from, (d.x, d.y));
            dd >= minr && dd <= maxr as u32
        })
        // Out of ammo, a machine gun still hits ground vehicles (rules::mg_damage).
        .filter(|d| base_damage_now(u, &d.kind).is_some())
        .map(|d| (d.x, d.y))
        .collect()
}

/// Pipe seams `u` could hit from `from` (same range rule as units; a seam takes what a
/// Neotank would).
pub fn seam_targets(s: &State, u: &Unit, from: Pos, moved: bool) -> Vec<Pos> {
    let info = tables().unit(&u.kind);
    if info.range[1] == 0 || (is_indirect(&u.kind) && moved) || base_damage_now(u, "Neotank").is_none() {
        return vec![];
    }
    let mut maxr = info.range[1] as i32;
    if is_indirect(&u.kind) {
        let pw = s.player(u.owner).map(|p| p.power.clone()).unwrap_or_else(|| "N".into());
        let co = s.player(u.owner).map(|p| p.co.clone()).unwrap_or_default();
        maxr = (maxr + co::range_bonus(&co, &pw, &u.kind)).max(2);
    }
    s.buildings.iter()
        .filter(|(_, b)| tables().tile(b.terrain).building.as_deref() == Some("PipeSeam") && b.capture > 0)
        .map(|(&p, _)| p)
        .filter(|&p| { let d = dist(from, p); d >= info.range[0] as u32 && d <= maxr as u32 })
        .collect()
}

/// What `u` may do after moving to `to` (no move if `to` is its tile).
pub fn options_at(s: &State, u: &Unit, to: Pos) -> Vec<Then> {
    let mut out = vec![];
    let moved = to != (u.x, u.y);
    let other = s.unit_at(to.0, to.1).filter(|o| o.id != u.id);
    match other {
        Some(o) if o.owner == u.owner => {
            if capacity(&o.kind) > o.cargo.len() && can_carry(&o.kind, &u.kind) {
                out.push(Then::Load);
            }
            if o.kind == u.kind && o.hp.display_hi() < 10 && o.cargo.is_empty() && u.cargo.is_empty() {
                out.push(Then::Join);
            }
            return out;
        }
        Some(_) => return out,
        None => {}
    }
    out.push(Then::Wait);
    let mut as_there = u.clone();
    as_there.x = to.0;
    as_there.y = to.1;
    for t in attack_targets(s, &as_there, to, moved) {
        out.push(Then::Attack { target: t });
    }
    if s.seams {
        for t in seam_targets(s, &as_there, to, moved) {
            out.push(Then::AttackSeam { target: t });
        }
    }
    if is_foot(&u.kind) {
        if let Some(b) = s.buildings.get(&to) {
            let kind = tables().tile(b.terrain).building.as_deref();
            let mine = b.owner.is_some_and(|o| s.allied(o, u.owner));
            if !mine && !matches!(kind, Some("PipeSeam") | Some("Missile") | None) {
                out.push(Then::Capture);
            }
        }
    }
    let adj = |p: Pos| -> Vec<Pos> {
        [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)].iter()
            .map(|(dx, dy)| (p.0 as i32 + dx, p.1 as i32 + dy))
            .filter(|&(x, y)| s.in_bounds(x, y))
            .map(|(x, y)| (x as u16, y as u16))
            .collect()
    };
    if u.kind == "APC" && adj(to).iter().any(|&p| s.unit_at(p.0, p.1).is_some_and(|o| o.owner == u.owner && o.id != u.id)) {
        out.push(Then::Supply);
    }
    if u.kind == "Black Boat" {
        for p in adj(to) {
            if s.unit_at(p.0, p.1).is_some_and(|o| o.owner == u.owner && o.id != u.id) {
                out.push(Then::Repair { target: p });
            }
        }
    }
    for &c in &u.cargo {
        let Some(cu) = s.units.get(&c) else { continue };
        for p in adj(to) {
            let free = s.unit_at(p.0, p.1).is_none_or(|o| o.id == u.id);
            if free && step_cost(s, cu, s.terrain_at(p.0, p.1)).is_some() {
                out.push(Then::Unload { cargo: c, to: p });
            }
        }
    }
    out
}

/// Builds the player to move can afford, at their free production buildings.
pub fn builds(s: &State) -> Vec<(Pos, String)> {
    let me = s.current;
    let Some(p) = s.player(me) else { return vec![] };
    let mult = co::price_multiplier(&p.co, &p.power);
    let mut out = vec![];
    for (&pos, b) in &s.buildings {
        if b.owner != Some(me) || s.unit_at(pos.0, pos.1).is_some() {
            continue;
        }
        let classes: &[&str] = match tables().tile(b.terrain).building.as_deref() {
            Some("Base") => &["F", "B", "T", "W", "P"],
            Some("Airport") => &["A"],
            Some("Port") => &["S", "L"],
            // Hachi's Merchant Union: ground units from cities.
            Some("City") if p.co == "Hachi" && p.power == "S" => &["F", "B", "T", "W", "P"],
            _ => continue,
        };
        for (name, info) in &tables().units {
            if classes.contains(&info.move_type.as_str())
                && ((info.cost as f64 * mult).round() as i64) <= p.funds
            {
                out.push((pos, name.clone()));
            }
        }
    }
    out.sort();
    out
}

/// All legal actions for the player to move.
pub fn legal_actions(s: &State) -> Vec<Action> {
    let mut out = vec![];
    for u in s.units.values() {
        if u.owner != s.current || u.moved || u.carried_by.is_some() {
            continue;
        }
        for (to, (_, path)) in reachable(s, u) {
            for then in options_at(s, u, to) {
                out.push(Action::Move { unit: u.id, path: path.clone(), then });
            }
        }
    }
    for (at, kind) in builds(s) {
        out.push(Action::Build { at, kind });
    }
    for kind in crate::co::powers_ready(s, s.current) {
        out.push(Action::Power { kind: kind.into() });
    }
    out.push(Action::EndTurn);
    out
}
