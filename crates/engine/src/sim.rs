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
        u.moved = false;
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
        let per_hp = info.cost as i64 / 10;
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
            m.hp = Hp::from_display(new_disp);
        } else if cur_disp == 10 {
            // Topping off a 9.x unit is free in AWBW only to a whole display point; leave as is.
        }
        m.fuel = info.fuel as i32;
        m.ammo = info.ammo as i32;
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
    for id in crashed {
        remove_unit(s, id);
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
                out.insert(u.id); // AWBW lists every adjacent unit, needed or not

                u.fuel = info.fuel as i32;
                u.ammo = info.ammo as i32;
            }
        }
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

/// Damage `att` does to `def` with a given luck roll (internal HP points).
pub fn damage(s: &State, att: &Unit, def: &Unit, roll: i32, counter: bool) -> u8 {
    let Some(base) = tables().base_damage(&att.kind, &def.kind) else { return 0 };
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
        scale: if counter { co::counter_multiplier(&co_of(s, att.owner)) } else { 1.0 },
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

/// One attack with its counterattack. Ammo is spent by whichever side uses its primary.
/// Charge CO power meters after a fight. AWBW: the side that loses HP gains the funds value
/// of the displayed HP it lost; the other side gains half of that. Meters stored x10, capped
/// at the super power's cost; no charge while a power is on.
pub fn charge(s: &mut State, att: &Unit, att_lost: u8, def: &Unit, def_lost: u8) {
    let value = |u: &Unit, lost: u8| tables().unit(&u.kind).cost as i64 * lost as i64; // x10 /10
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
    }
}

pub fn attack(s: &mut State, aid: UnitId, target: Pos, luck: Luck) -> Result<(), String> {
    let att = s.units.get(&aid).cloned().ok_or("attacker gone")?;
    let def = s.unit_at(target.0, target.1).cloned().ok_or("no target")?;
    let (lo, hi) = co::luck(&co_of(s, att.owner), &pw(s, att.owner));
    let dmg = damage(s, &att, &def, luck(lo, hi), false);
    if uses_primary(&att, &def.kind) {
        s.units.get_mut(&aid).unwrap().ammo -= 1;
    }
    let left = hp_now(&def).saturating_sub(dmg);
    set_hp(s, def.id, left);
    let def_lost = display(hp_now(&def)) - display(left);
    if left == 0 {
        charge(s, &att, 0, &def, def_lost);
        return Ok(());
    }
    let def = s.units.get(&def.id).cloned().unwrap();
    let att = s.units.get(&aid).cloned().unwrap();
    let di = tables().unit(&def.kind);
    let can_counter = dist((att.x, att.y), (def.x, def.y)) == 1 && !is_indirect(&def.kind)
        && tables().base_damage(&def.kind, &att.kind).is_some()
        && (def.ammo > 0 || di.ammo == 0 || secondary_target(&def.kind, &att.kind));
    if can_counter {
        let (lo, hi) = co::luck(&co_of(s, def.owner), &pw(s, def.owner));
        let dmg = damage(s, &def, &att, luck(lo, hi), true);
        if uses_primary(&def, &att.kind) {
            s.units.get_mut(&def.id).unwrap().ammo -= 1;
        }
        let after = hp_now(&att).saturating_sub(dmg);
        set_hp(s, aid, after);
        charge(s, &att, display(hp_now(&att)) - display(after), &def, def_lost);
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
        Action::Build { at, kind } => {
            let me = s.current;
            let p = s.player(me).ok_or("no player")?;
            let price = (tables().unit(kind).cost as f64 * co::price_multiplier(&p.co)).round() as i64;
            if price > p.funds || s.unit_at(at.0, at.1).is_some() {
                return Err(format!("can't build {kind} at {at:?}"));
            }
            s.player_mut(me).unwrap().funds -= price;
            let id = s.units.keys().max().copied().unwrap_or(0) + 1;
            let info = tables().unit(kind);
            s.units.insert(id, Unit {
                id, owner: me, kind: kind.clone(), x: at.0, y: at.1, hp: Hp::exact(100),
                fuel: info.fuel as i32, ammo: info.ammo as i32, moved: true,
                carried_by: None, cargo: vec![], dived: false,
            });
            Ok(())
        }
        Action::Move { unit, path, then } => {
            let u = s.units.get(unit).cloned().ok_or("no such unit")?;
            if u.owner != s.current || u.moved {
                return Err("unit can't act".into());
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
                    return Ok(());
                }
                Then::Join => {
                    let t = s.unit_at(end.0, end.1).filter(|o| o.id != *unit).cloned().ok_or("no join target")?;
                    let me = s.units.remove(unit).unwrap();
                    let info = tables().unit(&t.kind);
                    let sum = display(hp_now(&me)) as u32 + display(hp_now(&t)) as u32;
                    let refund = sum.saturating_sub(10) as i64 * info.cost as i64 / 10;
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
                        if let (true, Some(p)) = (hq, prev) {
                            eliminate(s, p, u.owner);
                        }
                    }
                    Ok(())
                }
                Then::Supply => {
                    let near: Vec<UnitId> = s.units.values()
                        .filter(|o| o.owner == u.owner && o.id != *unit && o.carried_by.is_none() && dist((o.x, o.y), end) == 1)
                        .map(|o| o.id).collect();
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
                    let cost = info.cost as i64 / 10;
                    let afford = s.player(t.owner).is_some_and(|p| p.funds >= cost);
                    let tm = s.units.get_mut(&t.id).unwrap();
                    tm.fuel = info.fuel as i32;
                    tm.ammo = info.ammo as i32;
                    if afford && display(hp_now(&t)) < 10 {
                        tm.hp = Hp::exact(((display(hp_now(&t)) + 1) * 10).min(100));
                        s.player_mut(t.owner).unwrap().funds -= cost;
                    }
                    Ok(())
                }
                Then::Unload { cargo, to } => {
                    s.units.get_mut(unit).unwrap().cargo.retain(|x| x != cargo);
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
        .filter(|p| s.day <= 1 || s.units.values().any(|u| u.owner == p.id))
        .collect();
    let teams: std::collections::BTreeSet<&str> = alive.iter().map(|p| p.team.as_str()).collect();
    (teams.len() == 1).then(|| teams.into_iter().next().unwrap().to_string())
}
