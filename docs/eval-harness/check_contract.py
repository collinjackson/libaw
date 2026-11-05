#!/usr/bin/env python3
"""Contract test for the opponent bot API (see CONTRACT.md). Standard library only.

  python3 check_contract.py [http://127.0.0.1:8080]

Prints one line per test state and a final `passed N/M`. Exit status 0 when all pass.
"""
import json, sys, urllib.error, urllib.request
from pathlib import Path

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8080"
HERE = Path(__file__).resolve().parent
THEN = {"wait", "attack", "capture", "load", "join", "supply", "repair", "unload"}


def items(v):
    return list(v.values()) if isinstance(v, dict) else list(v or [])


def call(method, path, body=None):
    req = urllib.request.Request(BASE + path, method=method,
                                 data=json.dumps(body).encode() if body is not None else None,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.loads(r.read())


def xy(v):
    return isinstance(v, list) and len(v) == 2 and all(isinstance(c, int) for c in v)


def check(state, reply):
    gs = state["gameState"]
    me = int(gs["currentTurnPId"])
    tiles = [(int(c["tiles_x"]), int(c["tiles_y"])) for col in items(gs["terrain"]) for c in items(col) if c]
    w, h = max(x for x, _ in tiles) + 1, max(y for _, y in tiles) + 1
    inb = lambda p: 0 <= p[0] < w and 0 <= p[1] < h
    units = {int(u["units_id"]): u for u in items(gs["units"])}
    t = reply.get("type")
    if t == "end_turn":
        return None
    if t == "build":
        if not (xy(reply.get("at")) and isinstance(reply.get("unit"), str)):
            return "build: bad fields"
        b = [b for col in items(gs["buildings"]) for b in items(col)
             if b and (int(b["buildings_x"]), int(b["buildings_y"])) == tuple(reply["at"])]
        if not b or str(b[0].get("buildings_players_id")) != str(me):
            return "build: not my building"
        return None
    if t != "move":
        return f"unknown type {t!r}"
    u = units.get(reply.get("unit_id"))
    if not u or int(u["units_players_id"]) != me:
        return "move: not my unit"
    if int(u["units_moved"]) != 0:
        return "move: unit already moved"
    path = reply.get("path")
    if not (isinstance(path, list) and path and all(xy(p) and inb(p) for p in path)):
        return "move: bad path"
    if tuple(path[0]) != (int(u["units_x"]), int(u["units_y"])):
        return "move: path doesn't start at the unit"
    th = reply.get("then") or {}
    if th.get("type") not in THEN:
        return f"move: bad then {th.get('type')!r}"
    for k in ("target", "to"):
        if k in th and not (xy(th[k]) and inb(th[k])):
            return f"move: bad {k}"
    return None


def main():
    h = call("GET", "/health")
    print("health:", {k: h.get(k) for k in ("ok", "commit", "bot_label", "bots")})
    bots = h.get("bots") or [h.get("bot_label")]
    files = sorted((HERE / "test-states").glob("*.json"))
    ok = total = 0
    for bot in bots:
        for f in files:
            state = json.loads(f.read_text())
            me = int(state["gameState"]["currentTurnPId"])
            try:
                reply = call("POST", "/act", {"player_id": me, "seed": 1, "bot": bot, "state": state})
                err = check(state, reply)
            except Exception as e:  # network error, timeout, bad JSON
                err = f"request failed: {type(e).__name__}"
            print(bot, f.name, "ok" if err is None else f"FAIL: {err}")
            ok += err is None
            total += 1
    # An unknown bot label must be refused.
    try:
        call("POST", "/act", {"player_id": 0, "seed": 1, "bot": "bot-zz", "state": {}})
        print("unknown bot: FAIL (accepted)")
    except urllib.error.HTTPError as e:
        print("unknown bot:", "ok" if e.code == 400 else f"FAIL (HTTP {e.code})")
        ok += e.code == 400
    except Exception as e:
        print(f"unknown bot: FAIL ({type(e).__name__})")
    total += 1
    print(f"passed {ok}/{total}")
    sys.exit(0 if ok == total else 1)


if __name__ == "__main__":
    main()
