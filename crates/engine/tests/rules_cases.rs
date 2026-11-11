//! Rules cases: one JSON file per rule, in tests/cases/. Each holds a position, one action, and
//! what AWBW does with it, so a rules bug report is a file that fails `cargo test`. Format and
//! how to file one: docs/rules-cases.md.
use awbw_engine::actions::{legal_actions, Action, Pos, Then};
use awbw_engine::{awbw, sim, State};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn pos(v: &Value) -> Pos {
    (v[0].as_u64().expect("[x, y]") as u16, v[1].as_u64().expect("[x, y]") as u16)
}

fn state(c: &Value, dir: &Path) -> State {
    let read = |k: &str| -> Option<Value> {
        c[k].as_str().map(|f| serde_json::from_slice(&std::fs::read(dir.join(f)).expect(f)).expect(f))
    };
    if c["state"].is_object() {
        serde_json::from_value(c["state"].clone()).expect("state")
    } else if let Some(v) = read("state_file") {
        serde_json::from_value(v).expect("state_file")
    } else if let Some(v) = read("awbw_turn_file") {
        awbw::state_from(&v)
    } else {
        panic!("a case needs state, state_file or awbw_turn_file")
    }
}

fn then(v: &Value) -> Then {
    if let Some(s) = v.as_str() {
        return match s {
            "wait" => Then::Wait,
            "capture" => Then::Capture,
            "load" => Then::Load,
            "join" => Then::Join,
            "supply" => Then::Supply,
            "hide" => Then::Hide,
            "unhide" => Then::Unhide,
            "delete" => Then::Delete,
            _ => panic!("unknown then {s}"),
        };
    }
    let (k, a) = v.as_object().and_then(|o| o.iter().next()).expect("then");
    match k.as_str() {
        "attack" => Then::Attack { target: pos(a) },
        "repair" => Then::Repair { target: pos(a) },
        "seam" => Then::AttackSeam { target: pos(a) },
        "launch" => Then::Launch { target: pos(a) },
        "unload" => Then::Unload { cargo: a["cargo"].as_u64().expect("cargo") as u32, to: pos(&a["to"]) },
        _ => panic!("unknown then {k}"),
    }
}

/// The action, plus the end tile for a move (legality ignores the path: AWBW checks the end).
fn action(s: &State, v: &Value) -> (Action, Option<Pos>) {
    if v.as_str() == Some("end_turn") {
        return (Action::EndTurn, None);
    }
    if let Some(m) = v.get("move") {
        let id = m["unit"].as_u64().expect("unit") as u32;
        let u = s.units.get(&id).unwrap_or_else(|| panic!("no unit {id}"));
        let to = if m["to"].is_array() { pos(&m["to"]) } else { (u.x, u.y) };
        let path = match m["path"].as_array() {
            Some(p) => p.iter().map(pos).collect(),
            None => awbw_engine::actions::reachable(s, u).get(&to).map(|r| r.1.clone()).unwrap_or(vec![(u.x, u.y), to]),
        };
        return (Action::Move { unit: id, path, then: then(m.get("then").unwrap_or(&Value::from("wait"))) }, Some(to));
    }
    if let Some(b) = v.get("build") {
        return (Action::Build { at: pos(&b["at"]), kind: b["kind"].as_str().expect("kind").into() }, None);
    }
    if let Some(p) = v.get("power") {
        return (Action::Power { kind: p.as_str().expect("power Y or S").into() }, None);
    }
    panic!("unknown action {v}")
}

fn is_legal(s: &State, a: &Action, to: Option<Pos>) -> bool {
    legal_actions(s).iter().any(|l| match (l, a) {
        (Action::Move { unit, path, then }, Action::Move { unit: u2, then: t2, .. }) => {
            unit == u2 && then == t2 && path.last().copied() == to
        }
        _ => l == a,
    })
}

fn check(file: &Path) -> Vec<String> {
    let c: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).expect("case json");
    let mut s = state(&c, file.parent().unwrap());
    let (a, to) = action(&s, &c["action"]);
    let e = &c["expect"];
    let mut bad = vec![];
    let legal = is_legal(&s, &a, to);
    if let Some(want) = e["legal"].as_bool() {
        if legal != want {
            bad.push(format!("legal: engine {legal}, AWBW {want}"));
        }
        if !want {
            return bad;
        }
    } else if !legal {
        bad.push("legal: engine refuses the action".into());
        return bad;
    }
    let roll = c["luck"].clone();
    let mut luck = |lo: i32, hi: i32| match roll.as_str() {
        Some("max") => hi,
        Some("min") | None => match roll.as_i64() {
            Some(n) => n as i32,
            None => lo,
        },
        Some(o) => panic!("luck is min, max or a number, not {o}"),
    };
    if let Err(err) = sim::step(&mut s, &a, &mut luck) {
        bad.push(format!("step error: {err}"));
        return bad;
    }
    for (id, want) in e["units"].as_object().into_iter().flatten() {
        let got = s.units.get(&id.parse::<u32>().expect("unit id")).map(|u| serde_json::to_value(u).unwrap());
        match (want.as_object(), got) {
            (None, None) => {}
            (None, Some(_)) => bad.push(format!("unit {id}: AWBW destroys it, engine keeps it")),
            (Some(_), None) => bad.push(format!("unit {id}: engine destroys it, AWBW keeps it")),
            (Some(w), Some(g)) => {
                for (k, wv) in w {
                    // hp: AWBW's internal 0-100; the engine carries a range, compare its top.
                    let gv = if k == "hp" { g["hp"]["hi"].clone() } else { g[k].clone() };
                    if &gv != wv {
                        bad.push(format!("unit {id} {k}: engine {gv}, AWBW {wv}"));
                    }
                }
            }
        }
    }
    for (pid, want) in e["players"].as_object().into_iter().flatten() {
        let p = s.players.iter().find(|p| p.id.to_string() == *pid).map(|p| serde_json::to_value(p).unwrap());
        for (k, wv) in want.as_object().into_iter().flatten() {
            let gv = p.as_ref().map(|p| p[k].clone()).unwrap_or(Value::Null);
            if &gv != wv {
                bad.push(format!("player {pid} {k}: engine {gv}, AWBW {wv}"));
            }
        }
    }
    bad
}

fn case_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    v.sort();
    v
}

#[test]
fn rules_cases() {
    let only = std::env::var("CASE").ok();
    let mut fails = vec![];
    let mut n = 0;
    for f in case_files() {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        if only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
            continue;
        }
        n += 1;
        for b in check(&f) {
            fails.push(format!("{name}: {b}"));
        }
    }
    assert!(n > 0, "no cases matched");
    assert!(fails.is_empty(), "{} of {n} cases disagree with AWBW:\n{}", fails.len(), fails.join("\n"));
}
