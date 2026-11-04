# awbw-engine (private)

A clean-room Advance Wars By Web rules engine, and later a strong bot on top of it. Private,
the "secret sauce": no remote, no pushing, nothing from here goes into public repos.

## Clean room: read this first

- **Never read, clone, search, or fetch the source or docs of `carsonhu/awbw-ai`**, beyond
  its top-level README, which has already been read. Never paste its code here. This applies
  to every session and every subagent.
- Ground truth for the rules is **recorded AWBW games** (the replay API) plus AWBW's own pages
  and the AWBW wiki. When the engine disagrees with a replay, the replay wins.
- Other open-source projects may be read only after checking that their license allows it, and
  the license has to be noted in `docs/sources.md` first. GPL/AGPL code may inform
  understanding but is never copied.
- Their bot will be run later, as a black box, from a separate private repo (`awbw-eval`) to
  score strength only. It's never a rules oracle and never training data.

## Budget

- Claude spend is capped at **$100/day** (API list price). Check it with
  `python3 -I tools/spend.py` before starting each chunk of work. At `EXHAUSTED`, stop and
  wait for the next day.
- Keep sessions narrow: one rule area per chunk. Don't re-read large files you've already
  read. Use the cheapest tool that works: `jq`/Python over the replay JSON beats reading it.

## Layout

- `crates/engine`: rules, `(state, action) -> state`, deterministic, no I/O.
- `crates/replay`: loads recorded games and reports where the engine diverges from them.
- `tools/`: Python helpers (spend, fetching replays, generating data tables).
- `data/`: game data tables, checked in. Replays live outside the repo (see `NOTES.md`).
- `NOTES.md`: work log, the current mismatch scoreboard, and what's next. Update it at the end
  of every chunk.

## Commits

Commit locally at the end of each chunk, with a clear message. Never add a remote or push.
