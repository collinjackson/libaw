#!/usr/bin/env python3
"""Create a game on libaw-server from an AWBW replay turn file and print its id.

    python3 create_game.py ws://127.0.0.1:8765 TURN.json [MAX_DAYS] [SEED]
"""
import asyncio, json, sys

import websockets


async def main(url, path, max_days="0", seed=None):
    async with websockets.connect(url, max_size=None) as ws:
        msg = {"type": "create", "turn": json.load(open(path)), "max_days": int(max_days)}
        if seed:
            msg["seed"] = int(seed)
        await ws.send(json.dumps(msg))
        reply = json.loads(await ws.recv())
        if reply["type"] != "created":
            sys.exit(f"error: {reply.get('message')}")
        for p in reply["players"]:
            print(f"  seat {p['seat']}: player {p['id']}, {p['co']}", file=sys.stderr)
        print(reply["game"])


if __name__ == "__main__":
    asyncio.run(main(*sys.argv[1:5]))
