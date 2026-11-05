//! Replay checker: replays recorded AWBW games through the engine and reports every place
//! the engine disagrees with what really happened.
//!
//!   cargo run --release -p awbw-replay -- <game dir>... [--examples N] [--only CHECK]
//!
//! Each game dir holds one JSON file per player turn: the state at the start of that turn
//! plus the actions taken in it. For each turn the checker applies the actions (checking each
//! one against the rules) and the start of the next turn, then compares the result with the
//! next file's state.



use awbw_engine::awbw::*;
mod matchrun;
use awbw_engine::view::{describe, frame, log_header};
use awbw_engine::data::tables;
use awbw_engine::rules::*;
use awbw_engine::*;
use awbw_engine::combat::*;
use awbw_engine::sim::*;
use serde_json::{json, Value};
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
    /// Counts for reporting, e.g. power uses whose effects differ, by CO and power.
    tally: BTreeMap<String, u64>,
    /// Units an APC may have supplied this turn (AWBW's Supply command leaves no record):
    /// their fuel/ammo without that supply. Either value is accepted at compare time.
    maybe_supplied: BTreeMap<UnitId, (i32, i32)>,
    /// Unit ids alive in the next turn file. Used only to settle a hidden-HP unit's fate
    /// (Sonja) when the damage range includes both death and survival.
    next_alive: std::collections::HashSet<UnitId>,
    /// Units in a hidden-HP (Sonja) exchange this turn: their HP isn't knowable from the
    /// records, so their HP isn't compared.
    hidden_hp: std::collections::HashSet<UnitId>,
    /// The last Move this turn: (unit, whether it changed tiles). Fire/Capt follow their Move.
    last_move: Option<(UnitId, bool)>,
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











/// Is the unit alive after action `i`? Yes if it's in the next turn file or a later action
/// this turn mentions it.
fn alive_after(r: &Report, actions: &[Value], i: usize, id: UnitId) -> bool {
    let needle = id.to_string();
    r.next_alive.contains(&id) || actions[i + 1..].iter().any(|a| a.to_string().contains(&needle))
}

/// A hidden result ("?", Sonja): every HP the formula allows, with death settled by
/// whether the unit is in the next turn file. None if nothing fits.
fn hidden_range(s: &State, alive: bool, att: &Unit, def: &Unit, base: u8, counter: bool) -> Option<Hp> {
    let mut out: Option<Hp> = None;
    for d in 0..=10u8 {
        if (d > 0) != alive {
            continue;
        }
        if let Some(h) = attack_feasible(s, att, def, d, base, counter) {
            out = Some(match out { None => h, Some(o) => Hp { lo: o.lo.min(h.lo), hi: o.hi.max(h.hi) } });
        }
    }
    // Nothing fits but the unit is known to live on: its HP is simply unknown.
    if out.is_none() && alive {
        return Some(Hp { lo: 1, hi: def.hp.hi.max(1) });
    }
    out
}

/// Does an attack from `att` on `def` leave `def` with the recorded displayed HP?
/// Returns the defender's new HP range consistent with the record, or None.
fn attack_feasible(s: &State, att: &Unit, def: &Unit, recorded: u8, base: u8, counter: bool) -> Option<Hp> {
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
                def_terrain: stars(s, def, dt),
                def_hp: dhp,
                luck_min: awbw_engine::co::luck(&co_of(s, att.owner), &pw(s, att.owner)).0,
                luck_max: awbw_engine::co::luck(&co_of(s, att.owner), &pw(s, att.owner)).1,
                scale: if counter { awbw_engine::co::counter_multiplier(&co_of(s, att.owner)) } else { 1.0 },
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
    for (i, a) in actions.iter().enumerate() {
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
                // Simulator check: the destination is one the generator offers.
                if !trapped {
                    let end = *path.last().unwrap_or(&(u.x, u.y));
                    let reach = awbw_engine::actions::reachable(s, &u);
                    r.check("legal.move_reachable", reach.contains_key(&end), || {
                        tag(c, format!("{} ({}) {:?} -> {end:?} not generated", u.kind, co_of(s, u.owner), (u.x, u.y)))
                    });
                }
                r.last_move = Some((u.id, path.len() > 1));
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
                        let alt = r.maybe_supplied.get(&u.id).map(|&(f, _)| f - cost as i32);
                        r.check("move.fuel_after", rec.fuel == want || alt == Some(rec.fuel), || {
                            {
                            let tiles: Vec<String> = path[1..].iter().map(|&(x, y)| {
                                let t = s.terrain_at(x, y);
                                format!("{}:{}", tables().tile(t).name, awbw_engine::rules::move_cost_in(t, &tables().unit(&u.kind).move_type, s.weather).unwrap_or(99))
                            }).collect();
                            tag(c, format!("{} fuel {} -{cost} = {want}, recorded {} [{}] {:?}", u.kind, u.fuel, rec.fuel, s.weather, tiles))
                        }
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
                // Rubble from a broken seam keeps the seam's last HP in its capture field
                // until a unit stops on it (passing over doesn't count).
                if (u.x, u.y) != (rec.x, rec.y) {
                    if let Some(b) = s.buildings.get_mut(&(rec.x, rec.y)) {
                        if matches!(b.terrain, 115 | 116) {
                            b.capture = CAPTURE_POINTS;
                        }
                    }
                }
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
                let offered = awbw_engine::actions::builds(s);
                r.check("legal.build", offered.contains(&((nu.x, nu.y), nu.kind.clone())), || {
                    tag(c, format!("{} at {:?} not generated", nu.kind, (nu.x, nu.y)))
                });
                let cost = (tables().unit(&nu.kind).cost as f64
                    * awbw_engine::co::price_multiplier(&co_of(s, nu.owner), &s.player(nu.owner).map(|p| p.power.clone()).unwrap_or_default())).round() as i64;
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
                r.check("legal.capture", awbw_engine::actions::options_at(s, &u, (x, y)).contains(&awbw_engine::actions::Then::Capture), || {
                    tag(c, format!("{} capture at {:?} not generated", u.kind, (x, y)))
                });
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
                }
                if let Some(m) = s.units.get_mut(&u.id) {
                    m.moved = true;
                }
                // A capture can eliminate a player (their HQ, or their last lab): their
                // buildings go to the capturer and their units are removed.
                if a["eliminated"].is_object() {
                    let gone = num(&a["eliminated"]["playerId"]) as u32;
                    let by = num(&a["eliminated"]["eliminatedByPId"]) as u32;
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
            }
            "Fire" => {
                let ra = &a["attacker"];
                let rd = &a["defender"];
                let (aid, did) = (num(&ra["units_id"]) as u32, num(&rd["units_id"]) as u32);
                if aid == 0 || did == 0 {
                    // One side is hidden from this view (a dived sub). Take the visible side's
                    // recorded result as given.
                    r.skip("fire: a side is hidden from this view (id 0)");
                    meter_check(s, a, r, c, None);
                    // Defender "?" (Sonja's unit, game-1746729): any enemy the attacker could
                    // reach may have been hit. Their HP is unknown this turn; one gone from the
                    // next record died here.
                    if did == 0 && rd.as_str() == Some("?") {
                        if let Some(att) = s.units.get(&aid).cloned() {
                            let moved = r.last_move.is_some_and(|(id, m)| id == att.id && m);
                            let cands: Vec<UnitId> = awbw_engine::actions::attack_targets(s, &att, (att.x, att.y), moved)
                                .into_iter().filter_map(|(x, y)| s.unit_at(x, y).map(|u| u.id)).collect();
                            for id in cands {
                                r.hidden_hp.insert(id);
                                if !r.next_alive.is_empty() && !r.next_alive.contains(&id) {
                                    remove_unit(s, id);
                                }
                            }
                        }
                    }
                    for (id, rv) in [(aid, ra), (did, rd)] {
                        if id == 0 { continue; }
                        let hp = hp_display(&rv["units_hit_points"]);
                        if let Some(m) = s.units.get_mut(&id) {
                            m.hp = Hp::from_display(hp);
                            m.ammo = num(&rv["units_ammo"]) as i32;
                        }
                        if hp == 0 { remove_unit(s, id); }
                    }
                    continue;
                }
                let (Some(att), Some(def)) = (s.units.get(&aid).cloned(), s.units.get(&did).cloned()) else {
                    r.check("fire.units_known", false, || tag(c, format!("{aid} or {did} missing")));
                    continue;
                };
                let moved = r.last_move.is_some_and(|(id, m)| id == att.id && m);
                let targets = awbw_engine::actions::attack_targets(s, &att, (att.x, att.y), moved);
                r.check("legal.attack_target", targets.contains(&(def.x, def.y)), || {
                    tag(c, format!("{} ({}, ammo {}) -> {} at distance {} not generated (moved={moved})",
                        att.kind, co_of(s, att.owner), att.ammo, def.kind, dist((att.x, att.y), (def.x, def.y))))
                });
                let d = dist((att.x, att.y), (def.x, def.y));
                let ai = tables().unit(&att.kind);
                let mut maxr = ai.range[1] as u32;
                if maxr > 1 {
                    maxr = (maxr as i32 + awbw_engine::co::range_bonus(&co_of(s, att.owner), &pw(s, att.owner), &att.kind)).max(2) as u32;
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
                if std::env::var("SKIP_POWERED").is_ok() && (pw(s, att.owner) != "N" || pw(s, def.owner) != "N") {
                    r.skip("fire: SKIP_POWERED set");
                    meter_check(s, a, r, c, None);
                    if let Some(m) = s.units.get_mut(&aid) { m.hp = Hp::from_display(att_rec); m.ammo = ammo_after; }
                    if let Some(m) = s.units.get_mut(&did) { m.hp = Hp::from_display(def_rec); m.ammo = num(&rd["units_ammo"]) as i32; }
                    for id in [aid, did] {
                        if s.units.get(&id).map(|u| u.hp.hi == 0).unwrap_or(false) { remove_unit(s, id); }
                    }
                    continue;
                }
                // Sonja hides her units' HP from opponents: the record says "?".
                let def_hidden = rd["units_hit_points"].as_str() == Some("?");
                let att_hidden = ra["units_hit_points"].as_str() == Some("?");
                if def_hidden || att_hidden {
                    r.hidden_hp.insert(aid);
                    r.hidden_hp.insert(did);
                }
                // Sonja's Counter Break (super, lasting through the opponent's turn): a defender
                // that can counter strikes first; the attacker then fires with what's left
                // (game-1746729 turns 019, 025, 031).
                {
                    let di = tables().unit(&def.kind);
                    let first = co_of(s, def.owner) == "Sonja" && pw(s, def.owner) == "S"
                        && d == 1 && di.range[1] == 1 && !is_indirect(&def.kind)
                        && tables().base_damage(&def.kind, &att.kind).is_some()
                        && (def.ammo > 0 || di.second_weapon || di.ammo == 0);
                    if first {
                        let b2 = tables().base_damage(&def.kind, &att.kind).unwrap();
                        // A first strike isn't a counterattack: no x1.5 (all 7 recorded ones fit
                        // without it, none with it).
                        let cf = if att_hidden { hidden_range(s, alive_after(r, actions, i, att.id), &def, &att, b2, false) } else { attack_feasible(s, &def, &att, att_rec, b2, false) };
                        let def_ammo_after = num(&rd["units_ammo"]) as i32;
                        let want = if uses_primary(&def, &att.kind) { def.ammo - 1 } else { def.ammo };
                        // A hidden (Sonja) defender's ammo shows as 0 in the record: keep ours.
                        // A dead defender's ammo shows as 0 too.
                        let def_ammo_after = if def_hidden || (def_ammo_after == 0 && hp_display(&rd["units_hit_points"]) == 0) { want } else { def_ammo_after };
                        r.check("fire.counter_ammo", def_ammo_after == want, || {
                            tag(c, format!("{} ammo {} -> {def_ammo_after} striking first at {}", def.kind, def.ammo, att.kind))
                        });
                        let att_rec = if att_hidden { cf.map(|h| h.display_hi()).unwrap_or(0) } else { att_rec };
                        r.check("fire.counter_damage", cf.is_some(), || {
                            tag(c, format!("first strike {} {}hp (Sonja) -> {} {}hp: recorded {}hp",
                                def.kind, def.hp.display_hi(), att.kind, att.hp.display_hi(), att_rec))
                        });
                        let new_att_hp = cf.unwrap_or(Hp::from_display(att_rec));
                        let new_def_hp = if att_rec == 0 {
                            def.hp
                        } else {
                            let mut a2 = att.clone();
                            a2.hp = new_att_hp;
                            let nd = if def_hidden { hidden_range(s, alive_after(r, actions, i, def.id), &a2, &def, base, false) } else { attack_feasible(s, &a2, &def, def_rec, base, false) };
                            r.check("fire.damage", nd.is_some(), || {
                                tag(c, format!("after first strike {} {}hp -> {} {}hp (Sonja): recorded {}hp",
                                    att.kind, att_rec, def.kind, def.hp.display_hi(), def_rec))
                            });
                            nd.unwrap_or(Hp::from_display(def_rec))
                        };
                        let def_rec = if def_hidden { new_def_hp.display_hi() } else { def_rec };
                        meter_check(s, a, r, c, None);
                        if let Some(m) = s.units.get_mut(&aid) {
                            m.hp = if att_rec == 0 { Hp::exact(0) } else { new_att_hp };
                            m.ammo = ammo_after;
                            m.moved = true;
                        }
                        if let Some(m) = s.units.get_mut(&did) {
                            m.hp = if def_rec == 0 { Hp::exact(0) } else { new_def_hp };
                            m.ammo = def_ammo_after;
                        }
                        for id in [aid, did] {
                            if s.units.get(&id).map(|u| u.hp.hi == 0).unwrap_or(false) {
                                remove_unit(s, id);
                            }
                        }
                        continue;
                    }
                }
                let new_def = if def_hidden { hidden_range(s, alive_after(r, actions, i, def.id), &att, &def, base, false) } else { attack_feasible(s, &att, &def, def_rec, base, false) };
                let def_rec = if def_hidden { new_def.map(|h| h.display_hi()).unwrap_or(0) } else { def_rec };
                r.check("fire.damage", new_def.is_some(), || {
                    tag(c, format!("{} {}hp ({}) -> {} {}hp ({}) on {}: recorded {}hp",
                        att.kind, att.hp.display_hi(), co_of(s, att.owner), def.kind, def.hp.display_hi(),
                        co_of(s, def.owner), tables().tile(s.terrain_at(def.x, def.y)).name, def_rec))
                        + &format!(" [att {} on {} def {} hp {:?}->{:?}]", att_value(s, &att, s.terrain_at(att.x, att.y)),
                            tables().tile(s.terrain_at(att.x, att.y)).name, def_value(s, &def, &att.kind), att.hp, def.hp)
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
                        let cf = if att_hidden { hidden_range(s, alive_after(r, actions, i, att.id), &d2, &att, b2, true) } else { attack_feasible(s, &d2, &att, att_rec, b2, true) };
                        let att_rec = if att_hidden { cf.map(|h| h.display_hi()).unwrap_or(0) } else { att_rec };
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
                let att_rec = if att_hidden { new_att_hp.display_hi() } else { att_rec };
                if !def_hidden && !att_hidden {
                    export_fire(s, &att, &def, def_rec, att_rec, new_def.is_some());
                }
                let lost = |u: &Unit, after: u8| u.hp.display_hi().saturating_sub(after);
                let fight = (!att_hidden && !def_hidden && att.hp.display().is_some() && def.hp.display().is_some())
                    .then(|| (att.clone(), lost(&att, att_rec), def.clone(), lost(&def, def_rec)));
                meter_check(s, a, r, c, fight);
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
                // The merged unit keeps any capture in progress on this tile.
                s.units.remove(&jid);
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
                        let have0 = s.player(t.owner).map(|p| p.funds).unwrap_or(0);
                        let afford = have0 >= tables().unit(&t.kind).cost as i64 / 10;
                        let want = if afford { (before + 1).min(10) } else { before };
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
                start_turn(s, next, day);
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
            "Unhide" | "Hide" => {
                let rec = unit_from(&a["unit"]);
                match s.units.get_mut(&rec.id) {
                    Some(m) => {
                        m.dived = kind == "Hide";
                        m.fuel = rec.fuel;
                        m.moved = true;
                    }
                    None => {
                        // It was hidden from this view until now.
                        let mut u = rec;
                        u.dived = false;
                        u.moved = true;
                        s.units.insert(u.id, u);
                    }
                }
            }
            "Launch" => {
                let (sx, sy) = (num(&a["siloX"]) as u16, num(&a["siloY"]) as u16);
                let (tx, ty) = (num(&a["targetX"]) as u16, num(&a["targetY"]) as u16);
                let launcher = s.unit_at(sx, sy).cloned();
                r.check("launch.foot_on_silo", launcher.as_ref().is_some_and(|u| is_foot(&u.kind)), || {
                    tag(c, format!("silo {:?} has {:?}", (sx, sy), launcher.as_ref().map(|u| u.kind.clone())))
                });
                let silo = s.buildings.get(&(sx, sy)).map(|b| b.terrain);
                r.check("launch.loaded_silo", silo.is_some_and(|t| tables().tile(t).building.as_deref() == Some("Missile")), || {
                    tag(c, format!("silo terrain {silo:?}"))
                });
                if let Some(b) = s.buildings.get_mut(&(sx, sy)) {
                    b.terrain = 112; // Missile Silo Empty
                }
                // 3 HP to every unit within 2 tiles of the target; it can't kill (leaves 0.1).
                let dmg = (-num(&a["hp"])).max(0) as u8 * 10;
                for u in s.units.values_mut() {
                    if u.carried_by.is_none() && dist((tx, ty), (u.x, u.y)) <= 2 {
                        u.hp = Hp { lo: u.hp.lo.saturating_sub(dmg).max(1), hi: u.hp.hi.saturating_sub(dmg).max(1) };
                    }
                }
                if let Some(u) = launcher {
                    s.units.get_mut(&u.id).unwrap().moved = true;
                }
            }
            "AttackSeam" => {
                let (x, y) = (num(&a["seamX"]) as u16, num(&a["seamY"]) as u16);
                let hp = num(&a["seamHp"]) as i32;
                let aid = num(&a["attacker"]["units_id"]) as u32;
                if let Some(u) = s.units.get(&aid).cloned() {
                    let d = dist((u.x, u.y), (x, y));
                    let ri = tables().unit(&u.kind).range;
                    r.check("seam.range", d >= ri[0] as u32 && d <= ri[1] as u32, || tag(c, format!("{} at {d}", u.kind)));
                    let m = s.units.get_mut(&aid).unwrap();
                    m.ammo = num(&a["attacker"]["units_ammo"]) as i32;
                    m.moved = true;
                }
                if hp <= 0 {
                    // A broken seam stays in the building list as rubble; its capture field
                    // keeps the HP the seam had before the breaking hit.
                    if let Some(b) = s.buildings.get_mut(&(x, y)) {
                        b.terrain = if b.terrain == 113 { 115 } else { 116 };
                    }
                } else if let Some(b) = s.buildings.get_mut(&(x, y)) {
                    b.capture = hp;
                }
            }
            "Supply" => {
                // Newer replays record the APC's Supply command explicitly.
                for x in items(&a["suppliedIds"]) {
                    let id = num(x) as u32;
                    if let Some(u) = s.units.get_mut(&id) {
                        let info = tables().unit(&u.kind);
                        u.fuel = info.fuel as i32;
                        u.ammo = info.ammo as i32;
                    } else {
                        r.check("supply.unit_known", false, || tag(c, format!("supplied {id} unknown")));
                    }
                }
            }
            "Delete" => {
                // A player scrapping one of their own units.
                let id = num(&a["deletedId"]) as u32;
                let mine = s.units.get(&id).map(|u| u.owner == s.current);
                r.check("delete.own_unit", mine == Some(true), || tag(c, format!("delete {id}: {mine:?}")));
                remove_unit(s, id);
            }
            "Resign" => return Err("resign".into()),
            "Power" => apply_power(s, a, r, c),
            other => return Err(format!("unsupported action {other}")),
        }
    }
    Ok(())
}

/// NextTurn lists the units it repaired (with new HP) and the units APCs supplied.
fn check_repairs(s: &State, before: &BTreeMap<UnitId, Unit>, a: &Value, r: &mut Report, c: &Ctx) {
    // Sonja's units are listed with HP "?" whether or not they were repaired: no information.
    let hidden: std::collections::HashSet<UnitId> = items(&a["repaired"]).iter()
        .filter(|x| x["units_hit_points"].as_str() == Some("?"))
        .map(|x| num(&x["units_id"]) as u32).collect();
    let rec: BTreeMap<UnitId, u8> = items(&a["repaired"]).iter()
        .filter(|x| x["units_hit_points"].as_str() != Some("?"))
        .map(|x| (num(&x["units_id"]) as u32, hp_display(&x["units_hit_points"]))).collect();
    let next = s.current;
    for (id, u) in &s.units {
        if u.owner != next {
            continue;
        }
        let Some(b) = before.get(id) else { continue };
        if hidden.contains(id) {
            r.skip("repair: hidden HP (Sonja)");
            continue;
        }
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


fn moved_this_action(_u: &Unit) -> bool {
    false // Move and Fire are separate records; the move check covers indirect move-and-fire later.
}



/// A CO power or super power. AWBW records most of its effects on the action itself:
/// changed units (`unitReplace`), across-the-board HP changes (`hpChange`), weather, funds.
/// FIRE_CASES=path: append each recorded attack (both HPs known) as a JSON line, so other
/// engines can be scored on the same cases (tools/warsworld bench). `ours` is our verdict.
fn export_fire(s: &State, att: &Unit, def: &Unit, def_after: u8, att_after: u8, ours: bool) {
    let Ok(path) = std::env::var("FIRE_CASES") else { return };
    let side = |u: &Unit| json!({
        "kind": u.kind, "hp": [u.hp.lo, u.hp.hi], "x": u.x, "y": u.y, "ammo": u.ammo,
        "terrain": tables().tile(s.terrain_at(u.x, u.y)).name, "co": co_of(s, u.owner), "power": pw(s, u.owner),
        "towers": awbw_engine::combat::towers(s, u.owner),
    });
    let line = json!({"attacker": side(att), "defender": side(def), "weather": s.weather.to_string(),
        "def_after": def_after, "att_after": att_after, "ours_ok": ours}).to_string();
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

/// After a fire: charge the meters the engine's way and compare with the recorded values
/// (`copValues`), then take the recorded values so one miss doesn't spread.
fn meter_check(s: &mut State, a: &Value, r: &mut Report, c: &Ctx, fight: Option<(Unit, u8, Unit, u8)>) {
    let cv = &a["copValues"];
    if !cv.is_object() {
        return;
    }
    if let Some((att, al, def, dl)) = fight {
        let before: Vec<(u32, i64)> = s.players.iter().map(|p| (p.id, p.meter)).collect();
        awbw_engine::sim::charge(s, &att, al, &def, dl);
        for side in ["attacker", "defender"] {
            let pid = num(&cv[side]["playerId"]) as u32;
            let want = num(&cv[side]["copValue"]);
            let ours = s.player(pid).map(|p| p.meter).unwrap_or(-1);
            let was = before.iter().find(|b| b.0 == pid).map(|b| b.1).unwrap_or(-1);
            r.check("power.meter", ours == want, || {
                tag(c, format!("{side} {pid} ({}, power {}): meter {was} -> engine {ours}, recorded {want} [{} lost {al}, {} lost {dl}]",
                    co_of(s, pid), pw(s, pid), att.kind, def.kind))
            });
        }
    }
    for side in ["attacker", "defender"] {
        let pid = num(&cv[side]["playerId"]) as u32;
        if let Some(p) = s.player_mut(pid) {
            p.meter = num(&cv[side]["copValue"]);
        }
    }
}

fn apply_power(s: &mut State, a: &Value, r: &mut Report, c: &Ctx) {
    let pid = num(&a["playerId"]) as u32;
    let kind = a["coPower"].as_str().unwrap_or("Y").to_string();
    r.check("power.player_is_current", pid == s.current, || tag(c, format!("power by {pid}")));
    if s.player(pid).is_some() {
        let was = s.player(pid).map(|p| p.meter).unwrap_or(0);
        awbw_engine::sim::spend_power(s, pid, &kind);
        if let Some(want) = opt_num(&a["copValue"]) {
            let ours = s.player(pid).map(|p| p.meter).unwrap_or(-1);
            r.check("power.meter_after_use", ours == want, || {
                tag(c, format!("{} {kind}: meter {was} -> engine {ours}, recorded {want}", co_of(s, pid)))
            });
            s.player_mut(pid).unwrap().meter = want;
        }
    }
    // Sasha's Market Crash drains other meters: predict from the state before the record's
    // values are written, then compare.
    let mut drained = s.clone();
    let drain_modelled = co_of(s, pid) == "Sasha" && kind == "Y" && awbw_engine::sim::power_effects(&mut drained, pid, &kind);
    if drain_modelled {
        if let Value::Object(m) = &a["playerReplace"] {
            let mut bad = vec![];
            for (id, v) in m {
                if let (Ok(id), Some(cp)) = (id.parse::<u32>(), opt_num(&v["players_co_power"])) {
                    let ours = drained.player(id).map(|p| p.meter).unwrap_or(-1);
                    if ours != cp { bad.push(format!("player {id} meter engine {ours}, record {cp}")); }
                }
            }
            r.check("power.drain", bad.is_empty(), || tag(c, format!("Sasha Y: {}", bad.join("; "))));
        }
    }
    if let Value::Object(m) = &a["playerReplace"] {
        for (id, v) in m {
            if let (Ok(id), Some(cp)) = (id.parse::<u32>(), opt_num(&v["players_co_power"])) {
                if let Some(p) = s.player_mut(id) { p.meter = cp; }
            }
        }
    }
    // The engine's prediction of the instant effects, compared after the record is applied.
    let mut pred = s.clone();
    let targets: Vec<(u16, u16)> = a["missileCoords"].as_array().map(|v| v.iter().filter_map(|c| {
        Some((opt_num(&c["x"])? as u16, opt_num(&c["y"])? as u16))
    }).collect()).unwrap_or_default();
    let modelled = awbw_engine::sim::power_effects_at(&mut pred, pid, &kind, if targets.is_empty() { None } else { Some(&targets) });
    // Older records: a movement change for all the player's units ("global").
    if let Some(d) = opt_num(&a["global"]["units_movement_points"]) {
        let ids: Vec<UnitId> = s.units.values().filter(|u| u.owner == pid).map(|u| u.id).collect();
        for id in ids {
            let m = movement_range(s, &s.units[&id]) as i64 + d;
            s.move_override.insert(id, m.max(0) as u32);
        }
    }
    for u in items(&a["unitReplace"]["units"]) {
        let id = num(&u["units_id"]) as u32;
        if let Some(m) = opt_num(&u["units_movement_points"]) {
            s.move_override.insert(id, m as u32);
        }
        let Some(su) = s.units.get_mut(&id) else { continue };
        if !u["units_hit_points"].is_null() {
            su.hp = Hp::from_display(hp_display(&u["units_hit_points"]));
        }
        if let Some(v) = opt_num(&u["units_fuel"]) { su.fuel = v as i32; }
        if let Some(v) = opt_num(&u["units_ammo"]) { su.ammo = v as i32; }
        if let Some(v) = opt_num(&u["units_moved"]) { su.moved = v != 0; su.stunned = v == -1; }
    }
    for (key, sign) in [("hpGain", 1i32), ("hpLoss", -1)] {
        let h = &a["hpChange"][key];
        if !h.is_object() {
            continue;
        }
        let players: Vec<u32> = items(&h["players"]).into_iter().map(|p| num(p) as u32).collect();
        let n = (num(&h["hp"]).abs() as i32 * 10) * sign;
        let fuel = h["units_fuel"].as_f64().unwrap_or(1.0);
        // Losses skip units inside transports; heals reach them (Andy's Hyper Upgrade healed
        // an Infantry riding a Black Boat, game-1754919 turn 28).
        for u in s.units.values_mut().filter(|u| players.contains(&u.owner) && (sign > 0 || u.carried_by.is_none())) {
            let f = |v: u8| (v as i32 + n).clamp(1, 100) as u8; // a power never kills
            u.hp = Hp { lo: f(u.hp.lo), hi: f(u.hp.hi) };
            if fuel != 1.0 {
                u.fuel = (u.fuel as f64 * fuel).ceil() as i32; // Drake: 47 -> 24
            }
        }
    }
    if let Some(w) = a["weatherReplace"]["weatherCode"].as_str().and_then(|w| w.chars().next()) {
        s.weather = w;
    }
    if let Some(m) = a["playerReplace"].as_object() {
        for (id, v) in m {
            let id: u32 = id.parse().unwrap_or(0);
            if let (Some(f), Some(p)) = (opt_num(&v["players_funds"]), s.player_mut(id)) {
                p.funds = f;
            }
        }
    }
    if !modelled {
        r.skip(&format!("power effects not modelled: {} {kind}", co_of(s, pid)));
        return;
    }
    let co = co_of(s, pid);
    // Carried units aren't in the record's unit list; take the engine's fuel and ammo for
    // them (Jess's resupply reaches them: the next turn shows them full).
    let carried: Vec<UnitId> = s.units.values().filter(|u| u.carried_by.is_some()).map(|u| u.id).collect();
    for id in carried {
        if let (Some(p), Some(u)) = (pred.units.get(&id), s.units.get_mut(&id)) {
            u.fuel = p.fuel;
            u.ammo = p.ammo;
        }
    }
    let mut bad = vec![];
    for (id, u) in &s.units {
        let Some(p) = pred.units.get(id) else { continue };
        // The record shows displayed HP (1-10): compare that, not our internal estimate.
        let shown = |h: Hp| ((h.lo as i32 + 9) / 10, (h.hi as i32 + 9) / 10);
        if p.stunned != u.stunned {
            bad.push(format!("{} {id}: engine stunned {}, record {}", u.kind, p.stunned, u.stunned));
        }
        // The record's unit list leaves out carried units, though Jess's resupply reaches
        // them (the next turn shows them full): don't compare their fuel here.
        let fuel_ok = p.fuel == u.fuel || u.carried_by.is_some();
        if shown(p.hp) != shown(u.hp) || !fuel_ok {
            bad.push(format!("{} {id}: engine hp {:?} fuel {}, record hp {:?} fuel {}", u.kind, p.hp, p.fuel, u.hp, u.fuel));
        }
        let (mp, mr) = (movement_range(&pred, p), movement_range(s, u));
        if mp != mr {
            bad.push(format!("{} {id}: engine move {mp}, record {mr}", u.kind));
        }
    }
    if pred.weather != s.weather {
        bad.push(format!("weather engine {} record {}", pred.weather, s.weather));
    }
    if !bad.is_empty() {
        *r.tally.entry(format!("power effects differ: {co} {kind}")).or_default() += 1;
    }
    r.check("power.effects", bad.is_empty(), || tag(c, format!("{co} {kind}: {}", bad.iter().take(3).cloned().collect::<Vec<_>>().join("; "))));
}



/// Compare engine state after a turn with the recorded state at the start of the next turn.
fn compare(ours: &State, rec: &State, r: &mut Report, c: &Ctx) -> bool {
    let mut clean = true;
    for rp in &rec.players {
        let Some(p) = ours.player(rp.id) else { continue };
        r.check("state.power_costs", (p.cop_cost, p.scop_cost) == (rp.cop_cost, rp.scop_cost), || {
            tag(c, format!("{} costs {}/{}, recorded {}/{}", p.co, p.cop_cost, p.scop_cost, rp.cop_cost, rp.scop_cost))
        });
        r.check("state.power_meter", p.meter == rp.meter, || {
            tag(c, format!("{} meter {}, recorded {}", p.co, p.meter, rp.meter))
        });
    }
    for (id, ru) in &rec.units {
        let Some(u) = ours.units.get(id) else {
            clean &= r.check("state.unit_exists", false, || tag(c, format!("recorded {} {id} missing", ru.kind)));
            continue;
        };
        clean &= r.check("state.unit_pos", (u.x, u.y) == (ru.x, ru.y), || {
            tag(c, format!("{} {id} at {:?}, recorded {:?}", u.kind, (u.x, u.y), (ru.x, ru.y)))
        });
        let rd = ru.hp.display_hi();
        if r.hidden_hp.contains(id) {
            r.skip("compare: HP hidden by Sonja this turn");
        } else
        { clean &= r.check("state.unit_hp", rd >= u.hp.display_lo() && rd <= u.hp.display_hi(), || {
            tag(c, format!("{} {id} hp {}..{}, recorded {rd}", u.kind, u.hp.display_lo(), u.hp.display_hi()))
        }); }
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
        if !rec.units.contains_key(id) && matches!(u.kind.as_str(), "Sub" | "Stealth") && u.owner != rec.current {
            // Replays show the game from the next player's side: dived subs and hidden
            // stealths of other players aren't in the record.
            r.skip("compare: possibly dived/hidden unit absent from record");
            continue;
        }
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
        .filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with('.'))
        .collect();
    files.sort();
    let game = dir.file_name().unwrap().to_string_lossy().to_string();
    let load = |p: &Path| -> Value {
        serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    };
    let mut cur = load(&files[0]);
    for i in 0..files.len().saturating_sub(1) {
        let next = load(&files[i + 1]);
        let file = files[i].file_name().unwrap().to_string_lossy().to_string();
        let c = Ctx { game: &game, file: &file };
        let mut s = state_from(&cur);
        r.next_alive = items(&next["gameState"]["units"]).into_iter().map(|u| num(&u["units_id"]) as u32).collect();
        r.maybe_supplied.clear();
        r.hidden_hp.clear();
        r.last_move = None;
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

/// Random legal play from a turn file until someone wins or `max_days` pass, checking
/// invariants after every action. A smoke test for the simulator.

fn selfplay(file: &Path, seed: u64, max_days: u32) -> u64 {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    let mut s = state_from(&v);
    for u in s.units.values_mut() {
        u.hp = Hp::exact(u.hp.hi);
    }
    let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut next = move || { rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (rng >> 33) as u32 };
    let (mut actions, mut errors, mut violations) = (0u64, 0u64, 0u64);
    let mut legal_total = 0u64;
    let t0 = std::time::Instant::now();
    let log_dir = std::env::var("SELFPLAY_LOG_DIR").ok();
    let mut frames = vec![frame(&s, "start")];
    let (mut rt_bad, mut act_bad) = (0u64, 0u64);
    let mut rt_time = std::time::Duration::ZERO; // excluded from actions/s
    while s.day <= max_days && awbw_engine::sim::winner(&s).is_none() {
        let legal = awbw_engine::actions::legal_actions(&s);
        legal_total += legal.len() as u64;
        // Prefer acting over ending the turn, so games progress.
        let a = if legal.len() > 1 && next() % 20 != 0 { &legal[next() as usize % (legal.len() - 1)] } else { legal.last().unwrap() };
        let label = describe(&s, a);
        // The chosen action must survive the contract format.
        let rt0 = std::time::Instant::now();
        match action_from_contract(&s, &action_to_contract(a)) {
            Ok(b) if &b == a => {}
            other => { act_bad += 1; if act_bad <= 5 { eprintln!("contract round trip: {a:?} -> {other:?}"); } }
        }
        rt_time += rt0.elapsed();
        let mut roll = |lo: i32, hi: i32| lo + (next() as i32).rem_euclid(hi - lo + 1);
        if let Err(e) = awbw_engine::sim::step(&mut s, a, &mut roll) {
            errors += 1;
            if errors <= 5 { eprintln!("step error: {e} on {a:?}"); }
            let _ = awbw_engine::sim::end_turn(&mut s);
        }
        actions += 1;
        if log_dir.is_some() {
            frames.push(frame(&s, &label));
        }
        // Invariants.
        let mut seen = std::collections::HashSet::new();
        for u in s.units.values() {
            let bad = (u.carried_by.is_none() && !seen.insert((u.x, u.y))) || u.fuel < 0 || u.ammo < 0
                || u.hp.hi == 0 || u.carried_by.is_some_and(|t| !s.units.get(&t).is_some_and(|t| t.cargo.contains(&u.id)));
            if bad {
                violations += 1;
                if violations <= 5 { eprintln!("invariant broken by {a:?}: {u:?}"); }
            }
        }
        if s.players.iter().any(|p| p.funds < 0) {
            violations += 1;
        }
        // The state must survive AWBW's format.
        let rt0 = std::time::Instant::now();
        let back = state_from(&to_turn_json(&s, &v));
        if fingerprint(&back) != fingerprint(&s) {
            rt_bad += 1;
            if rt_bad <= 3 { eprintln!("state round trip after {a:?}:\n{}", diff(&fingerprint(&s), &fingerprint(&back))); }
        }
        rt_time += rt0.elapsed();
    }
    if let Some(dir) = &log_dir {
        let name = format!("{}-seed{seed}", file.file_stem().unwrap().to_string_lossy());
        let mut log = log_header(&s, &format!("random vs random, {name}"));
        log["frames"] = Value::Array(frames);
        log["winner"] = serde_json::json!(awbw_engine::sim::winner(&s));
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(Path::new(dir).join(format!("{name}.json")), log.to_string()).unwrap();
    }
    violations += rt_bad + act_bad;
    println!("selfplay seed {seed}: {rt_bad} state round-trip and {act_bad} action round-trip failures");
    println!("selfplay seed {seed}: {actions} actions, day {}, winner {:?}, {:.1} legal/position, {errors} step errors, {violations} invariant violations, {:.0} actions/s",
        s.day, awbw_engine::sim::winner(&s), legal_total as f64 / actions.max(1) as f64,
        actions as f64 / (t0.elapsed() - rt_time).as_secs_f64());
    errors + violations
}

fn diff(a: &str, b: &str) -> String {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let mut out = String::new();
    for l in &la { if !lb.contains(l) { out += &format!("  - {l}\n"); } }
    for l in &lb { if !la.contains(l) { out += &format!("  + {l}\n"); } }
    out
}

/// Parse -> write -> parse every turn file; the two parses must agree.
fn roundtrip(dirs: &[String]) -> u64 {
    let (mut n, mut bad) = (0u64, 0u64);
    for d in dirs {
        let mut files: Vec<PathBuf> = std::fs::read_dir(d).unwrap().filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "json") && !p.file_name().unwrap().to_string_lossy().starts_with('.'))
            .collect();
        files.sort();
        for f in files {
            let v: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
            if v["gameState"].is_null() { continue; }
            let a = state_from(&v);
            let b = state_from(&to_turn_json(&a, &v));
            n += 1;
            if fingerprint(&a) != fingerprint(&b) {
                bad += 1;
                if bad <= 3 { eprintln!("{}:\n{}", f.display(), diff(&fingerprint(&a), &fingerprint(&b))); }
            }
        }
    }
    println!("roundtrip: {n} turn files, {bad} differ");
    bad
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--match") {
        matchrun::run(&args[2..]);
        return;
    }
    if args.get(1).map(String::as_str) == Some("--suggest") {
        matchrun::suggest(&args[2..]);
        return;
    }
    if args.get(1).map(String::as_str) == Some("--roundtrip") {
        std::process::exit((roundtrip(&args[2..]) > 0) as i32);
    }
    if args.get(1).map(String::as_str) == Some("--selfplay") {
        let mut bad = 0;
        for seed in 1..=args.get(3).and_then(|n| n.parse().ok()).unwrap_or(3u64) {
            bad += selfplay(Path::new(&args[2]), seed, 60);
        }
        std::process::exit(if bad > 0 { 1 } else { 0 });
    }
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
    for (k, n) in &r.tally {
        println!("tally: {k} x{n}");
    }
    for (k, ex) in &r.examples {
        if only.as_deref().map(|o| k.starts_with(o)).unwrap_or(true) {
            for e in ex {
                println!("  [{k}] {e}");
            }
        }
    }
}
