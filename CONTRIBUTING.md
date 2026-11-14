# Contributing to libaw

libaw is a rules engine compatible with Advance Wars By Web (AWBW). Its one job is to do exactly
what AWBW does, so **the most valuable contribution is a rules bug**: a place where the engine and
AWBW disagree, reported or fixed. You don't need to know Rust to find one, only AWBW.

Thank you for helping. This guide covers how.

## Code of conduct

libaw follows the [Rust Code of Conduct][rust-coc]. It is the minimum we expect from everyone
in issues, pull requests and discussions. To report a problem, contact a maintainer privately.

## The one rule: behaviour, never AWBW's code

libaw is written clean-room. Every rule comes from what AWBW visibly *does*: recorded games,
AWBW's own pages, the damage calculator, the wiki, a test game you played. Please never paste,
quote or paraphrase AWBW's source code in an issue, a case or a pull request, and never use it to
write a fix. A contribution built on it can't be accepted. Describe what happens, with numbers.

## Ways to contribute

1. **Report a rules bug.** Open a "Rules mismatch" issue: the AWBW game and turn (or how you saw
   it), what AWBW did, and what the engine does if you know. Numbers help most: HP, funds, tiles.
2. **Write a rules case.** A rules case is a small JSON file in `crates/engine/tests/cases/`: a
   position, one action, and what AWBW does with it. It fails `cargo test` until the engine is
   fixed, so it is both the report and the test. Format and a template:
   [docs/rules-cases.md](docs/rules-cases.md). The quickest way to make one is from a replay turn
   (`awbw_turn_file`).
3. **Fix a rules bug.** Make a failing case pass without breaking any other (see below).
4. **Add evidence to an open issue.** Another game where the same thing happens, or one where it
   doesn't, often settles a rule.

Larger changes (a new client, a change to the socket API, a refactor) are welcome too; please open
an issue first so we can agree on the shape before you spend the time.

## Fixing a rules bug

1. Start from a failing rules case, yours or one from an issue.
2. Make the change. The rules live in `crates/engine/src`: `actions.rs` (legal actions), `sim.rs`
   (applying them), `combat.rs` and `rules.rs` (damage, movement, terrain), `co.rs` (CO effects),
   `fog.rs` (vision).
3. Check it:

       cargo test --release                  # every rules case and unit test

   Match the style of the code around your change.

   If you have recorded games, run the replay checker before and after your change: a rules fix
   should fix games without breaking others.

       ./target/release/awbw-replay DATA/replays/game-* --examples 3

   Recordings come from AWBW's replay API (`tools/fetch_replays.sh` fetches them, politely, at a
   few seconds a request) and are not kept in this repository. Without them, say so in the pull
   request and a maintainer will run the check.
4. Open a pull request that says which rule it changes and the evidence: the AWBW game and turn,
   the damage calculator, or a test game.

Keep a pull request to one rule where you can; small ones are reviewed fastest. Documentation
fixes are welcome in their own pull request.

## Review

Expect questions: most are about evidence, since a rule that matches one game can break another.
That is a normal part of the process, not a judgement of the work. Please keep reviews kind in
both directions.

If a pull request stalls, ask its author before taking it over, and credit them when you do
(`Co-authored-by:` in the commit message).

## Bots and the leaderboard

libaw's own matches rate bots against each other on AWBW's rating scale (the War College
leaderboard). To enter a bot, register it: (form link to come). Open-source bots, bots whose code
you'd share privately and bots behind an API are all welcome; the form asks which. A bot plays
through the HTTP bot contract ([docs/eval-harness/CONTRACT.md](docs/eval-harness/CONTRACT.md)) or
the WebSocket API ([docs/socket-api.md](docs/socket-api.md)).

## License

By contributing, you agree that your contributions are dual-licensed under the MIT license and
the Apache License, Version 2.0, as described in the [README](README.md#license), without any
additional terms or conditions.

[rust-coc]: https://www.rust-lang.org/policies/code-of-conduct
