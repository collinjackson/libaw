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
| `tip=0` | No tile tip: hovering or tapping a tile doesn't show its unit and terrain. |
| `ghost=1` | Act out each step first, as the player's cursor and menus would (below). |
| `damage=1` | With the ghost, show the damage box (damage and counter ranges) while the cursor is on an attack's target. Off by default: it's only up for a moment. |
| `armies=bm,os` | With AWBW's art, draw each seat (in turn order) as that army instead of its own, like AWBW's army override for players who can't tell two armies apart. Leave one blank to keep it. |
| `ghostpace=K` | The ghost's segments K times as long (`0.5` twice as fast, `0` no ghost). |

### The ghost (`ghost=1`)

After [AWBW Ghost Input](sources.md) (onetrickwolf, MIT), whose behaviour and timings it follows
in libaw's own drawing: one cursor for the whole turn glides to each unit (Ghost Input's mouse
timing, 0.12 to 0.7 s by distance), shows its move range and traces the path arrow, and after the
move opens the menu the player saw at the destination (Fire, Unload, Repair, Supply, Hide or
Unhide, Capt, Launch, Explode, Delete, Wait, in AWBW's order; a Load or Join alone), hovers the
choice and presses it. Then, by what the unit did:

- **Fire**: the targets in red, the cursor glides to the one hit, and a damage box shows the
  damage the shot could do (lowest to highest luck) and the counterattack's.
- **Unload**: AWBW drops cargo only from where a transport stands, so a transport that moved
  waits first and is clicked again (a menu of Unload alone), then the cargo list, then the
  landing tiles with the cursor going to the drop. The cargo rolls out of the transport.
- **Build**: the cursor on the base, then its build menu (every unit it makes at the player's
  prices, cheapest first, the ones they couldn't afford greyed) with the bought unit pressed.
- **Black Bomb**: the blast area (3 tiles), its own tile pressed; the blast hits every unit in it.
- **Missile silo**: the cursor goes to the target with the 2-tile blast area following it,
  presses it, and a one-option menu (Fire Silo) confirms.
- **Black Boat**: green squares on the units beside it, the repaired one pressed.
- **Supply, Hide, Unhide, Delete, Capt, Join, Load**: just the menu's choice.

Ranges, menus, damage boxes, build menus and landing tiles come from libaw's engine in the page
(`web/pkg`, built with `wasm-pack build crates/wasm --release --target web --out-dir ../../web/pkg`).
Without it the ghost plays the same way, showing less: no ranges or damage boxes, menus with the
choice, Wait and what the unit alone tells. The cursor hides 4 seconds after the turn's last step
is drawn, and comes back with the next frame before the end.

The ghost makes a turn about twice as long as without it. A page that falls behind (a newer turn
waiting) can speed it up while it plays, the way Ghost Input does when its queue backs up (full
speed under 1 s behind, 0.6 under 2 s, 0.35 under 3.5 s, then none):

    const r = frame.contentWindow.__replay;
    pos = r.ghostPace(0.35, pos);   // returns where pos is in the new timeline, drawn there
    // ...and r.ghostPace(1, pos) once caught up

`ghostPace(pace, t)` lays the timeline out again with the ghost's segments `pace` times as long
(`0`: none) and returns the time in it that shows what `t` showed (the same moment of the same
segment, or the start of the next one when the ghost's segment is gone). The pace stays for later
storyboards given to `update`. In code that builds a `Replay` itself, pass `ghostPace` to the
constructor or call `replay.setGhostPace(pace, t)`, which returns the new time the same way;
`story.remapTime(oldTimeline, newTimeline, t)` does the mapping.

## Following a game: new boards in place

A page that embeds the viewer (in an iframe) and shows game after game, or a game as it's played,
doesn't need to reload it for each turn. Give the viewer the next storyboard:

    const r = frame.contentWindow.__replay;          // once r.ready
    await r.update(nextStoryboard, 0, { fade: 250 }); // then play from r.renderAt(0) on

`update(storyboard, t, { fade })` swaps in any storyboard (a longer copy of the same turn as
moves come in, the next turn, or another game), redraws it at time `t` (default: where it was),
and keeps the page, its listeners and the loaded art. With `fade` (ms), the old frame fades out
over the new one, so units that acted ease back to full colour at a turn change instead of
jumping. Other members of `__replay`: `duration`, `renderAt(t)`, `segments` (each shot's `t0`,
`t1`, `step`, `phase`), `steps`, `start`, `stateBefore(i)` (the board before step `i`),
`ghostPace(pace, t)` (above). A segment whose `phase` is the step's own type is the step itself;
the ghost's segments (`ghost: true`: phases `hop`, `aim`, `select`, `menu`, `target`, `drop`,
`repair`, `blast`, `silo`) come before it, and a move's `menu` right after it.

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
