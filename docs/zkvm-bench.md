# zkVM benchmark

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

## Measures

1. Cycles, from each zkVM's executor (no proof).
2. For `move` and `turn`: proving time, peak memory, proof size and verifying time, one zkVM at a
   time on 6 threads.
3. `game` proofs, where hardware allows.

Results go in the table below as they come in.

| zkVM | Version | License (crates used) | Builds? | Cycles: move / turn / game | Proof: turn |
|---|---|---|---|---|---|
