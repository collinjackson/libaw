# libaw web client

A static browser client for `libaw-server` (docs/socket-api.md): lobby, board, play, spectate.
Plain HTML/JS/CSS, no build step. Serve the repo root so the page can read `../data/*.json`.

    web/play_flakbot.sh                      # server + page + FlakBot's engine; prints your link
    web/play_flakbot.sh --fog --days 20      # options: TURN_FILE, --days N, --fog, --no-powers, --bot-seat 0|1

Or by hand:

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
- `test/headless.mjs`: plays a game against a running server using the page's own modules
  (`node web/test/headless.mjs ws://127.0.0.1:8765`).
