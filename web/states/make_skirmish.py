#!/usr/bin/env python3
"""An original two-player skirmish map, written as an AWBW-style turn file (day 1, no units), so
any bot that reads the bot contract's format (FlakBot through examples/http_bot_adapter.py) can
play it from the start. The map is ours (CC0 proposal, like web/assets).

    python3 -I web/states/make_skirmish.py [CO1] [CO2]     -> web/states/skirmish.json

Legend (top half; the bottom half is the top turned 180 degrees with owners swapped):
  .  plain   f  wood   m  mountain   ~  sea   r  road (joined automatically)
  n  city    N  base   A  airport    T  com tower      (neutral)
  H  HQ      B  base   C  city                         (player 1; player 2 on the mirror)
"""
import json, sys
from pathlib import Path

TOP = [
    "~~m.f..n..f.m~~",
    "~C.rrrrrr.n..f~",
    "mHBr..f.r..N...",
    ".C.r.m..rrrrT..",
    "f..r..nfn..r..f",   # the middle row must read the same backwards
]
P1, P2 = 101, 102
NEUTRAL = {"n": 34, "N": 35, "A": 36, "T": 133}
OWNED = {P1: {"H": 42, "B": 39, "C": 38}, P2: {"H": 47, "B": 44, "C": 43}}
COUNTRY = {P1: ("Orange Star", "os"), P2: ("Blue Moon", "bm")}
PLAIN = {".": 1, "m": 2, "f": 3, "~": 28}
ROAD = {"ew": 15, "ns": 16, "ensw": 17, "es": 18, "sw": 19, "nw": 20, "en": 21, "esw": 22, "nsw": 23, "enw": 24, "ens": 25}


def grid():
    mid = TOP[-1]
    assert mid == mid[::-1], "the middle row must be its own mirror"
    rows = [list(r) for r in TOP] + [list(r[::-1]) for r in reversed(TOP[:-1])]
    owner = [[P1 if y < len(TOP) - 1 else P2 if y >= len(TOP) else (P1 if x < len(mid) // 2 else P2)
              for x in range(len(rows[0]))] for y in range(len(rows))]
    return rows, owner


def road_id(rows, x, y):
    h, w = len(rows), len(rows[0])
    near = lambda dx, dy: 0 <= x + dx < w and 0 <= y + dy < h and rows[y + dy][x + dx] == "r"
    sides = "".join(s for s, (dx, dy) in (("e", (1, 0)), ("n", (0, -1)), ("s", (0, 1)), ("w", (-1, 0))) if near(dx, dy))
    key = "".join(sorted(sides))
    if key in ROAD:
        return ROAD[key]
    return 16 if key in ("n", "s") else 15


STAR = 90000   # AWBW's power meter per star (Andy: 3 stars -> players_co_max_power 270000)


def power_costs(co):
    c = json.load(open(Path(__file__).resolve().parents[2] / "data/cos.json")).get(co)
    if not c:
        raise SystemExit(f"unknown CO {co!r} (data/cos.json)")
    return c.get("cop", {}).get("PowerStars", 0) * STAR, c.get("scop", {}).get("PowerStars", 0) * STAR


def turn(co1="Andy", co2="Andy"):
    rows, owner = grid()
    terrain, buildings = [], []
    income = {P1: 0, P2: 0}
    for x in range(len(rows[0])):
        col, bcol = [], []
        for y in range(len(rows)):
            ch = rows[y][x]
            if ch in PLAIN:
                tid = PLAIN[ch]
            elif ch == "r":
                tid = road_id(rows, x, y)
            elif ch in NEUTRAL:
                tid = NEUTRAL[ch]
                bcol.append({"buildings_x": x, "buildings_y": y, "terrain_id": tid, "buildings_players_id": None,
                             "buildings_capture": 20})
            elif ch in "HBC":
                p = owner[y][x]
                tid = OWNED[p][ch]
                income[p] += 1000
                bcol.append({"buildings_x": x, "buildings_y": y, "terrain_id": tid, "buildings_players_id": p,
                             "buildings_capture": 20})
            else:
                raise SystemExit(f"unknown map char {ch!r}")
            col.append({"tiles_x": x, "tiles_y": y, "terrain_id": tid})
        terrain.append(col)
        buildings.append(bcol)
    players = []
    for order, (pid, co) in enumerate(((P1, co1), (P2, co2)), start=1):
        name, code = COUNTRY[pid]
        cop, scop = power_costs(co)
        players.append({"players_id": pid, "players_team": str(pid), "co_name": co, "players_order": order,
                        "players_funds": income[pid] if pid == P1 else 0, "players_income": income[pid],
                        "players_eliminated": "N", "players_co_power_on": "N", "players_co_power": 0,
                        "players_co_max_power": cop, "players_co_max_spower": scop,
                        "countries_name": name, "countries_code": code, "users_username": f"Player {order}"})
    return {
        "day": 1,
        "players": [{"id": p["players_id"], "funds": p["players_funds"], "order": p["players_order"]} for p in players],
        "gameState": {"terrain": terrain, "buildings": buildings, "units": [], "players": players,
                      "gameWeather": {"code": "C"}, "currentTurnPId": P1},
        "libaw": {"map": "skirmish", "license": "CC0 proposal (web/assets/LICENSE.md)", "rows": ["".join(r) for r in rows]},
    }


if __name__ == "__main__":
    out = Path(__file__).resolve().parent / "skirmish.json"
    out.write_text(json.dumps(turn(*sys.argv[1:3]), indent=1) + "\n")
    print(out)
