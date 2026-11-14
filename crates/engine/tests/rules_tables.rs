//! docs/rules/tables.md is generated from the engine's own data tables, so it can't drift from
//! what the engine plays. This test fails when the file is out of date; regenerate it with
//!   UPDATE_DOCS=1 cargo test -p libaw --test rules_tables
use libaw::data::tables;
use std::collections::BTreeMap;
use std::fmt::Write;

const MOVE_TYPES: [(&str, &str); 8] = [("F", "Foot"), ("B", "Boots"), ("T", "Treads"), ("W", "Tires"),
    ("A", "Air"), ("S", "Sea"), ("L", "Lander"), ("P", "Pipe")];

/// Short column names for the damage tables, so they fit a page.
fn short(name: &str) -> &str {
    match name {
        "Infantry" => "Inf", "Md.Tank" => "Md.T", "Artillery" => "Arty", "Rocket" => "Rkt", "Anti-Air" => "AA",
        "Missile" => "Msl", "Neotank" => "Neo", "Piperunner" => "Pipe", "Mega Tank" => "Mega", "B-Copter" => "B-Cop",
        "T-Copter" => "T-Cop", "Black Bomb" => "B.Bomb", "Battleship" => "B.Ship", "Black Boat" => "B.Boat",
        other => other,
    }
}

fn render() -> String {
    let t = tables();
    let mut s = String::new();
    s += "# Rules tables\n\n";
    s += "Generated from libaw's data tables (`data/*.json`) by `crates/engine/tests/rules_tables.rs`; do not edit by\n";
    s += "hand. These are the values the engine plays with. Where AWBW's published numbers come from, and how they\n";
    s += "were checked against recorded games, is in `docs/sources.md` and `docs/awbw-rule-findings.md`.\n\n";

    // Units.
    s += "## Units\n\nCost in funds; move in movement points; fuel per day (`burn`) is what an air or sea unit uses at\n";
    s += "the start of each of its turns; ammo 0 means it has only a machine gun or no weapon; range is min-max\n";
    s += "(1-1 is direct fire; none: it can't attack).\n\n";
    s += "Move types: F foot (Infantry), B boots (Mech), T treads, W tires, A air, S sea, L lander, P pipe.\n\n";
    s += "| Unit | Cost | Move | Type | Fuel | Burn | Ammo | Vision | Range | 2nd gun |\n";
    s += "|---|---|---|---|---|---|---|---|---|---|\n";
    let mut units: Vec<_> = t.units.iter().collect();
    units.sort_by_key(|(_, u)| u.id);
    for (name, u) in &units {
        let range = if u.range[1] == 0 { "none".to_string() } else { format!("{}-{}", u.range[0], u.range[1]) };
        writeln!(s, "| {name} | {} | {} | {} | {} | {} | {} | {} | {range} | {} |", u.cost, u.r#move, u.move_type, u.fuel,
            u.fuel_per_turn, u.ammo, u.vision, if u.second_weapon { "yes" } else { "no" }).unwrap();
    }

    // Damage chart.
    s += "\n## Base damage\n\nPercent of a full-HP defender's HP that a full-HP attacker deals before CO, terrain and luck\n";
    s += "(the damage formula is in `rules::damage_f64`). Rows attack, columns defend; blank means it can't attack that\n";
    s += "unit.\n\n";
    s += "Split by what is defending so each table fits a page. Column names are shortened: Inf Infantry, Md.T\n";
    s += "Md.Tank, Arty Artillery, Rkt Rocket, AA Anti-Air, Msl Missile, Neo Neotank, Pipe Piperunner, Mega Mega Tank,\n";
    s += "B-Cop/T-Cop B-/T-Copter, B.Bomb Black Bomb, B.Ship Battleship, B.Boat Black Boat.\n";
    let names: Vec<&str> = units.iter().map(|(n, _)| n.as_str()).collect();
    let domain = |n: &str| match t.units[n].move_type.as_str() { "A" => "air", "S" | "L" => "sea", _ => "ground" };
    for (title, d) in [("Against ground units", "ground"), ("Against air units", "air"), ("Against sea units", "sea")] {
        let cols: Vec<&str> = names.iter().copied().filter(|n| domain(n) == d).collect();
        write!(s, "\n### {title}\n\n| Attacker |").unwrap();
        for n in &cols { write!(s, " {} |", short(n)).unwrap(); }
        s += "\n|---|";
        for _ in &cols { s += "---|"; }
        s += "\n";
        for a in &names {
            if cols.iter().all(|dd| t.base_damage(a, dd).is_none()) {
                continue;
            }
            write!(s, "| {a} |").unwrap();
            for dd in &cols {
                match t.base_damage(a, dd) { Some(v) => write!(s, " {v} |").unwrap(), None => s += " |" }
            }
            s += "\n";
        }
    }

    // Terrain, clear weather.
    s += "\n## Terrain\n\nDefence stars and movement cost by move type in clear weather (blank: impassable). Rain and snow\n";
    s += "costs are in `rules::move_cost_in`. Tiles that share all values are listed together. Foot is Infantry,\n";
    s += "Boots is Mech; Lander covers Landers and Black Boats.\n\n";
    s += "| Terrain | Stars |";
    for (_, m) in MOVE_TYPES { write!(s, " {m} |").unwrap(); }
    s += "\n|---|---|";
    for _ in MOVE_TYPES { s += "---|"; }
    s += "\n";
    let mut groups: BTreeMap<(String, u8, String), Vec<String>> = BTreeMap::new();
    for info in t.terrain.values() {
        let costs: String = MOVE_TYPES.iter().map(|(k, _)| info.r#move.get(*k).map_or("-".into(), |c| c.to_string())).collect::<Vec<_>>().join(",");
        let family = info.building.clone().unwrap_or_else(|| if info.name.contains("Pipe") { "Pipe".into() } else { info.kind.clone() });
        groups.entry((family, info.defense, costs)).or_default().push(info.name.clone());
    }
    for ((family, stars, costs), mut tiles) in groups {
        tiles.sort();
        tiles.dedup();
        let label = if tiles.len() > 3 { format!("{family} ({} tiles)", tiles.len()) } else { tiles.join(", ") };
        write!(s, "| {label} | {stars} |").unwrap();
        for c in costs.split(',') { write!(s, " {} |", if c == "-" { "" } else { c }).unwrap(); }
        s += "\n";
    }

    // CO power costs.
    s += "\n## CO power costs\n\nStars for the CO power and the super power, before the per-use increase (each use adds 20% of\n";
    s += "the base, up to 10 uses). Day-to-day abilities and power effects are in `co.rs` and `docs/rules/`.\n\n";
    s += "| CO | Power | Super |\n|---|---|---|\n";
    let mut cos: Vec<_> = t.cos.iter().collect();
    cos.sort_by_key(|(n, _)| n.as_str());
    for (name, c) in cos {
        let st = |k: &str| c[k]["PowerStars"].as_u64().map_or("-".into(), |v| v.to_string());
        writeln!(s, "| {name} | {} | {} |", st("cop"), st("scop")).unwrap();
    }
    s
}

#[test]
fn rules_tables_are_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/rules/tables.md");
    let want = render();
    if std::env::var("UPDATE_DOCS").is_ok() {
        std::fs::write(path, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(path).unwrap_or_default();
    assert!(have == want, "docs/rules/tables.md is out of date: UPDATE_DOCS=1 cargo test -p libaw --test rules_tables");
}
