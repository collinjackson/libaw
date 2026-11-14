# zkVM benchmark

**The goal:** a bot proves that each move came from its own committed program, run on its own
view of the game, with no outside help (no human, no other engine, no hidden network call). The
rules don't need a proof: libaw is reproducible, and a trusted luck source plus the game's action
log let anyone replay a game and check it.

So the program proved is the **bot's decision**: given the seat's fog view (whose hash the server
publishes for every turn) and the bot's committed seed, output an action. The proof shows that a
program with a known image id, committed before the event, maps that view to that action. The
bot's code can stay private: only its image id is public.

What we measure is how much bot fits in a proof: the cost of one decision as the bot gets
stronger (more search), from a one-step greedy bot up.

## The referee program (a building block)

Which zkVM can prove a libaw game, and at what cost? Every zkVM runs the same referee program,
`crates/proof` (`libaw_proof::run`): it reads a position, a list of actions and a luck seed, checks
that each action is legal, applies it, and outputs the winner, the day and an FNV-1a hash of the
final position. It uses no zkVM's built-in hash or other accelerators, so the comparison is fair.

## Workloads

`cargo run --release -p libaw-proof --bin proof-workloads` writes them to `bench/zkvm/workloads/`.
They start from the day-22 test position and come from a fixed-seed random player.

| Workload | Actions | Input | Native time (M-series Mac, one core) |
|---|---|---|---|
| `move` | 1 | 9.7 KB | 0.9 ms (most of it loading the rules tables) |
| `turn` | 17 | 11.2 KB | 5.6 ms |
| `game` | 894 (15 days) | 90.9 KB | 541 ms |

A zkVM's result counts only if its output matches the native one exactly (`state_hash` included).

These workloads measure the engine's raw cost in each zkVM, since every bot that searches calls
it many times. The main benchmark adds bot workloads (to come): one decision by a greedy bot, and
by a search bot at rising budgets.

## Measures

1. Cycles, from each zkVM's executor (no proof), for the engine workloads and the bot decisions.
2. For one bot decision: proving time, peak memory, proof size and verifying time, one zkVM at a
   time on 6 threads.
3. How strong a bot can be at a given proving cost per decision (search budget vs. win rate).

Results go in the table below as they come in.

| zkVM | Version | License (crates used) | Builds? | Cycles: move / turn / game | Proof: one bot decision |
|---|---|---|---|---|---|
