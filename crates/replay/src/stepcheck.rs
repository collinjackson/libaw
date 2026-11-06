//! Step parity: replay each recorded turn through the simulator's own `sim::step` (the
//! code bots play with), not the checker's hand-applied record, and compare the state it
//! reaches with the next turn file. Luck is searched per attack so the recorded HP come out.
//!
//! `awbw-replay --step [--examples N] GAME_DIR...`. Fog games are skipped (cargo and
//! funds are hidden there), as are turns with records the engine has no action for.

use crate::{compare, print_report, tag, Ctx, Report};
use awbw_engine::actions::{Action, Pos, Then};
use awbw_engine::awbw::{items, num, state_from, unit_from};
use awbw_engine::state::*;
use serde_json::Value;
use std::path::Path;

pub fn run(args: &[String]) {
    let mut r = Report { max_examples: 3, ..Default::default() };
    let mut dirs = vec![];
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--examples" {
            r.max_examples = args[i + 1].parse().unwrap();
            i += 2;
        } else {
            dirs.push(args[i].clone());
            i += 1;
        }
    }
    for d in &dirs {
        game(Path::new(d), &mut r);
    }
    print_report(&r, None);
}

fn game(dir: &Path, r: &mut Report) {
    let mut files: Vec<_> = std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("turn-") && n.to_string_lossy().ends_with(".json")))
        .collect()).unwrap_or_default();
    files.sort();
    let name = dir.file_name().unwrap().to_string_lossy().to_string();
    let load = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    let Some(first) = files.first() else { return };
    let mut cur = load(first);
    if cur["gameState"]["fog_array"].is_array() {
        r.skip("game: fog");
        return;
    }
    for i in 0..files.len().saturating_sub(1) {
        let next = load(&files[i + 1]);
        let file = files[i].file_name().unwrap().to_string_lossy().to_string();
        let c = Ctx { game: &name, file: &file };
        let actions: Vec<Value> = items(&cur["actions"]).into_iter().cloned().collect();
        match turn(&cur, &next, &actions, r, &c) {
            Ok(s) => {
                r.turns += 1;
                let mut rec = state_from(&next);
                rec.weather = s.weather; // weather is drawn by the server
                if compare(&s, &rec, r, &c) {
                    r.turns_clean += 1;
                }
            }
            Err(why) => r.skip(&format!("turn: {why}")),
        }
        cur = next;
    }
}

fn pos(v: &Value, x: &str, y: &str) -> Pos {
    (num(&v[x]) as u16, num(&v[y]) as u16)
}

fn path_of(a: &Value) -> Vec<Pos> {
    items(&a["path"]).into_iter().map(|p| pos(p, "x", "y")).collect()
}

/// The follow-up a record action gives the unit that just moved (or stood still).
fn then_of(s: &State, a: &Value, mover: Option<(UnitId, Pos)>) -> Option<(UnitId, Then)> {
    let id = |v: &Value| num(v) as UnitId;
    Some(match a["action"].as_str()? {
        "Fire" if a["attacker"].is_object() && a["defender"].is_object() => {
            (id(&a["attacker"]["units_id"]), Then::Attack { target: pos(&a["defender"], "units_x", "units_y") })
        }
        "AttackSeam" => {
            (id(&a["attacker"]["units_id"]), Then::AttackSeam { target: pos(a, "seamX", "seamY") })
        }
        "Capt" => {
            let at = pos(&a["buildingInfo"], "buildings_x", "buildings_y");
            match mover {
                Some((m, end)) if end == at => (m, Then::Capture),
                _ => (s.unit_at(at.0, at.1)?.id, Then::Capture),
            }
        }
        "Load" => (id(&a["loadedId"]), Then::Load),
        "Delete" => (id(&a["deletedId"]), Then::Delete),
        "Launch" => {
            let at = pos(a, "siloX", "siloY");
            let who = match mover {
                Some((m, end)) if end == at => m,
                _ => s.unit_at(at.0, at.1)?.id,
            };
            (who, Then::Launch { target: pos(a, "targetX", "targetY") })
        }
        // Newer records give `unitId`, older (team) ones the whole `unit`.
        "Hide" => (id(&a["unitId"]).max(id(&a["unit"]["units_id"])), Then::Hide),
        "Unhide" => (id(&a["unitId"]).max(id(&a["unit"]["units_id"])), Then::Unhide),
        "Join" => (id(&a["joinId"]), Then::Join),
        "Unload" => (id(&a["transportId"]), Then::Unload {
            cargo: id(&a["unloadedUnit"]["units_id"]),
            to: pos(&a["unloadedUnit"], "units_x", "units_y"),
        }),
        "Repair" => {
            let t = s.units.get(&id(&a["repairedUnit"]["units_id"]))?;
            (id(&a["unitId"]), Then::Repair { target: (t.x, t.y) })
        }
        // Supply names only the supplied units; the supplier is the APC that just moved.
        // Standing still, it's the player's APC next to the first supplied unit.
        "Supply" => match mover {
            Some((m, _)) => (m, Then::Supply),
            None => {
                let first = s.units.get(&(num(items(&a["suppliedIds"]).first()?) as UnitId))?;
                let apc = s.units.values().find(|o| o.kind == "APC" && o.owner == first.owner && o.carried_by.is_none()
                    && (o.x as i32 - first.x as i32).abs() + (o.y as i32 - first.y as i32).abs() == 1)?;
                (apc.id, Then::Supply)
            }
        },
        _ => return None,
    })
}

/// Run one turn through `sim::step`. Err names why the turn can't be replayed this way.
fn turn(cur: &Value, next: &Value, actions: &[Value], r: &mut Report, c: &Ctx) -> Result<State, String> {
    if !actions.iter().any(|a| a["action"] == "NextTurn") {
        return Err("no NextTurn".into());
    }
    if crate::unrecorded_mover(cur, next, actions).is_some() {
        return Err("export lost actions".into());
    }
    let mut s = state_from(cur);
    s.powers = true;
    s.seams = true;
    s.dives = true;
    s.deletes = true;
    let mut i = 0;
    while i < actions.len() {
        let a = &actions[i];
        let kind = a["action"].as_str().unwrap_or("?");
        let act = match kind {
            "Move" => {
                let rec = unit_from(&a["unit"]);
                let path = path_of(a);
                let path = if path.is_empty() { vec![(rec.x, rec.y)] } else { path };
                let end = *path.last().unwrap();
                let then = match actions.get(i + 1).and_then(|n| then_of(&s, n, Some((rec.id, end)))) {
                    Some((who, t)) if who == rec.id => {
                        i += 1;
                        t
                    }
                    _ => Then::Wait,
                };
                Action::Move { unit: rec.id, path, then }
            }
            "Build" => {
                let nu = unit_from(&a["newUnit"]);
                // The engine numbers new units itself; take the record's id so later actions find it.
                let before: Vec<UnitId> = s.units.keys().copied().collect();
                step(&mut s, &Action::Build { at: (nu.x, nu.y), kind: nu.kind.clone() }, a, r, c)?;
                if let Some(&new) = s.units.keys().find(|k| !before.contains(k)) {
                    let mut u = s.units.remove(&new).unwrap();
                    u.id = nu.id;
                    s.units.insert(nu.id, u);
                }
                i += 1;
                continue;
            }
            "Power" => {
                let coords: Vec<Pos> = items(&a["missileCoords"]).into_iter().map(|m| pos(m, "x", "y")).collect();
                if !coords.is_empty() {
                    // The server aims missiles: land them where the record says.
                    let kind = a["coPower"].as_str().unwrap_or("");
                    let res = awbw_engine::sim::use_power(&mut s, kind, Some(&coords));
                    r.check("step.ok", res.is_ok(), || tag(c, format!("power {kind} at {coords:?}: {:?}", res.as_ref().err())));
                    res.map_err(|e| format!("step error: {e}"))?;
                    i += 1;
                    continue;
                }
                Action::Power { kind: a["coPower"].as_str().unwrap_or("").to_string() }
            }
            "NextTurn" => {
                step(&mut s, &Action::EndTurn, a, r, c)?;
                return Ok(s);
            }
            "Resign" => return Err(format!("{kind}: no engine action")),
            _ => match then_of(&s, a, None) {
                // A follow-up with no Move before it: the unit acted where it stood (or a
                // transport dropped its second cargo).
                Some((who, then)) => {
                    let u = s.units.get(&who).ok_or_else(|| format!("{kind}: unit {who} unknown"))?;
                    if let Then::Unload { .. } = then {
                        // A drop apart from the transport's own move (see sim::step).
                        let key = if u.moved { "drop apart from move: transport had moved" } else { "drop apart from move: transport unmoved" };
                        *r.tally.entry(key.to_string()).or_default() += 1;
                        let a = Action::Move { unit: who, path: vec![(u.x, u.y)], then: then.clone() };
                        r.check("legal.drop", awbw_engine::actions::legal_actions(&s).contains(&a), || tag(c, format!("{a:?} not generated")));
                    }
                    Action::Move { unit: who, path: vec![(u.x, u.y)], then }
                }
                None => return Err(format!("{kind}: not mapped")),
            },
        };
        // `i` now points at the record that carries the result (the Fire after its Move).
        step(&mut s, &act, &actions[i], r, c)?;
        i += 1;
    }
    Err("ran out of actions".into())
}

/// One `sim::step`. For an attack, try luck rolls until the recorded HP come out.
fn step(s: &mut State, act: &Action, rec: &Value, r: &mut Report, c: &Ctx) -> Result<(), String> {
    if let Action::Move { unit, path, then: then @ Then::Launch { .. } } = act {
        let end = path.last().copied();
        // A blast that hits nobody is the same move wherever it lands: match the dump target.
        let hits_none = |t: Pos| !s.units.values().any(|o| o.carried_by.is_none()
            && (o.x as i32 - t.0 as i32).abs() + (o.y as i32 - t.1 as i32).abs() <= 2);
        let Then::Launch { target } = then else { unreachable!() };
        let offered = awbw_engine::actions::legal_actions(s).iter().any(|a| matches!(a,
            Action::Move { unit: u, path: p, then: Then::Launch { target: t } } if u == unit && p.last().copied() == end
                && (t == target || (hits_none(*t) && hits_none(*target)))));
        r.check("legal.launch", offered, || tag(c, format!("{act:?} not generated")));
    }
    if let Action::Move { then: Then::Delete, .. } = act {
        r.check("legal.delete", awbw_engine::actions::legal_actions(s).contains(act), || tag(c, format!("{act:?} not generated")));
    }
    let fire = match act {
        Action::Move { then: Then::Attack { .. }, .. } => rec.clone(),
        Action::Move { .. } => Value::Null,
        _ => Value::Null,
    };
    let fire = if fire["attacker"].is_object() { Some(fire) } else { None };
    if let Some(f) = fire {
        let (aid, did) = (num(&f["attacker"]["units_id"]) as UnitId, num(&f["defender"]["units_id"]) as UnitId);
        let (ah, dh) = (num(&f["attacker"]["units_hit_points"]), num(&f["defender"]["units_hit_points"]));
        let shown = |s: &State, id: UnitId| s.units.get(&id).and_then(|u| u.hp.display()).map(|d| d as i64).unwrap_or(0);
        // Records show displayed HP only; try the HP each side could really have, top first.
        let range = |id: UnitId| s.units.get(&id).map(|u| (u.hp.lo.max(1)..=u.hp.hi).rev().collect::<Vec<u8>>()).unwrap_or_default();
        let (ar, dr) = (range(aid), range(did));
        // Keep every outcome that fits: the units' HP become the range over all of them.
        let mut found: Option<State> = None;
        let (mut alo, mut ahi, mut dlo, mut dhi) = (u8::MAX, 0u8, u8::MAX, 0u8);
        for &ahp in &ar {
            for &dhp in &dr {
                // Each roll spans the range the engine asks for (Rachel's power: 0..39).
                let (span1, span2) = (std::cell::Cell::new(9), std::cell::Cell::new(9));
                let mut k1 = 0;
                while k1 <= span1.get() {
                    let mut k2 = 0;
                    while k2 <= span2.get() {
                        let mut t = s.clone();
                        if let Some(u) = t.units.get_mut(&aid) { u.hp = Hp::exact(ahp); }
                        if let Some(u) = t.units.get_mut(&did) { u.hp = Hp::exact(dhp); }
                        let mut n = 0;
                        let mut luck = |lo: i32, hi: i32| {
                            n += 1;
                            if n == 1 { span1.set(hi - lo); } else { span2.set(hi - lo); }
                            (lo + if n == 1 { k1 } else { k2 }).min(hi)
                        };
                        let ok = awbw_engine::sim::step(&mut t, act, &mut luck).is_ok();
                        let calls = n;
                        if ok && shown(&t, aid) == ah && shown(&t, did) == dh {
                            if let Some(u) = t.units.get(&aid) { alo = alo.min(u.hp.lo); ahi = ahi.max(u.hp.hi); }
                            if let Some(u) = t.units.get(&did) { dlo = dlo.min(u.hp.lo); dhi = dhi.max(u.hp.hi); }
                            found.get_or_insert(t);
                        }
                        if calls < 2 { break; } // no counter: the second roll changes nothing
                        k2 += 1;
                    }
                    k1 += 1;
                }
            }
        }
        if let Some(mut t) = found {
            if let Some(u) = t.units.get_mut(&aid) { u.hp = Hp { lo: alo, hi: ahi }; }
            if let Some(u) = t.units.get_mut(&did) { u.hp = Hp { lo: dlo, hi: dhi }; }
            *s = t;
            r.check("step.fire", true, String::new);
            return Ok(());
        }
        r.check("step.fire", false, || tag(c, format!("no luck roll gives attacker {ah} / defender {dh}")));
        // Go on from the recorded result so one miss doesn't spread through the turn.
        let mut mid = |lo: i32, hi: i32| (lo + hi) / 2;
        let res = awbw_engine::sim::step(s, act, &mut mid);
        r.check("step.ok", res.is_ok(), || tag(c, format!("{act:?}: {}", res.as_ref().err().cloned().unwrap_or_default())));
        res.map_err(|e| format!("step error: {e}"))?;
        for (id, d) in [(aid, ah), (did, dh)] {
            if d <= 0 {
                if let Some(u) = s.units.remove(&id) { for cid in u.cargo { s.units.remove(&cid); } }
            } else if let Some(u) = s.units.get_mut(&id) {
                u.hp = Hp::from_display(d as u8);
            } else {
                return Err("forced fire result: unit gone".into());
            }
        }
        return Ok(());
    }
    let mut mid = |lo: i32, hi: i32| (lo + hi) / 2;
    let res = awbw_engine::sim::step(s, act, &mut mid);
    r.check("step.ok", res.is_ok(), || tag(c, format!("{act:?}: {}", res.as_ref().err().cloned().unwrap_or_default())));
    res.map_err(|e| format!("step error: {e}"))
}
