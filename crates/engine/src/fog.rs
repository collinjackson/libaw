//! Fog of war: what a player can see. Checked against the `fog_array` grid in fog-game
//! records (the current player's vision count per tile).
//!
//! Rules (AWBW's published fog rules, refined against records):
//! - a unit sees tiles within its vision range (Manhattan distance); carried units see nothing;
//! - foot soldiers on a mountain see 3 further;
//! - forests and reefs are seen only from next door (or by standing on them);
//! - a player's own properties see their own tile; pipes are always seen;
//! - rain cuts vision by 1 (never below 1);
//! - Sonja: +1 vision; under either power +1 more, and she sees into forests and reefs at
//!   any range (AWBW's CO page; fits the fog records' `discovered` lists).

#[allow(unused_imports)]
use crate::prelude::*;
use crate::combat::{co_of, pw};
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
        if pw(s, u.owner) != "N" {
            v += 1;
        }
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

/// Sonja under a power sees into hiding tiles from anywhere in range.
fn sees_hidden(s: &State, p: PlayerId) -> bool {
    co_of(s, p) == "Sonja" && pw(s, p) != "N"
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
        let piercing = sees_hidden(s, u.owner);
        let (ux, uy) = (u.x as i32, u.y as i32);
        for dy in -r..=r {
            let rem = r - dy.abs();
            for dx in -rem..=rem {
                let (x, y) = (ux + dx, uy + dy);
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                if !piercing && dx.abs() + dy.abs() > 1 && hiding_tile(s, x as u16, y as u16) {
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

/// Is this dived Sub or hidden Stealth out of `pid`'s sight? With or without fog, it shows
/// only to a team with an uncarried unit next to it, or when it stands on a property of
/// theirs.
pub fn concealed(s: &State, u: &Unit, pid: PlayerId) -> bool {
    if !u.dived || s.allied(u.owner, pid) {
        return false;
    }
    let beside = s.units.values().any(|o| s.allied(o.owner, pid) && o.carried_by.is_none()
        && (o.x as i32 - u.x as i32).abs() + (o.y as i32 - u.y as i32).abs() <= 1);
    let on_theirs = s.buildings.get(&(u.x, u.y)).is_some_and(|b| b.owner.is_some_and(|o| s.allied(o, pid)));
    !beside && !on_theirs
}

/// Trap: if `path` (from the mover's view) runs into an enemy unit it couldn't see, the
/// length of the prefix the unit actually walks: up to the tile before the enemy, backed
/// off past friendly units it can't stop on. None when the path is clear. In fog any enemy
/// unit on a planned path was unseen; without fog only a dived Sub or hidden Stealth can be
/// (five recorded traps in games without fog, e.g. game-1692796 turn 60).
pub fn trap(s: &State, u: &Unit, path: &[(u16, u16)]) -> Option<usize> {
    let hit = path.iter().skip(1).position(|&(x, y)| {
        s.unit_at(x, y).is_some_and(|o| !s.allied(o.owner, u.owner) && (s.fog || concealed(s, o, u.owner)))
    })? + 1;
    let mut k = hit; // path[..k] is walked; path[k - 1] is the stop tile
    while k > 1 && s.unit_at(path[k - 1].0, path[k - 1].1).is_some_and(|o| o.id != u.id) {
        k -= 1;
    }
    Some(k)
}

/// What `pid` may know. In fog: enemy units outside its vision are gone, enemy cargo and
/// funds are hidden, and properties out of sight look neutral and untouched (only an HQ
/// keeps its owner). With or without fog, dived Subs and hidden Stealths out of sight are
/// gone too (`concealed`), and an enemy Sonja's units don't show their HP. Bots plan on this.
pub fn player_view(s: &State, pid: PlayerId) -> State {
    let mut v = s.clone();
    // An enemy Sonja's units show "?" for HP: anything from 0.1 to 10.
    let sonja: Vec<PlayerId> = s.players.iter().filter(|p| p.co == "Sonja" && !s.allied(p.id, pid)).map(|p| p.id).collect();
    for u in v.units.values_mut().filter(|u| sonja.contains(&u.owner)) {
        u.hp = Hp { lo: 1, hi: 100 };
    }
    if !s.fog {
        let unseen: Vec<UnitId> = s.units.values().filter(|u| concealed(s, u, pid)).map(|u| u.id).collect();
        for id in &unseen {
            v.units.remove(id);
        }
        // With units missing, "no units left" no longer proves a player is out.
        v.partial |= !unseen.is_empty();
        drop_unseen_overrides(&mut v);
        return v;
    }
    v.partial = true;
    let seen = vision(s, pid);
    let w = s.width as usize;
    let hidden: Vec<UnitId> = s.units.values().filter(|u| !s.allied(u.owner, pid) && (u.carried_by.is_some()
        || seen[u.y as usize * w + u.x as usize] == 0 || concealed(s, u, pid))).map(|u| u.id).collect();
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
    // Properties out of sight: who holds them and any capture under way can't be seen.
    let unseen: Vec<(u16, u16)> = s.buildings.iter()
        .filter(|(&(x, y), b)| seen[y as usize * w + x as usize] == 0
            && matches!(tables().tile(b.terrain).building.as_deref(), Some("City" | "Base" | "Airport" | "Port" | "ComTower" | "Lab")))
        .map(|(&k, _)| k).collect();
    for k in unseen {
        let b = v.buildings.get_mut(&k).unwrap();
        (b.owner, b.capture) = (None, crate::rules::CAPTURE_POINTS);
        b.terrain = crate::awbw::owned_tile(b.terrain, None);
        v.terrain[k.1 as usize * w + k.0 as usize] = b.terrain;
    }
    // An HQ out of sight keeps its owner, but a capture under way on it can't be seen.
    for (&(x, y), b) in v.buildings.iter_mut() {
        if seen[y as usize * w + x as usize] == 0 && tables().tile(b.terrain).building.as_deref() == Some("HQ") {
            b.capture = crate::rules::CAPTURE_POINTS;
        }
    }
    drop_unseen_overrides(&mut v);
    v
}

/// A movement power's per-unit movement (`move_override`) is kept only for units the viewer
/// can see: the ids of the others, and foot vs vehicle by their value, would give them away.
fn drop_unseen_overrides(v: &mut State) {
    let units = &v.units;
    v.move_override.retain(|id, _| units.contains_key(id));
}
