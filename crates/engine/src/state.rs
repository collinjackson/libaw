//! Game state. Coordinates are (x, y) with x across, y down, as AWBW uses them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type PlayerId = u32;
pub type UnitId = u32;

/// Hit points on AWBW's internal 0..=100 scale. Replays only show the displayed value
/// (ceil(hp / 10)), so the engine carries a range that the true value is known to lie in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hp {
    pub lo: u8,
    pub hi: u8,
}

impl Hp {
    pub fn exact(v: u8) -> Hp {
        Hp { lo: v, hi: v }
    }
    /// Every internal value that displays as `d` (1..=10).
    pub fn from_display(d: u8) -> Hp {
        if d == 0 {
            return Hp { lo: 0, hi: 0 };
        }
        Hp { lo: (d - 1) * 10 + 1, hi: d * 10 }
    }
    pub fn display_hi(&self) -> u8 {
        self.hi.div_ceil(10)
    }
    pub fn display_lo(&self) -> u8 {
        self.lo.div_ceil(10)
    }
    /// The displayed value, when it's certain.
    pub fn display(&self) -> Option<u8> {
        (self.display_lo() == self.display_hi()).then(|| self.display_hi())
    }
    pub fn intersect(&self, o: Hp) -> Option<Hp> {
        let lo = self.lo.max(o.lo);
        let hi = self.hi.min(o.hi);
        (lo <= hi).then_some(Hp { lo, hi })
    }
    pub fn add(&self, n: u8) -> Hp {
        Hp { lo: (self.lo + n).min(100), hi: (self.hi + n).min(100) }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub owner: PlayerId,
    pub kind: String,
    pub x: u16,
    pub y: u16,
    pub hp: Hp,
    pub fuel: i32,
    pub ammo: i32,
    pub moved: bool,
    /// Stunned by Von Bolt's Ex Machina: can't act in its owner's next turn (AWBW records
    /// units_moved = -1).
    #[serde(default)]
    pub stunned: bool,
    /// The transport carrying this unit, if any.
    pub carried_by: Option<UnitId>,
    pub cargo: Vec<UnitId>,
    pub dived: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Building {
    pub terrain: u16,
    pub owner: Option<PlayerId>,
    /// Capture points left, 20 when untouched.
    pub capture: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub team: String,
    pub co: String,
    pub funds: i64,
    pub eliminated: bool,
    pub order: u32,
    /// "N" none, "Y" CO power, "S" super power (AWBW's players_co_power_on).
    pub power: String,
    /// CO power meter in AWBW units (funds of damage x 10), and the costs of each power.
    #[serde(default)]
    pub meter: i64,
    #[serde(default)]
    pub cop_cost: i64,
    #[serde(default)]
    pub scop_cost: i64,
    /// Extra attack percent while a power lasts, fixed when it starts (Colin's super:
    /// 3 per 1000 funds).
    #[serde(default)]
    pub power_attack: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub width: u16,
    pub height: u16,
    /// Terrain id per tile, row-major (y * width + x). Buildings carry their own id here too.
    pub terrain: Vec<u16>,
    #[serde(with = "pos_map")]
    pub buildings: BTreeMap<(u16, u16), Building>,
    pub units: BTreeMap<UnitId, Unit>,
    /// In turn order.
    pub players: Vec<Player>,
    pub current: PlayerId,
    pub day: u32,
    /// Funds per income property (AWBW default 1000; a per-game setting).
    pub funds_per_property: i64,
    /// Weather for this turn: 'C' clear, 'R' rain, 'S' snow.
    pub weather: char,
    /// Movement points set by a CO power this turn (AWBW records them on the Power action).
    pub move_override: BTreeMap<UnitId, u32>,
    /// Whether players may activate CO powers. Off by default ("No CO" play: Carson's bots
    /// don't use powers yet); the replay checker applies recorded powers regardless.
    #[serde(default)]
    pub powers: bool,
    /// Fog of war: players see only what `fog::vision` shows them; a move into a hidden
    /// enemy unit stops short (a trap).
    #[serde(default)]
    pub fog: bool,
}

impl State {
    pub fn terrain_at(&self, x: u16, y: u16) -> u16 {
        if let Some(b) = self.buildings.get(&(x, y)) {
            return b.terrain;
        }
        self.terrain[y as usize * self.width as usize + x as usize]
    }
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }
    /// The visible (not carried) unit standing on a tile.
    pub fn unit_at(&self, x: u16, y: u16) -> Option<&Unit> {
        self.units.values().find(|u| u.x == x && u.y == y && u.carried_by.is_none())
    }
    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.iter().find(|p| p.id == id)
    }
    pub fn player_mut(&mut self, id: PlayerId) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.id == id)
    }
    pub fn team_of(&self, id: PlayerId) -> String {
        self.player(id).map(|p| p.team.clone()).unwrap_or_default()
    }
    pub fn allied(&self, a: PlayerId, b: PlayerId) -> bool {
        a == b || self.team_of(a) == self.team_of(b)
    }
}

/// JSON maps need string keys: buildings travel as a list of [x, y, building].
mod pos_map {
    use super::Building;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;
    pub fn serialize<S: Serializer>(m: &BTreeMap<(u16, u16), Building>, s: S) -> Result<S::Ok, S::Error> {
        m.iter().map(|(&(x, y), b)| (x, y, b)).collect::<Vec<_>>().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<(u16, u16), Building>, D::Error> {
        Ok(Vec::<(u16, u16, Building)>::deserialize(d)?.into_iter().map(|(x, y, b)| ((x, y), b)).collect())
    }
}
