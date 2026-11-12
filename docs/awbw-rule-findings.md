# libaw vs AWBW rules: findings

Compiled 2026-11-11 from a rules review. Rules are described as behaviour;
no AWBW source is included. Game ids refer to the cached public replays in
`awbw-engine-data`. "Replays" means recorded games show it; "Rules only" means no recorded
game has the situation, so it rests on AWBW's rules alone.

## Fixed: combat rules

Replay check unchanged: all 6,848,173 checks over 499 league and 5 team games.

| # | AWBW rule | Evidence |
|---|---|---|
| 1 | Damage is computed in double precision in AWBW's order of operations, rounded to one decimal the way AWBW does it, then cut to a whole point. A raw x.95 counts as x+1 only when the float product reaches the double nearest to x.95. Pipe seams follow the same rule. | Replays: game-1749614 turn 26 (79.95 on paper, 79 dealt); all 2,243 seam hits. AWBW's public damage calculator agrees: a 5 HP Neotank on a 5 HP Infantry in a city shows 53-56% (56.95 on paper), a 5 HP Tank on a 6 HP Recon on a plain 39-44% (39.95 on paper) |
| 2 | Sonja's counterattack x1.5 multiplies her own attack value only, not com towers, the power bonus or luck. | Rules only |
| 3 | Kanbei's counterattacks get the same x1.5 under Samurai Spirit. | Rules only |
| 4 | Javier's defence against indirect fire: +20, +40 under Tower Shield, +80 under Tower of Power. | AWBW's damage calculator: Artillery on Javier's Tank under Tower of Power shows 7% |
| 5 | Kindle's High Society counts every owned property, com towers and labs included (games started after 2024-08-13). | Rules only |
| 6 | Colin's Power of Money uses the funds held at each attack, unrounded (+3% per 1000). | Rules only |
| 7 | Good luck and bad luck are separate uniform rolls, subtracted (Sonja, Flak, Jugger). | Rules only |
| 8 | Out of ammo: Mech vs Artillery is 32. Machine guns can hit Piperunners (Mech 6, B-Copter 6, Tank 6, Md.Tank 8, Neotank 10, Mega Tank 10). | Rules only |
| 9 | Max's and Sensei's Rockets take -10% attack like their other weakened units; Max's Rockets also lose 1 range. | AWBW's damage calculator: Max's or Sensei's Rocket on a Md.Tank shows 49-58% (55-64% without the penalty) |
| 10 | Jugger's powers cost 3 and 7 stars. | Rules only |

Follow-up in `awbw-bot`: its own luck roller (`src/play.rs`) should call `co::roll_luck`.

## Fixed: rules backed by recorded games

Replay check unchanged apart from one new check, `power.missile_aim` (60 of 60).

| # | AWBW rule | Evidence |
|---|---|---|
| A1 | Weather set by a power (Olaf, Drake) lasts until the start of the caster's next turn, then reverts. A later weather power takes over the timer. The engine reverts to clear weather: the records don't carry a game's own weather setting. | 93 of 93 clean casts; the step check shows 0 weather mismatches after the fix |
| A2 | Under Victory March, Sami's transports keep only their usual +1 movement; only Infantry and Mech gain +2. | 41 transport moves, none using the extra point (game-1752372 turn 25). The Power record's listing is a display quirk for loaded transports; the replay checker now ignores it. |
| A3 | A dived Sub or hidden Stealth is invisible to other teams in games without fog too, unless one of their units is adjacent or it stands on their property. A path into it is a trap. It can be attacked only if it was revealed before the attacker moved. The player view now leaves such units out with or without fog. | Five no-fog traps: game-1692796 turn 60; game-1548776 turns 95, 110, 121; game-1563482 turn 63. The own-property reveal is rules only. |
| A4 | On a map with no HQs, capturing a player's last lab eliminates them, exactly like an HQ capture. | Games 1724140, 1750733, 1747308, team 1578803. 105 of 502 cached games are on such maps. |
| A5 | Capture limit: after a completed capture, a player owning at least the game's limit of properties (towers and labs not counted) wins. League and live games set it to 70% of the map's income properties, rounded down. `State::capture_limit` is off unless set, as in AWBW's custom games (the records don't carry the setting); `rules::league_capture_limit` gives the league value and the match runner takes `MATCH_CAPTURE_LIMIT=league`. | game-1754313 turn 41 (32 of 46). The public pages of 7 recorded league games all show a capture limit of 70% of the map's income properties, rounded down. |
| A6 | Missile aim (Rachel, Von Bolt, Sturm), below. | New replay check: 60 of 60 comparable power uses |
| A7 | Property repair, exact HP: at 8.0 or more the unit goes to exactly 10.0 (free for a 9.x unit); below 8.0 it gains exactly 2.0 and keeps its fraction (7.4 becomes 9.4). Rachel: 7.0 or more goes to 10.0, otherwise +3.0. | With the tighter HP, all 47,057 recorded attacks still fit |
| A8 | A unit that uses ammo spends 1 on every pipe-seam attack, even at 0 (ammo goes to -1). | game-1751864 turn 32 |
| A9 | Unit limit: no build once the player has the game's limit of units, carried ones included. League default 50 (65 on high-funds maps). `State::unit_limit` defaults to 50; at the limit the generator offers no builds. | Consistent: nobody builds at 50+ in 42,614 builds (game-1752181 turn 43). All 50 games on the public list of completed league games show a unit limit of 50. |
| A10 | APC, T-Copter, Lander and Black Boat can't unload while standing on Sea or Reef. Cruiser and Carrier can unload anywhere. | 0 of about 3,900 recorded unloads |
| B1 | Sensei's APC, T-Copter, Lander and Black Boat always move +1. | Confirmed in a hot-seat test game on AWBW (game 1762036): Sensei's APC shows 7 movement |
| B2 | A property repair is all or nothing: if the full step isn't affordable there is no repair (the unit is still resupplied). | Rules only |
| B5 | Missiles, silos and Kindle's Urban Blight leave a unit at no less than exactly 0.1 HP. | Rules only |

Also here: a player's power now ends at the start of their turn, before repairs,
so Hachi's turn-start repairs cost 90%.

**A6 in detail.** Every map tile is scored, rows top to bottom, then left to right; the
strictly highest score wins, and nothing is fired if no tile scores above 0. A tile's score
sums over the 13-tile blast area, with the caster's own team counted negative. Per unit with
displayed HP h, the loss L is 3, or h-1 if h <= 3, or 0.1 if h <= 1; price is the owner's CO
price.
- Rachel fires three missiles, all aimed on the board as it stood before any landed, then
  all applied. First: L x weight (4 for Infantry/Mech, doubled for an enemy on a building
  it is capturing, 1 if h <= 1). Second: price x L. Third: L. The second and third break
  ties by larger total unit value.
- Von Bolt: price x L.
- Sturm: price x displayed HP; the centre must be a tile holding an enemy unit; the first
  tile scanned wins ties.

## Fixed: rules-only details

No recorded game exercises these, so they rest on AWBW's rules. Two were confirmed in a hot-seat test game on AWBW (game 1762036): Jake's Piperunner gains 1 range under his power (B10), and a damaged Infantry inside Hawke's APC is not healed by Black Wave (B6). Replay check unchanged.

| # | AWBW rule |
|---|---|
| B3 | An Eagle copter that ends at 0 fuel crashes like any other aircraft. |
| B6 | Hawke's heal skips carried units; Andy's includes them. |
| B7 | Andy's and Hawke's heals affect only the player's own units in team games. |
| B8 | Sasha's War Bonds pays on the victim's price to its owner (Kanbei 120%, Colin 80%, Hachi 90%, or 50% under his power), and also when Sonja's super makes her strike first. |
| B9 | Power costs stop growing after the 10th use (3x base). |
| B10 | Jake's powers give +1 range to every indirect except Battleship and Carrier, so Piperunners too. |
| B12 | A counterattack on a hidden Stealth does no damage unless the defender is a Fighter or Stealth (dived Sub: Cruiser or Sub). The defender still spends ammo. |
| B19 | Kindle's urban bonus applies on properties only, not on pipes. |
| B11 | Sensei's spawns stop at the unit limit, walking his empty cities by row, then column. |
| B4 | Black Boat repair: +1.0 keeping the fraction; a 9.x unit is topped up to 10.0 free only if it also needs fuel or ammo. |
| B14 | In fog, properties outside vision appear neutral with full capture points (HQs keep their owner), and an enemy Sonja's unit HP is unknown. |
| B15 | Elimination by losing all units is checked only when a unit is destroyed, on any day including day 1; a player who simply has no units is not out. With 3+ players the loser's properties turn neutral (the HQ becomes a neutral city) and their turn is skipped. |
| B16 | Two transports of one kind can join when the combined cargo fits; the cargo passes to the survivor. |
| B18 | An in-place unload blocked by an unseen unit is rejected without costing the transport its move. |
| B20 | Under Sonja's super, attacking a direct unit that could hit back but has no ammo and no second weapon does nothing at all. |
| B21 | Max's powers speed up the Black Bomb too (every unit that isn't an indirect, a foot soldier or a transport). |
| B22 | Sasha gets no +100 per property when the game's funds per property is 0. |
| B23 | A Cruiser out of ammo can't fire at all, not even at aircraft (it has no second weapon as AWBW counts them). |
| B24 | Under Sonja's Counter Break the attacker's strike, which comes second, is resolved as a counterattack: an attacking Sonja, or Kanbei under Samurai Spirit, gets the x1.5 counter bonus on it. |

Also here: `tools/gen_data.py` now keeps the Drake values `data/cos.json` already had by
hand, so regenerating the data no longer reverts them.

## Open

Left for the engine's author: each needs new machinery rather than a corrected rule.

| # | AWBW rule | Engine now |
|---|---|---|
| B17 | Teleport tiles: every unit crosses them for 0 movement and 0 fuel in any weather, a path may jump along a chain of them, nothing may stop or be dropped on one, and they are never visible in fog. No league map has them. | Treated like plains |
| B21 | The Black Bomb can explode: 5 HP to every uncarried unit within 3 tiles, own included, floor 0.1, and the bomb is gone. Banned by default. | No such action |
| B22 | Optional game settings: lab units (buildable only while the player owns a lab), a day limit (most properties wins), starting funds. | Not modelled |

## Checked and matching

Damage chart, unit stats, CO attack and defence tables, power costs, terrain stars, movement
costs in every weather, fuel use on about 199,000 recorded moves, capture points, build
rules and prices, income, turn order, fuel burn and crashes, repair costs, APC supply, meter
charging, power expiry, movement and range under powers (about 585,000 unit snapshots), and
fog vision (the recorded fog map is reproduced in all 2,852 fog turn files).

## Not settled

- Repair order when funds run short part-way through a turn start: AWBW's rules don't
  define one, and no recorded game has the situation. The engine repairs in unit-id order.
- A Sonja unit with vision 1 in rain: the engine gives it 1 tile, which reproduces every
  recorded fog map.
