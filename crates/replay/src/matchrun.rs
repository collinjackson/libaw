//! Match runner: our engine is the referee. Each player is an agent that gets the AWBW
//! turn-file JSON of the position (via `awbw::to_turn_json`) and answers one contract action
//! (docs/eval-harness/CONTRACT.md). The referee checks it against the legal actions, applies
//! it with seeded luck, and asks again until the game ends or hits the day limit.
//!
//!   awbw-replay --match START.json AGENT_A AGENT_B [GAMES] [MAX_DAYS] [SEED]
//!   AGENT: random | greedy | http://host:port#bot-b
//!
//! Games alternate seats. One JSON line per game goes to stdout; with SELFPLAY_LOG_DIR set,
//! a viewer log per game too; with POSITIONS_LOG=FILE.jsonl, value-model training rows (each
//! turn start's army by kind, funds, properties, meter per side, plus the game's winner).

use awbw_engine::actions::Action;
use awbw_engine::awbw::*;
use awbw_engine::sim;
use awbw_engine::*;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

pub enum Agent {
    Random,
    Greedy,
    Http { url: String, bot: String, agent: ureq::Agent },
}

impl Agent {
    pub fn parse(spec: &str) -> Agent {
        if let Some(rest) = spec.strip_prefix("http") {
            let (url, bot) = rest.split_once('#').unwrap_or((rest, "bot-a"));
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(15))).build().into();
            Agent::Http { url: format!("http{url}"), bot: bot.to_string(), agent }
        } else if spec == "greedy" {
            Agent::Greedy
        } else {
            Agent::Random
        }
    }
    pub fn name(&self) -> String {
        match self {
            Agent::Random => "random".into(),
            Agent::Greedy => "greedy".into(),
            Agent::Http { bot, .. } => bot.clone(),
        }
    }
    /// One action for the player to move, in contract JSON.
    /// In fog, the agent sees only `fog::player_view` (the caller passes that as `s`).
    fn act(&self, s: &State, template: &Value, seed: u64, game: &str, rng: &mut impl FnMut() -> u32) -> Result<Value, String> {
        match self {
            Agent::Random => Ok(action_to_contract(&awbw_engine::baseline::random(s, rng))),
            Agent::Greedy => {
                // The reply carries the top candidates (an optional "why" any agent may send).
                let ranked = awbw_engine::baseline::ranked(s);
                let pick = ranked.first().map(|(_, a)| a.clone()).unwrap_or(Action::EndTurn);
                let mut v = action_to_contract(&pick);
                v["why"] = json!(ranked.iter().take(5).map(|(g, a)| json!({
                    "label": awbw_engine::view::describe(s, a), "score": (g * 10.0).round() / 10.0,
                })).collect::<Vec<_>>());
                Ok(v)
            }
            Agent::Http { url, bot, agent } => {
                let body = json!({"player_id": s.current, "seed": seed, "game": game, "bot": bot, "powers": s.powers, "banned": s.banned, "state": to_turn_json(s, template)});
                let mut r = agent.post(format!("{url}/act")).send_json(&body).map_err(|e| format!("http: {e}"))?;
                r.body_mut().read_json::<Value>().map_err(|e| format!("bad json: {e}"))
            }
        }
    }
}

#[derive(Default)]
struct Stats {
    actions: u64,
    illegal: u64,
    normalized: u64,
    errors: u64,
    forced_ends: u64,
}

/// Power stars of a CO's power and super power, from data/cos.json.
fn stars(co: &str) -> Option<(i64, i64)> {
    let c = data::tables().cos.get(co)?;
    let n = |k: &str| c[k]["PowerStars"].as_i64();
    Some((n("cop")?, n("scop")?))
}

/// Give every player `co`. The power costs keep the start's value per star (AWBW: 9000 funds a star
/// at the start), scaled to the new CO's star counts.
pub fn set_co(s: &mut State, co: &str) -> Result<(), String> {
    set_co_players(&mut s.players, co)
}

fn set_co_players(players: &mut [Player], co: &str) -> Result<(), String> {
    let (cop, scop) = stars(co).ok_or_else(|| format!("unknown CO {co:?}"))?;
    for p in players.iter_mut() {
        let star = stars(&p.co).filter(|&(_, sc)| sc > 0 && p.scop_cost > 0).map(|(_, sc)| p.scop_cost / sc).unwrap_or(9000);
        p.co = co.to_string();
        p.cop_cost = cop * star;
        p.scop_cost = scop * star;
    }
    Ok(())
}

/// Play one game; `seats[i]` plays the i-th player in turn order.
pub fn play(start: &Value, seats: [&Agent; 2], seed: u64, max_days: u32, log_dir: Option<&str>, name: &str) -> Value {
    let mut s = state_from(start);
    // MATCH_FOG=1 plays the map with fog of war.
    if std::env::var("MATCH_FOG").is_ok() {
        s.fog = true;
    }
    s.partial = false; // the referee sees everything
    // Bots may disband (AWBW "delete"); a bot that never sends it is unaffected.
    s.deletes = true;
    // MATCH_POWERS=1: CO powers on (default No CO, as against Carson's bots). Bots are told
    // through the request's "powers" field.
    // MATCH_BANS: comma list of unbuildable units (default Stealth,Black Bomb as on AWBW;
    // empty allows everything). Bots are told through the request's "banned" field.
    if let Ok(v) = std::env::var("MATCH_BANS") {
        s.banned = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect();
    }
    if std::env::var("MATCH_POWERS").is_ok() {
        s.powers = true;
    }
    // MATCH_CO=Name: every player plays that CO (a mirror match, for self-play on one CO).
    if let Ok(v) = std::env::var("MATCH_CO") {
        set_co(&mut s, v.trim()).unwrap_or_else(|e| panic!("MATCH_CO: {e}"));
    }
    // MATCH_CAPTURE_LIMIT: "league" (70% of the map's income properties, as AWBW's league
    // games) or a number of properties. Default: off, as in AWBW's custom games.
    if let Ok(v) = std::env::var("MATCH_CAPTURE_LIMIT") {
        s.capture_limit = if v == "league" { Some(awbw_engine::rules::league_capture_limit(&s)) } else { v.parse().ok() };
    }
    // MATCH_UNIT_LIMIT: "league" (AWBW's 50) or a number of units. Default: no limit.
    if let Ok(v) = std::env::var("MATCH_UNIT_LIMIT") {
        s.unit_limit = if v == "league" { Some(awbw_engine::rules::LEAGUE_UNIT_LIMIT) } else { v.parse().ok() };
    }
    for u in s.units.values_mut() {
        u.hp = Hp::exact(u.hp.hi);
    }
    let order: Vec<PlayerId> = s.players.iter().map(|p| p.id).collect();
    let seat_of = |p: PlayerId| order.iter().position(|&q| q == p).unwrap_or(0).min(1);
    let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut next = move || { rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (rng >> 33) as u32 };
    let mut st = [Stats::default(), Stats::default()];
    let mut frames = vec![awbw_engine::view::frame(&s, "start")];
    let mut this_turn = 0u32;
    let mut first_error = None;
    // Training positions for the value model (POSITIONS_LOG=FILE.jsonl): one per turn start.
    let pos_log = std::env::var("POSITIONS_LOG").ok();
    let mut positions: Vec<Value> = vec![];
    let snap = |s: &State| -> Value {
        let side = |seat: usize| {
            let ids: Vec<PlayerId> = order.iter().copied().filter(|&p| seat_of(p) == seat).collect();
            let mut army = serde_json::Map::new();
            for u in s.units.values().filter(|u| ids.contains(&u.owner)) {
                let v = data::tables().unit(&u.kind).cost as f64 * u.hp.hi as f64 / 100.0;
                let e = army.entry(u.kind.clone()).or_insert(json!(0.0));
                *e = json!(e.as_f64().unwrap_or(0.0) + v);
            }
            let p = s.players.iter().find(|p| ids.contains(&p.id));
            json!({
                "army": army,
                "units": s.units.values().filter(|u| ids.contains(&u.owner)).count(),
                "funds": p.map_or(0, |p| p.funds),
                "props": s.buildings.values().filter(|b| b.owner.is_some_and(|o| ids.contains(&o))).count(),
                "meter": p.map_or(0.0, |p| p.meter as f64 / (p.scop_cost.max(1)) as f64),
                "co": p.map_or(String::new(), |p| p.co.clone()),
            })
        };
        json!({"day": s.day, "to_move": seat_of(s.current), "sides": [side(0), side(1)]})
    };
    if pos_log.is_some() { positions.push(snap(&s)); }
    // MATCH_TURNS=DIR: each turn start as an AWBW-style turn file, DIR/<game>/turn-NNN-Dd-pidP.json
    // (what tools/mistakes.py and the replay tools read), so our own matches can be analysed
    // like recorded games.
    let turns_dir = std::env::var("MATCH_TURNS").ok().map(|d| format!("{d}/{name}"));
    let mut turn_no = 0u32;
    let write_turn = |s: &State, n: u32| {
        if let Some(d) = &turns_dir {
            let _ = std::fs::create_dir_all(d);
            let f = format!("{d}/turn-{n:03}-D{}-pid{}.json", s.day, s.current);
            let _ = std::fs::write(f, serde_json::to_vec(&to_turn_json(s, start)).unwrap_or_default());
        }
    };
    if let Some(d) = &turns_dir {
        // The turn files keep the recorded game's usernames: seats.json says which agent played
        // which player id.
        let seats_by_pid: serde_json::Map<String, Value> = s.players.iter()
            .map(|p| (p.id.to_string(), json!(seats[seat_of(p.id)].name()))).collect();
        let _ = std::fs::create_dir_all(d);
        let _ = std::fs::write(format!("{d}/seats.json"), serde_json::to_vec_pretty(&seats_by_pid).unwrap_or_default());
    }
    write_turn(&s, turn_no);
    while s.day <= max_days && sim::winner(&s).is_none() {
        let i = seat_of(s.current);
        let req_seed = seed ^ (st[0].actions + st[1].actions).wrapping_mul(0x9E3779B97F4A7C15);
        // Fog: the agent plans on what its player can see; its reply is read against that
        // view and applied to the real state (a path may run into a hidden unit: a trap).
        let view = awbw_engine::fog::player_view(&s, s.current);
        let reply = seats[i].act(&view, start, req_seed, name, &mut next);
        let why = reply.as_ref().ok().map(|v| v["why"].clone()).filter(|w| w.is_array());
        // A Supply with nobody to supply does what Wait does: take it as Wait (counted),
        // rather than end the bot's turn over a no-op.
        let reply = reply.map(|v| {
            if v["then"]["type"] == "supply" && action_from_contract(&view, &v).is_err() {
                let mut w = v.clone();
                w["then"] = json!({"type": "wait"});
                if action_from_contract(&view, &w).is_ok() {
                    st[i].normalized += 1;
                    return w;
                }
            }
            v
        });
        let a = match reply.and_then(|v| action_from_contract(&view, &v).map_err(|e| format!("illegal: {e}: {v}"))) {
            Ok(a) => a,
            Err(e) => {
                if e.starts_with("illegal") { st[i].illegal += 1 } else { st[i].errors += 1 }
                if first_error.is_none() { first_error = Some(format!("{}: {e}", seats[i].name())); }
                Action::EndTurn // penalty: the turn ends
            }
        };
        this_turn += 1;
        let a = if this_turn > 300 { st[i].forced_ends += 1; Action::EndTurn } else { a };
        let ended = a == Action::EndTurn;
        if ended { this_turn = 0; }
        let label = awbw_engine::view::describe(&s, &a);
        let mut roll = |lo: i32, hi: i32| awbw_engine::co::roll_luck(lo, hi, &mut next);
        if let Err(e) = sim::step(&mut s, &a, &mut roll) {
            st[i].errors += 1;
            if first_error.is_none() { first_error = Some(format!("referee: {e} on {a:?}")); }
            let _ = sim::end_turn(&mut s);
            this_turn = 0;
        }
        st[i].actions += 1;
        if pos_log.is_some() && ended && sim::winner(&s).is_none() { positions.push(snap(&s)); }
        // MATCH_DUMP=DIR: the full engine state at each turn start (DIR/<game>-d<day>-p<player>.json),
        // to replay one decision in a bot offline.
        if ended && sim::winner(&s).is_none() {
            turn_no += 1;
            write_turn(&s, turn_no);
            if let Ok(dir) = std::env::var("MATCH_DUMP") {
                let f = format!("{dir}/{name}-d{:02}-p{}.json", s.day, s.current);
                let _ = std::fs::write(f, serde_json::to_vec(&s).unwrap_or_default());
            }
        }
        if log_dir.is_some() {
            let mut f = awbw_engine::view::frame(&s, &label);
            f["h"] = json!(awbw_engine::view::state_hash(&s));
            if let Some(w) = &why { f["why"] = w.clone(); }
            frames.push(f);
        }
    }
    let win_team = sim::winner(&s);
    let winner_seat = win_team.as_ref().and_then(|t| s.players.iter().find(|p| &p.team == t)).map(|p| seat_of(p.id));
    // At the day limit, a tiebreak by army value + funds + properties (x1000).
    let worth = |seat: usize| -> i64 {
        let ids: Vec<PlayerId> = order.iter().copied().filter(|&p| seat_of(p) == seat).collect();
        let army: i64 = s.units.values().filter(|u| ids.contains(&u.owner))
            .map(|u| data::tables().unit(&u.kind).cost as i64 * u.hp.hi as i64 / 100).sum();
        let funds: i64 = s.players.iter().filter(|p| ids.contains(&p.id)).map(|p| p.funds).sum();
        let props = s.buildings.values().filter(|b| b.owner.is_some_and(|o| ids.contains(&o))).count() as i64;
        army + funds + props * 1000
    };
    if let Some(dir) = log_dir {
        let mut log = awbw_engine::view::log_header(&s, &format!("{} vs {}, {name}", seats[0].name(), seats[1].name()));
        log["frames"] = Value::Array(frames);
        log["winner"] = json!(win_team);
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(Path::new(dir).join(format!("{name}.json")), log.to_string()).unwrap();
    }
    if let Some(f) = &pos_log {
        // Label: who won outright, else the day-limit tiebreak by worth (marked).
        let (w0, w1) = (worth(0), worth(1));
        let label = winner_seat.or(if w0 != w1 { Some(if w0 > w1 { 0 } else { 1 }) } else { None });
        if let Some(l) = label {
            use std::io::Write;
            // One write per game: parallel ladder jobs append to the same file, and per-line
            // writes interleaved (a corrupt row on 2026-11-05).
            let mut buf = String::new();
            for mut p in positions {
                p["game"] = json!(name);
                p["winner"] = json!(l);
                p["decisive"] = json!(winner_seat.is_some());
                buf.push_str(&p.to_string());
                buf.push('\n');
            }
            let mut out = std::fs::OpenOptions::new().create(true).append(true).open(f).expect("positions log");
            out.write_all(buf.as_bytes()).unwrap();
        }
    }
    json!({
        "game": name, "seats": [seats[0].name(), seats[1].name()], "seed": seed, "fog": s.fog,
        "winner": winner_seat.map(|i| seats[i].name()), "days": s.day.min(max_days),
        "worth": [worth(0), worth(1)],
        "actions": [st[0].actions, st[1].actions], "illegal": [st[0].illegal, st[1].illegal],
        "normalized": [st[0].normalized, st[1].normalized],
        "errors": [st[0].errors, st[1].errors], "forced_ends": [st[0].forced_ends, st[1].forced_ends],
        "first_error": first_error,
    })
}

pub fn run(args: &[String]) {
    let file = Path::new(&args[0]);
    let start: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    let (a, b) = (Agent::parse(&args[1]), Agent::parse(&args[2]));
    let games: u64 = args.get(3).and_then(|x| x.parse().ok()).unwrap_or(2);
    let max_days: u32 = args.get(4).and_then(|x| x.parse().ok()).unwrap_or(30);
    let seed0: u64 = args.get(5).and_then(|x| x.parse().ok()).unwrap_or(1);
    let log_dir = std::env::var("SELFPLAY_LOG_DIR").ok();
    let stem = file.parent().and_then(|d| d.file_name()).map(|d| d.to_string_lossy().to_string())
        .unwrap_or_else(|| file.file_stem().unwrap().to_string_lossy().to_string());
    for g in 0..games {
        let seats = if g % 2 == 0 { [&a, &b] } else { [&b, &a] };
        let fog = if std::env::var("MATCH_FOG").is_ok() { "-fog" } else { "" };
        let name = format!("{stem}-{}-vs-{}-s{}{fog}", seats[0].name(), seats[1].name(), seed0 + g);
        let r = play(&start, seats, seed0 + g, max_days, log_dir.as_deref(), &name);
        println!("{r}");
    }
}

/// One full turn for the player to move, played by `agent` from a turn file with average
/// luck (so the line isn't flattered or punished by rolls). Returns the moves, described,
/// and the position at the start of the next player's turn as a turn file, so commentary
/// can compare it with what the player actually did.
///
///   awbw-replay --suggest TURN.json AGENT [OUT.json]
pub fn suggest(args: &[String]) {
    let start: Value = serde_json::from_str(&std::fs::read_to_string(&args[0]).expect("turn file")).expect("json");
    let agent = Agent::parse(&args[1]);
    let mut s = state_from(&start);
    // MATCH_UNIT_LIMIT: "league" (AWBW's 50) or a number of units. Default: no limit.
    if let Ok(v) = std::env::var("MATCH_UNIT_LIMIT") {
        s.unit_limit = if v == "league" { Some(awbw_engine::rules::LEAGUE_UNIT_LIMIT) } else { v.parse().ok() };
    }
    for u in s.units.values_mut() {
        u.hp = Hp::exact(u.hp.hi);
    }
    let me = s.current;
    let mut rng = 0x5eed_u64;
    let mut next = move || { rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (rng >> 33) as u32 };
    let (mut moves, mut illegal, mut first_error) = (vec![], 0, None::<String>);
    for n in 0..300u64 {
        if s.current != me || sim::winner(&s).is_some() { break; }
        let a = match agent.act(&s, &start, 0x5eed ^ n, "contract-test", &mut next).and_then(|v| action_from_contract(&s, &v).map_err(|e| format!("illegal: {e}"))) {
            Ok(a) => a,
            Err(e) => { illegal += 1; first_error.get_or_insert(e); Action::EndTurn }
        };
        let label = awbw_engine::view::describe(&s, &a);
        let mut avg = |lo: i32, hi: i32| (lo + hi) / 2;
        if let Err(e) = sim::step(&mut s, &a, &mut avg) {
            first_error.get_or_insert(format!("referee: {e}"));
            let _ = sim::end_turn(&mut s);
            break;
        }
        if a != Action::EndTurn { moves.push(label); }
    }
    let out = json!({"agent": agent.name(), "moves": moves, "illegal": illegal, "error": first_error,
                     "after": to_turn_json(&s, &start)});
    match args.get(2) {
        Some(p) => std::fs::write(p, out.to_string()).expect("write"),
        None => println!("{out}"),
    }
}


#[cfg(test)]
mod co_tests {
    use super::*;

    #[test]
    fn match_co_sets_every_player_and_rescales_power_costs() {
        let mut s: Vec<Player> = vec![];
        let mut p = Player { id: 1, team: "1".into(), co: "Kanbei".into(), funds: 0, eliminated: false, order: 0, power: "N".into(),
                             meter: 0, cop_cost: 36000, scop_cost: 63000, power_attack: 0 };
        s.push(p.clone());
        p.id = 2; p.co = "Nobody".into(); p.cop_cost = 0; p.scop_cost = 0;
        s.push(p);
        set_co_players(&mut s, "Adder").unwrap();
        assert!(s.iter().all(|p| p.co == "Adder"));
        assert_eq!((s[0].cop_cost, s[0].scop_cost), (18000, 45000)); // 9000 a star kept from Kanbei's 7
        assert_eq!((s[1].cop_cost, s[1].scop_cost), (18000, 45000)); // unknown CO: 9000 a star
        assert!(set_co_players(&mut s, "Nobody").is_err());
    }
}
