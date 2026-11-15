//! Turns data/units.json, terrain.json and damage.json into typed Rust tables (enums and
//! constant arrays) at build time, so the engine parses no JSON and hashes no names at run time.
//! cos.json becomes the `Co` enum and typed `CoInfo` tables the same way.

use serde_json::Value;
use std::fmt::Write;
use std::{env, fs, path::PathBuf};

const MOVE_TYPES: [&str; 8] = ["F", "B", "T", "W", "A", "S", "L", "P"];

fn ident(name: &str) -> String {
    name.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

fn names_enum(out: &mut String, doc: &str, ty: &str, names: &[String]) {
    writeln!(out, "/// {doc}").unwrap();
    writeln!(out, "#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]\n#[repr(u8)]\npub enum {ty} {{").unwrap();
    for n in names {
        writeln!(out, "    {},", ident(n)).unwrap();
    }
    writeln!(out, "}}\nimpl {ty} {{").unwrap();
    writeln!(out, "    pub const COUNT: usize = {};", names.len()).unwrap();
    let all: Vec<String> = names.iter().map(|n| format!("{ty}::{}", ident(n))).collect();
    writeln!(out, "    pub const ALL: [{ty}; {}] = [{}];", names.len(), all.join(", ")).unwrap();
    let quoted: Vec<String> = names.iter().map(|n| format!("{n:?}")).collect();
    writeln!(out, "    const NAMES: [&'static str; {}] = [{}];", names.len(), quoted.join(", ")).unwrap();
    writeln!(out, "    /// The name AWBW and the JSON formats use.\n    pub fn name(self) -> &'static str {{ Self::NAMES[self as usize] }}").unwrap();
    writeln!(out, "    pub fn from_name(s: &str) -> Option<Self> {{\n        match s {{").unwrap();
    for n in names {
        writeln!(out, "            {n:?} => Some({ty}::{}),", ident(n)).unwrap();
    }
    writeln!(out, "            _ => None,\n        }}\n    }}").unwrap();
    writeln!(out, "    pub fn from_index(i: u8) -> Option<Self> {{ Self::ALL.get(i as usize).copied() }}\n}}").unwrap();
    writeln!(out, "named_enum!({ty});\n").unwrap();
}

fn main() {
    let data = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../data");
    let read = |f: &str| -> Value {
        let p = data.join(f);
        println!("cargo:rerun-if-changed={}", p.display());
        serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap()
    };
    let units = read("units.json");
    let terrain = read("terrain.json");
    let damage = read("damage.json");
    let cos = read("cos.json");
    let cos = cos.as_object().unwrap();
    let units = units.as_object().unwrap();
    let terrain = terrain.as_object().unwrap();

    let unit_names: Vec<String> = units.keys().cloned().collect();
    let mut out = String::new();
    names_enum(&mut out, "A unit type.", "UnitType", &unit_names);
    let mt: Vec<String> = MOVE_TYPES.iter().map(|s| s.to_string()).collect();
    names_enum(&mut out, "A movement type: Foot, Boots (mech), Treads, Wheels, Air, Sea, Lander, Pipe.", "MoveType", &mt);

    let mut kinds: Vec<String> = terrain.values().map(|t| t["kind"].as_str().unwrap().to_string()).collect();
    kinds.sort();
    kinds.dedup();
    names_enum(&mut out, "The broad class of a terrain tile.", "TerrainKind", &kinds);
    let mut buildings: Vec<String> = terrain.values().filter_map(|t| t["building"].as_str().map(str::to_string)).collect();
    buildings.sort();
    buildings.dedup();
    names_enum(&mut out, "What a building tile is.", "BuildingKind", &buildings);

    // Units, in UnitType order.
    writeln!(out, "pub static UNITS: [UnitInfo; {}] = [", unit_names.len()).unwrap();
    for n in &unit_names {
        let u = &units[n];
        let r = u["range"].as_array().unwrap();
        writeln!(
            out,
            "    UnitInfo {{ id: {}, cost: {}, r#move: {}, move_type: MoveType::{}, fuel: {}, fuel_per_turn: {}, ammo: {}, vision: {}, range: [{}, {}], second_weapon: {} }},",
            u["id"], u["cost"], u["move"], u["move_type"].as_str().unwrap(), u["fuel"], u["fuel_per_turn"],
            u["ammo"], u["vision"], r[0], r[1], u["second_weapon"]
        )
        .unwrap();
    }
    writeln!(out, "];\n").unwrap();

    // Base damage, attacker x defender; None = can't attack.
    writeln!(out, "pub static DAMAGE: [[Option<u8>; {n}]; {n}] = [", n = unit_names.len()).unwrap();
    for a in &unit_names {
        let row: Vec<String> = unit_names
            .iter()
            .map(|d| match damage.get(a).and_then(|m| m.get(d)) {
                Some(v) => format!("Some({v})"),
                None => "None".into(),
            })
            .collect();
        writeln!(out, "    [{}],", row.join(", ")).unwrap();
    }
    writeln!(out, "];\n").unwrap();

    // Terrain: a dense list plus an index from AWBW terrain id.
    let mut ids: Vec<u16> = terrain.keys().map(|k| k.parse().unwrap()).collect();
    ids.sort();
    let max = *ids.last().unwrap() as usize;
    writeln!(out, "pub static TERRAIN: [TerrainInfo; {}] = [", ids.len()).unwrap();
    for id in &ids {
        let t = &terrain[&id.to_string()];
        let mv: Vec<String> = MOVE_TYPES
            .iter()
            .map(|m| match t["move"].get(*m) {
                Some(c) => format!("Some({c})"),
                None => "None".into(),
            })
            .collect();
        let building = match t["building"].as_str() {
            Some(b) => format!("Some(BuildingKind::{})", ident(b)),
            None => "None".into(),
        };
        writeln!(
            out,
            "    TerrainInfo {{ id: {id}, name: {:?}, kind: TerrainKind::{}, building: {building}, defense: {}, r#move: [{}] }},",
            t["name"].as_str().unwrap(), ident(t["kind"].as_str().unwrap()), t["defense"], mv.join(", ")
        )
        .unwrap();
    }
    writeln!(out, "];\n").unwrap();
    let mut index = vec![u16::MAX; max + 1];
    for (i, id) in ids.iter().enumerate() {
        index[*id as usize] = i as u16;
    }
    let idx: Vec<String> = index.iter().map(|v| v.to_string()).collect();
    writeln!(out, "static TERRAIN_INDEX: [u16; {}] = [{}];", max + 1, idx.join(", ")).unwrap();

    // COs: one CoInfo per CO, in Co order; each phase's boosts in a static slice.
    let co_names: Vec<String> = cos.keys().cloned().collect();
    names_enum(&mut out, "A commanding officer.", "Co", &co_names);
    let hundredths = |v: &Value| -> i64 { (v.as_f64().unwrap() * 100.0).round() as i64 };
    let mut infos = vec![];
    for (ci, n) in co_names.iter().enumerate() {
        let c = &cos[n];
        let mut phases = vec![];
        for ph in ["d2d", "cop", "scop"] {
            let d = &c[ph];
            let known = ["PowerIncreases", "PowerStars", "LuckRange", "UnitPriceMultiplier", "CounterAttackPower",
                "PropertyFundIncrease", "AirFuelUsageDecrease", "HiddenHP", "SightIncrease", "SeeIntoHiddenTiles",
                "AttackFirst", "MoveCostPerTile", "WeatherWithNoMovementAffect", "WeatherWithAdditionalMovementAffect"];
            for k in d.as_object().unwrap().keys() {
                assert!(known.contains(&k.as_str()), "cos.json: unknown field {n}.{ph}.{k}");
            }
            let mut boosts = vec![];
            for e in d["PowerIncreases"].as_array().into_iter().flatten() {
                let mut mask: u32 = 0;
                for u in e["AffectedUnits"].as_array().unwrap() {
                    let u = u.as_str().unwrap();
                    if u == "all" {
                        mask = u32::MAX;
                    } else {
                        let i = unit_names.iter().position(|x| x == u).unwrap_or_else(|| panic!("cos.json: unknown unit {u}"));
                        mask |= 1 << i;
                    }
                }
                let terrain = match e["ConditionalTerrain"].as_str() {
                    Some(t) => {
                        assert!(kinds.iter().any(|k| k == t), "cos.json: unknown terrain kind {t}");
                        format!("Some(TerrainKind::{})", ident(t))
                    }
                    None => "None".into(),
                };
                let f = |k: &str| e.get(k).map_or(0, hundredths);
                boosts.push(format!(
                    "Boost {{ units: {mask:#x}, terrain: {terrain}, attack: {}, defense: {}, range: {} }}",
                    f("PowerIncrease"), f("DefenseIncrease"), f("RangeIncrease")
                ));
            }
            let opt = |v: &Value, conv: &dyn Fn(&Value) -> String| if v.is_null() { "None".to_string() } else { format!("Some({})", conv(v)) };
            let luck = opt(&d["LuckRange"], &|l| format!("({}, {})", l["x"].as_i64().unwrap_or(0), l["y"].as_i64().unwrap_or(9)));
            let pct = |v: &Value| hundredths(v).to_string();
            let weather = |k: &str| opt(&d[k], &|w| format!("{:?}", w.as_str().unwrap().chars().next().unwrap()));
            writeln!(
                out,
                "static CO_{ci}_{}: [Boost; {}] = [{}];",
                ph.to_uppercase(), boosts.len(), boosts.join(", ")
            ).unwrap();
            let phu = ph.to_uppercase();
            phases.push(format!(
                "Phase {{ stars: {}, boosts: &CO_{ci}_{phu}, luck: {luck}, price_pct: {}, counter_pct: {}, property_fund_increase: {}, \
                 air_fuel_decrease: {}, hidden_hp: {}, sight_increase: {}, see_hidden: {}, attack_first: {}, move_cost_per_tile: {}, \
                 weather_no_move_effect: {}, weather_extra_move_effect: {} }}",
                opt(&d["PowerStars"], &|v| v.to_string()), opt(&d["UnitPriceMultiplier"], &pct), opt(&d["CounterAttackPower"], &pct),
                d["PropertyFundIncrease"].as_i64().unwrap_or(0), d["AirFuelUsageDecrease"].as_i64().unwrap_or(0),
                d["HiddenHP"].as_bool().unwrap_or(false), d["SightIncrease"].as_i64().unwrap_or(0),
                d["SeeIntoHiddenTiles"].as_bool().unwrap_or(false), d["AttackFirst"].as_bool().unwrap_or(false),
                opt(&d["MoveCostPerTile"], &|v| v.to_string()),
                weather("WeatherWithNoMovementAffect"), weather("WeatherWithAdditionalMovementAffect"),
            ));
        }
        infos.push(format!("    CoInfo {{ id: {}, d2d: {}, cop: {}, scop: {} }},", c["id"], phases[0], phases[1], phases[2]));
    }
    writeln!(out, "pub static COS: [CoInfo; {}] = [\n{}\n];", co_names.len(), infos.join("\n")).unwrap();

    let dest = PathBuf::from(env::var("OUT_DIR").unwrap()).join("tables.rs");
    fs::write(dest, out).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}
