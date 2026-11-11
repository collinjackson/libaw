# awbw-engine

A rules engine for Advance Wars By Web: `(state, action) -> state`, deterministic, no I/O. It is
built clean-room from recorded AWBW games, AWBW's own pages and the AWBW wiki, and checked
against thousands of recorded turns. Where the engine and AWBW disagree, AWBW is right and the
engine has a bug.

## Build and test

    cargo build --release
    cargo test --release

`cargo test` runs the rules cases in `crates/engine/tests/cases/`: one JSON file per rule, each
a position, one action, and what AWBW does with it.

## Reporting a rules bug

The most useful report is a rules case: a small JSON file that fails `cargo test` until the
engine is fixed. See [docs/rules-cases.md](docs/rules-cases.md) for the format and a template.
An issue is also fine (see the "Rules mismatch" template): the AWBW game and turn, what AWBW did,
and what the engine does.

Please describe behaviour (what happens, with numbers), not AWBW's source code: this engine is
meant to be open-sourced, so its code and cases have to be independent of AWBW's code.

## Playing over the socket API

`libaw-server` runs games that any client can play over a WebSocket (protocol:
[docs/socket-api.md](docs/socket-api.md)). The examples need `pip install websockets` and Node 22+.

    cargo run --release -p libaw-server -- 127.0.0.1:8765
    GAME=$(python3 examples/create_game.py ws://127.0.0.1:8765 docs/eval-harness/test-states/game-1729758-turn-042-D22-pid4151012.json 30)
    python3 examples/random_bot.py ws://127.0.0.1:8765 $GAME &        # one seat
    node examples/first_legal_bot.mjs ws://127.0.0.1:8765 $GAME        # the other

`examples/spectator.html` watches a game in a browser. `examples/http_bot_adapter.py` seats any
bot that speaks the HTTP bot contract (docs/eval-harness/CONTRACT.md).

## Checking against recorded games

The replay checker plays recorded AWBW games through the engine and reports every turn where
the engine disagrees (replays live in a separate data repo):

    ./target/release/awbw-replay DATA/replays/game-* --examples 3   # per-turn mismatches
    ./target/release/awbw-replay --step DATA/replays/game-*          # through the simulator

## Layout

- `crates/engine`: the rules (`actions.rs` legal actions, `sim.rs` applying them, `co.rs` CO
  effects, `rules.rs` movement, combat and terrain helpers, `data/` tables).
- `crates/replay`: the replay checker (`awbw-replay`).
- `crates/wasm`: the engine for the browser.
