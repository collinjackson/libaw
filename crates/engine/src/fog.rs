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

/// Fog trap: if `path` (from the mover's view) runs into an enemy unit, the length of the
/// prefix the unit actually walks: up to the tile before the enemy, backed off past
/// friendly units it can't stop on. None when the path is clear (or there's no fog).
pub fn trap(s: &State, u: &Unit, path: &[(u16, u16)]) -> Option<usize> {
    if !s.fog {
        return None;
    }
    let hit = path.iter().skip(1).position(|&(x, y)| {
        s.unit_at(x, y).is_some_and(|o| !s.allied(o.owner, u.owner))
    })? + 1;
    let mut k = hit; // path[..k] is walked; path[k - 1] is the stop tile
    while k > 1 && s.unit_at(path[k - 1].0, path[k - 1].1).is_some_and(|o| o.id != u.id) {
        k -= 1;
    }
    Some(k)
}

/// What `pid` may know: enemy units outside its vision (and dived ones not next door) are
/// gone, enemy cargo and funds are hidden. Bots in fog games plan on this.
pub fn player_view(s: &State, pid: PlayerId) -> State {
    let mut v = s.clone();
    if !s.fog {
        return v;
    }
    v.partial = true;
    let seen = vision(s, pid);
    let w = s.width as usize;
    let near = |x: u16, y: u16| s.units.values().any(|o| s.allied(o.owner, pid) && o.carried_by.is_none()
        && (o.x as i32 - x as i32).abs() + (o.y as i32 - y as i32).abs() <= 1);
    let hidden: Vec<UnitId> = s.units.values().filter(|u| !s.allied(u.owner, pid) && (u.carried_by.is_some()
        || seen[u.y as usize * w + u.x as usize] == 0 || (u.dived && !near(u.x, u.y)))).map(|u| u.id).collect();
    for id in &hidden {
        v.units.remove(id);
    }
    for u in v.units.values_mut() {
        u.cargo.retain(|c| !hidden.contains(c));
    }
    for p in v.players.iter_mut() {
        if !s.allied(p.id, pid) {
            p.funds = 0;
        }
    }
    v
}
