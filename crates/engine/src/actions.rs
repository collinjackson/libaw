//! Legal actions: what the player to move may do now. Shapes follow the opponent API
//! contract (docs/eval-harness/CONTRACT.md) so bots and the harness share one vocabulary.
//!
//! Optional parts, off by default for contract v1 bots: seam attacks (`State::seams`),
//! diving and hiding (`State::dives`), CO powers (`State::powers`).

#[allow(unused_imports)]
use crate::prelude::*;
use crate::co;
use crate::data::{tables, BuildingKind as B, MoveType as M, UnitType as U};
use crate::rules::*;
use crate::state::*;
use alloc::collections::{BTreeMap, BinaryHeap};

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
    /// A Sub dives / a Stealth hides; Unhide surfaces or reveals it (only generated when
    /// `State::dives`).
    Hide,
    Unhide,
    /// A foot soldier on a loaded missile silo fires it: 3 HP to every unit within 2 tiles
    /// of `target` (never kills). Offered only at targets whose blast reaches an enemy.
    Launch { target: Pos },
    /// Scrap the unit (no refund). Only in place by an unmoved unit (all 47 recorded
    /// deletions); offered only when `State::deletes`.
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Move { unit: UnitId, path: Vec<Pos>, then: Then },
    Build { at: Pos, kind: crate::data::UnitType },
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
    let mut best: crate::fx::FxMap<Pos, u32> = crate::fx::FxMap::default();
    best.insert(start, 0);
    let mut prev: crate::fx::FxMap<Pos, Pos> = crate::fx::FxMap::default();
    let mut heap = BinaryHeap::from([core::cmp::Reverse((0u32, start))]);
    while let Some(core::cmp::Reverse((c, p))) = heap.pop() {
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
                heap.push(core::cmp::Reverse((nc, n)));
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
    let info = tables().unit(u.kind);
    if info.range[1] == 0 {
        return vec![];
    }
    let indirect = is_indirect(u.kind);
    if indirect && moved {
        return vec![];
    }
    let pw = s.player(u.owner).map(|p| p.power.clone()).unwrap_or_else(|| "N".into());
    let co = s.player(u.owner).map(|p| p.co.clone()).unwrap_or_default();
    let mut maxr = info.range[1] as i32;
    if indirect {
        maxr = (maxr + co::range_bonus(&co, &pw, u.kind)).max(2);
    }
    let minr = info.range[0] as u32;
    s.units.values()
        // A hidden Stealth can be hit by Fighters and Stealths, a dived Sub by Cruisers and
        // Subs (game-1692796: a Fighter on a hidden Stealth).
        .filter(|d| d.carried_by.is_none() && !s.allied(d.owner, u.owner) && (!d.dived || can_hit_hidden(u.kind, d.kind)))
        // ... and only once it is revealed, as the board stands before this unit moves.
        .filter(|d| !crate::fog::concealed(s, d, u.owner))
        .filter(|d| {
            let dd = dist(from, (d.x, d.y));
            dd >= minr && dd <= maxr as u32
        })
        // Out of ammo, a machine gun still hits ground vehicles (rules::mg_damage).
        .filter(|d| base_damage_now(u, d.kind).is_some())
        .map(|d| (d.x, d.y))
        .collect()
}

/// Pipe seams `u` could hit from `from` (same range rule as units; a seam takes what a
/// Neotank would).
pub fn seam_targets(s: &State, u: &Unit, from: Pos, moved: bool) -> Vec<Pos> {
    let info = tables().unit(u.kind);
    if info.range[1] == 0 || (is_indirect(u.kind) && moved) || base_damage_now(u, U::Neotank).is_none() {
        return vec![];
    }
    let mut maxr = info.range[1] as i32;
    if is_indirect(u.kind) {
        let pw = s.player(u.owner).map(|p| p.power.clone()).unwrap_or_else(|| "N".into());
        let co = s.player(u.owner).map(|p| p.co.clone()).unwrap_or_default();
        maxr = (maxr + co::range_bonus(&co, &pw, u.kind)).max(2);
    }
    s.buildings.iter()
        .filter(|(_, b)| tables().tile(b.terrain).building == Some(crate::data::BuildingKind::PipeSeam) && b.capture > 0)
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
            if capacity(o.kind) > o.cargo.len() && can_carry(o.kind, u.kind) {
                out.push(Then::Load);
            }
            // Transports may join while their cargo together still fits; it passes to the
            // unit joined.
            if o.kind == u.kind && o.hp.display_hi() < 10 && o.cargo.len() + u.cargo.len() <= capacity(o.kind) {
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
    if is_foot(u.kind) && s.buildings.get(&to).is_some_and(|b| tables().tile(b.terrain).building == Some(crate::data::BuildingKind::Missile)) {
        let mut targets = alloc::collections::BTreeSet::new();
        for e in s.units.values().filter(|e| !s.allied(e.owner, u.owner) && e.carried_by.is_none()) {
            for dy in -2i32..=2 {
                for dx in (-2 + dy.abs())..=(2 - dy.abs()) {
                    let (x, y) = (e.x as i32 + dx, e.y as i32 + dy);
                    if s.in_bounds(x, y) {
                        targets.insert((x as u16, y as u16));
                    }
                }
            }
        }
        // Plus one "dump" target whose blast hits nobody (the nearest such tile): emptying a
        // silo keeps the enemy off it (a recorded team-game launch hit empty ground).
        let empty_blast = |x: i32, y: i32| (-2i32..=2).all(|dy| ((-2 + dy.abs())..=(2 - dy.abs())).all(|dx| {
            !s.in_bounds(x + dx, y + dy) || s.unit_at((x + dx) as u16, (y + dy) as u16).is_none()
        }));
        // Search rings outward from the silo; the first fitting tile (row-major in its ring).
        let reach = s.width.max(s.height) as i32 * 2;
        let dump = (0..=reach).find_map(|d| {
            let ring = (-d..=d).flat_map(move |dy| { let r = d - dy.abs(); [(-r, dy), (r, dy)] });
            let mut c: Vec<(i32, i32)> = ring.map(|(dx, dy)| (to.0 as i32 + dx, to.1 as i32 + dy))
                .filter(|&(x, y)| s.in_bounds(x, y)).collect();
            c.sort_by_key(|&(x, y)| (y, x));
            c.into_iter().find(|&(x, y)| empty_blast(x, y))
        });
        targets.extend(dump.map(|(x, y)| (x as u16, y as u16)));
        out.extend(targets.into_iter().map(|t| Then::Launch { target: t }));
    }
    if s.deletes && to == (u.x, u.y) && !u.moved {
        out.push(Then::Delete);
    }
    if s.dives && matches!(u.kind, U::Sub | U::Stealth) {
        out.push(if u.dived { Then::Unhide } else { Then::Hide });
    }
    if is_foot(u.kind) {
        if let Some(b) = s.buildings.get(&to) {
            let kind = tables().tile(b.terrain).building;
            let mine = b.owner.is_some_and(|o| s.allied(o, u.owner));
            if !mine && !matches!(kind, Some(B::PipeSeam) | Some(B::Missile) | None) {
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
        if !can_unload_from(u.kind, s.terrain_at(to.0, to.1)) {
            break;
        }
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
pub fn builds(s: &State) -> Vec<(Pos, U)> {
    let me = s.current;
    let Some(p) = s.player(me) else { return vec![] };
    if at_unit_limit(s, me) {
        return vec![];
    }
    let mut out = vec![];
    for (&pos, b) in &s.buildings {
        if b.owner != Some(me) || s.unit_at(pos.0, pos.1).is_some() {
            continue;
        }
        let classes: &[M] = match tables().tile(b.terrain).building {
            Some(B::Base) => &[M::F, M::B, M::T, M::W, M::P],
            Some(B::Airport) => &[M::A],
            Some(B::Port) => &[M::S, M::L],
            // Hachi's Merchant Union: ground units from cities.
            Some(B::City) if p.co == "Hachi" && p.power == "S" => &[M::F, M::B, M::T, M::W, M::P],
            _ => continue,
        };
        for (name, info) in tables().units() {
            if classes.contains(&info.move_type)
                && !s.banned.contains(&name)
                && co::price(info.cost as i64, &p.co, &p.power) <= p.funds
            {
                out.push((pos, name));
            }
        }
    }
    out.sort();
    out
}

/// All legal actions for the player to move.
pub fn legal_actions(s: &State) -> Vec<Action> {
    let mut out = vec![];
    // Unmoved units first, then moved transports' in-place drops (bots that take the first
    // legal action rely on this order).
    for u in s.units.values().filter(|u| !u.moved) {
        unit_moves(s, u, &mut out);
    }
    for u in s.units.values().filter(|u| u.moved) {
        unit_moves(s, u, &mut out);
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

/// One unit's legal moves: the same as filtering `legal_actions` by unit, without working out
/// every other unit's.
pub fn unit_actions(s: &State, unit: UnitId) -> Vec<Action> {
    let mut out = vec![];
    if let Some(u) = s.units.get(&unit) {
        unit_moves(s, u, &mut out);
    }
    out
}

/// Whether `a` is legal: the same answer as `legal_actions(s).contains(a)`, at the cost of one
/// unit's moves (a referee checking a bot's action needs no more).
pub fn is_legal(s: &State, a: &Action) -> bool {
    match a {
        Action::Move { unit, .. } => unit_actions(s, *unit).contains(a),
        Action::Build { at, kind } => builds(s).iter().any(|(p, k)| p == at && k == kind),
        Action::Power { kind } => crate::co::powers_ready(s, s.current).iter().any(|k| *k == kind.as_str()),
        Action::EndTurn => true,
    }
}

fn unit_moves(s: &State, u: &Unit, out: &mut Vec<Action>) {
    if u.owner != s.current || u.carried_by.is_some() {
        return;
    }
    if !u.moved {
        for (to, (_, path)) in reachable(s, u) {
            for then in options_at(s, u, to) {
                out.push(Action::Move { unit: u.id, path: path.clone(), then });
            }
        }
    } else if !u.cargo.is_empty() {
        // A transport that has moved may still drop cargo where it stands (AWBW records ~400
        // such drops: a second cargo, cargo that boarded after it moved, or after other units
        // cleared the tile). An unmoved transport's in-place drop is offered above.
        for then in options_at(s, u, (u.x, u.y)) {
            if matches!(then, Then::Unload { .. }) {
                out.push(Action::Move { unit: u.id, path: vec![(u.x, u.y)], then });
            }
        }
    }
}
