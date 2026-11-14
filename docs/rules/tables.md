# Rules tables

Generated from libaw's data tables (`data/*.json`) by `crates/engine/tests/rules_tables.rs`; do not edit by
hand. These are the values the engine plays with. Where AWBW's published numbers come from, and how they
were checked against recorded games, is in `docs/sources.md` and `docs/awbw-rule-findings.md`.

## Units

Cost in funds; move in movement points; fuel per day (`burn`) is what an air or sea unit uses at
the start of each of its turns; ammo 0 means it has only a machine gun or no weapon; range is min-max
(1-1 is direct fire; none: it can't attack).

Move types: F foot (Infantry), B boots (Mech), T treads, W tires, A air, S sea, L lander, P pipe.

| Unit | Cost | Move | Type | Fuel | Burn | Ammo | Vision | Range | 2nd gun |
|---|---|---|---|---|---|---|---|---|---|
| Infantry | 1000 | 3 | F | 99 | 0 | 0 | 2 | 1-1 | no |
| Mech | 3000 | 2 | B | 70 | 0 | 3 | 2 | 1-1 | yes |
| Md.Tank | 16000 | 5 | T | 50 | 0 | 8 | 1 | 1-1 | yes |
| Tank | 7000 | 6 | T | 70 | 0 | 9 | 3 | 1-1 | yes |
| Recon | 4000 | 8 | W | 80 | 0 | 0 | 5 | 1-1 | no |
| APC | 5000 | 6 | T | 70 | 0 | 0 | 1 | none | no |
| Artillery | 6000 | 5 | T | 50 | 0 | 9 | 1 | 2-3 | no |
| Rocket | 15000 | 5 | W | 50 | 0 | 6 | 1 | 3-5 | no |
| Anti-Air | 8000 | 6 | T | 60 | 0 | 9 | 2 | 1-1 | no |
| Missile | 12000 | 4 | W | 50 | 0 | 6 | 5 | 3-5 | no |
| Fighter | 20000 | 9 | A | 99 | 5 | 9 | 2 | 1-1 | no |
| Bomber | 22000 | 7 | A | 99 | 5 | 9 | 2 | 1-1 | no |
| B-Copter | 9000 | 6 | A | 99 | 2 | 6 | 3 | 1-1 | yes |
| T-Copter | 5000 | 6 | A | 99 | 2 | 0 | 2 | none | no |
| Battleship | 28000 | 5 | S | 99 | 1 | 9 | 2 | 2-6 | no |
| Cruiser | 18000 | 6 | S | 99 | 1 | 9 | 3 | 1-1 | no |
| Lander | 12000 | 6 | L | 99 | 1 | 0 | 1 | none | no |
| Sub | 20000 | 5 | S | 60 | 1 | 6 | 5 | 1-1 | no |
| Black Boat | 7500 | 7 | L | 60 | 1 | 0 | 1 | none | no |
| Carrier | 30000 | 5 | S | 99 | 1 | 9 | 4 | 3-8 | no |
| Stealth | 24000 | 6 | A | 60 | 5 | 6 | 4 | 1-1 | no |
| Neotank | 22000 | 6 | T | 99 | 0 | 9 | 1 | 1-1 | yes |
| Piperunner | 20000 | 9 | P | 99 | 0 | 9 | 4 | 2-5 | yes |
| Black Bomb | 25000 | 9 | A | 45 | 5 | 0 | 1 | none | no |
| Mega Tank | 28000 | 4 | T | 50 | 0 | 3 | 1 | 1-1 | yes |

## Base damage

Percent of a full-HP defender's HP that a full-HP attacker deals before CO, terrain and luck
(the damage formula is in `rules::damage_f64`). Rows attack, columns defend; blank means it can't attack that
unit.

Split by what is defending so each table fits a page. Column names are shortened: Inf Infantry, Md.T
Md.Tank, Arty Artillery, Rkt Rocket, AA Anti-Air, Msl Missile, Neo Neotank, Pipe Piperunner, Mega Mega Tank,
B-Cop/T-Cop B-/T-Copter, B.Bomb Black Bomb, B.Ship Battleship, B.Boat Black Boat.

### Against ground units

| Attacker | Inf | Mech | Md.T | Tank | Recon | APC | Arty | Rkt | AA | Msl | Neo | Pipe | Mega |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Infantry | 55 | 45 | 1 | 5 | 12 | 14 | 15 | 25 | 5 | 25 | 1 | 5 | 1 |
| Mech | 65 | 55 | 15 | 55 | 85 | 75 | 70 | 85 | 65 | 85 | 15 | 55 | 5 |
| Md.Tank | 105 | 95 | 55 | 85 | 105 | 105 | 105 | 105 | 105 | 105 | 45 | 85 | 25 |
| Tank | 75 | 70 | 15 | 55 | 85 | 75 | 70 | 85 | 65 | 85 | 15 | 55 | 10 |
| Recon | 70 | 65 | 1 | 6 | 35 | 45 | 45 | 55 | 4 | 28 | 1 | 6 | 1 |
| Artillery | 90 | 85 | 45 | 70 | 80 | 70 | 75 | 80 | 75 | 80 | 40 | 70 | 15 |
| Rocket | 95 | 90 | 55 | 80 | 90 | 80 | 80 | 85 | 85 | 90 | 50 | 80 | 25 |
| Anti-Air | 105 | 105 | 10 | 25 | 60 | 50 | 50 | 55 | 45 | 55 | 5 | 25 | 1 |
| Bomber | 110 | 110 | 95 | 105 | 105 | 105 | 105 | 105 | 95 | 105 | 90 | 105 | 35 |
| B-Copter | 75 | 75 | 25 | 55 | 55 | 60 | 65 | 65 | 25 | 65 | 20 | 55 | 10 |
| Battleship | 95 | 90 | 55 | 80 | 90 | 80 | 80 | 85 | 85 | 90 | 50 | 80 | 25 |
| Stealth | 90 | 90 | 70 | 75 | 85 | 85 | 75 | 85 | 50 | 85 | 60 | 80 | 15 |
| Neotank | 125 | 115 | 75 | 105 | 125 | 125 | 115 | 125 | 115 | 125 | 55 | 105 | 35 |
| Piperunner | 95 | 90 | 55 | 80 | 90 | 80 | 80 | 85 | 85 | 90 | 50 | 80 | 25 |
| Mega Tank | 135 | 125 | 125 | 180 | 195 | 195 | 195 | 195 | 195 | 195 | 115 | 180 | 65 |

### Against air units

| Attacker | Fighter | Bomber | B-Cop | T-Cop | Stealth | B.Bomb |
|---|---|---|---|---|---|---|
| Infantry | | | 7 | 30 | | |
| Mech | | | 9 | 35 | | |
| Md.Tank | | | 12 | 45 | | |
| Tank | | | 10 | 40 | | |
| Recon | | | 10 | 35 | | |
| Anti-Air | 65 | 75 | 120 | 120 | 75 | 120 |
| Missile | 100 | 100 | 120 | 120 | 100 | 120 |
| Fighter | 55 | 100 | 100 | 100 | 85 | 120 |
| B-Copter | | | 65 | 95 | | |
| Cruiser | 55 | 65 | 115 | 115 | 100 | 120 |
| Carrier | 100 | 100 | 115 | 115 | 100 | 120 |
| Stealth | 45 | 70 | 85 | 95 | 55 | 120 |
| Neotank | | | 22 | 55 | | |
| Piperunner | 65 | 75 | 105 | 105 | 75 | 120 |
| Mega Tank | | | 22 | 55 | | |

### Against sea units

| Attacker | B.Ship | Cruiser | Lander | Sub | B.Boat | Carrier |
|---|---|---|---|---|---|---|
| Md.Tank | 10 | 45 | 35 | 10 | 35 | 10 |
| Tank | 1 | 5 | 10 | 1 | 10 | 1 |
| Artillery | 40 | 65 | 55 | 60 | 55 | 45 |
| Rocket | 55 | 85 | 60 | 85 | 60 | 60 |
| Bomber | 75 | 85 | 95 | 95 | 95 | 75 |
| B-Copter | 25 | 55 | 25 | 25 | 25 | 25 |
| Battleship | 50 | 95 | 95 | 95 | 95 | 60 |
| Cruiser | | | | 90 | 25 | 5 |
| Sub | 55 | 25 | 95 | 55 | 95 | 75 |
| Stealth | 45 | 35 | 65 | 55 | 65 | 45 |
| Neotank | 15 | 50 | 40 | 15 | 40 | 15 |
| Piperunner | 55 | 60 | 60 | 85 | 60 | 60 |
| Mega Tank | 45 | 65 | 75 | 45 | 105 | 45 |

## Terrain

Defence stars and movement cost by move type in clear weather (blank: impassable). Rain and snow
costs are in `rules::move_cost_in`. Tiles that share all values are listed together. Foot is Infantry,
Boots is Mech; Lander covers Landers and Black Boats.

| Terrain | Stars | Foot | Boots | Treads | Tires | Air | Sea | Lander | Pipe |
|---|---|---|---|---|---|---|---|---|---|
| Airport (21 tiles) | 3 | 1 | 1 | 1 | 1 | 1 |  |  |  |
| Base (21 tiles) | 3 | 1 | 1 | 1 | 1 | 1 |  |  | 1 |
| City (21 tiles) | 3 | 1 | 1 | 1 | 1 | 1 |  |  |  |
| ComTower (21 tiles) | 3 | 1 | 1 | 1 | 1 | 1 |  |  |  |
| Wood | 2 | 1 | 1 | 2 | 3 | 1 |  |  |  |
| HQ (20 tiles) | 4 | 1 | 1 | 1 | 1 | 1 |  |  |  |
| Lab (21 tiles) | 3 | 1 | 1 | 1 | 1 | 1 |  |  |  |
| Missile Silo | 3 | 1 | 1 | 1 | 1 | 1 |  |  |  |
| Mountain | 4 | 2 | 1 |  |  | 1 |  |  |  |
| Pipe (10 tiles) | 0 |  |  |  |  |  |  |  | 1 |
| HPipe Rubble, VPipe Rubble | 1 | 1 | 1 | 1 | 2 | 1 |  |  |  |
| Pipe Seam | 0 |  |  |  |  |  |  |  | 1 |
| Teleport Tile | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |
| Plain | 1 | 1 | 1 | 1 | 2 | 1 |  |  |  |
| Missile Silo Empty | 3 | 1 | 1 | 1 | 1 | 1 |  |  |  |
| Port (21 tiles) | 3 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |  |
| River (11 tiles) | 0 | 2 | 1 |  |  | 1 |  |  |  |
| Road (13 tiles) | 0 | 1 | 1 | 1 | 1 | 1 |  |  |  |
| Sea (47 tiles) | 0 |  |  |  |  | 1 | 1 | 1 |  |
| Reef | 1 |  |  |  |  | 1 | 2 | 2 |  |
| Shoal (81 tiles) | 0 | 1 | 1 | 1 | 1 | 1 |  | 1 |  |

## CO power costs

Stars for the CO power and the super power, before the per-use increase (each use adds 20% of
the base, up to 10 uses). Day-to-day abilities and power effects are in `co.rs` and `docs/rules/`.

| CO | Power | Super |
|---|---|---|
| Adder | 2 | 5 |
| Andy | 3 | 6 |
| Colin | 2 | 6 |
| Drake | 4 | 7 |
| Eagle | 3 | 9 |
| Flak | 3 | 6 |
| Grimm | 3 | 6 |
| Grit | 3 | 6 |
| Hachi | 3 | 5 |
| Hawke | 5 | 9 |
| Jake | 3 | 6 |
| Javier | 3 | 6 |
| Jess | 3 | 6 |
| Jugger | 3 | 7 |
| Kanbei | 4 | 7 |
| Kindle | 3 | 6 |
| Koal | 3 | 5 |
| Lash | 4 | 7 |
| Max | 3 | 6 |
| Nell | 3 | 6 |
| No CO | - | - |
| Olaf | 3 | 7 |
| Rachel | 3 | 6 |
| Sami | 3 | 8 |
| Sasha | 2 | 6 |
| Sensei | 2 | 6 |
| Sonja | 3 | 5 |
| Sturm | 6 | 10 |
| Von Bolt | - | 10 |
