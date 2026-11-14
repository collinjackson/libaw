# libaw

libaw is a rules engine compatible with [Advance Wars By Web](https://awbw.amarriner.com) (AWBW),
for bots: `(state, action) -> state`, deterministic, no I/O. It is built clean-room from recorded
AWBW games, AWBW's own pages and the AWBW wiki. Across 1,092 recorded games, about 13 million rule
checks match AWBW 99.96% of the time. Where the engine and AWBW disagree, AWBW is right and the
engine has a bug. libaw is an independent project, not affiliated with AWBW.

These pages cover two things:

- **Writing a bot:** start with the [bot guide](bots.md), then the [socket API](socket-api.md) or
  the [HTTP bot contract](eval-harness/CONTRACT.md).
- **The rules:** the [tables](rules/tables.md) of unit, damage, terrain and CO power values, and
  the [rule findings](awbw-rule-findings.md) for the details recorded games can't settle alone.

Use the search (the magnifier, or press `S`) to find a unit, CO or rule. The source of every page
is in the [libaw repository](https://github.com/collinjackson/libaw) under `docs/`, and the site
rebuilds on every change.
