//! Applying actions: the simulator. Turn start (income, fuel, repairs, supply) and unit
//! removal live here so the replay checker and bots share them.

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
        for c in u.cargo {
            s.units.remove(&c);
        }
        if let Some(t) = u.carried_by.and_then(|t| s.units.get_mut(&t)) {
            t.cargo.retain(|&x| x != id);
        }
    }
}

/// Start of `next`'s turn: income, then repairs/resupply on owned properties, fuel burn.
pub fn start_turn(s: &mut State, next: PlayerId, day: u32) {
    s.current = next;
    s.day = day;
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
    let fpp = s.funds_per_property + co::property_fund_increase(&co_of(s, next));
    if let Some(p) = s.player_mut(next) {
        p.funds += props * fpp;
    }
    // Fuel burn (air and sea units, per their daily rate; dived subs and hidden stealths more).
    let co = co_of(s, next);
    let ids: Vec<UnitId> = s.units.values().filter(|u| u.owner == next).map(|u| u.id).collect();
    let mut crashed = vec![];
    for id in &ids {
        let u = s.units.get_mut(id).unwrap();
        let info = tables().unit(&u.kind);
        let mut burn = info.fuel_per_turn as i32;
        if u.dived {
            burn = if u.kind == "Sub" { 5 } else { 8 };
        }
        if info.move_type == "A" {
            burn = (burn - co::air_fuel_decrease(&co)).max(0);
        }
        if burn > 0 && u.carried_by.is_none() && day > 1 {
            u.fuel = (u.fuel - burn).max(0);
            if u.fuel == 0 && matches!(info.move_type.as_str(), "A" | "S" | "L") {
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
        if b.owner != Some(next) || !services(b.terrain, &u.kind) {
            continue;
        }
        let info = tables().unit(&u.kind);
        let step = if co == "Rachel" { 30 } else { 20 };
        let cur_disp = u.hp.display_hi();
        let funds = s.player(next).map(|p| p.funds).unwrap_or(0);
        // At the owner's price (game-1759750: Colin's repairs cost him 80%).
        let per_hp = owner_price(s, next, &u.kind) / 10;
        let mut add_disp = 0u8;
        for k in (1..=(step / 10)).rev() {
            let k = k as u8;
            if cur_disp + k <= 10 && per_hp * k as i64 <= funds {
                add_disp = k;
                break;
            }
        }
        if cur_disp < 10 && add_disp == 0 && cur_disp + 1 > 10 {
            add_disp = 0;
        }
        let new_disp = (cur_disp + add_disp).min(10);
        let m = s.units.get_mut(id).unwrap();
        if add_disp > 0 {
            // An uncertain display (a range spanning several values) stays a range: each end
            // moves up by the same repair.
            let lo_disp = (u.hp.display_lo().max(1) + add_disp).min(10);
            m.hp = Hp { lo: Hp::from_display(lo_disp).lo, hi: Hp::from_display(new_disp).hi };
        } else if cur_disp == 10 {
            // Topping off a 9.x unit is free in AWBW only to a whole display point; leave as is.
        }
        m.fuel = info.fuel as i32;
        m.ammo = info.ammo as i32;
        // Its cargo is not resupplied there (53 recorded cases of carried units on ports
        // and airports keeping their fuel), unlike cargo of a unit an APC supplies.
        if add_disp > 0 {
            if let Some(p) = s.player_mut(next) {
                p.funds -= per_hp * add_disp as i64;
            }
        }
    }
    // Carriers and cruisers resupply the aircraft they carry.
    let carried: Vec<(UnitId, UnitId)> = s.units.values()
        .filter(|u| u.owner == next)
        .filter_map(|u| u.carried_by.map(|t| (u.id, t)))
        .collect();
    for (id, t) in carried {
        if matches!(s.units.get(&t).map(|t| t.kind.as_str()), Some("Carrier" | "Cruiser")) {
            let u = s.units.get_mut(&id).unwrap();
            let info = tables().unit(&u.kind);
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
pub fn apc_supply(s: &mut State, p: PlayerId) -> std::collections::BTreeSet<UnitId> {
    let apcs: Vec<(UnitId, u16, u16)> = s.units.values()
        .filter(|u| u.owner == p && u.kind == "APC" && u.carried_by.is_none())
        .map(|u| (u.id, u.x, u.y)).collect();
    let mut out = std::collections::BTreeSet::new();
    for (aid, x, y) in apcs {
        for u in s.units.values_mut() {
            if u.id != aid && u.owner == p && u.carried_by.is_none() && dist((x, y), (u.x, u.y)) == 1 {
                let info = tables().unit(&u.kind);
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
        let info = tables().unit(&u.kind);
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
/// would do to a Neotank, without luck or terrain: base x attack% x displayed HP / 10,
/// rounded down after rounding up to the next 0.05. Fits all 2241 corpus hits. None: it can't.
pub fn seam_damage(s: &State, att: &Unit) -> Option<i32> {
    let base = base_damage_now(att, "Neotank")? as u64;
    let av = att_value(s, att, s.terrain_at(att.x, att.y)) as u64;
    let hp = att.hp.display_hi().max(1) as u64;
    // AWBW rounds the raw damage up to the next 0.05 before flooring, so x.95 and above
    // count as the next whole point (13 recorded hits at 0.96, 0.99, 1.95, 4.95).
    Some(((base * av * hp + 50) / 1000) as i32)
}

/// Damage `att` does to `def` with a given luck roll (internal HP points).
pub fn damage(s: &State, att: &Unit, def: &Unit, roll: i32, counter: bool) -> u8 {
    let Some(base) = base_damage_now(att, &def.kind) else { return 0 };
    let at = s.terrain_at(att.x, att.y);
    let dt = s.terrain_at(def.x, def.y);
    let p = AttackParams {
        base: base as u32,
        att_value: att_value(s, att, at),
        att_hp: display(hp_now(att)),
        def_value: def_value(s, def, &att.kind),
        def_terrain: stars(s, def, dt),
        def_hp: display(hp_now(def)),
        luck_min: roll,
        luck_max: roll,
        scale_pct: if counter { co::counter_pct(&co_of(s, att.owner)) } else { 100 },
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
pub fn owner_price(s: &State, pid: PlayerId, kind: &str) -> i64 {
    let cost = tables().unit(kind).cost as i64;
    s.player(pid).map(|p| co::price(cost, &p.co, &p.power)).unwrap_or(cost)
}

/// One attack with its counterattack. Ammo is spent by whichever side uses its primary.
/// Charge CO power meters after a fight. AWBW: the side that loses HP gains the funds value
/// of the displayed HP it lost; the other side gains half of that. Meters stored x10, capped
/// at the super power's cost; no charge while a power is on.
pub fn charge(s: &mut State, att: &Unit, att_lost: u8, def: &Unit, def_lost: u8) {
    // At the owner's price (game-1759750: a Colin Infantry, 800 to him, charges 80%).
    let value = |u: &Unit, lost: u8| owner_price(s, u.owner, &u.kind) * lost as i64; // x10 /10
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
pub fn spend_power(s: &mut State, pid: PlayerId, kind: &str) {
    if let Some(p) = s.player_mut(pid) {
        let cost = if kind == "S" { p.scop_cost } else { p.cop_cost };
        p.meter = (p.meter - cost).max(0);
        p.power = kind.to_string();
        // Each use raises both costs by a fifth of the base (seen in the replays).
        if let Some((c, sc)) = co::power_base(&p.co) {
            p.cop_cost += c / 5;
            p.scop_cost += sc / 5;
        }
    }
}

/// Apply a power's instant effects (see `co::instant`). Returns false when the engine
/// doesn't model this CO's power yet (it was still activated; only effects are missing).
pub fn power_effects(s: &mut State, pid: PlayerId, kind: &str) -> bool {
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
        let floor_hi = u.hp.hi.min(10);
        u.hp = crate::state::Hp { lo: (u.hp.lo as i32 - hp * 10).max(1) as u8, hi: ((u.hp.hi as i32 - hp * 10).max(1) as u8).max(floor_hi) };
        if stun {
            u.stunned = true;
            u.moved = true;
        }
    }
}

/// Where AWBW isn't telling us: aim each missile at the 2-tile diamond where it takes the
/// most enemy value (cost x HP it removes). AWBW's own targeting may differ; ballpark.
fn aim_missile(s: &State, pid: PlayerId) -> (u16, u16) {
    let (mut best, mut at) = (-1i64, (0u16, 0u16));
    let enemies: Vec<&crate::state::Unit> = s.units.values().filter(|u| !s.allied(u.owner, pid) && u.carried_by.is_none()).collect();
    for c in &enemies {
        for dx in -2i32..=2 {
            for dy in -2i32..=2 {
                if dx.abs() + dy.abs() > 2 { continue; }
                let (x, y) = (c.x as i32 + dx, c.y as i32 + dy);
                if x < 0 || y < 0 { continue; }
                // Missiles hit every unit in range, the caster's own too (Rachel and Von Bolt,
                // 8 recorded uses): count own units' loss against the target.
                let v: i64 = s.units.values().filter(|u| u.carried_by.is_none() && (u.x as i32 - x).abs() + (u.y as i32 - y).abs() <= 2)
                    .map(|u| (if s.allied(u.owner, pid) { -1 } else { 1 }) * crate::data::tables().unit(&u.kind).cost as i64 * ((u.hp.hi as i64 - 1).min(30)) / 100).sum();
                if v > best { best = v; at = (x as u16, y as u16); }
            }
        }
    }
    at
}

/// The player to move activates a power. Missiles land on `targets` when given (the server
/// aims them in real games; replays record where), else on the engine's own aim.
pub fn use_power(s: &mut State, kind: &str, targets: Option<&[(u16, u16)]>) -> Result<(), String> {
    if !co::powers_ready(s, s.current).contains(&kind) {
        return Err(format!("power {kind} not ready"));
    }
    let me = s.current;
    spend_power(s, me, kind);
    power_effects_at(s, me, kind, targets);
    Ok(())
}

/// As `power_effects`, with missile targets given (the replay checker passes AWBW's).
pub fn power_effects_at(s: &mut State, pid: PlayerId, kind: &str, targets: Option<&[(u16, u16)]>) -> bool {
    let Some(e) = co::instant(&co_of(s, pid), kind) else { return false };
    for k in 0..e.missiles as usize {
        let at = match targets { Some(t) => match t.get(k) { Some(&p) => p, None => break }, None => aim_missile(s, pid) };
        missile(s, at, e.stun, e.missile_hp);
    }
    if let Some(p) = s.player_mut(pid) {
        p.power_attack = if e.funds_attack { (3 * p.funds / 1000) as i32 } else { 0 };
    }
    if let Some(kind) = e.spawn {
        let info = crate::data::tables().unit(kind);
        let cities: Vec<(u16, u16)> = s.buildings.iter()
            .filter(|(&(x, y), b)| b.owner == Some(pid) && s.unit_at(x, y).is_none()
                && crate::data::tables().tile(b.terrain).building.as_deref() == Some("City"))
            .map(|(&p, _)| p).collect();
        for (x, y) in cities {
            let id = s.units.keys().max().copied().unwrap_or(0) + 1;
            s.units.insert(id, Unit {
                id, owner: pid, kind: kind.to_string(), x, y, hp: Hp::exact(90),
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
    for id in mine {
        let u = s.units.get_mut(&id).unwrap();
        if e.gain != 0 {
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
        let own: Vec<UnitId> = s.units.values().filter(|u| u.owner == pid && co::moves_apply(&u.kind, e.moves_for)
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
                crate::data::tables().tile(b.terrain).building.as_deref().is_some_and(|k| !matches!(k, "Missile" | "PipeSeam"))
            }))
            .map(|u| u.id).collect();
        for id in hit {
            let u = s.units.get_mut(&id).unwrap();
            // Can't kill: a unit pushed to the floor shows 1 HP, internal 1-10 as far as we know.
            let floor_hi = u.hp.hi.min(10);
            u.hp = crate::state::Hp { lo: bump(u.hp.lo, -e.urban_loss * 10), hi: bump(u.hp.hi, -e.urban_loss * 10).max(floor_hi) };
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
            let foot = crate::rules::is_foot(&s.units[&id].kind);
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

pub fn attack(s: &mut State, aid: UnitId, target: Pos, luck: Luck) -> Result<(), String> {
    let att = s.units.get(&aid).cloned().ok_or("attacker gone")?;
    let def = s.unit_at(target.0, target.1).cloned().ok_or("no target")?;
    // Sonja's Counter Break: while her super lasts (through the opponent's turn), a defender
    // that can counter strikes first, without her x1.5 counter bonus; the attacker then fires
    // with what's left, and there's no second counter.
    {
        let first = co_of(s, def.owner) == "Sonja" && pw(s, def.owner) == "S"
            && dist((att.x, att.y), (def.x, def.y)) == 1 && !is_indirect(&def.kind)
            && base_damage_now(&def, &att.kind).is_some();
        if first {
            let (lo, hi) = co::luck(&co_of(s, def.owner), &pw(s, def.owner));
            let dmg = damage(s, &def, &att, luck(lo, hi), false);
            if uses_primary(&def, &att.kind) {
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
            let dmg = damage(s, &att, &def, luck(lo, hi), false);
            if uses_primary(&att, &def.kind) {
                s.units.get_mut(&aid).unwrap().ammo -= 1;
            }
            let left = hp_now(&def).saturating_sub(dmg);
            set_hp(s, def.id, left);
            charge(s, &att, att_lost, &def, display(hp_now(&def)) - display(left));
            return Ok(());
        }
    }
    let (lo, hi) = co::luck(&co_of(s, att.owner), &pw(s, att.owner));
    let dmg = damage(s, &att, &def, luck(lo, hi), false);
    if uses_primary(&att, &def.kind) {
        s.units.get_mut(&aid).unwrap().ammo -= 1;
    }
    let left = hp_now(&def).saturating_sub(dmg);
    set_hp(s, def.id, left);
    let def_lost = display(hp_now(&def)) - display(left);
    // Sasha's War Bonds: funds worth half the damage dealt (her counters pay too, below).
    if co_of(s, att.owner) == "Sasha" && pw(s, att.owner) == "S" {
        let bonus = tables().unit(&def.kind).cost as i64 * def_lost as i64 / 20;
        if let Some(p) = s.player_mut(att.owner) { p.funds += bonus; }
    }
    if left == 0 {
        charge(s, &att, 0, &def, def_lost);
        return Ok(());
    }
    let def = s.units.get(&def.id).cloned().unwrap();
    let att = s.units.get(&aid).cloned().unwrap();
    let can_counter = dist((att.x, att.y), (def.x, def.y)) == 1 && !is_indirect(&def.kind)
        && base_damage_now(&def, &att.kind).is_some();
    if can_counter {
        let (lo, hi) = co::luck(&co_of(s, def.owner), &pw(s, def.owner));
        let dmg = damage(s, &def, &att, luck(lo, hi), true);
        if uses_primary(&def, &att.kind) {
            s.units.get_mut(&def.id).unwrap().ammo -= 1;
        }
        let after = hp_now(&att).saturating_sub(dmg);
        set_hp(s, aid, after);
        let att_lost = display(hp_now(&att)) - display(after);
        // War Bonds pays on her counters too (game-1752437, during the opponent's turn).
        if co_of(s, def.owner) == "Sasha" && pw(s, def.owner) == "S" {
            let bonus = tables().unit(&att.kind).cost as i64 * att_lost as i64 / 20;
            if let Some(p) = s.player_mut(def.owner) { p.funds += bonus; }
        }
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
    match a {
        Action::EndTurn => end_turn(s),
        Action::Power { kind } => use_power(s, kind, None),
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
                    let info = tables().unit(&t.kind);
                    let sum = display(hp_now(&me)) as u32 + display(hp_now(&t)) as u32;
                    let refund = sum.saturating_sub(10) as i64 * owner_price(s, t.owner, &t.kind) / 10;
                    let tm = s.units.get_mut(&t.id).unwrap();
                    tm.hp = Hp::exact((sum.min(10) * 10) as u8);
                    tm.fuel = (tm.fuel + me.fuel).min(info.fuel as i32);
                    tm.ammo = (tm.ammo + me.ammo).min(info.ammo as i32);
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
                    if uses_primary(&u, "Neotank") {
                        s.units.get_mut(unit).unwrap().ammo -= 1;
                    }
                    let b = s.buildings.get_mut(target).ok_or("no seam")?;
                    if tables().tile(b.terrain).building.as_deref() != Some("PipeSeam") {
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
                    let b = s.buildings.get_mut(&end).ok_or("no building")?;
                    b.capture -= amt;
                    if b.capture <= 0 {
                        let prev = b.owner;
                        let hq = tables().tile(b.terrain).building.as_deref() == Some("HQ");
                        b.capture = CAPTURE_POINTS;
                        b.owner = Some(u.owner);
                        if hq {
                            // AWBW turns a captured HQ into the captor's city (3 stars, income).
                            let city = crate::awbw::owned_tile(b.terrain, None);
                            b.terrain = city;
                            let w = s.width as usize;
                            s.terrain[end.1 as usize * w + end.0 as usize] = city;
                        }
                        if let (true, Some(p)) = (hq, prev) {
                            eliminate(s, p, u.owner);
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
                    if tables().tile(b.terrain).building.as_deref() != Some("Missile") {
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
                        let info = tables().unit(&o.kind);
                        o.fuel = info.fuel as i32;
                        o.ammo = info.ammo as i32;
                    }
                    Ok(())
                }
                Then::Repair { target } => {
                    let t = s.unit_at(target.0, target.1).cloned().ok_or("no repair target")?;
                    let info = tables().unit(&t.kind);
                    let cost = owner_price(s, t.owner, &t.kind) / 10;
                    let afford = s.player(t.owner).is_some_and(|p| p.funds >= cost);
                    let tm = s.units.get_mut(&t.id).unwrap();
                    tm.fuel = info.fuel as i32;
                    tm.ammo = info.ammo as i32;
                    if afford && display(hp_now(&t)) < 10 {
                        // +1 displayed HP; the fraction under it is kept, so (like property
                        // repairs) the result is the whole range of the new display value.
                        tm.hp = Hp::from_display((display(hp_now(&t)) + 1).min(10));
                        s.player_mut(t.owner).unwrap().funds -= cost;
                    }
                    Ok(())
                }
                Then::Unload { cargo, to } => {
                    // Fog: a hidden enemy unit on the drop tile blocks the unload; the cargo
                    // stays aboard and the transport just waits.
                    if s.unit_at(to.0, to.1).is_some_and(|o| o.id != *unit) {
                        return Ok(());
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
    let order: Vec<PlayerId> = s.players.iter().filter(|p| !p.eliminated).map(|p| p.id).collect();
    let i = order.iter().position(|&p| p == s.current).ok_or("current player not in order")?;
    let next = order[(i + 1) % order.len()];
    let day = if i + 1 == order.len() { s.day + 1 } else { s.day };
    s.move_override.clear();
    if let Some(p) = s.player_mut(next) {
        p.power = "N".into(); // a power lasts until its owner's next turn
    }
    start_turn(s, next, day);
    apc_supply(s, next);
    Ok(())
}

/// The winner, if the game is over: the last player (or team) standing. A player with no
/// units after day 1 is out (AWBW's rout rule).
pub fn winner(s: &State) -> Option<String> {
    let alive: Vec<&Player> = s.players.iter()
        .filter(|p| !p.eliminated)
        .filter(|p| s.day <= 1 || s.partial || s.units.values().any(|u| u.owner == p.id))
        .collect();
    let teams: std::collections::BTreeSet<&str> = alive.iter().map(|p| p.team.as_str()).collect();
    (teams.len() == 1).then(|| teams.into_iter().next().unwrap().to_string())
}
