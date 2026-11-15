//! Static game data. `data/*.json` (from `tools/gen_data.py`) becomes typed enums and constant
//! tables at build time (build.rs), so looking a unit, a tile or a damage value up is an array
//! index, with no JSON parse or name hashing at run time. CO data (`Co::info`) is typed the same way.

#[allow(unused_imports)]
use crate::prelude::*;

/// Serde, Display and comparison with names for the generated name enums. JSON (and any other
/// human-readable format) carries the name, as AWBW does; binary formats carry the index.
macro_rules! named_enum {
    ($ty:ident) => {
        impl core::fmt::Display for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.write_str(self.name()) }
        }
        impl core::str::FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                $ty::from_name(s).ok_or_else(|| format!("unknown {} {s:?}", stringify!($ty)))
            }
        }
        impl PartialEq<str> for $ty {
            fn eq(&self, o: &str) -> bool { self.name() == o }
        }
        impl PartialEq<&str> for $ty {
            fn eq(&self, o: &&str) -> bool { self.name() == *o }
        }
        impl PartialEq<$ty> for &str {
            fn eq(&self, o: &$ty) -> bool { *self == o.name() }
        }
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                if s.is_human_readable() { s.serialize_str(self.name()) } else { s.serialize_u8(*self as u8) }
            }
        }
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                use serde::de::Error;
                if d.is_human_readable() {
                    let s = <alloc::borrow::Cow<'de, str>>::deserialize(d)?;
                    $ty::from_name(&s).ok_or_else(|| D::Error::custom(format!("unknown {} {s:?}", stringify!($ty))))
                } else {
                    let i = u8::deserialize(d)?;
                    $ty::from_index(i).ok_or_else(|| D::Error::custom(format!("bad {} index {i}", stringify!($ty))))
                }
            }
        }
    };
}

include!(concat!(env!("OUT_DIR"), "/tables.rs"));

#[derive(Debug)]
pub struct TerrainInfo {
    /// AWBW terrain id.
    pub id: u16,
    pub name: &'static str,
    pub kind: TerrainKind,
    pub building: Option<BuildingKind>,
    pub defense: u8,
    /// Movement cost by movement type (`MoveType as usize`). None = impassable.
    pub r#move: [Option<u8>; MoveType::COUNT],
}

impl TerrainInfo {
    pub fn move_cost(&self, m: MoveType) -> Option<u8> {
        self.r#move[m as usize]
    }
}

#[derive(Debug)]
pub struct UnitInfo {
    pub id: u32,
    pub cost: u32,
    pub r#move: u8,
    pub move_type: MoveType,
    pub fuel: u8,
    pub fuel_per_turn: u8,
    pub ammo: u8,
    pub vision: u8,
    pub range: [u8; 2],
    pub second_weapon: bool,
}

impl UnitType {
    pub fn info(self) -> &'static UnitInfo {
        &UNITS[self as usize]
    }
}

/// For tests and callers that hold names: panics on a name that is not a unit type.
impl From<&str> for UnitType {
    fn from(s: &str) -> Self { s.unit_type() }
}

/// A unit type given as the enum or by name, for lookups that accept either. A name that is not
/// a unit type panics, as the name-keyed tables did.
pub trait AsUnitType {
    fn unit_type(&self) -> UnitType;
}
impl AsUnitType for UnitType {
    fn unit_type(&self) -> UnitType { *self }
}
impl AsUnitType for str {
    fn unit_type(&self) -> UnitType { UnitType::from_name(self).unwrap_or_else(|| panic!("unknown unit {self}")) }
}
impl AsUnitType for String {
    fn unit_type(&self) -> UnitType { self.as_str().unit_type() }
}
impl<T: AsUnitType + ?Sized> AsUnitType for &T {
    fn unit_type(&self) -> UnitType { (**self).unit_type() }
}

/// A CO's stat boost for some unit types, maybe only on one kind of terrain. Values are in
/// hundredths (attack +20% = 20; range +1 = 100), as data/cos.json's decimals convert exactly.
#[derive(Debug)]
pub struct Boost {
    /// Bit `UnitType as u8` set for each affected type (all bits for "all").
    pub units: u32,
    pub terrain: Option<TerrainKind>,
    pub attack: i16,
    pub defense: i16,
    pub range: i16,
}

impl Boost {
    pub fn affects(&self, u: UnitType) -> bool {
        self.units & (1 << u as u32) != 0
    }
}

/// One of a CO's three tables: day to day, CO power, super power.
#[derive(Debug)]
pub struct Phase {
    pub stars: Option<u8>,
    pub boosts: &'static [Boost],
    /// Luck roll range (low, high) in percent points, replacing the default 0..=9.
    pub luck: Option<(i8, i8)>,
    pub price_pct: Option<u16>,
    pub counter_pct: Option<u16>,
    pub property_fund_increase: i16,
    pub air_fuel_decrease: u8,
    pub hidden_hp: bool,
    pub sight_increase: u8,
    pub see_hidden: bool,
    pub attack_first: bool,
    pub move_cost_per_tile: Option<u8>,
    /// Weather (first letter) that doesn't slow this CO, or slows them more.
    pub weather_no_move_effect: Option<char>,
    pub weather_extra_move_effect: Option<char>,
}

#[derive(Debug)]
pub struct CoInfo {
    pub id: u32,
    pub d2d: Phase,
    pub cop: Phase,
    pub scop: Phase,
}

impl Co {
    pub fn info(self) -> &'static CoInfo {
        &COS[self as usize]
    }
}

/// A CO given as the enum or by name. Unlike units, an unknown name is None (no CO abilities),
/// as the name-keyed table gave.
pub trait AsCo {
    fn as_co(&self) -> Option<Co>;
}
impl AsCo for Co {
    fn as_co(&self) -> Option<Co> { Some(*self) }
}
impl AsCo for Option<Co> {
    fn as_co(&self) -> Option<Co> { *self }
}
impl AsCo for str {
    fn as_co(&self) -> Option<Co> { Co::from_name(self) }
}
impl AsCo for String {
    fn as_co(&self) -> Option<Co> { Co::from_name(self) }
}
impl<T: AsCo + ?Sized> AsCo for &T {
    fn as_co(&self) -> Option<Co> { (**self).as_co() }
}

/// The game's static tables. All lookups are array indexes.
pub struct Tables;

static TABLES: Tables = Tables;

pub fn tables() -> &'static Tables {
    &TABLES
}

impl Tables {
    pub fn unit(&self, k: impl AsUnitType) -> &'static UnitInfo {
        k.unit_type().info()
    }
    /// Every unit type with its stats, in `UnitType` order.
    pub fn units(&self) -> impl Iterator<Item = (UnitType, &'static UnitInfo)> {
        UnitType::ALL.into_iter().map(|u| (u, u.info()))
    }
    pub fn try_tile(&self, id: u16) -> Option<&'static TerrainInfo> {
        match TERRAIN_INDEX.get(id as usize) {
            Some(&i) if i != u16::MAX => Some(&TERRAIN[i as usize]),
            _ => None,
        }
    }
    pub fn tile(&self, id: u16) -> &'static TerrainInfo {
        self.try_tile(id).unwrap_or_else(|| panic!("unknown terrain {id}"))
    }
    /// Every terrain, in id order.
    pub fn terrains(&self) -> impl Iterator<Item = &'static TerrainInfo> {
        TERRAIN.iter()
    }
    /// Base damage, percent; None = the attacker can't hit the defender.
    pub fn base_damage(&self, att: impl AsUnitType, def: impl AsUnitType) -> Option<u8> {
        DAMAGE[att.unit_type() as usize][def.unit_type() as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tables_match_the_json() {
        let units: serde_json::Value = serde_json::from_str(include_str!("../../../data/units.json")).unwrap();
        for (name, u) in units.as_object().unwrap() {
            let t = UnitType::from_name(name).unwrap();
            assert_eq!(t.name(), name);
            assert_eq!(t.info().cost as u64, u["cost"].as_u64().unwrap());
            assert_eq!(t.info().move_type.name(), u["move_type"].as_str().unwrap());
        }
        let dmg: serde_json::Value = serde_json::from_str(include_str!("../../../data/damage.json")).unwrap();
        for a in UnitType::ALL {
            for d in UnitType::ALL {
                let want = dmg.get(a.name()).and_then(|m| m.get(d.name())).map(|v| v.as_u64().unwrap() as u8);
                assert_eq!(tables().base_damage(a, d), want, "{a} vs {d}");
            }
        }
        let terrain: serde_json::Value = serde_json::from_str(include_str!("../../../data/terrain.json")).unwrap();
        for (id, t) in terrain.as_object().unwrap() {
            let info = tables().tile(id.parse().unwrap());
            assert_eq!(info.name, t["name"].as_str().unwrap());
            assert_eq!(info.building.map(|b| b.name()), t["building"].as_str());
            for m in MoveType::ALL {
                assert_eq!(info.move_cost(m).map(u64::from), t["move"].get(m.name()).and_then(|v| v.as_u64()));
            }
        }
        assert!(tables().try_tile(0).is_none() && tables().try_tile(60000).is_none());
        let cos: serde_json::Value = serde_json::from_str(include_str!("../../../data/cos.json")).unwrap();
        for (name, c) in cos.as_object().unwrap() {
            let co = Co::from_name(name).unwrap();
            assert_eq!(co.info().id as u64, c["id"].as_u64().unwrap());
            for (ph, p) in [("d2d", &co.info().d2d), ("cop", &co.info().cop), ("scop", &co.info().scop)] {
                assert_eq!(p.stars.map(u64::from), c[ph]["PowerStars"].as_u64(), "{name} {ph}");
                let n = c[ph]["PowerIncreases"].as_array().map_or(0, |a| a.len());
                assert_eq!(p.boosts.len(), n, "{name} {ph}");
            }
        }
        let andy = Co::Andy.info();
        assert!(andy.scop.boosts[0].affects(UnitType::Tank) && andy.scop.boosts[0].attack == 10);
    }
    #[test]
    fn names_round_trip_in_json_and_index_in_binary_formats() {
        let s = serde_json::to_string(&UnitType::MdTank).unwrap();
        assert_eq!(s, "\"Md.Tank\"");
        assert_eq!(serde_json::from_str::<UnitType>(&s).unwrap(), UnitType::MdTank);
        assert!(UnitType::MdTank == "Md.Tank" && "Mech" == UnitType::Mech);
    }
}
