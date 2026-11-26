# Storyboards: a turn as data for replay viewers

A storyboard is one turn as plain JSON: the board at the turn's start, then the turn's actions as
steps, with the results the game recorded (damage, captures). `web/replay.html` plays one, and
`web/capture.mjs` records that page (or any page built on `web/js/replay.js`) as a GIF or MP4.

    awbw-replay --storyboard TURN.json > turn.storyboard.json      # from an AWBW turn record
    python3 -m http.server 8000 --bind 127.0.0.1                    # from the repo root
    open 'http://127.0.0.1:8000/web/replay.html?sb=PATH&art=libaw&camera=1'
    node web/capture.mjs 'http://127.0.0.1:8000/web/replay.html?sb=PATH' out/reel --gif --mp4

## Format (version 1)

    {"format": "libaw-storyboard", "version": 1, "day": 11, "player": 101,
     "names": {"101": "...", "102": "..."}, "countries": {"101": "os", "102": "bm"},
     "start": <state, as the socket API's "state" message carries it>,
     "steps": [...]}

Steps, in order:

| type | fields |
|---|---|
| `move` | `unit_id`, `path` ([[x, y], ...], start first), `trapped` |
| `fire` | `attacker`, `defender`: each `{unit_id, at, hp, display_hp}` after the attack, or null when fog hides it. When the record hides the HP ("?": Sonja's units, seen by her opponents), `display_hp` is null and `hp` is `{lo: 1, hi: 100}`: the unit lives on with unknown HP, and viewers show "?" |
| `capture` | `at`, `capture` (points left), `done`, `new_owner` |
| `build` | `unit` (a full unit) |
| `power` | `player`, `co`, `name`, `super` |
| `join` | `from`, `into`, `unit` (the joined unit) |
| `load` | `unit_id` (the unit loaded), `transport_id` |
| `unload` | `transport_id`, `unit_id` (the cargo), `to` (where it lands) |
| `supply` | `supplied` (ids of the units supplied; the record doesn't name the APC: it's the APC that just moved, or the player's APC beside the first of them) |
| `hide`, `unhide` | `unit_id` (a Sub dives or surfaces, a Stealth hides or shows) |
| `delete` | `unit_id` |
| `explode` | `unit_id` (a Black Bomb), `at`, `damage` (HP taken by every other unit within 3 tiles, never below 1 HP) |
| `launch` | `silo`, `target`, `damage` (HP taken by every unit within 2 tiles of `target`, never below 1 HP); the silo is left empty |
| `repair` | `unit_id` (the Black Boat), `target_id`, `display_hp` (the repaired unit's HP after) |
| `end` | `next` (the next player) |
| `other` | `action` (a record this version does not show, e.g. `AttackSeam`) |

A move followed by what the unit did where it stopped (`fire`, `capture`, `join`, `load`,
`unload`, `supply`, `hide`, `unhide`, `explode`, `launch`, `repair`, `delete`) is one shot.
Storyboards made before these steps had their own types carry them as `other`.

Viewers ignore fields they don't know, so a caller can carry its own extras in a storyboard.

## Playing part of a turn

`web/js/story.js` groups steps into shots (a unit's move with what it did there) and lays out a
timeline. `Replay` (web/js/replay.js) plays all shots by default; a caller can pass the shots it
wants, add phases of its own ahead of a step (`before`) and draw them (`overlays`).

## Art

Without an art set the viewer draws libaw's own sprites (`web/assets`). `art=awbw`
draws units with Advance Wars By Web's sprites, loaded from AWBW's site when the page runs: libaw
carries no AWBW art, only the address pattern (`web/js/art.js`). A sprite that fails to load falls
back to libaw's.
