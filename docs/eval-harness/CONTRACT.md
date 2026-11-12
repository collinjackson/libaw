# Opponent bot API contract (v1)

v1 adds several bots behind one API (`bots` in `/health`, `bot` in `/act`). A v0 client
that sends no `bot` still gets `bot-a`.

Written by the clean side (the libaw author) without seeing the opponent's code. The
wrapper on the evaluation VM implements this; the clean side talks to it only through this
API and reads only `STATUS.json`.

## Transport

HTTP on `0.0.0.0:8080` on the VM (the firewall admits only Google IAP; the clean side
reaches it with `gcloud compute start-iap-tunnel`). JSON bodies, UTF-8. A response must
arrive within 10 seconds.

## `GET /health`

```json
{"ok": true, "commit": "<opponent repo commit sha>", "bot_label": "bot-a",
 "bots": ["bot-a", "bot-b", "bot-c"]}
```

`bots` lists every bot the opponent ships that can choose actions for a given state, one
opaque label each (`bot-a`, `bot-b`, ...): no names, descriptions or hints of strength or
method, and the order says nothing either. A label must keep meaning the same bot across
rebuilds of the same commit; if a rebuild changes which bots exist, add new letters rather
than reusing old ones. `bot_label` is the default bot (v0 clients).

## `POST /act`

One action at a time. The clean side is the referee: it applies the action with its own
engine (including luck) and asks again with the new state, until the bot ends its turn.

Request:

```json
{
  "player_id": 4204600,
  "seed": 12345,
  "bot": "bot-b",
  "state": { "gameState": { ... }, "day": 5, "players": [ ... ] }
}
```

- `bot` (optional, default `bot_label`): which bot answers. An unknown label is HTTP 400.

- `state` has the shape of an AWBW replay turn file (`load_replay.php`): see
  `test-states/`. `gameState` holds `terrain`, `buildings`, `units`, `players`,
  `gameWeather`, `currentTurnPId`. PHP arrays may arrive as JSON objects or lists.
  Mid-turn, units that have already acted have `units_moved: 1`. Unit HP
  (`units_hit_points`) is displayed HP, 1 to 10. Funds per player are in the top-level
  `players[].funds`.
- `player_id` is the player to move (always equal to `currentTurnPId`).
- `seed`: use it for any randomness, so the same request gives the same answer.
- Settings in these games: no fog, clear weather, CO day-to-day abilities on, CO powers
  never used by either side. Maps may contain pipes, pipe seams and missile silos (empty
  or full); the bot may treat seams and silos as impassable/plain terrain and never use
  them.

Response, exactly one of:

```json
{"type": "move", "unit_id": 202000001, "path": [[3,4],[3,5],[4,5]], "then": THEN}
{"type": "build", "at": [7,2], "unit": "Tank"}
{"type": "end_turn"}
```

- `path` starts at the unit's current tile and ends where it stops; `[[x,y]]` alone
  means it acts without moving. Coordinates are AWBW's `(x, y)`, x across, y down.
- `THEN` is one of:
  - `{"type": "wait"}`
  - `{"type": "attack", "target": [x,y]}` (the defender's tile)
  - `{"type": "capture"}`
  - `{"type": "load"}` (the path ends on a friendly transport)
  - `{"type": "join"}` (the path ends on a friendly unit of the same kind)
  - `{"type": "supply"}` (APC)
  - `{"type": "repair", "target": [x,y]}` (Black Boat)
  - `{"type": "unload", "cargo_id": 202000002, "to": [x,y]}`
- Unit names are AWBW's (`Infantry`, `Mech`, `Md.Tank`, `Anti-Air`, `B-Copter`, ...).
- If the bot's own engine disagrees with the state (an action it thinks is illegal, a
  unit it can't place), answer with its best legal action anyway; never an error.

Errors: HTTP 400 with `{"error": "<short reason>"}` only for a malformed request.

## `STATUS.json`

At `/opt/eval/STATUS.json` on the VM, the only file the clean side reads:

```json
{"built": true, "commit": "<sha>", "bot_label": "bot-a", "bots": ["bot-a", "bot-b"],
 "updated": "<ISO time>"}
```

v0's `contract_tests_*` fields are gone: the clean side runs the contract test itself,
through the API.

No logs, no prose, no excerpts of the opponent's code or docs, here or anywhere the clean
side can see.

## Contract test

Run by the clean side through the API (IAP tunnel), not on the VM. `check_contract.py` posts each file in `test-states/` and checks that the
reply has the shape above and refers to the moving player's own unmoved units, owned
bases and in-bounds tiles. It does not judge legality or strength; the clean side does
that with its engine.
