# libaw

A rules engine compatible with [Advance Wars By Web](https://awbw.amarriner.com) (AWBW):
`(state, action) -> state`, deterministic, no I/O. It is built clean-room from recorded AWBW games,
AWBW's own pages and the AWBW wiki, and checked against every recorded turn we hold. Where the
engine and AWBW disagree, AWBW is right and the engine has a bug. libaw is an independent project,
not affiliated with AWBW.

**How close it is:** across 1,092 recorded games, about 13 million rule checks (moves, legal
actions, damage, counters, captures, funds, CO powers), 99.96% match AWBW. Some of what's left is
information the recordings don't carry, such as Sonja's hidden HP. What the rules review fixed,
and what is still open: [docs/awbw-rule-findings.md](docs/awbw-rule-findings.md).

## What's here

| Part | What it does |
| --- | --- |
| `crates/engine` | `libaw`, the rules. `actions.rs` lists legal actions, `sim.rs` applies them, `co.rs` holds CO effects, `combat.rs` and `rules.rs` damage, movement and terrain, `fog.rs` what each player sees, `storyboard.rs` a turn's record as steps to replay. |
| `crates/replay` | `libaw-replay` (binary `awbw-replay`): checks recorded games against the engine, plays local matches between bots, and turns a recorded turn into a storyboard. |
| `crates/server` | `libaw-server`: runs games any client can play over a WebSocket. |
| `crates/wasm` | `libaw-wasm`: the engine in the browser as WebAssembly. |
| `web/` | A browser client for the server, a move planner and a turn replay viewer, with libaw's own art (or AWBW's, loaded from AWBW's site at view time). |
| `data/` | Unit, terrain, damage and CO tables; where each comes from is in [docs/sources.md](docs/sources.md). |

## Build and test

    cargo build --release
    cargo test --release

`cargo test` includes the rules cases in `crates/engine/tests/cases/`: one JSON file per rule,
each a position, one action, and what AWBW does with it.

## Checking against recorded games

The replay checker plays recorded AWBW games through the engine and reports every place it
disagrees. Recordings come from AWBW's replay API and live outside this repo
(`tools/fetch_replays.sh` fetches them, politely).

    ./target/release/awbw-replay DATA/replays/game-* --examples 3   # per-check mismatches
    ./target/release/awbw-replay --step DATA/replays/game-*          # through the simulator, action by action

Run both before and after a rules change: a change should fix games without breaking others.

## Reporting or fixing a rules bug

Rules bugs are the most useful contribution. The full guide, including the clean-room rule and how
a fix is checked, is in [CONTRIBUTING.md](CONTRIBUTING.md); in short:

- **A rules case** is the most useful report: a small JSON file that fails `cargo test` until the
  engine is fixed. Format and template: [docs/rules-cases.md](docs/rules-cases.md).
- **An issue** is fine too (the "Rules mismatch" template): the AWBW game and turn, what AWBW
  did, and what the engine does.
- **A pull request**: say which rule it changes and the evidence (a recorded game and turn, the
  damage calculator, a test game), and run the replay checker before and after.

Describe behaviour (what happens, with numbers), never AWBW's source code: libaw is open source,
so its code and cases must stay independent of AWBW's.

## Playing over the socket API

`libaw-server` runs games any client can play over a WebSocket (protocol:
[docs/socket-api.md](docs/socket-api.md)). The examples need `pip install websockets` and Node 22+.

    cargo run --release -p libaw-server -- 127.0.0.1:8765
    GAME=$(python3 examples/create_game.py ws://127.0.0.1:8765 docs/eval-harness/test-states/game-1729758-turn-042-D22-pid4151012.json 30)
    python3 examples/random_bot.py ws://127.0.0.1:8765 $GAME &        # one seat
    node examples/first_legal_bot.mjs ws://127.0.0.1:8765 $GAME        # the other

`examples/spectator.html` watches a game in a browser. `examples/http_bot_adapter.py` seats any
bot that speaks the HTTP bot contract ([docs/eval-harness/CONTRACT.md](docs/eval-harness/CONTRACT.md)).
To play in the browser yourself, see [web/README.md](web/README.md).

## Replaying a turn

    ./target/release/awbw-replay --storyboard TURN.json > turn.json

turns one recorded turn into a storyboard: the board at the start, then each step (move, fire,
capture, build, power). `web/replay.html?sb=turn.json` plays it, and `web/capture.mjs` records it
as a GIF or MP4. Format: [docs/storyboard.md](docs/storyboard.md).

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as
above, without any additional terms or conditions.

libaw's art (`web/assets`) is original and dedicated to the public domain under CC0 1.0
([web/assets/LICENSE.md](web/assets/LICENSE.md)). AWBW's art is never stored
here: the web client can load it from AWBW's site at view time, and it remains its owners'.
Advance Wars is a trademark of Nintendo; libaw is not affiliated with Nintendo or AWBW.
