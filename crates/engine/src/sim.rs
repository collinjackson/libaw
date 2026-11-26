//! Applying actions: the simulator. Turn start (income, fuel, repairs, supply) and unit
//! removal live here so the replay checker and bots share them.

#[allow(unused_imports)]
use crate::prelude::*;
use crate::co;
use crate::combat::*;
use crate::data::tables;
use crate::rules::*;
use crate::state::*;

pub fn remove_unit(s: &mut State, id: UnitId) {
    if let Some(u) = s.units.remove(&id) {
        // A capture in progress is lost with the unit.
        if u.carried_by.is_none() {
            if let Some(b) = s.buildings.get_mut(&(u.x, u.y)) {
                b.capture = CAPTURE_POINTS;
            }
        }
        // Cargo goes down with its transport, and cargo's cargo too (a loaded APC in a Lander:
        // its Infantry was left behind as a ghost, carried by nothing, in a local match).
        let mut cargo = u.cargo;
        while let Some(c) = cargo.pop() {
            if let Some(cu) = s.units.remove(&c) {
                cargo.extend(cu.cargo);
            }
        }
        if let Some(t) = u.carried_by.and_then(|t| s.units.get_mut(&t)) {
            t.cargo.retain(|&x| x != id);
        }
    }
}

/// Start of `next`'s turn: their power ends, then income, repairs/resupply on owned
/// properties, fuel burn.
pub fn start_turn(s: &mut State, next: PlayerId, day: u32) {
    s.current = next;
    s.day = day;
    // A power lasts until its owner's next turn, and so does weather it set (Olaf's snow,
    // Drake's rain), unless another player's active power has since set the same weather.
    // It gives way to clear weather: the records don't carry a game's own weather setting.
    let sets = |co: crate::data::Co, power: crate::data::Power| if power == crate::data::Power::None { None } else { co::instant(co, power).and_then(|e| e.weather) };
    let mine = s.player(next).and_then(|p| sets(p.co,p.power));
    if mine == Some(s.weather) && !s.players.iter().any(|p| p.id != next && sets(p.co,p.power) == Some(s.weather)) {
        s.weather = 'C';
    }
    if let Some(p) = s.player_mut(next) {
        p.power = "N".into();
    }
    for u in s.units.values_mut() {
        if u.stunned && u.owner == next {
            u.moved = true; // sits this turn out
            u.stunned = false;
        } else {
            u.moved = false;
        }
    }
    let props = s.buildings.values()
        .filter(|b| b.owner == Some(next) && is_income_property(b.terrain))
        .count() as i64;
    // Sasha's +100 per property comes on top of the game's funds setting, not instead of it.
    let fpp = if s.funds_per_property > 0 { s.funds_per_property + co::property_fund_increase(&co_of(s, next)) } else { 0 };
    if let Some(p) = s.player_mut(next) {
        p.funds += props * fpp;
    }
    // Fuel burn (air and sea units, per their daily rate; dived subs and hidden stealths more).
    let co = co_of(s, next);
    let ids: Vec<UnitId> = s.units.values().filter(|u| u.owner == next).map(|u| u.id).collect();
    let mut crashed = vec![];
    for id in &ids {
        let u = s.units.get_mut(id).unwrap();
        let info = tables().unit(u.kind);
        let mut burn = info.fuel_per_turn as i32;
        if u.dived {
            burn = if u.kind == "Sub" { 5 } else { 8 };
        }
        if info.move_type == "A" {
            burn = (burn - co::air_fuel_decrease(&co)).max(0);
        }
        // Out of fuel in the air or at sea, the unit is lost, even when it burned nothing
        // this turn (an Eagle copter that ended its move on 0).
        if u.carried_by.is_none() && day > 1 {
            u.fuel = (u.fuel - burn).max(0);
            if u.fuel == 0 && matches!(info.move_type, crate::data::MoveType::A | crate::data::MoveType::S | crate::data::MoveType::L) {
                crashed.push(*id);
            }
        }
    }
    // Repairs, in unit order (AWBW's order is an open question; affects who gets repaired
    // when funds run short).
    for id in &ids {
        let Some(u) = s.units.get(id).cloned() else { continue };
        if u.carried_by.is_some() {
            continue;
        }
        let Some(b) = s.buildings.get(&(u.x, u.y)) else { continue };
        if b.owner != Some(next) || !services(b.terrain, u.kind) {
            continue;
        }
        let info = tables().unit(u.kind);
        let funds = s.player(next).map(|p| p.funds).unwrap_or(0);
        // At the owner's price (game-1759750: Colin's repairs cost him 80%).
        let per_hp = owner_price(s, next, u.kind) / 10;
        // AWBW's repair, on the 0-100 scale: 80 or more (Rachel: 70 or more) goes to exactly
        // 100; anything lower gains exactly 20 (Rachel 30) and keeps its fraction (74 -> 94).
        // The price is per displayed point restored: nothing for a 9.x unit, 1 from display
        // 9, otherwise 2 (Rachel 3 from 7.0 down). All or nothing: when that isn't affordable
        // there is no repair, only the resupply.
        let rachel = co == "Rachel";
        let top = u.hp.hi;
        let points: u8 = if top > 90 { 0 } else if top > 80 { 1 } else if top > 70 || !rachel { 2 } else { 3 };
        let cost = per_hp * points as i64;
        let full_from = if rachel { 70 } else { 80 };
        // The gain is the rule's, not the price's: with an uncertain HP the price comes from the
        // top of the range, and the low end still gains its full 20 (Rachel 30).
        let gain = if rachel { 30 } else { 20 };
        let heal = |h: u8| if h >= full_from { 100 } else { (h + gain).min(100) };
        let repaired = u.hp.lo < 100 && cost <= funds;
        let m = s.units.get_mut(id).unwrap();
        if repaired {
            // An uncertain HP stays a range: each end is repaired the same way.
            m.hp = Hp { lo: heal(u.hp.lo.max(1)), hi: heal(u.hp.hi) };
        }
        m.fuel = info.fuel as i32;
        m.ammo = info.ammo as i32;
        // Its cargo is not resupplied there (53 recorded cases of carried units on ports
        // and airports keeping their fuel), unlike cargo of a unit an APC supplies.
        if repaired {
            if let Some(p) = s.player_mut(next) {
                p.funds -= cost;
            }
        }
    }
    // Carriers and cruisers resupply the aircraft they carry.
    let carried: Vec<(UnitId, UnitId)> = s.units.values()
        .filter(|u| u.owner == next)
        .filter_map(|u| u.carried_by.map(|t| (u.id, t)))
        .collect();
    for (id, t) in carried {
        if matches!(s.units.get(&t).map(|t| t.kind), Some(crate::data::UnitType::Carrier | crate::data::UnitType::Cruiser)) {
            let u = s.units.get_mut(&id).unwrap();
            let info = tables().unit(u.kind);
            u.fuel = info.fuel as i32;
            u.ammo = info.ammo as i32;
        }
    }
    // Only units still dry after resupply go down: a boat that burns its last fuel on its
    // own port is refuelled there, not sunk (7 Black Boats in league games).
    // An own APC next door refuels it too (apc_supply, right after this) before it falls:
    // a Bomber on 2 fuel beside an APC lived on at 99 (game-1754384 turn-055).
    let apc_next_door = |s: &State, u: &Unit| s.units.values().any(|a| a.kind == "APC" && a.owner == u.owner
        && a.carried_by.is_none() && dist((a.x, a.y), (u.x, u.y)) == 1);
    for id in crashed {
        if s.units.get(&id).is_some_and(|u| u.fuel == 0 && !apc_next_door(s, u)) {
            remove_unit(s, id);
        }
    }
}

/// End of turn: APCs resupply adjacent own units. Returns the units that needed it.
pub fn apc_supply(s: &mut State, p: PlayerId) -> alloc::collections::BTreeSet<UnitId> {
    let apcs: Vec<(UnitId, u16, u16)> = s.units.values()
        .filter(|u| u.owner == p && u.kind == "APC" && u.carried_by.is_none())
        .map(|u| (u.id, u.x, u.y)).collect();
    let mut out = alloc::collections::BTreeSet::new();
    for (aid, x, y) in apcs {
        for u in s.units.values_mut() {
            if u.id != aid && u.owner == p && u.carried_by.is_none() && dist((x, y), (u.x, u.y)) == 1 {
                let info = tables().unit(u.kind);
                // AWBW lists every adjacent unit, needed or not, except a hidden one: a hidden
                // Stealth is refuelled but not listed (game-1692796).
                if !u.dived {
                    out.insert(u.id);
                }

                u.fuel = info.fuel as i32;
                u.ammo = info.ammo as i32;
            }
        }
    }
    // A supplied transport's cargo is supplied too, and listed (a Black Boat beside an APC,
    // game-1752274; an APC beside an APC, game-1578803).
    let cargo: Vec<UnitId> = s.units.values()
        .filter(|u| u.carried_by.is_some_and(|t| out.contains(&t)))
        .map(|u| u.id).collect();
    for id in cargo {
        let u = s.units.get_mut(&id).unwrap();
        let info = tables().unit(u.kind);
        u.fuel = info.fuel as i32;
        u.ammo = info.ammo as i32;
        out.insert(id);
    }
    out
}

use crate::actions::{Action, Pos, Then};

/// Luck source: given the attacker's luck range (inclusive), return the roll. Bots pass a
/// seeded RNG; the replay checker passes rolls that reproduce a recorded result.
pub type Luck<'a> = &'a mut dyn FnMut(i32, i32) -> i32;

/// Exact HP for simulation: the state carries a range; take its top.
fn hp_now(u: &Unit) -> u8 {
    u.hp.hi
}

/// Damage to a pipe seam (99 HP, kept in its building's capture field): what the attacker
/// would do to a Neotank with neutral defence, no terrain and no luck, by the same formula
/// and rounding as any other attack. Fits all 2241 corpus hits. None: it can't.
pub fn seam_damage(s: &State, att: &Unit) -> Option<i32> {
    let base = base_damage_now(att, crate::data::UnitType::Neotank)? as u32;
    let p = AttackParams {
        base,
        att_value: att_value(s, att, s.terrain_at(att.x, att.y), false),
        att_milli: att_milli(s, att),
        att_hp: att.hp.display_hi().max(1),
        def_value: 100,
        def_terrain: 0,
        def_hp: 0,
        luck_min: 0,
        luck_max: 0,
    };
    Some(damage_range(&p).0 as i32)
}

/// Damage `att` does to `def` with a given luck roll (internal HP points).
pub fn damage(s: &State, att: &Unit, def: &Unit, roll: i32, counter: bool) -> u8 {
    let Some(base) = base_damage_now(att, def.kind) else { return 0 };
    let at = s.terrain_at(att.x, att.y);
    let dt = s.terrain_at(def.x, def.y);
    let p = AttackParams {
        base: base as u32,
        att_value: att_value(s, att, at, counter),
        att_milli: att_milli(s, att),
        att_hp: display(hp_now(att)),
        def_value: def_value(s, def, att.kind),
        def_terrain: stars(s, def, dt),
        def_hp: display(hp_now(def)),
        luck_min: roll,
        luck_max: roll,
    };
    damage_range(&p).0.min(255) as u8
}

fn set_hp(s: &mut State, id: UnitId, hp: u8) {
    if let Some(u) = s.units.get_mut(&id) {
        u.hp = Hp::exact(hp);
    }
    if hp == 0 {
        remove_unit(s, id);
    }
}

/// What a `kind` costs its owner `pid` now (CO price: Colin 80%, Kanbei 120%, ...).
pub fn owner_price(s: &State, pid: PlayerId, kind: crate::data::UnitType) -> i64 {
    let cost = tables().unit(kind).cost as i64;
    s.player(pid).map(|p| co::price(cost, &p.co, &p.power)).unwrap_or(cost)
}

/// One attack with its counterattack. Ammo is spent by whichever side uses its primary.
/// Charge CO power meters after a fight. AWBW: the side that loses HP gains the funds value
/// of the displayed HP it lost; the other side gains half of that. Meters stored x10, capped
/// at the super power's cost; no charge while a power is on.
pub fn charge(s: &mut State, att: &Unit, att_lost: u8, def: &Unit, def_lost: u8) {
    // At the owner's price (game-1759750: a Colin Infantry, 800 to him, charges 80%).
    let value = |u: &Unit, lost: u8| owner_price(s, u.owner, u.kind) * lost as i64; // x10 /10
    let (a_val, d_val) = (value(att, att_lost), value(def, def_lost));
    for (owner, gain) in [(att.owner, a_val + d_val / 2), (def.owner, d_val + a_val / 2)] {
        if let Some(p) = s.player_mut(owner) {
            if p.power == "N" {
                p.meter = (p.meter + gain).min(if p.scop_cost > 0 { p.scop_cost } else { i64::MAX });
            }
        }
    }
}

/// Activate a power: "Y" (CO power) or "S" (super). Spends its cost from the meter.
/// (Cost growth after each use and per-CO effects aren't modeled here yet.)
pub fn spend_power(s: &mut State, pid: PlayerId, kind: impl crate::data::AsPower) {
    let kind = kind.as_power();
    if let Some(p) = s.player_mut(pid) {
        let cost = if kind == crate::data::Power::Scop { p.scop_cost } else { p.cop_cost };
        p.meter = (p.meter - cost).max(0);
        p.power = kind;
        // Each use raises both costs by a fifth of the base (seen in the replays), up to the
        // tenth use: they stop at three times the base.
        if let Some((c, sc)) = co::power_base(&p.co) {
            if p.cop_cost < 3 * c {
                p.cop_cost += c / 5;
                p.scop_cost += sc / 5;
            }
        }
    }
}

/// Apply a power's instant effects (see `co::instant`). Returns false when the engine
/// doesn't model this CO's power yet (it was still activated; only effects are missing).
pub fn power_effects(s: &mut State, pid: PlayerId, kind: impl crate::data::AsPower) -> bool {
    power_effects_at(s, pid, kind, None)
}

/// Missile damage within 2 tiles of `at`: `hp` displayed HP off every unit there (either side), never
/// killing (a unit pushed to the floor shows 1 HP).
fn missile(s: &mut State, at: (u16, u16), stun: bool, hp: i32) {
    let hit: Vec<UnitId> = s.units.values()
        .filter(|u| u.carried_by.is_none()
            && (u.x as i32 - at.0 as i32).abs() + (u.y as i32 - at.1 as i32).abs() <= 2)
        .map(|u| u.id).collect();
    for id in hit {
        let u = s.units.get_mut(&id).unwrap();
        // AWBW leaves a unit with no less than exactly 0.1 HP.
        let hit = |h: u8| (h as i32 - hp * 10).max(1) as u8;
        u.hp = crate::state::Hp { lo: hit(u.hp.lo), hi: hit(u.hp.hi) };
        if stun {
            u.stunned = true;
            u.moved = true;
        }
    }
}

/// What AWBW's missile aim adds up over a blast (see `aim_missile`).
#[derive(Clone, Copy, PartialEq)]
enum Aim {
    /// Rachel's first missile: HP taken, foot soldiers counting four times (eight while
    /// capturing).
    Foot,
    /// Rachel's second and Von Bolt's: value destroyed.
    Value,
    /// Rachel's third: HP taken.
    Hp,
    /// Sturm's meteor: the value of the units in the blast, centred on an enemy unit.
    Sturm,
}

/// Where AWBW aims a power's missile. It scores every tile of the map, rows top to bottom,
/// by what the 2-tile blast would do: enemy units count for, the caster's side against. A
/// unit showing h HP loses 3, or h - 1 from 3 or less, or 0.1 at 1; value is its owner's
/// price. The strictly highest score wins (by value and HP aims, ties go to the blast with
/// more unit value in it); nothing is fired if no tile scores above zero.
/// Reproduces all 33 recorded Rachel missiles and all 16 of Von Bolt's.
fn aim_missile(s: &State, pid: PlayerId, aim: Aim) -> Option<(u16, u16)> {
    // Per tile holding an uncarried unit: (score, tie-break value, is an enemy), in tenths.
    let mut tiles: alloc::collections::BTreeMap<(i32, i32), (i64, i64, bool)> = Default::default();
    for u in s.units.values().filter(|u| u.carried_by.is_none()) {
        let enemy = !s.allied(u.owner, pid);
        let sign = if enemy { 1 } else { -1 };
        let h = u.hp.display_hi() as i64;
        let loss = if h <= 1 { 1 } else if h <= 3 { (h - 1) * 10 } else { 30 };
        let price = owner_price(s, u.owner, u.kind);
        let (score, value) = match aim {
            Aim::Value => (price * loss, price * h),
            Aim::Sturm => (price * h, price * h),
            Aim::Hp => (loss, h),
            Aim::Foot => {
                let capturing = enemy && s.buildings.get(&(u.x, u.y)).is_some_and(|b| b.capture < CAPTURE_POINTS);
                let weight = if h <= 1 { 1 } else { (if is_foot(u.kind) { 4 } else { 1 }) * (if capturing { 2 } else { 1 }) };
                (weight * loss, h)
            }
        };
        tiles.insert((u.x as i32, u.y as i32), (sign * score, sign * value, enemy));
    }
    let mut best: Option<((u16, u16), i64, i64)> = None;
    for y in 0..s.height as i32 {
        for x in 0..s.width as i32 {
            if aim == Aim::Sturm && !tiles.get(&(x, y)).is_some_and(|t| t.2) {
                continue;
            }
            let (mut score, mut value) = (0i64, 0i64);
            for dy in -2i32..=2 {
                let r = 2 - dy.abs();
                for dx in -r..=r {
                    if let Some(t) = tiles.get(&(x + dx, y + dy)) {
                        score += t.0;
                        value += t.1;
                    }
                }
            }
            let (top, top_value) = best.map_or((0, 0), |b| (b.1, b.2));
            let tie_broken = score == top && matches!(aim, Aim::Value | Aim::Hp) && value > top_value;
            if score > 0 && (score > top || tie_broken) {
                best = Some(((x as u16, y as u16), score, value));
            }
        }
    }
    best.map(|b| b.0)
}

/// The targets of a power's missiles when the engine aims them itself, all chosen on the
/// board as it stands before any lands (Rachel's three can stack on one tile).
fn aim_missiles(s: &State, pid: PlayerId, count: usize) -> Vec<(u16, u16)> {
    let aims: &[Aim] = match co_of(s, pid) {
        crate::data::Co::Rachel => &[Aim::Foot, Aim::Value, Aim::Hp],
        crate::data::Co::Sturm => &[Aim::Sturm],
        _ => &[Aim::Value],
    };
    aims.iter().take(count).filter_map(|&a| aim_missile(s, pid, a)).collect()
}

/// Where this power's missiles would land on the board as it stands (AWBW's aim).
pub fn missile_targets(s: &State, pid: PlayerId, kind: impl crate::data::AsPower,) -> Vec<(u16, u16)> {
    co::instant(co_of(s, pid), kind).map_or(vec![], |e| aim_missiles(s, pid, e.missiles as usize))
}

/// The player to move activates a power. Missiles land on `targets` when given (the server
/// aims them in real games; replays record where), else on the engine's own aim.
pub fn use_power(s: &mut State, kind: impl crate::data::AsPower, targets: Option<&[(u16, u16)]>) -> Result<(), String> {
    let kind = kind.as_power();
    if !co::powers_ready(s, s.current).contains(&kind) {
        return Err(format!("power {kind} not ready"));
    }
    let me = s.current;
    spend_power(s, me, kind);
    power_effects_at(s, me, kind, targets);
    Ok(())
}

/// As `power_effects`, with missile targets given (the replay checker passes AWBW's).
pub fn power_effects_at(s: &mut State, pid: PlayerId, kind: impl crate::data::AsPower, targets: Option<&[(u16, u16)]>) -> bool {
    let kind = kind.as_power();
    let Some(e) = co::instant(co_of(s, pid), kind) else { return false };
    let aimed = match targets {
        Some(t) => t.iter().copied().take(e.missiles as usize).collect(),
        None => aim_missiles(s, pid, e.missiles as usize),
    };
    for at in aimed {
        missile(s, at, e.stun, e.missile_hp);
    }
    if let Some(p) = s.player_mut(pid) {
        // Colin's funds bonus is worked out at each attack (combat::att_milli), not here.
        p.power_attack = 0;
    }
    if let Some(kind) = e.spawn {
        let info = crate::data::tables().unit(kind);
        let mut cities: Vec<(u16, u16)> = s.buildings.iter()
            .filter(|(&(x, y), b)| b.owner == Some(pid) && s.unit_at(x, y).is_none()
                && crate::data::tables().tile(b.terrain).building == Some(crate::data::BuildingKind::City))
            .map(|(&p, _)| p).collect();
        // AWBW walks the cities row by row and stops at the unit limit.
        cities.sort_by_key(|&(x, y)| (y, x));
        for (x, y) in cities {
            if at_unit_limit(s, pid) {
                break;
            }
            let id = s.units.keys().max().copied().unwrap_or(0) + 1;
            s.units.insert(id, Unit {
                id, owner: pid, kind, x, y, hp: Hp::exact(90),
                fuel: info.fuel as i32, ammo: info.ammo as i32, moved: false, stunned: false,
                carried_by: None, cargo: vec![], dived: false,
            });
        }
    }
    if e.funds_pct != 100 {
        if let Some(p) = s.player_mut(pid) { p.funds = p.funds * e.funds_pct / 100; }
    }
    let mine: Vec<UnitId> = s.units.values().filter(|u| s.allied(u.owner, pid)).map(|u| u.id).collect();
    let theirs: Vec<UnitId> = s.units.values().filter(|u| !s.allied(u.owner, pid) && u.carried_by.is_none()).map(|u| u.id).collect();
    let bump = |h: u8, n: i32| (h as i32 + n).clamp(1, 100) as u8;
    // A heal reaches the caster's own units, not a teammate's. Andy's reaches units inside
    // transports; Hawke's doesn't.
    let hawke = co_of(s, pid) == "Hawke";
    for id in mine {
        let u = s.units.get_mut(&id).unwrap();
        if e.gain != 0 && u.owner == pid && !(hawke && u.carried_by.is_some()) {
            u.hp = crate::state::Hp { lo: bump(u.hp.lo, e.gain * 10), hi: bump(u.hp.hi, e.gain * 10) };
        }
    }
    for id in theirs {
        let u = s.units.get_mut(&id).unwrap();
        if e.loss != 0 {
            u.hp = crate::state::Hp { lo: bump(u.hp.lo, -e.loss * 10), hi: bump(u.hp.hi, -e.loss * 10) };
            if e.loss_fuel_pct != 100 {
                u.fuel = (u.fuel * e.loss_fuel_pct + 99) / 100; // ceil, fuel >= 0
            }
        }
    }
    if e.moves != 0 {
        let own: Vec<UnitId> = s.units.values().filter(|u| u.owner == pid && co::moves_apply(u.kind, e.moves_for)
            // Recorded games: class-specific bonuses (Sami, Jess, Max, Jake) skip carried
            // units; whole-army ones (Koal, Adder, Andy) include them.
            && (e.moves_for == "all" || u.carried_by.is_none())).map(|u| u.id).collect();
        for id in own {
            let m = crate::rules::movement_range(s, &s.units[&id]) as i32 + e.moves;
            s.move_override.insert(id, m.max(0) as u32);
        }
    }
    if e.urban_loss != 0 {
        let hit: Vec<UnitId> = s.units.values()
            .filter(|u| !s.allied(u.owner, pid) && u.carried_by.is_none() && s.buildings.get(&(u.x, u.y)).is_some_and(|b| {
                // Properties only: not silos, pipe seams or their rubble.
                crate::data::tables().tile(b.terrain).building.is_some_and(|k| !matches!(k, crate::data::BuildingKind::Missile | crate::data::BuildingKind::PipeSeam))
            }))
            .map(|u| u.id).collect();
        for id in hit {
            let u = s.units.get_mut(&id).unwrap();
            // Can't kill: a unit pushed to the floor shows 1 HP, internal 1-10 as far as we know.
            u.hp = crate::state::Hp { lo: bump(u.hp.lo, -e.urban_loss * 10), hi: bump(u.hp.hi, -e.urban_loss * 10) };
        }
    }
    if e.drain {
        let funds = s.player(pid).map(|p| p.funds).unwrap_or(0);
        let others: Vec<PlayerId> = s.players.iter().filter(|p| !s.allied(p.id, pid)).map(|p| p.id).collect();
        for id in others {
            let p = s.player_mut(id).unwrap();
            p.meter = (p.meter - p.scop_cost * funds / 50000).max(0);
        }
    }
    if e.resupply || e.act_again {
        let own: Vec<UnitId> = s.units.values().filter(|u| u.owner == pid).map(|u| u.id).collect();
        for id in own {
            let foot = crate::rules::is_foot(s.units[&id].kind);
            let info = crate::data::tables().unit(&s.units[&id].kind);
            let u = s.units.get_mut(&id).unwrap();
            if e.resupply {
                u.fuel = info.fuel as i32;
                u.ammo = info.ammo as i32;
            }
            if e.act_again && !foot {
                u.moved = false;
            }
        }
    }
    if let Some(w) = e.weather {
        s.weather = w;
    }
    true
}

/// Sasha's War Bonds: funds worth half the damage she deals, at the victim's price to its
/// owner (Kanbei 120%, Colin 80%, Hachi 90% or 50%).
fn war_bonds(s: &mut State, sasha: PlayerId, victim: &Unit, lost: u8) {
    if co_of(s, sasha) == "Sasha" && pw(s, sasha) == "S" {
        let bonus = owner_price(s, victim.owner, victim.kind) * lost as i64 / 20;
        if let Some(p) = s.player_mut(sasha) {
            p.funds += bonus;
        }
    }
}

pub fn attack(s: &mut State, aid: UnitId, target: Pos, luck: Luck) -> Result<(), String> {
    let att = s.units.get(&aid).cloned().ok_or("attacker gone")?;
    let def = s.unit_at(target.0, target.1).cloned().ok_or("no target")?;
    // Sonja's Counter Break: while her super lasts (through the opponent's turn), a defender
    // that can counter strikes first, without her x1.5 counter bonus; the attacker then fires
    // with what's left, as if countering, and there's no second counter.
    {
        let counter_break = co_of(s, def.owner) == "Sonja" && pw(s, def.owner) == "S"
            && dist((att.x, att.y), (def.x, def.y)) == 1 && !is_indirect(def.kind);
        // A defender that could hit the attacker by the chart but can't fire (out of ammo,
        // no second weapon) never strikes, and AWBW then drops the attack altogether.
        if counter_break && tables().base_damage(def.kind, att.kind).is_some() && base_damage_now(&def, att.kind).is_none() {
            return Ok(());
        }
        let first = counter_break && base_damage_now(&def, att.kind).is_some();
        if first {
            let (lo, hi) = co::luck(&co_of(s, def.owner), &pw(s, def.owner));
            let dmg = damage(s, &def, &att, luck(lo, hi), false);
            if uses_primary(&def, att.kind) {
                s.units.get_mut(&def.id).unwrap().ammo -= 1;
            }
            let after = hp_now(&att).saturating_sub(dmg);
            set_hp(s, aid, after);
            let att_lost = display(hp_now(&att)) - display(after);
            if after == 0 {
                charge(s, &att, att_lost, &def, 0);
                return Ok(());
            }
            let att = s.units.get(&aid).cloned().unwrap();
            let def = s.units.get(&def.id).cloned().unwrap();
            let (lo, hi) = co::luck(&co_of(s, att.owner), &pw(s, att.owner));
            // AWBW resolves the attacker's strike here as a counterattack, so an attacking
            // Sonja, or Kanbei under Samurai Spirit, gets the x1.5 counter bonus on it.
            let dmg = damage(s, &att, &def, luck(lo, hi), true);
            if uses_primary(&att, def.kind) {
                s.units.get_mut(&aid).unwrap().ammo -= 1;
            }
            let left = hp_now(&def).saturating_sub(dmg);
            set_hp(s, def.id, left);
            let def_lost = display(hp_now(&def)) - display(left);
            war_bonds(s, att.owner, &def, def_lost);
            charge(s, &att, att_lost, &def, def_lost);
            return Ok(());
        }
    }
    let (lo, hi) = co::luck(&co_of(s, att.owner), &pw(s, att.owner));
    let dmg = damage(s, &att, &def, luck(lo, hi), false);
    if uses_primary(&att, def.kind) {
        s.units.get_mut(&aid).unwrap().ammo -= 1;
    }
    let left = hp_now(&def).saturating_sub(dmg);
    set_hp(s, def.id, left);
    let def_lost = display(hp_now(&def)) - display(left);
    // Sasha's War Bonds (her counters pay too, below).
    war_bonds(s, att.owner, &def, def_lost);
    if left == 0 {
        charge(s, &att, 0, &def, def_lost);
        return Ok(());
    }
    let def = s.units.get(&def.id).cloned().unwrap();
    let att = s.units.get(&aid).cloned().unwrap();
    let can_counter = dist((att.x, att.y), (def.x, def.y)) == 1 && !is_indirect(def.kind)
        && base_damage_now(&def, att.kind).is_some();
    if can_counter {
        // A dived Sub or hidden Stealth takes nothing from a counter by a unit that can't
        // hit it; the defender still fires (and spends its ammo).
        let dmg = if att.dived && !can_hit_hidden(def.kind, att.kind) {
            0
        } else {
            let (lo, hi) = co::luck(&co_of(s, def.owner), &pw(s, def.owner));
            damage(s, &def, &att, luck(lo, hi), true)
        };
        if uses_primary(&def, att.kind) {
            s.units.get_mut(&def.id).unwrap().ammo -= 1;
        }
        let after = hp_now(&att).saturating_sub(dmg);
        set_hp(s, aid, after);
        let att_lost = display(hp_now(&att)) - display(after);
        // War Bonds pays on her counters too (game-1752437, during the opponent's turn).
        war_bonds(s, def.owner, &att, att_lost);
        charge(s, &att, att_lost, &def, def_lost);
        return Ok(());
    }
    charge(s, &att, 0, &def, def_lost);
    Ok(())
}

fn eliminate(s: &mut State, gone: PlayerId, by: PlayerId) {
    for b in s.buildings.values_mut().filter(|b| b.owner == Some(gone)) {
        b.owner = Some(by);
    }
    let ids: Vec<UnitId> = s.units.values().filter(|u| u.owner == gone).map(|u| u.id).collect();
    for id in ids {
        remove_unit(s, id);
    }
    if let Some(p) = s.player_mut(gone) {
        p.eliminated = true;
    }
}

/// Apply one action for the player to move. The action should come from
/// `actions::legal_actions`; this checks only what it needs to stay consistent.
pub fn step(s: &mut State, a: &Action, luck: Luck) -> Result<(), String> {
    let fielded: Vec<PlayerId> = s.players.iter().map(|p| p.id).filter(|&p| s.units.values().any(|u| u.owner == p)).collect();
    apply(s, a, luck)?;
    // AWBW checks for a rout whenever a unit is lost (in a fight, by deletion, out of fuel).
    for p in fielded {
        rout(s, p);
    }
    // A player put out during their own turn doesn't finish it.
    if winner(s).is_none() && s.player(s.current).is_some_and(|p| p.eliminated) {
        end_turn(s)?;
    }
    Ok(())
}

/// A player whose last unit has just been destroyed is out, on any day. (One who simply has
/// no units, having built none, is not.) If the game goes on, their properties turn neutral,
/// the HQ into a neutral city.
fn rout(s: &mut State, pid: PlayerId) {
    // A view with units missing proves nothing.
    if s.partial || s.units.values().any(|u| u.owner == pid) || s.player(pid).is_none_or(|p| p.eliminated) {
        return;
    }
    s.player_mut(pid).unwrap().eliminated = true;
    if winner(s).is_some() {
        return;
    }
    let w = s.width as usize;
    let theirs: Vec<(u16, u16)> = s.buildings.iter().filter(|(_, b)| b.owner == Some(pid)).map(|(&k, _)| k).collect();
    for k in theirs {
        let b = s.buildings.get_mut(&k).unwrap();
        (b.owner, b.capture) = (None, CAPTURE_POINTS);
        b.terrain = crate::awbw::owned_tile(b.terrain, None);
        s.terrain[k.1 as usize * w + k.0 as usize] = b.terrain;
    }
}

fn apply(s: &mut State, a: &Action, luck: Luck) -> Result<(), String> {
    match a {
        Action::EndTurn => end_turn(s),
        Action::Power { kind } => use_power(s, * kind, None),
        Action::Build { at, kind } => {
            let me = s.current;
            let p = s.player(me).ok_or("no player")?;
            let price = co::price(tables().unit(kind).cost as i64, &p.co, &p.power);
            if price > p.funds || s.unit_at(at.0, at.1).is_some() {
                return Err(format!("can't build {kind} at {at:?}"));
            }
            s.player_mut(me).unwrap().funds -= price;
            let id = s.units.keys().max().copied().unwrap_or(0) + 1;
            let info = tables().unit(kind);
            s.units.insert(id, Unit {
                id, owner: me, kind: kind.clone(), x: at.0, y: at.1, hp: Hp::exact(100),
                fuel: info.fuel as i32, ammo: info.ammo as i32, moved: true, stunned: false,
                carried_by: None, cargo: vec![], dived: false,
            });
            Ok(())
        }
        Action::Move { unit, path, then } => {
            let u = s.units.get(unit).cloned().ok_or("no such unit")?;
            // Dropping cargo where the transport stands doesn't spend its move, and a moved
            // transport may still do it (AWBW records: 87 transports drop, then move).
            let in_place_drop = path.len() <= 1 && matches!(then, Then::Unload { .. });
            if u.owner != s.current || (u.moved && !in_place_drop) {
                return Err("unit can't act".into());
            }
            // Fog: a path planned from the player's view may run into a hidden enemy unit;
            // the move stops on the last free tile before it and the unit waits (a trap).
            let (trapped_path, trapped_then);
            let (path, then) = match crate::fog::trap(s, &u, path) {
                Some(k) => {
                    trapped_path = path[..k].to_vec();
                    trapped_then = Then::Wait;
                    (&trapped_path, &trapped_then)
                }
                None => (path, then),
            };
            // A dived Sub or hidden Stealth can be attacked only if it was already revealed
            // before this unit moved.
            if let Then::Attack { target } = then {
                if s.unit_at(target.0, target.1).is_some_and(|t| crate::fog::concealed(s, t, u.owner)) {
                    return Err("target is hidden".into());
                }
            }
            let cost = path_cost(s, &u, path)?;
            let end = *path.last().unwrap_or(&(u.x, u.y));
            {
                let m = s.units.get_mut(unit).unwrap();
                m.fuel -= cost as i32;
                m.moved = true;
            }
            if end != (u.x, u.y) {
                if let Some(b) = s.buildings.get_mut(&(u.x, u.y)) {
                    b.capture = CAPTURE_POINTS; // left mid-capture
                }
                if let Some(b) = s.buildings.get_mut(&end) {
                    if matches!(b.terrain, 115 | 116) {
                        b.capture = CAPTURE_POINTS;
                    }
                }
            }
            match then {
                Then::Load => {
                    let t = s.unit_at(end.0, end.1).filter(|o| o.id != *unit).map(|o| o.id).ok_or("no transport")?;
                    s.units.get_mut(&t).unwrap().cargo.push(*unit);
                    let m = s.units.get_mut(unit).unwrap();
                    m.x = end.0;
                    m.y = end.1;
                    m.carried_by = Some(t);
                    // A transport loading into another brings its own cargo to the tile (one
                    // level, as AWBW does; game-1752274: Infantry in an APC loaded into a Lander).
                    for c in s.units[unit].cargo.clone() {
                        if let Some(cu) = s.units.get_mut(&c) {
                            cu.x = end.0;
                            cu.y = end.1;
                        }
                    }
                    return Ok(());
                }
                Then::Join => {
                    let t = s.unit_at(end.0, end.1).filter(|o| o.id != *unit).cloned().ok_or("no join target")?;
                    let me = s.units.remove(unit).unwrap();
                    // Cargo passes to the unit joined.
                    for c in &me.cargo {
                        if let Some(cu) = s.units.get_mut(c) {
                            (cu.carried_by, cu.x, cu.y) = (Some(t.id), t.x, t.y);
                        }
                    }
                    let info = tables().unit(t.kind);
                    let sum = display(hp_now(&me)) as u32 + display(hp_now(&t)) as u32;
                    let refund = sum.saturating_sub(10) as i64 * owner_price(s, t.owner, t.kind) / 10;
                    let tm = s.units.get_mut(&t.id).unwrap();
                    tm.hp = Hp::exact((sum.min(10) * 10) as u8);
                    tm.fuel = (tm.fuel + me.fuel).min(info.fuel as i32);
                    tm.ammo = (tm.ammo + me.ammo).min(info.ammo as i32);
                    tm.cargo.extend(me.cargo.iter().copied());
                    tm.moved = true;
                    if let Some(p) = s.player_mut(me.owner) {
                        p.funds += refund;
                    }
                    return Ok(());
                }
                _ => {}
            }
            {
                let m = s.units.get_mut(unit).unwrap();
                m.x = end.0;
                m.y = end.1;
            }
            let cargo: Vec<UnitId> = s.units[unit].cargo.clone();
            for c in cargo {
                if let Some(cu) = s.units.get_mut(&c) {
                    cu.x = end.0;
                    cu.y = end.1;
                }
            }
            match then {
                Then::Wait | Then::Load | Then::Join => Ok(()),
                Then::Attack { target } => attack(s, *unit, *target, luck),
                Then::AttackSeam { target } => {
                    let u = s.units[unit].clone();
                    let d = seam_damage(s, &u).ok_or("can't hit a seam")?;
                    // A unit that carries ammo spends one on every seam attack, even at 0
                    // (the record then shows -1: game-1751864 turn 32).
                    if tables().unit(u.kind).ammo > 0 {
                        s.units.get_mut(unit).unwrap().ammo -= 1;
                    }
                    let b = s.buildings.get_mut(target).ok_or("no seam")?;
                    if tables().tile(b.terrain).building != Some(crate::data::BuildingKind::PipeSeam) {
                        return Err("not a seam".into());
                    }
                    if b.capture - d <= 0 {
                        // Broken: rubble (passable), keeping the seam's last HP, as AWBW does.
                        let rubble = if b.terrain == 113 { 115 } else { 116 };
                        b.terrain = rubble;
                        let w = s.width as usize;
                        s.terrain[target.1 as usize * w + target.0 as usize] = rubble;
                    } else {
                        b.capture -= d;
                    }
                    Ok(())
                }
                Then::Capture => {
                    let u = s.units[unit].clone();
                    let amt = capture_amount(s, &u, display(hp_now(&u)));
                    let (prev, kind) = {
                        let b = s.buildings.get(&end).ok_or("no building")?;
                        (b.owner, tables().tile(b.terrain).building)
                    };
                    let hq = kind == Some(crate::data::BuildingKind::HQ);
                    // On a map without HQs a player's last lab stands in for one: losing it
                    // eliminates them (four recorded games end this way, e.g. game-1724140).
                    let owns = |p: PlayerId, what: crate::data::BuildingKind| s.buildings.values()
                        .filter(|x| x.owner == Some(p) && tables().tile(x.terrain).building == Some(what))
                        .count();
                    let last_lab = kind == Some(crate::data::BuildingKind::Lab)
                        && prev.is_some_and(|p| owns(p, crate::data::BuildingKind::HQ) == 0 && owns(p, crate::data::BuildingKind::Lab) <= 1);
                    let b = s.buildings.get_mut(&end).ok_or("no building")?;
                    b.capture -= amt;
                    if b.capture <= 0 {
                        b.capture = CAPTURE_POINTS;
                        b.owner = Some(u.owner);
                        if hq {
                            // AWBW turns a captured HQ into the captor's city (3 stars, income). Stored as
                            // the neutral City tile: as for every building, `owner` says whose it is, and
                            // awbw::owned_tile picks the army's tile on export.
                            let city = crate::awbw::owned_tile(b.terrain, None);
                            b.terrain = city;
                            let w = s.width as usize;
                            s.terrain[end.1 as usize * w + end.0 as usize] = city;
                        }
                        if let (true, Some(p)) = (hq || last_lab, prev) {
                            eliminate(s, p, u.owner);
                        }
                        // Capture limit: owning enough income properties wins the game
                        // (game-1754313 turn 41 ends at 32 of 46).
                        let owned = s.buildings.values().filter(|x| x.owner == Some(u.owner) && is_income_property(x.terrain)).count() as u32;
                        if s.capture_limit.is_some_and(|n| owned >= n) {
                            let captor = u.owner;
                            let losers: Vec<PlayerId> = s.players.iter().filter(|p| !s.allied(p.id, captor)).map(|p| p.id).collect();
                            for p in losers {
                                s.player_mut(p).unwrap().eliminated = true;
                            }
                        }
                    }
                    Ok(())
                }
                Then::Delete => {
                    if end != (u.x, u.y) {
                        return Err("delete only in place".into());
                    }
                    remove_unit(s, *unit);
                    Ok(())
                }
                Then::Launch { target } => {
                    let b = s.buildings.get_mut(&end).ok_or("no silo")?;
                    if tables().tile(b.terrain).building != Some(crate::data::BuildingKind::Missile) {
                        return Err("silo is empty".into());
                    }
                    let empty = 112; // Missile Silo Empty: plain ground from now on
                    b.terrain = empty;
                    let w = s.width as usize;
                    s.terrain[end.1 as usize * w + end.0 as usize] = empty;
                    missile(s, *target, false, 3);
                    Ok(())
                }
                Then::Hide | Then::Unhide => {
                    s.units.get_mut(unit).unwrap().dived = matches!(then, Then::Hide);
                    Ok(())
                }
                Then::Supply => {
                    let mut near: Vec<UnitId> = s.units.values()
                        .filter(|o| o.owner == u.owner && o.id != *unit && o.carried_by.is_none() && dist((o.x, o.y), end) == 1)
                        .map(|o| o.id).collect();
                    // Cargo of a supplied transport is supplied too (game-1751953: the Supply
                    // record lists a Lander and both units aboard).
                    let cargo: Vec<UnitId> = near.iter().flat_map(|t| s.units[t].cargo.clone()).collect();
                    near.extend(cargo);
                    for id in near {
                        let o = s.units.get_mut(&id).unwrap();
                        let info = tables().unit(o.kind);
                        o.fuel = info.fuel as i32;
                        o.ammo = info.ammo as i32;
                    }
                    Ok(())
                }
                Then::Repair { target } => {
                    let t = s.unit_at(target.0, target.1).cloned().ok_or("no repair target")?;
                    let info = tables().unit(t.kind);
                    let cost = owner_price(s, t.owner, t.kind) / 10;
                    let afford = s.player(t.owner).is_some_and(|p| p.funds >= cost);
                    let low = t.fuel < info.fuel as i32 || t.ammo < info.ammo as i32;
                    let tm = s.units.get_mut(&t.id).unwrap();
                    tm.fuel = info.fuel as i32;
                    tm.ammo = info.ammo as i32;
                    if t.hp.hi <= 90 {
                        // 9.0 or less: exactly +1.0, the fraction kept, if it can be paid for.
                        if afford {
                            tm.hp = Hp { lo: t.hp.lo.max(1) + 10, hi: t.hp.hi + 10 };
                            s.player_mut(t.owner).unwrap().funds -= cost;
                        }
                    } else if t.hp.lo < 100 && low {
                        // A 9.x unit is topped up to 10.0 for free, but only when it also
                        // needed fuel or ammo.
                        tm.hp = Hp::exact(100);
                    }
                    Ok(())
                }
                Then::Unload { cargo, to } => {
                    // Fog: a hidden enemy unit on the drop tile blocks the unload; the cargo
                    // stays aboard and the transport just waits.
                    if s.unit_at(to.0, to.1).is_some_and(|o| o.id != *unit) {
                        // Nothing happened: a transport that hadn't moved may still move.
                        if in_place_drop {
                            s.units.get_mut(unit).unwrap().moved = u.moved;
                        }
                        return Ok(());
                    }
                    let carrier = &s.units[unit];
                    if !can_unload_from(carrier.kind, s.terrain_at(carrier.x, carrier.y)) {
                        return Err("transport can't unload from sea or reef".into());
                    }
                    let t = s.units.get_mut(unit).unwrap();
                    t.cargo.retain(|x| x != cargo);
                    if in_place_drop {
                        t.moved = u.moved;
                    }
                    let cu = s.units.get_mut(cargo).ok_or("no cargo")?;
                    cu.carried_by = None;
                    cu.x = to.0;
                    cu.y = to.1;
                    cu.moved = true;
                    Ok(())
                }
            }
        }
    }
}

/// End the current player's turn and start the next living player's.
pub fn end_turn(s: &mut State) -> Result<(), String> {
    // The next player still in the game, in turn order (the current one may just have
    // been put out); a new day starts when the order wraps around.
    let n = s.players.len();
    let i = s.players.iter().position(|p| p.id == s.current).ok_or("current player not in order")?;
    let j = (1..=n).map(|k| (i + k) % n).find(|&j| !s.players[j].eliminated).ok_or("no players left")?;
    let next = s.players[j].id;
    let day = if j <= i { s.day + 1 } else { s.day };
    s.move_override.clear();
    start_turn(s, next, day);
    apc_supply(s, next);
    Ok(())
}

/// The winner, if the game is over: the last player (or team) standing. A player with no
/// units after day 1 is out (AWBW's rout rule).
pub fn winner(s: &State) -> Option<String> {
    // Out means eliminated: by capture, or by losing their last unit (`rout`). Having no
    // units is not enough.
    let alive: Vec<&Player> = s.players.iter().filter(|p| !p.eliminated).collect();
    let teams: alloc::collections::BTreeSet<&str> = alive.iter().map(|p| p.team.as_str()).collect();
    (teams.len() == 1).then(|| teams.into_iter().next().unwrap().to_string())
}
