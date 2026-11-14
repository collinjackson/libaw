# AGENTS.md: writing a bot on libaw

This page is for coding agents (and people) building a bot that plays on libaw. Contributing to the
engine itself is covered in [CONTRIBUTING.md](CONTRIBUTING.md).

## Don't reimplement the rules: ask the engine

AWBW's rules have many special cases (CO abilities, rounding, fog, traps, repairs), and a bot
that copies them will get some wrong. libaw already answers the questions a bot needs:

| Question | Over the socket server ([docs/socket-api.md](docs/socket-api.md)) | In Rust (the `libaw` crate) |
|---|---|---|
| What can I do this turn? | join with `"legal": "summary"`: units that can act, builds, powers | `actions::legal_actions(&view)` |
| Where can this unit go, and what can it do there? | `options` {`unit`} | `actions::legal_actions`, filtered by unit |
| Only some actions | `legal` with a `filter` (`unit`, `to`, `then`, `kind`) | |
| What would this attack do? | `preview` (lowest and highest luck) | `rules::damage_range` |
| What happens if I do it? | `act` (the server is the referee) | `sim::step` on a copy |
| What can I see? | every `state` you receive is your own fog view | `fog::player_view(&state, me)` |

A move can name just its destination (`"to": [x, y]`); the engine picks the cheapest path. Send a
full `path` when the route matters, for example around a tile where a hidden unit might be.

The HTTP bot contract ([docs/eval-harness/CONTRACT.md](docs/eval-harness/CONTRACT.md)) is the
other way in: the referee POSTs the position and your bot answers with one action.
`examples/unit_by_unit_bot.mjs` is a complete small bot on the socket API that never looks at the
full legal list.

## What your bot can know

Only its own view. In fog that leaves out enemy units out of sight, enemy funds, and who owns
properties out of sight. With or without fog it leaves out dived Subs and hidden Stealths unless one
of your units is next to them, and an enemy Sonja's units show their HP only as a range (1 to 10). The server keeps the rest
back too:
- the luck seed (until the game is over);
- spectating a running game, which needs the creator's key;
- the count of enemy actions you didn't see (your `seq` counts changes to your own view).

Moving into a unit you couldn't see is a trap: your unit stops on the tile before it, as on AWBW.

## Where the rules are written down

- [docs/rules/tables.md](docs/rules/tables.md): unit stats, the damage chart, terrain costs and
  defence, and CO power costs. It's generated from the engine's data, so it matches what the engine
  plays.
- [docs/awbw-rule-findings.md](docs/awbw-rule-findings.md): the detailed rules recorded games
  can't settle on their own (rounding, counters, repairs, missile aim, unit and capture limits),
  each with its evidence, plus what is still open.
- The engine's behaviour tests describe one rule each, in plain names:
  - `crates/engine/tests/parity_combat.rs`;
  - `crates/engine/tests/parity_turns.rs`;
  - `crates/engine/tests/parity_details.rs`;
  - `crates/engine/tests/cases/`.

  For example, `a_blocked_drop_in_place_does_not_cost_the_transport_its_move`.
- The code, by area:

  | Area | File |
  |---|---|
  | Legal moves, paths, traps | `crates/engine/src/actions.rs` |
  | Applying actions, the turn start (income, repairs, fuel, crashes), winning | `crates/engine/src/sim.rs` |
  | The damage formula and movement costs by weather | `crates/engine/src/rules.rs` |
  | Attack and defence values | `crates/engine/src/combat.rs` |
  | CO abilities and powers | `crates/engine/src/co.rs` |
  | Vision and the player view | `crates/engine/src/fog.rs` |

libaw matches AWBW on 99.96% of about 13 million checks over 1,092 recorded games. If you find a
difference, it's a bug: see "Fixing a rules bug" in CONTRIBUTING.md.

## Running things

```
cargo run --release -p libaw-server -- 127.0.0.1:8765
GAME=$(python3 examples/create_game.py ws://127.0.0.1:8765 docs/eval-harness/test-states/game-1729758-turn-042-D22-pid4151012.json 30)
node examples/unit_by_unit_bot.mjs ws://127.0.0.1:8765 $GAME &   # one seat
node examples/unit_by_unit_bot.mjs ws://127.0.0.1:8765 $GAME     # the other
```

[README.md](README.md) has the rest of the quick start: other example bots, the spectator page and
playing in the browser.
