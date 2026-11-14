# libaw web client

A static browser client for `libaw-server` (docs/socket-api.md): lobby, board, play, spectate.
Plain HTML/JS/CSS, no build step. Serve the repo root so the page can read `../data/*.json`.

    cargo run --release -p libaw-server -- 127.0.0.1:8765
    python3 -m http.server 8000 --bind 127.0.0.1      # from the repo root
    open http://127.0.0.1:8000/web/                    # Connect, Create, Play / Watch

Playing: click a unit to see where it can go (blue) and what it can hit (red). Click a blue tile for
its action menu, or a red one to fire. Click your base, airport or port to build. Esc cancels, E
ends the turn. The URL keeps `?server=&game=&seat=`, and the seat's token is kept in
localStorage, so a reload takes the seat back.

- `assets/`: original placeholder art drawn by `assets/make_sprites.py` (CC0 proposal,
  `assets/LICENSE.md`). Team colours come from CSS variables.
- `states/skirmish.json`: an original two-player map at day 1 (`states/make_skirmish.py [CO1] [CO2]`).
- `parity.html` + `parity.json`: UI feature parity with AWBW's game page.
- `replay.html` + `js/replay.js`, `js/story.js`, `js/art.js`: play a turn's storyboard in libaw art or
  AWBW's (loaded from AWBW's site); `capture.mjs` records it as GIF/MP4. See docs/storyboard.md.
- `test/headless.mjs`: plays a game against a running server using the page's own modules
  (`node web/test/headless.mjs ws://127.0.0.1:8765`).

## Move planner (`planner.html`)

The same board with the engine running in the page as WebAssembly (`js/local.js` stands in for
the server): load a position (a bundled state or an AWBW turn file), then play the turn out by the
rules: moves, attacks (damage range shown first, then rolled), counters, captures, builds, powers.
Undo (U) and Reset go back; ending the turn hands the board to the next player, so one person
can try a turn and the reply. Luck: random, average, best or worst for the attacker. No AI hints.

    wasm-pack build crates/wasm --release --target web --out-dir ../../web/pkg
    python3 -m http.server 8000 --bind 127.0.0.1      # from the repo root
    open http://127.0.0.1:8000/web/planner.html
    node web/test/planner.mjs                          # engine checks: luck modes vs preview, undo, reset
