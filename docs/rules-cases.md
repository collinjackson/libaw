# Rules cases

A rules case is one JSON file in `crates/engine/tests/cases/` that pins down one rule: a position,
one action, and what AWBW does. `cargo test` checks every case; a case the engine gets wrong
fails with a line like

    tank-vs-tank-plains-min-luck.json: unit 2 hp: engine 51, AWBW 50

Run one case (name substring): `CASE=tank-vs cargo test --release -p awbw-engine --test rules_cases`.

## Format

```json
{
  "title": "Tank hits Tank on Plains, no CO effects, lowest luck",
  "source": "where the truth comes from: an AWBW game id and turn, the damage chart, ...",
  "state": { ... },
  "action": {"move": {"unit": 1, "to": [2, 0], "then": {"attack": [3, 0]}}},
  "luck": "min",
  "expect": {"units": {"2": {"hp": 51}}}
}
```

**Position**, one of:
- `state`: the engine's state inline (see the two starter cases; coordinates are `[x, y]` from
  the top left, terrain ids are AWBW's, `hp` is AWBW's internal 0-100 as `{"lo": n, "hi": n}`).
- `state_file`: the same, in a file next to the case.
- `awbw_turn_file`: an AWBW replay turn as recorded (the JSON with `gameState`), next to the case
  or under `positions/` (files there aren't run as cases).
  The quickest way to turn a replay moment into a case.

**Action**:
- `{"move": {"unit": ID, "to": [x, y], "then": T}}`. `to` defaults to where the unit stands;
  `path` (a list of tiles) is optional, the engine finds one. `T` is `"wait"` (the default),
  `"capture"`, `"load"`, `"join"`, `"supply"`, `"hide"`, `"unhide"`, `"delete"`,
  `{"attack": [x, y]}`, `{"repair": [x, y]}`, `{"seam": [x, y]}`, `{"launch": [x, y]}`,
  `{"unload": {"cargo": ID, "to": [x, y]}}`.
- `{"build": {"at": [x, y], "kind": "Tank"}}`, `{"power": "Y"}` (CO power) or `"S"` (super),
  `"end_turn"`.

**Luck** (attacks): `"min"` (default), `"max"`, or a number for both rolls.

**Expect**, any of:
- `"legal": false`: AWBW refuses the action (nothing else is checked).
- `"units"`: unit id to the fields that matter after the action (`hp`, `x`, `y`, `fuel`,
  `ammo`, `moved`, `owner`, ...); `null` means AWBW destroyed it.
- `"players"`: player id to fields (`funds`, `meter`, `power`, ...).

Fields you leave out aren't checked, so name only what the rule is about.

## Writing a good case

- One rule per case, the smallest board that shows it. Name the file after the rule.
- Say where the truth comes from in `source`: an AWBW game id and day, the damage chart, a test
  on a private AWBW game. Describe behaviour, never paste AWBW's code.
- A case can be committed failing on purpose: that's the bug report. Whoever fixes the engine
  makes it pass.
