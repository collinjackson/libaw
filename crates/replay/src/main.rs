//! Replay checker: replays recorded AWBW games through the engine and reports every place
//! the engine disagrees with what really happened.
//!
//!   cargo run --release -p awbw-replay -- <game dir>... [--examples N] [--only CHECK]
//!
//! Each game dir holds one JSON file per player turn: the state at the start of that turn
//! plus the actions taken in it. For each turn the checker applies the actions (checking each
//! one against the rules) and the start of the next turn, then compares the result with the
//! next file's state.

mod awbw;

use awbw::*;
use awbw_engine::data::tables;
use awbw_engine::rules::*;
use awbw_engine::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Report {
    checks: BTreeMap<String, (u64, u64)>,
    examples: BTreeMap<String, Vec<String>>,
    max_examples: usize,
    turns: u64,
    turns_clean: u64,
    skipped: BTreeMap<String, u64>,
    /// Units an APC may have supplied this turn (AWBW's Supply command leaves no record):
    /// their fuel/ammo without that supply. Either value is accepted at compare time.
    maybe_supplied: BTreeMap<UnitId, (i32, i32)>,
}

impl Report {
    fn check(&mut self, name: &str, ok: bool, ctx: impl FnOnce() -> String) -> bool {
        let e = self.checks.entry(name.to_string()).or_default();
        if ok {
            e.0 += 1;
        } else {
            e.1 += 1;
            let ex = self.examples.entry(name.to_string()).or_default();
            if ex.len() < self.max_examples {
                ex.push(ctx());
            }
        }
        ok
    }
    fn skip(&mut self, why: &str) {
        *self.skipped.entry(why.to_string()).or_default() += 1;
    }
}

struct Ctx<'a> {
    game: &'a str,
    file: &'a str,
}

fn tag(c: &Ctx, msg: String) -> String {
    format!("{} {}: {}", c.game, c.file, msg)
}

fn path_of(a: &Value) -> Vec<(u16, u16)> {
    items(&a["path"]).iter().map(|p| (num(&p["x"]) as u16, num(&p["y"]) as u16)).collect()
}

fn co_of(s: &State, p: PlayerId) -> String {
    s.player(p).map(|p| p.co.clone()).unwrap_or_default()
}

fn towers(s: &State, p: PlayerId) -> u32 {
    s.buildings.values()
        .filter(|b| b.owner == Some(p) && tables().tile(b.terrain).building.as_deref() == Some("ComTower"))
        .count() as u32
}

fn is_indirect(kind: &str) -> bool {
    tables().unit(kind).range[1] > 1
}

fn is_foot(kind: &str) -> bool {
    matches!(kind, "Infantry" | "Mech")
}

fn is_air(kind: &str) -> bool {
    tables().unit(kind).move_type == "A"
}

fn is_sea(kind: &str) -> bool {
    matches!(tables().unit(kind).move_type.as_str(), "S" | "L")
}

/// Attack and defence values (100 = neutral) from COs' day-to-day abilities and com towers.
fn att_value(s: &State, att: &Unit, att_terrain: u16) -> u32 {
    let co = co_of(s, att.owner);
    let k = att.kind.as_str();
    let mut v: i32 = 100 + 10 * towers(s, att.owner) as i32;
    let direct = !is_indirect(k);
    match co.as_str() {
        "Max" if direct && !is_foot(k) && tables().unit(k).ammo + 1 > 0 && !matches!(k, "APC" | "T-Copter" | "Lander" | "Black Boat") => v += 20,
        "Max" if !direct => v -= 10,
        "Sami" if is_foot(k) => v += 30,
        "Sami" if direct => v -= 10,
        "Von Bolt" => v += 10,
        "Jake" if tables().tile(att_terrain).kind == "Plain" => v += 10,
        "Koal" if tables().tile(att_terrain).kind == "Road" => v += 10,
        "Eagle" if is_air(k) => v += 15,
        "Eagle" if is_sea(k) => v -= 30,
        "Jess" if matches!(tables().unit(k).move_type.as_str(), "T" | "W") => v += 10,
        "Jess" => v -= 10,
        _ => {}
    }
    v.max(0) as u32
}

fn def_value(s: &State, def: &Unit, attacker_kind: &str) -> u32 {
    let co = co_of(s, def.owner);
    let mut v: i32 = 100;
    match co.as_str() {
        "Von Bolt" => v += 10,
        "Eagle" if is_air(&def.kind) => v += 10,
        "Javier" => {
            v += 10 * towers(s, def.owner) as i32;
            if is_indirect(attacker_kind) {
                v += 20;
            }
        }
        _ => {}
    }
    v as u32
}

fn luck_max(s: &State, p: PlayerId) -> u32 {
    if co_of(s, p) == "Nell" { 19 } else { 9 }
}

/// Does an attack from `att` on `def` leave `def` with the recorded displayed HP?
/// Returns the defender's new HP range consistent with the record, or None.
fn attack_feasible(s: &State, att: &Unit, def: &Unit, recorded: u8, base: u8) -> Option<Hp> {
    let at = s.terrain_at(att.x, att.y);
    let dt = s.terrain_at(def.x, def.y);
    let mut out: Option<Hp> = None;
    for ahp in att.hp.display_lo().max(1)..=att.hp.display_hi() {
        for dhp in def.hp.display_lo().max(1)..=def.hp.display_hi() {
            let p = AttackParams {
                base: base as u32,
                att_value: att_value(s, att, at),
                att_hp: ahp,
                def_value: def_value(s, def, &att.kind),
                def_terrain: terrain_stars(dt, &def.kind),
                def_hp: dhp,
                luck_max: luck_max(s, att.owner),
            };
            let (dlo, dhi) = damage_range(&p);
            let cand = def.hp.intersect(Hp::from_display(dhp))?;
            let after = Hp {
                lo: cand.lo.saturating_sub(dhi.min(255) as u8),
                hi: cand.hi.saturating_sub(dlo.min(255) as u8),
            };
            if let Some(h) = after.intersect(Hp::from_display(recorded)) {
                out = Some(match out {
                    None => h,
                    Some(o) => Hp { lo: o.lo.min(h.lo), hi: o.hi.max(h.hi) },
                });
            }
        }
    }
    out
}

/// Apply one turn's actions. Returns false if the turn hit something unsupported.
fn apply_actions(s: &mut State, actions: &[Value], r: &mut Report, c: &Ctx) -> Result<(), String> {
    for a in actions {
        let kind = a["action"].as_str().unwrap_or("?");
        match kind {
            "Move" => {
                let rec = unit_from(&a["unit"]);
                let Some(u) = s.units.get(&rec.id).cloned() else {
                    r.check("move.unit_known", false, || tag(c, format!("unit {} not in state", rec.id)));
                    continue;
                };
                let path = path_of(a);
                r.check("move.path_starts_at_unit", path.first() == Some(&(u.x, u.y)), || {
                    tag(c, format!("{} at {:?}, path starts {:?}", u.kind, (u.x, u.y), path.first()))
                });
                let trapped = a["trapped"].as_bool().unwrap_or(false);
                match path_cost(s, &u, &path) {
                    Ok(cost) => {
                        let range = movement_range(s, &u);
                        r.check("move.within_range", cost <= range, || {
                            tag(c, format!("{} ({}) spent {cost} of {range}", u.kind, co_of(s, u.owner)))
                        });
                        r.check("move.within_fuel", cost as i32 <= u.fuel, || {
                            tag(c, format!("{} spent {cost} with fuel {}", u.kind, u.fuel))
                        });
                        let want = u.fuel - cost as i32;
                        r.check("move.fuel_after", rec.fuel == want, || {
                            tag(c, format!("{} fuel {} -{cost} = {want}, recorded {}", u.kind, u.fuel, rec.fuel))
                        });
                    }
                    Err(e) => {
                        r.check("move.path_legal", trapped, || tag(c, e));
                    }
                }
                let end = *path.last().unwrap_or(&(u.x, u.y));
                r.check("move.end_matches_record", end == (rec.x, rec.y), || {
                    tag(c, format!("path ends {end:?}, unit recorded at {:?}", (rec.x, rec.y)))
                });
                // Leaving a building mid-capture resets it.
                if (u.x, u.y) != (rec.x, rec.y) {
                    if let Some(b) = s.buildings.get_mut(&(u.x, u.y)) {
                        b.capture = CAPTURE_POINTS;
                    }
                }
                // An APC may Supply after moving, which AWBW doesn't record as an action.
                // Assume it did, and accept either outcome at compare time.
                if u.kind == "APC" {
                    let (owner, end) = (u.owner, (rec.x, rec.y));
                    for o in s.units.values_mut() {
                        if o.owner == owner && o.id != u.id && o.carried_by.is_none() && dist(end, (o.x, o.y)) == 1 {
                            let info = tables().unit(&o.kind);
                            r.maybe_supplied.entry(o.id).or_insert((o.fuel, o.ammo));
                            o.fuel = info.fuel as i32;
                            o.ammo = info.ammo as i32;
                        }
                    }
                }
                let m = s.units.get_mut(&rec.id).unwrap();
                m.x = rec.x;
                m.y = rec.y;
                m.fuel = rec.fuel;
                m.moved = true;
                // Cargo rides along.
                for cid in m.cargo.clone() {
                    if let Some(cu) = s.units.get_mut(&cid) {
                        cu.x = rec.x;
                        cu.y = rec.y;
                    }
                }
            }
            "Build" => {
                let nu = unit_from(&a["newUnit"]);
                let cost = tables().unit(&nu.kind).cost as i64;
                let funds = s.player(nu.owner).map(|p| p.funds).unwrap_or(0);
                r.check("build.affordable", cost <= funds, || {
                    tag(c, format!("{} costs {cost}, funds {funds}", nu.kind))
                });
                let t = s.terrain_at(nu.x, nu.y);
                let b = tables().tile(t).building.clone().unwrap_or_default();
                let mt = tables().unit(&nu.kind).move_type.clone();
                let site_ok = match b.as_str() {
                    "Base" => matches!(mt.as_str(), "F" | "B" | "T" | "W" | "P"),
                    "Airport" => mt == "A",
                    "Port" => matches!(mt.as_str(), "S" | "L"),
                    _ => false,
                };
                r.check("build.site", site_ok, || tag(c, format!("{} built on {b}", nu.kind)));
                let owned = s.buildings.get(&(nu.x, nu.y)).and_then(|b| b.owner) == Some(nu.owner);
                r.check("build.owns_site", owned, || tag(c, format!("{} at {:?}", nu.kind, (nu.x, nu.y))));
                r.check("build.tile_free", s.unit_at(nu.x, nu.y).is_none(), || {
                    tag(c, format!("{} at {:?} occupied", nu.kind, (nu.x, nu.y)))
                });
                let info = tables().unit(&nu.kind);
                r.check("build.fresh_stats", nu.fuel == info.fuel as i32 && nu.ammo == info.ammo as i32, || {
                    tag(c, format!("{} fuel {} ammo {}", nu.kind, nu.fuel, nu.ammo))
                });
                if let Some(p) = s.player_mut(nu.owner) {
                    p.funds -= cost;
                }
                let mut nu = nu;
                nu.moved = true;
                nu.hp = Hp::exact(100);
                s.units.insert(nu.id, nu);
            }
            "Capt" => {
                let bi = &a["buildingInfo"];
                let (x, y) = (num(&bi["buildings_x"]) as u16, num(&bi["buildings_y"]) as u16);
                let left = num(&bi["buildings_capture"]) as i32;
                let Some(u) = s.unit_at(x, y).cloned() else {
                    r.check("capt.unit_on_building", false, || tag(c, format!("nobody at {:?}", (x, y))));
                    continue;
                };
                r.check("capt.foot_unit", is_foot(&u.kind), || tag(c, format!("{} capturing", u.kind)));
                let Some(b) = s.buildings.get(&(x, y)).cloned() else {
                    r.check("capt.is_building", false, || tag(c, format!("no building at {:?}", (x, y))));
                    continue;
                };
                let Some(dhp) = u.hp.display() else {
                    r.skip("capt: hp uncertain");
                    continue;
                };
                let after = b.capture - capture_amount(s, &u, dhp);
                let (want_left, flips) = if after <= 0 { (CAPTURE_POINTS, true) } else { (after, false) };
                r.check("capt.points", left == want_left, || {
                    tag(c, format!("{} {}hp ({}) on {} from {}: want {want_left}, recorded {left}",
                        u.kind, dhp, co_of(s, u.owner), tables().tile(b.terrain).name, b.capture))
                });
                let bm = s.buildings.get_mut(&(x, y)).unwrap();
                bm.capture = left;
                if flips || (left == CAPTURE_POINTS && b.capture != CAPTURE_POINTS) {
                    bm.owner = Some(u.owner);
                    // HQ capture eliminates its owner (units go too) — handled at compare time.
                }
                if let Some(m) = s.units.get_mut(&u.id) {
                    m.moved = true;
                }
            }
            "Fire" => {
                let ra = &a["attacker"];
                let rd = &a["defender"];
                let (aid, did) = (num(&ra["units_id"]) as u32, num(&rd["units_id"]) as u32);
                let (Some(att), Some(def)) = (s.units.get(&aid).cloned(), s.units.get(&did).cloned()) else {
                    r.check("fire.units_known", false, || tag(c, format!("{aid} or {did} missing")));
                    continue;
                };
                let d = dist((att.x, att.y), (def.x, def.y));
                let ai = tables().unit(&att.kind);
                let mut maxr = ai.range[1] as u32;
                if co_of(s, att.owner) == "Max" && maxr > 1 {
                    maxr -= 1;
                }
                r.check("fire.range", d >= ai.range[0] as u32 && d <= maxr, || {
                    tag(c, format!("{} -> {} at distance {d}, range {}..{maxr}", att.kind, def.kind, ai.range[0]))
                });
                if is_indirect(&att.kind) {
                    r.check("fire.indirect_didnt_move", !moved_this_action(&att), || tag(c, att.kind.clone()));
                }
                let def_rec = hp_display(&rd["units_hit_points"]);
                let att_rec = hp_display(&ra["units_hit_points"]);
                let Some(base) = tables().base_damage(&att.kind, &def.kind) else {
                    r.check("fire.can_target", false, || tag(c, format!("{} can't hit {}", att.kind, def.kind)));
                    continue;
                };
                let primary = uses_primary(&att, &def.kind);
                let ammo_after = num(&ra["units_ammo"]) as i32;
                let want_ammo = if primary { att.ammo - 1 } else { att.ammo };
                r.check("fire.attacker_ammo", ammo_after == want_ammo || att_rec == 0, || {
                    tag(c, format!("{} ammo {} -> {ammo_after} vs {} (primary={primary})", att.kind, att.ammo, def.kind))
                });
                let new_def = attack_feasible(s, &att, &def, def_rec, base);
                r.check("fire.damage", new_def.is_some(), || {
                    tag(c, format!("{} {}hp ({}) -> {} {}hp ({}) on {}: recorded {}hp",
                        att.kind, att.hp.display_hi(), co_of(s, att.owner), def.kind, def.hp.display_hi(),
                        co_of(s, def.owner), tables().tile(s.terrain_at(def.x, def.y)).name, def_rec))
                });
                let new_def_hp = new_def.unwrap_or(Hp::from_display(def_rec));
                // Counterattack.
                let mut new_att_hp = att.hp;
                if def_rec > 0 {
                    let di = tables().unit(&def.kind);
                    let can_counter = d == 1 && di.range[1] == 1 && !is_indirect(&def.kind)
                        && tables().base_damage(&def.kind, &att.kind).is_some()
                        && (def.ammo > 0 || di.second_weapon || di.ammo == 0);
                    if can_counter {
                        let mut d2 = def.clone();
                        d2.hp = new_def_hp;
                        let b2 = tables().base_damage(&def.kind, &att.kind).unwrap();
                        let cf = attack_feasible(s, &d2, &att, att_rec, b2);
                        r.check("fire.counter_damage", cf.is_some(), || {
                            tag(c, format!("counter {} {}hp ({}) -> {} {}hp: recorded {}hp",
                                def.kind, def_rec, co_of(s, def.owner), att.kind, att.hp.display_hi(), att_rec))
                        });
                        new_att_hp = cf.unwrap_or(Hp::from_display(att_rec));
                        let def_ammo_after = num(&rd["units_ammo"]) as i32;
                        let want = if uses_primary(&def, &att.kind) { def.ammo - 1 } else { def.ammo };
                        r.check("fire.counter_ammo", def_ammo_after == want || def_rec == 0, || {
                            tag(c, format!("{} ammo {} -> {def_ammo_after} countering {}", def.kind, def.ammo, att.kind))
                        });
                    } else {
                        r.check("fire.no_counter", att_rec == att.hp.display_hi() || att.hp.display().is_none(), || {
                            tag(c, format!("{} took damage from {} that can't counter", att.kind, def.kind))
                        });
                    }
                }
                if let Some(m) = s.units.get_mut(&aid) {
                    m.hp = if att_rec == 0 { Hp::exact(0) } else { new_att_hp };
                    m.ammo = ammo_after;
                    m.moved = true;
                }
                if let Some(m) = s.units.get_mut(&did) {
                    m.hp = if def_rec == 0 { Hp::exact(0) } else { new_def_hp };
                    m.ammo = num(&rd["units_ammo"]) as i32;
                }
                for id in [aid, did] {
                    if s.units.get(&id).map(|u| u.hp.hi == 0).unwrap_or(false) {
                        remove_unit(s, id);
                    }
                }
            }
            "Load" => {
                let (lid, tid) = (num(&a["loadedId"]) as u32, num(&a["transportId"]) as u32);
                let (Some(l), Some(t)) = (s.units.get(&lid).cloned(), s.units.get(&tid).cloned()) else {
                    r.check("load.units_known", false, || tag(c, format!("{lid}/{tid}")));
                    continue;
                };
                r.check("load.same_tile", (l.x, l.y) == (t.x, t.y), || tag(c, format!("{} onto {}", l.kind, t.kind)));
                r.check("load.capacity", t.cargo.len() < capacity(&t.kind), || {
                    tag(c, format!("{} full ({})", t.kind, t.cargo.len()))
                });
                r.check("load.cargo_type", can_carry(&t.kind, &l.kind), || tag(c, format!("{} into {}", l.kind, t.kind)));
                s.units.get_mut(&tid).unwrap().cargo.push(lid);
                let lu = s.units.get_mut(&lid).unwrap();
                lu.carried_by = Some(tid);
                lu.moved = true;
            }
            "Unload" => {
                let tid = num(&a["transportId"]) as u32;
                let uu = unit_from(&a["unloadedUnit"]);
                let Some(t) = s.units.get(&tid).cloned() else {
                    r.check("unload.units_known", false, || tag(c, format!("transport {tid}")));
                    continue;
                };
                r.check("unload.was_cargo", t.cargo.contains(&uu.id), || tag(c, format!("{} not in {}", uu.kind, t.kind)));
                r.check("unload.adjacent", dist((t.x, t.y), (uu.x, uu.y)) == 1, || {
                    tag(c, format!("{} from {:?} to {:?}", uu.kind, (t.x, t.y), (uu.x, uu.y)))
                });
                let tt = s.terrain_at(uu.x, uu.y);
                r.check("unload.passable", move_cost(tt, &tables().unit(&uu.kind).move_type).is_some(), || {
                    tag(c, format!("{} onto {}", uu.kind, tables().tile(tt).name))
                });
                s.units.get_mut(&tid).unwrap().cargo.retain(|&x| x != uu.id);
                if let Some(m) = s.units.get_mut(&uu.id) {
                    m.carried_by = None;
                    m.x = uu.x;
                    m.y = uu.y;
                    m.moved = true;
                }
            }
            "Join" => {
                let jid = num(&a["joinId"]) as u32;
                let ju = unit_from(&a["joinedUnit"]);
                let new_funds = num(&a["newFunds"]);
                let (Some(mover), Some(target)) = (s.units.get(&jid).cloned(), s.units.get(&ju.id).cloned()) else {
                    r.check("join.units_known", false, || tag(c, format!("{jid}/{}", ju.id)));
                    continue;
                };
                r.check("join.same_kind", mover.kind == target.kind, || tag(c, format!("{} into {}", mover.kind, target.kind)));
                let (Some(a1), Some(a2)) = (mover.hp.display(), target.hp.display()) else {
                    r.skip("join: hp uncertain");
                    continue;
                };
                let sum = a1 as u32 + a2 as u32;
                let want_hp = sum.min(10) as u8;
                let refund = (sum.saturating_sub(10)) as i64 * tables().unit(&mover.kind).cost as i64 / 10;
                r.check("join.hp", hp_display(&a["joinedUnit"]["units_hit_points"]) == want_hp, || {
                    tag(c, format!("{a1}+{a2} -> {}", hp_display(&a["joinedUnit"]["units_hit_points"])))
                });
                let funds = s.player(mover.owner).map(|p| p.funds).unwrap_or(0);
                r.check("join.refund", new_funds == funds + refund, || {
                    tag(c, format!("{} {a1}+{a2}: funds {funds} + {refund} vs recorded {new_funds}", mover.kind))
                });
                if let Some(p) = s.player_mut(mover.owner) {
                    p.funds = new_funds;
                }
                remove_unit(s, jid);
                let t = s.units.get_mut(&ju.id).unwrap();
                t.hp = Hp::from_display(want_hp);
                t.fuel = ju.fuel;
                t.ammo = ju.ammo;
                t.moved = true;
            }
            "Repair" => {
                let boat = num(&a["unitId"]) as u32;
                let rid = num(&a["repairedUnit"]["units_id"]) as u32;
                let hp_after = hp_display(&a["repairedUnit"]["units_hit_points"]);
                let funds = num(&a["funds"]);
                if let (Some(b), Some(t)) = (s.units.get(&boat).cloned(), s.units.get(&rid).cloned()) {
                    r.check("repair.adjacent", dist((b.x, b.y), (t.x, t.y)) == 1, || tag(c, t.kind.clone()));
                    if let Some(before) = t.hp.display() {
                        let want = (before + 1).min(10);
                        r.check("repair.hp", hp_after == want, || tag(c, format!("{} {before} -> {hp_after}", t.kind)));
                        let cost = if want > before { tables().unit(&t.kind).cost as i64 / 10 } else { 0 };
                        let have = s.player(t.owner).map(|p| p.funds).unwrap_or(0);
                        r.check("repair.funds", funds == have - cost, || {
                            tag(c, format!("{} repair: {have} - {cost} vs {funds}", t.kind))
                        });
                    }
                    let info = tables().unit(&t.kind);
                    let m = s.units.get_mut(&rid).unwrap();
                    m.hp = Hp::from_display(hp_after);
                    m.fuel = info.fuel as i32;
                    m.ammo = info.ammo as i32;
                    if let Some(p) = s.player_mut(t.owner) {
                        p.funds = funds;
                    }
                    if let Some(bm) = s.units.get_mut(&boat) {
                        bm.moved = true;
                    }
                }
            }
            "NextTurn" => {
                let next = num(&a["nextPId"]) as u32;
                let day = num(&a["day"]) as u32;
                let next_funds = num(&a["nextFunds"]);
                let before: BTreeMap<UnitId, Unit> = s.units.clone();
                start_turn(s, next, day, r, c);
                let supplied = apc_supply(s, next);
                let rec_sup: std::collections::BTreeSet<UnitId> = items(&a["supplied"]).iter().map(|x| num(x) as u32).collect();
                r.check("turn.supplied_set", supplied == rec_sup, || {
                    tag(c, format!("engine supplied {supplied:?}, recorded {rec_sup:?}"))
                });
                check_repairs(s, &before, a, r, c);
                let ours = s.player(next).map(|p| p.funds).unwrap_or(-1);
                r.check("turn.next_funds", ours == next_funds, || {
                    tag(c, format!("next player {next} ({}): engine funds {ours}, recorded {next_funds}", co_of(s, next)))
                });
                if let Some(p) = s.player_mut(next) {
                    p.funds = next_funds;
                }
            }
            "Resign" => return Err("resign".into()),
            "Power" => return Err("co power".into()),
            other => return Err(format!("unsupported action {other}")),
        }
    }
    Ok(())
}

/// NextTurn lists the units it repaired (with new HP) and the units APCs supplied.
fn check_repairs(s: &State, before: &BTreeMap<UnitId, Unit>, a: &Value, r: &mut Report, c: &Ctx) {
    let rec: BTreeMap<UnitId, u8> = items(&a["repaired"]).iter()
        .map(|x| (num(&x["units_id"]) as u32, hp_display(&x["units_hit_points"]))).collect();
    let next = s.current;
    for (id, u) in &s.units {
        if u.owner != next {
            continue;
        }
        let Some(b) = before.get(id) else { continue };
        let ours = u.hp != b.hp;
        let theirs = rec.get(id);
        r.check("turn.repaired_set", ours == theirs.is_some_and(|&h| Some(h) != b.hp.display()) || theirs.is_some() == ours, || {
            tag(c, format!("{} {id} ({}) hp {:?}: engine repaired={ours}, recorded {:?}", u.kind, co_of(s, next), b.hp.display(), theirs))
        });
        if let Some(&h) = theirs {
            r.check("turn.repaired_hp", u.hp.display_hi() == h, || {
                tag(c, format!("{} {id} ({}) {:?} -> engine {}, recorded {h}", u.kind, co_of(s, next), b.hp.display(), u.hp.display_hi()))
            });
        }
    }
    for x in items(&a["supplied"]) {
        let id = num(x) as u32;
        r.check("turn.supplied_known", s.units.contains_key(&id), || tag(c, format!("supplied {id} unknown")));
    }
}

/// End of turn: APCs resupply adjacent own units. Returns the units that needed it.
fn apc_supply(s: &mut State, p: PlayerId) -> std::collections::BTreeSet<UnitId> {
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

fn moved_this_action(_u: &Unit) -> bool {
    false // Move and Fire are separate records; the move check covers indirect move-and-fire later.
}

fn capacity(kind: &str) -> usize {
    match kind {
        "APC" | "T-Copter" => 1,
        "Lander" | "Cruiser" | "Carrier" | "Black Boat" => 2,
        _ => 0,
    }
}

fn can_carry(t: &str, l: &str) -> bool {
    let mt = tables().unit(l).move_type.clone();
    match t {
        "APC" | "T-Copter" | "Black Boat" => is_foot(l),
        "Lander" => matches!(mt.as_str(), "F" | "B" | "T" | "W"),
        "Cruiser" => matches!(l, "B-Copter" | "T-Copter"),
        "Carrier" => mt == "A",
        _ => false,
    }
}

fn remove_unit(s: &mut State, id: UnitId) {
    if let Some(u) = s.units.remove(&id) {
        for c in u.cargo {
            s.units.remove(&c);
        }
        if let Some(t) = u.carried_by.and_then(|t| s.units.get_mut(&t)) {
            t.cargo.retain(|&x| x != id);
        }
    }
}

/// Start of `next`'s turn: income, then repairs/resupply on owned properties, fuel burn.
fn start_turn(s: &mut State, next: PlayerId, day: u32, _r: &mut Report, _c: &Ctx) {
    s.current = next;
    s.day = day;
    for u in s.units.values_mut() {
        u.moved = false;
    }
    let props = s.buildings.values()
        .filter(|b| b.owner == Some(next) && is_income_property(b.terrain))
        .count() as i64;
    let fpp = s.funds_per_property;
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
        if co == "Eagle" && info.move_type == "A" {
            burn -= 2;
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

/// Compare engine state after a turn with the recorded state at the start of the next turn.
fn compare(ours: &State, rec: &State, r: &mut Report, c: &Ctx) -> bool {
    let mut clean = true;
    for (id, ru) in &rec.units {
        let Some(u) = ours.units.get(id) else {
            clean &= r.check("state.unit_exists", false, || tag(c, format!("recorded {} {id} missing", ru.kind)));
            continue;
        };
        clean &= r.check("state.unit_pos", (u.x, u.y) == (ru.x, ru.y), || {
            tag(c, format!("{} {id} at {:?}, recorded {:?}", u.kind, (u.x, u.y), (ru.x, ru.y)))
        });
        let rd = ru.hp.display_hi();
        clean &= r.check("state.unit_hp", rd >= u.hp.display_lo() && rd <= u.hp.display_hi(), || {
            tag(c, format!("{} {id} hp {}..{}, recorded {rd}", u.kind, u.hp.display_lo(), u.hp.display_hi()))
        });
        let alt = r.maybe_supplied.get(id).copied();
        let fuel_ok = u.fuel == ru.fuel || alt.is_some_and(|(f, _)| f - (tables().unit(&u.kind).fuel as i32 - u.fuel) == ru.fuel || f == ru.fuel);
        let ammo_ok = u.ammo == ru.ammo || alt.is_some_and(|(_, a)| a == ru.ammo || a - (tables().unit(&u.kind).ammo as i32 - u.ammo) == ru.ammo);
        clean &= r.check("state.unit_fuel", fuel_ok, || {
            let apc = ours.units.values().any(|a| a.kind == "APC" && a.owner == u.owner && dist((a.x, a.y), (u.x, u.y)) == 1);
            tag(c, format!("{} {id} ({}) fuel {}, recorded {} [on {} owner={:?} carried={:?} apc_adj={apc} mine_next={}]", u.kind, co_of(ours, u.owner), u.fuel, ru.fuel,
                tables().tile(ours.terrain_at(u.x, u.y)).name, ours.buildings.get(&(u.x, u.y)).map(|b| b.owner == Some(u.owner)), u.carried_by.map(|t| ours.units[&t].kind.clone()), u.owner == ours.current))
        });
        clean &= r.check("state.unit_ammo", ammo_ok, || {
            tag(c, format!("{} {id} ammo {}, recorded {}", u.kind, u.ammo, ru.ammo))
        });
    }
    for (id, u) in &ours.units {
        if !rec.units.contains_key(id) {
            clean &= r.check("state.unit_extra", false, || tag(c, format!("engine has {} {id}, record doesn't", u.kind)));
        }
    }
    for (pos, rb) in &rec.buildings {
        let ob = ours.buildings.get(pos);
        clean &= r.check("state.building_owner", ob.map(|b| b.owner) == Some(rb.owner), || {
            tag(c, format!("{pos:?} owner {:?}, recorded {:?}", ob.map(|b| b.owner), rb.owner))
        });
        clean &= r.check("state.building_capture", ob.map(|b| b.capture) == Some(rb.capture), || {
            tag(c, format!("{pos:?} capture {:?}, recorded {}", ob.map(|b| b.capture), rb.capture))
        });
    }
    for rp in &rec.players {
        let op = ours.player(rp.id);
        clean &= r.check("state.funds", op.map(|p| p.funds) == Some(rp.funds), || {
            tag(c, format!("player {} ({}) funds {:?}, recorded {}", rp.id, rp.co, op.map(|p| p.funds), rp.funds))
        });
    }
    clean
}

fn check_game(dir: &Path, r: &mut Report) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    files.sort();
    let game = dir.file_name().unwrap().to_string_lossy().to_string();
    let load = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    let mut cur = load(&files[0]);
    for i in 0..files.len().saturating_sub(1) {
        let next = load(&files[i + 1]);
        let file = files[i].file_name().unwrap().to_string_lossy().to_string();
        let c = Ctx { game: &game, file: &file };
        let mut s = state_from(&cur);
        r.maybe_supplied.clear();
        let actions: Vec<Value> = items(&cur["actions"]).into_iter().cloned().collect();
        if !actions.iter().any(|a| a["action"] == "NextTurn") {
            // The export lost this turn (an empty reply); the next file isn't its successor.
            r.skip("turn not compared: no NextTurn (export gap or game end)");
            cur = next;
            continue;
        }
        match apply_actions(&mut s, &actions, r, &c) {
            Ok(()) => {
                r.turns += 1;
                let rec = state_from(&next);
                if compare(&s, &rec, r, &c) {
                    r.turns_clean += 1;
                }
            }
            Err(why) => r.skip(&format!("turn not compared: {why}")),
        }
        cur = next;
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut r = Report { max_examples: 3, ..Default::default() };
    let mut only: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--examples" => {
                r.max_examples = args[i + 1].parse().unwrap();
                args.drain(i..i + 2);
            }
            "--only" => {
                only = Some(args[i + 1].clone());
                args.drain(i..i + 2);
            }
            _ => i += 1,
        }
    }
    for d in &args {
        check_game(Path::new(d), &mut r);
    }
    let pct = |a: u64, b: u64| if a + b == 0 { 100.0 } else { 100.0 * a as f64 / (a + b) as f64 };
    println!("turns compared: {}  clean: {} ({:.1}%)", r.turns, r.turns_clean, pct(r.turns_clean, r.turns - r.turns_clean));
    let (mut tp, mut tf) = (0, 0);
    for (k, (p, f)) in &r.checks {
        tp += p;
        tf += f;
        if only.as_deref().map(|o| k.starts_with(o)).unwrap_or(true) {
            println!("{:28} {:7} ok {:6} bad  {:6.2}%", k, p, f, pct(*p, *f));
        }
    }
    println!("all checks: {:.3}% of {}", pct(tp, tf), tp + tf);
    for (k, n) in &r.skipped {
        println!("skipped: {k} x{n}");
    }
    for (k, ex) in &r.examples {
        if only.as_deref().map(|o| k.starts_with(o)).unwrap_or(true) {
            for e in ex {
                println!("  [{k}] {e}");
            }
        }
    }
}
