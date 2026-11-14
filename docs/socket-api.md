# Socket API (protocol v1)

`libaw-server` holds games in memory and lets any client play them over a WebSocket: bots,
UIs, spectators, test harnesses. The engine is the referee. A client sends one action at a
time; the server checks it against the legal actions of what that player can see, applies it
with the game's seeded luck, and tells everyone in the game what changed.

    cargo run --release -p libaw-server -- 127.0.0.1:8765      # ws://127.0.0.1:8765

Every message is one JSON object with a `type`. A request may carry an `id` (any JSON value);
the server copies it onto its direct reply (`welcome`, `created`, `joined`, `ok`, `error`, ...),
so a client can match them up. Broadcasts (`state`, `event`, `turn`, `game_over`) carry no `id`.

## Vocabulary

- **Coordinates** are `[x, y]`, x across and y down from the top left, as on AWBW.
- **Actions** use the bot contract's shapes (docs/eval-harness/CONTRACT.md):
  - `{"type": "move", "unit_id": 12, "path": [[3,4],[3,5]], "then": THEN}`. The path starts on
    the unit's tile; `[[x,y]]` alone acts in place. A path that differs from the engine's but is
    valid is kept. Or give only the destination, `"to": [3,5]`, instead of `path`: the engine
    uses its cheapest path (send a path when the route matters, e.g. around a possible trap).
  - `THEN`: `{"type": "wait"}`, `{"type": "attack", "target": [x,y]}`, `{"type": "capture"}`,
    `{"type": "load"}`, `{"type": "join"}`, `{"type": "supply"}`,
    `{"type": "repair", "target": [x,y]}`, `{"type": "unload", "cargo_id": 13, "to": [x,y]}`,
    `{"type": "hide"}`, `{"type": "unhide"}`, `{"type": "delete"}`,
    `{"type": "launch", "target": [x,y]}`, `{"type": "attack_seam", "target": [x,y]}`.
  - `{"type": "build", "at": [x,y], "unit": "Tank"}`
  - `{"type": "power", "power": "Y"}` (CO power) or `"S"` (super), in games with powers on.
  - `{"type": "end_turn"}`
- **Names** are AWBW's identifiers: units (`Infantry`, `Md.Tank`, `B-Copter`, ...), COs
  (`Andy`, `Grimm`, ...), terrain ids. What a UI shows for them is the client's choice.
- **State formats** (chosen per seat or spectator with `format`):
  - `libaw` (default): the engine's state JSON (`crates/engine/src/state.rs`): `width`,
    `height`, `terrain` (row-major AWBW terrain ids), `buildings` (`[x, y, {terrain, owner,
    capture}]`), `units` (by id: `kind`, `owner`, `x`, `y`, `hp` as AWBW's internal 0-100 in
    `{"lo", "hi"}`, `fuel`, `ammo`, `moved`, `cargo`, ...), `players` (turn order: `id`, `team`,
    `co`, `funds`, `power`, `meter`, ...), `current`, `day`, `weather`, `fog`, `powers`.
  - `awbw`: an AWBW replay turn file (`gameState` with `units`, `buildings`, `players`, ...),
    the shape the bot contract's bots already read. Only for games created from a turn file.

## From a browser

Plain WebSocket, JSON text frames, no headers, cookies or subprotocols: a page connects with
`new WebSocket("ws://host:8765")` and sends the same messages. A seat's `token` goes in the
`join` message (a page can keep it in `localStorage` to take the seat back after a reload).
The `libaw` state has what a board needs: terrain ids, buildings (owner, capture points),
units (kind, owner, HP, fuel, ammo, moved, cargo), each player's funds, CO, power state and
meter, the day, the weather and who is to move. `examples/spectator.html` draws one.

## Bots are just clients

A bot holds a seat like any other client; nothing on the server tells them apart. A bot that
speaks the HTTP bot contract plugs in through `examples/http_bot_adapter.py`, which joins a
seat with `format: "awbw"` and forwards each `turn` to the bot's `POST /act`.

## Client to server

| type | fields | reply |
|---|---|---|
| `hello` | `name`? | `welcome` {`protocol`: 1, `server`, `version`} |
| `list` | | `games` {`games`: [summary]} |
| `create` | `turn` (AWBW turn file) or `state` (libaw state); `seed`?, `max_days`?, `fog`?, `powers`?, `banned`? [unit names], `name`? (game id) | `created` {`game`, `players`, `awbw_format`, `spectator_key`, `seed` (only if you chose it)} |
| `join` | `game`; `player` (id) or `seat` (index), else the first free seat; `token`? (to take a seat back); `format`?; `legal`? (`"summary"`: short `turn` messages, see below) | `joined` {`game`, `player`, `seat`, `token`, `format`, `info`}, then `state`, then `turn` if it's this player's |
| `spectate` | `game`, `key` (the game's `spectator_key`, while it runs), `format`? | `joined` {`spectator`: true, `info`}, then `state` |
| `state` | `game` | `state` |
| `legal` | `game`, `filter`? {`unit`, `to`, `then`, `kind`} | `turn` (only for the player to move); with a filter, only the matching actions |
| `options` | `game`, `unit` | `options` {`unit`, `tiles`: [{`to`, `path`, `then`: [THEN]}]}: one unit's choices, by destination |
| `act` | `game`, `action` | `ok` {`seq`}, then broadcasts |
| `resign` | `game` | `ok`, then broadcasts |
| `leave` | `game` | `left` |

A game's seats are its players, in turn order. Creating a game doesn't seat anyone. A seat
is held by one connection at a time; when it closes, the seat frees up and the `token` from
`joined` takes it back (any connection). Tokens stop seat mix-ups, not attackers: v1 has no
accounts or passwords, so run it on a trusted network.

## Server to client

- `state` {`game`, `format`, `day`, `current`, `seq`, `state`}: after every change, to every
  seat (its own fog view) and spectator.
- `event` {`game`, `seq`, `by`, `day`, `action`, `label`, `ended_turn`}: what was done, as
  applied (`label` is a short human description). In fog games only the mover's team and
  spectators get events; the others see the result in their next `state`.
- `turn` {`game`, `player`, `day`, `seq`, `legal`}: to the seat on move, after every change
  while it is still its turn, and when its turn comes. `legal` lists every legal action in
  what it can see, so a bot can just pick one.
- `game_over` {`game`, `winner` (a team, or null), `reason`, `day`}: reasons `last_standing`
  (AWBW's rout and HQ rules), `resign`, `day_limit` (no winner; `max_days` reached).
- `error` {`re` (the request's type), `message`}: the request changed nothing. An illegal
  action is an error and the player keeps the turn.

`seq` counts changes to what the receiver sees: for a seat, each change to its own view (so it can tell
if it missed one, but not how many enemy actions it didn't see); for a spectator, every applied action.
A seat whose view didn't change gets no `state`.

## Rules of play

- Luck: each game has a `seed` (given or random, reported in `created`). The same seed and the
  same actions give the same game.
- Fog: a seat sees `fog::player_view` (enemy units out of vision removed, enemy funds hidden).
  Its action is checked against that view and applied to the real position, so in fog a move
  can run into a hidden unit, as on AWBW.
- Disbanding (`delete`) is allowed. Units start with exact HP.
- No clocks: a turn waits as long as its player takes.

## A bot in a few lines

    send hello; send join {game}
    on turn: send act {action: pick(turn.legal)}
    on game_over: stop

`examples/` has a random Python bot, a JavaScript bot and a spectator page.

## Not in v1

- AWBW's own game socket (`Move`/`Fire`/`Capt`/... frames with tile-index paths): a
  compatibility layer could translate it, so AWBW clients could connect unchanged.
- Accounts, passwords, private games; persistence (games live in memory); clocks and timeouts.
- Lobby features: map lists, CO picks, creating a game from a map id instead of a position.
- Replays as a download (the `event` stream plus the seed is enough to rebuild one).

### `preview` (seat on move)

`{"type": "preview", "game": G, "action": A}` where `A` is a legal move that attacks (from `turn`).
The server plays it on a copy of the player's own view, once at the lowest luck roll and once at
the highest, and changes nothing:

```json
{"type": "preview", "game": "g1", "target": [3, 0], "attacker_hp": 100, "defender_hp": 100,
 "low_luck": {"attacker_hp": 86, "defender_hp": 51}, "high_luck": {"attacker_hp": 90, "defender_hp": 43}}
```

HP is AWBW's internal 0-100, so HP lost is the damage %: 49-57% dealt here, 10-14% taken on the counter.

### `income` on `state`

Every `state` message also carries `"income": {"<player id>": funds}`: what each player collects at
their next turn start, by the engine's rule (income properties times funds per property plus the
CO's bonus), counted on the receiving seat's own view: in fog, properties out of sight look neutral
and don't count.

### A turn without the full list

`turn` lists every legal action by default, and a mid-game turn has hundreds of them (597 for 13
units in a day-22 position, 62 KB), sent again after each action. A bot that plans one unit at a
time can ask for less:

- Join with `"legal": "summary"`. Its `turn` then carries `count` and a `summary` instead of
  `legal`: the units that can still act (`id`, `kind`, `at`, `hp`, `actions`), each Base's
  affordable builds (`at`, `units`), the powers ready, and `end_turn` (about 1 KB for that
  position).
- `options` for a unit: each tile it can end on, the path there, and what it can do there
  (wait, capture, each attack target, load, join, unload spots, ...).
- `legal` with a `filter`: `unit` (an id), `to` ([x, y], where a move ends), `then` (follow-up
  types, e.g. `["attack", "capture"]`), `kind` (`"move"`, `"build"`, `"power"`, `"end_turn"`).
  Fields combine; `{"all": true}` is everything.
- Act with `"to"` instead of a `path`.

All of these come from the seat's own view, like `legal`: they never show a hidden unit.

## Hidden information

A seat sees only its fog view. What else the server keeps back while a game runs:

- **Spectating** shows everything (fog, dived Subs, hidden Stealths, Sonja's HP), so a running game
  can be watched only with its `spectator_key`, which goes to the creator alone, and never from a
  connection seated in that game. Once the game is over anyone may watch it.
- **The luck seed** isn't sent (with it, every roll could be predicted) until the game is over, when
  it appears in the game's summary; a creator who chose the seed gets it back in `created`. Luck
  comes from ChaCha8 seeded with it, so a chosen seed replays the same game.
- **Seat tokens** are 128 random bits.
