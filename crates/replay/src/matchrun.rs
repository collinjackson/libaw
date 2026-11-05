//! Match runner: our engine is the referee. Each player is an agent that gets the AWBW
//! turn-file JSON of the position (via `awbw::to_turn_json`) and answers one contract action
//! (docs/eval-harness/CONTRACT.md). The referee checks it against the legal actions, applies
//! it with seeded luck, and asks again until the game ends or hits the day limit.
//!
//!   awbw-replay --match START.json AGENT_A AGENT_B [GAMES] [MAX_DAYS] [SEED]
//!   AGENT: random | http://host:port#bot-b
//!
//! Games alternate seats. One JSON line per game goes to stdout; with SELFPLAY_LOG_DIR set,
//! a viewer log per game too.

use awbw_engine::actions::{legal_actions, Action};
use awbw_engine::awbw::*;
use awbw_engine::sim;
use awbw_engine::*;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

pub enum Agent {
    Random,
    Http { url: String, bot: String, agent: ureq::Agent },
}

impl Agent {
    pub fn parse(spec: &str) -> Agent {
        if let Some(rest) = spec.strip_prefix("http") {
            let (url, bot) = rest.split_once('#').unwrap_or((rest, "bot-a"));
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(15))).build().into();
            Agent::Http { url: format!("http{url}"), bot: bot.to_string(), agent }
        } else {
            Agent::Random
        }
    }
    pub fn name(&self) -> String {
        match self {
            Agent::Random => "random".into(),
            Agent::Http { bot, .. } => bot.clone(),
        }
    }
    /// One action for the player to move, in contract JSON.
    fn act(&self, s: &State, template: &Value, seed: u64, rng: &mut impl FnMut() -> u32) -> Result<Value, String> {
        match self {
            Agent::Random => {
                let legal = legal_actions(s);
                // Prefer acting over ending the turn, so games progress.
                let a = if legal.len() > 1 && rng() % 20 != 0 { &legal[rng() as usize % (legal.len() - 1)] } else { legal.last().unwrap() };
                Ok(action_to_contract(a))
            }
            Agent::Http { url, bot, agent } => {
                let body = json!({"player_id": s.current, "seed": seed, "bot": bot, "state": to_turn_json(s, template)});
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
    errors: u64,
    forced_ends: u64,
}

/// Play one game; `seats[i]` plays the i-th player in turn order.
pub fn play(start: &Value, seats: [&Agent; 2], seed: u64, max_days: u32, log_dir: Option<&str>, name: &str) -> Value {
    let mut s = state_from(start);
    for u in s.units.values_mut() {
        u.hp = Hp::exact(u.hp.hi);
    }
    let order: Vec<PlayerId> = s.players.iter().map(|p| p.id).collect();
    let seat_of = |p: PlayerId| order.iter().position(|&q| q == p).unwrap_or(0).min(1);
    let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut next = move || { rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (rng >> 33) as u32 };
    let mut st = [Stats::default(), Stats::default()];
    let mut frames = vec![crate::frame(&s, "start")];
    let mut this_turn = 0u32;
    let mut first_error = None;
    while s.day <= max_days && sim::winner(&s).is_none() {
        let i = seat_of(s.current);
        let req_seed = seed ^ (st[0].actions + st[1].actions).wrapping_mul(0x9E3779B97F4A7C15);
        let reply = seats[i].act(&s, start, req_seed, &mut next);
        let a = match reply.and_then(|v| action_from_contract(&s, &v).map_err(|e| format!("illegal: {e}: {v}"))) {
            Ok(a) => a,
            Err(e) => {
                if e.starts_with("illegal") { st[i].illegal += 1 } else { st[i].errors += 1 }
                if first_error.is_none() { first_error = Some(format!("{}: {e}", seats[i].name())); }
                Action::EndTurn // penalty: the turn ends
            }
        };
        this_turn += 1;
        let a = if this_turn > 300 { st[i].forced_ends += 1; Action::EndTurn } else { a };
        if a == Action::EndTurn { this_turn = 0; }
        let label = crate::describe(&s, &a);
        let mut roll = |lo: i32, hi: i32| lo + (next() as i32).rem_euclid(hi - lo + 1);
        if let Err(e) = sim::step(&mut s, &a, &mut roll) {
            st[i].errors += 1;
            if first_error.is_none() { first_error = Some(format!("referee: {e} on {a:?}")); }
            let _ = sim::end_turn(&mut s);
            this_turn = 0;
        }
        st[i].actions += 1;
        if log_dir.is_some() { frames.push(crate::frame(&s, &label)); }
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
        let mut log = crate::log_header(&s, &format!("{} vs {}, {name}", seats[0].name(), seats[1].name()));
        log["frames"] = Value::Array(frames);
        log["winner"] = json!(win_team);
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(Path::new(dir).join(format!("{name}.json")), log.to_string()).unwrap();
    }
    json!({
        "game": name, "seats": [seats[0].name(), seats[1].name()], "seed": seed,
        "winner": winner_seat.map(|i| seats[i].name()), "days": s.day.min(max_days),
        "worth": [worth(0), worth(1)],
        "actions": [st[0].actions, st[1].actions], "illegal": [st[0].illegal, st[1].illegal],
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
        let name = format!("{stem}-{}-vs-{}-s{}", seats[0].name(), seats[1].name(), seed0 + g);
        let r = play(&start, seats, seed0 + g, max_days, log_dir.as_deref(), &name);
        println!("{r}");
    }
}
