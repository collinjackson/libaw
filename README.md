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
