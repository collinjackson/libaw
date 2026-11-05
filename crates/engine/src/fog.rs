//! Fog of war: what a player can see. Checked against the `fog_array` grid in fog-game
//! records (the current player's vision count per tile).
//!
//! Rules (AWBW's published fog rules, refined against records):
//! - a unit sees tiles within its vision range (Manhattan distance); carried units see nothing;
//! - foot soldiers on a mountain see 3 further;
//! - forests and reefs are seen only from next door (or by standing on them);
//! - a player's own properties see their own tile; pipes are always seen;
//! - rain cuts vision by 1 (never below 1).

use crate::combat::co_of;
use crate::data::tables;
use crate::state::*;

/// Vision range of unit `u` where it stands.
pub fn vision_range(s: &State, u: &Unit) -> u32 {
    let info = tables().unit(&u.kind);
    let mut v = info.vision as i32;
    let t = tables().tile(s.terrain_at(u.x, u.y));
    if matches!(u.kind.as_str(), "Infantry" | "Mech") && t.kind == "Mountain" {
        v += 3;
    }
    if co_of(s, u.owner) == "Sonja" {
        v += 1;
    }
    if s.weather == 'R' {
        v -= 1;
    }
    v.max(1) as u32
}

/// Tiles that hide units unless seen from next door.
/// An air unit over a forest or reef doesn't hide, and the tile shows at normal range.
pub fn hiding_tile(s: &State, x: u16, y: u16) -> bool {
    let t = tables().tile(s.terrain_at(x, y));
    (t.kind == "Forest" || t.name.contains("Reef"))
        && !s.unit_at(x, y).is_some_and(|u| tables().unit(&u.kind).move_type == "A")
}

/// How many of `pid`'s (and allies') sources see each tile, row-major (y * width + x).
pub fn vision(s: &State, pid: PlayerId) -> Vec<u8> {
    let (w, h) = (s.width as i32, s.height as i32);
    let mut out = vec![0u8; (w * h) as usize];
    let mut see = |x: i32, y: i32| {
        if x >= 0 && y >= 0 && x < w && y < h {
            let c = &mut out[(y * w + x) as usize];
            *c = c.saturating_add(1);
        }
    };
    for u in s.units.values().filter(|u| s.allied(u.owner, pid) && u.carried_by.is_none()) {
        let r = vision_range(s, u) as i32;
        let (ux, uy) = (u.x as i32, u.y as i32);
        for dy in -r..=r {
            let rem = r - dy.abs();
            for dx in -rem..=rem {
                let (x, y) = (ux + dx, uy + dy);
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                if dx.abs() + dy.abs() > 1 && hiding_tile(s, x as u16, y as u16) {
                    continue;
                }
                see(x, y);
            }
        }
    }
    // Pipes (seams and ends included, not rubble) are in everyone's view.
    for y in 0..h {
        for x in 0..w {
            let n = &tables().tile(s.terrain_at(x as u16, y as u16)).name;
            if n.contains("Pipe") && !n.contains("Rubble") {
                see(x, y);
            }
        }
    }
    for (&(x, y), b) in &s.buildings {
        if b.owner.is_some_and(|o| s.allied(o, pid)) {
            see(x as i32, y as i32);
        }
    }
    out
}
