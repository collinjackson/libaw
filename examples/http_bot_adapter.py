#!/usr/bin/env python3
"""Seat a bot that speaks the HTTP bot contract (docs/eval-harness/CONTRACT.md: POST /act with
an AWBW turn file, one action back) in a libaw-server game. Any contract bot plugs in this way,
e.g. a bot server started with `awbw-bot serve 18400`. Needs `pip install websockets`; the game
must be created from an AWBW turn file (the contract's state is one).

    python3 http_bot_adapter.py ws://127.0.0.1:8765 GAME http://127.0.0.1:18400 [PLAYER_ID] [BOT_LABEL]
"""
import asyncio, json, sys, urllib.request

import websockets


def ask(bot_url, body):
    req = urllib.request.Request(bot_url.rstrip("/") + "/act", data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)


async def main(url, game, bot_url, player=None, label=None):
    async with websockets.connect(url, max_size=None) as ws:
        async def send(**msg):
            await ws.send(json.dumps(msg))

        await send(type="hello", name="contract-adapter")
        join = {"type": "join", "game": game, "format": "awbw"}
        if player:
            join["player"] = int(player)
        await send(**join)
        info, state, me = {}, None, None
        async for raw in ws:
            m = json.loads(raw)
            t = m["type"]
            if t == "joined":
                info, me = m["info"], m["player"]
                print(f"seat {m['seat']}, player {me}: {bot_url}")
            elif t == "state":
                state = m["state"]          # the AWBW turn file the contract bot reads
            elif t == "turn" and state is not None:
                body = {"player_id": me, "seed": m["seq"], "game": game, "powers": info.get("powers", False),
                        "banned": info.get("banned", []), "state": state}
                if label:
                    body["bot"] = label
                try:
                    action = await asyncio.to_thread(ask, bot_url, body)
                    action.pop("why", None)
                except Exception as e:      # a bot that fails ends its turn, as in local matches
                    print("bot error:", e)
                    action = {"type": "end_turn"}
                await send(type="act", game=game, action=action)
            elif t == "error":
                print("error:", m["message"])
                if me is None:
                    return
                if m.get("re") == "act":    # an illegal reply: end the turn rather than loop
                    await send(type="act", game=game, action={"type": "end_turn"})
            elif t == "game_over":
                print("game over:", m["reason"], "winner team", m["winner"])
                return


if __name__ == "__main__":
    asyncio.run(main(*sys.argv[1:6]))
