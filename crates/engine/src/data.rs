//! Static game data, generated into `data/*.json` by `tools/gen_data.py`.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
pub struct TerrainInfo {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub building: Option<String>,
    pub defense: u8,
    /// Movement cost by movement type letter (F B T W A S L P). Missing = impassable.
    pub r#move: HashMap<String, u8>,
}

#[derive(Debug, Deserialize)]
pub struct UnitInfo {
    pub id: u32,
    pub cost: u32,
    pub r#move: u8,
    pub move_type: String,
    pub fuel: u8,
    pub fuel_per_turn: u8,
    pub ammo: u8,
    pub vision: u8,
    pub range: [u8; 2],
    pub second_weapon: bool,
}

pub struct Tables {
    pub terrain: HashMap<u16, TerrainInfo>,
    pub units: HashMap<String, UnitInfo>,
    /// Base damage, attacker -> defender, percent. Missing = can't attack.
    pub damage: HashMap<String, HashMap<String, u8>>,
    pub cos: HashMap<String, serde_json::Value>,
}

pub fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let terrain: HashMap<String, TerrainInfo> =
            serde_json::from_str(include_str!("../../../data/terrain.json")).unwrap();
        Tables {
            terrain: terrain.into_iter().map(|(k, v)| (k.parse().unwrap(), v)).collect(),
            units: serde_json::from_str(include_str!("../../../data/units.json")).unwrap(),
            damage: serde_json::from_str(include_str!("../../../data/damage.json")).unwrap(),
            cos: serde_json::from_str(include_str!("../../../data/cos.json")).unwrap(),
        }
    })
}

impl Tables {
    pub fn unit(&self, name: &str) -> &UnitInfo {
        self.units.get(name).unwrap_or_else(|| panic!("unknown unit {name}"))
    }
    pub fn tile(&self, id: u16) -> &TerrainInfo {
        self.terrain.get(&id).unwrap_or_else(|| panic!("unknown terrain {id}"))
    }
    pub fn base_damage(&self, att: &str, def: &str) -> Option<u8> {
        self.damage.get(att).and_then(|m| m.get(def)).copied()
    }
}
