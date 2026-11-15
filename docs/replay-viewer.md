# Replay viewer

`web/replay.html` plays one turn of a game on the board: every move, attack, capture and build
in order, with the damage the game recorded. It runs in the browser with no build step, and
`web/capture.mjs` records it as a GIF or MP4 for sharing.

The viewer plays a [storyboard](storyboard.md), which is one turn as JSON.

## 1. Make a storyboard

From a recorded AWBW turn (a turn file as the replay checker reads them: the board at the turn's
start plus the actions taken in it):

    cargo run --release -p awbw-replay -- --storyboard TURN.json > web/turn.json

Any program can write a storyboard instead: the [format](storyboard.md#format-version-1) is
small, and a bot server can save its own games this way.

## 2. Open it

Serve the repo root, so the page can read `data/*.json` beside it, then open the page with the
storyboard's address:

    python3 -m http.server 8000 --bind 127.0.0.1      # from the repo root
    open 'http://127.0.0.1:8000/web/replay.html?sb=turn.json'

The `sb` address is relative to the page (`web/`), or a full URL.

Controls: **Play** runs the turn, the slider scrubs through it, and **Follow the action** zooms
the camera onto each move. The art menu switches between libaw's own sprites and AWBW's.

URL options:

| Option | Meaning |
|---|---|
| `sb=URL` | The storyboard to play (required). |
| `art=libaw` or `art=awbw` | Which sprites to draw (default `libaw`). |
| `camera=1` | Start with "Follow the action" on. |
| `capture=1` | Hide the controls, for recording. |

## About the AWBW art

`art=awbw` draws units and terrain with Advance Wars By Web's sprites, which the browser loads
from AWBW's site when the page runs. libaw ships none of AWBW's art, only where to find it
(`web/js/art.js`). Each army takes its country's sprites from the storyboard's `countries`, and
units face the way they do on AWBW's own board. Any sprite that fails to load falls back to
libaw's.

## 3. Record a GIF or MP4

`web/capture.mjs` steps the page frame by frame, so the file shows exactly what the page draws,
however slow the machine. It needs [puppeteer](https://pptr.dev/) (`npm i -g puppeteer`) and
ffmpeg. With the server from step 2 still running:

    node web/capture.mjs 'http://127.0.0.1:8000/web/replay.html?sb=turn.json&art=awbw&camera=1' out/turn --gif --mp4

This writes `out/turn.gif` and `out/turn.mp4`. Options: `--fps N` (default 15), `--width N` in
pixels (default 720), `--vf FILTERS` for extra ffmpeg filters, and `--pages N` to record on N
browser pages at once on a machine with spare cores.

The tool records any page that sets `window.__replay = {duration, renderAt(t), ready}`. A page
built on `web/js/replay.js`, with its own overlays or a chosen set of shots, records the same
way (see [Storyboards](storyboard.md#playing-part-of-a-turn)).

## See also

- [Storyboards](storyboard.md): the format, and how a turn is cut into shots.
- [web/README.md](https://github.com/collinjackson/libaw/blob/main/web/README.md): the rest of the web client, including the move planner.
