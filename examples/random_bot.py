#!/usr/bin/env python3
"""A random bot for libaw-server (docs/socket-api.md): joins a game and plays random legal
actions, ending its turn now and then. Needs `pip install websockets`.

    python3 random_bot.py ws://127.0.0.1:8765 GAME [PLAYER_ID]
"""
import asyncio, json, random, sys

import websockets


async def main(url, game, player=None):
    async with websockets.connect(url, max_size=None) as ws:
        async def send(**msg):
            await ws.send(json.dumps(msg))

        await send(type="hello", name="random-bot")
        join = {"type": "join", "game": game}
        if player:
            join["player"] = int(player)
        await send(**join)
        rng = random.Random()
        joined = False
        async for raw in ws:
            m = json.loads(raw)
            t = m["type"]
            if t == "joined":
                joined = True
                print(f"seat {m['seat']}, player {m['player']} (token {m['token']})")
            elif t == "turn":
                legal = m["legal"]
                # Mostly act, sometimes stop: a random bot that never ended its turn would
                # just shuffle units until it ran out of moves.
                moves = [a for a in legal if a["type"] != "end_turn"]
                action = rng.choice(moves) if moves and rng.random() < 0.85 else {"type": "end_turn"}
                await send(type="act", game=game, action=action)
            elif t == "event":
                print(f"day {m['day']}: player {m['by']}: {m['label']}")
            elif t == "error":
                print("error:", m["message"])
                if not joined:
                    return  # couldn't join: nothing to play
            elif t == "game_over":
                print("game over:", m["reason"], "winner team", m["winner"])
                return


if __name__ == "__main__":
    asyncio.run(main(*sys.argv[1:4]))
